//! JohnnyGuitar NVSE batch 4 (#1007; docs/spec-notes/ext-jg4.md) on a synthetic plugin:
//! idle markers' animations and traits, regions' weather lists, race flags and faction
//! members.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const IDLM: u32 = 0x900;
const IDLE1: u32 = 0x901;
const IDLE2: u32 = 0x902;
const IDLE3: u32 = 0x903;
const REGN: u32 = 0x910;
const REGN_BARE: u32 = 0x911;
const WTHR1: u32 = 0x912;
const WTHR2: u32 = 0x913;
const WTHR3: u32 = 0x914;
const GLOB: u32 = 0x915;
const RACE: u32 = 0x920;
const FACT: u32 = 0x930;
const NPC_A: u32 = 0x931;
const NPC_B: u32 = 0x932;
const NPC_C: u32 = 0x933;

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

fn le(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// RDAT [documented: schemas/fnv/sky.json RegionDataHeader]: type, flags, priority.
fn rdat(kind: u32, flags: u8, priority: u8) -> fnv_esm::raw::Subrecord {
    sub(b"RDAT", [le(kind), vec![flags, priority, 0, 0]].concat())
}

fn plugin_bytes() -> Vec<u8> {
    let npc_player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let snam = |rank: u8| sub(b"SNAM", [le(FACT), vec![rank, 0, 0, 0]].concat());
    let npcs = [npc_player, edid(b"NPC_", NPC_A, "MemberA", &[snam(0)]), edid(b"NPC_", NPC_B, "MemberB", &[snam(1)]), edid(b"NPC_", NPC_C, "NotMember", &[])].concat();
    let fact = edid(b"FACT", FACT, "TestFaction", &[]);
    let idle = [edid(b"IDLE", IDLE1, "TestIdle1", &[]), edid(b"IDLE", IDLE2, "TestIdle2", &[]), edid(b"IDLE", IDLE3, "TestIdle3", &[])].concat();
    // Run in sequence (1), 2 animations, a 5 s timer.
    let idlm = edid(b"IDLM", IDLM, "TestMarker", &[sub(b"IDLF", vec![1]), sub(b"IDLC", vec![2, 0, 0, 0]), sub(b"IDLT", 5.0f32.to_le_bytes().to_vec()), sub(b"IDLA", u32s(&[IDLE1, IDLE2]))]);
    let wthr = [edid(b"WTHR", WTHR1, "TestWeather1", &[]), edid(b"WTHR", WTHR2, "TestWeather2", &[]), edid(b"WTHR", WTHR3, "TestWeather3", &[])].concat();
    let glob = record(b"GLOB", 0, GLOB, &[sub(b"EDID", z("TestGlobal")), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0.0f32.to_le_bytes().to_vec())]);
    // A map block first (type 4), then the weather block: override, priority 40, two
    // weathers, the second gated by the global.
    let regn = [
        edid(b"REGN", REGN, "TestRegion", &[rdat(4, 0, 60), sub(b"RDMP", z("Somewhere")), rdat(3, 1, 40), sub(b"RDWT", [u32s(&[WTHR1, 70, 0]), u32s(&[WTHR2, 30, GLOB])].concat())]),
        edid(b"REGN", REGN_BARE, "TestRegionBare", &[]),
    ]
    .concat();
    // DATA: seven skill boosts, two unused bytes, four heights and weights, the flags
    // (Playable) [documented: schemas/fnv/gameplay.json RACE DATA].
    let race = edid(b"RACE", RACE, "TestRace", &[sub(b"DATA", [vec![0; 16], vec![0; 16], le(1)].concat())]);
    [
        tes4(),
        top(b"GLOB", &glob),
        top(b"FACT", &fact),
        top(b"RACE", &race),
        top(b"NPC_", &npcs),
        top(b"IDLE", &idle),
        top(b"IDLM", &idlm),
        top(b"WTHR", &wthr),
        top(b"REGN", &regn),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Jg4.esm".into()).unwrap()];
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

/// An array's entries as FormIDs.
fn forms(g: &mut Game, expr: &str) -> Vec<u32> {
    let at = |i: &str| format!("array_var a
let a := {expr}
SetFunctionValue {i}");
    let len = match q(g, &at("(ar_Size a)")) {
        XVal::Num(x) => x as usize,
        v => panic!("{expr}: {v:?}"),
    };
    (0..len)
        .map(|i| match q(g, &at(&format!("(a[{i}])"))) {
            XVal::Ref(r) => r,
            XVal::Num(x) => x as u32,
            v => panic!("{expr}[{i}]: {v:?}"),
        })
        .collect()
}

#[test]
fn idle_marker_animations_and_traits() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: GetIdleMarkerTraitNumeric: 1 flags, 2 timer, 3 count].
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 1"), 1.0);
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 2"), 5.0);
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 3"), 2.0);
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 4"), 0.0);
    assert_eq!(forms(&mut g, "GetIdleMarkerAnimations TestMarker"), vec![IDLE1, IDLE2]);
    q(&mut g, "SetIdleMarkerTraitNumeric TestMarker 1 4");
    q(&mut g, "SetIdleMarkerTraitNumeric TestMarker 2 12.5");
    q(&mut g, "SetIdleMarkerTraitNumeric TestMarker 3 9");
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 1"), 4.0, "do once only");
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 2"), 12.5);
    // A timer past a float's range is refused (#1333 review); a negative one is kept
    // [observed: jg4].
    q(&mut g, "float f
let f := 10000000000.0 * 10000000000.0 * 10000000000.0 * 10000000000.0
SetIdleMarkerTraitNumeric TestMarker 2 f");
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 2"), 12.5);
    q(&mut g, "SetIdleMarkerTraitNumeric TestMarker 2 -3");
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 2"), -3.0);
    q(&mut g, "SetIdleMarkerTraitNumeric TestMarker 2 12.5");
    // The flags read back whole; the marker plays by bits 0 and 2 [observed: jg4].
    q(&mut g, "SetIdleMarkerTraitNumeric TestMarker 1 6");
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 1"), 6.0);
    q(&mut g, "SetIdleMarkerTraitNumeric TestMarker 1 4");
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 3"), 2.0, "the count is not set");
    q(&mut g, "SetIdleMarkerAnimation TestMarker 1 TestIdle3");
    q(&mut g, "SetIdleMarkerAnimation TestMarker 5 TestIdle3");
    assert_eq!(forms(&mut g, "GetIdleMarkerAnimations TestMarker"), vec![IDLE1, IDLE3]);
    // Any form is listed, an idle or not [observed: jg4].
    q(&mut g, "SetIdleMarkerAnimation TestMarker 0 TestRace");
    assert_eq!(forms(&mut g, "GetIdleMarkerAnimations TestMarker"), vec![RACE, IDLE3]);
    q(&mut g, "array_var a\nlet a := ar_List TestIdle2 TestIdle3 TestIdle1\nSetIdleMarkerAnimations TestMarker a");
    assert_eq!(forms(&mut g, "GetIdleMarkerAnimations TestMarker"), vec![IDLE2, IDLE3, IDLE1]);
    assert_eq!(n(&mut g, "GetIdleMarkerTraitNumeric TestMarker 3"), 3.0, "the count follows");
    // The engine's idles read the changed collection.
    let c = fnv_game::nvse_jg4::idle_marker(&g, FormId(IDLM)).unwrap();
    assert_eq!((c.idles.len(), c.timer, c.do_once, c.run_in_sequence), (3, 12.5, true, false));
}

#[test]
fn region_weather_lists() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(forms(&mut g, "GetRegionWeathers TestRegion"), vec![WTHR1, WTHR2]);
    assert_eq!(n(&mut g, "GetNthRegionWeatherType TestRegion 1"), f64::from(WTHR2));
    // Index 0 reads none and 0 [observed: jg4].
    assert_eq!(n(&mut g, "GetNthRegionWeatherChance TestRegion 0"), 0.0);
    assert_eq!(n(&mut g, "GetNthRegionWeatherType TestRegion 0"), 0.0);
    assert_eq!(n(&mut g, "GetNthRegionWeatherGlobal TestRegion 1"), f64::from(GLOB));
    assert_eq!(n(&mut g, "GetNthRegionWeatherGlobal TestRegion 0"), 0.0);
    assert_eq!(n(&mut g, "GetNthRegionWeatherType TestRegion 2"), 0.0);
    assert_eq!(n(&mut g, "IsWeatherInRegion TestRegion TestWeather2"), 1.0);
    assert_eq!(n(&mut g, "IsWeatherInRegion TestRegion TestWeather3"), 0.0);
    // The weather block's header, not the map block's before it.
    assert_eq!(n(&mut g, "GetRegionWeatherOverride TestRegion"), 1.0);
    assert_eq!(n(&mut g, "GetRegionWeatherPriority TestRegion"), 40.0);
    q(&mut g, "AddRegionWeather TestRegion TestWeather3 25 TestGlobal");
    q(&mut g, "AddRegionWeather TestRegion TestWeather3 99");
    // Listed first, once [observed: jg4].
    assert_eq!(forms(&mut g, "GetRegionWeathers TestRegion"), vec![WTHR3, WTHR1, WTHR2]);
    assert_eq!(n(&mut g, "GetNthRegionWeatherChance TestRegion 1"), 70.0);
    q(&mut g, "RemoveRegionWeather TestRegion TestWeather1");
    assert_eq!(forms(&mut g, "GetRegionWeathers TestRegion"), vec![WTHR3, WTHR2]);
    q(&mut g, "SetRegionWeatherOverride TestRegion 0");
    q(&mut g, "SetRegionWeatherPriority TestRegion 7");
    // Outside 0-255: nothing changes [observed: jg4].
    q(&mut g, "SetRegionWeatherPriority TestRegion 300");
    assert_eq!((n(&mut g, "GetRegionWeatherOverride TestRegion"), n(&mut g, "GetRegionWeatherPriority TestRegion")), (0.0, 7.0));
    // Every entry but the first goes [observed: jg4].
    q(&mut g, "ClearRegionWeathers TestRegion");
    assert_eq!(forms(&mut g, "GetRegionWeathers TestRegion"), vec![WTHR3]);
    // A region without a weather block gains none [observed: jg4].
    assert!(forms(&mut g, "GetRegionWeathers TestRegionBare").is_empty());
    q(&mut g, "AddRegionWeather TestRegionBare TestWeather1 100");
    assert!(forms(&mut g, "GetRegionWeathers TestRegionBare").is_empty());
}

#[test]
fn race_flags_and_faction_members() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!((n(&mut g, "GetRaceFlag TestRace 0"), n(&mut g, "GetRaceFlag TestRace 2")), (1.0, 0.0));
    q(&mut g, "SetRaceFlag TestRace 2 1");
    q(&mut g, "SetRaceFlag TestRace 0 0");
    q(&mut g, "SetRaceFlag TestRace 1 1");
    // Each bit as its value; the other bits are set too [observed: jg4].
    assert_eq!((n(&mut g, "GetRaceFlag TestRace 0"), n(&mut g, "GetRaceFlag TestRace 1"), n(&mut g, "GetRaceFlag TestRace 2")), (0.0, 2.0, 4.0));
    assert_eq!(forms(&mut g, "GetFactionMembers TestFaction"), vec![NPC_A, NPC_B]);
    assert_eq!(forms(&mut g, "GetFactionMembers TestFaction 1"), vec![NPC_B]);
}
