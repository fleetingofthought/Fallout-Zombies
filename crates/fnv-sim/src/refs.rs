//! Placed references (REFR, ACHR, ACRE) as authored in the plugins.
//!
//! This is the simulation's read-only view of placement data: what a reference is an
//! instance of, where it starts, and the extra data that drives activation (count,
//! lock, teleport, ownership, enable parent). Layer 3 owns streaming and drawing the
//! same references; this table only needs to exist once per load order.
//!
//! Which references exist, and the cell each is in, comes from Layer 3's
//! [`fnv_world::WorldIndex`] (innermost cell-children group of the winning definition,
//! docs/spec-notes/world.md), so the renderer and the simulation agree by construction;
//! this module adds the extra data the simulation needs.

use fnv_esm::decode::SubStatus;
use fnv_esm::raw::record_flags;
use fnv_esm::{LoadOrder, Record, Schema};
use fnv_world::{RefEntry, WorldIndex};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::extract::{self, all, first, global, has_flag, int, local_fid, num, text, Issues};
use crate::form::FormId;
use crate::forms::ser_sig;

/// Record flag 0x400 on references: "Persistent" [documented: GECK wiki GetReferenceFlag,
/// flag 1024 "Persistent"; GECK wiki Reference, "Reference Data Flags": the reference is
/// always loaded; xEdit wbDefinitionsFNV.pas record flag 10]. `fnvtool sim forms` reports
/// how it correlates with persistent-children groups.
pub const FLAG_PERSISTENT: u32 = 0x0000_0400;
/// Record flag 0x800 on references: "Initially Disabled" [documented: GECK wiki
/// GetReferenceFlag, flag 2048; GECK wiki Reference: the reference starts inactive and
/// unseen until a script or enable parent enables it; xEdit record flag 11].
pub const FLAG_INITIALLY_DISABLED: u32 = 0x0000_0800;
/// Record flag 0x0200_0000 (bit 25) on references: "No AI Acquire", "AI will not
/// attempt to pick up containers or objects marked with this flag" [documented: GECK
/// wiki Reference, "Reference Data Flags"]. 67 REFR and 12 actor references in the
/// masters carry it [observed].
pub const FLAG_NO_AI_ACQUIRE: u32 = 0x0200_0000;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Lock {
    pub level: u8,
    pub key: Option<FormId>,
    pub leveled: bool,
}

/// How many levels raise a leveled lock by one difficulty step, and the step (one lock
/// level class: Very Easy 0, Easy 25, Average 50, Hard 75, Very Hard 100) [hypothesis:
/// a leveled lock "will start at the base level specified and become harder as the
/// player levels" (documented: GECK wiki Reference, Lock tab); by how much is not
/// documented; whether the original hardens at all is not known either, the GECK tutorial
/// calls the checkbox deprecated; docs/spec-notes/simulation.md §5].
pub const LEVELED_LOCK_LEVELS_PER_STEP: u16 = 10;
pub const LEVELED_LOCK_STEP: u8 = 25;
/// The hardest a leveled lock gets: Very Hard; it never becomes Requires Key (255)
/// [hypothesis].
pub const LEVELED_LOCK_MAX: u8 = 100;

impl Lock {
    /// The lock's level read at `level` (`World::lock_read_level`: the level fixed when its
    /// cell loaded, else the configured one, 1 in play): the authored level, raised for a
    /// leveled lock by [`LEVELED_LOCK_STEP`] for every [`LEVELED_LOCK_LEVELS_PER_STEP`]
    /// levels past the first, up to [`LEVELED_LOCK_MAX`]; a lock that needs a key (above
    /// Very Hard) and a lock not leveled keep theirs [hypothesis, see the constants].
    pub fn level_at(&self, level: u16) -> u8 {
        if !self.leveled || self.level > LEVELED_LOCK_MAX {
            return self.level;
        }
        let steps = level.saturating_sub(1) / LEVELED_LOCK_LEVELS_PER_STEP;
        let raised = self.level as u32 + steps as u32 * LEVELED_LOCK_STEP as u32;
        raised.min(LEVELED_LOCK_MAX.max(self.level) as u32) as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Teleport {
    /// The door reference at the destination.
    pub door: FormId,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    /// XTEL "No Alarm": "No trespass alarm will be sent when the player uses the door"
    /// [documented: GECK wiki Reference, Teleport; xEdit wbDefinitionsFNV.pas REFR XTEL]
    /// (`fnv_game::crime`) (not written to JSON: the tools' output stays as it was).
    #[serde(skip)]
    pub no_alarm: bool,
}

/// XMRK, the data of a map marker reference [documented: xEdit wbDefinitionsFNV.pas
/// REFR "Map Marker Data"; schemas/fnv/world.json].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MapMarker {
    /// FNAM: 1 Visible, 2 Can Travel To, 4 "Show All" Hidden.
    pub flags: u8,
    /// TNAM's marker type, 0 None … 14 Vault (the icon).
    pub kind: u8,
}

/// XRDO, the radio data of a talking activator's reference [documented: xEdit
/// wbDefinitionsFNV.pas REFR "Radio Data"]: the range's radius, its type (0 Radius,
/// 1 Everywhere, 2 Worldspace and Linked Interiors, 3 Linked Interiors, 4 Current Cell
/// Only), the static percentage and the exterior position reference.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RadioData {
    pub radius: f32,
    pub range: u32,
    pub static_percent: f32,
    pub position_ref: Option<FormId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EnableParent {
    pub reference: FormId,
    pub opposite: bool,
    /// XESP flag "Pop In": enabled through its parent, it appears at once instead of
    /// fading in [documented: xEdit wbXESP flag bit 1; GECK wiki Enable, "In order for the
    /// children to Pop In (Instead of fade in which is the default for Enable) you must
    /// go to each child and ... check the 'Pop In' checkbox"].
    /// Not serialised: dumps of placed references stay as they were.
    #[serde(skip)]
    pub pop_in: bool,
}

/// An activate parent of a reference (XAPR): activating `reference` (its default
/// activation) sets this one off after `delay` seconds [documented: xEdit
/// wbDefinitionsFNV.pas "Activate Parent Ref" (reference, delay); GECK wiki Bethsoft
/// Tutorial Traps; docs/spec-notes/traps.md §2].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ActivateParent {
    pub reference: FormId,
    pub delay: f32,
}

/// A volume stored on the reference itself (XPRM) rather than in a model: trigger
/// boxes, collision markers [documented: xEdit REFR XPRM]. `bounds` are read as half
/// extents, a sphere's radius being the first [hypothesis: docs/spec-notes/physics.md
/// §4.1].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Primitive {
    pub sphere: bool,
    pub bounds: [f32; 3],
}

/// Record header flag of a placed reference: "Inaccessible" (bit 8) [documented: GECK
/// wiki GetFormFlag] (docs/spec-notes/cells-refs.md §2).
pub const INACCESSIBLE: u32 = 0x100;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacedRef {
    pub id: FormId,
    #[serde(serialize_with = "ser_sig")]
    pub sig: [u8; 4],
    pub edid: Option<String>,
    pub base: FormId,
    /// The reference's parent cell: the cell whose group stores it, except that a
    /// reference stored in a worldspace's persistent cell belongs to the exterior cell
    /// containing its position (see `stored_in`).
    pub cell: Option<FormId>,
    /// The cell whose children group stores the winning definition.
    pub stored_in: Option<FormId>,
    /// Load-order index of the plugin whose definition won.
    pub plugin: u8,
    pub flags: u32,
    /// Found in a cell's persistent-children group.
    pub in_persistent_group: bool,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: Option<f32>,
    /// XCNT: stack size of an item reference.
    pub count: Option<i32>,
    /// XAMT Ammo Type and XAMC Ammo Count of a placed weapon [documented: xEdit REFR]:
    /// picked up, the weapon comes with that ammo (docs/spec-notes/world-fields.md §1)
    /// (not written to JSON).
    #[serde(skip)]
    pub ammo: Option<(FormId, i32)>,
    /// XHLP: condition of a placed weapon or armour as a fraction of full health.
    pub health: Option<f32>,
    pub owner: Option<FormId>,
    pub lock: Option<Lock>,
    pub teleport: Option<Teleport>,
    pub enable_parent: Option<EnableParent>,
    pub linked: Option<FormId>,
    /// XPRD: seconds an actor waits at this patrol point before going on to the next
    /// linked one [documented: xEdit patrol data; GECK wiki, Idle Markers "Idle Time"].
    pub patrol_idle: Option<f32>,
    /// TNAM of a patrol point (4 bytes, after XPPA): the topic an actor says on arriving
    /// [documented: xEdit patrol data; GECK wiki, Idle Markers "Topic"].
    pub patrol_topic: Option<FormId>,
    /// XMRC on an actor reference: the container holding the actor's merchant stock.
    pub merchant_container: Option<FormId>,
    /// WMI1 on a map marker: the reputation it belongs to [documented: xEdit
    /// wbDefinitionsFNV.pas REFR, "Map Marker Reputation"].
    #[serde(skip)]
    pub marker_reputation: Option<FormId>,
    /// ONAM "Open by Default" (doors) [documented: xEdit wbDefinitionsFNV.pas REFR].
    pub open_by_default: bool,
    /// XPRM, when it is a box or a sphere (not written to JSON: the tools' output
    /// stays as it was).
    #[serde(skip)]
    pub primitive: Option<Primitive>,
    /// XAPR, one per parent (not written to JSON).
    #[serde(skip)]
    pub activate_parents: Vec<ActivateParent>,
    /// XAPD "Parent Activate Only": only its activate parents set it off; the player
    /// cannot activate it by hand [documented: xEdit; hypothesis: what the flag does,
    /// traps.md §2] (not written to JSON).
    #[serde(skip)]
    pub parent_activate_only: bool,
    /// XATO Activation Prompt: the text under the crosshair in place of the action
    /// (docs/spec-notes/cells-refs.md §3) (not written to JSON).
    #[serde(skip)]
    pub prompt: Option<String>,
    /// XRAD: the radiation a radiation marker gives, rads a second, and XRDS, the radius
    /// it reaches (game units) [documented: xEdit wbDefinitionsFNV.pas REFR; GECK wiki
    /// GetRadiationExtraData, "the amount of rads gained while standing in the marker's
    /// radius"]. docs/spec-notes/radiation.md §2. Not written to JSON.
    #[serde(skip)]
    pub radiation: Option<f32>,
    #[serde(skip)]
    pub radius: Option<f32>,
    /// XEZN: the reference's own encounter zone, over its cell's
    /// (docs/spec-notes/encounter-zones.md §1). Not written to JSON.
    #[serde(skip)]
    pub encounter_zone: Option<FormId>,
    /// XLCM on a placed leveled actor: 0 Easy, 1 Medium, 2 Hard, 3 Very Hard (Boss);
    /// `None` for the GECK's "None" (docs/spec-notes/encounter-zones.md §3). Not written
    /// to JSON.
    #[serde(skip)]
    pub level_modifier: Option<i32>,
    /// XMRK: the reference is a map marker, with its flags and type [documented: xEdit
    /// wbDefinitionsFNV.pas REFR "Map Marker Data"]. Not written to JSON.
    #[serde(skip)]
    pub map_marker: Option<MapMarker>,
    /// XRDO: a radio station's broadcast. Not written to JSON.
    #[serde(skip)]
    pub radio: Option<RadioData>,
}

impl PlacedRef {
    pub fn initially_disabled(&self) -> bool {
        self.flags & FLAG_INITIALLY_DISABLED != 0
    }

    /// [`FLAG_NO_AI_ACQUIRE`]: actors pick nothing up from it on their own.
    pub fn no_ai_acquire(&self) -> bool {
        self.flags & FLAG_NO_AI_ACQUIRE != 0
    }
}

pub const REF_SIGS: &[&[u8; 4]] = &[b"REFR", b"ACHR", b"ACRE", b"PGRE"];

#[derive(Default)]
pub struct RefTable {
    pub refs: BTreeMap<FormId, PlacedRef>,
    /// Lower-case Editor ID -> reference.
    pub edids: HashMap<String, FormId>,
    /// References by cell, in FormID order.
    pub by_cell: BTreeMap<FormId, Vec<FormId>>,
    /// Lower-case cell Editor ID -> cell.
    pub cell_edids: HashMap<String, FormId>,
    /// Cells that belong to a worldspace (exterior cells and the persistent cell).
    pub cell_world: HashMap<FormId, FormId>,
    /// Each worldspace's exterior cells by grid coordinate.
    pub world_grid: HashMap<FormId, BTreeMap<(i32, i32), FormId>>,
    /// Merchant containers (XMRC targets) -> the actor references whose stock they hold.
    pub merchant_containers: BTreeMap<FormId, Vec<FormId>>,
    /// Activate parents -> their activate children with each child's delay (XAPR read
    /// backwards).
    pub activate_children: BTreeMap<FormId, Vec<(FormId, f32)>>,
    /// Cells and worldspaces with an encounter zone (CELL and WRLD XEZN,
    /// docs/spec-notes/encounter-zones.md §1).
    pub cell_zones: HashMap<FormId, FormId>,
    pub world_zones: HashMap<FormId, FormId>,
    /// References whose winning definition carries the Deleted flag (left out).
    pub deleted: u64,
    pub issues: Issues,
}

impl RefTable {
    /// Build from scratch (indexes the load order's cells first).
    pub fn build(lo: &LoadOrder, schema: &Schema) -> RefTable {
        RefTable::from_index(lo, schema, &WorldIndex::build(lo, schema))
    }

    /// Build on an existing Layer 3 [`WorldIndex`], which decides which references
    /// exist and which cell each is in (one rule for the renderer and the simulation).
    pub fn from_index(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex) -> RefTable {
        let mut t = RefTable::default();
        for &(pi, fid) in &lo.unmappable {
            t.issues.note("record FormID outside master list", || {
                format!("{} 0x{fid:08X}", lo.plugins[pi as usize].name)
            });
        }
        let mut ids: Vec<(u32, &RefEntry)> = Vec::with_capacity(ix.refs.len());
        for (&fid, e) in &ix.refs {
            if REF_SIGS.contains(&&e.sig) {
                ids.push((fid, e));
            } else {
                t.issues.note(&format!("{}: placed type not simulated", String::from_utf8_lossy(&e.sig)), || {
                    format!("0x{fid:08X}")
                });
            }
        }
        ids.sort_unstable_by_key(|x| x.0);
        let loaded: Vec<(Option<PlacedRef>, bool, Issues)> = ids
            .par_iter()
            .map(|&(fid, e)| {
                let mut issues = Issues::default();
                let Some(d) = lo.index.get(&fid).and_then(|v| v.last()) else {
                    return (None, false, issues);
                };
                let (pi, r) = (d.plugin as usize, lo.records[d.plugin as usize][d.ordinal as usize]);
                if r.header.flags & record_flags::DELETED != 0 {
                    return (None, true, issues);
                }
                let pr = load_ref(lo, schema, pi, FormId(fid), r, FormId(e.cell), e.persistent, &mut issues);
                (pr, false, issues)
            })
            .collect();
        for c in ix.cells.values() {
            if let Some(e) = &c.edid {
                t.cell_edids.insert(e.to_ascii_lowercase(), FormId(c.form_id));
            }
            if let Some(w) = c.world {
                t.cell_world.insert(FormId(c.form_id), FormId(w));
            }
            if let Some(z) = c.encounter_zone {
                t.cell_zones.insert(FormId(c.form_id), FormId(z));
            }
        }
        let mut persistent_cells = HashSet::new();
        for (w, ws) in &ix.worldspaces {
            if let Some(z) = ws.encounter_zone {
                t.world_zones.insert(FormId(*w), FormId(z));
            }
            t.world_grid.insert(FormId(*w), ws.grid.iter().map(|(g, c)| (*g, FormId(*c))).collect());
            persistent_cells.extend(ws.persistent_cell.map(FormId));
        }
        for (pr, deleted, issues) in loaded {
            t.issues.merge(issues);
            t.deleted += deleted as u64;
            if let Some(mut pr) = pr {
                // A reference stored in a worldspace's persistent cell belongs to the
                // exterior cell that contains it. [observed in game: GetParentCell of the
                // persistent door 0x00103E69, asked outdoors, is Goodsprings 0x000DAEBB
                // (grid -18,0 containing its position), not the persistent cell]
                if let Some(c) = pr.cell.filter(|c| persistent_cells.contains(c)) {
                    match t.exterior_cell_at(c, pr.pos) {
                        Some(grid) => pr.cell = Some(grid),
                        None => t.issues.note("persistent exterior reference outside every grid cell", || {
                            format!("{} at ({}, {})", pr.id, pr.pos[0], pr.pos[1])
                        }),
                    }
                }
                if let Some(e) = &pr.edid {
                    t.edids.insert(e.to_ascii_lowercase(), pr.id);
                }
                if let Some(c) = pr.cell {
                    t.by_cell.entry(c).or_default().push(pr.id);
                }
                if let Some(m) = pr.merchant_container {
                    t.merchant_containers.entry(m).or_default().push(pr.id);
                }
                for p in &pr.activate_parents {
                    t.activate_children.entry(p.reference).or_default().push((pr.id, p.delay));
                }
                t.refs.insert(pr.id, pr);
            }
        }
        t
    }

    /// The exterior cell at `pos` in the worldspace that `cell` belongs to, if `cell` is
    /// a worldspace cell and a cell exists at that grid square.
    pub fn exterior_cell_at(&self, cell: FormId, pos: [f32; 3]) -> Option<FormId> {
        let world = self.cell_world.get(&cell)?;
        let (gx, gy) = fnv_world::cell_of(pos[0], pos[1]);
        self.world_grid.get(world)?.get(&(gx, gy)).copied()
    }

    /// The exterior cell of worldspace `world` at `pos`, if a cell exists at that grid
    /// square.
    pub fn world_cell_at(&self, world: FormId, pos: [f32; 3]) -> Option<FormId> {
        let (gx, gy) = fnv_world::cell_of(pos[0], pos[1]);
        self.world_grid.get(&world)?.get(&(gx, gy)).copied()
    }

    /// The encounter zone of a cell: its own (CELL XEZN), else its worldspace's (WRLD
    /// XEZN) [hypothesis: the worldspace's zone covers its cells that name none;
    /// docs/spec-notes/encounter-zones.md §1].
    pub fn cell_zone(&self, cell: FormId) -> Option<FormId> {
        self.cell_zones.get(&cell).or_else(|| self.world_of(cell).and_then(|w| self.world_zones.get(&w))).copied()
    }

    /// The encounter zone of a placed reference: its own XEZN, else its cell's
    /// ([`RefTable::cell_zone`]) [documented: GECK wiki Reference, "Encounter Zone: What
    /// Encounter Zone is this reference attached to?"; hypothesis: the reference's own
    /// zone wins over its cell's].
    pub fn zone_of(&self, id: FormId) -> Option<FormId> {
        let p = self.get(id)?;
        p.encounter_zone.or_else(|| self.cell_zone(p.cell?))
    }

    /// The worldspace a cell belongs to (`None`: an interior, or a cell not indexed).
    pub fn world_of(&self, cell: FormId) -> Option<FormId> {
        self.cell_world.get(&cell).copied()
    }

    /// The cell a teleport to a placed door lands in: the destination door's cell, or in
    /// a worldspace the exterior cell containing the destination position (as
    /// `World::activate` moves the actor). `None` when the door is not placed.
    pub fn teleport_cell(&self, tp: &Teleport) -> Option<FormId> {
        let c = self.get(tp.door)?.cell?;
        Some(self.exterior_cell_at(c, tp.pos).unwrap_or(c))
    }

    /// Whether `id` holds a merchant's stock (an actor reference names it in XMRC).
    /// [documented: xEdit "Merchant Container" on ACHR/ACRE; observed in game: such a
    /// container restocks on fixed days, not by the 72 h cell respawn — see
    /// `world::is_merchant_restock_day`, docs/spec-notes/simulation.md §3]
    pub fn is_merchant_container(&self, id: FormId) -> bool {
        self.merchant_containers.contains_key(&id)
    }

    /// A cell by Editor ID (case-insensitive).
    pub fn cell_by_edid(&self, edid: &str) -> Option<FormId> {
        self.cell_edids.get(&edid.to_ascii_lowercase()).copied()
    }

    pub fn get(&self, id: FormId) -> Option<&PlacedRef> {
        self.refs.get(&id)
    }

    /// Every placed reference's FormID, in no order.
    pub fn ids(&self) -> impl Iterator<Item = FormId> + '_ {
        self.refs.keys().copied()
    }

    pub fn by_edid(&self, edid: &str) -> Option<FormId> {
        self.edids.get(&edid.to_ascii_lowercase()).copied()
    }
}

fn field_fid(
    lo: &LoadOrder,
    pi: usize,
    id: FormId,
    v: Option<&serde_json::Value>,
    what: &str,
    issues: &mut Issues,
) -> Option<FormId> {
    match global(lo, pi, local_fid(v?)?) {
        Ok(g) => g,
        Err(l) => {
            issues.note("FormID outside master list", || format!("{id} {what} 0x{l:08X}"));
            None
        }
    }
}

/// A decoded number or flags value as its number (`{"value": n, …}` or `n`).
fn code(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.get("value").and_then(serde_json::Value::as_u64))
}

/// XRDO's broadcast range type as its number: the decoder gives a known value as the
/// schema's name for it (schemas/fnv/world.json XRDO), an unknown one as the number.
fn broadcast_range_code(v: &serde_json::Value) -> Option<u64> {
    const NAMES: [&str; 5] = ["Radius", "Everywhere", "Worldspace and Linked Interiors", "Linked Interiors", "Current Cell Only"];
    match v.as_str() {
        Some(name) => NAMES.iter().position(|n| *n == name).map(|i| i as u64),
        None => code(v),
    }
}

/// A decoded boolean enum (`BoolU8`: "True" / "False", or its number).
fn truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        serde_json::Value::String(s) => s.eq_ignore_ascii_case("true") || s.parse::<f64>().is_ok_and(|x| x != 0.0),
        serde_json::Value::Object(o) => o.get("value").or_else(|| o.get("name")).is_some_and(truthy),
        _ => false,
    }
}

fn vec3(v: Option<&serde_json::Value>) -> Option<[f32; 3]> {
    let v = v?;
    Some([num(v, "x")? as f32, num(v, "y")? as f32, num(v, "z")? as f32])
}

#[allow(clippy::too_many_arguments)]
fn load_ref(
    lo: &LoadOrder,
    schema: &Schema,
    pi: usize,
    id: FormId,
    r: &Record,
    cell: FormId,
    in_persistent_group: bool,
    issues: &mut Issues,
) -> Option<PlacedRef> {
    // Placed explosives (PGRE) use the placed-reference subrecords; the schema
    // describes them once, under REFR. [hypothesis: xEdit defines PGRE with the same
    // placed-object subrecords; checked by counting subrecords the REFR layout does not
    // decode, reported below]
    let layout = if &r.header.sig == b"PGRE" { b"REFR" } else { &r.header.sig };
    let d = match extract::decode_as(r, schema, layout) {
        Ok(d) => d,
        Err(e) => {
            issues.note("reference undecodable", || format!("{id}: {e}"));
            return None;
        }
    };
    if &r.header.sig == b"PGRE" {
        for x in d.iter().filter(|x| x.status != SubStatus::Ok) {
            issues.note(&format!("PGRE subrecord {} not decoded by the REFR layout", String::from_utf8_lossy(&x.sig)), || {
                format!("{id}")
            });
        }
    }
    let fid = |v: Option<&serde_json::Value>, what: &str, issues: &mut Issues| field_fid(lo, pi, id, v, what, issues);
    let Some(base) = fid(first(&d, b"NAME"), "NAME", issues) else {
        issues.note("reference without base (NAME)", || format!("{id}"));
        return None;
    };
    let posrot = first(&d, b"DATA");
    let pos = vec3(posrot.and_then(|v| v.get("position")));
    let rot = vec3(posrot.and_then(|v| v.get("rotation_radians")));
    if pos.is_none() {
        issues.note("reference without position (DATA)", || format!("{id}"));
    }
    let lock = first(&d, b"XLOC").map(|v| Lock {
        level: int(v, "level").unwrap_or(0) as u8,
        key: fid(v.get("key"), "XLOC key", issues),
        leveled: v.get("flags").is_some_and(|f| has_flag(f, "Leveled Lock")),
    });
    let teleport = match first(&d, b"XTEL") {
        Some(v) => {
            let door = fid(v.get("door"), "XTEL door", issues);
            let pr = v.get("pos_rot");
            match (door, vec3(pr.and_then(|p| p.get("position"))), vec3(pr.and_then(|p| p.get("rotation_radians")))) {
                (Some(door), Some(pos), Some(rot)) => Some(Teleport { door, pos, rot, no_alarm: v.get("no_alarm").is_some_and(|a| a.as_str() == Some("True") || a.as_u64().is_some_and(|n| n != 0)) }),
                _ => {
                    issues.note("XTEL incomplete", || format!("{id}"));
                    None
                }
            }
        }
        None => None,
    };
    let enable_parent = first(&d, b"XESP").and_then(|v| {
        Some(EnableParent {
            reference: fid(v.get("reference"), "XESP", issues)?,
            opposite: v.get("flags").is_some_and(|f| has_flag(f, "Set Enable State to Opposite of Parent")),
            pop_in: v.get("flags").is_some_and(|f| has_flag(f, "Pop In")),
        })
    });
    Some(PlacedRef {
        id,
        sig: r.header.sig,
        edid: first(&d, b"EDID").and_then(text),
        base,
        cell: Some(cell),
        stored_in: Some(cell),
        plugin: pi as u8,
        flags: r.header.flags,
        in_persistent_group,
        pos: pos.unwrap_or_default(),
        rot: rot.unwrap_or_default(),
        scale: first(&d, b"XSCL").and_then(serde_json::Value::as_f64).map(|s| s as f32),
        count: first(&d, b"XCNT").and_then(serde_json::Value::as_i64).map(|c| c as i32),
        ammo: fid(first(&d, b"XAMT"), "XAMT", issues).map(|a| (a, first(&d, b"XAMC").and_then(serde_json::Value::as_i64).unwrap_or(0) as i32)).filter(|a| a.1 > 0),
        health: first(&d, b"XHLP").and_then(serde_json::Value::as_f64).map(|h| h as f32),
        owner: fid(first(&d, b"XOWN"), "XOWN", issues),
        lock,
        teleport,
        enable_parent,
        linked: fid(first(&d, b"XLKR"), "XLKR", issues),
        patrol_idle: first(&d, b"XPRD").and_then(|v| v.as_f64()).map(|x| x as f32),
        // The map marker's TNAM decodes as a struct, the patrol topic as a FormID.
        patrol_topic: if d.iter().any(|x| &x.sig == b"XPPA") { all(&d, b"TNAM").find(|v| v.is_string()).and_then(|v| fid(Some(v), "TNAM", issues)) } else { None },
        merchant_container: fid(first(&d, b"XMRC"), "XMRC", issues),
        marker_reputation: fid(first(&d, b"WMI1"), "WMI1", issues),
        open_by_default: first(&d, b"ONAM").is_some(),
        primitive: first(&d, b"XPRM").and_then(|v| {
            let sphere = match v.get("type").and_then(|t| t.as_str()) {
                Some("Box") => false,
                Some("Sphere") => true,
                _ => return None,
            };
            Some(Primitive { sphere, bounds: vec3(v.get("bounds"))?.map(f32::abs) })
        }),
        activate_parents: all(&d, b"XAPR")
            .filter_map(|v| Some(ActivateParent { reference: fid(v.get("reference"), "XAPR", issues)?, delay: num(v, "delay").unwrap_or(0.0) as f32 }))
            .collect(),
        parent_activate_only: first(&d, b"XAPD").is_some_and(truthy),
        prompt: first(&d, b"XATO").and_then(text).filter(|t| !t.trim().is_empty()),
        radiation: first(&d, b"XRAD").and_then(serde_json::Value::as_f64).map(|x| x as f32),
        radius: first(&d, b"XRDS").and_then(serde_json::Value::as_f64).map(|x| x as f32),
        encounter_zone: fid(first(&d, b"XEZN"), "XEZN", issues),
        level_modifier: first(&d, b"XLCM").and_then(serde_json::Value::as_i64).map(|x| x as i32),
        map_marker: d.iter().any(|x| &x.sig == b"XMRK").then(|| MapMarker {
            flags: first(&d, b"FNAM").and_then(code).unwrap_or(0) as u8,
            kind: all(&d, b"TNAM").find_map(|v| v.get("marker_type").and_then(code)).unwrap_or(0) as u8,
        }),
        radio: first(&d, b"XRDO").map(|v| RadioData {
            radius: num(v, "range_radius").unwrap_or(0.0) as f32,
            range: v.get("broadcast_range_type").and_then(broadcast_range_code).unwrap_or(0) as u32,
            static_percent: num(v, "static_percentage").unwrap_or(0.0) as f32,
            position_ref: fid(v.get("position_reference"), "XRDO", issues),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leveled_locks_harden_with_the_level() {
        let lock = |level: u8, leveled: bool| Lock { level, key: None, leveled };
        // Not leveled: the authored level whatever the level.
        assert_eq!(lock(25, false).level_at(40), 25);
        // Leveled: one step of 25 for every 10 levels past the first.
        assert_eq!(lock(0, true).level_at(1), 0);
        assert_eq!(lock(0, true).level_at(10), 0);
        assert_eq!(lock(0, true).level_at(11), 25);
        assert_eq!(lock(25, true).level_at(21), 75);
        // Up to Very Hard, never Requires Key.
        assert_eq!(lock(50, true).level_at(50), 100);
        assert_eq!(lock(100, true).level_at(30), 100);
        assert_eq!(lock(255, true).level_at(30), 255);
        // Level 0 (no level known) counts as the first.
        assert_eq!(lock(50, true).level_at(0), 50);
    }
}
