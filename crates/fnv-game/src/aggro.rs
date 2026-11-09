//! The aggro radius: an actor that does not attack the player on sight warns a player
//! who comes too close and then attacks (docs/design/combat-ai.md §13; owner's playtest
//! list 4, item 18).
//!
//! - **The records** [documented: xEdit AIDT field names "Aggro Radius Behavior" and
//!   "Aggro Radius"]: the AI data (from the template while the template flag "AI Data"
//!   is set) turns the behaviour on and gives the radius in game units. The current
//!   package must allow it: "Aggro Radius Behavior: Allow aggro radius behavior (if any)"
//!   is one of the package's Fallout behaviour flags [documented: GECK wiki
//!   Category:Packages], read as `fnv_ai::social::allows` reads them (a package without
//!   "Skip Fallout Behavior" allows them all); an actor with no package allows it
//!   [hypothesis]; a package with "No Warn/Attack Behavior" does not [hypothesis: the
//!   flag's name].
//! - **The warning** [documented: GECK wiki Combat Tab, GuardTrespass "Said when issuing a
//!   warning, either from a Guard package, Aggro Radius behavior, or when the player is
//!   trespassing"; GetTrespassWarningLevel counts the warnings "an actor on a Guard
//!   Package (or responding to trespassing or responding to an aggro radius violation)"
//!   gave]: while it has the player detected (band Detected, measured within the last
//!   second) inside the radius, it says GuardTrespass every fAITrespassWarningTimer
//!   (5 s) up to iGuardWarnings (2) times, then attacks: a grudge ([`Why::Aggro`]) makes
//!   `Game::would_attack` true for the pair [hypothesis: the guard package's rule,
//!   `crate::crime`'s for trespassing; creatures have no GuardTrespass lines and warn
//!   in silence]. The count clears once the actor has gone twice the warning timer
//!   (10 s) without warning, i.e. after leaving the radius or losing the player for
//!   that long; a short loss of detection while it wanders keeps the count [decision,
//!   2026-10-13: clearing it at once let wandering geckos warn forever and never attack].
//!   An actor that has started warning also keeps counting while the player is only in
//!   its alert band (Noticed: it knows where the player is but does not see them, e.g.
//!   walking past with its back turned) [decision, same reason].
//! - **The look** [hypothesis: the idle tree; owner's playtest 2026-10-24, "baby geckos
//!   were not showing the aggressive stay away from me warnings"]: while it warns (from
//!   its first warning until it attacks or the player is out of its radius or unwatched
//!   at an aggro tick) the actor draws (`Game::draws_weapon`): a person its weapon, a
//!   creature its unarmed stance; the AI stops it and turns it to the player
//!   (`fnv_ai::alert`); GetCurrentAIPackage reads 35 and GetCurrentAIProcedure 44
//!   (GUARD). It is not alerted: GetIsAlerted stays 0, so the gecko's combat idles
//!   (NVGeckoCombatIdles 0x0011D9E9: IsMoving != 1 and GetIsAlerted 1 or the combat or
//!   flee procedure) wait for the fight [observed, original game, capture `warnings`
//!   young_gecko 2026-10-24: Young Gecko VERGSGecko6REF stopped and turned to the player
//!   (within 1 degree) in the sample after the player came inside 500 units; package a
//!   form made at run time, type 35, procedure 44; GetAnimAction 0 "equip weapon",
//!   IsWeaponOut 0 and GetIsAlerted 0 until it fought, then IsWeaponOut 1 and procedure
//!   13]. Without the look a creature warned in silence while it walked on, and the
//!   player saw nothing before the attack.
//! - **The timing** [observed, original game, captures `warnings` and `warnings2`
//!   2026-10-24/25, TimeScale 30, the player standing still]: a creature attacks 15 s
//!   after its warning began (Young Gecko Hunters 15.0 / 16.7 s at 607-618 of 650; a
//!   Young Gecko 15 s at 387-393 of 500; the coyote VERGSCoyote6REF 15 s at 727 of 1024),
//!   a person 21-22 s (Gomorrah's cash-room guard, four runs at 66-90 of his 128):
//!   [`CREATURE_ATTACK_SECONDS`], [`PERSON_ATTACK_SECONDS`] from the first warning, the
//!   second warning one timer after the first [hypothesis: the split; setgs of
//!   fAITrespassWarningTimer and iGuardWarnings showed no difference in `warnings2`,
//!   whose start distances varied too much to tell]. Inside half the aggro radius
//!   ([`AGGRO_ATTACK_SHARE`]) the attack comes at once [observed: the young gecko at
//!   196 and 217 of 500 at once; the guard at 32 and 62 of 128 after about 3 s]. Open: the
//!   cash-room guard warned at 98-121 units (inside his 128) and never attacked in 68 s,
//!   while the geckos at 0.78 of their radius did; not built.
//! - **Spawned actors do not warn** [observed, original game, capture `o25_coyote`
//!   2026-11-06, docs/handoff/observer-checks.md item 2, #1341]: a CrCoyote PlaceAtMe'd at
//!   the spot of the placed VERGSCoyote6REF, the player 700 units off (inside its 1,024),
//!   was in combat at the first sample with no warning, where the placed coyote warned for
//!   14.9 s first; the base has the same aggro radius and packages. Spawned coyotes on the
//!   airport runway (o16_air) did the same at 250 and 700 units. So an actor created at
//!   run time (a dynamic reference, [`FormId::is_dynamic`]) inside its aggro radius attacks
//!   at once, as one inside half of it does; the other rules (who, the package, the
//!   radius) stay [hypothesis: why: a PlaceAtMe'd actor has no editor location for the
//!   radius to be kept around, or it starts alerted; the rule covers every created actor,
//!   people too, which no capture tried].
//! - **Guard packages do not warn** [observed: `warnings2` g_room*, Gomorrah's room
//!   guards on vGomorrahGuardPackage, with no aggro radius of their own, neither warned
//!   nor fought a player 15 units from the door each guards or 60 units from them;
//!   their package allows warnings (no "No Warn/Attack Behavior")]. The wiki's Guard
//!   package summary ("warn intruders near the guarded reference", packages.md section 3)
//!   is not built: a guard's GuardTrespass lines come with an aggro radius, as the
//!   cash-room guard's did. Built on 2026-10-24, taken out on 2026-10-25.
//! - **Who**: the player only [design: other actors' aggro radii are not measured];
//!   never an actor that is the player's ally or friend or teammate, one already hostile
//!   by its records, a dead, unconscious or restrained one.
//!
//! Example (FalloutNV.esm): the "Young Gecko Hunter" geckos of GS02
//! (VSpawnSpecialTier1GeckoSmallGS02Hunter 0x00165183) are Unaggressive, neutral to the
//! player, with Aggro Radius Behavior on and a radius of 650 [observed: `fnvtool esm
//! decode`]: they ignore the player at a distance and attack within 650 units.

use crate::detection::Band;
use crate::game::Game;
use crate::package::{fallout, flags};
use crate::provocation::{Grudge, Note, Why, ALLY, FRIEND};
use fnv_sim::FormId;

/// After an aggro attack begins, how long the actor still counts as warning while it is
/// not yet in Combat ([`Game::warning_to`]), seconds [design].
pub const AGGRO_TURN_SECONDS: f64 = 2.0;

/// A creature attacks this long after its first warning (module notes, "The timing").
pub const CREATURE_ATTACK_SECONDS: f64 = 15.0;

/// A person attacks this long after its first warning (module notes, "The timing").
pub const PERSON_ATTACK_SECONDS: f64 = 21.0;

/// Inside this share of its aggro radius an actor attacks without warning (module notes,
/// "The timing").
pub const AGGRO_ATTACK_SHARE: f64 = 0.5;

impl Game<'_> {
    /// `actor`'s aggro radius in game units when its AI data turns the behaviour on
    /// (LNSetAggroRadius's radius first, `nvse_lutana`).
    pub fn aggro_radius(&self, actor: FormId) -> Option<f64> {
        let b = crate::functions::base_of(self, actor)?;
        let (_, d) = self.template_source(b, actor, |d| d.template_ai)?;
        // LNSetAggroRadius changes the NPC record's radius (`nvse_lutana::ai_record`).
        let r = self.other.lutana.aggro_radius.get(&b).copied().unwrap_or(d.aggro_radius);
        (d.aggro_radius_behavior && r > 0).then_some(r as f64)
    }

    /// Whether `actor`'s current package allows aggro radius behaviour (module notes).
    fn package_allows_aggro(&mut self, actor: FormId) -> bool {
        match self.current_package(actor).and_then(|p| self.package_def(&p)) {
            // "No Warn/Attack Behavior" turns the warnings and the attack off [hypothesis:
            // the flag's name, xEdit; no page describes it].
            Some(d) if d.has(flags::NO_WARN_ATTACK_BEHAVIOR) => false,
            Some(d) => !d.has(flags::SKIP_FALLOUT_BEHAVIOR) || d.fallout_flags & fallout::AGGRO_RADIUS != 0,
            None => true,
        }
    }

    /// After a round of detection measurements: aggro radius warnings and attacks (module
    /// notes). Returns how many actors turned on the player.
    pub fn aggro_tick(&mut self) -> usize {
        let s = self.crime_settings();
        let now = self.time;
        let Some(player) = self.world.peek(FormId::PLAYER_REF).map(|p| p.pos) else { return 0 };
        let watchers: Vec<FormId> = self
            .detection
            .pairs
            .iter()
            .filter(|((w, t), (d, at))| {
                // One that has started warning keeps counting while it has the player in
                // the alert band (module notes).
                let warning = self.provocations.aggro_warnings.contains_key(w);
                *t == FormId::PLAYER_REF && *w != FormId::PLAYER_REF && (d.band == Band::Detected || (warning && d.band == Band::Noticed)) && now - at <= 1.0
            })
            .map(|((w, _), _)| *w)
            .collect();
        // Actors no longer watching the player inside their radius start counting afresh
        // after a grace of two warning timers (module notes).
        let mut inside = Vec::new();
        let mut turned = 0;
        let at = |g: &Game, r: FormId| g.world.peek(r).map(|p| p.pos).or_else(|| g.world.placed.get(r).map(|p| p.pos));
        let gap = |a: [f32; 3]| (0..3).map(|i| (a[i] - player[i]).powi(2)).sum::<f32>().sqrt() as f64;
        for w in watchers {
            let out = |g: &Game, r: FormId| g.extra.get(&r).is_some_and(|e| e.dead || e.out_cold() || e.restrained || e.teammate);
            if out(self, w) || !self.present(w) || self.provocations.holds(w, FormId::PLAYER_REF) {
                continue;
            }
            let Some(radius) = self.aggro_radius(w) else { continue };
            let Some(distance) = at(self, w).map(gap) else { continue };
            if distance > radius || matches!(self.reaction_to(w, FormId::PLAYER_REF), ALLY | FRIEND) || !self.package_allows_aggro(w) {
                continue;
            }
            if self.would_attack(w, FormId::PLAYER_REF) {
                continue;
            }
            inside.push(w);
            let (level, last) = self.provocations.aggro_warnings.get(&w).copied().unwrap_or((0, f64::NEG_INFINITY));
            // Inside a share of its aggro radius it attacks at once (module notes, "The
            // timing").
            // So does one created at run time (module notes, "Spawned actors do not warn").
            let close = distance <= radius * AGGRO_ATTACK_SHARE || w.is_dynamic();
            let warned_out = (level as f64) >= s.guard_warnings;
            // The attack comes so long after the first warning: the time left after the
            // warnings already given.
            let attack_after = if self.creature_type(w).is_some() { CREATURE_ATTACK_SECONDS } else { PERSON_ATTACK_SECONDS };
            let due = if warned_out { (attack_after - (s.guard_warnings - 1.0).max(0.0) * s.warning_timer).max(s.warning_timer) } else { s.warning_timer };
            if !close && now - last < due {
                continue;
            }
            if !close && !warned_out {
                self.provocations.aggro_warnings.insert(w, (level + 1, now));
                self.detection_bark(w, Some(FormId::PLAYER_REF), "GuardTrespass");
                self.provocations.notes.push(Note::AggroWarned { actor: w, level: level + 1, distance, radius });
            } else {
                self.provocations.grudges.insert((w, FormId::PLAYER_REF), Grudge { why: Why::Aggro, since: now });
                self.provocations.notes.push(Note::AggroAttacks { actor: w, distance, radius });
                turned += 1;
            }
        }
        let grace = 2.0 * s.warning_timer;
        self.provocations.aggro_warnings.retain(|w, (_, last)| inside.contains(w) || now - *last < grace);
        // One that turned on the player fights now; it no longer warns.
        self.provocations.aggro_inside = inside.into_iter().filter(|w| !self.provocations.holds(*w, FormId::PLAYER_REF)).collect();
        turned
    }

    /// Whom `actor` is warning now (module notes, "The look"): the player while it stands
    /// inside its aggro radius between its first warning and its attack, or while it
    /// sees the player trespass after warning it (`crate::crime`) [hypothesis: the same
    /// look; the owner, 2026-10-24: "guards that are alerted/warning are supposed to draw
    /// their weapon"].
    pub fn warning_to(&self, actor: FormId) -> Option<FormId> {
        // Just turned on the player and not yet in Combat (detection settles a moment
        // later): still the warning, so the stance is not put away and drawn again
        // [design; observed in NVForever: 0.23 s, smoke `aggro-warning-look`].
        let turning = self.provocations.grudges.get(&(actor, FormId::PLAYER_REF)).is_some_and(|g| g.why == Why::Aggro && self.time - g.since < AGGRO_TURN_SECONDS) && !self.awareness.state(actor).in_combat();
        (self.provocations.aggro_inside.contains(&actor) || turning || self.crimes.warning_now.contains(&actor)).then_some(FormId::PLAYER_REF)
    }
}
