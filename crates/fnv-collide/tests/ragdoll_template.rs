//! Ragdoll Templates (`.rdt`, docs/spec-notes/ragdoll.md §6) on a synthetic file: no
//! template ships with the game, so the layout (nif.xml bhkRagdollTemplate,
//! bhkRagdollTemplateData) is checked here only.

use fnv_collide::ragdoll::{template, JointKind, Ragdoll, RagdollBody, RagdollJoint};
use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_nif::scene::Decoded;
use fnv_nif::{NifFile, Schema};
use std::path::Path;

fn schema() -> Schema {
    Schema::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif")).unwrap()
}

#[derive(Default)]
struct W(Vec<u8>);
impl W {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn f(&mut self, v: &[f32]) -> &mut Self {
        for x in v {
            self.0.extend_from_slice(&x.to_le_bytes());
        }
        self
    }
    fn sized(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
        self
    }
    fn export(&mut self, s: &str) -> &mut Self {
        self.u8(s.len() as u8 + 1);
        self.0.extend_from_slice(s.as_bytes());
        self.u8(0)
    }
}

/// A 20.2.0.7 / bs 34 file of `blocks` (type name, bytes) with string table `strings`.
fn build(blocks: &[(&str, Vec<u8>)], strings: &[&str]) -> Vec<u8> {
    let mut types: Vec<&str> = Vec::new();
    for (t, _) in blocks {
        if !types.contains(t) {
            types.push(t);
        }
    }
    let mut w = W::default();
    w.0.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    w.u32(0x1402_0007).u8(1).u32(11).u32(blocks.len() as u32);
    w.u32(34).export("tester").export("Do Nothing").export(" PE Static Art");
    w.u16(types.len() as u16);
    for t in &types {
        w.sized(t);
    }
    for (t, _) in blocks {
        w.u16(types.iter().position(|x| x == t).unwrap() as u16);
    }
    for (_, b) in blocks {
        w.u32(b.len() as u32);
    }
    w.u32(strings.len() as u32).u32(strings.iter().map(|s| s.len()).max().unwrap_or(0) as u32);
    for s in strings {
        w.sized(s);
    }
    w.u32(0);
    for (_, b) in blocks {
        w.0.extend_from_slice(b);
    }
    w.u32(1).i32(0);
    w.0
}

const IDENTITY: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// Strings: 0 the root's name, 1 "RDT", 2 the thigh, 3 the pelvis, 4 a bone no skeleton has.
const STRINGS: [&str; 5] = ["Scene Root", "RDT", "Bip01 L Thigh", "Bip01 NonAccum", "Bip01 Tail"];

/// The root NiNode: no children, the template as its extra data (the wiki's layout).
fn root() -> Vec<u8> {
    let mut w = W::default();
    w.u32(0).u32(1).i32(1).i32(-1); // name, one extra data (block 1), no controller
    w.u32(14).f(&[0.0; 3]).f(&IDENTITY).f(&[1.0]);
    w.u32(0).i32(-1); // no properties, no collision object
    w.u32(0).u32(0); // no children, no effects
    w.0
}

fn template_list(bones: &[i32]) -> Vec<u8> {
    let mut w = W::default();
    w.u32(1).u32(bones.len() as u32);
    for b in bones {
        w.i32(*b);
    }
    w.0
}

/// A template bone with fields as given and `constraints` already encoded.
fn bone(name: u32, mass: f32, friction: f32, material: u32, constraints: &[Vec<u8>]) -> Vec<u8> {
    let mut w = W::default();
    w.u32(name).f(&[mass, 0.4, friction, 1.0]).u32(material).u32(constraints.len() as u32);
    for c in constraints {
        w.0.extend_from_slice(c);
    }
    w.0
}

/// A wrapped ragdoll constraint (type 7) between template blocks `a` and `b` (Havok 6.6
/// order, bs_version > 16), pivots in stored units.
fn ragdoll_constraint(a: i32, b: i32, pivot_a: [f32; 3], pivot_b: [f32; 3], limits: [f32; 5]) -> Vec<u8> {
    let mut w = W::default();
    w.u32(7).u32(2).i32(a).i32(b).u32(1);
    let v4 = |w: &mut W, v: [f32; 3]| {
        w.f(&[v[0], v[1], v[2], 0.0]);
    };
    for (twist, plane, pivot) in [([0.0, 0.0, -1.0], [1.0, 0.0, 0.0], pivot_a), ([0.0, 0.0, -1.0], [1.0, 0.0, 0.0], pivot_b)] {
        v4(&mut w, twist);
        v4(&mut w, plane);
        v4(&mut w, [0.0, 1.0, 0.0]); // motor = twist × plane
        v4(&mut w, pivot);
    }
    w.f(&limits).f(&[20.0]); // the five limits, max_friction
    w.u8(0); // no motor
    w.0
}

/// A wrapped limited hinge inside a malleable constraint (type 13 around type 2).
fn malleable_hinge(a: i32, b: i32, limits: [f32; 2], strength: f32) -> Vec<u8> {
    let mut w = W::default();
    w.u32(13).u32(2).i32(a).i32(b).u32(1); // the wrapper's info
    w.u32(2).u32(2).i32(a).i32(b).u32(1); // the malleable's type and info
    for v in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0]] {
        w.f(&[v[0], v[1], v[2], 0.0]);
    }
    for v in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]] {
        w.f(&[v[0], v[1], v[2], 0.0]);
    }
    w.f(&limits).f(&[100.0]).u8(0); // limits, max_friction, no motor
    w.f(&[strength]);
    w.0
}

fn rdt() -> Vec<u8> {
    build(
        &[
            ("NiNode", root()),
            ("bhkRagdollTemplate", template_list(&[2, 3, 4])),
            ("bhkRagdollTemplateData", bone(2, 25.0, 0.6, 11, &[ragdoll_constraint(2, 3, [0.0, 0.0, 1.0], [0.5, 0.0, -1.0], [0.3, -0.2, 0.4, -0.1, 0.1]), malleable_hinge(2, 3, [-1.0, 0.5], 0.9)])),
            ("bhkRagdollTemplateData", bone(3, 0.0, 0.3, 5, &[])),
            ("bhkRagdollTemplateData", bone(4, 2.0, 0.3, 5, &[])),
        ],
        &STRINGS,
    )
}

fn read(bytes: &[u8]) -> fnv_collide::ragdoll::RagdollTemplate {
    let schema = schema();
    let file = NifFile::parse(bytes, Some(&schema)).unwrap();
    let dec = Decoded::new(&file, bytes, &schema);
    template(&file, &dec).expect("the file has a bhkRagdollTemplate")
}

fn body(name: &str, mass: f32) -> RagdollBody {
    RagdollBody {
        object: CollisionObject {
            node: Some(name.into()),
            node_block: 0,
            node_transform: Iso::IDENTITY,
            body_transform: Iso::IDENTITY,
            motion: Motion::Keyframed,
            layer: 8,
            mass,
            friction: 0.3,
            restitution: 0.8,
            shape: Shape::Sphere { radius: 1.0 }.into(),
            shell: 0.0,
            material: Some(7),
            wind: false,
        },
        linear_damping: 0.1,
        angular_damping: 0.05,
        stored: Iso::IDENTITY,
        unit: 7.0,
    }
}

/// The skeleton side: pelvis, left thigh (on a ball joint to the pelvis), right thigh.
fn skeleton() -> Ragdoll {
    let joint = |a, b| RagdollJoint { block: 40 + a, a, b, pivot_a: [0.0; 3], pivot_b: [1.0; 3], kind: JointKind::Ball, max_friction: 0.0, strength: None };
    Ragdoll { bodies: vec![body("Bip01 NonAccum", 15.0), body("Bip01 L Thigh", 6.0), body("Bip01 R Thigh", 6.0)], joints: vec![joint(1, 0), joint(2, 0)], problems: Vec::new() }
}

#[test]
fn a_template_file_reads_as_nif_xml_lays_it_out() {
    let t = read(&rdt());
    assert!(t.problems.is_empty(), "{:?}", t.problems);
    assert_eq!(t.bones.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["Bip01 L Thigh", "Bip01 NonAccum", "Bip01 Tail"]);
    let thigh = &t.bones[0];
    assert_eq!((thigh.mass, thigh.restitution, thigh.friction, thigh.radius, thigh.material), (25.0, 0.4, 0.6, 1.0, Some(11)));
    assert_eq!(thigh.constraints.len(), 2);
    let c = &thigh.constraints[0];
    assert_eq!(c.attached_to.as_deref(), Some("Bip01 NonAccum"));
    assert_eq!((c.pivot_a, c.pivot_b, c.max_friction, c.strength), ([0.0, 0.0, 1.0], [0.5, 0.0, -1.0], 20.0, None));
    match c.kind {
        JointKind::Ragdoll { cone_max, plane_min, plane_max, twist_min, twist_max, twist_a, .. } => {
            assert_eq!([cone_max, plane_min, plane_max, twist_min, twist_max], [0.3, -0.2, 0.4, -0.1, 0.1]);
            assert_eq!(twist_a, [0.0, 0.0, -1.0]);
        }
        k => panic!("{k:?}"),
    }
    let h = &thigh.constraints[1];
    assert_eq!(h.strength, Some(0.9));
    assert!(matches!(h.kind, JointKind::Hinge { limits: Some([lo, hi]), .. } if lo == -1.0 && hi == 0.5), "{:?}", h.kind);
}

#[test]
fn a_template_overrides_the_bodies_and_joints_of_its_bones() {
    let mut r = skeleton();
    let applied = r.apply_template(&read(&rdt()));
    // Thigh and pelvis matched; the tail has no body.
    assert_eq!(applied.bones, 2);
    assert!(applied.skipped.iter().any(|s| s.starts_with("Bip01 Tail")), "{:?}", applied.skipped);
    // The thigh's body takes the template's values.
    let thigh = &r.bodies[1].object;
    assert_eq!((thigh.mass, thigh.friction, thigh.restitution, thigh.material), (25.0, 0.6, 0.4, Some(11)));
    // The pelvis's stored mass of 0 is not taken: a body without mass would not fall.
    assert_eq!(r.bodies[0].object.mass, 15.0);
    assert!(applied.skipped.iter().any(|s| s.starts_with("Bip01 NonAccum: mass 0")), "{:?}", applied.skipped);
    // The thigh's joint to the pelvis is replaced by the template's first constraint, with
    // its pivots in game units; the second constraint (also to the pelvis) has no second
    // joint of that pair to replace and is added.
    assert_eq!((applied.joints_replaced, applied.joints_added), (1, 1));
    let j = &r.joints[0];
    assert_eq!((j.a, j.b, j.block), (1, 0, 41));
    assert_eq!((j.pivot_a, j.pivot_b), ([0.0, 0.0, 7.0], [3.5, 0.0, -7.0]));
    assert!(matches!(j.kind, JointKind::Ragdoll { cone_max, .. } if cone_max == 0.3));
    // The right thigh is untouched.
    assert_eq!((r.joints[1].a, r.joints[1].kind), (2, JointKind::Ball));
    assert_eq!(r.bodies[2].object.mass, 6.0);
    assert!(matches!(r.joints[2].kind, JointKind::Hinge { .. }) && r.joints[2].strength == Some(0.9));
}

#[test]
fn constraints_without_a_named_bone_replace_by_order() {
    // The thigh's ragdoll constraint with entities that are not template bones.
    let bytes = build(
        &[
            ("NiNode", root()),
            ("bhkRagdollTemplate", template_list(&[2])),
            ("bhkRagdollTemplateData", bone(2, 9.0, 0.3, 5, &[ragdoll_constraint(-1, -1, [0.0; 3], [0.0; 3], [0.5, -0.5, 0.5, -0.5, 0.5]), ragdoll_constraint(-1, -1, [0.0; 3], [0.0; 3], [0.5; 5])])),
        ],
        &STRINGS,
    );
    let t = read(&bytes);
    assert_eq!(t.bones[0].constraints[0].attached_to, None);
    let mut r = skeleton();
    let applied = r.apply_template(&t);
    assert_eq!((applied.joints_replaced, applied.joints_added), (1, 0));
    assert!(matches!(r.joints[0].kind, JointKind::Ragdoll { .. }) && r.joints[0].b == 0);
    // The second has no second joint of the thigh to replace.
    assert!(applied.skipped.iter().any(|s| s == "Bip01 L Thigh: constraint 1 has no joint to replace"), "{:?}", applied.skipped);
}

#[test]
fn a_file_without_a_template_is_not_one() {
    let bytes = build(&[("NiNode", root())], &STRINGS);
    let schema = schema();
    let file = NifFile::parse(&bytes, Some(&schema)).unwrap();
    let dec = Decoded::new(&file, &bytes, &schema);
    assert!(template(&file, &dec).is_none());
}
