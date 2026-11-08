//! Actors assembled from parts into one entity (workstream 5, docs/spec-notes/actors.md
//! §1.2 and §3): one skeleton hierarchy per actor; skinned part shapes bind to the
//! skeleton's bones by name; rigid parts hang from their attachment bone.
//!
//! - Skeletons and parts are prepared once per archive path ([`ActorParts`]): meshes
//!   (with joint attributes for skinned shapes), materials, inverse bind poses, the
//!   attachment bone, and a CPU copy of each mesh (for per-actor variants such as
//!   FaceGen morphs).
//! - A rigid part on the head (`Bip01 Head`) has its vertices in the skeleton's axes with
//!   the origin at the bone [observed, actors.md §1.2]: a part whose root node stores the
//!   inverse of the bone's bind rotation (eyes, mouth, teeth) is an ordinary child of the
//!   bone; any other head part (hair, beards, hats, whose node rotations are the identity
//!   or cancel) gets that inverse rotation from the attachment. A rigid part on any other
//!   bone (the Pip-Boy models on `Bip01 L ForeTwist`) is in the bone's own axes: an
//!   ordinary child. Node transforms are kept in every case.
//! - Dismemberment caps are hidden: `BSDismemberSkinInstance` partitions that clear part
//!   flag bit 0 [documented: nif.xml BSPartFlag bit 0 "PF_EDITOR_VISIBLE"; observed: the
//!   cap shapes of `upperbody.nif` (`bodycaps`, `limbcaps`, `meatneck01`, `meathead01`)
//!   all have it clear, its body shapes set it; hypothesis: an intact actor shows no cap].
//!   A shape all of caps is a cap shape ([`PartShape::cap`]); a cap partition in a shape
//!   with drawn ones (the radscorpion's `meatcapbody`) is hidden by itself
//!   ([`Partition::cap`], dismemberment.md §2.5).
//! - Pose: the skeleton's bind pose, frozen (owner decision 2026-10-08), by default; a
//!   KF clips can play on the skeleton's bones instead ([`blend_player`], actors.md §7–8).
//! - Creatures (actors.md §10.2) use the same assembly with one difference for unskinned
//!   shapes ([`PartUse::by_ancestor`]): such a shape hangs from its nearest ancestor in
//!   the part file whose name is a node of the skeleton, keeping the transforms below
//!   that ancestor; else from the part's `Prn`, as an ordinary child on every bone (the
//!   head rule above is for NPC head parts: under it a dog's eyes sit on one side of its
//!   head); else from the skeleton's root.

use crate::anim_play::{BlendPlayer, BoundClip, Clip};
use crate::data::GameData;
use crate::model::ModelCtx;
use crate::nif_material::NifMaterial;
use crate::nif_spawn::{affine, build_mesh, is_editor_marker, shape_material, shape_material_reusing_tangents, shape_props, shape_skin, to_bevy, Props};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::DynamicSkinnedMeshBounds;
use bevy::math::Affine3A;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use fnv_nif::scene::{scene_objects, Decoded, SceneObject};
use fnv_nif::{NifFile, Value};
use crate::texture::TextureCache;
use bevy::tasks::{futures::check_ready, Task};
use std::collections::HashMap;
use std::sync::Arc;

/// The bone a rigid part without `Prn` hangs from [hypothesis, actors.md §1.2].
pub const DEFAULT_RIGID_BONE: &str = "Bip01 Head";

/// A skeleton's node hierarchy; node 0 is the root, whose own transform the actor's
/// placement replaces (so it is the identity here).
#[derive(Debug, Clone, Default)]
pub struct PreparedSkeleton {
    pub names: Vec<Option<String>>,
    pub parents: Vec<Option<usize>>,
    /// Local transforms in NIF (game) space.
    pub locals: Vec<Transform>,
    /// Bind-pose world transforms relative to the root, NIF space.
    pub bind_world: Vec<Affine3A>,
    pub by_name: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct PartShape {
    pub name: Option<String>,
    pub mesh: Handle<Mesh>,
    pub material: Handle<NifMaterial>,
    /// The shape's properties, to rebuild its material with another base texture.
    pub(crate) props: Props,
    /// CPU copy (tangents and joint attributes included).
    pub cpu: Arc<Mesh>,
    /// Inverse bind poses and the names of the bones, for skinned shapes.
    pub skin: Option<(Handle<SkinnedMeshInverseBindposes>, Vec<String>)>,
    /// Shape transform below the part's root node (the root's own transform excluded,
    /// see [`PreparedPart::root`]), NIF space; used for rigid shapes.
    pub transform: Affine3A,
    /// A dismemberment cap: drawn with no triangles on an intact actor (its `mesh` has
    /// none; `cpu` has them all).
    pub cap: bool,
    /// Its dismemberment partitions (body part and triangles), for drawing it without a
    /// gone part or with only some caps (docs/spec-notes/dismemberment.md §5).
    pub partitions: Arc<Vec<Partition>>,
    /// The shape's ancestors in the part file, from its parent up to (not including) the
    /// root: node name and local transform (NIF space). `transform` is their product
    /// (outermost first) times the shape's own transform, `own`.
    pub ancestors: Vec<(Option<String>, Affine3A)>,
    pub own: Affine3A,
    /// Its position among the file's geometry blocks in depth-first order
    /// (`fnv_nif::scene::geometry_order`): what a record's alternate textures name it by
    /// besides its name (docs/spec-notes/texture-swaps.md §2).
    pub geometry_index: usize,
}

impl PartShape {
    /// The look its material stores, for clips that drive it (`material_anim`).
    pub fn stored_look(&self) -> crate::material_anim::StoredLook {
        let (emissive, mult) = self.props.emissive_parts();
        crate::material_anim::StoredLook(crate::material_anim::Look::stored(emissive, mult, self.props.alpha_part()))
    }

    /// The diffuse texture the model names (NIF texture path), if any.
    pub fn base_texture(&self) -> Option<&str> {
        self.props.textures.first().map(String::as_str).filter(|t| !t.is_empty())
    }

    /// The shape is skin: its shader property has the skin shader type
    /// (`nif_spawn::SHADER_SKIN`).
    pub fn is_skin(&self) -> bool {
        self.props.skin
    }

    /// The shape uses the hair shader: BSShaderFlags bit 18 "Hair" [documented: nif.xml]
    /// (the hair models, and beards among the head parts: `beardmustachecurly.nif`
    /// [observed, docs/spec-notes/actors.md §3]).
    pub fn is_hair(&self) -> bool {
        self.props.hair
    }
}

/// Per shape, given as (cap, skin), whether a texture given for the whole part (a race
/// part's ICON, the EYES or HAIR texture) replaces its diffuse: the skin shapes when the
/// part has any, else every shape; never a dismemberment cap. The race upper bodies hold
/// the underwear as a second shape with its own texture (`upperbody.nif`: `Arms:0`,
/// `armor\underwear\underwearm.dds`), which the skin texture must not cover; mouth,
/// teeth, tongue and eye models have one shape, not of the skin type, and some name no
/// texture of their own [observed: `crates/fnv-nif/examples/skin_survey.rs`; hypothesis
/// for the rule, docs/spec-notes/actors.md §3.1].
pub fn texture_targets(shapes: &[(bool, bool)]) -> Vec<bool> {
    let any_skin = shapes.iter().any(|&(cap, skin)| !cap && skin);
    shapes.iter().map(|&(cap, skin)| !cap && (skin || !any_skin)).collect()
}

/// The normal map a shape takes when its diffuse is replaced by `texture`, or `None` to
/// keep the one its model names: for a skin shape, the file beside `texture` named with
/// `_n` ([`fnv_vfs::normal_map_beside`]) when `exists` finds it (asked with the VFS
/// path). The race part models and the worn models name the adult human's skin maps
/// (every head model names `characters\male\headhuman_n.dds`) while each kind of skin
/// has its own beside its diffuse, most of them named by no mesh. Shapes that are not
/// skin keep theirs: eye textures have no such file, the mouth, teeth and tongue ones a
/// 1 × 1 stand-in where the models name a flat map, and for hair the files do not show
/// which map the game uses [observed: `crates/fnv-nif/examples/texture_refs.rs`;
/// hypothesis for the rule, docs/spec-notes/actors.md §3.1 "Normal maps"].
pub fn skin_normal_map(skin: bool, texture: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let normal = fnv_vfs::normal_map_beside(texture).filter(|_| skin)?;
    exists(&fnv_vfs::texture_path(&normal)).then_some(normal)
}

#[derive(Debug, Clone, Default)]
pub struct PreparedPart {
    pub shapes: Vec<PartShape>,
    /// `Prn`: the bone rigid shapes hang from.
    pub prn: Option<String>,
    /// The root node's own transform (NIF space).
    pub root: Affine3A,
    /// The part's `ProjectileNode…` nodes (where a creature's body weapon fires from)
    /// with their transforms in the part's space, root transform included: read here so
    /// that drawing a creature does not prepare the file again as a model to find them
    /// (#1429: 23.8 ms in one frame for one creature [observed 2026-11-07, Sierra Madre
    /// lobby]).
    pub projectile_nodes: Vec<(String, Affine3A)>,
}

/// Whether a node is named as a projectile node (`ProjectileNode…`, WEAP NNAM).
pub fn is_projectile_node(name: &str) -> bool {
    name.get(..14).is_some_and(|p| p.eq_ignore_ascii_case("ProjectileNode"))
}

impl PreparedPart {
    /// Per shape, whether a texture given for the whole part replaces its diffuse
    /// ([`texture_targets`]).
    pub fn texture_targets(&self) -> Vec<bool> {
        texture_targets(&self.shapes.iter().map(|s| (s.cap, s.is_skin())).collect::<Vec<_>>())
    }

    pub fn attach_bone(&self) -> &str {
        self.prn.as_deref().unwrap_or(DEFAULT_RIGID_BONE)
    }
}

/// Prepared skeletons and parts by archive path; `None` remembers a failure. Materials
/// with a replaced base texture are cached by (path, shape, texture).
#[derive(Resource, Default)]
pub struct ActorParts {
    skeletons: HashMap<String, Option<Arc<PreparedSkeleton>>>,
    parts: HashMap<String, Option<Arc<PreparedPart>>>,
    materials: HashMap<(String, usize, String), Handle<NifMaterial>>,
    clips: HashMap<String, Option<Arc<Clip>>>,
    /// Parts read and parsed on a worker thread ([`ActorCtx::parts_ready`]), waiting for
    /// [`ActorCtx::part`], with when they came in; and the reads under way.
    fetched: HashMap<String, (Nif, f64)>,
    fetching: HashMap<String, Task<Fetched>>,
    /// KF clips being read on a worker thread ([`ActorCtx::clip_ready`]).
    clip_reads: HashMap<String, Task<Result<Clip, String>>>,
    /// Textures an actor's materials name beyond its parts' own (a part's or the race's
    /// skin texture and the normal map beside it) being read on a worker thread
    /// ([`ActorCtx::textures_ready`]), by resolved path; and those whose read gave
    /// nothing (so an actor does not wait for them).
    texture_reads: HashMap<String, Task<TextureFiles>>,
    textures_unread: std::collections::HashSet<String>,
    pub failures: Vec<String>,
}

/// The first controller sequence of the KF file at `path`.
/// A panic while reading it, or a clip whose keys could not be played, is this clip's
/// failure (guard.rs).
fn read_clip(data: &GameData, path: &str) -> Result<Clip, String> {
    let (file, _, dec) = decode(data, path)?;
    crate::guard::guarded(|| {
        let clip = fnv_nif::anim::sequences(&file, &dec).first().map(Clip::from_sequence).ok_or("no sequence")?;
        clip.check()?;
        Ok(clip)
    })
}

/// Where KF clips come from: the actor part cache on the main thread ([`ActorCtx`]), or a
/// [`ClipReader`] on a worker thread (clip sets loaded ahead of the actors that use them).
pub trait ClipSource {
    /// The game's files.
    fn data(&self) -> &GameData;
    /// The first controller sequence of the KF file at `path` (loaded on first use).
    fn clip(&mut self, path: &str) -> Option<Arc<Clip>>;
}

impl ClipSource for ActorCtx<'_, '_> {
    fn data(&self) -> &GameData {
        &self.models.data
    }

    fn clip(&mut self, path: &str) -> Option<Arc<Clip>> {
        ActorCtx::clip(self, path)
    }
}

/// Clips read off the main thread, kept for the reader's life.
pub struct ClipReader {
    data: GameData,
    clips: HashMap<String, Option<Arc<Clip>>>,
}

impl ClipReader {
    pub fn new(data: GameData) -> ClipReader {
        ClipReader { data, clips: HashMap::new() }
    }
}

impl ClipSource for ClipReader {
    fn data(&self) -> &GameData {
        &self.data
    }

    fn clip(&mut self, path: &str) -> Option<Arc<Clip>> {
        let key = fnv_vfs::normalize(path);
        let data = &self.data;
        self.clips.entry(key).or_insert_with_key(|key| read_clip(data, key).ok().map(Arc::new)).clone()
    }
}

/// A NIF file read and parsed: the file, its bytes and its decoded blocks.
type Nif = Result<(NifFile, Vec<u8>, Decoded), String>;

/// A part read off the main thread, with the bytes of the textures its texture sets name
/// (for [`TextureCache::offer`]).
/// Texture files read on a worker thread: (resolved path, bytes).
type TextureFiles = Vec<(String, Vec<u8>)>;

struct Fetched {
    nif: Nif,
    textures: Vec<(String, Vec<u8>)>,
}

/// Seconds a part read ahead waits for its actor before it is dropped (an actor that
/// left view before it was drawn).
const FETCHED_KEEP_SECS: f64 = 30.0;

/// Read and parse the part at `path` and read the textures its texture sets name (on a
/// worker thread).
fn fetch(data: &GameData, path: &str) -> Fetched {
    let nif = decode(data, path);
    let mut textures = Vec::new();
    if let Ok((file, _, dec)) = &nif {
        let mut seen = std::collections::HashSet::new();
        for (i, b) in file.blocks.iter().enumerate() {
            let Some(Ok(set)) = dec.blocks.get(i).filter(|_| b.type_name == "BSShaderTextureSet") else { continue };
            for t in crate::nif_spawn::text_list(set, "textures").iter().filter(|t| !t.is_empty()) {
                let path = crate::texture::resolved_path(t, |p| data.vfs.exists(p));
                if seen.insert(path.clone()) && data.vfs.exists(&path) {
                    textures.extend(data.read(&path).map(|bytes| (path, bytes)));
                }
            }
        }
    }
    Fetched { nif, textures }
}

impl ActorParts {
    /// Take in the parts read ahead that are done (their textures go to `textures`), and
    /// drop those nobody took in time; the number taken in.
    pub fn take_fetched(&mut self, textures: &mut TextureCache, now: f64) -> usize {
        let mut done = Vec::new();
        self.fetching.retain(|path, task| match check_ready(task) {
            Some(f) => {
                done.push((path.clone(), f));
                false
            }
            None => true,
        });
        let n = done.len();
        for (path, f) in done {
            for (t, bytes) in f.textures {
                textures.offer(t, bytes, now);
            }
            self.fetched.insert(path, (f.nif, now));
        }
        self.fetched.retain(|_, (_, at)| now - *at < FETCHED_KEEP_SECS);
        let mut read = Vec::new();
        self.texture_reads.retain(|path, task| match check_ready(task) {
            Some(files) => {
                read.push((path.clone(), files));
                false
            }
            None => true,
        });
        for (path, files) in read {
            if !files.iter().any(|(p, _)| *p == path) {
                self.textures_unread.insert(path);
            }
            for (t, bytes) in files {
                textures.offer(t, bytes, now);
            }
        }
        n
    }

    /// Where the part (or skeleton) at `path` stands: prepared (or failed to be), read on
    /// a worker thread and waiting to be prepared, being read, or none of these.
    pub fn part_state(&self, path: &str) -> PartState {
        let key = fnv_vfs::normalize(path);
        if self.parts.contains_key(&key) || self.skeletons.contains_key(&key) {
            PartState::Prepared
        } else if self.fetched.contains_key(&key) {
            PartState::Read
        } else if self.fetching.contains_key(&key) {
            PartState::Reading
        } else {
            PartState::Unknown
        }
    }
}

/// Where a part stands in [`ActorParts`] ([`ActorParts::part_state`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartState {
    Prepared,
    Read,
    Reading,
    Unknown,
}

/// The asset stores actors are prepared into: the static models' ([`ModelCtx`]), so a
/// system can use both, plus the actor part cache.
pub struct ActorCtx<'a, 'w> {
    pub cache: &'a mut ActorParts,
    pub models: &'a mut ModelCtx<'w>,
}

/// One part to assemble, with its per-actor choices.
#[derive(Debug, Clone)]
pub struct PartUse {
    pub part: Arc<PreparedPart>,
    /// Per shape, the material to draw it with (overrides already applied).
    pub materials: Vec<Handle<NifMaterial>>,
    /// Per shape, a second material the shape is drawn with again, from an entity of its
    /// own beside the first (the blended layer of a hair model over its solid core,
    /// `nif_spawn::hair_model_layer`); empty or `None`: drawn once.
    pub layers: Vec<Option<Handle<NifMaterial>>>,
    /// Per shape, a replacement mesh (e.g. FaceGen-morphed), or `None` for the prepared one.
    pub meshes: Vec<Option<Handle<Mesh>>>,
    /// Shape names not to draw (e.g. the hair's `Hat` shape when no headgear is worn).
    pub hidden_shapes: Vec<String>,
    /// Attach unskinned shapes by their ancestors' names (creatures) instead of the
    /// part's `Prn` (NPCs).
    pub by_ancestor: bool,
}

/// Read and parse a NIF or KF; a panic while parsing is this file's failure (guard.rs).
fn decode(data: &GameData, path: &str) -> Result<(NifFile, Vec<u8>, Decoded), String> {
    crate::guard::guarded(|| {
        let bytes = data.read(path).ok_or("not in any archive")?;
        let schema = data.schema.clone();
        let file = NifFile::parse(&bytes, Some(&schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&file, &bytes, &schema);
        Ok((file, bytes, dec))
    })
}

/// Root first, then every object reachable from it, breadth first.
fn hierarchy(objects: &[SceneObject]) -> Vec<usize> {
    let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let Some(root) = objects.iter().find(|o| o.parent.is_none()) else { return Vec::new() };
    let mut order = vec![root.block];
    let mut i = 0;
    while i < order.len() {
        for &c in &by_block[&order[i]].children {
            if by_block.get(&c).is_some_and(|o| o.is_node) && !order.contains(&c) {
                order.push(c);
            }
        }
        i += 1;
    }
    order
}

/// A skin partition's body part (nif.xml `BSDismemberBodyPartType`, documentation only):
/// 0–13 body parts (BP_TORSO, BP_HEAD, …), 101–113 section caps, 201–213 torso caps, and
/// 1000 × part for torso sections (docs/spec-notes/dismemberment.md §2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    pub body_part: u16,
    /// A cap partition: every partition of a cap shape, and in a shape with drawn
    /// partitions a cap body part (1xx, 2xx) with part flag bit 0 (editor visible)
    /// clear. Not drawn on an intact actor (the radscorpion's `meatcapbody` holds one
    /// editor-visible triangle among its torso caps, Mr. Gutsy's body one torso cap;
    /// dismemberment.md §2.5).
    pub cap: bool,
    /// Its triangles, as indices into the shape's mesh vertices (3 per triangle).
    pub indices: Vec<u32>,
}

/// The dismemberment partitions of a skinned shape: the body part each partition of its
/// `BSDismemberSkinInstance` names, with that partition's triangles from its
/// `NiSkinPartition` (the two lists match by index), mapped to the shape's vertices by
/// the partition's vertex map. Empty for other shapes, or when the lists disagree.
fn shape_partitions(dec: &Decoded, shape: &fnv_nif::Struct, vertices: usize) -> Vec<Partition> {
    let Some(si) = shape.block_ref("skin_instance").and_then(|r| dec.get(r)) else { return Vec::new() };
    let Some(parts) = si.list("partitions") else { return Vec::new() };
    let Some(sp) = si.block_ref("skin_partition").and_then(|r| dec.get(r)).and_then(|p| p.list("partitions")) else { return Vec::new() };
    if parts.len() != sp.len() {
        return Vec::new();
    }
    let ints = |v: Option<&Value>| -> Vec<i64> {
        match v {
            Some(Value::IntList(l)) => l.clone(),
            Some(Value::List(l)) => l.iter().filter_map(Value::as_int).collect(),
            _ => Vec::new(),
        }
    };
    let mut out = Vec::new();
    for (p, s) in parts.iter().zip(sp) {
        let (Value::Struct(p), Value::Struct(s)) = (p, s) else { return Vec::new() };
        let body_part = p.int("body_part").unwrap_or(0) as u16;
        let cap = p.int("part_flag").unwrap_or(1) & 1 == 0;
        let map = ints(s.get("vertex_map"));
        let local = |i: i64| -> Option<u32> {
            let v = if map.is_empty() { i } else { *map.get(usize::try_from(i).ok()?)? };
            usize::try_from(v).ok().filter(|&v| v < vertices).map(|v| v as u32)
        };
        let mut tris: Vec<[i64; 3]> = ints(s.get("triangles")).as_chunks::<3>().0.to_vec();
        // Strips: a triangle per window of three, every other one turned; degenerate
        // ones dropped.
        if let Some(Value::List(strips)) = s.get("strips") {
            for strip in strips {
                let st = ints(Some(strip));
                for (k, w) in st.windows(3).enumerate() {
                    if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
                        continue;
                    }
                    tris.push(if k % 2 == 0 { [w[0], w[1], w[2]] } else { [w[0], w[2], w[1]] });
                }
            }
        }
        let indices = tris.iter().filter_map(|t| Some([local(t[0])?, local(t[1])?, local(t[2])?])).flatten().collect();
        out.push(Partition { body_part, cap, indices });
    }
    out
}

/// Whether a partition is a cap ([`Partition::cap`]): in a cap shape always; else a cap
/// body part (1xx, 2xx) whose part flag bit 0 is clear (`flag_clear`).
fn partition_is_cap(cap_shape: bool, flag_clear: bool, body_part: u16) -> bool {
    cap_shape || (flag_clear && (100..300).contains(&body_part))
}

/// Whether the mixed-cap shape `block` of `path` is reported for the first time (the line
/// is written once per model and shape, not each time the part is prepared again after a
/// release).
fn first_mixed_report(path: &str, block: usize) -> bool {
    static SEEN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<(String, usize)>>> = std::sync::OnceLock::new();
    SEEN.get_or_init(Default::default).lock().is_ok_and(|mut s| s.insert((path.to_string(), block)))
}

/// Whether a skinned shape is a dismemberment cap: its skin instance has partitions and
/// none of them sets part flag bit 0.
fn is_cap(dec: &Decoded, shape: &fnv_nif::Struct) -> bool {
    let Some(si) = shape.block_ref("skin_instance").and_then(|r| dec.get(r)) else { return false };
    let Some(parts) = si.list("partitions") else { return false };
    !parts.is_empty()
        && parts.iter().all(|p| match p {
            Value::Struct(p) => p.int("part_flag").unwrap_or(1) & 1 == 0,
            _ => false,
        })
}

impl ActorParts {
    /// Drop the prepared parts and retextured materials nothing else holds, after their
    /// grace time (`crate::release`); the numbers dropped (parts, materials). A
    /// retextured material that fell back to the part's own counts as the cache's copy.
    pub fn release(
        &mut self,
        parts: &mut crate::release::Unused<String>,
        materials: &mut crate::release::Unused<(String, usize, String)>,
        hints: &mut crate::release::ImageHints,
        stores: &Assets<NifMaterial>,
        now: f64,
        keep: crate::release::KeepLimits,
    ) -> (usize, usize) {
        use crate::release::Holders;
        let own_material = |k: &(String, usize, String)| self.parts.get(&k.0).and_then(|p| p.as_ref()).and_then(|p| p.shapes.get(k.1)).map(|s| &s.material);
        let entries: Vec<_> = self
            .materials
            .iter()
            .map(|(k, m)| {
                let mut h = Holders::default();
                h.add(m);
                if let Some(o) = own_material(k).filter(|o| *o == m) {
                    h.add(o);
                }
                (k.clone(), h.held_elsewhere())
            })
            .collect();
        let gone = materials.sweep_timed(now, entries, keep.secs, keep.max, |_| None);
        for (k, since) in &gone {
            if let Some(m) = self.materials.remove(k) {
                hints.note(stores, &m, *since, now);
            }
        }
        let entries: Vec<_> = self
            .parts
            .iter()
            .filter_map(|(k, p)| p.as_ref().map(|p| (k, p)))
            .map(|(k, p)| {
                if Arc::strong_count(p) > 1 {
                    return (k.clone(), true);
                }
                let mut h = Holders::default();
                for s in &p.shapes {
                    h.add(&s.mesh);
                    h.add(&s.material);
                    if let Some((b, _)) = &s.skin {
                        h.add(b);
                    }
                }
                for (mk, m) in &self.materials {
                    if mk.0 == *k && p.shapes.get(mk.1).is_some_and(|s| s.material == *m) {
                        h.add(m);
                    }
                }
                (k.clone(), h.held_elsewhere())
            })
            .collect();
        let drop = parts.sweep_timed(now, entries, keep.secs, keep.max, |_| None);
        for (k, since) in &drop {
            if let Some(Some(p)) = self.parts.remove(k) {
                for s in &p.shapes {
                    hints.note(stores, &s.material, *since, now);
                }
            }
        }
        (drop.len(), gone.len())
    }
}

impl ActorCtx<'_, '_> {
    /// The prepared skeleton at `path` (loaded on first use).
    pub fn skeleton(&mut self, path: &str) -> Option<Arc<PreparedSkeleton>> {
        let key = fnv_vfs::normalize(path);
        if let Some(s) = self.cache.skeletons.get(&key) {
            return s.clone();
        }
        let ahead = self.cache.fetched.remove(&key).map(|(nif, _)| nif);
        let s = match crate::guard::guarded(|| prepare_skeleton(ahead.unwrap_or_else(|| decode(&self.models.data, &key)))) {
            Ok(s) => Some(Arc::new(s)),
            Err(e) => {
                crate::guard::failed(&mut self.cache.failures, "actor file", format!("{key}: {e}"));
                None
            }
        };
        self.cache.skeletons.insert(key, s.clone());
        s
    }

    /// Whether every part at `paths` is prepared or read ahead, so preparing them reads
    /// no file on this thread. The rest are read (and their textures) on a worker thread,
    /// taken in by [`ActorParts::take_fetched`]: an actor can wait for them before it is
    /// drawn instead of stalling the frame it comes into view (the parse was a sixth of
    /// the hitch then [observed: CPU profile, 2026-10-22]).
    pub fn parts_ready<'p>(&mut self, paths: impl IntoIterator<Item = &'p str>) -> bool {
        let mut ready = true;
        for path in paths {
            let key = fnv_vfs::normalize(path);
            if self.cache.parts.contains_key(&key) || self.cache.skeletons.contains_key(&key) || self.cache.fetched.contains_key(&key) {
                continue;
            }
            ready = false;
            if self.cache.fetching.contains_key(&key) {
                continue;
            }
            let (data, job) = ((*self.models.data).clone(), key.clone());
            let task = crate::read_pool::read_pool().spawn(async move { fetch(&data, &job) });
            self.cache.fetching.insert(key, task);
        }
        ready
    }

    /// Whether the textures at the NIF texture paths `paths` are loaded or read ahead, so
    /// that a material made with one ([`Self::material_with_texture`]) reads no file on this
    /// thread. The rest are read, with the normal map beside each, on a worker thread
    /// (once), taken in by [`ActorParts::take_fetched`]. Missing files count as ready.
    /// Reading a race's skin texture there cost an NPC 5-6 ms in the frame it was drawn
    /// [observed 2026-11-07, #1429, Sierra Madre lobby].
    pub fn textures_ready<'p>(&mut self, paths: impl IntoIterator<Item = &'p str>) -> bool {
        let mut ready = true;
        for raw in paths {
            let vfs = &self.models.data.vfs;
            let path = crate::texture::resolved_path(raw, |p| vfs.exists(p));
            if !vfs.exists(&path) || self.cache.textures_unread.contains(&path) || self.models.textures.has_or_ahead(&path) {
                continue;
            }
            ready = false;
            if self.cache.texture_reads.contains_key(&path) {
                continue;
            }
            let normal = fnv_vfs::normal_map_beside(&path).filter(|n| vfs.exists(n) && !self.models.textures.has_or_ahead(n));
            let (data, job) = ((*self.models.data).clone(), path.clone());
            let task = crate::read_pool::read_pool().spawn(async move { [Some(job), normal].into_iter().flatten().filter_map(|p| data.read(&p).map(|b| (p, b))).collect() });
            self.cache.texture_reads.insert(path, task);
        }
        ready
    }

    /// The prepared part at `path` (loaded on first use).
    pub fn part(&mut self, path: &str) -> Option<Arc<PreparedPart>> {
        let key = fnv_vfs::normalize(path);
        if let Some(p) = self.cache.parts.get(&key) {
            return p.clone();
        }
        let p = match crate::guard::guarded(|| self.prepare_part(&key)) {
            Ok(p) => Some(Arc::new(p)),
            Err(e) => {
                crate::guard::failed(&mut self.cache.failures, "actor file", format!("{key}: {e}"));
                None
            }
        };
        self.cache.parts.insert(key, p.clone());
        p
    }

    /// The part at `path` with a record's alternate textures on the shapes they name
    /// (`fnv_world::alt_textures`, docs/spec-notes/texture-swaps.md): a copy whose shapes'
    /// properties and materials carry the texture sets, cached by the list. No list: the
    /// plain part.
    pub fn part_swapped(&mut self, path: &str, swaps: &[fnv_world::alt_textures::AltTexture]) -> Option<Arc<PreparedPart>> {
        let plain = self.part(path)?;
        if swaps.is_empty() {
            return Some(plain);
        }
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            swaps.hash(&mut h);
            format!("{}|swap{:016x}", fnv_vfs::normalize(path), h.finish())
        };
        if let Some(p) = self.cache.parts.get(&key) {
            return p.clone();
        }
        let named: Vec<(Option<&str>, usize)> = plain.shapes.iter().map(|s| (s.name.as_deref(), s.geometry_index)).collect();
        let mut set_of: Vec<Option<&[Option<String>; 6]>> = vec![None; plain.shapes.len()];
        for e in swaps {
            let to = fnv_world::alt_textures::targets(e, &named);
            if to.is_empty() {
                info!("{path}: texture swap: no shape {:?} (index {}) for texture set {:08X}", e.shape, e.index, e.texture_set);
            }
            for i in to {
                set_of[i] = Some(&e.textures);
            }
        }
        let mut part = (*plain).clone();
        let mut done = Vec::new();
        for (i, set) in set_of.iter().enumerate() {
            let Some(set) = set else { continue };
            part.shapes[i].material = self.material_with_texture_set(path, &plain, i, set);
            part.shapes[i].props.use_texture_set(set);
            done.push(format!("{} → {}", part.shapes[i].name.as_deref().unwrap_or("-"), part.shapes[i].base_texture().unwrap_or("-")));
        }
        info!("{}: texture swap: {} entr(ies), {} shape(s) retextured: {}", fnv_vfs::normalize(path), swaps.len(), done.len(), done.join(", "));
        let part = Some(Arc::new(part));
        self.cache.parts.insert(key, part.clone());
        part
    }

    /// Whether [`clip`](Self::clip) can give the KF file at `path` without reading it:
    /// loaded (or failed to load), or read on a worker thread since the last ask. When
    /// not, its read is started on a worker thread (once), so a caller can wait a few
    /// frames for it instead of reading it in this frame (#1429).
    pub fn clip_ready(&mut self, path: &str) -> bool {
        let key = fnv_vfs::normalize(path);
        if self.cache.clips.contains_key(&key) {
            return true;
        }
        if let Some(task) = self.cache.clip_reads.get_mut(&key) {
            let Some(read) = check_ready(task) else { return false };
            self.cache.clip_reads.remove(&key);
            self.take_clip(key, read);
            return true;
        }
        let (data, job) = ((*self.models.data).clone(), key.clone());
        let task = crate::read_pool::read_pool().spawn(async move { read_clip(&data, &job) });
        self.cache.clip_reads.insert(key, task);
        false
    }

    /// Keep a clip read (or its failure) under `key`.
    fn take_clip(&mut self, key: String, read: Result<Clip, String>) -> Option<Arc<Clip>> {
        let c = match read {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                crate::guard::failed(&mut self.cache.failures, "actor file", format!("{key}: {e}"));
                None
            }
        };
        self.cache.clips.insert(key, c.clone());
        c
    }

    /// The first controller sequence of the KF file at `path` (loaded on first use).
    pub fn clip(&mut self, path: &str) -> Option<Arc<Clip>> {
        let key = fnv_vfs::normalize(path);
        if let Some(c) = self.cache.clips.get(&key) {
            return c.clone();
        }
        // A read started on a worker thread is not started again here; it is dropped.
        self.cache.clip_reads.remove(&key);
        let read = read_clip(&self.models.data, &key);
        self.take_clip(key, read)
    }

    /// The material of shape `i` of the part at `path` with its base texture replaced by
    /// `texture` (a texture path; cached). A skin shape also takes the normal map that
    /// lies beside `texture` ([`skin_normal_map`]).
    pub fn material_with_texture(&mut self, path: &str, part: &PreparedPart, i: usize, texture: &str) -> Handle<NifMaterial> {
        let key = (fnv_vfs::normalize(path), i, fnv_vfs::normalize(texture));
        if let Some(m) = self.cache.materials.get(&key) {
            return m.clone();
        }
        let _span = bevy::log::info_span!("actor::material_with_texture").entered();
        let s = &part.shapes[i];
        let mut props = s.props.clone();
        if props.textures.is_empty() {
            props.textures.push(String::new());
        }
        props.textures[0] = texture.to_string();
        let vfs = &self.models.data.vfs;
        if let Some(normal) = skin_normal_map(s.is_skin(), texture, |p| vfs.exists(p)) {
            if props.textures.len() < 2 {
                props.textures.push(String::new());
            }
            props.textures[1] = normal;
        }
        let mesh_span = bevy::log::info_span!("actor::material_mesh_copy").entered();
        let mut mesh = (*s.cpu).clone();
        drop(mesh_span);
        let m = &mut *self.models;
        let (material, missing) = shape_material_reusing_tangents(&props, &mut mesh, &mut m.textures, &m.data, &mut m.images, m.formats.as_deref());
        let handle = match missing {
            // Keep the model's own texture when the replacement cannot be loaded.
            Some(t) => {
                self.cache.failures.push(format!("{}: texture {t} not loaded", key.0));
                s.material.clone()
            }
            None => self.models.materials.add(material),
        };
        self.cache.materials.insert(key, handle.clone());
        handle
    }

    /// The material of shape `i` of the part at `path` with a texture set's files put in
    /// (a record's alternate textures, texture-swaps.md §3; cached).
    pub fn material_with_texture_set(&mut self, path: &str, part: &PreparedPart, i: usize, set: &[Option<String>; 6]) -> Handle<NifMaterial> {
        let joined: Vec<&str> = set.iter().map(|t| t.as_deref().unwrap_or("")).collect();
        let key = (fnv_vfs::normalize(path), i, format!("txst:{}", joined.join("|").to_ascii_lowercase()));
        if let Some(m) = self.cache.materials.get(&key) {
            return m.clone();
        }
        let _span = bevy::log::info_span!("actor::material_with_texture_set").entered();
        let s = &part.shapes[i];
        let mut props = s.props.clone();
        props.use_texture_set(set);
        let mut mesh = (*s.cpu).clone();
        let m = &mut *self.models;
        let (material, missing) = shape_material_reusing_tangents(&props, &mut mesh, &mut m.textures, &m.data, &mut m.images, m.formats.as_deref());
        let handle = match missing {
            Some(t) => {
                self.cache.failures.push(format!("{}: texture {t} not loaded", key.0));
                s.material.clone()
            }
            None => self.models.materials.add(material),
        };
        self.cache.materials.insert(key, handle.clone());
        handle
    }

    /// A material derived from shape `i`'s, changed by `f` (not cached: per actor).
    pub fn material_variant(&mut self, part: &PreparedPart, i: usize, f: impl FnOnce(&mut NifMaterial)) -> Handle<NifMaterial> {
        let Some(mut m) = self.models.materials.get(&part.shapes[i].material).cloned() else { return part.shapes[i].material.clone() };
        f(&mut m);
        self.models.materials.add(m)
    }

    fn prepare_part(&mut self, path: &str) -> Result<PreparedPart, String> {
        let ahead = self.cache.fetched.remove(path).map(|(nif, _)| nif);
        let m = &mut *self.models;
        let (file, _bytes, dec) = ahead.unwrap_or_else(|| decode(&m.data, path))?;
        let objects = scene_objects(&file, &dec);
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        let hidden_self = |b: usize| {
            dec.blocks[b].as_ref().ok().and_then(|s| s.int("flags")).unwrap_or(0) & 1 != 0
                || is_editor_marker(by_block.get(&b).and_then(|o| o.name.as_deref()))
        };
        let mut out = PreparedPart::default();
        // `Prn` on the root node (actors.md §1.2).
        if let Some(r) = objects.iter().find(|o| o.parent.is_none()) {
            out.root = affine(&r.transform);
        }
        if let Some(root) = objects.iter().find(|o| o.parent.is_none()).and_then(|o| dec.blocks[o.block].as_ref().ok()) {
            for &e in root.refs("extra_data").unwrap_or_default() {
                if let Some(x) = dec.get(e).filter(|x| x.text("name").is_some_and(|n| n.eq_ignore_ascii_case("Prn"))) {
                    out.prn = x.text("string_data").map(str::to_string);
                }
            }
        }
        if objects.iter().any(|o| o.is_node && o.name.as_deref().is_some_and(is_projectile_node)) {
            let (nodes, _) = crate::model::model_nodes(&objects);
            for (i, n) in nodes.iter().enumerate() {
                let Some(name) = n.name.as_deref().filter(|n| is_projectile_node(n)) else { continue };
                let mut path = Vec::new();
                let mut at = Some(i);
                while let Some(j) = at {
                    path.push(j);
                    at = nodes[j].parent;
                }
                let local = path.iter().rev().fold(out.root, |m, &j| m * nodes[j].local);
                out.projectile_nodes.push((name.to_string(), local));
            }
        }
        let order = fnv_nif::scene::geometry_order(&objects);
        for o in objects.iter().filter(|o| !o.is_node) {
            // Below the root; the root's own transform is `out.root` (see `rigid_local`).
            let own = if o.parent.is_some() { affine(&o.transform) } else { Affine3A::IDENTITY };
            let mut t = own;
            let mut ancestors = Vec::new();
            let mut hidden = hidden_self(o.block);
            let mut cur = o.parent;
            let mut depth = 0;
            while let Some(p) = cur {
                let Some(po) = by_block.get(&p) else { break };
                if po.parent.is_some() {
                    t = affine(&po.transform) * t;
                    ancestors.push((po.name.clone(), affine(&po.transform)));
                }
                hidden |= hidden_self(p);
                cur = po.parent;
                depth += 1;
                if depth > 512 {
                    break;
                }
            }
            if hidden {
                continue;
            }
            let Ok(shape) = &dec.blocks[o.block] else { continue };
            let Some(data) = shape.block_ref("data").and_then(|r| dec.get(r)) else { continue };
            let mut props = shape_props(&file, &dec, o, &by_block);
            if props.is_helper() {
                continue;
            }
            // The actor's clips may fade this shape's material alpha (Mr. House's screens).
            props.alpha_gated = crate::nif_spawn::material_alpha_controlled(&file, &dec, o.block);
            let Some((mut mesh, _tris)) = build_mesh(data) else { continue };
            // Hair-shader shapes (hair, beards, eyebrows) carry a mask in their vertex
            // colours, not a colour [observed: red 0.04–0.39 with green and blue at 1 in
            // `hairbase.nif` and `beardmustachecurly.nif`; multiplied in, the hair turns
            // cyan]; they are not applied [hypothesis: what the mask does is open].
            if props.hair {
                mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
            }
            let (material, missing) = shape_material(&props, &mut mesh, &mut m.textures, &m.data, &mut m.images, m.formats.as_deref());
            if let Some(t) = missing {
                self.cache.failures.push(format!("{path}: texture {t} not loaded"));
            }
            let cap = is_cap(&dec, shape);
            let mut partitions = shape_partitions(&dec, shape, mesh.count_vertices());
            // In a shape with drawn partitions, only a cap body part (section caps 1xx,
            // torso caps 2xx) with bit 0 clear counts as a cap, so an outfit's piece
            // with an odd flag is never hidden; in a cap shape every partition is one.
            for p in &mut partitions {
                p.cap = partition_is_cap(cap, p.cap, p.body_part);
            }
            let partitions = Arc::new(partitions);
            let caps = partitions.iter().filter(|p| p.cap).count();
            if !cap && caps > 0 && first_mixed_report(path, o.block) {
                info!("actor parts: {path}: shape {:?} hides {caps} cap partitions beside {} drawn", o.name.as_deref().unwrap_or(""), partitions.len() - caps);
            }
            let skin = shape_skin(&dec, shape, mesh.count_vertices()).and_then(|skin| {
                let names: Option<Vec<String>> = skin.bones.iter().map(|b| by_block.get(b).and_then(|o| o.name.clone())).collect();
                let names = names?;
                let binds = skin.insert_into(&mut mesh);
                Some((m.bindposes.add(binds), names))
            });
            let mesh = if skin.is_some() {
                mesh.with_generated_skinned_mesh_bounds()
                    .unwrap_or_else(|_| Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD))
            } else {
                mesh
            };
            let cpu = Arc::new(mesh.clone());
            // A cap draws nothing until its part is gone (`dismember_mesh`); nor does a cap
            // partition in a shape that also holds drawn ones.
            let mesh = if partitions.iter().any(|p| p.cap) { dismember_mesh(&cpu, &partitions, |p| !p.cap) } else { mesh };
            out.shapes.push(PartShape {
                name: o.name.clone(),
                mesh: m.meshes.add(mesh),
                material: m.materials.add(material),
                props,
                cpu,
                skin,
                transform: t,
                cap,
                partitions,
                ancestors,
                own,
                geometry_index: order.get(&o.block).copied().unwrap_or(usize::MAX),
            });
        }
        Ok(out)
    }
}

fn prepare_skeleton(nif: Nif) -> Result<PreparedSkeleton, String> {
    let (file, _bytes, dec) = nif?;
    let objects = scene_objects(&file, &dec);
    let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let order = hierarchy(&objects);
    if order.is_empty() {
        return Err("no root node".into());
    }
    let index: HashMap<usize, usize> = order.iter().enumerate().map(|(i, &b)| (b, i)).collect();
    let mut s = PreparedSkeleton::default();
    for (i, &b) in order.iter().enumerate() {
        let o = by_block.get(&b).ok_or_else(|| format!("block {b} is not a scene object"))?;
        let parent = o.parent.and_then(|p| index.get(&p).copied());
        // A broken skeleton (a mod's file) is refused whole: a NaN bone
        // would spread NaN to every bone below it, the skinned meshes and the physics.
        let t = &o.transform;
        let finite = t.translation.iter().chain(&t.rotation).all(|v| v.is_finite()) && t.scale.is_finite();
        if i > 0 && !finite {
            return Err(format!("node {:?} (block {b}) has a non-finite transform", o.name.as_deref().unwrap_or("")));
        }
        let local = if i == 0 { Affine3A::IDENTITY } else { affine(&o.transform) };
        let world = match parent {
            Some(p) => *s.bind_world.get(p).ok_or_else(|| format!("node {:?} (block {b}) comes before its parent", o.name.as_deref().unwrap_or("")))? * local,
            None => local,
        };
        if let Some(n) = &o.name {
            s.by_name.entry(n.clone()).or_insert(i);
        }
        s.names.push(o.name.clone());
        s.parents.push(parent);
        s.locals.push(if i == 0 { Transform::IDENTITY } else { to_bevy(&o.transform) });
        s.bind_world.push(world);
    }
    Ok(s)
}

/// The local transform of a rigid part's shape under bone `b`, given the part's root
/// transform and the shape's transform below it: root ∘ shape. On the head bone
/// ([`DEFAULT_RIGID_BONE`]) that is preceded by the inverse of the bone's bind rotation
/// and scale unless the root already is that inverse; on any other bone the part is an
/// ordinary child (actors.md §1.2).
pub fn rigid_local(skel: &PreparedSkeleton, b: usize, root: &Affine3A, shape: &Affine3A) -> Affine3A {
    let head = skel.names.get(b).and_then(|n| n.as_deref()) == Some(DEFAULT_RIGID_BONE);
    // A bone missing or with a bind rotation that cannot be inverted (a broken
    // skeleton): the part is placed as an ordinary child.
    let Some(bind) = skel.bind_world.get(b).map(|w| Mat3::from(w.matrix3)).filter(|m| m.determinant().abs() > 1e-12 && m.is_finite()) else {
        return *root * *shape;
    };
    let inv = bind.inverse();
    let compensated = (Mat3::from(root.matrix3) - inv).to_cols_array().iter().all(|d| d.abs() < 0.02);
    if !head || compensated {
        *root * *shape
    } else {
        Affine3A::from_mat3(inv) * *root * *shape
    }
}

/// Where an unskinned creature shape hangs (actors.md §10.2): the skeleton node of its
/// nearest ancestor whose name the skeleton has, with the transforms below that
/// ancestor; else the part's `Prn` bone, as an ordinary child with the part's root
/// transform kept (root ∘ shape) whichever bone it is, `Bip01 Head` included: the NPC
/// head rule of [`rigid_local`] does not hold for creatures [observed: dog eyes and
/// skull caps, the Protectron's dome and hat]; else the skeleton's root with the whole
/// transform below the part's root.
pub fn creature_rigid_attach(skel: &PreparedSkeleton, part: &PreparedPart, s: &PartShape) -> (usize, Affine3A) {
    let mut below = s.own;
    for (name, t) in &s.ancestors {
        if let Some(&b) = name.as_ref().and_then(|n| skel.by_name.get(n)) {
            return (b, below);
        }
        below = *t * below;
    }
    match part.prn.as_ref().and_then(|n| skel.by_name.get(n)) {
        Some(&b) => (b, part.root * s.transform),
        None => (0, s.transform),
    }
}

/// Make a skinned shape that is drawn rigidly (a weapon's or a held object's shape hung
/// from a bone, or a part whose bones the skeleton lacks) a valid skinned draw: a skin of
/// `joints` joints that are all the shape's own entity `e`, with identity inverse bind
/// poses, so each vertex lands where the rigid entity puts it and follows its transform.
/// Its mesh keeps the skin's joint attributes, and Bevy draws such a mesh through the
/// skinned pipeline; with no skin the see-through (blended) pass bound the model-only
/// mesh group to it, a GPU validation error each frame that stopped the run (#1424: the
/// Sierra Madre lobby hologram's boxing tape, drawn see-through by its effect shader)
/// [observed 2026-11-06; bevy_pbr 0.20.1 `SetMeshBindGroup` chooses the blended pass's
/// group by the entity's skin, the pipeline by the mesh's attributes].
pub fn skin_in_place(commands: &mut Commands, e: Entity, joints: usize) {
    let n = joints.max(1);
    commands.queue(move |world: &mut World| {
        let binds = world.resource_mut::<Assets<SkinnedMeshInverseBindposes>>().add(SkinnedMeshInverseBindposes::from(vec![Mat4::IDENTITY; n]));
        if let Ok(mut ent) = world.get_entity_mut(e) {
            ent.insert(SkinnedMesh { inverse_bindposes: binds, joints: vec![e; n] });
        }
    });
}

/// What [`spawn_actor`] spawned.
#[derive(Debug)]
pub struct SpawnedActor {
    pub root: Entity,
    /// The skeleton's node entities, in [`PreparedSkeleton`] order (`nodes[0]` is `root`).
    pub nodes: Vec<Entity>,
    /// Entities carrying meshes.
    pub drawn: Vec<Entity>,
    /// For each of `drawn`, the index of the part (in the `parts` given) it belongs to.
    pub part_of: Vec<usize>,
    /// For each of `drawn`, the index of its shape in that part's `shapes`.
    pub shape_of: Vec<usize>,
    /// Skinned shapes whose bones the skeleton lacks: drawn rigidly at the root instead.
    pub unbound: usize,
}

/// Spawn an actor under `parent`: the skeleton's nodes (root at `transform`), then every
/// part's shapes, skinned to the skeleton or hung from their bone. Caps and
/// `hidden_shapes` are not spawned.
pub fn spawn_actor(commands: &mut Commands, parent: Entity, transform: Transform, skel: &PreparedSkeleton, parts: &[PartUse]) -> SpawnedActor {
    let nodes: Vec<Entity> = skel
        .locals
        .iter()
        .enumerate()
        .map(|(i, l)| commands.spawn((if i == 0 { transform } else { *l }, Visibility::Inherited)).id())
        .collect();
    commands.entity(parent).add_child(nodes[0]);
    for (i, p) in skel.parents.iter().enumerate().skip(1) {
        commands.entity(nodes[p.unwrap_or(0)]).add_child(nodes[i]);
    }
    let mut out = SpawnedActor { root: nodes[0], nodes, drawn: Vec::new(), part_of: Vec::new(), shape_of: Vec::new(), unbound: 0 };
    spawn_parts(commands, skel, parts, &mut out);
    out
}

/// On a drawn shape of an actor's part: the shape's name in its model, lower case (what
/// `SwapTexture` names; nvforever `texture_swap.rs`).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ShapeName(pub String);

/// Spawn every part's shapes on the skeleton nodes of `out` (an actor spawned already:
/// [`spawn_actor`], or parts put on later, as armour taken off shows the body), adding
/// them to its `drawn`, `part_of` (indices into `parts`) and `shape_of`.
/// `cpu` drawn with only the partitions `keep` keeps. A shape without partitions keeps
/// all its triangles when `keep` keeps a BP_TORSO (0) partition that is no cap, else none.
pub fn dismember_mesh(cpu: &Mesh, partitions: &[Partition], keep: impl Fn(&Partition) -> bool) -> Mesh {
    let mut m = cpu.clone();
    let idx: Vec<u32> = if partitions.is_empty() {
        if keep(&Partition { body_part: 0, cap: false, indices: Vec::new() }) {
            return m;
        }
        Vec::new()
    } else {
        partitions.iter().filter(|p| keep(p)).flat_map(|p| p.indices.iter().copied()).collect()
    };
    m.insert_indices(Indices::U32(idx));
    m
}

/// Per vertex: four joint indices and their weights.
pub type SkinWeights<'a> = (&'a [[u16; 4]], &'a [[f32; 4]]);

/// The skin weights of a skinned mesh's vertices.
pub fn skin_weights(m: &Mesh) -> Option<SkinWeights<'_>> {
    match (m.attribute(Mesh::ATTRIBUTE_JOINT_INDEX), m.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)) {
        (Some(VertexAttributeValues::Uint16x4(j)), Some(VertexAttributeValues::Float32x4(w))) if j.len() == w.len() => Some((j, w)),
        _ => None,
    }
}

/// `cpu` cut apart where severed pieces leave the body (docs/spec-notes/dismemberment.md
/// §5): `groups` are triangles (3 indices each) with the side they go with, 0 the body
/// and k > 0 a severed piece, and `side[j]` is the side joint j lies on. Every triangle
/// gets vertices of its own whose weights on joints of other sides are dropped and the
/// rest scaled back up to 1; a vertex left with no weight on its side is carried whole by
/// `fallback[k]`, or keeps its weights when there is none. So no triangle is stretched
/// between a piece and the body.
pub fn sever_mesh(cpu: &Mesh, groups: &[(u8, Vec<u32>)], side: &[u8], fallback: &[Option<u16>]) -> Mesh {
    let mut m = cpu.clone();
    let corner_side: Vec<u8> = groups.iter().flat_map(|(k, t)| std::iter::repeat_n(*k, t.len())).collect();
    m.insert_indices(Indices::U32(groups.iter().flat_map(|(_, t)| t.iter().copied()).collect()));
    m.duplicate_vertices();
    let Some((joints, weights)) = skin_weights(&m) else { return m };
    let (mut joints, mut weights) = (joints.to_vec(), weights.to_vec());
    for ((&k, j), w) in corner_side.iter().zip(&mut joints).zip(&mut weights) {
        let mut kept = *w;
        for c in 0..4 {
            if side.get(j[c] as usize) != Some(&k) {
                kept[c] = 0.0;
            }
        }
        let sum: f32 = kept.iter().sum();
        if sum > 1e-4 {
            *w = kept.map(|x| x / sum);
        } else if let Some(f) = fallback.get(k as usize).copied().flatten() {
            *j = [f, 0, 0, 0];
            *w = [1.0, 0.0, 0.0, 0.0];
        }
    }
    m.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(joints));
    m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, VertexAttributeValues::Float32x4(weights));
    // Vertices moved to other joints: the per-joint bounds are measured again.
    let _ = m.generate_skinned_mesh_bounds();
    m
}

/// A drawn actor shape, for dismemberment (docs/spec-notes/dismemberment.md §5): what it
/// is drawn from and which bones carry it.
#[derive(Component, Clone)]
pub struct LimbShape {
    pub cap: bool,
    pub partitions: Arc<Vec<Partition>>,
    pub cpu: Arc<Mesh>,
    /// The bones it is skinned to, or the one it hangs from.
    pub bones: Vec<String>,
}

pub fn spawn_parts(commands: &mut Commands, skel: &PreparedSkeleton, parts: &[PartUse], out: &mut SpawnedActor) {
    let nodes = &out.nodes;
    for (pi, u) in parts.iter().enumerate() {
        let bone = skel.by_name.get(u.part.attach_bone()).copied();
        for (i, s) in u.part.shapes.iter().enumerate() {
            if s.name.as_ref().is_some_and(|n| u.hidden_shapes.iter().any(|h| h.eq_ignore_ascii_case(n))) {
                continue;
            }
            // Caps are spawned with no triangles (`PartShape::cap`); a cap that cannot
            // bind to the skeleton is left out.
            if s.cap && s.skin.as_ref().is_none_or(|(_, names)| names.iter().any(|n| !skel.by_name.contains_key(n))) {
                continue;
            }
            let bones = match &s.skin {
                Some((_, names)) => names.clone(),
                None => vec![u.part.attach_bone().to_string()],
            };
            let limb = LimbShape { cap: s.cap, partitions: s.partitions.clone(), cpu: s.cpu.clone(), bones };
            let mesh = u.meshes.get(i).cloned().flatten().unwrap_or_else(|| s.mesh.clone());
            let material = u.materials.get(i).cloned().unwrap_or_else(|| s.material.clone());
            let joints: Option<Vec<Entity>> =
                s.skin.as_ref().and_then(|(_, names)| names.iter().map(|n| skel.by_name.get(n).map(|&j| nodes[j])).collect());
            if matches!((&s.skin, &joints), (Some(_), None)) {
                out.unbound += 1;
            }
            // The shape, and its second layer where it has one (`PartUse::layers`): the
            // same mesh at the same place under the same bones.
            let layer = u.layers.get(i).cloned().flatten();
            for material in std::iter::once(material).chain(layer) {
                let e = match (&s.skin, joints.clone()) {
                    (Some((binds, _)), Some(joints)) => {
                        let e = commands
                            .spawn((
                                Mesh3d(mesh.clone()),
                                MeshMaterial3d(material),
                                Transform::IDENTITY,
                                SkinnedMesh { inverse_bindposes: binds.clone(), joints },
                                DynamicSkinnedMeshBounds,
                            ))
                            .id();
                        commands.entity(nodes[0]).add_child(e);
                        e
                    }
                    (Some((_, names)), None) => {
                        // Bones missing from the skeleton: draw the bind pose at the root.
                        let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), Transform::from_matrix(Mat4::from(s.transform)))).id();
                        commands.entity(nodes[0]).add_child(e);
                        skin_in_place(commands, e, names.len());
                        e
                    }
                    (None, _) => {
                        let (at, local) = match bone {
                            _ if u.by_ancestor => {
                                let (b, local) = creature_rigid_attach(skel, &u.part, s);
                                (nodes[b], local)
                            }
                            Some(b) => (nodes[b], rigid_local(skel, b, &u.part.root, &s.transform)),
                            None => (nodes[0], s.transform),
                        };
                        let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), Transform::from_matrix(Mat4::from(local)))).id();
                        commands.entity(at).add_child(e);
                        e
                    }
                };
                if let Some(n) = &s.name {
                    commands.entity(e).insert((ShapeName(n.to_ascii_lowercase()), s.stored_look()));
                }
                commands.entity(e).insert(limb.clone());
                out.drawn.push(e);
                out.part_of.push(pi);
                out.shape_of.push(i);
            }
        }
    }
}

/// The skeleton node a clip track named `name` drives: any node but the root, whose
/// transform is the actor's placement.
pub fn clip_target(skel: &PreparedSkeleton, name: &str) -> Option<usize> {
    skel.by_name.get(name).copied().filter(|&i| i > 0)
}

/// The clip's track names split into those that drive a node of `skel` and those that
/// do not (for reports).
pub fn clip_binding<'a>(skel: &PreparedSkeleton, clip: &'a Clip) -> (Vec<&'a str>, Vec<&'a str>) {
    clip.tracks.iter().map(|(n, _)| n.as_str()).partition(|n| clip_target(skel, n).is_some())
}

/// `clip` bound to the nodes of `skel` (the root excluded, [`clip_target`]), to share
/// between the actors of that skeleton.
pub fn bind_clip(skel: &PreparedSkeleton, clip: Arc<Clip>) -> BoundClip {
    BoundClip::bind(clip, |n| clip_target(skel, n))
}

/// A [`BlendPlayer`] for a spawned actor (no clips yet): tracks drive the bone entities
/// by node name, so skinned shapes and rigid parts under their bones follow
/// (docs/spec-notes/actors.md §7 and §8).
pub fn blend_player(actor: &SpawnedActor, skel: &PreparedSkeleton) -> BlendPlayer {
    BlendPlayer::new(actor.nodes.clone(), Arc::new(skel.locals.clone()))
}

/// The prepared skeleton at `path`, read without a [`ClipReader`] (the examples).
pub fn probe_skeleton(data: &GameData, path: &str) -> Result<PreparedSkeleton, String> {
    prepare_skeleton(decode(data, &fnv_vfs::normalize(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_severed_piece_takes_its_own_vertices_weighted_to_its_side_only() {
        // Two triangles sharing vertices 1 and 2, which are weighted half to joint 0 (the
        // body) and half to joint 1 (the piece); vertex 3 only to the body.
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 4]);
        m.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(vec![[0, 0, 0, 0], [0, 1, 0, 0], [0, 1, 0, 0], [0, 0, 0, 0]]));
        m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[1.0f32, 0.0, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]]);
        let out = sever_mesh(&m, &[(0, vec![0, 1, 2]), (1, vec![1, 2, 3])], &[0, 1], &[None, Some(1)]);
        let (j, w) = skin_weights(&out).unwrap();
        assert_eq!(out.count_vertices(), 6);
        // The body's copies: all on joint 0.
        assert!(j[..3].iter().zip(&w[..3]).all(|(j, w)| j[0] == 0 && w[0] == 1.0));
        // The piece's copies of 1 and 2: all on joint 1; vertex 3 had nothing there, so
        // the fallback carries it.
        assert_eq!((j[3], w[3]), ([0, 1, 0, 0], [0.0, 1.0, 0.0, 0.0]));
        assert_eq!((j[5], w[5]), ([1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]));
    }

    #[test]
    fn cap_partitions_are_hidden_also_in_a_shape_with_drawn_ones() {
        // The radscorpion's `meatcapbody` (dismemberment.md §2.5): torso caps 206 and 203
        // (part flag bit 0 clear) around one editor-visible BP_TORSO triangle.
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 9]);
        let parts = [
            Partition { body_part: 206, cap: true, indices: vec![0, 1, 2] },
            Partition { body_part: 0, cap: false, indices: vec![3, 4, 5] },
            Partition { body_part: 203, cap: true, indices: vec![6, 7, 8] },
        ];
        let intact = dismember_mesh(&m, &parts, |p| !p.cap);
        assert_eq!(intact.indices().unwrap().iter().collect::<Vec<_>>(), [3, 4, 5]);
        // Its left foreleg (3) gone: that leg's torso cap shows too.
        let gone = dismember_mesh(&m, &parts, |p| !p.cap || p.body_part == 203);
        assert_eq!(gone.indices().unwrap().iter().collect::<Vec<_>>(), [3, 4, 5, 6, 7, 8]);
        // Which partitions count as caps: a flag-clear torso cap beside drawn ones; not
        // the flag-clear 65535 of `wastelandchild02\outfitf.nif`'s `Arms`; all of a cap shape.
        assert!(partition_is_cap(false, true, 205) && partition_is_cap(false, true, 101));
        assert!(!partition_is_cap(false, true, 65535) && !partition_is_cap(false, false, 205));
        assert!(partition_is_cap(true, true, 0));
    }

    #[test]
    fn a_part_texture_goes_on_the_skin_shapes_when_there_are_any() {
        // upperbody.nif: underwear, skin, caps.
        assert_eq!(texture_targets(&[(false, false), (false, true), (true, false)]), [false, true, false]);
        // An eye or a hair model (NoHat, Hat): no skin shape, so every shape.
        assert_eq!(texture_targets(&[(false, false), (false, false)]), [true, true]);
        // A cap of the skin type does not make the part a skin part.
        assert_eq!(texture_targets(&[(false, false), (true, true)]), [true, false]);
        assert_eq!(texture_targets(&[]), Vec::<bool>::new());
    }

    #[test]
    fn a_skin_shape_takes_the_normal_map_beside_its_new_texture() {
        let archive = ["textures\\characters\\ghoul\\upperbodymale_n.dds", "textures\\characters\\mouth\\mouthhuman_n.dds"];
        let exists = |p: &str| archive.contains(&p);
        // As an ICON gives it (no `textures\` in front, mixed case): found by its VFS path,
        // returned as written.
        assert_eq!(skin_normal_map(true, "Characters\\Ghoul\\UpperBodyMale.dds", exists).as_deref(), Some("Characters\\Ghoul\\UpperBodyMale_n.dds"));
        // No such file: the model's own normal map stays.
        assert_eq!(skin_normal_map(true, "characters\\ghoul\\handmale.dds", exists), None);
        // Not a skin shape (mouth, eyes, hair): the model's own stays even when the file exists.
        assert_eq!(skin_normal_map(false, "characters\\mouth\\mouthhuman.dds", exists), None);
        assert_eq!(skin_normal_map(true, "", exists), None);
    }

    #[test]
    fn creature_rigid_shapes_hang_from_their_nearest_skeleton_ancestor() {
        let skel = PreparedSkeleton {
            by_name: HashMap::from([("Bip01 Head".to_string(), 3)]),
            bind_world: vec![Affine3A::IDENTITY; 4],
            ..Default::default()
        };
        let screen = Affine3A::from_translation(Vec3::new(0.0, 5.0, 0.0));
        let head_copy = Affine3A::from_translation(Vec3::new(0.0, 0.0, 100.0));
        let own = Affine3A::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let s = |ancestors: Vec<(Option<String>, Affine3A)>| PartShape {
            name: None,
            mesh: Handle::default(),
            material: Handle::default(),
            props: Props::default(),
            cpu: Arc::new(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)),
            skin: None,
            transform: ancestors.iter().rev().fold(Affine3A::IDENTITY, |acc, a| acc * a.1) * own,
            cap: false,
            partitions: Arc::default(),
            ancestors,
            own,
            geometry_index: 0,
        };
        let mut part = PreparedPart::default();
        // Screen01 (not a bone) under the part's copy of Bip01 Head: hangs from the head
        // with Screen01's transform kept.
        let shape = s(vec![(Some("Screen01".into()), screen), (Some("Bip01 Head".into()), head_copy)]);
        let (b, local) = creature_rigid_attach(&skel, &part, &shape);
        assert_eq!(b, 3);
        assert!((Vec3::from(local.translation) - Vec3::new(1.0, 5.0, 0.0)).length() < 1e-5);
        // No ancestor the skeleton has, no Prn: the root, with the whole transform.
        let shape = s(vec![(Some("Screen01".into()), screen)]);
        let (b, local) = creature_rigid_attach(&skel, &part, &shape);
        assert_eq!(b, 0);
        assert!((Vec3::from(local.translation) - Vec3::new(1.0, 5.0, 0.0)).length() < 1e-5);
        // No such ancestor but a `Prn`: that bone, with the part's root transform kept.
        part.prn = Some("Bip01 Head".into());
        part.root = Affine3A::from_translation(Vec3::new(0.0, 0.0, 7.0));
        let (b, local) = creature_rigid_attach(&skel, &part, &shape);
        assert_eq!(b, 3);
        assert!((Vec3::from(local.translation) - Vec3::new(1.0, 5.0, 7.0)).length() < 1e-5);
    }

    #[test]
    fn creature_parts_on_the_head_are_plain_children() {
        // A dog's head bone: its x axis runs up and forward along the skull, its z axis
        // to the dog's left (-x of the skeleton; the dog faces +y). The eye model's root
        // is a half turn about an axis in the bone's xy plane; it is neither the identity
        // nor the inverse of the bone's rotation, so the NPC head rule would put both
        // eyes on one side of the head (actors.md §10.2).
        let (c, s) = (0.4, 0.9165);
        let rot = Mat3::from_cols(Vec3::new(0.0, c, s), Vec3::new(0.0, s, -c), -Vec3::X);
        let bone = Affine3A::from_mat3_translation(rot, Vec3::new(0.0, 42.1, 56.7));
        let skel = PreparedSkeleton {
            by_name: HashMap::from([(DEFAULT_RIGID_BONE.to_string(), 1)]),
            bind_world: vec![Affine3A::IDENTITY, bone],
            names: vec![None, Some(DEFAULT_RIGID_BONE.to_string())],
            ..Default::default()
        };
        let root = Affine3A::from_mat3(Mat3::from_cols(Vec3::new(c, s, 0.0), Vec3::new(s, -c, 0.0), -Vec3::Z));
        let part = PreparedPart { prn: Some(DEFAULT_RIGID_BONE.to_string()), root, ..Default::default() };
        let shape = PartShape {
            name: None,
            mesh: Handle::default(),
            material: Handle::default(),
            props: Props::default(),
            cpu: Arc::new(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)),
            skin: None,
            transform: Affine3A::IDENTITY,
            cap: false,
            partitions: Arc::default(),
            ancestors: Vec::new(),
            own: Affine3A::IDENTITY,
            geometry_index: 0,
        };
        let (b, local) = creature_rigid_attach(&skel, &part, &shape);
        assert_eq!(b, 1);
        assert_eq!(local, root);
        // The two eyes of the model (±z) land either side of the skeleton's x = 0, at
        // the same height and the same distance forward.
        let eye = |z: f32| (bone * local).transform_point3(Vec3::new(10.8, 2.9, z));
        let (l, r) = (eye(3.5), eye(-3.5));
        assert!((l.x + r.x).abs() < 1e-3 && (l.x - r.x).abs() > 6.9, "{l} {r}");
        assert!((l.y - r.y).abs() < 1e-3 && (l.z - r.z).abs() < 1e-3, "{l} {r}");
        // The NPC head rule on the same data: one eye above the other, off to one side.
        let npc = bone * rigid_local(&skel, 1, &root, &Affine3A::IDENTITY);
        let (l, r) = (npc.transform_point3(Vec3::new(10.8, 2.9, 3.5)), npc.transform_point3(Vec3::new(10.8, 2.9, -3.5)));
        assert!((l.x - r.x).abs() < 1e-3 && (l.z - r.z).abs() > 6.9, "{l} {r}");
    }

    #[test]
    fn rigid_parts_keep_skeleton_axes() {
        // A bone whose x axis points up (as `Bip01 Head`), at height 112.8.
        let rot = Mat3::from_cols(Vec3::Z, Vec3::Y, -Vec3::X);
        let bone = Affine3A::from_mat3_translation(rot, Vec3::new(0.0, 0.4, 112.8));
        let skel = PreparedSkeleton {
            bind_world: vec![Affine3A::IDENTITY, bone],
            names: vec![None, Some(DEFAULT_RIGID_BONE.to_string())],
            ..Default::default()
        };
        let up = Vec3::new(0.0, 0.0, 15.0);
        let above = Vec3::new(0.0, 0.4, 127.8);
        // Identity nodes (hats, beards): the attachment compensates the bone's rotation.
        let p = (bone * rigid_local(&skel, 1, &Affine3A::IDENTITY, &Affine3A::IDENTITY)).transform_point3(up);
        assert!((p - above).length() < 1e-4, "{p}");
        // Node rotations that cancel (hair): the same.
        let r0 = Affine3A::from_mat3(Mat3::from_cols(-Vec3::X, Vec3::Z, Vec3::Y));
        let p = (bone * rigid_local(&skel, 1, &r0, &r0)).transform_point3(up);
        assert!((p - above).length() < 1e-4, "{p}");
        // A root that stores the bone's inverse rotation (eyes): an ordinary child.
        let inv = Affine3A::from_mat3(rot.inverse());
        let p = (bone * rigid_local(&skel, 1, &inv, &Affine3A::IDENTITY)).transform_point3(up);
        assert!((p - above).length() < 1e-4, "{p}");
    }

    #[test]
    fn rigid_parts_off_the_head_are_plain_children() {
        // A forearm bone whose x axis runs down the arm (as `Bip01 L ForeTwist`): a part
        // modelled 10 units along x (the Pip-Boy) sits 10 units down the arm, whatever
        // the part's node transforms are.
        let rot = Mat3::from_cols(-Vec3::X, -Vec3::Y, Vec3::Z);
        let bone = Affine3A::from_mat3_translation(rot, Vec3::new(-29.0, -3.7, 94.4));
        let skel = PreparedSkeleton {
            bind_world: vec![Affine3A::IDENTITY, bone],
            names: vec![None, Some("Bip01 L ForeTwist".to_string())],
            ..Default::default()
        };
        let shape = Affine3A::from_translation(Vec3::new(8.0, 0.0, 0.0));
        let local = rigid_local(&skel, 1, &Affine3A::IDENTITY, &shape);
        assert_eq!(local, shape);
        let p = (bone * local).transform_point3(Vec3::new(2.0, 0.0, 0.0));
        assert!((p - Vec3::new(-39.0, -3.7, 94.4)).length() < 1e-4, "{p}");
    }

    #[test]
    fn a_degenerate_head_bone_places_parts_as_plain_children() {
        let skel = PreparedSkeleton {
            bind_world: vec![Affine3A::IDENTITY, Affine3A::from_mat3(Mat3::ZERO)],
            names: vec![None, Some(DEFAULT_RIGID_BONE.to_string())],
            ..Default::default()
        };
        let shape = Affine3A::from_translation(Vec3::new(8.0, 0.0, 0.0));
        assert_eq!(rigid_local(&skel, 1, &Affine3A::IDENTITY, &shape), shape);
        // A bone the skeleton does not have: the same.
        assert_eq!(rigid_local(&skel, 9, &Affine3A::IDENTITY, &shape), shape);
    }
}
