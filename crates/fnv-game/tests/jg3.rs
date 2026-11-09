//! JohnnyGuitar NVSE batch 3 (#1007; docs/spec-notes/ext-jg3.md) on a synthetic plugin:
//! effect shaders' traits and textures, media sets' traits, casinos' chips and winnings,
//! terminals' menu items, and the quest script delay.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const EFSH: u32 = 0x900;
const MSET: u32 = 0x901;
const SOUN: u32 = 0x902;
const SOUN2: u32 = 0x903;
const CSNO: u32 = 0x904;
const CHIP: u32 = 0x905;
const CHIP2: u32 = 0x906;
const TERM: u32 = 0x907;
const TERM2: u32 = 0x908;
const NOTE: u32 = 0x909;
const NOTE2: u32 = 0x90A;
const QUST: u32 = 0x90B;
const QUST_NO_DELAY: u32 = 0x90C;
const CELL: u32 = 0x200;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

/// EFSH DATA: 308 bytes [documented: schemas/fnv/effects.json EFSH DATA, from xEdit]:
/// flags 9 (No Membrane Shader, No Particle Shader), membrane source blend 5, fill alpha
/// fade in 0.25 s, edge fall off 1.5, particle lifetime 3, scale key 2 time 0.75, fill
/// colour (1, 2, 3), colour key 1 (4, 5, 6).
fn efsh_data() -> Vec<u8> {
    let mut d = vec![0u8; 308];
    d[0] = 9;
    d[4..8].copy_from_slice(&5u32.to_le_bytes());
    d[16..19].copy_from_slice(&[1, 2, 3]);
    d[188..191].copy_from_slice(&[4, 5, 6]);
    for (at, v) in [(20, 0.25f32), (52, 1.5), (132, 3.0), (184, 0.75)] {
        d[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    d
}

/// CSNO DATA [documented: schemas/fnv/objects.json CSNO]: shuffle share, payout, seven
/// reel stops, decks, max winnings 1000, the chip, the quest, the flags.
fn csno_data() -> Vec<u8> {
    [f32b(0.5), f32b(1.5), u32s(&[1; 7]), u32s(&[4, 1000, CHIP, 0, 0])].concat()
}

fn plugin_bytes() -> Vec<u8> {
    let npc = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let efsh = edid(b"EFSH", EFSH, "TestShader", &[sub(b"ICON", z("effects\\fill.dds")), sub(b"NAM7", z("effects\\holes.dds")), sub(b"DATA", efsh_data())]);
    let soun = [edid(b"SOUN", SOUN, "TestSound", &[]), edid(b"SOUN", SOUN2, "TestSound2", &[])].concat();
    // An incidental set: NAM1 3, a day path, a day dB, flags 5, daytime min 20, a daytime sound.
    let mset = edid(
        b"MSET",
        MSET,
        "TestMediaSet",
        &[
            sub(b"NAM1", 3i32.to_le_bytes().to_vec()),
            sub(b"NAM2", z("day.wav")),
            sub(b"NAM8", f32b(-6.0)),
            sub(b"PNAM", vec![5]),
            sub(b"DNAM", f32b(20.0)),
            sub(b"HNAM", SOUN.to_le_bytes().to_vec()),
        ],
    );
    let chip = [edid(b"CHIP", CHIP, "TestChip", &[]), edid(b"CHIP", CHIP2, "TestChip2", &[])].concat();
    let csno = edid(b"CSNO", CSNO, "TestCasino", &[sub(b"FULL", z("Test Casino")), sub(b"DATA", csno_data())]);
    let note = [edid(b"NOTE", NOTE, "TestNote", &[]), edid(b"NOTE", NOTE2, "TestNote2", &[])].concat();
    let term = [
        edid(
            b"TERM",
            TERM,
            "TestTerminal",
            &[
                sub(b"DNAM", vec![0, 2, 0, 0]),
                sub(b"ITXT", z("Read the log")),
                sub(b"RNAM", z("Done.")),
                sub(b"ANAM", vec![1]),
                sub(b"INAM", NOTE.to_le_bytes().to_vec()),
                sub(b"ITXT", z("Sub menu")),
                sub(b"RNAM", z("")),
                sub(b"ANAM", vec![0]),
                sub(b"TNAM", TERM2.to_le_bytes().to_vec()),
            ],
        ),
        edid(b"TERM", TERM2, "TestTerminal2", &[sub(b"DNAM", vec![0, 2, 0, 0])]),
    ]
    .concat();
    // Quest delay 0.5 s; the other has none (5 s).
    let qust = [edid(b"QUST", QUST, "TestQuest", &[sub(b"DATA", [vec![0, 50, 0, 0], f32b(0.5)].concat())]), edid(b"QUST", QUST_NO_DELAY, "TestQuestNoDelay", &[sub(b"DATA", vec![0, 50, 0, 0])])].concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    [
        tes4(),
        top(b"NPC_", &npc),
        top(b"EFSH", &efsh),
        top(b"SOUN", &soun),
        top(b"MSET", &mset),
        top(b"CHIP", &chip),
        top(b"CSNO", &csno),
        top(b"NOTE", &note),
        top(b"TERM", &term),
        top(b"QUST", &qust),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Jg3.esm".into()).unwrap()];
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
fn effect_shader_traits_and_textures() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: GetEffectShaderTraitNumeric's table].
    for (t, want) in [(0, 9.0), (1, 5.0), (5, 0.25), (13, 1.5), (33, 3.0), (46, 0.75), (2, 0.0)] {
        assert_eq!(n(&mut g, &format!("GetEffectShaderTraitNumeric TestShader {t}")), want, "trait {t}");
    }
    // The colours as one number (red × 65536 + green × 256 + blue); 47 is colour key 1;
    // past it 0 [observed: jg3].
    assert_eq!(n(&mut g, "GetEffectShaderTraitNumeric TestShader 4"), 66051.0);
    assert_eq!(n(&mut g, "GetEffectShaderTraitNumeric TestShader 14"), 0.0);
    assert_eq!(n(&mut g, "GetEffectShaderTraitNumeric TestShader 47"), 263430.0);
    assert_eq!(n(&mut g, "GetEffectShaderTraitNumeric TestShader -1"), 0.0);
    // The setter returns 1 and takes every trait the getter reads, past 42 too.
    assert_eq!(n(&mut g, "SetEffectShaderTraitNumeric TestShader 13 2.5"), 1.0);
    assert_eq!(n(&mut g, "GetEffectShaderTraitNumeric TestShader 13"), 2.5);
    assert_eq!(n(&mut g, "SetEffectShaderTraitNumeric TestShader 46 9"), 1.0);
    assert_eq!(n(&mut g, "GetEffectShaderTraitNumeric TestShader 46"), 9.0);
    assert_eq!(n(&mut g, "SetEffectShaderTraitNumeric TestShader 4 255"), 1.0);
    assert_eq!(n(&mut g, "GetEffectShaderTraitNumeric TestShader 4"), 255.0);
    assert_eq!(n(&mut g, "SetEffectShaderTraitNumeric TestShader 48 1"), 0.0);
    assert_eq!(n(&mut g, "SetEffectShaderTraitNumeric TestShader 1 7.9"), 1.0);
    assert_eq!(n(&mut g, "GetEffectShaderTraitNumeric TestShader 1"), 7.0, "an enum keeps a whole number");
    // Textures: 0 fill (ICON), 1 particle (ICO2, none here), 2 holes (NAM7).
    assert_eq!(s(&mut g, "GetEffectShaderTexturePath TestShader 0"), "effects\\fill.dds");
    assert_eq!(s(&mut g, "GetEffectShaderTexturePath TestShader 1"), "");
    assert_eq!(s(&mut g, "GetEffectShaderTexturePath TestShader 2"), "effects\\holes.dds");
    q(&mut g, "SetEffectShaderTexturePath TestShader 1 \"effects\\new.dds\"");
    assert_eq!(s(&mut g, "GetEffectShaderTexturePath TestShader 1"), "effects\\new.dds");
    // An id past the table fails the call [observed: jg3].
    assert!(g.run_query("Begin Function { }\nstring_var sOut\nlet sOut := GetEffectShaderTexturePath TestShader 3\nEnd\n", RunCtx::default()).is_err());
}

#[test]
fn media_set_traits() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: GetMediaSetTraitNumeric's table: 0 type, 1 NAM8, 13 flags, 14 DNAM].
    for (t, want) in [(0, 3.0), (1, -6.0), (13, 5.0), (14, 20.0), (2, 0.0), (18, 0.0)] {
        assert_eq!(n(&mut g, &format!("GetMediaSetTraitNumeric TestMediaSet {t}")), want, "trait {t}");
    }
    q(&mut g, "SetMediaSetTraitNumeric TestMediaSet 14 30");
    assert_eq!(n(&mut g, "GetMediaSetTraitNumeric TestMediaSet 14"), 30.0);
    // The record's path under Data\Music\; a set one as given [observed: jg3].
    assert_eq!(s(&mut g, "GetMediaSetTraitString TestMediaSet 0"), "Data\\Music\\day.wav");
    assert_eq!(s(&mut g, "GetMediaSetTraitString TestMediaSet 5"), "");
    q(&mut g, "SetMediaSetTraitString TestMediaSet 3 \"night.wav\"");
    assert_eq!(s(&mut g, "GetMediaSetTraitString TestMediaSet 3"), "night.wav");
    assert!(g.run_query("Begin Function { }\nstring_var sOut\nlet sOut := GetMediaSetTraitString TestMediaSet 6\nEnd\n", RunCtx::default()).is_err());
    assert_eq!(n(&mut g, "GetMediaSetTraitSound TestMediaSet 0"), f64::from(SOUN));
    assert_eq!(n(&mut g, "GetMediaSetTraitSound TestMediaSet 1"), 0.0);
    q(&mut g, "SetMediaSetTraitSound TestMediaSet 1 TestSound2");
    assert_eq!(n(&mut g, "GetMediaSetTraitSound TestMediaSet 1"), f64::from(SOUN2));
    // Not a sound, or none: refused [observed: jg3].
    q(&mut g, "SetMediaSetTraitSound TestMediaSet 1 TestNote");
    assert_eq!(n(&mut g, "GetMediaSetTraitSound TestMediaSet 1"), f64::from(SOUN2));
    q(&mut g, "ref rNone\nSetMediaSetTraitSound TestMediaSet 1 rNone");
    assert_eq!(n(&mut g, "GetMediaSetTraitSound TestMediaSet 1"), f64::from(SOUN2));
}

#[test]
fn casino_chip_and_winnings() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetCasinoChip TestCasino"), f64::from(CHIP));
    q(&mut g, "SetCasinoChip TestCasino TestChip2");
    assert_eq!(n(&mut g, "GetCasinoChip TestCasino"), f64::from(CHIP2));
    assert_eq!(fnv_game::casino::chip(&g, FormId(CSNO)), Some(FormId(CHIP2)), "the tables use it");
    // Not a chip: refused.
    q(&mut g, "SetCasinoChip TestCasino TestNote");
    assert_eq!(n(&mut g, "GetCasinoChip TestCasino"), f64::from(CHIP2));
    assert_eq!(n(&mut g, "GetCasinoWinnings TestCasino"), 0.0);
    q(&mut g, "SetCasinoWinnings TestCasino 600");
    assert_eq!(n(&mut g, "GetCasinoWinnings TestCasino"), 600.0);
    // The winnings level follows: 600 of 1000 is level 2 (`casino::winnings_level`).
    assert_eq!(g.player.casino_levels.get(&FormId(CSNO)), Some(&2));
    // Huge values from negative earnings do not overflow (#1328 review); the argument is
    // an int, so the script hands over at most i32::MAX.
    q(&mut g, "SetCasinoWinnings TestCasino -5000");
    q(&mut g, "SetCasinoWinnings TestCasino 1000000000000000000000000");
    assert_eq!(g.player.casino_earnings[&FormId(CSNO)], i64::from(i32::MAX));
    assert_eq!(g.player.casino_levels.get(&FormId(CSNO)), Some(&4), "past the max winnings");
}

#[test]
fn terminal_menu_items() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetTerminalMenuItemCount TestTerminal"), 2.0);
    assert_eq!(n(&mut g, "GetTerminalMenuItemCount TestTerminal2"), 0.0);
    assert_eq!(s(&mut g, "GetTerminalMenuItemText TestTerminal 1 0"), "Read the log");
    assert_eq!(s(&mut g, "GetTerminalMenuItemText TestTerminal 2 0"), "Done.");
    assert_eq!(s(&mut g, "GetTerminalMenuItemText TestTerminal 1 2"), "", "no item 2");
    // textID 0 fails the call [observed: jg3].
    assert!(g.run_query("Begin Function { }\nstring_var sOut\nlet sOut := GetTerminalMenuItemText TestTerminal 0 0\nEnd\n", RunCtx::default()).is_err());
    assert_eq!(n(&mut g, "GetTerminalMenuItemNote TestTerminal 0"), f64::from(NOTE));
    assert_eq!(n(&mut g, "GetTerminalMenuItemSubmenu TestTerminal 1"), f64::from(TERM2));
    assert_eq!(n(&mut g, "GetTerminalMenuItemFlags TestTerminal 0"), 1.0);
    q(&mut g, "SetTerminalMenuItemText TestTerminal 1 0 \"Read it again\"");
    q(&mut g, "SetTerminalMenuItemText TestTerminal 2 0 \"Again.\"");
    q(&mut g, "SetTerminalMenuItemNote TestTerminal 0 TestNote2");
    q(&mut g, "SetTerminalMenuItemSubmenu TestTerminal 1 0");
    q(&mut g, "SetTerminalMenuItemFlags TestTerminal 0 6");
    assert_eq!(s(&mut g, "GetTerminalMenuItemText TestTerminal 1 0"), "Read it again");
    assert_eq!(n(&mut g, "GetTerminalMenuItemNote TestTerminal 0"), f64::from(NOTE2));
    assert_eq!(n(&mut g, "GetTerminalMenuItemSubmenu TestTerminal 1"), 0.0);
    assert_eq!(n(&mut g, "GetTerminalMenuItemFlags TestTerminal 0"), 6.0, "the flags kept whole");
    // Choosing the item shows the changes.
    let chosen = fnv_game::terminal::choose(&mut g, FormId(TERM), FormId(TERM), 0).unwrap();
    assert_eq!((chosen.result.as_str(), chosen.note), ("Again.", Some(FormId(NOTE2))));
    assert_eq!(fnv_game::terminal::choose(&mut g, FormId(TERM), FormId(TERM), 1).unwrap().submenu, None);
}

#[test]
fn quest_delay() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetQuestDelay TestQuest"), 0.5);
    assert_eq!(n(&mut g, "GetQuestDelay TestQuestNoDelay"), 0.0, "the record's, not the 5 s default");
    q(&mut g, "SetQuestDelay TestQuest 2");
    assert_eq!(n(&mut g, "GetQuestDelay TestQuest"), 2.0);
}
