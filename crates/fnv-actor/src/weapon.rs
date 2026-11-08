//! Weapons in an actor's hands (workstream 5 part H, docs/spec-notes/actors.md §9): which
//! carried weapon an actor draws, which stance clips go with its animation type, where
//! the weapon hangs (the `prn:` text keys), and the holstered → equip → drawn → unequip
//! cycle. Headless; the engine samples the clips and moves the weapon model.

use crate::locomotion::{blend_time_of, wrap01, DEFAULT_BLEND};
use crate::records::Weapon;
use fnv_sim::FormId;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

/// WEAP DNAM "Animation Type" [documented: xEdit wbWeaponAnimTypeEnum, names as in
/// `schemas/fnv`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum AnimType {
    HandToHand,
    Melee1,
    Melee2,
    PistolBallistic,
    PistolEnergy,
    RifleBallistic,
    RifleAutomatic,
    RifleEnergy,
    Handle,
    Launcher,
    GrenadeThrow,
    LandMine,
    MineDrop,
    Thrown,
}

impl AnimType {
    pub const ALL: [AnimType; 14] = [
        AnimType::HandToHand,
        AnimType::Melee1,
        AnimType::Melee2,
        AnimType::PistolBallistic,
        AnimType::PistolEnergy,
        AnimType::RifleBallistic,
        AnimType::RifleAutomatic,
        AnimType::RifleEnergy,
        AnimType::Handle,
        AnimType::Launcher,
        AnimType::GrenadeThrow,
        AnimType::LandMine,
        AnimType::MineDrop,
        AnimType::Thrown,
    ];

    /// The schema's name for the value.
    pub fn name(self) -> &'static str {
        match self {
            AnimType::HandToHand => "Hand to Hand",
            AnimType::Melee1 => "Melee (1 Hand)",
            AnimType::Melee2 => "Melee (2 Hand)",
            AnimType::PistolBallistic => "Pistol - Ballistic (1 Hand)",
            AnimType::PistolEnergy => "Pistol - Energy (1 Hand)",
            AnimType::RifleBallistic => "Rifle - Ballistic (2 Hand)",
            AnimType::RifleAutomatic => "Rifle - Automatic (2 Hand)",
            AnimType::RifleEnergy => "Rifle - Energy (2 Hand)",
            AnimType::Handle => "Handle (2 Hand)",
            AnimType::Launcher => "Launcher (2 Hand)",
            AnimType::GrenadeThrow => "Grenade Throw (1 Hand)",
            AnimType::LandMine => "Land Mine (1 Hand)",
            AnimType::MineDrop => "Mine Drop (1 Hand)",
            AnimType::Thrown => "Thrown (1 Hand)",
        }
    }

    pub fn from_name(name: &str) -> Option<AnimType> {
        AnimType::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The stance prefix of the type's clips (`1hpequip.kf`, …) [observed, actors.md §9.3:
    /// each weapon's `##` parts are animated only by its prefix's clips (the laser pistol's
    /// by `1hp`, the laser rifle's `##LR_CellReleaseSpring` by `2hr`, `##MinigunBarrel` by
    /// `2hh`), and the thrown weapons' attack animations exist only as
    /// `1gtattackthrow6|7|8.kf`; `1lm` and `1md` by name only]. The energy types have no
    /// prefix of their own.
    pub fn prefix(self) -> &'static str {
        match self {
            AnimType::HandToHand => "h2h",
            AnimType::Melee1 => "1hm",
            AnimType::Melee2 => "2hm",
            AnimType::PistolBallistic | AnimType::PistolEnergy => "1hp",
            AnimType::RifleBallistic | AnimType::RifleEnergy => "2hr",
            AnimType::RifleAutomatic => "2ha",
            AnimType::Handle => "2hh",
            AnimType::Launcher => "2hl",
            AnimType::GrenadeThrow | AnimType::Thrown => "1gt",
            AnimType::LandMine => "1lm",
            AnimType::MineDrop => "1md",
        }
    }
}

/// Whether a weapon can be drawn: it has a model, is not part of a creature's body
/// ("Embedded Weapon") and has a known animation type.
pub fn drawable(w: &Weapon) -> bool {
    w.model.is_some() && !w.embedded && w.anim_type.as_deref().and_then(AnimType::from_name).is_some()
}

/// The damage a weapon is chosen by (actors.md §9.3): its base damage, plus the damage of
/// the explosion its projectile sets off for a weapon that is not thrown or laid nor an
/// Explosives weapon
/// (`fnv_sim::forms::weapon_choice_damage`, the rule the simulation equips NPCs by).
pub fn choice_damage(w: &Weapon) -> f32 {
    fnv_sim::forms::weapon_choice_damage(w.damage as f32, w.explosion_damage, w.anim_type.as_deref(), w.skill.as_deref())
}

/// The weapon an actor draws among its carried items: a weapon held in the hands (melee,
/// gun) before one thrown or laid ([`is_thrown`]), then the drawable weapon with the
/// highest [`choice_damage`], the first carried on ties [engine choice standing in for
/// Layer 6's equip logic, actors.md §9.3]; never one flagged Player Only or Not Used In
/// Normal Combat (docs/spec-notes/weapon-data-fields.md §1).
pub fn choose_weapon(carried: &[FormId], weapons: &HashMap<FormId, Weapon>) -> Option<FormId> {
    let mut best: Option<((bool, f32), FormId)> = None;
    for &id in carried {
        let Some(w) = weapons.get(&id).filter(|w| drawable(w) && !w.not_for_npcs) else { continue };
        let key = (!is_thrown(w), choice_damage(w));
        if best.is_none_or(|(k, _)| (key.0 && !k.0) || (key.0 == k.0 && key.1 > k.1)) {
            best = Some((key, id));
        }
    }
    best.map(|b| b.1)
}

/// Whether a weapon is thrown or laid rather than held (its animation type is Grenade
/// Throw, Thrown, Land Mine or Mine Drop, `fnv_sim::forms::THROWN_ANIM_TYPES`). An actor
/// holds one only when it carries no other weapon it can draw (actors.md §9.3).
pub fn is_thrown(w: &Weapon) -> bool {
    w.anim_type.as_deref().is_some_and(|a| fnv_sim::forms::THROWN_ANIM_TYPES.contains(&a))
}

/// The four clips of a weapon stance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum EquipClip {
    /// Weapon at rest, holstered (a short pose).
    Holster,
    /// Drawing it (plays once).
    Equip,
    /// Drawn, at the ready (loops).
    Aim,
    /// Putting it away (plays once).
    Unequip,
}

impl EquipClip {
    pub const ALL: [EquipClip; 4] = [EquipClip::Holster, EquipClip::Equip, EquipClip::Aim, EquipClip::Unequip];

    pub fn stem(self) -> &'static str {
        match self {
            EquipClip::Holster => "holster",
            EquipClip::Equip => "equip",
            EquipClip::Aim => "aim",
            EquipClip::Unequip => "unequip",
        }
    }

    /// Whether the clip loops while its stage lasts (the others play once and hold).
    pub fn loops(self) -> bool {
        self == EquipClip::Aim
    }
}

/// The clip of a stance in a skeleton folder: `<folder>\<prefix><stem>.kf` [observed:
/// e.g. `characters\_male\1hpequip.kf`; actors.md §9.1].
pub fn clip_path(folder: &str, prefix: &str, clip: EquipClip) -> String {
    format!("{folder}\\{prefix}{}.kf", clip.stem())
}

/// The crouched form of a stance clip, played while the actor sneaks:
/// `<folder>\sneak<prefix><stem>.kf` for the equip, aim and unequip clips, which every
/// stance has (`sneak1hpaim.kf`, `sneak2hrequip.kf`, `sneakh2hunequip.kf` …), and none
/// for the holster clip: there is no `sneak<prefix>holster.kf`, only the power-armour
/// `pasneak2haholster.kf` and `pasneak2hrholster.kf` [observed: `Fallout - Meshes.bsa`
/// and `Update.bsa` file lists, 2026-10-20; docs/spec-notes/actors.md §9.4]. The holster
/// clips drive no legs (priority 5 on the holster bone), so the standing one serves
/// crouched too [observed: kf_layer_survey].
pub fn sneak_clip_path(folder: &str, prefix: &str, clip: EquipClip) -> Option<String> {
    (clip != EquipClip::Holster).then(|| format!("{folder}\\sneak{prefix}{}.kf", clip.stem()))
}

/// The bone a `prn:` text key names (`prn: Bip01 R Hand`, `Prn: …`, `prn: Bip01 R Hand
/// -at none`): the weapon's parent from that time on [hypothesis, actors.md §9.2].
pub fn prn_bone(key: &str) -> Option<String> {
    let k = key.trim();
    let rest = k.get(..4).filter(|h| h.eq_ignore_ascii_case("prn:")).map(|_| k[4..].trim())?;
    let bone = rest.split(" -").next().unwrap_or(rest).trim();
    (!bone.is_empty()).then(|| bone.to_string())
}

/// What a stance clip says about the weapon: its `prn:` keys as (phase 0–1, bone) in
/// time order, the phase of its first `Attach` or `Detach` key, and its length and
/// blend-in time.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ClipFacts {
    pub duration: f32,
    pub blend_time: f32,
    pub prn: Vec<(f32, String)>,
    /// `Attach` (equip) or `Detach` (unequip): when the weapon changes bones. The clips'
    /// `Weapon` track jumps from its holster pose to its hand pose there and nowhere else,
    /// while their `prn:` keys sit elsewhere (`2hmequip.kf`: `prn:` 0.03 s, `Attach`
    /// 0.30 s; `2hrequip.kf`: `Attach` 0.30 s, `prn:` 0.63 s) [observed: animation.md
    /// §9.10].
    pub switch: Option<f32>,
}

impl ClipFacts {
    pub fn from_parts(start: f32, stop: f32, text_keys: &[(f32, String)]) -> ClipFacts {
        let len = (stop - start).max(0.0);
        let mut prn: Vec<(f32, String)> = text_keys
            .iter()
            .filter_map(|(t, k)| Some((if len > 0.0 { ((t - start) / len).clamp(0.0, 1.0) } else { 0.0 }, prn_bone(k)?)))
            .collect();
        prn.sort_by(|a, b| a.0.total_cmp(&b.0));
        let switch = text_keys
            .iter()
            .filter(|(_, k)| k.split(['\r', '\n']).map(str::trim).any(|l| l.eq_ignore_ascii_case("attach") || l.eq_ignore_ascii_case("detach")))
            .map(|(t, _)| if len > 0.0 { ((t - start) / len).clamp(0.0, 1.0) } else { 0.0 })
            .min_by(f32::total_cmp);
        ClipFacts { duration: len, blend_time: blend_time_of(text_keys).unwrap_or(DEFAULT_BLEND), prn, switch }
    }

    /// The bone of the last `prn:` key at or before `phase`.
    pub fn parent_at(&self, phase: f32) -> Option<&str> {
        self.prn.iter().rev().find(|(p, _)| *p <= phase).map(|(_, b)| b.as_str())
    }

    pub fn last_parent(&self) -> Option<&str> {
        self.prn.last().map(|(_, b)| b.as_str())
    }
}

/// What the clips of one stance say, by clip (a clip the stance lacks is absent).
pub type StanceFacts = BTreeMap<EquipClip, ClipFacts>;

/// Where a stance's weapon hangs when holstered and when drawn: the holster clip's last
/// `prn:` (else the unequip clip's), and the equip clip's (else the aim clip's, else
/// `drawn_default`) [observed: holster clips name `Bip01 Pelvis` (pistols, one-handed
/// melee, grenades, mines), `Bip01 Spine2` (rifles, two-handed melee, launchers) or the
/// right arm (`h2h`, `2hh`); actors.md §9.2]. The equip clip's, as the player body
/// takes it: `2hmaim.kf` alone names another bone (`Bip01 L Hand`) than its equip clip,
/// yet its `Weapon` track is the pose the equip clip ends in under the right hand, and
/// hung from the left hand the bat turned end over end in the hands [observed:
/// animation.md §9.10, owner's playtest 2026-10-16].
pub fn mount_bones(facts: &StanceFacts, drawn_default: &str) -> (String, String) {
    let last = |c: EquipClip| facts.get(&c).and_then(|f| f.last_parent().map(str::to_string));
    let drawn = last(EquipClip::Equip).or_else(|| last(EquipClip::Aim)).unwrap_or_else(|| drawn_default.to_string());
    let holster = last(EquipClip::Holster).or_else(|| last(EquipClip::Unequip)).unwrap_or_else(|| drawn.clone());
    (holster, drawn)
}

/// Stage of the draw cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Stage {
    Holstered,
    Equipping,
    Drawn,
    Unequipping,
}

impl Stage {
    pub fn clip(self) -> EquipClip {
        match self {
            Stage::Holstered => EquipClip::Holster,
            Stage::Equipping => EquipClip::Equip,
            Stage::Drawn => EquipClip::Aim,
            Stage::Unequipping => EquipClip::Unequip,
        }
    }
}

/// The draw cycle of one actor's weapon: the stage, the time into its clip and the clip
/// fading out (actors.md §9.4).
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponAnim {
    pub stage: Stage,
    pub time: f32,
    /// The clip fading out and its time, the fade's progress and length (seconds).
    pub prev: Option<(EquipClip, f32)>,
    pub fade: f32,
    pub fade_time: f32,
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

impl WeaponAnim {
    pub fn new(drawn: bool) -> WeaponAnim {
        WeaponAnim { stage: if drawn { Stage::Drawn } else { Stage::Holstered }, time: 0.0, prev: None, fade: 0.0, fade_time: 0.0 }
    }

    /// Weight of the current stage's clip; the fading one has the rest.
    pub fn weight(&self) -> f32 {
        match self.prev {
            Some(_) if self.fade_time > 0.0 => smoothstep(self.fade / self.fade_time),
            _ => 1.0,
        }
    }

    fn enter(&mut self, stage: Stage, facts: &StanceFacts) {
        self.prev = Some((self.stage.clip(), self.time));
        self.stage = stage;
        self.time = 0.0;
        self.fade = 0.0;
        self.fade_time = facts.get(&stage.clip()).map_or(DEFAULT_BLEND, |f| f.blend_time);
        if self.fade_time <= 0.0 {
            self.prev = None;
        }
    }

    /// Fade from the current clip, at its time, to another form of it (crouched or
    /// standing, [`sneak_clip_path`]): the current clip becomes the one fading out over
    /// `fade_time`, an older fade still running is dropped (as a change of locomotion set
    /// cuts one short), and the stage and its time go on. `fade_time` 0 swaps at once.
    pub fn fade_from_current(&mut self, fade_time: f32) {
        self.prev = (fade_time > 0.0).then_some((self.stage.clip(), self.time));
        self.fade = 0.0;
        self.fade_time = fade_time;
    }

    /// Advance by `dt` towards `drawn`. Equip and unequip play to their end before the
    /// next stage starts, so a change of mind waits for them.
    pub fn step(&mut self, drawn: bool, dt: f32, facts: &StanceFacts) {
        self.time += dt;
        if let Some((_, t)) = self.prev.as_mut() {
            *t += dt;
            self.fade += dt;
            if self.fade >= self.fade_time {
                self.prev = None;
            }
        }
        let len = |c: EquipClip| facts.get(&c).map_or(0.0, |f| f.duration);
        match self.stage {
            Stage::Holstered if drawn => self.enter(Stage::Equipping, facts),
            Stage::Equipping if self.time >= len(EquipClip::Equip) => self.enter(Stage::Drawn, facts),
            Stage::Drawn if !drawn => self.enter(Stage::Unequipping, facts),
            Stage::Unequipping if self.time >= len(EquipClip::Unequip) => self.enter(Stage::Holstered, facts),
            _ => {}
        }
    }

    /// Phase (0–1) of `clip` at `time`: wrapped for the looping aim, held at the end for
    /// the others.
    pub fn phase(clip: EquipClip, time: f32, facts: &StanceFacts) -> f32 {
        let len = facts.get(&clip).map_or(0.0, |f| f.duration);
        if len <= 0.0 {
            return 1.0;
        }
        if clip.loops() {
            wrap01(time / len)
        } else {
            (time / len).clamp(0.0, 1.0)
        }
    }

    /// The clips to sample: (clip, phase, weight), the current one first.
    pub fn layers(&self, facts: &StanceFacts) -> [Option<(EquipClip, f32, f32)>; 2] {
        let w = self.weight();
        let c = self.stage.clip();
        [Some((c, Self::phase(c, self.time, facts), w)), self.prev.map(|(p, t)| (p, Self::phase(p, t, facts), 1.0 - w))]
    }

    /// The bone the weapon hangs from now: the holster or hand bone at rest; while drawing
    /// or putting away, the other one from the clip's `Attach` / `Detach` key on, as the
    /// player body does ([`ClipFacts::switch`]), else (a clip without one) the last
    /// `prn:` key the clip has passed, starting from where the weapon was.
    pub fn parent<'a>(&self, holster: &'a str, drawn: &'a str, facts: &'a StanceFacts) -> &'a str {
        let passed = |c: EquipClip, from: &'a str, to: &'a str| {
            let f = facts.get(&c)?;
            let phase = Self::phase(c, self.time, facts);
            match f.switch {
                Some(at) => Some(if phase >= at { to } else { from }),
                None => f.parent_at(phase),
            }
        };
        match self.stage {
            Stage::Holstered => holster,
            Stage::Drawn => drawn,
            Stage::Equipping => passed(EquipClip::Equip, holster, drawn).unwrap_or(holster),
            Stage::Unequipping => passed(EquipClip::Unequip, drawn, holster).unwrap_or(drawn),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weapon(model: Option<&str>, anim: &str, damage: i32, embedded: bool) -> Weapon {
        Weapon { edid: None, model: model.map(str::to_string), anim_type: Some(anim.to_string()), damage, projectile: None, explosion_damage: 0.0, skill: None, embedded, non_playable: false, not_for_npcs: false, first_person: None, first_person_model: None, mod_models: Default::default(), mod_first_person: [None; 7], mod_first_person_models: Default::default(), held_world_model: None, grip: None, hide_backpack: false, addons: Vec::new() }
    }

    #[test]
    fn anim_types_and_prefixes() {
        assert_eq!(AnimType::from_name("Pistol - Energy (1 Hand)"), Some(AnimType::PistolEnergy));
        assert_eq!(AnimType::from_name("Laser"), None);
        assert_eq!(AnimType::PistolEnergy.prefix(), "1hp");
        assert_eq!(AnimType::Handle.prefix(), "2hh");
        assert_eq!(AnimType::Thrown.prefix(), "1gt");
        for t in AnimType::ALL {
            assert_eq!(AnimType::from_name(t.name()), Some(t));
        }
        assert_eq!(clip_path(r"meshes\characters\_male", "1hp", EquipClip::Equip), r"meshes\characters\_male\1hpequip.kf");
    }

    #[test]
    fn weapon_choice() {
        let (a, b, c, d) = (FormId(1), FormId(2), FormId(3), FormId(4));
        let ws = HashMap::from([
            (a, weapon(Some("w\\a.nif"), "Melee (1 Hand)", 10, false)),
            (b, weapon(Some("w\\b.nif"), "Pistol - Ballistic (1 Hand)", 20, false)),
            (c, weapon(Some("w\\c.nif"), "Rifle - Ballistic (2 Hand)", 20, false)),
            (d, weapon(Some("w\\d.nif"), "Hand to Hand", 99, true)),
        ]);
        assert_eq!(choose_weapon(&[a, b, c, d], &ws), Some(b));
        assert_eq!(choose_weapon(&[c, b], &ws), Some(c));
        assert_eq!(choose_weapon(&[d, FormId(9)], &ws), None);
        assert!(!drawable(&weapon(None, "Melee (1 Hand)", 1, false)));
        // A Player Only weapon (the Van Graff laser rifle) is passed over for a weaker one.
        let mut van_graff = weapon(Some("w\\v.nif"), "Rifle - Energy (2 Hand)", 50, false);
        van_graff.not_for_npcs = true;
        let ws = HashMap::from([(a, van_graff), (b, weapon(Some("w\\b.nif"), "Pistol - Ballistic (1 Hand)", 20, false))]);
        assert_eq!(choose_weapon(&[a, b], &ws), Some(b));
        assert_eq!(choose_weapon(&[a], &ws), None);
    }

    #[test]
    fn weapon_choice_counts_a_guns_explosion() {
        // Cook-Cook's pair: Incinerator (1, its glob's explosion 20) over Flamer (16).
        let (inc, flamer, grenade, bat) = (FormId(1), FormId(2), FormId(3), FormId(4));
        let mut incinerator = weapon(Some("w\\i.nif"), "Handle (2 Hand)", 1, false);
        incinerator.explosion_damage = 20.0;
        let mut frag = weapon(Some("w\\g.nif"), "Grenade Throw (1 Hand)", 1, false);
        frag.explosion_damage = 100.0;
        let ws = HashMap::from([
            (inc, incinerator),
            (flamer, weapon(Some("w\\f.nif"), "Handle (2 Hand)", 16, false)),
            (grenade, frag),
            (bat, weapon(Some("w\\b.nif"), "Melee (2 Hand)", 22, false)),
        ]);
        assert_eq!(choose_weapon(&[flamer, inc], &ws), Some(inc));
        // A thrown weapon's explosion does not count: it is not held as the weapon.
        assert_eq!(choose_weapon(&[grenade, bat], &ws), Some(bat));
    }

    #[test]
    fn an_explosives_weapon_ranks_by_its_base_damage() {
        // Observer check 1 (o13_reequip): nobody took a Grenade Rifle (2, its 40mm
        // grenade's explosion) over a Varmint Rifle (18); a Hunting Rifle (52) was taken.
        let (gl, varmint, hunting) = (FormId(1), FormId(2), FormId(3));
        let mut grenade_rifle = weapon(Some("w\\gl.nif"), "Rifle - Ballistic (2 Hand)", 2, false);
        grenade_rifle.explosion_damage = 75.0;
        grenade_rifle.skill = Some("Explosives".into());
        let gun = |d| {
            let mut w = weapon(Some("w\\r.nif"), "Rifle - Ballistic (2 Hand)", d, false);
            w.skill = Some("Guns".into());
            w
        };
        let ws = HashMap::from([(gl, grenade_rifle), (varmint, gun(18)), (hunting, gun(52))]);
        assert_eq!(choose_weapon(&[gl, varmint], &ws), Some(varmint));
        assert_eq!(choose_weapon(&[varmint, gl, hunting], &ws), Some(hunting));
        // Alone, it is still drawn.
        assert_eq!(choose_weapon(&[gl], &ws), Some(gl));
    }

    #[test]
    fn weapon_choice_holds_a_hand_weapon_before_a_thrown_one() {
        // A ghost harvester's pair: throwing spear (145) and spear (81): the spear is held.
        let (thrown, spear) = (FormId(1), FormId(2));
        let ws = HashMap::from([
            (thrown, weapon(Some("w\\t.nif"), "Thrown (1 Hand)", 145, false)),
            (spear, weapon(Some("w\\s.nif"), "Melee (2 Hand)", 81, false)),
        ]);
        assert_eq!(choose_weapon(&[thrown, spear], &ws), Some(spear));
        // Carrying only thrown weapons, the actor holds one.
        assert_eq!(choose_weapon(&[thrown], &ws), Some(thrown));
    }

    #[test]
    fn prn_keys() {
        assert_eq!(prn_bone("prn: Bip01 R Hand").as_deref(), Some("Bip01 R Hand"));
        assert_eq!(prn_bone("Prn: Bip01 Pelvis\r\n").as_deref(), Some("Bip01 Pelvis"));
        assert_eq!(prn_bone("prn: Bip01 R Hand  -at none").as_deref(), Some("Bip01 R Hand"));
        assert_eq!(prn_bone("Attach"), None);
        assert_eq!(prn_bone("prn:"), None);
        let keys: Vec<(f32, String)> = [(0.0, "start"), (0.0333, "Enum: Equip"), (0.1333, "prn: Bip01 R Hand"), (0.1667, "Attach"), (0.3667, "end")]
            .iter()
            .map(|&(t, k)| (t, k.to_string()))
            .collect();
        let f = ClipFacts::from_parts(0.0, 0.3667, &keys);
        assert_eq!(f.prn.len(), 1);
        assert!((f.prn[0].0 - 0.3635).abs() < 1e-3);
        assert_eq!(f.parent_at(0.2), None);
        assert_eq!(f.parent_at(0.5), Some("Bip01 R Hand"));
        assert!((f.switch.unwrap() - 0.4546).abs() < 1e-3);
        assert_eq!(f.blend_time, DEFAULT_BLEND);
    }

    /// A pistol stance shaped like `1hp*.kf` (actors.md §9.1).
    fn pistol() -> StanceFacts {
        let f = |duration: f32, blend_time: f32, prn: &[(f32, &str)]| ClipFacts { duration, blend_time, prn: prn.iter().map(|&(p, b)| (p, b.to_string())).collect(), switch: None };
        BTreeMap::from([
            (EquipClip::Holster, f(0.1, 1.0 / 30.0, &[(0.33, "Bip01 Pelvis")])),
            (EquipClip::Equip, f(0.3667, 0.2, &[(0.36, "Bip01 R Hand")])),
            (EquipClip::Aim, f(2.4667, 0.2, &[(0.01, "Bip01 R Hand")])),
            (EquipClip::Unequip, f(0.4667, 0.2, &[(0.43, "Bip01 Pelvis")])),
        ])
    }

    #[test]
    fn mounts_from_prn_keys() {
        assert_eq!(mount_bones(&pistol(), "Bip01 R Hand"), ("Bip01 Pelvis".to_string(), "Bip01 R Hand".to_string()));
        assert_eq!(mount_bones(&StanceFacts::new(), "Bip01 R Hand"), ("Bip01 R Hand".to_string(), "Bip01 R Hand".to_string()));
        // `2hm*.kf`: the aim clip names the left hand, the equip clip the right; the
        // equip clip's wins.
        let mut bat = two_hand_melee();
        assert_eq!(mount_bones(&bat, "Bip01 R Hand"), ("Bip01 Spine2".to_string(), "Bip01 R Hand".to_string()));
        bat.remove(&EquipClip::Equip);
        assert_eq!(mount_bones(&bat, "Bip01 R Hand").1, "Bip01 L Hand");
    }

    /// A two-handed melee stance shaped like `2hm*.kf` (animation.md §9.10): `prn:` keys
    /// near the start of the equip and unequip clips, `Attach` / `Detach` later.
    fn two_hand_melee() -> StanceFacts {
        let f = |duration: f32, prn: (f32, &str), switch: Option<f32>| ClipFacts { duration, blend_time: 0.2, prn: vec![(prn.0, prn.1.to_string())], switch };
        BTreeMap::from([
            (EquipClip::Holster, f(0.1, (0.67, "Bip01 Spine2"), None)),
            (EquipClip::Equip, f(1.2, (0.028, "Bip01 R Hand"), Some(0.25))),
            (EquipClip::Aim, f(3.5, (0.038, "Bip01 L Hand"), None)),
            (EquipClip::Unequip, f(1.2, (0.028, "Bip01 Spine2"), Some(0.72))),
        ])
    }

    #[test]
    fn bones_change_at_attach_and_detach() {
        let facts = two_hand_melee();
        let (h, d) = mount_bones(&facts, "Bip01 R Hand");
        let mut w = WeaponAnim::new(false);
        w.step(true, 0.01, &facts);
        assert_eq!(w.stage, Stage::Equipping);
        // Past the equip clip's `prn:` key (0.03 s) but before `Attach` (0.30 s): still
        // on the back.
        w.step(true, 0.1, &facts);
        assert_eq!(w.parent(&h, &d, &facts), "Bip01 Spine2");
        w.step(true, 0.25, &facts);
        assert_eq!(w.parent(&h, &d, &facts), "Bip01 R Hand");
        while w.stage != Stage::Drawn {
            w.step(true, 0.05, &facts);
        }
        assert_eq!(w.parent(&h, &d, &facts), "Bip01 R Hand");
        // Putting it away: in the hand until `Detach` (0.87 s), on the back after.
        w.step(false, 0.01, &facts);
        assert_eq!(w.stage, Stage::Unequipping);
        w.step(false, 0.5, &facts);
        assert_eq!(w.parent(&h, &d, &facts), "Bip01 R Hand");
        w.step(false, 0.4, &facts);
        assert_eq!(w.parent(&h, &d, &facts), "Bip01 Spine2");
    }

    #[test]
    fn draw_cycle() {
        let facts = pistol();
        let mut w = WeaponAnim::new(false);
        let dt = 0.01;
        let (h, d) = ("Bip01 Pelvis", "Bip01 R Hand");
        assert_eq!(w.layers(&facts)[0].map(|l| l.0), Some(EquipClip::Holster));
        // Draw: the equip clip fades in over the holster pose.
        w.step(true, dt, &facts);
        assert_eq!(w.stage, Stage::Equipping);
        assert_eq!(w.prev.map(|p| p.0), Some(EquipClip::Holster));
        assert_eq!(w.parent(h, d, &facts), h);
        let mut parents = vec![w.parent(h, d, &facts).to_string()];
        let mut stages = vec![w.stage];
        // Holster again half-way: the equip clip still plays to its end, then the aim, then
        // the unequip, back to holstered.
        for i in 0..200 {
            w.step(i < 10, dt, &facts);
            let [cur, prev] = w.layers(&facts);
            let total = cur.map_or(0.0, |l| l.2) + prev.map_or(0.0, |l| l.2);
            assert!((total - 1.0).abs() < 1e-5);
            if stages.last() != Some(&w.stage) {
                stages.push(w.stage);
            }
            let p = w.parent(h, d, &facts).to_string();
            if parents.last() != Some(&p) {
                parents.push(p);
            }
        }
        assert_eq!(stages, [Stage::Equipping, Stage::Drawn, Stage::Unequipping, Stage::Holstered]);
        assert_eq!(parents, [h, d, h]);
        // The aim loops; the others hold at their end.
        assert!((WeaponAnim::phase(EquipClip::Aim, 3.0, &facts) - (3.0 / 2.4667 - 1.0)).abs() < 1e-4);
        assert_eq!(WeaponAnim::phase(EquipClip::Equip, 3.0, &facts), 1.0);
    }

    #[test]
    fn crouched_forms_of_the_stance_clips() {
        let f = r"meshes\characters\_male";
        assert_eq!(sneak_clip_path(f, "1hp", EquipClip::Aim).as_deref(), Some(r"meshes\characters\_male\sneak1hpaim.kf"));
        assert_eq!(sneak_clip_path(f, "h2h", EquipClip::Unequip).as_deref(), Some(r"meshes\characters\_male\sneakh2hunequip.kf"));
        // No crouched holster clip: the standing one serves.
        assert_eq!(sneak_clip_path(f, "2hr", EquipClip::Holster), None);
        // Crouching while drawn: the aim fades over from its standing form at the same
        // time, the stage unchanged; a second change while it runs drops the older fade.
        let facts = pistol();
        let mut w = WeaponAnim::new(true);
        w.step(true, 1.0, &facts);
        w.fade_from_current(0.2);
        assert_eq!((w.stage, w.prev, w.weight()), (Stage::Drawn, Some((EquipClip::Aim, 1.0)), 0.0));
        w.step(true, 0.1, &facts);
        assert!(w.weight() > 0.0 && w.weight() < 1.0);
        w.fade_from_current(0.2);
        assert_eq!(w.prev.map(|p| p.1), Some(w.time));
        w.step(true, 0.25, &facts);
        assert_eq!((w.prev, w.weight()), (None, 1.0));
        // Without a fade time the form swaps at once.
        w.fade_from_current(0.0);
        assert_eq!(w.prev, None);
    }
}
