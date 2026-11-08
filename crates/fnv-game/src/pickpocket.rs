//! Pickpocketing (docs/spec-notes/pickpocket.md): who may be pickpocketed, the chance
//! of each take or placement, being caught, and live explosives planted in an actor's
//! inventory ("exploding pants").
//!
//! - **Who** ([`Game::pickpocket_refusal`]): a living, present actor other than the
//!   player that is not hostile to the player and not a teammate; every NPC ("can all be
//!   pickpocketed" [documented: GECK wiki Category:NPC]) and the creatures flagged Allow
//!   Pickpocket (CREA ACBS bit 28 [documented: GECK wiki ActorHasBaseFlag]). Not a ghost:
//!   "Cannot be pickpocketed … Instead, regular activation (dialog) occurs" [documented:
//!   GECK wiki SetGhost notes]. Not one that has caught the player before: the game's
//!   `sNoPickPocketAgain` "has already caught you." [observed: FalloutNV.esm].
//! - **Chance** ([`Rules::chance`]): `(thief Sneak × fPickPocketActorSkillMult +
//!   fPickPocketActorSkillBase) + (target Sneak × fPickPocketTargetSkillMult +
//!   fPickPocketTargetSkillBase) + (item value × fPickPocketAmountMult +
//!   fPickPocketAmountBase)`, rounded down, kept within fPickPocketMinChance and
//!   fPickPocketMaxChance [documented: GECK wiki Pickpocket].
//! - **Caught** ([`Game::pickpocket_caught`]): the victim says Pickpocket ("Said by an
//!   actor if they notice you picking their pocket" [documented: GECK wiki Combat Tab])
//!   and turns on the player as for a seen theft; it will not be pickpocketed again.
//! - **Planted explosives** ([`Game::plant_explosive`]): a grenade or mine placed in an
//!   actor's inventory goes live; GetPlantedExplosive is 1 after
//!   `fProjectileInventoryGrenadeFreakoutTime` (the victim panics: `Game::panic_idle`
//!   in `crate::idles`), and after `fProjectileInventoryGrenadeTimer` it explodes,
//!   "always killing it regardless of its HP" [documented: GECK wiki Projectile notes],
//!   when its explosion hurts that kind of victim ([`Game::explosive_harms`]). Not on a
//!   child or an essential actor (it stays an ordinary item). Judged as a kill seen by
//!   whoever detected the player while it was live, credited to the weapon the player
//!   holds (docs/spec-notes/pickpocket.md §5).

use crate::crime::{Kind, Note, Ownership};
use crate::functions::{actor, base_of};
use crate::game::Game;
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};

/// The pickpocket settings, with the wiki's defaults (FalloutNV.esm sets all but the two
/// bases and the bounds to these values [observed]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rules {
    pub actor_skill_mult: f64,
    pub actor_skill_base: f64,
    pub target_skill_mult: f64,
    pub target_skill_base: f64,
    pub amount_mult: f64,
    pub amount_base: f64,
    pub min_chance: f64,
    pub max_chance: f64,
    /// `fProjectileInventoryGrenadeTimer`, `fProjectileInventoryGrenadeFreakoutTime`
    /// (seconds) [observed: FalloutNV.esm 2.0 and 0.1].
    pub grenade_timer: f64,
    pub freakout_time: f64,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules {
            actor_skill_mult: 0.6,
            actor_skill_base: 40.0,
            target_skill_mult: -0.6,
            target_skill_base: 0.0,
            amount_mult: -0.5,
            amount_base: 0.0,
            min_chance: 5.0,
            max_chance: 85.0,
            grenade_timer: 2.0,
            freakout_time: 0.1,
        }
    }
}

impl Rules {
    pub fn read(g: &Game) -> Rules {
        let d = Rules::default();
        let s = |name: &str, v: f64| crate::combat::setting(g, name, v);
        Rules {
            actor_skill_mult: s("fPickPocketActorSkillMult", d.actor_skill_mult),
            actor_skill_base: s("fPickPocketActorSkillBase", d.actor_skill_base),
            target_skill_mult: s("fPickPocketTargetSkillMult", d.target_skill_mult),
            target_skill_base: s("fPickPocketTargetSkillBase", d.target_skill_base),
            amount_mult: s("fPickPocketAmountMult", d.amount_mult),
            amount_base: s("fPickPocketAmountBase", d.amount_base),
            min_chance: s("fPickPocketMinChance", d.min_chance),
            max_chance: s("fPickPocketMaxChance", d.max_chance),
            grenade_timer: s("fProjectileInventoryGrenadeTimer", d.grenade_timer),
            freakout_time: s("fProjectileInventoryGrenadeFreakoutTime", d.freakout_time),
        }
    }

    /// The chance in percent of taking or placing items worth `value` caps (module notes).
    pub fn chance(&self, thief_sneak: f64, target_sneak: f64, value: f64) -> i32 {
        let c = (thief_sneak * self.actor_skill_mult + self.actor_skill_base)
            + (target_sneak * self.target_skill_mult + self.target_skill_base)
            + (value * self.amount_mult + self.amount_base);
        let c = c.floor();
        let (lo, hi) = (self.min_chance.min(self.max_chance), self.max_chance.max(self.min_chance));
        c.clamp(lo, hi) as i32
    }
}

/// Why the player may not pickpocket an actor now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Dead, absent, the player, hostile, a teammate, a ghost or a creature without
    /// Allow Pickpocket: the actor is talked to (or searched) as usual.
    NotAllowed,
    /// It caught the player before: `sNoPickPocketAgain`.
    CaughtBefore,
}

/// What the engine knows of a planted explosive's blast: its explosion's damage (EXPL
/// Damage), the weapon's damage type (WEAP DNAM Resist, as an actor value code; `None`
/// for None and Damage Resistance) and the explosion's object effect (EXPL EITM).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Explosive {
    pub damage: f64,
    pub resist: Option<u16>,
    pub enchantment: Option<FormId>,
}

/// A live explosive placed in an actor's inventory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Planted {
    pub victim: FormId,
    pub item: FormId,
    /// Game time (seconds) it was placed.
    pub at: f64,
    pub blast: Explosive,
    /// Who detected the player while it was live (crime witnesses, taken at each tick).
    pub seen_by: Vec<FormId>,
}

/// What [`Game::tick_planted`] found due: the victim and the explosive item, for the
/// engine to set off its explosion where the victim stands, and whether it killed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blast {
    pub victim: FormId,
    pub item: FormId,
    pub killed: bool,
}

/// Why a placed explosive did not go live ([`Game::plant_explosive`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotLive {
    /// The victim's race has the Child flag.
    Child,
    /// The victim is essential.
    Essential,
}

/// The pickpocket state the game keeps for the session (who caught the player is the
/// saved `RefExtra::caught_pickpocket`).
#[derive(Debug, Clone, Default)]
pub struct State {
    pub rules: Option<Rules>,
    pub planted: Vec<Planted>,
    /// While a planted explosive's kill is judged: those who saw the player while it was
    /// live, witnesses besides those who see the player now (`Game::crime_witnesses`).
    pub blast_witnesses: Vec<FormId>,
    /// ShowOff's SetPlayerCanPickpocketEquippedItems: the player may take equipped items
    /// too (none from a child) [documented: Get/SetPlayerCanPickpocketEquippedItems].
    pub worn_items: bool,
}

impl Game<'_> {
    fn pickpocket_rules(&mut self) -> Rules {
        if let Some(r) = self.pickpocket.rules {
            return r;
        }
        let r = Rules::read(self);
        self.pickpocket.rules = Some(r);
        r
    }

    /// Whether the player may pickpocket `r` now (module notes); `None`: it may.
    pub fn pickpocket_refusal(&mut self, r: FormId) -> Option<Refusal> {
        if r == FormId::PLAYER_REF || self.is_dead(r) || !self.present(r) || self.is_ghost(r) {
            return Some(Refusal::NotAllowed);
        }
        if self.extra.get(&r).is_some_and(|e| e.teammate || e.out_cold()) {
            return Some(Refusal::NotAllowed);
        }
        let creature = base_of(self, r).and_then(|b| self.data.actors.get(&b)).is_some_and(|d| &d.sig == b"CREA");
        if creature && !self.base_data_def(r).is_some_and(|d| d.allow_pickpocket) {
            return Some(Refusal::NotAllowed);
        }
        let fighting_player = self.awareness.get(r).is_some_and(|a| a.state.in_combat() && a.targets.contains_key(&FormId::PLAYER_REF));
        if fighting_player || self.would_attack(r, FormId::PLAYER_REF) {
            return Some(Refusal::NotAllowed);
        }
        if self.extra.get(&r).is_some_and(|e| e.caught_pickpocket) {
            return Some(Refusal::CaughtBefore);
        }
        None
    }

    /// The player's and the victim's Sneak, for [`Rules::chance`]. A creature's Sneak is
    /// its Stealth skill (`actor_values`).
    pub fn pickpocket_skills(&mut self, victim: FormId) -> (f64, f64) {
        let thief = actor::actor_value_capped(self, FormId::PLAYER_REF, crate::actor_values::SNEAK);
        let target = actor::actor_value_capped(self, victim, crate::actor_values::SNEAK);
        (thief, target)
    }

    /// The chance of taking or placing items worth `value` from or on `victim`.
    pub fn pickpocket_chance(&mut self, victim: FormId, value: f64) -> i32 {
        let rules = self.pickpocket_rules();
        let (thief, target) = self.pickpocket_skills(victim);
        rules.chance(thief, target, value)
    }

    /// Whether the pickpocket menu lists `victim`'s equipped items: SetPlayerCanPickpocket-
    /// EquippedItems 1, and the victim is no child ("Child actors still cannot have their
    /// equipped items stolen" [documented: SetPlayerCanPickpocketEquippedItems]).
    pub fn pickpocket_worn(&self, victim: FormId) -> bool {
        self.pickpocket.worn_items && !self.actor_race(victim).is_some_and(|race| self.data.child_races.contains(&race))
    }

    /// The rules as read from the load order.
    pub fn pickpocket_settings(&mut self) -> Rules {
        self.pickpocket_rules()
    }

    /// The player took `count` of `item` from `victim` unseen: theft from its owner (the
    /// victim's base), so karma as for any theft (`crime.md` §3.1: an unseen theft costs
    /// karma too), and one more Pockets Picked [hypothesis: one per successful take].
    pub fn pickpocket_took(&mut self, victim: FormId, item: FormId, count: i32) {
        if let Some(owner) = base_of(self, victim) {
            let o = Ownership { owner, rank: None, from_cell: false };
            let s = self.crime_settings();
            if self.karma_for_theft_from(&o) {
                let name = self.data.name(item);
                self.change_karma(s.stealing, format!("pickpocketing {name} x{count}"));
            }
        }
        self.count_stat(crate::misc_stats::POCKETS_PICKED, 1.0);
    }

    /// The victim noticed the player's hand in its pocket: it says Pickpocket, turns on
    /// the player as a seen thief does (unless it ignores crime), and will not be
    /// pickpocketed again. Returns whether it turned.
    pub fn pickpocket_caught(&mut self, victim: FormId) -> bool {
        self.extra.entry(victim).or_default().caught_pickpocket = true;
        let ignores = self.extra.get(&victim).is_some_and(|e| e.ignore_crime);
        if ignores {
            // "The same as Pickpocket, but the actor doesn't regard it as a crime"
            // [documented: GECK wiki Combat Tab, PickpocketNoCrime].
            self.detection_bark(victim, Some(FormId::PLAYER_REF), "PickpocketNoCrime");
            return false;
        }
        let turned = self.crime_turn(victim, Kind::Pickpocket);
        let owner = base_of(self, victim);
        self.crimes.notes.push(Note::Crime { kind: Kind::Pickpocket, victim: owner, seen_by: vec![victim], hostile: if turned { vec![victim] } else { Vec::new() } });
        turned
    }

    /// The player placed `item` in `victim`'s pocket, a live explosive (the engine knows
    /// the weapons' projectiles; `blast` its explosion): it goes live, one per call. Not
    /// on a child or an essential actor: it stays an ordinary item there [documented:
    /// Fallout 3 puts the explosive in a child's inventory with no explosion (Fallout
    /// Wiki, Fallout 3 Pickpocket); hypothesis: the same in New Vegas, and for essential
    /// actors (observer-checks.md)].
    pub fn plant_explosive(&mut self, victim: FormId, item: FormId, blast: Explosive) -> Result<(), NotLive> {
        if self.actor_race(victim).is_some_and(|race| self.data.child_races.contains(&race)) {
            return Err(NotLive::Child);
        }
        if actor::essential(self, victim) {
            return Err(NotLive::Essential);
        }
        let at = self.time;
        self.pickpocket.planted.push(Planted { victim, item, at, blast, seen_by: Vec::new() });
        Ok(())
    }

    /// GetPlantedExplosive: a live explosive was planted on `r` at least
    /// `fProjectileInventoryGrenadeFreakoutTime` ago and has not gone off.
    pub fn has_planted_explosive(&mut self, r: FormId) -> bool {
        let rules = self.pickpocket_rules();
        let now = self.time;
        self.pickpocket.planted.iter().any(|p| p.victim == r && now - p.at >= rules.freakout_time)
    }

    /// Whether `blast` hurts `victim`'s kind, so that it kills it when it goes off in its
    /// pocket ("instantly killing them if it's a damaging type", owner 2026-11-05)
    /// [hypothesis, observer-checks.md]: never an actor that takes no damage
    /// ([`Game::is_immune`]); when the explosion's object effect has effects that lower
    /// Health, those effects decide, by their conditions on the victim (the Pulse
    /// Grenade's EMP: 200 Health on robots, 80 on power armour wearers [observed:
    /// FalloutNV.esm ENCH 0x0003A36B]; the explosion's own 10 points do not count then);
    /// otherwise the explosion's damage above 0, of a type the victim does not wholly
    /// resist (its value of the weapon's resistance below 100).
    pub fn explosive_harms(&mut self, victim: FormId, blast: &Explosive) -> bool {
        if self.is_immune(victim) {
            return false;
        }
        let data = self.data;
        let health: Vec<&crate::data::EffectItem> = blast
            .enchantment
            .and_then(|e| data.effect_items.get(&e))
            .into_iter()
            .flatten()
            .filter(|ei| {
                let Some(m) = data.magic_effects.get(&ei.effect) else { return false };
                let harmful = m.has(crate::data::MagicEffectDef::DETRIMENTAL) || m.has(crate::data::MagicEffectDef::HOSTILE);
                harmful && ei.magnitude > 0 && ei.actor_value.or(m.actor_value) == Some(crate::actor_values::HEALTH)
            })
            .collect();
        if !health.is_empty() {
            return health.into_iter().any(|ei| ei.conditions.is_empty() || self.conditions_pass(&ei.conditions, Some(victim), Some(FormId::PLAYER_REF), "planted explosive effect"));
        }
        let resisted = blast.resist.is_some_and(|code| actor::actor_value(self, victim, code).is_ok_and(|v| v >= 100.0));
        blast.damage > 0.0 && !resisted
    }

    /// The planted explosives whose time is up: each victim loses the explosive, one more
    /// Pants Exploded, and the engine sets off the explosions. A victim its explosion
    /// hurts ([`Game::explosive_harms`]) dies, killed by the player with the weapon the
    /// player holds (the kill counts for that weapon's challenges [documented: Fallout
    /// Wiki, New Vegas Pickpocket]). The kill is judged with those who detected the
    /// player while the explosive was live as witnesses, besides those who see the player
    /// now: a player hidden from the plant to the blast is seen by nobody, so no one turns
    /// hostile and no infamy is gained [documented: same page; hypothesis: taking the
    /// witnesses at each tick]. What the victim's own OnDeath does (Caesar's, Lucius's and
    /// Crocker's set quest stages that make enemies or give infamy) happens whoever saw
    /// [observed: FalloutNV.esm]. A victim that died or left before is let off
    /// [hypothesis].
    pub fn tick_planted(&mut self) -> Vec<Blast> {
        if self.pickpocket.planted.is_empty() {
            return Vec::new();
        }
        let rules = self.pickpocket_rules();
        let now = self.time;
        let seeing = self.crime_witnesses();
        for p in &mut self.pickpocket.planted {
            for &w in &seeing {
                if !p.seen_by.contains(&w) {
                    p.seen_by.push(w);
                }
            }
        }
        let (due, keep): (Vec<Planted>, Vec<Planted>) = std::mem::take(&mut self.pickpocket.planted).into_iter().partition(|p| now - p.at >= rules.grenade_timer);
        self.pickpocket.planted = keep;
        let mut out = Vec::new();
        for p in due {
            if self.is_dead(p.victim) {
                continue;
            }
            let _ = self.world.remove_item(p.victim, p.item, 1);
            self.count_stat(crate::misc_stats::PANTS_EXPLODED, 1.0);
            let killed = self.explosive_harms(p.victim, &p.blast);
            if killed {
                let weapon = crate::functions::equipped_weapon_of(self, FormId::PLAYER_REF);
                self.extra.entry(p.victim).or_default().last_hit = Some(crate::game::LastHit { attacker: Some(FormId::PLAYER_REF), weapon, location: -1, critical: false });
                self.pickpocket.blast_witnesses = p.seen_by;
                self.kill(p.victim, Some(FormId::PLAYER_REF));
                self.pickpocket.blast_witnesses.clear();
            }
            out.push(Blast { victim: p.victim, item: p.item, killed });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chance_follows_the_wiki() {
        let r = Rules::default();
        // Sneak 50 against Sneak 20, an item worth 10: 30 + 40 - 12 - 5 = 53.
        assert_eq!(r.chance(50.0, 20.0, 10.0), 53);
        // Rounded down: 0.6 × 33 = 19.8.
        assert_eq!(r.chance(33.0, 0.0, 0.0), 59);
        // Within the bounds.
        assert_eq!(r.chance(100.0, 0.0, 0.0), 85);
        assert_eq!(r.chance(0.0, 100.0, 1000.0), 5);
    }
}
