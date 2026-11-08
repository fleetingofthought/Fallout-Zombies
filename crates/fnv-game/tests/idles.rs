//! Idle animations asked for by the rules (`fnv_game::idles`) on a plugin built in
//! memory (no game data): a package's begin and change idles, `PlayIdle`,
//! `IsIdlePlaying`, and one script package at a time. The rules and their sources:
//! docs/spec-notes/idles.md.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::idles::{IdleRequest, IdleSource};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x900u32.to_le_bytes()].concat();
    let payload = write_subrecords(&[sub(b"HEDR", hedr)]);
    [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &[0; 4], &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

const IDLE_WAKE: u32 = 0xA01;
const IDLE_SIT: u32 = 0xA02;
const IDLE_FOLDER: u32 = 0xA03;
const IDLE_OWN: u32 = 0xA04;
const IDLE_UNREAD: u32 = 0xA05;
const IDLE_COWER: u32 = 0xA06;
/// Two leaves under `IDLE_FOLDER`: one for Somebody only, then one for anybody.
const IDLE_LEAF_OWN: u32 = 0xA07;
const IDLE_LEAF_ANY: u32 = 0xA08;
/// A third leaf, of the Idle section (DATA section 0): a standing look.
const IDLE_LEAF_LOOK: u32 = 0xA09;
const P_FIRST: u32 = 0xB01;
const P_SECOND: u32 = 0xB02;
const P_PLAIN: u32 = 0xB03;
const NPC: u32 = 0xC01;
const ACTOR: u32 = 0xD01;
const CELL: u32 = 0xE01;
/// A topic no line belongs to: saying it shows as `EventKind::NoLine`.
const TOPIC: u32 = 0xF01;

/// An IDLE: its file, loop minimum and maximum (the 8-byte DATA form).
fn idle(fid: u32, edid: &str, model: &str, loops: (u8, u8)) -> Vec<u8> {
    record(
        b"IDLE",
        fid,
        &[sub(b"EDID", z(edid)), sub(b"MODL", z(model)), sub(b"ANAM", vec![0; 8]), sub(b"DATA", vec![0x47, loops.0, loops.1, 0, 0, 0, 0, 0])],
    )
}

/// A condition `function(param) == 1` on the subject (the 28-byte CTDA form).
fn ctda(function: u16, param: u32) -> Subrecord {
    let mut d = vec![0u8; 4];
    d.extend(1.0f32.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(param.to_le_bytes());
    d.extend([0u8; 12]);
    sub(b"CTDA", d)
}

/// An IDLE with a file and conditions.
fn idle_if(fid: u32, edid: &str, conds: Vec<Subrecord>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"MODL", z("Characters\\_Male\\IdleAnims\\Own.kf"))];
    subs.extend(conds);
    subs.extend([sub(b"ANAM", vec![0; 8]), sub(b"DATA", vec![7, 0, 0, 0, 0, 0, 0, 0])]);
    record(b"IDLE", fid, &subs)
}

const GET_IS_ID: u16 = 72;
/// GetPlantedExplosive (condition function index).
const GET_PLANTED_EXPLOSIVE: u16 = 0x1AC;
const IDLE_PANIC: u32 = 0xC40;
const IDLE_PANIC_PA: u32 = 0xC41;
const IDLE_PANIC_PLAIN: u32 = 0xC42;

/// An IDLE with a file below `parent` in the tree, after `previous` (ANAM), with conditions.
fn leaf(fid: u32, edid: &str, parent: u32, previous: u32, conds: Vec<Subrecord>) -> Vec<u8> {
    leaf_in(fid, edid, parent, previous, conds, 7)
}

/// As [`leaf`], of animation group section `section` (DATA's first byte).
fn leaf_in(fid: u32, edid: &str, parent: u32, previous: u32, conds: Vec<Subrecord>, section: u8) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"MODL", z("Characters\\_Male\\IdleAnims\\Leaf.kf"))];
    subs.extend(conds);
    subs.extend([sub(b"ANAM", [parent.to_le_bytes(), previous.to_le_bytes()].concat()), sub(b"DATA", vec![section, 0, 0, 0, 0, 0, 0, 0])]);
    record(b"IDLE", fid, &subs)
}

/// A condition `function == value` on the subject, joined to the next by OR when `or`.
fn ctda_eq(function: u16, value: f32, or: bool) -> Subrecord {
    let mut d = vec![u8::from(or), 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0u8; 18]);
    sub(b"CTDA", d)
}

/// IsMoving and GetCurrentAIProcedure (condition function indices).
const IS_MOVING: u16 = 25;
const GET_CURRENT_AI_PROCEDURE: u16 = 143;

/// A Travel package at the editor location with a begin idle, a change idle and an idle
/// collection run in sequence; [`P_SECOND`] also has a begin topic ([`TOPIC`]).
fn pack(fid: u32, edid: &str, begin: Option<u32>, change: Option<u32>, collection: &[u32]) -> Vec<u8> {
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"PKDT", [&0u32.to_le_bytes()[..], &[6, 0], &[0; 6]].concat()),
        sub(b"PLDT", [0i32.to_le_bytes(), 0u32.to_le_bytes(), 0i32.to_le_bytes()].concat()),
        sub(b"PSDT", [&[0xFF, 0xFF, 0, 0xFF][..], &0u32.to_le_bytes()].concat()),
    ];
    if !collection.is_empty() {
        subs.push(sub(b"IDLF", vec![1]));
        subs.push(sub(b"IDLC", vec![collection.len() as u8]));
        subs.push(sub(b"IDLT", 0f32.to_le_bytes().to_vec()));
        subs.push(sub(b"IDLA", collection.iter().flat_map(|i| i.to_le_bytes()).collect()));
    }
    for (marker, idle) in [(b"POBA", begin), (b"POEA", None), (b"POCA", change)] {
        subs.push(sub(marker, vec![]));
        subs.push(sub(b"INAM", idle.unwrap_or(0).to_le_bytes().to_vec()));
        if fid == P_SECOND && marker == b"POBA" {
            subs.push(sub(b"TNAM", TOPIC.to_le_bytes().to_vec()));
        }
    }
    record(b"PACK", fid, &subs)
}

fn plugin_bytes() -> Vec<u8> {
    let idles = [
        idle(IDLE_WAKE, "LooseWake", "Characters\\_Male\\IdleAnims\\Wake.kf", (255, 255)),
        idle(IDLE_SIT, "LooseSit", "Characters\\_Male\\IdleAnims\\Sit.kf", (0, 0)),
        idle(IDLE_FOLDER, "Folder", "Characters\\_Male\\IdleAnims", (0, 0)),
        idle_if(IDLE_OWN, "OnlySomebody", vec![ctda(GET_IS_ID, NPC)]),
        // A function index the runtime has no function for.
        idle_if(IDLE_UNREAD, "Unknown", vec![ctda(4000, 0)]),
        // As FalloutNV.esm's CowerIdles: not moving, fleeing (16) or fleeing a non-combat
        // threat (34).
        idle_if(IDLE_COWER, "Cower", vec![ctda_eq(IS_MOVING, 0.0, false), ctda_eq(GET_CURRENT_AI_PROCEDURE, 16.0, true), ctda_eq(GET_CURRENT_AI_PROCEDURE, 34.0, false)]),
        leaf(IDLE_LEAF_OWN, "LeafOwn", IDLE_FOLDER, 0, vec![ctda(GET_IS_ID, NPC)]),
        leaf(IDLE_LEAF_ANY, "LeafAny", IDLE_FOLDER, IDLE_LEAF_OWN, vec![]),
        leaf_in(IDLE_LEAF_LOOK, "LeafLook", IDLE_FOLDER, IDLE_LEAF_ANY, vec![ctda(GET_IS_ID, NPC)], 0),
        // As FalloutNV.esm's PanicExplosivePlanted: a root of the folder that wants
        // GetPlantedExplosive == 1, its power armour leaf (here: never) before the plain one.
        record(
            b"IDLE",
            IDLE_PANIC,
            &[
                sub(b"EDID", z("PanicExplosivePlanted")),
                sub(b"MODL", z("Characters\\_Male\\IdleAnims")),
                ctda_eq(GET_PLANTED_EXPLOSIVE, 1.0, false),
                sub(b"ANAM", [0u32.to_le_bytes(), IDLE_FOLDER.to_le_bytes()].concat()),
                sub(b"DATA", vec![7, 0, 0, 0, 0, 0, 0, 0]),
            ],
        ),
        leaf(IDLE_PANIC_PA, "PanicGrenadePlantPA", IDLE_PANIC, 0, vec![ctda(GET_IS_ID, 0xBAD)]),
        leaf(IDLE_PANIC_PLAIN, "PanicGrenadePlant", IDLE_PANIC, IDLE_PANIC_PA, vec![]),
    ]
    .concat();
    let packs = [
        pack(P_FIRST, "First", Some(IDLE_WAKE), None, &[IDLE_WAKE, IDLE_SIT]),
        pack(P_SECOND, "Second", Some(IDLE_SIT), Some(IDLE_WAKE), &[]),
        pack(P_PLAIN, "Plain", None, None, &[]),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", NPC, &[sub(b"EDID", z("Somebody")), sub(b"PKID", P_PLAIN.to_le_bytes().to_vec())]),
    ]
    .concat();
    let actor = record(b"ACHR", ACTOR, &[sub(b"EDID", z("SomebodyRef")), sub(b"NAME", NPC.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let cell = [record(b"CELL", CELL, &[sub(b"EDID", z("TestCell"))]), group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &actor))].concat();
    [
        tes4(),
        top(b"NPC_", &npcs),
        top(b"IDLE", &idles),
        top(b"PACK", &packs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Idles.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::new(&f.data, world, GameConfig { strict: true, package_interval: Some(10.0), ..GameConfig::default() })
}

fn call(g: &mut Game, opcode: u16, on: FormId, args: &[ArgValue]) -> Value {
    let ctx = Ctx { unit: "test".into(), opcode, on: Some(on), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, args)
}

const PLAY_IDLE: u16 = 0x1190;
const IS_IDLE_PLAYING: u16 = 0x1070;
const ADD_SCRIPT_PACKAGE: u16 = 0x1097;
const REMOVE_SCRIPT_PACKAGE: u16 = 0x1098;

fn asked(g: &mut Game) -> Vec<(u32, u32, IdleSource)> {
    std::mem::take(&mut g.idle_requests).into_iter().map(|IdleRequest { actor, idle, source }| (actor.0, idle.0, source)).collect()
}

#[test]
fn idle_records_and_package_idle_data_are_read() {
    let f = fixture();
    let d = &f.data;
    assert_eq!(d.idles.len(), 12);
    let wake = &d.idles[&FormId(IDLE_WAKE)];
    assert_eq!((wake.model.as_deref(), wake.loop_min, wake.loop_max, wake.has_file()), (Some("Characters\\_Male\\IdleAnims\\Wake.kf"), 255, 255, true));
    assert!(!d.idles[&FormId(IDLE_FOLDER)].has_file());
    let first = &d.package_defs[&FormId(P_FIRST)];
    assert_eq!(first.events[0].idle, Some(FormId(IDLE_WAKE)));
    assert_eq!(first.events[2].idle, None);
    let c = first.idles.as_ref().unwrap();
    assert_eq!((c.run_in_sequence, c.do_once, &c.idles), (true, false, &vec![FormId(IDLE_WAKE), FormId(IDLE_SIT)]));
}

#[test]
fn an_idle_is_allowed_by_its_own_conditions() {
    let f = fixture();
    let mut g = game(&f);
    let (actor, player) = (FormId(ACTOR), FormId::PLAYER_REF);
    // No conditions: anybody.
    assert!(g.idle_allowed(actor, FormId(IDLE_SIT)) && g.idle_allowed(player, FormId(IDLE_SIT)));
    // GetIsID Somebody == 1.
    assert_eq!(f.data.idles[&FormId(IDLE_OWN)].conditions.as_ref().map(Vec::len), Some(1));
    assert!(g.idle_allowed(actor, FormId(IDLE_OWN)));
    assert!(!g.idle_allowed(player, FormId(IDLE_OWN)));
    // A function the runtime does not have answers 0, as in any condition: `== 1` fails.
    // The idle can still be played by name.
    assert_eq!(f.data.idles[&FormId(IDLE_UNREAD)].conditions.as_ref().map(Vec::len), Some(1));
    assert!(!g.idle_allowed(actor, FormId(IDLE_UNREAD)));
    assert!(g.ask_idle(actor, FormId(IDLE_UNREAD), IdleSource::Script));
    // Not an idle at all.
    assert!(!g.idle_allowed(actor, FormId(NPC)));
}

#[test]
fn play_idle_takes_the_editor_id_and_ignores_what_is_not_an_animation() {
    let f = fixture();
    let mut g = game(&f);
    let actor = FormId(ACTOR);
    call(&mut g, PLAY_IDLE, actor, &[ArgValue::Str("loosesit".into())]);
    assert_eq!(asked(&mut g), [(ACTOR, IDLE_SIT, IdleSource::Script)]);
    assert!(g.events.iter().any(|e| e.kind == EventKind::IdleAsked { actor, idle: FormId(IDLE_SIT), source: IdleSource::Script }));
    // An inner node of the idle tree names a folder: nothing to play.
    call(&mut g, PLAY_IDLE, actor, &[ArgValue::Str("Folder".into())]);
    assert!(asked(&mut g).is_empty());
    let reason = g.events.iter().find_map(|e| match &e.kind {
        EventKind::IdleNotPlayed { idle, reason, .. } if *idle == FormId(IDLE_FOLDER) => Some(*reason),
        _ => None,
    });
    assert_eq!(reason, Some("names no animation file"));
    // A name that is no idle at all (here an NPC's).
    call(&mut g, PLAY_IDLE, actor, &[ArgValue::Str("Somebody".into())]);
    assert!(asked(&mut g).is_empty());
}

#[test]
fn is_idle_playing_answers_what_the_animating_layer_reports() {
    let f = fixture();
    let mut g = game(&f);
    let actor = FormId(ACTOR);
    // Asked for, but nothing animates: not playing.
    call(&mut g, PLAY_IDLE, actor, &[ArgValue::Str("LooseSit".into())]);
    assert_eq!(call(&mut g, IS_IDLE_PLAYING, actor, &[]).num(), 0.0);
    g.set_idle_playing(actor, Some(FormId(IDLE_SIT)));
    assert_eq!(call(&mut g, IS_IDLE_PLAYING, actor, &[]).num(), 1.0);
    assert_eq!(g.idle_playing(actor), Some(FormId(IDLE_SIT)));
    g.set_idle_playing(actor, None);
    assert_eq!(call(&mut g, IS_IDLE_PLAYING, actor, &[]).num(), 0.0);
}

#[test]
fn a_package_asks_for_its_begin_and_change_idles() {
    let f = fixture();
    let mut g = game(&f);
    let player = FormId::PLAYER_REF;
    let form = |p: u32| ArgValue::Value(Value::Ref(p));
    // The first script package starts: its begin idle.
    call(&mut g, ADD_SCRIPT_PACKAGE, player, &[form(P_FIRST)]);
    assert_eq!(asked(&mut g), [(player.0, IDLE_WAKE, IdleSource::PackageBegin)]);
    let (package, collection) = g.package_idles(player).unwrap();
    assert_eq!((package, collection.idles.len()), (FormId(P_FIRST), 2));
    // The same package given again changes nothing.
    call(&mut g, ADD_SCRIPT_PACKAGE, player, &[form(P_FIRST)]);
    assert!(asked(&mut g).is_empty());
    // Another one replaces it: the old package has no change idle, the new one's begin
    // idle is asked for.
    call(&mut g, ADD_SCRIPT_PACKAGE, player, &[form(P_SECOND)]);
    assert_eq!(asked(&mut g), [(player.0, IDLE_SIT, IdleSource::PackageBegin)]);
    assert!(g.package_idles(player).is_none());
    // An actor holds one script package: removing it leaves none (the first is not
    // back), and the package left asks for its change idle.
    call(&mut g, ADD_SCRIPT_PACKAGE, player, &[form(P_SECOND)]);
    call(&mut g, REMOVE_SCRIPT_PACKAGE, player, &[]);
    assert_eq!(asked(&mut g), [(player.0, IDLE_WAKE, IdleSource::PackageChange)]);
    assert_eq!(g.extra[&player].current_package, None);
    assert!(g.extra[&player].script_package.is_none());
}

/// A package's begin topic is said after its result script and before its begin idle
/// is asked for (docs/spec-notes/packages.md §7.2).
#[test]
fn a_package_says_its_begin_topic() {
    let f = fixture();
    let mut g = game(&f);
    let actor = FormId(ACTOR);
    assert_eq!(f.data.package_defs[&FormId(P_SECOND)].events[0].topic, Some(FormId(TOPIC)));
    let from = g.events.len();
    call(&mut g, ADD_SCRIPT_PACKAGE, actor, &[ArgValue::Value(Value::Ref(P_SECOND))]);
    let kinds: Vec<&EventKind> = g.events[from..].iter().map(|e| &e.kind).collect();
    let said = kinds.iter().position(|k| **k == EventKind::NoLine { speaker: actor, topic: FormId(TOPIC) }).expect("the topic is said");
    let idle = kinds.iter().position(|k| matches!(k, EventKind::IdleAsked { idle, .. } if *idle == FormId(IDLE_SIT))).expect("the idle is asked for");
    assert!(said < idle);
    // A package without a topic says nothing.
    let from = g.events.len();
    call(&mut g, ADD_SCRIPT_PACKAGE, actor, &[ArgValue::Value(Value::Ref(P_FIRST))]);
    assert!(!g.events[from..].iter().any(|e| matches!(e.kind, EventKind::NoLine { .. })));
}

/// The tree's root idles that want the flee procedure play on a fleeing actor standing
/// still (docs/spec-notes/idles.md §13).
#[test]
fn a_fleeing_actor_standing_still_gets_the_cower_idle() {
    let f = fixture();
    let mut g = game(&f);
    let actor = FormId(ACTOR);
    let folder = "characters\\_male";
    assert_eq!(g.flee_idle(actor, folder, &|_| false), None, "not fleeing");
    g.start_combat(actor, FormId::PLAYER_REF);
    assert_eq!(g.flee_idle(actor, folder, &|_| false), None, "fighting, not fleeing");
    assert!(g.wants_weapon_out(actor), "fighting: the weapon is out");
    g.start_flight(actor, FormId::PLAYER_REF, [0.0; 3]);
    assert!(!g.wants_weapon_out(actor), "fleeing: the weapon is holstered");
    assert_eq!(g.flee_idle(actor, folder, &|_| false), Some(FormId(IDLE_COWER)));
    assert_eq!(g.flee_idle(actor, folder, &|i| i == FormId(IDLE_COWER)), None, "held back");
    assert_eq!(g.flee_idle(actor, "creatures\\dog", &|_| false), None, "another folder");
    // On its way it stands; at its flee spot a person hides crouched (combat-ai.md §12.2.1).
    assert!(!g.at_flee_spot(actor) && !g.sneaks(actor));
    let now = g.time;
    g.awareness.tactics.flights.get_mut(&actor).unwrap().arrived = Some(now);
    assert!(g.at_flee_spot(actor) && g.sneaks(actor));
    g.end_flight(actor);
    assert_eq!(g.flee_idle(actor, folder, &|_| false), None, "fighting again");
    assert!(g.wants_weapon_out(actor), "fighting again: drawn again");
}

/// PickIdle: the actor is queued for the animating layer, which knows its folder; the
/// tree's pick is the first leaf whose conditions pass (script-stubs.md §3.19).
#[test]
fn pick_idle_walks_the_tree() {
    let f = fixture();
    let mut g = game(&f);
    let actor = FormId(ACTOR);
    call(&mut g, 0x1064, actor, &[]);
    call(&mut g, 0x1064, actor, &[]);
    assert_eq!(g.idle_picks, vec![actor], "each actor once");
    assert!(asked(&mut g).is_empty(), "nothing played before the animating layer picks");
    let folder = "characters\\_male";
    assert_eq!(g.pick_idle(actor, folder, &|_| false), Some(FormId(IDLE_LEAF_OWN)));
    assert_eq!(g.pick_idle(FormId::PLAYER_REF, folder, &|_| false), Some(FormId(IDLE_LEAF_ANY)), "the player is not Somebody");
    assert_eq!(g.pick_idle(actor, folder, &|i| i == FormId(IDLE_LEAF_OWN)), Some(FormId(IDLE_LEAF_ANY)), "held back");
    assert_eq!(g.pick_idle(actor, "creatures\\dog", &|_| false), None, "another folder: nothing");
}

/// The panic of a victim of a planted explosive: the root naming GetPlantedExplosive,
/// its first leaf whose conditions pass, only while the explosive is live
/// (pickpocket.md §5).
#[test]
fn the_panic_idle_is_the_root_naming_get_planted_explosive() {
    let f = fixture();
    let mut g = game(&f);
    let actor = FormId(ACTOR);
    let folder = "characters\\_male";
    assert_eq!(g.panic_idle(actor, folder, &|_| false), None, "nothing planted");
    let start = g.time;
    g.plant_explosive(actor, FormId(0x4330), fnv_game::pickpocket::Explosive { damage: 125.0, ..Default::default() }).unwrap();
    assert_eq!(g.panic_idle(actor, folder, &|_| false), None, "before the freak-out time");
    g.time = start + 0.2;
    assert_eq!(g.panic_idle(actor, folder, &|_| false), Some(FormId(IDLE_PANIC_PLAIN)), "the power armour leaf's conditions fail");
    assert_eq!(g.panic_idle(actor, "creatures\\dog", &|_| false), None, "a creature's folder has none");
    // The tree's other walks pass the panic root by.
    assert_eq!(g.pick_idle(FormId::PLAYER_REF, folder, &|_| false), Some(FormId(IDLE_LEAF_ANY)));
    g.time = start + 2.1;
    g.tick_planted();
    assert_eq!(g.panic_idle(actor, folder, &|_| false), None, "gone off");
}

/// A standing look: the tree walked as for PickIdle, only the Idle section's leaves
/// counted (#1542).
#[test]
fn standing_idle_takes_only_idle_section_leaves() {
    let f = fixture();
    let mut g = game(&f);
    let folder = "characters\\_male";
    assert_eq!(g.standing_idle(FormId(ACTOR), folder), Some(FormId(IDLE_LEAF_LOOK)), "the special idles before it are passed over");
    assert_eq!(g.standing_idle(FormId::PLAYER_REF, folder), None, "its conditions fail: no look");
    assert_eq!(g.standing_idle(FormId(ACTOR), "creatures\\dog"), None, "another folder: nothing");
}
