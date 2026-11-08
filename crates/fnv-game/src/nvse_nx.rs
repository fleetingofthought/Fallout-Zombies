//! NX NVSE ("NVSE Extender", #1017): NX variables (numbers, forms and strings kept on a
//! reference under a string key), NX_IsInList, NX_GetRandom, NX_GetConversationPartner
//! and the skeleton functions [documented: Category:Functions (NX) and each page;
//! docs/spec-notes/ext-nx.md].

use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use rand_core::RngCore;
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One kind of NX variable on one reference: lower-cased key -> the key as given and the
/// value.
pub type NxMap<T> = BTreeMap<String, NxVar<T>>;

/// One NX variable. Saved as named fields, not a pair: a save loaded without a form's
/// plugin moves a field's FormID to the gone slot (`GameSnapshot::drop_gone` then drops
/// the variable), where a pair would lose the element and fail to read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NxVar<T> {
    pub key: String,
    pub value: T,
}

/// A reference's NX variables. "Keys for each type must be unique, although alternative
/// types may share the same key"; "Keys are case insensitive" [documented: Category:
/// Functions (NX)].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NxRef {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fl: NxMap<f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fo: NxMap<FormId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub st: NxMap<String>,
}

impl NxRef {
    fn is_empty(&self) -> bool {
        self.fl.is_empty() && self.fo.is_empty() && self.st.is_empty()
    }
}

/// Every reference's NX variables, saved: they "are persistent across game sessions"
/// [documented: Category:Functions (NX)].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NxVars(pub BTreeMap<FormId, NxRef>);

impl NxVars {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// After a remap: drop the form variables naming a gone plugin's form, and references
    /// left with none. Returns how many were dropped.
    pub(crate) fn drop_gone(&mut self, gone: impl Fn(FormId) -> bool) -> usize {
        let mut n = 0;
        for r in self.0.values_mut() {
            let before = r.fo.len();
            r.fo.retain(|_, v| !gone(v.value));
            n += before - r.fo.len();
        }
        self.0.retain(|_, r| !r.is_empty());
        n
    }
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn num(args: &[XVal], k: usize) -> f64 {
    match args.get(k) {
        Some(XVal::Num(n)) => *n,
        _ => 0.0,
    }
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        _ => None,
    }
}

/// Whether `key` (lower case) begins with `prefix` without regard to case.
fn starts(key: &str, prefix: &str) -> bool {
    key.starts_with(&prefix.to_lowercase())
}

/// Clear a key, or with `match` 2 every key beginning with it [documented: NX_ClrEVFl;
/// hypothesis: another `match` value clears the key alone].
fn clear<T>(m: &mut NxMap<T>, key: &str, prefix: bool) {
    if prefix {
        m.retain(|k, _| !starts(k, key));
    } else {
        m.remove(&key.to_lowercase());
    }
}

/// The variables of one kind whose key begins with `prefix`, as a string map keyed as
/// the variables were set ("Keys in the returned array match the NX ones"; copies)
/// [documented: NX_GetEVFlAr; hypothesis: the key's case as it was last set].
fn map_of<T: Clone>(g: &mut Game, m: Option<&NxMap<T>>, prefix: &str, val: impl Fn(T) -> XVal) -> XVal {
    let entries = m.map_or_else(Vec::new, |m| m.iter().filter(|(k, _)| starts(k, prefix)).map(|(_, v)| (XKey::Str(v.key.clone()), val(v.value.clone()))).collect());
    XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
}

/// The directories of a skeleton model path, lower case: `Characters\_Male\Skeleton.NIF`
/// gives `characters`, `_male`.
fn skeleton_dirs(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    parts[..parts.len().saturating_sub(1)].iter().map(|p| p.to_lowercase()).filter(|p| p != "meshes").collect()
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let on = subject.unwrap_or(FormId(0));
    let key = text(args, 0);
    let low = key.to_lowercase();
    let v = match name {
        // The setters return nothing [documented: "none"]; on no reference they do nothing.
        "nx_setevfl" | "nx_setevfo" | "nx_setevst" => {
            if subject.is_some() && !key.is_empty() {
                let r = g.nx.0.entry(on).or_default();
                match name {
                    "nx_setevfl" => {
                        r.fl.insert(low, NxVar { key, value: num(args, 1) });
                    }
                    "nx_setevfo" => {
                        r.fo.insert(low, NxVar { key, value: form(args, 1).unwrap_or(FormId(0)) });
                    }
                    _ => {
                        r.st.insert(low, NxVar { key, value: text(args, 1) });
                    }
                }
            }
            zero
        }
        // A key not set reads 0, an empty string for a string [documented: NX_GetEVFl,
        // NX_GetEVFo "Returns 0"; NX_GetEVSt "Returns NULL"].
        "nx_getevfl" => XVal::Num(g.nx.0.get(&on).and_then(|r| r.fl.get(&low)).map_or(0.0, |v| v.value)),
        "nx_getevfo" => XVal::Ref(g.nx.0.get(&on).and_then(|r| r.fo.get(&low)).map_or(0, |v| v.value.0)),
        "nx_getevst" => XVal::Str(g.nx.0.get(&on).and_then(|r| r.st.get(&low)).map(|v| v.value.clone()).unwrap_or_default()),
        "nx_clrevfl" | "nx_clrevfo" | "nx_clrevst" => {
            let prefix = num(args, 1) == 2.0;
            if let Some(r) = g.nx.0.get_mut(&on) {
                match name {
                    "nx_clrevfl" => clear(&mut r.fl, &key, prefix),
                    "nx_clrevfo" => clear(&mut r.fo, &key, prefix),
                    _ => clear(&mut r.st, &key, prefix),
                }
                if r.is_empty() {
                    g.nx.0.remove(&on);
                }
            }
            zero
        }
        "nx_getevflar" => {
            let m = g.nx.0.get(&on).map(|r| r.fl.clone());
            map_of(g, m.as_ref(), &key, XVal::Num)
        }
        "nx_getevfoar" => {
            let m = g.nx.0.get(&on).map(|r| r.fo.clone());
            map_of(g, m.as_ref(), &key, |f| XVal::Ref(f.0))
        }
        "nx_getevstar" => {
            let m = g.nx.0.get(&on).map(|r| r.st.clone());
            map_of(g, m.as_ref(), &key, XVal::Str)
        }
        // `min max`, both included [documented: NX_GetRandom], from the game's seeded
        // generator; a min above the max gives the min [hypothesis].
        "nx_getrandom" => {
            let (lo, hi) = (num(args, 0).trunc() as i64, num(args, 1).trunc() as i64);
            if hi <= lo {
                XVal::Num(lo as f64)
            } else {
                let span = (hi - lo + 1) as u64;
                XVal::Num((lo + (u64::from(g.rng.next_u32()) % span) as i64) as f64)
            }
        }
        // `list item`, by priority [documented: NX_IsInList]: 1 the item itself is in the
        // list, 2 its base is, 3 it is the base of a reference in the list, 4 its base is
        // the base of a reference in the list; 0 otherwise.
        "nx_isinlist" => {
            let (Some(l), Some(item)) = (form(args, 0), form(args, 1)) else { return Some(Ok(zero)) };
            let members = crate::functions::list_members(g, l);
            let base = crate::functions::base_of(g, item).filter(|b| *b != item);
            let member_bases: Vec<FormId> = members.iter().filter_map(|m| crate::functions::base_of(g, *m).filter(|b| b != m)).collect();
            let r = if members.contains(&item) {
                1
            } else if base.is_some_and(|b| members.contains(&b)) {
                2
            } else if member_bases.contains(&item) {
                3
            } else if base.is_some_and(|b| member_bases.contains(&b)) {
                4
            } else {
                0
            };
            XVal::Num(f64::from(r))
        }
        // The actor in conversation with the player, whoever began it [documented:
        // NX_GetConversationPartner]; 0 outside one.
        "nx_getconversationpartner" => XVal::Ref(g.dialogue.as_ref().map_or(0, |d| d.actor.0)),
        // The directory that names the actor's skeleton: its base's skeleton model's
        // folder [documented: NX_GetSkeletonPath, "_male" for a human, "NVGecko" for a
        // gecko; the model as GetSkeletonModel reads it, MODL].
        "nx_getskeletonpath" => {
            let path = skeleton_of(g, subject);
            let parts: Vec<&str> = path.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
            XVal::Str(parts.len().checked_sub(2).and_then(|i| parts.get(i)).map(|s| s.to_string()).unwrap_or_default())
        }
        // `name [actor]`: the skeleton is in a folder of that name under meshes, without
        // regard to case [documented: NX_IsUsingSkeleton; hypothesis: any folder of the
        // path, "characters" included].
        "nx_isusingskeleton" => {
            let who = form(args, 1).or(subject);
            let dirs = skeleton_dirs(&skeleton_of(g, who));
            XVal::Num(f64::from(u8::from(!key.is_empty() && dirs.contains(&low))))
        }
        _ => return None,
    };
    Some(Ok(v))
}

/// The skeleton model (MODL) of an actor, or of an actor base, "" when it has none. A
/// leveled base reads the actor drawn for the reference, and a template with Use
/// Model/Animation the template's [documented: actors.md §2.2, the Model/Animation group;
/// hypothesis: the skeleton is in that group].
pub fn skeleton_model(g: &Game, f: FormId) -> String {
    let base = crate::functions::base_of(g, f).unwrap_or(f);
    let b = g.template_source(base, f, |d| d.template_model).map_or(base, |(s, _)| s);
    // SetCRESkeletonModel's and SetNPCSkeletonModel's change first (`crate::nvse_jip8`).
    match crate::nvse::record_value(g, b, "MODL", "") {
        Some(XVal::Str(s)) => s,
        _ => String::new(),
    }
}

/// Whether the actor of a reference is a creature, by its own record: its base, or the
/// pick of a leveled base. Templates are not followed: a person on a creature's
/// Model/Animation stays a person, and a creature on a person's stays a creature; the
/// skeleton they have is [`skeleton_model`]'s. `None`: it comes to no actor record.
pub fn is_creature(g: &Game, f: FormId) -> Option<bool> {
    let base = crate::functions::base_of(g, f).unwrap_or(f);
    g.template_source(base, f, |_| false).map(|(_, d)| &d.sig == b"CREA")
}

fn skeleton_of(g: &Game, actor: Option<FormId>) -> String {
    actor.map(|a| skeleton_model(g, a)).unwrap_or_default()
}
