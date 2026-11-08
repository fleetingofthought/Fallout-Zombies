//! SUP NVSE batch 5 (#1008; docs/spec-notes/ext-sup5.md) on a synthetic plugin, called
//! from scripts: Excel workbooks, JSON files, file sizes and times, the load-order dump,
//! screenshots, QR codes and the nothing values of the network, the desktop and the
//! clipboard, on a made-up game folder with NVForever's layer beside it. Its own test
//! binary: the file roots are global to the process.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;
use std::path::PathBuf;

const CELL: u32 = 0x200;
const NPC: u32 = 0x900;
const MARK: u32 = 0x901;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn plugin_bytes() -> Vec<u8> {
    let npcs = [record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), record(b"NPC_", 0, NPC, &[sub(b"EDID", z("TestNpc")), sub(b"FULL", z("Tester"))])].concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let data = [f32b(100.0), f32b(200.0), f32b(0.0), vec![0; 12]].concat();
    let refs = record(b"ACHR", 0, MARK, &[sub(b"EDID", z("MarkRef")), sub(b"NAME", NPC.to_le_bytes().to_vec()), sub(b"DATA", data)]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [tes4(), top(b"NPC_", &npcs), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat())))].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Sup5.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

/// The made-up game folder and NVForever's layer, set once for the process.
fn folders() -> (PathBuf, PathBuf) {
    static DIRS: std::sync::OnceLock<(PathBuf, PathBuf)> = std::sync::OnceLock::new();
    DIRS.get_or_init(|| {
        use fnv_game::mod_files::{set_roots, Roots};
        let base = std::env::temp_dir().join(format!("nvf-sup5-{}", std::process::id()));
        let (game, layer) = (base.join("game"), base.join("layer"));
        std::fs::create_dir_all(game.join("Data")).unwrap();
        std::fs::create_dir_all(&layer).unwrap();
        std::fs::write(game.join("nvac.log"), b"0123456789").unwrap();
        std::fs::write(game.join("good.json"), br#"{"name":"Joe","level":7,"inv":[1,"two"],"pos":{"x":1.5}}"#).unwrap();
        std::fs::write(game.join("bad.json"), b"{ nope").unwrap();
        std::fs::write(game.join("notjson.txt"), b"{}").unwrap();
        set_roots(Roots { read: vec![game.join("Data")], write: Some(layer.clone()), ..Roots::default() });
        (game, layer)
    })
    .clone()
}

fn game(f: &Fixture) -> Game<'_> {
    folders();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g
}

const VARS: &str = "float fA\nstring_var sOut\narray_var aList\narray_var aIn\n";

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("{VARS}Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(g: &mut Game, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        v => panic!("{expr}: {v:?}"),
    }
}

fn s(g: &mut Game, expr: &str) -> String {
    match q(g, &format!("let sOut := {expr}\nSetFunctionValue sOut")) {
        XVal::Str(s) => s,
        v => panic!("{expr}: {v:?}"),
    }
}

#[test]
fn excel_cells_sheets_and_arrays() {
    let f = fixture();
    let mut g = game(&f);
    let (game_dir, layer) = folders();
    // A missing file: -1 [documented: every Excel page's note].
    assert_eq!(n(&mut g, "ExcelReadFloat \"xlNone\" 0 0 0"), -1.0);
    assert_eq!(n(&mut g, "ExcelGetWorkSheetCount \"xlNone\""), -1.0);
    // Writing makes the file and the sheet; 1 when written [documented: ExcelWriteFloat].
    assert_eq!(n(&mut g, "ExcelWriteFloat \"xlBook\" 0 0 0 10"), 1.0);
    assert_eq!(n(&mut g, "XLSWriteString \"xlBook\" 0 4 1 \"MyText\""), 1.0);
    assert_eq!(n(&mut g, "ExcelReadFloat \"xlBook\" 0 0 0"), 10.0);
    assert_eq!(s(&mut g, "ExcelReadString \"xlBook\" 0 4 1"), "MyText");
    // Types [documented: ExcelGetCellValueType]: a float 2, text 3, empty 0.
    assert_eq!(n(&mut g, "ExcelGetCellValueType \"xlBook\" 0 0 0"), 2.0);
    assert_eq!(n(&mut g, "ExcelGetCellValueType \"xlBook\" 0 4 1"), 3.0);
    assert_eq!(n(&mut g, "ExcelGetCellValueType \"xlBook\" 0 1 1"), 0.0);
    assert_eq!(n(&mut g, "ExcelGetRowCount \"xlBook\" 0"), 5.0);
    assert_eq!(n(&mut g, "ExcelGetColCount \"xlBook\" 0"), 2.0);
    // A third sheet makes the second too [hypothesis].
    assert_eq!(n(&mut g, "ExcelWriteFloat \"xlBook\" 2 0 0 1.5"), 1.0);
    assert_eq!(n(&mut g, "ExcelGetWorkSheetCount \"xlBook\""), 3.0);
    assert_eq!(s(&mut g, "ExcelSetSheetName \"xlBook\" 1"), "Sheet2");
    // Names cut to 31 [documented: ExcelSetSheetName].
    assert_eq!(s(&mut g, "ExcelSetSheetName \"xlBook\" 1 \"A name far longer than thirty-one letters\""), "A name far longer than thirty-o");
    assert_eq!(s(&mut g, "ExcelSetSheetName \"xlBook\" 1"), "A name far longer than thirty-o");
    // Erasing [documented: ExcelEraseCell, ExcelEraseSheet].
    assert_eq!(n(&mut g, "ExcelEraseCell \"xlBook\" 0 4 1"), 1.0);
    assert_eq!(n(&mut g, "ExcelGetCellValueType \"xlBook\" 0 4 1"), 0.0);
    assert_eq!(n(&mut g, "ExcelEraseSheet \"xlBook\" 2"), 1.0);
    assert_eq!(n(&mut g, "ExcelGetRowCount \"xlBook\" 2"), 0.0);
    assert_eq!(n(&mut g, "ExcelGetWorkSheetCount \"xlBook\""), 3.0);
    // The file is a workbook in NVForever's layer, not in the game folder.
    let bytes = std::fs::read(layer.join("GameRoot").join("xlBook.xls")).unwrap();
    assert_eq!(&bytes[..4], &[0xD0, 0xCF, 0x11, 0xE0]);
    assert!(!game_dir.join("xlBook.xls").exists());
    // An array down a column, a form as its name [documented: ExcelArrayToFile].
    q(&mut g, "let aIn := ar_Construct \"array\"\nar_Append aIn 2\nar_Append aIn \"MyString\"\nar_Append aIn 3.5\nar_Append aIn MarkRef\nExcelArrayToFile \"xlArr\" 0 0 0 aIn");
    assert_eq!(s(&mut g, "ExcelReadString \"xlArr\" 0 3 0"), "Tester");
    assert_eq!(n(&mut g, "ExcelReadFloat \"xlArr\" 0 2 0"), 3.5);
    // Along a row with 1.
    q(&mut g, "let aIn := ar_Construct \"array\"\nar_Append aIn 7\nar_Append aIn 8\nExcelArrayToFile \"xlArr\" 1 0 0 aIn 1");
    assert_eq!(n(&mut g, "ExcelReadFloat \"xlArr\" 1 0 1"), 8.0);
    // Read back, `*Plugin:hex` as the form [documented: ExcelArrayFromFile].
    q(&mut g, "ExcelWriteString \"xlArr\" 0 4 0 \"*Sup5.esm:000900\"");
    let out = s(&mut g, "\"\"\nlet aList := ExcelArrayFromFile \"xlArr\" 0 0 0\nlet sOut := sv_Construct \"%g %g %z %g %z\" (ar_Size aList) (aList[0]) (aList[1]) (aList[2]) (aList[3])");
    assert_eq!(out, "5 2 MyString 3.5 Tester");
    assert_eq!(n(&mut g, "IsFormValid (ExcelArrayFromFile \"xlArr\" 0 0 0)[4]"), 1.0);
    let row = s(&mut g, "\"\"\nlet aList := ExcelArrayFromFile \"xlArr\" 1 0 1\nlet sOut := sv_Construct \"%g %g %g\" (ar_Size aList) (aList[0]) (aList[1])");
    assert_eq!(row, "2 7 8");
    // A file that is no workbook is not overwritten: -1 [documented].
    std::fs::write(game_dir.join("xlText.xls"), b"plain text").unwrap();
    assert_eq!(n(&mut g, "ExcelWriteFloat \"xlText\" 0 0 0 1"), -1.0);
    assert_eq!(std::fs::read(game_dir.join("xlText.xls")).unwrap(), b"plain text");
}

#[test]
fn json_files() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: JSONIsFileValid's table].
    assert_eq!(n(&mut g, "JSONIsFileValid \"good.json\""), 1.0);
    assert_eq!(n(&mut g, "JSONIsFileValid \"bad.json\""), -3.0);
    assert_eq!(n(&mut g, "JSONIsFileValid \"none.json\""), -4.0);
    assert_eq!(n(&mut g, "JSONIsFileValid \"../good.json\""), -5.0);
    assert_eq!(n(&mut g, "JSONIsFileValid \"notjson.txt\""), -6.0);
    // A value in a list, a list as itself, an object as a map [hypothesis].
    let one = s(&mut g, "\"\"\nlet aList := JSONGetValue \"good.json\" \"name\" 0\nlet sOut := sv_Construct \"%g %z\" (ar_Size aList) (aList[0])");
    assert_eq!(one, "1 Joe");
    let inv = s(&mut g, "\"\"\nlet aList := JSONGetValue \"good.json\" \"inv\" 1\nlet sOut := sv_Construct \"%g %g %z\" (ar_Size aList) (aList[0]) (aList[1])");
    assert_eq!(inv, "2 1 two");
    assert_eq!(n(&mut g, "(JSONGetValue \"good.json\" \"pos\" 0)[\"x\"]"), 1.5);
    assert_eq!(n(&mut g, "(JSONGetValue \"good.json\" \"pos.x\" 0)[0]"), 1.5);
    assert_eq!(n(&mut g, "ar_Size (JSONGetValue \"good.json\" \"nope\" 0)"), -1.0);
}

#[test]
fn file_sizes_times_and_the_load_order() {
    let f = fixture();
    let mut g = game(&f);
    let (game_dir, layer) = folders();
    // [documented: GetFileSize, in bytes from the game folder]; missing -1 [hypothesis].
    assert_eq!(n(&mut g, "GetFileSize \"nvac.log\""), 10.0);
    assert_eq!(n(&mut g, "GetFileSize \"none.log\""), -1.0);
    // "DD/MM/YY HH:MM" [documented: GetFileTimeSTR].
    let t = s(&mut g, "GetFileTimeSTR \"nvac.log\" 2 0");
    assert_eq!(t.len(), 14, "{t}");
    assert_eq!((&t[2..3], &t[5..6], &t[8..9], &t[11..12]), ("/", "/", " ", ":"));
    assert_eq!(s(&mut g, "GetFileTimeSTR \"none.log\" 2 0"), "");
    assert_eq!(n(&mut g, "ar_Size (GetFileTime \"nvac.log\" 2)"), 6.0);
    assert!(n(&mut g, "(GetFileTime \"nvac.log\" 2)[0]") >= 2026.0);
    // The load order into NVForever's SUPNVSE.log [documented: DumpLoadOrder].
    q(&mut g, "DumpLoadOrder");
    let log = std::fs::read_to_string(layer.join("GameRoot").join("SUPNVSE.log")).unwrap();
    assert!(log.contains("00 Sup5.esm"), "{log}");
    assert!(!game_dir.join("SUPNVSE.log").exists());
}

#[test]
fn the_network_the_desktop_and_the_clipboard_give_nothing() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: the pages' failure text; decision: no network].
    assert_eq!(s(&mut g, "GetNexusModVersion 73160"), "ERROR");
    assert_eq!(s(&mut g, "GetNexusModVersionAPI 73160"), "ERROR");
    assert_eq!(s(&mut g, "GetGitHubLatestReleaseTag \"x/y\""), "ERROR");
    let multi = s(&mut g, "\"\"\nlet aIn := ar_Construct \"array\"\nar_Append aIn 73160\nar_Append aIn 58277\nlet aList := GetNexusModVersionMulti aIn\nlet sOut := sv_Construct \"%g %z\" (ar_Size aList) (aList[1])");
    assert_eq!(multi, "2 ERROR");
    assert_eq!(n(&mut g, "GetNVSEOnlineVersion"), 0.0);
    assert_eq!(n(&mut g, "IsNexusAPIKeyValid"), 0.0);
    assert_eq!(s(&mut g, "GetPastebinRawTextString \"j1SY2TFH\""), "");
    assert_eq!(n(&mut g, "ar_Size (GetPastebinRawTextArray \"j1SY2TFH\")"), 0.0);
    // [decision: no programs, folders or pages opened].
    assert_eq!(n(&mut g, "IsProgramRunning \"steam.exe\""), 0.0);
    q(&mut g, "OpenNexusModPage 73160\nOpenNexusUserPage 1\nOpenFolder \"C:\\\\\"");
    // [decision: no clipboard, fonts or language].
    q(&mut g, "StringToClipboard \"x\"");
    assert_eq!(s(&mut g, "ClipboardToString"), "");
    assert_eq!(s(&mut g, "GetUserWindowsLanguage"), "");
    assert_eq!(n(&mut g, "ar_Size (GetWindowsFonts)"), 0.0);
}

#[test]
fn screenshots_and_qr_codes() {
    let f = fixture();
    let mut g = game(&f);
    let (_, layer) = folders();
    // Requests for the engine [documented: CaptureScreenshot's default name and formats].
    q(&mut g, "Screenshot\nCaptureScreenshot \"MyScreen\" 1\nCaptureScreenshotAlt \"MyMod\" \"MyImage\" 0 1000 0 1200 6 0 50");
    let shots: Vec<(String, bool, u8)> = g.ui.screenshots.iter().map(|r| (r.path.clone(), r.root, r.format)).collect();
    assert_eq!(shots, vec![("a_Screenshot.jpg".to_string(), true, 0), ("MyScreen.bmp".to_string(), true, 1), ("textures\\SUPScreenshots\\MyMod\\MyImage.dds".to_string(), false, 6)]);
    assert_eq!(g.ui.screenshots[2].crop, Some([0.0, 1000.0, 0.0, 1200.0]));
    // Deleting [documented: DeleteScreenshot, -1 missing, -2 outside its folder].
    assert_eq!(n(&mut g, "DeleteScreenshot \"MyMod\" \"MyImage.dds\""), -1.0);
    assert_eq!(n(&mut g, "DeleteScreenshot \"..\\\\x\" \"MyImage.dds\""), -2.0);
    assert!(g.mod_files.write_bytes("textures\\SUPScreenshots\\MyMod\\MyImage.dds", b"DDS "));
    assert!(layer.join("textures/SUPScreenshots/MyMod/MyImage.dds").exists());
    assert_eq!(n(&mut g, "DeleteScreenshot \"MyMod\" \"MyImage.dds\""), 1.0);
    assert!(!layer.join("textures/SUPScreenshots/MyMod/MyImage.dds").exists());
    // A QR code as a PNG, the folder made [documented: TextToQRCode].
    q(&mut g, "TextToQRCode \"QrCodes\\MyQrCode\" \"I love bacon\" 0");
    let png = std::fs::read(layer.join("GameRoot/QrCodes/MyQrCode.png")).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    // Nothing drawn [decision].
    q(&mut g, "\"\"\nlet aIn := ar_Construct \"array\"\nDrawGraphicsToDDSTextureAndSave \"MyMod\" \"T.dds\" aIn 500 500");
    // No dialog: "" [decision].
    assert_eq!(s(&mut g, "OpenFolderDialogBox MarkRef"), "");
}
