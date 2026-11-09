//! What the character-creation menus are drawn with: the game's two menu fonts, the
//! interface's pictures and the pieces the menu files build from (lines, lines fading
//! at the corners, the highlight box, text set left, right or centred). Lengths are in
//! the interface's units (960 to the view's height), from the parent node's top left.

use crate::interface::{amber, at, face, plain_font, set_line, Face, Picture, Pictures};
use bevy::prelude::*;
use fnv_font::{Font, Line};
use nvf_assets::GameData;

/// Font 1 (glowing), font 2, the body text, and font 6, the titles [documented:
/// Fallout_default.ini, docs/spec-notes/fonts.md].
const GLOW_FONT: &str = "textures\\fonts\\glow_monofonto_large.fnt";
const BODY_FONT: &str = "textures\\fonts\\monofonto_large.fnt";
const TITLE_FONT: &str = "textures\\fonts\\glow_monofonto_vl_dialogs.fnt";

/// Which of the menus' fonts a text is set in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Glow,
    Body,
    Title,
}

/// Every line is 2 thick [documented: menus\globals.xml, on a PC].
pub const THICK: f32 = 2.0;
/// The short lines at a bracket's corners [documented: bottom_bracket.xml, the
/// headline's vertical fade lines; the 40 is message-menu.md §2's].
pub const CORNER: f32 = 40.0;
/// How far under a text's top its letters stand: the body font and the title font
/// (message-menu.md §3).
pub const BODY_BASE: f32 = 15.0;
pub const TITLE_BASE: f32 = 21.5;
/// A body text's line height (the font's 31) and the pitch of a wrapped text's lines
/// (message-menu.md §3).
pub const BODY_H: f32 = 31.0;
pub const LINE_PITCH: f32 = 35.0;

/// The fonts and pictures, read when the first menu opens.
pub struct Kit {
    pub glow: Face,
    pub body: Face,
    pub title: Face,
    pub pictures: Pictures,
}

impl Kit {
    pub fn load(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Kit {
        Kit {
            glow: face(GLOW_FONT, plain_font(31.0, 12.0), data, images, bc),
            body: face(BODY_FONT, plain_font(31.0, 12.0), data, images, bc),
            title: face(TITLE_FONT, plain_font(34.0, 14.0), data, images, bc),
            pictures: Pictures::load(data),
        }
    }
}

/// Where a text's letters start for its anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Right,
    Centre,
}

/// The first line of a text in a font (all of it when it has no line breaks).
pub fn first_line(font: &Font, s: &str) -> Line {
    font.lines(s, None).into_iter().next().unwrap_or_default()
}

pub struct Ctx<'a> {
    pub kit: &'a mut Kit,
    pub data: Option<&'a GameData>,
    pub images: &'a mut Assets<Image>,
    pub bc: bool,
    pub glyphs: &'a crate::pad::PadGlyphs,
}

impl Ctx<'_> {
    pub fn picture(&mut self, path: &str) -> Option<Picture> {
        self.kit.pictures.picture(path, self.data, self.images, self.bc)
    }

    /// A picture tinted `color`, stretched over `node`, with `extra` components.
    pub fn image(&mut self, p: &mut ChildSpawnerCommands, path: &str, node: Node, color: Color, extra: impl Bundle) -> Option<Entity> {
        let (image, rect) = self.picture(path)?;
        Some(p.spawn((node, ImageNode { rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }, extra)).id())
    }

    /// A straight line from the atlas's solid piece (or a plain fill).
    pub fn line(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if self.image(p, "solid.dds", at(x, y, w, h), color, ()).is_none() {
            p.spawn((at(x, y, w, h), BackgroundColor(color)));
        }
    }

    /// A line `len` long that fades out downwards (or upwards, `up`), its top at `y`.
    pub fn fade(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, len: f32, up: bool, color: Color) {
        let name = if up { "fade_to_top.dds" } else { "fade_to_bottom.dds" };
        if self.image(p, name, at(x, y, THICK, len), color, ()).is_none() {
            p.spawn((at(x, y, THICK, len), BackgroundColor(color.with_alpha(0.5))));
        }
    }

    /// The bottom bracket (bottom_bracket.xml): a line `w` long at `y` with a short line
    /// fading up at each end.
    pub fn bottom_bracket(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, color: Color) {
        self.line(p, x, y, w, THICK, color);
        for xx in [x, x + w - THICK] {
            self.fade(p, xx, y - CORNER + THICK, CORNER, true, color);
        }
    }

    /// The top bracket (top_bracket.xml): a line `w` long at `y` with a line fading down
    /// `h` at each end.
    pub fn top_bracket(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.line(p, x, y, w, THICK, color);
        for xx in [x, x + w - THICK] {
            self.fade(p, xx, y, h, false, color);
        }
    }

    /// The highlight box (box.xml through list_box.xml): an outline at brightness 200
    /// and a fill of alpha 40 [documented: `_line_alpha` 255 of `_line_brightness` 200,
    /// `_fill_alpha` 40].
    pub fn highlight(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, alpha: f32) {
        let fill = amber(128.0).with_alpha((40.0 / 255.0f32).powf(2.2) * alpha);
        p.spawn((at(x, y, w, h), BackgroundColor(fill)));
        let line = amber(200.0).with_alpha(alpha);
        self.outline(p, x, y, w, h, line);
    }

    /// A rectangle's four sides.
    pub fn outline(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, color: Color) {
        for yy in [y, y + h - THICK] {
            self.line(p, x, yy, w, THICK, color);
        }
        for xx in [x, x + w - THICK] {
            self.line(p, xx, y + THICK, THICK, h - 2.0 * THICK, color);
        }
    }

    pub fn face(&self, which: Which) -> &Face {
        match which {
            Which::Glow => &self.kit.glow,
            Which::Body => &self.kit.body,
            Which::Title => &self.kit.title,
        }
    }

    /// One line of text whose top is at `top`, placed by `align` about `x`. Its width.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, p: &mut ChildSpawnerCommands, which: Which, s: &str, x: f32, top: f32, align: Align, color: Color) -> f32 {
        let face = self.face(which);
        let width = crate::pad::hint_width(face, s, self.glyphs);
        let x = match align {
            Align::Left => x,
            Align::Right => x - width,
            Align::Centre => x - width / 2.0,
        };
        crate::pad::set_hint(p, face, s, x, top + if which == Which::Title { TITLE_BASE } else { BODY_BASE }, color, self.glyphs);
        width
    }

    /// A body text wrapped at `wrap` (and at its own line breaks), its first line's top at
    /// `top`. The number of lines.
    pub fn wrapped(&mut self, p: &mut ChildSpawnerCommands, s: &str, x: f32, top: f32, wrap: f32, color: Color) -> usize {
        let lines = self.kit.body.font.lines(s, Some(wrap));
        for (k, line) in lines.iter().enumerate() {
            set_line(p, &self.kit.body, line, x, top + BODY_BASE + LINE_PITCH * k as f32, color);
        }
        lines.len()
    }

    pub fn width(&self, which: Which, s: &str) -> f32 {
        first_line(&self.face(which).font, s).width
    }
}
