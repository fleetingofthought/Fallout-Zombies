//! Particle effects of add-on nodes (docs/spec-notes/particles.md): the sparks and smoke
//! of a muzzle flash, a missile's trail, the flamer's flame, the dust of an impact.
//!
//! - **Add-on nodes.** A model's `BSValueNode` named `AddOnNode<n>` stands for the ADDN
//!   record with that node index, whose model (`meshes\mps\…`) holds one or more
//!   `NiParticleSystem`s under a `BSMasterParticleSystem` [documented: xEdit ADDN;
//!   nif.xml]. Each add-on node in the world is an **emitter** of those systems for as
//!   long as it exists ([`AddonEmitter`]); the particles belong to the system, live in
//!   the world and outlast the node [hypothesis: the meaning of "master particle
//!   system", from the systems' `world_space` flag and from their particle counts,
//!   which equal birth rate × longest life × the emitter controller's `max_emitters`].
//! - **A system** is read by `nvf_assets::particles`; its particles are simulated here:
//!   born at the emitter's rate in its volume, thrown within its cone about the emitter
//!   object's +z, then aged, pulled by its gravity modifiers (in the world's axes),
//!   slowed by its drag modifiers, turned, sized by grow and fade, coloured over their
//!   life. A system holds at most its data block's vertex count of particles, and
//!   serves at most `max_emitters` emitters a frame.
//! - **Drawn** as one mesh per system, a square facing the camera for each particle,
//!   with the system's own texture, blending and vertex colours.
//! - **Model emitters** ([`ModelEmitter`]): a model's own particle systems, emitted from
//!   where its emitter objects are in the file (explosions, `shot_fx`).
//! - **Modifiers switched on and off** (`NiPSysModifierActiveCtlr`): a gravity or drag
//!   modifier acts on a particle while the emitter it came from is at a time its keys
//!   say on (`nvf_assets::particles::ModifierActive`).
//! - **Mesh emitters** start particles on their meshes (vertices, face or edge centres,
//!   anywhere on the faces or edges) thrown along the mesh's normal there; **strip
//!   systems** trail a strip of recorded points behind each particle, drawn as a ribbon;
//!   the **bomb** modifier pushes particles away from its object, **planar colliders**
//!   bounce or kill them; float controllers key emitter fields, gravity strength and
//!   birth rates over the emitter's time (particles.md §6). Bombs, colliders and
//!   spherical gravity sit where the emitter's node placed them when the particle was
//!   born (`Particle::frame`).
//! - **Placed models** emit their own systems and their add-on nodes' from a holder at
//!   the reference (`world::spawn_emitters`: Gomorrah's roof fires), not while hidden;
//!   a placed model's own live particles go at once when it is hidden or gone (a taken
//!   snow globe, #1596; log `particles: <ref> is not drawn any more: its <n> live
//!   particles go`);
//!   a placed model's own systems are that reference's own, and a master system serves
//!   the add-on nodes nearest the camera first (particles.md §7). References a script
//!   enables (`nvf_play::script_draw`) emit too.
//! - **Damage stages.** An add-on node below a `BSDamageStage` node emits only while its
//!   reference is drawn at one of that node's model damage stages ([`StagedAddon`],
//!   docs/spec-notes/destruction-stages.md): a motorcycle's `DamageStage1-3` smoke starts
//!   when a hit takes it to stage 1, never on an intact or indestructible (STAT) bike.
//!   Log: `damage stage: <ref> add-on node <n> waits for model stages <lo>-<hi> (at <s>)`
//!   when one is placed, then `… on at model stage <s>` / `… off at model stage <s>`.
//! - **Sequences of drawn models** (particles.md §6.7): a placed model drawn with its own
//!   nodes (a [`crate::object_anims::GroupRig`]) whose sequences drive a system's emitter
//!   emits by the sequence playing on it, on that sequence's time, and not at all while it
//!   plays none of them; and from its emitter object's node as the sequence moves it (the
//!   REPCONN rockets' exhaust rides with the rockets). Modifiers switched on and off by
//!   the sequences follow the one a particle was born under.
//! - Not built: turbulence, spawn-on-death; a mesh emitter's meshes stay where the file
//!   puts them; gravity strength keys from the first sequence only.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::math::Affine3A;
use bevy::mesh::{Indices, MeshVertexAttribute, VertexAttributeValues};
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use nvf_assets::anim_play::Playing;
use nvf_assets::model::ModelCtx;
use nvf_assets::particles::{particle_mesh, InitialVelocity, ParticleSystemDef, PreparedParticles};
use nvf_play::script::ScriptState;
use nvf_play::script_draw::{ScriptDrawnModel, ScriptDrawnRig, ScriptDrawnRoot};
use nvf_play::sfx::Sfx;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::world::WorldRoot;

/// The steps a second a drag modifier's percentage is taken per [hypothesis: fitted so
/// the flamer's flame, thrown at 2,700 units a second, ends near its projectile's range
/// of 640 units; docs/spec-notes/particles.md].
pub const DRAG_STEPS_PER_SECOND: f32 = 30.0;

/// An add-on node in the world: it emits the particles of ADDN `index` while it exists.
#[derive(Component)]
pub struct AddonEmitter {
    pub index: i32,
    age: f32,
    /// The placed reference it belongs to (the exit count of placed emitters).
    placed: Option<u32>,
}

impl AddonEmitter {
    pub fn new(index: i32) -> AddonEmitter {
        AddonEmitter { index, age: 0.0, placed: None }
    }

    /// An add-on node of placed reference `r` (a barrel's fire, a vehicle's smoke).
    pub fn placed(index: i32, r: u32) -> AddonEmitter {
        AddonEmitter { index, age: 0.0, placed: Some(r) }
    }
}

/// An add-on node below a model's `BSDamageStage` node, on placed reference `r`: an
/// [`AddonEmitter`] of ADDN `index` while the reference's model damage stage
/// (`Game::model_damage_stage`) is within `stages` [hypothesis: the node's children are
/// the model's look at those stages only, docs/spec-notes/destruction-stages.md].
#[derive(Component)]
pub struct StagedAddon {
    pub r: u32,
    pub index: i32,
    pub stages: (i32, i32),
    /// Last stage seen; `None` before the first look.
    at: Option<i32>,
}

/// Make node `i` of `nodes` (entity `e`) the emitter of ADDN `index` for placed
/// reference `r`: at once, or as a [`StagedAddon`] when it is below a `BSDamageStage`.
pub fn place_addon(commands: &mut Commands, e: Entity, r: u32, nodes: &[nvf_assets::model::ModelNode], i: usize, index: i32) {
    match nvf_assets::model::damage_stages_of(nodes, i) {
        Some(stages) => commands.entity(e).insert(StagedAddon { r, index, stages, at: None }),
        None => commands.entity(e).insert(AddonEmitter::placed(index, r)),
    };
}

/// Switch staged add-on nodes on and off as their references' damage stages change.
fn staged_addons(mut commands: Commands, script: Option<Res<ScriptState>>, mut staged: Query<(Entity, &mut StagedAddon)>) {
    let game = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    for (e, mut s) in &mut staged {
        let stage = game.map_or(0, |g| g.model_damage_stage(fnv_sim::FormId(s.r)) as i32);
        if s.at == Some(stage) {
            continue;
        }
        let (lo, hi) = s.stages;
        let shown = (lo..=hi).contains(&stage);
        let was = s.at.is_some_and(|a| (lo..=hi).contains(&a));
        if s.at.is_none() && !shown {
            info!("damage stage: {:08X} add-on node {} waits for model stages {lo}-{hi} (at {stage})", s.r, s.index);
        }
        if shown && !was {
            commands.entity(e).insert(AddonEmitter::placed(s.index, s.r));
            info!("damage stage: {:08X} add-on node {} on at model stage {stage}", s.r, s.index);
        } else if !shown && was {
            commands.entity(e).remove::<AddonEmitter>();
            info!("damage stage: {:08X} add-on node {} off at model stage {stage}", s.r, s.index);
        }
        s.at = Some(stage);
    }
}

/// A model in the world that emits the particle systems inside its own file (an
/// explosion's flash, smoke and shrapnel) while it exists, each from its emitter object's
/// place in the file under this entity [hypothesis: as an add-on node emits its ADDN's;
/// the systems' rate and on/off keys come from the model's sequence when its emitter
/// controllers leave them to it, nvf_assets::particles].
#[derive(Component)]
pub struct ModelEmitter {
    pub path: String,
    age: f32,
    /// The placed reference whose model it is: its systems are its own, not shared with
    /// the other references of the same model (particles.md §7.1).
    placed: Option<u32>,
    /// The sequence its drawn model played last frame and how far into it (seconds of
    /// the sequence's time): births between that and now (particles.md §6.7).
    clock: Option<(String, f32)>,
    /// A held weapon's model ([`ModelEmitter::held`]): its systems emit only as the
    /// holder's clips say, by system name: on, and the clip's birth rate where it keys
    /// one. A system not named does not emit.
    pub driven: Option<HashMap<String, Option<f32>>>,
}

impl ModelEmitter {
    pub fn new(path: impl Into<String>) -> ModelEmitter {
        ModelEmitter { path: path.into(), age: 0.0, placed: None, clock: None, driven: None }
    }

    /// The model of placed reference `r` (a dust cloud, a sconce's flame, a smoke column).
    pub fn placed(path: impl Into<String>, r: u32) -> ModelEmitter {
        ModelEmitter { path: path.into(), age: 0.0, placed: Some(r), clock: None, driven: None }
    }

    /// The model of placed reference `r` whose drawn rig starts its Idle `start` seconds
    /// in ("Random Anim Start" bases): the emitter's own time starts there too. Systems
    /// the rig's playing sequence drives follow that sequence's clock (§6.7); this keeps
    /// the rest, and the mover of those that are not driven (particles.md §6.8), in step
    /// with the rig and its sounds.
    pub fn placed_from(path: impl Into<String>, r: u32, start: f32) -> ModelEmitter {
        ModelEmitter { path: path.into(), age: start.max(0.0), placed: Some(r), clock: None, driven: None }
    }

    /// The model of a weapon in an actor's hand: its systems stay off until the actor's
    /// clips switch them on (the dynamite's fuse sparks while the throw clip lights it,
    /// particles.md §7.6); `driven` is set each frame by whoever plays the clips.
    pub fn held(path: impl Into<String>) -> ModelEmitter {
        ModelEmitter { path: path.into(), age: 0.0, placed: None, clock: None, driven: Some(HashMap::new()) }
    }
}

/// A model emitter drawn with its own nodes: the rig's entity, and the sequence playing
/// on it with the span of its time this frame covers (`None`: none plays).
struct Drive {
    rig: Entity,
    playing: Option<(String, f32, f32)>,
    /// The sequence began this frame (logged).
    began: bool,
}

/// Whose systems: an ADDN's (by node index) or a model file's own (by path).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Source {
    Addon(i32),
    Model(String),
}

impl Source {
    fn label(&self) -> String {
        match self {
            Source::Addon(i) => format!("add-on node {i}"),
            Source::Model(path) => path.clone(),
        }
    }
}

/// Live systems: a source's shared ones (`instance` `None`: an ADDN's master systems, a
/// shot effect model's), or one placed model's own (`instance`: its emitter entity).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    source: Source,
    instance: Option<Entity>,
}

struct Particle {
    /// Its emitter node's place in the world when it was born: where its file's bombs,
    /// colliders and gravity points are.
    frame: Affine3A,
    pos: Vec3,
    vel: Vec3,
    /// The scale of its speed and gravity when it was born: the reference's
    /// ([`frame_scale`]) times the system node's (particles.md §7.7).
    scale: f32,
    /// Its emitter's time when it was born: the emitter's time now is this plus `age`
    /// (for modifiers switched on and off on the emitter's time).
    born_at: f32,
    /// The sequence (index among the file's) whose time `born_at` is, when a drawn
    /// model's sequence drove its emitter (particles.md §6.7).
    sequence: Option<usize>,
    age: f32,
    life: f32,
    radius: f32,
    angle: f32,
    spin: f32,
    tex: usize,
    /// A strip system's recorded points, oldest first, and the time since the last.
    trail: Vec<Vec3>,
    trail_clock: f32,
}

struct Live {
    def: ParticleSystemDef,
    particles: Vec<Particle>,
    /// The previous frame's mesh storage. Geometry is built here, then exchanged with
    /// the mesh's attribute vectors so steady-state draws retain their capacities.
    geometry: ParticleGeometry,
    mesh: Handle<Mesh>,
    entity: Entity,
    /// Emitters served this frame.
    served: usize,
    /// The part of a particle owed from earlier frames. Kept by the system, not by the
    /// emitter: a muzzle flash lives one frame, in which its share of the birth rate is
    /// about one particle, and a count kept by each flash would round most of them away
    /// [design].
    owed: f32,
}

/// CPU-side particle geometry exchanged with a live mesh after each draw.
///
/// The mesh and this scratch storage each retain one set of capacities. Replacing mesh
/// attributes with new vectors every frame would drop the mesh's old allocation; swapping
/// lets both sets reach their high-water marks and be reused on subsequent frames.
///
/// The ADDN and shot effect systems live as long as the run, so a burst would otherwise
/// pin its high-water mark for good: each rebuilt set gives back memory once it uses
/// under a quarter of it ([`trim`]), so both sets stay within about four times the
/// last frames' geometry, and a system that empties drops its scratch set.
#[derive(Default)]
struct ParticleGeometry {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
    /// A ribbon's points (its trail and its place), reused from ribbon to ribbon.
    points: Vec<Vec3>,
}

/// Shrink `v` to twice its length when it uses under a quarter of its capacity.
fn trim<T>(v: &mut Vec<T>) {
    if v.len() < v.capacity() / 4 {
        v.shrink_to(v.len() * 2);
    }
}

impl ParticleGeometry {
    fn clear(&mut self) {
        self.positions.clear();
        self.normals.clear();
        self.uvs.clear();
        self.colors.clear();
        self.indices.clear();
    }

    fn trim(&mut self) {
        trim(&mut self.positions);
        trim(&mut self.normals);
        trim(&mut self.uvs);
        trim(&mut self.colors);
        trim(&mut self.indices);
    }

    fn rebuild(&mut self, d: &ParticleSystemDef, particles: &[Particle], right: Vec3, up: Vec3, back: Vec3) {
        self.clear();
        let n = particles.len();
        self.positions.reserve(n * 4);
        self.uvs.reserve(n * 4);
        self.colors.reserve(n * 4);
        self.indices.reserve(n * 6);
        if d.strip.is_some() {
            // Ribbons through each particle's recorded points and its place, facing the
            // camera; the texture rectangle's width across, its height along, the oldest
            // point at its bottom (particles.md §6.2).
            for q in particles {
                let points = &mut self.points;
                points.clear();
                points.extend(q.trail.iter().copied().chain([q.pos]));
                let count = points.len();
                if count < 2 {
                    continue;
                }
                let t = (q.age / q.life).clamp(0.0, 1.0);
                let half = q.radius * d.scale(q.age, q.life);
                let [u, w, v, h] = d.subtextures.get(q.tex).copied().unwrap_or([0.0, 1.0, 0.0, 1.0]);
                let tint = d.tint(t);
                let e = d.emitter.color;
                let colour = display_colour([tint[0] * e[0], tint[1] * e[1], tint[2] * e[2], tint[3] * e[3] * keyed_alpha(d, q)]);
                let base = self.positions.len() as u32;
                for (i, p) in self.points.iter().enumerate() {
                    let tangent = self.points[(i + 1).min(count - 1)] - self.points[i.saturating_sub(1)];
                    let side = tangent.cross(back).normalize_or(right) * half;
                    self.positions.extend([(*p - side).to_array(), (*p + side).to_array()]);
                    let along = v + h * (1.0 - i as f32 / (count - 1) as f32);
                    self.uvs.extend([[u, along], [u + w, along]]);
                    self.colors.extend([colour; 2]);
                }
                for i in 0..(count as u32 - 1) {
                    let a = base + i * 2;
                    self.indices.extend([a, a + 1, a + 3, a, a + 3, a + 2]);
                }
            }
        }
        for (k, q) in particles.iter().enumerate().filter(|_| d.strip.is_none()) {
            let t = (q.age / q.life).clamp(0.0, 1.0);
            let half = q.radius * d.scale(q.age, q.life);
            let (s, c) = q.angle.sin_cos();
            let (x, y) = ((right * c + up * s) * half, (up * c - right * s) * half);
            for corner in [-x - y, x - y, x + y, -x + y] {
                self.positions.push((q.pos + corner).to_array());
            }
            let [u, w, v, h] = d.subtextures.get(q.tex).copied().unwrap_or([0.0, 1.0, 0.0, 1.0]);
            self.uvs.extend([[u, v + h], [u + w, v + h], [u + w, v], [u, v]]);
            let tint = d.tint(t);
            let e = d.emitter.color;
            self.colors.extend([display_colour([tint[0] * e[0], tint[1] * e[1], tint[2] * e[2], tint[3] * e[3] * keyed_alpha(d, q)]); 4]);
            let b = (k * 4) as u32;
            self.indices.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        self.normals.resize(self.positions.len(), back.to_array());
        self.trim();
        trim(&mut self.points);
    }

    /// Replace every live attribute's contents while retaining both the mesh and scratch
    /// capacities. [`particle_mesh`] establishes these exact attribute and index types;
    /// should a mesh lack one or hold another type, the geometry is inserted instead
    /// (that set's capacity goes to the mesh) and false is returned.
    fn swap_into_mesh(&mut self, mesh: &mut Mesh) -> bool {
        let mut matched = swap_attribute(mesh, Mesh::ATTRIBUTE_POSITION, &mut self.positions, |v| match v {
            VertexAttributeValues::Float32x3(v) => Some(v),
            _ => None,
        });
        matched &= swap_attribute(mesh, Mesh::ATTRIBUTE_NORMAL, &mut self.normals, |v| match v {
            VertexAttributeValues::Float32x3(v) => Some(v),
            _ => None,
        });
        matched &= swap_attribute(mesh, Mesh::ATTRIBUTE_UV_0, &mut self.uvs, |v| match v {
            VertexAttributeValues::Float32x2(v) => Some(v),
            _ => None,
        });
        matched &= swap_attribute(mesh, Mesh::ATTRIBUTE_COLOR, &mut self.colors, |v| match v {
            VertexAttributeValues::Float32x4(v) => Some(v),
            _ => None,
        });
        if let Some(Indices::U32(values)) = mesh.indices_mut() {
            std::mem::swap(values, &mut self.indices);
        } else {
            mesh.insert_indices(Indices::U32(std::mem::take(&mut self.indices)));
            matched = false;
        }
        matched
    }
}

/// Swap `scratch` with the mesh's `attribute` when `values` finds a vector of its type
/// there; otherwise insert it (taking its storage) and return false.
fn swap_attribute<T>(mesh: &mut Mesh, attribute: MeshVertexAttribute, scratch: &mut Vec<T>, values: fn(&mut VertexAttributeValues) -> Option<&mut Vec<T>>) -> bool
where
    Vec<T>: Into<VertexAttributeValues>,
{
    if let Some(v) = mesh.attribute_mut(attribute.id).and_then(values) {
        std::mem::swap(v, scratch);
        return true;
    }
    mesh.insert_attribute(attribute, std::mem::take(scratch));
    false
}

#[derive(Resource)]
pub struct Particles {
    /// The live systems of each ADDN index, shot effect model or placed model instance;
    /// `None`: none we can read.
    systems: HashMap<Key, Option<Vec<Live>>>,
    reported: HashSet<Source>,
    /// Sources whose systems were logged (once each, however many instances).
    described: HashSet<Source>,
    /// Particles born, per source and system (by name, in file order): the exit report.
    tally: HashMap<Source, Vec<(String, u64)>>,
    /// Placed references with an emitter, and those whose emitters gave birth.
    placed_seen: HashSet<u32>,
    placed_emitted: HashSet<u32>,
    /// The placed reference of each placed model's emitter, for the log.
    instance_refs: HashMap<Entity, u32>,
    /// Most placed model instances live at once.
    most_instances: usize,
    rng: SplitMix,
    pub born: u64,
    pub most_alive: usize,
}

impl Particles {
    /// The systems held (placed models' and shared ones), for the stress census. A
    /// system's entity always holds its materials, so an entry counts as idle when none of
    /// its systems has a particle alive (a shared explosion or flash between uses, a
    /// placed one whose emitter has stopped), or it has no systems we can read.
    pub fn share(&self, materials: &Assets<nvf_assets::NifMaterial>) -> [nvf_assets::release::CacheShare; 2] {
        let mut out: [nvf_assets::release::CacheShare; 2] = Default::default();
        for (k, live) in &self.systems {
            let s = &mut out[usize::from(k.instance.is_none())];
            s.entries += 1;
            s.idle += usize::from(live.iter().flatten().all(|l| l.particles.is_empty()));
            for l in live.iter().flatten() {
                s.images_of(materials, &l.def.material);
            }
        }
        out
    }
}

impl Default for Particles {
    fn default() -> Self {
        Particles {
            systems: HashMap::new(),
            reported: HashSet::new(),
            described: HashSet::new(),
            tally: HashMap::new(),
            placed_seen: HashSet::new(),
            placed_emitted: HashSet::new(),
            instance_refs: HashMap::new(),
            most_instances: 0,
            rng: SplitMix(0x2545_f491),
            born: 0,
            most_alive: 0,
        }
    }
}

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Particles>().add_systems(Update, (script_drawn, script_drawn_rig, staged_addons, emit.after(crate::effect_shaders::sync_player_effects), simulate, draw).chain().after(crate::shot_fx::ShotFxSet)).add_systems(Last, report);
    }
}

/// A reference a script enabled, drawn by nvf-play (`script_draw`): its model's own
/// systems and add-on nodes emit as render streaming's do (`world::spawn_emitters`,
/// particles.md §7.4): the smoke of a burning tyre pile a quest lights. One drawn with
/// its own nodes and named sequences (a `ScriptDrawnRig` on its rig's root, where
/// `object_anims` puts its `GroupRig`) emits its own systems from that root instead
/// ([`script_drawn_rig`]), so its sequences drive them (§6.7, §7.5: the rising B-29).
fn script_drawn(mut commands: Commands, mut models: ModelCtx, added: Query<(Entity, &ScriptDrawnRoot, &ScriptDrawnModel), Added<ScriptDrawnModel>>) {
    for (e, root, path) in &added {
        let Some(model) = models.model(&path.0) else { continue };
        // script_draw's rule for a `ScriptDrawnRig`: a rig with named sequences.
        if model.particles && model.rig.as_ref().is_some_and(|rig| !rig.groups.is_empty()) {
            // Its add-on nodes only here; its own systems are the rig root's.
            let addons_only = nvf_assets::model::PreparedModel { particles: false, ..(*model).clone() };
            crate::world::spawn_emitters(&mut commands, e, root.0, &addons_only, &path.0, Affine3A::IDENTITY);
        } else {
            crate::world::spawn_emitters(&mut commands, e, root.0, &model, &path.0, Affine3A::IDENTITY);
        }
    }
}

/// A script-drawn rig with named sequences: its model's own systems emit from the rig's
/// root, beside its `GroupRig` and the clip playing there (`emit`'s drive, §6.7).
fn script_drawn_rig(mut commands: Commands, mut models: ModelCtx, added: Query<(Entity, &ScriptDrawnRig), Added<ScriptDrawnRig>>) {
    for (e, rig) in &added {
        if models.model(&rig.path).is_some_and(|m| m.particles) {
            commands.entity(e).insert(ModelEmitter::placed(rig.path.clone(), rig.reference));
        }
    }
}

/// An entity's place in game space from this frame's transforms: the chain of local
/// transforms up to the world root.
pub fn game_affine(e: Entity, transforms: &Query<&Transform>, parents: &Query<&ChildOf>, root: Entity) -> Affine3A {
    let mut m = Affine3A::IDENTITY;
    let mut at = Some(e);
    let mut depth = 0;
    while let Some(p) = at.filter(|p| *p != root) {
        if let Ok(t) = transforms.get(p) {
            m = t.compute_affine() * m;
        }
        at = parents.get(p).ok().map(ChildOf::parent);
        depth += 1;
        if depth > 256 {
            break;
        }
    }
    m
}

fn systems_of<'a>(
    p: &'a mut Particles,
    key: &Key,
    sfx: Option<&Sfx>,
    models: &mut ModelCtx,
    commands: &mut Commands,
    root: Entity,
) -> Option<&'a mut Vec<Live>> {
    let source = &key.source;
    if !p.systems.contains_key(key) {
        let (path, what) = match source {
            Source::Addon(index) => {
                let addon = sfx.and_then(|s| s.forms.addons.get(index));
                (addon.and_then(|a| a.model.as_deref()).map(fnv_actor::resolve::mesh_path), format!("add-on node {index}"))
            }
            Source::Model(path) => (Some(path.clone()), "model".to_string()),
        };
        let prepared: Option<Arc<PreparedParticles>> = path.as_deref().and_then(|p| models.particles(p));
        let live = prepared.filter(|pp| !pp.systems.is_empty()).map(|pp| {
            if p.described.insert(source.clone()) {
                p.tally.insert(source.clone(), pp.systems.iter().map(|d| (d.name.clone(), 0)).collect());
                // Systems a sequence of the model drives, and those sequences' names.
                let driven = pp.systems.iter().filter(|d| !d.sequences.is_empty()).count();
                let mut names: Vec<&str> = Vec::new();
                for s in pp.systems.iter().flat_map(|d| &d.sequences) {
                    if !names.contains(&s.sequence.as_str()) {
                        names.push(&s.sequence);
                    }
                }
                info!(
                    "particles: {what} {}: {} system(s) [{}]{}{}",
                    path.as_deref().unwrap_or("?"),
                    pp.systems.len(),
                    pp.systems.iter().map(ParticleSystemDef::describe).collect::<Vec<_>>().join("; "),
                    if pp.unread.is_empty() { String::new() } else { format!("; not read: {}", pp.unread.join(", ")) },
                    if driven == 0 { String::new() } else { format!("; {driven} driven by the sequences [{}]", names.join(", ")) }
                );
            }
            pp.systems
                .iter()
                .map(|def| {
                    let mesh = models.meshes.add(particle_mesh());
                    let entity = commands
                        .spawn((Mesh3d(mesh.clone()), MeshMaterial3d(def.material.clone()), Transform::IDENTITY, Visibility::Hidden, NotShadowCaster, NoFrustumCulling, ChildOf(root)))
                        .id();
                    Live { def: def.clone(), particles: Vec::new(), geometry: ParticleGeometry::default(), mesh, entity, served: 0, owed: 0.0 }
                })
                .collect()
        });
        // A model without systems of its own is the common case (bullet impacts).
        if live.is_none() && matches!(source, Source::Addon(_)) && p.reported.insert(source.clone()) {
            warn!("particles: {what}: no particle systems ({})", path.as_deref().unwrap_or("no ADDN record or model"));
        }
        p.systems.insert(key.clone(), live);
    }
    p.systems.get_mut(key).and_then(Option::as_mut)
}

/// A number in −1..1.
/// The uniform scale of an emitter's place in the world: the mean length of its axes
/// (1 for a frame that does not scale). A placed reference at scale 0.5 (the Ultra-Luxe
/// fountain's ULFountainFX, issue #1547) gives 0.5.
fn frame_scale(frame: &Affine3A) -> f32 {
    let m = frame.matrix3;
    let k = (m.x_axis.length() + m.y_axis.length() + m.z_axis.length()) / 3.0;
    if k.is_finite() && k > 0.0 {
        k
    } else {
        1.0
    }
}

fn spread(rng: &mut SplitMix) -> f32 {
    rng.next_f32() * 2.0 - 1.0
}

/// A model emitter, its visibility, and the rig and sequence playing when it is drawn
/// with its own nodes.
type ModelEmitterItem = (
    Entity,
    &'static mut ModelEmitter,
    Option<&'static InheritedVisibility>,
    Option<&'static crate::object_anims::GroupRig>,
    Option<&'static Playing>,
    Option<&'static crate::effect_shaders::PlayerEffectEmitter>,
);

/// An emitter to serve this frame: its entity, systems, time, placed reference and drive.
/// An emitter, its systems' key, age, placed reference, the sequence driving its drawn
/// model, and the clips' word on a held model's systems ([`ModelEmitter::driven`]).
type EmitSource = (Entity, Key, f32, Option<u32>, Option<Drive>, Option<HashMap<String, Option<f32>>>);

/// Births: each add-on node in the world emits into the systems of its ADDN, each
/// model emitter into its file's own.
#[allow(clippy::too_many_arguments)]
fn emit(
    mut commands: Commands,
    time: Res<Time>,
    mut p: ResMut<Particles>,
    sfx: Option<Res<Sfx>>,
    mut models: ModelCtx,
    root: Query<(Entity, &Transform), With<WorldRoot>>,
    camera: Query<&Transform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    mut emitters: Query<(Entity, &mut AddonEmitter, Option<&InheritedVisibility>, Option<&crate::effect_shaders::PlayerEffectEmitter>)>,
    mut model_emitters: Query<ModelEmitterItem>,
    rigs: Query<&crate::object_anims::GroupRig>,
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
    body: Option<Res<crate::player_body::PlayerBody>>,
) {
    let Ok((root, root_t)) = root.single() else { return };
    // The camera in game space: emitters nearest it are served first.
    let eye = camera.single().ok().map(|c| Vec3A::from(root_t.compute_affine().inverse().transform_point3(c.translation)));
    let dt = time.delta_secs();
    let p = &mut *p;
    for live in p.systems.values_mut().flatten().flatten() {
        live.served = 0;
    }
    let mut sources: Vec<EmitSource> = Vec::new();
    // Placed model instances whose emitter still exists (hidden or not), and those of
    // them hidden this frame.
    let mut present: HashSet<Entity> = HashSet::new();
    let mut hidden_now: HashSet<Entity> = HashSet::new();
    // A hidden emitter (a disabled placed fire, `world::spawn_emitters`) keeps its time
    // but makes nothing. Not in its first frame, before its visibility was worked out:
    // a muzzle flash lives one frame [design].
    let hidden = |age: f32, v: Option<&InheritedVisibility>| age > 0.0 && v.is_some_and(|v| !v.get());
    // [design: effect_shaders::player_addon] A player's hidden part suppresses births
    // immediately, including an emitter's first frame before visibility propagates.
    let part_seen = |part: Option<&crate::effect_shaders::PlayerEffectEmitter>| part.is_none_or(|part| body.as_deref().is_some_and(|b| b.sees_effect(part.0)));
    for (e, mut em, v, part) in &mut emitters {
        if let Some(r) = em.placed {
            p.placed_seen.insert(r);
        }
        if part_seen(part) && !hidden(em.age, v) {
            sources.push((e, Key { source: Source::Addon(em.index), instance: None }, em.age, em.placed, None, None));
        }
        em.age += dt;
    }
    for (e, mut em, v, rig, playing, part) in &mut model_emitters {
        // A placed model's systems are its own (particles.md §7.1).
        let instance = em.placed.map(|_| e);
        if let Some(r) = em.placed {
            p.placed_seen.insert(r);
            present.insert(e);
            p.instance_refs.insert(e, r);
        }
        // Drawn with its own nodes: the sequence playing on it and the span of its time
        // since last frame; a sequence just begun counts from its start, unless it was
        // shown already well into it (drawn at a group's held end).
        let drive = rig.map(|_| {
            let now = playing.map(|p| (p.name.clone(), (p.time - p.start).max(0.0)));
            let last = em.clock.take();
            let mut began = false;
            let span = now.as_ref().map(|(name, t)| {
                let from = match &last {
                    Some((n, l)) if n == name && *l <= *t => *l,
                    _ => {
                        began = true;
                        if *t <= 0.25 { 0.0 } else { (*t - dt).max(0.0) }
                    }
                };
                (name.clone(), from, *t)
            });
            em.clock = now;
            Drive { rig: e, playing: span, began }
        });
        if em.placed.is_some() && hidden(em.age, v) {
            hidden_now.insert(e);
        }
        // A held model whose clips switch nothing on costs nothing.
        let idle = em.driven.as_ref().is_some_and(HashMap::is_empty);
        if part_seen(part) && !hidden(em.age, v) && !idle {
            sources.push((e, Key { source: Source::Model(em.path.clone()), instance }, em.age, em.placed, drive, em.driven.clone()));
        }
        em.age += dt;
    }
    // A master system serves at most `max_emitters` emitters a frame: the ones nearest the
    // camera [hypothesis, particles.md §7.2], so the fires in view burn when more barrels
    // than that are loaded.
    let places: Vec<Affine3A> = sources.iter().map(|(e, ..)| game_affine(*e, &transforms, &parents, root)).collect();
    let mut order: Vec<usize> = (0..sources.len()).collect();
    if let Some(eye) = eye {
        order.sort_by(|&a, &b| places[a].translation.distance_squared(eye).total_cmp(&places[b].translation.distance_squared(eye)));
    }
    for i in order {
        let (_, key, age, placed, drive, driven) = &sources[i];
        let (age, placed) = (*age, *placed);
        let rig = drive.as_ref().and_then(|d| rigs.get(d.rig).ok());
        let mut rng = SplitMix(p.rng.0);
        let Some(systems) = systems_of(p, key, sfx.as_deref(), &mut models, &mut commands, root) else { continue };
        let node = places[i];
        let mut counts = vec![0u64; systems.len()];
        let mut born = 0;
        if let Some((name, ..)) = drive.as_ref().filter(|d| d.began).and_then(|d| d.playing.as_ref()) {
            let by = systems.iter().filter(|l| l.def.sequence(name).is_some()).count();
            if by > 0 {
                info!("particles: {:08X} {} emits as its {name} says: {by} of {} systems", placed.unwrap_or(0), key.source.label(), systems.len());
            }
        }
        for (k, live) in systems.iter_mut().enumerate() {
            let owed = &mut live.owed;
            let d = &live.def;
            // A drawn model's sequences drive it: by the one playing, on its time, else
            // not at all (particles.md §6.7).
            let by_sequence = drive.as_ref().filter(|_| !d.sequences.is_empty()).map(|dr| dr.playing.as_ref().and_then(|(name, from, to)| d.sequence(name).map(|s| (s, *from, *to))));
            let (on, rate, t, sequence) = match by_sequence {
                Some(None) => continue,
                Some(Some((s, from, to))) => (s.on_time(from, to), s.rate_at((from + to) * 0.5), to, Some(s)),
                None => match driven.as_ref().map(|m| m.get(&d.name).copied()) {
                    // A held model's system: on for the whole frame while the clips say
                    // so, at their rate where they key one, else its own (§7.6).
                    Some(None) => continue,
                    Some(Some(rate)) => (dt, rate.unwrap_or_else(|| d.rate_at(age + dt * 0.5)), age, None),
                    // A keyed rate at the middle of the frame (particles.md §6.6).
                    None => (d.on_time(age, age + dt), d.rate_at(age + dt * 0.5), age, None),
                },
            };
            // The emitter's node as a sequence of its file moves it (a dust devil on its
            // path, particles.md §6.8): on the drawn model's sequence time when one drives
            // the system, else on the emitter's own (`t`, the same clock as the births).
            // Box emitters also take their object's place from the drawn rig below; the
            // mover adds the mesh emitters and the bombs under the moved node.
            let node = node * d.moved(t);
            if on <= 0.0 || (d.max_emitters > 0 && live.served >= d.max_emitters) {
                continue;
            }
            live.served += 1;
            *owed += rate * on;
            // From its emitter object's node as the drawn model's sequence moves it.
            let moved = rig.filter(|_| d.emitter.mesh.is_none()).and_then(|r| r.node(d.emitter.object.as_deref()?)).map(|e| game_affine(e, &transforms, &parents, root));
            let frame = moved.unwrap_or(node * d.emitter.frame);
            let (_, turn, origin) = frame.to_scale_rotation_translation();
            // The reference's scale and the system node's set the spray's speed and its
            // gravity (particles.md §7.7).
            let spray = frame_scale(&node) * d.system_scale;
            // The volume is the emitter object's, at that object's scale.
            let volume = frame_scale(&frame);
            let vals = match sequence {
                Some(s) => d.emitter.values_in(t, s),
                None => d.emitter.values(age),
            };
            // Births past the free slots are dropped (as the loop below would skip them);
            // capping here keeps a broken file's huge or infinite rate from looping
            // without end.
            let free = d.max.saturating_sub(live.particles.len()) as f32;
            *owed = if owed.is_finite() { owed.min(free + owed.fract()) } else { 0.0 };
            while *owed >= 1.0 {
                *owed -= 1.0;
                if live.particles.len() >= d.max {
                    continue;
                }
                let m = &d.emitter;
                // Where it starts and the axis of its cone: on a mesh, along the normal
                // there (or at random, or a set axis); else somewhere in the emitter's
                // volume, about its +z.
                let (start, axis) = match &m.mesh {
                    Some(me) => {
                        let mesh = &me.meshes[((rng.next_f32() * me.meshes.len() as f32) as usize).min(me.meshes.len() - 1)];
                        let Some((at, normal)) = mesh.pick(me.from, &mut || rng.next_f32()) else { continue };
                        let mf = node * mesh.frame;
                        let along = match me.velocity {
                            InitialVelocity::Normals => mf.transform_vector3(normal),
                            InitialVelocity::Random => Vec3::new(spread(&mut rng), spread(&mut rng), spread(&mut rng)),
                            InitialVelocity::Direction(a) => mf.transform_vector3(a),
                        };
                        (mf.transform_point3(at), Quat::from_rotation_arc(Vec3::Z, along.normalize_or(Vec3::Z)))
                    }
                    None => {
                        let offset = if m.round {
                            let a = rng.next_f32() * std::f32::consts::TAU;
                            let r = m.size.x * rng.next_f32().sqrt();
                            Vec3::new(a.cos() * r, a.sin() * r, m.size.z * spread(&mut rng) * 0.5)
                        } else {
                            m.size * Vec3::new(spread(&mut rng), spread(&mut rng), spread(&mut rng)) * 0.5
                        };
                        (origin + turn * offset * volume, turn)
                    }
                };
                // Thrown within the cone: so far from the axis, any way round it.
                let decl = vals.declination + vals.declination_variation * spread(&mut rng);
                let planar = vals.planar_angle + vals.planar_angle_variation * spread(&mut rng);
                let dir = Vec3::new(decl.sin() * planar.cos(), decl.sin() * planar.sin(), decl.cos());
                let speed = (vals.speed + vals.speed_variation * spread(&mut rng) * 0.5) * spray;
                let rot = d.rotation.clone().unwrap_or_default();
                let mut spin = rot.speed + rot.speed_variation * spread(&mut rng);
                if rot.random_sign && rng.next_f32() < 0.5 {
                    spin = -spin;
                }
                live.particles.push(Particle {
                    frame: node,
                    pos: start,
                    vel: axis * dir * speed,
                    scale: spray,
                    born_at: t,
                    sequence: sequence.map(|s| s.index),
                    age: 0.0,
                    life: (vals.life + vals.life_variation * spread(&mut rng) * 0.5).max(0.02),
                    // Sized by the system node's scale; the reference's scale moves and speeds
                    // the spray but leaves the drops' size (particles.md §3, §7.7).
                    radius: (vals.radius + vals.radius_variation * spread(&mut rng)).max(0.0) * d.system_scale,
                    angle: rot.angle + rot.angle_variation * spread(&mut rng),
                    spin,
                    tex: if d.subtextures.is_empty() { 0 } else { ((rng.next_f32() * d.subtextures.len() as f32) as usize).min(d.subtextures.len() - 1) },
                    // A strip starts where its particle is born.
                    trail: if d.strip.is_some() { vec![start] } else { Vec::new() },
                    trail_clock: 0.0,
                });
                born += 1;
                counts[k] += 1;
            }
        }
        p.rng = rng;
        p.born += born;
        if born > 0 {
            if let Some(t) = p.tally.get_mut(&key.source) {
                for (slot, n) in t.iter_mut().zip(&counts) {
                    slot.1 += n;
                }
            }
            if let Some(r) = placed {
                p.placed_emitted.insert(r);
            }
        }
    }
    // A placed model's own particles go at once when it is no longer drawn: its emitter
    // hidden (a script disabled it, the item was taken) or gone (its cell unloaded, the
    // reference's model taken down) [design, #1596: a taken snow globe's snow went on
    // falling where it stood]. Its systems go with an emitter that is gone.
    for (k, live) in p.systems.iter_mut() {
        let Some(e) = k.instance.filter(|e| hidden_now.contains(e) || !present.contains(e)) else { continue };
        let n: usize = live.iter().flatten().map(|s| s.particles.len()).sum();
        if n > 0 {
            info!("particles: {:08X} is not drawn any more: its {n} live particles go", p.instance_refs.get(&e).copied().unwrap_or(0));
            for s in live.iter_mut().flatten() {
                s.particles.clear();
            }
        }
    }
    let gone: Vec<Key> = p.systems.keys().filter(|k| k.instance.is_some_and(|e| !present.contains(&e))).cloned().collect();
    p.instance_refs.retain(|e, _| present.contains(e));
    for k in gone {
        for live in p.systems.remove(&k).flatten().unwrap_or_default() {
            commands.entity(live.entity).try_despawn();
            models.meshes.remove(&live.mesh);
        }
    }
    let instances = p.systems.keys().filter(|k| k.instance.is_some()).count();
    p.most_instances = p.most_instances.max(instances);
}

/// Ageing and movement of every live particle.
fn simulate(time: Res<Time>, mut p: ResMut<Particles>) {
    let dt = time.delta_secs();
    let mut alive = 0;
    for live in p.systems.values_mut().flatten().flatten() {
        let d = &live.def;
        for q in &mut live.particles {
            q.age += dt;
            let at = q.born_at + q.age;
            // Born under a drawn model's sequence: that sequence's keys and length.
            let (seq, period, loops) = match q.sequence.and_then(|i| d.sequences.iter().find(|s| s.index == i)) {
                Some(s) => (Some(s.index), s.period, s.loops),
                None => (None, d.period, d.loops),
            };
            for g in d.gravity.iter().filter(|g| g.active.on_in(seq, at, period, loops)) {
                let centre = q.frame.transform_point3(g.origin);
                let pull = if g.spherical { (centre - q.pos).normalize_or_zero() } else { g.direction };
                // Weaker away from its object, by the distance in the system's own units
                // (those of its speeds and radii: `scale`).
                let fall = g.falloff(q.pos.distance(centre) / q.scale);
                q.vel += pull * g.strength_at(at) * fall * q.scale * dt;
            }
            for b in d.bombs.iter().filter(|b| b.active.on_in(seq, at, period, loops)) {
                q.vel += b.push(&q.frame, q.pos) * dt;
            }
            for drag in d.drag.iter().filter(|g| g.active.on_in(seq, at, period, loops)) {
                // The share of the speed along the axis kept after `dt`.
                // Only near its object, when the file gives it a range.
                // Its range is in the model's units, about its object's place in the file
                // (particles.md §7.7: fitted to the owner's video of the fountain).
                let reach = drag.reach(q.pos.distance(q.frame.transform_point3(drag.origin)) / frame_scale(&q.frame));
                if reach <= 0.0 {
                    continue;
                }
                let keep = (1.0 - drag.percentage.clamp(0.0, 1.0) * reach).powf(dt * DRAG_STEPS_PER_SECOND);
                let along = q.vel.dot(drag.axis);
                q.vel -= drag.axis * along * (1.0 - keep);
            }
            let to = q.pos + q.vel * dt;
            match d.colliders.iter().find_map(|c| c.hit(&q.frame, q.pos, to, q.vel)) {
                None => q.pos = to,
                Some(None) => q.life = q.age,
                Some(Some((at, vel))) => {
                    q.pos = at;
                    q.vel = vel;
                }
            }
            q.angle += q.spin * dt;
            if let Some(st) = d.strip {
                q.trail_clock += dt;
                while q.trail_clock >= st.step {
                    q.trail_clock -= st.step;
                    q.trail.push(q.pos);
                }
                // The recorded points and the particle itself make at most `max_points`.
                let keep = st.max_points.saturating_sub(1).max(1);
                if q.trail.len() > keep {
                    q.trail.drain(..q.trail.len() - keep);
                }
            }
        }
        live.particles.retain(|q| q.age < q.life);
        alive += live.particles.len();
    }
    p.most_alive = p.most_alive.max(alive);
}

/// One mesh per system: a square facing the camera for each particle.
fn draw(
    mut p: ResMut<Particles>,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    root: Query<&Transform, With<WorldRoot>>,
    mut visibility: Query<&mut Visibility>,
) {
    let (Ok(cam), Ok(root)) = (camera.single(), root.single()) else { return };
    // The camera's right, up and backward in game space.
    let to_game = root.compute_affine().inverse();
    let right = to_game.transform_vector3(cam.rotation * Vec3::X).normalize_or(Vec3::X);
    let up = to_game.transform_vector3(cam.rotation * Vec3::Y).normalize_or(Vec3::Z);
    let back = to_game.transform_vector3(cam.rotation * Vec3::Z).normalize_or(Vec3::Y);
    for live in p.systems.values_mut().flatten().flatten() {
        let shown = !live.particles.is_empty();
        if let Ok(mut v) = visibility.get_mut(live.entity) {
            let want = if shown { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
        if !shown {
            // An emptied system keeps only the mesh's set (its last geometry, hidden).
            if live.geometry.positions.capacity() > 0 {
                live.geometry = ParticleGeometry::default();
            }
            continue;
        }
        let Some(mut mesh) = meshes.get_mut(&live.mesh) else { continue };
        live.geometry.rebuild(&live.def, &live.particles, right, up, back);
        let matched = live.geometry.swap_into_mesh(&mut mesh);
        debug_assert!(matched, "particles: {}: mesh layout differs from particle_mesh's; geometry inserted", live.def.name);
    }
}

/// The system's keyed material alpha (`ParticleSystemDef::alpha_keys`) at the emitter
/// time particle `q` is drawn at (its birth time plus its age), 1 without keys
/// (particles.md §6.9).
fn keyed_alpha(d: &ParticleSystemDef, q: &Particle) -> f32 {
    d.alpha_keys.as_ref().and_then(|k| k.at(q.born_at + q.age)).map_or(1.0, |a| a.clamp(0.0, 1.0))
}

/// A particle's colour (the colour modifier's times the emitter's) taken as display
/// values: converted to linear, the alpha kept. The particle systems are unlit
/// (BSShaderNoLightingProperty), and an unlit shape's texture × vertex colour is formed
/// in display values [hypothesis, as `nvf_assets::nif_spawn::unlit_vertex_colors` for
/// placed smoke, nif.md §6 "Vertex colours"]: multiplied in as linear values, the impact
/// dust's 0.27–0.43 greys drew as 0.57–0.69, a pale cloud on every wall hit (owner,
/// playtest list 5 item 48; particles.md §2 "Simple colour").
fn display_colour(c: [f32; 4]) -> [f32; 4] {
    let l = |v: f32| Color::srgb(v.max(0.0), 0.0, 0.0).to_linear().red;
    [l(c[0]), l(c[1]), l(c[2]), c[3]]
}

fn report(mut exit: MessageReader<AppExit>, p: Res<Particles>) {
    if exit.read().last().is_some() && p.born > 0 {
        info!("particles: {} born in {} add-on systems, at most {} alive at once", p.born, p.systems.values().flatten().count(), p.most_alive);
        // Each system that emitted, by its file (all instances of a placed model together).
        for (source, systems) in &p.tally {
            let born: Vec<String> = systems.iter().filter(|(_, n)| *n > 0).map(|(name, n)| format!("{name} {n}")).collect();
            if !born.is_empty() {
                info!("particles: born in {}: {}", source.label(), born.join(", "));
            }
        }
        info!(
            "particles: placed references emitting: {} of {} with particle systems, at most {} placed model instances at once",
            p.placed_emitted.len(),
            p.placed_seen.len(),
            p.most_instances
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvf_assets::particles::{Emitter, Strip};

    fn test_def(strip: Option<Strip>) -> ParticleSystemDef {
        ParticleSystemDef {
            name: "test".into(),
            max: 0,
            material: default(),
            emitter: Emitter {
                frame: Affine3A::IDENTITY,
                size: Vec3::ZERO,
                round: false,
                speed: 0.0,
                speed_variation: 0.0,
                declination: 0.0,
                declination_variation: 0.0,
                planar_angle: 0.0,
                planar_angle_variation: 0.0,
                color: [1.0; 4],
                radius: 1.0,
                radius_variation: 0.0,
                life: 1.0,
                life_variation: 0.0,
                mesh: None,
                keyed: Vec::new(),
                object: None,
            },
            rate: 0.0,
            rate_keys: None,
            active: Vec::new(),
            period: 0.0,
            loops: false,
            sequences: Vec::new(),
            max_emitters: 0,
            grow_time: 0.0,
            fade_time: 0.0,
            base_scale: 1.0,
            system_scale: 1.0,
            color: None,
            color_keys: None,
            rotation: None,
            gravity: Vec::new(),
            drag: Vec::new(),
            bombs: Vec::new(),
            colliders: Vec::new(),
            strip,
            subtextures: vec![[0.25, 0.5, 0.125, 0.75]],
            mover: None,
            alpha_keys: None,
            soft: 0.0,
        }
    }

    #[test]
    fn an_emitters_scale_is_the_mean_length_of_its_axes() {
        assert_eq!(frame_scale(&Affine3A::IDENTITY), 1.0);
        let half = Affine3A::from_scale_rotation_translation(Vec3::splat(0.5), Quat::from_rotation_z(1.0), Vec3::new(1176.0, -222.0, 1012.0));
        assert!((frame_scale(&half) - 0.5).abs() < 1e-5);
        // A frame squashed to nothing sizes nothing down to zero: 1.
        assert_eq!(frame_scale(&Affine3A::from_scale(Vec3::ZERO)), 1.0);
    }

    fn test_particle(pos: Vec3, trail: Vec<Vec3>) -> Particle {
        Particle {
            frame: Affine3A::IDENTITY,
            pos,
            vel: Vec3::ZERO,
            scale: 1.0,
            born_at: 0.0,
            sequence: None,
            age: 0.25,
            life: 1.0,
            radius: 2.0,
            angle: 0.25,
            spin: 0.0,
            tex: 0,
            trail,
            trail_clock: 0.0,
        }
    }

    /// The pre-buffer-reuse construction path, retained only as a differential oracle.
    fn reference_geometry(d: &ParticleSystemDef, particles: &[Particle], right: Vec3, up: Vec3, back: Vec3) -> ParticleGeometry {
        let n = particles.len();
        let mut positions = Vec::with_capacity(n * 4);
        let mut uvs = Vec::with_capacity(n * 4);
        let mut colors = Vec::with_capacity(n * 4);
        let mut indices = Vec::with_capacity(n * 6);
        if d.strip.is_some() {
            for q in particles {
                let points: Vec<Vec3> = q.trail.iter().copied().chain([q.pos]).collect();
                let count = points.len();
                if count < 2 {
                    continue;
                }
                let t = (q.age / q.life).clamp(0.0, 1.0);
                let half = q.radius * d.scale(q.age, q.life);
                let [u, w, v, h] = d.subtextures.get(q.tex).copied().unwrap_or([0.0, 1.0, 0.0, 1.0]);
                let tint = d.tint(t);
                let e = d.emitter.color;
                let colour = display_colour([tint[0] * e[0], tint[1] * e[1], tint[2] * e[2], tint[3] * e[3] * keyed_alpha(d, q)]);
                let base = positions.len() as u32;
                for (i, p) in points.iter().enumerate() {
                    let tangent = points[(i + 1).min(count - 1)] - points[i.saturating_sub(1)];
                    let side = tangent.cross(back).normalize_or(right) * half;
                    positions.extend([(*p - side).to_array(), (*p + side).to_array()]);
                    let along = v + h * (1.0 - i as f32 / (count - 1) as f32);
                    uvs.extend([[u, along], [u + w, along]]);
                    colors.extend([colour; 2]);
                }
                for i in 0..(count as u32 - 1) {
                    let a = base + i * 2;
                    indices.extend([a, a + 1, a + 3, a, a + 3, a + 2]);
                }
            }
        }
        for (k, q) in particles.iter().enumerate().filter(|_| d.strip.is_none()) {
            let t = (q.age / q.life).clamp(0.0, 1.0);
            let half = q.radius * d.scale(q.age, q.life);
            let (s, c) = q.angle.sin_cos();
            let (x, y) = ((right * c + up * s) * half, (up * c - right * s) * half);
            for corner in [-x - y, x - y, x + y, -x + y] {
                positions.push((q.pos + corner).to_array());
            }
            let [u, w, v, h] = d.subtextures.get(q.tex).copied().unwrap_or([0.0, 1.0, 0.0, 1.0]);
            uvs.extend([[u, v + h], [u + w, v + h], [u + w, v], [u, v]]);
            let tint = d.tint(t);
            let e = d.emitter.color;
            colors.extend([display_colour([tint[0] * e[0], tint[1] * e[1], tint[2] * e[2], tint[3] * e[3] * keyed_alpha(d, q)]); 4]);
            let b = (k * 4) as u32;
            indices.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        ParticleGeometry { normals: vec![back.to_array(); positions.len()], positions, uvs, colors, indices, points: Vec::new() }
    }

    fn assert_geometry_eq(actual: &ParticleGeometry, expected: &ParticleGeometry) {
        assert_eq!(actual.positions, expected.positions);
        assert_eq!(actual.normals, expected.normals);
        assert_eq!(actual.uvs, expected.uvs);
        assert_eq!(actual.colors, expected.colors);
        assert_eq!(actual.indices, expected.indices);
    }

    #[test]
    fn particle_colours_are_display_values() {
        let c = display_colour([0.5, 1.0, 0.0, 0.3]);
        assert!((c[0] - 0.214).abs() < 1e-3 && (c[1] - 1.0).abs() < 1e-6 && c[2] == 0.0 && c[3] == 0.3, "{c:?}");
    }

    #[test]
    fn billboard_geometry_matches_reference_across_growth_and_shrink() {
        let d = test_def(None);
        let particles = vec![test_particle(Vec3::new(1.0, 2.0, 3.0), Vec::new()), test_particle(Vec3::new(-2.0, 1.0, 0.0), Vec::new())];
        let (right, up, back) = (Vec3::X, Vec3::Z, Vec3::Y);
        let mut actual = ParticleGeometry::default();
        actual.rebuild(&d, &particles, right, up, back);
        assert_geometry_eq(&actual, &reference_geometry(&d, &particles, right, up, back));
        let capacities = (actual.positions.capacity(), actual.normals.capacity(), actual.uvs.capacity(), actual.colors.capacity(), actual.indices.capacity());

        let smaller = &particles[..1];
        actual.rebuild(&d, smaller, right, up, back);
        assert_geometry_eq(&actual, &reference_geometry(&d, smaller, right, up, back));
        assert!(actual.positions.capacity() >= capacities.0 && actual.normals.capacity() >= capacities.1 && actual.uvs.capacity() >= capacities.2 && actual.colors.capacity() >= capacities.3 && actual.indices.capacity() >= capacities.4);
    }

    #[test]
    fn strip_geometry_matches_reference_at_empty_and_one_point_boundaries() {
        let d = test_def(Some(Strip { max_points: 4, step: 0.1 }));
        let (right, up, back) = (Vec3::X, Vec3::Z, Vec3::Y);
        let mut actual = ParticleGeometry::default();

        actual.rebuild(&d, &[], right, up, back);
        assert_geometry_eq(&actual, &reference_geometry(&d, &[], right, up, back));

        let one_point = [test_particle(Vec3::new(1.0, 0.0, 0.0), Vec::new())];
        actual.rebuild(&d, &one_point, right, up, back);
        assert_geometry_eq(&actual, &reference_geometry(&d, &one_point, right, up, back));
        assert!(actual.positions.is_empty() && actual.indices.is_empty());

        let trail = [test_particle(Vec3::new(1.0, 0.0, 0.0), vec![Vec3::ZERO])];
        actual.rebuild(&d, &trail, right, up, back);
        assert_geometry_eq(&actual, &reference_geometry(&d, &trail, right, up, back));
        assert_eq!((actual.positions.len(), actual.indices.len()), (4, 6));
    }

    #[test]
    fn mesh_swaps_publish_current_geometry_and_reuse_both_capacity_sets() {
        fn assert_mesh(mesh: &Mesh, expected: &ParticleGeometry) {
            assert_eq!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap(), expected.positions);
            assert_eq!(mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap().as_float3().unwrap(), expected.normals);
            let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!("UV type") };
            let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else { panic!("colour type") };
            let Some(Indices::U32(indices)) = mesh.indices() else { panic!("index type") };
            assert_eq!(uvs, &expected.uvs);
            assert_eq!(colors, &expected.colors);
            assert_eq!(indices, &expected.indices);
        }
        fn capacities(g: &ParticleGeometry) -> [usize; 5] {
            [g.positions.capacity(), g.normals.capacity(), g.uvs.capacity(), g.colors.capacity(), g.indices.capacity()]
        }
        for strip in [None, Some(Strip { max_points: 4, step: 0.1 })] {
            let d = test_def(strip);
            let particles = [
                test_particle(Vec3::new(1.0, 2.0, 3.0), vec![Vec3::ZERO, Vec3::X]),
                test_particle(Vec3::new(-2.0, 1.0, 0.0), vec![Vec3::Y]),
                test_particle(Vec3::new(2.0, -1.0, 0.0), Vec::new()),
            ];
            let mut geometry = ParticleGeometry::default();
            let mut mesh = particle_mesh();
            // Warm both storage sets before shrinking and growing again (an empty system
            // is not rebuilt: `draw` hides it and drops the scratch set).
            for _ in 0..3 {
                assert!(geometry.rebuild_and_swap(&d, &particles, Vec3::X, Vec3::Z, Vec3::Y, &mut mesh));
            }
            let retained = capacities(&geometry);
            for (frame, count) in [1, 3, 2, 3, 1, 3].into_iter().enumerate() {
                let rotation = Quat::from_rotation_z(frame as f32 * 0.37);
                let (right, up, back) = (rotation * Vec3::X, Vec3::Z, rotation * Vec3::Y);
                let expected = reference_geometry(&d, &particles[..count], right, up, back);
                assert!(geometry.rebuild_and_swap(&d, &particles[..count], right, up, back, &mut mesh));
                assert_mesh(&mesh, &expected);
                assert_eq!(capacities(&geometry), retained);
            }
        }
    }

    impl ParticleGeometry {
        fn rebuild_and_swap(&mut self, d: &ParticleSystemDef, particles: &[Particle], right: Vec3, up: Vec3, back: Vec3, mesh: &mut Mesh) -> bool {
            self.rebuild(d, particles, right, up, back);
            self.swap_into_mesh(mesh)
        }
    }

    /// Two squares worked by hand: camera right X, up Z, back Y, no spin, radius 2, the
    /// subtexture u 0.25 + 0.5, v 0.125 + 0.75.
    #[test]
    fn billboard_geometry_by_hand() {
        let d = test_def(None);
        let mut a = test_particle(Vec3::new(1.0, 2.0, 3.0), Vec::new());
        a.angle = 0.0;
        let mut b = test_particle(Vec3::ZERO, Vec::new());
        b.angle = 0.0;
        let mut g = ParticleGeometry::default();
        g.rebuild(&d, &[a, b], Vec3::X, Vec3::Z, Vec3::Y);
        assert_eq!(&g.positions[..4], &[[-1.0, 2.0, 1.0], [3.0, 2.0, 1.0], [3.0, 2.0, 5.0], [-1.0, 2.0, 5.0]]);
        assert_eq!(&g.positions[4..], &[[-2.0, 0.0, -2.0], [2.0, 0.0, -2.0], [2.0, 0.0, 2.0], [-2.0, 0.0, 2.0]]);
        assert_eq!(&g.uvs[..4], &[[0.25, 0.875], [0.75, 0.875], [0.75, 0.125], [0.25, 0.125]]);
        assert_eq!(g.indices, [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
        assert_eq!(g.normals, vec![[0.0, 1.0, 0.0]; 8]);
        assert_eq!(g.colors, vec![[1.0; 4]; 8]);
    }

    /// A two-point ribbon from the origin to X: its sides 2 either way along Z (X × Y),
    /// the oldest point at the subtexture's bottom.
    #[test]
    fn ribbon_geometry_by_hand() {
        let d = test_def(Some(Strip { max_points: 4, step: 0.1 }));
        let mut g = ParticleGeometry::default();
        g.rebuild(&d, &[test_particle(Vec3::X, vec![Vec3::ZERO])], Vec3::X, Vec3::Z, Vec3::Y);
        assert_eq!(g.positions, [[0.0, 0.0, -2.0], [0.0, 0.0, 2.0], [1.0, 0.0, -2.0], [1.0, 0.0, 2.0]]);
        assert_eq!(g.uvs, [[0.25, 0.875], [0.75, 0.875], [0.25, 0.125], [0.75, 0.125]]);
        assert_eq!(g.indices, [0, 1, 3, 0, 3, 2]);
    }

    /// A mesh without `particle_mesh`'s layout (a missing attribute, another type, other
    /// indices) gets the geometry inserted, and the swap says so.
    #[test]
    fn mesh_with_another_layout_gets_geometry_inserted() {
        let d = test_def(None);
        let particles = [test_particle(Vec3::ZERO, Vec::new()), test_particle(Vec3::X, Vec::new())];
        let expected = reference_geometry(&d, &particles, Vec3::X, Vec3::Z, Vec3::Y);
        let mut mesh = particle_mesh();
        mesh.remove_attribute(Mesh::ATTRIBUTE_NORMAL);
        // (insert_attribute refuses a wrong format, so the colours are changed in place.)
        *mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR).unwrap() = VertexAttributeValues::Float32x3(vec![[0.0; 3]]);
        mesh.insert_indices(Indices::U16(vec![0]));
        let mut g = ParticleGeometry::default();
        assert!(!g.rebuild_and_swap(&d, &particles, Vec3::X, Vec3::Z, Vec3::Y, &mut mesh));
        assert_eq!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap(), expected.positions);
        assert_eq!(mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap().as_float3().unwrap(), expected.normals);
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else { panic!("colour type") };
        assert_eq!(colors, &expected.colors);
        let Some(Indices::U32(indices)) = mesh.indices() else { panic!("index type") };
        assert_eq!(indices, &expected.indices);
        // Once inserted the layout matches, and the next frame swaps again.
        assert!(g.rebuild_and_swap(&d, &particles, Vec3::X, Vec3::Z, Vec3::Y, &mut mesh));
    }

    /// After a burst, a set drawing a quarter or less of its capacity gives memory back.
    #[test]
    fn burst_capacity_is_given_back() {
        let d = test_def(None);
        let burst: Vec<Particle> = (0..256).map(|i| test_particle(Vec3::splat(i as f32), Vec::new())).collect();
        let mut g = ParticleGeometry::default();
        let mut mesh = particle_mesh();
        for _ in 0..2 {
            g.rebuild_and_swap(&d, &burst, Vec3::X, Vec3::Z, Vec3::Y, &mut mesh);
        }
        assert!(g.positions.capacity() >= 1024);
        for _ in 0..2 {
            g.rebuild_and_swap(&d, &burst[..4], Vec3::X, Vec3::Z, Vec3::Y, &mut mesh);
        }
        // Both sets (the scratch one and the mesh's) are back to twice four squares.
        assert!(g.positions.capacity() <= 32 && g.indices.capacity() <= 48, "{} {}", g.positions.capacity(), g.indices.capacity());
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!("position type") };
        assert!(positions.capacity() <= 32);
    }
}
