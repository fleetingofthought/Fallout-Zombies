//! `fnvtool collide …`: Layer 4 collision geometry (crate `fnv-collide`).

use crate::source::process;
use crate::CollideCmd;
use fnv_collide::nif::{extract, motion, Aabb, Extracted};
use fnv_collide::shape::{Iso, Shape};
use fnv_nif::file::NifFile;
use fnv_nif::header::version_string;
use fnv_nif::scene::Decoded;
use fnv_nif::Schema;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn run(cmd: CollideCmd) -> anyhow::Result<Value> {
    match cmd {
        CollideCmd::Survey { sources, schema, filter } => survey(&sources, &Schema::load_dir(&schema)?, filter.as_deref()),
        CollideCmd::Objects { archive, schema, filter } => objects(&archive, &Schema::load_dir(&schema)?, filter.as_deref()),
        CollideCmd::Ragdoll { sources, schema, filter, summary } => ragdolls(&sources, &Schema::load_dir(&schema)?, filter.as_deref(), summary),
    }
}

fn analyse(data: &[u8], schema: &Schema) -> Result<Extracted, String> {
    let file = NifFile::parse(data, Some(schema)).map_err(|e| e.to_string())?;
    // As in fnv.nif.scene/1: only files with a block-size table are analysed.
    if file.header.block_sizes.is_none() {
        return Err(format!("no block-size table (version {})", version_string(file.header.version)));
    }
    let dec = Decoded::new(&file, data, schema);
    Ok(extract(&file, &dec))
}

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}

/// Leaf family of a shape tree, for the scale measurement.
fn family(types: &[String]) -> String {
    let mut leaves: Vec<&str> = types
        .iter()
        .map(String::as_str)
        .filter(|t| {
            !matches!(*t, "bhkListShape" | "bhkTransformShape" | "bhkConvexTransformShape" | "bhkMoppBvTreeShape")
        })
        .collect();
    leaves.sort_unstable();
    leaves.dedup();
    leaves.join("+")
}

fn bounds(shape: &Shape, iso: &Iso) -> Aabb {
    let mut b = Aabb::empty();
    shape.bound_points(iso, &mut |p| b.add(p));
    b
}

/// The shape with every compound child's rotation transposed (convention test).
fn transposed(s: &Shape) -> Shape {
    match s {
        Shape::Compound { children } => Shape::Compound {
            children: children
                .iter()
                .map(|(iso, c)| {
                    let r = iso.rotation;
                    (Iso { translation: iso.translation, rotation: [r[0], r[3], r[6], r[1], r[4], r[7], r[2], r[5], r[8]] }, transposed(c))
                })
                .collect(),
        },
        other => other.clone(),
    }
}

fn quantiles(mut v: Vec<f64>) -> Value {
    if v.is_empty() {
        return Value::Null;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let q = |f: f64| v[((v.len() - 1) as f64 * f).round() as usize];
    json!({"n": v.len(), "p05": q(0.05), "p25": q(0.25), "median": q(0.5), "p75": q(0.75), "p95": q(0.95)})
}

/// Observational survey of every collision object (fnv.collide.survey/1): structure
/// statistics plus the stored-units → render-units scale per shape family.
fn survey(sources: &[std::path::PathBuf], schema: &Schema, filter: Option<&str>) -> anyhow::Result<Value> {
    let items = process(sources, &["nif"], filter, |_, data| analyse(data, schema))?;
    let (mut files, mut with_collision, mut unreadable) = (0u64, 0u64, 0u64);
    let mut objects: BTreeMap<String, u64> = BTreeMap::new();
    let mut bodies: BTreeMap<String, u64> = BTreeMap::new();
    let mut trees: BTreeMap<String, u64> = BTreeMap::new();
    let mut layers: BTreeMap<String, u64> = BTreeMap::new();
    let mut motion: BTreeMap<String, u64> = BTreeMap::new();
    let mut quality: BTreeMap<String, u64> = BTreeMap::new();
    let mut problems: BTreeMap<String, u64> = BTreeMap::new();
    let mut body_moved: BTreeMap<String, u64> = BTreeMap::new();
    let mut mat_layout: BTreeMap<String, u64> = BTreeMap::new();
    let (mut compressed, mut mixed) = (0u64, 0u64);
    // Stored contact-shell radius (largest on a box / convex / mesh shape) by layer.
    let mut shells: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    // family -> ratios; and centre offsets (relative to render size) with/without body transform
    let mut ratio: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut ratio_samples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut centre_plain: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut centre_body: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    // Transform-shape rotation convention: fit error with the rotation as read vs transposed.
    let mut mat_fit: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    // Same question at vertex level: mean distance from collision points to the nearest
    // render vertex (relative to mesh size), only where a transform rotation is not identity.
    let mut mat_vfit: Vec<(String, f64, f64)> = Vec::new();
    for it in &items {
        files += 1;
        let ex = match &it.result {
            Ok(Ok(e)) => e,
            _ => {
                unreadable += 1;
                continue;
            }
        };
        for p in &ex.problems {
            let key = p.split(|c: char| c.is_ascii_digit()).next().unwrap_or(p).trim().to_string();
            bump(&mut problems, key);
        }
        if ex.objects.is_empty() {
            continue;
        }
        with_collision += 1;
        for o in &ex.objects {
            bump(&mut objects, &o.object_type);
            bump(&mut bodies, &o.body_type);
            bump(&mut trees, o.shape_types.join(">"));
            bump(&mut layers, o.layer.map_or("none".into(), |l| l.to_string()));
            bump(&mut motion, o.motion_system.map_or("none".into(), |l| l.to_string()));
            bump(&mut quality, o.quality_type.map_or("none".into(), |l| l.to_string()));
            let moved = o.body_translation.iter().any(|v| v.abs() > 1e-4);
            let rotated = (o.body_rotation[3].abs() - 1.0).abs() > 1e-4;
            bump(&mut body_moved, format!("{} translated={moved} rotated={rotated}", o.body_type));
            for m in &o.transform_matrices {
                let col = m[12].abs() + m[13].abs() + m[14].abs() > 1e-6;
                let row = m[3].abs() + m[7].abs() + m[11].abs() > 1e-6;
                bump(&mut mat_layout, format!("t@12-14={col} t@3,7,11={row} m15={}", m[15]));
            }
            compressed += o.compressed_vertices as u64;
            mixed += o.mixed_subpart_triangles as u64;
            let layer = o.layer.map_or("none".into(), |l| l.to_string());
            bump(shells.entry(layer).or_default(), format!("{:.3}", o.shell));
        }
        // Scale: one collision object on the root node, render bounds known.
        if let ([o], Some(render)) = (ex.objects.as_slice(), ex.render) {
            if o.node_block != 0 {
                continue;
            }
            let fam = format!("{} / {}", family(&o.shape_types), o.body_type);
            let plain = bounds(&o.shape, &Iso::IDENTITY);
            let with_body = bounds(&o.shape, &o.body_iso());
            let (re, ce) = (render.extent(), plain.extent());
            let k = (0..3).max_by(|&a, &b| re[a].total_cmp(&re[b])).unwrap();
            if ce[k] <= 1e-6 || re[k] <= 1e-3 {
                continue;
            }
            let r = (re[k] / ce[k]) as f64;
            ratio.entry(fam.clone()).or_default().push(r);
            let s = ratio_samples.entry(fam.clone()).or_default();
            if s.len() < 3 {
                s.push(format!("{} ratio {r:.3}", it.path));
            }
            // Centre offsets after scaling by 7 (the value to test), relative to size.
            let size = (re[0].max(re[1]).max(re[2])) as f64;
            let off = |b: &Aabb| {
                let (c, rc) = (b.center(), render.center());
                let d: f64 = (0..3).map(|i| ((c[i] * 7.0 - rc[i]) as f64).powi(2)).sum::<f64>().sqrt();
                d / size
            };
            if !o.transform_matrices.is_empty() {
                let body = if o.body_type == "bhkRigidBodyT" { o.body_iso() } else { Iso::IDENTITY };
                let err = |b: &Aabb| {
                    let (c, rc, e) = (b.center(), render.center(), b.extent());
                    (0..3).map(|i| ((c[i] * 7.0 - rc[i]).abs() + (e[i] * 7.0 - re[i]).abs()) as f64).sum::<f64>() / size
                };
                mat_fit.entry("as_read".into()).or_default().push(err(&bounds(&o.shape, &body)));
                let rotated = o.transform_matrices.iter().any(|m| {
                    let Some((iso, _)) = fnv_collide::nif::iso_from_mat44(m) else { return false };
                    let r = iso.rotation;
                    (r[1] - r[3]).abs() + (r[2] - r[6]).abs() + (r[5] - r[7]).abs() > 1e-3
                });
                if rotated && !ex.render_points.is_empty() {
                    let fit = |s: &Shape| {
                        let mut pts = Vec::new();
                        s.bound_points(&body, &mut |p| pts.push(p.map(|v| v * 7.0)));
                        let step = (pts.len() / 400).max(1);
                        let rstep = (ex.render_points.len() / 20000).max(1);
                        let sampled: Vec<_> = pts.iter().step_by(step).collect();
                        let total: f64 = sampled
                            .iter()
                            .map(|p| {
                                ex.render_points
                                    .iter()
                                    .step_by(rstep)
                                    .map(|q| (0..3).map(|i| ((p[i] - q[i]) as f64).powi(2)).sum::<f64>())
                                    .fold(f64::INFINITY, f64::min)
                                    .sqrt()
                            })
                            .sum();
                        total / sampled.len().max(1) as f64 / size
                    };
                    mat_vfit.push((it.path.clone(), fit(&o.shape), fit(&transposed(&o.shape))));
                }
                mat_fit.entry("transposed".into()).or_default().push(err(&bounds(&transposed(&o.shape), &body)));
            }
            centre_plain.entry(fam.clone()).or_default().push(off(&plain));
            centre_body.entry(fam).or_default().push(off(&with_body));
        }
    }
    Ok(json!({
        "format": "fnv.collide.survey/1",
        "files": files,
        "unreadable": unreadable,
        "files_with_collision": with_collision,
        "collision_objects": objects,
        "bodies": bodies,
        "body_transform": body_moved,
        "shape_trees": trees,
        "layers": layers,
        "motion_system": motion,
        "quality_type": quality,
        "transform_matrix_layout": mat_layout,
        "compressed_packed_vertices": compressed,
        "mixed_subpart_triangles": mixed,
        "shell_radius_by_layer": shells,
        "problems": problems,
        "scale": {
            "note": "single root collision object: render extent / stored extent along the render's largest axis",
            "by_family": ratio.into_iter().map(|(k, v)| (k, quantiles(v))).collect::<serde_json::Map<_, _>>(),
            "samples": ratio_samples,
            "centre_offset_x7_without_body_transform": centre_plain.into_iter().map(|(k, v)| (k, quantiles(v))).collect::<serde_json::Map<_, _>>(),
            "transform_shape_vertex_fit": {
                "meshes": mat_vfit.len(),
                "as_read_better": mat_vfit.iter().filter(|x| x.1 < x.2 * 0.9).count(),
                "transposed_better": mat_vfit.iter().filter(|x| x.2 < x.1 * 0.9).count(),
                "rows": mat_vfit.iter().map(|(p, a, b)| json!([p, a, b])).collect::<Vec<_>>(),
            },
            "transform_shape_fit_error": mat_fit.into_iter().map(|(k, v)| (k, quantiles(v))).collect::<serde_json::Map<_, _>>(),
            "centre_offset_x7_with_body_transform": centre_body.into_iter().map(|(k, v)| (k, quantiles(v))).collect::<serde_json::Map<_, _>>(),
        },
    }))
}

fn leaf_counts(s: &Shape, out: &mut BTreeMap<&'static str, u64>, hull: &mut u64, verts: &mut u64, tris: &mut u64) {
    let k = match s {
        Shape::Box { .. } => "box",
        Shape::Sphere { .. } => "sphere",
        Shape::Capsule { .. } => "capsule",
        Shape::ConvexHull { points } => {
            *hull += points.len() as u64;
            "convex_hull"
        }
        Shape::TriMesh { vertices, triangles, .. } => {
            *verts += vertices.len() as u64;
            *tris += triangles.len() as u64;
            "tri_mesh"
        }
        Shape::Compound { children } => {
            for (_, c) in children {
                leaf_counts(c, out, hull, verts, tris);
            }
            return;
        }
    };
    *out.entry(k).or_default() += 1;
}

fn f(v: f32) -> Value {
    fnv_nif::value::float_json(v)
}

/// Every solid collision object of every mesh in an archive, in game units
/// (fnv.collide.objects/1, contract: docs/spec-notes/collision.md §5).
fn objects(archive: &std::path::Path, schema: &Schema, filter: Option<&str>) -> anyhow::Result<Value> {
    let items = process(&[archive.to_path_buf()], &["nif"], filter, |_, data| analyse(data, schema))?;
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    let mut phantoms = Vec::new();
    for it in &items {
        let ex = match &it.result {
            Ok(Ok(e)) => e,
            Ok(Err(e)) | Err(e) => {
                skipped.push(json!({"path": it.path, "reason": e}));
                continue;
            }
        };
        if !ex.problems.is_empty() {
            skipped.push(json!({"path": it.path, "reason": ex.problems.join("; ")}));
            continue;
        }
        for (block, body_type) in &ex.phantoms {
            phantoms.push(json!({"key": format!("{}#{}", it.path, block), "body_type": body_type}));
        }
        for o in &ex.objects {
            let key = format!("{}#{}", it.path, o.object_block);
            let c = o.to_object();
            let mut leaves = BTreeMap::new();
            let (mut hull, mut verts, mut tris) = (0, 0, 0);
            leaf_counts(&c.shape, &mut leaves, &mut hull, &mut verts, &mut tris);
            let b = c.root_bounds();
            rows.push(json!({
                "key": key,
                "node": c.node,
                "node_block": c.node_block,
                "object_type": o.object_type,
                "body_type": o.body_type,
                "motion": motion(o.motion_system),
                "layer": c.layer,
                "mass": f(c.mass),
                "friction": f(c.friction),
                "restitution": f(c.restitution),
                "wind": c.wind,
                "shapes": leaves,
                "hull_points": hull,
                "mesh_vertices": verts,
                "mesh_triangles": tris,
                "aabb_min": if b.is_valid() { json!(b.min.map(f)) } else { Value::Null },
                "aabb_max": if b.is_valid() { json!(b.max.map(f)) } else { Value::Null },
            }));
        }
    }
    let name = archive.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(json!({
        "format": "fnv.collide.objects/1",
        "archive": name,
        "scale": f(fnv_collide::nif::HAVOK_SCALE),
        "objects": rows,
        "phantoms": phantoms,
        "skipped": skipped,
    }))
}

/// The jointed bodies of every mesh that has constraints (fnv.collide.ragdoll/1,
/// docs/spec-notes/ragdoll.md §5): observational, not a lab scenario.
fn ragdolls(sources: &[std::path::PathBuf], schema: &Schema, filter: Option<&str>, summary_only: bool) -> anyhow::Result<Value> {
    use fnv_collide::ragdoll::{ragdoll, JointKind, Ragdoll};
    let items = process(sources, &["nif"], filter, |_, data| -> Result<Ragdoll, String> {
        let file = NifFile::parse(data, Some(schema)).map_err(|e| e.to_string())?;
        if file.header.block_sizes.is_none() {
            return Err(format!("no block-size table (version {})", version_string(file.header.version)));
        }
        let dec = Decoded::new(&file, data, schema);
        Ok(ragdoll(&file, &dec))
    })?;
    let mut files = Vec::new();
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut inside: BTreeMap<String, u64> = BTreeMap::new();
    let mut flipped: BTreeMap<String, u64> = BTreeMap::new();
    let mut gaps = Vec::new();
    let mut stored_gaps = Vec::new();
    let mut gaps_t = Vec::new();
    let (mut meshes, mut bodies, mut jointed_t, mut problems) = (0u64, 0u64, 0u64, 0u64);
    for it in &items {
        let Ok(Ok(r)) = &it.result else { continue };
        if r.joints.is_empty() {
            continue;
        }
        meshes += 1;
        bodies += r.bodies.len() as u64;
        problems += r.problems.len() as u64;
        let rest: Vec<Iso> = r.bodies.iter().map(|b| b.rest()).collect();
        for b in &r.bodies {
            let p = b.object.node_transform.translation;
            stored_gaps.push((0..3).map(|i| f64::from(p[i] - b.stored.translation[i]).powi(2)).sum::<f64>().sqrt());
        }
        let mut rows = Vec::new();
        for j in &r.joints {
            let name = match j.kind {
                JointKind::Ragdoll { .. } => "ragdoll",
                JointKind::Hinge { limits: Some(_), .. } => "limited_hinge",
                JointKind::Hinge { .. } => "hinge",
                JointKind::Ball => "ball_and_socket",
                JointKind::Spring { .. } => "stiff_spring",
                JointKind::Other { .. } => "other",
            };
            let key = if j.strength.is_some() { format!("{name} (malleable)") } else { name.to_string() };
            bump(&mut kinds, key);
            let (a, b) = (&rest[j.a], &rest[j.b]);
            let angles = j.angles(a, b);
            let gap = j.pivot_gap(a, b);
            gaps.push(f64::from(gap));
            // The same joint read with the opposite sign of its angles: which reading
            // puts more bind poses inside their limits says which one the data follows.
            let mirrored = fnv_collide::ragdoll::JointAngles { cone: angles.cone, plane: -angles.plane, twist: -angles.twist };
            if j.within(&angles, 0.02) {
                bump(&mut inside, name);
            }
            if j.within(&mirrored, 0.02) {
                bump(&mut flipped, name);
            }
            let transformed = r.bodies[j.a].object.body_transform != Iso::IDENTITY || r.bodies[j.b].object.body_transform != Iso::IDENTITY;
            if transformed {
                jointed_t += 1;
                gaps_t.push(f64::from(gap));
            }
            rows.push(json!({
                "block": j.block,
                "a": r.bodies[j.a].object.node,
                "b": r.bodies[j.b].object.node,
                "joint": j.kind,
                "max_friction": f(j.max_friction),
                "strength": j.strength.map(f),
                "pivot_gap": f(gap),
                "transformed_body": transformed,
                "bind": {"cone": f(angles.cone), "plane": f(angles.plane), "twist": f(angles.twist)},
                "bind_within_limits": j.within(&angles, 0.02),
            }));
        }
        if !summary_only {
            files.push(json!({
                "path": it.path,
                "bodies": r.bodies.iter().map(|b| json!({
                    "node": b.object.node,
                    "layer": b.object.layer,
                    "mass": f(b.object.mass),
                    "friction": f(b.object.friction),
                    "restitution": f(b.object.restitution),
                    "linear_damping": f(b.linear_damping),
                    "angular_damping": f(b.angular_damping),
                })).collect::<Vec<_>>(),
                "joints": rows,
                "problems": r.problems,
            }));
        }
    }
    Ok(json!({
        "format": "fnv.collide.ragdoll/1",
        "summary": {
            "meshes_with_joints": meshes,
            "bodies": bodies,
            "joints": kinds,
            "bind_within_limits": inside,
            "bind_within_limits_opposite_sign": flipped,
            "joints_on_transformed_bodies": jointed_t,
            "pivot_gap": quantiles(gaps),
            "pivot_gap_on_transformed_bodies": quantiles(gaps_t),
            "stored_translation_to_bind": quantiles(stored_gaps),
            "problems": problems,
        },
        "files": files,
    }))
}
