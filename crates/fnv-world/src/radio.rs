//! Radios: the stations (talking activators marked as radio stations), the placed
//! references of the stations with their broadcast range, and the receivers (activators
//! that name a station). Facts and tags: docs/spec-notes/radio.md.
//!
//! What a station plays (its quests' Radio lines) is dialogue and lives in `fnv-game`
//! (`fnv_game::radio`); how a receiver sounds is the engine's.

use crate::fields::{formid_of, global, num, winner, Fields};
use crate::index::WorldIndex;
use crate::placed::{placement, ref_flags};
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;
use std::collections::BTreeMap;

/// TACT record-header flag bits seen on stations (docs/spec-notes/radio.md §2).
pub mod station_flags {
    /// Set on every talking activator the editor's tutorial would call a radio station
    /// (the GECK's "Radio Station" box) [hypothesis: by the stations that have it,
    /// radio.md §2].
    pub const RADIO_STATION: u32 = 0x0002_0000;
    /// The GECK's "Continuous Broadcast" box: the station's broadcast state at the start
    /// [hypothesis: radio.md §2].
    pub const CONTINUOUS: u32 = 0x4000_0000;
    /// "Non-Pipboy": the station is not on the Pip-Boy's list [documented: GECK wiki
    /// GetFormFlag and GetFlagsHigh, the record-header flag table adapted from FNVEdit:
    /// bit 28 "Non-Pipboy / Reflected by Auto Water"; radio.md §2: set on the casino
    /// radios, the Tops performers, Hanlon's and Lily's recordings, EDE and Repcon, not
    /// on Radio New Vegas, Mojave Music Radio, Black Mountain Radio, Camp Guardian or the
    /// NCR Emergency Radio].
    pub const NON_PIPBOY: u32 = 0x1000_0000;
}

/// A talking activator (TACT).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Station {
    pub form_id: u32,
    pub edid: Option<String>,
    pub name: Option<String>,
    /// The record header's flags.
    pub flags: u32,
    /// `VNAM` voice type.
    pub voice_type: Option<u32>,
    /// `INAM` radio template (SOUN): how far a receiver of the station is heard.
    pub template: Option<u32>,
    /// `SNAM` looping sound.
    pub looping: Option<u32>,
}

impl Station {
    pub fn is_radio(&self) -> bool {
        self.flags & station_flags::RADIO_STATION != 0
    }

    /// Broadcasting when the game starts [hypothesis, see [`station_flags::CONTINUOUS`]].
    pub fn continuous(&self) -> bool {
        self.flags & station_flags::CONTINUOUS != 0
    }

    /// A radio station the Pip-Boy lists (see [`station_flags::NON_PIPBOY`]).
    pub fn on_pipboy(&self) -> bool {
        self.is_radio() && self.flags & station_flags::NON_PIPBOY == 0
    }
}

/// An activator (ACTI) that names a radio station (`RNAM`): a receiver.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReceiverBase {
    pub form_id: u32,
    pub edid: Option<String>,
    pub name: Option<String>,
    pub station: u32,
    /// `INAM` radio template (SOUN) of the activator itself.
    pub template: Option<u32>,
    /// `SCRI`.
    pub script: Option<u32>,
    /// `DEST` present: the activator can be destroyed.
    pub destructible: bool,
}

/// The radio forms of a load order.
#[derive(Debug, Clone, Default)]
pub struct RadioForms {
    /// Every TACT, radio station or not.
    pub stations: BTreeMap<u32, Station>,
    pub receivers: BTreeMap<u32, ReceiverBase>,
}

impl RadioForms {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> RadioForms {
        let mut out = RadioForms::default();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let pi = d.plugin as usize;
            let r = lo.records[pi][d.ordinal as usize];
            if !matches!(&r.header.sig, b"TACT" | b"ACTI") || r.header.flags & ref_flags::DELETED != 0 {
                continue;
            }
            let Ok(f) = Fields::new(r, pi, schema) else { continue };
            let edid = f.string(b"EDID");
            let name = f.string(b"FULL").filter(|s| !s.trim().is_empty());
            if &r.header.sig == b"TACT" {
                let station = Station {
                    form_id: g,
                    edid,
                    name,
                    flags: r.header.flags,
                    voice_type: f.formid(b"VNAM", lo),
                    template: f.formid(b"INAM", lo),
                    looping: f.formid(b"SNAM", lo),
                };
                out.stations.insert(g, station);
            } else if let Some(station) = f.formid(b"RNAM", lo) {
                let receiver = ReceiverBase {
                    form_id: g,
                    edid,
                    name,
                    station,
                    template: f.formid(b"INAM", lo),
                    script: f.formid(b"SCRI", lo),
                    destructible: f.raw(b"DEST").is_some(),
                };
                out.receivers.insert(g, receiver);
            }
        }
        out
    }
}

/// REFR `XRDO` broadcast range type [documented: xEdit REFR "Radio Data"; GECK wiki
/// Reference, "Broadcast Range Type"].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BroadcastRange {
    #[default]
    Radius,
    Everywhere,
    WorldspaceAndLinkedInteriors,
    LinkedInteriors,
    CurrentCell,
    Other(u32),
}

impl BroadcastRange {
    /// The range type of an `XRDO` code (LU's SetRadioBroadcastType, `fnv_game::nvse_lu2`).
    pub fn of_code(code: u32) -> BroadcastRange {
        BroadcastRange::of(code)
    }

    fn of(code: u32) -> BroadcastRange {
        match code {
            0 => BroadcastRange::Radius,
            1 => BroadcastRange::Everywhere,
            2 => BroadcastRange::WorldspaceAndLinkedInteriors,
            3 => BroadcastRange::LinkedInteriors,
            4 => BroadcastRange::CurrentCell,
            n => BroadcastRange::Other(n),
        }
    }
}

/// A placed reference of a talking activator, with its `XRDO` when it has one.
#[derive(Debug, Clone, PartialEq)]
pub struct StationRef {
    pub ref_id: u32,
    pub base: u32,
    pub cell: u32,
    pub position: [f32; 3],
    pub initially_disabled: bool,
    /// `XRDO`: range radius, range type, static percentage, exterior position reference.
    pub radio: Option<(f32, BroadcastRange, f32, Option<u32>)>,
}

/// A placed receiver.
#[derive(Debug, Clone, PartialEq)]
pub struct Receiver {
    pub ref_id: u32,
    pub base: u32,
    pub station: u32,
    pub cell: u32,
    pub position: [f32; 3],
    pub initially_disabled: bool,
    pub enable_parent: Option<(u32, bool)>,
}

/// The placed stations and receivers of `cells`, by reference.
pub fn placed(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, forms: &RadioForms, cells: impl IntoIterator<Item = u32>) -> (Vec<StationRef>, Vec<Receiver>) {
    let (mut stations, mut receivers) = (Vec::new(), Vec::new());
    for cell in cells {
        let Some(c) = ix.cells.get(&cell) else { continue };
        for &fid in &c.refs {
            let Some((pi, r)) = winner(lo, fid) else { continue };
            if &r.header.sig != b"REFR" || r.header.flags & ref_flags::DELETED != 0 {
                continue;
            }
            let Ok(f) = Fields::new(r, pi, schema) else { continue };
            let Some(base) = f.formid(b"NAME", lo) else { continue };
            let is_station = forms.stations.contains_key(&base);
            let receiver = forms.receivers.get(&base);
            if !is_station && receiver.is_none() {
                continue;
            }
            let Some(p) = placement(lo, schema, fid) else { continue };
            if is_station {
                let radio = f.get(b"XRDO").map(|v| {
                    let range = v.get("broadcast_range_type").map_or(0, enum_code);
                    let pos_ref = v.get("position_reference").and_then(formid_of).and_then(|l| global(lo, pi, l));
                    (num(&v, "range_radius").unwrap_or(0.0) as f32, BroadcastRange::of(range), num(&v, "static_percentage").unwrap_or(0.0) as f32, pos_ref)
                });
                stations.push(StationRef { ref_id: fid, base, cell, position: p.position, initially_disabled: p.initially_disabled(), radio });
            }
            if let Some(rb) = receiver {
                receivers.push(Receiver {
                    ref_id: fid,
                    base,
                    station: rb.station,
                    cell,
                    position: p.position,
                    initially_disabled: p.initially_disabled(),
                    enable_parent: p.enable_parent,
                });
            }
        }
    }
    stations.sort_by_key(|s| s.ref_id);
    receivers.sort_by_key(|r| r.ref_id);
    (stations, receivers)
}

/// Where a placed station broadcasts from, for a listener's reception ([`reception`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Broadcast {
    pub range: BroadcastRange,
    pub radius: f32,
    /// Percentage of the radius, at its edge, over which static is heard.
    pub static_percent: f32,
    /// The station reference's own cell.
    pub cell: u32,
    /// Interior cells have none.
    pub world: Option<u32>,
    /// Where the station is in a worldspace: its own position outdoors, else its
    /// exterior position reference's ("for radio stations in interiors, you can select an
    /// exterior reference where the station appears to broadcast from" [documented: GECK
    /// wiki Reference, Broadcast Range Type]).
    pub exterior: Option<(u32, [f32; 3])>,
}

/// The station reference `s` as a [`Broadcast`]; `None` without `XRDO`.
pub fn broadcast(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, s: &StationRef) -> Option<Broadcast> {
    let (radius, range, static_percent, pos_ref) = s.radio?;
    let world_of = |cell: u32| ix.cells.get(&cell).and_then(|c| c.world);
    let world = world_of(s.cell);
    let exterior = match world {
        Some(w) => Some((w, s.position)),
        None => pos_ref.and_then(|r| {
            let w = world_of(ix.refs.get(&r)?.cell)?;
            Some((w, placement(lo, schema, r)?.position))
        }),
    };
    Some(Broadcast { range, radius, static_percent, cell: s.cell, world, exterior })
}

/// Where the one listening is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    /// The worldspace played, or inside an interior the one last walked in.
    pub world: u32,
    pub interior: Option<u32>,
    /// The position in `world`: outdoors the listener's own, inside an interior the last
    /// one outdoors.
    pub position: [f32; 3],
}

/// Whether a station is received where `l` is, and how much static there is then (0
/// clear … 1 only static). The range types [documented: GECK wiki Reference, Broadcast
/// Range Type: Everywhere "always available"; Worldspace "available at full strength
/// throughout the worldspace and connected interiors"; Connected interior cells "in all
/// connected interior cells", "only valid for station refs in interiors"; Radius "within
/// the specified radius", static "over this percentage of the radius"]. Where the wiki
/// says no more [design]:
/// - the interiors "connected" to a worldspace are those the listener walked into from
///   it; doors are not followed for Linked Interiors, which is the station's own cell;
/// - Current Cell outdoors is the 4096-unit grid square the station stands in;
/// - Radius is measured in three dimensions from the station's exterior position, from
///   the listener's last position outdoors while inside; static rises evenly from none
///   where the static part of the radius begins to all of it at the edge;
/// - a station in an interior without an exterior position reference is received in its
///   own cell only (Radius and Worldspace).
pub fn reception(b: &Broadcast, l: &Listener) -> Option<f32> {
    let own_cell = l.interior == Some(b.cell);
    let received = match b.range {
        BroadcastRange::Everywhere => true,
        BroadcastRange::WorldspaceAndLinkedInteriors => match b.world.or(b.exterior.map(|e| e.0)) {
            Some(w) => l.world == w || own_cell,
            None => own_cell,
        },
        BroadcastRange::LinkedInteriors => own_cell,
        BroadcastRange::CurrentCell => match b.world {
            None => own_cell,
            Some(w) => {
                let grid = |p: [f32; 3]| crate::cell_of(p[0], p[1]);
                l.interior.is_none() && l.world == w && b.exterior.is_some_and(|(_, p)| grid(p) == grid(l.position))
            }
        },
        BroadcastRange::Radius => {
            let Some((w, p)) = b.exterior else { return own_cell.then_some(0.0) };
            if l.world != w {
                return None;
            }
            let d = ((p[0] - l.position[0]).powi(2) + (p[1] - l.position[1]).powi(2) + (p[2] - l.position[2]).powi(2)).sqrt();
            if d > b.radius {
                return None;
            }
            let band = b.radius * (b.static_percent / 100.0).clamp(0.0, 1.0);
            let inner = b.radius - band;
            return Some(if band <= 0.0 || d <= inner { 0.0 } else { ((d - inner) / band).clamp(0.0, 1.0) });
        }
        BroadcastRange::Other(_) => false,
    };
    received.then_some(0.0)
}

/// An enum field as the decoder gives it: the schema's name of the value, or the number
/// when the schema has no name for it.
fn enum_code(v: &Value) -> u32 {
    match v.as_str() {
        Some("Radius") => 0,
        Some("Everywhere") => 1,
        Some("Worldspace and Linked Interiors") => 2,
        Some("Linked Interiors") => 3,
        Some("Current Cell Only") => 4,
        Some(_) => u32::MAX,
        None => v.as_u64().unwrap_or(0) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn station_flag_bits() {
        let s = Station { flags: 0x5002_0000, ..Station::default() };
        assert!(s.is_radio() && s.continuous());
        let s = Station { flags: 0x1002_0000, ..Station::default() };
        assert!(s.is_radio() && !s.continuous());
        assert!(!Station::default().is_radio());
    }

    #[test]
    fn non_pipboy_bit() {
        assert!(!Station { flags: 0x5002_0000, ..Station::default() }.on_pipboy());
        assert!(Station { flags: 0x4002_0000, ..Station::default() }.on_pipboy());
        assert!(!Station { flags: 0x4000_0000, ..Station::default() }.on_pipboy());
    }

    #[test]
    fn reception_by_range() {
        let out = |world, x: f32| Listener { world, interior: None, position: [x, 0.0, 0.0] };
        let inside = |world, cell| Listener { world, interior: Some(cell), position: [0.0; 3] };
        let b = |range, radius, static_percent, cell, world: Option<u32>, exterior| Broadcast { range, radius, static_percent, cell, world, exterior };
        // Everywhere: anywhere at all.
        let e = b(BroadcastRange::Everywhere, 0.0, 0.0, 7, None, None);
        assert_eq!(reception(&e, &inside(1, 99)), Some(0.0));
        // Radius 50,000 with 5 % static (Black Mountain Radio's numbers, radio.md §2).
        let r = b(BroadcastRange::Radius, 50_000.0, 5.0, 10, Some(1), Some((1, [0.0; 3])));
        assert_eq!(reception(&r, &out(1, 40_000.0)), Some(0.0));
        assert_eq!(reception(&r, &out(1, 47_500.0)), Some(0.0));
        assert!((reception(&r, &out(1, 48_750.0)).unwrap() - 0.5).abs() < 1e-4);
        assert_eq!(reception(&r, &out(1, 50_000.5)), None);
        assert_eq!(reception(&r, &out(2, 0.0)), None);
        // Inside, from the last position outdoors.
        assert_eq!(reception(&r, &Listener { world: 1, interior: Some(5), position: [1000.0, 0.0, 0.0] }), Some(0.0));
        // An interior station with an exterior position reference (Camp Guardian's shape).
        let g = b(BroadcastRange::Radius, 5000.0, 50.0, 20, None, Some((1, [10_000.0, 0.0, 0.0])));
        assert_eq!(reception(&g, &out(1, 0.0)), None);
        assert!((reception(&g, &out(1, 6250.0)).unwrap() - 0.5).abs() < 1e-4);
        // Without one, only its own cell.
        let n = b(BroadcastRange::Radius, 5000.0, 0.0, 20, None, None);
        assert_eq!(reception(&n, &inside(1, 20)), Some(0.0));
        assert_eq!(reception(&n, &out(1, 0.0)), None);
        // Worldspace and linked interiors: the worldspace, and interiors entered from it.
        let w = b(BroadcastRange::WorldspaceAndLinkedInteriors, 0.0, 0.0, 30, Some(1), Some((1, [0.0; 3])));
        assert_eq!(reception(&w, &out(1, 1e6)), Some(0.0));
        assert_eq!(reception(&w, &inside(1, 5)), Some(0.0));
        assert_eq!(reception(&w, &out(2, 0.0)), None);
        // Linked interiors: the station's own cell.
        let li = b(BroadcastRange::LinkedInteriors, 0.0, 0.0, 40, None, None);
        assert_eq!(reception(&li, &inside(1, 40)), Some(0.0));
        assert_eq!(reception(&li, &inside(1, 41)), None);
        // Current cell outdoors: the grid square.
        let c = b(BroadcastRange::CurrentCell, 0.0, 0.0, 50, Some(1), Some((1, [100.0, 100.0, 0.0])));
        assert_eq!(reception(&c, &out(1, 4000.0)), Some(0.0));
        assert_eq!(reception(&c, &out(1, 4100.0)), None);
        assert_eq!(reception(&b(BroadcastRange::Other(9), 0.0, 0.0, 1, None, None), &inside(1, 1)), None);
    }

    #[test]
    fn range_codes() {
        assert_eq!(BroadcastRange::of(enum_code(&json!("Everywhere"))), BroadcastRange::Everywhere);
        assert_eq!(BroadcastRange::of(enum_code(&json!("Current Cell Only"))), BroadcastRange::CurrentCell);
        assert_eq!(BroadcastRange::of(enum_code(&json!(4))), BroadcastRange::CurrentCell);
        assert_eq!(BroadcastRange::of(enum_code(&json!(7))), BroadcastRange::Other(7));
        assert_eq!(BroadcastRange::of(9), BroadcastRange::Other(9));
    }
}
