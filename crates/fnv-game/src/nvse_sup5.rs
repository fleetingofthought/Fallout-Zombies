//! SUP NVSE, batch 5 (#1008; docs/spec-notes/ext-sup5.md): the files and the machine.
//! Excel workbooks (`crate::xls`) and JSON files under the game's folder, file sizes and
//! times, the load-order dump, screenshots and QR codes (`crate::image_files`; the engine
//! takes the screenshots, `nvf_play::sup_shots`), and the functions of the network, the
//! desktop and the clipboard, which give their pages' nothing values (owner decisions of
//! 2026-11-08). Every file is read through `crate::mod_files`: NVForever's own layer
//! first, then the game's folder; every write goes into the layer, never the game folder.

use crate::game::{Game, RunCtx};
use crate::xls::{Book, Cell, MAX_SHEET_NAME};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k)? {
        XVal::Str(s) => Some(s.clone()),
        XVal::Num(x) => Some(format!("{x}")),
        _ => None,
    }
}

fn list(g: &mut Game, items: Vec<XVal>) -> XVal {
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

/// A path a script names, `/` as `\`; `None` when it could leave the folder it is under
/// (a root, a drive, `..`) [decision: the charter, as `mod_files::on_disk`].
fn safe_path(p: &str) -> Option<String> {
    let p = p.trim().replace('/', "\\");
    let bad = p.is_empty() || p.starts_with('\\') || p.contains(':') || p.split('\\').any(|c| c == ".." || c == ".");
    (!bad).then_some(p)
}

/// The workbook's path: under the game's folder, `.xls` added [documented: ExcelWriteFloat,
/// "File name is specified without extension", the examples' "aaExcelFile.xls";
/// hypothesis: the game's folder, as SUP's INI readers, and a name already ending in
/// `.xls` kept].
fn xls_path(args: &[XVal]) -> Option<String> {
    let p = safe_path(&text(args, 0)?)?;
    Some(if p.to_ascii_lowercase().ends_with(".xls") { p } else { format!("{p}.xls") })
}

/// The workbook, `Err` when the file is missing or no workbook (the pages' -1).
fn load_book(g: &Game, path: &str) -> Result<Book, ()> {
    g.mod_files.read_root_bytes(path).and_then(|b| Book::parse(&b)).ok_or(())
}

/// The workbook to write into: a missing file starts empty [documented: "if file doesn't
/// exists - it will be created"]; a file that is no workbook is not overwritten (-1)
/// [documented: "if file can't be loaded … returns -1"].
fn book_for_write(g: &Game, path: &str) -> Result<Book, ()> {
    match g.mod_files.read_root_bytes(path) {
        None => Ok(Book::default()),
        Some(b) => Book::parse(&b).ok_or(()),
    }
}

fn save_book(g: &mut Game, path: &str, b: &Book) -> bool {
    b.to_bytes().is_some_and(|bytes| g.mod_files.write_root_bytes(path, &bytes))
}

/// A sheet, row and column index: none below 0, none past the format's 65536 rows and
/// 256 columns [documented: MS-XLS limits; hypothesis: refused as a failed load, -1].
fn index(args: &[XVal], k: usize) -> Option<usize> {
    let v = num(args, k, -1.0);
    (v >= 0.0 && v.is_finite()).then_some(v as usize)
}

fn cell_at(args: &[XVal], k: usize) -> Option<(u32, u32)> {
    let (r, c) = (index(args, k)?, index(args, k + 1)?);
    (r < 65536 && c < 256).then_some((r as u32, c as u32))
}

/// A form's shown name (its base's FULL), as GetName.
fn name_of(g: &Game, f: FormId) -> String {
    let b = crate::functions::base_of(g, f).filter(|&b| b != f).unwrap_or(f);
    g.name_overrides.get(&b).or_else(|| g.data.full_names.get(&g.unclone(b))).cloned().unwrap_or_default()
}

/// A cell's text as ExcelArrayFromFile gives it: `*Plugin.esm:hex` is that plugin's form
/// [documented: ExcelArrayFromFile, "All to-build references must be prefixed with '*'"];
/// anything else stays text [hypothesis: an unknown plugin or form too].
fn form_text(g: &Game, s: &str) -> XVal {
    let parsed = s.strip_prefix('*').and_then(|r| r.rsplit_once(':')).and_then(|(plugin, hex)| {
        let i = g.data.load_order.iter().position(|p| p.eq_ignore_ascii_case(plugin.trim()))?;
        let local = u32::from_str_radix(hex.trim(), 16).ok()? & 0x00FF_FFFF;
        let id = ((i as u32) << 24) | local;
        (g.data.sig(FormId(id)).is_some() || id == FormId::PLAYER_REF.0).then_some(id)
    });
    match parsed {
        Some(id) => XVal::Ref(id),
        None => XVal::Str(s.to_string()),
    }
}

/// A JSON value as a script value: numbers, text, true/false as 1/0, null as 0, arrays as
/// lists and objects as string maps [hypothesis: the article the page points to is not in
/// the export].
fn json_value(g: &mut Game, v: &serde_json::Value) -> XVal {
    use serde_json::Value as J;
    match v {
        J::Null => XVal::Num(0.0),
        J::Bool(b) => XVal::Num(f64::from(u8::from(*b))),
        J::Number(n) => XVal::Num(n.as_f64().unwrap_or(0.0)),
        J::String(s) => XVal::Str(s.clone()),
        J::Array(a) => {
            let items = a.iter().map(|x| json_value(g, x)).collect();
            list(g, items)
        }
        J::Object(o) => {
            let entries = o.iter().map(|(k, x)| (XKey::Str(k.clone()), json_value(g, x))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
        }
    }
}

/// The value under `key`: the key whole, else a path of keys and list indexes split at
/// `.` or `/` [hypothesis].
fn json_lookup<'a>(root: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if let Some(v) = root.get(key) {
        return Some(v);
    }
    key.split(['.', '/']).filter(|p| !p.is_empty()).try_fold(root, |v, part| match v {
        serde_json::Value::Array(a) => part.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => v.get(part),
    })
}

/// JSONIsFileValid's answer for a path [documented: JSONIsFileValid's table]: -6 not a
/// `.json` file, -5 outside the game folder, -4 missing, -3 not valid JSON, and the file.
fn json_file(g: &Game, path: &str) -> Result<serde_json::Value, i32> {
    if !path.trim().to_ascii_lowercase().ends_with(".json") {
        return Err(-6);
    }
    let p = safe_path(path).ok_or(-5)?;
    let bytes = g.mod_files.read_root_bytes(&p).ok_or(-4)?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&bytes);
    serde_json::from_slice(bytes).map_err(|_| -3)
}

/// A file's time: 0 created, 1 last read, 2 last written [documented: GetFileTime].
fn file_time(g: &Game, path: &str, request: f64) -> Option<chrono::DateTime<chrono::Local>> {
    let (_, disk) = g.mod_files.root_file(&safe_path(path)?)?;
    let m = std::fs::metadata(disk?).ok()?;
    let t = match request as i32 {
        0 => m.created(),
        1 => m.accessed(),
        2 => m.modified(),
        _ => return None,
    }
    .ok()?;
    Some(t.into())
}

pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let _ = subject;
    let zero = XVal::Num(0.0);
    let minus = XVal::Num(-1.0);
    let v = match name {
        // ---- Excel workbooks under the game's folder ----
        // `file sheet row col value`: 1 written, -1 when the file is no workbook; the
        // file and the sheet are made when missing [documented: ExcelWriteFloat,
        // ExcelWriteString]. A number is written as a float (type 2) [hypothesis].
        "excelwritefloat" | "excelwritestring" => {
            let (Some(path), Some(sheet), Some(at)) = (xls_path(args), index(args, 1), cell_at(args, 2)) else { return Some(Ok(minus)) };
            let Ok(mut b) = book_for_write(g, &path) else { return Some(Ok(minus)) };
            let cell = if name == "excelwritefloat" { Cell::Float(num(args, 4, 0.0)) } else { Cell::of_text(&text(args, 4).unwrap_or_default()) };
            b.sheet_mut(sheet).cells.insert(at, cell);
            XVal::Num(if save_book(g, &path, &b) { 1.0 } else { -1.0 })
        }
        // `file sheet row col`: the number in the cell, the text [documented:
        // ExcelReadFloat, ExcelReadString]; -1 for a missing file [documented], the
        // number -1 from ReadString too [observed: sup5 xl.missing.string, a runtime
        // error where `%z` was given it]; an empty cell 0 and "" [hypothesis], a text
        // cell's number 0 and a number cell's text "" [observed: sup5 xl.cross].
        "excelreadfloat" | "excelreadstring" | "excelgetcellvaluetype" => {
            let book = xls_path(args).ok_or(()).and_then(|p| load_book(g, &p));
            let cell = book.as_ref().ok().and_then(|b| b.sheets.get(index(args, 1)?)).and_then(|s| s.cells.get(&cell_at(args, 2)?));
            match (name, &book, cell) {
                (_, Err(()), _) => minus,
                ("excelreadstring", _, c) => XVal::Str(c.map(Cell::text).unwrap_or_default()),
                ("excelreadfloat", _, c) => XVal::Num(c.map_or(0.0, Cell::number)),
                // [documented: ExcelGetCellValueType, 0 UNDEFINED for an empty cell].
                (_, _, c) => XVal::Num(f64::from(c.map_or(0, Cell::kind))),
            }
        }
        // `file`: the sheets [documented: ExcelGetWorkSheetCount]; `file sheet`: the rows
        // and columns in use, -1 for a missing file [documented] or sheet [hypothesis].
        "excelgetworksheetcount" => match xls_path(args).ok_or(()).and_then(|p| load_book(g, &p)) {
            Ok(b) => XVal::Num(b.sheets.len() as f64),
            Err(()) => minus,
        },
        "excelgetrowcount" | "excelgetcolcount" => {
            let book = xls_path(args).ok_or(()).and_then(|p| load_book(g, &p));
            match book.ok().as_ref().and_then(|b| b.sheets.get(index(args, 1)?)) {
                Some(s) => XVal::Num(f64::from(if name == "excelgetrowcount" { s.rows() } else { s.cols() })),
                None => minus,
            }
        }
        // `file sheet row col` / `file sheet`: the cell emptied, the sheet's cells all
        // emptied (the sheet kept) [documented: ExcelEraseCell; ExcelEraseSheet, "Will
        // erase all cells in Sheet"]; 1 done, -1 for a missing file [documented] or sheet
        // [hypothesis].
        "excelerasecell" | "excelerasesheet" => {
            let Some(path) = xls_path(args) else { return Some(Ok(minus)) };
            let Ok(mut b) = load_book(g, &path) else { return Some(Ok(minus)) };
            let Some(s) = index(args, 1).and_then(|i| b.sheets.get_mut(i)) else { return Some(Ok(minus)) };
            if name == "excelerasesheet" {
                s.cells.clear();
            } else if let Some(at) = cell_at(args, 2) {
                s.cells.remove(&at);
            }
            XVal::Num(if save_book(g, &path, &b) { 1.0 } else { -1.0 })
        }
        // `file sheet [name]`: renames the sheet, cut to 31 characters, and returns the
        // name; without a name returns it [documented: ExcelSetSheetName]. A missing file
        // gives the number -1 [documented]; a missing sheet too, and a rename returns a
        // number, 1 [hypothesis: sup5 xl.missing.sheet and xl.rename stopped at a runtime
        // error in the original where `%z` was given the return, as xl.missing.string].
        "excelsetsheetname" => {
            let Some(path) = xls_path(args) else { return Some(Ok(minus)) };
            let Ok(mut b) = load_book(g, &path) else { return Some(Ok(minus)) };
            let Some(i) = index(args, 1).filter(|&i| i < b.sheets.len()) else { return Some(Ok(minus)) };
            if let Some(n) = text(args, 2) {
                b.sheets[i].name = n.chars().take(MAX_SHEET_NAME).collect();
                return Some(Ok(XVal::Num(if save_book(g, &path, &b) { 1.0 } else { -1.0 })));
            }
            XVal::Str(b.sheets[i].name.clone())
        }
        // `file sheet startRow startCol array [horizontal]`: the array's values down a
        // column, or along a row; a form is written as its name [documented:
        // ExcelArrayToFile, PlayerRef as "Courier"]; nested arrays are passed over
        // [hypothesis]. 1 written, -1 not.
        "excelarraytofile" => {
            let (Some(path), Some(sheet), Some((r0, c0))) = (xls_path(args), index(args, 1), cell_at(args, 2)) else { return Some(Ok(minus)) };
            let Some(XVal::Array(id)) = args.get(4) else { return Some(Ok(minus)) };
            let vals: Vec<XVal> = g.ext.array(*id).map(|a| a.entries.iter().map(|(_, v)| v.clone()).collect()).unwrap_or_default();
            let horizontal = num(args, 5, 0.0) != 0.0;
            let Ok(mut b) = book_for_write(g, &path) else { return Some(Ok(minus)) };
            let cells: Vec<Option<Cell>> = vals
                .iter()
                .map(|v| match v {
                    XVal::Num(x) => Some(Cell::Float(*x)),
                    XVal::Str(s) => Some(Cell::of_text(s)),
                    XVal::Ref(r) => Some(Cell::of_text(&name_of(g, FormId(*r)))),
                    XVal::Array(_) => None,
                })
                .collect();
            let s = b.sheet_mut(sheet);
            for (k, c) in cells.into_iter().enumerate() {
                let at = if horizontal { (r0, c0 + k as u32) } else { (r0 + k as u32, c0) };
                if let (Some(c), true) = (c, at.0 < 65536 && at.1 < 256) {
                    s.cells.insert(at, c);
                }
            }
            XVal::Num(if save_book(g, &path, &b) { 1.0 } else { -1.0 })
        }
        // `file sheet n readRow`: column n's values (row n's with readRow 1), `*Plugin:hex`
        // as forms [documented: ExcelArrayFromFile]; empty cells passed over
        // [hypothesis]. A missing file gives no array [hypothesis: the page's -1 cannot be
        // an array].
        "excelarrayfromfile" => {
            let book = xls_path(args).ok_or(()).and_then(|p| load_book(g, &p));
            let (Ok(b), Some(sheet), Some(n)) = (book, index(args, 1), index(args, 2)) else { return Some(Ok(zero)) };
            let by_row = num(args, 3, 0.0) != 0.0;
            // The cells are kept by (row, column): down a column by row, along a row by
            // column.
            let cells: Vec<Cell> = b.sheets.get(sheet).map(|s| s.cells.iter().filter(|(&(r, c), _)| if by_row { r as usize == n } else { c as usize == n }).map(|(_, c)| c.clone()).collect()).unwrap_or_default();
            let items: Vec<XVal> = cells
                .iter()
                .map(|c| match c {
                    Cell::Str(s) | Cell::WStr(s) => form_text(g, s),
                    Cell::Formula(v) => match &**v {
                        Cell::Str(s) | Cell::WStr(s) => form_text(g, s),
                        v => XVal::Num(v.number()),
                    },
                    c => XVal::Num(c.number()),
                })
                .collect();
            list(g, items)
        }
        // ---- JSON files under the game's folder ----
        // [documented: JSONIsFileValid's table] 1 valid, -3 … -6.
        "jsonisfilevalid" => XVal::Num(f64::from(json_file(g, &text(args, 0).unwrap_or_default()).map_or_else(|e| e, |_| 1))),
        // `file key cached`: the key's value as an array [documented: JSONGetValue
        // "JSONValue:array_var"]: an object or list as itself, any other value in a
        // one-element list [hypothesis]. cached: the file read once and kept for later
        // cached calls [hypothesis: the parameter's name]. A missing file gives a
        // one-element list whatever the key [observed: sup5 json.name … json.missing,
        // size 1, its element read as empty text by `%z`: ""]; a missing key in a file
        // no array [hypothesis].
        "jsongetvalue" => {
            let path = text(args, 0).unwrap_or_default();
            let key = text(args, 1).unwrap_or_default();
            let cached = num(args, 2, 0.0) != 0.0;
            let ck = path.trim().replace('/', "\\").to_ascii_lowercase();
            let root = match g.mod_files.json.get(&ck).filter(|_| cached) {
                Some(v) => Some(v.clone()),
                None => json_file(g, &path).ok(),
            };
            let Some(root) = root else { return Some(Ok(list(g, vec![XVal::Str(String::new())]))) };
            if cached {
                g.mod_files.json.insert(ck, root.clone());
            }
            match json_lookup(&root, &key).cloned() {
                Some(v @ (serde_json::Value::Array(_) | serde_json::Value::Object(_))) => json_value(g, &v),
                Some(v) => {
                    let x = json_value(g, &v);
                    list(g, vec![x])
                }
                None => zero,
            }
        }
        // ---- file sizes and times, under the game's folder ----
        // [documented: GetFileSize, "relative to Fallout root folder", in bytes]; a
        // missing file -1 [hypothesis].
        "getfilesize" => XVal::Num(safe_path(&text(args, 0).unwrap_or_default()).and_then(|p| g.mod_files.root_file(&p)).map_or(-1.0, |(n, _)| n as f64)),
        // `file request`: [year, month, day, hour, minute, second] in local time
        // [documented: GetFileTime, an array; hypothesis: its fields]; a missing file, a
        // session-only write or a request outside 0-2 gives an empty array [hypothesis].
        "getfiletime" => {
            let t = file_time(g, &text(args, 0).unwrap_or_default(), num(args, 1, 0.0));
            use chrono::{Datelike, Timelike};
            let items = t.map(|t| [f64::from(t.year()), f64::from(t.month()), f64::from(t.day()), f64::from(t.hour()), f64::from(t.minute()), f64::from(t.second())].map(XVal::Num).to_vec()).unwrap_or_default();
            list(g, items)
        }
        // `file request type`: "DD/MM/YY HH:MM", or "MM/DD/YY HH:MM" with type 1
        // [documented: GetFileTimeSTR]; "" when there is no time [hypothesis].
        "getfiletimestr" => {
            let t = file_time(g, &text(args, 0).unwrap_or_default(), num(args, 1, 0.0));
            let fmt = if num(args, 2, 0.0) as i32 == 1 { "%m/%d/%y %H:%M" } else { "%d/%m/%y %H:%M" };
            XVal::Str(t.map(|t| t.format(fmt).to_string()).unwrap_or_default())
        }
        // The load order into SUPNVSE.log in the game's folder [documented: DumpLoadOrder]
        // (NVForever's copy of it, appended to this session's lines): one line a plugin,
        // its index in hex and its name [hypothesis: the page shows no line].
        "dumploadorder" => {
            let old = g.mod_files.read_root_text("SUPNVSE.log").unwrap_or_default();
            let mut out = old;
            for (i, p) in g.data.load_order.iter().enumerate() {
                out += &format!("{i:02X} {p}\r\n");
            }
            g.mod_files.write_root_text("SUPNVSE.log", out);
            zero
        }
        // ---- the network: nothing is sent or fetched [decision: owner 2026-11-08, no
        // network] ----
        // The pages' answer when the internet cannot be reached: "ERROR" [documented:
        // GetNexusModVersion, GetNexusModVersionAPI, GetNexusModVersionMulti (one a mod),
        // GetGitHubLatestReleaseTag].
        "getnexusmodversion" | "getnexusmodversionapi" | "getgithublatestreleasetag" => XVal::Str("ERROR".into()),
        "getnexusmodversionmulti" => {
            let n = match args.first() {
                Some(XVal::Array(id)) => g.ext.array(*id).map_or(0, |a| a.entries.len()),
                _ => 0,
            };
            list(g, vec![XVal::Str("ERROR".into()); n])
        }
        // No version, no valid key, no paste [decision: the nothing values].
        "getnvseonlineversion" | "isnexusapikeyvalid" => zero,
        "getpastebinrawtextstring" => XVal::Str(String::new()),
        "getpastebinrawtextarray" => list(g, Vec::new()),
        // ---- the desktop: no program is started, no folder, page or dialog opened
        // [decision: owner 2026-11-08] ----
        "opennexusmodpage" | "opennexususerpage" | "openfolder" => zero,
        // [decision: owner 2026-11-08] IsProgramRunning is always 0.
        "isprogramrunning" => zero,
        // `udf [extension]`: the handler is called next frame as when the player closes
        // the dialog without a choice, iSuccess 0 and an empty path [documented: the
        // pages' handler, "iSuccess will return 0 if file is not chosen by user";
        // decision: no dialog opens]; returns "".
        "openfiledialogbox" | "openfolderdialogbox" => {
            if let Some(f) = args.first().filter(|f| matches!(f, XVal::Ref(r) if *r != 0)) {
                crate::delayed::call_next_frame(g, f.clone(), ctx.self_ref, vec![XVal::Num(0.0), XVal::Str(String::new())]);
            }
            XVal::Str(String::new())
        }
        // ---- the clipboard and the computer's fonts and language: no channel to them
        // [decision: the engine leaves the clipboard out (nvforever main.rs disables
        // Bevy's ClipboardPlugin) and has no list of the system's fonts or its language;
        // follow-up in ext-sup5.md] ----
        "stringtoclipboard" => zero,
        "clipboardtostring" | "getuserwindowslanguage" => XVal::Str(String::new()),
        "getwindowsfonts" => list(g, Vec::new()),
        // ---- screenshots ----
        // `[file] [format]`: the screen into the game's folder, "a_Screenshot.jpg" by
        // default [documented: CaptureScreenshot]; the engine takes it at the end of the
        // frame (NVForever's copy of the game folder, `crate::image_files`).
        "capturescreenshot" => {
            let format = num(args, 1, 0.0).clamp(0.0, 255.0) as u8;
            let file = text(args, 0).filter(|f| !f.trim().is_empty()).unwrap_or_else(|| "a_Screenshot".into());
            if let Some(p) = safe_path(&file) {
                let path = format!("{p}.{}", crate::image_files::extension(format));
                g.ui.screenshots.push(crate::image_files::ShotRequest { path, root: true, format, crop: None, game_res: true });
            }
            zero
        }
        // `folder file x0 x1 y0 y1 format gameRes quality [elements]`: the part of the
        // screen into Data\textures\SUPScreenshots\<folder> [documented:
        // CaptureScreenshotAlt]. Quality and the drawn elements are not used [decision:
        // the elements' text needs the system's fonts; follow-up].
        "capturescreenshotalt" => {
            let format = num(args, 6, 0.0).clamp(0.0, 255.0) as u8;
            let (Some(folder), Some(file)) = (text(args, 0).and_then(|f| safe_path(&f)), text(args, 1).and_then(|f| safe_path(&f))) else { return Some(Ok(zero)) };
            let path = format!("textures\\SUPScreenshots\\{folder}\\{file}.{}", crate::image_files::extension(format));
            let crop = [num(args, 2, 0.0), num(args, 3, 0.0), num(args, 4, 0.0), num(args, 5, 0.0)].map(|v| v as f32);
            g.ui.screenshots.push(crate::image_files::ShotRequest { path, root: false, format, crop: Some(crop), game_res: num(args, 7, 0.0) != 0.0 });
            zero
        }
        // `folder file`: a screenshot of CaptureScreenshotAlt removed, the extension
        // given [documented: DeleteScreenshot]: -1 no such file, -2 a folder that leaves
        // SUPScreenshots [documented], 1 removed [hypothesis]. Only NVForever's layer
        // holds them.
        "deletescreenshot" => {
            let (Some(folder), Some(file)) = (text(args, 0).and_then(|f| safe_path(&f)), text(args, 1)) else { return Some(Ok(XVal::Num(-2.0))) };
            let Some(file) = safe_path(&file) else { return Some(Ok(XVal::Num(-2.0))) };
            let path = format!("textures\\SUPScreenshots\\{folder}\\{file}");
            XVal::Num(if g.mod_files.delete(&path) { 1.0 } else { -1.0 })
        }
        // [decision: drawing text in the system's fonts and pictures into a texture is a
        // design of its own; follow-up in ext-sup5.md] nothing is written.
        "drawgraphicstoddstextureandsave" => zero,
        // `file text [convert]`: a PNG of the text's QR code, "<file>.png", the folders
        // made [documented: TextToQRCode]; under the game's folder [observed: sup5
        // qr.where], as `image_files::qr_png`. The text is written as UTF-8 either way (NVForever's strings are
        // Unicode already).
        "texttoqrcode" => {
            let (Some(file), Some(t)) = (text(args, 0).and_then(|f| safe_path(&f)), text(args, 1)) else { return Some(Ok(zero)) };
            let path = if file.to_ascii_lowercase().ends_with(".png") { file } else { format!("{file}.png") };
            if let Some(bytes) = crate::image_files::qr_png(&t) {
                g.mod_files.write_root_bytes(&path, &bytes);
            }
            zero
        }
        _ => return None,
    };
    Some(Ok(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_under_their_folder() {
        assert_eq!(safe_path("a/b.xls").as_deref(), Some("a\\b.xls"));
        assert_eq!(safe_path("..\\x"), None);
        assert_eq!(safe_path("C:\\x"), None);
        assert_eq!(safe_path("\\x"), None);
        assert_eq!(safe_path(" "), None);
    }

    #[test]
    fn json_keys_and_paths() {
        let v: serde_json::Value = serde_json::from_str(r#"{"name":"Joe","a":{"b":[1,2,{"c":3}]},"x.y":5}"#).unwrap();
        assert_eq!(json_lookup(&v, "name"), Some(&serde_json::json!("Joe")));
        assert_eq!(json_lookup(&v, "a.b.2.c"), Some(&serde_json::json!(3)));
        assert_eq!(json_lookup(&v, "a/b/0"), Some(&serde_json::json!(1)));
        assert_eq!(json_lookup(&v, "x.y"), Some(&serde_json::json!(5)));
        assert_eq!(json_lookup(&v, "nope"), None);
    }
}
