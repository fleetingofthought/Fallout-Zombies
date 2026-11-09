//! NVForever's own list of plugins, `plugins.txt` in its home (owner decision 2026-10-16:
//! its own list rather than the original game's).
//!
//! The original game keeps the plugins it loads in `%LOCALAPPDATA%\FalloutNV\plugins.txt`,
//! one file name per line [observed: the owner's PC], and orders them by the files'
//! modified dates [hypothesis: modding tools' documentation, not checked]. NVForever never
//! reads or writes that file, so turning a mod on for one game leaves the other as it was.
//! The one exception is a start from Mod Organizer 2, which puts its profile's list at
//! that path for the programs it starts ([`crate::mo2`]).
//!
//! The format here is our own choice:
//!
//! ```text
//! ; comments start with ; or #
//! *FalloutNV.esm
//! *DeadMoney.esm
//! SomeMod.esp
//! ```
//!
//! Every plugin in the game's `Data` folder has a line; a `*` in front means it is loaded,
//! and the lines' order is the load order. A plugin that appears in `Data` later gets a
//! line of its own at the next start: off, except the game's own masters
//! (`FalloutNV.esm`, [`ADDON_MASTERS`]), which are on as they were before this list
//! existed. A new `.esm` goes after the last `.esm` line, so masters stay before the `.esp`
//! files [hypothesis: the extension stands for the header's master flag, true of the
//! game's own files]. Lines are never removed: a plugin no longer in `Data` is skipped
//! with a warning, and comes back on if the file returns.
//!
//! **Language plugins** (#1443): a localized install can keep the English masters and put
//! its text in one more plugin per master, `<master>_lang.esp` [observed: a Russian
//! install, docs/spec-notes/fonts.md §2.1]. Such a plugin of one of the game's masters
//! ([`is_language_plugin`]) gets its new line on, like the masters, after the `.esm`
//! lines and the language plugins already listed, so the masters' text is replaced but
//! mods listed later still win; in a new list they follow the masters in the masters'
//! order [design].

use crate::{guard_write, EngineHome, GameInstall, InstallError, ADDON_MASTERS};

/// The list's file in the home.
pub const PLUGINS_FILE: &str = "plugins.txt";

const HEADER: &str = "; NVForever's plugins, in load order (top loads first). A * in front of a plugin turns\n\
; it on; take the * away to turn it off, or move a line to change the order. Plugins new\n\
; in the game's Data folder are added at the next start, turned off.\n\
; NVForever has its own list: the original game's plugins.txt is not read or changed.\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginList {
    pub entries: Vec<PluginEntry>,
}

/// What [`sync`] found: the plugins to load, in order, and what the player should hear.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginSync {
    /// The active plugins present in `Data`, in load order; `FalloutNV.esm` first.
    pub load: Vec<String>,
    /// Plugins given a line this time (new in `Data`, or the list was just made).
    pub added: Vec<String>,
    /// Active lines whose file is not in `Data`.
    pub missing: Vec<String>,
    /// `FalloutNV.esm` was off or not first in the file; it is loaded first anyway.
    pub base_forced: bool,
    /// The file was written (made, or lines added).
    pub written: bool,
}

fn is_plugin(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".esm") || l.ends_with(".esp")
}

fn is_esm(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".esm")
}

/// The game's own masters, which a new line turns on.
fn is_game_master(name: &str) -> bool {
    name.eq_ignore_ascii_case("FalloutNV.esm") || ADDON_MASTERS.iter().any(|m| m.eq_ignore_ascii_case(name))
}

/// The language plugin of one of the game's masters: `<master>_lang.esp`, e.g.
/// `FalloutNV_lang.esp`, `DeadMoney_lang.esp` (module notes).
pub fn is_language_plugin(name: &str) -> bool {
    language_master_rank(name).is_some()
}

/// The rank ([`master_rank`]) of the master a language plugin belongs to.
fn language_master_rank(name: &str) -> Option<usize> {
    let l = name.to_ascii_lowercase();
    let stem = l.strip_suffix("_lang.esp")?;
    let rank = master_rank(&format!("{stem}.esm"));
    (rank != usize::MAX).then_some(rank)
}

/// Where a plugin sorts in a new list: the game's masters in their order, the other
/// `.esm` files, the language plugins in their masters' order, the other `.esp` files.
fn sort_key(name: &str) -> (u8, usize, String) {
    let lower = name.to_ascii_lowercase();
    match (master_rank(name), language_master_rank(name)) {
        (r, _) if r != usize::MAX => (0, r, lower),
        (_, Some(r)) => (2, r, lower),
        _ if is_esm(name) => (1, 0, lower),
        _ => (3, 0, lower),
    }
}

/// Where a game master sorts among the others: `FalloutNV.esm`, then [`ADDON_MASTERS`].
fn master_rank(name: &str) -> usize {
    if name.eq_ignore_ascii_case("FalloutNV.esm") {
        return 0;
    }
    ADDON_MASTERS.iter().position(|m| m.eq_ignore_ascii_case(name)).map_or(usize::MAX, |i| i + 1)
}

/// The `.esm` and `.esp` files in the game's `Data`, as named there: the game's masters in
/// their order, then the other `.esm` files, then the language plugins in their masters'
/// order, then the other `.esp` files by name.
pub fn data_plugins(game: &GameInstall) -> Result<Vec<String>, InstallError> {
    let rd = std::fs::read_dir(&game.data).map_err(|e| InstallError::Io(game.data.clone(), e))?;
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_plugin(n))
        .collect();
    names.sort_by_key(|n| sort_key(n));
    Ok(names)
}

impl PluginList {
    pub fn parse(text: &str) -> PluginList {
        let mut entries: Vec<PluginEntry> = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
                continue;
            }
            let (active, name) = match t.strip_prefix('*') {
                Some(rest) => (true, rest.trim()),
                None => (false, t),
            };
            // The first line of a name wins.
            if !name.is_empty() && !entries.iter().any(|e| e.name.eq_ignore_ascii_case(name)) {
                entries.push(PluginEntry { name: name.to_string(), active });
            }
        }
        PluginList { entries }
    }

    pub fn to_text(&self) -> String {
        let mut s = HEADER.to_string();
        for e in &self.entries {
            s.push_str(if e.active { "*" } else { "" });
            s.push_str(&e.name);
            s.push('\n');
        }
        s
    }

    fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Gives each of `present` (in [`data_plugins`]' order) without a line one: a game
    /// master or a language plugin on, anything else off; an `.esm` after the last `.esm`
    /// line, a language plugin after the last `.esm` or language plugin line, another
    /// `.esp` at the end. Returns the names added.
    pub fn add_new(&mut self, present: &[String]) -> Vec<String> {
        let mut added = Vec::new();
        for name in present {
            if self.contains(name) {
                continue;
            }
            let language = is_language_plugin(name);
            let entry = PluginEntry { name: name.clone(), active: is_game_master(name) || language };
            if is_esm(name) {
                let at = self.entries.iter().rposition(|e| is_esm(&e.name)).map_or(0, |i| i + 1);
                self.entries.insert(at, entry);
            } else if language {
                let at = self.entries.iter().rposition(|e| is_esm(&e.name) || is_language_plugin(&e.name)).map_or(0, |i| i + 1);
                self.entries.insert(at, entry);
            } else {
                self.entries.push(entry);
            }
            added.push(name.clone());
        }
        added
    }

    /// The active plugins of `present`, in the list's order, named as in `Data`;
    /// `FalloutNV.esm` first whatever the list says. Also the active names not present, and
    /// whether `FalloutNV.esm` had to be put first.
    pub fn load_order(&self, present: &[String]) -> (Vec<String>, Vec<String>, bool) {
        let mut load = Vec::new();
        let mut missing = Vec::new();
        for e in self.entries.iter().filter(|e| e.active) {
            match present.iter().find(|p| p.eq_ignore_ascii_case(&e.name)) {
                Some(p) => load.push(p.clone()),
                None => missing.push(e.name.clone()),
            }
        }
        let base = present.iter().find(|p| p.eq_ignore_ascii_case("FalloutNV.esm")).cloned().unwrap_or_else(|| "FalloutNV.esm".into());
        let forced = load.first() != Some(&base);
        if forced {
            load.retain(|p| p != &base);
            load.insert(0, base);
        }
        (load, missing, forced)
    }
}

impl EngineHome {
    pub fn plugins_file(&self) -> std::path::PathBuf {
        self.root.join(PLUGINS_FILE)
    }
}

/// Reads the home's plugin list (made when there is none), gives the plugins new in
/// `Data` their lines, writes it back when that changed it, and returns what to load.
pub fn sync(home: &EngineHome, game: &GameInstall) -> Result<PluginSync, InstallError> {
    let path = home.plugins_file();
    let mut list = match std::fs::read_to_string(&path) {
        Ok(t) => PluginList::parse(&t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PluginList::default(),
        Err(e) => return Err(InstallError::Io(path, e)),
    };
    let present = data_plugins(game)?;
    let added = list.add_new(&present);
    let written = !added.is_empty() || !path.exists();
    if written {
        guard_write(&path, Some(game))?;
        std::fs::create_dir_all(&home.root).map_err(|e| InstallError::Io(home.root.clone(), e))?;
        std::fs::write(&path, list.to_text()).map_err(|e| InstallError::Io(path.clone(), e))?;
    }
    let (load, missing, base_forced) = list.load_order(&present);
    Ok(PluginSync { load, added, missing, base_forced, written })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_and_write_back() {
        let l = PluginList::parse("; c\n# c\n*FalloutNV.esm\n  * DeadMoney.esm \nMod.esp\nmod.ESP\n\n");
        assert_eq!(
            l.entries,
            vec![
                PluginEntry { name: "FalloutNV.esm".into(), active: true },
                PluginEntry { name: "DeadMoney.esm".into(), active: true },
                PluginEntry { name: "Mod.esp".into(), active: false },
            ]
        );
        assert_eq!(PluginList::parse(&l.to_text()), l);
    }

    #[test]
    fn new_plugins_masters_on_mods_off() {
        let mut l = PluginList::parse("*FalloutNV.esm\n*Old.esp\n");
        let added = l.add_new(&names(&["FalloutNV.esm", "HonestHearts.esm", "ModMaster.esm", "New.esp"]));
        assert_eq!(added, names(&["HonestHearts.esm", "ModMaster.esm", "New.esp"]));
        let text = l.to_text();
        let body: Vec<&str> = text.lines().filter(|s| !s.starts_with(';')).collect();
        assert_eq!(body, ["*FalloutNV.esm", "*HonestHearts.esm", "ModMaster.esm", "*Old.esp", "New.esp"]);
    }

    #[test]
    fn language_plugins_are_the_game_masters_own() {
        for n in ["FalloutNV_lang.esp", "falloutnv_LANG.ESP", "DeadMoney_lang.esp", "CaravanPack_lang.esp"] {
            assert!(is_language_plugin(n), "{n}");
        }
        for n in ["MyMod_lang.esp", "FalloutNV_lang.esm", "FalloutNV.esp", "_lang.esp", "FalloutNV-lang.esp"] {
            assert!(!is_language_plugin(n), "{n}");
        }
    }

    #[test]
    fn language_plugins_follow_the_masters_and_are_on() {
        // A new list: in Data's order, the language plugins after the masters and the other
        // .esm files, in the masters' order, before the other .esp files.
        let mut present = names(&["Zed.esp", "DeadMoney_lang.esp", "Mod.esm", "FalloutNV_lang.esp", "Aaa.esp", "DeadMoney.esm", "FalloutNV.esm"]);
        present.sort_by_key(|n| sort_key(n));
        assert_eq!(present, names(&["FalloutNV.esm", "DeadMoney.esm", "Mod.esm", "FalloutNV_lang.esp", "DeadMoney_lang.esp", "Aaa.esp", "Zed.esp"]));
        let mut l = PluginList::default();
        l.add_new(&present);
        let body: Vec<String> = l.to_text().lines().filter(|s| !s.starts_with(';')).map(str::to_string).collect();
        assert_eq!(body, ["*FalloutNV.esm", "*DeadMoney.esm", "Mod.esm", "*FalloutNV_lang.esp", "*DeadMoney_lang.esp", "Aaa.esp", "Zed.esp"]);
        // In a list with mods, a new language plugin goes before them, so they still win.
        let mut l = PluginList::parse("*FalloutNV.esm\n*Mod.esp\n");
        assert_eq!(l.add_new(&names(&["FalloutNV.esm", "FalloutNV_lang.esp", "Mod.esp"])), names(&["FalloutNV_lang.esp"]));
        let body: Vec<String> = l.to_text().lines().filter(|s| !s.starts_with(';')).map(str::to_string).collect();
        assert_eq!(body, ["*FalloutNV.esm", "*FalloutNV_lang.esp", "*Mod.esp"]);
        // A line the player turned off stays off.
        let mut l = PluginList::parse("*FalloutNV.esm\nFalloutNV_lang.esp\n");
        assert!(l.add_new(&names(&["FalloutNV.esm", "FalloutNV_lang.esp"])).is_empty());
        assert!(!l.entries[1].active);
    }

    #[test]
    fn load_order_follows_lines_and_skips_missing() {
        let l = PluginList::parse("*B.esp\n*FalloutNV.esm\nC.esp\n*Gone.esp\n*a.esp\n");
        let (load, missing, forced) = l.load_order(&names(&["FalloutNV.esm", "A.esp", "B.esp", "C.esp"]));
        assert_eq!(load, names(&["FalloutNV.esm", "B.esp", "A.esp"]));
        assert_eq!(missing, names(&["Gone.esp"]));
        assert!(forced);
    }

    #[test]
    fn sync_makes_the_list_then_adds_new_files() {
        let t = std::env::temp_dir().join(format!("nvf-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let data = t.join("game").join("Data");
        std::fs::create_dir_all(&data).unwrap();
        for f in ["FalloutNV.esm", "DeadMoney.esm", "Mod.esp", "Readme.txt"] {
            std::fs::write(data.join(f), b"TES4").unwrap();
        }
        let game = GameInstall::open(t.join("game")).unwrap();
        let home = EngineHome::new(t.join("home"));
        let s = sync(&home, &game).unwrap();
        assert!(s.written);
        assert_eq!(s.load, names(&["FalloutNV.esm", "DeadMoney.esm"]));
        assert_eq!(s.added, names(&["FalloutNV.esm", "DeadMoney.esm", "Mod.esp"]));
        // The player turns the mod on; nothing new: the file is left alone.
        std::fs::write(home.plugins_file(), "*FalloutNV.esm\n*DeadMoney.esm\n*Mod.esp\n").unwrap();
        let s = sync(&home, &game).unwrap();
        assert!(!s.written);
        assert_eq!(s.load, names(&["FalloutNV.esm", "DeadMoney.esm", "Mod.esp"]));
        // A new mod appears in Data: a line, off.
        std::fs::write(data.join("Later.esp"), b"TES4").unwrap();
        let s = sync(&home, &game).unwrap();
        assert_eq!(s.added, names(&["Later.esp"]));
        assert!(std::fs::read_to_string(home.plugins_file()).unwrap().ends_with("*Mod.esp\nLater.esp\n"));
        std::fs::remove_dir_all(&t).ok();
    }
}
