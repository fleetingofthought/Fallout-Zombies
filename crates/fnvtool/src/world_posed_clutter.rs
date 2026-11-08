//! `fnvtool world posed-clutter`: every placed reference whose model is ragdoll-like —
//! two or more dynamic collision objects, or any constraint, or a model path naming a
//! skeleton — or which carries `XRGD`, with the branch NVForever takes
//! (docs/spec-notes/dead-on-spawn.md §5.2, issue #1400).
//!
//! The per-reference rows are game data and are not committed; the summary counts are what
//! the spec note records. This is observational, like `collide ragdoll`, not a lab scenario.

use crate::esm_cmd::open_load_order;
use crate::WorldArgs;
use anyhow::{anyhow, Context};
use fnv_collide::ragdoll::ragdoll;
use fnv_collide::Motion;
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_nif::file::NifFile;
use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::Schema as NifSchema;
use fnv_world::placed::{base_object, placement};
use fnv_world::{xrgd, WorldIndex};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// What one model contributes to the sweep.
#[derive(Debug, Clone, Default)]
struct ModelInfo {
    /// Solid collision objects (one per Havok body).
    objects: usize,
    /// Of those, the dynamic (falling) ones.
    dynamic: usize,
    /// Constraints joining the bodies.
    constraints: usize,
    /// Bodies whose node is named.
    named: usize,
    /// Bodies whose node is the mesh root (not below it); the drawing cannot pose these.
    rooted: usize,
    /// NiNode blocks below the mesh root (the drawing's `model.nodes.len()`).
    nodes_below_root: usize,
    /// A visible shape is skinned (a `skin_instance` block ref).
    skinned: bool,
    /// Why the model could not be read, if it could not.
    unreadable: Option<String>,
}

impl ModelInfo {
    /// Ragdoll-like: two or more dynamic bodies, or any constraint.
    fn jointed(&self) -> bool {
        self.dynamic >= 2 || self.constraints > 0
    }

    /// Whether the drawing's `clutter_pose::pose` can pose it: every body on a named node
    /// below the mesh root.
    fn posable_pose(&self) -> bool {
        self.named == self.objects && self.rooted == 0
    }
}

fn read_model(data: &[u8], nif: &NifSchema) -> ModelInfo {
    let file = match NifFile::parse(data, Some(nif)) {
        Ok(f) => f,
        Err(e) => return ModelInfo { unreadable: Some(e.to_string()), ..Default::default() },
    };
    let dec = Decoded::new(&file, data, nif);
    let r = ragdoll(&file, &dec);
    let objs = scene_objects(&file, &dec);
    let skinned = objs
        .iter()
        .any(|o| !o.is_node && dec.blocks[o.block].as_ref().ok().and_then(|s| s.block_ref("skin_instance")).is_some_and(|b| b >= 0));
    let roots: std::collections::HashSet<usize> = objs.iter().filter(|o| o.is_node && o.parent.is_none()).map(|o| o.block).collect();
    let nodes_below_root = objs.iter().filter(|o| o.is_node && o.parent.is_some()).count();
    let objects = r.bodies.len();
    let dynamic = r.bodies.iter().filter(|b| b.object.motion == Motion::Dynamic).count();
    let named = r.bodies.iter().filter(|b| b.object.node.is_some()).count();
    let rooted = r.bodies.iter().filter(|b| roots.contains(&b.object.node_block)).count();
    ModelInfo { objects, dynamic, constraints: r.joints.len(), named, rooted, nodes_below_root, skinned, unreadable: None }
}

/// One placed reference with what the sweep needs.
struct RefRow {
    form_id: u32,
    sig: [u8; 4],
    base: Option<u32>,
    base_sig: Option<[u8; 4]>,
    model: Option<String>,
    xrgd: usize,
    cell: u32,
}

fn fid(v: u32) -> Value {
    json!(formid_str(v))
}

fn sig(s: &[u8; 4]) -> String {
    String::from_utf8_lossy(s).into_owned()
}

pub fn run(w: &WorldArgs, nif_schema: Option<&Path>, summary_only: bool) -> anyhow::Result<Value> {
    let schema = Schema::load_dir(&w.schema).with_context(|| w.schema.display().to_string())?;
    let nif = match nif_schema {
        Some(d) => NifSchema::load_dir(d).with_context(|| d.display().to_string())?,
        None => {
            let dir = w.schema.parent().map(|p| p.join("nif")).filter(|d| d.is_dir()).ok_or_else(|| anyhow!("no NIF schema (give --nif-schema or put it next to --schema)"))?;
            NifSchema::load_dir(&dir).with_context(|| dir.display().to_string())?
        }
    };
    let plugins = open_load_order(&w.lo)?;
    let names: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
    let lo = LoadOrder::new(&plugins)?;
    let ix = WorldIndex::build(&lo, &schema);
    let vfs = fnv_vfs::Vfs::open_for_plugins(&w.lo.data_dir, &names)?;

    // Every placed reference, with its base's model and its XRGD entry count.
    let mut ids: Vec<u32> = ix.refs.keys().copied().collect();
    ids.sort_unstable();
    let rows: Vec<RefRow> = ids
        .par_iter()
        .filter_map(|&r| {
            let p = placement(&lo, &schema, r)?;
            let base = p.base.and_then(|b| base_object(&lo, &schema, b));
            Some(RefRow {
                form_id: r,
                sig: p.sig,
                base: p.base,
                base_sig: base.as_ref().map(|b| b.sig),
                model: base.as_ref().and_then(|b| b.world_model_path()),
                xrgd: xrgd::stored(&lo, r).map_or(0, |b| b.len()),
                cell: ix.refs[&r].cell,
            })
        })
        .collect();

    // Read each distinct model once, in parallel.
    let mut models: Vec<String> = rows.iter().filter_map(|r| r.model.clone()).collect();
    models.sort_unstable();
    models.dedup();
    let infos: HashMap<String, ModelInfo> = models
        .par_iter()
        .map(|m| {
            let info = match vfs.read(m) {
                None => ModelInfo { unreadable: Some("not found".into()), ..Default::default() },
                Some(Err(e)) => ModelInfo { unreadable: Some(e.to_string()), ..Default::default() },
                Some(Ok(data)) => read_model(&data, &nif),
            };
            (m.clone(), info)
        })
        .collect();

    // Classify each reference into the branch NVForever takes (out-of-scope ones — not
    // ragdoll-like, no `skeleton` path, no XRGD — are left out).
    let classified: Vec<Value> = rows
        .par_iter()
        .filter_map(|r| classify(&ix, r, &infos))
        .collect();

    // The counts, one branch per reference.
    let mut by_branch: BTreeMap<&str, u64> = BTreeMap::new();
    let mut refused: BTreeMap<&str, u64> = BTreeMap::new();
    let mut not_posable: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut by_base_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut actors = (0u64, 0u64);
    let mut skinned_models = BTreeMap::<String, u64>::new();
    for c in &classified {
        let branch = c["branch"].as_str().unwrap_or("");
        *by_branch.entry(branch).or_default() += 1;
        if let Some(r) = c.get("refused").and_then(Value::as_str) {
            *refused.entry(r).or_default() += 1;
        }
        if let (Some(t), Some(f)) = (c["base_type"].as_str(), c.get("fitting")) {
            if branch == "not_posable_type" {
                let e = not_posable.entry(t.to_string()).or_default();
                e.0 += 1;
                if f.as_bool().unwrap_or(false) {
                    e.1 += 1;
                }
            }
        }
        if let Some(t) = c["base_type"].as_str() {
            *by_base_type.entry(t.to_string()).or_default() += 1;
        }
        if branch == "actor" {
            actors.0 += 1;
            if c["xrgd"].as_u64().unwrap_or(0) > 0 {
                actors.1 += 1;
            }
        }
        if c["skinned"].as_bool().unwrap_or(false) {
            if let Some(m) = c["model"].as_str() {
                *skinned_models.entry(m.to_string()).or_default() += 1;
            }
        }
    }

    let mut out = json!({
        "schema": "fnv.world.posed_clutter/1",
        "models_read": models.len(),
        "models_unreadable": infos.values().filter(|m| m.unreadable.is_some()).count(),
        "summary": {
            "references": rows.len(),
            "in_scope": classified.len(),
            "by_branch": by_branch,
            "refused_by_reason": refused,
            "not_posable_with_xrgd_by_base_type": not_posable,
            "in_scope_by_base_type": by_base_type,
            "actors": {"placed": actors.0, "with_xrgd": actors.1},
            "skinned_models": skinned_models,
        },
    });
    if !summary_only {
        out["references"] = json!(classified);
    }
    out["load_order"] = json!(names);
    Ok(out)
}

/// The branch a reference takes, and why (issue #1400); `None` when it is out of scope
/// (not ragdoll-like, no `skeleton` path, no XRGD).
fn classify(ix: &WorldIndex, r: &RefRow, infos: &HashMap<String, ModelInfo>) -> Option<Value> {
    let model = infos.get(r.model.as_deref().unwrap_or_default());
    let objects = model.map_or(0, |m| m.objects);
    let named = model.map_or(0, |m| m.named);
    let dynamic = model.map_or(0, |m| m.dynamic);
    let constraints = model.map_or(0, |m| m.constraints);
    let skinned = model.is_some_and(|m| m.skinned);
    let jointed = model.is_some_and(|m| m.jointed());
    let skeleton_path = r.model.as_deref().is_some_and(|m| m.contains("skeleton"));
    let has_xrgd = r.xrgd > 0;
    let posable = r.base_sig.as_ref().is_some_and(xrgd::posable);
    let is_actor = matches!(&r.sig, b"ACHR" | b"ACRE");
    // Out of scope: not an actor, not ragdoll-like, no `skeleton` path, no XRGD.
    if !is_actor && !has_xrgd && !jointed && !skeleton_path {
        return None;
    }
    let base_sig = r.base_sig.as_ref().map(sig);
    // Whether the engine poses it: the shared count rule (`xrgd::matching`) holds and
    // every body's node is a named node below the mesh root (the drawing's
    // `clutter_pose::pose`, and the collision's matching gate, dead-on-spawn.md §5.2).
    let matched = xrgd::matching(r.xrgd, objects);
    let nodes_below_root = model.map_or(0, |m| m.nodes_below_root);
    let rooted = model.map_or(0, |m| m.rooted);
    let fitting = has_xrgd && matched.is_ok() && model.is_some_and(|m| m.posable_pose());

    let (branch, refused) = if is_actor {
        ("actor", None)
    } else if has_xrgd && posable && fitting {
        ("posed", None)
    } else if has_xrgd && posable {
        // The reason in the engine's own words (clutter_pose::pose): a body on the mesh
        // root is one whose name the drawing's below-root node list does not hold.
        ("refused_no_fit", Some(match matched { Err(e) => e, Ok(_) if rooted > 0 => "a body's node is not in the model", Ok(_) => "a body's node is unnamed" }))
    } else if has_xrgd {
        ("not_posable_type", None)
    } else if r.base_sig.is_some_and(|s| matches!(&s, b"STAT" | b"SCOL")) {
        ("stat_fixed", None)
    } else if jointed {
        ("no_xrgd_jointed", None)
    } else {
        ("skeleton_path_no_ragdoll", None)
    };

    let cell = ix.cells.get(&r.cell);
    Some(json!({
        "formid": fid(r.form_id),
        "type": sig(&r.sig),
        "cell": fid(r.cell),
        "world": cell.and_then(|c| c.world).map(fid),
        "base": r.base.map(fid),
        "base_type": base_sig,
        "model": r.model,
        "xrgd": r.xrgd,
        "objects": objects,
        "dynamic_objects": dynamic,
        "constraints": constraints,
        "named_bodies": named,
        "rooted_bodies": rooted,
        "nodes_below_root": nodes_below_root,
        "skinned": skinned,
        "fitting": fitting,
        "branch": branch,
        "refused": refused,
    }))
}
