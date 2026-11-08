//! A digest of each field of `GameData`, one line a field, for comparing two builds
//! (build_identity.rs, build_shortcuts.rs). Only digests and counts, no record's text.

use fnv_game::GameData;
use fnv_script::FunctionTable;
use std::collections::hash_map::DefaultHasher;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

/// One text's digest [design: SipHash with `DefaultHasher::new()`'s fixed keys, so the same
/// text gives the same number in every run].
fn text_digest(lines: &[String]) -> u64 {
    let mut h = DefaultHasher::new();
    lines.hash(&mut h);
    h.finish()
}

/// The digest of a value with no hash map in it.
fn ordered<T: Debug>(v: &T) -> u64 {
    text_digest(&[format!("{v:?}")])
}

/// The digest of a collection in no order of its own (a hash map or set): its items, sorted
/// by their text.
fn unordered<T: Debug>(items: impl IntoIterator<Item = T>) -> (usize, u64) {
    let mut lines: Vec<String> = items.into_iter().map(|i| format!("{i:?}")).collect();
    lines.sort_unstable();
    (lines.len(), text_digest(&lines))
}

macro_rules! map_fields {
    ($out:ident, $d:ident, $($f:ident),* $(,)?) => {
        $(
            let (n, h) = unordered($d.$f.iter());
            $out.push(format!("{}: {n} {h:016x}", stringify!($f)));
        )*
    };
}

macro_rules! value_fields {
    ($out:ident, $d:ident, $($f:ident),* $(,)?) => {
        $($out.push(format!("{}: {:016x}", stringify!($f), ordered(&$d.$f)));)*
    };
}

/// One line for each field of the build but the table ([`table_lines`]).
pub fn data_lines(d: &GameData) -> Vec<String> {
    let mut out = Vec::new();
    map_fields!(
        out, d, scripts, infos, topics, shared_lines, linked_from, follow_up_targets, topic_defs, packages, patrol_scripts, package_defs, messages, terminals, notes,
        form_lists, repair_lists, menu_icons, actors, combat_styles, leveled_actors, form_script, edids, edid_of, form_flags, item_order, object_bounds, full_names,
        sigs, reputations, reputation_names, challenges, challenge_thresholds, challenge_defs, regions, magic_effects, effect_items, ingestibles, spell_types,
        spell_flags, race_spells, voices_without_default, armor_enchantments, enchantment_types, child_races, race_older, race_younger, faction_reactions,
        faction_flags, faction_reputations, cells, owner_ranks, destructibles, weapon_skills, weapon_anim_types, furniture, idle_markers, idles, idle_children,
        image_modifiers, station_flags, perks, actor_value_infos, recipes, camera_paths, camera_shots, recipe_categories, casinos, caravan_cards, caravan_decks,
        need_stages, water_effects, drink_waters, water_damage, radiation_markers, class_services, unit_by_key,
    );
    value_fields!(out, d, load_order, plugin_versions, quests, lines_moved_by_previous, top_level, greeting, goodbye, ini_texts, scripted_by_cell, default_objects, issues);
    // In order, with each unit's whole text: the units' ids are their places in this list.
    out.push(format!("units: {} {:016x}", d.units.len(), text_digest(&d.units.iter().map(|c| format!("{:?} {:?} {} {}", c.unit, c.program, c.raw, c.key())).collect::<Vec<_>>())));
    out.push(format!("read_issues: {:016x} {:016x}", ordered(&d.read_issues.counts), ordered(&d.read_issues.samples)));
    out.push(format!("param_types: {:?}", unordered(d.param_types.0.iter())));
    // Each kept record's fields are decoded here, on first asking, as a script's getter would.
    let (n, h) = unordered(d.sigs.keys().filter_map(|f| d.record_fields.get(f).map(|x| (f, x))));
    out.push(format!("record_fields: {} {n} {h:016x}", d.record_fields.len()));
    out
}

/// One line for each part of a function table. Its name index is private: it is asked for
/// every function's name.
pub fn table_lines(t: &FunctionTable) -> Vec<String> {
    let names: Vec<String> = t.functions.values().map(|f| format!("{} {:?}", f.name, t.by_name(&f.name))).collect();
    vec![
        format!("table functions: {} {:016x}", t.functions.len(), ordered(&t.functions)),
        format!("table blocks: {:016x}", ordered(&t.blocks)),
        format!("table enums: {:016x}", ordered(&t.enums)),
        format!("table begin_params: {:016x}", ordered(&t.begin_params)),
        format!("table unnumbered: {:?}", unordered(t.unnumbered.iter())),
        format!("table names: {:016x}", text_digest(&names)),
    ]
}

/// Every line of the build.
pub fn lines(d: &GameData) -> Vec<String> {
    let mut out = data_lines(d);
    out.extend(table_lines(&d.table));
    out
}
