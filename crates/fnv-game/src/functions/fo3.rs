//! Vanilla functions only Fallout 3's scripts call: registered in the catalogue (GECK1,
//! opcode recorded), never called by the New Vegas masters, first run by Tale of Two
//! Wastelands (docs/handoff/ttw-fo3-script-audit-2026-11-02.md §1). Behaviour from the
//! catalogue (`summary`/`notes`) unless tagged otherwise.

use super::{actor, none, yes, Args, Ctx};
use crate::game::Game;
use crate::package::PackageType;
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::FormId;

// ---- Actors -----------------------------------------------------------------------------

/// `actor.ModDisposition target change`: the caller's disposition towards `target`
/// changes by `change` [documented: catalogue summary]. The change starts from what
/// GetDisposition reads now and is kept as SetDisposition keeps its value; the result
/// stays within 0–100, GetDisposition's range [hypothesis].
pub fn mod_disposition(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let me = ctx.subject()?;
    let target = a.form(0)?;
    let change = a.num(1)?;
    let now = actor::get_disposition(g, ctx, &Args(&[ArgValue::Value(Value::Ref(target.0))]))?.num();
    g.extra.entry(me).or_default().dispositions.insert(target, (now + change).clamp(0.0, 100.0));
    none()
}

/// `GetPCIsRace race`: the player's race is `race`, as GetIsRace on the player
/// [documented: catalogue summary, "the player-only version of GetIsRace"].
pub fn get_pc_is_race(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(g.actor_race(FormId::PLAYER_REF) == Some(a.form(0)?))
}

/// `actor.GetForceSneak`: SetForceSneak's flag on the caller [documented: catalogue
/// summary].
pub fn get_force_sneak(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.extra.get(&r).is_some_and(|e| e.force_sneak))
}

/// `actor.SetMerchantContainer container`: the caller's goods are in `container` from
/// now on, in place of its reference's merchant container (XMRC), for the barter menu
/// and for actors that buy from it ([`Game::merchant_container`]). Refused for a
/// container that is not persistent [documented: catalogue notes].
pub fn set_merchant_container(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let me = ctx.subject()?;
    let c = a.form(0)?;
    if !g.world.placed.get(c).is_some_and(|p| p.in_persistent_group) {
        return Err(format!("{} is not a persistent reference: SetMerchantContainer refused", g.data.name(c)));
    }
    g.extra.entry(me).or_default().merchant_container = Some(c);
    none()
}

/// `actor.MatchRace source`: the caller takes the race `source` has now, drawn again
/// with the new race's default face textures [documented: catalogue summary and notes]
/// (`crate::appearance`).
pub fn match_race(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let me = ctx.subject()?;
    let race = g.actor_race(a.form(0)?).ok_or("the source actor has no race")?;
    g.change_race(me, race);
    none()
}

/// `ClearActorsFactionsPlayerEnemyFlag actor`: the player is no longer an enemy of any
/// crime-tracking faction `actor` belongs to [documented: catalogue summary], as
/// ClearFactionPlayerEnemyFlag on each.
pub fn clear_actors_factions_player_enemy_flag(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = a.form(0)?;
    let tracks = |g: &Game, f: FormId| g.data.faction_flags.get(&f).is_some_and(|x| x & crate::crime::faction_flags::TRACK_CRIME != 0);
    for f in actor::factions_of(g, r) {
        if tracks(g, f) {
            g.player.enemy_of.remove(&f);
        }
    }
    actor::forget_relations(g);
    none()
}

// ---- The player -------------------------------------------------------------------------

/// `GetPCSleepHours`: the hours of the sleep the player chose in the menu, 0 while
/// awake [documented: catalogue summary] (`crate::rest`).
pub fn get_pc_sleep_hours(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(g.rest.sleep_hours() as f64))
}

/// `SetPCYoung flag`: the player counts as young (Fallout 3's childhood in Vault 101):
/// only the race's default hair, no face attachments [documented: catalogue summary].
/// Kept for the engine to draw by ([`crate::game::PlayerState::young`]).
pub fn set_pc_young(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.young = a.num_or(0, 1.0) != 0.0;
    none()
}

/// `SetPCToddler flag`: the player moves with the toddler's walk animations (1) or
/// the normal ones (0) [documented: catalogue summary]. Kept for the engine
/// ([`crate::game::PlayerState::toddler`]).
pub fn set_pc_toddler(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.toddler = a.num_or(0, 1.0) != 0.0;
    none()
}

/// `SetNoActivationSound sound`: the sound played when an activation is refused is
/// `sound` (an editor ID) in place of the default [documented: catalogue summary]. Kept
/// ([`crate::game::PlayerState::no_activation_sound`]).
pub fn set_no_activation_sound(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let sound = match a.peek_str(0) {
        Some(s) => g.data.by_edid(s).ok_or_else(|| format!("no form has the editor ID {s}"))?,
        None => a.form(0)?,
    };
    g.player.no_activation_sound = Some(sound);
    none()
}

/// `ClearNoActivationSound`: the default sound again [documented: catalogue summary].
pub fn clear_no_activation_sound(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.player.no_activation_sound = None;
    none()
}

/// `SetLocationSpecificLoadScreensOnly flag`: with 1, a load screen shows only when one
/// is tied to the place being loaded [documented: catalogue summary]. Kept for the
/// engine's loading screen ([`crate::game::PlayerState::location_load_screens_only`]).
pub fn set_location_specific_load_screens_only(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.location_load_screens_only = a.num(0)? != 0.0;
    none()
}

/// `SetLastExtDoorActivated door`: `door` counts as the last exterior door the player
/// used, which chooses the worldspace the Pip-Boy's map shows [documented: catalogue
/// summary and notes]; GetPCLastExteriorDoor reads it (`crate::nvse_jip3`).
pub fn set_last_ext_door_activated(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.gaps.jip3.last_exterior_door = Some(a.form(0)?);
    none()
}

/// `actor.MatchFaceGeometry source percent`: the caller's face geometry moves `percent`
/// (0–100) of the way to `source`'s (`crate::appearance`, module notes) [documented:
/// GECK wiki MatchFaceGeometry].
pub fn match_face_geometry(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let me = ctx.subject()?;
    let source = a.form(0)?;
    g.match_face_geometry(me, source, a.num(1)? as f32)?;
    none()
}

/// `actor.Wait package`: the caller waits, standing where it is, while `package` is its
/// current package and a Follow or Accompany one; otherwise nothing happens [documented:
/// GECK wiki Wait, "Works only if PackageID is the actor's current package, and if the
/// current package is a Follow or Accompany package"]. The AI's follow step stops while
/// [`Game::waits`] (`fnv_ai`).
pub fn wait(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let me = ctx.subject()?;
    let p = a.form(0)?;
    let follows = g.package_def(&p).is_some_and(|d| matches!(d.ty, PackageType::Follow | PackageType::Accompany));
    if follows && g.current_package(me) == Some(p) {
        g.extra.entry(me).or_default().waiting = Some(p);
    }
    none()
}

/// `actor.StopWaiting package`: the caller takes up `package` again after Wait
/// [documented: GECK wiki StopWaiting]. Another package named leaves the wait as it is
/// [hypothesis].
pub fn stop_waiting(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let me = ctx.subject()?;
    let p = a.form(0)?;
    if let Some(e) = g.extra.get_mut(&me).filter(|e| e.waiting == Some(p)) {
        e.waiting = None;
    }
    none()
}

/// `actor.IsWaiting`: 1 while the caller waits after Wait [documented: catalogue
/// summary].
pub fn is_waiting(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let me = ctx.subject()?;
    yes(g.waits(me))
}

// ---- Factions ---------------------------------------------------------------------------

/// `SetFactionReaction faction target value`: how `faction` regards `target` becomes
/// `value` [documented: GECK wiki SetFactionReaction], the disposition modifier between
/// the two [documented: GECK wiki GetFactionReaction, "the disposition modifier between
/// two factions"; xEdit FACT XNAM "Modifier"]. Kept (`FormChanges::faction_modifiers`,
/// saved); the modifiers are not part of a disposition yet (docs/spec-notes/factions.md),
/// so nothing reads it. The group combat reaction (SetEnemy, SetAlly) is not changed.
pub fn set_faction_reaction(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let (f, target) = (a.form(0)?, a.form(1)?);
    let v = a.num(2)?;
    g.forms.faction_modifiers.entry(f).or_default().insert(target, v as i64);
    none()
}

// ---- Cells and the world ----------------------------------------------------------------

/// Whether `cell` is an exterior cell (in a worldspace).
pub(crate) fn exterior(g: &Game, cell: FormId) -> bool {
    g.world.placed.cell_world.contains_key(&cell)
}

/// `SetCellFullName cell message`: the interior cell is called by the message's title
/// from now on, where load doors name it and on the local map [documented: catalogue
/// summary, "works only for interior cells"]. Kept as SetActorFullName keeps a base's
/// name (`FormChanges::names`, saved; [`Game::script_name`]); the door prompt reads it
/// (`nvf_play::activate`). Refused for an exterior cell.
pub fn set_cell_full_name(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let (cell, m) = (a.form(0)?, a.form(1)?);
    if exterior(g, cell) {
        return Err(format!("{} is an exterior cell: SetCellFullName refused", g.data.name(cell)));
    }
    if !g.data.messages.contains_key(&m) {
        return Err(format!("{} is not a message", g.data.name(m)));
    }
    g.forms.names.insert(cell, m);
    none()
}

/// `ResetInterior cell`: the interior cell is reset at its next load, its dead actors
/// left dead (`crate::respawn`) [documented: catalogue summary and notes]. Refused for an
/// exterior cell.
pub fn reset_interior(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let cell = a.form(0)?;
    if exterior(g, cell) {
        return Err(format!("{} is an exterior cell: ResetInterior refused", g.data.name(cell)));
    }
    g.forms.expired_cells.insert(cell);
    none()
}

/// `SetZoneRespawns zone flag`: the encounter zone's cells reset (1) or never (0), over
/// its "Never Resets" flag (`fnv_sim::World::set_zone_respawns`, saved) [documented:
/// catalogue summary].
pub fn set_zone_respawns(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let zone = a.form(0)?;
    g.world.set_zone_respawns(zone, a.num(1)? != 0.0);
    none()
}

/// `DisableAllMines`: every mine in the player's cell is disabled [documented: GECK wiki
/// DisableAllMines, "Disables all the mines located in the cell the player is in"]. The
/// engine holds the live mines and disables them (`nvf_play::mines`); a disabled one is
/// gone, as one that went off [hypothesis: "disable" read as the Disable function's, not
/// as disarming into a loose item]. The one caller is a Fallout 3 quest's scene in the
/// player's cell [observed: Fallout3.esm SCPT 0x0601F40A].
pub fn disable_all_mines(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.ui.mines_disabled += 1;
    none()
}

/// `RemapWaterType old new`: water of type `old` is drawn as `new` from now on
/// [documented: GECK wiki RemapWaterType, "Replaces all instances of one water type with
/// another"; its example cleans the Potomac's water, as Broken Steel does, observed:
/// BrokenSteel.esm QUST 0x09000802 stage 0]. Kept (`FormChanges::water_remaps`, saved;
/// [`Game::water_type`]); the engine draws water by it (`nvforever` water.rs). Remapping
/// a type to itself undoes its remap [hypothesis].
pub fn remap_water_type(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let (old, new) = (a.form(0)?, a.form(1)?);
    if old == new {
        g.forms.water_remaps.remove(&old);
    } else {
        g.forms.water_remaps.insert(old, new);
    }
    none()
}

/// `TriggerLODApocalypse` (no catalogue page; Fallout 3's MS11 stage 60, Megaton's
/// destruction, calls it between disabling the town and its map marker [observed:
/// Fallout3.esm QUST 0x06014E9E, the stage's source text against its compiled order]):
/// the distant view shows the destroyed town from now on [hypothesis: from the name and
/// the stage]. Kept (`FormChanges::lod_apocalypse`, saved); no LOD is drawn by it yet.
pub fn trigger_lod_apocalypse(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.forms.lod_apocalypse = true;
    none()
}

// ---- Screen and sound -------------------------------------------------------------------

/// `TriggerHitShader [strength]`: the screen blurs as when the player is staggered, at
/// `strength` [documented: GECK wiki TriggerHitShader]. Without an argument (The Pitt
/// calls it so, observed: ThePitt.esm SCPT 0x0800A740) the strength is 1 [hypothesis].
/// Kept for the engine (`UiRequests::hit_shaders`); nothing draws it yet.
pub fn trigger_hit_shader(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let s = Some(a.num_or(0, 1.0)).filter(|v| v.is_finite()).unwrap_or(1.0);
    g.ui.hit_shaders.push(s.max(0.0) as f32);
    none()
}

/// `TriggerScreenBlood count`: blood on the screen that fades slowly, more with a
/// larger count (1–100) [documented: catalogue summary and notes]. Drawn as New Vegas's
/// screen splatter, `count` splats ([`super::misc::trigger_screen_splatter`]) [hypothesis:
/// the same effect under Fallout 3's name].
pub fn trigger_screen_blood(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let n = Some(a.num_or(0, 1.0)).filter(|v| v.is_finite()).unwrap_or(1.0);
    let t = super::ui::SplatterTrigger { count: n.clamp(0.0, 100.0) as u32, duration: None, size: 1.0, opacity: 1.0 };
    g.ui.splatter.triggers.push(t);
    none()
}

/// `FadeSFX seconds`: every sound but music fades out over `seconds` [documented:
/// catalogue summary and notes; "it seems to come back only after a video plays"].
/// Kept for the engine (`UiRequests::sfx_fades`); nothing plays by it yet.
pub fn fade_sfx(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let s = Some(a.num_or(0, 0.0)).filter(|v| v.is_finite()).unwrap_or(0.0);
    g.ui.sfx_fades.push(s.max(0.0) as f32);
    none()
}

/// `KillQuestUpdates` (no catalogue entry; the wiki's page is empty): the quest lines
/// waiting to show are dropped [hypothesis: from the name and the one call, CG01 stage
/// 90, after `AutoDisplayObjectives 0` and before the fade to the next part of the
/// player's childhood, observed: Fallout3.esm QUST 0x06014E83]. The HUD drops them on
/// each new count (`UiRequests::quest_updates_killed`, `nvf_play::quest_messages`).
pub fn kill_quest_updates(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.ui.quest_updates_killed += 1;
    none()
}

/// `GetLastPlayerAction`: the index of the player's last action [documented: GECK wiki
/// GetLastPlayerAction], the same list as IsPlayerActionActive's. It read 4 while the
/// player threw a frag mine and 0 before and after [observed: AI observer campaign
/// `functions_round` 2026-10-19, `crate::anim_state::is_player_action_active`], so it
/// answers the action active now, 0 when none: 4 a mine's throw, 2 another throw, 9
/// iron sights out of combat [hypothesis for 2 and 9: IsPlayerActionActive's]. The Pitt
/// asks it for 3 when the player shoots (ThePitt.esm SCPT 0x08001076, its variable
/// `checkifplayershoots`); no capture has seen 3, so it is never answered (open: a
/// capture while firing).
pub fn get_last_player_action(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let code = match g.anim_state(FormId::PLAYER_REF) {
        Some(s) if s.throwing && s.mine => 4,
        Some(s) if s.throwing => 2,
        Some(s) if s.iron_sights && !g.awareness.hunted(FormId::PLAYER_REF) => 9,
        _ => 0,
    };
    Ok(Value::Num(f64::from(code)))
}

impl Game<'_> {
    /// The container that holds `actor`'s goods: SetMerchantContainer's, else its
    /// reference's (XMRC).
    pub fn merchant_container(&self, actor: FormId) -> Option<FormId> {
        self.extra.get(&actor).and_then(|e| e.merchant_container).or_else(|| self.world.placed.get(actor).and_then(|p| p.merchant_container))
    }

    /// Whether `actor` waits after Wait: the package it waits in is still its current one.
    pub fn waits(&self, actor: FormId) -> bool {
        self.extra.get(&actor).is_some_and(|e| e.waiting.is_some() && e.waiting == e.current_package)
    }

    /// The water type drawn for `water`: RemapWaterType's, else itself.
    pub fn water_type(&self, water: FormId) -> FormId {
        self.forms.water_remaps.get(&water).copied().unwrap_or(water)
    }
}
