//! Generated distant terrain: LOD chunks built from the user's own LAND records at a
//! higher resolution than the game's level-4 LOD (docs/design/generated-lod.md).
//!
//! Pure parts (no ECS): gathering a chunk's landscape, the mesh, the diffuse and normal
//! bakes, mip chains, BC1 + DDS encoding and the cache. `lod.rs` schedules and draws.
//! Generated data is game-derived: it is only ever written to NVForever's cache.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use fnv_dds::{to_rgba8, Dds, Format};
use fnv_esm::{LoadOrder, Schema};
use fnv_world::land::{land_texture, load_land, LandParts, DEFAULT_DIFFUSE, QUAD_SIDE, SIDE, SPACING};
use fnv_world::{WorldIndex, CELL_SIZE};
use nvf_assets::GameData;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Bumped whenever the generated output changes for the same inputs (part of the cache key).
pub const GENERATOR_VERSION: u32 = 3;
/// Cells per chunk side (the game's level-4 grid, world.md §7).
pub const CHUNK_CELLS: i32 = 4;
const CHUNK_SIZE: f32 = CHUNK_CELLS as f32 * CELL_SIZE;
/// Depth of the skirts hung from open mesh edges (game units).
const SKIRT: f32 = 256.0;
/// Colour where a base texture cannot be loaded (as terrain.rs draws it, sRGB).
const NO_BASE_SRGB: [f32; 3] = [0.45, 0.42, 0.38];

/// Generation parameters (also part of the cache key).
#[derive(Clone, Debug)]
pub struct GenParams {
    /// Diffuse texels per chunk side; the normal map has half as many.
    pub res: usize,
    /// Mesh vertex step in LAND grid vertices (must divide 32).
    pub step: usize,
    /// World units per terrain texture repeat (terrain.rs).
    pub tile: f32,
}

/// One texture level in linear RGB, for bilinear sampling with wrap-around.
pub struct Mip {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 3]>,
}

impl Mip {
    fn sample(&self, u: f32, v: f32) -> [f32; 3] {
        let (x, y) = (u * self.w as f32 - 0.5, v * self.h as f32 - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let wrap = |i: f32, n: usize| (i as i64).rem_euclid(n as i64) as usize;
        let (xa, xb, ya, yb) = (wrap(x0, self.w), wrap(x0 + 1.0, self.w), wrap(y0, self.h), wrap(y0 + 1.0, self.h));
        let p = |x: usize, y: usize| self.px[y * self.w + x];
        let (a, b, c, d) = (p(xa, ya), p(xb, ya), p(xa, yb), p(xb, yb));
        std::array::from_fn(|k| {
            let top = a[k] + (b[k] - a[k]) * fx;
            let bot = c[k] + (d[k] - c[k]) * fx;
            top + (bot - top) * fy
        })
    }
}

/// One quadrant's texturing: base texture, then layers with opacity per 17 × 17 vertex.
#[derive(Default, Clone)]
pub struct Quad {
    pub base: Option<Arc<Mip>>,
    pub layers: Vec<(Arc<Mip>, Vec<f32>)>,
}

/// The landscape of one cell, as the generator needs it.
#[derive(Clone)]
pub struct CellLand {
    /// 33 × 33 heights (rows south → north), game units.
    pub heights: Vec<f32>,
    pub normals: Option<Vec<[i8; 3]>>,
    pub colors: Option<Vec<[u8; 3]>>,
    /// Quadrants 0 south-west, 1 south-east, 2 north-west, 3 north-east.
    pub quads: [Quad; 4],
}

/// A chunk's 16 cells, index `y * 4 + x` from the south-west cell.
pub type ChunkCells = Vec<Option<CellLand>>;

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Bilinear value of a square grid of `n` × `n` samples at fractional grid position (gx, gy).
fn grid_bilinear<const K: usize>(n: usize, gx: f32, gy: f32, at: impl Fn(usize) -> [f32; K]) -> [f32; K] {
    let max = (n - 1) as f32;
    let (gx, gy) = (gx.clamp(0.0, max), gy.clamp(0.0, max));
    let (x0, y0) = ((gx as usize).min(n - 2), (gy as usize).min(n - 2));
    let (fx, fy) = (gx - x0 as f32, gy - y0 as f32);
    let (a, b, c, d) = (at(y0 * n + x0), at(y0 * n + x0 + 1), at((y0 + 1) * n + x0), at((y0 + 1) * n + x0 + 1));
    std::array::from_fn(|k| {
        let s = a[k] + (b[k] - a[k]) * fx;
        let t = c[k] + (d[k] - c[k]) * fx;
        s + (t - s) * fy
    })
}

/// Chunk-local game position of the centre of texel (i, j) in a `res` image whose row 0
/// is the chunk's north edge.
fn texel_pos(i: usize, j: usize, res: usize) -> (f32, f32) {
    let s = CHUNK_SIZE / res as f32;
    ((i as f32 + 0.5) * s, CHUNK_SIZE - (j as f32 + 0.5) * s)
}

/// The cell under a chunk-local position and the position within it.
fn cell_at(x: f32, y: f32) -> (usize, f32, f32) {
    let cx = ((x / CELL_SIZE) as usize).min(CHUNK_CELLS as usize - 1);
    let cy = ((y / CELL_SIZE) as usize).min(CHUNK_CELLS as usize - 1);
    (cy * CHUNK_CELLS as usize + cx, x - cx as f32 * CELL_SIZE, y - cy as f32 * CELL_SIZE)
}

/// Diffuse bake in linear RGB, `res` × `res`, row 0 north (generated-lod.md §2).
/// `origin` is the chunk's south-west corner in world units (for the texture repeat).
pub fn bake_diffuse(cells: &ChunkCells, res: usize, tile: f32, origin: Vec2) -> Vec<[f32; 3]> {
    let no_base = NO_BASE_SRGB.map(srgb_to_linear);
    let half = CELL_SIZE / 2.0;
    let mut out = vec![no_base; res * res];
    for j in 0..res {
        for i in 0..res {
            let (x, y) = texel_pos(i, j, res);
            let (c, lx, ly) = cell_at(x, y);
            let Some(cell) = &cells[c] else { continue };
            let q = usize::from(lx >= half) + 2 * usize::from(ly >= half);
            let quad = &cell.quads[q];
            // Same repeat as the loaded terrain: uv = (wx, wy) / tile.
            let (u, v) = ((origin.x + x) / tile, (origin.y + y) / tile);
            let mut col = quad.base.as_ref().map_or(no_base, |m| m.sample(u, v));
            let (qx, qy) = ((lx - if q % 2 == 1 { half } else { 0.0 }) / SPACING, (ly - if q >= 2 { half } else { 0.0 }) / SPACING);
            for (tex, alpha) in &quad.layers {
                let [a] = grid_bilinear(QUAD_SIDE, qx, qy, |k| [alpha[k]]);
                if a > 0.0 {
                    let t = tex.sample(u, v);
                    col = std::array::from_fn(|k| col[k] + (t[k] - col[k]) * a);
                }
            }
            if let Some(vc) = &cell.colors {
                let tint = grid_bilinear(SIDE, lx / SPACING, ly / SPACING, |k| vc[k].map(|b| srgb_to_linear(b as f32 / 255.0)));
                col = std::array::from_fn(|k| col[k] * tint[k]);
            }
            out[j * res + i] = col;
        }
    }
    out
}

/// Model-space normal bake (x east, y north, z up), `res` × `res`, row 0 north.
pub fn bake_normals(cells: &ChunkCells, res: usize) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0, 0.0, 1.0]; res * res];
    for j in 0..res {
        for i in 0..res {
            let (x, y) = texel_pos(i, j, res);
            let (c, lx, ly) = cell_at(x, y);
            let Some(n) = cells[c].as_ref().and_then(|c| c.normals.as_ref()) else { continue };
            let v = grid_bilinear(SIDE, lx / SPACING, ly / SPACING, |k| n[k].map(|b| b as f32));
            out[j * res + i] = Vec3::from_array(v).normalize_or(Vec3::Z).to_array();
        }
    }
    out
}

/// Mip chain by 2 × 2 box filtering down to 1 × 1; `renormalize` for normal maps.
pub fn mip_chain(base: Vec<[f32; 3]>, res: usize, renormalize: bool) -> Vec<(usize, Vec<[f32; 3]>)> {
    let mut levels = vec![(res, base)];
    while levels.last().unwrap().0 > 1 {
        let (n, prev) = levels.last().unwrap();
        let m = n / 2;
        let mut next = Vec::with_capacity(m * m);
        for y in 0..m {
            for x in 0..m {
                let p = |dx: usize, dy: usize| prev[(2 * y + dy) * n + 2 * x + dx];
                let (a, b, c, d) = (p(0, 0), p(1, 0), p(0, 1), p(1, 1));
                let mut v: [f32; 3] = std::array::from_fn(|k| (a[k] + b[k] + c[k] + d[k]) * 0.25);
                if renormalize {
                    v = Vec3::from_array(v).normalize_or(Vec3::Z).to_array();
                }
                next.push(v);
            }
        }
        levels.push((m, next));
    }
    levels
}

/// A BC1 DDS file (fourCC DXT1, full mip chain) [documented: Microsoft "DDS_HEADER" and
/// "DDS_PIXELFORMAT" reference; docs/spec-notes/dds.md]. Levels are square RGBA8.
pub fn encode_dds_bc1(levels: &[(usize, Vec<[u8; 4]>)]) -> Vec<u8> {
    let fmt = texpresso::Format::Bc1;
    let params = texpresso::Params { algorithm: texpresso::Algorithm::RangeFit, ..Default::default() };
    let n = levels[0].0;
    let mut out = Vec::new();
    let u32le = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    out.extend_from_slice(b"DDS ");
    u32le(&mut out, 124);
    // CAPS | HEIGHT | WIDTH | PIXELFORMAT | MIPMAPCOUNT | LINEARSIZE
    u32le(&mut out, 0x1 | 0x2 | 0x4 | 0x1000 | 0x2_0000 | 0x8_0000);
    u32le(&mut out, n as u32);
    u32le(&mut out, n as u32);
    u32le(&mut out, fmt.compressed_size(n, n) as u32);
    u32le(&mut out, 0); // depth
    u32le(&mut out, levels.len() as u32);
    for _ in 0..11 {
        u32le(&mut out, 0);
    }
    // DDS_PIXELFORMAT: size 32, flags FOURCC, "DXT1", no bit counts or masks.
    u32le(&mut out, 32);
    u32le(&mut out, 0x4);
    out.extend_from_slice(b"DXT1");
    for _ in 0..5 {
        u32le(&mut out, 0);
    }
    // TEXTURE | COMPLEX | MIPMAP; caps2-4 and reserved2 zero.
    u32le(&mut out, 0x1000 | 0x8 | 0x40_0000);
    for _ in 0..4 {
        u32le(&mut out, 0);
    }
    for (side, px) in levels {
        let mut block = vec![0u8; fmt.compressed_size(*side, *side)];
        fmt.compress(px.as_flattened(), *side, *side, params, &mut block);
        out.extend_from_slice(&block);
    }
    out
}

/// Diffuse levels as sRGB RGBA8.
pub fn diffuse_rgba(levels: Vec<(usize, Vec<[f32; 3]>)>) -> Vec<(usize, Vec<[u8; 4]>)> {
    let q = |c: f32| (linear_to_srgb(c) * 255.0 + 0.5) as u8;
    levels.into_iter().map(|(n, px)| (n, px.iter().map(|c| [q(c[0]), q(c[1]), q(c[2]), 255]).collect())).collect()
}

/// Normal levels as RGBA8, channel = n × 0.5 + 0.5.
pub fn normal_rgba(levels: Vec<(usize, Vec<[f32; 3]>)>) -> Vec<(usize, Vec<[u8; 4]>)> {
    let q = |c: f32| ((c.clamp(-1.0, 1.0) * 0.5 + 0.5) * 255.0 + 0.5) as u8;
    levels.into_iter().map(|(n, px)| (n, px.iter().map(|c| [q(c[0]), q(c[1]), q(c[2]), 255]).collect())).collect()
}

/// A cell edge for skirts: neighbour offset, edge vertices (row, column) in order.
type Edge = ((i32, i32), Vec<(usize, usize)>);

/// The chunk's terrain mesh in chunk-local game space: one `32 / step + 1` square vertex
/// grid per cell that has heights and is `keep`, skirts on edges whose neighbour cell is
/// not drawn, an up normal and east tangent per vertex (model-space normal map, lod.rs),
/// UVs over the whole chunk with v = 0 at the north edge.
pub fn chunk_mesh(heights: &[Option<&[f32]>], keep: &[bool], step: usize) -> Option<Mesh> {
    let n = (SIDE - 1) / step + 1;
    let cells = CHUNK_CELLS as usize;
    let drawn = |cx: i32, cy: i32| {
        (0..cells as i32).contains(&cx) && (0..cells as i32).contains(&cy) && {
            let c = cy as usize * cells + cx as usize;
            keep[c] && heights[c].is_some()
        }
    };
    let (mut pos, mut uv, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    let mut push = |p: [f32; 3], pos: &mut Vec<[f32; 3]>| {
        pos.push(p);
        uv.push([p[0] / CHUNK_SIZE, 1.0 - p[1] / CHUNK_SIZE]);
        (pos.len() - 1) as u32
    };
    for cy in 0..cells {
        for cx in 0..cells {
            let c = cy * cells + cx;
            let Some(h) = heights[c].filter(|_| keep[c]) else { continue };
            let (ox, oy) = (cx as f32 * CELL_SIZE, cy as f32 * CELL_SIZE);
            let first = pos.len() as u32;
            for r in 0..n {
                for k in 0..n {
                    let (gr, gc) = (r * step, k * step);
                    push([ox + gc as f32 * SPACING, oy + gr as f32 * SPACING, h[gr * SIDE + gc]], &mut pos);
                }
            }
            let at = |r: usize, k: usize| first + (r * n + k) as u32;
            for r in 0..n - 1 {
                for k in 0..n - 1 {
                    // Counter-clockwise from above, split south-west → north-east (terrain.rs).
                    let (i, e, nn, ne) = (at(r, k), at(r, k + 1), at(r + 1, k), at(r + 1, k + 1));
                    idx.extend_from_slice(&[i, e, ne, i, ne, nn]);
                }
            }
            // Skirts: (neighbour offset, edge vertices in order with the outside on the right).
            let edges: [Edge; 4] = [
                ((0, -1), (0..n).map(|k| (0, k)).collect()),
                ((1, 0), (0..n).map(|r| (r, n - 1)).collect()),
                ((0, 1), (0..n).rev().map(|k| (n - 1, k)).collect()),
                ((-1, 0), (0..n).rev().map(|r| (r, 0)).collect()),
            ];
            for ((dx, dy), verts) in edges {
                if drawn(cx as i32 + dx, cy as i32 + dy) {
                    continue;
                }
                for w in verts.windows(2) {
                    let (a, b) = (at(w[0].0, w[0].1), at(w[1].0, w[1].1));
                    let (pa, pb) = (pos[a as usize], pos[b as usize]);
                    let a2 = push([pa[0], pa[1], pa[2] - SKIRT], &mut pos);
                    let b2 = push([pb[0], pb[1], pb[2] - SKIRT], &mut pos);
                    // Faces outwards: a → b runs with the cell's interior on the left.
                    idx.extend_from_slice(&[a, a2, b2, a, b2, b]);
                }
            }
        }
    }
    if idx.is_empty() {
        return None;
    }
    let count = pos.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0, 0.0, 0.0, 1.0]; count]);
    mesh.insert_indices(Indices::U32(idx));
    Some(mesh)
}

/// 64-bit FNV-1a, stable across runs and builds (cache keys).
pub fn fnv1a(bytes: &[u8], mut h: u64) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}
pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// Cache key over the generator version, parameters, worldspace and load order (file
/// name, size and modification time of each plugin), plus a human-readable manifest.
pub fn cache_key(params: &GenParams, world: &str, data_dir: &Path, plugins: &[String]) -> (String, String) {
    let mut manifest = format!(
        "NVForever generated LOD (docs/design/generated-lod.md). Game-derived: do not share.\n\
         generator {GENERATOR_VERSION}\nres {}\ntile {}\nworld {world}\n",
        params.res, params.tile
    );
    for p in plugins {
        let meta = std::fs::metadata(fnv_install::child_ci(data_dir, p).unwrap_or_else(|| data_dir.join(p))).ok();
        let size = meta.as_ref().map_or(0, |m| m.len());
        let mtime = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        manifest.push_str(&format!("plugin {p} {size} {mtime}\n"));
    }
    (format!("{:016x}", fnv1a(manifest.as_bytes(), FNV_OFFSET)), manifest)
}

/// Where generated chunks are cached, and the guard that keeps writes out of the game.
#[derive(Clone)]
pub struct GenCache {
    pub dir: PathBuf,
    pub manifest: String,
    pub game: Option<fnv_install::GameInstall>,
}

impl GenCache {
    fn file(&self, chunk: (i32, i32), kind: &str) -> PathBuf {
        self.dir.join(format!("x{}.y{}.{kind}.dds", chunk.0, chunk.1))
    }

    fn read(&self, chunk: (i32, i32)) -> Option<(Vec<u8>, Vec<u8>)> {
        let d = std::fs::read(self.file(chunk, "diffuse")).ok()?;
        let n = std::fs::read(self.file(chunk, "normals")).ok()?;
        (Dds::parse(&d).is_ok() && Dds::parse(&n).is_ok()).then_some((d, n))
    }

    /// Write via a temporary name and rename, after the install guard.
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        fnv_install::guard_write(path, self.game.as_ref()).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let manifest = self.dir.join("manifest.txt");
        if !manifest.exists() {
            fnv_install::guard_write(&manifest, self.game.as_ref()).map_err(|e| e.to_string())?;
            std::fs::write(&manifest, &self.manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// A decoded level, or `None` when the texture could not be read.
type MipSlot = Option<Arc<Mip>>;

/// Decoded terrain textures shared by bake tasks: (path, target size) → level.
#[derive(Default)]
pub struct MipStore {
    map: Mutex<HashMap<(String, usize), MipSlot>>,
}

impl MipStore {
    /// The smallest level of a texture at least `target` texels wide, in linear RGB.
    fn get(&self, data: &GameData, raw: &str, target: usize) -> Option<Arc<Mip>> {
        let key = (fnv_vfs::texture_path(raw), target);
        if let Some(m) = self.map.lock().unwrap().get(&key) {
            return m.clone();
        }
        let mip = data.read(&key.0).and_then(|b| decode_level(&b, target)).map(Arc::new);
        self.map.lock().unwrap().insert(key, mip.clone());
        mip
    }
}

/// Decode the chosen level of a DDS texture to linear RGB.
fn decode_level(bytes: &[u8], target: usize) -> Option<Mip> {
    let dds = Dds::parse(bytes).ok()?;
    let levels = dds.levels.first()?;
    let mut i = 0;
    while i + 1 < levels.len() && (dds.header.level_dims(i as u32 + 1).0 as usize) >= target {
        i += 1;
    }
    let (w, h) = dds.header.level_dims(i as u32);
    let (w, h) = (w as usize, h as usize);
    let bc = match dds.format {
        Format::Dxt1 => Some(texpresso::Format::Bc1),
        Format::Dxt3 => Some(texpresso::Format::Bc2),
        Format::Dxt5 => Some(texpresso::Format::Bc3),
        _ => None,
    };
    let rgba = match bc {
        Some(f) => {
            let mut out = vec![0u8; w * h * 4];
            f.decompress(levels[i], w, h, &mut out);
            out
        }
        None => to_rgba8(dds.format, levels[i])?,
    };
    let lin: Vec<f32> = (0..=255).map(|b| srgb_to_linear(b as f32 / 255.0)).collect();
    let px = rgba.as_chunks::<4>().0.iter().map(|p| [lin[p[0] as usize], lin[p[1] as usize], lin[p[2] as usize]]).collect();
    Some(Mip { w, h, px })
}

/// Everything a bake task needs; cheap to clone per chunk.
#[derive(Clone)]
pub struct GenJob {
    pub lo: &'static LoadOrder<'static>,
    pub schema: &'static Schema,
    pub index: &'static WorldIndex,
    pub world: u32,
    pub data: GameData,
    pub params: GenParams,
    pub cache: Option<GenCache>,
    pub mips: Arc<MipStore>,
}

/// A generated chunk ready for upload: per-cell heights and the two DDS files.
pub struct Baked {
    pub heights: Vec<Option<Vec<f32>>>,
    pub diffuse: Vec<u8>,
    pub normals: Vec<u8>,
    /// Whether it was baked now (false: read from the cache).
    pub baked: bool,
}

impl GenJob {
    /// Gather, bake (or read from the cache) one chunk. `Ok(None)`: no LAND in it.
    pub fn run(&self, chunk: (i32, i32)) -> Result<Option<Baked>, String> {
        let cached = self.cache.as_ref().and_then(|c| c.read(chunk));
        let parts = if cached.is_some() { LandParts::HEIGHTS } else { LandParts::ALL };
        let cells = self.gather(chunk, parts);
        if cells.iter().all(Option::is_none) {
            return Ok(None);
        }
        let heights = cells.iter().map(|c| c.as_ref().map(|c| c.heights.clone())).collect();
        if let Some((diffuse, normals)) = cached {
            return Ok(Some(Baked { heights, diffuse, normals, baked: false }));
        }
        let res = self.params.res;
        let origin = Vec2::new(chunk.0 as f32 * CELL_SIZE, chunk.1 as f32 * CELL_SIZE);
        let diffuse = encode_dds_bc1(&diffuse_rgba(mip_chain(bake_diffuse(&cells, res, self.params.tile, origin), res, false)));
        let normals = encode_dds_bc1(&normal_rgba(mip_chain(bake_normals(&cells, res / 2), res / 2, true)));
        if let Some(c) = &self.cache {
            c.write(&c.file(chunk, "diffuse"), &diffuse)?;
            c.write(&c.file(chunk, "normals"), &normals)?;
        }
        Ok(Some(Baked { heights, diffuse, normals, baked: true }))
    }

    fn gather(&self, chunk: (i32, i32), parts: LandParts) -> ChunkCells {
        let grid = self.index.worldspaces.get(&self.world).map(|w| &w.grid);
        // Bake texels per texture repeat: pick the texture level closest above it.
        let target = ((self.params.tile * self.params.res as f32 / CHUNK_SIZE).ceil() as usize).max(1);
        let mut ltex: HashMap<u32, Option<String>> = HashMap::new();
        let mut cells = Vec::with_capacity(16);
        for cy in 0..CHUNK_CELLS {
            for cx in 0..CHUNK_CELLS {
                let land = grid
                    .and_then(|g| g.get(&(chunk.0 + cx, chunk.1 + cy)))
                    .and_then(|id| self.index.cells.get(id))
                    .and_then(|c| c.land)
                    .and_then(|l| load_land(self.lo, self.schema, l, parts));
                let Some(land) = land else {
                    cells.push(None);
                    continue;
                };
                let Some(heights) = land.heights.clone() else {
                    cells.push(None);
                    continue;
                };
                let mut tex = |id: Option<u32>| -> Option<Arc<Mip>> {
                    // No LTEX (no BTXT, or a null layer texture): the default, as terrain.rs.
                    let path = match id {
                        None => DEFAULT_DIFFUSE.to_string(),
                        Some(id) => ltex
                            .entry(id)
                            .or_insert_with(|| land_texture(self.lo, self.schema, id).and_then(|t| t.diffuse))
                            .clone()?,
                    };
                    self.mips.get(&self.data, &path, target)
                };
                let quads = std::array::from_fn(|q| {
                    let q = q as u8;
                    let base = land.layers.iter().rev().find(|l| l.base && l.quadrant == q);
                    let mut layers: Vec<_> = land.layers.iter().filter(|l| !l.base && l.quadrant == q).collect();
                    layers.sort_by_key(|l| l.layer);
                    Quad {
                        base: base.and_then(|b| tex(b.texture)),
                        layers: layers
                            .into_iter()
                            .filter_map(|l| {
                                let t = tex(l.texture)?;
                                let mut alpha = vec![0.0f32; QUAD_SIDE * QUAD_SIDE];
                                for &(p, o) in &l.alpha {
                                    if let Some(a) = alpha.get_mut(p as usize) {
                                        *a = o.clamp(0.0, 1.0);
                                    }
                                }
                                Some((t, alpha))
                            })
                            .collect(),
                    }
                });
                cells.push(Some(CellLand { heights, normals: land.normals, colors: land.colors, quads }));
            }
        }
        cells
    }
}

/// A finished bake: the chunk and what [`GenJob::run`] returned for it.
pub type BakeResult = ((i32, i32), Result<Option<Baked>, String>);

/// Bake threads of their own, at the lowest thread priority, so generated terrain only
/// uses cores the frame leaves free. Bakes on Bevy's async compute pool held a frame until
/// they were all done: Bevy compiles render pipelines on that pool and the render thread
/// waits for some of them (docs/design/streaming-hitches.md §3) [observed].
pub struct Baker {
    shared: Arc<BakerShared>,
}

struct BakerShared {
    state: Mutex<BakerQueue>,
    wake: std::sync::Condvar,
}

#[derive(Default)]
struct BakerQueue {
    /// Chunks waiting, the next to bake first.
    waiting: std::collections::VecDeque<(i32, i32)>,
    /// Finished bakes, oldest first.
    done: std::collections::VecDeque<BakeResult>,
    closed: bool,
}

impl Baker {
    pub fn new(job: GenJob, threads: usize) -> Baker {
        let shared = Arc::new(BakerShared { state: Mutex::new(BakerQueue::default()), wake: Default::default() });
        for i in 0..threads.max(1) {
            let (shared, job) = (shared.clone(), job.clone());
            let spawned = std::thread::Builder::new().name(format!("lodgen {i}")).spawn(move || {
                lowest_thread_priority();
                while let Some(chunk) = shared.next() {
                    let result = job.run(chunk);
                    shared.state.lock().unwrap().done.push_back((chunk, result));
                }
            });
            if let Err(e) = spawned {
                warn!("generated LOD: cannot start a bake thread: {e}");
            }
        }
        Baker { shared }
    }

    /// Make `want` (the next to bake first) the waiting chunks. `pending`: the chunks
    /// handed to the baker whose results have not been taken; chunks in it that are being
    /// baked or are done are not queued again, and waiting chunks left out of `want`
    /// leave it.
    pub fn schedule(&self, want: &[(i32, i32)], pending: &mut HashSet<(i32, i32)>) {
        let mut s = self.shared.state.lock().unwrap();
        let was: HashSet<(i32, i32)> = s.waiting.drain(..).collect();
        for c in &was {
            pending.remove(c);
        }
        s.waiting = want.iter().copied().filter(|c| was.contains(c) || !pending.contains(c)).collect();
        pending.extend(s.waiting.iter().copied());
        self.shared.wake.notify_all();
    }

    /// A finished bake, if any.
    pub fn try_recv(&self) -> Option<BakeResult> {
        self.shared.state.lock().unwrap().done.pop_front()
    }
}

impl Drop for Baker {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().closed = true;
        self.shared.wake.notify_all();
    }
}

impl BakerShared {
    /// The next chunk to bake, waiting for one; `None` once the baker is dropped.
    fn next(&self) -> Option<(i32, i32)> {
        let mut s = self.state.lock().unwrap();
        loop {
            if s.closed {
                return None;
            }
            if let Some(c) = s.waiting.pop_front() {
                return Some(c);
            }
            s = self.wake.wait(s).unwrap();
        }
    }
}

/// Run the calling thread only when no other thread wants its core.
fn lowest_thread_priority() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_LOWEST};
        // SAFETY: GetCurrentThread returns a pseudo-handle for this thread; no memory is shared.
        if unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST) } == 0 {
            warn!("generated LOD: cannot lower the bake thread's priority");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn flat_mip(rgb: [f32; 3]) -> Arc<Mip> {
        Arc::new(Mip { w: 4, h: 4, px: vec![rgb; 16] })
    }

    fn cell(height: f32) -> CellLand {
        CellLand { heights: vec![height; SIDE * SIDE], normals: None, colors: None, quads: Default::default() }
    }

    #[test]
    fn mesh_grid_cut_out_and_skirts() {
        let hs: Vec<Vec<f32>> = (0..16).map(|c| vec![c as f32; SIDE * SIDE]).collect();
        let heights: Vec<Option<&[f32]>> = hs.iter().map(|h| Some(h.as_slice())).collect();
        let all = chunk_mesh(&heights, &[true; 16], 2).unwrap();
        let n = 17;
        // 16 cells of 17 × 17 vertices; skirts only along the chunk border (4 × 4 edges).
        let border_skirt_verts = 16 * (n - 1) * 2;
        assert_eq!(all.count_vertices(), 16 * n * n + border_skirt_verts);
        let Some(VertexAttributeValues::Float32x3(p)) = all.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        // The chunk's north-east corner is drawn at cell (3, 3)'s height, and its skirt below.
        assert!(p.contains(&[CHUNK_SIZE, CHUNK_SIZE, 15.0]));
        assert!(p.contains(&[CHUNK_SIZE, CHUNK_SIZE, 15.0 - SKIRT]));
        // Cut out cell (1, 1): its 4 neighbours each gain one skirt edge.
        let mut keep = [true; 16];
        keep[5] = false;
        let cut = chunk_mesh(&heights, &keep, 2).unwrap();
        assert_eq!(cut.count_vertices(), 15 * n * n + border_skirt_verts + 4 * (n - 1) * 2);
        assert!(chunk_mesh(&heights, &[false; 16], 2).is_none());
    }

    #[test]
    fn diffuse_layers_and_vertex_colour() {
        let mut c = cell(0.0);
        let (red, blue) = ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        // Quadrant 0: base red, layer blue at full opacity on its east half.
        let alpha: Vec<f32> = (0..QUAD_SIDE * QUAD_SIDE).map(|k| if k % QUAD_SIDE >= 12 { 1.0 } else { 0.0 }).collect();
        c.quads[0] = Quad { base: Some(flat_mip(red)), layers: vec![(flat_mip(blue), alpha)] };
        // Vertex colour: mid grey (sRGB 188 ≈ linear 0.5) everywhere.
        c.colors = Some(vec![[188, 188, 188]; SIDE * SIDE]);
        let mut cells: ChunkCells = vec![None; 16];
        cells[0] = Some(c);
        let res = 64; // 256 units per texel; cell 0 = columns 0..16, rows 48..64.
        let img = bake_diffuse(&cells, res, 682.0, Vec2::ZERO);
        let tint = srgb_to_linear(188.0 / 255.0);
        let west = img[60 * res + 1];
        let east = img[60 * res + 7];
        assert!((west[0] - tint).abs() < 1e-3 && west[2] < 1e-3, "{west:?}");
        assert!((east[2] - tint).abs() < 1e-3 && east[0] < 1e-3, "{east:?}");
        // Cells without LAND keep the no-base colour.
        assert_eq!(img[0], NO_BASE_SRGB.map(srgb_to_linear));
    }

    #[test]
    fn normals_channels() {
        let mut c = cell(0.0);
        // Slope facing east: x positive.
        c.normals = Some(vec![[90, 0, 90]; SIDE * SIDE]);
        let mut cells: ChunkCells = vec![None; 16];
        cells[0] = Some(c);
        let img = bake_normals(&cells, 16);
        let v = img[15 * 16];
        assert!((v[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3 && v[1].abs() < 1e-6, "{v:?}");
        let rgba = normal_rgba(vec![(1, vec![v])]);
        assert_eq!(rgba[0].1[0], [218, 128, 218, 255]);
    }

    #[test]
    fn dds_round_trip() {
        let px: Vec<[f32; 3]> = (0..64).map(|i| [i as f32 / 64.0, 0.5, 0.25]).collect();
        let levels = diffuse_rgba(mip_chain(px, 8, false));
        assert_eq!(levels.iter().map(|l| l.0).collect::<Vec<_>>(), [8, 4, 2, 1]);
        let bytes = encode_dds_bc1(&levels);
        let dds = Dds::parse(&bytes).expect("fnv-dds reads our header");
        assert_eq!(dds.format, Format::Dxt1);
        assert_eq!((dds.header.width, dds.header.height, dds.header.levels()), (8, 8, 4));
        let m = decode_level(&bytes, 8).unwrap();
        assert_eq!((m.w, m.h), (8, 8));
        let m = decode_level(&bytes, 3).unwrap();
        assert_eq!((m.w, m.h), (4, 4), "smallest level at least 3 wide");
    }

    #[test]
    fn cache_key_depends_on_inputs() {
        let p = GenParams { res: 1024, step: 2, tile: 682.0 };
        let dir = Path::new("does-not-exist");
        let a = cache_key(&p, "WastelandNV", dir, &["A.esm".into()]).0;
        assert_ne!(a, cache_key(&GenParams { res: 512, ..p.clone() }, "WastelandNV", dir, &["A.esm".into()]).0);
        assert_ne!(a, cache_key(&p, "Other", dir, &["A.esm".into()]).0);
        assert_ne!(a, cache_key(&p, "WastelandNV", dir, &["A.esm".into(), "B.esm".into()]).0);
        assert_eq!(a, cache_key(&p, "WastelandNV", dir, &["A.esm".into()]).0);
    }

    #[test]
    fn cache_refuses_game_folder() {
        let tmp = std::env::temp_dir().join(format!("nvf-lodgen-test-{}", std::process::id()));
        let game_root = tmp.join("game");
        std::fs::create_dir_all(game_root.join("Data")).unwrap();
        std::fs::write(game_root.join("Data").join("FalloutNV.esm"), b"x").unwrap();
        let game = fnv_install::GameInstall::open(&game_root).unwrap();
        let bad = GenCache { dir: game_root.join("Data").join("lod"), manifest: String::new(), game: Some(game.clone()) };
        assert!(bad.write(&bad.file((0, 0), "diffuse"), b"x").is_err());
        assert!(!game_root.join("Data").join("lod").exists());
        let good = GenCache { dir: tmp.join("cache"), manifest: "m".into(), game: Some(game) };
        good.write(&good.file((0, 0), "diffuse"), b"x").unwrap();
        assert_eq!(std::fs::read(tmp.join("cache").join("x0.y0.diffuse.dds")).unwrap(), b"x");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
