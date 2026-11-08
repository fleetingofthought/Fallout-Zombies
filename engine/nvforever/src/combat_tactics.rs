//! Combat tactics on top of the combat AI (docs/design/combat-ai.md §12): cover, flanks,
//! a clear line of fire, and the fight summary. `npc_combat` calls in here for each
//! fighter; the posts it chooses go to `fnv_game::combat_group::Tactics::posts`, which
//! `fnv_ai::alert` walks the fighter to.
//!
//! - **Cover** [documented: GECK wiki Combat Style, Cover]: every 10 seconds a fighter
//!   with a gun decides by its style's Take Cover Chance whether it uses cover for the
//!   next 10. Using it, it looks for cover within the style's Combat Search Radius: a
//!   navmesh cover edge (`fnv_nav::CoverEdge`) its target stands beyond, from whose back
//!   the collision world hides the target from its eyes, with a peek point beside the
//!   edge's end from which it sees the target [design: the original's cover search is
//!   not described]. There it hides for the style's Wait Timer, steps out to the peek
//!   point and looks for its target for the Wait To Fire Timer, fires for the Fire Timer
//!   once it sees it, and goes back behind the cover. Cover another fighter has taken is
//!   left to it. Cover whose edge no longer faces the target, or too far from it, is
//!   given up.
//! - **Cover from the shooter** [design: docs/design/combat-ai.md §12.6.1]: the cover is
//!   taken against an enemy gunman that is firing at the fighter (the nearest first),
//!   then against its own target: the edge faces that gunman (`Cover::from`) and hides
//!   the fighter from it, and the peek point still sees the fighter's own target, at
//!   which it fires. Once that gunman stops firing at it, the cover follows its target.
//! - **Flanks** [design, the owner's list 2: "have them work together in order to win"]:
//!   fighters with guns of one combat group fighting one target, not in cover, spread
//!   round it: each gets a place at the distance it fights from, turned from the side its
//!   group comes from by 0, +35°, −35°, +70° … in FormID order, so they come at it from
//!   several sides and do not stand in each other's way.
//! - **Line of fire** [design]: a fighter does not fire while a comrade (or the player,
//!   when the player is not its target) stands within [`LINE_OF_FIRE_WIDTH`] of the line
//!   to its target; it steps aside, away from them. Bystanders do not hold it back (the
//!   Fiends' brahmin stand among them).
//! - **A wall in the way** [design: owner's report 2026-10-23, NPCs shooting at walls]:
//!   a gun is fired only when the shot can reach. As the trigger is pulled, rays go from
//!   the shooter's eyes, and from its weapon's muzzle where it has one (a shot leaves the
//!   eye line and, with something between the muzzle and the target, the muzzle:
//!   `nvf_play::combat`), to the target's chest, else its head ([`shot_clear`]); the
//!   collision world's solid shapes block them, actors' capsules do not, as for sight
//!   (`detection::clear_sight`). The shot goes at the point that is clear. Detection
//!   is not asked: it says whom an actor knows about, and it can say "seen" while a
//!   wall stops the shot (between its rounds, over the landscape alone, the muzzle
//!   below the eyes). A clear line holds for [`LINE_CLEAR_FOR`] (an automatic's burst);
//!   a blocked one holds fire for [`WALL_RECHECK`] before the next look. Held, the
//!   fighter steps aside ([`SIDESTEP`]) to whichever side has a clear line from its
//!   eyes ([`wall_step`]), else closes in on its target as on one it does not see
//!   (`fnv_ai::alert`); at a cover's peek point it gives that cover up.
//! - **Fight summary** ([`Battle`]): the sides (comrades together, named by a faction),
//!   hits, kills, flights, cover, flanks, steps aside, alarms and searches, and how long
//!   the fight took: `battle over` when nobody has fought for [`RESOLVED_AFTER`] seconds,
//!   `battle summary` at exit.

use bevy::prelude::*;
use fnv_game::combat_group::{GroupNote, PostKind};
use fnv_game::game::Game;
use fnv_nav::{CoverEdge, NavGraph, Space};
use fnv_sim::FormId;
use nvf_play::combat::ActorHit;
use nvf_play::phys::PhysState;
use nvf_play::script::ScriptState;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Take Cover Chance is rolled this often, seconds [documented: "evaluated every 10
/// seconds to determine if cover will be used for the next 10 seconds"].
pub const COVER_EVERY: f32 = 10.0;
/// Cover is looked for within this radius when the style gives none, game units [design].
pub const COVER_SEARCH_DEFAULT: f32 = 1024.0;
/// Cover is never looked for farther than this from the fighter, whatever its style's
/// radius, game units [design: FalloutNV.esm's styles had troopers run 2,000 units to
/// cover in the middle of a fight].
pub const COVER_MAX_RUN: f32 = 1024.0;
/// A fighter without cover looks for it again after this long, seconds [design].
pub const COVER_RETRY: f32 = 2.0;
/// Behind a cover edge a fighter stands this far in from it, game units [design].
pub const COVER_STANDOFF: f32 = 40.0;
/// A peek point is this far past the end of the cover edge, game units [design].
pub const PEEK_STEP: f32 = 64.0;
/// Cover nearer its target than this is not taken, game units [design: not in the
/// target's arms' reach].
pub const COVER_MIN_TARGET: f32 = 400.0;
/// Two fighters' cover or posts at least this far apart [design].
pub const TAKEN_RADIUS: f32 = 96.0;
/// Cover edges looked at per search, nearest first [design: rays are not free].
pub const COVER_CANDIDATES: usize = 16;
/// A fighter counts as at its post within this distance, game units [design].
pub const AT_POST: f32 = fnv_game::combat_group::POST_ARRIVAL;
/// Flank places turn this much more for each fighter of the group, degrees [design].
pub const FLANK_STEP_DEG: f32 = 35.0;
/// Flank places are looked at again this often, seconds [design].
pub const FLANK_EVERY: f32 = 2.0;
/// A flank place nearer than this to where the fighter stands is not walked to [design].
pub const FLANK_MIN_MOVE: f32 = 192.0;
/// Someone within this distance of the line from a shooter to its target is in its line
/// of fire, game units [design: a body's width and a little].
pub const LINE_OF_FIRE_WIDTH: f32 = 40.0;
/// A step aside out of a friend's way, game units [design].
pub const SIDESTEP: f32 = 128.0;
/// A gunman whose target is nearer than this steps back from it, walking backwards while
/// it fires, game units [design: owner 2026-10-16, "backtracking in order to go
/// backwards"]. Not the weapon's Min Range, which "tells combat AI they don't need to be
/// closer than this value" [documented: GECK wiki Weapons]: the service rifle's 768 had
/// the NCR troopers backing away most of their fight.
pub const BACK_OFF_WITHIN: f32 = 256.0;
/// Once stepping back, it goes on until the target is this much farther than
/// [`BACK_OFF_WITHIN`], so it does not stop and start at the edge [design].
pub const BACK_OFF_SLACK: f32 = 64.0;
/// One step back, game units [design].
pub const BACK_OFF_STEP: f32 = 192.0;
/// A fight is over when nobody has fought for this long, seconds [design].
pub const RESOLVED_AFTER: f32 = 5.0;
/// A kind of post the AI found no way to is not given again for this long, seconds
/// [design].
pub const NO_POST_SECONDS: f32 = 3.0;
/// A clear line of fire found as the trigger is pulled holds this long for the shots
/// that follow, seconds [design: rays are not free; an automatic fires every frame].
pub const LINE_CLEAR_FOR: f32 = 0.25;
/// A shot held for a wall is looked at again after this long, seconds [design].
pub const WALL_RECHECK: f32 = 0.5;

/// A wall stops a fighter's shot at its target (module notes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wall {
    pub target: u32,
    /// Since when (clock); the next look (clock).
    pub since: f32,
    pub recheck_at: f32,
    /// Where it steps aside to, a clear line from there; `None`: it closes in.
    pub step: Option<[f32; 3]>,
}

/// Whether a shot from `eyes` (and from `muzzle`, where the weapon has one) reaches the
/// target's `chest`, else its `head`: the point it can go at (module notes).
pub fn shot_clear(mut phys: Option<&mut PhysState>, eyes: [f32; 3], muzzle: Option<[f32; 3]>, chest: [f32; 3], head: [f32; 3]) -> Option<[f32; 3]> {
    [chest, head].into_iter().find(|to| crate::detection::clear_sight(phys.as_deref_mut(), eyes, *to) && muzzle.is_none_or(|m| crate::detection::clear_sight(phys.as_deref_mut(), m, *to)))
}

/// The two steps of `step` across the line from `feet` to `target`: `first` side first.
pub fn side_steps(feet: [f32; 3], target: [f32; 3], step: f32, first: bool) -> [[f32; 3]; 2] {
    let d = [target[0] - feet[0], target[1] - feet[1]];
    let len = d[0].hypot(d[1]).max(1e-3);
    let n = [-d[1] / len, d[0] / len];
    let s = if first { 1.0 } else { -1.0 };
    [[feet[0] + n[0] * step * s, feet[1] + n[1] * step * s, feet[2]], [feet[0] - n[0] * step * s, feet[1] - n[1] * step * s, feet[2]]]
}

/// Where a fighter whose shot a wall stops steps aside to: the first of its
/// [`side_steps`] from which its eyes (`eye` above the feet) see the target's `chest`.
pub fn wall_step(mut phys: Option<&mut PhysState>, feet: [f32; 3], eye: f32, target: [f32; 3], chest: [f32; 3], first: bool) -> Option<[f32; 3]> {
    side_steps(feet, target, SIDESTEP, first).into_iter().find(|p| crate::detection::clear_sight(phys.as_deref_mut(), [p[0], p[1], p[2] + eye], chest))
}

/// Where a fighter is in its use of cover.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    /// On its way behind the cover.
    Going,
    /// Behind it, out of sight, until `until` (seconds of [`Tactic`] time).
    Hiding { until: f32 },
    /// Out at the peek point looking for its target, until `until`.
    WaitToFire { until: f32 },
    /// Out at the peek point firing, until `until`.
    Firing { until: f32 },
}

/// Cover a fighter has chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cover {
    pub behind: [f32; 3],
    pub peek: [f32; 3],
    pub edge: CoverEdge,
    pub phase: Phase,
    /// Low cover: it hides crouched behind it and stands to fire, the peek point where it
    /// hides (docs/design/combat-ai.md §14.2).
    pub low: bool,
    /// The actor it hides from: a gunman firing at it, or its own target (§12.6.1).
    pub from: u32,
    /// The room round the behind and peek spots when chosen ([`room`]; infinite without
    /// collision).
    pub room: [f32; 2],
}

/// One the cover can be taken against: who, and where its chest is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Threat {
    pub actor: u32,
    pub chest: [f32; 3],
}

/// Whom a fighter takes cover against, in order (§12.6.1): the enemy gunmen firing at
/// it, nearest first, then its own target. `shooters`: (actor, chest) of those firing at
/// it; `feet` where it stands.
pub fn threats(shooters: &[(u32, [f32; 3])], target: Threat, feet: [f32; 3]) -> Vec<Threat> {
    let mut v: Vec<Threat> = shooters.iter().filter(|(a, _)| *a != target.actor).map(|(actor, chest)| Threat { actor: *actor, chest: *chest }).collect();
    v.sort_by(|a, b| flat(a.chest, feet).total_cmp(&flat(b.chest, feet)).then(a.actor.cmp(&b.actor)));
    v.push(target);
    v
}

/// One fighter's tactics.
#[derive(Debug, Clone, Default)]
pub struct Tactic {
    /// Uses cover for the current 10 seconds; seconds to the next roll; to the next
    /// search for cover.
    pub use_cover: bool,
    pub cover_roll_in: f32,
    pub cover_search_in: f32,
    pub cover: Option<Cover>,
    /// Seconds to the next look at its flank place.
    pub flank_in: f32,
    /// Who stands in its line of fire now.
    pub blocked_by: Option<u32>,
    /// Its health when it reached its flee spot.
    pub fled_health: Option<f64>,
    /// The kind of post it was given last frame; a kind the AI found no way to, barred
    /// until then; behind-cover places it could not reach.
    pub last_post: Option<PostKind>,
    pub no_post: Option<(fnv_game::combat_group::PostKind, f32)>,
    pub bad_cover: Vec<[f32; 3]>,
    /// A shift sideways it is on its way to (docs/design/combat-ai.md §14.2).
    pub shift: Option<[f32; 3]>,
    /// A clear line of fire to this target until then (clock), and the point it is clear
    /// to ([`LINE_CLEAR_FOR`]); a wall that stops its shot ([`Wall`]).
    pub shot_line: Option<(u32, f32, [f32; 3])>,
    pub wall: Option<Wall>,
}

impl Tactic {
    /// The post it was given last frame is gone: the AI dropped it (no way there, or its
    /// walk there ended away from it, docs/design/combat-ai.md §12.1). Not that kind again
    /// for a while; cover it could not reach is not looked at again.
    pub fn post_dropped(&mut self, clock: f32) {
        let Some(kind) = self.last_post else { return };
        self.no_post = Some((kind, clock + NO_POST_SECONDS));
        if matches!(kind, PostKind::Cover | PostKind::Duck | PostKind::Peek) {
            if let Some(cv) = self.cover.take() {
                self.bad_cover.push(cv.behind);
            }
            self.cover_search_in = COVER_RETRY;
        }
    }
}

/// A name for logs: the reference's name (its Editor ID or FULL), else its base form's
/// Editor ID, with the FormID.
pub fn who(g: &Game, id: FormId) -> String {
    if id == FormId::PLAYER_REF {
        return "the player".into();
    }
    let name = g.data.name(id);
    if name.starts_with("0x") {
        let base = g.world.placed.get(id).and_then(|p| g.data.edid_of.get(&p.base).cloned()).unwrap_or_default();
        format!("{base} {id}").trim().to_string()
    } else {
        format!("{name} {id}")
    }
}

/// The flank turn of the `index`-th fighter of a group, degrees: 0, +35, −35, +70, −70 …
pub fn flank_turn(index: usize) -> f32 {
    if index == 0 {
        return 0.0;
    }
    let k = index.div_ceil(2) as f32;
    if index % 2 == 1 {
        k * FLANK_STEP_DEG
    } else {
        -k * FLANK_STEP_DEG
    }
}

/// The flank place `radius` from `target`, turned `turn_deg` from the side `from` is on.
pub fn flank_point(target: [f32; 3], from: [f32; 3], radius: f32, turn_deg: f32) -> [f32; 3] {
    let base = (from[1] - target[1]).atan2(from[0] - target[0]);
    let a = base + turn_deg.to_radians();
    [target[0] + radius * a.cos(), target[1] + radius * a.sin(), target[2]]
}

/// Whether `other` stands in the line of fire from `from` to `to` (within `width` of the
/// segment, between the two ends with a body's room at each, horizontally; and within
/// 96 units of the line's height there).
pub fn in_line_of_fire(from: [f32; 3], to: [f32; 3], other: [f32; 3], width: f32) -> bool {
    let d = [to[0] - from[0], to[1] - from[1]];
    let len = d[0].hypot(d[1]);
    if len < 1.0 {
        return false;
    }
    let u = [d[0] / len, d[1] / len];
    let rel = [other[0] - from[0], other[1] - from[1]];
    let along = rel[0] * u[0] + rel[1] * u[1];
    if along <= width || along >= len - width {
        return false;
    }
    let across = (rel[0] * u[1] - rel[1] * u[0]).abs();
    let z = from[2] + (to[2] - from[2]) * along / len;
    across < width && (other[2] - z).abs() < 96.0
}

/// A step `step` across the line of fire from `from` to `to`, away from `other`.
pub fn sidestep(from: [f32; 3], to: [f32; 3], other: [f32; 3], step: f32) -> [f32; 3] {
    let d = [to[0] - from[0], to[1] - from[1]];
    let len = d[0].hypot(d[1]).max(1e-3);
    let mut n = [-d[1] / len, d[0] / len];
    if (other[0] - from[0]) * n[0] + (other[1] - from[1]) * n[1] > 0.0 {
        n = [-n[0], -n[1]];
    }
    [from[0] + n[0] * step, from[1] + n[1] * step, from[2]]
}

/// A step `step` straight back from `target`, from `from` (on top of `target`: towards
/// -y).
pub fn back_off(from: [f32; 3], target: [f32; 3], step: f32) -> [f32; 3] {
    let d = [from[0] - target[0], from[1] - target[1]];
    let len = d[0].hypot(d[1]);
    let u = if len < 1e-3 { [0.0, -1.0] } else { [d[0] / len, d[1] / len] };
    [from[0] + u[0] * step, from[1] + u[1] * step, from[2]]
}

/// Where a fighter using `cv` goes and as what post. The run to its cover is a Cover
/// post: it turns the way it goes. Once there, behind it is a Duck post and out at the
/// peek point a Peek post: it stays facing its target, so it strafes out to fire and
/// back in (`residents`: only Cover posts turn away) [design: owner 2026-10-18, "a real
/// person would side strafe in and out during shooting"].
pub fn cover_post(cv: &Cover) -> ([f32; 3], PostKind) {
    match cv.phase {
        Phase::Going => (cv.behind, PostKind::Cover),
        Phase::Hiding { .. } => (cv.behind, PostKind::Duck),
        Phase::WaitToFire { .. } | Phase::Firing { .. } => (cv.peek, PostKind::Peek),
    }
}

/// The two peek points of a cover edge: past each end of it, [`COVER_STANDOFF`] in from
/// its line.
pub fn peek_points(e: &CoverEdge) -> [[f32; 3]; 2] {
    let d = [e.b[0] - e.a[0], e.b[1] - e.a[1]];
    let len = d[0].hypot(d[1]).max(1e-3);
    let u = [d[0] / len, d[1] / len];
    let half = len / 2.0 + PEEK_STEP;
    let back = e.behind(COVER_STANDOFF);
    [[back[0] + u[0] * half, back[1] + u[1] * half, back[2]], [back[0] - u[0] * half, back[1] - u[1] * half, back[2]]]
}

/// A cover or peek spot keeps at least this much room round its middle, horizontally,
/// game units: the actor's capsule (`fnv_phys::CharacterParams::default().radius`, 22)
/// and a little [design]. Navmesh cover edges are not drawn the capsule's width off the
/// wall: some run along it or under what stands out of it (a drainpipe, a porch post), so
/// a spot [`COVER_STANDOFF`] in from the edge can be inside the wall [observed: owner's
/// screenshot 2026-10-23, an NCR trooper in Goodsprings half inside a house's wall and
/// drainpipe].
pub const SPOT_ROOM: f32 = 26.0;
/// An actor standing with less room than this round its feet has its capsule inside
/// something: the capsule's radius (`fnv_phys::CharacterParams::default().radius`).
pub const BODY_RADIUS: f32 = 22.0;
/// The room is measured this far out at most, game units [design].
pub const ROOM_REACH: f32 = 48.0;
/// Heights above the feet the room is measured at, game units: above a step (the
/// character's step height is 24) so kerbs and stairs it walks up are not walls, up to
/// the head [design].
const ROOM_HEIGHTS: [f32; 3] = [32.0, 64.0, 100.0];
/// Directions the room is measured in [design].
const ROOM_RAYS: usize = 16;
/// How much farther in from its edge a spot without room is tried again, and how many
/// times, game units [design].
const ROOM_BACK_STEP: f32 = 16.0;
const ROOM_BACK_TRIES: usize = 4;

/// The room round a spot (feet at `at`): the nearest collision within [`ROOM_REACH`],
/// horizontally, at the heights the body stands in, and the push straight away from it
/// that would give it [`SPOT_ROOM`] (if nothing else is near). Actors and the terrain are
/// not counted (a hill beside it is not a wall; the character stands on the ground). A
/// spot inside a solid shape has no room (a ray from inside a convex shape hits at once).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Room {
    pub nearest: f32,
    pub push: [f32; 2],
}

pub fn room(phys: &mut PhysState, at: [f32; 3]) -> Room {
    let mut filter = fnv_phys::Filter { exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()), ..Default::default() };
    filter.exclude.extend(phys.npcs.values().map(|n| n.ch.key()));
    let mut nearest = ROOM_REACH;
    let mut way = [0.0f32; 2];
    for h in ROOM_HEIGHTS {
        for i in 0..ROOM_RAYS {
            let a = i as f32 * std::f32::consts::TAU / ROOM_RAYS as f32;
            let dir = [a.cos(), a.sin(), 0.0];
            let Some(hit) = phys.ray([at[0], at[1], at[2] + h], dir, ROOM_REACH, filter.clone()) else { continue };
            if matches!(hit.owner, fnv_phys::Owner::Terrain { .. } | fnv_phys::Owner::Character { .. } | fnv_phys::Owner::Actor { .. }) {
                continue;
            }
            if hit.distance < nearest {
                nearest = hit.distance;
                way = [dir[0], dir[1]];
            }
        }
    }
    // Straight away from the nearest hit; what else is near is left to the next look.
    let short = (SPOT_ROOM - nearest).max(0.0);
    Room { nearest, push: [-way[0] * short, -way[1] * short] }
}

/// A spot near `want` with [`SPOT_ROOM`] round it, on the navmesh: `want` on the navmesh,
/// pushed out of what is too near once, and failing that, the same again up to
/// [`ROOM_BACK_TRIES`] steps farther along `back` (the way in from the cover edge, away
/// from what stands there). With its room; `None` when none has room. Without collision
/// (`phys` none) `want` on the navmesh, its room not known (infinite).
pub fn spot_with_room(nav: &NavGraph, phys: Option<&mut PhysState>, space: Space, want: [f32; 3], back: [f32; 2]) -> Option<([f32; 3], f32)> {
    let Some(phys) = phys else { return nav.nearest(space, want, 96.0).map(|(_, p)| (p, f32::INFINITY)) };
    let found = find_room(nav, phys, space, want, back);
    let i = match &found {
        None => 2,
        Some((p, _)) if nav.nearest(space, want, 96.0).is_none_or(|(_, w)| flat(*p, w) > 1.0) => 1,
        Some(_) => 0,
    };
    SPOT_TALLY[i].fetch_add(1, Ordering::Relaxed);
    found
}

/// Spots looked at by [`spot_with_room`] with collision: with room where wanted (on the
/// navmesh), with room once moved, and without room (not used), for the battle summary.
static SPOT_TALLY: [AtomicUsize; 3] = [AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0)];

/// Cover searches by [`find_cover`], for the battle summary: searches, edges looked at,
/// of those too near or too far from the target, taken by another, the one hidden from
/// seen from behind it, and no peek point seeing the target.
/// Of those out of range: too near the target, beyond the fighter's reach, too near the
/// one hidden from.
static SEARCH_TALLY: [AtomicUsize; 9] = [const { AtomicUsize::new(0) }; 9];

fn find_room(nav: &NavGraph, phys: &mut PhysState, space: Space, want: [f32; 3], back: [f32; 2]) -> Option<([f32; 3], f32)> {
    for k in 0..=ROOM_BACK_TRIES {
        let s = k as f32 * ROOM_BACK_STEP;
        let Some((_, mut p)) = nav.nearest(space, [want[0] + back[0] * s, want[1] + back[1] * s, want[2]], 96.0) else { continue };
        for _ in 0..2 {
            let r = room(phys, p);
            if r.nearest >= SPOT_ROOM {
                return Some((p, r.nearest));
            }
            if r.push == [0.0, 0.0] {
                break;
            }
            match nav.nearest(space, [p[0] + r.push[0], p[1] + r.push[1], p[2]], 16.0) {
                Some((_, q)) => p = q,
                None => break,
            }
        }
    }
    None
}

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Look for cover for a fighter at `feet` against `from` (a gunman firing at it, or its
/// target) while it fights a target whose chest is at `chest` (module notes): `eye` its
/// eye height, `radius` how far to look, `max_range` how far from the target it may
/// fight, `taken` places others hold. The edge faces `from` and its back hides the
/// fighter from it; the peek point sees the target. `crouch`: it can crouch, so cover
/// that hides only its crouched eyes is low cover it hides behind crouched and stands up
/// from to fire (docs/design/combat-ai.md §14.2).
#[allow(clippy::too_many_arguments)]
pub fn find_cover(nav: &NavGraph, mut phys: Option<&mut PhysState>, space: Space, feet: [f32; 3], from: Threat, chest: [f32; 3], eye: f32, radius: f32, max_range: f32, taken: &[[f32; 3]], crouch: bool) -> Option<Cover> {
    let hidden = from.chest;
    let sees_from = |phys: &mut Option<&mut PhysState>, at: [f32; 3], eye: f32, to: [f32; 3]| crate::detection::clear_sight(phys.as_deref_mut(), [at[0], at[1], at[2] + eye], to);
    let sees = |phys: &mut Option<&mut PhysState>, at: [f32; 3], to: [f32; 3]| sees_from(phys, at, eye, to);
    let mut low: Option<Cover> = None;
    let tally = |i: usize| SEARCH_TALLY[i].fetch_add(1, Ordering::Relaxed);
    tally(0);
    // Edges out of range are dropped before the nearest COVER_CANDIDATES are taken: the
    // cap limits the sight rays, and nearest-first edges behind a fighter still closing in
    // would otherwise fill it with cover it may not fight from (#1162).
    let in_range = |e: &CoverEdge| {
        tally(1);
        let d = flat(e.mid, chest);
        if (COVER_MIN_TARGET..=max_range).contains(&d) && flat(e.mid, hidden) >= COVER_MIN_TARGET {
            return true;
        }
        tally(2);
        tally(if d < COVER_MIN_TARGET { 6 } else if d > max_range { 7 } else { 8 });
        false
    };
    for e in nav.cover_edges_near(space, feet, radius).into_iter().filter(|e| e.faces(hidden)).filter(in_range).take(COVER_CANDIDATES) {
        let inward = [-e.out[0], -e.out[1]];
        let Some((behind, behind_room)) = spot_with_room(nav, phys.as_deref_mut(), space, e.behind(COVER_STANDOFF), inward) else { continue };
        if taken.iter().any(|t| flat(*t, behind) < TAKEN_RADIUS) {
            tally(3);
            continue;
        }
        // Covered: the one it hides from cannot be seen from its eyes there. Seen
        // standing but not crouched: low cover, kept in case no full cover is found; it
        // stands up there to fire, so it must see its target standing.
        if sees(&mut phys, behind, hidden) {
            if crouch && low.is_none() && !sees_from(&mut phys, behind, eye * crate::combat_stance::CROUCH_EYE, hidden) && (from.chest == chest || sees(&mut phys, behind, chest)) {
                low = Some(Cover { behind, peek: behind, edge: e, phase: Phase::Going, low: true, from: from.actor, room: [behind_room; 2] });
            }
            tally(4);
            continue;
        }
        let mut peek = None;
        for p in peek_points(&e) {
            if let Some((p, r)) = spot_with_room(nav, phys.as_deref_mut(), space, p, inward) {
                if sees(&mut phys, p, chest) {
                    peek = Some((p, r));
                    break;
                }
            }
        }
        if peek.is_none() {
            tally(5);
        }
        if let Some((peek, peek_room)) = peek {
            return Some(Cover { behind, peek, edge: e, phase: Phase::Going, low: false, from: from.actor, room: [behind_room, peek_room] });
        }
    }
    low
}

/// A bound for a melee fighter at `feet` closing on a gunman whose eyes are at `eyes`
/// (docs/design/combat-ai.md §14.4): the back point of a cover edge that gains ground
/// within the lane (`combat_stance::bound_gain`), hidden from the gunman's eyes at
/// `chest` height above it, not taken. Of those, one of the two that gain the most, by
/// `pick` (0 or 1). Returns the point and its gain.
#[allow(clippy::too_many_arguments)]
pub fn find_bound(nav: &NavGraph, mut phys: Option<&mut PhysState>, space: Space, feet: [f32; 3], target: [f32; 3], eyes: [f32; 3], chest: f32, taken: &[[f32; 3]], pick: usize) -> Option<([f32; 3], f32)> {
    use crate::combat_stance::{bound_gain, BOUND_RADIUS};
    let mut found: Vec<([f32; 3], f32)> = Vec::new();
    for e in nav.cover_edges_near(space, feet, BOUND_RADIUS).into_iter().filter(|e| e.faces(eyes)).take(COVER_CANDIDATES * 2) {
        let Some((behind, _)) = spot_with_room(nav, phys.as_deref_mut(), space, e.behind(COVER_STANDOFF), [-e.out[0], -e.out[1]]) else { continue };
        let Some(gain) = bound_gain(feet, target, behind) else { continue };
        if taken.iter().any(|t| flat(*t, behind) < TAKEN_RADIUS) {
            continue;
        }
        if crate::detection::clear_sight(phys.as_deref_mut(), eyes, [behind[0], behind[1], behind[2] + chest]) {
            continue;
        }
        found.push((behind, gain));
    }
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    found.get(pick.min(found.len().saturating_sub(1))).copied()
}

/// Counts for one side of the fight.
#[derive(Debug, Clone, Default)]
pub struct Side {
    pub label: String,
    pub members: BTreeSet<u32>,
    pub dead: BTreeSet<u32>,
    pub hits: usize,
    pub kills: usize,
    pub flights: usize,
}

/// The fight summary (module notes).
#[derive(Resource, Debug, Default)]
pub struct Battle {
    pub sides: Vec<Side>,
    /// Elapsed seconds of the first fighter, of the last frame anyone fought, and when the
    /// fight was over (logged once).
    pub first: Option<f32>,
    pub last: f32,
    pub over: Option<f32>,
    pub flights: [usize; 3],
    pub cornered: usize,
    pub back_to_fight: usize,
    pub boosts: usize,
    pub cover_taken: usize,
    /// Of those, cover from a gunman firing at the fighter rather than from its target.
    pub cover_from_shooter: usize,
    pub peeks: usize,
    pub flanks: usize,
    pub sidesteps: usize,
    pub back_offs: usize,
    pub held_fire: usize,
    /// Shots held for a wall in the way (times a fighter began holding), steps aside
    /// for it, and times it found its line clear again.
    pub wall_holds: usize,
    pub wall_steps: usize,
    pub wall_clears: usize,
    pub alarms: usize,
    pub no_alarms: usize,
    pub searches: usize,
    pub npc_grudges: usize,
    grudges: BTreeSet<(u32, u32)>,
    /// Flights counted by how they went: (actor, when the flight began).
    pub counted_flights: BTreeSet<(u32, u64)>,
}

impl Battle {
    /// The side `actor` is on, putting it on one: with its comrades, else a new side
    /// named by its first faction (or its base).
    pub fn side_of(&mut self, g: &Game, actor: u32) -> usize {
        if let Some(i) = self.sides.iter().position(|s| s.members.contains(&actor)) {
            return i;
        }
        let id = FormId(actor);
        let i = match self.sides.iter().position(|s| s.members.iter().any(|m| g.comrades(FormId(*m), id))) {
            Some(i) => i,
            None => {
                self.sides.push(Side { label: g.side_name(id), ..Default::default() });
                self.sides.len() - 1
            }
        };
        self.sides[i].members.insert(actor);
        i
    }

    /// Someone fought this frame, at `now` (elapsed seconds).
    pub fn fought(&mut self, now: f32) {
        self.first.get_or_insert(now);
        self.last = now;
        self.over = None;
    }

    fn line(&self) -> String {
        self.sides
            .iter()
            .map(|s| {
                let standing = s.members.len() - s.dead.len();
                format!("{} ({}): {standing} standing, {} dead, {} hits, {} kills, {} flights", s.label, s.members.len(), s.dead.len(), s.hits, s.kills, s.flights)
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

pub struct CombatTacticsPlugin;

impl Plugin for CombatTacticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Battle>().add_systems(Update, (hits, notes, resolved).after(nvf_play::PlaySet::Scripts)).add_systems(Last, report);
    }
}

/// Hits and kills by side.
fn hits(mut read: MessageReader<ActorHit>, mut battle: ResMut<Battle>, script: Option<Res<ScriptState>>) {
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else {
        read.clear();
        return;
    };
    for h in read.read() {
        let Some(by) = h.by else { continue };
        if battle.first.is_none() {
            continue;
        }
        let a = battle.side_of(&rt.game, by.0);
        battle.sides[a].hits += 1;
        if h.died {
            battle.sides[a].kills += 1;
            let v = battle.side_of(&rt.game, h.form_id);
            battle.sides[v].dead.insert(h.form_id);
            info!("battle: {} killed by {}", who(&rt.game, FormId(h.form_id)), who(&rt.game, by));
        }
    }
}

/// The combat groups' notes (alarms, searches) and NPC-against-NPC grudges, logged.
fn notes(mut script: Option<ResMut<ScriptState>>, mut battle: ResMut<Battle>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let notes = rt.game.awareness.tactics.take_notes();
    let g = &rt.game;
    let name = |id: FormId| who(g, id);
    // Fights between actors started by a hit (`Game::attacked_by_actor`).
    for ((a, b), gr) in &g.provocations.grudges {
        if *b != FormId::PLAYER_REF && gr.why == fnv_game::provocation::Why::Assault && battle.grudges.insert((a.0, b.0)) {
            battle.npc_grudges += 1;
        }
    }
    for n in notes {
        match n {
            GroupNote::Alarm { sender, receiver, about, combat } => {
                battle.alarms += 1;
                info!("combat group: {} alarms {} about {} ({})", name(sender), name(receiver), name(about), if combat { "started fighting" } else { "noticed it" });
            }
            GroupNote::NoAlarm { sender, about, ratio, alarm_confidence } => {
                battle.no_alarms += 1;
                info!("combat group: {} sends no alarm about {}: group threat ratio {ratio:.2} at or above its alarm confidence {alarm_confidence:.2}", name(sender), name(about));
            }
            GroupNote::SpecialCombat { sender, about, faction } => {
                battle.no_alarms += 1;
                info!("combat group: {} sends no alarm about {}: both in {}, a Special Combat faction", name(sender), name(about), name(faction));
            }
            GroupNote::Searched { actor, about, count, needed } => {
                battle.searches += 1;
                info!("combat group: {} has searched its share for {} ({count} of {needed} complete searches)", name(actor), name(about));
            }
        }
    }
}

/// `battle over` once nobody has fought for [`RESOLVED_AFTER`] seconds.
fn resolved(time: Res<Time>, mut battle: ResMut<Battle>) {
    let now = time.elapsed_secs();
    if let (Some(first), None) = (battle.first, battle.over) {
        if now - battle.last >= RESOLVED_AFTER {
            battle.over = Some(battle.last);
            info!("battle over after {:.1} s (first fighter at {first:.1} s, the last fought at {:.1} s): {}", battle.last - first, battle.last, battle.line());
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, battle: Res<Battle>) {
    if exit.read().next().is_none() {
        return;
    }
    let b = &*battle;
    let Some(first) = b.first else {
        info!("battle summary: no fight");
        return;
    };
    let actors: usize = b.sides.iter().map(|s| s.members.len()).sum();
    let resolved = match b.over {
        Some(last) => format!("resolved in {:.1} s", last - first),
        None => "still going at exit".to_string(),
    };
    info!(
        "battle summary: {} sides, {actors} actors; {}; {} flights ({} by a door, {} to cover, {} away; {} cornered), {} confidence boosts, {} back to the fight; {} cover taken ({} from a gunman firing at it), {} peeks out of it; {} flank posts, {} steps aside, {} steps back, {} shots held for someone in the line of fire; {} held for a wall ({} steps aside, {} clear again); {} alarms, {} not sent; {} complete searches; {} fights started by a hit between actors; first fighter at {first:.1} s, {resolved}; cover spots: {} with room, {} moved to have room, {} without room; cover searches: {} ({} edges: {} out of range ({} near the target, {} beyond reach, {} near the one hidden from), {} taken, {} seen from behind, {} without a peek)",
        b.sides.len(),
        b.line(),
        b.flights.iter().sum::<usize>(),
        b.flights[0],
        b.flights[1],
        b.flights[2],
        b.cornered,
        b.boosts,
        b.back_to_fight,
        b.cover_taken,
        b.cover_from_shooter,
        b.peeks,
        b.flanks,
        b.sidesteps,
        b.back_offs,
        b.held_fire,
        b.wall_holds,
        b.wall_steps,
        b.wall_clears,
        b.alarms,
        b.no_alarms,
        b.searches,
        b.npc_grudges,
        SPOT_TALLY[0].load(Ordering::Relaxed),
        SPOT_TALLY[1].load(Ordering::Relaxed),
        SPOT_TALLY[2].load(Ordering::Relaxed),
        SEARCH_TALLY[0].load(Ordering::Relaxed),
        SEARCH_TALLY[1].load(Ordering::Relaxed),
        SEARCH_TALLY[2].load(Ordering::Relaxed),
        SEARCH_TALLY[6].load(Ordering::Relaxed),
        SEARCH_TALLY[7].load(Ordering::Relaxed),
        SEARCH_TALLY[8].load(Ordering::Relaxed),
        SEARCH_TALLY[3].load(Ordering::Relaxed),
        SEARCH_TALLY[4].load(Ordering::Relaxed),
        SEARCH_TALLY[5].load(Ordering::Relaxed),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A collision world with a static box (a wall) of half extents `half` centred at `at`.
    fn wall(at: [f32; 3], half: [f32; 3]) -> PhysState {
        use fnv_collide::{CollisionObject, Iso, Motion, Shape};
        let mut phys = PhysState::default();
        let o = CollisionObject {
            node: None,
            node_block: 0,
            node_transform: Iso::IDENTITY,
            body_transform: Iso::IDENTITY,
            motion: Motion::Static,
            layer: fnv_phys::layers::STATIC,
            mass: 0.0,
            friction: 0.5,
            restitution: 0.0,
            shape: Shape::Box { half_extents: half }.into(),
            shell: 0.0,
            material: None,
            wind: false,
        };
        phys.world.add_ref(&fnv_phys::RefPose { form_id: 0x10, position: at, rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], scale: 1.0 }, &[o]);
        phys
    }

    fn cover_at(behind: [f32; 3]) -> Cover {
        let edge = CoverEdge { tri: 0, a: behind, b: behind, mid: behind, out: [1.0, 0.0] };
        Cover { behind, peek: behind, edge, phase: Phase::Going, low: false, from: 1, room: [0.0, 0.0] }
    }

    #[test]
    fn a_dropped_post_bars_its_kind_for_a_while() {
        // Nothing given last frame: nothing was dropped.
        let mut t = Tactic::default();
        t.post_dropped(5.0);
        assert_eq!(t.no_post, None);
        // A flank: barred for NO_POST_SECONDS; its cover, if any, is not its concern.
        let mut t = Tactic { last_post: Some(PostKind::Flank), cover: Some(cover_at([1.0, 2.0, 3.0])), ..Default::default() };
        t.post_dropped(5.0);
        assert_eq!(t.no_post, Some((PostKind::Flank, 5.0 + NO_POST_SECONDS)));
        assert!(t.cover.is_some() && t.bad_cover.is_empty());
    }

    #[test]
    fn a_dropped_cover_post_gives_up_that_cover() {
        for kind in [PostKind::Cover, PostKind::Duck, PostKind::Peek] {
            let mut t = Tactic { last_post: Some(kind), cover: Some(cover_at([1.0, 2.0, 3.0])), ..Default::default() };
            t.post_dropped(5.0);
            assert_eq!(t.no_post, Some((kind, 5.0 + NO_POST_SECONDS)));
            assert_eq!(t.cover, None);
            assert_eq!(t.bad_cover, vec![[1.0, 2.0, 3.0]]);
            assert_eq!(t.cover_search_in, COVER_RETRY);
        }
    }

    #[test]
    fn room_is_the_way_to_the_nearest_wall_and_out_of_it() {
        // A wall whose face is 10 units east of the spot, from the ground to 200 up.
        let mut phys = wall([60.0, 0.0, 100.0], [50.0, 200.0, 100.0]);
        let r = room(&mut phys, [0.0, 0.0, 0.0]);
        assert!((r.nearest - 10.0).abs() < 1.0, "{r:?}");
        assert!(r.push[0] < -(SPOT_ROOM - 11.0) && r.push[1].abs() < 1.0, "pushed west, away from the wall: {r:?}");
        // Pushed that way, it has room.
        let at = [r.push[0], r.push[1], 0.0];
        assert!(room(&mut phys, at).nearest >= SPOT_ROOM - 1.0, "{:?}", room(&mut phys, at));
        // Far enough from it, the room is all it measures.
        assert_eq!(room(&mut phys, [-100.0, 0.0, 0.0]).nearest, ROOM_REACH);
    }

    #[test]
    fn a_spot_inside_a_wall_has_no_room() {
        let mut phys = wall([20.0, 0.0, 100.0], [50.0, 200.0, 100.0]);
        assert!(room(&mut phys, [0.0, 0.0, 0.0]).nearest < 1.0);
    }

    #[test]
    fn a_wall_below_the_knee_is_not_counted() {
        // A kerb 20 units high is stepped onto, not a wall.
        let mut phys = wall([30.0, 0.0, 10.0], [20.0, 200.0, 10.0]);
        assert_eq!(room(&mut phys, [0.0, 0.0, 0.0]).nearest, ROOM_REACH);
    }

    #[test]
    fn flank_places_spread_round_the_target() {
        assert_eq!([0, 1, 2, 3, 4].map(flank_turn), [0.0, 35.0, -35.0, 70.0, -70.0]);
        // The group comes from the east (+x): the first stays east, the second turns 35°.
        let t = [0.0, 0.0, 0.0];
        let p = flank_point(t, [1000.0, 0.0, 0.0], 500.0, 0.0);
        assert!((p[0] - 500.0).abs() < 1e-3 && p[1].abs() < 1e-3);
        let q = flank_point(t, [1000.0, 0.0, 0.0], 500.0, flank_turn(1));
        assert!((q[1] - 500.0 * 35f32.to_radians().sin()).abs() < 1e-2);
    }

    #[test]
    fn a_friend_between_the_shooter_and_its_target_blocks_the_shot() {
        let (from, to) = ([0.0, 0.0, 100.0], [1000.0, 0.0, 100.0]);
        assert!(in_line_of_fire(from, to, [500.0, 20.0, 90.0], LINE_OF_FIRE_WIDTH));
        assert!(!in_line_of_fire(from, to, [500.0, 60.0, 90.0], LINE_OF_FIRE_WIDTH), "beside the line");
        assert!(!in_line_of_fire(from, to, [1100.0, 0.0, 90.0], LINE_OF_FIRE_WIDTH), "beyond the target");
        assert!(!in_line_of_fire(from, to, [-50.0, 0.0, 90.0], LINE_OF_FIRE_WIDTH), "behind the shooter");
        assert!(!in_line_of_fire(from, to, [500.0, 0.0, 400.0], LINE_OF_FIRE_WIDTH), "on a roof above");
        // It steps away from the friend: the friend is north (+y), the step south.
        let s = sidestep(from, to, [500.0, 20.0, 90.0], SIDESTEP);
        assert!((s[1] + SIDESTEP).abs() < 1e-3 && s[0].abs() < 1e-3);
    }

    #[test]
    fn peek_points_lie_past_the_ends_of_the_edge() {
        let e = CoverEdge { tri: 0, a: [0.0, 0.0, 0.0], b: [100.0, 0.0, 0.0], mid: [50.0, 0.0, 0.0], out: [0.0, 1.0] };
        let [p, q] = peek_points(&e);
        assert!((p[0] - (50.0 + 50.0 + PEEK_STEP)).abs() < 1e-3 && (p[1] + COVER_STANDOFF).abs() < 1e-3);
        assert!((q[0] - (50.0 - 50.0 - PEEK_STEP)).abs() < 1e-3);
    }

    #[test]
    fn a_wall_step_goes_across_the_line_either_way() {
        // Target due east: the steps go north and south, the chosen side first.
        let [a, b] = side_steps([0.0, 0.0, 10.0], [1000.0, 0.0, 0.0], SIDESTEP, true);
        assert!(a[0].abs() < 1e-3 && (a[1] - SIDESTEP).abs() < 1e-3 && a[2] == 10.0);
        assert!(b[0].abs() < 1e-3 && (b[1] + SIDESTEP).abs() < 1e-3);
        assert_eq!(side_steps([0.0; 3], [1000.0, 0.0, 0.0], SIDESTEP, false), [b, a].map(|p| [p[0], p[1], 0.0]));
        // Without a collision world every line is clear: the chest is the point.
        let (chest, head) = ([500.0, 0.0, 50.0], [500.0, 0.0, 70.0]);
        assert_eq!(shot_clear(None, [0.0, 0.0, 60.0], Some([10.0, -5.0, 50.0]), chest, head), Some(chest));
        assert_eq!(wall_step(None, [0.0; 3], 60.0, chest, chest, true).map(|p| p[1]), Some(SIDESTEP));
    }

    #[test]
    fn cover_is_taken_first_from_the_nearest_gunman_firing_at_the_fighter() {
        let target = Threat { actor: 9, chest: [2000.0, 0.0, 50.0] };
        let shooters = [(7, [900.0, 0.0, 50.0]), (8, [500.0, 0.0, 50.0]), (9, [2000.0, 0.0, 50.0])];
        let order: Vec<u32> = threats(&shooters, target, [0.0, 0.0, 0.0]).iter().map(|t| t.actor).collect();
        // The target firing at it too comes once, last.
        assert_eq!(order, [8, 7, 9]);
        assert_eq!(threats(&[], target, [0.0; 3]), vec![target]);
    }

    #[test]
    fn only_the_run_to_cover_turns_away_from_the_target() {
        let e = CoverEdge { tri: 0, a: [0.0, 0.0, 0.0], b: [100.0, 0.0, 0.0], mid: [50.0, 0.0, 0.0], out: [0.0, 1.0] };
        let (behind, peek) = ([50.0, -40.0, 0.0], [214.0, -40.0, 0.0]);
        let at = |phase| cover_post(&Cover { behind, peek, edge: e, phase, low: false, from: 1, room: [f32::INFINITY; 2] });
        assert_eq!(at(Phase::Going), (behind, PostKind::Cover));
        // Back behind it from the peek point, and out again: facing the target, strafing.
        assert_eq!(at(Phase::Hiding { until: 1.0 }), (behind, PostKind::Duck));
        assert_eq!(at(Phase::WaitToFire { until: 1.0 }), (peek, PostKind::Peek));
        assert_eq!(at(Phase::Firing { until: 1.0 }), (peek, PostKind::Peek));
    }
}
