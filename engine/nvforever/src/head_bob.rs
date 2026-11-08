//! The camera's bob while walking: a head bob in first person, none in third (owner,
//! 2026-11-07: "we had headbob in third person, but none in first person? it should be
//! the other way around").
//!
//! - **Before**: the first-person camera sat on the arms rig's camera node, which the
//!   game's first-person movement clips never move (`view_sway.rs` module notes), so
//!   only the arms and weapon bobbed. The third-person camera's pivot was the walking
//!   body's head bone, which rises and falls with every stride.
//! - **First person** [design; every constant an engineering choice]: the whole arms rig
//!   (camera node, arms and weapon together) dips at each footfall and swings a little
//!   sideways once a stride, in step with the movement clip the arms rig plays (the
//!   footfall phase of `view_sway`), growing with the ground speed up to a run's, less
//!   sneaking, little down the sights, none in the air. A switch of the settings page
//!   (`head-bob`, `--no-head-bob`).
//! - **Third person**: the pivot's height above the feet is the head's, smoothed by two
//!   first-order lags of [`PIVOT_SECONDS`] each: a stride's rise and fall (about 1.7 a
//!   second at a run) is cut to about an eighth, and crouching or standing up still
//!   lowers or raises the camera, 90 % of the way in a second.

use bevy::math::Vec3;

/// The ground speed (units per second) at which the bob is full: about a run.
const FULL_SPEED: f32 = 300.0;
/// The full bob: the dip at each footfall and the sideways swing, units.
pub const DIP: f32 = 1.6;
pub const SWING: f32 = 0.5;
/// Share of the bob down the sights and sneaking.
const SIGHTS: f32 = 0.25;
const SNEAK: f32 = 0.6;
/// Seconds the bob takes to follow the speed, as a time constant.
const FOLLOW_SECONDS: f32 = 0.2;
/// Each of the third-person pivot's two lags, seconds.
pub const PIVOT_SECONDS: f32 = 0.25;
/// A change of the head's height above the feet over this (units) in one frame is not a
/// stride: the pivot goes there at once [design: a teleport, a load, a ragdoll getting up].
const PIVOT_SNAP: f32 = 40.0;

/// What one frame's first-person bob depends on.
#[derive(Debug, Clone, Copy)]
pub struct BobInput {
    pub dt: f32,
    /// Ground speed, units per second.
    pub speed: f32,
    /// The movement clip's phase in its cycle, 0..1.
    pub phase: f32,
    /// The view's compass heading (clockwise from +y), radians.
    pub heading: f32,
    pub sights: bool,
    pub sneaking: bool,
    pub airborne: bool,
    /// The bob shows at all (first person, the switch on, nothing else driving the view).
    pub on: bool,
}

/// The bob's state across frames.
#[derive(Debug, Clone, Default)]
pub struct HeadBob {
    amount: f32,
    gain: f32,
    /// The third-person pivot's two lags (head height above the feet).
    pivot: Option<(f32, f32)>,
    /// The deepest dip shown since the start (for the exit summary).
    pub peak_dip: f32,
    /// Third person, for the exit summary: the head's and the pivot's height above the
    /// feet last frame, and the sums of their squared changes per frame over `frames`.
    last: Option<(f32, f32)>,
    head_moves: f64,
    pivot_moves: f64,
    pub frames: u32,
}

impl HeadBob {
    /// The first-person rig's offset this frame, game space.
    pub fn first_person(&mut self, i: BobInput) -> Vec3 {
        let dt = i.dt.max(0.0);
        let follow = |t: f32| if dt <= 0.0 { 0.0 } else { 1.0 - (-dt / t).exp() };
        self.gain += (f32::from(u8::from(i.on)) - self.gain) * follow(FOLLOW_SECONDS);
        let target = if i.airborne { 0.0 } else { (i.speed / FULL_SPEED).min(1.0) };
        self.amount += (target - self.amount) * follow(FOLLOW_SECONDS);
        let share = self.gain * self.amount * if i.sights { SIGHTS } else { 1.0 } * if i.sneaking { SNEAK } else { 1.0 };
        let a = std::f32::consts::TAU * (i.phase - crate::view_sway::FOOTFALL_PHASE);
        // Down at each footfall (twice a cycle), across once a cycle.
        let dip = -DIP * 0.5 * (1.0 + (2.0 * a).cos()) * share;
        let swing = SWING * a.sin() * share;
        self.peak_dip = self.peak_dip.max(-dip);
        // The view's right: heading clockwise from +y.
        let right = Vec3::new(i.heading.cos(), -i.heading.sin(), 0.0);
        right * swing + Vec3::Z * dip
    }

    /// The third-person pivot's height above the feet this frame, from the head's.
    pub fn pivot_height(&mut self, head: f32, dt: f32) -> f32 {
        let k = if dt <= 0.0 { 0.0 } else { 1.0 - (-dt / PIVOT_SECONDS).exp() };
        let (a, b) = match self.pivot {
            Some((a, b)) if (head - b).abs() <= PIVOT_SNAP => {
                let a = a + (head - a) * k;
                (a, b + (a - b) * k)
            }
            _ => (head, head),
        };
        self.pivot = Some((a, b));
        if let Some((h, p)) = self.last {
            self.head_moves += f64::from(head - h).powi(2);
            self.pivot_moves += f64::from(b - p).powi(2);
            self.frames += 1;
        }
        self.last = Some((head, b));
        b
    }

    /// RMS change per frame of the head's and the pivot's height above the feet in third
    /// person, units.
    pub fn pivot_rms(&self) -> (f32, f32) {
        let n = f64::from(self.frames.max(1));
        ((self.head_moves / n).sqrt() as f32, (self.pivot_moves / n).sqrt() as f32)
    }

    /// Forget the pivot (furniture, a view without the head): it starts at the head next.
    pub fn pivot_reset(&mut self) {
        self.pivot = None;
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn input(speed: f32, phase: f32) -> BobInput {
        BobInput { dt: DT, speed, phase, heading: 0.0, sights: false, sneaking: false, airborne: false, on: true }
    }

    #[test]
    fn standing_still_does_not_bob() {
        let mut b = HeadBob::default();
        let mut o = Vec3::ZERO;
        for f in 0..120 {
            o = b.first_person(input(0.0, f as f32 / 72.0));
        }
        assert!(o.length() < 1e-4, "{o}");
    }

    #[test]
    fn running_dips_twice_a_stride() {
        let mut b = HeadBob::default();
        let mut zs = Vec::new();
        // 1.2 s cycle, two seconds of running.
        for f in 0..240 {
            zs.push(b.first_person(input(300.0, (f as f32 * DT / 1.2).fract())).z);
        }
        // Frames 101-239: the footfalls (phases 0.33 and 0.83 of the 72-frame cycle) at
        // frames 132, 168 and 204.
        let last = &zs[101..];
        let low = last.iter().cloned().fold(0.0, f32::min);
        assert!((-DIP..-0.9 * DIP).contains(&low), "deepest {low}");
        let turns = last.windows(3).filter(|w| w[1] < w[0] && w[1] <= w[2]).count();
        assert_eq!(turns, 3, "{last:?}");
    }

    #[test]
    fn switched_off_fades_out() {
        let mut b = HeadBob::default();
        for f in 0..120 {
            b.first_person(input(300.0, f as f32 / 72.0));
        }
        let mut o = Vec3::ONE;
        for f in 0..120 {
            o = b.first_person(BobInput { on: false, ..input(300.0, f as f32 / 72.0) });
        }
        assert!(o.length() < 0.01, "{o}");
    }

    #[test]
    fn the_third_person_pivot_leaves_out_the_stride_but_follows_a_crouch() {
        let mut b = HeadBob::default();
        // The head rises and falls 3 units twice a 1.2 s stride about 120.
        let mut out = Vec::new();
        for f in 0..240 {
            let t = f as f32 * DT;
            out.push(b.pivot_height(120.0 + 3.0 * (std::f32::consts::TAU * 2.0 * t / 1.2).sin(), DT));
        }
        let tail = &out[120..];
        let (lo, hi) = tail.iter().fold((f32::MAX, f32::MIN), |(l, h), x| (l.min(*x), h.max(*x)));
        assert!(hi - lo < 0.15 * 6.0, "pivot moved {} of the head's 6", hi - lo);
        // Crouching: the head drops 30 units; 90 % of the way in a second.
        let mut p = 0.0;
        for _ in 0..60 {
            p = b.pivot_height(90.0, DT);
        }
        assert!(p < 93.0, "after a second {p}");
    }

    #[test]
    fn a_teleport_moves_the_pivot_at_once() {
        let mut b = HeadBob::default();
        b.pivot_height(120.0, DT);
        assert_eq!(b.pivot_height(30.0, DT), 30.0);
    }
}
