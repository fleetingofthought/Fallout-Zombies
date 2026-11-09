//! The character creator's picture (docs/spec-notes/character-creator.md §5): while its
//! camera is up (`nvf_play::control::PlayerControl::portrait`) the player is lit by two
//! lights of the engine's own that go with the camera, and actors standing between the
//! camera and the player are hidden. Both end when the creator closes: the lights are
//! removed and the actors shown again, so the scene's lighting and cast are as before.
//!
//! - **Light** [owner 2026-10-17: "we need some kind of light its hard to see details you
//!   are changing in the shadow"; design: the rig]: a warm key light above and to the
//!   camera's left, a dimmer cool fill to its right and lower, both point lights without
//!   shadows, their brightness in the units of the game's own lights (`lights::lumens`,
//!   scaled by `--light-scale` like them).
//! - **Nothing in the way** [owner 2026-10-17: Doc Mitchell in his chair stood between the
//!   camera and the player; design: hide, not move]: an actor whose root lies within
//!   [`CLEAR_RADIUS`] of the line from the camera to the point looked at, nearer the
//!   camera than the player, is hidden while it is in the way. The player is not moved:
//!   the opening's scene goes on round them (§6 of the note).
//!
//! - **The studio** [owner 2026-11-02: the birth room is an awkward place for the
//!   creator; design: a picture, not a place]: when the gene projector opened it
//!   (`nvf_play::creator::CreatorMenu::in_studio`), the camera sees only the player's body
//!   ([`crate::sky::BODY_LAYER`]) and [`STUDIO_LAYER`]: a closed backdrop round the
//!   player, a floor and a paler pool under the feet, plain unlit colours of the
//!   interface's dark tones, fog left out. The player is not moved; the camera's layers are
//!   put back when the creator closes.
//!
//! Log lines start `creator view:`.

use crate::limbs::ActorRig;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use nvf_play::control::{BodyView, PlayerControl};
use nvf_play::creator::CreatorMenu;
use nvf_play::player::Player;
use nvf_play::space;

/// Render layer of the creator's studio (backdrop and floor).
pub const STUDIO_LAYER: usize = 14;
/// The backdrop's radius about the player's chest and the floor's, game units [design:
/// beyond the creator's farthest camera, 260 units and its pan].
const BACKDROP_RADIUS: f32 = 480.0;
const FLOOR_RADIUS: f32 = 460.0;
// The backdrop holds the creator's farthest camera: 260 units from the eye (about 40
// over the chest), panned up to 80.
const _: () = assert!(BACKDROP_RADIUS > 260.0 + 80.0 + 40.0);
/// The pool of floor under the player, game units [design].
const POOL_RADIUS: f32 = 70.0;
/// The chest's height over the feet, where the backdrop is centred [design].
const CHEST: f32 = 90.0;
/// The studio's colours, display sRGB [design: the interface's dark slate, a floor a
/// little lighter, the pool lighter still].
const BACKDROP: [f32; 3] = [0.10, 0.12, 0.13];
const FLOOR: [f32; 3] = [0.15, 0.17, 0.18];
const POOL: [f32; 3] = [0.24, 0.26, 0.27];

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum StudioPart {
    Backdrop,
    Floor,
    Pool,
}

/// While the studio stands: the camera's layers from before it, put back after.
#[derive(Resource, Default)]
struct Studio(Option<RenderLayers>);

/// How far from the line of sight an actor's root may stand before it is hidden, game
/// units [design: a seated or standing body's half width and a margin].
pub const CLEAR_RADIUS: f32 = 40.0;
/// The lights' brightness (a LIGH record's fade) and reach [design].
const KEY: f32 = 1.6;
const FILL: f32 = 0.55;
const RANGE: f32 = 320.0;

#[derive(Component)]
struct CreatorLight;

/// The actors hidden because they stood in the way, with their visibility before.
#[derive(Resource, Default)]
struct Hidden(Vec<(Entity, Visibility)>);

pub struct CreatorViewPlugin {
    /// `--light-scale`.
    pub light_scale: f32,
}

#[derive(Resource)]
struct LightScale(f32);

impl Plugin for CreatorViewPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LightScale(self.light_scale)).init_resource::<Hidden>().init_resource::<Studio>().add_systems(PostUpdate, (lights, clear_view, studio).before(TransformSystems::Propagate));
    }
}

/// Where a studio part stands, Bevy space: the backdrop about the chest, the floor just
/// under the feet (so it does not fight the body's soles), the pool a hair above the floor.
fn studio_place(part: StudioPart, feet: [f32; 3]) -> Transform {
    let f = space::to_bevy(feet);
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    match part {
        StudioPart::Backdrop => Transform::from_translation(f + Vec3::Y * CHEST),
        StudioPart::Floor => Transform::from_translation(f - Vec3::Y * 0.5).with_rotation(flat),
        StudioPart::Pool => Transform::from_translation(f - Vec3::Y * 0.3).with_rotation(flat),
    }
}

/// Put the studio up while the creator is open in it, take it down after
/// (module notes).
#[allow(clippy::too_many_arguments)]
fn studio(
    mut commands: Commands,
    menu: Option<Res<CreatorMenu>>,
    player: Option<Res<Player>>,
    mut state: ResMut<Studio>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cameras: Query<&mut RenderLayers, With<nvf_play::PlayerCamera>>,
    mut parts: Query<(Entity, &mut Transform, &StudioPart), Without<nvf_play::PlayerCamera>>,
) {
    let want = menu.as_deref().is_some_and(CreatorMenu::in_studio).then_some(()).and(player.as_deref().map(|p| p.feet));
    match (want, state.0.is_some()) {
        (Some(feet), false) => {
            let Ok(mut layers) = cameras.single_mut() else { return };
            state.0 = Some(layers.clone());
            *layers = RenderLayers::from_layers(&[crate::sky::BODY_LAYER, STUDIO_LAYER]);
            let plain = |c: [f32; 3]| StandardMaterial { base_color: Color::srgb(c[0], c[1], c[2]), unlit: true, fog_enabled: false, cull_mode: None, ..default() };
            let set = [
                ("Creator studio backdrop", StudioPart::Backdrop, meshes.add(Sphere::new(BACKDROP_RADIUS).mesh().uv(48, 24)), BACKDROP),
                ("Creator studio floor", StudioPart::Floor, meshes.add(Circle::new(FLOOR_RADIUS).mesh().resolution(96)), FLOOR),
                ("Creator studio pool", StudioPart::Pool, meshes.add(Circle::new(POOL_RADIUS).mesh().resolution(64)), POOL),
            ];
            for (name, part, mesh, colour) in set {
                commands.spawn((Name::new(name), part, Mesh3d(mesh), MeshMaterial3d(materials.add(plain(colour))), studio_place(part, feet), RenderLayers::layer(STUDIO_LAYER)));
            }
            info!("creator view: studio up round the player at ({:.0}, {:.0}, {:.0}); the room is not drawn", feet[0], feet[1], feet[2]);
        }
        (Some(feet), true) => {
            // The player does not move, but the set follows them if a script does.
            for (_, mut t, part) in &mut parts {
                *t = studio_place(*part, feet);
            }
        }
        (None, true) => {
            for (e, ..) in &parts {
                commands.entity(e).despawn();
            }
            if let (Ok(mut layers), Some(before)) = (cameras.single_mut(), state.0.take()) {
                *layers = before;
            }
            info!("creator view: studio down; the room is drawn again");
        }
        (None, false) => {}
    }
}

/// The eye the creator's camera aims from: the drawn body's, as the camera's
/// (`nvf_play::control::place_camera`).
fn eye_of(p: &Player, body: Option<&BodyView>) -> [f32; 3] {
    body.and_then(BodyView::portrait_eye).unwrap_or_else(|| p.eye())
}

/// The camera's place and the point it looks at, Bevy space, while the creator is open.
fn view(control: &PlayerControl, eye: [f32; 3], facing: f32) -> Option<(Vec3, Vec3, f32)> {
    let p = control.portrait?;
    let (at, heading, _) = p.spot(eye, facing);
    let target = [eye[0], eye[1], eye[2] - p.drop];
    // The target as the camera sees it: shifted with the camera, so the lights stay on
    // the player and the line of sight runs to the picture's middle.
    let right = [(heading).cos(), -(heading).sin()];
    let seen = [target[0] - right[0] * p.shift, target[1] - right[1] * p.shift, target[2]];
    Some((space::to_bevy(at), space::to_bevy(seen), heading))
}

fn lights(
    mut commands: Commands,
    control: Option<Res<PlayerControl>>,
    player: Option<Res<Player>>,
    body: Option<Res<BodyView>>,
    scale: Res<LightScale>,
    mut rig: Query<(Entity, &mut Transform), With<CreatorLight>>,
) {
    let want = control.as_deref().zip(player.as_deref()).and_then(|(c, p)| view(c, eye_of(p, body.as_deref()), p.heading).map(|v| (v, p)));
    let Some(((camera, _, heading), p)) = want else {
        if !rig.is_empty() {
            for (e, _) in &rig {
                commands.entity(e).despawn();
            }
            info!("creator view: lights off");
        }
        return;
    };
    // Around the player's face: the key up and to the camera's left, the fill to its
    // right and lower, both on the camera's side of the player.
    let eye = space::to_bevy(eye_of(p, body.as_deref()));
    let toward = (camera - eye).with_y(0.0).normalize_or(Vec3::Z);
    let right = Vec3::new(heading.cos(), 0.0, heading.sin());
    // `right` in Bevy space: game x is Bevy x, game y is Bevy −z.
    let places = [eye + toward * 55.0 - right * 45.0 + Vec3::Y * 40.0, eye + toward * 60.0 + right * 55.0 - Vec3::Y * 10.0];
    if rig.is_empty() {
        let unit = crate::sky::light_unit();
        for (i, (f, c)) in [(KEY, crate::lights::display_color([1.0, 0.94, 0.85])), (FILL, crate::lights::display_color([0.82, 0.88, 1.0]))].into_iter().enumerate() {
            let intensity = crate::lights::lumens(f * scale.0, unit);
            // On the world's layer and the body's: a light lights only the views whose layers
            // meet its own, and the studio's camera sees the body's and the studio's only
            // (before, the studio had none of the creator's light).
            commands.spawn((
                Name::new("Creator light"),
                CreatorLight,
                PointLight { color: c, intensity, range: RANGE, shadow_maps_enabled: false, ..default() },
                Transform::from_translation(places[i]),
                RenderLayers::from_layers(&[0, crate::sky::BODY_LAYER]),
            ));
        }
        info!("creator view: lights on (key {KEY}, fill {FILL}, reach {RANGE})");
        return;
    }
    for (i, (_, mut t)) in rig.iter_mut().enumerate() {
        t.translation = places[i.min(1)];
    }
}

fn clear_view(control: Option<Res<PlayerControl>>, player: Option<Res<Player>>, body: Option<Res<BodyView>>, mut hidden: ResMut<Hidden>, mut actors: Query<(Entity, &ActorRig, &GlobalTransform, &mut Visibility)>) {
    let line = control.as_deref().zip(player.as_deref()).and_then(|(c, p)| view(c, eye_of(p, body.as_deref()), p.heading));
    let in_way = |pos: Vec3| -> bool {
        let Some((camera, target, _)) = line else { return false };
        let d = target - camera;
        let len = d.length();
        if len < 1.0 {
            return false;
        }
        let k = (pos - camera).dot(d) / len;
        // Between the camera and the player (with a little room before the player).
        if k < 0.0 || k > len - 15.0 {
            return false;
        }
        let closest = camera + d / len * k;
        // Its root is at its feet: a standing or sitting body reaches 120 units up.
        let up = (closest.y - pos.y).clamp(0.0, 120.0);
        (pos + Vec3::Y * up - closest).length() < CLEAR_RADIUS
    };
    for (e, rig, t, mut v) in &mut actors {
        if rig.reference == fnv_sim::FormId::PLAYER_REF.0 {
            continue;
        }
        let was = hidden.0.iter().position(|(h, _)| *h == e);
        match (in_way(t.translation()), was) {
            (true, None) => {
                hidden.0.push((e, *v));
                *v = Visibility::Hidden;
                info!("creator view: {:08X} hidden (in the way)", rig.reference);
            }
            (false, Some(i)) => {
                let (_, before) = hidden.0.remove(i);
                *v = before;
                info!("creator view: {:08X} shown again", rig.reference);
            }
            _ => {}
        }
    }
    // Actors gone meanwhile.
    hidden.0.retain(|(e, _)| actors.contains(*e));
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvf_play::control::Portrait;

    #[test]
    fn the_view_runs_from_the_camera_to_the_player() {
        let mut c = PlayerControl::default();
        let eye = [0.0, 0.0, 120.0];
        assert!(view(&c, eye, 0.0).is_none());
        c.portrait = Some(Portrait { yaw: 0.0, distance: 50.0, drop: 0.0, pitch: 0.0, shift: 0.0 });
        let (camera, target, _) = view(&c, eye, 0.0).unwrap();
        // Facing +y: the camera is 50 in front (Bevy −z), level with the eye.
        assert!((camera - target).length() > 49.0 && (camera.z - target.z + 50.0).abs() < 1e-3, "{camera} {target}");
    }

    #[test]
    fn the_studio_stands_round_the_player() {
        let feet = [100.0, -200.0, 50.0];
        let f = space::to_bevy(feet);
        // The backdrop about the chest (it holds the farthest camera, checked where its radius is set).
        let back = studio_place(StudioPart::Backdrop, feet);
        assert!((back.translation - (f + Vec3::Y * CHEST)).length() < 1e-3);
        // The floor flat under the feet, the pool just above it and still under the soles.
        let floor = studio_place(StudioPart::Floor, feet);
        let pool = studio_place(StudioPart::Pool, feet);
        assert!(floor.translation.y < pool.translation.y && pool.translation.y < f.y);
        assert!((floor.rotation * Vec3::Z).y.abs() > 0.99, "the disc faces up");
    }
}
