//! Following a path: [`Follower`] says where to head from the actor's current feet
//! (whatever moves the actor: the physics character controller, or [`Walker`], which
//! keeps the actor on the navmesh surface without physics).
//!
//! Arrival radii, turn rates and the stuck rule are engine choices
//! (docs/design/navmesh.md §4), not observed game values.

use crate::graph::{DoorEdge, NavGraph, Space, NONE};
use crate::jump::Flight;
use crate::path::{Leg, LedgeHop, Path, PathError, PathParams, Place, LEVEL_GAP};
use serde::Serialize;

/// What to do next.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Steer {
    /// Head for `target`; `dir` is the horizontal unit direction, `to_target` the
    /// horizontal distance to it, `remaining` the horizontal length left on the path
    /// (to its end, or to the door that ends this leg).
    Walk { target: [f32; 3], dir: [f32; 2], to_target: f32, remaining: f32 },
    /// At a door: move the actor to `arrival` (in the next leg's space), then call
    /// [`Follower::door_used`]. An actor that can turn first stops and faces the door
    /// ([`Walker::face_door`]) while this is given.
    Door(DoorEdge),
    /// At the stop in front of a door that opens in place (`at`): stand, open the door,
    /// and once it is fully open (its "Open" sequence has ended) call
    /// [`Follower::door_opened`]. Until then the follower holds here. A door already
    /// open is passed at once by calling `door_opened` straight away.
    OpenDoor { door: u32, at: [f32; 3] },
    /// At the end of the path.
    Arrived,
    /// At the end of the path seen from above, but not at its height (on another floor,
    /// say): not there ([`goal_height_fits`]). What to do about it: [`Walker::off_path`].
    OffPath,
}

/// A follower's default goal radius: the end of the way is reached within it.
pub const GOAL_RADIUS: f32 = 1.0;

/// Where along a path an actor is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Follower {
    pub path: Path,
    pub leg: usize,
    /// The point of the leg being walked to.
    pub next: usize,
    /// A corner counts as reached within this horizontal distance.
    pub arrive_radius: f32,
    /// The end of the path counts as reached within this horizontal distance.
    pub goal_radius: f32,
    /// A door that ends a leg is used from within this horizontal distance of the leg's
    /// end (in front of the door, where the frame may keep a body from the exact point).
    pub door_radius: f32,
    /// The next stop of the leg's `door_stops` (doors that open in place).
    pub stop: usize,
    /// Held at that stop ([`Steer::OpenDoor`] given) until [`Follower::door_opened`]:
    /// a door pushing the actor about while it opens does not send it walking again.
    pub holding: bool,
}

/// A place a stuck actor could be moved on to along its path ([`Follower::skip_candidates`]):
/// nothing has changed until [`Follower::commit_skip`], which the caller does once the
/// place is found good ([`NavGraph::skip_surface`], then its collision).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SkipCandidate {
    /// On the path; its height is the path's between two corners, a hint and not the
    /// surface's.
    pub pos: [f32; 3],
    /// The corner to walk to from there ([`Follower::next`] once committed).
    pub next: usize,
    /// Horizontal length of path from the feet's projection on the path.
    pub along: f32,
    /// At the leg's next door stop or its end, as far as a skip goes.
    pub at_limit: bool,
}

/// Where a skip measures from: the feet's projection on the stretch being walked.
struct SkipFrom {
    projection: [f32; 3],
    /// The corner the stretch ends at.
    corner: usize,
    /// The last point a skip may reach: the next door stop, or the leg's end.
    limit: usize,
    /// Horizontal distance of the feet from the projection.
    off: f32,
}

/// A ledge's take-off counts as reached within this horizontal distance: the actor goes
/// to the edge before it jumps [design].
pub const TAKE_OFF_RADIUS: f32 = 2.0;

/// How far above or below the end of its path feet right at it may be and still be there,
/// game units [design: an engineering number, not measured safe; a stair step fits it, a
/// storey does not].
pub const GOAL_HEIGHT_MARGIN: f32 = 40.0;
/// What the margin grows by per unit of horizontal distance from the end of the path: the
/// tangent of 60 degrees, so that feet on a slope up to that steep fit [design].
pub const GOAL_HEIGHT_SLOPE: f32 = 1.732_050_8;
/// New paths a [`Walker`] plans in a trip for feet that do not fit its goal's height,
/// before the height is waived [design].
pub const HEIGHT_REPLANS: u8 = 2;
/// A new path starting further than this above or below the feet is no way on from where
/// the actor stands; the height is waived instead, game units [design: provisional].
pub const REPLAN_START_HEIGHT: f32 = 64.0;
/// A destination this close to the one asked for before is the same trip, game units
/// [design: a Follow package asks for about the same place every half second].
pub const SAME_TRIP: f32 = 256.0;

fn hdist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Whether feet are at a height that fits the end of a path `goal` (a point on the
/// navmesh: the path's own end, where the destination was snapped to) [design,
/// docs/design/navmesh.md §4.1].
pub fn goal_height_fits(feet: [f32; 3], goal: [f32; 3]) -> bool {
    (feet[2] - goal[2]).abs() <= GOAL_HEIGHT_MARGIN + hdist(feet, goal) * GOAL_HEIGHT_SLOPE
}

impl Follower {
    pub fn new(path: Path) -> Follower {
        Follower { path, leg: 0, next: 1, arrive_radius: 16.0, goal_radius: GOAL_RADIUS, door_radius: 48.0, stop: 0, holding: false }
    }

    /// The space of the leg being walked.
    pub fn space(&self) -> Option<Space> {
        self.path.legs.get(self.leg).map(|l| l.space)
    }

    /// Advance past reached corners and say where to head from `feet`.
    pub fn steer(&mut self, feet: [f32; 3]) -> Steer {
        let Some(leg) = self.path.legs.get(self.leg) else { return Steer::Arrived };
        let pts = &leg.points;
        let pending = leg.door_stops.get(self.stop).copied();
        // A door stop reached, or at or behind the corner being walked to, holds the
        // follower there.
        if let Some(s) = pending.filter(|s| self.holding || s.point < self.next) {
            self.holding = true;
            return Steer::OpenDoor { door: s.door, at: pts[s.point] };
        }
        while self.next < pts.len() {
            let q = pts[self.next];
            let d = hdist(feet, q);
            let last = self.next + 1 == pts.len();
            let stop = pending.filter(|s| s.point == self.next);
            let goal = last && leg.door.is_none() && self.leg + 1 == self.path.legs.len();
            let radius = if stop.is_some() {
                self.arrive_radius
            } else if leg.hops.iter().any(|h| h.point == self.next) {
                TAKE_OFF_RADIUS
            } else if goal {
                self.goal_radius
            } else if last && leg.door.is_some() {
                self.door_radius
            } else {
                self.arrive_radius
            };
            // Passed a corner (beyond it along the segment that ends there) while close.
            let passed = !last && d < 4.0 * self.arrive_radius && {
                let p = pts[self.next - 1];
                let (sx, sy) = (q[0] - p[0], q[1] - p[1]);
                (feet[0] - q[0]) * sx + (feet[1] - q[1]) * sy >= 0.0
            };
            // A hair over, from a step that stopped at the radius (`Walker::update_with`).
            if d <= radius + 1e-3 || passed {
                if let Some(s) = stop {
                    self.holding = true;
                    return Steer::OpenDoor { door: s.door, at: q };
                }
                self.next += 1;
                continue;
            }
            break;
        }
        if self.next >= pts.len() {
            return match leg.door {
                Some(d) => Steer::Door(d),
                None if self.leg + 1 == self.path.legs.len() && !self.fits_goal(feet) => Steer::OffPath,
                None => Steer::Arrived,
            };
        }
        let target = pts[self.next];
        let to_target = hdist(feet, target);
        let dir = if to_target > 1e-4 { [(target[0] - feet[0]) / to_target, (target[1] - feet[1]) / to_target] } else { [0.0, 0.0] };
        let remaining = to_target + pts[self.next..].windows(2).map(|w| hdist(w[0], w[1])).sum::<f32>();
        Steer::Walk { target, dir, to_target, remaining }
    }

    /// The next door on the way, when the actor at `feet` is within `reach` (horizontal
    /// units) of it: the current leg's next door that opens in place (its stop), else the
    /// door that ends the leg. Held at a stop, that stop's door whatever the distance.
    /// For whoever has to ask the door first (`fnv_game::npc_doors`).
    pub fn door_ahead(&self, feet: [f32; 3], reach: f32) -> Option<u32> {
        let leg = self.path.legs.get(self.leg)?;
        if let Some(s) = leg.door_stops.get(self.stop) {
            return (self.holding || hdist(feet, leg.points[s.point]) <= reach).then_some(s.door);
        }
        let d = leg.door?;
        (hdist(feet, *leg.points.last()?) <= reach).then_some(d.door)
    }

    /// The door that ended the current leg was used: go on with the next leg.
    pub fn door_used(&mut self) {
        self.leg += 1;
        self.next = 1;
        self.stop = 0;
        self.holding = false;
    }

    /// The door of the current [`Steer::OpenDoor`] is open: walk on through it.
    pub fn door_opened(&mut self) {
        let Some(s) = self.path.legs.get(self.leg).and_then(|l| l.door_stops.get(self.stop)).copied() else { return };
        self.stop += 1;
        self.holding = false;
        self.next = self.next.max(s.point + 1);
    }

    /// Move on along the path `ahead` horizontal units from the projection of `feet` on
    /// the stretch being walked, as far as the leg's next door stop or its end at most,
    /// and return that point. Corners passed on the way count as reached. `None` while
    /// held at a door stop, with a door stop behind the corner being walked to (steering
    /// holds there), with no leg left or with numbers that are not finite.
    /// [`Follower::skip_candidate`] and [`Follower::commit_skip`] in one step, with no
    /// check of the place: the engine's stuck actors go through
    /// [`Follower::skip_candidates`] ([`Advice::Skip`]).
    pub fn skip_ahead(&mut self, feet: [f32; 3], ahead: f32) -> Option<[f32; 3]> {
        let c = self.skip_candidate(feet, ahead)?;
        self.commit_skip(&c);
        Some(c.pos)
    }

    /// The place [`Follower::skip_ahead`] would move on to, changing nothing.
    pub fn skip_candidate(&self, feet: [f32; 3], ahead: f32) -> Option<SkipCandidate> {
        if !ahead.is_finite() {
            return None;
        }
        Some(self.skip_along(&self.skip_from(feet)?, ahead.max(0.0)))
    }

    /// The places at each of `distances` along the path from the projection of `feet`, in
    /// that order, cut to the leg's next door stop or its end and without repeats. No
    /// distance on from the projection counts only for feet off the path by more than
    /// [`Follower::arrive_radius`] (or at the limit, where steering then holds, uses the
    /// door or arrives): anywhere else it would put the actor down where it stands. Empty
    /// for what makes [`Follower::skip_ahead`] `None`. Changes nothing.
    pub fn skip_candidates(&self, feet: [f32; 3], distances: &[f32]) -> Vec<SkipCandidate> {
        let Some(from) = self.skip_from(feet) else { return Vec::new() };
        let mut out: Vec<SkipCandidate> = Vec::with_capacity(distances.len());
        for &d in distances.iter().filter(|d| d.is_finite()) {
            let c = self.skip_along(&from, d.max(0.0));
            let here = c.along <= 0.0 && !c.at_limit && from.off <= self.arrive_radius;
            if !here && out.iter().all(|o| (o.along - c.along).abs() > 1e-3) {
                out.push(c);
            }
        }
        out
    }

    /// Go on from a candidate found good: corners it passed count as reached.
    pub fn commit_skip(&mut self, c: &SkipCandidate) {
        self.next = self.next.max(c.next);
    }

    fn skip_from(&self, feet: [f32; 3]) -> Option<SkipFrom> {
        if self.holding || !feet.iter().all(|v| v.is_finite()) {
            return None;
        }
        let leg = self.path.legs.get(self.leg)?;
        let pts = &leg.points;
        let last = pts.len().checked_sub(1)?;
        let stop = leg.door_stops.get(self.stop);
        if stop.is_some_and(|s| s.point < self.next) {
            return None;
        }
        let limit = stop.map_or(last, |s| s.point.min(last));
        let corner = self.next.max(1);
        if !pts[corner.min(limit + 1) - 1..=limit].iter().flatten().all(|v| v.is_finite()) {
            return None;
        }
        // Past the limit (the corners used up, or a path of one point): only the limit.
        if corner > limit {
            return Some(SkipFrom { projection: pts[limit], corner, limit, off: hdist(feet, pts[limit]) });
        }
        // Onto the stretch being walked and not the nearest of them all: a path that
        // comes back past the actor would otherwise have it jump along the loop.
        let (a, b) = (pts[corner - 1], pts[corner]);
        let (sx, sy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = sx * sx + sy * sy;
        let t = if len2 > 1e-6 { (((feet[0] - a[0]) * sx + (feet[1] - a[1]) * sy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let projection = [a[0] + sx * t, a[1] + sy * t, a[2] + (b[2] - a[2]) * t];
        Some(SkipFrom { projection, corner, limit, off: hdist(feet, projection) })
    }

    /// `ahead` horizontal units on from `from`. Only for a [`SkipFrom`] of this follower,
    /// so the leg is there.
    fn skip_along(&self, from: &SkipFrom, ahead: f32) -> SkipCandidate {
        let pts = &self.path.legs[self.leg].points;
        let mut at = from.projection;
        let mut left = ahead;
        for (i, &q) in pts.iter().enumerate().take(from.limit + 1).skip(from.corner) {
            let seg = hdist(at, q);
            if left < seg {
                let t = left / seg;
                return SkipCandidate { pos: [at[0] + (q[0] - at[0]) * t, at[1] + (q[1] - at[1]) * t, at[2] + (q[2] - at[2]) * t], next: i, along: ahead, at_limit: false };
            }
            left -= seg;
            at = q;
        }
        // At the door stop or the leg's end: steering from there holds, uses the door or
        // arrives.
        SkipCandidate { pos: pts[from.limit], next: self.next.max(from.limit), along: ahead - left, at_limit: true }
    }

    /// Whether the rest of the path (the leg being walked and those after it) crosses a
    /// triangle paths may no longer cross (a navmesh disabled since it was planned,
    /// [`NavGraph::usable`]).
    pub fn blocked(&self, nav: &NavGraph) -> bool {
        self.path.legs.iter().skip(self.leg).flat_map(|l| &l.corridor).any(|&t| (t as usize) < nav.tris.len() && !nav.usable(t))
    }

    /// Whether the rest of the path goes through one of `doors`: a load door ending a
    /// leg still ahead, or a door that opens in place on the way.
    pub fn through_doors(&self, doors: &[u32]) -> bool {
        self.path.legs.iter().skip(self.leg).any(|l| l.door.is_some_and(|d| doors.contains(&d.door)) || l.door_stops.iter().any(|s| doors.contains(&s.door)))
    }

    /// The ledge link being crossed: the corner walked to is the landing of one of the
    /// leg's [`LedgeHop`]s (its take-off reached). With the landing point.
    pub fn hop(&self) -> Option<(LedgeHop, [f32; 3])> {
        let leg = self.path.legs.get(self.leg)?;
        let h = *leg.hops.iter().find(|h| h.point + 1 == self.next)?;
        Some((h, *leg.points.get(self.next)?))
    }

    /// The take-off point of the ledge link [`Follower::hop`] names: the path point
    /// before its landing (`LedgeHop::point`).
    pub fn take_off(&self) -> Option<[f32; 3]> {
        let leg = self.path.legs.get(self.leg)?;
        let h = leg.hops.iter().find(|h| h.point + 1 == self.next)?;
        leg.points.get(h.point).copied()
    }

    /// The path's destination.
    pub fn destination(&self) -> Option<Place> {
        let l = self.path.legs.last()?;
        Some(Place { space: l.space, pos: *l.points.last()? })
    }

    /// Whether `feet` are at a height that fits the path's destination.
    pub fn fits_goal(&self, feet: [f32; 3]) -> bool {
        self.destination().is_none_or(|g| goal_height_fits(feet, g.pos))
    }
}

impl NavGraph {
    /// The navmesh surface under a [`SkipCandidate`] of `leg`: the place with the height
    /// of the triangle under it. `None` when the path says nothing is under it.
    ///
    /// Looked for from the route, in this order [design]. A corner of the path: the
    /// triangle under it nearest its height. A place between two corners: walking the
    /// surface to it from the corner before it, else from the corner after it (a walk
    /// follows the ground over a rise that the straight height between corners cuts
    /// through; the one after because what cut the navmesh behind the place, an obstacle
    /// or a navmesh switched off since the path was planned, is often what the actor is
    /// stuck at, and the place past it is what recovery is for). Both walks cut off, any
    /// usable triangle under it, the one nearest the path's height there and within
    /// [`LEVEL_GAP`] of it: a nearest floor, as the two ends of a walk leave no better
    /// guide. That limits the distance from the path's straight height and does not say
    /// the floor is the route's: another level within the limit can be taken, and nothing
    /// refuses it later. The leg's corridor is not asked: the shortcut pass straightens a path over triangles it does
    /// not list ([`NavGraph::walkable_line`]). Inside a ledge link's span there is nothing
    /// to stand on. No clearance from the outline is asked: the plan gave the path what
    /// room there was, and the body's room is the collision's to say. The actor's own
    /// limits (keys, water, slopes) are not checked again either, as [`Path`] does not
    /// keep them: the route was planned with them, this only follows it.
    pub fn skip_surface(&self, leg: &Leg, c: &SkipCandidate) -> Option<[f32; 3]> {
        let pos = c.pos;
        if !pos.iter().all(|v| v.is_finite()) {
            return None;
        }
        let pts = &leg.points;
        let same = |p: Option<&[f32; 3]>| p.is_some_and(|q| (0..3).all(|i| (q[i] - pos[i]).abs() <= 1e-3));
        let before = c.next.checked_sub(1).and_then(|i| pts.get(i));
        let after = pts.get(c.next);
        let corner = same(before) || same(after);
        if !corner && leg.hops.iter().any(|h| h.point + 1 == c.next) {
            return None;
        }
        let on = |t: u32| [pos[0], pos[1], self.height_on(t, pos[0], pos[1])];
        if corner {
            return self.locate(leg.space, pos, LEVEL_GAP).map(on);
        }
        // The route's own limits are behind it; the walk only asks where it leads.
        let route = PathParams { open_locked: true, ..PathParams::default() };
        let walk = |from: Option<&[f32; 3]>| {
            let &a = from?;
            let start = self.locate(leg.space, a, LEVEL_GAP)?;
            self.trace_line(leg.space, start, a, pos, 0.0, &route)
        };
        walk(before).or_else(|| walk(after)).or_else(|| self.locate(leg.space, pos, LEVEL_GAP)).map(on)
    }
}

/// What [`Unstuck`] advises.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Advice {
    /// Head this way (the path's direction, or a detour round an obstacle).
    Head([f32; 2]),
    /// Detours did not help: plan the path again from here.
    Replan,
    /// Detours did not help: move the actor on along its path, past what blocks it
    /// ([`Follower::skip_ahead`]; given instead of [`Advice::Replan`] by an [`Unstuck`]
    /// with `skip` set).
    Skip,
    /// Replanning did not help either.
    GiveUp,
}

/// Getting an actor moved by physics past obstacles the navmesh does not know about
/// (low rocks the navmesh covers but the character controller cannot climb, clutter,
/// other actors). When the actor has got less than a fifth of its speed closer to the end
/// of its path (the path's remaining length) over the last second, it detours: turned 70° one way for 0.6 s, then the other way, then 110° each
/// way; if still stuck, the path is planned again (or, with `skip`, the actor is moved on
/// along it: the failsafe the owner asked for on 2026-10-17); after `max_replans`, it
/// gives up. An engine choice (docs/design/navmesh.md §4), not the original game's behaviour.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unstuck {
    pub speed: f32,
    pub max_replans: usize,
    /// Advise [`Advice::Skip`] where it would advise [`Advice::Replan`].
    pub skip: bool,
    /// The path's remaining length over the last second, one sample per update with its
    /// time step.
    #[serde(skip)]
    history: std::collections::VecDeque<(f32, f32)>,
    /// The detour being made: index into [`DETOURS`] and time left.
    detour: Option<(usize, f32)>,
    /// Detours tried since the actor got stuck.
    tried: usize,
    /// The remaining length where the actor got stuck: detours count as having helped
    /// only once the actor is half a second's walk closer than this.
    stuck_at: Option<f32>,
    /// [`Unstuck::advise_moved`]: where the feet were and which way the actor was
    /// heading at the last update, and the progress so far, negated (counting down from
    /// 0, as a remaining length would).
    #[serde(skip)]
    last: Option<([f32; 2], [f32; 2])>,
    #[serde(skip)]
    moved_left: f32,
    pub detours: usize,
    /// Replans (or skips) advised.
    pub replans: usize,
    /// An [`Advice::Skip`] was given and not refused ([`Unstuck::skip_refused`]).
    pub skipped: bool,
}

/// Detour turns (radians, positive = to the left / counter-clockwise seen from above).
const DETOURS: [f32; 4] = [1.22, -1.22, 1.92, -1.92];
const DETOUR_SECONDS: f32 = 0.6;

fn rotate(d: [f32; 2], a: f32) -> [f32; 2] {
    let (s, c) = a.sin_cos();
    [d[0] * c - d[1] * s, d[0] * s + d[1] * c]
}

impl Unstuck {
    pub fn new(speed: f32) -> Unstuck {
        Unstuck { speed, max_replans: 5, skip: false, history: Default::default(), detour: None, tried: 0, stuck_at: None, last: None, moved_left: 0.0, detours: 0, replans: 0, skipped: false }
    }

    /// For an actor just moved on along its path ([`Advice::Skip`]): the detours were
    /// tried where it got stuck, so if it is stuck again before it has made a second's
    /// progress it is moved on again at once (a second without progress), not after the
    /// four detours. Once it gets on, the detours come back [design: a stretch blocked for
    /// longer than one skip (Boone just past his room's door, follower-boone smoke,
    /// 2026-10-17) cost 7.4 s per 160 units].
    pub fn after_skip(&mut self) {
        self.tried = DETOURS.len();
    }

    /// The [`Advice::Skip`] just given found nowhere to put the actor, which stays where
    /// it is: stuck after another second, it is advised to skip again, without the detours
    /// that failed already. A refused skip does not count towards
    /// [`Unstuck::max_replans`], so refusals never end in [`Advice::GiveUp`] [design].
    pub fn skip_refused(&mut self) {
        self.replans = self.replans.saturating_sub(1);
        self.skipped = false;
        self.tried = DETOURS.len();
    }

    /// Forget the progress history (after a door, a teleport or a new path).
    pub fn reset(&mut self) {
        self.history.clear();
        self.detour = None;
        self.stuck_at = None;
        self.last = None;
    }

    /// As [`Unstuck::advise`], with progress measured by how far the feet moved along the
    /// way the actor was to go (`want` of the last update), not by the path's remaining
    /// length: for paths planned again and again after a moving target (Follow packages,
    /// owner's playtest 2026-10-17: a pack brahmin running after its merchant kept a
    /// path of about the same length, counted as stuck all the way and detoured into a
    /// fence). Sliding along an obstacle, across the way, is still no progress.
    pub fn advise_moved(&mut self, feet: [f32; 3], want: [f32; 2], dt: f32) -> Advice {
        if let Some((at, way)) = self.last {
            self.moved_left -= (feet[0] - at[0]) * way[0] + (feet[1] - at[1]) * way[1];
        }
        self.last = Some(([feet[0], feet[1]], want));
        self.advise(self.moved_left, want, dt)
    }

    /// Record the path's remaining length after a step of `dt` seconds and say which way
    /// to head next, given the path's direction `want`. Moving without getting closer
    /// along the path (sliding round an obstacle, a detour that does not help) is not
    /// progress.
    pub fn advise(&mut self, remaining: f32, want: [f32; 2], dt: f32) -> Advice {
        self.history.push_back((remaining, dt));
        let mut span: f32 = self.history.iter().map(|h| h.1).sum();
        while span - self.history.front().map_or(0.0, |h| h.1) >= 1.0 {
            span -= self.history.pop_front().expect("non-empty").1;
        }
        if let Some((i, left)) = self.detour {
            let left = left - dt;
            if left > 0.0 {
                self.detour = Some((i, left));
                return Advice::Head(rotate(want, DETOURS[i]));
            }
            self.detour = None;
            self.history.clear();
            self.history.push_back((remaining, dt));
            return Advice::Head(want);
        }
        let first = self.history.front().expect("just pushed").0;
        let progress = first - remaining;
        if span < 1.0 {
            return Advice::Head(want);
        }
        if progress >= 0.2 * self.speed {
            if self.stuck_at.is_none_or(|m| remaining < m - 0.5 * self.speed) {
                self.tried = 0;
                self.stuck_at = None;
            }
            return Advice::Head(want);
        }
        // Stuck.
        self.history.clear();
        self.stuck_at = Some(self.stuck_at.map_or(remaining, |m| m.min(remaining)));
        if self.tried < DETOURS.len() {
            let i = self.tried;
            self.tried += 1;
            self.detours += 1;
            self.detour = Some((i, DETOUR_SECONDS));
            return Advice::Head(rotate(want, DETOURS[i]));
        }
        self.tried = 0;
        if self.replans >= self.max_replans {
            return Advice::GiveUp;
        }
        self.replans += 1;
        if self.skip {
            self.skipped = true;
            Advice::Skip
        } else {
            Advice::Replan
        }
    }
}

/// What a [`Walker`] did in an update.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum WalkEvent {
    /// No path, or standing at its end.
    Idle,
    Moved,
    /// Stood and turned on the spot towards the way to walk (not there yet, or just now).
    Turned,
    /// Used a door; the walker is now at its arrival, in another space or place.
    UsedDoor(DoorEdge),
    /// Reached the end of the path in this update.
    Arrived,
    /// Standing in front of a door that opens in place, waiting for it to open.
    WaitingForDoor(u32),
}

/// An actor moved along a path on the navmesh surface, without physics (the actor's
/// height is the navmesh's under it). For actors with no collision world around them;
/// the engine moves actors with collision through the character controller instead.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Walker {
    pub space: Space,
    pub feet: [f32; 3],
    /// Compass heading, radians (0 north, π/2 east).
    pub heading: f32,
    /// The triangle under the feet, or [`NONE`].
    pub tri: u32,
    pub speed: f32,
    /// The share of [`Self::speed`] it moves at (1: all of it): an actor's crippled legs
    /// slow it whatever speed its package or fight asks for ([`Self::moving_speed`]).
    pub pace: f32,
    /// Fastest turn while walking, radians per second.
    pub turn_rate: f32,
    /// Turn speed standing, radians per second ([`SPOT_TURN_RATE`]).
    pub spot_turn_rate: f32,
    /// Standing, a way to walk further than this off the heading (radians) is turned to
    /// on the spot before setting off ([`SPOT_TURN_START`]).
    pub spot_turn_start: f32,
    /// Turning on the spot before walking on.
    pub turning: bool,
    /// Moved in the last update (a walker that did not is standing).
    pub moving: bool,
    pub follower: Option<Follower>,
    /// Horizontal distance moved so far.
    pub travelled: f32,
    /// In the air over a ledge link ([`crate::jump`]): the arc and the time since take-off.
    pub air: Option<Air>,
    /// Ledges jumped (taken off over) and landings, so far.
    pub flights: u32,
    pub landings: u32,
    /// How far its heading was off the door (radians) when it last went through a load
    /// door whose place is known: 0 once it has turned to it ([`Walker::face_door`]).
    pub door_off: Option<f32>,
    /// The trip [`Walker::go_to`] last set off on. It counts while [`Self::follower`] is
    /// set: a walk that ended, arrived or cancelled, leaves the next `go_to` a new trip.
    #[serde(skip)]
    pub trip: Option<Trip>,
    /// Searches for a new path [`Walker::off_path`] made so far, all trips.
    #[serde(skip)]
    pub height_searches: u32,
    /// Goal heights waived so far ([`Walker::off_path`]).
    #[serde(skip)]
    pub height_waivers: u32,
}

/// Where a [`Walker`] was last asked to go: the place as asked for, not where the path
/// was snapped to, and what recovering from a height that does not fit has used up. Held
/// here and not on the [`Follower`], which each new path replaces.
#[derive(Debug, Clone, PartialEq)]
pub struct Trip {
    pub to: Place,
    pub params: PathParams,
    /// New paths planned in this trip ([`HEIGHT_REPLANS`]).
    pub replans: u8,
    /// The height is no longer asked for: the walk ends as if it fitted.
    pub waived: bool,
}

/// How far from a ledge link's take-off point the feet may be for the flight to leave
/// from that point, game units [design: about a frame's step at a run].
pub const TAKE_OFF_SNAP: f32 = 16.0;

/// A [`Walker`]'s flight over a ledge link.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Air {
    pub flight: Flight,
    pub t: f32,
}

/// Fastest turn of a walking actor, radians per second (180 degrees per second)
/// [design; owner's playtest 2026-10-11: actors turned half about in a frame or two].
/// The game's characters turn at the setting `fCharacterDefaultTurningSpeed` [documented:
/// GECK wiki, Settings list], whose value is not in FalloutNV.esm (a built-in default);
/// creatures have their own, CREA `TNAM` "Turning Speed" [documented: xEdit], degrees per
/// second × 1.25 on big turns [observed, actors.md §8.10 "TNAM"], given to their walkers
/// by the engine (`creature_pace::creature_turn_rate`). Was 720 degrees per second.
pub const WALK_TURN_RATE: f32 = std::f32::consts::PI;

/// While its heading catches up with the way to go, a walking actor keeps this share of
/// its speed at the least (heading a quarter turn or more off) and all of it when facing
/// the way ([`turn_slowdown`]) [design].
pub const TURN_SLOW_MIN: f32 = 0.3;

/// The share of its speed a walking actor keeps when its heading is `off` radians from
/// the way to go: it slows to turn [design].
pub fn turn_slowdown(off: f32) -> f32 {
    TURN_SLOW_MIN + (1.0 - TURN_SLOW_MIN) * off.cos().max(0.0)
}

/// The direction a heading faces (0 north = +y, π/2 east = +x).
pub fn dir_of(heading: f32) -> [f32; 2] {
    let (s, c) = heading.sin_cos();
    [s, c]
}

/// Which way a walking actor moves while it turns towards `way` (a unit direction) with
/// its heading now at `heading`: along its heading, so that it walks an arc round a
/// corner, while that is within a quarter turn of the way; else along the way itself
/// (it does not walk off backwards) [design]. With the share of its speed it keeps
/// ([`turn_slowdown`]).
pub fn arc_step(heading: f32, way: [f32; 2]) -> ([f32; 2], f32) {
    let off = heading_delta(heading, heading_of(way)).abs();
    let along = if off < std::f32::consts::FRAC_PI_2 { dir_of(heading) } else { way };
    (along, turn_slowdown(off))
}

/// Turn speed of a standing actor, radians per second (120 degrees per second) [design:
/// the turn clips carry no rotation, docs/spec-notes/actors.md §8.2 and §8.9].
pub const SPOT_TURN_RATE: f32 = 120.0 * std::f32::consts::PI / 180.0;
/// A standing actor turns on the spot before walking when its way lies further than
/// this off its heading, radians (20 degrees) [design, actors.md §8.9].
pub const SPOT_TURN_START: f32 = 20.0 * std::f32::consts::PI / 180.0;

/// Compass heading of a horizontal direction (0 north = +y, π/2 east = +x).
pub fn heading_of(dir: [f32; 2]) -> f32 {
    dir[0].atan2(dir[1])
}

/// Turn `from` towards `to` (radians) by at most `max`, the short way round.
pub fn turn_towards(from: f32, to: f32, max: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let d = (to - from + PI).rem_euclid(TAU) - PI;
    let r = from + d.clamp(-max, max);
    (r + PI).rem_euclid(TAU) - PI
}

/// The turn from heading `from` to `to`, radians in [-π, π): positive is clockwise seen
/// from above (to the right), as compass headings grow.
pub fn heading_delta(from: f32, to: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (to - from + PI).rem_euclid(TAU) - PI
}

/// Headings closer than this count as the same (radians).
const FACING: f32 = 1e-4;

impl Walker {
    pub fn new(space: Space, feet: [f32; 3], heading: f32, speed: f32) -> Walker {
        Walker {
            space,
            feet,
            heading,
            tri: NONE,
            speed,
            pace: 1.0,
            turn_rate: WALK_TURN_RATE,
            spot_turn_rate: SPOT_TURN_RATE,
            spot_turn_start: SPOT_TURN_START,
            turning: false,
            moving: false,
            follower: None,
            travelled: 0.0,
            air: None,
            flights: 0,
            landings: 0,
            door_off: None,
            trip: None,
            height_searches: 0,
            height_waivers: 0,
        }
    }

    /// The speed it moves at: [`Self::speed`] times [`Self::pace`].
    pub fn moving_speed(&self) -> f32 {
        self.speed * self.pace
    }

    /// Turn on the spot towards `heading` for `dt` seconds, at the standing turn speed.
    /// Returns whether the walker faces it now. For an actor standing with nowhere to
    /// go (facing whoever it talks to, say); on a path the walker turns by itself.
    pub fn face(&mut self, heading: f32, dt: f32) -> bool {
        self.heading = turn_towards(self.heading, heading, self.spot_turn_rate * dt.max(0.0));
        heading_delta(self.heading, heading).abs() <= FACING
    }

    /// At load door `d` (given [`Steer::Door`]) with the feet at `feet`: stand and turn on
    /// the spot to face the door itself before going through, rather than carry on into
    /// it along the way the actor came, which from the side walked it past the frame
    /// (#1472) [design: how the game turns actors at doors is not measured]. Returns
    /// whether it is still turning; once it faces the door (or the door's place is not
    /// known) the door is used.
    pub fn face_door(&mut self, d: &DoorEdge, feet: [f32; 3], dt: f32) -> bool {
        let Some(at) = d.door_at else { return false };
        let dir = [at[0] - feet[0], at[1] - feet[1]];
        if dir[0].hypot(dir[1]) < 1.0 {
            return false;
        }
        let want = heading_of(dir);
        if heading_delta(self.heading, want).abs() <= FACING {
            return false;
        }
        self.face(want, dt);
        true
    }

    /// Plan a path from the walker's feet to `to` and start following it.
    pub fn go_to(&mut self, nav: &NavGraph, to: Place, params: &PathParams) -> Result<&Path, PathError> {
        let path = nav.find_path(Place { space: self.space, pos: self.feet }, to, params)?;
        let walking = self.follower.is_some();
        self.follower = Some(Follower::new(path));
        let same = self.trip.as_ref().filter(|t| walking && t.to.space == to.space && (0..3).map(|i| (t.to.pos[i] - to.pos[i]).powi(2)).sum::<f32>() <= SAME_TRIP * SAME_TRIP);
        let (replans, waived) = same.map_or((0, false), |t| (t.replans, t.waived));
        self.trip = Some(Trip { to, params: params.clone(), replans, waived });
        Ok(&self.follower.as_ref().expect("just set").path)
    }

    /// `old` said [`Steer::OffPath`] with the feet at `feet`: the follower of a new path
    /// from there to the place asked for, or `None` when the height is waived and the
    /// walk ends as if it fitted (no tries left, no such path, a path through a door, a
    /// path starting too far above or below the feet, or a follower no [`Walker::go_to`]
    /// gave). Never fails the walk: a package or a sandbox sees an arrival, and a new path
    /// has no door to be refused at [design, docs/design/navmesh.md §4.1].
    pub fn off_path(&mut self, nav: &NavGraph, feet: [f32; 3], old: &Follower) -> Option<Follower> {
        let from = Place { space: self.space, pos: feet };
        if let Some(t) = self.trip.as_mut() {
            if t.waived {
                return None;
            }
            if t.replans < HEIGHT_REPLANS {
                t.replans += 1;
                self.height_searches += 1;
                let fits = |p: &Path| matches!(p.legs.as_slice(), [l] if l.door_stops.is_empty() && l.points.first().is_some_and(|s| (s[2] - feet[2]).abs() <= REPLAN_START_HEIGHT));
                if let Some(path) = nav.find_path(from, t.to, &t.params).ok().filter(fits) {
                    let mut f = Follower::new(path);
                    f.goal_radius = old.goal_radius;
                    return Some(f);
                }
            }
            t.waived = true;
        }
        self.height_waivers += 1;
        None
    }

    /// Move for `dt` seconds, passing doors that open in place as if they were open.
    pub fn update(&mut self, nav: &NavGraph, dt: f32) -> WalkEvent {
        self.update_with(nav, dt, &mut |_| true)
    }

    /// Move for `dt` seconds. At a door that opens in place, `open(door)` is asked every
    /// update until it says the door is fully open: it should start the door opening (if
    /// it is not already) and answer whether its "Open" sequence has ended; the walker
    /// stands meanwhile.
    pub fn update_with(&mut self, nav: &NavGraph, dt: f32, open: &mut dyn FnMut(u32) -> bool) -> WalkEvent {
        let Some(mut f) = self.follower.take() else { return WalkEvent::Idle };
        let ev = self.advance(nav, &mut f, dt, open);
        self.moving = ev == WalkEvent::Moved;
        // Keep the path unless it ended (an idle update with the path not finished keeps it).
        let keep = match ev {
            WalkEvent::Arrived => false,
            // `advance` ends a walk at Steer::OffPath only with the height waived.
            WalkEvent::Idle => !matches!(f.steer(self.feet), Steer::Arrived | Steer::OffPath),
            WalkEvent::Moved | WalkEvent::Turned | WalkEvent::UsedDoor(_) | WalkEvent::WaitingForDoor(_) => true,
        };
        if !keep {
            self.turning = false;
        }
        if keep {
            self.follower = Some(f);
        }
        ev
    }

    /// Fly on over a ledge for `dt` seconds; on landing the feet are on the landing
    /// point (where the path goes on from).
    fn fly(&mut self, nav: &NavGraph, dt: f32) -> WalkEvent {
        let Some(mut a) = self.air else { return WalkEvent::Idle };
        let before = self.feet;
        a.t += dt.max(0.0);
        self.feet = a.flight.at(a.t);
        self.travelled += hdist(before, self.feet);
        if a.t >= a.flight.duration {
            self.air = None;
            self.landings += 1;
            self.tri = self.tri_near(nav, self.feet).unwrap_or(NONE);
        } else {
            self.air = Some(a);
        }
        WalkEvent::Moved
    }

    fn advance(&mut self, nav: &NavGraph, f: &mut Follower, dt: f32, open: &mut dyn FnMut(u32) -> bool) -> WalkEvent {
        if self.air.is_some() {
            return self.fly(nav, dt);
        }
        let mut budget = self.moving_speed() * dt;
        let mut moved = false;
        // A step may pass several close corners.
        for _ in 0..8 {
            match f.steer(self.feet) {
                Steer::OffPath if let Some(new) = self.off_path(nav, self.feet, f) => *f = new,
                Steer::Arrived | Steer::OffPath => return if moved { WalkEvent::Arrived } else { WalkEvent::Idle },
                Steer::OpenDoor { door, .. } => {
                    if open(door) {
                        f.door_opened();
                        continue;
                    }
                    return WalkEvent::WaitingForDoor(door);
                }
                Steer::Door(d) => {
                    // It stops at the door and faces it (after this step's walk, on
                    // the next update) before it goes through.
                    if moved && d.door_at.is_some() {
                        return WalkEvent::Moved;
                    }
                    if self.face_door(&d, self.feet, dt) {
                        self.turning = false;
                        return WalkEvent::Turned;
                    }
                    self.door_off = d.door_at.map(|at| heading_delta(self.heading, heading_of([at[0] - self.feet[0], at[1] - self.feet[1]])).abs());
                    f.door_used();
                    self.space = f.space().unwrap_or(self.space);
                    self.feet = d.arrival;
                    self.heading = d.arrival_heading;
                    self.tri = d.to;
                    return WalkEvent::UsedDoor(d);
                }
                Steer::Walk { target, dir, to_target, .. } => {
                    if budget <= 1e-4 {
                        break;
                    }
                    // At the take-off of a ledge link: over it in an arc to the landing.
                    if let Some((hop, landing)) = f.hop() {
                        if dir != [0.0, 0.0] {
                            self.heading = heading_of(dir);
                        }
                        self.turning = false;
                        // From the link's own take-off point when the feet are at it, so
                        // where the actor leaves does not hang on its speed and the frame's
                        // step (the feet stand within a step past it) [design].
                        if let Some(t) = f.take_off().filter(|t| (t[0] - self.feet[0]).hypot(t[1] - self.feet[1]) <= TAKE_OFF_SNAP) {
                            self.feet = t;
                        }
                        self.air = Some(Air { flight: Flight::over_ledge(self.feet, landing, hop.up), t: 0.0 });
                        self.flights += 1;
                        let left = budget / self.moving_speed().max(1e-4);
                        return self.fly(nav, left);
                    }
                    // Standing and facing away from the way to go: turn on the spot
                    // first (the turn clips play, actors.md §8.9), then set off.
                    let want = heading_of(dir);
                    let standing = !self.moving && !moved;
                    if dir != [0.0, 0.0] && (self.turning || (standing && heading_delta(self.heading, want).abs() > self.spot_turn_start)) {
                        self.turning = !self.face(want, dt);
                        return WalkEvent::Turned;
                    }
                    self.heading = turn_towards(self.heading, want, self.turn_rate * dt);
                    // It slows while its heading catches up (on the navmesh it keeps
                    // to the path itself; the character controller walks the arc).
                    let slow = turn_slowdown(heading_delta(self.heading, want).abs());
                    // Towards the end of a way whose goal radius a caller widened, it stops
                    // at that radius, not past it: a long step (a wait's time passing)
                    // would otherwise end inside it, on the spot of the actor it was
                    // walking up to (issue #1597). With the default radius it walks onto
                    // the end, as before.
                    let to_goal = f.goal_radius > GOAL_RADIUS
                        && f.leg + 1 == f.path.legs.len() && f.path.legs[f.leg].door.is_none() && f.next + 1 == f.path.legs[f.leg].points.len();
                    let reach = if to_goal { (to_target - f.goal_radius).max(0.0) } else { to_target };
                    let step = (budget * slow).min(reach);
                    let start = self.feet;
                    let mut p = [start[0] + dir[0] * step, start[1] + dir[1] * step, start[2]];
                    // Height: the navmesh under the new point, else along the segment.
                    let near = self.tri_near(nav, p);
                    p[2] = match near {
                        Some(t) => {
                            self.tri = t;
                            nav.height_on(t, p[0], p[1])
                        }
                        None => start[2] + (target[2] - start[2]) * (step / to_target.max(1e-4)),
                    };
                    self.feet = p;
                    self.travelled += step;
                    budget -= step / slow;
                    moved = true;
                }
            }
        }
        if moved {
            WalkEvent::Moved
        } else {
            WalkEvent::Idle
        }
    }

    /// The triangle under `p`: the current one or one of its neighbours (two rings),
    /// else a search of the space.
    fn tri_near(&self, nav: &NavGraph, p: [f32; 3]) -> Option<u32> {
        if self.tri != NONE {
            let mut ring = vec![self.tri];
            for _ in 0..2 {
                if let Some(&t) = ring.iter().find(|&&t| nav.contains_xy(t, p[0], p[1])) {
                    return Some(t);
                }
                let mut next = Vec::new();
                for &t in &ring {
                    next.extend(nav.tris[t as usize].next.iter().copied().filter(|&n| n != NONE));
                }
                ring = next;
            }
            if let Some(&t) = ring.iter().find(|&&t| nav.contains_xy(t, p[0], p[1])) {
                return Some(t);
            }
        }
        nav.locate(self.space, p, 128.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Leg;

    fn path(points: Vec<[f32; 3]>) -> Path {
        Path { legs: vec![Leg { space: Space::Interior(1), points, corridor: vec![], door: None, door_stops: vec![], hops: vec![] }], length: 0.0, expanded: 0, start_snap: 0.0, goal_snap: 0.0 }
    }

    #[test]
    fn follower_walks_corners_in_order() {
        let mut f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]));
        match f.steer([0.0, 0.0, 0.0]) {
            Steer::Walk { target, dir, remaining, .. } => {
                assert_eq!(target, [100.0, 0.0, 0.0]);
                assert_eq!(dir, [1.0, 0.0]);
                assert!((remaining - 200.0).abs() < 1e-3);
            }
            s => panic!("{s:?}"),
        }
        // Within the arrival radius of the corner: head for the end.
        match f.steer([95.0, 0.0, 0.0]) {
            Steer::Walk { target, .. } => assert_eq!(target, [100.0, 100.0, 0.0]),
            s => panic!("{s:?}"),
        }
        assert_eq!(f.steer([100.0, 99.0, 0.0]), Steer::Arrived);
    }

    #[test]
    fn passing_a_corner_counts() {
        let mut f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]));
        // Overshot the corner by 30 units, 30 units from it.
        match f.steer([130.0, 0.0, 0.0]) {
            Steer::Walk { target, .. } => assert_eq!(target, [100.0, 100.0, 0.0]),
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn unstuck_detours_then_replans_then_gives_up() {
        let mut u = Unstuck::new(100.0);
        u.max_replans = 1;
        let dt = 0.1;
        // Moving well: no advice beyond the path's direction.
        for i in 0..20 {
            assert_eq!(u.advise(1000.0 - i as f32 * 10.0, [1.0, 0.0], dt), Advice::Head([1.0, 0.0]));
        }
        // Blocked 800 from the end: after a second, the four detours in turn, each 0.6 s.
        let mut turns = Vec::new();
        let mut advice = Vec::new();
        for _ in 0..200 {
            let a = u.advise(800.0, [1.0, 0.0], dt);
            if let Advice::Head(d) = a {
                if d != [1.0, 0.0] && turns.last() != Some(&d) {
                    turns.push(d);
                }
            } else {
                advice.push(a);
            }
            if advice.contains(&Advice::GiveUp) {
                break;
            }
        }
        assert_eq!(turns.len(), 8, "{turns:?}"); // four detours, twice (before and after the replan)
        assert!(turns[0][1] > 0.9 && turns[1][1] < -0.9, "left first, then right: {turns:?}");
        assert_eq!(advice, vec![Advice::Replan, Advice::GiveUp]);
        assert_eq!((u.detours, u.replans), (8, 1));
    }

    #[test]
    fn unstuck_with_skip_advises_skip_instead_of_replan() {
        let mut u = Unstuck::new(100.0);
        u.skip = true;
        let mut advice = Vec::new();
        for _ in 0..200 {
            match u.advise(800.0, [1.0, 0.0], 0.1) {
                Advice::Head(_) => {}
                a => {
                    advice.push(a);
                    break;
                }
            }
        }
        assert_eq!(advice, vec![Advice::Skip]);
        assert_eq!((u.detours, u.replans), (4, 1));
    }

    #[test]
    fn after_a_skip_a_still_stuck_actor_is_moved_on_again_at_once() {
        // Stuck for good: the first skip comes after a second and four detours.
        let first = |u: &mut Unstuck| (1..400).find(|_| u.advise(800.0, [1.0, 0.0], 0.1) == Advice::Skip).unwrap();
        let mut u = Unstuck::new(100.0);
        u.skip = true;
        let steps = first(&mut u);
        assert!(u.skipped && steps > 60, "{steps}");
        // Put down further on and stuck again: moved on after a second without progress.
        let mut again = Unstuck::new(100.0);
        again.skip = true;
        again.after_skip();
        let steps = first(&mut again);
        assert!(steps <= 11 && again.detours == 0, "{steps} {}", again.detours);
        // Getting on after the skip brings the detours back.
        let mut free = Unstuck::new(100.0);
        free.skip = true;
        free.after_skip();
        for i in 0..20 {
            assert_eq!(free.advise(800.0 - i as f32 * 10.0, [1.0, 0.0], 0.1), Advice::Head([1.0, 0.0]));
        }
        let steps = first(&mut free);
        assert!(free.detours == 4 && steps > 60, "{steps} {}", free.detours);
    }

    #[test]
    fn unstuck_by_motion_is_not_fooled_by_a_receding_target() {
        let mut u = Unstuck::new(100.0);
        // Walking after a target that keeps 500 ahead: moving at full speed is progress.
        for i in 0..100 {
            assert_eq!(u.advise_moved([i as f32 * 10.0, 0.0, 0.0], [1.0, 0.0], 0.1), Advice::Head([1.0, 0.0]));
        }
        // Sliding across the way (along a fence) is not.
        let mut detoured = false;
        for i in 0..30 {
            if u.advise_moved([1000.0, i as f32 * 10.0, 0.0], [1.0, 0.0], 0.1) != Advice::Head([1.0, 0.0]) {
                detoured = true;
            }
        }
        assert!(detoured);
        assert!(u.detours >= 1);
    }

    #[test]
    fn skip_ahead_moves_on_along_the_path() {
        let mut f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 10.0], [100.0, 300.0, 40.0]]));
        // Stuck 60 units along the first stretch: 160 on is 40 units past the corner,
        // which counts as reached; the height is the path's.
        let to = f.skip_ahead([60.0, 0.0, 6.0], 160.0).unwrap();
        assert!((to[0] - 100.0).abs() < 1e-3 && (to[1] - 120.0).abs() < 1e-3 && (to[2] - 22.0).abs() < 1e-3, "{to:?}");
        assert_eq!(f.next, 2);
        // Near the end: no further than the end, which then counts as arrived at.
        let to = f.skip_ahead([100.0, 250.0, 35.0], 160.0).unwrap();
        assert_eq!(to, [100.0, 300.0, 40.0]);
        assert_eq!(f.steer(to), Steer::Arrived);
    }

    #[test]
    fn skip_ahead_stops_at_a_door_stop() {
        let mut p = path(vec![[0.0, 0.0, 0.0], [50.0, 0.0, 0.0], [400.0, 0.0, 0.0]]);
        p.legs[0].door_stops = vec![crate::path::DoorStop { door: 7, point: 1 }];
        let mut f = Follower::new(p);
        let to = f.skip_ahead([10.0, 0.0, 0.0], 160.0).unwrap();
        assert_eq!(to, [50.0, 0.0, 0.0]);
        assert!(matches!(f.steer(to), Steer::OpenDoor { door: 7, .. }));
        // Held at the stop: no skipping through the door.
        assert_eq!(f.skip_ahead(to, 160.0), None);
    }

    /// The reproduction of #1501: feet beside a straight path.
    #[test]
    fn skip_ahead_from_feet_off_the_path_stays_on_it() {
        let line = || path(vec![[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0]]);
        assert_eq!(Follower::new(line()).skip_ahead([0.0, 0.0, 0.0], 160.0), Some([160.0, 0.0, 0.0]));
        assert_eq!(Follower::new(line()).skip_ahead([0.0, 100.0, 0.0], 160.0), Some([160.0, 0.0, 0.0]));
    }

    #[test]
    fn skip_ahead_goes_round_a_corner_from_the_projection() {
        let mut f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 300.0, 0.0]]));
        // 40 units to the corner from (60, 0), then 120 up the next stretch.
        assert_eq!(f.skip_ahead([60.0, 30.0, 0.0], 160.0), Some([100.0, 120.0, 0.0]));
        assert_eq!(f.next, 2);
    }

    #[test]
    fn a_path_that_comes_back_is_not_jumped_along() {
        // A loop passing back 20 units from the start: projected on the nearest stretch of
        // them all, these feet would be 900 units along.
        let mut f = Follower::new(path(vec![[0.0, 0.0, 0.0], [200.0, 0.0, 0.0], [200.0, 200.0, 0.0], [0.0, 200.0, 0.0], [0.0, 20.0, 0.0], [300.0, 20.0, 0.0]]));
        assert_eq!(f.skip_ahead([10.0, 18.0, 0.0], 160.0), Some([170.0, 0.0, 0.0]));
        assert_eq!(f.next, 1);
    }

    #[test]
    fn skip_candidates_change_nothing_until_committed() {
        let f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 300.0, 0.0]]));
        let before = f.clone();
        let cs = f.skip_candidates([60.0, 30.0, 0.0], &[160.0, 120.0, 80.0, 40.0, 0.0]);
        assert_eq!(f, before);
        let want = [[100.0, 120.0], [100.0, 80.0], [100.0, 40.0], [100.0, 0.0], [60.0, 0.0]];
        assert_eq!(cs.len(), want.len());
        for (c, w) in cs.iter().zip(want) {
            assert!(hdist(c.pos, [w[0], w[1], 0.0]) < 1e-3, "{c:?} {w:?}");
        }
        assert_eq!(cs.iter().map(|c| c.next).collect::<Vec<_>>(), vec![2, 2, 2, 2, 1]);
        // Refused: as it was. Accepted: the corners it passed count as reached.
        let mut g = f.clone();
        g.commit_skip(&cs[4]);
        assert_eq!(g, before);
        g.commit_skip(&cs[0]);
        assert_eq!(g.next, 2);
        // The same as moving on in one step.
        let mut h = f.clone();
        assert_eq!(h.skip_ahead([60.0, 30.0, 0.0], 160.0), Some(cs[0].pos));
        assert_eq!(h, g);
    }

    #[test]
    fn no_distance_on_counts_only_for_feet_off_the_path() {
        let f = Follower::new(path(vec![[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0]]));
        let ladder = [40.0, 0.0];
        assert_eq!(f.skip_candidates([100.0, 5.0, 0.0], &ladder).len(), 1, "as good as on the path");
        assert_eq!(f.skip_candidates([100.0, 16.0, 0.0], &ladder).len(), 1, "within the arrival radius");
        let off = f.skip_candidates([100.0, 40.0, 0.0], &ladder);
        assert_eq!(off.len(), 2);
        assert_eq!(off[1].pos, [100.0, 0.0, 0.0]);
    }

    #[test]
    fn skip_candidates_stop_at_a_door_stop_or_the_legs_end_once() {
        let ladder = [160.0, 120.0, 80.0, 40.0, 0.0];
        // 60 units to the stop: the far places are all the stop, and it is not repeated.
        let mut p = path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [400.0, 0.0, 0.0]]);
        p.legs[0].door_stops = vec![crate::path::DoorStop { door: 7, point: 1 }];
        let f = Follower::new(p);
        let cs = f.skip_candidates([40.0, 0.0, 0.0], &ladder);
        assert_eq!(cs.iter().map(|c| c.pos[0]).collect::<Vec<_>>(), vec![100.0, 80.0]);
        assert!(cs[0].at_limit && !cs[1].at_limit);
        assert_eq!(cs[0].along, 60.0);
        let mut g = f.clone();
        g.commit_skip(&cs[0]);
        assert!(matches!(g.steer(cs[0].pos), Steer::OpenDoor { door: 7, .. }));
        // At the end of a leg with a load door after it, the next leg is not touched.
        let mut p = path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]]);
        let again = p.legs[0].clone();
        p.legs.push(again);
        let f = Follower::new(p);
        let cs = f.skip_candidates([95.0, 0.0, 0.0], &ladder);
        assert_eq!(cs.iter().map(|c| c.pos[0]).collect::<Vec<_>>(), vec![100.0]);
        assert_eq!((cs[0].next, f.leg), (1, 0));
        // Standing at the limit with no way on: still offered, steering then ends the walk.
        let f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]]));
        let cs = f.skip_candidates([100.0, 0.0, 0.0], &ladder);
        assert_eq!(cs.iter().map(|c| c.pos).collect::<Vec<_>>(), vec![[100.0, 0.0, 0.0]]);
    }

    #[test]
    fn nothing_to_skip_along_answers_without_trouble() {
        let ladder = [160.0, 0.0];
        // No points at all, no leg left.
        let f = Follower::new(path(vec![]));
        assert_eq!(f.skip_candidate([0.0; 3], 160.0), None);
        assert!(f.skip_candidates([0.0; 3], &ladder).is_empty());
        let mut f = Follower::new(path(vec![[0.0; 3], [100.0, 0.0, 0.0]]));
        f.leg = 1;
        assert_eq!(f.skip_candidate([0.0; 3], 160.0), None);
        // A path of one point, and corners used up: that point, with the cursor kept.
        let mut f = Follower::new(path(vec![[5.0, 6.0, 7.0]]));
        assert_eq!(f.skip_ahead([0.0; 3], 160.0), Some([5.0, 6.0, 7.0]));
        assert_eq!(f.next, 1);
        let mut f = Follower::new(path(vec![[0.0; 3], [100.0, 0.0, 0.0]]));
        f.next = 2;
        assert_eq!(f.skip_ahead([90.0, 0.0, 0.0], 160.0), Some([100.0, 0.0, 0.0]));
        assert_eq!(f.next, 2);
        // Points on the same spot: no division by nothing, and on past them.
        let mut f = Follower::new(path(vec![[0.0; 3], [0.0; 3], [0.0; 3], [300.0, 0.0, 0.0]]));
        let to = f.skip_ahead([0.0, 50.0, 0.0], 160.0).unwrap();
        assert!(hdist(to, [160.0, 0.0, 0.0]) < 1e-3, "{to:?}");
        assert_eq!(f.next, 3);
        let mut f = Follower::new(path(vec![[0.0; 3], [0.0; 3]]));
        assert_eq!(f.skip_ahead([0.0, 50.0, 0.0], 160.0), Some([0.0; 3]));
        // Numbers that are not finite.
        let f = Follower::new(path(vec![[0.0; 3], [100.0, 0.0, 0.0]]));
        assert_eq!(f.skip_candidate([f32::NAN, 0.0, 0.0], 160.0), None);
        assert_eq!(f.skip_candidate([0.0, f32::INFINITY, 0.0], 160.0), None);
        assert_eq!(f.skip_candidate([0.0; 3], f32::NAN), None);
        assert_eq!(f.skip_candidate([0.0; 3], f32::INFINITY), None);
        assert_eq!(f.skip_candidates([0.0; 3], &[f32::NAN, 40.0]).len(), 1);
        let f = Follower::new(path(vec![[0.0; 3], [f32::NAN, 0.0, 0.0]]));
        assert_eq!(f.skip_candidate([0.0; 3], 160.0), None);
        // Backwards is not on.
        let f = Follower::new(path(vec![[0.0; 3], [100.0, 0.0, 0.0]]));
        assert_eq!(f.skip_candidate([50.0, 0.0, 0.0], -30.0).map(|c| c.pos), Some([50.0, 0.0, 0.0]));
    }

    #[test]
    fn a_door_stop_not_opened_is_never_skipped_past() {
        // Held there, or with it behind the corner walked to (steering holds): no place.
        let mut p = path(vec![[0.0, 0.0, 0.0], [50.0, 0.0, 0.0], [400.0, 0.0, 0.0]]);
        p.legs[0].door_stops = vec![crate::path::DoorStop { door: 7, point: 1 }];
        let mut f = Follower::new(p);
        f.next = 2;
        assert_eq!(f.skip_candidate([60.0, 0.0, 0.0], 160.0), None);
        // Opened: on through it.
        f.holding = true;
        f.door_opened();
        assert_eq!(f.skip_ahead([60.0, 0.0, 0.0], 160.0), Some([220.0, 0.0, 0.0]));
    }

    #[test]
    fn a_refused_skip_is_advised_again_after_a_second_whatever_the_number_of_them() {
        let mut u = Unstuck::new(100.0);
        u.skip = true;
        let dt = 0.05;
        let seconds_to_skip = |u: &mut Unstuck| {
            for i in 1..400 {
                match u.advise_moved([0.0, 0.0, 0.0], [1.0, 0.0], dt) {
                    Advice::Skip => return i as f32 * dt,
                    Advice::GiveUp => panic!("gave up"),
                    _ => {}
                }
            }
            panic!("no skip in 20 s");
        };
        // The four detours and their seconds first, as ever.
        assert!(seconds_to_skip(&mut u) > 5.0);
        assert!(u.skipped);
        // More refusals than `max_replans` allows skips.
        for _ in 0..20 {
            u.skip_refused();
            assert_eq!((u.skipped, u.replans), (false, 0));
            let t = seconds_to_skip(&mut u);
            assert!((0.95..1.2).contains(&t), "{t}");
        }
    }

    #[test]
    fn turning_takes_the_short_way() {
        use std::f32::consts::PI;
        let h = turn_towards(0.9 * PI, -0.9 * PI, 0.1);
        assert!((h - (0.9 * PI + 0.1)).abs() < 1e-5 || (h - (0.9 * PI + 0.1 - 2.0 * PI)).abs() < 1e-5, "{h}");
        assert!((turn_towards(0.0, 0.05, 1.0) - 0.05).abs() < 1e-6);
        assert!((heading_of([1.0, 0.0]) - PI / 2.0).abs() < 1e-6);
        assert!(heading_of([0.0, 1.0]).abs() < 1e-6);
        assert!((heading_delta(0.9 * PI, -0.9 * PI) - 0.2 * PI).abs() < 1e-5);
        assert!((heading_delta(0.1, -0.1) + 0.2).abs() < 1e-6);
    }

    /// A walker on a path with no navmesh under it (heights come from the path).
    fn walker(heading: f32, points: Vec<[f32; 3]>) -> (NavGraph, Walker) {
        let mut w = Walker::new(Space::Interior(1), points[0], heading, 100.0);
        w.follower = Some(Follower::new(path(points)));
        (NavGraph::default(), w)
    }

    #[test]
    fn a_standing_walker_turns_on_the_spot_before_it_sets_off() {
        use std::f32::consts::{FRAC_PI_2, PI};
        // Facing north, the way lies east: a quarter turn to the right at 120°/s.
        let (g, mut w) = walker(0.0, vec![[0.0, 0.0, 0.0], [200.0, 0.0, 0.0]]);
        let dt = 0.05;
        let mut turned = 0;
        loop {
            let before = w.heading;
            match w.update(&g, dt) {
                WalkEvent::Turned => {
                    turned += 1;
                    assert_eq!(w.feet, [0.0, 0.0, 0.0], "it stands while it turns");
                    let step = heading_delta(before, w.heading);
                    assert!(step > 0.0 && step <= SPOT_TURN_RATE * dt + 1e-5, "{step}");
                }
                WalkEvent::Moved => break,
                e => panic!("{e:?}"),
            }
            assert!(turned < 100);
        }
        // 90° at 120°/s is 0.75 s: fifteen steps of 0.05 s (one more for rounding).
        assert!((15..=16).contains(&turned), "{turned}");
        assert!((w.heading - FRAC_PI_2).abs() < 1e-4 && !w.turning && w.moving);
        // The other way round it turns left.
        let (g, mut w) = walker(PI, vec![[0.0, 0.0, 0.0], [200.0, 0.0, 0.0]]);
        assert_eq!(w.update(&g, dt), WalkEvent::Turned);
        assert!(heading_delta(PI, w.heading) < 0.0);
        // Nearly facing the way (within 20°): it sets off at once.
        let (g, mut w) = walker(FRAC_PI_2 - 0.3, vec![[0.0, 0.0, 0.0], [200.0, 0.0, 0.0]]);
        assert_eq!(w.update(&g, dt), WalkEvent::Moved);
        // It turns on as it walks, at the walking rate.
        assert!((w.heading - (FRAC_PI_2 - 0.3 + WALK_TURN_RATE * dt)).abs() < 1e-4, "{}", w.heading);
    }

    #[test]
    fn a_walker_on_its_way_takes_corners_without_stopping() {
        // East, then a right angle north: walking, it steers round the corner.
        let (g, mut w) = walker(std::f32::consts::FRAC_PI_2, vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]);
        let mut events = Vec::new();
        for _ in 0..100 {
            let ev = w.update(&g, 0.05);
            events.push(ev);
            if ev == WalkEvent::Arrived {
                break;
            }
        }
        assert_eq!(events.last(), Some(&WalkEvent::Arrived));
        assert!(!events.contains(&WalkEvent::Turned), "{events:?}");
        // (It cuts the corner within the arrival radius, so it ends a little east of north.)
        assert!(w.heading.abs() < 0.2 && !w.moving, "{}", w.heading);
    }

    #[test]
    fn a_walker_turns_at_most_at_the_walking_rate_and_slows_to_turn() {
        use std::f32::consts::FRAC_PI_2;
        // East, then a right angle north, at 100 units per second in steps of 1/60 s.
        let (g, mut w) = walker(FRAC_PI_2, vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 200.0, 0.0]]);
        let dt = 1.0 / 60.0;
        let mut slowest = f32::MAX;
        for _ in 0..600 {
            let (h, at) = (w.heading, w.feet);
            if w.update(&g, dt) == WalkEvent::Arrived {
                break;
            }
            assert!(heading_delta(h, w.heading).abs() <= WALK_TURN_RATE * dt + 1e-5, "turned {} in a step", heading_delta(h, w.heading));
            slowest = slowest.min((w.feet[0] - at[0]).hypot(w.feet[1] - at[1]) / dt);
        }
        // Round the corner it slowed down (it turns a quarter in half a second).
        assert!(slowest < 80.0, "{slowest}");
        assert!(w.follower.is_none() && (w.feet[1] - 200.0).abs() < 20.0, "{:?}", w.feet);
    }

    #[test]
    fn the_arc_follows_the_heading_within_a_quarter_turn() {
        use std::f32::consts::{FRAC_PI_2, PI};
        // Facing north, the way east: half a quarter off, it goes along its heading.
        let (d, k) = arc_step(PI / 4.0, [1.0, 0.0]);
        assert!((d[0] - (PI / 4.0).sin()).abs() < 1e-6 && (d[1] - (PI / 4.0).cos()).abs() < 1e-6);
        assert!(k > TURN_SLOW_MIN && k < 1.0);
        // Facing the way: full speed. Facing away: along the way, slowest.
        assert_eq!(arc_step(FRAC_PI_2, [1.0, 0.0]).1, 1.0);
        let (d, k) = arc_step(-FRAC_PI_2, [1.0, 0.0]);
        assert_eq!((d, k), ([1.0, 0.0], TURN_SLOW_MIN));
    }

    #[test]
    fn facing_a_heading_standing() {
        use std::f32::consts::PI;
        let (_, mut w) = walker(0.0, vec![[0.0, 0.0, 0.0], [200.0, 0.0, 0.0]]);
        w.follower = None;
        // Half a turn at 120°/s: 1.5 s.
        let mut t = 0.0f32;
        while !w.face(PI, 0.1) {
            t += 0.1;
            assert!(t < 3.0);
        }
        assert!((t - 1.4).abs() < 0.11, "{t}");
        assert!(heading_delta(w.heading, PI).abs() < 1e-4);
    }

    #[test]
    fn a_walker_flies_over_a_ledge_and_lands_on_the_path() {
        // A drop of 300 between (100, 0) and (200, 0): the path's hop.
        let mut p = path(vec![[0.0, 0.0, 300.0], [100.0, 0.0, 300.0], [200.0, 0.0, 0.0], [400.0, 0.0, 0.0]]);
        p.legs[0].hops = vec![LedgeHop { point: 1, up: false }];
        let mut w = Walker::new(Space::Interior(1), [0.0, 0.0, 300.0], std::f32::consts::FRAC_PI_2, 100.0);
        w.follower = Some(Follower::new(p));
        let g = NavGraph::default();
        let dt = 1.0 / 60.0;
        let (mut in_air, mut lowest_step) = (0, 0.0f32);
        for _ in 0..600 {
            let z = w.feet[2];
            if w.update(&g, dt) == WalkEvent::Arrived {
                break;
            }
            if w.air.is_some() {
                in_air += 1;
                // Falling faster and faster: never a ramp.
                lowest_step = lowest_step.min(w.feet[2] - z);
                assert!(w.feet[0] > 99.0 && w.feet[0] < 201.0, "{:?}", w.feet);
            }
        }
        assert_eq!((w.flights, w.landings), (1, 1));
        // 300 down from a standing start: 0.935 s, 56 steps at 60 a second.
        assert!((54..=57).contains(&in_air), "{in_air}");
        assert!(lowest_step < -9.0, "{lowest_step}");
        assert!(w.follower.is_none() && (w.feet[0] - 400.0).abs() < 2.0 && w.feet[2] == 0.0, "{:?}", w.feet);
    }

    #[test]
    fn a_walker_jumps_up_a_low_ledge() {
        let mut p = path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [130.0, 0.0, 50.0], [300.0, 0.0, 50.0]]);
        p.legs[0].hops = vec![LedgeHop { point: 1, up: true }];
        let mut w = Walker::new(Space::Interior(1), [0.0, 0.0, 0.0], std::f32::consts::FRAC_PI_2, 100.0);
        w.follower = Some(Follower::new(p));
        let g = NavGraph::default();
        let mut top = 0.0f32;
        for _ in 0..600 {
            if w.update(&g, 1.0 / 60.0) == WalkEvent::Arrived {
                break;
            }
            top = top.max(w.feet[2]);
        }
        // Above the ledge (50) by at least the jump's apex (61.6) or the clearance.
        assert!(top > 58.0 && top < 62.0, "{top}");
        assert_eq!((w.flights, w.landings), (1, 1));
        assert_eq!(w.feet, [300.0, 0.0, 50.0]);
    }
}
