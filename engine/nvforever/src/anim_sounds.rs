//! The sounds animations and doors name (docs/spec-notes/audio.md §10), in `--play`:
//!
//! - **Clips' text keys.** Every frame, for the player's rig in view (`player_body.rs`:
//!   the first-person arms rig while it is shown, else the body) and every drawn actor
//!   (`locomotion.rs`), the keys each clip passed since the last frame are read
//!   (`fnv_actor::anim_sounds`). Only clips with at least half the weight of their
//!   layer are heard, so two clips crossfading or blended by direction step once
//!   [design]. A clip first seen part-way through is not heard until it next passes a
//!   key, except one just started.
//!   - `Sound: <editor ID>` plays that SOUN: reloads, swings, swimming, sitting down,
//!     an idle's claps.
//!   - `Enum: Left` / `Right` play a footstep for the ground under the feet: the
//!     Havok material of what a ray down from the feet hits (a placed object's
//!     triangle, or the landscape texture showing most at that point: LTEX HNAM),
//!     taken to an `FST<family><gait><side>` record. The gait is the clip's (its
//!     `Armor…`/`Foley…` keys, else its name). Swimming is water. Ground with no
//!     material (a placed object's box or capsule) is concrete; nothing under the
//!     feet, or the default landscape texture, is dirt [design].
//!   - `Enum: ArmorWalk` … `FoleySneak` play the rustle of the armour worn (ARMO SNAM,
//!     its own or its template's), at its chance.
//! - **Doors** that open in place (`nvf_play::doors`): the sound keys their sequences
//!   pass, else the DOOR record's open (SNAM) or close (ANAM) sound as they start.
//! - **Creatures** (docs/spec-notes/audio.md §14): a creature whose record lists sounds
//!   (CREA `CSDT`/`CSDI`/`CSDC`, its own, else those of the creature it inherits sounds
//!   from, `CSCR`; of the creature a leveled list picked, else of the creature its model
//!   and animations come from, for a template-only creature such as `LvlFeralGhoul`) hears its `Enum:` keys as those
//!   types (`Left`, `Right`, `BackLeft`, `BackRight`, `Idle`, `Aware`, `Attack`,
//!   `Auxiliary1`–`8`): one of the type's sounds, at its chance [hypothesis: the keys
//!   name the types]. A hit plays its Hit sound and a death its Death sound
//!   (`nvf_play::combat::ActorHit`); the `Enum: Hit` keys of hit clips are not played
//!   again [design]. Nothing else plays a type: the original plays no Aware, Idle or
//!   Attack sound that no key names, nor the Securitrons' type-21 siren, on becoming
//!   aware, in combat, idling, after a crime alarm or on dying [observed 2026-11-06,
//!   observer batch o28, audio.md §16.1; issue #989]. A living creature in earshot sounds its Conscious Loop, and while it
//!   moves its Movement Loop, as `nvf_play::sfx::SoundSource`s (played by
//!   `nvf_play::ambient`); the loop's chance is rolled once per creature [design]. A
//!   creature without sounds steps like a person.
//! - **Jumps and landings**: leaving the ground going up faster than [`JUMP_SPEED`]
//!   plays the ground's `FST<family>Jump`, coming down after [`LAND_AIR`] seconds in
//!   the air its `FST<family>Land` (Dirt's when the family has none) [hypothesis: by the
//!   names]: the player's, and those of the actors that step like a person (people, and
//!   creatures without a sound list), told by `actor_jumps.rs` ([`ActorAir`]). A
//!   creature with sounds plays none: no creature lists a Jump sound (CSDT type 20)
//!   [observed: audio.md §14] [design].
//! - **Wading**: feet under the surface of water step as `FSTWater…` [design].
//!
//! The player's own sounds play at the listener, the others where the actor or door
//! is. Actors more than [`ACTOR_RANGE`] from the player are not heard [design].
//! At exit: `anim sounds summary: …`.

use crate::actor_jumps::{ActorAir, AirChange};
use crate::doors::DoorRig;
use crate::limbs::ActorRig;
use crate::player_body::PlayerBody;
use crate::world::bevy_to_game;
use bevy::prelude::*;
use fnv_actor::anim_sounds::{
    armour_foley, clip_gait, footstep_candidates, footstep_family, foley_for, land_texture_material, parse, passed, worn_foley, FoleySound, KeySound,
};
use fnv_combat::rng::SplitMix;
use fnv_phys::{layers, Filter, Key, Owner};
use fnv_sim::FormId;
use fnv_world::grass::Coverage;
use fnv_world::land::{load_land, LandParts};
use nvf_assets::anim_blend::Mode;
use nvf_assets::anim_play::{BlendPlayer, Clip};
use nvf_play::doors::DoorSound;
use nvf_play::phys::PhysState;
use nvf_play::player::{Player, BodySet};
use nvf_play::combat::{is_dead, ActorHit};
use nvf_play::script::ScriptState;
use nvf_play::sfx::{PlaySound, Sfx, SoundSource};
use fnv_actor::anim_sounds::{creature_key_type, creature_sound, creature_sound_type_name, creature_sounds, pick_creature_sound, CreatureSound};
use nvf_play::sim::SimState;
use nvf_play::{Location, PlayWorld};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Actors farther than this from the player's feet are not heard, game units [design:
/// most footstep records fade out well within it].
pub const ACTOR_RANGE: f32 = 3000.0;
/// A clip first seen less than this far into its time is heard from its start, seconds.
const JUST_STARTED: f32 = 0.1;
/// The share of its layer a clip needs to be heard.
const HEARD_WEIGHT: f32 = 0.5;
/// The shortest time between two footfalls (or two rustles) of one actor, seconds: two
/// movement clips crossfading can pass their keys a frame or two apart [design].
const MIN_STEP_GAP: f32 = 0.2;
/// Seconds in the air after which coming down is a landing with a sound [design].
pub const LAND_AIR: f32 = 0.4;
/// Upward speed on leaving the ground that is a jump, units/s [design].
const JUMP_SPEED: f32 = 100.0;
/// How deep the feet must be under a water surface to wade, units [design].
const WADE_DEPTH: f32 = 2.0;
/// Speed above which a creature moves (its Movement Loop sounds), units/s [design].
const MOVING: f32 = 20.0;
/// [`AnimSounds::step_due`] channels besides the creature sound types: a person's
/// footfall and rustle.
const FOOT: u32 = 100;
const RUSTLE: u32 = 101;
/// [`SoundSource::key`] kinds: a creature's conscious and movement loops.
const CONSCIOUS: u8 = 2;
const MOVEMENT: u8 = 3;

/// Whose clips: the player's rig (true: the first-person one) or an actor's.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Who {
    Player(bool),
    Actor(Entity),
}

/// A clip playing in a role: who, the clip, and the role (player layer tag, or the
/// actor player's slot is not used: a clip moves between slots as the blend goes on).
type Playing = (Who, usize, u32);

#[derive(Default)]
struct Counts {
    footsteps_player: u32,
    footsteps_actors: u32,
    rustles: u32,
    reload_keys: u32,
    other_keys: u32,
    door_sounds: u32,
    door_record_sounds: u32,
    /// Sound keys of groups scripts play on objects.
    object_sounds: u32,
    /// Keys naming a SOUN editor ID there is no record of.
    unknown_keys: u32,
    /// Footsteps by ground family.
    grounds: BTreeMap<&'static str, u32>,
    /// Creatures' sounds by type, keys of a type the creature does not list, hits and
    /// deaths with a sound.
    creature: BTreeMap<&'static str, u32>,
    creature_unlisted: u32,
    creature_hits: u32,
    creature_deaths: u32,
    /// The player's jumps and landings with a sound, and the actors'.
    jumps: u32,
    landings: u32,
    actor_jumps: u32,
    actor_landings: u32,
}

#[derive(Resource)]
pub struct AnimSounds {
    by_edid: HashMap<String, FormId>,
    built: bool,
    /// Clip time last frame, by clip playing.
    last: HashMap<Playing, f32>,
    /// Armour sounds by the worn list (player) or the reference (actors).
    worn: HashMap<Vec<FormId>, Arc<Vec<FoleySound>>>,
    actor_worn: HashMap<u32, Arc<Vec<FoleySound>>>,
    /// Landscape: per exterior cell, its texture coverage; per LTEX its material.
    land: HashMap<(u32, i32, i32), Option<Arc<Coverage>>>,
    ltex: HashMap<u32, Option<u32>>,
    unknown: HashSet<String>,
    rng: SplitMix,
    counts: Counts,
    /// Seconds since the start, this frame; the last footfall and rustle of each.
    now: f32,
    steps: HashMap<(Who, u32), f32>,
    /// Creatures' sounds by reference (`None`: not a creature; empty: none listed).
    creatures: HashMap<u32, Option<Arc<Vec<CreatureSound>>>>,
    /// Each creature's conscious loop, rolled against its chance once.
    conscious: HashMap<u32, Option<FormId>>,
    /// Where each creature was last frame.
    last_pos: HashMap<Entity, Vec3>,
    /// The player on the ground last frame, and seconds in the air then.
    air: Option<(bool, f32)>,
}

impl Default for AnimSounds {
    fn default() -> Self {
        AnimSounds {
            by_edid: HashMap::new(),
            built: false,
            last: HashMap::new(),
            worn: HashMap::new(),
            actor_worn: HashMap::new(),
            land: HashMap::new(),
            ltex: HashMap::new(),
            unknown: HashSet::new(),
            rng: SplitMix(0x0a11_5ed5),
            counts: Counts::default(),
            now: 0.0,
            steps: HashMap::new(),
            creatures: HashMap::new(),
            conscious: HashMap::new(),
            last_pos: HashMap::new(),
            air: None,
        }
    }
}

impl AnimSounds {
    /// Whether a footfall, a rustle or a creature sound (`channel`: [`FOOT`], [`RUSTLE`]
    /// or the creature sound type) of `who` may sound now; marks it if so.
    fn step_due(&mut self, who: Who, channel: u32) -> bool {
        if self.steps.get(&(who, channel)).is_some_and(|&t| self.now - t < MIN_STEP_GAP) {
            return false;
        }
        self.steps.insert((who, channel), self.now);
        true
    }

    /// The first of these SOUN editor IDs there is a record of.
    fn first_of(&self, names: &[String]) -> Option<FormId> {
        names.iter().find_map(|n| self.by_edid.get(&n.to_ascii_lowercase()).copied())
    }

    fn sound(&mut self, edid: &str) -> Option<FormId> {
        let s = self.by_edid.get(&edid.to_ascii_lowercase()).copied();
        if s.is_none() {
            self.counts.unknown_keys += 1;
            if self.unknown.insert(edid.to_string()) {
                debug!("anim sounds: no SOUN {edid}");
            }
        }
        s
    }
}

/// The SOUN editor IDs, once the sound forms are read.
fn build(s: &mut AnimSounds, sfx: &Sfx) {
    if s.built {
        return;
    }
    s.by_edid = sfx.forms.sounds.iter().filter_map(|(&id, snd)| Some((snd.edid.as_deref()?.to_ascii_lowercase(), id))).collect();
    s.built = true;
}

/// One heard key, with what it needs.
struct Heard<'a> {
    who: Who,
    clip: &'a Clip,
    key: KeySound<'a>,
}

/// The keys `clip` passed going to clip time `now` in role `playing`; `fresh` when the
/// clip may have just started. A clip played `once` that went back has been started
/// again (a round-by-round reload restarts its clip at `Hit` for the next round): heard
/// from its start, not round its end.
fn keys_of<'a>(last: &HashMap<Playing, f32>, seen: &mut Vec<(Playing, f32)>, playing: Playing, clip: &'a Clip, now: f32, once: bool) -> Vec<&'a str> {
    seen.push((playing, now));
    let from = match last.get(&playing) {
        Some(&t) if once && now < t => None,
        Some(&t) => Some(t),
        None if now - clip.start <= JUST_STARTED => None,
        None => return Vec::new(),
    };
    if from == Some(now) {
        return Vec::new();
    }
    passed(&clip.text_keys, clip.start, from, now).collect()
}

/// What is underfoot at `feet` (game space): the footstep family, stairs, and a name for
/// the report.
fn ground(s: &mut AnimSounds, phys: &mut PhysState, d: &PlayWorld, world: u32, feet: [f32; 3], exclude: Vec<Key>) -> (&'static str, bool) {
    // Wading: the feet under a water surface.
    if phys.world.water_surface_for(feet).is_some_and(|w| w > feet[2] + WADE_DEPTH) {
        return ("Water", false);
    }
    let filter = Filter { exclude, exclude_layers: layers::character_ignores(), ..Default::default() };
    let from = [feet[0], feet[1], feet[2] + 30.0];
    let Some(hit) = phys.world.queries().ray(from, [0.0, 0.0, -1.0], 90.0, &filter) else { return ("Dirt", false) };
    match hit.owner {
        Owner::Terrain { x, y } => {
            let code = terrain_material(s, d, world, x, y, hit.point);
            code.map_or(("Dirt", false), footstep_family)
        }
        owner => {
            trace!("anim sounds: underfoot {owner:?} material {:?}", hit.material);
            hit.material.map_or(("ConcSolid", false), footstep_family)
        }
    }
}

/// The Havok material of the landscape texture showing most at `p` in exterior cell
/// (`x`, `y`); `None` for the default texture.
fn terrain_material(s: &mut AnimSounds, d: &PlayWorld, world: u32, x: i32, y: i32, p: [f32; 3]) -> Option<u32> {
    let cover = s
        .land
        .entry((world, x, y))
        .or_insert_with(|| {
            let cell = d.index.worldspaces.get(&world)?.grid.get(&(x, y))?;
            let land = d.index.cells.get(cell)?.land?;
            let parts = LandParts { heights: false, normals: false, colors: false, layers: true };
            load_land(d.lo, d.schema, land, parts).map(|l| Arc::new(Coverage::new(&l)))
        })
        .clone()?;
    let size = fnv_world::CELL_SIZE;
    let (lx, ly) = (p[0] - x as f32 * size, p[1] - y as f32 * size);
    let (best, share) = cover.textures().into_iter().map(|t| (t, cover.at(t, lx, ly))).fold((None, 0.0f32), |b, (t, a)| if a > b.1 { (Some(t), a) } else { b });
    // The default texture shows where the named ones leave more than any one of them.
    let best = best.filter(|_| share >= 1.0 - share)?;
    *s.ltex.entry(best).or_insert_with(|| land_texture_material(d.lo, d.schema, best))
}

/// The armour sounds of a worn list.
fn foley_of(lo: &fnv_esm::LoadOrder, schema: &fnv_esm::Schema, worn: &[FormId]) -> Vec<FoleySound> {
    worn_foley(worn.iter().map(|a| armour_foley(lo, schema, a.0)).collect())
}

#[allow(clippy::too_many_arguments)]
fn sound_heard(
    s: &mut AnimSounds,
    out: &mut MessageWriter<PlaySound>,
    phys: &mut Option<ResMut<PhysState>>,
    d: &PlayWorld,
    world: u32,
    h: Heard,
    at: Option<[f32; 3]>,
    feet: [f32; 3],
    exclude: Vec<Key>,
    foley: &[FoleySound],
    swimming: bool,
) {
    match h.key {
        KeySound::Sound(edid) => {
            if h.clip.name.to_ascii_lowercase().starts_with("reload") {
                s.counts.reload_keys += 1;
            } else {
                s.counts.other_keys += 1;
            }
            if let Some(sound) = s.sound(edid) {
                out.write(PlaySound { sound, at });
            }
        }
        KeySound::Foot(side) => {
            if !s.step_due(h.who, FOOT) {
                return;
            }
            let gait = clip_gait(&h.clip.text_keys, &h.clip.name);
            let (family, stairs) = match (swimming, phys.as_deref_mut()) {
                (true, _) => ("Water", false),
                (false, Some(p)) => ground(s, p, d, world, feet, exclude),
                (false, None) => ("Dirt", false),
            };
            let Some(sound) = footstep_candidates(family, stairs, gait, side).iter().find_map(|n| s.by_edid.get(&n.to_ascii_lowercase()).copied()) else {
                s.counts.unknown_keys += 1;
                return;
            };
            match h.who {
                Who::Player(_) => s.counts.footsteps_player += 1,
                Who::Actor(_) => s.counts.footsteps_actors += 1,
            }
            *s.counts.grounds.entry(family).or_default() += 1;
            debug!("anim sounds: {:?} footstep {family}{} {:?} {side:?}", h.who, if stairs { " stairs" } else { "" }, gait);
            out.write(PlaySound { sound, at });
        }
        KeySound::Foley { armor, gait } => {
            let Some(f) = foley_for(foley, armor, gait) else { return };
            if !s.step_due(h.who, RUSTLE) {
                return;
            }
            if f.chance < 100 && s.rng.next_f32() * 100.0 >= f32::from(f.chance) {
                return;
            }
            s.counts.rustles += 1;
            debug!("anim sounds: {:?} rustle {} {gait:?}", h.who, if armor { "armour" } else { "cloth" });
            out.write(PlaySound { sound: FormId(f.sound), at });
        }
    }
}

/// The player's rig in view.
#[allow(clippy::too_many_arguments)]
fn player_keys(
    time: Res<Time>,
    mut s: ResMut<AnimSounds>,
    sfx: Option<Res<Sfx>>,
    body: Option<Res<PlayerBody>>,
    player: Option<Res<Player>>,
    mut phys: Option<ResMut<PhysState>>,
    d: Res<PlayWorld>,
    location: Res<Location>,
    mut out: MessageWriter<PlaySound>,
    mut seen: Local<Vec<(Playing, f32)>>,
) {
    let (Some(sfx), Some(body), Some(player)) = (sfx, body, player) else { return };
    let s = &mut *s;
    build(s, &sfx);
    s.now = time.elapsed_secs();
    jumps_and_landings(s, &mut out, &mut phys, &d, location.world, &player);
    let Some((a, first, worn)) = body.heard() else { return };
    let foley = match s.worn.get(worn) {
        Some(f) => f.clone(),
        None => {
            let f = Arc::new(foley_of(d.lo, d.schema, worn));
            s.worn.insert(worn.to_vec(), f.clone());
            f
        }
    };
    let who = Who::Player(first);
    seen.clear();
    let mut heard: Vec<(&Clip, KeySound)> = Vec::new();
    for l in a.layers.iter().filter(|l| l.weight * l.ease >= HEARD_WEIGHT) {
        let playing = (who, Arc::as_ptr(&l.clip) as usize, l.tag);
        for k in keys_of(&s.last, &mut seen, playing, &l.clip, l.sample_time(a.phase), l.mode == Mode::Once).into_iter().filter_map(parse) {
            heard.push((&l.clip, k));
        }
    }
    s.last.retain(|(w, _, _), _| !matches!(w, Who::Player(_)));
    s.last.extend(seen.drain(..));
    let swimming = phys.as_deref().and_then(|p| p.player.as_ref()).is_some_and(|c| c.swimming);
    let exclude: Vec<Key> = phys.as_deref().and_then(|p| p.player.as_ref()).map(|c| c.key()).into_iter().collect();
    let mut stepped = HashSet::new();
    for (clip, key) in heard {
        // One footfall a side a frame, whatever the clips.
        if let KeySound::Foot(side) = key {
            if !stepped.insert(side) {
                continue;
            }
        }
        sound_heard(s, &mut out, &mut phys, &d, location.world, Heard { who, clip, key }, None, player.feet, exclude.clone(), &foley, swimming);
    }
}

/// The drawn actors near the player.
#[allow(clippy::too_many_arguments)]
fn actor_keys(
    time: Res<Time>,
    mut s: ResMut<AnimSounds>,
    sfx: Option<Res<Sfx>>,
    player: Option<Res<Player>>,
    mut phys: Option<ResMut<PhysState>>,
    d: Res<PlayWorld>,
    location: Res<Location>,
    actor_world: Option<Res<crate::actors::ActorWorld>>,
    actors: Query<(Entity, &BlendPlayer, &GlobalTransform, Option<&ActorRig>)>,
    script: Option<Res<ScriptState>>,
    mut out: MessageWriter<PlaySound>,
    mut loops: MessageWriter<SoundSource>,
    mut seen: Local<Vec<(Playing, f32)>>,
) {
    let (Some(sfx), Some(player)) = (sfx, player) else { return };
    let s = &mut *s;
    build(s, &sfx);
    s.now = time.elapsed_secs();
    let dt = time.delta_secs();
    seen.clear();
    let listener = Vec3::from_array(player.feet);
    let mut moved: HashMap<Entity, Vec3> = HashMap::new();
    for (e, bp, gt, rig) in &actors {
        let feet = bevy_to_game(gt.translation());
        if feet.distance(listener) > ACTOR_RANGE {
            continue;
        }
        let who = Who::Actor(e);
        // A creature's sounds, and its loops while it lives.
        let creature = rig.and_then(|r| creature_list(s, &d, actor_world.as_deref(), r)).filter(|l| !l.is_empty());
        let before = s.last_pos.get(&e).copied();
        moved.insert(e, feet);
        if let (Some(list), Some(r)) = (&creature, rig) {
            if !is_dead(script.as_deref(), r.reference) {
                let conscious = *s.conscious.entry(r.reference).or_insert_with(|| {
                    let (pick, roll) = (s.rng.next_f32(), s.rng.next_f32());
                    pick_creature_sound(list, creature_sound::CONSCIOUS_LOOP, pick, roll).map(FormId)
                });
                if let Some(sound) = conscious {
                    loops.write(SoundSource { key: SoundSource::key(CONSCIOUS, r.reference), sound, at: feet.to_array(), looping: false });
                }
                let speed = before.map_or(0.0, |b| b.distance(feet) / dt.max(1e-3));
                if let (true, Some(m)) = (speed > MOVING, list.iter().find(|c| c.kind == creature_sound::MOVEMENT_LOOP)) {
                    loops.write(SoundSource { key: SoundSource::key(MOVEMENT, r.reference), sound: FormId(m.sound), at: feet.to_array(), looping: false });
                }
            }
        }
        let mut heard: Vec<(&Clip, KeySound)> = Vec::new();
        for l in bp.layers.iter().flatten().filter(|l| l.weight >= HEARD_WEIGHT) {
            let clip = &l.clip.clip;
            let now = clip.start + l.phase.clamp(0.0, 1.0) * clip.length();
            for k in keys_of(&s.last, &mut seen, (who, Arc::as_ptr(clip) as usize, 0), clip, now, false) {
                // A creature with sounds hears its `Enum:` keys as its sound types.
                if let (Some(list), Some(kind)) = (&creature, creature_key_type(k)) {
                    creature_heard(s, &mut out, who, kind, list, feet.to_array());
                    continue;
                }
                if let Some(k) = parse(k) {
                    heard.push((clip, k));
                }
            }
        }
        if heard.is_empty() {
            continue;
        }
        let reference = rig.map(|r| r.reference);
        let foley = match (reference, actor_world.as_deref()) {
            (Some(r), Some(w)) => s
                .actor_worn
                .entry(r)
                .or_insert_with(|| {
                    let worn = rig.and_then(|g| w.resolve(g.base, r).ok()).map(|a| a.worn).unwrap_or_default();
                    Arc::new(foley_of(d.lo, d.schema, &worn))
                })
                .clone(),
            _ => Arc::new(Vec::new()),
        };
        let exclude: Vec<Key> = reference.map(|r| vec![Key::Actor(r), Key::Character(r)]).unwrap_or_default();
        let swimming = reference.and_then(|r| phys.as_deref().and_then(|p| p.npcs.get(&r))).is_some_and(|n| n.ch.swimming);
        let mut stepped = HashSet::new();
        for (clip, key) in heard {
            if let KeySound::Foot(side) = key {
                if !stepped.insert(side) {
                    continue;
                }
            }
            let at = feet.to_array();
            sound_heard(s, &mut out, &mut phys, &d, location.world, Heard { who, clip, key }, Some(at), at, exclude.clone(), &foley, swimming);
        }
    }
    s.last.retain(|(w, _, _), _| matches!(w, Who::Player(_)));
    s.last.extend(seen.drain(..));
    s.last_pos = moved;
}

/// The sounds a creature lists, by reference (see the module notes); `None` for one
/// that is not a creature.
fn creature_list(s: &mut AnimSounds, d: &PlayWorld, w: Option<&crate::actors::ActorWorld>, rig: &ActorRig) -> Option<Arc<Vec<CreatureSound>>> {
    s.creatures
        .entry(rig.reference)
        .or_insert_with(|| {
            let crea = fnv_world::fields::winner(d.lo, rig.base).is_some_and(|(_, r)| &r.header.sig == b"CREA");
            if !crea {
                return None;
            }
            // The creature its model and animations come from: "Use Model/Animation
            // includes the Blood, Sounds, and Destruction tabs" [documented: GECK wiki
            // Category:NPC, Template Data; hypothesis: creatures alike]. The creature a leveled list picked when it has
            // no template for them; the template's otherwise (`LvlFeralGhoul` lists none
            // itself; `FriendshipMetroMetroRoach` lists its own, hidden by the flag, and
            // takes its template's [observed: docs/spec-notes/actors.md §1.3]). The one
            // placed when the creature cannot be resolved.
            let picked = w.and_then(|w| w.resolve_creature(rig.base, rig.reference).ok());
            let mut list = picked.as_ref().map(|c| creature_sounds(d.lo, d.schema, c.model_source.0).0).unwrap_or_default();
            if list.is_empty() && picked.is_none() {
                list = creature_sounds(d.lo, d.schema, rig.base).0;
            }
            debug!("anim sounds: creature {:08X} lists {} sounds", rig.reference, list.len());
            Some(Arc::new(list))
        })
        .clone()
}

/// A creature's key of sound type `kind`: one of its sounds of that type, at its chance.
fn creature_heard(s: &mut AnimSounds, out: &mut MessageWriter<PlaySound>, who: Who, kind: u32, list: &[CreatureSound], at: [f32; 3]) {
    // Hits sound when the hit lands (`creature_hits`).
    if kind == creature_sound::HIT || !s.step_due(who, kind) {
        return;
    }
    if !list.iter().any(|c| c.kind == kind) {
        s.counts.creature_unlisted += 1;
        return;
    }
    let (pick, roll) = (s.rng.next_f32(), s.rng.next_f32());
    let Some(sound) = pick_creature_sound(list, kind, pick, roll) else { return };
    *s.counts.creature.entry(creature_sound_type_name(kind)).or_default() += 1;
    debug!("anim sounds: {who:?} creature {}", creature_sound_type_name(kind));
    out.write(PlaySound { sound: FormId(sound), at: Some(at) });
}

/// Creatures hit or killed: their Hit or Death sound.
#[allow(clippy::too_many_arguments)]
fn creature_hits(
    mut s: ResMut<AnimSounds>,
    mut hits: MessageReader<ActorHit>,
    d: Res<PlayWorld>,
    actor_world: Option<Res<crate::actors::ActorWorld>>,
    actors: Query<(&ActorRig, &GlobalTransform)>,
    mut out: MessageWriter<PlaySound>,
) {
    let s = &mut *s;
    for h in hits.read() {
        let Some((rig, gt)) = actors.iter().find(|(r, _)| r.reference == h.form_id) else { continue };
        let Some(list) = creature_list(s, &d, actor_world.as_deref(), rig).filter(|l| !l.is_empty()) else { continue };
        let kind = if h.died { creature_sound::DEATH } else { creature_sound::HIT };
        let (pick, roll) = (s.rng.next_f32(), s.rng.next_f32());
        let Some(sound) = pick_creature_sound(&list, kind, pick, roll) else { continue };
        if h.died {
            s.counts.creature_deaths += 1;
        } else {
            s.counts.creature_hits += 1;
        }
        debug!("anim sounds: creature {:08X} {}", h.form_id, creature_sound_type_name(kind));
        out.write(PlaySound { sound: FormId(sound), at: Some(bevy_to_game(gt.translation()).to_array()) });
    }
}

/// The player leaving the ground going up (a jump) or coming down after a while in the
/// air (a landing): the ground's `FST…Jump` / `FST…Land` sound, at the listener.
fn jumps_and_landings(s: &mut AnimSounds, out: &mut MessageWriter<PlaySound>, phys: &mut Option<ResMut<PhysState>>, d: &PlayWorld, world: u32, player: &Player) {
    let Some(p) = phys.as_deref_mut() else { return };
    let Some((grounded, vz, air, swimming, key)) = p.player.as_ref().map(|c| (c.grounded, c.vertical_speed, c.air_time, c.swimming, c.key())) else { return };
    let was = s.air.replace((grounded, air));
    let Some((was_grounded, was_air)) = was else { return };
    let landing = if was_grounded && !grounded && vz > JUMP_SPEED {
        false
    } else if !was_grounded && grounded && was_air >= LAND_AIR {
        true
    } else {
        return;
    };
    if swimming {
        return;
    }
    let Some(family) = jump_sound(s, out, p, d, world, player.feet, vec![key], landing, None) else { return };
    if landing {
        s.counts.landings += 1;
    } else {
        s.counts.jumps += 1;
    }
    debug!("anim sounds: the player {} on {family}", if landing { "lands" } else { "jumps" });
}

/// The ground's `FST…Jump` (`landing` false) or `FST…Land` sound under `feet`, played
/// `at` (`None`: at the listener); the ground's family, `None` when no record is found.
#[allow(clippy::too_many_arguments)]
fn jump_sound(s: &mut AnimSounds, out: &mut MessageWriter<PlaySound>, p: &mut PhysState, d: &PlayWorld, world: u32, feet: [f32; 3], exclude: Vec<Key>, landing: bool, at: Option<[f32; 3]>) -> Option<&'static str> {
    let what = if landing { "Land" } else { "Jump" };
    let (family, _) = ground(s, p, d, world, feet, exclude);
    let sound = s.first_of(&[format!("FST{family}{what}"), format!("FSTDirt{what}")])?;
    out.write(PlaySound { sound, at });
    Some(family)
}

/// Actors leaving the ground going up or coming down after a while in the air
/// (`actor_jumps.rs`), by the player's rules: their ground's jump or landing sound,
/// where they are. Only actors that step like a person (see the module notes).
#[allow(clippy::too_many_arguments)]
fn actor_jumps(
    mut s: ResMut<AnimSounds>,
    sfx: Option<Res<Sfx>>,
    mut air: MessageReader<ActorAir>,
    player: Option<Res<Player>>,
    mut phys: Option<ResMut<PhysState>>,
    d: Res<PlayWorld>,
    location: Res<Location>,
    actor_world: Option<Res<crate::actors::ActorWorld>>,
    rigs: Query<&ActorRig>,
    mut out: MessageWriter<PlaySound>,
) {
    let (Some(sfx), Some(player), Some(p)) = (sfx, player, phys.as_deref_mut()) else {
        air.clear();
        return;
    };
    let s = &mut *s;
    build(s, &sfx);
    let listener = Vec3::from_array(player.feet);
    for m in air.read() {
        let landing = match m.change {
            AirChange::TakeOff { rising } if rising > JUMP_SPEED => false,
            AirChange::Landed { air } if air >= LAND_AIR => true,
            _ => continue,
        };
        if Vec3::from_array(m.feet).distance(listener) > ACTOR_RANGE {
            continue;
        }
        // Coming up out of water or falling into it is no jump on the ground.
        if p.npcs.get(&m.reference).is_some_and(|n| n.ch.swimming) {
            continue;
        }
        let Ok(rig) = rigs.get(m.actor) else { continue };
        if creature_list(s, &d, actor_world.as_deref(), rig).is_some_and(|l| !l.is_empty()) {
            continue;
        }
        let exclude = vec![Key::Actor(m.reference), Key::Character(m.reference)];
        let Some(family) = jump_sound(s, &mut out, p, &d, location.world, m.feet, exclude, landing, Some(m.feet)) else { continue };
        if landing {
            s.counts.actor_landings += 1;
        } else {
            s.counts.actor_jumps += 1;
        }
        info!("anim sounds: actor {:08X} {} on {family}", m.reference, if landing { "lands" } else { "jumps" });
    }
}

/// Doors' sounds, where the door is drawn.
fn door_sounds(
    mut s: ResMut<AnimSounds>,
    sfx: Option<Res<Sfx>>,
    mut doors: MessageReader<DoorSound>,
    rigs: Query<(&DoorRig, &GlobalTransform)>,
    sim: Option<Res<SimState>>,
    d: Res<PlayWorld>,
    mut out: MessageWriter<PlaySound>,
) {
    let Some(sfx) = sfx else {
        doors.clear();
        return;
    };
    let s = &mut *s;
    build(s, &sfx);
    for m in doors.read() {
        let at = rigs.iter().find(|(r, _)| r.form_id == m.door).map(|(_, g)| bevy_to_game(g.translation()).to_array());
        let sound = match &m.key {
            Some(edid) => {
                let found = s.sound(edid);
                s.counts.door_sounds += u32::from(found.is_some());
                found
            }
            None => {
                let base = sim.as_deref().and_then(|sim| sim.refs.get(FormId(m.door))).map(|p| p.base.0);
                let found = base.and_then(|b| door_record_sound(&d, b, m.opening));
                s.counts.door_record_sounds += u32::from(found.is_some());
                found
            }
        };
        if let Some(sound) = sound {
            debug!("anim sounds: door 0x{:08X} {} {:?}", m.door, if m.opening { "opens" } else { "closes" }, m.key);
            out.write(PlaySound { sound, at });
        }
    }
}

/// The sound keys of animation groups scripts play on objects (object_anims.rs), where
/// the object is.
fn object_sounds(mut s: ResMut<AnimSounds>, sfx: Option<Res<Sfx>>, mut keys: MessageReader<crate::object_anims::ObjectSound>, mut out: MessageWriter<PlaySound>) {
    let Some(sfx) = sfx else {
        keys.clear();
        return;
    };
    let s = &mut *s;
    build(s, &sfx);
    for k in keys.read() {
        // A key naming no SOUN is counted by `sound`.
        if let Some(sound) = s.sound(&k.key) {
            s.counts.object_sounds += 1;
            debug!("anim sounds: object key {} at {:?}", k.key, k.at);
            out.write(PlaySound { sound, at: Some(k.at) });
        }
    }
}

/// A DOOR record's open (SNAM) or close (ANAM) sound [documented: xEdit
/// wbDefinitionsFNV.pas DOOR, `schemas/fnv/objects.json`].
fn door_record_sound(d: &PlayWorld, base: u32, opening: bool) -> Option<FormId> {
    let (pi, r) = fnv_world::fields::winner(d.lo, base)?;
    let f = fnv_world::fields::Fields::new(r, pi, d.schema).ok()?;
    if &f.sig != b"DOOR" {
        return None;
    }
    f.formid(if opening { b"SNAM" } else { b"ANAM" }, d.lo).map(FormId)
}

fn report(mut exit: MessageReader<AppExit>, s: Res<AnimSounds>) {
    if exit.read().last().is_none() {
        return;
    }
    let c = &s.counts;
    let grounds: Vec<String> = c.grounds.iter().map(|(k, n)| format!("{k} {n}")).collect();
    let mut unknown: Vec<&String> = s.unknown.iter().collect();
    unknown.sort();
    info!(
        "anim sounds summary: {} footsteps ({} by the player, {} by actors; ground: {}), {} armour rustles, {} reload keys, {} other sound keys, {} door sounds ({} from door records), {} object group sounds, {} keys without a SOUN{}; creatures: {} sounds ({}), {} hits, {} deaths, {} keys of a type not listed; the player jumped {} and landed {} times; actors jumped {} and landed {} times",
        c.footsteps_player + c.footsteps_actors,
        c.footsteps_player,
        c.footsteps_actors,
        if grounds.is_empty() { "-".to_string() } else { grounds.join(", ") },
        c.rustles,
        c.reload_keys,
        c.other_keys,
        c.door_sounds + c.door_record_sounds,
        c.door_record_sounds,
        c.object_sounds,
        c.unknown_keys,
        if unknown.is_empty() { String::new() } else { format!(" ({})", unknown.iter().take(8).map(|u| u.as_str()).collect::<Vec<_>>().join(", ")) },
        c.creature.values().sum::<u32>(),
        if c.creature.is_empty() { "-".to_string() } else { c.creature.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(", ") },
        c.creature_hits,
        c.creature_deaths,
        c.creature_unlisted,
        c.jumps,
        c.landings,
        c.actor_jumps,
        c.actor_landings
    );
}

/// Added by nvforever with `--play`.
pub struct AnimSoundsPlugin;

impl Plugin for AnimSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnimSounds>()
            .add_systems(
                Update,
                (player_keys.after(BodySet), actor_keys.after(crate::locomotion::drive), actor_jumps, door_sounds.after(nvf_play::doors::DoorSystems), creature_hits).chain().before(nvf_play::sfx::SfxSet),
            )
            .add_systems(
                Update,
                object_sounds.after(crate::object_anims::drive).before(nvf_play::sfx::SfxSet).run_if(|m: Option<Res<crate::object_anims::ObjectAnims>>| m.is_some()),
            )
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_actor::anim_sounds::Side;

    fn clip(keys: &[(f32, &str)]) -> Clip {
        Clip {
            name: "Forward".into(),
            start: 0.0,
            stop: 1.0,
            frequency: 1.0,
            cycle_type: 0,
            tracks: Vec::new(),
            priorities: Vec::new(),
            skipped: 0,
            accum_root: None,
            text_keys: keys.iter().map(|&(t, k)| (t, k.to_string())).collect(),
            visibility: Vec::new(),
            emitters: Vec::new(),
            materials: Vec::new(),
            free: Vec::new(),
            free_visibility: Vec::new(),
            lights: Vec::new(),
        }
    }

    #[test]
    fn a_clip_is_heard_from_where_it_was_seen() {
        let c = clip(&[(0.0, "Sound: A"), (0.4, "Enum: Left"), (0.9, "Enum: Right")]);
        let mut seen = Vec::new();
        let p: Playing = (Who::Player(true), 1, 10);
        // Just started: heard from its start.
        let last = HashMap::new();
        fn parsed(v: Vec<&str>) -> Vec<KeySound<'_>> {
            v.into_iter().filter_map(parse).collect()
        }
        assert_eq!(parsed(keys_of(&last, &mut seen, p, &c, 0.05, false)), vec![KeySound::Sound("A")]);
        // First seen part-way: nothing until the next frame.
        assert!(keys_of(&last, &mut seen, p, &c, 0.5, false).is_empty());
        let last: HashMap<Playing, f32> = [(p, 0.3)].into();
        assert_eq!(parsed(keys_of(&last, &mut seen, p, &c, 0.5, false)), vec![KeySound::Foot(Side::Left)]);
        // Round the end of a loop.
        let last: HashMap<Playing, f32> = [(p, 0.8)].into();
        assert_eq!(parsed(keys_of(&last, &mut seen, p, &c, 0.1, false)), vec![KeySound::Sound("A"), KeySound::Foot(Side::Right)]);
        // Held at the end: nothing again.
        let last: HashMap<Playing, f32> = [(p, 1.0)].into();
        assert!(keys_of(&last, &mut seen, p, &c, 1.0, false).is_empty());
        // A clip played once, started again part-way (the next round of a reload): heard
        // from its start, not round its end.
        let last: HashMap<Playing, f32> = [(p, 0.6)].into();
        assert_eq!(parsed(keys_of(&last, &mut seen, p, &c, 0.05, true)), vec![KeySound::Sound("A")]);
    }
}
