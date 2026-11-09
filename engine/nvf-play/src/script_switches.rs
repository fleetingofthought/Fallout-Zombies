//! The engine side of JIP batch 9's switches (`fnv_game::nvse_jip9`;
//! docs/spec-notes/jip-ui-9.md): what scripts turned on or off reaches the controls each
//! frame ([`ScriptSwitches`]: the HUD cursor, the mouse's axes, the vanity wheel, the
//! camera's collision), SetCursorPos moves the pointer, SetGameVolume the volumes and
//! SetSystemColor the HUD's and the Pip-Boy's colours; the game is told the pointer, the
//! window's size, the volumes and the actor whose health bar the HUD shows.
//!
//! Log lines: `script switches: <switch> <state>`, `script switches: volume <n> <level>`,
//! `script switches: colour <n> <r> <g> <b>`.

use crate::audio_out::{SoundCategory, SoundVolumes};
use crate::script::ScriptState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_sim::FormId;

/// What scripts set for the controls, copied from the game each frame.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ScriptSwitches {
    /// ToggleHUDCursor: the pointer shows and works the HUD; mouse look stops.
    pub hud_cursor: bool,
    /// ToggleMouseMovement: bit 0 the horizontal look, bit 1 the vertical.
    pub mouse_axes: u8,
    /// ToggleVanityWheel: the wheel zooms the third-person camera.
    pub vanity_wheel: bool,
    /// ToggleCameraCollision: the third-person camera stops in front of walls.
    pub camera_collision: bool,
}

impl Default for ScriptSwitches {
    fn default() -> Self {
        ScriptSwitches { hud_cursor: false, mouse_axes: 3, vanity_wheel: true, camera_collision: true }
    }
}

impl ScriptSwitches {
    /// The mouse's look this frame with the axes scripts turned off taken out.
    pub fn look(&self, delta: Vec2) -> Vec2 {
        Vec2::new(if self.mouse_axes & 1 != 0 { delta.x } else { 0.0 }, if self.mouse_axes & 2 != 0 { delta.y } else { 0.0 })
    }
}

pub struct ScriptSwitchesPlugin;

impl Plugin for ScriptSwitchesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptSwitches>().add_systems(Update, sync.after(crate::PlaySet::Scripts).before(crate::PlaySet::Ui));
    }
}

/// The game's volume index (GetGameVolume's 1-5) of each sound category.
fn category_index(c: SoundCategory) -> usize {
    match c {
        SoundCategory::Footstep => 1,
        SoundCategory::Voice => 2,
        SoundCategory::Effects => 3,
        SoundCategory::Music => 4,
        SoundCategory::Radio => 5,
    }
}

fn sync(
    mut script: Option<ResMut<ScriptState>>,
    mut switches: ResMut<ScriptSwitches>,
    volumes: Option<Res<SoundVolumes>>,
    target: Option<Res<crate::hud_target::HudTarget>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let j = &mut rt.game.other.jip9;
    let now = ScriptSwitches { hud_cursor: j.hud_cursor, mouse_axes: j.mouse_axes, vanity_wheel: j.vanity_wheel, camera_collision: j.camera_collision };
    if *switches != now {
        let was = *switches;
        if was.hud_cursor != now.hud_cursor {
            info!("script switches: HUD cursor {}", if now.hud_cursor { "on" } else { "off" });
        }
        if was.mouse_axes != now.mouse_axes {
            info!("script switches: mouse movement {}", now.mouse_axes);
        }
        if was.vanity_wheel != now.vanity_wheel {
            info!("script switches: vanity wheel {}", if now.vanity_wheel { "on" } else { "off" });
        }
        if was.camera_collision != now.camera_collision {
            info!("script switches: camera collision {}", if now.camera_collision { "on" } else { "off" });
        }
        *switches = now;
    }
    // The volumes: the scripts' changes first, then what is heard told back.
    if let Some(v) = volumes.as_deref() {
        for (t, level) in std::mem::take(&mut j.volume_sets) {
            info!("script switches: volume {t} {level:.0}");
            let part = level / 100.0;
            match t {
                0 => v.set_master(part),
                _ => {
                    if let Some(c) = SoundCategory::ALL.into_iter().find(|&c| category_index(c) == t) {
                        v.set(c, part);
                    }
                }
            }
        }
        j.volumes[0] = v.master() * 100.0;
        for c in SoundCategory::ALL {
            j.volumes[category_index(c)] = v.get(c) * 100.0;
        }
    }
    // The HUD's and the Pip-Boy's colours; the other three interfaces keep theirs (their
    // colours are not separate in NVForever yet, jip-ui-9.md §6).
    for (id, rgb) in std::mem::take(&mut j.colour_sets) {
        info!("script switches: colour {id} {} {} {}", rgb[0], rgb[1], rgb[2]);
        match id {
            1 => crate::interface::set_hud_rgb(rgb),
            4 => crate::interface::set_pipboy_rgb(rgb),
            _ => {}
        }
    }
    j.enemy_target = target.and_then(|t| t.0.as_ref().map(|h| FormId(h.form_id)));
    let Ok(mut window) = windows.single_mut() else { return };
    let size = (f64::from(window.width()), f64::from(window.height()));
    j.window = Some(size);
    if let Some((x, y, xml)) = j.cursor_warp.take() {
        // XML units are the interface's 960-high frame (`crate::tiles::input::to_units`).
        let per = size.1.max(1.0) / 960.0;
        let (px, py) = if xml { (x * per, y * per) } else { (x, y) };
        let at = Vec2::new(px.clamp(0.0, size.0) as f32, py.clamp(0.0, size.1) as f32);
        window.set_cursor_position(Some(at));
    }
    j.cursor_px = window.cursor_position().map(|p| (f64::from(p.x), f64::from(p.y)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_look_keeps_only_the_axes_scripts_left_on() {
        let d = Vec2::new(3.0, -2.0);
        let with = |mouse_axes| ScriptSwitches { mouse_axes, ..Default::default() }.look(d);
        assert_eq!(with(3), d);
        assert_eq!(with(1), Vec2::new(3.0, 0.0));
        assert_eq!(with(2), Vec2::new(0.0, -2.0));
        assert_eq!(with(0), Vec2::ZERO);
    }

    #[test]
    fn each_category_has_its_game_volume_index() {
        let mut seen: Vec<usize> = SoundCategory::ALL.into_iter().map(category_index).collect();
        seen.sort();
        assert_eq!(seen, [1, 2, 3, 4, 5]);
    }
}
