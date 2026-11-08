//! Reading and building a physics cell away from the world: the plugin records (cell,
//! placements, LAND), the models' collision (archive read, NIF parse, collision blocks
//! to `fnv_collide` objects) and the rapier shapes. All of it is pure, so it runs on the
//! loader thread ([`super::loader`]) as well as inline; only inserting the result
//! ([`super::PhysState::load`], `stream`) touches the world.
//!
//! Whether a reference is in the world is not decided here: the simulation can change
//! that while a cell is being prepared, so every reference is prepared and the check is
//! made when the cell is inserted.

use super::cells::PhysCell;
use super::drawn::DrawnFloors;
use super::LoadTimes;
use crate::PlayData;
use fnv_collide::anim::ModelAnim;
use fnv_collide::nif::NodeTree;
use fnv_collide::{CollisionObject, Iso};
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_phys::{ref_collision_with, ModelCollision, PhysParams, PreparedRef, PreparedTerrain, RefCollisionOptions, RefPose};
use fnv_world::land::{load_land, LandParts};
use fnv_world::placed::{base_object, placement};
use fnv_world::Placement;
use nvf_assets::GameData;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

pub(super) fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

type Objects = Arc<Vec<CollisionObject>>;

/// What a model contributes: its solid collision objects, its water volumes (phantoms
/// on the Water layer, physics.md §6.6), its animation when its sequences move a node
/// of a keyframed collision object or it is a door model with "Open" and "Close"
/// sequences (`crate::doors`, [`AnimatedBodies`]), and for a model with collision the floors
/// of its drawn geometry ([`DrawnFloors`]) and its node tree (to pose the objects,
/// [`pose_stored`]), and its trigger volumes (phantoms on the Trigger layer, the traps'
/// pressure boxes, `crate::script_trigger`); or why it could not be read.
type ModelResult = Result<(Objects, Objects, Option<Arc<ModelAnim>>, Option<Arc<DrawnFloors>>, Option<Arc<NodeTree>>, Objects), String>;

/// Collision per model path (lower case), shared by the loader thread and the main
/// thread. A model is read once: a second caller waits for the first.
#[derive(Default)]
pub struct ModelCache {
    map: Mutex<HashMap<String, Arc<OnceLock<ModelResult>>>>,
}

impl ModelCache {
    fn slot(&self, path: &str) -> Arc<OnceLock<ModelResult>> {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).entry(path.to_ascii_lowercase()).or_default().clone()
    }

    fn read(&self, data: &GameData, path: &str, times: &mut LoadTimes) -> ModelResult {
        self.slot(path)
            .get_or_init(|| {
                let r = read_model(data, path, times);
                if let Err(e) = &r {
                    bevy::log::debug!("collision of {path}: {e}");
                }
                r
            })
            .clone()
    }

    /// A model's solid collision objects (empty when it has none), read through
    /// `fnv_collide::nif` once.
    pub fn objects(&self, data: &GameData, path: &str, times: &mut LoadTimes) -> Result<Objects, String> {
        self.read(data, path, times).map(|(solid, ..)| solid)
    }

    /// The floors of a model's drawn geometry, when it was read (by
    /// [`ModelCache::objects`]) and has collision and floors.
    pub fn drawn(&self, path: &str) -> Option<Arc<DrawnFloors>> {
        match self.slot(path).get() {
            Some(Ok((_, _, _, drawn, ..))) => drawn.clone(),
            _ => None,
        }
    }

    /// A door model's animation (read through the cache), when it has "Open" and
    /// "Close" sequences that move its nodes.
    pub fn door_anim(&self, data: &GameData, path: &str, times: &mut LoadTimes) -> Option<Arc<ModelAnim>> {
        self.anim(data, path, times).filter(|a| a.door().is_some())
    }

    /// A model's animation (read through the cache), when its sequences move a node of a
    /// keyframed collision object (or it is a door's).
    pub fn anim(&self, data: &GameData, path: &str, times: &mut LoadTimes) -> Option<Arc<ModelAnim>> {
        self.read(data, path, times).ok().and_then(|(_, _, anim, ..)| anim)
    }

    /// The node tree of a model with collision (read through the cache).
    pub fn tree(&self, data: &GameData, path: &str, times: &mut LoadTimes) -> Option<Arc<NodeTree>> {
        self.read(data, path, times).ok().and_then(|(_, _, _, _, tree, _)| tree)
    }

    /// A model's trigger volumes (phantoms on the Trigger layer), when it was read
    /// already (by the cell loader); `None` while it was not.
    pub fn triggers(&self, path: &str) -> Option<Objects> {
        match self.slot(path).get() {
            Some(Ok((.., triggers))) => Some(triggers.clone()),
            Some(Err(_)) => Some(Arc::default()),
            None => None,
        }
    }
}

/// Collision objects read from the archives through the shared cache.
struct Models<'a> {
    data: &'a GameData,
    cache: &'a ModelCache,
    times: &'a mut LoadTimes,
}

impl ModelCollision for Models<'_> {
    fn objects(&mut self, path: &str) -> Result<Objects, String> {
        self.cache.objects(self.data, path, self.times)
    }

    fn water(&mut self, path: &str) -> Objects {
        match self.cache.slot(path).get() {
            Some(Ok((_, water, ..))) => water.clone(),
            _ => Arc::default(),
        }
    }
}

/// A panic while building a model's collision is that model's failure (no collision),
/// not the game's (`nvf_assets::guard`).
fn read_model(data: &GameData, path: &str, times: &mut LoadTimes) -> ModelResult {
    nvf_assets::guard::guarded(|| read_model_unguarded(data, path, times))
}

fn read_model_unguarded(data: &GameData, path: &str, times: &mut LoadTimes) -> ModelResult {
    let schema = &*data.schema;
    let t = Instant::now();
    let read = data.vfs.read(path);
    times.read += ms_since(t);
    let bytes = match read {
        None => return Err("not found".to_string()),
        Some(Err(e)) => return Err(e.to_string()),
        Some(Ok(bytes)) => bytes,
    };
    let t = Instant::now();
    let file = NifFile::parse(&bytes, Some(schema)).map_err(|e| e.to_string());
    let dec = file.as_ref().ok().map(|file| Decoded::new(file, &bytes, schema));
    times.parse += ms_since(t);
    let file = file?;
    let t = Instant::now();
    let (solid, phantoms, _) = fnv_collide::nif::collision_and_phantoms(&file, dec.as_ref().expect("decoded with the file"));
    let (water, rest): (Vec<CollisionObject>, Vec<CollisionObject>) = phantoms.into_iter().partition(|o| o.layer == fnv_phys::layers::WATER);
    let triggers: Vec<CollisionObject> = rest.into_iter().filter(|o| o.layer == fnv_phys::layers::TRIGGER).collect();
    let dec = dec.expect("decoded with the file");
    // Only a model with embedded sequences and a body physics does not move itself can
    // have collision that follows an animation.
    let sequenced = file.blocks.iter().any(|b| b.type_name == "NiControllerSequence");
    let anim = (sequenced && solid.iter().any(|o| o.motion != fnv_collide::Motion::Dynamic)).then(|| ModelAnim::new(&file, &dec));
    let anim = anim.filter(|a| a.door().is_some() || solid.iter().any(|o| moved_by(a, o))).map(Arc::new);
    // Only what can be stood on needs its drawn floors.
    let drawn = if solid.is_empty() { None } else { DrawnFloors::from_nif(&file, &dec).map(Arc::new) };
    let tree = (!solid.is_empty()).then(|| Arc::new(NodeTree::new(&file, &dec)));
    times.convert += ms_since(t);
    Ok((Arc::new(solid), Arc::new(water), anim, drawn, tree, Arc::new(triggers)))
}

/// What preparing needs: the plugins, the archives and the world's parameters. Cheap to
/// clone (references and `Arc`s).
#[derive(Clone)]
pub struct Source {
    pub d: PlayData,
    /// Without archives only terrain and collision primitives load.
    pub data: Option<GameData>,
    pub models: Arc<ModelCache>,
    pub params: PhysParams,
}

/// One cell to prepare.
#[derive(Debug, Clone)]
pub struct Job {
    pub cell: PhysCell,
    /// The cell's references and, outdoors, the worldspace's persistent references
    /// standing in the square: sorted by FormID, no duplicates.
    pub refs: Vec<u32>,
    /// The cell's LAND record (exterior squares with terrain).
    pub land: Option<u32>,
    /// References the simulation holds at another pose than their placement (position,
    /// Euler angles; `SimState::moved_pose`), by FormID: their collision is built there.
    pub poses: Vec<(u32, [f32; 3], [f32; 3])>,
    /// References a script scaled (SetScale; `SimState::script_scale`), by FormID: their
    /// collision is built at that scale, none below `crate::sim::MIN_DRAWN_SCALE`.
    pub scales: Vec<(u32, f32)>,
    /// Destroyed references' destruction models (`PhysState::destruction_models`), by
    /// FormID: their collision is built from it.
    pub models: Vec<(u32, String)>,
}

impl Job {
    /// The simulation's pose of `r`, when it overrides the placement.
    pub(crate) fn pose_of(&self, r: u32) -> Option<([f32; 3], [f32; 3])> {
        self.poses.binary_search_by_key(&r, |p| p.0).ok().map(|i| (self.poses[i].1, self.poses[i].2))
    }

    /// The scale a script set on `r`, when it overrides the placement's.
    pub(crate) fn scale_of(&self, r: u32) -> Option<f32> {
        self.scales.binary_search_by_key(&r, |p| p.0).ok().map(|i| self.scales[i].1)
    }

    /// The destruction model `r` collides as, when it has one.
    pub(crate) fn model_of(&self, r: u32) -> Option<&str> {
        self.models.binary_search_by_key(&r, |p| p.0).ok().map(|i| self.models[i].1.as_str())
    }
}

/// A placement at the simulation's pose `pose` (position, Euler angles), when given.
pub fn at_pose(mut pl: Placement, pose: Option<([f32; 3], [f32; 3])>) -> Placement {
    if let Some((position, rotation)) = pose {
        pl.position = position;
        pl.rotation = rotation;
    }
    pl
}

/// A reference's collision, prepared.
pub struct RefShapes {
    /// The `refs.counts` outcomes `ref_collision` reported, in order.
    pub tallies: Vec<&'static str>,
    /// Water volumes of the model in the reference's frame (`RefCollision::water`).
    pub water: Vec<CollisionObject>,
    /// The shapes, when the reference has collision objects.
    pub prepared: Option<PreparedRef>,
    /// The collision object's node and body transforms, when the model has exactly one
    /// object and it is dynamic: physics can move the reference, and the body's pose
    /// gives the reference's (`crate::dropped::ref_pose_from_body`).
    pub dynamic_local: Option<fnv_collide::Iso>,
    /// A door that opens in place: its keyframed bodies follow its animation.
    pub door: Option<DoorBodies>,
    /// Any other reference whose model's sequences move its keyframed bodies (a gate, a
    /// lift, a drawbridge built as an activator): they follow the group its drawn model
    /// plays (`super::pose_groups`; issue #1396).
    pub animated: Option<AnimatedBodies>,
    /// The floors of the model's drawn geometry, for a reference whose collision stays
    /// where it was placed (not dynamic, not a door that opens in place).
    pub drawn: Option<Arc<DrawnFloors>>,
    /// The destruction model the collision was built from (`Job::models`).
    pub model: Option<String>,
}

/// The keyframed bodies of a door that opens in place (`crate::doors`), posed from its
/// model's "Open" and "Close" sequences.
#[derive(Clone)]
pub struct DoorBodies {
    pub anim: Arc<ModelAnim>,
    /// (object index in the reference's collision objects, node block, body transform)
    /// of each keyframed object.
    pub objects: Vec<(u16, usize, fnv_collide::Iso)>,
}

/// The keyframed bodies of a reference that is not a door opening in place, posed from
/// whichever of its model's sequences plays on it (`PlayGroup`, an idle, an activator's
/// Open and Close): the same shape as [`DoorBodies`], only the bodies a sequence moves.
pub type AnimatedBodies = DoorBodies;

/// Whether some sequence of `anim` moves the node of object `o`, which physics does not
/// move itself (keyframed or fixed: a dynamic body falls and is pushed on its own).
fn moved_by(anim: &ModelAnim, o: &CollisionObject) -> bool {
    o.motion != fnv_collide::Motion::Dynamic && anim.moves_block(o.node_block)
}

/// The model of a DOOR reference without XTEL (a door that opens in place).
fn in_place_door_model(src: &Source, pl: &Placement) -> Option<String> {
    if pl.teleport {
        return None;
    }
    let base = base_object(src.d.lo, src.d.schema, pl.base?)?;
    (&base.sig == b"DOOR").then(|| base.model_path()).flatten()
}

/// A reference of a prepared cell.
pub struct Entry {
    pub form_id: u32,
    /// `None` when its record could not be read.
    pub placement: Option<Placement>,
    pub shapes: Option<RefShapes>,
}

/// A cell ready to insert.
pub struct Prepared {
    pub cell: PhysCell,
    pub heights: Option<Vec<f32>>,
    pub terrain: Option<PreparedTerrain>,
    /// The job's references, in its (FormID) order.
    pub refs: Vec<Entry>,
    /// Time spent preparing, per part (on whichever thread prepared it).
    pub times: LoadTimes,
}

/// Prepare a cell: terrain, then every reference's collision (in the world or not).
pub fn cell(src: &Source, job: &Job) -> Prepared {
    let mut times = LoadTimes::default();
    let d = &src.d;
    let mut out =
        Prepared { cell: job.cell, heights: None, terrain: None, refs: Vec::with_capacity(job.refs.len()), times: LoadTimes::default() };
    if let (PhysCell::Exterior(x, y), Some(land)) = (job.cell, job.land) {
        let t = Instant::now();
        out.heights = load_land(d.lo, d.schema, land, LandParts::HEIGHTS).and_then(|l| l.heights);
        times.records += ms_since(t);
        if let Some(h) = &out.heights {
            let t = Instant::now();
            out.terrain = Some(PreparedTerrain::new(&src.params, (x, y), h));
            times.shapes += ms_since(t);
        }
    }
    for &r in &job.refs {
        let t = Instant::now();
        let mut pl = placement(d.lo, d.schema, r).map(|pl| at_pose(pl, job.pose_of(r)));
        if let Some(s) = job.scale_of(r) {
            pl = pl.filter(|_| s > crate::sim::MIN_DRAWN_SCALE).map(|mut pl| {
                pl.scale = s;
                pl
            });
        }
        times.records += ms_since(t);
        let shapes = pl.as_ref().map(|pl| reference(src, pl, job.model_of(r), &mut times));
        out.refs.push(Entry { form_id: r, placement: pl, shapes });
    }
    out.times = times;
    out
}

/// A placed reference with more than one dynamic collision object (a hanging lamp and its
/// chain, a globe on its stand, a bucket and its handle: the parts the model's constraints
/// join) keeps them keyframed where it was placed (physics.md §4.3.1): the constraints are
/// not modelled, so the parts fell apart, and the reference is drawn where it was placed
/// whatever its bodies do. Tallied as `jointed_held`. A destroyed car's hulk
/// (`vehicles\carhulk01.nif`: doors, hood, trunk, chassis, no constraints) is held so
/// too, until the drawing lets its parts fall (nvforever `wrecks.rs`).
fn hold_jointed(objects: &mut [fnv_collide::CollisionObject], tallies: &mut Vec<&'static str>) {
    use fnv_collide::Motion;
    if objects.iter().filter(|o| o.motion == Motion::Dynamic).count() < 2 {
        return;
    }
    for o in objects.iter_mut().filter(|o| o.motion == Motion::Dynamic) {
        o.motion = Motion::Keyframed;
    }
    tallies.push("jointed_held");
}

/// A jointed clutter object its designer left posed (`XRGD`, a fallen skeleton: dead-on-spawn.md
/// §5.1): its collision objects put where the drawing puts their nodes
/// (`nvforever::clutter_pose`), entry k on object k's node under its parent, the other nodes
/// as in the file. The same count rule as the drawing (`fnv_world::xrgd::matching`, so a
/// skeleton part copied with a whole skeleton's `XRGD` poses from the first entries).
/// `false` (objects untouched) when the entries do not fit.
pub(super) fn pose_stored(tree: &NodeTree, bones: &[fnv_world::xrgd::Bone], objects: &mut [CollisionObject]) -> bool {
    let Ok(n) = fnv_world::xrgd::matching(bones.len(), objects.len()) else { return false };
    let bones = &bones[..n];
    if !bones.iter().all(|b| b.position.iter().chain(&b.angles).all(|v| v.is_finite())) {
        return false;
    }
    let locals: Vec<Iso> = bones.iter().map(|b| Iso { translation: b.position, rotation: b.rotation() }).collect();
    tree.pose_objects(objects, &locals)
}

/// A placed static (base STAT, or a static collection SCOL) never moves, whatever its
/// model's Havok data says: its dynamic bodies are made fixed (physics.md §4.3.2;
/// playtest list 5 item 59). Tallied as `static_base_fixed`.
///
/// [observed: the three motorcycles outside the Goodsprings saloon are STAT
/// `Motorcycle01Static` references whose model, `vehicles\motorcycle01.nif`, has one dynamic
/// body (Props layer, 600 kg), the same model as the MSTT `Motorcycle01`; the owner reports
/// they do not move in the game. 1,131 STAT and 19 SCOL references placed in FalloutNV.esm use a
/// model with dynamic bodies, most of them `1stPerson…` weapon statics; hypothesis: the
/// game simulates Havok only for the moveable record types]
pub(super) fn fix_static_base(base_sig: Option<[u8; 4]>, objects: &mut [fnv_collide::CollisionObject], tallies: &mut Vec<&'static str>) {
    use fnv_collide::Motion;
    if !matches!(base_sig.as_ref(), Some(b"STAT" | b"SCOL")) || !objects.iter().any(|o| o.motion == Motion::Dynamic) {
        return;
    }
    for o in objects.iter_mut().filter(|o| o.motion == Motion::Dynamic) {
        o.motion = Motion::Static;
    }
    tallies.push("static_base_fixed");
}

/// Prepare one placed reference's collision (`fnv_phys::ref_collision`, the rule
/// `fnvtool phys cell` uses), its dynamic bodies asleep, a static's fixed
/// ([`fix_static_base`]), a posed one's put in its stored pose ([`pose_stored`]) and a
/// jointed object's held ([`hold_jointed`]). A panic while preparing it (bad data met by
/// a bug of ours, `nvf_assets::guard`) leaves that reference without collision, logged.
pub fn reference(src: &Source, pl: &Placement, model: Option<&str>, times: &mut LoadTimes) -> RefShapes {
    nvf_assets::guard::guarded(|| Ok(reference_unguarded(src, pl, model, times))).unwrap_or_else(|e| {
        bevy::log::warn!("phys: 0x{:08X}: collision not prepared ({e}); the reference has none", pl.form_id);
        RefShapes { tallies: vec!["prepare_failed"], water: Vec::new(), prepared: None, dynamic_local: None, door: None, animated: None, drawn: None, model: None }
    })
}

fn reference_unguarded(src: &Source, pl: &Placement, model: Option<&str>, times: &mut LoadTimes) -> RefShapes {
    let t0 = Instant::now();
    let before = *times;
    let mut tallies = Vec::new();
    // A door with an animation keeps its moving parts even when it starts open: they are
    // posed from the animation (`fnv_phys::RefCollisionOptions::posed_doors`).
    let anim = match (&src.data, in_place_door_model(src, pl)) {
        (Some(data), Some(path)) => src.models.door_anim(data, &path, times),
        _ => None,
    };
    let in_place_door = anim.is_some();
    let options = RefCollisionOptions { posed_doors: in_place_door, model };
    let mut models = src.data.as_ref().map(|data| Models { data, cache: &src.models, times: &mut *times });
    let rc = ref_collision_with(
        src.d.lo,
        src.d.schema,
        pl,
        models.as_mut().map(|m| m as &mut dyn ModelCollision),
        &mut |what| tallies.push(what),
        options,
    );
    let in_models = (times.read - before.read) + (times.parse - before.parse) + (times.convert - before.convert);
    times.records += ms_since(t0) - in_models;
    let mut rc = rc;
    let model_path = match model {
        Some(m) => fnv_world::placed::archive_path(m),
        None => pl.base.and_then(|b| base_object(src.d.lo, src.d.schema, b)).and_then(|b| b.world_model_path()),
    };
    fix_static_base(rc.base_sig, &mut rc.objects, &mut tallies);
    let posed = match (&src.data, rc.base_sig, &model_path) {
        (Some(data), Some(sig), Some(path)) if fnv_world::xrgd::posable(&sig) && !rc.objects.is_empty() => fnv_world::xrgd::stored(src.d.lo, pl.form_id)
            .zip(src.models.tree(data, path, times))
            .is_some_and(|(bones, tree)| pose_stored(&tree, &bones, &mut rc.objects)),
        _ => false,
    };
    if posed {
        tallies.push("xrgd_posed");
        bevy::log::info!("phys: 0x{:08X} {}: collision posed from XRGD ({} bodies)", pl.form_id, model_path.as_deref().unwrap_or(""), rc.objects.len());
    }
    hold_jointed(&mut rc.objects, &mut tallies);
    // Not a door that opens in place, nor an actor: its bodies that a sequence moves
    // follow what its drawn model plays (issue #1396), keyframed (a fixed one under a
    // moving node is made keyframed: it would stay where the file stores it). Read from
    // the model it collides with (a destruction model's own, when it has one).
    let animated = match (&src.data, &model_path) {
        (Some(data), Some(path)) if !in_place_door && !posed && !rc.base_sig.is_some_and(|s| fnv_phys::placed::is_actor_base(&s)) => {
            src.models.anim(data, path, times).and_then(|anim| {
                let objects: Vec<_> = rc
                    .objects
                    .iter_mut()
                    .enumerate()
                    .filter(|(_, o)| moved_by(&anim, o))
                    .map(|(i, o)| {
                        o.motion = fnv_collide::Motion::Keyframed;
                        (i as u16, o.node_block, o.body_transform)
                    })
                    .collect();
                (!objects.is_empty()).then_some(AnimatedBodies { anim, objects })
            })
        }
        _ => None,
    };
    if let Some(a) = &animated {
        tallies.push("animated_collision");
        bevy::log::debug!("phys: 0x{:08X} {}: {} collision bodies follow its sequences", pl.form_id, model_path.as_deref().unwrap_or(""), a.objects.len());
    }
    // Clutter starts asleep (physics.md §4.3.1).
    let prepared = (!rc.objects.is_empty()).then(|| {
        let t = Instant::now();
        let p = PreparedRef::new(&src.params, &RefPose::from_placement(pl), &rc.objects).asleep();
        times.shapes += ms_since(t);
        p
    });
    let dynamic_local = match rc.objects.as_slice() {
        [o] if o.motion == fnv_collide::Motion::Dynamic => Some(o.node_transform.compose(&o.body_transform)),
        _ => None,
    };
    let door = anim.map(|anim| DoorBodies {
        anim,
        objects: rc
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.motion == fnv_collide::Motion::Keyframed)
            .map(|(i, o)| (i as u16, o.node_block, o.body_transform))
            .collect(),
    });
    // A posed object's drawn floors would be the file's unposed geometry; so would an
    // animated one's once it moves.
    let fixed = prepared.is_some() && dynamic_local.is_none() && door.is_none() && animated.is_none() && !posed;
    let drawn = match (&src.data, model_path) {
        (Some(_), Some(path)) if fixed => src.models.drawn(&path),
        _ => None,
    };
    RefShapes { tallies, water: rc.water, prepared, dynamic_local, door, animated, drawn, model: model.map(str::to_string) }
}
