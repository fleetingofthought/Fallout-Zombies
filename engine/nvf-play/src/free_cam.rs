//! The console's camera commands (docs/design/console.md, Camera commands): the free
//! camera (`tfc`), its speed (`sucsm`), the field of view (`fov`) and hiding the HUD
//! (`tm`). The console's lines run as script functions (`fnv_game::functions::camera`),
//! which leave their requests in `Game::ui.camera`; this module takes them.
//!
//! **The free camera** leaves the player where it stands and flies with the viewer's
//! keys (`nvforever`'s `fly.rs`): W/A/S/D, Q down, E up, Shift five times faster, the
//! mouse wheel changes the speed; the mouse looks round without a button. It flies only
//! while the console is closed and no other menu is open. While it flies the player's
//! controls do nothing ([`crate::bindings::ControlInput`] reads no control, mouse look
//! leaves the player's view alone) and the player is shown from outside (third person)
//! [hypothesis: the original's free camera shows the third-person model; not checked].
//! `tfc` again puts the camera back where `control::place_camera` puts it, in the view
//! the player had.
//!
//! Modes [the catalogue names three; what each does here is our choice]: 0 (none given)
//! and any mode but 1: the world goes on. 1: the world stands still as under a menu
//! (`PlayState::Menu`: no game logic, no game clock) and so does the picture (the
//! virtual clock, `crate::pause::freeze_picture`), the way the catalogue says mode 1
//! "freezes the game fully" [hypothesis: whether the original holds animations too].
//! The catalogue's mode 2 (the last frame shown until exit) and 3 (the view's pose kept
//! for occlusion) are not built: they fly as mode 0.
//!
//! Test aid `NVF_PLAY_TEST_FLY=<units per second>`: the free camera flies forward (level)
//! at that speed while it flies.
//!
//! **SUP's TFC functions** (`fnv_game::nvse_sup3`) read and set the camera's place in
//! `Game::ui.camera.tfc`: [`take_requests`] writes the camera's own place there when it
//! turns on and whenever the viewer has moved it since, and puts the camera where a
//! script set it (`tfc_set`). A place a script set is kept exactly: the pitch is held to
//! ±1.55 rad only while the mouse moves it.

use crate::console::Console;
use crate::control::{PlayerControl, Pov};
use crate::script::ScriptState;
use crate::{space, PlaySet, PlayState, PlayerCamera};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

/// The free camera's speed before the wheel and `sucsm`, units per second (the viewer's
/// starting speed, `nvforever` `main.rs`) [decision].
pub const FLY_SPEED: f32 = 800.0;
/// Radians the view turns per pixel of mouse motion (the viewer's, `fly.rs`) [decision].
const LOOK_PER_PIXEL: f32 = 0.003;

/// The free camera.
#[derive(Resource, Debug, Default)]
pub struct FreeCam {
    /// The camera has left the player.
    pub on: bool,
    /// The mode `tfc` was given.
    pub mode: i32,
    /// It flies this frame: on, the console closed, no other menu.
    pub active: bool,
    /// Bevy space.
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// The wheel's share of the speed.
    pub wheel: f32,
    /// Where it left the player, and the player's feet then (game space).
    start: Vec3,
    feet: [f32; 3],
    /// The view the player had.
    pov: Pov,
    /// `NVF_PLAY_TEST_FLY`.
    aid_speed: f32,
    /// This run is the editor (`nvforever --editor`): while `on`, the editor owns the
    /// camera and the pointer, and the world stands still; off, the player plays. The
    /// console's `tfc` does nothing here.
    pub editor: bool,
    /// The place last written to or taken from the game's TFC pose: position, yaw,
    /// pitch (Bevy). A different place is the viewer's move, written back.
    shared: Option<(Vec3, f32, f32)>,
}

/// The game's TFC pose as the free camera's position, yaw and pitch (Bevy space): the
/// heading turns the other way round Bevy's Y, and angle X looks down where Bevy's
/// pitch looks up (`space::look_rotation`).
fn from_tfc(t: &fnv_game::functions::camera::TfcPose) -> (Vec3, f32, f32) {
    let p = t.pos.map(|v| v as f32);
    (space::to_bevy(p), -(t.angle_z.to_radians() as f32), -(t.angle_x.to_radians() as f32))
}

/// The free camera's place as the game's TFC pose.
fn to_tfc(pos: Vec3, yaw: f32, pitch: f32) -> fnv_game::functions::camera::TfcPose {
    let p = space::to_game(pos);
    fnv_game::functions::camera::TfcPose {
        pos: p.map(f64::from),
        angle_x: -f64::from(pitch).to_degrees(),
        angle_z: -f64::from(yaw).to_degrees(),
    }
}
}

impl FreeCam {
    /// Mode 1: the world stands still while the camera is free.
    pub fn holds_world(&self) -> bool {
        self.on && self.mode == 1
    }

    /// The editor has the camera now (the player is not playing).
    pub fn editor_holds(&self) -> bool {
        self.editor && self.on
    }

    /// The editor takes the camera and holds the world, or lets the player play.
    pub fn editor_hold(&mut self, hold: bool, control: &mut PlayerControl) {
        self.editor = true;
        if hold == self.on {
            return;
        }
        (self.on, self.mode) = (hold, 1);
        if hold {
            self.pov = control.pov;
            control.pov = Pov::Third;
        } else {
            self.active = false;
            control.pov = self.pov;
        }
    }
}

/// The first-person field of view `fov` asked for, in the game's degrees (`None`: the
/// default); `nvforever`'s `player_body` draws the arms through it.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct FirstPersonFov(pub Option<f32>);

/// `tm`: the HUD, the crosshair and the corner messages are hidden.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct MenusHidden(pub bool);

pub struct FreeCamPlugin;

impl Plugin for FreeCamPlugin {
    fn build(&self, app: &mut App) {
        let aid_speed = std::env::var("NVF_PLAY_TEST_FLY").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
        app.insert_resource(FreeCam { wheel: 1.0, aid_speed, ..default() })
            .init_resource::<FirstPersonFov>()
            .init_resource::<MenusHidden>()
            .add_systems(Update, take_requests.after(PlaySet::Scripts).before(PlaySet::Ui))
            .add_systems(Update, fly.in_set(PlaySet::Physics).after(crate::control::place_camera))
            .add_systems(Update, hold_world.in_set(PlaySet::Sync))
            .add_systems(PostUpdate, hide_menus.before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate));
    }
}

/// The game's FOV setting (degrees across 4:3) as Bevy's vertical angle (radians), which
/// stays fixed at other aspects ("Hor+") [hypothesis; docs/spec-notes/camera.md].
pub fn vertical_fov(setting_deg: f32) -> f32 {
    2.0 * ((setting_deg.to_radians() / 2.0).tan() * 3.0 / 4.0).atan()
}

/// The console's requests: toggles of the free camera, the field of view, the HUD.
#[allow(clippy::too_many_arguments)]
fn take_requests(
    script: Option<ResMut<ScriptState>>,
    mut free: ResMut<FreeCam>,
    mut first: ResMut<FirstPersonFov>,
    mut hidden: ResMut<MenusHidden>,
    mut control: ResMut<PlayerControl>,
    player: Option<Res<crate::player::Player>>,
    mut cams: Query<(&Transform, &mut Projection), With<PlayerCamera>>,
    mut default_vfov: Local<Option<f32>>,
    talk: Option<Res<crate::dialogue_zoom::DialogueZoom>>,
) {
    let Some(mut st) = script else { return };
    let Some(rt) = st.rt.as_mut() else { return };
    let asked = &mut rt.game.ui.camera;
    hidden.0 = asked.menus_hidden;
    let toggles = std::mem::take(&mut asked.fly_toggles);
    let fov = asked.fov.take();
    let feet = player.map_or([0.0; 3], |p| p.feet);
    let Ok((t, mut proj)) = cams.single_mut() else { return };
    for mode in toggles {
        if free.editor {
            continue;
        }
        if !free.on {
            let (yaw, pitch, _) = t.rotation.to_euler(EulerRot::YXZ);
            let f = &mut *free;
            (f.on, f.mode, f.pos, f.yaw, f.pitch, f.start, f.feet, f.pov) = (true, mode, t.translation, yaw, pitch, t.translation, feet, control.pov);
            control.pov = Pov::Third;
            // The camera's own place, for the TFC getters [observed: sup3 cam.on.pos, the
            // eye; cam.on.angles, the view's angles].
            asked.tfc = to_tfc(free.pos, free.yaw, free.pitch);
            free.shared = Some((free.pos, free.yaw, free.pitch));
            let at = space::to_game(t.translation);
            let what = if free.holds_world() { "the world stands still" } else { "the world goes on" };
            info!("free camera: on (mode {mode}, {what}) at ({:.0}, {:.0}, {:.0})", at[0], at[1], at[2]);
        } else {
            free.on = false;
            free.active = false;
            control.pov = free.pov;
            let moved = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            info!(
                "free camera: off, {:.0} units from where it left the player (the player moved {:.0}); the camera is back on the player",
                free.pos.distance(free.start),
                moved(feet, free.feet)
            );
        }
    }
    if free.on {
        // A script's change wins over the viewer's move of the same frame.
        if std::mem::take(&mut asked.tfc_set) {
            let (pos, yaw, pitch) = from_tfc(&asked.tfc);
            (free.pos, free.yaw, free.pitch) = (pos, yaw, pitch);
            free.shared = Some((pos, yaw, pitch));
            let t = &asked.tfc;
            info!("free camera: a script put it at ({:.0}, {:.0}, {:.0}), angle X {:.1}°, heading {:.1}°", t.pos[0], t.pos[1], t.pos[2], t.angle_x, t.angle_z);
        } else if free.shared != Some((free.pos, free.yaw, free.pitch)) {
            asked.tfc = to_tfc(free.pos, free.yaw, free.pitch);
            free.shared = Some((free.pos, free.yaw, free.pitch));
        }
    }
    if let Some((world, first_deg)) = fov {
        if let Projection::Perspective(p) = &mut *proj {
            // Not a conversation's zoom: what lies under it (`crate::dialogue_zoom`).
            let base = *default_vfov.get_or_insert(talk.as_deref().map_or(p.fov, |t| t.underlying(p.fov)));
            p.fov = world.map_or(base, vertical_fov);
            first.0 = first_deg;
            let deg = |v: Option<f32>| v.map_or("the default".to_string(), |v| format!("{v:.0}°"));
            info!("camera: field of view {} (vertical {:.1}°), first person {}", deg(world), p.fov.to_degrees(), deg(first_deg));
        }
    }
}

/// Fly the free camera and put the view there; after `place_camera`, so it wins.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn fly(
    real: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    (state, console, paused, menu, loading): (Res<State<PlayState>>, Res<Console>, Res<crate::pause::Paused>, Res<crate::menu::ActiveMenu>, Res<crate::loading::Loading>),
    script: Option<Res<ScriptState>>,
    mut free: ResMut<FreeCam>,
    mut cams: Query<&mut Transform, With<PlayerCamera>>,
) {
    if !free.on {
        return;
    }
    let state_free = match *state.get() {
        PlayState::Playing => true,
        PlayState::Menu => free.holds_world(),
        _ => false,
    };
    free.active = state_free && !console.open && !paused.0 && menu.0.is_none() && !loading.active();
    if free.editor {
        return;
    }
    let Ok(mut t) = cams.single_mut() else { return };
    if free.active {
        let f = &mut *free;
        f.yaw -= motion.delta.x * LOOK_PER_PIXEL;
        if motion.delta.y != 0.0 {
            f.pitch = (f.pitch - motion.delta.y * LOOK_PER_PIXEL).clamp(-1.55, 1.55);
        }
        if scroll.delta.y != 0.0 {
            f.wheel = (f.wheel * 1.2f32.powf(scroll.delta.y)).clamp(50.0 / FLY_SPEED, 50_000.0 / FLY_SPEED);
        }
        let rot = Quat::from_euler(EulerRot::YXZ, f.yaw, f.pitch, 0.0);
        let (fwd, right) = (rot * Vec3::NEG_Z, rot * Vec3::X);
        let mut dir = Vec3::ZERO;
        for (k, d) in [(KeyCode::KeyW, fwd), (KeyCode::KeyS, -fwd), (KeyCode::KeyD, right), (KeyCode::KeyA, -right), (KeyCode::KeyE, Vec3::Y), (KeyCode::KeyQ, -Vec3::Y)] {
            if keys.pressed(k) {
                dir += d;
            }
        }
        let boost = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) { 5.0 } else { 1.0 };
        let mult = script.as_ref().and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.ui.camera.fly_speed_mult).unwrap_or(1.0);
        let dt = real.delta_secs().min(0.25);
        f.pos += dir.normalize_or_zero() * FLY_SPEED * f.wheel * mult * boost * dt;
        if f.aid_speed != 0.0 {
            f.pos += Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero() * f.aid_speed * dt;
        }
    }
    t.translation = free.pos;
    t.rotation = Quat::from_euler(EulerRot::YXZ, free.yaw, free.pitch, 0.0);
}

/// Mode 1 holds the world as a menu does: the console's closing would let it go on, so
/// the state stays `Menu` while no other menu is open.
fn hold_world(free: Res<FreeCam>, console: Res<Console>, paused: Res<crate::pause::Paused>, menu: Res<crate::menu::ActiveMenu>, mut next: ResMut<NextState<PlayState>>, state: Res<State<PlayState>>) {
    if free.holds_world() && !console.open && !paused.0 && menu.0.is_none() && matches!(*state.get(), PlayState::Playing | PlayState::Menu) {
        NextState::set_if_different(&mut *next, PlayState::Menu);
    }
}

/// `tm`: hide the HUD (with the compass), the crosshair and the corner and quest
/// messages; their own systems show them again when it is undone.
#[allow(clippy::type_complexity)]
fn hide_menus(
    hidden: Res<MenusHidden>,
    free: Res<FreeCam>,
    mut roots: Query<
        &mut Visibility,
        Or<(With<crate::hud::HudRoot>, With<crate::reticle::Root>, With<crate::corner_messages::AreaRoot>, With<crate::quest_messages::AreaRoot>)>,
    >,
) {
    if hidden.0 || free.editor_holds() {
        for mut v in &mut roots {
            v.set_if_neq(Visibility::Hidden);
        }
    }
}
