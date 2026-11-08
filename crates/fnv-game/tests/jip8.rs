//! JIP LN NVSE batch 8 (#1006; docs/spec-notes/jip-records-8.md) on a synthetic plugin:
//! actor base data, actor effects, ingestibles and base effects, weapons, idles, body-part
//! names and limb models, cells and water.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC: u32 = 0xA00;
const NPC_MULT: u32 = 0xA01;
const CREA: u32 = 0xA02;
const TEMPLATE: u32 = 0xA03;
const MISC: u32 = 0xA04;
const MISC2: u32 = 0xA05;
const FACT: u32 = 0xA06;
const FACT2: u32 = 0xA07;
const FLST: u32 = 0xA08;
const SPEL: u32 = 0xA09;
const SPEL2: u32 = 0xA0A;
const ALCH: u32 = 0xA0B;
const MGEF: u32 = 0xA0C;
const MGEF_SCRIPT: u32 = 0xA0D;
const SCPT: u32 = 0xA0E;
const SCPT2: u32 = 0xA0F;
const ENCH: u32 = 0xA10;
const WEAP: u32 = 0xA11;
const IMOD: u32 = 0xA12;
const IDLE: u32 = 0xA13;
const BPTD: u32 = 0xA14;
const BPTD2: u32 = 0xA15;
const CELL: u32 = 0xA16;
const WATR: u32 = 0xA17;
const WATR2: u32 = 0xA18;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

/// ACBS [documented: xEdit NPC_/CREA]: flags, fatigue, barter gold, level (or the level
/// mult × 1000), calc min, calc max, speed mult, karma, disposition, template flags.
fn acbs(flags: u32, level: u16, min: u16, max: u16) -> Vec<u8> {
    [u32s(&[flags]), le16(0), le16(0), le16(level), le16(min), le16(max), le16(100), f32b(0.0), le16(0), le16(0)].concat()
}

/// SNAM: faction, rank, three unused bytes.
fn snam(f: u32, rank: i8) -> fnv_esm::raw::Subrecord {
    sub(b"SNAM", [u32s(&[f]), vec![rank as u8, 0, 0, 0]].concat())
}

/// MGEF DATA (72 bytes): flags, the associated item, the archetype; the rest empty.
fn mgef(fid: u32, name: &str, flags: u32, item: u32, archetype: u32) -> Vec<u8> {
    let data = [u32s(&[flags]), f32b(0.0), u32s(&[item]), vec![0xFF; 8], vec![0; 4], vec![0; 4], f32b(1.0), vec![0; 24], vec![0; 8], u32s(&[archetype]), (-1i32).to_le_bytes().to_vec()].concat();
    assert_eq!(data.len(), 72);
    edid(b"MGEF", fid, name, &[sub(b"DATA", data)])
}

/// BPND (84 bytes) of part type `part`, all else empty.
fn bpnd(part: i8) -> fnv_esm::raw::Subrecord {
    let data = [f32b(1.0), vec![0, part as u8, 100, 0xFF, 0, 0], vec![0; 2], vec![0; 72]].concat();
    assert_eq!(data.len(), 84);
    sub(b"BPND", data)
}

/// WATR DNAM: 49 four-byte slots; slot k (trait k + 2) holds k + 2, the three colours
/// (slots 10-12) RGB bytes.
fn watr_dnam() -> Vec<u8> {
    (0..49u8)
        .flat_map(|k| match k {
            10 => vec![10, 20, 30, 0],
            11 => vec![1, 2, 3, 0],
            12 => vec![255, 0, 128, 0],
            7 | 13 => vec![0; 4],
            _ => f32b(f32::from(k) + 2.0),
        })
        .collect()
}

fn plugin_bytes() -> Vec<u8> {
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        // Female (bit 0), level 5, 2-10; a faction rank 2; one actor effect; a death item,
        // a template; a skeleton.
        edid(b"NPC_", NPC, "TestNPC", &[sub(b"ACBS", acbs(1, 5, 2, 10)), snam(FACT, 2), sub(b"SPLO", u32s(&[SPEL])), sub(b"INAM", u32s(&[MISC])), sub(b"TPLT", u32s(&[TEMPLATE])), sub(b"MODL", z("characters\\_male\\skeleton.nif"))]),
        // PC Level Mult (bit 7), mult 0.5.
        edid(b"NPC_", NPC_MULT, "TestNPCMult", &[sub(b"ACBS", acbs(1 << 7, 500, 1, 0))]),
        edid(b"NPC_", TEMPLATE, "TestTemplate", &[sub(b"ACBS", acbs(0, 1, 0, 0))]),
    ]
    .concat();
    let crea = edid(b"CREA", CREA, "TestCreature", &[sub(b"ACBS", acbs(0, 3, 0, 0)), sub(b"PNAM", u32s(&[BPTD])), sub(b"SPLO", u32s(&[SPEL])), sub(b"SPLO", u32s(&[SPEL2])), sub(b"MODL", z("creatures\\gecko\\skeleton.nif"))]);
    let misc = [edid(b"MISC", MISC, "TestDeathItem", &[]), edid(b"MISC", MISC2, "TestDeathItem2", &[])].concat();
    let fact = [edid(b"FACT", FACT, "TestFaction", &[]), edid(b"FACT", FACT2, "TestFaction2", &[])].concat();
    let flst = edid(b"FLST", FLST, "TestFactionList", &[sub(b"LNAM", u32s(&[FACT2]))]);
    // SPIT: type, cost, level, flags (TestAbility's bit 4, so a set keeps the record's
    // other bits; TestAbility2's 0).
    let spit = |flags: u8| sub(b"SPIT", [u32s(&[4, 0, 0]), vec![flags, 0, 0, 0]].concat());
    let spel = [edid(b"SPEL", SPEL, "TestAbility", &[spit(16)]), edid(b"SPEL", SPEL2, "TestAbility2", &[spit(0)])].concat();
    // ENIT: value, flags 1 (food), unused, withdrawal SPEL, chance 0.1, sound.
    let alch = edid(b"ALCH", ALCH, "TestChem", &[sub(b"ENIT", [5i32.to_le_bytes().to_vec(), vec![2, 0, 0, 0], u32s(&[SPEL]), f32b(0.1), u32s(&[0])].concat())]);
    let mgefs = [mgef(MGEF, "TestEffect", 1 << 2, 0, 0), mgef(MGEF_SCRIPT, "TestScriptEffect", 0, SCPT, 1)].concat();
    let scpt = [edid(b"SCPT", SCPT, "TestScript", &[]), edid(b"SCPT", SCPT2, "TestScript2", &[])].concat();
    // ENIT: type, charge, cost, flags 1 (bit 0), unused.
    let ench = edid(b"ENCH", ENCH, "TestEnchant", &[sub(b"ENIT", [u32s(&[2, 0, 0]), vec![1, 0, 0, 0]].concat())]);
    let weap = edid(b"WEAP", WEAP, "TestGun", &[sub(b"MODL", z("weapons\\gun.nif")), sub(b"VNAM", u32s(&[1]))]);
    let imod = edid(b"IMOD", IMOD, "TestMod", &[]);
    // IDLE DATA: section, loop 1-3, unused, replay delay, flags, unused.
    let idle = edid(b"IDLE", IDLE, "TestIdle", &[sub(b"DATA", vec![0, 1, 3, 0, 0, 0, 0, 0])]);
    // Two parts: the torso named "Body" with a limb model, the head (by BPNN, no name).
    let bptd = [
        edid(
            b"BPTD",
            BPTD,
            "TestBodyParts",
            &[sub(b"BPTN", z("Body")), sub(b"BPNN", z("Bip01 Spine")), bpnd(0), sub(b"NAM1", z("torso_rep.nif")), sub(b"BPNN", z("Bip01 Head")), bpnd(1)],
        ),
        edid(b"BPTD", BPTD2, "TestBodyParts2", &[sub(b"BPTN", z("Torso")), sub(b"BPNN", z("Bip01")), bpnd(0)]),
    ]
    .concat();
    // Interior (bit 0) and has water (bit 1); LNAM ambient colour (bit 0); a water.
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell")), sub(b"DATA", vec![0x03]), sub(b"LNAM", u32s(&[1])), sub(b"XCWT", u32s(&[WATR]))]);
    let watr = [
        edid(b"WATR", WATR, "TestWater", &[sub(b"ANAM", vec![75]), sub(b"FNAM", vec![2]), sub(b"XNAM", u32s(&[SPEL])), sub(b"DATA", vec![0, 0]), sub(b"DNAM", watr_dnam())]),
        edid(b"WATR", WATR2, "TestWater2", &[sub(b"DATA", vec![0, 0])]),
    ]
    .concat();
    [
        tes4(),
        top(b"NPC_", &npcs),
        top(b"CREA", &crea),
        top(b"MISC", &misc),
        top(b"FACT", &fact),
        top(b"FLST", &flst),
        top(b"SPEL", &spel),
        top(b"ALCH", &alch),
        top(b"MGEF", &mgefs),
        top(b"SCPT", &scpt),
        top(b"ENCH", &ench),
        top(b"WEAP", &weap),
        top(b"IMOD", &imod),
        top(b"IDLE", &idle),
        top(b"BPTD", &bptd),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
        top(b"WATR", &watr),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Jip8.esm".into()).unwrap()];
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
fn actor_base_data() {
    let f = fixture();
    let mut g = game(&f);
    // Death item and template [documented: the pages' `F … baseForm` form].
    assert_eq!(n(&mut g, "GetDeathItem TestNPC"), f64::from(MISC));
    q(&mut g, "SetDeathItem TestDeathItem2 TestNPC");
    assert_eq!(n(&mut g, "GetDeathItem TestNPC"), f64::from(MISC2));
    assert_eq!(n(&mut g, "GetActorTemplate TestNPC"), f64::from(TEMPLATE));
    assert_eq!(n(&mut g, "GetActorTemplate TestCreature"), 0.0);
    // Not an actor: none.
    assert_eq!(n(&mut g, "GetDeathItem TestGun"), 0.0);
    // Leveling data [documented: the table, 0 level or mult, 1 calc min, 2 calc max].
    assert_eq!((n(&mut g, "GetActorLevelingData 0 TestNPC"), n(&mut g, "GetActorLevelingData 1 TestNPC"), n(&mut g, "GetActorLevelingData 2 TestNPC")), (5.0, 2.0, 10.0));
    assert_eq!(n(&mut g, "GetActorLevelingData 0 TestNPCMult"), 0.5);
    q(&mut g, "SetActorLevelingData 0 0.75 TestNPCMult");
    assert_eq!(n(&mut g, "GetActorLevelingData 0 TestNPCMult"), 0.75);
    assert_eq!(g.num_field(FormId(NPC_MULT), "ACBS", "level_or_level_mult_raw"), 750.0);
    q(&mut g, "SetActorLevelingData 1 5 TestNPC");
    assert_eq!(n(&mut g, "GetActorLevelingData 1 TestNPC"), 5.0);
    // Calc max not below calc min [observed: jip8].
    q(&mut g, "SetActorLevelingData 2 0 TestNPC");
    assert_eq!(n(&mut g, "GetActorLevelingData 2 TestNPC"), 5.0);
    assert_eq!(n(&mut g, "GetActorLevelingData 3 TestNPC"), 0.0);
    // Base faction rank, -1 when not a member [documented].
    assert_eq!(n(&mut g, "GetBaseFactionRank TestFaction TestNPC"), 2.0);
    assert_eq!(n(&mut g, "GetBaseFactionRank TestFaction2 TestNPC"), -1.0);
    // Body-part data and skeletons, read by the earlier getters.
    q(&mut g, "SetBodyPartData TestCreature TestBodyParts2");
    assert_eq!(n(&mut g, "GetBodyPartData TestCreature"), f64::from(BPTD2));
    q(&mut g, "SetCRESkeletonModel TestCreature \"creatures\\rat\\skeleton.nif\"");
    assert_eq!(s(&mut g, "GetSkeletonModel TestCreature"), "creatures\\rat\\skeleton.nif");
    q(&mut g, "SetNPCSkeletonModel TestNPC \"characters\\_male\\skeletonalt.nif\"");
    assert_eq!(s(&mut g, "GetSkeletonModel TestNPC"), "characters\\_male\\skeletonalt.nif");
}

#[test]
fn factions_and_combat_actions() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "PlayerREF.GetInFactionList TestFactionList"), 0.0);
    q(&mut g, "PlayerREF.AddToFaction TestFaction2 0");
    assert_eq!(n(&mut g, "PlayerREF.GetInFactionList TestFactionList"), 1.0);
    // The 27 bits [documented: the table]; past them dropped.
    assert_eq!(n(&mut g, "PlayerREF.GetExcludedCombatActions"), 0.0);
    q(&mut g, "PlayerREF.SetExcludedCombatActions 384");
    assert_eq!(n(&mut g, "PlayerREF.GetExcludedCombatActions"), 384.0);
    q(&mut g, "PlayerREF.SetExcludedCombatActions 268435457");
    assert_eq!(n(&mut g, "PlayerREF.GetExcludedCombatActions"), 1.0);
    q(&mut g, "PlayerREF.SetExcludedCombatActions 0");
    assert_eq!(n(&mut g, "PlayerREF.GetExcludedCombatActions"), 0.0);
}

#[test]
fn actor_effects_ingestibles_and_base_effects() {
    let f = fixture();
    let mut g = game(&f);
    // SPLO [documented: GetNumActorEffects, GetNthActorEffect]; the record's last first,
    // an added one at the front [observed: jip8].
    assert_eq!((n(&mut g, "GetNumActorEffects TestCreature"), n(&mut g, "GetNthActorEffect 0 TestCreature"), n(&mut g, "GetNthActorEffect 1 TestCreature")), (2.0, f64::from(SPEL2), f64::from(SPEL)));
    assert_eq!((n(&mut g, "GetNumActorEffects TestNPC"), n(&mut g, "GetNthActorEffect 0 TestNPC")), (1.0, f64::from(SPEL)));
    q(&mut g, "AddBaseEffectListEffect TestAbility2 TestNPC");
    q(&mut g, "AddBaseEffectListEffect TestAbility2 TestNPC");
    assert_eq!((n(&mut g, "GetNumActorEffects TestNPC"), n(&mut g, "GetNthActorEffect 0 TestNPC"), n(&mut g, "GetNthActorEffect 1 TestNPC")), (2.0, f64::from(SPEL2), f64::from(SPEL)));
    q(&mut g, "RemoveBaseEffectListEffect TestAbility TestNPC");
    assert_eq!((n(&mut g, "GetNumActorEffects TestNPC"), n(&mut g, "GetNthActorEffect 0 TestNPC")), (1.0, f64::from(SPEL2)));
    assert_eq!(n(&mut g, "GetNthActorEffect 5 TestNPC"), 0.0);
    // Ingestible ENIT [documented: the pages].
    assert!((n(&mut g, "GetAddictionChance TestChem") - 0.1).abs() < 1e-6);
    q(&mut g, "SetAddictionChance TestChem 0.25");
    assert_eq!(n(&mut g, "GetAddictionChance TestChem"), 0.25);
    // Kept as given [observed: jip8, 50, 1.5, -0.5].
    q(&mut g, "SetAddictionChance TestChem 1.5");
    assert_eq!(n(&mut g, "GetAddictionChance TestChem"), 1.5);
    assert_eq!(n(&mut g, "GetAddictionEffect TestChem"), f64::from(SPEL));
    q(&mut g, "SetAddictionEffect TestChem TestAbility2");
    assert_eq!(n(&mut g, "GetAddictionEffect TestChem"), f64::from(SPEL2));
    // None is ignored [observed: jip8].
    q(&mut g, "SetAddictionEffect TestChem 0");
    assert_eq!(n(&mut g, "GetAddictionEffect TestChem"), f64::from(SPEL2));
    // A script effect's script; not a script effect, none [documented: "script-type"].
    assert_eq!(n(&mut g, "GetBaseEffectScript TestScriptEffect"), f64::from(SCPT));
    q(&mut g, "SetBaseEffectScript TestScriptEffect TestScript2");
    assert_eq!(n(&mut g, "GetBaseEffectScript TestScriptEffect"), f64::from(SCPT2));
    q(&mut g, "SetBaseEffectScript TestEffect TestScript2");
    assert_eq!(n(&mut g, "GetBaseEffectScript TestEffect"), 0.0);
    // Base effect flags, read by GetBaseEffectFlag; 11, not in the table, too [observed:
    // jip8].
    assert_eq!(n(&mut g, "GetBaseEffectFlag TestEffect 2"), 1.0);
    q(&mut g, "SetBaseEffectFlag TestEffect 2 0");
    q(&mut g, "SetBaseEffectFlag TestEffect 0 1");
    q(&mut g, "SetBaseEffectFlag TestEffect 11 1");
    assert_eq!((n(&mut g, "GetBaseEffectFlag TestEffect 2"), n(&mut g, "GetBaseEffectFlag TestEffect 0"), n(&mut g, "GetBaseEffectFlag TestEffect 11")), (0.0, 1.0, 1.0));
    // Effect flags of each kind, read by GetEffectFlag; any bit 0-7 [observed: jip8].
    assert_eq!(n(&mut g, "GetEffectFlag TestChem 1"), 1.0);
    q(&mut g, "SetEffectFlag TestChem 2 1");
    q(&mut g, "SetEffectFlag TestChem 1 0");
    assert_eq!((n(&mut g, "GetEffectFlag TestChem 1"), n(&mut g, "GetEffectFlag TestChem 2")), (0.0, 1.0));
    // An actor effect's SPIT flags and an object effect's ENIT flags are the record's
    // own, and a set keeps their other bits.
    assert_eq!((n(&mut g, "GetEffectFlag TestAbility 4"), n(&mut g, "GetEffectFlag TestAbility 7")), (1.0, 0.0));
    assert_eq!((n(&mut g, "GetEffectFlag TestEnchant 0"), n(&mut g, "GetEffectFlag TestEnchant 2")), (1.0, 0.0));
    q(&mut g, "SetEffectFlag TestAbility 7 1");
    q(&mut g, "SetEffectFlag TestAbility 1 1");
    q(&mut g, "SetEffectFlag TestAbility 8 1");
    assert_eq!((n(&mut g, "GetEffectFlag TestAbility 7"), n(&mut g, "GetEffectFlag TestAbility 1"), n(&mut g, "GetEffectFlag TestAbility 8")), (1.0, 1.0, 0.0));
    assert_eq!(n(&mut g, "GetEffectFlag TestAbility 4"), 1.0);
    q(&mut g, "SetEffectFlag TestEnchant 2 1");
    assert_eq!((n(&mut g, "GetEffectFlag TestEnchant 2"), n(&mut g, "GetEffectFlag TestEnchant 0")), (1.0, 1.0));
}

#[test]
fn weapons() {
    let f = fixture();
    let mut g = game(&f);
    // VNAM [documented: 0 loud, 1 normal, 2 silent].
    assert_eq!(n(&mut g, "GetWeaponDetectionSoundLevel TestGun"), 1.0);
    q(&mut g, "SetWeaponDetectionSoundLevel TestGun 2");
    assert_eq!(n(&mut g, "GetWeaponDetectionSoundLevel TestGun"), 2.0);
    q(&mut g, "SetWeaponDetectionSoundLevel TestGun 9");
    assert_eq!(n(&mut g, "GetWeaponDetectionSoundLevel TestGun"), 2.0);
    // Mods, read by GetWeaponItemMod and the rest; left out, none [documented].
    q(&mut g, "SetWeaponItemMod TestGun 2 TestMod");
    assert_eq!(n(&mut g, "GetWeaponItemMod 2 TestGun"), f64::from(IMOD));
    q(&mut g, "SetWeaponItemMod TestGun 2");
    assert_eq!(n(&mut g, "GetWeaponItemMod 2 TestGun"), 0.0);
    q(&mut g, "SetWeaponItemModEffect TestGun 3 11");
    q(&mut g, "SetWeaponItemModValue TestGun 3 6.5");
    assert_eq!((n(&mut g, "GetWeaponItemModEffect 3 TestGun"), n(&mut g, "GetWeaponItemModValue1 3 TestGun")), (11.0, 6.5));
    // 0 is ignored, 15 stored [observed: jip8].
    q(&mut g, "SetWeaponItemModEffect TestGun 3 0");
    assert_eq!(n(&mut g, "GetWeaponItemModEffect 3 TestGun"), 11.0);
    q(&mut g, "SetWeaponItemModEffect TestGun 1 15");
    assert_eq!(n(&mut g, "GetWeaponItemModEffect 1 TestGun"), 15.0);
    // The slot with the type [documented: WeaponHasModType].
    assert_eq!(n(&mut g, "WeaponHasModType TestGun 11"), 3.0);
    assert_eq!(n(&mut g, "WeaponHasModType TestGun 3"), 0.0);
    // On hit, semi-auto delays, models, reload animation; read by the VM's getters.
    q(&mut g, "SetWeaponOnHitSetting TestGun 2");
    assert_eq!(n(&mut g, "GetWeaponOnHitSetting TestGun"), 2.0);
    q(&mut g, "SetWeaponSemiAutoFireDelay TestGun 1 0.5");
    q(&mut g, "SetWeaponSemiAutoFireDelay TestGun 0 0.25");
    assert_eq!((n(&mut g, "GetWeaponSemiAutoFireDelay TestGun 1"), n(&mut g, "GetWeaponSemiAutoFireDelay TestGun 0")), (0.5, 0.25));
    q(&mut g, "SetWeaponModel TestGun 3 \"weapons\\gun13.nif\"");
    assert_eq!((s(&mut g, "GetWeaponModel TestGun 3"), s(&mut g, "GetWeaponModel TestGun 0")), ("weapons\\gun13.nif".to_string(), "weapons\\gun.nif".to_string()));
    q(&mut g, "SetWeaponModReloadAnim TestGun 8");
    assert_eq!(n(&mut g, "GetWeaponModReloadAnim TestGun"), 8.0);
    // Embedded weapon: the condition actor values 0-6, and the node.
    q(&mut g, "SetEmbeddedWeaponAV TestGun BrainCondition");
    assert_eq!(g.num_field(FormId(WEAP), "DNAM", "embedded_weapon_actor_value"), 6.0);
    q(&mut g, "SetEmbeddedWeaponAV TestGun Luck");
    assert_eq!(g.num_field(FormId(WEAP), "DNAM", "embedded_weapon_actor_value"), 6.0);
    q(&mut g, "SetEmbeddedWeaponNode TestGun \"ProjectileNode_R\"");
}

#[test]
fn idles_and_body_parts() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!((n(&mut g, "GetIdleLoopTimes TestIdle 0"), n(&mut g, "GetIdleLoopTimes TestIdle 1")), (1.0, 3.0));
    q(&mut g, "SetIdleLoopTimes TestIdle 2 4");
    assert_eq!((n(&mut g, "GetIdleLoopTimes TestIdle 0"), n(&mut g, "GetIdleLoopTimes TestIdle 1")), (2.0, 4.0));
    // The min a u8, the max not below it [observed: jip8, 300 2 reads 255 255].
    q(&mut g, "SetIdleLoopTimes TestIdle 300 2");
    assert_eq!((n(&mut g, "GetIdleLoopTimes TestIdle 0"), n(&mut g, "GetIdleLoopTimes TestIdle 1")), (255.0, 255.0));
    // The torso's limb model; the head has none; no brain part.
    assert_eq!(s(&mut g, "GetBodyPartReplacementModel TestBodyParts 0"), "torso_rep.nif");
    assert_eq!(s(&mut g, "GetBodyPartReplacementModel TestBodyParts 1"), "");
    q(&mut g, "SetBodyPartReplacementModel TestBodyParts 1 \"head_rep.nif\"");
    assert_eq!(s(&mut g, "GetBodyPartReplacementModel TestBodyParts 1"), "head_rep.nif");
    q(&mut g, "SetBodyPartReplacementModel TestBodyParts 13 \"brain.nif\"");
    assert_eq!(s(&mut g, "GetBodyPartReplacementModel TestBodyParts 13"), "");
    // Names, read by GetBodyPartName [documented: SetBodyPartName's example].
    assert_eq!(s(&mut g, "GetBodyPartName TestBodyParts 0"), "Body");
    q(&mut g, "SetBodyPartName TestBodyParts 0 \"Chassis\"");
    assert_eq!(s(&mut g, "GetBodyPartName TestBodyParts 0"), "Chassis");
    assert_eq!(s(&mut g, "GetBodyPartName TestBodyParts2 0"), "Torso");
}

#[test]
fn cells_and_water() {
    let f = fixture();
    let mut g = game(&f);
    // DATA 0-7, LNAM 8-16, read by GetCellFlag [documented: the tables].
    assert_eq!((n(&mut g, "GetCellFlag TestCell 1"), n(&mut g, "GetCellFlag TestCell 8"), n(&mut g, "GetCellFlag TestCell 9")), (1.0, 1.0, 0.0));
    q(&mut g, "SetCellFlag TestCell 1 0");
    q(&mut g, "SetCellFlag TestCell 5 1");
    q(&mut g, "SetCellFlag TestCell 16 1");
    q(&mut g, "SetCellFlag TestCell 4 1");
    assert_eq!(n(&mut g, "GetCellFlag TestCell 4"), 1.0);
    assert_eq!((n(&mut g, "GetCellFlag TestCell 1"), n(&mut g, "GetCellFlag TestCell 5"), n(&mut g, "GetCellFlag TestCell 16")), (0.0, 1.0, 1.0));
    // Water form and effect, read by GetCellWaterForm and GetWaterFormEffect.
    assert_eq!(n(&mut g, "GetCellWaterForm TestCell"), f64::from(WATR));
    q(&mut g, "SetCellWaterForm TestCell TestWater2");
    assert_eq!(n(&mut g, "GetCellWaterForm TestCell"), f64::from(WATR2));
    assert_eq!(n(&mut g, "GetWaterFormEffect TestWater"), f64::from(SPEL));
    q(&mut g, "SetWaterFormEffect TestWater TestAbility2");
    assert_eq!(n(&mut g, "GetWaterFormEffect TestWater"), f64::from(SPEL2));
    // Water traits [documented: the table].
    assert_eq!((n(&mut g, "GetWaterTrait TestWater 0"), n(&mut g, "GetWaterTrait TestWater 1")), (75.0, 2.0));
    assert_eq!((n(&mut g, "GetWaterTrait TestWater 6"), n(&mut g, "GetWaterTrait TestWater 41"), n(&mut g, "GetWaterTrait TestWater 50")), (6.0, 41.0, 50.0));
    assert_eq!((n(&mut g, "GetWaterTrait TestWater 12"), n(&mut g, "GetWaterTrait TestWater 14")), (10_020_030.0, 255_000_128.0));
    assert_eq!((n(&mut g, "GetWaterTrait TestWater 9"), n(&mut g, "GetWaterTrait TestWater 51")), (0.0, 0.0));
}

