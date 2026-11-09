//! What every Pip-Boy page shares, drawn into the screen's image
//! (docs/spec-notes/pipboy.md §2): the heading (the page's title and its cards of figures)
//! and the tab line along the foot, and the tools the pages draw with. The screen around
//! them (background, scanlines) is `device_screen.rs`'s.
//! Lengths are the canvas's units (1024 × 768), from its top left corner.
//!
//! The numbers are the menu files' [documented: `menus\main\inventory_menu.xml` and the
//! pieces it includes: `card_info.xml`, `tabline.xml`, `tabline_template.xml`,
//! `super_text_box.xml`, `text_box.xml`, `box.xml`, `line.xml`,
//! `vertical_fade_line.xml`, `menus\globals.xml`] unless marked otherwise.

use super::{Clickable, Frame, Page, PageBody, PipBoy, PipBoyInput, PipBoyRoot, PipBoyScreen, CANVAS};
use crate::interface::{pipboy_amber as amber, at, face, plain_font, set_line, Face, Picture, Pictures};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_font::{Font, Line};
use nvf_assets::GameData;

/// The menus' rectangle on the canvas: 50 in and down, 855 × 650.
pub const MAIN: Rect = Rect { min: Vec2::new(50.0, 50.0), max: Vec2::new(905.0, 700.0) };
/// Every line is 2 thick on a PC.
pub const THICK: f32 = 2.0;
/// Lines are drawn at brightness 200, text at 255, a dimmed text at 128; a box's fill at
/// brightness 128 and alpha 32.
pub const LINE: f32 = 200.0;
pub const TEXT: f32 = 255.0;
pub const DIMMED: f32 = 128.0;
pub const FILL: (f32, f32) = (128.0, 32.0);
/// A vertical fade line is 60 long.
pub const FADE: f32 = 60.0;
/// The heading: the title (font 4) 70 in, a line of 50 before it and one from 20 after
/// it up to the first card.
const TITLE_X: f32 = 70.0;
const TITLE_GAP: f32 = 20.0;
const LEAD_LINE: f32 = 50.0;
/// A card of figures: its title 5 in, its value right-aligned 10 from its right end,
/// both 20 down, in font 2.
const CARD_TEXT_X: f32 = 5.0;
const CARD_TEXT_Y: f32 = 20.0;
const CARD_VALUE_IN: f32 = 10.0;
/// A button's text under the button's own y: `text_box.xml` centres the text in a box
/// `_verbuf` (15 in the menus' buttons) taller than it, then 7 + 3 lower (not glowing)
/// [documented: `text_box.xml` `button_text` y; the original's "Maintain R)" sits there
/// on the owner's ITEMS screenshot].
pub const BUTTON_TEXT_Y: f32 = 15.0 / 2.0 + 10.0;
/// The tab line: 650 down, as wide as the menus; a tab is its text and 35 across, 15 up
/// and down; the lines between the tabs are as long as each other [hypothesis: the files
/// leave their length to code; it fills the line evenly, as on the owner's screenshots].
const TABLINE_Y: f32 = 650.0;
const TAB_HORBUF: f32 = 35.0;
const TAB_VERBUF: f32 = 15.0;
/// A text sits 3 under the middle of its box.
const BOX_TEXT_DROP: f32 = 3.0;

/// The fonts' files by the number the menu files give them [documented:
/// `Fallout_default.ini`, fonts.md §4].
pub const FONTS: [&str; 8] = [
    "textures\\fonts\\glow_monofonto_large.fnt",
    "textures\\fonts\\monofonto_large.fnt",
    "textures\\fonts\\glow_monofonto_medium.fnt",
    "textures\\fonts\\monofonto_verylarge02_dialogs2.fnt",
    "textures\\fonts\\fixedsys_comp_uniform_width.fnt",
    "textures\\fonts\\glow_monofonto_vl_dialogs.fnt",
    "textures\\fonts\\baked-in_monofonto_large.fnt",
    "textures\\fonts\\glow_futura_caps_large.fnt",
];

/// How far under a text's top its letters stand: half the font's size less a little, as
/// measured for font 2 (15 of 31; message-menu.md §3) [hypothesis for the other fonts].
pub fn base(font: &Font) -> f32 {
    font.size * 15.0 / 31.0
}

/// What the pages are drawn with, read when the Pip-Boy first comes up.
#[derive(Resource, Default)]
pub struct Kit {
    faces: Vec<Face>,
    pictures: Option<Pictures>,
    /// The menus are the screen menu: the pages leave out their title, the header of page
    /// names stands in its place ([`heading_title`], [`header`]).
    pub screen: bool,
}

impl Kit {
    fn load(&mut self, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) {
        if self.faces.is_empty() {
            self.faces = FONTS.iter().map(|f| face(f, plain_font(31.0, 13.0), data, images, bc)).collect();
            self.pictures = Some(Pictures::load(data));
        }
    }

    /// Font `n` (1 to 8) as the menu files number them.
    pub fn face(&self, n: usize) -> &Face {
        &self.faces[n.clamp(1, FONTS.len()) - 1]
    }
}

/// Where a text's letters start for a given anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Centre,
}

/// The tools of a page's drawing.
pub struct Ctx<'a> {
    pub kit: &'a mut Kit,
    pub data: Option<&'a GameData>,
    pub images: &'a mut Assets<Image>,
    pub bc: bool,
    pub glyphs: &'a crate::pad::PadGlyphs,
}

impl<'a> Ctx<'a> {
    pub fn new(kit: &'a mut Kit, data: Option<&'a GameData>, images: &'a mut Assets<Image>, formats: Option<&CompressedImageFormatSupport>, glyphs: &'a crate::pad::PadGlyphs) -> Ctx<'a> {
        let bc = crate::interface::bc_supported(formats);
        kit.load(data, images, bc);
        Ctx { kit, data, images, bc, glyphs }
    }

    pub fn picture(&mut self, path: &str) -> Option<Picture> {
        self.kit.pictures.as_mut()?.picture(path, self.data, self.images, self.bc)
    }

    /// A picture tinted `color`, stretched over `node`.
    pub fn image(&mut self, p: &mut ChildSpawnerCommands, path: &str, node: Node, color: Color) -> Option<Entity> {
        let (image, rect) = self.picture(path)?;
        Some(p.spawn((node, ImageNode { rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) })).id())
    }

    /// A straight line from the atlas's solid piece (or a plain fill).
    pub fn line(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if self.image(p, "solid.dds", at(x, y, w, h), color).is_none() {
            p.spawn((at(x, y, w, h), BackgroundColor(color)));
        }
    }

    /// A line `len` long down from `y` that fades out downwards, or (`up`) up from `y`
    /// that fades out upwards.
    pub fn fade(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, len: f32, up: bool, color: Color) {
        let (name, top) = if up { ("fade_to_top.dds", y - len) } else { ("fade_to_bottom.dds", y) };
        if self.image(p, name, at(x, top, THICK, len), color).is_none() {
            p.spawn((at(x, top, THICK, len), BackgroundColor(color.with_alpha(0.5))));
        }
    }

    /// A box's four sides (and its fill, if any).
    pub fn boxed(&mut self, p: &mut ChildSpawnerCommands, r: Rect, line: Color, fill: Option<Color>) {
        if let Some(f) = fill {
            p.spawn((at(r.min.x, r.min.y, r.width(), r.height()), BackgroundColor(f)));
        }
        let (w, h) = (r.width(), r.height());
        self.line(p, r.min.x, r.min.y, w, THICK, line);
        self.line(p, r.min.x, r.max.y - THICK, w, THICK, line);
        self.line(p, r.min.x, r.min.y + THICK, THICK, h - 2.0 * THICK, line);
        self.line(p, r.max.x - THICK, r.min.y + THICK, THICK, h - 2.0 * THICK, line);
    }

    /// The first line of `s` in font `font`.
    pub fn first_line(&self, font: usize, s: &str) -> Line {
        self.kit.face(font).font.lines(s, None).into_iter().next().unwrap_or_default()
    }

    /// `s` in font `font`, broken at `wrap`.
    pub fn lines(&self, font: usize, s: &str, wrap: Option<f32>) -> Vec<Line> {
        self.kit.face(font).font.lines(s, wrap)
    }

    /// Width of `s` in font `font`.
    pub fn width(&self, font: usize, s: &str) -> f32 {
        self.first_line(font, s).width
    }

    /// The height of a text's line in font `font`.
    pub fn line_height(&self, font: usize) -> f32 {
        self.kit.face(font).font.size
    }

    /// Draw one line of `s` whose top is at `top`; returns its width.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, p: &mut ChildSpawnerCommands, font: usize, s: &str, x: f32, top: f32, align: Align, color: Color) -> f32 {
        let face = self.kit.face(font);
        let width = crate::pad::hint_width(face, s, self.glyphs);
        let x = match align {
            Align::Left => x,
            Align::Right => x - width,
            Align::Centre => x - width / 2.0,
        };
        crate::pad::set_hint(p, face, s, x, top + base(&face.font), color, self.glyphs);
        width
    }

    /// Draw a line already set, its top at `top`.
    #[allow(clippy::too_many_arguments)]
    pub fn set(&mut self, p: &mut ChildSpawnerCommands, font: usize, line: &Line, x: f32, top: f32, align: Align, color: Color) {
        let x = match align {
            Align::Left => x,
            Align::Right => x - line.width,
            Align::Centre => x - line.width / 2.0,
        };
        let face = self.kit.face(font);
        set_line(p, face, line, x, top + base(&face.font), color);
    }

    /// Draw `s` broken at `wrap`, lines `pitch` apart from `top`; returns the lines drawn.
    #[allow(clippy::too_many_arguments)]
    pub fn paragraph(&mut self, p: &mut ChildSpawnerCommands, font: usize, s: &str, x: f32, top: f32, wrap: f32, pitch: f32, color: Color) -> usize {
        let lines = self.lines(font, s, Some(wrap));
        for (i, l) in lines.iter().enumerate() {
            self.set(p, font, l, x, top + pitch * i as f32, Align::Left, color);
        }
        lines.len()
    }
}

/// A card of figures in a page's heading: where it starts, how wide, and its title and
/// value.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub x: f32,
    pub width: f32,
    pub title: String,
    pub value: String,
}

impl Card {
    pub fn new(x: f32, width: f32, title: impl Into<String>, value: impl Into<String>) -> Card {
        Card { x, width, title: title.into(), value: value.into() }
    }
}

/// Draw a page's heading inside the menus' rectangle (whose top left is `MAIN.min`): a
/// vertical line fading down at its left, a line of 50 to the title, the title in font 4,
/// a line from 20 after the title to the first card, and the cards.
pub fn heading(p: &mut ChildSpawnerCommands, c: &mut Ctx, title: &str, cards: &[Card]) {
    let (x0, y0) = (MAIN.min.x, MAIN.min.y);
    let line = amber(LINE);
    c.fade(p, x0, y0, FADE, false, line);
    c.line(p, x0, y0, LEAD_LINE, THICK, line);
    let after = heading_title(p, c, title);
    let first = cards.first().map_or(MAIN.width(), |k| k.x);
    if x0 + first > after {
        c.line(p, after, y0, x0 + first - after, THICK, line);
    }
    for k in cards {
        card(p, c, x0 + k.x, y0, k.width, &k.title, &k.value);
    }
}

/// Where the heading's lead line ends: where the line runs on from in the screen menu, which
/// has no title ([`heading_title`]).
pub fn lead_line_end() -> f32 {
    MAIN.min.x + LEAD_LINE
}

/// A page's title in its heading (font 4, 70 in, on the heading's line); where the line
/// goes on after it. In the screen menu the title is left out, the header of page names
/// standing on the line in its place ([`header`]), and the line runs on from the lead line
/// [design: the owner, 2026-10-31, not the page's name twice].
pub fn heading_title(p: &mut ChildSpawnerCommands, c: &mut Ctx, title: &str) -> f32 {
    let (x0, y0) = (MAIN.min.x, MAIN.min.y);
    if c.kit.screen {
        return x0 + LEAD_LINE;
    }
    let w = c.text(p, 4, title, x0 + TITLE_X, y0 - base(&c.kit.face(4).font) + THICK, Align::Left, amber(TEXT));
    x0 + TITLE_X + w + TITLE_GAP
}

/// The heading with a name (font 4) right-aligned `right_in` short of the menus' right
/// edge in place of cards, the line running from the title up to 20 before it [documented:
/// item_mod_menu.xml `IMM_ItemName`; hypothesis: the line stops at the name].
pub fn heading_named(p: &mut ChildSpawnerCommands, c: &mut Ctx, title: &str, name: &str, right_in: f32) {
    let (x0, y0) = (MAIN.min.x, MAIN.min.y);
    let line = amber(LINE);
    let bright = amber(TEXT);
    c.fade(p, x0, y0, FADE, false, line);
    c.line(p, x0, y0, LEAD_LINE, THICK, line);
    let top = y0 - base(&c.kit.face(4).font) + THICK;
    let after = heading_title(p, c, title);
    let right = MAIN.max.x - right_in;
    let name_w = c.text(p, 4, name, right, top, Align::Right, bright);
    let stop = right - name_w - TITLE_GAP;
    if stop > after {
        c.line(p, after, y0, stop - after, THICK, line);
    }
}

/// A card: a line along its top, one fading down its right side, the title at the left
/// and the value right-aligned, both in font 2.
pub fn card(p: &mut ChildSpawnerCommands, c: &mut Ctx, x: f32, y: f32, width: f32, title: &str, value: &str) {
    let line = amber(LINE);
    let bright = amber(TEXT);
    c.line(p, x, y, width, THICK, line);
    c.fade(p, x + width - THICK, y, FADE, false, line);
    c.text(p, 2, title, x + CARD_TEXT_X, y + CARD_TEXT_Y, Align::Left, bright);
    if !value.is_empty() {
        c.text(p, 2, value, x + width - CARD_VALUE_IN, y + CARD_TEXT_Y, Align::Right, bright);
    }
}

/// The brightness (0 to 1) of each row of a DXT1 texture's top level, read down its
/// first column; `None` for any other file.
pub fn scanline_column(dds: &[u8]) -> Option<Vec<f32>> {
    let d = fnv_dds::Dds::parse(dds).ok()?;
    if !matches!(d.format, fnv_dds::Format::Dxt1) {
        return None;
    }
    let (w, h) = (d.header.width as usize, d.header.height as usize);
    let top = d.levels.first()?.first()?;
    let blocks_across = w.div_ceil(4);
    let rgb = |c: u16| [((c >> 11) & 31) as f32 / 31.0, ((c >> 5) & 63) as f32 / 63.0, (c & 31) as f32 / 31.0];
    let mut out = Vec::with_capacity(h);
    for by in 0..h.div_ceil(4) {
        let b = top.get(by * blocks_across * 8..by * blocks_across * 8 + 8)?;
        let (c0, c1) = (u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]]));
        let (p0, p1) = (rgb(c0), rgb(c1));
        let mix = |a: f32, x: f32, y: f32| x + (y - x) * a;
        let palette: [[f32; 3]; 4] = if c0 > c1 {
            [p0, p1, std::array::from_fn(|i| mix(1.0 / 3.0, p0[i], p1[i])), std::array::from_fn(|i| mix(2.0 / 3.0, p0[i], p1[i]))]
        } else {
            [p0, p1, std::array::from_fn(|i| mix(0.5, p0[i], p1[i])), [0.0; 3]]
        };
        for row in 0..4 {
            let index = (b[4 + row] & 3) as usize;
            let [r, g, bl] = palette[index];
            out.push((r + g + bl) / 3.0);
        }
    }
    out.truncate(h);
    Some(out)
}

/// Where the tab line puts its tabs: each tab's box, left to right, and the length of
/// the lines between them (and at the two ends).
pub fn tab_boxes(widths: &[f32]) -> (Vec<Rect>, f32) {
    let boxes: Vec<f32> = widths.iter().map(|w| w + TAB_HORBUF).collect();
    let gap = ((MAIN.width() - boxes.iter().sum::<f32>()) / (boxes.len() + 1) as f32).max(0.0);
    let mut x = MAIN.min.x + gap;
    let rects = boxes
        .iter()
        .map(|&w| {
            let r = Rect::new(x, TABLINE_Y - TAB_VERBUF, x + w, TABLINE_Y + TAB_VERBUF);
            x += w + gap;
            r
        })
        .collect();
    (rects, gap)
}

/// The tab line along the foot: a vertical line rising at each end, the tabs' names in
/// font 2 joined by lines at their middle, the tab shown boxed and filled.
fn tab_line(p: &mut ChildSpawnerCommands, c: &mut Ctx, page: Page, current: usize) {
    let line = amber(LINE);
    let bright = amber(TEXT);
    let names = page.tabs();
    let text_h = c.line_height(2);
    let widths: Vec<f32> = names.iter().map(|n| c.width(2, n)).collect();
    let (boxes, gap) = tab_boxes(&widths);
    let half = (text_h + TAB_VERBUF) / 2.0;
    c.fade(p, MAIN.min.x, TABLINE_Y + THICK / 2.0, FADE, true, line);
    c.fade(p, MAIN.max.x - THICK, TABLINE_Y + THICK / 2.0, FADE, true, line);
    let mut x = MAIN.min.x;
    for (i, (name, r)) in names.iter().zip(&boxes).enumerate() {
        c.line(p, x, TABLINE_Y - THICK / 2.0, gap, THICK, line);
        x = r.max.x;
        let r = Rect::new(r.min.x, TABLINE_Y - half, r.max.x, TABLINE_Y + half);
        let selected = i == current;
        if selected {
            c.boxed(p, r, line, Some(amber(FILL.0).with_alpha(FILL.1 / 255.0)));
        } else {
            // Under the pointer another tab is boxed, unfilled [hypothesis: the
            // `text_box.xml` focus box; the original's look is not captured].
            p.spawn((at(0.0, 0.0, CANVAS.x, CANVAS.y), TabHoverBox(i), Visibility::Hidden, Pickable::IGNORE)).with_children(|b| c.boxed(b, r, line, None));
        }
        let top = r.min.y + (r.height() - text_h) / 2.0 + BOX_TEXT_DROP;
        c.text(p, 2, name, r.min.x + TAB_HORBUF / 2.0, top, Align::Left, bright);
        // The tab's box takes the pointer.
        p.spawn((at(r.min.x, r.min.y, r.width(), r.height()), Clickable(PipBoyInput::Tab(i)), Pickable::default()))
            .observe(super::on_click)
            .observe(on_tab_over)
            .observe(on_tab_out);
    }
    c.line(p, x, TABLINE_Y - THICK / 2.0, MAIN.max.x - x, THICK, line);
}

/// The box shown round tab `.0` while the pointer is on it.
#[derive(Component, Clone, Copy)]
pub struct TabHoverBox(pub usize);

fn on_tab_over(over: On<PointerOver>, nodes: Query<&Clickable>, mut pip: ResMut<PipBoy>, mut out: MessageWriter<super::sounds::TabHovered>) {
    if let Ok(Clickable(PipBoyInput::Tab(i))) = nodes.get(over.entity) {
        pip.hover_tab = Some(*i);
        out.write(super::sounds::TabHovered(*i));
    }
}

fn on_tab_out(out: On<PointerOut>, nodes: Query<&Clickable>, mut pip: ResMut<PipBoy>) {
    if let Ok(Clickable(PipBoyInput::Tab(i))) = nodes.get(out.entity) {
        if pip.hover_tab == Some(*i) {
            pip.hover_tab = None;
        }
    }
}

/// Show the hover box of the tab under the pointer (none while the Pip-Boy is down).
pub fn hover_boxes(mut pip: ResMut<PipBoy>, mut boxes: Query<(&TabHoverBox, &mut Visibility)>) {
    // Laid out in the Pip-Boy's screen texture, at its size whatever the UI scale (#1631).
    let _canvas = crate::interface::original_units();
    // A page drawn from its tiles has no hand-built tabs for this to be left over from.
    if (!pip.open() || boxes.is_empty()) && pip.hover_tab.is_some() {
        pip.hover_tab = None;
    }
    for (b, mut v) in &mut boxes {
        let want = if pip.hover_tab == Some(b.0) { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// Rebuild the frame (and with it every page's body) when the Pip-Boy's page or tab
/// changed, or it came up, or a page went over to its tiles or back. A page drawn from its
/// file (`crate::tiles::stats_menu::StatsTiles::ready`, the one decision) keeps the shell and
/// the screen menu's header, and has none of the hand-built page: its heading and tab line are
/// the tiles', once. The device's background and scanlines are `device_screen.rs`'s either way.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    pip: Res<PipBoy>,
    screen: Option<Res<PipBoyScreen>>,
    roots: Query<Entity, With<PipBoyRoot>>,
    mut kit: ResMut<Kit>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    glyphs: Res<crate::pad::PadGlyphs>,
    tiles: Option<Res<crate::tiles::stats_menu::StatsTiles>>,
    mut shown: Local<Option<(Page, usize, Frame, bool)>>,
) {
    // Laid out in the Pip-Boy's screen texture, at its size whatever the UI scale (#1631).
    let _canvas = crate::interface::original_units();
    let Some(screen) = screen else { return };
    let tiled = tiles.is_some_and(|t| t.ready.is_some());
    let want = pip.shown().then(|| (pip.page, pip.tab(), pip.frame, tiled));
    if want == *shown && (want.is_none() || !roots.is_empty()) {
        return;
    }
    *shown = want;
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some((page, tab, frame, tiled)) = want else { return };
    // The screen menu is plain, as the barter menu is: the device's own screen around the
    // pages is `device_screen.rs`'s, on the device only (docs/design/screen-menu.md §2).
    let device = frame == Frame::Device;
    let mut c = Ctx::new(&mut kit, data.as_deref(), &mut images, formats.as_deref(), &glyphs);
    c.kit.screen = !device;
    let root = Node { overflow: Overflow::clip(), ..at(0.0, 0.0, CANVAS.x, CANVAS.y) };
    commands.spawn((PipBoyRoot, root, UiTargetCamera(screen.camera), Name::new("Pip-Boy menus"))).with_children(|r| {
        // The shell every view of a page keeps: the screen menu's header.
        if !device {
            header(r, &mut c, page);
        }
        if !tiled {
            hand_built_page(r, &mut c, page, tab);
        }
    });
}

/// What a hand-built page has inside the shell: the tab line and the body its module fills.
fn hand_built_page(r: &mut ChildSpawnerCommands, c: &mut Ctx, page: Page, tab: usize) {
    tab_line(r, c, page, tab);
    r.spawn((at(0.0, 0.0, CANVAS.x, CANVAS.y), PageBody { page, tab }, Pickable::IGNORE));
}

// ---- The screen menu's header (docs/design/screen-menu.md §3.1) ---------------------

/// The box shown round a page's name in the header while the pointer is on it.
#[derive(Component)]
struct HeaderHover;

/// The names are in the title's font, their boxes spaced as far apart as the title is
/// from its line [design].
const HEADER_FONT: usize = 4;
const HEADER_GAP: f32 = TITLE_GAP * 2.0;

/// Where the header puts the three names' boxes on the canvas: from the title's place,
/// standing on the heading's line (their foot on it), each as wide as its name and a
/// tab's padding and as high as a tab.
pub fn header_boxes(widths: [f32; 3], text_h: f32) -> [Rect; 3] {
    let h = text_h + TAB_VERBUF;
    let foot = MAIN.min.y + THICK;
    let mut x = MAIN.min.x + TITLE_X - TAB_HORBUF / 2.0;
    widths.map(|w| {
        let r = Rect::new(x, foot - h, x + w + TAB_HORBUF, foot);
        x = r.max.x + HEADER_GAP;
        r
    })
}

/// The screen menu's header, in place of the page's title: STATS, ITEMS and DATA on the
/// heading's line, the page shown boxed and filled as the tab shown is, the others
/// dimmed; a name under the pointer boxed; a click shows its page.
fn header(r: &mut ChildSpawnerCommands, c: &mut Ctx, page: Page) {
    let line = amber(LINE);
    let text_h = c.line_height(HEADER_FONT);
    let widths = Page::ALL.map(|p| c.width(HEADER_FONT, p.title()));
    for (p, b) in Page::ALL.into_iter().zip(header_boxes(widths, text_h)) {
        let selected = p == page;
        let local = Rect::new(0.0, 0.0, b.width(), b.height());
        // Centred in its box by the letters themselves: across by the line's width, down
        // by its tallest letter's rise (capitals, no descenders) [design: the owner,
        // 2026-10-31; the font's own baseline left the names high].
        let name = c.first_line(HEADER_FONT, p.title());
        let face = c.kit.face(HEADER_FONT);
        let rise = name.glyphs.iter().map(|g| face.font.glyph(g.code).ascent).fold(0.0, f32::max);
        let (x, base) = ((b.width() - name.width) / 2.0, (b.height() + rise) / 2.0);
        r.spawn((at(b.min.x, b.min.y, b.width(), b.height()), Clickable(PipBoyInput::Page(p)), Pickable::default()))
            .with_children(|n| {
                if selected {
                    c.boxed(n, local, line, Some(amber(FILL.0).with_alpha(FILL.1 / 255.0)));
                } else {
                    n.spawn((at(0.0, 0.0, b.width(), b.height()), HeaderHover, Visibility::Hidden, Pickable::IGNORE)).with_children(|h| c.boxed(h, local, line, None));
                }
                set_line(n, c.kit.face(HEADER_FONT), &name, x, base, amber(if selected { TEXT } else { DIMMED }));
            })
            .observe(super::on_click)
            .observe(on_header_over)
            .observe(on_header_out);
    }
}

fn header_hover(entity: Entity, on: bool, children: &Query<&Children>, boxes: &mut Query<&mut Visibility, With<HeaderHover>>) {
    for c in children.get(entity).into_iter().flatten() {
        if let Ok(mut v) = boxes.get_mut(*c) {
            *v = if on { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

fn on_header_over(over: On<PointerOver>, nodes: Query<&Clickable>, children: Query<&Children>, mut boxes: Query<&mut Visibility, With<HeaderHover>>, mut out: MessageWriter<super::sounds::TabHovered>) {
    if let Ok(Clickable(PipBoyInput::Page(p))) = nodes.get(over.entity) {
        header_hover(over.entity, true, &children, &mut boxes);
        out.write(super::sounds::TabHovered(p.index()));
    }
}

fn on_header_out(out: On<PointerOut>, children: Query<&Children>, mut boxes: Query<&mut Visibility, With<HeaderHover>>) {
    header_hover(out.entity, false, &children, &mut boxes);
}

// ---- Added for the STATS page ------------------------------------------------------

impl Ctx<'_> {
    /// A picture's size in its file's pixels: its atlas piece's, or the whole file's.
    pub fn picture_size(&mut self, path: &str) -> Option<Vec2> {
        let (image, rect) = self.picture(path)?;
        match rect {
            Some(r) => Some(r.size()),
            None => self.images.get(&image).map(|i| i.size().as_vec2()),
        }
    }

    /// A picture as the menu files draw an image tile: the file at `zoom` (1 = one canvas
    /// unit a pixel), shown from its top left over `w` × `h` and cut off beyond
    /// [hypothesis: an image's width and height show a part of the file and do not
    /// stretch it; `stats_menu.xml` sizes its pictures as "filewidth/fileheight already
    /// adjusted for zoom", and the Vault Boy's pieces fill exactly the parts of their
    /// files that the file gives as their sizes]. Returns the size drawn.
    #[allow(clippy::too_many_arguments)]
    pub fn image_cut(&mut self, p: &mut ChildSpawnerCommands, path: &str, x: f32, y: f32, w: f32, h: f32, zoom: f32, color: Color) -> Option<Vec2> {
        let (image, rect) = self.picture(path)?;
        let full = match rect {
            Some(r) => r,
            None => Rect::from_corners(Vec2::ZERO, self.images.get(&image)?.size().as_vec2()),
        };
        let cut = Rect::new(full.min.x, full.min.y, (full.min.x + w / zoom).min(full.max.x), (full.min.y + h / zoom).min(full.max.y));
        let size = cut.size() * zoom;
        p.spawn((at(x, y, size.x, size.y), ImageNode { rect: Some(cut), color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }));
        Some(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dxt1_column_is_read_row_by_row() {
        // A 4 × 8 DXT1 file: two blocks, white over black, and black over white.
        let mut f = vec![0u8; 128];
        f[0..4].copy_from_slice(b"DDS ");
        f[4..8].copy_from_slice(&124u32.to_le_bytes());
        f[8..12].copy_from_slice(&0x1007u32.to_le_bytes());
        f[12..16].copy_from_slice(&8u32.to_le_bytes());
        f[16..20].copy_from_slice(&4u32.to_le_bytes());
        f[76..80].copy_from_slice(&32u32.to_le_bytes());
        f[80..84].copy_from_slice(&4u32.to_le_bytes());
        f[84..88].copy_from_slice(b"DXT1");
        // Block 1: c0 white, c1 black; rows 0-1 index 0 (white), rows 2-3 index 1.
        f.extend_from_slice(&[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x55, 0x55]);
        // Block 2: every texel index 2 (a third of the way to black).
        f.extend_from_slice(&[0xFF, 0xFF, 0x00, 0x00, 0xAA, 0xAA, 0xAA, 0xAA]);
        let col = scanline_column(&f).expect("read");
        assert_eq!(col.len(), 8);
        assert_eq!(&col[..4], &[1.0, 1.0, 0.0, 0.0]);
        assert!(col[4..].iter().all(|&l| (l - 2.0 / 3.0).abs() < 1e-5));
        assert_eq!(scanline_column(b"not a dds"), None);
    }

    #[test]
    fn the_header_names_stand_on_the_heading_line() {
        // Widths about the title font's for the three names.
        let boxes = header_boxes([110.0, 110.0, 90.0], 34.0);
        for w in boxes.windows(2) {
            assert!(w[1].min.x - w[0].max.x >= HEADER_GAP - 1e-3, "{boxes:?}");
        }
        for r in boxes {
            // On the line, inside the canvas's top margin (shown by the screen menu).
            assert!((r.max.y - (MAIN.min.y + THICK)).abs() < 1e-3 && r.min.y >= 0.0, "{r:?}");
        }
        // From where the title stands, ending before the right half of the menus.
        assert!((boxes[0].min.x - (MAIN.min.x + TITLE_X - TAB_HORBUF / 2.0)).abs() < 1e-3);
        assert!(boxes[2].max.x < MAIN.min.x + MAIN.width() * 0.75);
    }

    #[test]
    fn the_tabs_share_the_line_evenly() {
        let (boxes, gap) = tab_boxes(&[100.0, 100.0, 100.0, 100.0, 100.0]);
        // 855 less five tabs of 135, over six gaps.
        assert!((gap - (855.0 - 675.0) / 6.0).abs() < 1e-4);
        assert!((boxes[0].min.x - (MAIN.min.x + gap)).abs() < 1e-4);
        assert!((MAIN.max.x - boxes[4].max.x - gap).abs() < 1e-3);
        for w in boxes.windows(2) {
            assert!((w[1].min.x - w[0].max.x - gap).abs() < 1e-4);
        }
        // Tabs too wide for the line touch.
        let (_, gap) = tab_boxes(&[300.0; 5]);
        assert_eq!(gap, 0.0);
    }
}
