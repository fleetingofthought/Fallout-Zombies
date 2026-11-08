//! The actor's side of a conversation's facing (dialogue step D7,
//! docs/design/dialogue-play.md §8): while the dialogue screen is open, the actor the
//! player talks to turns its head to the player's eyes and, standing, its body too. The
//! player's side (the view turning to the actor's face) is
//! `nvf_play::dialogue_look`, which this file tells where the face is
//! ([`SpeakerFace`]).
//!
//! Only conversations (`Game::dialogue`). A line an actor says to someone outside one
//! (SayTo, a hello, a conversation between actors) is `speech_look.rs`'s
//! (dialogue-play.md §9).
//!
//! - **Head.** The head bone is turned on top of the frame's animation (after
//!   `locomotion::drive` has posed the skeleton), towards the player's eye, eased in over
//!   [`HEAD_IN_SECONDS`] and out over [`HEAD_OUT_SECONDS`] when the conversation ends.
//!   Only the animation's slow average head direction is aimed at the eye
//!   ([`KEEP_SECONDS`]): the talk clips' nods and glances stay on the turned head.
//!   Which bone and how far come from the game's data where it has them: the body part
//!   with the "IK Data - Headtracking" flag in the actor's body part data names the bone
//!   (its IK start node, `Bip01 Head`) and the largest angle (its Tracking Max Angle, 60
//!   for `DefaultBodyPartData`) [documented: GECK wiki "Body Part", owner export;
//!   observed: FalloutNV.esm BPTD 0x1D]. That angle limits the turn to either side
//!   [hypothesis: degrees, and that it bounds the yaw], measured from where the bone
//!   above the neck faces (the chest), not the actor's root: a pose that twists the upper
//!   body away (a fleeing actor's cower idle) would otherwise let the head end up facing
//!   backwards [observed: owner report 2026-11-07, #1814]; up and down the head goes at most
//!   [`HEAD_PITCH_LIMIT`] [design]. The neck bones above the head are turned by up to
//!   [`NECK_SHARE`] of the turn ([`shares`]), so the neck does not twist at one joint
//!   [design; whether the original turns anything but the IK start node is not known].
//!   Everything that hangs from the head bone or is skinned to it follows: the face and
//!   its lip sync (`lips.rs` moves vertices, not bones), hair, eyes, hats.
//! - **Body.** An actor standing in its idle whose body faces more than
//!   [`BODY_TURN_FROM`] away from the player turns on the spot to face them, eased, at
//!   about [`BODY_TURN_SPEED`], asking for the skeleton's turn clips through
//!   `Locomotion::turn` [design; the original can turn the whole body to a head-track
//!   target: documented for the `Look` function, GECK wiki "Look", owner export]. The
//!   body keeps its new heading afterwards [design: AI packages will take over].
//! - **Head only** for an actor whose body something else places or holds: an actor the
//!   AI runs (`--play`, `--npc-packages`: `fnv_ai::Ai::update_held` turns its body on the
//!   spot and keeps it facing the player for the whole conversation, in the walker's
//!   heading, which the engine draws; turning the drawn body here would be undone by the
//!   next frame's drawing), a `--npc-travel` traveller (its heading is the walker's), an
//!   actor under the walk test,
//!   and one marked [`BodyHeld`] (in furniture or lying down: `furniture.rs` marks the
//!   actors it seats).
//! - **Nobody turns** when the actor is dead or a ragdoll (never touched), is not a
//!   human-style NPC (a creature, a robot, a talking activator), has no such bone, or its
//!   body part data does not allow head tracking. The view still turns to it.
//! - **Pausing** (docs/design/dialogue.md §8). The eases run on the virtual clock: the
//!   Esc pause menu holds them. With `--menu-pause original` the picture stands still
//!   during the conversation, so the turns are completed at once and the frozen picture
//!   shows the two facing each other; the head eases back when the clock runs again.
//!
//! The angles and timings are NVForever's design choices unless tagged otherwise.
//!
//! Log, one line each, starting `dialogue look:`: an actor starts facing the player
//! (head and body, head only and why, or nobody and why; the angles), and stops.
//!
//! Test aid `NVF_PLAY_TEST_TALK_HEAD_ONLY=1`: no body turns (as if every actor were
//! [`BodyHeld`]), to see the head at its limit. Not a game behaviour.

use crate::head_look::{HeadLook, LookAt};
use crate::limbs::ActorRig;
use crate::locomotion::{Dying, Locomotion, WalkTest};
use crate::nav::NavState;
use crate::ragdoll::Corpse;
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_actor::locomotion::Move;
use fnv_sim::FormId;
use nvf_play::combat::{is_dead, Combat};
use nvf_play::dialogue_look::{smooth, wrap, LookSettings, SpeakerFace};
use nvf_play::pause::{MenuPause, Paused};
use nvf_play::player::Player;
use nvf_play::script::ScriptState;
use nvf_play::sim::SimState;
use nvf_play::{space, PlaySet};
use std::collections::HashMap;
use std::f32::consts::PI;

/// Seconds the head takes to turn to the player, and back to its animation [design].
pub const HEAD_IN_SECONDS: f32 = 0.45;
pub const HEAD_OUT_SECONDS: f32 = 0.6;
/// The largest turn of the head to either side when the body part data gives none,
/// radians [design; `DefaultBodyPartData` gives 60°].
pub const HEAD_YAW_LIMIT: f32 = 60.0 * PI / 180.0;
/// The largest turn the data may ask for [design: no neck goes further].
pub const HEAD_YAW_LIMIT_MAX: f32 = 80.0 * PI / 180.0;
/// The largest tilt of the head up or down, radians [design].
pub const HEAD_PITCH_LIMIT: f32 = 30.0 * PI / 180.0;
/// The share of the turn the neck bone next to the head is turned by; the neck bones
/// take evenly growing shares up to it ([`shares`]) [design].
pub const NECK_SHARE: f32 = 0.4;
/// A player further behind the actor than this keeps the side the head turned to
/// (straight behind, left and right are equally far), radians [design].
pub const BEHIND: f32 = 135.0 * PI / 180.0;
/// A standing actor's body turns when it faces further than this from the player,
/// radians [design].
pub const BODY_TURN_FROM: f32 = 30.0 * PI / 180.0;
/// Its average turning speed, radians per second, and the shortest and longest turn,
/// seconds [design].
pub const BODY_TURN_SPEED: f32 = 150.0 * PI / 180.0;
pub const BODY_TURN_MIN_SECONDS: f32 = 0.5;
pub const BODY_TURN_MAX_SECONDS: f32 = 1.4;
/// The point of the face the player looks at, from the head bone: forward and up, in the
/// skeleton's units [design; the eyes are about 9 forward and 7 up, `player_body.rs`].
pub const FACE_FORWARD: f32 = 5.0;
pub const FACE_UP: f32 = 6.0;
/// The animation's head movement kept under the turn: the turn aims the head's slow
/// average direction at the player, the average following the animation over this many
/// seconds, so a talk clip's nods and glances (0.3 to 1 s) still show [design; the
/// original's Sunny Smiles nods and sways on her body while she faces the player,
/// lipsync.md §5.1].
pub const KEEP_SECONDS: f32 = 0.8;
/// At most this much of the animation's own head movement is kept, each way, radians
/// [design].
pub const KEEP_LIMIT: f32 = 20.0 * PI / 180.0;

/// The body part flag that allows head tracking, and the head bone's name when the data
/// names none.
const HEADTRACKING_FLAG: &str = "IK Data - Headtracking";
const HEAD_BONE: &str = "Bip01 Head";

/// On an actor's root: its body must not be turned to face anyone (it sits or lies in
/// furniture; inserted and removed by `furniture.rs`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BodyHeld;

// ---- the turn (pure) ------------------------------------------------------------------

/// Yaw (clockwise seen from above, 0 = the actor's forward, +y) and pitch (up positive)
/// of a direction in the actor's own space (x right, y forward, z up).
pub fn yaw_pitch(d: Vec3) -> (f32, f32) {
    (d.x.atan2(d.y), d.z.atan2(d.x.hypot(d.y)))
}

/// The rotation, in the actor's space, that takes its forward to that yaw and pitch
/// without rolling.
pub fn look_quat(yaw: f32, pitch: f32) -> Quat {
    Quat::from_rotation_z(-yaw) * Quat::from_rotation_x(pitch)
}

/// The yaw towards a target, on the side it was on before (`before`: this function's
/// result last frame) while the target is behind the actor ([`BEHIND`]), so the head does
/// not flip from one shoulder to the other.
pub fn sided(yaw: f32, before: Option<f32>) -> f32 {
    let yaw = wrap(yaw);
    match before {
        Some(b) if yaw.abs() > BEHIND && b.abs() > 1e-3 && b.signum() != yaw.signum() => b.signum() * yaw.abs(),
        _ => yaw,
    }
}

/// Where the bone above the neck faces, as a yaw in the actor's space: `frame` is how
/// that bone is turned from its bind pose (in which it faces the actor's +y). Root
/// forward (0) when the bone points nearly straight up or down and has no heading
/// [design].
pub fn torso_yaw(frame: Quat) -> f32 {
    let f = frame * Vec3::Y;
    if f.x.hypot(f.y) < 0.25 {
        0.0
    } else {
        yaw_pitch(f).0
    }
}

/// [`limited`] with the yaw measured from `torso` (the actor's space, [`torso_yaw`]):
/// `want`'s yaw is given against the torso and the result is back in the actor's space.
pub fn limited_from(want: (f32, f32), torso: f32, yaw_limit: f32) -> (f32, f32) {
    let (yaw, pitch) = limited(want, yaw_limit);
    (wrap(torso + yaw), pitch)
}

/// The yaw and pitch the head may take towards a target at `want`: within `yaw_limit` to
/// either side and [`HEAD_PITCH_LIMIT`] up and down.
pub fn limited(want: (f32, f32), yaw_limit: f32) -> (f32, f32) {
    (want.0.clamp(-yaw_limit, yaw_limit), want.1.clamp(-HEAD_PITCH_LIMIT, HEAD_PITCH_LIMIT))
}

/// The animation's head direction averaged over [`KEEP_SECONDS`]: `rest` (last frame's
/// average, none at the start) moved towards `cur` (this frame's) for `dt` seconds.
pub fn follow_rest(rest: Option<(f32, f32)>, cur: (f32, f32), dt: f32) -> (f32, f32) {
    match rest {
        Some(r) => {
            let k = 1.0 - (-dt.max(0.0) / KEEP_SECONDS).exp();
            (wrap(r.0 + wrap(cur.0 - r.0) * k), r.1 + (cur.1 - r.1) * k)
        }
        None => cur,
    }
}

/// Where the head is aimed: `target` (on the player, limited) moved by the animation's
/// head movement away from its average `rest` ([`KEEP_LIMIT`] at most), so that movement
/// stays under the turn.
pub fn keep_motion(target: (f32, f32), cur: (f32, f32), rest: (f32, f32)) -> (f32, f32) {
    let keep = |d: f32| d.clamp(-KEEP_LIMIT, KEEP_LIMIT);
    (target.0 + keep(wrap(cur.0 - rest.0)), target.1 + keep(cur.1 - rest.1))
}

/// The rotations (in the actor's space) of the chain's bones for a head whose forward is
/// at `cur` and should be `share` of the way to `target`: `shares[i]` of that for bone
/// `i` (the head, last, has 1).
pub fn chain_turns(cur: (f32, f32), target: (f32, f32), share: f32, shares: &[f32]) -> Vec<Quat> {
    let d = (wrap(target.0 - cur.0), target.1 - cur.1);
    let back = look_quat(cur.0, cur.1).inverse();
    shares.iter().map(|k| look_quat(cur.0 + d.0 * share * k, cur.1 + d.1 * share * k) * back).collect()
}

/// A bone's new local rotation for a turn `turn` of the bone in the actor's space, when
/// its parent (rotated `parent` in the actor's space by the animation) was itself turned
/// by `parent_turn`.
pub fn local_turn(local: Quat, parent: Quat, parent_turn: Quat, turn: Quat) -> Quat {
    (parent.inverse() * parent_turn.inverse() * turn * parent * local).normalize()
}

/// How much of the turn each bone is turned by, for `necks` neck bones and the head: the
/// neck bones by evenly growing shares up to [`NECK_SHARE`], the head by all of it.
pub fn shares(necks: usize) -> Vec<f32> {
    let mut s: Vec<f32> = (1..=necks).map(|i| NECK_SHARE * i as f32 / necks as f32).collect();
    s.push(1.0);
    s
}

/// A body turning on the spot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyTurn {
    /// The root's rotation when the turn started.
    pub start: Quat,
    /// The whole turn, radians, clockwise seen from above.
    pub total: f32,
    pub t: f32,
    pub seconds: f32,
}

impl BodyTurn {
    /// The turn for a body `off` radians (clockwise) away from facing the player, when
    /// that is far enough to turn at all.
    pub fn plan(start: Quat, off: f32) -> Option<BodyTurn> {
        let off = wrap(off);
        (off.abs() > BODY_TURN_FROM).then(|| BodyTurn { start, total: off, t: 0.0, seconds: (off.abs() / BODY_TURN_SPEED).clamp(BODY_TURN_MIN_SECONDS, BODY_TURN_MAX_SECONDS) })
    }

    pub fn done(&self) -> bool {
        self.t >= self.seconds
    }

    /// How far it has turned, radians.
    pub fn turned(&self) -> f32 {
        self.total * smooth(self.t / self.seconds.max(1e-6))
    }

    /// Advance by `dt` (or to the end): the root's rotation, and the turn rate for the
    /// frame as `Locomotion::turn` wants it (radians per second, positive = left).
    pub fn step(&mut self, dt: f32, instant: bool) -> (Quat, f32) {
        let before = self.turned();
        self.t = if instant { self.seconds } else { (self.t + dt.max(0.0)).min(self.seconds) };
        let now = self.turned();
        let rate = if dt > 0.0 && !instant { -(now - before) / dt } else { 0.0 };
        (self.start * Quat::from_rotation_z(-now), rate)
    }
}

// ---- the actors -------------------------------------------------------------------------

/// The bones turned on an actor.
#[derive(Debug, Clone)]
struct HeadRig {
    /// Skeleton node indices from the topmost neck bone down to the head, and each one's
    /// share of the turn.
    chain: Vec<(usize, f32)>,
    /// The head's forward and up axes in its own space.
    forward: Vec3,
    up: Vec3,
    /// The bind rotation (actor space) of the bone above the chain, the one the yaw limit
    /// is measured from.
    frame_bind: Quat,
    yaw_limit: f32,
    /// For the log: the bones' names, and whether the limit is the body part data's.
    bones: String,
    limit_from_data: bool,
}

/// What turns on an actor, for the log.
#[derive(Debug, Clone, PartialEq)]
enum Turns {
    HeadAndBody,
    HeadOnly(&'static str),
    Nobody(String),
}

/// One actor facing the player, or easing back.
struct Look {
    entity: Entity,
    name: String,
    head: Option<HeadRig>,
    /// The conversation is open.
    active: bool,
    /// 0 (the animation's own pose) to 1 (on the player), before [`smooth`].
    weight: f32,
    /// The yaw wanted last frame against the torso, before the limit ([`sided`]).
    wanted_yaw: Option<f32>,
    /// The yaw and pitch the head had last frame (the actor's space).
    shown: (f32, f32),
    /// The animation's head direction averaged over [`KEEP_SECONDS`] ([`follow_rest`]).
    rest: Option<(f32, f32)>,
    /// Per bone of the chain: the local rotation the animation gave it and the one written
    /// over it last frame. A bone still holding what was written was not posed again
    /// since (its clips have no track for it, or the actor is not animated): its
    /// animation pose is still the remembered one.
    written: Vec<Option<(Quat, Quat)>>,
    body: Option<BodyTurn>,
}

impl Look {
    fn new(entity: Entity, name: String, head: Option<HeadRig>, weight: f32, body: Option<BodyTurn>) -> Look {
        let written = vec![None; head.as_ref().map_or(0, |h| h.chain.len())];
        Look { entity, name, head, active: true, weight, wanted_yaw: None, shown: (0.0, 0.0), rest: None, written, body }
    }
}

/// The local rotation the animation gave a bone that now has `now`: `now` itself, unless
/// the bone still holds what was written over it last frame (`written`: the animation's
/// rotation then, and what was written).
pub fn animated_rotation(now: Quat, written: Option<(Quat, Quat)>) -> Quat {
    match written {
        Some((base, w)) if now.abs_diff_eq(w, 1e-6) => base,
        _ => now,
    }
}

/// The actors facing the player or easing back, by reference.
#[derive(Resource, Default)]
pub struct Looks {
    by_ref: HashMap<u32, Look>,
    /// The conversation's actor that is not drawn (said once).
    missing: Option<u32>,
    /// `NVF_PLAY_TEST_TALK_HEAD_ONLY`: no body turns.
    head_only: bool,
}

impl Looks {
    /// Whether a conversation turns this reference's head now (facing the player, or
    /// easing back): head tracking (`head_look.rs`) keeps off the head meanwhile.
    pub fn turns_head(&self, reference: u32) -> bool {
        self.by_ref.get(&reference).is_some_and(|l| l.head.is_some())
    }
}

pub struct DialogueLookPlugin;

impl Plugin for DialogueLookPlugin {
    fn build(&self, app: &mut App) {
        // After the frame's animation has posed the skeletons, and after the play
        // systems (the conversation's state is this frame's).
        let head_only = nvf_play::dialogue::debug_switch(std::env::var("NVF_PLAY_TEST_TALK_HEAD_ONLY").ok().as_deref());
        if head_only {
            info!("test aid NVF_PLAY_TEST_TALK_HEAD_ONLY: no actor turns its body to the player");
        }
        app.insert_resource(Looks { head_only, ..default() }).add_systems(Update, turn_speaker.after(crate::locomotion::drive).after(PlaySet::Ui));
    }
}

/// What [`turn_speaker`] reads of an actor.
type Actor = (Entity, &'static ActorRig, &'static ChildOf, Has<Dying>, Has<Corpse>, Has<WalkTest>, Has<BodyHeld>);

/// A bone's transform in the actor's space as its entities have it (the root's own
/// transform, the actor's placement, left out).
fn in_actor_space(rig: &ActorRig, i: usize, transforms: &Query<&mut Transform>) -> Option<Affine3A> {
    let mut path = Vec::new();
    let mut n = i;
    while n != 0 {
        path.push(n);
        n = rig.skel.parents.get(n).copied()?.unwrap_or(0);
        if path.len() > rig.skel.parents.len() {
            return None;
        }
    }
    let mut m = Affine3A::IDENTITY;
    for &n in path.iter().rev() {
        m *= transforms.get(*rig.nodes.get(n)?).ok()?.compute_affine();
    }
    Some(m)
}

fn rotation(m: &Affine3A) -> Quat {
    m.to_scale_rotation_translation().1
}

/// The bones to turn on a human-style skeleton, or why none.
fn head_rig(rig: &ActorRig, combat: Option<&Combat>) -> Result<HeadRig, String> {
    // The body part that head tracking is set up on, when the actor has body part data.
    let data = combat.and_then(|c| c.forms.body_parts_for(FormId(rig.body), &rig.skeleton).and_then(|d| c.forms.body_parts.get(&d)));
    let part = data.and_then(|d| d.parts.iter().find(|p| p.flags.iter().any(|f| f == HEADTRACKING_FLAG)));
    if data.is_some() && part.is_none() {
        return Err("its body part data allows no head tracking".into());
    }
    let bone = part.and_then(|p| p.ik_start_node.as_deref()).unwrap_or(HEAD_BONE);
    let find = |n: &str| rig.skel.names.iter().position(|x| x.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(n)));
    let head = find(bone).or_else(|| find(HEAD_BONE)).filter(|&i| i > 0).ok_or_else(|| format!("its skeleton has no {bone}"))?;
    // The neck bones right above the head.
    let mut necks = Vec::new();
    let mut n = head;
    while let Some(p) = rig.skel.parents.get(n).copied().flatten().filter(|&p| p > 0) {
        if necks.len() == 2 || !rig.skel.names[p].as_deref().is_some_and(|s| s.to_ascii_lowercase().contains("neck")) {
            break;
        }
        necks.push(p);
        n = p;
    }
    necks.reverse();
    let chain: Vec<(usize, f32)> = necks.iter().copied().chain([head]).zip(shares(necks.len())).collect();
    // In the bind pose the actor faces +y with z up: the head's own axes for those.
    let bind = rotation(rig.skel.bind_world.get(head).ok_or("no bind pose")?).inverse();
    let top = rig.skel.parents.get(chain[0].0).copied().flatten().unwrap_or(0);
    let frame_bind = if top == 0 { Quat::IDENTITY } else { rotation(rig.skel.bind_world.get(top).ok_or("no bind pose")?) };
    let asked = part.map(|p| p.tracking_max_angle.to_radians()).filter(|a| a.is_finite() && *a > 1e-3);
    let bones = chain.iter().map(|c| rig.skel.names[c.0].as_deref().unwrap_or("?")).collect::<Vec<_>>().join(", ");
    Ok(HeadRig {
        chain,
        forward: bind * Vec3::Y,
        up: bind * Vec3::Z,
        frame_bind,
        yaw_limit: asked.unwrap_or(HEAD_YAW_LIMIT).min(HEAD_YAW_LIMIT_MAX),
        bones,
        limit_from_data: asked.is_some(),
    })
}

/// The compass heading (degrees) a root's rotation faces.
pub(crate) fn heading_deg(rotation: Quat) -> f32 {
    let f = rotation * Vec3::Y;
    f.x.atan2(f.y).to_degrees().rem_euclid(360.0)
}

/// Turn the conversation's actor to the player, and ease back the ones whose
/// conversation is over (the module notes).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn turn_speaker(
    time: Res<Time>,
    settings: Res<LookSettings>,
    mode: Res<MenuPause>,
    paused: Res<Paused>,
    script: Option<Res<ScriptState>>,
    sim: Res<SimState>,
    player: Option<Res<Player>>,
    (combat, nav, residents): (Option<Res<Combat>>, Option<Res<NavState>>, Option<Res<crate::residents::Residents>>),
    mut looks: ResMut<Looks>,
    mut face: ResMut<SpeakerFace>,
    actors: Query<Actor>,
    heads: Query<&HeadLook>,
    mut locos: Query<&mut Locomotion>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    let game = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    // An actor talking through an activator is not there to turn (`fnv_game::talking_activators`).
    let talking = game.and_then(|g| g.dialogue.as_ref()).filter(|c| c.through.is_none()).map(|c| c.actor.0).filter(|_| settings.enabled);
    let Some(player) = player else { return };
    let dt = time.delta_secs();
    // With `--menu-pause original` the picture stands still during a conversation, so a
    // turn could not be seen: it is completed at once. The pause menu holds everything
    // (no virtual time passes).
    let instant = talking.is_some() && *mode == MenuPause::Original && !paused.0;
    let looks = &mut *looks;

    // A conversation's actor starts facing the player.
    if let Some(r) = talking.filter(|r| !looks.by_ref.get(r).is_some_and(|l| l.active)) {
        let name = game.map_or_else(|| format!("{r:08X}"), |g| g.data.name(FormId(r)));
        match actors.iter().find(|a| a.1.reference == r) {
            None => {
                // Not drawn (or no actor at all): the view finds it by other means.
                looks.by_ref.remove(&r);
                if looks.missing.replace(r) != Some(r) {
                    info!("dialogue look: {name} is not drawn as an actor: nobody turns to the player");
                }
            }
            Some((entity, rig, child_of, dying, corpse, walk_test, held)) => {
                let npc = sim.refs.get(FormId(r)).is_some_and(|p| &p.sig == b"ACHR");
                let dead = dying || corpse || is_dead(script.as_deref(), r);
                let head = if dead {
                    Err("it is dead".to_string())
                } else if !npc {
                    Err("it is not a human-style NPC (a creature, a robot or an activator)".to_string())
                } else {
                    head_rig(rig, combat.as_deref())
                };
                let lo = locos.get(entity).ok();
                let traveller = nav.as_ref().is_some_and(|n| n.travellers.iter().any(|t| t.reference == r));
                // Without locomotion (the bind pose, `--actor-pose bind`) nothing moves it.
                let standing = lo.is_none_or(|lo| lo.blend.cur.mv == Move::Idle && lo.velocity == [0.0; 2]);
                let turns = match &head {
                    Err(why) => Turns::Nobody(why.clone()),
                    Ok(_) if held => Turns::HeadOnly("it is in furniture"),
                    Ok(_) if looks.head_only => Turns::HeadOnly("test aid NVF_PLAY_TEST_TALK_HEAD_ONLY"),
                    Ok(_) if traveller => Turns::HeadOnly("its heading is its route's (--npc-travel)"),
                    Ok(_) if residents.as_ref().is_some_and(|res| res.runs(r)) => Turns::HeadOnly("the AI turns its body to the player (fnv_ai)"),
                    Ok(_) if walk_test => Turns::HeadOnly("the walk test moves it"),
                    Ok(_) if !standing => Turns::HeadOnly("it is not standing in its idle"),
                    Ok(_) => Turns::HeadAndBody,
                };
                // How far the body is from facing the player.
                let root = transforms.get(entity).ok().copied();
                let parent = globals.get(child_of.parent()).ok().map(|g| g.affine());
                let off = root.zip(parent).map(|(root, parent)| {
                    let to_actor = (parent * root.compute_affine()).inverse();
                    yaw_pitch(to_actor.transform_point3(space::to_bevy(player.feet))).0
                });
                let body = match (&turns, root, off) {
                    (Turns::HeadAndBody, Some(root), Some(off)) => BodyTurn::plan(root.rotation, off),
                    _ => None,
                };
                // The head's reach and bones, for the log.
                let reach = head.as_ref().map_or_else(
                    |_| String::new(),
                    |h| {
                        format!(
                            "the head turns up to {:.0}° to a side ({}) and {:.0}° up or down, on {}",
                            h.yaw_limit.to_degrees(),
                            if h.limit_from_data { "the Tracking Max Angle of its body part data" } else { "NVForever's own limit: no body part data" },
                            HEAD_PITCH_LIMIT.to_degrees(),
                            h.bones
                        )
                    },
                );
                let off_deg = off.map_or(f32::NAN, f32::to_degrees);
                match (&turns, &body) {
                    (Turns::Nobody(why), _) => info!("dialogue look: {name} does not turn to the player: {why}"),
                    (Turns::HeadOnly(why), _) => {
                        info!("dialogue look: {name} faces the player with its head only ({why}): the player is {off_deg:+.0}° off its front; {reach}")
                    }
                    (Turns::HeadAndBody, Some(b)) => info!(
                        "dialogue look: {name} faces the player with head and body (standing in its idle): the player is {off_deg:+.0}° off its front, the body turns {:+.0}° in {:.2} s from heading {:.0}°; {reach}",
                        b.total.to_degrees(),
                        b.seconds,
                        root.map_or(f32::NAN, |t| heading_deg(t.rotation)),
                    ),
                    (Turns::HeadAndBody, None) => info!(
                        "dialogue look: {name} faces the player with its head (standing in its idle; the body stays: the player is {off_deg:+.0}° off its front, within {:.0}°); {reach}",
                        BODY_TURN_FROM.to_degrees()
                    ),
                }
                // An actor still easing back from an earlier conversation goes on from where
                // its head is (a bone it still holds keeps its remembered animation pose).
                let before = looks.by_ref.remove(&r).filter(|l| l.entity == entity && l.head.is_some() && head.is_ok());
                // Without a head to turn the entry only says where the view may look.
                // A head that head tracking (`head_look.rs`) already had on the player goes
                // on from there: it hands the head over for the conversation.
                let on_player = heads.get(entity).is_ok_and(|h| h.target() == Some(LookAt::Player));
                let weight = before.as_ref().map_or(if on_player { 1.0 } else { 0.0 }, |l| l.weight);
                let mut look = Look::new(entity, name, head.ok(), weight, body);
                if let Some(b) = before.filter(|b| b.written.len() == look.written.len()) {
                    look.written = b.written;
                }
                looks.by_ref.insert(r, look);
            }
        }
    }
    if talking.is_none() {
        looks.missing = None;
    }

    let mut said = None;
    let mut over = Vec::new();
    for (&r, look) in looks.by_ref.iter_mut() {
        let Ok((_, rig, child_of, dying, corpse, _, _)) = actors.get(look.entity) else {
            // No longer drawn (its cell went).
            over.push(r);
            continue;
        };
        if look.active && talking != Some(r) {
            look.active = false;
            if look.head.is_some() {
                let heading = transforms.get(look.entity).map_or(f32::NAN, |t| heading_deg(t.rotation));
                info!(
                    "dialogue look: {} stops facing the player: the head (yaw {:+.0}°, pitch {:+.0}°) eases back over {HEAD_OUT_SECONDS:.2} s, the body keeps its heading {heading:.0}°",
                    look.name,
                    look.shown.0.to_degrees(),
                    look.shown.1.to_degrees()
                );
            }
        }
        // A dead actor is its death pose's or its ragdoll's: never touched.
        if dying || corpse || is_dead(script.as_deref(), r) {
            if look.body.take().is_some() {
                if let Ok(mut lo) = locos.get_mut(look.entity) {
                    lo.turn = 0.0;
                }
            }
            look.head = None;
            if !look.active {
                over.push(r);
            }
            continue;
        }
        let Some(root_local) = transforms.get(look.entity).ok().copied() else { continue };
        let Ok(parent) = globals.get(child_of.parent()).map(|g| g.affine()) else { continue };

        // The body.
        let mut root_local = root_local;
        if let Some(b) = look.body.as_mut() {
            let (rot, rate) = b.step(dt, instant);
            root_local.rotation = rot;
            if let Ok(mut t) = transforms.get_mut(look.entity) {
                t.rotation = rot;
            }
            let done = b.done();
            if let Ok(mut lo) = locos.get_mut(look.entity) {
                lo.turn = if done { 0.0 } else { rate };
            }
            if done {
                debug!("dialogue look: {} has turned its body to heading {:.0}°", look.name, heading_deg(rot));
                look.body = None;
            }
        }
        let root = parent * root_local.compute_affine();

        // The head.
        let Some(head) = look.head.as_ref() else {
            // Nobody turns: only say where the head is, if there is one.
            if look.active {
                let at = rig.skel.by_name.get(HEAD_BONE).and_then(|&i| rig.nodes.get(i)).and_then(|&e| globals.get(e).ok());
                said = at.map(|g| (r, space::to_game(g.translation())));
            } else {
                over.push(r);
            }
            continue;
        };
        let step = if instant {
            1.0
        } else if look.active {
            dt / HEAD_IN_SECONDS
        } else {
            -dt / HEAD_OUT_SECONDS
        };
        look.weight = (look.weight + step).clamp(0.0, 1.0);
        // The chain as the animation has it this frame: above it the bones are untouched;
        // along it, a bone this system wrote last frame and nothing posed since still has
        // its remembered animation rotation.
        let top = rig.skel.parents.get(head.chain[0].0).copied().flatten().unwrap_or(0);
        let Some(above_m) = in_actor_space(rig, top, &transforms) else { continue };
        // (A bone head tracking wrote last frame and nothing posed since has the animation
        // rotation head tracking remembers.)
        let tracked = heads.get(look.entity).ok();
        let locals: Option<Vec<Transform>> = head
            .chain
            .iter()
            .zip(&look.written)
            .map(|(&(i, _), w)| {
                let t = transforms.get(*rig.nodes.get(i)?).ok().copied()?;
                let now = tracked.map_or(t.rotation, |h| h.animated(i, t.rotation));
                Some(Transform { rotation: animated_rotation(now, *w), ..t })
            })
            .collect();
        let Some(locals) = locals else { continue };
        // Each bone's parent in the actor's space, and the head itself.
        let mut parents = Vec::with_capacity(locals.len());
        let mut head_m = above_m;
        for l in &locals {
            parents.push(rotation(&head_m));
            head_m *= l.compute_affine();
        }
        let head_rot = rotation(&head_m);
        let cur = yaw_pitch(head_rot * head.forward);
        let eye = root.inverse().transform_point3(space::to_bevy(player.eye()));
        let want = yaw_pitch(eye - Vec3::from(head_m.translation));
        // The limit is the neck's: measured from where the chest faces, whatever the
        // animation does with the body below it.
        let torso = torso_yaw(rotation(&above_m) * head.frame_bind.inverse());
        let yaw = sided(wrap(want.0 - torso), look.wanted_yaw);
        look.wanted_yaw = Some(yaw);
        let target = limited_from((yaw, want.1), torso, head.yaw_limit);
        // The animation's own nods and glances stay under the turn (a frozen picture has
        // none to keep).
        let rest = if instant { cur } else { follow_rest(look.rest, cur, dt) };
        look.rest = Some(rest);
        let target = keep_motion(target, cur, rest);
        let share = smooth(look.weight);
        let shares: Vec<f32> = head.chain.iter().map(|c| c.1).collect();
        let turns = chain_turns(cur, target, share, &shares);
        let mut above = Quat::IDENTITY;
        for (k, &(i, _)) in head.chain.iter().enumerate() {
            if let Some(Ok(mut t)) = rig.nodes.get(i).map(|e| transforms.get_mut(*e)) {
                let turned = local_turn(locals[k].rotation, parents[k], above, turns[k]);
                t.rotation = turned;
                look.written[k] = Some((locals[k].rotation, turned));
            }
            above = turns[k];
        }
        let turned = above * head_rot;
        look.shown = yaw_pitch(turned * head.forward);
        if look.active {
            let point = Vec3::from(head_m.translation) + turned * (head.forward * FACE_FORWARD + head.up * FACE_UP);
            said = Some((r, space::to_game(root.transform_point3(point))));
        } else if look.weight <= 0.0 && look.body.is_none() {
            over.push(r);
        }
    }
    for r in over {
        looks.by_ref.remove(&r);
    }
    // With the turning switched off nothing above says where the face is, and the
    // conversation's zoom (`nvf_play::dialogue_zoom`) still wants its distance.
    if !settings.enabled {
        let speaker = game.and_then(|g| g.dialogue.as_ref()).filter(|c| c.through.is_none()).map(|c| c.actor.0);
        said = speaker.and_then(|r| {
            let rig = actors.iter().find(|a| a.1.reference == r)?.1;
            let at = globals.get(*rig.nodes.get(*rig.skel.by_name.get(HEAD_BONE)?)?).ok()?;
            Some((r, space::to_game(at.translation())))
        });
    }
    face.set_if_neq(SpeakerFace(said));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn yaw_is_clockwise_from_the_front_and_pitch_is_up() {
        let (y, p) = yaw_pitch(Vec3::new(0.0, 5.0, 0.0));
        assert!(y.abs() < 1e-6 && p.abs() < 1e-6);
        // To the actor's right; behind; up.
        assert!((yaw_pitch(Vec3::X).0 - FRAC_PI_2).abs() < 1e-6);
        assert!((yaw_pitch(Vec3::NEG_Y).0.abs() - PI).abs() < 1e-6);
        assert!((yaw_pitch(Vec3::new(0.0, 1.0, 1.0)).1 - PI / 4.0).abs() < 1e-6);
        // The rotation built from them takes the front there, and keeps the head level.
        for (yaw, pitch) in [(0.7f32, 0.3f32), (-2.0, -0.4), (3.0, 0.0)] {
            let q = look_quat(yaw, pitch);
            let (y, p) = yaw_pitch(q * Vec3::Y);
            assert!((wrap(y - yaw)).abs() < 1e-5 && (p - pitch).abs() < 1e-5, "{yaw} {pitch}: {y} {p}");
            assert!((q * Vec3::X).z.abs() < 1e-6, "no roll: the ear-to-ear axis stays level");
        }
    }

    #[test]
    fn the_neck_never_twists_past_its_limit() {
        let limit = 60f32.to_radians();
        // Within reach: looked at exactly.
        assert_eq!(limited((0.5, -0.2), limit), (0.5, -0.2));
        // To the side and behind: as far as the limit, on the player's side.
        assert_eq!(limited((2.0, 0.0), limit).0, limit);
        assert_eq!(limited((-2.0, 0.0), limit).0, -limit);
        // Up and down have their own limit.
        assert_eq!(limited((0.0, 1.2), limit).1, HEAD_PITCH_LIMIT);
        assert_eq!(limited((0.0, -1.2), limit).1, -HEAD_PITCH_LIMIT);
        // Straight behind: the head stays on the shoulder it was on, frame after frame.
        let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
        let first = sided(3.1, None);
        let second = sided(-3.1, Some(first));
        let third = sided(-3.1, Some(second));
        assert!(near(first, 3.1) && near(second, 3.1) && near(third, 3.1), "{first} {second} {third}");
        assert_eq!(limited((third, 0.0), limit).0, limit);
        assert!(near(sided(3.1, Some(-3.0)), -3.1));
        // But a player who is merely to the other side is followed there.
        assert!(near(sided(-1.0, Some(1.0)), -1.0));
        assert!(near(sided(4.0, None), wrap(4.0)));
    }

    #[test]
    fn the_limit_is_measured_from_the_chest_not_the_root() {
        let limit = 60f32.to_radians();
        // Upright, untwisted: the root's limit.
        assert!(torso_yaw(Quat::IDENTITY).abs() < 1e-6);
        // A cowering pose twists the chest 120° to the right of the root; the player
        // stands straight ahead of the root, 120° to the chest's left. The head stops 60°
        // to the chest's left (60° right of the root), not 60° to the root's side
        // (which is 180° from the chest: facing backwards).
        let chest = Quat::from_rotation_z(-120f32.to_radians());
        let torso = torso_yaw(chest);
        assert!((torso - 120f32.to_radians()).abs() < 1e-5, "{torso}");
        let (yaw, _) = limited_from((wrap(0.0 - torso), 0.0), torso, limit);
        assert!((yaw - 60f32.to_radians()).abs() < 1e-5, "{}", yaw.to_degrees());
        assert!(wrap(yaw - torso).abs() <= limit + 1e-5);
        // A chest bent straight down has no heading: the root's.
        assert_eq!(torso_yaw(Quat::from_rotation_x(FRAC_PI_2)), 0.0);
    }

    #[test]
    fn a_quick_nod_stays_under_the_turn_and_a_held_pose_does_not() {
        let target = (0.3, -0.1);
        let mut rest = follow_rest(None, (0.0, 0.0), 1.0 / 60.0);
        // 0.2 s of a nod 0.1 down: most of it still shows on the turned head.
        for _ in 0..12 {
            rest = follow_rest(Some(rest), (0.0, -0.1), 1.0 / 60.0);
        }
        let aim = keep_motion(target, (0.0, -0.1), rest);
        assert!(aim.1 < target.1 - 0.07 && (aim.0 - target.0).abs() < 1e-6, "{aim:?}");
        // Held for 5 s, the pose is the average again: the head is back on the target.
        for _ in 0..300 {
            rest = follow_rest(Some(rest), (0.0, -0.1), 1.0 / 60.0);
        }
        let aim = keep_motion(target, (0.0, -0.1), rest);
        assert!((aim.1 - target.1).abs() < 0.001, "{aim:?}");
        // A large swing is kept only up to the limit.
        assert_eq!(keep_motion(target, (1.0, 0.0), (0.0, 0.0)).0, target.0 + KEEP_LIMIT);
        // Across the back the yaw difference wraps.
        assert!((follow_rest(Some((3.1, 0.0)), (-3.1, 0.0), 10.0).0.abs() - 3.1).abs() < 0.01);
    }

    #[test]
    fn the_chain_shares_the_turn_and_the_head_ends_on_the_target() {
        assert_eq!(shares(0), vec![1.0]);
        assert_eq!(shares(1), vec![NECK_SHARE, 1.0]);
        assert_eq!(shares(2), vec![NECK_SHARE / 2.0, NECK_SHARE, 1.0]);
        // A head the animation holds a little left and down, a target up to the right.
        let (cur, target) = ((-0.1f32, -0.05f32), (0.9f32, 0.3f32));
        let sh = shares(2);
        let turns = chain_turns(cur, target, 1.0, &sh);
        let front = look_quat(cur.0, cur.1) * Vec3::Y;
        let (y, p) = yaw_pitch(turns[2] * front);
        assert!((y - target.0).abs() < 1e-5 && (p - target.1).abs() < 1e-5, "the head is on the target: {y} {p}");
        let (yn, _) = yaw_pitch(turns[1] * front);
        assert!((yn - (cur.0 + (target.0 - cur.0) * NECK_SHARE)).abs() < 1e-5, "the neck took its share: {yn}");
        // At no weight nothing turns.
        for q in chain_turns(cur, target, 0.0, &sh) {
            assert!(q.angle_between(Quat::IDENTITY) < 1e-5);
        }
        // Half way in, half the turn.
        let (y, _) = yaw_pitch(chain_turns(cur, target, 0.5, &sh)[2] * front);
        assert!((y - 0.4).abs() < 1e-5, "{y}");
    }

    #[test]
    fn bones_written_down_the_chain_end_where_they_should() {
        // A neck and a head with arbitrary animated poses under an arbitrary spine.
        let spine = Quat::from_euler(EulerRot::XYZ, 0.3, -0.2, 0.5);
        let neck_local = Quat::from_euler(EulerRot::XYZ, -0.1, 0.4, 0.2);
        let head_local = Quat::from_euler(EulerRot::XYZ, 0.2, 0.1, -0.3);
        let (neck, head) = (spine * neck_local, spine * neck_local * head_local);
        let turn_neck = look_quat(0.3, 0.1);
        let turn_head = look_quat(0.8, 0.2);
        let new_neck_local = local_turn(neck_local, spine, Quat::IDENTITY, turn_neck);
        let new_head_local = local_turn(head_local, neck, turn_neck, turn_head);
        // In the actor's space each bone is its animated pose turned by its own turn.
        let got_neck = spine * new_neck_local;
        let got_head = got_neck * new_head_local;
        for v in [Vec3::X, Vec3::Y, Vec3::Z] {
            assert!(close(got_neck * v, turn_neck * neck * v));
            assert!(close(got_head * v, turn_head * head * v));
        }
        // No turn leaves the animation's pose.
        assert!(local_turn(head_local, neck, Quat::IDENTITY, Quat::IDENTITY).angle_between(head_local) < 1e-5);
    }

    #[test]
    fn a_bone_nothing_posed_again_keeps_its_remembered_animation_pose() {
        let (anim, wrote) = (Quat::from_rotation_x(0.2), Quat::from_rotation_z(0.7));
        // Posed again by the animation since: its new rotation is the animation's.
        let posed = Quat::from_rotation_x(0.25);
        assert_eq!(animated_rotation(posed, Some((anim, wrote))), posed);
        // Still what was written: the turn is not applied on top of itself.
        assert_eq!(animated_rotation(wrote, Some((anim, wrote))), anim);
        assert_eq!(animated_rotation(posed, None), posed);
    }

    #[test]
    fn a_body_turns_on_the_spot_only_when_it_faces_far_from_the_player() {
        let start = Quat::from_rotation_z(-1.0); // heading 1 rad
        assert!(BodyTurn::plan(start, 20f32.to_radians()).is_none(), "the head alone reaches");
        let mut b = BodyTurn::plan(start, 150f32.to_radians()).expect("turns");
        assert!((b.seconds - 1.0).abs() < 1e-5, "150° at 150°/s");
        assert!((BodyTurn::plan(start, -40f32.to_radians()).unwrap().seconds - BODY_TURN_MIN_SECONDS).abs() < 1e-6);
        assert!((BodyTurn::plan(start, 3.5).unwrap().total - wrap(3.5)).abs() < 1e-6, "the short way round");
        // Clockwise: `Locomotion::turn` is negative (right), the heading grows, eased.
        let mut rates = Vec::new();
        let mut rot = start;
        while !b.done() {
            let (r, rate) = b.step(0.05, false);
            rot = r;
            rates.push(rate);
        }
        assert!(rates.iter().all(|r| *r <= 1e-6));
        assert!(rates[0].abs() < rates[rates.len() / 2].abs() / 3.0, "it starts gently: {rates:?}");
        let f = rot * Vec3::Y;
        assert!((wrap(f.x.atan2(f.y) - (1.0 + 150f32.to_radians()))).abs() < 1e-4, "it ends facing the player");
        // A frozen picture gets the end at once, with no turn clip.
        let mut b = BodyTurn::plan(start, -2.0).unwrap();
        let (r, rate) = b.step(0.0, true);
        assert!(b.done() && rate == 0.0);
        let f = r * Vec3::Y;
        assert!((wrap(f.x.atan2(f.y) - (1.0 - 2.0))).abs() < 1e-4);
        // Under the pause menu (no time passes) it holds.
        let mut b = BodyTurn::plan(start, -2.0).unwrap();
        b.step(0.3, false);
        let before = b.turned();
        b.step(0.0, false);
        assert_eq!(b.turned(), before);
    }
}
