//! Placed collision primitives: references whose shape is a box or sphere stored on the
//! reference itself (REFR XPRM) rather than in a model, with a collision layer (XTRI).
//! Layouts and layer names [documented: xEdit wbDefinitionsFNV.pas REFR, via
//! `schemas/fnv/world.json`]; physics.md §4.1 has the counts and open questions.

use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_esm::{LoadOrder, Schema};
use fnv_world::fields::{vec3, winner, Fields};
use serde_json::Value;

/// The engine-reserved base object of collision primitives [observed: STAT 0x00000021,
/// Editor ID "CollisionMarker", carries 4,205 of the 8,819 placed primitives of the ten
/// masters; physics.md §4.1].
pub const COLLISION_MARKER: u32 = 0x21;

/// XPRM primitive type [documented: xEdit].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrimitiveKind {
    None,
    Box,
    Sphere,
    PortalBox,
    Other(u32),
}

impl PrimitiveKind {
    fn from_value(v: &Value) -> PrimitiveKind {
        let n = match v {
            Value::String(s) => match s.as_str() {
                "None" => 0,
                "Box" => 1,
                "Sphere" => 2,
                "Portal Box" => 3,
                _ => u32::MAX,
            },
            v => v.as_u64().map_or(u32::MAX, |n| n as u32),
        };
        match n {
            0 => PrimitiveKind::None,
            1 => PrimitiveKind::Box,
            2 => PrimitiveKind::Sphere,
            3 => PrimitiveKind::PortalBox,
            n => PrimitiveKind::Other(n),
        }
    }

    pub fn name(&self) -> String {
        match self {
            PrimitiveKind::None => "none".into(),
            PrimitiveKind::Box => "box".into(),
            PrimitiveKind::Sphere => "sphere".into(),
            PrimitiveKind::PortalBox => "portal_box".into(),
            PrimitiveKind::Other(n) => format!("other_{n}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Primitive {
    /// XPRM bounds as stored.
    pub bounds: [f32; 3],
    pub kind: PrimitiveKind,
    /// XTRI collision layer, when present.
    pub layer: Option<u32>,
}

/// The primitive of a placed reference, if it has one.
pub fn primitive(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Primitive> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let x = f.get(b"XPRM")?;
    let bounds = x.get("bounds").and_then(vec3)?;
    let kind = x.get("type").map_or(PrimitiveKind::Other(u32::MAX), PrimitiveKind::from_value);
    // The schema decodes XTRI to a layer name; the number is what the world keeps.
    let layer = f.raw(b"XTRI").and_then(|s| Some(u32::from_le_bytes(s.data.get(..4)?.try_into().ok()?)));
    Some(Primitive { bounds, kind, layer })
}

/// How stored bounds map to shape dimensions. The default reads them as half extents
/// (box) and radius (sphere, first component): read as full extents, 77% of the exterior
/// collision boxes would float above the terrain, against 24% as half extents [observed:
/// `examples/primitive_bounds.rs`; still a hypothesis until checked in game, physics.md
/// §4.1].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoundsMeaning {
    #[default]
    HalfExtents,
    FullExtents,
}

/// The primitive as a static collision object in the reference's frame (the reference
/// transform, including its scale, is applied when it is added to the world).
pub fn primitive_object(p: &Primitive, meaning: BoundsMeaning) -> Option<CollisionObject> {
    let k = match meaning {
        BoundsMeaning::HalfExtents => 1.0,
        BoundsMeaning::FullExtents => 0.5,
    };
    let b = [p.bounds[0].abs() * k, p.bounds[1].abs() * k, p.bounds[2].abs() * k];
    let shape = match p.kind {
        PrimitiveKind::Box | PrimitiveKind::PortalBox => Shape::Box { half_extents: b },
        PrimitiveKind::Sphere => Shape::Sphere { radius: b[0] },
        _ => return None,
    };
    if !b.iter().all(|v| v.is_finite()) || b.iter().all(|&v| v == 0.0) {
        return None;
    }
    Some(CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion: Motion::Static,
        // No XTRI (all but 3 of the masters' 8,819 primitives): an invisible wall
        // (physics.md §4.1) [hypothesis: what a CollisionMarker is for].
        layer: p.layer.map_or(crate::layers::INVISIBLE_WALL, |l| l.min(255) as u8),
        mass: 0.0,
        friction: 0.5,
        restitution: 0.0,
        shape: shape.into(),
        // No Havok radius is known for placed primitives [hypothesis: 0].
        shell: 0.0,
        material: None,
        wind: false,
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kinds_from_names_and_numbers() {
        assert_eq!(PrimitiveKind::from_value(&json!("Box")), PrimitiveKind::Box);
        assert_eq!(PrimitiveKind::from_value(&json!("Portal Box")), PrimitiveKind::PortalBox);
        assert_eq!(PrimitiveKind::from_value(&json!(2)), PrimitiveKind::Sphere);
        assert_eq!(PrimitiveKind::from_value(&json!(9)), PrimitiveKind::Other(9));
    }

    #[test]
    fn objects_follow_the_bounds_meaning() {
        let p = Primitive { bounds: [10.0, 20.0, 30.0], kind: PrimitiveKind::Box, layer: Some(27) };
        let o = primitive_object(&p, BoundsMeaning::HalfExtents).unwrap();
        assert_eq!(*o.shape, Shape::Box { half_extents: [10.0, 20.0, 30.0] });
        assert_eq!(o.layer, 27);
        let o = primitive_object(&p, BoundsMeaning::FullExtents).unwrap();
        assert_eq!(*o.shape, Shape::Box { half_extents: [5.0, 10.0, 15.0] });
        let s = Primitive { bounds: [8.0, 8.0, 8.0], kind: PrimitiveKind::Sphere, layer: None };
        assert_eq!(primitive_object(&s, BoundsMeaning::HalfExtents).unwrap().layer, crate::layers::INVISIBLE_WALL);
        assert_eq!(*primitive_object(&s, BoundsMeaning::HalfExtents).unwrap().shape, Shape::Sphere { radius: 8.0 });
        let none = Primitive { bounds: [1.0; 3], kind: PrimitiveKind::None, layer: None };
        assert!(primitive_object(&none, BoundsMeaning::HalfExtents).is_none());
    }
}
