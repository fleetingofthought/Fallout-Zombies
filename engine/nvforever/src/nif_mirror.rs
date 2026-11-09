//! The drawn nodes for JIP's NIF block functions (fnv_game::nif_blocks;
//! docs/design/nif-blocks.md): each frame the scripts' node changes are applied to the
//! entities of the reference they name, and the node tree of every reference a script
//! asked about is told back. A reference's root is its actor rig, else the topmost
//! entity drawing it. A shape's NiMaterialProperty values (JIP's Get/SetMaterialPropertyValue)
//! are read from the reference's model file, once per model, with the scripts' changes
//! over them; a change draws the shape with its own copy of the material. The vertices of
//! the shapes a script asked for (SUP's NifBlockGetVerticles) are told back each frame.

use crate::limbs::ActorRig;
use bevy::prelude::*;
use fnv_game::nif_blocks::{SceneEdit, SceneNode};
use fnv_sim::FormId;
use nvf_assets::{GameData, NifHidden, NifMaterial};
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use std::collections::HashMap;

pub struct NifMirrorPlugin;

impl Plugin for NifMirrorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, mirror.before(TransformSystems::Propagate));
    }
}

/// What the mirror keeps between frames: the references it logged, the held transforms,
/// the models' material values, the scripts' material values and the shapes drawn with
/// their own material since.
#[derive(Default)]
struct Memory {
    logged: std::collections::HashSet<u32>,
    held: HashMap<(u32, String), Held>,
    tables: HashMap<String, HashMap<String, [f32; 9]>>,
    materials: HashMap<(u32, String), [f32; 9]>,
    owned: HashMap<Entity, Handle<NifMaterial>>,
}

/// Each named shape's NiMaterialProperty values in a model file (under meshes), in
/// GetMaterialPropertyValue's order: specular R G B, emissive R G B, glossiness, alpha,
/// emissive mult [documented: the page's value IDs; the fields: nif.xml
/// NiMaterialProperty]. The emissive mult is 1 in files without it.
fn material_table(data: &GameData, model: &str) -> HashMap<String, [f32; 9]> {
    let mut out = HashMap::new();
    if model.is_empty() {
        return out;
    }
    let Some(bytes) = data.read(&format!("meshes\\{model}")) else { return out };
    let Ok(file) = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)) else { return out };
    let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
    for i in 0..file.blocks.len() {
        let Some(s) = dec.get(i as i32) else { continue };
        let Some(name) = s.text("name").filter(|n| !n.is_empty()) else { continue };
        for &r in s.refs("properties").unwrap_or_default() {
            if !file.blocks.get(r as usize).is_some_and(|b| b.type_name == "NiMaterialProperty") {
                continue;
            }
            let Some(m) = dec.get(r) else { continue };
            let c = |f: &str| m.floats(f).filter(|v| v.len() >= 3).map_or([0.0; 3], |v| [v[0], v[1], v[2]]);
            let (sp, em) = (c("specular_color"), c("emissive_color"));
            let v = [sp[0], sp[1], sp[2], em[0], em[1], em[2], m.float("glossiness").unwrap_or(0.0), m.float("alpha").unwrap_or(1.0), m.float("emissive_mult").unwrap_or(1.0)];
            out.entry(name.to_string()).or_insert(v);
        }
    }
    out
}

/// A shape's material drawn with these values: the specular tint, the emitted light
/// (colour times mult), the roughness of the glossiness, the alpha [hypothesis: the
/// spawn's own display tuning of the emitted light is not redone here].
fn set_material(m: &mut NifMaterial, v: &[f32; 9]) {
    m.base.specular_tint = Color::linear_rgb(v[0], v[1], v[2]);
    m.base.emissive = LinearRgba::rgb(v[3] * v[8], v[4] * v[8], v[5] * v[8]);
    m.base.perceptual_roughness = nvf_assets::nif_spawn::gloss_roughness(v[6]);
    m.base.base_color.set_alpha(v[7].clamp(0.0, 1.0));
}

/// A node transform a script set, held over the animation.
#[derive(Clone, Copy, Default)]
struct Held {
    translation: Option<Vec3>,
    rotation: Option<Quat>,
    scale: Option<f32>,
}

type Visuals<'w, 's> = Query<'w, 's, (Entity, &'static RefVisual, Option<&'static ChildOf>), crate::lights::NotLightSource>;

type NodeQuery<'w, 's> = Query<'w, 's, (Option<&'static Name>, &'static mut Transform, Option<&'static Children>, Has<NifHidden>)>;

/// The root entity of a reference's drawing.
fn root_of(r: u32, actors: &Query<(Entity, &ActorRig)>, visuals: &Visuals) -> Option<Entity> {
    if let Some((e, _)) = actors.iter().find(|(_, a)| a.reference == r) {
        return Some(e);
    }
    visuals.iter().filter(|(_, v, _)| v.0 == r).find(|(_, _, p)| p.is_none_or(|p| visuals.get(p.parent()).is_ok_and(|(_, v, _)| v.0 != r))).map(|(e, _, _)| e)
}

/// The node entities under `root`, depth first, with their parent's index.
fn walk(root: Entity, nodes: &NodeQuery) -> Vec<(Entity, Option<usize>)> {
    let mut out = Vec::new();
    let mut stack: Vec<(Entity, Option<usize>)> = Vec::new();
    if let Ok((_, _, Some(ch), _)) = nodes.get(root) {
        stack.extend(ch.iter().rev().map(|c| (c, None)));
    }
    while let Some((e, parent)) = stack.pop() {
        let at = out.len();
        out.push((e, parent));
        if let Ok((_, _, Some(ch), _)) = nodes.get(e) {
            stack.extend(ch.iter().rev().map(|c| (c, Some(at))));
        }
    }
    out
}

/// An actor's skeleton node names, by entity (its node entities carry no `Name`).
fn rig_names(r: u32, actors: &Query<(Entity, &ActorRig)>) -> HashMap<Entity, String> {
    actors
        .iter()
        .find(|(_, a)| a.reference == r)
        .map(|(_, a)| a.nodes.iter().zip(&a.skel.names).filter_map(|(e, n)| n.clone().map(|n| (*e, n))).collect())
        .unwrap_or_default()
}

/// A node's name: its `Name`, else the rig's.
fn name_of(e: Entity, nodes: &NodeQuery, rig: &HashMap<Entity, String>) -> String {
    nodes.get(e).ok().and_then(|n| n.0.map(|n| n.as_str().to_string())).or_else(|| rig.get(&e).cloned()).unwrap_or_default()
}

/// The node of that name under `root`: exactly, else without regard to case.
fn find(root: Entity, name: &str, nodes: &NodeQuery, rig: &HashMap<Entity, String>) -> Option<Entity> {
    let all = walk(root, nodes);
    all.iter().map(|(e, _)| *e).find(|&e| name_of(e, nodes, rig) == name).or_else(|| all.iter().map(|(e, _)| *e).find(|&e| name_of(e, nodes, rig).eq_ignore_ascii_case(name)))
}

/// The material side of [`mirror`]: the data files, the shapes' materials, the assets.
type Materials<'w, 's> = (Option<Res<'w, GameData>>, Query<'w, 's, &'static MeshMaterial3d<NifMaterial>>, Option<ResMut<'w, Assets<NifMaterial>>>);

/// The shapes' meshes, for SUP's NifBlockGetVerticles.
type Shapes<'w, 's> = (Query<'w, 's, &'static Mesh3d>, Option<Res<'w, Assets<Mesh>>>);

#[allow(clippy::too_many_arguments)]
fn mirror(
    mut script: Option<ResMut<ScriptState>>,
    actors: Query<(Entity, &ActorRig)>,
    visuals: Visuals,
    parents: Query<&ChildOf>,
    mut nodes: NodeQuery,
    mut commands: Commands,
    mut mem: Local<Memory>,
    (data, drawn, mut assets): Materials,
    (shapes, meshes): Shapes,
) {
    let Memory { logged, held, tables, materials, owned } = &mut *mem;
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let edits = rt.game.scene.take_edits();
    for edit in edits {
        let r = match &edit {
            SceneEdit::Translation { r, .. } | SceneEdit::Rotation { r, .. } | SceneEdit::Scale { r, .. } | SceneEdit::Flags { r, .. } | SceneEdit::Rename { r, .. } | SceneEdit::Remove { r, .. } | SceneEdit::Insert { r, .. } | SceneEdit::Attach { r, .. } | SceneEdit::Material { r, .. } => *r,
        };
        let Some(root) = root_of(r.0, &actors, &visuals) else { continue };
        let rig = rig_names(r.0, &actors);
        match edit {
            // A transform a script sets holds over the animation, frame after frame
            // [observed: set-then-read stays exact on an animated bone].
            SceneEdit::Translation { node, v, .. } => held.entry((r.0, node)).or_default().translation = Some(Vec3::from(v)),
            SceneEdit::Rotation { node, v, .. } => {
                let [x, y, z] = v.map(f32::to_radians);
                held.entry((r.0, node)).or_default().rotation = Some(Quat::from_euler(EulerRot::ZYX, z, y, x));
            }
            SceneEdit::Scale { node, v, .. } => held.entry((r.0, node)).or_default().scale = Some(v),
            SceneEdit::Flags { node, flags, .. } => {
                if let Some(e) = find(root, &node, &nodes, &rig) {
                    if flags & 1 != 0 {
                        commands.entity(e).insert((NifHidden, Visibility::Hidden));
                    } else {
                        commands.entity(e).remove::<NifHidden>().insert(Visibility::Inherited);
                    }
                }
            }
            SceneEdit::Rename { node, to, .. } => {
                if let Some(e) = find(root, &node, &nodes, &rig) {
                    commands.entity(e).insert(Name::new(to.clone()));
                    if let Some(h) = held.remove(&(r.0, node)) {
                        held.insert((r.0, to), h);
                    }
                }
            }
            SceneEdit::Remove { node, .. } => {
                if let Some(e) = find(root, &node, &nodes, &rig) {
                    commands.entity(e).despawn();
                }
                held.remove(&(r.0, node));
            }
            SceneEdit::Insert { target, name, as_parent, .. } => {
                let Some(t) = find(root, &target, &nodes, &rig) else { continue };
                let new = commands.spawn((Name::new(name), Transform::IDENTITY, Visibility::Inherited)).id();
                if as_parent {
                    let above = parents.get(t).map(|p| p.parent()).unwrap_or(root);
                    commands.entity(above).add_child(new);
                    commands.entity(new).add_child(t);
                } else {
                    commands.entity(t).add_child(new);
                }
            }
            // The model is not drawn yet [hypothesis: slice 2 spawns it through
            // nvf_assets::SpawnCtx]; the mirror holds the request only.
            SceneEdit::Attach { .. } => {}
            // The mirror has the value already; the shape is drawn with its own copy of
            // the material from now on [hypothesis: the original's change is this
            // reference's; the page's note puts an inventory reference's on the base].
            SceneEdit::Material { node, id, v, .. } => {
                let Some(d) = data.as_deref() else { continue };
                let model = fnv_game::nif_blocks::model_of(&rt.game, r).unwrap_or_default();
                let table = tables.entry(model.clone()).or_insert_with(|| material_table(d, &model));
                let base = node.split(':').next().unwrap_or(&node).to_string();
                let Some(&start) = table.get(&node).or_else(|| table.get(&base)) else { continue };
                let vals = materials.entry((r.0, node.clone())).or_insert(start);
                vals[id] = v;
                let vals = *vals;
                let (Some(e), Some(assets)) = (find(root, &node, &nodes, &rig), assets.as_deref_mut()) else { continue };
                let Ok(handle) = drawn.get(e) else { continue };
                match owned.get(&e).filter(|h| *h == &handle.0).cloned() {
                    Some(h) => {
                        if let Some(mut m) = assets.get_mut(&h) {
                            set_material(&mut m, &vals);
                        }
                    }
                    None => {
                        let Some(mut m) = assets.get(&handle.0).cloned() else { continue };
                        set_material(&mut m, &vals);
                        let h = assets.add(m);
                        owned.insert(e, h.clone());
                        commands.entity(e).insert(MeshMaterial3d(h));
                    }
                }
            }
        }
    }
    // The held transforms, over this frame's pose.
    let keys: Vec<(u32, String)> = held.keys().cloned().collect();
    for (r, node) in keys {
        let Some(root) = root_of(r, &actors, &visuals) else { continue };
        let rig = rig_names(r, &actors);
        let h = held[&(r, node.clone())];
        if let Some(mut t) = find(root, &node, &nodes, &rig).and_then(|e| nodes.get_mut(e).ok()) {
            if let Some(v) = h.translation {
                t.1.translation = v;
            }
            if let Some(q) = h.rotation {
                t.1.rotation = q;
            }
            if let Some(s) = h.scale {
                t.1.scale = Vec3::splat(s);
            }
        }
    }
    // The trees of the references scripts asked about.
    let watched: Vec<FormId> = rt.game.scene.watched().collect();
    for r in watched {
        let Some(root) = root_of(r.0, &actors, &visuals) else {
            if logged.insert(r.0) {
                info!("nif mirror: {:08X} watched, not drawn", r.0);
            }
            rt.game.scene.forget(r);
            continue;
        };
        let rig = rig_names(r.0, &actors);
        let mut seen: HashMap<String, u32> = HashMap::new();
        let model = fnv_game::nif_blocks::model_of(&rt.game, r).unwrap_or_default();
        let table = match data.as_deref() {
            Some(d) => tables.entry(model.clone()).or_insert_with(|| material_table(d, &model)).clone(),
            None => HashMap::new(),
        };
        let tree: Vec<SceneNode> = walk(root, &nodes)
            .into_iter()
            .filter_map(|(e, parent)| {
                let base = name_of(e, &nodes, &rig);
                let (_, t, _, hidden) = nodes.get(e).ok()?;
                // A name seen before gets ":0", ":1" … [observed: "Object", "Object:0"].
                let n = seen.entry(base.to_ascii_lowercase()).or_insert(0);
                let shown = if *n == 0 { base.clone() } else { format!("{base}:{}", *n - 1) };
                *n += 1;
                let (z, y, x) = t.rotation.to_euler(EulerRot::ZYX);
                let material = materials.get(&(r.0, shown.clone())).or_else(|| table.get(&base)).copied();
                Some(SceneNode {
                    name: shown,
                    parent,
                    translation: t.translation.to_array(),
                    rotation: [x.to_degrees(), y.to_degrees(), z.to_degrees()],
                    scale: t.scale.x,
                    flags: u32::from(hidden),
                    material,
                })
            })
            .collect();
        if logged.insert(r.0 | 0x8000_0000) {
            info!("nif mirror: {:08X} told, {} nodes, first {:?}", r.0, tree.len(), tree.iter().take(4).map(|n| n.name.as_str()).collect::<Vec<_>>());
        }
        rt.game.scene.set_tree(r, tree);
    }
    // The vertices of the shapes scripts asked about (SUP's NifBlockGetVerticles), in the
    // shape's own space as its mesh holds them; none for a mesh kept on the GPU only
    // [hypothesis: a skinned shape's are its bind pose].
    let wanted: Vec<(FormId, String)> = rt.game.scene.wanted_vertices().cloned().collect();
    for (r, node) in wanted {
        let verts = root_of(r.0, &actors, &visuals)
            .and_then(|root| find(root, &node, &nodes, &rig_names(r.0, &actors)))
            .and_then(|e| shapes.get(e).ok())
            .and_then(|m| meshes.as_deref()?.get(&m.0))
            .and_then(|mesh| match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
                bevy::mesh::VertexAttributeValues::Float32x3(v) => Some(v.clone()),
                _ => None,
            });
        rt.game.scene.set_vertices(r, &node, verts);
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn a_light_source_is_not_a_references_root() {
        const R: u32 = 0x301;
        fn root(world: &mut World) -> Option<Entity> {
            world.run_system_once(|actors: Query<(Entity, &ActorRig)>, visuals: Visuals| root_of(R, &actors, &visuals)).unwrap()
        }
        // Without a parent, so that each entity passes the test on its parent and only the
        // filter on light sources can leave it out.
        let mut world = World::new();
        world.spawn((PointLight::default(), RefVisual(R)));
        world.spawn((SpotLight::default(), RefVisual(R)));
        assert_eq!(root(&mut world), None, "its light sources alone");
        let drawn = world.spawn((Transform::IDENTITY, RefVisual(R))).id();
        assert_eq!(root(&mut world), Some(drawn), "not a light source beside it");
    }
}
