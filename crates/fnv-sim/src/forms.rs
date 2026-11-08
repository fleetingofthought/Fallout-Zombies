//! Base forms the simulation needs, read from the winning record of every FormID.
//!
//! Only the record types that take part in Layer 4 (items, containers, doors,
//! activators, actors' carried items, leveled item lists, globals) are interpreted;
//! every other winner is kept as its signature so a reference to it can still be
//! classified. Field meanings come from the Layer 1 schema (see `extract`).

use fnv_esm::raw::record_flags;
use fnv_esm::{LoadOrder, Record, Schema};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

use crate::extract::{self, first, global, has_flag, int, local_fid, num, text, Decoded, Issues};
use crate::form::FormId;
use crate::inventory::Extra;
use crate::settings::{unhex, Setting};

/// Kinds of form that can sit in an inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Weapon,
    Armor,
    Ammo,
    Aid,
    Book,
    Key,
    Misc,
    Note,
    Light,
    ItemMod,
    CasinoChip,
    CaravanCard,
    CaravanMoney,
}

impl ItemKind {
    /// Record types whose forms are inventory items. LIGH counts only when its
    /// "Can be Carried" flag is set. [documented: xEdit wbDefinitionsFNV.pas lists
    /// these record types as valid CNTO items; LIGH flag name from the same file]
    pub fn from_sig(sig: &[u8; 4]) -> Option<ItemKind> {
        Some(match sig {
            b"WEAP" => ItemKind::Weapon,
            b"ARMO" => ItemKind::Armor,
            b"AMMO" => ItemKind::Ammo,
            b"ALCH" => ItemKind::Aid,
            b"BOOK" => ItemKind::Book,
            b"KEYM" => ItemKind::Key,
            b"MISC" => ItemKind::Misc,
            b"NOTE" => ItemKind::Note,
            b"LIGH" => ItemKind::Light,
            b"IMOD" => ItemKind::ItemMod,
            b"CHIP" => ItemKind::CasinoChip,
            b"CCRD" => ItemKind::CaravanCard,
            b"CMNY" => ItemKind::CaravanMoney,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub kind: ItemKind,
    /// `None` when the schema has no layout for this record type's value yet.
    pub value: Option<i32>,
    /// CMNY (caravan money: the NCR notes and Legion coins found in the world) DATA
    /// "Absolute Value": what the note or coin is worth in caps. Kept apart from `value`,
    /// the record's Value field, which CMNY has none of; [`Item::worth`] is either
    /// [observed: FalloutNV.esm MoneyNCR5/20/100 2, 8, 40, MoneyLegion1 4, MoneyLegion25
    /// 100, the Value of the MISC notes and coins of the same names; the CMNY records are
    /// the ones inventories and leveled lists name, the MISC ones hardly; owner's
    /// playtest list 7, item 7].
    #[serde(skip)]
    pub money: Option<i32>,
    pub weight: Option<f32>,
    /// ARMO BMDT biped flags: the body slots the item occupies when worn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slots: Option<u32>,
    /// WEAP DNAM: the projectile the weapon fires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projectile: Option<FormId>,
    /// WEAP/ARMO DATA: full health (condition 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<i32>,
    /// The "Non-Playable" flag: ARMO BMDT general flags, WEAP DNAM flags 1, AMMO DATA
    /// flags [documented: xEdit wbBMDT, wbDefinitionsFNV.pas WEAP and AMMO]. The Pip-Boy
    /// 3000 and the Pip-Boy Glove carry it [observed: `fnvtool esm decode`]; the game never
    /// shows the glove in the inventory and it stays equipped [observed by the owner,
    /// 2026-10-08]. Boone's rifle and machete and `AmmoCompanion` ("Magical Companion
    /// Ammo") carry it too [observed: `fnvtool esm decode`]. What it hides differs by type
    /// ([`Item::listed`], [`Item::non_playable_biped`]). Not part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub non_playable: bool,
    /// WEAP DNAM flags 1 "Can't Drop" [documented: xEdit]: the weapon does not leave the
    /// actor's hands, not at death and not shot out of them [documented: GECK wiki
    /// Weapon, Flags]. Not part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub cant_drop: bool,
    /// ARMO BMDT general flags "Medium" and "Heavy" [documented: xEdit wbBMDT]: the
    /// armour's class, light when neither is set. Detection's armour penalty reads it
    /// (docs/spec-notes/actor-behaviour.md §2.1). Not part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub armor_class: ArmorClass,
    /// ARMO BMDT general flag "Power Armor" [documented: xEdit wbBMDT]: the player wears
    /// it only with the training (`World::set_power_armor_training`). Not part of
    /// `fnv.sim.items/1`.
    #[serde(skip)]
    pub power_armor: bool,
    /// ARMO DNAM "DR", damage resistance in percent (the record stores it × 100
    /// [documented: xEdit wbDefinitionsFNV.pas, wbDiv(100)]). `None` for other items. Not
    /// part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub armor_dr: Option<f32>,
    /// ARMO DNAM "DT", damage threshold [documented: xEdit wbDefinitionsFNV.pas]. `None`
    /// for other items. The wearing rule ranks armour by it (`World::equip_default`). Not
    /// part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub armor_dt: Option<f32>,
    /// The damage the weapon choice ranks a weapon by ([`weapon_choice_damage`]): WEAP
    /// DATA "Base Damage" [documented: xEdit], plus the damage of the explosion its
    /// projectile sets off unless it is thrown or laid or an Explosives weapon. `None` for other items. Not part of
    /// `fnv.sim.items/1`.
    #[serde(skip)]
    pub weapon_damage: Option<f32>,
    /// WEAP DNAM: what each mod slot (1, 2, 3) takes off the weight when its mod is on,
    /// the value A of a "Decrease Weight" effect, 0 for the other effects and other items
    /// (docs/spec-notes/weapon-mods.md §2, §3; [`Item::weight_with_mods`]). Not part of
    /// `fnv.sim.items/1`.
    #[serde(skip)]
    pub weight_mods: [f32; 3],
    /// A weapon an actor can draw: it has a model (MODL), is not part of a creature's body
    /// (DNAM flag "Embedded Weapon") and names an animation type [the engine's rule,
    /// `fnv_actor::weapon::drawable`]. Not part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub drawable: bool,
    /// A weapon whose DNAM flags2 say "Player Only" or "Not Used In Normal Combat": no
    /// NPC's own weapon choice takes it (docs/spec-notes/weapon-data-fields.md §1). Not part
    /// of `fnv.sim.items/1`.
    #[serde(skip)]
    pub not_for_npcs: bool,
    /// A weapon thrown or laid rather than held: its animation type is one of
    /// [`THROWN_ANIM_TYPES`]. An actor's own choice holds one only when it carries no other
    /// weapon it can draw (docs/spec-notes/actors.md §9.3). Not part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub thrown: bool,
    /// The record header's "Quest Item" flag, as `schemas/fnv` names it per record type
    /// (`header_flags`; bit 10 [documented: xEdit wbDefinitionsFNV.pas for WEAP, ARMO,
    /// ALCH, BOOK, KEYM, MISC, LIGH; GECK wiki IsQuestItem and GetFormFlag for AMMO and
    /// IMOD]). The player cannot drop or sell such an item [documented: GECK wiki
    /// Category:Items, SetQuestObject]; scripts can override it at run time
    /// (`World::set_quest_object`). Not part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub quest_item: bool,
    /// ICON: the picture the menus show of the item, a path under `textures\`
    /// [documented: xEdit "Large Icon FileName"; observed: `interface\icons\pipboyimages\
    /// items\...` on MISC, AMMO and ALCH records]. Not part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub icon: Option<String>,
    /// How many units a reference placed in the world without a count (XCNT) stands for:
    /// an AMMO record's DATA "Clip Rounds" [documented: GECK wiki Ammo, "the number of
    /// rounds of ammo the object contains when placed in the world", a placed
    /// reference's Count overriding it; observed: the two Ammo9mm references on the
    /// Goodsprings gas station's counter carry no XCNT and Ammo9mm's Clip Rounds is 20,
    /// which the owner says the game gives]. `None` for other items: one unit. Not part
    /// of `fnv.sim.items/1`.
    #[serde(skip)]
    pub placed_count: Option<i32>,
    /// A skill book's skill: BOOK DATA "Skill" (the "Teaches" field of the editor) as
    /// an actor value code, Barter 32 … Unarmed 45; `None` for "None" and other items
    /// [documented: xEdit wbDefinitionsFNV.pas BOOK DATA, wbSkillEnum -1 None, 0 Barter
    /// … 13 Unarmed, `schemas/fnv`; GECK wiki Book, "Teaches: The Actor Value that is
    /// increased"]. Reading it is `fnv_game::books` (docs/spec-notes/skill-books.md). Not
    /// part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub teaches: Option<u16>,
    /// BOOK DATA flag "Can't be Taken" (bit 1) [documented: xEdit; GECK wiki
    /// GetBookFlags]. A placed reference of such a book is not picked up by hand
    /// ([`crate::World::activate`]; skill-books.md §4). Not part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub cant_be_taken: bool,
    /// ARMO DNAM flag "Modulates Voice" (bit 0): "the item modulates the actor's voice"
    /// [documented: xEdit wbDefinitionsFNV.pas ARMO DNAM; GECK wiki Armor, ArmorAddon
    /// ("like Power Armor helmets do")]. Its wearer's lines play through the voice filter
    /// (nvf-play `voice_filter`). Not part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub modulates_voice: bool,
    /// What a weapon's rank among weapons reads (`fnv_game::rearm`, docs/design/combat-ai.md
    /// §22). `None` for other items. Not part of `fnv.sim.items/1`.
    #[serde(skip)]
    pub weapon_traits: Option<WeaponTraits>,
}

/// A weapon's WEAP fields that its damage per second and its use by an actor depend on
/// [documented: xEdit wbDefinitionsFNV.pas WEAP DNAM, NAM0].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WeaponTraits {
    /// DNAM Skill as an actor value code, Barter 32 … Unarmed 45 (`None` for none).
    pub skill: Option<u16>,
    /// DNAM Flags 1 "Is Automatic".
    pub automatic: bool,
    /// DNAM Fire Rate: shots per second of an automatic weapon.
    pub fire_rate: f32,
    /// DNAM Attack Shots/Sec.
    pub shots_per_sec: f32,
    /// DNAM Attack Mult: the attack animation's speed.
    pub attack_mult: f32,
    /// NAM0: the AMMO it fires, or a form list of acceptable AMMO (global FormID).
    pub ammo: Option<FormId>,
    /// DNAM Max Range, game units (0: none).
    pub max_range: f32,
    /// DNAM Flags 2 "Range - Fixed": its combat style does not scale its range.
    pub range_fixed: bool,
    /// Its own projectile's PROJ DATA Range (0: none), explosion radius (EXPL DATA
    /// Radius, 0 without one) and type Flame (set by the caller, which reads the PROJ).
    pub projectile_range: f32,
    pub blast: f32,
    pub flame: bool,
    /// WMI1-WMI3: the item mod (IMOD) of each mod slot (global FormID; set by the caller).
    pub mods: [Option<FormId>; 3],
}

impl Item {
    /// Whether a menu lists the item in an inventory, the player's (`players`) or another
    /// holder's: non-playable items are not shown in inventories other than the player's,
    /// and non-playable biped items (armour) not in the player's either, while
    /// non-playable weapons and ammo are [documented: GECK wiki "Unplayable Items"]. Other
    /// kinds never carry the flag in the records; set on one, it hides it everywhere.
    /// What the item is worth in caps: its Value, or a note's or coin's
    /// [`Item::money`].
    pub fn worth(&self) -> Option<i32> {
        self.value.or(self.money)
    }

    pub fn listed(&self, players: bool) -> bool {
        !self.non_playable || (players && matches!(self.kind, ItemKind::Weapon | ItemKind::Ammo))
    }

    /// A non-playable biped item: RemoveAllItems and RemoveAllTypedItems leave it where
    /// it is [documented: GECK wiki RemoveAllItems and RemoveAllTypedItems, the exception
    /// is "Unplayable Biped items"; non-playable weapons and ammo move like the rest].
    pub fn non_playable_biped(&self) -> bool {
        self.non_playable && self.kind == ItemKind::Armor
    }

    /// The weight of one unit with the mod flags `mods` on (1 slot 1, 2 slot 2, 4 slot 3):
    /// each Decrease Weight mod on takes its value off, never below 0, as
    /// `fnv_combat::WeaponStats::modded` does for the figures (weapon-mods.md §3).
    pub fn weight_with_mods(&self, mods: u8) -> Option<f32> {
        let mut w = self.weight?;
        for (slot, a) in self.weight_mods.iter().enumerate() {
            if mods & (1 << slot) != 0 && *a != 0.0 {
                w = (w - a).max(0.0);
            }
        }
        Some(w)
    }
}

/// The weight class of a piece of armour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub enum ArmorClass {
    #[default]
    Light,
    Medium,
    Heavy,
}

/// The name `schemas/fnv` gives the quest-item bit in `header_flags`.
pub const QUEST_ITEM_FLAG: &str = "Quest Item";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Entry {
    pub item: FormId,
    pub count: i32,
    /// COED that follows the CNTO: condition and owner of these items.
    #[serde(skip_serializing_if = "Extra::is_plain")]
    pub extra: Extra,
}

impl Entry {
    pub fn new(item: FormId, count: i32) -> Entry {
        Entry { item, count, extra: Extra::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Container {
    pub respawns: bool,
    /// CNTO entries in record order; an entry may name a leveled item list.
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActorBase {
    /// Items the actor carries (CNTO), like a container's.
    pub entries: Vec<Entry>,
    /// NPC_ DATA attributes (S.P.E.C.I.A.L., in that order); `None` for creatures.
    pub attributes: Option<[u8; 7]>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Door {
    pub automatic: bool,
    pub minimal_use: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize)]
pub struct LeveledEntry {
    pub level: u16,
    pub item: FormId,
    pub count: u16,
    /// The condition of the items it gives, a fraction of full health: the entry's
    /// COED, or AddItemToLeveledList's health (`World::add_to_leveled_list`); `None`
    /// for entries without one (whole items).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeveledList {
    pub chance_none: u8,
    /// Flag "Calculate from all levels <= player's level".
    pub all_levels: bool,
    /// Flag "Calculate for each item in count".
    pub each: bool,
    /// Flag "Use All".
    pub use_all: bool,
    /// LVLG: a global whose current value replaces `chance_none` [observed in game,
    /// `L4.sim.leveled.b8`; simulation.md §4].
    pub global: Option<FormId>,
    pub entries: Vec<LeveledEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaseData {
    Item(Item),
    Container(Container),
    Door(Door),
    Activator,
    Furniture,
    Actor(ActorBase),
    LeveledItem(LeveledList),
    /// PROJ, the base of a placed explosive (PGRE). `item` is the weapon that fires
    /// this projectile, when exactly one does (what disarming yields).
    /// `item`: the weapon that fires it, the loose item a disarmed one becomes; linked
    /// only when the PROJ's DATA flag "Can Be Disabled" is set (or it has no DATA)
    /// [documented: xEdit PROJ flags] (docs/spec-notes/object-fields.md §1).
    Projectile { item: Option<FormId>, can_be_disabled: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BaseForm {
    pub id: FormId,
    #[serde(serialize_with = "ser_sig")]
    pub sig: [u8; 4],
    pub edid: Option<String>,
    pub name: Option<String>,
    pub script: Option<FormId>,
    /// ACTI XATO Activation Prompt (docs/spec-notes/cells-refs.md §3); a reference's own
    /// XATO comes first. Not written to JSON.
    #[serde(skip)]
    pub prompt: Option<String>,
    /// A talking activator (TACT) without the record header flag "No Voice Filter"
    /// ([`NO_VOICE_FILTER`]; 2 of FalloutNV.esm's 87 set it, both radios). Read by
    /// nothing in the engine: the original does not filter a talking activator's lines
    /// (nvf-play `voice_filter`; observed 2026-11-06, issue #992). Not written to JSON.
    #[serde(skip)]
    pub voice_filter: bool,
    #[serde(flatten)]
    pub data: BaseData,
}

/// Record header bit 13 of a TACT, "No Voice Filter" (8192) [documented: GECK wiki
/// GetFormFlag bit table, adapted from FNVEdit; owner export].
pub const NO_VOICE_FILTER: u32 = 1 << 13;

pub(crate) fn ser_sig<S: serde::Serializer>(sig: &[u8; 4], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&String::from_utf8_lossy(sig))
}

/// Other record types the simulation reads: globals, game settings and encounter zones.
pub const OTHER_SIGS: &[&[u8; 4]] = &[b"GLOB", b"GMST", b"ECZN"];

/// An encounter zone (ECZN DATA) [documented: xEdit wbDefinitionsFNV.pas ECZN; GECK wiki
/// GetZoneFlag bits]: docs/spec-notes/encounter-zones.md §1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ZoneDef {
    pub owner: Option<FormId>,
    pub rank: i8,
    pub min_level: i8,
    /// Flag bit 0: the zone's cells are never reset (no respawn, no container refill).
    pub never_resets: bool,
    /// Flag bit 1: a player below the minimum level gets the zone at their own level.
    pub match_pc_below_min: bool,
}

/// Record types interpreted as base forms.
pub const BASE_SIGS: &[&[u8; 4]] = &[
    b"WEAP", b"ARMO", b"AMMO", b"ALCH", b"BOOK", b"KEYM", b"MISC", b"NOTE", b"LIGH", b"IMOD", b"CHIP", b"CCRD",
    b"CMNY", b"CONT", b"DOOR", b"ACTI", b"TACT", b"FURN", b"NPC_", b"CREA", b"LVLI", b"PROJ", b"TERM",
];

#[derive(Default)]
pub struct FormStore {
    pub bases: BTreeMap<FormId, BaseForm>,
    /// Signature of the winning record of every global FormID.
    pub sigs: HashMap<FormId, [u8; 4]>,
    /// Lower-case Editor ID -> FormID, for the base forms and globals.
    pub edids: HashMap<String, FormId>,
    /// GLOB values as authored.
    pub globals: HashMap<FormId, f32>,
    /// GMST values by lower-case Editor ID.
    pub settings: HashMap<String, Setting>,
    /// Encounter zones (ECZN) by FormID.
    pub zones: HashMap<FormId, ZoneDef>,
    pub issues: Issues,
}

impl FormStore {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> FormStore {
        let mut store = FormStore::default();
        let mut wanted: Vec<(FormId, usize, &Record)> = Vec::new();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            if r.header.flags & record_flags::DELETED != 0 {
                // Deleted references are counted by `RefTable`; report base forms only.
                if BASE_SIGS.contains(&&r.header.sig) || OTHER_SIGS.contains(&&r.header.sig) {
                    store.issues.note("winning record is deleted", || format!("0x{g:08X} {}", r.sig_str()));
                }
                continue;
            }
            store.sigs.insert(FormId(g), r.header.sig);
            if BASE_SIGS.contains(&&r.header.sig) || OTHER_SIGS.contains(&&r.header.sig) {
                wanted.push((FormId(g), d.plugin as usize, r));
            }
        }
        wanted.sort_by_key(|w| w.0);
        let loaded: Vec<(FormId, Loaded, Issues)> = wanted
            .par_iter()
            .map(|&(id, pi, r)| {
                let mut issues = Issues::default();
                let l = load_one(lo, schema, id, pi, r, &mut issues);
                (id, l, issues)
            })
            .collect();
        let loaded = link_projectiles(loaded);
        for (id, l, issues) in loaded {
            store.issues.merge(issues);
            match l {
                Loaded::Base(b) => {
                    if let Some(e) = &b.edid {
                        store.edids.insert(e.to_ascii_lowercase(), id);
                    }
                    store.bases.insert(id, *b);
                }
                Loaded::Global(edid, v) => {
                    if let Some(e) = edid {
                        store.edids.insert(e.to_ascii_lowercase(), id);
                    }
                    store.globals.insert(id, v);
                }
                Loaded::Setting(edid, v) => {
                    if let Setting::Raw(_) = v {
                        store.issues.note("GMST with unknown prefix or size", || edid.clone());
                    }
                    store.settings.insert(edid.to_ascii_lowercase(), v);
                }
                Loaded::Zone(edid, z) => {
                    if let Some(e) = edid {
                        store.edids.insert(e.to_ascii_lowercase(), id);
                    }
                    store.zones.insert(id, z);
                }
                Loaded::Skip => {}
            }
        }
        store
    }

    pub fn get(&self, id: FormId) -> Option<&BaseForm> {
        self.bases.get(&id)
    }

    pub fn item(&self, id: FormId) -> Option<&Item> {
        match &self.bases.get(&id)?.data {
            BaseData::Item(i) => Some(i),
            _ => None,
        }
    }

    /// The value the attached mods add to a weapon: the IMOD DATA value of each slot whose
    /// flag (1, 2, 4 for slots 1-3) is in `mods`. Condition does not lower it [observed:
    /// o26_value, 2026-11-06: a Varmint Rifle (75) with Extended Mags (75), Night Scope
    /// (125) and Silencer (100) valued 150, 200, 175, 275 and 375 with mods 1, 2, 4, 3 and
    /// 7 at full condition, and 326, 309 and 349 with all three at 50, 25 and 75 % (26,
    /// 9 and 49 without), by ShowOff's GetCalculatedItemValue, the value its page says
    /// the screen shows; docs/spec-notes/weapon-mods.md §6, issue #982].
    pub fn mods_value(&self, weapon: FormId, mods: u8) -> i32 {
        let Some(t) = self.item(weapon).and_then(|i| i.weapon_traits.as_ref()) else { return 0 };
        t.mods
            .iter()
            .enumerate()
            .filter(|(k, _)| mods & (1 << k) != 0)
            .filter_map(|(_, m)| m.and_then(|m| self.item(m)).and_then(|i| i.value))
            .sum()
    }

    /// How many units a reference of `base` placed in the world stands for: its own count
    /// (XCNT) when it has one, else the base's [`Item::placed_count`], else 1.
    pub fn placed_count(&self, base: FormId, xcnt: Option<i32>) -> i32 {
        xcnt.or_else(|| self.item(base).and_then(|i| i.placed_count)).unwrap_or(1)
    }

    /// Whether the item's record carries the "Quest Item" header flag (the authored
    /// value; `World::is_quest_item` adds scripts' overrides).
    pub fn is_quest_item(&self, id: FormId) -> bool {
        self.item(id).is_some_and(|i| i.quest_item)
    }

    pub fn leveled(&self, id: FormId) -> Option<&LeveledList> {
        match &self.bases.get(&id)?.data {
            BaseData::LeveledItem(l) => Some(l),
            _ => None,
        }
    }

    /// Carried-item entries of a container or actor base.
    pub fn entries(&self, id: FormId) -> Option<&[Entry]> {
        match &self.bases.get(&id)?.data {
            BaseData::Container(c) => Some(&c.entries),
            BaseData::Actor(a) => Some(&a.entries),
            _ => None,
        }
    }

    pub fn by_edid(&self, edid: &str) -> Option<FormId> {
        self.edids.get(&edid.to_ascii_lowercase()).copied()
    }

    /// A numeric game setting by Editor ID.
    pub fn setting_f32(&self, edid: &str) -> Option<f32> {
        self.settings.get(&edid.to_ascii_lowercase())?.as_f32()
    }

    /// A global's authored value by Editor ID.
    pub fn global_by_edid(&self, edid: &str) -> Option<(FormId, f32)> {
        let id = self.by_edid(edid)?;
        Some((id, *self.globals.get(&id)?))
    }

    pub fn sig(&self, id: FormId) -> Option<[u8; 4]> {
        self.sigs.get(&id).copied()
    }
}

/// Give every PROJ base the weapon that fires it, when exactly one weapon does.
fn link_projectiles(mut loaded: Vec<(FormId, Loaded, Issues)>) -> Vec<(FormId, Loaded, Issues)> {
    let mut firing: HashMap<FormId, Vec<FormId>> = HashMap::new();
    for (id, l, _) in &loaded {
        if let Loaded::Base(b) = l {
            if let BaseData::Item(Item { kind: ItemKind::Weapon, projectile: Some(p), .. }) = &b.data {
                firing.entry(*p).or_default().push(*id);
            }
        }
    }
    for (_, l, issues) in &mut loaded {
        if let Loaded::Base(b) = l {
            if let BaseData::Projectile { item, can_be_disabled } = &mut b.data {
                match firing.get(&b.id).map(Vec::as_slice) {
                    Some([w]) if *can_be_disabled => *item = Some(*w),
                    Some([_]) => {}
                    Some(ws) => issues.note("PROJ fired by several weapons", || format!("{} {:?}", b.id, ws)),
                    None => {}
                }
            }
        }
    }
    loaded
}

enum Loaded {
    Base(Box<BaseForm>),
    Global(Option<String>, f32),
    Setting(String, Setting),
    Zone(Option<String>, ZoneDef),
    Skip,
}

fn load_one(lo: &LoadOrder, schema: &Schema, id: FormId, pi: usize, r: &Record, issues: &mut Issues) -> Loaded {
    let sig = r.header.sig;
    let sig_s = String::from_utf8_lossy(&sig).into_owned();
    let d = match extract::decode(r, schema) {
        Ok(d) => d,
        Err(e) => {
            issues.note("record undecodable", || format!("{id} {sig_s}: {e}"));
            // An undecodable item is still an item (fields unknown); an undecodable LIGH
            // cannot be shown to be carriable.
            return match ItemKind::from_sig(&sig).filter(|k| *k != ItemKind::Light) {
                Some(kind) => Loaded::Base(Box::new(BaseForm {
                    id,
                    sig,
                    edid: None,
                    name: None,
                    script: None,
                    prompt: None,
                    voice_filter: false,
                    data: BaseData::Item(Item {
                        kind,
                        value: None,
                        money: None,
                        weight: None,
                        slots: None,
                        projectile: None,
                        health: None,
                        non_playable: false,
                        cant_drop: false,
                        armor_class: ArmorClass::Light,
                        power_armor: false,
                        armor_dr: None,
                        armor_dt: None,
                        weapon_damage: None,
                        weight_mods: [0.0; 3],
                        drawable: false, not_for_npcs: false, thrown: false,
                        quest_item: quest_flag(schema, r),
                        icon: None,
                        placed_count: None,
                        teaches: None,
                        cant_be_taken: false,
                        modulates_voice: false,
                        weapon_traits: None,
                    }),
                })),
                None => Loaded::Skip,
            };
        }
    };
    let edid = first(&d, b"EDID").and_then(text);
    if &sig == b"GMST" {
        let Some(edid) = edid else {
            issues.note("GMST without EDID", || format!("{id}"));
            return Loaded::Skip;
        };
        return match first(&d, b"DATA").and_then(Value::as_str).and_then(unhex) {
            Some(bytes) => Loaded::Setting(edid.clone(), Setting::from_bytes(&edid, &bytes)),
            None => {
                issues.note("GMST without DATA", || edid.clone());
                Loaded::Skip
            }
        };
    }
    if &sig == b"GLOB" {
        return match first(&d, b"FLTV").and_then(Value::as_f64) {
            Some(v) => Loaded::Global(edid, v as f32),
            None => {
                issues.note("GLOB without FLTV value", || format!("{id}"));
                Loaded::Skip
            }
        };
    }
    if &sig == b"ECZN" {
        let Some(v) = first(&d, b"DATA") else {
            issues.note("ECZN without DATA", || format!("{id}"));
            return Loaded::Skip;
        };
        let owner = local_fid(v.get("owner").unwrap_or(&Value::Null)).and_then(|l| global(lo, pi, l).ok().flatten());
        let n = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0) as i8;
        let flags = v.get("flags").cloned().unwrap_or(Value::Null);
        return Loaded::Zone(
            edid,
            ZoneDef {
                owner,
                rank: n("rank"),
                min_level: n("minimum_level"),
                never_resets: crate::extract::has_flag(&flags, "Never Resets"),
                match_pc_below_min: crate::extract::has_flag(&flags, "Match PC Below Minimum Level"),
            },
        );
    }
    let mut fid = |v: Option<&Value>, what: &str| -> Option<FormId> {
        let local = local_fid(v?)?;
        match global(lo, pi, local) {
            Ok(g) => g,
            Err(l) => {
                issues.note("FormID outside master list", || format!("{id} {sig_s} {what} 0x{l:08X}"));
                None
            }
        }
    };
    let name = first(&d, b"FULL").and_then(text);
    let script = fid(first(&d, b"SCRI"), "SCRI");
    let data = if let Some(kind) = ItemKind::from_sig(&sig) {
        match item(kind, &d) {
            Some(mut i) => {
                i.quest_item = quest_flag(schema, r);
                if (i.value.is_none() || i.weight.is_none()) && kind_has_value_weight(kind) {
                    issues.note(&format!("{sig_s}: value/weight not decoded"), || format!("{id}"));
                }
                i.projectile = match i.projectile.map(|p| global(lo, pi, p.0)) {
                    Some(Ok(g)) => g,
                    Some(Err(l)) => {
                        issues.note("FormID outside master list", || format!("{id} DNAM projectile 0x{l:08X}"));
                        None
                    }
                    None => None,
                };
                if kind == ItemKind::Weapon {
                    let anim = first(&d, b"DNAM").and_then(|v| v.get("animation_type")).and_then(text);
                    let skill = first(&d, b"DNAM").and_then(|v| v.get("skill")).and_then(text);
                    let proj = i.projectile.map(|p| projectile_facts(lo, schema, p)).unwrap_or_default();
                    i.weapon_damage = i.weapon_damage.map(|base| weapon_choice_damage(base, proj.explosion_damage, anim.as_deref(), skill.as_deref()));
                    if let Some(t) = i.weapon_traits.as_mut() {
                        (t.projectile_range, t.blast, t.flame) = (proj.range, proj.blast, proj.flame);
                    }
                }
                if let Some(t) = i.weapon_traits.as_mut() {
                    t.ammo = match t.ammo.map(|a| global(lo, pi, a.0)) {
                        Some(Ok(g)) => g,
                        Some(Err(l)) => {
                            issues.note("FormID outside master list", || format!("{id} NAM0 ammo 0x{l:08X}"));
                            None
                        }
                        None => None,
                    };
                    for m in t.mods.iter_mut() {
                        *m = match m.map(|x| global(lo, pi, x.0)) {
                            Some(Ok(g)) => g,
                            Some(Err(l)) => {
                                issues.note("FormID outside master list", || format!("{id} WMI mod 0x{l:08X}"));
                                None
                            }
                            None => None,
                        };
                    }
                }
                BaseData::Item(i)
            }
            None => return Loaded::Skip, // non-carriable light
        }
    } else {
        match &sig {
            b"CONT" => {
                let respawns = first(&d, b"DATA").and_then(|v| v.get("flags")).is_some_and(|f| has_flag(f, "Respawns"));
                BaseData::Container(Container { respawns, entries: entries(lo, pi, id, &d, issues) })
            }
            b"NPC_" | b"CREA" => BaseData::Actor(ActorBase {
                entries: entries(lo, pi, id, &d, issues),
                attributes: if &sig == b"NPC_" { attributes(&d) } else { None },
            }),
            b"DOOR" => {
                let f = first(&d, b"FNAM");
                BaseData::Door(Door {
                    automatic: f.is_some_and(|f| has_flag(f, "Automatic Door")),
                    minimal_use: f.is_some_and(|f| has_flag(f, "Minimal Use")),
                })
            }
            // A terminal activates like an activator; what it shows (its menu, the
            // hacking game) is the engine's (docs/spec-notes/hacking.md §6).
            b"ACTI" | b"TACT" | b"TERM" => BaseData::Activator,
            b"FURN" => BaseData::Furniture,
            b"LVLI" => BaseData::LeveledItem(leveled(lo, pi, id, &d, issues)),
            b"PROJ" => BaseData::Projectile {
                item: None,
                can_be_disabled: first(&d, b"DATA").and_then(|v| v.get("flags")).is_none_or(|f| has_flag(f, "Can Be Disabled")),
            },
            _ => return Loaded::Skip,
        }
    };
    let prompt = if &sig == b"ACTI" { first(&d, b"XATO").and_then(text).filter(|t| !t.trim().is_empty()) } else { None };
    let voice_filter = &sig == b"TACT" && r.header.flags & NO_VOICE_FILTER == 0;
    Loaded::Base(Box::new(BaseForm { id, sig, edid, name, script, prompt, voice_filter, data }))
}

/// Whether the record's header carries the flag the schema names "Quest Item" for its
/// type (false when the schema names no such bit for the type).
fn quest_flag(schema: &Schema, r: &Record) -> bool {
    schema.header_flag(&r.header.sig, QUEST_ITEM_FLAG).is_some_and(|m| r.header.flags & m != 0)
}

/// Item types whose records have both a value and a weight field [documented: xEdit].
fn kind_has_value_weight(kind: ItemKind) -> bool {
    !matches!(kind, ItemKind::Note | ItemKind::CasinoChip | ItemKind::CaravanMoney | ItemKind::CaravanCard)
}

fn item(kind: ItemKind, d: &[Decoded]) -> Option<Item> {
    let data = first(d, b"DATA");
    let (value, weight) = match kind {
        ItemKind::Weapon | ItemKind::Armor | ItemKind::Book | ItemKind::Key | ItemKind::Misc => {
            (data.and_then(|v| int(v, "value")), data.and_then(|v| num(v, "weight")))
        }
        ItemKind::Ammo => (data.and_then(|v| int(v, "value")), first(d, b"DAT2").and_then(|v| num(v, "weight"))),
        ItemKind::Aid => (first(d, b"ENIT").and_then(|v| int(v, "value")), data.and_then(Value::as_f64)),
        ItemKind::Light => {
            let data = data?;
            if !data.get("flags").is_some_and(|f| has_flag(f, "Can be Carried")) {
                return None;
            }
            (int(data, "value"), num(data, "weight"))
        }
        ItemKind::ItemMod => (data.and_then(|v| int(v, "value")), data.and_then(|v| num(v, "weight"))),
        // CCRD DATA is the card's value alone [documented: xEdit].
        ItemKind::CaravanCard => (data.and_then(Value::as_i64), None),
        // NOTE, CHIP and CMNY have no value or weight field [documented: xEdit]; CMNY
        // DATA "Absolute Value" is not the item's value [hypothesis: the denomination].
        _ => (None, None),
    };
    let health = match kind {
        ItemKind::Weapon | ItemKind::Armor => data.and_then(|v| int(v, "health")).map(|h| h as i32),
        _ => None,
    };
    let projectile = match kind {
        ItemKind::Weapon => first(d, b"DNAM").and_then(|v| v.get("projectile")).and_then(local_fid),
        _ => None,
    };
    let slots = match kind {
        ItemKind::Armor => first(d, b"BMDT").and_then(|v| v.get("biped_flags")).and_then(|f| int(f, "value")),
        _ => None,
    };
    let weapon_flags = matches!(kind, ItemKind::Weapon).then(|| first(d, b"DNAM").and_then(|v| v.get("flags1"))).flatten();
    let non_playable = match kind {
        ItemKind::Armor => first(d, b"BMDT").and_then(|v| v.get("general_flags")).is_some_and(|f| has_flag(f, "Non-Playable")),
        ItemKind::Weapon => weapon_flags.is_some_and(|f| has_flag(f, "Non-Playable")),
        ItemKind::Ammo => data.and_then(|v| v.get("flags")).is_some_and(|f| has_flag(f, "Non-Playable")),
        _ => false,
    };
    let general = matches!(kind, ItemKind::Armor).then(|| first(d, b"BMDT").and_then(|v| v.get("general_flags"))).flatten();
    let armor_class = match general {
        Some(f) if has_flag(f, "Heavy") => ArmorClass::Heavy,
        Some(f) if has_flag(f, "Medium") => ArmorClass::Medium,
        _ => ArmorClass::Light,
    };
    Some(Item {
        kind,
        non_playable,
        cant_drop: weapon_flags.is_some_and(|f| has_flag(f, "Can't Drop")),
        armor_class,
        power_armor: general.is_some_and(|f| has_flag(f, "Power Armor")),
        armor_dr: match kind {
            ItemKind::Armor => first(d, b"DNAM").and_then(|v| int(v, "dr")).map(|dr| dr as f32 / 100.0),
            _ => None,
        },
        armor_dt: match kind {
            ItemKind::Armor => Some(first(d, b"DNAM").and_then(|v| num(v, "dt")).unwrap_or(0.0) as f32),
            _ => None,
        },
        // The explosion is added by the caller, which can read the projectile.
        weapon_damage: match kind {
            ItemKind::Weapon => Some(data.and_then(|v| int(v, "base_damage")).unwrap_or(0) as f32),
            _ => None,
        },
        weight_mods: match (kind, first(d, b"DNAM")) {
            (ItemKind::Weapon, Some(dn)) => [1, 2, 3].map(|slot| {
                let decrease = dn.get(format!("effect_mod{slot}")).and_then(Value::as_str) == Some("Decrease Weight");
                if decrease {
                    num(dn, &format!("value_a_mod{slot}")).unwrap_or(0.0) as f32
                } else {
                    0.0
                }
            }),
            _ => [0.0; 3],
        },
        drawable: kind == ItemKind::Weapon && {
            let dnam = first(d, b"DNAM");
            first(d, b"MODL").and_then(text).is_some_and(|m| !m.trim().is_empty())
                && !dnam.and_then(|v| v.get("flags1")).is_some_and(|f| has_flag(f, "Embedded Weapon"))
                && dnam.and_then(|v| v.get("animation_type")).and_then(text).is_some()
        },
        not_for_npcs: kind == ItemKind::Weapon
            && first(d, b"DNAM").and_then(|v| v.get("flags2")).is_some_and(|f| has_flag(f, "Player Only") || has_flag(f, "Not Used In Normal Combat")),
        thrown: kind == ItemKind::Weapon
            && first(d, b"DNAM").and_then(|v| v.get("animation_type")).and_then(text).is_some_and(|a| THROWN_ANIM_TYPES.contains(&a.as_str())),
        quest_item: false, // from the record header, set by the caller
        value: value.map(|v| v as i32),
        money: match kind {
            ItemKind::CaravanMoney => data.and_then(Value::as_i64).map(|v| v as i32),
            _ => None,
        },
        weight: weight.map(|w| w as f32).filter(|w| w.is_finite()),
        slots: slots.map(|s| s as u32),
        // File-local here; made global by the caller.
        projectile: projectile.map(FormId),
        health,
        icon: first(d, b"ICON").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        placed_count: match kind {
            ItemKind::Ammo => data.and_then(|v| int(v, "clip_rounds")).map(|n| n as i32).filter(|&n| n > 0),
            _ => None,
        },
        teaches: match kind {
            ItemKind::Book => data.and_then(|v| v.get("skill")).and_then(skill_code),
            _ => None,
        },
        cant_be_taken: kind == ItemKind::Book && data.and_then(|v| v.get("flags")).is_some_and(|f| has_flag(f, "Can't be Taken")),
        modulates_voice: kind == ItemKind::Armor && first(d, b"DNAM").and_then(|v| v.get("flags")).is_some_and(|f| has_flag(f, "Modulates Voice")),
        // NAM0 file-local here; made global by the caller.
        weapon_traits: (kind == ItemKind::Weapon).then(|| {
            let dn = first(d, b"DNAM");
            let f = |k: &str| dn.and_then(|v| num(v, k)).unwrap_or(0.0) as f32;
            WeaponTraits {
                skill: dn.and_then(|v| v.get("skill")).and_then(skill_code),
                automatic: weapon_flags.is_some_and(|x| has_flag(x, "Is Automatic")),
                fire_rate: f("fire_rate"),
                shots_per_sec: f("attack_shots_per_sec"),
                attack_mult: f("attack_mult"),
                ammo: first(d, b"NAM0").and_then(local_fid).map(FormId),
                max_range: f("max_range"),
                range_fixed: dn.and_then(|v| v.get("flags2")).is_some_and(|x| has_flag(x, "Range - Fixed")),
                // File-local here; made global by the caller.
                mods: [b"WMI1", b"WMI2", b"WMI3"].map(|sig| first(d, sig).and_then(local_fid).map(FormId)),
                ..Default::default()
            }
        }),
    })
}

/// The skills of BOOK DATA "Skill" in the record's order, 0 Barter … 13 Unarmed
/// [documented: xEdit wbSkillEnum, `schemas/fnv/common.json` Skill].
const BOOK_SKILLS: [&str; 14] = [
    "Barter", "Big Guns", "Energy Weapons", "Explosives", "Lockpick", "Medicine", "Melee Weapons", "Repair", "Science", "Guns", "Sneak", "Speech", "Survival", "Unarmed",
];

/// The actor value code of a decoded skill (its name, or its number when the schema
/// names none): the skills' codes run from Barter 32 in the same order [documented:
/// xEdit ActorValue enum]. `None` for "None" (-1).
fn skill_code(v: &Value) -> Option<u16> {
    let i = match v {
        Value::String(s) => BOOK_SKILLS.iter().position(|n| n == s)? as i64,
        v => v.as_i64()?,
    };
    (0..14).contains(&i).then(|| 32 + i as u16)
}

/// Animation types of weapons that are thrown or laid rather than held and fired
/// [documented: xEdit WEAP DNAM "Animation Type"; `schemas/fnv`].
pub const THROWN_ANIM_TYPES: [&str; 4] = ["Grenade Throw (1 Hand)", "Thrown (1 Hand)", "Land Mine (1 Hand)", "Mine Drop (1 Hand)"];

/// The damage an actor's weapon choice ranks a weapon by (docs/spec-notes/actors.md §9.3):
/// its base damage, plus the damage of the explosion its projectile sets off for a weapon
/// that is not thrown or laid and whose skill (WEAP DNAM "Skill") is not Explosives
/// [hypothesis: fitted to Cook-Cook drawing his Incinerator (base 1, its glob's explosion
/// 20; Energy Weapons) and never his Flamer (16) in three runs of the original,
/// npc-behaviour-compare.md §9.1, and to five Goodsprings residents none of whom took a
/// Grenade Rifle (2, Explosives) with its grenades over a Hunting Rifle, .357 or Varmint
/// Rifle, observer check 1, o13_reequip; thrown weapons stay at their base damage].
pub fn weapon_choice_damage(base: f32, explosion: f32, anim_type: Option<&str>, skill: Option<&str>) -> f32 {
    let thrown = anim_type.is_some_and(|a| THROWN_ANIM_TYPES.contains(&a));
    base + if thrown || skill == Some(EXPLOSIVES_SKILL) { 0.0 } else { explosion }
}

/// WEAP DNAM "Skill" of grenade rifles, launchers and other explosive weapons, as the
/// schema names it [documented: xEdit WEAP, the ActorValue enum].
pub const EXPLOSIVES_SKILL: &str = "Explosives";

/// The damage of the explosion projectile `proj` (a global FormID) sets off: PROJ DATA
/// flag "Explosion" and "Explosion" → EXPL DATA "Damage" [documented: xEdit PROJ, EXPL;
/// `schemas/fnv`]. 0 when it has none or a record does not decode.
pub fn explosion_damage(lo: &LoadOrder, schema: &Schema, proj: FormId) -> f32 {
    projectile_facts(lo, schema, proj).explosion_damage
}

/// What the weapon choices read of a projectile (`proj`, a global FormID).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProjectileFacts {
    /// [`explosion_damage`].
    pub explosion_damage: f32,
    /// The explosion's EXPL DATA "Radius"; 0 without one.
    pub blast: f32,
    /// PROJ DATA "Range" (0: none).
    pub range: f32,
    /// PROJ DATA "Type" Flame.
    pub flame: bool,
}

/// [`ProjectileFacts`] of `proj` [documented: xEdit PROJ, EXPL; `schemas/fnv`]; all 0 when
/// a record does not decode.
pub fn projectile_facts(lo: &LoadOrder, schema: &Schema, proj: FormId) -> ProjectileFacts {
    let data_of = |id: FormId, sig: &[u8; 4]| {
        let d = lo.index.get(&id.0).and_then(|defs| defs.last())?;
        let r = lo.records[d.plugin as usize][d.ordinal as usize];
        if &r.header.sig != sig || r.header.flags & record_flags::DELETED != 0 {
            return None;
        }
        let dec = extract::decode(r, schema).ok()?;
        Some((d.plugin as usize, first(&dec, b"DATA").cloned()?))
    };
    let Some((pi, data)) = data_of(proj, b"PROJ") else { return ProjectileFacts::default() };
    let mut out = ProjectileFacts {
        range: num(&data, "range").unwrap_or(0.0).max(0.0) as f32,
        flame: data.get("type").is_some_and(|t| text(t).as_deref() == Some("Flame") || t.as_i64() == Some(8)),
        ..Default::default()
    };
    if !data.get("flags").is_some_and(|f| has_flag(f, "Explosion")) {
        return out;
    }
    let Some(Ok(Some(expl))) = data.get("explosion").and_then(local_fid).map(|l| global(lo, pi, l)) else { return out };
    if let Some((_, e)) = data_of(expl, b"EXPL") {
        out.explosion_damage = num(&e, "damage").unwrap_or(0.0).max(0.0) as f32;
        out.blast = num(&e, "radius").unwrap_or(0.0).max(0.0) as f32;
    }
    out
}

/// S.P.E.C.I.A.L. names in NPC_ DATA order.
pub const SPECIAL: [&str; 7] = ["strength", "perception", "endurance", "charisma", "intelligence", "agility", "luck"];

fn attributes(d: &[Decoded]) -> Option<[u8; 7]> {
    let a = first(d, b"DATA")?.get("attributes")?;
    let mut out = [0u8; 7];
    for (o, name) in out.iter_mut().zip(SPECIAL) {
        *o = int(a, name)? as u8;
    }
    Some(out)
}

fn entries(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> Vec<Entry> {
    // COED belongs to the CNTO right before it [documented: xEdit wbCNTO/wbCOED
    // struct; observed: all 1,661 COED in the masters directly follow a CNTO].
    let mut out = Vec::new();
    let mut last_kept = false;
    for x in d {
        let Some(v) = x.value.as_ref() else {
            if &x.sig == b"CNTO" || &x.sig == b"COED" {
                issues.note("CNTO/COED undecodable", || format!("{id}"));
            }
            last_kept = false;
            continue;
        };
        match &x.sig {
            b"CNTO" => {
                last_kept = false;
                let (Some(local), Some(count)) = (v.get("item").and_then(local_fid), int(v, "count")) else {
                    issues.note("CNTO without item/count", || format!("{id}"));
                    continue;
                };
                match global(lo, pi, local) {
                    Ok(Some(item)) => {
                        out.push(Entry::new(item, count as i32));
                        last_kept = true;
                    }
                    Ok(None) => issues.note("CNTO with null item", || format!("{id}")),
                    Err(l) => issues.note("FormID outside master list", || format!("{id} CNTO 0x{l:08X}")),
                }
            }
            b"COED" if last_kept => {
                let e = &mut out.last_mut().expect("last_kept").extra;
                e.condition = num(v, "item_condition").map(|c| c as f32);
                e.owner = match v.get("owner").and_then(local_fid).map(|l| global(lo, pi, l)) {
                    Some(Ok(g)) => g,
                    Some(Err(l)) => {
                        issues.note("FormID outside master list", || format!("{id} COED 0x{l:08X}"));
                        None
                    }
                    None => None,
                };
            }
            b"COED" => issues.note("COED not after a kept CNTO", || format!("{id}")),
            _ => last_kept = false,
        }
    }
    out
}

fn leveled(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> LeveledList {
    let flags = first(d, b"LVLF");
    let flag = |n: &str| flags.is_some_and(|f| has_flag(f, n));
    let mut out = LeveledList {
        chance_none: first(d, b"LVLD").and_then(Value::as_u64).unwrap_or(0).min(255) as u8,
        all_levels: flag("Calculate from all levels <= player's level"),
        each: flag("Calculate for each item in count"),
        use_all: flag("Use All"),
        global: None,
        entries: Vec::new(),
    };
    if let Some(l) = first(d, b"LVLG").and_then(local_fid) {
        match global(lo, pi, l) {
            Ok(g) => out.global = g,
            Err(l) => issues.note("FormID outside master list", || format!("{id} LVLG 0x{l:08X}")),
        }
    }
    // A COED right after an LVLO gives that entry's items their condition (the CondNV…
    // lists: CondNVServiceRiflelLoot rolls a Service Rifle at 30 to 60 %) [documented:
    // xEdit's leveled entry is LVLO plus an optional COED; GECK wiki Container, "Health
    // %: the condition of the object"; observed: the original's NCR troopers drew those
    // rifles at 30 to 50 %, npc-behaviour-compare.md §9]. Its owner is not kept.
    let mut last_kept = false;
    for x in d {
        let v = match (&x.sig, x.value.as_ref()) {
            (b"COED", Some(c)) if last_kept => {
                let e = out.entries.last_mut().expect("last_kept");
                e.condition = num(c, "item_condition").map(|c| c as f32);
                continue;
            }
            (b"LVLO", Some(v)) => v,
            _ => {
                last_kept = false;
                continue;
            }
        };
        last_kept = false;
        let (Some(level), Some(local)) = (int(v, "level"), v.get("reference").and_then(local_fid)) else {
            issues.note("LVLO without level/reference", || format!("{id}"));
            continue;
        };
        // The count field is optional in the layout; a missing count means one.
        // [hypothesis: xEdit marks it optional (SetOptionalFrom(3)); every LVLO in the
        // shipped masters is checked by `fnvtool sim forms`]
        let count = match int(v, "count") {
            Some(c) => c as u16,
            None => {
                issues.note("LVLO without count (defaulted to 1)", || format!("{id}"));
                1
            }
        };
        match global(lo, pi, local) {
            Ok(Some(item)) => {
                out.entries.push(LeveledEntry { level: level as u16, item, count, condition: None });
                last_kept = true;
            }
            Ok(None) => issues.note("LVLO with null reference", || format!("{id}")),
            Err(l) => issues.note("FormID outside master list", || format!("{id} LVLO 0x{l:08X}")),
        }
    }
    out
}
