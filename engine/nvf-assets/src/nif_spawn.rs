//! NifAssetPlugin: spawn a NIF's scene graph as Bevy entities.
//!
//! Conventions [observed, docs/spec-notes/nif.md §7]: rotation matrices are stored row by
//! row and rotate column vectors; a child's world transform is parent ∘ child; triangles
//! are counter-clockwise when seen from the side their normals face; a bone's skin
//! transform is inverse(bone bind world) ∘ shape bind world, which is exactly the
//! inverse bind pose Bevy expects. Gamebryo is Z-up, Bevy Y-up: the root entity rotates
//! by −90° about X.

use crate::data::GameData;
use crate::nif_material::{display_emissive, emissive_in_display, nif_material, Falloff, NifMaterial};
use crate::texture::TextureCache;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::DynamicSkinnedMeshBounds;
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_nif::scene::{mesh_data, scene_objects, Decoded, SceneObject, Transform as NifTransform};
use fnv_nif::{NifFile, Struct, Value};
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

/// Marks the root of a spawned NIF.
#[derive(Component)]
pub struct NifRoot;

/// Set on objects whose NiAVObject flags have bit 0 set [hypothesis: "hidden"; editor
/// markers and collision helpers carry it].
#[derive(Component)]
pub struct NifHidden;

/// Summary of what was spawned, for the overlay and for framing the camera.
#[derive(Debug, Clone, Default)]
pub struct SpawnReport {
    pub blocks: usize,
    pub shapes: usize,
    pub skinned: usize,
    pub triangles: usize,
    pub textures_missing: Vec<String>,
    /// Bounds in Bevy world space (after the Z-up → Y-up rotation).
    pub bounds: Option<(Vec3, Vec3)>,
    /// Node name → entity, for animation targets.
    pub nodes: HashMap<String, Entity>,
    /// Local transform of each named node as spawned (its bind pose), so a node an
    /// animation took over can be put back.
    pub bind: HashMap<Entity, Transform>,
    pub root: Option<Entity>,
    /// Skeleton file the skinned shapes were bound to.
    pub skeleton: Option<String>,
}

#[derive(SystemParam)]
pub struct SpawnCtx<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<NifMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
    pub bindposes: ResMut<'w, Assets<SkinnedMeshInverseBindposes>>,
    pub textures: ResMut<'w, TextureCache>,
    pub data: Res<'w, GameData>,
    pub formats: Option<Res<'w, CompressedImageFormatSupport>>,
}

pub fn to_bevy(t: &NifTransform) -> Transform {
    let m = Mat3::from_cols_array(&t.rotation).transpose(); // stored row-major
    Transform { translation: Vec3::from_array(t.translation), rotation: Quat::from_mat3(&m).normalize(), scale: Vec3::splat(t.scale) }
}

pub(crate) fn affine(t: &NifTransform) -> Affine3A {
    let m = Mat3::from_cols_array(&t.rotation).transpose();
    Affine3A::from_mat3_translation(m * t.scale, Vec3::from_array(t.translation))
}

impl Props {
    /// Name the first-person form of each texture that has one
    /// (`crate::texture::first_person_texture`); the number named anew.
    pub(crate) fn use_first_person_textures(&mut self, exists: impl Fn(&str) -> bool) -> usize {
        let mut n = 0;
        for t in self.textures.iter_mut().chain(self.glow.as_mut()) {
            if let Some(first) = crate::texture::first_person_texture(t, &exists) {
                *t = first;
                n += 1;
            }
        }
        n
    }

    /// How many of the shape's textures (glow map included) are in their first-person form.
    pub(crate) fn first_person_texture_count(&self) -> usize {
        self.textures.iter().chain(self.glow.as_ref()).filter(|t| crate::texture::is_first_person_texture(t)).count()
    }

    /// Put in a texture set's files (a record's alternate textures, `fnv_world::alt_textures`;
    /// docs/spec-notes/texture-swaps.md §3), given in NIF texture-set slot order as the
    /// TXST stores them: each given slot replaces the shape's own, an empty one keeps it
    /// [hypothesis]. The glow map follows slot 2 unless the shape uses slot 2 for skin or
    /// hair, as when the file names it (`glow_map`). The number of slots replaced.
    pub(crate) fn use_texture_set(&mut self, set: &[Option<String>; 6]) -> usize {
        let mut n = 0;
        for (slot, t) in set.iter().enumerate() {
            let Some(t) = t.as_deref().filter(|t| !t.is_empty()) else { continue };
            if self.textures.len() <= slot {
                self.textures.resize(slot + 1, String::new());
            }
            self.textures[slot] = fnv_vfs::texture_path(t);
            if slot == 2 && !self.hair && !self.skin && !self.unlit {
                self.glow = Some(self.textures[2].clone());
            }
            n += 1;
        }
        n
    }
}

/// Rendering-relevant properties accumulated from an object and its ancestors.
#[derive(Debug, Clone, Default)]
pub(crate) struct Props {
    pub(crate) textures: Vec<String>,
    alpha: Option<(i64, i64)>,
    pub(crate) material_alpha: Option<f32>,
    emissive: Option<[f32; 3]>,
    emissive_mult: Option<f32>,
    /// Glow map: texture-set slot 2 of a BSShaderPPLightingProperty without the face-gen
    /// or hair flag (docs/spec-notes/nif.md §6, "Emissive light").
    glow: Option<String>,
    glossiness: Option<f32>,
    /// BSShaderFlags bit 0 "Specular" of the shader property [documented: nif.xml].
    specular: bool,
    /// BSShaderFlags bit 3 "Vertex_Alpha" [documented: nif.xml]: vertex colour alpha is
    /// used only with it (`fix_vertex_colors`).
    vertex_alpha: bool,
    double_sided: bool,
    vertex_colors_tint: bool,
    /// BSShaderNoLightingProperty: drawn without lighting (effects, glows).
    unlit: bool,
    /// The texture came from a BSShaderNoLightingProperty's file name: an
    /// NiTexturingProperty after it keeps it.
    shader_texture: bool,
    /// BSShaderFlags bit 19 "Dynamic_Alpha" [documented: nif.xml] on a
    /// BSShaderNoLightingProperty ([`shape_alpha_mode`]).
    dynamic_alpha: bool,
    /// BSShaderFlags bit 18 "Hair" [documented: nif.xml].
    pub(crate) hair: bool,
    /// The shader property's type is [`SHADER_SKIN`]: the shape is an actor's skin.
    pub(crate) skin: bool,
    /// The view-angle falloff of a BSShaderNoLightingProperty that switches it on
    /// ([`property_falloff`]).
    falloff: Option<Falloff>,
    /// BSShaderFlags bit 29 "External_Emittance" [documented: nif.xml].
    external_emittance: bool,
    /// A decal shader flag is set ([`is_decal`]).
    decal: bool,
    /// The Refraction shader flag is set ([`refracts`]): the stored refraction strength.
    refraction: Option<f32>,
    /// The Fire Refraction shader flag is set too ([`fire_period`]): seconds for the
    /// normal map to scroll once, else 0.
    fire_period: f32,
    /// BSShaderFlags2 bit 2 "LOD_Building" [documented: nif.xml]: object LOD block
    /// geometry, cut out by its atlas's alpha (`shape_alpha_mode`).
    lod_building: bool,
    /// A sequence of the model drives the shape's emissive colour or multiplier
    /// (`crate::material_anim`): its glow map is loaded even when the stored colour is
    /// black, as the radios' dials store black and light up by their "Open" sequence
    /// (docs/spec-notes/activators.md §2).
    pub(crate) animated_emissive: bool,
    /// A clip of the model drives the shape's material alpha ([`shape_alpha_mode`]).
    pub(crate) animated_alpha: bool,
    /// An actor's part whose material carries an NiAlphaController for the actor's
    /// clips to drive ([`shape_alpha_mode`], [`material_alpha_controlled`]).
    pub(crate) alpha_gated: bool,
    /// Some property is attached (to the shape or an ancestor).
    has_properties: bool,
    /// One of them derives from BSShaderProperty.
    shader: bool,
}

impl Props {
    /// Helper geometry the game does not draw: properties attached, but neither a
    /// shader property nor a texture. These are bone display boxes (`TTail1:0`,
    /// `Bone01:0`), collision proxies (`c_Minigun`, `DummyCol:3`) and invisible markers
    /// (`##PLRInvis:0`): 434 of 48,996 visible shapes in `Fallout - Meshes.bsa`, all with
    /// only a NiMaterialProperty [observed: `crates/fnv-nif/examples/shape_survey.rs`;
    /// hypothesis: the game skips them, consistent with the in-game look of the
    /// Goodsprings flag]. Shapes with no properties at all (landscape LOD) are drawn.
    pub(crate) fn external_emittance(&self) -> bool {
        self.external_emittance
    }

    /// The stored emissive colour (black when none) and multiplier (1 when none), held
    /// for unlit pictures by [`shape_emissive_mult`] [hypothesis, nif.md §6
    /// "Unlit shapes that are pictures, and cut-out signs"].
    pub(crate) fn emissive_parts(&self) -> ([f32; 3], f32) {
        (self.emissive.unwrap_or([0.0; 3]), self.emissive_mult.unwrap_or(1.0).max(0.0))
    }

    /// The stored refraction strength (0 when the shape does not refract).
    pub(crate) fn refraction_part(&self) -> f32 {
        self.refraction.unwrap_or(0.0)
    }

    /// The stored material alpha (1 when none).
    pub(crate) fn alpha_part(&self) -> f32 {
        self.material_alpha.unwrap_or(1.0)
    }

    pub(crate) fn is_helper(&self) -> bool {
        self.has_properties && !self.shader && self.textures.is_empty()
    }
}

/// BSShaderType 14 "SHADER_SKIN" [documented: nif.xml BSShaderType]. In
/// `Fallout - Meshes.bsa` the 255 visible shapes of this type are exactly those with the
/// "FaceGen" shader flag (bit 10), and all but 2 (which name no texture) name a head,
/// upper body or hand texture under `textures\characters\` [observed:
/// `crates/fnv-nif/examples/skin_survey.rs`, docs/spec-notes/actors.md §3.1].
pub(crate) const SHADER_SKIN: i64 = 14;

pub(crate) fn text_list(s: &Struct, name: &str) -> Vec<String> {
    s.list(name)
        .unwrap_or_default()
        .iter()
        .map(|v| match v {
            Value::Str { text: Some(t), .. } => t.clone(),
            _ => String::new(),
        })
        .collect()
}

/// The glow map of a BSShaderPPLightingProperty: texture-set slot 2, unless the face-gen
/// (bit 10) or hair (bit 18) shader flag is set [documented: nif.xml BSShaderFlags,
/// BSShaderTextureSet], in which case slot 2 holds skin or hair maps.
fn glow_map(shader_flags: i64, textures: &[String]) -> Option<String> {
    if shader_flags & (1 << 10 | 1 << 18) != 0 {
        return None;
    }
    textures.get(2).filter(|t| !t.is_empty()).cloned()
}

/// The shape is a decal: BSShaderFlags bit 26 "Decal_Single_Pass" or bit 27
/// "Dynamic_Decal_Single_Pass" [documented: nif.xml BSShaderFlags]. 2,130 visible shapes
/// in `Fallout - Meshes.bsa` set one (41 only bit 26, none only bit 27): dirt, stains,
/// cracks, markings and insignia laid 0.05 to 1 unit in front of another shape of their
/// file, a few exactly in its plane [observed: `crates/fnv-nif/examples/decal_survey.rs`,
/// docs/spec-notes/nif.md §6 "Decal flags"]. They are drawn with a depth offset towards
/// the camera (`nif_material::decal_bias`) [hypothesis].
pub(crate) fn is_decal(shader_flags: i64) -> bool {
    shader_flags & (1 << 26 | 1 << 27) != 0
}

/// The shape bends what lies behind it instead of being drawn: BSShaderFlags bit 15
/// "Refraction" [documented: nif.xml BSShaderFlags; docs/spec-notes/explosion-effects.md
/// §7]. 31 of the 257 models the weapon effect records name set it, all on a
/// BSShaderPPLightingProperty (the heat-haze shells of explosions), as do Mister Handy's
/// jet clouds, whose diffuse texture is plain white (`FXWHITE.dds`) [observed]; drawn as
/// ordinary shapes they show as solid grey or white domes [observed: the Incinerator's
/// `dlc03flamerexplosion.nif`, owner 2026-10-20].
pub(crate) fn refracts(shader_flags: i64) -> bool {
    shader_flags & (1 << 15) != 0
}

/// Seconds for a refracting shape's normal map to scroll once up its texture: with
/// BSShaderFlags bit 16 "Fire_Refraction" [documented: nif.xml BSShaderFlags] the
/// BSShaderPPLightingProperty's "Refraction Fire Period" [documented: nif.xml; the GECK
/// wiki's NIF controller page names `BSRefractionFirePeriodController` as animating it],
/// read as tenths of a second, a negative period scrolling down instead of up
/// [hypothesis, docs/spec-notes/refraction.md §3]; 0 without the flag or with a period
/// of 0.
pub(crate) fn fire_period(shader_flags: i64, period: Option<i64>) -> f32 {
    match period {
        Some(p) if p != 0 && shader_flags & (1 << 16) != 0 => p as f32 * FIRE_PERIOD_UNIT,
        _ => 0.0,
    }
}

/// Seconds per unit of the stored fire period [hypothesis, docs/spec-notes/refraction.md §3].
pub const FIRE_PERIOD_UNIT: f32 = 0.1;

/// BSShaderFlags bit 6 switches the view-angle falloff of a BSShaderNoLightingProperty on
/// [hypothesis, docs/spec-notes/nif.md §6 "View-angle falloff". nif.xml names the bit
/// "Empty" for this game and "Use_Falloff" ("Use Falloff value in EffectShaderProperty")
/// in the Skyrim and Fallout 4 flag sets, which keep this game's names at bits 0, 1, 3, 7,
/// 15 to 17, 29 and 31 [documented: nif.xml SkyrimShaderPropertyFlags1,
/// Fallout4ShaderPropertyFlags1]. In `Fallout - Meshes.bsa` 1,013 of the 1,193 visible
/// shapes with falloff values of their own set it and 95 of the 4,495 with the nif.xml
/// defaults; the Prospector Saloon lamp beam sets it [observed:
/// `crates/fnv-nif/examples/falloff_survey.rs`]. No game frame of a shape without the
/// bit was compared].
pub(crate) fn uses_falloff(shader_flags: i64) -> bool {
    shader_flags & (1 << 6) != 0
}

/// Whether a shader property's shapes take their vertex alpha: with BSShaderFlags bit 3
/// "Vertex_Alpha", and on an unlit shape (BSShaderNoLightingProperty) also with bit 19
/// "Dynamic_Alpha" [documented: nif.xml flag names; hypothesis for the second: the
/// dust vortex sheets of `effects\nv\sanddust\sanddust02.nif` set bit 19, not bit 3,
/// and their vertex alpha is 0 along the sheets' edges and 1 inside; without it they drew
/// as a dense sheet with hard vertical edges where the game shows a soft column of dust
/// (owner, playtest list 5 item 1; docs/spec-notes/nif.md §6 "Vertex colours")].
pub(crate) fn uses_vertex_alpha(property_type: &str, shader_flags: i64) -> bool {
    shader_flags & (1 << 3) != 0 || (property_type == "BSShaderNoLightingProperty" && shader_flags & (1 << 19) != 0)
}

/// The view-angle falloff of a BSShaderNoLightingProperty with these shader flags and
/// falloff fields (start angle, stop angle, start opacity, stop opacity; absent in files
/// of stream version 26 or lower): none without the flag ([`uses_falloff`]), whatever
/// the fields hold. Without the flag the fields are mostly the defaults, opacity 1 → 0
/// over cosine 1 → 0, which applied would fade every such shape by the cosine of the
/// view angle: neon tubes, lit sign letters, the shadows under playing cards.
pub(crate) fn property_falloff(shader_flags: i64, fields: [Option<f32>; 4]) -> Option<Falloff> {
    let [Some(a), Some(b), Some(c), Some(d)] = fields else { return None };
    uses_falloff(shader_flags).then(|| Falloff::from_fields(a, b, c, d)).flatten()
}

fn apply_property(file: &NifFile, dec: &Decoded, r: i32, p: &mut Props) {
    let Some(s) = dec.get(r) else { return };
    let ty = file.blocks[r as usize].type_name.as_str();
    p.has_properties = true;
    let shader = dec.is_a(r as usize, "BSShaderProperty");
    p.shader |= shader;
    if let Some(ts) = s.block_ref("texture_set").and_then(|t| dec.get(t)) {
        p.textures = text_list(ts, "textures");
    }
    if shader || ty == "NiTexturingProperty" {
        p.glow = None;
    }
    if shader {
        let flags = s.int("shader_flags").unwrap_or(0);
        p.specular = flags & 1 != 0;
        p.vertex_alpha = uses_vertex_alpha(ty, flags);
        p.dynamic_alpha = ty == "BSShaderNoLightingProperty" && flags & (1 << 19) != 0;
        p.hair = flags & (1 << 18) != 0;
        p.skin = s.int("shader_type") == Some(SHADER_SKIN);
        p.external_emittance = flags & (1 << 29) != 0;
        p.decal = is_decal(flags);
        p.refraction = refracts(flags).then(|| s.float("refraction_strength").unwrap_or(0.0));
        p.fire_period = fire_period(flags, s.int("refraction_fire_period"));
        p.lod_building = s.int("shader_flags_2").unwrap_or(0) & (1 << 2) != 0;
        p.falloff = None;
    }
    match ty {
        "BSShaderPPLightingProperty" => p.glow = glow_map(s.int("shader_flags").unwrap_or(0), &p.textures),
        "BSShaderNoLightingProperty" => {
            p.unlit = true;
            if let Some(f) = s.text("file_name") {
                p.textures = vec![f.to_string()];
                p.shader_texture = !f.is_empty();
            }
            let fields = ["falloff_start_angle", "falloff_stop_angle", "falloff_start_opacity", "falloff_stop_opacity"];
            p.falloff = property_falloff(s.int("shader_flags").unwrap_or(0), fields.map(|n| s.float(n)));
        }
        // The unlit shader's own texture wins over the base texture of an NiTexturingProperty
        // listed after it, which then only adds its texture transform [hypothesis: in
        // 3,783 shapes of Fallout - Meshes.bsa and Update.bsa holding both, two differ:
        // the jukebox's light tubes, whose shader names `JBLightTubesClean.dds` (bright
        // cyan in the owner's frames of 2026-10-27) and texturing `jblighttubesdirty.dds`,
        // and a REPCONN mist layer].
        "NiTexturingProperty" if p.shader_texture => {}
        "NiTexturingProperty" => {
            let src = s.strukt("base_texture").and_then(|b| b.block_ref("source")).and_then(|r| dec.get(r));
            if let Some(f) = src.and_then(|t| t.text("file_name")) {
                p.textures = vec![f.to_string()];
            }
        }
        "NiAlphaProperty" => p.alpha = Some((s.int("flags").unwrap_or(0), s.int("threshold").unwrap_or(128))),
        "NiMaterialProperty" => {
            p.material_alpha = s.float("alpha");
            p.emissive = s.floats("emissive_color").and_then(|e| e.try_into().ok());
            p.emissive_mult = s.float("emissive_mult");
            p.glossiness = s.float("glossiness");
        }
        "NiStencilProperty" => {
            // 20.1.0.3+: draw mode in flags bits 10-11; older files: draw_mode field.
            let mode = s.int("draw_mode").or_else(|| s.int("flags").map(|f| (f >> 10) & 3)).unwrap_or(0);
            p.double_sided = mode == 3;
        }
        "NiVertexColorProperty" => p.vertex_colors_tint = true,
        _ => {}
    }
}

/// Editor-only geometry: nodes named `EditorMarker...` [documented: nif.xml BSXFlags bit
/// 5 "EditorMarkers present"; observed: 327 meshes in Fallout - Meshes.bsa, 155 of them
/// effects, mostly without the hidden flag; hypothesis: the game never draws them].
pub fn is_editor_marker(name: Option<&str>) -> bool {
    name.is_some_and(|n| n.len() >= 12 && n[..12].eq_ignore_ascii_case("editormarker"))
}

/// Blend mode for NiAlphaProperty flags [documented: nif.xml AlphaFlags: bit 0 blend,
/// bits 1-4 source and 5-8 destination factor (AlphaFunction: 0 ONE, 1 ZERO, 2 SRC_COLOR,
/// 4 DEST_COLOR, 6 SRC_ALPHA, 7 INV_SRC_ALPHA, ...), bit 9 alpha test]. Bevy offers fixed
/// modes, so factors map to the nearest: destination ONE is additive, a source of ZERO
/// or DEST_COLOR multiplies.
///
/// When both test and blend are set, a threshold of [`BLEND_TEST_CUT`] or more draws
/// alpha-tested (the test cuts the shape: weapon parts), a lower one blended (the test only
/// trims nearly transparent texels: dirty glass) [hypothesis, from
/// `crates/fnv-nif/examples/alpha_survey.rs`, docs/spec-notes/nif.md §6 "Alpha"].
/// Additive stays additive. Tested first, the dirty glass of the NV poster marquees
/// (`clutter/signnv/nvposter01.nif`, flags 0x12ED, threshold 0) hid the poster behind
/// an opaque grey panel [observed].
///
/// The test compares alpha with the threshold by bits 10-12 (TestFunction: 4 GREATER, 6
/// GREATER_EQUAL, ...) [documented: nif.xml AlphaFlags]. Bevy's mask keeps alpha ≥ cutoff,
/// so GREATER moves the cutoff half a step above the threshold; other functions keep ≥.
pub(crate) const BLEND_TEST_CUT: i64 = 64;

pub(crate) fn alpha_mode(flags: i64, threshold: i64) -> AlphaMode {
    let (src, dst) = ((flags >> 1) & 0xF, (flags >> 5) & 0xF);
    let (blend, test) = (flags & 1 != 0, flags & 0x200 != 0);
    let strict = (flags >> 10) & 7 == 4;
    let cutoff = (threshold as f32 + if strict { 0.5 } else { 0.0 }) / 255.0;
    match (blend, test, src, dst) {
        (true, _, _, 0) => AlphaMode::Add,
        (false, true, _, _) => AlphaMode::Mask(cutoff),
        (true, true, _, _) if threshold >= BLEND_TEST_CUT => AlphaMode::Mask(cutoff),
        (true, _, 1 | 4, _) => AlphaMode::Multiply,
        (true, ..) => AlphaMode::Blend,
        _ => AlphaMode::Opaque,
    }
}

/// Whether a shape is additive with source factor ONE (NiAlphaProperty blend on, source
/// and destination blend modes 0, ONE [documented: nif.xml AlphaFlags]): 182 shapes in
/// `Fallout - Meshes.bsa` (docs/spec-notes/nif.md §6 "Additive blend"), among them the
/// Securitron's screen noise (effects-animation-audit.md §2).
pub(crate) fn adds_whole(props: &Props) -> bool {
    props.alpha.is_some_and(|(flags, _)| flags & 1 != 0 && (flags >> 1) & 0xF == 0 && (flags >> 5) & 0xF == 0)
}

/// Alpha cutoff for object LOD geometry: alpha above 128 is drawn [hypothesis: the game's
/// value is not known; 128 with TEST_GREATER is the most common setting of the
/// alpha-tested `*_lod.nif` meshes, 45 of 128 shapes, and `watertower01_lod.nif` has 127;
/// observed: `crates/fnv-nif/examples/lod_alpha_survey.rs`].
pub(crate) const LOD_BUILDING_CUT: f32 = 128.5 / 255.0;

/// How a shape's alpha is drawn: by its NiAlphaProperty when it has one. Without one,
/// shapes whose shader sets "LOD_Building" are alpha-tested at [`LOD_BUILDING_CUT`]
/// [hypothesis, docs/spec-notes/world.md §7 "Objects": all 355 object LOD block shapes
/// in `Fallout - Meshes.bsa` set the flag (no other shape does) and none has a
/// NiAlphaProperty, yet their atlases carry cut-outs in alpha (trees, pylons, the water
/// tower's legs: 14% of `WastelandNV.Buildings.dds` is alpha 0, near black in colour);
/// drawn opaque those were black boxes on the hills around Goodsprings]. An unlit shape
/// whose shader sets "Dynamic_Alpha" (bit 19) and whose alpha a clip of its model drives
/// is blended by its alpha [hypothesis,
/// docs/spec-notes/explosion-effects.md §9: of the 257 weapon effect models only two
/// shapes have no NiAlphaProperty and an unlit shader with the flag, the Fat Man
/// explosion's mushroom-cloud card `SmokeGeo:0` (`explosionlarge01.nif`) and `WhiteBG:0`
/// of `explosionincendiarygrenade.nif`, and both are faded in and out by alpha tracks
/// from a stored alpha of 0; drawn opaque the mushroom stood whole for 5 s (owner,
/// 2026-10-20). In the base mesh archives 29 more shapes have such a shader and no
/// NiAlphaProperty (Lucky 38's light glows, terminal screens), whose alpha no clip fades;
/// they stay as they were checked, opaque]. Such a shape of an actor's part whose
/// material carries an NiAlphaController, the target of the actor's own `.kf` clips
/// ([`material_alpha_controlled`]), is drawn opaque where its material alpha is 1/2 or
/// more and not at all below [observed: in the actor folders of all the archives, 9
/// such unlit shapes, all Mr. House's three stacked screens in the three
/// `nvpenthouemaincomputer` screen files, each with an alpha controller and an opaque
/// texture; `crates/fnv-nif/examples/actor_alpha_survey.rs`. They lie a few units apart,
/// Yes Man's face in front, House's behind it, Connection Lost at the back, and the
/// clips set their alpha to 0 or 1: `mtidle.kf` shows Connection Lost alone,
/// `DynamicIdle_MrHouse.kf` House alone, `DynamicIdle_YesMan.kf` Yes Man over Connection
/// Lost. Drawn opaque whatever the alpha, Yes Man's face covered House's before Yes Man
/// was uploaded (#1542). Hypothesis: the original skips geometry at material alpha 0 and
/// draws the rest by depth, as the clips' alphas of 0 and 1 suggest; blending is not
/// needed for them]. Everything else without the property is opaque.
pub(crate) fn shape_alpha_mode(props: &Props) -> AlphaMode {
    match props.alpha {
        Some((flags, threshold)) => alpha_mode(flags, threshold),
        None if props.lod_building => AlphaMode::Mask(LOD_BUILDING_CUT),
        None if props.unlit && props.dynamic_alpha && props.animated_alpha => AlphaMode::Blend,
        None if props.unlit && props.dynamic_alpha && props.alpha_gated => AlphaMode::Mask(0.5),
        None => AlphaMode::Opaque,
    }
}

/// Whether the NiMaterialProperty that shape block `shape` lists carries an
/// NiAlphaController in its controller chain: the material alpha a `.kf` clip can drive
/// (a sequence's controlled block names the shape and the property type, and drives the
/// controller of that type on it [documented: nif.xml ControlledBlock "Node Name",
/// "Property Type", "Controller Type"]).
pub(crate) fn material_alpha_controlled(file: &NifFile, dec: &Decoded, shape: usize) -> bool {
    let Some(s) = dec.blocks.get(shape).and_then(|b| b.as_ref().ok()) else { return false };
    s.refs("properties").unwrap_or_default().iter().any(|&r| {
        if !file.blocks.get(r as usize).is_some_and(|p| p.type_name == "NiMaterialProperty") {
            return false;
        }
        let mut c = dec.get(r).and_then(|m| m.block_ref("controller"));
        for _ in 0..64 {
            let Some(cr) = c.filter(|&x| x >= 0) else { return false };
            if file.blocks.get(cr as usize).is_some_and(|b| b.type_name == "NiAlphaController") {
                return true;
            }
            c = dec.get(cr).and_then(|b| b.block_ref("next_controller"));
        }
        false
    })
}

/// The cut of a shape that is drawn blended although its alpha test would make it an
/// alpha-tested one ([`alpha_mode`]: blend and test both set, threshold of
/// [`BLEND_TEST_CUT`] or more), or `None` for every other shape: its cutoff, when its
/// shader uses vertex alpha and some vertex's alpha is below 1. Such a shape's alpha is a
/// gradient across its triangles, not the outline of a texture: drawn alpha-tested, the
/// part above the cutoff would be opaque where the shape asks for a part of its colour.
/// So it blends, and the fragments below the cutoff are dropped, which is the reading of
/// "blend and test" itself [documented: nif.xml AlphaFlags; hypothesis that the game
/// tests the vertex alpha: docs/spec-notes/nif.md §6 "Alpha", "Vertex alpha and the
/// test"]. `mesh` must have been through [`fix_vertex_colors`].
pub(crate) fn blend_cut(props: &Props, mesh: &Mesh) -> Option<f32> {
    let (flags, threshold) = props.alpha?;
    if flags & 1 == 0 || !props.vertex_alpha {
        return None;
    }
    let AlphaMode::Mask(cutoff) = alpha_mode(flags, threshold) else { return None };
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else { return None };
    colors.iter().any(|c| c[3] < 1.0).then_some(cutoff)
}

/// Properties of a shape: its ancestors' first, then its own.
pub(crate) fn shape_props(file: &NifFile, dec: &Decoded, o: &SceneObject, by_block: &HashMap<usize, &SceneObject>) -> Props {
    let mut chain = vec![o.block];
    let mut cur = o.parent;
    while let Some(p) = cur {
        if chain.len() > 512 {
            break;
        }
        chain.push(p);
        cur = by_block.get(&p).and_then(|x| x.parent);
    }
    let mut props = Props::default();
    for b in chain.iter().rev() {
        if let Some(refs) = dec.blocks[*b].as_ref().ok().and_then(|s| s.refs("properties")) {
            for &r in refs {
                apply_property(file, dec, r, &mut props);
            }
        }
    }
    props
}

/// Perceptual roughness for a NiMaterialProperty glossiness, read as a Blinn-Phong
/// exponent n: Beckmann slope m = sqrt(2 / (n + 2)) [documented: Walter et al. 2007,
/// "Microfacet Models for Refraction through Rough Surfaces"], used as the GGX alpha,
/// and Bevy's perceptual roughness is sqrt(alpha) [hypothesis: the game's highlight
/// size follows glossiness as a Phong exponent].
pub fn gloss_roughness(gloss: f32) -> f32 {
    (2.0 / (gloss.max(0.0) + 2.0)).sqrt().sqrt()
}

/// Clean a shape's vertex colours: non-finite values become 1 and the rest are clamped
/// to 0..1, and the alpha is 1 unless the shader property sets the vertex-alpha flag
/// [hypothesis, docs/spec-notes/nif.md §6 "Vertex colours": of 3,032 visible shapes with
/// some vertex alpha below 1, 2,445 set the flag, while 331 of the 343 shapes with
/// non-finite or out-of-range values do not (Victor's shack trim: NaN, −3e38 and denormal
/// alphas on 709 of 1,173 vertices)].
pub(crate) fn fix_vertex_colors(mesh: &mut Mesh, vertex_alpha: bool) {
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) else { return };
    let clean = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 1.0 };
    for c in colors.iter_mut() {
        *c = [clean(c[0]), clean(c[1]), clean(c[2]), if vertex_alpha { clean(c[3]) } else { 1.0 }];
    }
}

/// The vertex colours of an unlit shape (BSShaderNoLightingProperty) taken as display
/// values: their colour is converted to linear, the alpha kept.
///
/// An unlit shape's colour is texture × vertex colour, which the game forms in display
/// values [hypothesis, as for the landscape's vertex colours (terrain.rs) and the grass
/// (grass.rs)]; this renderer multiplies in linear values, so a stored 0.57 grey drew as
/// a display 0.78. The smoke meshes are tinted so: the ground smoke at the camp fires
/// (`dlcpitt/effects/dlcpittfxgroundsmoke01.nif`, vertex colour (0.57, 0.54, 0.51), white
/// texture) is a grey near the desert floor's in the game, where it can barely be seen
/// (owner's screenshots, 2026-10-12), and drew as a white plume here
/// (docs/spec-notes/nif.md §6 "Vertex colours").
pub(crate) fn unlit_vertex_colors(mesh: &mut Mesh) {
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) else { return };
    for c in colors.iter_mut() {
        for v in &mut c[..3] {
            *v = Color::srgb(*v, 0.0, 0.0).to_linear().red;
        }
    }
}

/// Reflectance for a shape, and whether its normal map's alpha (the specular mask,
/// docs/spec-notes/nif.md §6 "Specular") scales it. Bevy maps mask alpha a to
/// reflectance × a × 0.5, so 2.0 lets the mask span the full [0, 1] range: a mask of
/// 0.5 gives the usual dielectric F0 of 0.04 [hypothesis]. Shaders without the specular
/// flag get none; shapes without a normal map keep the earlier constant. Hair-shader
/// shapes (hair, brows, beards) get none either [hypothesis, nif.md §6 "Hair": read as a
/// plain mask their normal-map alpha gave glinting white strands the game does not show].
///
/// A normal map without an alpha channel (DXT1: `unmasked`) holds no mask; read as one,
/// its alpha of 1 is the strongest mask there is, seven times the median authored one,
/// and made Doc Mitchell's bed, the Goodsprings gas station's vending machine and door
/// shine like wet paint (owner's playtest, 2026-10-11). Such a shape gets
/// [`UNMASKED_SPECULAR`] instead [hypothesis, nif.md §6 "Specular"].
pub(crate) fn specular(props: &Props, has_normal_map: bool, unmasked: bool) -> (f32, bool) {
    if props.hair {
        return (0.0, false);
    }
    match (has_normal_map, props.specular) {
        (true, true) if unmasked => (UNMASKED_SPECULAR, false),
        (true, true) => (2.0, true),
        (true, false) => (0.0, false),
        (false, _) => (0.3, false),
    }
}

/// The reflectance of a shape whose normal map has no alpha channel: the median mask of
/// the normal maps that have one, 35 of 255, as the masked path reads it (reflectance
/// 2.0 × 0.5 × mask) [observed: `crates/fnv-nif/examples/normal_mask_survey.rs` over
/// `Fallout - Textures.bsa` and `Textures2.bsa`, 2,998 masked normal maps, quartiles of
/// their mean alpha 21 / 35 / 58; the choice of the median is a hypothesis].
pub const UNMASKED_SPECULAR: f32 = 35.0 / 255.0;

/// Whether a loaded normal map has no alpha channel to hold a specular mask (DXT1).
pub fn lacks_alpha(image: &Image) -> bool {
    format_lacks_alpha(image.texture_descriptor.format)
}

/// [`lacks_alpha`] by the texture's format.
pub fn format_lacks_alpha(format: bevy::render::render_resource::TextureFormat) -> bool {
    use bevy::render::render_resource::TextureFormat;
    matches!(format, TextureFormat::Bc1RgbaUnorm | TextureFormat::Bc1RgbaUnormSrgb)
}

/// The falloff to draw a shape's material with. Falloff scales alpha, so only blended
/// materials use it [hypothesis: on alpha-tested or opaque shapes it would only cut holes;
/// 15 of the 1,108 visible shapes in `Fallout - Meshes.bsa` that switch the falloff on
/// with a range to fade over are not blended; observed:
/// `crates/fnv-nif/examples/falloff_survey.rs`, docs/spec-notes/nif.md §6].
pub(crate) fn shape_falloff(props: &Props, alpha_mode: AlphaMode) -> Option<Falloff> {
    props.falloff.filter(|_| matches!(alpha_mode, AlphaMode::Blend | AlphaMode::Add | AlphaMode::Multiply | AlphaMode::Premultiplied))
}

/// Whether a shape's emitted light is over white: emissive colour × multiplier over 1 in
/// its brightest channel (neon, glowing signs; [`crate::nif_material::soft_intersections`]).
pub(crate) fn burns(props: &Props) -> bool {
    let (e, m) = props.emissive_parts();
    e.iter().fold(0.0f32, |a, &c| a.max(c)) * m > 1.0
}

/// An opaque unlit surface without External Emittance is a picture, including House's
/// authored opaque screen drawn with an actor alpha gate (`shape_alpha_mode`): its colour
/// times its multiplier is held to 1, so a Securitron's face (grey 0.05 x 30) and House's
/// screen show their texture and no more. Blended and additive effects and authored
/// cut-outs keep their authored multipliers, and a material track's keys multiply the
/// stored multiplier, as authored (`Props::emissive_parts`) [hypothesis, fitted to the
/// original's frames of 2026-11-06; nif.md section 6 "Unlit shapes that are pictures, and
/// cut-out signs"].
pub(crate) fn shape_emissive_mult(props: &Props, alpha_mode: AlphaMode) -> f32 {
    let mult = props.emissive_mult.unwrap_or(1.0).max(0.0);
    let opaque = matches!(alpha_mode, AlphaMode::Opaque)
        || (props.alpha.is_none() && props.alpha_gated && !props.lod_building && matches!(alpha_mode, AlphaMode::Mask(_)));
    if props.unlit && !props.external_emittance && opaque {
        let top = props.emissive.map_or(1.0, |e| e.iter().copied().fold(0.0f32, f32::max));
        mult.min(1.0 / top.max(f32::EPSILON))
    } else {
        mult
    }
}

/// Unlit shapes without External Emittance skip the image-space scale (x). An authored
/// cut-out sign skips it too, and takes the plain path of blended shapes rather than
/// `hot_light` (y), so its texture keeps its letters and it stays under the bright pass's
/// threshold [hypothesis, fitted to the same frames and nif.md's section].
pub(crate) fn shape_emissive_rule(props: &Props, alpha_mode: AlphaMode) -> Vec4 {
    if !props.unlit {
        return Vec4::ZERO;
    }
    let cutout = props.alpha.is_some() && matches!(alpha_mode, AlphaMode::Mask(_));
    Vec4::new(if !props.external_emittance || cutout { 1.0 } else { 0.0 }, if cutout { 1.0 } else { 0.0 }, 0.0, 0.0)
}

/// Emitted light of a shape: emissive colour × emissive multiplier (× the glow map and the
/// surface colour in the shader) [hypothesis, docs/spec-notes/nif.md §6 "Emissive light"],
/// `None` when the colour is black. An unlit shape's value is [`unlit_emissive`]. A
/// multiply-blended or additive shape's light is part of what it does to the frame behind
/// it, which is formed in display values, so its brightness is raised to the display gamma
/// ([`display_emissive`], nif.md §6 "Multiply blend", "Additive blend").
pub(crate) fn shape_emissive(props: &Props, alpha_mode: AlphaMode) -> Option<LinearRgba> {
    let e = props.emissive.filter(|e| e.iter().any(|&c| c > 0.0))?;
    let m = shape_emissive_mult(props, alpha_mode);
    if props.unlit && !matches!(alpha_mode, AlphaMode::Multiply) {
        let mut tint = unlit_emissive(e, m);
        if emissive_in_display(alpha_mode) {
            // The hue is already a display colour raised to the gamma; the brightness
            // (max(1, m x brightest channel)) is raised too.
            let top = e.iter().copied().fold(0.0f32, f32::max);
            let scale = (m * top).max(1.0);
            tint = tint.map(|c| c * scale.powf(crate::nif_material::MULTIPLY_GAMMA - 1.0));
        }
        let [r, g, b] = tint;
        return Some(LinearRgba::rgb(r, g, b));
    }
    let value = |c: f32| if emissive_in_display(alpha_mode) { display_emissive(c * m) } else { c * m };
    Some(LinearRgba::rgb(value(e[0]), value(e[1]), value(e[2])))
}

/// The factor on an unlit shape's surface colour from its emissive colour `e` and
/// multiplier `m` (nif.md §6 "Emissive colour of unlit shapes"): the colour's hue (the
/// colour over its brightest channel, a display colour, so raised to the display gamma)
/// times max(1, m × brightest channel). A warm or cyan colour tints a white glow texture;
/// a grey colour changes nothing below 1 (Lucky 38's base lights store grey 0.1 × 1.25
/// and are bright in the original); neon at white × 25 burns 25 times its texture
/// [hypothesis from the original's frames].
pub fn unlit_emissive(e: [f32; 3], m: f32) -> [f32; 3] {
    let top = e.iter().copied().fold(0.0f32, f32::max);
    if top <= 0.0 {
        return [1.0; 3];
    }
    let scale = (m * top).max(1.0);
    e.map(|c| (c.max(0.0) / top).powf(crate::nif_material::MULTIPLY_GAMMA) * scale)
}

/// The largest side of a mesh's bounding box in its own units (0 without positions).
fn mesh_extent(mesh: &Mesh) -> f32 {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(ps)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return 0.0 };
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in ps {
        let p = Vec3::from_array(*p);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    if ps.is_empty() { 0.0 } else { (hi - lo).max_element() }
}

/// Where a shape's material takes its textures from: the texture cache and image store on
/// the main thread ([`StoreTextures`]), or a worker thread's handles reserved for images it
/// decodes (`model::Deferred`).
pub(crate) trait TextureSource {
    /// The texture at a NIF texture path, loaded once.
    fn texture(&mut self, raw_path: &str, srgb: bool) -> Option<Handle<Image>>;
    /// Whether the texture of `h` has no alpha channel ([`lacks_alpha`]).
    fn lacks_alpha(&self, h: &Handle<Image>) -> bool;
    /// Whether the texture at a NIF texture path has a cut-out mask in its alpha
    /// (`texture::AlphaShares::is_mask`), read from the archives again: asked only for the
    /// few shapes [`blended_cutout`] concerns.
    fn alpha_is_mask(&self, raw_path: &str) -> bool;
}

/// The main thread's textures: the cache and the image store.
pub(crate) struct StoreTextures<'a> {
    pub textures: &'a mut TextureCache,
    pub data: &'a GameData,
    pub images: &'a mut Assets<Image>,
    pub formats: Option<&'a CompressedImageFormatSupport>,
}

impl TextureSource for StoreTextures<'_> {
    fn texture(&mut self, raw_path: &str, srgb: bool) -> Option<Handle<Image>> {
        self.textures.get(raw_path, srgb, self.data, self.images, self.formats)
    }

    fn lacks_alpha(&self, h: &Handle<Image>) -> bool {
        self.images.get(h).is_some_and(lacks_alpha)
    }

    fn alpha_is_mask(&self, raw_path: &str) -> bool {
        crate::texture::texture_alpha_shares(raw_path, self.data).is_some_and(|s| s.is_mask())
    }
}

/// The alpha cut of a blended cut-out (`NifSurface::blended_cutout`) [engineering choice:
/// half way; the masks it applies to have under 13% of their texels between clear and
/// opaque, and the game's single corn plants cut the same texture at 124 of 255].
pub(crate) const BLENDED_CUTOUT_CUT: f32 = 0.5;

/// Whether a shape drawn blended draws the same as a cut-out, and its cut if so: it blends
/// SRC_ALPHA / INV_SRC_ALPHA and tests alpha (below [`BLEND_TEST_CUT`], or it would be
/// drawn alpha-tested already), and nothing but its base texture sets its alpha, which is
/// a cut-out mask (`mask`, asked last): no vertex-alpha cut, material alpha 1, no alpha
/// animation, view-angle falloff, light, decal, refraction or hair shading. Where alpha is
/// 0 or 1, blending and testing give the same pixels; they differ only on the mask's edge
/// texels. The game's corn rows (`scol\scolmaizerow1b.nif`, flags 0x12ED, threshold 0,
/// `maize_d.dds`) are such shapes [observed]; dirty glass (`nvposter01.nif`, the same
/// flags, `marquee.dds` 40% in between) is not. Survey: docs/design/fsr-upscaling.md
/// "Blended cut-outs". The test function (bits 10-12) is not asked: such a shape is drawn blended
/// whatever its function ([`alpha_mode`] below the cut), so the cut-out draws the pixels the
/// blend does, as FSR's switch needs.
pub(crate) fn blended_cutout(props: &Props, cut: Option<f32>, alpha_mode: AlphaMode, mask: impl FnOnce() -> bool) -> Option<f32> {
    let (flags, threshold) = props.alpha?;
    let source_over = (flags >> 1) & 0xF == 6 && (flags >> 5) & 0xF == 7;
    let tested = flags & 0x200 != 0 && threshold < BLEND_TEST_CUT;
    let alpha_from_texture_only = cut.is_none()
        && props.material_alpha.unwrap_or(1.0) >= 1.0
        && !props.animated_alpha
        && props.falloff.is_none()
        && !props.unlit
        && !props.decal
        && props.refraction.is_none()
        && !props.hair;
    (matches!(alpha_mode, AlphaMode::Blend) && source_over && tested && alpha_from_texture_only && mask()).then_some(BLENDED_CUTOUT_CUT)
}

/// The material for a shape's properties; also returns the base texture path when it
/// could not be loaded. Adds tangents to `mesh` when a normal map is used.
pub(crate) fn shape_material(
    props: &Props,
    mesh: &mut Mesh,
    textures: &mut TextureCache,
    data: &GameData,
    images: &mut Assets<Image>,
    formats: Option<&CompressedImageFormatSupport>,
) -> (NifMaterial, Option<String>) {
    shape_material_with(props, mesh, &mut StoreTextures { textures, data, images, formats })
}

/// [`shape_material`] for a copy of a mesh that went through it before (an actor part's
/// kept CPU mesh, `actor::PartShape::cpu`): tangents it already has are the ones
/// generating them again would give, so they are kept. The copy only serves to make the
/// material; generating them again cost an actor 5 ms in the frame it was drawn
/// [observed 2026-11-07, #1429, Sierra Madre lobby].
pub(crate) fn shape_material_reusing_tangents(
    props: &Props,
    mesh: &mut Mesh,
    textures: &mut TextureCache,
    data: &GameData,
    images: &mut Assets<Image>,
    formats: Option<&CompressedImageFormatSupport>,
) -> (NifMaterial, Option<String>) {
    shape_material_inner(props, mesh, &mut StoreTextures { textures, data, images, formats }, true)
}

/// [`shape_material`] with its textures from `textures`.
pub(crate) fn shape_material_with(props: &Props, mesh: &mut Mesh, textures: &mut impl TextureSource) -> (NifMaterial, Option<String>) {
    shape_material_inner(props, mesh, textures, false)
}

fn shape_material_inner(props: &Props, mesh: &mut Mesh, textures: &mut impl TextureSource, keep_tangents: bool) -> (NifMaterial, Option<String>) {
    fix_vertex_colors(mesh, props.vertex_alpha);
    if props.unlit {
        unlit_vertex_colors(mesh);
    }
    let base = props.textures.first().filter(|t| !t.is_empty()).and_then(|t| textures.texture(t, true));
    let normal = props.textures.get(1).filter(|t| !t.is_empty()).and_then(|t| textures.texture(t, false));
    let missing = if base.is_none() { props.textures.first().filter(|t| !t.is_empty()).cloned() } else { None };
    let has_uv = mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some();
    let normal = normal.filter(|_| has_uv && ((keep_tangents && mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some()) || mesh.generate_tangents().is_ok()));
    let cut = blend_cut(props, mesh);
    let alpha_mode = if cut.is_some() { AlphaMode::Blend } else { shape_alpha_mode(props) };
    let cutout = blended_cutout(props, cut, alpha_mode, || props.textures.first().is_some_and(|t| !t.is_empty() && textures.alpha_is_mask(t)));
    let gloss = props.glossiness.unwrap_or(10.0);
    // Emitted light × glow map, or uniform without a glow map. Not scaled by the camera
    // exposure (Bevy's default emissive_exposure_weight 0).
    let emissive = shape_emissive(props, alpha_mode);
    let glow = (emissive.is_some() || props.animated_emissive)
        .then_some(props.glow.as_deref())
        .flatten()
        .and_then(|t| textures.texture(t, true));
    let unmasked = normal.as_ref().is_some_and(|h| textures.lacks_alpha(h));
    let (reflectance, masked) = specular(props, normal.is_some(), unmasked);
    let material = StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, props.material_alpha.unwrap_or(1.0)),
        base_color_texture: base,
        specular_texture: normal.clone().filter(|_| masked),
        normal_map_texture: normal,
        // Green points down the image (DirectX style), Bevy expects up [observed,
        // docs/spec-notes/nif.md §6 "Normal maps"].
        flip_normal_map_y: true,
        perceptual_roughness: gloss_roughness(gloss),
        metallic: 0.0,
        reflectance,
        emissive: emissive.unwrap_or(LinearRgba::BLACK),
        emissive_texture: glow,
        alpha_mode,
        double_sided: props.double_sided,
        cull_mode: if props.double_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
        unlit: props.unlit,
        ..default()
    };
    let mut material = nif_material(material);
    if let Some(f) = shape_falloff(props, alpha_mode) {
        material.extension.falloff = f.params;
    }
    material.extension.alpha_cut = cut.unwrap_or(0.0);
    material.extension.blended_cutout = cutout;
    if props.hair && cut.is_none() && matches!(alpha_mode, AlphaMode::Blend) {
        material.extension.alpha_cut = HAIR_DEPTH_CUT;
        material.extension.depth_write = true;
    }
    material.extension.add_whole = if adds_whole(props) { 1.0 } else { 0.0 };
    material.extension.emissive_rule = shape_emissive_rule(props, alpha_mode);
    material.extension.decal = props.decal;
    if crate::nif_material::soft_intersections(alpha_mode, props.unlit, props.refraction.is_some(), burns(props)) {
        material.extension.soft.x = crate::nif_material::soft_distance(mesh_extent(mesh));
    }
    if let Some(strength) = props.refraction {
        refraction_material(&mut material, strength, props.fire_period);
        // The models' refracting shells bend all of their area, as before the cut was
        // read for refraction (docs/spec-notes/refraction.md §4).
        material.extension.alpha_cut = 0.0;
    }
    // Object LOD atlases hold near-black colour where their alpha is 0 (see
    // `shape_alpha_mode`); a soft edge would draw a little of it around every cut-out.
    material.extension.hard_edges = props.lod_building;
    (material, missing)
}

/// The alpha from which a blended hair-shader shape (hair, beards, brows) writes depth;
/// fainter fragments are dropped (`nif_material` module notes, "Depth of blended hair")
/// [engineering choice: high enough that the faint fringe of a strand leaves no outline
/// in the smoke or flames behind it, low enough that the strands keep their soft ends].
pub const HAIR_DEPTH_CUT: f32 = 0.25;

/// The alpha from which a hair model's shapes (the HAIR record's model) are drawn solid:
/// a cut-out at it ([`hair_model_cut_out`]) holds the hair's depth and shadow, and the
/// fainter texels are drawn blended over it ([`hair_model_layer`]) (docs/spec-notes/nif.md
/// §6 "Hair models: a solid core and a blended layer") [engineering choice, from the
/// hair textures' alpha: the Mohawk's shaved sides (`hairraider01.dds`, which 11 of the
/// 67 HAIR models use) are a plateau at 0.65–0.75 that has to stay see-through, while
/// `hairbase.dds` (Buzz Saw, Balding, Combover) is a plateau at 0.85–0.95 with no texel
/// above it, so a cut above 0.85 would leave those hairs no solid part at all].
pub const HAIR_MODEL_CUT: f32 = 0.8;

/// Make a hair model's shape the solid core of the hair: a cut-out at [`HAIR_MODEL_CUT`]
/// in place of the blend its NiAlphaProperty asks for (docs/spec-notes/nif.md §6). The
/// fainter texels are drawn by a second, blended draw of the shape
/// ([`hair_model_layer`]). Blended alone, the outer layer of `hairwavy.nif` (Chet), mapped
/// onto the soft edge of its texture's alpha, showed as a see-through shell around his
/// head against a bright window where the original draws solid hair [observed: owner's
/// frame of the original, 2026-11-03, #1454]: a blended layer that writes depth hides the
/// hair drawn after it behind it. Head parts that use the hair shader (beards, brows)
/// stay blended: as cut-outs `beardthin.nif` became a full beard, where the original's is
/// thin. A cut-out draws in the opaque passes, so it hides what lies behind it whatever
/// the order of the blended shapes, and it casts a shadow.
pub fn hair_model_cut_out(material: &mut NifMaterial) {
    if !matches!(material.base.alpha_mode, AlphaMode::Blend) || material.extension.refract {
        return;
    }
    material.base.alpha_mode = AlphaMode::Mask(HAIR_MODEL_CUT);
    material.extension.alpha_cut = 0.0;
    material.extension.depth_write = false;
}

/// The blended layer drawn over a hair model's solid core ([`hair_model_cut_out`]), made
/// from the shape's blended material: the texels below [`HAIR_MODEL_CUT`] (shaved and
/// stubbled scalp, the soft ends and edges of the strands) blend over what lies behind
/// them, the hair's own core included, as the NiAlphaProperty asks (SRC_ALPHA /
/// INV_SRC_ALPHA). It writes no depth, so its order among the hair's layers does not
/// matter, and it casts no shadow. Over the core it draws the core's own colour again at
/// the core's depth, which the main passes' depth test (greater or equal) lets through
/// and which leaves the colour as it is. `None` for a shape that is not blended or that
/// refracts.
pub fn hair_model_layer(material: &NifMaterial) -> Option<NifMaterial> {
    if !matches!(material.base.alpha_mode, AlphaMode::Blend) || material.extension.refract {
        return None;
    }
    let mut layer = material.clone();
    layer.extension.alpha_cut = 0.0;
    layer.extension.depth_write = false;
    Some(layer)
}

/// Make `material` a refracting one (`nif_material`, module notes "Refraction"): drawn
/// after the opaque pass from Bevy's copy of it (a specular transmission above 0 puts
/// the material there and keeps it out of the prepass), opaque, without depth writes.
pub fn refraction_material(material: &mut NifMaterial, strength: f32, fire_period: f32) {
    material.base.specular_transmission = 1.0;
    material.base.alpha_mode = AlphaMode::Opaque;
    material.extension.refraction = strength;
    material.extension.fire_period = fire_period;
    material.extension.refract = true;
    material.extension.depth_write = false;
}

pub(crate) fn build_mesh(data: &Struct) -> Option<(Mesh, usize)> {
    let md = mesh_data(data);
    if md.positions.is_empty() || md.triangles.is_empty() {
        return None;
    }
    let n = md.positions.len();
    let tris = md.triangles.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, md.positions);
    let has_normals = md.normals.len() == n;
    if has_normals {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, md.normals);
    }
    if let Some(uv) = md.uv_sets.first().filter(|u| u.len() == n) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone());
    }
    if md.colors.len() == n {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, md.colors);
    }
    let max = n as u32;
    let idx: Vec<u32> = md.triangles.iter().filter(|t| t.iter().all(|&i| i < max)).flatten().copied().collect();
    mesh.insert_indices(Indices::U32(idx));
    if !has_normals {
        mesh.compute_normals();
    }
    Some((mesh, tris))
}

/// Per-vertex joint indices and normalised weights (four influences per vertex).
type SkinWeights = (Vec<[u16; 4]>, Vec<[f32; 4]>);

/// Joint indices and weights per vertex from NiSkinData (top four influences).
fn skin_weights(skin_data: &Struct, vertices: usize) -> Option<SkinWeights> {
    let bones = skin_data.list("bone_list")?;
    let mut infl: Vec<Vec<(u16, f32)>> = vec![Vec::new(); vertices];
    for (b, bone) in bones.iter().enumerate() {
        let Value::Struct(bone) = bone else { continue };
        for w in bone.list("vertex_weights").unwrap_or_default() {
            let Value::Struct(w) = w else { continue };
            let (Some(i), Some(wt)) = (w.int("index"), w.float("weight")) else { continue };
            if let Some(v) = infl.get_mut(i as usize) {
                v.push((b as u16, wt));
            }
        }
    }
    if infl.iter().all(|v| v.is_empty()) {
        return None;
    }
    let mut joints = Vec::with_capacity(vertices);
    let mut weights = Vec::with_capacity(vertices);
    for mut v in infl {
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v.truncate(4);
        let sum: f32 = v.iter().map(|x| x.1).sum();
        let mut j = [0u16; 4];
        let mut w = [0f32; 4];
        for (k, (b, wt)) in v.iter().enumerate() {
            j[k] = *b;
            w[k] = if sum > 0.0 { wt / sum } else { 0.0 };
        }
        if sum <= 0.0 {
            w[0] = 1.0;
        }
        joints.push(j);
        weights.push(w);
    }
    Some((joints, weights))
}

/// A shape's skin: the bone blocks it binds to, per bone the inverse bind matrix in
/// shape space (the bone's stored skin transform, docs/spec-notes/nif.md §6), and per
/// vertex the joint indices and weights.
pub(crate) struct Skin {
    pub bones: Vec<usize>,
    pub inverse_binds: Vec<Mat4>,
    weights: SkinWeights,
}

impl Skin {
    /// Add the joint attributes to `mesh`.
    pub(crate) fn insert_into(self, mesh: &mut Mesh) -> Vec<Mat4> {
        let (ji, jw) = self.weights;
        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(ji));
        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, jw);
        self.inverse_binds
    }
}

/// The skin of `shape` (with `vertices` vertices), if it has a usable one.
pub(crate) fn shape_skin(dec: &Decoded, shape: &Struct, vertices: usize) -> Option<Skin> {
    let si = shape.block_ref("skin_instance").and_then(|r| dec.get(r))?;
    let bones: Vec<usize> = si.refs("bones").unwrap_or_default().iter().map(|&b| usize::try_from(b).ok()).collect::<Option<_>>()?;
    let sd = si.block_ref("data").and_then(|r| dec.get(r))?;
    let inverse_binds: Vec<Mat4> = sd
        .list("bone_list")
        .unwrap_or_default()
        .iter()
        .map(|b| match b {
            Value::Struct(b) => Some(Mat4::from(affine(&NifTransform::from_struct(b.strukt("skin_transform")?)?))),
            _ => None,
        })
        .collect::<Option<_>>()?;
    if inverse_binds.len() != bones.len() {
        return None;
    }
    Some(Skin { bones, inverse_binds, weights: skin_weights(sd, vertices)? })
}

impl SpawnCtx<'_, '_> {
    /// Spawn the NIF at `path` under a new root entity. When `skeleton` is given, that
    /// file's node hierarchy is spawned first and skinned shapes bind to its bones by
    /// name: creature and character body meshes store their bones flat, as direct
    /// children of the root with bind transforms baked in, while animations are relative
    /// to the skeleton's hierarchy [observed, docs/spec-notes/nif.md §6].
    pub fn spawn_nif(&mut self, path: &str, skeleton: Option<&str>) -> Result<SpawnReport, String> {
        let root = self
            .commands
            .spawn((
                Name::new(path.to_string()),
                NifRoot,
                Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                Visibility::default(),
            ))
            .id();
        // A panic while building is this file's failure, not the game's (guard.rs).
        let skel = match skeleton {
            Some(sp) => Some(crate::guard::guarded(|| self.spawn_into(sp, root, None))?),
            None => None,
        };
        let mut report = crate::guard::guarded(|| self.spawn_into(path, root, skel.as_ref().map(|s| &s.nodes)))?;
        if let Some(sk) = skel {
            // Animation targets: skeleton bones take precedence over the mesh's flat copies.
            for (k, v) in sk.nodes {
                report.nodes.insert(k, v);
            }
            report.bind.extend(sk.bind);
            report.skeleton = skeleton.map(str::to_string);
        }
        report.root = Some(root);
        Ok(report)
    }

    fn spawn_into(
        &mut self,
        path: &str,
        root: Entity,
        skeleton: Option<&HashMap<String, Entity>>,
    ) -> Result<SpawnReport, String> {
        let bytes = self.data.read(path).ok_or_else(|| format!("{path} not found"))?;
        let schema = self.data.schema.clone();
        let file = NifFile::parse(&bytes, Some(&schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&file, &bytes, &schema);
        let objects = scene_objects(&file, &dec);
        let mut report = SpawnReport { blocks: file.blocks.len(), ..Default::default() };

        // Pass 1: one entity per node or shape, with its local transform.
        let mut entity_of: HashMap<usize, Entity> = HashMap::new();
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        for o in &objects {
            let flags = dec.blocks[o.block].as_ref().ok().and_then(|s| s.int("flags")).unwrap_or(0);
            let mut e = self.commands.spawn((
                Name::new(o.name.clone().unwrap_or_else(|| o.type_name.clone())),
                to_bevy(&o.transform),
                Visibility::default(),
            ));
            if flags & 1 != 0 || is_editor_marker(o.name.as_deref()) {
                e.insert((NifHidden, Visibility::Hidden));
            }
            let id = e.id();
            entity_of.insert(o.block, id);
            if let Some(n) = &o.name {
                report.nodes.entry(n.clone()).or_insert(id);
                report.bind.insert(id, to_bevy(&o.transform));
            }
        }
        for o in &objects {
            let parent = o.parent.and_then(|p| entity_of.get(&p)).copied().unwrap_or(root);
            self.commands.entity(parent).add_child(entity_of[&o.block]);
        }

        // World transforms in NIF space (for bounds), parents first.
        let mut world: HashMap<usize, Affine3A> = HashMap::new();
        fn world_of(b: usize, by_block: &HashMap<usize, &SceneObject>, cache: &mut HashMap<usize, Affine3A>, depth: usize) -> Affine3A {
            if let Some(w) = cache.get(&b) {
                return *w;
            }
            let o = by_block[&b];
            let local = affine(&o.transform);
            let w = match o.parent {
                Some(p) if depth < 512 && by_block.contains_key(&p) => world_of(p, by_block, cache, depth + 1) * local,
                _ => local,
            };
            cache.insert(b, w);
            w
        }
        let to_y_up = Affine3A::from_rotation_x(-FRAC_PI_2);

        // Pass 2: geometry and materials.
        for o in objects.iter().filter(|o| !o.is_node) {
            let Ok(shape) = &dec.blocks[o.block] else { continue };
            let Some(data_ref) = shape.block_ref("data") else { continue };
            let Some(data) = dec.get(data_ref) else { continue };
            let Some((mut mesh, tris)) = build_mesh(data) else { continue };

            let props = shape_props(&file, &dec, o, &by_block);
            let (material, missing) = shape_material(
                &props,
                &mut mesh,
                &mut self.textures,
                &self.data,
                &mut self.images,
                self.formats.as_deref(),
            );
            report.textures_missing.extend(missing);
            if props.is_helper() {
                self.commands.entity(entity_of[&o.block]).insert((NifHidden, Visibility::Hidden));
            }

            // Skinning: bones bind to the skeleton's nodes by name when one is given.
            let joint = |b: &usize| {
                let by_name = by_block.get(b).and_then(|o| o.name.as_ref()).and_then(|n| skeleton?.get(n));
                by_name.or_else(|| entity_of.get(b)).copied()
            };
            let mut skinned = None;
            if let Some(skin) = shape_skin(&dec, shape, mesh.count_vertices()) {
                if let Some(joints) = skin.bones.iter().map(joint).collect::<Option<Vec<Entity>>>() {
                    let binds = skin.insert_into(&mut mesh);
                    skinned = Some(SkinnedMesh { inverse_bindposes: self.bindposes.add(binds), joints });
                }
            }

            // Bounds in Bevy world space.
            let w = to_y_up * world_of(o.block, &by_block, &mut world, 0);
            if let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                for v in p {
                    let q = w.transform_point3(Vec3::from_array(*v));
                    report.bounds = Some(match report.bounds {
                        None => (q, q),
                        Some((lo, hi)) => (lo.min(q), hi.max(q)),
                    });
                }
            }

            let e = entity_of[&o.block];
            let mut cmd = self.commands.entity(e);
            match skinned {
                Some(s) => {
                    let mesh = mesh.with_generated_skinned_mesh_bounds().unwrap_or_else(|_| Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD));
                    cmd.insert((Mesh3d(self.meshes.add(mesh)), MeshMaterial3d(self.materials.add(material)), s, DynamicSkinnedMeshBounds));
                    report.skinned += 1;
                }
                None => {
                    cmd.insert((Mesh3d(self.meshes.add(mesh)), MeshMaterial3d(self.materials.add(material))));
                }
            }
            report.shapes += 1;
            report.triangles += tris;
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoTextures;
    impl TextureSource for NoTextures {
        fn texture(&mut self, _: &str, _: bool) -> Option<Handle<Image>> {
            None
        }
        fn lacks_alpha(&self, _: &Handle<Image>) -> bool {
            false
        }
        fn alpha_is_mask(&self, _: &str) -> bool {
            false
        }
    }

    /// Textures whose alpha is a cut-out mask when their path says "mask".
    struct MaskTextures;
    impl TextureSource for MaskTextures {
        fn texture(&mut self, _: &str, _: bool) -> Option<Handle<Image>> {
            None
        }
        fn lacks_alpha(&self, _: &Handle<Image>) -> bool {
            false
        }
        fn alpha_is_mask(&self, raw_path: &str) -> bool {
            raw_path.contains("mask")
        }
    }

    #[test]
    fn blended_shapes_with_a_cut_out_mask_are_blended_cutouts() {
        let quad = || {
            let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
            m
        };
        // The corn rows: blend SRC_ALPHA / INV_SRC_ALPHA, test GREATER 0 (0x12ED), lit.
        let corn = Props { alpha: Some((0x12ED, 0)), textures: vec![r"landscape\plants\maize_mask.dds".into()], ..Default::default() };
        let (m, _) = shape_material_with(&corn, &mut quad(), &mut MaskTextures);
        assert!(matches!(m.base.alpha_mode, AlphaMode::Blend), "drawn blended until FSR asks");
        assert_eq!(m.extension.blended_cutout, Some(BLENDED_CUTOUT_CUT));
        // Translucent texture alpha (dirty glass), or no texture: stays blended only.
        assert_eq!(shape_material_with(&corn, &mut quad(), &mut NoTextures).0.extension.blended_cutout, None);
        // Anything else that sets alpha keeps it a real blend.
        for props in [
            Props { material_alpha: Some(0.5), ..corn.clone() },
            Props { animated_alpha: true, ..corn.clone() },
            Props { unlit: true, ..corn.clone() },
            Props { decal: true, ..corn.clone() },
            Props { hair: true, ..corn.clone() },
            Props { refraction: Some(1.0), ..corn.clone() },
            // Not tested, additive, or tested at a cut that already makes it a cut-out.
            Props { alpha: Some((0x12ED & !0x200, 0)), ..corn.clone() },
            Props { alpha: Some((0x12ED & !(0xF << 5), 0)), ..corn.clone() },
            Props { alpha: Some((0x12ED, 128)), ..corn.clone() },
        ] {
            assert_eq!(shape_material_with(&props, &mut quad(), &mut MaskTextures).0.extension.blended_cutout, None, "{props:?}");
        }
    }

    fn triangle() -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD).with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3])
    }

    /// `hairbase.nif`'s NiAlphaProperty: blended, no test (flags 4333) [observed].
    #[test]
    fn blended_hair_writes_depth_above_its_cut() {
        let hair = Props { hair: true, alpha: Some((4333, 0)), ..Default::default() };
        let (m, _) = shape_material_with(&hair, &mut triangle(), &mut NoTextures);
        assert!(matches!(m.base.alpha_mode, AlphaMode::Blend));
        assert!(m.extension.depth_write);
        assert_eq!(m.extension.alpha_cut, HAIR_DEPTH_CUT);
        assert!(crate::nif_material::see_through(&m), "hair still casts no shadow");
        // Other blended shapes (smoke, glass) write none and drop nothing.
        let (m, _) = shape_material_with(&Props { hair: false, ..hair }, &mut triangle(), &mut NoTextures);
        assert!(!m.extension.depth_write);
        assert_eq!(m.extension.alpha_cut, 0.0);
    }

    /// A hair model's blended shape becomes a solid cut-out (#1454) with a blended layer
    /// drawn over it (#1576); one that is already a cut-out or refracts is left alone.
    #[test]
    fn hair_models_are_a_core_and_a_layer() {
        let hair = Props { hair: true, alpha: Some((4333, 0)), ..Default::default() };
        let (blended, _) = shape_material_with(&hair, &mut triangle(), &mut NoTextures);
        let mut m = blended.clone();
        hair_model_cut_out(&mut m);
        assert_eq!(m.base.alpha_mode, AlphaMode::Mask(HAIR_MODEL_CUT));
        assert!(!m.extension.depth_write);
        assert_eq!(m.extension.alpha_cut, 0.0);
        assert!(!crate::nif_material::see_through(&m), "a cut-out casts a shadow");
        let layer = hair_model_layer(&blended).expect("a blended hair shape has a layer");
        assert_eq!(layer.base.alpha_mode, AlphaMode::Blend);
        assert!(!layer.extension.depth_write, "the layer writes no depth");
        assert_eq!(layer.extension.alpha_cut, 0.0, "the layer drops nothing");
        assert!(crate::nif_material::see_through(&layer), "the layer casts no shadow");
        let mut masked = m.clone();
        masked.base.alpha_mode = AlphaMode::Mask(0.7);
        hair_model_cut_out(&mut masked);
        assert_eq!(masked.base.alpha_mode, AlphaMode::Mask(0.7));
        assert!(hair_model_layer(&masked).is_none());
    }

    #[test]
    fn fire_period_needs_its_flag() {
        // The Incinerator's shell: flags 0x82018101, period 10.
        assert!((fire_period(0x8201_8101, Some(10)) - 1.0).abs() < 1e-6);
        // Refraction without Fire Refraction, or no period: still.
        assert_eq!(fire_period(1 << 15, Some(10)), 0.0);
        assert_eq!(fire_period(0x8201_8101, Some(0)), 0.0);
        assert_eq!(fire_period(0x8201_8101, None), 0.0);
        // A negative period scrolls the other way.
        assert!((fire_period(0x8201_8101, Some(-5)) + 0.5).abs() < 1e-6);
    }

    #[test]
    fn blend_modes() {
        let flags = |blend: bool, src: i64, dst: i64, test: bool| (blend as i64) | (src << 1) | (dst << 5) | ((test as i64) << 9);
        assert!(matches!(alpha_mode(flags(true, 6, 7, false), 0), AlphaMode::Blend));
        assert!(matches!(alpha_mode(flags(true, 6, 0, false), 0), AlphaMode::Add));
        assert!(matches!(alpha_mode(flags(true, 4, 1, false), 0), AlphaMode::Multiply));
        assert!(matches!(alpha_mode(flags(false, 6, 7, true), 128), AlphaMode::Mask(t) if (t - 128.0 / 255.0).abs() < 1e-6));
        assert!(matches!(alpha_mode(flags(false, 6, 7, false), 0), AlphaMode::Opaque));
        assert!(matches!(alpha_mode(flags(true, 6, 7, true), 128), AlphaMode::Mask(_)));
        // Blend + test with a low threshold blends (dirty glass).
        assert!(matches!(alpha_mode(flags(true, 6, 7, true), 0), AlphaMode::Blend));
        assert!(matches!(alpha_mode(flags(true, 6, 7, true), 63), AlphaMode::Blend));
        assert!(matches!(alpha_mode(flags(true, 6, 0, true), 128), AlphaMode::Add));
        assert!(is_editor_marker(Some("EditorMarker01")) && is_editor_marker(Some("editormarker")));
        assert!(!is_editor_marker(Some("C_EditorMarker01")) && !is_editor_marker(None));
        // nif.xml default flags 4844 = 0x12EC: blend off, test on, TEST_GREATER.
        assert!(matches!(alpha_mode(4844, 128), AlphaMode::Mask(t) if (t - 128.5 / 255.0).abs() < 1e-6));
        // The poster marquee glass (0x12ED, threshold 0) blends.
        assert!(matches!(alpha_mode(0x12ED, 0), AlphaMode::Blend));
        // Test only, TEST_GREATER with threshold 0: alpha 0 fails.
        assert!(matches!(alpha_mode(0x12EC, 0), AlphaMode::Mask(t) if t > 0.0 && t < 1.0 / 255.0));
        // GREATER_EQUAL keeps the threshold itself.
        assert!(matches!(alpha_mode(0x200 | (6 << 10), 128), AlphaMode::Mask(t) if (t - 128.0 / 255.0).abs() < 1e-6));
    }

    #[test]
    fn lod_building_is_alpha_tested() {
        // Object LOD blocks: the flag and no NiAlphaProperty.
        let block = Props { shader: true, has_properties: true, lod_building: true, ..Default::default() };
        assert!(matches!(shape_alpha_mode(&block), AlphaMode::Mask(t) if (t - 128.5 / 255.0).abs() < 1e-6));
        // A NiAlphaProperty wins; without the flag nothing is cut.
        assert!(matches!(shape_alpha_mode(&Props { alpha: Some((0x12ED, 0)), ..block.clone() }), AlphaMode::Blend));
        assert!(matches!(shape_alpha_mode(&Props { lod_building: false, ..block }), AlphaMode::Opaque));
        assert!(matches!(shape_alpha_mode(&Props { alpha: Some((4844, 64)), ..Default::default() }), AlphaMode::Mask(_)));
        // An unlit shape with "Dynamic_Alpha", faded by a clip and without a NiAlphaProperty
        // blends (the Fat Man's mushroom card); otherwise it stays opaque.
        let card = Props { shader: true, has_properties: true, unlit: true, dynamic_alpha: true, animated_alpha: true, ..Default::default() };
        assert!(matches!(shape_alpha_mode(&card), AlphaMode::Blend));
        assert!(matches!(shape_alpha_mode(&Props { animated_alpha: false, ..card.clone() }), AlphaMode::Opaque));
        assert!(matches!(shape_alpha_mode(&Props { dynamic_alpha: false, ..card.clone() }), AlphaMode::Opaque));
        assert!(matches!(shape_alpha_mode(&Props { unlit: false, ..card.clone() }), AlphaMode::Opaque));
        // An actor's screen whose material its clips fade is shown or not by its alpha
        // (Mr. House's screens, #1542).
        let screen = Props { animated_alpha: false, alpha_gated: true, ..card };
        assert!(matches!(shape_alpha_mode(&screen), AlphaMode::Mask(t) if t == 0.5));
        assert!(matches!(shape_alpha_mode(&Props { dynamic_alpha: false, ..screen }), AlphaMode::Opaque));
    }

    #[test]
    fn falloff_only_when_blended() {
        let props = Props { falloff: Falloff::from_fields(0.985, 0.174, 1.0, 0.0), ..Default::default() };
        assert!(shape_falloff(&props, AlphaMode::Add).is_some());
        assert!(shape_falloff(&props, AlphaMode::Blend).is_some());
        assert!(shape_falloff(&props, AlphaMode::Opaque).is_none());
        assert!(shape_falloff(&props, AlphaMode::Mask(0.5)).is_none());
        assert!(shape_falloff(&Props::default(), AlphaMode::Add).is_none());
    }

    #[test]
    fn falloff_only_with_the_flag() {
        // Shader flags as stored: the Saloon lamp beam (bit 6 set), the neon tubes of the
        // Saloon's sign and the Old Town shadow strips (bit 6 clear).
        let (beam, neon, strip) = (0xa200_0148, 0x8200_0100, 0x8e00_0008);
        assert!(uses_falloff(beam) && !uses_falloff(neon) && !uses_falloff(strip));
        let authored = [Some(0.985), Some(0.174), Some(1.0), Some(0.0)];
        let defaults = [Some(1.0), Some(0.0), Some(1.0), Some(0.0)];
        assert_eq!(property_falloff(beam, authored), Falloff::from_fields(0.985, 0.174, 1.0, 0.0));
        // With the flag the default values are a falloff too: opacity is the cosine.
        assert!((property_falloff(beam, defaults).unwrap().opacity(0.3) - 0.3).abs() < 1e-6);
        // Without it nothing fades, whether the fields hold the defaults or other values.
        assert_eq!(property_falloff(neon, defaults), None);
        assert_eq!(property_falloff(strip, defaults), None);
        assert_eq!(property_falloff(neon, authored), None);
        // The flag does not rescue an empty range, constant full opacity or absent fields.
        assert_eq!(property_falloff(beam, [Some(0.0); 4]), None);
        assert_eq!(property_falloff(beam, [Some(1.0), Some(0.0), Some(1.0), Some(1.0)]), None);
        assert_eq!(property_falloff(beam, [None; 4]), None);
        assert_eq!(property_falloff(beam, [Some(1.0), None, Some(1.0), Some(0.0)]), None);
    }

    #[test]
    fn unlit_emissive_tints_by_hue() {
        // Dead Money's lantern glow card: warm (0.85, 0.65, 0.47) x 1 tints a white texture.
        let t = unlit_emissive([0.847, 0.651, 0.475], 1.0);
        assert!((t[0] - 1.0).abs() < 1e-6 && t[1] < 0.6 && t[2] < 0.3, "{t:?}");
        // Grey below 1 changes nothing; white x 25 is 25 times the texture.
        assert_eq!(unlit_emissive([0.1; 3], 1.25), [1.0; 3]);
        assert_eq!(unlit_emissive([1.0; 3], 25.0), [25.0; 3]);
        // A multiplier of 0 keeps the surface colour.
        assert_eq!(unlit_emissive([0.5; 3], 0.0), [1.0; 3]);
        // An additive unlit shape takes the brightness in display values (nif.md §6
        // "Additive blend"): the Big MT's sky beams, white × 10, are 10^2.2 in linear, so
        // the shader's display value is 10 × the texture; alpha-blended ones keep 10.
        let beam = Props { unlit: true, emissive: Some([1.0; 3]), emissive_mult: Some(10.0), ..Default::default() };
        assert!((shape_emissive(&beam, AlphaMode::Add).unwrap().red - 10f32.powf(2.2)).abs() < 1e-2);
        assert!((shape_emissive(&beam, AlphaMode::Blend).unwrap().red - 10.0).abs() < 1e-5);
        // The hue stays a display colour raised to the gamma.
        let warm = Props { unlit: true, emissive: Some([1.0, 0.5, 0.0]), emissive_mult: Some(2.0), ..Default::default() };
        let w = shape_emissive(&warm, AlphaMode::Add).unwrap();
        assert!((w.red - 2f32.powf(2.2)).abs() < 1e-4 && (w.green - (0.5f32 * 2.0).powf(2.2)).abs() < 1e-4 && w.blue == 0.0);
    }

    #[test]
    fn an_unlit_pictures_tracks_multiply_the_stored_multiplier_not_the_held_one() {
        let picture = Props { unlit: true, emissive: Some([1.0; 3]), emissive_mult: Some(30.0), ..Default::default() };
        assert_eq!(shape_emissive_mult(&picture, AlphaMode::Opaque), 1.0);
        assert_eq!(shape_emissive(&picture, AlphaMode::Opaque).unwrap(), LinearRgba::WHITE);
        assert_eq!(picture.emissive_parts(), ([1.0; 3], 30.0));
        let gated = Props { dynamic_alpha: true, alpha_gated: true, ..picture.clone() };
        assert_eq!(gated.emissive_parts(), ([1.0; 3], 30.0));
        assert_eq!(shape_emissive(&gated, shape_alpha_mode(&gated)).unwrap(), LinearRgba::WHITE);

        let blended = Props { alpha: Some((0x12ed, 0)), emissive_mult: Some(40.0), ..picture.clone() };
        assert_eq!(shape_emissive_mult(&blended, AlphaMode::Blend), 40.0);
        assert_eq!(blended.emissive_parts().1, 40.0);
        assert_eq!(shape_emissive(&blended, AlphaMode::Blend).unwrap().red, 40.0);
        let additive = Props { alpha: Some((1, 0)), ..blended };
        assert_eq!(shape_emissive_mult(&additive, AlphaMode::Add), 40.0);
        assert_eq!(additive.emissive_parts().1, 40.0);
        assert!((shape_emissive(&additive, AlphaMode::Add).unwrap().red - 40f32.powf(2.2)).abs() < 1e-2);

        let neon = Props { external_emittance: true, emissive_mult: Some(25.0), ..picture };
        assert_eq!(shape_emissive_mult(&neon, AlphaMode::Opaque), 25.0);
        assert_eq!(neon.emissive_parts().1, 25.0);
        assert_eq!(shape_emissive(&neon, AlphaMode::Opaque).unwrap().red, 25.0);
    }

    #[test]
    fn a_picture_holds_its_colour_times_its_multiplier() {
        // A Securitron's screen: grey 0.05 x 30 shows its texture, not a twentieth of it.
        let face = Props { unlit: true, emissive: Some([0.05; 3]), emissive_mult: Some(30.0), ..Default::default() };
        // Its idle track keys 0.032 x 30, about 1, on the stored multiplier.
        assert_eq!(face.emissive_parts().1, 30.0);
        assert!((shape_emissive(&face, AlphaMode::Opaque).unwrap().red - 1.0).abs() < 1e-5);
        // Under 1 it is left alone; with the flag it is no picture.
        let dim = Props { emissive_mult: Some(4.0), ..face.clone() };
        assert_eq!(shape_emissive_mult(&dim, AlphaMode::Opaque), 4.0);
        let neon = Props { external_emittance: true, ..face };
        assert_eq!(shape_emissive_mult(&neon, AlphaMode::Opaque), 30.0);
    }

    #[test]
    fn unlit_cutouts_take_the_plain_path_and_no_scale() {
        let no_scale = Vec4::new(1.0, 0.0, 0.0, 0.0);
        let cutout_rule = Vec4::new(1.0, 1.0, 0.0, 0.0);
        let picture = Props { unlit: true, emissive_mult: Some(30.0), ..Default::default() };
        for mode in [AlphaMode::Opaque, AlphaMode::Blend, AlphaMode::Add] {
            assert_eq!(shape_emissive_rule(&picture, mode), no_scale);
        }
        // A gated screen with no authored alpha is a picture, not a cut-out.
        assert_eq!(shape_emissive_rule(&picture, AlphaMode::Mask(0.5)), no_scale);
        let sign = Props { external_emittance: true, emissive_mult: Some(4.0), alpha: Some((0x12EC, 128)), ..picture };
        for mult in [4.0, 7.5, 14.0] {
            let sign = Props { emissive_mult: Some(mult), ..sign.clone() };
            assert_eq!(shape_emissive_rule(&sign, AlphaMode::Mask(0.5)), cutout_rule);
            assert_eq!(shape_emissive_mult(&sign, AlphaMode::Mask(0.5)), mult);
            let no_flag = Props { external_emittance: false, ..sign };
            assert_eq!(shape_emissive_rule(&no_flag, AlphaMode::Mask(0.5)), cutout_rule);
        }
        for mode in [AlphaMode::Opaque, AlphaMode::Blend, AlphaMode::Add] {
            assert_eq!(shape_emissive_rule(&sign, mode), Vec4::ZERO);
        }
    }

    #[test]
    fn lit_shapes_keep_their_emissive_rules() {
        for external_emittance in [false, true] {
            let lit = Props { external_emittance, emissive: Some([1.0; 3]), emissive_mult: Some(30.0), ..Default::default() };
            for mode in [AlphaMode::Opaque, AlphaMode::Mask(0.5), AlphaMode::Blend, AlphaMode::Add] {
                assert_eq!(shape_emissive_mult(&lit, mode), 30.0);
                assert_eq!(shape_emissive_rule(&lit, mode), Vec4::ZERO);
            }
            assert_eq!(lit.emissive_parts().1, 30.0);
            assert_eq!(shape_emissive(&lit, AlphaMode::Opaque).unwrap().red, 30.0);
            assert_eq!(shape_emissive_rule(&Props { emissive_mult: Some(4.0), ..lit }, AlphaMode::Mask(0.5)), Vec4::ZERO);
        }
    }

    #[test]
    fn multiply_blended_emissive() {
        // Rex's skull cap (rex_skullcap.nif `##SkullCap:0`): NiAlphaProperty flags 4163
        // (blend, source ZERO, destination SRC_COLOR), emissive 0.19 × 12.
        let cap = Props { alpha: Some((4163, 0)), emissive: Some([0.190_283_18; 3]), emissive_mult: Some(12.0), ..Default::default() };
        let mode = alpha_mode(4163, 0);
        assert!(matches!(mode, AlphaMode::Multiply));
        // Its light is a display-value factor of 2.28 on the texture, 2.28^2.2 in linear.
        let e = shape_emissive(&cap, mode).unwrap();
        assert!((e.red - 2.283_398_f32.powf(2.2)).abs() < 1e-3 && e.red == e.green && e.green == e.blue);
        // An additive shape forms its light in display values too (nif.md §6 "Additive
        // blend"); one that is alpha-blended or opaque keeps the plain product.
        assert!((shape_emissive(&cap, AlphaMode::Add).unwrap().red - e.red).abs() < 1e-6);
        assert!((shape_emissive(&cap, AlphaMode::Blend).unwrap().red - 2.283_398).abs() < 1e-5);
        assert!((shape_emissive(&cap, AlphaMode::Opaque).unwrap().red - 2.283_398).abs() < 1e-5);
        // A black emissive colour emits nothing, whatever the multiplier.
        let dark = Props { emissive: Some([0.0; 3]), emissive_mult: Some(10.0), ..Default::default() };
        assert!(shape_emissive(&dark, mode).is_none());
        assert!(shape_emissive(&Props::default(), mode).is_none());
    }

    #[test]
    fn helper_shapes() {
        // No properties at all (landscape LOD): drawn.
        assert!(!Props::default().is_helper());
        // Material only (bone boxes, collision proxies): skipped.
        let material_only = Props { has_properties: true, ..Default::default() };
        assert!(material_only.is_helper());
        // A shader property, or a texture from NiTexturingProperty: drawn.
        assert!(!Props { shader: true, ..material_only.clone() }.is_helper());
        assert!(!Props { textures: vec!["textures\\a.dds".into()], ..material_only }.is_helper());
    }

    #[test]
    fn decal_flags() {
        // utlhallsm1waypipe01.nif: the rust stain `:8` (bits 26, 27 and 31) and the
        // alpha-tested decal `:24` are decals, the wall `:0` and the pipe cards `:1` not.
        assert!(is_decal(0x8c00_0000));
        assert!(is_decal(0x8e00_0101));
        assert!(!is_decal(0x8200_0001));
        assert!(!is_decal(0x8200_0101));
        // Either bit alone (creatures\protectron\decalflame.nif sets only bit 26).
        assert!(is_decal(1 << 26));
        assert!(is_decal(1 << 27));
        assert!(!is_decal(0xffff_ffff & !(1 << 26 | 1 << 27)));
        // The flags reach the material's properties from the shader property.
        assert!(!Props::default().decal);
    }

    #[test]
    fn glow_map_slot() {
        let set: Vec<String> = ["d.dds", "d_n.dds", "d_g.dds", "", "", ""].iter().map(|s| s.to_string()).collect();
        assert_eq!(glow_map(0x8200_0101, &set).as_deref(), Some("d_g.dds"));
        assert_eq!(glow_map(1 << 10, &set), None); // face gen: slot 2 is a skin map
        assert_eq!(glow_map(1 << 18, &set), None); // hair
        assert_eq!(glow_map(0, &set[..2]), None);
        let empty: Vec<String> = ["d.dds", "d_n.dds", ""].iter().map(|s| s.to_string()).collect();
        assert_eq!(glow_map(0, &empty), None);
    }

    #[test]
    fn vertex_colors_cleaned() {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.5, f32::NAN, 2.0, -3e38], [0.2, 0.3, 0.4, 0.25]]);
        let mut flagged = mesh.clone();
        fix_vertex_colors(&mut mesh, false);
        fix_vertex_colors(&mut flagged, true);
        let get = |m: &Mesh| match m.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(c)) => c.clone(),
            _ => panic!(),
        };
        assert_eq!(get(&mesh), vec![[0.5, 1.0, 1.0, 1.0], [0.2, 0.3, 0.4, 1.0]]);
        assert_eq!(get(&flagged), vec![[0.5, 1.0, 1.0, 0.0], [0.2, 0.3, 0.4, 0.25]]);
    }

    #[test]
    fn unlit_dynamic_alpha_shapes_take_their_vertex_alpha() {
        // The dust vortex's unlit sheets: 0xA2080140 (bits 6, 8, 19, 25, 29, 31).
        assert!(uses_vertex_alpha("BSShaderNoLightingProperty", 0xA208_0140));
        assert!(!uses_vertex_alpha("BSShaderPPLightingProperty", 0xA208_0140));
        assert!(uses_vertex_alpha("BSShaderPPLightingProperty", 1 << 3));
        assert!(!uses_vertex_alpha("BSShaderNoLightingProperty", 0x8000_0000));
    }

    #[test]
    fn vertex_alpha_gradient_blends_with_a_cut() {
        let mesh = |alphas: &[f32]| {
            let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, alphas.iter().map(|&a| [0.0, 0.0, 0.0, a]).collect::<Vec<_>>());
            m
        };
        // otbldgdestroy01nv.nif `OTBldgDestroy01NV:5`: untextured black strips along the
        // wall edges, flags 0x12ED (blend, test, GREATER), threshold 128, vertex alpha
        // 0 to 0.75 with the vertex-alpha flag.
        let strip = Props { alpha: Some((0x12ED, 128)), vertex_alpha: true, unlit: true, ..Default::default() };
        let gradient = mesh(&[0.55, 0.0, 0.3, 0.75]);
        assert!(matches!(shape_alpha_mode(&strip), AlphaMode::Mask(_)));
        assert!(matches!(blend_cut(&strip, &gradient), Some(c) if (c - 128.5 / 255.0).abs() < 1e-6));
        // Not without the flag (the alpha is forced to 1 then), with vertex alpha 1
        // throughout, or without vertex colours: those stay alpha-tested.
        let mut unflagged = gradient.clone();
        fix_vertex_colors(&mut unflagged, false);
        assert_eq!(blend_cut(&Props { vertex_alpha: false, ..strip.clone() }, &unflagged), None);
        assert_eq!(blend_cut(&strip, &mesh(&[1.0, 1.0, 1.0])), None);
        assert_eq!(blend_cut(&strip, &Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)), None);
        // Nor shapes that are only tested, blend below the threshold rule already, or add.
        assert_eq!(blend_cut(&Props { alpha: Some((0x12EC, 128)), ..strip.clone() }, &gradient), None);
        assert_eq!(blend_cut(&Props { alpha: Some((0x12ED, 0)), ..strip.clone() }, &gradient), None);
        assert_eq!(blend_cut(&Props { alpha: Some((0x120D, 128)), ..strip.clone() }, &gradient), None);
        assert_eq!(blend_cut(&Props { alpha: None, ..strip }, &gradient), None);
    }

    #[test]
    fn specular_from_mask() {
        let flagged = Props { specular: true, ..Default::default() };
        assert_eq!(specular(&flagged, true, false), (2.0, true));
        assert_eq!(specular(&flagged, true, true), (UNMASKED_SPECULAR, false));
        assert_eq!(specular(&Props::default(), true, false), (0.0, false));
        assert_eq!(specular(&flagged, false, false), (0.3, false));
        let hair = Props { specular: true, hair: true, ..Default::default() };
        assert_eq!((specular(&hair, true, false), specular(&hair, false, false)), ((0.0, false), (0.0, false)));
        // Phong exponent → roughness: 0 is fully rough, higher is smoother.
        assert!((gloss_roughness(0.0) - 1.0).abs() < 1e-6);
        assert!((gloss_roughness(67.0) - 0.41).abs() < 0.01);
        assert!(gloss_roughness(10.0) > gloss_roughness(67.0) && gloss_roughness(-5.0) == 1.0);
    }
}
