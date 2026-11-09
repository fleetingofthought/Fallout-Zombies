//! The Pip-Boy's labels from the load order's string settings (#1443).
//!
//! The menu files name a game setting for most of the Pip-Boy's words (`inventory_menu.xml`'s
//! `IM_Headline_PlayerWGInfo` shows `&-sInventoryWeight;`, `stats_menu.xml`'s
//! `stats_tailline_status` shows `&-sStatsStatus;`, `map_menu.xml`'s `MM_Headline_Title`
//! `&-sData;`, `item_stats_display.xml`'s `WeightInfo` `&-sInventoryWeightUpper;`)
//! [documented: the menu files of `Fallout - Misc.bsa`]. The page names and tabs have
//! settings of their own that no menu file names (`sStats`, `sInventoryItems`,
//! `sInventoryWeapons` … `sInventoryAmmo`, `sLocalMapTabText` … `sCommsTabText`)
//! [observed: a Russian install's language plugin defines them, and each one's text there is
//! the word of the place it is used here, 2026-11-08, docs/spec-notes/pipboy.md §4 "The words"].
//!
//! The English masters define none of these settings [observed: FalloutNV.esm and the
//! add-ons, 2026-11-08], so an English install shows NVForever's own words, the ones
//! written beside each name at its use. A localized install's language plugin defines
//! them, and its words are drawn [design].
//!
//! The settings are read once, when the simulation's tables are built ([`set`]): the load
//! order is fixed for the run.

use fnv_sim::settings::Setting;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The load order's string settings by name without case, trimmed, empty ones left out.
static DEFINED: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Keep the load order's string settings for [`label`] (once a run; a later call is
/// ignored).
pub fn set(settings: &HashMap<String, Setting>) {
    let _ = DEFINED.set(strings(settings));
}

fn strings(settings: &HashMap<String, Setting>) -> HashMap<String, String> {
    settings
        .iter()
        .filter_map(|(k, v)| match v {
            Setting::Str(s) if !s.trim().is_empty() => Some((k.to_ascii_lowercase(), s.trim().to_string())),
            _ => None,
        })
        .collect()
}

/// The setting `name`'s text when the load order defines it, else `ours`, NVForever's own
/// word for that place.
pub fn label(name: &str, ours: &'static str) -> &'static str {
    label_in(DEFINED.get(), name, ours)
}

fn label_in<'a>(defined: Option<&'a HashMap<String, String>>, name: &str, ours: &'a str) -> &'a str {
    defined.and_then(|m| m.get(&name.to_ascii_lowercase())).map_or(ours, String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_defined_setting_wins_else_our_word() {
        let mut s = HashMap::new();
        s.insert("sinventoryweight".to_string(), Setting::Str("Вс".into()));
        s.insert("sinventorycaps".to_string(), Setting::Str("  ".into()));
        s.insert("fsomething".to_string(), Setting::Float(1.0));
        let m = strings(&s);
        assert_eq!(label_in(Some(&m), "sInventoryWeight", "Wg"), "Вс");
        // Empty, missing or not a string: our word.
        assert_eq!(label_in(Some(&m), "sInventoryCaps", "Caps"), "Caps");
        assert_eq!(label_in(Some(&m), "sStatsHP", "HP"), "HP");
        assert_eq!(label_in(Some(&m), "fSomething", "x"), "x");
        // Before the settings are read (tests, the viewer): our word.
        assert_eq!(label_in(None, "sInventoryWeight", "Wg"), "Wg");
    }
}
