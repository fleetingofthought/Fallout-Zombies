//! The extender's input functions' side in the engine (task D6;
//! docs/spec-notes/ext-input.md): each frame, right after Bevy reads the devices, the
//! keys and mouse buttons held and the controls' bindings go to the script runtime
//! (`fnv_game::script_input`), the keys scripts disabled go to [`ScriptBlocked`] (which
//! `bindings::ControlInput` leaves out, so the player's controls ignore them), and the
//! keys scripts tapped are pressed for one frame.

use crate::bindings::{dik_key, Binding, Bindings, GAME_CONTROLS, GAME_INI_CONTROLS};
use bevy::prelude::*;
use fnv_game::script_input::MOUSE_BASE;

pub struct ScriptInputPlugin;

impl Plugin for ScriptInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptBlocked>()
            .add_systems(PreUpdate, script_input.after(bevy::input::InputSystems))
            .add_systems(Update, (tell_magazine, tell_player_state, mod_file_roots));
    }
}

/// The keys and mouse buttons scripts disabled (DisableKey, DisableControl).
#[derive(Resource, Default)]
pub struct ScriptBlocked {
    pub keys: Vec<KeyCode>,
    pub mouse: Vec<MouseButton>,
}

/// The extender's code of a mouse button: 256 left, 257 right, 258 middle, then the extra
/// buttons [documented: Template:DirectX Scancodes].
fn mouse_code(b: MouseButton) -> Option<u16> {
    Some(MOUSE_BASE
        + match b {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
            MouseButton::Other(n) if n < 8 => n,
            MouseButton::Other(_) => return None,
        })
}

fn mouse_of(code: u16) -> Option<MouseButton> {
    [MouseButton::Left, MouseButton::Right, MouseButton::Middle, MouseButton::Back, MouseButton::Forward, MouseButton::Other(5), MouseButton::Other(6), MouseButton::Other(7)]
        .into_iter()
        .find(|&b| mouse_code(b) == Some(code))
}

fn key_code(k: KeyCode) -> Option<u16> {
    (0u8..=255).find(|&c| dik_key(c) == Some(k)).map(u16::from)
}

/// Let go of a key or mouse button the scripts pressed.
fn release(c: u16, keys: &mut ButtonInput<KeyCode>, mouse: &mut ButtonInput<MouseButton>) {
    match c.checked_sub(MOUSE_BASE) {
        Some(_) => mouse_of(c).into_iter().for_each(|b| mouse.release(b)),
        None => dik_key(c as u8).into_iter().for_each(|k| keys.release(k)),
    }
}

fn script_input(
    mut script: Option<ResMut<crate::script::ScriptState>>,
    mut bindings: Option<ResMut<Bindings>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut blocked: ResMut<ScriptBlocked>,
    mut tapped: Local<Vec<u16>>,
    mut script_held: Local<Vec<u16>>,
) {
    // Last frame's taps end.
    for c in tapped.drain(..) {
        release(c, &mut keys, &mut mouse);
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let input = &mut rt.game.input;
    // Keys scripts held last frame and no longer hold (ReleaseKey, ReleaseControl) are let go.
    for c in script_held.drain(..).filter(|c| !input.script_held.contains(c)) {
        release(c, &mut keys, &mut mouse);
    }
    // SetControl's changes reach the player's controls first.
    if let Some(b) = bindings.as_deref_mut() {
        for (c, mouse, code) in std::mem::take(&mut input.rebinds) {
            let Some(&(control, _, _)) = GAME_CONTROLS.get(c) else { continue };
            let old = b.get(control);
            let new = if mouse {
                Binding { mouse: code.and_then(mouse_of), ..old }
            } else {
                Binding { key: code.and_then(|k| u8::try_from(k).ok()).and_then(dik_key), ..old }
            };
            b.set(control, new);
        }
    }
    input.held = keys.get_pressed().filter_map(|&k| key_code(k)).chain(mouse.get_pressed().filter_map(|&b| mouse_code(b))).collect();
    if let Some(b) = bindings.as_deref() {
        // The engine's controls are the extender's 0–27, in the same order.
        input.bindings = GAME_CONTROLS[..GAME_INI_CONTROLS].iter().map(|(c, _, _)| {
            let bind = b.get(*c);
            (bind.key.and_then(key_code), bind.mouse.and_then(mouse_code))
        }).collect();
    }
    let codes = input.blocked();
    blocked.keys = codes.iter().filter(|&&c| c < MOUSE_BASE).filter_map(|&c| dik_key(c as u8)).collect();
    blocked.mouse = codes.iter().filter_map(|&c| mouse_of(c)).collect();
    for c in std::mem::take(&mut input.taps).into_iter().chain(input.script_held.iter().copied()) {
        match c.checked_sub(MOUSE_BASE) {
            Some(_) => mouse_of(c).into_iter().for_each(|b| mouse.press(b)),
            None => dik_key(c as u8).into_iter().for_each(|k| keys.press(k)),
        }
        if !input.script_held.contains(&c) {
            tapped.push(c);
        }
    }
    script_held.extend(input.script_held.iter().copied());
}

/// The rounds in the player's magazine for GetPlayerCurrentAmmoRounds.
fn tell_magazine(mut combat: Option<ResMut<crate::combat::Combat>>, mut script: Option<ResMut<crate::script::ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    // SetPlayerCurrentAmmoRounds: the rounds go in the weapon in hand.
    if let (Some(n), Some(c)) = (rt.game.other.set_magazine.take(), combat.as_deref_mut()) {
        if let Some((w, _)) = c.magazine_for_save() {
            c.restore_magazine((w, n), None);
        }
    }
    rt.game.player_magazine = combat.and_then(|c| c.magazine_for_save()).map(|m| m.1);
}

/// The pad's buttons as the extender's controller functions number them
/// (`fnv_game::nvse_jip2::Pad`).
const PAD_BITS: [(GamepadButton, u32); 14] = [
    (GamepadButton::DPadUp, 0x1),
    (GamepadButton::DPadDown, 0x2),
    (GamepadButton::DPadLeft, 0x4),
    (GamepadButton::DPadRight, 0x8),
    (GamepadButton::Start, 0x10),
    (GamepadButton::Select, 0x20),
    (GamepadButton::LeftThumb, 0x40),
    (GamepadButton::RightThumb, 0x80),
    (GamepadButton::LeftTrigger, 0x100),
    (GamepadButton::RightTrigger, 0x200),
    (GamepadButton::South, 0x1000),
    (GamepadButton::East, 0x2000),
    (GamepadButton::West, 0x4000),
    (GamepadButton::North, 0x8000),
];

/// Tell the scripts the player's auto-move and aiming and the first connected pad
/// (GetAutoMove, GetPCUsingScope/IronSights, the controller functions).
fn tell_player_state(control: Option<Res<crate::control::PlayerControl>>, pads: Query<&Gamepad>, mut script: Option<ResMut<crate::script::ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let told = &mut rt.game.jip2.told;
    if let Some(c) = control {
        told.auto_move = c.auto_move;
        told.aiming = c.aiming && c.weapon_out;
    }
    let pad = &mut told.pad;
    match pads.iter().next() {
        Some(g) => {
            pad.connected = true;
            pad.buttons = PAD_BITS.iter().filter(|(b, _)| g.pressed(*b)).map(|(_, m)| m).sum();
            pad.triggers = [g.get(GamepadButton::LeftTrigger2).unwrap_or(0.0), g.get(GamepadButton::RightTrigger2).unwrap_or(0.0)];
            let (l, r) = (g.left_stick(), g.right_stick());
            pad.sticks = [l.x, l.y, r.x, r.y];
        }
        None => {
            pad.connected = false;
            pad.buttons = 0;
            pad.triggers = [0.0; 2];
            pad.sticks = [0.0; 4];
        }
    }
}

/// Once the game's Data is open: mods' files are read from NVForever's mods layer, then
/// the game's Data, and the archives answer FileExists (fnv_game::mod_files).
fn mod_file_roots(data: Option<Res<nvf_assets::GameData>>, mut done: Local<bool>) {
    let Some(d) = data.filter(|_| !*done) else { return };
    *done = true;
    let vfs = d.vfs.clone();
    let reader_vfs = d.vfs.clone();
    let mods = fnv_install::EngineHome::default_root().map(|r| fnv_install::EngineHome::new(r).mods());
    let data_dir = vfs.data_dir().to_path_buf();
    let archives: Vec<String> = vfs.archives().map(str::to_string).collect();
    fnv_game::mod_files::update_roots(move |r| {
        r.read = mods.into_iter().chain(std::iter::once(data_dir)).collect();
        r.archives = archives;
        r.reader = Some(std::sync::Arc::new(move |p: &str| reader_vfs.read(p).and_then(Result::ok)));
        r.lookup = Some(std::sync::Arc::new(move |p: &str| {
            if vfs.loose_path(p).is_some() {
                1
            } else if vfs.exists(p) {
                2
            } else {
                0
            }
        }));
    });
}
