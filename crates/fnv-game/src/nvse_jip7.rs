//! JIP LN NVSE, batch 7 (#1006): ammunition, ammo effects, projectiles, explosions, impact
//! data, weathers and climates, armour [documented: each page in the wiki export;
//! docs/spec-notes/jip-records-7.md]. Setters change the form for the session, read by the
//! getters (this file's and the earlier batches').

use crate::game::{Game, RunCtx};
use crate::nvse::{field_number, F32, U8};
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use serde_json::Value as Json;
use std::collections::HashMap;

/// One climate weather (WLST): the weather, its chance, the global that gates it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimateWeather {
    pub weather: FormId,
    pub chance: i64,
    pub global: Option<FormId>,
}

/// What this batch keeps for the session (nothing is saved [hypothesis], except where a
/// page says so: SetDamageToArmorMaxPercent "is saved in the game" [documented], kept in
/// the session only for now).
#[derive(Debug, Default)]
pub struct Jip7State {
    /// AddAmmoEffect's and RemoveAmmoEffect's lists, by ammunition (`Game::ammo_effects`).
    pub ammo_effects: HashMap<FormId, Vec<FormId>>,
    /// ClimateAddWeatherType's and ClimateRemoveWeatherType's lists, by climate
    /// (`Game::climate_weathers`).
    pub climate_weathers: HashMap<FormId, Vec<ClimateWeather>>,
    /// SetWeatherTransitionTimeOverride's seconds (`None`: the weathers' own).
    pub weather_transition: Option<f64>,
    /// SetArmorConditionPenalty's multiplier (`None`: the default 1.0).
    pub armor_condition_penalty: Option<f64>,
    /// SetDamageToArmorMaxPercent's share (`None`: not set).
    pub damage_to_armor_max: Option<f64>,
    /// SetImpactDataTraitNumeric 13's flags as set, read back by GetImpactDataTrait 16.
    pub impact_flags: HashMap<FormId, u32>,
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

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k) {
        Some(XVal::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// [`field_number`] for a field of this range, whole for a field that is not an f32 (an
/// integer or an enumeration).
fn fit(v: f64, range: (f64, f64)) -> Option<f64> {
    let v = field_number(v, range.0, range.1)?;
    Some(if range == F32 { v } else { v.trunc() })
}

/// How a trait setter takes a number the field cannot hold, as the original showed
/// [observed: jip7, jip7b, 2026-11-08].
#[derive(Clone, Copy, PartialEq)]
enum Take {
    /// clamped to the range (`fit`)
    Clamp((f64, f64)),
    /// cut to a whole number and kept as a byte: 300 reads 44, -1 reads 255
    Wrap8,
    /// cut to a whole number, sign kept, no range
    Int,
    /// an f32 that refuses a negative number
    NonNeg,
    /// only these values; any other sets nothing
    OneOf(&'static [f64]),
}

fn take(v: f64, how: Take) -> Option<f64> {
    if !v.is_finite() {
        return None;
    }
    match how {
        Take::Clamp(r) => fit(v, r),
        Take::Wrap8 => Some(f64::from(v.trunc() as i64 as u8)),
        Take::Int => Some(v.trunc()),
        Take::NonNeg => (v >= 0.0).then_some(v),
        Take::OneOf(ok) => ok.contains(&v).then_some(v),
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

fn json_form(v: Option<&Json>) -> Option<FormId> {
    v.and_then(Json::as_str).and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()).filter(|&f| f != 0).map(FormId)
}

/// AMMO numeric traits 0-3 [documented: SetAmmoTraitNumeric's table: 0 speed, 1
/// projectiles per shot, 2 ammo consumed percent, 3 clip rounds]; the fields: xEdit AMMO.
/// Each with its field's range (u32 projectiles, u8 clip rounds).
// Projectiles per shot keeps its sign (-5 reads -5, -2.5 reads -2) and the clip rounds wrap
// as a byte (300 reads 44) [observed: jip7 ammo.trait.*, jip7b ammo.clip.*, ammo.pps.*].
const AMMO_NUMERIC: [(&str, &str, Take); 4] =
    [("DATA", "speed", Take::Clamp(F32)), ("DAT2", "projectiles_per_shot", Take::Int), ("DAT2", "consumed_percentage", Take::Clamp(F32)), ("DATA", "clip_rounds", Take::Wrap8)];

/// AMEF numeric traits 0-2 [documented: SetAmmoEffectTraitNumeric's table] as
/// GetAmmoEffectTraitNumeric reads them (`nvse_gaps`).
/// The type is 0-5 and the operation 0-2 [documented: the page's tables].
// A type past 0-5 and a negative value set nothing [observed: jip7 amef.trait.0.9, jip7b amef.*].
const AMEF_NUMERIC: [(&str, Take); 3] =
    [("type", Take::OneOf(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0])), ("operation", Take::Clamp((0.0, 2.0))), ("value", Take::NonNeg)];

/// PROJ numeric traits 0-14 [documented: SetProjectileTraitNumeric's table]; 14 the
/// detection sound level is VNAM [the field: xEdit PROJ].
/// The type is one of 1-16 and the sound level 0-2 [documented: the page's tables]; the
/// rest are f32 fields.
// The type takes 0 or one of the type bits 1, 2, 4, 8, 16 (3 and 17 set nothing); the
// sound level only 1 or 2 (0, 3, 5 and -1 set nothing) [observed: jip7 proj.settrait.*,
// jip7b proj.*].
const PROJ_NUMERIC: [(&str, &str, Take); 15] = [
    ("DATA", "type", Take::OneOf(&[0.0, 1.0, 2.0, 4.0, 8.0, 16.0])),
    ("DATA", "gravity", Take::Clamp(F32)),
    ("DATA", "speed", Take::Clamp(F32)),
    ("DATA", "range", Take::Clamp(F32)),
    ("DATA", "tracer_chance", Take::Clamp(F32)),
    ("DATA", "explosion_alt_trigger_proximity", Take::Clamp(F32)),
    ("DATA", "explosion_alt_trigger_timer", Take::Clamp(F32)),
    ("DATA", "muzzle_flash_duration", Take::Clamp(F32)),
    ("DATA", "fade_duration", Take::Clamp(F32)),
    ("DATA", "impact_force", Take::Clamp(F32)),
    ("DATA", "rotation.x", Take::Clamp(F32)),
    ("DATA", "rotation.y", Take::Clamp(F32)),
    ("DATA", "rotation.z", Take::Clamp(F32)),
    ("DATA", "bouncy_mult", Take::Clamp(F32)),
    ("VNAM", "", Take::OneOf(&[1.0, 2.0])),
];

/// EXPL numeric traits 0-7 [documented: SetExplosionTraitNumeric's table].
/// f32 fields but the sound level, 0-2 [documented: the sound levels of the projectile
/// page].
// The sound level wraps as a byte with no range (9 reads 9, 300 reads 44) [observed: jip7
// expl.settrait.7.9, jip7b expl.7.*].
const EXPL_NUMERIC: [(&str, Take); 8] = [
    ("force", Take::Clamp(F32)),
    ("damage", Take::Clamp(F32)),
    ("radius", Take::Clamp(F32)),
    ("is_radius", Take::Clamp(F32)),
    ("radiation_level", Take::Clamp(F32)),
    ("radiation_dissipation_time", Take::Clamp(F32)),
    ("radiation_radius", Take::Clamp(F32)),
    ("sound_level", Take::Wrap8),
];

/// IPCT numeric traits of SetImpactDataTraitNumeric, 0-14 [documented: its table]: the
/// impact data (DATA) 0-4, the decal data (DODT) 5-12; 13 the flags and 14 the colour are
/// worked out apart. GetImpactDataTrait numbers the same with the three forms (texture
/// set, sounds 1 and 2) at 5-7, so its 8-17 are these 5-14 [documented: its table].
/// The orientation and sound level are 0-2 [documented: the page's table], the passes a
/// u8; the rest f32 fields.
const IPCT_NUMERIC: [(&str, &str, (f64, f64)); 13] = [
    ("DATA", "effect_duration", F32),
    ("DATA", "effect_orientation", (0.0, 2.0)),
    ("DATA", "angle_threshold", F32),
    ("DATA", "placement_radius", F32),
    ("DATA", "sound_level", (0.0, 2.0)),
    ("DODT", "min_width", F32),
    ("DODT", "max_width", F32),
    ("DODT", "min_height", F32),
    ("DODT", "max_height", F32),
    ("DODT", "depth", F32),
    ("DODT", "shininess", F32),
    ("DODT", "parallax_scale", F32),
    ("DODT", "parallax_passes", U8),
];

/// IPCT form traits 0-2 [documented: SetImpactDataTraitForm's table].
const IPCT_FORMS: [&str; 3] = ["DNAM", "SNAM", "NAM1"];

/// WTHR cloud texture layers 0-3 [documented: Get/SetWeatherTexture, layers 0-3; the
/// fields: xEdit WTHR DNAM, CNAM, ANAM, BNAM].
const WEATHER_LAYERS: [&str; 4] = ["DNAM", "CNAM", "ANAM", "BNAM"];

/// ARMO BMDT general flags: bit 3 Medium, bit 7 Heavy [the bits: xEdit ARMO]; neither
/// is Light.
const MEDIUM: u64 = 1 << 3;
const HEAVY: u64 = 1 << 7;

impl Game<'_> {
    /// An ammunition's effects (RCIL), AddAmmoEffect's and RemoveAmmoEffect's changes
    /// first.
    pub fn ammo_effects(&self, ammo: FormId) -> Vec<FormId> {
        if let Some(v) = self.other.jip7.ammo_effects.get(&ammo) {
            return v.clone();
        }
        self.data.record_all(ammo, "RCIL").into_iter().filter_map(|v| json_form(Some(v))).collect()
    }

    /// A climate's weathers (WLST), ClimateAddWeatherType's and ClimateRemoveWeatherType's
    /// changes first.
    pub fn climate_weathers(&self, climate: FormId) -> Vec<ClimateWeather> {
        if let Some(v) = self.other.jip7.climate_weathers.get(&climate) {
            return v.clone();
        }
        let list = self.data.record_field(climate, "WLST", "").and_then(Json::as_array);
        list.into_iter()
            .flatten()
            .filter_map(|e| {
                let weather = json_form(e.get("weather"))?;
                Some(ClimateWeather { weather, chance: e.get("chance").and_then(Json::as_i64).unwrap_or(0), global: json_form(e.get("global")) })
            })
            .collect()
    }

    /// An armour's class: 1 light, 2 medium, 3 heavy (BMDT's general flags), SetArmorClass'
    /// change first [documented: SetArmorClass's table].
    pub fn armor_class(&self, armor: FormId) -> u8 {
        let flags = self.num_field(armor, "BMDT", "general_flags") as u64;
        if flags & HEAVY != 0 {
            3
        } else if flags & MEDIUM != 0 {
            2
        } else {
            1
        }
    }

    /// SetArmorConditionPenalty's multiplier, 1.0 until a script sets it [documented:
    /// "1.0, by default"].
    pub fn armor_condition_penalty(&self) -> f64 {
        self.other.jip7.armor_condition_penalty.unwrap_or(1.0)
    }

    /// SetWeatherTransitionTimeOverride's seconds, `None` without one [documented: "no
    /// arguments (or with 0) removes the override"].
    pub fn weather_transition_override(&self) -> Option<f64> {
        self.other.jip7.weather_transition
    }

    /// A record's number (subrecord `sub`, then `path`), with the session's changes; 0
    /// when it has none.
    pub fn num_field(&self, f: FormId, sub: &str, path: &str) -> f64 {
        crate::nvse::record_value(self, f, sub, path).and_then(|v| v.num().ok()).unwrap_or(0.0)
    }

    fn set_field(&mut self, f: FormId, sub: &str, path: &str, v: XVal) {
        self.field_overrides.insert((f, format!("{sub}/{path}")), v);
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let base = |g: &Game, f: FormId| crate::functions::base_of(g, f).unwrap_or(f);
    let of_sig = |g: &Game, f: Option<FormId>, sig: &[u8; 4]| f.filter(|&f| g.data.sig(f) == Some(*sig));
    let zero = XVal::Num(0.0);
    // The setters of a trait return 0 [hypothesis: as the record setters of batch 5 were
    // observed to, jip-records-5.md].
    let v = match name {
        // ---- ammunition
        "setammotraitnumeric" => {
            let t = usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| AMMO_NUMERIC.get(i));
            if let (Some(a), Some(&(sub, path, how))) = (of_sig(g, form(args, 0), b"AMMO"), t) {
                if let Some(v) = take(num(args, 2, f64::NAN), how) {
                    g.set_field(a, sub, path, XVal::Num(v));
                }
            }
            zero
        }
        // `ammo [form]`: left out, none [documented: SetAmmoProjectile, SetAmmoCasing].
        "setammoprojectile" | "setammocasing" => {
            if let Some(a) = of_sig(g, form(args, 0), b"AMMO") {
                let path = if name == "setammoprojectile" { "projectile" } else { "consumed_ammo" };
                g.set_field(a, "DAT2", path, XVal::Ref(form(args, 1).map_or(0, |f| f.0)));
            }
            zero
        }
        // `ammo effect`: 1 when the list changed [documented: `successful`; hypothesis: 0
        // for an effect already there, or not there to remove].
        "addammoeffect" | "removeammoeffect" => {
            let (Some(a), Some(e)) = (of_sig(g, form(args, 0), b"AMMO"), of_sig(g, form(args, 1), b"AMEF")) else { return Some(Ok(zero)) };
            let mut list = g.ammo_effects(a);
            let had = list.contains(&e);
            let changed = if name == "addammoeffect" { !had } else { had };
            if name == "addammoeffect" {
                if !had {
                    list.push(e);
                }
            } else {
                list.retain(|&x| x != e);
            }
            g.other.jip7.ammo_effects.insert(a, list);
            yes(changed)
        }
        "setammoeffecttraitnumeric" => {
            let t = usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| AMEF_NUMERIC.get(i));
            if let (Some(e), Some(&(path, how))) = (of_sig(g, form(args, 0), b"AMEF"), t) {
                if let Some(v) = take(num(args, 2, f64::NAN), how) {
                    g.set_field(e, "DATA", path, XVal::Num(v));
                }
            }
            zero
        }
        // ---- projectiles and explosions
        "setprojectiletraitnumeric" => {
            let t = usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| PROJ_NUMERIC.get(i));
            if let (Some(p), Some(&(sub, path, how))) = (of_sig(g, form(args, 0), b"PROJ"), t) {
                if let Some(v) = take(num(args, 2, f64::NAN), how) {
                    g.set_field(p, sub, path, XVal::Num(v));
                }
            }
            zero
        }
        // `projectile [light]`: left out, none [documented: SetProjectileLight].
        "setprojectilelight" => {
            if let Some(p) = of_sig(g, form(args, 0), b"PROJ") {
                g.set_field(p, "DATA", "light", XVal::Ref(form(args, 1).map_or(0, |f| f.0)));
            }
            zero
        }
        // `projectile path`: the muzzle flash model (NAM1) [documented:
        // SetProjectileMuzzleFlash; the field: xEdit PROJ].
        "setprojectilemuzzleflash" => {
            if let (Some(p), Some(path)) = (of_sig(g, form(args, 0), b"PROJ"), text(args, 1)) {
                g.set_field(p, "NAM1", "", XVal::Str(path));
            }
            zero
        }
        "setexplosiontraitnumeric" => {
            let t = usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| EXPL_NUMERIC.get(i));
            if let (Some(e), Some(&(path, how))) = (of_sig(g, form(args, 0), b"EXPL"), t) {
                if let Some(v) = take(num(args, 2, f64::NAN), how) {
                    g.set_field(e, "DATA", path, XVal::Num(v));
                }
            }
            zero
        }
        // ---- impact data
        // `ipct trait`: 0-4 the impact data, 5-7 the texture set and sounds, 8-15 the
        // decal data, 16 the flags (1 no decal data, 2 parallax, 4 alpha blending, 8 alpha
        // testing), 17 the colour as R × 1000000 + G × 1000 + B [documented:
        // GetImpactDataTrait's tables]; past them 0.
        "getimpactdatatrait" => {
            let Some(i) = of_sig(g, form(args, 0), b"IPCT") else { return Some(Ok(zero)) };
            match num(args, 1, -1.0) as i64 {
                t @ 0..=4 => XVal::Num(g.num_field(i, IPCT_NUMERIC[t as usize].0, IPCT_NUMERIC[t as usize].1)),
                t @ 5..=7 => XVal::Ref(match crate::nvse::record_value(g, i, IPCT_FORMS[(t - 5) as usize], "") {
                    Some(XVal::Ref(r)) => r,
                    _ => 0,
                }),
                t @ 8..=15 => {
                    let (sub, path, _) = IPCT_NUMERIC[(t - 3) as usize];
                    if crate::nvse::record_value(g, i, "DODT", path).is_none() {
                        // An impact without decal data reads these: widths and heights 8 to
                        // 32, depth 32, shininess 4, parallax scale 1 and 4 passes [observed:
                        // jip7 ipct.trait.8-15 on BaseBallBatMetalImpact, which has no DODT].
                        XVal::Num([8.0, 32.0, 8.0, 32.0, 32.0, 4.0, 1.0, 4.0][(t - 8) as usize])
                    } else {
                        XVal::Num(g.num_field(i, sub, path))
                    }
                }
                16 => XVal::Num(g.other.jip7.impact_flags.get(&i).map_or(impact_flags(g, i) as f64, |&f| f64::from(f))),
                17 => XVal::Num(impact_colour(g, i)),
                _ => zero,
            }
        }
        "setimpactdatatraitnumeric" => {
            let Some(i) = of_sig(g, form(args, 0), b"IPCT") else { return Some(Ok(zero)) };
            let v = num(args, 2, f64::NAN);
            match num(args, 1, -1.0) as i64 {
                t @ 0..=12 => {
                    let (sub, path, range) = IPCT_NUMERIC[t as usize];
                    if let Some(v) = fit(v, range) {
                        g.set_field(i, sub, path, XVal::Num(v));
                    }
                }
                // The four flag bits.
                // Kept whole, to nine bits (99 reads 99, 256 reads 256, -1 reads 511), and read
                // back as set [observed: jip7 ipct.settrait.13.99, jip7b ipct.flags.*]; the four
                // known bits reach the records.
                13 => {
                    let Some(f) = take(v, Take::Int) else { return Some(Ok(zero)) };
                    let f = (f as i64 as u64) & 0x1FF;
                    g.other.jip7.impact_flags.insert(i, f as u32);
                    let f = f & 15;
                    let data = g.num_field(i, "DATA", "flags") as u64;
                    g.set_field(i, "DATA", "flags", XVal::Num(((data & !1) | (f & 1)) as f64));
                    let decal = g.num_field(i, "DODT", "flags") as u64;
                    g.set_field(i, "DODT", "flags", XVal::Num(((decal & !7) | ((f >> 1) & 7)) as f64));
                }
                14 => {
                    // past 0-255255255 nothing is set [observed: jip7b ipct.setcolour.255255256, .m1]
                    let Some(c) = take(v, Take::Int).filter(|c| (0.0..=255_255_255.0).contains(c)) else { return Some(Ok(zero)) };
                    let c = c as u64;
                    for (k, p) in [(c / 1_000_000) % 1000, (c / 1000) % 1000, c % 1000].into_iter().zip(["color.red", "color.green", "color.blue"]) {
                        // a channel past 255 is kept as given (300 reads 300) [observed: jip7b
                        // ipct.setcolour.300]
                        g.set_field(i, "DODT", p, XVal::Num(k as f64));
                    }
                }
                _ => {}
            }
            zero
        }
        "setimpactdatatraitform" => {
            let t = usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| IPCT_FORMS.get(i));
            // A form of the wrong type (a creature for a sound) sets none [observed: jip7
            // ipct.setform.sound1; a sound is taken, jip7b ipct.setform.sound].
            if let (Some(i), Some(sub)) = (of_sig(g, form(args, 0), b"IPCT"), t) {
                let want: &[u8; 4] = if *sub == "DNAM" { b"TXST" } else { b"SOUN" };
                g.set_field(i, sub, "", XVal::Ref(of_sig(g, form(args, 2), want).map_or(0, |f| f.0)));
            }
            zero
        }
        // `weapon dataset`: WEAP INAM [documented: SetImpactDataSet; the field: xEdit WEAP].
        "setimpactdataset" => {
            if let Some(w) = of_sig(g, form(args, 0), b"WEAP") {
                g.set_field(w, "INAM", "", XVal::Ref(of_sig(g, form(args, 1), b"IPDS").map_or(0, |f| f.0)));
            }
            zero
        }
        // `[baseActor]`, else the calling reference's base: NAM4 0-11 [documented:
        // Get/SetImpactMaterialType's table; the field: xEdit NPC_/CREA]; not an actor 0.
        "getimpactmaterialtype" => {
            let a = form(args, 0).or(subject).map(|f| base(g, f)).filter(|&a| matches!(g.data.sig(a).as_ref(), Some(b"NPC_" | b"CREA")));
            XVal::Num(a.map_or(0.0, |a| g.num_field(a, "NAM4", "")))
        }
        "setimpactmaterialtype" => {
            let a = form(args, 1).or(subject).map(|f| base(g, f)).filter(|&a| matches!(g.data.sig(a).as_ref(), Some(b"NPC_" | b"CREA")));
            let m = num(args, 0, -1.0);
            if let Some(a) = a.filter(|_| (0.0..=11.0).contains(&m)) {
                g.set_field(a, "NAM4", "", XVal::Num(m.trunc()));
            }
            zero
        }
        // ---- weathers and climates
        // The precipitation model (MODL) and the cloud layers 0-3 [documented: the pages].
        "getweatherprecipitationmodel" | "getweathertexture" => {
            let w = of_sig(g, form(args, 0), b"WTHR");
            let sub = if name == "getweathertexture" { usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| WEATHER_LAYERS.get(i)).copied() } else { Some("MODL") };
            XVal::Str(match w.zip(sub).and_then(|(w, sub)| crate::nvse::record_value(g, w, sub, "")) {
                Some(XVal::Str(s)) => s,
                _ => String::new(),
            })
        }
        "setweatherprecipitationmodel" | "setweathertexture" => {
            let w = of_sig(g, form(args, 0), b"WTHR");
            let (sub, k) = if name == "setweathertexture" { (usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| WEATHER_LAYERS.get(i)).copied(), 2) } else { (Some("MODL"), 1) };
            if let (Some(w), Some(sub), Some(path)) = (w, sub, text(args, k)) {
                g.set_field(w, sub, "", XVal::Str(path));
            }
            zero
        }
        // `[seconds]`: 0 or none removes the override [documented].
        "setweathertransitiontimeoverride" => {
            // NaN or infinity changes nothing.
            if let Some(s) = field_number(num(args, 0, 0.0), 0.0, F32.1) {
                g.other.jip7.weather_transition = (s > 0.0).then_some(s);
            }
            zero
        }
        // `climate weather chance [global]`: a weather already listed takes the chance and
        // global [documented: ClimateAddWeatherType]; 1 when done [documented:
        // `successful`].
        "climateaddweathertype" => {
            let (Some(c), Some(w)) = (of_sig(g, form(args, 0), b"CLMT"), of_sig(g, form(args, 1), b"WTHR")) else { return Some(Ok(zero)) };
            // The chance is 0-100 [documented: the page's `0-100`].
            let Some(chance) = fit(num(args, 2, f64::NAN), (0.0, 100.0)) else { return Some(Ok(zero)) };
            let entry = ClimateWeather { weather: w, chance: chance as i64, global: of_sig(g, form(args, 3), b"GLOB") };
            let mut list = g.climate_weathers(c);
            match list.iter_mut().find(|e| e.weather == w) {
                Some(e) => *e = entry,
                None => list.push(entry),
            }
            g.other.jip7.climate_weathers.insert(c, list);
            yes(true)
        }
        "climateremoveweathertype" => {
            let (Some(c), Some(w)) = (of_sig(g, form(args, 0), b"CLMT"), form(args, 1)) else { return Some(Ok(zero)) };
            let mut list = g.climate_weathers(c);
            let had = list.iter().any(|e| e.weather == w);
            list.retain(|e| e.weather != w);
            g.other.jip7.climate_weathers.insert(c, list);
            yes(had)
        }
        // ---- armour
        // `armor class`: 1 light, 2 medium, 3 heavy [documented: SetArmorClass's table].
        "setarmorclass" => {
            let class = num(args, 1, 0.0) as i64;
            if let Some(a) = of_sig(g, form(args, 0), b"ARMO").filter(|_| (1..=3).contains(&class)) {
                let flags = g.num_field(a, "BMDT", "general_flags") as u64 & !(MEDIUM | HEAVY);
                let flags = flags | if class == 2 { MEDIUM } else if class == 3 { HEAVY } else { 0 };
                g.set_field(a, "BMDT", "general_flags", XVal::Num(flags as f64));
            }
            zero
        }
        // `armor path`: the ragdoll constraint template (BMCT) [documented: SetArmorRCT].
        "setarmorrct" => {
            if let (Some(a), Some(path)) = (of_sig(g, form(args, 0), b"ARMO"), text(args, 1)) {
                g.set_field(a, "BMCT", "", XVal::Str(path));
            }
            zero
        }
        // `[multiplier]`: sets it, and returns it; left out, returns it unchanged
        // [documented: SetArmorConditionPenalty]. 0-2: past 2 the formula's mult would go
        // below zero at 0% condition [hypothesis: the page's formula,
        // `1.0 - (0.5 - condition) * multiplier`].
        // Both return the value before the call; the penalty takes any number (3 and -1),
        // the share only 0-1 (2 and -1 set nothing) [observed: jip7 armor.condpenalty,
        // armor.maxpercent; jip7b armor.*].
        "setarmorconditionpenalty" => {
            let was = g.armor_condition_penalty();
            if let Some(m) = args.first().and_then(|v| v.num().ok()).filter(|m| m.is_finite()) {
                g.other.jip7.armor_condition_penalty = Some(m);
            }
            XVal::Num(was)
        }
        // `[maxPercent]` 0-1, the same way [documented: SetDamageToArmorMaxPercent]; 1
        // (no cap) until a script sets it [hypothesis: the page gives no default].
        "setdamagetoarmormaxpercent" => {
            let was = g.other.jip7.damage_to_armor_max.unwrap_or(1.0);
            if let Some(m) = args.first().and_then(|v| v.num().ok()).filter(|m| (0.0..=1.0).contains(m)) {
                g.other.jip7.damage_to_armor_max = Some(m);
            }
            XVal::Num(was)
        }
        _ => return None,
    };
    Some(Ok(v))
}

/// An impact data's flags as GetImpactDataTrait 16 gives them: 1 No Decal Data (DATA's
/// flag bit 0), 2 Parallax, 4 Alpha Blending, 8 Alpha Testing (DODT's flag bits 0-2)
/// [documented: the page's table; the bits: xEdit IPCT DATA and the decal data].
fn impact_flags(g: &Game, i: FormId) -> u64 {
    let data = g.num_field(i, "DATA", "flags") as u64 & 1;
    let decal = g.num_field(i, "DODT", "flags") as u64 & 7;
    data | (decal << 1)
}

/// The decal colour as R × 1000000 + G × 1000 + B [documented: "Color (RRRGGGBBB)"].
fn impact_colour(g: &Game, i: FormId) -> f64 {
    let c: Vec<f64> = ["color.red", "color.green", "color.blue"].iter().map(|p| g.num_field(i, "DODT", p)).collect();
    c[0] * 1_000_000.0 + c[1] * 1000.0 + c[2]
}
