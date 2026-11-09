//! A coarse grid over fixed places that are heard within a reach of their own, so the
//! per-frame question "which of them can the listener hear" visits only the places that
//! can answer yes: the objects' own sounds (`crate::world_sounds`), the sound markers
//! (`crate::ambient`) and the audio markers (`crate::music`).
//!
//! [design] The grid only narrows the candidates. Every caller still runs its own exact
//! test on each candidate, so the answer is the one a walk over the whole list gives,
//! provided the caller's `far` test (see [`FixedGrid::near`]) can only be true for a
//! bucket none of whose entries the exact test would keep. The candidates come out in
//! list order (ascending index), so a caller that draws random numbers or breaks ties in
//! list order sees the same sequence as the walk over the whole list.
//!
//! Entries are bucketed by exterior cell (`fnv_world::cell_of`) and by the power of two
//! of their reach, so one far-reaching sound does not drag its cell's short-range ones
//! into every query. A bucket keeps the box its entries' places span and the largest
//! reach among them. A query tests the listener against every bucket's box (one per
//! occupied cell and reach class, in one array) and hands out the entries of the buckets
//! in reach. Entries whose place or reach is not a finite number
//! are kept out of the buckets and handed out by every query.

use std::collections::BTreeMap;

/// One bucket: the box its entries' places span, the largest reach among them, and its
/// entries' range in [`FixedGrid::items`].
#[derive(Debug, Clone)]
struct Bucket {
    min: [f32; 3],
    max: [f32; 3],
    reach: f32,
    start: u32,
    end: u32,
}

/// Fixed places with a reach each, by bucket (see the module notes).
#[derive(Debug, Clone, Default)]
pub struct FixedGrid {
    buckets: Vec<Bucket>,
    /// Entry indices, bucket after bucket.
    items: Vec<u32>,
    /// Entries whose place or reach is not finite: handed out by every query.
    always: Vec<u32>,
}

/// The bucket class of a reach: its binary exponent (the reach is finite and, as the
/// callers pass it, not negative).
fn reach_class(reach: f32) -> u32 {
    reach.max(0.0).to_bits() >> 23
}

/// One entry as [`FixedGrid::build`] takes it: (index, place, reach).
type Entry = (u32, [f32; 3], f32);
/// A bucket's key: its exterior cell and its reach class.
type BucketKey = ((i32, i32), u32);

impl FixedGrid {
    /// A grid over `entries`: (index, place, reach). Each index appears once. The reach
    /// is the one the caller's `far` test reads (see [`FixedGrid::near`]): the distance
    /// within which the entry can be kept.
    pub fn build(entries: impl IntoIterator<Item = Entry>) -> FixedGrid {
        let mut by: BTreeMap<BucketKey, Vec<Entry>> = BTreeMap::new();
        let mut always = Vec::new();
        for (i, at, reach) in entries {
            if !(at.iter().all(|v| v.is_finite()) && reach.is_finite()) {
                always.push(i);
                continue;
            }
            by.entry((fnv_world::cell_of(at[0], at[1]), reach_class(reach))).or_default().push((i, at, reach));
        }
        always.sort_unstable();
        let mut grid = FixedGrid { buckets: Vec::with_capacity(by.len()), items: Vec::new(), always };
        for (_, mut entries) in by {
            entries.sort_unstable_by_key(|e| e.0);
            let start = grid.items.len() as u32;
            let (mut min, mut max, mut reach) = (entries[0].1, entries[0].1, entries[0].2);
            for &(i, at, r) in &entries {
                min = std::array::from_fn(|k| min[k].min(at[k]));
                max = std::array::from_fn(|k| max[k].max(at[k]));
                reach = reach.max(r);
                grid.items.push(i);
            }
            grid.buckets.push(Bucket { min, max, reach, start, end: grid.items.len() as u32 });
        }
        grid
    }

    /// How many entries the grid holds.
    pub fn len(&self) -> usize {
        self.items.len() + self.always.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The indices of the entries that can be heard at `eye`, in ascending order, into
    /// `out` (cleared first): those of every bucket for which `far(point, reach)` is false,
    /// where `point` is the bucket box's point nearest `eye` (`eye` clamped to the box,
    /// axis by axis) and `reach` the bucket's largest, and the entries kept out of the
    /// buckets.
    ///
    /// For the answer to be exact, `far` must be the caller's own rejection test with the
    /// entry's place and reach replaced: if it rejects a point and a reach, it rejects
    /// every point at least as far from `eye` on each axis with any reach not larger.
    /// The distance and squared-distance tests the callers use are of that kind (their
    /// subtractions, squares, sums and square roots are each monotone in floating point),
    /// and they answer false for a NaN distance as the entry tests do, so a listener at a
    /// NaN place gets every entry, as the walk over the whole list would test them all.
    pub fn near(&self, eye: [f32; 3], far: impl Fn([f32; 3], f32) -> bool, out: &mut Vec<u32>) {
        out.clear();
        for b in &self.buckets {
            let point = [eye[0].clamp(b.min[0], b.max[0]), eye[1].clamp(b.min[1], b.max[1]), eye[2].clamp(b.min[2], b.max[2])];
            if far(point, b.reach) {
                continue;
            }
            out.extend_from_slice(&self.items[b.start as usize..b.end as usize]);
        }
        out.extend_from_slice(&self.always);
        out.sort_unstable();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A small deterministic generator for the tests' synthetic places.
    pub(crate) struct Lcg(pub u64);

    impl Lcg {
        pub(crate) fn next_u32(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }

        /// A number in `lo..hi`.
        pub(crate) fn range(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (hi - lo) * (self.next_u32() as f32 / (1u64 << 31) as f32)
        }
    }

    fn d2(a: [f32; 3], b: [f32; 3]) -> f32 {
        (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
    }

    /// Places across cell borders (exactly on them too), reaches of several powers of two,
    /// a few that are not finite; listeners on the borders, on a reach's edge, far away
    /// and at NaN: the grid's candidates with the exact test are the whole list's.
    #[test]
    fn candidates_with_the_exact_test_are_the_whole_lists_answer() {
        let mut rng = Lcg(7);
        let mut entries: Vec<([f32; 3], f32)> = Vec::new();
        for i in 0..2000 {
            let mut at = [rng.range(-20000.0, 20000.0), rng.range(-20000.0, 20000.0), rng.range(-2000.0, 4000.0)];
            if i % 7 == 0 {
                // On a cell border.
                at[0] = 4096.0 * (rng.next_u32() % 9) as f32 - 16384.0;
            }
            if i % 11 == 0 {
                at[1] = -4096.0 * (rng.next_u32() % 5) as f32;
            }
            let reach = match i % 5 {
                0 => rng.range(100.0, 300.0),
                1 => rng.range(1000.0, 3000.0),
                2 => 4096.0,
                3 => rng.range(5000.0, 12000.0),
                _ => rng.range(0.0, 50.0),
            };
            entries.push((at, reach));
        }
        entries.push(([f32::NAN, 0.0, 0.0], 100.0));
        entries.push(([10.0, 0.0, 0.0], f32::INFINITY));
        entries.push(([10.0, 0.0, 0.0], f32::NAN));
        let grid = FixedGrid::build(entries.iter().enumerate().map(|(i, &(at, r))| (i as u32, at, r)));
        assert_eq!(grid.len(), entries.len());
        let mut eyes: Vec<[f32; 3]> = (0..300).map(|_| [rng.range(-25000.0, 25000.0), rng.range(-25000.0, 25000.0), rng.range(-3000.0, 5000.0)]).collect();
        eyes.extend([[0.0; 3], [4096.0, -4096.0, 0.0], [8192.0, 0.0, 100.0], [f32::NAN, 0.0, 0.0], [1e9, 1e9, 0.0]]);
        // On the edge of an entry's reach: exactly its reach away along x.
        for &(at, r) in entries.iter().take(40) {
            eyes.push([at[0] + r, at[1], at[2]]);
            eyes.push([at[0] - r, at[1], at[2]]);
        }
        let mut out = Vec::new();
        for eye in eyes {
            let keep = |i: usize| {
                let (at, r) = entries[i];
                let far = d2(at, eye) > r * r;
                !far
            };
            let whole: Vec<u32> = (0..entries.len()).filter(|&i| keep(i)).map(|i| i as u32).collect();
            grid.near(eye, |c, r| d2(c, eye) > r * r, &mut out);
            assert!(out.windows(2).all(|w| w[0] < w[1]), "candidates in list order");
            let indexed: Vec<u32> = out.iter().copied().filter(|&i| keep(i as usize)).collect();
            assert_eq!(indexed, whole, "listener at {eye:?}");
            if eye[0].is_finite() {
                assert!(out.len() < entries.len(), "the grid narrows the candidates at {eye:?}");
            }
        }
    }
}
