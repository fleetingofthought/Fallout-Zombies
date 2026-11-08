//! Shared list selection drawing [design: trait_menu.rs and chargen.md §4.1].
//! The path table varies; readiness, opening/tree/row lifetimes, layout, scrolls and input
//! follow the trait menu's contract. Menu readers and wording stay beside their own sheets.
#![allow(non_snake_case)]
use super::layout::{HOTRECT_FILL, OWN_ALPHA};
use super::list::{self, BarPiece, Drag, RowTile, Wanted};
use super::menus::{SizesWritten, Tiled};
use super::trait_menu::{BACKGROUND_ALPHA, FAINT_ALPHA, ROW_ID};
use crate::chargen::TileAct;
use bevy::prelude::{info, warn, Res, ResMut, Resource};
use fnv_sim::FormId;
use fnv_ui::{TileId, Ui, Value};
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(super) struct Paths {
    pub TRAIT_MENU: &'static str,
    pub MAIN: &'static str,
    pub BACKGROUND: &'static str,
    pub TITLE: &'static str,
    pub LIST: &'static str,
    pub ICON: &'static str,
    pub COUNTER: &'static str,
    pub TEXT_BOX: &'static str,
    pub TEXT_BAR: &'static str,
    pub TEXT: &'static str,
    pub RESET: &'static str,
    pub DONE: &'static str,
    pub TEMPLATE: &'static str,
    pub ROW: &'static str,
    pub ROW_TEXT: &'static str,
    pub ROW_MARK: &'static str,
}

pub(super) fn paths<const CODE: u16>() -> Paths {
    if CODE == fnv_game::game::menus::TAG_SKILLS { return super::tag_menu::paths(); }
    use super::trait_menu::*;
    Paths {
        TRAIT_MENU,
        MAIN,
        BACKGROUND,
        TITLE,
        LIST,
        ICON,
        COUNTER,
        TEXT_BOX,
        TEXT_BAR,
        TEXT,
        RESET,
        DONE,
        TEMPLATE,
        ROW,
        ROW_TEXT,
        ROW_MARK,
    }
}

pub(super) fn num(ui: &Ui, path: &str, name: &str) -> f64 {
    ui.find_tile(path)
        .and_then(|t| ui.tile(t).get(name))
        .map_or(0.0, Value::num)
}

pub(super) fn set(ui: &mut Ui, path: &str, name: &str, v: Value) {
    if let Some(t) = ui.find_tile(path) {
        ui.set_trait(t, name, v);
    }
}

pub(super) fn flag(b: bool) -> Value {
    Value::Num(f64::from(u8::from(b)))
}

/// [design] Every path the model writes or the input reads is part of a successful build, and
/// the rows' template with the children written: a file a mod cut down keeps the hand-built
/// menu whole.
pub fn built<const CODE: u16>(ui: &Ui) -> Tiled {
    let Paths {
        TRAIT_MENU,
        MAIN,
        BACKGROUND,
        TITLE,
        LIST,
        ICON,
        COUNTER,
        TEXT_BOX,
        TEXT_BAR,
        TEXT,
        RESET,
        DONE,
        TEMPLATE,
        ROW,
        ROW_TEXT,
        ROW_MARK,
        ..
    } = paths::<CODE>();
    let mut paths: Vec<String> = [
        TRAIT_MENU, BACKGROUND, MAIN, TITLE, LIST, ICON, COUNTER, TEXT_BOX, TEXT_BAR, TEXT, RESET,
        DONE,
    ]
    .iter()
    .filter(|s| !s.is_empty())
    .map(|s| (*s).to_string())
    .collect();
    paths.push(format!("{LIST}/{}", list::HIGHLIGHT));
    for piece in [
        list::MARKER,
        list::UP,
        list::DOWN,
        list::PAGE_UP,
        list::PAGE_DOWN,
    ] {
        paths.push(format!("{LIST}/{}/{piece}", list::SCROLLBAR));
        if !TEXT_BAR.is_empty() { paths.push(format!("{TEXT_BAR}/{piece}")); }
    }
    if CODE == fnv_game::game::menus::TAG_SKILLS { paths.push(super::tag_menu::DIVIDER.into()); }
    for path in &paths {
        if ui.find_tile(path).is_none() {
            return Tiled::No(format!("{path} missing"));
        }
    }
    let Some(menu) = ui.find_tile(TRAIT_MENU) else {
        return Tiled::No(format!("{TRAIT_MENU} missing"));
    };
    let Some(template) = ui
        .tile(menu)
        .templates
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(TEMPLATE))
    else {
        return Tiled::No(format!("{TEMPLATE} missing"));
    };
    let row = template.body.iter().find_map(|n| match n {
        fnv_ui::xml::Node::Element(e)
            if e.attr("name").is_some_and(|n| n.eq_ignore_ascii_case(ROW)) =>
        {
            Some(e)
        }
        _ => None,
    });
    let Some(row) = row else {
        return Tiled::No(format!("{TEMPLATE}/{ROW} missing"));
    };
    let mut children = vec![("text", ROW_TEXT), ("image", ROW_MARK)];
    if CODE == fnv_game::game::menus::TAG_SKILLS { children.push(("text", super::tag_menu::ROW_VALUE)); }
    for (kind, name) in children {
        if !row
            .elements()
            .any(|e| e.is(kind) && e.attr("name").is_some_and(|n| n.eq_ignore_ascii_case(name)))
        {
            return Tiled::No(format!("{TEMPLATE}/{ROW}/{name} missing"));
        }
    }
    Tiled::Yes
}

/// A row as shown.
#[derive(Clone, Debug, PartialEq)]
pub struct RowShown {
    /// [design] Trait FormID, or a skill actor-value code wrapped as an opaque row key.
    /// The separate menu resources keep those two namespaces apart.
    pub key: FormId,
    pub name: String,
    pub value: Option<String>,
    /// The file's `_selected`: the trait is taken. Not the highlight.
    pub taken: bool,
    /// The player does not meet its requirements.
    pub faint: bool,
}

/// One whole state of the menu, as it shows it: a plain snapshot, so tests need no world.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// Which opening of a menu this is (`crate::chargen::ChargenMenus`).
    pub generation: u64,
    pub title: String,
    pub rows: Vec<RowShown>,
    /// The row with the highlight.
    pub selected: Option<usize>,
    pub icon: Option<String>,
    pub info: String,
    pub counter: String,
    pub done: bool,
    pub taken: usize,
    pub most: usize,
    pub only_add: bool,
    /// `crate::chargen::TraitsOpen::navigated`.
    pub navigated: u32,
}

/// Which scroll a bar moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scroll {
    List,
    Text,
}

/// What a click on the menu's tiles stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionTile {
    Row(FormId),
    Reset,
    Done,
    Bar(Scroll, BarPiece),
    /// Another tile of the menu: nothing to do.
    Other,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ListScroll {
    /// The scrollbar's value, whole.
    pub(super) value: f64,
    pub(super) most: f64,
    /// The highlighted row's top and height before the scroll.
    pub(super) selected: Option<(f64, f64)>,
    /// What a value moves the rows now.
    pub(super) scale: f64,
    /// The highlight shows: off when the menu opens, and off the rows.
    pub(super) highlight: bool,
    /// [`Snapshot::navigated`] the view last followed.
    pub(super) navigated: Option<u32>,
    /// The rows standing whole in the list from its top, for the keys' page.
    pub(super) page_rows: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct TextScroll {
    pub(super) value: f64,
    pub(super) most: f64,
    /// The row whose text this is: another starts at the top.
    pub(super) key: Option<FormId>,
}

/// What the file gave a tree before any code wrote it, kept as long as that tree stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FileCounts {
    pub(super) menu: TileId,
    /// The list's `_scroll_delta`, for a list that fits.
    pub(super) list_step: f64,
    /// The description bar's `_number_of_items`.
    pub(super) text_items: f64,
    pub(super) text_top: f64,
}

impl FileCounts {
    fn read<const CODE: u16>(ui: &Ui, menu: TileId) -> FileCounts {
        let Paths { LIST, TEXT_BAR, TEXT, .. } = paths::<CODE>();
        FileCounts {
            menu,
            list_step: num(ui, LIST, "_scroll_delta"),
            text_items: num(ui, TEXT_BAR, "_number_of_items"),
            text_top: num(ui, TEXT, "y"),
        }
    }
}

/// The tile menu's state: what the writer, the input and the hand-built menu share.
#[derive(Resource, Default)]
pub struct SelectionTiles<const CODE: u16> {
    /// The menu while it is drawn from its tiles: the one decision ([`prepare`]).
    pub menu: Option<TileId>,
    /// Done was pressed, or the pause menu is over it: drawn, taking nothing.
    pub(super) held: bool,
    /// The opening the scrolls and the highlight belong to ([`SelectionTiles::begin`]).
    pub(super) generation: Option<u64>,
    /// The counts of the tree last prepared.
    pub(super) file: Option<FileCounts>,
    pub(super) rows: Vec<RowTile<FormId>>,
    pub(super) shown: Option<Snapshot>,
    pub(super) written_in: Option<TileId>,
    pub(super) sizes: SizesWritten,
    pub(super) list: ListScroll,
    pub(super) text: TextScroll,
    pub(super) drag: Option<(Scroll, Drag)>,
    /// The pointer is over the description or its bar, for the wheel.
    pub(super) over_text: bool,
    /// For `crate::chargen`'s `input`, with the opening they were made in.
    pub(super) acts: Vec<(u64, TileAct)>,
    /// What the snapshot was last made from.
    pub(super) seen: Option<(crate::chargen::lists::ListMenu, u64, u32, bool)>,
    /// What was last logged as drawn.
    pub(super) drawn: Option<(f64, f64)>,
    pub(super) told: bool,
    /// [design] Each fallback reason is warned once a session.
    pub(super) warned: HashSet<String>,
}

/// Write what the menu shows of `s`: the rows, the strings, the picture and the backing. The
/// places are [`settle`]'s, once the texts are measured. False when a row could not be made.
pub fn write<const CODE: u16>(ui: &mut Ui, s: &Snapshot, st: &mut SelectionTiles<CODE>) -> bool {
    let Paths {
        TRAIT_MENU,
        BACKGROUND,
        TITLE,
        LIST,
        ICON,
        COUNTER,
        TEXT,
        DONE,
        TEMPLATE,
        ROW,
        ROW_TEXT,
        ROW_MARK,
        ..
    } = paths::<CODE>();
    let wanted: Vec<Wanted<'_, FormId>> = s
        .rows
        .iter()
        .map(|r| Wanted {
            key: &r.key,
            name: ROW,
        })
        .collect();
    if !list::reconcile(ui, LIST, TEMPLATE, &mut st.rows, &wanted) {
        return false;
    }
    for (row, shown) in st.rows.iter().zip(&s.rows) {
        if list::child(ui, row.tile, ROW_TEXT).is_none()
            || list::child(ui, row.tile, ROW_MARK).is_none()
        {
            return false;
        }
        if CODE == fnv_game::game::menus::TAG_SKILLS {
            let Some(value) = list::child(ui, row.tile, super::tag_menu::ROW_VALUE) else { return false };
            ui.set_trait(value, "string", Value::Str(shown.value.clone().unwrap_or_default()));
        }
        ui.set_trait(row.tile, "id", Value::Num(ROW_ID));
        ui.set_trait(row.tile, "string", Value::Str(shown.name.clone()));
        ui.set_trait(row.tile, "_selected", flag(shown.taken));
        ui.set_trait(
            row.tile,
            "_TextAlpha",
            Value::Num(if shown.faint { FAINT_ALPHA } else { 255.0 }),
        );
    }
    // The list's `_enabled` follows its `visible`, which the file leaves to code; without it
    // the rows take no pointer and the highlight stays hidden.
    set(ui, LIST, "visible", flag(true));
    if CODE == fnv_game::game::menus::TAG_SKILLS {
        // [documented: chargen.md §3] The list's top is left to code; 65 [observed: job ov_tags, §3.1].
        set(ui, LIST, "y", Value::Num(65.0));
        // The file leaves its height to code too: 728, fourteen rows of 52 [observed: job ov_tags].
        set(ui, LIST, "height", Value::Num(728.0));
        set(ui, TRAIT_MENU, "_OnlyAdd", flag(s.only_add));
    }
    set(ui, TITLE, "string", Value::Str(s.title.clone()));
    set(ui, TEXT, "string", Value::Str(s.info.clone()));
    set(ui, COUNTER, "string", Value::Str(s.counter.clone()));
    set(ui, ICON, "visible", flag(s.icon.is_some()));
    set(
        ui,
        ICON,
        "filename",
        Value::Str(s.icon.clone().unwrap_or_default()),
    );
    set(ui, TRAIT_MENU, "_CurrPoints", Value::Num(s.taken as f64));
    set(ui, TRAIT_MENU, "_MaxPoints", Value::Num(s.most as f64));
    set(ui, DONE, "target", flag(s.done));
    set(ui, BACKGROUND, "alpha", Value::Num(BACKGROUND_ALPHA));
    true
}

/// How far a text whose top stands at `top` at value 0 scrolls, a value moving it `step`, in a
/// box `page` high: as far as its bar's own count goes (`file_most`), which does not follow the
/// text [observed: 9 items on every row read, the text 207 to 277 high, the original's job
/// `trait`], and further only for a text that count would leave cut off [design: nothing in
/// the original is known to end the scroll sooner or reach further, and a description must be
/// readable to its end].
pub fn text_most(top: f64, height: f64, page: f64, step: f64, file_most: f64) -> f64 {
    let over = top + height - page;
    if over <= 0.0 || step <= 0.0 {
        file_most
    } else {
        file_most.max((over / step - 1e-9).ceil())
    }
}

/// A tiled frame's sizes, places and scrolls (see the module notes). `write_sizes` is one
/// measuring round, on the tree just worked out, so it finds a font computed from what was
/// written.
pub fn settle<const CODE: u16>(
    ui: &mut Ui,
    s: &Snapshot,
    st: &mut SelectionTiles<CODE>,
    write_sizes: &mut dyn FnMut(&mut Ui, &mut SizesWritten) -> usize,
) {
    let Paths {
        LIST,
        TEXT_BOX,
        TEXT_BAR,
        TEXT,
        ROW_TEXT,
        ..
    } = paths::<CODE>();
    ui.update();
    for _ in 0..3 {
        if write_sizes(ui, &mut st.sizes) == 0 {
            break;
        }
        ui.update();
    }
    if let Some(list_tile) = ui.find_tile(LIST) {
        let heights = list::measure_rows(ui, &st.rows, ROW_TEXT);
        let page = num(ui, LIST, "height");
        let step = st.file.map_or(0.0, |f| f.list_step);
        // The container capture's counts hold here too [observed: 14 rows of 52 in 676, the
        // original's job `trait`: 2 items, `_scroll_delta` 52, the marker 322].
        let counts = list::counts(&heights, page);
        list::write_counts(ui, list_tile, &counts, step);
        let scale = counts.delta.unwrap_or(step);
        st.list.scale = scale;
        st.list.most = counts.most();
        let picked = s.selected.filter(|&k| k < heights.len());
        st.list.selected = picked.map(|k| (heights[..k].iter().sum(), heights[k]));
        if st.list.navigated != Some(s.navigated) {
            // A first frame shows no highlight; the keys' choice does.
            st.list.highlight |= st.list.navigated.is_some();
            if let Some((top, height)) = st.list.selected {
                st.list.value =
                    list::keep_row_shown(st.list.value, top, height, page, scale, counts.most());
            }
            st.list.navigated = Some(s.navigated);
        }
        st.list.value = st.list.value.clamp(0.0, counts.most());
        let mut y = 0.0;
        st.list.page_rows = heights
            .iter()
            .take_while(|h| {
                y += **h;
                y <= page
            })
            .count();
    }
    let key = s.selected.and_then(|k| s.rows.get(k)).map(|r| r.key);
    if st.text.key != key {
        st.text.key = key;
        st.text.value = 0.0;
    }
    if let Some(bar) = ui.find_tile(TEXT_BAR) {
        let step = num(ui, TEXT_BAR, "_ScrollDelta");
        let value = num(ui, TEXT_BAR, "_current_value");
        // The text's place with no scroll, whatever the file adds to it.
        let top = num(ui, TEXT, "y") + value * step;
        let visible = ui
            .tile(bar)
            .get("_number_of_visible_items")
            .map_or(1.0, Value::num);
        let file_items = st.file.map_or(visible, |f| f.text_items);
        let most = text_most(
            top,
            num(ui, TEXT, "height"),
            num(ui, TEXT_BOX, "height"),
            step,
            (file_items - visible).max(0.0),
        );
        st.text.most = most;
        st.text.value = st.text.value.clamp(0.0, most);
        // Left at the file's count unless the text needs more: its marker and arrows stay as the
        // file has them on every row [observed].
        if most + visible != file_items {
            ui.set_trait(bar, "_number_of_items", Value::Num(most + visible));
        } else if ui
            .tile(bar)
            .get("_number_of_items")
            .is_none_or(|n| n.num() != file_items)
        {
            ui.set_trait(bar, "_number_of_items", Value::Num(file_items));
        }
    }
    if TEXT_BAR.is_empty() { super::tag_menu::settle_text(ui, st); }
    st.rewrite(ui);
    ui.update();
}

/// A length that can be used: a number, more than nothing.
pub(super) fn room(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

/// [`Tiled`] for what [`settle`] left, read from the tree as it stands, every frame: each row and
/// its children live, room for the list and the text, and text measured [design: a menu with no
/// room or no measure cannot be read, so the hand-built one stays; a row's spacing alone is no
/// measure].
pub(super) fn usable<const CODE: u16>(ui: &Ui, s: &Snapshot, st: &SelectionTiles<CODE>) -> Tiled {
    let Paths {
        LIST,
        TEXT_BOX,
        TEXT_BAR,
        TEXT,
        ROW_TEXT,
        ROW_MARK,
        ..
    } = paths::<CODE>();
    if !room(num(ui, LIST, "height")) || !room(num(ui, LIST, "width")) || !room(st.list.scale) {
        return Tiled::No(format!("{LIST} has no room or scroll step"));
    }
    if !room(num(ui, TEXT_BOX, "height"))
        || !room(num(ui, TEXT_BOX, "width"))
        || (!TEXT_BAR.is_empty() && !room(num(ui, TEXT_BAR, "_ScrollDelta")))
    {
        return Tiled::No(format!("{TEXT_BOX} has no room or scroll step"));
    }
    if st.rows.len() != s.rows.len() {
        return Tiled::No("a row could not be made".into());
    }
    for r in &st.rows {
        if CODE == fnv_game::game::menus::TAG_SKILLS && ui.is_live(r.tile) && list::child(ui, r.tile, super::tag_menu::ROW_VALUE).is_none() {
            return Tiled::No("a row lost its skill value".into());
        }
        if !ui.is_live(r.tile) || list::child(ui, r.tile, ROW_MARK).is_none() {
            return Tiled::No(format!("a row lost its tile or {ROW_MARK}"));
        }
        let Some(text) = list::child(ui, r.tile, ROW_TEXT) else {
            return Tiled::No(format!("a row lost its {ROW_TEXT}"));
        };
        if !room(ui.tile(text).get("height").map_or(0.0, Value::num))
            || !room(ui.tile(r.tile).get("height").map_or(0.0, Value::num))
        {
            return Tiled::No("a row measured no height".into());
        }
    }
    let text = num(ui, TEXT, "height");
    if !text.is_finite() || (!s.info.is_empty() && text <= 0.0) {
        return Tiled::No(format!("{TEXT} measured no height"));
    }
    Tiled::Yes
}

/// [`drawn`]'s decision from what it reads.
pub(super) fn drawn_by<const CODE: u16>(
    st: &SelectionTiles<CODE>,
    opening: Option<u64>,
    game_menu: Option<u16>,
    live: Option<TileId>,
) -> Option<TileId> {
    let menu = st.menu?;
    (opening.is_some() && st.generation == opening && game_menu == Some(CODE) && live == Some(menu))
        .then_some(menu)
}

/// One frame's readiness (see the module notes). `new` is what the menu shows now, when it was
/// read this frame. Whatever is not ready removes what was made; the hand-built menu stands.
pub fn prepare<const CODE: u16>(
    ui: &mut Ui,
    new: Option<Snapshot>,
    st: &mut SelectionTiles<CODE>,
    write_sizes: &mut dyn FnMut(&mut Ui, &mut SizesWritten) -> usize,
) -> Tiled {
    let Paths { TRAIT_MENU, .. } = paths::<CODE>();
    let status = built::<CODE>(ui);
    if !status.yes() {
        st.fail(ui);
        return status;
    }
    let Some(menu) = ui.menu(TRAIT_MENU) else {
        st.fail(ui);
        return Tiled::No(format!("{TRAIT_MENU} missing"));
    };
    if st.written_in != Some(menu) {
        // Built again: nothing of the last tree stands.
        st.fail(ui);
        st.written_in = Some(menu);
    }
    if st.file.is_none_or(|f| f.menu != menu) {
        // Before this frame's writes. A tree this drawing wrote to keeps what it read first.
        st.file = Some(FileCounts::read::<CODE>(ui, menu));
    }
    if let Some(s) = new {
        st.begin(s.generation);
        if st.shown.as_ref() != Some(&s) {
            if !write(ui, &s, st) {
                st.fail(ui);
                return Tiled::No("a row could not be made".into());
            }
            st.shown = Some(s);
            st.told = false;
        }
    }
    let Some(s) = st.shown.take() else {
        st.menu = None;
        return Tiled::No("nothing to show yet".into());
    };
    settle(ui, &s, st, write_sizes);
    let status = usable(ui, &s, st);
    st.shown = Some(s);
    if status.yes() {
        st.menu = Some(menu);
    } else {
        st.fail(ui);
    }
    status
}

impl<const CODE: u16> SelectionTiles<CODE> {
    /// A new opening, whatever became of the last: nothing of how it was left shows in this one.
    /// The same opening goes on through a fallback and back, with its scrolls.
    pub fn begin(&mut self, generation: u64) {
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            self.list = ListScroll::default();
            self.text = TextScroll::default();
            self.drag = None;
            self.over_text = false;
            self.acts.clear();
            self.drawn = None;
            self.told = false;
        }
    }

    /// Whether this drawing is of the opening `generation`.
    pub fn is_for(&self, generation: u64) -> bool {
        self.generation == Some(generation)
    }

    /// Nothing of this drawing stays: the rows made go from the tree and the hand-built menu
    /// draws. A later frame builds them again. The opening and the tree go on.
    pub fn fail(&mut self, ui: &mut Ui) {
        for r in std::mem::take(&mut self.rows) {
            if ui.is_live(r.tile) {
                ui.unload_tile(r.tile);
            }
        }
        self.menu = None;
        self.shown = None;
        self.seen = None;
        self.written_in = None;
        self.drag = None;
        self.over_text = false;
        self.acts.clear();
        self.sizes.clear();
        self.drawn = None;
    }

    /// The menu closed: nothing of it stays for the next (the warnings do).
    pub fn close(&mut self) {
        *self = SelectionTiles {
            warned: std::mem::take(&mut self.warned),
            ..Default::default()
        };
    }

    /// Whether there is anything to close.
    pub fn has_state(&self) -> bool {
        self.generation.is_some()
            || self.menu.is_some()
            || self.written_in.is_some()
            || self.shown.is_some()
            || self.file.is_some()
    }

    /// The menu while it takes input: drawn, not closing, not under the pause menu.
    pub fn takes(&self) -> Option<TileId> {
        self.menu.filter(|_| !self.held)
    }

    /// Whether a new snapshot is needed: what it was made from changed, none stands, or the
    /// menu was built again (a tree dropped takes what was written with it).
    pub(super) fn needs_snapshot(
        &self,
        menu: Option<TileId>,
        from: &(crate::chargen::lists::ListMenu, u64, u32, bool),
    ) -> bool {
        self.shown.is_none() || self.written_in != menu || self.seen.as_ref() != Some(from)
    }

    /// The scrolls and the highlight written from the view: after [`settle`]'s places, and
    /// after the input's step and its clicks ([`super::input::feed`]).
    pub fn rewrite(&self, ui: &mut Ui) {
        let Paths { LIST, TEXT_BAR, .. } = paths::<CODE>();
        if let Some(l) = ui.find_tile(LIST) {
            list::write_value(ui, l, self.list.value);
            list::write_highlight(
                ui,
                l,
                self.list.selected.filter(|_| self.list.highlight),
                self.list.value,
            );
        }
        if let Some(bar) = ui.find_tile(TEXT_BAR) {
            list::write_bar_value(ui, bar, self.text.value);
        } else if TEXT_BAR.is_empty() { super::tag_menu::rewrite_text(ui, self); }
    }

    /// Queue an act for `crate::chargen`'s `input`, as of the opening shown.
    pub(super) fn act(&mut self, act: TileAct) {
        if let Some(g) = self.generation {
            let act = if CODE == fnv_game::game::menus::TAG_SKILLS {
                match act { TileAct::Hover(key) => TileAct::SkillHover(key.0 as u16), TileAct::Click(key) => TileAct::SkillClick(key.0 as u16), other => other }
            } else { act };
            self.acts.push((g, act));
        }
    }

    /// The acts queued for the opening `generation`; those of another are dropped.
    pub fn take_acts(&mut self, generation: u64) -> Vec<TileAct> {
        std::mem::take(&mut self.acts)
            .into_iter()
            .filter(|(g, _)| *g == generation)
            .map(|(_, a)| a)
            .collect()
    }

    /// Nothing queued acts: the menu is closing.
    pub fn drop_acts(&mut self) {
        self.acts.clear();
    }

    /// The whole rows the list shows at once, for the keys' page.
    pub fn page_rows(&self) -> i32 {
        self.list.page_rows.max(1) as i32
    }

    pub(super) fn scroll_to(&mut self, which: Scroll, value: f64) {
        match which {
            Scroll::List => self.list.value = value.clamp(0.0, self.list.most),
            Scroll::Text => self.text.value = value.clamp(0.0, self.text.most),
        }
    }

    pub(super) fn value(&self, which: Scroll) -> f64 {
        match which {
            Scroll::List => self.list.value,
            Scroll::Text => self.text.value,
        }
    }

    /// The wheel: the description under the pointer, else the list, by the file's step of one
    /// value a notch [documented: scrollbar_vert.xml's `_wheelmoved`]. Applied on the next
    /// frame's writes.
    pub fn wheel(&mut self, by: f64) {
        let which = if self.over_text {
            Scroll::Text
        } else {
            Scroll::List
        };
        self.scroll_to(which, self.value(which) + by * if CODE == fnv_game::game::menus::TAG_SKILLS && which == Scroll::Text { super::tag_menu::TEXT_STEP } else { 1.0 });
    }

    /// The description's scroll by `by` values (the keys, `crate::chargen`).
    pub fn scroll_text(&mut self, by: f64) {
        self.scroll_to(Scroll::Text, self.text.value + by * if CODE == fnv_game::game::menus::TAG_SKILLS { super::tag_menu::TEXT_STEP } else { 1.0 });
    }

    /// A drag ends with its press or when the menu stops taking input.
    pub fn hold_drag(&mut self, held: bool) {
        let now = self.takes();
        if self
            .drag
            .is_some_and(|(_, d)| !list::drag_holds(&d, now, held))
        {
            self.drag = None;
        }
    }

    /// What a click on `tile` stands for, while the menu takes input. None otherwise: off, in
    /// the fallback or while it closes a script's queued click on the file's tile acts on
    /// nothing.
    pub fn clicked(&self, ui: &Ui, tile: TileId) -> Option<SelectionTile> {
        let Paths {
            RESET,
            DONE,
            LIST,
            TEXT_BAR,
            ..
        } = paths::<CODE>();
        let menu = self.takes()?;
        if !ui.is_live(tile) || ui.root_of(tile) != menu {
            return None;
        }
        let mut piece = None;
        let mut at = Some(tile);
        while let Some(t) = at {
            if let Some(r) = self.rows.iter().find(|r| r.tile == t) {
                return Some(SelectionTile::Row(r.key));
            }
            if piece.is_none() {
                piece = list::bar_piece(ui, t);
            }
            match ui.tile(t).name.to_ascii_lowercase().as_str() {
                name if name.eq_ignore_ascii_case(RESET.rsplit('/').next().unwrap()) => {
                    return Some(SelectionTile::Reset)
                }
                name if name.eq_ignore_ascii_case(DONE.rsplit('/').next().unwrap()) => {
                    return Some(SelectionTile::Done)
                }
                name if name.eq_ignore_ascii_case(LIST.rsplit('/').next().unwrap()) => {
                    return Some(piece.map_or(SelectionTile::Other, |p| {
                        SelectionTile::Bar(Scroll::List, p)
                    }))
                }
                name if name.eq_ignore_ascii_case(TEXT_BAR.rsplit('/').next().unwrap()) => {
                    return Some(piece.map_or(SelectionTile::Other, |p| {
                        SelectionTile::Bar(Scroll::Text, p)
                    }))
                }
                _ => {}
            }
            at = ui.tile(t).parent;
        }
        Some(SelectionTile::Other)
    }

    /// Whether `key` is the row with the highlight.
    pub(super) fn is_selected(&self, key: FormId) -> bool {
        self.shown.as_ref().is_some_and(|s| {
            s.selected
                .and_then(|k| s.rows.get(k))
                .is_some_and(|r| r.key == key)
        })
    }

    /// Logged when a scroll drawn changes, and when it is not the value written (smoke reads
    /// them): the view's values must outlast a click's pulse, as the container menu's.
    pub fn check_drawn(&mut self, ui: &Ui, log: &mut dyn FnMut(String)) {
        let Paths {
            TRAIT_MENU,
            LIST,
            TEXT_BOX,
            TEXT_BAR,
            TEXT,
            ..
        } = paths::<CODE>();
        let Some(l) = ui.find_tile(LIST) else { return };
        let text_value = if TEXT_BAR.is_empty() { self.file.map(|f| f.text_top - num(ui, TEXT, "y")) }
            else { ui.find_tile(TEXT_BAR).and_then(|t| ui.tile(t).get("_current_value").map(Value::num)) };
        let (Some(lv), Some(tv)) = (list::value_shown(ui, l), text_value) else {
            return;
        };
        let n = |x: f64| Value::Num(x).text();
        if self.drawn != Some((lv, tv)) {
            let end = num(ui, TEXT, "y") + num(ui, TEXT, "height");
            log(format!("tiles: {TRAIT_MENU} list scrolled to {} of {}, text scrolled to {} of {}, its end at {} in a box {} high", n(lv), n(self.list.most), n(tv), n(self.text.most), n(end), n(num(ui, TEXT_BOX, "height"))));
            if TEXT_BAR.is_empty() && self.text.most > 0.0 && tv == self.text.most {
                log(format!("tiles: {TRAIT_MENU} description end reached"));
            }
            self.drawn = Some((lv, tv));
        }
        if lv != self.list.value {
            log(format!(
                "tiles: {TRAIT_MENU} list drawn at {}, not the {} written",
                n(lv),
                n(self.list.value)
            ));
        }
        if tv != self.text.value {
            log(format!(
                "tiles: {TRAIT_MENU} text drawn at {}, not the {} written",
                n(tv),
                n(self.text.value)
            ));
        }
    }
}

/// Act on a frame's click (see the module notes). `None` when the click is not for this menu,
/// so the other menus' rules see it. `physical` as `super::menus::key_click_acts`.
pub fn follow_click<const CODE: u16>(
    st: &mut SelectionTiles<CODE>,
    ui: &Ui,
    c: &fnv_ui::input::Click,
    at: Option<(f64, f64)>,
    held: bool,
    physical: &[&str],
) -> Option<()> {
    let Paths { TRAIT_MENU, .. } = paths::<CODE>();
    use fnv_ui::input::By;
    let what = st.clicked(ui, c.tile)?;
    if let By::Key(k) = &c.by {
        if !super::menus::key_click_acts(TRAIT_MENU, k, physical) {
            return Some(());
        }
    }
    match what {
        SelectionTile::Row(key) => {
            info!("tiles: {TRAIT_MENU} row {key} clicked on its tiles");
            st.act(TileAct::Click(key));
        }
        SelectionTile::Reset => {
            info!("tiles: {TRAIT_MENU} Reset on its tiles");
            st.act(TileAct::Reset);
        }
        SelectionTile::Done => {
            info!("tiles: {TRAIT_MENU} Done on its tiles");
            st.act(TileAct::Done);
        }
        SelectionTile::Bar(which, BarPiece::Marker) => {
            let (owner, bar) = bar_of::<CODE>(ui, which)?;
            let menu = st.takes()?;
            st.drag = at
                .filter(|_| held && matches!(c.by, By::Pointer))
                .and_then(|(_, y)| list::marker_grab_on(ui, menu, owner, bar, y))
                .map(|d| (which, d));
        }
        SelectionTile::Bar(which, piece) => {
            let (_, bar) = bar_of::<CODE>(ui, which)?;
            let value = st.value(which);
            // The description's page areas do nothing: the file's `_jump_size` is 0 [observed].
            if let Some(v) = list::after_piece_on(ui, bar, piece, value) {
                info!("tiles: {TRAIT_MENU} {which:?} {piece:?} on its tiles");
                st.scroll_to(which, v);
            }
        }
        SelectionTile::Other => {}
    }
    Some(())
}

/// A scroll's tile and its bar.
pub(super) fn bar_of<const CODE: u16>(ui: &Ui, which: Scroll) -> Option<(TileId, TileId)> {
    let Paths { LIST, TEXT_BAR, .. } = paths::<CODE>();
    match which {
        Scroll::List => {
            let l = ui.find_tile(LIST)?;
            Some((l, list::child(ui, l, list::SCROLLBAR)?))
        }
        Scroll::Text => {
            let b = ui.find_tile(TEXT_BAR)?;
            Some((b, b))
        }
    }
}

/// Whether `at` is over the description's box or its bar beside it. The box takes no pointer, so
/// nothing under the pointer says so; its place is worked out as the layout's, from the tiles'
/// `x` and `y` up the tree.
pub(super) fn over_text<const CODE: u16>(ui: &Ui, at: (f64, f64)) -> bool {
    let Paths {
        TEXT_BOX, TEXT_BAR, ..
    } = paths::<CODE>();
    let Some(b) = ui.find_tile(TEXT_BOX) else {
        return false;
    };
    let (mut left, mut top) = (0.0, 0.0);
    let mut up = Some(b);
    while let Some(t) = up {
        left += ui.tile(t).get("x").map_or(0.0, Value::num);
        top += ui.tile(t).get("y").map_or(0.0, Value::num);
        up = ui.tile(t).parent;
    }
    let width = num(ui, TEXT_BOX, "width") + num(ui, TEXT_BAR, "width");
    at.0 >= left && at.0 < left + width && at.1 >= top && at.1 < top + num(ui, TEXT_BOX, "height")
}

/// The pointer's part besides clicks: a drag on a marker, the row or button it came onto, and
/// whether it is over the description, for the wheel. `moved`: it moved this frame, so a list
/// scrolled under a resting pointer leaves the choice alone [as the container menu, owner
/// 2026-10-22].
pub fn follow_pointer<const CODE: u16>(
    st: &mut SelectionTiles<CODE>,
    ui: &Ui,
    report: &fnv_ui::input::Report,
    at: Option<(f64, f64)>,
    moved: bool,
) {
    let Paths { TRAIT_MENU, .. } = paths::<CODE>();
    st.over_text = at.is_some_and(|p| over_text::<CODE>(ui, p));
    if let (Some((which, drag)), Some((_, y))) = (st.drag, at) {
        if let Some(v) = list::value_under(ui, drag, y) {
            st.scroll_to(which, v);
        }
    }
    let Some((_, to)) = report.over else { return };
    // Off the rows the highlight goes and the text stays [observed: `_highlight_y` -1 and 0
    // high, the original's job `trait`].
    let what = to.and_then(|to| st.clicked(ui, to));
    if !matches!(what, Some(SelectionTile::Row(_))) {
        st.list.highlight = false;
    }
    match what {
        Some(SelectionTile::Row(key)) if st.is_selected(key) => st.list.highlight = true,
        Some(SelectionTile::Row(key)) if moved => {
            info!("tiles: {TRAIT_MENU} row {key} chosen by the pointer on its tiles");
            st.list.highlight = true;
            st.act(TileAct::Hover(key));
        }
        Some(SelectionTile::Reset | SelectionTile::Done) => st.act(TileAct::ButtonFocus),
        _ => {}
    }
}

/// The mark and clip writes that follow what is in the tree, after the scripts and before the
/// drawing: every tile takes only its own alpha, as the container menu's [hypothesis:
/// layout.rs's `OWN_ALPHA`, the same prefabs], the highlight draws its fill, the windows crop,
/// and the description's bar, beside its window, is not cropped by it.
pub fn mark_tiles<const CODE: u16>(ui: &mut Ui) {
    let Paths {
        TRAIT_MENU,
        LIST,
        TEXT_BAR,
        ..
    } = paths::<CODE>();
    let Some(menu) = ui.menu(TRAIT_MENU) else {
        return;
    };
    fnv_ui::input::mark_clip_windows(ui, menu);
    let mut stack = vec![menu];
    while let Some(t) = stack.pop() {
        if !ui.is_live(t) {
            continue;
        }
        let tile = ui.tile(t);
        stack.extend(tile.children.iter().copied());
        if !tile.get(OWN_ALPHA).is_some_and(Value::truthy) {
            ui.set_trait(t, OWN_ALPHA, Value::Num(1.0));
        }
    }
    set(
        ui,
        &format!("{LIST}/{}", list::HIGHLIGHT),
        HOTRECT_FILL,
        Value::Num(1.0),
    );
    set(ui, TEXT_BAR, fnv_ui::input::OUTSIDE_CLIP, Value::Num(1.0));
}

/// The keys the hand-built menu acts on itself (`crate::chargen`'s `input`), by the tile input's
/// names, with the controller buttons standing in for them there (`crate::pad::menu_buttons`,
/// and the shared ones). The file binds R and A and Y [documented: trait_menu.xml]: all stay the hand-built
/// keys, a script's MenuTapKey acting through the file.
pub const HAND_BUILT_KEYS: [&str; 15] = [
    "A",
    "R",
    "W",
    "S",
    "xup",
    "xdown",
    "xleft",
    "xright",
    "xbuttona",
    "xbuttonx",
    "xbuttony",
    "xbuttonlb",
    "xbuttonrb",
    "xbuttonlt",
    "xbuttonrt",
];

/// A line for the log when what the menu shows changes (smoke reads these).
pub fn describe<const CODE: u16>(ui: &Ui, s: &Snapshot, st: &SelectionTiles<CODE>) -> String {
    let Paths {
        TRAIT_MENU,
        TEXT_BOX,
        TEXT,
        ..
    } = paths::<CODE>();
    let n = |x: f64| Value::Num(x).text();
    let selected = s
        .selected
        .and_then(|k| s.rows.get(k))
        .map_or("none", |r| r.name.as_str());
    format!(
        "tiles: {TRAIT_MENU} drawn from tiles: \"{}\" {} rows, {} of {} taken, selected \"{selected}\", counter \"{}\", text height {}, box {} high, text scrolls to {}, list scrolls to {} by {}",
        s.title,
        s.rows.len(),
        s.taken,
        s.most,
        s.counter,
        n(num(ui, TEXT, "height")),
        n(num(ui, TEXT_BOX, "height")),
        n(st.text.most),
        n(st.list.most),
        n(st.list.scale),
    )
}

/// The test aid `NVF_PLAY_TEST_TRAIT_TILES=<seconds>:<step>[,…]`, on the world aids' clock
/// (`crate::pause::AidTime`): `missing` (the file reads as not there, from then on), `cut:<tile
/// name>` (the first tile of that name goes from the tree when the menu is built, a file cut
/// down, or one a mod changed while it shows) and `long` (the description reads twice over,
/// past the bar's own count on most rows, so its scroll has an end to reach). Nothing is written to the game's files.
#[derive(Resource, Default)]
pub struct SelectionAid<const CODE: u16> {
    due: Vec<(f32, AidStep)>,
    missing: bool,
    long: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AidStep {
    Missing,
    Cut(String),
    Long,
}

/// Read [`SelectionAid`]'s list.
pub fn parse_aid(s: &str) -> Result<Vec<(f32, AidStep)>, String> {
    crate::test_aid::timed(s)?
        .into_iter()
        .map(|(t, step)| match step.trim().split_once(':') {
            Some(("cut", name)) => Ok((t, AidStep::Cut(name.trim().to_string()))),
            None if step.trim() == "missing" => Ok((t, AidStep::Missing)),
            None if step.trim() == "long" => Ok((t, AidStep::Long)),
            _ => Err(format!("{step:?}: unknown step")),
        })
        .collect()
}

/// The first live tile named `name` under `root`.
fn tile_named(ui: &Ui, root: TileId, name: &str) -> Option<TileId> {
    let mut stack = vec![root];
    while let Some(t) = stack.pop() {
        if ui.is_live(t) {
            if ui.tile(t).name.eq_ignore_ascii_case(name) {
                return Some(t);
            }
            stack.extend(ui.tile(t).children.iter().rev().copied());
        }
    }
    None
}

impl<const CODE: u16> SelectionAid<CODE> {
    fn step(&mut self, ui: &mut Ui, now: Option<f32>) {
        let Paths { TRAIT_MENU, .. } = paths::<CODE>();
        let Some(now) = now else { return };
        let menu = ui.menu(TRAIT_MENU);
        self.due.retain(|(t, step)| {
            if *t > now {
                return true;
            }
            match step {
                AidStep::Missing => {
                    info!("test aid: {TRAIT_MENU} file taken as missing");
                    self.missing = true;
                    false
                }
                AidStep::Long => {
                    info!("test aid: {TRAIT_MENU} descriptions long");
                    self.long = true;
                    false
                }
                AidStep::Cut(name) => {
                    // Waits for the menu's tree.
                    let Some(menu) = menu else { return true };
                    if let Some(tile) = tile_named(ui, menu, name) {
                        info!("test aid: {TRAIT_MENU} {name} cut from its tree");
                        ui.unload_tile(tile);
                    }
                    false
                }
            }
        });
    }
}

/// Read the test aid at start.
pub(super) fn aid_from_env<const CODE: u16>() -> SelectionAid<CODE> {
    let AID = if CODE == fnv_game::game::menus::TAG_SKILLS { super::tag_menu::AID } else { super::trait_menu::AID };
    let due = match std::env::var(AID) {
        Ok(v) => parse_aid(&v).unwrap_or_else(|e| {
            warn!("{AID}: {e}");
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    SelectionAid {
        due,
        ..Default::default()
    }
}

/// Write the menu's traits before the input's step (`super::input::feed`): what changed in the
/// open menu, then its sizes, places and scrolls, every frame it is drawn ([`prepare`]). Off,
/// nothing.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_system<const CODE: u16>(
    mut script: ResMut<crate::script::ScriptState>,
    switch: Res<super::vanilla::TileMenus>,
    chargen: Option<Res<crate::chargen::ChargenMenus>>,
    sim: Option<ResMut<crate::sim::SimState>>,
    paused: Option<Res<crate::pause::Paused>>,
    time: crate::pause::AidTime,
    mut fonts: super::draw::FontKit,
    (mut st, mut aid): (ResMut<SelectionTiles<CODE>>, ResMut<SelectionAid<CODE>>),
) {
    let Paths { TRAIT_MENU, .. } = paths::<CODE>();
    if !(if CODE == fnv_game::game::menus::TAG_SKILLS { switch.tags } else { switch.traits }) {
        return;
    }
    let open = chargen
        .as_deref()
        .and_then(|c| c.list_open(CODE));
    let game_open = script
        .rt
        .as_ref()
        .is_some_and(|rt| rt.game.menu == Some(CODE) && rt.game.ui_files.is_some());
    let (Some(open), true, Some(mut sim)) = (open, game_open, sim) else {
        if st.has_state() {
            st.close();
        }
        return;
    };
    st.begin(open.generation);
    let Some(rt) = script.rt.as_mut() else { return };
    aid.step(&mut rt.game.ui_tree, time.world_secs());
    let status = if aid.missing {
        Tiled::No(if CODE == fnv_game::game::menus::TAG_SKILLS { "menus\\chargen\\char_gen_menu.xml missing" } else { "menus\\trait_menu.xml missing" }.into())
    } else {
        built::<CODE>(&rt.game.ui_tree)
    };
    if let Tiled::No(reason) = status {
        if st.warned.insert(reason.clone()) {
            warn!("{TRAIT_MENU} drawn by hand: {reason}");
        }
        st.fail(&mut rt.game.ui_tree);
        return;
    }
    st.held = open.answered || paused.is_some_and(|p| p.0);
    // What the menu shows is read when it changed.
    let from = (open.menu.clone(), open.generation, open.navigated, aid.long);
    let new = if st.needs_snapshot(rt.game.ui_tree.menu(TRAIT_MENU), &from) {
        let read = crate::chargen::with_game(&mut script, &mut sim, |g| {
            (open.menu.view(g), open.menu.trait_facts(g))
        });
        let (view, facts) = read.map_or((None, None), |(v, f)| (Some(v), f));
        let mut made = if CODE == fnv_game::game::menus::TAG_SKILLS {
            view.as_ref().zip(super::tag_menu::sheet(open.menu)).map(|(v, sheet)| super::tag_menu::snapshot(v, sheet, open.generation, open.navigated))
        } else {
            view.as_ref().zip(open.menu.trait_sheet()).map(|(v, sheet)| Snapshot::of(v, sheet, facts, open.generation, open.navigated))
        };
        if aid.long { if let Some(s) = made.as_mut() { s.info = vec![s.info.as_str(); if CODE == fnv_game::game::menus::TAG_SKILLS { 8 } else { 2 }].join("\n\n"); } }
        st.seen = Some(from);
        made
    } else {
        None
    };
    let Some(rt) = script.rt.as_mut() else { return };
    let ui = &mut rt.game.ui_tree;
    let Some(menu) = ui.menu(TRAIT_MENU) else {
        return;
    };
    // Each round's texts and fonts as the tree stands then, with the rows a first write adds.
    let mut write_sizes = |ui: &mut Ui, sizes: &mut SizesWritten| {
        let texts = super::menus::texts(ui, menu);
        let size = fonts.menu_measure(&super::menus::fonts_of(ui, &texts));
        super::menus::write_text_sizes_of(ui, &texts, sizes, &size)
    };
    let status = prepare(ui, new, &mut st, &mut write_sizes);
    if let Tiled::No(reason) = status {
        if st.warned.insert(reason.clone()) {
            warn!("{TRAIT_MENU} drawn by hand: {reason}");
        }
        return;
    }
    mark_tiles::<CODE>(ui);
    if !st.told {
        st.told = true;
        if let Some(s) = st.shown.as_ref() {
            info!("{}", describe(ui, s, &st));
            if CODE == fnv_game::game::menus::TAG_SKILLS {
                let p = paths::<CODE>();
                info!("tiles: {} Done target {}, only add {}", p.TRAIT_MENU, num(ui, p.DONE, "target"), num(ui, p.TRAIT_MENU, "_OnlyAdd"));
            }
        }
    }
}

/// [design] The hand-built input's common access to either selection resource. The
/// const-generic resources remain distinct, so their openings and queued acts cannot mix.
pub(crate) trait SelectionInput {
    fn takes(&self) -> Option<TileId>;
    fn page_rows(&self) -> i32;
    fn wheel(&mut self, by: f64);
    fn scroll_text(&mut self, by: f64);
    fn take_acts(&mut self, generation: u64) -> Vec<TileAct>;
}
impl<const CODE: u16> SelectionInput for SelectionTiles<CODE> {
    fn takes(&self) -> Option<TileId> { self.takes() }
    fn page_rows(&self) -> i32 { self.page_rows() }
    fn wheel(&mut self, by: f64) { self.wheel(by); }
    fn scroll_text(&mut self, by: f64) { self.scroll_text(by); }
    fn take_acts(&mut self, generation: u64) -> Vec<TileAct> { self.take_acts(generation) }
}
