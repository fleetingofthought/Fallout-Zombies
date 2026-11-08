//! Kill cameras (docs/design/vats-mode.md §19): when the last marked shot of a VATS-mode
//! run kills its actor, the original's camera shots (CAMS) chosen by its camera paths
//! (CPTH) play, as VATS playback's kill cameras do [documented: GECK wiki CameraShot,
//! "cameras for VATS playback"; the paths and shots are the masters' own].
//!
//! - **When** [design: VATS mode fires its marks in one run (§9), so a kill camera only
//!   follows the run's last mark, not to cut the run short]: from the frame every mark's
//!   attack has started until [`WATCH_AFTER`] game seconds after the mode is off, the last
//!   mark's actor is watched; if it dies, the kill camera starts. An actor already dead
//!   when watched is not.
//! - **Which** (`fnv_game::camera_paths`): the masters' path tree, its conditions run on
//!   the player with GetVATSValue answering about the last shot, known to be fatal
//!   (`Game::kill_outcome`), and the VATS area functions on [`CameraRoom`] measured here
//!   by rays ([`measure_room`]). The shots the path starts on the hit play in turn
//!   (`camera_paths::hit_shots`).
//! - **A shot**: plays for its Max Time in real seconds, no less than [`SHOT_LEAST`]
//!   [hypothesis: "the max time the camera will playback before switching to the next
//!   camera", GECK wiki CameraShot; with nothing to cut to sooner, the most]; the world's
//!   clock runs at its Global time multiplier (the player's and the target's own are not
//!   applied: one clock for the world) and its image space modifier (MNAM) plays.
//! - **Where the camera is** [hypothesis: the shot models hold no reference frame; this is
//!   the frame in which their cameras stand where the shot names say, a front shot in
//!   front of the target and so on; observed on `simplefronthitl03.nif`, at (−59, −226,
//!   18) for a front hit, and `snipercamhit01.nif`]: the model's animated root, in a frame
//!   centred on the actor the shot is held by (its Location: the target or the attacker;
//!   a projectile counts as the attacker) at its middle (the aim point), with +Y the level
//!   direction from the attacker to the target and +Z up. With Position Follows Location
//!   the frame follows the actor, else it stays where it was when the shot began. The
//!   camera looks at its Target actor's middle, moved Target % Between Actors of the way
//!   to the other actor, followed while Rotation Follows Target is set, else as at the
//!   start; the model's own rotation is added as a sway (its turn since its first key). A
//!   First Person Camera shot leaves the view where it is. The camera is kept in front of
//!   what lies between the look point and it.
//! - While it plays the view counts as a third-person one (`PlayerControl::cinematic`):
//!   the player's body is drawn and the first-person arms are not.
//! - It ends early when the player dies, a menu opens or the game is not playing.
//! - **Only the last enemy** [owner's playtest request #1035; design: the original plays
//!   one on any killing VATS shot, hypothesis; combat is not a parity target]: with the
//!   settings page's Kill camera at Last enemy (the default, [`KillCamSetting`]), a kill
//!   that leaves another living actor in combat with the player ([`enemies_in_combat`],
//!   the player's IsInCombat rule: an actor in a combat state, Alert, Combat or Lost, with
//!   the player on its target or alert list) shows none, and the view stays the player's.
//!   Every kill shows one whoever is left; Off never shows one (#1037).
//! - **While it plays** (#1034, #1036) [design]: the HUD, the crosshair and the corner
//!   and quest notices are hidden (`hud::gather`, `reticle::draw`, the tile HUD,
//!   `corner_messages`, `quest_messages`; the notices wait, as under a message box), and
//!   the gameplay controls are held: the keys and buttons read through
//!   `bindings::ControlInput` other than Menu Mode, Wait, Quick Save and Quick Load (so
//!   the game can still be paused or a menu opened, which ends the camera), the mouse and
//!   stick look (`player::mouse_look`, the look test aid), the weapon wheel, and what the
//!   frame asked of the character: moving, jumping, the trigger, aiming, reload and ammo
//!   swap ([`hold_input`], which the test aids that stand in for the keys go through). A
//!   fire button held when it starts must be pressed again after it (the trigger needs a
//!   fresh press, `control::read_controls`).
//!
//! Log lines: `kill camera: <name> <ref> killed by the last mark: path <EDID> (<FormID>),
//! shots <EDID>, …` when it starts (or `kill camera: <ref> killed by the last mark: no
//! camera path chosen`), `kill camera: skipped, <n> enemies still in combat (…)` or `kill
//! camera: off in the settings` for a kill that shows none, `kill camera: shot <k> of <n>
//! <EDID>: …` for each shot, and `kill camera: done (…), input held <n> frames` when it
//! ends; at exit `kill camera summary: <n> shown, <n> skipped, <n> off`.

use crate::control::PlayerControl;
use crate::player::MoveIntent;
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::ScriptState;
use crate::vats_mode::{Phase, VatsMode};
use crate::{space, PlayState, PlayerCamera};
use bevy::prelude::*;
use fnv_game::camera_paths::{self, CameraShotDef, ShotActor, FIRST_PERSON_CAMERA, POSITION_FOLLOWS_LOCATION, ROTATION_FOLLOWS_TARGET};
use fnv_game::vats::{CameraRoom, VatsShot};
use fnv_sim::FormId;
use nvf_assets::camera_shot::CameraModel;
use nvf_assets::model::ModelCtx;
use std::sync::Arc;

/// Game seconds after VATS mode is off that the last mark's actor is still watched (a
/// shot still in flight) [design].
pub const WATCH_AFTER: f32 = 2.0;
/// The least real seconds a shot plays [design].
pub const SHOT_LEAST: f32 = 0.5;
/// The most real seconds a shot plays [design: no shot of the masters asks for more than
/// 10].
pub const SHOT_MOST: f32 = 10.0;
/// How far the room about the actors is measured, units [design: past the largest
/// distance the masters' paths compare with, 320, observed].
pub const ROOM_REACH: f32 = 1024.0;
/// The camera keeps this far in front of what it would go into, units [design].
pub const WALL_MARGIN: f32 = 8.0;
/// The slowest the world's clock is taken [design].
pub const SLOWEST: f32 = 0.02;

/// A shot playing.
struct Playing {
    def: CameraShotDef,
    model: Option<Arc<CameraModel>>,
    /// Real seconds it has played, and for how long it plays.
    real: f32,
    length: f32,
    /// The frame's origin and +Y when it began (game space), the look point then, and the
    /// model's first turn.
    origin: Vec3,
    forward: Vec3,
    look: Vec3,
    first_turn: Quat,
    /// The image space modifier it applied.
    imod: Option<FormId>,
}

/// A kill camera under way.
struct Show {
    target: FormId,
    shots: Vec<FormId>,
    next: usize,
    playing: Option<Playing>,
    real: f32,
    /// Frames in which [`hold_input`] struck something the player asked for.
    held: u32,
}

/// The last mark's actor being watched.
struct Watch {
    shot: VatsShot,
    /// Game seconds since VATS mode went off (while it is on, 0).
    since_off: f32,
}

#[derive(Resource, Default)]
pub struct KillCam {
    watch: Option<Watch>,
    show: Option<Show>,
    /// Kill cameras shown, and kills that showed none (others in combat; the setting
    /// off), for the exit report.
    shown: u32,
    skipped: u32,
    off: u32,
}

/// When kill cameras play: the settings page's Kill camera (`kill-camera` in
/// NVForever.ini) [owner's playtest requests #1035, #1037].
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KillCamSetting {
    /// Only for a kill that leaves no other actor in combat with the player.
    #[default]
    LastEnemy,
    /// For every kill by the last mark.
    EveryKill,
    /// Never.
    Off,
}

impl KillCamSetting {
    pub const ALL: [KillCamSetting; 3] = [KillCamSetting::LastEnemy, KillCamSetting::EveryKill, KillCamSetting::Off];

    pub fn name(self) -> &'static str {
        match self {
            KillCamSetting::LastEnemy => "Last enemy",
            KillCamSetting::EveryKill => "Every kill",
            KillCamSetting::Off => "Off",
        }
    }

    /// The setting of this name, any case.
    pub fn by_name(name: &str) -> Option<KillCamSetting> {
        KillCamSetting::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(name.trim()))
    }
}

fn setting_pick(w: &mut World, name: &str) -> bool {
    match KillCamSetting::by_name(name) {
        Some(s) => {
            *w.resource_mut::<KillCamSetting>() = s;
            true
        }
        None => false,
    }
}

/// The living actors other than `killed` in combat with the player: in a combat state
/// (Alert, Combat, Lost) with the player on their target or alert list, as the player's
/// IsInCombat counts them (`fnv_game::awareness::is_in_combat`).
pub fn enemies_in_combat(g: &fnv_game::Game, killed: FormId) -> Vec<FormId> {
    let player = FormId::PLAYER_REF;
    g.awareness
        .actors
        .iter()
        .filter(|(id, a)| **id != killed && **id != player && a.state.in_combat() && (a.targets.contains_key(&player) || a.alarms.iter().any(|x| x.owner == player)))
        .map(|(id, _)| *id)
        .filter(|id| !g.is_dead(*id))
        .collect()
}

impl KillCam {
    /// A kill camera has the view.
    pub fn active(&self) -> bool {
        self.show.is_some()
    }

    /// The world clock's speed it asks for (`script::time_multiplier`): the playing
    /// shot's Global time multiplier, else 1.
    pub fn time_scale(&self) -> Option<f32> {
        let p = self.show.as_ref()?.playing.as_ref()?;
        Some(if p.def.time_global > 0.0 { p.def.time_global.clamp(SLOWEST, 1.0) } else { 1.0 })
    }
}

/// How long a shot plays, real seconds.
pub fn shot_length(def: &CameraShotDef) -> f32 {
    def.max_time.max(def.min_time).clamp(SHOT_LEAST, SHOT_MOST)
}

/// The level unit direction from `from` to `to` (game space); +Y when they stand one
/// over the other.
pub fn level_forward(from: Vec3, to: Vec3) -> Vec3 {
    let d = Vec3::new(to.x - from.x, to.y - from.y, 0.0);
    if d.length_squared() < 1e-6 {
        Vec3::Y
    } else {
        d.normalize()
    }
}

/// A point of the shot's frame (+Y `forward`, +Z up, about `origin`) in game space.
pub fn in_frame(origin: Vec3, forward: Vec3, p: Vec3) -> Vec3 {
    let right = forward.cross(Vec3::Z);
    origin + right * p.x + forward * p.y + Vec3::Z * p.z
}

/// The four sides of the line from the attacker to the target, level: right, left, back,
/// front (the order of [`CameraRoom`]).
pub fn sides(forward: Vec3) -> [Vec3; 4] {
    let right = forward.cross(Vec3::Z);
    [right, -right, -forward, forward]
}

fn ray_filter() -> fnv_phys::Filter {
    fnv_phys::Filter {
        exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()),
        no_dynamic: true,
        ..Default::default()
    }
}

/// How far the way is free from `from` along `dir`, up to [`ROOM_REACH`].
fn free(phys: &mut PhysState, from: Vec3, dir: Vec3) -> f32 {
    phys.ray(from.to_array(), dir.to_array(), ROOM_REACH, ray_filter()).map_or(ROOM_REACH, |h| h.distance)
}

/// The room about the attacker and the target for the VATS area functions.
fn measure_room(phys: &mut PhysState, attacker: Vec3, target: Vec3) -> CameraRoom {
    let s = sides(level_forward(attacker, target));
    CameraRoom { area_free: s.map(|d| free(phys, attacker, d)), target_visible: s.map(|d| free(phys, target, d)) }
}

/// `want`, or short of it in front of what lies between `from` and it (game space).
fn clear(phys: &mut PhysState, from: Vec3, want: Vec3) -> Vec3 {
    let d = want - from;
    let len = d.length();
    if len < 1e-3 {
        return want;
    }
    match phys.ray(from.to_array(), (d / len).to_array(), len + WALL_MARGIN, ray_filter()) {
        Some(h) => from + d / len * (h.distance - WALL_MARGIN).clamp(0.0, len),
        None => want,
    }
}

/// The player's middle (game space): halfway from the feet to the eye.
fn player_middle(p: &Player) -> Vec3 {
    (Vec3::from_array(p.feet) + Vec3::from_array(p.eye())) * 0.5
}

/// An actor's middle (game space), the player's for the player.
fn middle(phys: &PhysState, player: &Player, actor: FormId) -> Option<Vec3> {
    if actor == FormId::PLAYER_REF {
        return Some(player_middle(player));
    }
    phys.world.actor_aim_point(actor.0).map(Vec3::from_array)
}

/// Watch the last mark, start and run the kill camera.
#[allow(clippy::too_many_arguments)]
fn update(
    (real, virt): (Res<Time<Real>>, Res<Time<Virtual>>),
    state: Res<State<PlayState>>,
    vats: Res<VatsMode>,
    mut cam: ResMut<KillCam>,
    mut script: Option<ResMut<ScriptState>>,
    mut phys: ResMut<PhysState>,
    player: Res<Player>,
    mut control: ResMut<PlayerControl>,
    mut models: ModelCtx,
    setting: Option<Res<KillCamSetting>>,
) {
    let setting = setting.map_or_else(KillCamSetting::default, |s| *s);
    let cam = &mut *cam;
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let playing = *state.get() == PlayState::Playing;
    let player_dead = rt.game.is_dead(FormId::PLAYER_REF);
    // The last mark: watched from the frame its attack starts.
    if let Some((shot, _)) = vats.final_shot(&player) {
        let same = cam.watch.as_ref().is_some_and(|w| w.shot.target == shot.target);
        if !same && cam.show.is_none() && !rt.game.is_dead(shot.target) {
            cam.watch = Some(Watch { shot, since_off: 0.0 });
        } else if let Some(w) = cam.watch.as_mut().filter(|_| same) {
            w.shot = shot;
        }
    } else if let Some(w) = cam.watch.as_mut() {
        if vats.phase == Phase::Off {
            w.since_off += virt.delta_secs();
        }
        if w.since_off > WATCH_AFTER || vats.phase == Phase::Marking {
            cam.watch = None;
        }
    }
    // A kill: whether the setting and the fight leave room for a camera.
    let mut killed = cam.watch.as_ref().filter(|w| cam.show.is_none() && rt.game.is_dead(w.shot.target)).map(|w| w.shot);
    if let Some(shot) = killed {
        cam.watch = None;
        let others = enemies_in_combat(&rt.game, shot.target);
        let skip = match setting {
            KillCamSetting::Off => {
                info!("kill camera: off in the settings");
                cam.off += 1;
                true
            }
            KillCamSetting::LastEnemy if !others.is_empty() => {
                let names: Vec<String> = others.iter().map(|o| format!("{} {o}", rt.game.data.name(*o))).collect();
                info!("kill camera: skipped, {} enemies still in combat ({})", others.len(), names.join(", "));
                cam.skipped += 1;
                true
            }
            _ => false,
        };
        if skip {
            killed = None;
        }
    }
    // Choose the path.
    if let Some(shot) = killed {
        let name = rt.game.data.name(shot.target);
        let attacker = player_middle(&player);
        let room = match middle(&phys, &player, shot.target) {
            Some(t) => measure_room(&mut phys, attacker, t),
            None => CameraRoom::default(),
        };
        let outcome = rt.game.kill_outcome(shot.target);
        let path = rt.game.pick_camera_path(VatsShot { success: true, ..shot }, outcome, room);
        let shots = path.and_then(|p| rt.game.data.camera_paths.get(&p)).map(|p| camera_paths::hit_shots(p, &rt.game.data.camera_shots)).unwrap_or_default();
        match path.filter(|_| !shots.is_empty()) {
            Some(p) => {
                let names: Vec<String> = shots.iter().map(|s| rt.game.data.name(*s)).collect();
                info!(
                    "kill camera: {name} {} killed by the last mark: path {} ({p}), shots {}; room right/left/back/front {:.0}/{:.0}/{:.0}/{:.0}, target seen {:.0}/{:.0}/{:.0}/{:.0}{}",
                    shot.target,
                    rt.game.data.name(p),
                    names.join(", "),
                    room.area_free[0],
                    room.area_free[1],
                    room.area_free[2],
                    room.area_free[3],
                    room.target_visible[0],
                    room.target_visible[1],
                    room.target_visible[2],
                    room.target_visible[3],
                    if outcome.dismember || outcome.explode { ", a part is gone" } else { "" }
                );
                cam.show = Some(Show { target: shot.target, shots, next: 0, playing: None, real: 0.0, held: 0 });
                cam.shown += 1;
            }
            None => info!("kill camera: {} killed by the last mark: no camera path chosen", shot.target),
        }
    }
    let Some(show) = cam.show.as_mut() else {
        control.cinematic = false;
        return;
    };
    // Cut short.
    let stop = if player_dead {
        Some("the player died")
    } else if !playing {
        Some("a menu opened")
    } else {
        None
    };
    let dt = if virt.is_paused() { 0.0 } else { real.delta_secs() };
    show.real += dt;
    let finished = match (&mut show.playing, stop) {
        (_, Some(_)) => true,
        (Some(p), None) => {
            p.real += dt;
            p.real >= p.length
        }
        (None, None) => true,
    };
    if finished {
        if let Some(p) = show.playing.take() {
            if let Some(m) = p.imod {
                rt.game.remove_image_modifier(m);
            }
        }
        let next = show.shots.get(show.next).copied().filter(|_| stop.is_none());
        // A shot a script changed (SetCameraShot…, `nvse_jg5`) plays as changed.
        match next.and_then(|s| fnv_game::nvse_jg5::camera_shot(&rt.game, s).map(|d| (s, d.clone()))) {
            Some((s, def)) => {
                show.next += 1;
                let model = def.model.as_deref().filter(|_| !def.has(FIRST_PERSON_CAMERA)).and_then(|m| models.camera_model(&format!("meshes\\{m}")));
                let attacker = player_middle(&player);
                let target = middle(&phys, &player, show.target).unwrap_or(attacker);
                let forward = level_forward(attacker, target);
                let origin = match def.location {
                    ShotActor::Target => target,
                    ShotActor::Attacker | ShotActor::Projectile => attacker,
                };
                let look = look_point(&def, attacker, target);
                let first_turn = model.as_ref().map_or(Quat::IDENTITY, |m| m.root_at(0.0).1);
                if let Some(m) = def.imod {
                    rt.game.apply_image_modifier(m);
                }
                let length = shot_length(&def);
                info!(
                    "kill camera: shot {} of {} {}: {}, held by the {:?}, looking at the {:?}, {length:.1} s at world speed {:.2}{}",
                    show.next,
                    show.shots.len(),
                    rt.game.data.name(s),
                    match (&model, def.has(FIRST_PERSON_CAMERA)) {
                        (_, true) => "first person".to_string(),
                        (Some(_), _) => def.model.clone().unwrap_or_default(),
                        (None, _) => format!("{} not loaded, the view stays", def.model.clone().unwrap_or_default()),
                    },
                    def.location,
                    def.target,
                    if def.time_global > 0.0 { def.time_global.clamp(SLOWEST, 1.0) } else { 1.0 },
                    def.imod.map_or_else(String::new, |m| format!(", image space modifier {}", rt.game.data.name(m))),
                );
                let imod = def.imod;
                show.playing = Some(Playing { def, model, real: 0.0, length, origin, forward, look, first_turn, imod });
            }
            None => {
                info!("kill camera: done ({:.1} s{}), input held {} frames", show.real, stop.map_or_else(String::new, |w| format!("; {w}")), show.held);
                cam.show = None;
            }
        }
    }
    control.cinematic = cam.show.as_ref().is_some_and(|s| s.playing.as_ref().is_some_and(|p| p.model.is_some()));
}

/// Where a shot looks (game space): its Target actor's middle, moved Target % Between
/// Actors of the way to the other actor.
pub fn look_point(def: &CameraShotDef, attacker: Vec3, target: Vec3) -> Vec3 {
    let (base, other) = match def.target {
        ShotActor::Target => (target, attacker),
        ShotActor::Attacker | ShotActor::Projectile => (attacker, target),
    };
    base + (other - base) * (def.target_pct / 100.0).clamp(0.0, 1.0)
}

/// Put the camera where the playing shot holds it.
fn place(
    cam: Res<KillCam>,
    player: Res<Player>,
    mut phys: ResMut<PhysState>,
    mut cams: Query<&mut Transform, With<PlayerCamera>>,
) {
    let Some(show) = cam.show.as_ref() else { return };
    let Some(p) = show.playing.as_ref() else { return };
    let Some(model) = p.model.as_ref() else { return };
    let Ok(mut t) = cams.single_mut() else { return };
    let attacker = player_middle(&player);
    let target = middle(&phys, &player, show.target).unwrap_or(attacker);
    let (origin, forward) = if p.def.has(POSITION_FOLLOWS_LOCATION) {
        let o = match p.def.location {
            ShotActor::Target => target,
            ShotActor::Attacker | ShotActor::Projectile => attacker,
        };
        (o, p.forward)
    } else {
        (p.origin, p.forward)
    };
    let look = if p.def.has(ROTATION_FOLLOWS_TARGET) { look_point(&p.def, attacker, target) } else { p.look };
    let (at, turn) = model.root_at(p.real);
    let want = in_frame(origin, forward, at);
    let eye = clear(&mut phys, look, want);
    // The model's sway: its turn since its first key, about the frame's axes.
    let sway = turn * p.first_turn.inverse();
    let (axis, angle) = sway.to_axis_angle();
    let frame_axis = in_frame(Vec3::ZERO, forward, axis);
    let sway_bevy = if angle.abs() > 1e-5 && frame_axis.length_squared() > 1e-8 { Quat::from_axis_angle(space::to_bevy(frame_axis.to_array()).normalize(), angle) } else { Quat::IDENTITY };
    let from = space::to_bevy(eye.to_array());
    let to = space::to_bevy(look.to_array());
    if (to - from).length_squared() < 1e-4 {
        return;
    }
    let base = Transform::from_translation(from).looking_at(to, Vec3::Y);
    let out = Transform { translation: from, rotation: (sway_bevy * base.rotation).normalize(), scale: Vec3::ONE };
    if out.translation.is_finite() && out.rotation.is_finite() {
        *t = out;
    }
}

/// Strike from what the frame asked of the character (the keys, held already by
/// `bindings::ControlInput`, and the test aids that stand in for them) what a playing
/// kill camera holds (module notes): moving, jumping, the trigger, aiming, reload and
/// ammo swap. After `control::enforce_locks`, before anything acts on it.
pub fn hold_input(mut cam: ResMut<KillCam>, mut intent: ResMut<MoveIntent>, mut control: ResMut<PlayerControl>) {
    let Some(show) = cam.show.as_mut() else { return };
    let (mut i, mut c) = (*intent, *control);
    (i.forward, i.right, i.jump) = (0.0, 0.0, false);
    (c.trigger, c.aiming, c.reload, c.ammo_swap, c.ready_held) = (false, false, false, false, None);
    if (i, c) != (*intent, *control) {
        show.held += 1;
    }
    intent.set_if_neq(i);
    control.set_if_neq(c);
}

fn report(mut exit: MessageReader<AppExit>, cam: Res<KillCam>) {
    if exit.read().last().is_none() {
        return;
    }
    info!("kill camera summary: {} shown, {} skipped, {} off", cam.shown, cam.skipped, cam.off);
}

pub struct KillCamPlugin;

impl Plugin for KillCamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KillCam>()
            .init_resource::<KillCamSetting>()
            .add_systems(Update, update.in_set(crate::PlaySet::Activation).after(crate::vats_mode::VatsUpdate).run_if(resource_exists::<PhysState>))
            .add_systems(Update, hold_input.after(crate::control::enforce_locks).after(crate::PlaySet::Input).before(crate::PlaySet::Physics))
            .add_systems(PostUpdate, place.before(bevy::transform::TransformSystems::Propagate).run_if(resource_exists::<PhysState>))
            .add_systems(Last, report);
        crate::settings::add_choice_list(
            app,
            crate::settings::Setting {
                key: "kill-camera",
                group: crate::pause::GROUP_MENUS,
                label: "Kill camera",
                note: "When a VATS shot kills its last mark, a cinematic camera can show the kill. Last enemy: only when no other enemy is still fighting you. Every kill: whoever is left. Off: never.",
                get: |_| false,
                set: |_, _| {},
                slider: None,
            },
            crate::settings::ChoiceList {
                names: |_| KillCamSetting::ALL.iter().map(|s| s.name().to_string()).collect(),
                chosen: |w| w.resource::<KillCamSetting>().name().to_string(),
                pick: setting_pick,
                save: |w| w.resource::<KillCamSetting>().name().to_string(),
                load: setting_pick,
                ask_keep: false,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_game::camera_paths::ShotAction;

    fn def(location: ShotActor, target: ShotActor, pct: f32) -> CameraShotDef {
        CameraShotDef {
            model: None,
            action: ShotAction::Hit,
            location,
            target,
            flags: 7,
            time_player: 4.0,
            time_target: 3.0,
            time_global: 0.25,
            max_time: 3.0,
            min_time: 1.0,
            target_pct: pct,
            imod: None,
        }
    }

    #[test]
    fn the_frame_faces_from_the_attacker_to_the_target() {
        let (a, t) = (Vec3::new(0.0, 0.0, 60.0), Vec3::new(1000.0, 0.0, 70.0));
        let f = level_forward(a, t);
        assert!((f - Vec3::X).length() < 1e-6, "{f}");
        // A front shot's camera (−59, −226, 18) about the target stands between it and
        // the attacker, a little to the left of the line.
        let cam = in_frame(t, f, Vec3::new(-59.0, -226.0, 18.0));
        assert!((cam - Vec3::new(774.0, 59.0, 88.0)).length() < 1e-3, "{cam}");
        let [right, left, back, front] = sides(f);
        assert!((right - Vec3::new(0.0, -1.0, 0.0)).length() < 1e-6 && (left + right).length() < 1e-6);
        assert!((back + Vec3::X).length() < 1e-6 && (front - Vec3::X).length() < 1e-6);
        // Standing one over the other: +Y.
        assert_eq!(level_forward(a, a + Vec3::Z * 5.0), Vec3::Y);
    }

    #[test]
    fn the_look_point_moves_towards_the_other_actor() {
        let (a, t) = (Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0));
        assert_eq!(look_point(&def(ShotActor::Target, ShotActor::Target, 0.0), a, t), t);
        assert_eq!(look_point(&def(ShotActor::Target, ShotActor::Target, 50.0), a, t), Vec3::new(50.0, 0.0, 0.0));
        assert_eq!(look_point(&def(ShotActor::Attacker, ShotActor::Attacker, 0.0), a, t), a);
    }

    #[test]
    fn kill_camera_settings_are_known_by_their_names() {
        assert_eq!(KillCamSetting::default(), KillCamSetting::LastEnemy);
        for s in KillCamSetting::ALL {
            assert_eq!(KillCamSetting::by_name(s.name()), Some(s));
        }
        assert_eq!(KillCamSetting::by_name(" every KILL "), Some(KillCamSetting::EveryKill));
        assert_eq!(KillCamSetting::by_name("On"), None);
    }

    #[test]
    fn a_playing_kill_camera_holds_the_frame_s_requests() {
        let mut app = App::new();
        app.init_resource::<KillCam>().init_resource::<MoveIntent>().init_resource::<PlayerControl>().add_systems(Update, hold_input);
        let ask = |app: &mut App| {
            *app.world_mut().resource_mut::<MoveIntent>() = MoveIntent { forward: 1.0, right: -1.0, jump: true, ..Default::default() };
            let mut c = app.world_mut().resource_mut::<PlayerControl>();
            (c.weapon_out, c.trigger, c.aiming, c.reload) = (true, true, true, true);
        };
        // No camera: nothing struck.
        ask(&mut app);
        app.update();
        assert_eq!(app.world().resource::<MoveIntent>().forward, 1.0);
        assert!(app.world().resource::<PlayerControl>().trigger);
        app.world_mut().resource_mut::<KillCam>().show = Some(Show { target: FormId(0x1234), shots: Vec::new(), next: 0, playing: None, real: 0.0, held: 0 });
        ask(&mut app);
        app.update();
        let (i, c) = (*app.world().resource::<MoveIntent>(), *app.world().resource::<PlayerControl>());
        assert_eq!((i.forward, i.right, i.jump), (0.0, 0.0, false));
        assert!(!c.trigger && !c.aiming && !c.reload && c.weapon_out, "the weapon stays out");
        // A frame that asks nothing is not counted.
        app.update();
        assert_eq!(app.world().resource::<KillCam>().show.as_ref().map(|s| s.held), Some(1));
    }

    #[test]
    fn a_shot_plays_its_max_time_within_bounds() {
        assert_eq!(shot_length(&def(ShotActor::Target, ShotActor::Target, 0.0)), 3.0);
        let mut d = def(ShotActor::Target, ShotActor::Target, 0.0);
        d.max_time = 0.01;
        d.min_time = 0.0;
        assert_eq!(shot_length(&d), SHOT_LEAST);
        d.max_time = 60.0;
        assert_eq!(shot_length(&d), SHOT_MOST);
    }
}
