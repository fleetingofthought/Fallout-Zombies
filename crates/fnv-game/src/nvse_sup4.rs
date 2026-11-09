//! SUP NVSE, batch 4 (#1008; docs/spec-notes/ext-sup4.md): the last 17 SUP functions.
//! Random array keys, invalid references in arrays, floats from an array, a UDF a number
//! of frames later, the compass targets, the last save's and load's names and sizes, the
//! cells and map markers near the player, scripts marked after a load, killing the
//! player's enemies, the caught-pickpocketing flag and Cyrillic to Latin letters
//! [documented: each page in the wiki export].

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use rand_core::RngCore;
use std::collections::BTreeSet;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn list(g: &mut Game, items: Vec<XVal>) -> XVal {
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

/// A save file as GetSaveName and the size getters give it: its name without the
/// extension and its size in bytes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SaveFile {
    pub name: String,
    pub bytes: u64,
}

/// The last save written and the last one loaded this session; the engine sets them
/// (`nvf_play::save`), and they outlast a load (`Game::take_session`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SaveFiles {
    pub saved: Option<SaveFile>,
    pub loaded: Option<SaveFile>,
}

/// Batch 4's state that a load clears: the scripts MarkScriptOnLoad marked since it.
#[derive(Debug, Clone, Default)]
pub struct Sup4State {
    pub marked: BTreeSet<FormId>,
}

/// The array's entries, `None` for no array.
fn entries(g: &Game, v: Option<&XVal>) -> Option<Vec<(XKey, XVal)>> {
    let id = match v? {
        XVal::Array(id) => *id,
        XVal::Num(x) if *x >= 1.0 => *x as u32,
        _ => return None,
    };
    Some(g.ext.array(id)?.entries.clone())
}

/// A reference in an array that is no longer valid: none (0), or a form neither the
/// load order nor the world knows (a removed temporary reference) [documented:
/// Ar_HasInvalidRefs, "origin mod is disabled or temporary reference … removed"].
fn invalid_ref(g: &Game, r: u32) -> bool {
    let f = FormId(r);
    r == 0 || (g.data.sig(f).is_none() && g.world.peek(f).is_none() && g.world.placed.get(f).is_none() && f != FormId::PLAYER_REF)
}

/// Cyrillic letters to Latin ones [decision: NVForever's own table, a common Russian
/// romanisation with the Ukrainian and Belarusian letters added (the page names no
/// scheme); an upper-case letter starts its Latin letters in upper case]. Other
/// characters are kept.
pub fn translit(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let lower = c.to_lowercase().next().unwrap_or(c);
        let latin = match lower {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' | 'ґ' => "g",
            'д' => "d",
            'е' | 'э' => "e",
            'ё' => "yo",
            'є' => "ye",
            'ж' => "zh",
            'з' => "z",
            'и' | 'і' => "i",
            'ї' => "yi",
            'й' | 'ы' => "y",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ў' => "w",
            'ф' => "f",
            'х' => "kh",
            'ц' => "ts",
            'ч' => "ch",
            'ш' => "sh",
            'щ' => "shch",
            'ъ' | 'ь' => "",
            'ю' => "yu",
            'я' => "ya",
            _ => {
                out.push(c);
                continue;
            }
        };
        if c != lower {
            let mut it = latin.chars();
            if let Some(first) = it.next() {
                out.extend(first.to_uppercase());
                out.push_str(it.as_str());
            }
        } else {
            out.push_str(latin);
        }
    }
    out
}

/// The cells within `depth` of the player's (their cell alone indoors); -1 takes
/// uGridsToLoad's, 5 cells across, so 2 [documented: GetNearCells, "-1 … uGridsToLoad";
/// hypothesis: the default 5, as GetGridsToLoad].
fn near_cells(g: &Game, depth: f64) -> Vec<FormId> {
    let Some(here) = g.cell_of(FormId::PLAYER_REF) else { return Vec::new() };
    let depth = if depth < 0.0 { 2 } else { depth as i64 };
    if depth == 0 {
        return vec![here];
    }
    g.scan_cells(here, depth)
}

pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // A joke page ("Two. Can play at that game. (no, they can't)") [documented];
        // there is one player [decision: nothing is done].
        "allocateplayer" => zero,
        // A random key of the array: a number for Ar_GetRandomKey, a string for the map
        // form (a number key given as its text) [documented]; an empty array 0 [observed:
        // sup4 randomkey], "" for the map form [hypothesis]. The original's map key was
        // neither of the two keys of `Ar_Map "LALALA"::1, "TROLOLO"::2` by `==` [observed:
        // sup4 randomkey; open: what it gives is not known].
        "ar_getrandomkey" | "ar_getrandomkeymap" => {
            let keys: Vec<XKey> = entries(g, args.first()).unwrap_or_default().into_iter().map(|(k, _)| k).collect();
            let pick = (!keys.is_empty()).then(|| keys[g.rng.next_u32() as usize % keys.len()].clone());
            match (name, pick) {
                ("ar_getrandomkey", Some(XKey::Num(k))) => XVal::Num(k),
                ("ar_getrandomkey", _) => zero,
                (_, Some(XKey::Str(s))) => XVal::Str(s),
                (_, Some(XKey::Num(k))) => XVal::Str(fnv_ui::Value::Num(k).text()),
                _ => XVal::Str(String::new()),
            }
        }
        // `array [dimension]`: 1 when the array holds an invalid reference; with a
        // dimension the inner array at that key is searched [documented: the page's
        // second example, `Ar_HasInvalidRefs ar_myArr 1` searching ar_myArr[1]].
        "ar_hasinvalidrefs" => {
            let mut es = entries(g, args.first()).unwrap_or_default();
            if let Some(k) = args.get(1).map(|_| num(args, 1, 0.0)) {
                let inner = es.into_iter().find(|(key, _)| *key == XKey::Num(k)).map(|(_, v)| v);
                es = entries(g, inner.as_ref()).unwrap_or_default();
            }
            let bad = es.iter().any(|(_, v)| matches!(v, XVal::Ref(r) if invalid_ref(g, *r)));
            XVal::Num(f64::from(u8::from(bad)))
        }
        // `array f1 … f10`: the VM writes the variables (`fnv_script::vm` `out_params`):
        // 0, then the array's numbers in key order [documented: the example, f1 3, f2 8,
        // f3 5]; an element that is no number gives 0 [hypothesis].
        "ar_setfloatsfrom" => {
            let vals: Vec<XVal> = entries(g, args.first())
                .unwrap_or_default()
                .into_iter()
                .take(10)
                .map(|(_, v)| {
                    XVal::Num(match v {
                        XVal::Num(x) => x,
                        _ => 0.0,
                    })
                })
                .collect();
            list(g, std::iter::once(zero).chain(vals).collect())
        }
        // `udf [frames] [removeOnLoad]`: the UDF `frames` frames later (1 to 1000, 1
        // without), on the calling reference; with removeOnLoad 0 it outlasts a load
        // [documented: the page's notes and examples]. A lambda does not outlast a load
        // [hypothesis: as the extender's other kept callbacks, `Game::take_session`].
        "callfunctionnextframe" => {
            let Some(f) = args.first().filter(|f| matches!(f, XVal::Ref(r) if *r != 0)).cloned() else { return Some(Ok(zero)) };
            let frames = num(args, 1, 1.0).clamp(1.0, 1000.0) as u64;
            let keep = num(args, 2, 1.0) == 0.0;
            crate::delayed::call_in_frames(g, f, subject.or(ctx.self_ref), frames, keep);
            zero
        }
        // The current objective's compass targets, one array of references [documented:
        // GetCurrentQuestObjectiveTeleportLinksAlt, "the current compass marker
        // objects"]: GetCurrentQuestObjectiveTeleportLinks's targets [hypothesis: the
        // same objects].
        "getcurrentquestobjectiveteleportlinksalt" => return crate::nvse::call(g, ctx, "getcurrentquestobjectiveteleportlinks", subject, &[]),
        // `type`: 0 the save file's bytes; 1 the .nvse co-save's, 0 [decision: NVForever
        // writes the extender's state into its one save file]; -1 before any save or
        // load this session [documented].
        "getloadedsavesize" | "getsavedsavesize" => {
            let f = if name == "getloadedsavesize" { &g.save_files.loaded } else { &g.save_files.saved };
            match (f, num(args, 0, 0.0) as i64) {
                (None, _) => XVal::Num(-1.0),
                (Some(f), 0) => XVal::Num(f.bytes as f64),
                (Some(_), _) => zero,
            }
        }
        // `type`: 0 the last save's name, 1 the last load's, without the extension; ""
        // before any [documented].
        "getsavename" => {
            let f = if num(args, 0, 0.0) as i64 == 1 { &g.save_files.loaded } else { &g.save_files.saved };
            XVal::Str(f.as_ref().map(|f| f.name.clone()).unwrap_or_default())
        }
        // `depth`: the cells near the player [documented: "references of near cells"].
        "getnearcells" => {
            let cells = near_cells(g, num(args, 0, 0.0));
            list(g, cells.into_iter().map(|c| XVal::Ref(c.0)).collect())
        }
        // `depth`: the map marker references in those cells [documented].
        "getnearmapmarkers" => {
            let mut found = Vec::new();
            for c in near_cells(g, num(args, 0, 0.0)) {
                for r in g.refs_in_cell(c, false) {
                    if g.world.placed.get(r).is_some_and(|p| p.map_marker.is_some()) {
                        found.push(XVal::Ref(r.0));
                    }
                }
            }
            list(g, found)
        }
        // The calling script, marked until the next load; the VM hands it over as the
        // argument [documented: MarkScriptOnLoad, "Marks calling script"]. 1, from a query
        // with no script form too [observed: sup4 markonload].
        "markscriptonload" => {
            if let Some(s) = form(args, 0) {
                g.sup4.marked.insert(s);
            }
            XVal::Num(1.0)
        }
        "isscriptmarkedonload" => XVal::Num(f64::from(u8::from(form(args, 0).is_some_and(|s| g.sup4.marked.contains(&s))))),
        // Kills the actors fighting the player, not the player's teammates [documented:
        // KillAll2, "kills all current player combat enemies leaving companions and
        // friendly NPCs intact"; deprecated].
        "killall2" | "killallalt" => {
            let all: Vec<FormId> = g.package_actors().into_iter().collect();
            for a in all {
                if a != FormId::PLAYER_REF && !g.is_dead(a) && g.present(a) && !g.is_teammate(a) && g.combat_target_of(a) == Some(FormId::PLAYER_REF) {
                    if let Err(e) = crate::functions::console::call_words(g, Some(a), "KillActor", &[], "KillAll2") {
                        return Some(Err(e));
                    }
                }
            }
            zero
        }
        // `[caught]` on an NPC: the flag a failed pickpocket sets (`crate::pickpocket`);
        // with an argument it is set; the flag as it is now either way [documented].
        "setcaughtpcpickpocketting" => {
            let Some(r) = subject.filter(|&r| r != FormId::PLAYER_REF) else { return Some(Ok(zero)) };
            if !args.is_empty() {
                g.extra.entry(r).or_default().caught_pickpocket = num(args, 0, 0.0) != 0.0;
            }
            XVal::Num(f64::from(u8::from(g.extra.get(&r).is_some_and(|e| e.caught_pickpocket))))
        }
        // The text with its Cyrillic letters in Latin ones (`translit`) [documented:
        // TranslitText; hypothesis: it returns the new text].
        "translittext" => XVal::Str(match args.first() {
            Some(XVal::Str(s)) => translit(s),
            _ => String::new(),
        }),
        _ => return None,
    };
    Some(Ok(v))
}

#[cfg(test)]
mod tests {
    use super::translit;

    #[test]
    fn cyrillic_to_latin() {
        assert_eq!(translit("Привет, мир!"), "Privet, mir!");
        assert_eq!(translit("Щука Жук Юля"), "Shchuka Zhuk Yulya");
        assert_eq!(translit("NV 2281"), "NV 2281");
    }
}
