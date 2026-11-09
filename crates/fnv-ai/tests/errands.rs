//! Escort, Find, Use Item At, Ambush and Flee Not Combat packages (`fnv_ai::errand`) on a
//! plugin and a navmesh built in memory (no game data): one interior cell of 20 × 20
//! squares, markers, items, a chair and actors with one package each. The rules and
//! their sources: docs/spec-notes/packages.md §3.1.

use fnv_ai::errand::{Act, Errand};
use fnv_ai::{Activity, Ai, AiConfig, AiEvent};
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::package::{flags, type_flags};
use fnv_game::GameData;
use fnv_nav::record::{NavMeshRecord, Triangle};
use fnv_nav::{NavGraph, Space};
use fnv_script::FunctionTable;
use fnv_sim::furniture_facts::{ActorKind, Fit, FurnitureFacts};
use std::sync::Arc;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

// ---- plugin builders ---------------------------------------------------------------

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

fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x900u32.to_le_bytes()].concat();
    let payload = write_subrecords(&[sub(b"HEDR", hedr)]);
    [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &[0; 4], &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn glob(fid: u32, edid: &str, v: f32) -> Vec<u8> {
    record(b"GLOB", fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", v.to_le_bytes().to_vec())])
}

/// A location or target: (type, form or code, radius or count).
type Loc = (i32, u32, i32);

/// A package without schedule or conditions: general flags, type, type-specific flags,
/// location 1, location 2, target, escort distance (PKE2).
#[allow(clippy::too_many_arguments)]
fn pack(id: u32, edid: &str, general: u32, ty: u8, specific: u16, loc: Option<Loc>, loc2: Option<Loc>, target: Option<Loc>, escort: Option<u32>) -> Vec<u8> {
    let three = |l: Loc| [l.0.to_le_bytes(), l.1.to_le_bytes(), l.2.to_le_bytes()].concat();
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"PKDT", [&general.to_le_bytes()[..], &[ty, 0, 0, 0], &specific.to_le_bytes(), &[0, 0]].concat())];
    subs.extend(loc.map(|l| sub(b"PLDT", three(l))));
    subs.push(sub(b"PSDT", [&[0xFF, 0xFF, 0, 0xFF][..], &0u32.to_le_bytes()].concat()));
    subs.extend(target.map(|t| sub(b"PTDT", three(t))));
    subs.extend(escort.map(|d| sub(b"PKE2", d.to_le_bytes().to_vec())));
    subs.extend(loc2.map(|l| sub(b"PLD2", three(l))));
    record(b"PACK", id, &subs)
}

fn npc(fid: u32, edid: &str, packages: &[u32]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    subs.extend(packages.iter().map(|p| sub(b"PKID", p.to_le_bytes().to_vec())));
    record(b"NPC_", fid, &subs)
}

fn crea(fid: u32, edid: &str, packages: &[u32]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    subs.extend(packages.iter().map(|p| sub(b"PKID", p.to_le_bytes().to_vec())));
    record(b"CREA", fid, &subs)
}

fn placed(sig: &[u8; 4], fid: u32, edid: &str, base: u32, pos: [f32; 3], owner: Option<u32>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec())];
    subs.extend(owner.map(|o| sub(b"XOWN", o.to_le_bytes().to_vec())));
    subs.push(sub(b"DATA", [pos[0].to_le_bytes(), pos[1].to_le_bytes(), pos[2].to_le_bytes(), [0; 4], [0; 4], 1.5f32.to_le_bytes()].concat()));
    record(sig, fid, &subs)
}

const STAT: u32 = 0x810;
const CHAIR: u32 = 0x811;
const BOTTLE: u32 = 0x812;
const MEAT: u32 = 0x813;
const TONIC: u32 = 0x814;
const STOCK: u32 = 0x815;
/// The caps item [documented: FormID 0000000F].
const CAPS: u32 = 0xF;
// Package types [documented: xEdit PACK type enum].
const FIND: u8 = 0;
const ESCORT: u8 = 2;
const USE_ITEM_AT: u8 = 8;
const AMBUSH: u8 = 9;
const FLEE: u8 = 10;
// Packages.
const P_ESCORT: u32 = 0x820;
const P_FIND: u32 = 0x821;
const P_FIND_OWNED: u32 = 0x822;
const P_STEAL: u32 = 0x823;
const P_USE: u32 = 0x824;
const P_AMBUSH: u32 = 0x825;
const P_FLEE: u32 = 0x826;
const P_FLEE_HOME: u32 = 0x827;
const P_FLEE_NOTHING: u32 = 0x828;
const P_FIND_NONE: u32 = 0x829;
const P_HUNT: u32 = 0x82A;
const P_BUY: u32 = 0x82B;
const P_USE_NAMED: u32 = 0x82C;
const P_FIND_CHAIR: u32 = 0x82D;
// Bases.
const N_LEADER: u32 = 0x840;
const N_GUEST: u32 = 0x841;
const N_FINDER: u32 = 0x842;
const N_HONEST: u32 = 0x843;
const N_THIEF: u32 = 0x844;
const N_SITTER: u32 = 0x845;
const N_AMBUSHER: u32 = 0x846;
const N_COWARD: u32 = 0x847;
const N_HOMEBODY: u32 = 0x848;
const N_CALM: u32 = 0x849;
const N_IDLE_FINDER: u32 = 0x84A;
const N_HUNTER: u32 = 0x84B;
const N_POACHER: u32 = 0x84C;
const N_BUYER: u32 = 0x84D;
const N_VENDOR: u32 = 0x84E;
const N_SITTER_NAMED: u32 = 0x84F;
// Cell and references.
const CELL: u32 = 0x860;
const MARK_FAR: u32 = 0x870;
const MARK_TRAP: u32 = 0x871;
const MARK_WAIT: u32 = 0x872;
const MARK_HOME: u32 = 0x873;
const R_CHAIR: u32 = 0x874;
const R_CHAIR_FAR: u32 = 0x8E0;
const R_SITTER_NAMED: u32 = 0x8E1;
const R_CHAIR_FINDER: u32 = 0x8E3;
const N_CHAIR_FINDER: u32 = 0x8E4;
const R_CHAIR_FINDER_2: u32 = 0x8E5;
const CHAIR_2: u32 = 0x8E2;
const R_BOTTLE: u32 = 0x875;
const R_OWNED: u32 = 0x876;
const R_LEADER: u32 = 0x880;
const R_GUEST: u32 = 0x881;
const R_FINDER: u32 = 0x882;
const R_HONEST: u32 = 0x883;
const R_THIEF: u32 = 0x884;
const R_SITTER: u32 = 0x885;
const R_AMBUSHER: u32 = 0x886;
const R_COWARD: u32 = 0x887;
const R_HOMEBODY: u32 = 0x888;
const R_CALM: u32 = 0x889;
const R_IDLE_FINDER: u32 = 0x88A;
const R_HUNTER: u32 = 0x88B;
const R_POACHER: u32 = 0x88C;
const R_BUYER: u32 = 0x88D;
const R_VENDOR: u32 = 0x88E;
const R_STOCK: u32 = 0x88F;

const C: Space = Space::Interior(CELL);

fn plugin_bytes() -> Vec<u8> {
    let globs = [
        glob(0x801, "GameYear", 2281.0),
        glob(0x802, "GameMonth", 9.0),
        glob(0x803, "GameDay", 19.0),
        glob(0x804, "GameHour", 12.0),
        glob(0x805, "GameDaysPassed", 5.0),
        glob(0x806, "TimeScale", 30.0),
    ]
    .concat();
    let packs = [
        // Lead the guest to the far marker, waiting when it is more than 200 behind.
        pack(P_ESCORT, "EscortGuest", 0, ESCORT, 0, Some((0, MARK_FAR, 0)), None, Some((0, R_GUEST, 0)), Some(200)),
        // Find one bottle within 1500 of where it stands.
        pack(P_FIND, "FindBottle", flags::MUST_COMPLETE, FIND, 0, Some((2, 0, 1500)), None, Some((1, BOTTLE, 1)), None),
        // Find meat, which only an owned stack is: left alone without Allow Stealing,
        // taken with it.
        pack(P_FIND_OWNED, "FindMeat", 0, FIND, 0, Some((2, 0, 1500)), None, Some((1, MEAT, 1)), None),
        pack(P_STEAL, "StealMeat", 0, FIND, type_flags::ALLOW_STEALING, Some((2, 0, 1500)), None, Some((1, MEAT, 1)), None),
        // Sit on furniture near the chair.
        pack(P_USE, "UseChair", 0, USE_ITEM_AT, 0, Some((0, R_CHAIR, 0)), None, Some((2, 11, 0)), None),
        pack(P_USE_NAMED, "UseThatChair", 0, USE_ITEM_AT, 0, Some((0, R_CHAIR, 0)), None, Some((0, R_CHAIR, 0)), None),
        // Find one piece of furniture within 1500 of where it stands.
        pack(P_FIND_CHAIR, "FindAChair", 0, FIND, 0, Some((2, 0, 1500)), None, Some((2, 11, 1)), None),
        // Wait at the wait marker, hidden, for the guest to come within 300 of the trap.
        pack(P_AMBUSH, "AmbushGuest", 0, AMBUSH, type_flags::HIDE_WHILE_AMBUSHING, Some((0, MARK_WAIT, 64)), Some((0, MARK_TRAP, 300)), Some((0, R_GUEST, 0)), None),
        // Keep 600 from the guest; run home from it; flee nothing.
        pack(P_FLEE, "FleeGuest", 0, FLEE, 0, None, None, Some((0, R_GUEST, 600)), None),
        pack(P_FLEE_HOME, "FleeHome", 0, FLEE, 0, Some((0, MARK_HOME, 64)), None, Some((0, R_GUEST, 600)), None),
        pack(P_FLEE_NOTHING, "FleeNothing", 0, FLEE, 0, None, None, None, None),
        // Object type "None": nothing to find; it goes to its location and stays.
        pack(P_FIND_NONE, "FindNothing", 0, FIND, 0, Some((0, MARK_HOME, 64)), None, Some((2, 0, 0)), None),
        // Find meat with Allow Killing: it attacks the actors of its area, whatever they
        // carry (the original's gecko on FindFood, packages.md section 3.1).
        pack(P_HUNT, "HuntMeat", 0, FIND, type_flags::ALLOW_KILLING, Some((2, 0, 1500)), None, Some((1, MEAT, 1)), None),
        // Find two tonics with Allow Buying: only the vendor's stock holds any.
        pack(P_BUY, "BuyTonics", 0, FIND, type_flags::ALLOW_BUYING, Some((2, 0, 1500)), None, Some((1, TONIC, 2)), None),
    ]
    .concat();
    let npcs = [
        npc(0x7, "Player", &[]),
        npc(N_LEADER, "Leader", &[P_ESCORT]),
        npc(N_GUEST, "Guest", &[]),
        npc(N_FINDER, "Finder", &[P_FIND]),
        npc(N_HONEST, "Honest", &[P_FIND_OWNED]),
        npc(N_THIEF, "Thief", &[P_STEAL]),
        npc(N_SITTER, "Sitter", &[P_USE]),
        npc(N_SITTER_NAMED, "NamedSitter", &[P_USE_NAMED]),
        npc(N_CHAIR_FINDER, "ChairFinder", &[P_FIND_CHAIR]),
        npc(N_AMBUSHER, "Ambusher", &[P_AMBUSH]),
        npc(N_COWARD, "Coward", &[P_FLEE]),
        npc(N_HOMEBODY, "Homebody", &[P_FLEE_HOME]),
        npc(N_CALM, "Calm", &[P_FLEE_NOTHING]),
        npc(N_IDLE_FINDER, "IdleFinder", &[P_FIND_NONE]),
        npc(N_POACHER, "Poacher", &[P_HUNT]),
        npc(N_BUYER, "Buyer", &[P_BUY]),
        // A vendor of Miscellaneous items (AIDT services, bit 10): the tonic is a MISC.
        record(b"NPC_", N_VENDOR, &[sub(b"EDID", z("Vendor")), sub(b"AIDT", [&[0u8; 8][..], &(1u32 << 10).to_le_bytes(), &[0u8; 8]].concat())]),
    ]
    .concat();
    let furn = [
        record(b"FURN", CHAIR, &[sub(b"EDID", z("Chair")), sub(b"MNAM", 0x4000_0001u32.to_le_bytes().to_vec())]),
        record(b"FURN", CHAIR_2, &[sub(b"EDID", z("SecondChair")), sub(b"MNAM", 0x4000_0001u32.to_le_bytes().to_vec())]),
    ]
    .concat();
    let misc = [
        record(b"MISC", BOTTLE, &[sub(b"EDID", z("Bottle")), sub(b"DATA", [1i32.to_le_bytes(), 0.5f32.to_le_bytes()].concat())]),
        record(b"MISC", MEAT, &[sub(b"EDID", z("Meat")), sub(b"DATA", [1i32.to_le_bytes(), 0.5f32.to_le_bytes()].concat())]),
        record(b"MISC", TONIC, &[sub(b"EDID", z("Tonic")), sub(b"DATA", [10i32.to_le_bytes(), 0.5f32.to_le_bytes()].concat())]),
        record(b"MISC", CAPS, &[sub(b"EDID", z("Caps001")), sub(b"DATA", [1i32.to_le_bytes(), 0.0f32.to_le_bytes()].concat())]),
    ]
    .concat();
    let stat = record(b"STAT", STAT, &[sub(b"EDID", z("Marker"))]);
    let cnto = |item: u32, n: i32| sub(b"CNTO", [item.to_le_bytes(), n.to_le_bytes()].concat());
    let cont = record(b"CONT", STOCK, &[sub(b"EDID", z("VendorStock")), cnto(TONIC, 3)]);
    // The vendor names its stock container in XMRC.
    let vendor = record(
        b"ACHR",
        R_VENDOR,
        &[
            sub(b"EDID", z("VendorRef")),
            sub(b"NAME", N_VENDOR.to_le_bytes().to_vec()),
            sub(b"XMRC", R_STOCK.to_le_bytes().to_vec()),
            sub(b"DATA", [1600.0f32.to_le_bytes(), 600.0f32.to_le_bytes(), [0; 4], [0; 4], [0; 4], 0.0f32.to_le_bytes()].concat()),
        ],
    );
    let refs = [
        placed(b"REFR", MARK_FAR, "MarkFar", STAT, [1850.0, 150.0, 0.0], None),
        placed(b"REFR", MARK_TRAP, "MarkTrap", STAT, [1000.0, 1700.0, 0.0], None),
        placed(b"REFR", MARK_WAIT, "MarkWait", STAT, [1500.0, 1700.0, 0.0], None),
        placed(b"REFR", MARK_HOME, "MarkHome", STAT, [150.0, 1850.0, 0.0], None),
        placed(b"REFR", R_CHAIR, "ChairRef", CHAIR, [1000.0, 1000.0, 0.0], None),
        placed(b"REFR", R_CHAIR_FAR, "FarChairRef", CHAIR_2, [1100.0, 1000.0, 0.0], None),
        placed(b"REFR", R_BOTTLE, "BottleRef", BOTTLE, [600.0, 600.0, 0.0], None),
        placed(b"REFR", R_OWNED, "OwnedMeatRef", MEAT, [700.0, 300.0, 0.0], Some(N_GUEST)),
        placed(b"ACHR", R_LEADER, "LeaderRef", N_LEADER, [250.0, 150.0, 0.0], None),
        placed(b"ACHR", R_GUEST, "GuestRef", N_GUEST, [150.0, 150.0, 0.0], None),
        placed(b"ACHR", R_FINDER, "FinderRef", N_FINDER, [150.0, 600.0, 0.0], None),
        placed(b"ACHR", R_HONEST, "HonestRef", N_HONEST, [300.0, 300.0, 0.0], None),
        placed(b"ACHR", R_THIEF, "ThiefRef", N_THIEF, [300.0, 300.0, 0.0], None),
        placed(b"ACHR", R_SITTER, "SitterRef", N_SITTER, [600.0, 1000.0, 0.0], None),
        placed(b"ACHR", R_SITTER_NAMED, "NamedSitterRef", N_SITTER_NAMED, [600.0, 1040.0, 0.0], None),
        placed(b"ACHR", R_CHAIR_FINDER, "ChairFinderRef", N_CHAIR_FINDER, [600.0, 960.0, 0.0], None),
        placed(b"ACHR", R_CHAIR_FINDER_2, "SecondChairFinderRef", N_CHAIR_FINDER, [600.0, 920.0, 0.0], None),
        placed(b"ACHR", R_AMBUSHER, "AmbusherRef", N_AMBUSHER, [1500.0, 1200.0, 0.0], None),
        placed(b"ACHR", R_COWARD, "CowardRef", N_COWARD, [450.0, 150.0, 0.0], None),
        placed(b"ACHR", R_HOMEBODY, "HomebodyRef", N_HOMEBODY, [450.0, 250.0, 0.0], None),
        placed(b"ACHR", R_CALM, "CalmRef", N_CALM, [1000.0, 500.0, 0.0], None),
        placed(b"ACHR", R_IDLE_FINDER, "IdleFinderRef", N_IDLE_FINDER, [1200.0, 1200.0, 0.0], None),
        placed(b"ACRE", R_HUNTER, "HunterRef", N_HUNTER, [1100.0, 600.0, 0.0], None),
        placed(b"ACHR", R_POACHER, "PoacherRef", N_POACHER, [1100.0, 700.0, 0.0], None),
        placed(b"ACHR", R_BUYER, "BuyerRef", N_BUYER, [1600.0, 200.0, 0.0], None),
        vendor,
        placed(b"REFR", R_STOCK, "VendorStockRef", STOCK, [1900.0, 1900.0, -2000.0], None),
    ]
    .concat();
    let cell = [record(b"CELL", CELL, &[sub(b"EDID", z("Hall"))]), group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs))].concat();
    let top = |sig: &[u8; 4], body: &[u8]| group(*sig, 0, body);
    [
        tes4(),
        top(b"GLOB", &globs),
        top(b"STAT", &stat),
        top(b"MISC", &misc),
        top(b"FURN", &furn),
        top(b"CONT", &cont),
        top(b"NPC_", &npcs),
        top(b"CREA", &crea(N_HUNTER, "Hunter", &[P_HUNT])),
        top(b"PACK", &packs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat()
}

/// A flat navmesh of 20 × 20 squares of side 100 at the origin.
fn nav() -> NavGraph {
    let n = 20usize;
    let vid = |i: usize, j: usize| (j * (n + 1) + i) as u16;
    let mut vertices = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            vertices.push([i as f32 * 100.0, j as f32 * 100.0, 0.0]);
        }
    }
    let tri = |i: isize, j: isize, h: usize| -> i16 {
        if i < 0 || j < 0 || i >= n as isize || j >= n as isize {
            -1
        } else {
            ((j as usize * n + i as usize) * 2 + h) as i16
        }
    };
    let mut triangles = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let (a, b, c, d) = (vid(i, j), vid(i + 1, j), vid(i + 1, j + 1), vid(i, j + 1));
            let (ii, jj) = (i as isize, j as isize);
            triangles.push(Triangle { v: [a, b, c], edges: [tri(ii, jj - 1, 1), tri(ii + 1, jj, 1), tri(ii, jj, 1)], flags: 0, cover: 0 });
            triangles.push(Triangle { v: [a, c, d], edges: [tri(ii, jj, 0), tri(ii, jj + 1, 0), tri(ii - 1, jj, 0)], flags: 0, cover: 0 });
        }
    }
    let m = NavMeshRecord { form_id: 1, vertices, triangles, ..Default::default() };
    NavGraph::assemble(&[(&m, Some(CELL), Some(C))])
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Errands.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    Fixture { data, forms, refs }
}

struct Sim<'a> {
    game: Game<'a>,
    ai: Ai,
    nav: NavGraph,
}

fn sim<'a>(f: &'a Fixture, actors: &[u32]) -> Sim<'a> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let game = Game::new(&f.data, world, GameConfig { strict: true, frame_seconds: 0.5, package_interval: Some(10.0), ..GameConfig::default() });
    let mut ai = Ai::new(AiConfig { walk_speed: 100.0, run_speed: 400.0, ..AiConfig::default() });
    for a in actors {
        ai.add(&game, FormId(*a)).unwrap();
    }
    Sim { game, ai, nav: nav() }
}

impl Sim<'_> {
    fn run(&mut self, seconds: f64) {
        for _ in 0..(seconds * 2.0) as usize {
            self.game.tick();
            self.ai.update(&mut self.game, &self.nav, 0.5);
        }
    }

    fn events(&self, actor: u32) -> Vec<&AiEvent> {
        self.ai.events.iter().filter(|e| e.actor == FormId(actor)).map(|e| &e.event).collect()
    }

    fn feet(&self, actor: u32) -> [f32; 3] {
        self.ai.agents[&FormId(actor)].walker.feet
    }

    /// Put an actor the AI does not move somewhere else in the cell.
    fn put(&mut self, actor: u32, pos: [f32; 3]) {
        self.game.set_actor_place(FormId(actor), Some(FormId(CELL)), pos, 0.0).unwrap();
    }

    fn goal_met(&self, actor: u32) -> bool {
        self.game.extra.get(&FormId(actor)).is_some_and(|e| e.package_goal_met)
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

// ---- tests -------------------------------------------------------------------------

#[test]
fn every_package_type_of_the_fixture_is_carried_out() {
    let f = fixture();
    assert!(f.data.issues.is_empty(), "{:?}", f.data.issues.to_json());
    let actors = [R_LEADER, R_FINDER, R_HONEST, R_SITTER, R_AMBUSHER, R_COWARD, R_HOMEBODY, R_CALM, R_IDLE_FINDER];
    let mut s = sim(&f, &actors);
    s.run(1.0);
    for a in actors {
        assert!(!s.events(a).iter().any(|e| matches!(e, AiEvent::NotCarriedOut { .. } | AiEvent::Unreachable { .. })), "{a:#x}: {:?}", s.events(a));
    }
}

#[test]
fn an_escort_waits_for_the_one_it_leads_and_goes_on_when_it_comes_up() {
    let f = fixture();
    let mut s = sim(&f, &[R_LEADER]);
    s.run(10.0);
    // It walked off and stopped about 200 units from the guest, who stands still.
    let ev = s.events(R_LEADER);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Trip { .. })), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, AiEvent::EscortWaits { target, away: Some(a), .. } if target.0 == R_GUEST && *a > 199.0 && *a < 260.0)), "{ev:?}");
    let waited_at = s.feet(R_LEADER);
    assert!(dist(waited_at, [150.0, 150.0, 0.0]) < 260.0);
    s.run(10.0);
    assert!(dist(s.feet(R_LEADER), waited_at) < 1.0, "it waits");
    assert_eq!(s.ai.agents[&FormId(R_LEADER)].errand, Some(Errand::Escort { waiting: true }));
    assert!(!s.goal_met(R_LEADER));
    // The guest comes up: it goes on; the guest keeps up; it arrives and its goal is met.
    for _ in 0..40 {
        let at = s.feet(R_LEADER);
        s.put(R_GUEST, [at[0] - 60.0, at[1], 0.0]);
        s.run(1.0);
    }
    let ev = s.events(R_LEADER);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::EscortOn { .. })), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Arrived { .. })), "{ev:?}");
    assert!(dist(s.feet(R_LEADER), [1850.0, 150.0, 0.0]) < 40.0);
    assert!(s.goal_met(R_LEADER));
}

#[test]
fn a_find_package_goes_to_the_item_and_takes_it() {
    let f = fixture();
    let mut s = sim(&f, &[R_FINDER]);
    s.run(15.0);
    let ev = s.events(R_FINDER);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::FindGoes { object, .. } if object.0 == R_BOTTLE)), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Found { object, act: Act::Takes, .. } if object.0 == R_BOTTLE)), "{ev:?}");
    assert_eq!(s.game.world.item_count(FormId(R_FINDER), FormId(BOTTLE)).unwrap(), 1);
    assert!(!s.game.present(FormId(R_BOTTLE)), "the bottle left the world");
    assert!(dist(s.feet(R_FINDER), [600.0, 600.0, 0.0]) < 40.0);
    assert!(s.goal_met(R_FINDER), "the count is found");
    // GetIsUsedItem sees what it took.
    assert_eq!(s.game.extra[&FormId(R_FINDER)].used_item, Some(FormId(BOTTLE)));
}

#[test]
fn an_owned_item_is_taken_only_with_allow_stealing() {
    let f = fixture();
    let mut s = sim(&f, &[R_HONEST]);
    s.run(10.0);
    let ev = s.events(R_HONEST);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::NothingFound { found: 0, .. })), "{ev:?}");
    assert!(s.game.present(FormId(R_OWNED)));
    assert!(s.goal_met(R_HONEST), "nothing to find: the package has done what it can");
    let mut s = sim(&f, &[R_THIEF]);
    s.run(15.0);
    assert!(s.events(R_THIEF).iter().any(|e| matches!(e, AiEvent::Found { object, act: Act::Takes, .. } if object.0 == R_OWNED)), "{:?}", s.events(R_THIEF));
    assert_eq!(s.game.world.item_count(FormId(R_THIEF), FormId(MEAT)).unwrap(), 1);
}

/// A creature on Find with Allow Killing attacks a living actor of its area that carries
/// none of what it looks for, not its kin [observed: the original's gecko on
/// VCrGeckoFindFood attacked bloatflies, a radroach and a protectron, AI observer
/// campaign `gecko_find` 2026-10-19]; a person on the same package does not.
#[test]
fn a_find_package_with_allow_killing_attacks_an_actor_carrying_nothing_it_wants() {
    let f = fixture();
    let mut s = sim(&f, &[R_HUNTER]);
    s.run(10.0);
    let ev = s.events(R_HUNTER);
    let attacked: Vec<u32> = ev.iter().filter_map(|e| if let AiEvent::Found { object, act: Act::Attacks, .. } = e { Some(object.0) } else { None }).collect();
    assert!(!attacked.is_empty(), "{ev:?}");
    for a in &attacked {
        assert_eq!(s.game.world.item_count(FormId(*a), FormId(MEAT)).unwrap_or(0), 0, "it carries no meat");
    }
    // A person with the same package only attacks one carrying meat (nobody here).
    let mut s = sim(&f, &[R_POACHER]);
    s.run(10.0);
    assert!(!s.events(R_POACHER).iter().any(|e| matches!(e, AiEvent::Found { act: Act::Attacks, .. })), "{:?}", s.events(R_POACHER));
}

/// Allow Buying: the vendor's merchant container holds the tonics; the buyer goes up to
/// the vendor and buys them one at a time at the player's buying price at its Barter
/// skill (value 10, at least 10 caps), the caps going into the container (packages.md
/// section 3.1, hypotheses). Without enough caps it finds nothing.
#[test]
fn a_find_package_with_allow_buying_buys_from_a_vendor() {
    let f = fixture();
    let mut s = sim(&f, &[R_BUYER]);
    s.game.world.add_item(FormId(R_BUYER), FormId(CAPS), 40).unwrap();
    let price = fnv_ai::errand::buying_price(&mut s.game, FormId(R_BUYER), FormId(TONIC));
    assert!((10..=16).contains(&price), "{price}");
    s.run(20.0);
    let ev = s.events(R_BUYER);
    let bought: Vec<u32> = ev.iter().filter_map(|e| if let AiEvent::Bought { vendor, item, price, .. } = e { (vendor.0 == R_VENDOR && item.0 == TONIC).then_some(*price) } else { None }).collect();
    assert_eq!(bought, [price, price], "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Found { object, act: Act::Buys, .. } if object.0 == R_VENDOR)), "{ev:?}");
    let count = |s: &mut Sim, r: u32, i: u32| s.game.world.item_count(FormId(r), FormId(i)).unwrap();
    assert_eq!((count(&mut s, R_BUYER, TONIC), count(&mut s, R_BUYER, CAPS)), (2, 40 - 2 * price as i32));
    assert_eq!((count(&mut s, R_STOCK, TONIC), count(&mut s, R_STOCK, CAPS)), (1, 2 * price as i32));
    assert!(dist(s.feet(R_BUYER), [1600.0, 600.0, 0.0]) < 120.0, "it went up to the vendor");
    assert!(s.goal_met(R_BUYER), "the count is found");
    assert_eq!(s.game.extra[&FormId(R_BUYER)].used_item, Some(FormId(TONIC)));

    // Nine caps buy nothing (a tonic costs at least its value): the vendor is not gone to.
    let mut s = sim(&f, &[R_BUYER]);
    s.game.world.add_item(FormId(R_BUYER), FormId(CAPS), 9).unwrap();
    s.run(10.0);
    let ev = s.events(R_BUYER);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::NothingFound { found: 0, .. })), "{ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, AiEvent::Bought { .. })), "{ev:?}");
    assert_eq!(count(&mut s, R_STOCK, TONIC), 3);
}

#[test]
fn a_find_package_with_nothing_to_find_goes_to_its_location_and_stays() {
    let f = fixture();
    let mut s = sim(&f, &[R_IDLE_FINDER]);
    s.run(30.0);
    let ev = s.events(R_IDLE_FINDER);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Arrived { .. })), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, AiEvent::NothingFound { .. })), "{ev:?}");
    assert!(dist(s.feet(R_IDLE_FINDER), [150.0, 1850.0, 0.0]) < 70.0);
    assert!(s.goal_met(R_IDLE_FINDER));
}

#[test]
fn a_use_item_at_package_sits_on_the_furniture() {
    let f = fixture();
    let mut s = sim(&f, &[R_SITTER]);
    s.run(10.0);
    let actor = FormId(R_SITTER);
    assert_eq!(s.ai.agents[&actor].using, Some(FormId(R_CHAIR)));
    assert!(s.events(R_SITTER).iter().any(|e| matches!(e, AiEvent::UsesAt { thing: Some(t), .. } if t.0 == R_CHAIR)));
    assert!(dist(s.feet(R_SITTER), [1000.0, 1000.0, 0.0]) < 40.0);
    assert!(s.goal_met(R_SITTER));
    assert_eq!(s.game.extra[&actor].used_item, Some(FormId(CHAIR)));
}

#[test]
fn an_ambush_waits_hidden_until_its_target_comes_into_the_trigger() {
    let f = fixture();
    let mut s = sim(&f, &[R_AMBUSHER]);
    s.run(10.0);
    let actor = FormId(R_AMBUSHER);
    assert!(s.events(R_AMBUSHER).iter().any(|e| matches!(e, AiEvent::AmbushWaits { hidden: true, .. })), "{:?}", s.events(R_AMBUSHER));
    assert!(dist(s.feet(R_AMBUSHER), [1500.0, 1700.0, 0.0]) < 70.0);
    assert!(!s.game.sneaks(actor), "Hide While Ambushing does not make it sneak");
    assert!(!s.game.awareness.state(actor).in_combat());
    // The guest walks by outside the trigger: nothing; then into it.
    s.put(R_GUEST, [1000.0, 1300.0, 0.0]);
    s.run(2.0);
    assert!(!s.events(R_AMBUSHER).iter().any(|e| matches!(e, AiEvent::AmbushSprung { .. })));
    s.put(R_GUEST, [1000.0, 1550.0, 0.0]);
    s.run(1.0);
    assert!(s.events(R_AMBUSHER).iter().any(|e| matches!(e, AiEvent::AmbushSprung { enemy, .. } if enemy.0 == R_GUEST)), "{:?}", s.events(R_AMBUSHER));
    assert!(s.game.awareness.state(actor).in_combat(), "it fights");
    assert!(s.goal_met(R_AMBUSHER));
}

#[test]
fn a_flee_package_runs_from_its_target_until_it_is_far_enough() {
    let f = fixture();
    let mut s = sim(&f, &[R_COWARD]);
    // The guest is no enemy: it stands, and nothing threatens it.
    s.run(2.0);
    assert!(dist(s.feet(R_COWARD), [450.0, 150.0, 0.0]) < 1.0);
    assert!(s.events(R_COWARD).iter().any(|e| matches!(e, AiEvent::FleeSafe { from: None, .. })), "{:?}", s.events(R_COWARD));
    // The guest turns on it.
    s.game.start_combat(FormId(R_GUEST), FormId(R_COWARD));
    s.run(1.0);
    assert!(s.events(R_COWARD).iter().any(|e| matches!(e, AiEvent::FleeRuns { from, .. } if from.0 == R_GUEST)), "{:?}", s.events(R_COWARD));
    s.run(10.0);
    assert!(dist(s.feet(R_COWARD), [150.0, 150.0, 0.0]) > 600.0, "{:?}", s.feet(R_COWARD));
    assert!(s.events(R_COWARD).iter().any(|e| matches!(e, AiEvent::FleeSafe { .. })));
    assert!(s.goal_met(R_COWARD));
    assert!(!s.game.awareness.state(FormId(R_COWARD)).in_combat(), "it does not fight");
    // The guest comes near again: it runs again.
    let at = s.feet(R_COWARD);
    s.put(R_GUEST, [at[0] - 100.0, at[1], 0.0]);
    s.run(2.0);
    let runs = s.events(R_COWARD).iter().filter(|e| matches!(e, AiEvent::FleeRuns { .. })).count();
    assert_eq!(runs, 2, "{:?}", s.events(R_COWARD));
}

#[test]
fn a_flee_package_with_a_location_runs_there_and_one_with_no_threat_stands() {
    let f = fixture();
    let mut s = sim(&f, &[R_HOMEBODY, R_CALM]);
    s.game.start_combat(FormId(R_GUEST), FormId(R_HOMEBODY));
    s.run(15.0);
    assert!(dist(s.feet(R_HOMEBODY), [150.0, 1850.0, 0.0]) < 70.0, "{:?}", s.feet(R_HOMEBODY));
    assert!(s.goal_met(R_HOMEBODY));
    // Nothing threatens the calm one: it stays and its package is done.
    assert!(dist(s.feet(R_CALM), [1000.0, 500.0, 0.0]) < 1.0);
    assert!(s.events(R_CALM).iter().any(|e| matches!(e, AiEvent::FleeSafe { from: None, .. })), "{:?}", s.events(R_CALM));
    assert!(s.goal_met(R_CALM));
    assert_eq!(s.ai.agents[&FormId(R_CALM)].activity, Activity::There);
}

// ---- furniture eligibility (issue #1522) --------------------------------------------

/// The actors of these tests fit the second chair's base only.
struct OnlyTheFarChair;

impl FurnitureFacts for OnlyTheFarChair {
    fn marker_ids(&self, _: FormId) -> Option<&[u8]> {
        None
    }
    fn fit(&self, _: ActorKind, _: &str, base: FormId) -> Fit {
        if base.0 == CHAIR_2 { Fit::Supported } else { Fit::Unsupported }
    }
}

#[test]
fn use_item_at_looks_past_the_furniture_it_has_no_clips_for() {
    let f = fixture();
    let mut s = sim(&f, &[R_SITTER]);
    s.ai.set_furniture_facts(Arc::new(OnlyTheFarChair));
    s.run(15.0);
    let actor = FormId(R_SITTER);
    assert_eq!(s.ai.agents[&actor].using, Some(FormId(R_CHAIR_FAR)));
    assert!(s.events(R_SITTER).iter().any(|e| matches!(e, AiEvent::UsesAt { thing: Some(t), .. } if t.0 == R_CHAIR_FAR)));
    assert!(s.goal_met(R_SITTER));
}

/// Named furniture it has no clips for: nothing kept, nothing faced, nothing used, and its
/// goal met as when there is nothing to use. The refusal is on record.
#[test]
fn use_item_at_a_named_chair_it_has_no_clips_for_uses_nothing() {
    let f = fixture();
    let mut s = sim(&f, &[R_SITTER_NAMED]);
    s.ai.set_furniture_facts(Arc::new(OnlyTheFarChair));
    s.run(15.0);
    let actor = FormId(R_SITTER_NAMED);
    assert_eq!(s.ai.agents[&actor].using, None);
    assert_eq!(s.ai.furniture_of(actor), None);
    assert!(s.events(R_SITTER_NAMED).iter().any(|e| matches!(e, AiEvent::UsesAt { thing: None, .. })), "{:?}", s.events(R_SITTER_NAMED));
    assert!(s.events(R_SITTER_NAMED).iter().any(|e| matches!(e, AiEvent::FurnitureEligibility { furniture, decision: "excluded", .. } if furniture.0 == R_CHAIR)));
    assert!(s.goal_met(R_SITTER_NAMED));
    // Without facts it sits, as before.
    let mut s = sim(&f, &[R_SITTER_NAMED]);
    s.run(15.0);
    assert_eq!(s.ai.agents[&actor].using, Some(FormId(R_CHAIR)));
}

/// Find: a chair it has no clips for is passed over before a way is planned, and does not
/// use up the count: the one it is asked for is the farther chair.
#[test]
fn find_looks_past_the_furniture_it_has_no_clips_for_and_does_not_count_it() {
    let f = fixture();
    let actor = FormId(R_CHAIR_FINDER);
    let found_things = |s: &Sim| -> Vec<(u32, Act)> {
        s.events(R_CHAIR_FINDER).iter().filter_map(|e| match e {
            AiEvent::Found { object, act, .. } => Some((object.0, *act)),
            _ => None,
        }).collect()
    };
    let mut s = sim(&f, &[R_CHAIR_FINDER]);
    s.ai.set_furniture_facts(Arc::new(OnlyTheFarChair));
    s.run(15.0);
    assert!(!s.events(R_CHAIR_FINDER).iter().any(|e| matches!(e, AiEvent::FindGoes { object, .. } if object.0 == R_CHAIR)));
    assert_eq!(found_things(&s), [(R_CHAIR_FAR, Act::Uses)]);
    assert_eq!(s.ai.agents[&actor].using, Some(FormId(R_CHAIR_FAR)));
    assert!(matches!(s.ai.agents[&actor].errand, Some(Errand::Find { found: 1, .. })));
    assert!(s.goal_met(R_CHAIR_FINDER));
    // Nothing fits: nothing found, nothing kept, the count untouched.
    let mut s = sim(&f, &[R_CHAIR_FINDER]);
    s.ai.set_furniture_facts(Arc::new(NoChair));
    s.run(15.0);
    assert!(found_things(&s).is_empty());
    assert_eq!(s.ai.agents[&actor].using, None);
    assert_eq!(s.ai.furniture_of(actor), None);
    assert!(s.events(R_CHAIR_FINDER).iter().any(|e| matches!(e, AiEvent::NothingFound { found: 0, .. })));
    // Without facts, the nearest, as before.
    let mut s = sim(&f, &[R_CHAIR_FINDER]);
    s.run(15.0);
    assert_eq!(found_things(&s), [(R_CHAIR, Act::Uses)]);
}

/// Two actors whose Find packages look for any furniture take one chair each: a chair
/// the other sits on or is on its way to is not free (as for Use Item At). In Fallout 3's
/// G.O.A.T. (CG03, Tale of Two Wastelands) two students went to each desk and one stood.
#[test]
fn two_finders_of_furniture_take_a_chair_each() {
    let f = fixture();
    let mut s = sim(&f, &[R_CHAIR_FINDER, R_CHAIR_FINDER_2]);
    s.run(20.0);
    let first = s.ai.agents[&FormId(R_CHAIR_FINDER)].using;
    let second = s.ai.agents[&FormId(R_CHAIR_FINDER_2)].using;
    assert!(first.is_some() && second.is_some(), "{first:?} {second:?}: {:?} {:?}", s.events(R_CHAIR_FINDER), s.events(R_CHAIR_FINDER_2));
    assert_ne!(first, second);
    let goes = |r: u32| -> Vec<u32> { s.events(r).iter().filter_map(|e| match e { AiEvent::FindGoes { object, .. } => Some(object.0), _ => None }).collect() };
    let (a, b) = (goes(R_CHAIR_FINDER), goes(R_CHAIR_FINDER_2));
    assert!(a.iter().all(|x| !b.contains(x)), "both went for one chair: {a:?} {b:?}");
}

/// A chair the player sits on is not free for a Find package: the actor takes the other
/// one (the G.O.A.T.: Paul went to the desk the player sat at, and the test never began).
#[test]
fn a_finder_of_furniture_passes_over_the_chair_the_player_sits_on() {
    use fnv_game::game::FurnitureUse;
    let f = fixture();
    let mut s = sim(&f, &[R_CHAIR_FINDER]);
    s.game.set_furniture_use(FormId::PLAYER_REF, Some(FurnitureUse { reference: FormId(R_CHAIR), marker_id: 0, state: 3 }));
    s.run(15.0);
    assert!(!s.events(R_CHAIR_FINDER).iter().any(|e| matches!(e, AiEvent::FindGoes { object, .. } if object.0 == R_CHAIR)), "{:?}", s.events(R_CHAIR_FINDER));
    assert_eq!(s.ai.agents[&FormId(R_CHAIR_FINDER)].using, Some(FormId(R_CHAIR_FAR)));
}

struct NoChair;

impl FurnitureFacts for NoChair {
    fn marker_ids(&self, _: FormId) -> Option<&[u8]> {
        None
    }
    fn fit(&self, _: ActorKind, _: &str, _: FormId) -> Fit {
        Fit::Unsupported
    }
}
