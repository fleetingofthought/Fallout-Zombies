//! Controller aim assist (docs/design/controller.md §5, combat.md §5.1): with a controller
//! working the controls, the right stick turns slower while the crosshair is on a nearby
//! hostile (#1196), and aiming down the sights eases the view onto one near the crosshair
//! for a moment (#1197). The mouse never gets either. All of it is [design]: the original
//! only bends shots toward a target near the crosshair (its auto aim, `fAutoAim*`), it
//! does not touch the view [documented: GECK wiki "Auto Aim Settings", owner's export].
//!
//! - **The target** ([`pick`]): a living actor drawn now (`crate::hitvol::HitVolumes`)
//!   that is hostile to the player (fighting the player, or would attack it: the rule the
//!   crosshair's red uses, `crate::hud_target`), within `fAutoAimMaxDistance` (1800) of
//!   the crosshair ray's start, inside the on-target cone around the crosshair
//!   ([`cone_half_angle`], from `fAutoAimScreenPercentage`, 10), and seen along a ray to
//!   its aim point (the same collision world shots use, passing what shots pass: water,
//!   triggers and NPCs' walking capsules, [`sight_filter`]; no lock through walls). The
//!   nearest to the crosshair by angle wins. Its aim point is the middle of its torso's
//!   posed part, else [`TORSO_ABOVE_FEET`] over its feet.
//! - **Slowdown** ([`slow_target`], [`ease`]): while the crosshair is within the
//!   target's body ([`FRICTION_RADIUS`] around the aim point, seen from the crosshair),
//!   the stick's turn is scaled down by the Aim slowdown setting, at most
//!   [`MAX_SLOWDOWN`]; the scale eases in and out over [`EASE_SECS`], so the turn never
//!   jumps. Friction only, no pull.
//! - **Lock-on** ([`Lock`], [`pull_step`]): when aiming down the sights begins (Block,
//!   LT, with the weapon out) and a target is picked, the view turns toward its aim point
//!   for [`LOCK_SECS`], easing in with a time constant [`PULL_TAU`] over the strength and
//!   at most [`MAX_PULL_RATE`] × strength radians a second, following a moving target. It
//!   lets go at the end, when the target dies, leaves the cone ([`HOLD_CONE`] of it),
//!   range or sight, when aiming ends, or when the right stick is pushed hard against the
//!   pull ([`breaks_lock`]). A new lock needs [`LOCK_COOLDOWN`] since the last began, so
//!   tapping LT does not aim for the player.
//! - **Not while** the free camera, the weapon wheel, a kill camera, VATS mode, an
//!   animation driving the view, furniture holding the player, a script's look lock, a
//!   circling or detached camera, an ungrabbed cursor or anything but play has the look
//!   (the guards of `crate::player`'s look, and VATS: [`Guards`]).
//! - **Settings** (Controller group of the settings page, kept in `[Settings]` of
//!   NVForever.ini): `aim-assist` on/off (on), `aim-slowdown` and `aim-lock` strengths
//!   (0 turns that part off).

use crate::bindings::{Bindings, InputDevice};
use crate::control::{ControlLocks, PlayerControl};
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::settings::{add_setting, Setting, Slider};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use fnv_phys::{layers, Filter, Key, Owner};
use fnv_sim::settings::Setting as GameSetting;
use fnv_sim::FormId;

/// The settings page's group of the controller's aim assist.
pub const GROUP: &str = "Controller";

/// The range when the load order has no `fAutoAimMaxDistance` [documented: GECK wiki
/// "Auto Aim Settings", 1800].
pub const DEFAULT_RANGE: f32 = 1800.0;
/// The on-target area when the load order has no `fAutoAimScreenPercentage` [documented:
/// same page, 10].
pub const DEFAULT_SCREEN_PERCENT: f32 = 10.0;
/// The horizontal field of view the screen percentage is measured against [hypothesis:
/// the original's default `fDefaultFOV` 75; the player's own setting is not read here].
pub const SCREEN_FOV_DEGREES: f32 = 75.0;
/// How far over its feet a target's torso is aimed at when its parts are not posed
/// [design: about the chest of a 128-unit human].
pub const TORSO_ABOVE_FEET: f32 = 80.0;
/// How far from the aim point the crosshair still counts as on the target for the
/// slowdown, game units [design: a human's chest and a margin].
pub const FRICTION_RADIUS: f32 = 40.0;
/// The most the slowdown takes off the stick's turn (the setting at full) [design: the
/// issue's 0.4–0.5 of the speed sits near the default].
pub const MAX_SLOWDOWN: f32 = 0.75;
/// The slowdown's default setting: 0.6 of [`MAX_SLOWDOWN`], 45% slower [design].
pub const DEFAULT_SLOWDOWN: f32 = 0.6;
/// The lock-on's default strength [design].
pub const DEFAULT_LOCK: f32 = 0.6;
/// Seconds the slowdown takes to come in or go out fully [design, #1196: ~0.1 s].
pub const EASE_SECS: f32 = 0.1;
/// Seconds a lock-on lasts [design, #1197: 0.2–0.3 s].
pub const LOCK_SECS: f32 = 0.3;
/// Seconds from one lock's start before another may start [design].
pub const LOCK_COOLDOWN: f32 = 0.6;
/// The pull's time constant at full strength, seconds; it grows as the strength drops
/// [design: at full, 0.3 s takes the error to about 2%].
pub const PULL_TAU: f32 = 0.08;
/// The pull's fastest turn at full strength, radians a second [design].
pub const MAX_PULL_RATE: f32 = 4.0;
/// A lock holds while its target stays within this many times the cone [design].
pub const HOLD_CONE: f32 = 1.5;
/// A right-stick push at least this far that points against the pull breaks the lock
/// [design].
pub const BREAK_PUSH: f32 = 0.7;

/// The aim assist's settings (module notes).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct AimAssist {
    pub on: bool,
    /// The slowdown's strength, 0 to 1 (a part of [`MAX_SLOWDOWN`]); 0: none.
    pub slowdown: f32,
    /// The lock-on's strength, 0 to 1; 0: none.
    pub lock: f32,
}

impl Default for AimAssist {
    fn default() -> Self {
        AimAssist { on: true, slowdown: DEFAULT_SLOWDOWN, lock: DEFAULT_LOCK }
    }
}

/// What the aim assist does this frame: the scale on the right stick's turn
/// (`crate::player`'s look reads it) and the lock-on under way.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct AimAssistState {
    /// The right stick's turn is multiplied by this (1: untouched).
    pub look_scale: f32,
    pub lock: Option<Lock>,
    /// Seconds since the last lock began.
    pub since_lock: f32,
    /// Aiming down the sights last frame (to see it begin).
    pub was_aiming: bool,
}

impl Default for AimAssistState {
    fn default() -> Self {
        AimAssistState { look_scale: 1.0, lock: None, since_lock: f32::INFINITY, was_aiming: false }
    }
}

/// A lock-on under way: its target and seconds left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lock {
    pub form_id: u32,
    pub left: f32,
}

/// A possible target as the crosshair sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub form_id: u32,
    /// The aim point, game space.
    pub aim: [f32; 3],
    /// From the crosshair ray's start, game units.
    pub distance: f32,
    /// Off the crosshair, radians.
    pub angle: f32,
}

/// The angle (radians) between the ray from `origin` along unit `dir` and the line to
/// `point`, and the distance to it.
pub fn angle_off(origin: [f32; 3], dir: [f32; 3], point: [f32; 3]) -> (f32, f32) {
    let d = [point[0] - origin[0], point[1] - origin[1], point[2] - origin[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1e-3 {
        return (0.0, 0.0);
    }
    let cos = (d[0] * dir[0] + d[1] * dir[1] + d[2] * dir[2]) / len;
    (cos.clamp(-1.0, 1.0).acos(), len)
}

/// The on-target cone's half angle, radians: a circle around the crosshair whose radius
/// is `percent` of the screen's width, at a horizontal field of view of `fov` radians
/// [hypothesis: how the original measures `fAutoAimScreenPercentage` is not documented].
pub fn cone_half_angle(percent: f32, fov: f32) -> f32 {
    (2.0 * (percent / 100.0).max(0.0) * (fov / 2.0).tan()).atan()
}

/// The target among `cands`: within `range` and `cone`, the nearest to the crosshair by
/// angle (ties: the nearer one).
pub fn pick(cands: &[Candidate], cone: f32, range: f32) -> Option<Candidate> {
    cands.iter().filter(|c| c.distance <= range && c.angle <= cone).min_by(|a, b| a.angle.total_cmp(&b.angle).then(a.distance.total_cmp(&b.distance))).copied()
}

/// Whether the crosshair is on `c`'s body: within [`FRICTION_RADIUS`] of its aim point as
/// seen from the crosshair.
pub fn on_body(c: &Candidate) -> bool {
    c.angle <= (FRICTION_RADIUS / c.distance.max(1.0)).atan()
}

/// The stick's turn scale the slowdown heads for: `1 - strength × MAX_SLOWDOWN` on the
/// target, 1 off it.
pub fn slow_target(strength: f32, on_target: bool) -> f32 {
    if on_target {
        1.0 - strength.clamp(0.0, 1.0) * MAX_SLOWDOWN
    } else {
        1.0
    }
}

/// `current` moved toward `target` over `dt` seconds, the whole way from 0 to 1 taking
/// [`EASE_SECS`].
pub fn ease(current: f32, target: f32, dt: f32) -> f32 {
    let step = (dt / EASE_SECS).max(0.0);
    current + (target - current).clamp(-step, step)
}

/// The heading and pitch of the line from `origin` to `point` (`crate::space::look_dir`'s
/// inverse).
pub fn heading_pitch_to(origin: [f32; 3], point: [f32; 3]) -> (f32, f32) {
    let d = [point[0] - origin[0], point[1] - origin[1], point[2] - origin[2]];
    (d[0].atan2(d[1]).rem_euclid(std::f32::consts::TAU), d[2].atan2(d[0].hypot(d[1])))
}

/// One frame (`dt` seconds) of the lock-on's pull at `strength` (above 0): the turn
/// (heading, pitch, radians) that takes the view from `(heading, pitch)` toward
/// `(want_h, want_p)`, easing (time constant [`PULL_TAU`] / strength) and capped at
/// [`MAX_PULL_RATE`] × strength radians a second.
pub fn pull_step(heading: f32, pitch: f32, want_h: f32, want_p: f32, strength: f32, dt: f32) -> (f32, f32) {
    let s = strength.clamp(0.0, 1.0);
    if s <= 0.0 || dt <= 0.0 {
        return (0.0, 0.0);
    }
    let err = (crate::control::wrap_angle(want_h - heading), want_p - pitch);
    let len = err.0.hypot(err.1);
    if len < 1e-6 {
        return (0.0, 0.0);
    }
    let eased = len * (1.0 - (-dt * s / PULL_TAU).exp());
    let turn = eased.min(MAX_PULL_RATE * s * dt);
    (err.0 / len * turn, err.1 / len * turn)
}

/// Whether the right stick `stick` (x right, y up, as `crate::pad::stick`) is pushed hard
/// against a pull turning by `pull` (heading, pitch): at least [`BREAK_PUSH`] and more
/// than 90° from the pull's way as the stick would turn the view.
pub fn breaks_lock(stick: Vec2, pull: (f32, f32), invert_y: bool) -> bool {
    let len = stick.length();
    let pl = pull.0.hypot(pull.1);
    if len < BREAK_PUSH || pl < 1e-6 {
        return false;
    }
    // The stick's turn: right raises the heading, up the pitch (down with Invert Y).
    let turn = Vec2::new(stick.x, if invert_y { -stick.y } else { stick.y });
    turn.dot(Vec2::new(pull.0, pull.1)) < 0.0
}

/// What keeps the aim assist from acting this frame (module notes: "Not while").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Guards {
    /// The setting's switch.
    pub on: bool,
    /// A controller works the controls.
    pub controller: bool,
    /// Play, or free look in a conversation.
    pub playing: bool,
    /// The window has the cursor (as `crate::player`'s look asks).
    pub grabbed: bool,
    /// A script locked looking.
    pub looking_locked: bool,
    pub wheel: bool,
    /// The camera circles or is detached.
    pub orbit: bool,
    pub free_cam: bool,
    pub kill_cam: bool,
    /// VATS mode has the input (`crate::vats_mode::VatsMode::holds_input`): it aims and
    /// turns the view itself.
    pub vats: bool,
    /// An animation drives the view (`crate::player::ViewDriven`).
    pub driven: bool,
    /// Furniture holds the player (its look is kept apart, `PlayerSeat::look`).
    pub seated: bool,
}

impl Guards {
    pub fn usable(&self) -> bool {
        self.on && self.controller && self.playing && self.grabbed && !self.looking_locked && !self.wheel && !self.orbit && !self.free_cam && !self.kill_cam && !self.vats && !self.driven && !self.seated
    }
}

/// The sight ray's filter: what a shot passes (water, triggers, zones, invisible walls:
/// `layers::shot_ignores`), the player's character, and every NPC's walking capsule
/// (`Key::Character`): a capsule is not the body, its hit volumes are, as for shots
/// (`crate::combat`'s rays).
pub fn sight_filter(player: Option<Key>, npcs: impl Iterator<Item = u32>) -> Filter {
    let mut f = Filter { exclude_layers: layers::shot_ignores(), ..Default::default() };
    f.exclude.extend(player);
    f.exclude.extend(npcs.map(Key::Character));
    f
}

/// Whether `c` is seen from `origin`: the ray to its aim point meets its own hit volume
/// (or its own capsule) first, or nothing up to it.
pub fn seen(q: &fnv_phys::Queries<'_>, filter: &Filter, origin: [f32; 3], c: &Candidate) -> bool {
    let to = [c.aim[0] - origin[0], c.aim[1] - origin[1], c.aim[2] - origin[2]];
    match q.ray(origin, to, c.distance + FRICTION_RADIUS, filter) {
        Some(h) => match h.owner {
            Owner::Actor { form_id, .. } => form_id == c.form_id,
            Owner::Character { id } => id == c.form_id,
            _ => false,
        },
        None => true,
    }
}

/// A game setting's float from the load order.
fn game_float(sim: &SimState, name: &str) -> Option<f32> {
    match sim.forms.settings.get(&name.to_ascii_lowercase()) {
        Some(GameSetting::Float(v)) => Some(*v),
        _ => None,
    }
}

pub struct AimAssistPlugin;

impl Plugin for AimAssistPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AimAssist>().init_resource::<AimAssistState>().add_systems(Update, assist.in_set(PlaySet::Ui));
        add_setting(
            app,
            Setting {
                key: "aim-assist",
                group: GROUP,
                label: "Aim assist",
                note: "Controller only, never the mouse. On: the right stick turns slower while the crosshair is on a nearby hostile, and aiming down the sights turns the view onto one near the crosshair for a moment. Off: neither.",
                get: |w| w.resource::<AimAssist>().on,
                set: |w, on| w.resource_mut::<AimAssist>().on = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting::slider(
                "aim-slowdown",
                GROUP,
                "Aim slowdown",
                "How much slower the right stick turns while the crosshair is on a nearby hostile (0: not at all).",
                Slider {
                    get: |w| w.resource::<AimAssist>().slowdown,
                    set: |w, v| w.resource_mut::<AimAssist>().slowdown = v,
                    min: 0.0,
                    step: 0.05,
                    shown: Some(|v| if v <= 0.0 { "OFF".to_string() } else { format!("{:.0}% slower", v * MAX_SLOWDOWN * 100.0) }),
                },
            ),
        );
        add_setting(
            app,
            Setting::slider(
                "aim-lock",
                GROUP,
                "Aim lock-on",
                "How hard aiming down the sights turns the view onto a nearby hostile near the crosshair (0: not at all). Push the right stick away to break it.",
                Slider { get: |w| w.resource::<AimAssist>().lock, set: |w, v| w.resource_mut::<AimAssist>().lock = v, min: 0.0, step: 0.05, shown: Some(|v| if v <= 0.0 { "OFF".to_string() } else { format!("{:.0}%", v * 100.0) }) },
            ),
        );
    }
}

/// The aim assist's frame (module notes): pick the target, ease the stick's scale, start,
/// run and end a lock-on.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn assist(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    (settings, bindings, pads): (Res<AimAssist>, Res<Bindings>, Query<&Gamepad>),
    (locks, look, free_cam, kill_cam): (Res<ControlLocks>, Option<Res<crate::dialogue::FreeLook>>, Option<Res<crate::free_cam::FreeCam>>, Option<Res<crate::kill_cam::KillCam>>),
    (vats, driven, cursor, seat): (Option<Res<crate::vats_mode::VatsMode>>, Option<Res<crate::player::ViewDriven>>, Query<&CursorOptions, With<PrimaryWindow>>, Option<Res<crate::furniture::PlayerSeat>>),
    control: Res<PlayerControl>,
    mut player: ResMut<Player>,
    phys: Option<ResMut<PhysState>>,
    vols: Option<Res<crate::hitvol::HitVolumes>>,
    parts: Option<Res<crate::hitvol::ActorParts>>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut st: ResMut<AimAssistState>,
) {
    let dt = time.delta_secs();
    st.since_lock += dt;
    let aiming = control.aiming && control.weapon_out;
    let began = aiming && !st.was_aiming;
    st.was_aiming = aiming;
    let playing = *state.get() == PlayState::Playing || look.is_some_and(|l| l.0);
    let guards = Guards {
        on: settings.on,
        controller: bindings.device == InputDevice::Controller,
        playing,
        grabbed: cursor.single().is_ok_and(|c| c.grab_mode != CursorGrabMode::None),
        looking_locked: locks.looking,
        wheel: control.wheel,
        orbit: control.orbit.active(),
        free_cam: free_cam.is_some_and(|f| f.on),
        kill_cam: kill_cam.is_some_and(|k| k.active()),
        vats: vats.is_some_and(|v| v.holds_input()),
        driven: driven.is_some_and(|d| d.0),
        seated: seat.is_some_and(|s| s.hold.is_some()),
    };
    let usable = guards.usable();
    let want_slow = usable && settings.slowdown > 0.0;
    let want_lock = usable && settings.lock > 0.0 && (st.lock.is_some() || began && st.since_lock >= LOCK_COOLDOWN);
    if !want_slow && !want_lock {
        st.look_scale = ease(st.look_scale, 1.0, dt);
        if st.lock.take().is_some() {
            info!("aim assist: lock-on let go (assist unavailable)");
        }
        return;
    }
    // The targets as the crosshair sees them.
    let (origin, dir) = crate::control::Crosshair::ray(control.crosshair.as_ref(), player.eye(), player.look_dir());
    let sim_ref = &mut *sim;
    let range = game_float(sim_ref, "fAutoAimMaxDistance").filter(|r| *r > 0.0).unwrap_or(DEFAULT_RANGE);
    let cone = cone_half_angle(game_float(sim_ref, "fAutoAimScreenPercentage").unwrap_or(DEFAULT_SCREEN_PERCENT), SCREEN_FOV_DEGREES.to_radians());
    let held = st.lock.map(|l| l.form_id);
    let near: Vec<Candidate> = vols
        .as_deref()
        .map(|v| v.living.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|&(form_id, feet)| {
            let torso = parts.as_deref().and_then(|p| p.by_ref.get(&form_id)).and_then(|p| p.segments.iter().find(|s| s.limb == fnv_combat::data::Limb::Torso));
            let aim = torso.map_or([feet[0], feet[1], feet[2] + TORSO_ABOVE_FEET], |s| [(s.a[0] + s.b[0]) / 2.0, (s.a[1] + s.b[1]) / 2.0, (s.a[2] + s.b[2]) / 2.0]);
            let (angle, distance) = angle_off(origin, dir, aim);
            let wide = if held == Some(form_id) { cone * HOLD_CONE } else { cone };
            (distance <= range && angle <= wide).then_some(Candidate { form_id, aim, distance, angle })
        })
        .collect();
    // Hostile and alive (the crosshair's red rule), then seen.
    let foes: Vec<Candidate> = match script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        Some(rt) if !near.is_empty() => with_world(rt, sim_ref, |rt| {
            near.iter()
                .filter(|c| {
                    let (r, g) = (FormId(c.form_id), &mut rt.game);
                    let fighting = g.awareness.get(r).and_then(|x| x.combat_target()).is_some_and(|(t, _)| t == FormId::PLAYER_REF);
                    !g.is_dead(r) && (fighting || g.would_attack(r, FormId::PLAYER_REF))
                })
                .copied()
                .collect()
        }),
        _ => Vec::new(),
    };
    let seen: Vec<Candidate> = match phys {
        Some(mut p) if !foes.is_empty() => {
            let p = &mut *p;
            let filter = sight_filter(p.player.as_ref().map(|c| c.key()), p.npcs.keys().copied());
            let q = p.world.queries();
            foes.into_iter().filter(|c| seen(&q, &filter, origin, c)).collect()
        }
        _ => Vec::new(),
    };
    // The slowdown, on any seen target the crosshair is on.
    let on_target = want_slow && seen.iter().any(on_body);
    st.look_scale = ease(st.look_scale, slow_target(settings.slowdown, on_target), dt);
    // The lock-on.
    if !want_lock {
        return;
    }
    let target = match st.lock {
        Some(l) => seen.iter().find(|c| c.form_id == l.form_id).copied(),
        None => pick(&seen, cone, range),
    };
    let Some(target) = target else {
        if let Some(l) = st.lock.take() {
            info!("aim assist: lock-on let go of 0x{:08X} (dead, gone from sight, the cone or range)", l.form_id);
        }
        return;
    };
    let mut lock = match st.lock {
        Some(l) if !aiming => {
            info!("aim assist: lock-on let go of 0x{:08X} (aiming ended)", l.form_id);
            st.lock = None;
            return;
        }
        Some(l) => l,
        None => {
            info!("aim assist: lock-on to 0x{:08X} at {:.0} units, {:.1}° off the crosshair", target.form_id, target.distance, target.angle.to_degrees());
            st.since_lock = 0.0;
            Lock { form_id: target.form_id, left: LOCK_SECS }
        }
    };
    let (want_h, want_p) = heading_pitch_to(origin, target.aim);
    let turn = pull_step(player.heading, player.pitch, want_h, want_p, settings.lock, dt);
    if breaks_lock(crate::pad::stick(&pads, false), turn, bindings.invert_y) {
        info!("aim assist: lock-on to 0x{:08X} broken by the right stick", lock.form_id);
        st.lock = None;
        return;
    }
    player.heading = (player.heading + turn.0).rem_euclid(std::f32::consts::TAU);
    player.pitch = (player.pitch + turn.1).clamp(-crate::player::MAX_PITCH, crate::player::MAX_PITCH);
    lock.left -= dt;
    st.lock = if lock.left > 0.0 {
        Some(lock)
    } else {
        info!("aim assist: lock-on to 0x{:08X} done", lock.form_id);
        None
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(form_id: u32, angle_deg: f32, distance: f32) -> Candidate {
        Candidate { form_id, aim: [0.0; 3], distance, angle: angle_deg.to_radians() }
    }

    #[test]
    fn the_cone_follows_the_screen_percentage() {
        let fov = SCREEN_FOV_DEGREES.to_radians();
        // 10% of the screen's width at 75°: about 8.7° off the crosshair.
        let c = cone_half_angle(DEFAULT_SCREEN_PERCENT, fov).to_degrees();
        assert!((c - 8.72).abs() < 0.05, "{c}");
        assert!(cone_half_angle(20.0, fov) > cone_half_angle(10.0, fov));
        assert_eq!(cone_half_angle(0.0, fov), 0.0);
    }

    #[test]
    fn angles_and_distances_from_the_crosshair() {
        let (a, d) = angle_off([0.0; 3], [0.0, 1.0, 0.0], [0.0, 100.0, 0.0]);
        assert!(a.abs() < 1e-6 && (d - 100.0).abs() < 1e-4);
        let (a, d) = angle_off([0.0; 3], [0.0, 1.0, 0.0], [100.0, 100.0, 0.0]);
        assert!((a.to_degrees() - 45.0).abs() < 1e-3 && (d - 141.42).abs() < 0.01);
    }

    #[test]
    fn the_nearest_by_angle_in_range_and_cone_is_picked() {
        let cone = 9f32.to_radians();
        let cands = [cand(1, 6.0, 500.0), cand(2, 2.0, 1200.0), cand(3, 1.0, 1900.0), cand(4, 12.0, 100.0)];
        // 3 is past the range, 4 outside the cone.
        assert_eq!(pick(&cands, cone, DEFAULT_RANGE).map(|c| c.form_id), Some(2));
        // Ties go to the nearer.
        let tie = [cand(5, 2.0, 900.0), cand(6, 2.0, 300.0)];
        assert_eq!(pick(&tie, cone, DEFAULT_RANGE).map(|c| c.form_id), Some(6));
        assert_eq!(pick(&[cand(4, 12.0, 100.0)], cone, DEFAULT_RANGE), None);
        assert_eq!(pick(&[], cone, DEFAULT_RANGE), None);
    }

    #[test]
    fn on_the_body_shrinks_with_distance() {
        // 40 units at 400: about 5.7°; at 1600 about 1.4°.
        assert!(on_body(&cand(1, 5.0, 400.0)));
        assert!(!on_body(&cand(1, 5.0, 1600.0)));
        assert!(on_body(&cand(1, 1.0, 1600.0)));
    }

    #[test]
    fn the_slowdown_scales_by_strength_on_target_only() {
        assert_eq!(slow_target(0.6, false), 1.0);
        assert!((slow_target(DEFAULT_SLOWDOWN, true) - 0.55).abs() < 1e-6);
        assert!((slow_target(1.0, true) - (1.0 - MAX_SLOWDOWN)).abs() < 1e-6);
        assert_eq!(slow_target(0.0, true), 1.0);
        // Out of range strengths are held to 0..1.
        assert!((slow_target(3.0, true) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn the_slowdown_eases_in_and_out_over_a_tenth_of_a_second() {
        // From 1 toward 0.55 at 60 frames a second: a sixth of the way each frame, there
        // after 0.045 s, never past it.
        let mut s = 1.0;
        let mut frames = 0;
        while (s - 0.55f32).abs() > 1e-6 {
            s = ease(s, 0.55, 1.0 / 60.0);
            frames += 1;
            assert!(s >= 0.55 - 1e-6);
        }
        assert_eq!(frames, 3);
        // One long frame does not overshoot.
        assert_eq!(ease(0.55, 1.0, 1.0), 1.0);
        // Half of EASE_SECS moves half the full range.
        assert!((ease(0.0, 1.0, EASE_SECS / 2.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn heading_and_pitch_point_at_the_target() {
        for (h, p) in [(0.3f32, 0.1f32), (2.5, -0.4), (5.9, 0.7)] {
            let d = crate::space::look_dir(h, p);
            let (hh, pp) = heading_pitch_to([10.0, 20.0, 30.0], [10.0 + d[0] * 500.0, 20.0 + d[1] * 500.0, 30.0 + d[2] * 500.0]);
            assert!((hh - h).abs() < 1e-4 && (pp - p).abs() < 1e-4, "{h} {p}: {hh} {pp}");
        }
    }

    /// The pull run for `secs` at 60 frames a second from 0, 0 toward `want`; the error left.
    fn pull_for(want: (f32, f32), strength: f32, secs: f32) -> (f32, f32) {
        let (mut h, mut p) = (0.0f32, 0.0f32);
        let dt = 1.0 / 60.0;
        for _ in 0..(secs / dt).round() as usize {
            let (dh, dp) = pull_step(h, p, want.0, want.1, strength, dt);
            assert!(dh.hypot(dp) <= MAX_PULL_RATE * strength * dt + 1e-6, "capped turn");
            h += dh;
            p += dp;
        }
        (crate::control::wrap_angle(want.0 - h), want.1 - p)
    }

    #[test]
    fn the_pull_eases_onto_the_target_within_the_lock() {
        // 8° off at full strength: within a quarter of a degree by the lock's end.
        let want = (6f32.to_radians(), 5.3f32.to_radians());
        let e = pull_for(want, 1.0, LOCK_SECS);
        assert!(e.0.hypot(e.1).to_degrees() < 0.25, "{e:?}");
        // Never past it.
        assert!(e.0 >= -1e-6 && e.1 >= -1e-6);
        // Weaker pulls leave more.
        let weak = pull_for(want, 0.3, LOCK_SECS);
        assert!(weak.0.hypot(weak.1) > e.0.hypot(e.1) * 5.0, "{weak:?}");
        // It eases: the first frame turns less than an even share would.
        let (dh, dp) = pull_step(0.0, 0.0, want.0, want.1, 1.0, 1.0 / 60.0);
        assert!(dh.hypot(dp) < want.0.hypot(want.1));
    }

    #[test]
    fn the_pull_takes_the_short_way_round_and_is_capped() {
        // From 350° to 10°: turns up through 0, not down.
        let (dh, _) = pull_step(350f32.to_radians(), 0.0, 10f32.to_radians(), 0.0, 1.0, 1.0 / 60.0);
        assert!(dh > 0.0);
        // A far target turns at the capped rate.
        let (dh, dp) = pull_step(0.0, 0.0, 1.5, 0.0, 0.5, 0.1);
        assert!((dh - MAX_PULL_RATE * 0.5 * 0.1).abs() < 1e-5 && dp == 0.0);
        assert_eq!(pull_step(0.0, 0.0, 1.0, 0.0, 0.0, 0.1), (0.0, 0.0));
    }

    #[test]
    fn a_hard_push_against_the_pull_breaks_the_lock() {
        let right = (0.05, 0.0);
        // Pushing left, hard, against a pull to the right.
        assert!(breaks_lock(Vec2::new(-0.9, 0.0), right, false));
        // Gently: holds.
        assert!(!breaks_lock(Vec2::new(-0.5, 0.0), right, false));
        // With the pull: holds.
        assert!(!breaks_lock(Vec2::new(0.9, 0.0), right, false));
        // Up against a pull down; with Invert Y, up turns down, with the pull.
        let down = (0.0, -0.05);
        assert!(breaks_lock(Vec2::new(0.0, 0.9), down, false));
        assert!(!breaks_lock(Vec2::new(0.0, 0.9), down, true));
    }

    /// A wall box of half size `half` at `at`.
    fn wall(world: &mut fnv_phys::PhysWorld, id: u32, at: [f32; 3], half: f32) {
        use fnv_collide::{CollisionObject, Iso, Motion, Shape};
        let wall = CollisionObject {
            node: None,
            node_block: 0,
            node_transform: Iso::IDENTITY,
            body_transform: Iso::IDENTITY,
            motion: Motion::Static,
            layer: layers::STATIC,
            mass: 0.0,
            friction: 0.5,
            restitution: 0.0,
            shape: Shape::Box { half_extents: [half; 3] }.into(),
            shell: 0.0,
            material: None,
            wind: false,
        };
        world.add_ref(&fnv_phys::RefPose { form_id: id, position: at, rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], scale: 1.0 }, &[wall]);
    }

    /// The player at the origin (eye 100) and NPC 2 walking 300 units east: its capsule
    /// (radius 22) around a thinner hit volume (radius 15), as physics-walked NPCs have.
    fn sight_scene() -> (fnv_phys::PhysWorld, Key, Candidate) {
        let mut world = fnv_phys::PhysWorld::new(fnv_phys::PhysParams::default());
        let me = world.add_character(1, fnv_phys::CharacterParams::default(), [0.0, 0.0, 0.0]);
        world.add_character(2, fnv_phys::CharacterParams::default(), [300.0, 0.0, 0.0]);
        world.set_actor_volume(2, fnv_phys::HitVolume { feet: [300.0, 0.0, 0.0], radius: 15.0, height: 120.0, dead: false });
        let (angle, distance) = angle_off([0.0, 0.0, 100.0], [1.0, 0.0, 0.0], [300.0, 0.0, 80.0]);
        (world, me.key(), Candidate { form_id: 2, aim: [300.0, 0.0, 80.0], distance, angle })
    }

    #[test]
    fn the_sight_ray_passes_walking_capsules_to_the_body() {
        let (mut world, me, c) = sight_scene();
        let filter = sight_filter(Some(me), [2u32].into_iter());
        let q = world.queries();
        let to = [c.aim[0], c.aim[1], c.aim[2] - 100.0];
        // The capsule is passed: the ray meets the hit volume.
        let hit = q.ray([0.0, 0.0, 100.0], to, c.distance + FRICTION_RADIUS, &filter).map(|h| h.owner);
        assert!(matches!(hit, Some(Owner::Actor { form_id: 2, .. })), "{hit:?}");
        assert!(seen(&q, &filter, [0.0, 0.0, 100.0], &c));
        // Without the capsules excluded the ray meets the capsule first; still the target.
        let bare = sight_filter(Some(me), std::iter::empty());
        assert!(matches!(q.ray([0.0, 0.0, 100.0], to, c.distance + FRICTION_RADIUS, &bare).map(|h| h.owner), Some(Owner::Character { id: 2 })));
        assert!(seen(&q, &bare, [0.0, 0.0, 100.0], &c));
    }

    #[test]
    fn another_npcs_capsule_does_not_hide_the_target_but_a_wall_does() {
        let (mut world, me, c) = sight_scene();
        world.add_character(3, fnv_phys::CharacterParams::default(), [150.0, 0.0, 0.0]);
        let filter = sight_filter(Some(me), [2u32, 3].into_iter());
        assert!(seen(&world.queries(), &filter, [0.0, 0.0, 100.0], &c));
        wall(&mut world, 0x100, [200.0, 0.0, 90.0], 30.0);
        assert!(!seen(&world.queries(), &filter, [0.0, 0.0, 100.0], &c));
    }

    #[test]
    fn the_sight_filter_passes_what_a_shot_passes() {
        let f = sight_filter(Some(Key::Character(1)), [7u32].into_iter());
        assert_eq!(f.exclude_layers, layers::shot_ignores());
        assert!(f.exclude.contains(&Key::Character(1)) && f.exclude.contains(&Key::Character(7)));
        assert!(!f.exclude.contains(&Key::Actor(7)), "the hit volume is the body");
    }

    #[test]
    fn not_while_anything_else_has_the_look() {
        let ok = Guards { on: true, controller: true, playing: true, grabbed: true, ..Default::default() };
        assert!(ok.usable());
        let blocked = [
            Guards { on: false, ..ok },
            Guards { controller: false, ..ok },
            Guards { playing: false, ..ok },
            Guards { grabbed: false, ..ok },
            Guards { looking_locked: true, ..ok },
            Guards { wheel: true, ..ok },
            Guards { orbit: true, ..ok },
            Guards { free_cam: true, ..ok },
            Guards { kill_cam: true, ..ok },
            Guards { vats: true, ..ok },
            Guards { driven: true, ..ok },
            Guards { seated: true, ..ok },
        ];
        for g in blocked {
            assert!(!g.usable(), "{g:?}");
        }
    }

    #[test]
    fn on_by_default_with_both_parts() {
        let a = AimAssist::default();
        assert!(a.on && a.slowdown > 0.0 && a.lock > 0.0);
        assert_eq!(AimAssistState::default().look_scale, 1.0);
    }
}
