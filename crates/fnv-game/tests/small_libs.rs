//! The small extender libraries (#1013, #1015-#1020): kNVSE's second slice, JIP PP LN,
//! vheyes-nvse, Anh's and CacheUI's last functions, NX_GetQVEVFL and the small plugins
//! (docs/spec-notes/ext-knvse.md, ext-jippp.md, ext-vheyes.md, ext-small.md), run through
//! scripts on a synthetic plugin and made-up files (no game data).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{ActiveEffect, Game, GameConfig};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;
use serde_json::Value as Json;
use std::sync::Arc;

const WEAPON: u32 = 0x800;
const AMMO: u32 = 0x801;
const AMMO_EFFECT: u32 = 0x802;
const LIGHT: u32 = 0x803;
const WORLD: u32 = 0x804;
const REGION: u32 = 0x805;
const OUTSIDE: u32 = 0x806;
const INSIDE: u32 = 0x807;
const BOX: u32 = 0x808;
const BOX_REF: u32 = 0x809;
const QUEST: u32 = 0x80A;
const SPELL: u32 = 0x80B;
const EFFECT: u32 = 0x80C;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn plugin_bytes() -> Vec<u8> {
    // WEAP DNAM (204 bytes): flags2 at 56 with bit 7 (Override - Damage to Weapon Mult),
    // the override at 84, the skill at 104 (schemas/fnv WeaponDNAM).
    let mut dnam = vec![0u8; 204];
    dnam[56..60].copy_from_slice(&(1u32 << 7).to_le_bytes());
    dnam[84..88].copy_from_slice(&0.5f32.to_le_bytes());
    dnam[104..108].copy_from_slice(&38u32.to_le_bytes());
    let weap_data = [&10i32.to_le_bytes()[..], &100i32.to_le_bytes(), &0.5f32.to_le_bytes(), &10i16.to_le_bytes(), &[12u8]].concat();
    let weap = record(b"WEAP", 0, WEAPON, &[sub(b"EDID", z("TestGun")), sub(b"DATA", weap_data), sub(b"DNAM", dnam)]);
    // AMEF DATA: Weapon Condition Mod (4), Multiply (1), 3.
    let amef = record(b"AMEF", 0, AMMO_EFFECT, &[sub(b"EDID", z("TestWear")), sub(b"DATA", [4u32.to_le_bytes().to_vec(), 1u32.to_le_bytes().to_vec(), f32b(3.0)].concat())]);
    let ammo = record(b"AMMO", 0, AMMO, &[sub(b"EDID", z("TestAmmo")), sub(b"RCIL", AMMO_EFFECT.to_le_bytes().to_vec())]);
    let ligh_data = [0u32.to_le_bytes().to_vec(), 200u32.to_le_bytes().to_vec(), vec![10, 20, 30, 0], 0u32.to_le_bytes().to_vec(), f32b(1.0), f32b(90.0), 0u32.to_le_bytes().to_vec(), f32b(0.0)].concat();
    let ligh = record(b"LIGH", 0, LIGHT, &[sub(b"EDID", z("TestLight")), sub(b"DATA", ligh_data)]);
    let wrld = record(b"WRLD", 0, WORLD, &[sub(b"EDID", z("TestWorld"))]);
    let regn = record(b"REGN", 0, REGION, &[sub(b"EDID", z("TestRegion"))]);
    let acti = record(b"ACTI", 0, BOX, &[sub(b"EDID", z("TestBox")), sub(b"MODL", z("Test\\Box.nif"))]);
    let qust = record(b"QUST", 0, QUEST, &[sub(b"EDID", z("TestQuest"))]);
    let mgef = record(b"MGEF", 0, EFFECT, &[sub(b"EDID", z("TestEffect"))]);
    let spel = record(b"SPEL", 0, SPELL, &[sub(b"EDID", z("TestSpell"))]);
    let outside = record(b"CELL", 0, OUTSIDE, &[sub(b"EDID", z("TestOutside")), sub(b"DATA", vec![0]), sub(b"XCLR", REGION.to_le_bytes().to_vec())]);
    let inside = record(b"CELL", 0, INSIDE, &[sub(b"EDID", z("TestInside")), sub(b"DATA", vec![1]), sub(b"XCLR", REGION.to_le_bytes().to_vec())]);
    let box_ref = record(b"REFR", 0, BOX_REF, &[sub(b"EDID", z("TestBoxRef")), sub(b"NAME", BOX.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let children = group(INSIDE.to_le_bytes(), 6, &group(INSIDE.to_le_bytes(), 9, &box_ref));
    [
        tes4(),
        top(b"MGEF", &mgef),
        top(b"ACTI", &acti),
        top(b"LIGH", &ligh),
        top(b"WEAP", &weap),
        top(b"AMMO", &ammo),
        top(b"AMEF", &amef),
        top(b"REGN", &regn),
        top(b"SPEL", &spel),
        top(b"QUST", &qust),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[outside, inside, children].concat()))),
        top(b"WRLD", &wrld),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Small.esm".into()).unwrap()];
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
    g.player_entered(Some(FormId(INSIDE)));
    let src = MemSource::default().with(
        "menus\\type.xml",
        r#"<menu name="TypeMenu"> <rect name="Line"> <_TS0> </_TS0> <_TS1> </_TS1> <_TS2> </_TS2>
             <_TA0> 0 </_TA0> <_TA1> 0 </_TA1> <_TA2> 0 </_TA2> <_flag> 0 </_flag> </rect> </menu>"#,
    );
    g.ui_tree.load_menu(&src, "menus\\type.xml").unwrap();
    g.ui_tree.update();
    g
}

/// A data folder made for the run: the KF files the kNVSE functions look for, Stewie's
/// Tweaks' INI and vheyes-nvse's queued command, and the plugin bytes TTW_CRC32 reads.
fn files() {
    use fnv_game::mod_files::{set_roots, Roots};
    let dir = std::env::temp_dir().join(format!("nvf-small-libs-{}", std::process::id()));
    // Written once per test process: the tests run in parallel, and writing a file again
    // empties it for a moment while another test may be reading it.
    static WRITE: std::sync::Once = std::sync::Once::new();
    WRITE.call_once(|| {
        std::fs::create_dir_all(dir.join("NVSE/Plugins")).unwrap();
        std::fs::create_dir_all(dir.join("NVSE/user_defined_functions")).unwrap();
        std::fs::create_dir_all(dir.join("Config/vheyes-nvse")).unwrap();
        std::fs::write(
            dir.join("NVSE/user_defined_functions/TileProbe.txt"),
            "Begin Function { }\nSetUIFloat \"HUDMainMenu/Probe/_calls\" ((GetUIFloat \"HUDMainMenu/Probe/_calls\") + 1)\nEnd\n",
        ).unwrap();
        std::fs::write(dir.join("NVSE/Plugins/nvse_stewie_tweaks.ini"), "[Tweaks]\r\nbJumpWhileAiming=1\r\n[Cookable Grenades]\r\nsSoundName=\"WPNGrenadeBeep\"\r\n").unwrap();
        std::fs::write(dir.join("Config/vheyes-nvse/QueuedCommand.txt"), "SetGlobalTimeMultiplier 2\r\n").unwrap();
    });
    set_roots(Roots {
        read: vec![dir],
        lookup: Some(Arc::new(|p: &str| u8::from(p.to_ascii_lowercase().starts_with("meshes\\test\\")))),
        reader: Some(Arc::new(|p: &str| p.eq_ignore_ascii_case("Small.esm").then(|| b"123456789".to_vec()))),
        ..Roots::default()
    });
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), Default::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(v: f64) -> XVal {
    XVal::Num(v)
}

#[test]
fn compile_script_from_loose_root_runs_as_a_tile_callback() {
    files();
    let f = fixture();
    let mut g = game(&f);
    // [design: script_input.rs, mod_files.rs] The home mods layer is a read root;
    // the host VFS reader does not contain this loose callback.
    assert!(fnv_game::mod_files::read_data("nvse\\user_defined_functions\\tileprobe.txt").is_none());
    let src = MemSource::default().with(
        "menus\\probe.xml",
        r#"<menu name="HUDMainMenu"><rect name="Probe"><_probe>1</_probe><_calls>0</_calls></rect></menu>"#,
    );
    g.ui_tree.load_menu(&src, "menus\\probe.xml").unwrap();
    g.ui_tree.update();
    let callback = q(&mut g, "SetFunctionValue (CompileScript \"tileprobe.txt\")");
    assert!(matches!(callback, XVal::Ref(id) if id != 0), "{callback:?}");
    assert_eq!(q(&mut g, "SetFunctionValue (GetUDFFromFile \"TileProbe.txt\")"), callback);
    assert_eq!(q(&mut g, "SetFunctionValue (CompileScript \"missing.txt\")"), XVal::Ref(0));
    q(&mut g, "SetTileScript \"hudmainmenu/PROBE/_PROBE\" (CompileScript \"TileProbe.txt\")");
    g.tick();
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/Probe/_calls"), Some(1.0));
    g.ui_tree.set_float("HUDMainMenu\\Probe\\_probe", 0.0);
    g.tick();
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/Probe/_calls"), Some(1.0));
    g.ui_tree.set_float("HUDMainMenu/Probe/_probe", 1.0);
    g.tick();
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/Probe/_calls"), Some(2.0));
}

fn s(v: &str) -> XVal {
    XVal::Str(v.into())
}

#[test]
fn knvse_clock_weights_text_keys_and_attributes() {
    files();
    let f = fixture();
    let mut g = game(&f);
    // The clock starts with the first frame [observed: knvse-anim, Ceil 1 soon after].
    assert_eq!(q(&mut g, "SetFunctionValue (Ceil (PlayerREF.GetGlobalAnimTime 0))"), n(1.0));
    q(&mut g, "PlayerREF.SetGlobalAnimTime 5");
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.GetGlobalAnimTime)"), n(5.0));
    g.tick();
    let after = q(&mut g, "SetFunctionValue (PlayerREF.GetGlobalAnimTime)");
    assert_eq!(after, n(5.0 + g.cfg.frame_seconds));
    // The current time is the offset plus the clock; a change shows from the next frame.
    q(&mut g, "PlayerREF.SetAnimCurrentTime \"test\\a.kf\" 1.5");
    assert_ne!(q(&mut g, "SetFunctionValue (PlayerREF.GetAnimCurrentTime \"test\\a.kf\")"), n(1.5));
    g.tick();
    let XVal::Num(now) = q(&mut g, "SetFunctionValue (PlayerREF.GetAnimCurrentTime \"test\\a.kf\")") else { panic!("a number") };
    assert!((now - (1.5 + g.cfg.frame_seconds)).abs() < 1e-9, "{now}");
    // Text keys: added for every actor, only for a file that exists.
    assert_eq!(q(&mut g, "SetFunctionValue (AddAnimTextKey \"test\\a.kf\" 0.5 \"Hit\")"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (AddAnimTextKey \"other\\none.kf\" 0.5 \"Hit\")"), n(0.0));
    assert_eq!(q(&mut g, "array_var a\nlet a := GetAnimationAttributes \"test\\a.kf\"\nSetFunctionValue (ar_Size a)"), n(13.0));
    assert_eq!(q(&mut g, "array_var a\nlet a := GetAnimationAttributes \"test\\a.kf\"\nlet a := a[\"textKeyValues\"]\nSetFunctionValue a[0]"), s("Hit"));
    assert_eq!(q(&mut g, "array_var a\nlet a := GetAnimationAttributes \"test\\a.kf\"\nSetFunctionValue a[\"sequenceName\"]"), s("a"));
    // Played on the actor: 22 keys, the weight set.
    q(&mut g, "PlayerREF.ActivateAnim \"test\\a.kf\"");
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.GetAnimCurrentTime \"test\\a.kf\")"), n(0.0));
    q(&mut g, "PlayerREF.SetAnimWeight \"test\\a.kf\" 0.25");
    assert_eq!(q(&mut g, "array_var a\nlet a := PlayerREF.GetAnimationAttributes \"test\\a.kf\"\nSetFunctionValue (ar_Size a)"), n(22.0));
    assert_eq!(q(&mut g, "array_var a\nlet a := PlayerREF.GetAnimationAttributes \"test\\a.kf\"\nSetFunctionValue a[\"seqWeight\"]"), n(0.25));
    // SetAnimTextKeys does nothing while the actor plays the sequence.
    q(&mut g, "PlayerREF.SetAnimTextKeys \"test\\a.kf\" (ar_List 0, 1) (ar_List \"S\", \"E\")");
    assert_eq!(q(&mut g, "array_var a\nlet a := PlayerREF.GetAnimationAttributes \"test\\a.kf\"\nlet a := a[\"textKeyValues\"]\nSetFunctionValue (ar_Size a)"), n(1.0));
    // CrossFadeAnims: the source must play and the destination not.
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.CrossFadeAnims \"test\\b.kf\" \"test\\a.kf\")"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.CrossFadeAnims \"test\\a.kf\" \"test\\b.kf\")"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.IsAnimSequencePlaying \"test\\a.kf\")"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.IsAnimSequencePlaying \"test\\b.kf\")"), n(1.0));
    // BlendToAnim: 1 for a destination that exists, the source playing or not; a playing
    // source stops at weight 0. A sequence the actor does not play has no attributes.
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.BlendToAnim \"test\\a.kf\" \"other\\c.kf\")"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.GetAnimationAttributes \"test\\c.kf\")"), XVal::Array(0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.BlendToAnim \"test\\b.kf\" \"test\\c.kf\")"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.IsAnimSequencePlaying \"test\\b.kf\")"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.BlendToAnimFromPose \"test\\d.kf\")"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.SetAnimAdditive \"test\\d.kf\" 0 \"test\\a.kf\")"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.SetAnimAdditive \"other\\d.kf\")"), n(0.0));
    // No body clip told: no sequence of a type plays, ar_Null.
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.GetPlayingAnimBySequenceType 0)"), XVal::Array(0));
    // Overrides copied between forms.
    q(&mut g, "SetWeaponAnimationPath TestGun 0 1 \"test\\gun.kf\"\nCopyAnimationsToForm TestGun TestLight");
    assert_eq!(g.anim_paths.weapon.get(&(FormId(LIGHT), false)).cloned(), Some(vec!["test\\gun.kf".to_string()]));
}

#[test]
fn jippp_runtime_nodes_effects_and_wear() {
    files();
    let f = fixture();
    let mut g = game(&f);
    // InsertNode / AttachModel calls fill the lists HasRuntimeNode reads.
    q(&mut g, "InsertNode TestBox 1 \"Weapon\\BarrelDevice|Second_Barrel\"\nAttachModel TestBox 1 \"Weapon|test\\scope.nif\"");
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestBox \"Weapon\\BarrelDevice|Second_Barrel\" 1)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestBox \"Weapon\\BarrelDevice|Second_Barrel\" 2)"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestBox \"Weapon|test\\scope.nif\" 2)"), n(1.0));
    // Mode 0 reads the target path only, not the new node's name [observed: jipppln].
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestBox \"BarrelDevice\" 0)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestBox \"Second_Barrel\" 0)"), n(0.0));
    // A query belongs to no plugin: it copies and clears nothing [observed: jipppln].
    assert_eq!(q(&mut g, "SetFunctionValue (CopyRuntimeNodes TestGun TestBox 0)"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (ClearRuntimeNodes TestBox)"), n(0.0));
    // A plugin's script: copied (overwrite, then concatenate), attached as a whole form,
    // and only its own entries cleared.
    let (gun, box_, light) = (Some(FormId(WEAPON)), XVal::Ref(BOX), XVal::Ref(LIGHT));
    let as_mod = |g: &mut Game, name: &str, args: &[XVal]| fnv_game::nvse_jippp::call_as(g, "small.esp", name, None, args).unwrap().unwrap();
    assert_eq!(as_mod(&mut g, "CopyRuntimeNodes", &[XVal::Ref(WEAPON), box_.clone(), n(0.0)]), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestGun \"BarrelDevice\")"), n(1.0));
    assert_eq!(as_mod(&mut g, "InsertNode", &[XVal::Ref(WEAPON), n(1.0), s("Bip01|Mine")]), n(0.0));
    assert_eq!(as_mod(&mut g, "AttachFormModel", &[light.clone(), box_.clone(), n(1.0), s("Root|*_X*")]), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestLight \"Root|Test\\Box.nif\" 2)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestLight \"Weapon\\BarrelDevice|Second_Barrel*_X*\" 1)"), n(1.0));
    // A remove succeeds again with nothing left to remove [observed: jipppln].
    assert_eq!(as_mod(&mut g, "AttachFormModel", &[light.clone(), box_.clone(), n(0.0), s("Root")]), n(1.0));
    assert_eq!(as_mod(&mut g, "AttachFormModel", &[light, box_, n(0.0), s("Root")]), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestLight \"Root|Test\\Box.nif\" 2)"), n(0.0));
    // The copied entries are the query's (no plugin), so only the mod's own one goes.
    assert_eq!(fnv_game::nvse_jippp::call_as(&mut g, "small.esp", "ClearRuntimeNodes", gun, &[XVal::Ref(WEAPON)]).unwrap().unwrap(), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestGun \"Bip01\")"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestGun \"BarrelDevice\")"), n(1.0));
    // Wear per shot: the record's override 0.5, times the ammunition's 3.
    assert_eq!(q(&mut g, "SetFunctionValue (GetCalculatedWeaponDegradation TestGun TestAmmo)"), n(1.5));
    // Active effects as maps, filtered and written back.
    g.effects.push(ActiveEffect {
        id: 77,
        target: FormId::PLAYER_REF,
        source: FormId(SPELL),
        effect: FormId(EFFECT),
        index: 0,
        magnitude: 10,
        duration: 30.0,
        elapsed: 10.0,
        permanent: false,
        caster: None,
        script: None,
        change: None,
        usage: 0.0,
        worn: None,
        inactive: false,
    });
    let info = "array_var a\narray_var e\nlet a := PlayerREF.GetActiveEffectsInfo";
    assert_eq!(q(&mut g, &format!("{info}\nSetFunctionValue (ar_Size a)")), n(1.0));
    assert_eq!(q(&mut g, &format!("{info} 2\nlet e := a[0]\nSetFunctionValue e[\"TimeLeft\"]")), n(20.0));
    // None to list: no array [observed: jipppln].
    assert_eq!(q(&mut g, &format!("{info} 3\nSetFunctionValue (ar_Size a)")), n(-1.0));
    assert_eq!(q(&mut g, &format!("{info} 0 0\nSetFunctionValue (ar_Size a)")), n(-1.0));
    assert_eq!(q(&mut g, &format!("{info} TestEffect\nlet e := a[0]\nSetFunctionValue e[\"MetaData\"]")), n(77.0));
    q(&mut g, &format!("{info}\nlet e := a[0]\nlet e[\"Magnitude\"] := 20\nlet e[\"TimeLeft\"] := 99\nPlayerREF.SetActiveEffectsInfo a"));
    let e = g.effects.iter().find(|e| e.id == 77).unwrap();
    assert_eq!((e.magnitude, e.elapsed), (20, 0.0));
    // A control's key and mouse button; a pad's is unbound.
    assert_eq!(q(&mut g, "SetFunctionValue (GetControlCodeByID 7 0)"), q(&mut g, "SetFunctionValue (GetControl 7 0)"));
    assert_eq!(q(&mut g, "SetFunctionValue (GetControlCodeByID 7 2)"), n(-1.0));
    // Asked every frame while nothing takes the queue: in it once.
    for _ in 0..3 {
        q(&mut g, "TestBoxRef.ClampToGround");
    }
    assert_eq!(g.take_clamp_requests(), vec![FormId(BOX_REF)]);
    assert_eq!(g.take_clamp_requests(), Vec::<FormId>::new());
    // On a base form (a weapon is its own base) the call stops the script, in a
    // non-strict run too [observed: jipppln clamp.base, "script call failed"].
    let base = "Begin Function { }\nref r\nlet r := TestGun\nSetFunctionValue (r.ClampToGround)\nEnd\n";
    let mut loose = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    for g in [&mut g, &mut loose] {
        assert_eq!(g.run_query(base, Default::default()).unwrap_err(), "clamptoground called on no reference");
    }
    assert_eq!(g.take_clamp_requests(), Vec::<FormId>::new());
    // Mode 2 removes [documented: InsertNode's addOrRemove table]: off the runtime list,
    // and nothing added to the list or the scene; a spec without `|` is not kept.
    q(&mut g, "InsertNode TestBox 2 \"Weapon\\BarrelDevice|Second_Barrel\"");
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestBox \"Weapon\\BarrelDevice|Second_Barrel\" 1)"), n(0.0));
    q(&mut g, "InsertNode TestBox 1 \"NoBar\"");
    assert_eq!(q(&mut g, "SetFunctionValue (HasRuntimeNode TestBox \"NoBar\" 1)"), n(0.0));
}

/// GetSessionTime adds up each frame's own length too: frames of changing length move it
/// on by exactly their sum, never back (#1297).
#[test]
fn session_time_adds_up_frames_of_changing_length() {
    files();
    let f = fixture();
    let mut g = game(&f);
    let secs = |g: &mut Game| match q(g, "SetFunctionValue (GetSessionTime)") {
        XVal::Num(x) => x,
        v => panic!("{v:?}"),
    };
    let start = secs(&mut g);
    let mut want = start;
    let mut last = start;
    for (i, dt) in [1.0 / 60.0, 1.0 / 30.0, 0.0166, 0.017, 0.1, 0.008].iter().cycle().take(600).enumerate() {
        g.cfg.frame_seconds = *dt;
        g.tick();
        want += dt;
        let now = secs(&mut g);
        assert!(now >= last, "frame {i}: {last} then {now}");
        assert!((now - want).abs() < 1e-9, "frame {i}: {now}, frames add up to {want}");
        last = now;
    }
    // A long session with a frame-time change: the clock does not jump with it.
    g.cfg.frame_seconds = 0.0166;
    for _ in 0..36_000 {
        g.tick();
    }
    let before = secs(&mut g);
    g.cfg.frame_seconds = 0.017;
    g.tick();
    assert!((secs(&mut g) - before - 0.017).abs() < 1e-6);
}

/// The CacheUI and vheyes clocks add up each frame's own length: the engine gives every
/// frame its own `frame_seconds`, so a change of it does not move the clock back or on.
#[test]
fn clocks_add_up_frames_of_changing_length() {
    files();
    let f = fixture();
    let mut g = game(&f);
    // 0.25 s of 1/60 frames, then the frame length doubles: the typewriter (1 s, no
    // fade) is half way at 0.5 s, not at 0.25 s + 15 × 2/60.
    g.cfg.frame_seconds = 1.0 / 60.0;
    for _ in 0..15 {
        g.tick();
    }
    q(&mut g, "SetUITypewriter \"TypeMenu\\Line\" \"abcd\" 1 0");
    g.cfg.frame_seconds = 2.0 / 60.0;
    for _ in 0..8 {
        g.tick();
    }
    // 8 × 2/60 = 0.267 s since the start: letters 0 and 1 (0 and 0.25 s) are in.
    assert_eq!(g.ui_tree.get_float("TypeMenu\\Line\\_TA1"), Some(255.0));
    assert_eq!(g.ui_tree.get_float("TypeMenu\\Line\\_TA2"), Some(0.0));
    // The moon's fade: 1 s, just past half way (alpha 1/30, the looks changed) after 31
    // frames of 1/60.
    q(&mut g, "SetMoonFade 1 0 1 0");
    g.cfg.frame_seconds = 1.0 / 60.0;
    for _ in 0..31 {
        g.tick();
    }
    let (look, _, alpha) = g.moon_look(1.0);
    assert!((alpha - 1.0 / 30.0).abs() < 1e-6, "{alpha}");
    assert!(look.doom);
    // A zero fade changes the looks at once.
    for _ in 0..40 {
        g.tick();
    }
    q(&mut g, "SetMoonFade 0 0 0 1");
    let (look, _, alpha) = g.moon_look(1.0);
    assert!(look.full && !look.doom && alpha == 1.0);
    // A reference queued for its 3D again and again is in the queue once.
    for _ in 0..3 {
        q(&mut g, "TestBoxRef.QueueRef3D");
    }
    assert_eq!(g.take_rebuild_3d(), vec![FormId(BOX_REF)]);
}

#[test]
fn vheyes_moon_stamps_and_queued_command() {
    files();
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(q(&mut g, "SetFunctionValue (GetMoonDoomState)"), n(0.0));
    q(&mut g, "SetMoonDoomSet 1");
    assert_eq!(q(&mut g, "SetFunctionValue (GetMoonDoomState)"), n(1.0));
    q(&mut g, "SetMoonDoom 1");
    assert_eq!(q(&mut g, "SetFunctionValue (GetMoonDoomState)"), n(2.0));
    // The fade changes the look at its midpoint; a second call meanwhile is ignored.
    q(&mut g, "SetMoonFade 0.1 0 0 1\nSetMoonFade 9 1 1 1");
    for _ in 0..4 {
        g.tick();
    }
    assert_eq!(q(&mut g, "SetFunctionValue (GetMoonDoomState)"), n(0.0));
    q(&mut g, "SetMoonGrowth 1 2 3 0");
    assert_eq!(g.moon_look(1.0).1, Some(2.0));
    assert_eq!(g.moon_look(5.0).1, Some(3.0));
    q(&mut g, "SetMoonScale 0.5");
    assert_eq!(g.moon_look(1.0).1, Some(0.5));
    // Stamps: first touch stamps and says 0, a changed model path says 1.
    assert_eq!(q(&mut g, "SetFunctionValue (TestBoxRef.QueueRefCloneHard)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestBoxRef.QueueRefClone)"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestBoxRef.QueueRefClone)"), n(0.0));
    q(&mut g, "SetModelPath \"Test\\Other.nif\" TestBox");
    assert_eq!(q(&mut g, "SetFunctionValue (TestBoxRef.QueueRefClone)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestBoxRef.DeferRefUpdate3D 1)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.DeferRefUpdate3D 1)"), n(0.0));
    g.tick();
    assert_eq!(g.take_rebuild_3d(), vec![FormId(BOX_REF)]);
    assert_eq!(q(&mut g, "SetFunctionValue (TestBoxRef.QueueRefClone)"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (ClearRefStamps)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestBoxRef.QueueRefCloneHard)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetRefCloneQueueCount)"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestBoxRef.DetachRef3D)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.QueueRef3D)"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestBoxRef.GetStoredAmmoRounds)"), n(-1.0));
    // The queued console line runs two frames later, once.
    assert_eq!(q(&mut g, "SetFunctionValue (ArmQueuedConsoleCommand)"), n(1.0));
    g.tick();
    assert_eq!(q(&mut g, "SetFunctionValue (GetGlobalTimeMultiplier)"), n(1.0));
    g.tick();
    assert_eq!(q(&mut g, "SetFunctionValue (GetGlobalTimeMultiplier)"), n(2.0));
}

#[test]
fn cacheui_anh_and_the_small_plugins() {
    files();
    let f = fixture();
    let mut g = game(&f);
    // Anh.
    assert_eq!(q(&mut g, "SetFunctionValue (ArrayToString (ar_List 1, 2))"), s("[1, 2]"));
    assert_eq!(q(&mut g, "SetFunctionValue (GetCameraFOV 1)"), n(55.0));
    q(&mut g, "SetCameraFOV 90 70");
    assert_eq!(q(&mut g, "SetFunctionValue (GetCameraFOV 0)"), n(90.0));
    // CacheUI.
    assert_eq!(q(&mut g, "SetFunctionValue (GetFPS)"), n(60.0));
    let refs = q(&mut g, "array_var a\nlet a := SplitStringToRef \"TestGun, TestAmmo,Small.esm:803,Nothing\" \",\"\nSetFunctionValue (ar_Size a)");
    assert_eq!(refs, n(3.0));
    assert_eq!(q(&mut g, "array_var a\nlet a := SplitStringToRef \"TestGun,Small.esm:803\" \",\"\nSetFunctionValue a[1]"), XVal::Ref(LIGHT));
    // The typewriter: letters at once, the alphas over the duration.
    q(&mut g, "SetUITypewriter \"TypeMenu\\Line\" \"abc\" 0.5 0.1");
    assert_eq!(g.ui_tree.get_string("TypeMenu\\Line\\_TS1"), Some("b".to_string()));
    assert_eq!(g.ui_tree.get_float("TypeMenu\\Line\\_TA2"), Some(0.0));
    for _ in 0..60 {
        g.tick();
    }
    assert_eq!(g.ui_tree.get_float("TypeMenu\\Line\\_TA2"), Some(255.0));
    q(&mut g, "SetUITypewriter \"TypeMenu\\Line\" \"abc\" 0");
    assert_eq!(g.ui_tree.get_float("TypeMenu\\Line\\_TA0"), Some(0.0));
    // The frame loop: every 0.05 s, two frames after.
    q(&mut g, "SetFrameControllerLoop ({} => SetUIFloat \"TypeMenu\\Line\\_flag\" ((GetUIFloat \"TypeMenu\\Line\\_flag\") + 1)) 1 0.05 2");
    for _ in 0..12 {
        g.tick();
    }
    let calls = g.ui_tree.get_float("TypeMenu\\Line\\_flag").unwrap();
    assert!((3.0..=4.0).contains(&calls), "{calls}");
    // The NX, Sandbox and plugin-less functions.
    assert_eq!(q(&mut g, "SetFunctionValue (NX_GetQVEVFL)"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetButcherPeteVersion)"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetQuestFailedAlt TestQuest)"), n(0.0));
    q(&mut g, "FailQuest TestQuest");
    assert_eq!(q(&mut g, "SetFunctionValue (GetQuestFailedAlt TestQuest)"), n(1.0));
    // Overcharge, yUI, hNVSE, Stewie's Tweaks, TTW.
    q(&mut g, "SetOCSettings 8 0.35\nPlayerREF.SetOCWeaponState TestGun 16 73");
    assert_eq!(q(&mut g, "SetFunctionValue (GetOCSettings 8)"), n(0.35));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.GetOCWeaponState TestGun 16)"), n(73.0));
    assert!(matches!(q(&mut g, "SetFunctionValue (CreateOCLightClone TestLight PlayerREF)"), XVal::Ref(r) if r != 0 && r != LIGHT));
    q(&mut g, "ySISetTrait \"tag\" \"(fba1)\" TestGun\nySISetTrait \"tag\" \"(fba1)\" TestAmmo\nySISetTrait \"icon\" \"Interface\\Icons\\X.dds\" TestGun");
    assert_eq!(q(&mut g, "SetFunctionValue (ySIGetTrait \"icon\" TestAmmo)"), s("Interface\\Icons\\X.dds"));
    assert_eq!(q(&mut g, "array_var a\nlet a := GetCellRegions TestOutside\nSetFunctionValue a[0]"), XVal::Ref(REGION));
    assert_eq!(q(&mut g, "array_var a\nlet a := GetCellRegions TestInside\nSetFunctionValue (ar_Size a)"), n(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (StewieGetValue \"bJumpWhileAiming:Tweaks\")"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (StewieGetValue \"sSoundName:Cookable Grenades\")"), s("WPNGrenadeBeep"));
    assert_eq!(q(&mut g, "SetFunctionValue (TTW_CRC32 \"Small.esm\")"), n(f64::from(0xCBF4_3926u32)));
    assert_eq!(q(&mut g, "array_var a\narray_var r\nlet a := TTW_GetPlugins\nlet r := a[0]\nSetFunctionValue r[0]"), s("Small.esm"));
    // TTW's gene projector is the race menu, NVForever's character creator.
    q(&mut g, "TTW_ShowGeneProjector");
    assert_eq!(g.open_menu_code(), Some(fnv_game::game::menus::RACE));
    g.close_menu();
    q(&mut g, "SetWorldspaceMapBounds TestWorld -10 20 30 -40");
    assert_eq!(g.world_map_bounds(FormId(WORLD)), Some([-10, 20, 30, -40]));
}
