//! What a loading screen needs from the render world: how many pipelines the destination
//! is still waiting for (#1520).
//!
//! Bevy compiles pipelines in the background and skips a draw whose pipeline is not ready,
//! pass by pass [documented: bevy_render 0.20.1 `render_phase/mod.rs`, `SetItemPipeline`].
//! A screen that came down with them still compiling left the world's first seconds to the
//! clear colour: a flat view, then everything at once [observed: Goodsprings start on main
//! e2517687, about 120 pipelines waiting when the screen came down, the view flat for about
//! 2 s after it].
//!
//! [`count`] runs at the end of each render frame and reads each pipeline's state rather
//! than `PipelineCache::waiting_pipelines`, which cannot tell why one is waiting [design:
//! bevy_render 0.20.1 `pipeline_cache.rs`, `process_pipeline`]. A pipeline counts as busy
//! while it is queued or compiling, and after the two errors Bevy retries (a shader or an
//! import not loaded yet). An error Bevy does not retry is logged once with the pipeline's
//! label and is not waited for; a draw that needs it stays skipped, as before. So is one
//! still being retried [`PIPELINE_CAP_SECS`] after it was asked for, a shader that never
//! loads for instance, so that it does not hold every screen after it to the screen's own
//! cap. One merely slow to compile is still waited for, up to that cap.
//!
//! The render world answers through [`PipelineGate`], shared with the main world, so what
//! the main world reads may be a frame or two old; a loading screen's settle time
//! (`nvf_play::loading::SETTLE_SECS`) covers that, and pipelines asked for late.
//!
//! Test aid `NVF_PLAY_TEST_PIPELINE_LOG=1`: one log line for each pipeline when it is asked
//! for and when it is ready or has failed for good, with its label and the time between.
//! The sun's shadow pipelines ([`sun_shadow`]) say so in their `asked` line.
//!
//! **A travel's screen and the sun's shadows** (#1732). Twelve trips through new places took
//! 15.4 s of screens with this wait against about 10 s without [observed: one launch], and
//! part of that may be shadow pipelines, whose lack shows less than a flat view: objects
//! stand unshaded, then the shadows pop in. Test aid
//! `NVF_PLAY_TEST_TRAVEL_SKIP_SUN_SHADOWS=1` has a travel's screen not wait for them, to
//! compare both ways by measuring; without it every screen waits for them, as before.
//! Whether it becomes the default is decided by that measurement. The start's screen always
//! waits ([`PipelineGate::busy_for_travel`] is the travel's alone), and nothing but these is
//! left out: the interface, the blit and the pipelines the scene is drawn with are waited
//! for all the same, so the view is not flat as a screen comes down (#1520). They may still
//! queue behind the shadows' compile tasks, so the gain can be less than their share of the
//! wait [hypothesis: tasks start in the order asked]. With the aid, the log says when only
//! they are left (`pipeline gate: only <n> sun shadow pipelines ...`) and when they are
//! ready (`pipeline gate: the sun shadow pipelines are ready`): how long the pop-in lasts.

use bevy::prelude::*;
use bevy::render::render_resource::{CachedPipelineState, PipelineCache, PipelineDescriptor};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::shader::{ShaderCacheError, ShaderDefVal};
use nvf_play::loading::PIPELINE_CAP_SECS;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Instant;

#[derive(Default)]
struct Shared {
    busy: AtomicU32,
    screen_busy: AtomicU32,
    frames: AtomicU32,
    asked: AtomicU32,
    list_next: AtomicBool,
    /// Of `busy`, those a travel's screen does not wait for (test aid, module notes): counted
    /// only under the aid.
    skipped: AtomicU32,
}

/// The render world's answer. A clone lives in each world.
#[derive(Resource, Clone, Default)]
pub struct PipelineGate(Arc<Shared>);

impl PipelineGate {
    /// Total pipelines seen by `count`, including those already ready [design: this file].
    pub fn asked(&self) -> u32 {
        self.0.asked.load(Relaxed)
    }

    /// Pipelines still to be ready, by the last render frame's count.
    pub fn busy(&self) -> u32 {
        self.0.busy.load(Relaxed)
    }

    /// Pipelines still to be ready that a travel's screen waits for (module notes).
    pub fn busy_for_travel(&self) -> u32 {
        self.0.busy.load(Relaxed).saturating_sub(self.0.skipped.load(Relaxed))
    }

    /// Of those, the ones a loading screen itself is drawn with ([`draws_the_screen`]).
    pub fn screen_busy(&self) -> u32 {
        self.0.screen_busy.load(Relaxed)
    }

    /// Render frames finished so far. Before the first, nothing has asked for a pipeline.
    pub fn frames(&self) -> u32 {
        self.0.frames.load(Relaxed)
    }

    /// Have the next render frame log the labels of those still busy.
    pub fn list_busy(&self) {
        self.0.list_next.store(true, Relaxed);
    }
}

pub struct PipelineGatePlugin;

impl Plugin for PipelineGatePlugin {
    fn build(&self, app: &mut App) {
        let gate = PipelineGate::default();
        app.insert_resource(gate.clone());
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.insert_resource(gate).add_systems(Render, count.in_set(RenderSystems::Cleanup));
            if std::env::var("NVF_PLAY_TEST_TRAVEL_SKIP_SUN_SHADOWS").is_ok_and(|v| v == "1") {
                info!("test aid NVF_PLAY_TEST_TRAVEL_SKIP_SUN_SHADOWS: a travel's screen does not wait for the sun's shadow pipelines");
                render.insert_resource(SkipSunShadows::default()).add_systems(Render, count_skipped.in_set(RenderSystems::Cleanup).after(count));
            }
            if std::env::var_os("NVF_PLAY_TEST_PIPELINE_LOG").is_some() {
                info!("test aid NVF_PLAY_TEST_PIPELINE_LOG: each pipeline logged");
                render.add_systems(Render, log_each.in_set(RenderSystems::Cleanup));
            }
        }
    }
}

/// A pipeline the screen waits for: compiling, or to be tried again.
fn waited_for(state: &CachedPipelineState) -> bool {
    match state {
        CachedPipelineState::Queued | CachedPipelineState::Creating(_) => true,
        CachedPipelineState::Ok(_) => false,
        CachedPipelineState::Err(e) => retried(e),
    }
}

/// The errors Bevy tries again [design: bevy_render 0.20.1 `process_pipeline`].
fn retried(e: &ShaderCacheError) -> bool {
    matches!(e, ShaderCacheError::ShaderNotLoaded(_) | ShaderCacheError::ShaderImportNotYetAvailable)
}

/// Pipelines a loading screen needs to be seen: the interface's, and the blit that puts the
/// view on the window, which the first frame blocks on [design: bevy_ui_render and
/// bevy_core_pipeline 0.20.1 labels].
fn draws_the_screen(label: &str) -> bool {
    matches!(label, "ui_pipeline" | "blit pipeline")
}

/// A pipeline that draws only into the sun's shadow maps. The label alone cannot tell: a
/// shadow pass specializes the same pipeline as a camera's depth prepass, `prepass_pipeline`
/// [documented: bevy_pbr 0.20.1 `prepass/mod.rs` `PrepassPipeline::specialize`, reached from
/// `render/light.rs` `specialize_shadows`], `pbr_prepass_pipeline` for a `StandardMaterial`
/// [documented: `pbr_material.rs`, its `specialize` prefixes the label]. What tells them
/// apart is the view's projection: a directional light's shadow view is orthographic, which
/// its key says and the pipeline's `VIEW_PROJECTION_ORTHOGRAPHIC` shader definition shows
/// [documented: `render/light.rs`, the `light_key`; `prepass/mod.rs`]. The only cameras with
/// a prepass here are perspective [design: main.rs `setup`, scope_view.rs's lens camera; the
/// scope's orthographic cameras have none]. A point or spot light's shadow pipeline is left
/// out of this: perspective like the camera's, the two can be one pipeline; the flashlight
/// and the poker table's lamp are the only such casters.
fn sun_shadow(d: &PipelineDescriptor) -> bool {
    let PipelineDescriptor::RenderPipelineDescriptor(d) = d else { return false };
    d.label.as_deref().is_some_and(|l| l == "prepass_pipeline" || l == "pbr_prepass_pipeline")
        && d.vertex.shader_defs.iter().any(|v| matches!(v, ShaderDefVal::Bool(name, true) if name == "VIEW_PROJECTION_ORTHOGRAPHIC"))
}

fn label(d: &PipelineDescriptor) -> &str {
    match d {
        PipelineDescriptor::RenderPipelineDescriptor(d) => d.label.as_deref(),
        PipelineDescriptor::ComputePipelineDescriptor(d) => d.label.as_deref(),
        PipelineDescriptor::MeshPipelineDescriptor(d) => d.label.as_deref(),
    }
    .unwrap_or("(no label)")
}

fn count(gate: Res<PipelineGate>, cache: Res<PipelineCache>, mut failed: Local<HashSet<usize>>, mut asked: Local<Vec<Instant>>) {
    let now = Instant::now();
    let (mut busy, mut screen_busy) = (0, 0);
    for (i, p) in cache.pipelines().enumerate() {
        if i >= asked.len() {
            asked.push(now);
        }
        if waited_for(&p.state) {
            if failed.contains(&i) {
                continue;
            }
            if matches!(p.state, CachedPipelineState::Err(_)) && now.duration_since(asked[i]).as_secs_f64() >= PIPELINE_CAP_SECS {
                failed.insert(i);
                warn!("pipeline {i} still being retried after {PIPELINE_CAP_SECS:.0} s and no longer waited for: {}", label(&p.descriptor));
                continue;
            }
            busy += 1;
            if draws_the_screen(label(&p.descriptor)) {
                screen_busy += 1;
            }
        } else if let CachedPipelineState::Err(e) = &p.state {
            if failed.insert(i) {
                warn!("pipeline {i} failed and will not be tried again: {}: {e}", label(&p.descriptor));
            }
        }
    }
    gate.0.busy.store(busy, Relaxed);
    gate.0.asked.store(asked.len() as u32, Relaxed);
    gate.0.screen_busy.store(screen_busy, Relaxed);
    gate.0.frames.fetch_add(1, Relaxed);
    if gate.0.list_next.swap(false, Relaxed) {
        let waiting = cache.pipelines().enumerate().filter(|(i, p)| waited_for(&p.state) && !failed.contains(i));
        let mut labels: Vec<&str> = waiting.map(|(_, p)| label(&p.descriptor)).collect();
        labels.sort_unstable();
        labels.dedup();
        warn!("pipelines still waited for ({busy}): {}", labels.join(", "));
    }
}

/// The aid's state (module notes): whether the log last said only sun shadows are left.
#[derive(Resource, Default)]
struct SkipSunShadows {
    only_they_left: bool,
}

/// Count the waited-for sun shadow pipelines, for [`PipelineGate::busy_for_travel`]. The
/// pipelines [`count`] stopped waiting for are not among them unless they are still being
/// retried, which only the cap ends: at worst this leaves out one it did not count, and
/// `busy_for_travel` saturates.
fn count_skipped(gate: Res<PipelineGate>, cache: Res<PipelineCache>, mut said: ResMut<SkipSunShadows>) {
    let skipped = cache.pipelines().filter(|p| waited_for(&p.state) && sun_shadow(&p.descriptor)).count() as u32;
    gate.0.skipped.store(skipped, Relaxed);
    let only_they_left = skipped > 0 && gate.busy_for_travel() == 0;
    if only_they_left && !said.only_they_left {
        info!("pipeline gate: only {skipped} sun shadow pipelines are still compiling");
        said.only_they_left = true;
    } else if skipped == 0 && said.only_they_left {
        info!("pipeline gate: the sun shadow pipelines are ready");
        said.only_they_left = false;
    }
}

/// `NVF_PLAY_TEST_PIPELINE_LOG` (module notes).
fn log_each(cache: Res<PipelineCache>, mut seen: Local<Vec<(Instant, bool)>>, mut defs: Local<Option<bool>>) {
    let defs = *defs.get_or_insert_with(|| std::env::var_os("NVF_PLAY_TEST_PIPELINE_DEFS").is_some());
    let now = Instant::now();
    for (i, p) in cache.pipelines().enumerate() {
        if i >= seen.len() {
            seen.push((now, false));
            info!("pipeline {i} asked: {}{}", label(&p.descriptor), if sun_shadow(&p.descriptor) { " (sun shadow)" } else { "" });
            if defs {
                if let PipelineDescriptor::RenderPipelineDescriptor(d) = &p.descriptor {
                    let interesting = |s: &&bevy::shader::ShaderDefVal| {
                        let name: &str = match s {
                            bevy::shader::ShaderDefVal::Bool(s, _) | bevy::shader::ShaderDefVal::Int(s, _) | bevy::shader::ShaderDefVal::UInt(s, _) => s,
                        };
                        matches!(name, "PIPBOY_SCREEN" | "ADD_LINEAR" | "FIRST_PERSON_DEPTH" | "HAS_PREVIOUS_SKIN" | "PREPASS_PIPELINE") || name.starts_with("BLEND_")
                    };
                    let fragment: Vec<_> = d.fragment.iter().flat_map(|f| f.shader_defs.iter()).filter(interesting).collect();
                    info!("pipeline {i} shader-defs: {fragment:?}: {}", label(&p.descriptor));
                }
            }
        }
        if !seen[i].1 && !waited_for(&p.state) {
            seen[i].1 = true;
            let how = if matches!(p.state, CachedPipelineState::Ok(_)) { "ready" } else { "failed" };
            info!("pipeline {i} {how} after {:.0} ms: {}", now.duration_since(seen[i].0).as_secs_f64() * 1000.0, label(&p.descriptor));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asked_is_shared_with_the_main_world() {
        let gate = PipelineGate::default();
        assert_eq!(gate.asked(), 0);
        let render = gate.clone();
        render.0.asked.store(17, Relaxed);
        assert_eq!(gate.asked(), 17);
    }

    #[test]
    fn waits_for_what_bevy_will_still_finish_and_not_for_a_final_failure() {
        assert!(waited_for(&CachedPipelineState::Queued));
        assert!(waited_for(&CachedPipelineState::Err(ShaderCacheError::ShaderImportNotYetAvailable)));
        assert!(waited_for(&CachedPipelineState::Err(ShaderCacheError::ShaderNotLoaded(AssetId::default()))));
        assert!(!waited_for(&CachedPipelineState::Err(ShaderCacheError::ProcessShaderError("bad".into()))));
        assert!(!waited_for(&CachedPipelineState::Err(ShaderCacheError::CreateShaderModule("bad".into()))));
    }

    fn prepass(label: &'static str, defs: &[&'static str]) -> PipelineDescriptor {
        PipelineDescriptor::RenderPipelineDescriptor(Box::new(bevy::render::render_resource::RenderPipelineDescriptor {
            label: Some(label.into()),
            vertex: bevy::render::render_resource::VertexState { shader_defs: defs.iter().map(|d| ShaderDefVal::from(*d)).collect(), ..default() },
            ..default()
        }))
    }

    #[test]
    fn only_the_suns_shadow_pipelines_are_left_out() {
        let ortho = ["PREPASS_PIPELINE", "VIEW_PROJECTION_ORTHOGRAPHIC", "DEPTH_PREPASS"];
        let perspective = ["PREPASS_PIPELINE", "VIEW_PROJECTION_PERSPECTIVE", "DEPTH_PREPASS"];
        assert!(sun_shadow(&prepass("prepass_pipeline", &ortho)));
        assert!(sun_shadow(&prepass("pbr_prepass_pipeline", &ortho)));
        // The camera's depth prepass, a point or spot light's shadows: the same label.
        assert!(!sun_shadow(&prepass("prepass_pipeline", &perspective)));
        assert!(!sun_shadow(&prepass("pbr_prepass_pipeline", &perspective)));
        // What draws the scene or the screen, whatever its definitions.
        assert!(!sun_shadow(&prepass("pbr_opaque_mesh_pipeline", &ortho)));
        assert!(!sun_shadow(&prepass("ui_pipeline", &ortho)));
    }

    #[test]
    fn a_travel_waits_for_all_but_the_ones_left_out() {
        let gate = PipelineGate::default();
        gate.0.busy.store(5, Relaxed);
        assert_eq!(gate.busy_for_travel(), 5, "without the aid nothing is left out");
        gate.0.skipped.store(3, Relaxed);
        assert_eq!(gate.busy_for_travel(), 2);
        assert_eq!(gate.busy(), 5, "the start's screen still counts them");
        gate.0.skipped.store(9, Relaxed);
        assert_eq!(gate.busy_for_travel(), 0, "never below nothing");
    }

    #[test]
    fn the_screen_needs_only_the_interface_and_the_blit() {
        assert!(draws_the_screen("ui_pipeline"));
        assert!(draws_the_screen("blit pipeline"));
        assert!(!draws_the_screen("pbr_opaque_mesh_pipeline"));
        assert!(!draws_the_screen("post_pipeline"));
    }
}
