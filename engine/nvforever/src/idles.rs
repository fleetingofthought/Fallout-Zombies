//! Idle animations played (docs/spec-notes/idles.md): what the game's rules ask for
//! (`fnv_game::idles`: a script's `PlayIdle`, a package's begin, end and change idles,
//! the idles on a dialogue response) and the idle collections of packages and idle
//! markers, on the actors drawn and on the player.
//!
//! - **Requests.** Each frame the game's `idle_requests` are taken and started: the
//!   idle's `.kf` is loaded, and how often it loops is rolled from its record
//!   (`IdleDef::loops`). An actor that is not drawn plays nothing.
//! - **Collections.** An actor standing where its package has it, with nothing else to
//!   do (`Residents::standing`), plays the idles of its package's collection, or of the
//!   idle marker it stands at (which override the package's [documented]), picked by
//!   `fnv_game::idles::IdlePicker`; so does the player under a script package. The pick
//!   is asked of the game like any other idle, so it is traced. A collection idle ends
//!   when its actor walks off.
//! - **Playing.** `fnv_actor::idle_play::IdlePlay` keeps each idle's time, loops and
//!   fades. On an NPC the clip is laid over what it plays otherwise (its locomotion, or
//!   its furniture clip) by track priority, on two more blend layers ([`ActorIdle`],
//!   read by `locomotion::drive`): the idle, and the one before it while the new one
//!   fades in. On the player ([`PlayerIdles`], read by `player_body.rs`) an idle whose
//!   clip drives the first-person camera node plays on the first-person rig and turns
//!   the view; any other plays on the third-person body.
//! - **Ends.** A one-shot idle ends at the end of its clip. An idle that loops for ever
//!   is replaced by the next idle asked for, and leaves its loop when the package (or
//!   marker) that asked for it is no longer the actor's.
//! - The rules are told what each actor plays (`Game::set_idle_playing`), which is what
//!   `IsIdlePlaying` answers.
//! - The slot also plays the animation groups scripts ask of actors (`PlayGroup`,
//!   `actor_groups.rs`); those are not idles for `IsIdlePlaying`.
//!
//! - **Drawn-weapon fidgets** (idles.md §11). An NPC standing still with its weapon out
//!   (a rifle loitering, lifting it one-handed to rest the arm, shifting its weight; an
//!   automatic rifle scanning), and the player standing still in third person with a
//!   weapon out and not aiming or firing, is checked every [`FIDGET_CHECK_SECONDS`]
//!   for a fidget from the IDLE tree (`Game::fidget`); the tree's own random
//!   percentages decide whether one plays. A fidget ends when its actor moves, puts the
//!   weapon away, or (the player) aims, fires, reloads or goes to first person.
//! - **Fidgets on furniture** (furniture.md §8). An actor on a chair, a bench, a bed or
//!   other furniture (`GetSitting` or `GetSleeping` 3: not getting on or off) is
//!   checked the same way for a fidget from the tree's furniture branch (a seated man
//!   relaxing, a sleeper stirring, a dancer, a wall leaner); so is the player on
//!   furniture in third person. It plays over the furniture's loop and ends (fades
//!   out) when its actor starts getting off.
//!
//! - **Talking gestures** (idles.md §12). An NPC in a conversation (with the player
//!   in the dialogue screen, or with another actor) is given a gesture from the IDLE
//!   tree's dialogue groups (`Game::talk_gesture`) when it starts saying a response,
//!   when it stops (it listens), and whenever the gesture it plays ends while it is
//!   still in the same part; the tree chooses talking or listening, the mood, sitting
//!   or standing. A response's own idle (INFO `SNAM`/`LNAM`) or any other idle asked
//!   for comes first: no gesture is picked while one plays. A gesture ends when the
//!   conversation does, or when the actor walks.
//!
//! - **Cowering** (idles.md §13). An NPC fleeing from a fight that stands still (as
//!   when its flight begins, or at its flee spot) is checked every
//!   [`COWER_CHECK_SECONDS`] for the idle the tree plays while fleeing
//!   (`Game::flee_idle`: `CowerIdles`, `3rdP_Cowering.kf`, by its own conditions
//!   IsMoving 0 and GetCurrentAIProcedure 16), before its fidgets. Started, it is not
//!   stopped when its actor runs: its arms and head (priority 33) are laid over the
//!   unarmed run (30), whose legs win over its own (23), so it runs with its hands over
//!   its head; standing, over the crouched set (20) the rules give a person at its flee
//!   spot, the whole cower shows [observed: the clips' priorities; owner 2026-10-17,
//!   "they'll cower whilst running but they only crouch when stopped"]. It ends when its
//!   actor no longer flees; after its loops (2 to 12 of 12 s) it is picked again when
//!   the actor next stands still. Flee Not Combat packages (procedure 34) are not
//!   carried out (`fnv_ai::goal`), so only combat flights cower.
//!
//! - **Panic** (pickpocket.md §5). An NPC with a live explosive planted in its pocket
//!   (`GetPlantedExplosive` 1: from `fProjectileInventoryGrenadeFreakoutTime` to the
//!   blast) is given the idle tree's panic idle once (`Game::panic_idle`: the roots whose
//!   conditions name GetPlantedExplosive, `PanicGrenadePlant.kf` or its power armour
//!   form); one whose folder has none (every creature) plays nothing. Either way it
//!   stands where it is until the blast (`residents.rs`).
//!
//! - **PickIdle**. A script's `PickIdle` asks for the tree's pick for its actor
//!   (`Game::idle_picks`); the actor's folder walked as for its fidgets, with no branch
//!   kept out (`Game::pick_idle`), and the pick played as a script's idle.
//!
//! - **Creatures' combat idles** (idles.md §14). A creature standing still with its
//!   stance drawn (`Game::draws_weapon`: fighting, alerted, or warning the player) is
//!   checked every [`FIDGET_CHECK_SECONDS`] for an idle from the tree's branch that
//!   names GetIsAlerted (the gecko's hiss and side hops, the mirelurk's and sentry bot's
//!   warnings); the tree's own conditions decide: the gecko's want alerted, combat or
//!   flee, so a gecko hisses standing in a fight and not while it warns (GetIsAlerted 0,
//!   procedure 44 then, as observed in the original). It ends when the creature moves
//!   or is calm again; an attack stops it as it stops any idle (`attack_anims.rs`).
//!   People keep their drawn-weapon fidgets [design].
//!
//! - **Stimpaks** (`npc_stimpaks.rs`). An NPC's stimpak idle plays on its upper body
//!   only, and over the fight's aim and attacks: it walks and runs while it injects.
//!
//! Not done: the rest of the idle manager (idles chosen from the IDLE tree by their
//! conditions: other fidgets), the sounds and other text keys of idle clips,
//! and package idles of actors that are not residents. The props idles hold (ANIO) are
//! drawn by `anim_objects.rs`.

use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;
use crate::residents::Residents;
use crate::weapons::{HeldWeapon, WeaponHold};
use bevy::prelude::*;
use fnv_actor::idle::meshes_path;
use fnv_actor::idle_play::{IdleKeys, IdlePlay};
use fnv_actor::weapon::Stage;
use fnv_game::game::Game;
use fnv_game::idles::{FidgetKind, IdlePicker, IdleRequest, IdleSource, Loops};
use fnv_game::package::flags;
use fnv_sim::FormId;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Clip, Layer};
use nvf_assets::model::ModelCtx;
use nvf_play::control::PlayerControl;
use nvf_play::player::Player;
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// The blend layers of an actor's idles: the one playing, then the one before it.
pub const IDLE_LAYER: usize = 4;

/// The first-person skeleton's camera node [observed: animation.md §9.5]. A clip with a
/// track for it is a first-person idle (idles.md §4).
pub const CAMERA_NODE: &str = "Camera1st";

/// After a collection's idle could not be started (its actor is not drawn, its clip does
/// not load), seconds until the collection is tried again [engineering choice].
const RETRY_SECONDS: f32 = 5.0;

/// How long an idle asked for waits for its clip to be read on a worker thread before it
/// is read in the frame it starts [engineering choice, #1429: a KF read and parse on the
/// main thread cost 8.4 ms in one frame at the Gomorrah floor; a few frames' delay to an
/// idle's start is not seen, a frame that long is].
const CLIP_WAIT: std::time::Duration = std::time::Duration::from_millis(500);

/// Seconds between two looks at the idle tree for an actor's drawn-weapon fidget
/// [design: how often the original evaluates its tree is not known; with the rifle
/// branch's three 5% leaves a standing rifleman fidgets about every 14 s].
pub const FIDGET_CHECK_SECONDS: f32 = 2.0;

/// Seconds between two looks at the idle tree for a fleeing actor's cower idle: short,
/// so it cowers soon after it stops and again soon after the idle's loops end
/// [engineering choice].
pub const COWER_CHECK_SECONDS: f32 = 0.5;

/// Seconds between two looks at the idle tree for a creature's standing look
/// (`IdleSource::Standing`): a changed state shows within a second [engineering choice].
pub const STANDING_CHECK_SECONDS: f32 = 1.0;

/// After the tree gave an actor in a conversation no gesture, seconds until it is asked
/// again in the same part (a new response asks at once) [engineering choice].
pub const GESTURE_RETRY_SECONDS: f32 = 1.0;

/// The player moved when their feet went further than this in a frame, game units
/// [engineering choice: above the collision's settling].
const PLAYER_STILL: f32 = 0.5;

/// The animation folder of people (NPC_ and the player) below `meshes\`
/// [observed: animation.md §3.1].
const PEOPLE_FOLDER: &str = "characters\\_male";

/// One idle playing, with the clip as its rig takes it.
pub struct Playing<C> {
    /// The IDLE record (0 for a group).
    pub idle: u32,
    /// The animation group a script asked for with `PlayGroup` (`actor_groups.rs`)
    /// instead of an idle.
    pub group: Option<u16>,
    pub source: IdleSource,
    /// What asked for a package's or a marker's idle: the actor's package at the time,
    /// or the marker's base.
    owner: Option<FormId>,
    pub clip: C,
    pub play: IdlePlay,
    /// Tells this idle from the others the slot played.
    pub serial: u32,
    /// Started in the step under way.
    fresh: bool,
    /// Holds its actor in place until it ends ([`holds_in_place`]).
    pub holds: bool,
}

/// The idles of one rig: the one playing and, while that fades in, the one before it.
pub struct IdleSlot<C> {
    pub cur: Option<Playing<C>>,
    pub prev: Option<Playing<C>>,
    serial: u32,
}

impl<C> Default for IdleSlot<C> {
    fn default() -> Self {
        IdleSlot { cur: None, prev: None, serial: 0 }
    }
}

impl<C> IdleSlot<C> {
    /// Play an idle from its start. What played until now is held under it while it
    /// fades in; an idle started in the same step is replaced outright (a package's
    /// change idle followed by the next package's begin idle).
    ///
    /// The idle that shows now asked for again by a script or a dialogue line is not
    /// started over: it plays on, or holds its last pose again (`IdlePlay::again`);
    /// returns false then. (Collections, fidgets and gestures that pick the same idle
    /// again play it again.) That holds too for an ended one that a fidget or a
    /// collection's idle, started as it ended, now fades in over: it comes back and the
    /// newcomer goes [design: #1355, the opening's mirror hand-over, where Doc's chair
    /// fidget started in the step the scripted idle ended and the next line's speaker
    /// idle, asked for a frame later, handed the mirror over a second time].
    fn start(&mut self, idle: u32, source: IdleSource, owner: Option<FormId>, clip: C, play: IdlePlay) -> bool {
        let asked = matches!(source, IdleSource::Script | IdleSource::Speaker | IdleSource::Listener);
        let same = |c: &Playing<C>| idle != 0 && c.idle == idle && c.group.is_none() && !c.play.gone();
        let picked = |c: &Playing<C>| !matches!(c.source, IdleSource::Script | IdleSource::Speaker | IdleSource::Listener);
        if asked && self.cur.as_ref().is_some_and(picked) && self.prev.as_ref().is_some_and(same) {
            self.cur = self.prev.take();
        }
        if let Some(c) = self.cur.as_mut().filter(|c| asked && same(c)) {
            c.play.again();
            (c.source, c.owner) = (source, owner);
            return false;
        }
        if let Some(old) = self.cur.take().filter(|o| !o.fresh && !o.play.gone()) {
            self.prev = Some(old);
        }
        self.serial = self.serial.wrapping_add(1);
        self.cur = Some(Playing { idle, group: None, source, owner, clip, play, serial: self.serial, fresh: true, holds: false });
        true
    }

    /// Play a script's animation group as an idle is played (`actor_groups.rs`); returns
    /// its serial.
    pub fn start_group(&mut self, group: u16, clip: C, play: IdlePlay) -> u32 {
        self.start(0, IdleSource::Script, None, clip, play);
        if let Some(c) = self.cur.as_mut() {
            c.group = Some(group);
        }
        self.serial
    }

    /// Advance by `dt` seconds.
    fn advance(&mut self, dt: f32) {
        if let Some(c) = self.cur.as_mut() {
            c.play.advance(dt);
        }
        if self.cur.as_ref().is_some_and(|c| c.play.gone()) {
            self.cur = None;
        }
        // The one before goes on as it was (a loop keeps looping, an ended one holds its
        // last pose) until the new one covers it.
        if let Some(p) = self.prev.as_mut().filter(|p| !p.play.ended()) {
            p.play.advance(dt);
        }
        if self.cur.as_ref().is_none_or(|c| c.play.ended() || c.play.weight() >= 1.0) {
            self.prev = None;
        }
    }

    /// The idle playing (not one that has ended and fades out).
    pub fn playing(&self) -> Option<&Playing<C>> {
        self.cur.as_ref().filter(|c| !c.play.ended())
    }

    pub fn is_empty(&self) -> bool {
        self.cur.is_none() && self.prev.is_none()
    }

    /// What shows, with its weight: the idle playing (fading in from its start, out
    /// from its end), and the one before it with the rest while the new one fades in.
    pub fn shown(&self) -> impl Iterator<Item = (&Playing<C>, f32)> {
        let w = self.cur.as_ref().map_or(0.0, |c| c.play.weight());
        self.cur.iter().map(move |c| (c, w)).chain(self.prev.iter().map(move |p| (p, 1.0 - w)))
    }

    /// Whether the idles cover the rig's other clips: one plays in full, or fades in
    /// over the one before it. (While the first one fades in, the other clips stay
    /// under it.)
    pub fn covers(&self) -> bool {
        self.playing().is_some_and(|c| self.prev.is_some() || c.play.weight() >= 1.0)
    }

    /// A package's or marker's idle that loops for ever leaves its loop once `owner`
    /// (the actor's package now, or the marker it stands at) is not what asked for it.
    fn check_owner(&mut self, owner_now: impl Fn(IdleSource) -> Option<FormId>) {
        // A new step: what plays was not started in it.
        if let Some(c) = self.cur.as_mut() {
            c.fresh = false;
        }
        if let Some(c) = self.cur.as_mut().filter(|c| c.source.of_package() && c.play.endless()) {
            if owner_now(c.source) != c.owner {
                c.play.leave_loop();
            }
        }
    }
}

/// The idles an actor plays (on its root, beside its [`Locomotion`]).
#[derive(Component, Default)]
pub struct ActorIdle(pub IdleSlot<Arc<BoundClip>>);

impl ActorIdle {
    /// Whether a stimpak's upper-body idle shows (`npc_stimpaks.rs`): it is laid over the
    /// fight's aim and attacks too.
    pub fn over_pose(&self) -> bool {
        self.0.shown().any(|(p, w)| w > 0.0 && p.source == IdleSource::Stimpak)
    }

    /// The idles as blend layers ([`IDLE_LAYER`] and the next).
    pub fn layers(&self, player: &mut BlendPlayer) {
        for (k, (p, w)) in self.0.shown().enumerate() {
            let len = p.play.keys.length.max(1e-3);
            player.layers[IDLE_LAYER + k] = (w > 0.0).then(|| Layer { clip: p.clip.clone(), phase: (p.play.time() / len).min(0.999), weight: w });
        }
    }
}

/// The NPCs whose legs an idle poses ([`holds_in_place`]) and who stand while it shows,
/// shared with `residents.rs` (idles.md §15).
#[derive(Resource, Default)]
pub struct IdleHolds {
    /// Written here: those whose idle shows (playing or fading out). Their walk waits.
    pub held: HashSet<u32>,
    /// Written by `residents.rs`: those of them whose AI has somewhere to go. Their idle
    /// fades out at once (a looping one too), and they walk once it has gone
    /// [design: owner 2026-10-31].
    pub breaking: HashSet<u32>,
    /// Written here: those of `held` whose idle carries the body away from the reference
    /// ([`carries_body`]).
    pub carried_off: HashSet<u32>,
}

/// How far (units, across the ground) an idle's `Bip01 NonAccum` must take the body from
/// the reference for [`carries_body`] [engineering choice: a standing idle's sway stays
/// within a few units; Dad's walk in the birth scene reaches 144].
const CARRY_DISTANCE: f32 = 32.0;

/// Whether `clip` moves `Bip01 NonAccum` (where the body hangs from the reference) more
/// than [`CARRY_DISTANCE`] across the ground at any of a few times through it: a scene
/// that walks its actor about while the reference stays (Fallout 3's birth scene: Dad's
/// sections keep him 96, 108 to 128 units forward and aside of his marker [observed:
/// `cg00dadsection00.kf` .. `02.kf`, fnvtool nif decode]).
pub fn carries_body(clip: &Clip) -> bool {
    let Some((_, t)) = clip.tracks.iter().find(|(n, _)| n.eq_ignore_ascii_case("Bip01 NonAccum")) else { return false };
    (0..=8).any(|k| t.translation.sample(clip.start + clip.length() * k as f32 / 8.0).is_some_and(|v| v[0].hypot(v[1]) > CARRY_DISTANCE))
}

/// Whether a track drives a leg: the people's `Bip01 L Thigh`, `Bip01 R Calf`,
/// `Bip01 L Foot`, and creatures' bones named the same way or `...Leg...`
/// [observed: the bone names of `_male\skeleton.nif` and the idle clips].
fn is_leg(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["thigh", "calf", "foot", "leg"].iter().any(|k| n.contains(k))
}

/// The highest priority of a clip's leg tracks.
fn leg_priority(clip: &Clip) -> Option<i32> {
    clip.tracks.iter().enumerate().filter(|(_, (n, _))| is_leg(n)).map(|(k, _)| clip.priorities.get(k).copied().unwrap_or(0)).max()
}

/// Whether an idle poses its actor's legs, so the actor stands while it shows and breaks
/// out of it (fading it out) to walk (idles.md §15) [design: the original's rule is not
/// documented, observer check 57; breaking out: owner 2026-10-31]:
///
/// - asked for by a script (`PlayIdle`, `PickIdle`), a package's begin, end or change, a
///   dialogue line, or a pick-up (collections, fidgets, gestures, cowering and stimpaks
///   end, or play on, when their actor moves: not held);
/// - not of the Upper Body section (`IDLE` DATA, [`fnv_game::data::IDLE_SECTION_UPPER_BODY`]);
/// - its legs win over the walk: one of its leg tracks has a higher priority than the
///   actor's walking clip's legs (`CheeringBothArmsUp.kf` 50 over `mtforward.kf` 30),
///   so walking on would show its legs standing while the body slides. A cower's legs
///   (23) lose to the run's (30): it runs with its arms over its head (§13).
pub fn holds_in_place(source: IdleSource, section: u8, idle: &Clip, walk: Option<&Clip>) -> bool {
    let asked = matches!(
        source,
        IdleSource::Script | IdleSource::Picked | IdleSource::PackageBegin | IdleSource::PackageEnd | IdleSource::PackageChange | IdleSource::Speaker | IdleSource::Listener | IdleSource::PickUp
    );
    asked
        && section != fnv_game::data::IDLE_SECTION_UPPER_BODY
        && match (leg_priority(idle), walk.and_then(leg_priority)) {
            (Some(i), Some(w)) => i > w,
            _ => false,
        }
}

/// The clip an actor walks on: its stance's forward walk, else its run.
fn walk_clip(lo: &Locomotion) -> Option<&Clip> {
    use fnv_actor::locomotion::Move;
    lo.set.clips.get(&Move::Forward).or_else(|| lo.set.clips.get(&Move::FastForward)).map(|c| &*c.bound.clip)
}

/// The idles the player plays: on the first-person rig (clips that drive its camera
/// node) and on the third-person body (all others).
#[derive(Resource, Default)]
pub struct PlayerIdles {
    pub first: IdleSlot<Arc<Clip>>,
    pub third: IdleSlot<Arc<Clip>>,
    /// The player when an idle began to turn the view ([`Held`]).
    pub held: Option<Held>,
}

/// The player as an idle that turns the view found them. While such idles play, the
/// first-person rig stands at `feet` with that heading, the character is held there
/// (`nvf_play::furniture::PlayerSeat`), and only the view's own turning since then is
/// added to what the animation gives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
    /// Where the game last put the player if they have not moved by themselves since
    /// (`Player::placed`), else where they stand [observed: the opening's start marker
    /// is in Doc Mitchell's bed, and the collision pushes the character about 10 units
    /// off it before the wake-up starts; idles.md §4].
    pub feet: [f32; 3],
    /// Where the character stood when the idle began, clear of what it collides with:
    /// it stands there again when the idles let it go [design: let go at `feet`, the
    /// opening's character was pushed up onto the bed frame; owner report 2026-10-16].
    pub free: [f32; 3],
    pub heading: f32,
    pub pitch: f32,
}

impl Held {
    /// How far (units) the game's placement may lie from `feet` and still be the same.
    const SAME_PLACE: f32 = 1.0;

    /// Where the player is held now that the game has put them somewhere else while the
    /// idles held them, `None` when it has not. A script's `MoveTo` lands a frame after
    /// the idle it starts with: Tale of Two Wastelands' opening starts the birth idle in
    /// its start room and moves the player into the birth cell in the same script run,
    /// and the view stayed at the start room's spot in the birth cell, an empty screen
    /// [observed: owner's TTW run 2026-11-02, the idle at (2036, 1946), the travel to
    /// (-5276, -7148)].
    pub fn moved(&self, placed: Option<[f32; 3]>, heading: f32, pitch: f32) -> Option<Held> {
        let p = placed?;
        let far = (0..3).any(|i| (p[i] - self.feet[i]).abs() > Self::SAME_PLACE);
        far.then_some(Held { feet: p, free: p, heading, pitch })
    }
}

/// An actor's place in an idle collection.
struct Collection {
    /// A marker's (true) or a package's, and its form.
    key: (bool, u32),
    picker: IdlePicker,
    /// The idle picked and the seconds still to wait before it.
    next: Option<(FormId, f32)>,
    /// Seconds until the collection is tried again after a failed start.
    retry: f32,
}

/// Counts for the run summary.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub asked: u32,
    pub script: u32,
    pub package: u32,
    pub collection: u32,
    pub dialogue: u32,
    pub started: u32,
    pub player_started: u32,
    pub not_drawn: u32,
    pub no_clip: u32,
    /// Drawn-weapon fidgets started on actors and on the player.
    pub actor_fidgets: u32,
    pub player_fidgets: u32,
    /// Creatures' combat idles started (alerted, standing).
    pub combat_idles: u32,
    /// Fidgets on furniture started on actors and on the player.
    pub actor_furniture_fidgets: u32,
    pub player_furniture_fidgets: u32,
    /// Talking gestures started while the actor talked and while it listened; walks of
    /// the tree that gave none.
    pub talk_gestures: u32,
    pub listen_gestures: u32,
    pub no_gesture: u32,
    /// Cower idles started on fleeing actors.
    pub cowers: u32,
    /// Panic idles started on actors with a live explosive in the pocket, and such
    /// actors whose folder has none.
    pub panics: u32,
    pub no_panic: u32,
    /// Idles that waited for their clip to be read on a worker thread, the longest wait
    /// (milliseconds), and those whose clip was read in the frame they started after
    /// [`CLIP_WAIT`].
    pub clip_waits: u32,
    pub longest_clip_wait_ms: f64,
    pub clip_read_late: u32,
}

/// An actor's part in a conversation as the gestures last saw it: talking or listening,
/// the line and its response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TalkKey {
    talking: bool,
    info: Option<FormId>,
    response: usize,
}

/// The gestures of one actor in a conversation.
struct Gesturing {
    key: TalkKey,
    /// Seconds until the tree is asked again after it gave nothing.
    retry: f32,
}

/// A small deterministic generator (SplitMix64) for loop counts and random picks: the
/// script runtime's random numbers are not touched, and runs repeat.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u32) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (((z ^ (z >> 31)) >> 32) as u32) % n.max(1)
    }
}

#[derive(Resource)]
pub struct Idles {
    collections: HashMap<u32, Collection>,
    /// Seconds until each actor's next look for a fidget, and for its cower idle.
    fidget_check: HashMap<u32, f32>,
    cower_check: HashMap<u32, f32>,
    /// The actors with a live explosive whose panic was looked for (`GetPlantedExplosive`).
    panicked: HashSet<u32>,
    /// Seconds until each creature's next look for its standing look.
    standing_check: HashMap<u32, f32>,
    /// When each actor last started each fidget or gesture (seconds of `clock`), for
    /// the replay delay.
    fidget_last: HashMap<(u32, u32), f32>,
    /// The actors in a conversation.
    gesturing: HashMap<u32, Gesturing>,
    clock: f32,
    /// The player's feet the frame before.
    player_feet: Option<[f32; 3]>,
    /// What each actor was last reported to the rules as playing.
    told: HashMap<u32, u32>,
    /// Idles asked for whose clip is being read on a worker thread, and since when.
    waiting: Vec<(IdleRequest, std::time::Instant)>,
    rng: Rng,
    pub stats: Stats,
}

impl Default for Idles {
    fn default() -> Self {
        Idles {
            collections: HashMap::new(),
            fidget_check: HashMap::new(),
            cower_check: HashMap::new(),
            panicked: HashSet::new(),
            standing_check: HashMap::new(),
            fidget_last: HashMap::new(),
            gesturing: HashMap::new(),
            clock: 0.0,
            player_feet: None,
            told: HashMap::new(),
            waiting: Vec::new(), rng: Rng(0x1D1E_5EED), stats: Stats::default() }
    }
}

/// The clip of an idle and its playback, as its record and text keys say; a creature's
/// standing look loops until replaced (`IdleSource::Standing`).
fn load(ctx: &mut ActorCtx, game: &Game, idle: FormId, rng: &mut Rng, endless: bool) -> Result<(Arc<Clip>, IdlePlay), String> {
    let def = game.data.idles.get(&idle).ok_or("not an idle animation")?;
    let path = clip_path(game, idle).ok_or("it names no file")?;
    let clip = ctx.clip(&path).ok_or_else(|| format!("{path} does not load"))?;
    let keys = IdleKeys::read(clip.start, clip.stop, &clip.text_keys);
    let passes = match def.loops(|n| rng.below(n)) {
        _ if endless => None,
        Loops::Forever => None,
        Loops::Times(n) => Some(n),
    };
    Ok((clip, IdlePlay::new(keys, passes)))
}

/// The KF file an idle plays, as its record names it.
fn clip_path(game: &Game, idle: FormId) -> Option<String> {
    Some(meshes_path(game.data.idles.get(&idle)?.model.as_deref()?))
}

/// Whether the clip of `idle` is loaded (or known not to load), so starting it reads no
/// file; when not, its read is started on a worker thread. An idle naming no file is
/// ready ([`load`] says why it does not play).
fn clip_ready(ctx: &mut ActorCtx, game: &Game, idle: FormId) -> bool {
    clip_path(game, idle).is_none_or(|p| ctx.clip_ready(&p))
}

/// Whether an idle asked for waits for its clip: not ready, and asked for less than
/// [`CLIP_WAIT`] ago.
fn waits_for_clip(ready: bool, waited: std::time::Duration) -> bool {
    !ready && waited < CLIP_WAIT
}

fn describe(game: &Game, idle: FormId, play: &IdlePlay) -> String {
    let k = play.keys;
    let loops = if play.endless() { ", looping until replaced" } else { "" };
    let range = k.loop_range.map_or(String::new(), |(a, b)| format!(", loop {a:.2}-{b:.2} s"));
    format!("{} ({:.2} s{range}{loops}, fades {:.2} / {:.2} s)", game.data.name(idle), k.length, k.blend_in, k.blend_out)
}

/// What [`drive`] reads and writes of a drawn actor.
type Drawn = (Entity, &'static RefVisual, &'static ActorRig, Option<&'static mut ActorIdle>, Option<&'static Locomotion>, Option<&'static WeaponHold>);

/// Take the idles asked for, run the collections, start and advance what plays.
#[allow(clippy::too_many_arguments)]
pub fn drive(
    mut commands: Commands,
    time: Res<Time>,
    mut st: ResMut<Idles>,
    mut player_idles: ResMut<PlayerIdles>,
    mut script: Option<ResMut<ScriptState>>,
    mut residents: Option<ResMut<Residents>>,
    mut sim: Option<ResMut<nvf_play::sim::SimState>>,
    player: Option<Res<Player>>,
    control: Option<Res<PlayerControl>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut actors: Query<Drawn>,
    held: Query<&HeldWeapon>,
    mut holds: ResMut<IdleHolds>,
) {
    let dt = time.delta_secs().min(0.1);
    let st = &mut *st;
    let pi = &mut *player_idles;
    let standing: HashMap<u32, Option<FormId>> = residents.as_deref().map(|r| r.standing().map(|s| (s.actor, s.marker)).collect()).unwrap_or_default();
    // The game state: the game being played, else the residents' own.
    let (game, played) = match script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        Some(rt) => (Some(&mut rt.game), true),
        None => (residents.as_deref_mut().and_then(|r| r.own_game()), false),
    };
    let Some(game) = game else { return };
    // A script's menu (a message box, the character menus) holds the game, as the AI's
    // walks are held (`residents.rs`): the idles may start but stand still until it
    // closes, so a scene goes on where it was [observed: owner's TTW run 2026-11-02, the
    // stage that opens the trait menu starts the birth scene's last section, which
    // played out behind the menu; hypothesis: the original's menu mode stops it].
    let dt = if game.menu.is_some() { 0.0 } else { dt };
    // Between frames the game being played keeps its world in the simulation
    // (`nvf_play::script::with_world`); conditions are evaluated with it put back.
    let mut with_world = |game: &mut Game<'static>, f: &mut dyn FnMut(&mut Game<'static>)| match sim.as_deref_mut().filter(|_| played) {
        Some(sim) => {
            std::mem::swap(&mut game.world, &mut sim.world);
            f(game);
            std::mem::swap(&mut game.world, &mut sim.world);
        }
        None => f(game),
    };
    let player_ref = FormId::PLAYER_REF.0;
    let package_of = |game: &Game, actor: u32| game.extra.get(&FormId(actor)).and_then(|e| e.current_package);

    // Idles that belong to a package or a marker that is no longer the actor's.
    let owner_now = |game: &Game, actor: u32, source: IdleSource| match source {
        IdleSource::MarkerIdles => standing.get(&actor).copied().flatten(),
        _ => package_of(game, actor),
    };
    for slot in [&mut pi.first, &mut pi.third] {
        slot.check_owner(|s| owner_now(game, player_ref, s));
    }
    let mut busy: HashMap<u32, bool> = HashMap::new();
    // The rules are told which NPCs hold their weapon in the hand (`IsWeaponOut`, as for
    // the player: once the weapon is on the hand [documented: catalogue notes]), and
    // which weapon that is (`GetWeaponAnimType` for NPCs with no weapon equipped in the
    // rules' inventories; the drawn one is the equipped one otherwise, outfits.rs).
    for h in &held {
        game.set_held_weapon(FormId(h.reference), Some(h.weapon));
    }
    for (_, v, _, _, _, hold) in &actors {
        if let Some(h) = hold {
            // GetAnimAction's drawing and putting away: `anim_state.rs`.
            game.set_weapon_out(FormId(v.0), h.anim.stage == Stage::Drawn);
        }
    }
    // Who is in a conversation, and their part in it (talking gestures).
    let talk_parts = game.talk_parts();
    // Scripts' StopIdle and ForceStopIdle (`fnv_game::nvse_plugins`).
    let stops = std::mem::take(&mut game.gaps.idle_stops);
    for (_, v, _, idle, lo, _) in &mut actors {
        let Some(mut idle) = idle else { continue };
        idle.0.check_owner(|s| owner_now(game, v.0, s));
        if stops.contains(&FormId(v.0)) {
            if let Some(c) = idle.0.cur.as_mut() {
                c.play.stop();
            }
        }
        // A collection's idle ends when its actor walks off or is drawn into a
        // conversation, so it can turn to the other and gesture (idles.md §12) [design].
        let walking = lo.is_some_and(|l| l.velocity != [0.0; 2]) || !standing.contains_key(&v.0);
        let talking = talk_parts.iter().any(|p| p.actor == FormId(v.0));
        if let Some(c) = idle.0.cur.as_mut().filter(|c| (walking || talking) && matches!(c.source, IdleSource::PackageIdles | IdleSource::MarkerIdles)) {
            c.play.stop();
        }
        // A fidget ends when its actor moves or puts the weapon away [design].
        let moving = lo.is_some_and(|l| l.velocity != [0.0; 2]);
        if let Some(c) = idle.0.cur.as_mut().filter(|c| c.source == IdleSource::Fidget && (moving || !weapon_out(game, v.0))) {
            c.play.stop();
        }
        // A creature's combat idle ends when it moves or is calm again [design].
        if let Some(c) = idle.0.cur.as_mut().filter(|c| c.source == IdleSource::CombatIdle && (moving || !game.draws_weapon(FormId(v.0)))) {
            c.play.stop();
        }
        // A fidget on furniture ends when its actor starts getting off: it fades out over
        // the start of the getting-off clip, which starts where the body sits
        // (furniture.md §4) [design].
        if let Some(c) = idle.0.cur.as_mut().filter(|c| c.source == IdleSource::FurnitureFidget && !on_furniture(game, v.0)) {
            c.play.stop();
        }
        // A cower goes on while its actor runs and ends when it no longer flees
        // (idles.md §13.2).
        if let Some(c) = idle.0.cur.as_mut().filter(|c| c.source == IdleSource::Cower && !fleeing(game, v.0)) {
            c.play.stop();
        }
        // A creature's standing look ends when it moves, fights or dies [design].
        if let Some(c) = idle.0.cur.as_mut().filter(|c| c.source == IdleSource::Standing && (moving || game.draws_weapon(FormId(v.0)) || game.is_dead(FormId(v.0)))) {
            c.play.stop();
        }
        // A gesture ends when its actor walks or is no longer in a conversation [design].
        if let Some(c) = idle.0.cur.as_mut().filter(|c| c.source == IdleSource::Gesture && (moving || !talk_parts.iter().any(|p| p.actor.0 == v.0))) {
            c.play.stop();
        }
        // An idle that poses its actor's legs ends when the actor has somewhere to go
        // (`residents.rs`), or goes into combat (alerted, fighting or searching): it
        // breaks out of it [design: owner 2026-10-31, idles.md §15].
        if let Some(c) = idle.0.cur.as_mut().filter(|c| c.holds && !c.play.ended()) {
            let why = if holds.breaking.contains(&v.0) {
                Some("to walk")
            } else if game.awareness.state(FormId(v.0)).in_combat() {
                Some("for combat")
            } else {
                None
            };
            if let Some(why) = why {
                info!("idle hold: {} ({:08X}) breaks out of its idle {why}", game.data.name(FormId(v.0)), v.0);
                c.play.stop();
            }
        }
        busy.insert(v.0, idle.0.playing().is_some());
    }
    // The player stands still with a weapon ready in third person: may fidget [design,
    // owner 2026-10-16]; a fidget playing ends otherwise.
    let feet = player.as_deref().map(|p| p.feet);
    let moved = match (feet, st.player_feet) {
        (Some(a), Some(b)) => (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt() > PLAYER_STILL,
        _ => true,
    };
    st.player_feet = feet;
    let player_still = !moved
        && control.as_deref().is_some_and(|c| c.weapon_out && !c.aiming && !c.trigger && !c.reload && c.third_person_view())
        && weapon_out(game, player_ref)
        && package_of(game, player_ref).is_none();
    if let Some(c) = pi.third.cur.as_mut().filter(|c| c.source == IdleSource::Fidget && !player_still) {
        c.play.stop();
    }
    // On furniture in third person: may fidget there (the tree's player nodes want the
    // third person, and a fidget in first person would sway the view) [design].
    let player_seated = on_furniture(game, player_ref) && control.as_deref().is_some_and(|c| c.third_person_view()) && package_of(game, player_ref).is_none();
    if let Some(c) = pi.third.cur.as_mut().filter(|c| c.source == IdleSource::FurnitureFidget && !player_seated) {
        c.play.stop();
    }
    if stops.contains(&FormId(player_ref)) {
        for slot in [&mut pi.first.cur, &mut pi.third.cur].into_iter().flatten() {
            slot.play.stop();
        }
    }
    busy.insert(player_ref, pi.first.playing().is_some() || pi.third.playing().is_some());
    // An idle asked for this frame (a package's begin idle) comes before its collection.
    for r in &game.idle_requests {
        busy.insert(r.actor.0, true);
    }
    // So is one waiting for its clip; a script's StopIdle drops it.
    st.waiting.retain(|(r, _)| !stops.contains(&r.actor));
    for (r, _) in &st.waiting {
        busy.insert(r.actor.0, true);
    }

    // Collections: the standing residents', and the player's under a script package.
    let candidates = standing.iter().map(|(a, m)| (*a, *m)).chain(package_of(game, player_ref).map(|_| (player_ref, None)));
    let mut live: Vec<u32> = Vec::new();
    let mut ask: Vec<(u32, FormId, IdleSource)> = Vec::new();
    for (actor, marker) in candidates {
        // A script's SetIdleMarkerAnimation(s) first (`fnv_game::nvse_jg4::idle_marker`),
        // copied out so the game can be changed below; the record's is borrowed.
        let edited = |m: FormId| game.other.idle_markers.get(&m).map(|c| Cow::Owned(c.clone()));
        let of_marker = marker
            .and_then(|m| edited(m).or_else(|| game.data.idle_markers.get(&m).map(Cow::Borrowed)).filter(|c| !c.idles.is_empty()).map(|c| ((true, m.0), c)));
        let quiet = package_of(game, actor).and_then(|p| game.package_def(&p)).is_some_and(|d| d.has(flags::NO_IDLE_ANIMS));
        let Some((key, collection)) = of_marker.or_else(|| game.package_idles(FormId(actor)).map(|(p, c)| ((false, p.0), Cow::Borrowed(c)))).filter(|_| !quiet) else { continue };
        live.push(actor);
        let c = st.collections.entry(actor).or_insert_with(|| Collection { key, picker: IdlePicker::default(), next: None, retry: 0.0 });
        if c.key != key {
            *c = Collection { key, picker: IdlePicker::default(), next: None, retry: 0.0 };
        }
        c.retry = (c.retry - dt).max(0.0);
        // In a conversation the collection waits; its gestures come from the tree.
        if busy.get(&actor).copied().unwrap_or(false) || c.retry > 0.0 || talk_parts.iter().any(|p| p.actor == FormId(actor)) {
            continue;
        }
        if c.next.is_none() {
            // The next idle of the collection whose own conditions the actor meets; when
            // none of them does, the collection is tried again later.
            let rng = &mut st.rng;
            with_world(game, &mut |game| {
                for _ in 0..collection.idles.len().max(1) {
                    c.next = c.picker.next(&collection, |n| rng.below(n as u32) as usize).filter(|(idle, _)| game.idle_allowed(FormId(actor), *idle));
                    if c.next.is_some() {
                        break;
                    }
                }
            });
            match c.next {
                None => c.retry = RETRY_SECONDS.max(collection.timer),
                // Its clip is read while the actor waits to play it.
                Some((idle, _)) => {
                    clip_ready(&mut ActorCtx { cache: &mut parts, models: &mut models }, game, idle);
                }
            }
        }
        let Some((idle, wait)) = c.next.as_mut() else { continue };
        *wait -= dt;
        if *wait <= 0.0 {
            ask.push((actor, *idle, if key.0 { IdleSource::MarkerIdles } else { IdleSource::PackageIdles }));
            c.next = None;
        }
    }
    st.collections.retain(|a, _| live.contains(a));
    for (actor, idle, source) in ask {
        game.ask_idle(FormId(actor), idle, source);
    }

    st.clock += dt;
    // Panic: a live explosive in the pocket (pickpocket.md §5); once per explosive,
    // before anything else is picked.
    let mut panics: Vec<(u32, FormId)> = Vec::new();
    let mut live_now: HashSet<u32> = HashSet::new();
    if !game.pickpocket.planted.is_empty() {
        for (_, v, rig, ..) in &actors {
            if game.is_dead(FormId(v.0)) || !game.has_planted_explosive(FormId(v.0)) {
                continue;
            }
            live_now.insert(v.0);
            if !st.panicked.insert(v.0) {
                continue;
            }
            let mut pick = None;
            if let Some(folder) = skeleton_folder(&rig.skeleton) {
                with_world(game, &mut |game| pick = game.panic_idle(FormId(v.0), &folder, &|_| false));
            }
            match pick {
                Some(idle) => {
                    panics.push((v.0, idle));
                    busy.insert(v.0, true);
                }
                None => {
                    st.stats.no_panic += 1;
                    info!("idle: {} ({:08X}) has a live explosive and no panic idle for its kind: it stands", game.data.name(FormId(v.0)), v.0);
                }
            }
        }
    }
    st.panicked.retain(|a| live_now.contains(a));
    for (actor, idle) in panics {
        st.stats.panics += 1;
        game.ask_idle(FormId(actor), idle, IdleSource::Panic);
    }
    // Cowering: fleeing NPCs standing still (idles.md §13); before their fidgets.
    let cowering: Vec<(u32, String)> = actors
        .iter()
        .filter(|(_, v, ..)| !live.contains(&v.0) && fleeing(game, v.0) && !game.is_dead(FormId(v.0)) && game.furniture_use(FormId(v.0)).is_none())
        .filter(|(_, _, _, _, lo, _)| lo.is_some_and(|l| l.velocity == [0.0; 2]))
        .filter_map(|(_, v, rig, ..)| Some((v.0, skeleton_folder(&rig.skeleton)?)))
        .collect();
    st.cower_check.retain(|a, _| cowering.iter().any(|(c, _)| c == a));
    let mut cowers: Vec<(u32, FormId)> = Vec::new();
    for (actor, folder) in &cowering {
        let check = st.cower_check.entry(*actor).or_insert(0.0);
        *check -= dt;
        if *check > 0.0 || busy.get(actor).copied().unwrap_or(false) {
            continue;
        }
        *check = COWER_CHECK_SECONDS;
        let mut pick = None;
        with_world(game, &mut |game| pick = game.flee_idle(FormId(*actor), folder, &|_| false));
        if let Some(idle) = pick {
            cowers.push((*actor, idle));
            busy.insert(*actor, true);
        }
    }
    for (actor, idle) in cowers {
        st.stats.cowers += 1;
        game.ask_idle(FormId(actor), idle, IdleSource::Cower);
    }

    // Fidgets: NPCs on furniture, or standing still with a weapon out; the player too.
    let mut still: Vec<(u32, String, FidgetKind)> = actors
        .iter()
        // An actor in a conversation has its talking gestures instead (idles.md §12).
        .filter(|(_, v, ..)| !live.contains(&v.0) && !game.is_dead(FormId(v.0)) && !talk_parts.iter().any(|p| p.actor.0 == v.0) && !cowering.iter().any(|(c, _)| *c == v.0))
        .filter_map(|(_, v, rig, _, lo, _)| {
            let folder = skeleton_folder(&rig.skeleton)?;
            let still = lo.is_some_and(|l| l.velocity == [0.0; 2]);
            let kind = if game.furniture_use(FormId(v.0)).is_some() {
                on_furniture(game, v.0).then_some(FidgetKind::OnFurniture)?
            } else if folder.split('\\').any(|p| p == "creatures") {
                (still && game.draws_weapon(FormId(v.0))).then_some(FidgetKind::Alerted)?
            } else {
                (still && weapon_out(game, v.0)).then_some(FidgetKind::WeaponOut)?
            };
            Some((v.0, folder, kind))
        })
        .collect();
    if player_seated {
        still.push((player_ref, PEOPLE_FOLDER.to_string(), FidgetKind::OnFurniture));
    } else if player_still {
        still.push((player_ref, PEOPLE_FOLDER.to_string(), FidgetKind::WeaponOut));
    }
    let mut fidgets: Vec<(u32, FormId, FidgetKind)> = Vec::new();
    let mut stop_gesture: Vec<u32> = Vec::new();
    st.fidget_check.retain(|a, _| still.iter().any(|(s, ..)| s == a));
    for (actor, folder, kind) in still {
        let check = st.fidget_check.entry(actor).or_insert(FIDGET_CHECK_SECONDS);
        *check -= dt;
        if *check > 0.0 || busy.get(&actor).copied().unwrap_or(false) {
            continue;
        }
        *check = FIDGET_CHECK_SECONDS;
        let (clock, last, data) = (st.clock, &st.fidget_last, game.data);
        // An idle is held back for its replay delay after it started [hypothesis: the
        // DATA replay delay is in seconds].
        let held_back = |idle: FormId| {
            let delay = data.idles.get(&idle).map_or(0.0, |d| d.replay_delay.max(0) as f32);
            delay > 0.0 && last.get(&(actor, idle.0)).is_some_and(|t| clock - t < delay)
        };
        let mut pick = None;
        with_world(game, &mut |game| pick = game.fidget(FormId(actor), &folder, kind, &held_back));
        if let Some(idle) = pick {
            fidgets.push((actor, idle, kind));
        }
    }
    for (actor, idle, kind) in fidgets {
        st.fidget_last.insert((actor, idle.0), st.clock);
        game.ask_idle(FormId(actor), idle, kind.source());
    }

    // Standing looks: creatures standing still and calm play the tree's Idle-section leaf
    // their state calls for in place of their base idle (Mr. House's screen, #1542). The
    // look is asked again now and then: a changed state changes it, none ends it.
    let calm: Vec<(u32, String, Option<u32>)> = actors
        .iter()
        .filter(|(_, v, ..)| !live.contains(&v.0) && !game.is_dead(FormId(v.0)) && !game.draws_weapon(FormId(v.0)) && game.furniture_use(FormId(v.0)).is_none())
        .filter(|(_, v, ..)| !talk_parts.iter().any(|p| p.actor.0 == v.0) && !cowering.iter().any(|(c, _)| *c == v.0))
        .filter(|(_, _, _, _, lo, _)| lo.is_none_or(|l| l.velocity == [0.0; 2]))
        .filter_map(|(_, v, rig, slot, ..)| {
            let folder = skeleton_folder(&rig.skeleton).filter(|f| f.split('\\').any(|p| p == "creatures"))?;
            Some((v.0, folder, slot.and_then(|s| s.0.playing()).filter(|p| p.source == IdleSource::Standing).map(|p| p.idle)))
        })
        .collect();
    st.standing_check.retain(|a, _| calm.iter().any(|(c, ..)| c == a));
    let mut looks: Vec<(u32, FormId)> = Vec::new();
    let mut end_looks: Vec<u32> = Vec::new();
    for (actor, folder, look) in &calm {
        let check = st.standing_check.entry(*actor).or_insert(0.0);
        *check -= dt;
        // Another idle plays (not its own look): the look waits.
        if *check > 0.0 || (busy.get(actor).copied().unwrap_or(false) && look.is_none()) {
            continue;
        }
        *check = STANDING_CHECK_SECONDS;
        let mut pick = None;
        with_world(game, &mut |game| pick = game.standing_idle(FormId(*actor), folder));
        match (pick, look) {
            (Some(p), Some(l)) if p.0 == *l => {}
            (Some(p), _) => looks.push((*actor, p)),
            (None, Some(_)) => end_looks.push(*actor),
            (None, None) => {}
        }
    }
    for actor in end_looks {
        if let Some(c) = actors.iter_mut().find(|(_, v, ..)| v.0 == actor).and_then(|(_, _, _, s, ..)| s).and_then(|s| s.into_inner().0.cur.as_mut()) {
            c.play.stop();
        }
    }
    for (actor, idle) in looks {
        game.ask_idle(FormId(actor), idle, IdleSource::Standing);
    }

    // Talking gestures: NPCs drawn in a conversation (idles.md §12).
    // Those waiting for their clip were asked for too (#1429).
    let asked_now: Vec<u32> = game.idle_requests.iter().chain(st.waiting.iter().map(|(r, _)| r)).map(|r| r.actor.0).collect();
    st.gesturing.retain(|a, _| talk_parts.iter().any(|p| p.actor.0 == *a));
    let mut gestures: Vec<(u32, FormId, Vec<FormId>, TalkKey)> = Vec::new();
    for part in &talk_parts {
        let actor = part.actor.0;
        let Some((_, _, rig, slot, lo, _)) = actors.iter().find(|(_, v, ..)| v.0 == actor) else { continue };
        let Some(folder) = skeleton_folder(&rig.skeleton) else { continue };
        if game.is_dead(part.actor) {
            continue;
        }
        let key = TalkKey { talking: part.talking, info: part.info, response: part.response };
        // The first frame of a part counts as a change for the wait's log line.
        let fresh = !st.gesturing.contains_key(&actor);
        let g = st.gesturing.entry(actor).or_insert(Gesturing { key, retry: 0.0 });
        let changed = g.key != key;
        if changed {
            g.key = key;
            g.retry = 0.0;
        }
        g.retry = (g.retry - dt).max(0.0);
        let playing = slot.and_then(|s| s.0.playing());
        // Another idle (the response's own, a script's) plays, or was asked for this
        // frame: it comes first. A gesture goes on until the part changes.
        // A standing look gives way to the gestures (House's talking face).
        let other = playing.is_some_and(|p| !matches!(p.source, IdleSource::Gesture | IdleSource::Standing)) || asked_now.contains(&actor);
        let gesture_on = playing.is_some_and(|p| p.source == IdleSource::Gesture);
        // Walking, or turning on the spot to face the other fast enough for the turn
        // clips (they step the feet; the walker's heading noise is far below it).
        let busy_body = lo.is_some_and(|l| l.velocity != [0.0; 2] || l.turn.abs() > fnv_actor::locomotion::TURN_RATE);
        if other || busy_body || (gesture_on && !changed) || (g.retry > 0.0 && !changed) {
            if (changed || fresh) && (other || busy_body) {
                let why = if other { "another idle plays" } else if lo.is_some_and(|l| l.velocity != [0.0; 2]) { "it walks" } else { "it turns on the spot" };
                info!("talk gesture: {} waits: {why}", game.data.name(part.actor));
            }
            continue;
        }
        let (clock, last, data) = (st.clock, &st.fidget_last, game.data);
        let held_back = |idle: FormId| {
            let delay = data.idles.get(&idle).map_or(0.0, |d| d.replay_delay.max(0) as f32);
            delay > 0.0 && last.get(&(actor, idle.0)).is_some_and(|t| clock - t < delay)
        };
        let mut pick = None;
        let partner = part.partner;
        with_world(game, &mut |game| pick = game.talk_gesture(FormId(actor), partner, &folder, &held_back));
        match pick {
            Some((idle, way)) => gestures.push((actor, idle, way, key)),
            None => {
                st.stats.no_gesture += 1;
                g.retry = GESTURE_RETRY_SECONDS;
                // The part changed and the tree has nothing for it: the old gesture ends.
                if changed && gesture_on {
                    stop_gesture.push(actor);
                }
            }
        }
    }
    for (actor, idle, way, key) in gestures {
        st.fidget_last.insert((actor, idle.0), st.clock);
        let data = game.data;
        let way: Vec<String> = way.iter().map(|n| data.name(*n)).collect();
        let line = key.info.map_or(String::new(), |i| format!(", line {} response {}", data.name(i), key.response + 1));
        info!("talk gesture: {} {} ({}{line}): {} by {}", data.name(FormId(actor)), if key.talking { "talks" } else { "listens" }, data.name(talk_parts.iter().find(|p| p.actor.0 == actor).map_or(FormId(0), |p| p.partner)), data.name(idle), way.join(" > "));
        if key.talking {
            st.stats.talk_gestures += 1;
        } else {
            st.stats.listen_gestures += 1;
        }
        game.ask_idle(FormId(actor), idle, IdleSource::Gesture);
    }

    for actor in stop_gesture {
        if let Some(mut s) = actors.iter_mut().find(|(_, v, ..)| v.0 == actor).and_then(|(_, _, _, s, _, _)| s) {
            if let Some(c) = s.0.cur.as_mut().filter(|c| c.source == IdleSource::Gesture) {
                c.play.stop();
            }
        }
    }

    // PickIdle: the tree's pick for each actor a script asked, by the actor's own folder
    // (`Game::pick_idle`); nothing when none fits [documented: GECK wiki PickIdle].
    for actor in std::mem::take(&mut game.idle_picks) {
        let folder = if actor.0 == player_ref { Some(PEOPLE_FOLDER.to_string()) } else { actors.iter().find(|(_, v, ..)| v.0 == actor.0).and_then(|(_, _, rig, ..)| skeleton_folder(&rig.skeleton)) };
        let name = game.data.name(actor);
        let Some(folder) = folder else {
            debug!("pick idle: {name} is not drawn: plays nothing");
            continue;
        };
        let (clock, last, data) = (st.clock, &st.fidget_last, game.data);
        let held_back = |idle: FormId| {
            let delay = data.idles.get(&idle).map_or(0.0, |d| d.replay_delay.max(0) as f32);
            delay > 0.0 && last.get(&(actor.0, idle.0)).is_some_and(|t| clock - t < delay)
        };
        let mut pick = None;
        with_world(game, &mut |game| pick = game.pick_idle(actor, &folder, &held_back));
        match pick {
            Some(idle) => {
                info!("pick idle: {name} picks {}", game.data.name(idle));
                st.fidget_last.insert((actor.0, idle.0), st.clock);
                game.ask_idle(actor, idle, IdleSource::Picked);
            }
            None => info!("pick idle: {name} finds no idle whose conditions it meets"),
        }
    }

    // Start what was asked for: those that waited for their clip first, in the order
    // asked. An actor's later asks wait behind one that waits, so its idles start in
    // the order asked, as they did in one frame.
    let fresh = std::mem::take(&mut game.idle_requests);
    let mut requests: Vec<(IdleRequest, Option<std::time::Instant>)> = std::mem::take(&mut st.waiting).into_iter().map(|(r, t)| (r, Some(t))).collect();
    requests.extend(fresh.into_iter().map(|r| (r, None)));
    if !requests.is_empty() {
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        let drawn: HashMap<u32, Entity> = actors.iter().map(|(e, v, ..)| (v.0, e)).collect();
        let mut held_up: HashSet<u32> = HashSet::new();
        for (request, since) in requests {
            let IdleRequest { actor, idle, source } = request;
            // Counted once, when asked for (not again after waiting for its clip).
            if since.is_none() {
                if !matches!(source, IdleSource::Fidget | IdleSource::FurnitureFidget | IdleSource::CombatIdle | IdleSource::Gesture | IdleSource::Cower | IdleSource::Stimpak) {
                    st.stats.asked += 1;
                }
                match source {
                    IdleSource::Script | IdleSource::Picked => st.stats.script += 1,
                    IdleSource::PackageBegin | IdleSource::PackageEnd | IdleSource::PackageChange => st.stats.package += 1,
                    IdleSource::PackageIdles | IdleSource::MarkerIdles => st.stats.collection += 1,
                    IdleSource::Speaker | IdleSource::Listener => st.stats.dialogue += 1,
                    IdleSource::Fidget | IdleSource::FurnitureFidget | IdleSource::CombatIdle | IdleSource::Gesture | IdleSource::Cower | IdleSource::Stimpak | IdleSource::PickUp | IdleSource::Standing | IdleSource::Panic => {}
                }
            }
            let name = game.data.name(actor);
            let failed = |st: &mut Idles, why: &str| {
                if let Some(c) = st.collections.get_mut(&actor.0).filter(|_| matches!(source, IdleSource::PackageIdles | IdleSource::MarkerIdles)) {
                    c.retry = RETRY_SECONDS;
                }
                debug!("idle: {name} does not play {} ({source:?}): {why}", game.data.name(idle));
            };
            if actor.0 != player_ref && !drawn.contains_key(&actor.0) {
                st.stats.not_drawn += 1;
                failed(st, "the actor is not drawn");
                continue;
            }
            // Its clip read on a worker thread rather than in this frame (#1429).
            let ready = clip_ready(&mut ctx, game, idle);
            let waited = since.map_or(std::time::Duration::ZERO, |t| t.elapsed());
            if held_up.contains(&actor.0) || waits_for_clip(ready, waited) {
                if since.is_none() && !ready {
                    st.stats.clip_waits += 1;
                }
                held_up.insert(actor.0);
                st.waiting.push((request, since.unwrap_or_else(std::time::Instant::now)));
                continue;
            }
            if since.is_some() {
                st.stats.longest_clip_wait_ms = st.stats.longest_clip_wait_ms.max(waited.as_secs_f64() * 1000.0);
            }
            if !ready {
                st.stats.clip_read_late += 1;
                debug!("idle: {name}: the clip of {} was not read in {} ms: read now", game.data.name(idle), CLIP_WAIT.as_millis());
            }
            let (clip, play) = match load(&mut ctx, game, idle, &mut st.rng, source == IdleSource::Standing) {
                Ok(l) => l,
                Err(why) => {
                    st.stats.no_clip += 1;
                    warn!("idle: {name} cannot play {} ({source:?}): {why}", game.data.name(idle));
                    failed(st, &why);
                    continue;
                }
            };
            let owner = owner_now(game, actor.0, source).filter(|_| source.of_package());
            let what = describe(game, idle, &play);
            if actor.0 == player_ref {
                let first = clip.tracks.iter().any(|(n, _)| n == CAMERA_NODE);
                if first && pi.held.is_none() {
                    pi.held = player.as_deref().map(|p| Held { feet: p.placed.unwrap_or(p.feet), free: p.feet, heading: p.heading, pitch: p.pitch });
                }
                let started = if first { &mut pi.first } else { &mut pi.third }.start(idle.0, source, owner, clip, play);
                match source {
                    IdleSource::Fidget => st.stats.player_fidgets += 1,
                    IdleSource::FurnitureFidget => st.stats.player_furniture_fidgets += 1,
                    _ => st.stats.player_started += 1,
                }
                let facing = player.as_deref().map_or(String::new(), |p| {
                    // A first-person idle plays where the player is held.
                    let at = pi.held.filter(|_| first).map_or(p.feet, |h| h.feet);
                    format!(", at ({:.1}, {:.1}, {:.1}) facing {:.0} degrees, pitch {:.0}", at[0], at[1], at[2], p.heading.to_degrees(), p.pitch.to_degrees())
                });
                let again = if started { "" } else { " again: not started over" };
                info!("idle: the player plays {what}{again} on the {} rig ({source:?}){facing}", if first { "first-person" } else { "third-person" });
                continue;
            }
            let Ok((e, _, rig, slot, lo, _)) = actors.get_mut(drawn[&actor.0]) else { continue };
            let section = game.data.idles.get(&idle).map_or(0, |d| d.section);
            let holds = holds_in_place(source, section, &clip, lo.and_then(walk_clip));
            let bound = bind_clip(&rig.skel, clip);
            // A stimpak plays on the upper body only (`npc_stimpaks.rs`).
            let bound = if source == IdleSource::Stimpak { crate::npc_stimpaks::upper_body(bound, &rig.skel.names) } else { bound };
            if bound.tracks.is_empty() {
                st.stats.no_clip += 1;
                failed(st, "its clip drives no node of the actor's skeleton");
                continue;
            }
            let bound = Arc::new(bound);
            let started = match slot {
                Some(mut s) => {
                    let started = s.0.start(idle.0, source, owner, bound, play);
                    if let Some(c) = s.0.cur.as_mut() {
                        c.holds = holds;
                    }
                    started
                }
                None => {
                    let mut s = ActorIdle::default();
                    s.0.start(idle.0, source, owner, bound, play);
                    if let Some(c) = s.0.cur.as_mut() {
                        c.holds = holds;
                    }
                    commands.entity(e).insert(s);
                    true
                }
            };
            match source {
                IdleSource::Fidget => st.stats.actor_fidgets += 1,
                IdleSource::FurnitureFidget => st.stats.actor_furniture_fidgets += 1,
                IdleSource::CombatIdle => st.stats.combat_idles += 1,
                IdleSource::Gesture | IdleSource::Cower | IdleSource::Stimpak => {}
                _ => st.stats.started += 1,
            }
            match source {
                IdleSource::PackageIdles | IdleSource::MarkerIdles | IdleSource::Gesture => debug!("idle: {name} plays {what} ({source:?})"),
                _ if !started => info!("idle: {name} plays {what} again: not started over ({source:?})"),
                _ => info!("idle: {name} plays {what} ({source:?}){}", if holds { ", its legs held" } else { "" }),
            }
        }
    }

    // Advance, and tell the rules what plays.
    let mut now: HashMap<u32, u32> = HashMap::new();
    let mut holding: HashSet<u32> = HashSet::new();
    let mut carried_off: HashSet<u32> = HashSet::new();
    for (e, v, _, idle, _, _) in &mut actors {
        let Some(mut idle) = idle else { continue };
        idle.0.advance(dt);
        // Held while it shows, its fade-out too (one gone is no longer `cur`).
        if let Some(p) = idle.0.cur.as_ref().filter(|p| p.holds) {
            holding.insert(v.0);
            if carries_body(&p.clip.clip) {
                carried_off.insert(v.0);
            }
        }
        match idle.0.playing() {
            // A script's animation group is not an idle.
            Some(p) if p.group.is_some() => {}
            Some(p) => {
                now.insert(v.0, p.idle);
            }
            None if idle.0.is_empty() => {
                commands.entity(e).remove::<ActorIdle>();
            }
            None => {}
        }
    }
    for slot in [&mut pi.first, &mut pi.third] {
        slot.advance(dt);
    }
    if let Some(p) = pi.first.playing().or(pi.third.playing()) {
        now.insert(player_ref, p.idle);
    }
    for (actor, idle) in &now {
        if st.told.get(actor) != Some(idle) {
            game.set_idle_playing(FormId(*actor), Some(FormId(*idle)));
        }
    }
    for actor in st.told.keys().filter(|a| !now.contains_key(a)) {
        game.set_idle_playing(FormId(*actor), None);
    }
    st.told = now;
    for a in holding.difference(&holds.held) {
        info!("idle hold: {} ({a:08X}) stands while its idle shows", game.data.name(FormId(*a)));
    }
    for a in holds.held.difference(&holding) {
        info!("idle hold: {} ({a:08X}) may walk again: its idle has gone", game.data.name(FormId(*a)));
    }
    holds.held = holding;
    holds.carried_off = carried_off;
}

/// Whether `actor` flees from a fight (`GetCurrentAIProcedure` 16).
fn fleeing(game: &Game, actor: u32) -> bool {
    let a = FormId(actor);
    game.awareness.state(a).in_combat() && game.awareness.tactics.flights.contains_key(&a)
}

/// Whether `actor` is on furniture, neither getting on nor off (`GetSitting` or
/// `GetSleeping` 3).
fn on_furniture(game: &Game, actor: u32) -> bool {
    game.furniture_use(FormId(actor)).is_some_and(|u| u.state == 3)
}

/// Whether the rules hold `actor`'s weapon out (`IsWeaponOut`).
fn weapon_out(game: &Game, actor: u32) -> bool {
    game.extra.get(&FormId(actor)).is_some_and(|e| e.weapon_out)
}

/// The animation folder of a skeleton below `meshes\`
/// (`meshes\characters\_male\skeleton.nif` → `characters\_male`).
fn skeleton_folder(skeleton: &str) -> Option<String> {
    let s = skeleton.to_ascii_lowercase().replace('/', "\\");
    let s = s.strip_prefix("meshes\\").unwrap_or(&s);
    Some(s.rsplit_once('\\')?.0.to_string())
}

/// The idle line of the run summary.
pub fn report(mut exit: MessageReader<AppExit>, st: Res<Idles>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = st.stats;
    info!(
        "idle summary: {} idles asked for ({} by scripts, {} by packages' begin, end and change, {} from collections, {} by dialogue lines); {} started on actors, {} on the player; {} for actors not drawn, {} without a usable clip",
        s.asked, s.script, s.package, s.collection, s.dialogue, s.started, s.player_started, s.not_drawn, s.no_clip
    );
    info!("fidget summary: {} drawn-weapon fidgets started on actors, {} on the player", s.actor_fidgets, s.player_fidgets);
    info!("creature combat idle summary: {} combat idles started on alerted creatures", s.combat_idles);
    info!(
        "idle clip summary: {} idles waited for their clip to be read on a worker thread (longest {:.0} ms); {} read in the frame they started after {} ms",
        s.clip_waits,
        s.longest_clip_wait_ms,
        s.clip_read_late,
        CLIP_WAIT.as_millis()
    );
    info!("furniture fidget summary: {} fidgets on furniture started on actors, {} on the player", s.actor_furniture_fidgets, s.player_furniture_fidgets);
    info!("talk gesture summary: {} gestures picked while talking, {} while listening; {} walks of the tree gave none", s.talk_gestures, s.listen_gestures, s.no_gesture);
    info!("cower summary: {} cower idles started on fleeing actors", s.cowers);
    if s.panics + s.no_panic > 0 {
        info!("panic summary: {} panic idles started on actors with a live explosive, {} with none for their kind", s.panics, s.no_panic);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_waits_for_its_clip_for_a_bounded_time() {
        use std::time::Duration;
        assert!(waits_for_clip(false, Duration::ZERO));
        assert!(waits_for_clip(false, CLIP_WAIT - Duration::from_millis(1)));
        // Past the bound it is read in the frame it starts; a clip in is never waited for.
        assert!(!waits_for_clip(false, CLIP_WAIT));
        assert!(!waits_for_clip(true, Duration::ZERO));
    }

    fn play(length: f32, blend_in: f32, passes: Option<u32>) -> IdlePlay {
        IdlePlay::new(IdleKeys { length, loop_range: None, blend_in, blend_out: 0.25 }, passes)
    }

    fn weights(s: &IdleSlot<u8>) -> Vec<(u8, f32)> {
        s.shown().map(|(p, w)| (p.clip, w)).collect()
    }

    #[test]
    fn a_new_idle_fades_in_over_the_one_before() {
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(1, IdleSource::Script, None, 1, play(2.0, 0.0, Some(1)));
        s.advance(0.5);
        assert_eq!(weights(&s), [(1, 1.0)]);
        assert!(s.covers());
        s.check_owner(|_| None);
        s.start(2, IdleSource::Script, None, 2, play(4.0, 1.0, None));
        s.advance(0.25);
        // Together they cover the rig in full.
        assert_eq!(weights(&s), [(2, 0.25), (1, 0.75)]);
        assert!(s.covers() && s.playing().unwrap().idle == 2);
        s.advance(0.75);
        assert_eq!(weights(&s), [(2, 1.0)]);
    }

    #[test]
    fn an_idle_started_in_the_same_step_is_replaced_outright() {
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(1, IdleSource::PackageIdles, None, 1, play(2.0, 0.0, None));
        s.advance(1.0);
        s.check_owner(|_| None);
        // A change idle, then the next package's begin idle, in one frame.
        s.start(2, IdleSource::PackageChange, None, 2, play(2.0, 0.5, None));
        s.start(3, IdleSource::PackageBegin, None, 3, play(2.0, 0.5, Some(1)));
        s.advance(0.25);
        assert_eq!(weights(&s), [(3, 0.5), (1, 0.5)]);
    }

    #[test]
    fn a_one_shot_idle_ends_fades_and_goes() {
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(1, IdleSource::Speaker, None, 1, play(1.0, 0.5, Some(1)));
        s.advance(0.25);
        // Fading in alone: the rig's other clips stay under it.
        assert!(!s.covers() && s.playing().is_some());
        s.advance(0.75);
        assert!(s.playing().is_none() && !s.is_empty());
        s.advance(0.125);
        assert_eq!(weights(&s), [(1, 0.5)]);
        s.advance(0.25);
        assert!(s.is_empty());
    }

    #[test]
    fn a_package_loop_is_left_when_the_package_is_no_longer_the_actors() {
        let (p, q) = (Some(FormId(1)), Some(FormId(2)));
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(1, IdleSource::PackageBegin, p, 1, play(1.0, 0.0, None));
        s.advance(0.5);
        s.check_owner(|_| p);
        assert!(s.playing().unwrap().play.endless());
        s.check_owner(|_| q);
        assert!(!s.playing().unwrap().play.endless());
        s.advance(0.75);
        assert!(s.playing().is_none());
        // A script's idle is nobody's package's.
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(1, IdleSource::Script, None, 1, play(1.0, 0.0, None));
        s.check_owner(|_| q);
        assert!(s.playing().unwrap().play.endless());
    }

    #[test]
    fn asked_again_by_a_line_an_idle_is_not_started_over() {
        // The opening: stage 30's PlayIdle, then 3 s later the next line's speaker idle,
        // the same, while the first holds its last pose and fades out.
        let mut s: IdleSlot<u8> = IdleSlot::default();
        assert!(s.start(7, IdleSource::Script, None, 1, play(3.0, 0.0, Some(1))));
        s.advance(3.03);
        s.check_owner(|_| None);
        assert!(!s.start(7, IdleSource::Speaker, None, 2, play(3.0, 0.0, Some(1))));
        assert_eq!(weights(&s), [(1, 1.0)]);
        assert_eq!(s.playing().unwrap().play.time(), 3.0);
        // A fidget started as the scripted idle ended (Doc's chair fidget, #1355): the
        // line's speaker idle brings the held one back, not a second hand-over.
        let mut s: IdleSlot<u8> = IdleSlot::default();
        assert!(s.start(7, IdleSource::Script, None, 1, play(3.0, 0.0, Some(1))));
        s.advance(3.01);
        s.check_owner(|_| None);
        assert!(s.start(9, IdleSource::FurnitureFidget, None, 3, play(16.0, 0.67, Some(1))));
        s.advance(0.02);
        assert!(!s.start(7, IdleSource::Speaker, None, 2, play(3.0, 0.0, Some(1))));
        assert_eq!(weights(&s), [(1, 1.0)]);
        assert_eq!(s.playing().unwrap().play.time(), 3.0);
        // Another asked idle over the fidget is a new one, as before.
        assert!(s.start(8, IdleSource::Speaker, None, 4, play(1.0, 0.0, Some(1))));
        // A collection picking the same idle again plays it again.
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(7, IdleSource::PackageIdles, None, 1, play(3.0, 0.0, Some(1)));
        s.advance(3.03);
        s.check_owner(|_| None);
        assert!(s.start(7, IdleSource::PackageIdles, None, 2, play(3.0, 0.0, Some(1))));
    }

    #[test]
    fn rolls_stay_below_the_bound() {
        let mut r = Rng(7);
        assert!((0..200).all(|_| r.below(5) < 5));
        assert_eq!(r.below(0), 0);
    }

    #[test]
    fn a_scene_idle_carries_the_body_off_its_reference() {
        use fnv_nif::anim::{Channel, TransformTrack};
        let clip = |at: [f32; 3]| Clip {
            name: String::new(), start: 0.0, stop: 9.97, frequency: 1.0, cycle_type: 0,
            tracks: vec![("Bip01 NonAccum".into(), Arc::new(TransformTrack { translation: Channel::Static(Some(at)), rotation: Channel::Static(None), euler: None, scale: Channel::Static(None) }))],
            priorities: vec![13], skipped: 0, accum_root: Some("Bip01".into()), text_keys: Vec::new(), visibility: Vec::new(), emitters: Vec::new(), materials: Vec::new(), free: Vec::new(), free_visibility: Vec::new(), lights: Vec::new(),
        };
        // Dad in the birth scene, against a body standing over its reference.
        assert!(carries_body(&clip([96.0, 110.6, 66.3])));
        assert!(!carries_body(&clip([0.66, 0.0, 79.1])));
    }

    #[test]
    fn held_follows_the_games_move() {
        // TTW's opening: the birth idle begins in the start room, the move lands after.
        let h = Held { feet: [2036.0, 1946.0, 7608.7], free: [2036.0, 1946.0, 7608.7], heading: 3.1, pitch: -0.2 };
        let to = [-5276.0, -7148.0, 7543.2];
        assert_eq!(h.moved(Some(to), 1.0, 0.1), Some(Held { feet: to, free: to, heading: 1.0, pitch: 0.1 }));
        // Where it was held (the opening's bed marker), or no placement: stays held there.
        assert_eq!(h.moved(Some([2036.4, 1946.0, 7608.7]), 1.0, 0.1), None);
        assert_eq!(h.moved(None, 1.0, 0.1), None);
    }
}
