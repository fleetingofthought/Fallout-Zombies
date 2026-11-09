//! Carrying out Escort, Find, Use Item At, Ambush and Flee Not Combat packages (the
//! goals [`Escort`], [`Find`], [`UseAt`], [`Ambush`], [`Flee`] of `crate::goal`). The
//! rules, their sources and confidence: docs/spec-notes/packages.md §3.1 and
//! docs/design/actor-behaviour.md §5.10; the walks are `crate::carry`'s.
//!
//! - **Escort**: it walks to the destination and stops whenever the one it leads is more
//!   than the escort distance away, facing it, until it is within the distance less
//!   [`ESCORT_SLACK`] again.
//! - **Find**: it goes to the search location, then looks in the area for what the
//!   package names, nearest first, goes to it and acts on it ([`Act`]), until it has
//!   found the package's count; with nothing (more) to find its goal is met and it looks
//!   again every [`LOOK_SECONDS`]. With Allow Buying a vendor of the area whose merchant
//!   container holds what it looks for counts too: it goes up to the vendor and buys one
//!   ([`Ai::offer`], [`buying_price`]).
//! - **Use Item At**: it chooses what to use (the package's reference, the nearest free
//!   object of the package's kind in the area, or an item it carries), goes there and
//!   uses it.
//! - **Ambush**: it goes to the wait location and lies in wait there, standing still (with
//!   Hide While Ambushing too: the original's actor did not sneak, packages.md §4); the
//!   first enemy inside the trigger location sets it off: it starts combat with it
//!   (`Game::start_combat`) and its goal is met.
//! - **Flee Not Combat**: while something threatens it (the package's target, else the
//!   nearest actor, within the flee distance, that is its enemy: one of the two would
//!   attack the other) it runs, to the
//!   package's location or straight away; with nothing threatening it its goal is met
//!   and it stands. It never starts a fight.

use crate::carry::{Activity, Ai, AiEvent};
use crate::goal::{self, Ambush, Area, Escort, Find, Flee, Goal, UseAt, Wanted};
use fnv_game::game::{ActMode, Game};
use fnv_game::repair::CAPS;
use fnv_game::package::furniture;
use fnv_nav::{NavGraph, Place};
use fnv_sim::FormId;
use serde::Serialize;

/// How far an actor is with one of these packages.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "errand", rename_all = "snake_case")]
pub enum Errand {
    /// Escort: on its way, or standing for the one it leads to come up.
    Escort { waiting: bool },
    /// Find: `centre` is the middle of the search area; on the way to the search
    /// location (`looking` false), or to `at`; `found` so far; `given_up`: objects it
    /// found no way to; it looks again from `next_look` (`Game::time`).
    Find { centre: Option<Place>, looking: bool, at: Option<FormId>, found: u32, given_up: Vec<FormId>, next_look: f64 },
    /// Use Item At: what it uses (`None`: nothing found; it stays at the location).
    UseAt { thing: Option<FormId> },
    /// Ambush: lying in wait (arrived), or set off.
    Ambush { lying: bool, sprung: bool },
    /// Flee Not Combat: running from `from` (`None`: nothing threatens it now); it looks
    /// again from `next_look`.
    Flee { from: Option<FormId>, next_look: f64 },
}

/// What a Find package's actor does with an object it reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Act {
    /// Picks up an item reference (`Game::activate`).
    Takes,
    /// Takes one of the items it looks for from a dead actor.
    Loots,
    /// Harvests flora (`Game::activate`).
    Harvests,
    /// Sits on (or lies on) furniture.
    Uses,
    /// Attacks an actor (Allow Killing): combat takes over.
    Attacks,
    /// Buys one of the items it looks for from a vendor (Allow Buying).
    Buys,
    /// Goes up to it and stands: an actor, a container, anything else.
    Reaches,
}

/// An escort that stopped for the one it leads goes on once it is this much nearer
/// than the escort distance, game units [our choice; as `crate::carry::FOLLOW_SLACK`].
pub const ESCORT_SLACK: f32 = 64.0;
/// A Find package with nothing found looks again this often, seconds [our choice].
pub const LOOK_SECONDS: f64 = 10.0;
/// A fleeing actor looks at its threat this often, seconds [our choice].
pub const FLEE_LOOK_SECONDS: f64 = 1.0;
/// A fleeing actor without a destination runs to this much beyond the flee distance,
/// game units [our choice].
const FLEE_MARGIN: f32 = 256.0;
/// Of the objects found, this many nearest are tried before it gives up for this look
/// [our choice].
const TRIES: usize = 4;
/// Objects more than this far above or below the area's centre are not looked at, game
/// units [our choice: another floor].
const AREA_HEIGHT: f32 = 512.0;
/// Fewest exterior cells searched around the centre, each way: an area of radius
/// infinite or larger than this many cells is cut there [our choice].
const MAX_CELLS: i32 = 2;
/// The object types that are weapons: any, ranged, melee [documented: xEdit
/// wbObjectTypeEnum; our choice: ranged and melee are not told apart].
const WEAPON_TYPES: [u32; 3] = [12, 21, 22];
/// fBarterBuyBase and fBarterBuyMult when the load order does not set them [documented:
/// GECK wiki "Barter System Formula"; FalloutNV.esm sets the same, barter-menu.md §4].
const BUY_BASE: f32 = 1.55;
const BUY_MULT: f32 = -0.45;

/// What `buyer` pays a vendor for one `item`, whole caps: the item's value (SetItemValue's,
/// else the record's) times `fBarterBuyBase + Barter × fBarterBuyMult / 100` at the
/// buyer's Barter skill, never under the value, rounded to the nearest cap, halves up
/// [documented: the player's buying price, GECK wiki "Barter System Formula",
/// barter-menu.md §4; hypothesis: an actor buying on its package pays as the player
/// would, at its own skill, and condition is not counted].
pub fn buying_price(game: &mut Game, buyer: FormId, item: FormId) -> u32 {
    let value = game.forms.item_values.get(&item).copied().or_else(|| game.world.forms.item(item).and_then(|i| i.value)).unwrap_or(0).max(0);
    let skill = fnv_game::functions::actor_value(game, buyer, fnv_game::actor_values::BARTER).unwrap_or(0.0) as f32;
    let base = game.world.setting_f32("fBarterBuyBase").unwrap_or(BUY_BASE);
    let mult = game.world.setting_f32("fBarterBuyMult").unwrap_or(BUY_MULT);
    let price = value as f32 * (base + skill * mult / 100.0).max(1.0);
    (price + 0.5).floor() as u32
}

/// Whether `item` (a base form carried by `holder`) is what `wanted` names.
fn item_wanted(game: &Game, holder: FormId, item: FormId, wanted: Wanted) -> bool {
    game.data.sig(item).is_some_and(|s| match wanted {
        Wanted::Base(b) => b == item || fnv_game::functions::list_members(game, b).contains(&item),
        Wanted::Type(c) => of_type(game, holder, item, &s, c),
        _ => false,
    })
}

fn hdist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Whether an item base form is one an actor can pick up [documented: inventory record
/// types, aid-items.md and items].
fn is_item(sig: &[u8; 4]) -> bool {
    matches!(sig, b"ALCH" | b"AMMO" | b"ARMO" | b"BOOK" | b"INGR" | b"KEYM" | b"MISC" | b"WEAP" | b"NOTE" | b"CHIP" | b"IMOD" | b"CCRD" | b"CMNY")
}

/// Whether `r` (a reference with base `base`, `sig` its base's record signature) is of
/// object type `code` (PTDT object type, `fnv_game::package::object_type_name`).
pub(crate) fn of_type(game: &Game, r: FormId, base: FormId, sig: &[u8; 4], code: u32) -> bool {
    let ref_sig = game.world.placed.get(r).map(|p| p.sig);
    let actor = matches!(ref_sig, Some(s) if &s == b"ACHR" || &s == b"ACRE") || matches!(sig, b"NPC_" | b"CREA");
    let food = || game.data.ingestibles.get(&base).is_some_and(|d| d.flags & 2 != 0 || d.equip_type == 12);
    match code {
        1 => sig == b"ACTI",
        2 | 4 | 19 | 20 => sig == b"ARMO",
        3 => sig == b"BOOK",
        5 => sig == b"CONT",
        6 => sig == b"DOOR",
        7 => sig == b"INGR",
        8 => sig == b"LIGH",
        9 => sig == b"MISC",
        10 => sig == b"FLOR",
        11 => sig == b"FURN",
        c if WEAPON_TYPES.contains(&c) => sig == b"WEAP",
        13 => sig == b"AMMO",
        14 => actor && (sig == b"NPC_" || ref_sig.is_some_and(|s| &s == b"ACHR")),
        15 => actor && (sig == b"CREA" || ref_sig.is_some_and(|s| &s == b"ACRE")),
        16 => sig == b"KEYM",
        17 => sig == b"ALCH",
        18 => sig == b"ALCH" && food(),
        28 => actor,
        _ => false,
    }
}

impl Ai {
    /// Where `actor` stands now.
    fn feet(&self, actor: FormId) -> Place {
        let w = &self.agents[&actor].walker;
        Place { space: w.space, pos: w.feet }
    }

    fn errand_mut(&mut self, actor: FormId) -> &mut Option<Errand> {
        &mut self.agents.get_mut(&actor).expect("listed").errand
    }

    /// Whether `r` is in the world, enabled and, for an actor, alive.
    fn alive(game: &Game, r: FormId) -> bool {
        game.present(r) && !game.extra.get(&r).is_some_and(|e| e.dead)
    }

    /// One of these packages is new (`crate::carry`'s `begin`): set off.
    pub(crate) fn errand_begin(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, package: FormId, goal: &Goal) {
        let feet = self.feet(actor);
        match goal {
            Goal::Escort(e) => {
                *self.errand_mut(actor) = Some(Errand::Escort { waiting: false });
                if self.escort_too_far(game, actor, e) {
                    self.escort_wait(game, actor, package, e);
                } else {
                    self.escort_on(game, nav, actor, package, e);
                }
            }
            Goal::Find(f) => {
                *self.errand_mut(actor) = Some(Errand::Find { centre: f.area.centre, looking: false, at: None, found: 0, given_up: Vec::new(), next_look: 0.0 });
                match self.way_to(feet, &f.search) {
                    Some(to) => self.plan(game, nav, actor, package, to, None),
                    None => self.find_look(game, nav, actor, package, f),
                }
            }
            Goal::UseAt(u) => self.use_begin(game, nav, actor, package, u),
            Goal::Ambush(a) => {
                *self.errand_mut(actor) = Some(Errand::Ambush { lying: false, sprung: false });
                match self.way_to(feet, &a.wait) {
                    Some(to) => self.plan(game, nav, actor, package, to, None),
                    None => self.lie_in_wait(game, actor, package, a),
                }
            }
            Goal::Flee(_) => {
                *self.errand_mut(actor) = Some(Errand::Flee { from: None, next_look: 0.0 });
                self.agents.get_mut(&actor).expect("listed").activity = Activity::There;
            }
            _ => {}
        }
    }

    /// The walk of one of these packages ended (`crate::carry`'s `arrived`).
    pub(crate) fn errand_arrived(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, package: FormId) {
        let a = self.agents.get_mut(&actor).expect("listed");
        a.activity = Activity::There;
        let (seconds, travelled) = (game.time - a.trip_started, a.walker.travelled - a.trip_from);
        let (goal, errand) = (a.goal.clone(), a.errand.clone());
        match (goal, errand) {
            (Some(Goal::Escort(e)), Some(Errand::Escort { waiting: false })) => {
                // At the destination: done, and it stands with the one it led.
                self.log(game, actor, AiEvent::Arrived { package, seconds, travelled, patrol_point: None });
                if self.way_to(self.feet(actor), &e.to).is_none() {
                    game.package_goal_met(actor);
                }
            }
            (Some(Goal::Find(f)), Some(Errand::Find { at: Some(object), .. })) => self.find_reached(game, nav, actor, package, &f, object),
            (Some(Goal::Find(f)), Some(Errand::Find { at: None, .. })) => {
                self.log(game, actor, AiEvent::Arrived { package, seconds, travelled, patrol_point: None });
                self.find_look(game, nav, actor, package, &f);
            }
            (Some(Goal::UseAt(_)), Some(Errand::UseAt { thing })) => {
                self.log(game, actor, AiEvent::Arrived { package, seconds, travelled, patrol_point: None });
                self.use_it(game, actor, package, thing);
            }
            (Some(Goal::Ambush(am)), Some(Errand::Ambush { sprung: false, .. })) => {
                self.log(game, actor, AiEvent::Arrived { package, seconds, travelled, patrol_point: None });
                self.lie_in_wait(game, actor, package, &am);
            }
            (Some(Goal::Flee(_)), Some(Errand::Flee { from, .. })) => {
                self.log(game, actor, AiEvent::FleeSafe { package, from, seconds });
                *self.errand_mut(actor) = Some(Errand::Flee { from: None, next_look: game.time + FLEE_LOOK_SECONDS });
                game.package_goal_met(actor);
            }
            _ => {}
        }
    }

    /// One update of one of these packages (`crate::carry`'s `step_actor`).
    pub(crate) fn errand_step(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, dt: f32) {
        let a = &self.agents[&actor];
        let (Some(package), Some(goal), Some(errand)) = (a.package, a.goal.clone(), a.errand.clone()) else { return };
        let travelling = a.activity == Activity::Travelling;
        match (goal, errand) {
            (Goal::Escort(e), Errand::Escort { waiting }) => {
                let far = self.escort_too_far(game, actor, &e);
                if waiting {
                    if let Some(t) = goal::place_of_ref(game, e.target).filter(|t| t.space == self.feet(actor).space) {
                        self.face(game, actor, t, dt);
                    }
                    let near = goal::place_of_ref(game, e.target).is_some_and(|t| {
                        let f = self.feet(actor);
                        t.space == f.space && hdist(t.pos, f.pos) <= (e.distance - ESCORT_SLACK).max(self.cfg.arrive)
                    });
                    if near || !Self::alive(game, e.target) {
                        self.escort_on(game, nav, actor, package, &e);
                    }
                } else if travelling && far {
                    self.escort_wait(game, actor, package, &e);
                }
            }
            (Goal::Find(f), Errand::Find { looking: true, at, next_look, .. }) => match at {
                // The one it goes after moves: it goes after it, and is there once within
                // the package's distance.
                Some(object) if game.world.placed.get(object).is_some_and(|p| matches!(&p.sig, b"ACHR" | b"ACRE")) || object == FormId::PLAYER_REF => {
                    let Some(t) = goal::place_of_ref(game, object).filter(|_| Self::alive(game, object)) else {
                        self.find_look(game, nav, actor, package, &f);
                        return;
                    };
                    let feet = self.feet(actor);
                    if t.space == feet.space && hdist(t.pos, feet.pos) <= f.distance.max(self.cfg.arrive * 3.0) {
                        let a = self.agents.get_mut(&actor).expect("listed");
                        (a.walker.follower, a.activity) = (None, Activity::There);
                        self.find_reached(game, nav, actor, package, &f, object);
                    } else if !travelling {
                        self.plan(game, nav, actor, package, t, None);
                    }
                }
                // Gone before it got there (someone else took it): it looks again.
                Some(object) if !game.present(object) => self.find_look(game, nav, actor, package, &f),
                None if !travelling && game.time >= next_look => self.find_look(game, nav, actor, package, &f),
                _ => {}
            },
            (Goal::Ambush(am), Errand::Ambush { lying: true, sprung: false }) => self.ambush_watch(game, actor, package, &am),
            (Goal::Flee(fl), Errand::Flee { from, next_look }) if game.time >= next_look => self.flee_look(game, nav, actor, package, &fl, from),
            _ => {}
        }
    }

    // ---- Escort ------------------------------------------------------------------

    /// Whether the one an escort leads is too far behind: farther than the escort
    /// distance, or in another space. One that is gone or dead is not waited for.
    fn escort_too_far(&self, game: &Game, actor: FormId, e: &Escort) -> bool {
        if !Self::alive(game, e.target) {
            return false;
        }
        let feet = self.feet(actor);
        goal::place_of_ref(game, e.target).is_none_or(|t| t.space != feet.space || hdist(t.pos, feet.pos) > e.distance)
    }

    fn escort_wait(&mut self, game: &mut Game, actor: FormId, package: FormId, e: &Escort) {
        let feet = self.feet(actor);
        let away = goal::place_of_ref(game, e.target).filter(|t| t.space == feet.space).map(|t| hdist(t.pos, feet.pos));
        let a = self.agents.get_mut(&actor).expect("listed");
        (a.walker.follower, a.activity) = (None, Activity::There);
        a.errand = Some(Errand::Escort { waiting: true });
        self.log(game, actor, AiEvent::EscortWaits { package, target: e.target, away });
    }

    fn escort_on(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, package: FormId, e: &Escort) {
        let was_waiting = matches!(self.agents[&actor].errand, Some(Errand::Escort { waiting: true }));
        *self.errand_mut(actor) = Some(Errand::Escort { waiting: false });
        if was_waiting {
            self.log(game, actor, AiEvent::EscortOn { package, target: e.target });
        }
        match self.way_to(self.feet(actor), &e.to) {
            Some(to) => self.plan(game, nav, actor, package, to, None),
            None => {
                self.agents.get_mut(&actor).expect("listed").activity = Activity::There;
                game.package_goal_met(actor);
            }
        }
    }

    // ---- Find --------------------------------------------------------------------

    /// The objects of the area that `wanted` names, with how far each is from `from`,
    /// nearest first. An item owned by another is left alone without Allow Stealing; a
    /// dead actor counts when it carries it; with Allow Killing a living one counts when it
    /// carries it, or for a creature finder whenever it is not kin, ally or friend; with
    /// Allow Buying a living vendor counts when it has one to sell that the actor can pay
    /// for ([`Ai::offer`]).
    #[allow(clippy::too_many_arguments)]
    fn objects(&self, game: &mut Game, actor: FormId, wanted: Wanted, area: &Area, centre: Place, kill: bool, steal: bool, buy: bool, from: Place) -> Vec<(f32, FormId)> {
        let refs = game.world.placed;
        let data = game.data;
        let mut found: Vec<(f32, FormId)> = Vec::new();
        let finder_creature = fnv_game::functions::base_of(game, actor).and_then(|b| data.sig(b)).as_ref() == Some(b"CREA");
        let consider = |game: &mut Game, r: FormId, out: &mut Vec<(f32, FormId)>| {
            if r == actor || !game.present(r) {
                return;
            }
            let Some(p) = goal::place_of_ref(game, r).filter(|p| p.space == centre.space) else { return };
            if hdist(p.pos, centre.pos) > area.radius || (p.pos[2] - centre.pos[2]).abs() > AREA_HEIGHT {
                return;
            }
            let base = fnv_game::functions::base_of(game, r).unwrap_or(FormId(0));
            let sig = data.sig(base).unwrap_or(*b"    ");
            let is_actor = refs.get(r).is_some_and(|p| matches!(&p.sig, b"ACHR" | b"ACRE"));
            let dead = game.extra.get(&r).is_some_and(|e| e.dead);
            // No AI Acquire (#962): an object, container or body so flagged is not
            // searched for, unless the package names that one reference. A living actor
            // stays a target for Allow Killing / Buying: the flag speaks of picking up.
            // [hypothesis for the exceptions]
            if (!is_actor || dead) && !matches!(wanted, Wanted::Reference(_)) && refs.get(r).is_some_and(|p| p.no_ai_acquire()) {
                return;
            }
            let matches = |b: FormId, s: &[u8; 4], game: &Game| match wanted {
                Wanted::Reference(x) => r == x,
                Wanted::Base(x) => b == x || fnv_game::functions::list_members(game, x).contains(&b),
                Wanted::Type(code) => of_type(game, r, b, s, code),
                Wanted::Nothing => false,
            };
            let faction = match wanted {
                Wanted::Base(x) => is_actor && data.sig(x).as_ref() == Some(b"FACT") && fnv_game::functions::faction_rank(game, r, x) >= 0,
                _ => false,
            };
            if matches(base, &sig, game) || faction {
                // An actor it looks for: alive, unless it looks for one reference.
                if is_actor && dead && !matches!(wanted, Wanted::Reference(_)) {
                    return;
                }
                if !is_actor && !steal && is_item(&sig) {
                    let owner = game.world.peek(r).and_then(|s| s.extra.owner).or_else(|| refs.get(r).and_then(|p| p.owner));
                    let mine = owner.is_none_or(|o| Some(o) == refs.get(actor).map(|p| p.base) || fnv_game::functions::faction_rank(game, actor, o) >= 0);
                    if !mine {
                        return;
                    }
                }
                out.push((goal::dist(p.pos, from.pos), r));
                return;
            }
            // A vendor with one of what it looks for in stock, alive: it buys one (Allow
            // Buying) [hypothesis: the flag's name is all that is documented].
            if is_actor && !dead && buy && !matches!(wanted, Wanted::Reference(_)) && Self::offer(game, actor, r, wanted).is_some() {
                out.push((goal::dist(p.pos, from.pos), r));
                return;
            }
            // An actor carrying what it looks for, dead: it takes it. With Allow Killing,
            // a creature attacks any living actor of the area that is not its kin, ally or
            // friend, whatever that actor carries; a person only one carrying it
            // [hypothesis for people: the shipped NPC packages with the flag look for
            // containers, collars or weapons]. For creatures [observed: gecko 0x0016518B on
            // VCrGeckoFindFood (Find, object type Food, Allow Killing), Unaggressive by its
            // own AI data, started fights with two bloatflies, a radroach and a protectron
            // (no food in its inventory or death item) and left the other gecko alone; a
            // Very Aggressive gecko on its Patrol attacked bloatflies too; AI observer
            // campaigns `gecko_find` 2026-10-19, owner-decision-captures.md §6].
            if is_actor && !matches!(wanted, Wanted::Reference(_)) && (dead || kill) {
                let target = if dead || !finder_creature {
                    carried(game, r).iter().any(|i| data.sig(*i).is_some_and(|s| matches(*i, &s, game)))
                } else {
                    let relation = fnv_game::functions::faction_relation(game, actor, r);
                    !game.kin(actor, r) && relation != 2 && relation != 3
                };
                if target {
                    out.push((goal::dist(p.pos, from.pos), r));
                }
            }
        };
        if let Wanted::Reference(r) = wanted {
            consider(game, r, &mut found);
        } else if wanted != Wanted::Nothing {
            for r in cells_around(game, centre, area.radius) {
                consider(game, r, &mut found);
            }
            // The player, when the package looks for actors.
            if matches!(wanted, Wanted::Type(14 | 28)) {
                consider(game, FormId::PLAYER_REF, &mut found);
                found.retain(|(_, r)| *r != FormId::PLAYER_REF);
            }
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        found.dedup_by_key(|x| x.1);
        found
    }

    /// A Find package's actor looks in its area and sets off to the nearest object it
    /// looks for; with none its goal is met and it looks again later.
    fn find_look(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, package: FormId, f: &Find) {
        let feet = self.feet(actor);
        let Some(Errand::Find { centre, found, given_up, .. }) = self.agents[&actor].errand.clone() else { return };
        let centre = centre.unwrap_or(feet);
        if found >= f.count {
            return;
        }
        // Furniture another actor sits on or is on its way to is not free (as for Use Item
        // At): in Fallout 3's G.O.A.T. (CG03, TTW) every student's Find looks for "any
        // furniture" near the classroom marker, and without this two of them went to
        // the same desk and stood beside it. A seat the player (or any actor the engine
        // seats) is on is not free either.
        let taken = self.taken_by_others(actor);
        let objects: Vec<(f32, FormId)> = self.objects(game, actor, f.wanted, &f.area, centre, f.kill, f.steal, f.buy || self.cfg.find_buying, feet)
            .into_iter()
            .filter(|(_, r)| !given_up.contains(r))
            .filter(|(_, r)| !taken.contains(r) && !game.furniture_used_by_other(actor, *r))
            // Furniture it has no clips for is not what it looks for: passed over before
            // a way is planned, it counts for nothing (`crate::furniture`).
            .filter(|(_, r)| self.furniture.fits_assigned(game, actor, *r))
            .collect();
        let mut given_up = given_up;
        for (_, object) in objects.into_iter().take(TRIES) {
            let Some(to) = goal::place_of_ref(game, object) else { continue };
            let params = self.path_params(game, actor);
            let running = game.data.package_defs.get(&package).is_some_and(|d| d.has(fnv_game::package::flags::ALWAYS_RUN));
            let speed = self.pace(game, actor, running);
            let a = self.agents.get_mut(&actor).expect("listed");
            a.walker.speed = speed;
            let near = to.space == feet.space && hdist(to.pos, feet.pos) <= self.cfg.arrive.max(f.distance);
            let way = if near { Ok(0.0) } else { a.walker.go_to(nav, to, &params).map(|p| p.length) };
            let Ok(length) = way else {
                given_up.push(object);
                continue;
            };
            a.errand = Some(Errand::Find { centre: Some(centre), looking: true, at: Some(object), found, given_up, next_look: 0.0 });
            if near {
                a.walker.follower = None;
                a.activity = Activity::There;
            } else {
                a.activity = Activity::Travelling;
                a.trip_started = game.time;
                a.trip_from = a.walker.travelled;
            }
            if game.data.furniture.contains_key(&fnv_game::functions::base_of(game, object).unwrap_or(FormId(0))) {
                // Kept for it from now, so the next actor looks for another.
                self.reserve(actor, object);
            }
            self.log(game, actor, AiEvent::FindGoes { package, object, length });
            if near {
                self.find_reached(game, nav, actor, package, f, object);
            }
            return;
        }
        // Nothing (more) to find: it stays, and its goal is met [our choice: else a Must
        // Complete Find with nothing in reach would hold its actor for ever].
        let first = matches!(self.agents[&actor].errand, Some(Errand::Find { looking: false, .. }) | Some(Errand::Find { at: Some(_), .. }));
        let a = self.agents.get_mut(&actor).expect("listed");
        a.activity = Activity::There;
        a.errand = Some(Errand::Find { centre: Some(centre), looking: true, at: None, found, given_up, next_look: game.time + LOOK_SECONDS });
        if first {
            self.log(game, actor, AiEvent::NothingFound { package, found });
            game.package_goal_met(actor);
        }
    }

    /// At the object it went to: act on it ([`Act`]), and look for the next.
    fn find_reached(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, package: FormId, f: &Find, object: FormId) {
        let refs = game.world.placed;
        let base = fnv_game::functions::base_of(game, object).unwrap_or(FormId(0));
        let sig = game.data.sig(base).unwrap_or(*b"    ");
        let is_actor = object == FormId::PLAYER_REF || refs.get(object).is_some_and(|p| matches!(&p.sig, b"ACHR" | b"ACRE"));
        let dead = game.extra.get(&object).is_some_and(|e| e.dead);
        let buy = (f.buy || self.cfg.find_buying) && is_actor && !dead && !matches!(f.wanted, Wanted::Reference(_));
        let offer = if buy { Self::offer(game, actor, object, f.wanted) } else { None };
        let act = if is_actor && dead && !matches!(f.wanted, Wanted::Reference(_)) {
            // Take one of what it looks for from the body.
            let data = game.data;
            let wanted = f.wanted;
            let item = carried(game, object).into_iter().find(|i| {
                data.sig(*i).is_some_and(|s| match wanted {
                    Wanted::Base(b) => b == *i || fnv_game::functions::list_members(game, b).contains(i),
                    Wanted::Type(c) => of_type(game, object, *i, &s, c),
                    _ => false,
                })
            });
            if let Some(item) = item {
                let _ = game.world.transfer(object, actor, item, 1);
                game.extra.entry(actor).or_default().used_item = Some(item);
            }
            Act::Loots
        } else if let Some((stock, item, price)) = offer {
            // It pays the vendor's merchant container, where the vendor's caps are, and
            // the item comes out of it [hypothesis: as the player's trade, barter-menu.md
            // §4].
            let paid = price == 0 || game.world.transfer(actor, stock, CAPS, price as i32).is_ok_and(|n| n == price as i32);
            if paid && game.world.transfer(stock, actor, item, 1).is_ok_and(|n| n == 1) {
                game.extra.entry(actor).or_default().used_item = Some(item);
                self.log(game, actor, AiEvent::Bought { package, vendor: object, item, price });
            }
            Act::Buys
        } else if is_actor && !dead && f.kill {
            game.start_combat(actor, object);
            Act::Attacks
        } else if is_actor {
            Act::Reaches
        } else if is_item(&sig) {
            let _ = game.activate(actor, object, ActMode::Default);
            game.extra.entry(actor).or_default().used_item = Some(base);
            Act::Takes
        } else if &sig == b"FLOR" {
            let _ = game.activate(actor, object, ActMode::Default);
            game.extra.entry(actor).or_default().used_item = Some(base);
            Act::Harvests
        } else if &sig == b"FURN" {
            self.set_using(game, actor, Some(object));
            Act::Uses
        } else {
            Act::Reaches
        };
        self.log(game, actor, AiEvent::Found { package, object, act });
        let Some(Errand::Find { centre, found, mut given_up, .. }) = self.agents[&actor].errand.clone() else { return };
        // A body, a container or an attacked actor is not looked at again this time; a
        // vendor is, while it has more to sell.
        if act != Act::Buys {
            given_up.push(object);
        }
        let found = if act == Act::Attacks { found } else { found + 1 };
        let a = self.agents.get_mut(&actor).expect("listed");
        a.errand = Some(Errand::Find { centre, looking: true, at: None, found, given_up, next_look: game.time + LOOK_SECONDS });
        a.activity = Activity::There;
        if found >= f.count {
            game.package_goal_met(actor);
        } else if act != Act::Attacks && act != Act::Uses {
            self.find_look(game, nav, actor, package, f);
        }
    }

    /// What `buyer` would buy from `vendor`: the vendor's merchant container, the cheapest
    /// item in it that `wanted` names and the buyer can pay for, and its price
    /// ([`buying_price`]). `None` when the vendor has no merchant container (an actor
    /// without one is not a vendor), nothing that is wanted, or nothing affordable
    /// [hypothesis: the merchant container is the vendor's stock as in the barter menu;
    /// the cheapest is our choice]. Only what the vendor sells: an item whose category is
    /// among its services (`fnv_game::services`, as in the barter menu;
    /// docs/spec-notes/vendor-services.md §4) [hypothesis: actors buy as the player does].
    pub fn offer(game: &mut Game, buyer: FormId, vendor: FormId, wanted: Wanted) -> Option<(FormId, FormId, u32)> {
        if !Self::alive(game, vendor) {
            return None;
        }
        let stock = game.merchant_container(vendor)?;
        let caps = game.world.item_count(buyer, CAPS).unwrap_or(0).max(0) as u32;
        let items: Vec<FormId> = carried(game, stock).into_iter().filter(|i| *i != CAPS && game.sells(vendor, *i) && item_wanted(game, stock, *i, wanted)).collect();
        let mut best: Option<(u32, FormId)> = None;
        for item in items {
            let price = buying_price(game, buyer, item);
            if price <= caps && best.is_none_or(|(p, i)| (price, item) < (p, i)) {
                best = Some((price, item));
            }
        }
        best.map(|(price, item)| (stock, item, price))
    }

    // ---- Use Item At ---------------------------------------------------------------

    /// Choose what to use and set off to it.
    fn use_begin(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, package: FormId, u: &UseAt) {
        let feet = self.feet(actor);
        let centre = u.area.centre.unwrap_or(feet);
        // An item it carries: used where it is.
        if let Wanted::Base(b) = u.wanted {
            if game.data.ingestibles.contains_key(&b) {
                *self.errand_mut(actor) = Some(Errand::UseAt { thing: None });
                let has = game.world.item_count(actor, b).unwrap_or(0) > 0;
                if has && game.world.remove_item(actor, b, 1).is_ok() {
                    game.ingest(actor, b);
                    game.extra.entry(actor).or_default().used_item = Some(b);
                }
                self.agents.get_mut(&actor).expect("listed").activity = Activity::There;
                self.log(game, actor, AiEvent::UsesItem { package, item: b, had: has });
                game.package_goal_met(actor);
                return;
            }
        }
        let taken = self.taken_by_others(actor);
        let free = |game: &Game, r: FormId| !taken.contains(&r) && !game.furniture_used_by_other(actor, r);
        let sit = |game: &Game, r: FormId| fnv_game::functions::base_of(game, r).and_then(|b| game.data.furniture.get(&b)).is_some_and(|f| f & furniture::SIT != 0);
        let thing = match u.wanted {
            // Furniture it has no clips for is nothing to use: it goes to the package's
            // place and its goal is met, as when a search finds nothing (`crate::furniture`).
            Wanted::Reference(r) => Some(r).filter(|r| self.furniture.fits_assigned(game, actor, *r)),
            Wanted::Nothing => None,
            w => {
                let area = Area { centre: Some(centre), radius: u.area.radius };
                let found = self.objects(game, actor, w, &area, centre, false, true, false, feet);
                found.into_iter().map(|x| x.1).filter(|r| free(game, *r)).filter(|r| !u.sit_down || sit(game, *r)).find(|r| self.furniture.fits_assigned(game, actor, *r))
            }
        };
        *self.errand_mut(actor) = Some(Errand::UseAt { thing });
        if let Some(f) = thing.filter(|r| game.data.furniture.contains_key(&fnv_game::functions::base_of(game, *r).unwrap_or(FormId(0)))) {
            // Kept for it from now [as `crate::sandbox`'s furniture].
            self.reserve(actor, f);
        }
        let to = match thing {
            Some(t) => goal::place_of_ref(game, t),
            None => self.way_to(feet, &u.at),
        };
        match to {
            Some(to) if !(to.space == feet.space && hdist(to.pos, feet.pos) <= self.cfg.arrive) => self.plan(game, nav, actor, package, to, None),
            _ => {
                self.agents.get_mut(&actor).expect("listed").activity = Activity::There;
                self.use_it(game, actor, package, thing);
            }
        }
    }

    /// At what it uses: furniture it sits (or lies) on, anything else it stands at,
    /// facing the way the object faces. Its goal is met.
    fn use_it(&mut self, game: &mut Game, actor: FormId, package: FormId, thing: Option<FormId>) {
        if let Some(t) = thing {
            let base = fnv_game::functions::base_of(game, t);
            if base.is_some_and(|b| game.data.furniture.contains_key(&b)) {
                self.set_using(game, actor, Some(t));
            } else {
                let h = game.world.peek(t).map(|s| s.rot[2]).or_else(|| game.world.placed.get(t).map(|p| p.rot[2]));
                self.agents.get_mut(&actor).expect("listed").face_to = h;
            }
            if let Some(b) = base {
                game.extra.entry(actor).or_default().used_item = Some(b);
            }
        }
        self.log(game, actor, AiEvent::UsesAt { package, thing });
        game.package_goal_met(actor);
    }

    // ---- Ambush ------------------------------------------------------------------

    fn lie_in_wait(&mut self, game: &mut Game, actor: FormId, package: FormId, a: &Ambush) {
        let ag = self.agents.get_mut(&actor).expect("listed");
        ag.activity = Activity::There;
        ag.errand = Some(Errand::Ambush { lying: true, sprung: false });
        self.log(game, actor, AiEvent::AmbushWaits { package, hidden: a.hide });
    }

    /// Lying in wait: the first enemy inside the trigger location sets it off.
    fn ambush_watch(&mut self, game: &mut Game, actor: FormId, package: FormId, a: &Ambush) {
        let feet = self.feet(actor);
        let base = |game: &Game, r: FormId| fnv_game::functions::base_of(game, r);
        let mut candidates: Vec<FormId> = match a.target {
            Wanted::Reference(r) => vec![r],
            _ => std::iter::once(FormId::PLAYER_REF).chain(self.agents.keys().copied()).filter(|r| *r != actor).collect(),
        };
        candidates.retain(|r| Self::alive(game, *r));
        let mut enemy = None;
        for c in candidates {
            let Some(at) = goal::place_of_ref(game, c) else { continue };
            if at.space != feet.space || !a.trigger.holds(game, feet, at) {
                continue;
            }
            let counts = match a.target {
                Wanted::Reference(_) => true,
                Wanted::Base(b) => base(game, c) == Some(b) || fnv_game::functions::faction_rank(game, c, b) >= 0,
                _ => game.would_attack(actor, c),
            };
            if counts {
                enemy = Some((c, hdist(at.pos, feet.pos)));
                break;
            }
        }
        let Some((enemy, away)) = enemy else { return };
        self.agents.get_mut(&actor).expect("listed").errand = Some(Errand::Ambush { lying: false, sprung: true });
        self.log(game, actor, AiEvent::AmbushSprung { package, enemy, away });
        game.start_combat(actor, enemy);
        game.package_goal_met(actor);
    }

    // ---- Flee Not Combat -------------------------------------------------------------

    /// What threatens a fleeing actor now: the package's target within the flee
    /// distance, else the nearest actor within it, either of them only when it is an
    /// enemy: one of the two would attack the other [observed: a Goodsprings settler given
    /// VTerrifyingPresencePackage (flee the player, 768) 200 units from the player, who
    /// was no enemy, stood for 0.4 game hours in procedure FLEE, packages.md §4;
    /// hypothesis: that an enemy makes it run].
    fn threat(&self, game: &mut Game, actor: FormId, f: &Flee) -> Option<(FormId, Place)> {
        let feet = self.feet(actor);
        let near = |game: &Game, r: FormId| goal::place_of_ref(game, r).filter(|p| p.space == feet.space && hdist(p.pos, feet.pos) <= f.distance && Self::alive(game, r));
        let enemy = |game: &mut Game, r: FormId| game.would_attack(r, actor) || game.would_attack(actor, r);
        if let Some(t) = f.from {
            return near(game, t).filter(|_| enemy(game, t)).map(|p| (t, p));
        }
        let mut best: Option<(f32, FormId, Place)> = None;
        for c in std::iter::once(FormId::PLAYER_REF).chain(self.agents.keys().copied()).filter(|r| *r != actor) {
            let Some(p) = near(game, c) else { continue };
            let d = hdist(p.pos, feet.pos);
            if best.as_ref().is_none_or(|b| d < b.0) && enemy(game, c) {
                best = Some((d, c, p));
            }
        }
        best.map(|b| (b.1, b.2))
    }

    fn flee_look(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, package: FormId, f: &Flee, was: Option<FormId>) {
        let threat = self.threat(game, actor, f);
        let travelling = self.agents[&actor].activity == Activity::Travelling;
        let next_look = game.time + FLEE_LOOK_SECONDS;
        let Some((from, at)) = threat else {
            // Nothing to flee: it stands, and the package has done what it is for
            // [observed: EasyPeteRef, whose Flee Not Combat package was current with no
            // threat about, stood in procedure DONE, packages.md §4].
            if !travelling {
                let first = was.is_some() || !game.extra.get(&actor).is_some_and(|e| e.package_goal_met);
                self.agents.get_mut(&actor).expect("listed").errand = Some(Errand::Flee { from: None, next_look });
                if first {
                    self.log(game, actor, AiEvent::FleeSafe { package, from: was, seconds: 0.0 });
                    game.package_goal_met(actor);
                }
            }
            return;
        };
        self.agents.get_mut(&actor).expect("listed").errand = Some(Errand::Flee { from: Some(from), next_look });
        if travelling && was == Some(from) {
            return;
        }
        let feet = self.feet(actor);
        // To the package's location, else straight away from the threat.
        let mut tries: Vec<Place> = Vec::new();
        if let Some(to) = f.to.as_deref() {
            match self.way_to(feet, to) {
                Some(p) => tries.push(p),
                // There already: it stays (its goal met), facing the threat.
                None => {
                    self.agents.get_mut(&actor).expect("listed").activity = Activity::There;
                    self.face(game, actor, at, FLEE_LOOK_SECONDS as f32);
                    if !game.extra.get(&actor).is_some_and(|e| e.package_goal_met) {
                        self.log(game, actor, AiEvent::FleeSafe { package, from: Some(from), seconds: 0.0 });
                        game.package_goal_met(actor);
                    }
                    return;
                }
            }
        } else {
            let (dx, dy) = (feet.pos[0] - at.pos[0], feet.pos[1] - at.pos[1]);
            let base = if dx.hypot(dy) > 1.0 { dy.atan2(dx) } else { 0.0 };
            let reach = f.distance + FLEE_MARGIN;
            for turn in [0.0f32, 30.0, -30.0, 60.0, -60.0, 90.0, -90.0] {
                let h = base + turn.to_radians();
                let p = [at.pos[0] + reach * h.cos(), at.pos[1] + reach * h.sin(), feet.pos[2]];
                if let Some((_, p)) = nav.nearest(feet.space, p, 512.0) {
                    if hdist(p, at.pos) > hdist(feet.pos, at.pos) {
                        tries.push(Place { space: feet.space, pos: p });
                    }
                }
            }
        }
        let params = self.path_params(game, actor);
        let speed = self.pace(game, actor, true);
        let a = self.agents.get_mut(&actor).expect("listed");
        a.walker.speed = speed;
        for to in tries {
            let Ok(length) = a.walker.go_to(nav, to, &params).map(|p| p.length) else { continue };
            a.activity = Activity::Travelling;
            a.trip_started = game.time;
            a.trip_from = a.walker.travelled;
            self.log(game, actor, AiEvent::FleeRuns { package, from, to: to.pos, length });
            return;
        }
        // Nowhere to run: it stands facing the threat.
        a.activity = Activity::There;
        a.walker.follower = None;
        self.face(game, actor, at, FLEE_LOOK_SECONDS as f32);
    }
}

/// The items `r` carries (its inventory set up first).
fn carried(game: &mut Game, r: FormId) -> Vec<FormId> {
    game.world.get(r).ok().and_then(|s| s.inventory.as_ref()).map(|inv| inv.iter().filter(|(_, n)| *n > 0).map(|(i, _)| i).collect()).unwrap_or_default()
}

/// The references (placed and created at run time) of the cells an area around `centre`
/// covers: the cell itself in an interior, the exterior cells within `radius` (at most
/// [`MAX_CELLS`] each way) outdoors.
pub(crate) fn cells_around(game: &Game, centre: Place, radius: f32) -> Vec<FormId> {
    let refs = game.world.placed;
    let mut cells = Vec::new();
    match centre.space {
        fnv_nav::Space::Interior(c) => cells.push(FormId(c)),
        fnv_nav::Space::World(w) => {
            let (cx, cy) = fnv_world::cell_of(centre.pos[0], centre.pos[1]);
            let k = ((radius / 4096.0).ceil() as i32).clamp(1, MAX_CELLS);
            if let Some(grid) = refs.world_grid.get(&FormId(w)) {
                for dx in -k..=k {
                    for dy in -k..=k {
                        cells.extend(grid.get(&(cx + dx, cy + dy)).copied());
                    }
                }
            }
        }
    }
    let mut out: Vec<FormId> = cells.iter().flat_map(|c| refs.by_cell.get(c).into_iter().flatten().copied()).collect();
    out.extend(game.world.created().filter(|(_, s)| s.cell.is_some_and(|c| cells.contains(&c)) && !s.deleted()).map(|(id, _)| id));
    out
}
