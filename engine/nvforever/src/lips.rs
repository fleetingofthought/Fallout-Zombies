//! Faces in the game (dialogue step D6, docs/spec-notes/lipsync.md): while an NPC says a
//! voiced response its face moves from the response's `.lip` file in time with the
//! sound, shows the response's emotion and nods its head; between lines a face near the
//! camera blinks and its eyes move.
//!
//! - **What moves.** The `.lip` gives a weight per channel, 30 times a second
//!   (`fnv_voice::lip`). A channel's name is the label of a morph in the FaceGen `.tri`
//!   file beside a head part's model (`fnv_actor::tri`): the head, the mouth, the lower
//!   teeth, the tongue, the eyes, beards and eyebrows each carry the morphs that concern
//!   them. A vertex is drawn at: the model's vertex, shaped by the actor's FaceGen `.egm`
//!   (facegen.rs), plus Σ weight × the morph's displacement [hypothesis: the morphs add
//!   up linearly on top of the face shape; lipsync.md §4.3].
//! - **Which channels.** The sixteen mouth shapes, the blinks, the brows, the squints and
//!   the eyes' Look channels ([`DRIVEN_MODIFIERS`]) move morphs; the three head channels
//!   turn the head bone ([`head_quat`], lipsync.md §5.1; their unit is a hypothesis,
//!   [`HEAD_RADIANS_PER_UNIT`]). The response's emotion adds the head's expression morph
//!   of its name ([`EXPRESSIONS`], lipsync.md §5.2) [engine choice].
//! - **When.** `nvf_play::voice` says which response each speaker is saying and when its
//!   sound started (also with `--no-voice`); the frame for the moment `t` after that is
//!   `t × 30 − start frame`, the two frames around it mixed. The frames a file has
//!   before the sound starts are not shown [hypothesis]. When the line ends or is cut
//!   the face eases back to rest over [`EASE_SECONDS`] [engine choice].
//! - **Between lines** ([`IdleFace`], lipsync.md §5.3, a design): drawn living NPCs
//!   within [`IDLE_RADIUS`] of the camera blink, their eyes make up what head tracking's
//!   turn stops short of, and they glance a little.
//! - **How.** An actor with a moving face ([`Face`]) has each of its moving shapes drawn
//!   from a mesh of its own whose positions are rewritten when the weights change
//!   (normals are kept as they are); it gets its own meshes back when it is let go.
//!   Speakers without a rig — creatures, robots, talking activators, the unseen — are
//!   skipped, with a log line.
//!
//! - **The console** (docs/design/console.md): `temo` (ToggleEmotions) leaves out every
//!   expression and mood, a line's emotion and those asked through [`FaceMorphs`]
//!   ([`LipState::emotions_off`]); `MFG` sets morphs on a face through [`FaceMorphs`]
//!   ([`mfg`]), its head modifiers turning the head bone as a `.lip`'s head channels do.
//!
//! One log line per response: `lip: animating <path> ...` or `lip: skipped <path>: <why>`;
//! a `lip summary:` and a `face idle summary:` at the end. `--no-lip` turns it all off,
//! `--no-face-idle` the faces between lines only.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use fnv_actor::records::FaceGen;
use fnv_actor::tri::{label_aliases, Tri};
use fnv_actor::Appearance;
use fnv_sim::FormId;
use fnv_voice::lip::{channel, Lip, CHANNELS, CHANNEL_NAMES, PHONEMES};
use nvf_assets::actor::{PartUse, SpawnedActor};
use nvf_assets::GameData;
use nvf_play::voice::VoiceState;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::f32::consts::PI;
use std::sync::Arc;

use crate::actors::ActorWorld;
use crate::face_morphs::FaceMorphs;
use crate::facegen::sibling;
use crate::head_look::{HeadLook, NoHeadLook};
use crate::limbs::ActorRig;
use crate::locomotion::Dying;
use crate::ragdoll::Corpse;
use crate::reflection::ReflectionCamera;

/// The `.lip` modifier channels applied to morphs (with every mouth shape).
pub const DRIVEN_MODIFIERS: [&str; 14] = [
    "BlinkLeft",
    "BlinkRight",
    "BrowDownLeft",
    "BrowDownRight",
    "BrowInLeft",
    "BrowInRight",
    "BrowUpLeft",
    "BrowUpRight",
    "LookDown",
    "LookLeft",
    "LookRight",
    "LookUp",
    "SquintLeft",
    "SquintRight",
];

/// The `.lip` channels that turn the head bone (lipsync.md §3.2, §5.1).
pub const HEAD_PITCH: usize = 30;
pub const HEAD_ROLL: usize = 31;
pub const HEAD_YAW: usize = 32;

/// Radians the head turns per unit of a head channel: reading (a) of lipsync.md §3.3.
/// `mfg` HeadPitch 100 tipped Doc Mitchell's head about 60° and 50 about 30° [observed,
/// owner's screenshots 2026-10-22]; that 100 on the console is 1 in a `.lip` is a
/// hypothesis.
pub const HEAD_RADIANS_PER_UNIT: f32 = 1.0;

/// The expression morphs a response's emotion shows, by emotion type code 1 to 7
/// (TRDT: 1 Anger, 2 Disgust, 3 Fear, 4 Sad, 5 Happy, 6 Surprise, 7 Pained; 0 Neutral
/// shows nothing) [hypothesis: by name, lipsync.md §4.4].
pub const EXPRESSIONS: [&str; 7] = ["Anger", "Disgust", "Fear", "Sad", "Happy", "Surprise", "Pained"];

/// The mood morphs of the heads (lipsync.md §4.2), which other systems may ask for
/// through [`FaceMorphs`] (the combat face sets `CombatAnger`).
pub const MOODS: [&str; 9] = ["MoodNeutral", "MoodAfraid", "MoodAnnoyed", "Cocky", "MoodDrugged", "MoodPleasant", "MoodAngry", "MoodSad", "CombatAnger"];

/// Morph slots: the 33 `.lip` channels, then the expressions, then the moods.
pub const SLOTS: usize = CHANNELS + EXPRESSIONS.len() + MOODS.len();

/// A weight per slot.
pub type Weights = [f32; SLOTS];

/// An emotion's weight per unit of its value (0 to 100): value ÷ 100 [hypothesis] × 0.6
/// [engine choice: an expression at full weight distorts the mouth the phonemes move].
pub const EMOTION_SCALE: f32 = 0.6 / 100.0;

/// Seconds an emotion takes to come in at the start of a response [engine choice].
pub const EMOTION_IN_SECONDS: f64 = 0.3;

/// Whether channel `c` of a `.lip` is applied to the morphs.
pub fn driven(c: usize) -> bool {
    c < PHONEMES || CHANNEL_NAMES.get(c).is_some_and(|n| DRIVEN_MODIFIERS.contains(n))
}

/// Whether slot `s` moves morphs (a driven channel or an expression).
pub fn morph_slot(s: usize) -> bool {
    (CHANNELS..SLOTS).contains(&s) || driven(s)
}

/// The morph label of slot `s`.
pub fn slot_name(s: usize) -> &'static str {
    if s < CHANNELS {
        CHANNEL_NAMES[s]
    } else if s < CHANNELS + EXPRESSIONS.len() {
        EXPRESSIONS[s - CHANNELS]
    } else {
        MOODS[s - CHANNELS - EXPRESSIONS.len()]
    }
}

/// The slot of an emotion type code (`None` for Neutral and unknown codes).
pub fn emotion_slot(code: u8) -> Option<usize> {
    (1..=EXPRESSIONS.len() as u8).contains(&code).then(|| CHANNELS + code as usize - 1)
}

/// The slot of a channel or expression name (case does not matter).
pub fn slot(name: &str) -> Option<usize> {
    channel(name)
        .or_else(|| EXPRESSIONS.iter().position(|e| e.eq_ignore_ascii_case(name)).map(|i| CHANNELS + i))
        .or_else(|| MOODS.iter().position(|e| e.eq_ignore_ascii_case(name)).map(|i| CHANNELS + EXPRESSIONS.len() + i))
}

/// Seconds over which a face returns to rest when its line ends or is cut.
pub const EASE_SECONDS: f64 = 0.15;

/// A response let go this long before its sound's end counts as cut (for the log).
pub const CUT_SECONDS: f64 = 0.25;

/// A weight below this moves nothing worth a mesh update.
const SMALL: f32 = 1e-4;

/// A face is written again only when a weight changed by more than this since the last
/// write [engine choice: invisible, and it keeps idle eyes that follow a slowly turning
/// head from rewriting the mesh every frame].
pub const CHANGE: f32 = 0.004;

// ---- the face between lines [design, lipsync.md §5.3] ---------------------------------

/// Idle faces are given to actors within this many units of the camera (about 10 m)
/// [design], and let go beyond [`IDLE_RELEASE`].
pub const IDLE_RADIUS: f32 = 700.0;
pub const IDLE_RELEASE: f32 = 840.0;
/// A blink: up in this many seconds, then down in [`BLINK_DOWN`] (0.2 s in all, the
/// length of the `.lip` files' blinks, lipsync.md §3.1).
pub const BLINK_UP: f64 = 0.07;
pub const BLINK_DOWN: f64 = 0.13;
/// Seconds between blinks: drawn evenly between these (mean 3.25 s; the `.lip` files'
/// blinks come about every 3.2 s) [design].
pub const BLINK_EVERY: (f64, f64) = (1.5, 5.0);
/// How far an eye turns per unit of a Look weight, radians [observed: the eye `.tri`
/// files' Look morphs, lipsync.md §3.3, rounded].
pub const EYE_SIDE: f32 = 22.0 * PI / 180.0;
pub const EYE_UP: f32 = 14.0 * PI / 180.0;
pub const EYE_DOWN: f32 = 22.0 * PI / 180.0;
/// The largest Look weight the idle eyes use [design: about 13° to the side].
pub const EYE_LIMIT: f32 = 0.6;
/// Glances: a new one every this many seconds, within these Look weights, reached in
/// [`GLANCE_MOVE`] seconds; a third of them go back to the middle [design].
pub const GLANCE_EVERY: (f64, f64) = (0.6, 2.5);
pub const GLANCE_SIDE: f32 = 0.06;
pub const GLANCE_UPDOWN: f32 = 0.04;
pub const GLANCE_MOVE: f64 = 0.05;

/// The lids' weight `t` seconds into a blink.
pub fn blink_weight(t: f64) -> f32 {
    if !(0.0..BLINK_UP + BLINK_DOWN).contains(&t) {
        0.0
    } else if t < BLINK_UP {
        (t / BLINK_UP) as f32
    } else {
        (1.0 - (t - BLINK_UP) / BLINK_DOWN) as f32
    }
}

/// Where the eyes look, as Look weights: `h` positive to the actor's right, `v` positive
/// up. Written into the four Look slots.
pub fn set_look(w: &mut Weights, h: f32, v: f32) {
    let (l, r, u, d) = (channel("LookLeft"), channel("LookRight"), channel("LookUp"), channel("LookDown"));
    for (c, x) in [(l, (-h).max(0.0)), (r, h.max(0.0)), (u, v.max(0.0)), (d, (-v).max(0.0))] {
        if let Some(c) = c {
            w[c] += x;
        }
    }
}

/// The Look weights that turn the eyes onto a target the head faces `off` away from
/// (`HeadLook::off_target`: yaw positive = the head is to the right of it, pitch
/// positive = above it, radians), each within [`EYE_LIMIT`].
pub fn eyes_onto(off: (f32, f32)) -> (f32, f32) {
    let h = -off.0 / EYE_SIDE;
    let v = if off.1 < 0.0 { -off.1 / EYE_UP } else { -off.1 / EYE_DOWN };
    (h.clamp(-EYE_LIMIT, EYE_LIMIT), v.clamp(-EYE_LIMIT, EYE_LIMIT))
}

/// A small generator of each face's own (splitmix64), so the draws repeat from run to
/// run and shift nobody else's.
#[derive(Clone, Copy, Debug)]
pub struct FaceRng(u64);

impl FaceRng {
    pub fn new(seed: u32) -> FaceRng {
        FaceRng(u64::from(seed) ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// A number in [0, 1).
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as f64 / (u64::MAX as f64 + 1.0)
    }

    pub fn between(&mut self, (lo, hi): (f64, f64)) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

/// Blinks and glances of one face, on the game clock.
#[derive(Clone, Debug)]
pub struct IdleFace {
    rng: FaceRng,
    next_blink: f64,
    blink_from: Option<f64>,
    glance_from: (f32, f32),
    glance_to: (f32, f32),
    glance_at: f64,
    next_glance: f64,
    pub blinks: u32,
    pub glances: u32,
}

impl IdleFace {
    pub fn new(seed: u32, now: f64) -> IdleFace {
        let mut rng = FaceRng::new(seed);
        // The first blink anywhere in a spacing, so faces that appear together do not
        // blink together.
        let next_blink = now + rng.between((0.0, BLINK_EVERY.1));
        let next_glance = now + rng.between(GLANCE_EVERY);
        IdleFace { rng, next_blink, blink_from: None, glance_from: (0.0, 0.0), glance_to: (0.0, 0.0), glance_at: now, next_glance, blinks: 0, glances: 0 }
    }

    /// The lids' weight and the glance (Look weights, `h` right, `v` up) at `now`.
    pub fn step(&mut self, now: f64) -> (f32, (f32, f32)) {
        if self.blink_from.is_none() && now >= self.next_blink {
            self.blink_from = Some(now);
            self.blinks += 1;
            self.next_blink = now + self.rng.between(BLINK_EVERY);
        }
        let lids = match self.blink_from {
            Some(t0) if now - t0 < BLINK_UP + BLINK_DOWN => blink_weight(now - t0),
            _ => {
                self.blink_from = None;
                0.0
            }
        };
        if now >= self.next_glance {
            self.glance_from = self.glance(now);
            self.glance_to = if self.rng.next() < 1.0 / 3.0 {
                (0.0, 0.0)
            } else {
                let h = (self.rng.next() * 2.0 - 1.0) as f32 * GLANCE_SIDE;
                let v = (self.rng.next() * 2.0 - 1.0) as f32 * GLANCE_UPDOWN;
                (h, v)
            };
            self.glance_at = now;
            self.glances += 1;
            self.next_glance = now + self.rng.between(GLANCE_EVERY);
        }
        (lids, self.glance(now))
    }

    fn glance(&self, now: f64) -> (f32, f32) {
        let k = ((now - self.glance_at) / GLANCE_MOVE).clamp(0.0, 1.0) as f32;
        let (a, b) = (self.glance_from, self.glance_to);
        (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k)
    }
}

// ---- the rig --------------------------------------------------------------------------

/// A shape of an actor that a `.tri` can move.
#[derive(Clone)]
pub struct LipShape {
    pub entity: Entity,
    /// The part's model (archive path); the `.tri` and `.egm` sit beside it.
    pub model: String,
    /// The model's mesh, unshaped.
    pub cpu: Arc<Mesh>,
    /// The mesh the shape is drawn with when the actor is silent.
    pub rest: Handle<Mesh>,
}

/// On an actor's root: its shapes that have a `.tri` beside their model, and the
/// FaceGen coefficients its face was shaped with (`None`: the race's base face).
#[derive(Component, Clone)]
pub struct LipRig {
    pub shapes: Vec<LipShape>,
    pub facegen: Option<FaceGen>,
}

/// The rig of a freshly spawned actor (`actors::spawn`): `uses[i]` is part
/// `a.parts[part_index[i]]`, and `spawned` names the part and shape of each shape
/// `spawn_actor` drew (`part_of`, `shape_of`; caps are drawn too, with no triangles).
/// `None` when no part has a `.tri`. `facegen` is what shaped the face
/// (`Appearance::shape_facegen`; `None` with FaceGen off).
pub fn rig(a: &Appearance, part_index: &[usize], uses: &[PartUse], spawned: &SpawnedActor, data: &GameData, facegen: Option<FaceGen>) -> Option<LipRig> {
    let mut shapes = Vec::new();
    let mut has_tri: Vec<Option<bool>> = vec![None; uses.len()];
    for ((&entity, &ui), &i) in spawned.drawn.iter().zip(&spawned.part_of).zip(&spawned.shape_of) {
        let u = uses.get(ui)?;
        let s = u.part.shapes.get(i)?;
        if s.cap {
            continue;
        }
        let model = &a.parts.get(*part_index.get(ui)?)?.model;
        if *has_tri[ui].get_or_insert_with(|| data.vfs.exists(&sibling(model, "tri"))) {
            let rest = u.meshes.get(i).cloned().flatten().unwrap_or_else(|| s.mesh.clone());
            shapes.push(LipShape { entity, model: model.clone(), cpu: s.cpu.clone(), rest });
        }
    }
    (!shapes.is_empty()).then(|| LipRig { shapes, facegen })
}

/// One morph of a shape at full weight: the vertices it moves and by how much.
pub type Displacement = Vec<(u32, [f32; 3])>;

/// The morphs of a shape that the slots drive: (slot, displacement), for the morph
/// slots the `.tri` has a morph for (by label, [`label_aliases`]) that moves at least
/// one vertex. `shaped`: the `.tri`'s V + K positions with the actor's FaceGen shape
/// applied, for the statistical morphs (`Tri::displacements`).
pub fn shape_morphs(tri: &Tri, shaped: Option<&[[f32; 3]]>) -> Vec<(usize, Displacement)> {
    (0..SLOTS)
        .filter(|&s| morph_slot(s))
        .filter_map(|s| {
            let d = label_aliases(slot_name(s)).into_iter().find_map(|label| tri.displacements(label, shaped))?;
            (!d.is_empty()).then_some((s, d))
        })
        .collect()
}

/// `base` with every morph added at its slot's weight.
pub fn morphed(base: &[[f32; 3]], morphs: &[(usize, Displacement)], weights: &Weights) -> Vec<[f32; 3]> {
    let mut out = base.to_vec();
    for (s, d) in morphs {
        let w = weights[*s];
        if w.abs() < SMALL {
            continue;
        }
        for &(v, delta) in d {
            if let Some(p) = out.get_mut(v as usize) {
                for k in 0..3 {
                    p[k] += w * delta[k];
                }
            }
        }
    }
    out
}

/// Whether `morphed(base, morphs, after)` can differ from `morphed(base, morphs, before)`:
/// a slot of the shape's own morphs whose weight, as `morphed` applies it (nothing below
/// [`SMALL`]), changed. When not, the shape's mesh already holds the positions and is not
/// written: writing a mesh sends it whole to the GPU again and has every entity drawing it
/// checked for its pipeline once more, and a blink or a glance moves only the lids' or the
/// eyes' morphs [design].
pub fn shape_changed(morphs: &[(usize, Displacement)], before: &Weights, after: &Weights) -> bool {
    let applied = |w: f32| if w.abs() < SMALL { 0.0 } else { w };
    morphs.iter().any(|(s, _)| applied(before[*s]) != applied(after[*s]))
}

/// The `.lip` weights to show `t` seconds after the sound started: the file's frames
/// around that moment, nothing before the sound starts or after the last frame, scaled
/// by `ease` (1 while the line is said). Channels neither applied to morphs nor turning
/// the head are 0.
pub fn weights_at(lip: &Lip, t: f64, ease: f32) -> [f32; CHANNELS] {
    let mut w = if t < 0.0 { None } else { lip.sample(t) }.unwrap_or([0.0; CHANNELS]);
    for (c, x) in w.iter_mut().enumerate() {
        *x = if driven(c) || (HEAD_PITCH..=HEAD_YAW).contains(&c) { *x * ease } else { 0.0 };
    }
    w
}

/// The head channels as a turn in the head's own frame (x right, y forward, z up):
/// yaw to the actor's right, then the tip up, then the tip to the right (lipsync.md
/// §3.3: yaw and pitch as `mfg` turns Doc Mitchell's head [observed]; roll a guess).
pub fn head_quat(pitch: f32, roll: f32, yaw: f32) -> Quat {
    let k = HEAD_RADIANS_PER_UNIT;
    Quat::from_rotation_z(-yaw * k) * Quat::from_rotation_x(pitch * k) * Quat::from_rotation_y(roll * k)
}

/// The turn of a `.lip` frame's head channels.
pub fn head_turn(w: &[f32]) -> Quat {
    head_quat(w[HEAD_PITCH], w[HEAD_ROLL], w[HEAD_YAW])
}

/// The local rotation that turns a bone holding `local` by `turn` in its own frame,
/// `bind` being its bind rotation in the actor's space (facing the actor's +y with z up
/// in the bind pose, as head tracking takes the head, actors.md §12.1): the bone's
/// actor-space rotation `parent × local` becomes `parent × local × bind⁻¹ × turn ×
/// bind`, whatever the parent.
pub fn turned_local(local: Quat, bind: Quat, turn: Quat) -> Quat {
    (local * bind.inverse() * turn * bind).normalize()
}

/// The largest head turn of a `.lip`, and of each channel, degrees.
fn largest_turn(lip: &Lip) -> (f32, [f32; 3]) {
    let mut each = [0.0f32; 3];
    let mut all = 0.0f32;
    for f in &lip.frames {
        for (k, c) in [HEAD_PITCH, HEAD_ROLL, HEAD_YAW].into_iter().enumerate() {
            each[k] = each[k].max((f[c] * HEAD_RADIANS_PER_UNIT).abs().to_degrees());
        }
        all = all.max(head_turn(f).angle_between(Quat::IDENTITY).to_degrees());
    }
    (all, each)
}

// ---- state ----------------------------------------------------------------------------

/// A shape being moved.
struct Moving {
    entity: Entity,
    rest: Handle<Mesh>,
    /// The mesh it is drawn from while its face moves.
    mesh: Handle<Mesh>,
    base: Vec<[f32; 3]>,
    morphs: Vec<(usize, Displacement)>,
    /// The weights its mesh was last written with (all 0: the base positions it is made
    /// with).
    shown: Weights,
}

/// The head bone of a face.
struct HeadBone {
    node: Entity,
    bind: Quat,
}

/// An actor whose face moves: its shapes on meshes of their own, what they show now,
/// and its idle state.
struct Face {
    root: Entity,
    shapes: Vec<Moving>,
    head: Option<HeadBone>,
    /// The weights last written to the meshes.
    last: Weights,
    idle: IdleFace,
    /// The head turn wanted this frame (identity: none).
    turn: Quat,
    /// A test aid's held weights.
    held: Option<Weights>,
}

/// A response being animated on one speaker.
struct Speaking {
    path: String,
    started_at: f64,
    /// Real time at which the sound is over.
    ends_at: f64,
    /// Real seconds the sound has been held so far (`nvf_play::voice::Spoken::held_secs`):
    /// the mouth's time is `now - started_at - held_secs`.
    held_secs: f64,
    lip: Arc<Lip>,
    /// Real time at which the voice stopped saying the response (its end, or a cut).
    released_at: Option<f64>,
    /// The emotion's slot and full weight, and the share it starts from (a response
    /// following one of the same emotion starts where that one was).
    emotion: Option<(usize, f32)>,
    emotion_from: f32,
}

impl Speaking {
    /// How much of the line shows: 1 while said, easing to 0 after.
    fn ease(&self, now: f64) -> f32 {
        match self.released_at {
            Some(r) => (1.0 - (now - r) / EASE_SECONDS).clamp(0.0, 1.0) as f32,
            None => 1.0,
        }
    }

    /// The emotion's share of its full weight at `now`.
    fn emotion_share(&self, now: f64) -> f32 {
        let k = ((now - self.started_at - self.held_secs) / EMOTION_IN_SECONDS).clamp(0.0, 1.0) as f32;
        (self.emotion_from + (1.0 - self.emotion_from) * k) * self.ease(now)
    }
}

#[derive(Resource)]
pub struct LipState {
    pub enabled: bool,
    /// The console's ToggleEmotions: no expression or mood is shown.
    pub emotions_off: bool,
    /// The faces between lines (`--no-face-idle` turns them off).
    pub idle: bool,
    tris: HashMap<String, Option<Arc<Tri>>>,
    faces: HashMap<u32, Face>,
    speaking: HashMap<FormId, Speaking>,
    /// (speaker, path, start) of the response last looked at per speaker, animated or
    /// skipped, so that each is handled and logged once.
    seen: HashMap<FormId, (String, f64)>,
    /// The head bones turned this frame: (node, rotation before, rotation written).
    written_heads: Vec<(Entity, Quat, Quat)>,
    hold: Option<Weights>,
    /// Responses animated, skipped (no `.lip`, no face to move), and animated ones that
    /// were cut short.
    pub animated: u64,
    pub skipped: u64,
    pub cut: u64,
    /// Responses whose head channels turned the head, and the largest turn (degrees).
    pub head_turned: u64,
    pub largest_turn: f32,
    /// Frames a head bone was turned by its `.lip`, and those where something posed the
    /// bone again before it was drawn (the turn was lost).
    head_frames: u64,
    head_lost: u64,
    /// Responses that showed an emotion, by expression.
    pub emotions: BTreeMap<&'static str, u64>,
    /// Actors given an idle face, those whose eyes followed a head-tracking target, and
    /// the blinks and glances of faces already let go.
    idle_actors: HashSet<u32>,
    followed: HashSet<u32>,
    /// `FaceMorphs` labels without a slot, named once.
    unknown_labels: HashSet<&'static str>,
    /// Actors whose asked morphs were logged.
    asked_shown: HashSet<u32>,
    blinks: u64,
    glances: u64,
    logged_near: usize,
}

impl LipState {
    /// The meshes the moving faces are drawn from (the profile aid's `profile change
    /// sources:` line).
    pub fn face_meshes(&self) -> impl Iterator<Item = AssetId<Mesh>> + '_ {
        self.faces.values().flat_map(|f| f.shapes.iter().map(|m| m.mesh.id()))
    }

    fn new(enabled: bool, idle: bool, hold: Option<Weights>) -> LipState {
        LipState {
            enabled,
            emotions_off: false,
            idle,
            tris: HashMap::new(),
            faces: HashMap::new(),
            speaking: HashMap::new(),
            seen: HashMap::new(),
            written_heads: Vec::new(),
            hold,
            animated: 0,
            skipped: 0,
            cut: 0,
            head_turned: 0,
            largest_turn: 0.0,
            head_frames: 0,
            head_lost: 0,
            emotions: BTreeMap::new(),
            idle_actors: HashSet::new(),
            followed: HashSet::new(),
            unknown_labels: HashSet::new(),
            asked_shown: HashSet::new(),
            blinks: 0,
            glances: 0,
            logged_near: 0,
        }
    }

    fn tri(&mut self, data: &GameData, path: &str) -> Option<Arc<Tri>> {
        self.tris
            .entry(path.to_string())
            .or_insert_with(|| {
                let bytes = data.vfs.exists(path).then(|| data.read(path)).flatten()?;
                match Tri::parse(&bytes) {
                    Ok(t) => Some(Arc::new(t)),
                    Err(e) => {
                        warn!("lip: {path}: {e}");
                        None
                    }
                }
            })
            .clone()
    }
}

/// The positions of a mesh.
fn positions(mesh: &Mesh) -> Option<Vec<[f32; 3]>> {
    match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
        VertexAttributeValues::Float32x3(p) => Some(p.clone()),
        _ => None,
    }
}

/// Set up the moving shapes of an actor: per shape with a `.tri` of its vertex count, the
/// rest positions (the model's, shaped by the actor's `.egm` as facegen.rs shapes them)
/// and the morphs. Returns the shapes and the number of morphs found.
fn prepare(st: &mut LipState, rig: &LipRig, world: &ActorWorld, data: &GameData, meshes: &mut Assets<Mesh>) -> (Vec<Moving>, usize) {
    let mut out = Vec::new();
    let mut count = 0;
    for s in &rig.shapes {
        let Some(tri) = st.tri(data, &sibling(&s.model, "tri")) else { continue };
        let Some(mut base) = positions(&s.cpu) else { continue };
        if base.len() != tri.vertices {
            continue;
        }
        // The file's own positions (mesh vertices, then the statistical morphs'
        // targets), shaped like the mesh, so a blink's targets follow the face.
        let mut shaped = tri.positions.clone();
        if let Some(fg) = &rig.facegen {
            let egm = world.facegen.lock().expect("facegen cache").egm(data, &sibling(&s.model, "egm"));
            if let Some(egm) = egm.filter(|e| base.len() <= e.vertices) {
                egm.apply(&mut base, &fg.fggs, &fg.fgga);
                egm.apply(&mut shaped, &fg.fggs, &fg.fgga);
            }
        }
        let morphs = shape_morphs(&tri, Some(&shaped));
        if morphs.is_empty() {
            continue;
        }
        let mut mesh = (*s.cpu).clone();
        mesh.asset_usage = RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD;
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, base.clone());
        count += morphs.len();
        out.push(Moving { entity: s.entity, rest: s.rest.clone(), mesh: meshes.add(mesh), base, morphs, shown: [0.0; SLOTS] });
    }
    (out, count)
}

/// The head bone of a skeleton (`Bip01 Head`) and its bind rotation.
fn head_bone(rig: &ActorRig) -> Option<HeadBone> {
    let skel = &rig.skel;
    let i = skel.names.iter().position(|n| n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case("Bip01 Head")))?;
    let node = *rig.nodes.get(i)?;
    Some(HeadBone { node, bind: skel.bind_world.get(i)?.to_scale_rotation_translation().1 })
}

/// Give an actor a moving face if it has none: its shapes on meshes of their own.
/// Returns the number of shapes and morphs, or `None` when it has nothing to move.
#[allow(clippy::too_many_arguments)]
fn ensure_face(st: &mut LipState, commands: &mut Commands, root: Entity, actor: &ActorRig, rig: &LipRig, world: &ActorWorld, data: &GameData, meshes: &mut Assets<Mesh>, now: f64) -> Option<(usize, usize)> {
    if let Some(f) = st.faces.get(&actor.reference) {
        if f.root == root {
            return Some((f.shapes.len(), f.shapes.iter().map(|m| m.morphs.len()).sum()));
        }
        // The actor was drawn again: let the old face go.
        if let Some(old) = st.faces.remove(&actor.reference) {
            restore(commands, meshes, &old.shapes);
        }
    }
    let (shapes, morphs) = prepare(st, rig, world, data, meshes);
    if shapes.is_empty() {
        return None;
    }
    for m in &shapes {
        if let Ok(mut e) = commands.get_entity(m.entity) {
            e.try_insert(Mesh3d(m.mesh.clone()));
        }
    }
    let n = shapes.len();
    let face = Face { root, shapes, head: head_bone(actor), last: [0.0; SLOTS], idle: IdleFace::new(actor.reference, now), turn: Quat::IDENTITY, held: None };
    st.faces.insert(actor.reference, face);
    Some((n, morphs))
}

/// Give the shapes their own meshes back.
fn restore(commands: &mut Commands, meshes: &mut Assets<Mesh>, shapes: &[Moving]) {
    for m in shapes {
        if let Ok(mut e) = commands.get_entity(m.entity) {
            e.try_insert(Mesh3d(m.rest.clone()));
        }
        meshes.remove(&m.mesh);
    }
}

/// Let a face go: its meshes back, its counts kept.
fn let_go(st: &mut LipState, commands: &mut Commands, meshes: &mut Assets<Mesh>, reference: u32) {
    if let Some(f) = st.faces.remove(&reference) {
        st.blinks += u64::from(f.idle.blinks);
        st.glances += u64::from(f.idle.glances);
        restore(commands, meshes, &f.shapes);
    }
}

/// The emotion of a response being said: its expression slot and full weight, and the
/// log's text.
fn emotion_of(script: Option<&nvf_play::script::ScriptState>, info: FormId, response: usize) -> (Option<(usize, f32)>, Option<String>) {
    let Some(r) = script.and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.data.infos.get(&info).and_then(|d| d.responses.get(response)).cloned()) else {
        return (None, None);
    };
    match emotion_slot(r.emotion) {
        Some(s) if r.emotion_value > 0 => {
            let w = (r.emotion_value.min(100) as f32 * EMOTION_SCALE).max(0.0);
            (Some((s, w)), Some(format!("{} {}", slot_name(s), r.emotion_value)))
        }
        _ => (None, None),
    }
}

type Rigs<'w, 's> =
    Query<'w, 's, (Entity, &'static ActorRig, &'static LipRig, &'static GlobalTransform, Option<&'static HeadLook>, Has<NoHeadLook>, Option<&'static FaceMorphs>), (Without<Dying>, Without<Corpse>)>;

/// The slots and weights an actor's [`FaceMorphs`] asks for (labels `lips.rs` does not
/// know are left out and named in `unknown`).
fn asked(m: &FaceMorphs, unknown: &mut HashSet<&'static str>) -> Vec<(usize, f32)> {
    let mut out = Vec::new();
    for (&label, &w) in &m.0 {
        match slot(label) {
            Some(s) if morph_slot(s) => out.push((s, w.clamp(0.0, 1.0))),
            // A head turn (MFG's head modifiers): either way.
            Some(s) if (HEAD_PITCH..=HEAD_YAW).contains(&s) => out.push((s, w.clamp(-1.0, 1.0))),
            _ => {
                if unknown.insert(label) {
                    warn!("face: no morph slot for the label {label:?} (FaceMorphs); left out");
                }
            }
        }
    }
    out
}

/// Whether slot `s` is an expression or a mood: the class a line's emotion takes over
/// ("one expression at a time", lipsync.md §5.4).
pub fn expression_class(s: usize) -> bool {
    (CHANNELS..SLOTS).contains(&s)
}

#[allow(clippy::too_many_arguments)]
fn drive(
    mut commands: Commands,
    real: Res<Time<Real>>,
    game: Res<Time>,
    voice: Res<VoiceState>,
    script: Option<Res<nvf_play::script::ScriptState>>,
    mut state: ResMut<LipState>,
    world: Option<Res<ActorWorld>>,
    data: Res<GameData>,
    mut meshes: ResMut<Assets<Mesh>>,
    rigs: Rigs,
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<ReflectionCamera>)>,
) {
    let st = &mut *state;
    let Some(world) = world else { return };
    let now = real.elapsed_secs_f64();
    let clock = game.elapsed_secs_f64();
    if let Some(w) = st.hold {
        // Test aid: every rig holds the weights, nothing else moves the faces.
        for (root, actor, rig, ..) in &rigs {
            if st.faces.get(&actor.reference).is_some_and(|f| f.root == root) {
                continue;
            }
            if let Some((shapes, morphs)) = ensure_face(st, &mut commands, root, actor, rig, &world, &data, &mut meshes, clock) {
                info!("lip test: holding the given weights on {shapes} shapes ({morphs} morphs) of an actor");
            }
            if let Some(f) = st.faces.get_mut(&actor.reference) {
                f.held = Some(w);
            }
        }
    } else if !st.enabled {
        // Switched on again, a response still being said is taken up where it is.
        st.seen.clear();
        if st.speaking.is_empty() && st.faces.is_empty() {
            return;
        }
    }
    // With each response, how long its sound has been held so far (a menu that holds the
    // line, the pause menu): the mouth holds and goes on with the sound.
    // Switched off while a face moves (the settings page): no response counts as said, so
    // the faces ease out and are let go.
    let spoken: Vec<(FormId, String, f64, f64, f64, FormId, usize)> = if st.enabled {
        voice.spoken().filter_map(|s| Some((s.speaker, s.path?.to_string(), s.started_at, s.ends_at, s.held_secs, s.info, s.response))).collect()
    } else {
        Vec::new()
    };

    // Responses that started: animate them, or say why not.
    for (speaker, path, started_at, ends_at, _, info, response) in &spoken {
        if st.seen.get(speaker).is_some_and(|(p, t)| p == path && t == started_at) {
            continue;
        }
        st.seen.insert(*speaker, (path.clone(), *started_at));
        let before = st.speaking.remove(speaker);
        let lip_path = fnv_voice::lip::lip_path(path);
        let who = script.as_ref().and_then(|s| s.rt.as_ref()).map_or_else(|| format!("{:08X}", speaker.0), |rt| rt.game.data.name(*speaker));
        let lip = voice.bank.as_ref().and_then(|b| b.read(&lip_path)).map(|bytes| Lip::parse(&bytes));
        let lip = match lip {
            Some(Ok(l)) => Arc::new(l),
            Some(Err(e)) => {
                st.skipped += 1;
                info!("lip: skipped {lip_path}: {e}");
                continue;
            }
            None => {
                st.skipped += 1;
                info!("lip: skipped {lip_path}: no such file (speaker {who})");
                continue;
            }
        };
        let Some((root, actor, rig, ..)) = rigs.iter().find(|r| r.1.reference == speaker.0) else {
            st.skipped += 1;
            info!("lip: skipped {lip_path}: speaker {who} has no face rig here (a creature, a robot, an activator, or not drawn)");
            continue;
        };
        let Some((shapes, morphs)) = ensure_face(st, &mut commands, root, actor, rig, &world, &data, &mut meshes, clock) else {
            st.skipped += 1;
            info!("lip: skipped {lip_path}: speaker {who} has no shape with morphs");
            continue;
        };
        st.animated += 1;
        let tracks = lip.active_channels();
        let (turn, [p, r, y]) = largest_turn(&lip);
        let has_head = st.faces.get(&speaker.0).is_some_and(|f| f.head.is_some());
        if has_head && turn > 0.0 {
            st.head_turned += 1;
            st.largest_turn = st.largest_turn.max(turn);
        }
        let (emotion, emotion_text) = emotion_of(script.as_deref(), *info, *response);
        // Does the face carry the expression's morph?
        let shown = emotion.filter(|(s, _)| st.faces.get(&speaker.0).is_some_and(|f| f.shapes.iter().any(|m| m.morphs.iter().any(|(ms, _)| ms == s))));
        if let Some((s, _)) = shown {
            *st.emotions.entry(slot_name(s)).or_default() += 1;
        }
        let emotion_from = match (&before, shown) {
            (Some(b), Some((s, _))) if b.emotion.is_some_and(|(bs, _)| bs == s) => b.emotion_share(now),
            _ => 0.0,
        };
        info!(
            "lip: animating {lip_path} ({} tracks, {} applied; {} frames, {:.2} s from {:.2} s) speaker {who}: {shapes} shapes, {morphs} morphs; head turn {}{}",
            tracks.len(),
            tracks.iter().filter(|&&c| driven(c) || (HEAD_PITCH..=HEAD_YAW).contains(&c)).count(),
            lip.frames.len(),
            lip.frames.len() as f64 / fnv_voice::lip::FRAMES_PER_SECOND,
            lip.start_seconds(),
            if has_head { format!("up to {turn:.1}° (pitch {p:.1}, roll {r:.1}, yaw {y:.1})") } else { "none (no head bone)".to_string() },
            match (&emotion_text, shown) {
                (Some(t), Some(_)) => format!("; emotion {t}"),
                (Some(t), None) => format!("; emotion {t} (no morph)"),
                _ => String::new(),
            },
        );
        st.speaking.insert(
            *speaker,
            Speaking { path: path.clone(), started_at: *started_at, ends_at: *ends_at, held_secs: 0.0, lip, released_at: None, emotion: shown, emotion_from },
        );
    }

    // The responses being said: follow their clocks; note those let go.
    let mut over = Vec::new();
    for (speaker, sp) in st.speaking.iter_mut() {
        let now_said = spoken.iter().find(|(s, p, t, ..)| s == speaker && *p == sp.path && *t == sp.started_at);
        if let Some((_, _, _, ends_at, held_secs, ..)) = now_said {
            (sp.ends_at, sp.held_secs) = (*ends_at, *held_secs);
        }
        if now_said.is_none() && sp.released_at.is_none() {
            sp.released_at = Some(now);
            if st.enabled && now < sp.ends_at - CUT_SECONDS {
                st.cut += 1;
                info!("lip: cut {} {:.2} s into its {:.2} s (the response was skipped or its speaker interrupted)", sp.path, now - sp.started_at - sp.held_secs, sp.ends_at - sp.started_at - sp.held_secs);
            }
        }
        if sp.released_at.is_some() && sp.ease(now) <= 0.0 {
            over.push(*speaker);
        }
    }
    for s in over {
        st.speaking.remove(&s);
    }

    // A speaker drawn again while it speaks (its cell reloaded) gets its face back.
    for (root, actor, rig, ..) in &rigs {
        if st.speaking.contains_key(&FormId(actor.reference)) && st.faces.get(&actor.reference).is_none_or(|f| f.root != root) {
            ensure_face(st, &mut commands, root, actor, rig, &world, &data, &mut meshes, clock);
        }
    }

    // Faces near the camera between lines, and faces other systems ask morphs of.
    let idle_on = st.enabled && st.idle && st.hold.is_none();
    let cam = camera.iter().next().map(GlobalTransform::translation);
    if st.enabled && st.hold.is_none() {
        if let Some(cam) = cam {
            for (root, actor, rig, g, .., m) in &rigs {
                if m.is_some_and(|m| !m.is_empty()) && g.translation().distance(cam) <= IDLE_RELEASE {
                    ensure_face(st, &mut commands, root, actor, rig, &world, &data, &mut meshes, clock);
                }
            }
        }
    }
    if idle_on {
        if let Some(cam) = cam {
            let mut near: usize = 0;
            for (root, actor, rig, g, _, off, _) in &rigs {
                if off || g.translation().distance(cam) > IDLE_RADIUS {
                    continue;
                }
                near += 1;
                if ensure_face(st, &mut commands, root, actor, rig, &world, &data, &mut meshes, clock).is_some() {
                    st.idle_actors.insert(actor.reference);
                }
            }
            if near.abs_diff(st.logged_near) >= 5 {
                st.logged_near = near;
                info!("face idle: {near} faces near the camera");
            }
        }
    }

    // Move the faces; let go of those nothing moves any more.
    let mut gone = Vec::new();
    for (&reference, face) in st.faces.iter_mut() {
        face.turn = Quat::IDENTITY;
        let Ok((_, _, _, g, look, off, morphs)) = rigs.get(face.root) else {
            // Despawned, dying or dead.
            gone.push(reference);
            continue;
        };
        let mut w: Weights = [0.0; SLOTS];
        let mut speech = 0.0;
        // How far a line's emotion has taken over the expressions asked for.
        let mut taken = 0.0;
        if let Some(h) = face.held {
            w = h;
            face.turn = head_turn(&h);
        } else if let Some(sp) = st.speaking.get(&FormId(reference)) {
            speech = sp.ease(now);
            let lip = weights_at(&sp.lip, now - sp.started_at - sp.held_secs, speech);
            w[..CHANNELS].copy_from_slice(&lip);
            face.turn = head_turn(&lip);
            w[HEAD_PITCH..=HEAD_YAW].fill(0.0);
            if let Some((s, full)) = sp.emotion.filter(|_| !st.emotions_off) {
                taken = sp.emotion_share(now);
                w[s] += full * taken;
            }
        }
        let far = cam.is_none_or(|c| g.translation().distance(c) > IDLE_RELEASE);
        let asks = st.enabled && face.held.is_none() && !far && morphs.is_some_and(|m| !m.is_empty());
        if asks {
            let list = asked(morphs.expect("asks"), &mut st.unknown_labels);
            if st.asked_shown.insert(reference) {
                let carried = |s: usize| face.shapes.iter().any(|m| m.morphs.iter().any(|(ms, _)| *ms == s));
                let names: Vec<String> = list.iter().map(|&(s, x)| format!("{} {x:.2}{}", slot_name(s), if carried(s) { "" } else { " (no morph)" })).collect();
                info!("face: {reference:08X} shows asked morphs: {}", names.join(", "));
            }
            let mut head: Weights = [0.0; SLOTS];
            for (s, x) in list {
                if (HEAD_PITCH..=HEAD_YAW).contains(&s) {
                    head[s] = x;
                } else if !(st.emotions_off && expression_class(s)) {
                    w[s] += if expression_class(s) { x * (1.0 - taken) } else { x };
                }
            }
            face.turn *= head_turn(&head);
        }
        let idle_here = idle_on && !off && !far && face.held.is_none();
        if idle_here && speech < 1.0 {
            let share = 1.0 - speech;
            let (lids, glance) = face.idle.step(clock);
            let target = look.and_then(HeadLook::off_target).map(eyes_onto);
            if target.is_some_and(|t| t.0.abs() > 0.01 || t.1.abs() > 0.01) {
                st.followed.insert(reference);
            }
            let (th, tv) = target.unwrap_or((0.0, 0.0));
            let h = (th + glance.0).clamp(-EYE_LIMIT, EYE_LIMIT);
            let v = (tv + glance.1).clamp(-EYE_LIMIT, EYE_LIMIT);
            let mut idle: Weights = [0.0; SLOTS];
            for c in ["BlinkLeft", "BlinkRight"].into_iter().filter_map(channel) {
                idle[c] = lids;
            }
            set_look(&mut idle, h, v);
            for (x, i) in w.iter_mut().zip(idle) {
                *x += share * i;
            }
        }
        if face.held.is_none() && !st.speaking.contains_key(&FormId(reference)) && !idle_here && !asks {
            gone.push(reference);
            continue;
        }
        let rest_now = w.iter().all(|x| x.abs() < SMALL);
        let was_rest = face.last.iter().all(|x| x.abs() < SMALL);
        let changed = w.iter().zip(&face.last).any(|(a, b)| (a - b).abs() > CHANGE);
        if !(changed || (rest_now && !was_rest)) {
            continue;
        }
        face.last = w;
        for m in &mut face.shapes {
            // Only the shapes whose own morphs moved (`get_mut` alone marks a mesh changed).
            if !shape_changed(&m.morphs, &m.shown, &w) {
                continue;
            }
            if let Some(mut mesh) = meshes.get_mut(&m.mesh) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, morphed(&m.base, &m.morphs, &w));
                m.shown = w;
            }
        }
    }
    for r in gone {
        let_go(st, &mut commands, &mut meshes, r);
    }
}

/// Turn the speakers' heads by their `.lip` head channels, on top of what the frame's
/// animation, head tracking and a conversation wrote to the bone.
fn turn_heads(mut state: ResMut<LipState>, mut transforms: Query<&mut Transform>) {
    let st = &mut *state;
    for face in st.faces.values() {
        let Some(h) = &face.head else { continue };
        if face.turn.abs_diff_eq(Quat::IDENTITY, 1e-7) {
            continue;
        }
        let Ok(mut t) = transforms.get_mut(h.node) else { continue };
        let base = t.rotation;
        t.rotation = turned_local(base, h.bind, face.turn);
        st.written_heads.push((h.node, base, t.rotation));
    }
}

/// Before transforms are propagated: whether the head turns written this frame are still
/// on the bones, so that a system posing the head after `turn_heads` shows in the summary.
fn check_heads(mut state: ResMut<LipState>, transforms: Query<&Transform>) {
    let st = &mut *state;
    for (node, _, written) in &st.written_heads {
        st.head_frames += 1;
        if transforms.get(*node).is_ok_and(|t| !t.rotation.abs_diff_eq(*written, 1e-6)) {
            st.head_lost += 1;
        }
    }
}

/// At the start of a frame: take last frame's head turns off again where nothing posed
/// the bone since, so the systems that turn heads never take them for animation.
fn untouch(mut state: ResMut<LipState>, mut transforms: Query<&mut Transform>) {
    for (node, base, written) in state.written_heads.drain(..) {
        if let Ok(mut t) = transforms.get_mut(node) {
            if t.rotation.abs_diff_eq(written, 1e-6) {
                t.rotation = base;
            }
        }
    }
}

/// At exit: what the faces did.
fn report(mut exit: MessageReader<AppExit>, state: Res<LipState>) {
    if exit.read().next().is_none() {
        return;
    }
    let st = &*state;
    let emotions: Vec<String> = st.emotions.iter().map(|(k, v)| format!("{k} {v}")).collect();
    info!(
        "lip summary: {} responses animated, {} skipped, {} cut; head turned on {} (largest {:.1}°, lost on {} of {} frames); emotions shown on {} ({})",
        st.animated,
        st.skipped,
        st.cut,
        st.head_turned,
        st.largest_turn,
        st.head_lost,
        st.head_frames,
        st.emotions.values().sum::<u64>(),
        if emotions.is_empty() { "none".to_string() } else { emotions.join(", ") },
    );
    if st.idle && st.enabled {
        let blinks = st.blinks + st.faces.values().map(|f| u64::from(f.idle.blinks)).sum::<u64>();
        let glances = st.glances + st.faces.values().map(|f| u64::from(f.idle.glances)).sum::<u64>();
        info!(
            "face idle summary: {} actors had an idle face; {blinks} blinks, {glances} glances, eyes followed a head-tracking target on {} of them",
            st.idle_actors.len(),
            st.followed.len()
        );
    }
}

/// Test aid `NVF_LIP_TEST_HOLD=<channel or expression>=<weight>[,...]`: every actor with
/// a rig holds these weights from the start (screenshots of single mouth shapes; no
/// line needs to be said). Not a game behaviour.
fn parse_hold(v: &str) -> Result<Weights, String> {
    let mut w = [0.0; SLOTS];
    for item in v.split(',').filter(|s| !s.trim().is_empty()) {
        let (name, value) = item.split_once('=').ok_or(format!("{item}: expected <channel>=<weight>"))?;
        let s = slot(name.trim()).ok_or(format!("{name}: no such channel"))?;
        w[s] = value.trim().parse().map_err(|_| format!("{value}: not a number"))?;
    }
    Ok(w)
}

/// Test aid `NVF_FACE_TEST_MORPHS=<label>=<weight>[,...]`: every drawn actor with a face
/// rig asks for these morphs through [`FaceMorphs`], as the combat face does (the path
/// other systems use, lipsync.md §5.4). Not a game behaviour.
#[derive(Resource)]
struct FaceTest(Vec<(&'static str, f32)>);

fn parse_face_test(v: &str) -> Result<Vec<(&'static str, f32)>, String> {
    let mut out = Vec::new();
    for item in v.split(',').filter(|s| !s.trim().is_empty()) {
        let (name, value) = item.split_once('=').ok_or(format!("{item}: expected <label>=<weight>"))?;
        let s = slot(name.trim()).filter(|&s| morph_slot(s)).ok_or(format!("{name}: no such morph"))?;
        out.push((slot_name(s), value.trim().parse().map_err(|_| format!("{value}: not a number"))?));
    }
    Ok(out)
}

/// The console's `MFG` (`fnv_game::Game::mfg`): each rigged actor's face asks for the
/// morphs set on it, intensity 100 a weight of 1 [hypothesis: as a `.lip`'s channels],
/// through [`FaceMorphs`]; a morph taken off (or the whole face, by `MFG Reset`) is set
/// back to 0. Labels: the expressions by `MFG_EXPRESSIONS`, phonemes and modifiers by the
/// `.lip` channel of the same number.
fn mfg(mut commands: Commands, script: Option<Res<nvf_play::script::ScriptState>>, mut shown: Local<HashMap<Entity, Vec<(&'static str, f32)>>>, mut rigs: Query<(Entity, &ActorRig, Option<&mut FaceMorphs>), With<LipRig>>) {
    use fnv_game::functions::console::MFG_EXPRESSIONS;
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    if game.mfg.is_empty() && shown.is_empty() {
        return;
    }
    for (e, rig, morphs) in &mut rigs {
        let mut want: Vec<(&'static str, f32)> = Vec::new();
        if let Some(f) = game.mfg.get(&FormId(rig.reference)) {
            want.extend(f.expression.and_then(|(id, v)| Some((*MFG_EXPRESSIONS.get(usize::from(id))?, v))));
            want.extend(f.phonemes.iter().filter_map(|(&id, &v)| Some((*CHANNEL_NAMES.get(usize::from(id))?, v))));
            want.extend(f.modifiers.iter().filter_map(|(&id, &v)| Some((*CHANNEL_NAMES.get(PHONEMES + usize::from(id))?, v))));
        }
        let before = shown.remove(&e).unwrap_or_default();
        if want.is_empty() && before.is_empty() {
            continue;
        }
        let mut fresh = FaceMorphs::default();
        let had = morphs.is_some();
        let m = match morphs {
            Some(m) => m.into_inner(),
            None => &mut fresh,
        };
        for &(label, _) in &before {
            if !want.iter().any(|&(w, _)| w == label) {
                m.set(label, 0.0);
            }
        }
        for &(label, v) in &want {
            if m.get(label) != v / 100.0 {
                m.set(label, v / 100.0);
            }
        }
        if want != before {
            let list: Vec<String> = want.iter().map(|(l, v)| format!("{l} {v:.0}")).collect();
            info!("face: {:08X} MFG {}", rig.reference, if list.is_empty() { "reset".to_string() } else { list.join(", ") });
        }
        if !want.is_empty() {
            shown.insert(e, want);
        }
        if !had && !fresh.is_empty() {
            commands.entity(e).try_insert(fresh);
        }
    }
}

/// Rigged actors not yet given [`FaceMorphs`].
type Unasked = (With<LipRig>, Without<FaceMorphs>);

fn face_test(mut commands: Commands, test: Res<FaceTest>, new: Query<(Entity, &ActorRig), Unasked>) {
    for (e, actor) in &new {
        let mut m = FaceMorphs::default();
        for &(label, w) in &test.0 {
            m.set(label, w);
        }
        let shown: Vec<String> = test.0.iter().map(|&(l, _)| format!("{l} {:.2}", m.get(l))).collect();
        info!("face test: {:08X} asks for {}", actor.reference, shown.join(", "));
        commands.entity(e).insert(m);
    }
}

pub struct LipsPlugin {
    pub enabled: bool,
    /// The faces between lines (`--no-face-idle`).
    pub idle: bool,
}

impl Plugin for LipsPlugin {
    fn build(&self, app: &mut App) {
        let hold = std::env::var("NVF_LIP_TEST_HOLD").ok().and_then(|v| match parse_hold(&v) {
            Ok(w) => Some(w),
            Err(e) => {
                warn!("NVF_LIP_TEST_HOLD: {e}");
                None
            }
        });
        // A held face and a spoken one would fight over the same shapes.
        app.insert_resource(LipState::new(self.enabled && hold.is_none(), self.idle, hold))
            .add_systems(PreUpdate, untouch)
            .add_systems(
                Update,
                (drive, turn_heads)
                    .chain()
                    .after(nvf_play::PlaySet::Ui)
                    .after(crate::locomotion::drive)
                    .after(crate::head_look::track)
                    .after(crate::dialogue_look::turn_speaker)
                    .after(crate::speech_look::face),
            )
            .add_systems(Update, mfg.before(drive))
            .add_systems(PostUpdate, check_heads.before(bevy::transform::TransformSystems::Propagate))
            .add_systems(Last, report);
        match std::env::var("NVF_FACE_TEST_MORPHS").map(|v| parse_face_test(&v)) {
            Ok(Ok(m)) => {
                app.insert_resource(FaceTest(m)).add_systems(Update, face_test.before(drive));
            }
            Ok(Err(e)) => warn!("NVF_FACE_TEST_MORPHS: {e}"),
            Err(_) => {}
        }
        if !self.enabled {
            info!("lip sync is off (--no-lip)");
        } else if !self.idle {
            info!("face idle: off (--no-face-idle)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lip(start: i32, frames: Vec<[f32; CHANNELS]>) -> Lip {
        Lip { version: 1, flag: 1, start_frame: start, frames }
    }

    fn frame(set: &[(usize, f32)]) -> [f32; CHANNELS] {
        let mut f = [0.0; CHANNELS];
        for &(c, v) in set {
            f[c] = v;
        }
        f
    }

    fn weights(set: &[(usize, f32)]) -> Weights {
        let mut f = [0.0; SLOTS];
        for &(c, v) in set {
            f[c] = v;
        }
        f
    }

    #[test]
    fn mouth_blink_brow_and_look_channels_are_driven_head_turns_the_bone() {
        let names: Vec<&str> = (0..CHANNELS).filter(|&c| driven(c)).map(|c| CHANNEL_NAMES[c]).collect();
        assert_eq!(names.len(), 30);
        for n in ["Aah", "W", "BlinkRight", "BrowUpLeft", "LookDown", "LookLeft", "LookRight", "LookUp"] {
            assert!(names.contains(&n), "{n}");
        }
        assert_eq!((CHANNEL_NAMES[HEAD_PITCH], CHANNEL_NAMES[HEAD_ROLL], CHANNEL_NAMES[HEAD_YAW]), ("HeadPitch", "HeadRoll", "HeadYaw"));
        for c in HEAD_PITCH..=HEAD_YAW {
            assert!(!driven(c) && !morph_slot(c));
        }
        // The expressions are slots of their own, by emotion code.
        assert_eq!(emotion_slot(0), None);
        assert_eq!(emotion_slot(1).map(slot_name), Some("Anger"));
        assert_eq!(emotion_slot(5).map(slot_name), Some("Happy"));
        assert_eq!(emotion_slot(7).map(slot_name), Some("Pained"));
        assert_eq!(emotion_slot(8), None);
        assert!(morph_slot(CHANNELS) && morph_slot(SLOTS - 1) && !morph_slot(SLOTS));
        assert_eq!(slot("happy"), emotion_slot(5));
        assert_eq!(slot("BigAah"), Some(1));
        assert_eq!(slot("combatanger").map(slot_name), Some("CombatAnger"));
        assert_eq!(slot("MoodNeutral").map(slot_name), Some("MoodNeutral"));
        assert!(expression_class(slot("CombatAnger").unwrap()) && expression_class(emotion_slot(1).unwrap()) && !expression_class(0));
        let mut m = FaceMorphs::default();
        m.set("CombatAnger", 1.5);
        m.set("Nose", 1.0);
        let mut unknown = HashSet::new();
        assert_eq!(asked(&m, &mut unknown), vec![(slot("CombatAnger").unwrap(), 1.0)]);
        assert!(unknown.contains("Nose"));
    }

    #[test]
    fn weights_follow_the_sounds_clock() {
        // Two frames before the sound, then Aah opening; a head turn rides along.
        let l = lip(-2, vec![frame(&[(0, 0.9)]), frame(&[(0, 0.8)]), frame(&[(0, 0.2), (30, 0.1), (18, 0.4)]), frame(&[(0, 0.6), (16, 1.0)])]);
        // At the sound's start: frame 2; the head channel is kept, an unused one is not.
        let w = weights_at(&l, 0.0, 1.0);
        assert!((w[0] - 0.2).abs() < 1e-6 && (w[30] - 0.1).abs() < 1e-6);
        // Half a frame later: between frames 2 and 3.
        let w = weights_at(&l, 0.5 / 30.0, 1.0);
        assert!((w[0] - 0.4).abs() < 1e-5 && (w[16] - 0.5).abs() < 1e-5);
        // Past the last frame, and before the sound: rest.
        assert_eq!(weights_at(&l, 1.0, 1.0), [0.0; CHANNELS]);
        assert_eq!(weights_at(&l, -0.01, 1.0), [0.0; CHANNELS]);
        // Easing out scales everything, the head too.
        let w = weights_at(&l, 0.0, 0.5);
        assert!((w[0] - 0.1).abs() < 1e-6 && (w[30] - 0.05).abs() < 1e-6);
    }

    #[test]
    fn morphs_add_on_top_of_the_shaped_face() {
        let base = vec![[1.0, 1.0, 1.0], [2.0, 2.0, 2.0], [3.0, 3.0, 3.0]];
        let anger = emotion_slot(1).unwrap();
        let morphs = vec![(0usize, vec![(1u32, [0.0, 0.0, -2.0])]), (2, vec![(1, [1.0, 0.0, 0.0]), (2, [0.0, 4.0, 0.0])]), (16, vec![(9, [1.0; 3])]), (anger, vec![(0, [0.0, 2.0, 0.0])])];
        let w = weights(&[(0, 0.5), (2, 0.25), (16, 1.0), (anger, 0.5)]);
        let p = morphed(&base, &morphs, &w);
        assert_eq!(p, vec![[1.0, 2.0, 1.0], [2.25, 2.0, 1.0], [3.0, 4.0, 3.0]]);
        // At rest nothing moves.
        assert_eq!(morphed(&base, &morphs, &[0.0; SLOTS]), base);
    }

    #[test]
    fn a_shape_is_written_only_when_its_own_morphs_move() {
        let base = vec![[1.0, 1.0, 1.0], [2.0, 2.0, 2.0]];
        let lids = vec![(0usize, vec![(0u32, [0.0, 0.0, -1.0])])];
        let eyes = vec![(2usize, vec![(1u32, [1.0, 0.0, 0.0])])];
        let before = weights(&[(0, 0.5), (2, 0.25)]);
        // Only slot 2 moved: the lids' shape is unchanged, the eyes' is not.
        let after = weights(&[(0, 0.5), (2, 0.5), (7, 1.0)]);
        assert!(!shape_changed(&lids, &before, &after));
        assert!(shape_changed(&eyes, &before, &after));
        // Unchanged means the same positions, bit for bit.
        assert_eq!(morphed(&base, &lids, &before), morphed(&base, &lids, &after));
        // Weights below SMALL move nothing, whatever their value.
        let tiny = weights(&[(0, SMALL * 0.5)]);
        assert!(!shape_changed(&lids, &[0.0; SLOTS], &tiny));
        assert_eq!(morphed(&base, &lids, &tiny), base);
        assert!(shape_changed(&lids, &tiny, &weights(&[(0, SMALL * 2.0)])));
    }

    #[test]
    fn a_shapes_morphs_come_from_its_tri_by_label() {
        // A three-vertex "mouth" with Aah, an empty BMP, an expression, a mood, a blink
        // and a look.
        let mut b = b"FRTRI003".to_vec();
        for x in [3i32, 0, 0, 0, 0, 0, 0, 4, 2, 2] {
            b.extend(x.to_le_bytes());
        }
        b.extend([0u8; 16]);
        for p in [[0.0f32; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, -0.5], [0.0, 1.0, 0.5]] {
            for c in p {
                b.extend(c.to_le_bytes());
            }
        }
        let string = |b: &mut Vec<u8>, s: &str| {
            b.extend((s.len() as i32 + 1).to_le_bytes());
            b.extend(s.as_bytes());
            b.push(0);
        };
        for (label, deltas) in [("Aah", [[0i16, 0, 0], [0, 0, -4], [0, 0, 0]]), ("BMP", [[0; 3]; 3]), ("Happy", [[2, 0, 0], [0; 3], [0; 3]]), ("MoodSad", [[1; 3]; 3])] {
            string(&mut b, label);
            b.extend(0.5f32.to_le_bytes());
            for d in deltas {
                for c in d {
                    b.extend(c.to_le_bytes());
                }
            }
        }
        for (label, index) in [("BlinkLeft", 1u32), ("LookUp", 2)] {
            string(&mut b, label);
            b.extend(1i32.to_le_bytes());
            b.extend(index.to_le_bytes());
        }
        let tri = Tri::parse(&b).unwrap();
        let morphs = shape_morphs(&tri, None);
        // Aah, the blink, the look, the expression and the mood; BMP moves nothing.
        let found: Vec<usize> = morphs.iter().map(|m| m.0).collect();
        assert_eq!(found, vec![0, 16, 27, emotion_slot(5).unwrap(), slot("MoodSad").unwrap()]);
        assert_eq!(morphs[0].1, vec![(1, [0.0, 0.0, -2.0])]);
        assert_eq!(morphs[1].1, vec![(1, [0.0, 0.0, -0.5])]);
        assert_eq!(morphs[2].1, vec![(2, [0.0, 0.0, 0.5])]);
        assert_eq!(morphs[3].1, vec![(0, [1.0, 0.0, 0.0])]);
    }

    /// The head's forward and up in the head's frame after a turn from the bind pose.
    fn turned_axes(bind: Quat, turn: Quat) -> (Vec3, Vec3) {
        // In the bind pose (parent identity, local = bind) the head's actor-space
        // rotation is bind; its frame (x right, y forward, z up) is head × bind⁻¹.
        let head = turned_local(bind, bind, turn);
        let frame = head * bind.inverse();
        (frame * Vec3::Y, frame * Vec3::Z)
    }

    #[test]
    fn the_head_tips_up_turns_right_and_tips_right_for_positive_channels() {
        let bind = Quat::from_euler(EulerRot::XYZ, 0.3, -1.2, 2.0);
        let a = 0.1f32;
        let (fwd, _) = turned_axes(bind, head_quat(a, 0.0, 0.0));
        assert!((fwd - Vec3::new(0.0, a.cos(), a.sin())).length() < 1e-5, "pitch: {fwd}");
        let (fwd, _) = turned_axes(bind, head_quat(0.0, 0.0, a));
        assert!((fwd - Vec3::new(a.sin(), a.cos(), 0.0)).length() < 1e-5, "yaw: {fwd}");
        let (fwd, up) = turned_axes(bind, head_quat(0.0, a, 0.0));
        assert!((fwd - Vec3::Y).length() < 1e-5 && (up - Vec3::new(a.sin(), 0.0, a.cos())).length() < 1e-5, "roll: {up}");
        // No channel, no turn.
        assert!(turned_local(bind, bind, head_turn(&[0.0; CHANNELS])).abs_diff_eq(bind, 1e-6));
    }

    #[test]
    fn the_turn_goes_on_top_of_whatever_the_head_faces() {
        // A head already turned 40° to the side by tracking tips up in its own frame: its
        // forward rises, its sideways direction stays.
        let bind = Quat::from_rotation_x(0.7);
        let tracked = Quat::from_rotation_z(-0.7) * bind;
        let local = turned_local(tracked, bind, head_quat(0.1, 0.0, 0.0));
        let before = tracked * bind.inverse();
        let after = local * bind.inverse();
        assert!((before * Vec3::X - after * Vec3::X).length() < 1e-5);
        assert!((after * Vec3::Y).z > 0.09);
    }

    #[test]
    fn blinks_and_glances() {
        assert_eq!(blink_weight(-0.01), 0.0);
        assert!((blink_weight(BLINK_UP) - 1.0).abs() < 1e-6);
        assert!((blink_weight(BLINK_UP + BLINK_DOWN / 2.0) - 0.5).abs() < 1e-6);
        assert_eq!(blink_weight(BLINK_UP + BLINK_DOWN), 0.0);
        // Over a minute: about one blink every 3.25 s, every one 0.2 s long; glances
        // stay small; the same seed gives the same face.
        let run = |seed| {
            let mut f = IdleFace::new(seed, 0.0);
            let mut shut = 0;
            let mut widest = 0.0f32;
            for i in 0..(60 * 120) {
                let (lids, (h, v)) = f.step(f64::from(i) / 120.0);
                if lids > 0.0 {
                    shut += 1;
                }
                widest = widest.max(h.abs() / GLANCE_SIDE).max(v.abs() / GLANCE_UPDOWN);
            }
            (f.blinks, f.glances, shut, widest)
        };
        let (blinks, glances, shut, widest) = run(0x0010_4C7D);
        assert!((12..=28).contains(&blinks), "{blinks}");
        assert!((15..=60).contains(&glances), "{glances}");
        assert!((shut as f64 / f64::from(blinks) / 120.0 - 0.2).abs() < 0.02);
        assert!(widest <= 1.0 + 1e-6);
        assert_eq!(run(0x0010_4C7D), (blinks, glances, shut, widest));
        assert_ne!(run(7).0 * 1000 + run(7).1, blinks * 1000 + glances);
    }

    #[test]
    fn the_eyes_make_up_what_the_head_falls_short_of() {
        // The head 11° to the right of its target and 7° above it: the eyes turn left by
        // half their reach and down by a third.
        let (h, v) = eyes_onto((11f32.to_radians(), 7f32.to_radians()));
        assert!((h + 0.5).abs() < 1e-5 && (v + 7.0 / 22.0).abs() < 1e-5);
        // Below the target the eyes go up, by the smaller reach of LookUp.
        let (_, v) = eyes_onto((0.0, -7f32.to_radians()));
        assert!((v - 0.5).abs() < 1e-5);
        // Never past the limit.
        assert_eq!(eyes_onto((PI, -PI)), (-EYE_LIMIT, EYE_LIMIT));
        let mut w = [0.0; SLOTS];
        set_look(&mut w, -0.2, 0.1);
        assert_eq!((w[channel("LookLeft").unwrap()], w[channel("LookRight").unwrap()], w[channel("LookUp").unwrap()], w[channel("LookDown").unwrap()]), (0.2, 0.0, 0.1, 0.0));
    }

    #[test]
    fn hold_aid_parses() {
        let w = parse_hold("BigAah=1, blinkleft=0.5, Anger=0.3").unwrap();
        assert_eq!((w[1], w[16], w[emotion_slot(1).unwrap()]), (1.0, 0.5, 0.3));
        assert!(parse_hold("Nose=1").is_err());
        assert!(parse_hold("Aah").is_err());
    }
}
