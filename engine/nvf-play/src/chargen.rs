//! The character-creation menus the opening's scripts open (docs/spec-notes/chargen.md),
//! drawn as the original's: the name menu (1051, [`name`]), the Vigor Tester's
//! S.P.E.C.I.A.L. menu (1074, [`vigor`]), the tag-skill menu (1048) and the trait menu
//! (1084, both [`lists`]). The rules (points, limits, what Done writes) are
//! `fnv_game::chargen`'s; this module draws the menus and turns keys and clicks into
//! changes of their sheets.
//!
//! - A menu opens when the runtime's open menu (`Game::menu`) becomes its code, and
//!   reads the player as the game holds them then.
//! - Done writes the sheet into the game ([`finish`]) and answers the menu
//!   (`ScriptState::answer`), which closes it after the frame's scripts ran, as the
//!   placeholder's Done did; the scripts then go on (the next stage's timer).
//! - Keys of the tag-skill and trait menus: Up/Down (or W/S, the wheel) choose a row,
//!   Page Up/Down a page, Enter or Space tags or takes the chosen row, R resets, A is
//!   Done [observed: the hints "Reset R)" and "Done A)"; hypothesis: the others]. The
//!   pointer chooses the row it is on, a click tags or takes it; Reset, Done and the
//!   scroll arrows are clickable. On the attributes page (AddSPECIALPoints, 1048) Right
//!   / Left (held, they repeat [documented: the row's `repeathorizontal`]) and Enter
//!   raise and lower the chosen attribute, a click on "+" or "−" raises or lowers its
//!   row, a click on a row only chooses it [hypothesis]. Name menu: typing adds to the name, Backspace rubs
//!   out, Enter or a click on Ok is Ok.
//! - Unattended runs: `NVF_PLAY_TEST_SCRIPT_MENUS` still closes a menu left open, keeping
//!   what the game holds (as before); `NVF_PLAY_TEST_CHARGEN=<seconds>:<op>[,...]` plays
//!   the player: `type:<text>`, `clear` (rub the name out), `pick:<row name>` (choose a
//!   row and tag or take it), `raise:<row name>` / `lower:<row name>` (the attributes
//!   page), `reset`, `done`.
//! - The race menu (1036) is the character creator's (`crate::creator`), NVForever's own
//!   design (docs/spec-notes/character-creator.md).
//! - With `[UI] TileMenus=TraitMenu` the trait menu is drawn from its file
//!   (`crate::tiles::trait_menu`) when that is ready: [`draw_menus`] leaves a sheet in place of
//!   its own drawing, and the tiles' clicks come here as [`TileAct`]s. The sheet, the keys,
//!   the sounds and Done stay as above.

pub mod draw;
pub mod lists;
pub mod name;
pub mod vigor;

use crate::pause::AidTime;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use crate::ui_pointer::PointerState;
use fnv_game::chargen as rules;
use fnv_game::game::menus;
use fnv_game::Game;
use fnv_sim::FormId;
use lists::{ListButton, ListMenu, ListRow, ListView};
use name::{NameMenu, OkButton};
use nvf_assets::GameData;

/// Whether a script's menu is drawn by this module (else by `crate::script_hud`).
pub fn has_own_menu(code: u16) -> bool {
    matches!(code, menus::NAME | menus::LOVE_TESTER | menus::TAG_SKILLS | menus::TRAITS)
}

/// The script menu open now, if any.
pub fn open_code(script: &ScriptState) -> Option<u16> {
    script.rt.as_ref().and_then(|rt| rt.game.menu)
}

/// Done in a character-creation menu: `apply` writes the sheet into the game (with the
/// simulation's world in it), then the menu is answered with Done. Whether the menu was
/// open to be answered. When what `apply` wrote opened another menu (a perk taken in the
/// level-up menu whose stage calls AddSPECIALPoints), that menu took this one's place
/// (`Game::open_menu`) and is left open: this one is done.
pub fn finish(script: &mut ScriptState, sim: &mut SimState, code: u16, how: &str, apply: impl FnOnce(&mut Game)) -> bool {
    let Some(rt) = script.rt.as_mut() else { return false };
    if rt.game.menu != Some(code) {
        return false;
    }
    crate::script::with_world(rt, sim, |rt| apply(&mut rt.game));
    if let Some(next) = rt.game.menu.filter(|&m| m != code) {
        info!("script: menu {code} done; menu {next} opened in its place");
        return true;
    }
    script.answer(None, how)
}

/// Run `f` on the game with the simulation's world in it.
pub(crate) fn with_game<R>(script: &mut ScriptState, sim: &mut SimState, f: impl FnOnce(&mut Game) -> R) -> Option<R> {
    let rt = script.rt.as_mut()?;
    Some(crate::script::with_world(rt, sim, |rt| f(&mut rt.game)))
}

/// The open menu drawn here.
enum OpenMenu {
    Name(NameMenu),
    List(ListMenu),
}

impl OpenMenu {
    fn code(&self) -> u16 {
        match self {
            OpenMenu::Name(_) => menus::NAME,
            OpenMenu::List(m) => m.code,
        }
    }
}

#[derive(Resource, Default)]
pub struct ChargenMenus {
    open: Option<OpenMenu>,
    /// Redraw on the next frame.
    dirty: bool,
    /// Done was pressed; the runtime closes the menu after this frame's scripts.
    answered: bool,
    /// Counted up each time a menu opens, so nothing a drawing queued for one acts on the next.
    generation: u64,
    /// Counted up each time the keys or the test aid move the chosen row: a tile list brings
    /// that row into view, and leaves it where it is for the pointer's choice
    /// (`crate::tiles::trait_menu`). Not part of [`ListMenu`], so the hand-built drawing is
    /// not spawned again for it.
    navigated: u32,
    /// [`draw_menus`] last left the menu to its tiles.
    tiles_draw: bool,
    kit: Option<draw::Kit>,
}

/// [design] The open trait or tag-skill list, as the tile drawing reads it. The sheet
/// stays [`ListMenu`]'s: the drawing never makes one of its own, as the sheets' constructors
/// use up what the scripts asked for.
pub struct TraitsOpen<'a> {
    pub menu: &'a ListMenu,
    pub generation: u64,
    pub navigated: u32,
    /// Done was pressed: the menu closes after the frame's scripts, and takes nothing more.
    pub answered: bool,
}

impl ChargenMenus {
    /// [design] The requested selection page, from the sheet already opened by sync.
    /// SpecialSheet uses menu 1048 too and deliberately stays with its hand-built drawing.
    pub fn list_open(&self, code: u16) -> Option<TraitsOpen<'_>> {
        match &self.open {
            Some(OpenMenu::List(m)) if m.code == code && (code == menus::TRAITS || (code == menus::TAG_SKILLS && matches!(m.sheet, lists::Sheet::Tags { .. }))) =>
                Some(TraitsOpen { menu: m, generation: self.generation, navigated: self.navigated, answered: self.answered }),
            _ => None,
        }
    }
    pub fn traits_open(&self) -> Option<TraitsOpen<'_>> { self.list_open(menus::TRAITS) }
    pub fn tags_open(&self) -> Option<TraitsOpen<'_>> { self.list_open(menus::TAG_SKILLS) }
}

/// What a click or the pointer on a selection menu's tiles asks of the menu, for [`input`] to
/// turn into the same changes as the hand-built parts ([`tile_ops`]). A row is named by its
/// trait or skill, not its place, so an act queued before anything changed acts on that row or on
/// nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileAct {
    /// The pointer came onto a row: it is chosen.
    Hover(FormId),
    /// A click on a row: it is chosen, then taken or given back.
    Click(FormId),
    /// [design] A skill actor-value code, kept apart from trait FormIDs.
    SkillHover(u16),
    SkillClick(u16),
    Reset,
    Done,
    /// The pointer came onto Reset or Done, for its sound (text_box.xml's).
    ButtonFocus,
}

/// The changes a tile's act makes, as the hand-built row's and buttons' (`input`). A row the
/// sheet no longer has makes none.
fn tile_ops(m: &ListMenu, act: TileAct) -> Vec<(Op, &'static str)> {
    match act {
        TileAct::Hover(key) => m.trait_row(key).map_or_else(Vec::new, |i| vec![(Op::Select(i), "pointer")]),
        TileAct::Click(key) => m.trait_row(key).map_or_else(Vec::new, |i| vec![(Op::Select(i), "pointer"), (Op::Click, "click")]),
        TileAct::SkillHover(key) => m.tag_row(key).map_or_else(Vec::new, |i| vec![(Op::Select(i), "pointer")]),
        TileAct::SkillClick(key) => m.tag_row(key).map_or_else(Vec::new, |i| vec![(Op::Select(i), "pointer"), (Op::Click, "click")]),
        TileAct::Reset => vec![(Op::Reset, "click")],
        TileAct::Done => vec![(Op::Done, "click")],
        TileAct::ButtonFocus => Vec::new(),
    }
}

#[derive(Component)]
struct ChargenRoot;

/// A change asked for by keys, the pointer or the test aid.
#[derive(Debug, Clone, PartialEq)]
enum Op {
    Type(String),
    Backspace,
    Clear,
    Step(i32),
    Select(usize),
    Toggle,
    /// A click on a row: tags or takes it; on the attributes page nothing more.
    Click,
    /// Raise or lower an attribute (the chosen one when `None`).
    Adjust(Option<usize>, bool),
    Pick(String),
    Scroll(i32),
    Reset,
    Done,
}

/// `NVF_PLAY_TEST_CHARGEN`: the steps still to take, by seconds on the aids' clock
/// (`crate::pause::AidTime`, as the level-up menu's).
#[derive(Resource, Default)]
struct ChargenAid(Vec<(f32, Op)>);

/// Read `NVF_PLAY_TEST_CHARGEN` (`<seconds>:<op>[,...]`).
fn parse_aid(s: &str) -> Result<Vec<(f32, Op)>, String> {
    let mut out = Vec::new();
    for item in s.split(',').map(str::trim).filter(|i| !i.is_empty()) {
        let (t, rest) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <seconds>:<op>"))?;
        let t: f32 = t.trim().parse().map_err(|_| format!("{item:?}: bad seconds"))?;
        let op = match rest.split_once(':') {
            Some(("type", text)) => Op::Type(text.to_string()),
            Some(("pick", row)) => Op::Pick(row.to_string()),
            Some(("raise", row)) => Op::Pick(format!("+{row}")),
            Some(("lower", row)) => Op::Pick(format!("-{row}")),
            None if rest == "clear" => Op::Clear,
            None if rest == "reset" => Op::Reset,
            None if rest == "done" => Op::Done,
            _ => return Err(format!("{item:?}: unknown step")),
        };
        out.push((t, op));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(out)
}

pub struct ChargenPlugin;

impl Plugin for ChargenPlugin {
    fn build(&self, app: &mut App) {
        let aid = match std::env::var("NVF_PLAY_TEST_CHARGEN") {
            Ok(v) => parse_aid(&v).unwrap_or_else(|e| {
                warn!("NVF_PLAY_TEST_CHARGEN: {e}");
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        app.init_resource::<ChargenMenus>()
            .insert_resource(ChargenAid(aid))
            .add_plugins(vigor::VigorPlugin)
            .add_systems(Update, (sync, input, draw_menus).chain().in_set(ChargenSet).in_set(PlaySet::Ui));
    }
}

/// Open the menu the scripts opened, close the one they closed.
/// The character menus' systems. The level-up menu's run after them, so a menu its Done
/// opens in its place (Intense Training's attributes page) opens on the next frame,
/// after the scripts have run the perk's stage (left unordered, the order changed when
/// other plugins were added: the `levelup-menu` smoke test, 2026-10-16).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChargenSet;

fn sync(mut st: ResMut<ChargenMenus>, script: Option<ResMut<ScriptState>>, sim: Option<ResMut<SimState>>) {
    let (Some(mut script), Some(mut sim)) = (script, sim) else { return };
    let code = open_code(&script).filter(|&c| has_own_menu(c) && c != menus::LOVE_TESTER);
    if st.open.as_ref().map(OpenMenu::code) == code {
        return;
    }
    if st.open.is_some() {
        info!("chargen: menu closed");
    }
    st.open = None;
    st.answered = false;
    st.dirty = true;
    st.generation = st.generation.wrapping_add(1);
    let Some(code) = code else { return };
    st.open = with_game(&mut script, &mut sim, |g| {
        if code == menus::NAME {
            let m = NameMenu::open(rules::name_offered(g), Some(g));
            info!("chargen: name menu open, offering \"{}\"", m.text);
            OpenMenu::Name(m)
        } else {
            let m = ListMenu::open(code, g);
            let what = if m.attributes() {
                "attributes menu open, points"
            } else if code == menus::TAG_SKILLS {
                "tag-skill menu open, tagged"
            } else {
                "trait menu open, taken"
            };
            let chosen = match &m.sheet {
                lists::Sheet::Special { now, .. } => now.remaining().to_string(),
                _ => list_or_none(&m.chosen_names(g)),
            };
            info!("chargen: {what}: {chosen} ({} rows)", m.rows());
            OpenMenu::List(m)
        }
    });
}

fn list_or_none(names: &[String]) -> String {
    if names.is_empty() {
        "none".into()
    } else {
        names.join(", ")
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn input(
    mut st: ResMut<ChargenMenus>,
    mut aid: ResMut<ChargenAid>,
    time: AidTime,
    keys: Res<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    mut wheel: MessageReader<MouseWheel>,
    rows: Query<(&PointerState, &ListRow), Changed<PointerState>>,
    buttons: Query<(&PointerState, &ListButton), Changed<PointerState>>,
    ok: Query<&PointerState, (Changed<PointerState>, With<OkButton>)>,
    script: Option<ResMut<ScriptState>>,
    sim: Option<ResMut<SimState>>,
    mut sounds: MessageWriter<UiSound>,
    mut traits: Option<ResMut<crate::tiles::trait_menu::TraitTiles>>,
    mut tags: Option<ResMut<crate::tiles::tag_menu::TagTiles>>,
) {
    use KeyCode::*;
    let typed: Vec<KeyboardInput> = typed.read().cloned().collect();
    let wheel: f32 = wheel.read().map(|w| if w.unit == MouseScrollUnit::Line { w.y } else { w.y / 40.0 }).sum();
    let (Some(mut script), Some(mut sim)) = (script, sim) else { return };
    if st.open.is_none() || st.answered {
        // Nothing the tiles queued acts on a menu closing.
        if let Some(t) = traits.as_mut().filter(|t| t.menu.is_some()) {
            t.drop_acts();
        }
        if let Some(t) = tags.as_mut() { t.drop_acts(); }
        return;
    }
    let mut ops: Vec<(Op, &str)> = Vec::new();
    // A world aid (`crate::pause::WorldAidClock`).
    let now = time.world_secs();
    while aid.0.first().is_some_and(|(t, _)| now.is_some_and(|now| *t <= now)) {
        let (_, op) = aid.0.remove(0);
        info!("test aid: chargen {op:?}");
        ops.push((op, "test aid NVF_PLAY_TEST_CHARGEN"));
    }
    let mut hover_ok = None;
    let mut hover_button = Vec::new();
    match st.open.as_ref() {
        Some(OpenMenu::Name(_)) => {
            for k in typed.iter().filter(|k| k.state.is_pressed()) {
                match &k.logical_key {
                    Key::Backspace => ops.push((Op::Backspace, "key")),
                    Key::Enter => ops.push((Op::Done, "Enter")),
                    _ => {
                        if let Some(t) = k.text.as_deref().filter(|t| !t.is_empty() && t.chars().all(|c| !c.is_control()) && fnv_util::text::encodable(t)) {
                            ops.push((Op::Type(t.to_string()), "key"));
                        }
                    }
                }
            }
            for i in &ok {
                hover_ok = Some(*i != PointerState::None);
                if *i == PointerState::Pressed {
                    ops.push((Op::Done, "click"));
                }
            }
        }
        Some(OpenMenu::List(m)) => {
            // Drawn from its tiles the rows' place and the scroll are the drawing's; while it
            // takes nothing (the pause menu over it) neither do these.
            let tiles: Option<&mut dyn crate::tiles::selection_menu::SelectionInput> = if m.code == menus::TRAITS {
                traits.as_deref_mut().filter(|t| t.menu.is_some() && t.is_for(st.generation)).map(|t| t as &mut dyn crate::tiles::selection_menu::SelectionInput)
            } else if m.code == menus::TAG_SKILLS && !m.attributes() {
                tags.as_deref_mut().filter(|t| t.menu.is_some() && t.is_for(st.generation)).map(|t| t as &mut dyn crate::tiles::selection_menu::SelectionInput)
            } else { None };
            let tiled = tiles.is_some();
            let page = tiles.as_ref().map_or(m.visible() as i32, |t| t.page_rows());
            let keys_on = tiles.as_ref().is_none_or(|t| t.takes().is_some());
            if let Some(t) = tiles.filter(|t| t.takes().is_some()) {
                if wheel.abs() >= 1.0 {
                    t.wheel(-f64::from(wheel.round()));
                }
                // NVForever's addition [design]: the keyboard and controller route to the end
                // of a long description. They move nothing in the original trait menu [observed: its job
                // `trait`, chargen.md §4.1].
                for (k, by) in [(ArrowLeft, -1.0), (ArrowRight, 1.0)] {
                    if keys.just_pressed(k) {
                        t.scroll_text(by);
                    }
                }
                for act in t.take_acts(st.generation) {
                    if act == TileAct::ButtonFocus {
                        sounds.write(UiSound { menu: list_menu(m.code), event: UiEvent::Focus });
                    }
                    ops.extend(tile_ops(m, act));
                }
            }
            if m.attributes() {
                for k in typed.iter().filter(|k| k.state.is_pressed()) {
                    match k.logical_key {
                        Key::ArrowRight => ops.push((Op::Adjust(None, true), "key")),
                        Key::ArrowLeft => ops.push((Op::Adjust(None, false), "key")),
                        _ => {}
                    }
                }
            }
            for (k, op) in [
                ([ArrowUp, KeyW], Op::Step(-1)),
                ([ArrowDown, KeyS], Op::Step(1)),
                ([PageUp, PageUp], Op::Step(-page)),
                ([PageDown, PageDown], Op::Step(page)),
                ([Enter, Space], Op::Toggle),
                ([NumpadEnter, NumpadEnter], Op::Toggle),
                ([KeyR, KeyR], Op::Reset),
                ([KeyA, KeyA], Op::Done),
            ] {
                if keys_on && keys.any_just_pressed(k) {
                    ops.push((op, "key"));
                }
            }
            if wheel.abs() >= 1.0 && !tiled {
                ops.push((Op::Scroll(-wheel.round() as i32), "wheel"));
            }
            for (i, r) in &rows {
                match i {
                    PointerState::Hovered => ops.push((Op::Select(r.0), "pointer")),
                    PointerState::Pressed => {
                        ops.push((Op::Select(r.0), "pointer"));
                        ops.push((Op::Click, "click"));
                    }
                    PointerState::None => {}
                }
            }
            for (i, b) in &buttons {
                hover_button.push((*b, *i != PointerState::None));
                if *i == PointerState::Pressed {
                    let op = match b {
                        ListButton::Reset => Op::Reset,
                        ListButton::Done => Op::Done,
                        ListButton::ScrollUp => Op::Scroll(-1),
                        ListButton::ScrollDown => Op::Scroll(1),
                        ListButton::Plus(r) => Op::Adjust(Some(*r), true),
                        ListButton::Minus(r) => Op::Adjust(Some(*r), false),
                    };
                    ops.push((op, "click"));
                }
            }
        }
        None => {}
    }
    // The pointer over a button draws its box.
    let st: &mut ChargenMenus = &mut st;
    match st.open.as_mut() {
        Some(OpenMenu::Name(m)) => {
            if let Some(over) = hover_ok.filter(|&o| o != m.hover_ok) {
                m.hover_ok = over;
                st.dirty = true;
            }
        }
        Some(OpenMenu::List(m)) => {
            for (b, over) in hover_button {
                let next = if over { Some(b) } else if m.hover == Some(b) { None } else { m.hover };
                if next != m.hover {
                    // The pointer coming onto Reset or Done (text_box.xml's sound).
                    if next.is_some_and(|b| matches!(b, ListButton::Reset | ListButton::Done)) {
                        sounds.write(UiSound { menu: list_menu(m.code), event: UiEvent::Focus });
                    }
                    m.hover = next;
                    st.dirty = true;
                }
            }
        }
        None => {}
    }
    for (op, how) in ops {
        if let Some(event) = apply(st, &mut script, &mut sim, op, how) {
            if let Some(OpenMenu::List(m)) = st.open.as_ref() {
                sounds.write(UiSound { menu: list_menu(m.code), event });
            }
        }
        if st.answered {
            break;
        }
    }
}

/// The tag-skill or the trait menu, for its sounds.
fn list_menu(code: u16) -> UiMenu {
    if code == menus::TAG_SKILLS {
        UiMenu::TagSkills
    } else {
        UiMenu::Traits
    }
}

/// Apply one change to the open menu; what a list menu's sound is for it (list 5 items
/// 41, 42): the chosen row moved, a row taken or given back (or refused), Reset, Done.
fn apply(st: &mut ChargenMenus, script: &mut ScriptState, sim: &mut SimState, op: Op, how: &str) -> Option<UiEvent> {
    let open = st.open.as_mut()?;
    let code = open.code();
    match open {
        OpenMenu::Name(m) => {
            let changed = match op {
                Op::Type(t) => m.type_text(&t),
                Op::Backspace => m.backspace(),
                Op::Clear => !std::mem::take(&mut m.text).is_empty(),
                Op::Done if m.can_finish() => {
                    let name = m.text.trim().to_string();
                    if finish(script, sim, code, how, |g| rules::apply_name(g, &name)) {
                        info!("chargen: done, name \"{name}\"");
                        st.answered = true;
                    }
                    false
                }
                Op::Done => {
                    info!("chargen: Ok refused: no name");
                    false
                }
                _ => false,
            };
            st.dirty |= changed;
            None
        }
        OpenMenu::List(m) => {
            let before = m.clone();
            let mut event = None;
            if matches!(op, Op::Step(_) | Op::Pick(_)) {
                st.navigated = st.navigated.wrapping_add(1);
            }
            match op {
                Op::Step(by) => m.step(by),
                Op::Select(i) => m.select(i),
                Op::Scroll(by) => m.scroll_by(by),
                Op::Reset => {
                    m.reset();
                    info!("chargen: reset");
                    event = Some(UiEvent::Reset);
                }
                Op::Click if m.attributes() => {}
                Op::Adjust(row, up) => {
                    let i = row.unwrap_or(m.selected);
                    if row.is_some() {
                        m.select(i);
                    }
                    let ok = m.adjust(i, up);
                    event = Some(if ok { UiEvent::Value { up } } else { UiEvent::Refused });
                    let name = with_game(script, sim, |g| m.row_name(g, i)).flatten().unwrap_or_default();
                    let left = match &m.sheet {
                        lists::Sheet::Special { now, .. } => now.remaining(),
                        _ => 0,
                    };
                    let what = match (up, ok) {
                        (true, true) => "raised",
                        (false, true) => "lowered",
                        (true, false) => "raise refused",
                        (false, false) => "lower refused",
                    };
                    info!("chargen: {name} {what}; {left} points left");
                }
                Op::Toggle if m.attributes() => {
                    return apply(st, script, sim, Op::Adjust(None, true), how);
                }
                Op::Toggle | Op::Click | Op::Pick(_) => {
                    if let Op::Pick(name) = &op {
                        // `raise:` / `lower:` steps of the test aid: "+Row" / "-Row".
                        if let Some((up, row)) = name.strip_prefix('+').map(|r| (true, r)).or_else(|| name.strip_prefix('-').map(|r| (false, r))) {
                            let found = with_game(script, sim, |g| (0..m.rows()).find(|&i| m.row_name(g, i).is_some_and(|n| n.eq_ignore_ascii_case(row)))).flatten();
                            let Some(i) = found else {
                                warn!("test aid: chargen: no row {row:?}");
                                return None;
                            };
                            return apply(st, script, sim, Op::Adjust(Some(i), up), how);
                        }
                        let found = with_game(script, sim, |g| (0..m.rows()).find(|&i| m.row_name(g, i).is_some_and(|n| n.eq_ignore_ascii_case(name)))).flatten();
                        match found {
                            Some(i) => m.select(i),
                            None => {
                                warn!("test aid: chargen: no row {name:?}");
                                return None;
                            }
                        }
                    }
                    let i = m.selected;
                    let ok = m.toggle();
                    event = Some(if ok { UiEvent::Picked } else { UiEvent::Refused });
                    with_game(script, sim, |g| {
                        let row = m.row_name(g, i).unwrap_or_default();
                        let chosen = m.chosen_names(g);
                        let what = if !ok {
                            "refused"
                        } else if chosen.contains(&row) {
                            "chosen"
                        } else {
                            "given back"
                        };
                        info!("chargen: {row} {what}; chosen: {}", list_or_none(&chosen));
                    });
                }
                Op::Done if m.can_finish() => {
                    let sheet = m.sheet.clone();
                    let names = with_game(script, sim, |g| m.chosen_names(g)).unwrap_or_default();
                    let done = finish(script, sim, code, how, |g| match &sheet {
                        lists::Sheet::Tags { now, .. } => now.apply(g),
                        lists::Sheet::Traits { now, .. } => now.apply(g),
                        lists::Sheet::Special { now, .. } => now.apply(g),
                    });
                    if done {
                        let what = if m.attributes() {
                            "attributes"
                        } else if code == menus::TAG_SKILLS {
                            "tag skills"
                        } else {
                            "traits"
                        };
                        info!("chargen: done, {what}: {}", list_or_none(&names));
                        st.answered = true;
                        event = Some(UiEvent::Accepted);
                    }
                }
                Op::Done => {
                    match &m.sheet {
                        lists::Sheet::Tags { now, .. } => info!("chargen: Done refused: {} of {} skills tagged", now.tagged.len(), now.count),
                        lists::Sheet::Special { now, .. } => info!("chargen: Done refused: {} points left", now.remaining()),
                        lists::Sheet::Traits { .. } => {}
                    }
                    event = Some(UiEvent::Refused);
                }
                _ => {}
            }
            // The chosen row moved (keys, wheel, pointer).
            if event.is_none() && m.selected != before.selected {
                event = Some(UiEvent::Focus);
            }
            st.dirty |= *m != before;
            event
        }
    }
}

enum View {
    Name(NameMenu),
    List(ListView),
}

#[allow(clippy::too_many_arguments)]
fn draw_menus(
    mut commands: Commands,
    mut st: ResMut<ChargenMenus>,
    roots: Query<Entity, With<ChargenRoot>>,
    script: Option<ResMut<ScriptState>>,
    sim: Option<ResMut<SimState>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    traits: Option<Res<crate::tiles::trait_menu::TraitTiles>>,
    tags: Option<Res<crate::tiles::tag_menu::TagTiles>>,
) {
    // The drawing's own decision, this frame's (`crate::tiles::trait_menu::drawn`): the menu
    // goes the frame its tiles draw, and comes back whole, its sheet as it stands, when they
    // cannot, and a menu the scripts put in its place is drawn at once. Off, this stays false.
    let game = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    let tiled = traits.as_ref().is_some_and(|t| crate::tiles::trait_menu::drawn(t, st.traits_open(), game).is_some())
        || tags.as_ref().is_some_and(|t| crate::tiles::tag_menu::drawn(t, st.tags_open(), game).is_some());
    if tiled != st.tiles_draw {
        st.tiles_draw = tiled;
        st.dirty = true;
    }
    if !st.dirty {
        return;
    }
    st.dirty = false;
    for e in &roots {
        commands.entity(e).despawn();
    }
    if tiled {
        // No click goes past it [documented: trait_menu.xml, `stackingtype` `&no_click_past;`];
        // the tile pointer reads the cursor itself.
        let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
        commands.spawn((full, GlobalZIndex(crate::interface::TOP_MENU_Z), ChargenRoot)).with_children(|r| {
            r.spawn(crate::tiles::menus::modal_sheet());
        });
        return;
    }
    let (Some(mut script), Some(mut sim)) = (script, sim) else { return };
    let view = match st.open.as_ref() {
        None => return,
        Some(OpenMenu::Name(m)) => View::Name(m.clone()),
        Some(OpenMenu::List(m)) => match with_game(&mut script, &mut sim, |g| m.view(g)) {
            Some(v) => View::List(v),
            None => return,
        },
    };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let st = st.into_inner();
    let kit = st.kit.get_or_insert_with(|| draw::Kit::load(data.as_deref(), &mut images, bc));
    let mut c = draw::Ctx { kit, data: data.as_deref(), images: &mut images, bc };
    let root = Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };
    commands.spawn((root, GlobalZIndex(crate::interface::TOP_MENU_Z), crate::interface::top_menu_backing(), ChargenRoot)).with_children(|r| match &view {
        View::Name(m) => name::spawn(r, m, &mut c),
        View::List(v) => lists::spawn(r, v, &mut c),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_aid_reads() {
        let steps = parse_aid("5:pick:Four Eyes, 3:type:Six,4:clear,9:done,6:reset,7:raise:Luck").unwrap();
        assert!(steps.contains(&(7.0, Op::Pick("+Luck".into()))));
        assert_eq!(steps[0], (3.0, Op::Type("Six".into())));
        assert_eq!(steps[2], (5.0, Op::Pick("Four Eyes".into())));
        assert_eq!(steps.last().unwrap().1, Op::Done);
        assert!(parse_aid("1:jump").is_err());
    }

    #[test]
    fn a_tile_act_names_its_row_by_its_trait() {
        use fnv_game::chargen::TraitSheet;
        let sheet = TraitSheet { traits: vec![FormId(7), FormId(9)], chosen: Default::default(), max: 2 };
        let m = ListMenu { code: menus::TRAITS, sheet: lists::Sheet::Traits { now: sheet.clone(), start: sheet, allowed: vec![true, true] }, selected: 0, scroll: 0, hover: None };
        assert_eq!(tile_ops(&m, TileAct::Hover(FormId(9))), vec![(Op::Select(1), "pointer")]);
        assert_eq!(tile_ops(&m, TileAct::Click(FormId(9))), vec![(Op::Select(1), "pointer"), (Op::Click, "click")]);
        assert!(tile_ops(&m, TileAct::Click(FormId(5))).is_empty(), "a trait no longer on the sheet");
        assert_eq!(tile_ops(&m, TileAct::Done), vec![(Op::Done, "click")]);
        assert!(tile_ops(&m, TileAct::ButtonFocus).is_empty());
    }

    #[test]
    fn the_own_menus() {
        assert!(has_own_menu(menus::NAME) && has_own_menu(menus::TRAITS) && has_own_menu(menus::TAG_SKILLS) && has_own_menu(menus::LOVE_TESTER));
        assert!(!has_own_menu(menus::RACE) && !has_own_menu(menus::MESSAGE));
    }
    #[test]
    fn tag_tile_acts_resolve_skill_codes_and_special_stays_hand_built() {
        use fnv_game::chargen::{TagSheet, SpecialSheet};
        let tags = TagSheet { skills: vec![32, 35], tagged: Default::default(), locked: Default::default(), count: 4 };
        let menu = ListMenu { code: menus::TAG_SKILLS, sheet: lists::Sheet::Tags { now: tags.clone(), start: tags }, selected: 0, scroll: 0, hover: None };
        assert_eq!(tile_ops(&menu, TileAct::SkillClick(35)), vec![(Op::Select(1), "pointer"), (Op::Click, "click")]);
        assert_eq!(tile_ops(&menu, TileAct::SkillHover(32)), vec![(Op::Select(0), "pointer")]);
        assert!(tile_ops(&menu, TileAct::Click(FormId(35))).is_empty());
        assert!(tile_ops(&menu, TileAct::SkillClick(99)).is_empty());
        let mut st = ChargenMenus { open: Some(OpenMenu::List(menu)), generation: 8, ..Default::default() };
        assert_eq!(st.tags_open().unwrap().generation, 8); assert!(st.traits_open().is_none());
        let special = SpecialSheet::new([5; 7]);
        st.open = Some(OpenMenu::List(ListMenu { code: menus::TAG_SKILLS, sheet: lists::Sheet::Special { now: special, start: special }, selected: 0, scroll: 0, hover: None }));
        assert!(st.tags_open().is_none());
        let Some(OpenMenu::List(menu)) = st.open.as_ref() else { panic!("missing sheet") };
        assert!(tile_ops(menu, TileAct::SkillClick(32)).is_empty());
    }

}
