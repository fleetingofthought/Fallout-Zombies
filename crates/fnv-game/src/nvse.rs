//! xNVSE's (and FOSE's) functions that read or change the game, called by name from the
//! script VM through `Host::ext_call` (the remaining-xNVSE phase,
//! docs/handoff/xnvse-remaining.md). The VM keeps the pure ones (strings, arrays, math);
//! these need the game's records, inventories and lists. Behaviour from the wiki export's
//! pages [documented: <page>], record fields from the Layer 1 schema; the rest is marked.

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

/// A record field of `form`: a number, a form or a string (`GameData::record_field`),
/// or what a setter put there this session.
fn field(g: &Game, form: FormId, sub: &str, path: &str) -> Option<XVal> {
    use serde_json::Value as Json;
    if let Some(v) = g.field_overrides.get(&(form, format!("{sub}/{path}"))) {
        return Some(v.clone());
    }
    // A clone reads its original's fields (TempCloneForm).
    let form = g.unclone(form);
    if let Some(v) = g.field_overrides.get(&(form, format!("{sub}/{path}"))) {
        return Some(v.clone());
    }
    Some(match g.data.record_field(form, sub, path)? {
        Json::Number(n) => XVal::Num(record_float(n.as_f64()?)),
        Json::String(s) if s.starts_with("0x") => XVal::Ref(u32::from_str_radix(&s[2..], 16).ok()?),
        Json::String(s) => XVal::Str(s.clone()),
        _ => return None,
    })
}

/// A record's number as the game reads it: a float32 field by its shortest decimal (1.3,
/// not 1.29999995), so that `x * 1000` floors to 1300 [observed: gaps9, Floor of
/// GetWeaponAnimAttackMult × 1000 on the machete (1.3 in the record) reads 1300].
pub(crate) fn record_float(x: f64) -> f64 {
    let f = x as f32;
    if x.fract() != 0.0 && f64::from(f) == x {
        format!("{f}").parse().unwrap_or(x)
    } else {
        x
    }
}

/// A number a script hands a record setter, made fit for the field it goes to: `None`
/// for NaN or infinity (nothing is set), else clamped to `lo..=hi`, the field's range
/// [hypothesis: the game keeps its record fields in range; NVForever never stores a value
/// a field cannot hold]. Every record setter of the extender batches uses it.
pub(crate) fn field_number(v: f64, lo: f64, hi: f64) -> Option<f64> {
    v.is_finite().then(|| v.clamp(lo, hi))
}

/// The range of an f32 field.
pub(crate) const F32: (f64, f64) = (-(f32::MAX as f64), f32::MAX as f64);
/// The range of a u8 field.
pub(crate) const U8: (f64, f64) = (0.0, 255.0);
/// The range of a u32 field.
pub(crate) const U32: (f64, f64) = (0.0, u32::MAX as f64);

/// [`field`] for the sibling files.
pub(crate) fn record_value(g: &Game, form: FormId, sub: &str, path: &str) -> Option<XVal> {
    field(g, form, sub, path)
}

fn num_of(v: Option<XVal>) -> XVal {
    match v {
        Some(XVal::Num(x)) => XVal::Num(x),
        _ => XVal::Num(0.0),
    }
}

fn form_of(v: Option<XVal>) -> XVal {
    match v {
        Some(XVal::Ref(r)) => XVal::Ref(r),
        _ => XVal::Ref(0),
    }
}

fn as_form(v: Option<&XVal>) -> Option<FormId> {
    match v? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn int_of(v: Option<&XVal>, dflt: i64) -> i64 {
    match v {
        Some(XVal::Num(x)) => *x as i64,
        _ => dflt,
    }
}

/// The WEAP fields of the GetWeapon… getters [documented: each page names the trait;
/// the field is the schema's of that name, schemas/fnv].
fn weapon_field(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "getweaponcliprounds" => ("DATA", "clip_size"),
        "getattackdamage" => ("DATA", "base_damage"),
        "getweaponammo" => ("NAM0", ""),
        "getweaponammouse" => ("DNAM", "ammo_use"),
        "getweaponactionpoints" => ("DNAM", "override_action_points"),
        "getweaponaimarc" => ("DNAM", "aim_arc"),
        "getweaponanimattackmult" => ("DNAM", "animation_attack_multiplier"),
        "getweaponanimjamtime" => ("DNAM", "jam_time"),
        "getweaponanimmult" => ("DNAM", "animation_multiplier"),
        "getweaponanimreloadtime" => ("DNAM", "reload_time"),
        "getweaponanimshotspersec" => ("DNAM", "attack_shots_per_sec"),
        "getweaponattackanimation" => ("DNAM", "attack_animation"),
        "getweaponbasevatschance" => ("DNAM", "base_vats_to_hit_chance"),
        "getweaponcritchance" => ("CRDT", "crit_percent_mult"),
        "getweaponcritdamage" => ("CRDT", "critical_damage"),
        "getweaponcriteffect" => ("CRDT", "effect"),
        "getweaponfiredelaymin" => ("DNAM", "semi_automatic_fire_delay_min"),
        "getweaponfiredelaymax" => ("DNAM", "semi_automatic_fire_delay_max"),
        "getweaponfirerate" => ("DNAM", "fire_rate"),
        "getweaponflags1" => ("DNAM", "flags1"),
        "getweaponflags2" => ("DNAM", "flags2"),
        "getweaponhandgrip" => ("DNAM", "grip_animation"),
        "getweaponlimbdamagemult" => ("DNAM", "limb_dmg_mult"),
        "getweaponmaxrange" => ("DNAM", "max_range"),
        "getweaponminrange" => ("DNAM", "min_range"),
        "getweaponminspread" => ("DNAM", "min_spread"),
        "getweaponnumprojectiles" => ("DNAM", "projectile_count"),
        "getweaponprojectile" => ("DNAM", "projectile"),
        "getweaponreach" => ("DNAM", "reach"),
        "getweaponregenrate" => ("DNAM", "regen_rate"),
        "getweaponreloadanim" => ("DNAM", "reload_animation"),
        "getweaponrequiredskill" => ("DNAM", "skill_req"),
        "getweaponrequiredstrength" => ("DNAM", "strength_req"),
        "getweaponresisttype" => ("DNAM", "resist_type"),
        "getweaponrumbleduration" => ("DNAM", "rumble_duration"),
        "getweaponrumbleleftmotor" => ("DNAM", "rumble_left_motor_strength"),
        "getweaponrumblerightmotor" => ("DNAM", "rumble_right_motor_strength"),
        "getweaponrumblewavelength" => ("DNAM", "rumble_wavelength"),
        "getweaponsightfov" => ("DNAM", "sight_fov"),
        "getweaponsightusage" => ("DNAM", "sight_usage"),
        "getweaponskill" => ("DNAM", "skill"),
        "getweaponspread" => ("DNAM", "spread"),
        "getweapontype" => ("DNAM", "animation_type"),
        _ => return None,
    })
}

impl Game<'_> {
    /// The members of a form list, with the session's ListAddForm/ListRemove… edits.
    pub(crate) fn edited_list(&mut self, list: FormId) -> &mut Vec<FormId> {
        if !self.list_edits.contains_key(&list) {
            let now = crate::functions::list_members(self, list);
            self.list_edits.insert(list, now);
        }
        self.list_edits.get_mut(&list).expect("inserted above")
    }
}

/// An xNVSE function by catalogue name (lower case); `None` when it is not built here.
/// `on` is the reference it was called on, else the calling reference.
pub fn call(g: &mut Game, ctx: &RunCtx, name: &str, on: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    if let Some(r) = crate::delayed::call(g, ctx, name, on, args) {
        return Some(r);
    }
    if let Some(v) = crate::script_input::call(&mut g.input, name, args) {
        // EnableKey frees the key whatever holds it, so DisableKeyEx's mask groups on it
        // go too; else a later EnableKeyEx would find a stale group and keep the key
        // disabled [hypothesis: ITR's groups sit on top of DisableKey's one switch].
        if name == "enablekey" {
            if let Some(k) = args.first().and_then(|a| a.num().ok()).and_then(|c| u16::try_from(c as i64).ok()) {
                g.other.itr.key_masks.remove(&k);
            }
        }
        return Some(Ok(v));
    }
    let subject = on.or(ctx.self_ref);
    if let Some(r) = crate::nvse_lutana::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_ttw::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_refs::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_misc::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_data::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_notes::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_nx::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_actor::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_items::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_amata::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jip::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_other::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jg1::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jg2::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jg3::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jg4::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jg5::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_sup1::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_dialogue::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_sup3::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_sup5::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jip5::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jip6::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jip7::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jip8::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_pp::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_showoff2::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_showoff3::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_showoff4::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_anh::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_itr1::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_itr2::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_knvse::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jippp::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_vheyes::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_small::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_one111::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_rest::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_console::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_lists::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_leveled::call(g, name, args) {
        return Some(r);
    }
    if let Some(r) = crate::mod_files::call(g, name, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_proj::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_sound::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_traits::call(g, name, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_anim::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nif_blocks::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jip2::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::anims::call_ext(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_showoff::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_hud::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jip3::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_jip4::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_gaps::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_plugins::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::script_runner::call(g, name, args) {
        return Some(r);
    }
    // The UI functions, over the menus' tile tree (docs/design/ui-tile-tree.md §5.1);
    // before the `set…` setters below, which would read SetUIFloat as a record field's
    // setter.
    if let Some(r) = crate::ui_functions::call(g, name, args) {
        return Some(r);
    }
    // The rest of the tile functions (task T9; docs/spec-notes/menu-xml.md §9).
    if let Some(r) = crate::ui_more::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_settings::call(g, name, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_tokens::call(g, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_handler_lists::call(g, name, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_debug::call(g, name, args) {
        return Some(r);
    }
    // The item a getter reads: its argument, else the calling reference's base form.
    let item = |g: &Game, k: usize| -> Option<FormId> {
        as_form(args.get(k)).or(subject).map(|f| crate::functions::base_of(g, f).unwrap_or(f))
    };
    // Setters: `Set… value [form]` (SetBaseItemValue: `form value`) change the field the
    // matching getter reads, for the session [documented: their pages; hypothesis: not
    // saved, and only the getters see the change, not combat or the menus].
    if let Some(rest) = name.strip_prefix("set") {
        let get = format!("get{rest}");
        let (value, form_at) = if name == "setbaseitemvalue" { (args.get(1), 0) } else { (args.first(), 1) };
        let f = item(g, form_at).unwrap_or(FormId(0));
        let target = match get.as_str() {
            "getbaseitemvalue" => Some(("DATA", "value")),
            "getweight" if g.data.sig(f) == Some(*b"AMMO") => Some(("DAT2", "weight")),
            "getweight" if g.data.sig(f) == Some(*b"ALCH") => Some(("DATA", "")),
            "getweight" => Some(("DATA", "weight")),
            "getarmorarmorrating" => Some(("DNAM", "dr")),
            "getarmordt" => Some(("DNAM", "dt")),
            "getammoconsumedpercent" => Some(("DAT2", "consumed_percentage")),
            // [documented: SetHairLength; SetNPCWeight] the NPC base's LNAM and NAM7.
            "gethairlength" => Some(("LNAM", "")),
            "getnpcweight" => Some(("NAM7", "")),
            // [observed: nvse-setters, SetObjectHealth 123 then GetBaseHealth 123].
            "getobjecthealth" => Some(("DATA", "health")),
            // A creature's attack damage is not set [observed: atk1, SetAttackDamage 50 on
            // VStreetCrBrahmin2 still reads 0, though the page names creatures].
            "getattackdamage" if g.data.sig(f) == Some(*b"CREA") => return Some(Ok(XVal::Num(0.0))),
            g2 => weapon_field(g2),
        };
        if let (Some((sub, path)), Some(v)) = (target, value) {
            g.field_overrides.insert((f, format!("{sub}/{path}")), v.clone());
            return Some(Ok(XVal::Num(0.0)));
        }
        // The flag setters: one bit of DNAM's flags.
        let bit = match name {
            "setweaponisautomatic" => Some(("flags1", 1)),
            "setweaponlongbursts" => Some(("flags2", 11)),
            _ => None,
        };
        if let (Some((path, bit)), Some(v)) = (bit, value) {
            let now = match field(g, f, "DNAM", path) {
                Some(XVal::Num(x)) => x as u64,
                _ => 0,
            };
            let new = if v.truthy() { now | (1 << bit) } else { now & !(1 << bit) };
            g.field_overrides.insert((f, format!("DNAM/{path}")), XVal::Num(new as f64));
            return Some(Ok(XVal::Num(0.0)));
        }
        // [documented: SetName] the base form's name.
        if name == "setname" {
            let text = match value {
                Some(XVal::Str(t)) => t.clone(),
                _ => String::new(),
            };
            g.name_overrides.insert(f, text);
            return Some(Ok(XVal::Num(0.0)));
        }
    }
    if let Some((sub, path)) = weapon_field(name) {
        let w = item(g, 0).unwrap_or(FormId(0));
        // A creature reads 0 [observed: atk1, VStreetCrBrahmin2; the page says it works on
        // creatures].
        if name == "getattackdamage" && g.data.sig(w) == Some(*b"CREA") {
            return Some(Ok(XVal::Num(0.0)));
        }
        let v = field(g, w, sub, path);
        return Some(Ok(match name {
            "getweaponammo" | "getweaponprojectile" | "getweaponcriteffect" => form_of(v),
            // An index into the page's own list, not the record's code [documented:
            // GetWeaponAttackAnimation; observed: nvse-forms, pistol 9, hunting rifle 1].
            "getweaponattackanimation" => XVal::Num(match num_of(v) {
                XVal::Num(c) => attack_animation_index(c as i64),
                _ => 0.0,
            }),
            // The record's DEFAULT (255) reads 0 [observed: nvse-forms]; HandGrip1–6 as 1–6
            // [hypothesis: the page's grip table is not in the export].
            "getweaponhandgrip" => XVal::Num(match num_of(v) {
                XVal::Num(c) if (230.0..=235.0).contains(&c) => c - 229.0,
                _ => 0.0,
            }),
            // None (-1) reads as unsigned [observed: nvse-forms, 4294967295].
            "getweaponresisttype" => match num_of(v) {
                XVal::Num(x) if x < 0.0 => XVal::Num(x + 4_294_967_296.0),
                other => other,
            },
            _ => num_of(v),
        }));
    }
    let v = match name {
        // [documented: GetWeaponIsAutomatic, GetWeaponHasScope, GetWeaponLongBursts] flag
        // bits of DNAM (schema WeaponDNAM: flags1 bit 1 Is Automatic, bit 2 Has Scope;
        // flags2 bit 11 Long Burst).
        "getweaponisautomatic" | "getweaponhasscope" | "getweaponlongbursts" => {
            let w = item(g, 0).unwrap_or(FormId(0));
            let (sub, bit) = match name {
                "getweaponisautomatic" => ("flags1", 1),
                "getweaponhasscope" => ("flags1", 2),
                _ => ("flags2", 11),
            };
            let bits = match field(g, w, "DNAM", sub) {
                Some(XVal::Num(x)) => x as u64,
                _ => 0,
            };
            XVal::Num(f64::from(u8::from(bits & (1 << bit) != 0)))
        }
        // [documented: GetWeight; GetItemValue; GetBaseHealth; GetArmorArmorRating;
        // GetArmorDT] the item's DATA / DNAM fields; ALCH keeps its weight in DATA and
        // its value in ENIT.
        "getweight" | "getitemvalue" | "getbasehealth" | "getarmorarmorrating" | "getarmordt" => {
            let f = item(g, 0).unwrap_or(FormId(0));
            let alch = g.data.sig(f) == Some(*b"ALCH");
            // [observed: nvse-forms, Stimpak value 0] an ALCH's value (ENIT) is not what
            // GetItemValue reads.
            if alch && name == "getitemvalue" {
                return Some(Ok(XVal::Num(0.0)));
            }
            let (sub, path) = match (name, alch) {
                ("getweight", true) => ("DATA", ""),
                // AMMO keeps its weight in DAT2 [observed: nvse-forms, 10mm ammo 0.033].
                ("getweight", false) if g.data.sig(f) == Some(*b"AMMO") => ("DAT2", "weight"),
                ("getweight", false) => ("DATA", "weight"),
                ("getitemvalue", false) => ("DATA", "value"),
                ("getbasehealth", _) => ("DATA", "health"),
                ("getarmorarmorrating", _) => ("DNAM", "dr"),
                _ => ("DNAM", "dt"),
            };
            num_of(field(g, f, sub, path))
        }
        // [documented: GetAmmoSpeed; GetAmmoConsumedPercent; GetAmmoCasing].
        "getammospeed" | "getammoconsumedpercent" | "getammocasing" => {
            let a = item(g, 0).unwrap_or(FormId(0));
            match name {
                "getammospeed" => num_of(field(g, a, "DATA", "speed")),
                "getammoconsumedpercent" => num_of(field(g, a, "DAT2", "consumed_percentage")),
                _ => form_of(field(g, a, "DAT2", "consumed_ammo")),
            }
        }
        // [documented: GetName] the full name of a form; on a reference always its
        // base's, neither the reference's own FULL nor SetRefName's name; a static
        // without one reads "<no name>" [observed: plugins3, the Goodsprings map marker
        // and the 605 references round the player; gaps4, Doc after SetRefName].
        "getname" => {
            let f = as_form(args.first()).or(subject).unwrap_or(FormId(0));
            let named = |x: FormId| g.name_overrides.get(&x).or_else(|| g.data.full_names.get(&g.unclone(x))).cloned();
            let b = crate::functions::base_of(g, f).filter(|&b| b != f).unwrap_or(f);
            let n = named(b).or_else(|| (g.data.sig(b) == Some(*b"STAT")).then(|| "<no name>".to_string()));
            XVal::Str(n.unwrap_or_default())
        }
        // [documented: GetParentCell] the cell the reference is in.
        "getparentcell" => XVal::Ref(subject.and_then(|r| g.cell_of(r)).map_or(0, |c| c.0)),
        // [documented: GetInventoryObject] the base form of the container's nth kind of
        // item, in FormID order [hypothesis: the order is not documented]; 0 past the end.
        "getinventoryobject" => {
            let c = subject.unwrap_or(FormId(0));
            let n = int_of(args.first(), 0);
            let items: Vec<FormId> = g.world.inventory_mut(c).map(|inv| inv.iter().filter(|(_, k)| *k > 0).map(|(i, _)| i).collect()).unwrap_or_default();
            XVal::Ref(usize::try_from(n).ok().and_then(|n| items.get(n)).map_or(0, |f| f.0))
        }
        // [documented: GetEquippedObject] the item in an equipment slot: 5 the weapon,
        // otherwise the worn item whose biped flags (BMDT) include the slot.
        "getequippedobject" => {
            let a = subject.unwrap_or(FormId(0));
            let slot = int_of(args.first(), -1);
            if slot == 5 {
                XVal::Ref(crate::functions::items::equipped_weapon(g, a).map_or(0, |w| w.0))
            } else {
                let worn = g.world.inventory_mut(a).map(|inv| inv.equipped()).unwrap_or_default();
                let hit = worn.into_iter().find(|&i| match field(g, i, "BMDT", "biped_flags") {
                    Some(XVal::Num(bits)) => (0..32).contains(&slot) && (bits as u64) & (1 << slot) != 0,
                    _ => false,
                });
                XVal::Ref(hit.map_or(0, |f| f.0))
            }
        }
        // Variables by name of a reference's or a quest's script [documented:
        // GetVariable; SetVariable; HasVariable; GetRefVariable; SetRefVariable;
        // GetStringVariable; SetStringVariable; GetArrayVariable]: the parent is the last
        // argument, else the calling reference.
        "getvariable" | "getrefvariable" | "hasvariable" | "getstringvariable" | "getarrayvariable" | "setvariable" | "setrefvariable" | "setstringvariable" => {
            let setter = name.starts_with("set");
            let var = match args.first() {
                Some(XVal::Str(v)) => v.clone(),
                _ => String::new(),
            };
            let parent = as_form(args.get(if setter { 2 } else { 1 })).or(subject).unwrap_or(FormId(0));
            let slot = g.instance_of(parent).and_then(|(key, script)| Some((key, script, g.data.script(script)?.var_index(&var)?)));
            let Some((key, script, i)) = slot else {
                return Some(Ok(match name {
                    "getstringvariable" => XVal::Str(String::new()),
                    "getrefvariable" => XVal::Ref(0),
                    "getarrayvariable" => XVal::Array(0),
                    _ => XVal::Num(0.0),
                }));
            };
            let now = g.var(key, i);
            match name {
                "hasvariable" => XVal::Num(1.0),
                // A ref variable reads as its form ID's bits taken as a double [observed:
                // nvse-vars, the player gives 9.88131e-323].
                "getvariable" => XVal::Num(match now {
                    fnv_script::vm::Value::Ref(f) => f64::from_bits(u64::from(f)),
                    fnv_script::vm::Value::Num(x) => x,
                }),
                "getrefvariable" => XVal::Ref(now.as_form().unwrap_or(0)),
                "getstringvariable" => XVal::Str(g.ext.text(now.num() as u32).unwrap_or_default().to_string()),
                "getarrayvariable" => XVal::Array(now.num() as u32),
                "setvariable" => {
                    let v = match args.get(1) {
                        Some(XVal::Num(x)) => *x,
                        _ => 0.0,
                    };
                    // Stored as given, an int variable too [observed: nvse-vars, 2.75 reads
                    // back 2.75].
                    g.store_raw(key, Some(script), i, fnv_script::vm::Value::Num(v));
                    XVal::Num(0.0)
                }
                "setrefvariable" => {
                    let v = as_form(args.get(1)).map_or(0, |f| f.0);
                    g.store(key, Some(script), i, fnv_script::vm::Value::Ref(v), "SetRefVariable", false);
                    XVal::Num(0.0)
                }
                _ => {
                    let text = match args.get(1) {
                        Some(XVal::Str(t)) => t.clone(),
                        _ => String::new(),
                    };
                    let id = now.num() as u32;
                    if g.ext.text(id).is_some() {
                        g.ext.set_text(id, text);
                    } else {
                        let id = g.ext.new_string(text);
                        g.store(key, Some(script), i, fnv_script::vm::Value::Num(f64::from(id)), "SetStringVariable", false);
                    }
                    XVal::Num(0.0)
                }
            }
        }
        // [documented: GetScript; IsScripted] the base form's script.
        "getscript" | "isscripted" => {
            let f = item(g, 0).unwrap_or(FormId(0));
            let s = g.base_script(f);
            if name == "getscript" { XVal::Ref(s.map_or(0, |s| s.0)) } else { XVal::Num(f64::from(u8::from(s.is_some()))) }
        }
        // [documented: GetRace; observed: nvse-forms, Doc Mitchell 0042C3, his RNAM though
        // he has a template (not using Traits); tmpl2 (#1057), PrimmNCRTrooper3 answers its
        // "Use Traits" template's race; tmpl3, a placed reference whose Traits come from a
        // leveled list answers the race of the actor the list gave it] the actor's race
        // record, as the character creator or AgeRace changed it.
        "getrace" => {
            // The argument first: `GetRace rF` on a base form [observed: nvse-forms].
            let a = as_form(args.first()).or(subject).unwrap_or(FormId(0));
            // A base form: its Traits template's through NPC records; a chain ending at a
            // list keeps the record's own (actors.md §2.2).
            let own = crate::functions::base_of(g, a)
                .is_none()
                .then(|| match field(g, g.data.base_template_source(a, |d| d.template_traits), "RNAM", "") {
                    Some(XVal::Ref(r)) if r != 0 => Some(FormId(r)),
                    _ => None,
                })
                .flatten();
            XVal::Ref(own.or_else(|| g.actor_race(a)).map_or(0, |r| r.0))
        }
        // [documented: GetNthModName] the plugin's file name at a load-order index.
        "getnthmodname" => XVal::Str(usize::try_from(int_of(args.first(), -1)).ok().and_then(|i| g.data.load_order.get(i).cloned()).unwrap_or_default()),
        "getnumloadedmods" => XVal::Num(g.data.load_order.len() as f64),
        // [documented: GetEquipType; observed: nvse-forms, 10mm pistol 2, leather armor 7,
        // stimpak 11, 10mm ammo 0] the item's ETYP.
        "getequiptype" => num_of(field(g, item(g, 0).unwrap_or(FormId(0)), "ETYP", "")),
        // [documented: GetRepairList; GetModelPath; GetIconPath].
        "getrepairlist" => form_of(field(g, item(g, 0).unwrap_or(FormId(0)), "REPL", "")),
        // GetTexturePath: the icon (ICON) [observed: nvse-misc, pistol, stimpak, caps, the
        // Action Boy perk].
        "getmodelpath" | "geticonpath" | "gettexturepath" => {
            let f = item(g, 0).unwrap_or(FormId(0));
            let sub = if name == "getmodelpath" { "MODL" } else { "ICON" };
            match field(g, f, sub, "").or_else(|| field(g, f, sub, "model")) {
                Some(XVal::Str(t)) => XVal::Str(t),
                _ => XVal::Str(String::new()),
            }
        }
        // The actor base form [documented: GetBaseNumFactions; GetBaseNthFaction;
        // GetBaseNthRank; GetHair; GetEyes; GetHairLength; GetHairColor; GetNPCHeight;
        // GetNPCWeight; GetActorBaseFlagsLow/High]: the argument, else the calling
        // reference's base.
        "getbasenumfactions" | "getbasenthfaction" | "getbasenthrank" => {
            let n = int_of(args.first(), 0);
            let a = if name == "getbasenumfactions" { item(g, 0) } else { item(g, 1) }.unwrap_or(FormId(0));
            // Counted from the record's last SNAM back [observed: nvse-actors, Sunny's
            // and Doc's factions]; the Use Factions template's [observed: o5, tmpl3 (#1057)].
            let src = g.data.base_template_source(a, |d| d.template_factions);
            let mut f = g.data.actors.get(&src).map(|d| d.factions.clone()).unwrap_or_default();
            f.reverse();
            let nth = usize::try_from(n).ok().and_then(|n| f.get(n));
            match name {
                "getbasenumfactions" => XVal::Num(f.len() as f64),
                "getbasenthfaction" => XVal::Ref(nth.map_or(0, |x| x.0 .0)),
                _ => XVal::Num(nth.map_or(0.0, |x| f64::from(x.1))),
            }
        }
        "gethair" | "geteyes" => form_of(field(g, item(g, 0).unwrap_or(FormId(0)), if name == "gethair" { "HNAM" } else { "ENAM" }, "")),
        "gethairlength" | "getnpcheight" | "getnpcweight" => {
            let sub = match name {
                "gethairlength" => "LNAM",
                "getnpcheight" => "NAM6",
                _ => "NAM7",
            };
            match num_of(field(g, item(g, 0).unwrap_or(FormId(0)), sub, "")) {
                // A height of 0 in the record reads 1 [observed: nvse-actors, Doc, Sunny
                // and Trudy].
                XVal::Num(x) if name == "getnpcheight" && x == 0.0 => XVal::Num(1.0),
                v => v,
            }
        }
        // `GetHairColor which [npc]`: 1 red, 2 green, 3 blue, 0 all packed as
        // red + green × 256 + blue × 65536 [observed: nvse-actors, Doc 191/183/183 gives
        // 12040127].
        "gethaircolor" => {
            let a = item(g, 1).unwrap_or(FormId(0));
            let c = |k: &str| match field(g, a, "HCLR", k) {
                Some(XVal::Num(x)) => x,
                _ => 0.0,
            };
            XVal::Num(match int_of(args.first(), 0) {
                1 => c("red"),
                2 => c("green"),
                3 => c("blue"),
                _ => c("red") + c("green") * 256.0 + c("blue") * 65536.0,
            })
        }
        // The ACBS flags' low and high 16 bits [hypothesis from the names].
        "getactorbaseflagslow" | "getactorbaseflagshigh" => {
            let bits = match field(g, item(g, 0).unwrap_or(FormId(0)), "ACBS", "flags") {
                Some(XVal::Num(x)) => x as u64,
                _ => 0,
            };
            XVal::Num(if name.ends_with("low") { (bits & 0xFFFF) as f64 } else { (bits >> 16) as f64 })
        }
        // AI packages of the reference's base: the "AI Packages" template's list whenever
        // the flag is set, the base's own list beside it unread [documented: GetPackageCount;
        // GetNthPackage; observed: nvse-misc, Doc 9, 0 = 15ED62; tmpl2 (#1057)].
        "getpackagecount" | "getnthpackage" => {
            let b = item(g, usize::MAX).unwrap_or(FormId(0));
            let src = g.data.base_template_source(b, |d| d.template_packages);
            let list = g.data.actors.get(&src).map(|d| d.packages.clone()).unwrap_or_default();
            // AddPackageAt's and RemovePackageAt's list first.
            let list = g.other.package_lists.get(&b).cloned().unwrap_or(list);
            if name == "getpackagecount" {
                XVal::Num(list.len() as f64)
            } else {
                XVal::Ref(usize::try_from(int_of(args.first(), -1)).ok().and_then(|n| list.get(n)).map_or(0, |p| p.0))
            }
        }
        // [documented: GetPackageTargetDistance] the target's count / distance.
        "getpackagetargetdistance" => {
            let p = as_form(args.first()).unwrap_or(FormId(0));
            XVal::Num(g.package_def(&p).and_then(|d| d.target).map_or(0.0, |t| f64::from(t.count_distance)))
        }
        "getcurrentpackage" => XVal::Ref(as_form(args.first()).or(subject).and_then(|a| g.current_package(a)).map_or(0, |p| p.0)),
        // [documented: GetQuestObjectiveCount; GetNthQuestObjective; observed: nvse-misc,
        // VCG02 9 objectives, 0–3 = 10, 5, 20, 30: record order].
        "getquestobjectivecount" | "getnthquestobjective" => {
            let q = g.data.quests.get(&as_form(args.first()).unwrap_or(FormId(0)));
            let order = q.map(|q| q.objective_order.clone()).unwrap_or_default();
            if name == "getquestobjectivecount" {
                XVal::Num(order.len() as f64)
            } else {
                XVal::Num(usize::try_from(int_of(args.get(1), -1)).ok().and_then(|n| order.get(n)).map_or(-1.0, |&i| f64::from(i)))
            }
        }
        // [documented: GetParentCellOwner; GetOwningFactionRequiredRank] the cell's owner
        // and the faction rank a reference's owner needs.
        "getparentcellowner" => XVal::Ref(subject.and_then(|r| g.cell_of(r)).and_then(|c| g.cell_ownership(c)).map_or(0, |o| o.owner.0)),
        "getowningfactionrequiredrank" => XVal::Num(subject.and_then(|r| g.other.owner_ranks.get(&r).or_else(|| g.data.owner_ranks.get(&r))).map_or(0.0, |&r| f64::from(r))),
        // `GetBipedModelPath which [form]`: 0 male biped (MODL), 1 female biped (MOD3),
        // 2 male ground (MOD2), 3 female ground (MOD4) [observed: nvse-misc, leather armor].
        "getbipedmodelpath" => {
            let f = item(g, 1).unwrap_or(FormId(0));
            let sub = match int_of(args.first(), -1) {
                0 => "MODL",
                1 => "MOD3",
                2 => "MOD2",
                3 => "MOD4",
                _ => "",
            };
            match field(g, f, sub, "").or_else(|| field(g, f, sub, "model")) {
                Some(XVal::Str(t)) => XVal::Str(t),
                _ => XVal::Str(String::new()),
            }
        }
        // [documented: GetNumRanks; GetNthFactionRankName] a faction's ranks.
        "getnumranks" => XVal::Num(g.data.faction_ranks(as_form(args.first()).unwrap_or(FormId(0))).len() as f64),
        // [documented: GetFactionRankNames; GetFactionRankFemaleNames; observed:
        // nvse-actors, BoS 8 names, [0] "Squire"] the titles as an array.
        "getfactionranknames" | "getfactionrankfemalenames" => {
            let ranks = g.data.faction_ranks(as_form(args.first()).unwrap_or(FormId(0)));
            let female = name.contains("female");
            let entries = ranks.into_iter().enumerate().map(|(i, r)| (fnv_script::vm::XKey::Num(i as f64), XVal::Str(if female { r.1 } else { r.0 }))).collect();
            XVal::Array(g.ext.new_array(fnv_script::vm::ArrayKind::List, entries))
        }
        "getnthfactionrankname" => {
            let ranks = g.data.faction_ranks(as_form(args.first()).unwrap_or(FormId(0)));
            let f = as_form(args.first()).unwrap_or(FormId(0));
            let i = usize::try_from(int_of(args.get(1), -1)).ok();
            let r = i.and_then(|i| ranks.get(i));
            let female = args.get(2).is_some_and(XVal::truthy);
            // SetNthFactionRankNameEx's title first.
            match i.and_then(|i| g.other.ranks.get(&(f, i, female))) {
                Some(t) => XVal::Str(t.clone()),
                None => XVal::Str(r.map(|r| if female { r.1.clone() } else { r.0.clone() }).unwrap_or_default()),
            }
        }
        // Form lists [documented: ListGetNthForm; ListGetFormIndex; ListAddForm;
        // ListRemoveForm; ListRemoveNthForm; ListClear]. The changes last for the session:
        // "Forms added to a list with this function will not persist across savegames"
        // [documented: ListAddForm].
        "listgetnthform" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            let n = int_of(args.get(1), 0);
            let m = crate::functions::list_members(g, l);
            XVal::Ref(usize::try_from(n).ok().and_then(|n| m.get(n)).map_or(0, |f| f.0))
        }
        "listgetformindex" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            let f = as_form(args.get(1));
            let m = crate::functions::list_members(g, l);
            XVal::Num(f.and_then(|f| m.iter().position(|&x| x == f)).map_or(-1.0, |i| i as f64))
        }
        "listaddform" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            let Some(f) = as_form(args.get(1)) else { return Some(Ok(XVal::Num(-1.0))) };
            let at = int_of(args.get(2), -2);
            let dupes = args.get(3).is_some_and(XVal::truthy);
            let list = g.edited_list(l);
            if dupes && list.contains(&f) {
                return Some(Ok(XVal::Num(-1.0)));
            }
            // -2 or past the end: at the end [documented]; another negative index fails
            // [hypothesis].
            let i = match usize::try_from(at) {
                Ok(i) if i <= list.len() => i,
                _ if at == -2 => list.len(),
                Ok(_) => list.len(),
                Err(_) => return Some(Ok(XVal::Num(-1.0))),
            };
            list.insert(i, f);
            XVal::Num(i as f64)
        }
        "listremoveform" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            let f = as_form(args.get(1));
            let list = g.edited_list(l);
            match f.and_then(|f| list.iter().position(|&x| x == f)) {
                Some(i) => {
                    list.remove(i);
                    XVal::Num(i as f64)
                }
                None => XVal::Num(-1.0),
            }
        }
        // Without an index, the last form [hypothesis: the page gives the index as
        // optional without a default].
        "listremoventhform" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            let list = g.edited_list(l);
            let n = int_of(args.get(1), list.len() as i64 - 1);
            match usize::try_from(n).ok().filter(|&n| n < list.len()) {
                Some(n) => XVal::Ref(list.remove(n).0),
                None => XVal::Ref(0),
            }
        }
        "listclear" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            g.edited_list(l).clear();
            XVal::Num(0.0)
        }
        _ => return None,
    };
    Some(Ok(v))
}

/// GetWeaponAttackAnimation's index of a WEAP DNAM attack-animation code [documented:
/// GetWeaponAttackAnimation's list; the codes are the schema's, WeaponDNAM].
fn attack_animation_index(code: i64) -> f64 {
    let order: [i64; 23] = [255, 38, 44, 50, 56, 62, 68, 26, 74, 32, 80, 86, 114, 120, 126, 132, 138, 102, 108, 144, 150, 156, 162];
    order.iter().position(|&c| c == code).map_or(0.0, |i| i as f64)
}
