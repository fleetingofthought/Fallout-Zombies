//! Plays KF controller sequences by sampling `fnv_nif::anim` tracks every frame and
//! writing the bones' local transforms. Done here rather than with Bevy animation clips
//! so that the original interpolation rules (B-splines, TBC keys, cycle types) stay in
//! the engine-independent format crate (docs/design/layer2.md §2.7).
//!
//! The viewer plays one sequence at a time ([`AnimState`]); world objects carry their
//! own [`Playing`] component, advanced by [`advance_instances`].

use bevy::prelude::*;
use fnv_nif::anim::{BoolTrack, Sequence, TrackData, TransformTrack, ValueTrack};
use std::collections::HashMap;
use std::sync::Arc;

/// A sequence's transform tracks by target node name, shareable between instances.
#[derive(Debug, Clone)]
pub struct Clip {
    pub name: String,
    pub start: f32,
    pub stop: f32,
    pub frequency: f32,
    pub cycle_type: i64,
    pub tracks: Vec<(String, Arc<TransformTrack>)>,
    /// Each track's priority (nif.xml ControlledBlock "Priority"; 0 when absent), in
    /// `tracks` order.
    pub priorities: Vec<i32>,
    /// Tracks without a target name or with an interpolator that is not a transform.
    pub skipped: usize,
    /// The sequence's accumulation root (nif.xml NiControllerSequence "Accum Root Name").
    pub accum_root: Option<String>,
    /// Text keys (time, text).
    pub text_keys: Vec<(f32, String)>,
    /// Visibility tracks (`NiVisController`): node name, priority, shown over time
    /// (docs/spec-notes/animation.md §9.7).
    pub visibility: Vec<(String, i32, BoolTrack)>,
    /// Particle emitter tracks (`NiPSysEmitterCtlr`) on systems of a model the clip
    /// moves the parts of: the dynamite's fuse sparks in its throw clip ([`EmitterTrack`]).
    pub emitters: Vec<EmitterTrack>,
    /// Tracks on a shape's material: its glow and texture offset (`crate::material_anim`,
    /// docs/spec-notes/activators.md §2).
    pub materials: Vec<MaterialTrack>,
    /// Transform tracks of controllers that run by themselves rather than under a
    /// sequence (circling vultures, turning light-ray fans; `model::free_transform_tracks`,
    /// docs/spec-notes/effects-animation-audit.md §3): node name, track, own timing.
    pub free: Vec<(String, Arc<TransformTrack>, OwnCycle)>,
    /// Visibility tracks of `NiVisController`s that run by themselves (the dust tube near
    /// Goodsprings, `model::free_visibility_tracks`, docs/spec-notes/nif.md §6): node
    /// name, track, own timing.
    pub free_visibility: Vec<(String, BoolTrack, OwnCycle)>,
    /// Tracks on a light of the model: its colour and dimmer (`NiLightColorController`,
    /// `NiLightDimmerController`; docs/spec-notes/model-lights.md).
    pub lights: Vec<LightTrack>,
}

/// A clip's tracks on one particle system's emitter controller (`NiPSysEmitterCtlr`),
/// by the system's name: interpolator ID `BirthRate` (particles a second) and
/// `EmitterActive` (on/off keys) [observed: `_1stpersongtattackthrow2.kf` drives the
/// dynamite's `##PCloud01` with both, on from 1.0 s to its `Release` key at 1.83 s, at
/// 21.6 a second; the model's own controller keeps it on for ever, docs/spec-notes/particles.md §7.6].
#[derive(Debug, Clone)]
pub struct EmitterTrack {
    pub system: String,
    pub priority: i32,
    pub rate: Option<Arc<ValueTrack>>,
    pub active: Option<BoolTrack>,
}

impl EmitterTrack {
    /// Whether the emitter is on at `t`, and its birth rate there where the clip keys one.
    /// Without an on/off track the emitter is on while the clip speaks for it.
    pub fn at(&self, t: f32) -> (bool, Option<f32>) {
        let on = self.active.as_ref().and_then(|a| a.at(t)).unwrap_or(true);
        let rate = self.rate.as_ref().and_then(|r| r.at(t)).map(|[v, ..]| v.max(0.0));
        (on, rate)
    }
}

/// What a light track drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightTarget {
    /// `NiLightColorController` with controller ID `Diffuse` (target colour 0,
    /// LC_DIFFUSE): the light's diffuse colour [documented: nif.xml LightColor; observed
    /// controller ID, docs/spec-notes/model-lights.md §1].
    Diffuse,
    /// `NiLightDimmerController`: the light's dimmer [documented: nif.xml].
    Dimmer,
}

/// One track of a sequence on a light (`NiPointLight`, `NiSpotLight`), by light name.
#[derive(Debug, Clone)]
pub struct LightTrack {
    pub light: String,
    pub target: LightTarget,
    pub values: Arc<ValueTrack>,
}

impl LightTrack {
    /// The light track of a sequence's track, if it drives a light's diffuse colour or
    /// dimmer. Ambient colour tracks (controller ID `Ambient`) are not taken: the
    /// engine draws no light's ambient colour.
    pub fn of(t: &fnv_nif::anim::Track) -> Option<LightTrack> {
        let light = t.node_name.clone()?;
        let values = t.values.clone()?;
        let target = match (t.controller_type.as_deref()?, &values) {
            ("NiLightColorController", ValueTrack::Point3(_)) if t.controller_id.as_deref() == Some("Diffuse") => LightTarget::Diffuse,
            ("NiLightDimmerController", ValueTrack::Float(_)) => LightTarget::Dimmer,
            _ => return None,
        };
        Some(LightTrack { light, target, values: Arc::new(values) })
    }
}

/// What a material track drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialTarget {
    /// `NiMaterialColorController` with controller ID `SELF_ILLUM`: the emissive colour
    /// [documented: nif.xml MaterialColor TC_SELF_ILLUM; observed controller ID].
    SelfIllum,
    /// `BSMaterialEmittanceMultController`: the emissive multiplier [hypothesis from the
    /// name; observed on blinking lights and slot machines].
    EmissiveMult,
    /// `NiTextureTransformController` `0-0-TT_TRANSLATE_U` (0) or `_V` (1): the base
    /// texture's offset [documented: nif.xml TransformMember; observed controller IDs].
    UvOffset(usize),
    /// `NiTextureTransformController` `0-0-TT_SCALE_U` (0) or `_V` (1): the base
    /// texture's scale [documented: nif.xml TransformMember; observed: the Securitron
    /// screens' `mtidle.kf`, docs/spec-notes/effects-animation-audit.md §2].
    UvScale(usize),
    /// `NiTextureTransformController` `0-0-TT_ROTATE`: the base texture's rotation,
    /// radians [documented: nif.xml TransformMember; hypothesis: the unit].
    UvRotate,
    /// `NiAlphaController` on the shape's `NiMaterialProperty`: the material's alpha
    /// [observed: the controlled blocks name property type NiMaterialProperty and the
    /// controller targets that property (`windowem01.nif`); hypothesis: the value
    /// replaces the stored alpha]. The neon signs and lit windows fade with it
    /// (activators.md §6).
    Alpha,
    /// `BSRefractionStrengthController` on the shape's BSShaderPPLightingProperty: the
    /// refraction strength of a refracting shape (`nif_material`, "Refraction")
    /// [hypothesis from the name; observed: the explosions' heat-haze shells,
    /// `dlc03flamerexplosion.nif` 0.056 → 0.156 → 0.056 over 0.23 s].
    Refraction,
}

/// One track of a sequence on a shape's material, by shape name.
#[derive(Debug, Clone)]
pub struct MaterialTrack {
    pub shape: String,
    pub target: MaterialTarget,
    pub values: Arc<ValueTrack>,
    /// The track's own timing, for a controller that runs by itself rather than under a
    /// sequence (`None`: it follows the sequence's time). See [`OwnCycle`].
    pub cycle: Option<OwnCycle>,
    /// The controlled block's priority (nif.xml ControlledBlock "Priority"; 0 for a
    /// controller of its own): of two layers as strong, the higher priority's value shows
    /// (`material_anim::actor_material_values`).
    pub priority: i32,
}

/// The timing of a controller that is not driven by a sequence: an effect mesh whose
/// texture scrolls by a `NiTextureTransformController` on its `NiTexturingProperty`
/// (smoke wisps, ground smoke; docs/spec-notes/nif.md §6 "Controllers outside
/// sequences"). Fields of NiTimeController [documented: nif.xml]: start and stop time,
/// frequency, phase, and the cycle type in flag bits 1-2 (0 loop, 1 reverse, 2 clamp).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OwnCycle {
    pub start: f32,
    pub stop: f32,
    pub frequency: f32,
    pub phase: f32,
    pub cycle_type: i64,
}

impl OwnCycle {
    /// The controller's time after `clock` seconds of play: clock × frequency + phase,
    /// then brought into start..stop by the cycle type (clamped, the first key held until
    /// the start time) [hypothesis: the order of frequency and phase, every looping
    /// controller surveyed has phase 0; that the clock is not offset by the start time:
    /// an impact model's second splash has its keys and its controller's start at
    /// 0.133 s, after the first's, docs/spec-notes/explosion-effects.md §2].
    pub fn time(&self, clock: f32) -> f32 {
        cycle_time(clock * self.frequency + self.phase, self.start, self.stop, self.cycle_type)
    }

    /// The length of one cycle in seconds of play.
    pub fn period(&self) -> f32 {
        (self.stop - self.start).max(0.0) / self.frequency.max(1e-6)
    }
}

impl MaterialTrack {
    /// The material track of a sequence's track, if it drives a material member this
    /// engine draws: emissive colour, emissive multiplier, base texture offset, scale and
    /// rotation, alpha.
    pub fn of(t: &fnv_nif::anim::Track) -> Option<MaterialTrack> {
        let shape = t.node_name.clone()?;
        let values = t.values.clone()?;
        let id = t.controller_id.as_deref().unwrap_or("");
        let target = match (t.controller_type.as_deref()?, &values) {
            ("NiMaterialColorController", ValueTrack::Point3(_)) if id == "SELF_ILLUM" => MaterialTarget::SelfIllum,
            ("BSMaterialEmittanceMultController", ValueTrack::Float(_)) => MaterialTarget::EmissiveMult,
            ("NiAlphaController", ValueTrack::Float(_)) => MaterialTarget::Alpha,
            ("BSRefractionStrengthController", ValueTrack::Float(_)) => MaterialTarget::Refraction,
            ("NiTextureTransformController", ValueTrack::Float(_)) => match id {
                "0-0-TT_TRANSLATE_U" => MaterialTarget::UvOffset(0),
                "0-0-TT_TRANSLATE_V" => MaterialTarget::UvOffset(1),
                "0-0-TT_SCALE_U" => MaterialTarget::UvScale(0),
                "0-0-TT_SCALE_V" => MaterialTarget::UvScale(1),
                "0-0-TT_ROTATE" => MaterialTarget::UvRotate,
                _ => return None,
            },
            _ => return None,
        };
        Some(MaterialTrack { shape, target, values: Arc::new(values), cycle: None, priority: t.priority.unwrap_or(0) as i32 })
    }
}

/// Whether every key of `k` holds finite numbers only.
fn keys_finite<const N: usize>(k: &fnv_nif::anim::Keys<N>) -> bool {
    k.keys.iter().all(|k| k.time.is_finite() && k.value.iter().chain(&k.forward).chain(&k.backward).chain(&k.tbc).all(|v| v.is_finite()))
}

fn channel_finite<const N: usize>(c: &fnv_nif::anim::Channel<N>) -> bool {
    use fnv_nif::anim::Channel;
    match c {
        Channel::Static(v) => v.is_none_or(|v| v.iter().all(|x| x.is_finite())),
        Channel::Keys(k) => keys_finite(k),
        Channel::BSpline { start, stop, spline } => start.is_finite() && stop.is_finite() && spline.points.iter().all(|v| v.is_finite()),
        Channel::Path(p) => keys_finite(&p.path) && keys_finite(&p.percent) && p.by_length.as_ref().is_none_or(|t| t.iter().all(|v| v.is_finite())),
    }
}

/// Whether a transform track holds finite numbers only ([`Clip::check`]).
fn track_finite(t: &TransformTrack) -> bool {
    channel_finite(&t.translation) && channel_finite(&t.rotation) && channel_finite(&t.scale) && t.euler.as_ref().is_none_or(|e| e.iter().all(keys_finite))
}

/// The largest frequency a playable clip may have ([`Clip::check`]), far past what the
/// game's files hold [observed: in `Fallout - Meshes.bsa` (6229 sequences) every
/// sequence and controller frequency is 1]: past it the clip's time runs out of the float
/// range. Values are only required to be finite: the game's own clips hold very large
/// ones [observed: `xanderrootplant.nif`'s Backward has a scale past 1e3, translations
/// reach 2.9e6], so a bound on them refuses good files.
const MAX_FREQUENCY: f32 = 1e3;

/// Why a transform track cannot be played, if it cannot: a number that is not finite.
fn track_fault(t: &TransformTrack) -> Option<&'static str> {
    (!track_finite(t)).then_some("a key is not a finite number")
}

/// Whether a frequency is one a clip can play at: above 0, at most [`MAX_FREQUENCY`].
fn frequency_ok(f: f32) -> bool {
    f > 0.0 && f <= MAX_FREQUENCY
}

/// Whether a controller's own timing is finite and its frequency playable.
fn cycle_ok(c: &OwnCycle) -> bool {
    [c.start, c.stop, c.phase].iter().all(|v| v.is_finite()) && frequency_ok(c.frequency)
}

impl Clip {
    pub fn from_sequence(seq: &Sequence) -> Clip {
        let mut tracks = Vec::new();
        let mut priorities = Vec::new();
        let mut skipped = 0;
        let mut visibility = Vec::new();
        let mut materials = Vec::new();
        let mut lights = Vec::new();
        let mut emitters: Vec<EmitterTrack> = Vec::new();
        for t in &seq.tracks {
            materials.extend(MaterialTrack::of(t));
            lights.extend(LightTrack::of(t));
            if let (Some(n), Some("NiPSysEmitterCtlr")) = (&t.node_name, t.controller_type.as_deref()) {
                let priority = t.priority.unwrap_or(0) as i32;
                let k = match emitters.iter().position(|e| e.system == *n && e.priority == priority) {
                    Some(k) => k,
                    None => {
                        emitters.push(EmitterTrack { system: n.clone(), priority, rate: None, active: None });
                        emitters.len() - 1
                    }
                };
                match t.interpolator_id.as_deref() {
                    Some("BirthRate") => emitters[k].rate = t.values.clone().map(Arc::new),
                    Some("EmitterActive") => emitters[k].active = t.bools.clone(),
                    _ => {}
                }
            }
            if let (Some(n), Some(b), Some("NiVisController")) = (&t.node_name, &t.bools, t.controller_type.as_deref()) {
                visibility.push((n.clone(), t.priority.unwrap_or(0) as i32, b.clone()));
            }
            match (&t.node_name, &t.data) {
                (Some(n), TrackData::Transform(tt)) => {
                    tracks.push((n.clone(), Arc::new(tt.clone())));
                    priorities.push(t.priority.unwrap_or(0) as i32);
                }
                _ => skipped += 1,
            }
        }
        Clip {
            name: seq.name.clone().unwrap_or_default(),
            start: seq.start_time,
            stop: seq.stop_time,
            frequency: if seq.frequency > 0.0 { seq.frequency } else { 1.0 },
            cycle_type: seq.cycle_type,
            tracks,
            priorities,
            skipped,
            accum_root: seq.accum_root_name.clone().filter(|n| !n.is_empty()),
            // A text key at no finite time marks nothing (its phase would be NaN).
            text_keys: seq.text_keys.iter().filter(|(t, _)| t.is_finite()).cloned().collect(),
            visibility,
            emitters,
            materials,
            free: Vec::new(),
            free_visibility: Vec::new(),
            lights,
        }
    }

    /// Whether the clip can be played: its timing (and that of its tracks' own
    /// controllers) and every transform key's time and value are finite numbers within
    /// the limits above. A broken file's NaN would otherwise reach the bones, and from
    /// them the physics bodies that follow them.
    pub fn check(&self) -> Result<(), String> {
        if ![self.start, self.stop, self.stop - self.start].iter().all(|v| v.is_finite()) || !frequency_ok(self.frequency) {
            return Err(format!("timing out of range (start {}, stop {}, frequency {})", self.start, self.stop, self.frequency));
        }
        let cycles = self.free.iter().map(|f| &f.2).chain(self.free_visibility.iter().map(|f| &f.2)).chain(self.materials.iter().filter_map(|m| m.cycle.as_ref()));
        if let Some(c) = cycles.into_iter().find(|c| !cycle_ok(c)) {
            return Err(format!("a controller's own timing is out of range ({c:?})"));
        }
        let tracks = self.tracks.iter().map(|(n, t)| (n, t)).chain(self.free.iter().map(|(n, t, _)| (n, t)));
        for (name, t) in tracks {
            if let Some(fault) = track_fault(t) {
                return Err(format!("track {name}: {fault}"));
            }
        }
        Ok(())
    }

    /// The clip a model plays for its controllers that run by themselves (material
    /// `tracks`, transform tracks `free` and visibility tracks `shows`, each with its own
    /// [`OwnCycle`]) when it has no Idle sequence: it lasts as long as the longest of their
    /// cycles and loops; each track keeps its own time.
    pub fn from_controllers(tracks: Vec<MaterialTrack>, free: Vec<(String, Arc<TransformTrack>, OwnCycle)>, shows: Vec<(String, BoolTrack, OwnCycle)>) -> Clip {
        let stop = tracks
            .iter()
            .filter_map(|t| t.cycle.map(|c| c.period()))
            .chain(free.iter().map(|f| f.2.period()))
            .chain(shows.iter().map(|f| f.2.period()))
            .fold(1.0f32, f32::max);
        Clip {
            name: "Idle".into(),
            start: 0.0,
            stop,
            frequency: 1.0,
            cycle_type: 0,
            tracks: Vec::new(),
            priorities: Vec::new(),
            skipped: 0,
            accum_root: None,
            text_keys: Vec::new(),
            visibility: Vec::new(),
            emitters: Vec::new(),
            materials: tracks,
            free,
            free_visibility: shows,
            lights: Vec::new(),
        }
    }

    /// Whether the clip holds nothing but transform and visibility tracks of controllers
    /// that run by themselves (`free`, `free_visibility`): what a weapon's file holds for
    /// the clips of the actor holding it (a slide, the `##` parts shown and hidden,
    /// effects-animation-audit.md §1), which an item lying in the world does not play.
    pub fn only_free_motion(&self) -> bool {
        self.tracks.is_empty() && self.visibility.is_empty() && self.materials.is_empty() && (!self.free.is_empty() || !self.free_visibility.is_empty())
    }

    pub fn length(&self) -> f32 {
        (self.stop - self.start).max(0.0)
    }

    /// The speed the clip's accumulation root moves at over one cycle (units per second
    /// at frequency 1), from its track's translation at the start and stop times: the
    /// ground speed the clip was made for [observed: `locomotion\male\mtforward.kf`
    /// moves `Bip01` 102.3 units in 1.2 s, docs/spec-notes/animation.md §9.2]. `None`
    /// when the root has no track or the clip no length.
    pub fn root_speed(&self) -> Option<f32> {
        let t = self.accum_track()?;
        let (a, b) = (t.translation.sample(self.start)?, t.translation.sample(self.stop)?);
        let len = self.length();
        (len > 0.0).then(|| Vec2::new(b[0] - a[0], b[1] - a[1]).length() / len)
    }

    /// The accumulation root's ground position (x, y) at `secs` into the clip (clamped to
    /// its length), from its track; `None` without one.
    pub fn root_ground(&self, secs: f32) -> Option<Vec2> {
        let v = self.accum_track()?.translation.sample(self.start + secs.clamp(0.0, self.length()))?;
        let v = Vec2::new(v[0], v[1]);
        v.is_finite().then_some(v)
    }

    /// The farthest the accumulation root gets from where it starts along the ground over
    /// the clip (sampled at 61 points), game units: a clip whose root travels more than
    /// [`CARRY_MIN`] carries its actor (docs/spec-notes/actors.md §8.13).
    pub fn root_reach(&self) -> f32 {
        let Some(start) = self.root_ground(0.0) else { return 0.0 };
        (0..=60).filter_map(|i| self.root_ground(self.length() * i as f32 / 60.0)).map(|p| (p - start).length()).fold(0.0, f32::max)
    }

    /// How far the accumulation root moves along the ground from `from` to `to` seconds
    /// into the clip (x, y in the actor's axes: +y forward, +x right, actors.md §8.2).
    pub fn root_travel(&self, from: f32, to: f32) -> Vec2 {
        match (self.root_ground(from), self.root_ground(to)) {
            (Some(a), Some(b)) => b - a,
            _ => Vec2::ZERO,
        }
    }

    /// The transform track of the accumulation root, if one drives it.
    pub fn accum_track(&self) -> Option<&TransformTrack> {
        let r = self.accum_root.as_deref()?;
        self.tracks.iter().find(|(n, _)| n == r).map(|(_, t)| t.as_ref())
    }

    /// The accumulation root when no track drives it. Such a clip carries the root's
    /// whole bind offset on the node below it (`Bip01 NonAccum` under `Bip01` in
    /// `mtidle.kf` [observed, docs/spec-notes/actors.md §7]), so the root itself must be
    /// held at the identity while it plays [hypothesis: the accumulated motion of a clip
    /// without a root track is none].
    pub fn undriven_accum_root(&self) -> Option<&str> {
        self.accum_root.as_deref().filter(|r| !self.tracks.iter().any(|(n, _)| n == r))
    }
}

/// A clip playing on a set of entities.
#[derive(Component)]
pub struct Playing {
    pub name: String,
    pub source: String,
    pub start: f32,
    pub stop: f32,
    pub frequency: f32,
    pub cycle_type: i64,
    pub time: f32,
    pub tracks: Vec<(Entity, Arc<TransformTrack>)>,
    /// Tracks whose target node was not found.
    pub unmatched: usize,
    /// Play in place: the index in `tracks` of the accumulation root's track and the
    /// root's translation over one cycle, removed in proportion to the cycle phase (the
    /// rule of [`BlendPlayer`], docs/spec-notes/actors.md §8.2). Set by
    /// [`Playing::in_place`]; world objects leave it unset.
    pub root_motion: Option<(usize, Vec3)>,
    /// Held where it is: [`advance_instances`] does not move it on (a radio switched
    /// off, docs/spec-notes/activators.md §3).
    pub held: bool,
    /// Tracks of the clip's controllers that run by themselves ([`Clip::free`]), each on
    /// its own cycle after `time - start` seconds of play.
    pub free: Vec<(Entity, Arc<TransformTrack>, OwnCycle)>,
    /// The nodes the clip shows and hides ([`Clip::visibility`], and
    /// [`Clip::free_visibility`] on their own cycles), written by [`Self::show`].
    pub shows: Vec<Shown>,
    /// The clip's light tracks ([`Clip::lights`]), read by whoever draws the light
    /// (nvforever `lights.rs`) at [`Self::sequence_time`].
    pub lights: Vec<LightTrack>,
}

/// A node a playing clip shows and hides: its visibility track, its own timing (`None`:
/// the sequence's time), and what was last written.
#[derive(Debug, Clone)]
pub struct Shown {
    pub node: Entity,
    pub track: BoolTrack,
    pub cycle: Option<OwnCycle>,
    pub last: Option<bool>,
}

impl Playing {
    /// Start `clip` from its start time, binding tracks to entities by node name.
    pub fn new(source: &str, clip: &Clip, node: impl Fn(&str) -> Option<Entity>) -> Playing {
        let mut tracks = Vec::new();
        let mut unmatched = 0;
        for (n, tt) in &clip.tracks {
            match node(n) {
                Some(e) => tracks.push((e, tt.clone())),
                None => unmatched += 1,
            }
        }
        let free = clip.free.iter().filter_map(|(n, tt, c)| Some((node(n)?, tt.clone(), *c))).collect();
        let shown = |n: &str, track: &BoolTrack, cycle: Option<OwnCycle>| Some(Shown { node: node(n)?, track: track.clone(), cycle, last: None });
        let shows = clip
            .visibility
            .iter()
            .filter_map(|(n, _, t)| shown(n, t, None))
            .chain(clip.free_visibility.iter().filter_map(|(n, t, c)| shown(n, t, Some(*c))))
            .collect();
        Playing {
            name: clip.name.clone(),
            source: source.to_string(),
            start: clip.start,
            stop: clip.stop,
            frequency: clip.frequency,
            cycle_type: clip.cycle_type,
            time: clip.start,
            tracks,
            unmatched,
            root_motion: None,
            held: false,
            free,
            shows,
            lights: clip.lights.clone(),
        }
    }

    /// Remove the travel of `clip`'s accumulation root, if a track drives it (walk and
    /// run clips), so the clip plays in place; `node` is the binding given to [`Self::new`].
    pub fn in_place(mut self, clip: &Clip, node: impl Fn(&str) -> Option<Entity>) -> Playing {
        let root = clip.accum_root.as_deref().and_then(&node);
        let track = clip.accum_track();
        if let (Some(root), Some(track)) = (root, track) {
            let at = |t: f32| track.translation.sample(t).map_or(Vec3::ZERO, Vec3::from_array);
            if let Some(k) = self.tracks.iter().position(|(e, _)| *e == root) {
                // Keys far apart enough to overflow give no travel to remove.
                self.root_motion = Some((k, finite_or_zero(at(clip.stop) - at(clip.start))));
            }
        }
        self
    }

    /// Start at `phase` (0–1) of the cycle instead of the clip's start.
    pub fn at_phase(mut self, phase: f32) -> Playing {
        self.time = self.start + phase.clamp(0.0, 1.0) * (self.stop - self.start);
        self
    }

    /// The value the clip gives light `light`'s member `target` now, if it drives it.
    pub fn light_value(&self, light: &str, target: LightTarget) -> Option<[f32; 3]> {
        let t = self.sequence_time();
        self.lights.iter().find(|l| l.target == target && l.light == light)?.values.at(t)
    }

    /// The time within the sequence after the cycle rule.
    pub fn sequence_time(&self) -> f32 {
        cycle_time(self.time, self.start, self.stop, self.cycle_type)
    }

    /// Advance by `dt` seconds (unless `paused`) and pose the targets.
    pub fn step(&mut self, dt: f32, paused: bool, transforms: &mut Query<&mut Transform>) {
        if !paused {
            self.time += dt * self.frequency;
        }
        let t = self.sequence_time();
        let phase = if self.stop > self.start { (t - self.start) / (self.stop - self.start) } else { 0.0 };
        for (k, (e, track)) in self.tracks.iter().enumerate() {
            let Ok(mut tr) = transforms.get_mut(*e) else { continue };
            let mut posed = *tr;
            if let Some(v) = track.translation.sample(t) {
                posed.translation = Vec3::from_array(v);
                if let Some((_, per_cycle)) = self.root_motion.filter(|r| r.0 == k) {
                    posed.translation -= per_cycle * phase;
                }
            }
            if let Some([w, x, y, z]) = track.rotation_at(t) {
                posed.rotation = Quat::from_xyzw(x, y, z, w).normalize();
            }
            if let Some([s]) = track.scale.sample(t) {
                posed.scale = Vec3::splat(s);
            }
            tr.set_if_neq(posed);
        }
        let clock = self.time - self.start;
        for (e, track, cycle) in &self.free {
            let Ok(mut tr) = transforms.get_mut(*e) else { continue };
            let t = cycle.time(clock);
            let mut posed = *tr;
            if let Some(v) = track.translation.sample(t) {
                posed.translation = Vec3::from_array(v);
            }
            if let Some([w, x, y, z]) = track.rotation_at(t) {
                posed.rotation = Quat::from_xyzw(x, y, z, w).normalize();
            }
            if let Some([s]) = track.scale.sample(t) {
                posed.scale = Vec3::splat(s);
            }
            tr.set_if_neq(posed);
        }
    }
}

impl Playing {
    /// The visibility each node of [`Self::shows`] takes now, by its track at the
    /// sequence time (a free controller's at its own time after `time - start` seconds of
    /// play): (index in `shows`, shown), for those whose track gives a value.
    pub fn visible_now(&self) -> impl Iterator<Item = (usize, bool)> + '_ {
        let t = self.sequence_time();
        let clock = self.time - self.start;
        self.shows.iter().enumerate().filter_map(move |(i, s)| Some((i, s.track.at(s.cycle.map_or(t, |c| c.time(clock)))?)))
    }

    /// Show and hide the nodes of [`Self::shows`] as their tracks say now
    /// (docs/spec-notes/animation.md §9.7); a node is written only when its value changes
    /// from what was last written, so something else hiding it in between is not undone
    /// until the track switches. Returns how many nodes switched (a node's first write
    /// not counted).
    pub fn show(&mut self, visibility: &mut Query<&mut Visibility>) -> usize {
        let now: Vec<(usize, bool)> = self.visible_now().filter(|&(i, on)| self.shows[i].last != Some(on)).collect();
        let mut switched = 0;
        for &(i, on) in &now {
            let s = &mut self.shows[i];
            switched += usize::from(s.last.is_some());
            s.last = Some(on);
            if let Ok(mut v) = visibility.get_mut(s.node) {
                *v = if on { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
        switched
    }
}

/// A clip's tracks bound to a skeleton's node indices, shared by every actor of that
/// skeleton, with the accumulation-root rule of docs/spec-notes/actors.md §8.2.
#[derive(Debug)]
pub struct BoundClip {
    pub clip: Arc<Clip>,
    /// (node index, track, priority).
    pub tracks: Vec<(usize, Arc<TransformTrack>, i32)>,
    /// The accumulation root's node index, if the skeleton has it.
    pub root: Option<usize>,
    /// The index in `tracks` of the root's track, the root's translation over one
    /// cycle, which sampling removes so the clip plays in place, and the root's height
    /// where the track starts, which sampling removes too: the root rises and falls only
    /// relative to it (actors.md §8.2; the 2ha/2hh `Holster` clips hold `Bip01` 67 units
    /// up with no `Bip01 NonAccum` track, which over an idle's pelvis height lifted the
    /// whole body off the ground, docs/spec-notes/floating-actors.md).
    pub root_track: Option<(usize, Vec3, f32)>,
    /// The clip's travel is its actor's (attack clips that lunge, actors.md §8.13): sampling holds
    /// the root's ground position (x, y) where the track starts, this, so all of the
    /// travel is taken out of the pose (not only the linear accumulation of §8.2), and
    /// whoever plays the clip moves the actor by [`BoundClip::travel`] instead. Set by
    /// [`BoundClip::carried`].
    pub carried: Option<Vec2>,
}

/// A clip whose root gets no farther than this from where it starts (game units, along
/// the ground) does not carry its actor ([`BoundClip::carried`]): it steps in place by
/// §8.2's rule [design: the lunges (the forward power attacks, 229 to 1427) carry, the
/// ordinary blows (13 to 72, a giant ant's 128) mostly not; carrying the dogs' 55-unit
/// bites brought them to 37 units of their target for the next, against the original's
/// median 133 (smoke combat-dogs, npc-behaviour-compare.md §3); actors.md §8.13].
pub const CARRY_MIN: f32 = 100.0;

impl BoundClip {
    /// Bind `clip`'s tracks to node indices by name (`node`).
    pub fn bind(clip: Arc<Clip>, node: impl Fn(&str) -> Option<usize>) -> BoundClip {
        let tracks: Vec<(usize, Arc<TransformTrack>, i32)> = clip
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(k, (n, t))| Some((node(n)?, t.clone(), clip.priorities.get(k).copied().unwrap_or(0))))
            .collect();
        let root = clip.accum_root.as_deref().and_then(&node);
        let root_track = root.and_then(|r| tracks.iter().position(|(i, _, _)| *i == r)).map(|k| {
            let tt = &tracks[k].1;
            let at = |t: f32| tt.translation.sample(t).map_or(Vec3::ZERO, Vec3::from_array);
            let start = at(clip.start);
            (k, finite_or_zero(at(clip.stop) - start), start.z)
        });
        BoundClip { clip, tracks, root, root_track, carried: None }
    }

    /// This clip carrying its actor ([`BoundClip::carried`]), when its root travels
    /// (more than [`CARRY_MIN`] from where it starts, at any time); otherwise unchanged.
    pub fn carried(mut self) -> BoundClip {
        if self.root_track.is_some() && self.clip.root_reach() > CARRY_MIN {
            self.carried = self.clip.root_ground(0.0);
        }
        self
    }

    /// How far a carried clip's root moves (x, y in the actor's axes: +y forward, +x
    /// right, actors.md §8.2) from `from` to `to` seconds into the clip (each clamped to
    /// its length); zero for a clip that does not carry its actor.
    pub fn travel(&self, from: f32, to: f32) -> Vec2 {
        if self.carried.is_none() {
            return Vec2::ZERO;
        }
        self.clip.root_travel(from, to)
    }

    /// Sequence time at `phase` (0–1 of the cycle).
    pub fn time_at(&self, phase: f32) -> f32 {
        self.clip.start + phase * (self.clip.stop - self.clip.start)
    }
}

/// One weighted clip of a [`BlendPlayer`].
#[derive(Debug, Clone)]
pub struct Layer {
    pub clip: Arc<BoundClip>,
    /// Where in the cycle (0–1).
    pub phase: f32,
    pub weight: f32,
}

/// How many clips a [`BlendPlayer`] blends at once: locomotion 0 and 1, weapon or stance
/// 2 and 3, idles and attacks 4 and 5, the idle under the movement 6, a robot's
/// `Talking` 7 (nvforever's `talk_clips.rs`).
pub const MAX_LAYERS: usize = 8;

/// The priority of the identity a clip gives an accumulation root it does not drive: below
/// every track, so any clip that drives the root wins.
const UNDRIVEN_ROOT_PRIORITY: i32 = i32::MIN;

/// One clip's sample of one node.
#[derive(Debug, Clone, Copy)]
struct Contrib {
    t: Vec3,
    r: Quat,
    s: f32,
    w: f32,
    p: i32,
}

const CONTRIB_NONE: Contrib = Contrib { t: Vec3::ZERO, r: Quat::IDENTITY, s: 1.0, w: 0.0, p: 0 };

/// A running weighted average of transforms (rotations nlerped along the shorter arc).
#[derive(Debug, Clone, Copy)]
struct Mix {
    t: Vec3,
    r: Quat,
    s: f32,
    w: f32,
}

impl Mix {
    const EMPTY: Mix = Mix { t: Vec3::ZERO, r: Quat::IDENTITY, s: 1.0, w: 0.0 };

    fn add(&mut self, t: Vec3, r: Quat, s: f32, w: f32) {
        if w <= 0.0 {
            return;
        }
        if self.w == 0.0 {
            *self = Mix { t, r, s, w };
            return;
        }
        let f = w / (self.w + w);
        let r = if self.r.dot(r) < 0.0 { -r } else { r };
        self.t = self.t.lerp(t, f);
        self.r = Quat::from_vec4(Vec4::from(self.r).lerp(Vec4::from(r), f));
        self.s += (s - self.s) * f;
        self.w += w;
    }
}

/// Blend one node's samples by priority (docs/spec-notes/actors.md §9.4): from the highest
/// priority down, each priority group takes its summed weight, at most what is left of 1,
/// shared by weight within the group; the rest goes to `bind`.
fn blend_by_priority(c: &mut [Contrib], bind: Transform) -> Transform {
    // Highest priority first (insertion sort: at most MAX_LAYERS entries).
    for i in 1..c.len() {
        let mut j = i;
        while j > 0 && c[j - 1].p < c[j].p {
            c.swap(j - 1, j);
            j -= 1;
        }
    }
    let mut mix = Mix::EMPTY;
    let mut left = 1.0f32;
    let mut k = 0;
    while k < c.len() && left > 1e-6 {
        let end = (k..c.len()).find(|&e| c[e].p != c[k].p).unwrap_or(c.len());
        let group: f32 = c[k..end].iter().map(|x| x.w).sum();
        if group > 0.0 {
            let share = group.min(left);
            for x in &c[k..end] {
                mix.add(x.t, x.r, x.s, x.w / group * share);
            }
            left -= share;
        }
        k = end;
    }
    if left > 1e-3 {
        mix.add(bind.translation, bind.rotation, bind.scale.x, left);
    }
    let out = Transform { translation: mix.t, rotation: mix.r.normalize(), scale: Vec3::splat(mix.s) };
    // Finite samples far enough apart can still overflow the mix: the rest pose then.
    if out.translation.is_finite() && out.rotation.is_finite() && out.scale.is_finite() {
        out
    } else {
        bind
    }
}

/// `v`, or zero where a part of it is not a finite number.
fn finite_or_zero(v: Vec3) -> Vec3 {
    if v.is_finite() {
        v
    } else {
        Vec3::ZERO
    }
}

/// Poses a skeleton from up to [`MAX_LAYERS`] weighted clips (docs/spec-notes/actors.md
/// §8.3 and §9.4): per node, the clips that drive it are blended by track priority and
/// weight (translations and scales lerped, rotations nlerped along the shorter arc), with
/// any weight left over going to the node's bind transform; a node no clip drives any
/// more returns to its bind transform. The accumulation root plays in place: a clip that
/// drives it has its per-cycle travel removed, and one that does not holds it at the
/// identity (below every priority). Buffers are allocated once, so posing allocates
/// nothing.
#[derive(Component, Debug)]
pub struct BlendPlayer {
    /// The skeleton's node entities, by node index.
    pub nodes: Vec<Entity>,
    /// The nodes' local bind transforms.
    pub bind: Arc<Vec<Transform>>,
    pub layers: [Option<Layer>; MAX_LAYERS],
    contribs: Vec<([Contrib; MAX_LAYERS], u8)>,
    touched: Vec<usize>,
    /// Nodes posed in the previous call, and a per-node flag for this call's.
    posed: Vec<usize>,
    driven: Vec<bool>,
    /// Nodes with a rest pose of their own in place of the bind transform
    /// ([`Self::set_rest`]).
    rest: Vec<(usize, Transform)>,
    /// How many times [`Self::set_rest`] changed a rest pose.
    rest_changes: u32,
    /// What the last pose was made from, when [`Self::pose_held`] made it.
    posed_from: Option<PosedFrom>,
}

/// What a [`BlendPlayer`]'s pose is made from, besides its bind transforms.
#[derive(Debug, Clone)]
struct PosedFrom {
    layers: [Option<Layer>; MAX_LAYERS],
    rest_changes: u32,
}

/// Whether layers and rest poses as they stand now give the pose made from `from`: the
/// same clips (the same `Arc`, whatever their contents) at exactly the same phases and
/// weights in the same slots. Without a pose made yet they do not.
fn pose_stands(from: Option<&PosedFrom>, layers: &[Option<Layer>; MAX_LAYERS], rest_changes: u32) -> bool {
    let same = |a: &Option<Layer>, b: &Option<Layer>| match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(&a.clip, &b.clip) && a.phase.to_bits() == b.phase.to_bits() && a.weight.to_bits() == b.weight.to_bits(),
        _ => false,
    };
    from.is_some_and(|f| f.rest_changes == rest_changes && f.layers.iter().zip(layers).all(|(a, b)| same(a, b)))
}

impl BlendPlayer {
    pub fn new(nodes: Vec<Entity>, bind: Arc<Vec<Transform>>) -> BlendPlayer {
        let n = nodes.len();
        BlendPlayer {
            nodes,
            bind,
            layers: Default::default(),
            contribs: vec![([CONTRIB_NONE; MAX_LAYERS], 0); n],
            touched: Vec::with_capacity(n),
            posed: Vec::with_capacity(n),
            driven: vec![false; n],
            rest: Vec::new(),
            rest_changes: 0,
            posed_from: None,
        }
    }

    /// Give node `i` a rest pose in place of its bind transform (`None`: the bind
    /// transform again): what it takes where no clip drives it, and what fills the weight
    /// the clips that drive it leave.
    pub fn set_rest(&mut self, i: usize, pose: Option<Transform>) {
        if self.rest.iter().find(|(n, _)| *n == i).map(|(_, p)| *p) != pose {
            self.rest_changes = self.rest_changes.wrapping_add(1);
        }
        self.rest.retain(|(n, _)| *n != i);
        if let Some(p) = pose {
            self.rest.push((i, p));
        }
    }

    /// Node `i`'s rest pose: its own ([`Self::set_rest`]) or its bind transform.
    fn rest_of(&self, i: usize) -> Transform {
        self.rest.iter().find(|(n, _)| *n == i).map_or_else(|| self.bind.get(i).copied().unwrap_or_default(), |(_, p)| *p)
    }

    fn add(&mut self, i: usize, c: Contrib) {
        let Some((slots, n)) = self.contribs.get_mut(i) else { return };
        if *n == 0 {
            self.touched.push(i);
        }
        if (*n as usize) < MAX_LAYERS {
            slots[*n as usize] = c;
            *n += 1;
        }
    }

    /// The blended local transforms (node index, transform) of every node a layer drives,
    /// and the bind transforms of the nodes the layers stopped driving; returns the number
    /// of tracks sampled. `out` is cleared first.
    pub fn sample(&mut self, out: &mut Vec<(usize, Transform)>) -> usize {
        out.clear();
        self.posed_from = None;
        let mut sampled = 0;
        let layers = std::mem::take(&mut self.layers);
        // A weight or phase that is not a finite number (a clip of no length divided by)
        // gives no layer or the cycle's start, never NaN bones.
        for layer in layers.iter().flatten().filter(|l| l.weight > 0.0 && l.weight.is_finite()) {
            let (bc, w) = (&layer.clip, layer.weight);
            let at = if layer.phase.is_finite() { layer.phase } else { 0.0 };
            let time = cycle_time(bc.time_at(at), bc.clip.start, bc.clip.stop, bc.clip.cycle_type);
            let phase = if bc.clip.stop > bc.clip.start { (time - bc.clip.start) / (bc.clip.stop - bc.clip.start) } else { 0.0 };
            for (j, (i, track, p)) in bc.tracks.iter().enumerate() {
                let b = self.rest_of(*i);
                let mut t = track.translation.sample(time).map_or(b.translation, Vec3::from_array);
                if let Some((rj, per_cycle, start_z)) = bc.root_track {
                    if rj == j {
                        match bc.carried {
                            // Its travel is the actor's: all of it out of the pose.
                            Some(at) => (t.x, t.y) = (at.x, at.y),
                            None => t -= per_cycle * phase,
                        }
                        t.z -= start_z;
                    }
                }
                let r = track.rotation_at(time).map_or(b.rotation, |[w, x, y, z]| Quat::from_xyzw(x, y, z, w).normalize());
                let s = track.scale.sample(time).map_or(b.scale.x, |[s]| s);
                self.add(*i, Contrib { t, r, s, w, p: *p });
                sampled += 1;
            }
            if let (Some(root), None) = (bc.root, bc.root_track) {
                self.add(root, Contrib { t: Vec3::ZERO, r: Quat::IDENTITY, s: 1.0, w, p: UNDRIVEN_ROOT_PRIORITY });
            }
        }
        self.layers = layers;
        for k in 0..self.touched.len() {
            let i = self.touched[k];
            let rest = self.rest_of(i);
            let (slots, n) = &mut self.contribs[i];
            out.push((i, blend_by_priority(&mut slots[..*n as usize], rest)));
            *n = 0;
            self.driven[i] = true;
        }
        // Nodes with a rest pose of their own that no clip drives take it.
        for k in 0..self.rest.len() {
            let (i, p) = self.rest[k];
            if self.driven.get(i) == Some(&false) {
                out.push((i, p));
                self.driven[i] = true;
                self.touched.push(i);
            }
        }
        // Nodes posed last time that no clip drives now go back to their bind transform.
        for &i in &self.posed {
            if !self.driven[i] {
                out.push((i, self.bind.get(i).copied().unwrap_or_default()));
            }
        }
        self.posed.clear();
        for &i in &self.touched {
            self.driven[i] = false;
            self.posed.push(i);
        }
        self.touched.clear();
        sampled
    }

    /// Pose the nodes: [`Self::sample`] written to their transforms. `scratch` is reused
    /// between calls.
    pub fn pose(&mut self, scratch: &mut Vec<(usize, Transform)>, transforms: &mut Query<&mut Transform>) -> usize {
        let n = self.sample(scratch);
        for (i, tr) in scratch.iter() {
            if let Some(Ok(mut t)) = self.nodes.get(*i).map(|e| transforms.get_mut(*e)) {
                t.set_if_neq(*tr);
            }
        }
        n
    }

    /// [`Self::pose`] for a pose that may stand as it is (a finished death clip): nothing
    /// is sampled while the last pose made here still holds, and `None` says so; otherwise
    /// the number of tracks sampled.
    ///
    /// The pose is a function of the layers, the rest poses and the bind transforms alone,
    /// so with the same layers and rest poses [`Self::pose`] would write the values the
    /// last call wrote. It would also put back any other system's write to those nodes
    /// (foot IK easing out on a corpse, the lips giving a turned head back), so a node
    /// written since the calling system last ran is sampled for too: every frame ends with
    /// the bones [`Self::pose`] would have left. The nodes checked are those it would
    /// write; the caller's own writes do not count as changes on its next run.
    pub fn pose_held(&mut self, scratch: &mut Vec<(usize, Transform)>, transforms: &mut Query<&mut Transform>) -> Option<usize> {
        if pose_stands(self.posed_from.as_ref(), &self.layers, self.rest_changes) && !self.nodes_written(transforms) {
            return None;
        }
        let n = self.pose(scratch, transforms);
        self.posed_from = Some(PosedFrom { layers: self.layers.clone(), rest_changes: self.rest_changes });
        Some(n)
    }

    /// [`Self::pose`] for callers that sample apart from writing (several actors at once
    /// on the compute pool, the writes made afterwards in a fixed order): the sampled
    /// transforms are added to `out` by node entity, nothing is written.
    pub fn sample_into(&mut self, scratch: &mut Vec<(usize, Transform)>, out: &mut Vec<(Entity, Transform)>) -> usize {
        let n = self.sample(scratch);
        out.extend(scratch.iter().filter_map(|(i, tr)| self.nodes.get(*i).map(|e| (*e, *tr))));
        n
    }

    /// [`Self::pose_held`] apart from writing, as [`Self::sample_into`]: `written` is
    /// [`Self::written_since`], asked before (it reads the transforms).
    pub fn sample_held_into(&mut self, written: bool, scratch: &mut Vec<(usize, Transform)>, out: &mut Vec<(Entity, Transform)>) -> Option<usize> {
        if pose_stands(self.posed_from.as_ref(), &self.layers, self.rest_changes) && !written {
            return None;
        }
        let n = self.sample_into(scratch, out);
        self.posed_from = Some(PosedFrom { layers: self.layers.clone(), rest_changes: self.rest_changes });
        Some(n)
    }

    /// Whether a node posed last time was written since the calling system last ran
    /// ([`Self::pose_held`]'s check, for [`Self::sample_held_into`]).
    pub fn written_since(&self, transforms: &mut Query<&mut Transform>) -> bool {
        self.nodes_written(transforms)
    }

    /// Forget the pose [`Self::pose_held`] made: its next call samples.
    pub fn forget_pose(&mut self) {
        self.posed_from = None;
    }

    /// Whether a node posed last time was written since the calling system last ran.
    fn nodes_written(&self, transforms: &mut Query<&mut Transform>) -> bool {
        self.posed.iter().any(|&i| self.nodes.get(i).is_some_and(|&e| transforms.get_mut(e).is_ok_and(|t| t.is_changed())))
    }
}

/// Every sequence that can be played on the current mesh, and the one playing.
#[derive(Resource, Default)]
pub struct AnimState {
    pub available: Vec<(String, Sequence)>,
    pub current: Option<usize>,
    pub playing: Option<Playing>,
    pub paused: bool,
    /// The accumulation root the current sequence holds at the identity
    /// ([`Clip::undriven_accum_root`]), with its bind transform to put back.
    pub held: Option<(Entity, Transform)>,
    /// Transforms [`advance`] writes before posing (holding or releasing that root).
    pending: Vec<(Entity, Transform)>,
}

impl AnimState {
    /// Play sequence `index` on the nodes. `bind` gives the nodes' local bind transforms,
    /// for the accumulation root a previous sequence held.
    pub fn start(&mut self, index: usize, nodes: &HashMap<String, Entity>, bind: &HashMap<Entity, Transform>) {
        let Some((source, seq)) = self.available.get(index) else { return };
        let clip = Clip::from_sequence(seq);
        if let Err(e) = clip.check() {
            warn!("{source}: sequence {} not played: {e}", clip.name);
            return;
        }
        // Walk and run clips play in place, as on actors (docs/spec-notes/actors.md §8.2).
        let playing = Playing::new(source, &clip, |n| nodes.get(n).copied()).in_place(&clip, |n| nodes.get(n).copied());
        let hold = clip.undriven_accum_root().and_then(|r| nodes.get(r)).copied();
        self.current = Some(index);
        self.playing = Some(playing);
        self.release();
        if let Some(e) = hold {
            self.pending.push((e, Transform::IDENTITY));
            self.held = Some((e, bind.get(&e).copied().unwrap_or_default()));
        }
    }

    /// Stop playing; a held accumulation root goes back to its bind transform.
    pub fn stop(&mut self) {
        self.current = None;
        self.playing = None;
        self.release();
    }

    fn release(&mut self) {
        if let Some(back) = self.held.take() {
            self.pending.push(back);
        }
    }
}

/// Map a running time onto the sequence according to nif.xml CycleType
/// (0 loop, 1 reverse = ping-pong, 2 clamp) [documented: nif.xml CycleType].
pub fn cycle_time(t: f32, start: f32, stop: f32, cycle: i64) -> f32 {
    let len = stop - start;
    if len <= 0.0 {
        return start;
    }
    let x = t - start;
    match cycle {
        2 => start + x.clamp(0.0, len),
        1 => {
            let m = x.rem_euclid(2.0 * len);
            start + if m > len { 2.0 * len - m } else { m }
        }
        _ => start + x.rem_euclid(len),
    }
}

pub fn advance(time: Res<Time>, mut state: ResMut<AnimState>, mut transforms: Query<&mut Transform>) {
    for (e, t) in std::mem::take(&mut state.pending) {
        if let Ok(mut tr) = transforms.get_mut(e) {
            tr.set_if_neq(t);
        }
    }
    let paused = state.paused;
    if let Some(p) = state.playing.as_mut() {
        p.step(time.delta_secs(), paused, &mut transforms);
    }
}

/// Advance every [`Playing`] component (world objects with their own animation), posing
/// their nodes and showing and hiding them ([`Playing::show`]). Returns how many nodes
/// switched between shown and hidden.
pub fn advance_instances(time: Res<Time>, mut playing: Query<&mut Playing>, mut transforms: Query<&mut Transform>, mut visibility: Query<&mut Visibility>) -> usize {
    let dt = time.delta_secs();
    let mut switched = 0;
    for mut p in &mut playing {
        let held = p.held;
        p.step(dt, held, &mut transforms);
        if !p.shows.is_empty() {
            switched += p.show(&mut visibility);
        }
    }
    switched
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::{RunSystemOnce, SystemState};
    use fnv_nif::anim::{Channel, Key, KeyKind, Keys, Track};
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

    #[test]
    fn a_clip_with_a_nan_key_is_refused() {
        let track = |x: f32| TransformTrack {
            translation: Channel::Keys(Keys { kind: KeyKind::Linear, keys: vec![Key { time: 0.0, value: [x, 0.0, 0.0], forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] }] }),
            rotation: Channel::Static(None),
            euler: None,
            scale: Channel::Static(Some([1.0])),
        };
        let mut clip = Clip::from_controllers(Vec::new(), Vec::new(), Vec::new());
        clip.tracks.push(("Bip01".into(), Arc::new(track(1.0))));
        assert_eq!(clip.check(), Ok(()));
        clip.tracks.push(("Bip01 Head".into(), Arc::new(track(f32::NAN))));
        assert_eq!(clip.check(), Err("track Bip01 Head: a key is not a finite number".into()));
        clip.tracks.pop();
        clip.stop = f32::INFINITY;
        assert!(clip.check().is_err());
    }

    #[test]
    fn a_clip_out_of_range_is_refused() {
        let track = |x: f32, sc: f32| TransformTrack {
            translation: Channel::Keys(Keys { kind: KeyKind::Linear, keys: vec![Key { time: 0.0, value: [x, 0.0, 0.0], forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] }] }),
            rotation: Channel::Static(None),
            euler: None,
            scale: Channel::Static(Some([sc])),
        };
        let clip = |t: TransformTrack| {
            let mut c = Clip::from_controllers(Vec::new(), Vec::new(), Vec::new());
            c.tracks.push(("Bip01".into(), Arc::new(t)));
            c
        };
        // The game's own extremes play (scale 0 hides a node; large values are in its files).
        assert_eq!(clip(track(2.9e6, 0.0)).check(), Ok(()));
        assert_eq!(clip(track(1.0, -0.2)).check(), Ok(()));
        assert_eq!(clip(track(1e9, 1e4)).check(), Ok(()));
        assert!(clip(track(f32::INFINITY, 1.0)).check().is_err());
        let mut c = clip(track(1.0, 1.0));
        c.frequency = 0.0;
        assert!(c.check().is_err());
        c.frequency = 1e6;
        assert!(c.check().is_err());
        let own = OwnCycle { start: 0.0, stop: 1.0, frequency: 1.0, phase: f32::NAN, cycle_type: 0 };
        let mut c = Clip::from_controllers(Vec::new(), Vec::new(), Vec::new());
        c.free.push(("Bip01".into(), Arc::new(track(1.0, 1.0)), own));
        assert!(c.check().is_err());
    }

    #[test]
    fn an_own_controller_runs_on_the_effect_clock_from_zero() {
        // `impactexplosiondirt.nif` FastMesh03's alpha: clamp, 0.133 s to 0.433 s. Its
        // first key holds until 0.133 s of the effect, then the keys play in step.
        let c = OwnCycle { start: 0.133, stop: 0.433, frequency: 1.0, phase: 0.0, cycle_type: 2 };
        assert_eq!(c.time(0.0), 0.133);
        assert!((c.time(0.2) - 0.2).abs() < 1e-6);
        assert!((c.time(1.0) - 0.433).abs() < 1e-6);
        // A looping one from 0 (the scrolling smoke) is as before.
        let l = OwnCycle { start: 0.0, stop: 13.3, frequency: 1.0, phase: 0.0, cycle_type: 0 };
        assert!((l.time(14.3) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_free_visibility_track_runs_on_its_own_cycle_from_the_phase() {
        // As `fxdustmeshtube01.nif`'s DustTube: shown, hidden from 4.03 s to 7.43 s, a
        // 56.7 s loop of its own (nif.md §6).
        let track = BoolTrack { value: None, keys: vec![(0.0, true), (4.0, true), (4.033, false), (7.433, false), (7.467, true), (56.667, true)] };
        let cycle = OwnCycle { start: 0.0, stop: 56.667, frequency: 1.0, phase: 0.0, cycle_type: 0 };
        let clip = Clip::from_controllers(Vec::new(), Vec::new(), vec![("DustTube".into(), track, cycle)]);
        assert!((clip.stop - 56.667).abs() < 1e-4);
        // Nothing but a controller of its own: an item holding only this stays still.
        assert!(clip.only_free_motion());
        let mut p = Playing::new("t", &clip, |_| Some(Entity::PLACEHOLDER));
        let at = |p: &Playing| p.visible_now().map(|(_, on)| on).next();
        assert_eq!(at(&p), Some(true));
        p.time = 5.0;
        assert_eq!(at(&p), Some(false));
        p.time = 56.667 + 5.0;
        assert_eq!(at(&p), Some(false));
        p.time = 56.667 + 8.0;
        assert_eq!(at(&p), Some(true));
        // A sequence's visibility track follows the sequence time.
        let mut idle = Clip::from_controllers(Vec::new(), Vec::new(), Vec::new());
        idle.stop = 1.0;
        idle.visibility.push(("BrightSpots01".into(), 0, BoolTrack { value: None, keys: vec![(0.0, true), (0.033, false)] }));
        let mut p = Playing::new("t", &idle, |_| Some(Entity::PLACEHOLDER)).at_phase(0.5);
        assert_eq!(at(&p), Some(false));
        p.time = 1.0;
        assert_eq!(at(&p), Some(true));
    }

    fn seq(tracks: &[&str]) -> Sequence {
        let track = |n: &&str| Track {
            node_name: Some(n.to_string()),
            controller_type: None,
            priority: None,
            bools: None,
            controller_id: None,
            values: None,
            interpolator_id: None,
            data: TrackData::Transform(TransformTrack {
                translation: Channel::Static(Some([0.0, -0.296, 66.33])),
                rotation: Channel::Static(None),
                euler: None,
                scale: Channel::Static(None),
            }),
        };
        Sequence {
            block: 0,
            name: None,
            start_time: 0.0,
            stop_time: 1.0,
            frequency: 1.0,
            cycle_type: 0,
            accum_root_name: Some("Bip01".into()),
            text_keys: Vec::new(),
            tracks: tracks.iter().map(track).collect(),
        }
    }

    #[test]
    fn light_tracks_follow_the_playing_clip() {
        use fnv_nif::anim::{Key, KeyKind, Keys};
        // The lantern light's Idle: diffuse 0.78 → 0.65 red over 0.6 s, looping over 3 s
        // (`lamplightwarm.nif`, docs/spec-notes/model-lights.md §1); an ambient track and
        // a material track are not light tracks.
        let key = |time: f32, r: f32| Key { time, value: [r, 0.58, 0.16], forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] };
        let colour = ValueTrack::Point3(Channel::Keys(Keys { kind: KeyKind::Linear, keys: vec![key(0.0, 0.78), key(0.6, 0.65), key(3.0, 0.78)] }));
        let light = |ty: &str, id: &str| Track {
            node_name: Some("object0".into()),
            controller_type: Some(ty.into()),
            priority: None,
            bools: None,
            controller_id: Some(id.into()),
            values: Some(colour.clone()),
            interpolator_id: None,
            data: TrackData::Other("NiPoint3Interpolator".into()),
        };
        let mut s = seq(&[]);
        s.stop_time = 3.0;
        s.tracks = vec![light("NiLightColorController", "Diffuse"), light("NiLightColorController", "Ambient"), light("NiMaterialColorController", "SELF_ILLUM")];
        let clip = Clip::from_sequence(&s);
        assert_eq!(clip.lights.len(), 1);
        assert_eq!(clip.lights[0].target, LightTarget::Diffuse);
        let mut p = Playing::new("lantern", &clip, |_| None);
        p.time = 3.6; // one cycle and 0.6 s on
        let c = p.light_value("object0", LightTarget::Diffuse).unwrap();
        assert!((c[0] - 0.65).abs() < 1e-4, "{c:?}");
        assert_eq!(p.light_value("object0", LightTarget::Dimmer), None);
        assert_eq!(p.light_value("object1", LightTarget::Diffuse), None);
    }

    #[test]
    fn undriven_accum_root_is_held_then_restored() {
        let (root, non_accum) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let nodes = HashMap::from([("Bip01".to_string(), root), ("Bip01 NonAccum".to_string(), non_accum)]);
        let bind_root = Transform::from_xyz(0.0, 0.657, 67.771).with_rotation(Quat::from_rotation_z(FRAC_PI_2));
        let bind = HashMap::from([(root, bind_root), (non_accum, Transform::IDENTITY)]);
        let mut state = AnimState {
            available: vec![
                ("idle".into(), seq(&["Bip01 NonAccum"])),
                ("walk".into(), seq(&["Bip01", "Bip01 NonAccum"])),
            ],
            ..Default::default()
        };
        // A clip without a Bip01 track holds Bip01 at the identity.
        state.start(0, &nodes, &bind);
        assert_eq!(std::mem::take(&mut state.pending), [(root, Transform::IDENTITY)]);
        // Switching to the same clip keeps holding it.
        state.start(0, &nodes, &bind);
        assert_eq!(std::mem::take(&mut state.pending), [(root, bind_root), (root, Transform::IDENTITY)]);
        // A clip that drives Bip01 gets it back at its bind pose first.
        state.start(1, &nodes, &bind);
        assert_eq!(std::mem::take(&mut state.pending), [(root, bind_root)]);
        assert_eq!(state.held, None);
        // Stopping releases a held root.
        state.start(0, &nodes, &bind);
        state.pending.clear();
        state.stop();
        assert_eq!(state.pending, [(root, bind_root)]);
        assert!(state.playing.is_none() && state.current.is_none());
    }

    fn track(t: Channel<3>, r: Channel<4>) -> Arc<TransformTrack> {
        Arc::new(TransformTrack { translation: t, rotation: r, euler: None, scale: Channel::Static(None) })
    }

    fn key3(time: f32, value: [f32; 3]) -> Key<3> {
        Key { time, value, forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] }
    }

    fn clip(len: f32, tracks: Vec<(&str, Arc<TransformTrack>)>) -> Arc<Clip> {
        prio_clip(len, tracks.into_iter().map(|(n, t)| (n, t, 30)).collect())
    }

    fn prio_clip(len: f32, tracks: Vec<(&str, Arc<TransformTrack>, i32)>) -> Arc<Clip> {
        Arc::new(Clip {
            name: String::new(),
            start: 0.0,
            stop: len,
            frequency: 1.0,
            cycle_type: 0,
            priorities: tracks.iter().map(|t| t.2).collect(),
            tracks: tracks.into_iter().map(|(n, t, _)| (n.to_string(), t)).collect(),
            skipped: 0,
            accum_root: Some("Bip01".into()),
            text_keys: Vec::new(),
            visibility: Vec::new(),
            emitters: Vec::new(),
            materials: Vec::new(),
            free: Vec::new(),
            free_visibility: Vec::new(),
            lights: Vec::new(),
        })
    }

    /// Nodes 1 `Bip01`, 2 `Bip01 NonAccum`, 3 `Other`; node 0 is the actor's root.
    fn player() -> (BlendPlayer, impl Fn(&str) -> Option<usize>) {
        let nodes = (1..=4).map(|i| Entity::from_raw_u32(i).unwrap()).collect();
        let bind = vec![Transform::IDENTITY, Transform::from_xyz(0.0, 0.657, 67.771), Transform::IDENTITY, Transform::from_xyz(5.0, 0.0, 0.0)];
        let ix = |n: &str| ["Bip01", "Bip01 NonAccum", "Other"].iter().position(|x| *x == n).map(|i| i + 1);
        (BlendPlayer::new(nodes, Arc::new(bind)), ix)
    }

    /// A clip with one track, on node 0 of the player in the `pose_held` tests.
    fn on_node_0(x: f32) -> Arc<BoundClip> {
        let c = clip(1.0, vec![("Other", pose([x, 0.0, 0.0]))]);
        Arc::new(BoundClip::bind(c, |n| (n == "Other").then_some(0)))
    }

    #[test]
    fn a_pose_stands_only_while_what_makes_it_does() {
        let (a, b) = (on_node_0(1.0), on_node_0(1.0));
        let mut layers: [Option<Layer>; MAX_LAYERS] = Default::default();
        layers[0] = Some(Layer { clip: a, phase: 0.999, weight: 1.0 });
        let from = PosedFrom { layers: layers.clone(), rest_changes: 3 };
        assert!(pose_stands(Some(&from), &layers, 3));
        // Nothing posed yet (a new player, a pose made some other way).
        assert!(!pose_stands(None, &layers, 3));
        // A rest pose changed.
        assert!(!pose_stands(Some(&from), &layers, 4));
        let with = |f: &dyn Fn(&mut [Option<Layer>; MAX_LAYERS])| {
            let mut l = layers.clone();
            f(&mut l);
            pose_stands(Some(&from), &l, 3)
        };
        // The same clip in a new slot array stands.
        assert!(with(&|_| {}));
        // Another phase (the frame a clip reaches its end), another weight.
        assert!(!with(&|l| l[0].as_mut().unwrap().phase = 0.998));
        assert!(!with(&|l| l[0].as_mut().unwrap().weight = 0.5));
        // Another clip, even one with the same contents.
        assert!(!with(&|l| l[0].as_mut().unwrap().clip = b.clone()));
        // A weapon layer come or gone, or moved to another slot.
        assert!(!with(&|l| l[2] = Some(Layer { clip: b.clone(), phase: 0.0, weight: 1.0 })));
        assert!(!with(&|l| l[0] = None));
        assert!(!with(&|l| l.swap(0, 1)));
    }

    #[test]
    fn a_held_pose_is_sampled_only_when_something_changed() {
        let mut world = World::new();
        let bind = Transform::from_xyz(5.0, 0.0, 0.0);
        let node = world.spawn(bind).id();
        let posed = Transform::from_xyz(9.0, 0.0, 0.0);
        let clip_a = on_node_0(9.0);
        let at = |phase: f32| Some(Layer { clip: clip_a.clone(), phase, weight: 1.0 });
        let mut p = BlendPlayer::new(vec![node], Arc::new(vec![bind]));
        let mut scratch = Vec::new();
        let mut state = SystemState::<Query<&mut Transform>>::new(&mut world);
        macro_rules! held {
            ($p:expr) => {{
                world.clear_trackers();
                $p.pose_held(&mut scratch, &mut state.get_mut(&mut world).unwrap())
            }};
        }

        // Playing: a new phase is a new pose; the same phase (time stopped) stands.
        p.layers[0] = at(0.5);
        assert_eq!(held!(p), Some(1));
        assert_transform(&world, node, posed, true);
        assert_eq!(held!(p), None);
        assert_transform(&world, node, posed, false);
        // The frame the clip first reaches its end samples, the ones after do not.
        p.layers[0] = at(0.999);
        assert_eq!(held!(p), Some(1));
        assert_eq!(held!(p), None);
        assert_eq!(held!(p), None);
        assert_transform(&world, node, posed, false);

        // A weapon layer comes and goes.
        p.layers[2] = Some(Layer { clip: on_node_0(9.0), phase: 0.0, weight: 1.0 });
        assert_eq!(held!(p), Some(2));
        assert_eq!(held!(p), None);
        p.layers[2] = None;
        assert_eq!(held!(p), Some(1));
        assert_eq!(held!(p), None);

        // Another writer: the very next call puts the pose back, as `pose` would have,
        // and it stands again after that (its own write does not count).
        world.clear_trackers();
        *world.get_mut::<Transform>(node).unwrap() = bind;
        assert_eq!(held!(p), Some(1));
        assert_transform(&world, node, posed, true);
        assert_eq!(held!(p), None);
        assert_transform(&world, node, posed, false);
        // Written with what it already holds: a pose, which changes nothing.
        world.clear_trackers();
        *world.get_mut::<Transform>(node).unwrap() = posed;
        assert_eq!(held!(p), Some(1));
        assert_eq!(held!(p), None);

        // A pose made the ordinary way (the actor up again) is not trusted.
        world.clear_trackers();
        p.pose(&mut scratch, &mut state.get_mut(&mut world).unwrap());
        assert_eq!(held!(p), Some(1));
        assert_eq!(held!(p), None);
        // Nor after being told to forget (a ragdoll had the bones).
        p.forget_pose();
        assert_eq!(held!(p), Some(1));
        assert_eq!(held!(p), None);

        // Another clip: its pose at once.
        p.layers[0] = Some(Layer { clip: on_node_0(7.0), phase: 0.999, weight: 1.0 });
        assert_eq!(held!(p), Some(1));
        assert_transform(&world, node, Transform::from_xyz(7.0, 0.0, 0.0), true);

        // A new player on the same bones (the actor attached again) has made no pose.
        let mut fresh = BlendPlayer::new(vec![node], Arc::new(vec![bind]));
        fresh.layers = p.layers.clone();
        assert_eq!(held!(fresh), Some(1));
        assert_eq!(held!(fresh), None);
    }

    #[test]
    fn a_held_pose_takes_a_changed_rest_pose_at_once() {
        // Node 0 is keyed by the clip; node 1 no clip keys, so its rest pose is its pose.
        let mut world = World::new();
        let keyed = world.spawn(Transform::IDENTITY).id();
        let bind = Transform::from_xyz(5.0, 0.0, 0.0);
        let unkeyed = world.spawn(bind).id();
        let rest = Transform::from_xyz(1.0, 2.0, 3.0);
        let mut p = BlendPlayer::new(vec![keyed, unkeyed], Arc::new(vec![Transform::IDENTITY, bind]));
        p.layers[0] = Some(Layer { clip: on_node_0(9.0), phase: 0.999, weight: 1.0 });
        let mut scratch = Vec::new();
        let mut state = SystemState::<Query<&mut Transform>>::new(&mut world);
        macro_rules! held {
            () => {{
                world.clear_trackers();
                p.pose_held(&mut scratch, &mut state.get_mut(&mut world).unwrap())
            }};
        }
        assert_eq!(held!(), Some(1));
        assert_eq!(held!(), None);
        assert_transform(&world, unkeyed, bind, false);

        // Set again as it was: nothing.
        p.set_rest(1, None);
        assert_eq!(held!(), None);
        // Changed: on the bone at once, and it holds there.
        p.set_rest(1, Some(rest));
        assert_eq!(held!(), Some(1));
        assert_transform(&world, unkeyed, rest, true);
        p.set_rest(1, Some(rest));
        assert_eq!(held!(), None);
        assert_transform(&world, unkeyed, rest, false);
        // That bone is one the pose writes: another writer's change is put back.
        world.clear_trackers();
        *world.get_mut::<Transform>(unkeyed).unwrap() = bind;
        assert_eq!(held!(), Some(1));
        assert_transform(&world, unkeyed, rest, true);
        assert_eq!(held!(), None);
        // Taken away: back to its bind transform at once.
        p.set_rest(1, None);
        assert_eq!(held!(), Some(1));
        assert_transform(&world, unkeyed, bind, true);
        assert_eq!(held!(), None);
        assert_transform(&world, unkeyed, bind, false);
    }

    #[test]
    fn a_held_pose_leaves_a_node_the_clips_stopped_driving_alone() {
        // `pose` gives such a node its bind transform once and no longer writes it: the
        // check looks at what `pose` would write, so a later write to it stays in both.
        let mut world = World::new();
        let bind = Transform::from_xyz(5.0, 0.0, 0.0);
        let node = world.spawn(bind).id();
        let mut p = BlendPlayer::new(vec![node], Arc::new(vec![bind]));
        let mut scratch = Vec::new();
        let mut state = SystemState::<Query<&mut Transform>>::new(&mut world);
        p.layers[0] = Some(Layer { clip: on_node_0(9.0), phase: 0.5, weight: 1.0 });
        world.clear_trackers();
        assert_eq!(p.pose_held(&mut scratch, &mut state.get_mut(&mut world).unwrap()), Some(1));
        p.layers[0] = None;
        world.clear_trackers();
        assert_eq!(p.pose_held(&mut scratch, &mut state.get_mut(&mut world).unwrap()), Some(0));
        assert_eq!(*world.get::<Transform>(node).unwrap(), bind);
        world.clear_trackers();
        let moved = Transform::from_xyz(1.0, 2.0, 3.0);
        *world.get_mut::<Transform>(node).unwrap() = moved;
        world.clear_trackers();
        assert_eq!(p.pose_held(&mut scratch, &mut state.get_mut(&mut world).unwrap()), None);
        assert_eq!(*world.get::<Transform>(node).unwrap(), moved);
    }

    /// The split `locomotion::drive` uses to sample actors on the compute pool (the check
    /// first, the sampling into a list, the writes afterwards) poses as `pose_held` does.
    #[test]
    fn sampling_apart_from_writing_poses_as_pose_held_does() {
        let bind = Transform::from_xyz(5.0, 0.0, 0.0);
        let (mut wa, mut wb) = (World::new(), World::new());
        let (na, nb) = (wa.spawn(bind).id(), wb.spawn(bind).id());
        let (mut a, mut b) = (BlendPlayer::new(vec![na], Arc::new(vec![bind])), BlendPlayer::new(vec![nb], Arc::new(vec![bind])));
        let (mut sa, mut sb) = (SystemState::<Query<&mut Transform>>::new(&mut wa), SystemState::<Query<&mut Transform>>::new(&mut wb));
        let mut scratch = Vec::new();
        let clip = on_node_0(9.0);
        let steps: [(Option<f32>, Option<Transform>); 6] = [(Some(0.5), None), (Some(0.5), None), (Some(0.999), None), (Some(0.999), Some(bind)), (Some(0.999), None), (None, None)];
        for (phase, other_writer) in steps {
            for (p, w) in [(&mut a, &mut wa), (&mut b, &mut wb)] {
                p.layers[0] = phase.map(|phase| Layer { clip: clip.clone(), phase, weight: 1.0 });
                if let Some(t) = other_writer {
                    w.clear_trackers();
                    *w.query::<&mut Transform>().single_mut(w).unwrap() = t;
                }
                w.clear_trackers();
            }
            let held = a.pose_held(&mut scratch, &mut sa.get_mut(&mut wa).unwrap());
            let (written, mut out) = (b.written_since(&mut sb.get_mut(&mut wb).unwrap()), Vec::new());
            let apart = b.sample_held_into(written, &mut scratch, &mut out);
            let mut q = sb.get_mut(&mut wb).unwrap();
            for (e, tr) in out {
                q.get_mut(e).unwrap().set_if_neq(tr);
            }
            assert_eq!(held, apart, "{phase:?}");
            assert_eq!(*wa.get::<Transform>(na).unwrap(), *wb.get::<Transform>(nb).unwrap(), "{phase:?}");
        }
    }

    fn assert_transform(world: &World, node: Entity, expected: Transform, changed: bool) {
        let tr = world.entity(node).get_ref::<Transform>().unwrap();
        assert_eq!(*tr, expected);
        assert_eq!(tr.is_changed(), changed);
    }

    #[test]
    fn playing_samples_only_mark_different_transforms_changed() {
        // Both sequence and free-controller tracks preserve absent channels, and still
        // sample while paused/held. Repeated values must not dirty the node.
        for free in [false, true] {
            let mut world = World::new();
            let initial = Transform { translation: Vec3::splat(9.0), rotation: Quat::from_rotation_z(FRAC_PI_2), scale: Vec3::new(2.0, 3.0, 4.0) };
            let nodes: Vec<_> = (0..3).map(|_| world.spawn(initial).id()).collect();
            let full = Arc::new(TransformTrack {
                translation: Channel::Static(Some([1.0, 2.0, 3.0])),
                rotation: Channel::Static(Some([1.0, 0.0, 0.0, 0.0])),
                euler: None,
                scale: Channel::Static(Some([2.0])),
            });
            let tracks = vec![
                ("Full", full),
                ("Partial", track(Channel::Static(Some([4.0, 5.0, 6.0])), Channel::Static(None))),
                ("Absent", track(Channel::Static(None), Channel::Static(None))),
            ];
            let mut c = (*clip(1.0, tracks)).clone();
            if free {
                let cycle = OwnCycle { start: 0.0, stop: 1.0, frequency: 1.0, phase: 0.0, cycle_type: 0 };
                c.free = std::mem::take(&mut c.tracks).into_iter().map(|(n, t)| (n, t, cycle)).collect();
            }
            let mut p = Playing::new("", &c, |n| ["Full", "Partial", "Absent"].iter().position(|x| *x == n).map(|i| nodes[i]));
            let mut state = SystemState::<Query<&mut Transform>>::new(&mut world);
            let full_pose = Transform { translation: Vec3::new(1.0, 2.0, 3.0), rotation: Quat::IDENTITY, scale: Vec3::splat(2.0) };
            let partial_pose = Transform { translation: Vec3::new(4.0, 5.0, 6.0), ..initial };

            world.clear_trackers();
            p.step(0.25, false, &mut state.get_mut(&mut world).unwrap());
            assert_eq!(p.time, 0.25);
            assert_transform(&world, nodes[0], full_pose, true);
            assert_transform(&world, nodes[1], partial_pose, true);
            assert_transform(&world, nodes[2], initial, false);

            world.clear_trackers();
            p.held = true;
            p.step(0.25, p.held, &mut state.get_mut(&mut world).unwrap());
            assert_eq!(p.time, 0.25);
            assert_transform(&world, nodes[0], full_pose, false);
            assert_transform(&world, nodes[1], partial_pose, false);
            assert_transform(&world, nodes[2], initial, false);

            // A held clip still restores its pose if something else moved a node.
            *world.get_mut::<Transform>(nodes[0]).unwrap() = initial;
            world.clear_trackers();
            p.step(0.25, true, &mut state.get_mut(&mut world).unwrap());
            assert_transform(&world, nodes[0], full_pose, true);
        }
    }

    #[test]
    fn playing_compares_after_removing_root_motion() {
        let mut world = World::new();
        let initial = Transform { rotation: Quat::from_rotation_z(FRAC_PI_2), scale: Vec3::new(2.0, 3.0, 4.0), ..default() };
        let node = world.spawn(initial).id();
        let travel = Channel::Keys(Keys { kind: KeyKind::Linear, keys: vec![key3(0.0, [0.0; 3]), key3(1.0, [0.0, 8.0, 0.0])] });
        let c = clip(1.0, vec![("Bip01", track(travel, Channel::Static(None)))]);
        let mut p = Playing::new("", &c, |_| Some(node)).in_place(&c, |_| Some(node));
        assert_eq!(p.root_motion, Some((0, Vec3::new(0.0, 8.0, 0.0))));
        let mut state = SystemState::<Query<&mut Transform>>::new(&mut world);
        world.clear_trackers();
        p.step(0.5, false, &mut state.get_mut(&mut world).unwrap());
        assert_transform(&world, node, initial, false);
        world.clear_trackers();
        p.step(0.0, true, &mut state.get_mut(&mut world).unwrap());
        assert_transform(&world, node, initial, false);

        world.get_mut::<Transform>(node).unwrap().translation = Vec3::X;
        world.clear_trackers();
        p.step(0.0, true, &mut state.get_mut(&mut world).unwrap());
        assert_transform(&world, node, initial, true);
    }

    #[test]
    fn blended_poses_and_released_nodes_only_mark_different_transforms_changed() {
        let mut world = World::new();
        let bind = Transform::from_xyz(5.0, 0.0, 0.0);
        let node = world.spawn(bind).id();
        let c = clip(1.0, vec![("Other", pose([9.0, 0.0, 0.0]))]);
        let bound = Arc::new(BoundClip::bind(c, |n| (n == "Other").then_some(0)));
        let layer = Some(Layer { clip: bound, phase: 0.0, weight: 1.0 });
        let mut p = BlendPlayer::new(vec![node], Arc::new(vec![bind]));
        p.layers[0] = layer.clone();
        let mut scratch = Vec::new();
        let mut state = SystemState::<Query<&mut Transform>>::new(&mut world);
        world.clear_trackers();
        assert_eq!(p.pose(&mut scratch, &mut state.get_mut(&mut world).unwrap()), 1);
        assert_transform(&world, node, Transform::from_xyz(9.0, 0.0, 0.0), true);
        world.clear_trackers();
        assert_eq!(p.pose(&mut scratch, &mut state.get_mut(&mut world).unwrap()), 1);
        assert_transform(&world, node, Transform::from_xyz(9.0, 0.0, 0.0), false);

        p.layers[0] = None;
        world.clear_trackers();
        assert_eq!(p.pose(&mut scratch, &mut state.get_mut(&mut world).unwrap()), 0);
        assert_eq!(scratch, [(0, bind)]);
        assert_transform(&world, node, bind, true);

        // Release a node that already has its bind pose: still emitted, never dirtied.
        p.layers[0] = layer;
        p.pose(&mut scratch, &mut state.get_mut(&mut world).unwrap());
        *world.get_mut::<Transform>(node).unwrap() = bind;
        p.layers[0] = None;
        world.clear_trackers();
        assert_eq!(p.pose(&mut scratch, &mut state.get_mut(&mut world).unwrap()), 0);
        assert_eq!(scratch, [(0, bind)]);
        assert_transform(&world, node, bind, false);
    }

    #[test]
    fn queued_root_resets_only_mark_different_transforms_changed() {
        let mut world = World::new();
        let node = world.spawn(Transform::IDENTITY).id();
        let bind = Transform::from_xyz(0.0, 0.0, 67.0);
        world.insert_resource(Time::<()>::default());
        world.insert_resource(AnimState { held: Some((node, bind)), ..default() });
        world.resource_mut::<AnimState>().stop();
        world.clear_trackers();
        world.run_system_once(advance).unwrap();
        assert_transform(&world, node, bind, true);
        assert!(world.resource::<AnimState>().pending.is_empty());

        world.resource_mut::<AnimState>().held = Some((node, bind));
        world.resource_mut::<AnimState>().stop();
        world.clear_trackers();
        world.run_system_once(advance).unwrap();
        assert_transform(&world, node, bind, false);
        assert!(world.resource::<AnimState>().pending.is_empty());
    }

    fn walk_and_idle(ix: &impl Fn(&str) -> Option<usize>) -> (Arc<BoundClip>, Arc<BoundClip>) {
        let travel = Keys {
            kind: KeyKind::Linear,
            keys: [(0.0, [0.0, 0.0, 0.0]), (0.5, [0.0, 52.0, 0.0]), (1.0, [0.0, 100.0, 0.0])]
                .map(|(time, value)| Key { time, value, forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] })
                .to_vec(),
        };
        let walk = clip(
            1.0,
            vec![
                ("Bip01", track(Channel::Keys(travel), Channel::Static(Some([1.0, 0.0, 0.0, 0.0])))),
                ("Bip01 NonAccum", track(Channel::Static(Some([0.0, 0.0, 65.0])), Channel::Static(Some([1.0, 0.0, 0.0, 0.0])))),
                ("Other", track(Channel::Static(Some([9.0, 0.0, 0.0])), Channel::Static(None))),
            ],
        );
        let (s, c) = (FRAC_PI_4.sin(), FRAC_PI_4.cos());
        let idle = clip(4.0, vec![("Bip01 NonAccum", track(Channel::Static(Some([0.0, 0.0, 61.0])), Channel::Static(Some([c, 0.0, 0.0, s])))), ("Missing", track(Channel::Static(None), Channel::Static(None)))]);
        (Arc::new(BoundClip::bind(walk, ix)), Arc::new(BoundClip::bind(idle, ix)))
    }

    #[test]
    fn bound_clip_root_motion() {
        let (_, ix) = player();
        let (walk, idle) = walk_and_idle(&ix);
        assert_eq!(walk.tracks.len(), 3);
        assert_eq!((walk.root, walk.root_track), (Some(1), Some((0, Vec3::new(0.0, 100.0, 0.0), 0.0))));
        // A track whose node the skeleton lacks is dropped; the idle drives no root.
        assert_eq!((idle.tracks.len(), idle.root, idle.root_track), (1, Some(1), None));
        assert_eq!(walk.time_at(0.25), 0.25);
    }

    #[test]
    fn a_carried_clip_leaves_its_travel_out_of_the_pose() {
        // A lunge: Bip01 160 forward at 0.5 s, back to 120 at its end (actors.md §8.13).
        let (mut player, ix) = player();
        let lunge = Keys {
            kind: KeyKind::Linear,
            keys: [(0.0, [0.0, 0.0, 0.0]), (0.5, [0.0, 160.0, 0.0]), (1.0, [0.0, 120.0, 0.0])].map(|(time, value)| Key { time, value, forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] }).to_vec(),
        };
        let c = clip(1.0, vec![("Bip01", track(Channel::Keys(lunge), Channel::Static(Some([1.0, 0.0, 0.0, 0.0]))))]);
        let plain = Arc::new(BoundClip::bind(c.clone(), &ix));
        let carried = Arc::new(BoundClip::bind(c, &ix).carried());
        assert_eq!((plain.carried, carried.carried), (None, Some(Vec2::ZERO)));
        let root_y = |player: &mut BlendPlayer, clip: &Arc<BoundClip>| {
            player.layers[0] = Some(Layer { clip: clip.clone(), phase: 0.5, weight: 1.0 });
            let mut out = Vec::new();
            player.sample(&mut out);
            out.iter().find(|(i, _)| *i == 1).unwrap().1.translation.y
        };
        // §8.2's rule leaves 160 less half of 120 in the pose; carried, none of it.
        assert!((root_y(&mut player, &plain) - 100.0).abs() < 1e-3);
        assert!(root_y(&mut player, &carried).abs() < 1e-3);
        // The travel goes to the actor instead, and ends where the clip ends.
        assert!((carried.travel(0.0, 0.5) - Vec2::new(0.0, 160.0)).length() < 1e-3);
        assert!((carried.travel(0.5, 2.0) - Vec2::new(0.0, -40.0)).length() < 1e-3);
        assert_eq!(plain.travel(0.0, 0.5), Vec2::ZERO);
        // A clip whose root stays within CARRY_MIN does not carry (a bite's step).
        let step = Keys {
            kind: KeyKind::Linear,
            keys: [(0.0, [0.0, 0.0, 0.0]), (1.0, [0.0, 55.0, 0.0])].map(|(time, value)| Key { time, value, forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] }).to_vec(),
        };
        let bite = clip(1.0, vec![("Bip01", track(Channel::Keys(step), Channel::Static(Some([1.0, 0.0, 0.0, 0.0]))))]);
        assert_eq!(BoundClip::bind(bite, &ix).carried().carried, None);
    }

    #[test]
    fn single_clip_player_plays_walks_in_place() {
        // nvf-viewer's player: the walk clip's Bip01 travel is removed, its sway kept.
        let (_, ix) = player();
        let (walk, _) = walk_and_idle(&ix);
        let mut app = App::new();
        let root = app.world_mut().spawn(Transform::IDENTITY).id();
        let other = app.world_mut().spawn(Transform::IDENTITY).id();
        let node = |n: &str| match n {
            "Bip01" => Some(root),
            "Other" => Some(other),
            _ => None,
        };
        let playing = Playing::new("walk", &walk.clip, node).in_place(&walk.clip, node).at_phase(0.5);
        assert_eq!(playing.root_motion, Some((0, Vec3::new(0.0, 100.0, 0.0))));
        app.world_mut().spawn(playing);
        app.add_systems(Update, |mut q: Query<&mut Playing>, mut t: Query<&mut Transform>| {
            for mut p in &mut q {
                p.step(0.0, true, &mut t);
            }
        });
        app.update();
        // Half way: 52 units travelled, 50 removed.
        let y = app.world().get::<Transform>(root).unwrap().translation.y;
        assert!((y - 2.0).abs() < 1e-4, "{y}");
        assert_eq!(app.world().get::<Transform>(other).unwrap().translation.x, 9.0);
        // Without in_place the travel stays (world objects).
        let p = Playing::new("walk", &walk.clip, node);
        assert_eq!(p.root_motion, None);
    }

    #[test]
    fn one_clip_plays_in_place() {
        let (mut p, ix) = player();
        let (walk, idle) = walk_and_idle(&ix);
        let mut out = Vec::new();
        p.layers[0] = Some(Layer { clip: walk, phase: 0.5, weight: 1.0 });
        assert_eq!(p.sample(&mut out), 3);
        let get = |out: &Vec<(usize, Transform)>, i: usize| out.iter().find(|(j, _)| *j == i).unwrap().1;
        // The travel is removed: 52 at half the cycle less half of 100.
        assert!((get(&out, 1).translation - Vec3::new(0.0, 2.0, 0.0)).length() < 1e-4);
        assert_eq!(get(&out, 2).translation, Vec3::new(0.0, 0.0, 65.0));
        // A channel with no value keeps the bind transform's.
        assert_eq!(get(&out, 3).translation, Vec3::new(9.0, 0.0, 0.0));
        assert_eq!(get(&out, 3).rotation, Quat::IDENTITY);
        // An idle without a root track holds the root at the identity.
        p.layers[0] = Some(Layer { clip: idle, phase: 0.0, weight: 1.0 });
        p.sample(&mut out);
        assert_eq!(get(&out, 1), Transform::IDENTITY);
        // `Other`, which only the walk drove, is back at its bind transform.
        assert_eq!(out.len(), 3);
        assert_eq!(get(&out, 3), Transform::from_xyz(5.0, 0.0, 0.0));
    }

    #[test]
    fn two_clips_blend_by_weight() {
        let (mut p, ix) = player();
        let (walk, idle) = walk_and_idle(&ix);
        let mut out = Vec::new();
        p.layers = [Some(Layer { clip: idle, phase: 0.0, weight: 0.75 }), Some(Layer { clip: walk, phase: 0.0, weight: 0.25 }), None, None, None, None, None, None];
        p.sample(&mut out);
        let get = |i: usize| out.iter().find(|(j, _)| *j == i).unwrap().1;
        assert_eq!(get(1), Transform::IDENTITY);
        let na = get(2);
        assert!((na.translation.z - (0.75 * 61.0 + 0.25 * 65.0)).abs() < 1e-4);
        // 90° in the idle, 0° in the walk: three quarters of the way, 67.5° (nlerp is
        // close to slerp here).
        let (axis, angle) = na.rotation.to_axis_angle();
        assert!(axis.z > 0.99 && (angle.to_degrees() - 67.5).abs() < 2.0, "{}", angle.to_degrees());
        // Only the walk drives `Other`: a quarter of its pose, the rest the bind pose.
        assert!((get(3).translation.x - (0.75 * 5.0 + 0.25 * 9.0)).abs() < 1e-4);
        // Weights are not kept between frames.
        p.layers[1] = None;
        p.layers[0].as_mut().unwrap().weight = 1.0;
        p.sample(&mut out);
        assert_eq!(out.len(), 3);
        assert!((out.iter().find(|(j, _)| *j == 2).unwrap().1.translation.z - 61.0).abs() < 1e-5);
        assert_eq!(out.iter().find(|(j, _)| *j == 3).unwrap().1, Transform::from_xyz(5.0, 0.0, 0.0));
    }

    fn pose(t: [f32; 3]) -> Arc<TransformTrack> {
        track(Channel::Static(Some(t)), Channel::Static(Some([1.0, 0.0, 0.0, 0.0])))
    }

    #[test]
    fn root_height_counts_from_the_track_start() {
        // A holster clip holding `Bip01` 67 up with no `Bip01 NonAccum` track (2ha/2hh
        // `Holster`, floating-actors.md) over an idle with the pelvis at 61: the body stays
        // at the idle's height.
        let (mut p, ix) = player();
        let holster = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Bip01", pose([0.0, 0.0, 67.0]), 40)]), &ix));
        let idle = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Bip01 NonAccum", pose([0.0, 0.0, 61.0]), 10)]), &ix));
        assert_eq!(holster.root_track, Some((0, Vec3::ZERO, 67.0)));
        let layer = |c: &Arc<BoundClip>| Some(Layer { clip: c.clone(), phase: 0.5, weight: 1.0 });
        p.layers = [layer(&idle), layer(&holster), None, None, None, None, None, None];
        let mut out = Vec::new();
        p.sample(&mut out);
        let get = |i: usize| out.iter().find(|(j, _)| *j == i).unwrap().1.translation;
        assert_eq!(get(1), Vec3::ZERO);
        assert_eq!(get(2).z, 61.0);
    }

    #[test]
    fn broken_layers_and_overflowing_keys_give_finite_bones() {
        let (mut p, ix) = player();
        // Keys finite each (Clip::check passes them) but far enough apart to overflow
        // between them, a root whose travel overflows, NaN keys.
        let lerp = |a: f32, b: f32| Channel::Keys(Keys { kind: KeyKind::Linear, keys: vec![key3(0.0, [a; 3]), key3(1.0, [b; 3])] });
        let wild = prio_clip(1.0, vec![("Bip01", track(lerp(-3.0e38, 3.0e38), Channel::Static(Some([1.0e30, 1.0e30, 0.0, 0.0]))), 30), ("Other", track(lerp(f32::NAN, 0.0), Channel::Static(None)), 30)]);
        assert_eq!(wild.check().map_err(|_| ()), Err(()));
        let mut finite = (*wild).clone();
        finite.tracks.truncate(1);
        assert_eq!(finite.check(), Ok(()));
        // Two clips at one priority whose finite poses overflow their mix.
        let far = |x: f32| Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Other", pose([x, 0.0, 0.0]), 30)]), &ix));
        let (a, b) = (far(3.0e38), far(-3.0e38));
        let bound = Arc::new(BoundClip::bind(Arc::new(finite), &ix));
        let broken_wild = Arc::new(BoundClip::bind(wild, &ix));
        let mut out = Vec::new();
        for (phase, weight) in [(f32::NAN, 1.0), (f32::INFINITY, 1.0), (0.5, f32::INFINITY), (0.5, f32::NAN), (0.3, 1.0)] {
            let layer = |c: &Arc<BoundClip>| Some(Layer { clip: c.clone(), phase, weight });
            p.layers = [layer(&bound), layer(&broken_wild), layer(&a), layer(&b), None, None, None, None];
            p.sample(&mut out);
            for (i, t) in &out {
                assert!(t.translation.is_finite() && t.rotation.is_finite() && t.scale.is_finite(), "node {i} at phase {phase}, weight {weight}: {t:?}");
                assert!((t.rotation.length() - 1.0).abs() < 1e-3, "{t:?}");
            }
        }
        // A clip whose length overflows cannot be played.
        assert!(prio_clip(1.0, Vec::new()).as_ref().clone().check().is_ok());
        let long = Clip { start: -3.0e38, stop: 3.0e38, ..prio_clip(1.0, Vec::new()).as_ref().clone() };
        assert!(long.check().is_err());
    }

    #[test]
    fn priority_decides_per_node() {
        let (mut p, ix) = player();
        // Legs (`Other`) at 30 in a walk, 26 in an equip clip; the arm (`Bip01 NonAccum`
        // here) at 55 in the equip clip only.
        let walk = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Other", pose([1.0, 0.0, 0.0]), 30)]), &ix));
        let equip = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Other", pose([2.0, 0.0, 0.0]), 26), ("Bip01 NonAccum", pose([0.0, 0.0, 7.0]), 55)]), &ix));
        let idle = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Other", pose([3.0, 0.0, 0.0]), 10), ("Bip01 NonAccum", pose([0.0, 0.0, 1.0]), 10)]), &ix));
        let mut out = Vec::new();
        let get = |out: &Vec<(usize, Transform)>, i: usize| out.iter().find(|(j, _)| *j == i).unwrap().1.translation;
        let layer = |c: &Arc<BoundClip>, weight: f32| Some(Layer { clip: c.clone(), phase: 0.0, weight });
        // Walking legs (30) beat the equip clip's (26); its upper body (55) wins.
        p.layers = [layer(&walk, 1.0), layer(&equip, 1.0), None, None, None, None, None, None];
        p.sample(&mut out);
        assert_eq!(get(&out, 3).x, 1.0);
        assert_eq!(get(&out, 2).z, 7.0);
        // Standing, the equip clip's legs (26) beat the idle's (10).
        p.layers = [layer(&idle, 1.0), layer(&equip, 1.0), None, None, None, None, None, None];
        p.sample(&mut out);
        assert_eq!(get(&out, 3).x, 2.0);
        // A higher priority fading in (0.25) leaves the rest to the lower one.
        p.layers = [layer(&idle, 1.0), layer(&equip, 0.25), None, None, None, None, None, None];
        p.sample(&mut out);
        assert!((get(&out, 2).z - (0.25 * 7.0 + 0.75 * 1.0)).abs() < 1e-5);
        // Equal priorities share by weight even when they add up to more than 1.
        let walk2 = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Other", pose([5.0, 0.0, 0.0]), 30)]), &ix));
        p.layers = [layer(&walk, 1.0), layer(&walk2, 1.0), None, None, None, None, None, None];
        p.sample(&mut out);
        assert!((get(&out, 3).x - 3.0).abs() < 1e-5);
        // A node no clip drives any more goes back to its bind transform (5, 0, 0).
        p.layers = [layer(&walk, 1.0), None, None, None, None, None, None, None];
        p.sample(&mut out);
        p.layers = [layer(&equip, 1.0), None, None, None, None, None, None, None];
        p.sample(&mut out);
        p.layers = [layer(&walk, 1.0), None, None, None, None, None, None, None];
        p.sample(&mut out);
        assert_eq!(get(&out, 2), Vec3::ZERO);
    }

    #[test]
    fn a_rest_pose_stands_in_for_the_bind_transform() {
        let (mut p, ix) = player();
        let other = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Other", pose([1.0, 0.0, 0.0]), 30)]), &ix));
        let mut out = Vec::new();
        let get = |out: &Vec<(usize, Transform)>, i: usize| out.iter().find(|(j, _)| *j == i).map(|o| o.1.translation);
        p.set_rest(3, Some(Transform::from_xyz(7.0, 0.0, 0.0)));
        // No clip drives the node: it takes the rest pose.
        p.sample(&mut out);
        assert_eq!(get(&out, 3), Some(Vec3::new(7.0, 0.0, 0.0)));
        // A clip on it at weight 0.5: the rest pose fills the other half.
        p.layers = [Some(Layer { clip: other, phase: 0.0, weight: 0.5 }), None, None, None, None, None, None, None];
        p.sample(&mut out);
        assert!((get(&out, 3).unwrap().x - 4.0).abs() < 1e-5);
        // Without it, the node goes back to its bind transform (5, 0, 0).
        p.layers = Default::default();
        p.set_rest(3, None);
        p.sample(&mut out);
        assert_eq!(get(&out, 3), Some(Vec3::new(5.0, 0.0, 0.0)));
    }

    #[test]
    fn cycles() {
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 0), 0.5);
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 1), 1.5);
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 2), 2.0);
        assert_eq!(cycle_time(1.0, 1.0, 1.0, 0), 1.0);
    }
}
