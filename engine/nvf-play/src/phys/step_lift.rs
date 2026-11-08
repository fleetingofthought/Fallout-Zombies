//! Drawn height smoothed over steps and bumps (owner's playtest list 7 item 9;
//! docs/spec-notes/physics.md §6.11).
//!
//! The character controller moves a character's feet up a step or down off a bump in one
//! fixed step, its feet follow every facet of a road's or the terrain's collision (the
//! height changes in straight lines with a kink at each edge), and the drawn ground under
//! them (`super::drawn_sink`) can change height from one step to the next as well; drawn
//! there, the body and the camera showed each step, kink and seam at once. A
//! [`StepLift`] is how far a character is drawn above (or below) where its feet and drawn
//! ground put it: what the drawn height changed by in one fixed step beyond what the
//! vertical speed of the last moments predicts is taken up into it, and it goes back to 0
//! as a critically damped spring, so the drawn height follows the ground smoothly. The
//! feet, the collision capsule and every query stay where the controller has them
//! [design].
//!
//! - **What is predicted**: the vertical speed on the ground, filtered over
//!   [`SPEED_SECONDS`]. A steady slope is predicted whole, so a long hill is drawn as
//!   walked once the filter has caught up; a step, a bump, a facet's kink or a snap down
//!   is not, and is smoothed. A step does not feed the filter: each sample may differ
//!   from the prediction by at most the horizontal distance walked × [`STEP_SLOPE`], so
//!   the drawn height does not overshoot the top of a step. The prediction is never more
//!   than [`MAX_GRADE`] × the horizontal distance walked (standing still it is 0, so
//!   stopping on a slope does not leave the body drawn above the feet). In the air, swimming or put
//!   somewhere (a change over [`JUMP`]) nothing is taken up and the speed starts again
//!   from 0.
//! - **How fast** [design]: [`OMEGA`] for a small lift, stiffer the larger it is (×2 at
//!   [`STIFFEN`] units), so ripples are smoothed and the lag at the foot of a steep hill
//!   stays under about 8 units; never more than [`MAX_LIFT`] either way.
//! - **Measured** [observed: `fnvtool phys walk` traces of the Goodsprings road and the
//!   desert south of it, replayed through this filter, 2026-11-07]: running along the
//!   road at 300 units/s the drawn height's vertical acceleration fell from 741 to 284
//!   units/s² RMS (the old slope rule let every road ripple through), walking from 239 to
//!   85, running over rocks and rough ground from 2621 to 504 (the old rule 1072).

/// Time constant of the predicted vertical speed, seconds [design].
pub const SPEED_SECONDS: f32 = 0.2;
/// Most a speed sample may differ from the prediction, per unit walked [design: steeper
/// is a step and is left out of the prediction].
pub const STEP_SLOPE: f32 = 0.4;
/// Most vertical speed predicted per unit walked [design: a 63° grade, steeper than the
/// controller climbs]; standing still nothing is predicted.
pub const MAX_GRADE: f32 = 2.0;
/// The spring's angular frequency for a small lift, 1/s [design].
pub const OMEGA: f32 = 8.0;
/// The spring is twice as stiff at a lift of this many units, three times at twice it
/// [design].
pub const STIFFEN: f32 = 6.0;
/// Most the drawn height may lag, game units [design: the controller's step height].
pub const MAX_LIFT: f32 = 24.0;
/// A change of the drawn height over this in one step is a placement, not a step: the
/// lift goes back to 0 [design].
pub const JUMP: f32 = 48.0;

/// A character's drawn lift (see the module notes), stepped with its physics.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StepLift {
    /// Drawn this far above the feet's drawn height after the last physics step.
    pub lift: f32,
    /// The lift before the last physics step (drawing between steps interpolates).
    pub prev: f32,
    rate: f32,
    /// The predicted vertical speed on the ground, units/s.
    speed: f32,
    /// Steps and drawn-ground changes taken up by more than half a unit, and the largest
    /// lift (either way) since the start (for the exit report).
    pub taken_up: u32,
    pub most: f32,
    /// How rough the feet's and the drawn height were while walking on the ground (for
    /// the exit report).
    pub rough: Roughness,
}

/// The vertical acceleration of the feet and of the drawn height over consecutive fixed
/// steps on the ground while walking (moving over half a unit a step).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Roughness {
    /// Last step's change of the feet's and the drawn height, while counting.
    last: Option<(f32, f32)>,
    feet: f64,
    drawn: f64,
    /// Steps counted.
    pub steps: u32,
}

impl Roughness {
    fn step(&mut self, feet_dz: f32, drawn_dz: f32, walking: bool, dt: f32) {
        if !walking || dt <= 0.0 {
            self.last = None;
            return;
        }
        if let Some((f, d)) = self.last {
            let per = 1.0 / f64::from(dt * dt);
            self.feet += (f64::from(feet_dz - f) * per).powi(2);
            self.drawn += (f64::from(drawn_dz - d) * per).powi(2);
            self.steps += 1;
        }
        self.last = Some((feet_dz, drawn_dz));
    }

    /// RMS vertical acceleration of the feet and of the drawn height, units/s².
    pub fn rms(&self) -> (f32, f32) {
        let n = f64::from(self.steps.max(1));
        ((self.feet / n).sqrt() as f32, (self.drawn / n).sqrt() as f32)
    }
}

impl StepLift {
    /// One fixed step of `dt` seconds in which the feet's drawn height changed by `dz`
    /// while they moved `horizontal` units; `on_ground`: on the ground before and after
    /// it and not swimming.
    pub fn step(&mut self, dz: f32, horizontal: f32, on_ground: bool, dt: f32) {
        self.prev = self.lift;
        if dz.abs() > JUMP {
            self.reset();
            return;
        }
        if on_ground {
            let excess = dz - self.speed * dt;
            self.lift -= excess;
            self.taken_up += u32::from(excess.abs() > 0.5);
        } else {
            self.speed = 0.0;
        }
        // Critically damped towards 0 (the exact solution over dt for this step's
        // stiffness), stiffer the further from 0.
        let w = OMEGA * (1.0 + self.lift.abs() / STIFFEN);
        let (x, v) = (self.lift, self.rate);
        let e = (-w * dt).exp();
        let k = v + w * x;
        self.lift = (x + k * dt) * e;
        self.rate = (v - w * k * dt) * e;
        if self.lift.abs() > MAX_LIFT {
            self.lift = self.lift.clamp(-MAX_LIFT, MAX_LIFT);
            self.rate = 0.0;
        }
        if on_ground && dt > 0.0 {
            let (predicted, most) = (self.speed * dt, horizontal * STEP_SLOPE);
            let sample = dz.clamp(predicted - most, predicted + most);
            self.speed += (sample / dt - self.speed) * (dt / SPEED_SECONDS).min(1.0);
            // Never more than [`MAX_GRADE`] per unit walked: standing still, nothing is
            // predicted.
            let bound = MAX_GRADE * horizontal / dt;
            self.speed = self.speed.clamp(-bound, bound);
        }
        self.rough.step(dz, dz + self.lift - self.prev, on_ground && horizontal > 0.5, dt);
        self.most = self.most.max(self.prev.abs());
    }

    /// Back to no lift (a placement), the tallies kept.
    pub fn reset(&mut self) {
        let rough = Roughness { last: None, ..self.rough };
        *self = StepLift { taken_up: self.taken_up, most: self.most, rough, ..StepLift::default() };
    }

    /// The drawn ground under the feet moved by `dz` without the feet moving (after the
    /// physics steps): taken up whole, as a step's excess.
    pub fn ground_moved(&mut self, dz: f32) {
        if dz.abs() > JUMP {
            self.reset();
            return;
        }
        self.taken_up += u32::from(dz.abs() > 0.5);
        self.lift = (self.lift - dz).clamp(-MAX_LIFT, MAX_LIFT);
        self.prev = (self.prev - dz).clamp(-MAX_LIFT, MAX_LIFT);
    }

    /// The lift between the last two physics steps, `alpha` of the way to the last.
    pub fn at(&self, alpha: f32) -> f32 {
        self.prev + (self.lift - self.prev) * alpha
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// Drawn height after each step: feet height plus lift.
    fn walk(lift: &mut StepLift, heights: &[f32], horizontal: f32) -> Vec<f32> {
        let mut out = Vec::new();
        for w in heights.windows(2) {
            lift.step(w[1] - w[0], horizontal, true, DT);
            out.push(w[1] + lift.lift);
        }
        out
    }

    #[test]
    fn a_step_up_is_climbed_smoothly_and_settles() {
        let mut lift = StepLift::default();
        // Walking level at 150 units/s, then a 10-unit step in one fixed step.
        let mut h = vec![0.0; 30];
        h.extend(std::iter::repeat_n(10.0, 60));
        let drawn = walk(&mut lift, &h, 2.5);
        let jumps: Vec<f32> = drawn.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
        assert!(jumps.iter().all(|j| *j < 2.0), "largest drawn change in a step {:?}", jumps.iter().cloned().fold(0.0, f32::max));
        assert!((drawn[29 + 12] - 10.0).abs() < 1.0, "most of the way after 0.2 s: {}", drawn[41]);
        assert!(lift.lift.abs() < 0.05, "settled a second after the step: {}", lift.lift);
    }

    #[test]
    fn a_steady_slope_is_drawn_as_walked() {
        let mut lift = StepLift::default();
        // Running 6 units a step up a 45° slope for two seconds.
        let h: Vec<f32> = (0..120).map(|i| i as f32 * 6.0).collect();
        walk(&mut lift, &h, 6.0);
        assert!(lift.lift.abs() < 0.5, "lag on the slope: {}", lift.lift);
    }

    #[test]
    fn a_bump_while_running_is_smoothed() {
        let mut lift = StepLift::default();
        // Level running, then a 4-unit bump up and down again, one step each.
        let mut h = vec![0.0; 30];
        h.extend([4.0, 4.0, 0.0]);
        h.extend(vec![0.0; 30]);
        let drawn = walk(&mut lift, &h, 6.0);
        let top = drawn.iter().cloned().fold(0.0, f32::max);
        assert!(top < 3.0, "the drawn body rose {top} of the 4");
    }

    /// The drawn height's change of vertical speed per step, largest over a run.
    fn sharpest(drawn: &[f32]) -> f32 {
        drawn.windows(3).map(|w| (w[2] - 2.0 * w[1] + w[0]).abs()).fold(0.0, f32::max)
    }

    #[test]
    fn a_road_rippled_by_its_facets_is_smoothed() {
        let mut lift = StepLift::default();
        // Running 5 units a step over facets sloping +0.05, -0.05 and -0.15 per unit, then
        // a 5.5-unit drop over two steps onto the ground beside the road (the shape of the
        // Goodsprings road's trace, `fnvtool phys walk`).
        let mut h = vec![0.0f32; 30];
        for (steps, per_step) in [(20, 0.25), (20, -0.25), (15, -0.75), (2, -2.75), (30, 0.0)] {
            for _ in 0..steps {
                let last = *h.last().unwrap();
                h.push(last + per_step);
            }
        }
        let drawn = walk(&mut lift, &h, 5.0);
        let (feet, drawn) = (sharpest(&h[1..]), sharpest(&drawn));
        assert!(drawn < feet / 3.0, "sharpest change of speed: feet {feet}, drawn {drawn}");
        let (feet, drawn) = lift.rough.rms();
        assert!(lift.rough.steps > 80 && drawn < feet / 2.0, "rms feet {feet}, drawn {drawn} over {} steps", lift.rough.steps);
    }

    #[test]
    fn a_step_up_is_not_overshot() {
        for horizontal in [2.5, 6.0] {
            let mut lift = StepLift::default();
            let mut h = vec![0.0; 30];
            h.extend(std::iter::repeat_n(10.0, 90));
            let top = walk(&mut lift, &h, horizontal).into_iter().fold(0.0, f32::max);
            assert!(top < 10.75, "drawn {top} over a 10-unit step at {horizontal} a step");
        }
    }

    /// Up a 30° slope at `per` units a step for two seconds, then `then` units a step on
    /// the level for two more.
    fn slope_then_level(per: f32, then: f32) -> (Vec<f32>, Vec<f32>, StepLift) {
        let (mut lift, mut h, mut drawn) = (StepLift::default(), vec![0.0f32; 30], Vec::new());
        for _ in 0..30 {
            lift.step(0.0, per, true, DT);
            drawn.push(lift.lift);
        }
        for i in 0..240 {
            let (dz, horizontal) = if i < 120 { (per * 0.57735, per) } else { (0.0, then) };
            let z = h.last().unwrap() + dz;
            h.push(z);
            lift.step(dz, horizontal, true, DT);
            drawn.push(z + lift.lift);
        }
        (h, drawn, lift)
    }

    #[test]
    fn stopping_on_a_slope_settles() {
        for per in [2.5, 5.0] {
            let (_, _, lift) = slope_then_level(per, 0.0);
            assert!(lift.lift.abs() < 0.05, "drawn {} above the feet a second after stopping at {per} a step", lift.lift);
            assert_eq!(lift.speed, 0.0);
        }
    }

    #[test]
    fn the_top_of_a_slope_is_overshot_by_at_most_ten_units() {
        // [observed: this replay, 2026-11-07] 9.9 at a run, 6.1 at a walk: the prediction
        // carries on over the kink for about 0.2 s (physics.md §6.11).
        for (per, most) in [(2.5, 6.5), (5.0, 10.5)] {
            let (h, drawn, _) = slope_then_level(per, per);
            let top = h[150];
            let over = drawn.iter().map(|d| d - top).fold(f32::MIN, f32::max);
            assert!(over < most, "drawn {over} over the top of the slope at {per} a step");
        }
    }

    #[test]
    fn a_flight_of_stairs_is_climbed_smoothly_and_overshot_by_little() {
        // 8-unit risers, 12-unit treads, ten of them; [observed: this replay, 2026-11-07]
        // up to 4.2 over the top step at a run, 1.2 at a walk (the prediction carries on
        // past the last riser; see physics.md §6.11).
        for (per, most) in [(2.5, 1.5), (5.0, 4.5)] {
            let mut lift = StepLift::default();
            let (mut x, mut h) = (0.0f32, vec![0.0f32; 30]);
            for _ in 0..240 {
                x += per;
                h.push(((x / 12.0).floor() * 8.0).min(80.0));
            }
            let drawn = walk(&mut lift, &h, per);
            let over = drawn.iter().map(|d| d - 80.0).fold(f32::MIN, f32::max);
            assert!(over < most, "drawn {over} over the top step at {per} a step");
            let jumps = drawn.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
            assert!(jumps < 8.0 * 0.6, "largest drawn change in a step {jumps}");
            assert!(lift.lift.abs() < 0.05, "settled: {}", lift.lift);
        }
    }

    #[test]
    fn falls_and_placements_are_not_smoothed() {
        let mut lift = StepLift::default();
        lift.step(-10.0, 2.0, true, DT);
        assert!(lift.lift > 5.0);
        lift.step(-300.0, 0.0, true, DT);
        assert_eq!((lift.lift, lift.prev, lift.taken_up), (0.0, 0.0, 1));
        lift.step(-20.0, 2.0, false, DT);
        assert_eq!(lift.lift, 0.0, "in the air the drawn height follows the feet");
    }

    #[test]
    fn a_change_of_the_drawn_ground_is_taken_up_and_interpolated() {
        let mut lift = StepLift::default();
        lift.ground_moved(3.0);
        assert_eq!((lift.prev, lift.lift, lift.taken_up), (-3.0, -3.0, 1));
        lift.step(0.0, 0.0, true, DT);
        assert!(lift.at(0.0) < lift.at(1.0) && lift.at(1.0) < 0.0);
    }
}
