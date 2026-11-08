//! The trait menu (1084) drawn from `menus\trait_menu.xml`, behind `[UI] TileMenus=TraitMenu`
//! [design: ui-tile-tree.md §6.2 step 3; the file's contract: chargen.md §4.1]. Off (the
//! default), none of this runs and the hand-built menu (`crate::chargen::lists`) draws.
//!
//! - **The rules stay the hand-built menu's**: `crate::chargen`'s `ListMenu` holds the
//!   `TraitSheet`, and every change goes through its `apply`, with its sounds and logs. This
//!   module never makes a sheet (the constructors use up what scripts asked for): it reads the
//!   open menu ([`Snapshot`]), writes the file's "set in code" traits, and says what a click
//!   stands for ([`TileAct`], queued for `crate::chargen`'s `input` later in the frame).
//! - **The words it writes are NVForever's**, not the hand-built menu's ([`own_counter`],
//!   [`own_text`]) [design: the string rule, LEGAL.md, chargen.md §4.1]: the file leaves the
//!   counter and the description to code, and no setting in the masters holds these lines.
//! - **Readiness** ([`prepare`], every frame, whatever was written last): the file built with
//!   every path, every row and its children live, and sizes that can be used. Otherwise the rows
//!   made so far go and the hand-built menu stands, its sheet as it was: nothing reopens, and the
//!   next frame tries again, with new rows. What is drawn is one decision ([`drawn`]) for the
//!   hand-built menu, the tiles and the blur: ready, for this very opening, in this very tree.
//! - **Three lifetimes**: an opening (`crate::chargen`'s generation) holds the scrolls and the
//!   highlight, and a fallback does not end it ([`TraitTiles::begin`]); a tree holds the counts
//!   the file gave before code wrote them ([`FileCounts`]), which a fallback must not take for
//!   the engine's; a readiness holds the rows.
//! - **A frame** ([`prepare`], before `super::input::feed`): the tree worked out, what changed
//!   written, the texts measured, the rows placed, the counts, scrolls and highlight written,
//!   the tree worked out again, as `super::container_menu`. The scrolls are written again after
//!   the input's step ([`TraitTiles::rewrite`]): the file's values count clicks from 0.
//! - **The hand-over**: `crate::chargen`'s `draw_menus` takes its menu away on the frame this
//!   is first ready, and the tile input starts on the next, when what stood on screen has gone
//!   ([`super::input`]), so a press acts once, on what the player saw.
//! - **Scrolls**: the list's and the description's are this module's own, no change of the
//!   sheet. The keys' choice brings its row into view (`navigated`), the pointer's does not,
//!   and a bar or the wheel leaves the choice alone. Another row starts its text at the top.
//! - **The chosen row and the highlight are apart**: the menu opens with row 0's picture and text
//!   and no highlight, and the pointer leaving the rows takes the highlight away and leaves the
//!   text [observed: the original's job `trait`, 1280 x 720, chargen.md §4.1]. The sheet's choice
//!   stands all the while, as the hand-built menu's.

#[cfg(test)]
use super::list;
use super::menus::Tiled;
#[cfg(test)]
use super::menus::SizesWritten;
use super::selection_menu::*;
pub use super::selection_menu::{
    follow_click, follow_pointer, RowShown, Scroll, SelectionTile as TraitTile, Snapshot,
    HAND_BUILT_KEYS,
};
use crate::chargen::lists::ListView;
#[cfg(test)]
use crate::chargen::TileAct;
use bevy::prelude::{Res, ResMut};
use fnv_game::chargen::TraitSheet;
use fnv_game::game::menus as codes;
#[cfg(test)]
use fnv_sim::FormId;
pub type TraitTiles = SelectionTiles<{ codes::TRAITS }>;
pub type TraitTileAid = SelectionAid<{ codes::TRAITS }>;
pub const AID: &str = "NVF_PLAY_TEST_TRAIT_TILES";
pub(super) fn aid_from_env() -> TraitTileAid {
    super::selection_menu::aid_from_env::<{ codes::TRAITS }>()
}
pub(super) use super::selection_menu::write_system as shared_write_system;
#[allow(clippy::too_many_arguments)]
pub(super) fn write_system(
    script: ResMut<crate::script::ScriptState>,
    switch: Res<super::vanilla::TileMenus>,
    chargen: Option<Res<crate::chargen::ChargenMenus>>,
    sim: Option<ResMut<crate::sim::SimState>>,
    paused: Option<Res<crate::pause::Paused>>,
    time: crate::pause::AidTime,
    fonts: super::draw::FontKit,
    state: (ResMut<TraitTiles>, ResMut<TraitTileAid>),
) {
    shared_write_system(script, switch, chargen, sim, paused, time, fonts, state);
}

#[cfg(test)]
use fnv_ui::Value;
use fnv_ui::{TileId, Ui};

/// The menu's name [documented: trait_menu.xml, chargen.md §4.1].
pub const TRAIT_MENU: &str = "TraitMenu";

pub(super) const MAIN: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect";
pub(super) const BACKGROUND: &str = "TraitMenu/NOGLOW_BRANCH/LUM_Background";
pub(super) const TITLE: &str =
    "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_HeadlineRect/LUM_Headline_Title";
pub(super) const LIST: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_PerkList";
pub(super) const ICON: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionIcon";
pub(super) const COUNTER: &str =
    "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionInfoRect/LUM_PointCounter";
pub(super) const TEXT_BOX: &str =
    "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionInfoRect/TM_DescriptionBox";
pub(super) const TEXT_BAR: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionInfoRect/TM_DescriptionBox/TM_DescriptionScrollbar";
pub(super) const TEXT: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionInfoRect/TM_DescriptionBox/TM_DescriptionText";
pub(super) const RESET: &str =
    "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_ButtonRect/LUM_ResetButton";
pub(super) const DONE: &str =
    "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_ButtonRect/LUM_ContinueButton";
pub(super) const TEMPLATE: &str = "LUM_PerkTemplate";
pub(super) const ROW: &str = "LUM_Perk";
pub(super) const ROW_TEXT: &str = "ListItemText";
pub(super) const ROW_MARK: &str = "LUM_Template_ItemMarker";

/// The dark picture's opacity [observed: `LUM_Background` `alpha` 193.80, the original's job
/// `trait`, 1280 x 720; the file's `alpha` copies the shared fill, as the container's].
pub const BACKGROUND_ALPHA: f64 = 193.8;
/// A row's `id` [observed: the active component reads 10 on each row, the original's job `trait`].
pub(super) const ROW_ID: f64 = 10.0;
/// A trait that cannot be taken: half strength, as the hand-built row [hypothesis: chargen.md §4;
/// the row's `_TextAlpha` allows it, no screenshot shows one].
pub(super) const FAINT_ALPHA: f64 = 128.0;

pub fn built(ui: &Ui) -> Tiled {
    super::selection_menu::built::<{ codes::TRAITS }>(ui)
}
pub fn mark_tiles(ui: &mut Ui) {
    super::selection_menu::mark_tiles::<{ codes::TRAITS }>(ui)
}
pub fn drawn(
    st: &TraitTiles,
    open: Option<crate::chargen::TraitsOpen<'_>>,
    game: Option<&fnv_game::Game<'_>>,
) -> Option<TileId> {
    let game = game.filter(|g| g.ui_files.is_some())?;
    drawn_by(
        st,
        open.map(|o| o.generation),
        game.menu,
        game.ui_tree.menu(TRAIT_MENU),
    )
}
#[cfg(test)]
fn over_text(ui: &Ui, at: (f64, f64)) -> bool {
    super::selection_menu::over_text::<{ codes::TRAITS }>(ui, at)
}

impl Snapshot {
    /// What the open menu shows: `view` is `ListMenu::view`, `sheet` its sheet, `facts`
    /// `ListMenu::trait_facts`. The title and the rows are the view's, read from the data; the
    /// counter and the text's lines are worded here.
    pub fn of(
        view: &ListView,
        sheet: &TraitSheet,
        facts: Option<(String, u8, String)>,
        generation: u64,
        navigated: u32,
    ) -> Snapshot {
        let rows: Vec<RowShown> = sheet
            .traits
            .iter()
            .zip(&view.rows)
            .map(|(&key, r)| RowShown {
                key,
                name: r.text.clone(),
                value: None,
                taken: r.marked,
                faint: r.dim,
            })
            .collect();
        Snapshot {
            generation,
            title: view.title.clone(),
            selected: Some(view.selected).filter(|&s| s < rows.len()),
            rows,
            icon: view.icon.clone(),
            info: facts.map_or_else(String::new, |(needs, ranks, description)| {
                own_text(&needs, ranks, &description)
            }),
            counter: own_counter(sheet.left()),
            done: view.done,
            taken: sheet.chosen.len(),
            most: sheet.max,
            only_add: false,
            navigated,
        }
    }
}

/// The counter, in NVForever's words: no singular to get wrong, and not the original's
/// [design: the string rule, LEGAL.md].
fn own_counter(left: usize) -> String {
    format!("{left} left to choose")
}

/// The description: what the trait needs, its most ranks, then the data's text, in NVForever's
/// words [design: the string rule, LEGAL.md].
fn own_text(needs: &str, ranks: u8, description: &str) -> String {
    format!(
        "Needs: {needs}\nMost ranks: {ranks}\n\n{}",
        description.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::super::menus::write_text_sizes_of;
    use super::*;
    use fnv_ui::input::{By, Click};
    use fnv_ui::MemSource;

    /// [design] A small layout written for these tests, with its own structure, numbers and
    /// expressions; it does not come from the game's files and says nothing about them. It keeps
    /// only the tile and trait names the model and input look up, which are the engine's interface
    /// ([`built`]), and plain literals wherever the arithmetic is not what a test is about. Its one
    /// dependency: the text stands 3 less its bar's `_moved`, the bar's step times its value. A
    /// bar's `_own_count` stands for a file's own count of clicks, for the model's writes to beat.
    /// So these tests say how the model drives the interface and give up the real file's geometry,
    /// fonts, wrapped rows, rows moving with the scroll, the highlight's and marks' showing, and
    /// its scroll counts, which the smoke scenarios `tiles-trait-*` read. With [`size`] every row
    /// is 40 high (one line of 30 and 10 of spacing), and the description 30 a line.
    fn menu_changed(change: impl FnOnce(&str) -> String) -> Ui {
        let bar = |name: &str, extra: &str| {
            format!(
                r#"<hotrect name="{name}"><width>10</width><target>&true;</target>{extra}
                  <_number_of_visible_items>1</_number_of_visible_items><_jump_size>0</_jump_size><_step_size>1</_step_size><_number_of_items>3</_number_of_items>
                  <_own_count>0</_own_count>
                  <_current_value><copy src="me()" trait="_own_count"/></_current_value>
                  <image name="scrollbar_vert_up"/><image name="scrollbar_vert_down"/>
                  <image name="scrollbar_vert_marker"><height>8</height><width>4</width><_step_height>5</_step_height></image>
                  <hotrect name="scrollbar_vert_page_up"/><hotrect name="scrollbar_vert_page_down"/></hotrect>"#
            )
        };
        let list_bar = bar("lb_scrollbar", "");
        let text_bar = bar("TM_DescriptionScrollbar", r#"<x>200</x><_ScrollDelta>25</_ScrollDelta><_moved><copy src="me()" trait="_ScrollDelta"/><mul src="me()" trait="_current_value"/></_moved>"#);
        let xml = format!(
            r#"<menu name="TraitMenu">
            <rect name="NOGLOW_BRANCH"><image name="LUM_Background"><alpha>7</alpha></image>
            <rect name="LUM_MainRect"><x>50</x><y>30</y>
              <rect name="LUM_HeadlineRect"><text name="LUM_Headline_Title"/></rect>
              <hotrect name="LUM_PerkList"><y>20</y><height>200</height><width>300</width><_scroll_delta>15</_scroll_delta>
                <hotrect name="lb_highlight_box"/>
                {list_bar}</hotrect>
              <image name="LUM_SelectionIcon"/>
              <rect name="LUM_SelectionInfoRect"><x>400</x><y>100</y>
                <hotrect name="TM_DescriptionBox"><y>10</y><height>100</height><width>200</width><clipwindow>&true;</clipwindow>
                  <_moved><add src="child(TM_DescriptionScrollbar)" trait="_moved"/></_moved>
                  {text_bar}
                  <text name="TM_DescriptionText"><wrapwidth>180</wrapwidth><y><copy>3</copy><sub src="parent()" trait="_moved"/></y></text>
                </hotrect>
                <text name="LUM_PointCounter"/></rect>
              <rect name="LUM_ButtonRect"><y>0</y><x>700</x>
                <hotrect name="LUM_ResetButton"><height>20</height><width>50</width><target>&true;</target></hotrect>
                <hotrect name="LUM_ContinueButton"><height>20</height><width>50</width><target>&true;</target><y>30</y></hotrect>
              </rect></rect></rect>
            <template name="LUM_PerkTemplate"><hotrect name="LUM_Perk"><width>280</width><x>10</x><target>&true;</target><_VerticalSpacing>10</_VerticalSpacing>
              <text name="ListItemText"><string>a row</string></text>
              <image name="LUM_Template_ItemMarker"/>
            </hotrect></template>
            <template name="SpareCounter"><text name="LUM_PointCounter"/></template></menu>"#
        );
        let mut ui = Ui::new();
        ui.load_menu(&MemSource::default().with("tm.xml", &change(&xml)), "tm.xml").unwrap();
        ui.update();
        ui
    }

    fn menu() -> Ui {
        menu_changed(str::to_string)
    }

    /// Text heights by the strings: as many lines as `|` splits.
    fn size(font: f64, text: &str, wrap: Option<f64>) -> (f64, f64) {
        list::lines_size(font, text, wrap)
    }

    fn snapshot(rows: usize, selected: usize, info: &str) -> Snapshot {
        Snapshot {
            generation: 1,
            title: "PICK".into(),
            rows: (0..rows as u32).map(|i| RowShown { key: FormId(100 + i), name: format!("Trait {i}"), value: None, taken: false, faint: false }).collect(),
            selected: Some(selected),
            icon: Some("x.dds".into()),
            info: info.into(),
            counter: "2 LEFT".into(),
            done: true,
            taken: 0,
            most: 2,
            only_add: false,
            navigated: 0,
        }
    }

    /// A frame as the writer does it.
    fn frame(ui: &mut Ui, s: &Snapshot, st: &mut TraitTiles) -> Tiled {
        let menu = ui.menu(TRAIT_MENU);
        let mut write_sizes = |ui: &mut Ui, sizes: &mut SizesWritten| match menu {
            Some(m) if ui.is_live(m) => {
                let texts = super::super::menus::texts(ui, m);
                write_text_sizes_of(ui, &texts, sizes, &size)
            }
            _ => 0,
        };
        let status = prepare(ui, Some(s.clone()), st, &mut write_sizes);
        if status.yes() {
            mark_tiles(ui);
        }
        status
    }

    /// `n` lines of description.
    fn lines(n: usize) -> String {
        vec!["line"; n].join("|")
    }

    fn click(tile: TileId, by: By) -> Click {
        Click { tile, id: None, by, stamp: 0 }
    }

    fn at(ui: &Ui, path: &str) -> f64 {
        ui.get_float(path).unwrap_or(-999.0)
    }

    #[test]
    fn readiness_needs_the_whole_file() {
        let ui = menu();
        assert_eq!(built(&ui), Tiled::Yes);
        assert!(!built(&Ui::new()).yes(), "the file missing");
        for gone in ["TM_DescriptionText", "LUM_PointCounter", "LUM_ResetButton", "scrollbar_vert_marker", "lb_highlight_box", "ListItemText", "LUM_Template_ItemMarker", "LUM_PerkTemplate"] {
            let cut = menu_changed(|xml| xml.replace(&format!(r#"name="{gone}""#), r#"name="Modded""#));
            assert!(!built(&cut).yes(), "{gone} cut");
        }
    }

    #[test]
    fn a_row_not_made_leaves_nothing() {
        // A template gone once the file passed: nothing stays made, and no menu is drawn.
        let mut ui = menu_changed(|xml| xml.replace("LUM_PerkTemplate", "Other"));
        let mut st = TraitTiles::default();
        assert!(!write(&mut ui, &snapshot(2, 0, "x"), &mut st));
        st.fail(&mut ui);
        assert!(st.menu.is_none() && st.rows.is_empty());
        assert!(!frame(&mut ui, &snapshot(2, 0, "x"), &mut st).yes());
        assert!(st.menu.is_none(), "the hand-built menu stays");
    }

    #[test]
    fn the_lines_are_worded_here_around_the_datas_facts() {
        assert_eq!(own_counter(2), "2 left to choose");
        assert_eq!(own_counter(1), "1 left to choose");
        assert_eq!(own_text("PER < 10", 1, "  What it does. "), "Needs: PER < 10
Most ranks: 1

What it does.");
    }

    /// The counter cut, as a mod might, and made again from the layout's spare.
    fn cut_counter(ui: &mut Ui) {
        let counter = ui.find_tile(COUNTER).unwrap();
        ui.unload_tile(counter);
    }

    fn restore_counter(ui: &mut Ui) {
        let info = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionInfoRect";
        assert!(ui.add_code_tile_from_template(&MemSource::default(), info, "SpareCounter", Some("LUM_PointCounter")).is_some());
    }

    #[test]
    fn recovery_comes_back_with_the_files_counts_and_the_openings_scrolls() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        // A long text raises the description's count past the layout's 3.
        assert!(frame(&mut ui, &snapshot(10, 0, &lines(10)), &mut st).yes());
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_number_of_items")), 10.0);
        st.scroll_to(Scroll::List, 2.0);
        // The file loses a tile: the rows go, and the hand-built menu draws.
        cut_counter(&mut ui);
        assert!(!frame(&mut ui, &snapshot(10, 0, "short"), &mut st).yes());
        assert!(st.menu.is_none() && st.rows.is_empty());
        // It comes back: tiles again, the same opening's scroll, and a short text back at the
        // file's count, not the 10 the engine wrote.
        restore_counter(&mut ui);
        assert_eq!(frame(&mut ui, &snapshot(10, 0, "short"), &mut st), Tiled::Yes);
        assert!(st.menu.is_some() && st.rows.len() == 10);
        assert_eq!(st.list.value, 2.0);
        assert_eq!((st.text.most, at(&ui, &format!("{TEXT_BAR}/_number_of_items"))), (2.0, 3.0));
    }

    #[test]
    fn a_new_opening_after_a_fallback_starts_over() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let mut s = snapshot(10, 0, &lines(10));
        assert!(frame(&mut ui, &s, &mut st).yes());
        s.navigated = 1;
        s.selected = Some(8);
        assert!(frame(&mut ui, &s, &mut st).yes());
        st.scroll_text(4.0);
        assert!(st.list.highlight && st.list.value > 0.0);
        cut_counter(&mut ui);
        assert!(!frame(&mut ui, &s, &mut st).yes());
        assert!(st.has_state(), "the fallback ended no opening");
        // The menu opens again, the tree whole.
        restore_counter(&mut ui);
        let next = Snapshot { generation: 2, navigated: 1, selected: Some(0), ..snapshot(10, 0, &lines(10)) };
        assert!(frame(&mut ui, &next, &mut st).yes());
        assert_eq!((st.list.value, st.text.value, st.list.highlight), (0.0, 0.0, false));
        assert_eq!(at(&ui, &format!("{LIST}/_highlight_y")), -1.0, "no highlight as it opens");
        // Closed with the drawing fallen back: closing still clears it.
        cut_counter(&mut ui);
        assert!(!frame(&mut ui, &next, &mut st).yes());
        st.close();
        assert!(!st.has_state());
    }

    #[test]
    fn a_rows_child_going_is_caught_with_the_sheet_unchanged_and_made_again() {
        for child in [ROW_TEXT, ROW_MARK] {
            let mut ui = menu();
            let mut st = TraitTiles::default();
            let s = snapshot(3, 0, "x");
            assert!(frame(&mut ui, &s, &mut st).yes());
            let gone = list::child(&ui, st.rows[1].tile, child).unwrap();
            ui.unload_tile(gone);
            // The same snapshot, so nothing is written: readiness reads the tree.
            assert!(!frame(&mut ui, &s, &mut st).yes(), "{child}");
            assert!(st.menu.is_none());
            assert!(frame(&mut ui, &s, &mut st).yes(), "{child}: the next frame makes the rows again");
            assert_eq!(st.rows.len(), 3);
        }
        // A row gone whole.
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let s = snapshot(3, 0, "x");
        assert!(frame(&mut ui, &s, &mut st).yes());
        ui.unload_tile(st.rows[0].tile);
        assert!(!frame(&mut ui, &s, &mut st).yes());
    }

    #[test]
    fn no_room_means_the_hand_built_menu() {
        for (from, to) in [
            ("<height>200</height><width>300</width>", "<height>0</height><width>300</width>"),
            ("<height>200</height><width>300</width>", "<height>200</height><width>0</width>"),
            ("<y>10</y><height>100</height><width>200</width>", "<y>10</y><height>0</height><width>200</width>"),
            ("<y>10</y><height>100</height><width>200</width>", "<y>10</y><height>100</height><width>0</width>"),
            ("<_ScrollDelta>25</_ScrollDelta>", "<_ScrollDelta>0</_ScrollDelta>"),
        ] {
            let mut ui = menu_changed(|xml| {
                assert!(xml.contains(from), "{from}");
                xml.replace(from, to)
            });
            let mut st = TraitTiles::default();
            assert!(!frame(&mut ui, &snapshot(3, 0, "x"), &mut st).yes(), "{to}");
            assert!(st.menu.is_none() && st.rows.is_empty(), "{to}");
        }
        // Nothing measured: a row's spacing alone is no measure, an empty text may be.
        assert!(!room(f64::NAN) && !room(f64::INFINITY) && !room(0.0) && room(1.0));
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let none = |_: f64, _: &str, _: Option<f64>| (0.0, 0.0);
        let menu_tile = ui.menu(TRAIT_MENU).unwrap();
        let mut write_sizes = |ui: &mut Ui, sizes: &mut SizesWritten| {
            let texts = super::super::menus::texts(ui, menu_tile);
            write_text_sizes_of(ui, &texts, sizes, &none)
        };
        assert!(!prepare(&mut ui, Some(snapshot(3, 0, "x")), &mut st, &mut write_sizes).yes());
    }

    #[test]
    fn the_one_decision_follows_the_menu_open_now() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TRAITS), None), None, "nothing ready");
        st.begin(1);
        assert!(frame(&mut ui, &snapshot(3, 0, "x"), &mut st).yes());
        let m = ui.menu(TRAIT_MENU);
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TRAITS), m), m);
        // Scripts put the tag-skill menu in its place: st.menu still holds this frame's readiness.
        assert_eq!(drawn_by(&st, None, Some(codes::TAG_SKILLS), m), None, "the hand-built side has moved on");
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TAG_SKILLS), m), None, "the game has, before that side saw it");
        assert_eq!(drawn_by(&st, None, Some(codes::TRAITS), m), None, "the sheet is gone");
        // Another opening of the same menu, or a tree built again, wait for their preparation.
        assert_eq!(drawn_by(&st, Some(2), Some(codes::TRAITS), m), None);
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TRAITS), None), None);
        // Readiness gone.
        cut_counter(&mut ui);
        assert!(!frame(&mut ui, &snapshot(3, 0, "x"), &mut st).yes());
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TRAITS), ui.menu(TRAIT_MENU)), None);
    }

    #[test]
    fn a_cut_file_gives_the_menu_back_and_comes_again_when_whole() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let s = snapshot(3, 0, "x");
        assert_eq!(frame(&mut ui, &s, &mut st), Tiled::Yes);
        assert_eq!(st.takes(), ui.menu(TRAIT_MENU));
        let rows: Vec<TileId> = st.rows.iter().map(|r| r.tile).collect();
        st.act(TileAct::Reset);
        let counter = ui.find_tile(COUNTER).unwrap();
        ui.unload_tile(counter);
        assert!(!frame(&mut ui, &s, &mut st).yes());
        assert!(st.menu.is_none() && st.takes().is_none());
        assert!(rows.iter().all(|&r| !ui.is_live(r)), "the rows made go");
        assert!(st.take_acts(1).is_empty(), "nothing queued acts");
        assert!(st.clicked(&ui, ui.menu(TRAIT_MENU).unwrap()).is_none());
        restore_counter(&mut ui);
        assert_eq!(frame(&mut ui, &s, &mut st), Tiled::Yes);
        assert_eq!(st.takes(), ui.menu(TRAIT_MENU), "whole again, it takes input");
        assert!(rows.iter().all(|r| !st.rows.iter().any(|now| now.tile == *r)), "with new rows");
    }

    #[test]
    fn rows_write_taken_and_faint_apart_from_the_highlight() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let mut s = snapshot(3, 1, "x");
        s.rows[0].taken = true;
        s.rows[2].faint = true;
        assert!(frame(&mut ui, &s, &mut st).yes());
        let row = |ui: &Ui, i: usize, name: &str| ui.tile(st.rows[i].tile).get(name).map_or(-1.0, Value::num);
        assert_eq!((row(&ui, 0, "_selected"), row(&ui, 1, "_selected")), (1.0, 0.0), "taken, not chosen");
        assert_eq!((row(&ui, 2, "_TextAlpha"), row(&ui, 0, "_TextAlpha")), (FAINT_ALPHA, 255.0));
        assert_eq!(at(&ui, &format!("{LIST}/_highlight_y")), -1.0, "no highlight as the menu opens");
        assert_eq!(ui.get_string(&format!("{TEXT}/string")).as_deref(), Some("x"), "though the text shows");
        assert_eq!(ui.tile(st.rows[0].tile).name, ROW);
        assert_eq!(ui.tile(st.rows[0].tile).get("id").map(Value::num), Some(ROW_ID));
        st.list.highlight = true;
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(at(&ui, &format!("{LIST}/_highlight_y")), 40.0, "the chosen row, the second");
        assert_eq!(at(&ui, &format!("{LIST}/_selected_height")), 40.0);
        assert_eq!(at(&ui, &format!("{TRAIT_MENU}/_MaxPoints")), 2.0);
        assert_eq!(ui.get_string(&format!("{TEXT}/string")).as_deref(), Some("x"));
        // A row keeps its tile through a later change, so a click queued means what it did.
        let tile = st.rows[1].tile;
        s.rows[1].taken = true;
        s.taken = 2;
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.rows[1].tile, tile);
    }

    #[test]
    fn the_keys_reveal_their_row_and_the_bar_is_left_where_it_was_put() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        // Ten rows of 40 in a list of 200: five whole, five positions after the first, 40 each.
        let mut s = snapshot(10, 0, "x");
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.list.most, 5.0);
        assert_eq!(st.list.scale, 40.0);
        assert_eq!(st.page_rows(), 5);
        // The pointer chooses a row out of view: nothing moves.
        s.selected = Some(8);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.list.value, 0.0);
        // The keys choose: it comes into view, whole.
        s.navigated = 1;
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.list.value, 4.0, "(320 + 40 - 200) / 40, up");
        let (y, h) = (at(&ui, &format!("{LIST}/_highlight_y")), at(&ui, &format!("{LIST}/_selected_height")));
        assert!(y >= 0.0 && y + h <= 200.0, "{y} {h}");
        // The bar moves the list and the next frame leaves it there.
        st.scroll_to(Scroll::List, 3.0);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.list.value, 3.0);
        assert_eq!(at(&ui, &format!("{LIST}/lb_scrollbar/_current_value")), 3.0);
    }

    #[test]
    fn a_long_description_reaches_its_end_and_another_starts_at_the_top() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        // Ten lines of 30 from 3 down in a box of 100: 203 over, 9 values of 25.
        let mut s = snapshot(3, 0, &lines(10));
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.text.most, 9.0);
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_number_of_items")), 10.0);
        st.scroll_text(100.0);
        st.rewrite(&mut ui);
        ui.update();
        assert_eq!(st.text.value, 9.0, "held at the end");
        let end = at(&ui, &format!("{TEXT}/y")) + at(&ui, &format!("{TEXT}/height"));
        assert!(end <= 100.0, "the last line in the box, at {end}");
        // The same row with a shorter text holds the value in the new range.
        s.info = lines(5);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!((st.text.most, st.text.value), (3.0, 3.0));
        // Another row: from the top.
        s.selected = Some(1);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.text.value, 0.0);
        assert_eq!(at(&ui, &format!("{TEXT}/y")), 3.0);
    }

    #[test]
    fn a_short_description_keeps_the_files_count_and_a_longer_one_goes_past_it() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert!(frame(&mut ui, &snapshot(3, 0, &lines(2)), &mut st).yes());
        assert_eq!(st.text.most, 2.0, "the layout's 3 items, whatever the text");
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_number_of_items")), 3.0);
        assert!(frame(&mut ui, &snapshot(3, 0, &lines(10)), &mut st).yes());
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_number_of_items")), 10.0);
        assert!(frame(&mut ui, &snapshot(3, 0, &lines(2)), &mut st).yes());
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_number_of_items")), 3.0, "and back");
    }

    #[test]
    fn off_the_rows_the_highlight_goes_and_the_text_stays() {
        use fnv_ui::input::Report;
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let mut s = snapshot(3, 0, "x");
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert!(!st.list.highlight, "none as the menu opens");
        let (first, second, reset) = (st.rows[0].tile, st.rows[1].tile, ui.find_tile(RESET).unwrap());
        let onto = |to| Report { over: Some((None, to)), ..Report::default() };
        // Onto the chosen row: no act, the highlight shows.
        follow_pointer(&mut st, &ui, &onto(Some(first)), None, true);
        assert!(st.list.highlight && st.take_acts(1).is_empty());
        // Onto another: chosen, once.
        follow_pointer(&mut st, &ui, &onto(Some(second)), None, true);
        assert_eq!(st.take_acts(1), [TileAct::Hover(st.rows[1].key)]);
        // A row moved under a resting pointer chooses nothing.
        follow_pointer(&mut st, &ui, &onto(Some(second)), None, false);
        assert!(st.take_acts(1).is_empty());
        // Onto a button, and off everything: the highlight goes.
        follow_pointer(&mut st, &ui, &onto(Some(reset)), None, true);
        assert!(!st.list.highlight);
        assert_eq!(st.take_acts(1), [TileAct::ButtonFocus]);
        st.list.highlight = true;
        follow_pointer(&mut st, &ui, &onto(None), None, true);
        assert!(!st.list.highlight);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(at(&ui, &format!("{LIST}/_highlight_y")), -1.0);
        assert_eq!(at(&ui, &format!("{LIST}/_selected_height")), 0.0);
        assert_eq!(ui.get_string(&format!("{TEXT}/string")).as_deref(), Some("x"));
        // The keys bring it back.
        s.navigated = 1;
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert!(st.list.highlight);
        assert_eq!(at(&ui, &format!("{LIST}/_highlight_y")), 0.0);
    }

    #[test]
    fn the_bar_beside_its_window_is_not_cropped() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert!(frame(&mut ui, &snapshot(1, 0, &lines(10)), &mut st).yes());
        let menu = ui.menu(TRAIT_MENU).unwrap();
        let down = ui.find_tile(&format!("{TEXT_BAR}/scrollbar_vert_down")).unwrap();
        let items = super::super::layout::layout(&ui, menu, &|_, _, _| (0.0, 0.0), true);
        assert_eq!(items.iter().find(|i| i.tile == down).unwrap().clip, None, "the bar stands clear of its window");
        let text = ui.find_tile(TEXT).unwrap();
        assert!(items.iter().find(|i| i.tile == text).unwrap().clip.is_some(), "the text is cropped");
    }

    #[test]
    fn a_click_acts_once_and_only_as_its_input_may() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert!(frame(&mut ui, &snapshot(3, 0, "x"), &mut st).yes());
        let (reset, done, row) = (ui.find_tile(RESET).unwrap(), ui.find_tile(DONE).unwrap(), st.rows[2].tile);
        let key = st.rows[2].key;
        // The pointer and a script: the same act.
        assert!(follow_click(&mut st, &ui, &click(row, By::Pointer), None, false, &[]).is_some());
        assert!(follow_click(&mut st, &ui, &click(done, By::Script), None, false, &[]).is_some());
        assert_eq!(st.take_acts(1), [TileAct::Click(key), TileAct::Done]);
        // A key the hand-built menu took: the file's click acts on nothing.
        assert!(follow_click(&mut st, &ui, &click(reset, By::Key("R".into())), None, false, &["R"]).is_some());
        assert!(st.take_acts(1).is_empty());
        // The controller standing in for R, and one that is not.
        assert!(follow_click(&mut st, &ui, &click(reset, By::Key("xbuttony".into())), None, false, &["R", "xbuttony"]).is_some());
        assert!(st.take_acts(1).is_empty());
        assert!(follow_click(&mut st, &ui, &click(reset, By::Key("xbuttony".into())), None, false, &[]).is_some());
        // A script's MenuTapKey.
        assert!(follow_click(&mut st, &ui, &click(reset, By::Key("R".into())), None, false, &[]).is_some());
        assert_eq!(st.take_acts(1), [TileAct::Reset, TileAct::Reset]);
        // Acts of another opening are dropped.
        st.act(TileAct::Done);
        assert!(st.take_acts(2).is_empty());
    }

    #[test]
    fn a_closing_menu_takes_nothing_and_a_gone_row_acts_on_nothing() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert!(frame(&mut ui, &snapshot(2, 0, "x"), &mut st).yes());
        let gone = st.rows[1].tile;
        assert!(frame(&mut ui, &snapshot(1, 0, "x"), &mut st).yes());
        assert!(!ui.is_live(gone));
        assert!(follow_click(&mut st, &ui, &click(gone, By::Pointer), None, false, &[]).is_none(), "a tile of a row gone");
        st.held = true;
        let row = st.rows[0].tile;
        assert_eq!(st.takes(), None);
        assert!(st.menu.is_some(), "still drawn");
        assert!(follow_click(&mut st, &ui, &click(row, By::Pointer), None, false, &[]).is_none());
    }

    #[test]
    fn the_bars_pieces_move_their_own_scroll() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert!(frame(&mut ui, &snapshot(10, 0, &lines(10)), &mut st).yes());
        let piece = |ui: &Ui, bar: &str, p: &str| ui.find_tile(&format!("{bar}/{p}")).unwrap();
        let down = piece(&ui, &format!("{LIST}/{}", list::SCROLLBAR), list::DOWN);
        assert!(follow_click(&mut st, &ui, &click(down, By::Pointer), None, false, &[]).is_some());
        assert_eq!((st.list.value, st.text.value), (1.0, 0.0));
        let down = piece(&ui, TEXT_BAR, list::DOWN);
        assert!(follow_click(&mut st, &ui, &click(down, By::Pointer), None, false, &[]).is_some());
        assert_eq!((st.list.value, st.text.value), (1.0, 1.0));
        // The layout's page step is nothing: its page area moves nothing.
        let page = piece(&ui, TEXT_BAR, list::PAGE_DOWN);
        assert!(follow_click(&mut st, &ui, &click(page, By::Pointer), None, false, &[]).is_some());
        assert_eq!(st.text.value, 1.0);
        st.scroll_text(3.0);
        assert_eq!(st.text.value, 4.0);
        // A value written after the click's pulse stays the view's.
        st.rewrite(&mut ui);
        ui.update();
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_current_value")), 4.0);
        let bar = ui.find_tile(TEXT_BAR).unwrap();
        ui.set_trait(bar, "_own_count", Value::Num(1.0));
        st.rewrite(&mut ui);
        ui.update();
        ui.set_trait(bar, "_own_count", Value::Num(0.0));
        st.rewrite(&mut ui);
        ui.update();
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_current_value")), 4.0);
        // The wheel goes by what is under the pointer.
        st.over_text = true;
        st.wheel(1.0);
        assert_eq!((st.list.value, st.text.value), (1.0, 5.0));
        st.over_text = false;
        st.wheel(1.0);
        assert_eq!((st.list.value, st.text.value), (2.0, 5.0));
        st.wheel(-100.0);
        assert_eq!(st.list.value, 0.0, "held at the top");
    }

    #[test]
    fn a_new_opening_starts_over_and_the_warnings_stay() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let s = snapshot(10, 0, &lines(10));
        assert!(frame(&mut ui, &s, &mut st).yes());
        st.scroll_to(Scroll::List, 5.0);
        st.scroll_text(4.0);
        let mut next = s.clone();
        next.generation = 2;
        assert!(frame(&mut ui, &next, &mut st).yes());
        assert_eq!((st.list.value, st.text.value), (0.0, 0.0));
        st.warned.insert("x".into());
        st.close();
        assert!(!st.has_state() && st.warned.contains("x"));
    }

    #[test]
    fn the_pointer_over_the_description_is_worked_out_from_the_tree() {
        let ui = menu();
        // The main rect at 50, 30, the info rect at 400, 100, the box 10 down, 200 x 100, its bar
        // 10 wide beside it.
        assert!(over_text(&ui, (451.0, 141.0)));
        assert!(over_text(&ui, (655.0, 239.0)), "the bar's width counts");
        assert!(!over_text(&ui, (449.0, 141.0)));
        assert!(!over_text(&ui, (451.0, 241.0)));
    }

    #[test]
    fn text_most_reaches_the_end() {
        assert_eq!(text_most(3.0, 300.0, 100.0, 25.0, 0.0), 9.0);
        assert_eq!(text_most(0.0, 100.0, 100.0, 25.0, 0.0), 0.0);
        assert_eq!(text_most(0.0, 101.0, 100.0, 25.0, 0.0), 1.0);
        assert_eq!(text_most(0.0, 900.0, 100.0, 0.0, 2.0), 2.0, "no step: the bar's own");
        assert_eq!(text_most(0.0, 90.0, 100.0, 25.0, 4.0), 4.0, "a text that fits: the bar's own");
        assert_eq!(text_most(0.0, 150.0, 100.0, 25.0, 4.0), 4.0, "its own reaches the end");
        assert_eq!(text_most(0.0, 300.0, 100.0, 25.0, 4.0), 8.0, "past its own only to the end");
    }

    #[test]
    fn the_aid_reads() {
        assert_eq!(parse_aid("1:missing,2:long,3:cut:LUM_PointCounter").unwrap(), vec![(1.0, AidStep::Missing), (2.0, AidStep::Long), (3.0, AidStep::Cut("LUM_PointCounter".into()))]);
        assert!(parse_aid("1:broken").is_err());
    }
}
