//! The records that decide what an actor looks like, read through the Layer 1 schema:
//! NPC_, RACE, HAIR, EYES, HDPT, ARMO (worn models and protection) and LVLN; for
//! creatures CREA and LVLC.
//!
//! Field meanings: docs/spec-notes/actors.md §1 (layouts from xEdit's public
//! definitions, `schemas/fnv/gameplay.json` and `objects.json`).

use std::collections::{BTreeMap, HashMap};

use fnv_esm::{LoadOrder, Record, Schema};
use fnv_sim::extract::{self, all, first, global, has_flag, int, local_fid, num, text, Decoded, Issues};
use fnv_sim::FormId;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

/// NPC_ template flags (ACBS) [documented: xEdit wbTemplateFlags, FO3/FNV branch].
pub mod template {
    pub const TRAITS: u16 = 1 << 0;
    pub const STATS: u16 = 1 << 1;
    pub const INVENTORY: u16 = 1 << 8;
    pub const MODEL_ANIMATION: u16 = 1 << 6;
}

/// The three FaceGen coefficient arrays: symmetric and asymmetric geometry, symmetric
/// texture (FGGS 50, FGGA 30, FGTS 50 floats) [documented: xEdit wbFaceGen]. Stored f32
/// values widened to f64, so reports print them exactly.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FaceGen {
    pub fggs: Vec<f64>,
    pub fgga: Vec<f64>,
    pub fgts: Vec<f64>,
}

impl FaceGen {
    fn from_decoded(d: &[Decoded]) -> Option<FaceGen> {
        let floats = |sig: &[u8; 4]| -> Option<Vec<f64>> {
            Some(first(d, sig)?.as_array()?.iter().filter_map(Value::as_f64).collect())
        };
        let (fggs, fgga, fgts) = (floats(b"FGGS"), floats(b"FGGA"), floats(b"FGTS"));
        if fggs.is_none() && fgga.is_none() && fgts.is_none() {
            return None;
        }
        Some(FaceGen { fggs: fggs.unwrap_or_default(), fgga: fgga.unwrap_or_default(), fgts: fgts.unwrap_or_default() })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Npc {
    pub id: FormId,
    pub edid: Option<String>,
    pub name: Option<String>,
    pub female: bool,
    /// ACBS flag "Is CharGen Face Preset" [documented: xEdit ACBS flags]: the race menu's
    /// starting faces.
    pub chargen_preset: bool,
    pub template: Option<FormId>,
    pub template_flags: u16,
    /// MODL: the skeleton, relative to `meshes\` [observed: `Characters\_Male\Skeleton.NIF`].
    pub skeleton: Option<String>,
    pub race: Option<FormId>,
    pub hair: Option<FormId>,
    pub hair_length: Option<f64>,
    pub hair_color: Option<[u8; 3]>,
    pub eyes: Option<FormId>,
    pub head_parts: Vec<FormId>,
    pub facegen: Option<FaceGen>,
    /// NAM6 "Height" [documented: xEdit]; meaning in game to be captured.
    pub height: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RacePart {
    /// INDX: head parts 0 Head, 1 Ears, 2 Mouth, 3 Teeth Lower, 4 Teeth Upper, 5 Tongue,
    /// 6 Left Eye, 7 Right Eye; body parts 0 Upper Body, 1 Left Hand, 2 Right Hand,
    /// 3 Upper Body Texture [documented: xEdit wbHeadPartIndexEnum, wbBodyPartIndexEnum].
    pub index: u32,
    pub model: Option<String>,
    pub icon: Option<String>,
}

/// Per sex: index 0 male, 1 female.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Race {
    pub id: FormId,
    pub edid: Option<String>,
    /// DATA male/female height.
    pub height: [f64; 2],
    pub head: [Vec<RacePart>; 2],
    pub body: [Vec<RacePart>; 2],
    pub default_hair: [Option<FormId>; 2],
    pub facegen: [Option<FaceGen>; 2],
    /// FULL.
    pub name: Option<String>,
    /// DATA flags Playable and Child [documented: xEdit RACE DATA].
    pub playable: bool,
    pub child: bool,
    /// The hairs (HNAM) and eyes (ENAM) the race allows, in record order.
    pub hairs: Vec<FormId>,
    pub eyes: Vec<FormId>,
    /// ONAM "Older" and YNAM "Younger" races.
    pub older: Option<FormId>,
    pub younger: Option<FormId>,
    /// VTCK voice types, male and female.
    pub voices: [Option<FormId>; 2],
}

/// Which sexes a HAIR or EYES record is for and whether the player may choose it (DATA
/// flags Playable, Not Male, Not Female [documented: xEdit HAIR, EYES]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Choosable {
    pub playable: bool,
    pub not_male: bool,
    pub not_female: bool,
}

impl Choosable {
    fn from_flags(v: Option<&Value>) -> Choosable {
        let has = |n: &str| v.is_some_and(|f| has_flag(f, n));
        Choosable { playable: has("Playable"), not_male: has("Not Male"), not_female: has("Not Female") }
    }

    /// Playable and allowed for that sex.
    pub fn for_sex(&self, female: bool) -> bool {
        self.playable && !(if female { self.not_female } else { self.not_male })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hair {
    pub model: Option<String>,
    /// ICON, named "Texture" by xEdit.
    pub texture: Option<String>,
    pub edid: Option<String>,
    pub name: Option<String>,
    pub choosable: Choosable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Eyes {
    pub texture: Option<String>,
    pub edid: Option<String>,
    pub name: Option<String>,
    pub choosable: Choosable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeadPart {
    pub model: Option<String>,
    pub extra: Vec<FormId>,
    pub edid: Option<String>,
    pub name: Option<String>,
    /// DATA "Playable" [documented: xEdit HDPT, a bool enum].
    pub playable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Armor {
    pub edid: Option<String>,
    /// BMDT biped flags.
    pub slots: u32,
    /// Worn (biped) models: MODL male, MOD3 female.
    pub worn: [Option<String>; 2],
    pub dt: f32,
    /// DNAM DR as stored (xEdit shows it divided by 100).
    pub dr_raw: i32,
    pub power_armor: bool,
    /// BMCT "Ragdoll Constraint Template": a `.rdt` path relative to `meshes\`
    /// [documented: xEdit ARMO BMCT; the path base: GECK wiki SetArmorRCT]
    /// (docs/spec-notes/ragdoll.md §6). Not part of any report.
    #[serde(skip)]
    pub ragdoll_template: Option<String>,
    /// BIPL "Biped Model List": a form list of armour addons (ARMA) worn with the item,
    /// their models drawn and their biped slots covered with it [documented: xEdit ARMO
    /// BIPL; observed: `ArmorLeather` and the CaravanPack's `PreordArmorLeather` (Upper
    /// Body) name `ArmorLeatherList`, the gloves `LeatherArmorGloveR` (Right Hand,
    /// `GloveR.NIF`) and `LeatherArmorGloveL` (Left Hand); hypothesis: worn as parts of
    /// the item]. Its members, as load-order FormIDs; empty for an ARMA. Not part of any
    /// report.
    #[serde(skip)]
    pub addons: Vec<FormId>,
}

/// A WEAP record as far as drawing and animating it goes (workstream 5 part H,
/// docs/spec-notes/actors.md §9) [documented: xEdit WEAP; layouts in `schemas/fnv`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Weapon {
    pub edid: Option<String>,
    /// `MODL`: the third-person model.
    pub model: Option<String>,
    /// DNAM "Animation Type" as the schema names it (e.g. "Pistol - Ballistic (1 Hand)").
    pub anim_type: Option<String>,
    /// DATA base damage.
    pub damage: i32,
    /// DNAM "Projectile" (resolved to a global FormID). Not part of any report.
    #[serde(skip)]
    pub projectile: Option<FormId>,
    /// The damage of the explosion that projectile sets off (PROJ DATA flag "Explosion"
    /// and "Explosion" → EXPL DATA "Damage"), 0 without one; filled by
    /// [`ActorForms::build`] (actors.md §9.3). Not part of any report.
    #[serde(skip)]
    pub explosion_damage: f32,
    /// DNAM "Skill" as the schema names it (e.g. "Guns", "Explosives"); the weapon choice
    /// reads it (actors.md §9.3). Not part of any report.
    #[serde(skip)]
    pub skill: Option<String>,
    /// DNAM flags: "Embedded Weapon" (part of a creature body), "Non-Playable".
    pub embedded: bool,
    pub non_playable: bool,
    /// DNAM flags2 "Player Only" or "Not Used In Normal Combat": no NPC's own weapon choice
    /// takes it (docs/spec-notes/weapon-data-fields.md §1). Not part of any report.
    #[serde(skip)]
    pub not_for_npcs: bool,
    /// WNAM "1st Person Model": a form (a STAT [observed: the 9mm pistol's,
    /// `1stPerson9mm`]) whose model the first-person view shows [documented: xEdit WEAP
    /// WNAM]. Not part of any report.
    #[serde(skip)]
    pub first_person: Option<FormId>,
    /// That form's MODL, relative to `meshes\`.
    #[serde(skip)]
    pub first_person_model: Option<String>,
    /// `MWD1`…`MWD7`: the third-person model with the mods whose flags add up to 1…7
    /// on [documented: xEdit WEAP; observed: the order, docs/spec-notes/weapon-mods.md §2].
    #[serde(skip)]
    pub mod_models: [Option<String>; 7],
    /// `WNM1`…`WNM7`: the first-person forms for the same combinations, and their MODL.
    #[serde(skip)]
    pub mod_first_person: [Option<FormId>; 7],
    #[serde(skip)]
    pub mod_first_person_models: [Option<String>; 7],
    /// `MOD4` "World Model" when it is a model to hold rather than a ground object
    /// ([`held_world_model`]): drawn in the hand in third person in place of `MODL`.
    #[serde(skip)]
    pub held_world_model: Option<String>,
    /// DNAM "Hand Grip": HandGrip1 … HandGrip6 as 1 … 6, `None` for DEFAULT ("a hand pose
    /// animation for 3rd person" [documented: GECK wiki Weapons]); the clips are
    /// `<prefix>handgrip<n>[_<name>].kf` [observed: Fallout - Meshes.bsa,
    /// `1gthandgrip1.kf` … `2hrhandgrip3_combatshotgun.kf`]. Not part of any report.
    #[serde(skip)]
    pub grip: Option<u8>,
    /// DNAM flags1 "Hide Backpack": "hides any equipped armor or armor addon that occupies
    /// the Backpack biped slot" [documented: GECK wiki Weapons]. Not part of any report.
    #[serde(skip)]
    pub hide_backpack: bool,
    /// BIPL "Biped Model List": a form list of armour addons (ARMA) worn with the weapon,
    /// as for an ARMO ([`Armor::addons`]) [documented: xEdit WEAP BIPL]. Every
    /// hand-to-hand weapon of FalloutNV.esm with a hand model names one: the power fist's
    /// `WeaponPowerfist` holds `ARMAPowerfist` (Right Hand, `PowerFist.NIF`, a skinned
    /// metal hand), the brass knuckles' `ARMABrassKnuckles` (Right Hand,
    /// `BrassKnuckles.NIF` / `BrassKnucklesf.NIF`, a skinned hand with the knuckles)
    /// [observed]. Its members, as load-order FormIDs. Not part of any report.
    #[serde(skip)]
    pub addons: Vec<FormId>,
}

/// A WEAP's `MOD4` "World Model" when the third-person view holds it in place of `MODL`:
/// when it differs from `MODL` and is not a ground object (a file named `…go.nif`,
/// `…_go.nif` or `…worldobject.nif`) [documented: xEdit WEAP MOD4 "World Model";
/// observed: in FalloutNV.esm 42 weapons' MOD4 differs from their MODL, 41 of them such
/// ground objects; the 42nd, Lucky's (`WeapNV357RevolverUnique`), is
/// `357revolvercustom.nif`, the same nodes as its MODL `357revolver.nif` with the
/// engraved `357revolverCustom.dds` in place of `357revolverStd.dds`, the texture its
/// first-person model shows; hypothesis: the game holds that one, the owner's playtest
/// list 7 saw the plain texture in third person].
pub fn held_world_model(model: Option<&str>, world: Option<&str>) -> Option<String> {
    let w = world?.trim();
    let l = w.to_ascii_lowercase();
    let stem = l.strip_suffix(".nif")?;
    if model.is_some_and(|m| m.trim().eq_ignore_ascii_case(w)) || stem.ends_with("go") || stem.ends_with("worldobject") {
        return None;
    }
    Some(w.to_string())
}

impl Weapon {
    /// The model for the view (`first_person`) with the mods `flags` on: the combination's
    /// own model when the record has one, else the plain one (weapon-mods.md §2).
    pub fn model_for(&self, first_person: bool, flags: u8) -> Option<&str> {
        let third = self.held_world_model.as_deref().or(self.model.as_deref());
        let plain = if first_person { self.first_person_model.as_deref().or(self.model.as_deref()) } else { third };
        self.modded_model(first_person, flags).or(plain)
    }

    /// The combination's own model for the view with the mods `flags` on, if the record
    /// has one. A weapon without a first-person form draws its third-person models in
    /// first person, so its `MWD` models stand for the missing `WNM` forms there
    /// (weapon-mods.md §2).
    pub fn modded_model(&self, first_person: bool, flags: u8) -> Option<&str> {
        let i = (flags & 7) as usize;
        if i == 0 {
            return None;
        }
        let third = self.mod_models[i - 1].as_deref();
        if !first_person {
            return third;
        }
        self.mod_first_person_models[i - 1].as_deref().or(third.filter(|_| self.first_person_model.is_none()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeveledNpc {
    pub chance_none: u8,
    pub all_levels: bool,
    /// (level, actor base or another LVLN)
    pub entries: Vec<(u16, FormId)>,
}

/// A creature base (CREA) [documented: xEdit wbDefinitionsFNV.pas CREA; layouts in
/// `schemas/fnv/gameplay.json`]. `MODL`, `NIFZ` and `KFFZ` belong to the template group
/// Model/Animation, `BNAM` to Stats (xEdit hides them when that group's flag is set).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Creature {
    pub id: FormId,
    pub edid: Option<String>,
    pub template: Option<FormId>,
    pub template_flags: u16,
    /// MODL: the skeleton, relative to `meshes\` [observed:
    /// `creatures\NVBigHorner\skeleton.nif`].
    pub skeleton: Option<String>,
    /// NIFZ "Model List": body part file names, as stored.
    pub models: Vec<String>,
    /// KFFZ "Animations": extra `.kf` file names, as stored.
    pub anims: Vec<String>,
    /// BNAM "Base Scale".
    pub base_scale: Option<f64>,
    /// DATA type (Animal, Mutated Insect, Robot, …), as the schema names it.
    pub kind: Option<String>,
}

#[derive(Default)]
pub struct ActorForms {
    pub npcs: BTreeMap<FormId, Npc>,
    pub races: HashMap<FormId, Race>,
    pub hairs: HashMap<FormId, Hair>,
    pub eyes: HashMap<FormId, Eyes>,
    pub head_parts: HashMap<FormId, HeadPart>,
    pub armors: HashMap<FormId, Armor>,
    pub weapons: HashMap<FormId, Weapon>,
    pub leveled: HashMap<FormId, LeveledNpc>,
    pub creatures: BTreeMap<FormId, Creature>,
    /// Leveled creature lists (LVLC; the LVLN layout).
    pub leveled_creatures: HashMap<FormId, LeveledNpc>,
    /// Signature of the winning record of every actor-related form, for reports.
    pub sigs: HashMap<FormId, [u8; 4]>,
    pub edids: HashMap<String, FormId>,
    pub issues: Issues,
    /// Alternate textures by (form, model subrecord): ARMO MODL/MOD3 (worn), HDPT MODL,
    /// WEAP MODL and the STATs weapons name as first-person models (MODL)
    /// (`fnv_world::alt_textures`, docs/spec-notes/texture-swaps.md). Only lists that
    /// have entries are kept.
    pub alt_textures: HashMap<(FormId, [u8; 4]), Vec<fnv_world::alt_textures::AltTexture>>,
}

const SIGS: &[&[u8; 4]] = &[b"NPC_", b"RACE", b"HAIR", b"EYES", b"HDPT", b"ARMO", b"ARMA", b"WEAP", b"LVLN", b"CREA", b"LVLC"];

enum Loaded {
    Npc(Box<Npc>),
    Race(Box<Race>),
    Hair(Hair),
    Eyes(Eyes),
    HeadPart(HeadPart),
    Armor(Armor),
    Weapon(Box<Weapon>),
    Leveled(LeveledNpc),
    Creature(Box<Creature>),
    LeveledCreature(LeveledNpc),
    Other,
}

impl ActorForms {
    /// The winning WEAP record of `id` alone, read as [`ActorForms::build`] reads it
    /// (without the first-person models of `WNAM`/`WNM*`, which name other records);
    /// `None` when `id` is no weapon or does not decode. For a weapon reference's model
    /// where the actor forms are not at hand ([`Weapon::model_for`]).
    pub fn weapon_record(lo: &LoadOrder, schema: &Schema, id: FormId) -> Option<Weapon> {
        let d = lo.index.get(&id.0)?.last()?;
        let pi = d.plugin as usize;
        let r = lo.records[pi][d.ordinal as usize];
        if &r.header.sig != b"WEAP" {
            return None;
        }
        let decoded = extract::decode(r, schema).ok()?;
        Some(weapon(lo, pi, id, &decoded, &mut Issues::default()))
    }

    pub fn build(lo: &LoadOrder, schema: &Schema) -> ActorForms {
        let mut out = ActorForms::default();
        let mut wanted: Vec<(FormId, usize, &Record)> = Vec::new();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            if !SIGS.contains(&&r.header.sig) || r.header.flags & 0x20 != 0 {
                continue;
            }
            wanted.push((FormId(g), d.plugin as usize, r));
        }
        wanted.sort_by_key(|w| w.0);
        let loaded: Vec<_> = wanted
            .par_iter()
            .map(|&(id, pi, r)| {
                let mut issues = Issues::default();
                let res = match extract::decode(r, schema) {
                    Ok(d) => {
                        let edid = first(&d, b"EDID").and_then(text);
                        (edid, load_one(lo, pi, id, &r.header.sig, &d, &mut issues))
                    }
                    Err(e) => {
                        issues.note("record does not decode", || format!("{id}: {e}"));
                        (None, Loaded::Other)
                    }
                };
                (id, r.header.sig, res, issues)
            })
            .collect();
        for (id, sig, (edid, l), issues) in loaded {
            out.issues.merge(issues);
            out.sigs.insert(id, sig);
            if let Some(e) = edid {
                out.edids.insert(e.to_ascii_lowercase(), id);
            }
            match l {
                Loaded::Npc(n) => {
                    out.npcs.insert(id, *n);
                }
                Loaded::Race(r) => {
                    out.races.insert(id, *r);
                }
                Loaded::Hair(h) => {
                    out.hairs.insert(id, h);
                }
                Loaded::Eyes(e) => {
                    out.eyes.insert(id, e);
                }
                Loaded::HeadPart(h) => {
                    out.head_parts.insert(id, h);
                }
                Loaded::Armor(a) => {
                    out.armors.insert(id, a);
                }
                Loaded::Weapon(w) => {
                    out.weapons.insert(id, *w);
                }
                Loaded::Leveled(l) => {
                    out.leveled.insert(id, l);
                }
                Loaded::Creature(c) => {
                    out.creatures.insert(id, *c);
                }
                Loaded::LeveledCreature(l) => {
                    out.leveled_creatures.insert(id, l);
                }
                Loaded::Other => {}
            }
        }
        // The forms weapons name as their first-person model (WNAM), read for their MODL
        // only: they are statics, too many to load as a type.
        let model_of = |fp: FormId| {
            let d = lo.index.get(&fp.0).and_then(|defs| defs.last())?;
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            if r.header.flags & 0x20 != 0 {
                return None;
            }
            extract::decode(r, schema).ok().and_then(|dec| path(first(&dec, b"MODL")))
        };
        // Alternate textures of the models drawn on actors (texture-swaps.md §4).
        let mut alt = Vec::new();
        alt.extend(out.armors.keys().flat_map(|&id| [(id, *b"MODL"), (id, *b"MOD3")]));
        alt.extend(out.head_parts.keys().map(|&id| (id, *b"MODL")));
        alt.extend(out.weapons.iter().flat_map(|(&id, w)| std::iter::once(id).chain(w.first_person)).map(|id| (id, *b"MODL")));
        alt.extend(out.weapons.iter().filter(|(_, w)| w.held_world_model.is_some()).map(|(&id, _)| (id, *b"MOD4")));
        out.alt_textures = alt
            .into_par_iter()
            .filter_map(|(id, sub)| {
                let list = fnv_world::alt_textures::alt_textures_of(lo, schema, id.0, &sub);
                (!list.is_empty()).then_some(((id, sub), list))
            })
            .collect();
        for w in out.weapons.values_mut() {
            w.first_person_model = w.first_person.and_then(model_of);
            w.mod_first_person_models = w.mod_first_person.map(|f| f.and_then(model_of));
            // The explosion its projectile sets off, for the weapon choice (actors.md §9.3).
            w.explosion_damage = w.projectile.map_or(0.0, |p| fnv_sim::forms::explosion_damage(lo, schema, p));
        }
        out
    }

    /// The alternate textures of `form`'s model in `sub` (empty when it has none).
    pub fn alt_textures(&self, form: FormId, sub: &[u8; 4]) -> &[fnv_world::alt_textures::AltTexture] {
        self.alt_textures.get(&(form, *sub)).map_or(&[], Vec::as_slice)
    }

    /// The alternate textures of a resolved actor part's model: a worn item's MODL list,
    /// or its MOD3 list when a female wears the female model; a head part's MODL list.
    /// Race parts and hair have none [observed: no RACE or HAIR record carries one].
    pub fn part_alt_textures(&self, part: &crate::resolve::Part, female: bool) -> &[fnv_world::alt_textures::AltTexture] {
        match part.role.as_str() {
            "armor" => {
                let female_model = female && self.armors.get(&part.form).is_some_and(|a| a.worn[1].is_some());
                self.alt_textures(part.form, if female_model { b"MOD3" } else { b"MODL" })
            }
            "head_part" => self.alt_textures(part.form, b"MODL"),
            _ => &[],
        }
    }

    /// The alternate textures of a weapon's model for the view with the mods `flags` on
    /// (the model [`Weapon::model_for`] gives): the first-person form's MODS in first
    /// person, the weapon's own MODS otherwise (its MO4S when it holds its MOD4,
    /// [`held_world_model`]); none for a modded model, which no alternate-texture list
    /// pairs with [documented: xEdit WEAP has MODS only beside MODL, MO2S–MO4S beside the
    /// casing, scope and world models].
    pub fn weapon_alt_textures(&self, id: FormId, first_person: bool, flags: u8) -> &[fnv_world::alt_textures::AltTexture] {
        let Some(w) = self.weapons.get(&id) else { return &[] };
        if w.modded_model(first_person, flags).is_some() {
            return &[];
        }
        match (first_person, w.first_person.filter(|_| w.first_person_model.is_some())) {
            (true, Some(fp)) => self.alt_textures(fp, b"MODL"),
            (false, _) if w.held_world_model.is_some() => self.alt_textures(id, b"MOD4"),
            _ => self.alt_textures(id, b"MODL"),
        }
    }

    pub fn by_edid(&self, edid: &str) -> Option<FormId> {
        self.edids.get(&edid.to_ascii_lowercase()).copied()
    }
}

fn fid(lo: &LoadOrder, pi: usize, id: FormId, sig: &str, v: Option<&Value>, issues: &mut Issues) -> Option<FormId> {
    let local = local_fid(v?)?;
    match global(lo, pi, local) {
        Ok(g) => g,
        Err(l) => {
            issues.note("FormID outside master list", || format!("{id} {sig} 0x{l:08X}"));
            None
        }
    }
}

fn path(v: Option<&Value>) -> Option<String> {
    v.and_then(text).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn load_one(lo: &LoadOrder, pi: usize, id: FormId, sig: &[u8; 4], d: &[Decoded], issues: &mut Issues) -> Loaded {
    match sig {
        b"NPC_" => Loaded::Npc(Box::new(npc(lo, pi, id, d, issues))),
        b"RACE" => Loaded::Race(Box::new(race(lo, pi, id, d, issues))),
        b"HAIR" => Loaded::Hair(Hair {
            model: path(first(d, b"MODL")),
            texture: path(first(d, b"ICON")),
            edid: first(d, b"EDID").and_then(text),
            name: first(d, b"FULL").and_then(text),
            choosable: Choosable::from_flags(first(d, b"DATA")),
        }),
        b"EYES" => Loaded::Eyes(Eyes {
            texture: path(first(d, b"ICON")),
            edid: first(d, b"EDID").and_then(text),
            name: first(d, b"FULL").and_then(text),
            choosable: Choosable::from_flags(first(d, b"DATA")),
        }),
        b"HDPT" => Loaded::HeadPart(HeadPart {
            model: path(first(d, b"MODL")),
            extra: all(d, b"HNAM").filter_map(|v| fid(lo, pi, id, "HNAM", Some(v), issues)).collect(),
            edid: first(d, b"EDID").and_then(text),
            name: first(d, b"FULL").and_then(text),
            playable: first(d, b"DATA").and_then(Value::as_str) == Some("True"),
        }),
        b"ARMO" => {
            let mut a = armor(d);
            let list = fid(lo, pi, id, "BIPL", first(d, b"BIPL"), issues);
            a.addons = list.map(|l| crate::furniture::list_members(lo, l)).unwrap_or_default();
            Loaded::Armor(a)
        }
        // Armour addons (BIPL members): the same biped data and models [documented:
        // xEdit ARMA].
        b"ARMA" => Loaded::Armor(armor(d)),
        b"WEAP" => {
            let mut w = weapon(lo, pi, id, d, issues);
            let list = fid(lo, pi, id, "BIPL", first(d, b"BIPL"), issues);
            w.addons = list.map(|l| crate::furniture::list_members(lo, l)).unwrap_or_default();
            Loaded::Weapon(Box::new(w))
        }
        b"LVLN" => Loaded::Leveled(leveled_npc(lo, pi, id, d, issues)),
        b"CREA" => Loaded::Creature(Box::new(creature(lo, pi, id, d, issues))),
        b"LVLC" => Loaded::LeveledCreature(leveled_npc(lo, pi, id, d, issues)),
        _ => Loaded::Other,
    }
}

fn npc(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> Npc {
    let acbs = first(d, b"ACBS");
    let female = acbs.and_then(|a| a.get("flags")).is_some_and(|f| has_flag(f, "Female"));
    let chargen_preset = acbs.and_then(|a| a.get("flags")).is_some_and(|f| has_flag(f, "Is CharGen Face Preset"));
    let template_flags = acbs.and_then(|a| a.get("template_flags")).and_then(|f| num(f, "value")).unwrap_or(0.0) as u16;
    let hair_color = first(d, b"HCLR").and_then(|c| Some([int(c, "red")? as u8, int(c, "green")? as u8, int(c, "blue")? as u8]));
    Npc {
        id,
        edid: first(d, b"EDID").and_then(text),
        name: first(d, b"FULL").and_then(text),
        female,
        chargen_preset,
        template: fid(lo, pi, id, "TPLT", first(d, b"TPLT"), issues),
        template_flags,
        skeleton: path(first(d, b"MODL")),
        race: fid(lo, pi, id, "RNAM", first(d, b"RNAM"), issues),
        hair: fid(lo, pi, id, "HNAM", first(d, b"HNAM"), issues),
        hair_length: first(d, b"LNAM").and_then(Value::as_f64),
        hair_color,
        eyes: fid(lo, pi, id, "ENAM", first(d, b"ENAM"), issues),
        head_parts: all(d, b"PNAM").filter_map(|v| fid(lo, pi, id, "PNAM", Some(v), issues)).collect(),
        facegen: FaceGen::from_decoded(d),
        height: first(d, b"NAM6").and_then(Value::as_f64),
    }
}

/// The entries of a `zstring_array` value, trimmed, empty ones left out.
fn strings(v: Option<&Value>) -> Vec<String> {
    let Some(a) = v.and_then(Value::as_array) else { return Vec::new() };
    a.iter().filter_map(text).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn creature(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> Creature {
    let acbs = first(d, b"ACBS");
    Creature {
        id,
        edid: first(d, b"EDID").and_then(text),
        template: fid(lo, pi, id, "TPLT", first(d, b"TPLT"), issues),
        template_flags: acbs.and_then(|a| a.get("template_flags")).and_then(|f| num(f, "value")).unwrap_or(0.0) as u16,
        skeleton: path(first(d, b"MODL")),
        models: strings(first(d, b"NIFZ")),
        anims: strings(first(d, b"KFFZ")),
        base_scale: first(d, b"BNAM").and_then(Value::as_f64),
        kind: first(d, b"DATA").and_then(|v| v.get("type")).and_then(text),
    }
}

/// RACE subrecords are positional (docs/spec-notes/actors.md §1.1): `NAM0` opens the head
/// data and `NAM1` the body data, each with an `MNAM` (male) and an `FNAM` (female) list
/// of `INDX` + `MODL`/`ICON` parts; after the body data's `HNAM`/`ENAM` arrays, `MNAM`
/// and `FNAM` open the male and female FaceGen data.
fn race(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> Race {
    #[derive(PartialEq)]
    enum Section {
        Start,
        Head,
        Body,
        FaceGen,
    }
    let mut out = Race {
        id,
        edid: first(d, b"EDID").and_then(text),
        height: [1.0, 1.0],
        head: Default::default(),
        body: Default::default(),
        default_hair: [None, None],
        facegen: [None, None],
        name: first(d, b"FULL").and_then(text),
        playable: false,
        child: false,
        hairs: Vec::new(),
        eyes: Vec::new(),
        older: fid(lo, pi, id, "ONAM", first(d, b"ONAM"), issues),
        younger: fid(lo, pi, id, "YNAM", first(d, b"YNAM"), issues),
        voices: [None, None],
    };
    if let Some(data) = first(d, b"DATA") {
        out.height = [
            num(data, "male_height").unwrap_or(1.0),
            num(data, "female_height").unwrap_or(1.0),
        ];
        let flags = data.get("flags");
        out.playable = flags.is_some_and(|f| has_flag(f, "Playable"));
        out.child = flags.is_some_and(|f| has_flag(f, "Child"));
    }
    if let Some(v) = first(d, b"VTCK") {
        out.voices = [fid(lo, pi, id, "VTCK", v.get("male"), issues), fid(lo, pi, id, "VTCK", v.get("female"), issues)];
    }
    let list = |sig: &[u8; 4], issues: &mut Issues| -> Vec<FormId> {
        let name = String::from_utf8_lossy(sig).into_owned();
        first(d, sig).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| fid(lo, pi, id, &name, Some(v), issues)).collect()).unwrap_or_default()
    };
    out.hairs = list(b"HNAM", issues);
    out.eyes = list(b"ENAM", issues);
    if let Some(h) = first(d, b"DNAM") {
        out.default_hair = [
            fid(lo, pi, id, "DNAM", h.get("male"), issues),
            fid(lo, pi, id, "DNAM", h.get("female"), issues),
        ];
    }
    let mut section = Section::Start;
    let mut sex = 0usize;
    let mut fg_start: [Option<usize>; 2] = [None, None];
    for (i, s) in d.iter().enumerate() {
        match &s.sig {
            b"NAM0" => section = Section::Head,
            b"NAM1" => section = Section::Body,
            b"HNAM" | b"ENAM" if section == Section::Body => section = Section::FaceGen,
            b"MNAM" | b"FNAM" => {
                sex = usize::from(&s.sig == b"FNAM");
                if section == Section::FaceGen {
                    fg_start[sex] = Some(i + 1);
                }
            }
            b"INDX" => {
                let Some(index) = s.value.as_ref().and_then(Value::as_u64) else {
                    issues.note("RACE INDX does not decode", || format!("{id}"));
                    continue;
                };
                let part = RacePart { index: index as u32, ..RacePart::default() };
                match section {
                    Section::Head => out.head[sex].push(part),
                    Section::Body => out.body[sex].push(part),
                    _ => issues.note("RACE INDX outside head/body data", || format!("{id}")),
                }
            }
            b"MODL" | b"ICON" => {
                let list = match section {
                    Section::Head => &mut out.head[sex],
                    Section::Body => &mut out.body[sex],
                    _ => continue,
                };
                let Some(part) = list.last_mut() else { continue };
                let v = path(s.value.as_ref());
                if &s.sig == b"MODL" {
                    part.model = v;
                } else {
                    part.icon = v;
                }
            }
            _ => {}
        }
    }
    for (sex, start) in fg_start.iter().enumerate() {
        if let Some(start) = *start {
            let end = d[start..].iter().position(|s| &s.sig == b"MNAM" || &s.sig == b"FNAM").map_or(d.len(), |p| start + p);
            out.facegen[sex] = FaceGen::from_decoded(&d[start..end]);
        }
    }
    out
}

fn armor(d: &[Decoded]) -> Armor {
    let bmdt = first(d, b"BMDT");
    let slots = bmdt.and_then(|b| b.get("biped_flags")).and_then(|f| num(f, "value")).unwrap_or(0.0) as u32;
    let power_armor = bmdt.and_then(|b| b.get("general_flags")).is_some_and(|f| has_flag(f, "Power Armor"));
    let dnam = first(d, b"DNAM");
    Armor {
        edid: first(d, b"EDID").and_then(text),
        slots,
        worn: [path(first(d, b"MODL")), path(first(d, b"MOD3"))],
        dt: dnam.and_then(|v| num(v, "dt")).unwrap_or(0.0) as f32,
        dr_raw: dnam.and_then(|v| int(v, "dr")).unwrap_or(0) as i32,
        power_armor,
        ragdoll_template: path(first(d, b"BMCT")),
        addons: Vec::new(),
    }
}

/// The number of a WEAP DNAM "Hand Grip" (`HandGrip2` → 2); `None` for DEFAULT and the
/// unnamed values.
pub fn hand_grip(name: &str) -> Option<u8> {
    name.strip_prefix("HandGrip")?.parse::<u8>().ok().filter(|n| (1..=6).contains(n))
}

fn weapon(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> Weapon {
    let dnam = first(d, b"DNAM");
    let flags = dnam.and_then(|v| v.get("flags1"));
    Weapon {
        edid: first(d, b"EDID").and_then(text),
        model: path(first(d, b"MODL")),
        anim_type: dnam.and_then(|v| v.get("animation_type")).and_then(text),
        damage: first(d, b"DATA").and_then(|v| int(v, "base_damage")).unwrap_or(0) as i32,
        projectile: fid(lo, pi, id, "DNAM projectile", dnam.and_then(|v| v.get("projectile")), issues),
        explosion_damage: 0.0,
        skill: dnam.and_then(|v| v.get("skill")).and_then(text),
        embedded: flags.is_some_and(|f| has_flag(f, "Embedded Weapon")),
        non_playable: flags.is_some_and(|f| has_flag(f, "Non-Playable")),
        not_for_npcs: dnam.and_then(|v| v.get("flags2")).is_some_and(|f| has_flag(f, "Player Only") || has_flag(f, "Not Used In Normal Combat")),
        first_person: fid(lo, pi, id, "WNAM", first(d, b"WNAM"), issues),
        first_person_model: None,
        mod_models: [b"MWD1", b"MWD2", b"MWD3", b"MWD4", b"MWD5", b"MWD6", b"MWD7"].map(|sig| path(first(d, sig))),
        mod_first_person: [b"WNM1", b"WNM2", b"WNM3", b"WNM4", b"WNM5", b"WNM6", b"WNM7"].map(|sig| fid(lo, pi, id, "WNM", first(d, sig), issues)),
        mod_first_person_models: Default::default(),
        held_world_model: held_world_model(path(first(d, b"MODL")).as_deref(), path(first(d, b"MOD4")).as_deref()),
        grip: dnam.and_then(|v| v.get("grip_animation")).and_then(text).as_deref().and_then(hand_grip),
        hide_backpack: flags.is_some_and(|f| has_flag(f, "Hide Backpack")),
        addons: Vec::new(),
    }
}

fn leveled_npc(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> LeveledNpc {
    let flags = first(d, b"LVLF");
    LeveledNpc {
        chance_none: first(d, b"LVLD").and_then(Value::as_u64).unwrap_or(0).min(255) as u8,
        all_levels: flags.is_some_and(|f| has_flag(f, "Calculate from all levels <= player's level")),
        entries: all(d, b"LVLO")
            .filter_map(|v| Some((int(v, "level")? as u16, fid(lo, pi, id, "LVLO", v.get("reference"), issues)?)))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modded_weapon_draws_the_model_of_its_mods() {
        let mut w = Weapon { edid: None, model: Some("plain.nif".into()), anim_type: None, damage: 0, projectile: None, explosion_damage: 0.0, skill: None, embedded: false, non_playable: false, not_for_npcs: false, first_person: None, first_person_model: Some("1st.nif".into()), mod_models: Default::default(), mod_first_person: [None; 7], mod_first_person_models: Default::default(), held_world_model: None, grip: None, hide_backpack: false, addons: Vec::new() };
        w.mod_models[0] = Some("ext.nif".into());
        w.mod_models[2] = Some("ext_scope.nif".into());
        w.mod_first_person_models[2] = Some("1st_ext_scope.nif".into());
        assert_eq!(w.model_for(false, 0), Some("plain.nif"));
        assert_eq!(w.model_for(true, 0), Some("1st.nif"));
        assert_eq!(w.model_for(false, 1), Some("ext.nif"));
        // Mods 1 and 2: field 3.
        assert_eq!(w.model_for(false, 3), Some("ext_scope.nif"));
        assert_eq!(w.model_for(true, 3), Some("1st_ext_scope.nif"));
        // No first-person model for mod 1 alone: the plain first-person one.
        assert_eq!(w.model_for(true, 1), Some("1st.nif"));
        // A combination without its own model: the plain one.
        assert_eq!(w.model_for(false, 4), Some("plain.nif"));
        // A world model to hold: third person draws it, unmodded and modded without a
        // model of its own; first person keeps its own.
        w.held_world_model = Some("custom.nif".into());
        assert_eq!(w.model_for(false, 0), Some("custom.nif"));
        assert_eq!(w.model_for(false, 4), Some("custom.nif"));
        assert_eq!(w.model_for(false, 1), Some("ext.nif"));
        assert_eq!(w.model_for(true, 0), Some("1st.nif"));
    }

    #[test]
    fn a_weapon_without_first_person_forms_shows_its_mods_in_first_person() {
        // As a mod's weapon may be: MODL and MWD models, no WNAM or WNM forms.
        let mut w = Weapon { edid: None, model: Some("plain.nif".into()), anim_type: None, damage: 0, projectile: None, explosion_damage: 0.0, skill: None, embedded: false, non_playable: false, not_for_npcs: false, first_person: None, first_person_model: None, mod_models: Default::default(), mod_first_person: [None; 7], mod_first_person_models: Default::default(), held_world_model: None, grip: None, hide_backpack: false, addons: Vec::new() };
        w.mod_models[3] = Some("ext.nif".into());
        assert_eq!(w.model_for(true, 0), Some("plain.nif"));
        assert_eq!(w.model_for(true, 4), Some("ext.nif"));
        assert_eq!(w.model_for(true, 2), Some("plain.nif"));
        // Its own WNM form still wins.
        w.mod_first_person_models[3] = Some("1st_ext.nif".into());
        assert_eq!(w.model_for(true, 4), Some("1st_ext.nif"));
    }

    #[test]
    fn only_a_world_model_that_is_no_ground_object_is_held() {
        let held = |m: Option<&str>, w: Option<&str>| held_world_model(m, w);
        assert_eq!(held(Some(r"weapons\1handpistol\357revolver.NIF"), Some(r"weapons\1handpistol\357revolverCustom.nif")).as_deref(), Some(r"weapons\1handpistol\357revolverCustom.nif"));
        assert_eq!(held(Some(r"weapons\1handpistol\thatgun.nif"), Some(r"weapons\1handpistol\thatgunGO.nif")), None);
        assert_eq!(held(Some("a.nif"), Some(r"weapons\hand2hand\boxingglove_go.nif")), None);
        assert_eq!(held(None, Some(r"weapons\hand2hand\powerfistworldobject.nif")), None);
        assert_eq!(held(Some(r"w\a.nif"), Some(r"W\A.NIF")), None);
        assert_eq!(held(Some("a.nif"), None), None);
    }
}
