//! Schema-driven decoding of subrecords into JSON values, plus coverage
//! accounting. Unknown or mis-sized data is preserved (hex) and reported, never
//! dropped (spec section 7).

use crate::raw::{Record, Subrecord};
use crate::schema::{Prim, Schema, TypeDef};
use serde_json::{json, Map, Value};
use fnv_util::{sha256::to_hex, tag_str, Cursor};
use rayon::prelude::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubStatus {
    /// Decoded and consumed exactly.
    Ok,
    /// No definition for this signature in this record type.
    Unknown,
    /// Decoded, but bytes were left over (layout probably incomplete).
    Trailing,
    /// Data too short or otherwise inconsistent with the definition.
    Error,
}

impl SubStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubStatus::Ok => "ok",
            SubStatus::Unknown => "unknown",
            SubStatus::Trailing => "trailing",
            SubStatus::Error => "error",
        }
    }
}

pub struct DecodeCtx<'a> {
    pub schema: &'a Schema,
    /// Called with every decoded FormID value (file-local), for reference checks.
    pub on_formid: Option<&'a mut dyn FnMut(u32)>,
}

pub fn formid_str(v: u32) -> String {
    format!("0x{v:08X}")
}

fn read_prim(p: Prim, c: &mut Cursor, ctx: &mut DecodeCtx) -> Result<Value, String> {
    let e = |e: fnv_util::OutOfBounds| e.to_string();
    Ok(match p {
        Prim::U8 => Value::from(c.u8().map_err(e)?),
        Prim::U16 => Value::from(c.u16().map_err(e)?),
        Prim::U32 => Value::from(c.u32().map_err(e)?),
        Prim::U64 => Value::from(c.u64().map_err(e)?),
        Prim::I8 => Value::from(c.i8().map_err(e)?),
        Prim::I16 => Value::from(c.i16().map_err(e)?),
        Prim::I32 => Value::from(c.i32().map_err(e)?),
        Prim::I64 => Value::from(c.i64().map_err(e)?),
        Prim::F32 => Value::from(c.f32().map_err(e)? as f64),
        Prim::F64 => Value::from(c.f64().map_err(e)?),
        Prim::FormId => {
            let v = c.u32().map_err(e)?;
            FORMID_OFFSETS.with_borrow_mut(|o| {
                if let Some(o) = o {
                    o.push(c.pos - 4);
                }
            });
            if let Some(f) = ctx.on_formid.as_mut() {
                f(v);
            }
            Value::from(formid_str(v))
        }
        Prim::ZString => {
            let rest = c.bytes(c.remaining()).unwrap();
            if rest.last() != Some(&0) && !rest.is_empty() {
                // Not terminated: keep the text but flag it via an error so the
                // definition gets reviewed.
                return Err(format!("zstring not NUL-terminated: {:?}", fnv_util::text::decode(rest)));
            }
            Value::from(fnv_util::text::decode_z(rest))
        }
        Prim::ZStringArray => {
            let rest = c.bytes(c.remaining()).unwrap();
            if rest.is_empty() {
                return Ok(Value::Array(Vec::new()));
            }
            let Some(body) = rest.strip_suffix(&[0]) else {
                return Err(format!("zstring array not NUL-terminated: {:?}", fnv_util::text::decode(rest)));
            };
            // Drop the extra NUL that closes the list, when present.
            let body = body.strip_suffix(&[0]).unwrap_or(body);
            if body.is_empty() {
                return Ok(Value::Array(Vec::new()));
            }
            Value::Array(body.split(|&b| b == 0).map(|s| Value::from(fnv_util::text::decode(s))).collect())
        }
        Prim::String => Value::from(fnv_util::text::decode(c.bytes(c.remaining()).unwrap())),
        Prim::LString => {
            let n = c.u32().map_err(e)? as usize;
            Value::from(fnv_util::text::decode(c.bytes(n).map_err(e)?))
        }
        Prim::Bytes => Value::from(to_hex(c.bytes(c.remaining()).unwrap())),
    })
}

fn read_int(p: Prim, c: &mut Cursor) -> Result<i64, String> {
    let e = |e: fnv_util::OutOfBounds| e.to_string();
    Ok(match p {
        Prim::U8 => c.u8().map_err(e)? as i64,
        Prim::U16 => c.u16().map_err(e)? as i64,
        Prim::U32 => c.u32().map_err(e)? as i64,
        Prim::U64 => c.u64().map_err(e)? as i64,
        Prim::I8 => c.i8().map_err(e)? as i64,
        Prim::I16 => c.i16().map_err(e)? as i64,
        Prim::I32 => c.i32().map_err(e)? as i64,
        Prim::I64 => c.i64().map_err(e)?,
        _ => return Err("non-integer base".into()),
    })
}

pub fn decode_type(t: &TypeDef, c: &mut Cursor, ctx: &mut DecodeCtx) -> Result<Value, String> {
    let schema = ctx.schema;
    match schema.resolve(t) {
        TypeDef::Prim(p) => read_prim(*p, c, ctx),
        TypeDef::Named(_) => Err("unresolved type".into()),
        TypeDef::Union(variants) => {
            let left = c.remaining();
            let chosen = variants
                .iter()
                .find(|(n, _)| *n == Some(left))
                .or_else(|| variants.iter().find(|(n, _)| n.is_none()))
                .ok_or_else(|| format!("no union variant for {left} bytes"))?;
            decode_type(&chosen.1, c, ctx)
        }
        TypeDef::Struct(fields) => {
            let mut obj = Map::new();
            for f in fields {
                if f.optional && c.is_empty() {
                    break;
                }
                let v = decode_type(&f.ty, c, ctx).map_err(|e| format!("{}: {e}", f.name))?;
                obj.insert(f.name.clone(), v);
            }
            Ok(Value::Object(obj))
        }
        TypeDef::Array { elem, count, prefix } => {
            let mut out = Vec::new();
            let count = match prefix {
                Some(p) => {
                    let n = read_int(*p, c)?;
                    // Every element takes at least a byte: a count past the data is wrong.
                    if n < 0 || n as usize > c.remaining() {
                        return Err(format!("array count {n} with {} bytes left", c.remaining()));
                    }
                    Some(n as usize)
                }
                None => *count,
            };
            match count {
                Some(n) => {
                    for i in 0..n {
                        let before = c.pos;
                        out.push(decode_type(elem, c, ctx).map_err(|e| format!("[{i}]: {e}"))?);
                        if c.pos == before {
                            // Zero-width elements would let a schema demand unbounded
                            // output from a few bytes of data.
                            return Err("array element consumed no bytes".into());
                        }
                    }
                }
                None => {
                    let mut i = 0;
                    while !c.is_empty() {
                        let before = c.pos;
                        out.push(decode_type(elem, c, ctx).map_err(|e| format!("[{i}]: {e}"))?);
                        if c.pos == before {
                            return Err("array element consumed no bytes".into());
                        }
                        i += 1;
                    }
                }
            }
            Ok(Value::Array(out))
        }
        TypeDef::Enum { base, values } => {
            let v = read_int(*base, c)?;
            match values.iter().find(|(k, _)| *k == v) {
                Some((_, name)) => Ok(Value::from(name.as_str())),
                None => Ok(Value::from(v)),
            }
        }
        TypeDef::Flags { base, bits } => {
            let v = read_int(*base, c)? as u64;
            let mut names = Vec::new();
            let mut rest = v;
            for (b, n) in bits {
                if v & (1u64 << b) != 0 {
                    names.push(Value::from(n.as_str()));
                    rest &= !(1u64 << b);
                }
            }
            let mut o = json!({"value": v, "set": names});
            if rest != 0 {
                o["unknown_bits"] = json!(format!("0x{rest:x}"));
            }
            Ok(o)
        }
    }
}

pub struct DecodedSub {
    pub status: SubStatus,
    /// The whole subrecord is defined only as opaque `bytes` (layout not understood).
    pub opaque: bool,
    pub name: Option<String>,
    pub value: Option<Value>,
    pub error: Option<String>,
}

pub fn decode_sub(rec_sig: &[u8; 4], s: &Subrecord, ctx: &mut DecodeCtx) -> DecodedSub {
    let def = match ctx.schema.lookup(rec_sig, &s.sig) {
        Some(d) => d,
        None => return DecodedSub { status: SubStatus::Unknown, opaque: false, name: None, value: None, error: None },
    };
    let opaque = matches!(ctx.schema.resolve(&def.ty), TypeDef::Prim(Prim::Bytes));
    let mut c = Cursor::new(&s.data);
    match decode_type(&def.ty, &mut c, ctx) {
        Ok(v) => {
            if c.is_empty() {
                DecodedSub { status: SubStatus::Ok, opaque, name: Some(def.name.clone()), value: Some(v), error: None }
            } else {
                let rest = to_hex(&s.data[c.pos..]);
                DecodedSub {
                    status: SubStatus::Trailing,
                    opaque,
                    name: Some(def.name.clone()),
                    value: Some(v),
                    error: Some(format!("{} trailing bytes: {rest}", s.data.len() - c.pos)),
                }
            }
        }
        Err(e) => {
            DecodedSub { status: SubStatus::Error, opaque, name: Some(def.name.clone()), value: None, error: Some(e) }
        }
    }
}

thread_local! {
    /// While [`formid_offsets`] decodes: the byte offset of every FormID read. Not a field of
    /// [`DecodeCtx`], whose literals stand in many files that follow upstream.
    static FORMID_OFFSETS: std::cell::RefCell<Option<Vec<usize>>> = const { std::cell::RefCell::new(None) };
}

/// The byte offsets of the FormIDs in a subrecord, from the schema. An error when the
/// schema cannot account for all of its bytes, so a caller that rewrites FormIDs does not
/// miss one in a part it cannot see.
pub fn formid_offsets(rec_sig: &[u8; 4], s: &Subrecord, schema: &Schema) -> Result<Vec<usize>, String> {
    FORMID_OFFSETS.set(Some(Vec::new()));
    let decoded = decode_sub(rec_sig, s, &mut DecodeCtx { schema, on_formid: None });
    let offsets = FORMID_OFFSETS.take().unwrap_or_default();
    match decoded.status {
        SubStatus::Ok if !decoded.opaque => Ok(offsets),
        status => Err(format!("{} {}: schema status {}", tag_str(rec_sig), s.sig_str(), status.as_str())),
    }
}

/// Decode a whole record to JSON.
pub fn decode_record(r: &Record, schema: &Schema) -> Value {
    let mut out = json!({
        "formid": formid_str(r.header.form_id),
        "type": r.sig_str(),
        "flags": r.header.flags,
        "form_version": r.header.form_version,
    });
    if let Some(rd) = schema.records.get(&r.header.sig) {
        out["type_name"] = json!(rd.name);
        // Header flag bits the schema names for this record type (`header_flags`).
        let named = rd.header_flag_names(r.header.flags);
        if !named.is_empty() {
            out["flag_names"] = json!(named);
        }
    }
    match r.subrecords() {
        Err(e) => {
            out["error"] = json!(e.to_string());
        }
        Ok(subs) => {
            let mut arr = Vec::with_capacity(subs.len());
            let mut ctx = DecodeCtx { schema, on_formid: None };
            for s in &subs {
                let d = decode_sub(&r.header.sig, s, &mut ctx);
                let mut o = Map::new();
                o.insert("sig".into(), json!(s.sig_str()));
                if let Some(n) = d.name {
                    o.insert("name".into(), json!(n));
                }
                o.insert("status".into(), json!(d.status.as_str()));
                match d.value {
                    Some(v) => o.insert("value".into(), v),
                    None => o.insert("raw".into(), json!(to_hex(&s.data))),
                };
                if let Some(e) = d.error {
                    o.insert("error".into(), json!(e));
                }
                arr.push(Value::Object(o));
            }
            out["subrecords"] = Value::Array(arr);
        }
    }
    out
}

/// Per (record type, subrecord signature) coverage counters.
#[derive(Debug, Clone, Default)]
pub struct SubCoverage {
    pub total: u64,
    pub ok: u64,
    /// Subset of `ok` whose definition is opaque `bytes`.
    pub opaque: u64,
    pub unknown: u64,
    pub trailing: u64,
    pub error: u64,
    pub sizes: BTreeMap<usize, u64>,
    pub first_error: Option<String>,
}

#[derive(Debug, Default)]
pub struct Coverage {
    pub by_sub: BTreeMap<(String, String), SubCoverage>,
    pub records: BTreeMap<String, u64>,
    pub record_errors: Vec<String>,
}

/// Coverage outcome of one record, computed independently of every other record.
pub struct RecordCoverage {
    sig: String,
    form_id: u32,
    /// Err if the payload cannot be split into subrecords.
    subs: Result<Vec<SubOutcome>, String>,
}

struct SubOutcome {
    sig: String,
    size: usize,
    status: SubStatus,
    opaque: bool,
    error: Option<String>,
}

impl Coverage {
    /// Decode one record's subrecords against the schema (no shared state).
    pub fn measure<'a>(r: &Record, schema: &'a Schema, on_formid: Option<&'a mut dyn FnMut(u32)>) -> RecordCoverage {
        let subs = r.subrecords().map_err(|e| e.to_string()).map(|subs| {
            let mut ctx = DecodeCtx { schema, on_formid };
            subs.iter()
                .map(|s| {
                    let d = decode_sub(&r.header.sig, s, &mut ctx);
                    SubOutcome { sig: s.sig_str(), size: s.data.len(), status: d.status, opaque: d.opaque, error: d.error }
                })
                .collect()
        });
        RecordCoverage { sig: r.sig_str(), form_id: r.header.form_id, subs }
    }

    /// Fold one measured record into the totals. Records must be added in file
    /// order: `first_error` and `record_errors` keep the earliest occurrences.
    pub fn add(&mut self, rc: RecordCoverage) {
        *self.records.entry(rc.sig.clone()).or_default() += 1;
        let subs = match rc.subs {
            Ok(s) => s,
            Err(e) => {
                if self.record_errors.len() < 100 {
                    self.record_errors.push(e);
                }
                return;
            }
        };
        for sub in subs {
            let e = self.by_sub.entry((rc.sig.clone(), sub.sig)).or_default();
            e.total += 1;
            *e.sizes.entry(sub.size).or_default() += 1;
            match sub.status {
                SubStatus::Ok => {
                    e.ok += 1;
                    if sub.opaque {
                        e.opaque += 1;
                    }
                }
                SubStatus::Unknown => e.unknown += 1,
                SubStatus::Trailing => e.trailing += 1,
                SubStatus::Error => e.error += 1,
            }
            if e.first_error.is_none() {
                if let Some(err) = sub.error {
                    e.first_error = Some(format!("{:08X}: {err}", rc.form_id));
                }
            }
        }
    }

    pub fn add_record<'a>(&mut self, r: &Record, schema: &'a Schema, on_formid: Option<&'a mut dyn FnMut(u32)>) {
        self.add(Self::measure(r, schema, on_formid));
    }

    /// [`Coverage::add_record`] for many records, decoded in parallel and folded in
    /// order, so the result is identical to adding them one by one.
    pub fn add_records_par(&mut self, recs: &[&Record], schema: &Schema) {
        for chunk in recs.chunks(4096) {
            let measured: Vec<RecordCoverage> = chunk.par_iter().map(|r| Self::measure(r, schema, None)).collect();
            for rc in measured {
                self.add(rc);
            }
        }
    }

    pub fn totals(&self) -> (u64, u64, u64, u64, u64) {
        let mut t = (0, 0, 0, 0, 0);
        for c in self.by_sub.values() {
            t.0 += c.total;
            t.1 += c.ok;
            t.2 += c.unknown;
            t.3 += c.trailing;
            t.4 += c.error;
        }
        t
    }

    pub fn to_json(&self, max_rows: usize) -> Value {
        let (total, ok, unknown, trailing, error) = self.totals();
        let pct = |n: u64| if total == 0 { 0.0 } else { (n as f64) * 100.0 / total as f64 };
        let summary = json!({
            "subrecords": total,
            "ok": ok,
            "unknown": unknown,
            "trailing": trailing,
            "error": error,
            "ok_percent": (pct(ok) * 100.0).round() / 100.0,
            "opaque_bytes": self.by_sub.values().map(|c| c.opaque).sum::<u64>(),
            "distinct_pairs": self.by_sub.len(),
            "pairs_fully_decoded": self.by_sub.values().filter(|c| c.ok == c.total).count(),
        });
        // Work queue: pairs with the most undecoded instances first.
        let mut rows: Vec<(&(String, String), &SubCoverage)> =
            self.by_sub.iter().filter(|(_, c)| c.ok < c.total).collect();
        rows.sort_by(|a, b| (b.1.total - b.1.ok).cmp(&(a.1.total - a.1.ok)).then(a.0.cmp(b.0)));
        let mut q = Vec::new();
        for ((rt, st), c) in rows.into_iter().take(max_rows) {
            let sizes: Vec<Value> = {
                let mut sv: Vec<(&usize, &u64)> = c.sizes.iter().collect();
                sv.sort_by(|a, b| b.1.cmp(a.1));
                sv.into_iter().take(5).map(|(sz, n)| json!({"size": sz, "count": n})).collect()
            };
            let mut o = json!({
                "record": rt,
                "sub": st,
                "total": c.total,
                "ok": c.ok,
                "unknown": c.unknown,
                "trailing": c.trailing,
                "error": c.error,
                "top_sizes": sizes,
            });
            if let Some(e) = &c.first_error {
                o["first_error"] = json!(e);
            }
            q.push(o);
        }
        json!({"summary": summary, "work_queue": q, "record_errors": self.record_errors})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_util::Cursor;

    #[test]
    fn counted_arrays_of_length_prefixed_strings() {
        // MODS: count, then per entry a u32-length name, a FormID and an index.
        let mut s = Schema::default();
        let t = r#"{"format":"fnv.schema/1","types":{"Alt":{"struct":[{"name":"shape_name","type":"lstring"},
          {"name":"texture_set","type":"formid"},{"name":"shape_index","type":"i32"}]}},
          "common":{"MODS":{"name":"Alternate Textures","type":{"array":"Alt","count":"u32"}}}}"#;
        s.merge_str(t, "t").unwrap();
        s.validate().unwrap();
        let mut b = vec![2, 0, 0, 0];
        for (name, fid, idx) in [("GO:1", 0x53363u32, 1i32), ("arms", 0x6bcf3, 0)] {
            b.extend((name.len() as u32).to_le_bytes());
            b.extend(name.as_bytes());
            b.extend(fid.to_le_bytes());
            b.extend(idx.to_le_bytes());
        }
        let ty = s.lookup(b"STAT", b"MODS").unwrap().ty.clone();
        let mut ctx = DecodeCtx { schema: &s, on_formid: None };
        let mut c = Cursor::new(&b);
        let v = decode_type(&ty, &mut c, &mut ctx).unwrap();
        assert!(c.is_empty());
        assert_eq!(v[0]["shape_name"], "GO:1");
        assert_eq!(v[0]["texture_set"], "0x00053363");
        assert_eq!(v[1]["shape_index"], 0);
        // A count past the data is an error, not a long loop.
        let mut c = Cursor::new(&[0xFF, 0xFF, 0, 0, 1, 2]);
        assert!(decode_type(&ty, &mut c, &mut ctx).is_err());
        let bad = r#"{"format":"fnv.schema/1","common":{"MODS":{"type":{"array":"u8","count":"f32"}}}}"#;
        assert!(Schema::default().merge_str(bad, "x").is_err());
    }
}
