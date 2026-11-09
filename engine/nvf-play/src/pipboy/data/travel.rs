//! Fast travel from the World Map (docs/spec-notes/fast-travel.md).
//!
//! - A click on a marker of the World Map, or the controller's A on one (or the test
//!   aid's `travel=`) is a [`FastTravelAsk`]; [`answer`] tries the refusals (§2,
//!   [`refusal`]) and puts up the
//!   question box ([`Popup`]): "Do you want to travel to …?" with Yes and No, or the
//!   refusal's text with OK. The DATA page draws the box over the map and takes its keys
//!   and clicks (`super::take_input`); Yes is a [`FastTravelGo`].
//! - [`go`] puts the Pip-Boy away and keeps the trip; once the game runs again
//!   ([`depart`]) the trip goes out as a door's does (`crate::loading::TravelRequest`),
//!   the clock moves on by the travel time ([`travel_hours`]) and the player's teammates
//!   come along (`fnv_game::Game::bring_teammates`).

use super::{read, DataState};
use crate::hitvol::HitVolumes;
use crate::loading::TravelRequest;
use crate::menu::model::Command;
use crate::menu::MenuInput;
use crate::music::MusicCues;
use crate::phys::PhysState;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::wait::{HourPassed, HOSTILE_DISTANCE};
use crate::{Location, PlayState, PlayWorld, Travel};
use bevy::prelude::*;
use fnv_sim::settings::Setting;
use fnv_sim::FormId;

/// A click on a map marker of the World Map, or the controller's A on one (or the test
/// aid's `travel=`): ask to go there.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct FastTravelAsk(pub u32);

/// The player answered Yes: travel to this marker.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct FastTravelGo(pub u32);

/// Why the player cannot travel now (fast-travel.md §2), in the order they are tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Undiscovered,
    Script,
    Place,
    InAir,
    Combat,
    HostilesNear,
    Encumbered,
}

impl Refusal {
    pub const ALL: [Refusal; 7] = [Refusal::Undiscovered, Refusal::Script, Refusal::Place, Refusal::InAir, Refusal::Combat, Refusal::HostilesNear, Refusal::Encumbered];

    /// The game setting holding the refusal's text [documented: GECK wiki, game settings
    /// list].
    pub fn setting(self) -> &'static str {
        match self {
            Refusal::Undiscovered => "sNoFastTravelUndiscovered",
            Refusal::Script => "sNoFastTravelScriptBlock",
            Refusal::Place => "sNoFastTravelCell",
            Refusal::InAir => "sNoFastTravelInAir",
            Refusal::Combat => "sNoFastTravelCombat",
            Refusal::HostilesNear => "sNoFastTravelHostileActorsNear",
            Refusal::Encumbered => "sNoFastTravelOverencumbered",
        }
    }

    /// NVForever's own text where no plugin sets the setting [design]; the script one is
    /// FalloutNV.esm's [observed].
    fn own_text(self) -> &'static str {
        match self {
            Refusal::Undiscovered => "You cannot fast travel to a location you have not discovered.",
            Refusal::Script => "Fast travel is currently unavailable from this location.",
            Refusal::Place => "You cannot fast travel from this location.",
            Refusal::InAir => "You cannot fast travel while in the air.",
            Refusal::Combat => "You cannot fast travel during combat.",
            Refusal::HostilesNear => "You cannot fast travel when enemies are nearby.",
            Refusal::Encumbered => "You cannot fast travel while carrying too much.",
        }
    }
}

/// What the place and the player are like when a travel is asked for.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Checks {
    /// The marker is a travel target (found, its Can Travel To flag, ShowMap's or
    /// ShowAllMapMarkers' canTravel).
    pub target: bool,
    /// EnableFastTravel 0 holds.
    pub script_block: bool,
    /// The worldspace's Can't Fast Travel flag outdoors, the cell's Can't Travel From
    /// Here flag indoors.
    pub place: bool,
    pub in_air: bool,
    pub combat: bool,
    pub hostiles_near: bool,
    pub encumbered: bool,
}

/// The first refusal that applies, if any.
pub fn refusal(c: &Checks) -> Option<Refusal> {
    let applies = |r: Refusal| match r {
        Refusal::Undiscovered => !c.target,
        Refusal::Script => c.script_block,
        Refusal::Place => c.place,
        Refusal::InAir => c.in_air,
        Refusal::Combat => c.combat,
        Refusal::HostilesNear => c.hostiles_near,
        Refusal::Encumbered => c.encumbered,
    };
    Refusal::ALL.into_iter().find(|&r| applies(r))
}

/// The CELL `DATA` flag "Can't Travel From Here" and the WRLD `DATA` flag "Can't Fast
/// Travel" [documented: xEdit, schemas/fnv/world.json].
pub const CELL_NO_TRAVEL: u8 = 1 << 2;
pub const WORLD_NO_TRAVEL: u8 = 1 << 1;

/// The question [observed: the owner's screenshot, "Do you want to travel to
/// Goodsprings?"]; a plugin's `sTravelQuestion` with `%s` takes the name there, one
/// without it is followed by the name [design].
pub const OWN_QUESTION: &str = "Do you want to travel to %s?";

pub fn question(template: &str, name: &str) -> String {
    if template.contains("%s") {
        template.replacen("%s", name, 1)
    } else {
        format!("{} {name}?", template.trim_end().trim_end_matches('?'))
    }
}

/// The trip's game hours: the straight line walked at `walk` units a second at
/// `time_scale` game seconds a real second [hypothesis, fast-travel.md §4]; none within
/// a cell's width [design].
pub fn travel_hours(distance: f32, walk: f32, time_scale: f64) -> f64 {
    if distance <= super::model::CELL || walk <= 0.0 {
        return 0.0;
    }
    distance as f64 / walk as f64 * time_scale / 3600.0
}

/// The game's TimeScale when the plugins lack the global [documented: GECK wiki
/// TimeScale, default 30].
const OWN_TIME_SCALE: f64 = 30.0;

/// What the question box asks.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// "Do you want to travel to …?": the marker.
    Travel(u32),
    /// A refusal: OK only.
    Refused,
}

/// The question box over the World Map.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    pub ask: Ask,
    pub text: String,
    pub buttons: Vec<String>,
    pub selected: usize,
}

impl Popup {
    pub fn travel(marker: u32, text: String) -> Popup {
        Popup { ask: Ask::Travel(marker), text, buttons: vec!["Yes".into(), "No".into()], selected: 0 }
    }

    pub fn refused(text: String) -> Popup {
        Popup { ask: Ask::Refused, text, buttons: vec!["OK".into()], selected: 0 }
    }

    /// Move the choice, stopping at the ends.
    pub fn step(&mut self, by: i32) {
        self.selected = (self.selected as i32 + by).clamp(0, self.buttons.len() as i32 - 1) as usize;
    }

    /// The answer `i` was taken: the marker to travel to, if it was Yes.
    pub fn take(&self, i: usize) -> Option<u32> {
        match self.ask {
            Ask::Travel(m) if i == 0 => Some(m),
            _ => None,
        }
    }
}

fn setting_str(sim: &SimState, name: &str) -> Option<String> {
    match sim.forms.settings.get(&name.to_ascii_lowercase()) {
        Some(Setting::Str(s)) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn setting_f32(sim: &SimState, name: &str) -> Option<f32> {
    match sim.forms.settings.get(&name.to_ascii_lowercase()) {
        Some(Setting::Float(v)) => Some(*v),
        _ => None,
    }
}

/// Whether the player may travel to `m` by the scripts' and the markers' word: found,
/// flagged Can Travel To, or made a target by ShowMap or ShowAllMapMarkers.
fn travel_target(st: &DataState, script: Option<&ScriptState>, m: &read::Marker) -> bool {
    let by_script = script.and_then(|s| s.rt.as_ref()).is_some_and(|rt| {
        let p = &rt.game.player;
        p.travel_markers.contains(&FormId(m.form)) || p.all_markers.is_some_and(|a| a.shown && a.travel)
    });
    // LU's SetMapMarkerTravel over the record's flag (`fnv_game::nvse_lu2`).
    let flags = script.and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.marker_flags(FormId(m.form))).unwrap_or(m.flags);
    st.discovered.contains(&m.form) || flags & 2 != 0 || by_script
}

/// What the refusals look at: the player's body, where they are, the actors near, the
/// music's battle test and the plugins.
type Around<'w> = (Res<'w, PhysState>, Res<'w, Location>, Res<'w, HitVolumes>, Option<Res<'w, MusicCues>>, Res<'w, PlayWorld>);

/// Answer the asks: put up the question, or the refusal.
pub fn answer(
    mut asks: MessageReader<FastTravelAsk>,
    state: Option<ResMut<DataState>>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    (phys, location, actors, cues, world): Around,
) {
    let Some(&FastTravelAsk(marker)) = asks.read().last() else { return };
    let Some(mut st) = state else { return };
    // The World Map reads the markers when first drawn; an ask from the test aid can come
    // before the map ever was.
    if st.markers.is_none() {
        st.markers = Some(read::markers(world.lo, world.schema, sim.refs));
    }
    let Some(m) = st.markers.iter().flatten().find(|m| m.form == marker).cloned() else {
        warn!("fast travel: {} is not a named map marker", FormId(marker));
        return;
    };
    let indoors = location.interior.is_some();
    let place = match location.interior {
        Some(c) => world.index.cells.get(&c).is_some_and(|c| c.data_flags & CELL_NO_TRAVEL != 0),
        None => read::world_flags(world.lo, world.schema, location.world) & WORLD_NO_TRAVEL != 0,
    };
    let player = phys.player.as_ref();
    let feet = player.map(|c| c.feet);
    let distance = setting_f32(&sim, ["fHostileActorExteriorDistance", "fHostileActorInteriorDistance"][usize::from(indoors)]).unwrap_or(HOSTILE_DISTANCE[usize::from(indoors)]);
    let script_block = script.as_ref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.player.fast_travel.travel_disabled);
    let mut hostile = None;
    if let (Some(rt), Some(feet)) = (script.as_mut().and_then(|s| s.rt.as_mut()), feet) {
        let near: Vec<u32> = actors.living.iter().filter(|(_, p)| (0..3).map(|k| (p[k] - feet[k]).powi(2)).sum::<f32>() <= distance * distance).map(|(id, _)| *id).collect();
        hostile = crate::script::with_world(rt, &mut sim, |rt| {
            near.iter().copied().find(|&a| fnv_game::rest::hostile_to_player(&mut rt.game, FormId(a))).map(|a| rt.game.data.name(FormId(a)))
        });
    }
    // Long Haul's Has Fast Travel Always: over-encumbered does not stop it
    // (docs/spec-notes/perk-entry-points.md §7).
    let over = sim.world.encumbrance(FormId::PLAYER_REF).is_ok_and(|e| e.over) && !crate::combat::god_mode();
    let long_haul = over
        && script.as_mut().and_then(|s| s.rt.as_mut()).is_some_and(|rt| crate::script::with_world(rt, &mut sim, |rt| rt.game.player_has_entry(fnv_game::perks::HAS_FAST_TRAVEL_ALWAYS)));
    if long_haul {
        info!("fast travel: over-encumbered, but Has Fast Travel Always (Long Haul) lets the player travel");
    }
    let encumbered = over && !long_haul;
    let checks = Checks {
        target: travel_target(&st, script.as_deref(), &m),
        script_block,
        place,
        // In water the player is not falling [hypothesis].
        in_air: player.is_some_and(|c| !c.grounded && !c.swimming),
        combat: cues.is_some_and(|c| c.combat),
        hostiles_near: hostile.is_some(),
        encumbered,
    };
    st.popup = Some(match refusal(&checks) {
        Some(r) => {
            let text = setting_str(&sim, r.setting()).unwrap_or_else(|| r.own_text().to_string());
            let why = match (r, &hostile) {
                (Refusal::HostilesNear, Some(n)) => format!("{n} is hostile and within {distance:.0} units"),
                _ => format!("{r:?}"),
            };
            info!("fast travel: to {} ({}) refused ({why}); the box says {text:?}", m.name, FormId(marker));
            Popup::refused(text)
        }
        None => {
            let text = question(&setting_str(&sim, "sTravelQuestion").unwrap_or_else(|| OWN_QUESTION.to_string()), &m.name);
            info!("fast travel: asks {text:?} ({})", FormId(marker));
            Popup::travel(marker, text)
        }
    });
}

/// A trip waiting for the Pip-Boy to go away.
#[derive(Resource, Default)]
pub struct Pending(Option<Trip>);

#[derive(Debug, Clone, Copy)]
struct Trip {
    travel: Travel,
    /// The destination reference (the marker's linked one, or the marker) and its cell.
    to: u32,
    cell: Option<u32>,
    rot: [f32; 3],
    hours: f64,
}

/// Yes: work out where the player arrives and how long it takes; put the Pip-Boy away.
#[allow(clippy::too_many_arguments)]
pub fn go(
    mut goes: MessageReader<FastTravelGo>,
    state: Option<Res<DataState>>,
    sim: Res<SimState>,
    phys: Res<PhysState>,
    location: Res<Location>,
    mut pending: ResMut<Pending>,
    mut menu: MessageWriter<MenuInput>,
) {
    let Some(&FastTravelGo(marker)) = goes.read().last() else { return };
    let Some(m) = state.as_ref().and_then(|st| st.markers.iter().flatten().find(|m| m.form == marker).cloned()) else { return };
    let placed = sim.refs.refs.get(&FormId(marker));
    // The marker's linked reference is where the player arrives [documented: GECK wiki
    // SetOnFastTravelEventHandler], else the marker [hypothesis].
    let dest = placed.and_then(|r| r.linked).and_then(|l| sim.refs.refs.get(&l)).or(placed);
    let Some(dest) = dest else {
        warn!("fast travel: {} has no placed reference", FormId(marker));
        return;
    };
    let world = dest.cell.and_then(|c| sim.refs.cell_world.get(&c)).map(|w| w.0);
    let interior = if world.is_some() { None } else { dest.cell.map(|c| c.0) };
    // Where the walk starts: the player's feet outdoors, indoors where they were last
    // outdoors in the destination's worldspace [design]; across worldspaces the places
    // are compared as they stand [design].
    let from = match location.interior {
        None => phys.player.as_ref().map(|c| c.feet),
        Some(_) => state.as_ref().and_then(|st| st.outdoors).filter(|o| Some(o.0) == world).map(|o| o.1),
    };
    let distance = from.map_or(0.0, |f| (dest.pos[0] - f[0]).hypot(dest.pos[1] - f[1]));
    let scale = sim.world.time_scale().unwrap_or(OWN_TIME_SCALE);
    let hours = travel_hours(distance, crate::player::MoveSpeeds::OBSERVED.walk, scale);
    let travel = Travel { world, interior, feet: dest.pos, heading_deg: crate::activate::outcome::heading_deg(dest.rot) };
    info!(
        "fast travel: to {} ({}): arrive at {} {:?}, {:.0} units away, {hours:.2} hours on the clock",
        m.name,
        FormId(marker),
        if dest.id.0 == marker { "the marker".to_string() } else { format!("its linked reference {}", dest.id) },
        dest.pos,
        distance
    );
    pending.0 = Some(Trip { travel, to: dest.id.0, cell: dest.cell.map(|c| c.0), rot: dest.rot, hours });
    menu.write(MenuInput(Command::Close));
}

/// Once the game runs again: the trip goes out behind the loading screen, the clock moves
/// on and the teammates come along.
#[allow(clippy::too_many_arguments)]
pub fn depart(
    state: Res<State<PlayState>>,
    mut pending: ResMut<Pending>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    mut travel: MessageWriter<TravelRequest>,
    mut passed: MessageWriter<HourPassed>,
    landing: Option<ResMut<crate::save::FastTravelLanding>>,
    screen: Option<ResMut<crate::loading::Loading>>,
) {
    if *state.get() != PlayState::Playing {
        return;
    }
    let Some(t) = pending.0.take() else { return };
    // The player's own trip (`Loading::arriving`): the destination's actors are put at
    // their package's place behind the screen.
    if let Some(mut s) = screen {
        s.arriving = true;
    }
    travel.write(TravelRequest(t.travel));
    if let Some(mut l) = landing {
        l.0 = true;
    }
    if t.hours > 0.0 {
        match sim.world.advance_hours(t.hours) {
            Ok(c) => info!("fast travel: {:.2} hours passed (GameHour {:.3}, GameDaysPassed {:.3})", t.hours, c.hour, c.days_passed),
            Err(e) => warn!("fast travel: the game clock cannot advance: {e}"),
        }
        for _ in 0..t.hours.floor() as u32 {
            passed.write(HourPassed { sleep: false });
        }
    }
    let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
    crate::script::with_world(rt, &mut sim, |rt| {
        let g = &mut rt.game;
        if t.hours > 0.0 {
            g.pass_hours(t.hours, false);
        }
        // The player's reference is where the trip ends, so that the teammates are put
        // beside them there (as a load door does).
        let cell = t.cell.map(|c| {
            let c = FormId(c);
            g.world.placed.exterior_cell_at(c, t.travel.feet).unwrap_or(c)
        });
        let from = g.world.peek(FormId::PLAYER_REF).map(|p| (p.cell, p.pos));
        if let Ok(p) = g.world.get(FormId::PLAYER_REF) {
            (p.cell, p.pos, p.rot) = (cell, t.travel.feet, [0.0, 0.0, t.rot[2]]);
        }
        g.player_entered(cell);
        let brought = g.bring_teammates(from);
        info!("fast travel: under way to {}; {} teammate(s) or follower(s) come along", FormId(t.to), brought.len());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refusals_go_in_order() {
        let open = Checks { target: true, ..Checks::default() };
        assert_eq!(refusal(&open), None);
        assert_eq!(refusal(&Checks::default()), Some(Refusal::Undiscovered));
        let all = Checks { target: true, script_block: true, place: true, in_air: true, combat: true, hostiles_near: true, encumbered: true };
        assert_eq!(refusal(&all), Some(Refusal::Script));
        assert_eq!(refusal(&Checks { script_block: false, place: false, ..all }), Some(Refusal::InAir));
        assert_eq!(refusal(&Checks { target: true, hostiles_near: true, ..Checks::default() }), Some(Refusal::HostilesNear));
    }

    #[test]
    fn the_question_names_the_marker() {
        assert_eq!(question(OWN_QUESTION, "Goodsprings"), "Do you want to travel to Goodsprings?");
        assert_eq!(question("Travel to", "Primm"), "Travel to Primm?");
    }

    #[test]
    fn travel_time_is_the_walk_at_timescale() {
        // 86.8 units a second for 3600 real seconds is one game hour at TimeScale 1.
        assert!((travel_hours(86.8 * 3600.0, 86.8, 1.0) - 1.0).abs() < 1e-5);
        assert!((travel_hours(86.8 * 120.0, 86.8, 30.0) - 1.0).abs() < 1e-5);
        assert_eq!(travel_hours(1000.0, 86.8, 30.0), 0.0);
    }

    #[test]
    fn the_box_answers() {
        let mut p = Popup::travel(7, "Do you want to travel to X?".into());
        assert_eq!(p.take(p.selected), Some(7));
        p.step(1);
        p.step(1);
        assert_eq!(p.selected, 1);
        assert_eq!(p.take(p.selected), None);
        let r = Popup::refused("No.".into());
        assert_eq!((r.buttons.len(), r.take(0)), (1, None));
    }
}
