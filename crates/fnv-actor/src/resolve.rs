//! From a placed NPC to the list of parts that make it up.
//!
//! Every rule here is [hypothesis] unless noted, written down in
//! docs/spec-notes/actors.md §2 so the in-game comparisons can correct it:
//! 1. An LVLN base is resolved to one actor base (§2.1); templates are followed per
//!    template-flag group (§2.2).
//! 2. Worn armour: the carried ARMO items, best protection first (DT, then DR), each
//!    worn unless a better one already covers one of its body slots (§2.3; the
//!    "best protection by DT and DR" part is the owner's statement, 2026-10-08).
//! 3. Race parts are hidden where worn armour covers their slot; headgear covering
//!    Hat selects the hair model's `Hat` shape (§2.4).
//! 4. The weapon an actor holds: among the same carried items, the drawable weapon with
//!    the highest damage, a gun's explosion counted (§9; `crate::weapon::choose_weapon`).

use std::collections::{BTreeMap, HashMap, HashSet};

use fnv_sim::forms::{BaseData, ItemKind};
use fnv_sim::zones::LevelAt;
use fnv_sim::{leveled, FormId, FormStore};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use serde::Serialize;

use crate::records::{template, ActorForms, Creature, FaceGen, Npc, RacePart};

/// BMDT biped slot bits [documented: xEdit wbBMDT].
pub mod slot {
    pub const HEAD: u32 = 1 << 0;
    pub const HAIR: u32 = 1 << 1;
    pub const UPPER_BODY: u32 = 1 << 2;
    pub const LEFT_HAND: u32 = 1 << 3;
    pub const RIGHT_HAND: u32 = 1 << 4;
    pub const PIPBOY: u32 = 1 << 6;
    /// Biped slot 7 [documented: xEdit wbDefinitionsFNV.pas BMDT biped flags].
    pub const BACKPACK: u32 = 1 << 7;
    pub const HAT: u32 = 1 << 10;
}

/// Template chains and nested leveled NPCs deeper than this are reported as cycles.
const MAX_DEPTH: u32 = 16;

/// The skeleton every NPC_ that names a real one uses [observed: 3,534 of 3,534 in the
/// ten masters; `fnvtool actor forms`].
pub const HUMAN_SKELETON: &str = r"meshes\characters\_male\skeleton.nif";

/// Editor placeholder models (`marker_creature.nif`, `marker_*.nif`).
pub fn is_editor_marker(model: &str) -> bool {
    let file = model.rsplit(['\\', '/']).next().unwrap_or(model).to_ascii_lowercase();
    file.starts_with("marker")
}

/// The level leveled lists resolve against for a static display (a level-1 player).
pub const DISPLAY_LEVEL: u16 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Part {
    /// What the part is: `body:upper_body`, `body:left_hand`, `body:right_hand`,
    /// `head:<index name>`, `hair`, `head_part`, `armor`.
    pub role: String,
    /// The form the part comes from (race, HAIR, HDPT or ARMO).
    pub form: FormId,
    /// Archive path, lower case (`meshes\...`).
    pub model: String,
    /// Texture that replaces the model's own diffuse texture, archive path (`textures\...`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Appearance {
    /// The actor base the appearance was taken from (after leveled resolution).
    pub npc: FormId,
    pub edid: Option<String>,
    pub female: bool,
    pub race: FormId,
    /// Skeleton, archive path.
    pub skeleton: String,
    /// The race's height for the actor's sex (DATA).
    pub race_height: f64,
    /// The NPC's own height (NAM6; 1 when 0 or absent), times the race's in the drawn
    /// scale (docs/spec-notes/actor-flags.md §5) [hypothesis]. Not part of the appearance
    /// report.
    #[serde(skip)]
    pub height: f64,
    pub parts: Vec<Part>,
    /// Worn armour, in the order chosen.
    pub worn: Vec<FormId>,
    /// Whether worn headgear selects the hair model's `Hat` shape (§2.4).
    pub hat: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hair_color: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facegen: Option<FaceGen>,
    /// Race FaceGen data for the actor's sex.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub race_facegen: Option<FaceGen>,
    /// Body index 3, "Upper Body Texture": an `.egt` for the body skin (part C).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_egt: Option<String>,
    /// The race's upper body skin texture (body index 0 ICON), whether or not the
    /// upper body is drawn: the texture the body `.egt` tints (engine use; not part of
    /// the appearance document).
    #[serde(skip)]
    pub body_texture: Option<String>,
    /// The race's hand skin texture (body index 1 ICON, else index 2; the two name one
    /// file in every race of FalloutNV.esm [observed]), whether or not a hand is drawn
    /// (engine use, as `body_texture`).
    #[serde(skip)]
    pub hand_texture: Option<String>,
    /// The weapon the actor draws (part H, §9). Not part of `fnv.actor.appearance/1`.
    #[serde(skip)]
    pub weapon: Option<FormId>,
    /// The mod flags of that weapon's unit (`Extra::mods`): its model is the one for them
    /// (`Weapon::model_for`, docs/spec-notes/weapon-mods.md §3). 0 as resolved from the
    /// data; the engine sets it from the simulation's inventory. Not part of
    /// `fnv.actor.appearance/1`.
    #[serde(skip)]
    pub weapon_mods: u8,
    /// Whether a leveled choice that could have gone another way decided the actor base
    /// or its worn armour (§2.1): such actors depend on the random stream, the others are
    /// fully determined by the data.
    pub random: bool,
}

impl Appearance {
    /// The race skin texture a skin shape of a worn model takes in place of the diffuse
    /// texture it names, `shape_texture` ([`worn_skin`]); `None`: the shape keeps its own
    /// (also when the race has no such texture).
    pub fn worn_skin_texture(&self, shape_texture: Option<&str>) -> Option<&str> {
        match worn_skin(shape_texture)? {
            WornSkin::Body => self.body_texture.as_deref(),
            WornSkin::Hand => self.hand_texture.as_deref(),
        }
    }
}

/// What a placed creature is drawn from (docs/spec-notes/actors.md §10).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreatureAppearance {
    /// The creature base after leveled resolution.
    pub creature: FormId,
    /// The creature its Model/Animation group comes from (itself, or a template's pick),
    /// whose body part data (`PNAM`) goes with the skeleton drawn
    /// (docs/spec-notes/dismemberment.md §2.6). Not in `fnvtool actor creatures`' output.
    #[serde(skip)]
    pub body: FormId,
    pub edid: Option<String>,
    /// DATA type of the Model/Animation source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Skeleton, archive path.
    pub skeleton: String,
    /// BNAM of the Stats source; 1 when absent.
    pub base_scale: f64,
    /// The NIFZ models, role `body`, in record order.
    pub parts: Vec<Part>,
    /// KFFZ names, lower case.
    pub anims: Vec<String>,
    /// Whether a leveled pick could have gone another way.
    pub random: bool,
    /// The creature the Model/Animation template group came from (`creature` itself
    /// without that flag): where a template-only creature's sounds are read
    /// (`anim_sounds::creature_sounds`; docs/spec-notes/audio.md §14.1). Not part of any
    /// report.
    #[serde(skip)]
    pub model_source: FormId,
}

/// Archive path of a creature's NIFZ entry: the name in the skeleton's folder
/// [observed: docs/spec-notes/animation.md §4].
pub fn creature_part_path(skeleton: &str, name: &str) -> String {
    let folder = skeleton.rsplit_once('\\').map_or("", |(f, _)| f);
    let name = name.trim().replace('/', "\\").to_ascii_lowercase();
    format!("{folder}\\{}", name.trim_start_matches('\\'))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ResolveError {
    NotAnActor(FormId),
    /// A creature base given to the NPC resolver ([`Resolver::resolve_creature`]
    /// handles it).
    Creature(FormId),
    LeveledEmpty(FormId),
    /// The leveled list's Chance none came up: no actor.
    LeveledNone(FormId),
    TooDeep(FormId),
    NoRace(FormId),
    UnknownRace(FormId),
    /// A creature was asked for, but the base is an NPC (or leveled NPC).
    NotACreature(FormId),
    /// A creature whose Model/Animation source names no skeleton, or an editor
    /// placeholder.
    NoSkeleton(FormId),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NotAnActor(id) => write!(f, "{id} is not an actor base"),
            ResolveError::Creature(id) => write!(f, "{id} is a creature (not handled yet)"),
            ResolveError::LeveledEmpty(id) => write!(f, "leveled actor {id} has no eligible entry"),
            ResolveError::LeveledNone(id) => write!(f, "leveled actor {id} rolled its chance none"),
            ResolveError::TooDeep(id) => write!(f, "template or leveled chain too deep at {id} (cycle?)"),
            ResolveError::NoRace(id) => write!(f, "{id} has no race"),
            ResolveError::UnknownRace(id) => write!(f, "race {id} is not loaded"),
            ResolveError::NotACreature(id) => write!(f, "{id} is not a creature base"),
            ResolveError::NoSkeleton(id) => write!(f, "creature {id} names no skeleton"),
        }
    }
}

/// Archive path of a model given relative to `meshes\` (or already under it).
pub fn mesh_path(p: &str) -> String {
    archive_path("meshes\\", p)
}

/// Archive path of a texture given relative to `textures\` (or already under it).
pub fn texture_path(p: &str) -> String {
    archive_path("textures\\", p)
}

impl Appearance {
    /// The FaceGen texture coefficients that tint the actor's skin (face, body, hands):
    /// its own FGTS, plus its race's for its sex value by value when `with_race`
    /// [hypothesis: the race's values are a baseline the actor's are added to,
    /// docs/spec-notes/actors.md §4.2]. `None` for an actor without FaceGen data.
    pub fn skin_fgts(&self, with_race: bool) -> Option<Vec<f64>> {
        let own = &self.facegen.as_ref()?.fgts;
        Some(match self.race_facegen.as_ref().filter(|_| with_race) {
            Some(race) => crate::facegen::add_coefficients(own, &race.fgts),
            None => own.clone(),
        })
    }

    /// The FaceGen coefficients that shape the actor's face and the parts fitted to it
    /// (FGGS and FGGA; FGTS left empty, see [`Appearance::skin_fgts`]): its own, plus its
    /// race's for its sex value by value when `with_race` [hypothesis, checked against
    /// the owner's in-game picture of Sunny Smiles, docs/spec-notes/actors.md §4.3].
    /// `None` for an actor without FaceGen data.
    pub fn shape_facegen(&self, with_race: bool) -> Option<FaceGen> {
        let own = self.facegen.as_ref()?;
        let add = crate::facegen::add_coefficients;
        let (fggs, fgga) = match self.race_facegen.as_ref().filter(|_| with_race) {
            Some(race) => (add(&own.fggs, &race.fggs), add(&own.fgga, &race.fgga)),
            None => (own.fggs.clone(), own.fgga.clone()),
        };
        Some(FaceGen { fggs, fgga, fgts: Vec::new() })
    }
}

/// The `.egt` of a race hand, from the race's body `.egt` (an archive path): beside it,
/// `lefthand<kind>.egt` or `righthand<kind>.egt`, `<kind>` being what follows
/// `upperbody` and an optional `human` in the body file's name (`upperbodyhumanmale.egt`
/// → `lefthandmale.egt`, `upperbodychildfemale.egt` → `righthandchildfemale.egt`)
/// [hypothesis from the file names: no record names the hand files; every body `.egt`
/// but `upperbodyhumanghoul.egt`, whose kind has hand files for both sexes, pairs with
/// existing hand files this way, docs/spec-notes/actors.md §4.1].
pub fn hand_egt(body_egt: &str, left: bool) -> Option<String> {
    let (dir, file) = body_egt.rsplit_once('\\')?;
    let kind = file.strip_suffix(".egt")?.strip_prefix("upperbody")?;
    let kind = kind.strip_prefix("human").unwrap_or(kind);
    Some(format!("{dir}\\{}hand{kind}.egt", if left { "left" } else { "right" }))
}

/// Which of the race's skin textures a skin shape of a worn model takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WornSkin {
    /// The race's upper body texture (body index 0 ICON).
    Body,
    /// The race's hand texture (body index 1 or 2 ICON).
    Hand,
}

/// The race skin texture that a skin shape (shader type 14) of a worn model takes in
/// place of the diffuse texture it names, `texture`; `None`: it keeps its own
/// [hypothesis, docs/spec-notes/actors.md §3.1]. Decided by the file name of the named
/// texture when it lies in a `characters` folder: `hand…` is hand skin; `upperbody…` and
/// `head…` are body skin, and so is a shape that names no texture. Every skin shape of
/// the worn models in `Fallout - Meshes.bsa` falls under one of these [observed:
/// `crates/fnv-nif/examples/skin_survey.rs`]; a skin shape naming any other texture
/// keeps it.
pub fn worn_skin(texture: Option<&str>) -> Option<WornSkin> {
    let Some(texture) = texture.map(str::trim).filter(|t| !t.is_empty()) else { return Some(WornSkin::Body) };
    let t = texture.replace('/', "\\").to_ascii_lowercase();
    let mut folders: Vec<&str> = t.split('\\').collect();
    let file = folders.pop()?;
    if !folders.contains(&"characters") {
        return None;
    }
    if file.starts_with("hand") {
        Some(WornSkin::Hand)
    } else if file.starts_with("upperbody") || file.starts_with("head") {
        Some(WornSkin::Body)
    } else {
        None
    }
}

fn archive_path(root: &str, p: &str) -> String {
    let p = p.trim().replace('/', "\\").to_ascii_lowercase();
    let p = p.trim_start_matches('\\');
    if p.starts_with(root) {
        p.to_string()
    } else {
        format!("{root}{p}")
    }
}

/// Deterministic per-reference random stream for leveled choices (not the game's roll).
pub fn rng_for(reference: FormId) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(u64::from(reference.0))
}

/// The leveled choices of one actor: each LVLN is picked once (all template groups that
/// name it see the same actor), and whether any choice could have gone another way.
struct Choices {
    rng: ChaCha8Rng,
    /// The level leveled lists are read at (fnv_sim::zones; [`DISPLAY_LEVEL`] for a
    /// static display).
    at: LevelAt,
    picks: HashMap<FormId, FormId>,
    random: bool,
}

/// A chosen look: what the character creator sets in place of the actor record's fields
/// (docs/spec-notes/character-creator.md). `hair` `None` is no hair (bald), not the race's
/// default; `hair_color` `None` keeps the record's.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Look {
    pub race: FormId,
    pub female: bool,
    pub hair: Option<FormId>,
    pub hair_color: Option<[u8; 3]>,
    pub eyes: Option<FormId>,
    pub head_parts: Vec<FormId>,
    pub facegen: FaceGen,
}

/// A script's change of an actor's race or sex (AgeRace, SexChange; `fnv_game::
/// appearance`): `None` keeps the record's (or the look's).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Change {
    pub race: Option<FormId>,
    pub female: Option<bool>,
}

impl Change {
    pub fn is_none(&self) -> bool {
        self.race.is_none() && self.female.is_none()
    }
}

pub struct Resolver<'a> {
    pub actors: &'a ActorForms,
    pub forms: &'a FormStore,
}

impl<'a> Resolver<'a> {
    /// `hair` when the race lists it (or lists none) and it is for that sex, else the
    /// race's default hair for the sex (`Self::resolve_changed`).
    fn valid_hair(&self, hair: Option<FormId>, race: Option<FormId>, female: bool) -> Option<FormId> {
        let Some(race) = race.and_then(|r| self.actors.races.get(&r)) else { return hair };
        let Some(h) = hair else { return race.default_hair[usize::from(female)] };
        let listed = race.hairs.is_empty() || race.hairs.contains(&h);
        let sex_ok = self.actors.hairs.get(&h).is_none_or(|d| if female { !d.choosable.not_female } else { !d.choosable.not_male });
        if listed && sex_ok {
            Some(h)
        } else {
            race.default_hair[usize::from(female)]
        }
    }

    /// Resolve an actor base placed by `reference` (the seed of its leveled choices).
    pub fn resolve(&self, base: FormId, reference: FormId) -> Result<Appearance, ResolveError> {
        self.resolve_wearing(base, reference, None)
    }

    /// [`Self::resolve`], with the armour worn given (`Some`: e.g. what the simulation
    /// has equipped on the player) instead of chosen from the carried items; entries that
    /// are not ARMO records are ignored, except a weapon held with "Hide Backpack".
    pub fn resolve_wearing(&self, base: FormId, reference: FormId, wearing: Option<&[FormId]>) -> Result<Appearance, ResolveError> {
        self.resolve_as(base, reference, wearing, None)
    }

    /// [`Self::resolve_wearing`], with the race, sex, hair, eyes, head parts, hair colour
    /// and FaceGen values of `look` in place of the record's (the player's choices in the
    /// character creator, docs/spec-notes/character-creator.md).
    pub fn resolve_as(&self, base: FormId, reference: FormId, wearing: Option<&[FormId]>, look: Option<&Look>) -> Result<Appearance, ResolveError> {
        self.resolve_changed(base, reference, wearing, look, Change::default())
    }

    /// [`Self::resolve_as`] with a script's race or sex change on top (AgeRace,
    /// SexChange). The new race's head, body and race FaceGen are drawn; a hair the new
    /// race does not list (RACE HNAM) or that is not for the new sex (HAIR Not Male, Not
    /// Female) gives way to the race's default hair for that sex [documented: GECK wiki
    /// SexChange and AgeRace notes, "invalid hair falls back to default"; hypothesis: what
    /// makes a hair invalid]. A chosen look (the player's) keeps its hair.
    pub fn resolve_changed(&self, base: FormId, reference: FormId, wearing: Option<&[FormId]>, look: Option<&Look>, change: Change) -> Result<Appearance, ResolveError> {
        self.resolve_changed_at(base, reference, wearing, look, change, LevelAt::plain(DISPLAY_LEVEL))
    }

    /// [`Self::resolve_changed`] with leveled lists read at `at` (the reference's level in
    /// the game, `fnv_sim::zones`; docs/spec-notes/encounter-zones.md §3) instead of the
    /// display level.
    pub fn resolve_changed_at(&self, base: FormId, reference: FormId, wearing: Option<&[FormId]>, look: Option<&Look>, change: Change, at: LevelAt) -> Result<Appearance, ResolveError> {
        let mut ch = Choices { rng: rng_for(reference), at, picks: HashMap::new(), random: false };
        let npc = self.actor_base(base, &mut ch, 0)?;
        let traits = self.template_source(npc, template::TRAITS, &mut ch)?;
        let model = self.template_source(npc, template::MODEL_ANIMATION, &mut ch)?;
        let inventory = self.template_source(npc, template::INVENTORY, &mut ch)?;
        let (traits_own, model_own);
        let (traits, model) = match look {
            Some(l) => {
                traits_own = Npc { race: Some(l.race), female: l.female, ..traits.clone() };
                model_own = Npc {
                    hair: l.hair,
                    hair_color: l.hair_color.or(model.hair_color),
                    eyes: l.eyes,
                    head_parts: l.head_parts.clone(),
                    facegen: Some(l.facegen.clone()),
                    ..model.clone()
                };
                (&traits_own, &model_own)
            }
            None => (traits, model),
        };
        let (changed_traits, changed_model);
        let (traits, model) = if change.is_none() {
            (traits, model)
        } else {
            changed_traits = Npc { race: change.race.or(traits.race), female: change.female.unwrap_or(traits.female), ..traits.clone() };
            let mut m = model.clone();
            if look.is_none() {
                m.hair = self.valid_hair(m.hair, changed_traits.race, changed_traits.female);
            }
            changed_model = m;
            (&changed_traits, &changed_model)
        };

        let race_id = traits.race.ok_or(ResolveError::NoRace(traits.id))?;
        let race = self.actors.races.get(&race_id).ok_or(ResolveError::UnknownRace(race_id))?;
        let female = traits.female;
        let sex = usize::from(female);
        // Skeleton (§2.2): an NPC_ naming no skeleton or the editor placeholder
        // `marker_creature.nif` takes the traits source's, else the human skeleton
        // [hypothesis: every other NPC_ names `characters\_male\skeleton.nif`].
        let named = |n: &Npc| n.skeleton.as_deref().filter(|s| !is_editor_marker(s)).map(mesh_path);
        let skeleton = named(model).or_else(|| named(traits)).unwrap_or_else(|| HUMAN_SKELETON.to_string());

        let carried = self.carried(inventory.id, &mut ch);
        let worn = match wearing {
            Some(w) => w.iter().copied().filter(|id| self.actors.armors.contains_key(id)).collect(),
            None => self.choose_armor(&carried),
        };
        let weapon = crate::weapon::choose_weapon(&carried, &self.actors.weapons);
        // A weapon with "Hide Backpack" hides the pieces in the Backpack slot ("hides any
        // equipped armor or armor addon that occupies the Backpack biped slot"
        // [documented: GECK wiki Weapons]; the power armours' backpack addons under a
        // minigun's or flamer's own tank): the weapon chosen, or one given among `wearing`.
        let hides_backpack = |id: &FormId| self.actors.weapons.get(id).is_some_and(|w| w.hide_backpack);
        let hide_backpack = match wearing {
            Some(w) => w.iter().any(hides_backpack),
            None => weapon.as_ref().is_some_and(hides_backpack),
        };
        // The worn items and the armour addons their biped model lists name (ARMO BIPL,
        // `records::Armor::addons`): an addon's slots are covered and its model drawn as
        // the item's. One hand model per hand (actors.md §2.3, #1067) [hypothesis: the
        // rules; observed: FalloutNV.esm]:
        // - an addon that is the race's own bare hand (the vault suits' `BaseFemale` list:
        //   `lefthand.nif`, `righthand.nif`) is left out when another worn item takes its
        //   slot itself, as the race's hand is: the Pip-Boy Glove's hand is drawn alone;
        // - a worn item whose every slot another item's addons take is left out: the
        //   Pip-Boy Glove under an outfit's glove or a power armour's gauntlet.
        let own_slots = |id: &FormId| self.actors.armors.get(id).map_or(0, |a| a.slots);
        let race_hands: Vec<String> = race.body[sex].iter().filter(|p| matches!(p.index, 1 | 2)).filter_map(|p| p.model.as_deref().map(mesh_path)).collect();
        let bare_hand = |id: &FormId| {
            let a = self.actors.armors.get(id);
            a.and_then(|a| a.worn[sex].as_ref().or(a.worn[0].as_ref())).is_some_and(|m| race_hands.contains(&mesh_path(m)))
        };
        // (piece, the worn item naming it when it is an addon)
        let mut pieces: Vec<(FormId, Option<FormId>)> = Vec::new();
        for w in &worn {
            pieces.push((*w, None));
            let others = worn.iter().filter(|o| *o != w).fold(0, |acc, o| acc | own_slots(o));
            for ad in self.actors.armors.get(w).map(|a| a.addons.clone()).unwrap_or_default() {
                if !(bare_hand(&ad) && own_slots(&ad) & others != 0) {
                    pieces.push((ad, Some(*w)));
                }
            }
        }
        // The held weapon's addons (WEAP BIPL, `records::Weapon::addons`), worn as an
        // outfit's are: a hand-to-hand weapon's hand model (the power fist's metal hand,
        // the brass knuckles' hand with the knuckles) covers the Right Hand slot, so the
        // race's bare hand is not drawn through it, and a worn addon that covers any slot
        // it takes (an outfit's glove, for one hand or both) is left out, one hand model
        // per hand
        // [observed: the original draws no bare hand at the power fist's or knuckles'
        // front, capture pf_h2h_rest3 2026-11-06, actors.md §9.2; hypothesis: worn while
        // the weapon is equipped, drawn or not, as h2h weapons hang on the forearm both
        // ways].
        let held: Vec<FormId> = match wearing {
            Some(w) => w.iter().copied().filter(|id| self.actors.weapons.contains_key(id)).collect(),
            None => weapon.into_iter().collect(),
        };
        for h in &held {
            let addons: Vec<FormId> = self.actors.weapons.get(h).map(|w| w.addons.clone()).unwrap_or_default();
            let taken = addons.iter().fold(0, |acc, a| acc | own_slots(a));
            if taken != 0 {
                pieces.retain(|(id, owner)| owner.is_none() || own_slots(id) & taken == 0);
            }
            for ad in addons.into_iter().filter(|a| self.actors.armors.contains_key(a)) {
                pieces.push((ad, Some(*h)));
            }
        }
        let by_others_addons = |id: &FormId| pieces.iter().filter(|(_, o)| o.is_some_and(|o| o != *id)).fold(0, |acc, (a, _)| acc | own_slots(a));
        let drawn: Vec<FormId> = pieces
            .iter()
            .filter(|(id, owner)| owner.is_some() || own_slots(id) == 0 || own_slots(id) & !by_others_addons(id) != 0)
            .map(|(id, _)| *id)
            .filter(|id| self.actors.armors.get(id).is_some_and(|a| !(hide_backpack && a.slots & slot::BACKPACK != 0)))
            .fold(Vec::new(), |mut v, id: FormId| {
                if !v.contains(&id) {
                    v.push(id);
                }
                v
            });
        let covered = drawn.iter().filter_map(|w| self.actors.armors.get(w)).fold(0, |acc, a| acc | a.slots);

        let mut parts = Vec::new();
        let mut body_egt = None;
        let mut body_texture = None;
        let mut hand_texture = None;
        for p in &race.body[sex] {
            if p.index == 0 {
                body_texture = p.icon.as_deref().map(texture_path);
            }
            if matches!(p.index, 1 | 2) && hand_texture.is_none() {
                hand_texture = p.icon.as_deref().map(texture_path);
            }
            let (role, hidden_by) = match p.index {
                0 => ("body:upper_body", slot::UPPER_BODY),
                1 => ("body:left_hand", slot::LEFT_HAND),
                2 => ("body:right_hand", slot::RIGHT_HAND),
                3 => {
                    body_egt = p.model.as_deref().map(mesh_path);
                    continue;
                }
                _ => continue,
            };
            if covered & hidden_by == 0 {
                push_race_part(&mut parts, role.to_string(), race_id, p, None);
            }
        }
        let head_covered = covered & slot::HEAD != 0;
        let eye_texture = model.eyes.and_then(|e| self.actors.eyes.get(&e)).and_then(|e| e.texture.as_deref()).map(texture_path);
        if !head_covered {
            for p in &race.head[sex] {
                let role = format!("head:{}", head_part_name(p.index));
                // The eye texture comes from the actor's EYES (the race's eye parts have
                // no ICON [observed]).
                let texture = if matches!(p.index, 6 | 7) { eye_texture.clone() } else { None };
                push_race_part(&mut parts, role, race_id, p, texture);
            }
            for &hp in &model.head_parts {
                self.push_head_part(&mut parts, hp, 0, &mut HashSet::new());
            }
        }
        // Hair (§2.4): gone with the head; under headgear covering Hat, the hair model's
        // `Hat` shape; replaced (e.g. a wig) when Hair is covered without Hat.
        // A chosen look without hair is bald, not the race's default.
        let hair = if look.is_some() { model.hair } else { model.hair.or(race.default_hair[sex]) };
        let hat = covered & slot::HAT != 0;
        if !head_covered && (hat || covered & slot::HAIR == 0) {
            if let Some((id, h)) = hair.and_then(|id| Some((id, self.actors.hairs.get(&id)?))) {
                if let Some(m) = &h.model {
                    parts.push(Part { role: "hair".into(), form: id, model: mesh_path(m), texture: h.texture.as_deref().map(texture_path) });
                }
            }
        }
        for &w in &drawn {
            let Some(a) = self.actors.armors.get(&w) else { continue };
            // A female actor wears MOD3, falling back to the male model when the item has
            // none.
            if let Some(m) = a.worn[sex].as_ref().or(a.worn[0].as_ref()) {
                parts.push(Part { role: "armor".into(), form: w, model: mesh_path(m), texture: None });
            }
        }
        Ok(Appearance {
            npc: npc.id,
            edid: npc.edid.clone(),
            female,
            race: race_id,
            skeleton,
            race_height: race.height[sex],
            height: npc.height.filter(|h| *h > 0.0).unwrap_or(1.0),
            parts,
            worn,
            hat,
            hair_color: model.hair_color,
            facegen: model.facegen.clone(),
            race_facegen: race.facegen[sex].clone(),
            body_egt,
            body_texture,
            hand_texture,
            weapon,
            weapon_mods: 0,
            random: ch.random,
        })
    }

    /// A head part and its extra parts, each once (`seen`): a part naming itself or a
    /// part already in (a broken mod) adds nothing more.
    fn push_head_part(&self, parts: &mut Vec<Part>, id: FormId, depth: u32, seen: &mut HashSet<FormId>) {
        if !seen.insert(id) {
            return;
        }
        let Some(h) = self.actors.head_parts.get(&id) else { return };
        if let Some(m) = &h.model {
            parts.push(Part { role: "head_part".into(), form: id, model: mesh_path(m), texture: None });
        }
        if depth < MAX_DEPTH {
            for &e in &h.extra {
                self.push_head_part(parts, e, depth + 1, seen);
            }
        }
    }

    /// The actor a leveled list (LVLN or LVLC) stands for when it is the first list met
    /// for `reference` (§2.1); `None` for a form that is no list or has nothing to pick.
    /// For checks that other code reading the list picks the same entry
    /// (`fnv_game::templates`).
    pub fn first_pick(&self, list: FormId, reference: FormId) -> Option<FormId> {
        self.first_pick_at(list, reference, LevelAt::plain(DISPLAY_LEVEL))
    }

    /// [`Self::first_pick`] at the level `at`.
    pub fn first_pick_at(&self, list: FormId, reference: FormId, at: LevelAt) -> Option<FormId> {
        let mut ch = Choices { rng: rng_for(reference), at, picks: HashMap::new(), random: false };
        if self.actors.leveled.contains_key(&list) {
            self.actor_base(list, &mut ch, 0).ok().map(|n| n.id)
        } else if self.actors.leveled_creatures.contains_key(&list) {
            self.creature_base(list, &mut ch, 0).ok().map(|c| c.id)
        } else {
            None
        }
    }

    /// An NPC_ base for `id`, resolving leveled NPCs (LVLN) once per actor.
    fn actor_base(&self, id: FormId, ch: &mut Choices, depth: u32) -> Result<&'a Npc, ResolveError> {
        if depth > MAX_DEPTH {
            return Err(ResolveError::TooDeep(id));
        }
        if let Some(n) = self.actors.npcs.get(&id) {
            return Ok(n);
        }
        if let Some(l) = self.actors.leveled.get(&id) {
            let next = match ch.picks.get(&id) {
                Some(&p) => p,
                None => {
                    let (pool, _) = leveled_pool(&l.entries, ch.at.all(l.all_levels), ch.at.level);
                    ch.random |= distinct(pool.iter().map(|e| e.1)) > 1;
                    // Chance none (docs/spec-notes/actor-flags.md §6), as `fnv_game::templates`.
                    if !pool.is_empty() && leveled::rolls_none(&mut ch.rng, l.chance_none) {
                        return Err(ResolveError::LeveledNone(id));
                    }
                    let p = pick_leveled(&l.entries, ch.at.all(l.all_levels), ch.at.level, &mut ch.rng).ok_or(ResolveError::LeveledEmpty(id))?;
                    ch.picks.insert(id, p);
                    p
                }
            };
            return self.actor_base(next, ch, depth + 1);
        }
        match self.actors.sigs.get(&id) {
            Some(b"CREA") | Some(b"LVLC") => Err(ResolveError::Creature(id)),
            _ => Err(ResolveError::NotAnActor(id)),
        }
    }

    /// The NPC whose data a template-flag group comes from: follow `TPLT` while the
    /// current NPC has the group's flag set (§2.2).
    fn template_source(&self, npc: &'a Npc, group: u16, ch: &mut Choices) -> Result<&'a Npc, ResolveError> {
        let mut cur = npc;
        for _ in 0..MAX_DEPTH {
            match cur.template {
                Some(t) if cur.template_flags & group != 0 => cur = self.actor_base(t, ch, 0)?,
                _ => return Ok(cur),
            }
        }
        Err(ResolveError::TooDeep(npc.id))
    }

    /// Resolve a creature base placed by `reference` (actors.md §10): an LVLC is picked
    /// like an LVLN (§2.1); the skeleton, model list and animation list come from the
    /// Model/Animation source of the template chain, the base scale from the Stats
    /// source.
    pub fn resolve_creature(&self, base: FormId, reference: FormId) -> Result<CreatureAppearance, ResolveError> {
        self.resolve_creature_at(base, reference, LevelAt::plain(DISPLAY_LEVEL))
    }

    /// [`Self::resolve_creature`] with leveled lists read at `at`.
    pub fn resolve_creature_at(&self, base: FormId, reference: FormId, at: LevelAt) -> Result<CreatureAppearance, ResolveError> {
        let mut ch = Choices { rng: rng_for(reference), at, picks: HashMap::new(), random: false };
        let crea = self.creature_base(base, &mut ch, 0)?;
        let model = self.creature_source(crea, template::MODEL_ANIMATION, &mut ch)?;
        let stats = self.creature_source(crea, template::STATS, &mut ch)?;
        let skeleton =
            model.skeleton.as_deref().filter(|s| !is_editor_marker(s)).map(mesh_path).ok_or(ResolveError::NoSkeleton(model.id))?;
        let parts = model
            .models
            .iter()
            .map(|m| Part { role: "body".into(), form: model.id, model: creature_part_path(&skeleton, m), texture: None })
            .collect();
        Ok(CreatureAppearance {
            creature: crea.id,
            body: model.id,
            edid: crea.edid.clone(),
            kind: model.kind.clone(),
            skeleton,
            base_scale: stats.base_scale.unwrap_or(1.0),
            parts,
            anims: model.anims.iter().map(|a| a.replace('/', "\\").to_ascii_lowercase()).collect(),
            random: ch.random,
            model_source: model.id,
        })
    }

    /// A CREA base for `id`, resolving leveled creatures (LVLC) once per actor.
    fn creature_base(&self, id: FormId, ch: &mut Choices, depth: u32) -> Result<&'a Creature, ResolveError> {
        if depth > MAX_DEPTH {
            return Err(ResolveError::TooDeep(id));
        }
        if let Some(c) = self.actors.creatures.get(&id) {
            return Ok(c);
        }
        if let Some(l) = self.actors.leveled_creatures.get(&id) {
            let next = match ch.picks.get(&id) {
                Some(&p) => p,
                None => {
                    let (pool, _) = leveled_pool(&l.entries, ch.at.all(l.all_levels), ch.at.level);
                    ch.random |= distinct(pool.iter().map(|e| e.1)) > 1;
                    // Chance none (docs/spec-notes/actor-flags.md §6), as `fnv_game::templates`.
                    if !pool.is_empty() && leveled::rolls_none(&mut ch.rng, l.chance_none) {
                        return Err(ResolveError::LeveledNone(id));
                    }
                    let p = pick_leveled(&l.entries, ch.at.all(l.all_levels), ch.at.level, &mut ch.rng).ok_or(ResolveError::LeveledEmpty(id))?;
                    ch.picks.insert(id, p);
                    p
                }
            };
            return self.creature_base(next, ch, depth + 1);
        }
        match self.actors.sigs.get(&id) {
            Some(b"NPC_") | Some(b"LVLN") => Err(ResolveError::NotACreature(id)),
            _ => Err(ResolveError::NotAnActor(id)),
        }
    }

    /// The creature whose data a template-flag group comes from (as
    /// [`Self::template_source`] for NPCs).
    fn creature_source(&self, c: &'a Creature, group: u16, ch: &mut Choices) -> Result<&'a Creature, ResolveError> {
        let mut cur = c;
        for _ in 0..MAX_DEPTH {
            match cur.template {
                Some(t) if cur.template_flags & group != 0 => cur = self.creature_base(t, ch, 0)?,
                _ => return Ok(cur),
            }
        }
        Err(ResolveError::TooDeep(c.id))
    }

    /// The carried items of `npc`, leveled lists resolved, in carried order.
    fn carried(&self, npc: FormId, ch: &mut Choices) -> Vec<FormId> {
        let Some(entries) = self.forms.entries(npc) else { return Vec::new() };
        let mut carried = Vec::new();
        for e in entries {
            ch.random |= e.count > 0 && leveled_item_random(self.forms, e.item, 0);
            // A leveled list that fails to resolve contributes nothing; the store
            // reports cycles when it loads.
            let _ = leveled::resolve(self.forms, e.item, e.count, ch.at.level, &mut ch.rng, &mut carried);
        }
        carried.iter().map(|e| e.item).collect()
    }

    /// Worn armour from the carried items (§2.3).
    fn choose_armor(&self, carried: &[FormId]) -> Vec<FormId> {
        let mut candidates: Vec<(usize, FormId)> = Vec::new();
        for (order, &item) in carried.iter().enumerate() {
            let is_armor = matches!(self.forms.get(item).map(|b| &b.data), Some(BaseData::Item(i)) if i.kind == ItemKind::Armor);
            if is_armor && self.actors.armors.contains_key(&item) && !candidates.iter().any(|c| c.1 == item) {
                candidates.push((order, item));
            }
        }
        let armors: BTreeMap<FormId, (f32, i32, u32)> = candidates
            .iter()
            .filter_map(|&(_, id)| self.actors.armors.get(&id).map(|a| (id, (a.dt, a.dr_raw, a.slots))))
            .collect();
        choose_by_protection(&candidates, &armors)
    }
}

/// Best protection first: DT descending, then DR descending, then carried order; an item
/// is worn unless an already chosen one shares a body slot. Items with no slot are not
/// worn.
pub fn choose_by_protection(candidates: &[(usize, FormId)], armors: &BTreeMap<FormId, (f32, i32, u32)>) -> Vec<FormId> {
    let mut sorted: Vec<_> = candidates.iter().filter_map(|&(order, id)| Some((order, id, *armors.get(&id)?))).collect();
    sorted.sort_by(|a, b| {
        b.2 .0.total_cmp(&a.2 .0).then(b.2 .1.cmp(&a.2 .1)).then(a.0.cmp(&b.0))
    });
    let mut used = 0u32;
    let mut worn = Vec::new();
    for (_, id, (_, _, slots)) in sorted {
        if slots == 0 || used & slots != 0 {
            continue;
        }
        used |= slots;
        worn.push(id);
    }
    worn
}

/// The entries a leveled pick chooses from (first value), and the eligible entries
/// (second): eligible entries have `level <= level`; the pool is all of them with
/// "Calculate from all levels", else those of the highest eligible level.
pub type Pool<T> = (Vec<(u16, T)>, Vec<(u16, T)>);

pub fn leveled_pool<T: Copy>(entries: &[(u16, T)], all_levels: bool, level: u16) -> Pool<T> {
    let eligible: Vec<_> = entries.iter().copied().filter(|e| e.0 <= level).collect();
    let pool = if all_levels {
        eligible.clone()
    } else {
        match eligible.iter().map(|e| e.0).max() {
            Some(top) => eligible.iter().copied().filter(|e| e.0 == top).collect(),
            None => Vec::new(),
        }
    };
    (pool, eligible)
}

fn distinct(ids: impl Iterator<Item = FormId>) -> usize {
    ids.collect::<std::collections::BTreeSet<_>>().len()
}

/// One entry of a leveled NPC list, uniformly from [`leveled_pool`], as for leveled items
/// (fnv-sim `leveled`). Chance none is not applied: a static display always shows an
/// actor (a display choice, not a claim about the game).
pub fn pick_leveled(entries: &[(u16, FormId)], all_levels: bool, level: u16, rng: &mut ChaCha8Rng) -> Option<FormId> {
    let (pool, _) = leveled_pool(entries, all_levels, level);
    if pool.is_empty() {
        return None;
    }
    Some(pool[leveled::below(rng, pool.len() as u32) as usize].1)
}

/// Whether resolving `item` at the display level can change which armour it yields: it
/// can yield armour at all, and involves a leveled item list with a chance none above 0
/// or an LVLG global, a pool of more than one distinct entry, or such a list among the
/// entries it can produce. Plain items are not random.
pub fn leveled_item_random(forms: &FormStore, item: FormId, depth: u32) -> bool {
    random_list(forms, item, depth, &mut HashSet::new())
}

/// [`leveled_item_random`], each list looked at once (`seen`): a list met again has
/// already had its say, so lists naming themselves or each other (a broken mod) cost
/// one look each, not one per path.
fn random_list(forms: &FormStore, item: FormId, depth: u32, seen: &mut HashSet<FormId>) -> bool {
    let Some(list) = forms.leveled(item) else { return false };
    if depth > MAX_DEPTH {
        return true;
    }
    if !seen.insert(item) {
        return false;
    }
    if !yields_armor(forms, item, 0, &mut HashSet::new()) {
        return false;
    }
    let entries: Vec<(u16, FormId)> = list.entries.iter().map(|e| (e.level, e.item)).collect();
    let (pool, eligible) = leveled_pool(&entries, list.all_levels, DISPLAY_LEVEL);
    if list.chance_none > 0 || list.global.is_some() {
        return true;
    }
    let reachable = if list.use_all { eligible } else { pool };
    if !list.use_all && distinct(reachable.iter().map(|e| e.1)) > 1 {
        return true;
    }
    reachable.iter().any(|e| random_list(forms, e.1, depth + 1, seen))
}

/// Whether `item` is armour or a leveled list that can produce armour at the display
/// level.
/// Each list looked at once (`seen`), as in [`random_list`].
fn yields_armor(forms: &FormStore, item: FormId, depth: u32, seen: &mut HashSet<FormId>) -> bool {
    if let Some(list) = forms.leveled(item) {
        if !seen.insert(item) {
            return false;
        }
        let entries: Vec<(u16, FormId)> = list.entries.iter().map(|e| (e.level, e.item)).collect();
        let (pool, eligible) = leveled_pool(&entries, list.all_levels, DISPLAY_LEVEL);
        let reachable = if list.use_all { eligible } else { pool };
        return depth <= MAX_DEPTH && reachable.iter().any(|e| yields_armor(forms, e.1, depth + 1, seen));
    }
    matches!(forms.get(item).map(|b| &b.data), Some(BaseData::Item(i)) if i.kind == ItemKind::Armor)
}

fn push_race_part(parts: &mut Vec<Part>, role: String, race: FormId, p: &RacePart, texture: Option<String>) {
    // Ears (index 1) have a texture but no model [observed: CaucasianOld].
    let Some(model) = &p.model else { return };
    let texture = texture.or_else(|| p.icon.as_deref().map(texture_path));
    parts.push(Part { role, form: race, model: mesh_path(model), texture });
}

/// [documented: xEdit wbHeadPartIndexEnum]
pub fn head_part_name(index: u32) -> &'static str {
    match index {
        0 => "head",
        1 => "ears",
        2 => "mouth",
        3 => "teeth_lower",
        4 => "teeth_upper",
        5 => "tongue",
        6 => "left_eye",
        7 => "right_eye",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armors(list: &[(u32, f32, i32, u32)]) -> BTreeMap<FormId, (f32, i32, u32)> {
        list.iter().map(|&(id, dt, dr, slots)| (FormId(id), (dt, dr, slots))).collect()
    }

    #[test]
    fn best_dt_wins_a_shared_slot() {
        let a = armors(&[(1, 2.0, 0, slot::UPPER_BODY), (2, 12.0, 0, slot::UPPER_BODY)]);
        assert_eq!(choose_by_protection(&[(0, FormId(1)), (1, FormId(2))], &a), vec![FormId(2)]);
    }

    #[test]
    fn dr_breaks_dt_ties_then_carried_order() {
        let a = armors(&[(1, 5.0, 0, slot::UPPER_BODY), (2, 5.0, 300, slot::UPPER_BODY), (3, 5.0, 300, slot::UPPER_BODY)]);
        let c = [(0, FormId(1)), (1, FormId(3)), (2, FormId(2))];
        assert_eq!(choose_by_protection(&c, &a), vec![FormId(3)]);
    }

    #[test]
    fn disjoint_slots_are_all_worn() {
        let a = armors(&[(1, 6.0, 0, slot::UPPER_BODY), (2, 1.0, 0, slot::HAT), (3, 0.0, 0, 0)]);
        let c = [(0, FormId(2)), (1, FormId(1)), (2, FormId(3))];
        assert_eq!(choose_by_protection(&c, &a), vec![FormId(1), FormId(2)]);
    }

    #[test]
    fn partial_overlap_blocks_the_weaker_item() {
        let a = armors(&[(1, 3.0, 0, slot::HEAD | slot::HAIR), (2, 1.0, 0, slot::HAIR | slot::HAT)]);
        assert_eq!(choose_by_protection(&[(0, FormId(2)), (1, FormId(1))], &a), vec![FormId(1)]);
    }

    #[test]
    fn leveled_pick_prefers_highest_eligible_level() {
        let mut rng = rng_for(FormId(7));
        let e = [(1, FormId(10)), (5, FormId(11)), (1, FormId(12))];
        for _ in 0..20 {
            let p = pick_leveled(&e, false, 1, &mut rng).unwrap();
            assert!(p == FormId(10) || p == FormId(12));
        }
        assert_eq!(pick_leveled(&e, false, 9, &mut rng), Some(FormId(11)));
        assert_eq!(pick_leveled(&[(3, FormId(1))], false, 1, &mut rng), None);
    }

    /// Leveled lists naming themselves and each other many times (a broken mod) are
    /// looked at once each, not once per path (which never ended).
    #[test]
    fn self_naming_leveled_lists_end() {
        use fnv_sim::forms::{BaseForm, LeveledEntry, LeveledList};
        let mut forms = FormStore::default();
        for (id, other) in [(1u32, 2u32), (2, 1)] {
            let entries = (0..8)
                .map(|k| LeveledEntry { level: 1, item: FormId(if k % 2 == 0 { id } else { other }), count: 1, condition: None })
                .collect();
            let list = LeveledList { chance_none: 0, all_levels: false, each: false, use_all: false, global: None, entries };
            let base = BaseForm { id: FormId(id), sig: *b"LVLI", edid: None, name: None, script: None, prompt: None, voice_filter: false, data: BaseData::LeveledItem(list) };
            forms.bases.insert(FormId(id), base);
        }
        assert!(!leveled_item_random(&forms, FormId(1), 0));
    }

    #[test]
    fn editor_markers() {
        assert!(is_editor_marker("marker_creature.nif"));
        assert!(is_editor_marker(r"Meshes\Marker_X.NIF"));
        assert!(!is_editor_marker(r"Characters\_Male\Skeleton.NIF"));
    }

    #[test]
    fn creature_parts_sit_beside_the_skeleton() {
        let sk = mesh_path(r"creatures\NVBigHorner\skeleton.nif");
        assert_eq!(creature_part_path(&sk, "NVBigHorner.nif"), r"meshes\creatures\nvbighorner\nvbighorner.nif");
        assert_eq!(creature_part_path(&sk, " Sub/Part.NIF"), r"meshes\creatures\nvbighorner\sub\part.nif");
    }

    #[test]
    fn hand_egts_sit_beside_the_body_egt() {
        let body = |f: &str| mesh_path(&format!(r"Characters\_Male\{f}"));
        assert_eq!(hand_egt(&body("UpperBodyHumanMale.egt"), true).as_deref(), Some(r"meshes\characters\_male\lefthandmale.egt"));
        assert_eq!(hand_egt(&body("UpperBodyHumanOldFemale.egt"), false).as_deref(), Some(r"meshes\characters\_male\righthandoldfemale.egt"));
        assert_eq!(hand_egt(&body("UpperBodyChild.egt"), false).as_deref(), Some(r"meshes\characters\_male\righthandchild.egt"));
        assert_eq!(hand_egt(&body("Body.egt"), true), None);
        assert_eq!(hand_egt("upperbodyhumanmale.egt", true), None);
    }

    #[test]
    fn worn_skin_shapes_are_classed_by_the_file_name_of_their_texture() {
        // The textures the worn models' skin shapes name, as written in the files.
        assert_eq!(worn_skin(Some(r"textures\characters\male\UpperBodyMale.dds")), Some(WornSkin::Body));
        assert_eq!(worn_skin(Some(r"textures\characters\childfemale\upperbodyfemale.dds")), Some(WornSkin::Body));
        assert_eq!(worn_skin(Some(r"textures\characters\ghoul\UpperBodyMale.dds")), Some(WornSkin::Body));
        assert_eq!(worn_skin(Some(r"textures\characters\female\HandFemale.dds")), Some(WornSkin::Hand));
        assert_eq!(worn_skin(Some(r"textures\characters\female\handfemale_1st.dds")), Some(WornSkin::Hand));
        // combatarmor\m\outfitm.nif `Upperbody` names the head texture; two shapes none.
        assert_eq!(worn_skin(Some(r"textures\characters\male\HeadHuman.dds")), Some(WornSkin::Body));
        assert_eq!(worn_skin(None), Some(WornSkin::Body));
        assert_eq!(worn_skin(Some("")), Some(WornSkin::Body));
        // Other spellings of the same paths.
        assert_eq!(worn_skin(Some("Data/Textures/Characters/Male/HandMale.dds")), Some(WornSkin::Hand));
        assert_eq!(worn_skin(Some(r"characters\raider\upperbodymale.dds")), Some(WornSkin::Body));
        // Anything else keeps its own texture.
        assert_eq!(worn_skin(Some(r"textures\armor\underwear\underwearm.dds")), None);
        assert_eq!(worn_skin(Some(r"textures\armor\handwraps\handwraps.dds")), None);
        assert_eq!(worn_skin(Some(r"textures\characters\hair\hairbase.dds")), None);
        assert_eq!(worn_skin(Some("upperbodymale.dds")), None);
    }

    #[test]
    fn paths() {
        assert_eq!(mesh_path("Characters\\_Male\\Skeleton.NIF"), "meshes\\characters\\_male\\skeleton.nif");
        assert_eq!(mesh_path("meshes/a.nif"), "meshes\\a.nif");
        assert_eq!(texture_path("\\Characters\\Male\\HeadHumanOld.dds"), "textures\\characters\\male\\headhumanold.dds");
    }
}
