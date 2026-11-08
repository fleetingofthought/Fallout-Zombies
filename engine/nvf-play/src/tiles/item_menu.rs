//! Shared container and barter view state [design: `container_menu`, `barter-menu.md` section 8].
//! Paths and click meanings belong to each adapter; row identity, measuring, scroll,
//! guards and alpha marks follow one implementation.

use super::layout::{HOTRECT_FILL, OWN_ALPHA};
use super::list::{self, Counts, Drag, RowTile, Wanted};
use super::menus::{SizesWritten, Tiled};
use crate::menu::look;
use crate::menu::model::{Category, MenuModel, Mode, Row, RowGuard, RowKey, Side};
use crate::pipboy::items::Info;
use bevy::prelude::Resource;
use fnv_ui::{TileId, Ui, Value};

/// [design] File names and paths only; no menu expressions are duplicated here.
pub struct Paths {
    pub menu: &'static str,
    pub lists: [&'static str; 2],
    pub titles: [&'static str; 2],
    pub template: &'static str,
    pub row: &'static str,
    pub row_text: &'static str,
    pub row_mark: &'static str,
    pub targets: &'static [(&'static str, MenuTile)],
}

pub const SIDES: [Side; 2] = [Side::Container, Side::Player];

/// [design] Check the paths and row children each adapter writes or reads.
pub fn built(ui: &Ui, layout: &Paths, paths: Vec<String>, children: &[(&str, &str)]) -> Tiled {
    let (menu_name, template_name, row_name) = (layout.menu, layout.template, layout.row);
    for path in &paths {
        if ui.find_tile(path).is_none() {
            return Tiled::No(format!("{path} missing"));
        }
    }
    let Some(menu) = ui.find_tile(layout.menu) else { return Tiled::No(format!("{menu_name} missing")) };
    let Some(template) = ui.tile(menu).templates.iter().find(|t| t.name.eq_ignore_ascii_case(layout.template)) else {
        return Tiled::No(format!("{template_name} missing"));
    };
    let row = template.body.iter().find_map(|n| match n {
        fnv_ui::xml::Node::Element(e) if e.attr("name").is_some_and(|n| n.eq_ignore_ascii_case(layout.row)) => Some(e),
        _ => None,
    });
    let Some(row) = row else { return Tiled::No(format!("{template_name}/{row_name} missing")) };
    for &(kind, name) in children {
        if !row.elements().any(|e| e.is(kind) && e.attr("name").is_some_and(|n| n.eq_ignore_ascii_case(name))) {
            return Tiled::No(format!("{template_name}/{row_name}/{name} missing"));
        }
    }
    Tiled::Yes
}

/// A row as shown.
#[derive(Clone, Debug, PartialEq)]
pub struct RowShown {
    pub key: RowKey,
    /// [observed: the tile's name, the item's without the count].
    pub name: String,
    pub text: String,
    pub equipped: bool,
}

/// A list as shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListShown {
    pub rows: Vec<RowShown>,
    pub category: Category,
    /// The row with the highlight: only in the list with the focus.
    pub selected: Option<usize>,
    /// The rows the category leaves out [observed: `_num_filtered` 12 of 15 in AID, 15 of 16 in
    /// AMMO, 0 in ITEMS].
    pub filtered: usize,
}

/// One whole state of the model, as the menu shows it: a frame with the same writes nothing
/// but sizes and scroll. A plain snapshot, so tests need no world.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// By [`Side`].
    pub lists: [ListShown; 2],
    pub titles: [String; 2],
    pub weight: String,
    pub focus: Side,
    /// The model's selection was last moved by the pointer: the lists stay where they are
    /// [owner's playtest list 7, item 11].
    pub pointer_pick: bool,
    /// The "How many?" prompt is open. Nothing here draws it; it is a change of what the menu
    /// shows, so the log reads the lists again once the prompt has closed [design].
    pub asking: bool,
    /// [`MenuModel::navigated`].
    pub navigated: u32,
    /// The selected item's figures ([`look::item_figures`]) when there is one.
    pub card: Option<Info>,
    pub icon: Option<String>,
    /// Take All has something to take.
    pub take_all: bool,
    /// The container adapter's Take All hint.
    pub take_all_hint: String,
    pub note: Option<String>,
}

/// The rows of a listing with their keys.
pub fn rows_shown(rows: &[Row]) -> Vec<RowShown> {
    let mut out: Vec<RowShown> = Vec::with_capacity(rows.len());
    for (index, r) in rows.iter().enumerate() {
        let Some(key) = RowKey::at(rows, index) else { continue };
        out.push(RowShown { key, name: r.name.clone(), text: look::row_text(r), equipped: r.equipped });
    }
    out
}

/// A list's view: where it is scrolled, and what the last frame worked out.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ListView {
    /// The scrollbar's value, whole.
    value: f64,
    counts: Counts,
    /// The highlighted row's top and height before the scroll.
    selected: Option<(f64, f64)>,
    /// The highlighted row the scroll last followed.
    watched: Option<RowKey>,
    /// Its top and height, the list's height, the scroll's scale and its highest value then.
    geometry: Option<(f64, f64, f64, f64, f64)>,
    /// [`Snapshot::navigated`] then.
    navigated: u32,
    /// The `_scroll_delta` a list that fits keeps: its last overflowing one, the file's before
    /// that (26) [observed once: 15 of 16 rows filtered read 57.2; hypothesis: that every list
    /// keeps it so].
    kept_delta: Option<f64>,
    category: Category,
    /// The file's `_scroll_delta` before any code wrote it, for a list that fits.
    file_delta: Option<f64>,
}

impl Default for ListView {
    fn default() -> Self {
        ListView { value: 0.0, counts: Counts { items: 1.0, visible: 1.0, delta: None }, selected: None, watched: None, geometry: None, navigated: 0, kept_delta: None, category: Category::All, file_delta: None }
    }
}

/// The tile menu's state: what input and the writer share.
#[derive(Resource)]
pub struct MenuTiles {
    /// The menu while it is drawn from tiles (the adapter's decision this frame).
    pub menu: Option<TileId>,
    pub(super) rows: [Vec<RowTile<RowKey>>; 2],
    paths: &'static Paths,
    views: [ListView; 2],
    shown: Option<Snapshot>,
    written_in: Option<TileId>,
    pub sizes: SizesWritten,
    /// A press on a marker, held.
    pub drag: Option<Drag>,
    /// The scroll last logged as drawn.
    drawn: [Option<f64>; 2],
    /// [design] The fallback's reason is warned once per session.
    pub warned: bool,
}

/// The heading the pointer is over, for the hand-built wheel (`crate::menu::menu_mouse`), which
/// steps its category there [design: owner request #1192].
#[derive(Resource, Default)]
pub struct TileWheelTitle(pub Option<Side>);

/// What a click on the menu's tiles stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuTile {
    Row(Side, usize),
    Arrow(Side, i32),
    Title(Side),
    /// [documented: barter_menu.xml] A hotrect over the list without focus.
    Focus(Side),
    TakeAll,
    Accept,
    Exit,
    Bar(Side, list::BarPiece),
    /// Another tile of the menu: nothing to do.
    Other,
}


impl Default for MenuTiles {
    fn default() -> Self { Self::new(&super::container_menu::PATHS) }
}

impl MenuTiles {
    pub fn name(&self) -> &'static str { self.paths.menu }

    pub fn new(paths: &'static Paths) -> Self {
        Self { paths, menu: None, rows: Default::default(), views: Default::default(), shown: None,
            written_in: None, sizes: Default::default(), drag: None, drawn: [None; 2], warned: false }
    }
}

pub(super) fn set(ui: &mut Ui, path: &str, name: &str, v: Value) {
    if let Some(t) = ui.find_tile(path) {
        ui.set_trait(t, name, v);
    }
}

pub(super) fn on(b: bool) -> Value {
    Value::Num(f64::from(u8::from(b)))
}

/// [design] Reconcile row identity and write the common list content.
pub fn write_lists(ui: &mut Ui, s: &Snapshot, st: &mut MenuTiles) {
    for side in SIDES {
        let i = side as usize;
        let wanted: Vec<Wanted<'_, RowKey>> = s.lists[i].rows.iter().map(|r| Wanted { key: &r.key, name: &r.name }).collect();
        list::reconcile(ui, st.paths.lists[i], st.paths.template, &mut st.rows[i], &wanted);
        for (row, shown) in st.rows[i].iter().zip(&s.lists[i].rows) {
            if let Some(t) = list::child(ui, row.tile, st.paths.row_text) {
                ui.set_trait(t, "string", Value::Str(shown.text.clone()));
                ui.set_trait(t, "alpha", Value::Num(255.0));
            }
            if let Some(t) = list::child(ui, row.tile, st.paths.row_mark) {
                ui.set_trait(t, "visible", on(shown.equipped));
            }
        }
        set(ui, st.paths.titles[i], "string", Value::Str(s.titles[i].clone()));
        set(ui, st.paths.lists[i], "_num_filtered", Value::Num(s.lists[i].filtered as f64));
    }
}

/// A tiled frame's sizes and places, after what changed was written (see the module notes):
/// the tree worked out, the texts measured until no wrap or size moves [design: three rounds,
/// a title's width to a card's value's wrap], the rows placed, the counts, scroll and highlight
/// written, the tree worked out again. `write_sizes` is one measuring round, each on the tree
/// just worked out, so it finds and loads a font computed from what the model wrote.
pub fn settle(ui: &mut Ui, s: &Snapshot, st: &mut MenuTiles, write_sizes: &mut dyn FnMut(&mut Ui, &mut SizesWritten) -> usize) {
    ui.update();
    for _ in 0..3 {
        if write_sizes(ui, &mut st.sizes) == 0 {
            break;
        }
        ui.update();
    }
    for side in SIDES {
        let i = side as usize;
        let Some(list_tile) = ui.find_tile(st.paths.lists[i]) else { continue };
        let heights = list::measure_rows(ui, &st.rows[i], st.paths.row_text);
        let page = ui.tile(list_tile).get("height").map_or(0.0, Value::num);
        let counts = list::counts(&heights, page);
        let view = &mut st.views[i];
        let file_delta = *view.file_delta.get_or_insert_with(|| ui.tile(list_tile).get("_scroll_delta").map_or(26.0, Value::num));
        view.kept_delta = counts.delta.or(view.kept_delta);
        list::write_counts(ui, list_tile, &counts, view.kept_delta.unwrap_or(file_delta));
        view.counts = counts;
        if view.category != s.lists[i].category {
            view.category = s.lists[i].category;
            view.value = 0.0;
        }
        let picked = s.lists[i].selected.filter(|&k| k < heights.len());
        view.selected = picked.map(|k| (heights[..k].iter().sum(), heights[k]));
        let key = picked.map(|k| s.lists[i].rows[k].key);
        let scale = counts.delta.unwrap_or(file_delta);
        let geometry = view.selected.map(|(top, height)| (top, height, page, scale, counts.most()));
        // The keys' picks bring their row into view, the pointer's do not. So does a key's or
        // the wheel's move that ends on the same row, and a row moved by a refresh or a new
        // measure (rows above it came or went, a wrap, the list's height, or a wrap below it,
        // which changes what a value moves); while none of that happens the bar and the marker
        // move the list freely.
        let moved = view.watched != key || view.navigated != s.navigated || view.geometry != geometry;
        if let (true, true, Some((top, height))) = (!s.pointer_pick, moved, view.selected) {
            view.value = list::keep_row_shown(view.value, top, height, page, scale, counts.most());
        }
        view.watched = key;
        view.geometry = geometry;
        view.navigated = s.navigated;
        view.value = view.value.clamp(0.0, counts.most());
    }
    st.rewrite(ui);
    ui.update();
}

impl MenuTiles {
    /// [`settle`] with the snapshot written last, on every frame the menu is drawn.
    pub fn settle_shown(&mut self, ui: &mut Ui, write_sizes: &mut dyn FnMut(&mut Ui, &mut SizesWritten) -> usize) {
        if let Some(s) = self.shown.take() {
            settle(ui, &s, self, write_sizes);
            self.shown = Some(s);
        }
    }

    /// The scroll and the highlight written from the view: after [`settle`]'s places, and
    /// after the input's step and its clicks ([`super::input::feed`]).
    pub fn rewrite(&self, ui: &mut Ui) {
        for side in SIDES {
            let i = side as usize;
            let Some(list_tile) = ui.find_tile(self.paths.lists[i]) else { continue };
            list::write_value(ui, list_tile, self.views[i].value);
            list::write_highlight(ui, list_tile, self.views[i].selected, self.views[i].value);
        }
    }

    /// The tile a click on `tile` stands for, while the menu is drawn from its tiles
    /// ([`Self::menu`]). None otherwise: off or in the fallback a script's queued click on the
    /// file's tile acts on nothing.
    pub fn clicked(&self, ui: &Ui, tile: TileId) -> Option<MenuTile> {
        let menu = self.menu?;
        if !ui.is_live(tile) || ui.root_of(tile) != menu {
            return None;
        }
        Some(self.stands_for(ui, tile))
    }

    fn stands_for(&self, ui: &Ui, tile: TileId) -> MenuTile {
        let mut piece = None;
        let mut at = Some(tile);
        while let Some(t) = at {
            for side in SIDES {
                if let Some(index) = self.rows[side as usize].iter().position(|r| r.tile == t) {
                    return MenuTile::Row(side, index);
                }
            }
            if piece.is_none() {
                piece = list::bar_piece(ui, t);
            }
            if let Some((_, what)) = self.paths.targets.iter().find(|(name, _)| ui.tile(t).name.eq_ignore_ascii_case(name)) {
                return *what;
            }
            for side in SIDES {
                if self.paths.lists[side as usize].rsplit('/').next().is_some_and(|name| ui.tile(t).name.eq_ignore_ascii_case(name)) {
                    return piece.map_or(MenuTile::Other, |p| MenuTile::Bar(side, p));
                }
            }
            at = ui.tile(t).parent;
        }
        MenuTile::Other
    }

    /// The index of `side`'s row at `index` when the row the tree shows is still the model's
    /// row there; a tile whose row moved or left the list acts on nothing.
    pub fn row_at(&self, m: &MenuModel, side: Side, index: usize) -> Option<usize> {
        self.row_in(side, index, &rows_shown(&m.list(side).rows))
    }

    /// [`Self::row_at`] over the rows `now`.
    pub(super) fn row_in(&self, side: Side, index: usize, now: &[RowShown]) -> Option<usize> {
        let tile_key = &self.rows[side as usize].get(index)?.key;
        (now.get(index)?.key == *tile_key).then_some(index)
    }

    /// The guard for commands on `side`'s row `index`: what its tile stands for, in the menu
    /// `mode`. `selected`: they act on it as the selected row.
    pub fn guard(&self, mode: Mode, side: Side, index: usize, selected: bool, then: u8) -> Option<RowGuard> {
        let key = self.rows[side as usize].get(index)?.key;
        Some(RowGuard { mode, side, index, key, selected, then })
    }

    /// The list tile of `side`.
    pub fn list_tile(&self, ui: &Ui, side: Side) -> Option<TileId> {
        ui.find_tile(self.paths.lists[side as usize])
    }

    /// A list's scroll now.
    pub fn value(&self, side: Side) -> f64 {
        self.views[side as usize].value
    }

    /// Set a list's scroll, held in its bar's range (the bar's pieces, the marker's drag): the
    /// view's own, no command of the model's. Returns whether it moved.
    pub fn scroll_to(&mut self, side: Side, value: f64) -> bool {
        let view = &mut self.views[side as usize];
        let value = value.clamp(0.0, view.counts.most());
        let moved = value != view.value;
        view.value = value;
        moved
    }

    /// Whether what `s` shows differs from what was written in `menu`, and if so that it is now.
    pub fn is_new(&mut self, menu: TileId, s: &Snapshot) -> bool {
        if self.written_in != Some(menu) {
            self.written_in = Some(menu);
            self.shown = None;
        }
        if self.shown.as_ref() == Some(s) {
            return false;
        }
        self.shown = Some(s.clone());
        true
    }

    /// The snapshot written last.
    pub fn shown(&self) -> Option<&Snapshot> {
        self.shown.as_ref()
    }

    /// Whether a new snapshot is needed whatever changed in the model: none yet in `menu`.
    pub fn needs_snapshot(&self, menu: TileId) -> bool {
        self.written_in != Some(menu) || self.shown.is_none()
    }

    /// The menu closed: nothing of it stays for the next (the warning does).
    pub fn close(&mut self) {
        *self = MenuTiles { warned: self.warned, ..Self::new(self.paths) };
    }

    /// Logged when the scroll drawn changes, and when it is not the value written (smoke reads
    /// both): the view's value must outlast a click's pulse, as the wait menu's slider.
    pub fn check_drawn(&mut self, ui: &Ui, log: &mut dyn FnMut(String)) {
        for side in SIDES {
            let i = side as usize;
            let Some(list_tile) = ui.find_tile(self.paths.lists[i]) else { continue };
            let Some(v) = list::value_shown(ui, list_tile) else { continue };
            let n = |x: f64| Value::Num(x).text();
            let name = side_name(side);
            let menu = self.paths.menu;
            if self.drawn[i] != Some(v) {
                log(format!("tiles: {menu} {name} list scrolled to {} of {}", n(v), n(self.views[i].counts.most())));
                self.drawn[i] = Some(v);
            }
            if v != self.views[i].value {
                log(format!("tiles: {menu} {name} list drawn at {}, not the {} written", n(v), n(self.views[i].value)));
            }
        }
    }
}

pub(super) fn side_name(side: Side) -> &'static str {
    match side {
        Side::Container => "container",
        Side::Player => "player",
    }
}

/// The mark and clip writes that follow what is in the tree, after the scripts and before
/// the drawing: a tile injected or a window enabled at any time is covered from that frame on.
/// A walk of the menu's tiles, writing only what is not there yet.
pub fn mark_tiles(ui: &mut Ui, paths: &Paths) {
    let Some(menu) = ui.menu(paths.menu) else { return };
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
    for list_path in paths.lists {
        set(ui, &format!("{list_path}/{}", list::HIGHLIGHT), HOTRECT_FILL, Value::Num(1.0));
    }
}

/// Whether the menu takes the pointer and keys this frame: drawn from its tiles and no prompt
/// over it, hand-built or from tiles (no click goes past the prompt [documented:
/// quantity_menu.xml, `&no_click_past;`]).
pub fn takes_input(menu: Option<TileId>, prompt: bool) -> Option<TileId> {
    menu.filter(|_| !prompt)
}

/// Whether the pointer's coming onto a row selects it: it moved there this frame, the wheel
/// does not hold the selection (`crate::menu::WheelHold`) and it is not selected already: so a
/// list moved under a resting pointer leaves the choice alone [owner 2026-10-22].
pub fn hover_selects(moved: bool, wheel_holds: bool, already: bool) -> bool {
    moved && !wheel_holds && !already
}

/// The commands a pointer click on a row sends, as the hand-built menu's (`menu_mouse`): the
/// row selected and shift, the stack; another row, it selected and acted on; the row selected,
/// its action.
pub fn row_commands(side: Side, index: usize, on_it: bool, shift: bool) -> Vec<crate::menu::model::Command> {
    use crate::menu::model::Command;
    match (on_it, shift) {
        (true, true) => vec![Command::Stack],
        (false, _) => vec![Command::Hover(side, index), if shift { Command::Stack } else { Command::One }],
        (true, false) => vec![Command::Select(side, index)],
    }
}
