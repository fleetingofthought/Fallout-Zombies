//! `fnvtool world ...`: worldspaces, cells, references and landscape (Layer 3 data).
//! JSON schemas are documented in docs/cli-contract.md.

use crate::esm_cmd::open_load_order;
use crate::{GridArgs, WorldArgs, WorldCmd};
use anyhow::{anyhow, Context};
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_world::land::{load_land, LandParts, SIDE, SPACING};
use fnv_world::placed::{base_object, placement};
use fnv_world::{cell_of, Cell, WorldIndex, Worldspace};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn run(cmd: WorldCmd) -> anyhow::Result<Value> {
    match cmd {
        WorldCmd::List { w } => with_world(&w, |lo, schema, ix| Ok(list(lo, schema, ix))),
        WorldCmd::Cells { w, world } => with_world(&w, |_, _, ix| cells(ix, world.as_deref())),
        WorldCmd::Refs { w, cell, world: wn, x, y, interiors } => with_world(&w, |lo, schema, ix| {
            let mut cs: Vec<&Cell> = match (cell, wn, x, y) {
                (Some(c), ..) => vec![ix.cell_by_name(&c).ok_or_else(|| anyhow!("no cell {c}"))?],
                (None, Some(wn), Some(x), Some(y)) => vec![grid_cell(ix, &wn, x, y)?],
                (None, Some(_), _, Some(_)) | (None, Some(_), Some(_), None) => {
                    return Err(anyhow!("--x and --y go together"))
                }
                (None, Some(wn), None, None) => {
                    let id = world(ix, &wn)?.form_id;
                    ix.cells.values().filter(|c| c.world == Some(id)).collect()
                }
                _ if interiors => ix.cells.values().filter(|c| c.world.is_none()).collect(),
                _ => unreachable!("clap requires --cell, --world or --interiors"),
            };
            cs.sort_by_key(|c| c.form_id);
            Ok(refs(lo, schema, ix, &cs))
        }),
        WorldCmd::Land { w, g, full } => with_world(&w, |lo, schema, ix| land(lo, schema, ix, &g, full)),
        WorldCmd::Heights { w, world } => with_world(&w, |lo, schema, ix| heights(lo, schema, ix, &world)),
        WorldCmd::Check { w, world } => with_world(&w, |lo, schema, ix| check(lo, schema, ix, world.as_deref())),
        WorldCmd::Audio { w, cell, at, hour } => {
            with_world(&w, |lo, schema, ix| crate::world_audio::audio(lo, schema, ix, cell.as_deref(), at.as_deref(), hour))
        }
        WorldCmd::Radio { w, functions } => crate::world_radio::run(&w, functions.as_deref()),
        WorldCmd::AltTextures { w, nif_schema, edid } => crate::world_alt_textures::survey(&w, nif_schema.as_deref(), &edid),
        WorldCmd::PosedClutter { w, nif_schema, summary } => crate::world_posed_clutter::run(&w, nif_schema.as_deref(), summary),
        WorldCmd::Terminals { w, functions } => crate::world_terminals::survey(&w, functions.as_deref()),
        WorldCmd::Hack { data_dir, difficulty, science, seed, pick } => crate::world_terminals::hack(&data_dir, difficulty, science, seed, &pick),
    }
}

fn with_world(
    w: &WorldArgs,
    f: impl FnOnce(&LoadOrder, &Schema, &WorldIndex) -> anyhow::Result<Value>,
) -> anyhow::Result<Value> {
    let schema = Schema::load_dir(&w.schema).with_context(|| w.schema.display().to_string())?;
    let plugins = open_load_order(&w.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let ix = WorldIndex::build(&lo, &schema);
    let mut v = f(&lo, &schema, &ix)?;
    v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
    Ok(v)
}

fn fid(v: u32) -> Value {
    json!(formid_str(v))
}

fn opt_fid(v: Option<u32>) -> Value {
    v.map_or(Value::Null, fid)
}

fn sig(s: &[u8; 4]) -> String {
    String::from_utf8_lossy(s).into_owned()
}

fn world<'a>(ix: &'a WorldIndex, name: &str) -> anyhow::Result<&'a Worldspace> {
    ix.world_by_name(name).ok_or_else(|| anyhow!("no worldspace {name}"))
}

fn grid_cell<'a>(ix: &'a WorldIndex, wn: &str, x: i32, y: i32) -> anyhow::Result<&'a Cell> {
    let w = world(ix, wn)?;
    let c = w.grid.get(&(x, y)).ok_or_else(|| anyhow!("{wn} has no cell at {x},{y}"))?;
    Ok(&ix.cells[c])
}

fn list(_lo: &LoadOrder, _schema: &Schema, ix: &WorldIndex) -> Value {
    let worlds: Vec<Value> = ix
        .worldspaces
        .values()
        .map(|w| {
            let cells: Vec<&Cell> = ix.cells.values().filter(|c| c.world == Some(w.form_id)).collect();
            let refs: usize = cells.iter().map(|c| c.refs.len()).sum();
            let xs = w.grid.keys().map(|k| k.0);
            let ys = w.grid.keys().map(|k| k.1);
            json!({
                "formid": fid(w.form_id),
                "edid": w.edid,
                "name": w.name,
                "parent": opt_fid(w.parent),
                "parent_flags": w.parent_flags,
                "persistent_cell": opt_fid(w.persistent_cell),
                "cells": cells.len(),
                "grid_cells": w.grid.len(),
                "land_cells": cells.iter().filter(|c| c.land.is_some()).count(),
                "refs": refs,
                "grid_min": [xs.clone().min(), ys.clone().min()],
                "grid_max": [xs.max(), ys.max()],
                "default_land_height": w.default_land_height,
                "default_water_height": w.default_water_height,
            })
        })
        .collect();
    let interiors = ix.cells.values().filter(|c| c.world.is_none()).count();
    json!({
        "schema": "fnv.world.list/1",
        "worldspaces": worlds,
        "interior_cells": interiors,
        "orphan_groups": ix.orphan_groups.iter().map(|&c| fid(c)).collect::<Vec<_>>(),
        "grid_conflicts": ix
            .grid_conflicts
            .iter()
            .map(|&(w, g, a, b)| json!({"world": fid(w), "grid": [g.0, g.1], "cells": [fid(a), fid(b)]}))
            .collect::<Vec<_>>(),
        "errors": ix.errors.iter().map(|(f, e)| json!({"formid": fid(*f), "error": e})).collect::<Vec<_>>(),
    })
}

fn cell_json(c: &Cell) -> Value {
    json!({
        "formid": fid(c.form_id),
        "edid": c.edid,
        "name": c.name,
        "world": opt_fid(c.world),
        "grid": c.grid.map(|(x, y)| json!([x, y])),
        "interior": c.is_interior(),
        "water_height": c.water_height,
        "refs": c.refs.len(),
        "land": opt_fid(c.land),
    })
}

fn cells(ix: &WorldIndex, wn: Option<&str>) -> anyhow::Result<Value> {
    let w = wn.map(|n| world(ix, n)).transpose()?;
    let want = w.map(|w| w.form_id);
    let mut cs: Vec<&Cell> = ix.cells.values().filter(|c| c.world == want).collect();
    cs.sort_by_key(|c| (c.grid, c.form_id));
    Ok(json!({
        "schema": "fnv.world.cells/1",
        "world": w.map(|w| fid(w.form_id)),
        "cells": cs.into_iter().map(cell_json).collect::<Vec<_>>(),
    }))
}

fn refs(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, cells: &[&Cell]) -> Value {
    let mut ids: Vec<u32> = cells.iter().flat_map(|c| c.refs.iter().copied()).collect();
    ids.sort_unstable();
    let rows: Vec<Value> = ids
        .par_iter()
        .map(|&r| {
            let e = ix.refs[&r];
            let Some(p) = placement(lo, schema, r) else {
                return json!({"formid": fid(r), "type": sig(&e.sig), "cell": fid(e.cell), "error": "unreadable"});
            };
            let base = p.base.and_then(|b| base_object(lo, schema, b));
            json!({
                "formid": fid(r),
                "type": sig(&p.sig),
                "cell": fid(e.cell),
                "base": opt_fid(p.base),
                "base_type": base.as_ref().map(|b| sig(&b.sig)),
                "base_edid": base.as_ref().and_then(|b| b.edid.clone()),
                "model": base.as_ref().and_then(|b| b.model_path()),
                "position": p.position,
                "rotation": p.rotation,
                "scale": p.scale,
                "flags": p.flags,
                "persistent": e.persistent,
                "initially_disabled": p.initially_disabled(),
                "deleted": p.deleted(),
                "enable_parent": p.enable_parent.map(|(f, o)| json!({"ref": fid(f), "opposite": o})),
            })
        })
        .collect();
    json!({"schema": "fnv.world.refs/1", "cells": cells.iter().map(|c| cell_json(c)).collect::<Vec<_>>(), "refs": rows})
}

fn rows<T: Copy>(v: &[T]) -> Vec<Vec<T>> {
    v.chunks(SIDE).map(|r| r.to_vec()).collect()
}

fn land(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, g: &GridArgs, full: bool) -> anyhow::Result<Value> {
    let c = grid_cell(ix, &g.world, g.x, g.y)?;
    let lf = c.land.ok_or_else(|| anyhow!("cell {} has no LAND", formid_str(c.form_id)))?;
    let parts = if full { LandParts::ALL } else { LandParts::HEIGHTS };
    let l = load_land(lo, schema, lf, parts).ok_or_else(|| anyhow!("LAND {} unreadable", formid_str(lf)))?;
    let mut v = json!({
        "schema": "fnv.world.land/1",
        "world": fid(c.world.unwrap_or(0)),
        "grid": [g.x, g.y],
        "cell": fid(c.form_id),
        "formid": fid(lf),
        "flags": l.flags,
        "heights": l.heights.as_deref().map(rows),
    });
    if full {
        v["normals"] = json!(l.normals.as_deref().map(rows));
        v["colors"] = json!(l.colors.as_deref().map(rows));
        v["layers"] = json!(l
            .layers
            .iter()
            .map(|t| json!({
                "quadrant": t.quadrant,
                "texture": opt_fid(t.texture),
                "layer": t.layer,
                "base": t.base,
                "alpha": t.alpha.iter().map(|(p, o)| json!([p, o])).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>());
    }
    Ok(v)
}

/// Heights of every land cell in a worldspace, keyed by grid.
fn world_heights(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, w: &Worldspace) -> BTreeMap<(i32, i32), Vec<f32>> {
    let cells: Vec<((i32, i32), u32)> =
        w.grid.iter().filter_map(|(&g, c)| Some((g, ix.cells[c].land?))).collect();
    cells
        .par_iter()
        .filter_map(|&(g, lf)| Some((g, load_land(lo, schema, lf, LandParts::HEIGHTS)?.heights?)))
        .collect()
}

fn heights(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, wn: &str) -> anyhow::Result<Value> {
    let w = world(ix, wn)?;
    let hs = world_heights(lo, schema, ix, w);
    let cells: Vec<Value> = hs
        .iter()
        .map(|(&(x, y), h)| {
            let min = h.iter().copied().fold(f32::INFINITY, f32::min);
            let max = h.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let mean = h.iter().map(|&v| v as f64).sum::<f64>() / h.len() as f64;
            let n = SIDE - 1;
            json!({
                "grid": [x, y],
                "min": min,
                "max": max,
                "mean": mean,
                "corners": [h[0], h[n], h[n * SIDE], h[n * SIDE + n]],
                "centre": h[(SIDE / 2) * SIDE + SIDE / 2],
            })
        })
        .collect();
    Ok(json!({"schema": "fnv.world.heights/1", "world": fid(w.form_id), "cells": cells}))
}

/// Normal of the height field at a vertex from central differences (one-sided at edges).
fn height_normal(h: &[f32], r: usize, c: usize) -> [f32; 3] {
    let at = |r: usize, c: usize| h[r * SIDE + c];
    let (c0, c1) = (c.saturating_sub(1), (c + 1).min(SIDE - 1));
    let (r0, r1) = (r.saturating_sub(1), (r + 1).min(SIDE - 1));
    let dx = (at(r, c1) - at(r, c0)) / ((c1 - c0) as f32 * SPACING);
    let dy = (at(r1, c) - at(r0, c)) / ((r1 - r0) as f32 * SPACING);
    let n = [-dx, -dy, 1.0];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    n.map(|v| v / l)
}

fn angle_deg(a: [f32; 3], b: [f32; 3]) -> f32 {
    let la = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let lb = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    if la == 0.0 || lb == 0.0 {
        return 180.0;
    }
    let d = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) / (la * lb);
    d.clamp(-1.0, 1.0).acos().to_degrees()
}

#[derive(Default)]
struct EdgeStats {
    pairs: u64,
    exact: u64,
    max_diff: f32,
    examples: Vec<Value>,
}

impl EdgeStats {
    fn add(&mut self, at: (i32, i32), a: impl Iterator<Item = f32>, b: impl Iterator<Item = f32>) {
        let d = a.zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max);
        self.pairs += 1;
        if d == 0.0 {
            self.exact += 1;
        } else if self.examples.len() < 10 {
            self.examples.push(json!({"grid": [at.0, at.1], "max_diff": d}));
        }
        self.max_diff = self.max_diff.max(d);
    }
    fn json(&self) -> Value {
        json!({"pairs": self.pairs, "exact": self.exact, "max_diff": self.max_diff, "examples": self.examples})
    }
}

struct RefRow {
    form_id: u32,
    persistent: bool,
    grid: Option<(i32, i32)>,
    /// Grid square containing the reference's position.
    at: (i32, i32),
    base_type: String,
    has_model: bool,
}

fn check(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, wn: Option<&str>) -> anyhow::Result<Value> {
    let worlds: Vec<&Worldspace> = match wn {
        Some(n) => vec![world(ix, n)?],
        None => ix.worldspaces.values().collect(),
    };
    let mut out = Vec::new();
    for w in worlds {
        // References: does each temporary exterior reference lie in its cell's square?
        let mut inside = 0u64;
        let mut outside = Vec::new();
        let mut outside_count = 0u64;
        let mut persistent_refs = 0u64;
        let mut base_types: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        let mut no_base = Vec::new();
        let mut cells: Vec<&Cell> = ix.cells.values().filter(|c| c.world == Some(w.form_id)).collect();
        cells.sort_by_key(|c| c.form_id);
        let placed: Vec<(&Cell, u32)> = cells.iter().flat_map(|c| c.refs.iter().map(move |&r| (*c, r))).collect();
        let rows: Vec<RefRow> = placed
            .par_iter()
            .filter_map(|&(c, r)| {
                let p = placement(lo, schema, r)?;
                let b = p.base.and_then(|b| base_object(lo, schema, b));
                Some(RefRow {
                    form_id: r,
                    persistent: ix.refs[&r].persistent,
                    grid: c.grid,
                    at: cell_of(p.position[0], p.position[1]),
                    base_type: b.as_ref().map_or("none".to_string(), |b| sig(&b.sig)),
                    has_model: b.as_ref().and_then(|b| b.model_path()).is_some(),
                })
            })
            .collect();
        for RefRow { form_id: r, persistent, grid, at, base_type: bt, has_model } in rows {
            if bt == "none" && no_base.len() < 10 {
                no_base.push(fid(r));
            }
            let e = base_types.entry(bt).or_default();
            e.0 += 1;
            if has_model {
                e.1 += 1;
            }
            if Some(ix.refs[&r].cell) == w.persistent_cell {
                persistent_refs += 1;
                continue;
            }
            if persistent {
                continue;
            }
            match grid {
                Some(g) if g == at => inside += 1,
                Some(g) => {
                    outside_count += 1;
                    if outside.len() < 20 {
                        outside.push(json!({"ref": fid(r), "cell": [g.0, g.1], "position_cell": [at.0, at.1]}));
                    }
                }
                None => {}
            }
        }

        // Landscape: shared edges between neighbouring cells, and stored normals against
        // normals derived from the heights.
        let hs = world_heights(lo, schema, ix, w);
        let (mut east, mut north, mut east_transposed) = (EdgeStats::default(), EdgeStats::default(), EdgeStats::default());
        let n = SIDE - 1;
        for (&(x, y), h) in &hs {
            if let Some(e) = hs.get(&(x + 1, y)) {
                east.add((x, y), (0..SIDE).map(|r| h[r * SIDE + n]), (0..SIDE).map(|r| e[r * SIDE]));
                east_transposed.add((x, y), (0..SIDE).map(|c| h[n * SIDE + c]), (0..SIDE).map(|c| e[c]));
            }
            if let Some(no) = hs.get(&(x, y + 1)) {
                north.add((x, y), (0..SIDE).map(|c| h[n * SIDE + c]), (0..SIDE).map(|c| no[c]));
            }
        }
        let sample: Vec<u32> = w
            .grid
            .values()
            .filter_map(|c| ix.cells[c].land)
            .step_by(7)
            .take(300)
            .collect();
        let normal_rows: Vec<(f64, f64, f64, u64)> = sample
            .par_iter()
            .filter_map(|&lf| {
                let l = load_land(lo, schema, lf, LandParts { heights: true, normals: true, colors: false, layers: false })?;
                let (h, nm) = (l.heights?, l.normals?);
                let (mut signed, mut unsigned, mut swapped, mut k) = (0.0, 0.0, 0.0, 0u64);
                for r in 1..SIDE - 1 {
                    for c in 1..SIDE - 1 {
                        let want = height_normal(&h, r, c);
                        let s = nm[r * SIDE + c];
                        let sf = [s[0] as f32, s[1] as f32, s[2] as f32];
                        let uf = [s[0] as u8 as f32, s[1] as u8 as f32, s[2] as u8 as f32];
                        signed += angle_deg(sf, want) as f64;
                        unsigned += angle_deg(uf, want) as f64;
                        swapped += angle_deg([sf[1], sf[0], sf[2]], want) as f64;
                        k += 1;
                    }
                }
                Some((signed, unsigned, swapped, k))
            })
            .collect();
        let k: u64 = normal_rows.iter().map(|r| r.3).sum();
        let mean = |f: fn(&(f64, f64, f64, u64)) -> f64| {
            if k == 0 {
                Value::Null
            } else {
                json!(normal_rows.iter().map(f).sum::<f64>() / k as f64)
            }
        };
        out.push(json!({
            "formid": fid(w.form_id),
            "edid": w.edid,
            "cells": cells.len(),
            "persistent_cell": opt_fid(w.persistent_cell),
            "persistent_cell_refs": persistent_refs,
            "temporary_refs_inside_cell": inside,
            "temporary_refs_outside_cell": outside_count,
            "temporary_refs_outside_cell_examples": outside,
            "refs_without_base_examples": no_base,
            "base_types": base_types.iter().map(|(t, (n, m))| json!({"type": t, "refs": n, "with_model": m})).collect::<Vec<_>>(),
            "land_cells": hs.len(),
            "edges_east": east.json(),
            "edges_north": north.json(),
            "edges_east_if_rows_were_x": east_transposed.json(),
            "normals_vertices": k,
            "normals_mean_angle_deg": {
                "signed": mean(|r| r.0),
                "unsigned": mean(|r| r.1),
                "signed_xy_swapped": mean(|r| r.2),
            },
        }));
    }
    Ok(json!({"schema": "fnv.world.check/1", "worldspaces": out}))
}
