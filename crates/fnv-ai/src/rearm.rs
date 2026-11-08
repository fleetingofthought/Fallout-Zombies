//! Actors in a combat state take up a better weapon (docs/design/combat-ai.md §22): the
//! looking and the fetching; the ranks and the taking are `fnv_game::rearm`'s. The rules
//! are **[design]**.
//!
//! - Every [`LOOK_SECONDS`] (the first time at once, the second after a part of a second
//!   of its own, [`look_phase`]) an actor that rearms (`Game::rearms`) and is not fetching
//!   first swaps a held weapon its combat style rules out (`Game::arm_restricted_by_style`,
//!   `fnv_game::style_weapon`), then, holding no usable weapon, holds the best one it
//!   carries; then it
//!   looks within [`ARMED_REACH`] ([`UNARMED_REACH`] with no usable weapon) for a better
//!   one: on the ground or carried by a dead actor, with ammunition for a gun carried or
//!   found within the same reach (on the ground, on a body, in an unlocked container).
//!   The best by rank, nearest on ties, becomes its fetch (`Rearms::fetches`). Before
//!   looking, a weapon that does not fire at its target's distance gives way to a
//!   carried one of its class that does (`Game::take_up_for_distance`).
//! - The fetch: it runs to each stop in turn (the nearer first), stands there for the
//!   pick-up idle ([`PICKUP_SECONDS`]) and takes or copies the item (`Game::take_leg`);
//!   after the last it holds the weapon it went for. A stop it has no way to, that is
//!   gone, or a fetch longer than [`FETCH_SECONDS`] is given up for this fight.

use crate::alert::Search;
use crate::carry::{Activity, AiEvent};
use crate::{goal, Ai};
use fnv_game::game::Game;
use fnv_game::idles::IdleSource;
use fnv_game::rearm::{Fetch, Leg, Rank, Take};
use fnv_nav::{NavGraph, Place};
use fnv_sim::forms::ItemKind;
use fnv_sim::FormId;

/// How often an actor looks for a better weapon, seconds [design].
pub const LOOK_SECONDS: f64 = 1.0;
/// How far it looks while it holds a usable weapon, game units (about 15 m) [design].
pub const ARMED_REACH: f32 = 1024.0;
/// How far it looks with none, game units (about 58 m) [design].
pub const UNARMED_REACH: f32 = 4096.0;
/// Items more than this above or below it are not looked at, game units [design].
pub const REACH_HEIGHT: f32 = 256.0;
/// A fetch not over after this long is given up, seconds [design].
pub const FETCH_SECONDS: f64 = 20.0;
/// It stands this long at each stop for the pick-up idle, seconds [observed: the engine
/// plays `PickUpGroundObject_0.3.kf` for 0.57 s, smoke `combat-npc-rearm` 2026-10-25].
pub const PICKUP_SECONDS: f64 = 0.6;
/// A walk that ended farther than this from its stop did not get there, game units
/// [design: about an arm's reach beyond the navmesh point nearest the item].
pub const ARRIVE_WITHIN: f32 = 160.0;

/// A weapon found and the stops to have it.
struct Found {
    weapon: FormId,
    rank: Rank,
    legs: Vec<Leg>,
    trip: f32,
}

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The part of [`LOOK_SECONDS`] by which `actor`'s looks are put off after its first,
/// in [0, 1): from its FormID, the same every time [engineering choice].
pub fn look_phase(actor: FormId) -> f64 {
    f64::from(actor.0.wrapping_mul(0x9E37_79B1) >> 16) / 65536.0
}

impl Ai {
    /// One update of re-arming for an actor in a combat state. Returns whether a fetch
    /// moves it now (the fight's own moves wait).
    pub(crate) fn rearm_step(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, dt: f32) -> bool {
        if !game.rearms(actor) {
            return false;
        }
        if let Some(f) = game.rearms.fetches.get(&actor).cloned() {
            return self.fetch_step(game, nav, actor, f, dt);
        }
        let now = game.time;
        let next = match game.rearms.next_look.get(&actor) {
            Some(t) if now < *t => return false,
            Some(_) => now + LOOK_SECONDS,
            // The first look: the next one a part of a second later, the actor's own, so
            // that actors who went into combat together do not all look in the same
            // frame again and again (#1799: 118 looks in one frame were 26-31 ms of a
            // 50-a-side battle's frame, once a second [observed 2026-11-07]).
            None => now + LOOK_SECONDS * (1.0 + look_phase(actor)),
        };
        game.rearms.next_look.insert(actor, next);
        // A held weapon its combat style rules out is the style rule's to swap
        // (`fnv_game::style_weapon`, #1721); then an empty hand takes the best carried.
        game.arm_restricted_by_style(actor);
        if let Some(weapon) = game.take_up_best_carried(actor) {
            self.log(game, actor, AiEvent::Rearmed { weapon, from: None });
        }
        // A weapon that does not fire at its target's distance gives way to one it carries
        // that does (`Game::take_up_for_distance`).
        let feet = self.agents[&actor].walker.feet;
        // The target it fights: the one it detects, else the one it detected last (as the
        // fight's moves take it, `crate::alert`), in Combat only.
        let target_at = game.awareness.get(actor).filter(|a| a.state == fnv_game::awareness::State::Combat).and_then(|a| {
            let latest = a.targets.values().filter(|t| t.detected.is_some()).max_by(|x, y| x.detected.unwrap_or(0.0).total_cmp(&y.detected.unwrap_or(0.0)));
            a.combat_target().map(|(_, t)| t).or(latest).map(|t| t.at)
        });
        if let Some(at) = target_at {
            let distance = goal::dist(feet, at);
            if let Some(weapon) = game.take_up_for_distance(actor, distance) {
                self.log(game, actor, AiEvent::RangeSwap { weapon, distance });
            }
        }
        let Some(found) = self.look_for_weapon(game, actor) else { return false };
        let first = found.legs[0];
        game.rearms.fetches.insert(actor, Fetch { weapon: found.weapon, rank: found.rank, legs: found.legs.clone(), since: now, busy_until: None });
        self.log(game, actor, AiEvent::Fetch { weapon: found.weapon, dps: found.rank.dps, stops: found.legs.len() as u32, trip: found.trip });
        if self.walk_to_leg(game, nav, actor, first) {
            true
        } else {
            self.drop_fetch(game, actor, first.from, "no way there");
            false
        }
    }

    /// The best weapon within reach that ranks above the one `actor` holds (module notes).
    fn look_for_weapon(&mut self, game: &mut Game, actor: FormId) -> Option<Found> {
        let w = &self.agents[&actor].walker;
        let feet = Place { space: w.space, pos: w.feet };
        let held = game.held_rank(actor);
        let reach = if held.is_none() { UNARMED_REACH } else { ARMED_REACH };
        let given_up = game.rearms.given_up.get(&actor).cloned().unwrap_or_default();
        let actor_base = game.world.placed.get(actor).map(|p| p.base);
        // Weapons (base, condition, stop) and ammunition sources (stop) within reach.
        let mut weapons: Vec<(FormId, f32, Leg)> = Vec::new();
        let mut ammo: Vec<Leg> = Vec::new();
        let forms = game.world.forms;
        for r in crate::errand::cells_around(game, feet, reach) {
            if r == actor || given_up.contains(&r) || !game.present(r) {
                continue;
            }
            // No AI Acquire: nothing is taken from it, a weapon, ammunition, a body or a
            // container alike (#962).
            if game.world.placed.get(r).is_some_and(|p| p.no_ai_acquire()) {
                continue;
            }
            let Some(p) = goal::place_of_ref(game, r).filter(|p| p.space == feet.space) else { continue };
            if flat(p.pos, feet.pos) > reach || (p.pos[2] - feet.pos[2]).abs() > REACH_HEIGHT {
                continue;
            }
            let Some(base) = fnv_game::functions::base_of(game, r) else { continue };
            let kind = forms.item(base).map(|i| i.kind);
            let sig = game.data.sig(base);
            let is_actor = game.world.placed.get(r).is_some_and(|p| matches!(&p.sig, b"ACHR" | b"ACRE")) || matches!(sig.as_ref(), Some(b"NPC_" | b"CREA"));
            match kind {
                Some(ItemKind::Weapon) => {
                    let Some(s) = game.world.peek(r) else { continue };
                    // Not someone else's: unowned, or the actor's or its faction's.
                    let owner = s.extra.owner.or_else(|| game.world.placed.get(r).and_then(|p| p.owner));
                    let mine = owner.is_none_or(|o| Some(o) == actor_base || fnv_game::functions::faction_rank(game, actor, o) >= 0);
                    if mine && !game.rearms.claimed(actor, r) {
                        let condition = s.extra.condition_or_whole();
                        weapons.push((base, condition, Leg { from: r, item: base, take: Take::Ground, at: p.pos }));
                    }
                }
                Some(ItemKind::Ammo) => ammo.push(Leg { from: r, item: base, take: Take::CopyAmmo, at: p.pos }),
                _ if is_actor && game.extra.get(&r).is_some_and(|e| e.dead) => {
                    let Ok(s) = game.world.get(r) else { continue };
                    let Some(inv) = s.inventory.as_ref() else { continue };
                    let mut best: std::collections::BTreeMap<FormId, f32> = std::collections::BTreeMap::new();
                    let mut rounds = Vec::new();
                    for (item, extra, n) in inv.groups() {
                        match forms.item(item).map(|i| i.kind) {
                            Some(ItemKind::Weapon) if n > 0 && !extra.broken() => {
                                let c = best.entry(item).or_insert(0.0);
                                *c = c.max(extra.condition_or_whole());
                            }
                            Some(ItemKind::Ammo) if n > 0 => rounds.push(item),
                            _ => {}
                        }
                    }
                    if !game.rearms.claimed(actor, r) {
                        weapons.extend(best.into_iter().map(|(item, c)| (item, c, Leg { from: r, item, take: Take::FromBody, at: p.pos })));
                    }
                    ammo.extend(rounds.into_iter().map(|item| Leg { from: r, item, take: Take::CopyAmmo, at: p.pos }));
                }
                _ if sig.as_ref() == Some(b"CONT") => {
                    let Ok(s) = game.world.get(r) else { continue };
                    if s.lock.as_ref().is_some_and(|l| l.locked) {
                        continue;
                    }
                    let Some(inv) = s.inventory.as_ref() else { continue };
                    let rounds: Vec<FormId> = inv.iter().filter(|(i, n)| *n > 0 && forms.item(*i).is_some_and(|it| it.kind == ItemKind::Ammo)).map(|(i, _)| i).collect();
                    ammo.extend(rounds.into_iter().map(|item| Leg { from: r, item, take: Take::CopyAmmo, at: p.pos }));
                }
                _ => {}
            }
        }
        let mut out: Option<Found> = None;
        for (weapon, condition, leg) in weapons {
            let accepted = game.accepted_ammo(weapon);
            let mut legs = vec![leg];
            if !accepted.is_empty() && !game.carries_any(actor, &accepted) {
                // The nearest source of ammunition it accepts; the nearer stop first.
                let Some(source) = ammo.iter().filter(|a| accepted.contains(&a.item)).min_by(|a, b| flat(a.at, feet.pos).total_cmp(&flat(b.at, feet.pos))).copied() else { continue };
                if source.from == leg.from {
                    // A body that carries the gun and its rounds: one stop for both.
                    legs.push(Leg { at: leg.at, ..source });
                } else if flat(source.at, feet.pos) < flat(leg.at, feet.pos) {
                    legs.insert(0, source);
                } else {
                    legs.push(source);
                }
            }
            let Some(rank) = game.weapon_rank(actor, weapon, condition, true) else { continue };
            if !rank.beats(held) {
                continue;
            }
            let trip = legs.iter().fold((feet.pos, 0.0), |(from, d), l| (l.at, d + flat(from, l.at))).1;
            let better = match &out {
                None => true,
                Some(o) => rank.beats(Some(o.rank)) || (!o.rank.beats(Some(rank)) && trip < o.trip),
            };
            if better {
                out = Some(Found { weapon, rank, legs, trip });
            }
        }
        out
    }

    /// Set off for `leg`, running. False when there is no way there.
    fn walk_to_leg(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, leg: Leg) -> bool {
        let params = self.path_params(game, actor);
        let speed = self.alert_speeds(actor).run * game.awareness.pace_of(actor);
        let now = game.time;
        let a = self.agents.get_mut(&actor).expect("listed");
        let space = a.walker.space;
        let Some((_, point)) = nav.nearest(space, leg.at, 256.0) else { return false };
        a.walker.speed = speed;
        if a.walker.go_to(nav, Place { space, pos: point }, &params).is_err() {
            return false;
        }
        a.activity = Activity::Travelling;
        a.trip_started = now;
        a.trip_from = a.walker.travelled;
        a.search = Some(Search::Fetching { from: leg.from, to: leg.at, since: now });
        game.awareness.crouching.remove(&actor);
        true
    }

    /// One update of a fetch under way.
    fn fetch_step(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, mut f: Fetch, dt: f32) -> bool {
        let now = game.time;
        let space = self.agents[&actor].walker.space;
        // Standing for the pick-up idle.
        if let Some(until) = f.busy_until {
            if now < until {
                return true;
            }
            f.busy_until = None;
            let Some(next) = f.legs.first().copied() else {
                game.rearms.fetches.remove(&actor);
                // What it holds now: what it fetched (an actor never outfitted may already
                // hold it by its outfitting's choice).
                game.take_up_fetched(actor, f.weapon);
                if let Some((weapon, _)) = game.held_weapon(actor) {
                    self.log(game, actor, AiEvent::Rearmed { weapon, from: Some(f.weapon) });
                }
                self.agents.get_mut(&actor).expect("listed").search = None;
                return false;
            };
            game.rearms.fetches.insert(actor, f);
            if !self.walk_to_leg(game, nav, actor, next) {
                self.drop_fetch(game, actor, next.from, "no way there");
                return false;
            }
            return true;
        }
        let Some(leg) = f.legs.first().copied() else {
            game.rearms.fetches.remove(&actor);
            return false;
        };
        if !game.leg_open(actor, &leg) {
            self.drop_fetch(game, actor, leg.from, "it is gone");
            return false;
        }
        if now - f.since > FETCH_SECONDS {
            self.drop_fetch(game, actor, leg.from, "too long on the way");
            return false;
        }
        let a = &self.agents[&actor];
        if a.activity == Activity::Travelling {
            return true;
        }
        // The walk ended: there, or as near as it got.
        let feet = a.walker.feet;
        if flat(feet, leg.at) > ARRIVE_WITHIN {
            self.drop_fetch(game, actor, leg.from, "could not reach it");
            return false;
        }
        self.face(game, actor, Place { space, pos: leg.at }, dt);
        let got = game.take_leg(actor, &leg);
        if let Some(idle) = game.pickup_idle(actor) {
            game.ask_idle(actor, idle, IdleSource::PickUp);
        }
        self.log(game, actor, AiEvent::FetchGot { item: leg.item, from: leg.from, take: leg.take, got });
        if !got {
            self.drop_fetch(game, actor, leg.from, "nothing to take");
            return false;
        }
        f.legs.remove(0);
        f.busy_until = Some(now + PICKUP_SECONDS);
        game.rearms.fetches.insert(actor, f);
        let a = self.agents.get_mut(&actor).expect("listed");
        a.walker.follower = None;
        a.activity = Activity::There;
        true
    }

    /// Give up a fetch and the stop `from` for this fight.
    fn drop_fetch(&mut self, game: &mut Game, actor: FormId, from: FormId, why: &'static str) {
        game.rearms.fetches.remove(&actor);
        game.rearms.given_up.entry(actor).or_default().insert(from);
        let a = self.agents.get_mut(&actor).expect("listed");
        if matches!(a.search, Some(Search::Fetching { .. })) {
            a.search = None;
            a.walker.follower = None;
            a.activity = Activity::There;
        }
        self.log(game, actor, AiEvent::FetchDropped { from, why });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_phases_spread_battle_actors_over_the_second() {
        // Runtime references as a battle creates them, one after another.
        let phases: Vec<f64> = (0..118u32).map(|i| look_phase(FormId(0xFF00_0000 + i))).collect();
        assert!(phases.iter().all(|p| (0.0..1.0).contains(p)));
        // No tenth of a second holds more than a fifth of them.
        for tenth in 0..10 {
            let n = phases.iter().filter(|&&p| (p * 10.0) as usize == tenth).count();
            assert!(n <= 118 / 5, "tenth {tenth} holds {n}");
        }
        assert_eq!(look_phase(FormId(0x0010_4C0C)), look_phase(FormId(0x0010_4C0C)));
    }
}
