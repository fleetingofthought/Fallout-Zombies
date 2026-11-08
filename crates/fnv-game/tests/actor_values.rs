//! Actor values from records and formulas, and their pools, on a synthetic plugin (no
//! game data). docs/spec-notes/actor-values.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::actor_values as av;
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC_GUARD: u32 = 0x830;
const NPC_COPY: u32 = 0x831;
const CREA: u32 = 0x832;
const CONT: u32 = 0x861;
const CELL: u32 = 0x200;
const GUARD_REF: u32 = 0x320;
const COPY_REF: u32 = 0x321;
const CREA_REF: u32 = 0x322;
const CONT_REF: u32 = 0x323;
const PLAYER: u32 = 0x14;

const GET: u16 = 0x100E;
const SET: u16 = 0x100F;
const MOD: u16 = 0x1010;
const FORCE: u16 = 0x110E;
const DAMAGE: u16 = 0x1181;
const RESTORE: u16 = 0x1182;
const GET_BASE: u16 = 0x1115;
const GET_PERMANENT: u16 = 0x11EF;
const RESET_HEALTH: u16 = 0x1150;
const GET_HEALTH_PERCENTAGE: u16 = 0x11AF;
const REWARD_KARMA: u16 = 0x1204;

/// ACBS: flags, fatigue, barter gold, level, calc min/max, speed multiplier, karma,
/// disposition, template flags.
fn acbs(fatigue: u16, level: u16, speed: u16, karma: f32, template_flags: u16) -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [fatigue, 0, level, 0, 0, speed] {
        d.extend(v.to_le_bytes());
    }
    d.extend(karma.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(template_flags.to_le_bytes());
    sub(b"ACBS", d)
}

fn npc_data(health: i32, special: [u8; 7]) -> fnv_esm::raw::Subrecord {
    sub(b"DATA", [health.to_le_bytes().to_vec(), special.to_vec()].concat())
}

fn dnam(values: [u8; 14], offsets: [u8; 14]) -> fnv_esm::raw::Subrecord {
    sub(b"DNAM", [values, offsets].concat())
}

/// AIDT: aggression, confidence, energy, responsibility, mood, …, assistance (byte 14).
fn aidt(aggression: u8, confidence: u8, energy: u8, responsibility: u8, mood: u8, assistance: u8) -> fnv_esm::raw::Subrecord {
    let mut d = vec![aggression, confidence, energy, responsibility, mood, 0, 0, 0, 0, 0, 0, 0, 0, 0, assistance, 0];
    d.extend(0i32.to_le_bytes());
    sub(b"AIDT", d)
}

fn gmst(fid: u32, edid: &str, v: f32) -> Vec<u8> {
    record(b"GMST", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", v.to_le_bytes().to_vec())])
}

fn placed(sig: &[u8; 4], fid: u32, base: u32) -> Vec<u8> {
    record(sig, 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

const GUARD_SKILLS: [u8; 14] = [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23];
const GUARD_OFFSETS: [u8; 14] = [0, 0, 30, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0];

fn plugin_bytes() -> Vec<u8> {
    // The masters hold the skill bases and no primary or Luck multiplier; here one base
    // differs from its default so the setting is seen to be read.
    let gmsts = [gmst(0x801, "fAVDActionPointsBase", 65.0), gmst(0x802, "fAVDActionPointsMult", 3.0), gmst(0x803, "fAVDSkillSmallGunsBase", 4.0)].concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), acbs(50, 1, 100, 0.0, 0), npc_data(100, [5, 6, 7, 4, 8, 9, 3]), dnam([99; 14], [0; 14])]),
        record(
            b"NPC_",
            0,
            NPC_GUARD,
            &[
                sub(b"EDID", z("Guard")),
                acbs(150, 4, 110, -250.0, 0),
                aidt(1, 3, 40, 70, 5, 2),
                npc_data(50, [6, 5, 4, 3, 7, 8, 2]),
                dnam(GUARD_SKILLS, GUARD_OFFSETS),
            ],
        ),
        // Stats (bit 1) and AI Data (bit 4) from the template; its own are not read.
        record(
            b"NPC_",
            0,
            NPC_COPY,
            &[
                sub(b"EDID", z("GuardCopy")),
                acbs(20, 9, 90, 300.0, 0x12),
                sub(b"TPLT", NPC_GUARD.to_le_bytes().to_vec()),
                aidt(0, 0, 5, 5, 0, 0),
                npc_data(10, [1; 7]),
                dnam([1; 14], [0; 14]),
            ],
        ),
    ]
    .concat();
    // CREA DATA: type, combat, magic and stealth skill, health, unused, damage, attributes.
    let crea_data = [vec![0u8, 65, 50, 40], 120i16.to_le_bytes().to_vec(), vec![0; 2], 9i16.to_le_bytes().to_vec(), vec![8, 7, 6, 2, 3, 4, 5]].concat();
    let crea = record(b"CREA", 0, CREA, &[sub(b"EDID", z("Gecko")), acbs(80, 3, 150, 0.0, 0), sub(b"DATA", crea_data)]);
    let cont = record(b"CONT", 0, CONT, &[sub(b"EDID", z("Box")), sub(b"DATA", [vec![0], 0f32.to_le_bytes().to_vec()].concat())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [placed(b"ACHR", GUARD_REF, NPC_GUARD), placed(b"ACHR", COPY_REF, NPC_COPY), placed(b"ACRE", CREA_REF, CREA), placed(b"REFR", CONT_REF, CONT)].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"GMST", &gmsts),
        top(b"NPC_", &npcs),
        top(b"CREA", &crea),
        top(b"CONT", &cont),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "ActorValues.esm".into()).unwrap()];
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

fn n(x: f64) -> ArgValue {
    ArgValue::Value(Value::Num(x))
}

fn code(c: u16) -> ArgValue {
    ArgValue::Enum(c)
}

/// Call a function by opcode on `on`, failing the test on a call error.
fn call(g: &mut Game, op: u16, on: u32, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(FormId(on)), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    let before = g.events.len();
    let v = g.call_function(&ctx, args);
    for e in &g.events[before..] {
        if let EventKind::CallFailed { name, error, .. } | EventKind::Unhandled { name, unit: error, .. } = &e.kind {
            panic!("0x{op:04X} {name}: {error}");
        }
    }
    match v {
        Value::Num(x) => x,
        Value::Ref(f) => f as f64,
    }
}

fn get(g: &mut Game, on: u32, c: u16) -> f64 {
    call(g, GET, on, &[code(c)])
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn the_records_are_read() {
    let f = fixture();
    let d = &f.data.actors[&FormId(NPC_GUARD)];
    assert_eq!(d.attributes, Some([6, 5, 4, 3, 7, 8, 2]));
    assert_eq!((d.skills, d.skill_offsets), (Some(GUARD_SKILLS), Some(GUARD_OFFSETS)));
    assert_eq!((d.fatigue, d.speed_mult, d.karma), (150, 110, -250.0));
    assert_eq!((d.aggression, d.confidence, d.energy, d.responsibility, d.mood, d.assistance), (Some(1), Some(3), Some(40), Some(70), Some(5), Some(2)));
    let c = &f.data.actors[&FormId(NPC_COPY)];
    assert!(c.template_stats && c.template_ai && !c.template_packages);
    let c = &f.data.actors[&FormId(CREA)];
    assert_eq!((c.attributes, c.combat_skill, c.stealth_skill, c.skills), (Some([8, 7, 6, 2, 3, 4, 5]), Some(65), Some(40), None));
}

#[test]
fn an_npc_starts_at_its_record_and_the_formulas() {
    let f = fixture();
    let mut g = game(&f);
    for (i, want) in [6.0, 5.0, 4.0, 3.0, 7.0, 8.0, 2.0].into_iter().enumerate() {
        assert_eq!(get(&mut g, GUARD_REF, av::STRENGTH + i as u16), want);
    }
    // Skill value plus skill offset.
    for i in 0..14 {
        assert_eq!(get(&mut g, GUARD_REF, av::BARTER + i as u16), (GUARD_SKILLS[i] + GUARD_OFFSETS[i]) as f64);
    }
    assert_eq!(get(&mut g, GUARD_REF, av::ENERGY_WEAPONS), 42.0);
    // ACBS and AIDT.
    assert_eq!(get(&mut g, GUARD_REF, av::FATIGUE), 150.0);
    assert_eq!(get(&mut g, GUARD_REF, av::SPEED_MULTIPLIER), 110.0);
    assert_eq!(get(&mut g, GUARD_REF, av::KARMA), -250.0);
    let ai = [av::AGGRESSION, av::CONFIDENCE, av::ENERGY, av::RESPONSIBILITY, av::MOOD, av::ASSISTANCE];
    for (c, want) in ai.into_iter().zip([1.0, 3.0, 40.0, 70.0, 5.0, 2.0]) {
        assert_eq!(get(&mut g, GUARD_REF, c), want);
    }
    // Derived statistics: Agility 8, Strength 6, Luck 2, Endurance 4, Unarmed 23, level 4.
    assert_eq!(get(&mut g, GUARD_REF, av::ACTION_POINTS), 65.0 + 3.0 * 8.0);
    assert_eq!(get(&mut g, GUARD_REF, av::CARRY_WEIGHT), 150.0 + 10.0 * 6.0);
    assert_eq!(get(&mut g, GUARD_REF, av::CRITICAL_CHANCE), 2.0);
    assert_eq!(get(&mut g, GUARD_REF, av::MELEE_DAMAGE), 3.0);
    assert!(close(get(&mut g, GUARD_REF, av::UNARMED_DAMAGE), 0.5 + 0.05 * 23.0));
    assert_eq!(get(&mut g, GUARD_REF, av::POISON_RESISTANCE), 15.0);
    assert_eq!(get(&mut g, GUARD_REF, av::RAD_RESISTANCE), 6.0);
    // A hand-set NPC: Base Health + (Endurance − 1) × 5, no level part (combat-rules.md §9).
    assert_eq!(get(&mut g, GUARD_REF, av::HEALTH), 50.0 + 3.0 * 5.0);
    // Limb conditions start whole, the rest at nothing.
    for c in av::LIMB_CONDITIONS {
        assert_eq!(get(&mut g, GUARD_REF, c), 100.0);
    }
    for c in [15, 18, 24, 47, 54, 62, 71, 72, 76] {
        assert_eq!(get(&mut g, GUARD_REF, c), 0.0, "code {c}");
    }
}

#[test]
fn a_template_supplies_the_stats_and_ai_data_it_is_flagged_for() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(get(&mut g, COPY_REF, av::STRENGTH), 6.0);
    assert_eq!(get(&mut g, COPY_REF, av::ENERGY_WEAPONS), 42.0);
    assert_eq!(get(&mut g, COPY_REF, av::FATIGUE), 150.0);
    assert_eq!(get(&mut g, COPY_REF, av::SPEED_MULTIPLIER), 110.0);
    assert_eq!(get(&mut g, COPY_REF, av::CONFIDENCE), 3.0);
    assert_eq!(get(&mut g, COPY_REF, av::ENERGY), 40.0);
    // Karma is its own.
    assert_eq!(get(&mut g, COPY_REF, av::KARMA), 300.0);
}

#[test]
fn a_creature_has_attributes_and_its_combat_skill() {
    let f = fixture();
    let mut g = game(&f);
    for (i, want) in [8.0, 7.0, 6.0, 2.0, 3.0, 4.0, 5.0].into_iter().enumerate() {
        assert_eq!(get(&mut g, CREA_REF, av::STRENGTH + i as u16), want);
    }
    for c in [av::BIG_GUNS, av::ENERGY_WEAPONS, av::EXPLOSIVES, av::MELEE_WEAPONS, av::GUNS] {
        assert_eq!(get(&mut g, CREA_REF, c), 65.0);
    }
    assert_eq!(get(&mut g, CREA_REF, av::SNEAK), 40.0);
    // No field: the formula. Barter from Charisma 2 and Luck 5: 2 + 4 + ⌈2.5⌉.
    assert_eq!(get(&mut g, CREA_REF, av::BARTER), 9.0);
    assert_eq!(get(&mut g, CREA_REF, av::HEALTH), 120.0);
    assert_eq!(get(&mut g, CREA_REF, av::FATIGUE), 80.0);
    assert_eq!(get(&mut g, CREA_REF, av::SPEED_MULTIPLIER), 150.0);
    assert_eq!(get(&mut g, CREA_REF, av::CARRY_WEIGHT), 230.0);
    // Its attributes take the script functions too.
    call(&mut g, SET, CREA_REF, &[code(av::STRENGTH), n(3.0)]);
    call(&mut g, MOD, CREA_REF, &[code(av::STRENGTH), n(2.0)]);
    assert_eq!(get(&mut g, CREA_REF, av::STRENGTH), 5.0);
    assert_eq!(call(&mut g, GET_BASE, CREA_REF, &[code(av::STRENGTH)]), 3.0);
    assert_eq!(call(&mut g, GET_PERMANENT, CREA_REF, &[code(av::STRENGTH)]), 5.0);
    assert_eq!(get(&mut g, CREA_REF, av::CARRY_WEIGHT), 200.0);
}

#[test]
fn the_players_skills_come_from_the_formula_and_follow_the_attributes() {
    let f = fixture();
    let mut g = game(&f);
    // S 5, P 6, E 7, C 4, I 8, A 9, L 3: ⌈3 × 0.5⌉ = 2. The record's DNAM (99s) is not read.
    assert_eq!(get(&mut g, PLAYER, av::BARTER), 2.0 + 2.0 * 4.0 + 2.0);
    assert_eq!(get(&mut g, PLAYER, av::MELEE_WEAPONS), 2.0 + 2.0 * 5.0 + 2.0);
    assert_eq!(get(&mut g, PLAYER, av::UNARMED), 2.0 + 2.0 * 7.0 + 2.0);
    // Guns reads fAVDSkillSmallGunsBase, 4 in this plugin.
    assert_eq!(get(&mut g, PLAYER, av::GUNS), 4.0 + 2.0 * 9.0 + 2.0);
    for (skill, attribute) in [(32, av::CHARISMA), (34, av::PERCEPTION), (37, av::INTELLIGENCE), (42, av::AGILITY), (44, av::ENDURANCE)] {
        assert_eq!(av::governing_attribute(skill), Some(attribute));
    }
    assert_eq!(av::governing_attribute(av::STRENGTH), None);
    // A tagged skill.
    g.player.tag_skills.insert(av::BARTER);
    assert_eq!(get(&mut g, PLAYER, av::BARTER), 27.0);
    // The formula reads the attribute as it is now.
    call(&mut g, DAMAGE, PLAYER, &[code(av::CHARISMA), n(2.0)]);
    assert_eq!(get(&mut g, PLAYER, av::BARTER), 23.0);
    call(&mut g, RESTORE, PLAYER, &[code(av::CHARISMA), n(2.0)]);
    call(&mut g, MOD, PLAYER, &[code(av::LUCK), n(2.0)]);
    assert_eq!(get(&mut g, PLAYER, av::BARTER), 28.0);
    assert_eq!(get(&mut g, PLAYER, av::CRITICAL_CHANCE), 5.0);
    // Within its range: Charisma 4 + 20 counts as 10.
    call(&mut g, MOD, PLAYER, &[code(av::CHARISMA), n(20.0)]);
    assert_eq!(get(&mut g, PLAYER, av::CHARISMA), 24.0);
    assert_eq!(get(&mut g, PLAYER, av::SPEECH), 2.0 + 20.0 + 3.0);
    // SetActorValue sets the part beside the formula's: the formula's 25 stays
    // [observed: gaps6, Guns 17 reads 67 after SetActorValue Guns 50].
    call(&mut g, SET, PLAYER, &[code(av::SPEECH), n(50.0)]);
    assert_eq!(get(&mut g, PLAYER, av::SPEECH), 75.0);
    assert_eq!(get(&mut g, PLAYER, av::SPEED_MULTIPLIER), 100.0);
}

/// `SetActorValue Repair 35` on the player reads 35 plus the attributes' part, value and
/// base alike; set again it stays there, 0 leaves the attributes' part, and a ModAV stays
/// on top [observed: o26_setav, 2026-11-06, nvfdoc at level 1, every skill 17 from the
/// attributes: Repair read 52 (base 52) after `player.setav repair 35` from the console
/// and from a script, 52 again after a second one; Science 17 after SetAV 0; Barter
/// ModAV +10 then SetAV 35 read 62, base 52] (issue #1575).
#[test]
fn set_actor_value_on_a_player_skill_adds_the_attributes_part() {
    let f = fixture();
    let mut g = game(&f);
    let formula = get(&mut g, PLAYER, av::REPAIR);
    assert!(formula > 0.0);
    call(&mut g, SET, PLAYER, &[code(av::REPAIR), n(35.0)]);
    assert_eq!(get(&mut g, PLAYER, av::REPAIR), formula + 35.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::REPAIR)]), formula + 35.0);
    call(&mut g, SET, PLAYER, &[code(av::REPAIR), n(35.0)]);
    assert_eq!(get(&mut g, PLAYER, av::REPAIR), formula + 35.0);
    call(&mut g, SET, PLAYER, &[code(av::REPAIR), n(0.0)]);
    assert_eq!(get(&mut g, PLAYER, av::REPAIR), formula);
    let barter = get(&mut g, PLAYER, av::BARTER);
    call(&mut g, MOD, PLAYER, &[code(av::BARTER), n(10.0)]);
    call(&mut g, SET, PLAYER, &[code(av::BARTER), n(35.0)]);
    assert_eq!(get(&mut g, PLAYER, av::BARTER), barter + 35.0 + 10.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::BARTER)]), barter + 35.0);
    // A tagged skill keeps its tag bonus (15 here) [observed: o26_setav, nvfLanius, Guns
    // 15 above the attributes' part after SetAV 50; the tag is a hypothesis].
    g.player.tag_skills.insert(av::SCIENCE);
    let science = get(&mut g, PLAYER, av::SCIENCE);
    call(&mut g, SET, PLAYER, &[code(av::SCIENCE), n(50.0)]);
    assert_eq!(get(&mut g, PLAYER, av::SCIENCE), science + 50.0);
}

#[test]
fn the_pools_are_kept_apart() {
    let f = fixture();
    let mut g = game(&f);
    let c = av::BARTER;
    // Base 10.
    call(&mut g, MOD, GUARD_REF, &[code(c), n(5.0)]);
    assert_eq!(get(&mut g, GUARD_REF, c), 15.0);
    assert_eq!(call(&mut g, GET_BASE, GUARD_REF, &[code(c)]), 10.0);
    assert_eq!(call(&mut g, GET_PERMANENT, GUARD_REF, &[code(c)]), 15.0);
    // SetActorValue replaces the base; the modifier stays on top.
    call(&mut g, SET, GUARD_REF, &[code(c), n(40.0)]);
    assert_eq!(get(&mut g, GUARD_REF, c), 45.0);
    call(&mut g, DAMAGE, GUARD_REF, &[code(c), n(10.0)]);
    assert_eq!(get(&mut g, GUARD_REF, c), 35.0);
    // ForceActorValue moves the permanent modifier.
    call(&mut g, FORCE, GUARD_REF, &[code(c), n(30.0)]);
    assert_eq!(get(&mut g, GUARD_REF, c), 30.0);
    assert_eq!(call(&mut g, GET_BASE, GUARD_REF, &[code(c)]), 40.0);
    assert_eq!(call(&mut g, GET_PERMANENT, GUARD_REF, &[code(c)]), 40.0);
    call(&mut g, RESTORE, GUARD_REF, &[code(c), n(100.0)]);
    assert_eq!(get(&mut g, GUARD_REF, c), 40.0);
    assert_eq!(fnv_game::functions::actor_value_parts(&mut g, FormId(GUARD_REF), c).unwrap(), [40.0, 0.0, 0.0]);

    // An attribute: the world holds the permanent value, so its carry weight rule sees it.
    let p = FormId::PLAYER_REF;
    call(&mut g, MOD, PLAYER, &[code(av::STRENGTH), n(2.0)]);
    assert_eq!(get(&mut g, PLAYER, av::STRENGTH), 7.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::STRENGTH)]), 5.0);
    assert_eq!(g.world.actor_value(p, "strength").unwrap(), 7.0);
    call(&mut g, SET, PLAYER, &[code(av::STRENGTH), n(8.0)]);
    assert_eq!(get(&mut g, PLAYER, av::STRENGTH), 10.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::STRENGTH)]), 8.0);
    assert_eq!(g.world.actor_value(p, "strength").unwrap(), 10.0);
    // A value written to the world directly (the menus) is the permanent value.
    g.world.set_actor_value(p, "strength", 6.0).unwrap();
    assert_eq!(get(&mut g, PLAYER, av::STRENGTH), 6.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::STRENGTH)]), 4.0);
    // The runner's `given` leaves exactly the value.
    fnv_game::functions::set_actor_value_current(&mut g, p, av::STRENGTH, 3.0).unwrap();
    assert_eq!(fnv_game::functions::actor_value_parts(&mut g, p, av::STRENGTH).unwrap(), [3.0, 0.0, 0.0]);
    assert_eq!(fnv_game::functions::actor_value_capped(&mut g, FormId(COPY_REF), av::ENERGY_WEAPONS), 42.0);
}

#[test]
fn reset_health_heals_and_cures_limbs_and_karma_keeps_its_modifier() {
    let f = fixture();
    let mut g = game(&f);
    call(&mut g, DAMAGE, GUARD_REF, &[code(av::HEALTH), n(30.0)]);
    call(&mut g, DAMAGE, GUARD_REF, &[code(27), n(100.0)]);
    assert_eq!(get(&mut g, GUARD_REF, av::HEALTH), 35.0);
    assert!(close(call(&mut g, GET_HEALTH_PERCENTAGE, GUARD_REF, &[]), 35.0 / 65.0));
    call(&mut g, RESET_HEALTH, GUARD_REF, &[]);
    assert_eq!(get(&mut g, GUARD_REF, av::HEALTH), 65.0);
    assert_eq!(get(&mut g, GUARD_REF, 27), 100.0);
    // Health's maximum follows Endurance.
    call(&mut g, MOD, GUARD_REF, &[code(av::ENDURANCE), n(2.0)]);
    assert_eq!(get(&mut g, GUARD_REF, av::HEALTH), 75.0);

    call(&mut g, MOD, PLAYER, &[code(av::KARMA), n(10.0)]);
    call(&mut g, REWARD_KARMA, PLAYER, &[n(25.0)]);
    assert_eq!(get(&mut g, PLAYER, av::KARMA), 35.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::KARMA)]), 25.0);
}

#[test]
fn something_that_is_no_actor_has_no_attributes() {
    let f = fixture();
    let mut g = game(&f);
    let chest = FormId(CONT_REF);
    assert!(fnv_game::functions::actor_value(&mut g, chest, av::STRENGTH).is_err());
    assert_eq!(fnv_game::functions::actor_value(&mut g, chest, av::BARTER).unwrap(), 0.0);
    assert_eq!(fnv_game::functions::actor_value(&mut g, chest, 25).unwrap(), 100.0);
}

#[test]
fn script_names_without_a_shipped_spelling_resolve() {
    // `GetAV PoisonResist` compiles in the original game (capture abilities-1); shipped
    // source never spells code 19, so encodings.json has no name for it.
    let f = fixture();
    let g = game(&f);
    let av = |s: &str| fnv_game::functions::av_code(&g, s);
    assert_eq!(av("PoisonResist"), Ok(19));
    assert_eq!(av("poisonresist"), Ok(19));
    assert_eq!(av("Poison Resistance"), Ok(19));
    assert_eq!(av("CritChance"), Ok(14));
    assert_eq!(av("FireResist"), Ok(52));
    assert_eq!(av("EnergyResist"), Ok(60));
    // The shipped spellings and the xEdit names still win.
    assert_eq!(av("DamageResist"), Ok(18));
    assert_eq!(av("RadResist"), Ok(20));
    assert_eq!(av("Energy"), Ok(2));
    assert!(av("PoisonResistance2").is_err());
}

/// Companion Nerve: a teammate's damage and armour share, 5 % per point of the player's
/// Charisma (Charisma 4 here: 20 %), read within 1 to 10; nothing for anyone else.
#[test]
fn companion_nerve_follows_the_players_charisma() {
    let f = fixture();
    let mut g = game(&f);
    let guard = FormId(GUARD_REF);
    assert_eq!(g.companion_nerve(guard), 0.0, "not a teammate");
    assert_eq!(g.companion_nerve(FormId::PLAYER_REF), 0.0, "the player has none");
    g.extra.entry(guard).or_default().teammate = true;
    assert!((g.companion_nerve(guard) - 0.20).abs() < 1e-9);
    call(&mut g, MOD, PLAYER, &[code(av::CHARISMA), n(20.0)]);
    assert!((g.companion_nerve(guard) - 0.50).abs() < 1e-9, "Charisma 24 counts as 10");
    call(&mut g, MOD, PLAYER, &[code(av::CHARISMA), n(-30.0)]);
    assert!((g.companion_nerve(guard) - 0.05).abs() < 1e-9, "Charisma under 1 counts as 1");
}

#[test]
fn the_character_revision_is_kept_from_an_add_on_copy_of_its_script() {
    // Old World Blues carries VCG04ActivatorScript too, so the game runs that copy.
    let f = fixture();
    let mut g = game(&f);
    let rev = |g: &mut Game, op: u16, on: Option<FormId>, args: &[ArgValue]| {
        let ctx = Ctx { unit: "OldWorldBlues.esm|SCPT 0x001164A4".into(), opcode: op, on, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
        g.call_function(&ctx, args);
    };
    g.player.skill_points.insert(av::BARTER, 10.0);
    call(&mut g, SET, PLAYER, &[code(av::LUCK), n(9.0)]);
    let barter = call(&mut g, GET, PLAYER, &[code(av::BARTER)]);
    let sum: i32 = fnv_game::chargen::SpecialSheet::of_player(&mut g).values.iter().sum();
    rev(&mut g, SET, Some(FormId(PLAYER)), &[code(av::LUCK), n(5.0)]);
    rev(&mut g, SET, Some(FormId(PLAYER)), &[code(av::BARTER), n(0.0)]);
    assert_eq!(call(&mut g, GET, PLAYER, &[code(av::LUCK)]), 9.0);
    assert_eq!(call(&mut g, GET, PLAYER, &[code(av::BARTER)]), barter);
    // SetSPECIALPoints 40: the character menu's attributes page (1048, as Intense
    // Training's), not the Vigor Tester, opens on the kept attributes with all their
    // points (#1031).
    rev(&mut g, 0x1218, None, &[n(40.0)]);
    assert_eq!(g.open_menu_code(), Some(fnv_game::game::menus::TAG_SKILLS));
    let sheet = fnv_game::chargen::SpecialSheet::for_menu(&mut g);
    assert_eq!((sheet.values[6], sheet.total, sheet.remaining()), (9, sum.max(40), sum.max(40) - sum));
}

/// SetSPECIALPoints from any script and Fallout 3's S.P.E.C.I.A.L. book
/// (ShowSPECIALBookMenuParams, `ssbmp`) open the attributes page; only
/// ShowLoveTesterMenuParams opens the Vigor Tester (owner 2026-11-02).
#[test]
fn only_the_love_tester_function_opens_the_vigor_tester() {
    use fnv_game::game::menus;
    let f = fixture();
    let mut g = game(&f);
    // Tale of Two Wastelands' rebuild at Vault 101's exit: SetSPECIALPoints 40.
    call(&mut g, 0x1218, PLAYER, &[n(40.0)]);
    assert_eq!(g.open_menu_code(), Some(menus::TAG_SKILLS));
    assert_eq!(fnv_game::chargen::SpecialSheet::for_menu(&mut g).total, 40);
    g.close_menu();
    // The book: ssbmp 40.
    call(&mut g, 0x11EE, PLAYER, &[n(40.0)]);
    assert_eq!(g.open_menu_code(), Some(menus::TAG_SKILLS));
    assert_eq!(fnv_game::chargen::SpecialSheet::for_menu(&mut g).total, 40);
    g.close_menu();
    call(&mut g, 0x1245, PLAYER, &[n(40.0)]);
    assert_eq!(g.open_menu_code(), Some(menus::LOVE_TESTER));
}

/// The player's speed follows its current Speed Mult: SetActorValue, ModActorValue and
/// ForceActorValue (issue #1364).
#[test]
fn speed_mult_scales_the_players_speed() {
    let f = fixture();
    let mut g = game(&f);
    let p = FormId(PLAYER);
    assert_eq!(g.move_speed_mult(p), 1.0);
    call(&mut g, SET, PLAYER, &[code(av::SPEED_MULTIPLIER), n(200.0)]);
    assert_eq!(g.move_speed_mult(p), 2.0);
    call(&mut g, MOD, PLAYER, &[code(av::SPEED_MULTIPLIER), n(-150.0)]);
    assert_eq!(g.move_speed_mult(p), 0.5);
    call(&mut g, FORCE, PLAYER, &[code(av::SPEED_MULTIPLIER), n(75.0)]);
    assert_eq!(g.move_speed_mult(p), 0.75);
    // Never below standing still.
    call(&mut g, MOD, PLAYER, &[code(av::SPEED_MULTIPLIER), n(-500.0)]);
    assert_eq!(g.move_speed_mult(p), 0.0);
}
