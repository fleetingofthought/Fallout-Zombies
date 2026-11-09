//! The mouse pointer: the game's own arrow, `textures\interface\icons\misc\cursor.dds` (a
//! light arrow with a dark outline, drawn in green) read from the owner's archives at run
//! time and recoloured to the interface's amber by its lightness, as the system's pointer of the window wherever it
//! is shown (menus, the console, message boxes). It is the original's pointer [observed:
//! the owner's screenshots of the original's pause and main menus show a filled amber
//! arrow with a dark edge, as this texture coloured amber; the owner's pick of the
//! interface's pointer textures, C:\fnv-private\mainmenu\cursor, todo 5 item 19]. The
//! first pointer, `glow_cursor.dds` (a soft glow, the map menu's cursor), was not it.
//! Without the game's files the system's own pointer stays.
//!
//! The arrow is cut to the part of the picture that shows (alpha above
//! [`CUT_ALPHA`]); its tip, the first solid texel from the top, is the point that clicks.

use crate::interface::AMBER;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorIcon, CustomCursor, CustomCursorImage, PrimaryWindow};
use nvf_assets::GameData;

const POINTER: &str = "textures\\interface\\icons\\misc\\cursor.dds";
/// Texels fainter than this are cut away round the arrow.
const CUT_ALPHA: u8 = 8;
/// The tip is the first texel at least this solid.
const TIP_ALPHA: u8 = 200;

pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (set_pointer.run_if(not(resource_exists::<PointerSet>)), apply_traits));
    }
}

#[derive(Resource)]
struct PointerSet;

/// The arrow cut out and coloured.
#[derive(Debug, PartialEq)]
pub struct Arrow {
    /// RGBA8, row-major.
    pub pixels: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// The tip: the point that clicks.
    pub tip: (u16, u16),
}

pub fn arrow(rgba: &[u8], w: usize, h: usize, tint: [f32; 3]) -> Option<Arrow> {
    let a = |x: usize, y: usize| rgba[(y * w + x) * 4 + 3];
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if a(x, y) > CUT_ALPHA {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    if x0 > x1 {
        return None;
    }
    let tip = (y0..=y1).find_map(|y| (x0..=x1).find(|&x| a(x, y) >= TIP_ALPHA).map(|x| (x, y)))?;
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    // The texture is drawn green (a mint fill, a dark green edge) [observed: cursor.dds
    // in Fallout - Textures2.bsa]; multiplying its channels by amber left it olive. Only
    // its lightness is kept, scaled so the brightest texel is the tint itself.
    let luma = |x: usize, y: usize| {
        let p = &rgba[(y * w + x) * 4..][..3];
        0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32
    };
    let peak = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).filter(|&(x, y)| a(x, y) > CUT_ALPHA).map(|(x, y)| luma(x, y)).fold(1.0f32, f32::max);
    let mut out = Vec::with_capacity(cw * ch * 4);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let l = luma(x, y) / peak * 255.0;
            out.extend_from_slice(&[(l * tint[0]).min(255.0) as u8, (l * tint[1]).min(255.0) as u8, (l * tint[2]).min(255.0) as u8, rgba[(y * w + x) * 4 + 3]]);
        }
    }
    Some(Arrow { pixels: out, width: cw, height: ch, tip: ((tip.0 - x0) as u16, (tip.1 - y0) as u16) })
}

fn set_pointer(mut commands: Commands, data: Option<Res<GameData>>, window: Query<Entity, With<PrimaryWindow>>, mut images: ResMut<Assets<Image>>) {
    let Ok(window) = window.single() else { return };
    commands.insert_resource(PointerSet);
    let Some(data) = data else { return };
    let decoded = data.read(POINTER).ok_or_else(|| "not in the game's files".to_string()).and_then(|bytes| {
        let dds = fnv_dds::Dds::parse(&bytes).map_err(|e| e.to_string())?;
        let (w, h) = (dds.header.width as usize, dds.header.height as usize);
        let rgba = fnv_dds::decode_blocks(dds.format, w as u32, h as u32, dds.levels[0][0]).or_else(|| fnv_dds::to_rgba8(dds.format, dds.levels[0][0])).ok_or("its format is not decoded")?;
        arrow(&rgba, w, h, AMBER).ok_or_else(|| "no arrow in it".to_string())
    });
    match decoded {
        Ok(Arrow { pixels: px, width: w, height: h, tip: hotspot }) => {
            commands.insert_resource(BaseArrow(Arrow { pixels: px.clone(), width: w, height: h, tip: hotspot }));
            let image = Image::new(Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD);
            let handle = images.add(image);
            commands.entity(window).insert(CursorIcon::Custom(CustomCursor::Image(CustomCursorImage { handle, texture_atlas: None, flip_x: false, flip_y: false, rect: None, hotspot })));
            info!("pointer: the game's arrow {POINTER} ({w}x{h}, tip at {hotspot:?})");
        }
        Err(e) => warn!("pointer: {POINTER}: {e}; the system's pointer stays"),
    }
}

/// The arrow as cut from the game's file, kept for the cursor's traits.
#[derive(Resource)]
struct BaseArrow(Arrow);

/// The cursor's traits from the scripts (`fnv_game::nvse_sup6`, SetCursorTrait): the
/// arrow redrawn when they change, scaled to width × height, its colours times
/// red/green/blue over 255 and its alpha times alpha over 255 [hypothesis: the pages
/// give no scale]. The arrow's own size goes to the game for the traits at rest.
fn apply_traits(
    mut commands: Commands,
    base: Option<Res<BaseArrow>>,
    mut script: Option<ResMut<crate::script::ScriptState>>,
    window: Query<Entity, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut last: Local<Option<[f64; 6]>>,
) {
    let (Some(base), Some(rt)) = (base, script.as_deref_mut().and_then(|s| s.rt.as_mut())) else { return };
    let a = &base.0;
    rt.game.sup.cursor_size = (a.width as f64, a.height as f64);
    let v = rt.game.sup.cursor_values(rt.game.time);
    if last.is_none_or(|l| l == v) {
        // The first frame takes the arrow as it is drawn.
        if last.is_none() {
            *last = Some(v);
        }
        return;
    }
    *last = Some(v);
    let Ok(window) = window.single() else { return };
    let (w, h) = ((v[1].round().max(1.0) as usize).min(256), (v[2].round().max(1.0) as usize).min(256));
    let k = |x: f64| (x / 255.0).clamp(0.0, 1.0) as f32;
    let (ka, kr, kg, kb) = (k(v[0]), k(v[3]), k(v[4]), k(v[5]));
    let px = traited(&a.pixels, a.width, a.height, w, h, [kr, kg, kb, ka]);
    let image = Image::new(Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD);
    let handle = images.add(image);
    let hotspot = (((a.tip.0 as usize * w) / a.width.max(1)) as u16, ((a.tip.1 as usize * h) / a.height.max(1)) as u16);
    commands.entity(window).insert(CursorIcon::Custom(CustomCursor::Image(CustomCursorImage { handle, texture_atlas: None, flip_x: false, flip_y: false, rect: None, hotspot })));
    info!("pointer: the script's traits, {w}x{h}, alpha {:.0}, colour {:.0} {:.0} {:.0}", v[0], v[3], v[4], v[5]);
}

/// The arrow `pixels` (`w`×`h`) scaled to `nw`×`nh` (nearest texel) and each channel
/// multiplied by `k` (red, green, blue, alpha).
pub fn traited(pixels: &[u8], w: usize, h: usize, nw: usize, nh: usize, k: [f32; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nw * nh * 4);
    for y in 0..nh {
        let sy = (y * h / nh).min(h.saturating_sub(1));
        for x in 0..nw {
            let sx = (x * w / nw).min(w.saturating_sub(1));
            let o = (sy * w + sx) * 4;
            for (c, kc) in k.iter().enumerate() {
                out.push((f32::from(pixels.get(o + c).copied().unwrap_or(0)) * kc).round() as u8);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arrow_is_cut_out_tinted_and_clicks_at_its_tip() {
        // 4×4: a faint glow texel at (0,0), the arrow's solid tip at (2,1), a body below.
        let mut px = vec![0u8; 4 * 4 * 4];
        let mut set = |x: usize, y: usize, a: u8| px[(y * 4 + x) * 4..][..4].copy_from_slice(&[255, 255, 255, a]);
        set(1, 0, 20);
        set(2, 1, 255);
        set(2, 2, 255);
        set(3, 3, 255);
        let Arrow { pixels: out, width: w, height: h, tip } = arrow(&px, 4, 4, [1.0, 0.5, 0.25]).unwrap();
        assert_eq!((w, h), (3, 4));
        assert_eq!(tip, (1, 1));
        // Tinted: the white tip (row 1, column 1 of the cut) becomes the tint.
        assert_eq!(&out[(3 + 1) * 4..][..4], &[255, 127, 63, 255]);
        assert!(arrow(&[0u8; 16], 2, 2, [1.0; 3]).is_none());
    }

    #[test]
    fn a_green_arrow_turns_the_tint_not_olive() {
        // The game's colours: a mint fill and a dark green edge.
        let px = [181, 247, 231, 255, 74, 134, 82, 255];
        let out = arrow(&px, 2, 1, AMBER).unwrap().pixels;
        let amber = AMBER.map(|c| (c * 255.0) as u8);
        assert_eq!(&out[..3], &amber);
        // The edge is a darker amber: the same hue, red still the strongest.
        assert!(out[4] < out[0] && out[4] > out[5] && out[5] > out[6]);
    }
}
