//! The player's attacks with ranged weapons (docs/design/combat.md step C4), on the
//! headless rules of `fnv-combat` (docs/spec-notes/combat-rules.md):
//!
//! - **Weapon and ammo.** The equipped weapon (simulation) and its loaded ammo: the
//!   first of the weapon's ammo list the player carries (`CombatForms::ammo_for`). The
//!   magazine is part of the carried count, as rounds in the inventory; a shot removes
//!   `ammo_use` rounds from the inventory. A weapon with no ammo of its own but a
//!   projectile (grenades, thrown weapons) spends itself. A newly equipped weapon starts
//!   loaded with what is carried [design].
//! - **The cycle** (`fnv_combat::attack::Cycle`): Use is the trigger, a Ready Item tap
//!   reloads, Ammo Swap moves down the ammo list and reloads [design]. It runs only once
//!   the player body says the weapon is out ([`WeaponAnim::ready`]); the body plays the
//!   attack and reload clips when [`WeaponAnim::attacks`] / [`WeaponAnim::reloads`] count
//!   up and gives their timing back.
//! - **Shots** leave the eye along the view, inside the spread cone (the documented New
//!   Vegas spread plus bloom, which builds while shots follow each other and shrinks once
//!   the weapon rests); recoil climbs the view up in a random pattern and stays there
//!   for the player to pull down (`fnv_combat::spread::Kick`) [design]. They are
//!   drawn from the muzzle of the held weapon ([`WeaponAnim::muzzle`], the model's
//!   `ProjectileNode`), closing on the view ray by the point aimed at; in third person a
//!   shot with something between the muzzle and that point leaves the muzzle instead
//!   (`fnv_combat::shot::leaving`; docs/design/combat.md §5.1).
//! - **Projectiles** fly through the physics world (`fnv_phys` rays; actor hit volumes,
//!   combat C3).
//! - **Melee and unarmed** (step C6, `fnv_combat::melee`): a tap of Use swings, and taps
//!   during a swing chain the next at the clip's `a:` key (right, left, right… by the
//!   keys); holding it [`CombatTuning::power_hold`] makes a power attack whose clip
//!   follows the movement keys (standing, forward, back, left, right) and whose damage is
//!   that direction's `fDamagePowerAttack…Bonus`. The body plays [`WeaponAnim::swing`]
//!   and times it ([`WeaponAnim::timed`]); the swing strikes once at the clip's `Hit` key
//!   [documented: NiTextKeyExtraData] with a fan of rays from the eye out to the reach
//!   (`damage::melee_reach`, which reaches from the body: plus the player's capsule
//!   radius from the eye [hypothesis, as the NPCs' 45 + 107 × Reach is read,
//!   combat-rules.md §10]), and at any actor whose centre is within that reach across
//!   the ground, low or high (`projectile::swing_reaching`: a gecko at the player's feet). With no weapon equipped the player fights with the
//!   Fists (WEAP `Fists`) [hypothesis: the game's unarmed weapon form]. Knockback force
//!   is computed (documented formula) and logged; living actors are not pushed until they
//!   move physically. Every shot and blow that lands on an actor also sends an
//!   [`ActorPush`], which shoves a dead actor's ragdoll [design].
//! - **Hits** (step C5): the damage of `fnv_combat::damage` against the target's worn
//!   armour (`CombatForms::worn_armor` over its inventory) goes to the script runtime's
//!   `Game::hit` (Health's damage part, OnHit/OnHitWith, death and OnDeath, kill XP), so
//!   scripts see what combat did. Explosions hit with location −1. Each hit sends an
//!   [`ActorHit`] (nvforever plays a hit reaction, when the hit cripples a body part or
//!   lands on a crippled one, or the death pose). Without the script runtime
//!   (`NVF_PLAY_SCRIPTS=0`) hits are only logged.
//! - **Limb hits** (step C8): an actor whose hit volume has body parts
//!   (`crate::hitvol::ActorParts`) is hit on a part of its BPTD: the part's damage
//!   multiplier, its location for `GetHitLocation`, and limb damage (damage before the
//!   part multiplier times the weapon's Limb Dmg Mult) on its condition (`Game::hit`),
//!   which cripples the limb at 0. Player shots and blows select that part from the
//!   single capsule's hit point (`player_body_part`); blows keep their existing Health
//!   multiplier [hypothesis: melee is open, combat-rules.md §12]. Explosions hit no part
//!   (location −1) [design]. Crippled limbs widen the
//!   spread (the documented crippled-arm terms, the head's), weaken melee
//!   (`Attacker::arm_condition`), slow movement by the game settings, make the torso
//!   flinch from any hit ([`player_flinch`]), sway the aim ([`arm_sway`]) and disarm
//!   NPCs ([`disarm`]) (docs/spec-notes/crippled-limbs.md); the HUD lists the player's.
//! - **Explosives and destructibles** (step C7): grenades and other thrown weapons are
//!   lobbed projectiles; mines arm and wait for an actor; C-4 waits for
//!   `DetonatePlacedExplosives`, which the detonator's OnFire script calls (weapons' OnFire
//!   blocks run on each shot). Explosions hurt actors (armour, knockdown by the EXPL flags
//!   and the documented formula, logged), damage destructible objects in their radius and
//!   push loose objects. Bullets damage destructibles they hit. Destruction stages
//!   (`Game::damage_object`, as `DamageObject`) disable, destroy, burn down by their
//!   self-damage and set off their explosions, which can set off others.
//! - **Shots on objects** (combat-rules.md §14): a shot that lands on a placed object
//!   that is not an actor runs the object's OnHit and OnHitWith blocks
//!   (`Game::hit_object`: the tutorial's bottles count the player's shots there) and
//!   strikes its loose body with the projectile's impact force ([`OBJECT_PUSH`]), so
//!   bottles and cans are knocked away [design]. The scripts are told when the player's
//!   weapon is out (`IsWeaponOut`): [`WeaponAnim::ready`].
//! - **What is drawn of a shot** (docs/spec-notes/combat-rules.md §15): each shot sends
//!   a [`ShotFired`] (the renderer shows the PROJ's muzzle flash) and every projectile
//!   in flight is listed in [`ShotVisuals`] (the renderer draws the PROJ's model there).
//!   A hitscan bullet, whose hit is found at once, is drawn flying from the muzzle to
//!   where it lands at the PROJ's speed; a beam is drawn from the muzzle to its end for
//!   the PROJ's fade duration [design].
//! - **Sounds and impacts** (docs/spec-notes/combat-rules.md §16): each shot plays the
//!   weapon's Shoot 2D sound (else its Shoot 3D), an empty trigger its No Ammo sound, and
//!   drawing and putting it away its Equip and Unequip sounds (`crate::sfx`). A shot
//!   that lands sends an [`ImpactFx`] with the IPCT its weapon's impact data set names
//!   for the surface's material, and plays that impact's sounds there; an explosion
//!   sends an [`ExplosionFx`] and plays the EXPL's sounds.
//! - **Item condition** (docs/spec-notes/combat-rules.md §17, `fnv_combat::condition`):
//!   each shot, and each swing that lands, wears the player's weapon; a hit wears the
//!   armour the target wears where it lands (head or body; an explosion both); a hit on
//!   the weapon an actor holds ([`crate::hitvol::WEAPON_PART`]) takes its damage from
//!   that weapon's condition and none from the actor. The player's weapon at zero is
//!   unequipped and stays in the inventory, broken; another actor's falls to the ground
//!   ([`WeaponBroken`]). A worn weapon hits for less and worn armour protects less.
//! - **Other actors' attacks** (combat AI, docs/design/combat-ai.md): nvforever's
//!   `npc_combat` decides when an actor fires or strikes and sends [`ActorFire`] or
//!   [`ActorStrike`]; [`actor_attacks`] turns them into projectiles and hits through the
//!   same rules as the player's. Every projectile knows who fired it, with which weapon
//!   and ammunition ([`By`]), so its hit is that actor's. Shots fired by other actors
//!   can hit the player's character [design: the player has no hit volume of its own;
//!   a hit counts as the torso]. Actors fire without spending ammunition [design: until
//!   looting balance is looked at].
//! - **The player's death**: at 0 health the player dies (`Game::hit`); every control
//!   is held from then on (`ScriptState::locks`) and the screen says so; the death
//!   camera (`nvforever` `death_cam.rs`) drops the body, slows the clock and blurs the
//!   picture [design: reloading the last save is not built]. God mode ([`god_mode`]:
//!   `--god`, the settings page's Developer switch, test aid `NVF_PLAY_TEST_GOD=1`): the
//!   player takes no damage, so a long unattended run is not ended by a fight.
//! - **Feedback** [design]: no hit marker at the crosshair (owner 2026-10-27), the last
//!   target's health for a few seconds, the player's
//!   crippled limbs, and a line for kills with their XP. Health, condition and ammo are
//!   on the HUD proper (`crate::hud`, [`Combat::hud_weapon`]). Developer markers, off
//!   unless [`HitMarks`] says so: a cross at each impact and a line from the muzzle
//!   along each hitscan shot.

use bevy::prelude::*;
use fnv_combat::attack::{ClipTiming, Cycle, Event, Input, Phase, Timing};
use fnv_combat::condition::{self, Wear, Zone};
use fnv_combat::damage::{self, Attacker, AttackKind, HitInput, HitPart, NoPerks, Target};
use fnv_combat::melee::{Melee, MeleeEvent, MeleeInput, SwingTiming};
use fnv_combat::data::AmmoEffect;
use fnv_combat::projectile::{blast, ActorId, CombatWorld, Motion, ProjEvent, Projectile, WorldHit};
use fnv_combat::rng::SplitMix;
use fnv_combat::shot::Seen;
use fnv_combat::spread::{self, Kick, Stance};
use fnv_combat::vec::{self, V3};
use fnv_combat::{shot, CombatForms, CombatTuning, Rules};
use fnv_phys::{layers, Filter, Key, Owner, Queries};
use fnv_sim::FormId;
use std::collections::{HashMap, HashSet};

use crate::activate::HudMessage;
use crate::sfx::{LoopSound, PlaySound, Sfx};
use crate::hitvol::{ActorParts, WEAPON_PART};
use fnv_combat::data::{BodyPart, WeaponStats};
use fnv_game::combat::LimbHit;
use crate::control::PlayerControl;
use crate::script::{with_world, ScriptState};
use fnv_game::combat::{CombatRequest, DestructionChange, HitInfo, HitResult};
use crate::phys::PhysState;
use crate::player::{MoveIntent, Player, MAX_PITCH};
use crate::sim::SimState;
use crate::{space, PlaySet, PlayState, PlayWorld};

mod hit_effects;
pub mod held_grenade_probe;

/// The shooter's id for projectiles (the player has no hit volume).
const PLAYER: ActorId = u64::MAX;

/// What the player body and the combat systems tell each other.
#[derive(Resource, Default, Debug, Clone)]
pub struct WeaponAnim {
    /// Set by the body: the weapon is drawn and in hand.
    pub ready: bool,
    /// Set by combat: the weapon's attack and reload animation names (`AttackRight`,
    /// `ReloadL`), for the body's clip choice.
    pub attack: Option<String>,
    pub reload: Option<String>,
    /// Set by the body: the attack and reload clips' timing in the rig in view.
    pub timing: ClipTiming,
    /// Counted up by combat when an attack or a reload starts; the body restarts the
    /// clip when it sees a new count.
    pub attacks: u32,
    pub reloads: u32,
    /// Set by combat: how fast the reload clips play, a multiple of their own speed
    /// (Agility and the Reload Speed perks, `fnv_combat::attack::reload_rate`); the cycle
    /// takes the same share of time. 0 before combat has set it: read as 1.
    pub reload_rate: f32,
    /// Set by combat: how fast the attack clip plays, a multiple of its own speed: the
    /// weapon's Animation and Animation Attack multipliers and the Modify Attack Speed
    /// perks; the shot cycle takes the same share of time. 0 before combat has set it.
    pub attack_rate: f32,
    /// Set by combat: how fast the draw and holster clips play (the Equip Speed perks:
    /// Quick Draw); the draw and holster take the same share of time. 0: read as 1.
    pub equip_rate: f32,
    /// Counted up by combat when a round-by-round reload starts on a round: the body
    /// plays the reload clip from its start (a reload count plays the opening clip).
    pub reload_rounds: u32,
    /// Counted up by combat when a weapon starts to spin up: the body plays the attack
    /// clip from its start, and the first shot follows at the clip's `Fire` key.
    pub spin_ups: u32,
    /// Counted up by combat when a pull starts a shot that leaves later, at the attack
    /// clip's `Hit` key (a thrown weapon's release, a gun's shot after its wind-up): the
    /// body plays the attack clip from its start then, and the shot that follows does not
    /// start it again (the owner's playtest list 7 item 22: the clip used to start at the
    /// shot, so dynamite waited, left, and only then was lit and thrown).
    pub wind_ups: u32,
    /// Set by combat: the weapon's jam animation (`JamA` for reload animation
    /// `ReloadA`: the game's `1hpjama.kf` beside `1hpreloada.kf` [observed: file lists]),
    /// and counted up when a shot jams the weapon; the body plays the jam clip.
    pub jam: Option<String>,
    pub jams: u32,
    /// Set by combat: the clip stem of the melee swing playing (`attackleft_b`,
    /// `attackforwardpower`; `fnv_combat::melee`), for the body's clip choice; `None`:
    /// the weapon's attack animation.
    pub swing: Option<String>,
    /// Set by the body: the attack stem whose clip [`Self::timing`] was read from.
    pub timed: Option<String>,
    /// Set by the body: the weapon is being drawn, or put away (GetAnimAction 0 and 1).
    pub drawing: bool,
    pub holstering: bool,
    /// Set by the body each frame: where the muzzle of the weapon in hand is, in the rig
    /// in view (game space): the weapon model's `ProjectileNode` [hypothesis: that the
    /// node marks where projectiles and the muzzle flash start, from its name and its
    /// place at the end of the barrel; docs/spec-notes/animation.md §9.9]. `None`
    /// without a body, a weapon in hand or such a node: shots are drawn from the eye.
    pub muzzle: Option<[f32; 3]>,
    /// Set by the body each frame: where the weapon's `ShellCasingNode` is in the rig
    /// in view and the way a casing leaves it (unit), game space.
    pub casing: Option<([f32; 3], [f32; 3])>,
    /// Set by the body: the weapon's attack clip has an `Eject` key, so its casings
    /// leave when the clip reaches it (the action worked), not at the shot.
    pub ejects_on_key: bool,
    /// Counted up by the body when the attack clip of the rig in view passes its
    /// `Eject` key; combat throws a casing from [`Self::casing`] then.
    pub ejects: u32,
}

/// The player fired a shot this frame: the PROJ it fires. The renderer shows the
/// PROJ's muzzle flash at the weapon's muzzle (nvforever `shot_fx`).
#[derive(Message, Clone, Copy, Debug)]
pub struct ShotFired {
    pub projectile: FormId,
    /// The weapon that fired it (nvforever `detection` hears its sound level).
    pub weapon: FormId,
    /// Who fired it.
    pub shooter: FormId,
    /// Where it left, for a shot of an actor other than the player (the player's
    /// flash is at the held weapon's muzzle).
    pub at: Option<V3>,
    /// The way it left (unit, game space), with `at`: an actor's flash without a muzzle
    /// marker is turned along it (nvforever `shot_fx`).
    pub dir: Option<V3>,
}

/// An actor other than the player fires its weapon once (combat AI, nvforever
/// `npc_combat`): aimed from its eyes (`origin`) along `aim` (a unit vector), inside a
/// cone of `cone` degrees (its spread), drawn from its weapon's `muzzle` when it has one
/// (as the player's first-person shots, `shot::leaving` without cover: the shot hits
/// what the eyes see, so an actor fires over low cover its barrel is behind) [design].
#[derive(Message, Clone, Copy, Debug)]
pub struct ActorFire {
    pub shooter: FormId,
    pub weapon: FormId,
    pub origin: V3,
    pub aim: V3,
    pub cone: f32,
    pub muzzle: Option<V3>,
    /// The shot does no damage (a weapon use's Do No Damage, nvforever `use_weapon`):
    /// it flies, strikes and is seen and heard as any other.
    pub harmless: bool,
}

/// An actor other than the player strikes `target` within its reach (combat AI): with
/// its melee weapon, else unarmed (the Fists), or, a creature, with its own attack
/// damage (CREA DATA Damage).
#[derive(Message, Clone, Copy, Debug)]
pub struct ActorStrike {
    pub attacker: FormId,
    pub target: FormId,
    pub weapon: Option<FormId>,
    pub creature_damage: Option<f32>,
    /// Where the blow lands and which way it goes (game space).
    pub point: V3,
    pub dir: V3,
    /// A power attack's attack group ("AttackForwardPower"), for an unarmed attack effect
    /// that names it and, a person's, its damage multiplier; `None`: an ordinary blow.
    pub anim: Option<&'static str>,
}

/// A melee or unarmed blow a blocker stopped part of (docs/design/combat-ai.md §14.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockedBlow {
    pub attacker: FormId,
    pub blocker: FormId,
    /// Damage before and after the block.
    pub damage: f32,
    pub dealt: f32,
}

/// Who fired a projectile: the actor, its weapon and the ammunition it was loaded with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct By {
    pub actor: FormId,
    pub weapon: FormId,
    pub ammo: Option<FormId>,
    /// It does no damage ([`ActorFire::harmless`]).
    pub harmless: bool,
    /// A marked shot of VATS mode (`crate::vats_mode`): its hit counts as a VATS attack
    /// (the critical chance bonus, docs/design/vats-mode.md §9.2).
    pub vats: bool,
    /// The player was sneaking as it left: its hit may be a sneak attack
    /// ([`sneak_attack`]).
    pub sneaking: bool,
}

/// Debris a destroyed object throws (a destruction stage's DEBR and count) from `at`
/// (docs/spec-notes/destruction-stages.md §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectDebris {
    pub reference: FormId,
    pub at: V3,
    pub debris: FormId,
    pub count: i32,
}

/// Who set an explosion off: the actor whose shot (or thrown or placed explosive) it came
/// from and that weapon. Its blast hits are that actor's hits with that weapon, as a
/// bullet's are (OnHit's attacker, kill credit and XP, combat-rules.md §11); `None` for
/// one nobody set off this frame (a destruction stage burning down) [design].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cause {
    actor: FormId,
    weapon: FormId,
}

impl Cause {
    fn of(by: &By) -> Cause {
        Cause { actor: by.actor, weapon: by.weapon }
    }
}

/// The hit an explosion's blast makes on an actor: its cause's, with no place on the body
/// (GetHitLocation −1) and no critical.
/// A weapon hit's object effects with an area reach the actors around the one hit
/// (`hit_effects::spread`); a blast's do not.
fn spread_hit_areas(script: Option<&mut ScriptState>, sim: &mut SimState, stats: &mut CombatStats, target: FormId, items: &[FormId], attacker: Option<FormId>) {
    if items.is_empty() {
        return;
    }
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return };
    with_world(rt, sim, |rt| hit_effects::spread(&mut rt.game, &mut stats.hit_effects, target, items, attacker));
}

/// The WeaponIneffective and ArmorIneffective lines of a hit (`fnv_game::combat_lines`):
/// what DT made of it.
fn armour_lines(script: Option<&mut ScriptState>, attacker: Option<FormId>, target: FormId, d: &damage::Damage, floor: f32) {
    if let Some(rt) = script.and_then(|s| s.rt.as_mut()) {
        rt.game.armour_lines(attacker, target, f64::from(d.raw), f64::from(d.dt), f64::from(d.dealt), f64::from(floor));
    }
}

/// How far before and past an NPC blow's landing point its body part is looked for, game
/// units [design].
const BLOW_PROBE: f32 = 24.0;

fn blast_hit_info(cause: Option<Cause>) -> HitInfo {
    HitInfo { attacker: cause.map(|c| c.actor), weapon: cause.map(|c| c.weapon), location: -1, critical: false, limb: None }
}

/// One projectile to draw this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotVisual {
    /// The same number for as long as the projectile is drawn.
    pub id: u64,
    /// Its PROJ.
    pub projectile: FormId,
    /// Who fired it.
    pub owner: FormId,
    /// Where it is drawn (game space) and the way it points (unit).
    pub pos: V3,
    pub dir: V3,
    /// A beam: its length from `pos` along `dir`.
    pub beam: Option<f32>,
    /// How far it has flown since it left.
    pub travelled: f32,
    /// How strongly it is drawn, 1 to 0: a beam fades out over its PROJ's Fade Duration
    /// (docs/spec-notes/combat-rules.md §15); everything else is 1.
    pub strength: f32,
}

/// The player's projectiles to draw this frame (nvforever `shot_fx` draws their models).
#[derive(Resource, Default)]
pub struct ShotVisuals {
    pub flying: Vec<ShotVisual>,
}

/// A shot threw out a shell casing: the weapon, where the casing starts and its
/// velocity (game space, units per second). The renderer draws the WEAP's casing model.
#[derive(Message, Clone, Copy, Debug)]
pub struct CasingFx {
    pub weapon: FormId,
    pub point: V3,
    pub velocity: V3,
    /// Seconds the casing stays (`fGunShellLifetime`).
    pub life: f32,
}

/// A shot landed: the IPCT of the weapon's impact data set for what it hit, where, the
/// surface's normal and the way the projectile was flying (game space). The renderer
/// shows the impact's effect (nvforever `shot_fx`).
#[derive(Message, Clone, Copy, Debug)]
pub struct ImpactFx {
    pub impact: FormId,
    /// Who fired the projectile (or struck, or set off the explosion); `FormId(0)`: no one.
    pub shooter: FormId,
    pub point: V3,
    pub normal: V3,
    pub dir: V3,
    /// The reference of the actor it struck, if it struck one (its decal goes on the
    /// actor's body, nvforever `decals`).
    pub actor: Option<u32>,
    /// That actor is flagged No Blood Spray / No Blood Decal: the impact's model, or its
    /// decal, is not shown ([`show_impact`], docs/spec-notes/actor-flags.md §2).
    pub no_spray: bool,
    pub no_decal: bool,
}

/// An explosion went off: its EXPL and where.
#[derive(Message, Clone, Copy, Debug)]
pub struct ExplosionFx {
    pub explosion: FormId,
    pub point: V3,
    /// Who set it off, when known (nvforever `detection` raises its event as theirs).
    pub cause: Option<FormId>,
    /// The normal of the ground under the blast, when there is ground near enough
    /// (`nvforever::shot_fx` aligns the model with it unless the EXPL always uses the
    /// world's orientation).
    pub normal: Option<V3>,
    /// How it is drawn.
    pub look: ExplosionLook,
}

/// How an explosion is drawn (`nvforever::shot_fx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplosionLook {
    /// Its EXPL's model and light.
    Own,
    /// Nothing of its own: it went off in or by water, which shows instead.
    Hidden,
    /// The water's surface thrown up ([`WATER_SURFACE_EXPLOSION`]), upright.
    WaterSurface,
}

/// The model of an explosion throwing up water: a column and drops over a spreading
/// ring of foam [observed: an original-game recording of 40mm grenades going off in Lake
/// Mead, 2026-10-25, shows a tall white column over a flat white disc and no fireball;
/// the model's meshes are named WideMesh, TallMesh, BaseMesh and SurfaceFun, with a Drops
/// emitter and the text key "sound: FXExplosionImpactWater"]. No record names it; that
/// this is the model the game drew is a hypothesis from that match.
pub const WATER_SURFACE_EXPLOSION: &str = r"Effects\WaterSurfaceExplosion01.NIF";

/// Every projectile is drawn, not only the PROJ's tracer chance of them [design: the
/// owner asked for bullets that are not invisible, 2026-10-10; the game draws a
/// projectile at its tracer chance, documented: GECK wiki Projectile].
pub const DRAW_EVERY_PROJECTILE: bool = true;

/// An actor was hit (and maybe died) by combat.
#[derive(Message, Clone, Copy, Debug)]
pub struct ActorHit {
    pub form_id: u32,
    pub died: bool,
    pub knocked_out: bool,
    /// Where it was hit (`GetHitLocation`'s numbering; −1 none, 0 torso).
    pub location: i32,
    /// The hit crippled a limb.
    pub crippled: bool,
    /// The body part hit is crippled (by this hit or an earlier one).
    pub part_crippled: bool,
    /// Its torso is crippled: any hit may stagger it (`damage::staggers`).
    pub torso_crippled: bool,
    /// Who hit it.
    pub by: Option<FormId>,
    /// The share of its maximum health the hit took (0 to 1; `crate::rumble`).
    pub share: f32,
    /// A power attack broke through its block (`damage::against_block`): it staggers.
    pub guard_broken: bool,
    /// The weapon of the hit, when known (Meltdown, perk-entry-points.md §9).
    pub weapon: Option<FormId>,
}

/// A hit that forces its target's hit reaction whatever it crippled: Khan Trick
/// [documented: GECK wiki GetForceHitReaction, "set by Khan Trick (AttackCustom4Power and
/// AttackCustom5Power)"; perk-entry-points.md §11.4]. The body plays the reaction for
/// `location` (nvforever `deaths`), without the stagger rule's wait.
#[derive(Message, Debug, Clone, Copy)]
pub struct ForcedHitReaction {
    pub form_id: u32,
    /// Where it was hit (`GetHitLocation`'s numbering).
    pub location: i32,
    /// The move that forced it, for the log.
    pub by: &'static str,
}

/// An actor's weapon left its hand: it reached zero condition and fell (§17), or a hit
/// crippled the actor's arm ([`disarm`]). The renderer takes it out of the hand
/// (nvforever `weapons`); `dropped` is the reference lying on the ground, drawn like any
/// dropped item.
#[derive(Message, Clone, Copy, Debug)]
pub struct WeaponBroken {
    pub form_id: u32,
    pub weapon: FormId,
    pub dropped: Option<FormId>,
}

/// A hit shoves an actor's body at a point (game space): the ragdoll of a dead actor is
/// pushed by it (nvforever `ragdoll.rs`). `dv` is the change of velocity of the body
/// part struck, game units per second [design].
#[derive(Message, Clone, Copy, Debug)]
pub struct ActorPush {
    pub form_id: u32,
    pub point: [f32; 3],
    pub dv: [f32; 3],
}

/// A melee blow's knockback on the living actor it struck (docs/spec-notes/
/// combat-rules.md, "Knockback"): the Melee Knockback Formula's force read as the speed it
/// pushes the actor at, units per second, falling evenly to rest over fKnockbackTime
/// (`secs`), so it travels `speed × secs / 2` [design]; `dir` is the way, along the
/// ground (game x, y, a unit vector). Whoever moves the actor pushes it through its
/// character, so walls and actors stop it: `crate::player::Lunge::knock` for the player,
/// nvforever's residents for the others.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Knockback {
    pub form_id: u32,
    pub dir: [f32; 2],
    pub speed: f32,
    pub secs: f32,
    /// Who struck the blow (a reference's FormID); 0: no blow (`SetVel`).
    pub attacker: u32,
    /// The blow's number in this run, from 1 in the order they land; 0: no blow. Ties a
    /// knockback's log lines to the blow.
    pub blow: u32,
    /// The blow was a power attack.
    pub power: bool,
}

impl Knockback {
    /// A knockback on `form_id` by a blow going along `dir` (game space; only its ground
    /// part counts), with the formula's `force`; `None` for a blow straight down or up
    /// or no force.
    pub fn of(form_id: u32, dir: V3, force: f32, secs: f32) -> Option<Knockback> {
        let len = dir[0].hypot(dir[1]);
        (len > 1e-3 && force > 0.0 && secs > 0.0).then(|| Knockback { form_id, dir: [dir[0] / len, dir[1] / len], speed: force, secs, attacker: 0, blow: 0, power: false })
    }

    /// This knockback comes of blow number `blow` by `attacker`.
    pub fn by(self, attacker: FormId, blow: u32, power: bool) -> Knockback {
        Knockback { attacker: attacker.0, blow, power, ..self }
    }

    /// How far it pushes in all, game units.
    pub fn distance(&self) -> f32 {
        self.speed * self.secs / 2.0
    }

    /// The speed `left` seconds before its end (falling evenly to 0).
    pub fn speed_at(&self, left: f32) -> f32 {
        if self.secs <= 0.0 {
            return 0.0;
        }
        self.speed * (left / self.secs).clamp(0.0, 1.0)
    }

    /// How far it pushes over a step of `dt` seconds from `left` seconds before its end
    /// (the exact share of the falling speed's ground).
    pub fn step(&self, left: f32, dt: f32) -> f32 {
        let to = (left - dt).max(0.0);
        (self.speed_at(left) + self.speed_at(to)) / 2.0 * (left.max(0.0) - to)
    }
}

/// How fast an explosion's knockdown throws the body away from the blast, game units per
/// second [design: the EXPL force is not converted].
pub const BLAST_KNOCKDOWN_SPEED: f32 = 300.0;

/// How fast a shot and a melee blow shove the body part they strike, game units per
/// second, and the share of a melee shove that goes upwards [design].
pub const SHOT_PUSH: f32 = 140.0;
pub const MELEE_PUSH: f32 = 220.0;
pub const POWER_PUSH: f32 = 1.6;

/// A shot's push on a loose object [design]: the projectile's Impact Force (PROJ DATA)
/// read as an impulse in the stored collision's units, mass × metres per second
/// [hypothesis: the field's unit is not documented; the masters' bullets carry 3–6, which
/// on a bottle of mass 1 is 3–6 m/s], times this.
pub const OBJECT_PUSH: f32 = 1.0;
/// The most speed one shot gives an object, game units per second [design].
pub const OBJECT_PUSH_MAX_SPEED: f32 = 700.0;

/// Within this distance of its target a shooter fires from the hip, beyond it down its
/// sights: 20 m at 70 units per metre [owner's playtest list 2026-10-11; the units per
/// metre: observed, gravity fits 9.81 m/s² at 70 units/m, physics.md §6.3]. The NPCs
/// (docs/design/combat-ai.md §10.6) and the player's VATS mode shots in third person
/// (docs/design/vats-mode.md §11.5) follow it.
pub const HIP_FIRE_WITHIN: f32 = 1400.0;
/// Once aiming down the sights, a shooter keeps them up until its target is this much
/// nearer than [`HIP_FIRE_WITHIN`], so it does not switch at the edge [design].
pub const SIGHTS_HYSTERESIS: f32 = 50.0;

/// Whether a shooter fires down its sights at a target `distance` away: only a gun with
/// sights (`sight_fov` above 0), beyond [`HIP_FIRE_WITHIN`]; `was`: it aimed down them
/// last frame (they stay up to [`SIGHTS_HYSTERESIS`] nearer).
pub fn aims_down_sights(was: bool, distance: f32, sight_fov: f32) -> bool {
    sight_fov > 0.0 && distance > if was { HIP_FIRE_WITHIN - SIGHTS_HYSTERESIS } else { HIP_FIRE_WITHIN }
}

/// The straight-line distance between two points, units.
pub fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The player's limb conditions (codes 25–31, 100 healthy, 0 or less crippled), read
/// from the script runtime each frame (step C8).
#[derive(Resource, Debug, Clone, Copy)]
pub struct PlayerLimbs {
    pub condition: [f32; 7],
    /// The share of normal movement speed the legs allow (`fnv_game::Game::leg_speed_mult`).
    pub leg_speed: f32,
}

impl Default for PlayerLimbs {
    fn default() -> Self {
        PlayerLimbs { condition: [100.0; 7], leg_speed: 1.0 }
    }
}

/// The limbs by condition code 25–31, for messages.
const LIMB_NAMES: [&str; 7] = ["head", "torso", "left arm", "right arm", "left leg", "right leg", "brain"];

impl PlayerLimbs {
    pub fn crippled(&self, code: u16) -> bool {
        code.checked_sub(25).and_then(|i| self.condition.get(i as usize)).is_some_and(|c| *c <= 0.0)
    }

    /// Left and right arm crippled.
    pub fn arms(&self) -> (bool, bool) {
        (self.crippled(27), self.crippled(28))
    }

    /// The share of normal movement speed the legs allow: the game settings for one and
    /// two crippled legs, 0.85 and 0.75 in FalloutNV.esm (docs/spec-notes/crippled-limbs.md
    /// §4), the rule of every actor.
    pub fn speed_mult(&self) -> f32 {
        self.leg_speed
    }

    /// Both legs crippled: no directional power attacks outside VATS mode
    /// (crippled-limbs.md §4).
    pub fn both_legs(&self) -> bool {
        self.crippled(29) && self.crippled(30)
    }

    /// A leg is crippled: the body limps (`locomotion\hurt\` clips) [design: either leg,
    /// owner's request 2026-10-16].
    pub fn limping(&self) -> bool {
        self.crippled(29) || self.crippled(30)
    }

    /// The crippled limbs, named.
    pub fn crippled_names(&self) -> Vec<&'static str> {
        (25u16..=31).filter(|&c| self.crippled(c)).map(|c| LIMB_NAMES[(c - 25) as usize]).collect()
    }
}

/// The body part an actor was hit on (step C8): its BPTD entry, when its volume has parts.
fn body_part(c: &Combat, parts: &ActorParts, actor: ActorId, part: Option<u8>) -> Option<BodyPart> {
    let data = parts.data_of(actor as u32)?;
    c.forms.body_parts.get(&data)?.parts.get(part? as usize).cloned()
}

/// The limb damage of hit `d` on part `bp` with weapon `w` (`Damage::limb_damage`: without
/// the part's damage multiplier): none when the part has no limb condition or the
/// weapon's Limb Dmg Mult is 0 [documented: WEAP field; hypothesis: 0 means no limb damage].
fn limb_hit(bp: Option<&BodyPart>, d: &damage::Damage, w: &WeaponStats) -> Option<LimbHit> {
    let bp = bp?;
    let condition = fnv_game::combat::limb_condition(bp.actor_value.as_deref()?)?;
    let damage = d.limb_damage(w.limb_dmg_mult) as f64;
    (damage > 0.0).then_some(LimbHit { condition, health_percent: bp.health_percent as f64, damage, points: false })
}

/// The human skeleton, whose body part data the player's limbs follow [design:
/// `CombatForms::body_parts_for`, docs/spec-notes/combat-rules.md §12].
const HUMAN_SKELETON: &str = "characters\\_male\\skeleton.nif";

// [design] Human skeleton proportions for the player's single capsule (combat-rules.md
// §12). How the original chooses the player's hit part remains [hypothesis].
const PLAYER_HEAD_HEIGHT: f32 = 0.86;
const PLAYER_LEG_HEIGHT: f32 = 0.50;
const PLAYER_ARM_RADIUS: f32 = 0.60;

/// The condition of the part struck on the player's capsule [hypothesis: the bands
/// above, not measured in the original]. Heading is clockwise from north, as `Player`.
fn player_hit_condition(point: V3, feet: V3, height: f32, radius: f32, heading: f32) -> u16 {
    use fnv_game::combat::{HEAD, LEFT_ARM, LEFT_MOBILITY, RIGHT_ARM, RIGHT_MOBILITY, TORSO};
    let height = height.max(f32::EPSILON);
    let z = (point[2] - feet[2]).clamp(0.0, height) / height;
    let side = (point[0] - feet[0]) * heading.cos() - (point[1] - feet[1]) * heading.sin();
    if z > PLAYER_HEAD_HEIGHT {
        HEAD
    } else if z < PLAYER_LEG_HEIGHT {
        if side < 0.0 { LEFT_MOBILITY } else { RIGHT_MOBILITY }
    } else if side.abs() > radius.max(0.0) * PLAYER_ARM_RADIUS {
        if side < 0.0 { LEFT_ARM } else { RIGHT_ARM }
    } else {
        TORSO
    }
}

/// Resolve the capsule's hit to the player's limb data by condition, never by BPTD
/// index [design: the same human data as `blast_limb_hits`, in either view].
fn player_body_part(c: &Combat, phys: &PhysState, player: Option<&Player>, point: V3) -> Option<BodyPart> {
    let ch = phys.player.as_ref()?;
    let condition = player_hit_condition(point, ch.feet, ch.params.height, ch.params.radius, player?.heading);
    let data = c.forms.body_parts_for(FormId::PLAYER_BASE, HUMAN_SKELETON)?;
    c.forms.body_parts.get(&data)?.parts.iter().find(|bp| bp.actor_value.as_deref().and_then(fnv_game::combat::limb_condition) == Some(condition)).cloned()
}

/// An explosion's limb damage on `target` (docs/spec-notes/crippled-limbs.md §5): the
/// shares of `fnv_game::combat::blast_limb_shares` (the torso 1/30 of the blast's damage
/// after armour, the legs 0.24; nothing on the head, arms or brain) of the limb conditions
/// its body part data names, times the weapon's Limb Dmg Mult (1 without a weapon)
/// [observed: the shares; hypothesis: the multiplier, 1 on the measured mine and grenade].
/// The player's body part data is the human skeleton's when the body is not posed.
fn blast_limb_hits(c: &Combat, parts: &ActorParts, target: FormId, dealt: f32, weapon: Option<&WeaponStats>) -> Vec<LimbHit> {
    let mult = weapon.map_or(1.0, |w| w.limb_dmg_mult.max(0.0));
    let damage = f64::from(dealt.max(0.0) * mult);
    let data = parts.data_of(target.0).or_else(|| (target == FormId::PLAYER_REF).then(|| c.forms.body_parts_for(FormId::PLAYER_BASE, HUMAN_SKELETON)).flatten());
    let Some(bpd) = data.and_then(|d| c.forms.body_parts.get(&d)).filter(|_| damage > 0.0) else { return Vec::new() };
    let conditions: Vec<u16> = bpd.parts.iter().filter_map(|bp| bp.actor_value.as_deref().and_then(fnv_game::combat::limb_condition)).collect();
    fnv_game::combat::blast_limb_shares(&conditions, damage)
}

/// A hit that may dismember (`crate::dismember`): one that killed the actor, or landed on
/// its corpse, on a body part. The player is not dismembered.
pub(crate) fn dismember_check(target: FormId, part: Option<u8>, w: &WeaponStats, r: &HitResult, by: Option<FormId>, point: V3, dir: V3) -> Option<crate::dismember::Check> {
    let part = part.filter(|&p| p != WEAPON_PART && p != fnv_phys::gore::GORE_PART && p != fnv_phys::actors::WHOLE)?;
    if target == FormId::PLAYER_REF || !(r.died || r.corpse) {
        return None;
    }
    Some(crate::dismember::Check {
        target,
        part: part as usize,
        on_hit: fnv_combat::dismember::OnHit::from_label(w.on_hit.as_deref()),
        crippled: r.limb.is_some_and(|(_, c)| c <= 0.0),
        corpse: r.corpse,
        overkill: r.overkill as f32,
        max_health: r.max_health as f32,
        by,
        point,
        dir,
        scripted: None,
    })
}

/// The body part of `actor` a line from `from` towards `to` meets first, and where
/// (other actors left out): the part a blast or a blow lands on, for dismemberment
/// (dismemberment.md §4) [design: as a shot's part is the one its ray meets].
pub(crate) fn facing_part(world: &dyn CombatWorld, actor: ActorId, from: V3, to: V3) -> Option<(u8, V3)> {
    let others: Vec<ActorId> = world.actors_near(to, 512.0).into_iter().map(|(a, _)| a).filter(|&a| a != actor).collect();
    let hit = world.ray(from, to, &others)?;
    (hit.actor == Some(actor)).then_some(())?;
    Some((hit.part?, hit.point))
}

/// Whether the script runtime has the actor dead.
pub fn is_dead(script: Option<&ScriptState>, form_id: u32) -> bool {
    script.and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.is_dead(FormId(form_id)))
}

/// What the HUD shows about hits [design].
#[derive(Resource, Default, Debug, Clone)]
pub struct HitFeedback {
    /// The last actor hit: name, health, maximum, seconds since.
    pub target: Option<(String, f32, f32, f32)>,
    /// Hits the player took since [`player_flinch`] last looked.
    pub player_hits: Vec<PlayerHit>,
    /// Actors whose arm a hit crippled since [`disarm`] last looked.
    pub disarms: Vec<FormId>,
    /// Health the player lost to hits since the screen's blood last looked, each with
    /// the player's maximum (`crate::screen_splatter`'s hurt splats).
    pub player_hurt: Vec<(f32, f32)>,
}

/// A hit the player took, for the flinch rule (`damage::staggers`).
#[derive(Debug, Clone, Copy)]
pub struct PlayerHit {
    pub crippled: bool,
    pub part_crippled: bool,
    pub torso_crippled: bool,
}

/// Height above a destructible's position its stage explosions go off [design].
pub(crate) const DESTRUCTION_BLAST_HEIGHT: f32 = 40.0;

/// How one actor's weapon wore from its own attacks (`CombatStats::npc_weapon_wear`).
#[derive(Default, Debug, Clone, Copy)]
pub struct NpcWeaponWear {
    pub attacks: u32,
    /// Health points lost; the condition before the first and after the last.
    pub points: f32,
    pub first: f32,
    pub last: f32,
}

/// Counts for the exit report.
#[derive(Resource, Default, Debug, Clone)]
pub struct CombatStats {
    /// The player's shots that rolled for their ammo's spent item (a case, hull or
    /// drained cell), and the items given, by item.
    pub spent_rolls: u32,
    pub spent_given: std::collections::BTreeMap<u32, u32>,
    /// The magic items hits applied and which are still active (`hit_effects`).
    pub hit_effects: hit_effects::HitEffects,
    /// Hits between actors other than the player, by target: (name, the damage of each
    /// hit, its maximum health), for the summary that sets a fight against the AI
    /// observer's (docs/spec-notes/npc-behaviour-compare.md §9).
    pub npc_hits: std::collections::BTreeMap<u32, (String, Vec<f32>, f32)>,
    /// Wear of other actors' weapons from their own shots and blows (§17), by actor and
    /// weapon, for the exit summary.
    pub npc_weapon_wear: std::collections::BTreeMap<(u32, u32), NpcWeaponWear>,
    pub shots: u32,
    pub projectiles: u32,
    pub actor_hits: u32,
    pub world_hits: u32,
    /// Shots on objects whose OnHit / OnHitWith blocks ran, and objects knocked away.
    pub object_scripts: u32,
    pub objects_pushed: u32,
    pub explosions: u32,
    pub damage: f32,
    pub reloads: u32,
    pub dry_fires: u32,
    /// Shots that jammed the weapon (item 9).
    pub jams: u32,
    /// Melee blows that showed an impact (item 24).
    pub melee_impacts: u32,
    /// Shots that splashed going into water, and explosions that threw up water.
    pub water_splashes: u32,
    pub water_blasts: u32,
    pub kills: u32,
    pub xp: f64,
    pub swings: u32,
    pub power_attacks: u32,
    /// Melee blows a blocker stopped part of (docs/design/combat-ai.md §14.3).
    pub blows_blocked: u32,
    /// Blocks power attacks broke through (combat-rules.md §12).
    pub blocks_broken: u32,
    /// Combat-tab lines said by anyone (Hit, Attack, PowerAttack, Death …), and tries by
    /// actors that have no line of the topic (`fnv_game::combat_lines`).
    pub combat_lines: u32,
    pub combat_lines_none: u32,
    /// Swings that went on from the last one's `a:` key (combos).
    pub chained_swings: u32,
    pub melee_hits: u32,
    pub knockdowns: u32,
    /// Shots drawn from the weapon's muzzle, and shots that left it because something
    /// stood in front of the barrel (third person).
    pub muzzle_shots: u32,
    pub covered_shots: u32,
    /// Item condition (§17): health points the player's weapons lost, hits on weapons
    /// actors hold, weapons broken, health points armour lost, armour worn to zero.
    pub weapon_wear: f32,
    pub weapon_hits: u32,
    pub weapons_broken: u32,
    pub armor_wear: f32,
    pub armor_broken: u32,
    /// Actors disarmed by a crippled arm ([`disarm`]).
    pub disarmed: u32,
    /// Weapons dropped by actors that died holding them ([`death_drops`]).
    pub death_drops: u32,
    /// Other actors' shots and blows (combat AI), their hits on the player and the
    /// damage those did, and whether the player died.
    pub actor_shots: u32,
    pub actor_strikes: u32,
    pub player_hits: u32,
    pub player_damage: f32,
    pub player_died: bool,
    /// Hits of harmless shots ([`ActorFire::harmless`]): no damage dealt.
    pub harmless_hits: u32,
}

/// A projectile in flight with where it left from, where it is drawn and whether it
/// draws a tracer.
/// A grenade live now ([`Combat::live_grenades`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveGrenade {
    /// Its number among the drawn projectiles, the same for its whole flight.
    pub id: u64,
    pub at: V3,
    /// Its explosion's radius, units.
    pub radius: f32,
    pub thrower: FormId,
    /// Seconds since it left.
    pub age: f32,
}

struct Flying {
    p: Projectile,
    /// Its number among the drawn projectiles ([`ShotVisual::id`]) and its PROJ.
    id: u64,
    proj: FormId,
    /// Its model is drawn.
    shown: bool,
    /// The way it last flew.
    dir: V3,
    /// It is drawn from the weapon's muzzle (not from the eye for want of one).
    from_muzzle: bool,
    origin: V3,
    seen: Seen,
    tracer: bool,
    projectiles_per_shot: u32,
    /// The projectile's Impact Force (PROJ DATA), for [`OBJECT_PUSH`].
    force: f32,
    by: By,
    /// The temporary reference scripts see it as (`fnv_game::nvse_proj`), and whether one
    /// was asked for; the speed multiplier last applied.
    rref: Option<FormId>,
    registered: bool,
    speed_mult: f32,
    /// Where its flight ended without a hit (`ProjEvent::Expired`).
    ended_at: Option<V3>,
}

/// A hit waiting for its delivery time (hitscan: distance ÷ speed).
struct Pending {
    due: f32,
    actor: Option<ActorId>,
    /// The body part hit (step C8).
    part: Option<u8>,
    /// The placed reference hit, for destructible objects.
    object: Option<u64>,
    point: V3,
    /// The way the projectile was flying.
    dir: V3,
    /// The surface's normal and collision material, for the impact effect.
    normal: V3,
    material: Option<u32>,
    projectiles: u32,
    force: f32,
    by: By,
    /// The shot's reference for scripts, kept until the hit lands; when it was made
    /// (`time`) and its speed multiplier as last applied.
    rref: Option<FormId>,
    since: f32,
    speed_mult: f32,
}

/// What is drawn of a projectile whose hit was found at once: a bullet flying from the
/// muzzle to where it landed, or a beam between the two [design].
struct Ghost {
    id: u64,
    proj: FormId,
    /// Who fired it.
    owner: FormId,
    from: V3,
    to: V3,
    /// Units per second (a bullet); a beam stays for `left` seconds instead.
    speed: f32,
    travelled: f32,
    beam: bool,
    left: f32,
    /// The whole of `left` when made (the PROJ's Fade Duration).
    fade: f32,
    /// Made this frame: drawn at its start before it moves.
    fresh: bool,
    /// A beam from the muzzle: its start stays on the muzzle while it shows.
    follow: bool,
}

/// How strongly a shot is drawn with `left` of its `fade` seconds to go: a beam fades
/// from full to nothing over its PROJ's Fade Duration, in a straight line [hypothesis:
/// what Fade Duration means, from its name and the beam staying that long; the line is
/// [design]]; anything else is drawn in full.
pub fn beam_strength(beam: bool, left: f32, fade: f32) -> f32 {
    if beam && fade > 0.0 {
        (left / fade).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// What a [`Mark`] shows.
#[derive(Clone, Copy, PartialEq)]
enum MarkKind {
    Tracer,
    Impact,
    /// A shot left the muzzle at `to`.
    Flash,
}

/// Whether the developer markers are drawn: a cross at each impact, a line along each
/// hitscan shot (settings page `hit-markers`, `--hit-markers`) [design]. Off by default:
/// the shots' own impact effects stand for them in play. The test aid
/// `NVF_PLAY_TEST_TRACERS` draws them whatever this says.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct HitMarks {
    pub show: bool,
}

/// A short-lived marker [design].
struct Mark {
    from: V3,
    to: V3,
    age: f32,
    life: f32,
    kind: MarkKind,
    /// A line from the muzzle: its start stays on the muzzle while it shows.
    follow: bool,
}

impl Mark {
    fn tracer(from: V3, to: V3, life: f32) -> Mark {
        Mark { from, to, age: 0.0, life, kind: MarkKind::Tracer, follow: false }
    }

    fn at(point: V3, life: f32, kind: MarkKind) -> Mark {
        Mark { from: point, to: point, age: 0.0, life, kind, follow: false }
    }
}

/// How long a looping shoot sound goes on after the last shot, seconds [design].
const LOOP_SOUND_HOLD: f32 = 0.25;
/// The size of the test aid's marker at the muzzle, units.
const FLASH_SIZE: f32 = 2.5;

/// Whether a projectile's hit is found at once, on its first step (hitscan, beams), as
/// `Projectile::step` decides it.
fn instant(p: &Projectile) -> bool {
    (p.hitscan || p.motion == Motion::Beam) && p.motion != Motion::Lobber
}
/// The reach of the view ray that finds the aim point when the projectile has no range
/// of its own, units [design].
const AIM_REACH: f32 = 100_000.0;

/// The player's combat state.
#[derive(Resource)]
pub struct Combat {
    pub forms: CombatForms,
    pub rules: Rules,
    pub tuning: CombatTuning,
    /// The weapon in use: the equipped one, else the Fists.
    weapon: Option<FormId>,
    /// The equipped weapon as the simulation has it (None: unarmed).
    equipped: Option<Option<FormId>>,
    /// The equipped unit's mod flags (`Extra::mods`): its figures are the modded ones
    /// (docs/spec-notes/weapon-mods.md §2).
    mods: u8,
    /// The weapon and mod sound the player's shots last played, while it is a silenced
    /// one: its log line is written once (weapon-mods.md §7).
    silenced_logged: Option<(FormId, FormId)>,
    /// The Fists form (WEAP `Fists`), the unarmed weapon.
    fists: Option<FormId>,
    ammo: Option<FormId>,
    /// The weapon spends itself (no ammo, a projectile).
    self_ammo: bool,
    /// The weapon is melee or unarmed (no cycle; [`Swing`]s).
    melee: bool,
    /// The melee swings (step C6).
    swings: Melee,
    /// Blows that have knocked someone back so far ([`Knockback::blow`]).
    blows: u32,
    /// Explosions to set off this frame (impacts, detonations, destruction stages), with
    /// who set each off.
    boom: Vec<(FormId, V3, Option<Cause>)>,
    /// Debris of destroyed objects for the engine to throw (nvforever `dismember.rs`).
    pub debris: Vec<ObjectDebris>,
    cycle: Option<Cycle>,
    kick: Kick,
    flying: Vec<Flying>,
    /// Thrown mines that came to rest this frame, for `crate::mines` to place.
    settled: Vec<SettledMine>,
    /// Thrown weapons that landed this frame, for `crate::thrown` to put down.
    landed: Vec<LandedThrow>,
    /// Shots that went down into water, shown when due ([`Splash`]).
    splashes: Vec<Splash>,
    ghosts: Vec<Ghost>,
    /// The weapon's looping shoot sound while it plays, and the seconds it has left
    /// unless another shot comes.
    looping: Option<(FormId, f32)>,
    /// The last [`WeaponAnim::ejects`] seen, and when (`time`) the last shot was fired.
    ejects_seen: u32,
    last_shot: f32,
    /// The next [`ShotVisual::id`].
    next_visual: u64,
    pending: Vec<Pending>,
    /// Shots scripts asked for (`FireWeapon`), fired with the actors' shots next frame.
    script_shots: Vec<ActorFire>,
    /// People blocking now and the way each faces (horizontal, game space): the engine's
    /// combat AI says each frame (docs/design/combat-ai.md §14.3).
    pub guarding: HashMap<FormId, [f32; 2]>,
    /// Blows blocked since the combat AI last took them.
    pub blocked_blows: Vec<BlockedBlow>,
    /// Until when (`time`) the player cannot block: a power attack broke its block.
    guard_down_until: f32,
    marks: Vec<Mark>,
    rng: SplitMix,
    /// The melee swings' own rolls (variants), so they leave the hit rolls as they were.
    swing_rng: SplitMix,
    time: f32,
    /// Test aid `NVF_PLAY_TEST_NO_SPREAD=1`: shots leave exactly along the view (no
    /// spread, no bloom, no recoil), so an unattended check hits what it aims at.
    no_spread: bool,
    /// A save's weapon and magazine, applied when that weapon is next taken in hand
    /// (`crate::save`).
    load_magazine: Option<(u32, u32)>,
    /// A save's weapon and the type of round in its magazine, with `load_magazine`.
    load_ammo: Option<(u32, FormId)>,
    /// Test aid `NVF_PLAY_TEST_TRACERS=<seconds>`: every projectile draws a line, and
    /// the lines and a marker at the muzzle stay that long, so one screenshot of an
    /// unattended check shows where the shots were drawn from. The muzzle flashes and
    /// the projectile models are not held: they are drawn as in play.
    tracer_hold: Option<f32>,
    /// The spread cone's half-angle in degrees a shot would leave in now, while a weapon
    /// with a firing cycle is drawn and ready; for the crosshair (`crate::reticle`).
    pub crosshair_cone: Option<f32>,
    /// The player's spread (PSp, spread units, after "Calculate Gun Spread" perks) at the
    /// same time: a scope's sway (docs/spec-notes/weapon-scopes.md §5).
    pub player_spread: Option<f32>,
    /// When (`time`) the player's block last stopped a blow: Scribe Counter's window
    /// (perk-entry-points.md §11).
    player_blocked_at: Option<f32>,
    /// The special move the swing playing is (chosen as it starts), for its strike.
    special_swing: Option<SpecialMove>,
    /// The cone (half-angle, degrees) VATS mode's hit chance reads for the gun in hand
    /// (docs/design/vats-mode.md §17.2): the spread with the sights up, no bloom, before
    /// `fVATSSpreadMult`; kept from the last frame a gun was ready, `None` without one.
    pub vats_cone: Option<f32>,
    /// The weapon and factor of the last Modify Throwing Velocity line (one a change).
    throw_logged: Option<(FormId, u32)>,
    /// The spread perks last logged, with their weapon ([`spread_perks`]).
    spread_perks_logged: Option<(FormId, SpreadPerks)>,
    /// The player's flinch (crippled-limbs.md §3): until when no attack starts, when the
    /// last one ended, and its own rolls (the view's kick).
    flinch_until: f32,
    flinch_end: Option<f32>,
    flinch_rng: SplitMix,
    /// The recoil's rolls, its own so other rolls keep their order.
    kick_rng: SplitMix,
    /// The bloom has just shrunk away; logged with the crosshair cone.
    kick_just_settled: bool,
    /// The spent-ammo items' own rolls (docs/spec-notes/spent-ammo-items.md), so they
    /// leave the shots' rolls as they were.
    spent_rng: SplitMix,
    /// A reload ended and no shot has been fired since (No Jam After Reload).
    pub fresh_reload: bool,
}

impl Combat {
    /// Whether the player's flinch (a staggering hit, [`player_flinch`]) still plays: the
    /// player does not walk meanwhile (`crate::player::flinch_holds_walk`).
    pub fn player_flinching(&self) -> bool {
        self.time < self.flinch_until
    }
}

impl Combat {
    /// Seconds since the player's last shot.
    pub fn since_last_shot(&self) -> f32 {
        self.time - self.last_shot
    }

    /// Projectiles in flight now (the stress tests' samples).
    pub fn in_flight(&self) -> usize {
        self.flying.len()
    }

    /// The grenades live now (docs/design/combat-ai.md §17.7): lobbed projectiles with an
    /// explosion, in flight or at rest, not mines.
    pub fn live_grenades(&self) -> Vec<LiveGrenade> {
        self.flying
            .iter()
            .filter(|f| f.p.alive && !f.p.mine && f.p.motion == Motion::Lobber)
            .filter_map(|f| {
                let radius = self.forms.explosions.get(&f.p.explosion?)?.radius;
                (radius > 0.0).then_some(LiveGrenade { id: f.id, at: f.p.pos, radius, thrower: f.by.actor, age: f.p.age })
            })
            .collect()
    }

    /// The thrown mines that came to rest since the last call (`crate::mines`).
    pub(crate) fn take_settled(&mut self) -> Vec<SettledMine> {
        std::mem::take(&mut self.settled)
    }

    /// The thrown weapons that landed since the last call (`crate::thrown`).
    pub(crate) fn take_landed(&mut self) -> Vec<LandedThrow> {
        std::mem::take(&mut self.landed)
    }

    /// A mine goes off (`crate::mines`): its explosion at `at`, its blast the hits of
    /// `by` (the actor who put it down and that weapon), when someone did.
    pub(crate) fn mine_explodes(&mut self, explosion: FormId, at: V3, by: Option<(FormId, FormId)>) {
        self.boom.push((explosion, at, by.map(|(actor, weapon)| Cause { actor, weapon })));
    }
}

/// A thrown weapon whose projectile ended (`crate::thrown`): the weapon it was, its PROJ,
/// where it ended, the surface's normal there (none when it ran out of range in the
/// air), the way it flew, the actor it struck, and who threw it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LandedThrow {
    pub weapon: FormId,
    pub proj: FormId,
    pub point: V3,
    pub normal: Option<V3>,
    pub dir: V3,
    pub struck: Option<FormId>,
    pub by: FormId,
}

/// A shot went down into water ([`ProjEvent::Splash`]): the water impact of its weapon's
/// impact data set shows on the surface when it gets there (a shot found at once:
/// distance ÷ speed after it was fired, as its hit).
#[derive(Clone, Copy, Debug)]
struct Splash {
    due: f32,
    point: V3,
    dir: V3,
    weapon: FormId,
    shooter: FormId,
}

/// A thrown mine that came to rest ([`ProjEvent::Settled`]): its PROJ, where, the way it
/// last flew, and who put it down.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SettledMine {
    pub proj: FormId,
    pub point: V3,
    pub dir: V3,
    pub actor: FormId,
}

impl Combat {
    /// The weapon the player fights with now: the equipped one, else the Fists.
    pub fn player_weapon(&self) -> Option<FormId> {
        self.weapon
    }

    /// The ammunition the player's weapon loads now (`None`: none, or melee).
    pub fn player_ammo(&self) -> Option<FormId> {
        self.ammo
    }

    /// The figures of the player's weapon `id`: the equipped one's with its mods on
    /// (`WeaponStats::modded`), any other's as the record has them.
    /// The next blow's number ([`Knockback::blow`]).
    fn next_blow(&mut self) -> u32 {
        self.blows += 1;
        self.blows
    }

    /// For the melee knockback test aid: whether the player's weapon is melee or the fists,
    /// and the swing playing, if any: whether it is a power attack.
    pub(crate) fn melee_swing(&self) -> (bool, Option<bool>) {
        (self.melee, self.swings.swing.as_ref().map(|s| s.power.is_some()))
    }

    pub fn player_stats(&self, id: FormId) -> Option<WeaponStats> {
        let w = self.forms.weapons.get(&id)?;
        Some(if self.equipped.flatten() == Some(id) && self.mods != 0 { w.modded(self.mods) } else { w.clone() })
    }

    /// The Fists (WEAP `Fists`), the unarmed weapon.
    pub fn fists_id(&self) -> Option<FormId> {
        self.fists
    }

    /// The weapon in hand and the rounds in its magazine, for a save.
    pub fn magazine_for_save(&self) -> Option<(u32, u32)> {
        let cy = self.cycle.as_ref().filter(|c| !c.no_ammo)?;
        Some((self.weapon?.0, cy.magazine))
    }

    /// The type of round in the magazine of the weapon in hand, for a save (#1574).
    pub fn magazine_ammo_for_save(&self) -> Option<u32> {
        self.cycle.as_ref().filter(|c| !c.no_ammo)?;
        self.ammo.map(|a| a.0)
    }

    /// A save's magazine and the type of round in it: the rounds go back in when that
    /// weapon is in hand. A weapon already in hand with another type is taken in hand
    /// again (`equip_sync`), so the saved type and its projectile are put back together.
    pub fn restore_magazine(&mut self, m: (u32, u32), ammo: Option<u32>) {
        self.load_magazine = Some(m);
        self.load_ammo = ammo.map(|a| (m.0, FormId(a)));
        if let (Some(w), Some(cy)) = (self.weapon, self.cycle.as_mut()) {
            if w.0 == m.0 {
                if ammo.is_some_and(|a| self.ammo != Some(FormId(a))) {
                    self.equipped = None;
                    return;
                }
                cy.magazine = m.1.min(cy.clip_size.max(m.1));
                self.load_magazine = None;
                self.load_ammo = None;
            }
        }
    }

    /// The weapon in use and its Sight FOV (degrees), when it is a gun with sights (Sight
    /// FOV above 0; melee weapons block instead) (`crate::aim_zoom`).
    pub fn sight_fov(&self) -> Option<(FormId, f32)> {
        let w = self.weapon.filter(|_| !self.melee)?;
        let fov = self.player_stats(w)?.sight_fov;
        (fov > 0.0).then_some((w, fov))
    }

    /// The gun in use has its scope (weapon-scopes.md §2: Has Scope, with its scope mod
    /// when the scope comes from one).
    pub fn player_weapon_scoped(&self) -> bool {
        self.sight_fov().and_then(|(w, _)| self.player_stats(w)).is_some_and(|s| s.scope_model().is_some())
    }

    /// The PROJ the weapon in use fires with the ammunition loaded, if it fires one.
    pub fn projectile(&self) -> Option<FormId> {
        self.forms.chain(self.weapon?, self.ammo)?.projectile
    }

    /// The weapon in hand as VATS mode needs it (docs/design/vats-mode.md §3, §7, §16), or
    /// why it cannot enter: a gun with a firing cycle that fires a projectile, not
    /// reloading or jammed, a round loaded; a melee or unarmed weapon (the Fists too); a
    /// grenade or thrown weapon (not a mine), one carried.
    pub fn vats_weapon(&self, sim: &SimState, gravity: f32) -> Result<VatsWeapon, &'static str> {
        const NO_GUN: &str = "VATS mode needs a weapon to aim.";
        use fnv_combat::vats::{shot_ap_cost, throw_reach, AttackStyle};
        let id = self.weapon.ok_or(NO_GUN)?;
        let w = &self.player_stats(id).ok_or(NO_GUN)?;
        let cost = shot_ap_cost(w, &self.rules.ap_attack);
        let edid = w.edid.clone().unwrap_or_else(|| id.to_string());
        if self.melee {
            return Ok(VatsWeapon {
                id,
                edid,
                cost,
                style: AttackStyle::Melee,
                rounds_per_shot: 1,
                rounds: u32::MAX / 2,
                reach: crate::vats_mode::MELEE_MARK_REACH,
                // From the eye, as the player's swing reaches (`melee`): the reach is
                // from the body, the capsule's radius further.
                melee_reach: damage::melee_reach(&self.rules, w, 1.0) + fnv_phys::CharacterParams::default().radius,
            });
        }
        // A gun needs to be equipped: the Fists are melee.
        if self.equipped.flatten() != Some(id) || self.player_weapon_is_mine() {
            return Err(NO_GUN);
        }
        let proj = self.forms.chain(id, self.ammo).and_then(|ch| ch.projectile).ok_or(NO_GUN)?;
        if self.self_ammo {
            let carried = player_count(sim, Some(id));
            if carried == 0 {
                return Err("Out of ammo.");
            }
            let p = self.forms.projectiles.get(&proj);
            let range = p.map_or(0.0, |p| if p.range > 0.0 { p.range } else { crate::vats_mode::REACH });
            let lob = p.map_or(f32::INFINITY, |p| throw_reach(p.speed, p.gravity * gravity));
            return Ok(VatsWeapon { id, edid, cost, style: AttackStyle::Thrown, rounds_per_shot: 1, rounds: carried, reach: range.min(lob), melee_reach: 0.0 });
        }
        if AttackKind::of(w) != AttackKind::Gun {
            return Err(NO_GUN);
        }
        let cy = self.cycle.as_ref().ok_or(NO_GUN)?;
        if cy.reloading() || cy.jammed() {
            return Err("Not while reloading.");
        }
        let style = AttackStyle::of(w, false, false);
        // God mode keeps the magazine full (the shots take nothing).
        let rounds = if cy.no_ammo {
            u32::MAX / 2
        } else if god_mode() && cy.regen.is_none() {
            cy.clip_size
        } else {
            cy.magazine
        };
        if !cy.no_ammo && rounds < cy.ammo_use {
            return Err("Out of ammo.");
        }
        let range = self.forms.projectiles.get(&proj).map_or(0.0, |p| p.range);
        Ok(VatsWeapon {
            id,
            edid,
            cost,
            style,
            // A burst's rounds are one mark's (§16.1).
            rounds_per_shot: if cy.no_ammo { 1 } else { cy.ammo_use * style.shots_per_mark() },
            rounds,
            reach: if range > 0.0 { range } else { crate::vats_mode::REACH },
            melee_reach: 0.0,
        })
    }

    /// GetAnimAction's code for what the player's weapon does now
    /// (`fnv_game::anim_state`): drawing (0) and putting away (1) as the body says, a
    /// gun's shot by its firing cycle (2 from the pull to the shot and while an automatic
    /// fires on with the trigger held, 3 the frame after, 4 until the cycle is ready
    /// again), a reload 9, a melee swing 2 until it strikes and 4 after, blocking 7. A
    /// weapon that spends itself (grenades, thrown weapons, mines) reads 5 until it leaves
    /// the hand, 6 the frame after and 4 while the next is drawn (`anim_state::
    /// throw_action`). A round-by-round reload reads 17 while it opens the weapon, 9 for
    /// its rounds and its close [hypothesis: `AnimAction::ReloadOpen`]. A jam reads 13
    /// [hypothesis: the page's catch-all, "every action not included in the ones above"].
    /// The player's weapon is a mine: its WEAP animation type is "Land Mine (1 Hand)" or
    /// "Mine Drop (1 Hand)" (IsPlayerActionActive 4 while one is laid).
    pub fn player_weapon_is_mine(&self) -> bool {
        self.weapon.and_then(|w| self.forms.weapons.get(&w)).and_then(|w| w.anim_type.as_deref()).is_some_and(|t| t.starts_with("Land Mine") || t.starts_with("Mine Drop"))
    }

    pub fn player_anim_action(&self, anim: &WeaponAnim, trigger: bool, aiming: bool, dt: f32) -> Option<fnv_game::anim_state::AnimAction> {
        use fnv_game::anim_state::{swing_action, AnimAction};
        if anim.drawing {
            return Some(AnimAction::Equip);
        }
        if anim.holstering {
            return Some(AnimAction::Unequip);
        }
        if !anim.ready {
            return None;
        }
        if self.melee {
            if let Some(s) = &self.swings.swing {
                return swing_action(s.t, s.timing.hit, s.timing.len);
            }
            return aiming.then_some(AnimAction::Block);
        }
        let cy = self.cycle.as_ref()?;
        match cy.phase {
            Phase::SpinUp { .. } | Phase::WindUp { .. } if self.self_ammo => Some(AnimAction::ThrowWindUp),
            Phase::SpinUp { .. } | Phase::WindUp { .. } => Some(AnimAction::Attack),
            Phase::Cooldown { left } => cooldown_action(&cy.timing, left, self.self_ammo, cy.automatic && trigger, dt),
            Phase::ReloadOpen { .. } => Some(AnimAction::ReloadOpen),
            Phase::Reloading { .. } | Phase::ReloadRound { .. } | Phase::ReloadClose { .. } => Some(AnimAction::Reload),
            Phase::Jammed { .. } => Some(AnimAction::ForceScriptAnim),
            Phase::Ready => None,
        }
    }

    /// The player aims down a gun's sights now: a gun with sights in hand and Block held
    /// (IsPlayerActionActive 9, GetPCUsingIronSights), not while a reload or a jam holds
    /// the sights down ([`sights_held_down`]).
    pub fn player_sights_up(&self, anim: &WeaponAnim, aiming: bool) -> bool {
        anim.ready && aiming && !self.player_sights_held_down() && self.sight_fov().is_some()
    }

    /// The gun's firing cycle holds the sights down now: it reloads or clears a jam
    /// ([`sights_held_down`]). The aim zoom and the sight pose ease out meanwhile
    /// (`crate::aim_zoom`) and come back once the cycle is through, if Block is still held.
    pub fn player_sights_held_down(&self) -> bool {
        !self.melee && self.cycle.as_ref().is_some_and(|c| sights_held_down(&c.phase))
    }

    /// The player's melee swing is playing (VATS mode waits for it, vats-mode.md §16.2).
    pub fn player_swinging(&self) -> bool {
        self.melee && self.swings.swing.is_some()
    }

    /// The player's skill with the weapon in hand (VATS mode's hit chance, vats-mode.md
    /// §17.2): its WEAP skill's actor value.
    pub fn player_weapon_skill(&self, script: Option<&mut ScriptState>, sim: &mut SimState) -> f32 {
        let Some(w) = self.weapon.and_then(|id| self.player_stats(id)) else { return Attacker::default().skill };
        attacker(script, sim, &self.rules, FormId::PLAYER_REF, &w, false).skill
    }

    /// The BPTD part an actor's hit volume part is (VATS mode's marks).
    pub fn vats_part(&self, parts: &ActorParts, actor: u32, part: Option<u8>) -> Option<BodyPart> {
        body_part(self, parts, actor as ActorId, part)
    }
}

/// A firing cycle in this phase holds the sights down: a reload (every step of a
/// round-by-round one too) or a jam plays its clip with the gun off the eye, so the player
/// comes out of the sights (and a scope) as it starts (the owner's report, #1642)
/// [design: the clip's timing is unchanged; the sights ease out as it begins].
pub fn sights_held_down(phase: &Phase) -> bool {
    matches!(phase, Phase::Reloading { .. } | Phase::ReloadOpen { .. } | Phase::ReloadRound { .. } | Phase::ReloadClose { .. } | Phase::Jammed { .. })
}

/// The weapon in hand for VATS mode ([`Combat::vats_weapon`]).
#[derive(Debug, Clone)]
pub struct VatsWeapon {
    pub id: FormId,
    pub edid: String,
    /// AP a mark (`fnv_combat::vats::shot_ap_cost`): a shot, a burst, a swing, a throw.
    pub cost: f32,
    /// How a mark is spent (vats-mode.md §16).
    pub style: fnv_combat::vats::AttackStyle,
    /// Rounds one mark takes (a burst's all).
    pub rounds_per_shot: u32,
    /// Rounds in the magazine now, or the thrown weapons carried (the marks' limit).
    pub rounds: u32,
    /// How far the crosshair's ray looks for a target: the projectile's range, else
    /// `vats_mode::REACH`; a thrown weapon's no farther than it throws; a melee
    /// weapon's `vats_mode::MELEE_MARK_REACH`.
    pub reach: f32,
    /// A melee weapon's own reach from the eye (`damage::melee_reach` plus the player's
    /// capsule radius), units; 0 otherwise.
    pub melee_reach: f32,
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var_os("NVF_PLAY_TEST_GOD").is_some_and(|v| v != "0") {
            set_god_mode(true);
        }
        crate::settings::add_setting(
            app,
            crate::settings::Setting {
                key: "reload-slows",
                group: crate::pause::GROUP_MENUS,
                label: "Slower while reloading",
                note: "On: you and the NPCs move at 75% of your speed while a reload runs. Off: reloading does not slow anyone, as in the original.",
                get: |_| reload_slows(),
                set: |_, on| set_reload_slows(on),
                slider: None,
            },
        );
        app.init_resource::<WeaponAnim>()
            .init_resource::<CombatStats>()
            .init_resource::<held_grenade_probe::Probe>()
            .init_resource::<ShotVisuals>()
            .add_message::<ShotFired>()
            .add_message::<ImpactFx>()
            .add_message::<CasingFx>()
            .add_message::<ExplosionFx>()
            .init_resource::<HitFeedback>()
            .init_resource::<HitMarks>()
            .init_resource::<PlayerLimbs>()
            .add_message::<ActorHit>()
            .add_message::<ForcedHitReaction>()
            .add_message::<ActorPush>()
            .add_message::<Knockback>()
            .add_message::<WeaponBroken>()
            .add_message::<ActorFire>()
            .add_message::<ActorStrike>()
            .init_resource::<crate::dismember::Dismemberment>()
            .add_message::<crate::dismember::Dismembered>()
            .add_systems(Startup, init_combat)
            .add_systems(
                Update,
                (track_limbs, (perk_speed, stat_speed), player_flinch, arm_sway, weapon_state, equip_sync, run_pace, reload_pace, weapon_sounds, fire, melee, actor_attacks, fly, crate::dismember::resolve, crate::mines::tick, crate::thrown::land, disarm, death_drops, meltdown, log_combat_lines).chain().in_set(PlaySet::Activation).run_if(in_state(PlayState::Playing)).run_if(resource_exists::<Combat>),
            )
            .add_systems(Update, (draw_marks, watch_hit_effects).run_if(resource_exists::<Combat>))
            .add_systems(Last, report);
    }
}

/// The player runs slower with a two-handed weapon drawn, by `fMove2HBigPenalty`
/// (`crate::player::RunPace`; the owner's playtest list 5 item 61) [design: every
/// two-handed weapon; hypothesis: the setting's role, from its name].
fn run_pace(c: Res<Combat>, control: Res<PlayerControl>, pace: Option<ResMut<crate::player::RunPace>>) {
    let Some(mut pace) = pace else { return };
    let two = control.weapon_out && c.weapon.and_then(|w| c.forms.weapons.get(&w)).is_some_and(WeaponStats::two_handed);
    let want = if two { (1.0 - c.rules.move_2h_penalty).clamp(0.1, 1.0) } else { 1.0 };
    if pace.0 != want {
        pace.0 = want;
        info!("movement: running at {:.0}% of the speed ({})", want * 100.0, if two { "a two-handed weapon drawn" } else { "no two-handed weapon drawn" });
    }
}

/// The share of their speed the player and NPCs keep while they reload, with the
/// settings page's `reload-slows` on [design: owner 2026-10-27, "slow the player and npc's
/// down to about 75% move speed" during a reload; not the original's rule, which does not
/// slow a reload]. NPCs: `nvforever` limping.rs.
pub const RELOAD_PACE: f32 = 0.75;

/// The `reload-slows` switch, for the process (NPCs are slowed by another crate); on by
/// default [design: owner 2026-10-27].
static RELOAD_SLOWS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Whether a reload slows the one reloading.
pub fn reload_slows() -> bool {
    RELOAD_SLOWS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Switch the reload's slowing on or off.
pub fn set_reload_slows(on: bool) {
    RELOAD_SLOWS.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The share of its speed an actor keeps: [`RELOAD_PACE`] while it reloads with the
/// setting on, all of it otherwise.
pub fn reload_share(reloading: bool) -> f32 {
    if reloading && reload_slows() {
        RELOAD_PACE
    } else {
        1.0
    }
}

/// The player moves slower while a reload runs ([`RELOAD_PACE`],
/// `crate::player::ReloadPace`).
fn reload_pace(c: Res<Combat>, pace: Option<ResMut<crate::player::ReloadPace>>) {
    let Some(mut pace) = pace else { return };
    let want = reload_share(c.cycle.as_ref().is_some_and(Cycle::reloading));
    if pace.0 != want {
        pace.0 = want;
        info!("movement: moving at {:.0}% of the speed ({})", want * 100.0, if want < 1.0 { "reloading" } else { "not slowed by a reload" });
    }
}

/// Scribe Counter's window: a blow begun this long (combat seconds) after the player's
/// block stopped one is the counter [design: "quick counter-attack after Block", the
/// fandom wiki; the original's window is not documented].
pub const COUNTER_WINDOW: f32 = 1.0;

/// An unarmed special move the player's perks give a power attack (perk-entry-points.md
/// §11).
#[derive(Debug, Clone, Copy)]
struct SpecialMove {
    name: &'static str,
    /// Its clip's stem: the weapon's prefix and this are the clip file
    /// (`h2hattackcustom1power.kf`), perk-entry-points.md §11.3.
    stem: &'static str,
    /// The DOBJ's effect for the direction, put on with the blow.
    spell: Option<FormId>,
    /// Legion Assault knocks the target back [the fandom wiki, "Knock back opponents";
    /// the DOBJ names no effect for it].
    knock_back: bool,
    /// Khan Trick forces the target's hit reaction ([`ForcedHitReaction`]).
    forces_reaction: bool,
    /// Legion Assault's clip lunges forward: the player moves with it, by the clip's own
    /// travel (`crate::player::Lunge`).
    lunges: bool,
}


/// The special move a power attack in `dir` is, when the player has its perk.
fn special_move(script: Option<&mut ScriptState>, sim: &mut SimState, dir: fnv_combat::melee::PowerDir) -> Option<SpecialMove> {
    use fnv_combat::melee::PowerDir;
    use fnv_game::data::DefaultObject as D;
    use fnv_game::perks as p;
    // The clips [observed: `h2hattackcustom1power.kf` … in both rigs' folders of Fallout -
    // Meshes.bsa; custom1's Bip01 goes 332 units forward over its 1.6 s, a lunge;
    // documented: GECK wiki GetForceHitReaction, "Khan Trick (AttackCustom4Power and
    // AttackCustom5Power)"; hypothesis: custom2 for the back by elimination, 4 left and 5
    // right in the entry points' order].
    let (point, slot, name, stem) = match dir {
        PowerDir::Forward => (p::HAS_UNARMED_FORWARD_POWER_ATTACK, D::UnarmedForwardPa, "Legion Assault", "attackcustom1power"),
        PowerDir::Back => (p::HAS_UNARMED_BACK_POWER_ATTACK, D::UnarmedBackwardPa, "Ranger Takedown", "attackcustom2power"),
        PowerDir::Left => (p::HAS_UNARMED_LEFT_POWER_ATTACK, D::UnarmedLeftPa, "Khan Trick", "attackcustom4power"),
        PowerDir::Right => (p::HAS_UNARMED_RIGHT_POWER_ATTACK, D::UnarmedRightPa, "Khan Trick", "attackcustom5power"),
        PowerDir::Stand => return None,
    };
    let rt = script.and_then(|s| s.rt.as_mut())?;
    let (has, spell) = with_world(rt, sim, |rt| (rt.game.player_has_entry(point), rt.game.data.default_object(slot)));
    let side = matches!(dir, PowerDir::Left | PowerDir::Right);
    has.then_some(SpecialMove { name, stem, spell, knock_back: dir == PowerDir::Forward, forces_reaction: side, lunges: dir == PowerDir::Forward })
}

/// Scribe Counter: its clip `h2hcounter.kf` [observed: both rigs' folders; hypothesis:
/// the move by the name], the DOBJ's counter effect (none in FalloutNV.esm).
fn scribe_counter(script: Option<&mut ScriptState>, sim: &mut SimState) -> Option<SpecialMove> {
    let rt = script.and_then(|s| s.rt.as_mut())?;
    let (has, spell) = with_world(rt, sim, |rt| (rt.game.player_has_entry(fnv_game::perks::HAS_UNARMED_COUNTER_ATTACK), rt.game.data.default_object(fnv_game::data::DefaultObject::UnarmedCounterPa)));
    has.then_some(SpecialMove { name: "Scribe Counter", stem: "counter", spell, knock_back: false, forces_reaction: false, lunges: false })
}

/// Knock `target` down away along `dir`, as a blast's knockdown does; logged with `why`.
fn knock_down(script: Option<&mut ScriptState>, by: FormId, target: FormId, dir: V3, why: &str) -> bool {
    let shove = [dir[0] * BLAST_KNOCKDOWN_SPEED, dir[1] * BLAST_KNOCKDOWN_SPEED, BLAST_KNOCKDOWN_SPEED * fnv_game::unconscious::PUSH_LIFT];
    let down = script.and_then(|s| s.rt.as_mut()).is_some_and(|rt| rt.game.knock_down(target, Some(by), shove));
    if down {
        info!("combat: {why} knocks {target} down");
    } else {
        info!("combat: {why}: {target} cannot be knocked down (No Knockdowns, or seated or talking)");
    }
    down
}

/// The Knockdown Chance perks of `attacker`'s hit with `weapon` on `target` (Super Slam
/// 0.15 / 0.30, And Stay Back 0.1, Elijah's Last Words 0.25; perk-entry-points.md §11):
/// from 0, rolled once a hit.
fn perk_knockdown(c: &mut Combat, mut script: Option<&mut ScriptState>, sim: &mut SimState, attacker: FormId, weapon: FormId, target: FormId, dir: V3) {
    let (chance, by) = owner_weapon_perk(script.as_deref_mut(), sim, attacker, fnv_game::perks::KNOCKDOWN_CHANCE, weapon, 0.0);
    if chance <= 0.0 || by.is_empty() {
        return;
    }
    let roll = c.rng.next_f32();
    if roll >= chance {
        info!("combat: Knockdown Chance {chance:.2} ({}): rolled {roll:.2}, {target} stays up", by.join(", "));
        return;
    }
    knock_down(script, attacker, target, dir, &format!("Knockdown Chance {chance:.2} ({}, rolled {roll:.2})", by.join(", ")));
}

/// Meltdown (Has Meltdown, perk-entry-points.md §9): a foe the player (or a teammate with
/// the perk) kills with an Energy Weapons weapon gives off the DOBJ's Meltdown Explosion
/// (`MinePlasmaExplosion` in FalloutNV.esm) at its body, the killer's blast with the same
/// weapon, so a kill by it does the same again: "this can cause a chain reaction" [observed:
/// the perk's DESC]. A body gives one off once.
#[allow(clippy::too_many_arguments)]
fn meltdown(mut c: ResMut<Combat>, mut hits: MessageReader<ActorHit>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, phys: Res<PhysState>, mut done: Local<HashSet<u32>>) {
    let kills: Vec<(u32, FormId, FormId)> = hits.read().filter(|h| h.died).filter_map(|h| Some((h.form_id, h.by?, h.weapon?))).collect();
    for (victim, by, weapon) in kills {
        let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
        if done.contains(&victim) || (by != FormId::PLAYER_REF && !rt.game.is_teammate(by)) {
            continue;
        }
        let energy = rt.game.data.weapon_skills.get(&weapon).copied() == Some(fnv_game::actor_values::ENERGY_WEAPONS as i64);
        let (has, explosion) = with_world(rt, &mut sim, |rt| {
            let g = &mut rt.game;
            (energy && g.entry_point(by, fnv_game::perks::HAS_MELTDOWN, 0.0).0 != 0.0, g.data.default_object(fnv_game::data::DefaultObject::MeltdownExplosion))
        });
        if !has {
            continue;
        }
        let Some(explosion) = explosion else {
            warn!("combat: Meltdown: the load order's DOBJ names no Meltdown Explosion");
            continue;
        };
        let Some(at) = phys.world.actor_aim_point(victim).or_else(|| sim.world.peek(FormId(victim)).map(|r| [r.pos[0], r.pos[1], r.pos[2] + 60.0])) else { continue };
        done.insert(victim);
        info!("combat: Meltdown: {} gives off {} ({by}'s kill with {})", FormId(victim), c.forms.explosions.get(&explosion).and_then(|x| x.edid.as_deref()).unwrap_or("?"), c.forms.weapons.get(&weapon).and_then(|w| w.edid.as_deref()).unwrap_or("?"));
        c.mine_explodes(explosion, at, Some((by, weapon)));
    }
}

/// The player's Modify Run Speed perks (Travel Light, Tunnel Runner, Implant M-5 …,
/// perk-entry-points.md §8): their factor on the player's movement, walking and running
/// alike, their conditions (armour, sneaking) asked every frame (`crate::player::PerkSpeed`).
fn perk_speed(c: Res<Combat>, mut anim: ResMut<WeaponAnim>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, speed: Option<ResMut<crate::player::PerkSpeed>>) {
    // The Equip Speed perks (Quick Draw) on the weapon in hand: the draw and holster
    // clips play that much faster (`WeaponAnim::equip_rate`, perk-entry-points.md §10).
    if let Some(weapon) = c.weapon {
        let (eq, by) = player_weapon_perk(script.as_deref_mut(), &mut sim, fnv_game::perks::EQUIP_SPEED, weapon, 1.0);
        let eq = if eq.is_finite() && eq > 0.0 { eq } else { 1.0 };
        if (eq - anim.equip_rate).abs() > 1e-4 && (anim.equip_rate > 0.0 || !by.is_empty()) {
            info!("combat: draws and holsters at x{eq:.2}{}", if by.is_empty() { String::new() } else { format!(" ({})", by.join(", ")) });
        }
        anim.equip_rate = eq;
    }
    let (Some(mut speed), Some(rt)) = (speed, script.as_deref_mut().and_then(|s| s.rt.as_mut())) else { return };
    let (want, by) = with_world(rt, &mut sim, |rt| {
        let g = &mut rt.game;
        let (v, applied) = g.entry_point(FormId::PLAYER_REF, fnv_game::perks::MODIFY_RUN_SPEED, 1.0);
        (v as f32, applied.iter().map(|a| g.data.name(a.perk)).collect::<Vec<_>>())
    });
    let want = want.clamp(0.1, 5.0);
    if (speed.0 - want).abs() > 1e-4 {
        speed.0 = want;
        if by.is_empty() {
            info!("movement: no Modify Run Speed perks apply: the player moves at 100%");
        } else {
            info!("movement: the Modify Run Speed perks make the player move at {:.0}% ({})", want * 100.0, by.join(", "));
        }
    }
}

/// The player's Speed Mult, body armour and size (`crate::player::StatSpeed`): asked every
/// frame, so SetAV / ModAV / ForceAV, effects, changes of armour and SetScale count at
/// once. A smaller player takes smaller steps: the speed goes with a script's scale, as a
/// creature's pace goes with its scale (`creature_pace`) [hypothesis: the clips' root
/// motion shrinks with the body; owner's TTW run 2026-11-02, the baby at
/// `player.SetScale 0.4` moved at full speed].
fn stat_speed(mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, speed: Option<ResMut<crate::player::StatSpeed>>) {
    let (Some(mut speed), Some(rt)) = (speed, script.as_deref_mut().and_then(|s| s.rt.as_mut())) else { return };
    let (mult, armor) = with_world(rt, &mut sim, |rt| (rt.game.move_speed_mult(FormId::PLAYER_REF) as f32, rt.game.armor_speed_mult(FormId::PLAYER_REF)));
    let scale = sim.script_scale(FormId::PLAYER_REF.0).unwrap_or(1.0).max(crate::sim::MIN_DRAWN_SCALE);
    let want = mult * scale;
    if (speed.0 - want).abs() > 1e-4 {
        speed.0 = want;
        let size = if scale == 1.0 { String::new() } else { format!(", scale {scale:.2}") };
        info!("movement: Speed Mult and body armour make the player move at {:.0}% (armour {:.0}%{size})", want * 100.0, armor * 100.0);
    }
}

/// Read the player's limb conditions from the script runtime (step C8).
fn track_limbs(mut limbs: ResMut<PlayerLimbs>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let (now, legs) = with_world(rt, &mut sim, |rt| (rt.game.limb_conditions(FormId::PLAYER_REF), rt.game.leg_speed_mult(FormId::PLAYER_REF)));
    let now = now.map(|v| v as f32);
    if legs as f32 != limbs.leg_speed {
        limbs.leg_speed = legs as f32;
        info!("movement: the player's legs allow {:.0}% of the speed", legs * 100.0);
    }
    if now != limbs.condition {
        let before = *limbs;
        limbs.condition = now;
        for c in 25u16..=31 {
            if limbs.crippled(c) != before.crippled(c) {
                info!("combat: player's {} {}", LIMB_NAMES[(c - 25) as usize], if limbs.crippled(c) { "crippled" } else { "healed" });
            }
        }
    }
}

/// The player flinches from a staggering hit (docs/spec-notes/crippled-limbs.md §3): the
/// hit that cripples a limb, a later hit on a crippled limb, or any hit while the torso
/// is crippled, by the rule NPCs stagger by (`damage::staggers`, with its cooldown after
/// the last flinch ended) [observed: `HelpCrippling`, "Chest: Higher chance to be
/// staggered", "These effects apply to you as well as to your enemies"; documented:
/// Fallout wiki crippling text, owner chat 2026-10-18, for the player a crippled torso
/// "throws off aim and interrupts attacks"; design: how]. The view is thrown off by up to
/// `flinch_kick_deg`, a swing under way is broken off, and for `flinch_time` no attack
/// starts (the trigger counts as released).
fn player_flinch(mut c: ResMut<Combat>, mut feedback: ResMut<HitFeedback>, mut player: ResMut<Player>, mut control: ResMut<PlayerControl>, mut anim: ResMut<WeaponAnim>) {
    let c = &mut *c;
    let now = c.time;
    for h in std::mem::take(&mut feedback.player_hits) {
        let since = c.flinch_end.map(|end| now - end);
        if now < c.flinch_until || !damage::staggers(&c.tuning, h.crippled, h.part_crippled, h.torso_crippled, since) {
            continue;
        }
        let (yaw, pitch) = ((c.flinch_rng.next_f32() * 2.0 - 1.0) * c.tuning.flinch_kick_deg, (c.flinch_rng.next_f32() * 2.0 - 1.0) * c.tuning.flinch_kick_deg * 0.5);
        player.heading += yaw.to_radians();
        player.pitch = (player.pitch + pitch.to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
        c.flinch_until = now + c.tuning.flinch_time;
        c.flinch_end = Some(c.flinch_until);
        let broke = c.swings.swing.is_some();
        c.swings.reset();
        anim.swing = None;
        let why = if h.crippled {
            "crippled now"
        } else if h.part_crippled {
            "crippled part hit"
        } else {
            "torso crippled"
        };
        info!("combat: the player flinches for {:.1} s ({why}): view thrown off by {yaw:.1}°, {pitch:.1}°{}", c.tuning.flinch_time, if broke { ", the swing broken off" } else { "" });
    }
    if now < c.flinch_until {
        control.trigger = false;
    }
}

/// An actor other than the player drops the weapon in its hand when a hit cripples one of
/// its arms (docs/spec-notes/crippled-limbs.md §3) [observed: FalloutNV.esm MESG
/// `HelpCrippling`, "Arms: … Enemies will be disarmed at the moment their arm is
/// crippled"; hypothesis: either arm, every actor but the player, as the text names
/// enemies]. The weapon falls as a broken one does (`WeaponBroken`): a reference in front
/// of it, out of its inventory. A weapon with the "Can't Drop" flag stays in the hand
/// [documented: GECK wiki Weapon, "the weapon cannot be shot out from the hands of
/// actors"].
fn disarm(mut feedback: ResMut<HitFeedback>, mut sim: ResMut<SimState>, mut stats: ResMut<CombatStats>, parts: Res<ActorParts>, mut broken: MessageWriter<WeaponBroken>) {
    for actor in std::mem::take(&mut feedback.disarms) {
        let Some(item) = parts.by_ref.get(&actor.0).and_then(|p| p.weapon) else { continue };
        if sim.forms.item(item).is_some_and(|it| it.cant_drop) {
            info!("combat: {actor}'s arm crippled: its {} ({item}) can't be dropped and stays in its hand", item_name(&sim, item));
            continue;
        }
        if sim.world.inventory_mut(actor).is_ok_and(|i| i.count(item) == 0) {
            let _ = sim.world.add_item(actor, item, 1);
        }
        let dropped = match sim.world.drop_equipped(actor, item) {
            Ok(refs) if !refs.is_empty() => Ok(refs),
            _ => sim.world.drop_item(actor, item, 1),
        };
        match dropped {
            Ok(refs) => {
                stats.disarmed += 1;
                let dropped = refs.first().copied();
                info!("combat: {actor}'s arm crippled: it drops its {} ({item}){}", item_name(&sim, item), dropped.map_or(String::new(), |r| format!(": reference {r}")));
                broken.write(WeaponBroken { form_id: actor.0, weapon: item, dropped });
            }
            Err(e) => warn!("combat: {actor}'s arm crippled: its {} could not be dropped: {e}", item_name(&sim, item)),
        }
    }
}

/// The default of `iDeathDropWeaponChance`, percent [observed: FalloutNV.esm, 100].
const DEATH_DROP_CHANCE: f32 = 100.0;

/// Actors seen alive with a weapon in the hand ([`death_drops`]).
#[derive(Default)]
struct DeathDrops {
    armed: HashMap<u32, FormId>,
    rng: Option<SplitMix>,
}

/// An actor other than the player that dies with a weapon in its hand drops it, with the
/// chance `iDeathDropWeaponChance` (percent; 100 in the masters) [hypothesis: the
/// setting's meaning, from its name; the original's corpses lie with their gun beside
/// them]. The weapon falls as a disarmed one does (`WeaponBroken`): a reference in front
/// of the body, out of its inventory. A weapon flagged "Can't Drop" stays with the body
/// [documented: GECK wiki Weapon, "the weapon cannot be shot out from the hands of
/// actors"; hypothesis: death too]. The dropped weapon stays lootable from the corpse:
/// its loot menu lists the reference beside the body's own items and taking it there
/// picks the one reference up (`RefState::dropped_by`; #1032) [hypothesis: the
/// original's corpses list the weapon lying beside them; owner's report]. Only deaths seen in play count: an actor dead when it
/// is drawn was never seen holding a weapon, so a corpse laid out by the game keeps its
/// own (`Game::starts_dead`).
fn death_drops(
    mut st: Local<DeathDrops>,
    mut sim: ResMut<SimState>,
    mut stats: ResMut<CombatStats>,
    parts: Res<ActorParts>,
    script: Option<Res<ScriptState>>,
    mut broken: MessageWriter<WeaponBroken>,
) {
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else { return };
    for (&id, p) in &parts.by_ref {
        if id == FormId::PLAYER_REF.0 || rt.game.is_dead(FormId(id)) {
            continue;
        }
        match p.weapon {
            Some(item) => st.armed.insert(id, item),
            None => st.armed.remove(&id),
        };
    }
    let died: Vec<(FormId, FormId)> = st.armed.iter().filter(|(id, _)| rt.game.is_dead(FormId(**id))).map(|(id, item)| (FormId(*id), *item)).collect();
    for (actor, _) in &died {
        st.armed.remove(&actor.0);
    }
    if died.is_empty() {
        return;
    }
    let chance = sim.forms.setting_f32("iDeathDropWeaponChance").unwrap_or(DEATH_DROP_CHANCE);
    for (actor, item) in died {
        let roll = st.rng.get_or_insert(SplitMix(5)).next_f32() * 100.0;
        if roll >= chance {
            info!("combat: {actor} died holding its {} ({item}): kept (roll {roll:.0} against iDeathDropWeaponChance {chance:.0})", item_name(&sim, item));
            continue;
        }
        if sim.forms.item(item).is_some_and(|it| it.cant_drop) {
            info!("combat: {actor} died holding its {} ({item}): it can't be dropped and stays with the body", item_name(&sim, item));
            continue;
        }
        if sim.world.inventory_mut(actor).is_ok_and(|i| i.count(item) == 0) {
            continue;
        }
        let dropped = match sim.world.drop_equipped(actor, item) {
            Ok(refs) if !refs.is_empty() => Ok(refs),
            _ => sim.world.drop_item(actor, item, 1),
        };
        match dropped {
            Ok(refs) => {
                stats.death_drops += 1;
                // The corpse's loot menu lists it, and taking it there picks this
                // reference up (`World::dropped_beside`, #1032).
                for r in &refs {
                    if let Ok(s) = sim.world.get(*r) {
                        s.dropped_by = Some(actor);
                    }
                }
                let dropped = refs.first().copied();
                info!("combat: {actor} died: it drops its {} ({item}){} (iDeathDropWeaponChance {chance:.0})", item_name(&sim, item), dropped.map_or(String::new(), |r| format!(": reference {r}")));
                broken.write(WeaponBroken { form_id: actor.0, weapon: item, dropped });
            }
            Err(e) => warn!("combat: {actor} died: its {} could not be dropped: {e}", item_name(&sim, item)),
        }
    }
}

/// The view sways while the player aims with a crippled arm (crippled-limbs.md §3): a
/// slow figure of eight of `arm_sway_deg` (one arm) or `arms_sway_deg` (both) at its
/// widest, which the shots follow as they leave along the view [documented: Fallout wiki
/// crippling text, owner chat 2026-10-18, "the first-person weapon sways, which shows
/// most through scopes"; design: the shape and size]. The view comes back when the arms
/// heal or the player stops aiming.
#[allow(clippy::type_complexity)]
fn arm_sway(time: Res<Time>, c: Res<Combat>, limbs: Res<PlayerLimbs>, control: Res<PlayerControl>, anim: Res<WeaponAnim>, mut player: ResMut<Player>, mut sway: Local<(f32, [f32; 2], bool)>) {
    let amp = match limbs.arms() {
        (true, true) => c.tuning.arms_sway_deg,
        (false, false) => 0.0,
        _ => c.tuning.arm_sway_deg,
    };
    let on = amp > 0.0 && control.aiming && anim.ready;
    let (t, last, was) = &mut *sway;
    if on != *was {
        info!("combat: the view {} (crippled arms)", if on { "sways while aiming" } else { "stops swaying" });
        *was = on;
    }
    let want = if on {
        *t += time.delta_secs();
        let a = *t / c.tuning.arm_sway_period.max(0.1) * std::f32::consts::TAU;
        [amp * a.sin(), 0.5 * amp * (2.0 * a).sin()]
    } else {
        *t = 0.0;
        [0.0, 0.0]
    };
    if want == *last {
        return;
    }
    player.heading += (want[0] - last[0]).to_radians();
    player.pitch = (player.pitch + (want[1] - last[1]).to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
    *last = want;
}

/// Tell the script runtime whether the player's weapon is out (`IsWeaponOut`): drawn and
/// in hand, as the body reports it [documented: the value changes when the weapon is
/// attached to the weapon node, catalogue notes of IsWeaponOut].
fn weapon_state(anim: Res<WeaponAnim>, mut script: Option<ResMut<ScriptState>>) {
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        rt.game.set_weapon_out(FormId::PLAYER_REF, anim.ready);
    }
}

fn init_combat(mut commands: Commands, d: Res<PlayWorld>) {
    let t = std::time::Instant::now();
    let tracer_hold = std::env::var("NVF_PLAY_TEST_TRACERS").ok().and_then(|v| v.trim().parse().ok()).filter(|s: &f32| *s > 0.0);
    let forms = CombatForms::build(d.lo, d.schema);
    let rules = Rules::from_forms(&forms);
    // A thrown grenade's restitution (combat-rules.md §5).
    let grenade_restitution = forms.setting("fGrenadeRestitution").unwrap_or(CombatTuning::default().grenade_restitution);
    let fists = forms.by_edid("Fists");
    info!("combat: {} weapons, {} ammo, {} projectiles, {} explosions read in {:.0} ms", forms.weapons.len(), forms.ammo.len(), forms.projectiles.len(), forms.explosions.len(), t.elapsed().as_secs_f64() * 1000.0);
    commands.insert_resource(Combat {
        forms,
        rules,
        tuning: CombatTuning {
            grenade_restitution,
            ..CombatTuning::default()
        },
        weapon: None,
        equipped: None,
        mods: 0,
        silenced_logged: None,
        fists,
        ammo: None,
        self_ammo: false,
        melee: false,
        swings: Melee::default(),
        blows: 0,
        boom: Vec::new(),
        debris: Vec::new(),
        cycle: None,
        kick: Kick::default(),
        flying: Vec::new(),
        settled: Vec::new(),
        landed: Vec::new(),
        splashes: Vec::new(),
        ghosts: Vec::new(),
        looping: None,
        ejects_seen: 0,
        last_shot: 0.0,
        next_visual: 0,
        pending: Vec::new(),
        script_shots: Vec::new(),
        guarding: HashMap::new(),
        blocked_blows: Vec::new(),
        fresh_reload: false,
        guard_down_until: 0.0,
        marks: Vec::new(),
        rng: SplitMix(1),
        swing_rng: SplitMix(2),
        time: 0.0,
        no_spread: std::env::var_os("NVF_PLAY_TEST_NO_SPREAD").is_some_and(|v| v != "0"),
        tracer_hold,
        load_magazine: None,
        load_ammo: None,
        crosshair_cone: None,
        player_spread: None,
        vats_cone: None,
        player_blocked_at: None,
        special_swing: None,
        throw_logged: None,
        spread_perks_logged: None,
        flinch_until: 0.0,
        flinch_end: None,
        flinch_rng: SplitMix(3),
        kick_rng: SplitMix(5),
        kick_just_settled: false,
        spent_rng: SplitMix(4),
    });
}

/// The player's spread perks for the weapon `id`: Modify Weapon Strength Req (points off
/// the requirement, from 0: Weapon Handling 2) and Modify Aiming Move Speed (the moving
/// penalty's divisor, from 1: Run 'n Gun × 2) [documented: GECK wiki Gun Spread Formula
/// (New Vegas), Perk entry points; perk-entry-points.md §12]. Logged when they change.
fn spread_perks(logged: &mut Option<(FormId, SpreadPerks)>, script: Option<&mut ScriptState>, sim: &mut SimState, id: FormId) -> SpreadPerks {
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return SpreadPerks::default() };
    let (cut, moving, by) = with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        let (cut, a) = g.entry_point_with(FormId::PLAYER_REF, fnv_game::perks::MODIFY_WEAPON_STRENGTH_REQ, &[Some(id)], 0.0);
        let (moving, b) = g.entry_point_with(FormId::PLAYER_REF, fnv_game::perks::MODIFY_AIMING_MOVE_SPEED, &[Some(id)], 1.0);
        let by: Vec<String> = a.iter().chain(&b).map(|x| g.data.name(x.perk)).collect();
        (cut as f32, moving as f32, by)
    });
    let p = SpreadPerks { strength_req_cut: cut, aiming_move_divisor: moving.max(1.0) };
    if *logged != Some((id, p)) {
        *logged = Some((id, p));
        if !by.is_empty() {
            info!("combat: spread perks for {id}: Strength requirement -{cut:.0}, moving penalty / {:.2} ({})", p.aiming_move_divisor, by.join(", "));
        }
    }
    p
}

/// What [`spread_perks`] gives the player's stance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpreadPerks {
    strength_req_cut: f32,
    aiming_move_divisor: f32,
}

impl Default for SpreadPerks {
    fn default() -> Self {
        SpreadPerks { strength_req_cut: 0.0, aiming_move_divisor: 1.0 }
    }
}

/// The player's stance for the spread: aiming, moving, running, sneaking, in the air,
/// the weapon's skill and Strength, crippled arms, the spread perks.
fn player_stance(control: &PlayerControl, phys: &PhysState, limbs: &PlayerLimbs, skill: f32, strength: f32, perks: SpreadPerks) -> Stance {
    Stance {
        aiming: control.aiming,
        moving: phys.player.as_ref().is_some_and(|ch| ch.velocity[0].hypot(ch.velocity[1]) > 1.0),
        running: control.always_run,
        sneaking: control.sneak,
        airborne: phys.player.as_ref().is_some_and(|ch| !ch.grounded && !ch.swimming),
        skill,
        strength,
        aiming_move_divisor: perks.aiming_move_divisor,
        strength_req_cut: perks.strength_req_cut,
        left_arm_crippled: limbs.arms().0,
        right_arm_crippled: limbs.arms().1,
        head_crippled: limbs.crippled(fnv_game::combat::HEAD),
    }
}

/// How many of `item` the player carries.
pub(crate) fn player_count(sim: &SimState, item: Option<FormId>) -> u32 {
    let inv = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref());
    item.and_then(|i| inv.map(|inv| inv.count(i))).unwrap_or(0).max(0) as u32
}

/// The rounds left in the magazine with `carried` of its ammo in the inventory, the
/// magazine's own among them: rounds dropped, sold or taken by a script are gone from the
/// gun too. A weapon that uses no ammo item, a regenerating one and god mode's full
/// magazine have no rounds in the inventory to follow.
fn magazine_in_hand(magazine: u32, carried: u32, no_ammo: bool, regenerates: bool, god: bool) -> u32 {
    if no_ammo || regenerates || god {
        magazine
    } else {
        magazine.min(carried)
    }
}

/// Follow the equipped weapon: a new weapon gets a new cycle, loaded from what is carried.
fn equip_sync(mut c: ResMut<Combat>, mut sim: ResMut<SimState>, mut anim: ResMut<WeaponAnim>) {
    let c = &mut *c;
    let equipped = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map(|i| i.equipped()).unwrap_or_default();
    let mut equipped = equipped.into_iter().find(|id| c.forms.weapons.contains_key(id));
    // A weapon at zero condition does not stay in the hand, however it got there (a
    // script's SetWeaponHealthPerc) (§17).
    if let Some(id) = equipped.filter(|&id| item_condition(&sim, FormId::PLAYER_REF, id) <= 0.0) {
        let _ = sim.world.unequip(FormId::PLAYER_REF, id);
        info!("combat: the player's {} ({id}) is broken: unequipped", item_name(&sim, id));
        equipped = None;
    }
    let mods = equipped.and_then(|id| sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map(|i| i.equipped_mods(id))).unwrap_or(0);
    if c.equipped == Some(equipped) && c.mods == mods {
        return;
    }
    // A mod put on the weapon in hand (`crate::pipboy::item_mod`): its rounds stay in.
    let kept = (c.equipped == Some(equipped)).then(|| c.cycle.as_ref().map(|cy| cy.magazine)).flatten();
    c.equipped = Some(equipped);
    c.mods = mods;
    let weapon = equipped.or(c.fists);
    c.weapon = weapon;
    c.cycle = None;
    c.ammo = None;
    c.melee = false;
    c.swings.reset();
    anim.swing = None;
    let Some(id) = weapon else {
        anim.attack = None;
        anim.reload = None;
        return;
    };
    let w = &c.player_stats(id).expect("weapon present");
    let mut ammo = c.forms.ammo_for(id, |a| player_count(&sim, Some(a)) > 0);
    // A save's type of round for this weapon (`crate::save`; #1574), while it is one of
    // the weapon's types and some is carried.
    if let Some((sw, a)) = c.load_ammo.take() {
        if sw == id.0 && c.forms.ammo_choices(w).contains(&a) && player_count(&sim, Some(a)) > 0 {
            ammo = Some(a);
        } else if sw != id.0 {
            c.load_ammo = Some((sw, a));
        }
    }
    let chain = c.forms.chain_with(id, w, ammo);
    let melee = AttackKind::of(w) != AttackKind::Gun;
    c.melee = melee && chain.projectile.is_none();
    c.self_ammo = !melee && chain.ammo_choices.is_empty() && chain.projectile.is_some();
    c.ammo = chain.ammo;
    let carried = player_count(&sim, c.ammo);
    let timing = Timing::of(w, &anim.timing, &c.tuning);
    // Ranged weapons without a projectile (the detonator, binoculars) use no ammo.
    let no_ammo = melee || c.self_ammo || (chain.projectile.is_none() && chain.ammo_choices.is_empty());
    c.cycle = Some(Cycle::new(w, timing, no_ammo, carried));
    if let (Some(n), Some(cy)) = (kept, c.cycle.as_mut()) {
        cy.magazine = n.min(cy.clip_size).min(carried);
    }
    // A save's magazine for this weapon (`crate::save`), no more than is carried.
    if let (Some((sw, n)), Some(cy)) = (c.load_magazine, c.cycle.as_mut()) {
        if sw == id.0 {
            cy.magazine = n.min(carried);
            c.load_magazine = None;
        }
    }
    anim.attack = w.attack_anim.clone();
    anim.reload = w.reload_anim.clone();
    anim.jam = w.reload_anim.as_deref().and_then(jam_anim);
    info!(
        "combat: weapon {} ({id}), ammo {}, magazine {}/{}, {} projectile(s) per shot",
        w.edid.as_deref().unwrap_or("?"),
        c.ammo.and_then(|a| c.forms.ammo.get(&a)).and_then(|a| a.edid.clone()).unwrap_or_else(|| if c.self_ammo { "itself".into() } else { "none".into() }),
        c.cycle.as_ref().map_or(0, |cy| cy.magazine),
        w.clip_size,
        chain.projectiles_per_shot
    );
    if c.mods != 0 {
        info!("combat: weapon {} mods {} on: damage {}, clip {}, min spread {:.2}, sight FOV {:.0}", w.edid.as_deref().unwrap_or("?"), c.mods, w.base_damage, w.clip_size, w.min_spread, w.sight_fov);
    }
}

/// The actor base `actor`'s Model/Animation data comes from: its impact material and a
/// creature's impact data set are read there, not from the placed base (a leveled list,
/// or an actor whose template supplies them; docs/spec-notes/actors.md §1.3). The placed
/// base without the game.
fn model_base(script: Option<&ScriptState>, sim: &SimState, actor: u32) -> Option<FormId> {
    let b = sim.base_of(actor)?;
    let r = FormId(actor);
    Some(script.and_then(|s| s.rt.as_ref()).map_or(b, |rt| { let (seed, at) = sim.pick(&rt.game, r); rt.game.data.model_base_at(b, seed, at) }))
}

/// An actor's Impact Material Type (NPC_ / CREA `NAM4`) from its Model/Animation source
/// ([`model_base`]), organic without one [design].
fn actor_material(s: &Sfx, script: Option<&ScriptState>, sim: &SimState, actor: u32) -> &'static str {
    model_base(script, sim, actor).and_then(|b| s.forms.actor_material(b)).unwrap_or("organic")
}

/// The impact a weapon's impact data set names for what a hit struck, and the material
/// it was chosen by: an actor's is its Impact Material Type ([`actor_material`]);
/// anything else's its collision material (docs/spec-notes/audio.md §11). Shots and
/// melee blows alike (item 24).
fn impact_for(s: &Sfx, script: Option<&ScriptState>, sim: &SimState, weapon: FormId, actor: Option<ActorId>, material: Option<u32>) -> (Option<FormId>, &'static str) {
    let material = match actor {
        Some(a) => actor_material(s, script, sim, a as u32),
        None => fnv_combat::fx::impact_material(material),
    };
    let set = s.forms.weapons.get(&weapon).and_then(|w| w.impact_dataset);
    (set.and_then(|set| s.forms.impact_id(set, material)), material)
}

/// Show an impact (the renderer's model and debris, `ImpactFx`) and play its sounds there.
fn show_impact(s: &Sfx, script: Option<&ScriptState>, sim: &SimState, mut fx: ImpactFx, sounds: &mut MessageWriter<PlaySound>, impacts: &mut MessageWriter<ImpactFx>) {
    // The base from the simulation: the game's world is not attached outside its turn.
    if let (Some(a), Some(rt)) = (fx.actor, script.and_then(|s| s.rt.as_ref())) {
        if let Some(b) = sim.base_of(a) {
            let (seed, at) = sim.pick(&rt.game, FormId(a));
            (fx.no_spray, fx.no_decal) = rt.game.data.no_blood(b, seed, at);
        }
    }
    impacts.write(fx);
    if let Some(i) = s.forms.impacts.get(&fx.impact) {
        for sound in i.sound_1.into_iter().chain(i.sound_2) {
            sounds.write(PlaySound { sound, at: Some(fx.point) });
        }
    }
}

/// The jam animation beside a reload animation: `ReloadA` → `JamA`, `ReloadX` → `JamX`
/// [observed: `1hpjama.kf`…`2hrjamz.kf` beside the reload clips in the meshes archive;
/// the letters' pairing is a hypothesis from the names].
pub fn jam_anim(reload: &str) -> Option<String> {
    let rest = reload.strip_prefix("Reload").or_else(|| reload.strip_prefix("reload"))?;
    (!rest.is_empty()).then(|| format!("Jam{rest}"))
}

/// Rays (shots, swings, blast lines) that passed a ghost's hit volume ([`ghosts`]), for
/// the exit summary.
static GHOST_PASSES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The physics world as combat sees it: rays that skip the player's capsule and the
/// layers nothing solid is on, and actor hit volumes.
struct PhysView<'a> {
    q: Queries<'a>,
    player: Option<Key>,
    /// The characters of the actors another system walks (`PhysState::npcs`).
    npcs: Vec<Key>,
    actors: Vec<(ActorId, V3, f32)>,
    /// Each actor's body part capsule centres, widest first.
    aims: HashMap<ActorId, Vec<V3>>,
    /// The ghosts (SetGhost 1) other than the player: rays pass their hit volumes
    /// ([`ghosts`]).
    ghosts: Vec<ActorId>,
}

impl CombatWorld for PhysView<'_> {
    fn ray(&self, from: V3, to: V3, ignore: &[ActorId]) -> Option<WorldHit> {
        let d = vec::sub(to, from);
        let len = vec::len(d);
        if len < 1e-4 {
            return None;
        }
        // Shots pass what the character passes (triggers, zones, water, …) and invisible
        // walls (`layers::shot_ignores`) [design]: they splash going into water
        // (`water_surface`, `ProjEvent::Splash`) and fly on.
        let mut f = Filter { exclude_layers: layers::shot_ignores(), ..Default::default() };
        // Another actor's shot can hit the player's character; the player's own and
        // the rest (blasts, picks) pass it, as before.
        let other_shoots = ignore.iter().any(|&a| a != PLAYER);
        if !other_shoots {
            f.exclude.extend(self.player);
        }
        // The shooter's own hit volume and character.
        f.exclude.extend(ignore.iter().filter(|&&a| a != PLAYER).flat_map(|&a| [Key::Actor(a as u32), Key::Character(a as u32)]));
        // An actor's character (the capsule that walks it) is not its body: its hit
        // volumes are. A shot or a swing that met the character first struck it as a
        // wall, dust and no damage, and one past a flying or low creature's capsule
        // missed its body (the owner's playtest list 5 items 16 and 25) [design].
        f.exclude.extend(self.npcs.iter().copied());
        // A ghost's hit volume is passed; counted when it would have been struck.
        if !self.ghosts.is_empty() {
            let struck = self.q.ray(from, d, len, &f).map(|h| h.owner.key());
            if struck.is_some_and(|k| self.ghosts.iter().any(|&a| k == Key::Actor(a as u32))) {
                GHOST_PASSES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        f.exclude.extend(self.ghosts.iter().map(|&a| Key::Actor(a as u32)));
        let h = self.q.ray(from, d, len, &f)?;
        let player_hit = other_shoots && self.player == Some(h.owner.key());
        Some(WorldHit {
            point: h.point,
            normal: h.normal,
            distance: h.distance,
            actor: match h.owner {
                Owner::Actor { form_id, .. } => Some(form_id as ActorId),
                _ if player_hit => Some(PLAYER),
                _ => None,
            },
            part: match h.owner {
                Owner::Actor { part, .. } if part != fnv_phys::actors::WHOLE => Some(part),
                _ => None,
            },
            small_transparent: matches!(h.owner, Owner::Ref { layer, .. } if layer == layers::TRANSPARENT_SMALL || layer == layers::TRANSPARENT_SMALL_ANIM),
            material: h.material,
            object: match h.owner {
                Owner::Ref { form_id, .. } => Some(form_id as u64),
                _ => None,
            },
        })
    }

    fn cover_ray(&self, from: V3, to: V3) -> Option<WorldHit> {
        let d = vec::sub(to, from);
        let len = vec::len(d);
        if len < 1e-4 {
            return None;
        }
        // The world only: no body, living or dead, shields another from a blast, nor
        // anything small or see-through (clutter, small debris, dropped weapons, fences,
        // thin trees); walls, rocks, the ground and wrecks do (the owner, 2026-10-27).
        let passes = layers::LayerSet::from_layers(&[
            layers::TRANSPARENT,
            layers::TRANSPARENT_SMALL,
            layers::TRANSPARENT_SMALL_ANIM,
            layers::INVISIBLE_WALL,
            layers::CLUTTER,
            layers::WEAPON,
            layers::PROJECTILE,
            layers::TREES,
            layers::BIPED,
            layers::DEAD_BIP,
        ]);
        let f = Filter { exclude_layers: layers::character_ignores().union(passes), no_actors: true, ..Default::default() };
        let h = self.q.ray(from, d, len, &f)?;
        Some(WorldHit {
            point: h.point,
            normal: h.normal,
            distance: h.distance,
            actor: None,
            part: None,
            small_transparent: matches!(h.owner, Owner::Ref { layer, .. } if layer == layers::TRANSPARENT_SMALL || layer == layers::TRANSPARENT_SMALL_ANIM),
            material: h.material,
            object: match h.owner {
                Owner::Ref { form_id, .. } => Some(form_id as u64),
                _ => None,
            },
        })
    }

    fn actors_near(&self, point: V3, radius: f32) -> Vec<(ActorId, V3)> {
        // A blast passes a ghost too: it "cannot be harmed by a weapon or spell/magic
        // effect in any way" [documented: GECK wiki SetGhost notes] (no knock-down either).
        let near = |&&(id, c, r): &&(ActorId, V3, f32)| vec::len(vec::sub(c, point)) <= radius + r && !self.ghosts.contains(&id);
        self.actors.iter().filter(near).map(|&(id, c, _)| (id, c)).collect()
    }

    fn aim_points(&self, actor: ActorId) -> Vec<V3> {
        self.aims.get(&actor).cloned().unwrap_or_default()
    }

    fn water_surface(&self, point: V3) -> Option<f32> {
        self.q.water_surface_for(point)
    }
}

/// The actors shots and blows pass through: ghosts (SetGhost 1), "Projectiles pass
/// through them" [documented: GECK wiki SetGhost notes]; a blow passes as a shot does
/// [hypothesis: the note names projectiles; a ghost "cannot be harmed by a weapon" in any
/// way, so a blow that stopped on it would only be wasted]. A ghosted player is still
/// struck, as "the player can" be damaged [documented: same notes; hypothesis: that its
/// body still stops projectiles]. Creatures whose base is flagged Is Ghost count as
/// ghosts too (docs/spec-notes/actor-flags.md §3).
fn ghosts(script: Option<&ScriptState>, phys: &PhysState, sim: Option<&SimState>) -> Vec<ActorId> {
    let (Some(rt), Some(sim)) = (script.and_then(|s| s.rt.as_ref()), sim) else { return Vec::new() };
    let set: Vec<ActorId> = rt.game.extra.iter().filter(|(r, e)| e.ghost && **r != FormId::PLAYER_REF).map(|(r, _)| r.0 as ActorId).collect();
    let flagged = phys.world.actor_volumes().filter(|&id| id != FormId::PLAYER_REF.0 && !set.contains(&(id as ActorId)) && sim.world.peek(FormId(id)).is_some_and(|s| rt.game.base_is_ghost(s.base, FormId(id))));
    let flagged: Vec<ActorId> = flagged.map(|id| id as ActorId).collect();
    set.into_iter().chain(flagged).collect()
}

fn view<'p>(phys: &'p mut PhysState, script: Option<&ScriptState>, sim: Option<&SimState>) -> PhysView<'p> {
    let ghosts = ghosts(script, phys, sim);
    let mut actors: Vec<(ActorId, V3, f32)> =
        phys.world.actor_volumes().filter_map(|id| phys.world.actor_volume(id).map(|v| (id as ActorId, v.center(), v.radius))).collect();
    // The player has no hit volume (its capsule is its body), so it was never among the
    // actors a blast reaches: mines, grenades and an NPC's incinerator did it no harm
    // (the owner's playtest list 7 items 19 and 29). Its capsule's middle stands in for
    // it; swings and proximity fuses skip their own attacker, so only others' reach it.
    if let Some(ch) = phys.player.as_ref() {
        let h = ch.params.height.max(0.0);
        actors.push((PLAYER, [ch.feet[0], ch.feet[1], ch.feet[2] + h / 2.0], ch.params.radius));
    }
    let player = phys.player.as_ref().map(|c| c.key());
    let npcs = phys.npcs.values().map(|b| b.ch.key()).collect();
    let aims = actors.iter().map(|&(id, ..)| (id, phys.world.actor_part_centers(id as u32))).filter(|(_, v)| !v.is_empty()).collect();
    phys.world.update_queries();
    PhysView { q: phys.world.queries(), player, npcs, actors, aims, ghosts }
}

/// An actor's current value as the formulas read it (within its range: S.P.E.C.I.A.L.
/// 1–10, skills 0–100), from the script runtime's actor values (the record, the
/// formulas and whatever scripts changed: docs/spec-notes/actor-values.md); `None`
/// without a runtime.
pub(crate) fn actor_av(script: Option<&mut ScriptState>, sim: &mut SimState, actor: FormId, code: u16) -> Option<f32> {
    let rt = script.and_then(|s| s.rt.as_mut())?;
    Some(with_world(rt, sim, |rt| fnv_game::functions::actor_value_capped(&mut rt.game, actor, code)) as f32)
}

/// An actor's attacker values with weapon `w`: the weapon's skill, Strength, Luck and
/// Unarmed, what the Melee Damage, Unarmed Damage and Critical Chance actor values hold
/// above their formulas (scripts' changes), and its arms' condition for the weapon
/// (`damage::arm_condition`: the player's and every other actor's). Without a script
/// runtime, the defaults.
fn attacker(mut script: Option<&mut ScriptState>, sim: &mut SimState, rules: &Rules, p: FormId, w: &WeaponStats, aiming_vats: bool) -> Attacker {
    use fnv_game::actor_values as avs;
    let d = Attacker::default();
    let skill = w.skill.as_deref();
    let mut av = |code: u16, or: f32| actor_av(script.as_deref_mut(), sim, p, code).unwrap_or(or);
    let (left, right) = (av(fnv_game::combat::LEFT_ARM, 100.0) <= 0.0, av(fnv_game::combat::RIGHT_ARM, 100.0) <= 0.0);
    let (strength, luck, unarmed_skill) = (av(avs::STRENGTH, d.strength), av(avs::LUCK, d.luck), av(avs::UNARMED, d.unarmed_skill));
    let skill = skill.and_then(fnv_game::functions::av_by_name).map_or(d.skill, |code| av(code, d.skill));
    let melee = damage::melee_damage_av(rules, strength);
    let unarmed = damage::unarmed_damage_av(rules, unarmed_skill);
    let crit = rules.crit_luck_base + rules.crit_luck_mult * luck;
    Attacker {
        skill,
        strength,
        luck,
        unarmed_skill,
        melee_damage_bonus: av(avs::MELEE_DAMAGE, melee) - melee,
        unarmed_damage_bonus: av(avs::UNARMED_DAMAGE, unarmed) - unarmed,
        crit_chance_bonus: av(avs::CRITICAL_CHANCE, crit) - crit,
        arm_condition: damage::arm_condition(w, left, right),
        in_vats: aiming_vats,
        nerve: companion_nerve(script, sim, p),
        ..d
    }
}

/// Companion Nerve of `actor` (`Game::companion_nerve`): for one of the player's
/// teammates the share its damage and armour go up by, from the player's Charisma; 0
/// for anyone else or without a script runtime.
fn companion_nerve(script: Option<&mut ScriptState>, sim: &mut SimState, actor: FormId) -> f32 {
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return 0.0 };
    with_world(rt, sim, |rt| rt.game.companion_nerve(actor)) as f32
}

/// Whether a hit by `attacker` on `target` is a sneak attack (the sneak multiplier and
/// always a critical, docs/spec-notes/combat-rules.md §4): the player's, made sneaking
/// (`sneaking`: as a swing lands, as a shot left), on a target that does not have the
/// player in its Detected band now; a pair detection has not measured counts as unseen,
/// as does every target without a script runtime.
fn sneak_attack(script: Option<&ScriptState>, attacker: FormId, sneaking: bool, target: FormId) -> bool {
    use fnv_game::detection::Band;
    attacker == FormId::PLAYER_REF
        && sneaking
        && script.and_then(|s| s.rt.as_ref()).is_none_or(|rt| rt.game.detection.get(target, FormId::PLAYER_REF).is_none_or(|d| d.band != Band::Detected))
}

/// The hit log's note on a critical: a sneak attack's says so.
fn crit_note(a: &Attacker, d: &damage::Damage) -> &'static str {
    match (d.critical, a.sneaking) {
        (false, _) => "",
        (true, false) => ", critical",
        (true, true) => ", sneak attack critical",
    }
}

/// A perk entry's resolved function as combat applies it.
fn perk_op(op: fnv_game::perks::Op) -> damage::PerkOp {
    use fnv_game::perks::Op;
    match op {
        Op::Set(x) => damage::PerkOp::Set(x as f32),
        Op::Add(x) => damage::PerkOp::Add(x as f32),
        Op::Mul(x) => damage::PerkOp::Mul(x as f32),
        Op::Abs => damage::PerkOp::Abs,
        Op::NegAbs => damage::PerkOp::NegAbs,
    }
}

/// `owner`'s perk entries of the entry points `points`, each with the argument tabs
/// `tabs` and `other` as the owner's target (`fnv_game::Game::entry_ops`).
fn entry_ops(g: &mut fnv_game::Game, owner: FormId, points: &[u8], tabs: &[Option<FormId>], other: Option<FormId>) -> damage::EntryOps {
    let mut out = damage::EntryOps::default();
    for &point in points {
        for (perk, op) in g.entry_ops(owner, point, tabs, other) {
            let name = g.data.name(perk);
            out.0.push((point, perk_op(op), name));
        }
    }
    out
}

/// The perk entries of one hit (docs/spec-notes/perk-entry-points.md §2): the
/// attacker's weapon damage, critical chance and damage and the DT its hit ignores, with
/// the tabs Weapon and Target; the target's change to the attacker's critical chance
/// (Weapon, then the attacker [hypothesis: the second tab]) and its own DT (Attacker,
/// Attacker's weapon) [documented: GECK wiki GetPerkModifier, the entry functions'
/// arguments]. Only the player and its teammates have perks (a teammate the teammate
/// perks, `fnv_game::Game::held_perks`); without a script runtime, none.
fn hit_perks(script: Option<&mut ScriptState>, sim: &mut SimState, attacker: FormId, weapon: FormId, target: Option<FormId>) -> (damage::EntryOps, damage::EntryOps) {
    use fnv_game::perks as p;
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return Default::default() };
    let holds = |r: FormId| r == FormId::PLAYER_REF || rt.game.is_teammate(r);
    if !holds(attacker) && !target.is_some_and(holds) {
        return Default::default();
    }
    with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        let mine = [p::CALCULATE_WEAPON_DAMAGE, p::CALCULATE_MY_CRITICAL_HIT_CHANCE, p::CALCULATE_MY_CRITICAL_HIT_DAMAGE, p::MODIFY_DAMAGE_THRESHOLD_ATTACKER];
        let a = entry_ops(g, attacker, &mine, &[Some(weapon), target], target);
        let t = match target {
            Some(t) => {
                let mut ops = entry_ops(g, t, &[p::MODIFY_ENEMY_CRITICAL_HIT_CHANCE], &[Some(weapon), Some(attacker)], Some(attacker));
                ops.0.extend(entry_ops(g, t, &[p::MODIFY_DAMAGE_THRESHOLD_DEFENDER], &[Some(attacker), Some(weapon)], Some(attacker)).0);
                ops
            }
            None => damage::EntryOps::default(),
        };
        (a, t)
    })
}

/// The perk entries of one blast on `target` (docs/spec-notes/perk-entry-points.md §2): of
/// [`hit_perks`], those a blast can use, the cause's weapon damage and the DT its hit
/// ignores, and the target's own DT. A blast with no cause has no attacker and no weapon
/// on those tabs, and nobody's stands in for them: what has no condition on them still
/// applies (Toughness), what has one does not (Hit the Deck) [design; hypothesis: that the
/// original does the same for a placed mine].
fn blast_entry_ops(g: &mut fnv_game::Game, cause: Option<Cause>, target: FormId) -> (damage::EntryOps, damage::EntryOps) {
    use fnv_game::perks as p;
    let a = cause.map_or_else(damage::EntryOps::default, |k| {
        entry_ops(g, k.actor, &[p::CALCULATE_WEAPON_DAMAGE, p::MODIFY_DAMAGE_THRESHOLD_ATTACKER], &[Some(k.weapon), Some(target)], Some(target))
    });
    let by = cause.map(|k| k.actor);
    let t = entry_ops(g, target, &[p::MODIFY_DAMAGE_THRESHOLD_DEFENDER], &[by, cause.map(|k| k.weapon)], by);
    (a, t)
}

/// [`blast_entry_ops`] in the played world; without a script runtime, none.
fn blast_perks(script: Option<&mut ScriptState>, sim: &mut SimState, cause: Option<Cause>, target: FormId) -> (damage::EntryOps, damage::EntryOps) {
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return Default::default() };
    with_world(rt, sim, |rt| blast_entry_ops(&mut rt.game, cause, target))
}

/// The player's value of entry point `point` with weapon `weapon` (the one argument tab
/// of spread, AP cost and the weapon's other figures) [documented: GECK wiki
/// GetPerkModifier], and the perks that changed it.
pub(crate) fn player_weapon_perk(script: Option<&mut ScriptState>, sim: &mut SimState, point: u8, weapon: FormId, v: f32) -> (f32, Vec<String>) {
    owner_weapon_perk(script, sim, FormId::PLAYER_REF, point, weapon, v)
}

/// [`player_weapon_perk`] for any owner: the player, or a teammate with the teammate
/// perks (`fnv_game::Game::held_perks`); others hold none.
pub(crate) fn owner_weapon_perk(script: Option<&mut ScriptState>, sim: &mut SimState, owner: FormId, point: u8, weapon: FormId, v: f32) -> (f32, Vec<String>) {
    use damage::PerkHooks;
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return (v, Vec::new()) };
    if owner != FormId::PLAYER_REF && !rt.game.is_teammate(owner) {
        return (v, Vec::new());
    }
    let ops = with_world(rt, sim, |rt| entry_ops(&mut rt.game, owner, &[point], &[Some(weapon)], None));
    (ops.entry(point, v), ops.names().into_iter().map(str::to_string).collect())
}

/// The item a spent round leaves the player (a case, a hull, a drained cell): the
/// ammo's DAT2 Consumed Ammo, given at its Consumed Percentage on every shot that spent
/// ammo [documented: GECK wiki Ammo, "Ammo Consumed (Percentage)": created in the
/// user's inventory "every time the weapon is fired"], the chance changed by the
/// player's Modify Chance for Ammo Item perks with the Weapon tab (Hand Loader and
/// Vigilant Recycler x 2) [documented: GECK wiki GetPerkModifier, entry 57's tab;
/// observed: the two perks' entries]. Not tied to Hardcore mode [hypothesis: the page
/// names no condition]. Its own rolls (`spent_rng`). docs/spec-notes/spent-ammo-items.md.
fn spent_ammo_item(rng: &mut SplitMix, stats: &mut CombatStats, script: Option<&mut ScriptState>, sim: &mut SimState, forms: &CombatForms, weapon: FormId, ammo: FormId) {
    let Some(a) = forms.ammo.get(&ammo) else { return };
    let Some((item, pct)) = a.consumed.filter(|(i, p)| i.0 != 0 && *p > 0.0) else { return };
    let (pct, by) = player_weapon_perk(script, sim, fnv_game::perks::MODIFY_CHANCE_FOR_AMMO_ITEM, weapon, pct);
    stats.spent_rolls += 1;
    let roll = rng.next_f32() * 100.0;
    if roll >= pct {
        return;
    }
    if sim.world.add_item(FormId::PLAYER_REF, item, 1).is_err() {
        warn!("combat: the spent item {item} of {} is not an item", a.edid.as_deref().unwrap_or("?"));
        return;
    }
    *stats.spent_given.entry(item.0).or_default() += 1;
    info!(
        "combat: spent ammo item {item} from {} given to the player ({pct:.0} % chance{})",
        a.edid.as_deref().unwrap_or("?"),
        if by.is_empty() { String::new() } else { format!(" after {}", by.join(", ")) }
    );
}

/// Logs what the perks of a hit changed, when they changed anything.
fn log_hit_perks(attacker: FormId, target: Option<FormId>, a: &damage::EntryOps, t: &damage::EntryOps, d: &damage::Damage) {
    if a.is_empty() && t.is_empty() {
        return;
    }
    let names = |o: &damage::EntryOps| o.names().join(", ");
    info!(
        "combat: perks on {attacker}'s hit{}: attacker [{}], target [{}]: weapon damage {:.2} -> {:.2}, crit chance {:.1} %{}, crit damage {:.1}, DT {:.1}, dealt {:.2}",
        target.map_or(String::new(), |t| format!(" on {t}")),
        names(a),
        names(t),
        d.base,
        d.modified,
        d.crit_chance,
        if d.critical { " (critical)" } else { "" },
        d.crit_bonus,
        d.dt,
        d.dealt
    );
}

/// Logs what the perks of a blast changed, when they changed anything.
fn log_blast_perks(cause: Option<Cause>, target: FormId, a: &damage::EntryOps, t: &damage::EntryOps, d: &damage::BlastHit) {
    if a.is_empty() && t.is_empty() {
        return;
    }
    info!(
        "combat: perks on {}'s blast on {target}: attacker [{}], target [{}]: damage {:.2} -> {:.2}, DT {:.1}, dealt {:.2}",
        cause.map_or("no one".to_string(), |k| k.actor.to_string()),
        a.names().join(", "),
        t.names().join(", "),
        d.base,
        d.modified,
        d.dt,
        d.dealt
    );
}

/// The weapon's Equip sound when the player starts to draw it and its Unequip sound
/// when they start to put it away [hypothesis: when the game plays them]; its Idle sound
/// (`UNAM`) going round while it is out (docs/spec-notes/audio.md §15.2).
fn weapon_sounds(
    c: Res<Combat>,
    control: Res<PlayerControl>,
    sfx: Option<Res<Sfx>>,
    (mut sounds, mut loops): (MessageWriter<PlaySound>, MessageWriter<LoopSound>),
    mut was_out: Local<bool>,
    mut idle: Local<Option<FormId>>,
) {
    let out = control.weapon_out;
    let fx = c.weapon.zip(sfx.as_deref()).and_then(|(w, s)| s.forms.weapons.get(&w));
    if out != *was_out {
        *was_out = out;
        if let Some(sound) = fx.and_then(|f| if out { f.equip } else { f.unequip }) {
            sounds.write(PlaySound { sound, at: None });
        }
    }
    let wanted = fx.filter(|_| out).and_then(|f| f.idle);
    if *idle != wanted {
        if let Some(old) = *idle {
            loops.write(LoopSound { sound: old, on: false });
        }
        if let Some(sound) = wanted {
            debug!("combat: the player's weapon idles: {}", sound_edid(sfx.as_deref(), sound));
            loops.write(LoopSound { sound, on: true });
        }
        *idle = wanted;
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn fire(
    time: Res<Time>,
    mut c: ResMut<Combat>,
    mut sim: ResMut<SimState>,
    mut phys: ResMut<PhysState>,
    mut player: ResMut<Player>,
    mut control: ResMut<PlayerControl>,
    mut anim: ResMut<WeaponAnim>,
    mut stats: ResMut<CombatStats>,
    mut script: Option<ResMut<ScriptState>>,
    limbs: Res<PlayerLimbs>,
    mut fired: MessageWriter<ShotFired>,
    sfx: Option<Res<Sfx>>,
    mut sounds: MessageWriter<PlaySound>,
    mut casings: MessageWriter<CasingFx>,
    mut loops: MessageWriter<LoopSound>,
    (mut hud, mut vats, lens_sway): (Option<ResMut<HudMessage>>, Option<ResMut<crate::vats_mode::VatsMode>>, Option<Res<crate::aim_zoom::LensSway>>),
) {
    let dt = time.delta_secs();
    let c = &mut *c;
    c.time += dt;
    // Where the gun points: the view, turned by a 3D scope's sway (the view itself sways
    // otherwise).
    let gun_dir = lens_sway.as_deref().map_or_else(|| player.look_dir(), |s| s.look_dir(player.heading, player.pitch));
    // The attack clip passed its `Eject` key since the last frame (the casing is thrown
    // below, with the weapon's effects at hand).
    let eject_now = anim.ejects != c.ejects_seen;
    c.ejects_seen = anim.ejects;
    // VATS mode's marked shot due now: the point it goes straight at (vats-mode.md §9.1).
    let vats_shot = vats.as_deref_mut().and_then(|v| v.take_shot());
    // The looping shoot sound stops a moment after the last shot.
    if let Some((sound, left)) = c.looping.as_mut() {
        *left -= dt;
        if *left <= 0.0 {
            loops.write(LoopSound { sound: *sound, on: false });
            c.looping = None;
        }
    }
    // Bloom shrinks whatever happens, once the weapon has rested for the settle delay.
    // The recoil's climb stays: pulling the view back down is the player's (owner,
    // 2026-10-24).
    let was_settled = c.kick.settled();
    c.kick.recover(&c.tuning, dt);
    // Logged with this frame's crosshair cone, below.
    c.kick_just_settled |= !was_settled && c.kick.settled();
    let reload = std::mem::take(&mut control.reload);
    let swap = std::mem::take(&mut control.ammo_swap);
    c.crosshair_cone = None;
    c.player_spread = None;
    if c.melee {
        c.vats_cone = None;
        return;
    }
    let Some(w) = c.weapon.and_then(|id| c.player_stats(id)) else {
        c.vats_cone = None;
        return;
    };
    let (Some(id), Some(cycle)) = (c.weapon, c.cycle.as_mut()) else { return };
    let god = god_mode();
    // Before the weapon's readiness: a holstered gun, the HUD and VATS mode see the same count.
    cycle.magazine = magazine_in_hand(cycle.magazine, player_count(&sim, c.ammo), cycle.no_ammo, cycle.regen.is_some(), god);
    if !anim.ready {
        if vats_shot.is_some() {
            info!("combat: the VATS mode shot could not leave (the weapon is not out)");
        }
        return;
    }
    // The reload at the player's rate: Agility, then the Reload Speed perks (Rapid
    // Reload); the body plays the reload clips at the same rate (`WeaponAnim::reload_rate`).
    let agility = actor_av(script.as_deref_mut(), &mut sim, FormId::PLAYER_REF, fnv_game::actor_values::AGILITY).unwrap_or(5.0);
    let (rate, _) = player_weapon_perk(script.as_deref_mut(), &mut sim, fnv_game::perks::RELOAD_SPEED, id, fnv_combat::attack::reload_rate(agility));
    let rate = if rate.is_finite() && rate > 0.0 { rate } else { 1.0 };
    if (rate - anim.reload_rate).abs() > 1e-4 {
        info!("combat: reloads at x{rate:.2} (Agility {agility:.0})");
    }
    anim.reload_rate = rate;
    // The attack at the player's rate: the Modify Attack Speed perks (Fast Shot, Loose
    // Cannon, Trigger Discipline …, perk-entry-points.md §10); the body plays the attack
    // clip at the weapon's speed times that (`WeaponAnim::attack_rate`).
    let (atk, by) = player_weapon_perk(script.as_deref_mut(), &mut sim, fnv_game::perks::MODIFY_ATTACK_SPEED, id, 1.0);
    let atk = if atk.is_finite() && atk > 0.0 { atk } else { 1.0 };
    let clip_rate = w.attack_mult.max(0.01) * w.anim_mult.max(0.01) * atk;
    if (clip_rate - anim.attack_rate).abs() > 1e-4 {
        if by.is_empty() {
            info!("combat: attacks at x{clip_rate:.2} ({})", w.edid.as_deref().unwrap_or("?"));
        } else {
            info!("combat: attacks at x{clip_rate:.2} ({}; perks x{atk:.2}: {})", w.edid.as_deref().unwrap_or("?"), by.join(", "));
        }
    }
    anim.attack_rate = clip_rate;
    cycle.timing = Timing::of(&w, &anim.timing, &c.tuning).with_reload_rate(rate).with_attack_rate(atk);
    // A VATS mode shot leaves now, whatever the weapon's cooldown or wind-up (the owner's
    // "faster than the weapon's normal rate"), unless it is reloading, jammed or empty.
    if vats_shot.is_some() {
        if cycle.reloading() || cycle.jammed() || (!cycle.no_ammo && cycle.magazine < cycle.ammo_use) {
            info!("combat: the VATS mode shot could not leave (reloading, jammed or empty)");
        } else {
            cycle.phase = fnv_combat::attack::Phase::WindUp { left: 0.0 };
        }
    }
    // Ammo Swap: the next carried type down the list, then a reload [design].
    if swap {
        if let Some(cur) = c.ammo {
            let next = c.forms.next_ammo(id, cur, |a| player_count(&sim, Some(a)) > 0);
            if next.is_some_and(|n| n != cur) {
                c.ammo = next;
                // God mode's magazine stays full: its shot goes on and nothing reloads.
                if !god {
                    cycle.swap_ammo();
                }
                info!("combat: ammo swapped to {}", next.and_then(|a| c.forms.ammo.get(&a)).and_then(|a| a.edid.clone()).unwrap_or_default());
            }
        }
    }
    // God mode: no ammo is used and nothing needs reloading (the owner's playtest list 5
    // item 49) [design]: the magazine stays full and shots take nothing from the
    // inventory.
    if god && !cycle.no_ammo && cycle.regen.is_none() && !c.self_ammo {
        cycle.magazine = cycle.clip_size;
    }
    let total = if c.self_ammo { player_count(&sim, Some(id)) } else { player_count(&sim, c.ammo) };
    let carried = total.saturating_sub(cycle.magazine);
    let was_winding = matches!(cycle.phase, fnv_combat::attack::Phase::WindUp { .. });
    cycle.regen_frozen = script.as_ref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.showoff.ammo_regen_frozen);
    let events = cycle.step(&c.tuning, Input { trigger: control.trigger, reload: reload && !god, dt, carried });
    // A pull whose shot waits for the clip's `Hit` key: the clip starts now (`wind_ups`).
    if !was_winding && cycle.timing.fire_delay > 0.0 && matches!(cycle.phase, fnv_combat::attack::Phase::WindUp { .. }) {
        anim.wind_ups += 1;
        let what = if c.self_ammo { "throw" } else { "shot" };
        if c.self_ammo {
            info!("combat: {} winds up, the {what} leaves at the clip's Hit key in {:.2} s", w.edid.as_deref().unwrap_or("?"), cycle.timing.fire_delay);
        } else {
            debug!("combat: {} winds up, the {what} leaves at the clip's Hit key in {:.2} s", w.edid.as_deref().unwrap_or("?"), cycle.timing.fire_delay);
        }
    }
    let chain = c.forms.chain_with(id, &w, c.ammo);
    // The cone a shot would leave in now, for the crosshair (bloom included).
    {
        let effects: Vec<&AmmoEffect> = chain.ammo_effects.iter().filter_map(|e| c.forms.ammo_effects.get(e)).collect();
        let shooter = attacker(script.as_deref_mut(), &mut sim, &c.rules, FormId::PLAYER_REF, &w, false);
        let sp = spread_perks(&mut c.spread_perks_logged, script.as_deref_mut(), &mut sim, id);
        let stance = player_stance(&control, &phys, &limbs, shooter.skill, shooter.strength, sp);
        let actor_spread = spread::actor_spread(&c.rules, &c.tuning, &w, &stance);
        let (actor_spread, _) = player_weapon_perk(script.as_deref_mut(), &mut sim, fnv_game::perks::CALCULATE_GUN_SPREAD, id, actor_spread);
        let cone = spread::cone_deg(actor_spread.max(0.0), c.kick.bloom, &w, &effects);
        c.crosshair_cone = Some(if c.no_spread { 0.0 } else { cone });
        if std::mem::take(&mut c.kick_just_settled) {
            info!(
                "combat: kick settled {:.2} s after the last shot, crosshair cone {cone:.2} deg; the burst climbed the view {:.2} deg up, {:.2} deg sideways",
                c.kick.since_shot, c.kick.climb_deg, c.kick.climb_yaw_deg
            );
        }
        c.player_spread = Some(if c.no_spread { 0.0 } else { actor_spread.max(0.0) });
        // VATS mode's hit chance: its own actor spread, no bloom (vats-mode.md §17.2).
        let vats_spread = spread::vats_actor_spread(&c.rules, &c.tuning, &w, &stance);
        let (vats_spread, _) = player_weapon_perk(script.as_deref_mut(), &mut sim, fnv_game::perks::CALCULATE_GUN_SPREAD, id, vats_spread);
        c.vats_cone = Some(if c.no_spread { 0.0 } else { spread::cone_deg(vats_spread.max(0.0), 0.0, &w, &effects) });
    }
    let weapon_fx = sfx.as_deref().and_then(|s| s.forms.weapons.get(&id));
    if eject_now && anim.ejects_on_key {
        throw_casing(&mut casings, &anim, weapon_fx, id, &c.rules);
        info!("combat: {} ejects its casing at the clip's Eject key, {:.2} s after the shot", w.edid.as_deref().unwrap_or("?"), c.time - c.last_shot);
    }
    for e in events {
        match e {
            Event::SpinUp => {
                anim.spin_ups += 1;
                debug!("combat: {} spins up, first shot in {:.2} s", w.edid.as_deref().unwrap_or("?"), cycle.timing.spin_up);
            }
            Event::Fired { rounds } => {
                anim.attacks += 1;
                c.last_shot = c.time;
                stats.shots += 1;
                if rounds > 0 && !god {
                    if let Some(a) = c.ammo {
                        let _ = sim.world.remove_item(FormId::PLAYER_REF, a, rounds as i32);
                        spent_ammo_item(&mut c.spent_rng, &mut stats, script.as_deref_mut(), &mut sim, &c.forms, id, a);
                    }
                }
                if c.self_ammo && !god {
                    let _ = sim.world.remove_item(FormId::PLAYER_REF, id, 1);
                }
                if c.self_ammo {
                    info!("combat: {} leaves the hand {:.2} s into the attack clip", w.edid.as_deref().unwrap_or("?"), cycle.timing.fire_delay);
                }
                // The weapon's OnFire blocks (the detonator's DetonatePlacedExplosives),
                // and the player's Attack or FireExplosive line (a thrown weapon).
                if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
                    if with_world(rt, &mut sim, |rt| rt.game.weapon_fired(FormId::PLAYER_REF, id)) {
                        info!("combat: OnFire of {} ran", w.edid.as_deref().unwrap_or("?"));
                    }
                    with_world(rt, &mut sim, |rt| rt.game.attack_line(FormId::PLAYER_REF, None, false, c.self_ammo));
                }
                let Some(proj) = chain.projectile else {
                    debug!("combat: {} fires no projectile", w.edid.as_deref().unwrap_or("?"));
                    continue;
                };
                fired.write(ShotFired { projectile: proj, weapon: id, shooter: FormId::PLAYER_REF, at: None, dir: None });
                // A casing for each shot of a weapon with a casing model, from its
                // `ShellCasingNode` at the game's eject speed (`fGunShellEjectSpeed`):
                // at the shot, unless the attack clip has an `Eject` key (a bolt, lever
                // or pump action), where it leaves when the clip reaches the key, below
                // [hypothesis: at the shot for the clips without the key].
                if !anim.ejects_on_key {
                    throw_casing(&mut casings, &anim, weapon_fx, id, &c.rules);
                }
                // A weapon with a looping shoot sound (the minigun has no other) plays
                // it for as long as shots keep coming [hypothesis: when the game does].
                if let Some(sound) = weapon_fx.and_then(|f| f.shoot_loop) {
                    if c.looping.is_none_or(|(s, _)| s != sound) {
                        if let Some((old, _)) = c.looping {
                            loops.write(LoopSound { sound: old, on: false });
                        }
                        loops.write(LoopSound { sound, on: true });
                    }
                    c.looping = Some((sound, LOOP_SOUND_HOLD));
                }
                // The player's own shot: the 2D sound where the weapon has one, the mod
                // set's with a Silence or Suppressor mod on (weapon-mods.md §7).
                let silenced = c.equipped.flatten() == Some(id) && w.sound_mod_on(c.mods);
                let shoot = weapon_fx.map(|f| f.shoot_sounds(silenced)).unwrap_or_default();
                if let Some(sound) = shoot.own {
                    if shoot.silenced && c.silenced_logged != Some((id, sound)) {
                        info!("combat: {} shoots its silenced sound {}", w.edid.as_deref().unwrap_or("?"), sound_edid(sfx.as_deref(), sound));
                    }
                    c.silenced_logged = shoot.silenced.then_some((id, sound));
                    sounds.write(PlaySound { sound, at: None });
                }
                let effects: Vec<&AmmoEffect> = chain.ammo_effects.iter().filter_map(|e| c.forms.ammo_effects.get(e)).collect();
                let shooter = attacker(script.as_deref_mut(), &mut sim, &c.rules, FormId::PLAYER_REF, &w, false);
                let sp = spread_perks(&mut c.spread_perks_logged, script.as_deref_mut(), &mut sim, id);
                let stance = player_stance(&control, &phys, &limbs, shooter.skill, shooter.strength, sp);
                // The Calculate Gun Spread perks change the actor spread, not the weapon's
                // minimum (docs/spec-notes/perk-entry-points.md §2).
                let formula = spread::actor_spread(&c.rules, &c.tuning, &w, &stance);
                let (actor_spread, by) = player_weapon_perk(script.as_deref_mut(), &mut sim, fnv_game::perks::CALCULATE_GUN_SPREAD, id, formula);
                let actor_spread = actor_spread.max(0.0);
                if !by.is_empty() {
                    debug!("combat: perks on the player's spread: {formula:.2} -> {actor_spread:.2} ({})", by.join(", "));
                }
                // A VATS mode shot has no spread but its crippled limbs' (crippled-limbs.md §3).
                let cone = if c.no_spread {
                    0.0
                } else if vats_shot.is_some() {
                    spread::half_angle(spread::deviation_deg(spread::crippled_spread(&c.rules, &c.tuning, &w, &stance), 0.0))
                } else {
                    spread::cone_deg(actor_spread, c.kick.bloom, &w, &effects)
                };
                let eye = player.eye();
                let gravity = phys.world.params.gravity;
                let (tracer_chance, force, range) =
                    chain.projectile.and_then(|p| c.forms.projectiles.get(&p)).map_or((0.0, 0.0, 0.0), |p| (p.tracer_chance, p.impact_force, p.range));
                // The aim is the view ray from the eye; the shot is drawn from the
                // muzzle, and in third person leaves it when the barrel is behind cover
                // (docs/design/combat.md §5.1).
                let reach = if range > 0.0 { range } else { AIM_REACH };
                // In third person the view ray is the camera's, through the crosshair over
                // the shoulder: the shot aims from the eye at what lies under it, as the
                // eye's view ray does in first person (playtest list 4, item 12).
                // Heave Ho!'s Modify Throwing Velocity: "run when ANY projectile is
                // created … the velocity of ANY projectile spawned by the perk owner
                // (filtered by conditions)" [documented: GECK wiki Entry Point]
                // (perk-entry-points.md §8).
                let (throw_mult, throw_by) = player_weapon_perk(script.as_deref_mut(), &mut sim, fnv_game::perks::MODIFY_THROWING_VELOCITY, id, 1.0);
                let throw_mult = throw_mult.max(0.01);
                let leaving = if let Some(point) = vats_shot.filter(|_| c.self_ammo) {
                    // VATS mode's throw: from the eye on the low arc that lands on the mark
                    // (vats-mode.md §16.3), at 45° when it is out of reach; at the speed the
                    // throw will leave with.
                    let (speed, g) = chain.projectile.and_then(|p| c.forms.projectiles.get(&p)).map_or((0.0, 0.0), |p| (p.speed * throw_mult, p.gravity * gravity));
                    let aim = fnv_combat::vats::lob_dir(eye, point, speed, g).unwrap_or_else(|| fnv_combat::vats::lob_far(eye, point));
                    shot::Leaving { origin: eye, aim, aim_point: point, aim_distance: vec::len(vec::sub(point, eye)), seen: Seen::ON_PATH, covered: false }
                } else {
                    let world = view(&mut phys, script.as_deref(), Some(&sim));
                    let aim = match control.crosshair.as_ref() {
                        // VATS mode: from the eye straight at the mark (100 % accuracy).
                        _ if vats_shot.is_some() => {
                            let d = vec::sub(vats_shot.unwrap_or(eye), eye);
                            if vec::len(d) > 1e-3 {
                                vec::normalize(d)
                            } else {
                                gun_dir
                            }
                        }
                        Some(cross) => {
                            let (start, dir) = crate::control::Crosshair::ray(Some(cross), eye, gun_dir);
                            let end = vec::add(start, vec::scale(dir, reach));
                            let hit = world.ray(start, end, &[PLAYER]).map(|h| h.point);
                            crate::control::Crosshair::aim_from(eye, start, dir, reach, hit)
                        }
                        None => gun_dir,
                    };
                    let leaving = shot::leaving(&world, eye, aim, anim.muzzle, reach, Some(PLAYER), control.third_person_view());
                    // An arcing explosive round (a grenade launcher's) leaves the muzzle,
                    // along the aim: the original's 40 mm round started 110 / 125 / 138
                    // units above the feet and 55 ahead at aim 0 / 15 / 30 degrees up,
                    // where our weapon's muzzle is (110 / 126 / 142, 64 ahead), not at the
                    // eye; from the eye it fell 5 % short at 15 degrees [observed: o26_gl,
                    // combat-rules.md §5, issue #1471]. Not when the barrel is in a wall.
                    let arcing = vats_shot.is_none()
                        && chain.explosion.is_some()
                        && chain.projectile.and_then(|p| c.forms.projectiles.get(&p)).is_some_and(|p| p.gravity > 0.0 && !p.has("Hitscan") && p.kind.as_deref() == Some("Missile"));
                    match anim.muzzle {
                        Some(m) if arcing && !leaving.covered && world.ray(eye, m, &[PLAYER]).is_none() => shot::Leaving { origin: m, seen: Seen::ON_PATH, ..leaving },
                        _ => leaving,
                    }
                };
                let rng = &mut c.rng;
                let mut rand = || rng.next_f32();
                let mut shots = shot::fire(&c.forms, &chain, leaving.origin, leaving.aim, cone, Some(PLAYER), gravity, &c.tuning, &mut rand);
                // A mod's Increase Projectile Speed (weapon-data-fields.md §5).
                if let Some(m) = w.projectile_speed_mult {
                    shots.iter_mut().for_each(|p| p.speed_up(m));
                }
                if (throw_mult - 1.0).abs() > 1e-4 {
                    shots.iter_mut().for_each(|p| p.speed_up(throw_mult));
                    if c.throw_logged != Some((id, throw_mult.to_bits())) {
                        info!("combat: perks change {}'s projectile speed x {throw_mult:.2} ({})", w.edid.as_deref().unwrap_or("?"), throw_by.join(", "));
                        c.throw_logged = Some((id, throw_mult.to_bits()));
                    }
                }
                stats.projectiles += shots.len() as u32;
                for p in shots {
                    let rolled = c.rng.next_f32() < tracer_chance || c.tracer_hold.is_some();
                    let shown = rolled || DRAW_EVERY_PROJECTILE;
                    // A line along the shot: for what is too fast to follow (found at
                    // once), and at the tracer chance for the rest [design].
                    let tracer = rolled || (shown && instant(&p));
                    let dir = vec::normalize(p.vel);
                    c.next_visual += 1;
                    let from_muzzle = anim.muzzle.is_some() && leaving.seen.at(leaving.origin, 0.0) != eye;
                    let by = By { actor: FormId::PLAYER_REF, weapon: id, ammo: c.ammo, harmless: false, vats: vats_shot.is_some(), sneaking: control.sneak };
                    c.flying.push(Flying { p, id: c.next_visual, proj, shown, dir, from_muzzle, origin: leaving.origin, seen: leaving.seen, tracer, projectiles_per_shot: chain.projectiles_per_shot, force, by, rref: None, registered: false, speed_mult: 1.0, ended_at: None });
                }
                let seen_from = leaving.seen.at(leaving.origin, 0.0);
                // The flash itself is the renderer's (`ShotFired`); the test aid marks
                // the point shots are drawn from.
                if let (Some(hold), true) = (c.tracer_hold, anim.muzzle.is_some()) {
                    c.marks.push(Mark::at(seen_from, hold, MarkKind::Flash));
                }
                stats.muzzle_shots += (seen_from != eye) as u32;
                stats.covered_shots += leaving.covered as u32;
                let p3 = |v: V3| format!("({:.1}, {:.1}, {:.1})", v[0], v[1], v[2]);
                debug!(
                    "combat: shot aimed from the eye {} at {} ({:.0} units), muzzle {}, drawn from {}{}",
                    p3(eye),
                    p3(leaving.aim_point),
                    leaving.aim_distance,
                    anim.muzzle.map_or("none (the eye)".to_string(), p3),
                    p3(seen_from),
                    if leaving.covered { "; cover in front of the barrel: it leaves the muzzle" } else { "" }
                );
                // VATS mode's shots kick nothing and grow no bloom [design, vats-mode.md §11.1],
                // nor do the NO_SPREAD test aid's, so each lands where it was aimed, nor a
                // throw (a grenade has no recoil).
                if vats_shot.is_none() && !c.no_spread && !c.self_ammo {
                    let rng = &mut c.kick_rng;
                    let kick = c.kick.shot(&c.tuning, &w, cycle.timing.interval, &mut || rng.next_f32());
                    player.pitch = (player.pitch + kick.pitch_deg.to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
                    player.heading += kick.yaw_deg.to_radians();
                    info!(
                        "combat: kick: shot {} in a row, bloom {:.2}, crosshair cone now {:.2} deg, view up {:.2} deg, sideways {:.2} deg",
                        c.kick.burst,
                        c.kick.bloom,
                        spread::cone_deg(actor_spread, c.kick.bloom, &w, &effects),
                        kick.pitch_deg,
                        kick.yaw_deg
                    );
                }
                debug!("combat: shot, cone {cone:.2}°, magazine {}", cycle.magazine);
                // The shot wears the weapon (§17); a thrown weapon is spent instead.
                if !c.self_ammo {
                    // Worn below the jamming mark, the shot may jam it (item 9): the jam
                    // clip plays and nothing fires until it ends. Weapons that use ammo
                    // only; a thrown one spends itself [design].
                    // The first shot after a reload of a weapon flagged No Jam After Reload
                    // does not jam [documented: the flag's name; hypothesis: the first shot;
                    // docs/spec-notes/item-fields.md §1].
                    let fresh = std::mem::take(&mut c.fresh_reload) && w.has_flag("No Jam After Reload");
                    let jams = !fresh && vats_shot.is_none() && c.ammo.is_some() && c.rng.next_f32() < condition::jam_chance(&c.tuning, item_condition(&sim, FormId::PLAYER_REF, id));
                    let points = condition::weapon_wear(&c.rules, &w, &effects);
                    if wear_weapon(&c.forms, &mut sim, &mut stats, hud.as_deref_mut(), None, script.as_deref_mut(), FormId::PLAYER_REF, id, points, None).is_some_and(|w| w.broke) {
                        break;
                    }
                    if jams {
                        // The jam clip's length, else the weapon's Jam Time, else the default.
                        let secs = anim.timing.jam_len.or(Some(w.jam_time).filter(|&t| t > 0.0)).unwrap_or(c.tuning.jam_fallback);
                        cycle.jam(secs);
                        anim.jams += 1;
                        stats.jams += 1;
                        info!(
                            "combat: {} jammed at {}% condition (clip {}, {secs:.2} s)",
                            w.edid.as_deref().unwrap_or("?"),
                            condition::percent(item_condition(&sim, FormId::PLAYER_REF, id)),
                            anim.jam.as_deref().unwrap_or("none")
                        );
                        break;
                    }
                }
            }
            Event::ReloadStarted => {
                anim.reloads += 1;
                match cycle.timing.rounds {
                    Some(r) => info!("combat: reloading round by round (opens {:.2} s, {:.2} s a round, closes {:.2} s)", r.open, r.round, r.close),
                    None => debug!("combat: reloading"),
                }
            }
            Event::ReloadRound => anim.reload_rounds += 1,
            Event::RoundLoaded => info!("combat: a round in, magazine {}", cycle.magazine),
            Event::ReloadClosing { stopped } => {
                info!("combat: closing{}", if stopped { " (the trigger was pulled)" } else { "" });
            }
            Event::Reloaded { taken } => {
                stats.reloads += 1;
                c.fresh_reload = true;
                info!("combat: reloaded {taken}, magazine {}", cycle.magazine);
            }
            Event::DryFire => {
                if let Some(sound) = weapon_fx.and_then(|f| f.no_ammo) {
                    sounds.write(PlaySound { sound, at: None });
                }
                if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
                    with_world(rt, &mut sim, |rt| rt.game.combat_line(FormId::PLAYER_REF, None, "NoAmmo"));
                }
                stats.dry_fires += 1;
                info!("combat: click (no ammo)");
            }
            Event::Regenerated => {}
            Event::Cleared => info!("combat: the jam is cleared"),
        }
    }
}

/// The target's DT and DR from the armour it wears (its carried ARMO by the wearing rule).
/// The simulation creates an actor's inventory on first touch (`World::inventory_mut`).
/// Each piece protects by its condition (§17).
pub(crate) fn target_armor(c: &Combat, mut script: Option<&mut ScriptState>, sim: &mut SimState, target: FormId) -> (f32, f32) {
    let (dt, dr) = c.forms.armor_values_worn(&worn_armor(&c.forms, sim, target));
    // The actor's own Damage Threshold (76) and Damage Resistance (18) actor values:
    // nothing until a script changes them (abilities are not applied yet).
    let own_dt = actor_av(script.as_deref_mut(), sim, target, 76).unwrap_or(0.0);
    let own_dr = actor_av(script, sim, target, 18).unwrap_or(0.0);
    (dt + own_dt, dr + own_dr)
}

/// `target`'s value, in percent, of the resistance weapon `w`'s damage type names
/// (`damage::resistance_of`, docs/spec-notes/weapon-data-fields.md §3); 0 for a weapon
/// whose damage DR takes, or without a script runtime. Logged when above 0.
fn target_resist(script: Option<&mut ScriptState>, sim: &mut SimState, target: FormId, w: &WeaponStats) -> f32 {
    let Some(name) = damage::resistance_of(w) else { return 0.0 };
    let Some(code) = fnv_game::functions::av_by_name(name) else { return 0.0 };
    let v = actor_av(script, sim, target, code).unwrap_or(0.0);
    if v > 0.0 {
        info!("combat: {target} takes {} damage less {v:.0} % ({name}) in place of DR", w.edid.as_deref().unwrap_or("?"));
    }
    v
}

/// The armour `target` wears, each piece with its condition: what the player has
/// equipped; of another actor, what the rules dress it in (`World::equipment`: its
/// equipped items once outfitted, as the engine draws it), else its carried ARMO by the
/// wearing rule.
fn worn_armor(forms: &CombatForms, sim: &mut SimState, target: FormId) -> Vec<(FormId, f32)> {
    let dressed = if target == FormId::PLAYER_REF { None } else { sim.world.equipment(target) };
    let Ok(inv) = sim.world.inventory_mut(target) else { return Vec::new() };
    let items: Vec<FormId> = match dressed {
        Some(d) => d,
        None if target == FormId::PLAYER_REF => inv.equipped(),
        None => inv.iter().filter(|(_, n)| *n > 0).map(|(id, _)| id).collect(),
    };
    forms.worn_armor(items).into_iter().map(|id| (id, inv.condition_in_use(id).unwrap_or(1.0))).collect()
}

/// The condition of the unit of `item` that `actor` uses; 1 when it carries none (the
/// Fists).
fn item_condition(sim: &SimState, actor: FormId, item: FormId) -> f32 {
    sim.world.peek(actor).and_then(|s| s.inventory.as_ref()).and_then(|i| i.condition_in_use(item)).unwrap_or(1.0)
}

fn item_name(sim: &SimState, item: FormId) -> String {
    sim.name(item).map_or_else(|| item.to_string(), str::to_string)
}

/// A hit of `damage` (before armour) wears the armour `target` wears in `zone` (§17):
/// the player's only. Another actor's armour keeps its condition [observed: Cook-Cook's
/// armour stayed at health 85 and his DT at 13 through 60 hits, and the troopers' and
/// Fiends' through theirs, AI observer campaign fight_stats_round 2026-10-19,
/// npc-behaviour-compare.md §9].
fn wear_armor(c: &Combat, sim: &mut SimState, stats: &mut CombatStats, target: FormId, zone: Zone, damage: f32) {
    if target != FormId::PLAYER_REF {
        return;
    }
    let worn: Vec<FormId> = worn_armor(&c.forms, sim, target).into_iter().map(|w| w.0).collect();
    let struck = c.forms.armor_struck(&worn, zone);
    if struck.is_empty() || damage <= 0.0 {
        return;
    }
    let points = condition::armor_wear(&c.rules, damage) / struck.len() as f32;
    for item in struck {
        let health = c.forms.item_health(item);
        let Some(w) = sim.world.inventory_mut(target).ok().and_then(|inv| condition::wear_item(inv, item, points, health)) else { continue };
        stats.armor_wear += (w.before - w.after) * health as f32;
        info!("combat: {target}'s {} ({zone:?} hit) loses {points:.1}: condition {:.1}% -> {:.1}%", item_name(sim, item), w.before * 100.0, w.after * 100.0);
        if w.broke {
            stats.armor_broken += 1;
            info!("combat: {target}'s {} ({item}) is worn to nothing", item_name(sim, item));
        }
    }
}

/// Take `points` health points from the weapon `item` that `actor` uses (§17). At zero
/// it breaks: the player's is unequipped and stays in the inventory, where it cannot be
/// equipped until it is repaired; another actor's falls to the ground (at `at`, when
/// given) and [`WeaponBroken`] tells the renderer, unless it has the "Can't Drop" flag:
/// then it is unequipped and stays in the inventory as the player's does [documented:
/// GECK wiki Weapon, a Can't Drop weapon "can still be disabled and rendered inoperable";
/// hypothesis: that it is unequipped rather than kept in the hand]. `None` when the weapon does not wear
/// (not carried, or its record has no health).
#[allow(clippy::too_many_arguments)]
pub fn wear_weapon(
    forms: &CombatForms,
    sim: &mut SimState,
    stats: &mut CombatStats,
    hud: Option<&mut HudMessage>,
    broken: Option<&mut MessageWriter<WeaponBroken>>,
    mut script: Option<&mut ScriptState>,
    actor: FormId,
    item: FormId,
    points: f32,
    at: Option<V3>,
) -> Option<Wear> {
    // The player's Modify Item Damage perks (Regular Maintenance × 0.5, Built to
    // Destroy × 1.15) (docs/spec-notes/perk-entry-points.md §2).
    let points = if actor == FormId::PLAYER_REF {
        player_weapon_perk(script.as_deref_mut(), sim, fnv_game::perks::MODIFY_ITEM_DAMAGE, item, points).0.max(0.0)
    } else {
        points
    };
    // A creature's body weapon ("Embedded Weapon") is part of it and does not wear
    // [design: docs/design/combat-ai.md §19; what harms it is its limb, the record's
    // Embedded Weapon Actor Value].
    if forms.weapons.get(&item).is_some_and(|w| w.embedded) {
        return None;
    }
    let health = forms.item_health(item);
    let w = condition::wear_item(sim.world.inventory_mut(actor).ok()?, item, points, health)?;
    if actor == FormId::PLAYER_REF {
        stats.weapon_wear += (w.before - w.after) * health as f32;
    }
    if !w.broke {
        return Some(w);
    }
    stats.weapons_broken += 1;
    // Its owner says WeaponBroke (docs/spec-notes/combat-dialogue.md).
    if let Some(rt) = script.and_then(|s| s.rt.as_mut()) {
        with_world(rt, sim, |rt| rt.game.combat_line(actor, None, "WeaponBroke"));
    }
    let name = item_name(sim, item);
    if actor != FormId::PLAYER_REF && sim.forms.item(item).is_some_and(|it| it.cant_drop) {
        let _ = sim.world.unequip(actor, item);
        info!("combat: {actor}'s {name} ({item}) broke: it can't be dropped, so it is unequipped and kept");
        if let Some(b) = broken {
            b.write(WeaponBroken { form_id: actor.0, weapon: item, dropped: None });
        }
        return Some(w);
    }
    if actor == FormId::PLAYER_REF {
        let _ = sim.world.unequip(actor, item);
        if let Some(h) = hud {
            h.show(format!("{name} is broken"));
        }
        info!("combat: the player's {name} ({item}) broke: unequipped, it stays in the inventory until repaired");
        return Some(w);
    }
    let dropped = match sim.world.drop_equipped(actor, item) {
        Ok(refs) => refs.first().copied(),
        Err(e) => {
            warn!("combat: {actor}'s broken {name} could not be dropped: {e}");
            None
        }
    };
    if let (Some(r), Some(at)) = (dropped, at) {
        if let Ok(s) = sim.world.get(r) {
            s.pos = at;
        }
    }
    if let Some(h) = hud {
        h.show(format!("{name} broken"));
    }
    info!("combat: {actor}'s {name} ({item}) broke and fell{}", dropped.map_or(String::new(), |r| format!(": reference {r}")));
    if let Some(b) = broken {
        b.write(WeaponBroken { form_id: actor.0, weapon: item, dropped });
    }
    Some(w)
}

/// An actor other than the player wore its weapon by its own attack (§17): `points`
/// come off the unit it holds, counted for the exit summary. One broken by it falls
/// where it is (`at`) as one hit to nothing, and its owner says WeaponBroke. A weapon
/// the simulation's inventory lacks (the renderer's choice, `strike_weapon`) does not
/// wear [design].
///
/// Only a weapon whose record sets its own Damage to Weapon Mult wears from an NPC's
/// attacks [observed: observer captures o14_reach and o15_round, 2026-11-01 (#1334): a
/// lead pipe, sledgehammer, 10mm Pistol and Hunting Rifle stayed at their exact health
/// through 10-42 attacks each, against god-mode and mortal players; Cook-Cook's
/// Incinerator, which has the override, wore. That the override is what decides is
/// [hypothesis], the one rule both fit].
#[allow(clippy::too_many_arguments)]
fn wear_actor_weapon(
    forms: &CombatForms,
    sim: &mut SimState,
    stats: &mut CombatStats,
    broken: &mut MessageWriter<WeaponBroken>,
    script: Option<&mut ScriptState>,
    actor: FormId,
    item: FormId,
    points: f32,
    at: V3,
) {
    if actor == FormId::PLAYER_REF || points <= 0.0 || !npc_weapon_wears(forms, item) {
        return;
    }
    let Some(w) = wear_weapon(forms, sim, stats, None, Some(broken), script, actor, item, points, Some(at)) else { return };
    let e = stats.npc_weapon_wear.entry((actor.0, item.0)).or_insert(NpcWeaponWear { first: w.before, ..NpcWeaponWear::default() });
    e.attacks += 1;
    e.points += (w.before - w.after) * forms.item_health(item) as f32;
    e.last = w.after;
}

/// Whether an NPC's own attacks wear `weapon`: only one whose record overrides Damage to
/// Weapon Mult ([`wear_actor_weapon`]).
pub fn npc_weapon_wears(forms: &CombatForms, weapon: FormId) -> bool {
    forms.weapons.get(&weapon).is_some_and(|w| w.damage_to_weapon_mult.is_some())
}

/// A hit landed on the weapon `target` holds (§17): the hit's damage comes off the
/// weapon's condition; the actor is not hurt [design: the owner's rule, 2026-10-11].
/// The weapon is the one the renderer put in the hand (`PosedParts::weapon`); when the
/// simulation's inventory, rolled on its own, lacks it, one is added first [design].
#[allow(clippy::too_many_arguments)]
fn strike_weapon(
    forms: &CombatForms,
    sim: &mut SimState,
    stats: &mut CombatStats,
    hud: Option<&mut HudMessage>,
    broken: &mut MessageWriter<WeaponBroken>,
    script: Option<&mut ScriptState>,
    parts: &ActorParts,
    target: FormId,
    points: f32,
    at: V3,
) {
    let Some(item) = parts.by_ref.get(&target.0).and_then(|p| p.weapon) else { return };
    if sim.world.inventory_mut(target).is_ok_and(|i| i.count(item) == 0) {
        let _ = sim.world.add_item(target, item, 1);
    }
    stats.weapon_hits += 1;
    match wear_weapon(forms, sim, stats, hud, Some(broken), script, target, item, points, Some(at)) {
        Some(w) => info!("combat: hit {target}'s {} for {points:.1}: condition {:.1}% -> {:.1}%", item_name(sim, item), w.before * 100.0, w.after * 100.0),
        None => info!("combat: hit {target}'s {} ({item}): it does not wear", item_name(sim, item)),
    }
}

/// The magic items a hit by `attacker` with `weapon` applies (`hit_effects`): the
/// weapon's object effect; for a blow without a weapon or with the Fists, the attacker's
/// unarmed attack effect.
/// Read inside the simulation's world: an attacker a script created (PlaceAtMe) is only
/// there. `anim`: a power attack's attack group ([`ActorStrike::anim`]).
#[allow(clippy::too_many_arguments)]
fn hit_items(c: &Combat, script: Option<&mut ScriptState>, sim: &mut SimState, stats: &mut CombatStats, attacker: FormId, weapon: Option<FormId>, anim: Option<&str>) -> Vec<FormId> {
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return Vec::new() };
    let unarmed = weapon.is_none() || weapon == c.fists;
    with_world(rt, sim, |rt| hit_effects::hit_items(&c.forms, &rt.game, Some(attacker), weapon, unarmed, anim, &mut stats.hit_effects))
}

/// A weapon's critical effect (WEAP CRDT: the actor effect and its On Death flag), for
/// [`apply_hit`] (`fnv_game::ash_piles`, docs/spec-notes/ash-piles.md §1).
pub(crate) fn crit_effect(c: &Combat, weapon: Option<FormId>) -> Option<(FormId, bool)> {
    let w = c.forms.weapons.get(&weapon?)?;
    Some((w.crit_effect?, w.crit_on_death))
}

/// Apply `dealt` health damage through the script runtime; report and remember it. A
/// critical hit then gives the weapon's critical effect (`crit`: the actor effect and
/// its On Death flag; `fnv_game::Game::apply_crit_effect`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_hit(
    script: Option<&mut ScriptState>,
    sim: &mut SimState,
    target: FormId,
    dealt: f32,
    info: HitInfo,
    stats: &mut CombatStats,
    feedback: &mut HitFeedback,
    hud: Option<&mut HudMessage>,
    messages: &mut MessageWriter<ActorHit>,
    effects: &[FormId],
    guard_broken: bool,
    crit: Option<(FormId, bool)>,
) -> Option<HitResult> {
    let (r, hit) = hit_actor(script, sim, target, dealt, info, stats, feedback, hud, effects, guard_broken, crit, &[])?;
    if let Some(h) = hit {
        messages.write(h);
    }
    Some(r)
}

/// [`apply_hit`] with an explosion's limb damage (`blast`, [`blast_limb_hits`]; empty for
/// any other hit), which goes on after the health damage and counts as the hit's own: a
/// limb it cripples disarms, shows on the HUD and makes the hit a crippling one for the
/// stagger rule (crippled-limbs.md §5). The [`ActorHit`] to send is returned, for the
/// caller to send once it has decided a knockdown (none for the player, whose
/// [`PlayerHit`] is in `feedback`).
#[allow(clippy::too_many_arguments)]
fn hit_actor(
    script: Option<&mut ScriptState>,
    sim: &mut SimState,
    target: FormId,
    dealt: f32,
    info: HitInfo,
    stats: &mut CombatStats,
    feedback: &mut HitFeedback,
    mut hud: Option<&mut HudMessage>,
    effects: &[FormId],
    guard_broken: bool,
    crit: Option<(FormId, bool)>,
    blast: &[LimbHit],
) -> Option<(HitResult, Option<ActorHit>)> {
    let mine = info.attacker == Some(FormId::PLAYER_REF);
    let rt = script.and_then(|s| s.rt.as_mut())?;
    if target == FormId::PLAYER_REF {
        return hit_player(rt, sim, dealt, info, stats, hud, effects, feedback, blast).map(|r| (r, None));
    }
    // The hit's effects go on before its damage (`hit_effects`).
    let span = info_span!("combat::hit_effects").entered();
    with_world(rt, sim, |rt| hit_effects::apply(&mut rt.game, &mut stats.hit_effects, target, effects, info.attacker));
    drop(span);
    let span = info_span!("combat::game_hit").entered();
    let mut r = with_world(rt, sim, |rt| rt.game.hit(target, dealt as f64, info));
    drop(span);
    let blasted = blast_limbs(rt, sim, target, info, blast, &r);
    r.crippled |= !blasted.is_empty();
    let name = sim.ref_name(Some(&rt.game), target.0).map_or_else(|| target.to_string(), str::to_string);
    if let Some((spell, on_death)) = crit.filter(|_| info.critical && !r.immune) {
        let _span = info_span!("combat::crit_effect").entered();
        let done = with_world(rt, sim, |rt| rt.game.apply_crit_effect(target, spell, on_death, r.died, info.attacker));
        if let fnv_game::ash_piles::CritEffect::Applied(ids) = done {
            let spell_name = rt.game.data.name(spell);
            info!("combat: critical effect {spell_name} ({spell}) on {name} {target}{}: {} effect(s)", if on_death { " (on death)" } else { "" }, ids.len());
        }
    }
    if mine {
        stats.damage += r.dealt as f32;
    } else if info.attacker.is_some() && !r.immune {
        let e = stats.npc_hits.entry(target.0).or_insert_with(|| (name.clone(), Vec::new(), r.max_health as f32));
        e.1.push(r.dealt as f32);
    }
    if r.died {
        if mine {
            stats.kills += 1;
            stats.xp += r.xp;
            if let Some(h) = hud.as_deref_mut() {
                h.show(if r.xp > 0.0 { format!("{name} killed (+{} XP)", r.xp) } else { format!("{name} killed") });
            }
        }
        let by = info.attacker.filter(|a| *a != FormId::PLAYER_REF).map_or(String::new(), |a| format!(" by {a}"));
        info!("combat: {name} {target} killed{by}{}", if r.xp > 0.0 { format!(", +{} XP", r.xp) } else { String::new() });
    } else if r.knocked_out {
        info!("combat: {name} {target} knocked out (essential)");
    }
    // The limbs this hit crippled: the one it struck, or those a blast did.
    let crippled: Vec<u16> = match r.limb {
        Some((code, _)) if r.crippled => vec![code],
        Some((code, condition)) => {
            debug!("combat: {name} {target}: {} condition {condition:.0}", limb_name(code));
            Vec::new()
        }
        None => blasted,
    };
    if !r.died && !r.knocked_out && crippled.iter().any(|c| matches!(*c, fnv_game::combat::LEFT_ARM | fnv_game::combat::RIGHT_ARM)) {
        feedback.disarms.push(target);
    }
    for &code in &crippled {
        let limb = limb_name(code);
        info!("combat: {name} {target}: {limb} crippled");
        if let Some(h) = hud.as_deref_mut().filter(|_| !r.died && mine) {
            h.show(format!("{name}: {limb} crippled"));
        }
    }
    let mut hit = None;
    if !r.immune {
        let torso_crippled = with_world(rt, sim, |rt| rt.game.is_crippled(target, fnv_game::combat::TORSO));
        if mine {
            feedback.target = Some((name, r.health as f32, r.max_health as f32, 0.0));
        }
        hit = Some(ActorHit { form_id: target.0, died: r.died, knocked_out: r.knocked_out, location: info.location, crippled: r.crippled, part_crippled: r.limb.is_some_and(|(_, c)| c <= 0.0), torso_crippled, by: info.attacker, share: if r.max_health > 0.0 { (r.dealt / r.max_health) as f32 } else { 0.0 }, guard_broken, weapon: info.weapon });
    }
    Some((r, hit))
}

/// A limb's name by its condition code (25–31).
fn limb_name(code: u16) -> &'static str {
    code.checked_sub(25).and_then(|i| LIMB_NAMES.get(i as usize)).copied().unwrap_or("limb")
}

/// An explosion's limb damage after its health damage `r` ([`hit_actor`]): none when the
/// hit killed, knocked out or did not touch the actor. Logs the damage; returns the limbs
/// it crippled.
fn blast_limbs(rt: &mut crate::script::Runtime, sim: &mut SimState, target: FormId, info: HitInfo, blast: &[LimbHit], r: &HitResult) -> Vec<u16> {
    if blast.is_empty() || r.immune || r.died || r.knocked_out || r.dealt <= 0.0 {
        return Vec::new();
    }
    let hurt = with_world(rt, sim, |rt| rt.game.blast_limbs(target, info.attacker, blast));
    let crippled: Vec<u16> = hurt.iter().filter(|h| h.2).map(|h| h.0).collect();
    if !hurt.is_empty() {
        let names: Vec<&str> = crippled.iter().map(|&c| limb_name(c)).collect();
        let each: Vec<String> = blast.iter().map(|l| format!("{} {:.2}", limb_name(l.condition), l.damage)).collect();
        info!("combat: blast on {target}: limb damage {}{}", each.join(", "), if names.is_empty() { String::new() } else { format!(", crippled {}", names.join(", ")) });
    }
    crippled
}

/// God mode: the player takes no damage from hits, so cannot be killed by them
/// (settings page `god-mode`, `--god`, test aid `NVF_PLAY_TEST_GOD=1`) [design]. One
/// switch for the process: hits reach the player from several systems.
static GOD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether god mode is on.
pub fn god_mode() -> bool {
    GOD.load(std::sync::atomic::Ordering::Relaxed)
}

/// Switch god mode on or off.
pub fn set_god_mode(on: bool) {
    GOD.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Another actor hits the player (module notes): the player's health takes the damage
/// through `Game::hit`, and at 0 the player dies.
#[allow(clippy::too_many_arguments)]
fn hit_player(rt: &mut crate::script::Runtime, sim: &mut SimState, dealt: f32, info: HitInfo, stats: &mut CombatStats, hud: Option<&mut HudMessage>, effects: &[FormId], feedback: &mut HitFeedback, blast: &[LimbHit]) -> Option<HitResult> {
    let by = info.attacker.map_or("?".to_string(), |a| sim.ref_name(Some(&rt.game), a.0).map_or_else(|| a.to_string(), str::to_string));
    stats.player_hits += 1;
    if god_mode() {
        info!("combat: {by} hits the player for {dealt:.1} (god mode: no damage taken)");
        // Still a hit: a crippled torso flinches (`player_flinch`).
        let torso_crippled = with_world(rt, sim, |rt| rt.game.is_crippled(FormId::PLAYER_REF, fnv_game::combat::TORSO));
        feedback.player_hits.push(PlayerHit { crippled: false, part_crippled: false, torso_crippled });
        let health = with_world(rt, sim, |rt| fnv_game::functions::actor_value(&mut rt.game, FormId::PLAYER_REF, fnv_game::actor_values::HEALTH).unwrap_or(0.0));
        return Some(HitResult { health, immune: true, ..HitResult::default() });
    }
    with_world(rt, sim, |rt| hit_effects::apply(&mut rt.game, &mut stats.hit_effects, FormId::PLAYER_REF, effects, info.attacker));
    let limb_before = info.limb.and_then(|l| with_world(rt, sim, |rt| fnv_game::functions::actor_value(&mut rt.game, FormId::PLAYER_REF, l.condition).ok()));
    let mut r = with_world(rt, sim, |rt| rt.game.hit(FormId::PLAYER_REF, dealt as f64, info));
    if let Some((before, (code, condition))) = limb_before.zip(r.limb).filter(|(before, (_, after))| before > after) {
        info!("combat: the player's {} takes {:.1}, condition {condition:.1}", limb_name(code), before - condition);
    }
    r.crippled |= !blast_limbs(rt, sim, FormId::PLAYER_REF, info, blast, &r).is_empty();
    stats.player_damage += r.dealt as f32;
    info!("combat: {by} hits the player for {:.1}, health {:.0}/{:.0}", r.dealt, r.health, r.max_health);
    if !r.immune && r.dealt > 0.0 {
        feedback.player_hurt.push((r.dealt as f32, r.max_health as f32));
    }
    if !r.died {
        let torso_crippled = with_world(rt, sim, |rt| rt.game.is_crippled(FormId::PLAYER_REF, fnv_game::combat::TORSO));
        feedback.player_hits.push(PlayerHit { crippled: r.crippled, part_crippled: r.limb.is_some_and(|(_, c)| c <= 0.0), torso_crippled });
    }
    if r.died {
        stats.player_died = true;
        info!("combat: the player died, killed by {by}");
        if let Some(h) = hud {
            h.show("You are dead.".to_string());
        }
    }
    Some(r)
}

/// Melee and unarmed attacks (see the module notes).
#[allow(clippy::too_many_arguments)]
fn melee(
    time: Res<Time>,
    mut c: ResMut<Combat>,
    mut sim: ResMut<SimState>,
    mut phys: ResMut<PhysState>,
    (player, intent): (Res<Player>, Res<MoveIntent>),
    control: Res<PlayerControl>,
    mut anim: ResMut<WeaponAnim>,
    mut stats: ResMut<CombatStats>,
    mut script: Option<ResMut<ScriptState>>,
    mut feedback: ResMut<HitFeedback>,
    mut hud: Option<ResMut<HudMessage>>,
    (mut messages, mut pushes, mut broken, mut forced): (MessageWriter<ActorHit>, MessageWriter<ActorPush>, MessageWriter<WeaponBroken>, MessageWriter<ForcedHitReaction>),
    (parts, limbs): (Res<ActorParts>, Res<PlayerLimbs>),
    (sfx, mut sounds, mut impacts): (Option<Res<Sfx>>, MessageWriter<PlaySound>, MessageWriter<ImpactFx>),
    (mut vats, mut dm, mut knocks): (Option<ResMut<crate::vats_mode::VatsMode>>, ResMut<crate::dismember::Dismemberment>, MessageWriter<Knockback>),
) {
    let dt = time.delta_secs();
    let c = &mut *c;
    // VATS mode's swing due now: one tap of Use (vats-mode.md §16.2).
    let vats_tap = vats.as_deref_mut().is_some_and(|v| v.take_swing());
    let Some(id) = c.weapon.filter(|_| c.melee) else { return };
    if !anim.ready {
        c.swings.reset();
        anim.swing = None;
        return;
    }
    let w = c.player_stats(id).expect("weapon present");
    // The clip the body timed for the swing playing; a new swing runs on the weapon's
    // own timing until the body has read its clip (the next frame).
    // The Modify Attack Speed perks (Slayer, Melee Hacker …) speed the swing as the
    // weapon's own multipliers do; the body plays the swing clip at the same rate.
    let (atk, by) = player_weapon_perk(script.as_deref_mut(), &mut sim, fnv_game::perks::MODIFY_ATTACK_SPEED, id, 1.0);
    let atk = if atk.is_finite() && atk > 0.0 { atk } else { 1.0 };
    let speed = w.attack_mult.max(0.01) * w.anim_mult.max(0.01) * atk;
    if (speed - anim.attack_rate).abs() > 1e-4 {
        if by.is_empty() {
            info!("combat: swings at x{speed:.2} ({})", w.edid.as_deref().unwrap_or("?"));
        } else {
            info!("combat: swings at x{speed:.2} ({}; perks x{atk:.2}: {})", w.edid.as_deref().unwrap_or("?"), by.join(", "));
        }
    }
    anim.attack_rate = speed;
    if let Some(stem) = anim.timed.as_deref() {
        let t = &anim.timing;
        c.swings.set_timing(stem, SwingTiming::of(t.attack_len, t.hit_time, t.next_attack, speed, 1.0));
    }
    let fallback = Timing::of(&w, &ClipTiming::default(), &c.tuning).interval;
    let first = fnv_actor::player_clips::attack_stem(anim.attack.as_deref());
    // Both legs crippled: no directional power attacks outside VATS mode (crippled-limbs.md
    // §4): the swing takes the standing one.
    let (forward, right) = if limbs.both_legs() { (0.0, 0.0) } else { (intent.forward, intent.right) };
    // The weapon's power attack override plays for the standing power attack.
    let input = MeleeInput { trigger: control.trigger || vats_tap, dt, forward, right, first: &first, rolls: [c.swing_rng.next_f32(), c.swing_rng.next_f32()], power_override: w.power_attack_override.as_deref() };
    let mut strike = None;
    for e in c.swings.tick(input, &c.tuning, SwingTiming::of(None, None, None, speed, fallback)) {
        match e {
            MeleeEvent::Started { stem, power, chained } => {
                // The unarmed special moves (perk-entry-points.md §11) are chosen as the
                // swing starts, so it plays the move's own clip: a power attack in a
                // direction whose move the player has; Scribe Counter, a plain blow begun
                // within COUNTER_WINDOW of the player's block, a standing power attack.
                let (mut stem, mut power) = (stem, power);
                let unarmed = Some(id) == c.fists || script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.data.weapon_skills.get(&id).copied() == Some(fnv_game::actor_values::UNARMED as i64));
                c.special_swing = None;
                if unarmed {
                    let countering = power.is_none() && c.player_blocked_at.is_some_and(|t| c.time - t <= COUNTER_WINDOW);
                    let special = match power {
                        Some(d) => special_move(script.as_deref_mut(), &mut sim, d),
                        None if countering => scribe_counter(script.as_deref_mut(), &mut sim),
                        None => None,
                    };
                    if let Some(m) = special {
                        if power.is_none() {
                            power = Some(fnv_combat::melee::PowerDir::Stand);
                            c.player_blocked_at = None;
                        }
                        info!("combat: {} ({:?} power attack, unarmed): clip {}", m.name, power.unwrap_or(fnv_combat::melee::PowerDir::Stand), m.stem);
                        if m.lunges {
                            // Its clip carries the player (`h2hattackcustom1power.kf`
                            // moves `Bip01` 332 units forward over its 1.6 s,
                            // perk-entry-points.md §11.4): the body's attack clips move the
                            // player by their root's travel, at the swing's speed
                            // (`crate::player::Lunge`, actors.md §8.13).
                            info!("combat: {}: the player lunges with its clip", m.name);
                        }
                        stem = m.stem.to_string();
                        if let Some(s) = c.swings.swing.as_mut() {
                            s.stem = stem.clone();
                            s.power = power;
                        }
                        c.special_swing = Some(m);
                    }
                }
                if power.is_some() {
                    stats.power_attacks += 1;
                    // Without a clip to time it, a power attack takes longer [design].
                    if let Some(s) = c.swings.swing.as_mut() {
                        s.timing = SwingTiming::of(None, None, None, speed, fallback * c.tuning.power_time);
                    }
                }
                stats.swings += 1;
                stats.chained_swings += u32::from(chained);
                // The player's PowerAttack or Attack line (docs/spec-notes/combat-dialogue.md).
                if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
                    with_world(rt, &mut sim, |rt| rt.game.attack_line(FormId::PLAYER_REF, None, power.is_some(), false));
                }
                info!("combat: {} swing {stem}{}", w.edid.as_deref().unwrap_or("?"), if chained { " (chained)" } else { "" });
                anim.swing = Some(stem);
                anim.attacks += 1;
            }
            MeleeEvent::Strike { power } => strike = Some(power),
            MeleeEvent::Ended => {}
        }
    }
    if let Some(power) = strike {
        // The special move chosen as the swing started (its clip is playing).
        let special = c.special_swing.take();
        let power_mult = power.map_or(1.0, |d| d.damage(&c.rules));
        // The reach is from the body; the rays start at the eye, the capsule's middle.
        let reach = damage::melee_reach(&c.rules, &w, 1.0) + phys.player.as_ref().map_or(0.0, |p| p.params.radius);
        let eye = player.eye();
        // A blow that meets no one but strikes the world within reach (a wall, the
        // ground ahead) shows its impact there.
        let mut struck_world: Option<WorldHit> = None;
        // VATS mode's swing lands on its mark when the mark is still close and the first
        // thing on the line from the eye to it (100 % accuracy, vats-mode.md §16.2).
        let vats_mark = vats.as_deref().and_then(|v| v.melee_mark());
        let hits = {
            let world = view(&mut phys, script.as_deref(), Some(&sim));
            let straight = vats_mark.and_then(|(target, point, close, lands)| {
                // The hit roll missed (vats-mode.md §17.4): the blow lands on no one.
                if !lands {
                    info!("combat: vats mode blow on {target:08X}: missed (the hit roll)");
                    return Some(Vec::new());
                }
                let flat = ((point[0] - eye[0]).powi(2) + (point[1] - eye[1]).powi(2)).sqrt();
                let d = vec::sub(point, eye);
                let end = vec::add(eye, vec::scale(vec::normalize(d), vec::len(d) + VATS_BLOW_PAST));
                let hit = (flat <= close).then(|| world.ray(eye, end, &[PLAYER])).flatten().filter(|h| h.actor == Some(target as ActorId));
                info!(
                    "combat: vats mode blow on {target:08X} ({}): {:.0} units across, {}",
                    hit.as_ref().and_then(|h| h.part).map_or("whole".to_string(), |p| format!("part {p}")),
                    flat,
                    if hit.is_some() { "struck" } else { "not in reach or not first on the line: an ordinary swing" }
                );
                hit.map(|h| vec![(target as ActorId, h)])
            });
            if let Some(hits) = straight {
                hits
            } else {
                // Low and high: a short creature at the player's feet is struck within the
                // reach across the ground (`swing_reaching`).
                let hits = fnv_combat::projectile::swing_reaching(&world, &c.tuning, eye, player.feet[2], player.look_dir(), reach, PLAYER);
                let fan = fnv_combat::projectile::swing(&world, &c.tuning, eye, player.look_dir(), [0.0, 0.0, 1.0], reach, PLAYER).len();
                if hits.len() > fan {
                    info!("combat: swing reaches {} actors (reach {reach:.0}), {} of them out of the fan of rays at eye height", hits.len(), hits.len() - fan);
                }
                if hits.is_empty() {
                    let ahead = world.ray(eye, vec::add(eye, vec::scale(player.look_dir(), 400.0)), &[]);
                    struck_world = ahead.clone().filter(|h| h.actor.is_none() && h.distance <= reach);
                    info!(
                        "combat: swing misses (reach {reach:.0}); straight ahead: {}",
                        ahead.map_or("nothing within 400".to_string(), |h| format!("{} at {:.0}", h.actor.map_or("the world".to_string(), |a| format!("actor {a:08X}")), h.distance))
                    );
                }
                hits
            }
        };
        // A blow that lands wears the weapon, once however many it strikes (§17); the
        // blow itself is dealt at the condition the weapon had.
        let weapon_condition = item_condition(&sim, FormId::PLAYER_REF, id);
        // The blow's impacts, as a shot's (the owner's playtest list 5 item 24): blood on
        // the actors struck, dust or sparks on the world, with their sounds.
        if let Some(s) = sfx.as_deref() {
            let dir = player.look_dir();
            let landed = hits.iter().map(|(a, h)| (Some(*a), h)).chain(struck_world.iter().map(|h| (None, h)));
            for (actor, h) in landed {
                if h.part == Some(WEAPON_PART) {
                    continue;
                }
                let (impact, material) = impact_for(s, script.as_deref(), &sim, id, actor, h.material);
                match impact {
                    Some(impact) => {
                        stats.melee_impacts += 1;
                        debug!("combat: melee impact {impact} ({material}) at ({:.0}, {:.0}, {:.0})", h.point[0], h.point[1], h.point[2]);
                        show_impact(s, script.as_deref(), &sim, ImpactFx { impact, shooter: FormId::PLAYER_REF, point: h.point, normal: h.normal, dir, actor: actor.map(|a| a as u32), no_spray: false, no_decal: false }, &mut sounds, &mut impacts);
                    }
                    None => debug!("combat: {} has no impact for {material}", w.edid.as_deref().unwrap_or("?")),
                }
            }
        }
        if !hits.is_empty() {
            wear_weapon(&c.forms, &mut sim, &mut stats, hud.as_deref_mut(), None, script.as_deref_mut(), FormId::PLAYER_REF, id, condition::weapon_wear(&c.rules, &w, &[]), None);
        }
        for (actor, hit) in hits {
            let target = FormId(actor as u32);
            stats.melee_hits += 1;
            let (dt_v, dr) = target_armor(c, script.as_deref_mut(), &mut sim, target);
            let mut a = attacker(script.as_deref_mut(), &mut sim, &c.rules, FormId::PLAYER_REF, &w, vats_mark.is_some());
            a.sneaking = sneak_attack(script.as_deref(), FormId::PLAYER_REF, control.sneak, target);
            let bp = body_part(c, &parts, actor, hit.part);
            let part = bp.as_ref().map_or_else(HitPart::default, BodyPart::hit_part);
            let h = HitInput { weapon: &w, projectiles: 1, condition: weapon_condition, ammo_effects: &[], part, power: power_mult };
            let roll = c.rng.next_f32();
            let (ap, tp) = hit_perks(script.as_deref_mut(), &mut sim, FormId::PLAYER_REF, id, Some(target));
            if hit.part == Some(WEAPON_PART) {
                let d = damage::hit(&c.rules, &c.tuning, &h, &a, &Target::default(), &ap, &NoPerks, roll);
                strike_weapon(&c.forms, &mut sim, &mut stats, hud.as_deref_mut(), &mut broken, script.as_deref_mut(), &parts, target, d.raw, hit.point);
                continue;
            }
            let resist = target_resist(script.as_deref_mut(), &mut sim, target, &w);
            let nerve = companion_nerve(script.as_deref_mut(), &mut sim, target);
            let mut d = damage::hit(&c.rules, &c.tuning, &h, &a, &Target { dt: dt_v, dr, resist, nerve, ..Target::default() }, &ap, &tp, roll);
            log_hit_perks(FormId::PLAYER_REF, Some(target), &ap, &tp, &d);
            // A person blocking it from the front stops part of it (combat-ai.md §14.3); a
            // power attack breaks through and staggers it (combat-rules.md §12).
            let mut guard_broken = false;
            if c.guarding.get(&target).is_some_and(|f| damage::from_front(*f, player.look_dir())) {
                let before = d.dealt;
                (d.dealt, guard_broken) = damage::against_block(&c.tuning, before, power.is_some());
                if guard_broken {
                    stats.blocks_broken += 1;
                    info!("combat: the player's power attack breaks {target}'s block ({before:.1}, all dealt)");
                } else {
                    stats.blows_blocked += 1;
                    info!("combat: {target} blocks the player's blow: {:.1} of {before:.1} stopped", before - d.dealt);
                    c.blocked_blows.push(BlockedBlow { attacker: FormId::PLAYER_REF, blocker: target, damage: before, dealt: d.dealt });
                    if let Some(b) = block_sound(c, &sim, sfx.as_deref(), target) {
                        sounds.write(b);
                    }
                }
            }
            let agility = actor_av(script.as_deref_mut(), &mut sim, target, fnv_game::actor_values::AGILITY).unwrap_or(5.0);
            let push = damage::knockback_force(&c.rules, d.dealt, agility);
            let location = bp.as_ref().map_or(0, BodyPart::location);
            let limb = limb_hit(bp.as_ref(), &d, &w);
            let info = HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: Some(id), location, critical: d.critical, limb };
            pushes.write(ActorPush { form_id: target.0, point: hit.point, dv: vec::scale(player.look_dir(), MELEE_PUSH * if power.is_some() { POWER_PUSH } else { 1.0 }) });
            let mut items = hit_items(c, script.as_deref_mut(), &mut sim, &mut stats, FormId::PLAYER_REF, Some(id), None);
            // A special move's effect goes on with the blow (Ranger Takedown's knockdown
            // spell, Khan Trick's head crippling: the DOBJ's).
            if let Some(spell) = special.and_then(|m| m.spell) {
                items.push(spell);
            }
            let r = apply_hit(script.as_deref_mut(), &mut sim, target, d.dealt, info, &mut stats, &mut feedback, hud.as_deref_mut(), &mut messages, &items, guard_broken, crit_effect(c, Some(id)));
            armour_lines(script.as_deref_mut(), Some(FormId::PLAYER_REF), target, &d, c.tuning.dt_floor);
            spread_hit_areas(script.as_deref_mut(), &mut sim, &mut stats, target, &items, info.attacker);
            if r.as_ref().is_some_and(|r| !r.died && !r.immune && !r.knocked_out) {
                if let Some(m) = special.filter(|m| m.forces_reaction) {
                    info!("combat: {}: {target} is forced into its hit reaction (location {location})", m.name);
                    forced.write(ForcedHitReaction { form_id: target.0, location, by: m.name });
                }
            }
            // A living actor struck is pushed back by the knockback (module notes).
            if r.as_ref().is_some_and(|r| !r.died && !r.immune && !r.knocked_out) {
                if let Some(k) = Knockback::of(target.0, player.look_dir(), push, c.rules.knockback_time) {
                    knocks.write(k.by(FormId::PLAYER_REF, c.next_blow(), power.is_some()));
                }
            }
            if r.as_ref().is_some_and(|r| !r.died && !r.immune) {
                // Legion Assault knocks back (no DOBJ spell); the Knockdown Chance perks
                // (Super Slam, Elijah's Last Words) may knock the target down.
                let away = player.look_dir();
                if special.is_some_and(|m| m.knock_back) {
                    knock_down(script.as_deref_mut(), FormId::PLAYER_REF, target, away, "Legion Assault");
                } else {
                    perk_knockdown(c, script.as_deref_mut(), &mut sim, FormId::PLAYER_REF, id, target, away);
                }
            }
            if let Some(check) = r.as_ref().and_then(|r| dismember_check(target, hit.part, &w, r, Some(FormId::PLAYER_REF), hit.point, player.look_dir())) {
                dm.checks.push(check);
            }
            if r.as_ref().is_none_or(|r| !r.immune) {
                wear_armor(c, &mut sim, &mut stats, target, Zone::of(bp.as_ref()), d.raw);
            }
            match r {
                Some(r) => info!(
                    "combat: {}{} hits {target} on {} for {:.1} (DT {:.0}{}), health {:.0}/{:.0}, knockback {push:.0}",
                    w.edid.as_deref().unwrap_or("?"),
                    if power.is_some() { " power attack" } else { "" },
                    bp.as_ref().and_then(|b| b.name.as_deref()).unwrap_or("the body"),
                    r.dealt,
                    d.dt,
                    crit_note(&a, &d),
                    r.health,
                    r.max_health
                ),
                None => info!("combat: melee hit {target} for {:.1} (no script runtime: not applied)", d.dealt),
            }
        }
    }
}

/// A VATS mode blow's line goes this far past its mark's point, units [design].
const VATS_BLOW_PAST: f32 = 32.0;

/// Explosions set off per frame at most (chains of destructibles) [design].
const MAX_EXPLOSIONS_PER_FRAME: u32 = 64;
/// How far above and below an explosion the ground is looked for to show the impact of
/// its data set there, game units [design].
const EXPLOSION_GROUND_UP: f32 = 32.0;
const EXPLOSION_GROUND_REACH: f32 = 160.0;
/// The game's own water explosion: `DefaultWaterExplosion` ("Water Explosion",
/// `Effects\UnderWaterExplosion.NIF`, the sound `CWaterExplosionSplash`, no damage)
/// [observed: FalloutNV.esm 000001F5]. Its sound plays where an explosion throws up
/// water; its model (bubbles and a glowing shock wave) is not the surface's
/// ([`WATER_SURFACE_EXPLOSION`]).
const WATER_EXPLOSION: FormId = FormId(0x0000_01F5);
/// How far from the surface, over or under it, an explosion throws up water, units:
/// `fExplosionSplashRadius` (250 in FalloutNV.esm) [hypothesis from its name; the
/// recording's first grenade went off 186 units under the surface and threw it up].
const WATER_BLAST_REACH: f32 = 250.0;

/// The surface an explosion at `at` throws water up from: the water it is in or over,
/// when it is within `reach` of the surface and the ground under it (its height, if
/// found) is not above the surface.
fn water_thrown_up(at: V3, surface: Option<f32>, ground: Option<f32>, reach: f32) -> Option<f32> {
    surface.filter(|&s| (at[2] - s).abs() <= reach && ground.is_none_or(|g| g < s))
}

/// The block sound of a blocked blow: the blocker's weapon's `NAM6`, at the blocker (at
/// the listener for the player) [hypothesis: whose weapon; docs/spec-notes/audio.md §15.2].
fn block_sound(c: &Combat, sim: &SimState, sfx: Option<&Sfx>, blocker: FormId) -> Option<PlaySound> {
    let sfx = sfx?;
    let weapon = if blocker == FormId::PLAYER_REF {
        c.weapon?
    } else {
        let inventory = sim.world.peek(blocker)?.inventory.as_ref()?;
        inventory.equipped().into_iter().find(|w| sfx.forms.weapons.contains_key(w))?
    };
    let sound = sfx.forms.weapons.get(&weapon)?.block?;
    debug!("combat: {blocker} blocks: {}", sound_edid(Some(sfx), sound));
    Some(PlaySound { sound, at: if blocker == FormId::PLAYER_REF { None } else { Some(ref_pos(sim, blocker)?) } })
}

/// Where a reference stands (its simulation state, else its placement).
fn ref_pos(sim: &SimState, r: FormId) -> Option<V3> {
    sim.world.peek(r).map(|s| s.pos).or_else(|| sim.refs.get(r).map(|p| p.pos))
}

/// Queue what a destruction change sets off and report the rest (step C7).
fn queue_destruction(c: &mut Combat, sim: &SimState, r: FormId, ch: &DestructionChange, cause: Option<Cause>) {
    if ch.after == ch.before {
        return;
    }
    let at = ref_pos(sim, r).map(|p| [p[0], p[1], p[2] + DESTRUCTION_BLAST_HEIGHT]);
    info!(
        "combat: {r} destruction stage {} -> {}{}{}{}",
        ch.before,
        ch.after,
        if ch.destroyed { ", destroyed" } else { "" },
        if ch.disabled { ", disabled" } else { "" },
        if ch.explosions.is_empty() { String::new() } else { format!(", {} explosion(s)", ch.explosions.len()) }
    );
    if let Some(at) = at {
        c.boom.extend(ch.explosions.iter().map(|&e| (e, at, cause)));
        // Its stages' debris, thrown by the engine (nvforever `dismember.rs`).
        c.debris.extend(ch.debris.iter().map(|&(d, n)| ObjectDebris { reference: r, at, debris: d, count: n }));
    }
}

/// Object damage to a destructible reference through the script runtime; `cause` set off
/// the stages' explosions.
fn damage_destructible(c: &mut Combat, sim: &mut SimState, script: Option<&mut ScriptState>, r: FormId, amount: f32, cause: Option<Cause>) {
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return };
    let ch = with_world(rt, sim, |rt| rt.game.damage_object(r, amount as f64));
    if let Some(ch) = ch {
        queue_destruction(c, sim, r, &ch, cause);
    }
}

/// Destructible references whose collision overlaps a sphere, with their distance.
fn destructibles_near(phys: &mut PhysState, sim: &SimState, script: Option<&ScriptState>, at: V3, radius: f32) -> Vec<(FormId, f32)> {
    let Some(rt) = script.and_then(|s| s.rt.as_ref()) else { return Vec::new() };
    phys.world.update_queries();
    let shape = fnv_collide::Shape::Sphere { radius };
    let pose = fnv_collide::Iso { translation: at, ..fnv_collide::Iso::IDENTITY };
    let owners = phys.world.queries().overlap(&shape, &pose, &Filter::default()).unwrap_or_default();
    let mut out: Vec<(FormId, f32)> = Vec::new();
    for o in owners {
        let Owner::Ref { form_id, .. } = o else { continue };
        let r = FormId(form_id);
        if out.iter().any(|x| x.0 == r) || !rt.game.is_destructible(r) {
            continue;
        }
        let d = ref_pos(sim, r).map_or(radius, |p| vec::len(vec::sub(p, at)).min(radius));
        out.push((r, d));
    }
    out
}

/// What a shot sends out: sounds, impact effects, explosions, broken weapons, and what
/// each impact picked (for `crate::world_sounds`).
type ShotEffects<'w> = (MessageWriter<'w, PlaySound>, MessageWriter<'w, ImpactFx>, MessageWriter<'w, ExplosionFx>, MessageWriter<'w, WeaponBroken>, MessageWriter<'w, crate::world_sounds::ImpactPicked>);

/// Fly the projectiles, deliver hits when due, set off explosions (and what they set
/// off: destruction stages, detonations).
/// What says whether the player blocks: its controls, its weapon's state, where it looks.
type Guard<'w> = (Option<Res<'w, PlayerControl>>, Option<Res<'w, WeaponAnim>>, Option<Res<'w, Player>>);

/// Other actors' attacks (module notes): their shots go into flight as the player's do,
/// their blows are dealt at once.
#[allow(clippy::too_many_arguments)]
fn actor_attacks(
    mut c: ResMut<Combat>,
    mut sim: ResMut<SimState>,
    mut phys: ResMut<PhysState>,
    mut stats: ResMut<CombatStats>,
    mut script: Option<ResMut<ScriptState>>,
    mut feedback: ResMut<HitFeedback>,
    mut hud: Option<ResMut<HudMessage>>,
    (mut shots, mut blows): (MessageReader<ActorFire>, MessageReader<ActorStrike>),
    (mut fired, mut sounds, mut messages, mut pushes): (MessageWriter<ShotFired>, MessageWriter<PlaySound>, MessageWriter<ActorHit>, MessageWriter<ActorPush>),
    sfx: Option<Res<Sfx>>,
    (mut impacts, mut broken, mut knocks): (MessageWriter<ImpactFx>, MessageWriter<WeaponBroken>, MessageWriter<Knockback>),
    (control, anim, player): Guard,
    mut dm: ResMut<crate::dismember::Dismemberment>,
    mut grenade_probe: ResMut<held_grenade_probe::Probe>,
    time: Res<Time>,
) {
    let c = &mut *c;
    let gravity = phys.world.params.gravity;
    // The player blocks with Block held and fists or a melee weapon drawn (combat.md §5.4);
    // NPC blows from its front are then blocked as an NPC's are (combat-ai.md §14.3).
    let player_guard = (c.melee && control.as_deref().is_some_and(|k| k.aiming) && anim.as_deref().is_some_and(|a| a.ready)).then(|| player.as_deref().map(|p| p.look_dir())).flatten().map(|d| [d[0], d[1]]);
    let script_shots = std::mem::take(&mut c.script_shots);
    for (s, scripted) in shots.read().map(|s| (s, false)).chain(script_shots.iter().map(|s| (s, true))) {
        let probe_request = grenade_probe.request(s, &sim.world);
        // The shooter's own body is never hit: the player's is `PLAYER`, not its FormID (a
        // player's FireWeapon hit the player at the muzzle, 2026-10-27).
        let own = if s.shooter == FormId::PLAYER_REF { PLAYER } else { s.shooter.0 as ActorId };
        let carried = |a: FormId| sim.world.peek(s.shooter).and_then(|st| st.inventory.as_ref()).is_some_and(|i| i.count(a) > 0);
        let ammo = c.forms.ammo_for(s.weapon, carried);
        let Some(chain) = c.forms.chain(s.weapon, ammo) else { grenade_probe.finish_request(probe_request, 0); continue };
        let Some(proj) = chain.projectile else { grenade_probe.finish_request(probe_request, 0); continue };
        stats.actor_shots += 1;
        let range = c.forms.projectiles.get(&proj).map_or(0.0, |p| p.range);
        let reach = if range > 0.0 { range } else { AIM_REACH };
        let leaving = {
            let world = view(&mut phys, script.as_deref(), Some(&sim));
            shot::leaving(&world, s.origin, s.aim, s.muzzle, reach, Some(own), false)
        };
        let seen_from = leaving.seen.at(leaving.origin, 0.0);
        fired.write(ShotFired { projectile: proj, weapon: s.weapon, shooter: s.shooter, at: Some(seen_from), dir: Some(leaving.aim) });
        // Heard where it is fired: the weapon's 3D shoot sound, its distant one beyond
        // the 3D sound's reach, the mod set's with a Silence or Suppressor mod on the
        // unit in use (weapon-mods.md §7).
        if let Some(fx) = sfx.as_deref().and_then(|x| x.forms.weapons.get(&s.weapon)) {
            let mods = sim.world.peek(s.shooter).and_then(|st| st.inventory.as_ref()).map_or(0, |i| i.mods_in_use(s.weapon));
            let silenced = mods != 0 && c.forms.weapons.get(&s.weapon).is_some_and(|w| w.sound_mod_on(mods));
            let shoot = fx.shoot_sounds(silenced);
            let near_max = shoot.near.and_then(|n| sfx.as_deref().and_then(|x| x.forms.sounds.get(&n))).map_or(0.0, |n| n.max_distance);
            let distance = player.as_deref().map_or(0.0, |p| vec::len(vec::sub(p.eye(), seen_from)));
            let (sound, far) = shoot.heard_at(distance, near_max);
            if let Some(sound) = sound {
                debug!("combat: npc {} shot sound {} ({}, {})", s.shooter, sound_edid(sfx.as_deref(), sound), if shoot.silenced { "silenced" } else { "plain" }, if far { "distant" } else { "3D" });
                sounds.write(PlaySound { sound, at: Some(seen_from) });
            }
        }
        let (tracer_chance, force) = c.forms.projectiles.get(&proj).map_or((0.0, 0.0), |p| (p.tracer_chance, p.impact_force));
        let by = By { actor: s.shooter, weapon: s.weapon, ammo, harmless: s.harmless, vats: false, sneaking: false };
        let rng = &mut c.rng;
        let mut rand = || rng.next_f32();
        let mut ps = shot::fire(&c.forms, &chain, leaving.origin, leaving.aim, s.cone, Some(own), gravity, &c.tuning, &mut rand);
        // The mods on the unit it uses: Increase Projectile Speed (weapon-data-fields.md §5).
        let mods = sim.world.peek(s.shooter).and_then(|st| st.inventory.as_ref()).map_or(0, |i| i.mods_in_use(s.weapon));
        if let Some(m) = c.forms.weapons.get(&s.weapon).filter(|_| mods != 0).and_then(|w| w.modded(mods).projectile_speed_mult) {
            ps.iter_mut().for_each(|p| p.speed_up(m));
        }
        let probe_created = ps.len();
        for p in ps {
            let rolled = c.rng.next_f32() < tracer_chance || c.tracer_hold.is_some();
            let shown = rolled || DRAW_EVERY_PROJECTILE;
            let tracer = rolled || (shown && instant(&p));
            let dir = vec::normalize(p.vel);
            c.next_visual += 1;
            c.flying.push(Flying { p, id: c.next_visual, proj, shown, dir, from_muzzle: s.muzzle.is_some(), origin: leaving.origin, seen: leaving.seen, tracer, projectiles_per_shot: chain.projectiles_per_shot, force, by, rref: None, registered: false, speed_mult: 1.0, ended_at: None });
            if let Some(flying) = c.flying.last() {
                grenade_probe.accepted(probe_request, s, &flying.p, flying.proj, flying.id);
            }
        }
        grenade_probe.finish_request(probe_request, probe_created);
        let wname = c.forms.weapons.get(&s.weapon).and_then(|w| w.edid.as_deref()).unwrap_or("?");
        if scripted {
            // A script's FireWeapon (a rigged shotgun, a turret): worth a line.
            info!("combat: FireWeapon: {} fires {wname} from ({:.0}, {:.0}, {:.0})", s.shooter, leaving.origin[0], leaving.origin[1], leaving.origin[2]);
        } else {
            debug!("combat: {} fires {wname} (cone {:.1}°)", s.shooter, s.cone);
        }
        // The shot wears the shooter's weapon as the player's (§17); a scripted
        // FireWeapon does not, it is the reference's, not a carried gun's [design].
        if !scripted {
            if let Some(w) = c.forms.weapons.get(&s.weapon) {
                let effects: Vec<&AmmoEffect> = chain.ammo_effects.iter().filter_map(|e| c.forms.ammo_effects.get(e)).collect();
                let points = condition::weapon_wear(&c.rules, w, &effects);
                wear_actor_weapon(&c.forms, &mut sim, &mut stats, &mut broken, script.as_deref_mut(), s.shooter, s.weapon, points, seen_from);
            }
        }
    }
    grenade_probe.receiver_done(time.elapsed_secs());
    for b in blows.read() {
        // The weapon: its own, else the Fists; a creature's own attack damage in the Fists'
        // place [design: the Fists' other figures stand in for a creature's].
        let Some(id) = b.weapon.or(c.fists) else { continue };
        let Some(mut w) = c.forms.weapons.get(&id).cloned() else { continue };
        if let Some(d) = b.creature_damage {
            w.base_damage = d.round() as i32;
        }
        stats.actor_strikes += 1;
        let a = attacker(script.as_deref_mut(), &mut sim, &c.rules, b.attacker, &w, false);
        // A person's power attack: × its direction's fDamagePowerAttack…Bonus, as the
        // player's (combat-rules.md §10.1); a creature's deals its usual damage.
        let power = b.anim.filter(|_| b.creature_damage.is_none()).and_then(fnv_combat::melee::PowerDir::of_group).map_or(1.0, |d| d.damage(&c.rules));
        let h = HitInput { weapon: &w, projectiles: 1, condition: item_condition(&sim, b.attacker, id), ammo_effects: &[], part: HitPart::default(), power };
        if power != 1.0 {
            info!("combat: {}'s power attack {} on {}: damage x{power:.1}", b.attacker, b.anim.unwrap_or("?"), b.target);
        }
        let (dt_v, dr) = target_armor(c, script.as_deref_mut(), &mut sim, b.target);
        let roll = c.rng.next_f32();
        let (ap, tp) = hit_perks(script.as_deref_mut(), &mut sim, b.attacker, id, Some(b.target));
        let resist = target_resist(script.as_deref_mut(), &mut sim, b.target, &w);
        let nerve = companion_nerve(script.as_deref_mut(), &mut sim, b.target);
        let mut d = damage::hit(&c.rules, &c.tuning, &h, &a, &Target { dt: dt_v, dr, resist, nerve, ..Target::default() }, &ap, &tp, roll);
        log_hit_perks(b.attacker, Some(b.target), &ap, &tp, &d);
        // A blocker facing the blow stops part of it (docs/design/combat-ai.md §14.3); a
        // power attack (a creature's, `ActorStrike::anim`) breaks through (combat-rules.md
        // §12): an NPC staggers, the player cannot block for a while.
        let player_guard = player_guard.filter(|_| c.time >= c.guard_down_until);
        let guard = if b.target == FormId::PLAYER_REF { player_guard } else { c.guarding.get(&b.target).copied() };
        let mut guard_broken = false;
        if guard.is_some_and(|f| damage::from_front(f, b.dir)) {
            let before = d.dealt;
            (d.dealt, guard_broken) = damage::against_block(&c.tuning, before, b.anim.is_some());
            if guard_broken {
                stats.blocks_broken += 1;
                info!("combat: {}'s power attack breaks {}'s block ({before:.1}, all dealt)", b.attacker, b.target);
                if b.target == FormId::PLAYER_REF {
                    c.guard_down_until = c.time + c.tuning.guard_break;
                }
            } else {
                stats.blows_blocked += 1;
                info!("combat: {} blocks {}'s blow: {:.1} of {before:.1} stopped", b.target, b.attacker, before - d.dealt);
                if b.target == FormId::PLAYER_REF {
                    c.player_blocked_at = Some(c.time);
                }
                c.blocked_blows.push(BlockedBlow { attacker: b.attacker, blocker: b.target, damage: before, dealt: d.dealt });
                if let Some(sound) = block_sound(c, &sim, sfx.as_deref(), b.target) {
                    sounds.write(sound);
                }
            }
        }
        // [hypothesis] Melee's part multipliers are unmeasured (combat-rules.md §12):
        // keep Health on HitPart::default(), but give player blows a location and limb.
        // A creature uses this path's Fists Limb Dmg Mult in the absence of its own weapon.
        let bp = (b.target == FormId::PLAYER_REF).then(|| player_body_part(c, &phys, player.as_deref(), b.point)).flatten();
        let info = HitInfo { attacker: Some(b.attacker), weapon: b.weapon, location: bp.as_ref().map_or(0, BodyPart::location), critical: d.critical, limb: limb_hit(bp.as_ref(), &d, &w) };
        if b.target != FormId::PLAYER_REF {
            pushes.write(ActorPush { form_id: b.target.0, point: b.point, dv: vec::scale(b.dir, MELEE_PUSH) });
        }
        if let Some(sound) = sfx.as_deref().and_then(|x| b.weapon.and_then(|w| x.forms.weapons.get(&w))).and_then(|f| f.shoot_3d.or(f.shoot_2d)) {
            sounds.write(PlaySound { sound, at: Some(b.point) });
        }
        // Its impact on the one struck, as the player's blows (item 24); a creature's own
        // attack by its CREA CNAM impact data set (docs/spec-notes/actor-fields.md §2).
        if let Some(s) = sfx.as_deref() {
            // Both from the Model/Animation source [hypothesis: CREA CNAM is in that group
            // as NAM4 is; docs/spec-notes/actors.md §1.3].
            let own = b.creature_damage.and(model_base(script.as_deref(), &sim, b.attacker.0)).and_then(|base| s.forms.creature_impact_set(base));
            let picked = match own {
                Some(set) => {
                    let material = actor_material(s, script.as_deref(), &sim, b.target.0);
                    (s.forms.impact_id(set, material), material)
                }
                None => impact_for(s, script.as_deref(), &sim, id, Some(b.target.0 as ActorId), None),
            };
            if let (Some(impact), _) = picked {
                stats.melee_impacts += 1;
                show_impact(s, script.as_deref(), &sim, ImpactFx { impact, shooter: b.attacker, point: b.point, normal: vec::scale(b.dir, -1.0), dir: b.dir, actor: Some(b.target.0), no_spray: false, no_decal: false }, &mut sounds, &mut impacts);
            }
        }
        // A blow that lands wears the weapon once, as the player's (§17); the Fists and
        // a creature's own attack are no item and do not wear.
        if let (Some(weapon), true) = (b.weapon, b.creature_damage.is_none()) {
            let points = condition::weapon_wear(&c.rules, &w, &[]);
            wear_actor_weapon(&c.forms, &mut sim, &mut stats, &mut broken, script.as_deref_mut(), b.attacker, weapon, points, b.point);
        }
        let items = hit_items(c, script.as_deref_mut(), &mut sim, &mut stats, b.attacker, b.weapon, b.anim);
        let r = apply_hit(script.as_deref_mut(), &mut sim, b.target, d.dealt, info, &mut stats, &mut feedback, hud.as_deref_mut(), &mut messages, &items, guard_broken, crit_effect(c, b.weapon));
        armour_lines(script.as_deref_mut(), Some(b.attacker), b.target, &d, c.tuning.dt_floor);
        // A killing blow (or one on a corpse) may dismember the part it lands on, by the
        // weapon's On Hit, as the player's blows (dismemberment.md §4, §7); the part is the
        // one a short line along the blow meets at its landing point.
        if r.as_ref().is_some_and(|r| r.died || r.corpse) && b.target != FormId::PLAYER_REF {
            let from = vec::sub(b.point, vec::scale(b.dir, BLOW_PROBE));
            let to = vec::add(b.point, vec::scale(b.dir, BLOW_PROBE));
            let facing = facing_part(&view(&mut phys, script.as_deref(), Some(&sim)), b.target.0 as ActorId, from, to);
            if let Some(check) = r.as_ref().zip(facing).and_then(|(rr, (part, point))| dismember_check(b.target, Some(part), &w, rr, Some(b.attacker), point, b.dir)) {
                dm.checks.push(check);
            }
        }
        spread_hit_areas(script.as_deref_mut(), &mut sim, &mut stats, b.target, &items, info.attacker);
        if r.as_ref().is_none_or(|r| !r.immune) {
            wear_armor(c, &mut sim, &mut stats, b.target, Zone::Body, d.raw);
        }
        // A living actor struck (the player too) is pushed back by the knockback; not the
        // player in god mode (`tgm`), which captures and smoke tests keep in place.
        if r.as_ref().is_some_and(|r| !r.died && !r.immune && !r.knocked_out) {
            let agility = actor_av(script.as_deref_mut(), &mut sim, b.target, fnv_game::actor_values::AGILITY).unwrap_or(5.0);
            let force = damage::knockback_force(&c.rules, d.dealt, agility);
            if let Some(k) = Knockback::of(b.target.0, b.dir, force, c.rules.knockback_time) {
                knocks.write(k.by(b.attacker, c.next_blow(), b.anim.is_some()));
            }
        }
        if b.target != FormId::PLAYER_REF {
            if let Some(r) = r {
                info!("combat: {} strikes {} with {} for {:.1}, health {:.0}/{:.0}", b.attacker, b.target, w.edid.as_deref().unwrap_or("?"), r.dealt, r.health, r.max_health);
            }
        }
    }
}

/// The shot `FireWeapon` asks of `from` (`fnv_game::functions` fighting): from the
/// reference's centre straight ahead along its rotation, with no spread [documented:
/// GECK wiki FireWeapon, "from the center of the reference … straight out"]. The centre
/// is the reference's origin, an actor's raised to its middle (half of 128 units)
/// [hypothesis]; ahead is its heading (z, clockwise from north) tilted by its x angle,
/// a positive x looking down [hypothesis: the game turns both clockwise]. The model's
/// projectile node is not used yet.
fn script_shot(sim: &SimState, from: FormId, weapon: FormId) -> Option<ActorFire> {
    // A reference nothing has moved yet (a rigged shotgun) stands where it was placed.
    let (pos, rot) = match sim.world.peek(from) {
        Some(s) => (s.pos, s.rot),
        None => sim.world.placed.get(from).map(|p| (p.pos, p.rot))?,
    };
    let actor = from == FormId::PLAYER_REF || sim.is_actor(from.0);
    let [x, _, z] = rot;
    let aim = [z.sin() * x.cos(), z.cos() * x.cos(), -x.sin()];
    let origin = [pos[0], pos[1], pos[2] + if actor { 64.0 } else { 0.0 }];
    Some(ActorFire { shooter: from, weapon, origin, aim, cone: 0.0, muzzle: None, harmless: false })
}

#[allow(clippy::too_many_arguments)]
fn fly(
    time: Res<Time>,
    mut c: ResMut<Combat>,
    mut sim: ResMut<SimState>,
    mut phys: ResMut<PhysState>,
    mut stats: ResMut<CombatStats>,
    mut script: Option<ResMut<ScriptState>>,
    mut feedback: ResMut<HitFeedback>,
    mut hud: Option<ResMut<HudMessage>>,
    mut messages: MessageWriter<ActorHit>,
    mut pushes: MessageWriter<ActorPush>,
    (parts, player): (Res<ActorParts>, Option<Res<Player>>),
    mut visuals: ResMut<ShotVisuals>,
    anim: Res<WeaponAnim>,
    sfx: Option<Res<Sfx>>,
    mut effects: ShotEffects,
    mut dm: ResMut<crate::dismember::Dismemberment>,
) {
    let dt = time.delta_secs();
    let c = &mut *c;
    // What scripts asked (DetonatePlacedExplosives, a placed explosion, DamageObject's
    // stage changes) and what burns down (self-damage).
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        let (requests, burning) = with_world(rt, &mut sim, |rt| {
            let mut changes = std::mem::take(&mut rt.game.destruction_changes);
            changes.extend(rt.game.destruction_tick(dt as f64));
            (std::mem::take(&mut rt.game.combat_requests), changes)
        });
        for req in requests {
            let actor = match req {
                CombatRequest::Detonate(actor) => actor,
                CombatRequest::Fire { from, weapon } => {
                    match script_shot(&sim, from, weapon) {
                        Some(shot) => c.script_shots.push(shot),
                        None => debug!("combat: FireWeapon on {from}: no such reference in the world"),
                    }
                    continue;
                }
                CombatRequest::Explode { explosion, at } => {
                    c.boom.push((explosion, at, None));
                    continue;
                }
                CombatRequest::Dismember { actor, location, by } => {
                    match crate::dismember::scripted_check(c, &parts, &sim, actor, location, by) {
                        Some(check) => dm.checks.push(check),
                        None => info!("dismember: KillActor on {actor}: no part at location {location} can go (or its body is not drawn)"),
                    }
                    continue;
                }
            };
            let shooter = if actor == FormId::PLAYER_REF { Some(PLAYER) } else { Some(actor.0 as ActorId) };
            let mut n = 0;
            for f in c.flying.iter_mut().filter(|f| f.p.shooter == shooter) {
                if let Some(ProjEvent::Explode { explosion, point }) = f.p.detonate() {
                    c.boom.push((explosion, point, Some(Cause::of(&f.by))));
                    n += 1;
                }
            }
            c.flying.retain(|f| f.p.alive);
            info!("combat: {actor} detonates placed explosives: {n}");
        }
        for (r, ch) in burning {
            queue_destruction(c, &sim, r, &ch, None);
        }
    }
    // Each shot is a temporary reference of its PROJ for scripts (`fnv_game::nvse_proj`):
    // made the frame it is fired, moved as it flies; a speed multiplier a script set
    // speeds the flight or the hit's arrival [observed: proj1].
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        let forms = &c.forms;
        let (flying, pending, now) = (&mut c.flying, &mut c.pending, c.time);
        with_world(rt, &mut sim, |rt| {
            for f in flying.iter_mut() {
                if !f.registered {
                    f.registered = true;
                    let damage = forms.weapons.get(&f.by.weapon).map_or(0.0, |w| w.base_damage as f32);
                    f.rref = rt.game.projectile_fired(f.proj, f.by.actor, f.by.weapon, damage, f.p.pos);
                    debug!("combat: shot reference {:?} for {} (speed {:.0}, hitscan {})", f.rref, f.proj, f.p.speed, f.p.hitscan);
                }
                let Some(r) = f.rref else { continue };
                if let Some(m) = rt.game.gaps.projs.shots.get(&r).map(|s| s.speed_mult).filter(|m| *m > 0.0 && *m != f.speed_mult) {
                    let k = m / f.speed_mult;
                    f.p.vel = vec::scale(f.p.vel, k);
                    f.p.speed *= k;
                    f.speed_mult = m;
                }
                rt.game.projectile_moved(r, f.p.pos, f.p.age, f.p.travelled);
            }
            for p in pending.iter_mut() {
                let Some(r) = p.rref else { continue };
                if let Some(m) = rt.game.gaps.projs.shots.get(&r).map(|s| s.speed_mult).filter(|m| *m > 0.0 && *m != p.speed_mult) {
                    p.due = now + (p.due - now).max(0.0) * p.speed_mult / m;
                    p.speed_mult = m;
                }
                let travelled = rt.game.gaps.projs.shots.get(&r).map_or(0.0, |s| s.travelled);
                rt.game.projectile_moved(r, p.point, now - p.since, travelled);
            }
        });
    }
    if c.flying.is_empty() && c.pending.is_empty() && c.boom.is_empty() && c.ghosts.is_empty() {
        if !visuals.flying.is_empty() {
            visuals.flying.clear();
        }
        return;
    }
    // Shots whose flight ended this frame without a hit to deliver: their references go.
    let mut over: Vec<(FormId, f32)> = Vec::new();
    {
        let world = view(&mut phys, script.as_deref(), Some(&sim));
        let hold = c.tracer_hold;
        for f in &mut c.flying {
            let before = f.p.pos;
            // Where it is drawn: from the muzzle, closing on its path (`Seen`).
            let seen_before = f.seen.at(before, f.p.travelled);
            // Found at once: what is drawn flies (or, a beam, stands) from there to
            // where it ended.
            let at_once = instant(&f.p) && f.p.age == 0.0;
            let fade = c.forms.projectiles.get(&f.proj).map_or(0.0, |p| p.fade_duration);
            let (id, proj, speed, beam, owner) = (f.id, f.proj, f.p.speed, f.p.motion == Motion::Beam, f.by.actor);
            // What stays on show after the shot (a beam, the line along the shot) keeps
            // its start on the muzzle: the attack clip moves the gun in the frames after
            // the shot [observed: the laser rifle's muzzle is 10 units higher one frame
            // after it fires], and a beam left where the muzzle was hangs below the barrel.
            // Only the player's own: the muzzle followed is the player's weapon's, so an
            // enemy's shot was drawn from the player's gun (the owner's playtest list 5
            // item 20).
            let follow = at_once && f.from_muzzle && f.by.actor == FormId::PLAYER_REF;
            // A thrown weapon lands where its flight ends (`crate::thrown`): the weapon
            // thrown, else the PROJ's Default Weapon Source.
            let lands = c.forms.projectiles.get(&f.proj).filter(|p| p.lands_as_weapon()).and_then(|p| {
                let thrown = c.forms.weapons.get(&f.by.weapon).is_some_and(|w| w.projectile == Some(f.proj));
                if thrown { Some(f.by.weapon) } else { p.default_weapon }
            });
            let ghost = move |to: V3| Ghost { id, proj, owner, from: seen_before, to, speed, travelled: 0.0, beam, left: fade, fade, fresh: true, follow: follow && beam };
            let line = move |to: V3, life: f32| Mark { follow, ..Mark::tracer(seen_before, to, life) };
            let touched_before = f.p.first_contact.is_some();
            let events = f.p.step(&world, dt);
            // An explosive's arc, for comparing with the original's (combat-rules.md §5,
            // issue #1471): where it left and how, and where it first touched something.
            if let (false, Some((point, age)), Some((from, v)), Some(_)) = (touched_before, f.p.first_contact, f.p.launch, f.p.explosion) {
                let edid = c.forms.projectiles.get(&f.proj).and_then(|p| p.edid.as_deref()).unwrap_or("?");
                let across = |a: V3, b: V3| (a[0] - b[0]).hypot(a[1] - b[1]);
                info!(
                    "combat: projectile {edid} left ({:.0}, {:.0}, {:.0}) at {:.0} units/s, {:.1} deg up; first touched ({:.0}, {:.0}, {:.0}) after {:.2} s, {:.0} units across the ground and {:.0} below where it left",
                    from[0],
                    from[1],
                    from[2],
                    vec::len(v),
                    v[2].atan2(v[0].hypot(v[1])).to_degrees(),
                    point[0],
                    point[1],
                    point[2],
                    age,
                    across(point, from),
                    from[2] - point[2]
                );
            }
            if let (Some((from, _)), Some(ProjEvent::Explode { point, .. })) = (f.p.launch, events.iter().find(|e| matches!(e, ProjEvent::Explode { .. }))) {
                let edid = c.forms.projectiles.get(&f.proj).and_then(|p| p.edid.as_deref()).unwrap_or("?");
                info!(
                    "combat: projectile {edid} went off after {:.2} s at ({:.0}, {:.0}, {:.0}), {:.0} units across the ground from where it left{}",
                    f.p.age,
                    point[0],
                    point[1],
                    point[2],
                    (point[0] - from[0]).hypot(point[1] - from[1]),
                    if f.p.rolling { ", after rolling" } else { "" }
                );
            }
            for e in events {
                match e {
                    ProjEvent::Hit { actor, part, object, point, delay, normal, material } => {
                        let from = if f.p.hitscan { f.origin } else { before };
                        let d = vec::sub(point, from);
                        let dir = if vec::len(d) > 1e-3 { vec::scale(d, 1.0 / vec::len(d)) } else { [0.0; 3] };
                        let since = c.time - f.p.age;
                        c.pending.push(Pending { due: c.time + delay, actor, part, object, point, dir, normal, material, projectiles: f.projectiles_per_shot, force: f.force, by: f.by, rref: f.rref.take(), since, speed_mult: f.speed_mult });
                        if let Some(weapon) = lands {
                            let struck = actor.map(|a| if a == PLAYER { FormId::PLAYER_REF } else { FormId(a as u32) });
                            c.landed.push(LandedThrow { weapon, proj: f.proj, point, normal: Some(normal), dir: f.dir, struck, by: f.by.actor });
                        }
                        if f.tracer {
                            debug!(
                                "combat: tracer from ({:.1}, {:.1}, {:.1}) to the hit at ({:.1}, {:.1}, {:.1})",
                                seen_before[0], seen_before[1], seen_before[2], point[0], point[1], point[2]
                            );
                            c.marks.push(line(point, hold.unwrap_or(0.08)));
                        }
                        if at_once && f.shown {
                            c.ghosts.push(ghost(point));
                        }
                    }
                    // A harmless shot sets nothing off [design].
                    ProjEvent::Explode { explosion, point } if !f.by.harmless => c.boom.push((explosion, point, Some(Cause::of(&f.by)))),
                    ProjEvent::Explode { .. } => {}
                    ProjEvent::Settled { point, .. } => c.settled.push(SettledMine { proj: f.proj, point, dir: f.dir, actor: f.by.actor }),
                    ProjEvent::Splash { point, dir } => {
                        let delay = if at_once && !beam && speed > 0.0 { vec::len(vec::sub(point, f.origin)) / speed } else { 0.0 };
                        c.splashes.push(Splash { due: c.time + delay, point, dir, weapon: f.by.weapon, shooter: f.by.actor });
                    }
                    // A tracer that meets nothing is drawn to where it ends.
                    ProjEvent::Expired { point } => {
                        f.ended_at = Some(point);
                        if let Some(weapon) = lands {
                            c.landed.push(LandedThrow { weapon, proj: f.proj, point, normal: None, dir: f.dir, struck: None, by: f.by.actor });
                        }
                        if f.tracer {
                            c.marks.push(line(point, hold.unwrap_or(0.08)));
                        }
                        if at_once && f.shown {
                            c.ghosts.push(ghost(point));
                        }
                    }
                }
            }
            if f.tracer && f.p.alive {
                c.marks.push(Mark::tracer(seen_before, f.seen.at(f.p.pos, f.p.travelled), hold.unwrap_or(0.05)));
            }
            if vec::len(f.p.vel) > 1.0 {
                f.dir = vec::normalize(f.p.vel);
            }
        }
        // A shot found at once (hitscan) flies on for scripts as long as the bullet
        // would take to cover its way [observed: proj1, the 10mm shot aimed up still
        // there the frame after; hypothesis: the time at its speed].
        over.extend(c.flying.iter().filter(|f| !f.p.alive).filter_map(|f| {
            let left = match (f.p.hitscan && f.p.speed > 0.0, f.ended_at) {
                (true, Some(end)) => (vec::len(vec::sub(end, f.origin)) / f.p.speed - f.p.age).max(0.0),
                _ => 0.0,
            };
            f.rref.map(|r| (r, left))
        }));
        c.flying.retain(|f| f.p.alive);
    }
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()).filter(|_| !over.is_empty()) {
        with_world(rt, &mut sim, |rt| {
            for (r, left) in over {
                debug!("combat: shot reference {r} ends, {left:.3} s left in flight");
                if left > 0.0 {
                    rt.game.projectile_flies_on(r, left);
                } else {
                    rt.game.projectile_gone(r);
                }
            }
        });
    }
    // What is drawn this frame: the projectiles in flight where they are seen, and
    // those found at once on their way.
    c.ghosts.retain_mut(|g| {
        if let (true, Some(m)) = (g.follow, anim.muzzle) {
            g.from = m;
        }
        if std::mem::take(&mut g.fresh) {
            return true;
        }
        if g.beam {
            g.left -= dt;
            g.left > 0.0
        } else {
            g.travelled += g.speed.max(1.0) * dt;
            g.travelled < vec::len(vec::sub(g.to, g.from))
        }
    });
    visuals.flying.clear();
    visuals.flying.extend(c.flying.iter().filter(|f| f.shown).map(|f| ShotVisual { id: f.id, projectile: f.proj, owner: f.by.actor, pos: f.seen.at(f.p.pos, f.p.travelled), dir: f.dir, beam: None, travelled: f.p.travelled, strength: 1.0 }));
    visuals.flying.extend(c.ghosts.iter().map(|g| {
        let d = vec::sub(g.to, g.from);
        let len = vec::len(d);
        let dir = if len > 1e-3 { vec::scale(d, 1.0 / len) } else { [0.0, 1.0, 0.0] };
        ShotVisual { id: g.id, projectile: g.proj, owner: g.owner, pos: vec::add(g.from, vec::scale(dir, g.travelled)), dir, beam: g.beam.then_some(len), travelled: g.travelled, strength: beam_strength(g.beam, g.left, g.fade) }
    }));
    // Explosions, with those they set off, a bounded number per frame.
    let mut n = 0;
    while let Some((id, at, cause)) = c.boom.pop() {
        n += 1;
        if n > MAX_EXPLOSIONS_PER_FRAME {
            warn!("combat: more than {MAX_EXPLOSIONS_PER_FRAME} explosions in one frame; {} left for the next", c.boom.len() + 1);
            c.boom.push((id, at, cause));
            break;
        }
        stats.explosions += 1;
        let Some(mut x) = c.forms.explosions.get(&id).cloned() else { continue };
        let ground = phys.ground_below(at, EXPLOSION_GROUND_UP, EXPLOSION_GROUND_REACH);
        // Splash Damage's Adjust Explosion Radius, for the cause's blasts (the player's
        // and its teammates'; perk-entry-points.md §8).
        if let Some(cause) = cause {
            let (r, by) = owner_weapon_perk(script.as_deref_mut(), &mut sim, cause.actor, fnv_game::perks::ADJUST_EXPLOSION_RADIUS, cause.weapon, x.radius);
            if !by.is_empty() && (r - x.radius).abs() > 1e-3 {
                info!("combat: perks change {}'s radius {:.0} -> {:.0} ({})", x.edid.as_deref().unwrap_or("?"), x.radius, r.max(0.0), by.join(", "));
                x.radius = r.max(0.0);
            }
        }
        // Water near enough to throw up, when no ground lies between the blast and it:
        // the water shows instead of the blast's own model (no fireball in the
        // original, `WATER_SURFACE_EXPLOSION`); its damage and push are as on land.
        let reach = c.forms.setting("fExplosionSplashRadius").filter(|r| *r > 0.0).unwrap_or(WATER_BLAST_REACH);
        let water = water_thrown_up(at, phys.world.water_surface_for(at), ground.as_ref().map(|g| g.point[2]), reach);
        let look = if water.is_some() { ExplosionLook::Hidden } else { ExplosionLook::Own };
        effects.2.write(ExplosionFx { explosion: id, point: at, cause: cause.map(|c| c.actor), normal: ground.as_ref().map(|g| g.normal), look });
        if let Some(surface) = water {
            let point = [at[0], at[1], surface];
            stats.water_blasts += 1;
            info!("combat: explosion {} throws up water at ({:.0}, {:.0}, {:.0}), {:.0} units from the surface", x.edid.as_deref().unwrap_or("?"), point[0], point[1], point[2], (at[2] - surface).abs());
            // No one's: it is not heard as a second blast (detection hears the blast's).
            effects.2.write(ExplosionFx { explosion: WATER_EXPLOSION, point, cause: None, normal: None, look: ExplosionLook::WaterSurface });
            if let Some(splash) = sfx.as_deref().and_then(|s| s.forms.explosions.get(&WATER_EXPLOSION)) {
                for sound in splash.sound_1.into_iter().chain(splash.sound_2) {
                    effects.0.write(PlaySound { sound, at: Some(point) });
                }
            }
        }
        if let Some(e) = sfx.as_deref().and_then(|s| s.forms.explosions.get(&id)) {
            for sound in e.sound_1.into_iter().chain(e.sound_2) {
                effects.0.write(PlaySound { sound, at: Some(at) });
            }
            // The impact of the EXPL's data set for the ground under it, when the ground
            // is near: the dirt it throws up (`ImpactExplosionDirt.nif`) [hypothesis:
            // that the game shows the EXPL's IPDS on what lies under the blast, as a
            // shot's on what it hits; the reach is a design choice].
            // Not on a bed under the water it threw up.
            let impact = e.impact_dataset.zip(ground.as_ref().filter(|_| water.is_none())).and_then(|(set, g)| sfx.as_deref()?.forms.impact_id(set, fnv_combat::fx::impact_material(g.material)));
            if let (Some(impact), Some(g)) = (impact, ground.filter(|_| water.is_none())) {
                // The dirt is the cause's (no one's without one): detection hears it as theirs.
                effects.1.write(ImpactFx { impact, shooter: cause.map_or(FormId(0), |c| c.actor), point: g.point, normal: g.normal, dir: [0.0, 0.0, -1.0], actor: None, no_spray: false, no_decal: false });
                if let Some(i) = sfx.as_deref().and_then(|s| s.forms.impacts.get(&impact)) {
                    for sound in i.sound_1.into_iter().chain(i.sound_2) {
                        effects.0.write(PlaySound { sound, at: Some(g.point) });
                    }
                }
            }
        }
        let hits = {
            let world = view(&mut phys, script.as_deref(), Some(&sim));
            blast(&world, &c.tuning, &x, at)
        };
        let always = x.flags.iter().any(|f| f == "Knock Down - Always");
        let by_formula = x.flags.iter().any(|f| f == "Knock Down - By Formula");
        for b in hits {
            let target = if b.actor == PLAYER { FormId::PLAYER_REF } else { FormId(b.actor as u32) };
            let (dt_v, dr) = target_armor(c, script.as_deref_mut(), &mut sim, target);
            // ShowOff:OnExplosionHit's handlers may change the blast's damage first.
            let raw = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map_or(b.damage, |rt| rt.game.explosion_hit(target, f64::from(b.damage)) as f32);
            let (ap, tp) = blast_perks(script.as_deref_mut(), &mut sim, cause, target);
            let d = damage::blast_hit(&c.rules, &c.tuning, raw, dt_v, dr, &ap, &tp);
            log_blast_perks(cause, target, &ap, &tp, &d);
            let dealt = d.dealt;
            let info = blast_hit_info(cause);
            // The blast's object effect on those it catches (hit-effects.md §1).
            let items: Vec<FormId> = x.enchantment.into_iter().collect();
            // Its limb damage, on the torso and legs (crippled-limbs.md §5): from the damage
            // after armour; `Game::blast_limbs` applies the difficulty, as `hit` does to Health.
            let limbs = blast_limb_hits(c, &parts, target, dealt, cause.and_then(|k| c.forms.weapons.get(&k.weapon)));
            let (r, hit) = hit_actor(script.as_deref_mut(), &mut sim, target, dealt, info, &mut stats, &mut feedback, hud.as_deref_mut(), &items, false, None, &limbs).unzip();
            let mut hit = hit.flatten();
            if r.as_ref().is_none_or(|r| !r.immune) {
                wear_armor(c, &mut sim, &mut stats, target, Zone::All, b.damage);
            }
            // A blast that kills (or hits a corpse) may dismember the part facing it, by
            // its weapon's On Hit (dismemberment.md §4, §7): most grenades and every mine are
            // No Dismember/Explode; launchers, the Fat Man and dynamite are not.
            if let (Some(rr), Some(w)) = (r.as_ref().filter(|r| r.died || r.corpse), cause.and_then(|k| c.forms.weapons.get(&k.weapon)).cloned()) {
                let center = { view(&mut phys, script.as_deref(), Some(&sim)).actors_near(at, x.radius.max(1.0)).into_iter().find(|(a, _)| *a == b.actor).map(|(_, p)| p) };
                let facing = center.and_then(|p| facing_part(&view(&mut phys, script.as_deref(), Some(&sim)), b.actor, at, p));
                if let Some(check) = facing.and_then(|(part, point)| dismember_check(target, Some(part), &w, rr, info.attacker, point, b.push)) {
                    dm.checks.push(check);
                }
            }
            if let Some(r) = r {
                // Knockdown [documented: the EXPL flags; Explosion Knockdown Formula].
                let agility = actor_av(script.as_deref_mut(), &mut sim, target, fnv_game::actor_values::AGILITY).unwrap_or(5.0);
                let chance = if r.died || r.immune {
                    0.0
                } else if always {
                    1.0
                } else if by_formula {
                    damage::knockdown_chance(&c.rules, r.dealt as f32, agility, (r.health + r.dealt) as f32, r.max_health as f32)
                } else {
                    0.0
                };
                let rolled = chance > 0.0 && c.rng.next_f32() < chance;
                // The rules lay it down (unconscious.md §5); refused for actors flagged
                // No Knockdowns and for the player seated or talking.
                let shove = [b.push[0] * BLAST_KNOCKDOWN_SPEED, b.push[1] * BLAST_KNOCKDOWN_SPEED, b.push[2].max(0.0) * BLAST_KNOCKDOWN_SPEED + BLAST_KNOCKDOWN_SPEED * fnv_game::unconscious::PUSH_LIFT];
                let down = rolled && script.as_deref_mut().and_then(|s| s.rt.as_mut()).is_some_and(|rt| rt.game.knock_down(target, info.attacker, shove));
                if down {
                    stats.knockdowns += 1;
                    // Laid down, it does not also stagger from the limbs the blast crippled.
                    if let Some(h) = hit.as_mut() {
                        h.crippled = false;
                    }
                    if target == FormId::PLAYER_REF {
                        if let Some(h) = feedback.player_hits.last_mut() {
                            h.crippled = false;
                        }
                    }
                }
                info!(
                    "combat: blast {} on {target} at {:.0}: {:.1} health damage, health {:.0}/{:.0}{}",
                    x.edid.as_deref().unwrap_or("?"),
                    b.distance,
                    r.dealt,
                    r.health,
                    r.max_health,
                    if down { ", knocked down" } else if rolled { ", knockdown refused (No Knockdowns, or the player seated or talking)" } else { "" }
                );
            }
            if let Some(h) = hit {
                messages.write(h);
            }
        }
        // Not one gone from the world this frame: a mine that went off is disabled
        // before its explosion, so its blast does not set off its destruction stage too.
        for (r, d) in destructibles_near(&mut phys, &sim, script.as_deref(), at, x.radius).into_iter().filter(|(r, _)| sim.present(r.0)).collect::<Vec<_>>() {
            let dmg = damage::explosion_damage(&c.tuning, x.damage, x.radius, d);
            damage_destructible(c, &mut sim, script.as_deref_mut(), r, dmg, cause);
        }
        let pushed = phys.world.push_dynamic(at, x.radius, x.force);
        c.marks.push(Mark::at(at, 0.6, MarkKind::Impact));
        info!(
            "combat: explosion {} at ({:.0}, {:.0}, {:.0}), {pushed} loose object(s) pushed{}",
            x.edid.as_deref().unwrap_or("?"),
            at[0],
            at[1],
            at[2],
            if x.placed_object.is_some() { " (placed object not built yet)" } else { "" }
        );
        // The radiation it leaves (docs/spec-notes/radiation.md §2.1).
        if x.radiation_level > 0.0 {
            let zone = script.as_deref_mut().and_then(|s| s.rt.as_mut()).and_then(|rt| with_world(rt, &mut sim, |rt| rt.game.add_radiation_zone(id, at, x.radiation_level, x.radiation_radius, x.radius, x.radiation_dissipation_time)));
            if let Some(z) = zone {
                info!(
                    "radiation: explosion {} leaves a radiation zone (#{}): {:.1} rads/s within {:.0} units, fading over {:.0} s",
                    x.edid.as_deref().unwrap_or("?"),
                    z.serial,
                    z.level,
                    z.radius,
                    z.duration
                );
            }
        }
    }
    // Shots that went down into water: the water impact of the weapon's impact data set
    // on the surface, facing up (no decal on water).
    let (splashes, keep): (Vec<Splash>, Vec<Splash>) = std::mem::take(&mut c.splashes).into_iter().partition(|w| w.due <= c.time);
    c.splashes = keep;
    if let Some(s) = sfx.as_deref() {
        for w in splashes {
            let impact = c.forms.weapons.get(&w.weapon).and_then(|x| x.impact_dataset).and_then(|set| s.forms.impact_id(set, "water"));
            if let Some(impact) = impact {
                stats.water_splashes += 1;
                debug!("combat: splash {impact} at ({:.0}, {:.0}, {:.0})", w.point[0], w.point[1], w.point[2]);
                show_impact(s, script.as_deref(), &sim, ImpactFx { impact, shooter: w.shooter, point: w.point, normal: [0.0, 0.0, 1.0], dir: w.dir, actor: None, no_spray: false, no_decal: true }, &mut effects.0, &mut effects.1);
            }
        }
    }
    let now = c.time;
    let due: Vec<Pending> = {
        let (d, keep): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut c.pending).into_iter().partition(|p| p.due <= now);
        c.pending = keep;
        d
    };
    for mut p in due {
        // What scripts set on the shot (`fnv_game::nvse_proj`): its base damage and its
        // source, who the hit counts as [observed: proj1, set values held to the hit].
        let struck = match (p.actor, p.object) {
            (Some(a), _) if a == PLAYER => FormId::PLAYER_REF,
            (Some(a), _) => FormId(a as u32),
            (None, Some(o)) => FormId(o as u32),
            (None, None) => FormId(0),
        };
        let shot = p.rref.and_then(|r| script.as_deref_mut().and_then(|s| s.rt.as_mut()).and_then(|rt| with_world(rt, &mut sim, |rt| rt.game.projectile_impact(r, struck, p.material))));
        if let Some(s) = &shot {
            p.by.actor = s.source;
        }
        let id = p.by.weapon;
        // The player's weapon hits with its mods' figures (weapon-mods.md §2).
        let w = if p.by.actor == FormId::PLAYER_REF { c.player_stats(id) } else { c.forms.weapons.get(&id).cloned() };
        let Some(mut w) = w else { continue };
        if let Some(s) = &shot {
            w.base_damage = s.damage.round() as i32;
        }
        let chain = c.forms.chain(id, p.by.ammo);
        let effect_ids = chain.map(|ch| ch.ammo_effects).unwrap_or_default();
        let shooter = p.by.actor;
        c.marks.push(Mark::at(p.point, 0.3, MarkKind::Impact));
        // The impact the weapon's data set names for what was hit: an actor's is its
        // base's Impact Material Type (NPC_ / CREA `NAM4`), organic without one
        // [design]; anything else's its collision material (docs/spec-notes/audio.md §11).
        if let Some(s) = sfx.as_deref() {
            let (impact, material) = impact_for(s, script.as_deref(), &sim, id, p.actor, p.material);
            if let Some(impact) = impact {
                show_impact(s, script.as_deref(), &sim, ImpactFx { impact, shooter, point: p.point, normal: p.normal, dir: p.dir, actor: p.actor.map(|a| a as u32), no_spray: false, no_decal: false }, &mut effects.0, &mut effects.1);
            }
            let hit = p.actor.map(|a| a as u32).or(p.object.map(|o| o as u32));
            effects.4.write(crate::world_sounds::log_impact(s, hit, p.actor.is_some(), p.material, material, impact));
        }
        // A harmless shot is seen and heard where it lands, and does nothing more.
        if p.by.harmless {
            stats.harmless_hits += 1;
            continue;
        }
        let ammo_effects: Vec<&AmmoEffect> = effect_ids.iter().filter_map(|e| c.forms.ammo_effects.get(e)).collect();
        let mut a = attacker(script.as_deref_mut(), &mut sim, &c.rules, shooter, &w, p.by.vats);
        a.sneaking = p.actor.is_some_and(|t| sneak_attack(script.as_deref(), shooter, p.by.sneaking, FormId(t as u32)));
        let bp = p.actor.and_then(|a| body_part(c, &parts, a, p.part)).or_else(|| {
            (p.actor == Some(PLAYER)).then(|| player_body_part(c, &phys, player.as_deref(), p.point)).flatten()
        });
        let part = bp.as_ref().map_or_else(HitPart::default, BodyPart::hit_part);
        let h = HitInput { weapon: &w, projectiles: p.projectiles, condition: item_condition(&sim, shooter, id), ammo_effects: &ammo_effects, part, power: 1.0 };
        let roll = c.rng.next_f32();
        let Some(actor) = p.actor else {
            stats.world_hits += 1;
            debug!("combat: shot lands at ({:.0}, {:.0}, {:.0}) on {}", p.point[0], p.point[1], p.point[2], p.object.map_or("no object".to_string(), |o| format!("{:08X}", o as u32)));
            if let Some(obj) = p.object {
                let r = FormId(obj as u32);
                // The object's OnHit and OnHitWith blocks, before any damage
                // [documented: GECK wiki Event Handling] (combat-rules.md §14).
                let info = HitInfo { attacker: Some(shooter), weapon: Some(id), location: -1, critical: false, limb: None };
                let ran = script.as_deref_mut().and_then(|s| s.rt.as_mut()).is_some_and(|rt| with_world(rt, &mut sim, |rt| rt.game.hit_object(r, info)));
                // The weapon's object effect reaches the object: its OnMagicEffectHit
                // blocks (the Vault 22 traps catch the flamer's burn this way).
                let items: Vec<FormId> = c.forms.weapons.get(&id).and_then(|w| w.enchantment).into_iter().collect();
                let ran = ran | script.as_deref_mut().and_then(|s| s.rt.as_mut()).is_some_and(|rt| with_world(rt, &mut sim, |rt| rt.game.magic_effect_hits(r, &items, Some(shooter))));
                // A destructible object takes the hit's damage (no armour) (step C7).
                let (ap, _) = hit_perks(script.as_deref_mut(), &mut sim, shooter, id, None);
                let d = damage::hit(&c.rules, &c.tuning, &h, &a, &Target::default(), &ap, &NoPerks, roll);
                damage_destructible(c, &mut sim, script.as_deref_mut(), r, d.dealt, Some(Cause::of(&p.by)));
                // A loose object is knocked away.
                let impulse = vec::scale(p.dir, p.force * OBJECT_PUSH * phys.world.params.units_per_metre);
                let pushed = phys.world.push_ref(obj as u32, p.point, impulse, OBJECT_PUSH_MAX_SPEED);
                stats.object_scripts += ran as u32;
                stats.objects_pushed += pushed.is_some() as u32;
                if ran || pushed.is_some() {
                    info!(
                        "combat: shot hits {} {r}{}{}",
                        sim.base_of(obj as u32).and_then(|b| sim.name(b).map(str::to_string)).unwrap_or_else(|| "object".into()),
                        if ran { ", its hit blocks ran" } else { "" },
                        pushed.map_or(String::new(), |v| format!(", knocked away at {v:.0} units/s"))
                    );
                }
            }
            continue;
        };
        stats.actor_hits += 1;
        let target = if actor == PLAYER { FormId::PLAYER_REF } else { FormId(actor as u32) };
        // A shot on the weapon the actor holds wears the weapon, not the actor (§17).
        let (ap, tp) = hit_perks(script.as_deref_mut(), &mut sim, shooter, id, Some(target));
        if p.part == Some(WEAPON_PART) {
            let d = damage::hit(&c.rules, &c.tuning, &h, &a, &Target::default(), &ap, &NoPerks, roll);
            strike_weapon(&c.forms, &mut sim, &mut stats, hud.as_deref_mut(), &mut effects.3, script.as_deref_mut(), &parts, target, d.raw, p.point);
            continue;
        }
        // A piece of gore (an exploded part's bits, dismemberment.md §6) is seen hit, no more.
        if p.part == Some(fnv_phys::gore::GORE_PART) {
            continue;
        }
        let (dt_v, dr) = target_armor(c, script.as_deref_mut(), &mut sim, target);
        let resist = target_resist(script.as_deref_mut(), &mut sim, target, &w);
        let nerve = companion_nerve(script.as_deref_mut(), &mut sim, target);
        let d = damage::hit(&c.rules, &c.tuning, &h, &a, &Target { dt: dt_v, dr, resist, nerve, ..Target::default() }, &ap, &tp, roll);
        log_hit_perks(shooter, Some(target), &ap, &tp, &d);
        // [design] Missing body part data keeps the whole-capsule fallback (location 0).
        let location = bp.as_ref().map_or(0, BodyPart::location);
        let limb = limb_hit(bp.as_ref(), &d, &w);
        let info = HitInfo { attacker: Some(shooter), weapon: Some(id), location, critical: d.critical, limb };
        if target != FormId::PLAYER_REF {
            pushes.write(ActorPush { form_id: target.0, point: p.point, dv: vec::scale(p.dir, SHOT_PUSH) });
        }
        let items = hit_items(c, script.as_deref_mut(), &mut sim, &mut stats, shooter, Some(id), None);
        let r = apply_hit(script.as_deref_mut(), &mut sim, target, d.dealt, info, &mut stats, &mut feedback, hud.as_deref_mut(), &mut messages, &items, false, crit_effect(c, Some(id)));
        armour_lines(script.as_deref_mut(), Some(shooter), target, &d, c.tuning.dt_floor);
        spread_hit_areas(script.as_deref_mut(), &mut sim, &mut stats, target, &items, info.attacker);
        // The ammo's Fatigue Mod (AMEF type 5: the bean bag's 250) takes that much fatigue,
        // once a hit [documented: GECK wiki GetAmmoEffectTraitNumeric effect types;
        // hypothesis: from 0, once a hit; logic-fields.md §3].
        let fatigue = damage::apply_effects(0.0, &ammo_effects, "Fatigue Mod");
        if fatigue > 0.0 && r.as_ref().is_some_and(|r| !r.immune) {
            if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
                with_world(rt, &mut sim, |rt| rt.game.damage_fatigue(target, f64::from(fatigue)));
                info!("combat: {target} loses {fatigue:.0} fatigue to the ammo");
            }
        }
        // The Knockdown Chance perks on the shooter's hit (And Stay Back, per pellet).
        if r.as_ref().is_some_and(|r| !r.died && !r.immune) && target != FormId::PLAYER_REF {
            perk_knockdown(c, script.as_deref_mut(), &mut sim, shooter, id, target, p.dir);
        }
        if let Some(check) = r.as_ref().and_then(|r| dismember_check(target, p.part, &w, r, Some(shooter), p.point, p.dir)) {
            dm.checks.push(check);
        }
        if r.as_ref().is_none_or(|r| !r.immune) {
            wear_armor(c, &mut sim, &mut stats, target, Zone::of(bp.as_ref()), d.raw);
        }
        match r {
            Some(r) => info!(
                "combat: hit {target} on {} for {:.1} (DT {:.0}{}), health {:.0}/{:.0}{}",
                bp.as_ref().and_then(|b| b.name.as_deref()).unwrap_or("the body"),
                r.dealt,
                d.dt,
                crit_note(&a, &d),
                r.health,
                r.max_health,
                if r.immune { ", takes no damage" } else { "" }
            ),
            None => {
                stats.damage += d.dealt;
                info!("combat: hit {target} for {:.1} (no script runtime: not applied)", d.dealt);
            }
        }
    }
}

fn draw_marks(time: Res<Time>, mut c: ResMut<Combat>, anim: Res<WeaponAnim>, show: Res<HitMarks>, mut gizmos: Gizmos) {
    let dt = time.delta_secs();
    // The test aid keeps its lines where they were drawn, for the record of a check.
    let follow = c.tracer_hold.is_none();
    let draw = show.show || c.tracer_hold.is_some();
    for m in &mut c.marks {
        m.age += dt;
        if let (true, true, Some(muzzle)) = (follow, m.follow, anim.muzzle) {
            m.from = muzzle;
        }
        if !draw {
            continue;
        }
        let fade = (1.0 - m.age / m.life).clamp(0.0, 1.0);
        match m.kind {
            MarkKind::Tracer => {
                gizmos.line(space::to_bevy(m.from), space::to_bevy(m.to), Color::srgba(1.0, 0.9, 0.6, fade));
            }
            MarkKind::Impact | MarkKind::Flash => {
                let p = space::to_bevy(m.to);
                let (s, col) = if m.kind == MarkKind::Flash { (FLASH_SIZE, Color::srgba(1.0, 0.95, 0.7, fade)) } else { (4.0, Color::srgba(1.0, 0.8, 0.3, fade)) };
                gizmos.line(p - Vec3::X * s, p + Vec3::X * s, col);
                gizmos.line(p - Vec3::Y * s, p + Vec3::Y * s, col);
                gizmos.line(p - Vec3::Z * s, p + Vec3::Z * s, col);
            }
        }
    }
    c.marks.retain(|m| m.age < m.life);
}

/// What the HUD shows of the weapon the player has equipped.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HudWeapon {
    /// The equipped weapon's condition, 0 to 1 (§17); `None` with bare hands.
    pub condition: Option<f32>,
    /// Rounds in the magazine and rounds carried besides, for a weapon that uses ammo;
    /// a weapon that spends itself (thrown) has its count first and 0 besides.
    pub ammo: Option<(u32, u32)>,
    /// The ammo type's abbreviation (AMMO `QNAM`: "HP", "JHP", "Slg"), shown left of the
    /// counter; standard rounds have none (hud.md §4).
    pub abbrev: Option<String>,
}

impl Combat {
    /// The equipped weapon as the HUD shows it (docs/spec-notes/hud.md §5).
    pub fn hud_weapon(&self, sim: &SimState) -> HudWeapon {
        let condition = self.equipped.flatten().map(|id| item_condition(sim, FormId::PLAYER_REF, id));
        let ammo = match (self.equipped.flatten(), self.cycle.as_ref()) {
            (Some(_), Some(cy)) if !cy.no_ammo => {
                let total = player_count(sim, self.ammo);
                Some((cy.magazine.min(total), total.saturating_sub(cy.magazine)))
            }
            (Some(id), Some(_)) if self.self_ammo => Some((player_count(sim, Some(id)), 0)),
            _ => None,
        };
        let abbrev = match self.cycle.as_ref() {
            Some(cy) if ammo.is_some() && !cy.no_ammo => {
                self.ammo.and_then(|a| self.forms.ammo.get(&a)).and_then(|a| a.abbrev.clone()).filter(|a| !a.trim().is_empty())
            }
            _ => None,
        };
        HudWeapon { condition, ammo, abbrev }
    }
}

/// The Combat-tab lines said this frame (`fnv_game::combat_lines`), one log line each:
/// `combat line: <who> says <topic> line <info>`; tries without a line at debug level.
fn log_combat_lines(mut script: Option<ResMut<ScriptState>>, mut stats: ResMut<CombatStats>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let g = &mut rt.game;
    if g.combat_lines.said.is_empty() {
        return;
    }
    for (topic, who, line) in std::mem::take(&mut g.combat_lines.said) {
        match line {
            Some(l) => {
                stats.combat_lines += 1;
                info!("combat line: {} says {topic} line {:08X}", g.data.name(who), l.0);
            }
            None => {
                stats.combat_lines_none += 1;
                debug!("combat line: {} has no {topic} line", g.data.name(who));
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<CombatStats>) {
    if exit.read().last().is_some() {
        let s = &*stats;
        info!(
            "combat: {} shots ({} drawn from the muzzle, {} from behind cover), {} projectiles, {} actor hits, {} world hits ({} objects' hit blocks, {} objects knocked away), {} explosions ({} knockdowns), {} swings ({} chained, {} power, {} hits), {:.0} health damage, {} kills (+{} XP), {} reloads, {} dry fires, {} jams, {} melee impacts, {} water splashes, {} water blasts",
            s.shots, s.muzzle_shots, s.covered_shots, s.projectiles, s.actor_hits, s.world_hits, s.object_scripts, s.objects_pushed, s.explosions, s.knockdowns, s.swings, s.chained_swings, s.power_attacks, s.melee_hits, s.damage, s.kills, s.xp, s.reloads, s.dry_fires, s.jams, s.melee_impacts, s.water_splashes, s.water_blasts
        );
        if s.spent_rolls > 0 {
            let given: Vec<String> = s.spent_given.iter().map(|(i, n)| format!("{i:08X} x {n}")).collect();
            info!("combat: spent ammo items: {} shots rolled, {} given [{}]", s.spent_rolls, s.spent_given.values().sum::<u32>(), given.join(", "));
        }
        info!("combat lines: {} said, {} tries without a line", s.combat_lines, s.combat_lines_none);
        info!("combat blocks: {} blows blocked, {} blocks broken by power attacks", s.blows_blocked, s.blocks_broken);
        info!(
            "condition summary: the player's weapons lost {:.1} health, {} hits on held weapons, {} weapons broken, armour lost {:.1} health, {} pieces worn to nothing",
            s.weapon_wear, s.weapon_hits, s.weapons_broken, s.armor_wear, s.armor_broken
        );
        info!("{}", hit_effects::summary(&s.hit_effects));
        let passes = GHOST_PASSES.load(std::sync::atomic::Ordering::Relaxed);
        if passes > 0 {
            info!("combat summary: {passes} rays passed through ghosts (SetGhost or Is Ghost)");
        }
        if s.death_drops > 0 {
            info!("combat summary: {} weapons dropped by actors that died holding them", s.death_drops);
        }
        if s.disarmed > 0 {
            info!("crippled limbs summary: {} actors disarmed by a crippled arm", s.disarmed);
        }
        for (id, (name, hits, max)) in &s.npc_hits {
            let mut d = hits.clone();
            d.sort_by(f32::total_cmp);
            info!(
                "npc hits summary: {name} {id:08X} took {} hits from actors for {:.0} damage (median {:.1}, most {:.1}), maximum health {max:.0}",
                d.len(),
                d.iter().sum::<f32>(),
                d[d.len() / 2],
                d[d.len() - 1]
            );
        }
        for ((actor, weapon), w) in &s.npc_weapon_wear {
            info!(
                "npc weapon wear summary: {actor:08X}'s {weapon:08X} lost {:.2} health in {} attacks ({:.3} an attack), condition {:.1}% -> {:.1}%",
                w.points,
                w.attacks,
                w.points / w.attacks.max(1) as f32,
                w.first * 100.0,
                w.last * 100.0
            );
        }
    }
}

/// Hit effects that ended this frame are logged (`hit_effects`).
fn watch_hit_effects(script: Option<Res<ScriptState>>, mut stats: ResMut<CombatStats>) {
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else { return };
    hit_effects::watch(&rt.game, &mut stats.hit_effects);
    // A death by an effect's drain (a burn, a poison) or a script rather than by a hit.
    if !stats.player_died && rt.game.is_dead(FormId::PLAYER_REF) {
        stats.player_died = true;
        info!("combat: the player died (no hit: an effect's drain or a script)");
    }
}

/// GetAnimAction during a cycle's cooldown, `left` seconds before it is ready again (the
/// cycle already stepped by this frame's `dt`): the clip's time is the time since the
/// trigger at the frame's start, `interval - left - dt`, no earlier than the shot (the
/// cooldown starts at the shot, `fire_delay` in), over a clip of `interval`. A thrown weapon reads as a throw
/// (6 until the clip's `Attach`), the others as a shot.
pub fn cooldown_action(timing: &fnv_combat::attack::Timing, left: f32, thrown: bool, repeating: bool, dt: f32) -> Option<fnv_game::anim_state::AnimAction> {
    use fnv_game::anim_state::{shot_action, throw_action};
    let (fire, interval) = (timing.fire_delay, timing.interval);
    let t = (interval - left - dt).max(fire);
    if thrown {
        throw_action(t, fire, timing.attach, interval, dt)
    } else {
        shot_action(t, fire, interval, dt, repeating)
    }
}

/// The cycle's clip timing from the attack clip (length, `Hit` or `Fire` key, and
/// whether a `Loop` key follows that key), the reload clip (length and `Hit` key) and the
/// length of the reload's opening clip, as the player body reads them; `next_attack`: the
/// attack clip's `a:` key, its time and text (`fnv_actor::player_clips::ClipKeys`);
/// `attach`: the attack clip's `Attach` key (a thrown weapon's).
/// A casing of weapon `id` from its `ShellCasingNode` (the place and way the body
/// gives) at the game's eject speed (`fGunShellEjectSpeed`), for a weapon with a casing
/// model.
fn throw_casing(casings: &mut MessageWriter<CasingFx>, anim: &WeaponAnim, fx: Option<&fnv_combat::fx::WeaponFx>, id: FormId, rules: &Rules) {
    if let (Some((point, dir)), true) = (anim.casing, fx.is_some_and(|f| f.shell_casing_model.is_some())) {
        casings.write(CasingFx { weapon: id, point, velocity: vec::scale(dir, rules.shell_eject_speed), life: rules.shell_lifetime });
    }
}

pub fn clip_timing(attack: Option<(f32, Option<f32>, bool)>, reload: Option<(f32, Option<f32>)>, reload_open: Option<f32>, next_attack: Option<(f32, &str)>, attach: Option<f32>) -> ClipTiming {
    ClipTiming {
        attach_time: attach,
        next_attack: next_attack.and_then(|(t, v)| fnv_combat::melee::NextAttack::parse(v).map(|n| (t, n))),
        attack_len: attack.map(|a| a.0).filter(|l| *l > 0.0),
        hit_time: attack.and_then(|a| a.1),
        reload_len: reload.map(|r| r.0).filter(|l| *l > 0.0),
        reload_hit: reload.and_then(|r| r.1),
        reload_open_len: reload_open,
        spin_up: attack.filter(|a| a.2).and_then(|a| a.1),
        jam_len: None,
    }
}

/// A SOUN's editor ID for a log line (its form ID without one).
fn sound_edid(sfx: Option<&Sfx>, sound: FormId) -> String {
    sfx.and_then(|x| x.forms.sounds.get(&sound)).and_then(|s| s.edid.clone()).unwrap_or_else(|| sound.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phys::NpcBody;

    #[test]
    fn player_capsule_hit_bands_and_clamps() {
        use fnv_game::combat::{HEAD, LEFT_ARM, LEFT_MOBILITY, RIGHT_ARM, RIGHT_MOBILITY, TORSO};
        let part = |x: f32, z: f32| player_hit_condition([10.0 + x, 20.0, 30.0 + z], [10.0, 20.0, 30.0], 100.0, 10.0, 0.0);
        for (x, z, expected) in [
            (0.0, 87.0, HEAD), (-10.0, 1000.0, HEAD),
            (-1.0, 49.0, LEFT_MOBILITY), (1.0, 49.0, RIGHT_MOBILITY),
            (-1.0, -100.0, LEFT_MOBILITY), (1.0, -100.0, RIGHT_MOBILITY),
            (-7.0, 65.0, LEFT_ARM), (7.0, 65.0, RIGHT_ARM),
            (-6.0, 65.0, TORSO), (6.0, 65.0, TORSO), (0.0, 65.0, TORSO),
            (0.0, 50.0, TORSO), (0.0, 86.0, TORSO), (7.0, 86.0, RIGHT_ARM),
            (0.0, 0.0, RIGHT_MOBILITY),
        ] {
            assert_eq!(part(x, z), expected, "x {x}, z {z}");
        }
    }

    #[test]
    fn player_capsule_sides_follow_heading() {
        use fnv_game::combat::{LEFT_ARM, LEFT_MOBILITY, RIGHT_ARM, RIGHT_MOBILITY};
        for (heading, right) in [(std::f32::consts::FRAC_PI_2, [0.0, -8.0]), (std::f32::consts::PI, [-8.0, 0.0])] {
            for (side, arm, leg) in [(1.0, RIGHT_ARM, RIGHT_MOBILITY), (-1.0, LEFT_ARM, LEFT_MOBILITY)] {
                for (z, expected) in [(65.0, arm), (25.0, leg)] {
                    assert_eq!(player_hit_condition([right[0] * side, right[1] * side, z], [0.0; 3], 100.0, 10.0, heading), expected);
                }
            }
        }
    }

    #[test]
    fn player_shot_head_multiplier_and_blocked_blow_limb_damage() {
        let bp = BodyPart { damage_mult: 2.0, health_percent: 20, actor_value: Some("Perception Condition".into()), part_type: Some("Head 1".into()), ..Default::default() };
        let w = WeaponStats { base_damage: 16, limb_dmg_mult: 1.0, ..Default::default() };
        assert_eq!(bp.hit_part().damage_mult, 2.0);
        assert_eq!(bp.location(), 1);
        let hit = |part| damage::hit(&Rules::defaults(), &CombatTuning::default(), &HitInput { weapon: &w, projectiles: 1, condition: 1.0, ammo_effects: &[], part, power: 1.0 }, &Attacker::default(), &Target::default(), &NoPerks, &NoPerks, 1.0);
        let mut d = hit(bp.hit_part());
        let body = hit(HitPart::default());
        assert!(body.dealt > 0.0);
        assert_eq!(d.dealt, body.dealt * 2.0);
        assert_eq!(limb_hit(Some(&bp), &d, &w).unwrap().damage, f64::from(body.dealt));
        // [design: actor_attacks] Blows keep the default part and use damage left after blocking.
        d.part_mult = HitPart::default().damage_mult;
        d.dealt = damage::against_block(&CombatTuning::default(), 16.0, false).0;
        assert_eq!(limb_hit(Some(&bp), &d, &w).map(|l| l.damage), (d.dealt > 0.0).then_some(f64::from(d.dealt)));
        d.dealt = 0.0;
        assert!(limb_hit(Some(&bp), &d, &w).is_none());
        assert!(limb_hit(None, &d, &w).is_none());
    }

    #[test]
    fn npc_weapons_wear_only_with_their_own_damage_to_weapon_mult() {
        let mut forms = CombatForms::default();
        forms.weapons.insert(FormId(1), fnv_combat::data::WeaponStats::default());
        forms.weapons.insert(FormId(2), fnv_combat::data::WeaponStats { damage_to_weapon_mult: Some(0.005), ..Default::default() });
        assert!(!npc_weapon_wears(&forms, FormId(1)), "the masters' setting: an NPC's weapon does not wear (#1334)");
        assert!(npc_weapon_wears(&forms, FormId(2)), "the record's override: it wears, as Cook-Cook's Incinerator");
        assert!(!npc_weapon_wears(&forms, FormId(3)), "no record");
    }

    #[test]
    fn the_magazine_never_holds_more_than_is_carried() {
        assert_eq!(magazine_in_hand(30, 0, false, false, false), 0, "every round removed");
        assert_eq!(magazine_in_hand(30, 12, false, false, false), 12);
        // Fewer loaded than carried stay so: VATS mode counts the rounds loaded, and
        // a reload (whole or round by round) is what tops the magazine up.
        assert_eq!(magazine_in_hand(2, 20, false, false, false), 2);
        // No rounds in the inventory to follow: no ammo item, regenerating, god mode.
        assert_eq!(magazine_in_hand(1, 0, true, false, false), 1);
        assert_eq!(magazine_in_hand(5, 0, false, true, false), 5);
        assert_eq!(magazine_in_hand(30, 0, false, false, true), 30);
    }

    #[test]
    fn a_reload_or_a_jam_holds_the_sights_down() {
        for p in [
            Phase::Reloading { left: 1.0 },
            Phase::ReloadOpen { left: 0.3, stop: false },
            Phase::ReloadRound { left: 0.5, stop: false, taken: 1 },
            Phase::ReloadClose { left: 0.4, taken: 2 },
            Phase::Jammed { left: 1.2 },
        ] {
            assert!(sights_held_down(&p), "{p:?}");
        }
        // Firing and waiting keep them up.
        for p in [Phase::Ready, Phase::SpinUp { left: 0.2 }, Phase::WindUp { left: 0.1 }, Phase::Cooldown { left: 0.3 }] {
            assert!(!sights_held_down(&p), "{p:?}");
        }
    }

    #[test]
    fn a_knockback_pushes_half_its_force_over_its_time() {
        // A Sledgehammer blow of 14 on Agility 5: force 114 (combat-rules.md, Knockback).
        let r = fnv_combat::tuning::Rules::defaults();
        let force = damage::knockback_force(&r, 14.0, 5.0);
        assert!((force - 114.0).abs() < 0.5, "{force}");
        let k = Knockback::of(7, [0.0, 2.0, -1.0], force, 1.0).unwrap();
        assert_eq!(k.dir, [0.0, 1.0]);
        let dt = 1.0 / 60.0;
        let (mut left, mut far) = (k.secs, 0.0);
        while left > 0.0 {
            far += k.step(left, dt);
            left -= dt;
        }
        assert!((far - k.distance()).abs() < 1e-2 && (far - 57.0).abs() < 0.5, "{far}");
        // Straight down, or with no force: no push.
        assert!(Knockback::of(7, [0.0, 0.0, -1.0], force, 1.0).is_none() && Knockback::of(7, [1.0, 0.0, 0.0], 0.0, 1.0).is_none());
    }

    /// The laser rifle's beam (BeamLaserProjectile, Fade Duration 0.14 s) is drawn in
    /// full when made, at half strength half way through and not at all at the end; a
    /// bullet, or a beam whose PROJ gives no fade, in full.
    /// A stand-in world: a ray hits actor 7's part 3 at (1, 2, 3) unless 7 is ignored,
    /// when it hits actor 9 (no part); actors 7 and 9 stand near.
    struct Two;
    impl CombatWorld for Two {
        fn ray(&self, _from: V3, _to: V3, ignore: &[ActorId]) -> Option<fnv_combat::projectile::WorldHit> {
            let (actor, part) = if ignore.contains(&7) { (9, None) } else { (7, Some(3)) };
            Some(fnv_combat::projectile::WorldHit { point: [1.0, 2.0, 3.0], normal: [0.0, 0.0, 1.0], distance: 1.0, actor: Some(actor), part, object: None, small_transparent: false, material: None })
        }
        fn actors_near(&self, _point: V3, _radius: f32) -> Vec<(ActorId, V3)> {
            vec![(7, [0.0; 3]), (9, [0.0; 3])]
        }
    }

    #[test]
    fn a_blast_or_blow_lands_on_the_part_its_line_meets() {
        // The other actor is left out of the line: actor 7's part 3.
        assert_eq!(facing_part(&Two, 7, [0.0; 3], [1.0, 0.0, 0.0]), Some((3, [1.0, 2.0, 3.0])));
        // Actor 9 has no part volumes: nothing to dismember.
        assert_eq!(facing_part(&Two, 9, [0.0; 3], [1.0, 0.0, 0.0]), None);
    }

    #[test]
    fn a_beam_fades_over_its_fade_duration() {
        assert_eq!(beam_strength(true, 0.14, 0.14), 1.0);
        assert!((beam_strength(true, 0.07, 0.14) - 0.5).abs() < 1e-6);
        assert_eq!(beam_strength(true, -0.01, 0.14), 0.0);
        assert_eq!(beam_strength(false, 0.0, 0.14), 1.0);
        assert_eq!(beam_strength(true, 0.0, 0.0), 1.0);
    }

    /// A blast is its cause's hit: an NPC's Incinerator burst on a trooper is that NPC's
    /// with that weapon, not the player's (every blast used to be the player's, with the
    /// player's weapon: XP, hit marker and OnHit attacker for NPC-against-NPC blasts, and
    /// none of them in the `npc hits summary`). One nobody set off has no attacker.
    /// Only the player's hits made sneaking are sneak attacks; with no detection measured
    /// the target counts as unseen.
    #[test]
    fn sneak_attacks_are_the_sneaking_players() {
        let target = FormId(0x00104F0A);
        assert!(sneak_attack(None, FormId::PLAYER_REF, true, target));
        assert!(!sneak_attack(None, FormId::PLAYER_REF, false, target));
        assert!(!sneak_attack(None, FormId(0x000F5704), true, target));
    }

    #[test]
    fn a_blast_hit_is_its_causes_not_the_players() {
        let by = By { actor: FormId(0x000F5704), weapon: FormId(0x000906DA), ammo: None, harmless: false, vats: false, sneaking: false };
        let info = blast_hit_info(Some(Cause::of(&by)));
        assert_eq!((info.attacker, info.weapon, info.location, info.critical), (Some(FormId(0x000F5704)), Some(FormId(0x000906DA)), -1, false));
        let none = blast_hit_info(None);
        assert_eq!((none.attacker, none.weapon), (None, None));
    }

    /// A grenade's cooldown, played from the shot (the clip's `Release`, 0.867 s) as the
    /// cycle runs it: 6 until `Attach` (1.533 s), then 4 to the clip's end (1.7 s). The
    /// clip's time used to start at twice the fire delay, past `Attach` (smoke
    /// anim-state-throw: 5 then 4, no 6).
    #[test]
    fn a_throws_cooldown_reads_6_until_attach_then_4() {
        use fnv_game::anim_state::AnimAction;
        let timing = fnv_combat::attack::Timing { interval: 1.7, fire_delay: 0.867, reload: 2.0, spin_up: 0.0, rounds: None, attach: Some(1.533) };
        let dt = 1.0 / 60.0;
        let mut left = timing.interval - timing.fire_delay;
        let mut codes = Vec::new();
        while left > 0.0 {
            left -= dt;
            codes.push(cooldown_action(&timing, left.max(0.0), true, false, dt).map(AnimAction::code));
        }
        let sixes = codes.iter().filter(|c| **c == Some(6)).count();
        let fours = codes.iter().filter(|c| **c == Some(4)).count();
        assert_eq!(codes[0], Some(6), "6 from the release: {codes:?}");
        assert!((38..=41).contains(&sixes), "{codes:?}");
        assert!((8..=11).contains(&fours), "{codes:?}");
        // A pistol's shot: 3 the frame after, then 4 (no 2 in the cooldown).
        let gun = fnv_combat::attack::Timing { interval: 0.5, fire_delay: 0.033, attach: None, ..timing };
        let first = cooldown_action(&gun, gun.interval - gun.fire_delay - dt, false, false, dt);
        assert_eq!(first, Some(AnimAction::FollowThrough));
        assert_eq!(cooldown_action(&gun, 0.2, false, false, dt), Some(AnimAction::AttackReturn));
    }

    /// An NPC walked by a character controller: its character (a man-sized capsule from
    /// the ground) and, inside it, a smaller body hovering (a bloatfly's hit volume).
    fn npc_world() -> PhysState {
        let mut phys = PhysState::default();
        let id = 0x1234;
        let ch = phys.world.add_character(id, fnv_phys::CharacterParams::default(), [0.0, 0.0, 0.0]);
        phys.npcs.insert(id, NpcBody { prev_feet: ch.feet, ch, velocity: [0.0; 2], jump: false, sink: 0.0, lift: Default::default() });
        phys.world.set_actor_volume(id, fnv_phys::HitVolume { feet: [0.0, 0.0, 30.0], radius: 8.0, height: 30.0, dead: false });
        phys
    }

    #[test]
    fn shots_pass_an_npcs_character_and_strike_its_body() {
        // The playtest list 5 item 16: the character was struck as a wall (dust, no damage).
        let mut phys = npc_world();
        let world = view(&mut phys, None, None);
        let h = world.ray([-300.0, 0.0, 45.0], [300.0, 0.0, 45.0], &[PLAYER]).expect("the body is struck");
        assert_eq!(h.actor, Some(0x1234));
        assert!(h.point[0] > -10.0, "struck at the body, not the character's side: {:?}", h.point);
        // Below the body, through the character's legs: nothing.
        assert!(world.ray([-300.0, 0.0, 10.0], [300.0, 0.0, 10.0], &[PLAYER]).is_none());
    }

    #[test]
    fn jam_animations_beside_reload_animations() {
        assert_eq!(jam_anim("ReloadA").as_deref(), Some("JamA"));
        assert_eq!(jam_anim("ReloadZ").as_deref(), Some("JamZ"));
        assert_eq!(jam_anim("Reload"), None);
        assert_eq!(jam_anim("AttackLeft"), None);
    }

    #[test]
    fn explosions_throw_up_water_near_its_surface() {
        // Just over the surface, on it, and deep under it.
        let r = WATER_BLAST_REACH;
        // Over the surface, on it, and on the bed 186 under it (the recording's grenade).
        assert_eq!(water_thrown_up([0.0, 0.0, 140.0], Some(100.0), Some(-200.0), r), Some(100.0));
        assert_eq!(water_thrown_up([0.0, 0.0, 100.0], Some(100.0), None, r), Some(100.0));
        assert_eq!(water_thrown_up([0.0, 0.0, -86.0], Some(100.0), Some(-90.0), r), Some(100.0));
        // Too high over it, too deep, no water, or on a bank above the surface.
        assert_eq!(water_thrown_up([0.0, 0.0, 100.0 + r + 1.0], Some(100.0), None, r), None);
        assert_eq!(water_thrown_up([0.0, 0.0, 100.0 - r - 1.0], Some(100.0), None, r), None);
        assert_eq!(water_thrown_up([0.0, 0.0, 120.0], None, Some(0.0), r), None);
        assert_eq!(water_thrown_up([0.0, 0.0, 130.0], Some(100.0), Some(110.0), r), None);
    }

    /// A game over a plugin with a launcher and a rifle, an explosives list, and the
    /// perks of a blast as the masters' entries have them: the damage perk's condition on
    /// the Weapon tab, Hit the Deck's on the attacker's weapon's, Toughness none.
    mod blast_perks {
        use super::*;
        use fnv_esm::raw::{write_subrecords, Subrecord};
        use fnv_esm::{LoadOrder, Plugin, Schema};
        use fnv_game::perks as p;
        use fnv_game::{Game, GameConfig, GameData};
        use fnv_sim::{FormStore, RefTable, SimConfig, World};

        const CELL: u32 = 0x200;
        const NPC: u32 = 0x400;
        const NPC_REF: u32 = 0x500;
        const LAUNCHER: u32 = 0x600;
        const RIFLE: u32 = 0x601;
        const EXPLOSIVES: u32 = 0x650;
        const DEMOLITION: u32 = 0x700;
        const DECK: u32 = 0x701;
        const TOUGHNESS: u32 = 0x702;
        const IS_IN_LIST: u16 = 372;

        fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
            Subrecord { sig: *sig, data, extended: false }
        }

        fn z(s: &str) -> Vec<u8> {
            [s.as_bytes(), &[0]].concat()
        }

        fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
            let payload = write_subrecords(subs);
            [&sig[..], &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
        }

        fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
            [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
        }

        /// IsInList Explosives == 1.
        fn in_explosives() -> Subrecord {
            let d = [&[0u8, 0, 0, 0][..], &1f32.to_le_bytes(), &IS_IN_LIST.to_le_bytes(), &[0, 0], &EXPLOSIVES.to_le_bytes(), &[0; 12]].concat();
            sub(b"CTDA", d)
        }

        /// A perk of one entry: entry point, function, number, the tab of the condition.
        fn perk(fid: u32, edid: &str, point: u8, function: u8, value: f32, tab: Option<u8>) -> Vec<u8> {
            let mut s = vec![sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"DATA", vec![0, 1, 1, 1, 0]), sub(b"PRKE", vec![2, 0, 0]), sub(b"DATA", vec![point, function, 1])];
            if let Some(tab) = tab {
                s.push(sub(b"PRKC", vec![tab]));
                s.push(in_explosives());
            }
            s.push(sub(b"EPFT", vec![1]));
            s.push(sub(b"EPFD", value.to_le_bytes().to_vec()));
            s.push(sub(b"PRKF", vec![]));
            record(b"PERK", 0, fid, &s)
        }

        fn plugin_bytes() -> Vec<u8> {
            let top = |sig: &[u8; 4], body: &[u8]| group(*sig, 0, body);
            let perks = [
                perk(DEMOLITION, "DemolitionExpert", p::CALCULATE_WEAPON_DAMAGE, p::MULTIPLY_VALUE, 1.2, Some(1)),
                perk(DECK, "HitTheDeck", p::MODIFY_DAMAGE_THRESHOLD_DEFENDER, p::ADD_VALUE, 25.0, Some(2)),
                perk(TOUGHNESS, "Toughness", p::MODIFY_DAMAGE_THRESHOLD_DEFENDER, p::ADD_VALUE, 6.0, None),
            ]
            .concat();
            let npcs = [record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), record(b"NPC_", 0, NPC, &[sub(b"EDID", z("Grenadier"))])].concat();
            let weapons = [record(b"WEAP", 0, LAUNCHER, &[sub(b"EDID", z("Launcher"))]), record(b"WEAP", 0, RIFLE, &[sub(b"EDID", z("Rifle"))])].concat();
            let list = record(b"FLST", 0, EXPLOSIVES, &[sub(b"EDID", z("Explosives")), sub(b"LNAM", LAUNCHER.to_le_bytes().to_vec())]);
            let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
            let achr = record(b"ACHR", 0, NPC_REF, &[sub(b"NAME", NPC.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
            let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &achr));
            let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
            [
                record(b"TES4", 1, 0, &[sub(b"HEDR", hedr)]),
                top(b"PERK", &perks),
                top(b"NPC_", &npcs),
                top(b"WEAP", &weapons),
                top(b"FLST", &list),
                top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
            ]
            .concat()
        }

        struct Fixture {
            data: GameData,
            forms: FormStore,
            refs: RefTable,
        }

        fn fixture() -> Fixture {
            let plugins = vec![Plugin::parse(&plugin_bytes(), "BlastPerks.esm".into()).unwrap()];
            let lo = LoadOrder::new(&plugins).unwrap();
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
            let schema = Schema::load_dir(root.join("fnv")).unwrap();
            let functions = root.join("script/functions.json");
            let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
            let (forms, refs) = (FormStore::build(&lo, &schema), RefTable::build(&lo, &schema));
            let data = GameData::build(&lo, &schema, fnv_script::FunctionTable::load(&functions).unwrap(), &json, &refs);
            Fixture { data, forms, refs }
        }

        fn game<'a>(f: &'a Fixture, perks: &[u32]) -> Game<'a> {
            let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..GameConfig::default() });
            g.new_game();
            g.player_entered(Some(FormId(CELL)));
            for &perk in perks {
                g.player.perks.insert(FormId(perk), 1);
            }
            g
        }

        /// A blast of 100 on `target`, in no armour, with the perks its cause and target hold.
        fn blast_on(g: &mut Game<'_>, cause: Option<Cause>, target: FormId) -> damage::BlastHit {
            let (a, t) = blast_entry_ops(g, cause, target);
            damage::blast_hit(&Rules::defaults(), &CombatTuning::default(), 100.0, 0.0, 0.0, &a, &t)
        }

        fn close(a: f32, b: f32) -> bool {
            (a - b).abs() < 1e-3
        }

        #[test]
        fn the_causes_weapon_decides_the_conditioned_entries() {
            let f = fixture();
            let mut g = game(&f, &[DEMOLITION, DECK]);
            let (player, npc) = (FormId::PLAYER_REF, FormId(NPC_REF));
            let mine = |weapon: u32| Some(Cause { actor: player, weapon: FormId(weapon) });
            // The player's grenade on an NPC: the damage perk alone.
            assert!(close(blast_on(&mut g, mine(LAUNCHER), npc).dealt, 120.0));
            // On the player too: 100 x 1.2 - 25, each entry once.
            assert!(close(blast_on(&mut g, mine(LAUNCHER), player).dealt, 95.0));
            // A blast with another weapon behind it (a Meltdown's) answers neither condition.
            assert!(close(blast_on(&mut g, mine(RIFLE), player).dealt, 100.0));
            // An NPC's grenade: Hit the Deck runs on its weapon; the damage perk is the player's.
            assert!(close(blast_on(&mut g, Some(Cause { actor: npc, weapon: FormId(LAUNCHER) }), player).dealt, 75.0));
            // Nobody on either side holds perks.
            let (a, t) = blast_entry_ops(&mut g, Some(Cause { actor: npc, weapon: FormId(LAUNCHER) }), npc);
            assert!(a.is_empty() && t.is_empty());
        }

        /// A placed mine nobody put down: Toughness applies, Hit the Deck has no weapon to
        /// look at, and the player's weapon does not stand in for one.
        #[test]
        fn a_blast_with_no_cause_has_only_the_unconditioned_defence() {
            let f = fixture();
            let mut g = game(&f, &[DEMOLITION, DECK, TOUGHNESS]);
            let (a, t) = blast_entry_ops(&mut g, None, FormId::PLAYER_REF);
            assert!(a.is_empty(), "no attacker's perks");
            assert_eq!(t.names(), ["Toughness"]);
            assert!(close(blast_on(&mut g, None, FormId::PLAYER_REF).dealt, 94.0));
        }

        /// A grenade that strikes an actor deals its weapon's hit and its explosion's blast;
        /// the damage perk multiplies each of them once.
        #[test]
        fn a_direct_hit_and_its_blast_are_each_modified_once() {
            let f = fixture();
            let mut g = game(&f, &[DEMOLITION]);
            let (player, npc) = (FormId::PLAYER_REF, FormId(NPC_REF));
            let (launcher, target) = (FormId(LAUNCHER), Some(npc));
            let w = WeaponStats { base_damage: 40, anim_type: Some("Grenade Launcher".into()), ..WeaponStats::default() };
            let h = HitInput { weapon: &w, projectiles: 1, condition: 1.0, ammo_effects: &[], part: HitPart::default(), power: 1.0 };
            let ap = entry_ops(&mut g, player, &[p::CALCULATE_WEAPON_DAMAGE], &[Some(launcher), target], target);
            let direct = damage::hit(&Rules::defaults(), &CombatTuning::default(), &h, &Attacker::default(), &Target::default(), &ap, &NoPerks, 1.0);
            let blasted = blast_on(&mut g, Some(Cause { actor: player, weapon: launcher }), npc);
            assert!(direct.base > 0.0 && close(direct.modified, direct.base * 1.2), "{direct:?}");
            assert!(close(blasted.base, 100.0) && close(blasted.modified, 120.0), "{blasted:?}");
        }
    }
}
