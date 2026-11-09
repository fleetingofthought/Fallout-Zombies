//! ShowOff NVSE batch 5 (#1010; docs/spec-notes/ext-showoff.md §8) on a synthetic plugin
//! written for the test (no game data, made-up names): the level-up menu's asks, the
//! pause menu ask, live explosions, the shot switches, fast travel, AP costs and
//! RemoveAllItemsShowOff's flags.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{menus, Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const GUN: u32 = 0x800;
const PIP: u32 = 0x801;
const VEST: u32 = 0x802;
const BOOM: u32 = 0x810;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

/// WEAP DNAM: the animation type first (3 Pistol - Ballistic) [documented: schemas/fnv
/// objects.json WeaponDNAM]; the rest left zero.
fn weap(id: u32, edid: &str, anim: u32) -> Vec<u8> {
    let data = [0i32.to_le_bytes().to_vec(), 100i32.to_le_bytes().to_vec(), 1.0f32.to_le_bytes().to_vec(), vec![0; 3]].concat();
    let mut dnam = anim.to_le_bytes().to_vec();
    dnam.resize(204, 0);
    record(b"WEAP", 0, id, &[sub(b"EDID", z(edid)), sub(b"DATA", data), sub(b"DNAM", dnam)])
}

fn armo(id: u32, edid: &str, biped: u32, general: u8) -> Vec<u8> {
    let bmdt = [biped.to_le_bytes().to_vec(), vec![general, 0, 0, 0]].concat();
    let data = [5i32.to_le_bytes().to_vec(), 100i32.to_le_bytes().to_vec(), 1.0f32.to_le_bytes().to_vec()].concat();
    record(b"ARMO", 0, id, &[sub(b"EDID", z(edid)), sub(b"BMDT", bmdt), sub(b"DATA", data)])
}

fn fixture() -> Fixture {
    let expl = record(b"EXPL", 0, BOOM, &[sub(b"EDID", z("TestBoom"))]);
    let bytes = [
        tes4(),
        top(b"WEAP", &weap(GUN, "TestPistol", 3)),
        top(b"ARMO", &[armo(PIP, "TestPip", 1 << 6, 0x40), armo(VEST, "TestVest", 1 << 2, 0)].concat()),
        top(b"EXPL", &expl),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "ShowOff5Test.esm".into()).unwrap()];
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

#[test]
fn the_level_up_menu_and_the_pause_menu_take_the_asks() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetLevelUpMenuCurrentPage"), -1.0, "closed");
    assert_eq!(n(&mut g, "GetLevelUpMenuPoints"), -1.0);
    assert_eq!(n(&mut g, "SetLevelUpMenuCurrentPage 1"), 0.0, "closed");
    assert_eq!(n(&mut g, "SetLevelUpMenuCanExitEarly 1"), 0.0);
    // ShowSkillMenu 5: the skills page alone, five points [documented].
    assert_eq!(n(&mut g, "ShowSkillMenu 5"), 1.0);
    assert!(g.menu_open(menus::LEVEL_UP));
    let ask = g.showoff.b5.levelup.open_as.clone().unwrap();
    assert!(!ask.perks_only);
    assert_eq!(ask.count, Some(5));
    // Read at once, before the engine's first frame [observed 2026-11-08:
    // showoff5.levelup.skillmenu].
    assert_eq!((n(&mut g, "GetLevelUpMenuPage"), n(&mut g, "GetLevelUpMenuPoints"), n(&mut g, "GetLevelUpMenuPoints 1")), (0.0, 5.0, 0.0));
    // The engine reports the page and points each frame; here by hand.
    g.showoff.b5.levelup.page = Some(0);
    g.showoff.b5.levelup.points = [5, 2, 0, 0];
    assert_eq!(n(&mut g, "GetLevelUpMenuPage"), 0.0);
    assert_eq!(n(&mut g, "GetLevelUpMenuPoints 0 1"), 2.0);
    assert_eq!(n(&mut g, "SetLevelUpMenuCurrentPage 0"), 0.0, "already there");
    assert_eq!(n(&mut g, "SetLevelUpMenuPage 2"), 1.0);
    assert_eq!(g.showoff.b5.levelup.page_request, Some(2));
    assert_eq!(n(&mut g, "SetLevelUpMenuCanExitEarly 1"), 1.0);
    assert!(g.showoff.b5.levelup.can_exit_early);
    // With a menu open, no second menu [hypothesis].
    assert_eq!(n(&mut g, "ShowSkillMenu 5"), 0.0);
    g.close_menu();
    // ShowSkillMenu 0 0: no points and not to be shown anyway.
    assert_eq!(n(&mut g, "ShowSkillMenu 0 0"), 0.0);
    let asks = g.showoff.b5.pause_asks;
    q(&mut g, "ShowPauseMenu 3");
    assert_eq!((g.showoff.b5.pause_asks, g.showoff.b5.pause_mode), (asks + 1, 3));
}

#[test]
fn live_explosions_and_the_shot_switches() {
    let f = fixture();
    let mut g = game(&f);
    let r = g.explosion_placed(FormId(BOOM), Some(FormId::PLAYER_REF), [0.0; 3], 256.0).expect("placed");
    let on = |g: &mut Game, body: &str| {
        let ctx = RunCtx { self_ref: Some(r), ..RunCtx::default() };
        match g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue ({body})\nEnd\n"), ctx).unwrap() {
            XVal::Num(x) => x,
            XVal::Ref(x) => f64::from(x),
            v => panic!("{v:?}"),
        }
    };
    assert_eq!(on(&mut g, "GetExplosionRefRadius"), 256.0);
    assert_eq!(n(&mut g, "player.GetExplosionRefRadius"), -1.0, "no explosion [observed]");
    assert_eq!(on(&mut g, "GetExplosionRefSource"), f64::from(FormId::PLAYER_REF.0));
    assert_eq!(on(&mut g, "SetExplosionRefRadius 100.5"), 1.0);
    assert_eq!(g.explosion_radius(r), Some(100.5));
    assert_eq!(n(&mut g, "SetForceDrawHitscanProjectiles 1"), 0.0);
    assert_eq!(n(&mut g, "SetForceDrawHitscanProjectiles"), 1.0, "-1 reads");
    assert_eq!(n(&mut g, "SetProjectileTracerChanceOverride 0"), -1.0);
    assert_eq!(n(&mut g, "SetProjectileTracerChanceOverride"), 0.0, "-2 changes nothing");
    assert_eq!(g.showoff.b5.tracer_override, Some(false));
    assert_eq!(n(&mut g, "SetProjectileTracerChanceOverride -1"), 0.0);
    assert_eq!(g.showoff.b5.tracer_override, None);
}

#[test]
fn travel_ap_and_removing_items() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetPCHasScriptedFastTravelOverride"), 0.0);
    q(&mut g, "EnableFastTravel 0");
    assert_eq!(n(&mut g, "GetPCHasScriptedFastTravelOverride"), 1.0);
    assert_eq!(n(&mut g, "GetPCCanFastTravel"), 0.0);
    // The pistol's AP: fActionPointsAttackPistol, 20 without the setting.
    assert_eq!(n(&mut g, "GetCalculatedAPCost 0 TestPistol"), 20.0);
    assert_eq!(n(&mut g, "GetCalculatedAPCost 1 TestPistol") + n(&mut g, "GetCalculatedAPCost 2 TestPistol"), 0.0, "a gun's 1 and 2 [observed]");
    assert_eq!(s(&mut g, "string_var t\nlet t := ToANSIChar 16\nSetFunctionValue t"), "");
    // RemoveAllItemsShowOff: the non-playable item stays unless flag 8.
    let inv = g.world.inventory_mut(FormId::PLAYER_REF).unwrap();
    inv.add(FormId(PIP), 1);
    inv.add(FormId(VEST), 1);
    assert_eq!(n(&mut g, "player.RemoveAllItemsShowOff"), 1.0);
    let count = |g: &mut Game, i: u32| g.world.get(FormId::PLAYER_REF).unwrap().inventory.as_ref().map_or(0, |inv| inv.count(FormId(i)));
    assert_eq!((count(&mut g, PIP), count(&mut g, VEST)), (1, 0));
    assert_eq!(n(&mut g, "player.RemoveAllItemsShowOff 8"), 1.0);
    assert_eq!(count(&mut g, PIP), 0);
}

fn s(g: &mut Game, body: &str) -> String {
    match q(g, body) {
        XVal::Str(t) => t,
        v => panic!("{body}: {v:?}"),
    }
}
