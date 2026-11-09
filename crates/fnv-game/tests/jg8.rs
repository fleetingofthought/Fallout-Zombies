//! JohnnyGuitar NVSE batch 8 (#1007; docs/spec-notes/ext-jg8.md) on a synthetic plugin:
//! the event handlers fired by the game, file hashes, texture and bitmap headers, arrays,
//! the system colours and the sleep/wait menu's state.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::nvse_jg8::{bmp_pixel, dds_header, md5, sha1};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const GLOB: u32 = 0x900;
const QUST: u32 = 0x901;
const REPU: u32 = 0x902;
const MISC_A: u32 = 0x903;
const MISC_B: u32 = 0x904;

fn edid(sig: &[u8; 4], fid: u32, name: &str) -> Vec<u8> {
    record(sig, 0, fid, &[sub(b"EDID", z(name))])
}

fn plugin_bytes() -> Vec<u8> {
    let glob = record(b"GLOB", 0, GLOB, &[sub(b"EDID", z("TestGlobal")), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    [
        tes4(),
        top(b"NPC_", &edid(b"NPC_", 0x7, "Player")),
        top(b"GLOB", &glob),
        top(b"QUST", &edid(b"QUST", QUST, "TestQuest")),
        top(b"REPU", &edid(b"REPU", REPU, "TestReputation")),
        top(b"MISC", &[edid(b"MISC", MISC_A, "ZuluThing"), edid(b"MISC", MISC_B, "AlphaThing")].concat()),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Jg8.esm".into()).unwrap()];
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

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() })
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(g: &mut Game, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        XVal::Ref(r) => f64::from(r),
        v => panic!("{expr}: {v:?}"),
    }
}

fn s(g: &mut Game, body: &str) -> String {
    match q(g, body) {
        XVal::Str(s) => s,
        v => panic!("{body}: {v:?}"),
    }
}

/// TestGlobal after the event: each handler adds what it was given.
fn global(g: &mut Game) -> f64 {
    f64::from(g.world.global(FormId(GLOB)).unwrap())
}

#[test]
fn handlers_hear_the_game() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: SetJohnnyOnFailQuestEventHandler, the quest as the argument; a filter
    // that does not match keeps the handler quiet].
    q(&mut g, "SetJohnnyOnFailQuestEventHandler 1 (begin function {ref rQ}\nif rQ == TestQuest\nlet TestGlobal := TestGlobal + 1\nendif\nend) 0 TestQuest");
    q(&mut g, "SetJohnnyOnFailQuestEventHandler 1 (begin function {ref rQ}\nlet TestGlobal := TestGlobal + 100\nend) 0 TestReputation");
    q(&mut g, "FailQuest TestQuest");
    assert_eq!(global(&mut g), 1.0);
    // [documented: SetJohnnyOnSleepWaitEventHandler, 0 both, 1 wait, 2 rest].
    q(&mut g, "let TestGlobal := 0\nSetJohnnyOnSleepWaitEventHandler 1 (begin function {int iMode}\nlet TestGlobal := TestGlobal + iMode\nend) 0 0");
    q(&mut g, "SetJohnnyOnSleepWaitEventHandler 1 (begin function {int iMode}\nlet TestGlobal := TestGlobal + 10\nend) 0 2");
    g.begin_rest(false, 2);
    assert_eq!(global(&mut g), 1.0);
    g.begin_rest(true, 8);
    assert_eq!(global(&mut g), 1.0 + 2.0 + 10.0);
    // [documented: SetJohnnyOnLimbGoneEventHandler, (actor, limb), the limb filter].
    q(&mut g, "let TestGlobal := 0\nSetJohnnyOnLimbGoneEventHandler 1 (begin function {ref rA, int iLimb}\nlet TestGlobal := TestGlobal + iLimb\nend) 0 0 1");
    g.dismember(FormId::PLAYER_REF, &[(1, false), (3, false)], None);
    assert_eq!(global(&mut g), 1.0);
    // [hypothesis: SetOnReputationChangeEventHandler's (reputation, axis, value)].
    q(&mut g, "let TestGlobal := 0\nSetOnReputationChangeEventHandler 1 (begin function {ref rR, int iAxis, float fV}\nlet TestGlobal := fV\nend) 0 TestReputation");
    fnv_game::functions::reputation_store(&mut g, FormId(REPU), 0, 5.0);
    assert_eq!(global(&mut g), 5.0);
    // Removing it: `0 function flag filters` as set.
    q(&mut g, "let TestGlobal := 0");
    let removed = g.other.jg8.handlers.len();
    q(&mut g, "SetJohnnyOnFailQuestEventHandler 0 (begin function {ref rQ}\nend) 0 TestQuest");
    assert_eq!(g.other.jg8.handlers.len(), removed, "another function's removal leaves the handler");
}

#[test]
fn files_and_headers() {
    // [documented: RFC 1321 and RFC 3174 test vectors].
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    assert_eq!(hex(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(hex(&sha1(&[b'a'; 1000])), "291e9a6c66994949b57ba5e650361e98fc36b1ba");
    // [documented: dds.md §1, height at 12, width 16, mip count 28, FourCC 84].
    let mut dds = vec![0u8; 128];
    dds[..4].copy_from_slice(b"DDS ");
    dds[12..16].copy_from_slice(&256u32.to_le_bytes());
    dds[16..20].copy_from_slice(&512u32.to_le_bytes());
    dds[28..32].copy_from_slice(&10u32.to_le_bytes());
    dds[84..88].copy_from_slice(b"DXT5");
    assert_eq!(dds_header(&dds), Some((512, 256, 10, *b"DXT5")));
    // A 2x2 24-bit bitmap, bottom-up rows padded to 8 bytes: top-left red, bottom-right blue.
    let mut bmp = vec![0u8; 54];
    bmp[..2].copy_from_slice(b"BM");
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&2u32.to_le_bytes());
    bmp[22..26].copy_from_slice(&2u32.to_le_bytes());
    bmp[28] = 24;
    bmp.extend_from_slice(&[0, 0, 0, 255, 0, 0, 0, 0]); // bottom row: black, blue
    bmp.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]); // top row: red, green
    assert_eq!(bmp_pixel(&bmp, 0, 0), Some([255, 0, 0]));
    assert_eq!(bmp_pixel(&bmp, 1, 0), Some([0, 255, 0]));
    assert_eq!(bmp_pixel(&bmp, 1, 1), Some([0, 0, 255]));
    assert_eq!(bmp_pixel(&bmp, 2, 0), None);
}

#[test]
fn arrays_colours_and_menus() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: Ar_SortEditor by editor ID, reverse with 1].
    assert_eq!(s(&mut g, "array_var a\nlet a := Ar_SortEditor (Ar_List ZuluThing, AlphaThing)\nSetFunctionValue (GetEditorID a[0])"), "AlphaThing");
    assert_eq!(s(&mut g, "array_var a\nlet a := Ar_SortEditor (Ar_List AlphaThing, ZuluThing) 1\nSetFunctionValue (GetEditorID a[0])"), "ZuluThing");
    // [documented: Ar_Shuffle keeps the elements].
    assert_eq!(n(&mut g, "Ar_Size (Ar_Shuffle (Ar_List 1, 2, 3, 4, 5))"), 5.0);
    assert_eq!(q(&mut g, "array_var a\nlet a := Ar_Shuffle (Ar_List 1, 2, 3, 4, 5)\nSetFunctionValue (a[0] + a[1] + a[2] + a[3] + a[4])"), XVal::Num(15.0));
    // [documented: GetSystemColor / GetSystemColorAlt, SetSystemColor's colour, amber
    // before].
    // [observed: probe set jg8, 1/4/5 FFB642, 2 FF432A, 3 21E779].
    assert_eq!(n(&mut g, "GetSystemColor 1"), f64::from(0xFFB642u32));
    assert_eq!(n(&mut g, "GetSystemColor 2"), f64::from(0xFF432Au32));
    assert_eq!(n(&mut g, "GetSystemColor 3"), f64::from(0x21E779u32));
    q(&mut g, "SetSystemColor 4 10 20 30");
    assert_eq!(s(&mut g, "int r\nint gg\nint b\nGetSystemColorAlt 4 r gg b\nSetFunctionValue (sv_Construct \"%g %g %g\" r gg b)"), "10 20 30");
    assert_eq!(n(&mut g, "GetSystemColor 6"), 0.0);
    // [documented: GetSleepWaitMenuState 0 closed, 1 wait, 2 sleep].
    assert_eq!(n(&mut g, "GetSleepWaitMenuState"), 0.0);
    g.other.jg8.sleep_menu = true;
    g.open_menu(1012);
    assert_eq!(n(&mut g, "GetSleepWaitMenuState"), 2.0);
    // Nothing to clear yet; IsRadioRefPlaying reads 0 (the page: it crashes the game).
    assert_eq!(n(&mut g, "ClearCustomMapMarker"), 0.0);
    assert_eq!(n(&mut g, "IsBSALoaded \"Fallout - Meshes.bsa\""), 0.0);
}
