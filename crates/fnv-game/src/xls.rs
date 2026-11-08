//! Excel 97-2003 workbooks (`.xls`) for SUP's Excel functions (`crate::nvse_sup5`;
//! docs/spec-notes/ext-sup5.md): a workbook is a BIFF8 record stream named `Workbook`
//! inside a compound file [documented: Microsoft's open specifications [MS-CFB] (the
//! compound file: header, sector allocation table, directory) and [MS-XLS] (the BIFF8
//! records)]. Read: the cell values of every sheet (numbers, RK and MULRK numbers, shared
//! and inline strings, formula results, booleans); written: one `Workbook` stream with the
//! sheets, their names and values. Formatting is neither read nor kept.

use std::collections::BTreeMap;

/// A cell's value. The kinds are the ones ExcelGetCellValueType reports [documented:
/// ExcelGetCellValueType, 1 int, 2 float, 3 string, 4 wide string, 5 formula].
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Int(i32),
    Float(f64),
    /// Text whose characters all fit in one byte (stored 8 bits a character).
    Str(String),
    /// Text with a character past U+00FF (stored 16 bits a character).
    WStr(String),
    /// A formula's last result (the formula itself is not kept).
    Formula(Box<Cell>),
}

impl Cell {
    /// The value type number [documented: ExcelGetCellValueType].
    pub fn kind(&self) -> i32 {
        match self {
            Cell::Int(_) => 1,
            Cell::Float(_) => 2,
            Cell::Str(_) => 3,
            Cell::WStr(_) => 4,
            Cell::Formula(_) => 5,
        }
    }

    /// The value as a number: text gives 0 [hypothesis].
    pub fn number(&self) -> f64 {
        match self {
            Cell::Int(i) => f64::from(*i),
            Cell::Float(x) => *x,
            Cell::Formula(c) => c.number(),
            Cell::Str(_) | Cell::WStr(_) => 0.0,
        }
    }

    /// The value as text: a number gives "" [hypothesis].
    pub fn text(&self) -> String {
        match self {
            Cell::Str(s) | Cell::WStr(s) => s.clone(),
            Cell::Formula(c) => c.text(),
            Cell::Int(_) | Cell::Float(_) => String::new(),
        }
    }

    /// Text as the kind its characters need.
    pub fn of_text(s: &str) -> Cell {
        if s.chars().all(|c| (c as u32) < 0x100) {
            Cell::Str(s.to_string())
        } else {
            Cell::WStr(s.to_string())
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sheet {
    pub name: String,
    /// By (row, column), both from 0.
    pub cells: BTreeMap<(u32, u32), Cell>,
}

impl Sheet {
    /// The rows in use: the last used row + 1, 0 when empty.
    pub fn rows(&self) -> u32 {
        self.cells.keys().map(|&(r, _)| r + 1).max().unwrap_or(0)
    }

    /// The columns in use: the last used column + 1, 0 when empty.
    pub fn cols(&self) -> u32 {
        self.cells.keys().map(|&(_, c)| c + 1).max().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Book {
    pub sheets: Vec<Sheet>,
}

/// The longest sheet name Excel keeps [documented: ExcelSetSheetName, "Max name length is
/// 31 symbols"].
pub const MAX_SHEET_NAME: usize = 31;

impl Book {
    /// The sheet at `i`, the sheets before it made first, named `Sheet1`, `Sheet2`, …
    /// [documented: ExcelWriteFloat, "if sheet in file doesn't exists - it will be
    /// created"; hypothesis: the names and the sheets between].
    pub fn sheet_mut(&mut self, i: usize) -> &mut Sheet {
        while self.sheets.len() <= i {
            let n = self.sheets.len() + 1;
            self.sheets.push(Sheet { name: format!("Sheet{n}"), cells: BTreeMap::new() });
        }
        &mut self.sheets[i]
    }

    /// A workbook from a file's bytes: `None` when it is not a compound file with a
    /// BIFF5 or BIFF8 workbook stream.
    pub fn parse(bytes: &[u8]) -> Option<Book> {
        let stream = cfb_stream(bytes, &["Workbook", "Book"])?;
        parse_biff(&stream)
    }

    /// The file's bytes: `None` when the workbook is too large for a file without an
    /// extended allocation table (about 7 MB).
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        cfb_write(&write_biff(self))
    }
}

// ---- the compound file [documented: MS-CFB] ----

const SIG: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const FREESECT: u32 = 0xFFFF_FFFF;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const NOSTREAM: u32 = 0xFFFF_FFFF;
/// Sector numbers above this are markers, not sectors.
const MAXREGSECT: u32 = 0xFFFF_FFFA;

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// The first stream of a compound file named one of `names` (any case).
fn cfb_stream(b: &[u8], names: &[&str]) -> Option<Vec<u8>> {
    if b.get(..8)? != SIG {
        return None;
    }
    let ss = 1usize << u16_at(b, 30)?.min(16);
    let mss = 1usize << u16_at(b, 32)?.min(16);
    let dir_start = u32_at(b, 48)?;
    let cutoff = u32_at(b, 56)? as usize;
    let minifat_start = u32_at(b, 60)?;
    let difat_start = u32_at(b, 68)?;
    let sector = |i: u32| -> Option<&[u8]> {
        let o = (i as usize + 1).checked_mul(ss)?;
        b.get(o..o + ss)
    };
    // The allocation table's sectors: 109 in the header, the rest in a chain of DIFAT
    // sectors, each ending with the next one's number.
    let mut fat_sectors: Vec<u32> = (0..109).filter_map(|k| u32_at(b, 76 + 4 * k)).filter(|&s| s < MAXREGSECT).collect();
    let mut d = difat_start;
    let mut guard = 0;
    while d < MAXREGSECT && guard < 1 << 16 {
        let s = sector(d)?;
        let n = ss / 4 - 1;
        fat_sectors.extend((0..n).filter_map(|k| u32_at(s, 4 * k)).filter(|&s| s < MAXREGSECT));
        d = u32_at(s, 4 * n)?;
        guard += 1;
    }
    let fat: Vec<u32> = fat_sectors.iter().filter_map(|&s| sector(s)).flat_map(|s| s.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c))).collect();
    let chain = |start: u32, table: &[u32]| -> Vec<u32> {
        let mut out = Vec::new();
        let mut s = start;
        while s < MAXREGSECT && out.len() <= table.len() {
            out.push(s);
            s = *table.get(s as usize).unwrap_or(&ENDOFCHAIN);
        }
        out
    };
    let read = |start: u32| -> Vec<u8> { chain(start, &fat).into_iter().filter_map(sector).flatten().copied().collect() };
    let dir = read(dir_start);
    let entries: Vec<&[u8]> = dir.as_chunks::<128>().0.iter().map(|e| &e[..]).collect();
    let root = entries.first()?;
    let entry_size = |e: &[u8]| u32_at(e, 120).unwrap_or(0) as usize;
    let mini_stream = read(u32_at(root, 116)?);
    let minifat: Vec<u32> = read(minifat_start).as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
    for e in &entries {
        if e[66] != 2 {
            continue;
        }
        let len = (u16_at(e, 64)? as usize).min(64);
        let name: String = char::decode_utf16(e[..len].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c))).map(|c| c.unwrap_or('?')).filter(|&c| c != '\0').collect();
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let size = entry_size(e);
        let start = u32_at(e, 116)?;
        let mut data = if size < cutoff {
            chain(start, &minifat).into_iter().filter_map(|s| mini_stream.get(s as usize * mss..s as usize * mss + mss)).flatten().copied().collect()
        } else {
            read(start)
        };
        if data.len() < size {
            return None;
        }
        data.truncate(size);
        return Some(data);
    }
    None
}

/// A compound file holding one stream, `Workbook`, of `data`. The stream is padded with
/// zeros to the 4096 bytes below which a stream would go into the mini stream, so the
/// file needs none [design].
fn cfb_write(data: &[u8]) -> Option<Vec<u8>> {
    const SS: usize = 512;
    let mut data = data.to_vec();
    if data.len() < 4096 {
        data.resize(4096, 0);
    }
    let n_stream = data.len().div_ceil(SS);
    // Sectors: the stream, one directory sector, then the allocation table's own.
    let mut n_fat = 1;
    while (n_stream + 1 + n_fat).div_ceil(SS / 4) > n_fat {
        n_fat += 1;
    }
    if n_fat > 109 {
        return None;
    }
    let dir_sec = n_stream as u32;
    let fat_first = dir_sec + 1;
    let mut fat = vec![FREESECT; n_fat * SS / 4];
    for (i, slot) in fat.iter_mut().enumerate().take(n_stream) {
        *slot = if i + 1 == n_stream { ENDOFCHAIN } else { i as u32 + 1 };
    }
    fat[dir_sec as usize] = ENDOFCHAIN;
    for k in 0..n_fat {
        fat[fat_first as usize + k] = FATSECT;
    }
    let mut out = Vec::with_capacity((1 + n_stream + 1 + n_fat) * SS);
    out.extend_from_slice(&SIG);
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&0x003Eu16.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&0xFFFEu16.to_le_bytes());
    out.extend_from_slice(&9u16.to_le_bytes());
    out.extend_from_slice(&6u16.to_le_bytes());
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&0u32.to_le_bytes()); // directory sectors (version 3: 0)
    out.extend_from_slice(&(n_fat as u32).to_le_bytes());
    out.extend_from_slice(&dir_sec.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // transaction signature
    out.extend_from_slice(&4096u32.to_le_bytes()); // mini stream cutoff
    out.extend_from_slice(&ENDOFCHAIN.to_le_bytes()); // mini allocation table
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&ENDOFCHAIN.to_le_bytes()); // DIFAT chain
    out.extend_from_slice(&0u32.to_le_bytes());
    for k in 0..109 {
        let v = if k < n_fat { fat_first + k as u32 } else { FREESECT };
        out.extend_from_slice(&v.to_le_bytes());
    }
    debug_assert_eq!(out.len(), SS);
    out.extend_from_slice(&data);
    out.resize(SS * (1 + n_stream), 0);
    let entry = |name: &str, kind: u8, child: u32, start: u32, size: u32| -> Vec<u8> {
        let mut e = vec![0u8; 128];
        let units: Vec<u16> = name.encode_utf16().collect();
        for (i, u) in units.iter().enumerate() {
            e[2 * i..2 * i + 2].copy_from_slice(&u.to_le_bytes());
        }
        let len = if name.is_empty() { 0 } else { (units.len() as u16 + 1) * 2 };
        e[64..66].copy_from_slice(&len.to_le_bytes());
        e[66] = kind;
        e[67] = 1; // black
        e[68..72].copy_from_slice(&NOSTREAM.to_le_bytes());
        e[72..76].copy_from_slice(&NOSTREAM.to_le_bytes());
        e[76..80].copy_from_slice(&child.to_le_bytes());
        e[116..120].copy_from_slice(&start.to_le_bytes());
        e[120..124].copy_from_slice(&size.to_le_bytes());
        e
    };
    out.extend(entry("Root Entry", 5, 1, ENDOFCHAIN, 0));
    out.extend(entry("Workbook", 2, NOSTREAM, 0, data.len() as u32));
    out.extend(entry("", 0, NOSTREAM, 0, 0));
    out.extend(entry("", 0, NOSTREAM, 0, 0));
    for v in fat {
        out.extend_from_slice(&v.to_le_bytes());
    }
    Some(out)
}

// ---- BIFF records [documented: MS-XLS 2.4] ----

const BOF: u16 = 0x0809;
const EOF: u16 = 0x000A;
const BOUNDSHEET: u16 = 0x0085;
const SST: u16 = 0x00FC;
const CONTINUE: u16 = 0x003C;
const LABELSST: u16 = 0x00FD;
const LABEL: u16 = 0x0204;
const NUMBER: u16 = 0x0203;
const RK: u16 = 0x027E;
const MULRK: u16 = 0x00BD;
const FORMULA: u16 = 0x0006;
const STRING: u16 = 0x0207;
const BOOLERR: u16 = 0x0205;
/// The longest record body; longer data goes on in CONTINUE records.
const MAX_BODY: usize = 8224;

/// The records of a stream from `at`, as (type, body, offset).
fn records(s: &[u8], at: usize) -> impl Iterator<Item = (u16, &[u8], usize)> {
    let mut o = at;
    std::iter::from_fn(move || {
        let t = u16_at(s, o)?;
        let n = u16_at(s, o + 2)? as usize;
        let body = s.get(o + 4..o + 4 + n)?;
        let here = o;
        o += 4 + n;
        Some((t, body, here))
    })
}

/// An RK number: a 30-bit integer or the top 30 bits of a double, either one possibly
/// divided by 100 [documented: MS-XLS 2.5.217 RkNumber].
fn rk(v: u32) -> Cell {
    let div = v & 1 != 0;
    if v & 2 != 0 {
        let i = (v as i32) >> 2;
        if div {
            Cell::Float(f64::from(i) / 100.0)
        } else {
            Cell::Int(i)
        }
    } else {
        let x = f64::from_bits(u64::from(v & 0xFFFF_FFFC) << 32);
        Cell::Float(if div { x / 100.0 } else { x })
    }
}

/// Bytes over a record and its CONTINUE records: character data broken at a record's end
/// goes on after a new option byte [documented: MS-XLS 2.1.7.1].
struct Segs<'a> {
    segs: Vec<&'a [u8]>,
    i: usize,
    p: usize,
}

impl<'a> Segs<'a> {
    fn at_end_of_seg(&self) -> bool {
        self.segs.get(self.i).is_none_or(|s| self.p >= s.len())
    }

    fn next_seg(&mut self) -> bool {
        self.i += 1;
        self.p = 0;
        self.i < self.segs.len()
    }

    fn byte(&mut self) -> Option<u8> {
        while self.at_end_of_seg() {
            if !self.next_seg() {
                return None;
            }
        }
        let v = self.segs[self.i][self.p];
        self.p += 1;
        Some(v)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes([self.byte()?, self.byte()?]))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes([self.byte()?, self.byte()?, self.byte()?, self.byte()?]))
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        for _ in 0..n {
            self.byte()?;
        }
        Some(())
    }

    fn chars(&mut self, n: usize, mut wide: bool) -> Option<String> {
        let mut units = Vec::with_capacity(n);
        for _ in 0..n {
            if self.at_end_of_seg() {
                if !self.next_seg() {
                    return None;
                }
                wide = self.byte()? & 1 != 0;
            }
            units.push(if wide { self.u16()? } else { u16::from(self.byte()?) });
        }
        Some(char::decode_utf16(units).map(|c| c.unwrap_or('\u{FFFD}')).collect())
    }

    /// An XLUnicodeRichExtendedString [documented: MS-XLS 2.5.293].
    fn rich_string(&mut self) -> Option<(String, bool)> {
        let n = self.u16()? as usize;
        let flags = self.byte()?;
        let runs = if flags & 8 != 0 { self.u16()? as usize } else { 0 };
        let ext = if flags & 4 != 0 { self.u32()? as usize } else { 0 };
        let wide = flags & 1 != 0;
        let s = self.chars(n, wide)?;
        self.skip(4 * runs + ext)?;
        Some((s, wide))
    }
}

/// Text stored 16 bits a character is a wide string [hypothesis: the stored width
/// decides ExcelGetCellValueType's 3 or 4].
fn text_cell(s: String, wide: bool) -> Cell {
    if wide {
        Cell::WStr(s)
    } else {
        Cell::Str(s)
    }
}

/// An XLUnicodeString (BIFF8: count, option byte, characters) or a BIFF5 byte string.
fn label(b: &[u8], biff8: bool) -> Option<Cell> {
    let n = u16_at(b, 0)? as usize;
    if !biff8 {
        return Some(Cell::Str(b.get(2..2 + n)?.iter().map(|&c| c as char).collect()));
    }
    let wide = *b.get(2)? & 1 != 0;
    let mut s = Segs { segs: vec![&b[3..]], i: 0, p: 0 };
    Some(text_cell(s.chars(n, wide)?, wide))
}

fn parse_biff(s: &[u8]) -> Option<Book> {
    let mut it = records(s, 0);
    let (t, bof, _) = it.next()?;
    if t != BOF && t != 0x0209 && t != 0x0409 {
        return None;
    }
    let biff8 = u16_at(bof, 0)? == 0x0600;
    let mut sheets: Vec<(usize, String)> = Vec::new();
    let mut sst: Vec<(String, bool)> = Vec::new();
    let recs: Vec<(u16, &[u8], usize)> = it.take_while(|r| r.0 != EOF).collect();
    let mut k = 0;
    while k < recs.len() {
        let (t, b, _) = recs[k];
        match t {
            BOUNDSHEET => {
                let off = u32_at(b, 0)? as usize;
                // grbit's high byte: 0 a worksheet; charts and macro sheets are passed over.
                if b.get(5).copied().unwrap_or(0) == 0 {
                    let n = *b.get(6)? as usize;
                    let name = if biff8 {
                        let wide = *b.get(7)? & 1 != 0;
                        let mut sg = Segs { segs: vec![&b[8..]], i: 0, p: 0 };
                        sg.chars(n, wide)?
                    } else {
                        b.get(7..7 + n)?.iter().map(|&c| c as char).collect()
                    };
                    sheets.push((off, name));
                }
            }
            SST => {
                let mut segs = vec![&b[8..]];
                while k + 1 < recs.len() && recs[k + 1].0 == CONTINUE {
                    k += 1;
                    segs.push(recs[k].1);
                }
                let unique = u32_at(b, 4)? as usize;
                let mut sg = Segs { segs, i: 0, p: 0 };
                for _ in 0..unique {
                    match sg.rich_string() {
                        Some(v) => sst.push(v),
                        None => break,
                    }
                }
            }
            _ => {}
        }
        k += 1;
    }
    let mut book = Book::default();
    for (off, name) in sheets {
        let mut sheet = Sheet { name, cells: BTreeMap::new() };
        let mut it = records(s, off);
        match it.next() {
            Some((BOF, _, _)) => {}
            _ => continue,
        }
        let mut pending_string: Option<(u32, u32)> = None;
        for (t, b, _) in it {
            let rc = || Some((u32::from(u16_at(b, 0)?), u32::from(u16_at(b, 2)?)));
            match t {
                EOF => break,
                NUMBER => {
                    if let (Some(at), Some(x)) = (rc(), b.get(6..14)) {
                        sheet.cells.insert(at, Cell::Float(f64::from_le_bytes(x.try_into().unwrap_or([0; 8]))));
                    }
                }
                RK => {
                    if let (Some(at), Some(v)) = (rc(), u32_at(b, 6)) {
                        sheet.cells.insert(at, rk(v));
                    }
                }
                MULRK => {
                    let (Some(row), Some(first)) = (u16_at(b, 0), u16_at(b, 2)) else { continue };
                    let n = b.len().saturating_sub(6) / 6;
                    for j in 0..n {
                        if let Some(v) = u32_at(b, 4 + 6 * j + 2) {
                            sheet.cells.insert((u32::from(row), u32::from(first) + j as u32), rk(v));
                        }
                    }
                }
                LABELSST => {
                    if let (Some(at), Some(i)) = (rc(), u32_at(b, 6)) {
                        if let Some((s, wide)) = sst.get(i as usize) {
                            sheet.cells.insert(at, text_cell(s.clone(), *wide));
                        }
                    }
                }
                LABEL | 0x00D6 => {
                    if let (Some(at), Some(c)) = (rc(), b.get(6..).and_then(|x| label(x, biff8))) {
                        sheet.cells.insert(at, c);
                    }
                }
                BOOLERR => {
                    if let (Some(at), Some(&v), Some(&err)) = (rc(), b.get(6), b.get(7)) {
                        if err == 0 {
                            sheet.cells.insert(at, Cell::Int(i32::from(v)));
                        }
                    }
                }
                FORMULA => {
                    let (Some(at), Some(res)) = (rc(), b.get(6..14)) else { continue };
                    if res[6] == 0xFF && res[7] == 0xFF {
                        match res[0] {
                            // A string result: the next STRING record.
                            0 => pending_string = Some(at),
                            1 => {
                                sheet.cells.insert(at, Cell::Formula(Box::new(Cell::Int(i32::from(res[2])))));
                            }
                            3 => {
                                sheet.cells.insert(at, Cell::Formula(Box::new(Cell::Str(String::new()))));
                            }
                            _ => {}
                        }
                    } else {
                        sheet.cells.insert(at, Cell::Formula(Box::new(Cell::Float(f64::from_le_bytes(res.try_into().unwrap_or([0; 8]))))));
                    }
                }
                STRING => {
                    if let (Some(at), Some(c)) = (pending_string.take(), label(b, biff8)) {
                        sheet.cells.insert(at, Cell::Formula(Box::new(c)));
                    }
                }
                _ => {}
            }
        }
        book.sheets.push(sheet);
    }
    Some(book)
}

fn rec(out: &mut Vec<u8>, t: u16, body: &[u8]) {
    out.extend_from_slice(&t.to_le_bytes());
    out.extend_from_slice(&(body.len() as u16).to_le_bytes());
    out.extend_from_slice(body);
}

fn needs_wide(s: &str) -> bool {
    s.chars().any(|c| (c as u32) >= 0x100)
}

/// The shared string table as an SST record and its CONTINUE records: a string's count
/// and option byte never split; its characters go on after a new option byte
/// [documented: MS-XLS 2.1.7.1].
fn sst_records(strings: &[(String, bool)], total: u32) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    cur.extend_from_slice(&total.to_le_bytes());
    cur.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for (s, wide) in strings {
        let units: Vec<u16> = s.encode_utf16().collect();
        if cur.len() + 3 > MAX_BODY {
            out.push(std::mem::take(&mut cur));
        }
        cur.extend_from_slice(&(units.len() as u16).to_le_bytes());
        cur.push(u8::from(*wide));
        let unit = if *wide { 2 } else { 1 };
        let mut rest = &units[..];
        while !rest.is_empty() {
            let mut room = (MAX_BODY - cur.len()) / unit;
            if room == 0 {
                out.push(std::mem::take(&mut cur));
                cur.push(u8::from(*wide));
                room = (MAX_BODY - 1) / unit;
            }
            let n = room.min(rest.len());
            for &u in &rest[..n] {
                if *wide {
                    cur.extend_from_slice(&u.to_le_bytes());
                } else {
                    cur.push(u as u8);
                }
            }
            rest = &rest[n..];
        }
    }
    out.push(cur);
    out
}

fn write_biff(book: &Book) -> Vec<u8> {
    let mut sheets = book.sheets.clone();
    if sheets.is_empty() {
        sheets.push(Sheet { name: "Sheet1".into(), cells: BTreeMap::new() });
    }
    // The shared strings, each once, in the order the cells use them.
    let mut strings: Vec<(String, bool)> = Vec::new();
    let mut index: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut total = 0u32;
    let plain = |c: &Cell| -> Cell {
        match c {
            Cell::Formula(v) => (**v).clone(),
            v => v.clone(),
        }
    };
    for sh in &sheets {
        for c in sh.cells.values() {
            if let Cell::Str(s) | Cell::WStr(s) = plain(c) {
                total += 1;
                if !index.contains_key(&s) {
                    index.insert(s.clone(), strings.len() as u32);
                    let wide = needs_wide(&s) || matches!(plain(c), Cell::WStr(_));
                    strings.push((s, wide));
                }
            }
        }
    }
    let bof = |kind: u16| -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&0x0600u16.to_le_bytes());
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&0x0DBBu16.to_le_bytes());
        b.extend_from_slice(&0x07CCu16.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&0x0006u32.to_le_bytes());
        b
    };
    let mut g = Vec::new();
    rec(&mut g, BOF, &bof(0x0005));
    rec(&mut g, 0x0042, &1200u16.to_le_bytes()); // CODEPAGE: UTF-16
    rec(&mut g, 0x003D, &[0, 0, 0, 0, 0x5C, 0x3A, 0xBE, 0x23, 0x38, 0, 0, 0, 0, 0, 1, 0, 0x58, 0x02]); // WINDOW1
    for _ in 0..4 {
        let mut f = vec![0xC8, 0, 0, 0, 0xFF, 0x7F, 0x90, 0x01, 0, 0, 0, 0, 0, 0];
        f.extend_from_slice(&[5, 0]);
        f.extend_from_slice(b"Arial");
        rec(&mut g, 0x0031, &f); // FONT
    }
    for _ in 0..16 {
        rec(&mut g, 0x00E0, &[0, 0, 0, 0, 0xF5, 0xFF, 0x20, 0, 0, 0xF4, 0, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0x20]); // XF (style)
    }
    rec(&mut g, 0x00E0, &[0, 0, 0, 0, 0x01, 0x00, 0x20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0x20]); // XF 16 (cells)
    let mut bound_at = Vec::new();
    for sh in &sheets {
        bound_at.push(g.len() + 4);
        let name: Vec<u16> = sh.name.chars().take(MAX_SHEET_NAME).collect::<String>().encode_utf16().collect();
        let mut b = vec![0, 0, 0, 0, 0, 0, name.len() as u8, 1];
        for u in name {
            b.extend_from_slice(&u.to_le_bytes());
        }
        rec(&mut g, BOUNDSHEET, &b);
    }
    for (i, body) in sst_records(&strings, total).into_iter().enumerate() {
        rec(&mut g, if i == 0 { SST } else { CONTINUE }, &body);
    }
    rec(&mut g, EOF, &[]);
    for (k, sh) in sheets.iter().enumerate() {
        let off = g.len() as u32;
        g[bound_at[k]..bound_at[k] + 4].copy_from_slice(&off.to_le_bytes());
        rec(&mut g, BOF, &bof(0x0010));
        let mut d = Vec::new();
        d.extend_from_slice(&if sh.cells.is_empty() { 0 } else { sh.cells.keys().map(|k| k.0).min().unwrap_or(0) }.to_le_bytes());
        d.extend_from_slice(&sh.rows().to_le_bytes());
        d.extend_from_slice(&(if sh.cells.is_empty() { 0 } else { sh.cells.keys().map(|k| k.1).min().unwrap_or(0) } as u16).to_le_bytes());
        d.extend_from_slice(&(sh.cols() as u16).to_le_bytes());
        d.extend_from_slice(&[0, 0]);
        rec(&mut g, 0x0200, &d); // DIMENSIONS
        let sel: u16 = if k == 0 { 0x06B6 } else { 0x02B6 };
        let mut w = sel.to_le_bytes().to_vec();
        w.extend_from_slice(&[0, 0, 0, 0, 0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        rec(&mut g, 0x023E, &w); // WINDOW2
        for (&(r, c), cell) in &sh.cells {
            let (r16, c16) = (r.min(0xFFFF) as u16, c.min(0xFF) as u16);
            let mut b = Vec::new();
            b.extend_from_slice(&r16.to_le_bytes());
            b.extend_from_slice(&c16.to_le_bytes());
            b.extend_from_slice(&16u16.to_le_bytes());
            match plain(cell) {
                Cell::Int(i) if (-(1 << 29)..(1 << 29)).contains(&i) => {
                    b.extend_from_slice(&(((i << 2) | 2) as u32).to_le_bytes());
                    rec(&mut g, RK, &b);
                }
                Cell::Int(i) => {
                    b.extend_from_slice(&f64::from(i).to_le_bytes());
                    rec(&mut g, NUMBER, &b);
                }
                Cell::Float(x) => {
                    b.extend_from_slice(&x.to_le_bytes());
                    rec(&mut g, NUMBER, &b);
                }
                Cell::Str(s) | Cell::WStr(s) => {
                    b.extend_from_slice(&index[&s].to_le_bytes());
                    rec(&mut g, LABELSST, &b);
                }
                Cell::Formula(_) => {}
            }
        }
        rec(&mut g, EOF, &[]);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_book_comes_back_as_written() {
        let mut b = Book::default();
        b.sheet_mut(0).cells.insert((0, 0), Cell::Float(10.5));
        b.sheet_mut(0).cells.insert((4, 1), Cell::Str("MyText".into()));
        b.sheet_mut(0).cells.insert((2, 3), Cell::Int(-7));
        b.sheet_mut(2).cells.insert((1, 1), Cell::of_text("Привет"));
        b.sheets[1].name = "Named".into();
        let bytes = b.to_bytes().unwrap();
        let back = Book::parse(&bytes).unwrap();
        assert_eq!(back, b);
        assert_eq!(back.sheets[0].rows(), 5);
        assert_eq!(back.sheets[0].cols(), 4);
        assert_eq!(back.sheets[2].cells[&(1, 1)].kind(), 4);
    }

    #[test]
    fn a_long_string_table_goes_on_in_continue_records() {
        let mut b = Book::default();
        for r in 0..3000u32 {
            let s = if r % 2 == 0 { format!("text number {r} with some length") } else { format!("Ж {r}") };
            b.sheet_mut(0).cells.insert((r, 0), Cell::of_text(&s));
        }
        // One string far longer than a record.
        b.sheet_mut(0).cells.insert((0, 1), Cell::Str("x".repeat(20000)));
        let back = Book::parse(&b.to_bytes().unwrap()).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn rk_numbers() {
        assert_eq!(rk((5 << 2) | 2), Cell::Int(5));
        assert_eq!(rk(((-3i32 << 2) | 2) as u32), Cell::Int(-3));
        assert_eq!(rk((1234 << 2) | 3), Cell::Float(12.34));
        let x = (1.5f64.to_bits() >> 32) as u32;
        assert_eq!(rk(x), Cell::Float(1.5));
    }

    #[test]
    fn not_a_workbook() {
        assert_eq!(Book::parse(b"hello"), None);
        assert_eq!(Book::parse(&[0u8; 2048]), None);
    }
}
