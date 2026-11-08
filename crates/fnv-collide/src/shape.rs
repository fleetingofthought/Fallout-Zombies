//! Engine-neutral collision shapes: the API between collision geometry (this crate,
//! Layer 4 workstream 1) and physics (`fnv-phys`, workstream 2), docs/design/layer4.md §7.
//!
//! Conventions, shared with `fnv_nif::scene` (docs/spec-notes/nif.md §6): lengths in game
//! units **after** the Havok-to-game scale has been applied (docs/spec-notes/collision.md);
//! Z up; rotations are row-major 3×3 matrices rotating column vectors; a child transform
//! composes as parent ∘ child. Physics never sees NIF blocks, only these types.

use serde::Serialize;
use std::sync::Arc;

/// Rigid placement: `p' = rotation · p + translation` (no scale; scale is baked into the
/// shape's dimensions).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Iso {
    pub translation: [f32; 3],
    /// Row-major 3×3.
    pub rotation: [f32; 9],
}

impl Iso {
    pub const IDENTITY: Iso =
        Iso { translation: [0.0; 3], rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] };

    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let r = &self.rotation;
        let t = &self.translation;
        [
            r[0] * p[0] + r[1] * p[1] + r[2] * p[2] + t[0],
            r[3] * p[0] + r[4] * p[1] + r[5] * p[2] + t[1],
            r[6] * p[0] + r[7] * p[1] + r[8] * p[2] + t[2],
        ]
    }

    /// `self ∘ other` (apply `other` first).
    pub fn compose(&self, other: &Iso) -> Iso {
        let a = &self.rotation;
        let b = &other.rotation;
        let mut r = [0.0f32; 9];
        for i in 0..3 {
            for j in 0..3 {
                r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
            }
        }
        Iso { translation: self.apply(other.translation), rotation: r }
    }

    /// The placement that undoes this one (the rotation is orthonormal, so its inverse is
    /// its transpose).
    pub fn inverse(&self) -> Iso {
        let r = &self.rotation;
        let rt = [r[0], r[3], r[6], r[1], r[4], r[7], r[2], r[5], r[8]];
        let t = self.translation;
        let inv = Iso { translation: [0.0; 3], rotation: rt };
        Iso { translation: inv.apply([-t[0], -t[1], -t[2]]), rotation: rt }
    }

    /// The rotation alone applied to a direction.
    pub fn rotate(&self, v: [f32; 3]) -> [f32; 3] {
        let r = &self.rotation;
        [r[0] * v[0] + r[1] * v[1] + r[2] * v[2], r[3] * v[0] + r[4] * v[1] + r[5] * v[2], r[6] * v[0] + r[7] * v[1] + r[8] * v[2]]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    /// Centred on the origin.
    Box { half_extents: [f32; 3] },
    Sphere { radius: f32 },
    /// The segment `a`–`b` swept by `radius`.
    Capsule { a: [f32; 3], b: [f32; 3], radius: f32 },
    /// Convex hull of the points.
    ConvexHull { points: Vec<[f32; 3]> },
    /// Triangles with one Havok material code each (meaning: collision.md).
    TriMesh { vertices: Vec<[f32; 3]>, triangles: Vec<[u32; 3]>, materials: Vec<u32> },
    Compound { children: Vec<(Iso, Shape)> },
}

impl Shape {
    /// Every length multiplied by `f` (stored units → game units).
    pub fn scaled(&self, f: f32) -> Shape {
        let s3 = |p: &[f32; 3]| p.map(|v| v * f);
        match self {
            Shape::Box { half_extents } => Shape::Box { half_extents: s3(half_extents) },
            Shape::Sphere { radius } => Shape::Sphere { radius: radius * f },
            Shape::Capsule { a, b, radius } => Shape::Capsule { a: s3(a), b: s3(b), radius: radius * f },
            Shape::ConvexHull { points } => Shape::ConvexHull { points: points.iter().map(s3).collect() },
            Shape::TriMesh { vertices, triangles, materials } => Shape::TriMesh {
                vertices: vertices.iter().map(s3).collect(),
                triangles: triangles.clone(),
                materials: materials.clone(),
            },
            Shape::Compound { children } => Shape::Compound {
                children: children
                    .iter()
                    .map(|(iso, s)| (Iso { translation: s3(&iso.translation), rotation: iso.rotation }, s.scaled(f)))
                    .collect(),
            },
        }
    }

    /// Call `f` with points whose bounding box equals the shape's bounds under `iso`
    /// (box corners, sphere/capsule extremes, hull and mesh vertices).
    pub fn bound_points(&self, iso: &Iso, f: &mut impl FnMut([f32; 3])) {
        match self {
            Shape::Box { half_extents: h } => {
                for i in 0..8 {
                    let s = |b: usize, v: f32| if i & (1 << b) != 0 { v } else { -v };
                    f(iso.apply([s(0, h[0]), s(1, h[1]), s(2, h[2])]));
                }
            }
            Shape::Sphere { radius } => sphere_points(iso.translation, *radius, f),
            Shape::Capsule { a, b, radius } => {
                sphere_points(iso.apply(*a), *radius, f);
                sphere_points(iso.apply(*b), *radius, f);
            }
            Shape::ConvexHull { points } => points.iter().for_each(|p| f(iso.apply(*p))),
            Shape::TriMesh { vertices, .. } => vertices.iter().for_each(|p| f(iso.apply(*p))),
            Shape::Compound { children } => {
                for (c, s) in children {
                    s.bound_points(&iso.compose(c), f);
                }
            }
        }
    }
}

fn sphere_points(c: [f32; 3], r: f32, f: &mut impl FnMut([f32; 3])) {
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let mut p = c;
            p[axis] += sign * r;
            f(p);
        }
    }
}

/// How a body moves. [hypothesis until mapped from the Havok motion system /
/// quality fields on the data, collision.md]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    Static,
    Keyframed,
    Dynamic,
}

/// One collision body of a mesh.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CollisionObject {
    /// Name of the NIF node the collision object is attached to (`None` if unnamed).
    pub node: Option<String>,
    /// Block index of that node in the file.
    pub node_block: usize,
    /// Node's transform relative to the mesh root (from the scene graph).
    pub node_transform: Iso,
    /// Body transform relative to the node (bhkRigidBodyT carries one).
    pub body_transform: Iso,
    pub motion: Motion,
    /// Havok collision layer (meaning: collision.md).
    pub layer: u8,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    /// Shared: the placed copies of a model hold the one shape its file was read into,
    /// and physics converts it once per scale (`fnv_phys::convert`, issue #1085).
    pub shape: Arc<Shape>,
    /// Contact shell, game units: the largest `radius` stored on the body's box,
    /// convex-vertices or triangle-mesh shapes (spheres and capsules carry theirs in their
    /// geometry), 0 when none. Havok keeps contacts this far outside the surface, so an
    /// actor rests about that much higher than on the bare geometry [hypothesis for the
    /// mechanism; the in-game rest heights it explains: physics.md §6.2].
    pub shell: f32,
    /// Havok material code of the body's convex shapes (box, sphere, capsule,
    /// convex vertices): the first one's when they differ [design], `None` when it has
    /// none. Triangle meshes keep one per triangle in the shape instead. What is hit, for
    /// impact effects and sounds (docs/spec-notes/audio.md §11).
    pub material: Option<u32>,
    /// The body responds to the weather's wind: its Body Flags bit 0 [documented: nif.xml
    /// bhkRigidBody, "1 = respond to wind"]; set on the tumbleweed model alone among the
    /// masters' placed models [observed: nif-engine-gap-audit.md, body_flags].
    pub wind: bool,
}
