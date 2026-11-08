//! DDS → GPU textures. Block-compressed data is uploaded as is (BC1/BC2/BC3 with its
//! mip chain); the three uncompressed layouts are converted to RGBA8.

use crate::data::GameData;
use bevy::asset::RenderAssetUsages;
use bevy::image::{
    CompressedImageFormatSupport, CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler,
    ImageSamplerDescriptor,
};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::asset::AssetHandleProvider;
use fnv_dds::{to_rgba8, Dds, Format};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

/// Loaded textures by (VFS path, sRGB); `None` remembers a failure.
#[derive(Resource, Default)]
pub struct TextureCache {
    map: HashMap<(String, bool), Option<Handle<Image>>>,
    /// Copies sampled with clamped edges ([`Self::get_clamped`]), by the same key.
    clamped: HashMap<(String, bool), Option<Handle<Image>>>,
    /// Files read ahead on a worker thread ([`Self::offer`]), by resolved path, with when
    /// they came in.
    ahead: HashMap<String, (Vec<u8>, f64)>,
    /// Textures decoded on worker threads for models prepared there
    /// (`ModelCtx::models_ready`), shared with them.
    pub(crate) workers: Arc<WorkerTextures>,
    /// Counts changes to `map`, so a copy of it for worker threads is made again only
    /// when it changed ([`Self::snapshot`]).
    generation: u64,
    pub failures: Vec<String>,
}

/// Seconds a texture read ahead waits to be loaded before it is dropped.
const AHEAD_KEEP_SECS: f64 = 30.0;

/// The textures worker threads decoded, by (resolved path, sRGB): the handle reserved for
/// each, its format, and the image until the main thread puts it in the image store
/// ([`TextureCache::take_workers`]). A worker that needs a texture another one decoded
/// takes the same handle. Entries stay a while after their image went in, so workers with
/// an older copy of the cache's list ([`TextureCache::snapshot`]) still find them.
#[derive(Default)]
pub struct WorkerTextures {
    entries: Mutex<HashMap<(String, bool), WorkerTexture>>,
    /// Textures a worker could not load, with why, until the cache remembers them.
    failed: Mutex<HashMap<(String, bool), String>>,
}

struct WorkerTexture {
    handle: Handle<Image>,
    format: TextureFormat,
    image: Option<Image>,
    /// When the image went into the store (real seconds); `None` while it waits.
    stored_at: Option<f64>,
}

/// Seconds a worker texture's entry stays after its image went into the store.
const WORKER_KEEP_SECS: f64 = 30.0;

/// The textures the cache holds, with their formats, as a worker thread sees them.
pub type TextureSnapshot = HashMap<(String, bool), (Handle<Image>, TextureFormat)>;

/// A texture for a worker thread: from the cache's list, from another worker, or read and
/// decoded here under a handle reserved from `images` (the image store's handle provider).
/// `None`: not in any archive, or not decodable (a load on the main thread reports why).
pub(crate) fn worker_texture(
    raw_path: &str,
    srgb: bool,
    data: &GameData,
    cached: &TextureSnapshot,
    workers: &WorkerTextures,
    images: &AssetHandleProvider,
    bc_supported: bool,
) -> Option<(Handle<Image>, TextureFormat)> {
    let key = (resolved_path(raw_path, |p| data.vfs.exists(p)), srgb);
    if let Some(found) = cached.get(&key) {
        return Some(found.clone());
    }
    let shared = |w: &WorkerTexture| (w.handle.clone(), w.format);
    if let Some(w) = workers.entries.lock().expect("worker textures").get(&key) {
        return Some(shared(w));
    }
    if workers.failed.lock().expect("worker textures").contains_key(&key) {
        return None;
    }
    let image = match data.read(&key.0).ok_or_else(|| "not in any archive".to_string()).and_then(|bytes| dds_image(&bytes, srgb, bc_supported)) {
        Ok(mut image) => {
            image.texture_descriptor.label = Some(folder_label(&key.0));
            image
        }
        Err(e) => {
            workers.failed.lock().expect("worker textures").insert(key, e);
            return None;
        }
    };
    let mut entries = workers.entries.lock().expect("worker textures");
    // Another worker may have decoded it meanwhile: its handle is the one to use.
    let w = entries.entry(key).or_insert_with(|| WorkerTexture {
        handle: images.reserve_handle().typed::<Image>(),
        format: image.texture_descriptor.format,
        image: Some(image),
        stored_at: None,
    });
    Some(shared(w))
}

/// The archive path a NIF texture path loads from: normalised, the PC's own form of it if
/// any ([`pc_texture`]).
pub fn resolved_path(raw_path: &str, exists: impl Fn(&str) -> bool) -> String {
    let path = fnv_vfs::texture_path(raw_path);
    pc_texture(&path, exists).unwrap_or(path)
}

/// Anisotropic filtering of the game's textures made from now on (8 unless the
/// renderer's graphics settings say otherwise: nvforever `graphics.rs`).
static ANISOTROPY: AtomicU16 = AtomicU16::new(8);

/// Set the anisotropic filtering (1, 2, 4, 8 or 16) of textures made from now on.
pub fn set_anisotropy(clamp: u16) {
    ANISOTROPY.store(clamp.clamp(1, 16), Ordering::Relaxed);
}

pub fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: ANISOTROPY.load(Ordering::Relaxed),
        ..ImageSamplerDescriptor::linear()
    })
}

/// Build a GPU image from DDS bytes; `None` for cube maps, unsupported formats, or BC
/// textures the device cannot sample.
pub fn dds_image(bytes: &[u8], srgb: bool, bc_supported: bool) -> Result<Image, String> {
    // A panic while decoding is this texture's failure, not the game's (guard.rs).
    crate::guard::guarded(|| dds_image_unguarded(bytes, srgb, bc_supported, false))
}

/// [`dds_image`] with the largest level only, its smaller ones left out: for a sheet of
/// pictures cut out by rectangles (a font's glyphs), where a smaller level blends each
/// picture with its neighbours. The base game's font textures have one level
/// [observed: fonts.md §2]; a localized or modded one may have more (#1439).
pub fn dds_image_top(bytes: &[u8], srgb: bool, bc_supported: bool) -> Result<Image, String> {
    crate::guard::guarded(|| dds_image_unguarded(bytes, srgb, bc_supported, true))
}

fn dds_image_unguarded(bytes: &[u8], srgb: bool, bc_supported: bool, top_only: bool) -> Result<Image, String> {
    let dds = Dds::parse(bytes).map_err(|e| e.to_string())?;
    if dds.header.is_cubemap() {
        return Err("cube map (not used by the viewer)".into());
    }
    let (w, h) = (dds.header.width, dds.header.height);
    let levels = if top_only { &dds.levels[0][..1] } else { &dds.levels[0][..] };
    let (format, data) = match dds.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            if !bc_supported {
                return Err("GPU lacks BC texture support".into());
            }
            if w % 4 != 0 || h % 4 != 0 {
                return Err(format!("{w}x{h} block-compressed texture is not a multiple of 4"));
            }
            let f = match (dds.format, srgb) {
                (Format::Dxt1, true) => TextureFormat::Bc1RgbaUnormSrgb,
                (Format::Dxt1, false) => TextureFormat::Bc1RgbaUnorm,
                (Format::Dxt3, true) => TextureFormat::Bc2RgbaUnormSrgb,
                (Format::Dxt3, false) => TextureFormat::Bc2RgbaUnorm,
                (_, true) => TextureFormat::Bc3RgbaUnormSrgb,
                (_, false) => TextureFormat::Bc3RgbaUnorm,
            };
            (f, levels.concat())
        }
        other => {
            let mut out = Vec::new();
            for l in levels {
                out.extend(to_rgba8(other, l).ok_or("unsupported layout")?);
            }
            (if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm }, out)
        }
    };
    let mut img = checked_image(w, h, levels.len() as u32, format, data)?;
    img.sampler = sampler();
    Ok(img)
}

/// The largest side of a texture taken from a file. Above every texture the game ships
/// (4096 at most [observed]) and within what desktop GPUs take; a larger size in a file
/// is a broken header.
pub const MAX_TEXTURE_SIDE: u32 = 16384;

/// A 2D image of `w`×`h` with `mips` levels whose bytes are `data`, or why the GPU would
/// refuse it. Bevy closes the game on a texture the GPU refuses (a zero or oversized
/// side, more levels than the size allows), and the render thread panics when the data
/// is shorter than the size says; so every image made from a file's numbers is built
/// here (docs/design/broken-files.md).
pub fn checked_image(w: u32, h: u32, mips: u32, format: TextureFormat, data: Vec<u8>) -> Result<Image, String> {
    if w == 0 || h == 0 || w > MAX_TEXTURE_SIDE || h > MAX_TEXTURE_SIDE {
        return Err(format!("size {w}x{h} out of range (1 to {MAX_TEXTURE_SIDE})"));
    }
    let most = 32 - w.max(h).leading_zeros();
    if mips == 0 || mips > most {
        return Err(format!("{mips} mip levels for a {w}x{h} texture (at most {most})"));
    }
    let (bw, bh) = format.block_dimensions();
    let block = format.block_copy_size(None).ok_or("format has no single block size")? as usize;
    let expected: usize = (0..mips).map(|m| ((w >> m).max(1).div_ceil(bw) * (h >> m).max(1).div_ceil(bh)) as usize * block).sum();
    if data.len() != expected {
        return Err(format!("{} bytes of data for a {w}x{h} texture of {mips} levels ({expected} expected)", data.len()));
    }
    let mut img = Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, data, format, RenderAssetUsages::RENDER_WORLD);
    img.texture_descriptor.mip_level_count = mips;
    img.texture_descriptor.label = Some("dds texture");
    Ok(img)
}

/// The GPU label of a game texture: its folder under `textures\` (`textures\clutter`),
/// so the GPU allocator's report (nvforever `stress_sample.rs`) tells what kinds of
/// texture it holds. One string per folder, made once.
fn folder_label(path: &str) -> &'static str {
    static LABELS: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);
    let mut parts = path.split('\\');
    let folder = match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(_)) => format!("{a}\\{b}"),
        _ => "textures".to_string(),
    };
    let mut labels = LABELS.lock().expect("texture labels");
    labels.get_or_insert_with(HashMap::new).entry(folder).or_insert_with_key(|k| Box::leak(k.clone().into_boxed_str()))
}

/// The PC's own form of a texture a model names in a console's folder: the same path
/// with `\pc\` for `\xbox\`, when that file exists. [observed: the Pip-Boy models
/// (`pipboy3000\pipboyarm.nif` and the rest) name `textures\pipboy3000\xbox\english\
/// PipBoyArm01.dds`, whose buttons are labelled with the console's LT and RT, and
/// `pipboy3000\pc\english\pipboyarm01.dds` lies beside it, as do `ps3` and `ps3o`
/// forms; the owner's screenshots of the PC game show no such labels. hypothesis: the
/// PC game reads the `pc` folder's file; docs/spec-notes/pipboy.md §1]
pub fn pc_texture(path: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    let i = lower.find("\\xbox\\")?;
    let pc = format!("{}\\pc\\{}", &path[..i], &path[i + "\\xbox\\".len()..]);
    exists(&pc).then_some(pc)
}

/// The first-person form of a texture a model names: the same folder with `1stperson`
/// before the file name, when that file exists. [observed: `meshes\weapons\2handrifle\
/// laserrifle.nif`, the only laser rifle model, names `LaserRifle01.dds` (128x128), and
/// `1stpersonlaserrifle01.dds` (1024x1024) lies beside it; 413 of the 455 `1stperson`
/// textures in the archives, all weapons', have such a plain twin and no model names
/// them; the owner's screenshot of the game shows the gun's small print sharp in first
/// person. hypothesis: the game draws the first-person weapon with these;
/// docs/spec-notes/first-person-textures.md §3]
pub fn first_person_texture(path: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let path = fnv_vfs::texture_path(path);
    if is_first_person_texture(&path) {
        return None;
    }
    let file = path.rfind('\\').map_or(0, |i| i + 1);
    let first = format!("{}1stperson{}", &path[..file], &path[file..]);
    exists(&first).then_some(first)
}

/// Whether a texture's file name is in its first-person form (starts `1stperson`).
pub fn is_first_person_texture(path: &str) -> bool {
    let path = fnv_vfs::texture_path(path);
    let file = path.rfind('\\').map_or(0, |i| i + 1);
    path[file..].starts_with("1stperson")
}

impl TextureCache {
    /// Drop the textures nothing else holds (no live material uses them), after their
    /// grace time (`crate::release`); the number dropped. Keys: (path, sRGB, clamped).
    /// The textures held, for the stress census: an entry is idle when only the cache
    /// holds its image (it waits out its keep time).
    pub fn share(&self) -> crate::release::CacheShare {
        let mut s = crate::release::CacheShare::default();
        for h in self.map.values().chain(self.clamped.values()).flatten() {
            s.entries += 1;
            s.idle += usize::from(!crate::release::held_beyond(h, 1));
            s.images.insert(h.id());
        }
        s
    }

    /// Drop the textures nothing has used for `keep.secs` ([`KeepLimits`](crate::release::KeepLimits)):
    /// counted from when the model or part that last held one went unused, where `hints`
    /// says (release.rs [`ImageHints`](crate::release::ImageHints)).
    pub fn release(&mut self, idle: &mut crate::release::Unused<(String, bool, bool)>, hints: &crate::release::ImageHints, now: f64, keep: crate::release::KeepLimits) -> usize {
        use crate::release::held_beyond;
        let mut ids = HashMap::new();
        let entries: Vec<_> = [(false, &self.map), (true, &self.clamped)]
            .into_iter()
            .flat_map(|(clamp, map)| map.iter().filter_map(move |((p, srgb), h)| h.as_ref().map(|h| ((p.clone(), *srgb, clamp), h))))
            .map(|(k, h)| {
                ids.insert(k.clone(), h.id());
                (k, held_beyond(h, 1))
            })
            .collect();
        self.ahead.retain(|_, (_, at)| now - *at < AHEAD_KEEP_SECS);
        self.workers.entries.lock().expect("worker textures").retain(|_, w| w.stored_at.is_none_or(|t| now - t < WORKER_KEEP_SECS));
        let drop = idle.sweep_timed(now, entries, keep.secs, keep.max, |k| ids.get(k).and_then(|&id| hints.since(id)));
        for ((p, srgb, clamp), _) in &drop {
            (if *clamp { &mut self.clamped } else { &mut self.map }).remove(&(p.clone(), *srgb));
        }
        self.generation += u64::from(!drop.is_empty());
        drop.len()
    }

    /// Put the images worker threads decoded into the image store and the cache (a texture
    /// the main thread loaded meanwhile keeps its own; the worker's copy stays for the
    /// materials made with it); the number put in. `now`: real seconds.
    pub fn take_workers(&mut self, images: &mut Assets<Image>, now: f64) -> usize {
        let mut n = 0;
        let mut entries = self.workers.entries.lock().expect("worker textures");
        for (key, w) in entries.iter_mut() {
            let Some(image) = w.image.take() else { continue };
            if images.insert(w.handle.id(), image).is_ok() {
                self.map.entry(key.clone()).or_insert_with(|| Some(w.handle.clone()));
                n += 1;
            }
            w.stored_at = Some(now);
        }
        drop(entries);
        for (key, e) in self.workers.failed.lock().expect("worker textures").drain() {
            crate::guard::failed(&mut self.failures, "texture", format!("{}: {e}", key.0));
            self.map.entry(key).or_insert(None);
        }
        self.generation += u64::from(n > 0);
        n
    }

    /// The textures held now with their formats, for worker threads (`ModelCtx`
    /// keeps the copy while it is unchanged and in use).
    pub fn snapshot(&self, images: &Assets<Image>) -> TextureSnapshot {
        self.map
            .iter()
            .filter_map(|(k, h)| {
                let h = h.as_ref()?;
                Some((k.clone(), (h.clone(), images.get(h)?.texture_descriptor.format)))
            })
            .collect()
    }

    /// Changes to the cache so far ([`Self::snapshot`]).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The bytes of the texture file at `path` (resolved: [`resolved_path`]), read ahead on
    /// a worker thread, for the first load of it; kept unless it is loaded already.
    pub fn offer(&mut self, path: String, bytes: Vec<u8>, now: f64) {
        let loaded = [true, false].iter().any(|&srgb| self.map.contains_key(&(path.clone(), srgb)) || self.clamped.contains_key(&(path.clone(), srgb)));
        if !loaded {
            self.ahead.insert(path, (bytes, now));
        }
    }

    /// Whether the texture at the resolved path `path` is loaded (or failed to load), being
    /// put in from a worker thread, or its file read ahead ([`Self::offer`]): loading it
    /// reads no file.
    pub fn has_or_ahead(&self, path: &str) -> bool {
        let key = |srgb: bool| (path.to_string(), srgb);
        self.ahead.contains_key(path)
            || [true, false].iter().any(|&srgb| self.map.contains_key(&key(srgb)) || self.workers.entries.lock().expect("worker textures").contains_key(&key(srgb)))
    }

    /// Textures loaded and held now (failures not counted).
    pub fn len(&self) -> usize {
        self.map.values().chain(self.clamped.values()).filter(|h| h.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Load (once) the texture at a NIF texture path (the PC's own form of it, if any:
    /// [`pc_texture`]).
    pub fn get(
        &mut self,
        raw_path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        self.load(raw_path, srgb, false, data, images, formats)
    }

    /// Like [`Self::get`], a copy of the texture whose sampler clamps at its edges
    /// instead of repeating (the grass atlas, nvforever `grass.rs`).
    pub fn get_clamped(
        &mut self,
        raw_path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        self.load(raw_path, srgb, true, data, images, formats)
    }

    fn load(
        &mut self,
        raw_path: &str,
        srgb: bool,
        clamp: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        let path = resolved_path(raw_path, |p| data.vfs.exists(p));
        if let Some(h) = (if clamp { &self.clamped } else { &self.map }).get(&(path.clone(), srgb)) {
            return h.clone();
        }
        // Decoded on a worker thread: its handle (and its image, if still waiting).
        if !clamp {
            let mut entries = self.workers.entries.lock().expect("worker textures");
            if let Some(w) = entries.get_mut(&(path.clone(), srgb)) {
                if let Some(image) = w.image.take() {
                    let _ = images.insert(w.handle.id(), image);
                    w.stored_at = Some(0.0);
                }
                let h = w.handle.clone();
                drop(entries);
                self.map.insert((path, srgb), Some(h.clone()));
                self.generation += 1;
                return Some(h);
            }
        }
        let bc = formats.is_none_or(|f| f.0.contains(CompressedImageFormats::BC));
        let read_span = bevy::log::info_span!("texture::read").entered();
        let read = self.ahead.remove(&path).map(|(bytes, _)| bytes).or_else(|| data.read(&path));
        drop(read_span);
        let decode_span = bevy::log::info_span!("texture::decode").entered();
        let result = match read {
            None => Err("not in any archive".to_string()),
            Some(bytes) => dds_image(&bytes, srgb, bc),
        };
        drop(decode_span);
        let handle = match result {
            Ok(mut img) => {
                img.texture_descriptor.label = Some(folder_label(&path));
                if clamp {
                    if let ImageSampler::Descriptor(d) = &mut img.sampler {
                        d.address_mode_u = ImageAddressMode::ClampToEdge;
                        d.address_mode_v = ImageAddressMode::ClampToEdge;
                    }
                }
                Some(images.add(img))
            }
            Err(e) => {
                crate::guard::failed(&mut self.failures, "texture", format!("{path}: {e}"));
                None
            }
        };
        (if clamp { &mut self.clamped } else { &mut self.map }).insert((path, srgb), handle.clone());
        self.generation += 1;
        handle
    }
}

/// How a texture's alpha is spread over its top level: the shares of texels that are clear
/// (alpha 8 or less of 255), opaque (247 or more) and in between.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlphaShares {
    pub clear: f32,
    pub opaque: f32,
    pub partial: f32,
}

impl AlphaShares {
    /// A cut-out mask: a part of the texture is clear and nearly all the rest opaque, the
    /// in-between texels being only the edges between them. The game's corn
    /// (`textures\landscape\plants\maize_d.dds`) is 32% clear, 60% opaque, 8% between
    /// [observed]; dirty glass and smoke are mostly in between.
    pub fn is_mask(self) -> bool {
        self.clear >= MASK_MIN_CLEAR && self.partial <= MASK_MAX_PARTIAL
    }
}

/// Least clear share of a cut-out mask [engineering choice: a texture with no hole is no
/// cut-out].
pub const MASK_MIN_CLEAR: f32 = 0.02;
/// Most in-between share of a cut-out mask [engineering choice from the survey of blended
/// alpha-tested shapes, docs/design/fsr-upscaling.md "Blended cut-outs": plants, lattices and
/// grave flowers lie at or under 0.12 (glass shards 0.12, corn 0.08); the next are hair at
/// 0.14 (excluded otherwise), explosion debris at 0.15, translucent glasses at 0.26 and up].
pub const MASK_MAX_PARTIAL: f32 = 0.13;

/// The alpha shares of a DDS file's top level, read from at most about 16,000 of its
/// texels' blocks spread over it. `None` for files that do not parse and for layouts
/// without alpha.
pub fn alpha_shares(bytes: &[u8]) -> Option<AlphaShares> {
    let dds = Dds::parse(bytes).ok()?;
    let top = *dds.levels.first()?.first()?;
    let (w, h) = (dds.header.width as usize, dds.header.height as usize);
    let mut counts = [0usize; 3];
    let mut add = |a: u8| counts[if a <= 8 { 0 } else if a >= 247 { 1 } else { 2 }] += 1;
    match dds.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            let size = dds.format.block_bytes()?;
            let blocks = (w.div_ceil(4) * h.div_ceil(4)).min(top.len() / size);
            let step = blocks.div_ceil(16_384).max(1);
            for b in top.chunks_exact(size).take(blocks).step_by(step) {
                for a in block_alpha(dds.format, b) {
                    add(a);
                }
            }
        }
        Format::A8R8G8B8 | Format::A4R4G4B4 | Format::A8B8G8R8 | Format::A1R5G5B5 | Format::A8L8 => {
            let rgba = to_rgba8(dds.format, top)?;
            let texels = rgba.len() / 4;
            let step = texels.div_ceil(262_144).max(1);
            for p in rgba.as_chunks::<4>().0.iter().step_by(step) {
                add(p[3]);
            }
        }
        Format::R8G8B8 | Format::X8R8G8B8 | Format::X8B8G8R8 | Format::R5G6B5 | Format::X1R5G5B5 | Format::L8 => return None,
    }
    let n = counts.iter().sum::<usize>().max(1) as f32;
    Some(AlphaShares { clear: counts[0] as f32 / n, opaque: counts[1] as f32 / n, partial: counts[2] as f32 / n })
}

/// The 16 alphas of one DXT block [documented: the DXT1/3/5 block layouts; fnv-dds].
fn block_alpha(format: Format, b: &[u8]) -> [u8; 16] {
    match format {
        // Colour 0 not above colour 1: the fourth index is transparent black.
        Format::Dxt1 => {
            let (c0, c1) = (u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]]));
            let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            std::array::from_fn(|i| if c0 <= c1 && (bits >> (2 * i)) & 3 == 3 { 0 } else { 255 })
        }
        Format::Dxt3 => std::array::from_fn(|i| ((b[i / 2] >> (4 * (i % 2))) & 15) * 17),
        _ => {
            let (a0, a1) = (b[0] as u32, b[1] as u32);
            let pal: [u32; 8] = if a0 > a1 {
                std::array::from_fn(|k| match k { 0 => a0, 1 => a1, k => ((8 - k as u32) * a0 + (k as u32 - 1) * a1) / 7 })
            } else {
                std::array::from_fn(|k| match k { 0 => a0, 1 => a1, 6 => 0, 7 => 255, k => ((6 - k as u32) * a0 + (k as u32 - 1) * a1) / 5 })
            };
            let bits = b[2..8].iter().rev().fold(0u64, |acc, &x| (acc << 8) | u64::from(x));
            std::array::from_fn(|i| pal[((bits >> (3 * i)) & 7) as usize] as u8)
        }
    }
}

/// [`alpha_shares`] of the texture at a NIF texture path, read from the archives.
pub fn texture_alpha_shares(raw_path: &str, data: &GameData) -> Option<AlphaShares> {
    let path = resolved_path(raw_path, |p| data.vfs.exists(p));
    alpha_shares(&data.read(&path)?)
}

#[cfg(test)]
mod tests {
    use super::first_person_texture;
    use super::{alpha_shares, AlphaShares};

    /// A DDS file of one level: the header (fnv-dds layout) and `payload`.
    fn dds(fourcc: &[u8; 4], w: u32, h: u32, payload: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..4].copy_from_slice(b"DDS ");
        let put = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut b, 4, 124);
        put(&mut b, 8, 0x1007);
        put(&mut b, 12, h);
        put(&mut b, 16, w);
        put(&mut b, 28, 1);
        put(&mut b, 76, 32);
        put(&mut b, 80, 4);
        b[84..88].copy_from_slice(fourcc);
        put(&mut b, 108, 0x1000);
        b.extend_from_slice(payload);
        b
    }

    /// A DXT5 block with every texel at alpha `a` (endpoints a and a, index 0).
    fn dxt5_block(a: u8) -> [u8; 16] {
        let mut blk = [0u8; 16];
        blk[0] = a;
        blk[1] = a;
        blk
    }

    #[test]
    fn cut_out_masks_are_told_from_translucent_alpha() {
        // 8x8 = four blocks: two clear, one opaque, one in between.
        let mut payload = Vec::new();
        for a in [0u8, 0, 255, 128] {
            payload.extend_from_slice(&dxt5_block(a));
        }
        let s = alpha_shares(&dds(b"DXT5", 8, 8, &payload)).unwrap();
        assert_eq!(s, AlphaShares { clear: 0.5, opaque: 0.25, partial: 0.25 });
        assert!(!s.is_mask(), "a quarter in between is translucent");
        // Interpolated DXT5 indices: a0 = 255, a1 = 0, index 7 is (1·255 + 6·0)/7 ≈ 36.
        let mut blk = [0u8; 16];
        blk[0] = 255;
        blk[2..8].copy_from_slice(&[0xFF; 6]);
        let mut mask = Vec::new();
        for b in [dxt5_block(0), dxt5_block(255), dxt5_block(255), blk] {
            mask.extend_from_slice(&b);
        }
        let s = alpha_shares(&dds(b"DXT5", 8, 8, &mask)).unwrap();
        assert!((s.partial - 0.25).abs() < 1e-6, "{s:?}");
        let mostly = [dxt5_block(0), dxt5_block(255), dxt5_block(255), dxt5_block(255), dxt5_block(255), dxt5_block(255), dxt5_block(255), dxt5_block(0)];
        let s = alpha_shares(&dds(b"DXT5", 16, 8, &mostly.concat())).unwrap();
        assert!(s.is_mask(), "{s:?}");
        // All opaque: no hole, no cut-out.
        assert!(!alpha_shares(&dds(b"DXT5", 4, 4, &dxt5_block(255))).unwrap().is_mask());
        // DXT1 punch-through: colour 0 ≤ colour 1 and index 3 is clear.
        let mut dxt1 = [0u8; 8];
        dxt1[2] = 1;
        dxt1[4..8].copy_from_slice(&[0xFF, 0xFF, 0, 0]);
        let s = alpha_shares(&dds(b"DXT1", 4, 4, &dxt1)).unwrap();
        assert_eq!(s, AlphaShares { clear: 0.5, opaque: 0.5, partial: 0.0 });
        assert!(s.is_mask());
        // DXT3: explicit 4-bit alphas, 0x0F → 255 then 0.
        let mut dxt3 = [0u8; 16];
        dxt3[..8].copy_from_slice(&[0x0F; 8]);
        let s = alpha_shares(&dds(b"DXT3", 4, 4, &dxt3)).unwrap();
        assert_eq!(s, AlphaShares { clear: 0.5, opaque: 0.5, partial: 0.0 });
    }

    #[test]
    fn the_first_person_twin_is_used_when_it_exists() {
        let twin = r"textures\weapons\2handrifle\1stpersonlaserrifle01.dds";
        let exists = |p: &str| p == twin;
        assert_eq!(first_person_texture(r"textures\weapons\2handrifle\LaserRifle01.dds", exists).as_deref(), Some(twin));
        // A path as models write it, without the `textures` folder.
        assert_eq!(first_person_texture(r"weapons\2handrifle\LaserRifle01.dds", exists).as_deref(), Some(twin));
        assert_eq!(first_person_texture(r"textures\weapons\2handrifle\LaserRifle02.dds", exists), None);
        // Already a first-person texture: kept.
        assert_eq!(first_person_texture(r"textures\weapons\2handrifle\1stPersonLaserRifle01.dds", |_| true), None);
    }
}
