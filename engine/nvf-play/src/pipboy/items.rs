//! The ITEMS page (docs/spec-notes/pipboy.md §4): the player's inventory, one tab per
//! kind of item (Weapons, Apparel, Aid, Misc, Ammo), over the inventory model
//! (`menu::model`, `Mode::Inventory`).
//!
//! Lengths are the canvas's, from the menus' rectangle (`frame::MAIN`) as
//! `menus\main\inventory_menu.xml` gives them [documented] unless marked otherwise.
//!
//! - The heading's cards: Wg (carried weight / what the player can carry), HP, DT and
//!   Caps [observed: the owner's screenshots; documented: their places].
//! - The list: one row per stack, its name in font 2 with the count in brackets, a filled
//!   square before what is equipped; the selected row boxed.
//! - Keys: Up/Down (W/S) select, Enter or Space equip, unequip or use; the wheel
//!   scrolls; a left click on a row selects it, a left click on the selected row acts on
//!   it. A right click on a row drops one (a stack asks how many), Shift + right click
//!   the stack [design: owner 2026-11-02, #1393]. The keyboard has no drop key: X is the
//!   original's PC key for Mod (`IM_ModButton`), and X on the controller is its drop
//!   button (`IM_DropButton`'s `&xbuttonx;` [documented: inventory_menu.xml]), which
//!   drops one here when the controller works the controls.
//! - The scroll bar at the list's left, only while the list is longer than its window:
//!   the arrows at its ends and a line for the part shown [observed: the owner's scroll
//!   screenshot; the DATA lists' bar, `data::scroll_bar`].
//! - The selected (pointed at) item at the right: its picture (`IM_ItemIcon`, 450, 10,
//!   380 square) and its figures (`IM_ItemInfoRect`, 385, 340, 446 × 150, three cards a
//!   row of a third of it, 10 apart, a third of its height high, `item_stats_display.xml`)
//!   [documented: places]: [`info_tiles`].
//! - A weapon's skill badge (`IM_ItemIconBadge`, 390, 260, 64 square): the picture of
//!   the skill the weapon uses (`weap_skill_icon_*.dds`), and beside it the skill it
//!   needs (`IM_StrReq`, 440, 300, font 7, left), "0" too [documented: places; observed:
//!   the owner's ITEMS screenshots, Guns 0/25/50 under the bullets, the Throwing Spear's
//!   Melee Weapons 25 under the knife]. Not on the ITEMS page: DPS (the original shows
//!   none there, and the badge takes its place over DAM [observed]); the container and
//!   barter figures keep it ([`weapon_dps`]).
//! - "Mod X)" under the Repair hint (`IM_ModButton`, 47 lower, `sInventoryMod`) for a
//!   weapon with mod slots, always bright: the mod menu attaches and removes mods
//!   [documented: place, key; hypothesis: when it shows; design: always bright, the
//!   owner's rule 2026-10-27]. X on it opens the mod menu (`super::item_mod`,
//!   weapon-mods.md §5); elsewhere X does nothing.
//! - A modded weapon's figures are its modded ones (`WeaponStats::modded`).
//! - While a hotkey's key is held, the hotkey panel stands in the picture's place and a
//!   left click (or Enter) puts the row's item in that slot (`super::hotkey_panel`).

use super::frame::{self, Align, Card, Ctx, Kit, LINE, MAIN, TEXT, THICK};
use super::hotkey_panel::HotkeyHold;
use crate::hotkeys::Hotkeys;
use super::{Page, PageBody, PipBoy};
use crate::interface::{pipboy_amber as amber, at};
use crate::menu::model::{self, Category, Command, MenuModel, Mode, Row, Side};
use crate::menu::{ActiveMenu, MenuInput};
use crate::sim::SimState;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_assets::GameData;

/// The heading's cards: x and width inside the menus' rectangle.
const WG: (f32, f32) = (215.0, 155.0);
const HP: (f32, f32) = (380.0, 155.0);
const DT: (f32, f32) = (545.0, 110.0);
const CAPS: (f32, f32) = (665.0, 190.0);
/// The list: 15 left of the menus' rectangle, 75 down, 400 × 468; a row 32 in (past the
/// scroll bar) to 5 short of the list's right edge, as the DATA lists' (the Repair and
/// Mod pages' 393 is of their 430-wide lists; here it ran over the item's figures), its
/// name 40 in and 20 down, broken at 300; the equipped square 25 across at 15, 18.
const LIST: Rect = Rect { min: Vec2::new(-15.0, 75.0), max: Vec2::new(385.0, 543.0) };
const ROW_X: f32 = 32.0;
const ROW_W: f32 = 400.0 - ROW_X - 5.0;
const NAME_X: f32 = 40.0;
const NAME_Y: f32 = 20.0;
const WRAP: f32 = 300.0;
const MARK: Rect = Rect { min: Vec2::new(15.0, 18.0), max: Vec2::new(40.0, 43.0) };
/// A row is 51 high and its name's further lines 35 below the first [hypothesis: as the
/// barter menu's rows, measured there, barter-menu.md §3].
pub const ROW_H: f32 = 51.0;
const LINE_PITCH: f32 = 35.0;

/// The category of each tab.
pub const TABS: [Category; 5] = [Category::Weapons, Category::Apparel, Category::Aid, Category::Misc, Category::Ammo];

/// A row's text: its name, and its count in brackets when more than one.
pub fn row_text(r: &Row) -> String {
    if r.count > 1 {
        format!("{} ({})", r.name, r.count)
    } else {
        r.name.clone()
    }
}

/// The item picture's box and the figures' rectangle, inside the menus' rectangle.
const ICON: Rect = Rect { min: Vec2::new(450.0, 10.0), max: Vec2::new(830.0, 390.0) };
const INFO: Rect = Rect { min: Vec2::new(385.0, 340.0), max: Vec2::new(831.0, 490.0) };
/// The condition meter inside its card: 62 in, 15 down, 20 high, 72 short of the card
/// [documented: item_stats_display.xml ConditionMeter].
const METER: (f32, f32, f32, f32) = (62.0, 15.0, 20.0, 72.0);
/// The weapon's skill badge and the skill it needs, font 7 [documented:
/// `IM_ItemIconBadge`, `IM_StrReq`].
const BADGE: Rect = Rect { min: Vec2::new(390.0, 260.0), max: Vec2::new(454.0, 324.0) };
const SKILL_REQ: Vec2 = Vec2::new(440.0, 300.0);
const SKILL_REQ_FONT: usize = 7;
/// "Mod X)": the Repair hint's place 47 lower [documented: `IM_ModButton` `_y` 47].
const MOD_HINT: Vec2 = Vec2::new(855.0, 45.0 + 47.0);
/// The scroll bar: 32 wide, arrows 32 × 16 [documented: list_box.xml, the DATA lists].
const SCROLLBAR: f32 = 32.0;

/// What the figures show of an item.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    /// The first card: ("DAM", damage) of a weapon, ("DT", threshold) of armour.
    pub first: Option<(&'static str, String)>,
    pub weight: Option<String>,
    pub value: Option<String>,
    /// A weapon's Strength requirement, over the value.
    pub strength: Option<u32>,
    /// Condition 0 to 1 of a weapon or armour.
    pub condition: Option<f32>,
    /// Beside the condition: a weapon's ammunition "20ga (2/30)", armour's class.
    pub beside: Option<String>,
    /// The effects' text ("HP +40", "AGL -1").
    pub effects: Option<String>,
    /// A weapon's damage a second, over the DAM card.
    pub dps: Option<String>,
    /// A weapon's skill picture for the badge ([`skill_badge`]).
    pub badge: Option<String>,
    /// The skill the weapon needs, beside the badge.
    pub skill_req: Option<u32>,
    /// A weapon with mod slots: the "Mod X)" hint shows.
    pub mod_hint: bool,
}

/// The badge of a weapon's skill (WEAP DNAM skill, by its name) [documented: the six
/// `textures\interface\icons\typeicons\weap_skill_icon_*.dds` in Fallout - Textures;
/// observed: which shows for Guns and Melee Weapons on the owner's screenshots;
/// hypothesis: the other four by their names]. Other skills (Barter on binoculars) have
/// none.
pub fn skill_badge(skill: &str) -> Option<&'static str> {
    Some(match skill {
        "Guns" => "Interface\\Icons\\TypeIcons\\weap_skill_icon_sm_arms.dds",
        "Big Guns" => "Interface\\Icons\\TypeIcons\\weap_skill_icon_big_guns.dds",
        "Energy Weapons" => "Interface\\Icons\\TypeIcons\\weap_skill_icon_energy.dds",
        "Explosives" => "Interface\\Icons\\TypeIcons\\weap_skill_icon_explosives.dds",
        "Melee Weapons" => "Interface\\Icons\\TypeIcons\\weap_skill_icon_melee.dds",
        "Unarmed" => "Interface\\Icons\\TypeIcons\\weap_skill_icon_unarmed.dds",
        _ => return None,
    })
}

/// Damage a second of a weapon at `condition`: the damage a shot does at that condition
/// times its shots a second (the fire rate of an automatic weapon, else its attack shots a
/// second, else one) [hypothesis: the GECK's DPS calculator uses the damage and an
/// automatic weapon's fire rate, GECK wiki Weapons].
pub fn weapon_dps(w: &fnv_combat::data::WeaponStats, condition: f32) -> f32 {
    let per_second = if w.automatic && w.fire_rate > 0.0 {
        w.fire_rate
    } else if w.attack_shots_per_sec > 0.0 {
        w.attack_shots_per_sec
    } else {
        1.0
    };
    w.base_damage.max(0) as f32 * fnv_combat::damage::condition_mult(condition) * per_second
}

/// One card of the figures, canvas units inside the figures' rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub rect: Rect,
    pub title: String,
    pub value: String,
    /// The condition meter's fill.
    pub meter: Option<f32>,
}

/// The cards of the figures [documented: item_stats_display.xml: a card is a third of the
/// rectangle wide and high; damage or DT at the left of the first row, weight and value
/// 10 apart after it; the Strength requirement over the value; the second row the
/// condition and beside it the ammunition two cards and 10 wide; effects across the
/// three cards and 20 (hypothesis: on the first free row, as on the owner's apparel and
/// aid screenshots)].
pub fn info_tiles(i: &Info) -> Vec<Tile> {
    tiles_in(i, INFO.size())
}

/// [`info_tiles`] in a figures' rectangle of `size` (the container and barter menus'
/// is 530 × 120, `crate::menu::look`; the same `item_stats_display.xml`).
pub fn tiles_in(i: &Info, size: Vec2) -> Vec<Tile> {
    let (w, h) = (size.x / 3.0, size.y / 3.0);
    let col = |n: f32| n * (w + 10.0);
    let tile = |x: f32, y: f32, width: f32, title: &str, value: String| Tile { rect: Rect::new(x, y, x + width, y + h), title: title.into(), value, meter: None };
    let mut out = Vec::new();
    if let Some((title, v)) = &i.first {
        out.push(tile(0.0, 0.0, w, title, v.clone()));
    }
    out.push(tile(col(1.0), 0.0, w, "WG", i.weight.clone().unwrap_or_else(|| "--".into())));
    out.push(tile(col(2.0), 0.0, w, "VAL", i.value.clone().unwrap_or_else(|| "--".into())));
    if let Some(s) = i.strength {
        out.push(tile(col(2.0), -h, w, "STR", s.to_string()));
    }
    if let Some(d) = &i.dps {
        out.push(tile(0.0, -h, w, "DPS", d.clone()));
    }
    let mut row = 1.0;
    if i.condition.is_some() || i.beside.is_some() {
        if let Some(cnd) = i.condition {
            out.push(Tile { meter: Some(cnd.clamp(0.0, 1.0)), ..tile(0.0, h, w, "CND", String::new()) });
        }
        if let Some(b) = &i.beside {
            out.push(tile(col(1.0), h, 2.0 * w + 10.0, b, String::new()));
        }
        row = 2.0;
    }
    if let Some(e) = &i.effects {
        out.push(tile(0.0, row * h, 3.0 * w + 20.0, "EFFECTS", e.clone()));
    }
    out
}

/// A weight or value of a stack: "--" for nothing [observed: the Stimpak's WG "--"; its
/// VAL 675 is 9 × 75, so the figures are the stack's (hypothesis for the weight)].
fn figure(v: f32, decimals: bool) -> Option<String> {
    (v > 0.0).then(|| if decimals { format!("{v:.2}") } else { format!("{v:.0}") })
}

/// What the figures show of a row, from the records (the ITEMS page's, and the
/// container and barter menus' item figures, `crate::menu::look`). An aid item's effects
/// are those that would apply to the player now (`fnv_game::Game::aid_preview`: the
/// effects' conditions, one line per value), read from the records only without the
/// scripts' game.
pub fn info_of(r: &Row, combat: Option<&crate::combat::Combat>, sim: &mut SimState, forms: Option<&mut super::stats::read::Forms>, script: Option<&mut crate::script::ScriptState>) -> Info {
    use fnv_sim::forms::ItemKind;
    let n = r.count.max(1) as f32;
    let mut i = Info {
        weight: r.weight.and_then(|w| figure(w * n, true)),
        value: r.value.and_then(|v| figure(v as f32 * n, false)),
        condition: r.condition.filter(|_| matches!(r.kind, Some(ItemKind::Weapon | ItemKind::Armor))),
        ..Info::default()
    };
    let cf = combat.map(|c| &c.forms);
    match r.kind {
        Some(ItemKind::Weapon) => {
            if let Some(w) = cf.and_then(|f| f.weapons.get(&r.item)).map(|w| w.modded(r.mods)) {
                let w = &w;
                // Binoculars and the like are weapons without damage: no DAM or STR
                // [hypothesis].
                i.first = (w.base_damage > 0).then(|| ("DAM", w.base_damage.to_string()));
                i.strength = (w.strength_req > 0).then_some(w.strength_req);
                i.badge = w.skill.as_deref().and_then(skill_badge).map(str::to_string);
                i.skill_req = i.badge.is_some().then_some(w.skill_req);
                if w.base_damage > 0 {
                    i.dps = Some(format!("{:.0}", weapon_dps(w, r.condition.unwrap_or(1.0))));
                }
                // Always bright: the mod menu also takes mods off [design: the owner's
                // rule, 2026-10-27].
                i.mod_hint = w.mods.iter().any(|m| m.item.is_some());
                let f = cf.expect("weapon stats came from it");
                let choices = f.ammo_choices(w);
                let carried: Vec<i32> = choices.iter().map(|&a| sim.world.item_count(FormId::PLAYER_REF, a).unwrap_or(0)).collect();
                let pick = choices.iter().zip(&carried).find(|(_, n)| **n > 0).or(choices.iter().zip(&carried).next());
                if let Some((&ammo, &count)) = pick {
                    let a = &f.ammo[&ammo];
                    let name = a.short_name.clone().or_else(|| a.name.clone()).unwrap_or_default();
                    // In the clip: as much as a clip holds of what is carried [hypothesis:
                    // "(2/30)" on the owner's screenshot reads as clip/carried].
                    let clip = (w.clip_size as i32).min(count).max(0);
                    i.beside = Some(format!("{name} ({clip}/{count})"));
                }
            }
        }
        Some(ItemKind::Armor) => {
            if let Some(a) = cf.and_then(|f| f.armor.get(&r.item)) {
                i.first = Some(("DT", format!("{:.0}", a.dt)));
            }
            i.beside = sim.forms.item(r.item).map(|it| format!("{:?}", it.armor_class));
        }
        _ => {}
    }
    if let Some(forms) = forms {
        let preview = script.and_then(|s| s.rt.as_mut()).filter(|_| r.kind == Some(ItemKind::Aid)).map(|rt| crate::script::with_world(rt, sim, |rt| rt.game.aid_preview(FormId::PLAYER_REF, r.item)));
        if let Some(lines) = preview {
            let changes: Vec<super::stats::model::Change> = lines.into_iter().filter(|(_, amount)| amount.abs() >= 0.5).map(|(code, amount)| super::stats::model::Change { value: forms.short_name(code), amount: amount.abs(), detrimental: amount < 0.0 }).collect();
            i.effects = (!changes.is_empty()).then(|| super::stats::model::changes_text(&changes));
            return i;
        }
        // Aid items carry their effects; apparel and weapons their enchantment's.
        let ench = if r.kind == Some(ItemKind::Aid) { Some(r.item.0) } else { forms.item_ench(r.item.0) };
        if let Some(e) = ench.map(|e| forms.ench(e)).filter(|e| !e.changes.is_empty()) {
            let changes: Vec<super::stats::model::Change> = e.changes.iter().map(|&(code, amount, detrimental)| super::stats::model::Change { value: forms.short_name(code), amount, detrimental }).collect();
            i.effects = Some(super::stats::model::changes_text(&changes));
        }
    }
    i
}

/// A clickable row of the list.
#[derive(Component, Clone, Copy)]
struct RowButton(usize);

/// The ITEMS page's keys and wheel, while it is shown.
#[allow(clippy::too_many_arguments)]
pub fn keys(
    pip: Res<PipBoy>,
    (repair, modding): (Res<super::repair::PipRepair>, Res<super::item_mod::PipItemMod>),
    active: Res<ActiveMenu>,
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    mut lines: Local<f32>,
    mut out: MessageWriter<MenuInput>,
    (mut hold, mut hotkeys): (ResMut<HotkeyHold>, ResMut<Hotkeys>),
    (pads, bindings): (Query<&Gamepad>, Option<Res<crate::bindings::Bindings>>),
) {
    // Laid out in the Pip-Boy's screen texture, at its size whatever the UI scale (#1631).
    let _canvas = crate::interface::original_units();
    use KeyCode::*;
    // The repair list and the mod menu have the keys while one is shown (`super::repair`,
    // `super::item_mod`, which opens on X before this runs).
    let items = pip.open() && pip.page == Page::Items && !repair.is_open() && !modding.is_open() && active.0.as_ref().is_some_and(|m| m.mode == Mode::Inventory);
    let mut send = |c| {
        out.write(MenuInput(c));
    };
    if !items {
        wheel.clear();
        return;
    }
    // The model shows the tab's kind of item.
    let want = TABS[pip.tab()];
    if active.0.as_ref().is_some_and(|m| m.category[Side::Player as usize] != want) {
        send(Command::SetCategory(Side::Player, want));
    }
    for (k, c) in [([ArrowUp, KeyW], Command::Up), ([ArrowDown, KeyS], Command::Down), ([PageUp, PageUp], Command::PageUp), ([PageDown, PageDown], Command::PageDown)] {
        if keys.any_just_pressed(k) {
            send(c);
        }
    }
    if keys.any_just_pressed([Enter, NumpadEnter, Space]) {
        // With a hotkey's key held, Enter puts the selected item in its slot.
        match hold.slot {
            Some(slot) => {
                if let Some(r) = active.0.as_ref().and_then(|m| m.selected_row(Side::Player)) {
                    hold.said = Some(super::hotkey_panel::assign_row(&mut hotkeys, slot, r));
                }
            }
            None => send(Command::One),
        }
    }
    // Backspace leaves an open Misc folder (the "Back" row does too) [design].
    if keys.just_pressed(Backspace) {
        send(Command::Back);
    }
    // The controller's X drops one (module notes); the keyboard's X is the mod menu's. Not
    // while "How many?" is up (the model would ignore it there too).
    let asking = active.0.as_ref().is_some_and(|m| m.quantity.is_some());
    if !asking && crate::pad::menu_button(&pads, bindings.as_deref(), GamepadButton::West) {
        send(Command::Drop);
    }
    for w in wheel.read() {
        *lines += match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 20.0,
        };
    }
    let whole = lines.trunc();
    if whole != 0.0 {
        send(Command::Scroll(-whole as i32));
        *lines -= whole;
    }
}

/// The row the pointer comes onto takes the selection, unless the wheel holds it
/// (`crate::menu::WheelHold`: a list redrawn under a resting pointer would undo the wheel).
fn on_row_over(over: On<PointerOver>, rows: Query<&RowButton>, active: Res<ActiveMenu>, hold: Option<Res<crate::menu::WheelHold>>, mut out: MessageWriter<MenuInput>) {
    if hold.is_some_and(|h| h.held()) {
        return;
    }
    let (Ok(r), Some(m)) = (rows.get(over.entity), active.0.as_ref()) else { return };
    if m.selected[Side::Player as usize] != r.0 {
        out.write(MenuInput(Command::Hover(Side::Player, r.0)));
    }
}

/// A left click selects the row (and on the selected row acts on it); with a hotkey's key
/// held, it puts the row's item in that slot instead (`super::hotkey_panel`). A right
/// click selects the row without acting and drops one, with Shift the stack (module notes).
fn on_row_click(
    click: On<PointerClick>,
    rows: Query<&RowButton>,
    active: Res<ActiveMenu>,
    keys: Res<ButtonInput<KeyCode>>,
    (mut hold, mut hotkeys): (ResMut<HotkeyHold>, ResMut<Hotkeys>),
    mut out: MessageWriter<MenuInput>,
) {
    let Ok(r) = rows.get(click.entity) else { return };
    if click.button == PointerButton::Secondary {
        if hold.slot.is_none() {
            let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
            out.write(MenuInput(Command::Hover(Side::Player, r.0)));
            out.write(MenuInput(if shift { Command::DropStack } else { Command::Drop }));
        }
        return;
    }
    match hold.slot {
        Some(slot) if click.button == PointerButton::Primary => {
            if let Some(row) = active.0.as_ref().and_then(|m| m.list(Side::Player).rows.get(r.0)) {
                hold.said = Some(super::hotkey_panel::assign_row(&mut hotkeys, slot, row));
            }
        }
        Some(_) => {}
        None => {
            out.write(MenuInput(Command::Select(Side::Player, r.0)));
        }
    }
}

/// Fill the ITEMS page's body when it is new or the inventory changed.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    bodies: Query<(Entity, &PageBody)>,
    added: Query<(), Added<PageBody>>,
    mut active: ResMut<ActiveMenu>,
    mut sim: ResMut<SimState>,
    mut kit: ResMut<Kit>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    combat: Option<Res<crate::combat::Combat>>,
    play: Option<Res<crate::PlayWorld>>,
    mut forms: Local<Option<super::stats::read::Forms>>,
    (repair, modding, mut script): (Res<super::repair::PipRepair>, Res<super::item_mod::PipItemMod>, Option<ResMut<crate::script::ScriptState>>),
    (hold, hotkeys): (Res<HotkeyHold>, Res<Hotkeys>),
) {
    // Laid out in the Pip-Boy's screen texture, at its size whatever the UI scale (#1631).
    let _canvas = crate::interface::original_units();
    let Some((body, _)) = bodies.iter().find(|(_, b)| b.page == Page::Items) else { return };
    if added.get(body).is_err() && !active.is_changed() && !repair.is_changed() && !modding.is_changed() && !hold.is_changed() && !hotkeys.is_changed() {
        return;
    }
    if forms.is_none() {
        *forms = play.as_ref().map(|p| super::stats::read::Forms::new(p.lo, p.schema));
    }
    // The mod menu in place of the page's body (`super::item_mod`).
    if let Some(s) = modding.0.as_ref() {
        let chosen = s.selected.checked_sub(1).and_then(|i| s.offer.choices.get(i));
        let desc = chosen.zip(forms.as_mut()).map(|(ch, f)| f.text(ch.item.0).description.clone());
        let mut c = Ctx::new(&mut kit, data.as_deref(), &mut images, formats.as_deref());
        commands.entity(body).despawn_children().with_children(|p| super::item_mod::draw_list(p, &mut c, s, combat.as_deref(), desc.as_deref()));
        return;
    }
    // The repair list in place of the page's body (`super::repair`).
    if let Some(s) = repair.0.as_ref() {
        let mut c = Ctx::new(&mut kit, data.as_deref(), &mut images, formats.as_deref());
        commands.entity(body).despawn_children().with_children(|p| super::repair::draw_list(p, &mut c, s, combat.as_deref()));
        return;
    }
    let Some(m) = active.0.as_ref().filter(|m| m.mode == Mode::Inventory) else { return };
    let sim = sim.bypass_change_detection();
    let mut script = script.as_mut().map(|s| s.bypass_change_detection());
    let caps = sim.world.item_count(FormId::PLAYER_REF, model::CAPS).unwrap_or(0);
    let selected = m.list(Side::Player).rows.get(m.selected[Side::Player as usize]);
    // No DPS on this page: the skill badge stands over DAM (`item_info`).
    let info = selected.map(|r| (r.icon.clone(), Info { dps: None, ..info_of(r, combat.as_deref(), sim, forms.as_mut(), script.as_deref_mut()) }));
    if let (Some(r), Some((_, i))) = (selected, &info) {
        info!("pipboy items: {} selected: {:?}", r.name, i);
    }
    // The heading's HP and DT: the STATS page's health, and the damage threshold a hit on
    // the player meets (worn armour at its condition and the DT actor value,
    // `crate::combat::target_armor`) [observed: "HP 205/205", "DT 8.0" on the owner's
    // screenshots; hypothesis: the figure is the one combat uses].
    let hp = forms.as_mut().and_then(|f| super::stats::read::player_hp(f, sim, script.as_deref_mut()));
    let dt = combat.as_deref().map(|c| crate::combat::target_armor(c, script.as_deref_mut(), sim, FormId::PLAYER_REF).0);
    let (hp, dt) = (hp_text(hp), dt.map_or_else(|| "--".to_string(), |v| format!("{v:.1}")));
    let offer = super::repair::current(&active, sim, script.as_deref());
    let mut c = Ctx::new(&mut kit, data.as_deref(), &mut images, formats.as_deref());
    // The rows' heights and the first row drawn: the selection is always drawn, rows of
    // two lines taking more room (owner's playtest list 7 item 15).
    let heights = row_heights(&mut c, m);
    let first = if m.pointer_pick { m.scroll[Side::Player as usize].min(heights.len()) } else { first_shown(&heights, m.scroll[Side::Player as usize], m.selected[Side::Player as usize], LIST.height()) };
    if active.is_changed() {
        info!("pipboy items: heading HP {hp}, DT {dt}; rows {first}.. of {} drawn", heights.len());
    }
    commands.entity(body).despawn_children().with_children(|p| {
        let wg = match &m.encumbrance {
            Ok(e) => format!("{:.0}/{:.0}", e.weight, e.capacity),
            Err(_) => "--".into(),
        };
        let cards = [
            Card::new(WG.0, WG.1, "Wg", wg),
            Card::new(HP.0, HP.1, "HP", hp),
            Card::new(DT.0, DT.1, "DT", dt),
            Card::new(CAPS.0, CAPS.1, "Caps", caps.to_string()),
        ];
        frame::heading(p, &mut c, Page::Items.title(), &cards);
        list(p, &mut c, m, first, &heights);
        // The hotkey panel hides the picture, the badge and the hints (`IM_ButtonRect`).
        let panel = hold.slot.is_some();
        if let (true, Some(k)) = (panel, hold.slot) {
            info!("pipboy hotkeys: panel drawn, key {} chosen{}", k + 1, hold.said.as_ref().map_or_else(String::new, |s| format!(", says {s:?}")));
            super::hotkey_panel::draw(p, &mut c, &hotkeys, &hold, selected, sim, combat.as_deref());
        } else {
            super::repair::draw_hint(p, &mut c, offer.as_ref());
        }
        if let Some((icon, i)) = &info {
            if panel {
                item_info(p, &mut c, None, &Info { badge: None, skill_req: None, ..i.clone() });
            } else {
                item_info(p, &mut c, icon.as_deref(), i);
            }
            if let (false, true, Some(r)) = (panel, i.mod_hint, selected) {
                // X, with a controller LB (`inventory_menu.xml`'s, `crate::pad::menu_buttons`).
                let hint = format!("Mod {}", crate::pad::hint(&[KeyCode::KeyX], crate::pad::at_menu(fnv_game::game::menus::INVENTORY)));
                info!("pipboy items: {} {hint}", r.name);
                c.text(p, 2, &hint, MAIN.min.x + MOD_HINT.x - 10.0, MAIN.min.y + MOD_HINT.y + frame::BUTTON_TEXT_Y, Align::Right, amber(TEXT));
            }
        }
    });
    // The model keeps the first row drawn as its scroll position, so a row the pointer
    // picks later does not move the list back (as the container and barter menus do).
    if first != m.scroll[Side::Player as usize] {
        if let Some(m) = active.bypass_change_detection().0.as_mut() {
            m.scroll[Side::Player as usize] = first;
        }
    }
}

/// The selected item's picture and figures.
fn item_info(p: &mut ChildSpawnerCommands, c: &mut Ctx, icon: Option<&str>, i: &Info) {
    let bright = amber(TEXT);
    if let Some(icon) = icon {
        let r = Rect::from_corners(MAIN.min + ICON.min, MAIN.min + ICON.max);
        c.image(p, icon, at(r.min.x, r.min.y, r.width(), r.height()), bright);
    }
    if let Some(b) = i.badge.as_deref() {
        let r = Rect::from_corners(MAIN.min + BADGE.min, MAIN.min + BADGE.max);
        c.image(p, b, at(r.min.x, r.min.y, r.width(), r.height()), bright);
    }
    if let Some(req) = i.skill_req {
        c.text(p, SKILL_REQ_FONT, &req.to_string(), MAIN.min.x + SKILL_REQ.x, MAIN.min.y + SKILL_REQ.y, Align::Left, bright);
    }
    let o = MAIN.min + INFO.min;
    for t in info_tiles(i) {
        let (x, y, w) = (o.x + t.rect.min.x, o.y + t.rect.min.y, t.rect.width());
        frame::card(p, c, x, y, w, &t.title, &t.value);
        if let Some(fill) = t.meter {
            let (mx, my, mh, short) = METER;
            let mw = w - short;
            p.spawn((at(x + mx, y + my, mw, mh), BackgroundColor(bright.with_alpha(0.15))));
            p.spawn((at(x + mx, y + my, mw * fill, mh), BackgroundColor(bright)));
        }
    }
}

/// The HP card's figure: health out of its whole, "--" unread.
fn hp_text(hp: Option<(f64, f64)>) -> String {
    hp.map_or_else(|| "--".to_string(), |(v, max)| format!("{}/{}", super::stats::model::whole(v), super::stats::model::whole(max)))
}

/// Each row's height as drawn: a name broken over more lines makes its row taller.
fn row_heights(c: &mut Ctx, m: &MenuModel) -> Vec<f32> {
    m.list(Side::Player).rows.iter().map(|r| ROW_H + LINE_PITCH * c.lines(2, &row_text(r), Some(WRAP)).len().saturating_sub(1) as f32).collect()
}

/// The first row to draw: the list's own first row, or a later one when the rows from
/// there to the selection do not fit in `room` (rows of two lines take more of it than the
/// model's nine rows a page count on).
pub fn first_shown(heights: &[f32], first: usize, selected: usize, room: f32) -> usize {
    let mut first = first.min(heights.len());
    let last = selected.min(heights.len().saturating_sub(1));
    while first < last && heights[first..=last].iter().sum::<f32>() > room {
        first += 1;
    }
    first
}

fn list(p: &mut ChildSpawnerCommands, c: &mut Ctx, m: &MenuModel, first: usize, heights: &[f32]) {
    let bright = amber(TEXT);
    let line = amber(LINE);
    let x0 = MAIN.min.x + LIST.min.x + ROW_X;
    let mut y = MAIN.min.y + LIST.min.y;
    let rows = &m.list(Side::Player).rows;
    let selected = m.selected[Side::Player as usize];
    let mut fit = 0;
    for i in first..rows.len() {
        let r = &rows[i];
        let lines = c.lines(2, &row_text(r), Some(WRAP));
        let h = heights[i];
        if y + h > MAIN.min.y + LIST.max.y {
            break;
        }
        fit += 1;
        if i == selected {
            c.boxed(p, Rect::new(x0, y, x0 + ROW_W, y + h), line, None);
        }
        if r.equipped {
            let mk = Rect::new(x0 + MARK.min.x, y + MARK.min.y, x0 + MARK.max.x, y + MARK.max.y);
            if c.image(p, "square_filled.dds", at(mk.min.x, mk.min.y, mk.width(), mk.height()), bright).is_none() {
                p.spawn((at(mk.min.x, mk.min.y, mk.width(), mk.height()), BackgroundColor(bright)));
            }
        }
        for (k, l) in lines.iter().enumerate() {
            c.set(p, 2, l, x0 + NAME_X, y + NAME_Y - THICK + LINE_PITCH * k as f32, Align::Left, bright);
        }
        p.spawn((at(x0, y, ROW_W, h), RowButton(i), Pickable::default())).observe(on_row_over).observe(on_row_click);
        y += h;
    }
    // The scroll bar, only while some rows do not show.
    if let Some((from, part)) = scroll_part(first, fit, rows.len()) {
        let (x, top) = (MAIN.min.x + LIST.min.x, MAIN.min.y + LIST.min.y);
        p.spawn((at(x, top, SCROLLBAR, LIST.height()), Pickable::IGNORE)).with_children(|b| super::data::scroll_bar(b, c, 0.0, LIST.height(), from, part));
    }
}

/// Where the scroll bar's line starts and how much of the track it takes, when the rows
/// from `first`, `fit` of them, are not all of `len`.
pub fn scroll_part(first: usize, fit: usize, len: usize) -> Option<(f32, f32)> {
    (len > 0 && (first > 0 || first + fit < len)).then(|| (first as f32 / len as f32, fit as f32 / len as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_selected_row_stops_short_of_the_figures() {
        // The box ends 5 short of the list's edge, left of the figures and the badge.
        assert_eq!(LIST.min.x + ROW_X + ROW_W, LIST.max.x - 5.0);
        assert!(LIST.min.x + ROW_X + ROW_W < INFO.min.x.min(BADGE.min.x));
    }

    #[test]
    fn a_stack_shows_its_count() {
        let mut r = Row {
            item: FormId(1),
            at: None,
            name: "Stimpak".into(),
            count: 9,
            weight: None,
            value: None,
            kind: None,
            equipped: false,
            quest: false,
            condition: None,
            icon: None,
            price: None,
            bartered: false,
            mods: 0,
            mods_value: 0,
            folder: None,
            reference: None,
        };
        assert_eq!(row_text(&r), "Stimpak (9)");
        r.count = 1;
        assert_eq!(row_text(&r), "Stimpak");
    }

    #[test]
    fn dps_is_the_damage_at_the_condition_times_the_shots_a_second() {
        let auto = fnv_combat::data::WeaponStats { base_damage: 5, automatic: true, fire_rate: 10.0, attack_shots_per_sec: 2.0, ..Default::default() };
        assert!((weapon_dps(&auto, 1.0) - 50.0).abs() < 1e-3);
        let semi = fnv_combat::data::WeaponStats { automatic: false, ..auto.clone() };
        assert!((weapon_dps(&semi, 1.0) - 10.0).abs() < 1e-3);
        // Worn, it does less.
        assert!(weapon_dps(&semi, 0.5) < weapon_dps(&semi, 1.0));
        let none = fnv_combat::data::WeaponStats { base_damage: 7, ..Default::default() };
        assert!((weapon_dps(&none, 1.0) - 7.0).abs() < 1e-3);
    }

    /// Nine rows of 51 fill the list; one name over two lines (86) pushes the last of
    /// them out, so the first row drawn moves on to keep the selection in (owner's
    /// playtest list 7 item 15: the last weapon could not be scrolled into view).
    #[test]
    fn a_two_line_row_moves_the_first_row_on() {
        let mut heights = vec![ROW_H; 12];
        heights[6] = ROW_H + LINE_PITCH;
        let room = LIST.height();
        // The model scrolls nine rows a page: selecting the last row puts row 3 first.
        assert_eq!(first_shown(&heights, 3, 11, room), 4);
        // Without the long name it fits.
        assert_eq!(first_shown(&[ROW_H; 12], 3, 11, room), 3);
        // A selection above the first row is not moved here (the model scrolls up).
        assert_eq!(first_shown(&heights, 3, 2, room), 3);
        assert_eq!(first_shown(&[], 0, 0, room), 0);
    }

    #[test]
    fn the_scroll_bar_shows_only_for_a_longer_list() {
        assert_eq!(scroll_part(0, 5, 5), None);
        assert_eq!(scroll_part(0, 0, 0), None);
        assert_eq!(scroll_part(0, 9, 18), Some((0.0, 0.5)));
        assert_eq!(scroll_part(9, 9, 18), Some((0.5, 0.5)));
    }

    #[test]
    fn a_weapon_skill_has_its_badge() {
        assert_eq!(skill_badge("Guns"), Some("Interface\\Icons\\TypeIcons\\weap_skill_icon_sm_arms.dds"));
        assert_eq!(skill_badge("Melee Weapons"), Some("Interface\\Icons\\TypeIcons\\weap_skill_icon_melee.dds"));
        assert_eq!(skill_badge("Barter"), None);
    }

    #[test]
    fn the_figures_are_laid_out_as_the_file() {
        let (w, h) = (INFO.width() / 3.0, INFO.height() / 3.0);
        // A weapon: DAM, WG, VAL; STR over VAL; CND and the ammunition.
        let weapon = Info {
            first: Some(("DAM", "72".into())),
            weight: Some("5.50".into()),
            value: Some("299".into()),
            strength: Some(3),
            condition: Some(0.8),
            beside: Some("40mm Grenade (1/19)".into()),
            effects: None,
            dps: Some("72".into()),
            badge: None,
            skill_req: None,
            mod_hint: false,
        };
        let t = info_tiles(&weapon);
        let titles: Vec<&str> = t.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["DAM", "WG", "VAL", "STR", "DPS", "CND", "40mm Grenade (1/19)"]);
        assert_eq!(t[4].rect.min, Vec2::new(0.0, -h));
        assert_eq!(t[1].rect.min, Vec2::new(w + 10.0, 0.0));
        assert_eq!(t[3].rect.min, Vec2::new(2.0 * (w + 10.0), -h));
        assert_eq!(t[5].meter, Some(0.8));
        assert!((t[6].rect.width() - (2.0 * w + 10.0)).abs() < 1e-4);
        // An aid item: WG and VAL, effects on the second row.
        let aid = Info { value: Some("675".into()), effects: Some("HP +40".into()), ..Info::default() };
        let t = info_tiles(&aid);
        assert_eq!(t[0].value, "--");
        assert_eq!((t[2].title.as_str(), t[2].rect.min.y), ("EFFECTS", h));
        // Armour: effects under the condition row.
        let armour = Info { condition: Some(1.0), beside: Some("Medium".into()), effects: Some("AGL -1".into()), ..Info::default() };
        assert_eq!(info_tiles(&armour).last().map(|t| t.rect.min.y), Some(2.0 * h));
    }
}
