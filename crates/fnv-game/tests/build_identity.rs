//! What `GameData::build` makes from the game's own masters, as one digest a field (#1732):
//! a change meant only to make the build faster must leave every line as it was, so a
//! run before the change and one after are compared with `diff`. It needs the game data,
//! which is never in the repository: set `FNVLAB_DATA_DIR` (the parity lab's `data_dir`,
//! parity-lab/lab.json) to the game's `Data` folder. Without it, or without the ten masters
//! there, it prints why it was skipped and passes.
//!
//! ```text
//! FNVLAB_DATA_DIR=<Data> cargo test -p fnv-game --release --test build_identity -- --nocapture
//! ```
//!
//! Only digests are printed, no game text. `NVF_COMPILE_SOURCE=always` also takes the build
//! through `source_compile`. The `script build:` lines are the build's step times.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::RefTable;
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

#[path = "common/digest.rs"]
mod digest;

const MASTERS: [&str; 10] = [
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
    "CaravanPack.esm",
];

/// The game's Data folder, or `None` (with the reason printed) when the check cannot run.
fn data_dir() -> Option<PathBuf> {
    let Some(dir) = std::env::var_os("FNVLAB_DATA_DIR").map(PathBuf::from) else {
        eprintln!("skipped: FNVLAB_DATA_DIR is not set (no game data)");
        return None;
    };
    if let Some(missing) = MASTERS.iter().find(|m| !dir.join(m).is_file()) {
        eprintln!("skipped: {missing} is not in {}", dir.display());
        return None;
    }
    Some(dir)
}

#[test]
fn every_field_digest() {
    let Some(dir) = data_dir() else { return };
    let _ = tracing_subscriber::fmt().with_writer(std::io::stderr).with_max_level(tracing::Level::INFO).try_init();
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(dir.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_dir(root.join("schemas/fnv")).unwrap();
    let fp = root.join("schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&fp).unwrap()).unwrap();
    let table = FunctionTable::load(&fp).unwrap();
    let fresh = digest::table_lines(&table);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    for line in digest::lines(&data) {
        println!("identity {line}");
    }
    assert_eq!(digest::table_lines(&data.table), fresh, "the build gives back the table it was given");
}
