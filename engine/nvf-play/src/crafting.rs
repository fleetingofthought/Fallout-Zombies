//! The recipe menu: the workbench, the campfire and the reloading bench share it
//! (docs/spec-notes/crafting.md; the records and the check are `fnv_game::crafting`).
//!
//! A script opens it (`station.ShowRecipeMenu category`, the activators' own scripts);
//! it lists the recipes of that category or subcategory whose visibility conditions pass
//! for the player: the ones the player can make now first, then the rest, each part by
//! name (owner's request 2026-10-26). Seven rows show at once with a scroll bar right of
//! them when there are more; the keys and the wheel move the choice and scroll to keep it
//! in view, the pointer only chooses the row under it (the list never moves under it,
//! as in every menu). The left/right arrows over the list filter it by subcategory
//! (all, then each subcategory the list holds, by name) [hypothesis: the arrows of the
//! original's `RM_Items_LeftFilterArrow` / `RightFilterArrow` cycle the subcategories;
//! not captured]. At the right, for the chosen recipe: where it is made (its category's
//! name), the skill it needs with the player's value and the level ("Science (17/45)"),
//! and each ingredient with how many the player has and needs ("Fission Battery (0/1)")
//! [observed: the owner's screenshots, C:\fnv-private\Crafting]; under the list the
//! first output's weight and value. Accept (A, Enter) makes it: the ingredients leave the
//! player's inventory and the outputs arrive, with an "added" line in the corner; a
//! recipe the player cannot make yet stays in the list half bright and Accept refuses
//! it. Exit (X, Esc, Tab) closes the menu.
//!
//! Making several at once (2026-10-19): when the player has what it takes to make the
//! recipe, Accept opens the "How many?" prompt, even when only one can be made (owner
//! 2026-10-26: Sunny's healing powder asked nothing) of the other menus
//! (`menus\quantity_menu.xml`, drawn by `menu::look::spawn_quantity`), from 1 up to the
//! most the ingredients allow ([`times_possible`]); Left/Right (or the wheel, the arrows,
//! a press on the meter) set the amount, Enter or A makes that many, E or Esc goes back
//! [documented: the prompt's file; hypothesis: the original's recipe menu asks so]. Log
//! `recipe menu: how many? 1 of <n>`, `recipe menu: made <n> x "<name>" (...)`.
//!
//! The output's picture and its effects (2026-10-19) [documented: `RM_ItemIcon` at the
//! list box's left plus 22, 472 down, 256 square; the item stats display's EffectsInfo
//! card in `RM_ItemData`; hypothesis: the effects text as the Pip-Boy's ITEMS page writes
//! it, under the WG and VAL cards]: the first output's ICON, and an aid item's own effects
//! or an apparel's or a weapon's enchantment's ("HP +40, RAD -10").
//!
//! The look [documented: the original's `menus\recipe_menu.xml`, layout in its units:
//! the menu 1024 by 800 in the middle of the screen, the list's box at (20, 20) 464 wide,
//! the recipe's box 560 further right; the titles in font 6, the values in font 3, the
//! rows in font 2; Accept and Exit at y 670; observed: the screenshots for the rest].
//! Test aid `NVF_PLAY_TEST_RECIPE=<seconds>:<action>[,…]`: `down`, `up`, `filter` (the
//! right arrow), `accept`, `exit`, `more` and `less` (the prompt's amount), `cancel` (the
//! prompt), `shot` (a screenshot `recipe_menu.png` in the working folder). Open the menu through a script, e.g. the console aid's `player.craft
//! WorkbenchRecipes`.

use crate::interface::{amber, bc_supported, face, plain_font, set_line, u, Face};
use crate::menu::look::ROWS_BAR_W;
use crate::menu::model::keep_in_view;
use crate::pause::AidTime;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::start_menu::{color, highlight_colors, Shade, LINE};
use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
use crate::{PlaySet, PlayState};
use bevy::image::CompressedImageFormatSupport;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use crate::ui_pointer::{Button, PointerState};
use fnv_game::crafting::{missing, skill_name, RecipeDef, RECIPE_MENU};
use fnv_sim::FormId;
use nvf_assets::GameData;

/// The menu's size and where its boxes are, in its file's units [documented:
/// recipe_menu.xml `RM_MainRect`, `RM_ItemsRect`, `RM_ContainerRect`].
const MAIN: Vec2 = Vec2::new(1024.0, 800.0);
const ITEMS_AT: Vec2 = Vec2::new(20.0, 20.0);
const BOX_WIDTH: f32 = 464.0;
const RIGHT_AT: Vec2 = Vec2::new(580.0, 20.0);
/// The list: its top, its rows' height and how many show [documented: `RM_Items_
/// InventoryList` at y 60, 365 high; observed: rows about 58 of 1080 pixels apart].
const LIST_TOP: f32 = 80.0;
const ROW: f32 = 52.0;
const ROWS: usize = 7;
/// The buttons' top [documented: `RM_ButtonRect` y 670].
const BUTTONS_TOP: f32 = 670.0;
/// The output's picture [documented: `RM_ItemIcon`, the list box's x plus 22, its y plus
/// 472, 256 square].
const ICON_AT: Vec2 = Vec2::new(ITEMS_AT.x + 22.0, ITEMS_AT.y + 472.0);
const ICON_SIZE: f32 = 256.0;
/// The sheet behind the menu [documented: `RM_Background` solid black at brightness 128
/// and the background fill alpha, 1144 by 970 from (-40, -105)]: faint, as the original's,
/// which shows only the frame's lines over the world [observed: the owner's screenshot,
/// 2026-10-19; the 0.75 of the other menus drew a dark box far past the frame]. The
/// original also blurs the world behind the menu (not built). Ours darkens the whole
/// screen as the character menus do: the owner found the world behind too bright for the
/// text (playtest list 7 item 2, `crate::interface::top_menu_backing`) [design].
/// A recipe that cannot be made yet: a little dimmer than one that can [observed: the
/// owner's screenshots, where no recipe could be made, show the rows a shade under the
/// titles; the brightness is a guess].
const NOT_READY: f32 = 190.0;

/// Fonts by the number the menu file gives them [documented: Fallout_default.ini].
const TITLE_FONT: usize = 6;
const VALUE_FONT: usize = 3;
const ROW_FONT: usize = 2;

pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        let mut aid = Vec::new();
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_RECIPE") {
            for part in v.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                match part.split_once(':').and_then(|(t, a)| Some((t.trim().parse::<f32>().ok()?, a.trim().to_string()))) {
                    Some(x) => aid.push(x),
                    None => warn!("NVF_PLAY_TEST_RECIPE: {part:?}: expected <seconds>:<action>"),
                }
            }
            aid.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        app.init_resource::<RecipeMenu>()
            .insert_resource(RecipeAid(aid))
            .add_systems(Update, (scripts_open, (keys, draw).chain().run_if(in_state(PlayState::Menu))).chain().in_set(PlaySet::Ui))
            .add_systems(OnExit(PlayState::Menu), close);
    }
}

/// `NVF_PLAY_TEST_RECIPE`: (seconds, action), soonest first.
#[derive(Resource, Default)]
struct RecipeAid(Vec<(f32, String)>);

/// The recipe menu while it is open.
#[derive(Resource, Default)]
pub struct RecipeMenu(pub Option<Open>);

impl RecipeMenu {
    pub fn is_open(&self) -> bool {
        self.0.is_some()
    }
}

pub struct Open {
    category: FormId,
    station: Option<FormId>,
    /// 0: all; else the subcategory at this place of the view's filters, less one.
    filter: usize,
    selected: usize,
    /// The list's first row shown: the keys and the wheel move it to keep the choice in
    /// view, the pointer never does.
    first: usize,
    drawn: Option<View>,
    /// The "How many?" prompt: the amount and the most that can be made.
    quantity: Option<(i32, i32)>,
}

/// How many times the player can make `r` with what it has: the fewest of each
/// ingredient's count over what one making takes (no ingredients: once).
pub fn times_possible(r: &RecipeDef, have: impl Fn(FormId) -> i64) -> u32 {
    r.ingredients.iter().filter(|(_, n)| *n > 0).map(|&(i, n)| (have(i).max(0) as u64 / n as u64) as u32).min().unwrap_or(1)
}

#[derive(Component)]
struct RecipeRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum RecipeButton {
    Row(usize),
    Accept,
    Exit,
    Left,
    Right,
}

/// A row of the list.
#[derive(Debug, Clone, PartialEq)]
struct RowView {
    recipe: FormId,
    label: String,
    /// Can be made now.
    ready: bool,
}

/// Everything the menu shows: drawn again when it changes.
#[derive(Debug, Clone, PartialEq)]
struct View {
    title: String,
    rows: Vec<RowView>,
    /// The subcategories the arrows go through, by name.
    filters: usize,
    selected: usize,
    /// The first row shown.
    first: usize,
    made_at: String,
    /// Skill name, the player's value, the level needed.
    skill: Option<(String, u32, u32)>,
    /// Name, have, need.
    ingredients: Vec<(String, u32, u32)>,
    weight: Option<f32>,
    value: Option<i32>,
    /// The first output's picture and effects.
    icon: Option<String>,
    effects: Option<String>,
    /// The "How many?" prompt: amount and most.
    quantity: Option<(i32, i32)>,
    /// The names the hints use (`crate::pad::labels`).
    labels: crate::pad::Labels,
}

/// The recipe's label: its name, and the number made after it when more than one and the
/// name does not say it already [observed: "Antivenom (2)", "Conversion, Fission Battery
/// to SEC (25)" on the owner's screenshots].
fn label(r: &RecipeDef) -> String {
    let n = r.made();
    if n > 1 && !r.name.trim_end().ends_with(')') {
        format!("{} ({n})", r.name)
    } else {
        r.name.clone()
    }
}

/// The recipes a category lists for the player now: its own or its subcategory's, their
/// conditions passing, by name.
fn listed(rt: &mut crate::script::Runtime, category: FormId) -> Vec<&'static RecipeDef> {
    let data: &'static fnv_game::data::GameData = rt.game.data;
    let mut out: Vec<&RecipeDef> = data.recipes.values().filter(|r| r.listed_under(category)).collect();
    out.retain(|r| rt.game.conditions_pass(&r.conditions, Some(FormId::PLAYER_REF), None, "recipe"));
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.form.0.cmp(&b.form.0)));
    out
}

/// What the player lacks to make `r` once (with the world in the runtime).
fn lacks(rt: &mut crate::script::Runtime, r: &RecipeDef) -> Vec<fnv_game::crafting::Missing> {
    let skill = r.skill.map_or(0.0, |(av, _)| fnv_game::functions::actor_value_capped(&mut rt.game, FormId::PLAYER_REF, av));
    let counts: Vec<(FormId, i64)> = r.ingredients.iter().map(|&(i, _)| (i, rt.game.world.item_count(FormId::PLAYER_REF, i).unwrap_or(0) as i64)).collect();
    missing(r, 1, |_| skill, |i| counts.iter().find(|c| c.0 == i).map_or(0, |c| c.1))
}

/// What the menu shows for `open`.
fn view(rt: &mut crate::script::Runtime, sim: &mut SimState, open: &Open) -> View {
    let forms = sim.forms;
    crate::script::with_world(rt, sim, |rt| {
        let data = rt.game.data;
        let all = listed(rt, open.category);
        // The subcategories in the list, by name.
        let mut subs: Vec<(String, FormId)> = all.iter().filter_map(|r| r.subcategory).filter(|s| *s != open.category).map(|s| (data.recipe_categories.get(&s).map_or_else(String::new, |c| c.name.clone()), s)).collect();
        subs.sort();
        subs.dedup();
        let filter = subs.get(open.filter.wrapping_sub(1)).map(|s| s.1);
        let have = |rt: &mut crate::script::Runtime, item: FormId| rt.game.world.item_count(FormId::PLAYER_REF, item).unwrap_or(0) as i64;
        // The recipes that can be made now come first, each part by name (owner's
        // request 2026-10-26).
        let mut listed: Vec<(&RecipeDef, bool)> = all.into_iter().filter(|r| filter.is_none_or(|f| r.subcategory == Some(f))).map(|r| (r, lacks(rt, r).is_empty())).collect();
        listed.sort_by_key(|&(_, ready)| !ready);
        let shown: Vec<&RecipeDef> = listed.iter().map(|l| l.0).collect();
        let rows: Vec<RowView> = listed.iter().map(|&(r, ready)| RowView { recipe: r.form, label: label(r), ready }).collect();
        let selected = open.selected.min(rows.len().saturating_sub(1));
        let title = match filter {
            None => "RECIPES".to_string(),
            Some(_) => subs[open.filter - 1].0.to_uppercase(),
        };
        let chosen = shown.get(selected).copied();
        let name = |id: FormId| crate::menu::model::display_name(forms, id);
        let made_at = chosen.and_then(|r| r.category).and_then(|c| data.recipe_categories.get(&c)).map_or_else(String::new, |c| c.name.clone());
        let skill = chosen.and_then(|r| r.skill).map(|(av, level)| (skill_name(av).to_string(), fnv_game::functions::actor_value_capped(&mut rt.game, FormId::PLAYER_REF, av).max(0.0).floor() as u32, level));
        let ingredients = chosen.map(|r| r.ingredients.iter().map(|&(i, n)| (name(i), have(rt, i).max(0) as u32, n)).collect()).unwrap_or_default();
        let item = chosen.and_then(|r| r.outputs.first()).and_then(|o| match forms.get(o.0).map(|b| &b.data) {
            Some(fnv_sim::forms::BaseData::Item(it)) => Some((it.weight, it.value, it.icon.clone())),
            _ => None,
        });
        let (weight, value, icon) = item.map_or((None, None, None), |i| (i.0, i.1, i.2));
        let first = keep_in_view(open.first, selected, rows.len(), ROWS);
        View { title, rows, filters: subs.len(), selected, first, made_at, skill, ingredients, weight, value, icon, effects: None, quantity: open.quantity, labels: crate::pad::labels() }
    })
}

/// The recipe menus scripts asked for, opened while playing.
#[allow(clippy::too_many_arguments)]
fn scripts_open(
    script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut menu: ResMut<RecipeMenu>,
    active: Res<crate::menu::ActiveMenu>,
    mut sounds: MessageWriter<UiSound>,
) {
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    if rt.game.recipe_requests.is_empty() || *state.get() != PlayState::Playing || menu.is_open() || active.0.is_some() {
        return;
    }
    let Some(r) = rt.game.recipe_requests.pop() else { return };
    rt.game.recipe_requests.clear();
    let open = Open { category: r.category, station: r.station, filter: 0, selected: 0, first: 0, drawn: None, quantity: None };
    let v = view(rt, &mut sim, &open);
    let name = rt.game.data.recipe_categories.get(&r.category).map_or_else(|| r.category.to_string(), |c| c.name.clone());
    let first: Vec<&str> = v.rows.iter().take(6).map(|r| r.label.as_str()).collect();
    info!(
        "recipe menu: open {name} ({} recipes listed of {} known, {} ready; station {}): {}",
        v.rows.len(),
        rt.game.data.recipes.len(),
        v.rows.iter().filter(|r| r.ready).count(),
        open.station.map_or("none".to_string(), |s| s.to_string()),
        first.join(", ")
    );
    menu.0 = Some(open);
    next.set(PlayState::Menu);
    sounds.write(UiSound { menu: UiMenu::Recipe, event: UiEvent::Opened });
}

/// The "How many?" prompt's buttons and its meter (`crate::menu`).
type AmountButton = (&'static PointerState, &'static crate::menu::CmdButton);
type AmountPress = (&'static PointerState, &'static bevy::ui::RelativeCursorPosition);
/// The records the effects line is read from.
type EffectForms = crate::pipboy::stats::read::Forms;

#[allow(clippy::too_many_arguments)]
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    time: AidTime,
    mut aid: ResMut<RecipeAid>,
    mut menu: ResMut<RecipeMenu>,
    script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    buttons: Query<(&PointerState, &RecipeButton), Changed<PointerState>>,
    (amount_buttons, meter): (Query<AmountButton, Changed<PointerState>>, Query<AmountPress, With<crate::menu::AmountMeter>>),
    scroll: Option<Res<AccumulatedMouseScroll>>,
    mut next: ResMut<NextState<PlayState>>,
    mut sounds: MessageWriter<UiSound>,
    mut corner: Option<ResMut<crate::corner_messages::CornerMessages>>,
    mut moved: MessageReader<bevy::window::CursorMoved>,
    mut commands: Commands,
) {
    // Only a pointer that moves takes the selection: the rows drawn when the menu opens
    // (or redrawn) under a resting pointer leave the keys' choice alone (as `crate::menu`).
    let pointer_moved = moved.read().count() > 0;
    let Some(open) = menu.0.as_mut() else { return };
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    // A world aid (`crate::pause::WorldAidClock`).
    let now = time.world_secs();
    let mut acts = Vec::new();
    while aid.0.first().is_some_and(|(t, _)| now.is_some_and(|now| *t <= now)) {
        let (_, a) = aid.0.remove(0);
        info!("test aid NVF_PLAY_TEST_RECIPE: {a}");
        acts.push(a);
    }
    let act = |n: &str| acts.iter().any(|a| a == n);
    if act("shot") {
        info!("recipe menu: screenshot recipe_menu.png");
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk("recipe_menu.png"));
    }
    let v = open.drawn.clone().unwrap_or_else(|| view(rt, &mut sim, open));
    // The "How many?" prompt has the keys while it is open (module notes).
    if let Some((amount, most)) = open.quantity {
        use crate::menu::model::Command;
        let mut to = amount;
        let (mut make, mut back) = (false, false);
        for (i, b) in &amount_buttons {
            if *i == PointerState::Pressed {
                match b.0 {
                    Command::AmountStep(d) => to += d,
                    Command::AmountOk => make = true,
                    Command::AmountCancel => back = true,
                    _ => {}
                }
            }
        }
        for (i, at) in &meter {
            if let (PointerState::Pressed, Some(p)) = (i, at.normalized) {
                to = crate::menu::model::amount_at(p.x + 0.5, most);
            }
        }
        let wheel = scroll.as_ref().map_or(0.0, |s| s.delta.y);
        if keys.any_just_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) || wheel > 0.0 || act("more") {
            to += 1;
        }
        if keys.just_pressed(KeyCode::ArrowLeft) || wheel < 0.0 || act("less") {
            to -= 1;
        }
        let to = to.clamp(1, most.max(1));
        if to != amount {
            open.quantity = Some((to, most));
            sounds.write(UiSound { menu: UiMenu::Recipe, event: UiEvent::Focus });
        }
        if keys.any_just_pressed([KeyCode::KeyE, KeyCode::Escape]) || back || act("cancel") || act("exit") {
            info!("recipe menu: how many? cancelled");
            open.quantity = None;
            sounds.write(UiSound { menu: UiMenu::Recipe, event: UiEvent::Back });
            return;
        }
        if keys.any_just_pressed([KeyCode::KeyA, KeyCode::Enter, KeyCode::NumpadEnter]) || make || act("accept") {
            open.quantity = None;
            let chosen = v.rows.get(open.selected.min(v.rows.len().saturating_sub(1))).map(|r| r.recipe);
            if let Some(recipe) = chosen.and_then(|f| rt.game.data.recipes.get(&f)) {
                make_recipe(rt, &mut sim, recipe, to as u32, &mut sounds, corner.as_deref_mut());
                follow_made(rt, &mut sim, open, recipe.form);
            }
            open.drawn = None;
        }
        return;
    }
    let n = v.rows.len();
    let before = (open.selected, open.filter);
    let mut clicked = None;
    for (i, b) in &buttons {
        match (i, b) {
            (PointerState::Pressed, b) => clicked = Some(*b),
            (PointerState::Hovered, RecipeButton::Row(r)) if pointer_moved => open.selected = *r,
            _ => {}
        }
    }
    let wheel = scroll.as_ref().map_or(0.0, |s| s.delta.y);
    if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || wheel > 0.0 || act("up") {
        open.selected = open.selected.min(n.saturating_sub(1)).saturating_sub(1);
    }
    if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || wheel < 0.0 || act("down") {
        open.selected = (open.selected + 1).min(n.saturating_sub(1));
    }
    let filters = v.filters + 1;
    if keys.just_pressed(KeyCode::ArrowRight) || clicked == Some(RecipeButton::Right) || act("filter") {
        open.filter = (open.filter + 1) % filters;
        open.selected = 0;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) || clicked == Some(RecipeButton::Left) {
        open.filter = (open.filter + filters - 1) % filters;
        open.selected = 0;
    }
    if let Some(RecipeButton::Row(r)) = clicked {
        open.selected = r;
    }
    open.first = keep_in_view(open.first, open.selected, n, ROWS);
    if (open.selected, open.filter) != before {
        sounds.write(UiSound { menu: UiMenu::Recipe, event: if open.filter != before.1 { UiEvent::Switch } else { UiEvent::Focus } });
    }
    if keys.any_just_pressed([KeyCode::KeyX, KeyCode::Escape, KeyCode::Tab]) || clicked == Some(RecipeButton::Exit) || act("exit") {
        info!("recipe menu: exit");
        sounds.write(UiSound { menu: UiMenu::Recipe, event: UiEvent::Back });
        next.set(PlayState::Playing);
        return;
    }
    if keys.any_just_pressed([KeyCode::KeyA, KeyCode::Enter, KeyCode::NumpadEnter]) || clicked == Some(RecipeButton::Accept) || act("accept") {
        let chosen = v.rows.get(open.selected.min(n.saturating_sub(1))).map(|r| r.recipe);
        let Some(recipe) = chosen.and_then(|f| rt.game.data.recipes.get(&f)) else { return };
        // Any can be made: how many? (module notes)
        let most = crate::script::with_world(rt, &mut sim, |rt| {
            if !lacks(rt, recipe).is_empty() {
                return 0;
            }
            let counts: Vec<(FormId, i64)> = recipe.ingredients.iter().map(|&(i, _)| (i, rt.game.world.item_count(FormId::PLAYER_REF, i).unwrap_or(0) as i64)).collect();
            times_possible(recipe, |i| counts.iter().find(|c| c.0 == i).map_or(0, |c| c.1))
        });
        if most >= 1 {
            info!("recipe menu: how many? 1 of {most} ({:?})", recipe.name);
            open.quantity = Some((1, most.min(i32::MAX as u32) as i32));
            sounds.write(UiSound { menu: UiMenu::Recipe, event: UiEvent::Opened });
            return;
        }
        make_recipe(rt, &mut sim, recipe, 1, &mut sounds, corner.as_deref_mut());
        follow_made(rt, &mut sim, open, recipe.form);
        // What the player has changed: draw it again.
        open.drawn = None;
    }
}

/// After making `recipe` the ready recipes may have moved (they are listed first): the
/// choice stays on it, kept in view.
fn follow_made(rt: &mut crate::script::Runtime, sim: &mut SimState, open: &mut Open, recipe: FormId) {
    let v = view(rt, sim, open);
    if let Some(i) = v.rows.iter().position(|r| r.recipe == recipe) {
        open.selected = i;
        open.first = keep_in_view(open.first, i, v.rows.len(), ROWS);
    }
}

/// Make `recipe` `times` times: the ingredients go, the outputs come, an "added" line
/// each in the corner (module notes).
fn make_recipe(rt: &mut crate::script::Runtime, sim: &mut SimState, recipe: &RecipeDef, times: u32, sounds: &mut MessageWriter<UiSound>, corner: Option<&mut crate::corner_messages::CornerMessages>) {
    let forms = sim.forms;
    let made = crate::script::with_world(rt, sim, |rt| {
        let skill = recipe.skill.map_or(0.0, |(av, _)| fnv_game::functions::actor_value_capped(&mut rt.game, FormId::PLAYER_REF, av));
        let counts: Vec<(FormId, i64)> = recipe.ingredients.iter().map(|&(i, _)| (i, rt.game.world.item_count(FormId::PLAYER_REF, i).unwrap_or(0) as i64)).collect();
        let lack = missing(recipe, times, |_| skill, |i| counts.iter().find(|c| c.0 == i).map_or(0, |c| c.1));
        if !lack.is_empty() {
            return Err(format!("{lack:?}"));
        }
        for &(i, k) in &recipe.ingredients {
            rt.game.world.remove_item(FormId::PLAYER_REF, i, (k * times) as i32).map_err(|e| format!("{e:?}"))?;
        }
        let mut added = Vec::new();
        for &(i, k) in &recipe.outputs {
            rt.game.world.add_item(FormId::PLAYER_REF, i, (k * times) as i32).map_err(|e| format!("{e:?}"))?;
            added.push((i, k * times));
        }
        Ok(added)
    });
    match made {
        Ok(added) => {
            rt.game.count_stat(fnv_game::misc_stats::ITEMS_CRAFTED, times as f64);
            let ingredients: Vec<FormId> = recipe.ingredients.iter().map(|&(i, _)| i).collect();
            rt.game.challenge_crafted(&ingredients, times);
            // JIP's crafting event, once per output (`fnv_game::jip_events`).
            crate::script::with_world(rt, sim, |rt| {
                for &(i, k) in &added {
                    rt.game.item_crafted(i, i64::from(k));
                }
            });
            let what = added.iter().map(|(i, k)| format!("{k} x {}", crate::menu::model::display_name(forms, *i))).collect::<Vec<_>>().join(", ");
            if times > 1 {
                info!("recipe menu: made {times} x {:?} ({what})", recipe.name);
            } else {
                info!("recipe menu: made {:?} ({what})", recipe.name);
            }
            sounds.write(UiSound { menu: UiMenu::Recipe, event: UiEvent::Picked });
            // ToggleCraftingMessages 0 keeps them off (`fnv_game::nvse_jip6`).
            if let Some(c) = corner.filter(|_| rt.game.crafting_messages_shown()) {
                for (i, k) in added {
                    let name = crate::menu::model::display_name(forms, i);
                    c.push(if k > 1 { format!("{name} ({k}) added") } else { format!("{name} added") }.into());
                }
            }
        }
        Err(why) => {
            info!("recipe menu: cannot make {:?}: {why}", recipe.name);
            sounds.write(UiSound { menu: UiMenu::Recipe, event: UiEvent::Refused });
        }
    }
}

/// The menu's fonts and the arrow pictures, read once.
#[derive(Resource)]
struct RecipeLook {
    title: Face,
    value: Face,
    row: Face,
    arrows: Option<(Handle<Image>, Handle<Image>)>,
}

fn text(p: &mut ChildSpawnerCommands, f: &Face, s: &str, x: f32, top: f32, c: Color) {
    if let Some(line) = f.font.lines(s, None).into_iter().next() {
        set_line(p, f, &line, x, top + f.font.size * 0.8, c);
    }
}

fn wrapped(p: &mut ChildSpawnerCommands, f: &Face, s: &str, x: f32, top: f32, width: f32, c: Color) -> f32 {
    let lines = f.font.lines(s, Some(width));
    for (i, line) in lines.iter().enumerate() {
        set_line(p, f, line, x, top + f.font.size * (0.8 + i as f32 * 0.9), c);
    }
    lines.len().max(1) as f32 * f.font.size * 0.9
}

fn at(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: u(x), top: u(y), width: u(w), height: u(h), ..default() }
}

/// A line of the brackets.
fn rule(p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32) {
    p.spawn((at(x, y, w, h), BackgroundColor(amber(200.0))));
}

/// The two boxes' brackets: a line along the top with short sides going down, and one
/// along the bottom with short sides going up [documented: recipe_menu.xml headline and
/// footer lines, 40 high sides].
fn brackets(p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, bottom: f32, title_gap: (f32, f32)) {
    rule(p, x, y, title_gap.0, LINE);
    rule(p, x + title_gap.1, y, w - title_gap.1, LINE);
    rule(p, x, y, LINE, 40.0);
    rule(p, x + w - LINE, y, LINE, 40.0);
    rule(p, x, bottom - 40.0, LINE, 40.0);
    rule(p, x, bottom, w, LINE);
    rule(p, x + w - LINE, bottom - 40.0, LINE, 40.0);
}

#[allow(clippy::too_many_arguments)]
fn spawn(commands: &mut Commands, look: &RecipeLook, v: &View, words: &[String; 2], kit: &mut crate::menu::look::Kit, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool, glyphs: &crate::pad::PadGlyphs) {
    let left = Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(u(-MAIN.x / 2.0)), top: crate::interface::from_top((960.0 - MAIN.y) / 2.0, 0.5), width: u(MAIN.x), height: u(MAIN.y), ..default() };
    commands.spawn((RecipeRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, GlobalZIndex(crate::interface::TOP_MENU_Z), crate::interface::top_menu_backing())).with_children(|root| {
        root.spawn(left).with_children(|m| {
            let bright = color(Shade::Bright);
            // The list's box: "◀ RECIPES ▶" in the top line.
            let tw = look.title.font.width(&v.title);
            brackets(m, ITEMS_AT.x, ITEMS_AT.y, BOX_WIDTH, ITEMS_AT.y + 480.0, (25.0, 50.0 + tw + 40.0));
            if let Some((l, r)) = &look.arrows {
                m.spawn((Button, RecipeButton::Left, at(ITEMS_AT.x + 30.0, ITEMS_AT.y - 17.0, 17.5, 35.0), ImageNode { color: amber(255.0), ..ImageNode::new(l.clone()) }));
                m.spawn((Button, RecipeButton::Right, at(ITEMS_AT.x + 55.0 + tw, ITEMS_AT.y - 17.0, 17.5, 35.0), ImageNode { color: amber(255.0), ..ImageNode::new(r.clone()) }));
            }
            m.spawn(at(ITEMS_AT.x + 50.0, ITEMS_AT.y - 28.0, tw + 4.0, 56.0)).with_children(|t| text(t, &look.title, &v.title, 0.0, 0.0, bright));
            // The rows from the first shown.
            for (k, (i, r)) in v.rows.iter().enumerate().skip(v.first).take(ROWS).enumerate() {
                let (line, fill) = highlight_colors();
                let chosen = i == v.selected;
                let node = Node { border: UiRect::all(u(LINE)), ..at(ITEMS_AT.x + 5.0, LIST_TOP + k as f32 * ROW, BOX_WIDTH - 15.0 - ROWS_BAR_W, ROW) };
                m.spawn((Button, RecipeButton::Row(i), node, BorderColor::all(if chosen { line } else { Color::NONE }), BackgroundColor(if chosen { fill } else { Color::NONE }))).with_children(|row| {
                    text(row, &look.row, &r.label, 40.0, 6.0, if r.ready { bright } else { amber(NOT_READY) });
                });
            }
            // The scroll bar, right of the rows, when they do not all fit (owner's request
            // 2026-10-26; drawn as the container menu's).
            let bar = (ITEMS_AT.x + BOX_WIDTH - 5.0 - ROWS_BAR_W, LIST_TOP, ROWS as f32 * ROW);
            crate::menu::look::spawn_rows_bar(m, kit, data, images, bc, glyphs, bar, v.rows.len(), v.first, ROWS);
            if v.rows.is_empty() {
                m.spawn(at(ITEMS_AT.x + 5.0, LIST_TOP, BOX_WIDTH, ROW)).with_children(|t| text(t, &look.row, "No recipes", 40.0, 6.0, color(Shade::Dim)));
            }
            // The output's picture.
            if let Some(icon) = v.icon.as_deref() {
                let path = format!("textures\\{}", icon.trim_start_matches("textures\\").trim_start_matches("Textures\\"));
                if let Some((img, _)) = data.and_then(|d| crate::interface::texture(&path, d, images, bc)) {
                    m.spawn((at(ICON_AT.x, ICON_AT.y, ICON_SIZE, ICON_SIZE), ImageNode { color: amber(255.0), ..ImageNode::new(img) }));
                }
            }
            // Its effects, under the WG and VAL cards.
            if let Some(e) = v.effects.as_deref() {
                m.spawn(at(340.0, 690.0, 290.0, 100.0)).with_children(|t| {
                    text(t, &look.row, "EFFECTS", 6.0, 0.0, bright);
                    wrapped(t, &look.row, e, 6.0, 36.0, 280.0, bright);
                });
            }
            // The output's weight and value.
            let wg = v.weight.map_or("--".to_string(), |w| format!("{w:.1}").trim_end_matches(".0").to_string());
            let val = v.value.map_or("--".to_string(), |x| x.to_string());
            for (k, (name, s)) in [("WG", wg), ("VAL", val)].into_iter().enumerate() {
                let x = 340.0 + k as f32 * 150.0;
                rule(m, x, 640.0, 140.0, LINE);
                rule(m, x + 140.0 - LINE, 640.0, LINE, 30.0);
                m.spawn(at(x, 645.0, 140.0, 50.0)).with_children(|t| {
                    text(t, &look.row, name, 6.0, 0.0, bright);
                    let w = look.row.font.width(&s);
                    text(t, &look.row, &s, 130.0 - w, 0.0, bright);
                });
            }
            // The recipe's box.
            let rx = RIGHT_AT.x;
            let mw = look.title.font.width("Made at ...");
            brackets(m, rx, RIGHT_AT.y, BOX_WIDTH, RIGHT_AT.y + 515.0, (25.0, 50.0 + mw + 20.0));
            m.spawn(at(rx, RIGHT_AT.y - 28.0, BOX_WIDTH, 600.0)).with_children(|b| {
                text(b, &look.title, "Made at ...", 50.0, 0.0, bright);
                text(b, &look.value, &v.made_at, 60.0, 78.0, bright);
                if let Some((name, have, need)) = &v.skill {
                    text(b, &look.title, "Skill Requirement", 50.0, 128.0, bright);
                    let ok = have >= need;
                    text(b, &look.value, &format!("{name} ({have}/{need})"), 60.0, 188.0, if ok { bright } else { amber(NOT_READY) });
                }
                text(b, &look.title, "Ingredients", 50.0, 238.0, bright);
                let mut y = 298.0;
                for (name, have, need) in &v.ingredients {
                    let ok = have >= need;
                    y += wrapped(b, &look.row, &format!("{name} ({have}/{need})"), 110.0, y, 340.0, if ok { bright } else { amber(NOT_READY) }) + 8.0;
                }
            });
            // Accept and Exit [observed: "Accept  A)", "Exit  X)"].
            let ready = v.rows.get(v.selected).is_some_and(|r| r.ready);
            let hint_at = crate::pad::at_menu(fnv_game::crafting::RECIPE_MENU);
            let accept = format!("Accept  {}", crate::pad::hint_as(&[KeyCode::KeyA], hint_at, v.labels));
            let exit = format!("Exit  {}", crate::pad::hint_as(&[KeyCode::KeyX, KeyCode::Escape], hint_at, v.labels));
            for (k, (label, b, on)) in [(accept.as_str(), RecipeButton::Accept, ready), (exit.as_str(), RecipeButton::Exit, true)].into_iter().enumerate() {
                let w = crate::pad::hint_width(&look.row, label, glyphs);
                m.spawn((Button, b, at(rx + BOX_WIDTH - w - 10.0, BUTTONS_TOP + k as f32 * 48.0, w + 10.0, 44.0))).with_children(|t| {
                    crate::pad::set_hint(t, &look.row, label, 0.0, look.row.font.size * 0.8, if on { bright } else { amber(NOT_READY) }, glyphs);
                });
            }
        });
        // The "How many?" prompt over the menu (module notes).
        if let Some((amount, most)) = v.quantity {
            let screen = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() };
            let q = crate::menu::model::Quantity { cmd: crate::menu::model::Command::One, side: crate::menu::model::Side::Player, row: 0, max: most, amount };
            root.spawn(screen).with_children(|s| crate::menu::look::spawn_quantity(s, &q, words, kit, data, images, bc, glyphs));
        }
    });
}

/// Draw the menu again when what it shows changed.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    mut menu: ResMut<RecipeMenu>,
    script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    roots: Query<Entity, With<RecipeRoot>>,
    look: Option<Res<RecipeLook>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    glyphs: Res<crate::pad::PadGlyphs>,
    play: Option<Res<crate::PlayWorld>>,
    (mut forms, mut kit): (Local<Option<EffectForms>>, Local<Option<crate::menu::look::Kit>>),
) {
    let Some(open) = menu.0.as_mut() else { return };
    let Some(look) = look else {
        let bc = bc_supported(formats.as_deref());
        let font = |n: usize, images: &mut Assets<Image>| face(crate::pipboy::frame::FONTS[n - 1], plain_font(30.0, 14.0), data.as_deref(), images, bc);
        let pic = |p: &str, images: &mut Assets<Image>| data.as_deref().and_then(|d| crate::interface::texture(p, d, images, bc)).map(|x| x.0);
        let arrows = pic("textures\\interface\\main\\main_arrow_left.dds", &mut images).zip(pic("textures\\interface\\main\\main_arrow_right.dds", &mut images));
        commands.insert_resource(RecipeLook { title: font(TITLE_FONT, &mut images), value: font(VALUE_FONT, &mut images), row: font(ROW_FONT, &mut images), arrows });
        return;
    };
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    let mut v = view(rt, &mut sim, open);
    if forms.is_none() {
        *forms = play.as_ref().map(|p| crate::pipboy::stats::read::Forms::new(p.lo, p.schema));
    }
    let output = v.rows.get(v.selected).and_then(|r| rt.game.data.recipes.get(&r.recipe)).and_then(|r| r.outputs.first()).map(|o| o.0);
    v.effects = output.zip(forms.as_mut()).and_then(|(o, f)| effects_of(&sim, f, o));
    if open.drawn.as_ref() == Some(&v) && !roots.is_empty() {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    let bc = bc_supported(formats.as_deref());
    let kit = kit.get_or_insert_with(|| crate::menu::look::Kit::load(data.as_deref(), &mut images, bc));
    let words = crate::menu::look::quantity_words(Some(&rt.game));
    spawn(&mut commands, &look, &v, &words, kit, data.as_deref(), &mut images, bc, &glyphs);
    if v.effects.is_some() && open.drawn.as_ref().is_none_or(|d| d.selected != v.selected) {
        info!("recipe menu: {} effects: {}", v.rows.get(v.selected).map_or("", |r| r.label.as_str()), v.effects.as_deref().unwrap_or(""));
    }
    open.selected = v.selected;
    open.drawn = Some(v);
}

/// An item's effects as the Pip-Boy's ITEMS page writes them: an aid item's own, an
/// apparel's or a weapon's enchantment's.
fn effects_of(sim: &SimState, forms: &mut crate::pipboy::stats::read::Forms, item: FormId) -> Option<String> {
    use crate::pipboy::stats::model::{changes_text, Change};
    let aid = sim.forms.item(item).is_some_and(|it| it.kind == fnv_sim::forms::ItemKind::Aid);
    let ench = if aid { Some(item.0) } else { forms.item_ench(item.0) }?;
    let e = forms.ench(ench);
    if e.changes.is_empty() {
        return None;
    }
    let changes: Vec<Change> = e.changes.iter().map(|&(code, amount, detrimental)| Change { value: forms.short_name(code), amount, detrimental }).collect();
    Some(changes_text(&changes))
}

fn close(mut commands: Commands, mut menu: ResMut<RecipeMenu>, roots: Query<Entity, With<RecipeRoot>>, script: Option<ResMut<ScriptState>>) {
    if menu.0.take().is_none() {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    if let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) {
        if rt.game.menu == Some(RECIPE_MENU) {
            rt.game.close_menu();
        }
    }
    info!("recipe menu: closed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_that_can_be_made_is_set_by_the_scarcest_ingredient() {
        let r = RecipeDef { ingredients: vec![(FormId(1), 2), (FormId(2), 1)], ..Default::default() };
        let have = |i: FormId| if i == FormId(1) { 7 } else { 5 };
        assert_eq!(times_possible(&r, have), 3);
        assert_eq!(times_possible(&r, |_| 0), 0);
        assert_eq!(times_possible(&RecipeDef::default(), |_| 0), 1);
    }

    #[test]
    fn a_label_says_how_many_are_made_once() {
        let r = |name: &str, n: u32| RecipeDef { name: name.into(), outputs: vec![(FormId(1), n)], ..Default::default() };
        assert_eq!(label(&r("Antivenom", 2)), "Antivenom (2)");
        assert_eq!(label(&r("Conversion, Fission Battery to SEC (25)", 25)), "Conversion, Fission Battery to SEC (25)");
        assert_eq!(label(&r("Battle Brew", 1)), "Battle Brew");
    }
}
