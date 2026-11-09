//! Breath and drowning in play (docs/spec-notes/drowning.md): the player's head under
//! water uses up the breath, and with none left the player drowns. The rule is
//! `fnv_game::breath`; the meter is `crate::hud_breath`.
//!
//! - **Under water**: the player's eye (`Player::eye`: the drawn body's, else
//!   `eye_height` above the feet) below the surface of the water it is in
//!   (`PhysWorld::water_surface_for`), swimming or walking the bed [observed: `sw_walkin`
//!   drowned a player walking the bed, physics.md §6.6; hypothesis: the eye as the
//!   head].
//! - **Time** runs while the game does: not in menus, dialogue or loading.
//! - **Rule** (`Game::breath_rules`): the breath by Endurance, a fifth of the maximum
//!   health a second without it, the breath whole again at once above the water.
//! - **Damage**: `Game::hit_without_lines`, a hit with no attacker and no body part
//!   (god mode spares the player), with none of a blow's feedback: no Hit lines ("Ahh!"),
//!   no Death line when it kills, no flinch, no blood on the screen [design: owner
//!   2026-10-25, "should not hear the player hit sounds", "remove the death sound
//!   effect from drowning to death"].
//! - **Sound**: while drowning, the game's `NPCHumanDrowning` (a take at random from
//!   `fx\npc\human\drowning\`, 1.36–1.66 s long) as the breath runs out and then every
//!   [`GASP_SECONDS`] [design: owner 2026-10-25, "specific drowning sound effects";
//!   hypothesis: the original's rhythm is not measured].
//! - **Who**: the player only [hypothesis: no NPC was seen drowning; the masters give
//!   their water creatures Water Breathing, which suggests the rule reaches actors, but
//!   NPCs' paths keep out of deep water, so they are spared until a capture says].
//! - Logged: `drowning: head under water …`, `drowning: out of breath …`,
//!   `drowning: head above water …`, `drowning: <n> damage …` and
//!   `drowning: sound NPCHumanDrowning`.

use crate::activate::HudMessage;
use crate::combat::{CombatStats, HitFeedback};
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sfx::{PlaySound, Sfx};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_game::breath::{Breath, BreathRules};
use fnv_sim::FormId;

/// The damage is dealt in hits of this many seconds' worth [design: the original takes
/// the health away every frame (`dr_short`: 0.64 a game loop of 16 ms), a hit a frame
/// would log and flash a hit each frame; a quarter of a second looks as smooth].
pub const HIT_SECONDS: f32 = 0.25;

/// Seconds between the drowning sound's takes: the longest take and a little [design].
pub const GASP_SECONDS: f32 = 1.7;
/// The drowning sound [observed: FalloutNV.esm SOUN 0x0004D23F, the folder
/// `fx\npc\human\drowning\` of three takes].
pub const GASP: &str = "NPCHumanDrowning";

/// The rule until the game is read: an Endurance of 5 and 200 health.
pub const RULES: BreathRules = BreathRules { seconds: 10.0, damage: 40.0, recover: f32::INFINITY };

/// The player's breath, for the meter.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct PlayerBreath {
    pub breath: Breath,
    pub rules: BreathRules,
    /// Damage owed, not yet dealt as a hit.
    pub owed: f32,
    /// Seconds until the next drowning sound.
    pub gasp_in: f32,
}

impl Default for PlayerBreath {
    fn default() -> Self {
        PlayerBreath { breath: Breath::full(&RULES), rules: RULES, owed: 0.0, gasp_in: 0.0 }
    }
}

impl PlayerBreath {
    /// The breath left, 0 to 1.
    pub fn share(&self) -> f32 {
        self.breath.share(&self.rules)
    }

    pub fn under(&self) -> bool {
        self.breath.under
    }
}

pub struct DrowningPlugin;

impl Plugin for DrowningPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerBreath>().add_systems(
            Update,
            breathe.in_set(PlaySet::Activation).run_if(resource_exists::<SimState>).run_if(resource_exists::<CombatStats>).run_if(resource_exists::<HitFeedback>),
        );
    }
}

/// One frame of the player's breath.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn breathe(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    player: Option<Res<Player>>,
    phys: Option<Res<PhysState>>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    mut stats: ResMut<CombatStats>,
    mut hud: Option<ResMut<HudMessage>>,
    mut pb: ResMut<PlayerBreath>,
    (sfx, mut sounds, mut gasp): (Option<Res<Sfx>>, MessageWriter<PlaySound>, Local<Option<Option<FormId>>>),
) {
    let (Some(player), Some(phys)) = (player, phys) else { return };
    if *state.get() != PlayState::Playing || script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.in_menu()) {
        return;
    }
    // A dead player drowns no more.
    if script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.is_dead(FormId::PLAYER_REF)) {
        return;
    }
    let eye = player.eye();
    let surface = phys.world.water_surface_for(eye);
    let under = surface.is_some_and(|s| eye[2] < s);
    let pb = &mut *pb;
    let mut breathes = false;
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        (pb.rules, breathes) = with_world(rt, &mut sim, |rt| (rt.game.breath_rules(FormId::PLAYER_REF), under && rt.game.breathes_water(FormId::PLAYER_REF)));
    }
    let (was_under, had) = (pb.breath.under, pb.breath.left);
    let dmg = pb.breath.step(&pb.rules, under, breathes, time.delta_secs());
    // GetActorDiveBreath and SetActorDiveBreath (`fnv_game::nvse_jip11`).
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        if let Some(left) = rt.game.sync_player_breath(pb.breath.left, under) {
            pb.breath.left = left.min(pb.rules.seconds);
        }
    }
    if under && !was_under {
        info!(
            "drowning: head under water ({:.1} below the surface), breath {:.1} of {:.1} s, {:.1} health a second without{}",
            surface.unwrap_or(eye[2]) - eye[2],
            pb.breath.left,
            pb.rules.seconds,
            pb.rules.damage,
            if breathes { ", breathes water" } else { "" }
        );
    } else if !under && was_under {
        info!("drowning: head above water, breath {had:.1} s");
        pb.owed = 0.0;
    }
    if had > 0.0 && pb.breath.left <= 0.0 && under {
        info!("drowning: out of breath, drowning ({:.0} health a second)", pb.rules.damage);
    }
    pb.owed += dmg;
    // The drowning sound while the breath is gone.
    if dmg > 0.0 {
        pb.gasp_in -= time.delta_secs();
        if pb.gasp_in <= 0.0 {
            pb.gasp_in = GASP_SECONDS;
            if let Some(sfx) = sfx.as_deref() {
                let form = *gasp.get_or_insert_with(|| sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(GASP))).map(|(f, _)| *f));
                match form {
                    Some(sound) => {
                        info!("drowning: sound {GASP}");
                        sounds.write(PlaySound { sound, at: None });
                    }
                    None => warn!("drowning: no sound {GASP} in the plugins"),
                }
            }
        }
    } else {
        pb.gasp_in = 0.0;
    }
    let hit = pb.rules.damage * HIT_SECONDS;
    while pb.owed >= hit && hit > 0.0 {
        pb.owed -= hit;
        let info = fnv_game::combat::HitInfo { attacker: None, weapon: None, location: -1, critical: false, limb: None };
        let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { break };
        if crate::combat::god_mode() {
            info!("drowning: {hit:.1} damage (immune: god mode)");
            continue;
        }
        let r = with_world(rt, &mut sim, |rt| rt.game.hit_without_lines(FormId::PLAYER_REF, hit as f64, info));
        stats.player_damage += r.dealt as f32;
        info!("drowning: {hit:.1} damage, health {:.0}/{:.0}{}", r.health, r.max_health, if r.immune { " (immune)" } else if r.died { ", dead" } else { "" });
        if r.died {
            stats.player_died = true;
            if let Some(h) = hud.as_deref_mut() {
                h.show("You are dead.".to_string());
            }
        }
    }
}
