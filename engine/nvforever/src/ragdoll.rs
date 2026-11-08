//! Ragdolls of dead actors (docs/spec-notes/ragdoll.md §4): when an actor dies, the
//! Havok bodies and constraints of its skeleton (`fnv_collide::ragdoll`) become jointed
//! rigid bodies in the physics world (`fnv_phys::ragdoll`) at the pose the skeleton is
//! in, and from then on the bodies move the bones: the corpse drops and comes to rest.
//!
//! - `deaths.rs` marks a dead actor whose skeleton has a ragdoll with [`Corpse`].
//! - [`start`] puts the ragdoll in the world once the physics cell under the actor is
//!   loaded (until then the actor shows its death pose as before), and passes on the
//!   shoves of hits (`nvf_play::combat::ActorPush`), the killing one included.
//! - [`pose`] writes the bodies' poses to the bones after each physics step. Bones
//!   without a body (fingers, clavicles, twist bones) keep the pose they died in under
//!   their parent.
//! - Where a corpse came to rest is remembered by reference ([`Ragdolls::rest`]): a
//!   corpse whose ground unloads, or that is drawn again later, is put back there at rest
//!   instead of falling again [design: the original keeps corpse poses in its saves].
//! - A corpse that is hidden (a script disabled or deleted its reference) leaves the
//!   physics world.
//! - A corpse that gets under the terrain is lifted back above it (a guard, as for the
//!   player).
//! - A corpse in water rises and floats at the surface (`fnv_phys::buoyancy`); it is
//!   left moving for up to [`MAX_AWAKE_IN_WATER`] instead of [`MAX_AWAKE`] so that it
//!   gets there. A corpse put back at rest (or in the pose its reference stores) stays
//!   where it lay until a hit or a blast rouses it.
//! - The player's own body drops the same way when the player dies (`death_cam.rs`): its
//!   third-person rig carries a [`PlayerRig`] in place of an [`ActorRig`], so that the
//!   systems of placed actors (head tracking, hit volumes, outfits) leave it alone.

use crate::actors::ActorWorld;
use crate::limbs::{ActorRig, LimbRig};
use crate::locomotion::Dying;
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_collide::ragdoll::{Ragdoll, RagdollTemplate};
use fnv_collide::Iso;
use fnv_combat::data::Limb;
use fnv_nif::scene::Decoded;
use fnv_nif::NifFile;
use fnv_phys::{BodyState, RagdollStart};
use fnv_sim::FormId;
use nvf_assets::data::GameData;
use nvf_assets::guard::guarded;
use nvf_play::combat::ActorPush;
use nvf_play::phys::PhysState;
use nvf_play::space;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// A corpse this far under the terrain has fallen through it, game units; it is put back
/// this far above it, at most [`MAX_LIFTS`] times.
/// A ragdoll whose bones all move slower than [`CALM_SPEED`] (game units per second)
/// and turn slower than [`CALM_SPIN`] (radians per second) for [`CALM_TIME`] seconds is
/// set at rest (`PhysWorld::set_ragdoll_at_rest`: held where it lies until a hit or a
/// blast): joints resting against their limits can keep a corpse creeping below what the
/// eye sees but above what the physics engine calls rest.
///
/// The speeds are the bones' net motion over windows of [`CALM_WINDOW`] seconds, not the
/// bodies' velocities: a limb pressed between the ground and a joint limit shivers, its
/// velocity flipping from step to step at up to several radians per second while it goes
/// nowhere, and read from the velocities such a corpse never came to rest
/// (docs/spec-notes/ragdoll.md §4 "Rest").
pub const CALM_SPEED: f32 = 12.0;
pub const CALM_SPIN: f32 = 0.6;
pub const CALM_TIME: f32 = 1.5;
pub const CALM_WINDOW: f32 = 0.25;

/// A ragdoll still moving this many seconds after it started (or was roused) is set at
/// rest where it is [engineering choice, a cost guard: two dozen mantis nymphs that died
/// at the player's feet lay in a heap that never calmed, and the contact solver took
/// 300 ms a frame (docs/design/frame-rate-audit.md §2)].
pub const MAX_AWAKE: f32 = 3.0;

/// [`MAX_AWAKE`] for a ragdoll in water (`fnv_phys::buoyancy`): long enough for a corpse
/// on a deep bed to rise to the surface and settle there [design: from 400 units down it
/// takes about 10 s].
pub const MAX_AWAKE_IN_WATER: f32 = 30.0;

/// [`MAX_AWAKE`] for a corpse at rest that a hit's shove roused (`ActorPush`): a shot's or
/// blow's shove moves one body 140-350 units a second, and it has fallen back within half
/// a second [design: owner-review item 51, decided 2026-11-08; in the 50-a-side battle
/// such re-wakes were a third of the seconds corpses moved, and with 3 s they cost the
/// solver as long as a fresh death]. A blast's rouse, a fresh death and a body in water
/// keep their own limits.
pub const MAX_AWAKE_SHOVED: f32 = 1.0;

pub const FELL_THROUGH: f32 = 150.0;
pub const LIFT_ABOVE: f32 = 20.0;
pub const MAX_LIFTS: u8 = 3;

/// The ragdoll data of skeletons, where corpses rest, and counts for the exit report.
#[derive(Resource, Default)]
pub struct Ragdolls {
    /// By skeleton path; `None` for a skeleton without a usable ragdoll.
    cache: HashMap<String, Option<Arc<Ragdoll>>>,
    /// Ragdoll Templates by path under `meshes\`; `None` for a path the data lacks.
    templates: HashMap<String, Option<Arc<RagdollTemplate>>>,
    /// Bone poses (game space, in the ragdoll's body order) of corpses by reference.
    pub rest: HashMap<u32, Vec<Option<Iso>>>,
    pub started: usize,
    pub restored: usize,
    pub rested: usize,
    pub lifted: usize,
    pub pushes: usize,
    /// Corpses given the pose their reference stores (`XRGD`, `deaths.rs`), and those
    /// whose stored pose did not fit their skeleton.
    pub authored: usize,
    pub authored_skipped: usize,
    /// Ragdolls set at rest by [`MAX_AWAKE`] (or [`MAX_AWAKE_SHOVED`]) while still moving.
    pub held: usize,
    /// Corpses at rest roused by a hit's shove ([`MAX_AWAKE_SHOVED`]).
    pub shoved_awake: usize,
    /// Corpses at rest that a hit shoved since [`pose`] last ran: they get
    /// [`MAX_AWAKE_SHOVED`] when it finds them roused.
    shoved_at_rest: HashSet<u32>,
    /// Ragdolls that came to rest in water (floating).
    pub floated: usize,
    /// `PhysWorld::steps` when [`pose`] last ran: the calm and awake timers count the
    /// physics time stepped since, not the frame's time (#1049).
    last_steps: Option<u64>,
    /// Per live ragdoll: its fastest bone's speed and spin over the last calm window, and the
    /// longest it has been calm ([`CALM_TIME`] sets it at rest), for the exit report of
    /// ragdolls that never came to rest.
    calm: HashMap<u32, Calm>,
}

#[derive(Clone, Default)]
struct Calm {
    speed: f32,
    spin: f32,
    longest: f32,
    /// The bodies (by bone name) that ended a calm spell by moving or turning faster than
    /// the limits: how often, and the fastest speed and spin they had then.
    breakers: HashMap<String, (u32, f32, f32)>,
}

impl Ragdolls {
    /// The ragdoll of the skeleton at `skeleton` (read once): at least two bodies and a
    /// joint. A file that breaks the reader (a panic, caught) has no ragdoll.
    fn data(&mut self, data: &GameData, skeleton: &str) -> Option<Arc<Ragdoll>> {
        let key = fnv_vfs::normalize(skeleton);
        self.cache
            .entry(key.clone())
            .or_insert_with(|| {
                let bytes = data.read(&key)?;
                let read = guarded(|| {
                    let file = NifFile::parse(&bytes, Some(&data.schema)).map_err(|e| e.to_string())?;
                    let dec = Decoded::new(&file, &bytes, &data.schema);
                    Ok(fnv_collide::ragdoll::ragdoll(&file, &dec))
                });
                let r = match read {
                    Ok(r) => r,
                    Err(e) => {
                        warn!("ragdoll: {key}: not read ({e}); no ragdoll for this skeleton");
                        return None;
                    }
                };
                let mass: f32 = r.bodies.iter().map(|b| b.object.mass).sum();
                info!("ragdoll: {key}: {} bodies ({mass:.0} mass), {} joints{}", r.bodies.len(), r.joints.len(), if r.problems.is_empty() { String::new() } else { format!(", problems: {}", r.problems.join("; ")) });
                (r.bodies.len() >= 2 && !r.joints.is_empty()).then(|| Arc::new(r))
            })
            .clone()
    }

    /// The Ragdoll Template at `path` (an ARMO's BMCT, relative to `meshes\`; read once):
    /// `None` when the data has no such file or it holds no template (ragdoll.md §6).
    fn template(&mut self, data: &GameData, path: &str) -> Option<Arc<RagdollTemplate>> {
        let rel = fnv_vfs::normalize(path);
        let key = if rel.starts_with("meshes\\") { rel } else { format!("meshes\\{rel}") };
        self.templates
            .entry(key.clone())
            .or_insert_with(|| {
                let Some(bytes) = data.read(&key) else {
                    info!("ragdoll template: {key}: not in the data; the skeleton's own bodies and joints");
                    return None;
                };
                let t = guarded(|| {
                    let file = NifFile::parse(&bytes, Some(&data.schema)).ok();
                    Ok(file.as_ref().and_then(|f| fnv_collide::ragdoll::template(f, &Decoded::new(f, &bytes, &data.schema))))
                })
                .unwrap_or_else(|e| {
                    warn!("ragdoll template: {key}: not read ({e})");
                    None
                });
                match &t {
                    Some(t) => info!("ragdoll template: {key}: {} bones{}", t.bones.len(), if t.problems.is_empty() { String::new() } else { format!(", problems: {}", t.problems.join("; ")) }),
                    None => warn!("ragdoll template: {key}: not a template file (no bhkRagdollTemplate); not used"),
                }
                t.map(Arc::new)
            })
            .clone()
    }

    /// The skeleton's ragdoll with the worn armour's Ragdoll Templates laid over it, in
    /// the order given (ragdoll.md §6; read once per skeleton and list of templates).
    fn data_with(&mut self, data: &GameData, skeleton: &str, templates: &[String]) -> Option<Arc<Ragdoll>> {
        let base = self.data(data, skeleton)?;
        let found: Vec<(String, Arc<RagdollTemplate>)> = templates.iter().filter_map(|p| Some((p.clone(), self.template(data, p)?))).collect();
        if found.is_empty() {
            return Some(base);
        }
        let key = format!("{}|{}", fnv_vfs::normalize(skeleton), found.iter().map(|(p, _)| fnv_vfs::normalize(p)).collect::<Vec<_>>().join("|"));
        self.cache
            .entry(key.clone())
            .or_insert_with(|| {
                let mut r = (*base).clone();
                for (path, t) in &found {
                    // A template that breaks the merge (a panic, caught) is left out; the
                    // ragdoll is taken back to what it was before it.
                    let before = r.clone();
                    let a = match guarded(|| Ok(r.apply_template(t))) {
                        Ok(a) => a,
                        Err(e) => {
                            warn!("ragdoll template: {path} on {}: not applied ({e})", fnv_vfs::normalize(skeleton));
                            r = before;
                            continue;
                        }
                    };
                    info!(
                        "ragdoll template: {path} on {}: {} bones, {} joints replaced, {} added{}",
                        fnv_vfs::normalize(skeleton),
                        a.bones,
                        a.joints_replaced,
                        a.joints_added,
                        if a.skipped.is_empty() { String::new() } else { format!("; not applied: {}", a.skipped.join("; ")) }
                    );
                }
                Some(Arc::new(r))
            })
            .clone()
    }

    /// The [`Corpse`] of an actor, when its skeleton has a ragdoll and a bone for each
    /// of its bodies; `templates` are the Ragdoll Templates of the armour it wears
    /// ([`worn_templates`]).
    pub fn corpse(&mut self, data: &GameData, rig: &ActorRig, templates: &[String]) -> Option<Corpse> {
        let rag = self.data_with(data, &rig.skeleton, templates)?;
        let bones: Option<Vec<usize>> = rag.bodies.iter().map(|b| b.object.node.as_ref().and_then(|n| rig.skel.by_name.get(n)).copied().filter(|&i| i > 0)).collect();
        let bones = bones?;
        let mut body_of = vec![None; rig.skel.parents.len()];
        for (b, &n) in bones.iter().enumerate() {
            body_of[n] = Some(b);
        }
        Some(Corpse { data: rag, bones, body_of, live: false, velocity: [0.0; 3], settled: false, lifts: 0, calm: 0.0, since: Vec::new(), window: 0.0, awake: 0.0, limit: MAX_AWAKE })
    }
}

/// The Ragdoll Templates (ARMO BMCT) of the armour in `worn`, in that order, each once.
pub fn worn_templates(world: Option<&ActorWorld>, worn: &[FormId]) -> Vec<String> {
    let Some(world) = world else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for t in worn.iter().filter_map(|a| world.actors.armors.get(a)?.ragdoll_template.clone()) {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&t)) {
            out.push(t);
        }
    }
    out
}

/// The player's third-person rig while it lies as a ragdoll (`death_cam.rs`): the rig of
/// reference [`fnv_sim::FormId::PLAYER_REF`], on the rig's root.
#[derive(Component, Clone)]
pub struct PlayerRig(pub ActorRig);

/// The rig of a corpse: a placed actor's or the player's.
type AnyRig = AnyOf<(&'static ActorRig, &'static PlayerRig)>;

fn rig_of<'a>((actor, player): (Option<&'a ActorRig>, Option<&'a PlayerRig>)) -> &'a ActorRig {
    actor.or(player.map(|p| &p.0)).expect("AnyOf holds one of them")
}

/// A dead actor with a ragdoll (on the actor's root, next to its [`ActorRig`]).
#[derive(Component)]
#[cfg_attr(test, derive(Default))]
pub struct Corpse {
    pub data: Arc<Ragdoll>,
    /// Per ragdoll body, its bone's index in the skeleton.
    bones: Vec<usize>,
    /// Per skeleton node, the ragdoll body on it.
    body_of: Vec<Option<usize>>,
    /// The ragdoll is in the physics world.
    pub live: bool,
    /// The velocity its bodies start with (game units a second, game space): a knockdown's
    /// shove (`knockouts.rs`); used once.
    pub velocity: [f32; 3],
    /// The bones hold the pose the ragdoll went to sleep in.
    settled: bool,
    lifts: u8,
    /// Seconds it has been nearly still.
    calm: f32,
    /// The bones' poses at the start of the current calm window ([`CALM_WINDOW`]), and
    /// the seconds since.
    since: Vec<Option<Iso>>,
    window: f32,
    /// Seconds awake since it started or was roused, and the most it may be awake
    /// ([`MAX_AWAKE`], [`MAX_AWAKE_SHOVED`]).
    awake: f32,
    limit: f32,
}

impl Corpse {
    /// Per ragdoll body, its bone's index in the skeleton (`crate::dismember` cuts by it).
    pub fn body_bones(&self) -> &[usize] {
        &self.bones
    }
}

pub struct RagdollPlugin;

impl Plugin for RagdollPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ragdolls>()
            .add_systems(Update, pose.after(nvf_play::PlaySet::Physics).run_if(resource_exists::<PhysState>))
            .add_systems(PostUpdate, start.after(bevy::transform::TransformSystems::Propagate).run_if(resource_exists::<PhysState>))
            .add_systems(Last, report);
    }
}

/// A bone's world transform (Bevy space, any uniform scale) as a game-space pose.
pub(crate) fn game_iso(a: &Affine3A) -> Iso {
    let col = |c: bevy::math::Vec3A| Vec3::from(space::to_game(Vec3::from(c))).normalize_or_zero();
    let (x, y, z) = (col(a.matrix3.x_axis), col(a.matrix3.y_axis), col(a.matrix3.z_axis));
    Iso { translation: space::to_game(Vec3::from(a.translation)), rotation: [x.x, y.x, z.x, x.y, y.y, z.y, x.z, y.z, z.z] }
}

/// A game-space pose with a uniform scale as a world transform (Bevy space).
pub(crate) fn bevy_affine(translation: [f32; 3], r: &[f32; 9], scale: f32) -> Affine3A {
    let col = |j: usize| space::to_bevy([r[j], r[3 + j], r[6 + j]]) * scale;
    Affine3A::from_cols(col(0).into(), col(1).into(), col(2).into(), space::to_bevy(translation).into())
}

fn iso_of(b: &BodyState) -> Iso {
    Iso { translation: b.translation, rotation: b.rotation }
}

/// The body part each ragdoll body reports when hit: the part whose bone chain holds the
/// body's bone or the nearest bone above it, else the torso's, else none.
fn body_parts(rig: &ActorRig, corpse: &Corpse, limbs: Option<&LimbRig>) -> Vec<u8> {
    let Some(limbs) = limbs else { return Vec::new() };
    let part_of = |node: usize| limbs.chains.iter().find(|(_, _, chain, _)| chain.contains(&rig.nodes[node])).map(|c| c.0);
    let torso = limbs.chains.iter().find(|c| c.1 == Limb::Torso).map(|c| c.0);
    corpse
        .bones
        .iter()
        .map(|&b| {
            let mut i = Some(b);
            while let Some(n) = i {
                if let Some(p) = part_of(n) {
                    return p;
                }
                i = rig.skel.parents.get(n).copied().flatten();
            }
            torso.unwrap_or(fnv_phys::actors::WHOLE)
        })
        .collect()
}

/// What [`start`] reads of a corpse.
type Waiting = (AnyRig, &'static mut Corpse, Option<&'static mut Dying>, Option<&'static LimbRig>, &'static InheritedVisibility);

/// Put waiting corpses' ragdolls in the world where the ground is loaded, and pass on
/// the frame's shoves.
fn start(
    mut phys: ResMut<PhysState>,
    mut ragdolls: ResMut<Ragdolls>,
    mut pushes: MessageReader<ActorPush>,
    mut corpses: Query<Waiting>,
    globals: Query<&GlobalTransform>,
) {
    for (rig, mut c, dying, limbs, vis) in &mut corpses {
        let rig = rig_of(rig);
        if c.live || !vis.get() || c.lifts > MAX_LIFTS {
            continue;
        }
        let Ok(root) = globals.get(rig.nodes[0]) else { continue };
        let scale = root.affine().matrix3.x_axis.length();
        let stored = ragdolls.rest.get(&rig.reference).filter(|s| s.len() == c.bones.len());
        let nodes: Option<Vec<Iso>> = match stored {
            Some(s) => s.iter().copied().collect(),
            None => c.bones.iter().map(|&b| globals.get(rig.nodes[b]).ok().map(|g| game_iso(&g.affine()))).collect(),
        };
        // A body is drawn its rest gap below its character's feet (`drawn_sink`), in the
        // ground's contact shell: its ragdoll starts at the character's height, clear of
        // that shell, or it never settles.
        let lift = if stored.is_some() { 0.0 } else { phys.drawn_sink_of(rig.reference) };
        let nodes = nodes.map(|n| n.into_iter().map(|mut i| { i.translation[2] += lift; i }).collect::<Vec<_>>());
        let Some(nodes) = nodes.filter(|n| !n.is_empty()) else { continue };
        if !phys.ready_at(nodes[0].translation) {
            continue;
        }
        let parts = body_parts(rig, &c, limbs);
        let asleep = stored.is_some();
        let velocity = std::mem::take(&mut c.velocity);
        let start = RagdollStart { form_id: rig.reference, ragdoll: &c.data, scale, nodes: &nodes, velocity, parts: &parts, asleep };
        // Bad ragdoll data that breaks the physics build (a panic, caught): this corpse
        // keeps its death pose, logged once (`lifts` past the limit stops the retries).
        let r = match guarded(|| Ok(phys.world.add_ragdoll(&start))) {
            Ok(r) => r,
            Err(e) => {
                warn!("ragdoll: actor {:08X}: not built ({e}); it keeps its death pose", rig.reference);
                phys.world.remove_ragdoll(rig.reference);
                c.lifts = MAX_LIFTS + 1;
                continue;
            }
        };
        debug!("ragdoll: actor {:08X}: {} bodies, {} joints, {} self-collision pairs{}{}", rig.reference, r.bodies, r.joints, r.self_pairs, if asleep { ", put back at rest" } else { "" }, if r.errors.is_empty() { String::new() } else { format!(", {} shape errors", r.errors.len()) });
        if r.bodies == 0 {
            phys.world.remove_ragdoll(rig.reference);
            c.lifts = MAX_LIFTS + 1;
            continue;
        }
        c.live = true;
        c.settled = false;
        c.calm = 0.0;
        c.since.clear();
        c.window = 0.0;
        c.awake = 0.0;
        c.limit = MAX_AWAKE;
        if asleep {
            ragdolls.restored += 1;
        } else {
            ragdolls.started += 1;
        }
        // The ragdoll has the pose from here on.
        if let Some(mut d) = dying {
            d.clip = None;
        }
    }
    for p in pushes.read() {
        let at_rest = phys.world.ragdoll_asleep(p.form_id);
        if phys.world.push_ragdoll(p.form_id, p.point, p.dv) {
            ragdolls.pushes += 1;
            if at_rest {
                ragdolls.shoved_at_rest.insert(p.form_id);
            }
        }
    }
}

/// Per body, how fast its bone moved and turned on average between the poses `a` and `b`
/// taken `seconds` apart: game units per second and radians per second (0 for a body
/// missing from either).
pub(crate) fn net_motion(a: &[Option<Iso>], b: &[Option<Iso>], seconds: f32) -> Vec<(f32, f32)> {
    let t = seconds.max(1e-6);
    a.iter()
        .zip(b)
        .map(|(a, b)| {
            let (Some(a), Some(b)) = (a, b) else { return (0.0, 0.0) };
            let d = (0..3).map(|i| (a.translation[i] - b.translation[i]).powi(2)).sum::<f32>().sqrt();
            // The angle of the turn from a to b: trace(aᵀ b) = 1 + 2 cos θ.
            let trace: f32 = a.rotation.iter().zip(&b.rotation).map(|(x, y)| x * y).sum();
            let angle = ((trace - 1.0) / 2.0).clamp(-1.0, 1.0).acos();
            (d / t, angle / t)
        })
        .collect()
}

/// Write the ragdolls' bodies to their bones, and take out of the world the ragdolls
/// whose ground or whose actor is gone.
pub(crate) fn pose(
    time: Res<Time>,
    mut phys: ResMut<PhysState>,
    mut ragdolls: ResMut<Ragdolls>,
    mut corpses: Query<(AnyRig, &mut Corpse, &InheritedVisibility)>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    // The physics time stepped since the last frame: none while a menu or a message box
    // holds the world. Counting the frame's time instead, a body killed just before a
    // box opened stood still under it, read as calm and was set at rest standing (#1049).
    let steps = phys.world.steps();
    let dt = ragdolls.last_steps.replace(steps).map_or(time.delta_secs(), |last| steps.saturating_sub(last) as f32 * phys.world.params.dt);
    let mut seen = HashSet::new();
    for (rig, mut c, vis) in &mut corpses {
        let rig = rig_of(rig);
        if !c.live {
            continue;
        }
        let id = rig.reference;
        if !vis.get() {
            // Disabled or deleted by a script (a raven cleans itself up): no body is left.
            phys.world.remove_ragdoll(id);
            ragdolls.rest.remove(&id);
            c.live = false;
            continue;
        }
        let Some(nodes) = phys.world.ragdoll_nodes(id) else {
            c.live = false;
            continue;
        };
        let Some(first) = nodes.iter().flatten().next().map(|b| b.translation) else { continue };
        let rest = || nodes.iter().map(|b| b.as_ref().map(iso_of)).collect::<Vec<_>>();
        if !phys.ready_at(first) {
            // The ground under it is gone: keep the pose for when it is back.
            ragdolls.rest.insert(id, rest());
            phys.world.remove_ragdoll(id);
            c.live = false;
            continue;
        }
        let lowest = nodes.iter().flatten().map(|b| b.translation[2]).fold(f32::INFINITY, f32::min);
        if let Some(t) = phys.terrain_z(first[0], first[1]).filter(|t| lowest < t - FELL_THROUGH) {
            warn!("ragdoll: actor {id:08X} fell through at ({:.0}, {:.0}, {lowest:.0}), terrain z {t:.0}: back above the ground", first[0], first[1]);
            // From where it last lay when that is known (its pose now may be torn), else
            // from its pose now; lifted so that its lowest bone is above the terrain.
            let from = ragdolls.rest.remove(&id).filter(|r| r.len() == nodes.len()).unwrap_or_else(rest);
            let low = from.iter().flatten().map(|i| i.translation[2]).fold(f32::INFINITY, f32::min);
            let at = from.iter().flatten().next().map_or(first, |i| i.translation);
            let up = phys.terrain_z(at[0], at[1]).unwrap_or(t) + LIFT_ABOVE - low;
            let lifted: Vec<Option<Iso>> = from.into_iter().map(|i| i.map(|mut i| { i.translation[2] += up; i })).collect();
            phys.world.remove_ragdoll(id);
            ragdolls.lifted += 1;
            c.lifts += 1;
            c.live = false;
            if c.lifts > MAX_LIFTS {
                continue;
            }
            // Start again from the lifted pose (awake): `start` reads the bones.
            let states: Vec<Option<BodyState>> = nodes.iter().zip(&lifted).map(|(b, l)| b.zip(*l).map(|(b, l)| BodyState { translation: l.translation, rotation: l.rotation, ..b })).collect();
            write_bones(rig, &c, &states, &globals, &mut transforms);
            continue;
        }
        seen.insert(id);
        let now = rest();
        if phys.world.ragdoll_held(id) {
            // Carried by the player's grab: never held at rest (its held body would stop
            // following the crosshair); calm and the awake guard count again from the drop.
            (c.calm, c.awake, c.window, c.settled, c.limit) = (0.0, 0.0, 0.0, false, MAX_AWAKE);
            c.since.clear();
            write_bones(rig, &c, &nodes, &globals, &mut transforms);
            continue;
        }
        if c.settled && !phys.world.ragdoll_asleep(id) {
            // Roused by a hit or a blast: calm from scratch; a hit's shove alone moves it
            // for less long.
            c.calm = 0.0;
            c.since.clear();
            c.awake = 0.0;
            c.limit = if ragdolls.shoved_at_rest.contains(&id) { MAX_AWAKE_SHOVED } else { MAX_AWAKE };
            if c.limit < MAX_AWAKE {
                ragdolls.shoved_awake += 1;
            }
        }
        c.awake += dt;
        c.window += dt;
        if c.since.len() != now.len() {
            (c.since, c.window) = (now, 0.0);
        } else if c.window >= CALM_WINDOW {
            let moved = net_motion(&c.since, &now, c.window);
            let (v, w) = moved.iter().fold((0.0f32, 0.0f32), |m, &(v, w)| (m.0.max(v), m.1.max(w)));
            let calm = v < CALM_SPEED && w < CALM_SPIN;
            let seen_calm = ragdolls.calm.entry(id).or_default();
            if !calm && c.calm > 0.0 {
                // Which bodies ended the calm spell, for the exit report.
                for (i, &(v, w)) in moved.iter().enumerate().filter(|(_, &(v, w))| v >= CALM_SPEED || w >= CALM_SPIN) {
                    let name = c.data.bodies.get(i).and_then(|b| b.object.node.clone()).unwrap_or_else(|| format!("body {i}"));
                    let e = seen_calm.breakers.entry(name).or_default();
                    *e = (e.0 + 1, e.1.max(v), e.2.max(w));
                }
            }
            c.calm = if calm { c.calm + c.window } else { 0.0 };
            (seen_calm.speed, seen_calm.spin) = (v, w);
            seen_calm.longest = seen_calm.longest.max(c.calm);
            (c.since, c.window) = (now, 0.0);
        }
        if c.calm >= CALM_TIME || phys.world.ragdoll_asleep(id) {
            phys.world.set_ragdoll_at_rest(id);
        } else if c.awake >= c.limit && (c.awake >= MAX_AWAKE_IN_WATER || !phys.world.ragdoll_in_water(id)) {
            phys.world.set_ragdoll_at_rest(id);
            ragdolls.held += 1;
            info!("ragdoll: actor {id:08X} held at rest still moving after {:.0} s (calm for {:.2} s)", c.awake, c.calm);
        }
        let asleep = phys.world.ragdoll_asleep(id);
        if asleep && c.settled {
            continue;
        }
        write_bones(rig, &c, &nodes, &globals, &mut transforms);
        if asleep {
            ragdolls.rested += 1;
            ragdolls.rest.insert(id, rest());
            // How high its trunk lies over its lowest bone: a corpse lying down is low, one
            // set at rest standing (#1049) about a body's height.
            let trunk = phys.world.ragdoll_center(id).unwrap_or(first);
            info!("ragdoll: actor {id:08X} at rest at ({:.0}, {:.0}, {:.0}), trunk {:.0} units over its lowest bone, after {:.1} s of physics", first[0], first[1], first[2], trunk[2] - lowest, c.awake);
            if phys.world.ragdoll_in_water(id) {
                ragdolls.floated += 1;
                let at = phys.world.ragdoll_center(id).unwrap_or(first);
                let surface = phys.world.water_surface_for(at).unwrap_or(f32::NAN);
                info!("ragdoll: actor {id:08X} floats at ({:.0}, {:.0}, {:.0}), its trunk {:.0} units under the surface at {surface:.0}, {:.1} s after it started moving", at[0], at[1], at[2], surface - at[2], c.awake);
            }
        }
        c.settled = asleep;
    }
    // Ragdolls whose actor is no longer drawn: remember where they lie.
    let gone: Vec<u32> = phys.world.ragdolls().filter(|&id| !seen.contains(&id) && !phys.world.is_object_ragdoll(id)).collect();
    for id in gone {
        if let Some(nodes) = phys.world.ragdoll_nodes(id) {
            ragdolls.rest.insert(id, nodes.iter().map(|b| b.as_ref().map(iso_of)).collect());
        }
        phys.world.remove_ragdoll(id);
    }
    ragdolls.shoved_at_rest.clear();
}

/// Set the local transforms of the bones that have a body so that they stand where their
/// bodies are. Nodes are in parent-first order; a node without a body keeps its local
/// transform under its parent.
fn write_bones(rig: &ActorRig, c: &Corpse, bodies: &[Option<BodyState>], globals: &Query<&GlobalTransform>, transforms: &mut Query<&mut Transform>) {
    let Ok(root) = globals.get(rig.nodes[0]).map(|g| g.affine()) else { return };
    let scale = root.matrix3.x_axis.length();
    let n = rig.nodes.len().min(rig.skel.parents.len());
    let mut global: Vec<Affine3A> = Vec::with_capacity(n);
    global.push(root);
    for i in 1..n {
        let parent = global[rig.skel.parents[i].filter(|&p| p < i).unwrap_or(0)];
        let body = c.body_of[i].and_then(|b| bodies.get(b).copied().flatten());
        let g = match body {
            Some(b) => {
                // The bone's own scale in the bind pose stays on it.
                let own = rig.skel.bind_world.get(i).map_or(1.0, |w| w.matrix3.x_axis.length());
                let g = bevy_affine(b.translation, &b.rotation, scale * own);
                if let Ok(mut t) = transforms.get_mut(rig.nodes[i]) {
                    let (_, rotation, translation) = (parent.inverse() * g).to_scale_rotation_translation();
                    if rotation.is_finite() && translation.is_finite() {
                        t.translation = translation;
                        t.rotation = rotation.normalize();
                    }
                }
                g
            }
            None => parent * transforms.get(rig.nodes[i]).map_or(Affine3A::IDENTITY, |t| t.compute_affine()),
        };
        global.push(g);
    }
}

fn report(mut exit: MessageReader<AppExit>, ragdolls: Res<Ragdolls>, phys: Option<Res<PhysState>>) {
    if exit.read().last().is_none() || ragdolls.started + ragdolls.restored == 0 {
        return;
    }
    let (live, asleep) = phys.as_deref().map_or((0, 0), |p| (p.world.ragdolls().count(), p.world.ragdolls().filter(|&id| p.world.ragdoll_asleep(id)).count()));
    info!(
        "ragdolls: {} started, {} put back at rest, {} came to rest ({} held at rest still moving after {MAX_AWAKE} s or {MAX_AWAKE_SHOVED} s, {} floating in water), {} shoved by hits ({} roused from rest), {} lifted back above the terrain; {live} in the world at exit ({asleep} at rest); {} laid in the pose their reference stores ({} stored poses did not fit)",
        ragdolls.started, ragdolls.restored, ragdolls.rested, ragdolls.held, ragdolls.floated, ragdolls.pushes, ragdolls.shoved_awake, ragdolls.lifted, ragdolls.authored, ragdolls.authored_skipped
    );
    // Those still moving: how fast, and how near they came to resting.
    if let Some(p) = phys.as_deref() {
        for id in p.world.ragdolls().filter(|&id| !p.world.ragdoll_asleep(id)) {
            let c = ragdolls.calm.get(&id).cloned().unwrap_or_default();
            let at = p.world.ragdoll_center(id).map_or_else(|| "unknown".to_string(), |a| format!("({:.0}, {:.0}, {:.0})", a[0], a[1], a[2]));
            info!(
                "ragdoll: actor {id:08X} not at rest at exit: its fastest bone moved {:.1} units/s (calm below {CALM_SPEED}) and turned {:.2} rad/s (below {CALM_SPIN}) over the last {CALM_WINDOW} s; calm for {:.2} s at most ({CALM_TIME} s sets it at rest); trunk at {at}",
                c.speed, c.spin, c.longest
            );
            let mut breakers: Vec<_> = c.breakers.iter().collect();
            breakers.sort_by_key(|b| std::cmp::Reverse(b.1 .0));
            let list: Vec<String> = breakers.iter().take(6).map(|(n, (k, v, w))| format!("{n} {k}x (up to {v:.1} units/s, {w:.2} rad/s)")).collect();
            info!("ragdoll: actor {id:08X}: calm spells ended by {}", if list.is_empty() { "nothing (never calm)".to_string() } else { list.join(", ") });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poses_survive_the_trip_to_bevy_space_and_back() {
        // 90° about z and 30° about x, at a point, on an actor scaled 1.1.
        let (s, c) = 30f32.to_radians().sin_cos();
        let r = [0.0, -c, s, 1.0, 0.0, 0.0, 0.0, s, c];
        let t = [120.0, -40.0, 66.0];
        let a = bevy_affine(t, &r, 1.1);
        // Game z is Bevy y; the scale is on the axes.
        assert!((a.translation.y - 66.0).abs() < 1e-4 && (a.matrix3.x_axis.length() - 1.1).abs() < 1e-5);
        let back = game_iso(&a);
        assert!(back.translation.iter().zip(&t).all(|(a, b)| (a - b).abs() < 1e-3), "{:?}", back.translation);
        assert!(back.rotation.iter().zip(&r).all(|(a, b)| (a - b).abs() < 1e-5), "{:?}", back.rotation);
    }
}
