//! Quest objective targets: which targets of the objectives shown and not done pass their
//! conditions, and the door outdoors at which a target indoors is marked
//! (docs/spec-notes/quests-dialogue.md §2, "Target conditions"; ESM gap audit row 17).
//!
//! - A target's conditions run with the **target reference as subject** and no target
//!   [documented: GECK wiki Quest Objectives Tab, "the conditions are applied to the quest
//!   target", "Run on Target is invalid"; observed: the masters' subject-run `GetDead` /
//!   `GetDisabled` items on targets name no reference].
//! - The engine evaluates them for its markers as they change; the console's
//!   ShowQuestTargets and MoveToQuestTarget when called.

use crate::game::Game;
use fnv_sim::{FormId, World};

/// One target of an objective shown and not done, and whether its conditions pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetCheck {
    pub quest: FormId,
    pub objective: i32,
    /// The target's place in the objective's list (record order).
    pub index: usize,
    pub target: FormId,
    /// "Compass Marker Ignores Locks".
    pub ignores_locks: bool,
    /// The conditions pass: the target is marked.
    pub shown: bool,
}

impl Game<'_> {
    /// Every target of `quest`'s objectives shown and not done (objectives by index,
    /// targets in record order), each with whether its conditions pass now. A target
    /// with no reference (none set, or one the load order lacks) is left out.
    pub fn check_quest_targets(&mut self, quest: FormId) -> Vec<TargetCheck> {
        let data = self.data;
        let (Some(def), Some(state)) = (data.quests.get(&quest), self.quests.get(&quest)) else { return Vec::new() };
        let open: Vec<i32> = state.objectives.iter().filter(|(_, o)| o.displayed && !o.completed).map(|(i, _)| *i).collect();
        let mut out = Vec::new();
        for objective in open {
            // The scripts' target list when they changed it (`crate::nvse_jip10`): a
            // record's target keeps its conditions, an added one has none [hypothesis].
            if self.objective_targets_changed(quest, objective) {
                let record = def.objectives.get(&objective).map(|d| d.targets.clone()).unwrap_or_default();
                for (index, target) in self.objective_targets(quest, objective).into_iter().enumerate() {
                    let t = record.iter().find(|t| t.target == Some(target));
                    let label = format!("QUST {quest} objective {objective} target {index}");
                    let shown = t.is_none_or(|t| self.conditions_pass(&t.conditions, Some(target), None, &label));
                    out.push(TargetCheck { quest, objective, index, target, ignores_locks: t.is_some_and(|t| t.ignores_locks), shown });
                }
                continue;
            }
            for (index, t) in def.objectives.get(&objective).into_iter().flat_map(|d| d.targets.iter().enumerate()) {
                let Some(target) = t.target else { continue };
                let label = format!("QUST {quest} objective {objective} target {index}");
                let shown = self.conditions_pass(&t.conditions, Some(target), None, &label);
                out.push(TargetCheck { quest, objective, index, target, ignores_locks: t.ignores_locks, shown });
            }
        }
        out
    }

    /// The doors outdoors (in an exterior cell) that lead into the interior `cell`, in
    /// FormID order.
    pub fn doors_into(&self, cell: FormId) -> Vec<FormId> {
        let refs = self.world.placed;
        refs.refs
            .iter()
            .filter(|(_, d)| {
                d.teleport.is_some_and(|t| refs.get(t.door).is_some_and(|inside| inside.cell == Some(cell)))
                    && d.cell.is_some_and(|c| refs.cell_world.contains_key(&c))
            })
            .map(|(id, _)| *id)
            .collect()
    }
}

/// The door a marker for a target indoors stands at: the first of `doors` that is not
/// locked in `world`, else the first; with `ignores_locks` the first [hypothesis: the
/// Objectives Tab's path "will attempt to avoid locked doors if possible"; NVForever picks
/// one door, quests-dialogue.md §2].
pub fn marker_door(world: &World, doors: &[FormId], ignores_locks: bool) -> Option<FormId> {
    let first = doors.first().copied();
    if ignores_locks {
        return first;
    }
    doors.iter().copied().find(|d| !door_locked(world, *d)).or(first)
}

/// The door is locked now: its run-time state once the world holds one, else its placed
/// lock.
fn door_locked(world: &World, door: FormId) -> bool {
    match world.peek(door) {
        Some(s) => s.lock.is_some_and(|l| l.locked),
        None => world.placed.get(door).is_some_and(|p| p.lock.is_some()),
    }
}

/// Where a reference is, for the doors between places: an interior cell, or a worldspace
/// (its exterior cells are one place).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Place {
    Interior(FormId),
    World(FormId),
}

impl Place {
    /// The place of a reference in `cell`.
    pub fn of_cell(refs: &fnv_sim::RefTable, cell: FormId) -> Place {
        refs.cell_world.get(&cell).map_or(Place::Interior(cell), |&w| Place::World(w))
    }
}

/// Places joined by their load doors (REFR XTEL): from each place, the doors in it and
/// the place each leads to, in FormID order. Built once from the placed references.
#[derive(Debug, Clone, Default)]
pub struct DoorGraph {
    pub edges: std::collections::BTreeMap<Place, Vec<(FormId, Place)>>,
}

/// A quest marker leads through at most this many doors [design].
pub const MARKER_DOORS_MAX: usize = 8;

impl DoorGraph {
    pub fn build(refs: &fnv_sim::RefTable) -> DoorGraph {
        let mut edges: std::collections::BTreeMap<Place, Vec<(FormId, Place)>> = Default::default();
        for (&id, d) in &refs.refs {
            let (Some(t), Some(cell)) = (d.teleport, d.cell) else { continue };
            let Some(dest) = refs.get(t.door).and_then(|o| o.cell) else { continue };
            edges.entry(Place::of_cell(refs, cell)).or_default().push((id, Place::of_cell(refs, dest)));
        }
        DoorGraph { edges }
    }

    /// The door of `from` that starts the way to `to` through the fewest doors (at most
    /// [`MARKER_DOORS_MAX`]): a way without locked doors when there is one, unless
    /// `ignores_locks` [hypothesis: the Objectives Tab's marker "will attempt to avoid
    /// locked doors if possible" (GECK wiki), applied to every door on the way].
    pub fn first_door(&self, world: &World, from: Place, to: Place, ignores_locks: bool) -> Option<FormId> {
        if !ignores_locks {
            if let Some(d) = self.search(from, to, |d| !door_locked(world, d)) {
                return Some(d);
            }
        }
        self.search(from, to, |_| true)
    }

    /// Breadth-first over the places from `from`: the first door of the shortest way to
    /// `to` through doors that `open` allows; doors in FormID order, so ties go to the
    /// lowest.
    pub fn search(&self, from: Place, to: Place, open: impl Fn(FormId) -> bool) -> Option<FormId> {
        if from == to {
            return None;
        }
        let mut seen = std::collections::BTreeSet::from([from]);
        let mut frontier: Vec<(Place, FormId)> = Vec::new();
        for &(d, next) in self.edges.get(&from).into_iter().flatten() {
            if open(d) && seen.insert(next) {
                if next == to {
                    return Some(d);
                }
                frontier.push((next, d));
            }
        }
        for _ in 1..MARKER_DOORS_MAX {
            let mut next_frontier = Vec::new();
            for (place, first) in frontier {
                for &(d, next) in self.edges.get(&place).into_iter().flatten() {
                    if open(d) && seen.insert(next) {
                        if next == to {
                            return Some(first);
                        }
                        next_frontier.push((next, first));
                    }
                }
            }
            if next_frontier.is_empty() {
                return None;
            }
            frontier = next_frontier;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A worldspace W with a building B (two floors B1, B2) and a cave C entered from W.
    fn graph() -> DoorGraph {
        let (w, b1, b2, c) = (Place::World(FormId(1)), Place::Interior(FormId(10)), Place::Interior(FormId(11)), Place::Interior(FormId(20)));
        let mut g = DoorGraph::default();
        g.edges.insert(w, vec![(FormId(0x100), b1), (FormId(0x101), c)]);
        g.edges.insert(b1, vec![(FormId(0x110), w), (FormId(0x111), b2)]);
        g.edges.insert(b2, vec![(FormId(0x120), b1)]);
        g.edges.insert(c, vec![(FormId(0x130), w)]);
        g
    }

    #[test]
    fn a_marker_leads_through_the_first_door_of_the_shortest_way() {
        let g = graph();
        let (w, b2, c) = (Place::World(FormId(1)), Place::Interior(FormId(11)), Place::Interior(FormId(20)));
        // From the cave to the building's upper floor: out of the cave first.
        assert_eq!(g.search(c, b2, |_| true), Some(FormId(0x130)));
        // From outside: the building's door, two floors below the target.
        assert_eq!(g.search(w, b2, |_| true), Some(FormId(0x100)));
        // Upstairs to the cave: down first.
        assert_eq!(g.search(b2, c, |_| true), Some(FormId(0x120)));
        // There already, and a door shut on the only way.
        assert_eq!(g.search(c, c, |_| true), None);
        assert_eq!(g.search(w, b2, |d| d != FormId(0x111)), None);
    }
}
