//! The Pip-Boy's repair (docs/spec-notes/repair.md §5; the rules are `fnv_game::repair`):
//! on the ITEMS page, R on a worn weapon or piece of armour lists the items whose parts
//! can go into it (the same kind, or one on its repair list); choosing one scraps it and
//! raises the item's condition by the player's Repair skill.
//!
//! - The page's hint: "Repair R)" or, once the item works as well as it can, "Maintain R)",
//!   right-aligned at the top right under the heading; dim when nothing can repair it
//!   [observed: the owner's screenshots, C:\fnv-private\repairmenuPlayer; documented: the
//!   inventory file's `IM_RepairButton` at (855, 45), `sInventoryRepair`, PC key R]. An
//!   item at full condition cannot be repaired [owner's rule]: its hint is dim and R does
//!   nothing. Full is `SHOWN_FULL`, as for the vendor, so the 99.9 % that AddItem gives
//!   counts as full [design: `fnv_game::repair::vendor_lists`].
//! - The repair list [documented: `menus\repair_menu.xml`, in the menus' rectangle]: the
//!   heading "ITEMS" with "Repair Skill" as a card 220 wide at the right; the list 15 left
//!   and 75 down, 430 wide, its rows 32 in and 393 wide, the name in font 2 at 40, 20
//!   down, wrapped at 270, the condition bar 60 x 16 at 320, the equipped square 25 at
//!   15; the item's picture 400 square at (450, 20); the figures from (415, 400), 427 x
//!   180: the condition and the stat as cards of half that width, the arrow 80 square in
//!   the middle, the gains in font 4 at a quarter and three quarters, 30 under the
//!   arrow's top, the cards after the repair at the bottom; "Cancel E)" right-aligned at
//!   855, 75 down. The list's first row is the item being repaired, boxed; the others are
//!   the parts [observed: the screenshots]. On the item itself only its figures show; on
//!   a part, the figures before and after with the gains ("+9 %", "+0").
//! - Keys: Up/Down (W/S) choose, Enter or Space uses the chosen part, E or Esc goes back to
//!   the list [documented: `_PCButton_E` RM_CancelButton; design: the rest].
//! - The pointer [design: as the ITEMS list's rows, `items.rs`]: a row under the pointer is
//!   chosen, a click on a row chooses it and a click on the chosen part uses it; a click on
//!   "Cancel E)" goes back. Log: `pipboy repair: clicked row <n>`.
//! - After a repair the list stays open on the same item for the next repair; it closes
//!   (the ITEMS list comes back) when the item is at full condition or nothing is left to
//!   repair it with, or on Cancel [owner's rule, playtest list 7 item 16]. Log
//!   `pipboy repair: stays open: …` or `pipboy repair: closed: …`.
//! - The repair list's rows are silent [documented: the template's `&nosound;`]; a repair
//!   plays `UIRepairWeapon` (`crate::ui_sounds`, as the vendor's).
//! - Test aid: the Pip-Boy aid's `repair=open|down|up|use|cancel` (`test_aid.rs`).

use super::frame::{self, Align, Card, Ctx, LINE, MAIN, TEXT};
use super::{Page, PipBoy};
use crate::interface::{pipboy_amber as amber, at};
use crate::menu::model::{Mode, Side};
use crate::menu::ActiveMenu;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
use bevy::prelude::*;
use fnv_game::repair::{gain_percent, hint, merged_condition, repairs_with, Gear, MergeSettings, SHOWN_FULL};
use fnv_sim::forms::{BaseData, FormStore, ItemKind};
use fnv_sim::inventory::{Extra, Pick};
use fnv_sim::FormId;

/// The list and its rows [documented: `RM_RepairList`, the template].
const LIST: Rect = Rect { min: Vec2::new(-15.0, 75.0), max: Vec2::new(415.0, 543.0) };
const ROW_X: f32 = 32.0;
const ROW_W: f32 = 393.0;
const NAME_X: f32 = 40.0;
const NAME_Y: f32 = 20.0;
const WRAP: f32 = 270.0;
const BAR: Rect = Rect { min: Vec2::new(320.0, 20.0), max: Vec2::new(380.0, 36.0) };
const MARK: Rect = Rect { min: Vec2::new(15.0, 17.5), max: Vec2::new(40.0, 42.5) };
/// Rows as the ITEMS list's [hypothesis: the barter menu's measure, barter-menu.md §3].
const ROW_H: f32 = super::items::ROW_H;
/// The rows the keys keep the choice among: the window moves when the choice would leave
/// the eight from the first shown (as many single-line rows as the list holds).
const SHOWN: usize = 8;
const LINE_PITCH: f32 = 35.0;
/// The picture and the figures [documented: `RM_ItemIcon`, `RM_CompareRect`].
const ICON: Rect = Rect { min: Vec2::new(450.0, 20.0), max: Vec2::new(850.0, 420.0) };
const COMPARE: Rect = Rect { min: Vec2::new(415.0, 400.0), max: Vec2::new(842.0, 580.0) };
const CARD_H: f32 = 60.0;
const ARROW: f32 = 80.0;
/// The meter in a condition card: 60 x 16, 15 from its right end, 3 above the title's
/// line [documented: `RM_BrokenItemHealthMeter`].
const METER: Vec2 = Vec2::new(60.0, 16.0);
/// The heading's skill card and the buttons [documented: `RM_Headline_PlayerSkillInfo`,
/// `RM_ButtonRect`].
const SKILL_W: f32 = 220.0;
const BUTTONS: Vec2 = Vec2::new(855.0, 75.0);
/// The ITEMS page's Repair button [documented: inventory_menu.xml `IM_ButtonRect`].
const HINT: Vec2 = Vec2::new(855.0, 45.0);
/// The bars' empty part [documented: the template's alpha 64].
const BAR_BACK: f32 = 64.0 / 255.0;

/// A step from the keys or the test aid.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairInput {
    Open,
    Up,
    Down,
    Use,
    Cancel,
    /// The pointer came onto the row at this place (0 the item, then its parts).
    Hover(usize),
    /// A click on the row at this place: chooses it, or uses the part already chosen.
    Click(usize),
}

/// A row of the repair list under the pointer.
#[derive(Component, Clone, Copy)]
struct RepairRow(usize);

/// "Cancel E)" under the pointer.
#[derive(Component, Clone, Copy)]
struct RepairCancel;

/// Only a row not chosen yet: choosing redraws the list, and the new row under the pointer
/// would come over it again every frame, so a press and its release never met one row.
fn on_row_over(over: On<PointerOver>, rows: Query<&RepairRow>, repair: Res<PipRepair>, mut out: MessageWriter<RepairInput>) {
    let (Ok(r), Some(s)) = (rows.get(over.entity), repair.0.as_ref()) else { return };
    if s.selected != r.0 {
        out.write(RepairInput::Hover(r.0));
    }
}

fn on_row_click(click: On<PointerClick>, rows: Query<&RepairRow>, mut out: MessageWriter<RepairInput>) {
    if let Ok(r) = rows.get(click.entity) {
        out.write(RepairInput::Click(r.0));
    }
}

fn on_cancel_click(click: On<PointerClick>, cancel: Query<&RepairCancel>, mut out: MessageWriter<RepairInput>) {
    if cancel.get(click.entity).is_ok() {
        out.write(RepairInput::Cancel);
    }
}

/// What a step does with the list as it is: a click on the chosen part uses it, any
/// other click chooses its row.
pub fn resolve(selected: Option<usize>, step: RepairInput) -> RepairInput {
    match (selected, step) {
        (Some(s), RepairInput::Click(i)) if i == s && i > 0 => RepairInput::Use,
        (_, RepairInput::Click(i)) => RepairInput::Hover(i),
        (_, x) => x,
    }
}

/// The repair list while it is shown.
#[derive(Resource, Default)]
pub struct PipRepair(pub Option<Session>);

impl PipRepair {
    pub fn is_open(&self) -> bool {
        self.0.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub offer: Offer,
    /// The player's Repair skill when the list opened, and the settings.
    pub skill: f32,
    pub settings: MergeSettings,
    /// The list's first row shown: moved only to keep the choice in view (the keys), so
    /// the pointer never scrolls the list.
    pub first: usize,
    /// 0: the item itself; else the part at this place, less one.
    pub selected: usize,
}

/// A part: a unit group of the player's inventory that can go into the item.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub item: FormId,
    pub extra: Extra,
    pub name: String,
    pub condition: f32,
}

/// What the ITEMS page's selected row offers.
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub item: FormId,
    pub extra: Extra,
    pub name: String,
    pub gear: Gear,
    pub condition: f32,
    pub icon: Option<String>,
    pub parts: Vec<Part>,
}

impl Offer {
    /// Whether R does anything: below full condition, with something to use.
    pub fn usable(&self) -> bool {
        self.condition < SHOWN_FULL && !self.parts.is_empty()
    }
}

fn gear_of(forms: &FormStore, item: FormId) -> Option<(Gear, Option<String>)> {
    match forms.get(item).map(|b| &b.data) {
        Some(BaseData::Item(it)) if it.health.is_some_and(|h| h > 0) => match it.kind {
            ItemKind::Weapon => Some((Gear::Weapon, it.icon.clone())),
            ItemKind::Armor if !it.non_playable => Some((Gear::Armor, it.icon.clone())),
            _ => None,
        },
        _ => None,
    }
}

/// What the row of `item` whose units `pick` names (their condition and mods, as the ITEMS
/// model has them) offers: the unit (the equipped one first when `equipped`) and the parts
/// for it.
pub fn offer(groups: &[(FormId, Extra, i32)], forms: &FormStore, repair_list: &[FormId], item: FormId, pick: Pick, equipped: bool) -> Option<Offer> {
    let (gear, icon) = gear_of(forms, item)?;
    let mine = groups.iter().filter(|g| g.0 == item && g.1.picked(pick)).max_by_key(|g| g.1.equipped == equipped)?;
    let (target, condition) = (mine.1, mine.1.condition_or_whole());
    let mut parts: Vec<Part> = Vec::new();
    for &(i, extra, count) in groups {
        // The unit being repaired is not a part of itself; neither are worn units
        // [design] or units at full condition [owner's rule].
        let spare = if i == item && extra == target { count - 1 } else { count };
        if spare <= 0 || extra.equipped || extra.condition_or_whole() >= SHOWN_FULL || gear_of(forms, i).is_none() || !repairs_with(item, i, repair_list) {
            continue;
        }
        let name = crate::menu::model::modded_name(crate::menu::model::display_name(forms, i), extra.mods);
        parts.push(Part { item: i, extra, name: if spare > 1 { format!("{name} ({spare})") } else { name }, condition: extra.condition_or_whole() });
    }
    parts.sort_by(|a, b| a.name.cmp(&b.name).then(b.condition.total_cmp(&a.condition)));
    Some(Offer { item, extra: target, name: crate::menu::model::display_name(forms, item), gear, condition, icon, parts })
}

/// The repair list after a repair raised the item to `to`: the same unit with what can
/// still go into it, or `None` when the list closes: the item is at full condition or
/// nothing is left to repair it with [owner's rule, playtest list 7 item 16: the list
/// stays open for repeated repairs of the item; it also closes on Cancel].
pub fn after_repair(groups: &[(FormId, Extra, i32)], forms: &FormStore, repair_list: &[FormId], o: &Offer, to: f32) -> Option<Offer> {
    if to >= SHOWN_FULL {
        return None;
    }
    offer(groups, forms, repair_list, o.item, Pick::row(Some(to), o.extra.mods), o.extra.equipped).filter(Offer::usable)
}

/// The ITEMS page's selected row's offer, now.
pub fn current(active: &ActiveMenu, sim: &mut SimState, script: Option<&ScriptState>) -> Option<Offer> {
    let m = active.0.as_ref().filter(|m| m.mode == Mode::Inventory)?;
    let r = m.list(Side::Player).rows.get(m.selected[Side::Player as usize])?;
    let forms = sim.forms;
    let groups: Vec<(FormId, Extra, i32)> = sim.world.inventory_mut(FormId::PLAYER_REF).map(|i| i.groups().collect()).unwrap_or_default();
    // The item's repair list, and with Jury Rigging the like items carried
    // (`fnv_game::Game::player_repair_list`, perk-entry-points.md §9).
    let list = script.and_then(|s| s.rt.as_ref()).map(|rt| rt.game.player_repair_list(r.item, groups.iter().map(|g| g.0))).unwrap_or_default();
    offer(&groups, forms, &list, r.item, r.pick(), r.equipped)
}

/// The keys of the repair list and R on the ITEMS page.
#[allow(clippy::too_many_arguments)]
pub fn keys(
    pip: Res<PipBoy>,
    keys: Res<ButtonInput<KeyCode>>,
    mut aid: MessageReader<RepairInput>,
    mut repair: ResMut<PipRepair>,
    mut active: ResMut<ActiveMenu>,
    mut sim: ResMut<SimState>,
    script: Option<ResMut<ScriptState>>,
    mut sounds: MessageWriter<UiSound>,
    prompt: Res<crate::menu::PromptKeys>,
    modding: Res<super::item_mod::PipItemMod>,
) {
    // Laid out in the Pip-Boy's screen texture, at its size whatever the UI scale (#1631).
    let _canvas = crate::interface::original_units();
    use KeyCode::*;
    let mut steps: Vec<RepairInput> = aid.read().copied().collect();
    let items = pip.open() && pip.page == Page::Items && active.0.as_ref().is_some_and(|m| m.mode == Mode::Inventory);
    if !items {
        if repair.0.take().is_some() {
            info!("pipboy repair: closed with the Pip-Boy");
        }
        return;
    }
    // The mod menu has the keys while it is shown (`super::item_mod`).
    if prompt.held(&active) || modding.is_open() {
        return;
    }
    for (k, s) in [(&[KeyR, KeyR][..], RepairInput::Open), (&[ArrowUp, KeyW], RepairInput::Up), (&[ArrowDown, KeyS], RepairInput::Down), (&[Enter, NumpadEnter, Space], RepairInput::Use), (&[KeyE, Escape], RepairInput::Cancel)] {
        if keys.any_just_pressed(k.iter().copied()) {
            steps.push(s);
        }
    }
    let mut script = script.map(|s| s.into_inner());
    for step in steps {
        if let RepairInput::Click(i) = step {
            info!("pipboy repair: clicked row {i}");
        }
        let step = resolve(repair.0.as_ref().map(|s| s.selected), step);
        match (repair.0.as_mut(), step) {
            (None, RepairInput::Open) => {
                let Some(o) = current(&active, &mut sim, script.as_deref()) else { continue };
                if !o.usable() {
                    info!("pipboy repair: {} at {}%: nothing to repair it with{}", o.name, fnv_combat::condition::percent(o.condition), if o.condition >= SHOWN_FULL { " (full condition)" } else { "" });
                    sounds.write(UiSound { menu: UiMenu::Repair, event: UiEvent::Refused });
                    continue;
                }
                let skill = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map_or(0.0, |rt| crate::script::with_world(rt, &mut sim, |rt| fnv_game::functions::actor_value_capped(&mut rt.game, FormId::PLAYER_REF, fnv_game::actor_values::REPAIR)) as f32);
                info!(
                    "pipboy repair: open {} at {}% (Repair {:.0}): {}",
                    o.name,
                    fnv_combat::condition::percent(o.condition),
                    skill,
                    o.parts.iter().map(|p| format!("{} {}%", p.name, fnv_combat::condition::percent(p.condition))).collect::<Vec<_>>().join(", ")
                );
                let settings = MergeSettings::from_settings(|n| sim.forms.setting_f32(n));
                repair.0 = Some(Session { offer: o, skill, settings, first: 0, selected: 0 });
            }
            (None, _) => {}
            (Some(s), RepairInput::Up) => {
                s.selected = s.selected.saturating_sub(1);
                s.first = crate::menu::model::keep_in_view(s.first, s.selected, s.offer.parts.len() + 1, SHOWN);
            }
            (Some(s), RepairInput::Down) => {
                s.selected = (s.selected + 1).min(s.offer.parts.len());
                s.first = crate::menu::model::keep_in_view(s.first, s.selected, s.offer.parts.len() + 1, SHOWN);
            }
            (Some(s), RepairInput::Hover(i)) => s.selected = i.min(s.offer.parts.len()),
            (Some(_), RepairInput::Click(_)) => {}
            (Some(_), RepairInput::Cancel) => {
                info!("pipboy repair: cancel");
                sounds.write(UiSound { menu: UiMenu::Repair, event: UiEvent::Back });
                repair.0 = None;
            }
            (Some(_), RepairInput::Open) => {}
            (Some(s), RepairInput::Use) => {
                let Some(part) = s.selected.checked_sub(1).and_then(|i| s.offer.parts.get(i)).cloned() else { continue };
                let o = &s.offer;
                let to = merged_condition(&s.settings, o.condition, part.condition, s.skill);
                let done = sim.world.inventory_mut(FormId::PLAYER_REF).is_ok_and(|inv| {
                    let taken: i32 = inv.take_at(part.item, 1, Pick::row(Some(part.condition), part.extra.mods)).iter().map(|g| g.1).sum();
                    taken == 1 && inv.set_condition_one(o.item, o.extra, to)
                });
                let mut next = None;
                if done {
                    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
                        rt.game.count_stat(fnv_game::misc_stats::ITEMS_REPAIRED, 1.0);
                    }
                    info!("pipboy repair: {} {}% with {} {}% -> {}% (Repair {:.0})", o.name, fnv_combat::condition::percent(o.condition), part.name, fnv_combat::condition::percent(part.condition), fnv_combat::condition::percent(to), s.skill);
                    sounds.write(UiSound { menu: UiMenu::Repair, event: UiEvent::Picked });
                    let groups: Vec<(FormId, Extra, i32)> = sim.world.inventory_mut(FormId::PLAYER_REF).map(|i| i.groups().collect()).unwrap_or_default();
                    let list = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| rt.game.player_repair_list(o.item, groups.iter().map(|g| g.0))).unwrap_or_default();
                    let after = after_repair(&groups, sim.forms, &list, o, to);
                    match &after {
                        Some(n) => info!("pipboy repair: stays open: {} at {}%, {} part(s) left", n.name, fnv_combat::condition::percent(n.condition), n.parts.len()),
                        None if to >= SHOWN_FULL => info!("pipboy repair: closed: {} is at full condition", o.name),
                        None => info!("pipboy repair: closed: nothing left to repair {} with", o.name),
                    }
                    next = after;
                } else {
                    info!("pipboy repair: {} with {}: the items are gone", o.name, part.name);
                    sounds.write(UiSound { menu: UiMenu::Repair, event: UiEvent::Refused });
                }
                match next {
                    Some(n) => {
                        s.selected = s.selected.min(n.parts.len());
                        s.offer = n;
                    }
                    None => repair.0 = None,
                }
                if let Some(m) = active.0.as_mut() {
                    if let Err(e) = m.refresh(&mut sim.world) {
                        warn!("pipboy repair: the list: {e:?}");
                    }
                }
            }
        }
    }
}

/// The ITEMS page's Repair or Maintain hint for the selected row.
pub fn draw_hint(p: &mut ChildSpawnerCommands, c: &mut Ctx, o: Option<&Offer>) {
    let Some(o) = o else { return };
    // R, with a controller Y (`inventory_menu.xml`'s Repair, `crate::pad::menu_buttons`).
    // The word: its setting's text when the load order defines it (`super::labels`, #1443).
    let word = match hint(o.gear, o.condition) {
        "Maintain" => super::labels::label("sMaintainItem", "Maintain"),
        w => super::labels::label("sInventoryRepair", w),
    };
    let label = format!("{} {}", word, crate::pad::hint(&[KeyCode::KeyR], crate::pad::at_menu(fnv_game::game::menus::INVENTORY)));
    info!("pipboy items: {} {label}{}", o.name, if o.usable() { "" } else { " (dim)" });
    let color = amber(if o.usable() { TEXT } else { frame::DIMMED });
    c.text(p, 2, &label, MAIN.min.x + HINT.x - 10.0, MAIN.min.y + HINT.y + frame::BUTTON_TEXT_Y, Align::Right, color);
}

/// The figure a card shows for an item at a condition: damage or DT by our condition
/// rules (combat-rules.md §17).
fn stat(combat: Option<&crate::combat::Combat>, o: &Offer, condition: f32) -> Option<(&'static str, i32)> {
    let c = combat?;
    match o.gear {
        Gear::Weapon => c.forms.weapons.get(&o.item).filter(|w| w.base_damage > 0).map(|w| (super::labels::label("sInventoryDamage", "DAM"), (w.base_damage as f32 * fnv_combat::damage::condition_mult(condition)).round() as i32)),
        Gear::Armor => c.forms.armor.get(&o.item).map(|a| (super::labels::label("sInventoryDamageThreshold", "DT"), (a.dt * fnv_combat::condition::armor_mult(condition)).round() as i32)),
    }
}

fn bar(p: &mut ChildSpawnerCommands, r: Rect, fill: f32, color: Color) {
    p.spawn((at(r.min.x, r.min.y, r.width(), r.height()), BackgroundColor(color.with_alpha(BAR_BACK))));
    p.spawn((at(r.min.x, r.min.y, r.width() * fill.clamp(0.0, 1.0), r.height()), BackgroundColor(color)));
}

/// The repair list in place of the ITEMS page's body.
pub fn draw_list(p: &mut ChildSpawnerCommands, c: &mut Ctx, s: &Session, combat: Option<&crate::combat::Combat>) {
    let bright = amber(TEXT);
    let line = amber(LINE);
    let o = &s.offer;
    frame::heading(p, c, Page::Items.title(), &[Card::new(MAIN.width() - SKILL_W, SKILL_W, "Repair Skill", format!("{:.0}", s.skill.max(0.0).floor()))]);
    let by = MAIN.min.y + BUTTONS.y + frame::BUTTON_TEXT_Y;
    let cancel = format!("Cancel {}", crate::pad::hint(&[KeyCode::KeyE, KeyCode::Escape], crate::pad::at_menu(fnv_game::game::menus::INVENTORY)));
    // The drawn width, not the text's: a hint that names a button draws its picture.
    let cw = c.text(p, 2, &cancel, MAIN.min.x + BUTTONS.x - 10.0, by, Align::Right, bright);
    p.spawn((at(MAIN.min.x + BUTTONS.x - 10.0 - cw, by - 5.0, cw + 10.0, 40.0), RepairCancel, Pickable::default())).observe(on_cancel_click);
    // The rows: the item, then its parts.
    let rows: Vec<(&str, f32, bool)> = std::iter::once((o.name.as_str(), o.condition, o.extra.equipped)).chain(o.parts.iter().map(|q| (q.name.as_str(), q.condition, q.extra.equipped))).collect();
    let x0 = MAIN.min.x + LIST.min.x + ROW_X;
    let mut y = MAIN.min.y + LIST.min.y;
    let first = s.first;
    for (i, (name, cond, worn)) in rows.iter().enumerate().skip(first) {
        let lines = c.lines(2, name, Some(WRAP));
        let h = ROW_H + LINE_PITCH * lines.len().saturating_sub(1) as f32;
        if y + h > MAIN.min.y + LIST.max.y {
            break;
        }
        if i == 0 || i == s.selected {
            c.boxed(p, Rect::new(x0, y, x0 + ROW_W, y + h), line, None);
        }
        if *worn {
            let mk = Rect::new(x0 + MARK.min.x, y + MARK.min.y, x0 + MARK.max.x, y + MARK.max.y);
            if c.image(p, "square_filled.dds", at(mk.min.x, mk.min.y, mk.width(), mk.height()), bright).is_none() {
                p.spawn((at(mk.min.x, mk.min.y, mk.width(), mk.height()), BackgroundColor(bright)));
            }
        }
        for (k, l) in lines.iter().enumerate() {
            c.set(p, 2, l, x0 + NAME_X, y + NAME_Y - frame::THICK + LINE_PITCH * k as f32, Align::Left, bright);
        }
        bar(p, Rect::new(x0 + BAR.min.x, y + BAR.min.y, x0 + BAR.max.x, y + BAR.max.y), *cond, bright);
        p.spawn((at(x0, y, ROW_W, h), RepairRow(i), Pickable::default())).observe(on_row_over).observe(on_row_click);
        y += h;
    }
    // The picture and the figures.
    if let Some(icon) = o.icon.as_deref() {
        let r = Rect::from_corners(MAIN.min + ICON.min, MAIN.min + ICON.max);
        c.image(p, icon, at(r.min.x, r.min.y, r.width(), r.height()), bright);
    }
    let cr = Rect::from_corners(MAIN.min + COMPARE.min, MAIN.min + COMPARE.max);
    let half = cr.width() / 2.0;
    let cards = |p: &mut ChildSpawnerCommands, c: &mut Ctx, y: f32, cond: f32| {
        frame::card(p, c, cr.min.x, y, half - 5.0, super::labels::label("sInventoryCondition", "CND"), "");
        let m = Rect::new(cr.min.x + half - 5.0 - METER.x - 15.0, y + 17.0, cr.min.x + half - 5.0 - 15.0, y + 17.0 + METER.y);
        bar(p, m, cond, bright);
        let (t, v) = stat(combat, o, cond).map_or(("", String::new()), |(t, v)| (t, v.to_string()));
        frame::card(p, c, cr.min.x + half + 5.0, y, half - 5.0, t, &v);
    };
    cards(p, c, cr.min.y, o.condition);
    let part = s.selected.checked_sub(1).and_then(|i| o.parts.get(i));
    if let Some(q) = part {
        let to = merged_condition(&s.settings, o.condition, q.condition, s.skill);
        let ay = cr.min.y + (cr.height() - ARROW) / 2.0 - 5.0;
        c.image(p, "interface\\shared\\arrow\\repair_arrow_down.dds", at(cr.min.x + (cr.width() - ARROW) / 2.0, ay, ARROW, ARROW), bright);
        c.text(p, 4, &format!("+{}%", gain_percent(o.condition, to)), cr.min.x + cr.width() / 4.0, ay + 30.0, Align::Centre, bright);
        let gain = stat(combat, o, to).zip(stat(combat, o, o.condition)).map_or(0, |(a, b)| a.1 - b.1);
        c.text(p, 4, &format!("+{gain}"), cr.min.x + cr.width() * 0.75, ay + 30.0, Align::Centre, bright);
        cards(p, c, cr.max.y - CARD_H, to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use fnv_sim::forms::{BaseForm, Item};

    fn store(items: &[(u32, ItemKind)]) -> FormStore {
        let mut f = FormStore::default();
        for &(id, kind) in items {
            let it = Item { kind, value: Some(10), money: None, weight: Some(1.0), slots: None, projectile: None, health: Some(100), non_playable: false, cant_drop: false, armor_class: Default::default(), power_armor: false, armor_dr: None, armor_dt: Some(5.0), weapon_damage: None, weight_mods: [0.0; 3], drawable: false, not_for_npcs: false, thrown: false, quest_item: false, icon: None, placed_count: None, teaches: None, cant_be_taken: false, modulates_voice: false, weapon_traits: None };
            f.bases.insert(FormId(id), BaseForm { id: FormId(id), sig: *b"ARMO", edid: None, name: Some(format!("Item {id}")), script: None, prompt: None, voice_filter: false, data: BaseData::Item(it) });
        }
        f
    }

    fn at(c: f32) -> Extra {
        Extra { condition: Some(c), ..Extra::default() }
    }

    #[test]
    fn the_parts_are_like_items_below_full_condition() {
        let (a, b, c, misc) = (FormId(1), FormId(2), FormId(3), FormId(4));
        let forms = store(&[(1, ItemKind::Armor), (2, ItemKind::Armor), (3, ItemKind::Armor), (4, ItemKind::Misc)]);
        let worn = Extra { equipped: true, ..at(0.5) };
        let groups = vec![(a, worn, 1), (a, at(0.3), 1), (b, at(0.4), 1), (b, Extra::default(), 1), (c, at(0.2), 1), (misc, Extra::default(), 3)];
        // Item 1 repaired by itself and item 2 (its list); item 3 is not on the list, and
        // item 2 at full condition is no part (owner's rule).
        let o = offer(&groups, &forms, &[a, b], a, Pick::at(Some(0.5)), true).unwrap();
        assert_eq!((o.extra, o.condition, o.gear), (worn, 0.5, Gear::Armor));
        let parts: Vec<(FormId, f32)> = o.parts.iter().map(|p| (p.item, p.condition)).collect();
        assert_eq!(parts, vec![(a, 0.3), (b, 0.4)]);
        assert!(o.usable());
        // Repairing the unworn one: the worn unit is no part.
        let o = offer(&groups, &forms, &[a, b], a, Pick::at(Some(0.3)), false).unwrap();
        assert_eq!(o.parts.iter().map(|p| p.item).collect::<Vec<_>>(), vec![b]);
        // Two units in one group: the other one is a part.
        let pair = vec![(c, at(0.2), 2)];
        let o = offer(&pair, &forms, &[], c, Pick::at(Some(0.2)), false).unwrap();
        assert_eq!(o.parts.len(), 1);
        // At full condition: nothing to do.
        let full = vec![(b, Extra::default(), 1), (b, at(0.4), 1)];
        assert!(!offer(&full, &forms, &[], b, Pick::at(Some(1.0)), false).unwrap().usable());
        // Not a weapon or armour.
        assert_eq!(offer(&groups, &forms, &[], misc, Pick::ANY, false), None);
    }

    #[test]
    fn the_list_stays_open_until_full_or_out_of_parts() {
        let (a, b) = (FormId(1), FormId(2));
        let forms = store(&[(1, ItemKind::Armor), (2, ItemKind::Armor)]);
        let worn = Extra { equipped: true, ..at(0.5) };
        let o = offer(&[(a, worn, 1), (b, at(0.4), 2)], &forms, &[b], a, Pick::at(Some(0.5)), true).unwrap();
        // One of the two parts used, the item now at 0.7: one part left, the list stays.
        let after = vec![(a, Extra { equipped: true, ..at(0.7) }, 1), (b, at(0.4), 1)];
        let n = after_repair(&after, &forms, &[b], &o, 0.7).unwrap();
        assert_eq!((n.condition, n.extra.equipped, n.parts.len()), (0.7, true, 1));
        // The last part used: nothing left, the list closes.
        let gone = vec![(a, Extra { equipped: true, ..at(0.9) }, 1)];
        assert_eq!(after_repair(&gone, &forms, &[b], &n, 0.9), None);
        // Full condition: the list closes though parts are left.
        let full = vec![(a, Extra { equipped: true, ..Extra::default() }, 1), (b, at(0.4), 1)];
        assert_eq!(after_repair(&full, &forms, &[b], &n, 1.0), None);
    }

    /// What AddItem gives a weapon or armour (`fnv_game::functions::items::ADDED_CONDITION`).
    const ADDED: f32 = 0.999;
    const JUST_BELOW: f32 = 0.994;

    #[test]
    fn an_item_shown_as_full_is_not_repaired() {
        let (a, b) = (FormId(1), FormId(2));
        let forms = store(&[(1, ItemKind::Armor), (2, ItemKind::Armor)]);
        // AddItem's 99.9 %, with a worn part to hand: full, R does nothing.
        let added = offer(&[(a, at(ADDED), 1), (b, at(0.5), 1)], &forms, &[b], a, Pick::at(Some(ADDED)), false).unwrap();
        assert_eq!(added.parts.len(), 1);
        assert!(!added.usable());
        assert!(!offer(&[(a, at(SHOWN_FULL), 1), (b, at(0.5), 1)], &forms, &[b], a, Pick::at(Some(SHOWN_FULL)), false).unwrap().usable());
        // Just below the line it can still be repaired.
        assert!(offer(&[(a, at(JUST_BELOW), 1), (b, at(0.5), 1)], &forms, &[b], a, Pick::at(Some(JUST_BELOW)), false).unwrap().usable());
    }

    #[test]
    fn an_item_shown_as_full_is_not_a_part() {
        let (a, b, c) = (FormId(1), FormId(2), FormId(3));
        let forms = store(&[(1, ItemKind::Armor), (2, ItemKind::Armor), (3, ItemKind::Armor)]);
        let groups = vec![(a, at(0.5), 1), (b, at(ADDED), 1), (b, at(SHOWN_FULL), 1), (c, at(JUST_BELOW), 1)];
        let o = offer(&groups, &forms, &[b, c], a, Pick::at(Some(0.5)), false).unwrap();
        assert_eq!(o.parts.iter().map(|p| (p.item, p.condition)).collect::<Vec<_>>(), vec![(c, JUST_BELOW)]);
        // The two AddItem units of one group: neither the item nor the part is repairable.
        let pair = offer(&[(a, at(ADDED), 2)], &forms, &[], a, Pick::at(Some(ADDED)), false).unwrap();
        assert!(pair.parts.is_empty());
        assert!(!pair.usable());
    }

    #[test]
    fn the_list_closes_on_reaching_what_shows_as_full() {
        let (a, b) = (FormId(1), FormId(2));
        let forms = store(&[(1, ItemKind::Armor), (2, ItemKind::Armor)]);
        let o = offer(&[(a, at(0.5), 1), (b, at(0.4), 3)], &forms, &[b], a, Pick::at(Some(0.5)), false).unwrap();
        // Parts are left in both: only the item's condition closes the list.
        let below = vec![(a, at(JUST_BELOW), 1), (b, at(0.4), 2)];
        assert_eq!(after_repair(&below, &forms, &[b], &o, JUST_BELOW).map(|n| n.condition), Some(JUST_BELOW));
        let shown_full = vec![(a, at(SHOWN_FULL), 1), (b, at(0.4), 2)];
        assert_eq!(after_repair(&shown_full, &forms, &[b], &o, SHOWN_FULL), None);
    }

    #[test]
    fn a_click_chooses_a_row_and_a_second_click_uses_the_part() {
        assert_eq!(resolve(Some(0), RepairInput::Click(2)), RepairInput::Hover(2));
        assert_eq!(resolve(Some(2), RepairInput::Click(2)), RepairInput::Use);
        // The item itself is not used on itself.
        assert_eq!(resolve(Some(0), RepairInput::Click(0)), RepairInput::Hover(0));
        assert_eq!(resolve(None, RepairInput::Click(1)), RepairInput::Hover(1));
        assert_eq!(resolve(Some(1), RepairInput::Down), RepairInput::Down);
    }
}
