//! SUP's screenshots (`fnv_game::nvse_sup5`; docs/spec-notes/ext-sup5.md): the requests
//! scripts leave in `Game::ui.screenshots` are taken after the frame's scripts as a
//! picture of the window, cut to CaptureScreenshotAlt's part of the screen, encoded
//! (`fnv_game::image_files`) and written through the game's `mod_files` into NVForever's
//! own layer, never the game folder. Logged: `sup: screenshot <path> <w>x<h>`.

use crate::script::ScriptState;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use fnv_game::image_files::{self, ShotRequest};

pub struct SupShotsPlugin;

impl Plugin for SupShotsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, take.after(crate::PlaySet::Scripts));
    }
}

/// The UI's height in its own units: CaptureScreenshotAlt's coordinates without Use Game
/// Res are in them [documented: CaptureScreenshotAlt, "UI(XML) screen resolution"; the
/// tile tree's screen() is 960 high, crates/fnv-ui].
const UI_HEIGHT: f32 = 960.0;

fn take(mut commands: Commands, mut script: ResMut<ScriptState>) {
    let Some(rt) = script.rt.as_mut() else { return };
    if rt.game.ui.screenshots.is_empty() {
        return;
    }
    for req in std::mem::take(&mut rt.game.ui.screenshots) {
        commands.spawn(Screenshot::primary_window()).observe(move |ev: On<ScreenshotCaptured>, mut script: ResMut<ScriptState>| {
            let img = &ev.image;
            let bgra = match img.texture_descriptor.format {
                TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => true,
                TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => false,
                f => {
                    warn!("sup: screenshot {}: the window's picture is {f:?}; not written", req.path);
                    return;
                }
            };
            let Some(mut px) = img.data.clone() else { return };
            if bgra {
                for p in px.as_chunks_mut::<4>().0 {
                    p.swap(0, 2);
                }
            }
            let Some(rt) = script.rt.as_mut() else { return };
            write(&mut rt.game, &req, img.width(), img.height(), px);
        });
    }
}

/// Cut, encode and write one picture.
fn write(game: &mut fnv_game::game::Game<'static>, req: &ShotRequest, w: u32, h: u32, px: Vec<u8>) {
    let (w, h, px) = match req.crop {
        Some(c) => {
            let scale = if req.game_res { 1.0 } else { h as f32 / UI_HEIGHT };
            match image_files::crop_rect(w, h, c, scale) {
                Some(r) => (r.2, r.3, image_files::crop(&px, w, r)),
                None => {
                    warn!("sup: screenshot {}: the part asked for is outside the screen; not written", req.path);
                    return;
                }
            }
        }
        None => (w, h, px),
    };
    let Some(bytes) = image_files::encode(req.format, w, h, &px) else { return };
    let ok = if req.root { game.mod_files.write_root_bytes(&req.path, &bytes) } else { game.mod_files.write_bytes(&req.path, &bytes) };
    if ok {
        info!("sup: screenshot {} {w}x{h}", req.path);
    } else {
        warn!("sup: screenshot {}: not written", req.path);
    }
}
