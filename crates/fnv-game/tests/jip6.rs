//! JIP LN NVSE batch 6 (#1006; docs/spec-notes/jip-records-6.md) on a synthetic plugin:
//! destruction data, creature data and messages.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const ACTI: u32 = 0x900;
const EXPL: u32 = 0x901;
const EXPL2: u32 = 0x902;
const CREA: u32 = 0x903;
const CREA_SOUNDS: u32 = 0x904;
const WEAPONS: u32 = 0x905;
const MESG: u32 = 0x906;
const MESG2: u32 = 0x907;
const MESSAGES: u32 = 0x908;
const PLAIN: u32 = 0x909;

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

/// DSTD [documented: schemas/fnv/common.json, from xEdit wbDEST]: health percent, index,
/// damage stage, flags, self damage per second, explosion, debris, debris count.
fn dstd(health: u8, index: u8, stage: u8, flags: u8, self_damage: i32, explosion: u32, count: i32) -> Vec<u8> {
    [vec![health, index, stage, flags], self_damage.to_le_bytes().to_vec(), u32s(&[explosion, 0]), count.to_le_bytes().to_vec()].concat()
}

fn plugin_bytes() -> Vec<u8> {
    let npc = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    // Health 100, two stages, VATS targetable; stage 1 has a replacement model.
    let acti = [
        edid(
            b"ACTI",
            ACTI,
            "TestBox",
            &[
                sub(b"DEST", [100i32.to_le_bytes().to_vec(), vec![2, 1, 0, 0]].concat()),
                sub(b"DSTD", dstd(50, 0, 1, 1, 0, EXPL, 0)),
                sub(b"DSTF", vec![]),
                sub(b"DSTD", dstd(0, 1, 2, 4, 3, 0, 2)),
                sub(b"DMDL", z("clutter\\boxhulk.nif")),
                sub(b"DSTF", vec![]),
            ],
        ),
        edid(b"ACTI", PLAIN, "TestPlain", &[]),
    ]
    .concat();
    let expl = [edid(b"EXPL", EXPL, "TestExplosion", &[]), edid(b"EXPL", EXPL2, "TestExplosion2", &[])].concat();
    // DATA [documented: schemas/fnv/gameplay.json CREA DATA]: type, three skills, health,
    // two unused bytes, damage 25, seven attributes.
    let crea_data = [vec![0, 50, 0, 50], 100i16.to_le_bytes().to_vec(), vec![0, 0], 25i16.to_le_bytes().to_vec(), vec![5; 7]].concat();
    let crea = [
        edid(
            b"CREA",
            CREA,
            "TestCreature",
            &[
                sub(b"NIFZ", [z("dogbody.nif"), z("eyessetvicious.nif")].concat()),
                sub(b"DATA", crea_data.clone()),
                sub(b"RNAM", vec![96]),
                sub(b"LNAM", WEAPONS.to_le_bytes().to_vec()),
                sub(b"CSCR", CREA_SOUNDS.to_le_bytes().to_vec()),
            ],
        ),
        edid(b"CREA", CREA_SOUNDS, "TestSounds", &[sub(b"DATA", crea_data)]),
    ]
    .concat();
    let flst = [edid(b"FLST", WEAPONS, "TestWeaponList", &[]), edid(b"FLST", MESSAGES, "TestMessages", &[sub(b"LNAM", u32s(&[MESG])), sub(b"LNAM", u32s(&[MESG2]))])].concat();
    let mesg = [
        edid(b"MESG", MESG, "TestNote", &[sub(b"DESC", z("A note")), sub(b"DNAM", u32s(&[2])), sub(b"TNAM", u32s(&[4]))]),
        edid(b"MESG", MESG2, "TestBoxMessage", &[sub(b"DESC", z("A box")), sub(b"DNAM", u32s(&[1]))]),
    ]
    .concat();
    [tes4(), top(b"NPC_", &npc), top(b"ACTI", &acti), top(b"EXPL", &expl), top(b"CREA", &crea), top(b"FLST", &flst), top(b"MESG", &mesg)].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Jip6.esm".into()).unwrap()];
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

/// The value a query's last line sets.
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

fn s(g: &mut Game, expr: &str) -> String {
    match q(g, &format!("string_var sOut\nlet sOut := {expr}\nSetFunctionValue sOut")) {
        XVal::Str(s) => s,
        v => panic!("{expr}: {v:?}"),
    }
}

#[test]
fn destruction_stages_read_and_change() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetDestructionDataTargetable TestBox"), 1.0);
    assert_eq!(n(&mut g, "GetDestructionDataTargetable TestPlain"), 0.0, "no destruction data");
    // [documented: GetNthDestructionStageTrait's table: 0 health %, 1 self damage, 2 model
    // damage stage, 3 flags, 4 debris count].
    for (stage, t, want) in [(0, 0, 50.0), (0, 2, 1.0), (0, 3, 1.0), (1, 1, 3.0), (1, 3, 4.0), (1, 4, 2.0), (0, 5, 0.0), (2, 0, 0.0)] {
        assert_eq!(n(&mut g, &format!("GetNthDestructionStageTrait TestBox {stage} {t}")), want, "stage {stage} trait {t}");
    }
    assert_eq!(n(&mut g, "GetNthDestructionStageExplosion TestBox 0"), f64::from(EXPL));
    assert_eq!(n(&mut g, "GetNthDestructionStageExplosion TestBox 1"), 0.0);
    assert_eq!(s(&mut g, "GetNthDestructionStageReplacement TestBox 1"), "clutter\\boxhulk.nif");
    assert_eq!(s(&mut g, "GetNthDestructionStageReplacement TestBox 0"), "");
    // Setters; a left-out explosion is none [documented: SetNthDestructionStageExplosion].
    q(&mut g, "SetNthDestructionStageExplosion TestBox 1 TestExplosion2");
    q(&mut g, "SetNthDestructionStageExplosion TestBox 0");
    assert_eq!((n(&mut g, "GetNthDestructionStageExplosion TestBox 0"), n(&mut g, "GetNthDestructionStageExplosion TestBox 1")), (0.0, f64::from(EXPL2)));
    q(&mut g, "SetNthDestructionStageDebris TestBox 0 TestExplosion");
    assert_eq!(n(&mut g, "GetNthDestructionStageDebris TestBox 0"), f64::from(EXPL));
    q(&mut g, "SetNthDestructionStageTrait TestBox 1 3 5");
    assert_eq!(n(&mut g, "GetNthDestructionStageTrait TestBox 1 3"), 5.0);
    // Infinity and NaN are not set (#1354 review); a health percentage past 100 is kept
    // modulo 101 [observed: jip6b dest.hp.*].
    q(&mut g, "SetNthDestructionStageTrait TestBox 0 0 150");
    assert_eq!(n(&mut g, "GetNthDestructionStageTrait TestBox 0 0"), 49.0);
    q(&mut g, "SetNthDestructionStageTrait TestBox 0 0 50");
    for (t, bad, keep) in [(1, "(Pow 10 400)", 0.0), (2, "((Pow 10 400) - (Pow 10 400))", 1.0)] {
        q(&mut g, &format!("SetNthDestructionStageTrait TestBox 0 {t} {bad}"));
        assert_eq!(n(&mut g, &format!("GetNthDestructionStageTrait TestBox 0 {t}")), keep, "trait {t} {bad}");
    }
    q(&mut g, "SetNthDestructionStageTrait TestBox 0 0 100");
    assert_eq!(n(&mut g, "GetNthDestructionStageTrait TestBox 0 0"), 100.0);
    q(&mut g, "SetNthDestructionStageTrait TestBox 0 0 50");
    q(&mut g, "SetNthDestructionStageReplacement TestBox 0 \"clutter\\other.nif\"");
    assert_eq!(s(&mut g, "GetNthDestructionStageReplacement TestBox 0"), "clutter\\other.nif");
    q(&mut g, "SetDestructionDataTargetable TestBox 0");
    assert_eq!(n(&mut g, "GetDestructionDataTargetable TestBox"), 0.0);
    // Add and remove: the stage's index, -1 when nothing changed [observed: jip6 dest.add.*,
    // dest.remove.*].
    assert_eq!(n(&mut g, "AddDestructionStage TestBox 1 80 3"), 1.0);
    assert_eq!(n(&mut g, "GetNthDestructionStageTrait TestBox 1 0"), 80.0);
    assert_eq!(n(&mut g, "GetNthDestructionStageTrait TestBox 1 2"), 3.0);
    assert_eq!(n(&mut g, "GetNthDestructionStageExplosion TestBox 2"), f64::from(EXPL2), "the old stage 1 moved down");
    // a form without destruction data gets it [observed: jip6b dest.add.none]
    assert_eq!(n(&mut g, "AddDestructionStage TestPlain 0 50 1"), 0.0);
    assert_eq!(n(&mut g, "GetDestructionDataNumStages TestPlain"), 1.0);
    assert_eq!(n(&mut g, "AddDestructionStage TestBox 9 50 1"), -1.0, "past the end");
    // A NaN index adds nothing (#1354 review).
    assert_eq!(n(&mut g, "AddDestructionStage TestBox ((Pow 10 400) - (Pow 10 400)) 50 1"), -1.0);
    assert_eq!(n(&mut g, "GetNthDestructionStageTrait TestBox 0 0"), 50.0);
    assert_eq!(n(&mut g, "RemoveDestructionStage TestBox 0"), 0.0);
    assert_eq!(n(&mut g, "RemoveDestructionStage TestBox 7"), -1.0);
    // The game reads the changed data too.
    let d = g.destruction(FormId(ACTI)).unwrap();
    assert_eq!(d.stages.iter().map(|s| (s.health_percent, s.index)).collect::<Vec<_>>(), vec![(80, 0), (0, 1)]);
    assert_eq!(n(&mut g, "GetDestructionDataNumStages TestBox"), 2.0);
}

#[test]
fn creature_data_and_models() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetCreatureDamage TestCreature"), 25.0);
    assert_eq!(n(&mut g, "GetCreatureReach TestCreature"), 96.0);
    assert_eq!(n(&mut g, "GetCreatureWeaponList TestCreature"), f64::from(WEAPONS));
    assert_eq!(n(&mut g, "GetCreatureSoundsTemplate TestCreature"), f64::from(CREA_SOUNDS));
    assert_eq!(n(&mut g, "GetCreatureDamage TestBox"), 0.0, "not a creature");
    q(&mut g, "SetCreatureDamage 40 TestCreature");
    assert_eq!(n(&mut g, "GetCreatureDamage TestCreature"), 40.0);
    // Combat's attack reads the change; NaN is not set (#1354 review), anything else is
    // held to 0-65535 [observed: jip6b cr.dmg.*].
    assert_eq!(g.creature_attack(FormId(CREA)).map(|a| a.0), Some(40.0));
    q(&mut g, "SetCreatureDamage ((Pow 10 400) - (Pow 10 400)) TestCreature");
    assert_eq!(n(&mut g, "GetCreatureDamage TestCreature"), 40.0, "NaN");
    for (v, want) in [("40000", 40000.0), ("70000", 65535.0), ("-3", 0.0), ("40", 40.0)] {
        q(&mut g, &format!("SetCreatureDamage {v} TestCreature"));
        assert_eq!(n(&mut g, "GetCreatureDamage TestCreature"), want, "{v}");
    }
    q(&mut g, "SetCreatureSoundsTemplate TestCreature");
    assert_eq!(n(&mut g, "GetCreatureSoundsTemplate TestCreature"), 0.0, "left out clears it");
    // The model list [documented: CreatureHasModel, case-insensitive].
    assert_eq!(n(&mut g, "CreatureHasModel \"EyesSetVicious.nif\" TestCreature"), 1.0);
    assert_eq!(n(&mut g, "CreatureHasModel \"tail.nif\" TestCreature"), 0.0);
    let models = |g: &mut Game| q(g, "array_var a\nlet a := GetCreatureModels TestCreature\nSetFunctionValue (Ar_Size a)");
    assert_eq!(models(&mut g), XVal::Num(2.0));
    assert_eq!(n(&mut g, "ToggleCreatureModel \"eyessetvicious.nif\" 0 TestCreature"), 1.0);
    assert_eq!(n(&mut g, "ToggleCreatureModel \"tail.nif\" 0 TestCreature"), 0.0);
    let first = q(&mut g, "array_var a\nstring_var sOut\nlet a := GetCreatureModels TestCreature\nlet sOut := a[0]\nSetFunctionValue sOut");
    assert_eq!(first, XVal::Str("dogbody.nif".into()));
    assert_eq!(models(&mut g), XVal::Num(1.0));
    // a model turned off is not had [observed: jip6 cr.toggle.off]
    assert_eq!(n(&mut g, "CreatureHasModel \"eyessetvicious.nif\" TestCreature"), 0.0);
}

#[test]
fn message_flags_time_and_disabling() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: GetMessageFlags, 1 Is Message Box, 2 Auto-display].
    assert_eq!((n(&mut g, "GetMessageFlags TestNote"), n(&mut g, "GetMessageFlags TestBoxMessage")), (2.0, 1.0));
    q(&mut g, "SetMessageFlags TestNote 3");
    assert_eq!(n(&mut g, "GetMessageFlags TestNote"), 3.0);
    assert_eq!(g.message_flags(FormId(MESG)) & 1, 1, "the game shows it as a box now");
    assert_eq!(g.message_seconds(FormId(MESG)), Some(4));
    q(&mut g, "SetMessageDisplayTime TestNote 9");
    assert_eq!(g.message_seconds(FormId(MESG)), Some(9));
    // Negative, NaN or infinity: not set (#1354 review).
    for bad in ["(Pow 10 400)", "((Pow 10 400) - (Pow 10 400))", "-5"] {
        q(&mut g, &format!("SetMessageDisplayTime TestNote {bad}"));
        assert_eq!(g.message_seconds(FormId(MESG)), Some(9), "{bad}");
    }
    // A form list disables every message in it; a disabled message is never shown
    // [documented: SetMessageDisabled].
    q(&mut g, "SetMessageDisabled TestMessages 1");
    assert_eq!((n(&mut g, "GetMessageDisabled TestNote"), n(&mut g, "GetMessageDisabled TestBoxMessage")), (1.0, 1.0));
    let shown = |g: &Game| g.events.iter().filter(|e| matches!(e.kind, EventKind::MessageShown { .. })).count();
    let before = shown(&g);
    q(&mut g, "ShowMessage TestBoxMessage");
    assert_eq!(shown(&g), before, "not shown while disabled");
    q(&mut g, "SetMessageDisabled TestBoxMessage 0");
    assert_eq!(n(&mut g, "GetMessageDisabled TestBoxMessage"), 0.0);
    q(&mut g, "ShowMessage TestBoxMessage");
    assert_eq!(shown(&g), before + 1);
    // ToggleCraftingMessages [documented].
    assert!(g.crafting_messages_shown());
    q(&mut g, "ToggleCraftingMessages 0");
    assert!(!g.crafting_messages_shown());
    q(&mut g, "ToggleCraftingMessages 1");
    assert!(g.crafting_messages_shown());
}


