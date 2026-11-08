//! The plugin the editor saves: overrides of placed references, new references and new
//! static objects. Every other record stays as its plugin has it. Its masters are the load
//! order before it, so a FormID in the file is the same as the load-order (global) FormID.
//! Format facts: `docs/spec-notes/esm.md`.

use crate::index::WorldIndex;
use crate::cell_of;
use bytes::Bytes;
use fnv_esm::decode::formid_offsets;
use fnv_esm::raw::{group_type, parse_subrecords, record_flags, visit_records, write_nodes, write_subrecords};
use fnv_esm::{Group, GroupHeader, LoadOrder, Node, Record, RecordHeader, Schema, Subrecord};
use std::collections::BTreeMap;

/// The file name of the plugin. It loads after every other plugin.
pub const EDIT_PLUGIN: &str = "NVForeverEdits.esp";
/// The first FormID low part the file gives to a new record: lower ones are the engine's.
pub const FIRST_OBJECT_ID: u32 = 0x800;

const FORM_VERSION: u16 = 15;
const HEADER_VERSION: f32 = 1.34;
const EXTERIOR_BLOCK_CELLS: i32 = 32;
const EXTERIOR_SUBBLOCK_CELLS: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub position: [f32; 3],
    /// Euler angles in radians, as stored (`crate::transform`).
    pub rotation: [f32; 3],
    pub scale: f32,
}

/// A change to a reference that a plugin in the load order defines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefChange {
    pub pose: Pose,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewRef {
    pub form_id: u32,
    pub base: u32,
    pub pose: Pose,
    /// The worldspace it stands in; its grid square comes from its position.
    pub world: u32,
    /// The interior cell it stands in, instead of the worldspace.
    pub interior: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewStatic {
    pub form_id: u32,
    pub edid: String,
    /// As MODL stores it: relative to `meshes\`.
    pub model: String,
}

#[derive(Default)]
pub struct Edits {
    pub changes: BTreeMap<u32, RefChange>,
    pub new_refs: Vec<NewRef>,
    pub new_statics: Vec<NewStatic>,
    pub next_object_id: u32,
}

/// The stored Euler angles of a rotation matrix, row-major as `crate::transform`: the
/// inverse of `ref_rotation`, whose matrix is `Rx(-x) Ry(-y) Rz(-z)`.
pub fn rotation_from_matrix(m: [f32; 9]) -> [f32; 3] {
    let y = m[2].clamp(-1.0, 1.0).asin();
    [-(-m[5]).atan2(m[8]), -y, -(-m[1]).atan2(m[0])]
}

/// Where the plugin stands in `lo`, when `lo` has it.
pub fn own_index(lo: &LoadOrder) -> Option<usize> {
    lo.plugins.iter().position(|p| p.name.eq_ignore_ascii_case(EDIT_PLUGIN))
}

/// A FormID for a record of the plugin that `lo` will load last.
pub fn new_form_id(lo: &LoadOrder, low: u32) -> u32 {
    ((own_index(lo).unwrap_or(lo.plugins.len()) as u32) << 24) | (low & 0x00FF_FFFF)
}

/// The first free FormID low part: past the plugin's saved records.
pub fn first_free_object_id(lo: &LoadOrder) -> u32 {
    own_index(lo).map_or(FIRST_OBJECT_ID, |i| lo.plugins[i].header.next_object_id.max(FIRST_OBJECT_ID))
}

fn plugin_index<'a>(lo: &LoadOrder<'a>, global: u32) -> Option<(usize, &'a Record)> {
    let (plugin, record) = lo.winner(global)?;
    Some((lo.plugins.iter().position(|p| std::ptr::eq(p, plugin))?, record))
}

fn local_is_global(lo: &LoadOrder, plugin: usize) -> bool {
    let masters = lo.master_map(plugin);
    masters.len() == plugin && masters.iter().enumerate().all(|(i, &m)| m as usize == i)
}

/// `record` of `plugin`, with its FormIDs written as the new plugin reads them.
fn global_copy(lo: &LoadOrder, schema: &Schema, plugin: usize, record: &Record, global: u32) -> Result<Record, String> {
    let mut header = record.header.clone();
    header.form_id = global;
    if local_is_global(lo, plugin) {
        return Ok(Record { header, raw: record.raw.clone(), offset: 0 });
    }
    let mut subs = subrecords(record).map_err(|e| format!("{global:08X}: {e}"))?;
    for s in &mut subs {
        let offsets = formid_offsets(&record.header.sig, s, schema)
            .map_err(|e| format!("{global:08X} of {}: {e}", lo.plugins[plugin].name))?;
        for at in offsets {
            let bytes: [u8; 4] = s.data[at..at + 4].try_into().map_err(|_| "a FormID past the subrecord's end")?;
            let mapped = lo.to_global(plugin, u32::from_le_bytes(bytes)).ok_or("a FormID with no plugin")?;
            s.data[at..at + 4].copy_from_slice(&mapped.to_le_bytes());
        }
    }
    header.flags &= !record_flags::COMPRESSED;
    Ok(Record { header, raw: Bytes::from(write_subrecords(&subs)), offset: 0 })
}

fn subrecords(record: &Record) -> Result<Vec<Subrecord>, String> {
    let payload = record.payload().map_err(|e| e.to_string())?;
    parse_subrecords(&payload).map_err(|(at, e)| format!("subrecord at {at}: {e}"))
}

fn pose_data(p: &Pose) -> Vec<u8> {
    p.position.iter().chain(&p.rotation).flat_map(|v| v.to_le_bytes()).collect()
}

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn zstring(s: &str) -> Vec<u8> {
    s.bytes().chain([0]).collect()
}

fn record(sig: &[u8; 4], flags: u32, form_id: u32, subs: &[Subrecord]) -> Record {
    let raw = Bytes::from(write_subrecords(subs));
    let header = RecordHeader { sig: *sig, data_size: raw.len() as u32, flags, form_id, vc_info: 0, form_version: FORM_VERSION, vc_info2: 0 };
    Record { header, raw, offset: 0 }
}

fn group(label: [u8; 4], kind: i32, children: Vec<Node>) -> Node {
    let header = GroupHeader { size: 0, label, group_type: kind, stamp: 0, unknown1: 0, version: 0, unknown2: 0 };
    Node::Group(Group { header, children, offset: 0 })
}

/// A reference with the pose and the deleted flag of `change`.
fn changed_ref(mut copy: Record, change: &RefChange) -> Result<Record, String> {
    let mut subs = subrecords(&copy)?;
    let data = subs.iter().position(|s| &s.sig == b"DATA").ok_or_else(|| format!("{:08X}: a reference with no DATA", copy.header.form_id))?;
    subs[data].data = pose_data(&change.pose);
    let scale = subs.iter().position(|s| &s.sig == b"XSCL");
    match (scale, change.pose.scale == 1.0) {
        (Some(i), true) => {
            subs.remove(i);
        }
        (Some(i), false) => subs[i].data = change.pose.scale.to_le_bytes().to_vec(),
        (None, false) => subs.insert(data, sub(b"XSCL", change.pose.scale.to_le_bytes().to_vec())),
        (None, true) => {}
    }
    copy.header.flags &= !record_flags::COMPRESSED;
    if change.deleted {
        copy.header.flags |= record_flags::DELETED;
    } else {
        copy.header.flags &= !record_flags::DELETED;
    }
    copy.raw = Bytes::from(write_subrecords(&subs));
    Ok(copy)
}

/// Cell nodes by exterior block and sub-block.
type GridBlocks = BTreeMap<(i32, i32), BTreeMap<(i32, i32), Vec<Node>>>;

#[derive(Default)]
struct Children {
    persistent: Vec<Node>,
    temporary: Vec<Node>,
}

fn grid_label(x: i32, y: i32) -> [u8; 4] {
    let (x, y) = ((x as i16).to_le_bytes(), (y as i16).to_le_bytes());
    [y[0], y[1], x[0], x[1]]
}

/// The CELL record and its children groups.
fn cell_nodes(lo: &LoadOrder, schema: &Schema, cell: u32, children: Children) -> Result<Vec<Node>, String> {
    let (plugin, source) = plugin_index(lo, cell).ok_or_else(|| format!("no record for cell {cell:08X}"))?;
    let label = cell.to_le_bytes();
    let mut groups = Vec::new();
    if !children.persistent.is_empty() {
        groups.push(group(label, group_type::CELL_PERSISTENT_CHILDREN, children.persistent));
    }
    if !children.temporary.is_empty() {
        groups.push(group(label, group_type::CELL_TEMPORARY_CHILDREN, children.temporary));
    }
    Ok(vec![Node::Record(global_copy(lo, schema, plugin, source, cell)?), group(label, group_type::CELL_CHILDREN, groups)])
}

/// The bytes of the plugin: `edits`, and what the saved plugin in `lo` already holds.
pub fn build(lo: &LoadOrder, schema: &Schema, index: &WorldIndex, edits: &Edits) -> Result<Vec<u8>, String> {
    let own = own_index(lo);
    let masters = &lo.plugins[..own.unwrap_or(lo.plugins.len())];
    let mut cells: BTreeMap<u32, Children> = BTreeMap::new();

    for (&id, entry) in &index.refs {
        let change = edits.changes.get(&id);
        let (plugin, source) = plugin_index(lo, id).ok_or_else(|| format!("no record for reference {id:08X}"))?;
        if change.is_none() && Some(plugin) != own {
            continue;
        }
        let copy = global_copy(lo, schema, plugin, source, id)?;
        let (node, target) = match change {
            Some(change) => {
                let moved_to = (!entry.persistent)
                    .then(|| index.cells.get(&entry.cell))
                    .flatten()
                    .and_then(|c| c.world)
                    .and_then(|w| index.worldspaces.get(&w))
                    .and_then(|w| w.grid.get(&cell_of(change.pose.position[0], change.pose.position[1])));
                (changed_ref(copy, change)?, moved_to.copied().unwrap_or(entry.cell))
            }
            None => (copy, entry.cell),
        };
        let children = cells.entry(target).or_default();
        if entry.persistent { &mut children.persistent } else { &mut children.temporary }.push(Node::Record(node));
    }
    for r in &edits.new_refs {
        let cell = match r.interior {
            Some(cell) => cell,
            None => {
                let world = index.worldspaces.get(&r.world).ok_or_else(|| format!("no worldspace {:08X}", r.world))?;
                let grid = cell_of(r.pose.position[0], r.pose.position[1]);
                *world.grid.get(&grid).ok_or_else(|| format!("no cell at {}, {}: a new object needs one", grid.0, grid.1))?
            }
        };
        let mut subs = vec![sub(b"NAME", r.base.to_le_bytes().to_vec())];
        if r.pose.scale != 1.0 {
            subs.push(sub(b"XSCL", r.pose.scale.to_le_bytes().to_vec()));
        }
        subs.push(sub(b"DATA", pose_data(&r.pose)));
        cells.entry(cell).or_default().temporary.push(Node::Record(record(b"REFR", 0, r.form_id, &subs)));
    }

    let mut statics = Vec::new();
    if let Some(own) = own {
        visit_records(&lo.plugins[own].nodes[1..], &mut |r, _| {
            if &r.header.sig == b"STAT" {
                statics.push(Node::Record(r.clone()));
            }
        });
    }
    for s in &edits.new_statics {
        let subs = [sub(b"EDID", zstring(&s.edid)), sub(b"OBND", vec![0; 12]), sub(b"MODL", zstring(&s.model))];
        statics.push(Node::Record(record(b"STAT", 0, s.form_id, &subs)));
    }

    let mut interiors: BTreeMap<u32, BTreeMap<u32, Vec<Node>>> = BTreeMap::new();
    struct World {
        persistent: Vec<Node>,
        blocks: GridBlocks,
    }
    let mut worlds: BTreeMap<u32, World> = BTreeMap::new();
    for (id, children) in cells {
        let cell = index.cells.get(&id).ok_or_else(|| format!("no cell {id:08X}"))?;
        let nodes = cell_nodes(lo, schema, id, children)?;
        match cell.world {
            None => interiors.entry(id % 10).or_default().entry((id / 10) % 10).or_default().extend(nodes),
            Some(w) => {
                let world = worlds.entry(w).or_insert_with(|| World { persistent: Vec::new(), blocks: BTreeMap::new() });
                let persistent = index.worldspaces.get(&w).and_then(|s| s.persistent_cell) == Some(id);
                match cell.grid.filter(|_| !persistent) {
                    None => world.persistent.extend(nodes),
                    Some((x, y)) => {
                        let block = (x.div_euclid(EXTERIOR_BLOCK_CELLS), y.div_euclid(EXTERIOR_BLOCK_CELLS));
                        let sub = (x.div_euclid(EXTERIOR_SUBBLOCK_CELLS), y.div_euclid(EXTERIOR_SUBBLOCK_CELLS));
                        world.blocks.entry(block).or_default().entry(sub).or_default().extend(nodes);
                    }
                }
            }
        }
    }

    let mut top = Vec::new();
    if !statics.is_empty() {
        top.push(group(*b"STAT", group_type::TOP, statics));
    }
    if !interiors.is_empty() {
        let blocks = interiors.into_iter().map(|(b, subs)| {
            let subs = subs.into_iter().map(|(s, n)| group(s.to_le_bytes(), group_type::INTERIOR_CELL_SUBBLOCK, n)).collect();
            group(b.to_le_bytes(), group_type::INTERIOR_CELL_BLOCK, subs)
        });
        top.push(group(*b"CELL", group_type::TOP, blocks.collect()));
    }
    if !worlds.is_empty() {
        let mut nodes = Vec::new();
        for (id, world) in worlds {
            let (plugin, source) = plugin_index(lo, id).ok_or_else(|| format!("no record for worldspace {id:08X}"))?;
            nodes.push(Node::Record(global_copy(lo, schema, plugin, source, id)?));
            let mut children = world.persistent;
            for ((bx, by), subs) in world.blocks {
                let subs = subs.into_iter().map(|((sx, sy), n)| group(grid_label(sx, sy), group_type::EXTERIOR_CELL_SUBBLOCK, n)).collect();
                children.push(group(grid_label(bx, by), group_type::EXTERIOR_CELL_BLOCK, subs));
            }
            nodes.push(group(id.to_le_bytes(), group_type::WORLD_CHILDREN, children));
        }
        top.push(group(*b"WRLD", group_type::TOP, nodes));
    }

    let mut groups = 0i32;
    fnv_esm::raw::visit_groups(&top, &mut |_, _| groups += 1);
    let mut records = 0i32;
    visit_records(&top, &mut |_, _| records += 1);
    let mut hedr = HEADER_VERSION.to_le_bytes().to_vec();
    hedr.extend((groups + records).to_le_bytes());
    hedr.extend(edits.next_object_id.max(first_free_object_id(lo)).to_le_bytes());
    let mut header = vec![sub(b"HEDR", hedr), sub(b"CNAM", zstring("NVForever Editor"))];
    for m in masters {
        header.push(sub(b"MAST", zstring(&m.name)));
        header.push(sub(b"DATA", 0u64.to_le_bytes().to_vec()));
    }
    let mut out = Vec::new();
    write_nodes(&[Node::Record(record(b"TES4", 0, 0, &header))], &mut out);
    write_nodes(&top, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placed::{base_object, placement};
    use crate::transform::ref_rotation;
    use fnv_esm::Plugin;

    fn schema() -> Schema {
        Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()
    }

    #[test]
    fn rotation_round_trip() {
        for angles in [[0.0, 0.0, 0.0], [0.3, -0.7, 1.2], [-1.1, 0.4, -2.9], [0.0, 1.5, 0.5]] {
            let got = rotation_from_matrix(ref_rotation(angles));
            let (a, b) = (ref_rotation(angles), ref_rotation(got));
            for i in 0..9 {
                assert!((a[i] - b[i]).abs() < 1e-5, "{angles:?} gave {got:?}");
            }
        }
    }

    /// A master with one worldspace, one exterior cell at 1, 2 and one reference.
    fn master() -> Plugin {
        let world = record(b"WRLD", 0, 0x100, &[sub(b"EDID", zstring("TestWorld"))]);
        let mut xclc = Vec::new();
        xclc.extend(1i32.to_le_bytes());
        xclc.extend(2i32.to_le_bytes());
        xclc.extend([0u8; 4]);
        let cell = record(b"CELL", 0, 0x200, &[sub(b"EDID", zstring("TestCell")), sub(b"XCLC", xclc)]);
        let stat = record(b"STAT", 0, 0x300, &[sub(b"EDID", zstring("TestRock")), sub(b"MODL", zstring("rocks\\a.nif"))]);
        let pose = Pose { position: [5000.0, 9000.0, 10.0], rotation: [0.0; 3], scale: 1.0 };
        let reference = record(b"REFR", 0, 0x400, &[sub(b"NAME", 0x300u32.to_le_bytes().to_vec()), sub(b"DATA", pose_data(&pose))]);
        let temp = group(0x200u32.to_le_bytes(), group_type::CELL_TEMPORARY_CHILDREN, vec![Node::Record(reference)]);
        let children = group(0x200u32.to_le_bytes(), group_type::CELL_CHILDREN, vec![temp]);
        let sub_block = group(grid_label(0, 0), group_type::EXTERIOR_CELL_SUBBLOCK, vec![Node::Record(cell), children]);
        let block = group(grid_label(0, 0), group_type::EXTERIOR_CELL_BLOCK, vec![sub_block]);
        let world_children = group(0x100u32.to_le_bytes(), group_type::WORLD_CHILDREN, vec![block]);
        let top = [
            group(*b"STAT", group_type::TOP, vec![Node::Record(stat)]),
            group(*b"WRLD", group_type::TOP, vec![Node::Record(world), world_children]),
        ];
        let mut hedr = HEADER_VERSION.to_le_bytes().to_vec();
        hedr.extend(0i32.to_le_bytes());
        hedr.extend(FIRST_OBJECT_ID.to_le_bytes());
        let mut bytes = Vec::new();
        write_nodes(&[Node::Record(record(b"TES4", record_flags::MASTER, 0, &[sub(b"HEDR", hedr)]))], &mut bytes);
        write_nodes(&top, &mut bytes);
        Plugin::parse(&bytes, "Test.esm".to_string()).unwrap()
    }

    fn load(bytes: &[u8]) -> Vec<Plugin> {
        vec![master(), Plugin::parse(bytes, EDIT_PLUGIN.to_string()).unwrap()]
    }

    #[test]
    fn saved_plugin_overrides_and_adds() {
        let schema = schema();
        let plugins = [master()];
        let lo = LoadOrder::new(&plugins).unwrap();
        let index = WorldIndex::build(&lo, &schema);
        let moved = Pose { position: [5100.0, 9000.0, 20.0], rotation: [0.0, 0.0, 1.0], scale: 2.0 };
        let new_id = new_form_id(&lo, FIRST_OBJECT_ID);
        let base_id = new_form_id(&lo, FIRST_OBJECT_ID + 1);
        let edits = Edits {
            changes: BTreeMap::from([(0x400, RefChange { pose: moved, deleted: false })]),
            new_refs: vec![NewRef { form_id: new_id, base: base_id, pose: moved, world: 0x100, interior: None }],
            new_statics: vec![NewStatic { form_id: base_id, edid: "NewRock".into(), model: "rocks\\b.nif".into() }],
            next_object_id: FIRST_OBJECT_ID + 2,
        };
        let bytes = build(&lo, &schema, &index, &edits).unwrap();

        let plugins = load(&bytes);
        let lo = LoadOrder::new(&plugins).unwrap();
        let index = WorldIndex::build(&lo, &schema);
        assert!(index.errors.is_empty(), "{:?}", index.errors);
        assert!(index.orphan_groups.is_empty(), "{:?}", index.orphan_groups);
        let p = placement(&lo, &schema, 0x400).unwrap();
        assert_eq!((p.position, p.rotation, p.scale, p.base), (moved.position, moved.rotation, 2.0, Some(0x300)));
        let n = placement(&lo, &schema, new_id).unwrap();
        assert_eq!((n.position, n.base), (moved.position, Some(base_id)));
        assert_eq!(base_object(&lo, &schema, base_id).unwrap().model.as_deref(), Some("rocks\\b.nif"));
        assert!(index.cells[&0x200].refs.contains(&new_id));
        assert_eq!(index.worldspaces[&0x100].grid[&(1, 2)], 0x200);
    }

    #[test]
    fn second_save_keeps_the_first() {
        let schema = schema();
        let plugins = [master()];
        let lo = LoadOrder::new(&plugins).unwrap();
        let index = WorldIndex::build(&lo, &schema);
        let moved = Pose { position: [5100.0, 9000.0, 20.0], rotation: [0.0; 3], scale: 1.0 };
        let new_id = new_form_id(&lo, FIRST_OBJECT_ID);
        let first = Edits {
            changes: BTreeMap::from([(0x400, RefChange { pose: moved, deleted: false })]),
            new_refs: vec![NewRef { form_id: new_id, base: 0x300, pose: moved, world: 0x100, interior: None }],
            new_statics: Vec::new(),
            next_object_id: FIRST_OBJECT_ID + 1,
        };
        let bytes = build(&lo, &schema, &index, &first).unwrap();

        let plugins = load(&bytes);
        let lo = LoadOrder::new(&plugins).unwrap();
        let index = WorldIndex::build(&lo, &schema);
        assert_eq!(first_free_object_id(&lo), FIRST_OBJECT_ID + 1);
        let bytes = build(&lo, &schema, &index, &Edits::default()).unwrap();

        let plugins = load(&bytes);
        let lo = LoadOrder::new(&plugins).unwrap();
        assert_eq!(placement(&lo, &schema, 0x400).unwrap().position, moved.position);
        assert_eq!(placement(&lo, &schema, new_id).unwrap().base, Some(0x300));
    }

    #[test]
    fn deleted_reference_is_flagged() {
        let schema = schema();
        let plugins = [master()];
        let lo = LoadOrder::new(&plugins).unwrap();
        let index = WorldIndex::build(&lo, &schema);
        let pose = placement(&lo, &schema, 0x400).map(|p| Pose { position: p.position, rotation: p.rotation, scale: p.scale }).unwrap();
        let edits = Edits { changes: BTreeMap::from([(0x400, RefChange { pose, deleted: true })]), ..Edits::default() };
        let bytes = build(&lo, &schema, &index, &edits).unwrap();
        let plugins = load(&bytes);
        let lo = LoadOrder::new(&plugins).unwrap();
        assert!(placement(&lo, &schema, 0x400).unwrap().deleted());

        // A later save can bring it back.
        let index = WorldIndex::build(&lo, &schema);
        let edits = Edits { changes: BTreeMap::from([(0x400, RefChange { pose, deleted: false })]), ..Edits::default() };
        let bytes = build(&lo, &schema, &index, &edits).unwrap();
        let plugins = load(&bytes);
        let lo = LoadOrder::new(&plugins).unwrap();
        assert!(!placement(&lo, &schema, 0x400).unwrap().deleted());
    }

    /// With the real game: overrides in every plugin keep their references' meaning.
    /// Run with `NVF_TEST_DATA=<game Data folder> cargo test --release -- --ignored`.
    #[test]
    #[ignore = "needs the game's data"]
    fn real_game_round_trip() {
        let data = std::path::PathBuf::from(std::env::var("NVF_TEST_DATA").unwrap());
        let names = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm", "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm", "CaravanPack.esm"];
        let plugins: Vec<Plugin> = names.iter().map(|n| Plugin::open(data.join(n)).unwrap()).collect();
        let schema = schema();
        let lo = LoadOrder::new(&plugins).unwrap();
        let index = WorldIndex::build(&lo, &schema);
        let (mut changes, mut per_plugin) = (BTreeMap::new(), vec![0usize; plugins.len()]);
        let mut ids: Vec<u32> = index.refs.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let (plugin, _) = plugin_index(&lo, id).unwrap();
            let p = placement(&lo, &schema, id).unwrap();
            if per_plugin[plugin] == 25 || p.deleted() {
                continue;
            }
            per_plugin[plugin] += 1;
            let pose = Pose { position: [p.position[0] + 8.0, p.position[1], p.position[2]], rotation: p.rotation, scale: p.scale };
            changes.insert(id, RefChange { pose, deleted: false });
        }
        // A rock at Goodsprings, floated and doubled, for a look at the plugin in the game:
        // `NVF_TEST_OUT=<file>` keeps the plugin.
        let rock = placement(&lo, &schema, 0x0010758C).unwrap();
        let floated = Pose { position: [rock.position[0], rock.position[1], rock.position[2] + 600.0], rotation: rock.rotation, scale: 2.0 };
        changes.insert(0x0010758C, RefChange { pose: floated, deleted: false });
        let edits = Edits { changes: changes.clone(), ..Edits::default() };
        let bytes = build(&lo, &schema, &index, &edits).unwrap();
        if let Ok(out) = std::env::var("NVF_TEST_OUT") {
            std::fs::write(out, &bytes).unwrap();
        }

        let mut with_edits: Vec<Plugin> = names.iter().map(|n| Plugin::open(data.join(n)).unwrap()).collect();
        with_edits.push(Plugin::parse(&bytes, EDIT_PLUGIN.to_string()).unwrap());
        let after = LoadOrder::new(&with_edits).unwrap();
        let after_index = WorldIndex::build(&after, &schema);
        assert!(after_index.errors.is_empty(), "{:?}", after_index.errors);
        for (id, change) in &changes {
            let (old, new) = (placement(&lo, &schema, *id).unwrap(), placement(&after, &schema, *id).unwrap());
            assert_eq!(new.position, change.pose.position, "{id:08X}");
            assert_eq!(new.base, old.base, "{id:08X}: the base changed");
            assert_eq!(new.enable_parent, old.enable_parent, "{id:08X}: the enable parent changed");
            assert_eq!(new.sig, old.sig, "{id:08X}");
        }
        for (id, cell) in &index.cells {
            assert_eq!(after_index.cells[id].edid, cell.edid, "{id:08X}");
            assert_eq!(after_index.cells[id].grid, cell.grid, "{id:08X}");
            assert_eq!(after_index.cells[id].encounter_zone, cell.encounter_zone, "{id:08X}");
            assert_eq!(after_index.cells[id].water_type, cell.water_type, "{id:08X}");
        }
        println!("saved {} bytes for {} references, per plugin {per_plugin:?}", bytes.len(), changes.len());
    }
}
