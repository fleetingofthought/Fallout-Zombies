//! The voice filter: dialogue said through a helmet (docs/spec-notes/audio.md §15.5).
//!
//! - **Who** [documented: xEdit ARMO/ARMA DNAM flag "Modulates Voice"; GECK wiki Armor,
//!   "Checked if the item modulates the actor's voice", and ArmorAddon, "distorts the
//!   character's voice (like Power Armor helmets do, for example)"]: a speaker wearing an
//!   armour with the flag (38 of FalloutNV.esm's armours, all helmets and masks: the power
//!   armour helmets, the NCR Ranger combat helmet, the Legion helmets, gas, welding and
//!   surgical masks [observed]).
//! - **Not talking activators** [observed 2026-11-06, observer batch o28, issue #992]: the
//!   original plays a line said through an intercom as its file. Benny's line through the
//!   Tops' security intercom (a TACT without "No Voice Filter") came out of the original
//!   with the file's spectrum, within 4 dB of it from 200 Hz to 6 kHz (no band at 350 or
//!   3,200 Hz), and the intercom lines' voice files are already band-limited
//!   (docs/spec-notes/audio.md §15.5). So a talking activator's lines are not filtered again, whatever
//!   its "No Voice Filter" flag says.
//! - **How it sounds** [hypothesis: the game's filter is not documented; NVForever's is a
//!   radio-like one]: a band from [`LOW_CUT_HZ`] to [`HIGH_CUT_HZ`] (two second-order
//!   high-pass and low-pass sections, the Audio EQ Cookbook's biquads), a mild soft
//!   saturation ([`DRIVE`]), and a short metallic ring ([`RING_SECONDS`],
//!   [`RING_FEEDBACK`]) as of a mask's cavity; the level is kept about the same.
//! - Played by `crate::voice`: a filtered line always goes through `crate::sfx::Panned`
//!   (the conversation's at both ears whole), with the filter on.

use fnv_sim::FormId;

use crate::sim::SimState;

/// The filter's band [hypothesis: a radio voice band].
pub const LOW_CUT_HZ: f32 = 350.0;
pub const HIGH_CUT_HZ: f32 = 3200.0;
/// The saturation's drive, `tanh(DRIVE · x) / tanh(DRIVE)` [hypothesis].
pub const DRIVE: f32 = 1.8;
/// The ring's delay and feedback [hypothesis].
pub const RING_SECONDS: f32 = 0.0035;
pub const RING_FEEDBACK: f32 = 0.35;
/// The level after the filter, to make up for the band taken away [hypothesis].
pub const MAKEUP: f32 = 1.6;

/// Whether `speaker`'s lines go through the voice filter (module notes): a speaker
/// wearing an armour that modulates the voice. A talking activator never is.
pub fn speaker_filtered(sim: &SimState, speaker: FormId) -> bool {
    sim.world.equipment(speaker).is_some_and(|items| items.iter().any(|&i| sim.forms.item(i).is_some_and(|it| it.modulates_voice)))
}

/// A biquad section (Audio EQ Cookbook form, normalised by a0).
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    /// Butterworth (Q = 1/√2) low- or high-pass at `hz`.
    fn new(hz: f32, rate: f32, high: bool) -> Biquad {
        let w = std::f32::consts::TAU * (hz / rate).min(0.49);
        let (sin, cos) = w.sin_cos();
        let alpha = sin / std::f32::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        let b = if high { [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0] } else { [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0] };
        Biquad { b: b.map(|v| v / a0), a: [-2.0 * cos / a0, (1.0 - alpha) / a0], x: [0.0; 2], y: [0.0; 2] }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// One channel of the voice filter (module notes).
#[derive(Clone, Debug)]
pub struct VoiceFilter {
    low_cut: Biquad,
    high_cut: Biquad,
    ring: Vec<f32>,
    at: usize,
}

impl VoiceFilter {
    pub fn new(rate: u32) -> VoiceFilter {
        let r = rate.max(1) as f32;
        VoiceFilter {
            low_cut: Biquad::new(LOW_CUT_HZ, r, true),
            high_cut: Biquad::new(HIGH_CUT_HZ, r, false),
            ring: vec![0.0; ((RING_SECONDS * r).round() as usize).max(1)],
            at: 0,
        }
    }

    /// One sample through the filter.
    pub fn run(&mut self, x: f32) -> f32 {
        let band = self.high_cut.run(self.low_cut.run(x));
        let driven = (DRIVE * band).tanh() / DRIVE.tanh();
        let delayed = self.ring[self.at];
        let out = driven + RING_FEEDBACK * delayed;
        self.ring[self.at] = out;
        self.at = (self.at + 1) % self.ring.len();
        (out * MAKEUP * (1.0 - RING_FEEDBACK)).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The level of a sine at `hz` after the filter has settled, over its level before.
    fn response(hz: f32) -> f32 {
        let rate = 22050;
        let mut f = VoiceFilter::new(rate);
        let n = rate as usize;
        let (mut inp, mut out) = (0.0f32, 0.0f32);
        for i in 0..n {
            let x = 0.1 * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin();
            let y = f.run(x);
            if i > n / 2 {
                inp += x * x;
                out += y * y;
            }
        }
        (out / inp).sqrt()
    }

    #[test]
    fn keeps_the_voice_band_and_cuts_the_rest() {
        let mid = response(1000.0);
        assert!(mid > 0.5 && mid < 2.0, "1 kHz kept: {mid}");
        assert!(response(80.0) < 0.2 * mid, "80 Hz cut: {}", response(80.0));
        assert!(response(8000.0) < 0.2 * mid, "8 kHz cut: {}", response(8000.0));
    }

    #[test]
    fn silence_stays_silent_and_loud_input_is_bounded() {
        let mut f = VoiceFilter::new(44100);
        assert!((0..1000).all(|_| f.run(0.0) == 0.0));
        assert!((0..1000).map(|i| f.run(if i % 2 == 0 { 1.0 } else { -1.0 })).all(|y| y.abs() <= 1.0));
    }
}
