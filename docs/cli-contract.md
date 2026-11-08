# fnvtool CLI contract

Every `fnvtool` command, from Layer 1 on; the sections follow the layers, and the commands
added without a section of their own are listed at the end.

The parity lab runs `fnvtool` and compares its stdout (one JSON document) with oracle
goldens of the **same schema**. Any oracle tool (xEdit export script, independent
reader) must emit these shapes. Exit code 0 = success, 1 = input could not be processed,
2 = usage error (`--help` exits 0); diagnostics go to stderr. `fnvtool --help` and
`fnvtool <group> <command> --help` describe every option.

Conventions: FormIDs are strings `"0x%08X"` (upper-case hex) and are **file-local**
(exactly as stored in the plugin, not load-order adjusted). Text is decoded from
Windows-1252 to UTF-8. Missing optional values are `null`.

## `fnvtool esm inventory <plugin>` → `fnv.esm.inventory/1`

```json
{
  "schema": "fnv.esm.inventory/1",
  "plugin": "FalloutNV.esm",
  "header": {"plugin": "...", "is_master": true, "flags": 1, "version": 1.34,
             "num_records": 542016, "next_object_id": "0xFF17BA48", "author": "...",
             "description": null, "masters": [], "overridden_forms": 0,
             "other_subrecords": [], "file_size": 245650747},
  "errors": [],
  "records": [
    {"formid": "0x0017B37C", "type": "GMST", "edid": "sPoisonNoWeaponMessage",
     "full": null, "flags": 0, "form_version": 15}
  ]
}
```

`records` lists every record except TES4, in file order. `header.version` is the HEDR
f32 widened to a double without rounding (e.g. 1.340000033378601). `overridden_forms` is
the number of u32 entries in ONAM (0 when absent). `errors` entries are
`{"code": ..., "detail": ...}`; they are diagnostics and not compared. `edid`/`full` are the first
`EDID`/`FULL` subrecord read as NUL-terminated text. `flags` is the raw record-header
flags dword (including the compression bit 0x40000).

Lab comparator: **keyed** on `formid`, `select: "/records"`, fields
`["type", "edid", "full", "flags", "form_version"]` (the xEdit cross-check compares only
`type`, `edid`, `full`). Header compared separately with **exact** on `/header`,
including `file_size`.

## `fnvtool esm stats <plugin>` → `fnv.esm.stats/1`

```json
{"schema": "fnv.esm.stats/1", "plugin": "...", "records": 465017, "groups": 77000,
 "compressed_records": 33179, "deleted_records": 0, "hedr_num_records": 542016,
 "hedr_matches_records_plus_groups_minus_one": true, "top_groups": ["GMST", "..."],
 "by_type": {"ACHR": 3386, "...": 0}, "by_group_type": {"top": 101, "...": 0}}
```

`records` and `by_type` include TES4. `by_group_type` always lists all 11 group-type
names, in the order below, including zero counts; an out-of-range type N is counted under `unknown_N`. Group-type names: `top, world_children, interior_cell_block,
interior_cell_subblock, exterior_cell_block, exterior_cell_subblock, cell_children,
topic_children, cell_persistent_children, cell_temporary_children,
cell_visible_distant_children`.

## `fnvtool bsa list <archive> [--no-content-hash]` → `fnv.bsa.listing/1`

```json
{
  "schema": "fnv.bsa.listing/1",
  "archive": "Update.bsa",
  "header": {"version": 104, "archive_flags": 131, "file_flags": 283, "folder_count": 22,
             "file_count": 86, "total_folder_name_length": 645, "total_file_name_length": 1900},
  "warnings": [{"code": "...", "detail": "..."}],
  "errors": [],
  "files": [
    {"path": "meshes\\creatures\\nvsecuritron\\death.kf", "name_hash": "0x1776fd25640574e8",
     "folder_hash": "0x...", "stored_size": 2199, "compressed": false,
     "size": 2199, "sha256": "..."}
  ]
}
```

`path` is `folder\name` as stored (lower case, backslashes); when the folder name is
empty the path is the bare file name. `header.file_flags` is the low u16 only (the two
following bytes are not part of the contract). `size`/`sha256` describe the
decoded (decompressed) contents and are omitted with `--no-content-hash`. Lab comparator:
**keyed** on `path`, fields `["stored_size", "compressed", "size", "sha256", "name_hash"]`.

## Other commands (not yet scenarios)

| Command | Schema | Purpose |
|---|---|---|
| `bsa verify <archive>` | `fnv.bsa.verify/1` | byte-identical rebuild + decode every file |
| `bsa extract <archive> <path> <out>` | `fnv.bsa.extract/1` | write one decoded file |
| `bsa check <archive>…` | `fnv.bsa.check/1` | the game's start-up check (header, folder and file tables, data inside the file; nothing decompressed); fails naming each damaged archive |
| `esm header <plugin>` | (header object above) | TES4 summary |
| `esm verify <plugin>` | `fnv.esm.verify/1` | byte-identical rebuild, decompression, subrecord round-trip, zlib trailer anomalies |
| `esm coverage <plugin> --schema <dir>` | `fnv.esm.coverage/1` | schema decode coverage + ranked work queue |
| `esm decode <plugin> --schema <dir> --formid X \| --edid E` | `fnv.esm.decode/1` | schema-decoded record(s) |
| `esm loadorder <data-dir> <plugins…> [--schema <dir>]` | `fnv.esm.loadorder/1` | masters, overrides, injected forms, ONAM check, FormID-reference resolution |
| `esm survey <data-dir> <plugins…> [--plugin P]` | `fnv.esm.survey/1` | per (record, subrecord) size/string/FormID statistics |
| `esm fields <data-dir> <plugins…> --schema <dir>` | `fnv.esm.fields/1` | per record type, subrecord and schema field path: present / non-zero counts, top values, flag bits set |

### Top-level keys of the other documents

- `fnv.bsa.verify/1`: `archive`, `files`, `roundtrip_identical` (bool), `original_size`,
  `rewritten_size`, `first_difference` (byte offset or null), `decode_errors` [string],
  `warnings` [{code, detail}] (includes `zlib_trailer_missing`, `zlib_checksum_mismatch`,
  `zlib_bytes_after_trailer`,
  `data_not_contiguous`, `trailing_bytes`, `noncanonical_folder_name`, hash mismatches).
- `fnv.esm.verify/1`: `plugin`, `roundtrip_identical`, `first_difference`, `records`,
  `compressed_records`, `extended_size_subrecords`, `decompress_errors` [string],
  `zlib_trailer_anomalies` [string] (a missing or wrong Adler-32, or `Extra { bytes }` after
  a correct one), `subrecord_errors` [string] (max 50),
  `payload_roundtrip_mismatches` [string] (max 50).
- `fnv.esm.coverage/1`: `plugin`; `summary` {subrecords, ok, unknown, trailing, error,
  ok_percent, opaque_bytes, distinct_pairs, pairs_fully_decoded}; `work_queue` [{record,
  sub, total, ok, unknown, trailing, error, top_sizes [{size, count}], first_error?}]
  sorted by undecoded instances; `record_errors` [string].
- `fnv.esm.decode/1`: `plugin`, `records` [{formid, type, type_name?, flags,
  form_version, subrecords [{sig, name?, status, value | raw (hex), error?}]}].
- `fnv.esm.loadorder/1`: `plugins` [{plugin, load_index, records, new_forms, overrides,
  unmappable_formids [formid], onam_entries, onam_entries_that_are_overrides,
  temporary_group_overrides, temporary_group_overrides_in_onam, injected_forms_by_type,
  overrides_by_type, overrides_not_in_onam_by_type}], `unique_global_forms`,
  `definitions_per_form` {count: forms}; with `--schema` also `references` {formid_fields,
  null, resolved, bad_master_index, unresolved_by_kind, unresolved_examples}.
- `fnv.esm.survey/1`: `records` {type: count}, `pairs` [{record, sub, count,
  distinct_sizes, top_sizes, zstring_like, formid_array_like, first_u32_formid,
  max_per_record}], `errors`.
- `fnv.esm.fields/1`: `plugins` [name]; `records` {type: {count, name, header_bits
  {"bit[ name]": count}, plugins}}; `fields` [{path (`REC/SUB[/field…]`, arrays as `[]`),
  present, nonzero, distinct (a number, or ">256"), top [[value, count]] (6 at most),
  plugins (those with a non-zero value), sub_name?, status? {ok, unknown, trailing,
  error} (subrecord rows), bits? {name: count}, unknown_bits? {bit: count}}]. A
  subrecord the schema does not know counts its raw size as its value. Every record of
  every plugin is counted, overrides included.

# Layer 2 (meshes, animations, textures)

Inputs are BSA archives; every `.nif` / `.kf` (or `.dds`) file inside is reported, in
archive order. Paths are as stored in the archive (lower case, backslashes). Floats are
f32 values widened to JSON numbers without rounding; NaN and infinities are `null`
(two node translations in the game are NaN). -0.0 and 0.0 compare equal. Block references
are block indices. `error` / `reason` texts are diagnostics: tools should agree on whether
one is present, and the lab compares them, but none occur in the game data.

## `fnvtool nif stats <archive>` → `fnv.nif.stats/1`

```json
{
  "schema": "fnv.nif.stats/1",
  "archive": "Fallout - Meshes.bsa",
  "files": [
    {"path": "meshes\clutter\x.nif", "version": "20.2.0.7", "user_version": 11,
     "bs_version": 34, "num_blocks": 12, "strings": 9,
     "block_types": {"BSFadeNode": 1, "NiTriStrips": 2, "...": 0}, "error": null}
  ]
}
```

Everything here comes from the file header (docs/spec-notes/nif.md §2), so it is defined
for every version. `strings` is the string-table entry count (`null` for versions without
a string table). `block_types` counts blocks per type name (keys sorted). `error` is a
message when the header cannot be read, and then the other fields are `null`. Lab
comparator: **keyed** on `path`, fields `["version", "user_version", "bs_version",
"num_blocks", "strings", "block_types", "error"]`.

## `fnvtool nif scene <archive>` → `fnv.nif.scene/1`

```json
{
  "schema": "fnv.nif.scene/1",
  "archive": "Fallout - Meshes.bsa",
  "skipped": [{"path": "...", "reason": "no block-size table (version 20.0.0.4)"}],
  "objects": [
    {"key": "meshes\clutter\x.nif#0", "type": "BSFadeNode", "name": "x", "parent": null,
     "translation": [0.0, 0.0, 0.0], "rotation": [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
     "scale": 1.0}
  ],
  "shapes": [
    {"key": "meshes\clutter\x.nif#3", "type": "NiTriStripsData", "vertices": 24,
     "triangles": 30, "has_normals": true, "has_tangents": true, "uv_sets": 1,
     "has_vertex_colors": false, "bound_center": [0.0, 0.0, 1.5], "bound_radius": 3.2,
     "aabb_min": [-1.0, -1.0, 0.0], "aabb_max": [1.0, 1.0, 3.0]}
  ]
}
```

Only files with a block-size table are analysed; the others are listed in `skipped` (and
also files whose container cannot be read). `key` is `path#block`.

- `objects`: every block whose type is NiNode, a type derived from it (spec notes §3), or
  NiTriShape, NiTriStrips, BSSegmentedTriShape, in block order. `name` is the object name
  (`null` for "no string"). `parent` is the index of the first node (lowest block index)
  whose child list contains this block, else `null`. `rotation` is row-major.
- A file in which any block cannot be decoded is listed in `skipped` and contributes no
  objects or shapes.
- `shapes`: every NiTriShapeData and NiTriStripsData block. `vertices` is the stored vertex
  count. `has_tangents` means tangents are actually stored (normals present and the
  tangent bit set). `triangles` = triangle count
  if the has-triangles byte is set (lists), or the sum over strips of
  max(length − 2, 0) if the has-points byte is set (strips), else 0. `uv_sets` = the
  number of UV sets actually stored. `aabb_min`/`aabb_max` = component-wise min/max over
  the stored vertices (`null` when there are none).

Lab comparator: **keyed** on `key` for `/objects` and for `/shapes` (all fields), plus
**exact** on `/skipped`.

## `fnvtool nif nodes <sources…> --schema s [--examples 3] [--values 20]` → `fnv.nif.nodes/1`

```json
{
  "schema": "fnv.nif.nodes/1",
  "sources": ["Fallout - Meshes.bsa", "Update.bsa"],
  "files": 19277,
  "undecoded_blocks": 0,
  "skipped": [{"path": "...", "reason": "..."}],
  "block_types": [{"name": "NiNode", "files": 4271, "count": 38714, "types": {}, "values": [], "examples": ["meshes\\..."]}],
  "node":     [{"name": "ProjectileNode", "files": 218, "count": 221, "types": {"NiNode": 221}, "values": [], "examples": []}],
  "object":   [],
  "other":    [],
  "extra":    [{"name": "Prn", "files": 466, "count": 466, "types": {"NiStringExtraData": 466}, "values": [["Weapon", 190]], "examples": []}],
  "text_key": [{"name": "Eject", "files": 86, "count": 86, "types": {"NiTextKeyExtraData": 86}, "values": [], "examples": []}]
}
```

For finding names the engine never reads (docs/spec-notes/nif-node-audit.md). Each list is
sorted by `files` (how many files have the name), then by name; `count` is how many
blocks. `node`: names of blocks derived from NiNode; `object`: other NiAVObjects (shapes,
lights, cameras, particle systems); `other`: other named NiObjectNETs (properties,
controllers); `extra`: NiExtraData names with their string, integer or float values (the
`--values` most common); `text_key`: every NiTextKeyExtraData key text. `examples` keeps
the first `--examples` paths. A key list (`node`, `object`…) is absent when nothing has
that kind. Not used by the lab.

## `fnvtool dds list <archive>` → `fnv.dds.listing/1`

Defined in docs/spec-notes/dds.md together with the texture notes.

## `fnvtool spt list <archive>` → `fnv.spt.listing/1`, `fnvtool spt dump <source> [--path p]` → `fnv.spt.dump/1`

SpeedTree parameter files (docs/spec-notes/speedtree.md). `list`: one row per `.spt` in
the archive: `path`, `entries`, `seed`, `bark_texture`, `levels`, `leaf_maps`,
`leaf_uvs`, `generated` (`vertices`, `triangles`, `leaves`, `top`, `width` of NVForever's
generated tree at size 1), `error` (`null`, or the parse error with only `path` and
`entries` beside it). `dump`: `summary` (a `list` row) and `entries`: every entry in
file order as `tag`, `offset`, `value` (`null` for markers, a number for u8, `{u32, f32}`
for 4-byte payloads, a string, a float array, or an array of 8-float arrays for 10002).
Not compared by the lab (no golden).

# Layer 3 (world data)

All `world` commands take the Data folder, the plugins in load order, and
`--schema <schemas/fnv>`; they print the load order back as `load_order`. FormIDs are
**load-order (global) FormIDs** as `"0x%08X"`: the top byte is the index of the defining
plugin in the given load order (docs/spec-notes/esm.md). Floats are f32 values widened to
JSON numbers. Facts behind the fields: docs/spec-notes/world.md.

A cell's references are the records in persistent (8), temporary (9) and
visible-distant (10) cell-children groups labelled with that cell, except LAND and NAVM.
When several plugins define the same reference FormID, the last one in load order wins,
including which cell's group it is in. The same rule picks the cell's LAND.

## `fnvtool world refs ... (--cell <edid|formid> | --world <edid|formid> [--x X --y Y] | --interiors)` → `fnv.world.refs/1`

Selects one cell (`--cell`, or `--world` with `--x/--y` for an exterior grid cell), every
cell of a worldspace (`--world` alone, its persistent cell included), or every interior
cell (`--interiors`).

```json
{
  "schema": "fnv.world.refs/1",
  "cells": [
    {"formid": "0x0000ABCD", "edid": "Goodsprings", "name": null, "world": "0x000DA726",
     "grid": [-18, 0], "interior": false, "water_height": 3.4028234663852886e38,
     "refs": 159, "land": "0x0000ABCE"}
  ],
  "refs": [
    {"formid": "0x00012345", "type": "REFR", "cell": "0x0000ABCD", "base": "0x00012300",
     "base_type": "STAT", "base_edid": "Rock01", "model": "meshes\rocks\rock01.nif",
     "position": [1.0, 2.0, 3.0], "rotation": [0.0, 0.0, 1.5707964], "scale": 1.0,
     "flags": 0, "persistent": false, "initially_disabled": false, "deleted": false,
     "enable_parent": null}
  ]
}
```

Membership: a cell belongs to a worldspace when its winning CELL record sits inside that
worldspace's world-children group (type 1; the label is the WRLD FormID, local to the
plugin); interior cells are inside no world-children group. The persistent cell is the
CELL directly inside the world-children group (not inside a block group).

- `cells` sorted by `formid`; `refs` = the references of all selected cells, sorted by
  `formid`. `cell` = the cell whose group holds the winning definition.
- `type` = record signature of the winning definition.
- `base` = NAME mapped to the load order (`null` if absent or 0). `base_type`, `base_edid`
  come from the base's winning record; `null` when the base is not defined in the load
  order (engine-defined forms). `model` = the base's MODL with surrounding whitespace
  removed, `null` if that leaves it empty; otherwise ASCII letters lower-cased, `/` → `\`,
  leading `\` removed (TREE models are stored as e.g. `\WastelandShrub01.spt`), then
  prefixed with `trees\` if it ends in `.spt` (SpeedTree), else with `meshes\`, unless it
  already starts with that prefix.
- `position`, `rotation` = DATA (rotation in radians, as stored); `[0,0,0]` if DATA is
  absent. `scale` = XSCL, else 1.0. `flags` = record header flags of the winning
  definition; `initially_disabled` = bit 0x800, `deleted` = bit 0x20. `persistent` = the
  winning definition is in a type 8 group.
- `enable_parent` = XESP as `{"ref": formid, "opposite": bool (flag bit 0)}`, else `null`;
  the reference FormID is mapped like `base` (a null reference gives `null` for the whole
  field).
- Cell fields come from the winning CELL record: `edid` (EDID), `name` (FULL), `world`,
  `water_height` = XCLW (`null` if absent), `grid` = XCLC (x, y) or `null`, `interior` =
  DATA bit 0, `refs` = number of references whose winning definition is in this cell,
  `land` = the cell's LAND FormID or `null`.
- Field layouts: docs/spec-notes/world.md §5.

Lab comparator: **keyed** on `formid` for `/refs` and for `/cells` (all fields).

## `fnvtool world heights ... --world <edid|formid>` → `fnv.world.heights/1`

```json
{
  "schema": "fnv.world.heights/1",
  "world": "0x000DA726",
  "cells": [
    {"grid": [-18, 0], "min": 4012.0, "max": 4410.0, "mean": 4201.37,
     "corners": [4012.0, 4100.0, 4300.0, 4410.0], "centre": 4200.0}
  ]
}
```

One entry per exterior grid cell of the worldspace whose LAND has VHGT, sorted by
(x, y). The worldspace's persistent cell (the CELL directly in the world-children group)
is not a grid cell. Heights are the 33 × 33 integrated VHGT values in world units
(world.md §3); `corners` = [south-west, south-east, north-west, north-east] =
h[0][0], h[0][32], h[32][0], h[32][32] with h[row][col], rows south → north;
`centre` = h[16][16]; `mean` is the mean of all 1089 values.

Lab comparator: **keyed** on `grid` for `/cells` (all fields, float tolerance 1e-3).

## Other world commands (not yet scenarios)

- `world list` → `fnv.world.list/1`: per worldspace FormID, EDID, name, parent, persistent
  cell, cell / grid-cell / land-cell / reference counts, grid bounds, default heights;
  interior cell count; orphan groups; unreadable records.
- `world cells [--world W]` → `fnv.world.cells/1`: the cell objects above, for one
  worldspace or (without `--world`) all interior cells.
- `world land --world W --x X --y Y [--full]` → `fnv.world.land/1`: the 33 × 33 heights
  (rows south → north); with `--full` also normals (signed), colours and texture layers.
- `world check [--world W]` → `fnv.world.check/1`: the observational checks behind
  world.md (cell membership of references, shared LAND edges, normals against heights,
  base object types).
- `world posed-clutter [--nif-schema D] [--summary]` → `fnv.world.posed_clutter/1`: every
  placed reference whose model is ragdoll-like (two or more dynamic collision objects or
  any constraint), or whose model path names a `skeleton`, or which carries `XRGD`, with
  the branch NVForever takes (posed / refused and why / not posable type / no `XRGD` →
  held / STAT fixed) and the `XRGD` entry count against the model's bodies; `--summary`
  leaves out the per-reference rows (game data, not committed). The counts are recorded
  in docs/spec-notes/dead-on-spawn.md §5.2.
- `world audio [--cell C [--at x,y,z] [--hour H]]` → `fnv.world.audio/1`: `counts` of
  music types, media sets by type, controllers and acoustic spaces, `music_types` and
  `acoustic_spaces` (each with its reverb environment, ASPC `ANAM`, and interior flag).
  Without `--cell`: `world`, statistics of every audio marker and sound marker of the load
  order. With `--cell`: `cell` (music type, acoustic space with its sounds) and
  `audio_markers`, `controllers` and `sound_markers` in earshot (the whole worldspace for
  an exterior cell). With `--at`: `at`, the marker, set, layers and files chosen there
  at `--hour` (default 12), the background loop and the sound markers in reach
  (docs/spec-notes/audio.md §4–6).
- `world radio [--functions F]` → `fnv.world.radio/1`: `counts` (talking activators by
  header flags, placed station references by broadcast range, receivers by script, the
  stations' lines and song files, stations on the Pip-Boy), `stations` (with `on_pipboy`
  and each placed reference's `broadcasts_from`), `receiver_bases` (with their placed
  references) and `goodsprings_receivers`; no texts (docs/spec-notes/radio.md §7).
- `world terminals [--functions F]` → `fnv.world.terminals/1`: terminal records by hacking
  difficulty, flag, server; menu items with scripts, notes, sub menus and conditions (by
  run-on and function); placed terminals by level with examples; no texts
  (docs/spec-notes/hacking.md §1).
- `world hack <DATA_DIR> [--difficulty 0-5] [--science N] [--seed N] [--pick I,…]` →
  `fnv.world.hack/1`: one hacking game from the game's word list: `word_count`,
  `password`, `words` with their likeness, `pairs`, the 17 `rows` as the screen shows them,
  and the `picks` and `log` after the picked dump indices (hacking.md §2–5). Prints words of
  the game's word list: for the owner's console, not for committed files.

## `fnvtool phys ...` (Layer 4 physics)

`phys cell` (`fnv.phys.cell/1`: the physics world around a cell and probe characters
dropped in it), `phys walk` (`fnv.phys.walk/1`: a scripted character walk traced step by
step), `phys heights` (`fnv.phys.heights/1`: terrain height under fixed points of every
landscape cell, parity lab `L4.phys.heights.*`) and `phys terrain` (`fnv.phys.terrain/1`:
terrain colliders checked against the LAND heights of a worldspace). Their contract lives
in docs/spec-notes/physics.md §7.

# Layer 4 (collision geometry)

`fnvtool collide ragdoll` (`fnv.collide.ragdoll/1`, observational: the jointed bodies of
skeletons) is described in docs/spec-notes/ragdoll.md §5.
`fnvtool collide objects` (`fnv.collide.objects/1`) and `fnvtool collide survey`
(`fnv.collide.survey/1`) are defined in docs/spec-notes/collision.md §5 together with the
collision notes.

# Layer 4 (core simulation)

`fnvtool sim containers` (`fnv.sim.containers/1`), `fnvtool sim run`
(`fnv.sim.actions/1` → `fnv.sim.trace/1`), `fnvtool sim forms` (`fnv.sim.forms/1`) and
`fnvtool sim doors` (`fnv.sim.doors/1`) are defined in docs/spec-notes/simulation.md §7
together with the simulation notes.

`fnvtool sim door-models <data_dir> <plugin>... --schema <dir> [--nif-schema <dir>]`
(`fnv.sim.door_models/1`): every DOOR base with `door`, `edid`, `model`, `flags` (FNAM),
`open_sound`/`close_sound`/`loop_sound` (SNAM/ANAM/BNAM), its placed references (`refs`,
`teleport_refs`, `in_cell_refs`, `in_cell_with_lock`, `in_cell_open_by_default`), its
model's `sequences` (`name`, `start`, `stop`, `cycle_type`, `tracks`, `text_keys` as
`[time, text]`), `bodies` (collision objects counted by motion and layer, and for
keyframed bodies of Open/Close models whether the end of Close equals the stored pose
and whether the end of Open moves them) and `error`; `summary` aggregates them
(`by_sequence_names`, `text_keys_by_sequence`, `durations`, `cycle_types`,
`rigid_bodies`, `flags`). Notes: docs/spec-notes/doors.md.

# Layer 5 (compiled scripts)

`fnvtool script units` (`fnv.script.units/1`), `script decode` (`fnv.script.decode/1`),
`script decompile` (`fnv.script.decompile/1`, parity lab `L5.script.decode`) and
`script coverage` (`fnv.script.coverage/1`) are defined in docs/spec-notes/scripts.md §3.5
together with the bytecode notes.

`fnvtool script run <data_dir> <plugin>... --schema <dir> --actions <file>`
(`fnv.script.actions/1` → `fnv.script.trace/1`, parity lab `L5.quest.*`) follows the
contract in docs/spec-notes/layer5-captures.md §4; the operations beyond it (headless
stand-ins for Layer 6) and the trace's `events` and `final` are in
docs/spec-notes/scripts.md §8. `script coverage` fills `implemented` and `stub` per opcode
and reports the intro chain's coverage (scripts.md §7).

# Dialogue (Layer 6, steps D1 and D2)

`fnvtool dialogue topics` (`fnv.dialogue.topics/1`), `dialogue lines`
(`fnv.dialogue.lines/1`) and `dialogue talk` (`fnv.dialogue.actions/1` →
`fnv.dialogue.trace/1`: a scripted conversation run headless) are defined in
docs/spec-notes/dialogue.md §8. Texts are game data and are printed only with `--text`.

# Combat data (Layer 6, step C1)

`fnvtool combat weapon` (`fnv.combat.weapon/1`), `fnvtool combat form`
(`fnv.combat.form/1`) and `fnvtool combat survey` (`fnv.combat.survey/1`) are defined in
docs/spec-notes/combat-data.md §4; `fnvtool combat damage` (`fnv.combat.damage/1`) and
`fnvtool combat sim` (`fnv.combat.sim/1`) in docs/spec-notes/combat-rules.md §6.

# Actor appearance (workstream 5)

`fnvtool actor forms` (`fnv.actor.forms/1`), `fnvtool actor appearance`
(`fnv.actor.appearance/1`) and `fnvtool actor meshes` (`fnv.actor.meshes/1`) are defined in
docs/spec-notes/actors.md §5 together with the actor appearance notes.

# Animation data (workstream 5, part F)

`fnvtool actor anims` (`fnv.actor.anims/1`: every `.kf` grouped by skeleton folder with its
sequences and skeleton bindings, IDLE records per animation group with the `.kf` they
name, creature KFFZ/NIFZ lookups) is defined in docs/spec-notes/animation.md §6 together
with the animation data notes.

# Locomotion clips (workstream 5, part G)

`fnvtool actor locomotion` (`fnv.actor.locomotion/1`: the locomotion clips of a skeleton
folder per body and stance prefix, and each clip's root motion: accumulation-root
translation and turn per cycle, ground speed, text keys) is defined in
docs/spec-notes/actors.md §8.5 together with the locomotion notes.

# Placed creatures (workstream 5, part D)

`fnvtool actor creatures` (`fnv.actor.creatures/1`: every placed creature after leveled
and template resolution — skeleton, NIFZ body models, base scale, KFFZ names — and with
`--check-meshes` how each model joins its skeleton) is defined in
docs/spec-notes/actors.md §10.4 together with the creature notes.
# Weapon stance clips (workstream 5, part H)

`fnvtool actor weapons` (`fnv.actor.weapons/1`: per stance prefix the holster, equip, aim
and unequip clips with their `prn:` keys and track priorities, and the weapon each placed
NPC draws) is defined in docs/spec-notes/actors.md §9.5 together with the weapon notes.

# Furniture

`fnvtool actor furniture` (`fnv.actor.furniture/1`: every FURN with its model's entry
markers, marker flags and placed count, and the clips the IDLE tree names for getting
on, staying on and getting off per marker ID) is defined in
docs/spec-notes/furniture.md §6.

# Dialogue voice

`fnvtool voice line` (`fnv.voice.line/1`: a line's sound files per voice type and their
lengths), `voice coverage` (`fnv.voice.coverage/1`: the voice files of an install against
the lines of a load order) and `voice length` (`fnv.voice.length/1`: the headers of loose
sound files) are defined in docs/spec-notes/voice.md §10 together with the voice notes.
`fnvtool script run` and `fnvtool dialogue talk` read voice file lengths by default
(`--no-voice-lengths` or `FNVTOOL_VOICE_LENGTHS=0` keep the fitted line length; voice.md
§8).

# Navigation (navmeshes)

`fnvtool nav survey` (`fnv.nav.survey/1`), `nav graph` (`fnv.nav.graph/1`), `nav near`
(`fnv.nav.near/1`), `nav path` (`fnv.nav.path/1`), `nav walk` (`fnv.nav.walk/1`, an
NPC-sized physics character walking a path) and `nav observed` (`fnv.nav.observed/1`, NPC
movement recorded in the game against the navmesh) are defined in docs/spec-notes/navmesh.md §6
together with the navmesh notes.

# AI packages

`fnvtool ai survey` (`fnv.ai.survey/1`, what the PACK records hold, read from the
schema's decoded values), `ai packages` (`fnv.ai.packages/1`, the typed reader's tallies
and whether they agree with the survey), `ai show` (`fnv.ai.show/1`, packages in full and
actors with their package lists), `ai day` (`fnv.ai.day/1`, the package an actor chooses
at every hour and why) and `ai observed` (`fnv.ai.observed/1`, AI observer samples from
the game against the reader and the choice: package type codes, package lists, package
targets, schedules, a replay of the choice), `ai run` (`fnv.ai.run/1`, actors carrying
their packages out over the navmesh, alone or against observer samples) and `ai reach`
(`fnv.ai.reach/1`, whether every actor can reach where each of its packages sends it) are
defined in docs/spec-notes/packages.md §5 together with the package notes.

## `fnvtool ext scan <data_dir> <plugins…> --schema s [--functions f]` → `fnv.ext.scan/1`

Script-extender use in a load order's scripts (task N2, docs/handoff/extender-core-tasks.md;
`crates/fnvtool/src/ext_cmd.rs`). Only names and counts; no script text.

- `summary`: `plugins`, `units` (units with compiled bytes or source), `units_with_extender_use`,
  `units_without_source`, `units_without_source_using_extender` (no `SCTX`, and the compiled
  bytes call an opcode at or above 0x1400, past the base game's range [documented: GECK wiki
  "NVSE Opcode Base"]).
- `functions`: `[{name, origin, calls, plugins}]`, ranked by `plugins`, then `calls`: catalogue
  functions of family `extender` named in the source (name or alias, any case; `;` comments,
  quoted text and the unit's own variable names left out). A word that also names a base-game
  function is not counted.
- `syntax`: line counts of `let`, `assign` (`:=`), `compound` (`+=` `-=` `*=` `/=` `^=` `%=`
  `|=` `&=`), `if_eval`, `while`, `foreach`, `string_var`, `array_var`, `begin_override`
  (`Begin _Block`), `subscript` (`[`), `member` (`->`).
- `per_plugin`: the same per plugin.

Over the ten masters every extender count is 0 (14,907 units, 3 without source), as the
base game uses no extender (script-functions.md §2).

# Commands without a section above (listed 2026-10-25)

Their JSON layouts are defined by the code and the spec notes named; `fnvtool <group>
<command> --help` gives the arguments.

| Command | Schema | Purpose |
|---|---|---|
| `setup --game <dir>` | — | record the game folder (read-only) in NVForever.ini and create NVForever's home; refused when the game lacks files NVForever needs (design/install-layout.md) |
| `paths` | — | where NVForever keeps its files |
| `nif coverage <sources…> --schema s` | `fnv.nif.coverage/1` | per block type, how many decode exactly and re-encode byte-identically (the Layer 2 work queue) |
| `nif anim <archive>` | `fnv.nif.anim/1` | animation sequences of every .kf/.nif, with sanity statistics for the evaluated tracks |
| `nif textures` | `fnv.nif.textures/1` | every texture path every mesh names, and whether it resolves (archives in order + loose files) |
| `nif decode` | `fnv.nif.decode/1` | one file's blocks decoded with the schema |
| `world alt-textures` | `fnv.world.alt_textures/1` | records' alternate textures (MODS/MO2S/MO3S/MO4S): texture set, shapes, whether the files exist (spec-notes/texture-swaps.md) |
| `sim items` | `fnv.sim.items/1` | every inventory item base with value, weight, health and body slots |
| `sim sample` | `fnv.sim.samples/1` | leveled-list outcome histograms for the cases of a `fnv.sim.cases/1` file |
| `sim activator-models` | `fnv.sim.activator_models/1` | activators and talking activators: their models' controller sequences and what the tracks drive |
| `sim playgroup-calls` | `fnv.sim.playgroup_calls/1` | scripts' PlayGroup and IsAnimPlaying calls and whether the object's model has that sequence |
| `script compile` | `fnv.script.compile/1` | compile every unit's source and compare with its compiled bytes (spec-notes/script-compiler.md; lab `L5.script.compile.vanilla`) |
| `script stubs` | `fnv.script.stubs/1` | every stubbed function, why, its use in the scripts, and functions the scripts use that are not registered; `origin` per row (design/script-stubs.md) |
| `script uses` | `fnv.script.uses/1` | per quest, the functions its scripts call and its conditions test, by the intro chain's rule |
| `actor values` | `fnv.actor.values/1` | the actor values of named actors in a fresh game; `--survey` for the records (spec-notes/actor-values.md) |
| `actor aid` | `fnv.actor.aid/1` | every ingestible with flags, addiction and effects; `--use` for what using them does (spec-notes/aid-items.md §8) |
| `actor chargen` | `fnv.actor.chargen/1` | character creation as a new player sees it (spec-notes/chargen.md §6) |
| `actor factions` | `fnv.actor.factions/1` | named actors' template chains, factions and stand towards the player (spec-notes/factions.md) |
| `actor karma` | `fnv.actor.karma/1` | karma and crime settings, actors' karma and kill karma, cells' owners (spec-notes/crime-karma.md) |
| `actor fidgets` | `fnv.actor.fidgets/1` | drawn-weapon fidgets from the IDLE tree; `--furniture` the furniture ones (spec-notes/idles.md §11) |
| `actor gestures` | `fnv.actor.gestures/1` | talking and listening gestures picked from the IDLE tree (spec-notes/idles.md §12) |
| `actor detect` | `fnv.actor.detect/1` | one actor's detection numbers against a target at several distances (design/detection.md §4) |
| `actor creator` | `fnv.actor.creator/1` | the character creator's races, hairs, eyes, faces, beards, colours and named FaceGen sliders (spec-notes/character-creator.md §7) |
| `actor creature-weapons` | `fnv.actor.creature_weapons/1` | every weapon each placed creature base can carry and whether it is drawn (spec-notes/actors.md §10.4) |
| `actor tri` | `fnv.actor.tri/1` | FaceGen `.tri` morph labels and whether each matches its model's vertices |
| `combat fx` | `fnv.combat.fx/1` | a weapon's sounds, casing, projectile and explosion sounds, and impacts by material |
| `nav cells` | `fnv.world.cells/1` | cells of a worldspace, or every interior cell |
| `voice lip` | `fnv.voice.lip_files/1` | a response's `.lip` file decoded, one weight track per channel at 30 frames a second (`fnv.voice.lip/1` per file; spec-notes/lipsync.md) |
| `voice lip-file <files…>` | `fnv.voice.lip_files/1` | loose `.lip` files decoded |
| `voice lip-coverage` | `fnv.voice.lip_coverage/1` | every `.lip` file of the install decoded and counted |

## `fnvtool ext probes <data_dir> <plugins…> --schema s --probes <set> [--probes-dir d] [--frames n] [--risky]` → `fnv.ext.probes/1`

The candidate side of the extender probe scenarios (`L5.ext.nvse.<set>`,
parity-lab/scenarios/layer5_ext.json). For every probe quest of
`<probes-dir>/<set>` (script name + `Q`; a selector script with `iWhich` once per probe
number, `iWhich` set halfway) it runs a headless game (`fnv_game::runner`, `new_game`
false: start the quest, wait `--frames`, default 600) and keeps every trace event text
containing `EXTPROBE|`. Output: the same document `tools/capture/ext_probes.py collect`
writes from the original game's console (`rows` `{id, value}`, `files` with begin/end
markers, `duplicates`), plus `runs` (`{quest, which, lines}` or `{…, error}`). The runaway
loop and deep recursion run only with `--risky`.
