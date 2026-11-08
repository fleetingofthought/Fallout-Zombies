//! The Layer 5 game state over the Layer 4 world: script instances and their variables,
//! quests, event dispatch, conditions, and the [`fnv_script::vm::Host`] that connects the
//! interpreter to all of it. Behaviour notes with evidence: docs/spec-notes/scripts.md
//! §4–7.

use crate::cond::{self, Condition, Operand, RunOn};
use crate::data::{Compiled, GameData, UnitId};
use crate::functions::{self, Args, Ctx, Kind};
use crate::quest::{QuestState, STAGE_COMPLETE, STAGE_FAIL};
use crate::trace::{Event, EventKind};
use fnv_script::ir::{Arg, Block, RefIndex, Stmt, VarRef, VarSlot};
use fnv_script::unit::{RefSlot, VarType};
use fnv_script::vm::{to_int_storage, ArgValue, Host, RunOpts, Value, Vm, VmError};
use fnv_sim::world::{Outcome, Snapshot};
use fnv_sim::{FormId, World};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The last hit an actor took (`crate::combat`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LastHit {
    pub attacker: Option<FormId>,
    pub weapon: Option<FormId>,
    /// GetHitLocation's index: −1 none, 0 torso, 1–2 head, 3–6 arms, 7–12 legs, 13 brain,
    /// 14 weapon [documented: catalogue notes of GetHitLocation].
    pub location: i32,
    pub critical: bool,
}

/// The furniture an actor is using (docs/spec-notes/furniture.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FurnitureUse {
    /// The furniture reference.
    pub reference: FormId,
    /// The ID of the model marker the actor entered by (`GetFurnitureMarkerID`).
    pub marker_id: u8,
    /// `GetSitting` / `GetSleeping`: 2 getting on, 3 on it, 4 getting off.
    pub state: u8,
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// Begin-block event codes (docs/spec-notes/script-functions.md §3.3 [observed]).
pub mod event {
    pub const GAME_MODE: u16 = 0x00;
    pub const MENU_MODE: u16 = 0x01;
    pub const ON_ACTIVATE: u16 = 0x02;
    pub const ON_ADD: u16 = 0x03;
    pub const ON_EQUIP: u16 = 0x04;
    pub const ON_UNEQUIP: u16 = 0x05;
    pub const ON_DROP: u16 = 0x06;
    pub const SAY_TO_DONE: u16 = 0x07;
    pub const ON_HIT: u16 = 0x08;
    pub const ON_HIT_WITH: u16 = 0x09;
    pub const ON_DEATH: u16 = 0x0A;
    pub const ON_COMBAT_END: u16 = 0x0C;
    pub const ON_PACKAGE_START: u16 = 0x0F;
    pub const ON_PACKAGE_DONE: u16 = 0x10;
    pub const ON_PACKAGE_CHANGE: u16 = 0x14;
    pub const ON_LOAD: u16 = 0x15;
    pub const SCRIPT_EFFECT_START: u16 = 0x11;
    pub const SCRIPT_EFFECT_FINISH: u16 = 0x12;
    pub const SCRIPT_EFFECT_UPDATE: u16 = 0x13;
    pub const ON_DESTRUCTION_STAGE_CHANGE: u16 = 0x23;
    pub const ON_FIRE: u16 = 0x24;
    pub const ON_GRAB: u16 = 0x21;
    pub const ON_RELEASE: u16 = 0x22;
    pub const ON_TRIGGER: u16 = 0x18;
    pub const ON_START_COMBAT: u16 = 0x19;
    pub const ON_RESET: u16 = 0x1E;
    pub const ON_TRIGGER_ENTER: u16 = 0x1A;
    pub const ON_TRIGGER_LEAVE: u16 = 0x1B;
    /// On the reference a magic effect reached; its parameter is the base effect (MGEF)
    /// [documented: GECK wiki event handler table, OnMagicEffectHit "Target, BaseEffect"].
    pub const ON_MAGIC_EFFECT_HIT: u16 = 0x16;
    /// Never fired, as in the original: the GECK wiki notes that this event "never ran
    /// prior to xNVSE v6.2.8", so the two shipped OnSell blocks (Lonesome Road's
    /// Blood-Nap, Gun Runners' Arsenal's Esther) never ran in the game either
    /// [documented: GECK wiki event handler table, OnSell].
    pub const ON_SELL: u16 = 0x17;
    /// On the actor's own script when it equips an item; the parameter is the item
    /// (base form). It runs for an aid item an EquipItem uses up too: the Legate's
    /// script waits for his healing powder this way [documented: GECK wiki event handler
    /// table, OnActorEquip "Equipper, Equipped (item Base Form)", "runs before the item is
    /// considered as equipped"; observed: VLegateScript `Begin OnActorEquip
    /// NVHealingPowderLegate` answers its own `EquipItem` of the powder].
    pub const ON_ACTOR_EQUIP: u16 = 0x1C;
    /// On a door or container that opens: a door's open flag turning on (by anyone), a
    /// container's menu opening [documented: GECK wiki event handler table, OnOpen
    /// "Opened"; hypothesis: the moments, and that an NPC opening a door counts].
    pub const ON_OPEN: u16 = 0x1F;
    /// On a door or container that closes: the door's open flag turning off, the
    /// container's menu closing [documented: GECK wiki event handler table, OnClose
    /// "Closed"; hypothesis: the moments].
    pub const ON_CLOSE: u16 = 0x20;
}

/// Nested script runs allowed (a stage result script setting another stage, an
/// `Activate` running an OnActivate block) [documented: GECK wiki SetStage, at least 250
/// recursive SetStage calls work]. Stack: 256 nested SetStage result scripts ran in a
/// 768 KiB thread stack and overflowed 640 KiB [observed: release build, synthetic
/// quest, 2026-10-24], so they fit a 2 MiB worker thread and, with less room, the 1 MiB
/// Windows main thread; deeper frames of other nestings are not measured.
pub const MAX_DEPTH: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GameConfig {
    /// Fixed frame step in seconds (deterministic traces).
    pub frame_seconds: f64,
    /// Unknown opcodes and undecodable pieces stop a script (tests) instead of being
    /// logged and skipped.
    pub strict: bool,
    /// Trace writes to an instance's own locals (off by default: timers write every
    /// frame). Writes to another script's variables and to globals are always traced.
    pub trace_locals: bool,
    pub seed: u64,
    pub max_steps: u64,
    /// A message box is a menu that holds GameMode until the player closes it
    /// ([`Game::show_box`]) [observed: layer5-captures.md §10.1]. Off: message boxes are
    /// only traced, as before round 4 (the headless runner for inputs that do not state
    /// the menus of their save, docs/spec-notes/scripts.md §8).
    #[serde(default = "yes")]
    pub modal_message_boxes: bool,
    /// The AI runs packages: actors around the player choose their package again every
    /// this many seconds ([`Game::tick`], `crate::ai`), and a package's begin, end and
    /// change scripts run when it starts, finishes and is replaced. `None` (the
    /// default): packages change only when a script or an input asks
    /// (`EvaluatePackage`, the runner's `package` input), as in Layer 5. The game's
    /// interval is about 10 seconds [documented: GECK wiki, How Are Packages Evaluated].
    #[serde(default)]
    pub package_interval: Option<f64>,
}

fn yes() -> bool {
    true
}

impl Default for GameConfig {
    fn default() -> Self {
        GameConfig { frame_seconds: 1.0 / 60.0, strict: false, trace_locals: false, seed: 0, max_steps: 100_000, modal_message_boxes: true, package_interval: None }
    }
}

/// Whose variables a script instance holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstKey {
    /// A placed or created reference whose base form has a script.
    Ref { reference: FormId },
    /// A quest's script.
    Quest { quest: FormId },
    /// A scripted item in an inventory (one instance per holder and item).
    Item { holder: FormId, item: FormId },
    /// The effect script of one active magic effect ([`ActiveEffect::id`]).
    Effect { id: u32 },
    /// A challenge's reward script, run once (`crate::challenges`).
    Challenge { challenge: FormId },
}

impl InstKey {
    pub fn label(&self) -> String {
        match self {
            InstKey::Ref { reference } => format!("ref:{reference}"),
            InstKey::Quest { quest } => format!("quest:{quest}"),
            InstKey::Item { holder, item } => format!("item:{item}@{holder}"),
            InstKey::Effect { id } => format!("effect:{id}"),
            InstKey::Challenge { challenge } => format!("challenge:{challenge}"),
        }
    }
}

/// A script instance: the variables of one running copy of a script.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub script: Option<FormId>,
    pub vars: BTreeMap<u16, Value>,
    /// Clock time (1/64 s steps) at which the instance's per-frame block (GameMode,
    /// ScriptEffectUpdate) last started to run, and the start of the run before it:
    /// GetSecondsPassed is the difference (docs/spec-notes/scripts.md §5).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub run_last: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub run_prev: Option<f64>,
}

/// [`RefExtra::script_package`] from a save: one package, none, or the list older saves
/// kept (its last entry was the current one).
fn one_script_package<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<FormId>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Slot {
        One(FormId),
        List(Vec<FormId>),
    }
    Ok(match Option::<Slot>::deserialize(d)? {
        Some(Slot::One(p)) => Some(p),
        Some(Slot::List(l)) => l.last().copied(),
        None => None,
    })
}

/// Per-reference state the Layer 4 world does not keep.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RefExtra {
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub dead: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub destroyed: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub ghost: bool,
    /// SetGhost 0 on an actor whose base is flagged Is Ghost: it stops being one
    /// (actor-flags.md §3) [hypothesis: SetGhost overrides the base flag].
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub ghost_off: bool,
    /// It caught the player picking its pocket: it cannot be pickpocketed again
    /// (`crate::pickpocket`; the flag SetCaughtPCPickpocketting sets [documented: GECK
    /// wiki]).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub caught_pickpocket: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub restrained: bool,
    /// The player woke it by talking to it while it slept: until this game time it stays
    /// off its bed (`Game::wake`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub woken_until: Option<f64>,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub unconscious: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub marked_for_delete: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub teammate: bool,
    /// SetVatsTarget's VATS targeting for this reference, over its base's DEST flag
    /// (`Game::vats_targetable`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub vats_target: Option<bool>,
    /// SetCombatStyle's combat style for this reference, over its base form's
    /// (`Game::combat_style`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub combat_style: Option<FormId>,
    /// SetAlert's alerted animation state (weapon drawn) [documented: GECK wiki SetAlert,
    /// savebaked].
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub alerted: bool,
    /// SetForceSneak's forced sneaking [documented: GECK wiki SetForceSneak, "Turns force
    /// sneak on or off"].
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub force_sneak: bool,
    /// Faction rank changes: a rank, or −1 for "removed".
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub factions: BTreeMap<FormId, i8>,
    /// SetActorValue's number in place of an actor value's base, by code. Not for
    /// S.P.E.C.I.A.L. of actors whose attributes the world holds (`functions::actor`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub actor_values: BTreeMap<u16, f64>,
    /// The permanent modifier per actor value code (ModActorValue, ForceActorValue),
    /// S.P.E.C.I.A.L. included.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub av_permanent: BTreeMap<u16, f64>,
    /// Damage taken per actor value code (DamageActorValue; RestoreActorValue heals it),
    /// S.P.E.C.I.A.L. included. The current value is the permanent value minus this.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub av_damage: BTreeMap<u16, f64>,
    /// The temporary modifier per actor value code: the Value Modifier effects with
    /// Recover active on the actor (`crate::aid`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub av_temporary: BTreeMap<u16, f64>,
    /// The package added with AddScriptPackage: one slot, a second call replaces the
    /// first and RemoveScriptPackage empties it, giving the actor its own list back
    /// [documented: GECK wiki AddScriptPackage notes, "only one script package at a
    /// time"; observed: od_08_script_packages, owner-decision-captures.md item 8].
    /// Saves from before the slot held a list (`packages`); its last entry is read.
    #[serde(skip_serializing_if = "Option::is_none", default, alias = "packages", deserialize_with = "one_script_package")]
    pub script_package: Option<FormId>,
    /// The AI package the actor chose when its packages were last evaluated
    /// ([`Game::evaluate_package`]); `None` until then.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub current_package: Option<FormId>,
    /// The current package reached its goal ([`Game::package_goal_met`]).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub package_goal_met: bool,
    /// Once Per Day packages: GameDaysPassed when the actor last chose each.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub package_chosen_at: BTreeMap<FormId, f32>,
    /// Game time ([`Game::time`]) of the actor's next periodic package choice.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub package_due: Option<f64>,
    /// Packages whose location the actor could not reach: not chosen until
    /// GameDaysPassed reaches the value ([`Game::block_package`]).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub package_blocked: BTreeMap<FormId, f32>,
    /// The current package (first) ends when GameDaysPassed reaches the second
    /// ([`Game::end_timed_packages`]): set by SetPackDuration, or when a package with a
    /// duration and no starting hour is chosen (`Game::set_package`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub package_ends: Option<(FormId, f64)>,
    /// The `package_ends` timer is the package's own schedule duration (a package with
    /// no starting hour), not SetPackDuration's: Must Complete and Continue If PC Near
    /// hold it ([`Game::end_timed_packages`]).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub package_ends_by_schedule: bool,
    /// SetActorsAI 0: the actor's AI does not run (no package choice, no detection, no
    /// walking); its scripts still do.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub ai_off: bool,
    /// SetActorRefEssential (the reference only; the base's flag is separate).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub essential: Option<bool>,
    /// Who killed the actor (KillActor's killer), for IsKiller.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub killer: Option<FormId>,
    /// KillActor's cause of death, which GetCauseofDeath reads before the killing hit.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub death_cause: Option<i32>,
    /// The furniture the actor is on, getting on or getting off.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub furniture: Option<FurnitureUse>,
    /// The item (base form) the actor last used or took under its package (a Find or
    /// Use Item At package), until its package changes; with the furniture it is on, what
    /// GetIsUsedItem and its kin read (`functions::furniture`). Not saved.
    #[serde(skip)]
    pub used_item: Option<FormId>,
    /// What the AI layer has the actor do within its package (`crate::ai::AiPhase`),
    /// for GetCurrentAIProcedure. Not saved.
    #[serde(skip)]
    pub ai_phase: Option<crate::ai::AiPhase>,
    /// The last hit taken (combat), for GetHitLocation.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_hit: Option<LastHit>,
    /// The hit that killed the actor, for GetKillingBlowLimb, GetCauseofDeath and the kill
    /// camera; `None` when no hit did (KillActor, an effect's damage, a script's) or when
    /// the save is older than this record, whose hits are not guessed at from `last_hit`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub killing_hit: Option<LastHit>,
    /// Its dismembered body parts by location (GetHitLocation's numbering), true when the
    /// part exploded, false when it was severed (docs/spec-notes/dismemberment.md §5).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub limbs_gone: BTreeMap<i32, bool>,
    /// The actor it last hit (GetOwnerLastTarget, `crate::unconscious`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_target: Option<FormId>,
    /// Knocked out: an essential actor at zero health, or its Fatigue ran out
    /// (`crate::unconscious`). Not SetUnconscious's flag (`unconscious`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub knocked_out: Option<crate::unconscious::KnockOut>,
    /// Concussed until this game time (GetConcussed, `crate::magic_archetypes`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub concussed_until: Option<f64>,
    /// Its head was crippled when last looked at while it had a Concussion effect
    /// (`crate::magic_archetypes`).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub head_crippled: bool,
    /// The actor has its weapon out (IsWeaponOut): told by whoever animates the actor
    /// ([`Game::set_weapon_out`]).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub weapon_out: bool,
    /// The weapon the animating side draws for an actor whose inventory here has none
    /// equipped (an NPC's weapon as its appearance resolves it), for GetWeaponAnimType
    /// ([`Game::set_held_weapon`]). Not saved: told again when the actor is drawn.
    #[serde(skip)]
    pub held_weapon: Option<FormId>,
    /// Health damage taken from the player and its hired companions (`Game::kill_earns_xp`),
    /// and from anyone (kill XP's share rule).
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub player_damage: f64,
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub total_damage: f64,
    /// Object damage taken (DamageObject), for the destruction stage.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub damage: Option<f64>,
    /// Self-damage per second of the destruction stage reached (burning down).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub burning: Option<f64>,
    /// Spells added with AddSpell, in order (the authored abilities among them).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub spells: Vec<FormId>,
    /// The actor's authored abilities (record and race SPLO) were given
    /// (`crate::abilities`), and the race they were given for.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub abilities_given: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ability_race: Option<FormId>,
    /// Abilities a Dispel took off while the actor is loaded (they come back when it is
    /// loaded again), and worn items whose enchantment a Dispel took off (until the item
    /// is unequipped).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub dispelled: Vec<FormId>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub worn_dispelled: Vec<FormId>,
    /// Perks given to this actor itself (`<actor>.AddPerk perk`), with their ranks
    /// ([`Game::held_perks`]); the player's own are `Player::perks`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub perks: BTreeMap<FormId, u8>,
    /// SetBroadcastState of a radio station (talking activator).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub broadcast: Option<bool>,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub ignore_crime: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub ignore_friendly_hits: bool,
    /// SetTalkingActivatorActor.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub talking_actor: Option<FormId>,
    /// SetCriticalStage (0 none, 1 goo start, 2 goo end, 3 disintegrate start, 4 end).
    #[serde(skip_serializing_if = "is_zero", default)]
    pub critical_stage: u16,
    /// The ash or goo pile AttachAshPile left of this actor (`crate::ash_piles`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ash_pile: Option<FormId>,
    /// On a pile: the actor it came from (GetAshPileSource; `crate::ash_piles`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ash_pile_source: Option<FormId>,
    /// SetScale in saves from before 2026-10-18 (it is now kept on the reference's state,
    /// `fnv_sim::world::RefState::scale`); read by GetScale, cleared by the next SetScale.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scale: Option<f64>,
    /// SetLevel of an actor other than the player.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub level: Option<i32>,
    /// AgeRace: the race in place of the record's (`crate::appearance`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub race: Option<FormId>,
    /// SexChange: true = female, in place of the record's sex (`crate::appearance`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub female: Option<bool>,
    /// The player ate this corpse (StartCannibal; HasBeenEaten, `crate::activate_choices`).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub eaten: bool,
    /// SetMinimalUse of a door.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub minimal_use: Option<bool>,
    /// The actor has been in a conversation with the player (GetTalkedToPC).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub talked_to_pc: bool,
    /// SetDisposition: the disposition towards an actor, set by a script
    /// (`functions::actor::get_disposition`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub dispositions: BTreeMap<FormId, f64>,
    /// SetMerchantContainer: the container of the actor's goods in place of its
    /// reference's (`Game::merchant_container`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub merchant_container: Option<FormId>,
    /// Wait: the Follow or Accompany package the actor waits in, standing where it is,
    /// until StopWaiting (`functions::fo3`; the AI's follow step reads [`Game::waits`]).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub waiting: Option<FormId>,
    /// MatchFaceGeometry: the morphs of the actor's face towards others', in call order
    /// (`crate::appearance::FaceMatch`; the engine draws the head with them).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub face_matches: Vec<crate::appearance::FaceMatch>,
    /// The special idle the actor plays (IsIdlePlaying): told by whoever animates the
    /// actor ([`Game::set_idle_playing`]); not part of the saved state.
    #[serde(skip)]
    pub idle: Option<FormId>,
    /// The special idle the actor last played (IsLastIdlePlayed): the last one told
    /// ([`Game::set_idle_playing`]); not part of the saved state.
    #[serde(skip)]
    pub last_idle: Option<FormId>,
    /// How many times ResurrectActor rolled the actor's random choices again (its leveled
    /// template and carried items, `crate::resurrect`); the seed of its picks
    /// (`fnv_sim::leveled::roll_seed`).
    #[serde(skip_serializing_if = "is_zero_u32", default)]
    pub rolls: u32,
}

fn is_zero(v: &u16) -> bool {
    *v == 0
}

/// A challenge's run-time state (UnlockChallenge, RemoveRecurringFromChallenge, the
/// completion the Layer 6 challenge counters report).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChallengeState {
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub unlocked: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub completed: bool,
    /// RemoveRecurringFromChallenge cleared the Recurring flag.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub not_recurring: bool,
    /// Progress towards the threshold (`crate::challenges`).
    #[serde(skip_serializing_if = "is_zero_u32", default)]
    pub progress: u32,
    /// A part step not yet counted (damage), below 1.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub carry: f64,
    /// Times completed (a recurring challenge more than once).
    #[serde(skip_serializing_if = "is_zero_u32", default)]
    pub times: u32,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// EnableFastTravel's three flags (all false = the default: travel and waiting allowed).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct FastTravel {
    pub travel_disabled: bool,
    pub wait_disabled: bool,
    /// The setting survives an interior → exterior change.
    pub keep: bool,
}

impl FastTravel {
    pub fn is_default(&self) -> bool {
        *self == FastTravel::default()
    }
}

/// Run-time changes to base forms and cells (they apply to every reference of the form)
/// [documented per function: catalogue summaries of SetEssential, SetActorFullName,
/// SetQuestObject, SetCellOwnership, SetCellPublicFlag, SetItemValue, SetEnemy, SetAlly].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FormChanges {
    /// Faction → other faction → group combat reaction (0 neutral, 1 enemy, 2 ally,
    /// 3 friend), over the FACT XNAM list.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub faction_reactions: BTreeMap<FormId, BTreeMap<FormId, i64>>,
    /// SetEssential on actor bases.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub essential: BTreeMap<FormId, bool>,
    /// SetActorFullName: base → the message whose title is the new name.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub names: BTreeMap<FormId, FormId>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub quest_items: BTreeMap<FormId, bool>,
    /// SetCellOwnership (None: the owner was cleared).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub cell_owners: BTreeMap<FormId, FormId>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub public_cells: BTreeMap<FormId, bool>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub item_values: BTreeMap<FormId, i32>,
    /// SetCellImageSpace: cell → the image space it has instead of its XCIM.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub cell_image_spaces: BTreeMap<FormId, FormId>,
    /// SetFactionReaction: faction → other faction → disposition modifier, in place of
    /// FACT XNAM's (`functions::fo3`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub faction_modifiers: BTreeMap<FormId, BTreeMap<FormId, i64>>,
    /// ResetInterior: interior cells reset at their next load (`crate::respawn`).
    #[serde(skip_serializing_if = "std::collections::BTreeSet::is_empty", default)]
    pub expired_cells: std::collections::BTreeSet<FormId>,
    /// ShowOff's ResetInteriorAlt: cells reset at their next load with their dead
    /// respawning actors brought back (`crate::respawn`) [documented: ResetInteriorAlt].
    #[serde(skip_serializing_if = "std::collections::BTreeSet::is_empty", default)]
    pub expired_cells_full: std::collections::BTreeSet<FormId>,
    /// RemapWaterType: water type → the one drawn in its place ([`Game::water_type`]).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub water_remaps: BTreeMap<FormId, FormId>,
    /// TriggerLODApocalypse has run (Megaton's destruction, `functions::fo3`).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub lod_apocalypse: bool,
}

/// A magic effect active on an actor (docs/spec-notes/scripts.md §7.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveEffect {
    pub id: u32,
    pub target: FormId,
    /// The magic item it came from (ALCH, SPEL, ENCH, INGR).
    pub source: FormId,
    /// The base effect (MGEF).
    pub effect: FormId,
    /// Its index in the source's effect list.
    pub index: usize,
    pub magnitude: u32,
    /// Seconds; ignored when `permanent`.
    pub duration: f64,
    pub elapsed: f64,
    /// An ability added with AddSpell: lasts until removed.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub permanent: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub caster: Option<FormId>,
    /// The effect script (MGEF archetype Script), if any.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub script: Option<FormId>,
    /// What it does to an actor value (Value Modifier, Value And Parts): `crate::aid`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub change: Option<crate::aid::ValueChange>,
    /// The usage monitor of an ingestible: the magnitudes of its uses while it lasts
    /// (GetSpellUsageNum; docs/spec-notes/aid-items.md §5).
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub usage: f64,
    /// A worn enchantment: the equipped item it comes from (`crate::abilities`); it
    /// lasts while the item is worn.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub worn: Option<FormId>,
    /// An ability's or worn enchantment's effect whose conditions fail: listed, with no
    /// value change and no script running (aid-items.md §13).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub inactive: bool,
}

/// An image space modifier playing on the screen (docs/spec-notes/image-modifiers.md §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveImageModifier {
    /// The IMAD record.
    pub modifier: FormId,
    /// Seconds since it was applied ([`Game::image_modifiers`] says which seconds count).
    pub elapsed: f64,
}

/// What the last ShowAllMapMarkers asked for (docs/design/script-stubs.md §3.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllMarkers {
    /// Every marker is on the map (1), or the call took them off (0).
    pub shown: bool,
    /// Fast travel to them is allowed (no fast travel is built yet; kept for it).
    pub travel: bool,
    /// Markers flagged "Show All" Hidden are left out.
    pub skip_hidden: bool,
}

/// What the race menu (1036) lets the player change, by the function that opened it:
/// ShowRaceMenu, ShowBarberMenu and ShowPlasticSurgeonMenu all open menu 1036
/// [documented: GECK wiki "MenuMode (Function)", notes: "ShowRaceMenu,
/// ShowPlasticSurgeonMenu, and ShowBarberMenu all return true for MenuMode 1036"].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RaceMenu {
    /// ShowRaceMenu: everything (the character creator).
    #[default]
    Creator,
    /// ShowBarberMenu: the hair [documented: GECK wiki ShowBarberMenu, "select hair color
    /// and style"] and the beard [owner 2026-10-17, todo 6]; not the race, sex or name
    /// [owner].
    Barber,
    /// ShowPlasticSurgeonMenu: the face [documented: GECK wiki ShowPlasticSurgeonMenu,
    /// "allows the player to change their face"]: its shape, skin, eyes and brows, the
    /// starting faces; not the hair or beard, the race, sex or name [owner 2026-10-18,
    /// todo 6].
    Surgeon,
}

/// The dialogue topics the player knows, newest first: the order that breaks ties of
/// topic priority in the topic list (`Game::build_choices`).
///
/// The topics one line adds go to the front together, in their record order; a line's
/// topic already known moves to the front again ([`KnownTopics::learn`]). `AddTopic` on a
/// known topic leaves it where it is ([`KnownTopics::insert`]): a script may call it every
/// frame, and the list must not depend on how often scripts run [design: review of
/// #1205]. The original's lists for Boone and Manny Vargas follow no rule of learning
/// order [observed 2026-10-31, dialogue.md §3.5]; newest first is the owner's choice of
/// 2026-10-31 [design]. Saves from before kept a sorted set; they load in that order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KnownTopics(Vec<FormId>);

impl KnownTopics {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, topic: &FormId) -> bool {
        self.0.contains(topic)
    }

    /// Newest first.
    pub fn iter(&self) -> std::slice::Iter<'_, FormId> {
        self.0.iter()
    }

    /// Learns `topics` (one line's Add Topics): all go to the front, the known ones too,
    /// and returns those that were not known before, in their order.
    pub fn learn(&mut self, topics: &[FormId]) -> Vec<FormId> {
        let mut batch: Vec<FormId> = Vec::with_capacity(topics.len());
        for &t in topics {
            if !batch.contains(&t) {
                batch.push(t);
            }
        }
        let new: Vec<FormId> = batch.iter().copied().filter(|t| !self.0.contains(t)).collect();
        self.0.retain(|t| !batch.contains(t));
        self.0.splice(0..0, batch);
        new
    }

    /// Learns one topic (`AddTopic`): to the front if it was not known, else it stays
    /// where it is. True if it was not known before.
    pub fn insert(&mut self, topic: FormId) -> bool {
        let new = !self.0.contains(&topic);
        if new {
            self.0.insert(0, topic);
        }
        new
    }
}

impl Extend<FormId> for KnownTopics {
    fn extend<I: IntoIterator<Item = FormId>>(&mut self, iter: I) {
        let topics: Vec<FormId> = iter.into_iter().collect();
        self.learn(&topics);
    }
}

/// The player's state outside the world's reference table.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub xp: f64,
    /// AutoDisplayObjectives: the active quest's objectives show on their own
    /// (`functions::ui::auto_display_objectives`).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub auto_objectives: bool,
    /// The name typed in the name menu (`crate::chargen`); `None` before it.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub perks: BTreeMap<FormId, u8>,
    /// Perks given to the player's teammates (AddPerk with forTeammates).
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub teammate_perks: BTreeSet<FormId>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub notes: BTreeSet<FormId>,
    /// Notes read in the Pip-Boy (GetNoteRead, `Game::mark_note_read`), and those
    /// SetNoteRead marked with `serialize` 1 [documented: SetNoteRead].
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub notes_read: BTreeSet<FormId>,
    /// Infamy (index 0) and fame (index 1) per reputation, as GetReputation's axis.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub reputation: BTreeMap<FormId, [f64; 2]>,
    /// What the player's OnEquip / OnUnequip blocks moved out of [`Self::reputation`]
    /// (faction armour's disguise, `functions::reputation::real`); added to it, the
    /// reputation the player really has.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub reputation_hidden: BTreeMap<FormId, [f64; 2]>,
    /// `SexChange`: true = female.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub female: Option<bool>,
    /// The look chosen in the character creator (the race menu, `crate::chargen::Look`);
    /// `None` keeps the player record's.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub look: Option<crate::chargen::Look>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub tag_skills: BTreeSet<u16>,
    /// The tag skills SetPlayerTagSkill put in its four slots, by slot (0–3); the
    /// tag-skill menu's choice replaces them.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub tag_slots: BTreeMap<u8, u16>,
    /// The points spent on each skill in the level-up menu (`crate::levelup`), added to
    /// the skill's formula [documented: GECK wiki ActorValue, the player's spent skill
    /// points sit on top of the derived base; docs/spec-notes/levelup.md §4].
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub skill_points: BTreeMap<u16, f64>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub achievements: BTreeSet<i32>,
    /// Actors killed, by base form (GetDeadCount).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub dead_count: BTreeMap<FormId, u32>,
    /// Disabled control groups: movement, Pip-Boy, fighting, POV, looking, rollover
    /// text, sneaking.
    #[serde(default)]
    pub controls_disabled: [bool; 7],
    /// SetHardcore (otherwise the configuration's).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hardcore: Option<bool>,
    /// The stage record (RADS, DEHY, HUNG, SLPD) whose effect the player has, by the
    /// need's actor value code (`crate::needs`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub need_stages: BTreeMap<u16, FormId>,
    /// The part of a point each need has built up over hours waited or slept whose share
    /// is under one point (`crate::needs`, a high TimeScale).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub need_carry: BTreeMap<u16, f64>,
    /// Miscellaneous statistics by code (ModPCMiscStat).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub misc_stats: BTreeMap<u16, f64>,
    /// Map markers ShowMap made visible.
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub map_markers: BTreeSet<FormId>,
    /// Map markers ShowMap made fast-travel targets (its canTravel argument).
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub travel_markers: BTreeSet<FormId>,
    /// Map markers the player found by coming near them: the engine's discovery
    /// (`nvf-play` compass), told to the rules (`Game::set_found_markers`).
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub found_markers: BTreeSet<FormId>,
    /// ShowAllMapMarkers: every marker shown at once; `None` before any call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_markers: Option<AllMarkers>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub challenges: BTreeMap<FormId, ChallengeState>,
    #[serde(skip_serializing_if = "FastTravel::is_default", default)]
    pub fast_travel: FastTravel,
    /// Factions the player is an enemy of after a crime (SetPCEnemyOfFaction).
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub enemy_of: BTreeSet<FormId>,
    /// SetInCharGen.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub in_chargen: bool,
    /// Casino winnings level (0–4) per casino (CSNO), set by the casino games (Layer 6).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub casino_levels: BTreeMap<FormId, i32>,
    /// The net chips won at each casino over every game (`crate::casino`; a ban at the
    /// casino's max winnings).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub casino_earnings: BTreeMap<FormId, i64>,
    /// The roulette help message has been shown (it comes the first time only).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub roulette_help_shown: bool,
    /// The blackjack help message has been shown (it comes the first time only).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub blackjack_help_shown: bool,
    /// The slot machine help message has been shown (it comes the first time only).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub slots_help_shown: bool,
    /// The Caravan cards owned, the last deck built and the record against each opponent
    /// (`crate::caravan`).
    #[serde(skip_serializing_if = "crate::caravan::CaravanState::is_empty", default)]
    pub caravan: crate::caravan::CaravanState,
    /// SetPCCanUsePowerArmor.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub power_armor: bool,
    /// SetPCYoung (Fallout 3's childhood): the race's default hair, no face attachments.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub young: bool,
    /// SetPCToddler: the toddler's walk animations.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub toddler: bool,
    /// SetNoActivationSound: the sound of a refused activation in place of the default.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub no_activation_sound: Option<FormId>,
    /// SetLocationSpecificLoadScreensOnly 1: only load screens tied to the place loaded.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub location_load_screens_only: bool,
    /// Dialogue topics added with AddTopic and lines' Add Topics, newest first
    /// ([`KnownTopics`]).
    #[serde(skip_serializing_if = "KnownTopics::is_empty", default)]
    pub topics: KnownTopics,
    /// SetLevel on the player (otherwise the configuration's level).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub level: Option<i32>,
    /// AgeRace on the player before the character creator gave a look (with a look, the
    /// look's race changes; `crate::appearance`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub race: Option<FormId>,
    /// Hardcore mode was off at some point (IsAlwaysHardcore).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub hardcore_was_off: bool,
    /// The player murdered a non-Evil NPC (IsPCAMurderer; `crate::crime`).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub murderer: bool,
    /// Crimes against each actor: [minor, major] (GetMinorCrimeCount, GetMajorCrimeCount).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub crime_counts: BTreeMap<FormId, [u32; 2]>,
    /// Actors that are the player's enemies because of crimes against them or seen by
    /// them (GetActorCrimePlayerEnemy; `crate::crime`).
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub crime_enemies: BTreeSet<FormId>,
}

/// A message box waiting for a button (`ShowMessage` of a message with buttons).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageBox {
    pub message: FormId,
    pub buttons: usize,
    /// The button chosen (runner input), returned once by `GetButtonPressed`.
    pub pressed: Option<i32>,
    /// The script that showed the box (its instance, or the unit of a result script):
    /// only that script's `GetButtonPressed` receives the button; any other script
    /// reads −1 and leaves the box alone [hypothesis, from the GECK wiki's ShowMessage
    /// and GetButtonPressed pages: the script that shows a message box polls its button].
    /// `None` (a box from an older snapshot): the first caller takes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// The buttons whose conditions failed when the box was shown ([`Game::hidden_buttons`]):
    /// not on screen, so the player cannot pick them [documented: GECK wiki
    /// GetButtonPressed, as [`Game::hidden_buttons`]].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<usize>,
}

impl MessageBox {
    /// The identity of a calling script for [`MessageBox::owner`].
    pub fn owner_of(inst: Option<InstKey>, unit: &str) -> String {
        match inst {
            Some(k) => k.label(),
            None => unit.to_string(),
        }
    }

    /// Whether the player can pick `button`: one the box shows. A hidden button is
    /// refused (the headless runner's `press_button` once pressed it all the same: the
    /// Lucky 38 mainframe's "Bug the Lucky 38 Network", hidden until Emily Ortal gives the
    /// bug, set The Moon Comes Over the Tower's stage 20 without it).
    pub fn can_press(&self, button: i32) -> Result<(), String> {
        if usize::try_from(button).is_ok_and(|b| self.hidden.contains(&b)) {
            return Err(format!("button {button} is not shown (its conditions fail)"));
        }
        Ok(())
    }

    /// Whether the box still waits for the player's click. An answered box stays in
    /// [`Game::message_box`] until its script reads the button, which a fire-and-forget
    /// box (a mod's MessageBoxEx warning at start) never does: it is off the screen all
    /// the same, and must not count as open (saving was refused for a whole session, #1649).
    pub fn waiting(&self) -> bool {
        self.pressed.is_none()
    }
}

/// Menu codes (`MenuMode <code>`) of the menus scripts open [observed: each code was
/// open right after the stage whose result script calls the function, save `nvfnew`,
/// docs/spec-notes/layer5-captures.md §10.4].
pub mod menus {
    /// A message box (ShowMessage of a message with the Message Box flag).
    pub const MESSAGE: u16 = 1001;
    /// The race menu (ShowRaceMenu) [observed: layer5-captures.md §10.3].
    pub const RACE: u16 = 1036;
    /// The tag-skill menu (SetTagSkills).
    pub const TAG_SKILLS: u16 = 1048;
    /// The name menu (GetPlayerName).
    pub const NAME: u16 = 1051;
    /// The Vigor Tester's SPECIAL menu (ShowLoveTesterMenuParams).
    pub const LOVE_TESTER: u16 = 1074;
    /// The trait menu (ShowTraitMenu).
    pub const TRAITS: u16 = 1084;
    /// The level-up menu (AdvancePCLevel; `crate::levelup`) [documented: GECK wiki
    /// MenuMode, the example "if MenuMode 1027 ; level-up menu is active"; not captured].
    pub const LEVEL_UP: u16 = 1027;
    /// The lockpicking menu [documented: GECK wiki GetMenuTargetRef, owner export: "1014
    /// The door/container being picked"].
    pub const LOCKPICK: u16 = 1014;
    /// The barter menu (ShowBarterMenu) [hypothesis: the GECK wiki's MenuMode list as
    /// remembered; no shipped script reads it and the owner's export lacks the page].
    pub const BARTER: u16 = 1053;
    /// The dialogue menu [documented: GECK wiki GetMenuTargetRef, owner export; not
    /// captured]. It is not kept in `Game::menu`: the conversation itself
    /// (`Game::dialogue`) is the open menu, under any other menu that opens during it
    /// (`Game::in_menu`, `Game::open_menu_code`, `Game::menu_open`).
    pub const DIALOGUE: u16 = crate::dialogue::DIALOGUE_MENU;
    /// `MenuMode 1`: any of the Pip-Boy's menus ([`is_pipboy`]) [documented: GECK wiki
    /// "MenuMode (Function)", owner export: the example `If (MenuMode 1)` "will check to
    /// see if the Pipboy menu is open"]. A group, never a menu's own code; CGTutorial's
    /// `Begin MenuMode 1` block (the crippled-limb tutorial) runs while the Pip-Boy is
    /// open [observed in the masters].
    pub const PIPBOY: u16 = 1;
    /// The Pip-Boy's ITEMS page, the inventory [documented: GECK wiki GetSelectedItemRef,
    /// "Inventory Menu (1002)"; GetMenuItemListRefs, "1002 Pip-Boy Inventory Menu"].
    pub const INVENTORY: u16 = 1002;
    /// The Pip-Boy's STATS page [documented: GECK wiki GetSelectedItemRef, "Pipboy STATS
    /// Menu (1003) - Perk"].
    pub const STATS: u16 = 1003;
    /// A loading screen [hypothesis: the GECK wiki's MenuMode notes name 1007 among the
    /// three codes true at Fallout 3's main menu; eight DLC scripts set "from load" flags
    /// in `Begin MenuMode 1007` blocks to put shaders back on after a load, observed in
    /// the masters]. Its blocks run under the loading screen (`Game::tick`).
    pub const LOADING: u16 = 1007;
    /// The start (pause) menu [hypothesis: the same note names 1013; three DLC effect
    /// scripts test `MenuMode 1007 || MenuMode 1013` for a load, observed in the masters].
    /// Not reported: scripts do not run under NVForever's pause menu (`nvf_play::script`).
    pub const START: u16 = 1013;
    /// The Pip-Boy's DATA page (the map, quests, notes) [documented: GECK wiki
    /// GetSelectedItemRef, "Pipboy DATA Menu (1023) - Quest/Note/Challenge";
    /// GetMenuTargetRef, "1023 The Map Marker on which the mouse is over"].
    pub const MAP: u16 = 1023;
    /// The Pip-Boy's repair list (`crate::repair::REPAIR_MENU`) [documented: GECK wiki
    /// GetSelectedItemRef, "Pipboy Repair Menu (1035)"].
    pub const REPAIR: u16 = crate::repair::REPAIR_MENU;
    /// The Pip-Boy's weapon mod menu [documented: GECK wiki GetMenuTargetRef, "1061 The
    /// weapon being modded (Inventory Reference)"; docs/spec-notes/weapon-mods.md §5].
    pub const ITEM_MOD: u16 = 1061;
    /// VATS target selection [documented: GECK wiki GetMenuTargetRef, "1056 The current
    /// target in VATS target-selection"]. NVForever's VATS is a slow-time mode, not a
    /// menu that pauses the game (`crate::vats`): `MenuMode 1056` holds and the
    /// `Begin MenuMode 1056` blocks run while it marks, beside GameMode (`Game::tick`)
    /// [design].
    pub const VATS: u16 = 1056;
    /// The tutorial menu (ShowTutorialMenu, `Game::show_tutorial`) [observed: MenuMode 1059
    /// was 1, and 1001 and 1060 were 0, with it open, capture mh1_tutorial, save nvfstrip, 2026-10-18]. Not a message box.
    pub const TUTORIAL: u16 = 1059;
    /// The "How many?" prompt [documented: Hardcoded UI Shortcuts, the `QuantityMenu`
    /// class constant, `fnv_ui::value`]. The engine shows it over a container, barter or
    /// inventory menu without making it [`Game::menu`] (MenuMode does not report it); it
    /// stands in [`Game::ui_prompt`] for the tile tree (T15a).
    pub const QUANTITY: u16 = 1016;
    /// The companion wheel (`crate::companion_wheel`) [hypothesis: the code, see there].
    pub const COMPANION_WHEEL: u16 = crate::companion_wheel::MENU_CODE;

    /// The codes of the Pip-Boy's menus, which `MenuMode 1` answers for: its three pages
    /// and the repair list and mod menu it opens [hypothesis: they count as the Pip-Boy's;
    /// the wiki names the first "Pipboy Repair Menu", the mod menu's file "Pipboy mod
    /// menu"].
    pub fn is_pipboy(code: u16) -> bool {
        matches!(code, INVENTORY | STATS | MAP | REPAIR | ITEM_MOD)
    }

    /// Whether `MenuMode want` holds with the menu `open` on top: its own code, or the
    /// Pip-Boy's group.
    pub fn answers(want: u16, open: u16) -> bool {
        want == open || (want == PIPBOY && is_pipboy(open))
    }

    /// Whether two codes name the same menu (the Pip-Boy by any of its pages).
    pub fn same_menu(a: u16, b: u16) -> bool {
        a == b || (is_pipboy(a) && is_pipboy(b))
    }
}

/// What an open menu is about or has highlighted, for the extender's menu getters
/// (GetMenuTargetRef, GetSelectedItemRef; #983): an item in a container's inventory (an
/// inventory reference to scripts) or a form [documented: GetSelectedItemRef, "either a
/// temporary Inventory Reference, or a Base Form, depending on the menu type"].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPick {
    Item { owner: FormId, item: FormId },
    Form(FormId),
}

/// A menu's [`MenuPick`]s: what it is derived from and its highlighted row. The engine
/// sets them each frame ([`Game::set_menu_picks`]); not saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuPicks {
    pub target: Option<MenuPick>,
    pub selected: Option<MenuPick>,
}

/// `merchant.ShowBarterMenu [discount]`: trade with this actor. `discount` is a percentage
/// in -100 to 100 that lowers the prices the merchant sells at and raises those it buys
/// at; 0 leaves them [documented: GECK wiki ShowBarterMenu, owner export].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarterRequest {
    pub merchant: FormId,
    pub discount: i32,
}

/// A dialogue line being said outside a conversation (SayTo, Say, a greeting with the
/// Goodbye flag, `Game::start_dialogue`, or the rest of a Goodbye line the player chose,
/// `Game::choose`): docs/spec-notes/scripts.md §7.7.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Speech {
    pub speaker: FormId,
    /// The listener (SayTo's target; none for Say).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<FormId>,
    pub topic: FormId,
    pub info: FormId,
    /// Seconds of game time left until the line ends.
    pub remaining: f64,
    /// Said by SayTo (else by Say). The speaker's SayToDone blocks run when either ends
    /// [documented: level-design.md §11.2 D1].
    #[serde(default)]
    pub say_to: bool,
    /// A greeting with the Goodbye flag, said at the player without a conversation
    /// (`Game::start_dialogue`, docs/spec-notes/dialogue.md §5): when it ends its Add
    /// Topics are applied and no SayToDone block runs [hypothesis for both].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub greeting: bool,
    /// A Goodbye line the player chose, said at the player after its conversation closed
    /// at once (`Game::choose`, docs/spec-notes/dialogue.md §5) [observed: owner's
    /// playtest 2026-11-05, against the original: choosing a Goodbye closes the menu at
    /// once and the actor finishes the line as the player walks away]. It ends as a
    /// Goodbye greeting does: its end script and Add Topics, no SayToDone block; it is
    /// never cut (`Game::cut_line`); a `StartConversation player` in its end script starts
    /// after its Add Topics (`Game::hold_force_greet`). Unlike a greeting it does not
    /// hold its speaker in place (`Game::greets_player`) [design].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub farewell: bool,
    /// A line of a conversation between two actors (StartConversation with an actor as
    /// the target, `crate::dialogue`, docs/spec-notes/dialogue.md §5.1): its number in
    /// the conversation, from 1. Only its first response is said; when it ends the
    /// conversation goes on with the next line, and no SayToDone block runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<u32>,
    /// The topics of the conversation's earlier lines, in order (for the quests without
    /// "Allow Repeated Conversation Topics", `crate::dialogue`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub earlier_topics: Vec<FormId>,
    /// A hello said to the player as the player passes (`Game::say_hello`,
    /// docs/spec-notes/dialogue.md §5.2): when it ends no SayToDone block runs and no
    /// topic is added [hypothesis for both].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hello: bool,
    /// Who the line turns to whom ([`Game::facings`]).
    #[serde(default, skip_serializing_if = "LineLook::is_both")]
    pub look: LineLook,
    /// Position (in the order the responses are said) of the response whose idles were
    /// last asked for (`crate::idles`); not part of the saved state.
    #[serde(skip)]
    pub response: usize,
    /// Say's ForceSubtitles: the subtitle shows whatever the setting and the distance.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force_subtitles: bool,
    /// Say's is2D: heard as a 2D sound (full volume, not panned, anywhere).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub two_d: bool,
    /// The line's end result script already ran as it started (a companion wheel command,
    /// `Game::wheel_command`): it does not run again when the line ends.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub end_ran: bool,
}

/// Who a line said to someone ([`Speech::target`]) turns to whom ([`Game::facings`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LineLook {
    /// The speaker turns to the listener and the listener looks at the speaker.
    #[default]
    Both,
    /// The listener is not made to look at the speaker: SayTo's `noTargetLookFlag`
    /// [documented: GECK wiki SayTo, owner export: with the flag set "Joe is not forced
    /// to look at the speaker during the SayTo"].
    Speaker,
    /// Nobody turns: a conversation between actors started with `headTrack` 0
    /// [documented: GECK wiki StartConversation: "Enter 0 to disable" head tracking
    /// during the conversation].
    Nobody,
}

impl LineLook {
    pub fn is_both(&self) -> bool {
        *self == LineLook::Both
    }
}

impl Speech {
    /// A Goodbye line said at the player outside a conversation: a greeting
    /// ([`Speech::greeting`]) or a farewell ([`Speech::farewell`]). It is never cut
    /// (`Game::cut_line`).
    pub fn goodbye_at_player(&self) -> bool {
        (self.greeting || self.farewell) && self.target == Some(FormId::PLAYER_REF)
    }
}

/// One actor turning to another while a line is said ([`Game::facings`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facing {
    /// Who turns, and to whom (either may be the player).
    pub who: FormId,
    pub to: FormId,
    /// The body turns too, where it stands free (else the head only).
    pub body: bool,
    /// `who` says the line (else it listens).
    pub speaking: bool,
    /// The line (INFO).
    pub info: FormId,
}

/// What a line said outside a conversation with the player is ([`Speech`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Say,
    SayTo,
    /// A greeting with the Goodbye flag ([`Speech::greeting`]).
    Greeting,
    /// Line `n` of a conversation between two actors ([`Speech::conversation`]).
    Conversation(u32),
    /// A hello to the passing player ([`Speech::hello`]).
    Hello,
}

/// See [`Game::ui_dialogue_hook`].
pub type DialogueHook = Box<dyn for<'g> FnMut(&mut Game<'g>) + Send + Sync>;

pub struct Game<'a> {
    pub data: &'a GameData,
    /// The script extender's strings and arrays by id (fnv_script::vm::ExtStore;
    /// docs/spec-notes/ext-values.md), saved with the game (`GameSnapshot::ext`).
    pub ext: fnv_script::vm::ExtStore,
    /// The user-defined functions running, innermost last (script-compiler.md §9).
    udf: Vec<UdfFrame>,
    /// The lambdas scripts made: the unit, the lambda, the instance whose variables it
    /// shares; named by `LAMBDA_BASE + index`.
    lambdas: Vec<LambdaDef<'a>>,
    /// CompileScript's scripts, named by `COMPILED_BASE + index`, and their files' paths
    /// (lower case) to them.
    compiled_files: Vec<&'a Compiled>,
    compiled_paths: std::collections::HashMap<String, u32>,
    /// JIP's Script Runner: its files and their units (`crate::script_runner`).
    pub(crate) sr: crate::script_runner::SrState<'a>,
    /// A lambda already made, by its unit, its number in the unit and whose variables it
    /// shares ([`LambdaKey`]).
    lambda_ids: std::collections::HashMap<LambdaKey, u32>,
    /// The extender's event handlers (tasks E1–E3); not saved [documented: SetEventHandler
    /// lasts for the session; SetEventHandlerAlt is flushed on every load].
    pub(crate) handlers: Vec<Handler>,
    handler_seq: u64,
    /// JIP LN's SetOn…EventHandler and main-loop registrations (task J6); not saved.
    pub(crate) jip: crate::jip_events::JipEvents,
    /// The temporary inventory references foreach made this frame (task B5): container
    /// and item, id [`TEMP_REF_BASE`] + index; gone at the next frame [observed: probe
    /// loops/02, valid after the loop, invalid a frame later].
    pub(crate) temp_refs: Vec<(FormId, FormId)>,
    /// Form lists as ListAddForm and the ListRemove… functions left them, for the
    /// session only [documented: ListAddForm, its changes are not saved].
    pub(crate) list_edits: std::collections::HashMap<FormId, Vec<FormId>>,
    /// xNVSE's queued delayed calls (`crate::delayed`); not saved [documented].
    pub(crate) delayed: Vec<crate::delayed::Delayed>,
    /// The input the extender's input functions read (task D6); the engine fills it.
    pub input: crate::script_input::ScriptInput,
    /// Record fields xNVSE's setters changed this session (`crate::nvse`), by form and
    /// "SUB/path", and the names SetName gave.
    pub(crate) field_overrides: std::collections::HashMap<(FormId, String), fnv_script::vm::XVal>,
    pub(crate) name_overrides: std::collections::HashMap<FormId, String>,
    /// xNVSE's mod local data: lower-cased key -> (key as given, value); the session's.
    pub(crate) mod_data: BTreeMap<String, (String, fnv_script::vm::XVal)>,
    /// Notes the extender's setters changed this session (`crate::nvse_notes`), whole.
    pub(crate) note_edits: BTreeMap<FormId, crate::data::NoteDef>,
    /// The holotape playing and the line it started with (`Game::holotape_script`): its
    /// end script is that line's, whatever the start script changed of the note.
    pub(crate) tape_playing: Option<(FormId, FormId)>,
    /// SetNoteRead's changes without `serialize`: for the session, over the saved flag.
    pub(crate) note_read_session: BTreeMap<FormId, bool>,
    /// What each open menu is about and has highlighted, by menu code ([`MenuPicks`]).
    pub(crate) menu_picks: BTreeMap<u16, MenuPicks>,
    /// NX NVSE's variables on references (`crate::nvse_nx`), saved.
    pub(crate) nx: crate::nvse_nx::NxVars,
    /// TempCloneForm's clones and their originals (`crate::nvse_data`).
    pub(crate) clones: std::collections::HashMap<FormId, FormId>,
    pub(crate) clone_seq: u32,
    /// Lines for the console: what scripts print (PrintC, Print …) and script errors;
    /// the engine's console takes them each frame.
    pub console_out: Vec<String>,
    /// SetNthEffectTraitNumeric's changes: (form, entry, trait) -> value, the session's.
    pub(crate) effect_overrides: std::collections::HashMap<(FormId, usize, usize), f64>,
    /// JIP switches kept for the session (ToggleTeammateKillable, SetWheelDisabled,
    /// ToggleNoHealthReset).
    pub(crate) jip_flags: std::collections::HashSet<(FormId, &'static str)>,
    /// Packages a script changed (SetPackageLocationRadius, SetPackageLocationReference,
    /// SetPackageTargetReference): the changed copy, kept for the session (leaked, so it
    /// lives as long as the records; a few per session).
    pub(crate) package_overrides: std::collections::HashMap<FormId, &'a crate::package::PackageDef>,
    /// The N7–N9 functions' session state and what the engine tells them
    /// (`crate::nvse_jip2`).
    pub jip2: crate::nvse_jip2::Jip2State,
    /// The most called functions no earlier set built (`crate::nvse_gaps`).
    pub gaps: crate::nvse_gaps::GapsState,
    /// The HUD reads and the scripted weapon draws (`crate::nvse_hud`).
    pub hud: crate::nvse_hud::HudState,
    /// ShowOff's events and timer stop handlers (`crate::nvse_showoff`).
    pub showoff: crate::nvse_showoff::ShowOffState,
    /// Mods' INI and other files (`crate::mod_files`).
    pub mod_files: crate::mod_files::ModFiles,
    /// Sound, weather and image-space traits scripts changed (`crate::nvse_traits`).
    pub traits: crate::nvse_traits::TraitState,
    /// kNVSE's animation paths and script-played sequences (`crate::nvse_anim`).
    pub anim_paths: crate::nvse_anim::AnimPaths,
    /// kNVSE's AnimGroupOverride folders (`crate::nvse_anim_folders`).
    pub anim_folders: crate::nvse_anim_folders::AnimFolders,
    /// kNVSE's animation clock, weights, text keys and blends (`crate::nvse_knvse`).
    pub knvse: crate::nvse_knvse::KnvseState,
    /// JIP PP LN's runtime node lists and ground requests (`crate::nvse_jippp`).
    pub jippp: crate::nvse_jippp::JipppState,
    /// vheyes-nvse's moon, stamps and 3D rebuilds (`crate::nvse_vheyes`).
    pub vheyes: crate::nvse_vheyes::VheyesState,
    /// The small plugins' and CacheUI's state (`crate::nvse_small`).
    pub small: crate::nvse_small::SmallState,
    /// The node trees scripts asked about, as the engine tells them (`crate::nif_blocks`).
    pub scene: crate::nif_blocks::SceneMirror,
    /// The xNVSE "other" functions' and the control locks' session state.
    pub other: crate::nvse_other::OtherState,
    /// Scripts' edits to topics and lines for the session (`crate::dialogue_edits`).
    pub dialogue_edits: crate::dialogue_edits::DialogueEdits,
    /// The 1111 NVSE plugin's condition lists, weapon stat vectors and damage event
    /// (`crate::nvse_one111`).
    pub one111: crate::nvse_one111::One111State,
    /// The INI settings the extender reads and sets (`crate::nvse_settings`).
    pub ini: crate::nvse_settings::IniState,
    /// ppNVSE's AX variables, form instances, item-mod slots and call loops
    /// (`crate::nvse_pp`).
    pub pp: crate::nvse_pp::PpState,
    /// Whether the last [`Game::run_function`] call's function set its value
    /// (SetFunctionValue), for callers that tell "set to 0" from "not set" (CallLoop).
    pub(crate) udf_value_set: bool,
    /// MessageBoxExAlt's callback, run with the pressed button (`Game::tick`).
    pub(crate) box_callback: Option<fnv_script::vm::XVal>,
    /// The rounds in the player's magazine, as the engine's combat has them (None without
    /// the engine).
    pub player_magazine: Option<u32>,
    /// SetIsPlayable's changes, for the session.
    pub(crate) playable_overrides: std::collections::HashMap<FormId, bool>,
    /// What GetNextRef gives next (GetFirstRef's walk).
    pub(crate) ref_walk: Vec<FormId>,
    /// Plugins whose debug mode is on (SetDebugMode, DebugPrint); not saved
    /// [hypothesis: the page gives no lifetime].
    debug_plugins: BTreeSet<String>,
    /// The events being handled, innermost last (GetCurrentEventName).
    event_names: Vec<String>,
    /// The scripts that saw GetGameLoaded / GetGameRestarted true (by unit key).
    loaded_seen: BTreeSet<String>,
    restarted_seen: BTreeSet<String>,
    /// The console's world toggles (`functions::toggles`).
    pub toggles: crate::functions::toggles::Toggles,
    /// The menus' tile tree (`fnv_ui`, docs/design/ui-tile-tree.md; `ui` above is the requests to the engine's own menus): the engine loads
    /// and draws the menus, the UI script functions (`crate::ui_functions`) read and
    /// change it. Not saved: menus are built again after a load.
    pub ui_tree: fnv_ui::Ui,
    /// Menu names to files, rebuilt with the tree [design: ui-tile-tree.md §3.2, §9].
    pub ui_menus: fnv_ui::MenuIndex,
    /// Menu lifetime's once-only warnings, reset with the tree [design].
    pub ui_lifetime: crate::ui_loading::MenuLifetime,
    /// Conversations started in this run (`crate::dialogue::conversation_stamp`).
    pub dialogue_serial: u32,
    /// Told at each transition of the conversation, before any script of it runs
    /// ([`Game::dialogue_transition`]): for what shows the conversation to what scripts read.
    pub ui_dialogue_hook: Option<DialogueHook>,
    /// Where InjectUIXML and the menus' includes read files: the engine's archives and
    /// loose files. `None` headless: InjectUIXML then fails, InjectUIComponent works
    /// without includes.
    pub ui_files: Option<Box<dyn fnv_ui::Source + Send + Sync>>,
    /// The menu an engine page stands for while [`Game::page_tick`] runs its blocks.
    page_menu: Option<u16>,
    /// This frame's HUD writes, kept back while nothing reads the HUD's traits
    /// (`crate::ui_hud::defer`); the first UI function call makes them
    /// (`crate::ui_hud::flush`).
    pub ui_hud_pending: Option<Box<crate::ui_hud::Pending>>,
    /// A script called a UI function that reads or changes the tree: the engine writes
    /// the HUD every frame from then on.
    pub ui_used: bool,
    /// Keys MenuTapKey pressed (DirectInput scan codes), for the engine to take as one
    /// frame's presses in the open menu.
    pub ui_taps: Vec<u32>,
    /// The trait changes over real time (SetUIFloatGradual, CacheUI's gradual and reveal
    /// functions): the engine steps them each frame (`ui_gradients::Gradients::step`).
    pub ui_gradients: crate::ui_gradients::Gradients,
    /// The other tile functions' session state (`crate::ui_more`): CacheUI's caches and
    /// queues, SUP's HUD bars, the attached components, the text measure.
    pub ui_extra: crate::ui_more::UiExtra,
    /// The pointer on the menus drawn from tiles: the tile with `mouseover`, the `clicked`
    /// pulses to end (`crate::ui_input`, `fnv_ui::input`). Reset with the tree.
    pub ui_pointer: fnv_ui::input::Pointer,
    /// The game's difficulty (`crate::difficulty`): the engine sets it from its settings.
    pub difficulty: crate::difficulty::Difficulty,
    /// What scripts asked of the engine's combat (`crate::combat::CombatRequest`), for
    /// the engine to take.
    pub combat_requests: Vec<crate::combat::CombatRequest>,
    /// Destruction stage changes made by scripts (`DamageObject`), with their
    /// references, for the engine to set off as it does a hit's (explosions, model).
    /// Not part of the saved state.
    pub destruction_changes: Vec<(FormId, crate::combat::DestructionChange)>,
    /// Activate children waiting for their delay (`crate::traps`). Saved.
    pub due_children: Vec<crate::traps::DueChild>,
    /// The swinging traps let go and whom they hit (`crate::swing_traps`). Not saved.
    pub swing_traps: crate::swing_traps::SwingTraps,
    /// Knockouts and getting up since the engine last took them
    /// (`crate::unconscious::News`): actor, cause, `true` when it got up.
    pub knockout_news: Vec<crate::unconscious::News>,
    /// What the magic effect archetypes did since the engine last took it
    /// (`crate::magic_archetypes::MagicNews`). Not saved.
    pub magic_news: Vec<crate::magic_archetypes::MagicNews>,
    /// The idle animations actors were asked to play (`crate::idles::IdleRequest`), for
    /// the layer that animates actors to take.
    pub idle_requests: Vec<crate::idles::IdleRequest>,
    /// Actors a script's `PickIdle` asked to choose an idle from the idle tree
    /// (`crate::idles::pick_idle`), for the layer that animates actors to take: it knows
    /// their animation folder. Each actor once. Not part of the saved state.
    pub idle_picks: Vec<FormId>,
    /// Actors' weapon uses (`crate::use_weapon`: UseWeapon, Use Weapon packages), and
    /// the last number given to one. Not part of the saved state.
    pub weapon_uses: BTreeMap<FormId, crate::use_weapon::WeaponUse>,
    pub weapon_use_seq: u64,
    /// Speakers a script's StartConversation sent to walk up to the player before they
    /// talk, with the topic to start with ([`Game::walk_up_greet`]), for the AI to carry
    /// out (`fnv_ai`). Not part of the saved state.
    pub walk_up_greets: BTreeMap<FormId, Option<FormId>>,
    /// The sounds scripts asked for (`crate::sound::SoundRequest`), for the engine to
    /// take. Not part of the saved state.
    pub sound_requests: Vec<crate::sound::SoundRequest>,
    /// The music types scripts asked for (`PlayMusic`, `crate::sound`), for the engine to
    /// take. Not part of the saved state.
    pub music_requests: Vec<FormId>,
    /// The textures scripts put on references' shapes (`SwapTexture`,
    /// `crate::functions::misc::swap_texture`), for the engine to draw. Not part of the
    /// saved state.
    pub texture_swaps: crate::functions::misc::TextureSwaps,
    /// Effect shaders and actor alpha scripts asked for (`crate::functions::misc::ScriptLooks`),
    /// for the engine to draw. Not part of the saved state.
    pub script_looks: crate::functions::misc::ScriptLooks,
    /// Animation groups scripts asked objects to play, and what the engine reports they
    /// play (`crate::anims`). Not part of the saved state.
    pub anims: crate::anims::ObjectAnims,
    /// Barter menus scripts opened (ShowBarterMenu), for the engine to draw; the menu
    /// is open for scripts ([`menus::BARTER`]) until the engine closes it. Not part of
    /// the saved state.
    pub barter_requests: Vec<BarterRequest>,
    /// Recipe menus scripts opened (ShowRecipeMenu, `crate::crafting`), for the engine to
    /// draw; the menu is open for scripts ([`crate::crafting::RECIPE_MENU`]) until the
    /// engine closes it. Not part of the saved state.
    pub recipe_requests: Vec<crate::crafting::RecipeRequest>,
    /// Repair services menus scripts opened (ShowRepairMenu, `crate::repair`), for the
    /// engine to draw; the menu is open for scripts
    /// ([`crate::repair::REPAIR_SERVICES_MENU`]) until the engine closes it. Not part of
    /// the saved state.
    pub repair_requests: Vec<crate::repair::RepairRequest>,
    /// Roulette menus asked for by ShowRouletteMenuParams (`crate::roulette`); the engine
    /// opens them.
    pub roulette_requests: Vec<crate::roulette::RouletteRequest>,
    /// Slot machine menus asked for by ShowSlotMachineMenuParams (`crate::slots`); the
    /// engine opens them.
    pub slots_requests: Vec<crate::slots::SlotsRequest>,
    /// Blackjack menus asked for by ShowBlackJackMenuParams (`crate::blackjack`); the
    /// engine opens them.
    pub blackjack_requests: Vec<crate::blackjack::BlackjackRequest>,
    /// Caravan games asked for by ShowCaravanMenu (`crate::caravan`); the engine opens them.
    pub caravan_requests: Vec<crate::caravan::CaravanRequest>,
    /// Teammates' inventories scripts opened (OpenTeammateContainer,
    /// `crate::followers`), for the engine to show as a container menu. Not part of the
    /// saved state.
    pub teammate_containers: Vec<FormId>,
    /// The companion wheel's state: the open wheel, back-up orders, who sneaks with the
    /// player (`crate::companion_wheel`). Not saved.
    pub companions: crate::companion_wheel::Companions,
    /// NPCs using the stimpaks they carry (`crate::npc_stimpaks`). Not saved.
    pub npc_stimpaks: crate::npc_stimpaks::NpcStimpaks,
    /// NPCs taking up weapons again (`crate::rearm`). Not saved.
    pub rearms: crate::rearm::Rearms,
    /// Teammates brought to the player when the player changed cells
    /// (`crate::followers`), for the engine's log; they are in [`Game::teleported`] too.
    /// Not part of the saved state.
    pub brought_along: BTreeSet<FormId>,
    /// Doors locked or unlocked by packages' door flags (`crate::ai`), for the engine's
    /// log: actor, package, locked (else unlocked), how many. Not part of the saved state.
    pub package_door_notes: Vec<(FormId, FormId, bool, usize)>,
    /// Sleeping and waiting as scripts see and ask for them (`crate::rest`); the menu is
    /// the engine's. Not part of the saved state.
    pub rest: crate::rest::Rest,
    /// The latest detection value of each (detector, target) pair the engine measured
    /// (`crate::detection`); GetDetected and GetDetectionLevel read it. Not part of the
    /// saved state: the engine measures again.
    pub detection: crate::detection::Detections,
    /// What a detection round has worked out so far ([`Game::begin_detection_round`]).
    pub(crate) detection_round: Option<crate::detection::Round>,
    /// Factions kept for a frame of the combat AI ([`Game::begin_faction_frame`]).
    pub(crate) faction_frame: Option<crate::functions::actor::FactionKeep>,
    /// Every actor's detection state (Normal, Alert, Combat, Lost), its alert and target
    /// lists, and detection events waiting to be heard (`crate::awareness`). The actors'
    /// states and lists and StartCombat's pairs are saved (a fight survives a save and
    /// load, #971); the rest is not: the engine measures again.
    pub awareness: crate::awareness::Awarenesses,
    /// Fights started by being hit, friendly hits, yields and allies joining
    /// (`crate::provocation`). Not part of the saved state.
    pub provocations: crate::provocation::Provocations,
    /// Navmeshes the scripts enabled or disabled, and setAllReachable
    /// (`functions::navigation`). Saved.
    pub nav_switches: crate::functions::navigation::NavSwitches,
    /// SetAllVisible and SetNoAvoidance: this frame's word, cleared at each
    /// [`Game::tick`] (`functions::navigation::FrameSwitches`). Not part of the saved
    /// state: the original clears them every frame.
    pub frame_switches: crate::functions::navigation::FrameSwitches,
    /// Combat-tab lines: attack timers, health marks, DeathResponse lines due
    /// (`crate::combat_lines`). Not part of the saved state.
    pub combat_lines: crate::combat_lines::CombatLines,
    /// What switched radio receivers on and off (`crate::radio`); the engine plays the
    /// stations. Not part of the saved state.
    pub radio: crate::radio::Radios,
    /// Terminals hacked or locked out (`crate::terminal`).
    pub terminals: crate::terminal::Terminals,
    /// Locks broken by forcing them (`crate::lockpick`). Not part of the saved state yet.
    pub locks: crate::lockpick::Locks,
    /// What the engine tells the scripts of the actors' movement, sneaking and sight and
    /// of the camera (`functions::senses`). Not part of the saved state: the engine
    /// measures again.
    pub senses: crate::functions::senses::Senses,
    /// VATS mode's state and the marked shot leaving now, as the engine says
    /// (`crate::vats`). Not part of the saved state: the mode is never saved.
    pub vats: crate::vats::VatsState,
    /// The weather scripts forced or set, and the one the engine shows
    /// (`crate::weather`). Not part of the saved state yet.
    pub weather: crate::weather::WeatherState,
    /// What scripts asked of the Pip-Boy and the character-creation menus, and to end
    /// the game (`functions::ui`), for the engine to take. Not part of the saved state.
    pub ui: crate::functions::ui::UiRequests,
    /// What console commands asked of the engine (`functions::console`), for it to take.
    pub console_requests: Vec<crate::functions::console::ConsoleRequest>,
    /// The faces `MFG` set, by actor (`functions::console::FaceMfg`), for the engine's
    /// faces to show. Not part of the saved state [design: a console aid].
    pub mfg: std::collections::BTreeMap<FormId, crate::functions::console::FaceMfg>,
    /// Crimes being reported, trespass warnings and karma changes for the engine
    /// (`crate::crime`); what lasts is in [`PlayerState`]. Not part of the saved state.
    pub crimes: crate::crime::Crimes,
    /// Pickpocketing: who caught the player, planted explosives (`crate::pickpocket`).
    pub pickpocket: crate::pickpocket::State,
    pub world: World<'a>,
    pub cfg: GameConfig,
    pub quests: BTreeMap<FormId, QuestState>,
    pub instances: BTreeMap<InstKey, Instance>,
    pub extra: BTreeMap<FormId, RefExtra>,
    pub player: PlayerState,
    /// Players 2–4 of a co-op game, in slot order (`crate::coop`; docs/design/coop-save.md).
    /// Kept and saved; nothing plays them yet.
    pub coop: Vec<crate::coop::CoopPlayer>,
    /// Forms added to form lists at run time.
    pub form_lists: BTreeMap<FormId, Vec<FormId>>,
    pub loaded_cells: BTreeSet<FormId>,
    /// Scripted references a script moved (they may have left their authored cell).
    pub moved: BTreeSet<FormId>,
    /// References other than the player that a script put somewhere (MoveTo) since the
    /// layer that moves actors last took them ([`Game::take_teleported`]): it has to
    /// start them again from where they are now. Not saved.
    pub teleported: BTreeSet<FormId>,
    /// Rotate: this frame's turn per reference (axis 0–2, degrees per second); the last
    /// call of the frame counts. Applied at the end of [`Game::tick`]. Not saved.
    pub rotations: BTreeMap<FormId, (usize, f64)>,
    /// References Rotate turned or SetScale resized in the latest [`Game::tick`], for the
    /// engine to draw at their new angle and size. Not saved.
    pub turned: BTreeSet<FormId>,
    /// Actors whose OnStartCombat block was fired and whose OnCombatEnd was not yet
    /// (`Game::combat_blocks`). Saved with the combat states, so that a load runs no block
    /// for a fight already going [documented: GECK wiki event handler table, OnStartCombat
    /// does not run again on loading a save with combat already going].
    pub fighting: BTreeSet<FormId>,
    /// The target of the condition list being evaluated ([`Game::conditions_pass`]), for
    /// the functions that ask for it (Exists). Not saved.
    pub condition_target: Option<FormId>,
    /// Cell resets since the engine last took them ([`Game::take_cell_resets`],
    /// `crate::respawn`). Not saved.
    pub cell_resets: Vec<crate::respawn::CellReset>,
    /// Zone levels set and leveled actors fixed, not yet taken (`crate::zones`).
    pub(crate) level_news: Vec<crate::zones::LevelNews>,
    /// Actors ResurrectActor brought back since the engine last took them
    /// ([`Game::take_resurrections`], `crate::resurrect`). Not saved.
    pub resurrections: Vec<crate::resurrect::Resurrection>,
    /// Actors whose race or sex changed since the engine last took them
    /// ([`Game::take_appearance_changes`], `crate::appearance`). Not saved.
    pub appearance_changes: Vec<crate::appearance::AppearanceChange>,
    /// The perks' choice list open for an activation (`crate::activate_choices`). Not
    /// saved.
    pub activate_menu: Option<crate::activate_choices::ActivateMenu>,
    /// Cannibal's and Mister Sandman's acts under way (GetCannibal, GetSandman). Not
    /// saved.
    pub perk_acts: Vec<crate::activate_choices::PerkAct>,
    /// Challenge rewards and notices waiting for the next frame (`crate::challenges`).
    /// Not saved.
    pub challenge_queue: crate::challenges::Queue,
    /// Notes the player was given since the engine last took them (its corner
    /// messages, [`Game::take_note_notices`]). Not saved.
    pub note_notices: Vec<FormId>,
    /// The open menu, by menu code. While a menu is open, [`Game::tick`] runs MenuMode
    /// blocks instead of GameMode, and game time stops for scripts (GetSecondsPassed and
    /// the quest delay of GameMode) [observed: layer5-captures.md §8.3 e]. Part of the
    /// engine API, docs/spec-notes/scripts.md §7.2.
    pub menu: Option<u16>,
    /// Which function opened the race menu (1036): it is one menu for the race menu, the
    /// barber and the plastic surgeon, which change different parts of the look
    /// (docs/spec-notes/character-creator.md §10). Not saved.
    pub race_menu: RaceMenu,
    /// The menu under the one open, when the tutorial menu opened over it (the Pip-Boy
    /// under CGTutorial's crippled-limb tutorial): it is the open menu again once the
    /// tutorial closes [hypothesis]. Saved.
    pub menu_under: Option<u16>,
    /// A menu the engine shows over the open one without it being [`Game::menu`] (the
    /// "How many?" prompt, [`menus::QUANTITY`]): the tile tree builds its file while set
    /// (`crate::ui_loading::sync`), so it can be drawn from it (task T15a) [design:
    /// MenuMode does not report it; whether the original's MenuMode 1016 holds over a
    /// container is not captured]. Set by the engine each frame. Not saved.
    pub ui_prompt: Option<u16>,
    /// The messages of the open tutorial menu (ShowTutorialMenu), its pages in the order
    /// shown [hypothesis: further calls while it is open add pages]. Saved.
    pub tutorial: Vec<FormId>,
    /// A loading screen is up: [`Game::tick`] runs no script and game time stops
    /// [observed: GetSecondsPassed did not count a `coc` loading screen,
    /// layer5-captures.md §8.3 e]. Part of the engine API, scripts.md §7.2.
    pub loading: bool,
    pub message_box: Option<MessageBox>,
    /// Message boxes shown and not yet closed, in the order they were shown; the first
    /// is on screen (menu [`menus::MESSAGE`]), the others wait for it to close
    /// [hypothesis: one box at a time, the next when the open one closes]. `FormId(0)`:
    /// a box whose message is not known (a scenario's `given`).
    pub boxes: Vec<FormId>,
    /// What each box in [`Game::boxes`] was when shown, in step with it (`queued[i]` is
    /// `boxes[i]`'s): how many buttons it declared (a form-less box, MessageBoxEx, every
    /// one `FormId(0)`, has no Message form to count them from, and [`Game::message_box`]
    /// is the last box shown, not the one on screen) and which showing it is. Kept in the
    /// snapshot. Out of step (code that changed `boxes` alone) it is not read
    /// ([`Game::front_box`]) until the queue next changes, which puts it back in step
    /// with every count unknown ([`Game::align_queued`]).
    pub queued: Vec<QueuedBox>,
    /// The last [`QueuedBox::serial`] given.
    pub box_serial: u64,
    /// Dialogue lines being said (SayTo, Say), one per speaker.
    pub speech: Vec<Speech>,
    /// SayTo calls that found no line the speaker could say, (speaker, topic): their
    /// SayToDone blocks run in the next frame ([`Game::say`]); saved, as a save lands
    /// between frames (`GameSnapshot::say_to_unsaid`).
    pub say_to_unsaid: Vec<(FormId, FormId)>,
    /// Actors a script's StartConversation with allowMovement 1 lets go on moving while
    /// they talk to each other ([`Game::conversation_moves`]); not saved.
    pub moving_talkers: BTreeSet<FormId>,
    /// (speaker, line) pairs of the Say Once lines already said, and of every line said
    /// in a conversation with the player (the shading of choices).
    pub said: BTreeSet<(FormId, FormId)>,
    /// Say Once a Day lines: speaker → line → the game clock in hours when it was last
    /// said (`crate::dialogue`).
    pub said_day: BTreeMap<FormId, BTreeMap<FormId, f64>>,
    /// The conversation between the player and an actor, if one is running
    /// (`crate::dialogue`).
    pub dialogue: Option<crate::dialogue::Conversation>,
    /// While a conversation through a talking activator is being opened: the actor and
    /// the activator (IsActorTalkingThroughActivator, `crate::talking_activators`). Not
    /// saved.
    pub talking_through: Option<(FormId, FormId)>,
    /// A farewell's end result script is running ([`Speech::farewell`], `Game::end_line`).
    /// Not saved: a save never lands inside a script.
    pub(crate) farewell_ending: bool,
    /// A force greet (`StartConversation player`) asked for by that end script, as
    /// (speaker, topic): it starts after the line's Add Topics
    /// ([`Game::hold_force_greet`]). Both are set and put back by each `end_line`, so
    /// a line ending inside another's end script keeps to its own. Not saved.
    pub(crate) farewell_then: Option<(FormId, Option<FormId>)>,
    /// The last Goodbye line the player chose that went on in the world, as (speaker,
    /// line), set as its conversation closes (`Game::say_farewell`). nvf-play's log
    /// takes it when it logs the matching goodbye `dialogue_ended` (same actor). A
    /// headless run leaves it set, to no effect on the game. It is not cleared when a
    /// conversation starts or ends, on purpose: another change in the same frame would
    /// erase it before the frame's events are logged. Not saved.
    pub farewell_said: Option<(FormId, FormId)>,
    /// The lengths of voice files, when the game was given them (`crate::voice`): a
    /// response then lasts as long as its sound. Not part of the saved state.
    pub voice: Option<std::sync::Arc<dyn crate::voice::VoiceLengths>>,
    pub frame: u64,
    /// Game time in seconds: the frame steps that were not in a menu or a loading
    /// screen. GetSecondsPassed reads it.
    pub time: f64,
    /// References enabled since the last frame whose OnLoad runs at the start of the
    /// next one [observed: OnLoad after Enable had not run on the line after Enable and
    /// had run by the next frame, layer5-captures.md §8.3 d].
    pub pending_onload: Vec<FormId>,
    /// Scripted doors' last answers to the NPCs walking through them, by (actor, door)
    /// (`crate::npc_doors`). Not part of the saved state.
    pub door_gates: std::collections::HashMap<(FormId, FormId), crate::npc_doors::Gate>,
    /// The door block being run for an NPC walking through it (`Game::npc_door`).
    pub npc_door_run: Option<crate::npc_doors::DoorRun>,
    /// Reference events the engine reported outside a script run (a container menu
    /// opened or closed: OnOpen, OnClose), run at the start of the next frame with the
    /// world in place ([`Game::queue_ref_event`]).
    pub queued_ref_events: Vec<(FormId, u16, Option<FormId>)>,
    /// Each activated reference's action reference: who activated it last
    /// ([`Game::activate`]). A bare `Activate` in the reference's own GameMode uses it
    /// (`functions::refs::activate`). Not part of the saved state.
    pub action_refs: std::collections::HashMap<FormId, FormId>,
    /// Deaths during a script run (KillActor, a script's damage) whose OnDeath blocks wait
    /// for the outermost script to finish, with their killers ([`Game::kill`]).
    deferred_deaths: Vec<(FormId, Option<FormId>)>,
    /// The actors a hit is being applied to, from its effects until it has read their
    /// Health back after its script events ([`Game::health_lowered`]). Not part of the
    /// saved state.
    pub(crate) hits_running: Vec<FormId>,
    /// The open flag last seen on each scripted door with OnOpen or OnClose blocks
    /// ([`Game::watch_doors`]). Saved only where the door changed since it was seen:
    /// a door first seen after a load is taken as it stands.
    door_open_seen: BTreeMap<FormId, bool>,
    pub events: Vec<Event>,
    /// What this run reached of the scripts, when recorded (`crate::coverage`).
    pub coverage: Option<crate::coverage::Coverage>,
    /// Calls per opcode that have no implementation.
    pub unhandled: BTreeMap<u16, u64>,
    /// Calls per opcode that reached a Layer 6 stub.
    pub stubbed: BTreeMap<u16, u64>,
    unhandled_seen: BTreeSet<(u16, String)>,
    arg_unread_seen: BTreeSet<(u16, String, usize)>,
    stubs_seen: BTreeSet<(u16, String, String)>,
    pub rng: ChaCha8Rng,
    /// The random numbers of the idle tree's talking gestures (`crate::idles`), kept
    /// apart so that picking them does not move the scripts' `rng`. Not saved.
    pub(crate) idle_rng: ChaCha8Rng,
    /// Run-time changes to base forms and cells.
    pub forms: FormChanges,
    /// Magic effects active on actors, in the order they started.
    pub effects: Vec<ActiveEffect>,
    pub next_effect: u32,
    /// Bringing an actor's abilities and worn enchantments up to date
    /// (`crate::abilities`): not again inside. Not saved.
    pub(crate) syncing_effects: bool,
    /// When the abilities' and worn enchantments' conditions are checked next. Not saved.
    pub(crate) condition_check_due: f64,
    /// Image space modifiers on the screen, in the order they were applied
    /// (ApplyImageSpaceModifier). An animatable one ends by itself when its duration has
    /// passed; the others stay until removed. Their time passes with the frames outside
    /// menus and loading screens, conversations included [hypothesis: as the lines of a
    /// conversation do; image-modifiers.md §3].
    pub image_modifiers: Vec<ActiveImageModifier>,
    /// ScriptEffectElapsedSeconds inside a ScriptEffectUpdate block (0 elsewhere).
    pub effect_elapsed: f64,
    pub(crate) depth: u32,
    /// A function asked the running script to stop as `Return` does (RemoveMe).
    stop_run: bool,
    /// The (script record, opcode) pairs whose call on an empty reference was already
    /// traced: a record's script runs every frame, so the failure is traced the first
    /// time only (`Exec::call`). Not part of the saved state.
    empty_ref_traced: BTreeSet<(u32, u16)>,
    /// Game time of the next scan for actors whose package choice is due (`crate::ai`).
    pub(crate) package_scan_due: f64,
    /// Actors choosing their package now (`Game::select_package`): a condition of one
    /// of their packages that asks for the current package gets the one they had
    /// rather than starting the choice again (`Game::current_package`). Not saved.
    pub(crate) choosing_package: Vec<FormId>,
}

/// A lambda in a save: the unit it is written in (by key), the lambda, the instance whose
/// variables it shares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedLambda {
    pub unit: String,
    pub lambda: fnv_script::ir::XExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inst: Option<InstKey>,
}

/// A message box in [`Game::boxes`], as it was shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedBox {
    /// The buttons it declared, whatever their conditions; `None` when not known (a
    /// scenario's box of a message it does not name).
    pub buttons: Option<usize>,
    /// Which showing it is, counted up for every box shown: two boxes alike (the same
    /// message, or two form-less boxes of the same words) are two showings.
    pub serial: u64,
}

/// Snapshot of the Layer 5 state plus the world's (`fnv.game.state/1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSnapshot {
    pub format: String,
    /// The script extender's strings and arrays, kept with the save as the original keeps
    /// them in its co-save (task F1; docs/spec-notes/ext-save.md; NVForever never reads
    /// or writes the original's `.nvse` file).
    #[serde(default, skip_serializing_if = "fnv_script::vm::ExtStore::is_empty")]
    pub ext: fnv_script::vm::ExtStore,
    /// The lambdas scripts made, so variables naming them stay right after a load.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lambdas: Vec<SavedLambda>,
    /// NX NVSE's variables on references (`crate::nvse_nx`), saved [documented: they "are
    /// persistent across game sessions"].
    #[serde(default, skip_serializing_if = "crate::nvse_nx::NxVars::is_empty")]
    pub nx: crate::nvse_nx::NxVars,
    pub sim: Snapshot,
    pub quests: BTreeMap<FormId, QuestState>,
    pub instances: Vec<(InstKey, Instance)>,
    pub extra: BTreeMap<FormId, RefExtra>,
    pub player: PlayerState,
    pub form_lists: BTreeMap<FormId, Vec<FormId>>,
    pub loaded_cells: BTreeSet<FormId>,
    pub moved: BTreeSet<FormId>,
    pub menu: Option<u16>,
    #[serde(default)]
    pub loading: bool,
    pub message_box: Option<MessageBox>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boxes: Vec<FormId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued: Vec<QueuedBox>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speech: Vec<Speech>,
    /// Live inventory explosives, on the saved `time` clock (pickpocket.md section 5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planted: Vec<crate::pickpocket::Planted>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub said: BTreeSet<(FormId, FormId)>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub said_day: BTreeMap<FormId, BTreeMap<FormId, f64>>,
    /// `said_day` counts hours as GameDaysPassed × 24. Absent in older saves, whose hours
    /// added GameHour as well: one such number fits many times of day, so it cannot be
    /// converted and a load drops those holds (Say Once history stays).
    #[serde(default)]
    pub said_day_days_clock: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialogue: Option<crate::dialogue::Conversation>,
    pub frame: u64,
    pub time: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_onload: Vec<FormId>,
    pub rng_word_pos: String,
    #[serde(default)]
    pub forms: FormChanges,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<ActiveEffect>,
    #[serde(default)]
    pub next_effect: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_modifiers: Vec<ActiveImageModifier>,
    /// Broken locks and the lockpicking help's first display (`crate::lockpick::Locks`).
    #[serde(default, skip_serializing_if = "crate::lockpick::Locks::is_empty")]
    pub locks: crate::lockpick::Locks,
    /// Terminals hacked or locked out (`crate::terminal::Terminals::access`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub terminals: BTreeMap<FormId, crate::terminal::Access>,
    /// Terminals scripts locked or unlocked (`crate::terminal::Terminals::script_locks`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub terminal_locks: BTreeMap<FormId, crate::terminal::ScriptLock>,
    /// Terminals whose Computer Whiz try was spent (`crate::terminal::Terminals::retried`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub terminal_retries: std::collections::BTreeSet<FormId>,
    /// The Pip-Boy radio as scripts and the player left it: on, and the station tuned.
    #[serde(default)]
    pub pipboy_radio_on: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipboy_station: Option<FormId>,
    /// SetNPCRadio: actor -> the station's reference it plays.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub npc_radios: BTreeMap<FormId, FormId>,
    /// The weather the sky showed and whether it rained (`crate::weather::WeatherState`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weather_shown: Option<FormId>,
    #[serde(default)]
    pub raining: bool,
    /// Long-lived provocations: (actor, whom it fights) and why; when each actor last
    /// accepted the player's yield (`crate::provocation::Provocations`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grudges: Vec<((FormId, FormId), crate::provocation::Grudge)>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub forgiven: BTreeMap<FormId, f64>,
    /// Navmeshes the scripts switched, and setAllReachable (`functions::navigation`).
    #[serde(default, skip_serializing_if = "crate::functions::navigation::NavSwitches::is_empty")]
    pub nav_switches: crate::functions::navigation::NavSwitches,
    /// Securitron faces SetSecuritronExpression set: target -> screen texture
    /// (`functions::misc::TextureSwaps::securitron`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub securitron_faces: BTreeMap<FormId, String>,
    /// `MoveToMarkerWithFade` moves still waiting for their fade out
    /// (`functions::misc::ScriptLooks::fade_moves`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fade_moves: BTreeMap<FormId, crate::functions::misc::FadeMove>,
    /// Players 2–4 of a co-op game (`crate::coop`); absent in a one-player save.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coop: Vec<crate::coop::CoopPlayer>,
    /// Activate children waiting for their delay (`crate::traps`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub due_children: Vec<crate::traps::DueChild>,
    /// The menu under the tutorial menu and the tutorial's pages (`Game::show_tutorial`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_under: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tutorial: Vec<FormId>,
    /// What waits for the next frame's start: the engine saves after the frame's
    /// [`Game::tick`] (`nvf-play` `write_saves` runs after `PlaySet::Sync`, the tick in
    /// `PlaySet::Scripts`), so a save lands between the frame that filled these and the
    /// one that runs them. SayToDone of SayTo calls with no line ([`Game::say`]) and
    /// reference events the engine queued ([`Game::queue_ref_event`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub say_to_unsaid: Vec<(FormId, FormId)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_ref_events: Vec<(FormId, u16, Option<FormId>)>,
    /// Scripted doors whose open flag changed since [`Game::watch_doors`] last saw them:
    /// the flag it saw. Doors it saw as they stand are left out (a door first seen after
    /// a load is taken as it stands, which is the same).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub door_open_seen: BTreeMap<FormId, bool>,
    /// Actors out of the Normal state or with anything on their lists: state, alert and
    /// target lists, the target fought (`crate::awareness::Awareness`), so that whoever
    /// fought is still fighting after a load and IsInCombat on the player reads the same
    /// before the first detection round (#971; save-load.md §3.2).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub awareness: BTreeMap<FormId, crate::awareness::Awareness>,
    /// Who has had OnStartCombat and not yet OnCombatEnd (`Game::fighting`), written when
    /// empty too: a start still due when the save was made still runs. `None` in saves
    /// made before this field, which did not record it (`Game::restore`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fighting: Option<BTreeSet<FormId>>,
    /// (actor, target) pairs StartCombat made hostile until StopCombat.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub forced_combat: BTreeSet<(FormId, FormId)>,
    /// ppNVSE's form instances [documented: CreateFormInstance, "tied to the users save
    /// game"] (`crate::nvse_pp`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub form_instances: Vec<(FormId, crate::nvse_pp::Instance)>,
    /// The last animation group scripts asked of each reference (code, name;
    /// `crate::anims::ObjectAnims::asked`): a gate left open shows and collides open.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_groups: BTreeMap<FormId, (u16, String)>,
    /// JohnnyGuitar's extra misc stats, by name in lower case [documented:
    /// ModExtraMiscStat, "Stats and their values are stored in the save"].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_misc_stats: BTreeMap<String, f64>,
}

impl GameSnapshot {
    /// Whether [`Game::restore`] and `World::restore` can take this snapshot: a load asks
    /// before it starts anything from the save (docs/design/save-load.md §6).
    pub fn check(&self) -> Result<(), String> {
        if self.format != SNAPSHOT_FORMAT {
            return Err(format!("unsupported snapshot format {:?}", self.format));
        }
        self.sim.check()?;
        crate::coop::checked(self.coop.clone()).map_err(|e| e.to_string())?;
        self.rng_word_pos.parse::<u128>().map_err(|_| format!("bad rng_word_pos {:?}", self.rng_word_pos))?;
        Ok(())
    }

    /// After a save made with another load order was remapped (`fnv_save::remap_json`,
    /// docs/design/save-load.md §4.1): drop what stands on forms of plugins no longer
    /// loaded (`gone`), where a field and not a key names them. References whose base form
    /// or cell is gone (created ones: a plugin's creature placed with PlaceAtMe, its item
    /// dropped), script instances whose key or script is gone, magic effects whose effect,
    /// source, target or script is gone (and their effect scripts; a co-op player's
    /// likewise), NX form variables naming a gone form, and the radio station
    /// or weather shown. Live plants lose missing victims, items and object effects;
    /// their witness lists lose missing or dropped references. A dropped reference's
    /// detection state and place among the fighters go with it: in Combat and not among them, it would start a fight on the next tick.
    /// Other fields keep the gone ID, which names no form: a lookup of it
    /// fails as one of any form that does not exist [hypothesis: the original's behaviour
    /// with a missing plugin is not documented]. Returns how many were dropped.
    pub fn drop_gone(&mut self, gone: impl Fn(FormId) -> bool) -> usize {
        let mut n = 0;
        let dropped: BTreeSet<FormId> = self.sim.refs.iter().filter(|(_, r)| gone(r.base) || r.cell.is_some_and(&gone)).map(|(&id, _)| id).collect();
        self.sim.refs.retain(|id, _| !dropped.contains(id));
        n += dropped.len();
        let before = self.planted.len();
        self.planted.retain(|p| !gone(p.victim) && !dropped.contains(&p.victim) && !gone(p.item) && !p.blast.enchantment.is_some_and(&gone));
        n += before - self.planted.len();
        for p in &mut self.planted {
            p.seen_by.retain(|w| !gone(*w) && !dropped.contains(w));
        }
        self.awareness.retain(|id, _| !dropped.contains(id));
        if let Some(fighting) = &mut self.fighting {
            fighting.retain(|id| !dropped.contains(id));
        }
        let before = self.sim.deleted_created.len();
        self.sim.deleted_created.retain(|_, &mut b| !gone(b));
        n += before - self.sim.deleted_created.len();
        let before = self.effects.len();
        self.effects.retain(|e| !gone(e.effect) && !gone(e.source) && !gone(e.target) && !e.script.is_some_and(&gone));
        n += before - self.effects.len();
        // A co-op player's effects go as the game's own player's do; its inventory and
        // values are keyed by form, which the remap already dropped.
        for p in &mut self.coop {
            let before = p.effects.len();
            p.effects.retain(|e| !gone(e.effect) && !gone(e.source) && !e.script.is_some_and(&gone));
            n += before - p.effects.len();
        }
        let live: BTreeSet<u32> = self.effects.iter().map(|e| e.id).collect();
        let before = self.instances.len();
        self.instances.retain(|(k, inst)| {
            let key_gone = match k {
                InstKey::Ref { reference } => gone(*reference),
                InstKey::Quest { quest } => gone(*quest),
                InstKey::Item { holder, item } => gone(*holder) || gone(*item),
                InstKey::Effect { id } => !live.contains(id),
                InstKey::Challenge { challenge } => gone(*challenge),
            };
            !key_gone && !inst.script.is_some_and(&gone)
        });
        n += before - self.instances.len();
        n += self.nx.drop_gone(&gone);
        for slot in [&mut self.pipboy_station, &mut self.weather_shown] {
            if slot.is_some_and(&gone) {
                *slot = None;
                n += 1;
            }
        }
        n
    }
}

pub const SNAPSHOT_FORMAT: &str = "fnv.game.state/1";

/// What the default action of the player's own activation did
/// ([`Game::default_activation_by_hand`]).
#[derive(Debug, Clone, PartialEq)]
pub struct HandActivation {
    pub activation: fnv_sim::world::Activation,
    /// Where its `Activated` event is in [`Game::events`]: the engine applies the outcome
    /// itself and leaves this event out of what it turns into effects.
    pub activated_event: usize,
    /// Where a pickup's `ItemAdded` event is, likewise.
    pub item_added_event: Option<usize>,
}

/// How an activation runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActMode {
    /// The player (or an actor) activates by hand: a script's OnActivate block runs
    /// instead of the default action; the default action runs only when the block calls
    /// `Activate` [documented: GECK wiki Activate; layer5.md §3.4].
    Hand,
    /// `Activate` with flag 0 (or none): the default action, bypassing any OnActivate
    /// block [documented: GECK wiki Activate; observed: a scripted `Activate player` of
    /// the Doc Mitchell house door and of dead ED-E did not run their blocks,
    /// layer5-captures.md §8.3 f].
    Default,
    /// `Activate actor 1`: the OnActivate block instead of the default action
    /// [documented: GECK wiki Activate, RunOnActivateBlockFlag]; the default action when
    /// the target has no block [observed: a dead legionary without an OnActivate block
    /// opened its loot menu, `L5.quest.dead_activation`].
    Block,
}

/// Where a running script keeps its locals.
enum Locals {
    Instance(InstKey),
    /// A result script (stage, dialogue, package): locals live for one run.
    Temp(BTreeMap<u16, Value>),
}

/// What makes a lambda the same one each time a script makes it (#1088): its unit (by
/// address: units live as long as the game, from the data or leaked, so no other unit
/// takes the address; their keys are not unique, every console line is `CONS`), its
/// number in the unit (`XExpr::Lambda::index`), and the instance whose variables it
/// shares. Not the expression's own address: user-defined functions run from a copy of
/// their body, so that changed with every call and could be a dropped lambda's.
type LambdaKey = (usize, u32, Option<InstKey>);

fn lambda_key(unit: &Compiled, l: &fnv_script::ir::XExpr, inst: Option<InstKey>) -> LambdaKey {
    let index = match l {
        fnv_script::ir::XExpr::Lambda { index, .. } => *index,
        _ => u32::MAX,
    };
    (unit as *const Compiled as usize, index, inst)
}

/// The lambda of `unit` that has the number of `l`, as the unit is compiled now.
fn unit_lambda(unit: &Compiled, l: &fnv_script::ir::XExpr) -> Option<fnv_script::ir::XExpr> {
    fn find(v: &serde_json::Value, index: u64) -> Option<&serde_json::Value> {
        match v {
            serde_json::Value::Object(m) => {
                if m.get("x").and_then(|x| x.as_str()) == Some("lambda") && m.get("index").and_then(|i| i.as_u64()).unwrap_or(0) == index {
                    return Some(v);
                }
                m.values().find_map(|x| find(x, index))
            }
            serde_json::Value::Array(a) => a.iter().find_map(|x| find(x, index)),
            _ => None,
        }
    }
    let fnv_script::ir::XExpr::Lambda { index, .. } = l else { return None };
    let program = serde_json::to_value(&unit.program).ok()?;
    find(&program, *index as u64).and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// A lambda a script made: its unit (`None` when a loaded save names a unit that is gone)
/// and that unit's key, the lambda, and the instance whose variables it shares.
#[derive(Clone)]
struct LambdaDef<'a> {
    unit: Option<&'a Compiled>,
    key: String,
    lambda: fnv_script::ir::XExpr,
    inst: Option<InstKey>,
    /// The variables of a maker that has no instance (a function script, a result
    /// script) when the lambda was made: the lambda reads them [observed: jip-events
    /// ev.this, a handler lambda compares GetSelf with the query's own variable].
    captured: Option<BTreeMap<u16, Value>>,
}

/// What lasts for the session across a load ([`Game::take_session`]).
pub struct Session {
    handlers: Vec<Handler>,
    /// SetGameMainLoopCallback's callbacks of function scripts without flag 8
    /// [documented: SetGameMainLoopCallback, 8 "auto-remove on load game"; observed:
    /// save-read, runtime lambdas did not run after a load, flag 8 or not].
    main_loop: Vec<crate::jip_events::MainLoop>,
    restarted_seen: BTreeSet<String>,
    handler_seq: u64,
    /// ppNVSE's instance and item-mod handlers, and the instances to deconstruct.
    pp: crate::nvse_pp::PpSession,
    /// ITR's settings that outlast a load (`crate::nvse_itr1::ItrKept`).
    itr_kept: crate::nvse_itr1::ItrKept,
    /// The Script Runner files RegisterSRScript added (`crate::script_runner`).
    sr_registered: Vec<String>,
    /// JIP's session settings (ash-pile activators, terminal art).
    jip_kept: crate::jip_events::JipKept,
    /// The dialogue records scripts changed (`crate::dialogue_edits`).
    dialogue_edits: crate::dialogue_edits::DialogueEdits,
}

/// An event handler: the event (lower case), the function or lambda, the filters by
/// argument index (0 the calling reference), its priority, whether SetEventHandlerAlt set
/// it, and its place in registration order.
#[derive(Debug, Clone)]
pub(crate) struct Handler {
    pub(crate) event: String,
    /// The event's name as the script gave it (GetEventHandlers' StringMap keys).
    pub(crate) name: String,
    pub(crate) func: fnv_script::vm::XVal,
    pub(crate) filters: Vec<(usize, fnv_script::vm::XVal)>,
    pub(crate) priority: i32,
    pub(crate) alt: bool,
    pub(crate) seq: u64,
}

/// A user-defined function running: the value it set, and the script that called it.
struct UdfFrame {
    value: Option<fnv_script::vm::XVal>,
    caller: Option<FormId>,
}

/// User-defined functions nest at most this deep; a call deeper is refused [documented:
/// User Defined Function, NVSE aborts after 30 nested calls].
pub const MAX_UDF_DEPTH: usize = 30;

/// Lambdas are named by values from here on, in a `ref` variable's slot [decision: a
/// FormID this high needs a 255th plugin, which NVForever's load order never gives a
/// lambda's place to; script-compiler.md §9].
/// Ids of foreach's temporary inventory references (B5), below the lambdas'.
pub const TEMP_REF_BASE: u32 = 0xFE40_0000;
/// Ids of the scripts CompileScript made from text files, below the temporary
/// references' [decision, as the lambdas'].
pub const COMPILED_BASE: u32 = 0xFE20_0000;
pub const LAMBDA_BASE: u32 = 0xFE80_0000;

/// What a script run is attached to.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunCtx {
    pub self_ref: Option<FormId>,
    pub action_ref: Option<FormId>,
    pub quest: Option<FormId>,
}

/// The compiler's view of a running game: names as the winning records give them, a
/// form's script by [`Game::instance_of`] (`fnv_script::compile::Env`).
pub struct GameEnv<'g, 'a> {
    pub g: &'g Game<'a>,
}

impl fnv_script::compile::Env for GameEnv<'_, '_> {
    fn form(&self, editor_id: &str) -> Option<fnv_script::compile::FormInfo> {
        let id = *self.g.data.edids.get(&editor_id.to_ascii_lowercase())?;
        self.info(id.0)
    }
    fn info(&self, form: u32) -> Option<fnv_script::compile::FormInfo> {
        let sig = *self.g.data.sigs.get(&FormId(form))?;
        Some(fnv_script::compile::FormInfo { form, sig })
    }
    fn editor_id(&self, form: u32) -> Option<String> {
        self.g.data.edid_of.get(&FormId(form)).cloned()
    }
    /// FormIDs typed at the console are load-order FormIDs.
    fn local_form(&self, local: u32) -> Option<u32> {
        self.g.data.sigs.contains_key(&FormId(local)).then_some(local)
    }
    fn script_vars(&self, form: u32) -> Option<Vec<fnv_script::VarDecl>> {
        let (_, script) = self.g.instance_of(FormId(form))?;
        Some(self.g.data.script(script)?.unit.variables.clone())
    }
}

impl<'a> Game<'a> {
    pub fn new(data: &'a GameData, mut world: World<'a>, cfg: GameConfig) -> Game<'a> {
        // GetTimePlayed counts from the first game the process makes (`nvse_jg5`).
        crate::nvse_jg5::mark_launch();
        // Placed actors carry their template's items (template flag "Inventory").
        // Each at the level it would be fixed at now (`crate::zones`); a cell load fixes it.
        let player_level = world.config().player_level;
        // A new game: nothing rolled again yet, every pick seeded by its reference.
        let sources = data.inventory_sources_at(world.placed, |r| (r, world.level_at(r, player_level)));
        world.set_inventory_sources(sources);
        Game {
            data,
            ext: Default::default(),
            udf: Vec::new(),
            lambdas: Vec::new(),
            compiled_files: Vec::new(),
            compiled_paths: std::collections::HashMap::new(),
            sr: Default::default(),
            lambda_ids: Default::default(),
            handlers: Vec::new(),
            handler_seq: 0,
            jip: Default::default(),
            debug_plugins: BTreeSet::new(),
            temp_refs: Vec::new(),
            list_edits: std::collections::HashMap::new(),
            delayed: Vec::new(),
            ref_walk: Vec::new(),
            field_overrides: std::collections::HashMap::new(),
            name_overrides: std::collections::HashMap::new(),
            mod_data: BTreeMap::new(),
            note_edits: BTreeMap::new(),
            tape_playing: None,
            note_read_session: BTreeMap::new(),
            menu_picks: BTreeMap::new(),
            nx: Default::default(),
            clones: std::collections::HashMap::new(),
            clone_seq: 0,
            playable_overrides: std::collections::HashMap::new(),
            player_magazine: None,
            console_out: Vec::new(),
            effect_overrides: std::collections::HashMap::new(),
            jip_flags: std::collections::HashSet::new(),
            box_callback: None,
            other: Default::default(),
            dialogue_edits: Default::default(),
            one111: Default::default(),
            ini: Default::default(),
            pp: Default::default(),
            udf_value_set: false,
            package_overrides: std::collections::HashMap::new(),
            jip2: Default::default(),
            gaps: Default::default(),
            hud: Default::default(),
            showoff: Default::default(),
            mod_files: Default::default(),
            traits: Default::default(),
            anim_paths: Default::default(),
            anim_folders: Default::default(),
            knvse: Default::default(),
            jippp: Default::default(),
            vheyes: Default::default(),
            small: Default::default(),
            scene: Default::default(),
            input: Default::default(),
            event_names: Vec::new(),
            loaded_seen: BTreeSet::new(),
            restarted_seen: BTreeSet::new(),
            toggles: Default::default(),
            ui_tree: fnv_ui::Ui::new(),
            ui_menus: fnv_ui::MenuIndex::default(),
            ui_lifetime: crate::ui_loading::MenuLifetime::default(),
            dialogue_serial: 0,
            ui_dialogue_hook: None,
            ui_files: None,
            page_menu: None,
            ui_hud_pending: None,
            ui_used: false,
            ui_taps: Vec::new(),
            ui_gradients: Default::default(),
            ui_extra: Default::default(),
            ui_pointer: fnv_ui::input::Pointer::default(),
            difficulty: Default::default(),
            combat_requests: Vec::new(),
            destruction_changes: Vec::new(),
            due_children: Vec::new(),
            swing_traps: Default::default(),
            knockout_news: Vec::new(),
            magic_news: Vec::new(),
            idle_requests: Vec::new(),
            idle_picks: Vec::new(),
            weapon_uses: BTreeMap::new(),
            weapon_use_seq: 0,
            walk_up_greets: BTreeMap::new(),
            sound_requests: Vec::new(),
            music_requests: Vec::new(),
            texture_swaps: Default::default(),
            script_looks: Default::default(),
            anims: Default::default(),
            barter_requests: Vec::new(),
            recipe_requests: Vec::new(),
            repair_requests: Vec::new(),
            roulette_requests: Vec::new(),
            slots_requests: Vec::new(),
            blackjack_requests: Vec::new(),
            caravan_requests: Vec::new(),
            teammate_containers: Vec::new(),
            companions: Default::default(),
            npc_stimpaks: Default::default(),
            rearms: Default::default(),
            brought_along: BTreeSet::new(),
            package_door_notes: Vec::new(),
            rest: Default::default(),
            detection: Default::default(),
            detection_round: None,
            faction_frame: None,
            awareness: Default::default(),
            provocations: Default::default(),
            nav_switches: Default::default(),
            frame_switches: Default::default(),
            combat_lines: Default::default(),
            radio: Default::default(),
            terminals: Default::default(),
            locks: Default::default(),
            senses: Default::default(),
            vats: Default::default(),
            weather: Default::default(),
            ui: Default::default(),
            console_requests: Vec::new(),
            mfg: Default::default(),
            crimes: Default::default(),
            pickpocket: Default::default(),
            world,
            cfg,
            quests: BTreeMap::new(),
            instances: BTreeMap::new(),
            extra: BTreeMap::new(),
            player: PlayerState::default(),
            coop: Vec::new(),
            form_lists: BTreeMap::new(),
            loaded_cells: BTreeSet::new(),
            moved: BTreeSet::new(),
            teleported: BTreeSet::new(),
            rotations: BTreeMap::new(),
            turned: BTreeSet::new(),
            cell_resets: Vec::new(),
            level_news: Vec::new(),
            resurrections: Vec::new(),
            appearance_changes: Vec::new(),
            activate_menu: None,
            perk_acts: Vec::new(),
            challenge_queue: Default::default(),
            note_notices: Vec::new(),
            fighting: BTreeSet::new(),
            condition_target: None,
            menu: None,
            race_menu: RaceMenu::Creator,
            menu_under: None,
            ui_prompt: None,
            tutorial: Vec::new(),
            loading: false,
            message_box: None,
            boxes: Vec::new(),
            queued: Vec::new(),
            box_serial: 0,
            speech: Vec::new(),
            say_to_unsaid: Vec::new(),
            moving_talkers: BTreeSet::new(),
            said: BTreeSet::new(),
            said_day: BTreeMap::new(),
            dialogue: None,
            talking_through: None,
            farewell_ending: false,
            farewell_then: None,
            farewell_said: None,
            voice: None,
            frame: 0,
            time: 0.0,
            pending_onload: Vec::new(),
            door_gates: Default::default(),
            npc_door_run: None,
            queued_ref_events: Vec::new(),
            action_refs: Default::default(),
            deferred_deaths: Vec::new(),
            hits_running: Vec::new(),
            door_open_seen: BTreeMap::new(),
            events: Vec::new(),
            unhandled: BTreeMap::new(),
            stubbed: BTreeMap::new(),
            coverage: None,
            unhandled_seen: BTreeSet::new(),
            arg_unread_seen: BTreeSet::new(),
            stubs_seen: BTreeSet::new(),
            rng: ChaCha8Rng::seed_from_u64(cfg.seed ^ 0x5C21_7000),
            idle_rng: ChaCha8Rng::seed_from_u64(cfg.seed ^ 0x1D1E_7A1C),
            forms: FormChanges::default(),
            effects: Vec::new(),
            next_effect: 1,
            syncing_effects: false,
            condition_check_due: 0.0,
            image_modifiers: Vec::new(),
            effect_elapsed: 0.0,
            depth: 0,
            stop_run: false,
            empty_ref_traced: BTreeSet::new(),
            package_scan_due: 0.0,
            choosing_package: Vec::new(),
        }
    }

    pub fn emit(&mut self, kind: EventKind) {
        let acquired = match kind {
            EventKind::ItemAdded { to: FormId::PLAYER_REF, item, count, .. } => Some((item, count)),
            _ => None,
        };
        self.events.push(Event { frame: self.frame, kind });
        // Items coming to the player count towards the acquire challenges (`crate::challenges`).
        if let Some((item, count)) = acquired {
            self.challenge_acquired(item, count);
        }
    }

    /// Notes that reached the player's inventory (taken from a container, picked up,
    /// AddItem, bought) become the player's notes and leave the inventory: a note is
    /// never an inventory item [documented: GECK wiki GetItemCount, "does not work with
    /// Notes"; observed: GetItemCount 0 for a holotape just added, capture
    /// `fa_batch3_actions`]. The owner's playtest: Jeannie May's Bill of Sale, taken from
    /// her safe, was a misc item and missing from the Pip-Boy's notes.
    pub fn file_carried_notes(&mut self) {
        let Some(inv) = self.world.peek(FormId::PLAYER_REF).and_then(|p| p.inventory.as_ref()) else { return };
        let forms = &self.world.forms;
        let carried: Vec<(FormId, i32)> =
            inv.iter().filter(|&(i, n)| n > 0 && forms.item(i).is_some_and(|it| it.kind == fnv_sim::forms::ItemKind::Note)).collect();
        for (note, n) in carried {
            let _ = self.world.remove_item(FormId::PLAYER_REF, note, n);
            if self.file_note(note) {
                tracing::info!("notes: {} {note} filed with the player's notes", self.data.name(note));
            }
        }
    }

    /// The player is given a note (picked up, taken, AddItem, AddNote, a terminal's Add
    /// Note item). The first time it becomes one of the player's notes, JIP's
    /// `noteadded` event runs and the engine is told to show its corner notice
    /// ([`Game::take_note_notices`]); `false` when the player had it, which shows nothing
    /// [documented: issue #1163, the owner's request; hypothesis: that the original
    /// shows it for script-added notes too, docs/handoff/observer-checks.md].
    pub fn file_note(&mut self, note: FormId) -> bool {
        if !self.player.notes.insert(note) {
            return false;
        }
        self.jip_event("noteadded", None, vec![fnv_script::vm::XVal::Ref(note.0)]);
        self.note_notices.push(note);
        true
    }

    /// Take the notes given since the last call (the engine's corner messages).
    pub fn take_note_notices(&mut self) -> Vec<FormId> {
        std::mem::take(&mut self.note_notices)
    }

    /// The corner line for a note given to the player: its name (FULL; the editor ID
    /// when it has none) after "Note: ".
    /// No setting in the masters holds this line [observed: no string GMST of
    /// FalloutNV.esm or the add-on masters names notes, 2026-10-31], so the words are
    /// NVForever's, the owner's example in issue #1163 (LEGAL.md).
    pub fn note_notice_text(&self, note: FormId) -> String {
        let name = self.data.notes.get(&note).and_then(|n| n.name.clone()).filter(|n| !n.trim().is_empty());
        format!("Note: {}", self.expand_codes(&name.unwrap_or_else(|| self.data.name(note))))
    }

    /// The new-game state: every quest flagged "Start game enabled" is running, with no
    /// stage set [hypothesis: the flag's documented meaning (xEdit); which quest the
    /// "New Game" menu starts is a separate input, scripts.md §8]; the placed actors that
    /// start as corpses are dead ([`Game::lay_out_corpses`]).
    pub fn new_game(&mut self) {
        // Placed corpses (Base Health 0) lie dead from the start.
        self.lay_out_corpses();
        let start: Vec<FormId> = self.data.quests.values().filter(|q| q.start_enabled()).map(|q| q.form).collect();
        for q in start {
            self.start_quest(q);
        }
    }

    /// The New Game menu starts the character-generation quest (the original's INI
    /// `sCharGenQuest`, VCG00 by the original's defaults [documented: Fallout_default.ini
    /// `SCharGenQuest=00102037`]) and the quest's `Begin MenuMode` blocks run once, as
    /// under the main menu (`menus::START`) [hypothesis: the original runs them every
    /// frame there; once is what VCG00's and TTW's start scripts need, each guarding its
    /// MenuMode work by a stage or a flag, observed in their plugins]. VCG00's MenuMode
    /// block sets its stage 0 while the stage is below 5. Returns false when `q` is not
    /// a quest.
    pub fn start_chargen_quest(&mut self, q: FormId) -> bool {
        if !self.start_quest(q) {
            return false;
        }
        let script = self.data.quests.get(&q).and_then(|d| d.script).filter(|s| self.data.scripts.contains_key(s));
        if let Some(script) = script {
            let subject = Some(Value::Num(f64::from(menus::START)));
            self.run_event(InstKey::Quest { quest: q }, script, event::MENU_MODE, subject, RunCtx { quest: Some(q), ..Default::default() });
        }
        true
    }

    // ---- quests ------------------------------------------------------------------

    pub fn quest(&mut self, q: FormId) -> &mut QuestState {
        self.quests.entry(q).or_default()
    }

    pub fn quest_state(&self, q: FormId) -> QuestState {
        self.quests.get(&q).cloned().unwrap_or_default()
    }

    pub fn start_quest(&mut self, q: FormId) -> bool {
        if !self.data.is_quest(q) {
            return false;
        }
        let s = self.quest(q);
        if !s.running {
            s.running = true;
            s.since_run = 0.0;
            s.since_menu_run = 0.0;
            self.emit(EventKind::QuestStarted { quest: q });
            self.quest_event(q, true);
        }
        true
    }

    pub fn stop_quest(&mut self, q: FormId) -> bool {
        if !self.data.is_quest(q) {
            return false;
        }
        let s = self.quest(q);
        if s.running {
            s.running = false;
            self.emit(EventKind::QuestStopped { quest: q });
            self.quest_event(q, false);
        }
        true
    }

    /// `SetStage quest stage` [documented: GECK wiki SetStage; observed: the 5C captures
    /// named per rule]:
    /// - a stage the quest does not define does nothing [observed: VMS16 stage 15];
    /// - a stage already done does nothing unless the quest allows repeated stages
    ///   [observed: VMS16 stage 10 and VCG03 stage 10 set again did not rerun their
    ///   scripts; Generic, with the flag, did];
    /// - the quest starts if it is not running [observed: VMS16];
    /// - the stage is marked done, then every entry whose conditions pass runs its result
    ///   script immediately, nested in the caller [documented: wiki, "resolves any of the
    ///   attached stage items that pass their conditions"], then applies its Complete /
    ///   Fail flag [hypothesis on the order]. Completing does not stop the quest
    ///   [observed: VMS22 ran on after its complete stage].
    pub fn set_stage(&mut self, q: FormId, stage: i32) -> bool {
        let data = self.data;
        let Some(def) = data.quests.get(&q) else { return false };
        let Some(entries) = def.stages.get(&stage) else {
            self.emit(EventKind::StageIgnored { quest: q, stage, reason: "undefined_stage" });
            return false;
        };
        if self.quest(q).stages_done.contains(&stage) && !def.repeated_stages() {
            self.emit(EventKind::StageIgnored { quest: q, stage, reason: "already_done" });
            return false;
        }
        self.start_quest(q);
        self.quest(q).stages_done.insert(stage);
        let player = Some(FormId::PLAYER_REF);
        let label = format!("QUST {q} stage {stage}");
        let mut ran = Vec::new();
        for (i, e) in entries.iter().enumerate() {
            // Stage conditions run on the player [hypothesis].
            if self.conditions_pass(&e.conditions, player, player, &label) {
                ran.push(i);
            }
        }
        self.emit(EventKind::StageSet { quest: q, stage, entries: ran.clone() });
        self.cover_stage(q, stage);
        // JIP's quest-stage handlers, before the result scripts, which a registration
        // with skipScript keeps from running [documented: SetOnQuestStageEventHandler].
        let skip = self.jip_skips_stage(q, stage);
        self.jip_event("queststage", None, vec![fnv_script::vm::XVal::Ref(q.0), fnv_script::vm::XVal::Num(f64::from(stage))]);
        for i in ran {
            let e = &entries[i];
            if let Some(u) = e.unit.filter(|_| !skip) {
                self.run_result(u, RunCtx { quest: Some(q), ..Default::default() });
            }
            if e.flags & STAGE_COMPLETE != 0 {
                let s = self.quest(q);
                if !s.completed {
                    s.completed = true;
                    self.quest_completed(q);
                }
                // The entry's Next Quest (NAM0) "will automatically become the player's
                // active quest" [documented: GECK wiki Quest Stages], as ForceActiveQuest
                // makes it (the Pip-Boy takes `UiRequests::active_quest`).
                if let Some(next) = e.next_quest.filter(|n| data.quests.contains_key(n)) {
                    tracing::info!("quest: {q} stage {stage} completes it; its Next Quest {next} becomes the active quest");
                    self.ui.active_quest = Some(next);
                    self.ui.active_quest_count += 1;
                }
            }
            if e.flags & STAGE_FAIL != 0 {
                // A Fail Quest stage also reads as completed (GetQuestCompleted 1)
                // [observed: VMS16's fail stage 110, `L5.quest.stage_order`].
                let s = self.quest(q);
                let first = !s.failed;
                s.failed = true;
                s.completed = true;
                if first {
                    self.emit(EventKind::QuestFailed { quest: q });
                }
            }
        }
        true
    }

    /// `SetObjectiveDisplayed` / `SetObjectiveCompleted`. Only objectives the quest
    /// defines change; displaying one starts a stopped quest [documented: GECK wiki
    /// SetObjectiveDisplayed]. Completing does not display; showing a hidden objective
    /// clears the completed flag [observed: VMS16 objectives 10 and 5, layer5-captures.md
    /// §5.2], displaying one already shown keeps it [observed: o5_00, VMS19 65 and
    /// VFSAtomicPimp 34, observer check 40, issue #1108].
    pub fn set_objective(&mut self, q: FormId, index: i32, displayed: Option<bool>, completed: Option<bool>) -> bool {
        let data = self.data;
        let Some(def) = data.quests.get(&q) else { return false };
        if !def.objectives.contains_key(&index) {
            return false;
        }
        if displayed == Some(true) {
            self.start_quest(q);
        }
        let o = self.quest(q).objectives.entry(index).or_default();
        let (d0, c0) = (o.displayed, o.completed);
        if let Some(d) = displayed {
            if d && !o.displayed {
                o.completed = false;
            }
            o.displayed = d;
        }
        if let Some(c) = completed {
            o.completed = c;
        }
        let (d1, c1) = (o.displayed, o.completed);
        if displayed.is_some() && d0 != d1 {
            self.emit(EventKind::ObjectiveDisplayed { quest: q, index, on: d1 });
        }
        if c0 != c1 {
            // The message shows only for an objective on display of a quest not yet
            // completed [documented: GECK wiki SetObjectiveCompleted notes, "If the quest
            // itself is not complete, AND the objective is currently being displayed";
            // "If the quest itself is already complete, no message will be displayed"].
            let silent = !d1 || self.quest(q).completed;
            self.emit(EventKind::ObjectiveCompleted { quest: q, index, on: c1, silent });
        }
        true
    }

    // ---- instances and variables -------------------------------------------------

    /// The script a reference runs (its base form's), if any.
    pub fn ref_script(&self, r: FormId) -> Option<FormId> {
        let base = match self.world.peek(r) {
            Some(s) => s.base,
            None => self.world.placed.get(r).map(|p| p.base).or((r == FormId::PLAYER_REF).then_some(FormId::PLAYER_BASE))?,
        };
        // An actor with the template flag "Script" runs its template's script [documented:
        // GECK wiki GetActorTemplateFlag, "Use Script"].
        let base = if self.data.actors.contains_key(&base) || self.data.leveled_actors.contains_key(&base) {
            self.template_source(base, r, |d| d.template_script).map_or(base, |(id, _)| id)
        } else {
            base
        };
        // RemoveScript's and SetScript's changes first.
        self.base_script(base)
    }

    /// The instance that `form.var` addresses: a quest's script or a reference's.
    pub fn instance_of(&self, form: FormId) -> Option<(InstKey, FormId)> {
        if let Some(q) = self.data.quests.get(&form) {
            let s = q.script.filter(|s| self.data.scripts.contains_key(s))?;
            return Some((InstKey::Quest { quest: form }, s));
        }
        Some((InstKey::Ref { reference: form }, self.ref_script(form)?))
    }

    fn instance(&mut self, key: InstKey, script: Option<FormId>) -> &mut Instance {
        self.instances.entry(key).or_insert_with(|| Instance { script, ..Default::default() })
    }

    /// Read a variable of an instance (0 when never written).
    pub fn var(&self, key: InstKey, index: u16) -> Value {
        self.instances.get(&key).and_then(|i| i.vars.get(&index)).copied().unwrap_or_default()
    }

    /// Store into a variable as given, whatever its declared type (xNVSE's SetVariable).
    pub(crate) fn store_raw(&mut self, key: InstKey, script: Option<FormId>, index: u16, v: Value) {
        self.instance(key, script).vars.insert(index, v);
    }

    /// Store into a variable by its declared type: integer variables truncate (see
    /// [`to_int_storage`]), float/ref variables keep the value.
    pub fn store(&mut self, key: InstKey, script: Option<FormId>, index: u16, v: Value, unit: &str, letter_int: bool) {
        let data = self.data;
        let decl = script.and_then(|s| data.script(s)).and_then(|c| c.var_type(index).cloned());
        if script.is_some() && decl.is_none() {
            self.emit(EventKind::UndeclaredVariable { owner: key.label(), index, unit: unit.to_string() });
        }
        let int = match decl {
            Some(VarType::Int) => true,
            Some(_) => false,
            None => letter_int,
        };
        let v = if int { Value::Num(to_int_storage(v.num()) as f64) } else { v };
        self.instance(key, script).vars.insert(index, v);
    }

    pub fn var_name(&self, script: Option<FormId>, index: u16) -> Option<String> {
        script.and_then(|s| self.data.script(s)).and_then(|c| c.var_name(index)).map(str::to_string)
    }

    // ---- running scripts ---------------------------------------------------------

    fn opts(&self) -> RunOpts {
        RunOpts { strict: self.cfg.strict, max_steps: self.cfg.max_steps }
    }

    /// Run statements; returns whether the run ended with `Return` (or a function that
    /// acts as one).
    fn exec(&mut self, unit: &'a Compiled, stmts: &'a [Stmt], locals: Locals, ctx: RunCtx) -> bool {
        self.exec_at(unit, stmts, locals, ctx, None).0
    }

    /// [`Game::exec`], from the start or (`resume`) after the place a ScriptWait left;
    /// with the wait the run ended on, if it did.
    fn exec_at(&mut self, unit: &'a Compiled, stmts: &'a [Stmt], locals: Locals, ctx: RunCtx, resume: Option<&[(usize, usize)]>) -> (bool, Option<(u32, fnv_script::vm::WaitPoint)>) {
        if self.depth >= MAX_DEPTH {
            self.emit(EventKind::ScriptError { unit: unit.key(), error: format!("nesting deeper than {MAX_DEPTH}") });
            return (false, None);
        }
        self.depth += 1;
        let opts = self.opts();
        let data = self.data;
        let table = &data.table;
        let mut ex = Exec { game: self, unit, locals, ctx };
        let mut vm = Vm::new(table, &mut ex, opts);
        let r = match resume {
            Some(at) => vm.resume(stmts, at),
            None => vm.run(stmts),
        };
        let wait = vm.take_wait();
        self.depth -= 1;
        if self.depth == 0 {
            self.run_deferred_deaths();
        }
        match r {
            Ok(exit) => (exit == fnv_script::vm::Exit::Returned, wait),
            Err(e) => {
                self.emit(EventKind::ScriptError { unit: unit.key(), error: e.to_string() });
                (false, None)
            }
        }
    }

    /// Run a result script (quest stage, dialogue, package, terminal): the top-level
    /// statements, with temporary locals.
    pub fn run_result(&mut self, unit: UnitId, ctx: RunCtx) {
        let c = self.data.unit(unit);
        self.cover_result(|| c.key());
        self.exec(c, &c.program.top, Locals::Temp(BTreeMap::new()), ctx);
    }

    /// Run `stmts` as statements of `unit` with temporary locals (the sweep's call probe,
    /// `crate::sweep::probe_calls`).
    pub(crate) fn run_stmts(&mut self, unit: UnitId, stmts: &'a [Stmt], ctx: RunCtx) {
        let c = self.data.unit(unit);
        self.exec(c, stmts, Locals::Temp(BTreeMap::new()), ctx);
    }

    /// Compile `src` as a result script (statements without blocks) and run it now: the
    /// console's lines that its own reader does not take (`set x to a + 1`,
    /// docs/design/console.md). Names resolve against the winning records
    /// ([`GameEnv`]). The compiled unit lives as long as the game data (it is leaked:
    /// one per such line, a debugging aid). Returns the compiler's warnings.
    pub fn run_source(&mut self, src: &str, ctx: RunCtx) -> Result<Vec<String>, String> {
        let (unit, warnings) = self.compile_statements(src, "console", "CONS")?;
        self.run_statements(unit, ctx);
        Ok(warnings)
    }

    /// `src` compiled as a result script (statements without blocks), its unit leaked so it
    /// lives as long as the game data, named `plugin` with an owner of record type `sig`;
    /// with the compiler's warnings. The console's lines and the Script Runner's files
    /// (`crate::script_runner`).
    pub(crate) fn compile_statements(&mut self, src: &str, plugin: &str, sig: &str) -> Result<(&'a Compiled, Vec<String>), String> {
        let c = fnv_script::compile::compile(src, &self.data.table, &GameEnv { g: self }, None);
        if let Some(e) = c.errors().next() {
            // The console's own line needs no number.
            return Err(match sig {
                "CONS" => format!("{}: {}", e.kind.replace('_', " "), e.detail),
                _ => format!("line {}: {}: {}", e.line, e.kind.replace('_', " "), e.detail),
            });
        }
        let warnings = c.diags.iter().map(|d| format!("{}: {}", d.kind.replace('_', " "), d.detail)).collect();
        let d = fnv_script::decode(&c.bytes, &self.data.table);
        let unit = fnv_script::ScriptUnit {
            plugin: plugin.into(),
            owner: fnv_script::UnitOwner::Other { sig: sig.into(), form: 0, ordinal: 0 },
            kind: fnv_script::ScriptKind::Object,
            header: Default::default(),
            variables: c.variables,
            refs: c.refs,
            has_scda: true,
            bytes: c.bytes,
            source: Some(src.to_string()),
        };
        let unit: &'a Compiled = Box::leak(Box::new(Compiled::new(unit, d.program, d.report.raw_total())));
        Ok((unit, warnings))
    }

    /// Run a unit of [`Game::compile_statements`] now, with temporary locals; its strings
    /// and arrays are not freed when it ends (as a result script's). False when it stopped
    /// on an error (traced as `ScriptError`).
    pub(crate) fn run_statements(&mut self, unit: &'a Compiled, ctx: RunCtx) -> bool {
        let errors = self.events.len();
        self.exec(unit, &unit.program.top, Locals::Temp(BTreeMap::new()), ctx);
        let key = unit.key();
        !self.events[errors..].iter().any(|e| matches!(&e.kind, EventKind::ScriptError { unit, .. } if *unit == key))
    }

    /// A whole function script from source, run once as nvf_capture's query step runs one
    /// (xnvse-capture.md §5): its `Begin Function` block with fresh variables, on
    /// `ctx.self_ref`; the value is what SetFunctionValue set (0 when nothing). The
    /// extender's and JIP's stores, handlers and the game itself keep what it changed.
    pub fn run_query(&mut self, src: &str, ctx: RunCtx) -> Result<fnv_script::vm::XVal, String> {
        use fnv_script::vm::XVal;
        let c = fnv_script::compile::compile(src, &self.data.table, &GameEnv { g: self }, None);
        if let Some(e) = c.errors().next() {
            // The error is the capture's own text; what the compiler said goes to the trace.
            let what = format!("query: {}: {}", e.kind.replace('_', " "), e.detail);
            self.emit(EventKind::Skipped { unit: "query".into(), what });
            return Err("script did not compile".into());
        }
        let d = fnv_script::decode(&c.bytes, &self.data.table);
        let unit = fnv_script::ScriptUnit {
            plugin: "query".into(),
            owner: fnv_script::UnitOwner::Other { sig: "QURY".into(), form: 0, ordinal: 0 },
            kind: fnv_script::ScriptKind::Object,
            header: Default::default(),
            variables: c.variables,
            refs: c.refs,
            has_scda: true,
            bytes: c.bytes,
            source: Some(src.to_string()),
        };
        // Leaked as the console's lines are: a lambda the query makes may outlive it.
        let unit: &'a Compiled = Box::leak(Box::new(Compiled::new(unit, d.program, d.report.raw_total())));
        let block = unit.program.blocks.iter().find(|b| b.event == fnv_script::ir::FUNCTION_BLOCK).ok_or("the script has no Function block")?;
        let data = self.data;
        let opts = self.opts();
        self.udf.push(UdfFrame { value: None, caller: None });
        let mut ex = Exec { game: self, unit, locals: Locals::Temp(BTreeMap::new()), ctx };
        let result = Vm::new(&data.table, &mut ex, opts).run(&block.body).map_err(|e| e.to_string());
        if let Locals::Temp(m) = &ex.locals {
            for (&index, v) in m {
                match unit.var_type(index) {
                    Some(VarType::Other(t)) if t == "string_var" => ex.game.ext.destroy(v.num() as u32),
                    Some(VarType::Other(t)) if t == "array_var" => ex.game.ext.release(&XVal::Array(v.num() as u32)),
                    _ => {}
                }
            }
        }
        let frame = self.udf.pop();
        let v = frame.and_then(|f| f.value).unwrap_or(XVal::Num(0.0));
        self.ext.hand_over(&v);
        self.ext.sweep();
        result.map(|_| v)
    }

    /// CompileScript: the script a text file under `Data\NVSE\user_defined_functions`
    /// compiles to, cached by path unless `force` [documented: CompileScript, "cached for
    /// performance", forceRecompile]; `None` when the file is missing or does not compile
    /// [hypothesis: the function then gives no form].
    pub(crate) fn compile_file(&mut self, path: &str, force: bool) -> Option<u32> {
        let key = path.replace('/', "\\").trim_start_matches('\\').to_ascii_lowercase();
        if !force {
            if let Some(&id) = self.compiled_paths.get(&key) {
                return Some(id);
            }
        }
        let bytes = crate::mod_files::read_data_file(&format!("nvse\\user_defined_functions\\{key}"))?;
        // UTF-8 when it is, else 8-bit text in this run's code page (#1439).
        let src = std::str::from_utf8(&bytes).map_or_else(|_| fnv_util::text::decode(&bytes), str::to_owned);
        let id = self.compile_source(&format!("CompileScript {path}"), src)?;
        self.compiled_paths.insert(key, id);
        Some(id)
    }

    /// EvaluateString: whether `text` holds as an `if` condition [documented:
    /// EvaluateString]; each text is compiled once, as a function script, and a text that
    /// does not compile is remembered too, so it is not compiled again. A running unit
    /// must live as long as the game (`Exec::unit`, and lambdas keep theirs), so a unit
    /// cannot be freed: instead at most [`crate::nvse_pp::EVALUATE_MAX`] distinct texts
    /// are compiled in one game, and a new text past that is refused [decision: PR #1291
    /// review, memory bounded].
    pub(crate) fn evaluate_string(&mut self, text: &str) -> Result<bool, String> {
        self.evaluate_on(text, None)
    }

    /// EvaluateString's test run on a reference (`this` and the functions without a
    /// reference name it): kNVSE's AnimGroupOverride conditions
    /// (`crate::nvse_anim_folders`). The compiled texts are EvaluateString's.
    pub(crate) fn evaluate_on(&mut self, text: &str, on: Option<FormId>) -> Result<bool, String> {
        let id = match self.pp.evaluated.get(text) {
            Some(&id) => id,
            None => {
                if self.pp.evaluated.len() >= crate::nvse_pp::EVALUATE_MAX {
                    return Err(format!("{text:?}: more than {} distinct texts in one game", crate::nvse_pp::EVALUATE_MAX));
                }
                let src = format!("Begin Function {{ }}\nif {text}\nSetFunctionValue 1\nendif\nEnd\n");
                let id = self.compile_source("EvaluateString", src);
                self.pp.evaluated.insert(text.to_string(), id);
                id
            }
        };
        let id = id.ok_or_else(|| format!("{text:?} does not compile"))?;
        let ctx = RunCtx { self_ref: on, ..RunCtx::default() };
        Ok(self.run_function(&fnv_script::vm::XVal::Ref(id), on.map(|f| Value::Ref(f.0)), Vec::new(), None, ctx)?.truthy())
    }

    /// A function script from source text, kept as CompileScript keeps its files; `None`
    /// when it does not compile (the compiler's words go to the trace).
    fn compile_source(&mut self, what: &str, src: String) -> Option<u32> {
        // The IDs end where the temporary references' begin: no more units past it.
        if COMPILED_BASE as usize + self.compiled_files.len() >= TEMP_REF_BASE as usize {
            self.emit(EventKind::Skipped { unit: "CompileScript".into(), what: format!("{what}: no compiled-script ID left") });
            return None;
        }
        let c = fnv_script::compile::compile(&src, &self.data.table, &GameEnv { g: self }, None);
        if let Some(e) = c.errors().next() {
            let what = format!("{what}: {}: {}", e.kind.replace('_', " "), e.detail);
            self.emit(EventKind::Skipped { unit: "CompileScript".into(), what });
            return None;
        }
        let d = fnv_script::decode(&c.bytes, &self.data.table);
        let unit = fnv_script::ScriptUnit {
            plugin: "CompileScript".into(),
            owner: fnv_script::UnitOwner::Other { sig: "SCPT".into(), form: 0, ordinal: 0 },
            kind: fnv_script::ScriptKind::Object,
            header: Default::default(),
            variables: c.variables,
            refs: c.refs,
            has_scda: true,
            bytes: c.bytes,
            source: Some(src),
        };
        let unit: &'a Compiled = Box::leak(Box::new(Compiled::new(unit, d.program, d.report.raw_total())));
        let id = COMPILED_BASE + self.compiled_files.len() as u32;
        self.compiled_files.push(unit);
        Some(id)
    }

    /// `Call`: a function script's `Begin Function` block with fresh variables of its own,
    /// its parameters set from the arguments, on the calling reference (`ref.Call`); or a
    /// lambda, over the variables of the script that made it [documented: Call; User
    /// Defined Function; Lambda]. The function's strings and arrays go when it ends
    /// [documented: Ar Null, arrays of a function script are freed].
    pub fn run_function(&mut self, f: &fnv_script::vm::XVal, on: Option<Value>, args: Vec<fnv_script::vm::XVal>, caller: Option<FormId>, ctx: RunCtx) -> Result<fnv_script::vm::XVal, String> {
        use fnv_script::vm::XVal;
        let id = match f {
            XVal::Ref(id) => *id,
            XVal::Num(x) => *x as u32,
            _ => return Err("Call needs a function script or a lambda".into()),
        };
        if self.udf.len() >= MAX_UDF_DEPTH {
            return Err(format!("more than {MAX_UDF_DEPTH} nested user-defined function calls"));
        }
        let data = self.data;
        let opts = self.opts();
        let self_ref = on.and_then(|v| v.as_form()).map(FormId);
        let (unit, params, body, value, locals): (&Compiled, Vec<VarRef>, Vec<Stmt>, Option<fnv_script::ir::XExpr>, Locals) = if id >= LAMBDA_BASE {
            let d = self.lambdas.get((id - LAMBDA_BASE) as usize).cloned().ok_or("an unknown lambda")?;
            let (unit, l, inst) = (d.unit.ok_or_else(|| format!("the lambda's script {} is gone", d.key))?, d.lambda, d.inst);
            let fnv_script::ir::XExpr::Lambda { params, body, value, .. } = l else { return Err("an unknown lambda".into()) };
            let locals = match inst {
                Some(k) => Locals::Instance(k),
                None => Locals::Temp(d.captured.unwrap_or_default()),
            };
            (unit, params, body, value.map(|v| *v), locals)
        } else {
            let compiled = id.checked_sub(COMPILED_BASE).filter(|_| id < TEMP_REF_BASE).and_then(|i| self.compiled_files.get(i as usize).copied());
            let unit = match compiled {
                Some(u) => u,
                None => data.script(FormId(id)).ok_or_else(|| format!("{} is not a function script", data.name(FormId(id))))?,
            };
            let block = unit.program.blocks.iter().find(|b| b.event == fnv_script::ir::FUNCTION_BLOCK).ok_or_else(|| format!("{} has no Function block", data.name(FormId(id))))?;
            let params = block.params.iter().flatten().filter_map(|a| if let Arg::Var(v) = a { Some(*v) } else { None }).collect();
            (unit, params, block.body.clone(), None, Locals::Temp(BTreeMap::new()))
        };
        self.udf.push(UdfFrame { value: None, caller });
        let ctx = RunCtx { self_ref: self_ref.or(ctx.self_ref), ..ctx };
        let mut ex = Exec { game: self, unit, locals, ctx };
        let mut vm = Vm::new(&data.table, &mut ex, opts);
        let mut result = Ok(());
        for (p, a) in params.iter().zip(args) {
            if let Err(e) = vm.bind(p, a) {
                result = Err(e.to_string());
                break;
            }
        }
        let mut lambda_value = None;
        if result.is_ok() {
            match &value {
                Some(v) => match vm.xeval(v) {
                    Ok(x) => lambda_value = Some(x),
                    Err(e) => result = Err(e.to_string()),
                },
                None => {
                    if let Err(e) = vm.run(&body) {
                        result = Err(e.to_string());
                    }
                }
            }
        }
        // A function script's own strings and arrays go with it (a lambda's variables are
        // its maker's).
        if let (true, Locals::Temp(m)) = (id < LAMBDA_BASE, &ex.locals) {
            for (&index, v) in m {
                match unit.var_type(index) {
                    Some(VarType::Other(t)) if t == "string_var" => ex.game.ext.destroy(v.num() as u32),
                    Some(VarType::Other(t)) if t == "array_var" => ex.game.ext.release(&XVal::Array(v.num() as u32)),
                    _ => {}
                }
            }
        }
        let frame = self.udf.pop();
        self.udf_value_set = frame.as_ref().is_some_and(|f| f.value.is_some());
        let v = match lambda_value {
            Some(v) => v,
            None => {
                let v = frame.and_then(|f| f.value).unwrap_or(XVal::Num(0.0));
                self.ext.hand_over(&v);
                v
            }
        };
        result.map(|_| v)
    }

    /// The event functions (tasks E1–E3; docs/spec-notes/ext-events.md). `sender` is the
    /// calling script's plugin, DispatchEvent's default sender.
    pub fn event_call(&mut self, c: fnv_script::vm::EventCall, sender: &str) -> Result<fnv_script::vm::XVal, String> {
        use fnv_script::vm::{EventCall, XKey, XVal};
        let key_text = |k: &XVal| if let XVal::Str(s) = k { s.to_ascii_lowercase() } else { String::new() };
        Ok(match c {
            EventCall::Set { event, func, pairs, alt } => {
                let mut h = Handler { event: event.to_ascii_lowercase(), name: event.clone(), func, filters: Vec::new(), priority: 1, alt, seq: self.handler_seq };
                for (k, v) in pairs {
                    match (key_text(&k).as_str(), &k) {
                        ("priority", _) => {
                            // Default 1, higher first, 0 refused, -9999..9999 [documented:
                            // SetEventHandler; Event Handler Priority System].
                            let p = v.num().map_err(|e| e.to_string())? as i32;
                            if p == 0 || !(-9999..=9999).contains(&p) {
                                return Ok(XVal::Num(0.0));
                            }
                            h.priority = p;
                        }
                        // SetEventHandler: "first"/"second"; an array filter is ignored
                        // [documented: SetEventHandler].
                        ("first", _) if !alt => {
                            if !matches!(v, XVal::Array(_)) {
                                h.filters.push((1, v));
                            }
                        }
                        ("second", _) if !alt => {
                            if !matches!(v, XVal::Array(_)) {
                                h.filters.push((2, v));
                            }
                        }
                        // SetEventHandlerAlt: index 0 the calling reference, 1–15 the
                        // arguments [documented: SetEventHandlerAlt].
                        (_, XVal::Num(i)) if alt && (0.0..=15.0).contains(i) => {
                            self.ext.retain(&v);
                            h.filters.push((*i as usize, v));
                        }
                        _ => return Ok(XVal::Num(0.0)),
                    }
                }
                self.ext.retain(&h.func);
                self.handler_seq += 1;
                self.handlers.push(h);
                XVal::Num(1.0)
            }
            EventCall::Remove { event, func, pairs } => {
                let event = event.to_ascii_lowercase();
                let filters: Vec<(usize, XVal)> = pairs
                    .into_iter()
                    .filter_map(|(k, v)| match (key_text(&k).as_str(), &k) {
                        ("first", _) => Some((1, v)),
                        ("second", _) => Some((2, v)),
                        (_, XVal::Num(i)) => Some((*i as usize, v)),
                        _ => None,
                    })
                    .collect();
                let before = self.handlers.len();
                let ext = &mut self.ext;
                self.handlers.retain(|h| {
                    let hit = h.event == event && h.func == func && filters.iter().all(|f| h.filters.contains(f));
                    if hit {
                        for (_, v) in &h.filters {
                            ext.release(v);
                        }
                    }
                    !hit
                });
                XVal::Num(if self.handlers.len() < before { 1.0 } else { 0.0 })
            }
            // [documented: DispatchEvent] one string map: eventName and eventSender, or the
            // info map given (eventName and eventSender added when it lacks them
            // [hypothesis]); the handlers run at once [hypothesis].
            EventCall::Dispatch { event, map, sender: s } => {
                let sender = s.unwrap_or_else(|| sender.to_string());
                let id = match map {
                    Some(XVal::Array(id)) => id,
                    _ => self.ext.new_array(fnv_script::vm::ArrayKind::StringMap, Vec::new()),
                };
                if let Some(a) = self.ext.array_mut(id).filter(|a| a.kind == fnv_script::vm::ArrayKind::StringMap) {
                    for (k, v) in [("eventName", event.clone()), ("eventSender", sender)] {
                        let key = XKey::Str(k.into());
                        if a.get(&key).is_none() {
                            a.entries.push((key, XVal::Str(v)));
                        }
                    }
                    a.entries.sort_by(|x, y| x.0.cmp_key(&y.0));
                }
                self.ext.retain(&XVal::Array(id));
                self.dispatch_event(&event, vec![XVal::Array(id)], None);
                self.ext.hand_over(&XVal::Array(id));
                XVal::Num(0.0)
            }
            EventCall::DispatchAlt { event, on, args } => {
                self.dispatch_event(&event, args, on.and_then(|v| v.as_form()).map(FormId));
                XVal::Num(1.0)
            }
            EventCall::CurrentName => XVal::Str(self.event_names.last().cloned().unwrap_or_default()),
            EventCall::JipSet { event, func, add, filters } => XVal::Num(f64::from(u8::from(self.jip_set(event, func, add, filters)))),
            EventCall::MainLoop { func, add, delay, flags, on } => {
                self.jip_main_loop(func, add, delay, flags, on.and_then(|v| v.as_form()).map(FormId));
                XVal::Num(0.0)
            }
        })
    }

    /// A temporary inventory reference for one kind of item in a container (B5).
    pub(crate) fn temp_ref(&mut self, container: FormId, item: FormId) -> FormId {
        if let Some(i) = self.temp_refs.iter().position(|&(c, x)| c == container && x == item) {
            return FormId(TEMP_REF_BASE + i as u32);
        }
        self.temp_refs.push((container, item));
        FormId(TEMP_REF_BASE + self.temp_refs.len() as u32 - 1)
    }

    /// The container and item of a live temporary inventory reference.
    pub fn temp_item(&self, r: FormId) -> Option<(FormId, FormId)> {
        let i = r.0.checked_sub(TEMP_REF_BASE).filter(|_| r.0 < LAMBDA_BASE)?;
        self.temp_refs.get(i as usize).copied()
    }

    /// How many parameters a function script's Function block or a lambda declares;
    /// `None` when `f` is neither.
    pub(crate) fn function_arity(&self, f: &fnv_script::vm::XVal) -> Option<usize> {
        use fnv_script::vm::XVal;
        let id = match f {
            XVal::Ref(id) => *id,
            XVal::Num(x) => *x as u32,
            _ => return None,
        };
        if id >= LAMBDA_BASE {
            let d = self.lambdas.get((id - LAMBDA_BASE) as usize)?;
            let fnv_script::ir::XExpr::Lambda { params, .. } = &d.lambda else { return None };
            return Some(params.len());
        }
        let unit = self.data.script(FormId(id))?;
        let block = unit.program.blocks.iter().find(|b| b.event == fnv_script::ir::FUNCTION_BLOCK)?;
        Some(block.params.iter().flatten().count())
    }

    /// What lasts for the session across a load (task E3): the SetEventHandler
    /// registrations [documented: they last for the session; SetEventHandlerAlt ones are
    /// flushed on every load] of function scripts with filters that are not arrays (a
    /// lambda or an array belongs to the old game's state, so those go [hypothesis]), and
    /// which scripts saw GetGameRestarted.
    pub fn take_session(&mut self) -> Session {
        let handlers = self
            .handlers
            .iter()
            .filter(|h| !h.alt)
            .filter(|h| matches!(h.func, fnv_script::vm::XVal::Ref(f) if f < LAMBDA_BASE))
            .filter(|h| h.filters.iter().all(|(_, v)| !matches!(v, fnv_script::vm::XVal::Array(_))))
            .cloned()
            .collect();
        let main_loop = self
            .jip
            .main_loop
            .iter()
            .filter(|m| m.flags & 8 == 0 && matches!(m.func, fnv_script::vm::XVal::Ref(f) if f < LAMBDA_BASE))
            .cloned()
            .map(|mut m| {
                m.due = 0;
                m
            })
            .collect();
        let pp = self.pp_take_session();
        let sr_registered = self.sr.take_registered();
        Session { handlers, main_loop, restarted_seen: std::mem::take(&mut self.restarted_seen), handler_seq: self.handler_seq, pp, itr_kept: std::mem::take(&mut self.other.itr.kept), sr_registered, jip_kept: std::mem::take(&mut self.jip.kept), dialogue_edits: self.dialogue_edits.for_session() }
    }

    /// Take over what lasts for the session ([`Game::take_session`]).
    pub fn put_session(&mut self, s: Session) {
        self.handlers = s.handlers;
        self.jip.main_loop = s.main_loop;
        self.restarted_seen = s.restarted_seen;
        self.handler_seq = s.handler_seq;
        self.pp_put_session(s.pp);
        self.other.itr.kept = s.itr_kept;
        self.sr.put_registered(s.sr_registered);
        self.jip.kept = s.jip_kept;
        self.dialogue_edits = s.dialogue_edits;
    }

    /// Run every handler registered for `event` whose filters match, highest priority
    /// first, ties in registration order [documented: SetEventHandler; Event Handler
    /// Priority System]. A handler may take fewer parameters than the event sends
    /// [documented: DispatchEventAlt]. Returns how many ran. Free when nothing is
    /// registered, so a firing point costs nothing in an unmodded game.
    pub fn dispatch_event(&mut self, event: &str, args: Vec<fnv_script::vm::XVal>, on: Option<FormId>) -> usize {
        if self.handlers.is_empty() {
            return 0;
        }
        let lname = event.to_ascii_lowercase();
        let mut hs: Vec<Handler> = self.handlers.iter().filter(|h| h.event == lname && self.handler_matches(h, &args, on)).cloned().collect();
        if hs.is_empty() {
            return 0;
        }
        hs.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.seq.cmp(&b.seq)));
        self.event_names.push(event.to_string());
        let ctx = RunCtx { self_ref: on, ..RunCtx::default() };
        for h in &hs {
            if let Err(error) = self.run_function(&h.func, on.map(|f| Value::Ref(f.0)), args.clone(), None, ctx) {
                self.emit(EventKind::ScriptError { unit: format!("event handler for {event}"), error });
            }
        }
        self.event_names.pop();
        hs.len()
    }

    fn handler_matches(&self, h: &Handler, args: &[fnv_script::vm::XVal], on: Option<FormId>) -> bool {
        h.filters.iter().all(|(i, f)| {
            let v = match i {
                0 => on.map(|r| fnv_script::vm::XVal::Ref(r.0)),
                i => args.get(i - 1).cloned(),
            };
            v.is_some_and(|v| self.filter_matches(f, &v, h.alt))
        })
    }

    /// [documented: SetEventHandler; New xNVSE Event Filtering System] numbers equal;
    /// strings without regard to case; a form the same form, a reference also its base
    /// form, a form list any of its forms; for SetEventHandlerAlt an array any of its
    /// elements.
    pub(crate) fn filter_matches(&self, f: &fnv_script::vm::XVal, v: &fnv_script::vm::XVal, alt: bool) -> bool {
        use fnv_script::vm::XVal;
        match (f, v) {
            (XVal::Array(id), v) if alt => self.ext.array(*id).is_some_and(|a| a.entries.iter().any(|(_, e)| self.filter_matches(e, v, false))),
            (XVal::Num(a), XVal::Num(b)) => a == b,
            (XVal::Str(a), XVal::Str(b)) => a.eq_ignore_ascii_case(b),
            (XVal::Ref(a), XVal::Ref(b)) => {
                let (a, b) = (FormId(*a), FormId(*b));
                a == b
                    || functions::base_of(self, b) == Some(a)
                    || {
                        let m = functions::list_members(self, a);
                        m.contains(&b) || functions::base_of(self, b).is_some_and(|base| m.contains(&base))
                    }
            }
            _ => false,
        }
    }

    /// Whether a block's parameter accepts `subject` (a block without parameters accepts
    /// everything; `Begin OnActivate player` only the player; `Begin MenuMode 1036` only
    /// that menu).
    fn block_accepts(&mut self, unit: &'a Compiled, key: InstKey, block: &Block, subject: Option<Value>) -> bool {
        let Some(params) = block.params.as_ref().filter(|p| !p.is_empty()) else { return true };
        let Some(subject) = subject else { return false };
        let want = match &params[0] {
            Arg::Int(i) => Value::Num(*i as f64),
            Arg::Float(f) => Value::Num(*f),
            Arg::Enum(c) => Value::Num(*c as f64),
            Arg::Ref(r) => {
                let mut ex = Exec { game: self, unit, locals: Locals::Instance(key), ctx: RunCtx::default() };
                match ex.ref_value(*r) {
                    Ok(v) => v,
                    Err(_) => return false,
                }
            }
            _ => return true,
        };
        // `Begin MenuMode 1` runs for any of the Pip-Boy's menus (`menus::answers`).
        if block.event == event::MENU_MODE && !matches!(params[0], Arg::Ref(_)) {
            return menus::answers(want.num() as u16, subject.num() as u16);
        }
        match (want.as_form(), subject.as_form()) {
            // A form list as the parameter accepts any of its members [hypothesis, fit
            // to the data: Old World Blues' force fields (e.g. SCPT 0x01016AB4) run
            // `Begin OnHitWith NVDLC03SonicWeaponList`, a list of the five Sonic
            // Emitters, and no weapon is a list itself; observer check 38].
            (Some(a), Some(b)) => a == b || crate::functions::refs::list_members(self, FormId(a)).contains(&FormId(b)),
            _ => want.num() == subject.num(),
        }
    }

    /// Run every block of `event` in the instance's script that accepts `subject`.
    /// Returns whether a block ran.
    pub fn run_event(&mut self, key: InstKey, script: FormId, event: u16, subject: Option<Value>, ctx: RunCtx) -> bool {
        self.run_blocks(key, script, event, subject, ctx, false)
    }

    /// [`Game::run_event`]; with `coded_only`, only the blocks with a parameter run (the
    /// side menus' `Begin MenuMode 1007` and `1056`, [`Game::side_menu`]).
    fn run_blocks(&mut self, key: InstKey, script: FormId, event: u16, subject: Option<Value>, ctx: RunCtx, coded_only: bool) -> bool {
        let data = self.data;
        let Some(unit) = data.script(script) else { return false };
        let mut ran = false;
        let coded = |b: &&Block| !coded_only || b.params.as_ref().is_some_and(|p| !p.is_empty());
        for (i, b) in unit.program.blocks.iter().enumerate().filter(|(_, b)| b.event == event && coded(b)) {
            if !self.block_accepts(unit, key, b, subject) {
                continue;
            }
            // A block that waits (JIP's ScriptWait) is skipped, the rest of the script
            // runs, and once its cycles are over it goes on from the wait [documented:
            // ScriptWait]. A cycle is one run of the block's event (every frame for an
            // object script's GameMode, the processing delay for a quest's) [documented];
            // `ScriptWait n` goes on in the n-th cycle after it, 0 as 1 [hypothesis].
            let mut resume = None;
            if let Some(w) = self.jip.waits.get_mut(&key).filter(|w| w.script == script && w.block == i) {
                w.left = w.left.saturating_sub(1);
                if w.left > 0 {
                    continue;
                }
                resume = self.jip.waits.remove(&key).map(|w| w.at);
            }
            self.instance(key, Some(script));
            self.cover_block(|| unit.key(), i);
            ran = true;
            let outer = self.jip.wait_depth.replace(self.depth + 1);
            let (returned, wait) = self.exec_at(unit, &b.body, Locals::Instance(key), ctx, resume.as_deref());
            self.jip.wait_depth = outer;
            // One waiting block per script: a new wait ends the old one [documented].
            if let Some((n, at)) = wait {
                self.jip.waits.insert(key, crate::jip_events::Waiting { script, block: i, left: n.max(1), at });
            }
            // Return ends the script for this run, not just its block: a later block
            // of the same event does not run [documented: GECK wiki Adding an Options
            // Menu, owner export; docs/spec-notes/script-language.md §7.3 row 2].
            if returned {
                break;
            }
        }
        ran
    }

    /// The script extender's event of the same name as the block (`OnHit`, `OnEquip` …),
    /// to the handlers SetEventHandler registered, with the reference and the block's
    /// parameter as its two arguments [documented: SetEventHandler's event table, "the
    /// first argument is the reference the event happened to"; observed: nvse-events,
    /// OnHit (creature, 0), OnActorEquip and OnEquip (player, pistol), OnLoad (the ref
    /// twice)]. OnDeath and OnActivate are sent where they happen, with their own
    /// arguments.
    /// Whether an extender handler is set for `event` (lower case).
    pub(crate) fn handled(&self, event: &str) -> bool {
        self.handlers.iter().any(|h| h.event == event)
    }

    pub(crate) fn ext_event(&mut self, ev: u16, on: FormId, other: Option<Value>) {
        if self.handlers.is_empty() || matches!(ev, event::GAME_MODE | event::MENU_MODE | event::ON_DEATH | event::ON_ACTIVATE) {
            return;
        }
        let Some(name) = self.data.table.block_name(ev).map(str::to_string) else { return };
        use fnv_script::vm::XVal;
        let second = match other {
            Some(v) => match v.as_form() {
                Some(f) => XVal::Ref(f),
                None => XVal::Num(v.num()),
            },
            None => XVal::Num(0.0),
        };
        // OnLoad gives the reference twice [observed].
        let second = if ev == event::ON_LOAD { XVal::Ref(on.0) } else { second };
        self.dispatch_event(&name, vec![XVal::Ref(on.0), second], Some(on));
    }

    /// Run a non-frame event on a reference's script, with a trace line.
    pub fn ref_event(&mut self, r: FormId, event: u16, subject: Option<Value>, action_ref: Option<FormId>) -> bool {
        self.ext_event(event, r, subject);
        if event == event::ON_LOAD {
            // An OnLoad run by the caller (a cell load, or an engine that runs it
            // itself) settles one queued by Enable: OnLoad runs once per load.
            self.pending_onload.retain(|&x| x != r);
        }
        let Some(script) = self.ref_script(r) else { return false };
        let has = self.data.script(script).is_some_and(|u| u.has_block(event));
        if !has {
            return false;
        }
        let ran = self.run_event(
            InstKey::Ref { reference: r },
            script,
            event,
            subject,
            RunCtx { self_ref: Some(r), action_ref, quest: None },
        );
        if ran {
            let block = self.data.table.block_name(event).unwrap_or("?").to_string();
            let unit = self.data.script(script).map(|u| u.key()).unwrap_or_default();
            self.emit(EventKind::BlockRun { unit, block, on: Some(r) });
        }
        ran
    }

    /// Run `event` on `r`'s script at the start of the next frame, with `r` as its
    /// subject and `by` as the action ref: for an engine that reports something outside a
    /// script run (the container menu opened or closed: OnOpen, OnClose).
    pub fn queue_ref_event(&mut self, r: FormId, event: u16, by: Option<FormId>) {
        self.queued_ref_events.push((r, event, by));
    }

    /// A script is running (a block, a result script, a function a script called).
    pub(crate) fn in_script(&self) -> bool {
        self.depth > 0
    }

    /// `r` died during a script run: its OnDeath runs when the outermost script ends.
    pub(crate) fn defer_death(&mut self, r: FormId, killer: Option<FormId>) {
        self.deferred_deaths.push((r, killer));
    }

    /// The OnDeath blocks of deaths during the script run that just ended; deaths in those
    /// blocks run theirs after them.
    fn run_deferred_deaths(&mut self) {
        while !self.deferred_deaths.is_empty() {
            for (r, killer) in std::mem::take(&mut self.deferred_deaths) {
                self.on_death(r, killer);
            }
        }
    }

    /// OnOpen and OnClose of the scripted doors among `refs`: a door's open flag that
    /// changed since the last look runs the block (the player's activation, a script's
    /// SetOpenState, an NPC opening it on its way). The first look at a door only notes
    /// its state.
    fn watch_doors(&mut self, refs: &[FormId]) {
        let data = self.data;
        for &r in refs {
            let Some(script) = self.ref_script(r) else { continue };
            let Some(unit) = data.script(script) else { continue };
            if !unit.has_block(event::ON_OPEN) && !unit.has_block(event::ON_CLOSE) {
                continue;
            }
            if functions::base_of(self, r).and_then(|b| data.sig(b)).as_ref() != Some(b"DOOR") {
                continue;
            }
            let Some(open) = self.world.peek(r).map(|s| s.open).or_else(|| self.world.get(r).ok().map(|s| s.open)) else { continue };
            if self.door_open_seen.insert(r, open).is_some_and(|was| was != open) {
                self.ref_event(r, if open { event::ON_OPEN } else { event::ON_CLOSE }, Some(Value::Ref(r.0)), None);
            }
        }
    }

    /// OnMagicEffectHit on `target` for each base effect of the magic `items` that reach
    /// it (a weapon's or a blast's object effect on an object it hits, a spell on an
    /// actor), the effect as the block's parameter. Returns whether a block ran.
    pub fn magic_effect_hits(&mut self, target: FormId, items: &[FormId], caster: Option<FormId>) -> bool {
        // The extender's OnMagicEffectHit handlers hear it on targets without the block.
        let has_block = self.ref_script(target).is_some_and(|s| self.data.script(s).is_some_and(|u| u.has_block(event::ON_MAGIC_EFFECT_HIT)));
        if !has_block && self.handlers.is_empty() {
            return false;
        }
        let effects: Vec<FormId> = items.iter().filter_map(|i| self.data.effect_items.get(i)).flatten().map(|e| e.effect).collect();
        let mut ran = false;
        for e in effects {
            ran |= self.ref_event(target, event::ON_MAGIC_EFFECT_HIT, Some(Value::Ref(e.0)), caster);
        }
        ran
    }

    /// An actor equipped `item`: OnActorEquip on the actor's own script, then, unless the
    /// item was used up (an aid item), OnEquip on the item's script.
    pub fn equip_events(&mut self, actor: FormId, item: FormId, consumed: bool) {
        self.ref_event(actor, event::ON_ACTOR_EQUIP, Some(Value::Ref(item.0)), Some(actor));
        if !consumed {
            self.item_event(actor, item, event::ON_EQUIP, Some(Value::Ref(actor.0)));
        }
    }

    // ---- frames ------------------------------------------------------------------

    /// One frame. Nothing runs during a loading screen ([`Game::loading`]). Otherwise:
    /// the OnLoad blocks of references enabled since the last frame; quest scripts whose
    /// delay has passed; the object scripts of enabled references in loaded cells; the
    /// scripts of the items the player carries. Each runs GameMode, or MenuMode while a
    /// menu is open ([`Game::menu`]). Then active magic effects and the dialogue lines
    /// being said ([`Game::speech`]) advance.
    ///
    /// Game time ([`Game::time`]) and the quest delay of GameMode advance only outside
    /// menus and loading screens [observed: GetSecondsPassed after a 900-frame container
    /// menu was one ordinary frame for an every-frame quest and the quest's own 0.5 s
    /// delay for VMQ03aTimer, `L5.quest.seconds_after_menu`; layer5-captures.md §8.3 e;
    /// a quest timer and an object script's stopwatch stood still in the name menu,
    /// §10.1]. MenuMode keeps its own delay counter, which advances only in menus
    /// [hypothesis].
    /// Order: quest scripts before the object scripts of references [observed for one
    /// pair: BeatrixFX's GameMode moved GameHour on and two Goodsprings windows
    /// (WastelandWindowScript, copying GetCurrentTime every frame) saw the new hour in
    /// the same frame, `L5.quest.script_order`, layer5-captures.md §9.3 c; hypothesis
    /// for every other pair], each in FormID order [hypothesis], then the items the
    /// player carries, then active magic effects, then dialogue (SayTo lines)
    /// [hypothesis]. Last, in menu mode as well, the conversation with the player
    /// (`crate::dialogue`): it is a menu that pauses the game while its own lines go on
    /// [owner statement 2026-10-10].
    ///
    /// A menu opened by a GameMode script (a message box, the name menu…) holds the
    /// GameMode scripts that have not run yet in this frame: they run, quests on their
    /// due delay, once the menu is closed [hypothesis, generalised from the four pack
    /// plugins' message boxes, which opened one at a time as their start-game quests
    /// ran, each after the previous one was closed, layer5-captures.md §10.1–10.2].
    pub fn tick(&mut self) {
        self.frame += 1;
        self.temp_refs.clear();
        self.sync_player_level();
        // JIP's SetGameMainLoopCallback functions due this frame (task J6), then xNVSE's
        // delayed calls.
        self.run_main_loop();
        // JIP's key-down and key-up events of the keys the engine says are held.
        self.jip_keys();
        self.run_delayed();
        // CacheUI's loops, tile scripts and typewriters; vheyes-nvse's moon fade, deferred
        // 3D refreshes and queued console command; kNVSE's SetAnimCurrentTime calls.
        self.small_tick();
        self.vheyes_tick();
        self.knvse_tick();
        // kNVSE's AnimGroupOverride folders: which hold for each actor.
        self.anim_folders_tick();
        // ppNVSE's CallLoop functions due (`crate::nvse_pp`).
        self.run_pp_loops();
        self.run_box_callback();
        self.track_velocity();
        self.run_due_snippets();
        self.itr_tick();
        let (dt, frame) = (self.cfg.frame_seconds, self.frame);
        self.gaps.sounds.tick(dt, frame);
        self.projectiles_tick(dt as f32);
        self.jg_event(crate::nvse_jip3::JgEvent::RenderUpdate, None);
        self.hud_tick();
        self.expire_aux_timers();
        self.input.next_frame();
        self.turned.clear();
        // SetAllVisible and SetNoAvoidance last one frame: the scripts of this frame
        // set them again (`functions::navigation::FrameSwitches`).
        self.frame_switches = Default::default();
        // The world refuses power armour on an untrained player (a loaded game's flag
        // reaches it here too).
        self.world.set_power_armor_training(self.player.power_armor);
        // Cannibal's and Mister Sandman's acts that are over (`crate::activate_choices`).
        if !self.perk_acts.is_empty() {
            self.finish_perk_acts();
        }
        // The rewards of challenges completed last frame (`crate::challenges`).
        if !self.challenge_queue.rewards.is_empty() {
            self.run_challenge_rewards();
        }
        // In menu mode too: a note taken in the container menu files at once.
        self.file_carried_notes();
        if self.loading {
            self.side_blocks(menus::LOADING);
            return;
        }
        let dt = self.cfg.frame_seconds;
        // A conversation with the player is menu mode too (`Game::in_menu`).
        let in_menu = self.in_menu();
        // FreezeTime holds game time (`nvse_jip3`).
        if !in_menu && !self.gaps.jip3.time_frozen {
            self.time += dt;
        }
        // `Disable 1` holds that ended (`functions::refs::disable`).
        let now = self.time;
        let ended: Vec<FormId> = self.script_looks.fading_out.iter().filter(|(_, &until)| until <= now).map(|(&r, _)| r).collect();
        for r in ended {
            self.script_looks.fading_out.remove(&r);
            if self.world.set_enabled(r, false).is_ok() {
                self.script_looks.disabled_after_fade.push(r);
            }
        }
        // `MoveToMarkerWithFade` moves whose fade out has ended.
        crate::functions::refs::finish_fade_moves(self);
        // Without an engine taking them (a headless run) the lists are kept short.
        if self.script_looks.disabled_after_fade.len() > 256 {
            self.script_looks.disabled_after_fade.drain(..128);
        }
        if self.script_looks.critical_changed.len() > 256 {
            self.script_looks.critical_changed.drain(..128);
        }
        if self.script_looks.fades.len() > 256 {
            self.script_looks.fades.drain(..128);
        }
        for r in std::mem::take(&mut self.pending_onload) {
            if self.present(r) && self.cell_of(r).is_some_and(|c| self.loaded_cells.contains(&c)) {
                self.ref_event(r, event::ON_LOAD, None, None);
            }
        }
        for (r, ev, by) in std::mem::take(&mut self.queued_ref_events) {
            self.ref_event(r, ev, Some(Value::Ref(r.0)), by);
        }
        self.combat_blocks();
        // Activate children whose delay passed (`crate::traps`).
        self.activate_due_children();
        // Fighting NPCs take up the weapon their combat style allows (`crate::style_weapon`).
        self.arm_by_style();
        let ev = if in_menu { event::MENU_MODE } else { event::GAME_MODE };
        let subject = self.open_menu_code().map(|m| Value::Num(m as f64));
        let data = self.data;
        // ToggleScripts: no per-frame block runs (`functions::toggles`).
        let scripts = !self.toggles.scripts_off;
        let due: Vec<(FormId, FormId)> = self
            .quests
            .iter_mut()
            .filter(|(_, s)| scripts && s.running)
            .filter_map(|(q, s)| {
                let def = data.quests.get(q)?;
                let script = def.script.filter(|s| data.scripts.contains_key(s))?;
                let counter = if in_menu { &mut s.since_menu_run } else { &mut s.since_run };
                *counter += dt;
                let delay = s.delay.unwrap_or_else(|| def.script_delay());
                (*counter + 1e-9 >= delay).then_some((*q, script))
            })
            .collect();
        // A menu opened during this frame's GameMode holds the rest of it.
        let held = |g: &Game| !in_menu && g.in_menu();
        for (q, script) in due {
            if held(self) {
                break;
            }
            // A quest stopped by an earlier script this frame does not run.
            let Some(s) = self.quests.get_mut(&q).filter(|s| s.running) else { continue };
            if in_menu {
                s.since_menu_run = 0.0;
            } else {
                s.since_run = 0.0;
            }
            let key = InstKey::Quest { quest: q };
            if !in_menu {
                self.mark_run(key, Some(script));
            }
            self.run_event(key, script, ev, subject, RunCtx { quest: Some(q), ..Default::default() });
        }
        let active = if scripts { self.active_refs() } else { Vec::new() };
        // Doors opened or closed since the last frame, by anyone (OnOpen, OnClose).
        self.watch_doors(&active);
        for r in active {
            if held(self) {
                break;
            }
            let Some(script) = self.ref_script(r) else { continue };
            if self.data.script(script).is_some_and(|u| u.has_block(ev)) {
                let key = InstKey::Ref { reference: r };
                if !in_menu {
                    self.mark_run(key, Some(script));
                }
                self.run_event(key, script, ev, subject, RunCtx { self_ref: Some(r), ..Default::default() });
            }
        }
        let holder = FormId::PLAYER_REF;
        for (item, script) in if scripts { self.carried_scripts(holder, ev) } else { Vec::new() } {
            if held(self) {
                break;
            }
            // An earlier item's script may have removed this one.
            if self.world.item_count(holder, item).unwrap_or(0) > 0 {
                let key = InstKey::Item { holder, item };
                if !in_menu {
                    self.mark_run(key, Some(script));
                }
                self.run_event(key, script, ev, subject, RunCtx { self_ref: None, action_ref: Some(holder), quest: None });
            }
        }
        // NVForever's VATS is no menu: its `Begin MenuMode 1056` blocks run beside
        // GameMode while it marks (`menus::VATS`).
        if !in_menu && !held(self) && self.side_menu() == Some(menus::VATS) {
            self.side_blocks(menus::VATS);
        }
        // Abilities and worn enchantments follow the loaded actors and what they wear,
        // menus included (an item equipped in the Pip-Boy).
        self.sync_loaded_actor_effects();
        if !in_menu {
            self.update_effects(dt);
            self.tick_concussions();
            self.tick_radiation(dt);
            self.tick_needs(dt);
            self.tick_knockouts(dt);
            self.update_speech(dt);
            self.apply_rotations(dt);
            self.companions_sneak();
        } else {
            // The companion wheel's reply goes on behind it (`crate::companion_wheel`).
            self.update_wheel_line(dt);
        }
        self.rotations.clear();
        // The conversation's own lines go on in real time while it pauses the game;
        // another menu on top of it (a message box) holds it.
        if self.menu.is_none() {
            self.update_dialogue(dt);
            self.tick_packages();
            self.update_image_modifiers(dt);
        }
    }

    /// The `Begin MenuMode <code>` blocks of a side menu ([`Game::side_menu`]) in the
    /// running quests' scripts and the scripts of the references in loaded cells, in the
    /// frame order of [`Game::tick`]; blocks without a code and other events do not run,
    /// no game time passes and the quest delays do not count [hypothesis: under a
    /// loading screen the original runs the blocks that ask for it; GetSecondsPassed does
    /// not count it, layer5-captures.md §8.3 e].
    fn side_blocks(&mut self, code: u16) {
        if self.toggles.scripts_off {
            return;
        }
        let subject = Some(Value::Num(code as f64));
        let data = self.data;
        let wants = |script: FormId| {
            data.script(script).is_some_and(|u| {
                u.program.blocks.iter().any(|b| {
                    let c = match b.params.as_deref() {
                        Some([Arg::Int(c), ..]) => *c as f64,
                        Some([Arg::Enum(c), ..]) => *c as f64,
                        Some([Arg::Float(c), ..]) => *c,
                        _ => return false,
                    };
                    b.event == event::MENU_MODE && c == code as f64
                })
            })
        };
        let quests: Vec<(FormId, FormId)> = self
            .quests
            .iter()
            .filter(|(_, s)| s.running)
            .filter_map(|(q, _)| Some((*q, data.quests.get(q)?.script?)))
            .filter(|(_, s)| wants(*s))
            .collect();
        for (q, script) in quests {
            self.run_blocks(InstKey::Quest { quest: q }, script, event::MENU_MODE, subject, RunCtx { quest: Some(q), ..Default::default() }, true);
        }
        for r in self.active_refs() {
            let Some(script) = self.ref_script(r).filter(|s| wants(*s)) else { continue };
            self.run_blocks(InstKey::Ref { reference: r }, script, event::MENU_MODE, subject, RunCtx { self_ref: Some(r), ..Default::default() }, true);
        }
    }

    /// An instance's per-frame block is about to run: GetSecondsPassed in this run
    /// returns the clock time since the previous run started [observed: quest timers
    /// counted down by GetSecondsPassed came 0.95–1.0 × their fTimer after the stage
    /// that set it, also when the script had not called GetSecondsPassed since an
    /// earlier countdown ran out (VCG01 7 at 3.22 s of 3.25, 70 at 0.96 s of 1.0,
    /// layer5-captures.md §10.7); the drops of the delay captures are the quest delay,
    /// §5.4; the rule layer5.md §3.4 planned].
    pub fn mark_run(&mut self, key: InstKey, script: Option<FormId>) {
        let now = functions::clock_now(self.time);
        let inst = self.instance(key, script);
        inst.run_prev = inst.run_last;
        inst.run_last = Some(now);
    }

    /// A trigger is about to run its OnTrigger block for the actors inside it this frame:
    /// call once per frame, before those runs. OnTrigger runs every frame an actor is
    /// inside, so for a script without a GameMode block it is the per-frame run that
    /// GetSecondsPassed counts from [documented: GECK wiki GetSecondsPassed, the time
    /// since the script last ran; hypothesis: an OnTrigger run counts as one]. Without
    /// this, GetSecondsPassed read 0 there: VMS15's detonator spot (FalloutNV SCPT
    /// 0x0014F50D) adds it to a timer and beeps when the timer is 0, resetting it after
    /// 0.5 s, and beeped every frame. A script with a GameMode block is marked by its
    /// GameMode run already.
    pub fn trigger_frame(&mut self, trigger: FormId) {
        let Some(script) = self.ref_script(trigger) else { return };
        let per_frame = self.data.script(script).is_some_and(|u| u.has_block(event::ON_TRIGGER) && !u.has_block(event::GAME_MODE));
        if per_frame && !self.paused() {
            self.mark_run(InstKey::Ref { reference: trigger }, Some(script));
        }
    }

    /// The scripted items `holder` carries whose script has a block for `ev`, in FormID
    /// order. Only the player's inventory runs per-frame item scripts here [observed: a
    /// jar of pistol powder's GameMode ran in the player's inventory by the frame after
    /// it was added, `L5.quest.onadd_gamemode`; hypothesis: items held by other actors
    /// and containers also run, not modelled].
    fn carried_scripts(&self, holder: FormId, ev: u16) -> Vec<(FormId, FormId)> {
        let Some(inv) = self.world.peek(holder).and_then(|s| s.inventory.as_ref()) else { return Vec::new() };
        inv.iter()
            .filter_map(|(item, _)| {
                let script = self.data.script_of_base(item)?;
                self.data.script(script).is_some_and(|u| u.has_block(ev)).then_some((item, script))
            })
            .collect()
    }

    /// Whether game time is stopped for scripts (a menu, a conversation with the player,
    /// or a loading screen).
    pub fn paused(&self) -> bool {
        self.in_menu() || self.loading
    }

    /// Whether the game is in menu mode: a menu is open ([`Game::menu`]) or a
    /// conversation with the player is running ([`Game::dialogue`]) [owner statement
    /// 2026-10-10: a back-and-forth dialogue with the player pauses the game in the
    /// background; lines said outside one (SayTo, Say) do not]. In menu mode
    /// [`Game::tick`] runs MenuMode blocks instead of GameMode and game time stands
    /// still, by the rules observed for the other menus.
    pub fn in_menu(&self) -> bool {
        self.menu.is_some() || self.dialogue.is_some()
    }

    /// The code of the menu on top: [`Game::menu`], else the dialogue menu (1009)
    /// during a conversation. `Begin MenuMode <code>` blocks get this code.
    pub fn open_menu_code(&self) -> Option<u16> {
        self.menu.or(self.dialogue.as_ref().map(|_| menus::DIALOGUE))
    }

    /// Whether the menu with this code is open (`MenuMode <code>`): the menu on top, or
    /// the dialogue menu under it during a conversation [hypothesis: a message box over
    /// a conversation leaves `MenuMode 1009` at 1], or the menu under the tutorial menu
    /// [hypothesis, as the dialogue's]. `MenuMode 1` is the Pip-Boy by any of its codes
    /// ([`menus::answers`]); a loading screen is 1007 and NVForever's VATS marking 1056
    /// ([`Game::side_menu`]).
    /// The engine tells what the menu `code` is about and has highlighted ([`MenuPicks`];
    /// the default when it is closed).
    pub fn set_menu_picks(&mut self, code: u16, picks: MenuPicks) {
        if picks == MenuPicks::default() {
            self.menu_picks.remove(&code);
        } else {
            self.menu_picks.insert(code, picks);
        }
    }

    /// The picks of the menu `code` while it is open.
    pub fn menu_picks(&self, code: u16) -> MenuPicks {
        self.menu_picks.get(&code).copied().filter(|_| self.menu_open(code)).unwrap_or_default()
    }

    /// The open menu on top: the one given ([`Game::menu`]), else the one under it, else
    /// the conversation.
    pub fn top_menu(&self) -> Option<u16> {
        self.menu.or(self.menu_under).or(self.dialogue.as_ref().map(|_| menus::DIALOGUE))
    }

    pub fn menu_open(&self, code: u16) -> bool {
        let open = |m: Option<u16>| m.is_some_and(|m| menus::answers(code, m));
        open(self.menu) || open(self.menu_under) || open(self.side_menu()) || (code == menus::DIALOGUE && self.dialogue.is_some())
    }

    /// A menu code that holds beside the open menu: the loading menu (1007) under a
    /// loading screen, VATS (1056) while NVForever's VATS marks targets (`crate::vats`).
    /// Only the `Begin MenuMode` blocks with that code run for it (`Game::tick`).
    pub fn side_menu(&self) -> Option<u16> {
        if self.loading {
            Some(menus::LOADING)
        } else if self.vats.mode == crate::vats::MODE_TARGET_SELECT {
            Some(menus::VATS)
        } else {
            self.page_menu
        }
    }

    /// One frame of a page the engine shows over a held game in place of the original's
    /// menu `code` (the pause menu's Mods page for the start menu, 1013): the `Begin
    /// MenuMode <code>` blocks of the running quests and of the references in loaded
    /// cells run, as under a side menu ([`Game::side_blocks`]), and `MenuMode <code>` is
    /// true while they do; nothing else of a frame happens (no game time, no queued
    /// events or OnLoad blocks, no AI) and the open menu is left as it is, so a menu a
    /// block opens (ShowMessage, ShowTutorialMenu) stays open [design: issue #976 T11].
    pub fn page_tick(&mut self, code: u16) {
        self.frame += 1;
        self.temp_refs.clear();
        let before = self.page_menu.replace(code);
        self.side_blocks(code);
        self.page_menu = before;
    }

    /// A function asks the running script to stop as `Return` does (RemoveMe).
    pub fn request_return(&mut self) {
        self.stop_run = true;
    }

    // ---- menus and message boxes (docs/spec-notes/scripts.md §7.7) ----------------

    /// A script opens a menu (the name, race, tag-skill, trait or SPECIAL menu, a message
    /// box): it is the open menu from now on [observed: each menu was open on the frame
    /// after the stage whose result script opens it, layer5-captures.md §10.2–10.3;
    /// hypothesis: a second menu replaces the first].
    pub fn open_menu(&mut self, code: u16) {
        if self.menu != Some(code) {
            self.menu = Some(code);
            self.emit(EventKind::MenuOpened { menu: code });
            self.jip_menu_event("menuopen", code);
        }
    }

    /// JIP's menu-open and menu-close events, with the menu's code [documented:
    /// SetOnMenuOpenEventHandler, SetOnMenuCloseEventHandler: the handler gets the menu
    /// ID; without one it runs for any menu].
    fn jip_menu_event(&mut self, event: &str, code: u16) {
        self.jip_event(event, None, vec![fnv_script::vm::XVal::Num(f64::from(code))]);
    }

    /// `ShowMessage` of a message box: it opens at once when no menu is open, otherwise
    /// it waits until the open menu and the boxes shown before it are closed
    /// [hypothesis]. While it is open, game time stops and MenuMode 1001 runs [observed:
    /// the pack plugins' boxes held GameMode, VCG00's countdown and the world clock,
    /// layer5-captures.md §10.1].
    pub fn show_box(&mut self, message: FormId) {
        let buttons = self.data.messages.get(&message).map_or(0, |m| m.buttons.len());
        self.show_box_buttons(message, buttons);
    }

    /// [`Game::show_box`] of a box that declared `buttons` buttons, all of them whatever
    /// their conditions (a form-less MessageBoxEx box counts its own): kept with the box
    /// while it waits and is on screen ([`Game::front_box`]).
    pub fn show_box_buttons(&mut self, message: FormId, buttons: usize) {
        if !self.cfg.modal_message_boxes {
            return;
        }
        self.align_queued();
        let entry = self.next_queued(Some(buttons));
        self.queued.push(entry);
        self.boxes.push(message);
        if self.menu.is_none() {
            self.open_menu(menus::MESSAGE);
        }
    }

    /// A box of a message not known, put in front of the others (a scenario's `given`
    /// menu): its buttons not known.
    pub fn put_box_in_front(&mut self, message: FormId) {
        self.align_queued();
        let entry = self.next_queued(None);
        self.queued.insert(0, entry);
        self.boxes.insert(0, message);
    }

    /// Put [`Game::queued`] back in step with [`Game::boxes`] when code changed the queue
    /// alone (or an older snapshot had no `queued`): every box then its own showing and
    /// its buttons not known, so nothing is closed on a made-up count and no two boxes
    /// count as one [design].
    pub fn align_queued(&mut self) {
        if self.queued.len() != self.boxes.len() {
            self.queued = (0..self.boxes.len()).map(|_| self.next_queued(None)).collect();
        }
    }

    fn next_queued(&mut self, buttons: Option<usize>) -> QueuedBox {
        self.box_serial += 1;
        QueuedBox { buttons, serial: self.box_serial }
    }

    /// What the box on screen (or first in line) was when shown; `None` without a box, or
    /// when [`Game::queued`] is out of step with [`Game::boxes`].
    pub fn front_box(&self) -> Option<QueuedBox> {
        (self.queued.len() == self.boxes.len()).then(|| self.queued.first().copied()).flatten()
    }

    /// `ShowTutorialMenu message`: the tutorial menu shows the message, a page of the
    /// "VDSG Manual" [documented: GECK wiki ShowTutorialMenu, owner export: "Shows the
    /// specified Message Box form, similar to ShowMessage, but with slightly different
    /// visuals"]. It opens over the menu that is open (CGTutorial shows it from its
    /// `Begin MenuMode 1` block, over the Pip-Boy), which is open again once it closes
    /// [hypothesis]; called while it is open, the message is one more page [hypothesis].
    /// While it is open MenuMode 1059 runs and game time stops, as for the other menus.
    pub fn show_tutorial(&mut self, message: FormId) {
        if !self.tutorial.contains(&message) {
            self.tutorial.push(message);
        }
        if self.menu != Some(menus::TUTORIAL) {
            self.menu_under = self.menu.take();
            self.open_menu(menus::TUTORIAL);
        }
    }

    /// The engine closed one of its menus (the Pip-Boy, a container…): it is no longer
    /// the open menu, by any of its codes ([`menus::same_menu`]), nor the one waiting
    /// under the tutorial menu. A message box waiting meanwhile then opens. Returns
    /// whether it was either.
    pub fn engine_menu_closed(&mut self, code: u16) -> bool {
        if self.menu.is_some_and(|m| menus::same_menu(m, code)) {
            self.close_menu();
            true
        } else if let Some(m) = self.menu_under.filter(|&m| menus::same_menu(m, code)) {
            self.menu_under = None;
            self.jip_menu_event("menuclose", m);
            true
        } else {
            false
        }
    }

    /// The Pip-Boy shows another of its menus (a page, the repair list): its code is the
    /// open menu's from now on, when the Pip-Boy is the open menu. Returns whether it was.
    pub fn set_pipboy_menu(&mut self, code: u16) -> bool {
        match self.menu {
            Some(m) if menus::is_pipboy(m) && menus::is_pipboy(code) => {
                // The page left closes [hypothesis: each page is a menu of its own code,
                // so JIP's menu-close event sees it go].
                if m != code {
                    self.jip_menu_event("menuclose", m);
                }
                self.open_menu(code);
                true
            }
            _ => false,
        }
    }

    /// The player closes the open menu with OK / Done (the console's CloseAllMenus closes
    /// it the same way [observed: layer5-captures.md §10.1, §10.3]; the name menu keeps
    /// the name it has). A message box waiting for a button gets button 0 (OK). The next
    /// waiting message box then opens [hypothesis]. Returns the menu closed.
    pub fn close_menu(&mut self) -> Option<u16> {
        let m = self.menu.take()?;
        if m == menus::TUTORIAL {
            // The menu under it is the open one again (`Game::show_tutorial`).
            self.tutorial.clear();
            self.emit(EventKind::MenuClosed { menu: m });
            self.jip_menu_event("menuclose", m);
            if let Some(under) = self.menu_under.take() {
                self.open_menu(under);
            } else if !self.boxes.is_empty() {
                self.open_menu(menus::MESSAGE);
            }
            return Some(m);
        }
        if m == menus::MESSAGE && !self.boxes.is_empty() {
            self.align_queued();
            self.queued.remove(0);
            let b = self.boxes.remove(0);
            if let Some(mb) = self.message_box.as_mut().filter(|mb| mb.message == b && mb.pressed.is_none()) {
                mb.pressed = Some(0);
            }
        }
        self.emit(EventKind::MenuClosed { menu: m });
        self.jip_menu_event("menuclose", m);
        if !self.boxes.is_empty() {
            self.open_menu(menus::MESSAGE);
        }
        Some(m)
    }

    /// The console's CloseAllMenus: the open menu closes as [`Game::close_menu`] has it,
    /// and with the tutorial menu the menu under it too [observed: one CloseAllMenus left
    /// MenuMode 0 after ShowTutorialMenu over a menu already open, capture mh1_tutorial,
    /// docs/spec-notes/tutorial-menu.md §4]. Returns the menus closed.
    pub fn close_all_menus(&mut self) -> Vec<u16> {
        let mut closed: Vec<u16> = self.close_menu().into_iter().collect();
        if closed == [menus::TUTORIAL] && self.menu.is_some() {
            closed.extend(self.close_menu());
        }
        closed
    }

    /// The player picks a button of the open message box: GetButtonPressed of the script
    /// that showed it returns the button once, and the box closes.
    pub fn press_button(&mut self, button: i32) -> Result<(), String> {
        let m = self.message_box.as_mut().ok_or("no message box is open")?;
        m.can_press(button)?;
        m.pressed = Some(button);
        let message = m.message;
        if self.menu == Some(menus::MESSAGE) && self.boxes.first() == Some(&message) {
            self.close_menu();
        }
        Ok(())
    }

    // ---- dialogue lines (docs/spec-notes/scripts.md §7.7) --------------------------

    /// `speaker.SayTo target topic` (`say_to`) or `speaker.Say topic`: the speaker says the
    /// first line of the topic it can say, and the line's begin result script runs at
    /// once; the end result script runs when the line ends ([`Game::tick`])
    /// [documented: GECK wiki Category:Dialogue, owner export 2026-10-09: the begin
    /// script runs when the actor starts the line, the end script when the actor finishes
    /// it].
    ///
    /// Which line: [`Game::select_line`] (who can say a line: [`Game::can_say`]), with
    /// the listener as the conditions' target (the player for Say): lines of
    /// higher-priority quests first, then the stored order, the first line the speaker
    /// can say, Random stacks apart [observed: Doc Mitchell's SayTo VCG01Intro took the
    /// next unsaid line each time, also the first line of the topic when it had never
    /// been said (stage 60 without 55), layer5-captures.md §10.3]. A speaker already
    /// saying a line stops it (its end script does not run) [hypothesis].
    ///
    /// A SayTo that finds no line the speaker can say still finishes: the speaker's
    /// SayToDone blocks for the topic run in the next frame [hypothesis: the end slides
    /// (the narrator's SayToDone chain, VEnding) call topics that have no line for some
    /// endings, e.g. VEndingBrotherhood for the NCR ending with the Brotherhood never
    /// met, and the chain must go on to VEndingConclusion; observer-checks.md;
    /// docs/handoff/mq-walkthrough-2026-10-27.md].
    pub fn say(&mut self, speaker: FormId, target: Option<FormId>, topic: FormId, say_to: bool) -> Option<FormId> {
        self.say_with(speaker, target, topic, say_to, false)
    }

    /// [`Game::say`] for the script functions Say and SayTo: when no line of the topic
    /// passes the fixed-speaker check (the speaker is not the line's fixed speaker, and
    /// the line has no GetIsID condition of its own), judged from the data alone
    /// ([`Game::no_line_for_speaker`]; a line refused at run time by Say Once or a
    /// condition does not count), the lines are tried without that check, so the speaker the script names says the fixed speaker's line in its
    /// stead ([`crate::dialogue::Says::InStead`]); a topic with a line the speaker passes
    /// the check for picks as before [hypothesis, fitted to the masters and the story
    /// add-ons: every scripted Say/SayTo of a topic none of whose lines passes the check
    /// is a narrator or an intercom saying another character's lines, Dead Money's
    /// narrator saying Elijah's and Vera's slide lines among them; docs/spec-notes/
    /// dialogue.md §4, issue #965]. Combat barks, package and patrol topics keep
    /// [`Game::say`]: an actor with no Hit line does not say another's.
    pub fn say_by_script(&mut self, speaker: FormId, target: Option<FormId>, topic: FormId, say_to: bool) -> Option<FormId> {
        self.say_with(speaker, target, topic, say_to, true)
    }

    fn say_with(&mut self, speaker: FormId, target: Option<FormId>, topic: FormId, say_to: bool, in_stead: bool) -> Option<FormId> {
        let listener = target.or(Some(FormId::PLAYER_REF));
        // The fallback is decided from the data, not from select_line's result: a topic
        // with a line the speaker passes the fixed-speaker check for never falls back, so
        // a spent Say Once line or a false condition leaves the call unsaid as before.
        let chosen = if in_stead && self.no_line_for_speaker(speaker, topic) {
            self.select_line_as(speaker, listener, topic, |_| true, crate::dialogue::Says::InStead)
        } else {
            self.select_line(speaker, listener, topic)
        };
        let Some(info) = chosen else {
            self.emit(EventKind::NoLine { speaker, topic });
            if say_to {
                self.say_to_unsaid.push((speaker, topic));
            }
            return None;
        };
        self.say_line(speaker, target, topic, info, if say_to { LineKind::SayTo } else { LineKind::Say });
        Some(info)
    }

    /// Say's flags on the line `speaker` is saying ([`Speech::force_subtitles`],
    /// [`Speech::two_d`]).
    pub fn set_line_flags(&mut self, speaker: FormId, force_subtitles: bool, two_d: bool) {
        if let Some(s) = self.speech.iter_mut().find(|s| s.speaker == speaker) {
            s.force_subtitles = force_subtitles;
            s.two_d = two_d;
        }
    }

    /// Set who the line `speaker` is saying turns to whom ([`LineLook`]).
    pub fn set_line_look(&mut self, speaker: FormId, look: LineLook) {
        if let Some(s) = self.speech.iter_mut().find(|s| s.speaker == speaker) {
            s.look = look;
        }
    }

    /// Who turns to whom while the lines outside a conversation with the player are
    /// said (docs/design/dialogue-play.md §9). Each line said to someone (SayTo, a hello
    /// or a Goodbye greeting at the player, a line of a conversation between actors):
    /// - the speaker turns its head and body to the listener [owner decision
    ///   2026-10-11: "speaker turns head and body"];
    /// - the listener looks at the speaker [documented: GECK wiki SayTo: the target is
    ///   "forced to look at the speaker" unless `noTargetLookFlag`]: its head only for a
    ///   SayTo line [design], head and body in a conversation between actors, where it
    ///   speaks next [design, as `fnv_ai` has turned both since it started them];
    /// - nobody, when the line's [`LineLook`] says so.
    ///
    /// A line said to nobody (Say) turns nobody. Whether the engine can turn a body (one
    /// on furniture or on its way cannot) is the engine's.
    pub fn facings(&self) -> Vec<Facing> {
        let mut out = Vec::new();
        for s in &self.speech {
            let Some(to) = s.target.filter(|t| *t != s.speaker) else { continue };
            if s.look == LineLook::Nobody {
                continue;
            }
            out.push(Facing { who: s.speaker, to, body: true, speaking: true, info: s.info });
            if s.look == LineLook::Both {
                out.push(Facing { who: to, to: s.speaker, body: s.conversation.is_some(), speaking: false, info: s.info });
            }
        }
        out
    }

    /// The speaker starts saying the line `info` outside a conversation with the player:
    /// a line it was saying stops, the line is remembered for Say Once, and its begin
    /// result script runs, followed by a Run Immediately end result (dialogue.md section
    /// 3.6 [design]; [`Game::say`]; [`LineKind`]). A line of a conversation
    /// between actors lasts as long as its first response.
    pub(crate) fn say_line(&mut self, speaker: FormId, target: Option<FormId>, topic: FormId, info: FormId, kind: LineKind) {
        let data = self.data;
        let dv = self.dialogue_view();
        let Some(def) = dv.info(&info) else { return };
        self.cut_line(speaker);
        self.mark_said(speaker, info, false);
        let conversation = match kind {
            LineKind::Conversation(n) => Some(n),
            _ => None,
        };
        let seconds = match (conversation, def.response_order().first()) {
            (Some(_), Some(&r)) => self.response_seconds(speaker, info, r),
            _ => self.line_seconds(speaker, info),
        };
        self.emit(EventKind::LineStarted { speaker, target, topic, info, seconds });
        self.cover_info(info);
        let (say_to, greeting, hello) = (kind == LineKind::SayTo, kind == LineKind::Greeting, kind == LineKind::Hello);
        // [design] Mark this line before callbacks can finish it or replace it.
        let immediate = def.has(crate::data::info_flag::RUN_IMMEDIATELY);
        self.speech.push(Speech { speaker, target, topic, info, remaining: seconds, say_to, greeting, farewell: false, conversation, earlier_topics: Vec::new(), hello, look: LineLook::Both, response: 0, force_subtitles: false, two_d: false, end_ran: immediate });
        if let Some(&first) = def.response_order().first() {
            self.ask_response_idles(speaker, target, info, first);
        }
        if let Some(u) = def.begin {
            self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(speaker) });
            self.run_result(u, RunCtx { self_ref: Some(speaker), action_ref: None, quest: def.quest });
        }
        if let Some(u) = def.end.filter(|_| immediate) {
            self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(speaker) });
            self.run_result(u, RunCtx { self_ref: Some(speaker), action_ref: None, quest: def.quest });
        }
    }

    /// The lines being said outside a conversation move on to their next responses as
    /// the time of each passes: the idles of a response that starts are asked for
    /// (`crate::idles`; the first response's when the line starts).
    fn speech_responses(&mut self) {
        let dv = self.dialogue_view();
        let mut started = Vec::new();
        for k in 0..self.speech.len() {
            let s = &self.speech[k];
            // A conversation line between actors is its first response only.
            if s.conversation.is_some() {
                continue;
            }
            let Some(def) = dv.info(&s.info) else { continue };
            let order = def.response_order();
            if order.len() < 2 {
                continue;
            }
            let lengths = self.response_lengths(s.speaker, s.info);
            let total: f64 = lengths.iter().sum();
            let mut end = 0.0;
            let at = order.iter().position(|&r| {
                end += lengths.get(r).copied().unwrap_or(0.0);
                total - s.remaining < end
            });
            let at = at.unwrap_or(order.len() - 1);
            if at > s.response {
                started.push((s.speaker, s.target, s.info, order[at]));
                self.speech[k].response = at;
            }
        }
        for (speaker, target, info, response) in started {
            self.ask_response_idles(speaker, target, info, response);
        }
    }

    /// Stop the line a speaker is saying, without its end script. A conversation between
    /// actors whose line is cut ends there [hypothesis]. A Goodbye greeting at the player
    /// is never cut: whatever stops it (its speaker dying, disabled or deleted, another
    /// line, a fight) ends it as if said to the end, its end result script and Add
    /// Topics applying [owner statement 2026-10-27, of the Goodsprings settler's water:
    /// "do this for any reason as a catch all"]. Nor is a farewell, the Goodbye line the
    /// player chose ([`Speech::farewell`]) [hypothesis: a Goodbye line's end script sets
    /// quest stages and must not be lost; the owner's catch-all statement was about the
    /// greeting].
    pub(crate) fn cut_line(&mut self, speaker: FormId) {
        if let Some(k) = self.speech.iter().position(|s| s.speaker == speaker) {
            if self.speech[k].goodbye_at_player() {
                self.end_line(k);
                return;
            }
            let s = self.speech.remove(k);
            self.emit(EventKind::LineEnded { speaker, info: s.info, cut: true });
            if let (Some(lines), Some(target)) = (s.conversation, s.target) {
                self.emit(EventKind::ActorConversationEnded { speaker, target, reason: crate::dialogue::ended::CUT, lines });
            }
        }
    }

    /// Whether an actor is saying a line (IsTalking): a SayTo or Say line, or a line of
    /// the conversation with the player. While the player chooses, the actor is not
    /// talking [hypothesis]. A SayTo line held by a menu (a conversation included) still
    /// counts, as for the other menus.
    pub fn is_talking(&self, actor: FormId) -> bool {
        self.speech.iter().any(|s| s.speaker == actor) || self.dialogue.as_ref().is_some_and(|c| c.actor == actor && c.line.is_some())
    }

    /// Whether an actor is saying its Goodbye greeting at the player (the player talked
    /// to it, no conversation: [`Speech::greeting`]). The engine keeps it where it
    /// stands, turned to the player, until the line is over; its package goes on after
    /// [owner statement 2026-10-27, the Goodsprings settler's water line: "make her stop
    /// and look at the player"; docs/design/dialogue-play.md §1]. Not once it is in a
    /// combat state (Alert, Combat, Lost): the line then ends as if said to the end
    /// (`Game::update_speech_of`) and it fights.
    pub fn greets_player(&self, actor: FormId) -> bool {
        !self.awareness.state(actor).in_combat() && self.speech.iter().any(|s| s.speaker == actor && s.greeting && s.target == Some(FormId::PLAYER_REF))
    }

    /// One frame of dialogue (game time only, like script time [hypothesis: a line does
    /// not advance while a menu is open; observed: Doc's line started at stage 60 while
    /// a message box was open ended after the boxes, layer5-captures.md §10.3]): lines
    /// whose time is up end, in the order they started; each line's end result script
    /// runs on its speaker, then the speaker's SayToDone blocks (after Say too, D1). A speaker
    /// that was disabled, deleted or died stops its line [hypothesis].
    fn update_speech(&mut self, dt: f64) {
        for (speaker, topic) in std::mem::take(&mut self.say_to_unsaid) {
            self.ref_event(speaker, event::SAY_TO_DONE, Some(Value::Ref(topic.0)), None);
        }
        self.update_speech_of(dt, None);
        self.combat_lines_tick();
    }

    /// [`Game::update_speech`] for the lines of one speaker only (`Some`), or of all.
    pub(crate) fn update_speech_of(&mut self, dt: f64, only: Option<FormId>) {
        let mut ended = Vec::new();
        let mut cut = Vec::new();
        self.speech_responses();
        // A Death line goes on after its speaker died (`crate::combat_lines`).
        let death = self.data.by_edid("Death");
        for s in self.speech.iter_mut().filter(|s| only.is_none_or(|o| o == s.speaker)) {
            s.remaining -= dt;
            let here = self.world.peek(s.speaker).is_none_or(|w| !w.deleted()) && self.world.is_enabled(s.speaker).unwrap_or(false);
            // A Goodbye greeting at the player whose speaker is drawn into a fight ends at
            // once as if said to the end (`Game::cut_line`, `Game::greets_player`) [owner
            // statement 2026-10-27: "skip to the end of the line as if she finished
            // speaking", "all the code runs"]; so does a farewell ([`Speech::farewell`])
            // [hypothesis, as in `Game::cut_line`].
            let fights = s.goodbye_at_player() && self.awareness.state(s.speaker).in_combat();
            if !here || (self.extra.get(&s.speaker).is_some_and(|e| e.dead) && Some(s.topic) != death) {
                cut.push(s.speaker);
            } else if s.remaining <= 1e-9 || fights {
                ended.push((s.speaker, s.info));
            }
        }
        for sp in cut {
            self.cut_line(sp);
        }
        for (speaker, info) in ended {
            if let Some(k) = self.speech.iter().position(|s| s.speaker == speaker && s.info == info) {
                self.end_line(k);
            }
        }
    }

    /// The line `self.speech[k]` is over, said to the end: its end result script runs on
    /// its speaker, then what follows the kind of line ([`Game::update_speech`]).
    fn end_line(&mut self, k: usize) {
        let data = self.data;
        let dv = self.dialogue_view();
        let s = self.speech.remove(k);
        let (speaker, info) = (s.speaker, s.info);
        self.emit(EventKind::LineEnded { speaker, info, cut: false });
        // The force greet this line's end script holds, if it is a farewell
        // (`Game::hold_force_greet`).
        let mut then = None;
        if let Some(def) = dv.info(&info) {
            if let Some(u) = def.end.filter(|_| !s.end_ran) {
                self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(speaker) });
                // A farewell's end script may ask for the next conversation. The state is
                // this call's own: a line that ends inside the script (another farewell
                // its speaker's new line ends) neither sees nor clears the outer one's
                // request, and the outer state comes back afterwards [design].
                let outer = (std::mem::replace(&mut self.farewell_ending, s.farewell), self.farewell_then.take());
                self.run_result(u, RunCtx { self_ref: Some(speaker), action_ref: None, quest: def.quest });
                then = self.farewell_then.take().filter(|_| s.farewell);
                (self.farewell_ending, self.farewell_then) = outer;
            }
            // A greeting said at the player ends as a conversation's line does
            // (`Game::start_dialogue`): its Add Topics join the known topics, and it
            // is no SayTo line [hypothesis for both]. A farewell is a conversation's
            // line that ends after its conversation closed (`Game::choose`): the same,
            // as the line would have ended in the conversation (`Game::finish_line`).
            if s.greeting || s.farewell {
                for t in self.player.topics.learn(&def.add_topics) {
                    self.emit(EventKind::TopicAdded { topic: t, by: Some(info) });
                }
            }
        }
        // The force greet a farewell's end script asked for starts now, after the end
        // script and the Add Topics, as after a Goodbye line that ends in its conversation
        // (`Game::finish_line`). Refusals are not errors, as for StartConversation.
        if s.farewell {
            if let Some((speaker, topic)) = then {
                let _ = self.force_greet(speaker, topic);
            }
            return;
        }
        if s.greeting || s.hello {
            return;
        }
        // A conversation between actors goes on (`crate::dialogue`); its lines are no
        // SayTo lines [hypothesis].
        if let Some(lines) = s.conversation {
            self.next_conversation_line(&s, lines);
            return;
        }
        // SayToDone follows a line said by Say as well as by SayTo [documented: GECK
        // tutorials, docs/spec-notes/level-design.md §11.2 D1].
        self.ref_event(speaker, event::SAY_TO_DONE, Some(Value::Ref(s.topic.0)), None);
    }

    /// `r` was enabled: when it is in a loaded cell and its script has an OnLoad block,
    /// that block runs at the start of the next frame, not inside Enable [observed: the
    /// jukebox's and the broken radio's OnLoad effects were not there on the line after
    /// Enable and were there one frame later, layer5-captures.md §8.3 d].
    pub fn note_enabled(&mut self, r: FormId) {
        // The extender's OnLoad handlers hear every reference whose 3D loads, scripted or not.
        let has = self.ref_script(r).and_then(|s| self.data.script(s)).is_some_and(|u| u.has_block(event::ON_LOAD)) || self.handled("onload");
        if has && self.cell_of(r).is_some_and(|c| self.loaded_cells.contains(&c)) && !self.pending_onload.contains(&r) {
            self.pending_onload.push(r);
        }
    }

    pub fn frames(&mut self, n: u64) {
        for _ in 0..n {
            self.tick();
        }
    }

    /// The current cell of a reference.
    pub fn cell_of(&self, r: FormId) -> Option<FormId> {
        match self.world.peek(r) {
            Some(s) => s.cell,
            None => self.world.placed.get(r).and_then(|p| p.cell),
        }
    }

    /// Whether a reference is in the world and enabled (enable parents followed). Reads
    /// the world's reference states alone, so it changes only with
    /// `fnv_sim::World::presence_serial` (the AI's obstacle poll relies on that,
    /// `fnv_ai::Ai::obstacles_in_line`): keep it so, or key that poll on the new input.
    pub fn present(&self, r: FormId) -> bool {
        self.world.peek(r).is_none_or(|s| !s.deleted()) && self.world.is_enabled(r).unwrap_or(false)
    }

    /// Whether a reference's critical stage culls it (GooEnd, DisintegrateEnd): the
    /// engine does not draw it and it has no collision or ragdoll, though it is still
    /// enabled (`functions::objects::set_critical_stage`).
    pub fn culled(&self, r: FormId) -> bool {
        self.extra.get(&r).is_some_and(|e| crate::functions::objects::critical_stage_culls(e.critical_stage))
    }

    /// Scripted references that run per-frame blocks now: in a loaded cell and not
    /// deleted, disabled ones included [documented: GECK wiki Object scripts, owner
    /// export: object scripts run even for disabled references; observed: capture job
    /// `od_09a_disabled_ref_script`, a disabled skull's timer kept falling]. OnLoad at a cell
    /// load runs for disabled references too (`Game::set_loaded`).
    fn active_refs(&self) -> Vec<FormId> {
        self.scripted_refs_in(&self.loaded_cells)
    }

    /// Scripted references now in `cells` and not deleted, disabled ones included, in
    /// FormID order: those placed there that stayed, and those moved or created there.
    fn scripted_refs_in(&self, cells: &BTreeSet<FormId>) -> Vec<FormId> {
        let mut out: BTreeSet<FormId> = BTreeSet::new();
        for c in cells {
            if let Some(v) = self.data.scripted_by_cell.get(c) {
                out.extend(v.iter().copied());
            }
        }
        out.extend(self.moved.iter().copied());
        out.into_iter()
            .filter(|&r| self.cell_of(r).is_some_and(|c| cells.contains(&c)) && self.world.peek(r).is_none_or(|s| !s.deleted()))
            .collect()
    }

    /// The cells loaded around `cell`: the cell itself for an interior; for an exterior,
    /// the 5×5 cells around it [hypothesis: the uGridsToLoad default of 5].
    pub fn cells_around(&self, cell: FormId) -> BTreeSet<FormId> {
        let mut out = BTreeSet::from([cell]);
        let placed = self.world.placed;
        let Some(world) = placed.world_of(cell) else { return out };
        let Some(grid) = placed.world_grid.get(&world) else { return out };
        let Some((&(x, y), _)) = grid.iter().find(|(_, &c)| c == cell) else { return out };
        for dx in -2..=2 {
            for dy in -2..=2 {
                if let Some(&c) = grid.get(&(x + dx, y + dy)) {
                    out.insert(c);
                }
            }
        }
        out
    }

    /// Make `cells` the loaded set; scripted references now in newly loaded cells get
    /// OnLoad, disabled ones included [hypothesis: OnLoad runs when the cell a reference
    /// is in now is loaded; observed for a disabled one, below; a moved one was not seen
    /// in the original].
    pub fn set_loaded(&mut self, cells: BTreeSet<FormId>) {
        let new: Vec<FormId> = cells.difference(&self.loaded_cells).copied().collect();
        let left: Vec<FormId> = self.loaded_cells.difference(&cells).copied().collect();
        // Cells left start their reset clock; cells entered whose clock ran out are reset
        // before anything in them runs (`crate::respawn`).
        self.cells_left(&left);
        let reset: Vec<FormId> = new.iter().copied().filter(|&c| self.cell_entered(c).is_some()).collect();
        // Their encounter zones get a level, their actors and containers theirs, before
        // anything rolls their carried items (`crate::zones`).
        for &c in &new {
            self.fix_cell_levels(c);
        }
        self.loaded_cells = cells;
        // The references of a reset cell run their OnReset blocks, before OnLoad
        // (`crate::respawn`).
        for c in reset {
            self.cell_reset_blocks(c);
        }
        // Actors of the new cells choose their package at the next tick, and dress now.
        self.package_scan_due = 0.0;
        self.outfit_loaded_actors();
        // Disabled references run OnLoad here too [observed: VMS15's rising plane,
        // disabled, read `bLoaded` 0 before its cell loaded and 1 on the line after its
        // Enable; only its OnLoad sets it, so that OnLoad had run at the cell load,
        // layer5-captures.md §8.3 d]. Their Enable queues OnLoad again
        // (`Game::note_enabled`). References disabled through an enable parent are
        // treated the same [hypothesis: the plane was disabled by its own flag].
        let all = self.handled("onload");
        // Chosen before any block runs: a block that moves a reference into another new
        // cell, or makes one, must not give it a second OnLoad here.
        let chosen: Vec<(Vec<FormId>, Vec<FormId>)> = new
            .iter()
            .map(|&c| {
                let mut refs = self.scripted_refs_in(&BTreeSet::from([c]));
                // MoveTo lists a scripted player as moved; no cell loads the player.
                refs.retain(|&r| r != FormId::PLAYER_REF);
                // The extender's OnLoad handlers hear the unscripted references too.
                let others = if all { self.refs_in_cell(c, false).into_iter().filter(|r| !refs.contains(r)).collect() } else { Vec::new() };
                (refs, others)
            })
            .collect();
        for (refs, others) in chosen {
            for r in refs {
                if self.world.peek(r).is_none_or(|s| !s.deleted()) {
                    self.ref_event(r, event::ON_LOAD, None, None);
                }
            }
            for r in others {
                if self.world.peek(r).is_none_or(|s| !s.deleted()) {
                    self.ext_event(event::ON_LOAD, r, None);
                }
            }
        }
    }

    /// Actors in loaded cells that never were loaded put on their outfit and take up
    /// their weapon (`fnv_sim::World::outfit`), dead ones too [hypothesis: a corpse
    /// placed dead wears its outfit as the living do].
    pub fn outfit_loaded_actors(&mut self) {
        for a in self.package_actors() {
            let _ = self.world.outfit(a);
        }
    }

    /// The player is now in `cell`: load it (and its neighbours outdoors).
    pub fn player_entered(&mut self, cell: Option<FormId>) {
        if let Some(c) = cell {
            // EnableFastTravel without its keep flag: going from an interior back to an
            // exterior allows fast travel again [documented: catalogue notes of
            // EnableFastTravel].
            let placed = self.world.placed;
            let was_interior = !self.loaded_cells.is_empty() && self.loaded_cells.iter().all(|&l| placed.world_of(l).is_none());
            // Only travel comes back: "the transition won't allow them to wait; this
            // will have to be manually re-enabled" [documented: GECK wiki
            // EnableFastTravel, example `0 0 0`].
            if was_interior && placed.world_of(c).is_some() && !self.player.fast_travel.keep {
                self.player.fast_travel.travel_disabled = false;
            }
            let cells = self.cells_around(c);
            self.set_loaded(cells);
        }
    }

    // ---- activation and other events -------------------------------------------

    /// Whether `target` refuses every activation: a door or activator marked destroyed
    /// (`SetDestroyed 1`, or a destruction stage with the Destroy flag)
    /// [documented: GECK wiki SetDestroyed, "Destroyed doors/activators will no longer
    /// give rollover text and cannot be activated"]. Terminals and talking activators
    /// count as activators [hypothesis: they are activator-like records; the wiki names
    /// only doors and activators]. Other references (actors, containers, items) keep
    /// their activation: the wiki names only doors and activators.
    pub fn activation_refused(&self, target: FormId) -> bool {
        if !self.extra.get(&target).is_some_and(|e| e.destroyed) {
            return false;
        }
        let sig = functions::base_of(self, target).and_then(|b| self.data.sig(b));
        matches!(sig.as_ref(), Some(b"DOOR") | Some(b"ACTI") | Some(b"TACT") | Some(b"TERM"))
    }

    /// `actor` activates `target`. A destroyed door or activator
    /// ([`Game::activation_refused`]) does nothing: no default action, no OnActivate.
    pub fn activate(&mut self, actor: FormId, target: FormId, mode: ActMode) -> Result<serde_json::Value, String> {
        if !self.activation_begins(actor, target) {
            return Ok(serde_json::json!({"result": "destroyed"}));
        }
        // A door's block letting the NPC that walks through it go on (`crate::npc_doors`).
        if mode == ActMode::Default && self.npc_door_activated(actor, target) {
            return Ok(serde_json::json!({"result": "npc door"}));
        }
        let has_block = self.ref_script(target).and_then(|s| self.data.script(s)).is_some_and(|u| u.has_block(event::ON_ACTIVATE));
        match mode {
            ActMode::Hand | ActMode::Block if has_block => {
                // The block runs inside the activation [observed: a variable the door's
                // OnActivate set was read on the next line of the script that called
                // `Activate player 1`, `L5.quest.ref_activation`].
                let ran = self.ref_event(target, event::ON_ACTIVATE, Some(Value::Ref(actor.0)), Some(actor));
                Ok(serde_json::json!({"result": "script", "ran": ran}))
            }
            ActMode::Hand | ActMode::Block | ActMode::Default => self.default_activation(actor, target, mode == ActMode::Hand),
        }
    }

    /// What every activation does first: the extender's OnActivate handlers run, even when
    /// the activation is then refused, and do not block it [documented: NVSE Event
    /// Handling]. Whether it goes on (not [`Game::activation_refused`]), with `actor` as
    /// the target's action reference.
    fn activation_begins(&mut self, actor: FormId, target: FormId) -> bool {
        self.dispatch_event("OnActivate", vec![fnv_script::vm::XVal::Ref(target.0), fnv_script::vm::XVal::Ref(actor.0)], Some(target));
        if self.activation_refused(target) {
            return false;
        }
        self.action_refs.insert(target, actor);
        true
    }

    /// The player's own activation (E) of a target whose script has no OnActivate block:
    /// [`Game::activate`] in [`ActMode::Hand`] but for the door. The engine's loading takes
    /// the player into the new cell and tells [`Game::player_entered`] when the player is
    /// there, so this does not. `None`: the activation was refused.
    pub fn default_activation_by_hand(&mut self, target: FormId) -> Result<Option<HandActivation>, String> {
        let player = FormId::PLAYER_REF;
        if !self.activation_begins(player, target) {
            return Ok(None);
        }
        self.run_default_activation(player, target, true, false).map(Some)
    }

    /// The default action of an activation (Layer 4), with its Layer 5 consequences:
    /// a picked-up item reaches the actor's inventory (OnAdd), a door moves the player, a
    /// dead actor opens its inventory (the loot menu) [observed: a scripted Activate of a
    /// dead actor opened the container menu, `L5.quest.dead_activation`], and so does one
    /// lying pushed down by PushActorAway [documented: GECK wiki PushActorAway notes, "can
    /// have its inventory accessed by the player until the actor gets up again"]. An ash
    /// or goo pile opens the inventory of the actor it came from (`crate::ash_piles`).
    pub fn default_activation(&mut self, actor: FormId, target: FormId, by_hand: bool) -> Result<serde_json::Value, String> {
        let done = self.run_default_activation(actor, target, by_hand, true)?;
        serde_json::to_value(&done.activation).map_err(|e| e.to_string())
    }

    /// [`Game::default_activation`]; `enter_cell`: a player sent through a door enters the
    /// new cell now, else the caller sees to [`Game::player_entered`].
    fn run_default_activation(&mut self, actor: FormId, target: FormId, by_hand: bool, enter_cell: bool) -> Result<HandActivation, String> {
        let base = functions::base_of(self, target);
        // Where the player was, for whoever follows them through a door
        // (`Game::bring_teammates`).
        let from = self.world.peek(FormId::PLAYER_REF).map(|p| (p.cell, p.pos));
        let pushed = self.knockout(target).is_some_and(|k| k.pushed && k.lying());
        let dead_actor = (self.extra.get(&target).is_some_and(|e| e.dead) || pushed)
            && base.and_then(|b| self.data.sig(b)).is_some_and(|s| matches!(&s, b"NPC_" | b"CREA" | b"LVLN" | b"LVLC"));
        let pile_of = self.ash_pile_source(target).filter(|_| self.present(target));
        let act = if let Some(body) = pile_of {
            tracing::info!("activate: the pile {target} opens the inventory of {body}");
            fnv_sim::world::Activation { outcome: Outcome::OpenedContainer { container: body }, unlocked_with: None, script: None }
        } else if dead_actor && self.present(target) {
            let script = base.and_then(|b| self.world.forms.get(b)).and_then(|b| b.script);
            fnv_sim::world::Activation { outcome: Outcome::OpenedContainer { container: target }, unlocked_with: None, script }
        } else if by_hand {
            self.world.activate(actor, target).map_err(|e| e.to_string())?
        } else {
            self.world.activate_scripted(actor, target).map_err(|e| e.to_string())?
        };
        let v = serde_json::to_value(&act).map_err(|e| e.to_string())?;
        self.note_activating(actor, target);
        let activated_event = self.events.len();
        self.emit(EventKind::Activated { reference: target, by: actor, outcome: Some(v) });
        let mut item_added_event = None;
        if matches!(act.outcome, Outcome::Activated) {
            self.radio_activated(target);
            self.drink_from(actor, target);
        }
        // Its activate children go off (`crate::traps`).
        self.set_off_children(target);
        match act.outcome {
            Outcome::PickedUp { item, count, .. } => {
                item_added_event = Some(self.events.len());
                self.emit(EventKind::ItemAdded { to: actor, item, count, silent: false });
                self.item_event(actor, item, event::ON_ADD, Some(Value::Ref(actor.0)));
            }
            Outcome::Teleported { cell, .. } if actor == FormId::PLAYER_REF => {
                self.arrived_through(target, cell);
                if enter_cell {
                    self.player_entered(cell);
                }
                let brought = self.bring_teammates(from);
                if !brought.is_empty() {
                    tracing::info!("script: {} teammate(s) or follower(s) come along through the door", brought.len());
                }
            }
            // A placed mine disarmed by the player gives its XP (`crate::experience`).
            Outcome::Disarmed { .. } if actor == FormId::PLAYER_REF => {
                self.count_stat(crate::misc_stats::MINES_DISARMED, 1.0);
                let x = crate::experience::mine_disarm_xp(self);
                crate::experience::award(self, crate::experience::Source::DisarmMine, x);
            }
            _ => {}
        }
        Ok(HandActivation { activation: act, activated_event, item_added_event })
    }

    /// The water an activator gives to drink (ACTI WNAM), when it names one.
    pub fn drink_water(&self, target: FormId) -> Option<FormId> {
        functions::base_of(self, target).and_then(|b| self.data.drink_waters.get(&b)).copied()
    }

    /// `actor` drank from the activator `target`: its water's actor effect (WATR XNAM:
    /// the healing and the rads) applies to the drinker, as from the water itself
    /// [hypothesis: drinking applies the effect the water names; activators.md §9].
    /// Whether the activator's script ran too is the activation's business.
    ///
    /// The drinker's Adjust Drinking Radiation perks multiply the rads (Implant Y-3 ×
    /// 0.01; perk-entry-points.md §9) [documented: GECK wiki Entry Point, "run when the
    /// player drinks from a placed water source; modifies the radiation from placed water
    /// sources"].
    pub fn drink_from(&mut self, actor: FormId, target: FormId) {
        let Some(water) = self.drink_water(target) else { return };
        let (rads, by) = self.entry_point(actor, crate::perks::ADJUST_DRINKING_RADIATION, 1.0);
        if !by.is_empty() {
            let names: Vec<String> = by.iter().map(|a| self.data.name(a.perk)).collect();
            tracing::info!("perks: drinking from {target}: rads x {rads:.4} ({})", names.join(", "));
        }
        // SetWaterFormEffect's change first (`crate::nvse_jip8`).
        let Some(spell) = self.water_effect(water) else {
            tracing::info!("drink: {target}'s water {water} has no actor effect");
            return;
        };
        let mods = crate::aid::Mods { rads: rads.max(0.0), ..crate::aid::Mods::default() };
        self.apply_magic_item_with(actor, spell, Some(actor), false, &mods);
    }

    /// An inventory event on a scripted item (OnAdd, OnDrop, OnEquip, OnUnequip): the
    /// item's script runs in an instance per holder and item [hypothesis].
    pub fn item_event(&mut self, holder: FormId, item: FormId, ev: u16, subject: Option<Value>) -> bool {
        // The extender's OnEquip and OnUnequip (and OnActorUnequip with it): the holder and
        // the item [observed: nvse-events, (player, pistol) for all three]. Not OnAdd:
        // AddItem of a base form did not fire it [observed]; OnDrop is sent by Drop with
        // the dropped reference.
        if matches!(ev, event::ON_EQUIP | event::ON_UNEQUIP) {
            self.ext_event(ev, holder, Some(Value::Ref(item.0)));
        }
        if ev == event::ON_ADD {
            self.showoff_on_add(holder, item);
        }
        if ev == event::ON_UNEQUIP {
            use fnv_script::vm::XVal;
            self.dispatch_event("OnActorUnequip", vec![XVal::Ref(holder.0), XVal::Ref(item.0)], Some(holder));
        }
        let Some(script) = self.data.script_of_base(item) else { return false };
        if !self.data.script(script).is_some_and(|u| u.has_block(ev)) {
            return false;
        }
        let key = InstKey::Item { holder, item };
        let disguise = holder == FormId::PLAYER_REF && matches!(ev, event::ON_EQUIP | event::ON_UNEQUIP);
        let before = disguise.then(|| self.player.reputation.clone());
        let ran = self.run_event(key, script, ev, subject, RunCtx { self_ref: None, action_ref: Some(holder), quest: None });
        if let Some(before) = before {
            functions::reputation_hide(self, &before);
        }
        if ran {
            let block = self.data.table.block_name(ev).unwrap_or("?").to_string();
            let unit = self.data.script(script).map(|u| u.key()).unwrap_or_default();
            self.emit(EventKind::BlockRun { unit, block, on: Some(holder) });
        }
        ran
    }

    /// A dialogue line was said (runner input standing in for Layer 6 dialogue): its
    /// begin and end result scripts run on the speaker.
    ///
    /// The line's conditions are evaluated first (subject the speaker, target the player)
    /// and a line whose conditions fail is refused unless `force` is set: in the game such
    /// a line would not be said. Returns whether the conditions passed.
    pub fn run_info(&mut self, info: FormId, speaker: Option<FormId>, force: bool) -> Result<bool, String> {
        let data = self.data;
        let dv = self.dialogue_view();
        let def = dv.info(&info).ok_or_else(|| format!("{info} is not a dialogue line (INFO)"))?;
        let speaker = speaker.or(def.speaker);
        let pass = self.conditions_pass(&def.conditions, speaker, Some(FormId::PLAYER_REF), &format!("INFO {info}"));
        if !pass && !force {
            let why = self.explain_conditions(&def.conditions, speaker, Some(FormId::PLAYER_REF));
            return Err(format!("the conditions of {info} do not pass: {}", why.join("; ")));
        }
        if let Some(sp) = speaker {
            self.mark_said(sp, info, false);
        }
        for u in [def.begin, def.end].into_iter().flatten() {
            self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: speaker });
            self.run_result(u, RunCtx { self_ref: speaker, action_ref: None, quest: def.quest });
        }
        Ok(pass)
    }

    /// The player plays a holotape: for a Voice note, the result script of the line it
    /// plays ([`crate::holotapes::tape`]) runs, `which` 0 the begin script (as the tape
    /// starts) or 1 the end script (as it ends), on the player as `self`; the line is
    /// returned (`None` for a note that plays no line). [hypothesis: Dead Money's Dog
    /// command tape (NVDLC01DogCommandNote, line 0x0100DFC7, NVDLC01DogDialogueBackInYourCage)
    /// is its quest's only way to stage 40 of NVDLC01MQ01a, so the original runs the
    /// line's scripts; whom they run on is not known, and the DM lines name every
    /// reference they touch. docs/spec-notes/holotapes.md §3.] The end script is the one of
    /// the line the tape started with, though the start script changed the note (SetNoteTopic)
    /// [hypothesis: the tape plays the line it began].
    pub fn holotape_script(&mut self, note: FormId, which: usize) -> Option<FormId> {
        let data = self.data;
        let dv = self.dialogue_view();
        let started = self.tape_playing.take().filter(|&(n, _)| n == note && which == 1).map(|(_, i)| i);
        let info = match started {
            Some(i) => i,
            None => {
                let crate::holotapes::Tape::Voice { info, .. } = self.note_tape(note)? else { return None };
                info
            }
        };
        if which == 0 {
            self.tape_playing = Some((note, info));
        }
        let def = dv.info(&info)?;
        if let Some(u) = [def.begin, def.end][which.min(1)] {
            self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(FormId::PLAYER_REF) });
            self.run_result(u, RunCtx { self_ref: Some(FormId::PLAYER_REF), action_ref: None, quest: def.quest });
        }
        Some(info)
    }

    /// A package event (runner input standing in for Layer 6 AI): the package's result
    /// script (0 begin, 1 end, 2 change) and the actor's OnPackageStart / OnPackageDone /
    /// OnPackageChange blocks.
    pub fn package_event(&mut self, actor: FormId, package: FormId, which: usize) {
        let data = self.data;
        if which != 1 {
            self.extra.entry(actor).or_default().current_package = Some(package);
        }
        if let Some(Some(u)) = data.packages.get(&package).map(|p| p[which]) {
            self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(actor) });
            self.run_result(u, RunCtx { self_ref: Some(actor), ..Default::default() });
        }
        // The topic and the idle follow the result script, so they see what the script
        // set [documented: GECK wiki, Category:Packages, Begin/End/Change tab]; the topic
        // is said as Say says it, before the idle is asked for [hypothesis: their order;
        // docs/spec-notes/packages.md §7.2].
        if let Some(topic) = self.package_def(&package).and_then(|d| d.events[which.min(2)].topic) {
            if !self.is_dead(actor) {
                self.say(actor, None, topic, false);
            }
        }
        self.ask_package_idle(actor, package, which);
        let ev = [event::ON_PACKAGE_START, event::ON_PACKAGE_DONE, event::ON_PACKAGE_CHANGE][which];
        self.ref_event(actor, ev, Some(Value::Ref(package.0)), None);
    }

    /// `actor` arrived at patrol point `point`: the point's result script runs (the actor
    /// as `self`), then the actor says the point's topic [documented: GECK wiki Idle
    /// Markers, Patrol Data "Topic", "Result Script"; hypothesis: the order, as for
    /// package events; docs/spec-notes/packages.md §7.4].
    pub fn patrol_point_reached(&mut self, actor: FormId, point: FormId) {
        let data = self.data;
        if let Some(&u) = data.patrol_scripts.get(&point) {
            self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(actor) });
            self.run_result(u, RunCtx { self_ref: Some(actor), ..Default::default() });
        }
        if let Some(topic) = self.world.placed.get(point).and_then(|p| p.patrol_topic) {
            if !self.is_dead(actor) {
                self.say(actor, None, topic, false);
            }
        }
    }

    // AI package choice: crate::ai.

    // ---- magic effects (docs/spec-notes/scripts.md §7.6) --------------------------

    /// Apply the effects of a magic item (an ingestible the actor consumes, a spell
    /// cast on it, an ability added to it) to `target`. Each effect whose conditions
    /// pass (subject: the target) becomes an [`ActiveEffect`]; an effect whose base
    /// effect is a Script effect runs its ScriptEffectStart block at once, inside this
    /// call, in an instance of its own [observed: Nuka-Cola's cap, Cateye's Nighteye and
    /// Contreras' supplies' restored limb were there on the line after `EquipItem`,
    /// layer5-captures.md §9.3 b; hypothesis: per effect, in the item's effect order].
    ///
    /// An effect of duration 0 that is not permanent finishes inside the call as well
    /// (ScriptEffectFinish right after the start): nothing of it runs afterwards
    /// [observed: a limb damaged again after Contreras' supplies (duration 0) stayed
    /// damaged in the next 20 frames, §9.3 b; hypothesis: Finish runs inside the call
    /// rather than not at all; the supplies' blocks all do the same].
    ///
    /// The same item applied again while its effect is still active adds its duration
    /// to the active effect: one effect, one Finish [observed: a second Cateye 10.2 s
    /// after the first ended 240 s after the first, with no second Finish, §9.3 b;
    /// hypothesis: the start block does not run again, and only effects of the same
    /// item (and effect index) merge]. Returns the effects started.
    pub fn apply_magic_item(&mut self, target: FormId, source: FormId, caster: Option<FormId>, permanent: bool) -> Vec<u32> {
        self.apply_magic_item_with(target, source, caster, permanent, &crate::aid::Mods::default()).into_iter().filter(|a| !a.extended).map(|a| a.id).collect()
    }

    /// [`Game::apply_magic_item`] with the beneficial Value Modifier effects' magnitudes
    /// multiplied by `mods.potency` (an ingestible's Medicine or Food potency), the
    /// beneficial effects' durations by `mods.duration` and the rads given by
    /// `mods.rads` (perks; `crate::aid::Mods`). Returns the effects started and extended,
    /// with their value changes as applied.
    pub fn apply_magic_item_with(&mut self, target: FormId, source: FormId, caster: Option<FormId>, permanent: bool, mods: &crate::aid::Mods) -> Vec<crate::aid::Applied> {
        let data = self.data;
        let Some(items) = data.effect_items.get(&source) else { return Vec::new() };
        let mut started: Vec<crate::aid::Applied> = Vec::new();
        // The effects that took, kept here: one of duration 0 has finished by the hit check
        // and is a hit all the same [observed: gaps2, a used stimpak raises OnMagicEffectHit
        // on the player with its first effect].
        let mut took_ids: Vec<u32> = Vec::new();
        for (index, e) in items.iter().enumerate() {
            if mods.only.is_some_and(|o| o != index) {
                continue;
            }
            let label = format!("{} effect {index}", data.name(source));
            // A lasting effect whose conditions fail (an ability, a worn enchantment, or a
            // spell's effect with a duration) is listed inactive, to take hold when they
            // pass (`crate::abilities::recheck_conditions`); only an instant one is dropped.
            // PlayerConcussed is cast while the head is whole and its Reduced Perception
            // holds once the head is crippled (crippled-limbs.md §2) [hypothesis: the
            // original rechecks a spell's effects as it does an ability's].
            let pass = self.conditions_pass(&e.conditions, Some(target), caster, &label);
            if !pass && !permanent && e.duration == 0 {
                continue;
            }
            let seconds = mods.seconds(data.magic_effects.get(&e.effect), e);
            if !permanent && e.duration > 0 {
                let active = self
                    .effects
                    .iter_mut()
                    .find(|x| !x.permanent && x.target == target && x.source == source && x.index == index);
                if let Some(x) = active {
                    if mods.hit {
                        // A hit's effect starts its time again (hit-effects.md §2).
                        x.duration = x.duration.max(x.elapsed + seconds);
                    } else {
                        x.duration += seconds;
                    }
                    // The usage monitor counts every use while it lasts (aid-items.md §5).
                    if e.effect == crate::aid::USAGE_MONITOR {
                        x.usage += e.magnitude as f64;
                    }
                    let (id, duration, change) = (x.id, x.duration, x.change);
                    self.emit(EventKind::EffectExtended { id, target, source, duration });
                    started.push(crate::aid::Applied { id, effect: e.effect, change, duration, extended: true, inactive: false });
                    continue;
                }
            }
            let id = self.next_effect;
            self.next_effect += 1;
            let script = data.magic_effects.get(&e.effect).and_then(|m| m.script).filter(|s| data.scripts.contains_key(s));
            let resisted = if mods.hit { self.resisted(target, data.magic_effects.get(&e.effect)) } else { 0.0 };
            if resisted >= 1.0 {
                continue;
            }
            let change = data
                .magic_effects
                .get(&e.effect)
                .and_then(|m| crate::aid::value_change(m, e, mods.potency, permanent))
                .map(|c| mods.on_change(c))
                .map(|mut c| {
                    c.amount *= 1.0 - resisted;
                    c
                });
            let usage = if e.effect == crate::aid::USAGE_MONITOR { e.magnitude as f64 } else { 0.0 };
            self.effects.push(ActiveEffect {
                id,
                target,
                source,
                effect: e.effect,
                index,
                magnitude: e.magnitude,
                duration: seconds,
                elapsed: 0.0,
                permanent,
                caster,
                script,
                change,
                usage,
                worn: mods.worn,
                inactive: !pass,
            });
            self.emit(EventKind::EffectStarted { id, target, source, effect: e.effect });
            if !pass {
                started.push(crate::aid::Applied { id, effect: e.effect, change, duration: seconds, extended: false, inactive: true });
                continue;
            }
            self.start_change(id);
            took_ids.push(id);
            let change = self.effects.iter().find(|x| x.id == id).and_then(|x| x.change);
            started.push(crate::aid::Applied { id, effect: e.effect, change, duration: seconds, extended: false, inactive: false });
            self.effect_archetype_start(id);
            self.effect_event(id, event::SCRIPT_EFFECT_START, 0.0);
            if !permanent && e.duration == 0 && self.effects.iter().any(|x| x.id == id) {
                self.finish_effect(id, true);
            }
        }
        // The target's OnMagicEffectHit for each effect that took (not abilities and worn
        // enchantments, which are not hits) [hypothesis: after the effects start].
        if !permanent {
            let took = |id: u32| took_ids.contains(&id) || self.effects.iter().any(|x| x.id == id && !x.inactive);
            let mut hit: Vec<FormId> = started.iter().filter(|a| a.extended || took(a.id)).map(|a| a.effect).collect();
            hit.dedup();
            for e in hit {
                self.ref_event(target, event::ON_MAGIC_EFFECT_HIT, Some(Value::Ref(e.0)), caster);
            }
        }
        started
    }

    /// The player (or an actor) consumes an ingestible (ALCH) from its inventory: one is
    /// removed and it is used ([`Game::ingest`]: its effects with the item's potency, the
    /// usage monitor and the addiction roll). EquipItem of an aid item does the same.
    pub fn consume(&mut self, actor: FormId, item: FormId) -> Result<Vec<u32>, String> {
        let removed = self.world.remove_item(actor, item, 1).map_err(|e| e.to_string())?;
        if removed == 0 {
            return Err(format!("{} carries no {}", self.data.name(actor), self.data.name(item)));
        }
        self.emit(EventKind::ItemRemoved { from: actor, item, count: 1, silent: false });
        Ok(self.ingest(actor, item).effects)
    }

    /// ScriptEffectStart of an effect taking hold later (`crate::abilities`).
    pub(crate) fn effect_started(&mut self, id: u32) {
        self.effect_event(id, event::SCRIPT_EFFECT_START, 0.0);
    }

    /// ScriptEffectFinish of an effect letting go while it stays listed.
    pub(crate) fn effect_finished(&mut self, id: u32) {
        self.effect_event(id, event::SCRIPT_EFFECT_FINISH, 0.0);
    }

    /// Run an effect's script block (if its script has one) on the effect's target.
    fn effect_event(&mut self, id: u32, ev: u16, elapsed: f64) {
        self.effect_event_at(id, None, ev, elapsed);
    }

    /// [`Game::effect_event`] with the effect's position, if known (`hint`, checked by id:
    /// [`effect_index`]).
    fn effect_event_at(&mut self, id: u32, hint: Option<usize>, ev: u16, elapsed: f64) {
        let Some(i) = effect_index(&self.effects, id, hint) else { return };
        let e = &self.effects[i];
        let (Some(script), target, caster) = (e.script, e.target, e.caster) else { return };
        if !self.data.script(script).is_some_and(|u| u.has_block(ev)) {
            return;
        }
        let before = std::mem::replace(&mut self.effect_elapsed, elapsed);
        if ev == event::SCRIPT_EFFECT_UPDATE {
            self.mark_run(InstKey::Effect { id }, Some(script));
        }
        self.run_event(InstKey::Effect { id }, script, ev, None, RunCtx { self_ref: Some(target), action_ref: caster, quest: None });
        self.effect_elapsed = before;
    }

    /// End an effect: its ScriptEffectFinish block runs (when `run_finish`), then it and
    /// its script instance are dropped.
    pub fn finish_effect(&mut self, id: u32, run_finish: bool) {
        if !self.effects.iter().any(|x| x.id == id) {
            return;
        }
        // An inactive effect never started: nothing of it runs.
        let inactive = self.effects.iter().any(|x| x.id == id && x.inactive);
        if run_finish && !inactive {
            self.effect_event(id, event::SCRIPT_EFFECT_FINISH, 0.0);
        }
        self.end_change(id);
        if let Some(i) = self.effects.iter().position(|x| x.id == id) {
            let e = self.effects.remove(i);
            self.instances.remove(&InstKey::Effect { id });
            self.emit(EventKind::EffectFinished { id, target: e.target, source: e.source });
            self.drug_wore_off(e.target, e.source);
        }
    }

    /// The last effect of a chem (ALCH of equip type Chems) the player took has ended: the
    /// default objects' Drug Wears Off Image Space plays [documented: the DOBJ entry's
    /// name; hypothesis: when, docs/spec-notes/default-objects.md].
    fn drug_wore_off(&mut self, target: FormId, source: FormId) {
        let chem = self.data.ingestibles.get(&source).is_some_and(|i| i.equip_type == crate::data::IngestibleDef::CHEMS);
        if target != FormId::PLAYER_REF || !chem || self.effects.iter().any(|x| x.target == target && x.source == source) {
            return;
        }
        if let Some(m) = self.data.default_object(crate::data::DefaultObject::DrugWearsOffImageSpace).filter(|m| self.data.image_modifiers.contains_key(m)) {
            self.apply_image_modifier(m);
        }
    }

    /// One frame of active effects (game time only): each runs ScriptEffectUpdate with
    /// ScriptEffectElapsedSeconds = the frame time, then an effect whose duration has
    /// passed finishes. Observed (layer5-captures.md §9.3 b): Update runs every frame
    /// (an ability restoring ScriptEffectElapsedSeconds / 10 health changed it in 59 of
    /// 59 frame steps, at 0.1 HP per second of script time, so Elapsed is the time since
    /// the previous Update, as documented on the GECK wiki); an open menu stops Update
    /// and the duration; Cateye's 120 s effect finished 119.82–119.94 s of
    /// GetSecondsPassed time after the use. Hypothesis: the effects run after the
    /// frame's quest and object scripts, and effects on dead actors keep running.
    /// Active effects advance by `seconds` at once, as if that much game-mode time had
    /// passed in one frame (a wait or a sleep, `crate::needs`): each runs one
    /// ScriptEffectUpdate with those seconds, a value changed by the second changes by
    /// the seconds inside its duration, and the effects whose duration has passed
    /// finish [hypothesis: aid-items.md §6].
    pub fn advance_effects(&mut self, seconds: f64) {
        if seconds > 0.0 {
            self.update_effects(seconds);
        }
    }

    fn update_effects(&mut self, dt: f64) {
        self.refresh_suspended(dt);
        self.recheck_conditions();
        // An inactive effect's time runs all the same, and it goes when its duration has
        // passed, with no update or finish of its own: a Hardcore Stimpak's heal over time
        // used outside Hardcore lasts its 6 s inactive and is gone, rather than waiting to
        // take hold whenever Hardcore is turned on (#1438) [hypothesis: the original
        // counts a lasting effect's duration whether or not its conditions pass;
        // aid-items.md §6].
        let idle: Vec<u32> = self.effects.iter().filter(|e| e.inactive && !e.permanent).map(|e| e.id).collect();
        for id in idle {
            let Some(e) = self.effects.iter_mut().find(|x| x.id == id) else { continue };
            e.elapsed += dt;
            if e.elapsed + 1e-9 >= e.duration {
                self.finish_effect(id, true);
            }
        }
        let ids: Vec<u32> = self.effects.iter().filter(|e| !e.inactive).map(|e| e.id).collect();
        // Each effect is looked up once (#1389): the ids are in list order, so the next one
        // is searched from where the last one was; its position then goes to the value
        // change and the script, which check it by id, as a callback (a dispel, a cast)
        // may have moved it.
        let mut cursor = 0;
        for id in ids {
            let Some(i) = effect_from(&self.effects, id, cursor) else { continue };
            cursor = i;
            let e = &mut self.effects[i];
            let before = e.elapsed;
            e.elapsed += dt;
            let done = !e.permanent && e.elapsed + 1e-9 >= e.duration;
            self.tick_change_at(id, Some(i), before);
            self.effect_event_at(id, Some(i), event::SCRIPT_EFFECT_UPDATE, dt);
            if done {
                self.finish_effect(id, true);
            }
        }
    }

    /// Remove the effects of `target` that `pick` selects. An effect whose own script is
    /// running stops that script like a return and does not run its finish block
    /// [documented: GECK wiki Dispel]; the others run ScriptEffectFinish [hypothesis].
    pub fn dispel(&mut self, target: FormId, running: Option<InstKey>, pick: impl Fn(&ActiveEffect) -> bool) -> usize {
        let ids: Vec<u32> = self.effects.iter().filter(|e| e.target == target && pick(e)).map(|e| e.id).collect();
        for &id in &ids {
            let own = running == Some(InstKey::Effect { id });
            if own {
                self.request_return();
            }
            self.finish_effect(id, !own);
        }
        ids.len()
    }

    // ---- image space modifiers ---------------------------------------------------

    /// ApplyImageSpaceModifier: the modifier starts playing. One that is already on the
    /// screen starts again from its beginning [hypothesis: one instance per modifier;
    /// image-modifiers.md §3].
    pub fn apply_image_modifier(&mut self, modifier: FormId) {
        self.image_modifiers.retain(|m| m.modifier != modifier);
        self.image_modifiers.push(ActiveImageModifier { modifier, elapsed: 0.0 });
        self.emit(EventKind::ImageModifierApplied { modifier });
    }

    /// RemoveImageSpaceModifier: whether the modifier was on the screen.
    pub fn remove_image_modifier(&mut self, modifier: FormId) -> bool {
        let before = self.image_modifiers.len();
        self.image_modifiers.retain(|m| m.modifier != modifier);
        let was = self.image_modifiers.len() != before;
        if was {
            self.emit(EventKind::ImageModifierRemoved { modifier, ended: false });
        }
        was
    }

    /// IsImageSpaceActive.
    pub fn image_modifier_active(&self, modifier: FormId) -> bool {
        self.image_modifiers.iter().any(|m| m.modifier == modifier)
    }

    /// Age the modifiers; an animatable one whose duration has passed ends [hypothesis
    /// from the shipped scripts: 29 lines wait for IsImageSpaceActive to read 0 on
    /// FadeToBlackISFX (4 s), which no script removes; image-modifiers.md §3].
    fn update_image_modifiers(&mut self, dt: f64) {
        let data = self.data;
        let mut ended = Vec::new();
        self.image_modifiers.retain_mut(|m| {
            m.elapsed += dt;
            let over = data.image_modifiers.get(&m.modifier).is_some_and(|d| d.animatable && m.elapsed + 1e-9 >= d.duration as f64);
            if over {
                ended.push(m.modifier);
            }
            !over
        });
        for modifier in ended {
            self.emit(EventKind::ImageModifierRemoved { modifier, ended: true });
        }
    }

    // ---- conditions --------------------------------------------------------------

    /// Evaluate a condition list (`subject` and `target` for the run-on choices).
    pub fn conditions_pass(&mut self, conds: &[Condition], subject: Option<FormId>, target: Option<FormId>, label: &str) -> bool {
        if conds.is_empty() {
            return true;
        }
        let mut results = Vec::with_capacity(conds.len());
        let outer = std::mem::replace(&mut self.condition_target, target);
        for c in conds {
            let on = match c.run_on {
                RunOn::Subject => subject,
                RunOn::Target => target,
                RunOn::Reference(r) => r.map(FormId),
                RunOn::LinkedReference => subject.and_then(|s| self.world.placed.get(s)).and_then(|p| p.linked),
                RunOn::CombatTarget => subject.and_then(|s| self.combat_target_of(s)),
                RunOn::Other(_) => None,
            };
            let v = self.condition_value(c, on, label);
            let rhs = match c.rhs {
                Operand::Value(x) => x,
                Operand::Global(Some(g)) => self.world.global(FormId(g)).map(|v| v as f64).unwrap_or(0.0),
                Operand::Global(None) => 0.0,
            };
            results.push((c.cmp.test(v, rhs), c.or));
        }
        self.condition_target = outer;
        cond::combine(&results)
    }

    /// The buttons of a message (indices in record order) whose conditions fail when it
    /// is shown: they are not shown, and the others keep their indices [documented: GECK
    /// wiki GetButtonPressed]. The conditions run on the player as subject and the
    /// reference whose script shows the message as target [hypothesis: the craft and
    /// repair buttons of Doc Mitchell's house test the player's skills with a fixed
    /// PlayerRef run-on, so the choice matters only for Subject/Target conditions].
    pub fn hidden_buttons(&mut self, message: FormId, shown_by: Option<FormId>) -> Vec<usize> {
        let data = self.data;
        let Some(def) = data.messages.get(&message) else { return Vec::new() };
        let mut hidden = Vec::new();
        for (i, c) in def.button_conditions.iter().enumerate() {
            // A list a script changed (One111's SetConditionFormTwo…, `crate::nvse_one111`).
            let changed = self.one111.lists.get(&crate::nvse_one111::ListKey::Two(message, i as i32)).cloned();
            let c = changed.as_ref().unwrap_or(c);
            if !c.is_empty() && !self.conditions_pass(c, Some(FormId::PLAYER_REF), shown_by, &format!("MESG {message} button {i}")) {
                hidden.push(i);
            }
        }
        hidden
    }

    /// Each condition with its value, for diagnostics ("GetStage(0x…) = 10 == 20: false").
    pub fn explain_conditions(&mut self, conds: &[Condition], subject: Option<FormId>, target: Option<FormId>) -> Vec<String> {
        let data = self.data;
        let mut out = Vec::new();
        for c in conds {
            let on = match c.run_on {
                RunOn::Subject => subject,
                RunOn::Target => target,
                RunOn::Reference(r) => r.map(FormId),
                RunOn::LinkedReference => subject.and_then(|s| self.world.placed.get(s)).and_then(|p| p.linked),
                RunOn::CombatTarget => subject.and_then(|s| self.combat_target_of(s)),
                RunOn::Other(_) => None,
            };
            let v = self.condition_value(c, on, "explain");
            let rhs = match c.rhs {
                Operand::Value(x) => x,
                Operand::Global(Some(g)) => self.world.global(FormId(g)).map(|v| v as f64).unwrap_or(0.0),
                Operand::Global(None) => 0.0,
            };
            let name = data.table.get(c.opcode()).map(|f| f.name.clone()).unwrap_or_else(|| format!("0x{:04X}", c.opcode()));
            let args: Vec<String> = c.params.iter().map(|a| arg_json(data, a).to_string()).collect();
            out.push(format!("{name}({}) = {v} {:?} {rhs}: {}{}", args.join(", "), c.cmp, c.cmp.test(v, rhs), if c.or { " OR" } else { "" }));
        }
        out
    }

    pub(crate) fn condition_value(&mut self, c: &Condition, on: Option<FormId>, label: &str) -> f64 {
        let ctx = Ctx {
            unit: label.into(),
            opcode: c.opcode(),
            on,
            self_ref: on,
            action_ref: None,
            quest: None,
            inst: None,
            in_condition: true,
        };
        // IsLastIdlePlayed has a condition index but no script opcode (`crate::idles`).
        if c.function == crate::idles::IS_LAST_IDLE_PLAYED {
            return crate::idles::is_last_idle_played(self, on, &c.params);
        }
        // The VATS area functions have condition indices only, too (`crate::vats`).
        if crate::vats::is_area_function(c.function) {
            return crate::vats::area_value(self, c.function);
        }
        // A null form parameter (FormID 0 in the CTDA) reads 0 without calling the
        // function, so no failed call is logged (#966) [observed for GetIsID and
        // GetQuestVariable, dialogue.md "A null form parameter"; hypothesis for the rest].
        if c.params.iter().any(|p| matches!(p, ArgValue::Value(Value::Ref(0)))) {
            return 0.0;
        }
        self.call_function(&ctx, &c.params).num()
    }

    /// Dispatch a function by opcode (scripts and conditions share it).
    pub fn call_function(&mut self, ctx: &Ctx, args: &[ArgValue]) -> Value {
        let data = self.data;
        let name = || data.table.get(ctx.opcode).map(|f| f.name.clone()).unwrap_or_else(|| format!("0x{:04X}", ctx.opcode));
        // A console function (a number of ours, no opcode of the game's) by the function's catalogue name.
        let console = || (ctx.opcode >= fnv_script::table::CONSOLE_OPCODE_BASE).then(|| data.table.get(ctx.opcode)).flatten().and_then(|f| functions::lookup_console(&f.name));
        match functions::lookup(ctx.opcode).or_else(console) {
            Some(Kind::Impl(f)) => {
                let outer = functions::begin_unread();
                let r = f(self, ctx, &Args(args));
                self.call_result(ctx, r, functions::end_unread(outer))
            }
            Some(Kind::NoOp(_)) => Value::ZERO,
            Some(Kind::Stub(_)) => {
                *self.stubbed.entry(ctx.opcode).or_default() += 1;
                self.stub_note(ctx, args);
                Value::ZERO
            }
            None => {
                *self.unhandled.entry(ctx.opcode).or_default() += 1;
                if self.unhandled_seen.insert((ctx.opcode, ctx.unit.to_string())) {
                    let n = name();
                    tracing::debug!(opcode = ctx.opcode, name = %n, unit = %ctx.unit, "unhandled script function");
                    self.emit(EventKind::Unhandled { opcode: format!("0x{:04X}", ctx.opcode), name: n, unit: ctx.unit.to_string() });
                }
                Value::ZERO
            }
        }
    }

    /// A function's result: an error traced as `CallFailed`, the arguments it could not
    /// read in a call that succeeded as `ArgUnread` (`functions::Unread`).
    fn call_result(&mut self, ctx: &Ctx, r: Result<Value, String>, unread: Vec<functions::Unread>) -> Value {
        let data = self.data;
        let name = || data.table.get(ctx.opcode).map(|f| f.name.clone()).unwrap_or_else(|| format!("0x{:04X}", ctx.opcode));
        match r {
            Ok(v) => {
                // Read wrong without failing: the call went on with a default.
                for u in unread {
                    if self.arg_unread_seen.insert((ctx.opcode, ctx.unit.to_string(), u.param)) {
                        let n = name();
                        self.emit(EventKind::ArgUnread {
                            opcode: format!("0x{:04X}", ctx.opcode),
                            name: n,
                            unit: ctx.unit.to_string(),
                            param: u.param,
                            got: u.got.into(),
                            wanted: u.wanted.into(),
                        });
                    }
                }
                v
            }
            Err(e) => {
                let n = name();
                self.emit(EventKind::CallFailed { opcode: format!("0x{:04X}", ctx.opcode), name: n, unit: ctx.unit.to_string(), error: e });
                Value::ZERO
            }
        }
    }

    /// Trace a call to a function that is implemented for its Layer 5 part but whose
    /// effect belongs to Layer 6 (e.g. AddScriptPackage).
    pub fn stub_note(&mut self, ctx: &Ctx, args: &[ArgValue]) {
        let data = self.data;
        let name = data.table.get(ctx.opcode).map(|f| f.name.clone()).unwrap_or_default();
        let args: Vec<serde_json::Value> = args.iter().map(|a| arg_json(data, a)).collect();
        let on = ctx.on.or(ctx.self_ref);
        let sig = format!("{on:?}{args:?}");
        if !self.stubs_seen.insert((ctx.opcode, ctx.unit.to_string(), sig)) {
            return;
        }
        self.emit(EventKind::Stub { opcode: format!("0x{:04X}", ctx.opcode), name, unit: ctx.unit.to_string(), on, args });
    }

    // ---- snapshots -----------------------------------------------------------------

    pub fn snapshot(&self) -> GameSnapshot {
        let mut sim = self.world.snapshot();
        // SetOwnershipTemp's owners are not saved: the owner before it is (`nvse_showoff4`).
        let mut forms = self.forms.clone();
        for (&t, &before) in &self.showoff.temp_owners {
            if let Some(st) = sim.refs.get_mut(&t) {
                st.extra.owner = before;
            } else {
                match before {
                    Some(o) => forms.cell_owners.insert(t, o),
                    None => forms.cell_owners.remove(&t),
                };
            }
        }
        GameSnapshot {
            format: SNAPSHOT_FORMAT.into(),
            ext: self.ext.clone(),
            lambdas: self.lambdas.iter().map(|d| SavedLambda { unit: d.key.clone(), lambda: d.lambda.clone(), inst: d.inst }).collect(),
            // A created reference deleted since keeps no variables: its FormID is not
            // used again, so they could never be read.
            nx: crate::nvse_nx::NxVars(self.nx.0.iter().filter(|(id, _)| !sim.deleted_created.contains_key(id)).map(|(k, v)| (*k, v.clone())).collect()),
            sim,
            quests: self.quests.clone(),
            instances: self.instances.iter().map(|(k, v)| (*k, v.clone())).collect(),
            extra: self.extra.clone(),
            player: self.player.clone(),
            form_lists: self.form_lists.clone(),
            loaded_cells: self.loaded_cells.clone(),
            moved: self.moved.clone(),
            menu: self.menu,
            loading: self.loading,
            message_box: self.message_box.clone(),
            boxes: self.boxes.clone(),
            queued: self.queued.clone(),
            speech: self.speech.clone(),
            planted: self.pickpocket.planted.clone(),
            said: self.said.clone(),
            said_day: self.said_day.clone(),
            said_day_days_clock: true,
            dialogue: self.dialogue.clone(),
            frame: self.frame,
            time: self.time,
            pending_onload: self.pending_onload.clone(),
            rng_word_pos: self.rng.get_word_pos().to_string(),
            forms,
            effects: self.effects.clone(),
            next_effect: self.next_effect,
            image_modifiers: self.image_modifiers.clone(),
            locks: self.locks.clone(),
            terminals: self.terminals.access.clone(),
            terminal_retries: self.terminals.retried.clone(),
            terminal_locks: self.terminals.script_locks.clone(),
            pipboy_radio_on: self.radio.pipboy.on,
            pipboy_station: self.radio.pipboy.station,
            npc_radios: self.radio.npc_radios.clone(),
            weather_shown: self.weather.shown,
            raining: self.weather.raining,
            grudges: self.provocations.grudges.iter().map(|(k, v)| (*k, *v)).collect(),
            forgiven: self.provocations.forgiven.clone(),
            nav_switches: self.nav_switches.clone(),
            securitron_faces: self.texture_swaps.securitron.clone(),
            fade_moves: self.script_looks.fade_moves.clone(),
            coop: self.coop.clone(),
            due_children: self.due_children.clone(),
            menu_under: self.menu_under,
            tutorial: self.tutorial.clone(),
            say_to_unsaid: self.say_to_unsaid.clone(),
            queued_ref_events: self.queued_ref_events.clone(),
            door_open_seen: self.door_open_seen.iter().filter(|&(&r, &seen)| self.world.peek(r).is_some_and(|s| s.open != seen)).map(|(&r, &seen)| (r, seen)).collect(),
            awareness: self.awareness.actors.iter().filter(|(_, a)| a.state.in_combat() || !a.targets.is_empty() || !a.alarms.is_empty() || a.fighting.is_some()).map(|(&r, a)| (r, a.clone())).collect(),
            fighting: Some(self.fighting.clone()),
            forced_combat: self.awareness.forced.clone(),
            form_instances: self.pp.instances.iter().map(|(k, v)| (*k, v.clone())).collect(),
            object_groups: self.anims.asked_all().clone(),
            extra_misc_stats: self.jip2.misc_stats.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        }
    }

    pub fn restore(data: &'a GameData, world: World<'a>, cfg: GameConfig, s: &GameSnapshot) -> Result<Game<'a>, String> {
        s.check()?;
        let mut g = Game::new(data, world, cfg);
        g.ext = s.ext.clone();
        for l in &s.lambdas {
            // A lambda whose unit is gone (a plugin removed, a console line) keeps its place,
            // so the later ones keep their names; calling it fails.
            let unit = data.unit_by_key.get(&l.unit).map(|&i| &data.units[i]);
            // Made again after the load, it is the same lambda, but only while the unit's
            // lambda of that number is still this code: a save from before the numbering
            // reads every lambda as number 0, and an edited plugin can number other code
            // the same; those get a new ID with the current code when made.
            if let Some(u) = unit.filter(|u| unit_lambda(u, &l.lambda).is_some_and(|now| now == l.lambda)) {
                g.lambda_ids.entry(lambda_key(u, &l.lambda, l.inst)).or_insert(LAMBDA_BASE + g.lambdas.len() as u32);
            }
            g.lambdas.push(LambdaDef { unit, key: l.unit.clone(), lambda: l.lambda.clone(), inst: l.inst, captured: None });
        }
        g.nx = s.nx.clone();
        g.quests = s.quests.clone();
        g.instances = s.instances.iter().cloned().collect();
        g.extra = s.extra.clone();
        g.player = s.player.clone();
        // The world does not save the level it rolls at.
        g.sync_player_level();
        g.form_lists = s.form_lists.clone();
        g.loaded_cells = s.loaded_cells.clone();
        g.moved = s.moved.clone();
        g.menu = s.menu;
        g.loading = s.loading;
        g.message_box = s.message_box.clone();
        g.boxes = s.boxes.clone();
        g.queued = s.queued.clone();
        g.box_serial = g.queued.iter().map(|q| q.serial).max().unwrap_or(0);
        // An older snapshot has no `queued`: each box its own showing, buttons not known.
        g.align_queued();
        g.speech = s.speech.clone();
        g.pickpocket.planted = s.planted.clone();
        g.said = s.said.clone();
        if s.said_day_days_clock {
            g.said_day = s.said_day.clone();
        }
        g.dialogue = s.dialogue.clone();
        g.frame = s.frame;
        g.time = s.time;
        g.pending_onload = s.pending_onload.clone();
        g.forms = s.forms.clone();
        g.effects = s.effects.clone();
        g.next_effect = s.next_effect.max(1);
        g.image_modifiers = s.image_modifiers.clone();
        g.locks = s.locks.clone();
        g.terminals.access = s.terminals.clone();
        g.terminals.retried = s.terminal_retries.clone();
        g.terminals.script_locks = s.terminal_locks.clone();
        g.radio.pipboy.on = s.pipboy_radio_on;
        g.radio.pipboy.station = s.pipboy_station;
        g.radio.npc_radios = s.npc_radios.clone();
        g.weather.shown = s.weather_shown;
        g.weather.raining = s.raining;
        g.provocations.grudges = s.grudges.iter().copied().collect();
        g.provocations.forgiven = s.forgiven.clone();
        g.nav_switches = s.nav_switches.clone();
        g.due_children = s.due_children.clone();
        g.menu_under = s.menu_under;
        g.tutorial = s.tutorial.clone();
        g.say_to_unsaid = s.say_to_unsaid.clone();
        g.queued_ref_events = s.queued_ref_events.clone();
        g.door_open_seen = s.door_open_seen.clone();
        g.pp_restore_instances(&s.form_instances);
        g.anims.restore_asked(s.object_groups.clone());
        g.jip2.misc_stats = s.extra_misc_stats.iter().map(|(k, v)| (k.clone(), *v)).collect();
        g.awareness.actors = s.awareness.clone();
        g.awareness.forced = s.forced_combat.clone();
        // A save from before `fighting` was saved cannot say which starts had run: everyone
        // in Combat or Lost counts as started, Alert (no fight yet) does not [design:
        // docs/design/save-load.md §3.2]. A start still due in such a save does not run.
        g.fighting = s.fighting.clone().unwrap_or_else(|| s.awareness.iter().filter(|(_, a)| matches!(a.state, crate::awareness::State::Combat | crate::awareness::State::Lost)).map(|(&r, _)| r).collect());
        g.coop =crate::coop::checked(s.coop.clone()).map_err(|e| e.to_string())?;
        for (&on, texture) in &s.securitron_faces {
            g.texture_swaps.set_securitron(on, texture.clone());
        }
        // Moves a save caught during their fade are made at once: the restored engine
        // draws the reference where it ends up [design].
        for (r, m) in &s.fade_moves {
            let _ = crate::functions::refs::move_ref(&mut g, *r, m.marker, m.offset);
        }
        let pos: u128 = s.rng_word_pos.parse().map_err(|_| format!("bad rng_word_pos {:?}", s.rng_word_pos))?;
        g.rng.set_word_pos(pos);
        Ok(g)
    }
}

/// Seconds of a line per response without its voice file: a base plus a time per
/// character of the response text. The game takes the length from the voice file; these
/// two numbers are fitted to the observed spans of Doc Mitchell's lines [hypothesis:
/// one line of 29 characters took 2.9 s, three of 193 characters 15.4 s and two of 129
/// characters 10 s from the stage before them to the stage their end script set,
/// layer5-captures.md §10.2–10.3; the fit gives 2.9, 15.4 and 10.3 s].
pub const LINE_BASE_SECONDS: f64 = 1.1;
pub const LINE_SECONDS_PER_CHAR: f64 = 1.0 / 16.0;

/// The fitted length of one response of `chars` characters ([`LINE_BASE_SECONDS`],
/// [`LINE_SECONDS_PER_CHAR`]). The runtime times responses through
/// [`Game::response_seconds`] only, which falls back to this.
pub fn fitted_seconds(chars: usize) -> f64 {
    LINE_BASE_SECONDS + chars as f64 * LINE_SECONDS_PER_CHAR
}

/// The fitted length of a line from its responses' character counts (for callers that
/// have no game state, such as the subtitle display; the runtime uses
/// [`Game::line_seconds`]).
pub fn line_seconds(response_chars: &[usize]) -> f64 {
    if response_chars.is_empty() {
        return fitted_seconds(0);
    }
    response_chars.iter().map(|&c| fitted_seconds(c)).sum()
}

/// An argument as trace JSON (forms by FormID, with the Editor ID when known).
pub fn arg_json(data: &GameData, a: &ArgValue) -> serde_json::Value {
    use serde_json::json;
    match a {
        ArgValue::Value(Value::Ref(f)) => match data.edid_of.get(&FormId(*f)) {
            Some(e) => json!(format!("{} ({e})", FormId(*f))),
            None => json!(FormId(*f).to_string()),
        },
        ArgValue::Value(Value::Num(x)) => json!(x),
        ArgValue::Enum(c) => json!({"enum": c}),
        ArgValue::Axis(c) => json!(c.to_string()),
        ArgValue::Str(s) => json!(s),
        ArgValue::Vars(v) => json!(v.iter().map(|x| x.num()).collect::<Vec<_>>()),
        ArgValue::Int32(i) => json!(i),
    }
}

/// The position of effect `id` in `effects`: `hint` when the effect there has that id,
/// else the first with it. Effect ids are unique [design: each is taken from
/// `Game::next_effect`, which only grows, in `Game::apply_magic_item_with`, and a save
/// keeps the counter], so a checked hint is the effect a search finds (#1389).
///
/// Known limits, where ids could repeat and a hint could pick a later copy than a search
/// from the start: a save whose `next_effect` is at or below an id it lists (a hand-edited
/// or broken save; the next cast reuses the id), a save that lists one id twice, and the
/// `u32` counter wrapping after 2^32 effects. Nothing guards against them; with a
/// repeated id, the searches before #1389 found its first copy and a hint may find another.
pub(crate) fn effect_index(effects: &[ActiveEffect], id: u32, hint: Option<usize>) -> Option<usize> {
    match hint {
        Some(i) if effects.get(i).is_some_and(|e| e.id == id) => Some(i),
        _ => effects.iter().position(|e| e.id == id),
    }
}

/// The position of effect `id`, searched from `from` on, then before it (a callback
/// removed effects ahead of it); with unique ids ([`effect_index`]) the same effect a
/// search from the start finds.
fn effect_from(effects: &[ActiveEffect], id: u32, from: usize) -> Option<usize> {
    let from = from.min(effects.len());
    effects[from..].iter().position(|e| e.id == id).map(|i| from + i).or_else(|| effects[..from].iter().position(|e| e.id == id))
}

/// The host the VM runs against for one script run.
struct Exec<'g, 'a> {
    game: &'g mut Game<'a>,
    unit: &'a Compiled,
    locals: Locals,
    ctx: RunCtx,
}

impl Exec<'_, '_> {
    fn inst_key(&self) -> Option<InstKey> {
        match self.locals {
            Locals::Instance(k) => Some(k),
            Locals::Temp(_) => None,
        }
    }

    fn global_form(&mut self, g: RefIndex) -> Result<FormId, String> {
        match self.unit.unit.slot(g) {
            Some(RefSlot::Form(f)) => Ok(FormId(f)),
            other => Err(format!("global slot {} is {other:?}", g.0)),
        }
    }

    fn remote(&mut self, r: RefIndex) -> Result<(InstKey, FormId), String> {
        let f = self.ref_value(r)?.as_form().ok_or_else(|| format!("reference slot {} is empty", r.0))?;
        self.game.instance_of(FormId(f)).ok_or_else(|| format!("{} runs no script", self.game.data.name(FormId(f))))
    }
}

impl Host for Exec<'_, '_> {
    fn var_kind(&mut self, var: &VarRef) -> fnv_script::vm::VarKind {
        use fnv_script::vm::VarKind;
        let VarSlot::Local { index, .. } = var.var else { return VarKind::Value };
        let ty = match var.on {
            None => self.unit.var_type(index).cloned(),
            Some(r) => self.remote(r).ok().and_then(|(_, s)| self.game.data.script(s)).and_then(|c| c.var_type(index).cloned()),
        };
        match ty {
            Some(VarType::Other(t)) if t == "string_var" => VarKind::String,
            Some(VarType::Other(t)) if t == "array_var" => VarKind::Array,
            _ => VarKind::Value,
        }
    }

    fn ext(&mut self) -> Option<&mut fnv_script::vm::ExtStore> {
        Some(&mut self.game.ext)
    }

    /// The form's name, a reference's its base form's [documented: ToString]; the player's
    /// the name typed in the name menu [observed: probe strings/04 gives the player's name].
    fn form_text(&mut self, form: u32) -> Option<String> {
        let f = FormId(form);
        if f == FormId::PLAYER_REF {
            if let Some(n) = self.game.player.name.clone() {
                return Some(n);
            }
        }
        let g = &*self.game;
        let named = |x: FormId| g.name_overrides.get(&x).or_else(|| g.data.full_names.get(&g.unclone(x))).cloned();
        named(f).or_else(|| crate::functions::base_of(g, f).and_then(named))
    }

    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        match (var.on, var.var) {
            (_, VarSlot::Global { global }) => {
                let g = self.global_form(global)?;
                Ok(Value::Num(self.game.world.global(g).map_err(|e| e.to_string())? as f64))
            }
            (None, VarSlot::Local { index, .. }) => Ok(match &self.locals {
                Locals::Temp(m) => m.get(&index).copied().unwrap_or_default(),
                Locals::Instance(k) => self.game.var(*k, index),
            }),
            (Some(r), VarSlot::Local { index, .. }) => {
                let (key, script) = self.remote(r)?;
                if self.game.data.script(script).is_some_and(|c| c.var_type(index).is_none()) {
                    let unit = self.unit.key();
                    self.game.emit(EventKind::UndeclaredVariable { owner: key.label(), index, unit });
                }
                Ok(self.game.var(key, index))
            }
        }
    }

    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        let short = |l| matches!(l, fnv_script::ir::VarLetter::Short);
        match (var.on, var.var) {
            (_, VarSlot::Global { global }) => {
                let g = self.global_form(global)?;
                let v = value.num() as f32;
                self.game.world.set_global(g, v).map_err(|e| e.to_string())?;
                self.game.emit(EventKind::GlobalSet { global: g, value: v });
            }
            (None, VarSlot::Local { index, letter }) => match &mut self.locals {
                Locals::Temp(m) => {
                    let int = matches!(self.unit.var_type(index), Some(VarType::Int)) || (self.unit.var_type(index).is_none() && short(letter));
                    m.insert(index, if int { Value::Num(to_int_storage(value.num()) as f64) } else { value });
                }
                Locals::Instance(k) => {
                    let k = *k;
                    let script = self.game.instances.get(&k).and_then(|i| i.script);
                    let unit = self.unit.key();
                    self.game.store(k, script, index, value, &unit, short(letter));
                    if self.game.cfg.trace_locals {
                        let value = self.game.var(k, index);
                        let name = self.game.var_name(script, index);
                        self.game.emit(EventKind::VarSet { owner: k.label(), index, name, value });
                    }
                }
            },
            (Some(r), VarSlot::Local { index, letter }) => {
                let (key, script) = self.remote(r)?;
                let unit = self.unit.key();
                self.game.store(key, Some(script), index, value, &unit, short(letter));
                let value = self.game.var(key, index);
                let name = self.game.var_name(Some(script), index);
                self.game.emit(EventKind::VarSet { owner: key.label(), index, name, value });
            }
        }
        Ok(())
    }

    fn ref_value(&mut self, r: RefIndex) -> Result<Value, String> {
        match self.unit.unit.slot(r) {
            Some(RefSlot::Form(f)) => Ok(Value::Ref(f)),
            Some(RefSlot::Local(i)) => {
                let v = VarRef { on: None, var: VarSlot::Local { index: i as u16, letter: fnv_script::ir::VarLetter::Float } };
                self.get_var(&v)
            }
            Some(RefSlot::Unmapped(l)) => Err(format!("reference slot {} holds unmapped FormID 0x{l:08X}", r.0)),
            None => Err(format!("reference slot {} is not in the list", r.0)),
        }
    }

    fn call(&mut self, opcode: u16, on: Option<Value>, args: &[ArgValue]) -> Result<Value, String> {
        let on = match on {
            Some(v) => match v.as_form() {
                Some(f) => Some(FormId(f)),
                None => {
                    // A call on an empty reference variable is skipped: it gives 0 and
                    // the script goes on [observed: o5_00, a function script went on to
                    // its next line, alone and in an `if`; the Nellis artillery's trigger
                    // script (NellisArtilleryScript) calls CurrentActor2.GetDead with the
                    // variable unset every frame one actor is inside, and the original's
                    // barrage fires (owner, issue #1600). The GECK wiki's Ref Variable,
                    // Eval and Let pages say it stops the script: until 2026-11-05 it did
                    // here, and the barrage never came; script-language.md §7.3 row 1].
                    // Traced as `call_failed`. Classic `&&` and `||` evaluate both sides,
                    // so a reference guarded by `&&` (EDEScript) still makes this call
                    // every frame, in the original too [observed: o5_01, observer-checks.md
                    // check 51]. A script record's failure is traced once per function,
                    // not every frame.
                    if let fnv_script::UnitOwner::Script { form } = self.unit.unit.owner {
                        if !self.game.empty_ref_traced.insert((form, opcode)) {
                            return Ok(Value::ZERO);
                        }
                    }
                    let name = self.game.data.table.get(opcode).map(|f| f.name.clone()).unwrap_or_default();
                    let unit = self.unit.key();
                    self.game.emit(EventKind::CallFailed {
                        opcode: format!("0x{opcode:04X}"),
                        name,
                        unit,
                        error: "called on an empty reference".into(),
                    });
                    return Ok(Value::ZERO);
                }
            },
            None => None,
        };
        let ctx = Ctx {
            // The unit's key, built once for the unit (#1389).
            unit: std::sync::Arc::clone(self.unit.shared_key()),
            opcode,
            on,
            self_ref: self.ctx.self_ref,
            action_ref: self.ctx.action_ref,
            quest: self.ctx.quest,
            inst: self.inst_key(),
            in_condition: false,
        };
        Ok(self.game.call_function(&ctx, args))
    }

    fn skipped(&mut self, what: &VmError) {
        let unit = self.unit.key();
        self.game.emit(EventKind::Skipped { unit, what: what.to_string() });
    }

    fn take_return(&mut self) -> bool {
        std::mem::take(&mut self.game.stop_run)
    }

    fn take_wait(&mut self) -> Option<u32> {
        self.game.jip.wait_request.take()
    }

    fn print(&mut self, text: &str) {
        let unit = self.unit.key();
        self.game.console_out.push(text.to_string());
        self.game.emit(EventKind::ScriptPrint { unit, text: text.to_string() });
    }

    fn script_error(&mut self, text: &str) {
        let unit = self.unit.key();
        self.game.console_out.push(format!("Script error in {unit}: {text}"));
        self.game.emit(EventKind::ScriptError { unit, error: text.to_string() });
    }

    fn box_callback(&mut self, func: fnv_script::vm::XVal) {
        self.game.box_callback = Some(func);
    }

    fn show_text(&mut self, text: &str, buttons: Vec<String>) {
        let none = FormId(0);
        if !buttons.is_empty() {
            let owner = Some(MessageBox::owner_of(self.inst_key(), &self.unit.key()));
            self.game.message_box = Some(MessageBox { message: none, buttons: buttons.len(), pressed: None, owner, hidden: Vec::new() });
        }
        let (is_box, count) = (!buttons.is_empty(), buttons.len());
        self.game.emit(EventKind::MessageShown { message: none, text: Some(text.to_string()), buttons, hidden: Vec::new() });
        if is_box {
            // Its own buttons: no Message form to count them from (`Game::queued`).
            self.game.show_box_buttons(none, count);
        } else {
            self.game.queue_corner_message(text, "", crate::nvse_hud::CORNER_SECONDS);
        }
    }

    fn debug_mode(&mut self, set: Option<bool>, plugin: Option<u32>) -> bool {
        let name = match plugin {
            Some(i) => self.game.data.load_order.get(i as usize).cloned().unwrap_or_default(),
            None => self.unit.unit.plugin.clone(),
        }
        .to_ascii_lowercase();
        match set {
            Some(true) => {
                self.game.debug_plugins.insert(name.clone());
            }
            Some(false) => {
                self.game.debug_plugins.remove(&name);
            }
            None => {}
        }
        self.game.debug_plugins.contains(&name)
    }

    fn make_lambda(&mut self, l: &fnv_script::ir::XExpr) -> Result<fnv_script::vm::XVal, String> {
        let key = lambda_key(self.unit, l, self.inst_key());
        if let Some(&id) = self.game.lambda_ids.get(&key) {
            // Made again by a maker with no instance: it reads that run's variables
            // [hypothesis, script-compiler.md §9 Lambdas].
            if let (Locals::Temp(m), Some(d)) = (&self.locals, self.game.lambdas.get_mut((id - LAMBDA_BASE) as usize)) {
                d.captured = Some(m.clone());
            }
            return Ok(fnv_script::vm::XVal::Ref(id));
        }
        let id = LAMBDA_BASE + self.game.lambdas.len() as u32;
        let captured = match &self.locals {
            Locals::Temp(m) => Some(m.clone()),
            Locals::Instance(_) => None,
        };
        self.game.lambdas.push(LambdaDef { unit: Some(self.unit), key: self.unit.key(), lambda: l.clone(), inst: key.2, captured });
        self.game.lambda_ids.insert(key, id);
        Ok(fnv_script::vm::XVal::Ref(id))
    }

    fn set_function_value(&mut self, v: fnv_script::vm::XVal) -> Result<(), String> {
        let game = &mut *self.game;
        let frame = game.udf.last_mut().ok_or("SetFunctionValue outside a user-defined function")?;
        game.ext.retain(&v);
        if let Some(old) = frame.value.replace(v) {
            game.ext.release(&old);
        }
        Ok(())
    }

    fn calling_script(&mut self) -> Option<u32> {
        self.game.udf.last().and_then(|f| f.caller).map(|f| f.0)
    }

    /// `Call` (`Game::run_function`).
    fn call_function(&mut self, f: &fnv_script::vm::XVal, on: Option<Value>, args: Vec<fnv_script::vm::XVal>) -> Result<fnv_script::vm::XVal, String> {
        let caller = match self.unit.unit.owner {
            fnv_script::UnitOwner::Script { form } => Some(FormId(form)),
            _ => None,
        };
        let ctx = self.ctx;
        self.game.run_function(f, on, args, caller, ctx)
    }

    fn event_call(&mut self, c: fnv_script::vm::EventCall) -> Result<fnv_script::vm::XVal, String> {
        let sender = self.unit.unit.plugin.clone();
        self.game.event_call(c, &sender)
    }

    fn members(&mut self, form: u32) -> Option<Vec<u32>> {
        let f = FormId(form);
        if self.game.data.sig(f) == Some(*b"FLST") {
            return Some(functions::list_members(self.game, f).into_iter().map(|m| m.0).collect());
        }
        // A container's items, one temporary reference per kind of item [documented:
        // ForEach; observed: probe loops/02].
        let items: Vec<FormId> = self.game.world.inventory_mut(f).ok()?.iter().filter(|(_, n)| *n > 0).map(|(i, _)| i).collect();
        Some(items.into_iter().map(|i| self.game.temp_ref(f, i).0).collect())
    }

    fn random(&mut self) -> f64 {
        use rand_core::RngCore;
        self.game.rng.next_u32() as f64 / (u32::MAX as f64 + 1.0)
    }

    fn form_sig(&mut self, form: u32) -> Option<[u8; 4]> {
        let f = FormId(form);
        // A shot that is over is no form any more [observed: proj1, IsFormValid 0].
        if self.game.gaps.projs.gone.contains(&f) {
            return None;
        }
        if self.game.temp_item(f).is_some() {
            return Some(*b"REFR");
        }
        let f = self.game.unclone(f);
        // The player reference is no record of a plugin [documented: PlayerREF, 00000014].
        if let Some(s) = self.game.data.sig(f).or_else(|| (f == FormId::PLAYER_REF).then_some(*b"ACHR")) {
            return Some(s);
        }
        // A reference made at run time (a dropped item, PlaceAtMe) is a reference too, an
        // actor's by its base [observed: rgb-ui, OnDrop's dropped stimpak is a reference
        // in the game].
        // One deleted before the last load answers as it did before the save (#1002).
        let base = self.game.world.peek(f).map(|s| s.base).or_else(|| self.game.world.deleted_base(f))?;
        Some(match self.game.data.sig(base) {
            Some(s) if &s == b"NPC_" => *b"ACHR",
            Some(s) if &s == b"CREA" => *b"ACRE",
            _ => *b"REFR",
        })
    }

    fn base_form(&mut self, form: u32) -> Option<u32> {
        crate::functions::base_of(self.game, FormId(form)).map(|b| b.0)
    }

    fn record_field(&mut self, form: u32, sub: &str, path: &str) -> Option<fnv_script::vm::XVal> {
        use fnv_script::vm::XVal;
        // A script's change of the field for the session first (SetFormDescription …).
        if let Some(v) = self.game.field_overrides.get(&(FormId(form), format!("{sub}/{path}"))) {
            return Some(v.clone());
        }
        let v = self.game.data.record_field(FormId(form), sub, path)?;
        Some(match v {
            serde_json::Value::Number(n) => XVal::Num(crate::nvse::record_float(n.as_f64()?)),
            // Form IDs decode as "0x0007EA31"; the null form is no form.
            serde_json::Value::String(s) if s.starts_with("0x") => XVal::Ref(u32::from_str_radix(&s[2..], 16).ok()?),
            serde_json::Value::String(s) => XVal::Str(s.clone()),
            _ => return None,
        })
    }

    fn hit_value(&mut self, actor: u32, what: &str, k: i64) -> Option<fnv_script::vm::XVal> {
        self.game.hit_value(FormId(actor), what, k)
    }

    fn body_part_name(&mut self, form: u32, part: i64) -> Option<String> {
        // SetBodyPartName's change first (`crate::nvse_jip8`).
        self.game.body_part_text(FormId(form), part, "BPTN").or_else(|| self.game.data.body_part_name(FormId(form), part))
    }

    fn calling_plugin(&mut self) -> Option<String> {
        let p = &self.unit.unit.plugin;
        (!matches!(p.as_str(), "query" | "console")).then(|| p.to_ascii_lowercase())
    }

    fn current_script(&mut self) -> Option<u32> {
        match self.unit.unit.owner {
            fnv_script::UnitOwner::Script { form } => Some(form),
            _ => None,
        }
    }

    fn local_var(&mut self, name: &str) -> Option<VarRef> {
        let index = self.unit.var_index(name)?;
        Some(VarRef { on: None, var: VarSlot::Local { index, letter: fnv_script::ir::VarLetter::Float } })
    }

    fn var_name(&mut self, var: &VarRef) -> Option<String> {
        match (&var.on, &var.var) {
            (None, VarSlot::Local { index, .. }) => self.unit.var_name(*index).map(str::to_string),
            _ => None,
        }
    }

    fn ext_call(&mut self, name: &str, on: Option<Value>, args: &[fnv_script::vm::XVal]) -> Option<Result<fnv_script::vm::XVal, String>> {
        let on = on.and_then(|v| v.as_form()).map(FormId);
        let ctx = self.ctx;
        // Who calls: DisablePlayerControlsAlt keeps its locks by plugin, GetNthExplicitRef
        // reads the calling script.
        self.game.other.plugin = self.calling_plugin().unwrap_or_default();
        self.game.other.script = self.current_script().map(FormId);
        crate::nvse::call(self.game, &ctx, name, on, args)
    }

    fn game_setting(&mut self, name: &str) -> Option<fnv_script::vm::XVal> {
        use fnv_script::vm::XVal;
        if let Some(v) = self.game.world.setting_f32(name) {
            return Some(XVal::Num(f64::from(v)));
        }
        match self.game.world.forms.settings.get(&name.to_ascii_lowercase())? {
            fnv_sim::settings::Setting::Str(s) => Some(XVal::Str(s.clone())),
            _ => None,
        }
    }

    fn objective_text(&mut self, quest: u32, index: i32) -> Option<String> {
        self.game.data.quests.get(&FormId(quest))?.objectives.get(&index)?.text.clone()
    }

    fn calling_ref(&mut self) -> Option<u32> {
        self.ctx.self_ref.map(|f| f.0)
    }

    fn mod_name(&mut self, index: u32) -> Option<String> {
        self.game.data.load_order.get(index as usize).cloned()
    }

    fn actor_value_code(&mut self, name: &str) -> Option<u16> {
        crate::functions::av_code(self.game, name).ok()
    }

    fn mod_index(&mut self, name: &str) -> Option<u32> {
        self.game.data.load_order.iter().position(|p| p.eq_ignore_ascii_case(name)).map(|i| i as u32)
    }

    fn game_loaded(&mut self, restarted: bool) -> bool {
        let key = self.unit.key();
        let seen = if restarted { &mut self.game.restarted_seen } else { &mut self.game.loaded_seen };
        seen.insert(key)
    }
}

#[cfg(test)]
mod effect_lookup_tests {
    use super::*;

    fn fx(id: u32) -> ActiveEffect {
        ActiveEffect {
            id,
            target: FormId::PLAYER_REF,
            source: FormId(0x800),
            effect: FormId(0x801),
            index: 0,
            magnitude: 0,
            duration: 1.0,
            elapsed: 0.0,
            permanent: false,
            caster: None,
            script: None,
            change: None,
            usage: 0.0,
            worn: None,
            inactive: false,
        }
    }

    /// Every hint and every starting point finds what the search from the start (the
    /// lookup before #1389) finds, present or missing, stale hints and ends included.
    #[test]
    fn a_checked_hint_or_cursor_finds_the_effect_a_search_finds() {
        let list: Vec<ActiveEffect> = [3, 5, 8, 9, 12].into_iter().map(fx).collect();
        let search = |id: u32| list.iter().position(|e| e.id == id);
        for id in [1, 3, 5, 8, 9, 12, 13] {
            for hint in [None, Some(0), Some(1), Some(2), Some(3), Some(4), Some(5), Some(99)] {
                assert_eq!(effect_index(&list, id, hint), search(id), "id {id} hint {hint:?}");
            }
            for from in 0..=7 {
                assert_eq!(effect_from(&list, id, from), search(id), "id {id} from {from}");
            }
        }
        assert_eq!(effect_index(&[], 3, Some(0)), None);
        assert_eq!(effect_from(&[], 3, 2), None);
    }
}
