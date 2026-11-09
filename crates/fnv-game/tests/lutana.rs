//! Lutana NVSE batch 1 (#1009; docs/spec-notes/ext-lutana.md) on a synthetic plugin:
//! challenges, casinos, recipes, actor AI data, perks, zones, leveled lists, activators,
//! terminals, load screens, form flags and playability, and the misc getters.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CHAL: u32 = 0x900;
const CSNO: u32 = 0x901;
const RCPE: u32 = 0x902;
const RCPE2: u32 = 0x903;
const RCCT: u32 = 0x904;
const ITEM_A: u32 = 0x905;
const ITEM_B: u32 = 0x906;
const ITEM_C: u32 = 0x907;
const NPC: u32 = 0x910;
const CSTY: u32 = 0x911;
const PERK_TRAIT: u32 = 0x912;
const PERK: u32 = 0x913;
const ECZN: u32 = 0x914;
const GLOB: u32 = 0x915;
const LVLI: u32 = 0x916;
const ACTI: u32 = 0x917;
const TACT: u32 = 0x918;
const SOUN: u32 = 0x919;
const TERM: u32 = 0x91A;
const NOTE: u32 = 0x91B;
const LSCR: u32 = 0x91C;
const LSCT: u32 = 0x91D;
const LSCT2: u32 = 0x91E;
const HAIR: u32 = 0x91F;
const LIGH: u32 = 0x920;

fn le(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

fn plugin_bytes() -> Vec<u8> {
    // CHAL DATA [documented: schemas/fnv/gameplay.json CHAL]: type 2 (KillInCategory),
    // threshold 50, flags 2 (Recurring), interval 10, values; SNAM, XNAM.
    let chal = edid(b"CHAL", CHAL, "TestChal", &[sub(b"DATA", [u32s(&[2, 50, 2, 10]), vec![0; 8]].concat()), sub(b"SNAM", le(ITEM_A)), sub(b"XNAM", le(ITEM_B))]);
    // CSNO DATA: shuffle share, payout, 7 reel stops, decks, max winnings, chip, quest,
    // soft 17 flags.
    let csno = edid(b"CSNO", CSNO, "TestCasino", &[sub(b"DATA", [f32b(0.5), f32b(1.5), u32s(&[2, 2, 2, 2, 2, 2, 2]), u32s(&[3, 9000, 0, 0, 0])].concat())]);
    // RCPE DATA: skill (Survival 44), level 30, category, subcategory; RCIL/RCOD pairs.
    let rcpe = edid(b"RCPE", RCPE, "TestRecipe", &[sub(b"DATA", u32s(&[44, 30, RCCT, 0])), sub(b"RCIL", le(ITEM_A)), sub(b"RCQY", le(2)), sub(b"RCOD", le(ITEM_C)), sub(b"RCQY", le(1))]);
    let rcpe2 = edid(b"RCPE", RCPE2, "TestRecipe2", &[sub(b"DATA", [(-1i32).to_le_bytes().to_vec(), u32s(&[0, 0, 0])].concat()), sub(b"RCIL", le(ITEM_A)), sub(b"RCQY", le(1)), sub(b"RCOD", le(ITEM_B)), sub(b"RCQY", le(3))]);
    let rcct = edid(b"RCCT", RCCT, "TestCategory", &[sub(b"DATA", vec![0])]);
    let misc = [edid(b"MISC", ITEM_A, "ItemA", &[]), edid(b"MISC", ITEM_B, "ItemB", &[]), edid(b"MISC", ITEM_C, "ItemC", &[])].concat();
    // AIDT [documented: schemas/fnv ActorAIData]: aggression … mood, 3 unused, services
    // (Weapons 1 | Food 16), teaches, max training, assistance, aggro behaviour on, radius
    // 650.
    let aidt = [vec![0, 0, 50, 50, 0, 0, 0, 0], le(1 | 16), vec![0, 0, 0, 1], le(650)].concat();
    let npc = edid(b"NPC_", NPC, "TestNPC", &[sub(b"AIDT", aidt), sub(b"ZNAM", le(CSTY))]);
    let npc_player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let csty = edid(b"CSTY", CSTY, "TestStyle", &[]);
    // PERK DATA: trait, min level, ranks, playable, hidden.
    let perks = [edid(b"PERK", PERK_TRAIT, "TestTrait", &[sub(b"DATA", vec![1, 1, 1, 1, 0])]), edid(b"PERK", PERK, "TestPerk", &[sub(b"DATA", vec![0, 2, 1, 0, 0])])].concat();
    // ECZN DATA: owner, rank, minimum level, flags (Never Resets), unused.
    let eczn = edid(b"ECZN", ECZN, "TestZone", &[sub(b"DATA", [le(0), vec![0, 5, 1, 0]].concat())]);
    let glob = edid(b"GLOB", GLOB, "TestGlobal", &[sub(b"FNAM", vec![b'f']), sub(b"FLTV", f32b(0.0))]);
    let lvli = edid(b"LVLI", LVLI, "TestList", &[sub(b"LVLD", vec![10]), sub(b"LVLF", vec![0]), sub(b"LVLG", le(GLOB))]);
    let acti = edid(b"ACTI", ACTI, "TestActivator", &[sub(b"RNAM", le(TACT))]);
    let tact = edid(b"TACT", TACT, "TestStation", &[]);
    let soun = edid(b"SOUN", SOUN, "TestSound", &[]);
    // TERM DNAM: difficulty 3 (Hard), flags, server, unused; PNAM the password note.
    let term = edid(b"TERM", TERM, "TestTerminal", &[sub(b"DNAM", vec![3, 0, 0, 0]), sub(b"PNAM", le(NOTE))]);
    let note = edid(b"NOTE", NOTE, "TestNote", &[sub(b"DATA", vec![0])]);
    let lscr = edid(b"LSCR", LSCR, "TestScreen", &[sub(b"WMI1", le(LSCT))]);
    let lsct = [edid(b"LSCT", LSCT, "TestType", &[]), edid(b"LSCT", LSCT2, "TestType2", &[])].concat();
    // HAIR DATA flags: Playable (1).
    let hair = edid(b"HAIR", HAIR, "TestHair", &[sub(b"DATA", vec![1])]);
    // LIGH DATA: time, radius 200, colour, flags, falloff, FOV, value, weight.
    let ligh = edid(b"LIGH", LIGH, "TestLight", &[sub(b"DATA", [le(0), le(200), vec![255, 255, 255, 0], le(0), f32b(1.0), f32b(90.0), le(0), f32b(0.0)].concat())]);
    [
        tes4(),
        top(b"GLOB", &glob),
        top(b"SOUN", &soun),
        top(b"NOTE", &note),
        top(b"MISC", &misc),
        top(b"LIGH", &ligh),
        top(b"HAIR", &hair),
        top(b"CSTY", &csty),
        top(b"NPC_", &[npc_player, npc].concat()),
        top(b"ACTI", &acti),
        top(b"TACT", &tact),
        top(b"TERM", &term),
        top(b"LVLI", &lvli),
        top(b"PERK", &perks),
        top(b"CSNO", &csno),
        top(b"CHAL", &chal),
        top(b"RCCT", &rcct),
        top(b"RCPE", &[rcpe, rcpe2].concat()),
        top(b"ECZN", &eczn),
        top(b"LSCT", &lsct),
        top(b"LSCR", &lscr),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Lutana.esm".into()).unwrap()];
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
fn challenges_read_and_set_their_record() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetChallengeType TestChal"), 2.0);
    assert_eq!(n(&mut g, "GetChallengeThreshold TestChal"), 50.0);
    assert_eq!(n(&mut g, "GetChallengeInterval TestChal"), 10.0);
    assert_eq!(n(&mut g, "GetChallengeFlags TestChal"), 2.0);
    assert_eq!(n(&mut g, "GetChallengeForm1 TestChal"), f64::from(ITEM_A));
    assert_eq!(n(&mut g, "GetChallengeForm2 TestChal"), f64::from(ITEM_B));
    q(&mut g, "SetChallengeThreshold TestChal 7
SetChallengeInterval TestChal 2
SetChallengeFlags TestChal 5
SetChallengeValue1 TestChal 3
SetChallengeValue3 TestChal 9
SetChallengeForm1 TestChal ItemC");
    assert_eq!(n(&mut g, "GetChallengeThreshold TestChal"), 7.0);
    assert_eq!(n(&mut g, "GetChallengeInterval TestChal"), 2.0);
    assert_eq!(n(&mut g, "GetChallengeFlags TestChal"), 5.0);
    assert_eq!(n(&mut g, "GetChallengeValue1 TestChal"), 3.0);
    assert_eq!(n(&mut g, "GetChallengeValue2 TestChal"), 0.0);
    assert_eq!(n(&mut g, "GetChallengeValue3 TestChal"), 9.0);
    assert_eq!(n(&mut g, "GetChallengeForm1 TestChal"), f64::from(ITEM_C));
    // A negative threshold is kept and read back signed (lutana1 chal.setthreshold.neg).
    q(&mut g, "SetChallengeThreshold TestChal -4");
    assert_eq!(n(&mut g, "GetChallengeThreshold TestChal"), -4.0);
    // Not a challenge: 0, and nothing is made.
    assert_eq!(n(&mut g, "GetChallengeThreshold ItemA"), 0.0);
    q(&mut g, "SetChallengeThreshold ItemA 3");
    assert_eq!(n(&mut g, "GetChallengeThreshold ItemA"), 0.0);
}

#[test]
fn casinos_read_and_set_their_record() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetCasinoNumDecks TestCasino"), 3.0);
    assert_eq!(n(&mut g, "GetCasinoMaxWinnings TestCasino"), 9000.0);
    assert_eq!(n(&mut g, "GetCasinoBJPayout TestCasino"), 1.5);
    assert_eq!(n(&mut g, "GetCasinoShufflePercent TestCasino"), 0.5);
    assert_eq!(n(&mut g, "GetCasinoBJ17Stand TestCasino"), 0.0);
    assert_eq!(n(&mut g, "GetCasinoReelStops TestCasino 7"), 2.0);
    // A symbol outside 1-7: -1 (lutana1 csno.reel.8).
    assert_eq!(n(&mut g, "GetCasinoReelStops TestCasino 8"), -1.0);
    q(&mut g, "SetCasinoNumDecks TestCasino 6
SetCasinoMaxWinnings TestCasino 12000
SetCasinoBJPayout TestCasino 2.5
SetCasinoShufflePercent TestCasino 0.25
SetCasinoBJ17Stand TestCasino 1
SetCasinoReelStops TestCasino 7 3");
    assert_eq!(n(&mut g, "GetCasinoNumDecks TestCasino"), 6.0);
    assert_eq!(n(&mut g, "GetCasinoMaxWinnings TestCasino"), 12000.0);
    assert_eq!(n(&mut g, "GetCasinoBJPayout TestCasino"), 2.5);
    assert_eq!(n(&mut g, "GetCasinoShufflePercent TestCasino"), 0.25);
    assert_eq!(n(&mut g, "GetCasinoBJ17Stand TestCasino"), 1.0);
    assert_eq!(n(&mut g, "GetCasinoReelStops TestCasino 7"), 3.0);
    // The tables play to the changed max winnings.
    assert_eq!(fnv_game::casino::max_winnings(&g, FormId(CSNO), 0), 12000);
    // As given, 0 decks and a share above 1 too (lutana1 csno.setnumdecks.0,
    // csno.setshufflepercent.over).
    q(&mut g, "SetCasinoNumDecks TestCasino 0
SetCasinoShufflePercent TestCasino 1.5");
    assert_eq!(n(&mut g, "GetCasinoNumDecks TestCasino"), 0.0);
    assert_eq!(n(&mut g, "GetCasinoShufflePercent TestCasino"), 1.5);
}

#[test]
fn recipes_read_and_set_their_lists() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetRecipeRequiredSkill TestRecipe"), 44.0);
    assert_eq!(n(&mut g, "GetRecipeRequiredSkillLevel TestRecipe"), 30.0);
    assert_eq!(n(&mut g, "GetRecipeRequiredSkill TestRecipe2"), -1.0);
    assert_eq!(n(&mut g, "GetRecipeInputCount TestRecipe ItemA"), 2.0);
    assert_eq!(n(&mut g, "GetRecipeOutputCount TestRecipe ItemC"), 1.0);
    assert_eq!(forms(&mut g, "GetFormRecipes ItemA"), vec![RCPE, RCPE2]);
    assert_eq!(forms(&mut g, "GetFormRecipes ItemA TestCategory"), vec![RCPE]);
    assert_eq!(forms(&mut g, "GetFormRecipeOutputs ItemA"), vec![ITEM_C, ITEM_B]);
    q(&mut g, "SetRecipeRequiredSkill TestRecipe 40
SetRecipeRequiredSkillLevel TestRecipe 45
SetRecipeInputCount TestRecipe ItemA 4
AddRecipeInputForm TestRecipe ItemB 1
AddRecipeOutputForm TestRecipe ItemB 2
ReplaceRecipeOutputForm TestRecipe ItemC ItemA");
    assert_eq!(n(&mut g, "GetRecipeRequiredSkill TestRecipe"), 40.0);
    assert_eq!(n(&mut g, "GetRecipeRequiredSkillLevel TestRecipe"), 45.0);
    assert_eq!(n(&mut g, "GetRecipeInputCount TestRecipe ItemA"), 4.0);
    assert_eq!(n(&mut g, "GetRecipeInputCount TestRecipe ItemB"), 1.0);
    assert_eq!(n(&mut g, "GetRecipeOutputCount TestRecipe ItemA"), 1.0);
    assert_eq!(n(&mut g, "GetRecipeOutputCount TestRecipe ItemC"), 0.0);
    // GetRecipeInputForms and GetRecipeOutputForms read the change too; an added entry
    // goes first (lutana1 rcpe.addinput.order).
    assert_eq!(forms(&mut g, "GetRecipeInputForms TestRecipe"), vec![ITEM_B, ITEM_A]);
    assert_eq!(forms(&mut g, "GetRecipeOutputForms TestRecipe"), vec![ITEM_B, ITEM_A]);
    q(&mut g, "RemoveRecipeInputForm TestRecipe ItemA
RemoveRecipeOutputForm TestRecipe ItemB
SetRecipeOutputCount TestRecipe ItemA 0");
    assert_eq!(forms(&mut g, "GetRecipeInputForms TestRecipe"), vec![ITEM_B]);
    assert_eq!(forms(&mut g, "GetRecipeOutputForms TestRecipe"), vec![ITEM_A]);
    // A count of 0 is kept (lutana1 rcpe.setoutcount.0).
    assert_eq!(n(&mut g, "GetRecipeOutputCount TestRecipe ItemA"), 0.0);
    // An item added twice is listed twice, the later first; the count read and the entry
    // removed are the first (lutana1 rcpe.addinput).
    q(&mut g, "AddRecipeInputForm TestRecipe ItemC 3
AddRecipeInputForm TestRecipe ItemC 2");
    assert_eq!(n(&mut g, "GetRecipeInputCount TestRecipe ItemC"), 2.0);
    q(&mut g, "RemoveRecipeInputForm TestRecipe ItemC");
    assert_eq!(n(&mut g, "GetRecipeInputCount TestRecipe ItemC"), 3.0);
    q(&mut g, "RemoveRecipeInputForm TestRecipe ItemC");
    // Without a skill the level stays (lutana1 rcpe.setskill.neg).
    q(&mut g, "SetRecipeRequiredSkill TestRecipe -1");
    assert_eq!(n(&mut g, "GetRecipeRequiredSkill TestRecipe"), -1.0);
    assert_eq!(n(&mut g, "GetRecipeRequiredSkillLevel TestRecipe"), 45.0);
    // The recipe no longer takes ItemA.
    assert_eq!(forms(&mut g, "GetFormRecipes ItemA"), vec![RCPE2]);
}

#[test]
fn actor_ai_data_services_and_aggro() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetServiceFlag TestNPC 16"), 1.0);
    assert_eq!(n(&mut g, "GetServiceFlag TestNPC 2"), 0.0);
    q(&mut g, "SetServiceFlag TestNPC 2 1
SetServiceFlag TestNPC 16 0");
    assert_eq!(n(&mut g, "GetServiceFlag TestNPC 2"), 1.0);
    assert_eq!(n(&mut g, "GetServiceFlag TestNPC 16"), 0.0);
    assert_eq!(n(&mut g, "LNGetAggroRadius TestNPC"), 650.0);
    q(&mut g, "LNSetAggroRadius 1000 TestNPC");
    assert_eq!(n(&mut g, "LNGetAggroRadius TestNPC"), 1000.0);
    assert_eq!(n(&mut g, "GetCombatStyle TestNPC"), f64::from(CSTY));
}

#[test]
fn record_getters_and_setters() {
    let f = fixture();
    let mut g = game(&f);
    // Perks.
    assert_eq!(n(&mut g, "IsTrait TestTrait"), 1.0);
    assert_eq!(n(&mut g, "IsTrait TestPerk"), 0.0);
    assert_eq!(n(&mut g, "LNIsPlayable TestTrait"), 1.0);
    assert_eq!(n(&mut g, "LNIsPlayable TestPerk"), 0.0);
    q(&mut g, "LNSetIsPlayable 1 TestPerk");
    assert_eq!(n(&mut g, "LNIsPlayable TestPerk"), 1.0);
    // Hair.
    assert_eq!(n(&mut g, "LNIsPlayable TestHair"), 1.0);
    q(&mut g, "LNSetIsPlayable 0 TestHair");
    assert_eq!(n(&mut g, "LNIsPlayable TestHair"), 0.0);
    // Zones: the flag reads back, and GetZoneMatchLevel sees it.
    assert_eq!(n(&mut g, "GetZoneFlag TestZone 1"), 1.0);
    assert_eq!(n(&mut g, "GetZoneFlag TestZone 2"), 0.0);
    q(&mut g, "SetZoneFlag TestZone 2 1
SetZoneFlag TestZone 1 0");
    assert_eq!(n(&mut g, "GetZoneFlag TestZone 2"), 1.0);
    assert_eq!(n(&mut g, "GetZoneFlag TestZone 1"), 0.0);
    assert_eq!(n(&mut g, "GetZoneMatchLevel TestZone"), 1.0);
    // Leveled lists.
    assert_eq!(n(&mut g, "GetChanceNoneGlobal TestList"), f64::from(GLOB));
    // Activators: the radio station, and the sound setters read by their getters.
    assert_eq!(n(&mut g, "GetActivatorRadioStation TestActivator"), f64::from(TACT));
    q(&mut g, "SetActivatorSoundActivate TestSound TestActivator
SetActivatorRadioStation 0 TestActivator");
    assert_eq!(n(&mut g, "GetActivatorSoundActivate TestActivator"), f64::from(SOUN));
    assert_eq!(n(&mut g, "GetActivatorRadioStation TestActivator"), 0.0);
    // Terminals.
    assert_eq!(n(&mut g, "GetTerminalLock TestTerminal"), 3.0);
    assert_eq!(n(&mut g, "GetPasswordNote TestTerminal"), f64::from(NOTE));
    q(&mut g, "SetPasswordNote 0 TestTerminal");
    assert_eq!(n(&mut g, "GetPasswordNote TestTerminal"), 0.0);
    // Load screens.
    assert_eq!(n(&mut g, "GetLoadScreenType TestScreen"), f64::from(LSCT));
    q(&mut g, "SetLoadScreenType TestScreen TestType2");
    assert_eq!(n(&mut g, "GetLoadScreenType TestScreen"), f64::from(LSCT2));
    // Lights.
    assert_eq!(n(&mut g, "GetRadius TestLight"), 200.0);
    q(&mut g, "PlayerREF.SetRadius 350 TestLight");
    assert_eq!(n(&mut g, "GetRadius TestLight"), 350.0);
    // Form flags, as GetFlagsLow reads them.
    assert_eq!(n(&mut g, "GetFormFlag ItemA 1024"), 0.0);
    q(&mut g, "SetFormFlag ItemA 1024 1");
    assert_eq!(n(&mut g, "GetFormFlag ItemA 1024"), 1.0);
    assert_eq!(n(&mut g, "GetFlagsLow ItemA") as u32 & 1024, 1024);
    q(&mut g, "SetFormFlag ItemA 1024 0");
    assert_eq!(n(&mut g, "GetFormFlag ItemA 1024"), 0.0);
}

#[test]
fn misc_getters() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetDefaultMessageTime"), 2.0);
    q(&mut g, "SetDefaultMessageTime 3.5");
    assert_eq!(n(&mut g, "GetDefaultMessageTime"), 3.5);
    // Kept as given (lutana1 misc.setdmt).
    q(&mut g, "SetDMT -1");
    assert_eq!(n(&mut g, "GetDefaultMessageTime"), -1.0);
    // Not registered by the installed JIP LN: its own text (lutana1 misc.mdt).
    assert_eq!(q(&mut g, "SetFunctionValue (GetMDT)"), XVal::Str("GetMDT".into()));
    assert_eq!(n(&mut g, "GetPCCanUsePowerArmor"), 0.0);
    q(&mut g, "SetPCCanUsePowerArmor 1");
    assert_eq!(n(&mut g, "GetPCCanUsePowerArmor"), 1.0);
    // AddNoteNS files the note without a corner message.
    q(&mut g, "AddNoteNS TestNote");
    assert!(g.player.notes.contains(&FormId(NOTE)));
    assert!(g.take_note_notices().is_empty());
    match q(&mut g, "string_var s
let s := GetTimeStamp
SetFunctionValue s") {
        XVal::Str(s) => assert!(s.len() == 8 && s.as_bytes()[2] == b'-' && s.as_bytes()[5] == b'-', "{s}"),
        v => panic!("{v:?}"),
    }
    // Nothing under the crosshair.
    assert_eq!(n(&mut g, "CrosshairRefInList TestList"), 0.0);
}
