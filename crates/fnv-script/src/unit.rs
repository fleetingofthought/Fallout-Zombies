//! Script units: one compiled script with its header, variables and references, read
//! from any record that embeds one (docs/spec-notes/scripts.md §1).
//!
//! Values are read through the Layer 1 schema (`schemas/fnv`, `common.json`: SCHR, SLSD,
//! SCVR, SCRO, SCRV, SCTX; `gameplay.json`: QUST INDX); only the compiled bytes (`SCDA`,
//! an opaque `bytes` field in the schema) are taken from the subrecord directly.

use fnv_esm::decode::{decode_sub, DecodeCtx};
use fnv_esm::{LoadOrder, Record, Schema};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The script type in `SCHR` [documented: xEdit].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptKind {
    Object,
    Quest,
    Effect,
    Other(u16),
}

/// Local variable type in `SLSD` [documented: xEdit].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VarType {
    Int,
    FloatOrRef,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VarDecl {
    pub index: u32,
    pub name: String,
    pub ty: VarType,
}

/// One entry of the reference list, in order (index 1 is the first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "slot", content = "value", rename_all = "snake_case")]
pub enum RefSlot {
    /// `SCRO`: a form, as a load-order (global) FormID.
    Form(u32),
    /// `SCRV`: a local variable (holding a reference), by variable index.
    Local(u32),
    /// `SCRO` whose FormID cannot be mapped into the load order (file-local value).
    Unmapped(u32),
}

/// Where a unit lives. FormIDs are load-order (global) FormIDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "owner", rename_all = "snake_case")]
pub enum UnitOwner {
    /// A standalone `SCPT` record.
    Script { form: u32 },
    /// A `QUST` stage entry: `INDX` stage value, then the entry's position within that
    /// stage (0-based, counting the units after that `INDX`).
    QuestStage { form: u32, stage: i64, entry: u32 },
    /// An `INFO` result script: the one before `NEXT` runs at the start of the line, the
    /// one after it at the end [documented: xEdit "Script (Begin)"/"Script (End)"].
    Info { form: u32, end: bool },
    /// A `PACK` event script after `POBA` (begin), `POEA` (end) or `POCA` (change).
    Package { form: u32, event: PackageEvent },
    /// A `TERM` menu item's script; `item` counts the `ITXT` items (0-based).
    Terminal { form: u32, item: u32 },
    /// A `PERK` entry's script; `entry` counts the `PRKE` entries (0-based).
    Perk { form: u32, entry: u32 },
    /// A placed reference's patrol script (after `XPPA`).
    Patrol { form: u32 },
    /// Any other record that embeds a unit; `ordinal` counts its units (0-based).
    Other { sig: String, form: u32, ordinal: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageEvent {
    Begin,
    End,
    Change,
}

impl UnitOwner {
    pub fn form(&self) -> u32 {
        match *self {
            UnitOwner::Script { form }
            | UnitOwner::QuestStage { form, .. }
            | UnitOwner::Info { form, .. }
            | UnitOwner::Package { form, .. }
            | UnitOwner::Terminal { form, .. }
            | UnitOwner::Perk { form, .. }
            | UnitOwner::Patrol { form }
            | UnitOwner::Other { form, .. } => form,
        }
    }

    /// Record signature of the owner.
    pub fn sig(&self) -> &str {
        match self {
            UnitOwner::Script { .. } => "SCPT",
            UnitOwner::QuestStage { .. } => "QUST",
            UnitOwner::Info { .. } => "INFO",
            UnitOwner::Package { .. } => "PACK",
            UnitOwner::Terminal { .. } => "TERM",
            UnitOwner::Perk { .. } => "PERK",
            UnitOwner::Patrol { .. } => "REFR",
            UnitOwner::Other { sig, .. } => sig,
        }
    }

    /// Stable text form, e.g. `QUST 0x00104C1C stage 10 entry 0`
    /// (docs/spec-notes/scripts.md §1.3).
    pub fn label(&self) -> String {
        let f = format!("{} 0x{:08X}", self.sig(), self.form());
        match self {
            UnitOwner::Script { .. } => f,
            UnitOwner::QuestStage { stage, entry, .. } => format!("{f} stage {stage} entry {entry}"),
            UnitOwner::Info { end, .. } => format!("{f} {}", if *end { "end" } else { "begin" }),
            UnitOwner::Package { event, .. } => format!(
                "{f} {}",
                match event {
                    PackageEvent::Begin => "begin",
                    PackageEvent::End => "end",
                    PackageEvent::Change => "change",
                }
            ),
            UnitOwner::Terminal { item, .. } => format!("{f} item {item}"),
            UnitOwner::Perk { entry, .. } => format!("{f} entry {entry}"),
            UnitOwner::Patrol { .. } => format!("{f} patrol"),
            UnitOwner::Other { ordinal, .. } => format!("{f} unit {ordinal}"),
        }
    }
}

/// The `SCHR` counters, kept for checks (docs/spec-notes/scripts.md §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Header {
    pub ref_count: u32,
    pub compiled_size: u32,
    pub variable_count: u32,
    pub flags: u32,
}

/// One compiled script with everything it needs to be decoded and run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScriptUnit {
    /// Plugin file the unit was read from.
    pub plugin: String,
    pub owner: UnitOwner,
    pub kind: ScriptKind,
    pub header: Header,
    /// Local variables in record order. Address them by `index`, never by position.
    pub variables: Vec<VarDecl>,
    /// Reference list; bytecode reference indices are 1-based into this.
    pub refs: Vec<RefSlot>,
    /// Whether the unit has an `SCDA` subrecord.
    pub has_scda: bool,
    /// Compiled bytes (empty without `SCDA`).
    #[serde(skip)]
    pub bytes: Vec<u8>,
    /// Source text (`SCTX`), if present.
    #[serde(skip)]
    pub source: Option<String>,
}

impl ScriptUnit {
    /// `<plugin>|<owner label>`: unique across a load order (docs/spec-notes/scripts.md
    /// §1.3).
    pub fn key(&self) -> String {
        format!("{}|{}", self.plugin, self.owner.label())
    }

    /// The reference list entry for a 1-based bytecode index.
    pub fn slot(&self, r: crate::ir::RefIndex) -> Option<RefSlot> {
        (r.0 as usize).checked_sub(1).and_then(|i| self.refs.get(i)).copied()
    }

    /// The first variable declared with this index.
    pub fn var(&self, index: u32) -> Option<&VarDecl> {
        self.variables.iter().find(|v| v.index == index)
    }
}

/// Anomalies met while reading units, counted by kind with a few samples each.
#[derive(Debug, Default, Clone, Serialize)]
pub struct ReadIssues {
    pub counts: BTreeMap<String, u64>,
    pub samples: BTreeMap<String, Vec<String>>,
}

impl ReadIssues {
    pub fn note(&mut self, kind: &str, sample: impl FnOnce() -> String) {
        *self.counts.entry(kind.to_string()).or_default() += 1;
        let s = self.samples.entry(kind.to_string()).or_default();
        if s.len() < 5 {
            s.push(sample());
        }
    }
    pub fn merge(&mut self, o: ReadIssues) {
        for (k, n) in o.counts {
            *self.counts.entry(k).or_default() += n;
        }
        for (k, v) in o.samples {
            let s = self.samples.entry(k).or_default();
            let room = 5usize.saturating_sub(s.len());
            s.extend(v.into_iter().take(room));
        }
    }
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }
}

const UNIT_SUBS: [&[u8; 4]; 7] = [b"SCHR", b"SCDA", b"SCTX", b"SLSD", b"SCVR", b"SCRO", b"SCRV"];

/// Where the next unit of a record belongs, tracked over its subrecords.
#[derive(Default)]
struct OwnerState {
    stage: Option<i64>,
    stage_units: u32,
    next_seen: bool,
    pack_event: Option<PackageEvent>,
    items: u32,
    entries: u32,
    patrol: bool,
    ordinal: u32,
}

/// Read every unit embedded in one record. `to_global` maps a FormID local to the
/// record's plugin into the load order.
pub fn read_record(
    r: &Record,
    plugin: &str,
    to_global: &dyn Fn(u32) -> Option<u32>,
    schema: &Schema,
    issues: &mut ReadIssues,
) -> Vec<ScriptUnit> {
    let sig = r.header.sig;
    let subs = match r.subrecords() {
        Ok(s) => s,
        Err(e) => {
            issues.note("record_unreadable", || format!("{} 0x{:08X}: {e}", fnv_util::tag_str(&sig), r.header.form_id));
            return Vec::new();
        }
    };
    if !subs.iter().any(|s| &s.sig == b"SCHR") {
        return Vec::new();
    }
    let form = to_global(r.header.form_id).unwrap_or(r.header.form_id);
    let where_ = || format!("{plugin} {} 0x{:08X}", fnv_util::tag_str(&sig), form);
    let mut ctx = DecodeCtx { schema, on_formid: None };
    let mut val = |s: &fnv_esm::Subrecord, issues: &mut ReadIssues| -> Option<Value> {
        let d = decode_sub(&sig, s, &mut ctx);
        if d.value.is_none() || d.error.is_some() {
            issues.note(&format!("schema_{}_{}", fnv_util::tag_str(&s.sig), d.status.as_str()), || {
                format!("{}: {}", where_(), d.error.clone().unwrap_or_default())
            });
        }
        d.value
    };
    let mut st = OwnerState::default();
    let mut out = Vec::new();
    let mut i = 0;
    while i < subs.len() {
        let s = &subs[i];
        match &s.sig {
            b"INDX" if &sig == b"QUST" => {
                st.stage = val(s, issues).and_then(|v| v.as_i64());
                st.stage_units = 0;
            }
            b"NEXT" if &sig == b"INFO" => st.next_seen = true,
            b"POBA" => st.pack_event = Some(PackageEvent::Begin),
            b"POEA" => st.pack_event = Some(PackageEvent::End),
            b"POCA" => st.pack_event = Some(PackageEvent::Change),
            b"ITXT" if &sig == b"TERM" => st.items += 1,
            b"PRKE" if &sig == b"PERK" => st.entries += 1,
            b"XPPA" => st.patrol = true,
            _ => {}
        }
        if &s.sig != b"SCHR" {
            i += 1;
            continue;
        }
        // A unit: SCHR and the script subrecords that follow it.
        let owner = match &sig {
            b"SCPT" => UnitOwner::Script { form },
            b"QUST" if st.stage.is_some() => {
                let o = UnitOwner::QuestStage { form, stage: st.stage.unwrap_or_default(), entry: st.stage_units };
                st.stage_units += 1;
                o
            }
            b"INFO" => UnitOwner::Info { form, end: st.next_seen },
            b"PACK" if st.pack_event.is_some() => {
                UnitOwner::Package { form, event: st.pack_event.unwrap_or(PackageEvent::Begin) }
            }
            b"TERM" if st.items > 0 => UnitOwner::Terminal { form, item: st.items - 1 },
            b"PERK" if st.entries > 0 => UnitOwner::Perk { form, entry: st.entries - 1 },
            b"REFR" if st.patrol => UnitOwner::Patrol { form },
            _ => UnitOwner::Other { sig: fnv_util::tag_str(&sig), form, ordinal: st.ordinal },
        };
        st.ordinal += 1;
        let mut u = ScriptUnit {
            plugin: plugin.to_string(),
            owner,
            kind: ScriptKind::Object,
            header: Header::default(),
            variables: Vec::new(),
            refs: Vec::new(),
            has_scda: false,
            bytes: Vec::new(),
            source: None,
        };
        if let Some(h) = val(s, issues) {
            let n = |k: &str| h.get(k).and_then(Value::as_u64).unwrap_or(0) as u32;
            u.header = Header {
                ref_count: n("ref_count"),
                compiled_size: n("compiled_size"),
                variable_count: n("variable_count"),
                flags: h.get("flags").and_then(|f| f.get("value")).and_then(Value::as_u64).unwrap_or(0) as u32,
            };
            u.kind = match h.get("type") {
                Some(Value::String(t)) if t == "Object" => ScriptKind::Object,
                Some(Value::String(t)) if t == "Quest" => ScriptKind::Quest,
                Some(Value::String(t)) if t == "Effect" => ScriptKind::Effect,
                Some(v) => ScriptKind::Other(v.as_u64().unwrap_or(0xFFFF) as u16),
                None => ScriptKind::Other(0xFFFF),
            };
        }
        i += 1;
        while i < subs.len() && UNIT_SUBS.contains(&&subs[i].sig) && &subs[i].sig != b"SCHR" {
            let s = &subs[i];
            match &s.sig {
                b"SCDA" => {
                    u.has_scda = true;
                    u.bytes = s.data.clone();
                }
                b"SCTX" => u.source = val(s, issues).and_then(|v| v.as_str().map(str::to_string)),
                b"SLSD" => {
                    if let Some(v) = val(s, issues) {
                        let ty = match v.get("type") {
                            Some(Value::String(t)) if t == "Integer" => VarType::Int,
                            Some(Value::String(t)) if t == "FloatOrRef" => VarType::FloatOrRef,
                            Some(t) => VarType::Other(t.to_string()),
                            None => VarType::Other(String::new()),
                        };
                        let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
                        u.variables.push(VarDecl { index, name: String::new(), ty });
                    }
                }
                b"SCVR" => {
                    let name = val(s, issues).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                    match u.variables.last_mut() {
                        Some(v) if v.name.is_empty() => v.name = name,
                        _ => issues.note("scvr_without_slsd", || format!("{}: {name}", where_())),
                    }
                }
                b"SCRO" => {
                    if let Some(local) = val(s, issues).as_ref().and_then(Value::as_str).and_then(|t| {
                        u32::from_str_radix(t.trim_start_matches("0x"), 16).ok()
                    }) {
                        u.refs.push(match to_global(local) {
                            Some(g) => RefSlot::Form(g),
                            None => {
                                issues.note("scro_unmapped", || format!("{}: 0x{local:08X}", where_()));
                                RefSlot::Unmapped(local)
                            }
                        });
                    }
                }
                b"SCRV" => {
                    if let Some(v) = val(s, issues).and_then(|v| v.as_u64()) {
                        u.refs.push(RefSlot::Local(v as u32));
                    }
                }
                _ => {}
            }
            i += 1;
        }
        out.push(u);
    }
    out
}

/// Every unit of every plugin in the load order, in load order and file order.
pub fn read_load_order(lo: &LoadOrder<'_>, schema: &Schema) -> (Vec<ScriptUnit>, ReadIssues) {
    use rayon::prelude::*;
    let mut all = Vec::new();
    let mut issues = ReadIssues::default();
    for (pi, p) in lo.plugins.iter().enumerate() {
        let to_global = |l: u32| lo.to_global(pi, l);
        let parts: Vec<(Vec<ScriptUnit>, ReadIssues)> = lo.records[pi]
            .par_iter()
            .map(|r| {
                let mut iss = ReadIssues::default();
                let u = read_record(r, &p.name, &to_global, schema, &mut iss);
                (u, iss)
            })
            .collect();
        for (u, iss) in parts {
            all.extend(u);
            issues.merge(iss);
        }
    }
    (all, issues)
}
