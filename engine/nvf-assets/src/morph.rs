//! Vertex morphs on models (docs/spec-notes/vertex-morphs.md): a shape's
//! `NiGeomMorpherController` blends its `NiMorphData` targets by weights over time
//! (waving flags, sagging wires, the Big MT's holograms).
//!
//! A morphing shape's mesh is kept in the main world too (its usage includes
//! `MAIN_WORLD`), registered here by asset id when the model is prepared
//! (`ModelCtx::prepare`), and rewritten every frame from the shared clock: every placed
//! copy of a model shares its meshes, so they all wave in step [hypothesis: the original's
//! controllers run on the game's clock the same way; no capture has compared two flags].
//! Normals are left as stored: every morph controller of the base archives has
//! `morpher_flags` UPDATE_NORMALS_DISABLED [observed: `examples/morph_survey.rs`;
//! documented: nif.xml GeomMorpherFlags].

use crate::anim_play::OwnCycle;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use fnv_nif::anim::ValueTrack;
use std::collections::HashSet;
use std::sync::Arc;

/// One morphing shape: its stored vertices, the later morphs' offsets with their
/// weights, and the timing they run on.
#[derive(Debug)]
pub struct ShapeMorph {
    /// `<model path> <shape name>`, for the log.
    pub label: String,
    /// Morph 0, the shape's stored vertices.
    pub base: Vec<[f32; 3]>,
    /// Each later morph's offsets and its weight over time (`None`: weight 0).
    pub offsets: Vec<(Vec<[f32; 3]>, Option<ValueTrack>)>,
    pub cycle: OwnCycle,
}

impl ShapeMorph {
    /// The weights at `clock` seconds of play.
    pub fn weights(&self, clock: f32) -> Vec<f32> {
        let t = self.cycle.time(clock);
        self.offsets.iter().map(|(_, w)| w.as_ref().and_then(|w| w.at(t)).map_or(0.0, |v| v[0])).collect()
    }

    /// The vertex positions for `weights`: the base plus each morph times its weight
    /// (relative targets [hypothesis], docs/spec-notes/vertex-morphs.md §2).
    pub fn positions(&self, weights: &[f32]) -> Vec<[f32; 3]> {
        let mut out = self.base.clone();
        for ((offsets, _), &w) in self.offsets.iter().zip(weights) {
            if w == 0.0 {
                continue;
            }
            for (p, v) in out.iter_mut().zip(offsets) {
                p[0] += w * v[0];
                p[1] += w * v[1];
                p[2] += w * v[2];
            }
        }
        out
    }
}

/// The meshes that morph (kept in [`crate::ModelCache`]), by asset id (weak: a mesh nothing holds any more is dropped
/// from the list), with the weights they were last written for.
#[derive(Default)]
pub struct MorphMeshes {
    entries: Vec<(AssetId<Mesh>, Arc<ShapeMorph>, Vec<f32>)>,
    logged: HashSet<String>,
    /// Meshes rewritten, summed over frames (the `--run-seconds` summary).
    pub writes: u64,
}

impl MorphMeshes {
    pub fn add(&mut self, mesh: AssetId<Mesh>, morph: Arc<ShapeMorph>) {
        self.entries.push((mesh, morph, Vec::new()));
    }

    /// Morphing meshes registered and still present.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The registered meshes (nvforever's profile aid, `profile change sources:`).
    pub fn ids(&self) -> impl Iterator<Item = AssetId<Mesh>> + '_ {
        self.entries.iter().map(|(id, ..)| *id)
    }
}

/// Rewrite every registered mesh whose weights changed since its last write.
pub fn play_vertex_morphs(time: Res<Time>, mut cache: ResMut<crate::ModelCache>, mut meshes: ResMut<Assets<Mesh>>) {
    let clock = time.elapsed_secs();
    let MorphMeshes { entries, logged, writes } = &mut cache.morphs;
    entries.retain_mut(|(id, morph, last)| {
        // Read the weights first: a mesh unchanged is not touched (no re-upload).
        let w = morph.weights(clock);
        if *last == w {
            return meshes.contains(*id);
        }
        let Some(mut mesh) = meshes.get_mut(*id) else { return false };
        let positions = morph.positions(&w);
        if positions.len() != mesh.count_vertices() {
            return false;
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, VertexAttributeValues::Float32x3(positions));
        if !last.is_empty() && logged.insert(morph.label.clone()) {
            info!("vertex morphs: {} running ({} morph(s), weights {:?})", morph.label, w.len(), w.iter().map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>());
        }
        *last = w;
        *writes += 1;
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_nif::anim::{Channel, Key, KeyKind, Keys};

    #[test]
    fn base_plus_weighted_offsets() {
        let ramp = Keys { kind: KeyKind::Linear, keys: vec![Key { time: 0.0, value: [0.0], forward: [0.0], backward: [0.0], tbc: [0.0; 3] }, Key { time: 2.0, value: [1.0], forward: [0.0], backward: [0.0], tbc: [0.0; 3] }] };
        let m = ShapeMorph {
            label: "flag.nif Flag:0".into(),
            base: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]],
            offsets: vec![(vec![[0.0, 0.0, 0.0], [0.0, 4.0, 0.0]], Some(ValueTrack::Float(Channel::Keys(ramp)))), (vec![[1.0, 0.0, 0.0], [1.0, 0.0, 0.0]], None)],
            cycle: OwnCycle { start: 0.0, stop: 2.0, frequency: 1.0, phase: 0.0, cycle_type: 0 },
        };
        // Half way through the loop, and again one cycle later.
        for clock in [1.0, 3.0] {
            let w = m.weights(clock);
            assert!((w[0] - 0.5).abs() < 1e-5 && w[1] == 0.0);
            assert_eq!(m.positions(&w), vec![[0.0, 0.0, 0.0], [10.0, 2.0, 0.0]]);
        }
    }
}
