//! JIP LN NVSE, batch 9 (#1006): the HUD cursor, the mouse and the camera's toggles, the
//! active menu, the hotkeys' inventory references, the Pip-Boy radio, the game volumes,
//! fonts and interface colours, the enemy health bar's actor, the player's immortal mode,
//! interaction by form type, GetPosEx [documented: each page in the wiki export;
//! docs/spec-notes/jip-ui-9.md]. The NIF block ones of the batch (the material values, the
//! translation and rotation into script variables) are in `crate::nif_blocks`.
//!
//! The engine reads the toggles every frame and tells back what only it knows (the
//! cursor, the volumes, the health bar's actor): `nvf_play::script_switches`.

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

/// The volumes a new player starts with, 0-100, in GetGameVolume's order (master,
/// footsteps, voice, effects, music, radio) [observed: the original's GetGameVolume on an
/// install left at its defaults, 60 50 60 100 50 for music, footsteps, voice, effects and
/// radio (engine `audio_out::DEFAULT_VOLUMES`); master 100]. SetGameVolume without a level
/// goes back to these [hypothesis: the page's "the value defined in the game's INI file",
/// Fallout_default.ini's fDefault…Volume, which these are].
pub const DEFAULT_VOLUMES: [f32; 6] = [100.0, 50.0, 60.0, 100.0, 60.0, 50.0];

/// What this batch keeps for the session; none of it is saved [hypothesis: the pages
/// say nothing of saves; the immortal mode and the disabled types are a mod's to set
/// again on load, as JIP's other session switches].
#[derive(Debug)]
pub struct Jip9State {
    /// ToggleHUDCursor: the cursor works the HUD; off when any menu opens [documented].
    pub hud_cursor: bool,
    /// ToggleMouseMovement: bit 0 horizontal, bit 1 vertical [documented: 0-3].
    pub mouse_axes: u8,
    /// ToggleVanityWheel: the wheel moves the third-person camera.
    pub vanity_wheel: bool,
    /// ToggleCameraCollision: the third-person camera stops in front of walls.
    pub camera_collision: bool,
    /// SetCursorPos for the engine: (x, y, in XML coordinates).
    pub cursor_warp: Option<(f64, f64, bool)>,
    /// The cursor in the window's pixels from its top left, as the engine last told it.
    pub cursor_px: Option<(f64, f64)>,
    /// The window's size in pixels, as the engine last told it; `None` until it does
    /// (a headless run: the INI's display size, [`window`]).
    pub window: Option<(f64, f64)>,
    /// The game volumes 0-100 in GetGameVolume's order, as the engine last told them
    /// (SetGameVolume's change at once).
    pub volumes: [f32; 6],
    /// SetGameVolume's changes for the engine.
    pub volume_sets: Vec<(usize, f32)>,
    /// SetFontFile's files, by font type (`Game::font_file`).
    pub fonts: BTreeMap<i64, String>,
    /// SetSystemColor's colours, by interface 1-5.
    pub colours: BTreeMap<u8, [u8; 3]>,
    /// SetSystemColor's changes for the engine.
    pub colour_sets: Vec<(u8, [u8; 3])>,
    /// The actor whose health bar the HUD shows, as the engine last told it.
    pub enemy_target: Option<FormId>,
    /// ToggleImmortalMode's value as set, any number [observed: jip9 immortal.5 reads 5
    /// back]; 1 health not below 1, 2 no health or limb damage, others off.
    pub immortal: i64,
    /// SetInteractionDisabledType's form type codes.
    pub disabled_types: BTreeSet<i32>,
}

impl Default for Jip9State {
    fn default() -> Self {
        Jip9State {
            hud_cursor: false,
            mouse_axes: 3,
            vanity_wheel: true,
            camera_collision: true,
            cursor_warp: None,
            cursor_px: None,
            window: None,
            volumes: DEFAULT_VOLUMES,
            volume_sets: Vec::new(),
            fonts: BTreeMap::new(),
            colours: BTreeMap::new(),
            colour_sets: Vec::new(),
            enemy_target: None,
            immortal: 0,
            disabled_types: BTreeSet::new(),
        }
    }
}

fn num(args: &[XVal], k: usize) -> Option<f64> {
    match args.get(k) {
        Some(XVal::Num(x)) if x.is_finite() => Some(*x),
        _ => None,
    }
}

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k) {
        Some(XVal::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

/// A toggle's argument: `Some(on)` when given.
fn toggle(args: &[XVal]) -> Option<bool> {
    num(args, 0).map(|x| x != 0.0)
}

/// The axis an argument names: `X` 0, `Y` 1.
fn axis(v: Option<&XVal>) -> Option<usize> {
    match v? {
        XVal::Str(s) if s.eq_ignore_ascii_case("x") => Some(0),
        XVal::Str(s) if s.eq_ignore_ascii_case("y") => Some(1),
        _ => None,
    }
}

/// The window's size in pixels: as the engine told it, else the game INI's display size
/// (`iSize W` and `iSize H` under `[Display]`, FalloutPrefs.ini) when the host gave INI
/// text, else 1280 by 720 [observed: jip9 cursor.*, the original's pointer sat at 640 360
/// pixels, 853.333 480 units, on an install whose FalloutPrefs.ini says iSize W=1280,
/// iSize H=720; decision: the same size when no INI names one].
pub fn window(g: &mut Game) -> (f64, f64) {
    if let Some(w) = g.other.jip9.window {
        return w;
    }
    let mut ini = |k: &str| g.ini_number(k).filter(|v| *v > 0.0);
    match (ini("isize w:display"), ini("isize h:display")) {
        (Some(w), Some(h)) => (w, h),
        _ => (1280.0, 720.0),
    }
}

/// Window pixels per interface (XML) unit: the interface is 960 units high at any window
/// size [documented: menu-xml.md §3, the engine's `tiles::input::to_units`].
fn px_per_unit(g: &mut Game) -> f64 {
    window(g).1.max(1.0) / 960.0
}

impl Game<'_> {
    /// SetFontFile's file for a font type, if a script set one (GetFontFile reads it).
    pub fn font_file(&self, font: i64) -> Option<&str> {
        self.other.jip9.fonts.get(&font).map(String::as_str)
    }

    /// The player may not activate `target` by hand, nor see its rollover text: its
    /// form type is one SetInteractionDisabledType disabled, or SetInteractionDisabled
    /// disabled the reference [documented: both pages; scripted Activate still works].
    pub fn player_interaction_disabled(&self, target: FormId) -> bool {
        if self.gaps.interaction_disabled.contains(&target) {
            return true;
        }
        let types = &self.other.jip9.disabled_types;
        !types.is_empty() && crate::functions::base_of(self, target).and_then(|b| self.data.sig(b)).and_then(|s| type_code(&s)).is_some_and(|c| types.contains(&c))
    }

    /// The player's immortal mode (ToggleImmortalMode): 0 off, 1 Health not below 1,
    /// 2 no Health or limb damage.
    pub fn immortal_mode(&self) -> u8 {
        // Another value is kept and read back [observed: jip9 immortal.5]; it holds
        // nothing [hypothesis: the page names 0-2 only].
        match self.other.jip9.immortal {
            m @ (1 | 2) => m as u8,
            _ => 0,
        }
    }

    /// A menu opened: the HUD cursor goes off [documented: ToggleHUDCursor, "switched off
    /// upon opening any game menu"].
    pub(crate) fn hud_cursor_menu_opened(&mut self) {
        self.other.jip9.hud_cursor = false;
    }
}

/// The form type code of a base form's record type, for SetInteractionDisabledType: the
/// numbering GetIsUsedItemType and RemoveAllTypedItems take, with NPC_ 42 and CREA 43
/// [hypothesis: the same numbering continued, as `functions::furniture` has it].
pub fn type_code(sig: &[u8; 4]) -> Option<i32> {
    match sig {
        b"NPC_" => Some(42),
        b"CREA" => Some(43),
        _ => crate::functions::furniture::used_type_code(sig),
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- the cursor, the mouse, the camera
        // `[toggle]`: the state before it, so without an argument the state [observed:
        // jip9 hudcursor, 0 from `ToggleHUDCursor 1` with it off, 1 asked after; the
        // page's "return whether cursor mode is active"]. The switches below answer the
        // same way [observed: jip9 mouse.*, vanity, camcoll, immortal.*]. Not while a
        // menu is open [hypothesis: a menu switches it off].
        "togglehudcursor" => {
            let was = g.other.jip9.hud_cursor;
            if let Some(on) = toggle(args) {
                g.other.jip9.hud_cursor = on && g.menu.is_none();
            }
            yes(was)
        }
        // `axis [useXML]`. Screen coordinates are the window's pixels from its top left
        // [documented: "(0, 0) is at the top-left corner"]; XML ones the interface's
        // units, the pointer the tile tree hits with. Outside MenuMode and the HUD cursor
        // it is the system cursor's position over the window [observed: jip9 reruns gave
        // 640 / 360 and 1278.5 / 431.94 on a 1280 by 720 window, where the mouse was];
        // with no cursor known (headless), the window's centre.
        "getcursorpos" => {
            let (Some(a), xml) = (axis(args.first()), num(args, 1).is_some_and(|x| x != 0.0)) else { return Some(Ok(zero)) };
            let per = px_per_unit(g);
            if g.menu.is_none() && !g.other.jip9.hud_cursor {
                let (w, h) = window(g);
                let (x, y) = g.other.jip9.cursor_px.unwrap_or((w / 2.0, h / 2.0));
                let c = if a == 0 { x } else { y };
                return Some(Ok(XVal::Num(if xml { c / per } else { c })));
            }
            let at = if xml {
                g.ui_pointer.at.or_else(|| g.other.jip9.cursor_px.map(|(x, y)| (x / per, y / per)))
            } else {
                g.other.jip9.cursor_px.or_else(|| g.ui_pointer.at.map(|(x, y)| (x * per, y * per)))
            };
            XVal::Num(at.map_or(0.0, |p| if a == 0 { p.0 } else { p.1 }))
        }
        // `x y [useXML]`: for the engine to move the pointer; only in MenuMode or with the
        // HUD cursor [documented].
        "setcursorpos" => {
            if let (Some(x), Some(y)) = (num(args, 0), num(args, 1)) {
                if g.menu.is_some() || g.other.jip9.hud_cursor {
                    let xml = num(args, 2).is_some_and(|v| v != 0.0);
                    g.other.jip9.cursor_warp = Some((x, y, xml));
                    let per = px_per_unit(g);
                    g.other.jip9.cursor_px = Some(if xml { (x * per, y * per) } else { (x, y) });
                }
            }
            zero
        }
        // `[flags]`: 0 both off, 1 horizontal only, 2 vertical only, 3 both [documented];
        // the two low bits are kept [observed: jip9 mouse.9, 9 reads back 1].
        "togglemousemovement" => {
            let was = g.other.jip9.mouse_axes;
            if let Some(f) = num(args, 0) {
                g.other.jip9.mouse_axes = ((f as i64) & 3) as u8;
            }
            XVal::Num(f64::from(was))
        }
        "togglevanitywheel" => {
            let was = g.other.jip9.vanity_wheel;
            if let Some(on) = toggle(args) {
                g.other.jip9.vanity_wheel = on;
            }
            yes(was)
        }
        "togglecameracollision" => {
            let was = g.other.jip9.camera_collision;
            if let Some(on) = toggle(args) {
                g.other.jip9.camera_collision = on;
            }
            yes(was)
        }
        // `[closeAll]`: the open menu closes as the player's OK does, or every one
        // [documented]. Nothing is returned [documented: no return type].
        "closeactivemenu" => {
            if num(args, 0).is_some_and(|x| x != 0.0) {
                for _ in 0..16 {
                    if g.close_menu().is_none() {
                        break;
                    }
                }
            } else {
                g.close_menu();
            }
            zero
        }
        // ---- hotkeys: the same eight as GetHotkeyItem's
        // `hotkey`: an inventory reference of the item in the player's inventory, 0 when
        // the key holds none or the player no longer carries it [documented: "an Inventory
        // Reference of the item"; hypothesis: not carried].
        "gethotkeyitemref" => {
            let Some(k) = num(args, 0).and_then(|k| usize::try_from(k as i64 - 1).ok()).filter(|&k| k < 8) else { return Some(Ok(XVal::Ref(0))) };
            let Some(item) = g.other.hotkeys[k] else { return Some(Ok(XVal::Ref(0))) };
            let carried = g.world.inventory_mut(FormId::PLAYER_REF).is_ok_and(|inv| inv.count(item) > 0);
            XVal::Ref(if carried { g.temp_ref(FormId::PLAYER_REF, item).0 } else { 0 })
        }
        // `invRef.SetHotkeyItemRef key`: keys 1 and 3-8 [documented: the page's values;
        // key 2 is Ammo Swap]; an inventory reference of the player's [hypothesis: the
        // hotkeys are the player's]. 1 when set.
        "sethotkeyitemref" => {
            let k = num(args, 0).map(|k| k as i64);
            let Some(k) = k.filter(|&k| k == 1 || (3..=8).contains(&k)) else { return Some(Ok(zero)) };
            let Some((holder, item)) = subject.and_then(|r| g.temp_item(r)) else { return Some(Ok(zero)) };
            if holder != FormId::PLAYER_REF {
                return Some(Ok(zero));
            }
            g.other.hotkeys[(k - 1) as usize] = Some(item);
            XVal::Num(1.0)
        }
        // ---- the Pip-Boy radio: the station's reference while it plays, else 0
        // [documented].
        "getpipboyradio" => {
            let p = &g.radio.pipboy;
            XVal::Ref(if p.on { p.station.map_or(0, |s| s.0) } else { 0 })
        }
        // ---- volumes: `type` 0-5, 0-100 [documented]
        "getgamevolume" => {
            let Some(t) = num(args, 0).map(|t| t as i64).and_then(|t| usize::try_from(t).ok()).filter(|&t| t < 6) else { return Some(Ok(zero)) };
            XVal::Num(f64::from(g.other.jip9.volumes[t]))
        }
        // `type [level]`: a level past 100 changes nothing [observed: jip9 volume.set,
        // 30 then 150 reads 30], nor one below 0 [hypothesis: likewise]; left out, the
        // default [observed: music back to 60].
        "setgamevolume" => {
            if let Some(t) = num(args, 0).map(|t| t as i64).and_then(|t| usize::try_from(t).ok()).filter(|&t| t < 6) {
                let v = match num(args, 1) {
                    None => Some(DEFAULT_VOLUMES[t]),
                    Some(v) => (0.0..=100.0).contains(&v).then_some(v as f32),
                };
                if let Some(v) = v {
                    g.other.jip9.volumes[t] = v;
                    g.other.jip9.volume_sets.push((t, v));
                }
            }
            zero
        }
        // ---- fonts and colours
        // `type path`: types 1-8 and 10-89, a file under Data [documented]; 1 when the
        // file is there [hypothesis: the page's "on success"]. GetFontFile reads it.
        "setfontfile" => {
            let (Some(t), Some(path)) = (num(args, 0).map(|t| t as i64), text(args, 1)) else { return Some(Ok(zero)) };
            if !((1..=8).contains(&t) || (10..=89).contains(&t)) || path.is_empty() {
                return Some(Ok(zero));
            }
            if crate::mod_files::read_data(&path).is_none() {
                return Some(Ok(zero));
            }
            g.other.jip9.fonts.insert(t, path);
            XVal::Num(1.0)
        }
        // `interface r g b`: 1 HUD, 2 HUD alt, 3 terminal, 4 Pip-Boy, 5 main menu
        // [documented]; each part rounded into 0-255 [hypothesis].
        "setsystemcolor" => {
            let Some(id) = num(args, 0).map(|t| t as i64).filter(|t| (1..=5).contains(t)) else { return Some(Ok(zero)) };
            let part = |k: usize| num(args, k).unwrap_or(0.0).round().clamp(0.0, 255.0) as u8;
            let rgb = [part(1), part(2), part(3)];
            g.other.jip9.colours.insert(id as u8, rgb);
            g.other.jip9.colour_sets.push((id as u8, rgb));
            zero
        }
        // ---- the HUD's enemy health bar: its actor, else none [documented].
        "getenemyhealthtarget" => XVal::Ref(g.other.jip9.enemy_target.map_or(0, |f| f.0)),
        // ---- the player's immortal mode: `[mode]` 0-2 [documented]; any value is kept
        // and read back [observed: jip9 immortal.5]; the mode before it.
        "toggleimmortalmode" => {
            let was = g.other.jip9.immortal;
            if let Some(m) = num(args, 0) {
                g.other.jip9.immortal = m as i64;
            }
            XVal::Num(was as f64)
        }
        // ---- interaction by form type: `disable type` / `type` [documented]
        "setinteractiondisabledtype" => {
            if let (Some(on), Some(t)) = (num(args, 0), num(args, 1)) {
                if on != 0.0 {
                    g.other.jip9.disabled_types.insert(t as i32);
                } else {
                    g.other.jip9.disabled_types.remove(&(t as i32));
                }
            }
            zero
        }
        "getinteractiondisabledtype" => yes(num(args, 0).is_some_and(|t| g.other.jip9.disabled_types.contains(&(t as i32)))),
        // ---- `ref.GetPosEx x y z`: the position into three script variables
        // [documented]; no result [documented: no return type].
        "getposex" => {
            let Some((p, _)) = subject.and_then(|r| g.ref_pos_rot(r)) else { return Some(Ok(zero)) };
            let entries = [0.0, f64::from(p[0]), f64::from(p[1]), f64::from(p[2])].into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), XVal::Num(v))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        _ => return None,
    };
    Some(Ok(v))
}
