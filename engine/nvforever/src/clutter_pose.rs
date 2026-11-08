//! The pose a designer left a placed jointed object in: clutter skeletons
//! (`meshes\clutter\skeleton\…`, MSTT) whose reference carries `XRGD` "Ragdoll Bones"
//! (docs/spec-notes/dead-on-spawn.md §5.1). Without it they stood in the file's own pose,
//! upright with the arms out, where the original shows them lying on the floor (playtest
//! list 5 item 71, the Goodsprings cave).
//!
//! What the entries mean here [observed: `fnvtool esm decode` and `nif decode` of the
//! Goodsprings cave's three skeletons, `skeletonfemale50scasual.nif` and
//! `skeletonmale50scasual.nif`, 17 bodies and 17 entries each]:
//!
//! - Entry k belongs to the file's k-th collision object (the order `fnv_collide` reads
//!   them in, the blocks' order).
//! - Each entry is its body node's transform under the node's parent, in the convention
//!   of a reference's rotation (`authored_pose`): for a node under another body node the
//!   positions repeat the file's own local translations (forearm under upper arm 23.81 in
//!   both, head under torso 45.09 against 45.10, right upper arm under torso (40.72, −11.59,
//!   −2.33) against (40.76, −11.68, −2.21)); for a node under the file's root, the root
//!   standing for the reference. Read so, the bodies of the three lie within 15 to 35 units
//!   of height of each other, scattered as a fallen skeleton's, the first entry at the
//!   reference's position [hypothesis for the root-level ones: the alternative, under the
//!   first body, put the thighs 30 units below the arm].
//! - Nodes without a body keep their place under their parent.
//!
//! The collision is posed by the same rule from the same entries, on the physics side
//! (`nvf_play::phys::prepare`, `fnv_collide::nif::NodeTree`), and held where it lies.

use crate::authored_pose::{self, Bone};
use bevy::math::{Affine3A, Mat3, Vec3};
use fnv_nif::scene::Decoded;
use fnv_nif::NifFile;
use nvf_assets::data::GameData;
use nvf_assets::model::ModelNode;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};

/// The nodes a file's collision objects hang on, in body order.
type BodyNodes = Arc<Vec<Option<String>>>;

/// [`BodyNodes`] by archive path (`None`: no collision).
static BODIES: LazyLock<Mutex<HashMap<String, Option<BodyNodes>>>> = LazyLock::new(Default::default);

pub use fnv_world::xrgd::{matching, posable, stored};

/// The nodes the collision objects of the file at `path` hang on, in body order. A file
/// that breaks the reader (a panic, caught) counts as one without collision.
pub fn body_nodes(data: &GameData, path: &str) -> Option<BodyNodes> {
    let key = fnv_vfs::normalize(path);
    let mut cache = BODIES.lock().ok()?;
    cache
        .entry(key.clone())
        .or_insert_with(|| {
            let bytes = data.read(&key)?;
            let read = nvf_assets::guard::guarded(|| {
                let Ok(file) = NifFile::parse(&bytes, Some(&data.schema)) else { return Ok(None) };
                let dec = Decoded::new(&file, &bytes, &data.schema);
                let ex = fnv_collide::nif::extract_collision(&file, &dec);
                Ok((!ex.objects.is_empty()).then(|| Arc::new(ex.objects.iter().map(|o| o.node_name.clone()).collect())))
            });
            read.unwrap_or_else(|e| {
                bevy::log::warn!("clutter pose: {key}: collision not read ({e}); left in the file's pose");
                None
            })
        })
        .clone()
}

/// Every node's transform from the model's root (game space, the root's own left out)
/// with the bodies' nodes put where `bones` says (module notes); `Err` with the reason
/// when the entries do not match the bodies (`xrgd::matching`), a body's node is missing,
/// an entry is not finite or a node's parent does not come before it (a broken record or
/// model).
pub fn pose(nodes: &[ModelNode], bodies: &[Option<String>], bones: &[Bone]) -> Result<Vec<Affine3A>, &'static str> {
    let n = matching(bones.len(), bodies.len())?;
    let bones = &bones[..n];
    if !bones.iter().all(|b| b.position.iter().chain(&b.angles).all(|v| v.is_finite())) {
        return Err("an entry is not finite");
    }
    let mut local: Vec<Affine3A> = nodes.iter().map(|n| n.local).collect();
    for (body, bone) in bodies.iter().zip(bones) {
        let name = body.as_deref().ok_or("a body's node is unnamed")?;
        let i = nodes.iter().position(|n| n.name.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(name))).ok_or("a body's node is not in the model")?;
        let scale = Vec3::new(local[i].matrix3.x_axis.length(), local[i].matrix3.y_axis.length(), local[i].matrix3.z_axis.length());
        let r: Mat3 = authored_pose::rotation(bone.angles);
        local[i] = Affine3A::from_mat3_translation(r * Mat3::from_diagonal(scale), Vec3::from(bone.position));
    }
    // Parents come before their children (`model_nodes`).
    let mut world: Vec<Affine3A> = Vec::with_capacity(nodes.len());
    for (i, n) in nodes.iter().enumerate() {
        let parent = match n.parent {
            Some(p) => *world.get(p).ok_or("a node's parent comes after it")?,
            None => Affine3A::IDENTITY,
        };
        world.push(parent * local[i]);
    }
    Ok(world)
}

/// Once per model, log why a reference's `XRGD` does not pose it (issue #1400). `why` is
/// [`matching`]'s count reason or [`pose`]'s model reason.
pub fn log_refused(path: &str, why: &str, bones: &[Bone], bodies: &[Option<String>]) {
    static TOLD: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);
    let key = fnv_vfs::normalize(path);
    if TOLD.lock().ok().map(|mut s| s.insert(key)).unwrap_or(false) {
        let count = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        bevy::log::warn!(
            "clutter pose: {path}: its XRGD does not fit ({why}; {}, {}); left in the file's pose",
            count(bones.len(), "entry", "entries"),
            count(bodies.len(), "body", "bodies"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, parent: Option<usize>, t: Vec3) -> ModelNode {
        ModelNode { name: Some(name.into()), parent, local: Affine3A::from_translation(t), billboard: None, addon: None, damage_stages: None }
    }

    #[test]
    fn bodies_take_their_entries_and_children_follow() {
        // Root → Torso (body 0) → Ribs (no body) and Head (body 1).
        let nodes = vec![node("Torso", None, Vec3::new(0.0, 0.0, 69.0)), node("Ribs", Some(0), Vec3::new(10.0, 0.0, 0.0)), node("Head", Some(0), Vec3::new(45.0, 0.0, 0.0))];
        let bodies = vec![Some("Torso".to_string()), Some("head".to_string())];
        // The torso lies at the reference turned 90° about z (a clockwise angle, as
        // references store it), the head 45 out along it, unturned.
        let bones = [Bone { position: [0.0; 3], angles: [0.0, 0.0, -std::f32::consts::FRAC_PI_2] }, Bone { position: [45.0, 0.0, 0.0], angles: [0.0; 3] }];
        let w = pose(&nodes, &bodies, &bones).unwrap();
        let at = |i: usize| w[i].translation;
        assert!((Vec3::from(at(0)) - Vec3::ZERO).length() < 1e-4);
        assert!((Vec3::from(at(2)) - Vec3::new(0.0, 45.0, 0.0)).length() < 1e-4, "{:?}", at(2));
        // The ribs keep their place under the torso.
        assert!((Vec3::from(at(1)) - Vec3::new(0.0, 10.0, 0.0)).length() < 1e-4, "{:?}", at(1));
        assert!(pose(&nodes, &bodies, &bones[..1]).is_err(), "fewer entries than bodies");
        assert!(pose(&nodes, &[Some("Torso".into()), Some("Tail".into())], &bones).is_err(), "a body's node must exist");
        let nan = [bones[0], Bone { position: [f32::NAN, 0.0, 0.0], ..bones[1] }];
        assert_eq!(pose(&nodes, &bodies, &nan).unwrap_err(), "an entry is not finite");
        let looped = vec![node("Torso", Some(1), Vec3::ZERO), node("Head", Some(0), Vec3::ZERO)];
        assert_eq!(pose(&looped, &bodies, &bones).unwrap_err(), "a node's parent comes after it");
    }

    #[test]
    fn a_skeleton_part_poses_from_the_first_entries() {
        // A torso (five bodies: Torso, Head, RightUpArm, RightForeArm, RightHand) copied
        // with the whole clothed skeleton's 15-entry XRGD poses from the first five
        // entries; the ten extra entries are ignored (issue #1400).
        let nodes = vec![
            node("Torso", None, Vec3::ZERO),
            node("Head", Some(0), Vec3::new(45.0, 0.0, 0.0)),
            node("RightUpArm", Some(0), Vec3::new(40.0, -11.0, -2.0)),
            node("RightForeArm", Some(2), Vec3::new(23.0, 0.0, 0.0)),
            node("RightHand", Some(3), Vec3::new(18.0, 0.0, 0.0)),
        ];
        let bodies: Vec<Option<String>> = ["Torso", "Head", "RightUpArm", "RightForeArm", "RightHand"].iter().map(|n| Some(n.to_string())).collect();
        let mut bones: Vec<Bone> = [0.0, 45.0, 40.0, 23.0, 18.0].iter().map(|&x| Bone { position: [x, 0.0, 0.0], angles: [0.0; 3] }).collect();
        while bones.len() < 15 {
            bones.push(Bone { position: [999.0; 3], angles: [0.0; 3] });
        }
        let w = pose(&nodes, &bodies, &bones).unwrap();
        // The hand sits at the end of its chain (Torso 0 → RightUpArm 40 → RightForeArm
        // 23 → RightHand 18): the first five entries posed it, the extras did not interfere.
        let hand = Vec3::from(w[4].translation);
        assert!((hand.x - 81.0).abs() < 1e-3, "{hand:?}");
        // One body cannot stand for several entries.
        assert_eq!(pose(&nodes, &bodies[..1], &bones).unwrap_err(), "one body, but more XRGD entries than it can hold");
    }
}
