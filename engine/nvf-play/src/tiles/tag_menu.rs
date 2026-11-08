//! The tag-skill page of menu 1048, opt-in [design: chargen.md §3.1].
//! Reads the already-open ListMenu, never constructs a sheet. Shared readiness, three
//! lifetimes, frame order, hand-over and list input are selection_menu's (trait_menu notes).
//! Attribute sheets stay hand-built. Uncaptured tag behavior follows TraitMenu [hypothesis].

use super::selection_menu::{self, *};
use crate::chargen::lists::{ListMenu, ListView, Sheet};
use fnv_game::chargen::TagSheet;
use fnv_game::game::menus as codes;
use fnv_sim::FormId;
use fnv_ui::{TileId, Ui, Value};

pub const TAG_MENU: &str = "CharGenMenu";
/// [design] One wheel notch / Left/Right press moves 20 interface units, following
/// the trait scroll step [hypothesis: chargen.md §3.1; tag capture pending].
pub(super) const TEXT_STEP: f64 = 20.0;
pub const AID: &str = "NVF_PLAY_TEST_TAG_TILES";
pub type TagTiles = SelectionTiles<{ codes::TAG_SKILLS }>;
pub type TagTileAid = SelectionAid<{ codes::TAG_SKILLS }>;

// [documented: char_gen_menu.xml tile names; chargen.md §3]. No file expressions here.
pub(super) const MAIN: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_MainRect";
pub(super) const BACKGROUND: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_Background";
pub(super) const TITLE: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_MainRect/CGM_HeadlineRect/CGM_Headline_Title";
pub(super) const LIST: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_MainRect/CGM_ItemList";
pub(super) const ICON: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_MainRect/CGM_SelectionIcon";
pub(super) const COUNTER: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_MainRect/CGM_SelectionInfoRect/CGM_PointCounter";
pub(super) const TEXT_BOX: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_MainRect/CGM_SelectionInfoRect";
pub(super) const TEXT: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_MainRect/CGM_SelectionInfoRect/CGM_SelectionText";
pub(super) const DIVIDER: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_MainRect/CGM_SelectionInfoRect/CGM_DividerB";
pub(super) const RESET: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_MainRect/CGM_ButtonRect/CGM_ResetButton";
pub(super) const DONE: &str = "CharGenMenu/NOGLOW_BRANCH/CGM_MainRect/CGM_ButtonRect/CGM_DoneButton";
pub(super) const ROW_VALUE: &str = "CGM_Template_ItemValue";

pub(super) fn paths() -> Paths {
    Paths {
        TRAIT_MENU: TAG_MENU,
        MAIN,
        BACKGROUND,
        TITLE,
        LIST,
        ICON,
        COUNTER,
        TEXT_BOX,
        TEXT_BAR: "",
        TEXT,
        RESET,
        DONE,
        TEMPLATE: "CGM_SelectItemTemplate",
        ROW: "CGM_SelectItem",
        ROW_TEXT: "ListItemText",
        ROW_MARK: "CGM_Template_ItemMarker",
    }
}

pub(super) fn aid_from_env() -> TagTileAid {
    selection_menu::aid_from_env::<{ codes::TAG_SKILLS }>()
}

pub fn drawn(st: &TagTiles, open: Option<crate::chargen::TraitsOpen<'_>>, game: Option<&fnv_game::Game<'_>>) -> Option<TileId> {
    let game = game.filter(|g| g.ui_files.is_some())?;
    drawn_by(st, open.map(|o| o.generation), game.menu, game.ui_tree.menu(TAG_MENU))
}

/// [design: LEGAL.md, fnv_ui::strings] Neither tag title nor counter has a named setting
/// in that table. Use our own words, including the sheet's variable count.
fn own_title(taken: usize, count: usize) -> String {
    format!("Choose skills: {taken}/{count}")
}
/// Nothing with none left: the counter's tile is empty and hidden at 3/3 [observed: the
/// original's job ov_tags, chargen.md §3.1].
fn own_counter(left: usize) -> String {
    if left == 0 {
        return String::new();
    }
    format!("{left} left to choose")
}

pub(super) fn snapshot(view: &ListView, sheet: &TagSheet, generation: u64, navigated: u32) -> Snapshot {
    let rows: Vec<RowShown> = sheet
        .skills
        .iter()
        .zip(&view.rows)
        .map(|(&skill, r)| RowShown { key: FormId(u32::from(skill)), name: r.text.clone(), value: r.value.clone(), taken: r.marked, faint: r.dim })
        .collect();
    Snapshot {
        generation,
        title: own_title(sheet.tagged.len(), sheet.count),
        selected: Some(view.selected).filter(|&i| i < rows.len()),
        rows,
        icon: view.icon.clone(),
        info: view.info.clone(),
        counter: own_counter(sheet.remaining()),
        done: sheet.complete(),
        taken: sheet.tagged.len(),
        most: sheet.count,
        only_add: !sheet.locked.is_empty(),
        navigated,
    }
}

/// [design: lists.rs::open] Menu 1048 also hosts SpecialSheet. Never tile that page.
pub(super) fn sheet(menu: &ListMenu) -> Option<&TagSheet> {
    match &menu.sheet {
        Sheet::Tags { now, .. } if menu.code == codes::TAG_SKILLS => Some(now),
        _ => None,
    }
}

/// [design] A window ending at the lower divider, with the counter and the file's
/// decorations outside its crop. No scrollbar is present in this file [documented: §3.1].
/// Left/Right and the wheel move measured text; no new visual control or wording is added.
pub(super) fn settle_text<const CODE: u16>(ui: &mut Ui, st: &mut SelectionTiles<CODE>) {
    let page = num(ui, DIVIDER, "y");
    set(ui, TEXT_BOX, "height", Value::Num(page));
    set(ui, TEXT_BOX, "clips", flag(true));
    if let Some(info) = ui.find_tile(TEXT_BOX) {
        let text = ui.find_tile(TEXT);
        let children = ui.tile(info).children.clone();
        for child in children {
            if Some(child) != text {
                ui.set_trait(child, fnv_ui::input::OUTSIDE_CLIP, flag(true));
            }
        }
    }
    // [hypothesis: follows TraitMenu] The file's initial top belongs to the tree, not
    // a fallback or a readiness. The shared state remembers it before any scroll write.
    let top = st.file.map_or(0.0, |f| f.text_top);
    st.text.most = (top + num(ui, TEXT, "height") - page).max(0.0);
    st.text.value = st.text.value.clamp(0.0, st.text.most);
}

pub(super) fn rewrite_text<const CODE: u16>(ui: &mut Ui, st: &SelectionTiles<CODE>) {
    let top = st.file.map_or(0.0, |f| f.text_top);
    set(ui, TEXT, "y", Value::Num(top - st.text.value));
}

#[cfg(test)]
mod tests {
    use super::super::{
        list,
        menus::{self, Tiled},
    };
    use super::*;
    use crate::chargen::{lists::RowView, TileAct};
    use fnv_ui::{
        input::{By, Click},
        MemSource,
    };

    // [design] Authored for these tests: deliberately different geometry and literal
    // controls, no game file or its expressions. The real layout belongs to smoke.
    fn fixture(change: impl FnOnce(String) -> String) -> Ui {
        let pieces =
            [list::MARKER, list::UP, list::DOWN, list::PAGE_UP, list::PAGE_DOWN].map(|n| format!("<image name=\"{n}\"><width>7</width><height>9</height></image>")).join("");
        let xml = format!(
            r#"<menu name="CharGenMenu"><rect name="NOGLOW_BRANCH">
          <image name="CGM_Background"/><rect name="CGM_MainRect">
            <rect name="CGM_HeadlineRect"><text name="CGM_Headline_Title"/></rect>
            <hotrect name="CGM_ItemList"><width>230</width><height>100</height><_scroll_delta>40</_scroll_delta>
              <hotrect name="lb_highlight_box"/><image name="lb_scrollbar"><_number_of_items>1</_number_of_items>
                <_number_of_visible_items>1</_number_of_visible_items><_step_size>1</_step_size><_jump_size>2</_jump_size>{pieces}</image></hotrect>
            <image name="CGM_SelectionIcon"/>
            <rect name="CGM_SelectionInfoRect"><x>300</x><width>240</width>
              <text name="CGM_SelectionText"><y>25</y><wrapwidth>200</wrapwidth></text>
              <image name="CGM_DividerB"><y>125</y><width>240</width><height>2</height></image>
              <text name="CGM_PointCounter"><y>150</y></text></rect>
            <rect name="CGM_ButtonRect"><hotrect name="CGM_ResetButton"/><hotrect name="CGM_DoneButton"/></rect>
          </rect></rect>
          <template name="CGM_SelectItemTemplate"><hotrect name="CGM_SelectItem"><width>220</width><_VerticalSpacing>10</_VerticalSpacing>
            <text name="ListItemText"><string>row</string></text><text name="CGM_Template_ItemValue"/>
            <image name="CGM_Template_ItemMarker"/></hotrect></template></menu>"#
        );
        let mut ui = Ui::new();
        ui.load_menu(&MemSource::default().with("test.xml", &change(xml)), "test.xml").unwrap();
        ui.update();
        ui
    }
    fn menu() -> Ui {
        fixture(|s| s)
    }
    fn tags(count: usize) -> TagSheet {
        TagSheet { skills: vec![32, 33, 34, 35, 36], tagged: Default::default(), locked: Default::default(), count }
    }
    fn view(sheet: &TagSheet, info: &str) -> ListView {
        ListView {
            code: codes::TAG_SKILLS,
            labels: Default::default(),
            attributes: false,
            title: "unused title".into(),
            rows: sheet
                .skills
                .iter()
                .map(|&s| RowView { text: format!("Skill {s}"), value: Some("17".into()), marked: sheet.tagged.contains(&s), dim: false, plus: false, minus: false })
                .collect(),
            selected: 0,
            scroll: 0,
            icon: Some("test.dds".into()),
            info: info.into(),
            counter: "unused counter".into(),
            done: sheet.complete(),
            hover: None,
        }
    }
    fn shown(info: &str) -> Snapshot {
        let sheet = tags(3);
        snapshot(&view(&sheet, info), &sheet, 1, 0)
    }
    fn frame(ui: &mut Ui, s: &Snapshot, st: &mut TagTiles) -> Tiled {
        let root = ui.menu(TAG_MENU).unwrap();
        let mut measure = |ui: &mut Ui, sizes: &mut menus::SizesWritten| menus::write_text_sizes_of(ui, &menus::texts(ui, root), sizes, &list::lines_size);
        let result = prepare(ui, Some(s.clone()), st, &mut measure);
        if result.yes() {
            selection_menu::mark_tiles::<{ codes::TAG_SKILLS }>(ui);
        }
        result
    }

    #[test]
    fn snapshot_uses_sheet_count_and_our_words_and_data() {
        for count in [0, 1, 3, 4, 5, 13] {
            let mut sheet = tags(count);
            if count > 0 {
                sheet.tagged.insert(32);
            }
            let view = view(&sheet, "A description from AVIF.");
            let s = snapshot(&view, &sheet, 7, 4);
            assert_eq!(s.title, format!("Choose skills: {}/{count}", sheet.tagged.len()));
            let left = sheet.remaining();
            assert_eq!(s.counter, if left == 0 { String::new() } else { format!("{left} left to choose") });
            assert_eq!(s.done, sheet.complete());
            assert_eq!((s.most, s.taken, s.generation, s.navigated), (count, sheet.tagged.len(), 7, 4));
            assert_eq!(s.info, view.info);
            assert_eq!(s.rows[0].value.as_deref(), Some("17"));
        }
    }

    #[test]
    fn add_tag_skills_keeps_locks_limit_and_reset_in_the_open_sheet() {
        let mut start = tags(4);
        start.tagged.extend([32, 33, 34]);
        start.locked = start.tagged.clone();
        let mut m = ListMenu { code: codes::TAG_SKILLS, sheet: Sheet::Tags { now: start.clone(), start: start.clone() }, selected: 0, scroll: 0, hover: None };
        assert!(!m.toggle(), "an initial tag stays locked");
        m.select(3);
        assert!(m.toggle() && m.can_finish());
        m.select(4);
        assert!(!m.toggle(), "the fourth is the limit");
        let now = sheet(&m).unwrap();
        let s = snapshot(&view(now, "text"), now, 1, 0);
        assert!(s.only_add && s.done);
        assert_eq!((s.taken, s.most), (4, 4));
        m.select(3);
        assert!(m.toggle() && !m.can_finish());
        m.reset();
        assert_eq!(sheet(&m), Some(&start));
    }

    #[test]
    fn the_attributes_page_is_never_a_tag_snapshot() {
        let special = fnv_game::chargen::SpecialSheet::new([5; 7]);
        let mut m = ListMenu { code: codes::TAG_SKILLS, sheet: Sheet::Special { now: special, start: special }, selected: 0, scroll: 0, hover: None };
        assert!(m.attributes());
        assert!(sheet(&m).is_none());
        assert!(m.tag_row(32).is_none());
        m.code = codes::TRAITS;
        assert!(sheet(&m).is_none());
    }

    #[test]
    fn writes_marks_values_and_variable_points_and_done() {
        let mut ui = menu();
        let mut st = TagTiles::default();
        let mut s = shown("description");
        s.rows[0].taken = true;
        s.rows[0].value = Some("42".into());
        s.taken = 3;
        s.most = 4;
        s.only_add = true;
        s.done = false;
        assert!(frame(&mut ui, &s, &mut st).yes());
        let value = list::child(&ui, st.rows[0].tile, ROW_VALUE).unwrap();
        assert_eq!(ui.tile(value).get("string").map(Value::text).as_deref(), Some("42"));
        assert_eq!(num(&ui, TAG_MENU, "_CurrPoints"), 3.0);
        assert_eq!(num(&ui, TAG_MENU, "_MaxPoints"), 4.0);
        assert_eq!(num(&ui, TAG_MENU, "_OnlyAdd"), 1.0);
        assert_eq!(num(&ui, DONE, "target"), 0.0);
        assert_eq!(num(&ui, LIST, "y"), 65.0);
        assert_eq!(ui.tile(st.rows[0].tile).get("_selected").map(Value::num), Some(1.0));
        s.taken = 4;
        s.done = true;
        s.rows[0].value = Some("57".into());
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(num(&ui, DONE, "target"), 1.0);
        assert_eq!(ui.tile(value).get("string").map(Value::text).as_deref(), Some("57"));
    }

    #[test]
    fn readiness_requires_paths_template_value_live_children_and_room() {
        assert!(!selection_menu::built::<{ codes::TAG_SKILLS }>(&Ui::new()).yes());
        for name in ["CGM_PointCounter", "CGM_SelectionText", "CGM_DividerB", ROW_VALUE] {
            let ui = fixture(|s| s.replace(name, "absent"));
            assert!(!selection_menu::built::<{ codes::TAG_SKILLS }>(&ui).yes(), "{name}");
        }
        let mut ui = menu();
        let mut st = TagTiles::default();
        let s = shown("text");
        assert!(frame(&mut ui, &s, &mut st).yes());
        let child = list::child(&ui, st.rows[0].tile, ROW_VALUE).unwrap();
        ui.unload_tile(child);
        assert!(!frame(&mut ui, &s, &mut st).yes());
        assert!(st.menu.is_none() && st.rows.is_empty());
        assert!(frame(&mut ui, &s, &mut st).yes(), "next readiness makes fresh rows");
        set(&mut ui, DIVIDER, "y", Value::Num(0.0));
        assert!(!frame(&mut ui, &s, &mut st).yes());
    }

    #[test]
    fn long_text_reaches_end_and_clips_only_text_and_resets_on_another_skill() {
        let mut ui = menu();
        let mut st = TagTiles::default();
        let mut s = shown(&["line"; 20].join("|"));
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert!(st.text.most > 0.0);
        st.over_text = true;
        st.wheel(1.0);
        assert_eq!(st.text.value, TEXT_STEP);
        let text_scroll = st.text.value;
        st.over_text = false;
        st.wheel(1.0);
        assert_eq!(st.text.value, text_scroll, "the list wheel leaves the description alone");
        assert_eq!(st.shown.as_ref().unwrap().selected, Some(0));
        st.scroll_text(1000.0);
        st.rewrite(&mut ui);
        ui.update();
        assert_eq!(num(&ui, TEXT, "y") + num(&ui, TEXT, "height"), num(&ui, TEXT_BOX, "height"));
        let items = super::super::layout::layout(&ui, ui.menu(TAG_MENU).unwrap(), &|_, _, _| (0.0, 0.0), true);
        let text = ui.find_tile(TEXT).unwrap();
        let counter = ui.find_tile(COUNTER).unwrap();
        assert!(items.iter().find(|i| i.tile == text).unwrap().clip.is_some());
        assert!(items.iter().find(|i| i.tile == counter).unwrap().clip.is_none());
        s.selected = Some(1);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.text.value, 0.0);
        assert_eq!(num(&ui, TEXT, "y"), 25.0);
        s.info = "short".into();
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.text.most, 0.0);
    }

    #[test]
    fn fallback_keeps_opening_scrolls_and_new_opening_resets_them() {
        let mut ui = menu();
        let mut st = TagTiles::default();
        let mut s = shown(&["line"; 20].join("|"));
        assert!(frame(&mut ui, &s, &mut st).yes());
        st.scroll_text(2.0);
        let scroll = st.text.value;
        st.fail(&mut ui);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.text.value, scroll);
        assert_eq!(st.file.unwrap().text_top, 25.0);
        s.generation = 2;
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.text.value, 0.0);
        assert!(!st.list.highlight);
    }

    #[test]
    fn pointer_script_and_keys_queue_skills_once_and_never_cross_openings() {
        let mut ui = menu();
        let mut st = TagTiles::default();
        assert!(frame(&mut ui, &shown("text"), &mut st).yes());
        let row = st.rows[1].tile;
        let done = ui.find_tile(DONE).unwrap();
        let reset = ui.find_tile(RESET).unwrap();
        for (tile, by) in [(row, By::Pointer), (done, By::Script), (reset, By::Key("R".into()))] {
            follow_click(&mut st, &ui, &Click { tile, id: None, by, stamp: 0 }, None, false, &["R"]);
        }
        assert_eq!(st.take_acts(1), [TileAct::SkillClick(33), TileAct::Done]);
        st.act(TileAct::Hover(FormId(32)));
        assert!(st.take_acts(2).is_empty());
        st.held = true;
        assert!(follow_click(&mut st, &ui, &Click { tile: row, id: None, by: By::Script, stamp: 0 }, None, false, &[]).is_none());
    }

    #[test]
    fn handover_uses_the_current_menu_opening_and_tree() {
        let mut ui = menu();
        let mut st = TagTiles::default();
        assert!(frame(&mut ui, &shown("text"), &mut st).yes());
        let menu = ui.menu(TAG_MENU);
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TAG_SKILLS), menu), menu);
        assert_eq!(drawn_by(&st, None, Some(codes::TRAITS), menu), None);
        assert_eq!(drawn_by(&st, Some(2), Some(codes::TAG_SKILLS), menu), None);
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TAG_SKILLS), None), None);
    }
}
