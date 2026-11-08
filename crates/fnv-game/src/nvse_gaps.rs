//! The most called extender functions the mod lists use that no earlier set built
//! (docs/handoff/modlist-function-counts.json) [documented: their pages; observed: the
//! peer's gaps2, gaps3 and gaps4 sets in the original game]. Record reads come from the
//! schema's decoded fields (`GameData::record_fields`, enums as their numbers).

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};

/// A placed radio station, as the engine reads it.
#[derive(Debug, Clone)]
pub struct RadioSite {
    pub station: FormId,
    pub initially_disabled: bool,
    /// On the Pip-Boy's list (the station has no Non-Pipboy flag).
    pub on_pipboy: bool,
    pub broadcast: fnv_world::radio::Broadcast,
}

/// This file's session state.
#[derive(Debug, Default)]
pub struct GapsState {
    /// SetCombatDisabled's actors.
    pub combat_disabled: HashSet<FormId>,
    /// The Pip-Boy light, and when a script turned it off: the light reads off from the
    /// next frame on [observed: gaps2, `TogglePipBoyLight 0` still reads 1 in its frame
    /// and 0 in the next; on reads 1 at once].
    pub pipboy_light: bool,
    pub light_off_at: Option<f64>,
    /// SetRefName's names, by reference; GetName does not read them [observed: gaps4].
    pub ref_names: HashMap<FormId, String>,
    /// SetActorGravityMult's multipliers.
    pub gravity: HashMap<FormId, f64>,
    /// ToggleBipedSlotVisibility's hidden slots.
    pub hidden_slots: HashSet<u32>,
    /// SetPackageFlag's general flags, by package.
    pub package_flags: HashMap<FormId, u32>,
    /// SetPerkLevel's levels.
    pub perk_levels: HashMap<FormId, u8>,
    /// SetPerkFlag's flags: (perk, flag ID 0 trait, 1 playable, 2 hidden).
    pub perk_flags: HashMap<(FormId, u8), bool>,
    /// SetContainerRespawns's flags, by container base.
    pub respawns: HashMap<FormId, bool>,
    /// SetDestructionDataHealth's values.
    pub destruction_health: HashMap<FormId, i32>,
    /// References whose activation SetInteractionDisabled turned off.
    pub interaction_disabled: HashSet<FormId>,
    /// SetNthEffectBase's bases: (item, effect index).
    pub effect_bases: HashMap<(FormId, usize), FormId>,
    /// Actors whose special idle scripts stopped (StopIdle, ForceStopIdle), for the
    /// engine to take.
    pub idle_stops: Vec<FormId>,
    /// The placed radio stations, from the engine; `None` until the engine sets them.
    pub radio_sites: Option<Vec<RadioSite>>,
    /// The armed mines of the loaded cells that someone other than the player put down,
    /// from the engine's mines at their last frame.
    pub armed_mines: HashSet<FormId>,
    /// Seconds each actor's body has been off the ground, from the engine's physics at
    /// the last step (absent on the ground).
    pub air_times: HashMap<FormId, f32>,
    /// The third JIP batch's state (`crate::nvse_jip3`).
    pub jip3: crate::nvse_jip3::Jip3State,
    /// The fourth JIP batch's state (`crate::nvse_jip4`).
    pub jip4: crate::nvse_jip4::Jip4State,
    /// The shots in flight, for the projectile functions (`crate::nvse_proj`).
    pub projs: crate::nvse_proj::ProjState,
    /// The sounds scripts played, for the sound functions (`crate::nvse_sound`).
    pub sounds: crate::nvse_sound::SoundState,
    /// PushObject's pushes, for the engine to take: (the reference, the push's origin,
    /// its force).
    pub pushes: Vec<(FormId, [f32; 3], f32)>,
    /// ToggleFirstPerson's and TogglePlayerSneaking's asks, for the engine to take.
    pub view_request: Option<bool>,
    pub sneak_request: Option<bool>,
    /// JohnnyGuitar's actor value change handlers: (the function, the code it waits for).
    pub av_handlers: Vec<(XVal, Option<u16>)>,
    /// SetMapMarkerName's names, by marker reference.
    pub marker_names: HashMap<FormId, String>,
    /// JohnnyGuitar's quest handlers: (on start, the function, the quest it waits for).
    pub quest_handlers: Vec<(bool, XVal, Option<FormId>)>,
    /// JohnnyGuitar's perk handlers: (on add, the function, the perk it waits for).
    pub perk_handlers: Vec<(bool, XVal, Option<FormId>)>,
    /// The angles GetAngle still reads after a SetAngle this frame: (reference, axis) ->
    /// degrees.
    pub angles_shown: HashMap<(FormId, usize), f64>,
    /// SetMapMarkerRep's reputations, by marker reference (`None`: cleared).
    pub marker_reputations: HashMap<FormId, Option<FormId>>,
    /// SetRecipeCategory's and SetRecipeSubcategory's forms: (recipe, sub).
    pub recipe_categories: HashMap<(FormId, bool), FormId>,
    /// The music type a script last asked for (PlayMusic), and whether the engine plays
    /// any music.
    pub music_type: Option<FormId>,
    pub music_any: bool,
    /// The player's always-run switch as the engine tells it, and SetAlwaysRun's ask.
    pub always_run: bool,
    pub always_run_request: Option<bool>,
    /// RunScriptSnippet's snippets waiting: (frames left, source, reference).
    pub snippets: Vec<(u32, String, FormId)>,
}

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

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

fn json_form(v: Option<&Json>) -> XVal {
    match v.and_then(Json::as_str) {
        Some(s) if s.starts_with("0x") => XVal::Ref(u32::from_str_radix(&s[2..], 16).unwrap_or(0)),
        _ => XVal::Ref(0),
    }
}

fn json_num(v: Option<&Json>) -> f64 {
    v.and_then(|v| v.get("value").filter(|_| v.get("set").is_some()).unwrap_or(v).as_f64()).unwrap_or(0.0)
}

/// A list array of the values.
fn list(g: &mut Game, items: Vec<XVal>) -> XVal {
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

/// The bit `bit` of a flags number.
fn bit(flags: f64, bit: f64) -> XVal {
    yes((0.0..64.0).contains(&bit) && (flags as u64) >> (bit as u64) & 1 != 0)
}

impl Game<'_> {
    /// JohnnyGuitar's perk handlers on the player gaining or losing a perk: (perk, rank
    /// before, rank after) on a gain, (perk) on a loss [observed: plugins3, Toughness
    /// gave (Toughness, 0, 1) and then Toughness].
    pub(crate) fn perk_event(&mut self, perk: FormId, gained: bool, before: u8, after: u8) {
        let hs: Vec<XVal> = self.gaps.perk_handlers.iter().filter(|(add, _, p)| *add == gained && p.is_none_or(|p| p == perk)).map(|(_, f, _)| f.clone()).collect();
        for f in hs {
            let args = if gained { vec![XVal::Ref(perk.0), XVal::Num(f64::from(before)), XVal::Num(f64::from(after))] } else { vec![XVal::Ref(perk.0)] };
            if let Err(error) = self.run_function(&f, None, args, None, RunCtx::default()) {
                self.emit(crate::trace::EventKind::ScriptError { unit: "JohnnyGuitar perk handler".into(), error });
            }
        }
    }

    /// JohnnyGuitar's actor value change handlers on a change of one of the actor's
    /// values: (code, old value, new value) (`functions::actor` tells them).
    pub(crate) fn av_event(&mut self, _actor: FormId, code: u16, old: f64, new: f64) {
        let hs: Vec<XVal> = self.gaps.av_handlers.iter().filter(|(_, c)| c.is_none_or(|c| c == code)).map(|(f, _)| f.clone()).collect();
        for f in hs {
            let args = vec![XVal::Num(f64::from(code)), XVal::Num(old), XVal::Num(new)];
            if let Err(error) = self.run_function(&f, None, args, None, RunCtx::default()) {
                self.emit(crate::trace::EventKind::ScriptError { unit: "JohnnyGuitar actor value handler".into(), error });
            }
        }
    }

    /// JohnnyGuitar's quest handlers on a quest starting or stopping: (quest)
    /// [documented: SetJohnnyOnStartQuestEventHandler; hypothesis: the arguments].
    pub(crate) fn quest_event(&mut self, quest: FormId, started: bool) {
        let hs: Vec<XVal> = self.gaps.quest_handlers.iter().filter(|(s, _, q)| *s == started && q.is_none_or(|q| q == quest)).map(|(_, f, _)| f.clone()).collect();
        for f in hs {
            if let Err(error) = self.run_function(&f, None, vec![XVal::Ref(quest.0)], None, RunCtx::default()) {
                self.emit(crate::trace::EventKind::ScriptError { unit: "JohnnyGuitar quest handler".into(), error });
            }
        }
    }

    /// RunScriptSnippet's snippets whose delay has passed run (`Game::tick`).
    pub(crate) fn run_due_snippets(&mut self) {
        self.gaps.angles_shown.clear();
        if self.gaps.snippets.is_empty() {
            return;
        }
        for s in &mut self.gaps.snippets {
            s.0 = s.0.saturating_sub(1);
        }
        let (due, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut self.gaps.snippets).into_iter().partition(|s| s.0 == 0);
        self.gaps.snippets = wait;
        for (_, src, on) in due {
            run_snippet(self, &src, on);
        }
    }

    /// The Pip-Boy light as scripts see it (`GapsState::light_off_at`).
    fn pipboy_light(&mut self) -> bool {
        if self.gaps.light_off_at.is_some_and(|t| self.time > t) {
            self.gaps.pipboy_light = false;
            self.gaps.light_off_at = None;
        }
        self.gaps.pipboy_light
    }
}

/// A weapon's damage by JohnnyGuitar's rules: the base damage times 0.5 + skill / 200
/// (the skill counted up to 100), and the damage of a hit adding the actor's Melee Damage
/// to a melee weapon's and Unarmed Damage to an unarmed one's, whose hit takes no skill
/// factor. Returns (the skill-scaled base, the hit) [observed: gaps6 and gaps9, the pistol
/// at Guns 17/67/117 (the last capped), the knife's 12.05 = 15 × 0.67 + 2, brass knuckles
/// 19.35 = 18 + 1.35 at Unarmed 17, fists 1.35].
fn weapon_damage(g: &mut Game, actor: FormId, w: FormId) -> (f64, f64) {
    use crate::actor_values::{MELEE_DAMAGE, MELEE_WEAPONS, UNARMED, UNARMED_DAMAGE};
    let damage = crate::nvse::record_value(g, w, "DATA", "base_damage").and_then(|v| v.num().ok()).unwrap_or(0.0);
    let skill_code = g.data.weapon_skills.get(&w).map(|&s| s as u16);
    let skill = skill_code.and_then(|s| crate::functions::actor::actor_value(g, actor, s).ok()).unwrap_or(0.0);
    let scaled = damage * (0.5 + skill.clamp(0.0, 100.0) / 200.0);
    let hit = match skill_code {
        Some(MELEE_WEAPONS) => scaled + crate::functions::actor::actor_value(g, actor, MELEE_DAMAGE).unwrap_or(0.0),
        Some(UNARMED) => damage + crate::functions::actor::actor_value(g, actor, UNARMED_DAMAGE).unwrap_or(0.0),
        _ => scaled,
    };
    (scaled, hit)
}

/// Run a snippet's lines on a reference, as the console runs a line; an error is logged.
fn run_snippet(g: &mut Game, src: &str, on: FormId) {
    if let Err(e) = g.run_source(src, RunCtx { self_ref: Some(on), ..Default::default() }) {
        tracing::warn!("RunScriptSnippet: {e}");
    }
}

/// IPDS DATA's materials in GetImpactDataSetForm's order [observed: gaps10, the 40mm
/// grenade's set: 0–2 stone, dirt, grass, 4 metal, 8 water, 9 hollow metal; 10 and 11
/// none on it].
const IMPACT_MATERIALS: [&str; 12] = ["stone", "dirt", "grass", "glass", "metal", "wood", "organic", "cloth", "water", "hollow_metal", "organic_bug", "organic_glow"];

/// MessageExAlt's default faces, `#0`–`#6` [documented: MessageExAlt].
const FACES: [&str; 7] = ["sad", "neutral", "angry", "thinking", "very_happy", "surprised", "in_pain"];

/// A MessageExAlt text's leading `icon|` and `$sound|` parts and the text after them.
fn split_styled(t: &str) -> (String, Option<String>, Option<String>) {
    let (mut icon, mut sound) = (None, None);
    let mut rest = t;
    while let Some((head, tail)) = rest.split_once('|') {
        let h = head.trim();
        if let Some(s) = h.strip_prefix('$') {
            sound = Some(s.to_string());
        } else if let Some(k) = h.strip_prefix('#').and_then(|d| d.parse::<usize>().ok()) {
            icon = FACES.get(k).map(|f| format!("Interface\\Icons\\Message Icons\\glow_message_vaultboy_{f}.dds"));
        } else if h.to_ascii_lowercase().ends_with(".dds") {
            icon = Some(h.to_string());
        } else {
            break;
        }
        rest = tail;
    }
    (rest.to_string(), icon, sound)
}

/// WEAP sound IDs of GetWeaponSound: the subrecord and which of its repeats [observed:
/// gaps4, 0–2 the 10mm pistol's two SNAM then XNAM, 3 none on it; hypothesis: the rest in
/// the record's order of sound fields].
fn weapon_sound_sub(id: i64) -> Option<(&'static str, usize)> {
    Some(match id {
        0 => ("SNAM", 0),
        1 => ("SNAM", 1),
        2 => ("XNAM", 0),
        3 => ("NAM7", 0),
        4 => ("TNAM", 0),
        5 => ("NAM6", 0),
        6 => ("UNAM", 0),
        7 => ("NAM9", 0),
        8 => ("NAM8", 0),
        9 => ("WMS1", 0),
        10 => ("WMS1", 1),
        11 => ("WMS2", 0),
        _ => return None,
    })
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let base = |g: &Game, f: FormId| crate::functions::base_of(g, f).unwrap_or(f);
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- magic items and effects ----
        // The item's effects in record order [observed: gaps2, the stimpak 4, the first
        // RestoreHealth, past the end 0].
        "getnumeffects" => XVal::Num(form(args, 0).and_then(|f| g.data.effect_items.get(&f)).map_or(0, Vec::len) as f64),
        "getntheffectbase" => {
            let i = num(args, 1, 0.0);
            let f = form(args, 0).unwrap_or(FormId(0));
            let e = g.data.effect_items.get(&f).and_then(|l| if i >= 0.0 { l.get(i as usize) } else { None });
            match e {
                Some(e) => XVal::Ref(g.gaps.effect_bases.get(&(f, i as usize)).copied().unwrap_or(e.effect).0),
                None => XVal::Ref(0),
            }
        }
        // Returns 0 (not the old base), for the session [observed: gaps10, the stimpak's
        // first effect set to Silver Sting's reads 00033595, and back].
        "setntheffectbase" => {
            let (Some(f), Some(b)) = (form(args, 0), form(args, 2)) else { return Some(Ok(zero)) };
            let i = num(args, 1, -1.0);
            if i >= 0.0 && g.data.effect_items.get(&f).is_some_and(|l| (i as usize) < l.len()) {
                g.gaps.effect_bases.insert((f, i as usize), b);
            }
            XVal::Ref(0)
        }
        // ENIT flags, bit by bit [observed: gaps3, the stimpak's flags 5 read 1 0 1].
        // An actor effect's SPIT, an object effect's or ingestible's ENIT; SetEffectFlag's
        // change first (`crate::nvse_jip8`).
        "geteffectflag" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let sub = if g.data.sig(f) == Some(*b"SPEL") { "SPIT" } else { "ENIT" };
            let flags = crate::nvse::record_value(g, f, sub, "flags").and_then(|v| v.num().ok()).unwrap_or(0.0);
            bit(flags, num(args, 1, -1.0))
        }
        // MGEF DATA [observed: gaps3, RestoreHealth archetype 0, actor value 16, flags
        // 112 so bits 0–3 read 0].
        "getbaseeffectarchetype" => XVal::Num(form(args, 0).and_then(|f| g.data.magic_effects.get(&f)).map_or(0, |m| m.archetype) as f64),
        "getbaseeffectav" => XVal::Num(form(args, 0).and_then(|f| g.data.magic_effects.get(&f)).and_then(|m| m.actor_value).map_or(-1.0, f64::from)),
        "getbaseeffectflag" => {
            // SetBaseEffectFlag's change first (`crate::nvse_jip8`).
            let flags = form(args, 0).map_or(0, |f| g.base_effect_flags(f));
            bit(f64::from(flags), num(args, 1, -1.0))
        }
        // ---- ammunition ----
        // AMMO RCIL in record order [observed: gaps2, 10mm JHP hand load 2, the first
        // 00160C45; plain 10mm 0].
        // AddAmmoEffect's and RemoveAmmoEffect's changes first (`crate::nvse_jip7`).
        "getnumammoeffects" => XVal::Num(form(args, 0).map_or(0, |f| g.ammo_effects(f).len()) as f64),
        "getnthammoeffect" => {
            let i = num(args, 1, 0.0);
            let all = form(args, 0).map(|f| g.ammo_effects(f)).unwrap_or_default();
            XVal::Ref(if i >= 0.0 { all.get(i as usize).map_or(0, |f| f.0) } else { 0 })
        }
        // AMEF DATA: 0 the type, 1 the operation, 2 the value [observed: gaps2, the DT
        // reduction (DT Mod, Subtract, 2) reads 2 2 2, trait 3 reads 0].
        "getammoeffecttraitnumeric" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let key = match num(args, 1, -1.0) as i64 {
                0 => "type",
                1 => "operation",
                2 => "value",
                _ => return Some(Ok(zero)),
            };
            // SetAmmoEffectTraitNumericAlt's change first (`crate::nvse_one111`).
            match crate::nvse::record_value(g, f, "DATA", key) {
                Some(XVal::Num(x)) => XVal::Num(x),
                _ => XVal::Num(json_num(g.data.record_field(f, "DATA", key))),
            }
        }
        // ---- globals ----
        // LU's take the global by name too [observed: plugins3, `GetGlobalValue
        // TimeScale`].
        "getglobalvariable" | "getglobalvalue" => {
            let f = form(args, 0).or_else(|| g.data.by_edid(&text(args, 0)));
            XVal::Num(f.and_then(|f| g.world.global(f).ok()).map_or(0.0, f64::from))
        }
        // LU's: returns 0, not the old value the page gives [observed: plugins3].
        "setglobalvalue" => {
            if let Some(f) = form(args, 0).or_else(|| g.data.by_edid(&text(args, 0))) {
                let _ = g.world.set_global(f, num(args, 1, 0.0) as f32);
            }
            zero
        }
        // `set|remove function flag [perk]` [documented: SetJohnnyOnAddPerkEventHandler;
        // observed: plugins3].
        "setjohnnyonaddperkeventhandler" | "setjohnnyonremoveperkeventhandler" => {
            let add = name == "setjohnnyonaddperkeventhandler";
            let Some(f) = args.get(1).cloned() else { return Some(Ok(zero)) };
            let perk = form(args, 3);
            g.gaps.perk_handlers.retain(|(a, h, p)| !(*a == add && *h == f && *p == perk));
            if num(args, 0, 1.0) != 0.0 {
                g.gaps.perk_handlers.push((add, f, perk));
            }
            zero
        }
        // Returns 0 [observed: gaps3, TimeScale set to 12 reads back 12].
        "setglobalvariable" => {
            if let Some(f) = form(args, 0) {
                let _ = g.world.set_global(f, num(args, 1, 0.0) as f32);
            }
            zero
        }
        // ---- perks ----
        // PERK DATA: 0 trait, 1 playable, 2 hidden [observed: gaps2, Toughness reads
        // 0 1 0 0 0; hypothesis: the order of the flags on the page, 3 and on none].
        "getperkflag" | "setperkflag" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let k = num(args, 1, -1.0) as i64;
            let Some(p) = g.data.perks.get(&f) else { return Some(Ok(zero)) };
            let now = match k {
                0 => p.is_trait,
                1 => p.playable,
                2 => p.hidden,
                _ => return Some(Ok(zero)),
            };
            let now = g.gaps.perk_flags.get(&(f, k as u8)).copied().unwrap_or(now);
            // SetPerkFlag returns 0, for the session [observed: gaps7, 0 both ways].
            if name == "setperkflag" {
                g.gaps.perk_flags.insert((f, k as u8), num(args, 2, 0.0) != 0.0);
                return Some(Ok(zero));
            }
            yes(now)
        }
        // [observed: gaps3, Toughness 2 entries]
        "getperkentrycount" => XVal::Num(form(args, 0).and_then(|f| g.data.perks.get(&f)).map_or(0, |p| p.entries.len()) as f64),
        // Entry point 1, ability 5, quest stage 6; past the entries 0 [observed: gaps3 and
        // gaps5, Toughness/Lead Belly/Educated/Commando 1, Rad Resistance and Small Frame 5,
        // Intense Training 6].
        "getnthperkentrytype" => {
            let e = form(args, 0).and_then(|f| g.data.perks.get(&f)).and_then(|p| p.entries.get(num(args, 1, 0.0).max(0.0) as usize));
            XVal::Num(match e.map(|e| &e.effect) {
                Some(crate::data::PerkEffect::EntryPoint(_)) => 1.0,
                Some(crate::data::PerkEffect::Ability(_)) => 5.0,
                Some(crate::data::PerkEffect::QuestStage { .. }) => 6.0,
                _ => 0.0,
            })
        }
        // The entry's string: an activate choice's button label (EPF2) [documented:
        // GetNthPerkEntryString, "Activate Prompt" for type 4]; "" for an entry without
        // one [observed: jip-forms perkentry.0, ""].
        "getnthperkentrystring" => {
            let e = form(args, 0).and_then(|f| g.data.perks.get(&f)).and_then(|p| p.entries.get(num(args, 1, 0.0).max(0.0) as usize));
            XVal::Str(match e.map(|e| &e.effect) {
                Some(crate::data::PerkEffect::EntryPoint(d)) => d.button.clone().unwrap_or_default(),
                _ => String::new(),
            })
        }
        // The entry point's code [observed: gaps4, Toughness's first 56].
        "getnthperkentryfunction" => {
            let e = form(args, 0).and_then(|f| g.data.perks.get(&f)).and_then(|p| p.entries.get(num(args, 1, 0.0).max(0.0) as usize));
            XVal::Num(match e.map(|e| &e.effect) {
                Some(crate::data::PerkEffect::EntryPoint(d)) => f64::from(d.point),
                _ => -1.0,
            })
        }
        // `av [type]`: the modifiers, not the damage [observed: gaps5, Health reads 0 on
        // every filter before and after DamageAV 30]; 0 the permanent one, 1 the
        // temporary one, none both [hypothesis: the filters' meanings].
        "getactorvaluemodifier" => {
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            let code = match args.first() {
                Some(XVal::Num(c)) => *c as u16,
                Some(XVal::Str(n)) => match crate::functions::av_code(g, n) {
                    Ok(c) => c,
                    Err(_) => return Some(Ok(zero)),
                },
                _ => return Some(Ok(zero)),
            };
            let parts = crate::functions::actor::actor_value_parts(g, a, code).map(|p| p[1]).unwrap_or(0.0);
            let temp = crate::functions::actor::temporary(g, a, code);
            XVal::Num(match args.get(1) {
                Some(XVal::Num(t)) if *t == 0.0 => parts,
                Some(XVal::Num(t)) if *t == 1.0 => temp,
                Some(XVal::Num(_)) => 0.0,
                _ => parts + temp,
            })
        }
        // ---- base forms ----
        // CREA DATA type; not a creature -1 [observed: gaps2, the street brahmin 1
        // (Mutated Animal), Doc -1].
        "getcreaturetype" => {
            let b = form(args, 0).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            if g.data.sig(b) == Some(*b"CREA") { XVal::Num(json_num(g.data.record_field(b, "DATA", "type"))) } else { XVal::Num(-1.0) }
        }
        // [observed: gaps4, the street brahmin's PNAM 00015833]
        "getbodypartdata" => {
            let b = form(args, 0).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            // SetBodyPartData's change first (`crate::nvse_jip8`).
            match crate::nvse::record_value(g, b, "PNAM", "") {
                Some(XVal::Ref(r)) if g.data.sig(b) == Some(*b"CREA") => XVal::Ref(r),
                _ => XVal::Ref(0),
            }
        }
        // YNAM [observed: gaps2, the 10mm pistol 000864D5, the stimpak none].
        "getpickupsound" => match form(args, 0).and_then(|f| crate::nvse::record_value(g, f, "YNAM", "")) {
            Some(XVal::Ref(r)) => XVal::Ref(r),
            _ => XVal::Ref(0),
        },
        // DEST stages [observed: gaps3, the 10mm pistol 0].
        "getdestructiondatanumstages" => XVal::Num(form(args, 0).and_then(|f| g.destruction(f)).map_or(0, |d| d.stages.len()) as f64),
        // ---- weapons ----
        // ShowOff's index into DNAM flags1 [observed: gaps4, the 10mm pistol reads 0 on
        // 0–5 and SetWeaponFlag 5 1 reads back 1; hypothesis: flags1's bits].
        "weaponhasflag" | "setweaponflag" => {
            let set = name == "setweaponflag";
            let w = form(args, if set { 2 } else { 1 }).or_else(|| subject.and_then(|r| crate::functions::items::equipped_weapon(g, r)));
            let Some(w) = w else { return Some(Ok(zero)) };
            let flags = match crate::nvse::record_value(g, w, "DNAM", "flags1") {
                Some(XVal::Num(x)) => x as u64,
                _ => 0,
            };
            let k = num(args, 0, -1.0);
            if !(0.0..32.0).contains(&k) {
                return Some(Ok(zero));
            }
            if !set {
                return Some(Ok(yes(flags >> (k as u64) & 1 != 0)));
            }
            let new = if num(args, 1, 0.0) != 0.0 { flags | 1 << (k as u64) } else { flags & !(1 << (k as u64)) };
            g.field_overrides.insert((w, "DNAM/flags1".into()), XVal::Num(new as f64));
            zero
        }
        "getweaponsound" | "setweaponsound" => {
            let w = form(args, 0).unwrap_or(FormId(0));
            let id = num(args, 1, -1.0) as i64;
            let Some((sub, k)) = weapon_sound_sub(id) else { return Some(Ok(XVal::Ref(0))) };
            let key = (w, format!("weaponsound/{id}"));
            if name == "setweaponsound" {
                g.field_overrides.insert(key, XVal::Ref(form(args, 2).map_or(0, |f| f.0)));
                return Some(Ok(zero));
            }
            match g.field_overrides.get(&key) {
                Some(v) => v.clone(),
                None => json_form(g.data.record_all(w, sub).get(k).copied()),
            }
        }
        // JohnnyGuitar's damage and DPS (`weapon_damage`).
        "getcalculatedweapondamage" => {
            let w = form(args, 0).or_else(|| subject.and_then(|r| crate::functions::items::equipped_weapon(g, r))).unwrap_or(FormId(0));
            XVal::Num(weapon_damage(g, subject.unwrap_or(FormId::PLAYER_REF), w).1)
        }
        // The record's attack rate × (the skill-scaled base, without the Melee or Unarmed
        // Damage bonus, + Critical Chance % × CRDT damage × CRDT multiplier), for every
        // kind of weapon [observed: gaps7 and gaps9, the pistol, combat knife, machete,
        // sledgehammer and brass knuckles; the critical part doubles with Critical Chance
        // and does not change with the skill; fists 0].
        "getcalculatedweapondps" => {
            let w = form(args, 0).or_else(|| subject.and_then(|r| crate::functions::items::equipped_weapon(g, r))).unwrap_or(FormId(0));
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            let shots = json_num(g.data.record_field(w, "DNAM", "attack_shots_per_sec"));
            let chance = crate::functions::actor::actor_value(g, a, crate::actor_values::CRITICAL_CHANCE).unwrap_or(0.0).max(0.0) / 100.0;
            let crit = crate::nvse::record_value(g, w, "CRDT", "critical_damage").and_then(|v| v.num().ok()).unwrap_or(0.0) * crate::nvse::record_value(g, w, "CRDT", "crit_percent_mult").and_then(|v| v.num().ok()).unwrap_or(0.0);
            XVal::Num((weapon_damage(g, a, w).0 + chance * crit) * shots)
        }
        // DESC, which GetFormDescription reads (the VM's); the VM formats it; returns 1
        // and holds for the session [observed: gaps6, "NVF desc %g" 3 reads "NVF desc 3"].
        "setformdescription" => {
            let Some(f) = form(args, 0) else { return Some(Ok(zero)) };
            g.field_overrides.insert((f, "DESC/".into()), XVal::Str(text(args, 1)));
            XVal::Num(1.0)
        }
        // The value GetValue reads (DATA value); returns 0 [observed: gaps6, 999 reads
        // back].
        "setvaluealt" => {
            if let Some(f) = form(args, 0) {
                g.field_overrides.insert((f, "DATA/value".into()), XVal::Num(num(args, 1, 0.0)));
            }
            zero
        }
        // EITM; SetObjectEffect takes only an enchantment of the item's kind, a weapon's
        // on a weapon [observed: gaps6, an apparel enchantment on the pistol is refused;
        // hypothesis: the kinds].
        "getobjecteffect" => {
            let f = form(args, 0).or_else(|| subject.map(|r| base(g, r))).unwrap_or(FormId(0));
            match g.field_overrides.get(&(f, "EITM/".to_string())) {
                Some(v) => v.clone(),
                None => json_form(g.data.record_field(f, "EITM", "")),
            }
        }
        "setobjecteffect" => {
            let (Some(f), e) = (form(args, 0), form(args, 1)) else { return Some(Ok(zero)) };
            let want = if g.data.sig(f) == Some(*b"WEAP") { 2 } else { 3 };
            if e.is_none_or(|e| g.data.enchantment_types.get(&e) == Some(&want)) {
                g.field_overrides.insert((f, "EITM/".into()), XVal::Ref(e.map_or(0, |e| e.0)));
            }
            zero
        }
        // PROJ DATA flags bit by bit, and the explosion [observed: gaps6, the 10mm
        // projectile's flags 141 read 1 0 1 1 0 0; a null explosion does not clear it].
        "getprojectileflag" | "setprojectileflag" => {
            let Some(f) = form(args, 0) else { return Some(Ok(zero)) };
            let flags = match crate::nvse::record_value(g, f, "DATA", "flags") {
                Some(XVal::Num(x)) => x as u64,
                _ => 0,
            };
            let k = num(args, 1, -1.0);
            if !(0.0..32.0).contains(&k) {
                return Some(Ok(zero));
            }
            if name == "getprojectileflag" {
                return Some(Ok(yes(flags >> (k as u64) & 1 != 0)));
            }
            let new = if num(args, 2, 0.0) != 0.0 { flags | 1 << (k as u64) } else { flags & !(1 << (k as u64)) };
            g.field_overrides.insert((f, "DATA/flags".into()), XVal::Num(new as f64));
            zero
        }
        "getprojectileexplosion" => match form(args, 0).and_then(|f| crate::nvse::record_value(g, f, "DATA", "explosion")) {
            Some(XVal::Ref(r)) => XVal::Ref(r),
            _ => XVal::Ref(0),
        },
        "setprojectileexplosion" => {
            if let (Some(f), Some(e)) = (form(args, 0), form(args, 1)) {
                g.field_overrides.insert((f, "DATA/explosion".into()), XVal::Ref(e.0));
            }
            zero
        }
        // 0 for the player and the loaded actors, 3 for the rest [observed: gaps6, the
        // player 0, Doc out of the loaded cells 3].
        "getactorprocessinglevel" => {
            let a = subject.unwrap_or(FormId(0));
            XVal::Num(if a == FormId::PLAYER_REF || g.package_actors().contains(&a) { 0.0 } else { 3.0 })
        }
        // Returns 0 [observed: gaps6]; NVForever keeps every reference it changes.
        "setpersistent" => zero,
        // `limb`: the limb's condition to 0 [observed: gaps6, 2 (the left arm) leaves Left
        // Attack Condition 0]; 0 head, 1 torso, 3 right arm, 4–5 legs [hypothesis: the
        // order of the condition values].
        "cripplelimb" => {
            let a = subject.unwrap_or(FormId(0));
            // Perception, Endurance, Left/Right Attack, Left/Right Mobility Condition
            // (25–30).
            let code = match num(args, 0, -1.0) as i64 {
                k @ 0..=5 => 25 + k as u16,
                _ => return Some(Ok(zero)),
            };
            let now = crate::functions::actor::actor_value(g, a, code).unwrap_or(0.0);
            crate::functions::actor::add_damage(g, a, code, now.max(0.0));
            zero
        }
        // ---- containers and inventories ----
        // `item count base` on the base's carried items [documented: BaseRemoveItem].
        "baseremoveitem" | "baseadditem" => {
            let (Some(item), Some(b)) = (form(args, 0), form(args, 2)) else { return Some(Ok(zero)) };
            let n = num(args, 1, 1.0).max(1.0) as i32;
            let add = name == "baseadditem";
            g.world.edit_base_entries(b, |e| {
                if add {
                    e.push(fnv_sim::forms::Entry::new(item, n));
                    return;
                }
                let mut left = n;
                for x in e.iter_mut().filter(|x| x.item == item) {
                    let take = left.min(x.count);
                    x.count -= take;
                    left -= take;
                }
                e.retain(|x| x.count > 0);
            });
            zero
        }
        // The weight of the carried items, with 1 only the playable ones [observed:
        // gaps2, the player 93 either way].
        "getinventoryweight" => {
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            XVal::Num(g.world.carried_weight(a).unwrap_or(0.0))
        }
        // The base items carried, one each, with a type code only that type [observed:
        // gaps2, 27; 40 (weapons) 8].
        "getallitems" => {
            let a = subject.unwrap_or(FormId(0));
            let want = num(args, 0, 0.0) as i32;
            let items: Vec<FormId> = g.world.inventory_mut(a).map(|inv| inv.iter().filter(|&(_, n)| n > 0).map(|(i, _)| i).collect()).unwrap_or_default();
            let items: Vec<XVal> = items
                .into_iter()
                .filter(|&i| want == 0 || g.data.sig(i).and_then(|s| crate::functions::items::form_type_code(&s)) == Some(want))
                .map(|i| XVal::Ref(i.0))
                .collect();
            list(g, items)
        }
        // The inventory reference of what is in the slot [observed: gaps2, an empty slot
        // reads 0].
        "getequippeditemref" => {
            let a = subject.unwrap_or(FormId(0));
            match crate::nvse::call(g, ctx, "getequippedobject", subject, args) {
                Some(Ok(XVal::Ref(i))) if i != 0 => XVal::Ref(g.temp_ref(a, FormId(i)).0),
                _ => XVal::Ref(0),
            }
        }
        // `item [count] [clearOwnership]`: as Drop [documented: DropAlt; observed: gaps3,
        // nine stimpaks leave eight].
        "dropalt" => {
            let (Some(a), Some(item)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let n = g.world.item_count(a, item).unwrap_or(0).min(num(args, 1, 1.0).max(1.0) as i32);
            if n > 0 {
                if let Ok(made) = g.world.drop_item(a, item, n) {
                    g.emit(crate::trace::EventKind::ItemRemoved { from: a, item, count: n, silent: false });
                    for r in made {
                        crate::functions::objects::note_created(g, r);
                    }
                }
            }
            zero
        }
        // ---- references and actors ----
        // On the full name of the base [observed: gaps2, GetName reads it on the
        // reference and the base].
        "setfullnamealt" => {
            let b = form(args, 1).or(subject).map(|f| base(g, f));
            if let Some(b) = b {
                g.name_overrides.insert(b, text(args, 0));
            }
            zero
        }
        "setrefname" => {
            if let Some(r) = subject {
                g.gaps.ref_names.insert(r, text(args, 0));
            }
            zero
        }
        // Returns 0 either way [observed: gaps2]; GetCombatDisabled reads it [observed:
        // gaps3].
        "setcombatdisabled" => {
            if let Some(r) = subject {
                if num(args, 0, 0.0) != 0.0 {
                    g.gaps.combat_disabled.insert(r);
                } else {
                    g.gaps.combat_disabled.remove(&r);
                }
            }
            zero
        }
        "getcombatdisabled" => yes(subject.is_some_and(|r| g.gaps.combat_disabled.contains(&r))),
        // The reference's (or the base's) script has a block of that ID [observed: gaps2,
        // Doc's GameMode (0) reads 1; hypothesis: the IDs are the compiled block codes].
        "hasscriptblock" => {
            let s = form(args, 1).or(subject).and_then(|f| g.ref_script(f).or_else(|| g.base_script(base(g, f))));
            let ev = num(args, 0, -1.0);
            yes(ev >= 0.0 && s.and_then(|s| g.data.script(s)).is_some_and(|u| u.has_block(ev as u16)))
        }
        // In other cells the distance is not a number to compare [observed: gaps2,
        // Floor of it reads -2147483648]: the largest float [hypothesis].
        "getdistance2d" => {
            let (Some(a), Some(b)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let same = match (g.cell_of(a), g.cell_of(b)) {
                (Some(x), Some(y)) => x == y || (g.world.placed.world_of(x).is_some() && g.world.placed.world_of(x) == g.world.placed.world_of(y)),
                _ => false,
            };
            match (same, g.ref_pos_rot(a), g.ref_pos_rot(b)) {
                (true, Some((p, _)), Some((q, _))) => XVal::Num(f64::from((p[0] - q[0]).hypot(p[1] - q[1]))),
                _ => XVal::Num(f64::from(f32::MAX)),
            }
        }
        // The player's teammates; none is no array [observed: gaps2, ar_Null].
        "getteammates" => {
            let t: Vec<XVal> = g.extra.iter().filter(|(_, e)| e.teammate).map(|(r, _)| XVal::Ref(r.0)).collect();
            if t.is_empty() { zero } else { list(g, t) }
        }
        // 0 the actors in the loaded cells, 3 the others; 1 and 2 none here, no array
        // [observed: gaps3, 16, ar_Null, ar_Null, over 1000; hypothesis: NVForever runs
        // every loaded actor fully].
        "getactorsbyprocessinglevel" => {
            let high = g.package_actors();
            let actors: Vec<XVal> = match num(args, 0, -1.0) as i64 {
                0 => high.iter().map(|r| XVal::Ref(r.0)).collect(),
                3 => g.world.placed.refs.iter().filter(|(r, p)| matches!(&p.sig, b"ACHR" | b"ACRE") && !high.contains(r)).map(|(r, _)| XVal::Ref(r.0)).collect(),
                _ => Vec::new(),
            };
            if actors.is_empty() { zero } else { list(g, actors) }
        }
        // The references of a base: in the loaded cells, persistent ones anywhere
        // [observed: gaps4, no loaded stimpak 0, Doc's base with exteriors 1; hypothesis:
        // the persistent ones count].
        "getformrefs" => {
            let Some(b) = form(args, 0) else { return Some(Ok(zero)) };
            let loaded: Vec<FormId> = g.loaded_cells.iter().copied().collect();
            let mut refs: Vec<FormId> = loaded.iter().flat_map(|&c| g.refs_in_cell(c, false)).filter(|&r| base(g, r) == b).collect();
            refs.extend(g.world.placed.refs.iter().filter(|(_, p)| p.base == b && (p.in_persistent_group || p.flags & fnv_sim::refs::FLAG_PERSISTENT != 0)).map(|(&r, _)| r));
            refs.sort();
            refs.dedup();
            let refs = refs.into_iter().map(|r| XVal::Ref(r.0)).collect();
            list(g, refs)
        }
        "isincombatwith" => yes(subject.zip(form(args, 0)).is_some_and(|(a, t)| g.combat_target_of(a) == Some(t))),
        "setactorgravitymult" => {
            if let Some(r) = subject {
                g.gaps.gravity.insert(r, num(args, 0, 1.0));
            }
            zero
        }
        // OBND size on the axis times the reference's scale [observed: gaps7, Doc 46 ×
        // 34 × 132, the Nuka-Cola machine 90 × 70 × 184].
        "getobjectdimensions" => {
            let f = form(args, 1).or(subject).unwrap_or(FormId(0));
            let b = base(g, f);
            let scale = if f != b { g.world.peek(f).and_then(|s| s.scale).or_else(|| g.world.placed.get(f).and_then(|p| p.scale)).unwrap_or(1.0) } else { 1.0 };
            let k = match args.first() {
                Some(XVal::Str(s)) => match s.to_ascii_uppercase().as_str() { "X" => 0, "Y" => 1, _ => 2 },
                Some(XVal::Num(n)) => match *n as i64 { 88 => 0, 89 => 1, _ => 2 },
                _ => 2,
            };
            XVal::Num(g.data.object_bounds.get(&b).map_or(0.0, |o| f64::from(o[k + 3] - o[k]) * f64::from(scale)))
        }
        // None to list gives no array [observed: gaps7, ar_Null for the player's perks,
        // followers, temporary effects and detecting actors, Doc's dropped weapons].
        "getplayerperks" => {
            let mut p: Vec<FormId> = g.player.perks.keys().copied().collect();
            p.sort();
            if p.is_empty() { zero } else { let p = p.into_iter().map(|f| XVal::Ref(f.0)).collect(); list(g, p) }
        }
        "getfollowers" | "getdroppedrefs" | "getdetectingactors" => zero,
        // The first member of the list the actor carries [observed: gaps7, the player
        // with the 10mm ammo list gives Ammo10mm, with a list it has none of 0].
        "pickfromlist" => {
            let a = subject.unwrap_or(FormId(0));
            let l = form(args, 0).unwrap_or(FormId(0));
            let members = crate::functions::list_members(g, l);
            XVal::Ref(members.into_iter().find(|&m| g.world.item_count(a, m).unwrap_or(0) > 0).map_or(0, |m| m.0))
        }
        // All its effects hostile and detrimental (MGEF flags 1 and 4) [documented:
        // GetIsPoison; observed: gaps7, Silver Sting 1, the stimpak 0].
        "getispoison" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let effects = g.data.effect_items.get(&f).map(Vec::as_slice).unwrap_or_default();
            yes(!effects.is_empty() && effects.iter().all(|e| g.data.magic_effects.get(&e.effect).is_some_and(|m| m.flags & 0x5 == 0x5)))
        }
        "isarmoraddon" => yes(form(args, 0).is_some_and(|f| g.data.sig(f) == Some(*b"ARMA"))),
        // A form type an inventory holds [documented: GetIsItem].
        "getisitem" => {
            let f = form(args, 0).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            yes(g.data.sig(f).is_some_and(|s| crate::functions::items::form_type_code(&s).is_some() || matches!(&s, b"CCRD" | b"CHIP" | b"CMNY" | b"IMOD")))
        }
        "getdestructiondatahealth" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            XVal::Num(g.gaps.destruction_health.get(&f).copied().or_else(|| g.destruction(f).map(|d| d.health)).unwrap_or(0) as f64)
        }
        // Returns 0, for the session [observed: gaps8, 75 reads back].
        "setdestructiondatahealth" => {
            if let Some(f) = form(args, 0) {
                g.gaps.destruction_health.insert(f, num(args, 1, 0.0) as i32);
            }
            zero
        }
        // CONT's Respawns flag; SetContainerRespawns takes the value first and returns 0
        // [observed: gaps8].
        "getcontainerrespawns" => {
            let b = form(args, 0).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            let rec = match g.world.forms.get(b).map(|x| &x.data) {
                Some(fnv_sim::forms::BaseData::Container(c)) => c.respawns,
                _ => false,
            };
            yes(g.gaps.respawns.get(&b).copied().unwrap_or(rec))
        }
        "setcontainerrespawns" => {
            if let Some(b) = form(args, 1).or(subject).map(|f| base(g, f)) {
                g.gaps.respawns.insert(b, num(args, 0, 0.0) != 0.0);
            }
            zero
        }
        // On the reference, not its base [observed: gaps8, Doc 0 then 1 then 0, the base
        // 0].
        "getinteractiondisabled" => yes(subject.is_some_and(|r| g.gaps.interaction_disabled.contains(&r))),
        "setinteractiondisabled" => {
            if let Some(r) = subject {
                if num(args, 0, 0.0) != 0.0 {
                    g.gaps.interaction_disabled.insert(r);
                } else {
                    g.gaps.interaction_disabled.remove(&r);
                }
            }
            zero
        }
        // ---- worldspaces and climates ----
        // WRLD CNAM; the setter for the session [observed: gaps8, WastelandNV 0008809B,
        // set to the Strip's and back].
        "getworldspaceclimate" => match form(args, 0).and_then(|w| crate::nvse::record_value(g, w, "CNAM", "")) {
            Some(XVal::Ref(c)) => XVal::Ref(c),
            _ => XVal::Ref(0),
        },
        "setworldspaceclimate" => {
            if let (Some(w), Some(c)) = (form(args, 0), form(args, 1)) {
                g.field_overrides.insert((w, "CNAM/".into()), XVal::Ref(c.0));
            }
            zero
        }
        // The player's worldspace's climate; none indoors [observed: gaps4 and gaps8,
        // 0008809B in the Mojave, unchanged by RefreshCurrentClimate].
        "getcurrentclimate" => {
            let w = g.cell_of(FormId::PLAYER_REF).and_then(|c| g.world.placed.world_of(c));
            match w.and_then(|w| crate::nvse::record_value(g, w, "CNAM", "")) {
                Some(XVal::Ref(c)) => XVal::Ref(c),
                _ => XVal::Ref(0),
            }
        }
        "refreshcurrentclimate" => zero,
        // WRLD DATA flags bit by bit [observed: gaps8, WastelandNV only 7].
        "getworldspaceflag" | "setworldspaceflag" => {
            let Some(w) = form(args, 0) else { return Some(Ok(zero)) };
            let flags = match crate::nvse::record_value(g, w, "DATA", "") {
                Some(XVal::Num(x)) => x as u64,
                _ => 0,
            };
            let k = num(args, 1, -1.0);
            if !(0.0..16.0).contains(&k) {
                return Some(Ok(zero));
            }
            if name == "setworldspaceflag" {
                let new = if num(args, 2, 0.0) != 0.0 { flags | 1 << (k as u64) } else { flags & !(1 << (k as u64)) };
                g.field_overrides.insert((w, "DATA/".into()), XVal::Num(new as f64));
                return Some(Ok(zero));
            }
            yes(flags >> (k as u64) & 1 != 0)
        }
        // CLMT GNAM and MODL [observed: gaps8, the Strip's sky\NV_sunglare.dds and
        // Sky\Stars.nif; a set texture reads back].
        "getclimatesunglaretexture" | "getclimatesuntexture" | "getclimatenightskymodel" => {
            let sub = match name {
                "getclimatesunglaretexture" => "GNAM",
                "getclimatesuntexture" => "FNAM",
                _ => "MODL",
            };
            match form(args, 0).and_then(|c| crate::nvse::record_value(g, c, sub, "")) {
                Some(XVal::Str(s)) => XVal::Str(s),
                _ => XVal::Str(String::new()),
            }
        }
        "setclimatesunglaretexture" | "setclimatesuntexture" | "setclimatenightskymodel" => {
            let sub = match name {
                "setclimatesunglaretexture" => "GNAM/",
                "setclimatesuntexture" => "FNAM/",
                _ => "MODL/",
            };
            if let Some(c) = form(args, 0) {
                g.field_overrides.insert((c, sub.into()), XVal::Str(text(args, 1)));
            }
            zero
        }
        // ARMO TNAM; setting none leaves it [observed: gaps8, leather 0013D3B5, the
        // hazmat suit's none does not clear it].
        "getarmoraudiotemplate" => match form(args, 0).and_then(|f| crate::nvse::record_value(g, f, "TNAM", "")) {
            Some(XVal::Ref(t)) => XVal::Ref(t),
            _ => XVal::Ref(0),
        },
        "setarmoraudiotemplate" => {
            if let (Some(f), Some(t)) = (form(args, 0), form(args, 1)) {
                g.field_overrides.insert((f, "TNAM/".into()), XVal::Ref(t.0));
            }
            zero
        }
        // `armor slot "path"` (formatted by the VM): 0 male biped MODL, 1 female MOD3, 2
        // male world MOD2, 3 female world MOD4, as GetBipedModelPath reads them
        // [documented: SetBipedModelPathAlt; observed: gaps8, the set path reads back].
        "setbipedmodelpathalt" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let sub = match num(args, 1, -1.0) as i64 {
                0 => "MODL/",
                1 => "MOD3/",
                2 => "MOD2/",
                3 => "MOD4/",
                _ => return Some(Ok(zero)),
            };
            g.field_overrides.insert((f, sub.into()), XVal::Str(text(args, 2)));
            zero
        }
        // The base's MODL [observed: gaps7, the dog's creature skeleton, Doc's
        // Characters\_Male\Skeleton.NIF]; a leveled or templated actor's as NX reads it
        // (`crate::nvse_nx::skeleton_model`).
        "getskeletonmodel" => XVal::Str(form(args, 0).or(subject).map(|f| crate::nvse_nx::skeleton_model(g, f)).unwrap_or_default()),
        "getownerofcell" => XVal::Ref(form(args, 0).and_then(|c| g.data.cells.get(&c)).and_then(|c| c.owner).map_or(0, |o| o.0)),
        // PKDT general flags bit by bit [observed: gaps7, Doc's sandbox 9 and 21 set];
        // SetPackageFlag returns 0, for the session [observed: gaps8].
        "getpackageflag" | "setpackageflag" => {
            let p = form(args, 0).unwrap_or(FormId(0));
            let k = num(args, 1, -1.0);
            if !(0.0..32.0).contains(&k) {
                return Some(Ok(zero));
            }
            let Some(def) = g.data.package_defs.get(&p) else { return Some(Ok(zero)) };
            let flags = g.gaps.package_flags.get(&p).copied().unwrap_or(def.flags);
            let old = flags >> (k as u32) & 1;
            if name == "setpackageflag" {
                let new = if num(args, 2, 0.0) != 0.0 { flags | 1 << (k as u32) } else { flags & !(1 << (k as u32)) };
                g.gaps.package_flags.insert(p, new);
                return Some(Ok(zero));
            }
            XVal::Num(f64::from(old))
        }
        // PERK DATA min level; SetPerkLevel for the session [observed: gaps7, Bloody Mess
        // 6, set to 8 and back].
        "getperklevel" => {
            let p = form(args, 0).unwrap_or(FormId(0));
            XVal::Num(g.gaps.perk_levels.get(&p).copied().or_else(|| g.data.perks.get(&p).map(|d| d.min_level)).map_or(0.0, f64::from))
        }
        "setperklevel" => {
            if let Some(p) = form(args, 0) {
                g.gaps.perk_levels.insert(p, num(args, 1, 0.0).clamp(0.0, 255.0) as u8);
            }
            zero
        }
        // The explosion's form traits, 0 light … 6 placed impact object as GetExplosion-
        // TraitForm reads them; no form clears it [observed: gaps7].
        "setexplosiontraitform" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let key = match num(args, 1, -1.0) as i64 {
                0 => "DATA/light",
                1 => "DATA/sound_1",
                2 => "DATA/sound_2",
                3 => "DATA/impact_dataset",
                4 => "INAM/",
                5 => "EITM/",
                6 => "MNAM/",
                _ => return Some(Ok(zero)),
            };
            g.field_overrides.insert((f, key.into()), XVal::Ref(form(args, 2).map_or(0, |e| e.0)));
            zero
        }
        // EXPL DATA flags; returns 0 [observed: gaps7, 0 both ways].
        "setexplosionflag" => {
            let Some(f) = form(args, 0) else { return Some(Ok(zero)) };
            let flags = match crate::nvse::record_value(g, f, "DATA", "flags") {
                Some(XVal::Num(x)) => x as u64,
                _ => 0,
            };
            let k = num(args, 1, -1.0);
            if !(0.0..32.0).contains(&k) {
                return Some(Ok(zero));
            }
            let new = if num(args, 2, 0.0) != 0.0 { flags | 1 << (k as u64) } else { flags & !(1 << (k as u64)) };
            g.field_overrides.insert((f, "DATA/flags".into()), XVal::Num(new as f64));
            zero
        }
        // The leveled list a placed actor came from, else its own base [documented:
        // GetLeveledActorBase; observed: gaps7, Doc his base 00104C0C].
        "getleveledactorbase" => {
            let b = subject.map(|r| g.world.placed.get(r).map_or_else(|| base(g, r), |p| p.base));
            XVal::Ref(b.map_or(0, |b| b.0))
        }
        // An actor can move [documented: IsMobile; observed: gaps7, Doc 1].
        "ismobile" => yes(subject.is_some_and(|r| crate::ai::is_actor_base(g.world.forms, base(g, r)))),
        // A null form when none is set [observed: plugins2, 00000000].
        "getcustommapmarker" => XVal::Ref(0),
        // `[base]`: the base's item list, one entry each [observed: gaps7, the Nuka-Cola
        // machine 3, Doc 7; hypothesis: each element the item].
        "getbaseitems" => {
            let b = form(args, 0).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            let items: Vec<XVal> = g.world.base_entries(b).unwrap_or_default().iter().map(|e| XVal::Ref(e.item.0)).collect();
            list(g, items)
        }
        // `typeCode [modIndex]`: the loaded forms of the type in the game's order
        // (`GameData::forms_of_type`), of one plugin when given [observed: gaps1, weapons
        // 496 in all, 265 of FalloutNV.esm; aid 226 and 189].
        "getloadedtypearray" => {
            let code = num(args, 0, -1.0) as i32;
            let plugin = args.get(1).map(|_| num(args, 1, 0.0) as u32);
            let forms: Vec<FormId> = g.data.forms_of_type(code).into_iter().filter(|f| plugin.is_none_or(|p| f.0 >> 24 == p)).collect();
            let forms = forms.into_iter().map(|f| XVal::Ref(f.0)).collect();
            list(g, forms)
        }
        // Carried weight over capacity, in percent [observed: gaps7, the player 46.50].
        "getencumbrancerate" => {
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            let w = g.world.carried_weight(a).unwrap_or(0.0);
            let cap = crate::functions::actor::actor_value(g, a, crate::actor_values::CARRY_WEIGHT).unwrap_or(0.0);
            XVal::Num(if cap > 0.0 { w / cap * 100.0 } else { 0.0 })
        }
        "gethardcoretracking" => yes(!g.jip2.hardcore_tracking_off),
        // `frames "source"` (formatted by the VM, `%r` a new line): the lines run on the
        // calling reference, the player when there is none, at once for 0, else that many
        // frames later [documented: RunScriptSnippet; observed: gaps10, "ModAV Luck 1"
        // raised the player's Luck at once].
        "runscriptsnippet" => {
            let delay = num(args, 0, 0.0).max(0.0) as u32;
            let src = text(args, 1);
            let on = subject.unwrap_or(FormId::PLAYER_REF);
            if delay == 0 {
                run_snippet(g, &src, on);
            } else {
                g.gaps.snippets.push((delay, src, on));
            }
            zero
        }
        // IPDS DATA by material and IPCT MODL [observed: gaps10].
        "getimpactdatasetform" | "setimpactdatasetform" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let Some(m) = IMPACT_MATERIALS.get(num(args, 1, -1.0).max(-1.0) as usize).filter(|_| num(args, 1, -1.0) >= 0.0) else { return Some(Ok(XVal::Ref(0))) };
            if name == "setimpactdatasetform" {
                g.field_overrides.insert((f, format!("DATA/{m}")), XVal::Ref(form(args, 2).map_or(0, |x| x.0)));
                return Some(Ok(zero));
            }
            match crate::nvse::record_value(g, f, "DATA", m) {
                Some(XVal::Ref(r)) => XVal::Ref(r),
                _ => XVal::Ref(0),
            }
        }
        "getimpactdatamodel" | "getweaponshellcasingmodel" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let sub = if name == "getimpactdatamodel" { "MODL" } else { "MOD2" };
            match crate::nvse::record_value(g, f, sub, "") {
                Some(XVal::Str(s)) => XVal::Str(s),
                _ => XVal::Str(String::new()),
            }
        }
        "setimpactdatamodel" | "setweaponshellcasingmodel" => {
            if let Some(f) = form(args, 0) {
                let sub = if name == "setimpactdatamodel" { "MODL/" } else { "MOD2/" };
                g.field_overrides.insert((f, sub.into()), XVal::Str(text(args, 1)));
            }
            zero
        }
        // WRLD INAM [observed: gaps10, WastelandNV 0008809D].
        "getworldspaceimagespace" => match form(args, 0).and_then(|w| crate::nvse::record_value(g, w, "INAM", "")) {
            Some(XVal::Ref(r)) => XVal::Ref(r),
            _ => XVal::Ref(0),
        },
        "setworldspaceimagespace" => {
            if let (Some(w), Some(i)) = (form(args, 0), form(args, 1)) {
                g.field_overrides.insert((w, "INAM/".into()), XVal::Ref(i.0));
            }
            zero
        }
        // `x y z [translate]`: the position, or with translate a move in the reference's
        // own frame, +Y ahead: world ΔX = x cos h + y sin h, ΔY = y cos h − x sin h for
        // the heading h [observed: gaps11, Doc at 186.36° and the player at 0° and 90°].
        "setposex" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            let Some((p, rot)) = g.ref_pos_rot(r) else { return Some(Ok(zero)) };
            let d = [num(args, 0, 0.0) as f32, num(args, 1, 0.0) as f32, num(args, 2, 0.0) as f32];
            let to = if num(args, 3, 0.0) != 0.0 {
                let (s, c) = rot[2].sin_cos();
                [p[0] + d[0] * c + d[1] * s, p[1] - d[0] * s + d[1] * c, p[2] + d[2]]
            } else {
                d
            };
            if let Ok(st) = g.world.get(r) {
                st.pos = to;
            }
            g.moved.insert(r);
            g.teleported.insert(r);
            zero
        }
        // No tilt shows on a person [observed: gaps10, Doc reads 0 before and after
        // SetActorTiltAngle X 20].
        "getactortiltangle" => zero,
        "setactortiltangle" => zero,
        // On an inventory reference: its condition, 1 as a fraction of the base health;
        // returns 1 [observed: gaps11, 50 reads 50; 0.5 1 reads 40 of the pistol's 80].
        "setitemrefcurrenthealth" | "setweaponrefmodflags" => {
            let Some((c, it)) = subject.and_then(|r| g.temp_item(r)) else { return Some(Ok(zero)) };
            let full = crate::nvse::record_value(g, it, "DATA", "health").and_then(|v| v.num().ok()).unwrap_or(0.0);
            let v = num(args, 0, 0.0);
            let ok = g.world.inventory_mut(c).is_ok_and(|inv| {
                let Some((_, e, _)) = inv.groups().find(|(i, _, _)| *i == it) else { return false };
                if name == "setweaponrefmodflags" {
                    inv.set_mods_one(it, e, v.clamp(0.0, 255.0) as u8)
                } else {
                    let cond = if num(args, 1, 0.0) != 0.0 { v } else if full > 0.0 { v / full } else { 1.0 };
                    inv.set_condition_one(it, e, cond as f32)
                }
            });
            yes(ok)
        }
        // The magazine of the actor (the player's as the engine tells it); an inventory
        // reference has none and SetCurrentAmmoRounds on one does nothing [observed:
        // gaps11, the item 0, the player 12; the player set to 3 reads 3].
        "getcurrentammorounds" | "setcurrentammorounds" => {
            let Some(a) = subject.filter(|&r| g.temp_item(r).is_none()) else { return Some(Ok(zero)) };
            if a != FormId::PLAYER_REF {
                return Some(Ok(zero));
            }
            let call = if name == "setcurrentammorounds" { "setplayercurrentammorounds" } else { "getplayercurrentammorounds" };
            return crate::nvse::call(g, ctx, call, None, args);
        }
        // ---- keywords ----
        // The base forms with the keyword [observed: gaps3, one after AssignKeyword, an
        // unknown keyword an empty array].
        "getkeywordforms" => {
            let k = text(args, 0).to_ascii_lowercase();
            let mut forms: Vec<FormId> = g.jip2.keywords.iter().filter(|(_, s)| s.contains(&k)).map(|(&f, _)| f).collect();
            forms.sort();
            let forms = forms.into_iter().map(|f| XVal::Ref(f.0)).collect();
            list(g, forms)
        }
        // ---- cells, climates, recipes ----
        // CELL DATA flags bit by bit [observed: gaps3, Doc's house (Interior, Public)
        // reads 1 0 0 0].
        // 0-7 DATA, 8-16 the lighting template inherit flags (LNAM bits 0-8)
        // [documented: the page's table; hypothesis: LNAM's order]; SetCellFlag's change
        // first (`crate::nvse_jip8`).
        "getcellflag" => {
            let c = form(args, 0).unwrap_or(FormId(0));
            let k = num(args, 1, -1.0);
            let (sub, k) = if k >= 8.0 { ("LNAM", k - 8.0) } else { ("DATA", k) };
            let flags = crate::nvse::record_value(g, c, sub, "").and_then(|v| v.num().ok()).unwrap_or(0.0);
            bit(flags, k)
        }
        // XCIM [observed: gaps4, Doc's house 0001507A].
        "getcellimagespace" => json_form(form(args, 0).and_then(|c| g.data.record_field(c, "XCIM", ""))),
        // CLMT WLST [observed: gaps4, the Strip's climate 1 weather, 00161241, global
        // none, past the end 0].
        // ClimateAddWeatherType's and ClimateRemoveWeatherType's changes first
        // (`crate::nvse_jip7`).
        "getclimatenumweathertypes" => XVal::Num(form(args, 0).map_or(0, |c| g.climate_weathers(c).len()) as f64),
        "getclimatenthweathertype" | "getclimatenthweatherglobal" | "getclimatenthweatherchance" => {
            let i = num(args, 1, -1.0);
            let all = form(args, 0).map(|c| g.climate_weathers(c)).unwrap_or_default();
            let e = if i >= 0.0 { all.get(i as usize).copied() } else { None };
            match name {
                "getclimatenthweathertype" => XVal::Ref(e.map_or(0, |e| e.weather.0)),
                "getclimatenthweatherglobal" => XVal::Ref(e.and_then(|e| e.global).map_or(0, |f| f.0)),
                _ => XVal::Num(e.map_or(0, |e| e.chance) as f64),
            }
        }
        // Night outside the climate's sunrise begin to sunset end (TNAM, ten-minute steps)
        // [observed: gaps4, the Strip's climate at 10 reads 0; hypothesis: the edges].
        "isnight" => {
            let c = form(args, 0).unwrap_or(FormId(0));
            let hour = g.world.global(FormId(0x38)).map_or(12.0, f64::from);
            let t = |k: &str| json_num(g.data.record_field(c, "TNAM", k)) / 6.0;
            let (rise, set) = (t("sunrise_begin"), t("sunset_end"));
            yes(rise != set && (hour < rise || hour >= set))
        }
        "getrecipeoutputforms" => {
            let outs: Vec<XVal> = form(args, 0).and_then(|f| crate::nvse_lutana::recipe(g, f)).map(|r| r.outputs.iter().map(|(f, _)| XVal::Ref(f.0)).collect()).unwrap_or_default();
            list(g, outs)
        }
        "getactivequest" => XVal::Ref(g.ui.active_quest.map_or(0, |q| q.0)),
        // ---- the player and the interface ----
        // `seconds "[icon|][$sound|]text"`: a corner message; the icon a texture under the
        // textures folder or `#0`–`#6` the page's default faces, the sound a SOUN's Editor
        // ID after `$` [documented: MessageExAlt].
        "messageexalt" => {
            let seconds = num(args, 0, 2.0) as f32;
            let (text, icon, sound) = split_styled(&text(args, 1));
            if let Some(s) = sound.and_then(|s| g.data.by_edid(&s)) {
                g.ask_sound(crate::sound::SoundRequest { sound: s, at: None, system: true, id: 0 });
            }
            g.emit(crate::trace::EventKind::NoteShown { text, icon, seconds });
            zero
        }
        // [observed: gaps2, off at the start; on at once, off from the next frame]
        "togglepipboylight" => {
            match args.first() {
                Some(XVal::Num(x)) if *x != 0.0 => {
                    g.gaps.pipboy_light = true;
                    g.gaps.light_off_at = None;
                }
                Some(XVal::Num(_)) if g.gaps.pipboy_light && g.gaps.light_off_at.is_none() => g.gaps.light_off_at = Some(g.time),
                _ => {}
            }
            yes(g.pipboy_light())
        }
        // `slot [toggle]`: the slot's visibility, 1 shown [observed: gaps3, slot 2 reads 1].
        "togglebipedslotvisibility" => {
            let slot = num(args, 0, -1.0).max(0.0) as u32;
            if let Some(XVal::Num(t)) = args.get(1) {
                if *t == 0.0 {
                    g.gaps.hidden_slots.insert(slot);
                } else {
                    g.gaps.hidden_slots.remove(&slot);
                }
            }
            yes(!g.gaps.hidden_slots.contains(&slot))
        }
        // The view changes at the engine's next frame [observed: engine1, third person
        // reads from the next frame, first person after one to three]; sneaking at once,
        // IsSneaking reading it in the same frame [observed: engine1].
        "togglefirstperson" => {
            g.gaps.view_request = Some(num(args, 0, 1.0) != 0.0);
            zero
        }
        "toggleplayersneaking" => {
            let on = num(args, 0, 1.0) != 0.0;
            g.gaps.sneak_request = Some(on);
            let mut m = g.motion(FormId::PLAYER_REF);
            m.sneaking = on;
            g.set_motion(FormId::PLAYER_REF, m);
            zero
        }
        // `x1 y1 z1 x2 y2 z2 [flag]`: the distance from the first point to the first thing
        // hit on the way to the second, 0 for none (`crate::ray`, the engine's physics, no
        // terrain) [observed: engine1, 997 and 1963 to the walls by the airport, 0 with no
        // hit or a zero-length cast; engine2 and engine3, flags 0–4 read alike].
        "getp2praycastrange" => {
            let p: Vec<f32> = (0..6).map(|k| num(args, k, 0.0) as f32).collect();
            let (a, b) = ([p[0], p[1], p[2]], [p[3], p[4], p[5]]);
            if a == b {
                return Some(Ok(zero));
            }
            let hit = crate::ray::cast(a, b, crate::ray::CastKind::Scene);
            tracing::debug!("GetP2PRayCastRange {a:?} -> {b:?}: {hit:?}");
            XVal::Num(hit.map_or(0.0, f64::from))
        }
        // 2 playing, 0 not (1 paused is not told apart); a type from the frame PlayMusic
        // asks for it, none given any music [documented: IsMusicPlaying; observed:
        // engine2, musDocMitchellHouse reads 2 in PlayMusic's frame, the place's music 2].
        "ismusicplaying" => {
            let on = match form(args, 0) {
                Some(t) => g.gaps.music_type == Some(t),
                None => g.gaps.music_any || g.gaps.music_type.is_some(),
            };
            XVal::Num(if on { 2.0 } else { 0.0 })
        }
        // `x y`: the terrain's height there, from the engine's loaded terrain; outside it
        // -20.48 [observed: gaps7, 60.3 at the player; -20.48 far outside the loaded cells].
        // `x y z force [origin]`: the reference's loose body is pushed away from the
        // origin (the reference's place, or `origin`'s, moved by x y z), towards it for a
        // negative force; the engine applies it at its next frame [documented: PushObject;
        // observed: phys2, the speed grows with the force, `nvf_play::script`].
        "pushobject" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            let at = form(args, 4).unwrap_or(r);
            if let Some((p, _)) = g.ref_pos_rot(at) {
                let origin = [p[0] + num(args, 0, 0.0) as f32, p[1] + num(args, 1, 0.0) as f32, p[2] + num(args, 2, 0.0) as f32];
                g.gaps.pushes.push((r, origin, num(args, 3, 0.0) as f32));
            }
            zero
        }
        // JohnnyGuitar: the enabled Pip-Boy stations that broadcast everywhere, wherever
        // the reference stands, in FormID order; none is an empty array [documented:
        // GetAvailableRadios; observed: phys2, phys3, radio1, the same 5 inside the
        // nvftest interior and outdoors at Goodsprings: Radio New Vegas, Mojave Music
        // Radio and the three add-ons' broadcasts; not the radius stations nor the
        // everywhere stations off the Pip-Boy (Gomorrah's lounge music, EDE's frequency);
        // hypothesis: the order, which was not FormID order].
        "getavailableradios" => {
            if subject.is_none() {
                return Some(Ok(zero));
            }
            let mut found: Vec<u32> = g
                .gaps
                .radio_sites
                .iter()
                .flatten()
                .filter(|s| s.on_pipboy && matches!(s.broadcast.range, fnv_world::radio::BroadcastRange::Everywhere))
                .filter(|s| g.world.is_enabled(s.station).unwrap_or(!s.initially_disabled))
                .map(|s| s.station.0)
                .collect();
            found.sort_unstable();
            list(g, found.into_iter().map(XVal::Ref).collect())
        }
        // A live mine that is not going off, 0 for the player's own mines and anything
        // else [documented: GetMineArmed, "always returns 0 on player-placed mines";
        // observed: phys1, phys2, a placed frag mine read 1 from its first frame, the
        // player 0; hypothesis: a burning fuse reads 0].
        "getminearmed" => yes(subject.is_some_and(|r| g.gaps.armed_mines.contains(&r))),
        // Seconds since the actor's body left the ground, 0 on it [documented:
        // GetFallTimeElapsed; observed: phys3, rising 0.094 every 5 frames from a SetPos
        // lift of 600 to 1.297, 0 from the landing on; hypothesis: a jump counts too].
        "getfalltimeelapsed" => XVal::Num(f64::from(subject.and_then(|a| g.gaps.air_times.get(&a)).copied().unwrap_or(0.0))),
        // `navmesh`: the switches DisableNavmesh and EnableNavMesh keep, the navmesh named
        // by reference (`functions::navigation`) [documented: DisableNavMeshAlt,
        // EnableNavMeshAlt].
        "disablenavmeshalt" | "enablenavmeshalt" => {
            if let Some(m) = form(args, 0) {
                g.nav_switches.navmeshes.insert(m, name == "enablenavmeshalt");
            }
            zero
        }
        // `thing [scan]`: the player's theft of the item or container reference, as taking
        // it would be (`crime`: karma unless the owner is Evil, the owner's side turns on
        // the player when it sees it) [documented: SendStealingAlarm]. With `scan` 1 no
        // alarm, as nothing here marks items stolen; other thieves send none [hypothesis:
        // NVForever's crimes are the player's].
        "sendstealingalarm" => {
            let thief = subject.unwrap_or(FormId::PLAYER_REF);
            if let (Some(r), true, false) = (form(args, 0), thief == FormId::PLAYER_REF, num(args, 1, 0.0) != 0.0) {
                let item = base(g, r);
                g.player_stole(Some(r), None, item, 1);
            }
            zero
        }
        "getterrainheight" => {
            let (x, y) = (num(args, 0, 0.0) as f32, num(args, 1, 0.0) as f32);
            let top = 100_000.0f32;
            match crate::ray::cast([x, y, top], [x, y, -top], crate::ray::CastKind::Terrain) {
                Some(d) => XVal::Num(f64::from(top - d)),
                None => XVal::Num(-20.48),
            }
        }
        // `[max]`: the distance along the crosshair to what it meets, 0 for nothing
        // within reach (the engine's crosshair ray) [observed: gaps8, 93.84 on the save;
        // hypothesis: nothing reads 0].
        "getreticlerange" => {
            let hit = g.senses.reticle.as_ref().and_then(|r| r.hit);
            let max = num(args, 0, 0.0);
            XVal::Num(hit.map_or(0.0, f64::from).min(if max > 0.0 { max } else { f64::MAX }))
        }
        // An attack is on the way: the body's attack, follow-through or return action
        // [documented: IsAttacking; observed: gaps4, 0 standing; hypothesis: the actions].
        "isattacking" => {
            use crate::anim_state::AnimAction;
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            yes(g.senses.anim.get(&a).and_then(|s| s.action).is_some_and(|x| matches!(x, AnimAction::Attack | AnimAction::FollowThrough | AnimAction::AttackReturn)))
        }
        // [documented: IsPCInCombat] the player is hunted (`awareness`).
        "ispcincombat" => yes(g.awareness.hunted(FormId::PLAYER_REF)),
        // Nothing to reload: the engine draws the equipped models as they are.
        "reloadequippedmodels" => zero,
        // NVForever is no 32-bit program [observed: gaps4, 0].
        "getislaa" => zero,
        // The open menu's code, 0 in game mode [observed: gaps3, 0].
        "getactivemenumode" => XVal::Num(g.menu.map_or(0.0, f64::from)),
        // [observed: gaps3, 1]
        "getglobaltimemultiplier" => XVal::Num(g.ui.time_multiplier.unwrap_or(1.0)),
        // No water under the crosshair here [observed: gaps2, 0; hypothesis: the engine
        // tells no crosshair water].
        "getcrosshairwater" => XVal::Ref(0),
        // Return 0 [observed: gaps4].
        "setsoundflag" | "stopsoundalt" => zero,
        // An unknown patch [observed: gaps2 and gaps4, 0 for both].
        "getoptionalpatch" | "setoptionalpatch" => zero,
        _ => return None,
    };
    Some(Ok(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_sound_ids_follow_the_observed_order() {
        assert_eq!(weapon_sound_sub(0), Some(("SNAM", 0)));
        assert_eq!(weapon_sound_sub(1), Some(("SNAM", 1)));
        assert_eq!(weapon_sound_sub(2), Some(("XNAM", 0)));
        assert_eq!(weapon_sound_sub(12), None);
    }

    #[test]
    fn styled_messages_take_their_icon_and_sound_off_the_front() {
        let (t, i, s) = split_styled("Interface\\Icons\\a.dds|$UIRepGood|Name is X|Y");
        assert_eq!((t.as_str(), i.as_deref(), s.as_deref()), ("Name is X|Y", Some("Interface\\Icons\\a.dds"), Some("UIRepGood")));
        let (t, i, _) = split_styled("#0|Test");
        assert_eq!(t, "Test");
        assert!(i.is_some_and(|i| i.ends_with("glow_message_vaultboy_sad.dds")));
        assert_eq!(split_styled("plain").0, "plain");
    }

    #[test]
    fn flag_bits() {
        assert_eq!(bit(5.0, 0.0), XVal::Num(1.0));
        assert_eq!(bit(5.0, 1.0), XVal::Num(0.0));
        assert_eq!(bit(5.0, 2.0), XVal::Num(1.0));
        assert_eq!(bit(5.0, -1.0), XVal::Num(0.0));
    }
}
