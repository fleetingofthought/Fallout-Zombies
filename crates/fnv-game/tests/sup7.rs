//! SUP NVSE batch 7 (#1008; docs/spec-notes/ext-sup7.md) on a synthetic plugin, called
//! from scripts: the NIF block interpolations frame by frame, their finish events, the
//! vertices the engine tells, and the HUD bars' script-variable and UDF values. The
//! engine's part is played by the test (it fills the scene mirror as the engine would).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::nif_blocks::{SceneEdit, SceneNode};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const P: FormId = FormId::PLAYER_REF;

fn plugin_bytes() -> Vec<u8> {
    let npcs = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let glob = record(b"GLOB", 0, 0x800, &[sub(b"EDID", z("Sup7Global")), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let header = record(b"TES4", 0, 0, &[sub(b"HEDR", [1.34f32.to_le_bytes().to_vec(), 1i32.to_le_bytes().to_vec(), 0x800u32.to_le_bytes().to_vec()].concat())]);
    [header, top(b"GLOB", &glob), top(b"NPC_", &npcs), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)))].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Sup7.esm".into()).unwrap()];
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

/// Half-second frames; the player drawn with a head under a root.
fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, frame_seconds: 0.5, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    let node = |name: &str, parent| SceneNode { name: name.into(), parent, translation: [0.0; 3], rotation: [0.0; 3], scale: 1.0, flags: 0, material: None };
    g.scene.set_tree(P, vec![node("Scene Root", None), SceneNode { scale: 1.5, translation: [1.0, 2.0, 3.0], ..node("Bip01 Head", Some(0)) }]);
    g
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("array_var a\nBegin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(g: &mut Game, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        v => panic!("{expr}: {v:?}"),
    }
}

/// The scales sent to the engine for the head since the last call.
fn scales(g: &mut Game) -> Vec<f32> {
    g.scene.take_edits().into_iter().filter_map(|e| match e {
        SceneEdit::Scale { node, v, .. } if node == "Bip01 Head" => Some(v),
        _ => None,
    }).collect()
}

#[test]
fn an_interpolation_runs_frame_by_frame_and_ends() {
    let f = fixture();
    let mut g = game(&f);
    // From 1 to 3 in 2 s, once [documented: NifBlockInterpolateScale's first example].
    assert_eq!(n(&mut g, r#"PlayerREF.NifBlockInterpolateScale "Bip01 Head" 1 3 2 0 0 0"#), 1.0);
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head""#), 1.0);
    // Type 0 is the scale, 1 the rotation; the first-person model is another one; no
    // block finds none [observed: sup7 scale.start, `1 1 0 0 0 0` with type 2 asked].
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head" 0"#), 1.0);
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head" 2"#), 0.0);
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head" 1"#), 0.0);
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head" -1 1"#), 0.0);
    assert_eq!(n(&mut g, "PlayerREF.IsNifBlockInterpolated"), 0.0);
    scales(&mut g);
    // The frame it starts shows the start.
    g.tick();
    g.tick();
    assert_eq!(scales(&mut g), vec![1.0, 1.5]);
    // The mirror shows it at once, as JIP's setters do.
    assert_eq!(n(&mut g, r#"PlayerREF.GetNifBlockScale "Bip01 Head""#), 1.5);
    for _ in 0..3 {
        g.tick();
    }
    assert_eq!(scales(&mut g), vec![2.0, 2.5, 3.0]);
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head""#), 0.0);
}

#[test]
fn calculate_start_value_takes_the_block_as_it_is() {
    let f = fixture();
    let mut g = game(&f);
    // The start 9 is ignored; the head's 1.5 is taken [documented: "Scale N will be
    // calculated automatically"].
    n(&mut g, r#"PlayerREF.NifBlockInterpolateScale "Bip01 Head" 9 3.5 1 0 0 1"#);
    g.scene.take_edits();
    g.tick();
    g.tick();
    assert_eq!(scales(&mut g), vec![1.5, 2.5]);
    // The translation's arguments: world before pc, calcStart last.
    n(&mut g, r#"PlayerREF.NifBlockInterpolateTranslation "Bip01 Head" 0 0 0 1 2 13 2 0 0 0 1"#);
    g.tick();
    g.tick();
    let t: Vec<[f32; 3]> = g.scene.take_edits().into_iter().filter_map(|e| match e {
        SceneEdit::Translation { v, .. } => Some(v),
        _ => None,
    }).collect();
    assert_eq!(t, vec![[1.0, 2.0, 3.0], [1.0, 2.0, 5.5]]);
}

#[test]
fn a_back_and_forth_loop_stops_when_asked() {
    let f = fixture();
    let mut g = game(&f);
    n(&mut g, r#"PlayerREF.NifBlockInterpolateScale "Bip01 Head" 1 2 1 2 0 0"#);
    for _ in 0..5 {
        g.tick();
    }
    assert_eq!(scales(&mut g), vec![1.0, 1.5, 2.0, 1.5, 1.0]);
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head""#), 1.0);
    // Another type on the block stops nothing, -1 [observed: sup7 scale.next]; the
    // scale's type does.
    assert_eq!(n(&mut g, r#"PlayerREF.NifBlockInterpolateStop "Bip01 Head" 1"#), -1.0);
    // A bare Stop gives 0 and stops nothing [observed: sup7 second run].
    assert_eq!(n(&mut g, "PlayerREF.NifBlockInterpolateStop"), 0.0);
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head""#), 1.0);
    assert_eq!(n(&mut g, r#"PlayerREF.NifBlockInterpolateStop "Bip01 Head" 0"#), 1.0);
    g.tick();
    assert!(scales(&mut g).is_empty());
}

#[test]
fn trait_0_sets_the_start_again_at_the_end() {
    let f = fixture();
    let mut g = game(&f);
    n(&mut g, r#"PlayerREF.NifBlockInterpolateScale "Bip01 Head" 1 3 1 0 0 0"#);
    // 0 all the same [observed: sup7 trait].
    assert_eq!(n(&mut g, "PlayerREF.NifBlockInterpolateSetTrait 0 1"), 0.0);
    assert_eq!(n(&mut g, "PlayerREF.NifBlockInterpolateSetTrait 5 1"), 0.0);
    for _ in 0..3 {
        g.tick();
    }
    assert_eq!(scales(&mut g), vec![1.0, 2.0, 1.0]);
}

#[test]
fn the_finish_event_may_start_another() {
    let f = fixture();
    let mut g = game(&f);
    // The handler starts a rotation on the block that ended [documented:
    // SetSUPOnInterpolateFinishEvent, a new interpolation may be set in the event].
    q(&mut g, "SetSUPOnInterpolateFinishEvent (Begin Function {ref rr, string_var s, int t, int l, int pc}
rr.NifBlockInterpolateRotation \"Bip01 Head\" 0 0 0 0 0 90 1 0 0 0
End)");
    n(&mut g, r#"PlayerREF.NifBlockInterpolateScale "Bip01 Head" 1 2 0.5 0 0 0"#);
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head" 1"#), 0.0);
    g.tick();
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head" 0"#), 1.0);
    g.tick();
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head" 0"#), 0.0);
    assert_eq!(n(&mut g, r#"PlayerREF.IsNifBlockInterpolated "Bip01 Head" 1"#), 1.0);
}

#[test]
fn vertices_come_from_the_engine() {
    let f = fixture();
    let mut g = game(&f);
    // Not told yet, no such block: -1; a node with none: -2 [observed: sup7 verts].
    assert_eq!(n(&mut g, r#"PlayerREF.GetNifBlockNumVerticles "Head:0""#), -1.0);
    assert_eq!(n(&mut g, r#"PlayerREF.GetNifBlockNumVerticles "Bip01 Head""#), -2.0);
    assert_eq!(g.scene.wanted_vertices().count(), 2);
    g.scene.set_vertices(P, "head:0", Some(vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]));
    assert_eq!(n(&mut g, r#"PlayerREF.GetNifBlockNumVerticles "Head:0""#), 2.0);
    // [documented: myarray[1][0] is the first vertex's Y].
    let v = q(&mut g, r#"let a := PlayerREF.NifBlockGetVerticles "Head:0"
SetFunctionValue a[1][0] + a[2][1] * 10 + (Ar_Size a)"#);
    assert_eq!(v, XVal::Num(2.0 + 60.0 + 3.0));
    let v = q(&mut g, r#"let a := PlayerREF.NifBlockGetVerticles "Nothing"
SetFunctionValue (Ar_Size a)"#);
    assert_eq!(v, XVal::Num(0.0));
}

#[test]
fn bars_take_values_from_a_variable_and_a_function() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, r#"HudBarCreate "B" 10 10 3
HudBarCreate "U" 10 40 3
HudBarSetValueMax "B" 200"#);
    // A value that is no variable is refused; a global is taken.
    assert_eq!(n(&mut g, r#"HudBarSetValueScriptVar "B" 5 1"#), -1.0);
    assert_eq!(n(&mut g, r#"HudBarSetValueScriptVar "Missing" Sup7Global 1"#), -1.0);
    assert_eq!(n(&mut g, r#"HudBarSetValueScriptVar "B" Sup7Global 1"#), 1.0);
    q(&mut g, "set Sup7Global to 50");
    g.tick();
    let b = g.ui_extra.bars.list[0].clone();
    assert_eq!((b.value, b.share), (50.0, 0.25));
    // Read again only after fUpdateTimer (0.1 s by default; half-second frames).
    q(&mut g, r#"set Sup7Global to 100
HudBarSetBarTrait "B" 2 1"#);
    g.tick();
    assert_eq!(g.ui_extra.bars.list[0].value, 100.0);
    q(&mut g, "set Sup7Global to 150");
    g.tick();
    assert_eq!(g.ui_extra.bars.list[0].value, 100.0);
    g.tick();
    assert_eq!(g.ui_extra.bars.list[0].value, 150.0);
    // The UDF's result, raw as the share without iCalculateValueMax.
    assert_eq!(n(&mut g, r#"HudBarSetValueUDF "U" (Begin Function { }
SetFunctionValue 0.75
End)"#), 1.0);
    g.tick();
    let u = g.ui_extra.bars.list[1].clone();
    assert_eq!((u.value, u.share), (0.75, 0.75));
    // A bar destroyed takes its source with it.
    q(&mut g, r#"HudBarDestroy "U""#);
    g.tick();
    assert_eq!(g.sup7.feeds.len(), 1);
}
