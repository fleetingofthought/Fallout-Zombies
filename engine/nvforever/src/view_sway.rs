//! Sway and bob of the first-person arms and weapon (the owner's request, 2026-10-20: the
//! arms and weapon "stay rigid throughout movement").
//!
//! The game's first-person movement clips do not move the arms: the `mt` clips of
//! `characters\_1stperson\locomotion` drive `Bip01` (the root motion, 102.3 units over
//! `mtforward.kf`'s 1.2 s) and leave `Bip01 NonAccum` still, the `1hp` ones add
//! `Camera1st`, `Bip01 Translate` and `Bip01 Rotate` tracks without keys that move them,
//! and the folder has no `2hr`, `2ha` or `2hh` movement clips at all [observed:
//! `fnvtool nif decode` of `mtforward.kf` and `1hpforward.kf`, the folder's file list].
//! The camera node and the arms both hang from `Bip01 Looking`, so whatever moved that
//! chain would move the view with the arms. The executable holds the settings
//! `fFirstPersonHandFollowMult`, `fFirstPersonHandChaseSeconds` and
//! `fFirstPersonHandChaseSecondsAttack` [observed: their names in `FalloutNV.exe`'s
//! strings; not in `FalloutNV.esm` or `Fallout_default.ini`], which reads as the hands
//! chasing the view after a while [hypothesis: their meaning from the names; their
//! values are unknown]. So the sway is the engine's own, and this one is NVForever's
//! [design; every constant below an engineering choice, tuned by eye]:
//!
//! - **Turn lag**: the arms keep the way they pointed and catch the view up over
//!   [`CHASE_SECONDS`], so a turn leaves them behind by the turn rate × that time, up to
//!   [`MAX_LAG_YAW`] and [`MAX_LAG_PITCH`]; a sideways lag tilts the weapon a little.
//! - **Walking bob**: two dips a stride, one at each footfall, and one sideways swing
//!   a stride, at the phase of the movement clips the arms rig plays (their footfalls,
//!   `m:L` and `m:R`, fall about half a cycle apart: 0.40 s and 1.03 s of
//!   `mtforward.kf`'s 1.2 s [observed: audio.md]). It grows with the ground speed up to
//!   a run's, less sneaking, little down the sights, none in the air.
//! - **Movement lag**: starting, stopping and strafing leave the weapon a little behind
//!   the body and lean it into a strafe.
//!
//! The pose turns and moves `Bip01 Translate` (the node above the arms, under `Bip01
//! Looking`) about the eye, in the view's own frame, so the camera node (`Camera1st`,
//! beside it under `Bip01 Looking`) and the view stay where they were.

use bevy::math::{Quat, Vec2, Vec3};

/// Seconds the arms take to catch up with the view, as a time constant.
pub const CHASE_SECONDS: f32 = 0.11;
/// Most the arms fall behind a turn: sideways and up or down, radians.
pub const MAX_LAG_YAW: f32 = 5.0 * std::f32::consts::PI / 180.0;
pub const MAX_LAG_PITCH: f32 = 3.5 * std::f32::consts::PI / 180.0;
/// Weapon tilt per radian of sideways lag.
const LAG_ROLL: f32 = 0.6;
/// The ground speed (units per second) at which the bob is full: about a run
/// [hypothesis: the player's walk is 85 units/s, `mtforward.kf`'s root motion].
const FULL_BOB_SPEED: f32 = 300.0;
/// The full bob: the dip, the sideways swing (units, at the eye's distance) and the
/// tilt with the swing (radians).
const BOB_DIP: f32 = 0.9;
const BOB_SWAY: f32 = 0.6;
const BOB_ROLL: f32 = 1.2 * std::f32::consts::PI / 180.0;
/// The first footfall's place in the movement cycle (0.40 s of 1.2 s).
pub const FOOTFALL_PHASE: f32 = 0.33;
/// Movement lag: how far behind the velocity the weapon's own trails (seconds), units
/// of offset per unit/s of difference, and most offset.
const MOVE_LAG_SECONDS: f32 = 0.18;
const MOVE_LAG_PER_SPEED: f32 = 0.006;
const MAX_MOVE_LAG: f32 = 1.2;
/// Tilt into a strafe at a run's sideways speed, radians.
const STRAFE_ROLL: f32 = 1.5 * std::f32::consts::PI / 180.0;
/// Shares down the sights and sneaking.
const SIGHTS_SWAY: f32 = 0.35;
const SIGHTS_BOB: f32 = 0.2;
const SNEAK_BOB: f32 = 0.6;
/// Seconds to fade the whole effect in or out (Pip-Boy, idles, the switch).
const FADE_SECONDS: f32 = 0.15;

/// What the sway reads each frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct SwayInput {
    pub dt: f32,
    /// The view's compass heading (clockwise) and pitch (up), radians.
    pub heading: f32,
    pub pitch: f32,
    /// Ground velocity in the view's frame: x to the right, y ahead (units/s).
    pub velocity: Vec2,
    /// Where the movement clips are in their cycle (0–1).
    pub phase: f32,
    pub sights: bool,
    pub sneaking: bool,
    pub airborne: bool,
    /// Whether the arms may sway at all this frame.
    pub on: bool,
}

/// The pose to put on the arms: a turn and a move in the view's frame (x right, y ahead,
/// z up) about the eye.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwayPose {
    pub rotation: Quat,
    pub offset: Vec3,
}

impl SwayPose {
    pub const NONE: SwayPose = SwayPose { rotation: Quat::IDENTITY, offset: Vec3::ZERO };
}

/// The sway's state across frames.
#[derive(Debug, Clone, Default)]
pub struct ViewSway {
    last: Option<(f32, f32)>,
    /// How far the arms are behind the view: heading and pitch, radians (arms − view).
    lag: Vec2,
    /// The weapon's trailing velocity (view frame).
    trail: Vec2,
    /// How much of the bob shows (follows the speed).
    bob: f32,
    /// How much of everything shows.
    gain: f32,
    /// The most turn lag (sideways, up or down) and bob dip shown since the start (for
    /// the exit summary).
    pub peak_lag: Vec2,
    pub peak_dip: f32,
}

/// `x` towards `max` and never past it, smoothly.
fn soft_clamp(x: f32, max: f32) -> f32 {
    max * (x / max).tanh()
}

/// `a - b` as the shorter way round, radians.
fn angle_diff(a: f32, b: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (a - b + PI).rem_euclid(TAU) - PI
}

impl ViewSway {
    pub fn update(&mut self, i: SwayInput) -> SwayPose {
        let dt = i.dt.max(0.0);
        let follow = |t: f32| if dt <= 0.0 { 0.0 } else { 1.0 - (-dt / t).exp() };
        self.gain += (f32::from(u8::from(i.on)) - self.gain) * follow(FADE_SECONDS);
        // Turn lag: the view turned by `d`, the arms not yet; they then close the gap.
        // A jump of the view (a teleport, an idle letting go) starts afresh.
        if let Some((h, p)) = self.last {
            let d = Vec2::new(angle_diff(i.heading, h), i.pitch - p);
            if d.length() < 0.5 {
                self.lag -= d;
            }
        }
        self.last = Some((i.heading, i.pitch));
        self.lag *= 1.0 - follow(CHASE_SECONDS);
        self.lag = Vec2::new(soft_clamp(self.lag.x, MAX_LAG_YAW), soft_clamp(self.lag.y, MAX_LAG_PITCH));
        // Movement: the bob follows the speed; the trailing velocity follows the velocity.
        let speed = i.velocity.length();
        let bob_target = if i.airborne { 0.0 } else { (speed / FULL_BOB_SPEED).min(1.0) };
        self.bob += (bob_target - self.bob) * follow(0.2);
        self.trail += (i.velocity - self.trail) * follow(MOVE_LAG_SECONDS);

        let g = self.gain;
        let sway_share = g * if i.sights { SIGHTS_SWAY } else { 1.0 };
        let bob_share = g * self.bob * if i.sights { SIGHTS_BOB } else { 1.0 } * if i.sneaking { SNEAK_BOB } else { 1.0 };
        let lag = self.lag * sway_share;
        // Heading grows clockwise, a turn about +z anticlockwise: arms behind a turn to
        // the right point left of it.
        let turn = Quat::from_rotation_z(-lag.x) * Quat::from_rotation_x(lag.y);
        let a = std::f32::consts::TAU * (i.phase - FOOTFALL_PHASE);
        // Down at each footfall (twice a cycle), across once a cycle.
        let dip = -BOB_DIP * 0.5 * (1.0 + (2.0 * a).cos()) * bob_share;
        let swing = a.sin();
        let lean = (i.velocity.x / FULL_BOB_SPEED).clamp(-1.0, 1.0);
        let roll = lag.x * LAG_ROLL + BOB_ROLL * swing * bob_share + STRAFE_ROLL * lean * sway_share;
        let behind = i.velocity - self.trail;
        let behind = behind * MOVE_LAG_PER_SPEED;
        let behind = if behind.length() > MAX_MOVE_LAG { behind.normalize() * MAX_MOVE_LAG } else { behind };
        let offset = Vec3::new(BOB_SWAY * swing * bob_share - behind.x * sway_share, -behind.y * sway_share, dip);
        self.peak_lag = self.peak_lag.max(lag.abs());
        self.peak_dip = self.peak_dip.max(-dip);
        SwayPose { rotation: turn * Quat::from_rotation_y(roll), offset }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(heading: f32, velocity: Vec2, phase: f32) -> SwayInput {
        SwayInput { dt: 1.0 / 60.0, heading, pitch: 0.0, velocity, phase, sights: false, sneaking: false, airborne: false, on: true }
    }

    fn settle(s: &mut ViewSway, i: SwayInput, frames: usize) -> SwayPose {
        let mut p = SwayPose::NONE;
        for _ in 0..frames {
            p = s.update(i);
        }
        p
    }

    #[test]
    fn standing_still_rests() {
        let mut s = ViewSway::default();
        let p = settle(&mut s, input(1.0, Vec2::ZERO, 0.0), 120);
        assert!(p.offset.length() < 1e-4 && p.rotation.angle_between(Quat::IDENTITY) < 1e-4, "{p:?}");
    }

    #[test]
    fn turning_right_leaves_the_arms_to_the_left_and_they_catch_up() {
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        // A steady turn to the right at 1 rad/s.
        let mut h = 0.0;
        let mut p = SwayPose::NONE;
        for _ in 0..60 {
            h += 1.0 / 60.0;
            p = s.update(input(h, Vec2::ZERO, 0.0));
        }
        let ahead = p.rotation * Vec3::Y;
        assert!(ahead.x < -0.01, "the weapon points left of the view: {ahead:?}");
        assert!(s.lag.x.abs() <= MAX_LAG_YAW + 1e-6);
        let p = settle(&mut s, input(h, Vec2::ZERO, 0.0), 120);
        assert!(p.rotation.angle_between(Quat::IDENTITY) < 1e-3, "back on the view: {p:?}");
    }

    #[test]
    fn a_fast_flick_stays_within_the_limit() {
        let mut s = ViewSway::default();
        let mut h = 0.0;
        s.update(input(h, Vec2::ZERO, 0.0));
        for _ in 0..30 {
            h += 0.3;
            s.update(input(h, Vec2::ZERO, 0.0));
        }
        assert!(s.lag.x.abs() <= MAX_LAG_YAW + 1e-6 && s.lag.x.abs() > MAX_LAG_YAW * 0.9, "{}", s.lag.x);
    }

    #[test]
    fn walking_dips_at_the_footfalls() {
        let mut s = ViewSway::default();
        let v = Vec2::new(0.0, 300.0);
        settle(&mut s, input(0.0, v, FOOTFALL_PHASE), 120);
        let at_footfall = s.update(input(0.0, v, FOOTFALL_PHASE)).offset.z;
        let between = s.update(input(0.0, v, FOOTFALL_PHASE + 0.25)).offset.z;
        let other = s.update(input(0.0, v, FOOTFALL_PHASE + 0.5)).offset.z;
        assert!(at_footfall < -0.5 * BOB_DIP && other < -0.5 * BOB_DIP, "{at_footfall} {other}");
        assert!(between > -0.05, "{between}");
    }

    #[test]
    fn the_sights_and_the_air_calm_it() {
        let v = Vec2::new(0.0, 300.0);
        let mut free = ViewSway::default();
        let mut sights = ViewSway::default();
        let mut air = ViewSway::default();
        let a = settle(&mut free, input(0.0, v, FOOTFALL_PHASE), 120).offset.z;
        let b = settle(&mut sights, SwayInput { sights: true, ..input(0.0, v, FOOTFALL_PHASE) }, 120).offset.z;
        let c = settle(&mut air, SwayInput { airborne: true, ..input(0.0, v, FOOTFALL_PHASE) }, 120).offset.z;
        assert!(b > a * 0.5 && b < 0.0, "{a} {b}");
        assert!(c.abs() < 1e-3, "{c}");
    }

    #[test]
    fn switched_off_it_fades_out() {
        let mut s = ViewSway::default();
        let v = Vec2::new(0.0, 300.0);
        settle(&mut s, input(0.0, v, FOOTFALL_PHASE), 120);
        let p = settle(&mut s, SwayInput { on: false, ..input(0.0, v, FOOTFALL_PHASE) }, 120);
        assert!(p.offset.length() < 1e-3, "{p:?}");
    }
}
