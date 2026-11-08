// Water surface (WATR), see water.rs for the model and docs/spec-notes/world.md §6 for
// which parts are documented and which are hypotheses.
//
// Bevy world space: y up. Game space (x, y, z) = Bevy (x, -z, y).

import bevy_pbr::{
    render::forward_io::VertexOutput,
    render::mesh_view_bindings::{view, globals, lights, view_transmission_texture, view_transmission_sampler},
    render::pbr_types,
    render::pbr_functions,
    render::view_transformations::{ndc_to_uv, position_ndc_to_world},
};
@if(DEPTH_PREPASS) import bevy_pbr::prepass::utils as prepass_utils;
@if(DEPTH_PREPASS) import bevy_pbr::render::mesh_view_bindings::depth_prepass_texture;

// During temporal upscaling, the main pass occupies only the render-sized region
// of full-sized attachments. Geometry UVs and texture UVs are distinct.
fn frag_coord_to_uv(fc: vec2<f32>) -> vec2<f32> {
    return (fc - view.main_pass_viewport.xy) / view.main_pass_viewport.zw;
}
fn frag_coord_to_ndc(fc: vec4<f32>) -> vec3<f32> {
    let uv = frag_coord_to_uv(fc.xy);
    return vec3(uv * vec2(2.0, -2.0) + vec2(-1.0, 1.0), fc.z);
}
fn transmission_uv(uv: vec2<f32>) -> vec2<f32> {
    let pixel = view.main_pass_viewport.xy + uv * view.main_pass_viewport.zw;
    let bounded = clamp(pixel, view.main_pass_viewport.xy + 0.5,
        view.main_pass_viewport.xy + view.main_pass_viewport.zw - 0.5);
    return bounded / vec2<f32>(textureDimensions(view_transmission_texture));
}

struct WaterParams {
    shallow: vec4<f32>,
    deep: vec4<f32>,
    // rgb = reflection colour, a = 1 when the type is reflective (reflects the sky).
    reflection: vec4<f32>,
    sky_upper: vec4<f32>,
    sky_horizon: vec4<f32>,
    // near, far, amount, 1 when the amount caps the opacity (placed water)
    above_fog: vec4<f32>,
    under_fog: vec4<f32>,
    // reflectivity, fresnel amount, shininess, wave height (world units)
    surface: vec4<f32>,
    // per layer: uv velocity (per second), 1 / uv scale (per world unit), amplitude
    layers: array<vec4<f32>, 3>,
    // x = Bevy y of the plane the reflection texture shows, y = 1 when it is in use,
    // z = 1 when screen-space reflections stand in where it is not (enhanced graphics)
    reflection_plane: vec4<f32>,
    // ripple grid: game x, y of its corner, units per cell, height scale (0: none)
    ripple: vec4<f32>,
    // shore fade: depth falloff start, end (units of water crossed), 1 when in use
    edge: vec4<f32>,
    // x = Bevy y of the water whose drawn pixels are counted, y = 1 while counting
    seen_plane: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> water: WaterParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var noise_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var noise_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var reflection_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var reflection_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(5) var ripple_texture: texture_2d<f32>;
// Drawn water pixels at seen_plane's height (water.rs WaterMaterial::seen, #1060).
@group(constants::MATERIAL_BIND_GROUP) @binding(6) var<storage, read_write> water_seen: atomic<u32>;

// Whether opaque surfaces (depth prepass) lie in front of this fragment in every sample
// of its pixel: the depth test would throw the whole pixel away. Reverse z: nearer is
// larger. False without a prepass.
fn hidden_by_scene(fc: vec4<f32>) -> bool {
    @if(DEPTH_PREPASS && MULTISAMPLED) if true {
        let n = textureNumSamples(depth_prepass_texture);
        for (var i = 0u; i < n; i += 1u) {
            if textureLoad(depth_prepass_texture, vec2<i32>(fc.xy), i32(i)) <= fc.z {
                return false;
            }
        }
        return true;
    }
    @if(DEPTH_PREPASS && !MULTISAMPLED) if true {
        return textureLoad(depth_prepass_texture, vec2<i32>(fc.xy), 0) > fc.z;
    }
    return false;
}

// Ripple height at grid position `t` (cells), bilinear (the texture is not filterable).
fn ripple_height(t: vec2<f32>) -> f32 {
    let n = vec2<i32>(textureDimensions(ripple_texture)) - 1;
    let i = vec2<i32>(floor(t));
    let f = t - floor(t);
    let a = textureLoad(ripple_texture, clamp(i, vec2(0), n), 0).r;
    let b = textureLoad(ripple_texture, clamp(i + vec2(1, 0), vec2(0), n), 0).r;
    let c = textureLoad(ripple_texture, clamp(i + vec2(0, 1), vec2(0), n), 0).r;
    let d = textureLoad(ripple_texture, clamp(i + vec2(1, 1), vec2(0), n), 0).r;
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// Slope (game space) of the ripples at game position `g`, faded out towards the grid edge.
fn ripple_slope(g: vec2<f32>) -> vec2<f32> {
    if water.ripple.w <= 0.0 {
        return vec2(0.0);
    }
    let size = f32(textureDimensions(ripple_texture).x);
    let t = (g - water.ripple.xy) / water.ripple.z;
    let edge = min(min(t.x, t.y), min(size - t.x, size - t.y));
    let fade = clamp((edge - 2.0) / 24.0, 0.0, 1.0);
    if fade <= 0.0 {
        return vec2(0.0);
    }
    let dx = ripple_height(t + vec2(1.0, 0.0)) - ripple_height(t - vec2(1.0, 0.0));
    let dy = ripple_height(t + vec2(0.0, 1.0)) - ripple_height(t - vec2(0.0, 1.0));
    return vec2(dx, dy) / (2.0 * water.ripple.z) * water.ripple.w * fade;
}

// Distance along the wavy reflected ray at which the reflection texture is sampled: the
// texture has no depth, so the wave offset is that of a point this far away [tuned by eye].
const REFLECTION_SAMPLE_DISTANCE: f32 = 500.0;

// Refraction: the scene behind the surface is sampled this far (in screen UV per unit of
// view-space normal tilt) from the pixel, growing to that over this much water
// [tuned by eye; the engine's refraction rule is unknown].
const REFRACTION_STRENGTH: f32 = 0.08;
const REFRACTION_DEPTH: f32 = 200.0;

// The opaque surface behind screen position `fc` (depth prepass): world position, and
// w = 1 when there is one (0 for the sky or without a prepass).
fn scene_point(fc: vec2<f32>) -> vec4<f32> {
    @if(DEPTH_PREPASS) let d = prepass_utils::prepass_depth(vec4(fc, 0.0, 1.0), 0u);
    @if(DEPTH_PREPASS) if d > 0.0 {
        let ndc = frag_coord_to_ndc(vec4(fc, d, 1.0));
        return vec4(position_ndc_to_world(ndc), 1.0);
    }
    return vec4(0.0);
}

// Screen-space reflections (enhanced graphics, graphics-enhanced.md §3.8): where the
// mirrored camera does not show this plane (water at another height, interiors), the
// reflected ray is marched through the depth prepass and the opaque picture (the
// transmission copy) is taken where it passes behind something. Steps grow by SSR_GROWTH
// from SSR_FIRST units, SSR_STEPS of them (about 9,000 units in all), then four halvings
// [engineering choices, by eye]. Returns the colour (display values: fogged and
// tonemapped, as the copy holds them) and in w how much to trust it (0: no hit).
const SSR_STEPS: i32 = 28;
const SSR_FIRST: f32 = 24.0;
const SSR_GROWTH: f32 = 1.16;
fn ssr(pos: vec3<f32>, R: vec3<f32>) -> vec4<f32> {
    @if(DEPTH_PREPASS) let o = (view.view_from_world * vec4(pos, 1.0)).xyz;
    @if(DEPTH_PREPASS) let d = normalize((view.view_from_world * vec4(R, 0.0)).xyz);
    @if(DEPTH_PREPASS) let size = view.main_pass_viewport.zw;
    @if(DEPTH_PREPASS) var t0 = 0.0;
    @if(DEPTH_PREPASS) var t = SSR_FIRST;
    @if(DEPTH_PREPASS) var stride = SSR_FIRST;
    @if(DEPTH_PREPASS) for (var i = 0; i < SSR_STEPS; i++) {
        let p = o + d * t;
        if p.z >= -1.0 {
            return vec4(0.0);
        }
        let clip = view.clip_from_view * vec4(p, 1.0);
        let uv = ndc_to_uv(clip.xy / clip.w);
        if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) {
            return vec4(0.0);
        }
        let scene = prepass_utils::prepass_depth(vec4(view.main_pass_viewport.xy + uv * size, 0.0, 1.0), 0u);
        // Distance along the view axis of the ray's point and of the scene there
        // (reverse infinite z: distance = near / depth; the sky, depth 0, is never hit).
        let ray_z = -p.z;
        let near = view.clip_from_view[3][2];
        let scene_z = select(1.0e9, near / scene, scene > 0.0);
        // Behind the scene, by less than a step's length (else it passed behind an object
        // it does not touch).
        if ray_z > scene_z && ray_z - scene_z < stride * 2.0 + 20.0 {
            // Halve the last step four times.
            var lo = t0;
            var hi = t;
            for (var k = 0; k < 4; k++) {
                let m = 0.5 * (lo + hi);
                let pm = o + d * m;
                let cm = view.clip_from_view * vec4(pm, 1.0);
                let um = ndc_to_uv(cm.xy / cm.w);
                let sm = prepass_utils::prepass_depth(vec4(view.main_pass_viewport.xy + um * size, 0.0, 1.0), 0u);
                if sm > 0.0 && -pm.z > near / sm {
                    hi = m;
                } else {
                    lo = m;
                }
            }
            let ph = o + d * hi;
            let ch = view.clip_from_view * vec4(ph, 1.0);
            let uh = ndc_to_uv(ch.xy / ch.w);
            let c = textureSampleLevel(view_transmission_texture, view_transmission_sampler, transmission_uv(uh), 0.0).rgb;
            // Less trust near the picture's edges and towards the end of the march.
            let edge = min(min(uh.x, 1.0 - uh.x), min(uh.y, 1.0 - uh.y));
            let trust = smoothstep(0.0, 0.08, edge) * (1.0 - smoothstep(0.6, 1.0, f32(i) / f32(SSR_STEPS)));
            return vec4(c, trust);
        }
        t0 = t;
        stride *= SSR_GROWTH;
        t += stride;
    }
    return vec4(0.0);
}

// Slope (dh/dx, dh/dy in game space) of one noise layer: height = texture channel `mask`
// (0..1) times the layer amplitude times the wave height.
fn layer_slope(g: vec2<f32>, layer: vec4<f32>, mask: vec4<f32>, t: f32) -> vec2<f32> {
    let uv = g * layer.z + layer.xy * t;
    let e = 1.0 / vec2<f32>(textureDimensions(noise_texture));
    let h0 = dot(textureSample(noise_texture, noise_sampler, uv), mask);
    let hu = dot(textureSample(noise_texture, noise_sampler, uv + vec2(e.x, 0.0)), mask);
    let hv = dot(textureSample(noise_texture, noise_sampler, uv + vec2(0.0, e.y)), mask);
    // d(uv)/d(world) = layer.z
    return vec2(hu - h0, hv - h0) / e * layer.z * layer.w * water.surface.w;
}

fn sky_colour(y: f32) -> vec3<f32> {
    // The same gradient as the sky dome above the horizon (sky.rs).
    return mix(water.sky_horizon.rgb, water.sky_upper.rgb, sqrt(clamp(y / 0.5, 0.0, 1.0)));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let pos = in.world_position.xyz;
    // The counter's writes turn off the GPU's early depth test, so water behind the
    // scene is thrown away here, before the costly part, as the depth test would. What
    // is left counts, at the reflected height, one pixel in 16 (#1060).
    //
    // Keep the texture sampling below in helper functions (layer_slope and the rest):
    // naga checks uniform control flow per function, and a textureSample written
    // directly in fragment() after this discard would fail validation, so water would
    // stop drawing.
    if hidden_by_scene(in.position) {
        discard;
    }
    // One pixel in 16 (a 4 x 4 grid): water seen only through a gap narrower than that
    // may count nothing and show the sky alone in it, an accepted cost for 16 times
    // fewer atomic adds on one address.
    let grid = vec2<u32>(in.position.xy) & vec2(3u);
    if water.seen_plane.y > 0.5 && abs(pos.y - water.seen_plane.x) < 2.0 && grid.x == 0u && grid.y == 0u {
        atomicAdd(&water_seen, 1u);
    }
    let to_cam = view.world_position.xyz - pos;
    let dist = max(length(to_cam), 1e-3);
    let V = to_cam / dist;
    let above = V.y >= 0.0;

    let g = vec2(pos.x, -pos.z);
    let t = globals.time;
    var s = layer_slope(g, water.layers[0], vec4(1.0, 0.0, 0.0, 0.0), t);
    s += layer_slope(g, water.layers[1], vec4(0.0, 1.0, 0.0, 0.0), t);
    s += layer_slope(g, water.layers[2], vec4(0.0, 0.0, 1.0, 0.0), t);
    s += ripple_slope(g);
    // Game-space normal (-sx, -sy, 1) in Bevy space.
    var N = normalize(vec3(-s.x, 1.0, s.y));
    var up = vec3(0.0, 1.0, 0.0);
    if !above {
        N = -N;
        up = -up;
    }

    // The length of the view ray inside the water: from above, from the surface to the
    // opaque surface behind it (depth prepass), so 0 at the waterline; from below, from
    // the camera to the surface. It sets the body's opacity, the refraction and the shore
    // fade.
    //
    // The fog depth: from above, how far below the surface that opaque surface lies
    // (the above fog's near and far planes are "the distance in game units below the
    // surface of the water" of the shallow and the deep colour plane [documented: GECK
    // wiki WaterType]); it sets the body's colour, so a shallow pool keeps its shallow
    // colour at a grazing look;
    // from below, the distance from the camera (the under fog's planes are distances
    // "from the player" [documented: same page]). Nothing behind: the far end of both.
    let size = view.main_pass_viewport.zw;
    let uv0 = frag_coord_to_uv(in.position.xy);
    let behind0 = scene_point(in.position.xy);
    var fog_distance = 1.0e7;
    var fog_depth = 1.0e7;
    if behind0.w > 0.5 {
        fog_distance = distance(behind0.xyz, pos);
        fog_depth = max(pos.y - behind0.y, 0.0);
    }
    // Refraction: shift the sample by the wave normal's tilt in view space, more through
    // deeper water; keep the unshifted sample where the shifted one lands on something in
    // front of the water (it would bleed into the surface).
    let tilt = (view.view_from_world * vec4(N - up, 0.0)).xy;
    let depth_scale = clamp(min(fog_distance, dist) / REFRACTION_DEPTH, 0.0, 1.0);
    var uv = clamp(uv0 + vec2(tilt.x, -tilt.y) * REFRACTION_STRENGTH * depth_scale, vec2(0.0), vec2(1.0));
    if above {
        let behind = scene_point(view.main_pass_viewport.xy + uv * size);
        if behind.w > 0.5 && distance(behind.xyz, view.world_position.xyz) < dist {
            uv = uv0;
        } else if behind.w > 0.5 {
            fog_distance = distance(behind.xyz, pos);
            fog_depth = max(pos.y - behind.y, 0.0);
        }
    }
    if !above {
        fog_distance = dist;
        fog_depth = dist;
    }
    let refracted = textureSampleLevel(view_transmission_texture, view_transmission_sampler, transmission_uv(uv), 0.0).rgb;
    let fog = select(water.under_fog, water.above_fog, above);
    // The colour: shallow to deep by the fog depth (#1787). The opacity: by the length of
    // the view ray in the water over the same near to far ramp, at most the above fog's
    // amount for placed water ("controls the transparency of the Deep Color water plane",
    // 1.0 opaque [documented: GECK wiki WaterType]; above_fog.w = 1). Cell and distant
    // water, and any water seen from below, are opaque beyond the far distance: capped,
    // Lake Mead's bed showed through far out [design, until a far-out Lake Mead
    // comparison is made, world.md §6].
    // [hypothesis, checked against the original at the Ultra-Luxe fountain, the
    // Goodsprings Source trough and Boulder Beach, world.md §6]: by the depth alone a
    // 14-unit pool under a 36-unit fog showed its floor where the original is opaque.
    let depth01 = clamp((fog_depth - fog.x) / max(fog.y - fog.x, 1.0), 0.0, 1.0);
    let ray01 = clamp((fog_distance - fog.x) / max(fog.y - fog.x, 1.0), 0.0, 1.0);
    let body_alpha = select(ray01, ray01 * clamp(water.above_fog.z, 0.0, 1.0), above && water.above_fog.w > 0.5);
    let body_colour = mix(water.shallow.rgb, water.deep.rgb, depth01);

    // The water body is lit like a diffuse surface facing up (from either side).
    var pbr = pbr_types::pbr_input_new();
    pbr.material.base_color = vec4(body_colour, 1.0);
    pbr.material.perceptual_roughness = 1.0;
    pbr.material.metallic = 0.0;
    pbr.material.reflectance = vec3(0.0);
    pbr.frag_coord = in.position;
    pbr.world_position = in.world_position;
    pbr.world_normal = vec3(0.0, 1.0, 0.0);
    pbr.N = vec3(0.0, 1.0, 0.0);
    pbr.V = V;
    pbr.is_orthographic = view.clip_from_view[3].w == 1.0;
    let lit = pbr_functions::apply_pbr_lighting(pbr).rgb;

    // Reflection: the world (reflective types: the mirrored camera's texture when it shows
    // this plane, else the sky gradient) or the reflection colour, weighted by a Schlick
    // fresnel term blended with a constant by the fresnel amount.
    var fr = 0.0;
    var reflected = vec3(0.0);
    // Screen-space reflection where the mirror camera does not cover (display values).
    var traced = vec4(0.0);
    if above {
        let R = reflect(-V, N);
        let schlick = 0.02 + 0.98 * pow(1.0 - clamp(dot(N, V), 0.0, 1.0), 5.0);
        fr = clamp(water.surface.x * mix(1.0, schlick, water.surface.y), 0.0, 1.0);
        // Sampled unconditionally (uniform control flow), used when it shows this plane.
        // The mirror image of the wavy reflected ray's point, seen from the camera; for a
        // flat surface that is this very pixel. Projected without the view's temporal
        // jitter (FSR, TAA): the reflection camera draws without it, so the jittered
        // projection put each lookup up to half a render pixel off, differently in every
        // frame, and the reflections wobbled. Without jitter the two are the same.
        let mirrored = pos + vec3(R.x, -R.y, R.z) * REFLECTION_SAMPLE_DISTANCE;
        let clip = view.unjittered_clip_from_world * vec4(mirrored, 1.0);
        var uv = frag_coord_to_uv(in.position.xy);
        if clip.w > 0.0 {
            uv = ndc_to_uv(clip.xy / clip.w);
        }
        let world_reflection = textureSampleLevel(reflection_texture, reflection_sampler, clamp(uv, vec2(0.0), vec2(1.0)), 0.0).rgb;
        let mirror_here = water.reflection_plane.y > 0.5 && abs(pos.y - water.reflection_plane.x) < 2.0;
        let sky = select(sky_colour(R.y), world_reflection, mirror_here);
        if !mirror_here && water.reflection_plane.z > 0.5 && water.reflection.a > 0.5 {
            traced = ssr(pos, R);
        }
        reflected = select(water.reflection.rgb, sky, water.reflection.a > 0.5);
        if lights.n_directional_lights > 0u {
            let sun = lights.directional_lights[0];
            let spec = pow(max(dot(R, sun.direction_to_light), 0.0), max(water.surface.z, 1.0));
            // In a world view the sun's colour holds display values (nvforever lights.rs
            // DISPLAY_HELPERS): its light is that value raised to the display gamma.
            let sun_light = select(sun.color.rgb * view.exposure, 3.14159265 * pow(max(sun.color.rgb * view.exposure / 3.14159265, vec3(0.0)), vec3(2.2)), lights.ambient_color.a < 0.0);
            reflected += spec * sun_light;
        }
    } else {
        // From below, the sky shows only within Snell's window (refractive index 1.33:
        // cos > 0.66 from the vertical); outside it the surface reflects the water body.
        let c = clamp(dot(N, V), 0.0, 1.0);
        fr = 1.0 - smoothstep(0.62, 0.70, c);
        reflected = lit;
    }

    // Shore fade (water.rs): from above, the surface fades in over the depth falloff.
    var edge = 1.0;
    if above && water.edge.z > 0.5 {
        edge = clamp((fog_distance - water.edge.x) / max(water.edge.y - water.edge.x, 1e-3), 0.0, 1.0);
    }
    let alpha = (fr + (1.0 - fr) * body_alpha) * edge;
    let premultiplied = (fr * reflected + (1.0 - fr) * body_alpha * lit) * edge;
    var colour = vec4(premultiplied / max(alpha, 1e-4), alpha);
    // Distance fog and (non-HDR) tonemapping as for the standard material, then over the
    // refracted scene (already fogged and tonemapped: a copy of the opaque pass).
    pbr.material.flags = pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    colour = pbr_functions::main_pass_post_lighting_processing(pbr, colour);
    var result = colour.rgb * colour.a + refracted * (1.0 - colour.a);
    // The traced reflection takes the reflected share's place [approximation: mixed in
    // after fog and tonemapping, which the copy already has].
    result = mix(result, traced.rgb, fr * traced.w);
    return vec4(result, 1.0);
}
