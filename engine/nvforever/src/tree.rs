//! SpeedTree trees (TREE references): geometry generated from the `.spt` parameters by
//! `fnv_spt` (docs/spec-notes/speedtree.md §5), scaled to the record's billboard size.
//! Branches use [`BarkMaterial`] with the bark texture; leaf cards use [`LeafMaterial`],
//! which turns every card towards the camera. Both move in the wind (tree.wesl).

use bevy::asset::embedded_asset;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::ecs::system::SystemParam;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use nvf_assets::model::ModelCtx;
use std::collections::HashMap;
use std::sync::Arc;

use crate::world::WorldData;

pub type LeafMaterial = ExtendedMaterial<StandardMaterial, LeafCards>;
pub type BarkMaterial = ExtendedMaterial<StandardMaterial, BarkWind>;
/// Distant tree billboards (treelod.rs).
pub type BillboardMaterial = ExtendedMaterial<StandardMaterial, TreeBillboards>;

const SHADER: &str = "embedded://nvforever/tree.wesl";

/// Wind direction in game space (degrees from +x towards +y): where the wind blows to.
/// The records give none [observed: WTHR DATA has a wind speed but no direction], so it
/// is fixed: 120°, north-north-west (330° clockwise from north), where the original's
/// tumbleweeds rolled around Goodsprings [observed: o27_tumble 2026-11-06, 308–357°;
/// docs/spec-notes/wind.md]. Was 30° [chosen] until then.
const WIND_DIRECTION_DEG: f32 = 120.0;

/// The `TreeWind` uniform of tree.wesl.
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct WindParams {
    /// xyz: wind direction (Bevy world space, horizontal, unit); w: strength 0-1.
    pub wind: Vec4,
    /// x: unused (time comes from the view globals); y: rock angular speed; z: rustle
    /// angular speed (rad/s); w: tree height in the mesh's own units.
    pub motion: Vec4,
    /// Wind before the most recent change, used only on that rendered frame.
    pub previous_wind: Vec4,
    /// x: rendered frame of the change (FrameCount increments in Last).
    pub change_frame: UVec4,
}

/// Leaf cards: camera-facing, rocking and rustling (vertex shaders of the main pass and
/// the prepass, so depth and shadows match).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct LeafCards {
    #[uniform(100)]
    pub wind: WindParams,
}

/// Bark: rocking with the leaves.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct BarkWind {
    #[uniform(100)]
    pub wind: WindParams,
}

/// Distant tree billboards (treelod.rs): upright cards turned about the vertical towards
/// the camera by the vertex shader (tree.wesl `TREE_BILLBOARD`). They stand still: the
/// wind uniform stays zero.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct TreeBillboards {
    #[uniform(100)]
    pub wind: WindParams,
}

impl MaterialExtension for TreeBillboards {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.shader_defs.push("TREE_BILLBOARD".into());
        // Billboards crossfade with the cells whose trees they stand in for (treelod.rs).
        nvf_assets::fade_prepass(descriptor, key.mesh_key);
        nvf_assets::soft_edges(descriptor, key.mesh_key);
        Ok(())
    }
}

impl MaterialExtension for LeafCards {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.shader_defs.push("TREE_LEAVES".into());
        // Trees fade in and out with their cells (fade.rs).
        nvf_assets::fade_prepass(descriptor, key.mesh_key);
        nvf_assets::soft_edges(descriptor, key.mesh_key);
        Ok(())
    }
}

impl MaterialExtension for BarkWind {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.shader_defs.push("TREE_BARK".into());
        // Trees fade in and out with their cells (fade.rs).
        nvf_assets::fade_prepass(descriptor, key.mesh_key);
        Ok(())
    }
}

pub struct TreePlugin;

impl Plugin for TreePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "tree.wesl");
        app.add_plugins((
            MaterialPlugin::<LeafMaterial>::default(),
            MaterialPlugin::<BarkMaterial>::default(),
            MaterialPlugin::<BillboardMaterial>::default(),
        ))
            .init_resource::<TreeCache>()
            .add_systems(Update, wind.after(crate::sky::update_sky));
    }
}

/// The CNAM rock and rustle speeds as angular speeds (rad/s) [tuned by eye:
/// WastelandShrub01 (rock 0.07, rustle 0.35) rocks about every 4 s and rustles about
/// once a second; trees with 1.0 and 1.0 about twice as fast]. How far trees move is
/// set in tree.wesl.
fn angular_speeds(rock: f32, rustle: f32) -> (f32, f32) {
    let tau = std::f32::consts::TAU;
    (tau * (0.2 + 0.6 * rock.max(0.0)), tau * (0.5 + 1.5 * rustle.max(0.0)))
}

/// The weather's wind: xyz its direction (Bevy world space, horizontal, unit), w its
/// strength 0-1 (WTHR DATA wind speed / 255, faded with the weather; none indoors). Also
/// the grass's wind (grass.rs).
pub fn wind_vector(sky: &crate::sky::SkyState) -> Vec4 {
    let speed = |w: &fnv_world::sky::Weather| w.wind_speed as f32 / 255.0;
    let now = speed(&sky.weather);
    let strength = match &sky.prev {
        Some(p) => speed(p) + (now - speed(p)) * sky.fade.clamp(0.0, 1.0),
        None => now,
    };
    let strength = if sky.interior.is_some() { 0.0 } else { strength };
    let a = WIND_DIRECTION_DEG.to_radians();
    // Game (x, y, z) is Bevy (x, z, -y).
    // Steps of 1/1000 so a weather fade touches the materials a few hundred times at most.
    Vec3::new(a.cos(), 0.0, -a.sin()).extend((strength * 1000.0).round() / 1000.0)
}

/// Give every tree material the weather's wind ([`wind_vector`]). Materials are only touched when the wind or the set of
/// trees changes: a changed material is prepared again with every entity drawn with it,
/// which every frame cost a third of the frame rate while cells streamed [observed].
fn wind(
    sky: Res<crate::sky::SkyState>,
    cache: Res<TreeCache>,
    frame: Res<bevy::diagnostic::FrameCount>,
    mut leaves: ResMut<Assets<LeafMaterial>>,
    mut bark: ResMut<Assets<BarkMaterial>>,
    mut last: Local<(Vec4, usize)>,
) {
    let dir = wind_vector(&sky);
    let built = cache.0.len();
    if *last == (dir, built) {
        return;
    }
    *last = (dir, built);
    let set = |w: &mut WindParams| {
        w.previous_wind = w.wind;
        w.change_frame.x = frame.0.wrapping_add(1);
        w.wind = dir;
    };
    for v in cache.0.values().flatten() {
        for p in &v.parts {
            match &p.material {
                PartMaterial::Bark(h) => {
                    // Read first: taking the asset mutably marks it changed.
                    if bark.get(h).is_some_and(|m| m.extension.wind.wind != dir) {
                        if let Some(mut m) = bark.get_mut(h) {
                            set(&mut m.extension.wind);
                        }
                    }
                }
                PartMaterial::Leaves(h) => {
                    // Read first: taking the asset mutably marks it changed.
                    if leaves.get(h).is_some_and(|m| m.extension.wind.wind != dir) {
                        if let Some(mut m) = leaves.get_mut(h) {
                            set(&mut m.extension.wind);
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub enum PartMaterial {
    Bark(Handle<BarkMaterial>),
    Leaves(Handle<LeafMaterial>),
}

#[derive(Clone)]
pub struct TreePart {
    pub mesh: Handle<Mesh>,
    pub material: PartMaterial,
    /// Bounds in the tree's own space (leaf cards included).
    pub aabb: Aabb,
}

/// One TREE base made drawable: parts in tree space (size 1) and the scale to game units.
pub struct TreeVisual {
    pub parts: Vec<TreePart>,
    pub scale: f32,
}

/// Generated trees per TREE FormID; `None` remembers a failure (the caller falls back
/// to the billboard).
#[derive(Resource, Default)]
pub struct TreeCache(HashMap<u32, Option<Arc<TreeVisual>>>);

impl TreeCache {
    /// Trees generated (the stress census).
    pub fn len(&self) -> usize {
        self.0.values().flatten().count()
    }
}

#[derive(SystemParam)]
pub struct TreeCtx<'w> {
    cache: ResMut<'w, TreeCache>,
    leaf_materials: ResMut<'w, Assets<LeafMaterial>>,
    bark_materials: ResMut<'w, Assets<BarkMaterial>>,
}

impl TreeCtx<'_> {
    pub fn visual(&mut self, world: &WorldData, tree: u32, models: &mut ModelCtx) -> Option<Arc<TreeVisual>> {
        if let Some(v) = self.cache.0.get(&tree) {
            return v.clone();
        }
        let made = build(world, tree, models, &mut self.leaf_materials, &mut self.bark_materials).map(Arc::new);
        if made.is_none() {
            warn!("TREE {tree:08X}: no generated geometry, drawing its billboard");
        }
        self.cache.0.insert(tree, made.clone());
        made
    }
}

fn build(
    world: &WorldData,
    tree: u32,
    models: &mut ModelCtx,
    leaf_materials: &mut Assets<LeafMaterial>,
    bark_materials: &mut Assets<BarkMaterial>,
) -> Option<TreeVisual> {
    let rec = fnv_world::placed::tree_record(world.lo, world.schema, tree)?;
    let bytes = models.data.read(&rec.model)?;
    // A broken file (an error or a panic) draws the billboard instead.
    let (spt, g) = match nvf_assets::guard::guarded(|| {
        let spt = fnv_spt::Tree::parse(&bytes).map_err(|e| e.to_string())?;
        let g = fnv_spt::generate(&spt);
        Ok((spt, g))
    }) {
        Ok(v) => v,
        Err(e) => {
            warn!("{}: {e}", rec.model);
            return None;
        }
    };
    let scale = rec.size? / g.width().max(g.top()).max(1e-6);
    let mut parts = Vec::new();
    let (rock, rustle) = angular_speeds(rec.rock_speed.unwrap_or(1.0), rec.rustle_speed.unwrap_or(1.0));
    let wind = WindParams { wind: Vec4::ZERO, motion: Vec4::new(0.0, rock, rustle, g.top()), ..default() };
    // Room for the swing (tree.wesl ROCK: a quarter of the height at the top).
    let sway = 0.3 * g.top();

    if !g.indices.is_empty() {
        let bark = bark_texture(&spt.bark_texture);
        let image = bark.and_then(|b| models.textures.get(&b, true, &models.data, &mut models.images, models.formats.as_deref()));
        let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::RENDER_WORLD);
        let aabb = bounds(g.positions.iter().copied(), sway);
        if let Some(d) = rec.branch_dimming {
            let c: Vec<[f32; 4]> = g.positions.iter().map(|p| dim_colour(g.brightness(d, p[2]))).collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, c);
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, g.positions.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, g.normals.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, g.uvs.clone());
        mesh.insert_indices(bevy::mesh::Indices::U32(g.indices.clone()));
        let mat = bark_materials.add(BarkMaterial {
            base: StandardMaterial {
                base_color: if image.is_some() { Color::WHITE } else { Color::srgb(0.3, 0.25, 0.2) },
                base_color_texture: image,
                perceptual_roughness: 0.9,
                reflectance: 0.2,
                ..default()
            },
            extension: BarkWind { wind },
        });
        parts.push(TreePart { mesh: models.meshes.add(mesh), material: PartMaterial::Bark(mat), aabb });
    }

    if !g.leaves.is_empty() {
        let texture = rec.leaf_texture.as_deref()?;
        let image = models.textures.get(texture, true, &models.data, &mut models.images, models.formats.as_deref())?;
        let n = g.leaves.len();
        let (mut pos, mut nor, mut uv, mut corner) = (Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(4 * n));
        let mut colour = Vec::with_capacity(4 * n);
        let mut indices = Vec::with_capacity(6 * n);
        let mut reach = 0.0f32;
        for l in &g.leaves {
            let [w, h] = l.size;
            let [px, py] = l.pivot;
            let (x0, x1, y0, y1) = (-px * w, (1.0 - px) * w, -py * h, (1.0 - py) * h);
            reach = reach.max(x0.abs()).max(x1.abs()).max(y0.abs()).max(y1.abs());
            // Leaf-map cell corners as stored: bottom right, bottom left, top left, top
            // right (speedtree.md §3, tag 10002); the whole texture without one.
            let c = spt.leaf_uvs.get(l.map).copied().unwrap_or([1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]);
            let first = pos.len() as u32;
            let tint = dim_colour(rec.leaf_dimming.map_or(1.0, |d| g.brightness(d, l.position[2])));
            // Bottom left, bottom right, top right, top left: counter-clockwise on screen.
            for (cx, cy, k) in [(x0, y0, 1), (x1, y0, 0), (x1, y1, 3), (x0, y1, 2)] {
                pos.push(l.position);
                nor.push(l.normal);
                uv.push([c[2 * k], c[2 * k + 1]]);
                corner.push([cx, cy]);
                colour.push(tint);
            }
            indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }
        let aabb = bounds(pos.iter().copied(), reach + sway);
        let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, corner);
        if rec.leaf_dimming.is_some() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colour);
        }
        mesh.insert_indices(bevy::mesh::Indices::U32(indices));
        let mat = leaf_materials.add(LeafMaterial {
            base: StandardMaterial {
                base_color_texture: Some(image),
                alpha_mode: AlphaMode::Mask(0.5),
                cull_mode: None,
                double_sided: false,
                perceptual_roughness: 1.0,
                reflectance: 0.1,
                ..default()
            },
            extension: LeafCards { wind },
        });
        parts.push(TreePart { mesh: models.meshes.add(mesh), material: PartMaterial::Leaves(mat), aabb });
    }
    (!parts.is_empty()).then_some(TreeVisual { parts, scale })
}

/// Vertex colour for a dimming brightness. The brightness is a display-space factor (the
/// original shades in display space [hypothesis]); this renderer lights in linear space,
/// where the same darkening is the factor to the power 2.2.
fn dim_colour(brightness: f32) -> [f32; 4] {
    let l = brightness.clamp(0.0, 1.0).powf(2.2);
    [l, l, l, 1.0]
}

/// `textures\trees\branches\<file name of the authored bark path, .dds>` (speedtree.md §6).
fn bark_texture(authored: &str) -> Option<String> {
    let name = authored.rsplit(['\\', '/']).next()?.trim();
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    (!stem.is_empty()).then(|| format!("textures\\trees\\branches\\{}.dds", stem.to_ascii_lowercase()))
}

fn bounds(points: impl Iterator<Item = [f32; 3]>, pad: f32) -> Aabb {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in points {
        lo = lo.min(Vec3::from_array(p));
        hi = hi.max(Vec3::from_array(p));
    }
    if lo.x > hi.x {
        (lo, hi) = (Vec3::ZERO, Vec3::ZERO);
    }
    Aabb::from_min_max(lo - pad, hi + pad)
}

/// Spawn a tree's parts under `parent` with the reference transform `rt` (game space,
/// relative to the parent). Returns the number of parts.
pub fn spawn(commands: &mut Commands, parent: Entity, v: &TreeVisual, rt: Mat4, extra: impl Bundle + Clone) -> usize {
    let t = Transform::from_matrix(rt * Mat4::from_scale(Vec3::splat(v.scale)));
    for p in &v.parts {
        let common = (Mesh3d(p.mesh.clone()), t, p.aabb, NoAutoAabb, extra.clone());
        match &p.material {
            PartMaterial::Bark(m) => commands.entity(parent).with_child((common, MeshMaterial3d(m.clone()))),
            // Leaf cards cast shadows (onto the ground and other objects) but receive
            // none: in the shadow pass they face the light, in the main pass the camera,
            // so a tree's own cards blacken its lower half in hard, mismatched blocks
            // [observed]. The crown darkens by the CNAM dimming instead (vertex colours).
            PartMaterial::Leaves(m) => {
                commands.entity(parent).with_child((common, MeshMaterial3d(m.clone()), bevy::light::NotShadowReceiver))
            }
        };
    }
    v.parts.len()
}

#[cfg(test)]
mod tests {
    #[test]
    fn wind_speeds() {
        let (rock, rustle) = super::angular_speeds(0.07, 0.35);
        let period = |w: f32| std::f32::consts::TAU / w;
        assert!((period(rock) - 4.0).abs() < 0.3, "{}", period(rock));
        assert!((period(rustle) - 1.0).abs() < 0.1, "{}", period(rustle));
        assert_eq!(super::angular_speeds(-1.0, -1.0), super::angular_speeds(0.0, 0.0));
    }

    #[test]
    fn bark_paths() {
        assert_eq!(
            super::bark_texture("C:\\Noah\\Fallout\\Trees\\WastelandShrub01\\\\WastelandShrub01Bark.tga").as_deref(),
            Some("textures\\trees\\branches\\wastelandshrub01bark.dds")
        );
        assert_eq!(super::bark_texture(""), None);
    }
}
