//! JIP LN NVSE batch 10 (#1006; docs/spec-notes/jip-scripts-10.md) on a synthetic plugin:
//! quest objectives and their targets, the quest lists, SetStageAlt, perks and factions,
//! script variables added at run time, scripts and event blocks switched off,
//! FakeScriptEvent, linked and enable children, extra data, map markers and text files.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const NPC: u32 = 0x900;
const MARK: u32 = 0x901;
const QUEST: u32 = 0x920;
const QSCRIPT: u32 = 0x921;
const TARGET: u32 = 0x922;
const TARGET2: u32 = 0x923;
const STAT: u32 = 0x924;
const CHILD: u32 = 0x925;
const PERK: u32 = 0x926;
const HIDDEN_PERK: u32 = 0x927;
const FACT: u32 = 0x928;
const ASCRIPT: u32 = 0x929;
const ACTI: u32 = 0x92A;
const ACTI_REF: u32 = 0x92B;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn placed(sig: &[u8; 4], fid: u32, edid: &str, base: u32, pos: [f32; 3], extra: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    let data = [f32b(pos[0]), f32b(pos[1]), f32b(pos[2]), vec![0; 12]].concat();
    record(sig, 0, fid, &[vec![sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec())], extra.to_vec(), vec![sub(b"DATA", data)]].concat())
}

/// A script compiled from its source at load (it declares a string variable).
fn source_script(fid: u32, edid: &str, source: &str) -> Vec<u8> {
    let mut subs = unit(0, &script(&[(0, None, Vec::new())]), &[], &[]);
    subs.push(sub(b"SCTX", format!("scn {edid}\n{source}").into_bytes()));
    record(b"SCPT", 0, fid, &[vec![sub(b"EDID", z(edid))], subs].concat())
}

fn perk(fid: u32, edid: &str, hidden: u8) -> Vec<u8> {
    // DATA: trait, min level, ranks, playable, hidden [the fields: xEdit PERK].
    record(b"PERK", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"DATA", vec![0, 1, 1, 1, hidden])])
}

fn plugin_bytes() -> Vec<u8> {
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, NPC, &[sub(b"EDID", z("TestNpc")), sub(b"SNAM", [u32s(&[FACT]), vec![2, 0, 0, 0]].concat())]),
    ]
    .concat();
    let fact = record(b"FACT", 0, FACT, &[sub(b"EDID", z("TestFaction"))]);
    let perks = [perk(PERK, "TestPerk", 0), perk(HIDDEN_PERK, "TestHiddenPerk", 1)].concat();
    let scripts = [
        source_script(QSCRIPT, "TestQuestScript", "float fOwn\nstring_var sText\nbegin GameMode\nend\n"),
        source_script(ASCRIPT, "TestActiScript", "short iHits\nstring_var sText\nbegin OnHit\nset iHits to iHits + 1\nend\nbegin OnLoad\nset iHits to iHits + 10\nend\nbegin OnTriggerEnter\nMarkRef.SetRefExtraData 2 -903.5\nMarkRef.SetRefExtraData 6 TestStatic\nMarkRef.SetRefExtraData 19 \"Girly\"\nMarkRef.SetRefExtraData 18 7\nend\nbegin OnTriggerLeave\nMarkRef.SetRefExtraData\nend\n"),
    ]
    .concat();
    let stat = record(b"STAT", 0, STAT, &[sub(b"EDID", z("TestStatic"))]);
    let acti = record(b"ACTI", 0, ACTI, &[sub(b"EDID", z("TestActi")), sub(b"SCRI", ASCRIPT.to_le_bytes().to_vec())]);
    let qsta = |t: u32| sub(b"QSTA", [u32s(&[t]), vec![0, 0, 0, 0]].concat());
    let quest = record(
        b"QUST",
        0,
        QUEST,
        &[
            sub(b"EDID", z("TestQuest")),
            sub(b"SCRI", QSCRIPT.to_le_bytes().to_vec()),
            sub(b"FULL", z("Test Quest")),
            sub(b"DATA", vec![0, 0, 0, 0, 0, 0, 0, 0]),
            sub(b"INDX", 10i16.to_le_bytes().to_vec()),
            sub(b"QSDT", vec![0]),
            sub(b"INDX", 20i16.to_le_bytes().to_vec()),
            sub(b"QSDT", vec![0]),
            sub(b"QOBJ", 10i32.to_le_bytes().to_vec()),
            sub(b"NNAM", z("Find the thing")),
            qsta(TARGET),
            sub(b"QOBJ", 20i32.to_le_bytes().to_vec()),
            sub(b"NNAM", z("Report back")),
        ],
    );
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let xesp = sub(b"XESP", [u32s(&[TARGET]), vec![0, 0, 0, 0]].concat());
    let xlkr = sub(b"XLKR", u32s(&[TARGET]));
    let refs = [
        placed(b"ACHR", MARK, "MarkRef", NPC, [100.0, 200.0, 0.0], &[]),
        placed(b"REFR", TARGET, "TargetRef", STAT, [0.0, 0.0, 0.0], &[]),
        placed(b"REFR", TARGET2, "TargetTwoRef", STAT, [5.0, 0.0, 0.0], &[]),
        placed(b"REFR", CHILD, "ChildRef", STAT, [9.0, 0.0, 0.0], &[xlkr, xesp]),
        placed(b"REFR", ACTI_REF, "ActiRef", ACTI, [0.0, 9.0, 0.0], &[]),
    ]
    .concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"FACT", &fact),
        top(b"SCPT", &scripts),
        top(b"NPC_", &npcs),
        top(b"PERK", &perks),
        top(b"STAT", &stat),
        top(b"ACTI", &acti),
        top(b"QUST", &quest),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Jip10.esm".into()).unwrap()];
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
    g
}

/// A game loaded from a snapshot (`Game::restore`).
fn restored<'a>(f: &'a Fixture, snap: &fnv_game::game::GameSnapshot) -> Game<'a> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::restore(&f.data, world, GameConfig { strict: true, ..GameConfig::default() }, snap).unwrap()
}

const VARS: &str = "float fX\nfloat fR\nref rItem\nstring_var sOut\narray_var aList\narray_var aEl\n";

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("{VARS}Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
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

/// An array's elements as text, through the script (`%g` numbers, `%i` forms); "none" for
/// no array at all (`ar_Size` -1), what a list getter with nothing to give returns
/// [observed: the jip10 probes].
fn elements(g: &mut Game, expr: &str, spec: &str) -> String {
    s(g, &format!("let aList := {expr}\nlet sOut := \"none\"\nif (ar_Size aList) >= 0\nlet sOut := \"\"\nforeach aEl <- aList\nlet sOut := sOut + (sv_Construct \"{spec} \" aEl[\"value\"])\nloop\nendif\nSetFunctionValue sOut"))
}

fn ids(g: &mut Game, expr: &str) -> String {
    elements(g, expr, "%i")
}

fn nums(g: &mut Game, expr: &str) -> String {
    elements(g, expr, "%g")
}

#[test]
fn objective_targets_and_texts() {
    let f = fixture();
    let mut g = game(&f);
    let quest = FormId(QUEST);
    assert_eq!(g.objective_targets(quest, 10), [FormId(TARGET)]);
    // Not displayed yet: no change [documented].
    q(&mut g, "AddObjectiveTarget TestQuest 10 TargetTwoRef");
    assert_eq!(g.objective_targets(quest, 10), [FormId(TARGET)]);
    q(&mut g, "SetObjectiveDisplayed TestQuest 10 1\nAddObjectiveTarget TestQuest 10 TargetTwoRef");
    assert_eq!(g.objective_targets(quest, 10), [FormId(TARGET), FormId(TARGET2)]);
    assert_eq!(n(&mut g, "GetObjectiveHasTarget TestQuest 10 TargetTwoRef"), 1.0);
    assert_eq!(ids(&mut g, "GetObjectiveTargets TestQuest 10").trim(), format!("{TARGET:08X} {TARGET2:08X}"));
    q(&mut g, "SetObjectiveNthTarget TestQuest 10 0 ChildRef\nRemoveObjectiveTarget TestQuest 10 TargetTwoRef");
    assert_eq!(g.objective_targets(quest, 10), [FormId(CHILD)]);
    // The text, formatted [documented: the example].
    assert_eq!(g.objective_text(quest, 10).as_deref(), Some("Find the thing"));
    q(&mut g, "SetObjectiveText TestQuest 10 \"Kill %g of them\" 3");
    assert_eq!(g.objective_text(quest, 10).as_deref(), Some("Kill 3 of them"));
    // The lists of objectives and quests.
    q(&mut g, "SetObjectiveDisplayed TestQuest 20 1\nSetObjectiveCompleted TestQuest 10 1");
    assert_eq!(nums(&mut g, "GetQuestObjectives TestQuest 0").trim(), "20");
    assert_eq!(nums(&mut g, "GetQuestObjectives TestQuest 1").trim(), "10");
    q(&mut g, "StartQuest TestQuest");
    assert_eq!(ids(&mut g, "GetQuests").trim(), format!("{QUEST:08X}"));
    assert_eq!(ids(&mut g, "GetQuests 1").trim(), "none");
    q(&mut g, "ForceActiveQuest TestQuest");
    assert_eq!(nums(&mut g, "GetActiveObjectives").trim(), "20");
    q(&mut g, "ClearActiveQuest");
    assert_eq!(g.ui.active_quest, None);
}

#[test]
fn set_stage_alt_lowers_the_stage() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "SetStage TestQuest 20");
    assert_eq!(n(&mut g, "GetStage TestQuest"), 20.0);
    q(&mut g, "SetStageAlt TestQuest 10");
    assert_eq!((n(&mut g, "GetStage TestQuest"), n(&mut g, "GetStageDone TestQuest 10")), (10.0, 1.0));
}

#[test]
fn perks_and_factions() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "player.AddPerk TestPerk\nplayer.AddPerk TestHiddenPerk\nplayer.AddPerk TestPerk 1");
    assert_eq!(ids(&mut g, "player.GetAllPerks").trim(), format!("{PERK:08X}"));
    assert_eq!(ids(&mut g, "player.GetAllPerks 0 1").trim(), format!("{PERK:08X} {HIDDEN_PERK:08X}"));
    assert_eq!(ids(&mut g, "player.GetAllPerks 1").trim(), format!("{PERK:08X}"));
    q(&mut g, "player.RemoveAllPerks 1");
    assert!(g.player.teammate_perks.is_empty() && g.player.perks.len() == 2);
    q(&mut g, "player.RemoveAllPerks");
    assert!(g.player.perks.is_empty());
    q(&mut g, "MarkRef.AddPerk TestPerk");
    assert_eq!(ids(&mut g, "MarkRef.GetAllPerks").trim(), format!("{PERK:08X}"));
    q(&mut g, "MarkRef.RemoveAllPerks");
    assert_eq!(ids(&mut g, "MarkRef.GetAllPerks").trim(), "none");
    // The base's factions, and the reference's now.
    assert_eq!(ids(&mut g, "GetFactions TestNpc").trim(), format!("{FACT:08X}"));
    q(&mut g, "MarkRef.RemoveFromFaction TestFaction");
    assert_eq!(ids(&mut g, "MarkRef.GetFactions").trim(), "none");
    assert_eq!(ids(&mut g, "GetFactions TestNpc").trim(), format!("{FACT:08X}"));
}

#[test]
fn variables_added_to_a_script() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "StartQuest TestQuest");
    assert_eq!(n(&mut g, "AddScriptVariable \"fNew\" TestQuest"), 1.0);
    assert_eq!(n(&mut g, "AddScriptVariable \"fNew\" TestQuest"), 1.0, "again [observed]");
    assert_eq!(n(&mut g, "AddScriptVariable \"fOwn\" TestQuest"), 0.0, "the script's own");
    assert_eq!(n(&mut g, "HasVariableAdded \"fnew\" TestQuest"), 1.0);
    q(&mut g, "SetVariable \"fNew\" 2.5 TestQuest");
    assert_eq!(n(&mut g, "GetVariable \"fNew\" TestQuest"), 2.5);
    q(&mut g, "SetRefVariable \"fNew\" MarkRef TestQuest");
    assert_eq!(n(&mut g, "GetRefVariable \"fNew\" TestQuest"), f64::from(MARK));
    assert_eq!(n(&mut g, "RemoveScriptVariable \"fNew\" TestQuest"), 1.0);
    assert_eq!((n(&mut g, "HasVariableAdded \"fNew\" TestQuest"), n(&mut g, "GetVariable \"fNew\" TestQuest")), (0.0, 0.0));
    q(&mut g, "AddScriptVariable \"fA\" TestQuest\nAddScriptVariable \"fB\" TestQuest\nRemoveAllAddedVariables TestQuest");
    assert_eq!(n(&mut g, "HasVariableAdded \"fB\" TestQuest"), 0.0);
    q(&mut g, "AddScriptVariable \"fC\" TestQuest\nClearJIPSavedData 1 0 0 0");
    assert_eq!(n(&mut g, "HasVariableAdded \"fC\" TestQuest"), 0.0);
    // Saved with the game [documented].
    q(&mut g, "AddScriptVariable \"fD\" TestQuest");
    let snap = g.snapshot();
    let mut g2 = restored(&f, &snap);
    assert_eq!(n(&mut g2, "HasVariableAdded \"fD\" TestQuest"), 1.0);
}

#[test]
fn scripts_and_event_blocks_switched_off() {
    let f = fixture();
    let mut g = game(&f);
    // The cell's load ran OnLoad (+10) already: count the hits past it.
    let loaded = n(&mut g, "GetVariable \"iHits\" ActiRef");
    let hits = |g: &mut Game| n(g, "GetVariable \"iHits\" ActiRef") - loaded;
    assert_eq!(n(&mut g, "ActiRef.FakeScriptEvent \"OnHit\""), 1.0);
    assert_eq!(hits(&mut g), 1.0);
    // A block switched off on the base: the event does not run it; FakeScriptEvent does.
    q(&mut g, "SetScriptEventDisabled TestActi \"OnHit OnLoad\" 1");
    assert_eq!((n(&mut g, "GetScriptEventDisabled TestActi \"onhit\""), n(&mut g, "GetScriptEventDisabled TestActi \"OnAdd\"")), (1.0, 0.0));
    g.ref_event(FormId(ACTI_REF), fnv_game::game::event::ON_HIT, None, None);
    assert_eq!(hits(&mut g), 1.0, "switched off");
    q(&mut g, "ActiRef.FakeScriptEvent \"OnHit\"");
    assert_eq!(hits(&mut g), 2.0, "faked anyway [documented]");
    q(&mut g, "SetScriptEventDisabled TestActi \"OnHit\" 0");
    g.ref_event(FormId(ACTI_REF), fnv_game::game::event::ON_HIT, None, None);
    assert_eq!(hits(&mut g), 3.0);
    // OnActivate is not faked; an unknown name neither.
    assert_eq!((n(&mut g, "ActiRef.FakeScriptEvent \"OnActivate\""), n(&mut g, "ActiRef.FakeScriptEvent \"OnNothing\"")), (0.0, 0.0));
    // The whole script off.
    q(&mut g, "SetScriptDisabled TestActiScript 1");
    assert_eq!(n(&mut g, "GetScriptDisabled TestActiScript"), 1.0);
    g.ref_event(FormId(ACTI_REF), fnv_game::game::event::ON_HIT, None, None);
    assert_eq!(hits(&mut g), 3.0, "the variables still read");
    q(&mut g, "SetScriptDisabled TestActiScript 0");
    g.ref_event(FormId(ACTI_REF), fnv_game::game::event::ON_HIT, None, None);
    assert_eq!(hits(&mut g), 4.0);
}

#[test]
fn children_extra_data_and_markers() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(ids(&mut g, "TargetRef.GetLinkedChildren").trim(), format!("{CHILD:08X}"));
    assert_eq!(ids(&mut g, "TargetRef.GetEnableChildren").trim(), format!("{CHILD:08X}"));
    q(&mut g, "TargetTwoRef.SetLinkedRef TargetRef");
    assert_eq!(ids(&mut g, "TargetRef.GetLinkedChildren").trim(), format!("{TARGET2:08X} {CHILD:08X}"), "by FormID [documented]");
    // Extra data: slots 0-17 numbers and forms, 18-19 strings; by the owning mod's index.
    // A script of no plugin keeps nothing [observed]; the rest from a script of the
    // plugin (index 0).
    q(&mut g, "MarkRef.SetRefExtraData 2 -903.5");
    assert_eq!(n(&mut g, "MarkRef.RefHasExtraData 255"), 0.0);
    // The plugin's script sets slots 2, 6, 19 and 18 (a number, refused) in its
    // OnTriggerEnter block and clears them in OnTriggerLeave; read by the plugin's index.
    assert_eq!(n(&mut g, "ActiRef.FakeScriptEvent \"OnTriggerEnter\""), 1.0);
    assert_eq!(n(&mut g, "MarkRef.GetRefExtraData 2 0"), -903.5);
    assert_eq!(n(&mut g, "MarkRef.GetRefExtraData 6 0"), f64::from(STAT));
    assert_eq!(s(&mut g, "let sOut := MarkRef.GetRefExtraData 19 0\nSetFunctionValue sOut"), "Girly");
    assert_eq!(s(&mut g, "let sOut := MarkRef.GetRefExtraData 18 0\nSetFunctionValue sOut"), "", "a number in a string slot is refused");
    assert_eq!((n(&mut g, "MarkRef.RefHasExtraData 0"), n(&mut g, "MarkRef.RefHasExtraData 3"), n(&mut g, "MarkRef.GetRefExtraData 2 3")), (1.0, 0.0, 0.0));
    assert_eq!(n(&mut g, "ActiRef.FakeScriptEvent \"OnTriggerLeave\""), 1.0);
    assert_eq!(n(&mut g, "MarkRef.RefHasExtraData 0"), 0.0);
    // Map markers, not saved [documented].
    q(&mut g, "TargetRef.AddRefMapMarker");
    assert_eq!(n(&mut g, "TargetRef.RefHasMapMarker"), 1.0);
    let snap = g.snapshot();
    q(&mut g, "TargetRef.RemoveRefMapMarker");
    assert_eq!(n(&mut g, "TargetRef.RefHasMapMarker"), 0.0);
    q(&mut g, "TargetRef.AddRefMapMarker");
    let mut g = restored(&f, &snap);
    assert_eq!(n(&mut g, "TargetRef.RefHasMapMarker"), 0.0, "gone after a load");
}

#[test]
fn text_files() {
    let f = fixture();
    let mut g = game(&f);
    // Written in NVForever's layer (here the session only), read back by lines.
    assert_eq!(n(&mut g, "WriteStringToFile \"nvf_jip10_test.txt\" 0 \"one%rtwo%rthree\""), 1.0);
    assert_eq!(n(&mut g, "WriteStringToFile \"nvf_jip10_test.txt\" 1 \"%rfour %g\" 5"), 1.0);
    assert_eq!(s(&mut g, "SetFunctionValue (ReadStringFromFile \"nvf_jip10_test.txt\")"), "one\ntwo\nthree\n\nfour 5", "an append starts a line [observed]");
    assert_eq!(s(&mut g, "SetFunctionValue (ReadStringFromFile \"nvf_jip10_test.txt\" 2 2)"), "two\nthree", "no break after the last line [observed]");
    assert_eq!(s(&mut g, "SetFunctionValue (ReadStringFromFile \"nvf_jip10_no_such_file.txt\")"), "");
}
