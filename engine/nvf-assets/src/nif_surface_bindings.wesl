// The NifSurface extension's values and textures for one fragment, bindless or not
// (nif_material.rs "Bindless"; the Rust side: `NifSurface`, `NifSurfaceUniform`). Used by
// nif_material.wesl and nif_prepass.wesl.


import bevy_pbr::render::mesh_bindings::mesh;
@if(BINDLESS) import bevy_render::bindless::{bindless_textures_2d, bindless_samplers_filtering};
@if(BINDLESS) import bevy_pbr::render::pbr_bindings::{material_indices, material_array};
@if(BINDLESS) import bevy_pbr::render::pbr_types::StandardMaterial;
@if(!BINDLESS) import bevy_pbr::render::pbr_bindings;
@if(!BINDLESS) import bevy_pbr::render::pbr_types::StandardMaterial;

struct NifSurface {
    // x start angle (cosine), y stop angle (cosine), z start opacity, w stop opacity;
    // (1, 0, 1, 1) when the shape has no falloff.
    falloff: vec4<f32>,
    // Fragments with a lower alpha are not drawn (the alpha test of a shape that is
    // drawn blended); 0 on every other shape.
    alpha_cut: f32,
    // 1: an additive shape with source factor ONE, which adds its colour whole (only the
    // view-angle falloff fades it); 0 otherwise.
    add_whole: f32,
    // REFRACT: the screen shift (UV) per unit of the view-space normal (the strength ×
    // nif_material.rs REFRACTION_SCREEN); 0 otherwise.
    refraction: f32,
    // REFRACT: seconds for the normal map to scroll once up its texture (fire
    // refraction); 0: it does not move.
    fire_period: f32,
    // x 1 skips the image-space emissive scale; y holds each channel on the soft path
    // (0: no hold); z, w unused [hypothesis, nif.md §6 "Unlit shapes that are pictures,
    // and cut-out signs"]. Same fields in the same order as NifSurfaceUniform.
    emissive_rule: vec4<f32>,
    // Holes: x the value (alpha units, negative: no holes), y the edge band's width.
    holes: vec4<f32>,
    // The edge band's colour (linear).
    holes_edge: vec4<f32>,
    // Soft intersections: x the fade distance (game units), 0 for none.
    soft: vec4<f32>,
}

// The extension's index table (indices 50-53 of NifSurface's bindless attribute).
@if(BINDLESS) struct NifSurfaceIndices {
    surface: u32,          // 50
    holes_texture: u32,    // 51
    holes_sampler: u32,    // 52
    facegen_offset: u32,   // 53
}
@if(BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(100) var<storage> nif_indices: array<NifSurfaceIndices>;
@if(BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(101) var<storage> nif_surfaces: array<NifSurface>;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(50) var<uniform> nif_surface_values: NifSurface;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(51) var holes_texture: texture_2d<f32>;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(52) var holes_sampler: sampler;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(53) var facegen_offset: texture_2d<f32>;

// The material's slot in the bindless tables (unused without bindless).
fn material_slot(instance_index: u32) -> u32 {
    return mesh[instance_index].material_and_lightmap_bind_group_slot & 0xffffu;
}

fn nif_surface(instance_index: u32) -> NifSurface {
    @if(BINDLESS) return nif_surfaces[nif_indices[material_slot(instance_index)].surface];
    @if(!BINDLESS) return nif_surface_values;
}

// The standard part of the material.
fn standard_material(instance_index: u32) -> StandardMaterial {
    @if(BINDLESS) return material_array[material_indices[material_slot(instance_index)].material];
    @if(!BINDLESS) return pbr_bindings::material;
}

// The holes texture's alpha at `uv` (0-1).
fn holes_alpha(instance_index: u32, uv: vec2<f32>) -> f32 {
    @if(BINDLESS) let i = nif_indices[material_slot(instance_index)];
    @if(BINDLESS) return textureSampleLevel(bindless_textures_2d[i.holes_texture], bindless_samplers_filtering[i.holes_sampler], uv, 0.0).a;
    @if(!BINDLESS) return textureSampleLevel(holes_texture, holes_sampler, uv, 0.0).a;
}

// The size of the FaceGen offset image.
fn facegen_size(instance_index: u32) -> vec2<u32> {
    @if(BINDLESS) return textureDimensions(bindless_textures_2d[nif_indices[material_slot(instance_index)].facegen_offset]);
    @if(!BINDLESS) return textureDimensions(facegen_offset);
}

// One texel of the FaceGen offset image.
fn facegen_texel(instance_index: u32, p: vec2<i32>) -> vec3<f32> {
    @if(BINDLESS) return textureLoad(bindless_textures_2d[nif_indices[material_slot(instance_index)].facegen_offset], p, 0).rgb;
    @if(!BINDLESS) return textureLoad(facegen_offset, p, 0).rgb;
}

// The base colour texture sampled at `uv` with the view's mip bias `bias`.
fn base_color_texel(instance_index: u32, uv: vec2<f32>, bias: f32) -> vec4<f32> {
    @if(BINDLESS) let i = material_indices[material_slot(instance_index)];
    @if(BINDLESS) return textureSampleBias(bindless_textures_2d[i.base_color_texture], bindless_samplers_filtering[i.base_color_sampler], uv, bias);
    @if(!BINDLESS) return textureSampleBias(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, uv, bias);
}

// The normal map sampled at `uv`.
fn normal_map_texel(instance_index: u32, uv: vec2<f32>) -> vec4<f32> {
    @if(BINDLESS) let i = material_indices[material_slot(instance_index)];
    @if(BINDLESS) return textureSample(bindless_textures_2d[i.normal_map_texture], bindless_samplers_filtering[i.normal_map_sampler], uv);
    @if(!BINDLESS) return textureSample(pbr_bindings::normal_map_texture, pbr_bindings::normal_map_sampler, uv);
}
