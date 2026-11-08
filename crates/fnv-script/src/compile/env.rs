//! [`Env`] over a load order: Editor IDs, record types, the script a form runs and that
//! script's variables, as one plugin sees them (the plugin and its masters, last
//! definition wins), the way the editor saw them when it compiled that plugin's scripts
//! [hypothesis: the editor resolves names against the active file and its masters].

use super::{Env, FormInfo};
use crate::unit::{ScriptUnit, UnitOwner, VarDecl};
use fnv_esm::LoadOrder;
use rayon::prelude::*;
use std::collections::HashMap;

/// One definition of a form.
#[derive(Debug, Clone)]
struct Def {
    plugin: usize,
    sig: [u8; 4],
    edid: Option<String>,
    /// `SCRI` (global).
    script: Option<u32>,
    /// `NAME` of a placed reference (global).
    base: Option<u32>,
}

pub struct LoadOrderIndex<'a> {
    lo: &'a LoadOrder<'a>,
    visible: Vec<Vec<bool>>,
    defs: HashMap<u32, Vec<Def>>,
    /// Lower-case Editor ID → FormIDs that carry it, in load order.
    edids: HashMap<String, Vec<u32>>,
    /// Script FormID → (plugin, variables), in load order.
    scripts: HashMap<u32, Vec<(usize, Vec<VarDecl>)>>,
}

fn formid(data: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(..4)?.try_into().ok()?))
}

impl<'a> LoadOrderIndex<'a> {
    pub fn build(lo: &'a LoadOrder<'a>, units: &[ScriptUnit]) -> LoadOrderIndex<'a> {
        let n = lo.plugins.len();
        let visible = (0..n)
            .map(|i| {
                let mut v = vec![false; n];
                v[i] = true;
                for &m in lo.master_map(i) {
                    v[m as usize] = true;
                }
                v
            })
            .collect();
        let read: Vec<(u32, Def)> = (0..n)
            .into_par_iter()
            .flat_map_iter(|pi| {
                lo.records[pi]
                    .par_iter()
                    .filter_map(|r| {
                        let form = lo.to_global(pi, r.header.form_id)?;
                        let mut d = Def { plugin: pi, sig: r.header.sig, edid: None, script: None, base: None };
                        r.visit_subrecords(|sig, data| match sig {
                            b"EDID" if d.edid.is_none() => d.edid = Some(fnv_util::text::decode_z(data)),
                            b"SCRI" if d.script.is_none() => d.script = formid(data).and_then(|l| lo.to_global(pi, l)),
                            b"NAME" if d.base.is_none() && matches!(&r.header.sig, b"REFR" | b"ACHR" | b"ACRE") => {
                                d.base = formid(data).and_then(|l| lo.to_global(pi, l))
                            }
                            _ => {}
                        })
                        .ok()?;
                        Some((form, d))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut defs: HashMap<u32, Vec<Def>> = HashMap::new();
        let mut edids: HashMap<String, Vec<u32>> = HashMap::new();
        for (form, d) in read {
            if let Some(e) = &d.edid {
                let v = edids.entry(e.to_ascii_lowercase()).or_default();
                if !v.contains(&form) {
                    v.push(form);
                }
            }
            defs.entry(form).or_default().push(d);
        }
        for v in defs.values_mut() {
            v.sort_by_key(|d| d.plugin);
        }
        let mut scripts: HashMap<u32, Vec<(usize, Vec<VarDecl>)>> = HashMap::new();
        for u in units {
            if let (UnitOwner::Script { form }, Some(pi)) = (&u.owner, lo.plugins.iter().position(|p| p.name == u.plugin)) {
                scripts.entry(*form).or_default().push((pi, u.variables.clone()));
            }
        }
        LoadOrderIndex { lo, visible, defs, edids, scripts }
    }

    /// The names as plugin `plugin` (an index into the load order) sees them.
    pub fn env(&self, plugin: usize) -> PluginEnv<'_> {
        PluginEnv { ix: self, plugin }
    }

    pub fn plugin_index(&self, name: &str) -> Option<usize> {
        self.lo.plugins.iter().position(|p| p.name == name)
    }

    fn def(&self, form: u32, viewer: usize) -> Option<&Def> {
        self.defs.get(&form)?.iter().rev().find(|d| self.visible[viewer][d.plugin])
    }
}

pub struct PluginEnv<'a> {
    ix: &'a LoadOrderIndex<'a>,
    plugin: usize,
}

impl Env for PluginEnv<'_> {
    fn form(&self, editor_id: &str) -> Option<FormInfo> {
        let cands = self.ix.edids.get(&editor_id.to_ascii_lowercase())?;
        // The last visible definition whose Editor ID (as the viewer sees the form) matches.
        let mut best: Option<(usize, FormInfo)> = None;
        for &f in cands {
            if let Some(d) = self.ix.def(f, self.plugin) {
                if d.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(editor_id)) && best.as_ref().is_none_or(|b| d.plugin >= b.0) {
                    best = Some((d.plugin, FormInfo { form: f, sig: d.sig }));
                }
            }
        }
        best.map(|b| b.1)
    }

    fn info(&self, form: u32) -> Option<FormInfo> {
        self.ix.def(form, self.plugin).map(|d| FormInfo { form, sig: d.sig })
    }

    fn editor_id(&self, form: u32) -> Option<String> {
        self.ix.def(form, self.plugin)?.edid.clone()
    }

    fn local_form(&self, local: u32) -> Option<u32> {
        let g = self.ix.lo.to_global(self.plugin, local)?;
        self.ix.def(g, self.plugin).map(|_| g)
    }

    fn script_vars(&self, form: u32) -> Option<Vec<VarDecl>> {
        let d = self.ix.def(form, self.plugin)?;
        let script = d.script.or_else(|| self.ix.def(d.base?, self.plugin)?.script)?;
        let vis = &self.ix.visible[self.plugin];
        // The version the viewer sees; when only a later plugin's is at hand (a game reads
        // the winning records only), that one.
        let versions = self.ix.scripts.get(&script)?;
        versions.iter().rev().find(|(p, _)| vis[*p]).or_else(|| versions.last()).map(|(_, v)| v.clone())
    }
}
