//! Vendor services (docs/spec-notes/vendor-services.md): which item categories a vendor
//! sells, from its AI data's "Buys/Sells and Services" flags (or its class's, with
//! "Autocalc Service"), and whether it offers its services now (`GetOffersServicesNow`),
//! from its current package's "Offers Services" flag.
//!
//! - **The flags** [documented: xEdit wbServiceFlags, schema `gameplay.json`
//!   ServiceFlags; GECK wiki GetClassFlag, the same list two places on]: 0 Weapons, 1
//!   Armor, 2 Alcohol, 3 Books, 4 Food, 5 Chems, 6 Stimpaks, 10 Miscellaneous, 14
//!   Training, 16 Recharge, 17 Repair.
//! - **Whose flags**: the AI data's (NPC_/CREA AIDT), following the template while it
//!   supplies the AI data; an NPC with ACBS "Autocalc Service" uses its class's (CLAS
//!   DATA) [documented: GECK wiki Class, "If an NPC is set to auto-calc ... the buys/sells
//!   flags from their class are used. If the NPC is not Auto-calc'd, you can set their
//!   buys and sells flags directly"; hypothesis: the auto-calc meant is the "Autocalc
//!   Service" flag, the class is the one the traits give and the flag follows the AI
//!   data's template].
//! - **What they limit**: what the vendor sells. "Buys is not used in Fallout 3 or New
//!   Vegas and vendors can buy any item" [documented: GECK wiki Class].
//! - **An item's category** [`item_service`]: WEAP Weapons, ARMO Armor, BOOK Books
//!   [documented: GECK wiki Class, "Books: Can sell Skill Books"]; an ingestible by its
//!   equip type ETYP, 10 Chems, 11 Stimpak, 12 Food, 13 Alcohol [hypothesis: the wiki
//!   reads the ENIT flags, "Food: Ingestibles marked as food", "Chems: ... unmarked",
//!   "Stimpacks: ... marked as Medicine", and calls Alcohol unused; the equip types name
//!   the same four groups and fit the masters' vendors better: Nuka-Cola and Sunset
//!   Sarsaparilla (unmarked, ETYP Food) sit in the "FoodDrinkNoLiquor" lists that
//!   food-and-drink vendors such as Trudy (Alcohol, Food) stock, and the High Times
//!   dealer sells Alcohol and Chems; observed in FalloutNV.esm], falling back to the
//!   wiki's flags when the equip type is none of the four; AMMO and IMOD Weapons
//!   [hypothesis: Mick (Weapons, Armor, Repair) stocks bare boxes of ammunition and no
//!   Misc, observed]; everything else (MISC, KEYM, NOTE, LIGH, CHIP, CCRD, CMNY)
//!   Miscellaneous [hypothesis for all but MISC].
//! - **Offers services now** [`Game::offers_services_now`]: the actor's current package
//!   has "Offers Services" ("If the actor is a service provider, he will offer services
//!   so long as this package is running" [documented: GECK wiki Category:Packages]); not
//!   while it talks with another actor, nor while it is on its way to the package's
//!   location [documented: GECK wiki GetOffersServicesNow, notes: "false if the npc gets
//!   into a conversation with another npc or while moving to a destination", a travel
//!   package with the flag is "in a temporary NOT OFFER SERVICE state while they are
//!   moving"; hypothesis: "on its way" is the AI layer's Travelling phase before the
//!   goal is met, so an actor the AI does not move counts as there]; not in combat, nor
//!   dead [hypothesis]. A conversation with the player does not stop it [hypothesis: the
//!   function is meant for vendor dialogue conditions].
//! - **Not gated by it**: `ShowBarterMenu` opens whatever the package [observed: Chet's
//!   two packages and Mick's three have no "Offers Services" flag, yet both are the
//!   game's vendors through dialogue; no condition or script of FalloutNV.esm calls
//!   GetOffersServicesNow].

use crate::data::{GameData, IngestibleDef};
use crate::game::Game;
use crate::package::flags;
use fnv_sim::FormId;

pub const WEAPONS: u32 = 1 << 0;
pub const ARMOR: u32 = 1 << 1;
pub const ALCOHOL: u32 = 1 << 2;
pub const BOOKS: u32 = 1 << 3;
pub const FOOD: u32 = 1 << 4;
pub const CHEMS: u32 = 1 << 5;
pub const STIMPAKS: u32 = 1 << 6;
pub const MISC: u32 = 1 << 10;
pub const TRAINING: u32 = 1 << 14;
pub const RECHARGE: u32 = 1 << 16;
pub const REPAIR: u32 = 1 << 17;

const NAMES: [(u32, &str); 11] = [
    (WEAPONS, "Weapons"),
    (ARMOR, "Armor"),
    (ALCOHOL, "Alcohol"),
    (BOOKS, "Books"),
    (FOOD, "Food"),
    (CHEMS, "Chems"),
    (STIMPAKS, "Stimpaks"),
    (MISC, "Miscellaneous"),
    (TRAINING, "Training"),
    (RECHARGE, "Recharge"),
    (REPAIR, "Repair"),
];

/// The names of the flags set in `mask`, in flag order.
pub fn names(mask: u32) -> Vec<&'static str> {
    NAMES.iter().filter(|(b, _)| mask & b != 0).map(|(_, n)| *n).collect()
}

/// The name of one category flag.
pub fn name(flag: u32) -> &'static str {
    NAMES.iter().find(|(b, _)| *b == flag).map_or("?", |(_, n)| n)
}

/// The service category an item is sold under (one flag), `None` for what is no item
/// (module notes).
pub fn item_service(data: &GameData, item: FormId) -> Option<u32> {
    Some(match &data.sig(item)? {
        b"WEAP" | b"AMMO" | b"IMOD" => WEAPONS,
        b"ARMO" => ARMOR,
        b"BOOK" => BOOKS,
        b"ALCH" => ingestible_service(data.ingestibles.get(&item)),
        b"MISC" | b"KEYM" | b"NOTE" | b"LIGH" | b"CHIP" | b"CCRD" | b"CMNY" => MISC,
        _ => return None,
    })
}

fn ingestible_service(i: Option<&IngestibleDef>) -> u32 {
    let Some(i) = i else { return CHEMS };
    match i.equip_type {
        10 => CHEMS,
        11 => STIMPAKS,
        12 => FOOD,
        13 => ALCOHOL,
        _ if i.flags & IngestibleDef::MEDICINE != 0 => STIMPAKS,
        _ if i.flags & IngestibleDef::FOOD != 0 => FOOD,
        _ => CHEMS,
    }
}

impl Game<'_> {
    /// `actor`'s service flags (module notes): its class's with "Autocalc Service", else its
    /// AI data's as SetServiceFlag left them (`nvse_lutana`). 0 for what is no actor.
    pub fn services(&self, actor: FormId) -> u32 {
        let Some(b) = crate::functions::base_of(self, actor) else { return 0 };
        let Some((_, ai)) = self.template_source(b, actor, |d| d.template_ai) else { return 0 };
        if ai.autocalc_service {
            let class = self.traits_def(actor).and_then(|d| d.class);
            if let Some(s) = class.and_then(|c| self.data.class_services.get(&c)) {
                return *s;
            }
        }
        // SetServiceFlag changes the NPC record's flags (a "Use AI Data" template's are
        // read until then), which "Autocalc Service" overrides [observed: lutana1
        // actor.setservice.tender2].
        self.other.lutana.services.get(&b).copied().unwrap_or(ai.services)
    }

    /// Whether `vendor` sells `item`: the item's category is among its services. Caps and
    /// what is no item are always let through (the barter menu handles the caps itself).
    pub fn sells(&self, vendor: FormId, item: FormId) -> bool {
        self.refused_category(vendor, item).is_none()
    }

    /// The category `vendor` does not sell `item` under, when it does not.
    pub fn refused_category(&self, vendor: FormId, item: FormId) -> Option<u32> {
        let c = item_service(self.data, item)?;
        (self.services(vendor) & c == 0).then_some(c)
    }

    /// `GetOffersServicesNow` (module notes).
    pub fn offers_services_now(&mut self, actor: FormId) -> bool {
        if actor == FormId::PLAYER_REF || self.extra.get(&actor).is_some_and(|e| e.dead) {
            return false;
        }
        if self.awareness.state(actor).in_combat() {
            return false;
        }
        if self.speech.iter().any(|s| s.conversation.is_some() && (s.speaker == actor || s.target == Some(actor))) {
            return false;
        }
        let Some(p) = self.current_package(actor) else { return false };
        if !self.package_def(&p).is_some_and(|d| d.has(flags::OFFERS_SERVICES)) {
            return false;
        }
        let on_the_way = self.extra.get(&actor).is_some_and(|e| e.ai_phase == Some(crate::ai::AiPhase::Travelling) && !e.package_goal_met);
        !on_the_way
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingestibles_by_equip_type_then_flags() {
        let i = |equip_type, flags| IngestibleDef { equip_type, flags, ..IngestibleDef::default() };
        assert_eq!(ingestible_service(Some(&i(12, 0))), FOOD); // Nuka-Cola
        assert_eq!(ingestible_service(Some(&i(13, IngestibleDef::FOOD))), ALCOHOL); // Beer
        assert_eq!(ingestible_service(Some(&i(11, IngestibleDef::MEDICINE))), STIMPAKS);
        assert_eq!(ingestible_service(Some(&i(10, IngestibleDef::MEDICINE))), CHEMS); // RadAway
        assert_eq!(ingestible_service(Some(&i(-1, IngestibleDef::FOOD))), FOOD);
        assert_eq!(ingestible_service(Some(&i(0, IngestibleDef::MEDICINE))), STIMPAKS);
        assert_eq!(ingestible_service(Some(&i(-1, 0))), CHEMS);
    }

    #[test]
    fn flag_names() {
        assert_eq!(names(ALCOHOL | FOOD), ["Alcohol", "Food"]);
        assert_eq!(names(WEAPONS | MISC | REPAIR), ["Weapons", "Miscellaneous", "Repair"]);
        assert_eq!(name(ARMOR), "Armor");
    }
}
