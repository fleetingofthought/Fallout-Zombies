//! `GameData::build`'s shortcuts leave its result as the long way makes it (#1732): every
//! record read and copied, the source compile done first. Over plugins written here (no game
//! data), so it runs everywhere: a master and a later plugin overriding its records, with
//! deleted and empty overrides, compressed records, unreadable ones, and the record types
//! the build skips with and without what it looks for in them.

mod common;
#[path = "common/digest.rs"]
mod digest;

use common::*;
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::source_compile::{set_mode, Mode};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, RefTable};
use serde_json::Value as Json;
use std::path::Path;

const DELETED: u32 = 0x20;
const PERSISTENT: u32 = 0x400;
const COMPRESSED: u32 = 0x0004_0000;

/// The record types the build may skip (`BULK_SIGS` of data.rs) and what it looks for in them.
const BULK: [&[u8; 4]; 6] = [b"REFR", b"ACHR", b"ACRE", b"PGRE", b"LAND", b"NAVM"];
const READ_SUBS: [[u8; 4]; 6] = [*b"EDID", *b"FULL", *b"SCRI", *b"DEST", *b"XPPA", *b"XRNK"];

fn raw_record(sig: &[u8; 4], flags: u32, fid: u32, payload: &[u8]) -> Vec<u8> {
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], payload].concat()
}

/// A zlib stream holding `data` in one stored block [documented: RFC 1950, RFC 1951 §3.2.4].
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let len = u16::try_from(data.len()).expect("one stored block");
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    [&[0x78, 0x01, 0x01][..], &len.to_le_bytes(), &(!len).to_le_bytes(), data, &((b << 16) | a).to_be_bytes()].concat()
}

/// A compressed record of these subrecords, as a plugin holds one.
fn compressed(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let p = write_subrecords(subs);
    raw_record(sig, flags | COMPRESSED, fid, &[&(p.len() as u32).to_le_bytes()[..], &zlib_stored(&p)].concat())
}

fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.concat()
}

const OBND: [u8; 12] = [1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0];
const OBND2: [u8; 12] = [9, 0, 8, 0, 7, 0, 6, 0, 5, 0, 4, 0];
/// Cut short: a subrecord's size past the end of the record.
const CUT: &[u8] = b"DATA\xFF\xFF";

fn name(base: u32) -> Subrecord {
    sub(b"NAME", base.to_le_bytes().to_vec())
}

/// Bytes that do nothing, for a script unit.
fn bytes() -> Vec<u8> {
    script(&[(0, None, Vec::new())])
}

/// A script with this source.
fn source_script(fid: u32, edid: &str, source: &str) -> Vec<u8> {
    let mut subs = unit(0, &bytes(), &[], &[]);
    subs.push(sub(b"SCTX", format!("scn {edid}\n{source}").into_bytes()));
    record(b"SCPT", 0, fid, &[vec![sub(b"EDID", z(edid))], subs].concat())
}

/// A script whose source declares a `string_var`, so the source compile picks it.
fn picked_script(fid: u32, edid: &str) -> Vec<u8> {
    source_script(fid, edid, "string_var s\nbegin GameMode\nend\n")
}

/// A record of each bulk type holding every subrecord the schema knows for it, in each of
/// these fillings and with each of these flags; `with_read_subs` keeps those the build looks for.
fn every_sub_records(schema: &Schema, first: u32, with_read_subs: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut fid = first;
    let fillings: [Vec<u8>; 4] = [vec![0; 64], (0..64).collect(), vec![0xFF; 64], vec![1, 0, 0, 0]];
    for sig in BULK {
        let mut sigs: Vec<[u8; 4]> = schema.common.keys().copied().collect();
        sigs.extend(schema.records.get(sig).into_iter().flat_map(|r| r.subrecords.keys().copied()));
        sigs.retain(|x| with_read_subs || !READ_SUBS.contains(x));
        sigs.sort_unstable();
        sigs.dedup();
        for fill in &fillings {
            let subs: Vec<Subrecord> = sigs.iter().map(|s| sub(s, fill.clone())).collect();
            out.extend(record(sig, 0, fid, &subs));
            out.extend(record(sig, PERSISTENT, fid + 1, &subs));
            out.extend(record(sig, DELETED, fid + 2, &subs));
            out.extend(compressed(sig, 0, fid + 3, &subs));
            fid += 4;
        }
    }
    out
}

/// The master. The numbers are what `assert_fixture` and the overrides name.
fn master(schema: &Schema) -> Vec<u8> {
    let stats = cat(&[
        record(b"STAT", 0, 0x800, &[sub(b"EDID", z("StatOverridden")), sub(b"OBND", OBND.to_vec())]),
        record(b"STAT", 0, 0x801, &[sub(b"EDID", z("StatEmptied")), sub(b"OBND", OBND.to_vec())]),
        record(b"STAT", 0, 0x802, &[sub(b"EDID", z("StatShortBounds")), sub(b"OBND", vec![1, 0, 2, 0])]),
        record(b"STAT", 0, 0x803, &[sub(b"OBND", OBND.to_vec()), sub(b"OBND", OBND2.to_vec())]),
        record(b"STAT", 0, 0x804, &[sub(b"OBND", vec![1, 0]), sub(b"OBND", OBND2.to_vec())]),
        compressed(b"STAT", 0, 0x805, &[sub(b"EDID", z("StatCompressed")), sub(b"OBND", OBND.to_vec())]),
        raw_record(b"STAT", 0, 0x806, &[&write_subrecords(&[sub(b"OBND", OBND.to_vec())])[..], CUT].concat()),
        record(b"ACTI", 0, 0x807, &[sub(b"EDID", z("ScriptedActivator")), sub(b"SCRI", 0x850u32.to_le_bytes().to_vec())]),
        record(b"STAT", 0, 0x808, &[sub(b"EDID", z("StatGone"))]),
    ]);
    let refs = cat(&[
        record(b"REFR", 0, 0x810, &[sub(b"EDID", z("RefDeleted")), name(0x800)]),
        record(b"REFR", 0, 0x811, &[sub(b"EDID", z("RefEmptied")), sub(b"FULL", z("Emptied")), sub(b"SCRI", 0x850u32.to_le_bytes().to_vec()), name(0x800)]),
        record(b"REFR", 0, 0x812, &[name(0x800)]),
        record(b"REFR", 0, 0x813, &[sub(b"EDID", z("RefNamesGone")), name(0x800)]),
        record(b"REFR", 0, 0x814, &[name(0x800), sub(b"SCRI", 0x850u32.to_le_bytes().to_vec())]),
        record(b"REFR", 0, 0x815, &[name(0x800), sub(b"FULL", z("Full Only"))]),
        record(b"REFR", 0, 0x816, &[name(0x800), sub(b"XRNK", 3i32.to_le_bytes().to_vec())]),
        record(b"REFR", 0, 0x817, &[name(0x800), sub(b"DEST", vec![50, 0, 0, 0, 0, 0, 0, 0])]),
        record(b"ACHR", 0, 0x818, &[name(0x800), sub(b"EDID", z("ActorRef"))]),
        record(b"ACHR", 0, 0x819, &[name(0x800)]),
        record(b"ACRE", 0, 0x81A, &[name(0x800), sub(b"XRNK", 1i32.to_le_bytes().to_vec())]),
        record(b"ACRE", 0, 0x81B, &[name(0x800)]),
        record(b"PGRE", 0, 0x81C, &[name(0x800), sub(b"FULL", z("Grenade"))]),
        record(b"PGRE", 0, 0x81D, &[name(0x800)]),
        record(b"NAVM", 0, 0x81E, &[sub(b"NVER", vec![1, 0, 0, 0])]),
        record(b"NAVM", 0, 0x81F, &[sub(b"EDID", z("NavWithName"))]),
        // Compressed.
        compressed(b"LAND", 0, 0x820, &[sub(b"DATA", vec![1; 4]), sub(b"VHGT", vec![7; 1093])]),
        compressed(b"LAND", 0, 0x821, &[sub(b"EDID", z("LandWithName")), sub(b"DATA", vec![1; 4])]),
        // Compressed, and not zlib: unreadable.
        raw_record(b"LAND", COMPRESSED, 0x822, &[&16u32.to_le_bytes()[..], b"not a zlib stream"].concat()),
        // Compressed, too short to say its size.
        raw_record(b"REFR", COMPRESSED, 0x823, &[1, 2]),
        compressed(b"REFR", 0, 0x824, &[name(0x800)]),
        compressed(b"REFR", 0, 0x825, &[name(0x800), sub(b"EDID", z("RefCompressed"))]),
        compressed(b"REFR", 0, 0x826, &[name(0x800), sub(b"XPPA", vec![])]),
        // Cut short, with and without what the build looks for before the cut.
        raw_record(b"REFR", 0, 0x827, CUT),
        raw_record(b"REFR", 0, 0x828, &[&write_subrecords(&[sub(b"EDID", z("RefCut"))])[..], CUT].concat()),
        raw_record(b"REFR", 0, 0x829, &[&write_subrecords(&[sub(b"XPPA", vec![])])[..], CUT].concat()),
        // No subrecord at all.
        record(b"REFR", PERSISTENT, 0x82A, &[]),
        record(b"LAND", 0, 0x82B, &[]),
    ]);
    // Patrol points: with a script, without one, and one the later plugin empties.
    let patrol = |fid: u32| record(b"REFR", 0, fid, &[vec![sub(b"EDID", z(&format!("Patrol{fid:X}"))), name(0x800), sub(b"XPPA", vec![])], unit(0, &bytes(), &[], &[])].concat());
    let patrols = cat(&[patrol(0x840), record(b"REFR", 0, 0x841, &[name(0x800), sub(b"XPPA", vec![])]), patrol(0x842)]);
    let script_rec = |fid: u32, edid: &str| record(b"SCPT", 0, fid, &[vec![sub(b"EDID", z(edid))], unit(0, &bytes(), &[], &[])].concat());
    let scripts = cat(&[
        script_rec(0x850, "ScriptOverridden"),
        script_rec(0x851, "ScriptEmptied"),
        script_rec(0x852, "ScriptDeleted"),
        picked_script(0x853, "ScriptPicked"),
        compressed(b"SCPT", 0, 0x854, &[vec![sub(b"EDID", z("ScriptCompressed"))], unit(0, &bytes(), &[], &[])].concat()),
        // Picked only when every unit is, and one picked whose source does not compile.
        source_script(0x855, "ScriptPlainSource", "begin GameMode\nend\n"),
        source_script(0x856, "ScriptBadSource", "string_var s\nbegin GameMode\nset NoSuchVariableAnywhere to 1\nend\n"),
    ]);
    let stage = |fid: u32, edid: &str| {
        let mut q = vec![sub(b"EDID", z(edid)), sub(b"INDX", 10i16.to_le_bytes().to_vec()), sub(b"QSDT", vec![0])];
        q.extend(unit(0, &bytes(), &[], &[]));
        record(b"QUST", 0, fid, &q)
    };
    let quests = cat(&[stage(0x860, "QuestOverridden"), stage(0x861, "QuestEmptied")]);
    let info = |fid: u32| record(b"INFO", 0, fid, &[vec![sub(b"TRDT", vec![0; 24]), sub(b"NAM1", z("A line"))], unit(0, &bytes(), &[], &[])].concat());
    let dialogue = cat(&[
        record(b"DIAL", 0, 0x870, &[sub(b"EDID", z("Topic")), sub(b"DATA", vec![0, 0])]),
        group(0x870u32.to_le_bytes(), 7, &cat(&[info(0x871), info(0x872)])),
    ]);
    let cell = cat(&[
        record(b"CELL", 0, 0x880, &[sub(b"EDID", z("CellOne")), sub(b"DATA", vec![1])]),
        group(0x880u32.to_le_bytes(), 6, &group(0x880u32.to_le_bytes(), 9, &cat(&[
            // Placed and plain: skipped by the read, but counted among the scripted references.
            record(b"REFR", 0, 0x881, &[name(0x807), sub(b"DATA", vec![0; 24])]),
            record(b"REFR", 0, 0x882, &[name(0x800), sub(b"DATA", vec![0; 24])]),
        ]))),
    ]);
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    cat(&[
        record(b"TES4", 1, 0, &[sub(b"HEDR", hedr)]),
        top(b"STAT", &stats),
        top(b"REFR", &refs),
        top(b"REFR", &patrols),
        top(b"SCPT", &scripts),
        top(b"QUST", &quests),
        top(b"DIAL", &dialogue),
        top(b"CELL", &cell),
        top(b"REFR", &every_sub_records(schema, 0x1000, false)),
        top(b"REFR", &every_sub_records(schema, 0x2000, true)),
    ])
}

/// The later plugin: form numbers with master index 0 override the master's.
fn patch(schema: &Schema) -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let own = 0x0100_0000u32;
    let changed_script = script(&[(0, None, set(var(b's', 1), ex(&[a("#1")])))]);
    cat(&[
        record(b"TES4", 0, 0, &[sub(b"HEDR", hedr), sub(b"MAST", z("Base.esm")), sub(b"DATA", vec![0; 8])]),
        top(
            b"STAT",
            &cat(&[
                record(b"STAT", 0, 0x800, &[sub(b"EDID", z("StatOverridden")), sub(b"OBND", OBND2.to_vec())]),
                // An empty override of a record with data.
                record(b"STAT", 0, 0x801, &[]),
                // A deleted override.
                record(b"STAT", DELETED, 0x808, &[]),
                compressed(b"STAT", 0, own | 0xA00, &[sub(b"EDID", z("PatchStat")), sub(b"OBND", OBND.to_vec())]),
            ]),
        ),
        top(
            b"REFR",
            &cat(&[
                record(b"REFR", DELETED, 0x810, &[]),
                record(b"REFR", 0, 0x811, &[]),
                // A plain record gains a name, a named one loses it.
                record(b"REFR", 0, 0x812, &[sub(b"EDID", z("RefGainsName")), name(0x800)]),
                record(b"REFR", 0, 0x813, &[name(0x800)]),
                // A record turned compressed, and one turned plain.
                compressed(b"LAND", 0, 0x82B, &[sub(b"DATA", vec![2; 4])]),
                record(b"LAND", 0, 0x820, &[sub(b"DATA", vec![3; 4])]),
                // Patrol points: emptied, gaining a script from nothing, script changed.
                record(b"REFR", 0, 0x840, &[]),
                record(b"REFR", 0, 0x841, &[vec![name(0x800), sub(b"XPPA", vec![])], unit(0, &bytes(), &[], &[])].concat()),
                record(b"REFR", DELETED, 0x842, &[vec![name(0x800), sub(b"XPPA", vec![])], unit(0, &changed_script, &[(1, "s", 0)], &[])].concat()),
                // The later plugin's own records.
                record(b"REFR", 0, own | 0xA10, &[name(0x800)]),
                record(b"REFR", 0, own | 0xA11, &[name(own | 0xA00), sub(b"EDID", z("PatchRef"))]),
            ]),
        ),
        top(
            b"SCPT",
            &cat(&[
                record(b"SCPT", 0, 0x850, &[vec![sub(b"EDID", z("ScriptOverridden"))], unit(0, &changed_script, &[(1, "s", 0)], &[])].concat()),
                record(b"SCPT", 0, 0x851, &[]),
                record(b"SCPT", DELETED, 0x852, &[]),
                picked_script(own | 0xA20, "PatchPicked"),
            ]),
        ),
        top(
            b"QUST",
            &cat(&[
                record(b"QUST", 0, 0x860, &[vec![sub(b"EDID", z("QuestOverridden")), sub(b"INDX", 20i16.to_le_bytes().to_vec()), sub(b"QSDT", vec![0])], unit(0, &bytes(), &[], &[])].concat()),
                record(b"QUST", 0, 0x861, &[]),
            ]),
        ),
        top(
            b"DIAL",
            &cat(&[
                record(b"DIAL", 0, 0x870, &[sub(b"EDID", z("Topic")), sub(b"DATA", vec![0, 0])]),
                group(0x870u32.to_le_bytes(), 7, &cat(&[record(b"INFO", 0, 0x871, &[]), record(b"INFO", DELETED, 0x872, &[])])),
            ]),
        ),
        top(b"REFR", &every_sub_records(schema, own | 0x1000, false)),
    ])
}

/// That the plugins are what the comparison means them to be, whichever way they were built.
fn assert_fixture(d: &GameData) {
    let f = FormId;
    assert_eq!(d.load_order, ["Base.esm", "Patch.esp"]);
    // Overrides.
    assert_eq!(d.object_bounds.get(&f(0x800)), Some(&[9, 8, 7, 6, 5, 4]), "the later plugin's bounds win");
    assert!(!d.object_bounds.contains_key(&f(0x801)) && !d.edid_of.contains_key(&f(0x801)), "the empty override wins");
    assert_eq!(d.sigs.get(&f(0x801)), Some(b"STAT"));
    assert_eq!(d.form_flags.get(&f(0x808)), Some(&DELETED), "the deleted override wins");
    assert!(!d.edid_of.contains_key(&f(0x810)) && d.form_flags.get(&f(0x810)) == Some(&DELETED));
    assert!(!d.edid_of.contains_key(&f(0x811)) && !d.full_names.contains_key(&f(0x811)) && !d.form_script.contains_key(&f(0x811)));
    assert_eq!(d.name(f(0x812)), "RefGainsName");
    assert!(!d.edid_of.contains_key(&f(0x813)));
    assert_eq!(d.name(f(0x0100_0A11)), "PatchRef");
    // Bounds: too short, the first of two, the first too short, compressed, unreadable.
    assert!(!d.object_bounds.contains_key(&f(0x802)));
    assert_eq!(d.object_bounds.get(&f(0x803)), Some(&[1, 2, 3, 4, 5, 6]));
    assert!(!d.object_bounds.contains_key(&f(0x804)));
    assert_eq!(d.object_bounds.get(&f(0x805)), Some(&[1, 2, 3, 4, 5, 6]));
    assert!(!d.object_bounds.contains_key(&f(0x806)));
    // Bulk types holding what the build looks for, and compressed ones.
    assert_eq!(d.form_script.get(&f(0x814)), Some(&f(0x850)));
    assert!(d.full_names.contains_key(&f(0x815)) && d.full_names.contains_key(&f(0x81C)));
    assert_eq!(d.owner_ranks.get(&f(0x816)), Some(&3));
    assert_eq!(d.owner_ranks.get(&f(0x81A)), Some(&1));
    assert!(d.destructibles.contains_key(&f(0x817)));
    for (form, name) in [(0x818, "ActorRef"), (0x81F, "NavWithName"), (0x821, "LandWithName"), (0x825, "RefCompressed")] {
        assert_eq!(d.name(f(form)), name);
    }
    // Skipped ones are still known by type.
    for (form, sig) in [(0x819, b"ACHR"), (0x81B, b"ACRE"), (0x81D, b"PGRE"), (0x81E, b"NAVM"), (0x820, b"LAND"), (0x824, b"REFR"), (0x82A, b"REFR"), (0x881, b"REFR")] {
        assert_eq!(d.sigs.get(&f(form)), Some(sig), "{form:X}");
    }
    assert_eq!(d.form_flags.get(&f(0x82A)), Some(&PERSISTENT));
    assert!(d.issues.count("record unreadable") >= 5, "{}", d.issues.count("record unreadable"));
    // Units.
    assert!(!d.patrol_scripts.contains_key(&f(0x840)), "an emptied patrol point has no script");
    assert!(d.patrol_scripts.contains_key(&f(0x841)) && d.patrol_scripts.contains_key(&f(0x842)));
    assert!(d.scripts.contains_key(&f(0x850)) && d.scripts.contains_key(&f(0x854)));
    assert!(!d.scripts.contains_key(&f(0x851)) && !d.scripts.contains_key(&f(0x852)));
    assert!(d.quests[&f(0x860)].stages.contains_key(&20) && d.quests[&f(0x860)].stages[&20][0].unit.is_some());
    assert!(!d.quests[&f(0x861)].stages.contains_key(&10));
    let topic = d.topics.get(&f(0x870)).expect("the topic's lines");
    assert_eq!(topic.len(), 2);
    assert_eq!(d.scripted_by_cell.get(&f(0x880)), Some(&vec![f(0x881)]), "a skipped record counts as scripted through its base");
}

#[test]
fn the_shortcuts_give_what_the_long_way_gives() {
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let plugins = vec![Plugin::parse(&master(&schema), "Base.esm".into()).unwrap(), Plugin::parse(&patch(&schema), "Patch.esp".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let refs = RefTable::build(&lo, &schema);
    // One test, so the three modes (a process-wide setting) take turns.
    for (mode, from_source, not_compiled) in [(Mode::Auto, 2, 1), (Mode::Always, 3, 1), (Mode::Never, 0, 0)] {
        set_mode(mode);
        let fast = GameData::build(&lo, &schema, table.clone(), &j, &refs);
        let slow = GameData::build_the_long_way(&lo, &schema, table.clone(), &j, &refs);
        assert_fixture(&fast);
        assert_eq!(fast.issues.count("script_compiled_from_source"), from_source, "{mode:?}");
        assert_eq!(fast.issues.count("script_source_does_not_compile"), not_compiled, "{mode:?}");
        let (a, b) = (digest::lines(&fast), digest::lines(&slow));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x, y, "{mode:?}");
        }
        assert_eq!(digest::table_lines(&fast.table), digest::table_lines(&table), "{mode:?}: the build gives back its table");
    }
    set_mode(Mode::Auto);
}
