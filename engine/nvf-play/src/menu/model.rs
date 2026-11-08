//! The menus as plain data over `fnv_sim::World`, without Bevy: rows, selection,
//! scrolling, and the commands that move items, all unit-tested on a synthetic plugin.
//!
//! Rows are one per base item (a stack), except that weapons and armour get one row per
//! condition: units stack only with units at the same condition [owner, 2026-10-16: six
//! Varmint Rifles, one at 95 %, list as a row of five at full condition and below it a row
//! of one]. Owner and the equipped mark do not split a row. A row moves, drops and
//! equips only its own units (`Row::at`); within them the simulation decides which go
//! (its removal order). Rows are sorted by display name in byte order, capitals before
//! small letters, FormID breaking ties [observed: the owner's barter screenshot lists
//! "LAER+" before "Lad's Life", and "$5 NCR" and ".223 Round" before letters;
//! docs/spec-notes/barter-menu.md §3], then the best condition first [owner, as above].
//!
//! The container and barter menus list one category at a time, chosen with the arrows
//! beside each list's title: all items, weapons, apparel, aid, misc, ammunition
//! [observed: "ITEMS" and "MISC" titles on the owner's screenshots; the other names and
//! their order are hypotheses, after the Pip-Boy's item pages]. The mouse wheel over a
//! list's heading steps its category as the arrows do (`Command::wheel`) [design: owner
//! request 2026-10-31, #1192].
//!
//! Non-playable items (`Item::non_playable`) are not listed in inventories other than
//! the player's, and non-playable armour (the Pip-Boy Glove) not in the player's either,
//! so they cannot be moved, equipped or dropped from a menu; the player's own
//! non-playable weapons and ammo are listed [documented: GECK wiki "Unplayable Items";
//! observed by the owner: the game never shows the glove in the inventory, and Boone's
//! rifle and "Magical Companion Ammo" do not show in his]. Take All leaves them in the
//! container [hypothesis: hidden items stay where they are].
//!
//! Items whose record has no name (no FULL, or a blank one) are not listed in any
//! inventory, the player's included ([`shown`]), so a row never reads as an Editor ID
//! (#1603: the vanilla book leveled list SublistBooks100 holds the nameless
//! DONOTUSEBookGeneric07 0x000199BB, which household and library clutter lists hand out)
//! [hypothesis: mods use nameless items as hidden tokens because the original's menus
//! leave them out; observer check in docs/handoff/observer-checks.md].
//!
//! Quest items (`World::is_quest_item`: the record's "Quest Item" header flag, or a
//! script's SetQuestObject) are listed and marked; dropping one is refused by the
//! simulation and the status line says so [documented: GECK wiki Category:Items, the
//! player cannot drop one]. Storing one in a container is allowed [hypothesis: the page
//! does not mention containers]. They cannot be sold [documented: the same page], so the
//! barter menu does not list them [hypothesis: hidden rather than refused].
//!
//! Barter ([`Mode::Barter`], [`Trade`]): items are put up from either side and listed on
//! the other side, first, marked, until the trade is accepted [observed: the owner's
//! screenshot shows the sold Gold Bar at the top of the merchant's list with an open
//! square, gone from the player's]; caps are not listed but shown in each list's heading
//! [observed: no Bottle Cap row in the barter screenshots, while the container
//! screenshot lists one]. Accept moves the items and the caps the prices add up to; it
//! is refused when the side that pays has too few caps [hypothesis: the wording is ours].
//!
//! "How many?" ([`Quantity`]): taking or storing one unit of a stack of more than
//! [`ASK_QUANTITY_ABOVE`] units, putting one up for barter (or taking it back), and
//! dropping one from the inventory ask how many first [owner, 2026-10-15: "buy, transfer,
//! drop stacks of items when more than 5"; observed: the owner's screenshots of the
//! original's prompt over the Pip-Boy, a stack of six Varmint Rifles]. The prompt starts
//! at the whole stack [observed: the first screenshot shows 6 of 6, the later ones 4 and
//! 3], goes from 1 to the stack [hypothesis: 0 would move nothing], and Ok moves that
//! many; Cancel (or closing) moves nothing. While it is open the menu takes no other
//! command. Smaller stacks still move one unit per press, and the Stack commands move the
//! whole stack at once without asking [design: NVForever's shortcuts].

use super::barter::{self, Prices};
use fnv_sim::forms::{BaseData, FormStore, Item, ItemKind};
use fnv_sim::refs::PlacedRef;
use fnv_sim::world::{Encumbrance, SimError};
use fnv_sim::{FormId, Pick, RefTable, World};
use std::collections::BTreeMap;

/// Rows the inventory (the Pip-Boy's ITEMS list) shows at once; the list scrolls to keep
/// the selection visible. Its list is 468 high and a row 51 (`pipboy::items`).
pub const VISIBLE_ROWS: usize = 9;
/// Rows the container and barter lists show at once: the list is 365 high and a row 51
/// (docs/spec-notes/barter-menu.md §2).
pub const LIST_ROWS: usize = 7;

/// A stack of more than this many units asks how many to move ([`Quantity`]) [owner,
/// 2026-10-15; the original's own threshold is a game setting not in FalloutNV.esm and
/// not read].
pub const ASK_QUANTITY_ABOVE: i32 = 5;

/// What the quantity prompt's Page Up / Page Down move it by [design: the interface
/// file's "jump size" is set by the original's code, not read].
pub const QUANTITY_JUMP: i32 = 10;

/// The caps item, Bottle Cap [documented: FalloutNV.esm MISC 0x0000000F, the item the
/// vendor caps lists hold].
pub const CAPS: FormId = FormId(0x0000000F);

/// One stack in an inventory.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub item: FormId,
    /// The units the row stands for (`Extra::is_at`): weapons' and armour's at this
    /// condition, every unit (`None`) of other items.
    pub at: Option<f32>,
    pub name: String,
    pub count: i32,
    /// Weight of one unit as authored (`None`: not decoded). Ammunition weighs nothing in
    /// the totals outside Hardcore mode (the simulation's rule), but its row shows the
    /// authored weight.
    pub weight: Option<f32>,
    /// Value of one unit.
    pub value: Option<i32>,
    pub kind: Option<ItemKind>,
    /// At least one unit is equipped.
    pub equipped: bool,
    /// A quest item: the player cannot drop it (`World::is_quest_item`).
    pub quest: bool,
    /// The condition of the row's units, 0 to 1 (weapons and armour).
    pub condition: Option<f32>,
    /// The item's picture (ICON, a path under `textures\`).
    pub icon: Option<String>,
    /// Barter: what one unit fetches in this trade, in caps (before rounding).
    pub price: Option<f32>,
    /// Barter: units the other side puts up, listed here until the trade is accepted.
    pub bartered: bool,
    /// A weapon's attached mods (`Extra::mods`): its units are a row of their own, named
    /// with " +" (docs/spec-notes/weapon-mods.md §3).
    pub mods: u8,
    /// What the attached mods add to one unit's value: their IMOD values, which its
    /// condition does not lower (`FormStore::mods_value`, issue #982).
    pub mods_value: i32,
    /// The Misc tab's folder rows (the keyring, the caravan cards, and "Back" inside
    /// one): no item of their own ([`Folder`]).
    pub folder: Option<FolderRow>,
    /// A corpse's weapon lying beside it (`World::dropped_beside`): the created
    /// reference this row stands for. Taking the row picks that reference up, so the
    /// weapon on the ground and the row are one item (#1032).
    pub reference: Option<FormId>,
}

/// A folder of the inventory's Misc tab: the keys and the Caravan cards are listed in
/// one row each, which opens on a list of them [owner's playtest list 7 item 6: "a
/// keyring item that all keys are added to ... the same for caravan cards", so the Misc
/// tab is not cluttered; design: the rows' names and places are NVForever's].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    /// KEYM items.
    Keys,
    /// The player's Caravan cards: those AddCardToPlayer gave (`fnv_game::caravan`, kept
    /// outside the inventory, [`MenuModel::cards`]) and any CCRD item carried.
    Cards,
}

impl Folder {
    /// The folder row's name.
    pub fn name(self) -> &'static str {
        match self {
            Folder::Keys => "Keyring",
            Folder::Cards => "Caravan Cards",
        }
    }

    fn holds(self, kind: Option<ItemKind>) -> bool {
        matches!((self, kind), (Folder::Keys, Some(ItemKind::Key)) | (Folder::Cards, Some(ItemKind::CaravanCard)))
    }
}

/// What a folder row does when used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderRow {
    /// Opens the folder.
    Open(Folder),
    /// Back to the Misc list (the first row inside a folder).
    Back,
}

/// The name of the row that leaves a folder.
pub const BACK: &str = "Back";

impl Row {
    /// A folder row: `count` items inside (0 for "Back").
    fn of_folder(f: FolderRow, count: i32) -> Row {
        let name = match f {
            FolderRow::Open(f) => f.name().to_string(),
            FolderRow::Back => BACK.to_string(),
        };
        Row { item: FormId(0), at: None, name, count, weight: None, value: None, kind: None, equipped: false, quest: false, condition: None, icon: None, price: None, bartered: false, mods: 0, mods_value: 0, folder: Some(f), reference: None }
    }
}

/// An inventory's rows (sorted) and totals.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    pub rows: Vec<Row>,
    /// Total weight as the simulation counts it (`Inventory::totals`), hidden
    /// (non-playable) items included: they are carried.
    pub weight: f64,
    /// Value of the listed stacks.
    pub value: i64,
    /// Stacks not listed because they are non-playable (`Item::listed`).
    pub hidden: usize,
}

/// What a row says of a worn weapon or piece of armour after its name: the condition
/// of its units, nothing at full condition (docs/spec-notes/combat-rules.md §17).
pub fn condition_note(kind: Option<ItemKind>, condition: Option<f32>) -> String {
    match (kind, condition) {
        (Some(ItemKind::Weapon | ItemKind::Armor), Some(c)) if c <= 0.0 => " (broken)".into(),
        (Some(ItemKind::Weapon | ItemKind::Armor), Some(c)) if c < 1.0 => format!(" (CND {}%)", fnv_combat::condition::percent(c)),
        _ => String::new(),
    }
}

/// A row's name for units with the mod flags `mods`: " +" after it when any mod is on
/// [hypothesis: the GECK wiki's SetItemModFlags calls the flag "the inventory '+'";
/// weapon-mods.md §3].
pub fn modded_name(name: String, mods: u8) -> String {
    if mods != 0 {
        format!("{name} +")
    } else {
        name
    }
}

/// Whether a menu lists `item` in an inventory, the player's (`players`) or another
/// holder's: a playable item by `Item::listed`, and one with a name (module notes). A form
/// the store does not know is listed, by its FormID.
pub fn shown(forms: &FormStore, item: FormId, players: bool) -> bool {
    let Some(b) = forms.get(item) else { return true };
    b.name.as_deref().is_some_and(|n| !n.trim().is_empty()) && forms.item(item).is_none_or(|i| i.listed(players))
}

/// Display name of a form: its FULL name, else its Editor ID, else its FormID.
pub fn display_name(forms: &FormStore, id: FormId) -> String {
    match forms.get(id) {
        Some(b) => b.name.clone().filter(|n| !n.trim().is_empty()).or_else(|| b.edid.clone()).unwrap_or_else(|| id.to_string()),
        None => id.to_string(),
    }
}

/// A failed command in words for the player: items by name, not FormID. Starts with
/// "Cannot" (the menu draws such lines as warnings).
pub fn plain_error(forms: &FormStore, e: &SimError) -> String {
    let name = |id: &FormId| forms.get(*id).and_then(|b| b.name.clone().or_else(|| b.edid.clone())).unwrap_or_else(|| "this item".into());
    match e {
        SimError::NotEquippable(item) => format!("Cannot equip or use {}", name(item)),
        SimError::NotCarried { item, .. } => format!("Cannot find {} in the inventory", name(item)),
        SimError::NotAnItem(item) => format!("Cannot carry {}", name(item)),
        SimError::Deleted(item) => format!("Cannot reach {}: it is gone", name(item)),
        other => format!("Cannot: {other}"),
    }
}

/// What tells a row's units apart: the item, the condition's bits (a condition is never
/// negative, so the bits order as the numbers do) and the mod flags.
pub type Units = (FormId, Option<u32>, u8);

impl Row {
    /// One unit's value before a trade's multiplier: the item's value at its condition
    /// plus what its attached mods add, which the condition does not lower [observed:
    /// o26_value 2026-11-06, `FormStore::mods_value`; issue #982].
    pub fn worth(&self, prices: &Prices) -> Option<f32> {
        self.value.map(|v| prices.value_at(v, self.condition) + self.mods_value as f32)
    }

    pub fn units(&self) -> Units {
        (self.item, self.at.map(f32::to_bits), self.mods)
    }

    /// The units the row's actions take: its condition and its mods (weapon-mods.md §3).
    pub fn pick(&self) -> Pick {
        Pick::row(self.at, self.mods)
    }
}

/// The units [`Row::units`] names, for the simulation.
fn units_pick(&(_, at, mods): &Units) -> Pick {
    Pick::row(at.map(f32::from_bits), mods)
}

/// The order rows are listed in: put-up rows first, then by name in byte order, then the
/// best condition first.
fn sort_rows(rows: &mut [Row]) {
    let key = |r: &Row| (!r.bartered, r.name.as_bytes().to_vec(), r.item, std::cmp::Reverse(r.at.map(f32::to_bits)), r.mods);
    rows.sort_by_cached_key(key);
}

/// The rows of `holder`'s inventory, sorted by name.
pub fn listing(world: &mut World<'_>, holder: FormId) -> Result<Listing, SimError> {
    let forms = world.forms;
    let hardcore = world.config().hardcore;
    // The holder's perks (Pack Rat, Heavyweight) in the total (perk-entry-points.md §7).
    let rules = world.weight_rules(holder);
    let s = world.get(holder)?;
    if s.deleted() {
        return Err(SimError::Deleted(holder));
    }
    let inv = s.inventory.as_ref().ok_or(SimError::NoInventory(holder))?;
    let totals = inv.totals_with(forms, hardcore, &rules);
    let mut rows: Vec<Row> = Vec::new();
    let mut hidden = 0;
    for (item, extra, count) in inv.groups() {
        let it = forms.item(item);
        if !shown(forms, item, holder == PLAYER) {
            hidden += 1;
            continue;
        }
        let kind = it.map(|i| i.kind);
        let at = matches!(kind, Some(ItemKind::Weapon | ItemKind::Armor)).then(|| extra.condition_or_whole());
        // Groups of one item come together (`Inventory::groups`).
        if let Some(r) = rows.iter_mut().rev().take_while(|r| r.item == item).find(|r| r.at == at && r.mods == extra.mods) {
            r.count += count;
            r.equipped |= extra.equipped;
            continue;
        }
        rows.push(Row {
            item,
            at,
            name: modded_name(display_name(forms, item), extra.mods),
            count,
            weight: it.and_then(|i| i.weight),
            value: it.and_then(|i| i.worth()),
            kind,
            equipped: extra.equipped,
            quest: false, // below, once the inventory is no longer borrowed
            condition: at,
            icon: it.and_then(|i| i.icon.clone()),
            price: None,
            bartered: false,
            mods: extra.mods,
            mods_value: forms.mods_value(item, extra.mods),
            folder: None,
            reference: None,
        });
    }
    for r in &mut rows {
        r.quest = world.is_quest_item(r.item);
    }
    sort_rows(&mut rows);
    let value = rows.iter().map(|r| (r.value.unwrap_or(0) + r.mods_value) as i64 * r.count as i64).sum();
    Ok(Listing { rows, weight: totals.weight, value, hidden })
}

/// Add to a container column the weapons its holder dropped as it died and that still
/// lie in the world (`World::dropped_beside`): one row per reference, after the
/// inventory's rows and sorted with them. Their weight and value count in the column's
/// totals [design: the corpse's loot, as the owner's report asks (#1032)].
fn add_dropped_beside(world: &World<'_>, holder: FormId, l: &mut Listing) {
    let forms = world.forms;
    let mut added = false;
    for r in world.dropped_beside(holder) {
        let Some(s) = world.peek(r) else { continue };
        let it = forms.item(s.base);
        if !shown(forms, s.base, false) {
            continue;
        }
        let kind = it.map(|i| i.kind);
        let at = matches!(kind, Some(ItemKind::Weapon | ItemKind::Armor)).then(|| s.extra.condition_or_whole());
        let row = Row {
            item: s.base,
            at,
            name: modded_name(display_name(forms, s.base), s.extra.mods),
            count: s.count.max(1),
            weight: it.and_then(|i| i.weight),
            value: it.and_then(|i| i.worth()),
            kind,
            equipped: false,
            quest: world.is_quest_item(s.base),
            condition: at,
            icon: it.and_then(|i| i.icon.clone()),
            price: None,
            bartered: false,
            mods: s.extra.mods,
            mods_value: forms.mods_value(s.base, s.extra.mods),
            folder: None,
            reference: Some(r),
        };
        l.weight += f64::from(row.weight.unwrap_or(0.0)) * f64::from(row.count);
        l.value += (row.value.unwrap_or(0) + row.mods_value) as i64 * row.count as i64;
        l.rows.push(row);
        added = true;
    }
    if added {
        sort_rows(&mut l.rows);
    }
}

/// A column of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// The container, or the merchant (drawn on the right).
    Container = 0,
    /// The player (drawn on the left; the only column of the inventory menu).
    Player = 1,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Container => Side::Player,
            Side::Player => Side::Container,
        }
    }
}

/// The items a list shows, chosen with the arrows beside its title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Category {
    #[default]
    All,
    Weapons,
    Apparel,
    Aid,
    Misc,
    Ammo,
}

impl Category {
    pub const ORDER: [Category; 6] = [Category::All, Category::Weapons, Category::Apparel, Category::Aid, Category::Misc, Category::Ammo];

    /// Whether an item of this kind is listed.
    pub fn holds(self, kind: Option<ItemKind>) -> bool {
        self == Category::All || self == Category::of(kind, false)
    }

    /// Whether this item is listed: as [`Category::holds`], a skill book with the aid
    /// items.
    pub fn holds_item(self, kind: Option<ItemKind>, item: Option<&Item>) -> bool {
        self == Category::All || self == Category::of(kind, item.is_some_and(|i| i.teaches.is_some()))
    }

    /// The tab an item is listed in. A skill book (a BOOK that teaches a skill) is with
    /// the aid items, other books with the misc ones [owner's playtest list 7 item 33:
    /// "skill books ... should be showing in the Aid tab"; hypothesis: books that teach
    /// nothing stay in Misc].
    pub fn of(kind: Option<ItemKind>, skill_book: bool) -> Category {
        match kind {
            Some(ItemKind::Weapon) => Category::Weapons,
            Some(ItemKind::Armor) => Category::Apparel,
            Some(ItemKind::Aid) => Category::Aid,
            Some(ItemKind::Book) if skill_book => Category::Aid,
            Some(ItemKind::Ammo) => Category::Ammo,
            _ => Category::Misc,
        }
    }

    /// The next category (`by` 1) or the one before (-1), round the ends.
    pub fn step(self, by: i32) -> Category {
        let n = Self::ORDER.len() as i32;
        let i = Self::ORDER.iter().position(|&c| c == self).unwrap_or(0) as i32;
        Self::ORDER[(i + by).rem_euclid(n) as usize]
    }

    /// The player's list title [observed: "ITEMS", "MISC"; the others hypotheses].
    pub fn title(self) -> &'static str {
        match self {
            Category::All => "ITEMS",
            Category::Weapons => "WEAPONS",
            Category::Apparel => "APPAREL",
            Category::Aid => "AID",
            Category::Misc => "MISC",
            Category::Ammo => "AMMO",
        }
    }
}

/// Which menu is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The container menu on this container reference: player | container.
    Container(FormId),
    /// The player's inventory alone.
    Inventory,
    /// The barter menu with this merchant (an actor reference): player | merchant.
    Barter(FormId),
}

/// What a row stands for: its units, the reference a corpse's dropped weapon is, and which of
/// rows alike it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowKey {
    pub units: Units,
    pub reference: Option<FormId>,
    pub nth: u32,
}

impl RowKey {
    /// The key of `rows[index]`.
    pub fn at(rows: &[Row], index: usize) -> Option<RowKey> {
        let row = rows.get(index)?;
        let (units, reference) = (row.units(), row.reference);
        let nth = rows[..index].iter().filter(|r| r.units() == units && r.reference == reference).count() as u32;
        Some(RowKey { units, reference, nth })
    }
}

/// [design] A row as the input that clicked or came onto it saw it: the commands after
/// [`Command::Expect`] are for this row of this menu, and for no other row a command queued
/// before them leaves at its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowGuard {
    pub mode: Mode,
    pub side: Side,
    pub index: usize,
    pub key: RowKey,
    /// The commands act on the row as the selected one.
    pub selected: bool,
    /// How many commands follow the guard.
    pub then: u8,
}

/// What the player asks the menu to do (keys, mouse and the test aid all go through
/// this).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// The next commands hold only if the row is still there ([`MenuModel::row_is`]);
    /// `crate::menu::apply_input` drops them otherwise.
    Expect(RowGuard),
    Up,
    Down,
    PageUp,
    PageDown,
    /// Move the selection by this many rows (mouse wheel).
    Scroll(i32),
    /// Focus a column (container and barter menus).
    Focus(Side),
    /// Select a row (mouse). Selecting the row that is already selected does
    /// [`Command::One`].
    Select(Side, usize),
    /// Select a row without acting on it (the pointer came onto it).
    Hover(Side, usize),
    /// Show the next (1) or previous (-1) category of a column.
    Category(Side, i32),
    /// Show this category in a column (the Pip-Boy's ITEMS tabs).
    SetCategory(Side, Category),
    /// Container menu: move one unit of the selected stack to the other column (take
    /// from the container, store from the player). Barter: put one unit up, or take one
    /// back. Inventory: equip or unequip it (aid items are used).
    One,
    /// Container and barter menus: the same for the whole selected stack.
    Stack,
    /// Container menu: take everything from the container.
    TakeAll,
    /// Barter: make the trade.
    Accept,
    /// Inventory: drop one unit of the selected stack into the world.
    Drop,
    /// Inventory: drop the whole selected stack.
    DropStack,
    /// Inventory: leave the open Misc folder ([`Folder`]).
    Back,
    /// The "How many?" prompt: set its amount (clamped to 1 and the stack).
    Amount(i32),
    /// The prompt: change its amount by this much.
    AmountStep(i32),
    /// The prompt's Ok: do what asked with the amount chosen.
    AmountOk,
    /// The prompt's Cancel.
    AmountCancel,
    Close,
}

impl Command {
    /// What whole wheel steps (`steps`, up positive) do: with the pointer over a list's
    /// heading (`title`) they step that list's category, down the next as the right arrow
    /// does; anywhere else they move the selection, up moving it up [design: owner
    /// request 2026-10-31, #1192].
    pub fn wheel(title: Option<Side>, steps: i32) -> Command {
        match title {
            Some(side) => Command::Category(side, -steps),
            None => Command::Scroll(-steps),
        }
    }
}

/// The "How many?" prompt open over a menu (`menus\quantity_menu.xml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantity {
    /// What asked: [`Command::One`] (take, store, put up, take back) or
    /// [`Command::Drop`]; done with [`Quantity::amount`] units on Ok.
    pub cmd: Command,
    /// The row it is for.
    pub side: Side,
    pub row: usize,
    /// The stack's size, the most it can move.
    pub max: i32,
    pub amount: i32,
}

impl Quantity {
    /// How full the prompt's meter is: the amount as a part of the stack.
    pub fn part(&self) -> f32 {
        self.amount as f32 / self.max.max(1) as f32
    }
}

/// The first row shown of a list of `rows`, `page` at once: `first` moved as little as
/// keeps `selected` in view. A row the pointer is over is already in view, so hovering
/// never scrolls a list kept this way; only the keys and the wheel do.
pub fn keep_in_view(first: usize, selected: usize, rows: usize, page: usize) -> usize {
    first.max((selected + 1).saturating_sub(page)).min(selected).min(rows.saturating_sub(page))
}

/// The amount at a point of the prompt's meter (`part` of its length from the left): that
/// part of the stack, rounded [documented: quantity_menu.xml, the drag's 0 to 1 times the
/// stack, rounded], at least 1.
pub fn amount_at(part: f32, max: i32) -> i32 {
    ((part.clamp(0.0, 1.0) * max as f32).round() as i32).clamp(1, max.max(1))
}

/// A trade being put together in the barter menu.
#[derive(Debug, Clone, PartialEq)]
pub struct Trade {
    pub merchant: FormId,
    /// What holds the merchant's goods and caps: its merchant container (XMRC), else the
    /// merchant itself [observed: Chet's container VendorContainerChet holds his stock
    /// and the vendor caps lists, his own inventory only pocket change; hypothesis: the
    /// merchant's own inventory is not offered].
    pub stock: FormId,
    /// The item that counts as money.
    pub caps: FormId,
    /// `ShowBarterMenu`'s percentage.
    pub discount: i32,
    /// The player's Barter skill.
    pub skill: f32,
    pub prices: Prices,
    /// Units put up, by the side they come from (`Side as usize`).
    pub offered: [BTreeMap<Units, i32>; 2],
    /// The caps each side holds (`Side as usize`).
    pub purse: [i32; 2],
    /// The caps the trade moves: positive when the player is paid.
    pub balance: i64,
    /// What the merchant sells (its services, docs/spec-notes/vendor-services.md §4):
    /// `None` sells everything (no script data to ask).
    pub sells: Option<Sells>,
    /// The merchant's items held back at the last refresh, with the category it does not
    /// sell them under, in row order.
    pub withheld: Vec<(FormId, &'static str)>,
}

/// Whether the merchant sells an item: `None` when it does, else the name of the service
/// category it lacks (`fnv_game::services`). Vendors buy any item [documented: GECK wiki
/// Class, "vendors can buy any item"], so only the merchant's own rows are asked.
#[derive(Clone)]
pub struct Sells(pub std::sync::Arc<dyn Fn(FormId) -> Option<&'static str> + Send + Sync>);

impl std::fmt::Debug for Sells {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sells(..)")
    }
}

impl PartialEq for Sells {
    fn eq(&self, other: &Sells) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Trade {
    /// Whether the side that pays holds the caps the balance asks of it. When it does not,
    /// the figure dims; only a player who is short is refused ([`Trade::paid`]).
    pub fn affordable(&self) -> bool {
        let (player, merchant) = (Side::Player as usize, Side::Container as usize);
        match self.balance {
            b if b < 0 => self.purse[player] as i64 >= -b,
            b => self.purse[merchant] as i64 >= b,
        }
    }

    /// The caps the trade moves, positive when the player is paid: the balance, held at the
    /// merchant's purse when it cannot pay all of it [observed 2026-11-06: barter-menu.md
    /// section 9, the original completes such a sale at that amount].
    pub fn paid(&self) -> i64 {
        self.balance.min(i64::from(self.purse[Side::Container as usize]))
    }

    /// What one unit of value is worth when it comes from `from`.
    fn mult(&self, from: Side) -> f32 {
        match from {
            Side::Player => self.prices.selling(self.skill, self.discount),
            Side::Container => self.prices.buying(self.skill, self.discount),
        }
    }

    /// What holds a side's items.
    fn holder(&self, side: Side) -> FormId {
        match side {
            Side::Container => self.stock,
            Side::Player => PLAYER,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.offered.iter().all(BTreeMap::is_empty)
    }
}

/// The state of an open menu.
#[derive(Debug, Clone)]
pub struct MenuModel {
    pub mode: Mode,
    /// The column that has the keyboard (always `Player` in the inventory menu).
    pub focus: Side,
    /// Selected row per column, indexed by `Side as usize`.
    pub selected: [usize; 2],
    /// First visible row per column.
    pub scroll: [usize; 2],
    /// The selection was last moved by the pointer coming onto a row: the lists stay
    /// where they are (only the wheel and the keys scroll them; owner's playtest list 7,
    /// item 11). The menu keeps `scroll` at the rows it draws (`look::drawn_first`).
    pub pointer_pick: bool,
    /// Counts the keys' and the wheel's moves of the selection, also one that ends on the same
    /// row: asked to move, the tile menu shows the selected row again.
    pub navigated: u32,
    /// By column: the rows the category leaves out (the file's `_num_filtered`).
    pub filtered: [usize; 2],
    /// Rows per column; the container column is empty in the inventory menu.
    pub lists: [Listing; 2],
    /// The category each column shows.
    pub category: [Category; 2],
    /// The player's encumbrance, or why it is unknown.
    pub encumbrance: Result<Encumbrance, String>,
    /// Heading: the container's or merchant's name, or "Inventory".
    pub title: String,
    /// The last command's result or error, shown in the menu.
    pub status: Option<String>,
    /// References created by drops (the simulation has them; nothing renders them yet).
    pub dropped: Vec<FormId>,
    /// The barter menu's trade.
    pub trade: Option<Trade>,
    /// The "How many?" prompt, while it is open.
    pub quantity: Option<Quantity>,
    /// The amount the prompt chose, for the command it does on Ok.
    amount: Option<i32>,
    /// The container menu is picking a living actor's pocket ([`Steal`]).
    pub steal: Option<Steal>,
    /// Inventory: the Misc folder open, if any ([`Folder`]).
    pub folder: Option<Folder>,
    /// Inventory: the player's Caravan cards, one entry a card, set by the engine from
    /// the scripts' game ([`MenuModel::set_cards`]).
    pub cards: Vec<FormId>,
}

/// What a command did besides changing the model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    /// The menu should close.
    pub close: bool,
    /// The inventories (or the trade) changed.
    pub changed: bool,
    /// References created by this command (drops).
    pub created: Vec<FormId>,
    /// An item the player used up (an aid item "equipped" from the inventory): its
    /// effects (and effect scripts) are the script runtime's (`crate::ItemUsed`).
    pub used: Option<FormId>,
    /// Items the player took out of the container, with their counts (`crate::crime`:
    /// taking owned items is theft).
    pub taken: Vec<(FormId, i32)>,
    /// Caps that changed hands in an accepted trade (Barter Amount Traded,
    /// `fnv_game::misc_stats`).
    pub traded: u32,
    /// Power armour the player has no training for was refused: the runtime shows the
    /// game's message (`fnv_game::functions::refuse_power_armor`).
    pub power_armor_refused: bool,
    /// Pickpocketing: items the player put into the victim's pocket, with their counts.
    pub given: Vec<(FormId, i32)>,
    /// Pickpocketing: the roll for this item failed, the victim noticed (the menu closes).
    pub caught: Option<FormId>,
    /// World references the player picked up from the menu (a corpse's dropped weapon,
    /// [`Row::reference`]): gone from the world, as a pick-up there would leave them.
    pub picked: Vec<FormId>,
}

/// Take a corpse's dropped weapon from its loot menu: the player picks the reference up,
/// the same as activating it on the ground (`World::activate`), so its condition and mods
/// come with it and it leaves the world. Returns the units taken.
fn take_reference(world: &mut World<'_>, r: FormId, picked: &mut Vec<FormId>) -> Result<i32, SimError> {
    match world.activate(PLAYER, r)?.outcome {
        fnv_sim::world::Outcome::PickedUp { count, .. } => {
            picked.push(r);
            Ok(count)
        }
        _ => Ok(0),
    }
}

/// The container menu on a living actor's inventory while the player sneaks: each take
/// or placement rolls against its chance ([`Steal::chance`], `fnv_game::pickpocket`);
/// a failed roll moves nothing and closes the menu ([`Applied::caught`]). Equipped items
/// are not listed ("By default, the player cannot pickpocket equipped items"
/// [documented: GECK wiki SetPlayerCanPickpocketEquippedItems]; hypothesis: a stack with
/// one unit equipped is hidden whole), and Take All is refused [design].
#[derive(Debug, Clone)]
pub struct Steal {
    pub victim: FormId,
    pub rules: fnv_game::pickpocket::Rules,
    /// The player's and the victim's Sneak.
    pub thief: f64,
    pub target: f64,
    /// Equipped items are listed too (ShowOff's SetPlayerCanPickpocketEquippedItems,
    /// `fnv_game::Game::pickpocket_worn`).
    pub worn: bool,
    rng: fnv_combat::rng::SplitMix,
}

impl Steal {
    pub fn new(victim: FormId, rules: fnv_game::pickpocket::Rules, thief: f64, target: f64, seed: u64) -> Steal {
        Steal { victim, rules, thief, target, worn: false, rng: fnv_combat::rng::SplitMix(seed) }
    }

    /// The chance, percent, of moving `count` units of `row` either way.
    pub fn chance(&self, row: &Row, count: i32) -> i32 {
        let value = f64::from(row.value.unwrap_or(0).max(0)) * f64::from(count.max(1));
        self.rules.chance(self.thief, self.target, value)
    }

    /// Roll against `chance`: whether the move goes unnoticed.
    fn unnoticed(&mut self, chance: i32) -> bool {
        (self.rng.next_f32() * 100.0) < chance as f32
    }
}

const PLAYER: FormId = FormId::PLAYER_REF;

impl MenuModel {
    /// Open the container menu. Fails when the reference has no inventory.
    pub fn open_container(world: &mut World<'_>, container: FormId) -> Result<MenuModel, SimError> {
        let base = world.get(container)?.base;
        let title = display_name(world.forms, base);
        let mut m = MenuModel::empty(Mode::Container(container), title);
        m.focus = Side::Container;
        m.refresh(world)?;
        Ok(m)
    }

    /// Open the player's inventory.
    pub fn open_inventory(world: &mut World<'_>) -> Result<MenuModel, SimError> {
        let mut m = MenuModel::empty(Mode::Inventory, "Inventory".into());
        m.focus = Side::Player;
        m.refresh(world)?;
        Ok(m)
    }

    /// Open the barter menu with `merchant`, whose goods and caps `stock` holds. Fails
    /// when either has no inventory.
    pub fn open_barter(world: &mut World<'_>, merchant: FormId, stock: FormId, caps: FormId, skill: f32, discount: i32) -> Result<MenuModel, SimError> {
        let base = world.get(merchant)?.base;
        let title = display_name(world.forms, base);
        let mut m = MenuModel::empty(Mode::Barter(merchant), title);
        m.trade = Some(Trade {
            merchant,
            stock,
            caps,
            discount,
            skill,
            prices: Prices::from_settings(world.forms),
            offered: [BTreeMap::new(), BTreeMap::new()],
            purse: [0, 0],
            balance: 0,
            sells: None,
            withheld: Vec::new(),
        });
        m.focus = Side::Player;
        m.refresh(world)?;
        Ok(m)
    }

    /// Limit the merchant's rows to what it sells, and list the trade again.
    pub fn set_sells(&mut self, world: &mut World<'_>, sells: Sells) -> Result<(), SimError> {
        if let Some(t) = self.trade.as_mut() {
            t.sells = Some(sells);
        }
        self.refresh(world)
    }

    fn empty(mode: Mode, title: String) -> MenuModel {
        MenuModel {
            mode,
            focus: Side::Player,
            selected: [0; 2],
            scroll: [0; 2],
            pointer_pick: false,
            navigated: 0,
            filtered: [0; 2],
            lists: [Listing::default(), Listing::default()],
            category: [Category::All; 2],
            encumbrance: Err(String::new()),
            title,
            status: None,
            dropped: Vec::new(),
            trade: None,
            quantity: None,
            amount: None,
            steal: None,
            folder: None,
            cards: Vec::new(),
        }
    }

    pub fn list(&self, side: Side) -> &Listing {
        &self.lists[side as usize]
    }

    /// The selected row of a column.
    pub fn selected_row(&self, side: Side) -> Option<&Row> {
        self.lists[side as usize].rows.get(self.selected[side as usize])
    }

    /// Rows shown at once.
    pub fn page(&self) -> usize {
        match self.mode {
            Mode::Inventory => VISIBLE_ROWS,
            _ => LIST_ROWS,
        }
    }

    /// Indices of the visible rows of a column.
    pub fn visible(&self, side: Side) -> std::ops::Range<usize> {
        let n = self.lists[side as usize].rows.len();
        let start = self.scroll[side as usize].min(n);
        start..(start + self.page()).min(n)
    }

    fn columns(&self) -> &'static [Side] {
        match self.mode {
            Mode::Container(_) | Mode::Barter(_) => &[Side::Container, Side::Player],
            Mode::Inventory => &[Side::Player],
        }
    }

    /// Re-read the inventories from the simulation, keeping each column's selection on
    /// the same row when it is still there (else on the same position).
    pub fn refresh(&mut self, world: &mut World<'_>) -> Result<(), SimError> {
        let keep = [Side::Container, Side::Player].map(|s| self.selected_row(s).map(|r| (r.units(), r.bartered, r.folder)));
        if self.trade.is_some() {
            self.refresh_trade(world)?;
        } else {
            for &side in self.columns() {
                let holder = match (side, self.mode) {
                    (Side::Container, Mode::Container(c)) => c,
                    _ => PLAYER,
                };
                let mut l = listing(world, holder)?;
                if side == Side::Container && self.steal.is_none() {
                    add_dropped_beside(world, holder, &mut l);
                }
                let forms = world.forms;
                let all = l.rows.len();
                l.rows.retain(|r| self.category[side as usize].holds_item(r.kind, forms.item(r.item)));
                self.filtered[side as usize] = all - l.rows.len();
                if side == Side::Container && self.steal.as_ref().is_some_and(|s| !s.worn) {
                    l.rows.retain(|r| !r.equipped);
                }
                if self.mode == Mode::Inventory && self.category[side as usize] == Category::Misc {
                    self.fold(&mut l, forms);
                }
                self.lists[side as usize] = l;
            }
        }
        for &side in self.columns() {
            let i = side as usize;
            let rows = &self.lists[i].rows;
            let at = keep[i].and_then(|k| rows.iter().position(|r| (r.units(), r.bartered, r.folder) == k));
            self.selected[i] = at.unwrap_or(self.selected[i]).min(rows.len().saturating_sub(1));
            self.ensure_visible(side);
        }
        self.encumbrance = world.encumbrance(PLAYER).map_err(|e| e.to_string());
        Ok(())
    }

    /// The barter lists: each side's own rows less what it put up, and what the other
    /// side put up; prices; the caps on each side and the trade's balance.
    fn refresh_trade(&mut self, world: &mut World<'_>) -> Result<(), SimError> {
        let Some(t) = self.trade.as_mut() else { return Ok(()) };
        let mut own: [Vec<Row>; 2] = [Vec::new(), Vec::new()];
        let mut weights = [0.0; 2];
        for side in [Side::Container, Side::Player] {
            let i = side as usize;
            let l = listing(world, t.holder(side))?;
            weights[i] = l.weight;
            t.purse[i] = world.item_count(t.holder(side), t.caps)?;
            let mult = t.mult(side);
            let prices = t.prices;
            let mut withheld = Vec::new();
            own[i] = l
                .rows
                .into_iter()
                .filter(|r| r.item != t.caps && !r.quest)
                .filter(|r| {
                    let why = (side == Side::Container).then(|| t.sells.as_ref().and_then(|s| (s.0)(r.item))).flatten();
                    if let Some(w) = why.filter(|_| !withheld.iter().any(|(i, _)| *i == r.item)) {
                        withheld.push((r.item, w));
                    }
                    why.is_none()
                })
                .map(|mut r| {
                    r.price = r.worth(&prices).map(|v| v * mult);
                    r
                })
                .collect();
            if side == Side::Container {
                t.withheld = withheld;
            }
            // What is no longer there cannot stay put up.
            let rows = &own[i];
            t.offered[i].retain(|units, n| match rows.iter().find(|r| r.units() == *units) {
                Some(r) => {
                    *n = (*n).min(r.count);
                    *n > 0
                }
                None => false,
            });
        }
        let mut shown: [Vec<Row>; 2] = [Vec::new(), Vec::new()];
        let mut lines: [Vec<(f32, i32)>; 2] = [Vec::new(), Vec::new()];
        for side in [Side::Container, Side::Player] {
            let i = side as usize;
            for r in &own[i] {
                let up = t.offered[i].get(&r.units()).copied().unwrap_or(0);
                if r.count > up {
                    shown[i].push(Row { count: r.count - up, ..r.clone() });
                }
                if up > 0 {
                    lines[i].push((r.price.unwrap_or(0.0), up));
                    // Shown on the other side without the equipped mark [hypothesis].
                    shown[side.other() as usize].push(Row { count: up, bartered: true, equipped: false, ..r.clone() });
                }
            }
        }
        t.balance = barter::balance(lines[Side::Player as usize].iter().copied(), lines[Side::Container as usize].iter().copied());
        for side in [Side::Container, Side::Player] {
            let i = side as usize;
            let mut rows = std::mem::take(&mut shown[i]);
            rows.retain(|r| self.category[i].holds_item(r.kind, world.forms.item(r.item)));
            sort_rows(&mut rows);
            let value = rows.iter().map(|r| r.value.unwrap_or(0) as i64 * r.count as i64).sum();
            self.lists[i] = Listing { rows, weight: weights[i], value, hidden: 0 };
        }
        Ok(())
    }

    /// The Misc tab's folders: outside them, the keys and the cards are one row each at
    /// the top (only when there are some); inside one, a "Back" row and what it holds.
    fn fold(&self, l: &mut Listing, forms: &FormStore) {
        let (inside, rest): (Vec<Row>, Vec<Row>) = std::mem::take(&mut l.rows).into_iter().partition(|r| Folder::Keys.holds(r.kind) || Folder::Cards.holds(r.kind));
        match self.folder {
            None => {
                let count = |f: Folder| inside.iter().filter(|r| f.holds(r.kind)).map(|r| r.count).sum::<i32>();
                let cards = count(Folder::Cards) + self.cards.len() as i32;
                let mut rows: Vec<Row> = [(Folder::Keys, count(Folder::Keys)), (Folder::Cards, cards)].into_iter().filter(|&(_, n)| n > 0).map(|(f, n)| Row::of_folder(FolderRow::Open(f), n)).collect();
                rows.extend(rest);
                l.rows = rows;
            }
            Some(f) => {
                let mut held: Vec<Row> = inside.into_iter().filter(|r| f.holds(r.kind)).collect();
                if f == Folder::Cards {
                    for &card in &self.cards {
                        match held.iter_mut().find(|r| r.item == card) {
                            Some(r) => r.count += 1,
                            None => held.push(Row {
                                item: card,
                                at: None,
                                name: display_name(forms, card),
                                count: 1,
                                weight: None,
                                value: None,
                                kind: Some(ItemKind::CaravanCard),
                                equipped: false,
                                quest: false,
                                condition: None,
                                icon: forms.item(card).and_then(|i| i.icon.clone()),
                                price: None,
                                bartered: false,
                                mods: 0,
                                mods_value: 0,
                                folder: None,
                                reference: None,
                            }),
                        }
                    }
                    sort_rows(&mut held);
                }
                l.rows = std::iter::once(Row::of_folder(FolderRow::Back, 0)).chain(held).collect();
            }
        }
    }

    /// Open a Misc folder, or leave the one open (`None`, the folder's row then
    /// selected).
    fn enter(&mut self, world: &mut World<'_>, folder: Option<Folder>) -> Result<(), SimError> {
        let from = std::mem::replace(&mut self.folder, folder);
        let i = Side::Player as usize;
        self.selected[i] = 0;
        self.scroll[i] = 0;
        self.refresh(world)?;
        if let (None, Some(f)) = (folder, from) {
            if let Some(at) = self.lists[i].rows.iter().position(|r| r.folder == Some(FolderRow::Open(f))) {
                self.selected[i] = at;
                self.ensure_visible(Side::Player);
            }
        }
        Ok(())
    }

    /// The player's Caravan cards changed (the engine reads them from the scripts'
    /// game): the inventory's lists are read again.
    pub fn set_cards(&mut self, world: &mut World<'_>, cards: Vec<FormId>) -> Result<(), SimError> {
        if self.cards == cards {
            return Ok(());
        }
        self.cards = cards;
        if self.mode == Mode::Inventory {
            self.refresh(world)?;
        }
        Ok(())
    }

    fn ensure_visible(&mut self, side: Side) {
        let page = self.page();
        let i = side as usize;
        let n = self.lists[i].rows.len();
        let (sel, s) = (self.selected[i], &mut self.scroll[i]);
        if sel < *s {
            *s = sel;
        } else if sel >= *s + page {
            *s = sel + 1 - page;
        }
        *s = (*s).min(n.saturating_sub(page));
    }

    fn move_selection(&mut self, delta: i64) {
        let i = self.focus as usize;
        let n = self.lists[i].rows.len() as i64;
        if n == 0 {
            return;
        }
        self.selected[i] = (self.selected[i] as i64 + delta).clamp(0, n - 1) as usize;
        self.ensure_visible(self.focus);
    }

    /// Apply a command. Errors from the simulation become the status line; nothing
    /// panics.
    pub fn apply(&mut self, world: &mut World<'_>, cmd: Command) -> Applied {
        let mut out = Applied::default();
        if self.quantity.is_some() {
            return self.apply_quantity(world, cmd);
        }
        let page = self.page() as i64 - 1;
        if matches!(cmd, Command::Up | Command::Down | Command::PageUp | Command::PageDown | Command::Scroll(_)) {
            self.navigated = self.navigated.wrapping_add(1);
        }
        match cmd {
            Command::Hover(..) => self.pointer_pick = true,
            Command::Up | Command::Down | Command::PageUp | Command::PageDown | Command::Scroll(_) | Command::Select(..) | Command::SetCategory(..) => {
                self.pointer_pick = false
            }
            _ => {}
        }
        match cmd {
            Command::Up => self.move_selection(-1),
            Command::Down => self.move_selection(1),
            Command::PageUp => self.move_selection(-page),
            Command::PageDown => self.move_selection(page),
            Command::Scroll(d) => self.move_selection(d as i64),
            Command::Focus(side) => {
                if self.columns().contains(&side) {
                    self.focus = side;
                }
            }
            Command::Select(side, idx) | Command::Hover(side, idx) => {
                if !self.columns().contains(&side) || idx >= self.lists[side as usize].rows.len() {
                    return out;
                }
                if matches!(cmd, Command::Select(..)) && self.focus == side && self.selected[side as usize] == idx {
                    return self.apply(world, Command::One);
                }
                self.focus = side;
                self.selected[side as usize] = idx;
                if !self.pointer_pick {
                    self.ensure_visible(side);
                }
            }
            Command::SetCategory(side, cat) => {
                let i = side as usize;
                if !self.columns().contains(&side) || self.category[i] == cat {
                    return out;
                }
                self.category[i] = cat;
                self.folder = None;
                self.selected[i] = 0;
                self.scroll[i] = 0;
                if let Err(e) = self.refresh(world) {
                    self.status = Some(format!("Cannot list the inventories: {e}"));
                }
            }
            Command::Category(side, by) => {
                if self.mode == Mode::Inventory || !self.columns().contains(&side) {
                    return out;
                }
                let i = side as usize;
                self.category[i] = self.category[i].step(by);
                self.selected[i] = 0;
                self.scroll[i] = 0;
                if let Err(e) = self.refresh(world) {
                    self.status = Some(format!("Cannot list the inventories: {e}"));
                }
            }
            Command::Close => out.close = true,
            Command::Back => {
                if self.mode == Mode::Inventory && self.folder.is_some() {
                    if let Err(e) = self.enter(world, None) {
                        self.status = Some(format!("Cannot list the inventories: {e}"));
                    }
                }
            }
            Command::Amount(_) | Command::AmountStep(_) | Command::AmountOk | Command::AmountCancel => {}
            _ => {
                if let Some(q) = self.asks_quantity(cmd) {
                    self.quantity = Some(q);
                    self.status = None;
                    return out;
                }
                let r = self.act(world, cmd, &mut out);
                self.amount = None;
                self.status = Some(match r {
                    Ok(msg) => msg,
                    Err(e) => plain_error(world.forms, &e),
                });
                if out.changed {
                    if let Err(e) = self.refresh(world) {
                        self.status = Some(format!("Cannot list the inventories: {e}"));
                    }
                }
            }
        }
        out
    }

    /// Whether `g`'s row is still the row its input saw, in this menu and with no prompt over it.
    pub fn row_is(&self, g: &RowGuard) -> bool {
        self.mode == g.mode
            && self.quantity.is_none()
            && RowKey::at(&self.list(g.side).rows, g.index) == Some(g.key)
            && (!g.selected || (self.focus == g.side && self.selected[g.side as usize] == g.index))
    }

    /// The prompt asked for by `cmd` on the selected row, if its stack is large enough.
    fn asks_quantity(&self, cmd: Command) -> Option<Quantity> {
        let asks = matches!((self.mode, cmd), (Mode::Container(_) | Mode::Barter(_), Command::One) | (Mode::Inventory, Command::Drop));
        let r = self.selected_row(self.focus)?;
        (asks && self.amount.is_none() && r.count > ASK_QUANTITY_ABOVE && r.folder.is_none() && r.kind != Some(ItemKind::CaravanCard)).then(|| Quantity {
            cmd,
            side: self.focus,
            row: self.selected[self.focus as usize],
            max: r.count,
            amount: r.count,
        })
    }

    /// A command while the prompt is open: only the prompt's own (and Close, as Cancel).
    fn apply_quantity(&mut self, world: &mut World<'_>, cmd: Command) -> Applied {
        let Some(q) = self.quantity.as_mut() else { return Applied::default() };
        match cmd {
            Command::Amount(n) => q.amount = n.clamp(1, q.max),
            Command::AmountStep(d) => q.amount = q.amount.saturating_add(d).clamp(1, q.max),
            Command::AmountCancel | Command::Close => self.quantity = None,
            Command::AmountOk => {
                let q = *q;
                self.quantity = None;
                // The rows stand still while the prompt is open.
                self.focus = q.side;
                self.selected[q.side as usize] = q.row;
                self.amount = Some(q.amount);
                return self.apply(world, q.cmd);
            }
            _ => {}
        }
        Applied::default()
    }

    /// The units a command moves: the prompt's amount, else one or the whole stack.
    fn want(&mut self, single: bool, r: &Row) -> i32 {
        self.amount.take().unwrap_or(if single { 1 } else { r.count })
    }

    /// The commands that change inventories; returns the status message.
    fn act(&mut self, world: &mut World<'_>, cmd: Command, out: &mut Applied) -> Result<String, SimError> {
        let forms = world.forms;
        let sel = self.selected_row(self.focus).cloned();
        let name = |r: &Row| r.name.clone();
        match (self.mode, cmd) {
            (Mode::Container(_), Command::TakeAll) if self.steal.is_some() => Ok("Take items one at a time to steal them".into()),
            (Mode::Container(c), Command::TakeAll) => {
                // The listed stacks only: non-playable and nameless items stay hidden in the container.
                let mut moved = Vec::new();
                for r in &self.lists[Side::Container as usize].rows {
                    let n = match r.reference {
                        Some(at) => take_reference(world, at, &mut out.picked)?,
                        None => world.transfer_at(c, PLAYER, r.item, r.count, r.pick())?,
                    };
                    if n > 0 {
                        moved.push((r.item, n));
                    }
                }
                out.changed = !moved.is_empty();
                // Taking everything closes the menu (owner, list 5 item 60).
                out.close = out.changed;
                let n: i64 = moved.iter().map(|m| m.1 as i64).sum();
                out.taken.clone_from(&moved);
                Ok(if moved.is_empty() { "Nothing to take".into() } else { format!("Took everything ({n} items)") })
            }
            (Mode::Container(c), Command::One | Command::Stack) => {
                let Some(r) = sel else { return Ok("Nothing selected".into()) };
                let (from, to) = match self.focus {
                    Side::Container => (c, PLAYER),
                    Side::Player => (PLAYER, c),
                };
                let want = self.want(cmd == Command::One, &r);
                if let Some(st) = self.steal.as_mut() {
                    let chance = st.chance(&r, want);
                    if !st.unnoticed(chance) {
                        out.caught = Some(r.item);
                        out.close = true;
                        return Ok(format!("Caught! ({} x{want}, {chance}%)", name(&r)));
                    }
                }
                let n = match (r.reference, self.focus) {
                    (Some(at), Side::Container) => take_reference(world, at, &mut out.picked)?,
                    _ => world.transfer_at(from, to, r.item, want, r.pick())?,
                };
                out.changed = n > 0;
                if self.focus == Side::Container && n > 0 {
                    out.taken.push((r.item, n));
                }
                if self.focus == Side::Player && n > 0 && self.steal.is_some() {
                    out.given.push((r.item, n));
                }
                let verb = if self.focus == Side::Container { "Took" } else { "Stored" };
                Ok(format!("{verb} {} x{n}", name(&r)))
            }
            (Mode::Barter(_), Command::One | Command::Stack) => {
                let Some(r) = sel else { return Ok("Nothing selected".into()) };
                let want = self.want(cmd == Command::One, &r);
                let Some(t) = self.trade.as_mut() else { return Ok(String::new()) };
                out.changed = true;
                if r.bartered {
                    // Taken back by the side it came from.
                    let from = self.focus.other() as usize;
                    let left = t.offered[from].get(&r.units()).copied().unwrap_or(0) - want;
                    if left > 0 {
                        t.offered[from].insert(r.units(), left);
                    } else {
                        t.offered[from].remove(&r.units());
                    }
                    return Ok(format!("Took back {} x{want}", name(&r)));
                }
                *t.offered[self.focus as usize].entry(r.units()).or_insert(0) += want;
                let verb = if self.focus == Side::Player { "Selling" } else { "Buying" };
                Ok(format!("{verb} {} x{want}", name(&r)))
            }
            (Mode::Barter(_), Command::Accept) => self.accept(world, out),
            (Mode::Inventory, Command::One) => {
                let Some(r) = sel else { return Ok("Nothing selected".into()) };
                match r.folder {
                    Some(FolderRow::Open(f)) => {
                        self.enter(world, Some(f))?;
                        return Ok(format!("Opened {}", f.name()));
                    }
                    Some(FolderRow::Back) => {
                        self.enter(world, None)?;
                        return Ok("Back to Misc".into());
                    }
                    None => {}
                }
                // Caravan cards are for the Caravan table [design].
                if r.kind == Some(ItemKind::CaravanCard) {
                    return Ok(format!("{} is played at a Caravan table", name(&r)));
                }
                out.changed = true;
                // An item a script locked on or off stays as it is (EquipItem's
                // NoUnequipFlag, UnequipItem's NoEquipFlag, `fnv_sim::world::EquipLock`)
                // [documented: GECK wiki EquipItem, "actor (including the player) will be
                // unable to unequip the item"; the line is ours].
                if r.equipped {
                    match world.unequip(PLAYER, r.item) {
                        Err(SimError::EquipLocked { .. }) => return Ok(format!("{} can't be unequipped", name(&r))),
                        other => other?,
                    };
                    return Ok(format!("Unequipped {}", name(&r)));
                }
                let e = match world.equip_at(PLAYER, r.item, r.pick()) {
                    Err(SimError::EquipLocked { item, .. }) if item == r.item => return Ok(format!("{} can't be equipped", name(&r))),
                    Err(SimError::EquipLocked { item, .. }) => return Ok(format!("{} can't be equipped while {} is worn", name(&r), display_name(forms, item))),
                    Err(SimError::NoPowerArmorTraining(_)) => {
                        out.power_armor_refused = true;
                        return Ok(format!("Cannot wear {} without Power Armor Training", name(&r)));
                    }
                    other => other?,
                };
                if e.consumed {
                    out.used = Some(r.item);
                }
                let off: Vec<String> = e.unequipped.iter().map(|&i| display_name(forms, i)).collect();
                Ok(match (e.consumed, off.is_empty()) {
                    (true, _) => format!("Used {}", name(&r)),
                    (false, true) => format!("Equipped {}", name(&r)),
                    (false, false) => format!("Equipped {} (took off {})", name(&r), off.join(", ")),
                })
            }
            (Mode::Inventory, Command::Drop | Command::DropStack) => {
                let Some(r) = sel else { return Ok("Nothing selected".into()) };
                // A folder row is no item; the cards AddCardToPlayer gave are not in the
                // inventory [design: carried CCRD items are not dropped either].
                if r.folder.is_some() {
                    return Ok("Nothing to drop".into());
                }
                if r.kind == Some(ItemKind::CaravanCard) {
                    return Ok(format!("{} can't be dropped", name(&r)));
                }
                let want = self.want(cmd == Command::Drop, &r);
                let created = match world.player_drop_item_at(r.item, want, r.pick()) {
                    Err(SimError::QuestItem(_)) => return Ok(format!("{} is a quest item and can't be dropped", name(&r))),
                    other => other?,
                };
                out.changed = !created.is_empty();
                self.dropped.extend(&created);
                out.created = created;
                Ok(format!("Dropped {} x{want}", name(&r)))
            }
            (Mode::Container(_) | Mode::Barter(_), Command::Drop | Command::DropStack) => Ok("Drop from the inventory menu (Tab)".into()),
            (Mode::Inventory, Command::Stack | Command::TakeAll) => Ok("No container open".into()),
            _ => Ok(String::new()),
        }
    }

    /// Barter: move what was put up and the caps it comes to, if the player can pay; a
    /// merchant who cannot pays what it holds ([`Trade::paid`]).
    fn accept(&mut self, world: &mut World<'_>, out: &mut Applied) -> Result<String, SimError> {
        let Some(t) = self.trade.as_mut() else { return Ok(String::new()) };
        if t.is_empty() {
            return Ok("Nothing to trade".into());
        }
        let (player, merchant) = (Side::Player as usize, Side::Container as usize);
        if t.balance < 0 && !t.affordable() {
            return Ok("You don't have enough caps.".into());
        }
        for (u, &n) in &t.offered[player] {
            world.transfer_at(PLAYER, t.stock, u.0, n, units_pick(u))?;
        }
        for (u, &n) in &t.offered[merchant] {
            world.transfer_at(t.stock, PLAYER, u.0, n, units_pick(u))?;
        }
        let paid = t.paid();
        match paid {
            p if p > 0 => world.transfer(t.stock, PLAYER, t.caps, p as i32)?,
            p if p < 0 => world.transfer(PLAYER, t.stock, t.caps, (-p) as i32)?,
            _ => 0,
        };
        t.offered = [BTreeMap::new(), BTreeMap::new()];
        out.changed = true;
        out.traded = paid.unsigned_abs() as u32;
        Ok(match paid {
            p if p > 0 => format!("Traded: received {p} caps"),
            p if p < 0 => format!("Traded: paid {} caps", -p),
            _ => "Traded".into(),
        })
    }

    /// Key hints for the footer, for the current mode, one line each.
    pub fn bindings(&self) -> [&'static str; 2] {
        match self.mode {
            Mode::Container(_) => [
                "Up/Down select   Left/Right column   Enter/Space move one   Shift+Enter move stack   R take all",
                "Click a row to select it, again to move one   Wheel scroll   E/Esc/Tab close   * equipped   Q quest item",
            ],
            Mode::Barter(_) => [
                "Up/Down select   Left/Right column   Enter/Space put up or take back one   Shift+Enter the stack   A accept",
                "Click a row to put it up or take it back   [ ] categories   Wheel scroll   X/Esc/Tab exit",
            ],
            Mode::Inventory => [
                "Up/Down select   Enter/Space equip/unequip (aid: use)   Right click drop one   Shift+right click drop stack",
                "Click a row to select it, again to equip   Wheel scroll   Tab/Esc/E close   * equipped   Q quest item",
            ],
        }
    }
}

/// Test aid (F10): the container reference nearest to `pos` (game space) that is in the
/// world now, with its distance. `interior`: search that cell; `None`: the worldspace
/// `world`. Containers are references whose base is a CONT.
pub fn nearest_container(
    world: &World<'_>,
    refs: &RefTable,
    interior: Option<FormId>,
    worldspace: FormId,
    pos: [f32; 3],
) -> Option<(FormId, f32)> {
    let forms = world.forms;
    let candidates: Box<dyn Iterator<Item = &PlacedRef>> = match interior {
        Some(cell) => Box::new(refs.by_cell.get(&cell).into_iter().flatten().filter_map(|&id| refs.get(id))),
        None => Box::new(refs.refs.values().filter(|p| p.cell.and_then(|c| refs.cell_world.get(&c)) == Some(&worldspace))),
    };
    candidates
        .filter_map(|p| {
            let (base, at, gone) = match world.peek(p.id) {
                Some(s) => (s.base, s.pos, s.deleted()),
                None => (p.base, p.pos, false),
            };
            let is_container = matches!(forms.get(base).map(|b| &b.data), Some(BaseData::Container(_)));
            let present = !gone && world.is_enabled(p.id).unwrap_or(true);
            let d = ((at[0] - pos[0]).powi(2) + (at[1] - pos[1]).powi(2) + (at[2] - pos[2]).powi(2)).sqrt();
            (is_container && present).then_some((p.id, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
}

#[cfg(test)]
mod tests;
