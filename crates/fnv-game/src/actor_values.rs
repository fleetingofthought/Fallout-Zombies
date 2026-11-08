//! Actor values: what each of the 77 values starts at for an actor, before any script
//! or effect changes it (docs/spec-notes/actor-values.md).
//!
//! The base value comes from a field of the actor's record or from a formula over other
//! values and game settings [documented: GECK wiki ActorValue]:
//!
//! - **S.P.E.C.I.A.L.** (5–11): NPC_ and CREA DATA attributes.
//! - **Skills** (32–45):
//!   - NPC: NPC_ DNAM skill value + skill offset [documented: xEdit field names;
//!     hypothesis: the sum, see actor-values.md §3.2 for what the masters show];
//!   - creature: its Combat Skill for the five weapon skills [documented: GECK wiki
//!     Stats Tab - Creatures], its Stealth Skill for Sneak [hypothesis], the formula for
//!     the others [hypothesis];
//!   - player: the formula, plus `fAVDTagSkillBonus` for a tagged skill and the points
//!     spent on it in the level-up menu (`crate::levelup`);
//!   - the formula: `fAVDSkill<Name>Base` + attribute × `fAVDSkillPrimaryBonusMult` +
//!     ⌈Luck × `fAVDSkillLuckBonusMult`⌉ [documented: GECK wiki Derived Skill Settings].
//! - **Derived statistics**: Health (`crate::combat`), Action Points, Carry Weight,
//!   Critical Chance, Melee Damage, Unarmed Damage, Poison and Rad Resistance from their
//!   formulas ([`Game::base_actor_value`]); Speed Multiplier, Fatigue and Karma from ACBS.
//! - **AI attributes** (Aggression, Confidence, Energy, Responsibility, Mood,
//!   Assistance): AIDT.
//! - **Limb conditions** (25–31): 100. **Everything else**: 0.
//!
//! Formulas read the *current* value of the attribute or skill they use, kept within
//! its range (1–10, 0–100) [documented: GECK wiki GetThresholdedActorValue, the game
//! calculates with the capped value; hypothesis: that the current and not the base value
//! feeds them]. The value's pools (SetActorValue's override, the permanent modifier, the
//! damage) are `functions::actor`'s.

use crate::combat::setting;
use crate::data::ActorDef;
use crate::functions::{actor, base_of};
use crate::game::Game;
use fnv_sim::FormId;

pub const AGGRESSION: u16 = 0;
pub const CONFIDENCE: u16 = 1;
pub const ENERGY: u16 = 2;
pub const RESPONSIBILITY: u16 = 3;
pub const MOOD: u16 = 4;
pub const STRENGTH: u16 = 5;
pub const PERCEPTION: u16 = 6;
pub const ENDURANCE: u16 = 7;
pub const CHARISMA: u16 = 8;
pub const INTELLIGENCE: u16 = 9;
pub const AGILITY: u16 = 10;
pub const LUCK: u16 = 11;
pub const ACTION_POINTS: u16 = 12;
pub const CARRY_WEIGHT: u16 = 13;
pub const CRITICAL_CHANCE: u16 = 14;
pub const HEALTH: u16 = 16;
pub const MELEE_DAMAGE: u16 = 17;
pub const POISON_RESISTANCE: u16 = 19;
pub const RAD_RESISTANCE: u16 = 20;
pub const SPEED_MULTIPLIER: u16 = 21;
pub const FATIGUE: u16 = 22;
pub const KARMA: u16 = 23;
pub const BARTER: u16 = 32;
pub const BIG_GUNS: u16 = 33;
pub const ENERGY_WEAPONS: u16 = 34;
pub const EXPLOSIVES: u16 = 35;
pub const LOCKPICK: u16 = 36;
pub const MEDICINE: u16 = 37;
pub const MELEE_WEAPONS: u16 = 38;
pub const REPAIR: u16 = 39;
pub const SCIENCE: u16 = 40;
pub const GUNS: u16 = 41;
/// Experience points (the player's, `Game::player`).
pub const XP: u16 = 24;
pub const SNEAK: u16 = 42;
pub const SPEECH: u16 = 43;
pub const SURVIVAL: u16 = 44;
pub const UNARMED: u16 = 45;
pub const INVENTORY_WEIGHT: u16 = 46;
pub const UNARMED_DAMAGE: u16 = 56;
pub const ASSISTANCE: u16 = 57;

/// The S.P.E.C.I.A.L. codes, Strength to Luck.
pub const SPECIALS: std::ops::RangeInclusive<u16> = STRENGTH..=LUCK;
/// The skill codes, Barter to Unarmed.
pub const SKILLS: std::ops::RangeInclusive<u16> = BARTER..=UNARMED;
/// The limb-condition codes, Perception Condition to Brain Condition.
pub const LIMB_CONDITIONS: std::ops::RangeInclusive<u16> = 25..=31;

/// The skills' field names in NPC_ DNAM, in code order (schema `gameplay.json`).
pub const SKILL_FIELDS: [&str; 14] = [
    "barter", "big_guns", "energy_weapons", "explosives", "lockpick", "medicine", "melee_weapons", "repair",
    "science", "guns", "sneak", "speech", "survival", "unarmed",
];

/// Each skill's base setting, in code order [documented: GECK wiki Settings, Derived
/// Skill Settings; observed: the masters hold these fourteen, Guns under its Fallout 3
/// name and Survival's where that list has none, actor-values.md §3.1].
const SKILL_BASE_SETTINGS: [&str; 14] = [
    "fAVDSkillBarterBase",
    "fAVDSkillBigGunsBase",
    "fAVDSkillEnergyWeaponsBase",
    "fAVDSkillExplosivesBase",
    "fAVDSkillLockpickBase",
    "fAVDSkillMedicineBase",
    "fAVDSkillMeleeWeaponsBase",
    "fAVDSkillRepairBase",
    "fAVDSkillScienceBase",
    "fAVDSkillSmallGunsBase",
    "fAVDSkillSneakBase",
    "fAVDSkillSpeechBase",
    "fAVDSkillSurvivalBase",
    "fAVDSkillUnarmedBase",
];

/// The attribute behind each skill, in code order [observed: the one attribute for
/// which the skill formula gives the stored skill value of every auto-calculated NPC of
/// FalloutNV.esm whose class does not tag the skill (`fnvtool actor values --survey`,
/// actor-values.md §3.1). Hypothesis for Big Guns and Survival, where no attribute fits
/// them all: Endurance, the best fit for Big Guns and the game's own pairing for
/// Survival].
const GOVERNING: [u16; 14] = [
    CHARISMA,     // Barter
    ENDURANCE,    // Big Guns
    PERCEPTION,   // Energy Weapons
    PERCEPTION,   // Explosives
    PERCEPTION,   // Lockpick
    INTELLIGENCE, // Medicine
    STRENGTH,     // Melee Weapons
    INTELLIGENCE, // Repair
    INTELLIGENCE, // Science
    AGILITY,      // Guns
    AGILITY,      // Sneak
    CHARISMA,     // Speech
    ENDURANCE,    // Survival
    ENDURANCE,    // Unarmed
];

/// The S.P.E.C.I.A.L. attribute a skill is derived from.
pub fn governing_attribute(skill: u16) -> Option<u16> {
    SKILLS.contains(&skill).then(|| GOVERNING[(skill - BARTER) as usize])
}

/// The value kept within the range the game calculates with: S.P.E.C.I.A.L. 1–10, skills
/// 0–100, others as they are [documented: GECK wiki GetThresholdedActorValue].
pub fn capped(code: u16, v: f64) -> f64 {
    if SPECIALS.contains(&code) {
        v.clamp(1.0, 10.0)
    } else if SKILLS.contains(&code) {
        v.clamp(0.0, 100.0)
    } else {
        v
    }
}

/// The skills a creature's Combat Skill stands for [documented: GECK wiki Stats Tab -
/// Creatures, "Small Guns, Big Guns, Energy Weapons, Explosives, Melee Weapons"].
const CREATURE_COMBAT_SKILLS: [u16; 5] = [BIG_GUNS, ENERGY_WEAPONS, EXPLOSIVES, MELEE_WEAPONS, GUNS];

impl<'a> Game<'a> {
    /// The actor definition supplying one group of `r`'s data: its base, or the base's
    /// template while `from_template` says the template supplies that group, a leveled
    /// list on the way standing for one of its entries
    /// ([`crate::GameData::template_source`]) [documented: xEdit template flags].
    pub(crate) fn def_following(&self, r: FormId, from_template: fn(&ActorDef) -> bool) -> Option<&'a ActorDef> {
        self.template_source(base_of(self, r)?, r, from_template).map(|(_, d)| d)
    }

    /// The definition supplying `r`'s stats (template flag "Stats").
    pub(crate) fn stats_def(&self, r: FormId) -> Option<&'a ActorDef> {
        self.def_following(r, |d| d.template_stats)
    }

    /// A creature's type (CREA DATA type code: 0 Animal … 5 Feral Ghoul … 7 Giant),
    /// from the definition supplying its stats, as GetIsCreatureType reads it; `None`
    /// for NPCs.
    pub fn creature_type(&self, r: FormId) -> Option<i64> {
        self.stats_def(r).and_then(|d| d.creature_type)
    }

    /// A creature's CREA `TNAM` "Turning Speed" ("How quickly the creature rotates", on
    /// the Stats tab [documented: GECK wiki Stats Tab for Creatures]), from the definition
    /// supplying the stats of `r` (placed with `base`), with SetCreatureTurningSpeed's
    /// change; `None` for NPCs. `base` is given, as a reference a script made is not in
    /// the world outside `with_world`.
    pub fn creature_turning_speed(&self, base: FormId, r: FormId) -> Option<f32> {
        let (id, def) = self.template_source(base, r, |d| d.template_stats)?;
        if &def.sig != b"CREA" {
            return None;
        }
        crate::nvse::record_value(self, id, "TNAM", "").and_then(|v| v.num().ok()).map(|v| v as f32)
    }

    /// The definition supplying `r`'s traits: race, sex, voice type, class (template
    /// flag "Traits") [documented: GECK wiki Category:NPC, Template Data: each flag stands
    /// for a tab; the Traits tab holds these].
    pub(crate) fn traits_def(&self, r: FormId) -> Option<&'a ActorDef> {
        self.def_following(r, |d| d.template_traits)
    }

    /// GetClass's class of `f` (docs/spec-notes/actors.md §2.2): a reference's Traits
    /// source, the actor its leveled list gave it included [observed: tmpl3 (#1057)]; an
    /// actor base's along its Traits templates through NPC records, the record's own
    /// when the chain reaches a list [hypothesis, as GetRace].
    pub(crate) fn traits_class(&self, f: FormId) -> Option<FormId> {
        match base_of(self, f) {
            Some(_) => self.traits_def(f).and_then(|d| d.class),
            None => self.data.actors.get(&self.data.base_template_source(f, |d| d.template_traits)).and_then(|d| d.class),
        }
    }

    /// The definition supplying `r`'s base data: its name and the configuration flags
    /// (Essential, Respawn, No Low Level Processing, Invulnerable) (template flag "Base
    /// Data") [documented: GECK wiki Category:NPC, Template Data, "Base Data does not
    /// include the Quest Item, No Knockdowns, No VATS Melee, Is CharGen Face Preset, or
    /// Can Be All Races flags"].
    pub(crate) fn base_data_def(&self, r: FormId) -> Option<&'a ActorDef> {
        self.def_following(r, |d| d.template_base)
    }

    /// Whether the player may not go into VATS against `r` with a melee weapon: ACBS "No
    /// VATS Melee" on its own record, which a template's Base Data does not supply
    /// [documented: GECK wiki Category:NPC] (docs/spec-notes/actor-flags.md §1).
    pub fn no_vats_melee(&self, r: FormId) -> bool {
        r != FormId::PLAYER_REF && base_of(self, r).is_some_and(|b| self.data.no_vats_melee(b, self.pick_seed(r), self.leveled_at(r)))
    }

    /// Whether hits on `r` leave no blood spray and no blood decal (ACBS "No Blood Spray",
    /// "No Blood Decal"), from the definition supplying its model and blood ("Use
    /// Model/Animation includes the Blood … tabs" [documented: GECK wiki Category:NPC];
    /// hypothesis: the two flags go with that tab) (actor-flags.md §2).
    pub fn no_blood(&self, r: FormId) -> (bool, bool) {
        base_of(self, r).map_or((false, false), |b| self.data.no_blood(b, self.pick_seed(r), self.leveled_at(r)))
    }

    /// Whether creature `r` may be talked to: CREA ACBS "Allow PC Dialogue", from the
    /// definition supplying its base data (actor-flags.md §4) [hypothesis: not among the
    /// flags the wiki says Base Data leaves out].
    pub fn allows_pc_dialogue(&self, r: FormId) -> bool {
        self.base_data_def(r).is_some_and(|d| d.allow_pc_dialogue)
    }

    /// Whether `r` is a creature flagged "Can't Open Doors" (ACBS), from the definition
    /// supplying its base data [hypothesis: the flag is not among those the wiki says
    /// Base Data leaves out, so the template's counts].
    pub fn cant_open_doors(&self, r: FormId) -> bool {
        r != FormId::PLAYER_REF && self.base_data_def(r).is_some_and(|d| d.cant_open_doors)
    }

    /// The record that gives `r` its movement flags (Swims, Walks, Flies, No Combat in
    /// Water: the Animation tab's [documented: GECK wiki Animation Tab]): through a
    /// template with "Use Model/Animation" [hypothesis: the tab goes with the model].
    fn movement_def(&self, r: FormId) -> Option<&'a ActorDef> {
        self.def_following(r, |d| d.template_model)
    }

    /// Whether `r` keeps out of water: a creature that moves (Walks or Flies) without the
    /// Swims flag "will avoid using water triangles" [documented: GECK wiki Navmesh
    /// Creation Workflow] (its paths close them, `fnv_ai`). People swim by their
    /// packages' Allow Swimming; a creature with none of the three flags is left as it is.
    pub fn keeps_out_of_water(&self, r: FormId) -> bool {
        r != FormId::PLAYER_REF && self.movement_def(r).is_some_and(ActorDef::keeps_out_of_water)
    }

    /// Whether `r` holds its attacks while it swims: CREA "No Combat in Water", "will not
    /// perform attacks in water" [documented: GECK wiki Animation Tab].
    pub fn no_combat_in_water(&self, r: FormId) -> bool {
        r != FormId::PLAYER_REF && self.movement_def(r).is_some_and(|d| d.no_combat_in_water)
    }

    /// The current value of an actor value as the formulas read it: within its range, 0
    /// when it cannot be read.
    fn formula_input(&mut self, r: FormId, code: u16) -> f64 {
        capped(code, actor::actor_value(self, r, code).unwrap_or(0.0))
    }

    /// The skill formula for a value of the skill's attribute and of Luck: the skill's
    /// base setting + attribute × `fAVDSkillPrimaryBonusMult` + ⌈Luck ×
    /// `fAVDSkillLuckBonusMult`⌉ [documented: GECK wiki Derived Skill Settings, with the
    /// defaults 2, 2 and 0.5]. 0 for a code that is no skill.
    pub fn skill_formula(&self, skill: u16, attribute: f64, luck: f64) -> f64 {
        if !SKILLS.contains(&skill) {
            return 0.0;
        }
        setting(self, SKILL_BASE_SETTINGS[(skill - BARTER) as usize], 2.0)
            + attribute * setting(self, "fAVDSkillPrimaryBonusMult", 2.0)
            + (luck * setting(self, "fAVDSkillLuckBonusMult", 0.5)).ceil()
    }

    /// A skill from the actor's attribute and Luck.
    pub(crate) fn derived_skill(&mut self, r: FormId, skill: u16) -> f64 {
        let attribute = self.formula_input(r, GOVERNING[(skill - BARTER) as usize]);
        let luck = self.formula_input(r, LUCK);
        self.skill_formula(skill, attribute, luck)
    }

    /// The player's tag bonus on `skill`: fAVDTagSkillBonus when it is tagged, else 0.
    pub(crate) fn tag_bonus(&self, skill: u16) -> f64 {
        if self.player.tag_skills.contains(&skill) { setting(self, "fAVDTagSkillBonus", 15.0) } else { 0.0 }
    }

    fn base_skill(&mut self, r: FormId, skill: u16) -> f64 {
        let i = (skill - BARTER) as usize;
        if r == FormId::PLAYER_REF {
            let tag = self.tag_bonus(skill);
            let spent = self.player.skill_points.get(&skill).copied().unwrap_or(0.0);
            return self.derived_skill(r, skill) + tag + spent;
        }
        let d = self.stats_def(r);
        if let Some(values) = d.and_then(|d| d.skills) {
            let offset = d.and_then(|d| d.skill_offsets).map_or(0, |o| o[i]);
            return values[i] as f64 + offset as f64;
        }
        let creature = d.filter(|d| &d.sig == b"CREA");
        if let Some(v) = creature.and_then(|d| d.combat_skill).filter(|_| CREATURE_COMBAT_SKILLS.contains(&skill)) {
            return v as f64;
        }
        if let Some(v) = creature.and_then(|d| d.stealth_skill).filter(|_| skill == SNEAK) {
            return v as f64;
        }
        self.derived_skill(r, skill)
    }

    /// The base value of an actor value: what the actor's record and the formulas give,
    /// before SetActorValue, the modifiers and damage (see the module notes). `Err` for
    /// an attribute of something that has none.
    pub fn base_actor_value(&mut self, r: FormId, code: u16) -> Result<f64, String> {
        if SPECIALS.contains(&code) {
            let i = (code - STRENGTH) as usize;
            return match self.stats_def(r).and_then(|d| d.attributes) {
                Some(a) => Ok(a[i] as f64),
                None => self.world.actor_value(r, fnv_sim::forms::SPECIAL[i]).map(|v| v as f64).map_err(|e| e.to_string()),
            };
        }
        if code == HEALTH {
            return Ok(self.max_health(r));
        }
        if LIMB_CONDITIONS.contains(&code) {
            return Ok(100.0);
        }
        // Nothing but an actor has the rest.
        if self.stats_def(r).is_none() {
            return Ok(0.0);
        }
        if SKILLS.contains(&code) {
            return Ok(self.base_skill(r, code));
        }
        Ok(match code {
            // [documented: the settings' names, GECK wiki Settings; observed: 65 and 3 in
            // the masters; hypothesis: base + mult × Agility, as Carry Weight's form.]
            ACTION_POINTS => setting(self, "fAVDActionPointsBase", 65.0) + setting(self, "fAVDActionPointsMult", 3.0) * self.formula_input(r, AGILITY),
            // [observed in game: 200 at Strength 5, 160 at Strength 1, simulation.md §3.]
            CARRY_WEIGHT => setting(self, "fAVDCarryWeightsBase", 150.0) + setting(self, "fAVDCarryWeightMult", 10.0) * self.formula_input(r, STRENGTH),
            // [documented: GECK wiki Critical Hit Chance Formula.]
            CRITICAL_CHANCE => setting(self, "fAVDCritLuckBase", 0.0) + setting(self, "fAVDCritLuckMult", 1.0) * self.formula_input(r, LUCK),
            // [documented: GECK wiki Weapon Damage Formula.]
            MELEE_DAMAGE => {
                setting(self, "fAVDMeleeDamageStrengthMult", 0.5) * (setting(self, "fAVDMeleeDamageStrengthOffset", 0.0) + self.formula_input(r, STRENGTH))
            }
            UNARMED_DAMAGE => setting(self, "fAVDUnarmedDamageBase", 0.5) + setting(self, "fAVDUnarmedDamageMult", 0.05) * self.formula_input(r, UNARMED),
            // [documented: GECK wiki Stats Tab - NPC, both derive from Endurance;
            // hypothesis: (Endurance − 1) × 5 and × 2, the figures the Pip-Boy shows.]
            POISON_RESISTANCE => (self.formula_input(r, ENDURANCE) - 1.0) * 5.0,
            RAD_RESISTANCE => (self.formula_input(r, ENDURANCE) - 1.0) * 2.0,
            // [documented: xEdit ACBS fields; GECK wiki Stats Tab pages, "Speed Mult %"
            // and a creature's Fatigue. Hypothesis: an NPC's Fatigue is the field too.]
            SPEED_MULTIPLIER => self.stats_def(r).map_or(100.0, |d| d.speed_mult as f64),
            FATIGUE => self.stats_def(r).map_or(0.0, |d| d.fatigue as f64),
            // [documented: xEdit ACBS Karma; GECK wiki GetBaseKarma.] From the template
            // when the base takes its traits from one [hypothesis: which template flag
            // carries Karma is not documented; an NPC made from a template has 0 of its
            // own, so a templated Evil convict read as Neutral and killing it cost karma,
            // the owner's playtest list 5 item 22]; with Stats only, its own.
            KARMA => self.traits_def(r).map_or(0.0, |d| d.karma),
            // [documented: xEdit AIDT; hypothesis: the value is the enumeration's code.]
            AGGRESSION | CONFIDENCE | ENERGY | RESPONSIBILITY | MOOD | ASSISTANCE => {
                let d = self.def_following(r, |d| d.template_ai);
                let v = match code {
                    AGGRESSION => d.and_then(|d| d.aggression),
                    CONFIDENCE => d.and_then(|d| d.confidence),
                    ENERGY => d.and_then(|d| d.energy).map(i64::from),
                    RESPONSIBILITY => d.and_then(|d| d.responsibility),
                    MOOD => d.and_then(|d| d.mood),
                    _ => d.and_then(|d| d.assistance),
                };
                v.unwrap_or(0) as f64
            }
            // What the actor carries [documented: GECK wiki GetCalculatedMaxCarryWeight,
            // "CarryWeight and InventoryWeight are both Actor Values"].
            INVENTORY_WEIGHT => self.world.carried_weight(r).unwrap_or(0.0),
            _ => 0.0,
        })
    }
}
