//! Mods' own files: the INI files under `Data\Config` that LU, ShowOff and MCM read and
//! write, and the file queries of JIP and LU (FileExists, GetFilesInFolder)
//! [documented: their pages; observed: the peer's ini-files and plugin-odds sets in the
//! original game].
//!
//! NVForever never writes into the game folder (install-layout.md): a file a mod writes
//! goes under NVForever's own layer over Data (`fnv_install::EngineHome::mods`), at the
//! same path, and readers look there first, then in the game's Data (owner-review.md,
//! J9: "writers write only under NVForever's own engine-home folder, mirroring the
//! relative path; readers prefer that copy"). Without a write root (a headless run, a test
//! run in the player's home) the writes stay in memory for the session.

use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The host's answer for a path in the archives too: 0 none, 1 a loose file, 2 inside an
/// archive.
pub type Lookup = Arc<dyn Fn(&str) -> u8 + Send + Sync>;

/// The host's reader of a Data path, archives included.
pub type Reader = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/// Where the files are: `read` in order (NVForever's layer first, then the game's Data),
/// `write` the layer, and `lookup` the host's answer for a path in the archives too
/// (0 none, 1 a loose file, 2 inside an archive).
#[derive(Clone, Default)]
pub struct Roots {
    pub read: Vec<PathBuf>,
    /// A folder standing for `Data\Config` before the read roots (`fnvtool --config`).
    pub config: Option<PathBuf>,
    pub write: Option<PathBuf>,
    pub lookup: Option<Lookup>,
    pub reader: Option<Reader>,
}

static ROOTS: Mutex<Option<Roots>> = Mutex::new(None);

/// Set the roots for the games made after this (the engine, `fnvtool --config`).
pub fn set_roots(r: Roots) {
    if let Ok(mut g) = ROOTS.lock() {
        *g = Some(r);
    }
}

/// Change the roots in place (the engine sets the write root, then the read roots once
/// the game's Data is open).
pub fn update_roots(f: impl FnOnce(&mut Roots)) {
    if let Ok(mut g) = ROOTS.lock() {
        f(g.get_or_insert_with(Roots::default));
    }
}

/// A Data file's bytes through the host (archives included).
pub fn read_data(path: &str) -> Option<Vec<u8>> {
    roots().reader.as_ref().and_then(|f| f(path))
}

/// Whether a NIF names a block exactly so: its header's string table holds every name
/// as a length and the characters [documented: nif.xml, the header's Strings list from
/// version 20.1].
pub fn nif_has_name(bytes: &[u8], name: &str) -> bool {
    let n = name.as_bytes();
    if n.is_empty() {
        return false;
    }
    let mut pat = (n.len() as u32).to_le_bytes().to_vec();
    pat.extend_from_slice(n);
    bytes.windows(pat.len()).any(|w| w == pat.as_slice())
}

/// The loose files directly in a Data folder: each name (as on disk) with its path, from
/// the read roots in order (NVForever's layer first), a name once, the first root's.
/// Archives are not looked in.
pub fn data_files(folder: &str) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for root in roots().read {
        let Some(dir) = on_disk_any_case(&root, folder) else { continue };
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if e.path().is_file() && !out.iter().any(|(x, _)| x.eq_ignore_ascii_case(&n)) {
                out.push((n, e.path()));
            }
        }
    }
    out
}

/// A Data file's bytes: the first read root's loose copy (NVForever's layer before the
/// game's Data), else through the host (archives included).
pub fn read_data_file(path: &str) -> Option<Vec<u8>> {
    roots().read.iter().filter_map(|root| on_disk_any_case(root, path)).find_map(|p| std::fs::read(p).ok()).or_else(|| read_data(path))
}

/// [`on_disk`], each part of the path matched to what is on disk in any case, as Windows
/// does: mods spell `NVSE\Plugins\Scripts` every way [observed: the owner's MO2 mods], and
/// a case-sensitive file system (Linux) would miss them. `None` when a part is missing.
fn on_disk_any_case(base: &Path, rel: &str) -> Option<PathBuf> {
    let exact = on_disk(base, rel)?;
    if exact.exists() {
        return Some(exact);
    }
    let mut p = base.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|c| !c.is_empty()) {
        let direct = p.join(part);
        p = if direct.exists() {
            direct
        } else {
            std::fs::read_dir(&p).ok()?.flatten().map(|e| e.path()).find(|e| e.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(part)))?
        };
    }
    Some(p)
}

/// The game's Data folder, when the host gave one (the last read root).
pub fn data_dir() -> Option<PathBuf> {
    roots().read.last().cloned()
}

/// Whether a Data path is a file of the game: in a read root (its parts in any case, as
/// on Windows; mods spell `Meshes` and `meshes` both) or in an archive.
pub fn exists(path: &str) -> bool {
    let r = roots();
    r.read.iter().any(|root| on_disk(root, path).is_some_and(|p| p.is_file()) || find_file(root, path).is_some()) || r.lookup.as_ref().is_some_and(|f| f(path) > 0)
}

fn roots() -> Roots {
    ROOTS.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// The session's view of mods' files.
#[derive(Default)]
pub struct ModFiles {
    /// Files written this session, by lower-case Data path (the disk copy, if any, is
    /// written too).
    written: HashMap<String, String>,
    /// ShowOff's cached INI files, by lower-case Data path: key -> value, read once.
    cache: HashMap<String, HashMap<String, String>>,
    /// ShowOff's cached JSON files (ReadFromJSON, WriteToJSON with useCache), by the
    /// lower-case path under the game's folder.
    showoff_json: HashMap<String, serde_json::Value>,
    /// Folders CreateFolder made this session (under the game's folder, lower case).
    folders: std::collections::HashSet<String>,
    /// SUP's JSONGetValue files read with iCached, by lower-case path (`crate::nvse_sup5`).
    pub(crate) json: HashMap<String, serde_json::Value>,
}

/// The folder of NVForever's layer standing for the game's own folder.
const GAME_ROOT: &str = "GameRoot";

/// A value as an array file writes it: numbers as `%g` does, forms as their hex ID.
fn cell_text(v: &XVal) -> String {
    match v {
        XVal::Num(x) if x.fract() == 0.0 && x.abs() < 1e15 => format!("{}", *x as i64),
        XVal::Num(x) => format!("{x}"),
        XVal::Ref(r) => format!("{r:08X}"),
        XVal::Str(s) => s.clone(),
        XVal::Array(_) => String::new(),
    }
}

/// `base` joined with a Data-relative path whose parts are split at `\` or `/`: the
/// same path on Windows, and a real subfolder (not a name with a backslash in it) on
/// other systems. `None` for a path that could leave `base`: one that starts at a root
/// (`\x`), names a drive or stream (`C:\x`, any `:`), or has a `.` or `..` part. Every
/// part must be a plain name (`Component::Normal`), so a script's path stays under the
/// folder it is meant for (CLAUDE.md charter; PR #1291 review).
fn on_disk(base: &Path, rel: &str) -> Option<PathBuf> {
    if rel.starts_with(['\\', '/']) || rel.contains(':') {
        return None;
    }
    let mut p = base.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|c| !c.is_empty()) {
        let mut c = Path::new(part).components();
        match (c.next(), c.next()) {
            (Some(Component::Normal(_)), None) => p.push(part),
            _ => return None,
        }
    }
    Some(p)
}

/// A folder under `base` by a Data-relative path, each part matched without regard to
/// case (mods spell `_1stPerson`, `_1stperson` and `Meshes` as they like, and other
/// systems than Windows tell them apart); `None` when a part is missing or could leave
/// `base` (as [`on_disk`]).
fn find_dir(base: &Path, rel: &str) -> Option<PathBuf> {
    on_disk(base, rel)?;
    let mut p = base.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|c| !c.is_empty()) {
        let direct = p.join(part);
        p = if direct.is_dir() {
            direct
        } else {
            std::fs::read_dir(&p).ok()?.flatten().map(|e| e.path()).find(|e| e.is_dir() && e.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(part)))?
        };
    }
    Some(p)
}

/// The loose files under a Data folder, as paths relative to it (`\` between parts),
/// walked through every read root: a path in an earlier root (NVForever's layer) hides
/// the same path, compared without regard to case, in a later one. Sorted without
/// regard to case. kNVSE's AnimGroupOverride folders (`crate::nvse_anim_folders`).
pub fn files_under(rel: &str) -> Vec<String> {
    list_files(rel, true)
}

/// The loose files directly in a Data folder (no subfolders), as [`files_under`].
pub fn files_in(rel: &str) -> Vec<String> {
    list_files(rel, false)
}

fn list_files(rel: &str, deep: bool) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, deep: bool, seen: &mut std::collections::HashSet<String>, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let rel = if prefix.is_empty() { name } else { format!("{prefix}\\{name}") };
            if e.path().is_dir() {
                if deep {
                    walk(&e.path(), &rel, deep, seen, out);
                }
            } else if seen.insert(rel.to_ascii_lowercase()) {
                out.push(rel);
            }
        }
    }
    let (mut seen, mut out) = (std::collections::HashSet::new(), Vec::new());
    for root in roots().read {
        if let Some(dir) = find_dir(&root, rel) {
            walk(&dir, "", deep, &mut seen, &mut out);
        }
    }
    out.sort_by_key(|n| n.to_ascii_lowercase());
    out
}

/// A file under `base` by a Data-relative path, each part matched without regard to
/// case (as [`find_dir`]).
fn find_file(base: &Path, rel: &str) -> Option<PathBuf> {
    // The whole path stays under `base`, the file's name too (`C:x` names a drive).
    on_disk(base, rel)?;
    let (dir, name) = rel.rsplit_once(['\\', '/']).unwrap_or(("", rel));
    let d = find_dir(base, dir)?;
    let direct = d.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(&d).ok()?.flatten().map(|e| e.path()).find(|e| e.is_file() && e.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
}

/// A loose Data file's bytes through the read roots (the first that has it), the path's
/// parts matched without regard to case.
pub fn read_loose(rel: &str) -> Option<Vec<u8>> {
    roots().read.iter().find_map(|root| std::fs::read(find_file(root, rel)?).ok())
}

/// Whether NVForever may write `path`: never inside the game's install or the original
/// game's user folder (`fnv_install::guard_write`), nor under the game folder the Data
/// read root stands in (a headless run that did not register the install).
fn may_write(path: &Path) -> bool {
    if fnv_install::guard_write(path, None).is_err() {
        return false;
    }
    let game = roots().read.last().and_then(|d| d.parent().map(Path::to_path_buf));
    !game.is_some_and(|g| path.starts_with(g))
}

/// Write `text` at `path` (its folders made), when [`may_write`] allows; whether it did.
fn write_disk_bytes(path: &Path, bytes: &[u8]) -> bool {
    if !may_write(path) {
        return false;
    }
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(path, bytes).is_ok()
}

/// Bytes as the one-byte-a-character text the readers give a disk file as.
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&c| c as char).collect()
}

/// A Data-relative path with `/` turned to `\` and lower case, for the maps.
fn norm(p: &str) -> String {
    p.replace('/', "\\").trim_start_matches('\\').to_ascii_lowercase()
}

impl ModFiles {
    /// [`ModFiles::read`] for the other files of the crate: a Data file's text, this
    /// session's write first.
    pub(crate) fn read_text(&self, rel: &str) -> Option<String> {
        self.read(rel)
    }

    /// A file's text: this session's write, else NVForever's layer, else the game's Data.
    fn read(&self, rel: &str) -> Option<String> {
        if let Some(t) = self.written.get(&norm(rel)) {
            return Some(t.clone());
        }
        let rel = rel.replace('/', "\\");
        let r = roots();
        let config = r.config.as_ref().and_then(|c| {
            let lower = rel.to_ascii_lowercase();
            lower.strip_prefix("config\\").and_then(|_| on_disk(c, &rel["config\\".len()..]))
        });
        on_disk(Path::new(""), &rel)?;
        config.into_iter().chain(r.read.iter().filter_map(|root| on_disk(root, &rel))).find_map(|p| std::fs::read(p).ok()).map(|b| b.iter().map(|&c| c as char).collect())
    }

    /// A file under the game's own folder (JIP's array files): this session's write,
    /// else NVForever's copy (its layer's `GameRoot` folder), else the game folder's.
    fn read_root(&self, rel: &str) -> Option<String> {
        on_disk(Path::new(""), rel)?;
        let key = format!("<root>\\{}", norm(rel));
        if let Some(t) = self.written.get(&key) {
            return Some(t.clone());
        }
        let rel = rel.replace('/', "\\");
        let r = roots();
        let mine = r.write.and_then(|w| on_disk(&w.join(GAME_ROOT), &rel));
        let game = r.read.last().and_then(|d| d.parent()).and_then(|d| on_disk(d, &rel));
        mine.into_iter().chain(game).find_map(|p| std::fs::read(p).ok()).map(|b| b.iter().map(|&c| c as char).collect())
    }

    /// Write a file of the game's own folder: into NVForever's layer, never the game's
    /// folder (CLAUDE.md charter; docs/design/install-layout.md). False, and nothing
    /// kept, for a path [`on_disk`] refuses or a disk path [`may_write`] refuses.
    fn write_root(&mut self, rel: &str, text: String) -> bool {
        on_disk(Path::new(""), rel).is_some() && self.write_under(format!("<root>\\{}", norm(rel)), rel, Some(GAME_ROOT), text)
    }

    /// CreateFolder: a folder under the game's folder, made in NVForever's layer (its
    /// `GameRoot` folder), never the game's; false for a path [`on_disk`] or
    /// [`may_write`] refuses. Without a write root it is only kept for the session.
    fn make_folder_root(&mut self, rel: &str) -> bool {
        if rel.trim().is_empty() || on_disk(Path::new(""), rel).is_none() {
            return false;
        }
        if let Some(w) = roots().write {
            let Some(p) = on_disk(&w.join(GAME_ROOT), rel) else { return false };
            if !may_write(&p) || std::fs::create_dir_all(&p).is_err() {
                return false;
            }
        }
        self.folders.insert(norm(rel));
        true
    }

    /// Write a Data file into NVForever's layer; false when refused, as `write_root`.
    pub(crate) fn write(&mut self, rel: &str, text: String) -> bool {
        on_disk(Path::new(""), rel).is_some() && self.write_under(norm(rel), rel, None, text)
    }

    fn write_under(&mut self, key: String, rel: &str, sub: Option<&str>, text: String) -> bool {
        let bytes = text.as_bytes().to_vec();
        self.write_raw(key, rel, sub, text, &bytes)
    }

    /// `text` kept for this session's readers, `bytes` written to the disk copy.
    fn write_raw(&mut self, key: String, rel: &str, sub: Option<&str>, text: String, bytes: &[u8]) -> bool {
        if let Some(w) = roots().write {
            let base = sub.map_or_else(|| w.clone(), |s| w.join(s));
            let Some(p) = on_disk(&base, rel) else { return false };
            if !write_disk_bytes(&p, bytes) {
                return false;
            }
        }
        self.written.insert(key, text);
        true
    }

    /// [`ModFiles::read_root`] for the other files of the crate.
    pub(crate) fn read_root_text(&self, rel: &str) -> Option<String> {
        self.read_root(rel)
    }

    /// [`ModFiles::write_root`] for the other files of the crate.
    pub(crate) fn write_root_text(&mut self, rel: &str, text: String) -> bool {
        self.write_root(rel, text)
    }

    /// A file under the game's own folder as bytes ([`ModFiles::read_root`]; SUP's Excel
    /// files).
    pub(crate) fn read_root_bytes(&self, rel: &str) -> Option<Vec<u8>> {
        self.read_root(rel).map(|t| t.chars().map(|c| c as u8).collect())
    }

    /// Write a binary file of the game's own folder into NVForever's layer
    /// ([`ModFiles::write_root`]).
    pub fn write_root_bytes(&mut self, rel: &str, bytes: &[u8]) -> bool {
        on_disk(Path::new(""), rel).is_some() && self.write_raw(format!("<root>\\{}", norm(rel)), rel, Some(GAME_ROOT), latin1(bytes), bytes)
    }

    /// Write a binary Data file into NVForever's layer ([`ModFiles::write`]; screenshots).
    pub fn write_bytes(&mut self, rel: &str, bytes: &[u8]) -> bool {
        on_disk(Path::new(""), rel).is_some() && self.write_raw(norm(rel), rel, None, latin1(bytes), bytes)
    }

    /// Remove a Data file NVForever's layer holds (this session's write and its disk
    /// copy); never a file of the game's own Data. Whether there was one.
    pub(crate) fn delete(&mut self, rel: &str) -> bool {
        if on_disk(Path::new(""), rel).is_none() {
            return false;
        }
        let mut gone = self.written.remove(&norm(rel)).is_some();
        if let Some(p) = roots().write.and_then(|w| find_file(&w, rel)) {
            if may_write(&p) && std::fs::remove_file(&p).is_ok() {
                gone = true;
            }
        }
        gone
    }

    /// Size in bytes and the disk file (for its times) of a file under the game's own
    /// folder: this session's write, else NVForever's copy, else the game folder's
    /// ([`ModFiles::read_root`]'s order). A session-only write has no disk file.
    pub(crate) fn root_file(&self, rel: &str) -> Option<(u64, Option<PathBuf>)> {
        on_disk(Path::new(""), rel)?;
        if let Some(t) = self.written.get(&format!("<root>\\{}", norm(rel))) {
            let r = roots();
            let disk = r.write.and_then(|w| find_file(&w.join(GAME_ROOT), rel));
            return Some((t.chars().count() as u64, disk));
        }
        let r = roots();
        let mine = r.write.and_then(|w| find_file(&w.join(GAME_ROOT), rel));
        let game = r.read.last().and_then(|d| d.parent()).and_then(|d| find_file(d, rel));
        let p = mine.or(game)?;
        Some((std::fs::metadata(&p).ok()?.len(), Some(p)))
    }
}

/// The INI file a function names, under `Data\Config`, ".ini" added when it has no
/// extension [observed: "NVFExtProbe\probe" and "…\probe.ini" both read]; none given:
/// the calling mod's own, `<plugin name>.ini` [documented: GetINISection, "the name of
/// the mod the function is called from is used"].
fn ini_path(g: &Game, file: Option<&str>) -> String {
    let f = match file.map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => f.replace('/', "\\"),
        None => {
            let p = &g.other.plugin;
            p.rsplit_once('.').map_or(p.as_str(), |(n, _)| n).to_string()
        }
    };
    let f = if f.to_ascii_lowercase().ends_with(".ini") { f } else { format!("{f}.ini") };
    format!("Config\\{f}")
}

/// `Section:Key` with ":", "/" or "\" between [observed: all three work], lower case.
fn section_key(s: &str) -> Option<(String, String)> {
    let i = s.find([':', '/', '\\'])?;
    Some((s[..i].trim().to_ascii_lowercase(), s[i + 1..].trim().to_ascii_lowercase()))
}

/// The value of a key in INI text (section and key without regard to case).
pub(crate) fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut cur = String::new();
    for line in text.lines() {
        let l = line.trim();
        if let Some(s) = l.strip_prefix('[') {
            cur = s.trim_end_matches(']').trim().to_ascii_lowercase();
        } else if cur == section {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    let v = v.split(';').next().unwrap_or("").trim();
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Every key=value of a section, in order.
fn ini_section(text: &str, section: &str) -> Vec<(String, String)> {
    let mut cur = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if let Some(s) = l.strip_prefix('[') {
            cur = s.trim_end_matches(']').trim().to_ascii_lowercase();
        } else if cur == section && !l.starts_with(';') {
            if let Some((k, v)) = l.split_once('=') {
                out.push((k.trim().to_string(), v.split(';').next().unwrap_or("").trim().to_string()));
            }
        }
    }
    out
}

fn ini_sections(text: &str) -> Vec<String> {
    text.lines().filter_map(|l| l.trim().strip_prefix('[').map(|s| s.trim_end_matches(']').trim().to_string())).collect()
}

/// Set a key in INI text: rewritten in place, else added at the end of its section,
/// else a new section at the end; lines end in CRLF [observed: "fSet=2.25" appended at
/// the end of [General], "fNum=6" rewritten in place, a new file "[New]\r\nfVal=1.5\r\n"].
fn ini_set(text: &str, section: &str, key: &str, value: &str, section_name: &str, key_name: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut cur = String::new();
    let mut last_in_section: Option<usize> = None;
    for (i, line) in lines.iter_mut().enumerate() {
        let l = line.trim().to_string();
        if let Some(s) = l.strip_prefix('[') {
            cur = s.trim_end_matches(']').trim().to_ascii_lowercase();
            if cur == section {
                last_in_section = Some(i);
            }
            continue;
        }
        if cur == section {
            if let Some((k, _)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    *line = format!("{}={value}", k.trim());
                    return lines.join("\r\n") + "\r\n";
                }
            }
            if !l.is_empty() {
                last_in_section = Some(i);
            }
        }
    }
    match last_in_section {
        Some(i) => lines.insert(i + 1, format!("{key_name}={value}")),
        None => {
            lines.push(format!("[{section_name}]"));
            lines.push(format!("{key_name}={value}"));
        }
    }
    lines.join("\r\n") + "\r\n"
}

/// A number as LU writes it: no decimals for a whole number [observed: "fNum=6",
/// "fSet=2.25"].
fn lu_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn num_of(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k)? {
        XVal::Str(s) => Some(s.clone()),
        XVal::Num(n) => Some(format!("{n}")),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

/// The text of the INI file SUP's readers name (argument 0, from the game's folder,
/// [`ModFiles::read_root`]), its UTF-8 byte order mark taken off (read as three
/// characters, `read_root` keeping each byte).
fn sup_ini(g: &Game, args: &[XVal]) -> Option<String> {
    let t = g.mod_files.read_root(text(args, 0)?.trim())?;
    Some(t.strip_prefix("\u{ef}\u{bb}\u{bf}").map(str::to_string).unwrap_or(t))
}

/// Read an INI file into ShowOff's cache when it is not there yet (or again, with
/// `reload`); its cache key.
fn cache_ini(g: &mut Game, path: &str, reload: bool) -> String {
    let key = norm(path);
    if reload || !g.mod_files.cache.contains_key(&key) {
        let text = g.mod_files.read(path).unwrap_or_default();
        let mut m = HashMap::new();
        for sec in ini_sections(&text) {
            let ls = sec.to_ascii_lowercase();
            for (kk, vv) in ini_section(&text, &ls) {
                m.insert(format!("{ls}:{}", kk.to_ascii_lowercase()), vv);
            }
        }
        g.mod_files.cache.insert(key.clone(), m);
    }
    key
}

/// A JSON string standing for a form: `@`, the plugin's file name, `:`, the FormID without
/// its load-order index in hex, leading zeros optional [documented: ReadFromJSON,
/// "@FalloutNV.esm:096BCE"]. `None` for any other string; `Some(0)` for a plugin not
/// loaded [hypothesis: the form is none].
fn json_form(g: &Game, s: &str) -> Option<u32> {
    let rest = s.strip_prefix('@')?;
    let (file, id) = rest.rsplit_once(':')?;
    let id = u32::from_str_radix(id.trim(), 16).ok()?;
    let index = g.data.load_order.iter().position(|p| p.eq_ignore_ascii_case(file.trim()));
    Some(index.map_or(0, |i| ((i as u32) << 24) | (id & 0x00FF_FFFF)))
}

/// A JSON value as a script value: an object a string map, an array an array, numbers
/// and booleans numbers, `null` Ar_Null's 0, a form string the form [documented:
/// ReadFromJSON's examples].
fn json_to_xval(g: &mut Game, v: &serde_json::Value) -> XVal {
    use serde_json::Value as J;
    match v {
        J::Null => XVal::Num(0.0),
        J::Bool(b) => XVal::Num(f64::from(u8::from(*b))),
        J::Number(n) => XVal::Num(n.as_f64().unwrap_or(0.0)),
        J::String(s) => match json_form(g, s) {
            Some(r) => XVal::Ref(r),
            None => XVal::Str(s.clone()),
        },
        J::Array(items) => {
            let entries = items.iter().enumerate().map(|(i, x)| (XKey::Num(i as f64), json_to_xval(g, x))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        J::Object(m) => {
            let entries = m.iter().map(|(k, x)| (XKey::Str(k.clone()), json_to_xval(g, x))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
        }
    }
}

/// A script value as JSON, the other way round [documented: WriteToJSON, "takes inputs
/// in the same format as what is outputted by ReadFromJSON"]: a form is `@`, its plugin,
/// `:` and its FormID in eight hex digits without the index [documented: WriteToJSON's
/// example, "@LonesomeRoad.esm:00003E43"]; no form, or one of no loaded plugin (made in
/// play), is `null`; a numeric map an object keyed by its numbers' text [hypothesis].
fn xval_to_json(g: &Game, v: &XVal, depth: usize) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        XVal::Num(x) => serde_json::Number::from_f64(*x).map_or(J::Null, J::Number),
        XVal::Str(s) => J::String(s.clone()),
        XVal::Ref(0) => J::Null,
        XVal::Ref(r) => match g.data.load_order.get((r >> 24) as usize) {
            Some(p) => J::String(format!("@{p}:{:08X}", r & 0x00FF_FFFF)),
            None => J::Null,
        },
        XVal::Array(id) => {
            let Some(a) = g.ext.array(*id).filter(|_| depth < 64) else { return J::Null };
            match a.kind {
                ArrayKind::List => J::Array(a.entries.iter().map(|(_, x)| xval_to_json(g, x, depth + 1)).collect()),
                _ => J::Object(
                    a.entries
                        .iter()
                        .map(|(k, x)| {
                            let key = match k {
                                XKey::Str(s) => s.clone(),
                                XKey::Num(n) => cell_text(&XVal::Num(*n)),
                            };
                            (key, xval_to_json(g, x, depth + 1))
                        })
                        .collect(),
                ),
            }
        }
    }
}

/// Put `value` at a JSON Pointer (RFC 6901) in `root`, making the objects on the way
/// that are missing [hypothesis: the page does not say what a pointer into a missing
/// part does]; an array index at or past the end (or `-`) appends. False when the path
/// runs through a value that is neither an object nor an array.
fn json_set(root: &mut serde_json::Value, pointer: &str, value: serde_json::Value) -> bool {
    use serde_json::Value as J;
    if pointer.is_empty() {
        *root = value;
        return true;
    }
    let Some(rest) = pointer.strip_prefix('/') else { return false };
    let parts: Vec<String> = rest.split('/').map(|p| p.replace("~1", "/").replace("~0", "~")).collect();
    let mut cur = root;
    let mut value = Some(value);
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        if cur.is_null() {
            *cur = J::Object(serde_json::Map::new());
        }
        cur = match cur {
            J::Object(m) => {
                if last {
                    m.insert(part.clone(), value.take().unwrap_or(J::Null));
                    return true;
                }
                m.entry(part.clone()).or_insert(J::Null)
            }
            J::Array(items) => {
                let at = match part.parse::<usize>() {
                    Ok(n) => n.min(items.len()),
                    Err(_) if part == "-" => items.len(),
                    Err(_) => return false,
                };
                if at == items.len() {
                    items.push(J::Null);
                }
                if last {
                    items[at] = value.take().unwrap_or(J::Null);
                    return true;
                }
                &mut items[at]
            }
            _ => return false,
        };
    }
    false
}

/// A JSON file under the game's folder: the cache's copy, else the file parsed. Every
/// parser mode reads standard JSON [hypothesis: JAXN (1) and TaoConfig (2) extend it;
/// their extensions are not read, so such a file fails to parse].
fn json_file(g: &Game, path: &str) -> Option<serde_json::Value> {
    if let Some(v) = g.mod_files.showoff_json.get(&norm(path)) {
        return Some(v.clone());
    }
    let text = g.mod_files.read_root(path)?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- SUP's INI readers, any file under the game's folder ----
        // `path section key`, the path from the game's folder with `/` or `\`, the
        // section and key in any case, a UTF-8 byte order mark and spaces before a
        // section's bracket passed over [documented: ReadINIFloatFromFile, "relative to
        // the game folder"; observed: sup3 ini.float.*]. A missing key or file gives 0,
        // the Alt form NaN [documented; observed: ini.float.missing, ini.alt.*]. A path
        // that leaves the game folder is not read [decision: mod_files::on_disk, the
        // charter; the original read one, sup3 ini.float.outside].
        "readinifloatfromfile" | "readinifloatfromfilealt" | "readinistringfromfile" => {
            let found = sup_ini(g, args).and_then(|t| ini_get(&t, &text(args, 1).unwrap_or_default().trim().to_ascii_lowercase(), text(args, 2).unwrap_or_default().trim()));
            match (name, found) {
                ("readinistringfromfile", v) => XVal::Str(v.unwrap_or_default()),
                (_, Some(v)) => XVal::Num(num_of(&v)),
                ("readinifloatfromfilealt", None) => XVal::Num(f64::NAN),
                _ => zero,
            }
        }
        // `path section`: the section's keys; `path`: the file's sections; each sorted
        // [observed: sup3 ini.keys, JohnnyGuitar.ini's nine [MAIN] keys from
        // bFixDeathVoicelines to bReset3rdPersonCamera, not in the file's order;
        // hypothesis: without regard to case, and the sections the same]. A missing file
        // gives no array [observed: ini.sections.nofile].
        "readinisectionkeysfromfile" | "readinisectionsfromfile" => {
            let Some(t) = sup_ini(g, args) else { return Some(Ok(zero)) };
            let mut names: Vec<String> = if name == "readinisectionsfromfile" {
                ini_sections(&t)
            } else {
                ini_section(&t, &text(args, 1).unwrap_or_default().trim().to_ascii_lowercase()).into_iter().map(|(k, _)| k).collect()
            };
            names.sort_by_key(|n| n.to_ascii_lowercase());
            names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            let entries = names.into_iter().enumerate().map(|(i, n)| (XKey::Num(i as f64), XVal::Str(n))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        // ---- JIP's array files, under the game's folder ----
        // `path append array [transpose]`: a list is one line of tab-separated values; a
        // list of lists is its columns side by side (transposed: its rows); appending
        // starts a new line; returns 1 [observed: gaps10, ar_List 4 5 6 writes `4\t5\t6`
        // with no newline after, appending 7 8 9 adds `\n7\t8\t9`].
        "writearraytofile" => {
            let path = text(args, 0).unwrap_or_default();
            let append = num(args, 1, 0.0) != 0.0;
            let transpose = num(args, 3, 0.0) != 0.0;
            let Some(XVal::Array(id)) = args.get(2) else { return Some(Ok(zero)) };
            let vals: Vec<XVal> = g.ext.array(*id).map(|a| a.entries.iter().map(|(_, v)| v.clone()).collect()).unwrap_or_default();
            let cols: Vec<Vec<XVal>> = vals
                .iter()
                .filter_map(|v| match v {
                    XVal::Array(i) => g.ext.array(*i).map(|a| a.entries.iter().map(|(_, v)| v.clone()).collect()),
                    _ => None,
                })
                .collect();
            let rows: Vec<Vec<XVal>> = if cols.is_empty() {
                vec![vals]
            } else if transpose {
                cols
            } else {
                let n = cols.iter().map(Vec::len).max().unwrap_or(0);
                (0..n).map(|i| cols.iter().map(|c| c.get(i).cloned().unwrap_or(XVal::Str(String::new()))).collect()).collect()
            };
            let body = rows.iter().map(|r| r.iter().map(cell_text).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join("\n");
            let text = match g.mod_files.read_root(&path).filter(|_| append) {
                Some(old) if !old.is_empty() => format!("{old}\n{body}"),
                _ => body,
            };
            XVal::Num(if g.mod_files.write_root(&path, text) { 1.0 } else { 0.0 })
        }
        // `path [transpose]`: one line gives a list of its values; more give a list of
        // columns (transposed: of rows); numbers read as numbers; a missing file gives
        // no array [observed: gaps10, 4 5 6 reads back with element 1 = 5; two lines of
        // three give 3; missing -1].
        "readarrayfromfile" => {
            let path = text(args, 0).unwrap_or_default();
            let transpose = num(args, 1, 0.0) != 0.0;
            let Some(t) = g.mod_files.read_root(&path) else { return Some(Ok(zero)) };
            let parse = |c: &str| match c.trim().parse::<f64>() {
                Ok(x) => XVal::Num(x),
                Err(_) => XVal::Str(c.to_string()),
            };
            let rows: Vec<Vec<XVal>> = t.lines().filter(|l| !l.trim().is_empty()).map(|l| l.split('\t').map(parse).collect()).collect();
            let list = |g: &mut Game, items: Vec<XVal>| {
                let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
                XVal::Array(g.ext.new_array(ArrayKind::List, entries))
            };
            if rows.len() == 1 {
                let r = rows.into_iter().next().unwrap_or_default();
                return Some(Ok(list(g, r)));
            }
            let groups: Vec<Vec<XVal>> = if transpose {
                rows
            } else {
                let n = rows.iter().map(Vec::len).max().unwrap_or(0);
                (0..n).map(|i| rows.iter().filter_map(|r| r.get(i).cloned()).collect()).collect()
            };
            let inner: Vec<XVal> = groups.into_iter().map(|c| list(g, c)).collect();
            list(g, inner)
        }
        // ---- ppNVSE's line files ----
        // `path [start] [end]`: the file's lines as strings, from `start` (0 the first) up
        // to before `end` [documented: pReadFile, "0 5" reads the first five lines;
        // hypothesis: the end left out], the path under the game's folder as
        // ReadArrayFromFile's [documented: pWriteFile, "relative to the game's
        // installation path"]. A missing file gives an empty array [hypothesis].
        "preadfile" => {
            let path = text(args, 0).unwrap_or_default();
            let lines: Vec<String> = g.mod_files.read_root(&path).map(|t| t.lines().map(str::to_string).collect()).unwrap_or_default();
            let start = num(args, 1, 0.0).max(0.0) as usize;
            let end = match args.get(2) {
                Some(XVal::Num(e)) => e.max(0.0) as usize,
                _ => lines.len(),
            }
            .min(lines.len());
            let entries = lines.into_iter().skip(start).take(end.saturating_sub(start)).enumerate().map(|(i, l)| (XKey::Num(i as f64), XVal::Str(l))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        // `path array [append]`: one line per element [documented: pWriteFile], into
        // NVForever's own layer, never the game's folder (`write_root`); pWriteFileAlt's
        // path is under Data [documented: pWriteFileAlt, "relative to the game's DATA
        // path"; hypothesis: the calling mod's staging folder is its place in Data]. 1
        // when written, 0 without a path or an array [documented: pWriteFileAlt].
        "pwritefile" | "pwritefilealt" => {
            let path = text(args, 0).unwrap_or_default();
            let Some(XVal::Array(id)) = args.get(1) else { return Some(Ok(zero)) };
            if path.trim().is_empty() {
                return Some(Ok(zero));
            }
            let lines: Vec<String> = g.ext.array(*id).map(|a| a.entries.iter().map(|(_, v)| cell_text(v)).collect()).unwrap_or_default();
            let append = num(args, 2, 0.0) != 0.0;
            let body = lines.join("\n");
            let alt = name == "pwritefilealt";
            let old = if alt { g.mod_files.read(&path) } else { g.mod_files.read_root(&path) };
            let text = match old.filter(|_| append) {
                Some(o) if !o.is_empty() => format!("{o}\n{body}"),
                _ => body,
            };
            // A path that would leave NVForever's layer is refused: 0 [decision: PR #1291
            // review, the charter; the page gives no failure value].
            let ok = if alt { g.mod_files.write(&path, text) } else { g.mod_files.write_root(&path, text) };
            XVal::Num(if ok { 1.0 } else { 0.0 })
        }
        // ---- LU: read the file each time; write it at once ----
        "getinifloat" | "getinistring" => {
            let Some((s, k)) = text(args, 0).as_deref().and_then(section_key) else { return Some(Ok(zero)) };
            let path = ini_path(g, text(args, 1).as_deref());
            let v = g.mod_files.read(&path).and_then(|t| ini_get(&t, &s, &k));
            // A text value reads 0 as a number [observed].
            if name == "getinifloat" { XVal::Num(v.map_or(0.0, |v| num_of(&v))) } else { XVal::Str(v.unwrap_or_default()) }
        }
        "setinifloat" | "setinistring" => {
            let Some(raw) = text(args, 0) else { return Some(Ok(zero)) };
            let Some((s, k)) = section_key(&raw) else { return Some(Ok(zero)) };
            let i = raw.find([':', '/', '\\']).unwrap_or(0);
            let (sn, kn) = (raw[..i].trim().to_string(), raw[i + 1..].trim().to_string());
            let value = if name == "setinifloat" { lu_number(num(args, 1, 0.0)) } else { text(args, 1).unwrap_or_default() };
            let path = ini_path(g, text(args, 2).as_deref());
            let old = g.mod_files.read(&path).unwrap_or_default();
            let new = ini_set(&old, &s, &k, &value, &sn, &kn);
            g.mod_files.write(&path, new);
            zero
        }
        "getinisection" => {
            let sec = text(args, 0).unwrap_or_default().to_ascii_lowercase();
            let path = ini_path(g, text(args, 1).as_deref());
            let numeric = num(args, 2, 0.0) != 0.0;
            let pairs = g.mod_files.read(&path).map(|t| ini_section(&t, &sec)).unwrap_or_default();
            let entries = pairs.into_iter().map(|(k, v)| (XKey::Str(k), if numeric { XVal::Num(num_of(&v)) } else { XVal::Str(v) })).collect();
            XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
        }
        "getinisectionnames" => {
            let path = ini_path(g, text(args, 0).as_deref());
            let names = g.mod_files.read(&path).map(|t| ini_sections(&t)).unwrap_or_default();
            let entries = names.into_iter().enumerate().map(|(i, n)| (XKey::Num(i as f64), XVal::Str(n))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        // ---- ShowOff: a file is read once and kept; OrCreate changes only the cache
        // [observed: the created key was not in the file, and LU read 0] ----
        "getinifloatordefault_cached" | "getinistringordefault_cached" | "getinifloatorcreate_cached" | "getinistringorcreate_cached" | "setinivalue_cached" | "setiniinteger_cached" => {
            let Some(raw) = text(args, 0) else { return Some(Ok(zero)) };
            let Some((s, k)) = section_key(&raw) else { return Some(Ok(zero)) };
            let file_at = if name.starts_with("set") { 2 } else { 1 };
            let path = ini_path(g, text(args, file_at).as_deref());
            let key = norm(&path);
            if !g.mod_files.cache.contains_key(&key) {
                let text = g.mod_files.read(&path).unwrap_or_default();
                let mut m = HashMap::new();
                for sec in ini_sections(&text) {
                    let ls = sec.to_ascii_lowercase();
                    for (kk, vv) in ini_section(&text, &ls) {
                        m.insert(format!("{ls}:{}", kk.to_ascii_lowercase()), vv);
                    }
                }
                g.mod_files.cache.insert(key.clone(), m);
            }
            let ck = format!("{s}:{k}");
            let cached = g.mod_files.cache.get(&key).and_then(|m| m.get(&ck)).cloned();
            let float = name.contains("float") || name == "setiniinteger_cached";
            match name {
                // A missing key gives the default (0 without one); a text value gives 0
                // for the float form, not the default [observed].
                "getinifloatordefault_cached" => XVal::Num(cached.map_or(num(args, 2, 0.0), |v| num_of(&v))),
                "getinistringordefault_cached" => XVal::Str(cached.or_else(|| text(args, 2)).unwrap_or_default()),
                "getinifloatorcreate_cached" | "getinistringorcreate_cached" => {
                    let v = match cached {
                        Some(v) => v,
                        None => {
                            let v = text(args, 2).unwrap_or_default();
                            if let Some(m) = g.mod_files.cache.get_mut(&key) {
                                m.insert(ck, v.clone());
                            }
                            v
                        }
                    };
                    if float { XVal::Num(num_of(&v)) } else { XVal::Str(v) }
                }
                // The cache and the file [hypothesis: not probed].
                _ => {
                    let value = if float { lu_number(num(args, 1, 0.0).trunc()) } else { text(args, 1).unwrap_or_default() };
                    if let Some(m) = g.mod_files.cache.get_mut(&key) {
                        m.insert(ck, value.clone());
                    }
                    let i = raw.find([':', '/', '\\']).unwrap_or(0);
                    let old = g.mod_files.read(&path).unwrap_or_default();
                    let new = ini_set(&old, &s, &k, &value, raw[..i].trim(), raw[i + 1..].trim());
                    g.mod_files.write(&path, new);
                    XVal::Num(1.0)
                }
            }
        }
        "clearfilecacheshowoff" => {
            let path = norm(&text(args, 0).unwrap_or_default());
            let had = g.mod_files.cache.keys().chain(g.mod_files.showoff_json.keys()).any(|k| k.ends_with(&path) && !path.is_empty());
            g.mod_files.cache.retain(|k, _| path.is_empty() || !k.ends_with(&path));
            g.mod_files.showoff_json.retain(|k, _| path.is_empty() || !k.ends_with(&path));
            XVal::Num(f64::from(u8::from(had)))
        }
        // `sectionAndKey [iniPath]`: whether the cached file has the key; the file is
        // cached when it was not [documented: HasINISetting_Cached, alias HasINISettingC].
        "hasinisetting_cached" | "hasinisettingc" => {
            let Some((s, k)) = text(args, 0).as_deref().and_then(section_key) else { return Some(Ok(zero)) };
            let path = ini_path(g, text(args, 1).as_deref());
            let key = cache_ini(g, &path, false);
            XVal::Num(f64::from(u8::from(g.mod_files.cache.get(&key).is_some_and(|m| m.contains_key(&format!("{s}:{k}"))))))
        }
        // `[iniPath]`: the cache read again from the file, what it held lost; 1 when the
        // file exists [documented: ReloadIniCache; hypothesis: a missing file fails and
        // leaves the cache as it was].
        "reloadinicache" => {
            let path = ini_path(g, text(args, 0).as_deref());
            if g.mod_files.read(&path).is_none() {
                return Some(Ok(zero));
            }
            cache_ini(g, &path, true);
            XVal::Num(1.0)
        }
        // `[iniPath]`: the cached keys written to the file, each in place or added to its
        // section; 1 when written, 0 with no cache of the file [documented:
        // SaveCachedIniFile, alias SaveCachedIni; hypothesis: the file's comments and
        // order kept, keys the cache added written in lower case, as the cache keeps them].
        "savecachedinifile" | "savecachedini" => {
            let path = ini_path(g, text(args, 0).as_deref());
            let Some(m) = g.mod_files.cache.get(&norm(&path)) else { return Some(Ok(zero)) };
            let mut pairs: Vec<(String, String)> = m.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            pairs.sort();
            let mut t = g.mod_files.read(&path).unwrap_or_default();
            for (sk, v) in pairs {
                if let Some((s, k)) = sk.split_once(':') {
                    if ini_get(&t, s, k).as_deref() != Some(v.as_str()) {
                        t = ini_set(&t, s, k, &v, s, k);
                    }
                }
            }
            XVal::Num(f64::from(u8::from(g.mod_files.write(&path, t))))
        }
        // ---- ShowOff's JSON files, under the game's folder (`read_root`) ----
        // `filepath [jsonPointer] [parserMode] [useCache]`: the value at the pointer (the
        // root for ""), converted (`json_to_xval`); kept in the cache when asked
        // [documented: ReadFromJSON]. A missing file, a parse error or a pointer to
        // nothing gives 0 [hypothesis: the page names no failure value].
        "readfromjson" => {
            let path = text(args, 0).unwrap_or_default();
            let pointer = text(args, 1).unwrap_or_default();
            let Some(root) = json_file(g, &path) else { return Some(Ok(zero)) };
            if num(args, 3, 0.0) != 0.0 {
                g.mod_files.showoff_json.insert(norm(&path), root.clone());
            }
            match root.pointer(&pointer) {
                Some(v) => {
                    let v = v.clone();
                    json_to_xval(g, &v)
                }
                None => zero,
            }
        }
        // `toWrite filepath [jsonPointer] [parserMode] [useCache] [saveFile]
        // [compactFormatting]`: the value converted and put at the pointer of the file's
        // JSON (a file that is missing or not JSON starts from nothing); cached when
        // asked, saved unless saveFile is 0, minified or indented [documented:
        // WriteToJSON]. Saved into NVForever's layer, never the game's folder
        // (`write_root`). 1 when done [documented: "success"].
        "writetojson" => {
            let Some(value) = args.first() else { return Some(Ok(zero)) };
            let path = text(args, 1).unwrap_or_default();
            if path.trim().is_empty() {
                return Some(Ok(zero));
            }
            let pointer = text(args, 2).unwrap_or_default();
            let new = xval_to_json(g, value, 0);
            let mut root = json_file(g, &path).unwrap_or(serde_json::Value::Null);
            if !json_set(&mut root, &pointer, new) {
                return Some(Ok(zero));
            }
            if num(args, 4, 0.0) != 0.0 {
                g.mod_files.showoff_json.insert(norm(&path), root.clone());
            }
            let mut ok = true;
            if num(args, 5, 1.0) != 0.0 {
                let body = if num(args, 6, 0.0) != 0.0 { serde_json::to_string(&root) } else { serde_json::to_string_pretty(&root) }.unwrap_or_default();
                ok = g.mod_files.write_root(&path, body);
            }
            XVal::Num(f64::from(u8::from(ok)))
        }
        // `folderPath`: a folder under the game's folder, made in NVForever's layer; 1
        // when made [documented: CreateFolder, alias CreateDirectory].
        "createfolder" | "createdirectory" => {
            let rel = text(args, 0).unwrap_or_default().replace('/', "\\");
            XVal::Num(f64::from(u8::from(g.mod_files.make_folder_root(&rel))))
        }
        // `list [fileName] [bAppend]`: the list's forms, to the console, or to a file under
        // the game's folder, added after its text unless bAppend is 0 [documented:
        // DumpFormList, alias FListDump]. The lines are NVForever's own words.
        "dumpformlist" | "flistdump" => {
            let l = match args.first() {
                Some(XVal::Ref(r)) if *r != 0 => fnv_sim::FormId(*r),
                _ => return Some(Ok(zero)),
            };
            let members = crate::functions::list_members(g, l);
            let mut lines = vec![format!("Form list {} ({:08X}): {} forms", g.data.name(l), l.0, members.len())];
            lines.extend(members.iter().enumerate().map(|(i, f)| format!("  {i}: {:08X} {}", f.0, g.data.name(*f))));
            match text(args, 1).filter(|f| !f.trim().is_empty()) {
                None => g.console_out.extend(lines),
                Some(file) => {
                    let body = lines.join("\n");
                    let t = match g.mod_files.read_root(&file).filter(|_| num(args, 2, 1.0) != 0.0) {
                        Some(old) if !old.is_empty() => format!("{old}\n{body}"),
                        _ => body,
                    };
                    g.mod_files.write_root(&file, t);
                }
            }
            zero
        }
        // ---- MCM: "File/Section/Key" of Data\Config\File.ini; a number is written with
        // four decimals [observed: "fMcm=5.0000"] ----
        "getmodinisetting" | "setmodinisetting" => {
            let raw = text(args, 0).unwrap_or_default();
            let mut parts = raw.splitn(3, ['/', '\\']);
            let (Some(file), Some(sec), Some(key)) = (parts.next(), parts.next(), parts.next()) else { return Some(Ok(zero)) };
            let path = ini_path(g, Some(file));
            if name == "getmodinisetting" {
                return Some(Ok(match g.mod_files.read(&path).and_then(|t| ini_get(&t, &sec.to_ascii_lowercase(), key)) {
                    Some(v) => match v.trim().parse::<f64>() {
                        Ok(n) => XVal::Num(n),
                        Err(_) => XVal::Str(v),
                    },
                    None => zero,
                }));
            }
            let value = match args.get(1) {
                Some(XVal::Num(n)) => format!("{n:.4}"),
                Some(XVal::Str(s)) => s.clone(),
                _ => String::new(),
            };
            let old = g.mod_files.read(&path).unwrap_or_default();
            let new = ini_set(&old, &sec.to_ascii_lowercase(), &key.to_ascii_lowercase(), &value, sec, key);
            g.mod_files.write(&path, new);
            zero
        }
        // ---- JIP / LU files ----
        // 1 a loose file, 2 inside an archive, 0 none; with isFolder, a folder
        // [observed: a loose plugin 1, a mesh in a BSA 2, "NVSE" as a folder 1].
        "fileexists" => {
            let p = text(args, 0).unwrap_or_default().replace('/', "\\");
            let folder = num(args, 1, 0.0) != 0.0;
            let r = roots();
            // In any case, and never outside the root (`find_dir`, `find_file`).
            let loose = r.read.iter().any(|root| if folder { find_dir(root, &p).is_some() } else { find_file(root, &p).is_some() }) ||(!folder && g.mod_files.written.contains_key(&norm(&p)));
            if loose {
                XVal::Num(1.0)
            } else if !folder {
                XVal::Num(f64::from(r.lookup.as_ref().map_or(0, |f| f(&p))))
            } else {
                zero
            }
        }
        // `folder [filter]`: the file names (no path) of a folder under the game's
        // folder and NVForever's layer [observed: "NVSE\Plugins" "*.dll" gives bare
        // names]; a missing folder gives an empty array [observed: plugin-odds files.none,
        // ar_Size 0, where an unset array reads -1].
        // GetFoldersInFolder the same for folders [observed: gaps3, "NVSE" gives
        // "Plugins"; a missing folder an empty array].
        "getfilesinfolder" | "getfoldersinfolder" => {
            let folders = name == "getfoldersinfolder";
            let p = text(args, 0).unwrap_or_default().replace('/', "\\");
            let filter = text(args, 1).unwrap_or_else(|| "*".into()).to_ascii_lowercase();
            let mut names: Vec<String> = Vec::new();
            for root in roots().read {
                // A path under the game folder (NVSE\Plugins) or under Data.
                for base in [root.parent().map(|x| x.to_path_buf()), Some(root.clone())].into_iter().flatten() {
                    if let Ok(rd) = std::fs::read_dir(base.join(&p)) {
                        for e in rd.flatten() {
                            if e.path().is_dir() == folders {
                                let n = e.file_name().to_string_lossy().to_string();
                                if glob(&filter, &n.to_ascii_lowercase()) && !names.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
                                    names.push(n);
                                }
                            }
                        }
                    }
                }
            }
            names.sort_by_key(|n| n.to_ascii_lowercase());
            let entries = names.into_iter().enumerate().map(|(i, n)| (XKey::Num(i as f64), XVal::Str(n))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        _ => return None,
    };
    Some(Ok(v))
}

/// A `*` / `?` file-name pattern.
fn glob(pat: &str, s: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some(b'*'), _) => go(&p[1..], s) || (!s.is_empty() && go(p, &s[1..])),
            (Some(b'?'), Some(_)) => go(&p[1..], &s[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &s[1..]),
            _ => false,
        }
    }
    go(pat.as_bytes(), s.as_bytes())
}

#[cfg(test)]
mod path_tests {
    use super::{on_disk, on_disk_any_case};
    use std::path::Path;

    #[test]
    fn a_data_path_matches_folders_in_any_case() {
        let base = std::env::temp_dir().join(format!("nvf-any-case-{}", std::process::id()));
        std::fs::create_dir_all(base.join("NVSE").join("Plugins").join("Scripts")).unwrap();
        std::fs::write(base.join("NVSE").join("Plugins").join("Scripts").join("gr_A.txt"), "").unwrap();
        let found = on_disk_any_case(&base, "nvse\\plugins\\scripts\\GR_a.txt").expect("found in any case");
        assert!(found.is_file());
        assert_eq!(on_disk_any_case(&base, "nvse\\plugins\\missing.txt"), None);
        assert_eq!(on_disk_any_case(&base, "..\\x"), None, "the plain-name rule of on_disk still holds");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn only_plain_names_stay_under_the_base() {
        let base = Path::new("layer");
        assert_eq!(on_disk(base, "Config\\x.ini"), Some(base.join("Config").join("x.ini")));
        assert_eq!(on_disk(base, "Config//x.ini"), Some(base.join("Config").join("x.ini")));
        for bad in ["..\\..\\x", "../x", "Config\\..\\..\\x", "C:\\x", "C:x", "\\x", "/x", ".\\x", "x:stream", "\\\\server\\share\\x"] {
            assert_eq!(on_disk(base, bad), None, "{bad}");
        }
    }
}
