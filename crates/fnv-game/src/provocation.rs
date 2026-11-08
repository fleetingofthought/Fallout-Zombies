//! Being attacked: assault, friendly fire, yielding and assistance
//! (docs/design/combat-ai.md §10; owner's playtest list 2026-10-11).
//!
//! Who attacks whom on sight is [`Game::would_attack`] (aggression against faction
//! relations). This module adds the fights that start because someone was **hit**, and
//! how they end:
//!
//! - **Assault** [documented: GECK wiki Friendly Fire, "Neutral: aggro immediately on
//!   player when attacked, in or out of combat"]: an actor the player hits that would not
//!   attack the player holds a **grudge** against the player from then on, so it fights
//!   (`would_attack` is true for the pair) and says its Assault line [documented: GECK
//!   wiki Combat Tab, "Said when the actor is first attacked"]. Enemies that are not
//!   aggressive enough to attack on sight are treated the same [design].
//! - **Owner's exception** (playtest list 2026-10-11): a neutral actor already fighting
//!   someone else, hit by the player while the player is fighting too, does not turn on
//!   the player (a stray shot in a shared fight) [design].
//! - **Friendly fire** [documented: GECK wiki Friendly Fire]: a Friend (faction reaction 3)
//!   turns on the player at once out of combat (iFriendHitNonCombatAllowed hits allowed)
//!   and only after more than iFriendHitCombatAllowed hits in combat; an Ally (reaction 2,
//!   and the player's teammates [hypothesis]) after more than iAllyHitNonCombatAllowed
//!   hits out of combat and never in combat (iAllyHitCombatAllowed, 1000 or more meaning
//!   never). A hit counts only fFriendMinimumLastHitTime after the one before, and runs
//!   out after fFriendHitTimer. GetFriendHit returns the count; an ally in combat never
//!   counts above 1. SetIgnoreFriendlyHits makes a friend or ally ignore them all
//!   [hypothesis: what the function's name says].
//! - **Yielding** [documented: GECK wiki Yielding]: the player yields by putting the weapon
//!   away within fCombatYieldTime (10 s) of the fight's start. Only actors fighting
//!   because the player attacked them (a grudge, not their records or StartCombat)
//!   accept: they stop fighting, say AcceptYield, and forget the grudge. The owner's rule
//!   (playtest list 2026-10-11): an actor that forgave the player within
//!   fCombatYieldRetryTime (600 s) and was attacked again ignores the yield. The wiki's
//!   timer is the player's (no one accepts a yield again until it runs out); ours is each
//!   actor's [design: the owner's wording, "if they have already recently forgiven"].
//! - **Assistance** [documented: GECK wiki GetFactionCombatReaction, "An actor's
//!   Assistance setting determine whether it will help an Ally / a Friend in combat or
//!   not"; the codes 0 Helps Nobody, 1 Helps Allies, 2 Helps Friends and Allies:
//!   xEdit AIDT]: an actor that detects an ally (or a friend, with code 2) in a combat
//!   state takes up that ally's fight against each actor the ally fights, unless it is
//!   itself an ally or friend of that actor. It holds a grudge as an assaulted actor does
//!   and is alarmed at the attacker's position. Actors sharing a faction, or placed from
//!   the same base form, count as allies [engineering choice, as `would_attack` never
//!   lets them fight each other].
//! - **Cowards stay out** [documented: GECK wiki Confidence, "Cowardly … I will always
//!   flee from combat"; owner's playtest list 4, item 14: confidence and aggression must
//!   override the helps-allies join]: an actor whose Confidence is Cowardly never takes up
//!   another's fight, by assistance here or by an alarm (`crate::combat_group`); it
//!   still fights whom its own records make it attack, and whoever attacks it. Avoiding
//!   the fight (running from it) is not built (combat-ai.md §11, "Avoiding threats out of
//!   combat") [design: Unaggressive alone does not keep a helper out; the wiki's
//!   Unaggressive only says whom an actor might avoid].
//! - **A danger to itself** [design: owner's playtest list 5 item 35, "Sunny and her dog
//!   should fight as soon as the geckos aggro on the player"]: an actor that detects an
//!   attacker fighting the player or itself, where that attacker would attack it too
//!   (Very Aggressive geckos attack everyone), takes up the fight against it at once, as
//!   if helping the attacked one, instead of waiting to be hit. Not cowards, not actors
//!   whose package is Defensive Combat, not against an ally or friend of its own. Fights
//!   between other actors are left to assistance and alarms (above, and
//!   `crate::combat_group`): with this rule there too, every NCR trooper near Cook-Cook's
//!   camp took the fight up on its own and fewer alarmed the others.
//! - **Escorts** [documented: GECK wiki Defensive Combat, "The exception is for the
//!   Accompany, Escort and Follow packages … if the target is attacked, the actor will
//!   join into the combat"]: an actor whose current package is Escort takes up the fight
//!   of the one it escorts (the package's target) against everyone attacking it: actors
//!   in a combat state with the charge on their target list, and whoever hurt it. Its
//!   Assistance does not matter; it never fights its own allies or friends, and a coward
//!   stays out. Choices [hypothesis]: the escort must at least notice its charge (band
//!   Noticed or Detected); a fight the charge starts is joined only once the foe
//!   fights back; a charge that provoked the attacker is still helped; it holds for the
//!   whole package, also once at the destination; the player's teammates are left to
//!   `crate::followers`. Follow and Accompany (the same exception) are not built here.
//!
//! A grudge ends when its holder goes back to Normal, on StopCombat, and on an accepted
//! yield. Hits by actors other than the player go to `Game::attacked_by_actor`
//! (`crate::combat_group`, NPC against NPC).

use crate::awareness::{Cause, State};
use crate::detection::Band;
use crate::functions::actor::{actor_value, faction_relation};
use crate::game::Game;
use fnv_sim::FormId;
use std::collections::{BTreeMap, HashMap};

const ASSISTANCE: u16 = crate::actor_values::ASSISTANCE;

/// Whom an actor fights, and where each was last known to be.
type Foes = Vec<(FormId, [f32; 3])>;

/// Faction combat reactions [documented: GECK wiki GetFactionCombatReaction].
pub const NEUTRAL: i64 = 0;
pub const ENEMY: i64 = 1;
pub const ALLY: i64 = 2;
pub const FRIEND: i64 = 3;

/// Why an actor fights someone its records would leave alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Why {
    /// It was attacked by them.
    Assault,
    /// It helps `victim`, whom they fight.
    Assist { victim: FormId },
    /// The player stayed inside its aggro radius past its warnings (`crate::aggro`).
    Aggro,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Grudge {
    pub why: Why,
    /// `Game::time` the fight began for it: the yield timer counts from here.
    pub since: f64,
}

/// The game settings of friendly fire and yielding [documented: GECK wiki Friendly Fire
/// and Yielding, with the wiki's defaults; FalloutNV.esm sets iFriendHitCombatAllowed to
/// 4, observed 2026-10-12].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub friend_hit_timer: f64,
    pub friend_min_last_hit: f64,
    pub friend_combat: f64,
    pub friend_non_combat: f64,
    pub ally_combat: f64,
    pub ally_non_combat: f64,
    pub yield_time: f64,
    pub yield_retry: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            friend_hit_timer: 10.0,
            friend_min_last_hit: 0.5,
            friend_combat: 3.0,
            friend_non_combat: 0.0,
            ally_combat: 1000.0,
            ally_non_combat: 3.0,
            yield_time: 10.0,
            yield_retry: 600.0,
        }
    }
}

impl Settings {
    pub fn read(g: &Game) -> Settings {
        let d = Settings::default();
        let s = |name: &str, v: f64| crate::combat::setting(g, name, v);
        Settings {
            friend_hit_timer: s("fFriendHitTimer", d.friend_hit_timer),
            friend_min_last_hit: s("fFriendMinimumLastHitTime", d.friend_min_last_hit),
            friend_combat: s("iFriendHitCombatAllowed", d.friend_combat),
            friend_non_combat: s("iFriendHitNonCombatAllowed", d.friend_non_combat),
            ally_combat: s("iAllyHitCombatAllowed", d.ally_combat),
            ally_non_combat: s("iAllyHitNonCombatAllowed", d.ally_non_combat),
            yield_time: s("fCombatYieldTime", d.yield_time),
            yield_retry: s("fCombatYieldRetryTime", d.yield_retry),
        }
    }
}

/// "Any value 1000 or over is treated as infinite" [documented: GECK wiki Friendly Fire].
pub const INFINITE_HITS: f64 = 1000.0;

/// What happened, for the engine's log and summary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Note {
    /// `actor` turns on `by`, who hit it (its reaction to `by`; the friendly hits counted).
    Assaulted { actor: FormId, by: FormId, relation: i64, hits: u32 },
    /// A friend's or ally's hit counted, not yet enough (`allowed` hits are let pass).
    Tolerated { actor: FormId, by: FormId, relation: i64, hits: u32, allowed: f64, in_combat: bool },
    /// Ignored: both were fighting someone else (the owner's exception), or the actor
    /// ignores friendly hits.
    Ignored { actor: FormId, by: FormId, why: &'static str },
    /// `helper` takes up `victim`'s fight against `attacker`.
    Assists { helper: FormId, victim: FormId, attacker: FormId },
    /// `actor` accepted the player's yield and stopped fighting.
    Forgives { actor: FormId, after: f64 },
    /// `actor` ignored the yield.
    Refuses { actor: FormId, why: &'static str, after: f64 },
    /// `actor` warned the player inside its aggro radius (`level` warnings so far;
    /// `crate::aggro`).
    AggroWarned { actor: FormId, level: u32, distance: f64, radius: f64 },
    /// `actor` attacks the player, who stayed inside its aggro radius.
    AggroAttacks { actor: FormId, distance: f64, radius: f64 },
    /// `helper` stays out of `victim`'s fight: its confidence is Cowardly (module notes).
    StaysOut { helper: FormId, victim: FormId },
}

/// The grudges, friendly hits and forgiveness of every actor. The grudges and the
/// forgiveness times are saved (`GameSnapshot::grudges`, `forgiven`); the rest is not.
#[derive(Debug, Clone, Default)]
pub struct Provocations {
    /// (actor, whom it fights) → why.
    pub grudges: BTreeMap<(FormId, FormId), Grudge>,
    /// (actor, by) → the times of the friendly hits counted, oldest first.
    pub friend_hits: BTreeMap<(FormId, FormId), Vec<f64>>,
    /// When each actor last accepted the player's yield.
    pub forgiven: BTreeMap<FormId, f64>,
    /// Actors someone attacked (GetAttacked), until they leave combat.
    pub attacked: std::collections::BTreeSet<FormId>,
    pub notes: Vec<Note>,
    pub settings: Option<Settings>,
    /// Aggro radius warnings each actor gave the player inside its radius, and when it
    /// gave the last (`crate::aggro`).
    pub aggro_warnings: std::collections::BTreeMap<FormId, (u32, f64)>,
    /// Actors warning the player now: inside their aggro radius with the player watched
    /// at the last aggro tick (`crate::aggro`, `Game::warning_to`).
    pub aggro_inside: std::collections::BTreeSet<FormId>,
    /// (helper, victim) pairs whose staying out was noted ([`Note::StaysOut`], once).
    pub stayed_out: std::collections::BTreeSet<(FormId, FormId)>,
}

impl Provocations {
    /// Whether `actor` fights `other` because of a grudge.
    pub fn holds(&self, actor: FormId, other: FormId) -> bool {
        self.grudges.contains_key(&(actor, other))
    }

    /// Forget every grudge `actor` holds (it left its fight).
    pub fn calm(&mut self, actor: FormId) {
        self.grudges.retain(|(a, _), _| *a != actor);
        self.friend_hits.retain(|(a, _), _| *a != actor);
        self.attacked.remove(&actor);
    }

    pub fn take_notes(&mut self) -> Vec<Note> {
        std::mem::take(&mut self.notes)
    }
}

/// How many friendly hits are let pass before a friend or ally turns (module notes):
/// `None` for an actor that is neither.
pub fn hits_allowed(s: &Settings, relation: i64, in_combat: bool) -> Option<f64> {
    match (relation, in_combat) {
        (FRIEND, true) => Some(s.friend_combat),
        (FRIEND, false) => Some(s.friend_non_combat),
        (ALLY, true) => Some(s.ally_combat),
        (ALLY, false) => Some(s.ally_non_combat),
        _ => None,
    }
}

/// Whether an actor with Assistance `assistance` helps one it stands to as `relation`
/// [documented: codes 0 Helps Nobody, 1 Helps Allies, 2 Helps Friends and Allies].
pub fn helps(assistance: i64, relation: i64) -> bool {
    match relation {
        ALLY => assistance >= 1,
        FRIEND => assistance >= 2,
        _ => false,
    }
}

impl Game<'_> {
    fn provocation_settings(&mut self) -> Settings {
        match self.provocations.settings {
            Some(s) => s,
            None => {
                let s = Settings::read(self);
                self.provocations.settings = Some(s);
                s
            }
        }
    }

    /// How `actor` stands to `other` for friendly fire and assistance: its faction combat
    /// reaction, the player's teammates being allies of the player [hypothesis], and
    /// actors sharing a faction or a base form allies [engineering choice].
    pub fn reaction_to(&self, actor: FormId, other: FormId) -> i64 {
        if other == FormId::PLAYER_REF && self.extra.get(&actor).is_some_and(|e| e.teammate) {
            return ALLY;
        }
        if actor == FormId::PLAYER_REF && self.extra.get(&other).is_some_and(|e| e.teammate) {
            return ALLY;
        }
        let relation = faction_relation(self, actor, other);
        if relation != NEUTRAL || actor == FormId::PLAYER_REF || other == FormId::PLAYER_REF {
            return relation;
        }
        if self.kin(actor, other) {
            return ALLY;
        }
        NEUTRAL
    }

    /// Whether `helper` stays out of `victim`'s fight (a Cowardly helper, module notes);
    /// the first time for the pair it is noted ([`Note::StaysOut`]).
    pub fn stays_out(&mut self, helper: FormId, victim: FormId) -> bool {
        let cowardly = actor_value(self, helper, crate::actor_values::CONFIDENCE).unwrap_or(2.0) as i64 == 0;
        if cowardly && self.provocations.stayed_out.insert((helper, victim)) {
            self.provocations.notes.push(Note::StaysOut { helper, victim });
        }
        cowardly
    }

    /// `actor` was hit by `by` (and lives): it may turn on `by` (module notes). Returns
    /// whether it now holds a grudge it did not hold before. The engine calls this for
    /// every hit before [`Game::hurt_by`], which then alarms the actor.
    pub fn attacked_by(&mut self, actor: FormId, by: FormId) -> bool {
        if actor == by || actor == FormId::PLAYER_REF || self.is_dead(actor) {
            return false;
        }
        self.provocations.attacked.insert(actor);
        // Hits by other actors: `crate::combat_group` (NPC against NPC).
        if by != FormId::PLAYER_REF {
            return self.attacked_by_actor(actor, by);
        }
        if self.provocations.holds(actor, by) || self.would_attack(actor, by) {
            return false;
        }
        let s = self.provocation_settings();
        let now = self.time;
        let state = self.awareness.state(actor);
        let in_combat = state.in_combat();
        let relation = self.reaction_to(actor, by);
        let hits = match relation {
            ALLY | FRIEND => {
                if self.extra.get(&actor).is_some_and(|e| e.ignore_friendly_hits) {
                    self.provocations.notes.push(Note::Ignored { actor, by, why: "it ignores friendly hits" });
                    return false;
                }
                let allowed = hits_allowed(&s, relation, in_combat).unwrap_or(0.0);
                let list = self.provocations.friend_hits.entry((actor, by)).or_default();
                list.retain(|t| now - t < s.friend_hit_timer);
                if list.last().is_none_or(|t| now - t >= s.friend_min_last_hit) {
                    list.push(now);
                }
                if allowed >= INFINITE_HITS {
                    // "The count never goes above 1".
                    list.drain(..list.len().saturating_sub(1));
                }
                let hits = list.len() as u32;
                if allowed >= INFINITE_HITS || hits as f64 <= allowed {
                    self.provocations.notes.push(Note::Tolerated { actor, by, relation, hits, allowed, in_combat });
                    return false;
                }
                hits
            }
            _ => {
                // The owner's exception: both already fighting someone else.
                let fights_other = state == State::Combat && self.awareness.get(actor).and_then(|a| a.combat_target()).is_some_and(|(t, _)| t != by);
                let player_fights = self.awareness.actors.iter().any(|(id, a)| *id != actor && a.state.in_combat() && a.targets.contains_key(&by));
                if fights_other && player_fights {
                    self.provocations.notes.push(Note::Ignored { actor, by, why: "both were fighting someone else" });
                    return false;
                }
                1
            }
        };
        self.provocations.grudges.insert((actor, by), Grudge { why: Why::Assault, since: now });
        self.provocations.notes.push(Note::Assaulted { actor, by, relation, hits });
        self.player_assaulted(actor);
        self.detection_bark(actor, Some(by), "Assault");
        true
    }

    /// GetFriendHit: the friendly hits `by` made on `actor` that still count.
    pub fn friend_hits(&self, actor: FormId, by: FormId) -> u32 {
        let window = self.provocations.settings.unwrap_or_default().friend_hit_timer;
        self.provocations.friend_hits.get(&(actor, by)).map_or(0, |l| l.iter().filter(|t| self.time - **t < window).count() as u32)
    }

    /// The player put the weapon away: a yield (module notes). Returns how many actors
    /// accepted it.
    pub fn player_yields(&mut self) -> usize {
        let s = self.provocation_settings();
        let now = self.time;
        let fighting: Vec<(FormId, Grudge)> = self.provocations.grudges.iter().filter(|((_, o), _)| *o == FormId::PLAYER_REF).map(|((a, _), g)| (*a, *g)).collect();
        let mut accepted = 0;
        for (actor, g) in fighting {
            if !self.awareness.state(actor).in_combat() && g.why == Why::Assault {
                // Not fighting yet (still to notice the attacker): nothing to yield to.
                continue;
            }
            let after = now - g.since;
            if after > s.yield_time {
                self.provocations.notes.push(Note::Refuses { actor, why: "the fight is past the time to yield", after });
                continue;
            }
            if self.provocations.forgiven.get(&actor).is_some_and(|t| now - t < s.yield_retry) {
                self.provocations.notes.push(Note::Refuses { actor, why: "it forgave the player recently", after });
                continue;
            }
            self.provocations.forgiven.insert(actor, now);
            self.provocations.notes.push(Note::Forgives { actor, after });
            self.stop_combat(actor);
            self.detection_bark(actor, Some(FormId::PLAYER_REF), "AcceptYield");
            accepted += 1;
        }
        accepted
    }

    /// Allies of actors in a fight take it up (module notes): called after a round of
    /// measurements, before [`Game::settle_awareness`]. Returns how many joined.
    pub fn call_allies(&mut self) -> usize {
        let fights: Vec<(FormId, Foes)> = self
            .awareness
            .actors
            .iter()
            .filter(|(_, a)| a.state.in_combat())
            .map(|(id, a)| {
                let mut foes: Vec<(FormId, [f32; 3])> = a.targets.iter().map(|(t, x)| (*t, x.at)).collect();
                for al in a.alarms.iter().filter(|x| x.cause == Cause::Hurt) {
                    if !foes.iter().any(|f| f.0 == al.owner) {
                        foes.push((al.owner, al.at));
                    }
                }
                (*id, foes)
            })
            .filter(|(_, f)| !f.is_empty())
            .collect();
        let mut joined = 0;
        // Who perceives whom (detected, seen or heard), in one pass over the pairs, each
        // victim's watchers in the pairs' order [engineering choice: one scan per victim
        // over every pair was a fifth of a 50-a-side battle's detection, CPU profile
        // 2026-10-23]. Each helper's Assistance is read once.
        let mut watching: HashMap<FormId, Vec<FormId>> = HashMap::new();
        for ((w, t), (d, _)) in &self.detection.pairs {
            if d.band == Band::Detected {
                watching.entry(*t).or_default().push(*w);
            }
        }
        let mut assistance_of: HashMap<FormId, i64> = HashMap::new();
        // Who would attack whom, worked out once per pair while no grudge is added: a
        // grudge is the only thing this loop changes that it depends on (`Game::would_attack`),
        // so the memo is emptied when one is. Asked again for every victim, it was 10 ms of
        // a 50-a-side battle's measurement: ~12,000 asks for ~2,500 pairs [observed
        // 2026-11-07, #1799].
        let mut attacks: HashMap<(FormId, FormId), bool> = HashMap::new();
        for (victim, foes) in fights {
            let watchers = watching.get(&victim).cloned().unwrap_or_default();
            for helper in watchers {
                if helper == FormId::PLAYER_REF || self.is_dead(helper) || !self.present(helper) {
                    continue;
                }
                let assistance = match assistance_of.get(&helper) {
                    Some(&a) => a,
                    None => {
                        let a = actor_value(self, helper, ASSISTANCE).unwrap_or(0.0) as i64;
                        assistance_of.insert(helper, a);
                        a
                    }
                };
                if !helps(assistance, self.reaction_to(helper, victim)) {
                    continue;
                }
                if self.stays_out(helper, victim) {
                    continue;
                }
                for &(attacker, at) in &foes {
                    if attacker == helper || self.provocations.holds(helper, attacker) {
                        continue;
                    }
                    let would = match attacks.get(&(helper, attacker)) {
                        Some(&w) => w,
                        None => {
                            let w = self.would_attack(helper, attacker);
                            attacks.insert((helper, attacker), w);
                            w
                        }
                    };
                    if would {
                        continue;
                    }
                    // Never against its own allies and friends [documented: they "cannot
                    // attack each other under any circumstances"].
                    if matches!(self.reaction_to(helper, attacker), ALLY | FRIEND) {
                        continue;
                    }
                    let now = self.time;
                    self.provocations.grudges.insert((helper, attacker), Grudge { why: Why::Assist { victim }, since: now });
                    attacks.clear();
                    self.provocations.notes.push(Note::Assists { helper, victim, attacker });
                    let t = self.awareness.timers.unwrap_or_default();
                    self.awareness.actors.entry(helper).or_default().alarm(attacker, at, 0.0, Cause::Ally, now, &t);
                    joined += 1;
                }
            }
        }
        joined += self.join_against_dangers();
        joined += self.escorts_assist();
        // The player's teammates take up the player's fights (`crate::followers`).
        joined + self.teammates_assist()
    }

    /// An actor running an Escort package takes up its charge's fights (module notes,
    /// "Escorts"; docs/spec-notes/packages.md §3.1): for each actor attacking the charge
    /// (in a combat state with the charge on its target list, or having hurt it), the
    /// escort holds a grudge and is alarmed at the attacker's position, as an assisting
    /// ally is. Returns how many joined.
    pub fn escorts_assist(&mut self) -> usize {
        let escorts: Vec<(FormId, FormId)> = self
            .extra
            .iter()
            .filter_map(|(id, e)| {
                let def = self.package_def(&e.current_package?)?;
                (def.ty == crate::package::PackageType::Escort).then_some((*id, def))
            })
            .filter_map(|(id, def)| self.package_target(id, def).map(|t| (id, t)))
            .filter(|(id, charge)| id != charge)
            .collect();
        let mut joined = 0;
        for (escort, charge) in escorts {
            if escort == FormId::PLAYER_REF || self.is_dead(escort) || !self.present(escort) || self.is_dead(charge) || self.is_teammate(escort) {
                continue;
            }
            // It must at least notice its charge [hypothesis: an escort keeps track of the
            // one it leads even while walking ahead with its back to it, so Noticed is
            // enough where an ally's assistance needs Detected].
            if !self.detection.pairs.get(&(escort, charge)).is_some_and(|(d, _)| d.band != Band::Hidden) {
                continue;
            }
            for (attacker, at) in self.attackers_of(charge) {
                if attacker == escort || self.is_dead(attacker) {
                    continue;
                }
                let knows = self.awareness.get(escort).is_some_and(|a| a.targets.contains_key(&attacker) || a.alarms.iter().any(|x| x.owner == attacker));
                if knows || self.provocations.holds(escort, attacker) {
                    continue;
                }
                // Never against its own allies and friends [documented: they "cannot
                // attack each other under any circumstances"].
                if matches!(self.reaction_to(escort, attacker), ALLY | FRIEND) {
                    continue;
                }
                if self.stays_out(escort, charge) {
                    break;
                }
                let now = self.time;
                if !self.would_attack(escort, attacker) {
                    self.provocations.grudges.insert((escort, attacker), Grudge { why: Why::Assist { victim: charge }, since: now });
                }
                self.provocations.notes.push(Note::Assists { helper: escort, victim: charge, attacker });
                let t = self.awareness.timers.unwrap_or_default();
                self.awareness.actors.entry(escort).or_default().alarm(attacker, at, 0.0, Cause::Ally, now, &t);
                joined += 1;
            }
        }
        joined
    }

    /// Who attacks `victim`, with where they are: every actor in a combat state with
    /// `victim` on its target list, and whoever hurt `victim` (its `Hurt` alarms).
    fn attackers_of(&self, victim: FormId) -> Foes {
        let pos = |g: &Game, r: FormId| g.world.peek(r).map(|s| s.pos).or_else(|| g.world.placed.get(r).map(|p| p.pos));
        let mut out: Foes = self
            .awareness
            .actors
            .iter()
            .filter(|(id, a)| **id != victim && a.state.in_combat() && a.targets.contains_key(&victim))
            .filter_map(|(id, _)| pos(self, *id).map(|p| (*id, p)))
            .collect();
        if let Some(a) = self.awareness.get(victim) {
            for al in a.alarms.iter().filter(|x| x.cause == Cause::Hurt) {
                if al.owner != victim && !out.iter().any(|o| o.0 == al.owner) {
                    out.push((al.owner, al.at));
                }
            }
        }
        out
    }
}

impl Game<'_> {
    /// Actors that detect an attacker which would attack them too join the fight against
    /// it (module notes, "A danger to itself"). Returns how many joined.
    fn join_against_dangers(&mut self) -> usize {
        // Who fights whom: each attacker in a combat state and its targets.
        let fights: Vec<(FormId, FormId)> = self
            .awareness
            .actors
            .iter()
            .filter(|(_, a)| a.state == State::Combat)
            .flat_map(|(id, a)| a.targets.keys().map(move |t| (*id, *t)))
            .collect();
        let mut joined = 0;
        // Each attacker's watchers in the pairs' order, from one pass over the pairs: a
        // pass per fight was 7.5-10.6 ms of a 50-a-side battle's measurement [observed
        // 2026-11-07, #1799]. Nothing below changes the pairs.
        let mut watching: HashMap<FormId, Vec<FormId>> = HashMap::new();
        for ((w, t), (d, _)) in &self.detection.pairs {
            if d.band == Band::Detected {
                watching.entry(*t).or_default().push(*w);
            }
        }
        for (attacker, victim) in fights {
            let Some(at) = self.world.peek(attacker).map(|s| s.pos).or_else(|| self.world.placed.get(attacker).map(|p| p.pos)) else { continue };
            let watchers = watching.get(&attacker).cloned().unwrap_or_default();
            for helper in watchers {
                // Another actor's fight counts only when it is the watcher's own.
                if victim != FormId::PLAYER_REF && helper != victim {
                    continue;
                }
                if helper == FormId::PLAYER_REF || self.is_dead(helper) || !self.present(helper) || self.awareness.state(helper) == State::Combat {
                    continue;
                }
                if self.provocations.holds(helper, attacker) || self.would_attack(helper, attacker) || !self.would_attack(attacker, helper) {
                    continue;
                }
                if matches!(self.reaction_to(helper, attacker), ALLY | FRIEND) || self.stays_out(helper, attacker) || self.package_has(helper, crate::package::flags::DEFENSIVE_COMBAT) {
                    continue;
                }
                let now = self.time;
                // Gone for itself: as if attacked; else as helping the one attacked.
                let why = if victim == helper { Why::Assault } else { Why::Assist { victim } };
                self.provocations.grudges.insert((helper, attacker), Grudge { why, since: now });
                self.provocations.notes.push(Note::Assists { helper, victim, attacker });
                // It noticed the danger itself, so it alarms its own allies in turn
                // (`crate::combat_group`, which passes on no alarm from an ally).
                let t = self.awareness.timers.unwrap_or_default();
                self.awareness.actors.entry(helper).or_default().alarm(attacker, at, 0.0, Cause::Noticed, now, &t);
                joined += 1;
            }
        }
        joined
    }
}

/// `actor.GetFriendHit target`: the friendly hits `target` made on the actor that still
/// count [documented: GECK wiki GetFriendHit, Friendly Fire].
pub fn get_friend_hit(g: &mut Game, ctx: &crate::functions::Ctx, a: &crate::functions::Args) -> Result<fnv_script::vm::Value, String> {
    let r = ctx.subject()?;
    Ok(fnv_script::vm::Value::Num(g.friend_hits(r, a.form(0)?) as f64))
}

/// `actor.GetAttacked`: 1 once someone attacked the actor, cleared when it drops out of
/// combat [documented: GECK wiki GetAttacked].
pub fn get_attacked(g: &mut Game, ctx: &crate::functions::Ctx, _a: &crate::functions::Args) -> Result<fnv_script::vm::Value, String> {
    let r = ctx.subject()?;
    crate::functions::yes(g.provocations.attacked.contains(&r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_hits_allowed_follow_the_settings() {
        let s = Settings { friend_combat: 4.0, ..Settings::default() };
        assert_eq!(hits_allowed(&s, FRIEND, true), Some(4.0));
        assert_eq!(hits_allowed(&s, FRIEND, false), Some(0.0));
        assert_eq!(hits_allowed(&s, ALLY, true), Some(1000.0));
        assert_eq!(hits_allowed(&s, ALLY, false), Some(3.0));
        assert_eq!(hits_allowed(&s, NEUTRAL, false), None);
    }

    #[test]
    fn assistance_codes() {
        assert!(!helps(0, ALLY) && !helps(0, FRIEND));
        assert!(helps(1, ALLY) && !helps(1, FRIEND));
        assert!(helps(2, ALLY) && helps(2, FRIEND));
        assert!(!helps(2, NEUTRAL) && !helps(2, ENEMY));
    }
}
