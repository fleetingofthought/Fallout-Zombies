//! The particle shaders of effect shaders (EFSH, docs/spec-notes/effect-shaders.md §6):
//! the flames of a burning actor (`Flames01`, the flamer's "Burning"), the smoke over it
//! (`Smoke01`), played on a reference by a magic effect or a script (`effect_shaders`).
//!
//! - **Births**: while the shader plays, particles are born at its birth ratio for the
//!   time since it started (ramp up, full, ramp down, then the persistent ratio while the
//!   persistent part shows) × [`BIRTHS_PER_RATIO`] a second [design: the GECK wiki calls
//!   the ratio "the amount of particles that spawn" without a unit].
//! - **Where**: on the reference's drawn meshes: at one of the bones a skinned mesh is
//!   bound to (so on the body as it moves), [`SURFACE_OFFSET`] out along the particle's
//!   normal, or for an unskinned mesh somewhere in its bounds [design: the game's
//!   "emit from the target object" surface points are not documented]; none from a
//!   hidden mesh (the player's body in first person). The normal is a random direction
//!   ("mostly random", GECK wiki).
//! - **Motion and look** [documented: the GECK wiki EffectShader page; hypothesis where
//!   `fnv_world::effect_shader::ParticleShader` says]: speed along the normal ± its range
//!   with its acceleration, plus the world velocity and acceleration (game axes); life ±
//!   its range; scale from 0 through the two keys (the scale as the particle's width in
//!   game units [hypothesis]); colour keys over the life from and back to the texture's
//!   own colour, the alpha held at the nearest key's (`ParticleShader::tint`); rotation ± range and spin ± range; one frame of the texture's U × V
//!   grid each, at random [hypothesis]. Drawn as squares facing the camera, unlit, the
//!   tint taken as display values as `particles.rs` does, blended by the destination
//!   blend (One: added).
//! - **End**: no births once the play ends; the particles alive live out their time.
//!
//! Log: `effect particles: <EDID> on <who> (<texture>, …)` when a play starts drawing,
//! and at exit `effect particles summary: …`.

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::Indices;
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use fnv_sim::FormId;
use fnv_world::effect_shader::{blend, ParticleShader};
use nvf_assets::model::ModelCtx;
use nvf_assets::nif_material::nif_material;
use nvf_assets::particles::particle_mesh;
use nvf_play::script::ScriptState;
use std::collections::HashMap;

use crate::effect_shaders::{Shaders, Source};
use crate::world::WorldRoot;

/// Particles a second per unit of birth ratio [design: fitted by eye between
/// `Flames01`'s flames (persistent 15, living 0.35 s) covering a burning human and
/// `Smoke01`'s smoke (15, living 2.3 s and growing to 100 units) not hiding it].
pub const BIRTHS_PER_RATIO: f32 = 3.0;

/// Game units a particle starts out from the bone it is born at, along its normal: a
/// bone lies inside the body [design].
pub const SURFACE_OFFSET: f32 = 10.0;

/// Particles alive at once for one play, at most [design].
pub const MAX_ALIVE: usize = 600;

struct Particle {
    pos: Vec3,
    vel: Vec3,
    normal: Vec3,
    age: f32,
    life: f32,
    angle: f32,
    spin: f32,
    frame: u32,
}

/// The particles of one play.
struct Burst {
    def: ParticleShader,
    edid: String,
    particles: Vec<Particle>,
    mesh: Handle<Mesh>,
    entity: Entity,
    owed: f32,
    /// The play still goes on (births); once it ends the burst goes when empty.
    playing: bool,
}

#[derive(Resource)]
pub struct EffectParticles {
    bursts: HashMap<Source, Burst>,
    /// Shaders whose particle texture is missing, reported once.
    missing: Vec<u32>,
    rng: SplitMix,
    born: u64,
    plays: u32,
    most_alive: usize,
}

impl Default for EffectParticles {
    fn default() -> Self {
        EffectParticles { bursts: HashMap::new(), missing: Vec::new(), rng: SplitMix(0x5eed_f1a3), born: 0, plays: 0, most_alive: 0 }
    }
}

/// A number in −1..1.
fn spread(rng: &mut SplitMix) -> f32 {
    rng.next_f32() * 2.0 - 1.0
}

/// A random unit vector.
fn random_dir(rng: &mut SplitMix) -> Vec3 {
    let z = spread(rng);
    let a = rng.next_f32() * std::f32::consts::TAU;
    let r = (1.0 - z * z).max(0.0).sqrt();
    Vec3::new(r * a.cos(), r * a.sin(), z)
}

fn alpha_mode(dest_blend: u32) -> AlphaMode {
    match dest_blend {
        blend::SOURCE_INVERTED_ALPHA => AlphaMode::Blend,
        blend::DEST_COLOR => AlphaMode::Multiply,
        _ => AlphaMode::Add,
    }
}

/// A particle's tint as display values: converted to linear, the alpha kept
/// (`particles.rs`, `display_colour`).
fn display_colour(c: [f32; 4]) -> [f32; 4] {
    let l = Color::srgb(c[0].max(0.0), c[1].max(0.0), c[2].max(0.0)).to_linear();
    [l.red, l.green, l.blue, c[3]]
}

/// Births, motion and drawing of every play's particles.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn draw(
    mut commands: Commands,
    time: Res<Time>,
    shaders: Res<Shaders>,
    script: Option<Res<ScriptState>>,
    mut st: ResMut<EffectParticles>,
    mut models: ModelCtx,
    root: Query<(Entity, &GlobalTransform), With<WorldRoot>>,
    skins: Query<&SkinnedMesh>,
    globals: Query<&GlobalTransform>,
    bounds: Query<&Aabb>,
    shown: Query<&InheritedVisibility>,
    layers: Query<&bevy::camera::visibility::RenderLayers>,
    camera: Query<&Transform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
) {
    let Ok((root, root_at)) = root.single() else { return };
    let st = &mut *st;
    let (now, dt) = (time.elapsed_secs(), time.delta_secs());
    let to_game = root_at.affine().inverse();

    // New plays with a particle shader get a burst; ended ones stop being born into.
    for b in st.bursts.values_mut() {
        b.playing = false;
    }
    for (key, a) in &shaders.active {
        let Some(def) = a.shader.particles.as_ref() else { continue };
        if let Some(b) = st.bursts.get_mut(key) {
            b.playing = true;
            continue;
        }
        let path = a.shader.particle_texture.as_deref().map(|t| {
            let p = t.replace('/', "\\").to_ascii_lowercase();
            if p.starts_with("textures\\") {
                p
            } else {
                format!("textures\\{p}")
            }
        });
        let m = &mut models;
        let texture = path.as_deref().and_then(|p| m.textures.get(p, true, &m.data, &mut m.images, m.formats.as_deref()));
        if texture.is_none() {
            if !st.missing.contains(&a.shader.form_id) {
                st.missing.push(a.shader.form_id);
                warn!("effect particles: {}: particle texture {} not found", a.shader.edid, path.as_deref().unwrap_or("?"));
            }
            continue;
        }
        let material = models.materials.add(nif_material(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: texture,
            unlit: true,
            alpha_mode: alpha_mode(def.dest_blend),
            double_sided: true,
            cull_mode: None,
            ..default()
        }));
        let mesh = models.meshes.add(particle_mesh());
        let entity = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), Transform::IDENTITY, Visibility::Hidden, NotShadowCaster, NoFrustumCulling, ChildOf(root))).id();
        st.plays += 1;
        let who = if a.on == FormId::PLAYER_REF.0 { "the player".to_string() } else { script.as_deref().and_then(|s| s.rt.as_ref()).map_or_else(|| format!("{:08X}", a.on), |rt| rt.game.data.name(FormId(a.on)).to_string()) };
        info!(
            "effect particles: {} on {who} ({}, {:.0}/s at full, {:.0}/s persistent, life {:.2} s, frames {}x{})",
            a.shader.edid,
            path.as_deref().unwrap_or("?"),
            def.full_ratio * BIRTHS_PER_RATIO,
            def.persistent_ratio * BIRTHS_PER_RATIO,
            def.lifetime,
            def.frames[0],
            def.frames[1]
        );
        st.bursts.insert(*key, Burst { def: def.clone(), edid: a.shader.edid.clone(), particles: Vec::new(), mesh, entity, owed: 0.0, playing: true });
    }

    // Births on the covered meshes.
    let rng = &mut st.rng;
    let mut born = 0;
    for (key, b) in st.bursts.iter_mut().filter(|(_, b)| b.playing) {
        let Some(a) = shaders.active.get(key) else { continue };
        let d = &b.def;
        b.owed += d.birth_ratio(now - a.since, a.persist) * BIRTHS_PER_RATIO * dt;
        // Births past the free slots are dropped (as the loop below would skip them);
        // capping here keeps a broken shader's huge or infinite ratio from looping
        // without end.
        let free = MAX_ALIVE.saturating_sub(b.particles.len()) as f32;
        b.owed = if b.owed.is_finite() { b.owed.min(free + b.owed.fract()) } else { 0.0 };
        if b.owed < 1.0 {
            continue;
        }
        // Where particles can be born: the bones of the skinned meshes, the bounds of the others.
        let mut bones: Vec<Entity> = Vec::new();
        let mut boxes: Vec<(Entity, Aabb)> = Vec::new();
        for &e in a.covered.keys() {
            // [design: player_body::show_rigs] Hidden meshes still give off none;
            // the player's shadow-only meshes also have no births in the camera.
            if shown.get(e).is_ok_and(|v| !v.get()) || (a.on == FormId::PLAYER_REF.0 && !crate::effect_shaders::player_mesh_seen(layers.get(e).ok())) {
                continue;
            }
            match skins.get(e) {
                Ok(s) => bones.extend(s.joints.iter().copied()),
                Err(_) => {
                    if let Ok(bx) = bounds.get(e) {
                        boxes.push((e, *bx));
                    }
                }
            }
        }
        bones.sort();
        bones.dedup();
        let places = bones.len() + boxes.len();
        if places == 0 {
            b.owed = 0.0;
            continue;
        }
        while b.owed >= 1.0 {
            b.owed -= 1.0;
            if b.particles.len() >= MAX_ALIVE {
                continue;
            }
            let pick = ((rng.next_f32() * places as f32) as usize).min(places - 1);
            let normal = random_dir(rng);
            let at = if pick < bones.len() {
                let Ok(g) = globals.get(bones[pick]) else { continue };
                to_game.transform_point3(g.translation()) + normal * SURFACE_OFFSET
            } else {
                let (e, bx) = &boxes[pick - bones.len()];
                let Ok(g) = globals.get(*e) else { continue };
                let local = Vec3::from(bx.center) + Vec3::from(bx.half_extents) * Vec3::new(spread(rng), spread(rng), spread(rng));
                to_game.transform_point3(g.transform_point(local))
            };
            let speed = d.speed_normal + d.speed_normal_range * spread(rng);
            b.particles.push(Particle {
                pos: at,
                vel: normal * speed + Vec3::from(d.velocity),
                normal,
                age: 0.0,
                life: (d.lifetime + d.lifetime_range * spread(rng)).max(0.02),
                angle: (d.rotation + d.rotation_range * spread(rng)).to_radians(),
                spin: (d.spin + d.spin_range * spread(rng)).to_radians(),
                frame: (rng.next_f32() * (d.frames[0] * d.frames[1]) as f32) as u32,
            });
            born += 1;
        }
    }
    st.born += born;

    // Motion; bursts whose play ended and whose particles are gone go.
    let mut alive = 0;
    for b in st.bursts.values_mut() {
        let d = &b.def;
        let accel = Vec3::from(d.accel);
        for q in &mut b.particles {
            q.age += dt;
            q.vel += (accel + q.normal * d.accel_normal) * dt;
            q.pos += q.vel * dt;
            q.angle += q.spin * dt;
        }
        b.particles.retain(|q| q.age < q.life);
        alive += b.particles.len();
    }
    st.most_alive = st.most_alive.max(alive);
    let done: Vec<Source> = st.bursts.iter().filter(|(_, b)| !b.playing && b.particles.is_empty()).map(|(k, _)| *k).collect();
    for k in done {
        if let Some(b) = st.bursts.remove(&k) {
            commands.entity(b.entity).try_despawn();
            models.meshes.remove(&b.mesh);
            debug!("effect particles: {} done", b.edid);
        }
    }

    // Drawing: a square facing the camera per particle.
    let Ok(cam) = camera.single() else { return };
    let right = to_game.transform_vector3(cam.rotation * Vec3::X).normalize_or(Vec3::X);
    let up = to_game.transform_vector3(cam.rotation * Vec3::Y).normalize_or(Vec3::Z);
    let back = to_game.transform_vector3(cam.rotation * Vec3::Z).normalize_or(Vec3::Y);
    for b in st.bursts.values() {
        let shown = !b.particles.is_empty();
        // The burst's entity may have gone with the world root (a load); nothing to show then.
        commands.entity(b.entity).try_insert(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if !shown {
            continue;
        }
        let Some(mut mesh) = models.meshes.get_mut(&b.mesh) else { continue };
        let d = &b.def;
        let n = b.particles.len();
        let (mut positions, mut uvs, mut colors, mut indices) = (Vec::with_capacity(n * 4), Vec::with_capacity(n * 4), Vec::with_capacity(n * 4), Vec::with_capacity(n * 6));
        let [fu, fv] = d.frames;
        let (w, h) = (1.0 / fu as f32, 1.0 / fv as f32);
        for (k, q) in b.particles.iter().enumerate() {
            let f = (q.age / q.life).clamp(0.0, 1.0);
            let half = 0.5 * d.scale(f).max(0.0);
            let (s, c) = q.angle.sin_cos();
            let (x, y) = ((right * c + up * s) * half, (up * c - right * s) * half);
            for corner in [-x - y, x - y, x + y, -x + y] {
                positions.push((q.pos + corner).to_array());
            }
            let (u, v) = ((q.frame % fu) as f32 * w, (q.frame / fu % fv) as f32 * h);
            uvs.extend([[u, v + h], [u + w, v + h], [u + w, v], [u, v]]);
            colors.extend([display_colour(d.tint(f)); 4]);
            let i = (k * 4) as u32;
            indices.extend([i, i + 1, i + 2, i, i + 2, i + 3]);
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![back.to_array(); n * 4]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(indices));
    }
}

pub fn report(mut exit: MessageReader<AppExit>, st: Res<EffectParticles>) {
    if exit.read().last().is_some() && st.plays > 0 {
        info!("effect particles summary: {} plays drew particles, {} born, at most {} alive at once", st.plays, st.born, st.most_alive);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_directions_are_unit_and_spread() {
        let mut rng = SplitMix(1);
        let mut sum = Vec3::ZERO;
        for _ in 0..2000 {
            let d = random_dir(&mut rng);
            assert!((d.length() - 1.0).abs() < 1e-4);
            sum += d;
        }
        assert!(sum.length() / 2000.0 < 0.1, "{sum:?}");
    }
}
