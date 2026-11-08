//! Ragdolls on hand-built worlds (no game data): the joint conventions against
//! `fnv_collide::ragdoll`'s angles, a jointed figure dropped on flat ground, contacts.

use fnv_collide::ragdoll::{JointKind, Ragdoll, RagdollBody, RagdollJoint};
use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::*;
use fnv_world::land::SIDE;
use std::sync::Arc;

const ID: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
const X: [f32; 3] = [1.0, 0.0, 0.0];
const Y: [f32; 3] = [0.0, 1.0, 0.0];
const Z: [f32; 3] = [0.0, 0.0, 1.0];

/// Flat ground at z = 0 around the origin.
fn ground() -> PhysWorld {
    let mut w = PhysWorld::new(PhysParams::default());
    for x in -1..=0 {
        for y in -1..=0 {
            w.add_terrain((x, y), &vec![0.0; SIDE * SIDE]);
        }
    }
    w
}

fn at(p: [f32; 3]) -> Iso {
    Iso { translation: p, rotation: ID }
}

/// A body whose bone stands at `rest` in the bind pose.
fn body(name: &str, shape: Shape, mass: f32, rest: [f32; 3]) -> RagdollBody {
    RagdollBody {
        object: CollisionObject {
            node: Some(name.into()),
            node_block: 0,
            node_transform: at(rest),
            body_transform: Iso::IDENTITY,
            motion: Motion::Keyframed,
            layer: layers::BIPED,
            mass,
            friction: 0.3,
            restitution: 0.8,
            shape: shape.into(),
            shell: 0.0,
            material: None,
            wind: false,
        },
        linear_damping: 0.1,
        angular_damping: 0.05,
        stored: at(rest),
        unit: 7.0,
    }
}

fn joint(a: usize, b: usize, pivot_a: [f32; 3], pivot_b: [f32; 3], kind: JointKind) -> RagdollJoint {
    RagdollJoint { block: 0, a, b, pivot_a, pivot_b, kind, max_friction: 100.0, strength: None }
}

fn start<'a>(form_id: u32, ragdoll: &'a Arc<Ragdoll>, nodes: &'a [Iso]) -> RagdollStart<'a> {
    RagdollStart { form_id, ragdoll, scale: 1.0, nodes, velocity: [0.0; 3], parts: &[], asleep: false }
}

fn nodes(w: &PhysWorld, id: u32) -> Vec<Iso> {
    w.ragdoll_nodes(id).unwrap().into_iter().map(|b| b.map(|b| Iso { translation: b.translation, rotation: b.rotation }).unwrap()).collect()
}

/// A heavy block on the ground with an arm sticking out level along +y from a joint on
/// its side: gravity turns the arm down about the world x axis until the joint stops it.
/// Returns the joint's angles after `seconds`.
fn arm_on_block(kind: JointKind, seconds: f32) -> fnv_collide::ragdoll::JointAngles {
    let mut w = ground();
    let rag = Arc::new(Ragdoll {
        bodies: vec![
            body("arm", Shape::Capsule { a: [0.0, 4.0, 0.0], b: [0.0, 30.0, 0.0], radius: 3.0 }, 1.0, [0.0, 25.0, 39.0]),
            body("block", Shape::Box { half_extents: [20.0, 20.0, 20.0] }, 2000.0, [0.0, 0.0, 24.0]),
        ],
        joints: vec![joint(0, 1, [0.0; 3], [0.0, 25.0, 15.0], kind)],
        problems: vec![],
    });
    let rest: Vec<Iso> = rag.bodies.iter().map(|b| b.rest()).collect();
    let r = w.add_ragdoll(&start(7, &rag, &rest));
    assert_eq!((r.bodies, r.joints, r.errors.len()), (2, 1, 0));
    for _ in 0..(seconds * 60.0) as usize {
        w.step();
    }
    let n = nodes(&w, 7);
    assert!(rag.joints[0].pivot_gap(&n[0], &n[1]) < 0.5, "the joint holds: gap {}", rag.joints[0].pivot_gap(&n[0], &n[1]));
    rag.joints[0].angles(&n[0], &n[1])
}

#[test]
fn non_finite_joints_and_damping_are_left_out() {
    let mut w = ground();
    let mut arm = body("arm", Shape::Capsule { a: [0.0, 4.0, 0.0], b: [0.0, 30.0, 0.0], radius: 3.0 }, 1.0, [0.0, 25.0, 39.0]);
    arm.linear_damping = f32::NAN;
    arm.angular_damping = f32::INFINITY;
    let rag = Arc::new(Ragdoll {
        bodies: vec![arm, body("block", Shape::Box { half_extents: [20.0, 20.0, 20.0] }, 2000.0, [0.0, 0.0, 24.0])],
        joints: vec![
            joint(0, 1, [f32::NAN; 3], [0.0, 25.0, 15.0], JointKind::Ball),
            joint(0, 1, [0.0; 3], [0.0, 25.0, 15.0], JointKind::Spring { length: f32::NAN }),
        ],
        problems: vec![],
    });
    let rest: Vec<Iso> = rag.bodies.iter().map(|b| b.rest()).collect();
    let r = w.add_ragdoll(&RagdollStart { velocity: [f32::NAN; 3], ..start(7, &rag, &rest) });
    assert_eq!((r.bodies, r.joints, r.errors.len()), (2, 0, 2));
    for _ in 0..60 {
        w.step();
    }
    assert!(nodes(&w, 7).iter().all(|n| n.translation.iter().all(|v| v.is_finite())));
}

#[test]
fn a_hinge_stops_at_the_limit_on_its_side() {
    let hinge = |limits| JointKind::Hinge { axis_a: X, perp_a: Y, axis_b: X, perp_b: Y, limits: Some(limits) };
    // Falling is a negative turn about x (y towards −z).
    let a = arm_on_block(hinge([-0.2, 1.2]), 4.0);
    assert!((a.twist + 0.2).abs() < 0.06, "{a:?}");
    let a = arm_on_block(hinge([-1.2, 0.2]), 4.0);
    assert!((a.twist + 1.2).abs() < 0.06, "{a:?}");
}

#[test]
fn a_ragdoll_joint_stops_at_its_plane_limit() {
    // Twist along the arm, plane axis up: falling leans the twist axis away from it.
    let rag = |plane_min, plane_max| JointKind::Ragdoll {
        twist_a: Y,
        plane_a: Z,
        twist_b: Y,
        plane_b: Z,
        cone_max: 1.4,
        plane_min,
        plane_max,
        twist_min: -0.1,
        twist_max: 0.1,
    };
    let a = arm_on_block(rag(-0.3, 0.9), 4.0);
    assert!((a.plane + 0.3).abs() < 0.06 && a.twist.abs() < 0.15, "{a:?}");
    let a = arm_on_block(rag(-0.9, 0.3), 4.0);
    assert!((a.plane + 0.9).abs() < 0.06, "{a:?}");
}

#[test]
fn a_ragdoll_joint_stops_at_its_cone_and_twist_limits() {
    // Plane limits wider than the cone: the cone stops the fall.
    let cone = JointKind::Ragdoll { twist_a: Y, plane_a: Z, twist_b: Y, plane_b: Z, cone_max: 0.5, plane_min: -1.2, plane_max: 1.2, twist_min: -0.1, twist_max: 0.1 };
    let a = arm_on_block(cone, 4.0);
    assert!((a.cone - 0.5).abs() < 0.06, "{a:?}");
    // Twist about x with the arm out along the plane axis: falling is a negative twist.
    let twist = |twist_min, twist_max| JointKind::Ragdoll { twist_a: X, plane_a: Y, twist_b: X, plane_b: Y, cone_max: 0.3, plane_min: -0.3, plane_max: 0.3, twist_min, twist_max };
    let a = arm_on_block(twist(-0.2, 0.6), 4.0);
    assert!((a.twist + 0.2).abs() < 0.06 && a.cone < 0.1, "{a:?}");
    let a = arm_on_block(twist(-0.6, 0.2), 4.0);
    assert!((a.twist + 0.6).abs() < 0.06, "{a:?}");
}

/// A small standing figure: pelvis, chest, head, two thighs, two calves; the bones'
/// x axes are the world's (identity rotations), limbs hang along −z.
fn figure() -> Arc<Ragdoll> {
    let cap = |a: [f32; 3], b: [f32; 3], radius: f32| Shape::Capsule { a, b, radius };
    let ball = |cone: f32, twist: f32| JointKind::Ragdoll { twist_a: Z, plane_a: X, twist_b: Z, plane_b: X, cone_max: cone, plane_min: -cone, plane_max: cone, twist_min: -twist, twist_max: twist };
    let knee = JointKind::Hinge { axis_a: X, perp_a: Y, axis_b: X, perp_b: Y, limits: Some([-0.1, 2.2]) };
    Arc::new(Ragdoll {
        bodies: vec![
            body("pelvis", cap([-6.0, 0.0, 0.0], [6.0, 0.0, 0.0], 7.0), 12.0, [0.0, 0.0, 66.0]),
            body("chest", cap([0.0, 0.0, 6.0], [0.0, 0.0, 26.0], 9.0), 20.0, [0.0, 0.0, 74.0]),
            body("head", Shape::Sphere { radius: 8.0 }, 4.0, [0.0, 0.0, 120.0]),
            body("l thigh", cap([0.0, 0.0, -4.0], [0.0, 0.0, -28.0], 5.0), 7.0, [-8.0, 0.0, 62.0]),
            body("l calf", cap([0.0, 0.0, -3.0], [0.0, 0.0, -24.0], 4.0), 4.0, [-8.0, 0.0, 32.0]),
            body("r thigh", cap([0.0, 0.0, -4.0], [0.0, 0.0, -28.0], 5.0), 7.0, [8.0, 0.0, 62.0]),
            body("r calf", cap([0.0, 0.0, -3.0], [0.0, 0.0, -24.0], 4.0), 4.0, [8.0, 0.0, 32.0]),
        ],
        joints: vec![
            joint(1, 0, [0.0; 3], [0.0, 0.0, 8.0], ball(0.4, 0.2)),
            joint(2, 1, [0.0, 0.0, -9.0], [0.0, 0.0, 37.0], ball(0.5, 0.6)),
            joint(3, 0, [0.0; 3], [-8.0, 0.0, -4.0], ball(0.9, 0.2)),
            joint(4, 3, [0.0; 3], [0.0, 0.0, -30.0], knee),
            joint(5, 0, [0.0; 3], [8.0, 0.0, -4.0], ball(0.9, 0.2)),
            joint(6, 5, [0.0; 3], [0.0, 0.0, -30.0], knee),
        ],
        problems: vec![],
    })
}

/// Drop the figure from standing (feet a little above the ground, leaning a little so
/// it does not balance) and step until it sleeps or `seconds` pass.
fn drop_figure(w: &mut PhysWorld, id: u32, origin: [f32; 3], seconds: f32) -> (Arc<Ragdoll>, RagdollReport, bool) {
    let rag = figure();
    let lean = Iso { translation: origin, rotation: { let (s, c) = 0.12f32.sin_cos(); [1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c] } };
    let poses: Vec<Iso> = rag.bodies.iter().map(|b| lean.compose(&b.rest())).collect();
    let report = w.add_ragdoll(&start(id, &rag, &poses));
    let mut slept = false;
    for _ in 0..(seconds * 60.0) as usize {
        w.step();
        if w.ragdoll_asleep(id) {
            slept = true;
            break;
        }
    }
    (rag, report, slept)
}

#[test]
fn a_figure_falls_and_comes_to_rest_on_the_ground() {
    let mut w = ground();
    let (rag, report, slept) = drop_figure(&mut w, 0x10, [100.0, 100.0, 6.0], 20.0);
    assert_eq!((report.bodies, report.joints), (7, 6));
    assert!(report.errors.is_empty());
    assert!(slept, "the figure comes to rest");
    let n = nodes(&w, 0x10);
    // Lying on the ground: nothing below it, the head no longer up at 120.
    for (i, p) in n.iter().enumerate() {
        assert!(p.translation[2] > 0.0 && p.translation[2] < 45.0, "body {i} at {:?}", p.translation);
    }
    // The joints hold and stay near their limits.
    for j in &rag.joints {
        let (a, b) = (&n[j.a], &n[j.b]);
        assert!(j.pivot_gap(a, b) < 1.5, "joint {}–{} gap {}", j.a, j.b, j.pivot_gap(a, b));
        assert!(j.within(&j.angles(a, b), 0.25), "joint {}–{} at {:?}", j.a, j.b, j.angles(a, b));
    }
    // It did not slide far.
    assert!((n[0].translation[0] - 100.0).abs() < 80.0 && (n[0].translation[1] - 100.0).abs() < 120.0, "{:?}", n[0].translation);
}

#[test]
fn falls_repeat_exactly() {
    let run = || {
        let mut w = ground();
        drop_figure(&mut w, 0x10, [100.0, 100.0, 6.0], 3.0);
        w.state_digest()
    };
    assert_eq!(run(), run());
}

#[test]
fn rays_find_the_corpse_and_characters_walk_through_it() {
    let mut w = ground();
    let (_, _, slept) = drop_figure(&mut w, 0x10, [100.0, 100.0, 6.0], 20.0);
    assert!(slept);
    let pelvis = nodes(&w, 0x10)[0].translation;
    // A ray from above the pelvis hits the corpse on the dead layer; floor rays skip it.
    let from = [pelvis[0], pelvis[1], pelvis[2] + 60.0];
    let hit = w.queries().ray(from, [0.0, 0.0, -1.0], 200.0, &Filter::default()).unwrap();
    assert!(matches!(hit.owner, Owner::Actor { form_id: 0x10, layer: layers::DEAD_BIP, .. }), "{:?}", hit.owner);
    let floor = Filter { exclude_layers: layers::actor_volumes(), ..Default::default() };
    assert!(matches!(w.queries().ray(from, [0.0, 0.0, -1.0], 200.0, &floor).unwrap().owner, Owner::Terrain { .. }));
    // A character walking across the corpse neither stops nor moves it.
    let before = nodes(&w, 0x10);
    let mut c = w.add_character(1, CharacterParams::default(), [pelvis[0] - 120.0, pelvis[1], 3.5]);
    for _ in 0..180 {
        w.move_character(&mut c, CharacterInput { velocity: [100.0, 0.0], ..Default::default() });
        w.step();
    }
    assert!(c.feet[0] > pelvis[0] + 100.0, "walked through: {:?}", c.feet);
    let after = nodes(&w, 0x10);
    for (a, b) in before.iter().zip(&after) {
        let d: f32 = (0..3).map(|i| (a.translation[i] - b.translation[i]).powi(2)).sum::<f32>().sqrt();
        assert!(d < 0.5, "the corpse stayed: moved {d}");
    }
}

#[test]
fn a_push_wakes_and_moves_a_resting_corpse() {
    let mut w = ground();
    let (_, _, slept) = drop_figure(&mut w, 0x10, [100.0, 100.0, 6.0], 20.0);
    assert!(slept);
    let before = nodes(&w, 0x10)[0].translation;
    assert_eq!(w.ragdoll_speed(0x10), Some((0.0, 0.0)));
    assert!(w.push_ragdoll(0x10, before, [400.0, 0.0, 150.0]));
    assert!(!w.push_ragdoll(0x11, before, [400.0, 0.0, 150.0]));
    assert!(w.ragdoll_speed(0x10).unwrap().0 > 100.0);
    for _ in 0..30 {
        w.step();
    }
    assert!(!w.ragdoll_asleep(0x10));
    assert!(nodes(&w, 0x10)[0].translation[0] > before[0] + 5.0);
    assert!(w.ragdoll_center(0x10).is_some() && w.ragdoll_center(0x11).is_none());
}

#[test]
fn a_corpse_put_back_asleep_stays_as_it_lay() {
    let mut w = ground();
    let (rag, _, slept) = drop_figure(&mut w, 0x10, [100.0, 100.0, 6.0], 20.0);
    assert!(slept);
    let lay = nodes(&w, 0x10);
    // Taken out and put back asleep (its ground unloaded, or its cell drawn again), in
    // another world too.
    for mut w in [w, ground()] {
        w.remove_ragdoll(0x10);
        w.add_ragdoll(&RagdollStart { asleep: true, ..start(0x10, &rag, &lay) });
        // A query before the next step (the game casts rays every frame) does not wake it.
        let from = [lay[0].translation[0], lay[0].translation[1], lay[0].translation[2] + 60.0];
        assert!(matches!(w.queries().ray(from, [0.0, 0.0, -1.0], 200.0, &Filter::default()).unwrap().owner, Owner::Actor { form_id: 0x10, .. }));
        for _ in 0..120 {
            w.step();
        }
        assert!(w.ragdoll_asleep(0x10));
        for (a, b) in lay.iter().zip(&nodes(&w, 0x10)) {
            assert!((0..3).all(|i| (a.translation[i] - b.translation[i]).abs() < 1e-2), "{:?} {:?}", a.translation, b.translation);
        }
        // A push still wakes it.
        w.push_ragdoll(0x10, lay[0].translation, [300.0, 0.0, 100.0]);
        for _ in 0..10 {
            w.step();
        }
        assert!(!w.ragdoll_asleep(0x10));
        // Set at rest by hand it holds still at once, until a blast throws it.
        assert!(w.set_ragdoll_at_rest(0x10) && w.ragdoll_asleep(0x10) && !w.set_ragdoll_at_rest(0x11));
        let held = nodes(&w, 0x10);
        for _ in 0..60 {
            w.step();
        }
        assert!(w.ragdoll_asleep(0x10) && w.ragdoll_speed(0x10) == Some((0.0, 0.0)));
        assert_eq!(held[0].translation, nodes(&w, 0x10)[0].translation);
        let c = w.ragdoll_center(0x10).unwrap();
        assert!(w.push_dynamic([c[0] + 30.0, c[1], c[2] - 10.0], 300.0, 600.0) >= 1);
        for _ in 0..10 {
            w.step();
        }
        assert!(!w.ragdoll_asleep(0x10) && w.ragdoll_center(0x10).unwrap()[0] < c[0] - 5.0);
    }
}

#[test]
fn bodies_of_one_ragdoll_collide_only_when_apart_and_unjoined() {
    let mut w = ground();
    let far = |x: f32| body("b", Shape::Sphere { radius: 5.0 }, 1.0, [x, 0.0, 50.0]);
    // Three balls in a row: 0–1 joined, 1–2 touching in the bind pose, 0–2 apart.
    let rag = Arc::new(Ragdoll { bodies: vec![far(0.0), far(20.0), far(29.0)], joints: vec![joint(0, 1, [10.0, 0.0, 0.0], [-10.0, 0.0, 0.0], JointKind::Ball)], problems: vec![] });
    let rest: Vec<Iso> = rag.bodies.iter().map(|b| b.rest()).collect();
    assert_eq!(w.add_ragdoll(&start(5, &rag, &rest)).self_pairs, 1);
    assert!(w.has_ragdoll(5) && w.ragdolls().eq([5]));
    assert_eq!(w.stats().bodies_dynamic, 3);
    // Adding it again replaces it; removing it leaves nothing.
    w.add_ragdoll(&start(5, &rag, &rest));
    assert_eq!(w.stats().bodies_dynamic, 3);
    assert!(w.remove_ragdoll(5) && !w.remove_ragdoll(5));
    assert_eq!(w.stats().bodies_dynamic, 0);
    assert!(w.ragdoll_nodes(5).is_none());
}

#[test]
fn bone_poses_come_back_as_given() {
    let mut w = ground();
    // A body whose frame is turned and moved against its bone, on a scaled actor.
    let mut b = body("b", Shape::Sphere { radius: 5.0 }, 1.0, [0.0; 3]);
    b.object.body_transform = Iso { translation: [3.0, 0.0, 1.0], rotation: [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] };
    let rag = Arc::new(Ragdoll { bodies: vec![b], joints: vec![], problems: vec![] });
    let bone = Iso { translation: [40.0, -20.0, 300.0], rotation: [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0] };
    w.add_ragdoll(&RagdollStart { scale: 1.5, asleep: true, ..start(9, &rag, &[bone]) });
    let n = nodes(&w, 9);
    for i in 0..3 {
        assert!((n[0].translation[i] - bone.translation[i]).abs() < 1e-2, "{:?}", n[0].translation);
    }
    for i in 0..9 {
        assert!((n[0].rotation[i] - bone.rotation[i]).abs() < 1e-4, "{:?}", n[0].rotation);
    }
    // Started asleep, it stays where it was put.
    assert!(w.ragdoll_asleep(9));
    for _ in 0..30 {
        w.step();
    }
    assert!((nodes(&w, 9)[0].translation[2] - 300.0).abs() < 1e-2);
}

#[test]
fn a_roused_corpse_moves_sanely() {
    let mut w = ground();
    let (rag, _, slept) = drop_figure(&mut w, 0x10, [100.0, 100.0, 6.0], 20.0);
    assert!(slept);
    let lay = nodes(&w, 0x10);
    w.remove_ragdoll(0x10);
    w.add_ragdoll(&RagdollStart { asleep: true, ..start(0x10, &rag, &lay) });
    for _ in 0..30 {
        w.step();
    }
    let mut fastest: f32 = 0.0;
    for shot in 0..3 {
        // Queries between the changes and the next step, as the game makes them.
        let above = [lay[1].translation[0], lay[1].translation[1], 80.0];
        assert!(w.queries().ray(above, [0.0, 0.0, -1.0], 200.0, &Filter::default()).is_some());
        assert!(w.push_ragdoll(0x10, lay[1].translation, [0.0, 140.0, -20.0]));
        assert!(w.queries().ray(above, [0.0, 0.0, -1.0], 200.0, &Filter::default()).is_some());
        for i in 0..90 {
            w.step();
            let v = w.ragdoll_speed(0x10).unwrap();
            fastest = fastest.max(v.0);
            assert!(v.0 < 400.0 && v.1 < 30.0, "shot {shot} step {i}: speed {v:?}");
            for (k, p) in nodes(&w, 0x10).iter().enumerate() {
                assert!(p.translation[2] > -1.0 && p.translation[2] < 60.0, "shot {shot} step {i}: body {k} at {:?}", p.translation);
            }
        }
        w.set_ragdoll_at_rest(0x10);
    }
    // The shove is felt, through the joints and against the ground.
    assert!(fastest > 20.0, "fastest {fastest}");
}

/// Dismemberment (docs/spec-notes/dismemberment.md §5): cutting the joint between the left
/// thigh and the pelvis frees the leg, which falls apart from the body; the cut survives
/// the corpse being held at rest and roused again.
#[test]
fn a_cut_joint_lets_the_limb_fall_away() {
    let mut w = ground();
    let rag = figure();
    let poses: Vec<Iso> = rag.bodies.iter().map(|b| at([100.0, 100.0, 6.0]).compose(&b.rest())).collect();
    w.add_ragdoll(&start(0x10, &rag, &poses));
    // The left leg is bodies 3 and 4: one joint joins them to the rest.
    assert_eq!(w.cut_ragdoll(0x10, &[3, 4]), 1);
    assert_eq!(w.cut_ragdoll(0x10, &[3, 4]), 0);
    assert_eq!(w.ragdoll_cut(0x10), vec![2]);
    // Throw the leg sideways: free of the pelvis it travels far from it.
    w.push_ragdoll(0x10, nodes(&w, 0x10)[4].translation, [-400.0, 0.0, 200.0]);
    for _ in 0..120 {
        w.step();
    }
    let n = nodes(&w, 0x10);
    // A joint that holds keeps its pivots within 1.5 units (a_figure_falls_…).
    let hip = rag.joints[2].pivot_gap(&n[3], &n[0]);
    assert!(hip > 10.0, "the thigh left the pelvis: {hip}");
    // The knee still holds the calf to the thigh.
    assert!(rag.joints[3].pivot_gap(&n[4], &n[3]) < 1.5);
    assert!(w.set_ragdoll_at_rest(0x10));
    w.push_ragdoll(0x10, n[0].translation, [10.0, 0.0, 0.0]);
    assert_eq!(w.ragdoll_cut(0x10), vec![2]);
}

/// Gore pieces (docs/spec-notes/dismemberment.md §6) fly, land on the ground and settle;
/// rays find the corpse they belong to; characters walk through them.
#[test]
fn gore_pieces_fly_land_and_belong_to_the_corpse() {
    use fnv_phys::gore::{GoreStart, GORE_PART};
    let mut w = ground();
    for i in 0..4u32 {
        w.add_gore(&GoreStart {
            id: i,
            actor: 0x10,
            translation: [100.0 + i as f32 * 3.0, 100.0, 60.0],
            rotation: ID,
            half_extents: [2.0, 1.5, 1.0],
            center: [0.0; 3],
            linvel: [150.0, 50.0 * i as f32, 200.0],
            angvel: [3.0, 1.0, 0.0],
            mass: 0.3,
        });
    }
    assert_eq!(w.gore_pieces().count(), 4);
    for _ in 0..(8.0 * 60.0) as usize {
        w.step();
    }
    for i in 0..4 {
        let s = w.gore_state(i).unwrap();
        assert!(s.translation[2] > -0.5 && s.translation[2] < 6.0, "piece {i} on the ground: {:?}", s.translation);
        assert!(s.translation[0] > 130.0, "piece {i} flew: {:?}", s.translation);
        let speed: f32 = s.linvel.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(speed < 5.0, "piece {i} settled: {speed}");
    }
    let p = w.gore_state(0).unwrap().translation;
    w.update_queries();
    let hit = w.queries().ray([p[0], p[1], p[2] + 30.0], [0.0, 0.0, -1.0], 60.0, &Filter::default()).unwrap();
    assert_eq!(hit.owner, Owner::Actor { form_id: 0x10, layer: layers::DEAD_BIP, part: GORE_PART });
    let mut c = w.add_character(1, CharacterParams::default(), [p[0] - 100.0, p[1], 3.5]);
    for _ in 0..120 {
        w.move_character(&mut c, CharacterInput { velocity: [100.0, 0.0], ..Default::default() });
        w.step();
    }
    assert!(c.feet[0] > p[0] + 50.0, "walked through: {:?}", c.feet);
    assert!(w.remove_gore(0) && !w.remove_gore(0));
}

#[test]
fn a_held_corpse_is_dragged_and_lies_again_when_let_go() {
    let mut w = ground();
    let (_, _, slept) = drop_figure(&mut w, 0x10, [100.0, 100.0, 6.0], 20.0);
    assert!(slept);
    assert!(w.set_ragdoll_at_rest(0x10));
    let grip = nodes(&w, 0x10)[0].translation;
    assert!(w.grip_mass(GripOn::Ragdoll(0x10), grip).is_some());
    assert!(w.grab(GripOn::Ragdoll(0x10), grip, HoldParams::default()).is_some());
    // Dragged 80 units along the ground, held 20 up.
    let target = [grip[0] + 80.0, grip[1], grip[2] + 20.0];
    w.set_grip_target(target);
    for _ in 0..120 {
        w.step();
    }
    let p = w.grip_point().unwrap();
    assert!((p[0] - target[0]).abs() < 10.0, "{p:?} {target:?}");
    w.release_grip(None);
    for _ in 0..600 {
        w.step();
    }
    assert!(nodes(&w, 0x10)[0].translation[0] > grip[0] + 50.0);
}

#[test]
fn a_held_corpse_is_not_put_at_rest() {
    let mut w = ground();
    let (_, _, slept) = drop_figure(&mut w, 0x10, [100.0, 100.0, 6.0], 20.0);
    assert!(slept);
    let grip = nodes(&w, 0x10)[0].translation;
    assert!(w.grab(GripOn::Ragdoll(0x10), grip, HoldParams::default()).is_some());
    assert!(w.ragdoll_held(0x10) && !w.ragdoll_held(0x11));
    // The engine's rest guard fires while it is carried: refused, it still follows.
    for _ in 0..30 {
        w.step();
    }
    assert!(!w.set_ragdoll_at_rest(0x10) && !w.ragdoll_asleep(0x10));
    let target = [grip[0] + 80.0, grip[1], grip[2] + 20.0];
    w.set_grip_target(target);
    for _ in 0..120 {
        w.step();
    }
    let p = w.grip_point().unwrap();
    assert!((p[0] - target[0]).abs() < 10.0, "{p:?} {target:?}");
    w.release_grip(None);
    assert!(!w.ragdoll_held(0x10) && w.set_ragdoll_at_rest(0x10));
}

/// The highest and lowest point of the figure's bones.
fn span(w: &PhysWorld, id: u32) -> (f32, f32) {
    nodes(w, id).iter().map(|n| n.translation[2]).fold((f32::INFINITY, f32::NEG_INFINITY), |m, z| (m.0.min(z), m.1.max(z)))
}

#[test]
fn a_corpse_on_a_lake_bed_rises_and_floats_at_the_surface() {
    let mut w = ground();
    w.set_water(WaterArea::Exterior(0, 0), Some(400.0));
    let (_, report, _) = drop_figure(&mut w, 0x10, [100.0, 100.0, 6.0], 0.0);
    assert_eq!(report.bodies, 7);
    assert!(w.ragdoll_in_water(0x10));
    let mut rose_at = None;
    for i in 0..(30.0 * 60.0) as usize {
        w.step();
        if rose_at.is_none() && span(&w, 0x10).1 > 380.0 {
            rose_at = Some(i as f32 / 60.0);
        }
    }
    let rose_at = rose_at.expect("the corpse reaches the surface");
    // Rising some 300 units at the drag-capped pace takes seconds, not a frame.
    assert!(rose_at > 2.0 && rose_at < 15.0, "reached the surface after {rose_at} s");
    // Floating: every bone near the surface, none sunk back or thrown out of the water.
    let (lo, hi) = span(&w, 0x10);
    assert!(lo > 340.0 && hi < 440.0, "bones between {lo} and {hi}");
    let (v, _) = w.ragdoll_speed(0x10).unwrap();
    assert!(v < 15.0, "still at the surface: {v} units/s");
    let c = w.ragdoll_center(0x10).unwrap();
    assert!((c[2] - 400.0).abs() < 30.0, "the trunk floats at the surface: {c:?}");
    // Straight up: water lifts, it does not push sideways.
    assert!((c[0] - 100.0).abs() < 60.0 && (c[1] - 100.0).abs() < 60.0, "drifted to {c:?}");
}

#[test]
fn a_corpse_dropped_into_water_sinks_no_further_than_the_bed_and_comes_back_up() {
    let mut w = ground();
    w.set_water(WaterArea::Exterior(0, 0), Some(400.0));
    // From 200 units above the surface: in at speed, slowed, back up.
    let (_, _, _) = drop_figure(&mut w, 0x10, [100.0, 100.0, 600.0], 0.0);
    let mut deepest = f32::INFINITY;
    for _ in 0..(30.0 * 60.0) as usize {
        w.step();
        deepest = deepest.min(span(&w, 0x10).0);
    }
    assert!(deepest < 380.0 && deepest > 0.0, "went under to {deepest}");
    let c = w.ragdoll_center(0x10).unwrap();
    assert!((c[2] - 400.0).abs() < 30.0, "floats again: {c:?}");
}

#[test]
fn out_of_water_nothing_floats() {
    let mut w = ground();
    // Water in the next square over only.
    w.set_water(WaterArea::Exterior(-1, -1), Some(400.0));
    let (_, _, slept) = drop_figure(&mut w, 0x10, [100.0, 100.0, 6.0], 20.0);
    assert!(slept && !w.ragdoll_in_water(0x10));
    assert!(span(&w, 0x10).1 < 45.0, "lies on the ground");
}

#[test]
fn gore_pieces_float() {
    use fnv_phys::gore::GoreStart;
    let mut w = ground();
    w.set_water(WaterArea::Exterior(0, 0), Some(400.0));
    w.add_gore(&GoreStart { id: 1, actor: 0x10, translation: [100.0, 100.0, 50.0], rotation: ID, half_extents: [2.0, 1.5, 1.0], center: [0.0; 3], linvel: [0.0; 3], angvel: [0.0; 3], mass: 0.3 });
    for _ in 0..(20.0 * 60.0) as usize {
        w.step();
    }
    let z = w.gore_state(1).unwrap().translation[2];
    assert!((z - 400.0).abs() < 5.0, "the piece floats: z {z}");
}

/// A placed object's jointed bodies (a swinging trap, `add_object_ragdoll`): its
/// keyframed anchor stays fixed in the air while its dynamic arm, held out level, swings
/// down on the joint.
#[test]
fn an_object_ragdoll_swings_from_its_fixed_anchor() {
    let mut w = ground();
    let mut arm = body("arm", Shape::Capsule { a: [0.0, 4.0, 0.0], b: [0.0, 60.0, 0.0], radius: 3.0 }, 10.0, [0.0, 0.0, 300.0]);
    arm.object.motion = Motion::Dynamic;
    let anchor = body("anchor", Shape::Box { half_extents: [5.0, 5.0, 5.0] }, 0.0, [0.0, 0.0, 300.0]);
    let rag = Arc::new(Ragdoll { bodies: vec![arm, anchor], joints: vec![joint(0, 1, [0.0; 3], [0.0; 3], JointKind::Ball)], problems: vec![] });
    let rest: Vec<Iso> = rag.bodies.iter().map(|b| b.rest()).collect();
    let r = w.add_object_ragdoll(&start(0x0016_5273, &rag, &rest));
    assert_eq!((r.bodies, r.joints), (2, 1));
    assert!(w.is_object_ragdoll(0x0016_5273));
    let mut lowest = f32::INFINITY;
    for _ in 0..90 {
        w.step();
        let n = nodes(&w, 0x0016_5273);
        // The arm's far end swings below the anchor.
        lowest = lowest.min(n[0].apply([0.0, 60.0, 0.0])[2]);
        assert!((n[1].translation[2] - 300.0).abs() < 1e-3, "the anchor stays: {:?}", n[1].translation);
    }
    assert!(lowest < 260.0, "the arm swung down: lowest end at z {lowest}");
}

/// A placed object's joint that starts outside its limits (the swinging log's rear chain
/// on its ram) is built without them, and its parts do not collide with each other; the
/// same joint on an actor's ragdoll keeps its limits.
#[test]
fn an_object_joint_starting_outside_its_limits_is_built_free() {
    let mut w = ground();
    let mut arm = body("arm", Shape::Capsule { a: [0.0, 4.0, 0.0], b: [0.0, 60.0, 0.0], radius: 3.0 }, 10.0, [0.0, 0.0, 300.0]);
    arm.object.motion = Motion::Dynamic;
    // The arm's node turned 90 degrees about x: the hinge's angle starts at 90 degrees,
    // past its limits of ±0.1.
    arm.object.node_transform.rotation = [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0];
    let anchor = body("anchor", Shape::Box { half_extents: [5.0, 5.0, 5.0] }, 0.0, [0.0, 0.0, 300.0]);
    let hinge = JointKind::Hinge { axis_a: X, perp_a: Y, axis_b: X, perp_b: Y, limits: Some([-0.1, 0.1]) };
    let rag = Arc::new(Ragdoll { bodies: vec![arm, anchor], joints: vec![joint(0, 1, [0.0; 3], [0.0; 3], hinge)], problems: vec![] });
    let rest: Vec<Iso> = rag.bodies.iter().map(|b| b.rest()).collect();
    assert!(!rag.joints[0].within(&rag.joints[0].angles(&rest[0], &rest[1]), 0.05), "the test pose starts outside");
    let r = w.add_object_ragdoll(&start(0x0016_5273, &rag, &rest));
    assert_eq!((r.joints, r.freed.clone(), r.self_pairs), (1, vec![0], 0));
    let mut actor = ground();
    let r = actor.add_ragdoll(&start(7, &rag, &rest));
    assert!(r.freed.is_empty(), "an actor's joint keeps its limits");
}

/// A heap of corpses dropped in one spot (#910): 25 chains of three balls. Small ones
/// (28 units long) pass through each other and still land on the ground; the same chains
/// at three times the size (84 units, bigger than `SMALL_RAGDOLL_SPAN`) pile up. Prints the
/// heap's contact pairs and step time (`--nocapture`).
#[test]
fn small_corpses_pass_through_each_other_big_ones_pile_up() {
    let ball = |x: f32| body("b", Shape::Sphere { radius: 4.0 }, 2.0, [x, 0.0, 0.0]);
    let joints = vec![joint(0, 1, [-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], JointKind::Ball), joint(1, 2, [-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], JointKind::Ball)];
    let rag = Arc::new(Ragdoll { bodies: vec![ball(0.0), ball(10.0), ball(20.0)], joints, problems: vec![] });
    let heap = |scale: f32| {
        let mut w = ground();
        for i in 0..25u32 {
            // Stacked over one spot, each a little turned and shifted.
            let (s, c) = (i as f32 * 0.7).sin_cos();
            let lift = [(i % 3) as f32 * 2.0 * scale, (i % 2) as f32 * 2.0 * scale, (12.0 + i as f32 * 9.0) * scale];
            let nodes: Vec<Iso> = rag
                .bodies
                .iter()
                .map(|b| {
                    let p = b.rest().translation.map(|v| v * scale);
                    Iso { translation: [c * p[0] - s * p[1] + lift[0], s * p[0] + c * p[1] + lift[1], p[2] + lift[2]], rotation: [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0] }
                })
                .collect();
            w.add_ragdoll(&RagdollStart { scale, ..start(0x100 + i, &rag, &nodes) });
            assert_eq!(w.ragdoll_is_small(0x100 + i), scale < 2.0);
        }
        let (mut peak, mut sum, t) = (0, 0, std::time::Instant::now());
        for _ in 0..180 {
            w.step();
            let n = w.ragdoll_contacts_between();
            peak = peak.max(n);
            sum += n;
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0 / 180.0;
        eprintln!("25 chains at scale {scale}: contact pairs between corpses peak {peak}, mean {:.1}; {ms:.3} ms per step", sum as f64 / 180.0);
        let lowest = (0..25).flat_map(|i| nodes(&w, 0x100 + i)).map(|n| n.translation[2]).fold(f32::INFINITY, f32::min);
        (peak, lowest)
    };
    let (small_peak, small_low) = heap(1.0);
    assert_eq!(small_peak, 0, "small corpses never touch each other");
    assert!(small_low > 0.0, "and still lie on the ground: lowest bone at z {small_low}");
    let (big_peak, big_low) = heap(3.0);
    assert!(big_peak > 10, "big corpses pile up: {big_peak} pairs");
    assert!(big_low > 0.0, "lowest bone at z {big_low}");
}
