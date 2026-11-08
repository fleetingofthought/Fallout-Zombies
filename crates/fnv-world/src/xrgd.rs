//! `XRGD` "Ragdoll Bones" on placed references: the pose a designer left a corpse or a
//! jointed clutter object in (docs/spec-notes/dead-on-spawn.md §5, §5.1). Shared by the
//! drawing and the collision of posed clutter so that the two follow one rule.

use crate::transform::{ref_rotation, Mat3};
use fnv_esm::LoadOrder;

/// One `XRGD` entry: position (game units) and angles (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bone {
    pub position: [f32; 3],
    pub angles: [f32; 3],
}

impl Bone {
    /// The entry's rotation, in the convention of a reference's (row-major, `v' = M v`)
    /// [observed: dead-on-spawn.md §5].
    pub fn rotation(&self) -> Mat3 {
        ref_rotation(self.angles)
    }
}

/// The entries of an `XRGD` subrecord: 28 bytes each, a byte (not used), three bytes of
/// padding, then six little-endian floats [documented: xEdit wbRagdoll].
pub fn parse(data: &[u8]) -> Vec<Bone> {
    let f = |c: &[u8; 28], i: usize| f32::from_le_bytes([c[4 + 4 * i], c[5 + 4 * i], c[6 + 4 * i], c[7 + 4 * i]]);
    let (entries, _) = data.as_chunks::<28>();
    entries.iter().map(|c| Bone { position: [f(c, 0), f(c, 1), f(c, 2)], angles: [f(c, 3), f(c, 4), f(c, 5)] }).collect()
}

/// The base record types whose references are posed from their `XRGD` [observed: the
/// clutter skeletons are MSTT; `fnvtool world posed-clutter` over the ten masters found
/// a whole clothed skeleton placed as a CONT (container), which joins them. ACTI is left
/// out (a swinging trap's model is drawn by `swing_traps`, its own rig, so posing its
/// collision from `XRGD` would part it from the drawing), ARMO too (its fitting `XRGD`
/// references are skinned ground objects, case E), and the single-body items ALCH, WEAP
/// and PROJ (their one-entry `XRGD` is a single-body pose, a different concern from the
/// ragdoll-like clutter pose here)].
pub fn posable(sig: &[u8; 4]) -> bool {
    matches!(sig, b"MSTT" | b"STAT" | b"MISC" | b"CONT")
}

/// How many of `entries` XRGD entries pose `bodies` collision objects, or why they do
/// not (issue #1400). One entry per body when the counts match; a reference whose `XRGD`
/// has more entries than its model has bodies — a skeleton part copied with the whole
/// skeleton's `XRGD` — takes the first `bodies` entries, when the model has at least two
/// bodies [observed: `skeletontorsonaked.nif`'s five bodies, Torso, Head, RightUpArm,
/// RightForeArm, RightHand, are the first five of the whole clothed skeleton's fifteen,
/// so its fifteen-entry `XRGD` poses them by its first five entries].
pub fn matching(entries: usize, bodies: usize) -> Result<usize, &'static str> {
    if bodies == 0 {
        return Err("the model has no bodies");
    }
    if entries == 0 {
        return Err("no XRGD entries");
    }
    if entries == bodies {
        return Ok(entries);
    }
    if entries > bodies && bodies >= 2 {
        return Ok(bodies);
    }
    if entries > bodies {
        return Err("one body, but more XRGD entries than it can hold");
    }
    Err("fewer XRGD entries than bodies")
}

/// The `XRGD` entries of reference `r` (its winning record), if it has any.
pub fn stored(lo: &LoadOrder, r: u32) -> Option<Vec<Bone>> {
    let d = lo.index.get(&r)?.last()?;
    let subs = lo.records[d.plugin as usize][d.ordinal as usize].subrecords().ok()?;
    let x = subs.iter().find(|s| &s.sig == b"XRGD")?;
    let bones = parse(&x.data);
    (!bones.is_empty()).then_some(bones)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_28_bytes_each() {
        let mut data = Vec::new();
        for k in 0..2u8 {
            data.extend([k, 0, 0, 0]);
            for v in [1.0f32, 2.0, 3.0, 0.5, 0.0, -0.25] {
                data.extend((v * f32::from(k + 1)).to_le_bytes());
            }
        }
        data.extend([9, 9]); // a short tail is ignored
        let b = parse(&data);
        assert_eq!(b.len(), 2);
        assert_eq!(b[1], Bone { position: [2.0, 4.0, 6.0], angles: [1.0, 0.0, -0.5] });
    }

    #[test]
    fn posable_types() {
        assert!(posable(b"MSTT") && posable(b"STAT") && posable(b"MISC"));
        assert!(posable(b"CONT"));
        assert!(!posable(b"NPC_") && !posable(b"DOOR") && !posable(b"ACTI") && !posable(b"ARMO") && !posable(b"WEAP") && !posable(b"ALCH") && !posable(b"PROJ"));
    }

    #[test]
    fn matching_counts() {
        assert_eq!(matching(17, 17), Ok(17));
        // A skeleton part copied with the whole skeleton's XRGD: the first entries pose it.
        assert_eq!(matching(15, 5), Ok(5));
        assert_eq!(matching(0, 5), Err("no XRGD entries"));
        assert_eq!(matching(5, 0), Err("the model has no bodies"));
        // One body cannot stand for several entries, and fewer entries cannot pose more bodies.
        assert_eq!(matching(3, 1), Err("one body, but more XRGD entries than it can hold"));
        assert_eq!(matching(3, 5), Err("fewer XRGD entries than bodies"));
    }
}
