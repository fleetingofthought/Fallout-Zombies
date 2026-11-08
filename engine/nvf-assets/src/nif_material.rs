//! The material of NIF shapes: Bevy's StandardMaterial with changes in the fragment shader
//! (`nif_material.wesl`):
//! - the emissive light is multiplied by the surface colour (diffuse texture × vertex
//!   colour), as NIF emissive light is modulated by the texture [hypothesis,
//!   docs/spec-notes/nif.md §6 "Emissive light"];
//! - the view-angle falloff of BSShaderNoLightingProperty scales the alpha, so light
//!   beams, glows, force fields and dust sheets fade where they turn edge-on to the camera
//!   (docs/spec-notes/nif.md §6, "View-angle falloff");
//! - the colour of a multiply-blended shape (tinted glass, shadow overlays) is a factor on
//!   what lies behind it: its lit colour and emissive light are summed in display values
//!   and held to 0..1, without fog or tonemapping ([`multiply_factor`],
//!   docs/spec-notes/nif.md §6, "Multiply blend");
//! - an additive shape (light beams, glows, flames) adds what it would add to the
//!   display values of the opaque frame behind it, read from Bevy's copy of that frame
//!   (the view transmission texture; [`add_display`], [`TransmissionAnchor`], nif.md §6
//!   "Additive blend"); its emissive multiplier counts in display values;
//! - an unlit alpha-blended shape (smoke, mist, dust, particles) puts about what it would
//!   put on display values: its colour is scaled by √alpha ([`blend_colour_scale`],
//!   nif.md §6 "Alpha blend of unlit shapes").
//!
//! And one change in the pipeline: shapes with a decal shader flag are drawn with a depth
//! offset towards the camera ([`decal_bias`], docs/spec-notes/nif.md §6, "Decal flags").
//!
//! Opaque, alpha-tested and alpha-blended shapes with a black emissive colour, no falloff
//! and no decal flag render exactly as with StandardMaterial.
//!
//! Soft intersections: an unlit effect shape that adds or blends (light beams, glows,
//! smoke and dust sheets, particles; [`soft_intersections`]) fades out over
//! [`SOFT_DISTANCE`] towards the opaque surface behind it (the view's depth prepass), so
//! a beam's panels end softly where they pass into a road or a wall instead of along a
//! hard line, the way they blend in the game's frames [engineering choice: the game's
//! own rule is not known; docs/spec-notes/nif.md §6 "Soft intersections"]. Where the
//! shape lies along that surface (its facing within about 25° of the surface's, both
//! taken from screen derivatives) it is not faded, so light pools and grime cards laid on
//! a floor or wall stay. One rule for every such shape, not a setting per model.
//!
//! Soft edges: where the view is multisampled, the cut-out edges of alpha-tested shapes
//! (plants, fences, hair) are anti-aliased ([`soft_edges`]), as the game's transparency
//! multisampling does [observed: the owner's `FalloutPrefs.ini` has `iMultiSample=8` and
//! `bTransparencyMultisampling=1`; docs/spec-notes/nif.md §6 "Alpha"].
//!
//! Crossfades: meshes with a crossfading `VisibilityRange` (the streamed cells' fade in
//! and out, nvforever `fade.rs`) are dithered in every pass. Bevy's depth-only prepass
//! skips the dither, so the pixels the main pass discards would still hold depth there and
//! hide what lies behind them; [`fade_prepass`] gives that prepass a fragment that dithers.
//!
//! Vertex alpha: the main pass tests material alpha × texture alpha × vertex alpha, while
//! Bevy's depth-only prepass leaves the vertex alpha out. NIF shapes get a prepass
//! fragment that tests what the main pass tests ([`nif_prepass`], `nif_prepass.wesl`), for
//! the same reason as the dither.
//!
//! Blend and test together: a blended shape can carry the threshold of its alpha test as
//! a cut ([`NifSurface::alpha_cut`]): fragments below it are dropped, the rest blend
//! (docs/spec-notes/nif.md §6 "Alpha", "Vertex alpha and the test").
//!
//! Refraction: a shape with the Refraction shader flag (`nif_spawn::refracts`: explosions'
//! heat-haze shells, Mister Handy's jet) is not drawn as a surface; it shows what lies
//! behind it, shifted on the screen by its normal (normal map applied) in view space times
//! its refraction strength ([`NifSurface::refraction`], which the model's
//! `BSRefractionStrengthController` animates) times [`REFRACTION_SCREEN`], in screen UV
//! [hypothesis, docs/spec-notes/explosion-effects.md §7, refraction.md §6]. It is drawn after the opaque pass from Bevy's copy of it (the
//! view transmission texture, as the water is), writes no depth, so the flames and smoke
//! inside a fireball's shell still show, and draws nothing into the depth-only passes
//! (shadows). Particles and other blended shapes are drawn later and are not bent.
//!
//! Fire refraction: a refracting shape with the Fire Refraction flag as well
//! (`nif_spawn::fire_period`) moves its bend: its normal map is read scrolled up its
//! texture once every [`NifSurface::fire_period`] seconds, and that tilt is added to the
//! tilt of the shape's own (vertex) normal; the unscrolled normal map is not used, so the
//! map counts once [hypothesis, docs/spec-notes/refraction.md §3]. The
//! Stealth Boy's field (nvforever `stealth_field.rs`) uses the same to make an actor's
//! body shimmer. A refracting shape keeps the outline of its alpha cut
//! ([`NifSurface::alpha_cut`]).
//!
//! Holes: a shape can be eaten away by a holes texture (an effect shader's dissolve,
//! nvforever `effect_shaders.rs`, docs/spec-notes/effect-shaders.md §7): where the
//! texture's alpha (0–255, read at the shape's own texture coordinates) is at or above
//! [`NifSurface::holes`]`.x` nothing is drawn, in every pass, and the band of `.y` units
//! below it is drawn in [`NifSurface::holes_edge`]; `.x` negative (every other shape): no
//! holes. With `.z` 1 the test is turned round: alpha below `.x` leaves a hole and there
//! is no band (an effect shader's overlay cut by an alpha-tested mesh's own texture). A shape with holes is drawn alpha-tested, so the depth-only passes run the
//! same test (`nif_prepass.wesl`).
//!
//! Depth of blended hair: hair-shader shapes (hair, beards, brows) are blended
//! ([`NifSurface::depth_write`]). Bevy writes no depth for blended shapes, so a blended
//! shape drawn after the hair (fire, a dust devil, an effect shader's glow) painted over
//! it wherever only the room's depth lay behind it, and the player's head showed bald
//! through flames (owner's playtest, 2026-10-24). Hair writes depth where its alpha
//! reaches [`nif_spawn::HAIR_DEPTH_CUT`](crate::nif_spawn::HAIR_DEPTH_CUT) and drops the
//! fainter fragments [engineering choice; docs/spec-notes/nif.md §6 "Hair"].
//!
//! Shade light: a shape can take the sun's shadows from another light's shadow map
//! instead of the sun's own ([`NifSurface::shade_light`]). That light is a second
//! directional light with shadows, pointing as the sun does, so its shadow map holds the
//! sun's shadows of the things on its own render layers. It carries a small share of the
//! sun's light (any share; the shader reads it from the two lights' colours), which is
//! what lets the shader tell the sun's part of a fragment's light from the rest using
//! Bevy's lighting alone (`with_shade_light` in the shader). For the player's
//! first-person arms, which the unseen body that casts the player's shadow stands in the
//! middle of [engineering choice, nvforever `player_body.rs`]. Every other shape is lit
//! by Bevy's lighting alone.
//!
//! FaceGen tint: an NPC's face and skin shapes carry the `.egt` colour offset of its
//! texture coefficients ([`NifSurface::facegen_offset`], made by nvforever `facegen.rs`)
//! and the shader adds it to the diffuse texture as the CPU rule does
//! (`fnv_actor::facegen::add_offset`, docs/spec-notes/actors.md §4): the offset image,
//! upsampled bilinearly over the texture and flipped vertically (its first row is the
//! texture's bottom row), is added to the texel's stored sRGB value (0–255) and held to
//! 0–255, then the sum goes back to linear [hypothesis, as the CPU rule]. It differs from
//! adding to the stored bytes only in that the texture is filtered first: no rounding to
//! whole units, and the hold to 0–255 comes after filtering, not per texel
//! [engineering choice: a small per-NPC offset image in place of a full-size texture per
//! NPC, docs/design/streaming-hitches.md §3]. Every other shape has no offset and the
//! shader is unchanged (the `FACEGEN_TINT` shader def).
//!
//! First-person depth: the player's arms and weapon in view and the flash at their muzzle
//! ([`NifSurface::first_person`], set by nvforever `player_body.rs` and `shot_fx.rs`) are
//! drawn against the same depth buffer as the world, but with their depth squeezed into
//! the nearest [`FIRST_PERSON_DEPTH_SHARE`] of the depth range (the fragment shaders write
//! it, `first_person_depth.wesl`): no wall, door or actor the camera stands against hides
//! them, they still hide each other in their own order, and they keep the one camera's
//! lighting, shadows, fog, multisampling and image space; their nodes stay where they are
//! (muzzle, shell ejection, the weapon-lowering rays). The depth prepass holds the squeezed
//! depth too, so water and blended world shapes are not drawn over them; the passes that
//! read the prepass as distances undo it (soft intersections here, nvforever `ssao.wesl`)
//! [engineering choice, docs/design/first-person-depth.md].
//!
//! Bindless [engineering choice, docs/design/graphics-enhanced.md §5.6]: where the device
//! supports it (Bevy decides; `BINDLESS` in the shaders), every NIF material's values and
//! textures are reached through index tables, so the draws of shapes with different
//! materials share a batch set instead of each binding a bind group of its own: the
//! alpha-tested shapes in every shadow cascade above all. The shaders reach the values,
//! the holes and the FaceGen offset through `nif_surface`, `holes_alpha` and
//! `facegen_texel` (nif_material.wesl, nif_prepass.wesl), which read either binding.

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline, MaterialPlugin, MeshPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType, CompareFunction, DepthBiasState, FragmentState,RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::{ShaderDefVal, ShaderRef};

/// fade_prepass.wesl.
const FADE_PREPASS_SHADER: Handle<Shader> = uuid_handle!("6f0c9b51-2a7e-4f43-9d0e-5b8c1e7a4d21");

/// nif_prepass.wesl.
const NIF_PREPASS_SHADER: Handle<Shader> = uuid_handle!("b1d6a2f4-7c35-4e0a-8f29-3a5d9c0e6b17");

/// nif_surface_bindings.wesl (imported by the two above).
const NIF_SURFACE_BINDINGS_SHADER: Handle<Shader> = uuid_handle!("3c8e1f60-9a4d-4b27-b5e2-7d0f6a91c348");

/// first_person_depth.wesl.
const FIRST_PERSON_DEPTH_SHADER: Handle<Shader> = uuid_handle!("9e4a7c12-58d3-4f6b-a0e1-2c7b5d8f3a64");

/// Make a depth-only prepass pipeline of a crossfading mesh dither as the main pass does
/// (module docs). Call from `MaterialExtension::specialize` of materials whose meshes
/// can fade; other pipelines are left alone.
pub fn fade_prepass(descriptor: &mut RenderPipelineDescriptor, mesh_key: MeshPipelineKey) {
    if !mesh_key.contains(MeshPipelineKey::VISIBILITY_RANGE_DITHER) {
        return;
    }
    depth_prepass_fragment(descriptor, FADE_PREPASS_SHADER);
}

/// The depth-only prepass fragment of NIF shapes (module docs, "Vertex alpha"): the
/// dither of [`fade_prepass`] and the main pass's alpha test, vertex alpha and
/// [`NifSurface::alpha_cut`] included. Pipelines that neither dither nor discard keep
/// having no fragment stage.
fn nif_prepass(descriptor: &mut RenderPipelineDescriptor, mesh_key: MeshPipelineKey) {
    if !mesh_key.intersects(MeshPipelineKey::VISIBILITY_RANGE_DITHER | MeshPipelineKey::MAY_DISCARD) {
        return;
    }
    depth_prepass_fragment(descriptor, NIF_PREPASS_SHADER);
}

/// Give a depth-only or motion-vector prepass pipeline `shader` as its fragment stage.
/// NIF alpha/holes and first-person depth must be identical with and without FSR.
fn depth_prepass_fragment(descriptor: &mut RenderPipelineDescriptor, shader: Handle<Shader>) {
    let defs = &descriptor.vertex.shader_defs;
    let has = |d: &'static str| defs.contains(&ShaderDefVal::from(d));
    // The fade-only shader is depth-only; Bevy already handles its dither with colour
    // targets. NIF's motion variant also preserves its custom alpha and squeezed depth.
    let nif_motion = shader == NIF_PREPASS_SHADER && has("MOTION_VECTOR_PREPASS") && !has("NORMAL_PREPASS") && !has("DEFERRED_PREPASS");
    if !has("PREPASS_PIPELINE") || (has("PREPASS_FRAGMENT") && !nif_motion) {
        return;
    }
    let (shader_defs, targets) = match descriptor.fragment.take() {
        Some(f) => (f.shader_defs, f.targets),
        None => (defs.clone(), Vec::new()),
    };
    descriptor.fragment = Some(FragmentState { shader, shader_defs, targets, ..default() });
}

/// The pipeline of a first-person shape (module notes, "First-person depth"): in the main
/// pass and the view's depth-only prepass of a perspective view, the fragment shader gets
/// `FIRST_PERSON_DEPTH` and writes its depth squeezed; the prepass gets the NIF prepass
/// fragment whatever the shape's alpha, to write it. Orthographic views (the
/// [`NifSurface::shade_light`] light's shadow cascade, which the arms cast into) and
/// normal/deferred prepasses are left alone; the motion-vector prepass is supported.
fn first_person_depth(descriptor: &mut RenderPipelineDescriptor, mesh_key: MeshPipelineKey) {
    if mesh_key & MeshPipelineKey::VIEW_PROJECTION_RESERVED_BITS != MeshPipelineKey::VIEW_PROJECTION_PERSPECTIVE {
        return;
    }
    let defs = &descriptor.vertex.shader_defs;
    let prepass = defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE"));
    if prepass {
        if defs.contains(&ShaderDefVal::from("PREPASS_FRAGMENT"))
            && (!defs.contains(&ShaderDefVal::from("MOTION_VECTOR_PREPASS"))
                || defs.contains(&ShaderDefVal::from("NORMAL_PREPASS"))
                || defs.contains(&ShaderDefVal::from("DEFERRED_PREPASS")))
        {
            return;
        }
        depth_prepass_fragment(descriptor, NIF_PREPASS_SHADER);
    }
    if let Some(fragment) = descriptor.fragment.as_mut() {
        fragment.shader_defs.push("FIRST_PERSON_DEPTH".into());
    }
}

/// The share of the depth range the depth of first-person shapes is squeezed into
/// (module notes, "First-person depth"; first_person_depth.wesl, the same value). Reverse
/// z puts the near plane at 1, so they lie in [1 − share, 1]: a world surface hides them
/// only nearer than near ÷ (1 − share) (5.08 units at the main camera's near plane of 5,
/// which clips everything nearer than 5), and a share of 1/64 still leaves their own
/// order about 4·10⁻⁵ of depth per 2 units at 60 units away, some 600 steps of a 32-bit
/// float depth buffer near 1 [engineering choice].
pub const FIRST_PERSON_DEPTH_SHARE: f32 = 1.0 / 64.0;

/// The depth a first-person fragment at depth `z` writes (first_person_depth.wesl
/// `squeeze`).
pub fn first_person_squeeze(z: f32) -> f32 {
    1.0 - FIRST_PERSON_DEPTH_SHARE * (1.0 - z)
}

/// Anti-alias the cut-out edges of an alpha-tested material in the main pass of a
/// multisampled view: the fragment shader gets `SOFT_EDGES` and must write, as its alpha,
/// the share of the pixel's samples it covers (alpha to coverage). Call from
/// `MaterialExtension::specialize`; pipelines of other materials and passes, and views
/// without multisampling, are left alone (the alpha is tested per pixel there).
///
/// The share the shaders write is 1 at and above the material's cutoff and falls to 0
/// within one pixel outside it: `(alpha - cutoff) / fwidth(alpha) + 1`. The depth prepass
/// tests at the cutoff, so every pixel it has written depth for is fully covered and
/// nothing behind the shape is wrongly hidden [chosen: Bevy's own alpha-to-coverage mode
/// centres the edge on the cutoff and drops the prepass's alpha test].
pub fn soft_edges(descriptor: &mut RenderPipelineDescriptor, mesh_key: MeshPipelineKey) {
    if !mesh_key.contains(MeshPipelineKey::MAY_DISCARD) || descriptor.multisample.count <= 1 {
        return;
    }
    if descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
        return;
    }
    if let Some(fragment) = descriptor.fragment.as_mut() {
        fragment.shader_defs.push("SOFT_EDGES".into());
        descriptor.multisample.alpha_to_coverage_enabled = true;
    }
}

/// The extension part: the falloff parameters, the alpha cut, the decal switch and the
/// fragment shader.
///
/// Bindless where the device allows it, as `StandardMaterial` is (module docs,
/// "Bindless"): the values go to one array for all NIF materials ([`NifSurfaceUniform`],
/// binding 101, index 50 of the extension's index table at binding 100) and the textures
/// to Bevy's shared texture arrays, so shapes of different materials are drawn in one
/// batch. Without bindless the values are a uniform at binding 50 and the textures at 51
/// to 53, Bevy's numbering for extensions.
///
/// A slab holds at most 512 resources (Bevy's own limit on desktop is 2,048): a slab is
/// sent to the GPU whole when any material in it changes, and animated or hour-driven
/// materials change every frame, more of them at night; with the smaller slabs the render
/// graph took 30-40 % less time and the material writes about half, on two-lap tours with
/// the owner's settings (docs/design/streaming-hitches.md §3.1) [observed 2026-10-28].
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[data(50, NifSurfaceUniform, binding_array(101))]
#[bindless(limit(512), index_table(range(50..54), binding(100)))]
#[bind_group_data(NifSurfaceKey)]
pub struct NifSurface {
    /// Start angle, stop angle (cosines), start opacity, stop opacity; constant opacity 1
    /// when the shape has no falloff ([`Falloff::NONE`]).
    pub falloff: Vec4,
    /// Fragments whose alpha is below this are not drawn: the alpha test of a shape that
    /// is drawn blended (`nif_spawn::blend_cut`); 0 on every other shape, which drops
    /// nothing.
    pub alpha_cut: f32,
    /// 1 for an additive shape whose source factor is ONE (NiAlphaProperty ONE / ONE): its
    /// colour is added whole, whatever its alpha [documented: nif.xml AlphaFlags, source
    /// blend mode 0 ONE]; the view-angle falloff still fades it [hypothesis]. 0 for every
    /// other shape (an additive SRC_ALPHA / ONE shape adds colour × alpha, [`add_display`]).
    pub add_whole: f32,
    /// The refraction strength of a refracting shape as the model stores or keys it (module
    /// notes, "Refraction"): × [`REFRACTION_SCREEN`] the screen shift per unit of its
    /// view-space normal, in screen UV; 0 for every other shape.
    pub refraction: f32,
    /// Seconds for the normal map of a refracting shape to scroll once up its texture,
    /// negative for down (module notes, "Fire refraction"); 0: it does not move.
    pub fire_period: f32,
    /// Unlit emissive rules: x 1 skips the image-space scale; y holds each light channel
    /// to that value on the soft path (0: no hold); z, w unused. Zero keeps the default
    /// [hypothesis, nif.md §6 "Unlit shapes that are pictures, and cut-out signs"].
    pub emissive_rule: Vec4,
    /// The shape refracts (module notes, "Refraction").
    pub refract: bool,
    /// The shape lies on another surface (a decal shader flag is set): drawn with
    /// [`decal_bias`].
    pub decal: bool,
    /// An alpha-tested shape whose edges stay hard ([`soft_edges`] is not applied).
    pub hard_edges: bool,
    /// The shape's sun shadows are those of the shade light (module docs, "Shade light"),
    /// where the view has one.
    pub shade_light: bool,
    /// A blended shape that writes depth in the main pass (module notes, "Depth of blended
    /// hair"); its [`alpha_cut`](Self::alpha_cut) keeps faint fragments out of the depth.
    pub depth_write: bool,
    /// A first-person shape (the arms and weapon in view, the flash at their muzzle):
    /// drawn with its depth squeezed in front of the world (module notes, "First-person
    /// depth").
    pub first_person: bool,
    /// An additive shape that adds its colour in linear light, not by [`add_display`]: the
    /// Pip-Boy's glare over the menus (nvforever `pipboy_arm.rs`), which has to stay on the
    /// arms' squeezed depth, so Bevy's own unlit additive material cannot draw it [design].
    pub add_linear: bool,
    /// The Pip-Boy's screen (nvforever `pipboy_arm.rs`): red stops at a ceiling after the
    /// tint, so the scanlines dim green and blue and leave bright red where it is, and its
    /// pixels are marked in alpha so the image-space grade leaves them alone
    /// (`imagespace.wesl`, `SCREEN_MARK`).
    pub pipboy_screen: bool,
    /// A blended shape that draws the same as a cut-out (`nif_spawn::blended_cutout`): its
    /// alpha is a texture's cut-out mask and nothing else. Where [`BlendedCutouts`] is on
    /// (FSR's temporal upscaling) it is drawn alpha-tested at this cut, so it has depth
    /// and motion and is not translucent to the upscaler; otherwise blended. CPU only.
    pub blended_cutout: Option<f32>,
    /// Holes (module notes, "Holes"): x the value in alpha units (0–255) at and above
    /// which the holes texture's alpha leaves a hole, negative for none; y the width of
    /// the edge band below it; z 1 turns the test round (a hole where the alpha is
    /// below x, no band: an effect shader's overlay on an alpha-tested mesh, cut by the
    /// mesh's own texture), 0 otherwise; w unused.
    pub holes: Vec4,
    /// The colour of the holes' edge band (linear, alpha unused).
    pub holes_edge: Vec4,
    /// Soft intersections (module notes, "Soft intersections"): x the distance in game
    /// units over which the shape fades out towards the opaque surface behind it, 0 for
    /// none; y, z, w unused.
    pub soft: Vec4,
    /// The holes texture; `None` with no holes.
    #[texture(51)]
    #[sampler(52)]
    pub holes_texture: Option<Handle<Image>>,
    /// FaceGen tint (module notes, "FaceGen tint"): the colour offset in colour units
    /// (0–255 scale), one texel per `.egt` image texel, in the file's row order (first row
    /// at the texture's bottom); read with `textureLoad`, so it is not filtered. `None` on
    /// every shape but an NPC's face and skin.
    /// Read with `textureLoad` only, but declared filterable: Bevy's bindless texture
    /// arrays hold filterable textures, and the offsets are Rgba16Float, which is.
    #[texture(53)]
    pub facegen_offset: Option<Handle<Image>>,
}

/// The values of a [`NifSurface`] the shaders read (`nif_surface_bindings.wesl`, shared
/// by nif_material.wesl and nif_prepass.wesl: the same fields in the same order).
#[derive(Clone, Default, ShaderType)]
pub struct NifSurfaceUniform {
    pub falloff: Vec4,
    pub alpha_cut: f32,
    pub add_whole: f32,
    pub refraction: f32,
    pub fire_period: f32,
    /// The image-space bypass and plain path of [`NifSurface::emissive_rule`].
    pub emissive_rule: Vec4,
    pub holes: Vec4,
    pub holes_edge: Vec4,
    pub soft: Vec4,
}

/// Screen UV of shift per unit of refraction strength and of view-space tilt
/// [hypothesis, docs/spec-notes/refraction.md §6]. The models store strengths from 0.015
/// (the Ghoul body glow) to 1.0 (the HELIOS One beam, the displacer impact); read as screen
/// UV whole, 1.0 would move the picture by a screen width and the Incinerator's burst (keyed
/// to 0.156) drew a plain lens-like dome over the flames (owner, 2026-10-25).
pub const REFRACTION_SCREEN: f32 = 0.1;

impl From<&NifSurface> for NifSurfaceUniform {
    fn from(s: &NifSurface) -> Self {
        NifSurfaceUniform { falloff: s.falloff, alpha_cut: s.alpha_cut, add_whole: s.add_whole, refraction: s.refraction * REFRACTION_SCREEN, fire_period: s.fire_period, emissive_rule: s.emissive_rule, holes: s.holes, holes_edge: s.holes_edge, soft: s.soft }
    }
}

/// Game units over which a soft-intersecting shape fades out towards the surface behind
/// it (module notes, "Soft intersections") [engineering choice, judged on the Strip's
/// lamp beams against the owner's game frame].
pub const SOFT_DISTANCE: f32 = 160.0;

/// Share of a shape's size (the largest side of its box) that its soft intersection may
/// span: a glow card a few dozen units across around a lantern's glass (Dead Money's
/// `nvdlc01_sconceon.nif`, 58 units) faded over 160 units was a dim card with a visible
/// seam where it meets the glass, while the original's glow is seamless [engineering
/// choice, owner's frame of the lantern; nif.md §6 "Soft intersections"].
pub const SOFT_SIZE_SHARE: f32 = 0.25;

/// The fade distance of a soft-intersecting shape whose largest side is `extent` units:
/// [`SOFT_DISTANCE`], or [`SOFT_SIZE_SHARE`] of the shape when that is less. A shape of
/// unknown size (extent 0) keeps [`SOFT_DISTANCE`].
pub fn soft_distance(extent: f32) -> f32 {
    if extent <= 0.0 {
        return SOFT_DISTANCE;
    }
    (extent * SOFT_SIZE_SHARE).clamp(1.0, SOFT_DISTANCE)
}

/// Whether a shape of this alpha mode fades out where it meets the surface behind it
/// (module notes, "Soft intersections"): unlit, added or blended (not multiplied: those
/// darken what they lie on), and not refracting. Shapes with the decal flags are
/// included: a decal laid on a surface lies along it and is not faded, while Dead Money's
/// lantern glow (three crossed discs through the glass, decal flags set) must fade where
/// it comes out of a pane, or the pane shows a hard seam (nif.md §6 "Soft intersections
/// of small shapes"). Not burning shapes either (emissive colour × multiplier over 1 in
/// its brightest channel: neon tubes and glowing signs mounted on a building): Gomorrah's
/// porch tube (`OverHangGlow`, × 15) is tilted 45° a few units off the fascia and faded
/// almost to nothing, where the original burns it bright with a halo (owner's frames
/// 2026-10-25, nif.md §6 "Soft intersections").
pub fn soft_intersections(alpha_mode: AlphaMode, unlit: bool, refracts: bool, burning: bool) -> bool {
    unlit && !refracts && !burning && matches!(alpha_mode, AlphaMode::Add | AlphaMode::Blend | AlphaMode::Premultiplied)
}

/// [`NifSurface::holes`] of a shape with no holes.
pub const NO_HOLES: Vec4 = Vec4::new(-1.0, 0.0, 0.0, 0.0);

impl Default for NifSurface {
    fn default() -> Self {
        NifSurface {
            falloff: Falloff::NONE.params,
            alpha_cut: 0.0,
            add_whole: 0.0,
            refraction: 0.0,
            fire_period: 0.0,
            emissive_rule: Vec4::ZERO,
            refract: false,
            decal: false,
            hard_edges: false,
            shade_light: false,
            depth_write: false,
            first_person: false,
            add_linear: false,
            pipboy_screen: false,
            blended_cutout: None,
            holes: NO_HOLES,
            holes_edge: Vec4::ONE,
            soft: Vec4::ZERO,
            holes_texture: None,
            facegen_offset: None,
        }
    }
}

/// The part of [`NifSurface`] that selects the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NifSurfaceKey {
    pub decal: bool,
    pub refract: bool,
    pub hard_edges: bool,
    pub shade_light: bool,
    /// The shape has a FaceGen offset ([`NifSurface::facegen_offset`]).
    pub facegen: bool,
    pub depth_write: bool,
    pub first_person: bool,
    pub add_linear: bool,
    pub pipboy_screen: bool,
}

impl From<&NifSurface> for NifSurfaceKey {
    fn from(surface: &NifSurface) -> Self {
        NifSurfaceKey {
            decal: surface.decal,
            refract: surface.refract,
            hard_edges: surface.hard_edges,
            shade_light: surface.shade_light,
            facegen: surface.facegen_offset.is_some(),
            depth_write: surface.depth_write,
            first_person: surface.first_person,
            add_linear: surface.add_linear,
            pipboy_screen: surface.pipboy_screen,
        }
    }
}

impl MaterialExtension for NifSurface {
    fn fragment_shader() -> ShaderRef {
        "embedded://nvf_assets/nif_material.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://nvf_assets/nif_material.wesl".into()
    }

    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        nif_prepass(descriptor, key.mesh_key);
        if key.bind_group_data.first_person {
            first_person_depth(descriptor, key.mesh_key);
        }
        if !key.bind_group_data.hard_edges {
            soft_edges(descriptor, key.mesh_key);
        }
        if key.bind_group_data.shade_light && !descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("SHADE_LIGHT".into());
            }
        }
        if key.bind_group_data.facegen && !descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("FACEGEN_TINT".into());
            }
        }
        if key.bind_group_data.add_linear && !descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("ADD_LINEAR".into());
            }
        }
        if key.bind_group_data.pipboy_screen && !descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("PIPBOY_SCREEN".into());
            }
        }
        if key.bind_group_data.refract {
            refraction_pipeline(descriptor);
        } else if key.bind_group_data.depth_write && !descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
            if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
                depth_stencil.depth_write_enabled = Some(true);
            }
        }
        // StandardMaterial has set the rasteriser's bias from the material's depth bias.
        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.bias.constant = raster_bias(depth_stencil.bias.constant);
            if key.bind_group_data.decal {
                depth_stencil.bias = decal_bias(depth_stencil.bias);
            }
        }
        Ok(())
    }
}

/// The pipeline of a refracting shape (module notes, "Refraction"): in the main pass the
/// shader gets `REFRACT` and no depth is written; a depth-only pass (the shadow maps; Bevy
/// leaves such materials out of the view's prepass) passes no fragment.
fn refraction_pipeline(descriptor: &mut RenderPipelineDescriptor) {
    let prepass = descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE"));
    if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
        depth_stencil.depth_write_enabled = Some(false);
        if prepass {
            depth_stencil.depth_compare = Some(CompareFunction::Never);
        }
    }
    if !prepass {
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.push("REFRACT".into());
        }
    }
}

/// Sort-only part of the depth bias of blended meshes that are drawn before every other
/// blended mesh of the world ([`sort_first`]). Bevy sorts blended meshes by view distance
/// plus the material's depth bias, ascending, and draws them in that order
/// [documented: bevy_core_pipeline 0.19 Transparent3d]; the sky uses −1e6 (nvforever
/// `sky.rs`), so these come after the sky and before the rest.
pub const SORT_FIRST_BIAS: f32 = -5.0e5;

/// The material depth bias of a blended mesh that lies on an opaque surface and is drawn
/// before every other blended mesh, with `bias` as its bias in the depth buffer (terrain
/// texture layers). Blended meshes write no depth: one drawn before such a layer would be
/// painted over by it wherever the layer sorts as the nearer of the two.
pub fn sort_first(bias: f32) -> f32 {
    SORT_FIRST_BIAS + bias
}

/// Whether a blended mesh's transparent sort key (view distance plus material depth bias,
/// as Bevy sorts them) lies in the [`sort_first`] band: after the sky's −1e6 and before
/// every other blended mesh. Half the bias on either side leaves room for every view
/// distance in the loaded world; the same half marks the band in [`raster_bias`].
pub fn sorts_first(sort_key: f32) -> bool {
    sort_key > SORT_FIRST_BIAS * 1.5 && sort_key <= SORT_FIRST_BIAS / 2.0
}

/// The rasteriser's depth bias for a material depth bias: Bevy uses one number for the
/// draw order and for the depth buffer, so the sort-only part of [`sort_first`] is taken
/// out again.
pub fn raster_bias(constant: i32) -> i32 {
    if constant <= (SORT_FIRST_BIAS / 2.0) as i32 {
        constant - SORT_FIRST_BIAS as i32
    } else {
        constant
    }
}

/// Constant part of [`decal_bias`], in steps of the depth buffer at the shape's depth
/// (2⁻²³ of the depth value for the 32-bit float buffer). With depth = near / distance
/// that is at most 64 · 2⁻²³ ≈ 7.6e-6 of the distance to the camera: 0.008 units at
/// 1,000 units, far less than the 0.05 to 1 unit the decals are lifted off their surface.
pub const DECAL_BIAS: i32 = 64;

/// Slope part of [`decal_bias`]: one pixel's change of depth across the triangle, which
/// covers the difference between a decal and a surface triangulated differently.
pub const DECAL_SLOPE_BIAS: f32 = 1.0;

/// The rasteriser's depth bias for a shape with a decal shader flag, added to `bias`: a
/// small offset towards the camera (depth is reversed, so a positive bias is nearer), so
/// that the decal wins the depth test against the surface it lies on whichever is drawn
/// first [hypothesis, docs/spec-notes/nif.md §6 "Decal flags"]. Only the depth buffer is
/// concerned: the draw order of blended decals stays with their distance.
pub fn decal_bias(bias: DepthBiasState) -> DepthBiasState {
    DepthBiasState { constant: bias.constant + DECAL_BIAS, slope_scale: bias.slope_scale + DECAL_SLOPE_BIAS, clamp: bias.clamp }
}

pub type NifMaterial = ExtendedMaterial<StandardMaterial, NifSurface>;

/// A NIF material from its StandardMaterial part.
pub fn nif_material(base: StandardMaterial) -> NifMaterial {
    NifMaterial { base, extension: NifSurface::default() }
}

/// View-angle falloff as stored in BSShaderNoLightingProperty: start angle and stop
/// angle (cosines), start opacity and stop opacity [documented: nif.xml].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Falloff {
    pub params: Vec4,
}

impl Falloff {
    /// No falloff: opacity 1 at every angle.
    pub const NONE: Falloff = Falloff { params: Vec4::new(1.0, 0.0, 1.0, 1.0) };

    /// The falloff to draw for the stored fields, or `None` when it changes nothing or
    /// is degenerate. A start angle equal to the stop angle gives no range to interpolate
    /// over; 166 shapes store all four fields as 0 (e.g. the vertex-coloured quads of
    /// `offpitwall01.nif`), which read literally would hide them entirely [hypothesis: the
    /// game skips the falloff for an empty range]. Constant opacity 1 is no falloff.
    pub fn from_fields(start_angle: f32, stop_angle: f32, start_opacity: f32, stop_opacity: f32) -> Option<Falloff> {
        let p = Vec4::new(start_angle, stop_angle, start_opacity, stop_opacity);
        if !p.is_finite() || (start_angle - stop_angle).abs() < 1e-4 || (start_opacity == 1.0 && stop_opacity == 1.0) {
            return None;
        }
        Some(Falloff { params: p })
    }

    /// Opacity at the cosine `c` of the view angle: linear in the cosine between the
    /// start and stop angles, held beyond them (CPU mirror of nif_material.wesl).
    pub fn opacity(&self, c: f32) -> f32 {
        let p = self.params;
        let t = ((c - p.y) / (p.x - p.y)).clamp(0.0, 1.0);
        p.w + (p.z - p.w) * t
    }
}

/// Display gamma of [`multiply_factor`], [`display_emissive`] and [`add_display`]: a plain power law, so a
/// product of display values is the product of the linear values.
pub const MULTIPLY_GAMMA: f32 = 2.2;

/// The factor a multiply-blended shape (NiAlphaProperty source factor ZERO, destination
/// factor SRC_COLOR: result = destination × source colour [documented: nif.xml
/// AlphaFlags]) puts on what lies behind it, for the shape's lit colour and emissive
/// light, both linear (CPU mirror of nif_material.wesl).
///
/// The game's shader output is taken as texture × (light + emissive colour × emissive
/// multiplier) in display values, held to 0..1 when written to the frame [hypothesis,
/// docs/spec-notes/nif.md §6 "Multiply blend"]. So both parts are summed as display
/// values and the sum is limited: a multiply can only darken.
pub fn multiply_factor(lit: Vec3, emissive: Vec3) -> Vec3 {
    let display = lit.max(Vec3::ZERO).powf(1.0 / MULTIPLY_GAMMA) + emissive.max(Vec3::ZERO).powf(1.0 / MULTIPLY_GAMMA);
    display.clamp(Vec3::ZERO, Vec3::ONE).powf(MULTIPLY_GAMMA)
}

/// Whether a shape of this alpha mode forms its emissive light in display values, so
/// its stored emissive colour × multiplier is raised to the display gamma
/// ([`display_emissive`]): multiply-blended shapes (nif.md §6 "Multiply blend") and
/// additive ones (nif.md §6 "Additive blend").
pub fn emissive_in_display(alpha_mode: AlphaMode) -> bool {
    matches!(alpha_mode, AlphaMode::Multiply | AlphaMode::Add)
}

/// The linear emissive light of a shape whose light is formed in display values
/// ([`emissive_in_display`]), for a NiMaterialProperty emissive colour × multiplier
/// `value`: `value` raised to the display gamma. The shader multiplies it by the linear
/// texture colour, and [`multiply_factor`] or [`add_display`] take the product back to
/// display values, which gives texture (display) × `value`, the game's term
/// [hypothesis].
pub fn display_emissive(value: f32) -> f32 {
    value.max(0.0).powf(MULTIPLY_GAMMA)
}

/// The largest display value the sum of a frame and an additive shape reaches in
/// [`add_display`] [engineering choice: it keeps a shape of a very large emissive
/// multiplier (the Securitron's screen flash, 0.99 × 30) from adding thousands of times
/// white in the HDR view, while the Big MT's sky beams (display 1.5 at the core) pass
/// whole].
pub const ADD_DISPLAY_MAX: f32 = 2.0;

/// The linear light an additive shape (NiAlphaProperty destination factor ONE, drawn as
/// `AlphaMode::Add`) adds to a pixel whose opaque frame is `behind` (linear), for its
/// linear colour `colour` and its alpha `alpha` (1 for a ONE / ONE shape; CPU mirror of
/// nif_material.wesl).
///
/// The game adds colour × alpha to the frame's display values [hypothesis,
/// docs/spec-notes/nif.md §6 "Additive blend"]. Bevy adds linear light, so the shader
/// reads the frame behind the shape (Bevy's copy of the opaque frame, which
/// [`TransmissionAnchor`] keeps in every view), adds in display values and returns the
/// difference in linear light. The sum is held to [`ADD_DISPLAY_MAX`] (or the frame's
/// own value, if brighter).
pub fn add_display(colour: Vec3, alpha: f32, behind: Vec3) -> Vec3 {
    let behind = behind.max(Vec3::ZERO);
    let shown = behind.powf(1.0 / MULTIPLY_GAMMA);
    let added = colour.max(Vec3::ZERO).powf(1.0 / MULTIPLY_GAMMA) * alpha.clamp(0.0, 1.0);
    let sum = (shown + added).min(shown.max(Vec3::splat(ADD_DISPLAY_MAX)));
    (sum.powf(MULTIPLY_GAMMA) - behind).max(Vec3::ZERO)
}

/// Power on the alpha in the colour term of unlit alpha-blended shapes
/// ([`blend_colour_scale`]; nif_material.wesl has the same number) [hypothesis, fitted:
/// docs/spec-notes/nif.md §6 "Alpha blend of unlit shapes"].
pub const BLEND_ALPHA_POWER: f32 = 1.5;

/// The factor on the colour of an unlit alpha-blended shape (smoke, mist, dust, every
/// particle system; NiAlphaProperty SRC_ALPHA / INV_SRC_ALPHA) of alpha `a` (CPU mirror
/// of nif_material.wesl).
///
/// The game blends colour × alpha + behind × (1 − alpha) in display values
/// [hypothesis, docs/spec-notes/nif.md §6 "Alpha blend of unlit shapes"]; Bevy does it
/// in linear light, where light smoke over a dark room shows about twice as bright
/// (owner's Bison Steve lobby screenshots, 2026-10-23). Scaled by √alpha, the colour
/// term becomes colour × alpha^[`BLEND_ALPHA_POWER`], fitted against the display-value
/// blend; the share of what lies behind is kept.
pub fn blend_colour_scale(a: f32) -> f32 {
    a.clamp(0.0, 1.0).sqrt()
}

pub struct NifMaterialPlugin;

impl Plugin for NifMaterialPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "nif_material.wesl");
        // WESL modules named by their embedded path, so the other shaders import them as
        // `nvf_assets::<file>` (Bevy 0.20 takes the import path from it).
        load_internal_asset!(app, FADE_PREPASS_SHADER, "fade_prepass.wesl", |s, _| Shader::from_wesl(s, "embedded://nvf_assets/fade_prepass.wesl"));
        load_internal_asset!(app, NIF_SURFACE_BINDINGS_SHADER, "nif_surface_bindings.wesl", |s, _| Shader::from_wesl(s, "embedded://nvf_assets/nif_surface_bindings.wesl"));
        load_internal_asset!(app, NIF_PREPASS_SHADER, "nif_prepass.wesl", |s, _| Shader::from_wesl(s, "embedded://nvf_assets/nif_prepass.wesl"));
        load_internal_asset!(app, FIRST_PERSON_DEPTH_SHADER, "first_person_depth.wesl", |s, _| Shader::from_wesl(s, "embedded://nvf_assets/first_person_depth.wesl"));
        // Every 3D camera gets the view transmission texture, as each one had before Bevy
        // 0.20 made `ScreenSpaceTransmission` opt-in: the refraction here and the water
        // (nvforever water.rs) read it, and their shaders bind it unconditionally.
        app.register_required_components::<Camera3d, bevy::pbr::ScreenSpaceTransmission>();
        app.add_plugins(MaterialPlugin::<NifMaterial>::default())
            .init_resource::<BlendedCutouts>()
            .add_systems(Startup, spawn_transmission_anchor)
            .add_systems(PostUpdate, (apply_blended_cutouts, see_through_casts_no_shadow));
    }
}

/// Keeps Bevy's copy of the opaque frame (the view transmission texture) in every 3D
/// view, for additive shapes to read what lies behind them ([`add_display`]). Bevy makes
/// the copy only for a view with something in its transmissive phase (an opaque material
/// that reads the texture: water, refracting shapes); without it the shader reads a white
/// stand-in. This is such a material on a triangle of no area, in every render layer and
/// never culled, so it draws nothing [engineering choice; cost: one copy of the frame per
/// view and frame, which a view with water in sight made already].
#[derive(Component)]
pub struct TransmissionAnchor;

/// Render layers the anchor is in: every layer a view of the game uses.
const ANCHOR_LAYERS: usize = 32;

fn spawn_transmission_anchor(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<NifMaterial>>) {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::PrimitiveTopology;
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 3])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    // Opaque and reading the texture (specular transmission), so in the transmissive
    // phase; the refraction pipeline writes no depth, and every fragment would fall below
    // the cut.
    let material = NifMaterial {
        base: StandardMaterial { base_color: Color::NONE, unlit: true, specular_transmission: 1.0, ..default() },
        extension: NifSurface { refract: true, alpha_cut: 1.0, ..default() },
    };
    commands.spawn((
        Name::new("Transmission anchor"),
        TransmissionAnchor,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(material)),
        Transform::IDENTITY,
        Visibility::Visible,
        bevy::camera::visibility::NoFrustumCulling,
        bevy::camera::visibility::RenderLayers::from_layers(&(0..ANCHOR_LAYERS).collect::<Vec<_>>()),
        bevy::light::NotShadowCaster,
        bevy::light::NotShadowReceiver,
    ));
}

/// Whether a material is see-through: blended, added or multiplied onto what lies behind
/// it (glass, light rays, glows, smoke, hair), and not an alpha-tested cut-out with soft
/// edges (`NifSurface::alpha_cut`), which is solid where it is drawn. Hair's cut only
/// keeps faint fragments out of its depth ([`NifSurface::depth_write`]); it stays
/// see-through, as before that cut. A blended cut-out ([`NifSurface::blended_cutout`])
/// stays see-through while [`BlendedCutouts`] draws it alpha-tested, so its shadows do
/// not change with the upscaler.
pub fn see_through(m: &NifMaterial) -> bool {
    m.extension.blended_cutout.is_some()
        || matches!(m.base.alpha_mode, AlphaMode::Blend | AlphaMode::Add | AlphaMode::Multiply | AlphaMode::Premultiplied)
            && (m.extension.alpha_cut <= 0.0 || m.extension.depth_write)
}

/// Whether blended cut-outs ([`NifSurface::blended_cutout`]) are drawn alpha-tested: set by
/// the renderer while FSR upscales the main view (nvforever `graphics.rs`). Temporal
/// upscaling needs the depth and motion of what it accumulates; a blended mesh has
/// neither and its whole area counts as translucent, so FSR kept almost no history of the
/// corn rows and redrew them from each jittered frame: they flickered (owner, 2026-11-05;
/// docs/design/fsr-upscaling.md "Blended cut-outs").
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlendedCutouts(pub bool);

/// The alpha mode of a blended cut-out material while [`BlendedCutouts`] is `on`, or
/// `None` when the material is not one in its own state: copies whose alpha a fade, an
/// effect or a clip drives (their alpha cut or opacity set) are left as they are.
fn blended_cutout_mode(m: &NifMaterial, on: bool) -> Option<AlphaMode> {
    let cut = m.extension.blended_cutout?;
    let own = m.base.base_color.alpha() >= 1.0
        && match m.base.alpha_mode {
            AlphaMode::Blend => m.extension.alpha_cut <= 0.0,
            AlphaMode::Mask(c) => c == cut,
            _ => false,
        };
    own.then_some(if on { AlphaMode::Mask(cut) } else { AlphaMode::Blend })
}

/// Draw the blended cut-outs as [`BlendedCutouts`] says: all of them when it changes, and
/// each material as it is added (models prepared on worker threads come in later).
fn apply_blended_cutouts(
    state: Res<BlendedCutouts>,
    mut added: MessageReader<AssetEvent<NifMaterial>>,
    mut materials: ResMut<Assets<NifMaterial>>,
    mut applied: Local<bool>,
    mut flipped: Local<usize>,
) {
    let on = state.0;
    let toggled = *applied != on;
    let ids: Vec<AssetId<NifMaterial>> = if toggled {
        *applied = on;
        added.clear();
        materials.iter().filter(|(_, m)| m.extension.blended_cutout.is_some()).map(|(id, _)| id).collect()
    } else {
        added.read().filter_map(|e| match e {
            AssetEvent::Added { id } => Some(*id),
            _ => None,
        }).collect()
    };
    let mut changed = 0;
    for id in ids {
        // Read first: taking the asset mutably marks it changed.
        let Some(mode) = materials.get(id).and_then(|m| blended_cutout_mode(m, on)) else { continue };
        if materials.get(id).is_some_and(|m| m.base.alpha_mode != mode) {
            if let Some(mut m) = materials.get_mut(id) {
                m.base.alpha_mode = mode;
                changed += 1;
            }
        }
    }
    if toggled {
        *flipped = 0;
        info!("blended cut-outs: {} ({changed} materials changed)", if on { "drawn alpha-tested for FSR" } else { "drawn blended" });
    }
    if on && *flipped == 0 && changed > 0 && !toggled {
        info!("blended cut-outs: models loaded since are drawn alpha-tested for FSR (first {changed})");
    }
    *flipped += changed;
}

/// Marks a mesh [`see_through_casts_no_shadow`] took the shadow from.
#[derive(Component)]
pub struct SeeThroughNoShadow;

/// See-through shapes cast no shadows, from the sun, the flashlight or any other light
/// (owner, playtest 2026-10-11: "anything see-through should not be generating a
/// shadow"; the Prospector Saloon's window rays and lit lanterns threw the flashlight's
/// shadows). Their depth-only shadow pass drew every fragment with alpha above 0.05
/// (`nif_prepass.wesl`), so a light ray stood in the light like a wall. Checked whenever
/// a mesh gets its material (docs/spec-notes/nif.md §6 "Shadows of see-through shapes").
#[allow(clippy::type_complexity)]
fn see_through_casts_no_shadow(
    mut commands: Commands,
    materials: Res<Assets<NifMaterial>>,
    changed: Query<(Entity, &MeshMaterial3d<NifMaterial>, Has<bevy::light::NotShadowCaster>, Has<SeeThroughNoShadow>), Changed<MeshMaterial3d<NifMaterial>>>,
) {
    for (e, m, no_shadow, ours) in &changed {
        let Some(m) = materials.get(&m.0) else { continue };
        match (see_through(m), no_shadow, ours) {
            (true, false, _) => {
                commands.entity(e).try_insert((bevy::light::NotShadowCaster, SeeThroughNoShadow));
            }
            (false, true, true) => {
                commands.entity(e).try_remove::<(bevy::light::NotShadowCaster, SeeThroughNoShadow)>();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nif_motion_prepass_preserves_targets_alpha_and_first_person_depth() {
        let mut descriptor = RenderPipelineDescriptor {
            vertex: bevy::render::render_resource::VertexState {
                shader_defs: vec!["PREPASS_PIPELINE".into(), "PREPASS_FRAGMENT".into(), "MOTION_VECTOR_PREPASS".into(), "MAY_DISCARD".into()],
                ..default()
            },
            ..default()
        };
        let targets = vec![None, Some(bevy::render::render_resource::ColorTargetState {
            format: bevy::render::render_resource::TextureFormat::Rg16Float,
            blend: None,
            write_mask: bevy::render::render_resource::ColorWrites::ALL,
        })];
        descriptor.fragment = Some(FragmentState { shader_defs: descriptor.vertex.shader_defs.clone(), targets: targets.clone(), ..default() });
        nif_prepass(&mut descriptor, MeshPipelineKey::MAY_DISCARD);
        let fragment = descriptor.fragment.as_ref().unwrap();
        assert_eq!(fragment.shader, NIF_PREPASS_SHADER);
        assert_eq!(fragment.targets, targets);
        first_person_depth(&mut descriptor, MeshPipelineKey::VIEW_PROJECTION_PERSPECTIVE);
        let fragment = descriptor.fragment.as_ref().unwrap();
        assert!(fragment.shader_defs.contains(&"FIRST_PERSON_DEPTH".into()));
        assert_eq!(fragment.targets, targets);
        // A shadow cascade remains unsqueezed, even for the same first-person mesh.
        let mut shadow = RenderPipelineDescriptor {
            vertex: bevy::render::render_resource::VertexState { shader_defs: vec!["PREPASS_PIPELINE".into()], ..default() },
            ..default()
        };
        first_person_depth(&mut shadow, MeshPipelineKey::VIEW_PROJECTION_ORTHOGRAPHIC);
        assert!(shadow.fragment.is_none());
    }

    #[test]
    fn see_through_shapes_are_the_blended_ones_without_a_cut() {
        let with = |alpha_mode: AlphaMode, cut: f32| {
            let mut m = nif_material(StandardMaterial { alpha_mode, ..default() });
            m.extension.alpha_cut = cut;
            see_through(&m)
        };
        assert!(with(AlphaMode::Blend, 0.0));
        assert!(with(AlphaMode::Add, 0.0));
        assert!(with(AlphaMode::Multiply, 0.0));
        assert!(!with(AlphaMode::Blend, 0.5));
        assert!(!with(AlphaMode::Mask(0.5), 0.0));
        assert!(!with(AlphaMode::Opaque, 0.0));
    }

    #[test]
    fn falloff_fields() {
        // fxlightbeambrtw02.nif: opaque facing the camera (within 10°), gone past 80°.
        let beam = Falloff::from_fields(0.985, 0.174, 1.0, 0.0).unwrap();
        assert_eq!(beam.opacity(1.0), 1.0);
        assert_eq!(beam.opacity(0.0), 0.0);
        assert!((beam.opacity((0.985 + 0.174) / 2.0) - 0.5).abs() < 1e-5);
        // Reversed ranges fade in towards the edges (rim glows).
        let rim = Falloff::from_fields(0.866, 0.174, 0.0, 1.0).unwrap();
        assert_eq!(rim.opacity(1.0), 0.0);
        assert_eq!(rim.opacity(0.1), 1.0);
        // The nif.xml defaults are a real falloff: opacity equals the cosine.
        assert!((Falloff::from_fields(1.0, 0.0, 1.0, 0.0).unwrap().opacity(0.3) - 0.3).abs() < 1e-6);
        // Empty ranges and constant full opacity draw without falloff.
        assert_eq!(Falloff::from_fields(0.0, 0.0, 0.0, 0.0), None);
        assert_eq!(Falloff::from_fields(1.0, 0.0, 1.0, 1.0), None);
        assert_eq!(Falloff::from_fields(f32::NAN, 0.0, 1.0, 0.0), None);
        // The default parameters keep full opacity at every angle.
        for c in [0.0, 0.5, 1.0] {
            assert_eq!(Falloff::NONE.opacity(c), 1.0);
        }
    }

    #[test]
    fn blended_cutouts_flip_only_in_their_own_state_and_keep_their_shadows() {
        let mut corn = nif_material(StandardMaterial { alpha_mode: AlphaMode::Blend, ..default() });
        corn.extension.blended_cutout = Some(0.5);
        assert_eq!(blended_cutout_mode(&corn, true), Some(AlphaMode::Mask(0.5)));
        assert_eq!(blended_cutout_mode(&corn, false), Some(AlphaMode::Blend));
        assert!(see_through(&corn), "casts no shadow, as before");
        corn.base.alpha_mode = AlphaMode::Mask(0.5);
        assert_eq!(blended_cutout_mode(&corn, false), Some(AlphaMode::Blend));
        assert!(see_through(&corn), "nor while drawn alpha-tested");
        // Copies a fade or an effect drives: left alone.
        let mut fading = corn.clone();
        fading.base.alpha_mode = AlphaMode::Blend;
        fading.extension.alpha_cut = 0.25;
        assert_eq!(blended_cutout_mode(&fading, true), None);
        let mut faded = nif_material(StandardMaterial { alpha_mode: AlphaMode::Blend, base_color: Color::srgba(1.0, 1.0, 1.0, 0.4), ..default() });
        faded.extension.blended_cutout = Some(0.5);
        assert_eq!(blended_cutout_mode(&faded, true), None);
        let mut other_cut = corn.clone();
        other_cut.base.alpha_mode = AlphaMode::Mask(0.3);
        assert_eq!(blended_cutout_mode(&other_cut, false), None);
        // Unmarked materials are never touched.
        assert_eq!(blended_cutout_mode(&nif_material(StandardMaterial { alpha_mode: AlphaMode::Blend, ..default() }), true), None);
    }

    #[test]
    fn sort_first_bias() {
        // Terrain layers: drawn before a blended mesh thousands of units farther away
        // (Bevy sorts by distance + bias, ascending), in rank order among themselves, and
        // with their plain bias in the depth buffer.
        for rank in 0..8 {
            let bias = 2.0 * (rank as f32 + 1.0);
            let m = sort_first(bias);
            assert!(m + 400_000.0 < 0.0);
            assert!(m > -1.0e6 + 100.0, "after the sky");
            assert!(sort_first(bias + 2.0) > m);
            assert_eq!(raster_bias(m as i32), bias as i32);
            // Seen from anywhere in the loaded cells, nearer or farther than the mesh's centre.
            for distance in [-60_000.0, -4_096.0, 0.0, 4_096.0, 60_000.0] {
                assert!(sorts_first(m + distance));
            }
        }
        // The sky (sky.rs SKY_SORT_BIAS and its layers) and ordinary blended meshes are not
        // in the band.
        for key in [-1.0e6 - 30.0, -1.0e6, -1.0e6 + 60_000.0, -60_000.0, 0.0, 16.0, 60_000.0] {
            assert!(!sorts_first(key), "{key}");
        }
        // Other materials keep their bias (LOD meshes use −50).
        for b in [0, 2, 16, -50, -1000] {
            assert_eq!(raster_bias(b), b);
        }
    }

    #[test]
    fn decal_depth_bias() {
        // A decal is nearer in the depth buffer than a plain shape (reversed depth:
        // positive is nearer), by a constant and by the slope of its triangle.
        let plain = DepthBiasState::default();
        let decal = decal_bias(plain);
        assert!(decal.constant > plain.constant && decal.slope_scale > plain.slope_scale);
        assert_eq!(decal.clamp, plain.clamp);
        // The constant part stays far below the 0.05 units the nearest decals are lifted
        // off their surface: its share of the distance to the camera, at 5,000 units.
        let share = DECAL_BIAS as f32 / (1u32 << 23) as f32;
        assert!(share * 5000.0 < 0.05);
        // It adds to a bias the material already has, and is not a sort-first bias.
        let lod = decal_bias(DepthBiasState { constant: -50, ..default() });
        assert_eq!(lod.constant, DECAL_BIAS - 50);
        assert_eq!(raster_bias(decal.constant), decal.constant);
        // Only the switches in `NifSurfaceKey` select the pipeline; the falloff and the
        // alpha cut are uniforms.
        let mut surface = NifSurface::default();
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: false, refract: false, hard_edges: false, shade_light: false, facegen: false, depth_write: false, first_person: false, add_linear: false, pipboy_screen: false });
        assert_eq!(surface.alpha_cut, 0.0);
        assert_eq!(surface.emissive_rule, Vec4::ZERO);
        assert_eq!(NifSurfaceUniform::from(&surface).emissive_rule, Vec4::ZERO);
        surface.falloff = Vec4::new(0.9, 0.2, 1.0, 0.0);
        surface.alpha_cut = 0.5;
        surface.emissive_rule = Vec4::new(1.0, 1.5, 0.0, 0.0);
        assert_eq!(NifSurfaceUniform::from(&surface).emissive_rule, surface.emissive_rule);
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey::from(&NifSurface::default()));
        surface.decal = true;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: false, hard_edges: false, shade_light: false, facegen: false, depth_write: false, first_person: false, add_linear: false, pipboy_screen: false });
        surface.hard_edges = true;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: false, hard_edges: true, shade_light: false, facegen: false, depth_write: false, first_person: false, add_linear: false, pipboy_screen: false });
        surface.shade_light = true;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: false, hard_edges: true, shade_light: true, facegen: false, depth_write: false, first_person: false, add_linear: false, pipboy_screen: false });
        // The refraction strength is a uniform; whether the shape refracts is the pipeline's.
        surface.refraction = 0.1;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: false, hard_edges: true, shade_light: true, facegen: false, depth_write: false, first_person: false, add_linear: false, pipboy_screen: false });
        surface.refract = true;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: true, hard_edges: true, shade_light: true, facegen: false, depth_write: false, first_person: false, add_linear: false, pipboy_screen: false });
        // A FaceGen offset selects the tinting shader.
        surface.facegen_offset = Some(Handle::default());
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: true, hard_edges: true, shade_light: true, facegen: true, depth_write: false, first_person: false, add_linear: false, pipboy_screen: false });
    }

    /// First-person depth (module notes): the squeeze keeps the near plane at 1, puts the
    /// far end at 1 − share, keeps the order, and is the shader's (same share).
    #[test]
    fn first_person_squeeze_keeps_order_in_front() {
        assert_eq!(first_person_squeeze(1.0), 1.0);
        assert_eq!(first_person_squeeze(0.0), 1.0 - FIRST_PERSON_DEPTH_SHARE);
        // The main camera's near plane is 5: a hand at 15 units, a barrel at 60, a wall
        // at 20 between them. Both stay in front of the wall and in their order.
        let z = |d: f32| 5.0 / d;
        let (hand, barrel, wall) = (first_person_squeeze(z(15.0)), first_person_squeeze(z(60.0)), z(20.0));
        assert!(hand > barrel && barrel > wall);
        // Two units apart at 60 units still differ by many steps of an f32 near 1.
        assert!(first_person_squeeze(z(60.0)) - first_person_squeeze(z(62.0)) > 100.0 * f32::EPSILON);
        assert!(include_str!("first_person_depth.wesl").contains("const FIRST_PERSON_DEPTH_SHARE: f32 = 0.015625;"));
        // The linear add is a pipeline of its own, on the shader's own switch.
        assert!(NifSurfaceKey::from(&NifSurface { add_linear: true, ..default() }).add_linear);
        assert!(!NifSurfaceKey::from(&NifSurface::default()).add_linear);
        assert!(include_str!("nif_material.wesl").contains("@if(ADD_LINEAR)"));
        assert!(NifSurfaceKey::from(&NifSurface { pipboy_screen: true, ..default() }).pipboy_screen);
        assert!(!NifSurfaceKey::from(&NifSurface::default()).pipboy_screen);
        assert!(include_str!("nif_material.wesl").contains("@if(PIPBOY_SCREEN)"));
        assert_eq!(FIRST_PERSON_DEPTH_SHARE, 0.015625);
    }

    /// CPU mirror of the shaders' edge formula (nif_material.wesl, tree.wesl).
    fn edge(alpha: f32, cutoff: f32, width: f32) -> f32 {
        ((alpha - cutoff) / width.max(0.0001) + 1.0).clamp(0.0, 1.0)
    }

    #[test]
    fn soft_distance_follows_small_shapes() {
        assert_eq!(soft_distance(0.0), SOFT_DISTANCE);
        assert_eq!(soft_distance(2_000.0), SOFT_DISTANCE);
        assert!((soft_distance(58.5) - 14.625).abs() < 1e-4);
        assert_eq!(soft_distance(1.0), 1.0);
    }

    #[test]
    fn soft_intersections_for_unlit_added_and_blended_shapes_only() {
        // Beams and glows (added), smoke and particles (blended), premultiplied.
        for mode in [AlphaMode::Add, AlphaMode::Blend, AlphaMode::Premultiplied] {
            assert!(soft_intersections(mode, true, false, false));
        }
        // Not lit shapes (glass), multiply cards, solid or cut-out shapes, decals or
        // refracting shells.
        assert!(!soft_intersections(AlphaMode::Blend, false, false, false));
        assert!(!soft_intersections(AlphaMode::Multiply, true, false, false));
        assert!(!soft_intersections(AlphaMode::Opaque, true, false, false));
        assert!(!soft_intersections(AlphaMode::Mask(0.5), true, false, false));
        // Decal-flagged shapes are included (the along-the-surface rule keeps decals).
        assert!(!soft_intersections(AlphaMode::Add, true, true, false));
        // Burning neon keeps its full strength against the wall it is mounted on.
        assert!(!soft_intersections(AlphaMode::Blend, true, false, true));
    }

    #[test]
    fn soft_edge_covers_what_the_alpha_test_keeps() {
        // `width` is how much the alpha changes across one pixel.
        for (cutoff, width) in [(0.5, 0.25), (0.36, 0.05), (128.5 / 255.0, 1.0)] {
            // At and above the cutoff (where the depth prepass writes depth): all samples.
            assert_eq!(edge(cutoff, cutoff, width), 1.0);
            assert_eq!(edge(1.0, cutoff, width), 1.0);
            // One pixel's change below the cutoff: nothing; half way: half the samples.
            assert_eq!(edge(cutoff - width, cutoff, width), 0.0);
            assert!((edge(cutoff - width / 2.0, cutoff, width) - 0.5).abs() < 1e-5);
        }
        // Constant alpha (no change across the pixel): the plain alpha test.
        assert_eq!(edge(0.49, 0.5, 0.0), 0.0);
        assert_eq!(edge(0.5, 0.5, 0.0), 1.0);
    }

    #[test]
    fn multiply_blend_factor() {
        let display = |v: Vec3| v.powf(1.0 / MULTIPLY_GAMMA);
        let close = |a: Vec3, b: [f32; 3]| (a - Vec3::from_array(b)).abs().max_element() < 0.005;
        // Rex's skull cap (rex_skullcap.nif `##SkullCap:0`): a flat texture of sRGB
        // (49, 87, 117), emissive colour 0.19 × multiplier 12. In the dark it leaves what
        // lies behind at texture × 2.28 in display values, blue held at 1: a light blue
        // glass, not black.
        let texture = Vec3::new(49.0, 87.0, 117.0) / 255.0;
        let emissive = texture.powf(MULTIPLY_GAMMA) * display_emissive(0.190_283_18 * 12.0);
        assert!(close(display(multiply_factor(Vec3::ZERO, emissive)), [0.439, 0.779, 1.0]));
        // Light on the cap adds in display values and cannot pass 1.
        let lit = Vec3::splat(0.25f32.powf(MULTIPLY_GAMMA));
        assert!(close(display(multiply_factor(lit, emissive)), [0.689, 1.0, 1.0]));
        // Without emissive light the factor is the shape's colour: an unlit shadow overlay
        // darkens by its vertex colour, white changes nothing and black hides.
        assert!(close(multiply_factor(Vec3::splat(0.5), Vec3::ZERO), [0.5; 3]));
        assert!(close(multiply_factor(Vec3::ONE, Vec3::ZERO), [1.0; 3]));
        assert!(close(multiply_factor(Vec3::splat(4.0), Vec3::ZERO), [1.0; 3]));
        assert!(close(multiply_factor(Vec3::ZERO, Vec3::ZERO), [0.0; 3]));
        // An emissive value of 1 is 1 in both spaces; larger ones grow by the gamma.
        assert_eq!(display_emissive(1.0), 1.0);
        assert!((display_emissive(2.0) - 2f32.powf(2.2)).abs() < 1e-6);
        assert_eq!(display_emissive(-1.0), 0.0);
    }

    #[test]
    fn additive_blend_in_display_values() {
        let display = |v: Vec3| v.powf(1.0 / MULTIPLY_GAMMA);
        let linear = |v: Vec3| v.powf(MULTIPLY_GAMMA);
        // The display value of a frame `behind` (display) once a shape of display colour
        // `colour` and alpha `a` is added.
        let shown = |colour: Vec3, a: f32, behind: Vec3| display(linear(behind) + add_display(linear(colour), a, linear(behind)));
        let close = |a: Vec3, b: Vec3| (a - b).abs().max_element() < 0.002;
        let grey = Vec3::splat;
        // The Saloon's window rays (indfxlightrays01.nif): white, vertex alpha 0.1 at
        // most. The game adds 0.1 to the display value whatever lies behind; alpha^1.5
        // (the rule before) added 0.21 on black and 0.05 on a wall of 0.3.
        assert!(close(shown(Vec3::ONE, 0.1, Vec3::ZERO), grey(0.1)));
        assert!(close(shown(Vec3::ONE, 0.1, grey(0.3)), grey(0.4)));
        // The Strip's lamp beams (fxlightbeamnvstrip.nif): white, vertex alpha 0.25 at
        // most, on the night sky: 0.25, not the 0.39 of alpha^1.5.
        assert!(close(shown(Vec3::ONE, 0.25, Vec3::ZERO), grey(0.25)));
        // The Big MT's sky beams (nvdlc03imposterbeamblue.nif): texture (0, 207, 255),
        // emissive white × 10, vertex alpha 0.15 at the core, over a night sky of
        // (17, 17, 33): 1.5 × the texture is added, past white in green and blue.
        let texture = Vec3::new(0.0, 207.0, 255.0) / 255.0;
        let sky = Vec3::new(17.0, 17.0, 33.0) / 255.0;
        let beam = linear(texture) * display_emissive(10.0);
        assert!(close(shown(display(beam), 0.15, sky), sky + texture * 1.5));
        // A ONE / ONE shape (alpha 1) on black adds its colour as it is.
        let c = Vec3::new(0.2, 0.5, 0.9);
        assert!(close(add_display(c, 1.0, Vec3::ZERO), c));
        // The sum is held to ADD_DISPLAY_MAX; a frame brighter than that gets nothing and
        // loses nothing.
        let flash = Vec3::splat(display_emissive(0.99 * 30.0));
        assert!(close(shown(display(flash), 1.0, Vec3::ZERO), grey(ADD_DISPLAY_MAX)));
        assert_eq!(add_display(flash, 1.0, grey(9.0)), Vec3::ZERO);
        // No alpha adds nothing.
        assert!(close(add_display(Vec3::ONE, 0.0, grey(0.2)), Vec3::ZERO));
    }

    #[test]
    fn emissive_in_display_values_for_multiply_and_additive_shapes() {
        assert!(emissive_in_display(AlphaMode::Multiply));
        assert!(emissive_in_display(AlphaMode::Add));
        assert!(!emissive_in_display(AlphaMode::Blend));
        assert!(!emissive_in_display(AlphaMode::Opaque));
    }

    #[test]
    fn unlit_blend_colour() {
        let display = |v: f32| v.powf(1.0 / MULTIPLY_GAMMA);
        let linear = |v: f32| v.powf(MULTIPLY_GAMMA);
        // Bevy's blend of a colour (display) with the factor on it, over `behind`.
        let blend = |colour: f32, a: f32, behind: f32| display(linear(colour) * blend_colour_scale(a) * a + linear(behind) * (1.0 - a));
        // Grey 0.5 smoke at alpha 0.3 over black: 0.15 in display values, 0.29 as
        // blended before; now 0.22.
        assert!((blend(0.5, 0.3, 0.0) - 0.220).abs() < 0.002, "{}", blend(0.5, 0.3, 0.0));
        assert!((display(linear(0.5) * 0.3) - 0.290).abs() < 0.002);
        // The colour term is colour × alpha^BLEND_ALPHA_POWER.
        assert!((blend_colour_scale(0.3) * 0.3 - 0.3f32.powf(BLEND_ALPHA_POWER)).abs() < 1e-6);
        // Over a backdrop of its own colour it stays near it (display 0.5).
        assert!((blend(0.5, 0.3, 0.5) - 0.5).abs() < 0.04);
        // Full and no alpha are unchanged.
        assert_eq!(blend_colour_scale(1.0), 1.0);
        assert_eq!(blend_colour_scale(0.0), 0.0);
        assert_eq!(blend_colour_scale(2.0), 1.0);
    }
}
