//! Structural layer of TES4-family plugins: records, groups and subrecords.
//!
//! This layer knows nothing about what records mean. It parses the container
//! structure losslessly and can write it back byte-for-byte; see
//! `docs/spec-notes/esm.md` for the format description and confidence tags.

use bytes::Bytes;
use fnv_util::{tag_str, Cursor};
use std::borrow::Cow;

pub const RECORD_HEADER_SIZE: usize = 24;
pub const GROUP_HEADER_SIZE: usize = 24;
pub const GRUP: [u8; 4] = *b"GRUP";
pub const XXXX: [u8; 4] = *b"XXXX";
/// Largest decompressed size a compressed record may declare. The shipped masters'
/// largest is 42,112 bytes (a FalloutNV.esm LAND) and their largest uncompressed record
/// 621,700 bytes (NAVI) [observed]; 64 MiB leaves mods a hundredfold room while a
/// record of a few MB inflating to gigabytes (a zip bomb) is refused before it is built.
pub const MAX_DECOMPRESSED_RECORD: usize = 64 << 20;

/// Record flag bits whose meaning is relied upon by the structural layer.
pub mod record_flags {
    /// Plugin is a master file (TES4 record only).
    pub const MASTER: u32 = 0x0000_0001;
    pub const DELETED: u32 = 0x0000_0020;
    /// Record payload is a u32 decompressed size followed by a zlib stream.
    pub const COMPRESSED: u32 = 0x0004_0000;
}

#[derive(Debug, thiserror::Error)]
pub enum EsmError {
    /// The message embeds the io::Error, so it is not declared as #[source].
    #[error("I/O error: {0}")]
    Io(std::io::Error),
    #[error("structure error at 0x{offset:x}: {msg}")]
    Structure { offset: u64, msg: String },
    #[error("cannot decompress record {form_id:08X} at 0x{offset:x}: {msg}")]
    Decompress { offset: u64, form_id: u32, msg: String },
}

impl From<std::io::Error> for EsmError {
    fn from(e: std::io::Error) -> Self {
        EsmError::Io(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordHeader {
    pub sig: [u8; 4],
    /// On-disk payload size (compressed size when compressed).
    pub data_size: u32,
    pub flags: u32,
    pub form_id: u32,
    /// Version-control info (creation-kit bookkeeping), preserved verbatim.
    pub vc_info: u32,
    /// Per-record format version (15 for most FalloutNV.esm records) [observed].
    pub form_version: u16,
    pub vc_info2: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupHeader {
    /// Total group size including this 24-byte header.
    pub size: u32,
    pub label: [u8; 4],
    pub group_type: i32,
    pub stamp: u16,
    pub unknown1: u16,
    pub version: u16,
    pub unknown2: u16,
}

impl GroupHeader {
    pub fn label_u32(&self) -> u32 {
        u32::from_le_bytes(self.label)
    }
    pub fn label_i32(&self) -> i32 {
        i32::from_le_bytes(self.label)
    }
    /// Exterior cell block/sub-block labels are two i16 grid coordinates (y, x).
    pub fn label_grid(&self) -> (i16, i16) {
        let y = i16::from_le_bytes([self.label[0], self.label[1]]);
        let x = i16::from_le_bytes([self.label[2], self.label[3]]);
        (x, y)
    }
}

/// Group types. Names follow community documentation; type 10 is documented for
/// this engine generation but does not occur in the shipped FNV masters [observed].
pub mod group_type {
    pub const TOP: i32 = 0;
    pub const WORLD_CHILDREN: i32 = 1;
    pub const INTERIOR_CELL_BLOCK: i32 = 2;
    pub const INTERIOR_CELL_SUBBLOCK: i32 = 3;
    pub const EXTERIOR_CELL_BLOCK: i32 = 4;
    pub const EXTERIOR_CELL_SUBBLOCK: i32 = 5;
    pub const CELL_CHILDREN: i32 = 6;
    pub const TOPIC_CHILDREN: i32 = 7;
    pub const CELL_PERSISTENT_CHILDREN: i32 = 8;
    pub const CELL_TEMPORARY_CHILDREN: i32 = 9;
    pub const CELL_VISIBLE_DISTANT_CHILDREN: i32 = 10;

    pub fn name(t: i32) -> &'static str {
        match t {
            TOP => "top",
            WORLD_CHILDREN => "world_children",
            INTERIOR_CELL_BLOCK => "interior_cell_block",
            INTERIOR_CELL_SUBBLOCK => "interior_cell_subblock",
            EXTERIOR_CELL_BLOCK => "exterior_cell_block",
            EXTERIOR_CELL_SUBBLOCK => "exterior_cell_subblock",
            CELL_CHILDREN => "cell_children",
            TOPIC_CHILDREN => "topic_children",
            CELL_PERSISTENT_CHILDREN => "cell_persistent_children",
            CELL_TEMPORARY_CHILDREN => "cell_temporary_children",
            CELL_VISIBLE_DISTANT_CHILDREN => "cell_visible_distant_children",
            _ => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Record {
    pub header: RecordHeader,
    /// Payload exactly as stored on disk (still compressed if flagged). A zero-copy
    /// slice of the plugin's buffer when parsed from a file.
    pub raw: Bytes,
    /// File offset of the record header (diagnostics only).
    pub offset: u64,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub header: GroupHeader,
    pub children: Vec<Node>,
    pub offset: u64,
}

#[derive(Debug, Clone)]
pub enum Node {
    Record(Record),
    Group(Group),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subrecord {
    pub sig: [u8; 4],
    pub data: Vec<u8>,
    /// True when the size came from a preceding XXXX subrecord (data > 65535 bytes).
    pub extended: bool,
}

impl Subrecord {
    pub fn sig_str(&self) -> String {
        tag_str(&self.sig)
    }
}

impl Record {
    pub fn sig_str(&self) -> String {
        tag_str(&self.header.sig)
    }

    pub fn is_compressed(&self) -> bool {
        self.header.flags & record_flags::COMPRESSED != 0
    }

    /// Uncompressed subrecord bytes. Tolerates zlib trailer anomalies (see
    /// [`Record::payload_checked`] to observe them).
    /// Callers that need the trailer status use [`Record::payload_checked`]; here an
    /// accepted anomaly is only logged.
    pub fn payload(&self) -> Result<Cow<'_, [u8]>, EsmError> {
        let (p, trailer) = self.payload_checked()?;
        if trailer != fnv_inflate::Trailer::Verified {
            tracing::debug!(
                record = %self.sig_str(),
                form_id = format_args!("{:08X}", self.header.form_id),
                offset = self.offset,
                ?trailer,
                "accepted zlib trailer anomaly"
            );
        }
        Ok(p)
    }

    /// Uncompressed payload plus the zlib trailer status (`Verified` for
    /// uncompressed records).
    pub fn payload_checked(&self) -> Result<(Cow<'_, [u8]>, fnv_inflate::Trailer), EsmError> {
        if !self.is_compressed() {
            return Ok((Cow::Borrowed(&self.raw), fnv_inflate::Trailer::Verified));
        }
        if self.raw.len() < 4 {
            return Err(EsmError::Decompress {
                offset: self.offset,
                form_id: self.header.form_id,
                msg: "compressed payload shorter than 4 bytes".into(),
            });
        }
        let size = u32::from_le_bytes([self.raw[0], self.raw[1], self.raw[2], self.raw[3]]) as usize;
        if size > MAX_DECOMPRESSED_RECORD {
            return Err(EsmError::Decompress {
                offset: self.offset,
                form_id: self.header.form_id,
                msg: format!("declared decompressed size {size} is over the {MAX_DECOMPRESSED_RECORD}-byte limit"),
            });
        }
        fnv_inflate::zlib_decompress_ex(&self.raw[4..], Some(size))
            .map(|(d, t)| (Cow::Owned(d), t))
            .map_err(|e| EsmError::Decompress { offset: self.offset, form_id: self.header.form_id, msg: e.to_string() })
    }

    pub fn subrecords(&self) -> Result<Vec<Subrecord>, EsmError> {
        let p = self.payload()?;
        parse_subrecords(&p).map_err(|(pos, msg)| self.structure_error(pos, msg))
    }

    /// [`Record::subrecords`] without copying them: `f` gets each one's signature and data,
    /// in record order, and the result is `Err` exactly when [`Record::subrecords`]'s is (`f`
    /// has then seen the ones before the bad one).
    pub fn visit_subrecords(&self, f: impl FnMut(&[u8; 4], &[u8])) -> Result<(), EsmError> {
        let p = self.payload()?;
        visit_subrecords(&p, f).map_err(|(pos, msg)| self.structure_error(pos, msg))
    }

    fn structure_error(&self, pos: usize, msg: String) -> EsmError {
        EsmError::Structure {
            offset: self.offset,
            msg: format!("record {} {:08X}: subrecord at payload+{pos}: {msg}", self.sig_str(), self.header.form_id),
        }
    }
}

/// Parse a record payload into subrecords, resolving XXXX size extensions.
pub fn parse_subrecords(p: &[u8]) -> Result<Vec<Subrecord>, (usize, String)> {
    let mut out = Vec::new();
    visit_sized(p, |sig, data, extended| out.push(Subrecord { sig: *sig, data: data.to_vec(), extended }))?;
    Ok(out)
}

/// Walk a record payload's subrecords without copying them: `f` gets each one's signature
/// and data, by the rules and with the errors of [`parse_subrecords`].
pub fn visit_subrecords(p: &[u8], mut f: impl FnMut(&[u8; 4], &[u8])) -> Result<(), (usize, String)> {
    visit_sized(p, |sig, data, _| f(sig, data))
}

/// The walk itself; `f`'s last argument is [`Subrecord::extended`].
fn visit_sized(p: &[u8], mut f: impl FnMut(&[u8; 4], &[u8], bool)) -> Result<(), (usize, String)> {
    let mut c = Cursor::new(p);
    let mut pending_size: Option<u32> = None;
    while !c.is_empty() {
        let at = c.pos;
        let sig = c.tag().map_err(|e| (at, e.to_string()))?;
        let size16 = c.u16().map_err(|e| (at, e.to_string()))?;
        if sig == XXXX {
            if size16 != 4 {
                return Err((at, format!("XXXX with size {size16}, expected 4")));
            }
            pending_size = Some(c.u32().map_err(|e| (at, e.to_string()))?);
            continue;
        }
        let (size, extended) = match pending_size.take() {
            Some(s) => {
                if size16 != 0 {
                    return Err((at, format!("subrecord after XXXX has non-zero size field {size16}")));
                }
                (s as usize, true)
            }
            None => (size16 as usize, false),
        };
        let data = c.bytes(size).map_err(|e| (at, format!("{} data: {e}", tag_str(&sig))))?;
        f(&sig, data, extended);
    }
    if pending_size.is_some() {
        return Err((p.len(), "XXXX at end of record".into()));
    }
    Ok(())
}

/// Serialize subrecords back to a payload (inverse of `parse_subrecords`).
pub fn write_subrecords(subs: &[Subrecord]) -> Vec<u8> {
    let mut out = Vec::new();
    for s in subs {
        if s.extended || s.data.len() > u16::MAX as usize {
            out.extend_from_slice(&XXXX);
            out.extend_from_slice(&4u16.to_le_bytes());
            out.extend_from_slice(&(s.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&s.sig);
            out.extend_from_slice(&0u16.to_le_bytes());
        } else {
            out.extend_from_slice(&s.sig);
            out.extend_from_slice(&(s.data.len() as u16).to_le_bytes());
        }
        out.extend_from_slice(&s.data);
    }
    out
}

fn structure(offset: usize, msg: impl Into<String>) -> EsmError {
    EsmError::Structure { offset: offset as u64, msg: msg.into() }
}

/// Parse a sequence of nodes occupying `data[start..end]`.
/// Parse `data[start..end]`. Record payloads are slices sharing `data`'s buffer.
pub fn parse_nodes(data: &Bytes, start: usize, end: usize, depth: usize) -> Result<Vec<Node>, EsmError> {
    if depth > 16 {
        return Err(structure(start, "groups nested deeper than 16 levels"));
    }
    let mut out = Vec::new();
    let mut pos = start;
    while pos < end {
        if end - pos < 24 {
            return Err(structure(pos, format!("{} trailing bytes, too short for a header", end - pos)));
        }
        let mut c = Cursor::new(&data[pos..pos + 24]);
        let sig = c.tag().unwrap();
        if sig == GRUP {
            let h = GroupHeader {
                size: c.u32().unwrap(),
                label: c.tag().unwrap(),
                group_type: c.i32().unwrap(),
                stamp: c.u16().unwrap(),
                unknown1: c.u16().unwrap(),
                version: c.u16().unwrap(),
                unknown2: c.u16().unwrap(),
            };
            let gend = pos
                .checked_add(h.size as usize)
                .filter(|&e| e <= end && h.size as usize >= GROUP_HEADER_SIZE)
                .ok_or_else(|| structure(pos, format!("group size {} overruns its container", h.size)))?;
            let children = parse_nodes(data, pos + GROUP_HEADER_SIZE, gend, depth + 1)?;
            out.push(Node::Group(Group { header: h, children, offset: pos as u64 }));
            pos = gend;
        } else {
            let h = RecordHeader {
                sig,
                data_size: c.u32().unwrap(),
                flags: c.u32().unwrap(),
                form_id: c.u32().unwrap(),
                vc_info: c.u32().unwrap(),
                form_version: c.u16().unwrap(),
                vc_info2: c.u16().unwrap(),
            };
            let dstart = pos + RECORD_HEADER_SIZE;
            let dend = dstart
                .checked_add(h.data_size as usize)
                .filter(|&e| e <= end)
                .ok_or_else(|| {
                    structure(pos, format!("record {} data size {} overruns its container", tag_str(&sig), h.data_size))
                })?;
            out.push(Node::Record(Record { header: h, raw: data.slice(dstart..dend), offset: pos as u64 }));
            pos = dend;
        }
    }
    Ok(out)
}

pub fn write_nodes(nodes: &[Node], out: &mut Vec<u8>) {
    for n in nodes {
        match n {
            Node::Record(r) => {
                let h = &r.header;
                out.extend_from_slice(&h.sig);
                out.extend_from_slice(&(r.raw.len() as u32).to_le_bytes());
                out.extend_from_slice(&h.flags.to_le_bytes());
                out.extend_from_slice(&h.form_id.to_le_bytes());
                out.extend_from_slice(&h.vc_info.to_le_bytes());
                out.extend_from_slice(&h.form_version.to_le_bytes());
                out.extend_from_slice(&h.vc_info2.to_le_bytes());
                out.extend_from_slice(&r.raw);
            }
            Node::Group(g) => {
                let start = out.len();
                let h = &g.header;
                out.extend_from_slice(&GRUP);
                out.extend_from_slice(&0u32.to_le_bytes()); // patched below
                out.extend_from_slice(&h.label);
                out.extend_from_slice(&h.group_type.to_le_bytes());
                out.extend_from_slice(&h.stamp.to_le_bytes());
                out.extend_from_slice(&h.unknown1.to_le_bytes());
                out.extend_from_slice(&h.version.to_le_bytes());
                out.extend_from_slice(&h.unknown2.to_le_bytes());
                write_nodes(&g.children, out);
                let size = (out.len() - start) as u32;
                out[start + 4..start + 8].copy_from_slice(&size.to_le_bytes());
            }
        }
    }
}

/// Depth-first visit of every record, with the chain of enclosing group headers.
pub fn visit_records<'a, F: FnMut(&'a Record, &[&'a GroupHeader])>(nodes: &'a [Node], f: &mut F) {
    let mut stack: Vec<&'a GroupHeader> = Vec::new();
    fn go<'a, F: FnMut(&'a Record, &[&'a GroupHeader])>(nodes: &'a [Node], stack: &mut Vec<&'a GroupHeader>, f: &mut F) {
        for n in nodes {
            match n {
                Node::Record(r) => f(r, stack),
                Node::Group(g) => {
                    stack.push(&g.header);
                    go(&g.children, stack, f);
                    stack.pop();
                }
            }
        }
    }
    go(nodes, &mut stack, f);
}

pub fn visit_groups<'a, F: FnMut(&'a Group, usize)>(nodes: &'a [Node], f: &mut F) {
    fn go<'a, F: FnMut(&'a Group, usize)>(nodes: &'a [Node], depth: usize, f: &mut F) {
        for n in nodes {
            if let Node::Group(g) = n {
                f(g, depth);
                go(&g.children, depth + 1, f);
            }
        }
    }
    go(nodes, 0, f);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(sig: &[u8; 4], data: &[u8]) -> Subrecord {
        Subrecord { sig: *sig, data: data.to_vec(), extended: false }
    }

    #[test]
    fn subrecord_roundtrip_with_xxxx() {
        let big = vec![0xABu8; 70_000];
        let subs = vec![
            sub(b"EDID", b"TestRec\0"),
            Subrecord { sig: *b"OFST", data: big.clone(), extended: true },
            sub(b"DATA", &[1, 2, 3, 4]),
        ];
        let bytes = write_subrecords(&subs);
        let back = parse_subrecords(&bytes).unwrap();
        assert_eq!(back, subs);
        assert_eq!(back[1].data.len(), 70_000);
    }

    #[test]
    fn subrecord_errors() {
        assert!(parse_subrecords(b"EDID\x05\x00abc").is_err());
        assert!(parse_subrecords(b"XXXX\x04\x00\x10\x00\x00\x00").is_err());
        assert!(parse_subrecords(b"XXXX\x02\x00\x10\x00").is_err());
    }

    #[test]
    fn nodes_roundtrip_and_errors() {
        // TES4 record + one top group with a record inside.
        let rec = |sig: &[u8; 4], fid: u32, payload: &[u8]| {
            let mut v = Vec::new();
            v.extend_from_slice(sig);
            v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            v.extend_from_slice(&0u32.to_le_bytes());
            v.extend_from_slice(&fid.to_le_bytes());
            v.extend_from_slice(&[0; 8]);
            v.extend_from_slice(payload);
            v
        };
        let inner = rec(b"GLOB", 0x10, &write_subrecords(&[sub(b"EDID", b"G\0")]));
        let mut grp = Vec::new();
        grp.extend_from_slice(b"GRUP");
        grp.extend_from_slice(&((24 + inner.len()) as u32).to_le_bytes());
        grp.extend_from_slice(b"GLOB");
        grp.extend_from_slice(&[0; 12]);
        grp.extend_from_slice(&inner);
        let mut file = rec(b"TES4", 0, &write_subrecords(&[sub(b"HEDR", &[0; 12])]));
        file.extend_from_slice(&grp);
        let file = Bytes::from(file);
        let nodes = parse_nodes(&file, 0, file.len(), 0).unwrap();
        let mut out = Vec::new();
        write_nodes(&nodes, &mut out);
        assert_eq!(out, file);
        let mut n = 0;
        visit_records(&nodes, &mut |_r, _s| n += 1);
        assert_eq!(n, 2);
        // Corrupt the group size: must be a structured error, not a panic.
        let mut bad = file.to_vec();
        let gpos = file.len() - grp.len();
        bad[gpos + 4..gpos + 8].copy_from_slice(&9999u32.to_le_bytes());
        assert!(parse_nodes(&Bytes::from(bad), 0, file.len(), 0).is_err());
        assert!(parse_nodes(&file.slice(..file.len() - 3), 0, file.len() - 3, 0).is_err());
    }

    fn grup(size: u32, body: &[u8]) -> Vec<u8> {
        let mut v = b"GRUP".to_vec();
        v.extend_from_slice(&size.to_le_bytes());
        v.extend_from_slice(&[0; 16]);
        v.extend_from_slice(body);
        v
    }

    fn record(flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut v = b"GLOB".to_vec();
        v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        v.extend_from_slice(&flags.to_le_bytes());
        v.extend_from_slice(&[0; 12]);
        v.extend_from_slice(payload);
        v
    }

    fn only_record(data: Vec<u8>) -> Record {
        let b = Bytes::from(data);
        match parse_nodes(&b, 0, b.len(), 0).unwrap().remove(0) {
            Node::Record(r) => r,
            Node::Group(_) => unreachable!(),
        }
    }

    #[test]
    fn groups_nested_too_deep_are_an_error() {
        let nest = |levels: usize| {
            let mut body = Vec::new();
            for _ in 0..levels {
                body = grup((24 + body.len()) as u32, &body);
            }
            Bytes::from(body)
        };
        let ok = nest(16);
        assert!(parse_nodes(&ok, 0, ok.len(), 0).is_ok());
        // Thousands of levels must not recurse that deep (stack overflow).
        for levels in [17, 5000] {
            let deep = nest(levels);
            let e = parse_nodes(&deep, 0, deep.len(), 0).unwrap_err().to_string();
            assert!(e.contains("nested deeper"), "{e}");
        }
    }

    #[test]
    fn group_sizes_below_the_header_or_past_the_parent_are_errors() {
        // A size under 24 would make the walk stand still or step backwards.
        for size in [0u32, 1, 23] {
            let b = Bytes::from(grup(size, &[0; 8]));
            assert!(parse_nodes(&b, 0, b.len(), 0).is_err(), "size {size}");
        }
        // An inner group that runs past its parent, and sizes that overflow on addition.
        for inner in [100u32, u32::MAX] {
            let b = Bytes::from(grup(48, &grup(inner, &[])));
            assert!(parse_nodes(&b, 0, b.len(), 0).is_err(), "inner {inner}");
        }
        let mut r = record(0, b"EDID\x02\x00x\0");
        r[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        let b = Bytes::from(r);
        assert!(parse_nodes(&b, 0, b.len(), 0).is_err());
    }

    #[test]
    fn bad_compressed_payloads_are_errors() {
        let comp = |declared: u32, stream: &[u8]| {
            let mut p = declared.to_le_bytes().to_vec();
            p.extend_from_slice(stream);
            only_record(record(record_flags::COMPRESSED, &p))
        };
        let good = fnv_inflate::zlib_store(b"EDID\x02\x00x\0");
        assert_eq!(comp(8, &good).subrecords().unwrap().len(), 1);
        // Declared size wrong, over the cap, garbage stream, cut stream, no size at all.
        assert!(comp(9, &good).payload().is_err());
        let e = comp(u32::MAX, &good).payload().unwrap_err().to_string();
        assert!(e.contains("limit"), "{e}");
        assert!(comp(MAX_DECOMPRESSED_RECORD as u32 + 1, &good).payload().is_err());
        assert!(comp(8, b"not zlib at all").payload().is_err());
        assert!(comp(8, &good[..good.len() / 2]).payload().is_err());
        assert!(only_record(record(record_flags::COMPRESSED, &[1, 0])).payload().is_err());
    }

    #[test]
    fn xxxx_sizes_past_the_payload_are_errors() {
        let mut p = b"XXXX\x04\x00".to_vec();
        p.extend_from_slice(&u32::MAX.to_le_bytes());
        p.extend_from_slice(b"OFST\x00\x00abc");
        assert!(parse_subrecords(&p).is_err());
        // Two XXXX in a row: the second size wins, and it still has to fit.
        let mut p = b"XXXX\x04\x00\x00\x00\x00\x10XXXX\x04\x00\x03\x00\x00\x00".to_vec();
        p.extend_from_slice(b"OFST\x00\x00abc");
        assert_eq!(parse_subrecords(&p).unwrap()[0].data, b"abc");
    }
}
