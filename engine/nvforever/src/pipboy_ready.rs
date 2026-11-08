//! [design: #1803] Observe the real rig's main-pass queue and bindings, not the
//! global pipeline count. Queue membership is evidence of submission, not pixels.

use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::pbr::{MaterialQueueProbe, PreparedMaterial, RenderMaterialInstances, RenderMeshInstances, SpecializedMaterialPipelineCache, SpecializedPrepassMaterialPipelineCache};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::camera::visibility::ViewVisibility;
use bevy::render::erased_render_asset::ErasedRenderAssets;
use bevy::render::material_bind_groups::MaterialBindGroupAllocators;
use bevy::render::mesh::{allocator::MeshAllocator, RenderMesh};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{CachedRenderPipelineId, PipelineCache};
use bevy::render::sync_world::MainEntity;
use nvf_play::pipboy::{Phase, PipBoy};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// [design: player_body.rs] Only the first-person rig's shapes.
#[derive(Component)]
pub struct PipBoyShape;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Status {
    pub shapes: usize,
    pub queued: usize,
    pub opaque: usize,
    pub prepared: usize,
    pub bound: usize,
    pub ready: usize,
    pub pipelines: Vec<usize>,
    pub gpu_meshes: usize,
    pub slabs: usize,
    pub specialized: usize,
}

impl Status {
    pub fn submitted(&self) -> bool {
        self.shapes > 0 && self.queued == self.shapes && self.bound == self.shapes
            && self.gpu_meshes == self.shapes && self.slabs == self.shapes
            && self.specialized == self.shapes
    }

    pub fn complete(&self) -> bool {
        self.submitted() && self.ready == self.shapes
    }
}

#[derive(Resource, Clone, Default)]
pub struct PipBoyReady(Arc<Mutex<Status>>);

impl PipBoyReady {
    pub fn status(&self) -> Status {
        self.0.lock().expect("Pip-Boy render status").clone()
    }
}

#[derive(Resource, Default)]
struct Input {
    entities: Vec<MainEntity>,
    camera: Option<MainEntity>,
    visible: bool,
    diagnostic: bool,
}

pub fn setup(app: &mut App) {
    let ready = PipBoyReady::default();
    app.insert_resource(ready.clone());
    let diagnostic = std::env::var_os("NVF_PLAY_TEST_PIPBOY_DRAW").is_some();
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.insert_resource(ready).insert_resource(Input { diagnostic, ..default() })
            .add_systems(ExtractSchedule, extract)
            .add_systems(Render, observe.in_set(RenderSystems::Cleanup));
        if diagnostic {
            render.add_systems(Render, report.after(observe).in_set(RenderSystems::Cleanup));
            info!("test aid NVF_PLAY_TEST_PIPBOY_DRAW: first visible frame and queue changes logged");
        }
    }
}

fn extract(
    mut commands: Commands,
    mut input: ResMut<Input>,
    mut probe: Option<ResMut<MaterialQueueProbe>>,
    pip: Extract<Res<PipBoy>>,
    shapes: Extract<Query<(Entity, &ViewVisibility), With<PipBoyShape>>>,
    camera: Extract<Query<Entity, With<nvf_play::PlayerCamera>>>,
) {
    input.visible = !pip.pose_hidden && pip.warming.is_none() && pip.on_arm() && matches!(pip.phase, Phase::Raising | Phase::Up);
    input.entities.clear();
    input.camera = camera.single().ok().map(MainEntity::from);
    if pip.on_arm() && (pip.warming.is_some() || pip.phase == Phase::Raising || input.diagnostic) {
        input.entities.extend(shapes.iter().filter(|(_, v)| v.get()).map(|(e, _)| MainEntity::from(e)));
    }
    if input.entities.is_empty() {
        commands.remove_resource::<MaterialQueueProbe>();
    } else if let Some(probe) = probe.as_mut() {
        probe.watched.clear();
        probe.watched.extend(input.entities.iter().copied());
        let watched = probe.watched.clone();
        for queued in probe.queued.values_mut() {
            queued.retain(|e, _| watched.contains(e));
        }
    } else {
        let mut probe = MaterialQueueProbe::default();
        probe.watched.extend(input.entities.iter().copied());
        commands.insert_resource(probe);
    }
}

#[derive(SystemParam)]
struct Observe<'w> {
    input: Res<'w, Input>,
    shared: Res<'w, PipBoyReady>,
    probe: Option<Res<'w, MaterialQueueProbe>>,
    instances: Res<'w, RenderMaterialInstances>,
    materials: Res<'w, ErasedRenderAssets<PreparedMaterial>>,
    allocators: Res<'w, MaterialBindGroupAllocators>,
    prepass: Res<'w, SpecializedPrepassMaterialPipelineCache>,
    cache: Res<'w, PipelineCache>,
    mesh_instances: Res<'w, RenderMeshInstances>,
    meshes: Res<'w, RenderAssets<RenderMesh>>,
    allocator: Res<'w, MeshAllocator>,
    main: Res<'w, SpecializedMaterialPipelineCache>,
}

fn observe(params: Observe) {
    let Observe { input, shared, probe, instances, materials, allocators, prepass, cache, mesh_instances, meshes, allocator, main } = params;
    let mut status = Status { shapes: input.entities.len(), ..default() };
    let mut pipelines = HashSet::<CachedRenderPipelineId>::new();
    for &e in &input.entities {
        let mut mesh_ready = false;
        if let Some(mesh) = mesh_instances.render_mesh_queue_data(e) {
            let uploaded = meshes.get(mesh.mesh_asset_id()).is_some();
            let allocated = allocator.mesh_slabs(&mesh.mesh_asset_id()).is_some();
            status.gpu_meshes += usize::from(uploaded);
            status.slabs += usize::from(allocated);
            mesh_ready = uploaded && allocated;
        }
        let specialized = main.iter().any(|(v, entries)| Some(v.main_entity) == input.camera && entries.contains_key(&e));
        status.specialized += usize::from(specialized);
        let queue = probe.as_ref().and_then(|p| p.queued.iter().find(|(v, _)| Some(v.main_entity) == input.camera)).and_then(|(_, q)| q.get(&e));
        let mut loaded = false;
        if let Some(&(pipeline, opaque)) = queue {
            status.queued += 1;
            status.opaque += usize::from(opaque);
            pipelines.insert(pipeline);
            loaded = cache.get_render_pipeline(pipeline).is_some();
        }
        // [design: bevy_pbr/prepass/mod.rs] Depth and colour own separate caches.
        for (_, entries) in prepass.iter().filter(|(v, _)| Some(v.main_entity) == input.camera) {
            if let Some(&(_, pipeline, _)) = entries.get(&e) {
                pipelines.insert(pipeline);
                loaded &= cache.get_render_pipeline(pipeline).is_some();
            }
        }
        let Some(instance) = instances.instances.get(&e) else { continue };
        let Some(material) = materials.get(instance.asset_id) else { continue };
        status.prepared += 1;
        let bound = allocators.get(&instance.asset_id.type_id()).and_then(|a| a.get(material.binding.group)).is_some_and(|b| b.bind_group().is_some());
        status.bound += usize::from(bound);
        status.ready += usize::from(loaded && bound && mesh_ready && specialized);
    }
    status.pipelines = pipelines.iter().map(|p| p.id()).collect();
    status.pipelines.sort_unstable();
    *shared.0.lock().expect("Pip-Boy render status") = status;
}

fn report(input: Res<Input>, shared: Res<PipBoyReady>, mut last: Local<Option<Status>>) {
    if !input.visible {
        *last = None;
        return;
    }
    let status = shared.status();
    if last.as_ref() == Some(&status) {
        return;
    }
    let first = last.is_none();
    info!("pipboy draw: first={first}, shapes={}, queued={}, opaque={}, prepared={}, bound={}, ready={}, missing={} gpu_meshes={}, slabs={}, specialized={} pipelines={:?}", status.shapes, status.queued, status.opaque, status.prepared, status.bound, status.ready, status.shapes.saturating_sub(status.ready), status.gpu_meshes, status.slabs, status.specialized, status.pipelines);
    *last = Some(status);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_pipelines_without_a_main_queue_or_binding_do_not_release_the_raise() {
        let mut s = Status { shapes: 3, prepared: 3, ready: 3, gpu_meshes: 3, slabs: 3, specialized: 3, ..default() };
        assert!(!s.complete());
        s.queued = 3;
        assert!(!s.complete());
        s.bound = 3;
        assert!(s.complete());
        s.ready = 2;
        assert!(s.submitted() && !s.complete());
        assert!(!Status::default().complete());
    }
}
