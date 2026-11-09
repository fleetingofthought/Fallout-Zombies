//! SUP NVSE batch 4 (#1008; docs/spec-notes/ext-sup4.md) on a synthetic plugin, called
//! from scripts: random array keys, invalid references, floats from an array, a UDF some
//! frames later, the save names and sizes (the engine's part played by the test), the
//! cells near the player, the caught-pickpocketing flag and transliteration.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::nvse_sup4::SaveFile;
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC: u32 = 0x800;
const CELL: u32 = 0x900;
const NPC_REF: u32 = 0x910;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let acbs = [&0u32.to_le_bytes()[..], &[0; 4], &1u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat();
    let npcs = [record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), record(b"NPC_", 0, NPC, &[sub(b"EDID", z("TestMark")), sub(b"ACBS", acbs)])].concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestRoom")), sub(b"DATA", vec![1])]);
    let npc_ref = record(b"ACHR", 0, NPC_REF, &[sub(b"EDID", z("TestMarkRef")), sub(b"NAME", NPC.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &npc_ref));
    let bytes = [tes4(), top(b"NPC_", &npcs), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat())))].concat();
    let plugins = vec![Plugin::parse(&bytes, "Sup4Test.esm".into()).unwrap()];
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
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(CELL));
    g
}

const VARS: &str = "float fA\nfloat fB\nfloat fC\nref rNone\narray_var aR\narray_var aIn\nstring_var sOut\n";

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("{VARS}Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(g: &mut Game, expr: &str) -> f64 {
    let body = if expr.contains('\n') { expr.to_string() } else { format!("SetFunctionValue ({expr})") };
    match q(g, &body) {
        XVal::Num(x) => x,
        XVal::Ref(r) => f64::from(r),
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
fn arrays() {
    let f = fixture();
    let mut g = game(&f);
    for _ in 0..10 {
        let k = n(&mut g, "let aR := Ar_List 10, 20, 30, 40, 50\nSetFunctionValue (Ar_GetRandomKey aR)");
        assert!((0.0..5.0).contains(&k) && k.fract() == 0.0, "{k}");
        let m = s(&mut g, "Ar_GetRandomKeyMap (Ar_Map \"LALALA\"::1, \"TROLOLO\"::2)");
        assert!(m == "LALALA" || m == "TROLOLO", "{m}");
    }
    // An empty array 0 [observed: sup4 randomkey].
    assert_eq!(n(&mut g, "let aR := Ar_List\nSetFunctionValue (Ar_GetRandomKey aR)"), 0.0);
    // An empty reference is an invalid one; a dimension searches the inner array.
    assert_eq!(n(&mut g, "let aR := Ar_List PlayerREF, TestMarkRef\nSetFunctionValue (Ar_HasInvalidRefs aR)"), 0.0);
    assert_eq!(n(&mut g, "let aR := Ar_List PlayerREF, rNone\nSetFunctionValue (Ar_HasInvalidRefs aR)"), 1.0);
    assert_eq!(n(&mut g, "let aIn := Ar_List rNone\nlet aR := Ar_List PlayerREF, aIn\nSetFunctionValue (Ar_HasInvalidRefs aR)"), 0.0);
    assert_eq!(n(&mut g, "let aIn := Ar_List rNone\nlet aR := Ar_List PlayerREF, aIn\nSetFunctionValue (Ar_HasInvalidRefs aR 1)"), 1.0);
    // The floats from the array [documented: the page's example].
    assert_eq!(n(&mut g, "let aR := Ar_List 3, 8, 5, 1\nAr_SetFloatsFrom aR fA fB fC\nSetFunctionValue fA * 100 + fB * 10 + fC"), 385.0);
}

#[test]
fn a_function_frames_later() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "CallFunctionNextFrame (Begin Function { }\nPlayerREF.AuxVarSetFlt \"*sup4\" 1\nEnd) 3");
    for _ in 0..2 {
        g.tick();
    }
    assert_eq!(n(&mut g, "PlayerREF.AuxVarGetFlt \"*sup4\""), 0.0);
    g.tick();
    assert_eq!(n(&mut g, "PlayerREF.AuxVarGetFlt \"*sup4\""), 1.0);
}

#[test]
fn saves_cells_and_the_rest() {
    let f = fixture();
    let mut g = game(&f);
    // Before any save or load: -1 and "" [documented].
    assert_eq!(n(&mut g, "GetSavedSaveSize 0"), -1.0);
    assert_eq!(s(&mut g, "GetSaveName 1"), "");
    g.save_files.saved = Some(SaveFile { name: "Save 3".into(), bytes: 12345 });
    g.save_files.loaded = Some(SaveFile { name: "quicksave".into(), bytes: 999 });
    assert_eq!(n(&mut g, "GetSavedSaveSize 0"), 12345.0);
    assert_eq!(n(&mut g, "GetLoadedSaveSize 0"), 999.0);
    assert_eq!(n(&mut g, "GetLoadedSaveSize 1"), 0.0);
    assert_eq!(s(&mut g, "GetSaveName 0"), "Save 3");
    assert_eq!(s(&mut g, "GetSaveName 1"), "quicksave");
    // A load keeps them.
    let session = g.take_session();
    g.put_session(session);
    assert_eq!(s(&mut g, "GetSaveName 1"), "quicksave");
    // Indoors the player's cell alone; no map markers.
    assert_eq!(n(&mut g, "let aR := GetNearCells 2\nSetFunctionValue aR[0]"), f64::from(CELL));
    assert_eq!(n(&mut g, "let aR := GetNearMapMarkers -1\nSetFunctionValue (Ar_Size aR)"), 0.0);
    // The caught flag, set and read.
    assert_eq!(n(&mut g, "TestMarkRef.SetCaughtPCPickpocketting"), 0.0);
    assert_eq!(n(&mut g, "TestMarkRef.SetCaughtPCPickpocketting 1"), 1.0);
    assert_eq!(n(&mut g, "TestMarkRef.SetCaughtPCPickpocketting"), 1.0);
    // No script runs a query: nothing marked, 1 all the same [observed: sup4 markonload].
    assert_eq!(n(&mut g, "MarkScriptOnLoad"), 1.0);
    assert_eq!(s(&mut g, "TranslitText \"Привет\""), "Privet");
    assert_eq!(n(&mut g, "AllocatePlayer"), 0.0);
}
