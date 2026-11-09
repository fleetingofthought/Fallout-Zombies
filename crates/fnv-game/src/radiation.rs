//! Radiation from the environment: radiation markers and irradiated water giving the
//! player rads by the second, cut by Rad Resistance (docs/spec-notes/radiation.md).
//!
//! - **Markers** ([`RadMarker`]): placed references carrying XRAD (rads a second) and XRDS
//!   (the radius they reach) [documented: xEdit REFR; GECK wiki GetRadiationExtraData].
//!   In FalloutNV.esm all 119 are references of the static `RadiationMarker` [observed].
//!   A marker's level falls linearly with the 3-D distance from it, from its whole XRAD
//!   at its position to nothing at its radius ([`level_at`]) [documented: GECK wiki
//!   Reference, XRAD is the radiation given off "at the location of the marker", XRDS
//!   "the distance from the Radiation Marker that starts giving the player radiation";
//!   observed: the original's GetRadiationLevel in the nuclear test site's shack, 693.65
//!   units from its 15/2000 marker with Rad Resistance 8, read 9.013784, and
//!   15 × (1 − 693.65 ÷ 2000) × 0.92 = 9.013783 (issue #894); hypothesis: the linear
//!   shape at other distances]. Where markers overlap the strongest at the point counts
//!   [hypothesis: radiation.md §2]. A disabled marker gives nothing [hypothesis].
//! - **Water** ([`WaterContact`], told by the engine): swimming or wading in a water whose
//!   actor effect (WATR XNAM, the effect of drinking it) gives rads gives that many rads a
//!   second times `fSwimRadiationDamageMult` or `fWadeRadiationDamageMult` (0.1 each in
//!   FalloutNV.esm) [observed: the settings and the effects; hypothesis: the rule].
//! - **The level** ([`Exposure::level`]): markers plus water, before Rad Resistance.
//!   GetRadiationLevel and the HUD's meter give it after Rad Resistance
//!   ([`Exposure::resisted`]) [documented: GECK wiki GetRadiationLevel, the level "is
//!   directly proportional to your rads/sec meter"; observed: the original's 9.013784
//!   is the marker's 9.7976 × (1 − 8 ÷ 100), issue #894], before
//!   `fRadiationAccumulationRate` [documented: GECK wiki fRadiationAccumulationRate,
//!   the setting "does not influence the "+? Rad/Sec" UI element"].
//! - **Rads taken** a second: level × `fRadiationAccumulationRate` (1.0) × (1 − Rad
//!   Resistance ÷ 100), the resistance held to [`RESIST_CAP`] [hypothesis: the setting's
//!   name; resistance as a percentage, as Rad-X's +50 % reads]. They raise the Rads
//!   actor value; its stages (radiation sickness) follow in `crate::needs`.
//! - **Explosion zones** ([`RadZone`]): an explosion with a radiation level (EXPL DATA
//!   "RAD Level", "Dissipation Time", "Radius") leaves a zone where it went off: the
//!   level, fading linearly to nothing over the dissipation time (seconds of play),
//!   within the radiation radius (the Fat Man's: 10 rads/s, 12 s, 616 units)
//!   [documented: GECK wiki Explosion, "Amount of RADs per second", "Duration of
//!   radiation effect"; observed: the masters' values; hypothesis: the fade and the time
//!   unit]. A zone counts whole within its radius [hypothesis: whether it falls off
//!   with distance as a marker does is not known], the strongest of markers and zones
//!   counting (radiation.md §2.1).
//! - **Damaging water**: a water with "Causes Damage" takes its DATA damage of health
//!   from the player once a second while the player is in it [documented: GECK wiki
//!   WaterType, "Causes n damage per second"; hypothesis: wading counts as swimming, the
//!   first hit a second after going in]. No master's water has it [observed].
//! - Only the player takes environment radiation here [design]; GetRadiationLevel on
//!   another actor is 0 [documented: GECK wiki GetRadiationLevelAlt, the vanilla function
//!   "would only return a value when called on the player"].

use crate::actor_values::RAD_RESISTANCE;
use crate::aid::{value_change, RAD_LEVEL};
use crate::combat::setting;
use crate::data::GameData;
use crate::functions::actor as fa;
use crate::game::Game;
use fnv_sim::refs::RefTable;
use fnv_sim::FormId;
use std::collections::HashMap;

/// The highest Rad Resistance that counts, percent [hypothesis: as Damage Resistance's
/// 85 % cap].
pub const RESIST_CAP: f64 = 85.0;

/// One radiation marker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadMarker {
    pub id: FormId,
    pub pos: [f32; 3],
    /// Rads a second at the marker's position (XRAD), falling to 0 at the radius.
    pub level: f32,
    /// Game units (XRDS; 0 when absent).
    pub radius: f32,
}

/// A radiation zone an explosion left (EXPL DATA radiation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadZone {
    /// Counts up from 1 per game; the engine tells zones apart by it.
    pub serial: u32,
    /// The explosion (EXPL) that left it.
    pub explosion: FormId,
    /// The worldspace, else the interior cell, it lies in.
    pub space: FormId,
    pub pos: [f32; 3],
    /// Rads a second when it was left (EXPL "RAD Level").
    pub level: f32,
    /// Game units.
    pub radius: f32,
    /// Seconds of play it takes to fade (EXPL "Dissipation Time").
    pub duration: f32,
    /// Seconds of play since it was left.
    pub age: f64,
}

impl RadZone {
    /// The zone's level now: falling linearly from its first level to nothing at its
    /// duration [hypothesis: the shape of the fade].
    pub fn level_now(&self) -> f64 {
        let d = self.duration as f64;
        if d <= 0.0 || self.age >= d {
            return 0.0;
        }
        self.level as f64 * (1.0 - self.age / d)
    }
}

/// The zone an explosion leaves, if any: one with a radiation level above 0 and a
/// dissipation time above 0; its radius the radiation radius when that is at least one
/// unit, else the blast's radius [observed: every master's EXPL with radiation has a
/// radiation radius, 100 to 3000; hypothesis: the fallback, which no master needs].
/// `serial` and `space` are filled in by [`Game::add_radiation_zone`].
pub fn zone_of(explosion: FormId, pos: [f32; 3], level: f32, radiation_radius: f32, blast_radius: f32, dissipation: f32) -> Option<RadZone> {
    if !(level > 0.0 && dissipation > 0.0) {
        return None;
    }
    let radius = if radiation_radius >= 1.0 { radiation_radius } else { blast_radius.max(0.0) };
    (radius > 0.0).then_some(RadZone { serial: 0, explosion, space: FormId(0), pos, level, radius, duration: dissipation, age: 0.0 })
}

/// The level at `pos` from the zones of `space` and the strongest zone covering it.
pub fn zone_level(zones: &[RadZone], space: FormId, pos: [f32; 3]) -> (f64, Option<FormId>) {
    let mut best: (f64, Option<FormId>) = (0.0, None);
    for z in zones.iter().filter(|z| z.space == space) {
        let d2: f32 = (0..3).map(|i| (z.pos[i] - pos[i]).powi(2)).sum();
        let l = z.level_now();
        if d2 <= z.radius * z.radius && l > best.0 {
            best = (l, Some(z.explosion));
        }
    }
    best
}

/// What the engine says of the player and water.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterContact {
    /// The water type (WATR).
    pub water: FormId,
    /// Swimming; else wading (the feet under the surface).
    pub swimming: bool,
}

/// The player's exposure as the last frame measured it (not saved: the next frame
/// measures again).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Exposure {
    /// The water the player is in, as the engine told it.
    pub water: Option<WaterContact>,
    /// Rads a second from the place: the strongest marker or explosion zone covering the
    /// player, and which marker, or which zone's explosion, it is.
    pub markers: f64,
    pub marker: Option<FormId>,
    pub zone: Option<FormId>,
    /// Rads a second from the water.
    pub from_water: f64,
    /// The level (markers and water, before resistance).
    pub level: f64,
    /// The level after Rad Resistance, before `fRadiationAccumulationRate`:
    /// GetRadiationLevel and the HUD's meter.
    pub resisted: f64,
    /// Rads a second the player takes (after Rad Resistance).
    pub rate: f64,
    /// The Rad Resistance that cut it (percent, before the cap).
    pub resist: f64,
    /// Health a second the water takes ("Causes Damage"), 0 for a harmless one.
    pub water_damage: f64,
    /// Seconds in damaging water towards the next hit.
    pub water_clock: f64,
    /// Hits the water has dealt in all.
    pub water_hits: u32,
}

/// The markers of the placed references, by the space they are in (the worldspace of an
/// exterior cell, else the interior cell).
pub fn markers(refs: &RefTable) -> HashMap<FormId, Vec<RadMarker>> {
    let mut out: HashMap<FormId, Vec<RadMarker>> = HashMap::new();
    for (&id, p) in &refs.refs {
        let (Some(level), Some(cell)) = (p.radiation, p.cell) else { continue };
        if level <= 0.0 {
            continue;
        }
        let space = refs.world_of(cell).unwrap_or(cell);
        out.entry(space).or_default().push(RadMarker { id, pos: p.pos, level, radius: p.radius.unwrap_or(0.0).max(0.0) });
    }
    out
}

/// A marker's level at `pos`: its XRAD at its position, falling linearly with the 3-D
/// distance to 0 at its radius, 0 beyond (see the module notes).
pub fn level_at(m: &RadMarker, pos: [f32; 3]) -> f64 {
    let d = (0..3).map(|i| (m.pos[i] as f64 - pos[i] as f64).powi(2)).sum::<f64>().sqrt();
    let r = m.radius as f64;
    if r <= 0.0 || d >= r {
        return 0.0;
    }
    m.level as f64 * (1.0 - d / r)
}

/// The level at `pos` from `markers` and the strongest marker there (the first in the
/// list on a tie), each by [`level_at`].
pub fn marker_level(markers: &[RadMarker], pos: [f32; 3], enabled: impl Fn(FormId) -> bool) -> (f64, Option<FormId>) {
    let mut best: (f64, Option<FormId>) = (0.0, None);
    for m in markers {
        let l = level_at(m, pos);
        if l > best.0 && enabled(m.id) {
            best = (l, Some(m.id));
        }
    }
    best
}

/// Rads a second a water's actor effect gives when drunk: its value changes that raise
/// Rads ("Damage Radiation Level"), added up; 0 for a water without effect.
pub fn water_rads(data: &GameData, water: FormId) -> f64 {
    let Some(spell) = data.water_effects.get(&water) else { return 0.0 };
    let Some(items) = data.effect_items.get(spell) else { return 0.0 };
    items
        .iter()
        .filter_map(|e| value_change(data.magic_effects.get(&e.effect)?, e, 1.0, false))
        .filter(|c| c.code == RAD_LEVEL && c.amount < 0.0)
        .map(|c| -c.amount)
        .sum()
}

/// Rads a second taken at `level` with this Rad Resistance (percent).
pub fn taken(level: f64, resist: f64, accumulation: f64) -> f64 {
    (level * accumulation * (1.0 - resist.clamp(0.0, RESIST_CAP) / 100.0)).max(0.0)
}

impl Game<'_> {
    /// The engine tells what water the player is in (`None`: out of water).
    pub fn set_player_water(&mut self, water: Option<WaterContact>) {
        self.senses.radiation.water = water;
    }

    /// The player's exposure as last measured.
    pub fn radiation(&self) -> Exposure {
        self.senses.radiation
    }

    /// An explosion went off at `pos`: the radiation zone it leaves, if it leaves one
    /// ([`zone_of`]), in the space the player is in (explosions go off in the loaded
    /// world around the player) [design].
    pub fn add_radiation_zone(&mut self, explosion: FormId, pos: [f32; 3], level: f32, radiation_radius: f32, blast_radius: f32, dissipation: f32) -> Option<RadZone> {
        let mut z = zone_of(explosion, pos, level, radiation_radius, blast_radius, dissipation)?;
        let p = FormId::PLAYER_REF;
        let cell = self.world.peek(p).and_then(|s| s.cell).or_else(|| self.world.placed.get(p).and_then(|r| r.cell))?;
        z.space = self.world.placed.world_of(cell).unwrap_or(cell);
        self.senses.rad_serial += 1;
        z.serial = self.senses.rad_serial;
        self.senses.rad_zones.push(z);
        Some(z)
    }

    /// The radiation zones explosions left that have not faded yet.
    pub fn radiation_zones(&self) -> &[RadZone] {
        &self.senses.rad_zones
    }

    /// `seconds` of play pass for the zones (a frame, or hours waited); faded ones go.
    pub(crate) fn age_radiation_zones(&mut self, seconds: f64) {
        for z in &mut self.senses.rad_zones {
            z.age += seconds.max(0.0);
        }
        self.senses.rad_zones.retain(|z| z.level_now() > 0.0);
    }

    /// IsInDangerousWater: the player is in a water that "Causes Damage".
    pub fn in_dangerous_water(&self) -> bool {
        self.senses.radiation.water.is_some_and(|w| self.data.water_damage.contains_key(&w.water))
    }

    /// A space's markers with SetRadiationExtraData's levels: a marker's own level
    /// replaced, and a reference of the space that had none given one at its XRDS radius
    /// (0 without one, so it gives nothing); a level of 0 or less takes the marker away.
    pub(crate) fn edited_markers(&self, list: &[RadMarker], space: FormId) -> Vec<RadMarker> {
        let mut out: Vec<RadMarker> = list.iter().filter(|m| !self.showoff.radiation.contains_key(&m.id)).cloned().collect();
        let mut set: Vec<(FormId, f32)> = self.showoff.radiation.iter().map(|(&r, &l)| (r, l)).collect();
        set.sort_by_key(|x| x.0);
        for (id, level) in set {
            let Some(p) = self.world.placed.get(id) else { continue };
            let Some(cell) = p.cell else { continue };
            if level <= 0.0 || self.world.placed.world_of(cell).unwrap_or(cell) != space {
                continue;
            }
            out.push(RadMarker { id, pos: p.pos, level, radius: p.radius.unwrap_or(0.0).max(0.0) });
        }
        out
    }

    /// One frame of game-mode time: measure the player's exposure and add the rads.
    pub(crate) fn tick_radiation(&mut self, dt: f64) {
        let p = FormId::PLAYER_REF;
        let data = self.data;
        let place = self.world.peek(p).map(|s| (s.cell, s.pos)).or_else(|| self.world.placed.get(p).map(|r| (r.cell, r.pos)));
        self.age_radiation_zones(dt);
        let (markers, marker, zone) = match place {
            Some((Some(cell), pos)) => {
                let space = self.world.placed.world_of(cell).unwrap_or(cell);
                let list = data.radiation_markers.get(&space).map(Vec::as_slice).unwrap_or_default();
                // SetRadiationExtraData's levels over the records' (`nvse_showoff3`).
                let edited;
                let list = if self.showoff.radiation.is_empty() {
                    list
                } else {
                    edited = self.edited_markers(list, space);
                    edited.as_slice()
                };
                let world = &self.world;
                let (m, marker) = marker_level(list, pos, |id| world.is_enabled(id).unwrap_or(true));
                let (z, zone) = zone_level(&self.senses.rad_zones, space, pos);
                // The strongest counts, a marker on a tie (radiation.md §2.1).
                if z > m {
                    (z, None, zone)
                } else {
                    (m, marker, None)
                }
            }
            _ => (0.0, None, None),
        };
        let before = self.senses.radiation;
        let water = before.water;
        let from_water = water.map_or(0.0, |w| {
            let mult = if w.swimming { setting(self, "fSwimRadiationDamageMult", 0.1) } else { setting(self, "fWadeRadiationDamageMult", 0.1) };
            water_rads(data, w.water) * mult
        });
        let level = markers + from_water;
        let resist = if level > 0.0 { fa::actor_value(self, p, RAD_RESISTANCE).unwrap_or(0.0) } else { 0.0 };
        let resisted = taken(level, resist, 1.0);
        let rate = taken(level, resist, setting(self, "fRadiationAccumulationRate", 1.0));
        if rate > 0.0 && dt > 0.0 {
            self.shift_need(RAD_LEVEL, rate * dt);
        }
        // Damaging water: a hit of its damage each full second in it (§3.1).
        let water_damage = water.and_then(|w| data.water_damage.get(&w.water).copied()).unwrap_or(0.0);
        let mut water_clock = if water_damage > 0.0 { before.water_clock + dt.max(0.0) } else { 0.0 };
        let mut water_hits = before.water_hits;
        while water_damage > 0.0 && water_clock >= 1.0 {
            water_clock -= 1.0;
            water_hits += 1;
            self.effect_health_damage(p, water_damage, None);
        }
        self.senses.radiation = Exposure { water, markers, marker, zone, from_water, level, resisted, rate, resist, water_damage, water_clock, water_hits };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: u32, level: f32, radius: f32) -> RadMarker {
        RadMarker { id: FormId(id), pos: [0.0, 0.0, 0.0], level, radius }
    }

    fn near((l, id): (f64, Option<FormId>), want: f64, who: u32) -> bool {
        (l - want).abs() < 1e-9 && id == Some(FormId(who))
    }

    #[test]
    fn a_marker_falls_off_linearly_within_its_radius_and_the_strongest_wins() {
        // The Camp Searchlight crater's three markers on one spot.
        let list = [m(1, 5.0, 10000.0), m(2, 10.0, 6000.0), m(3, 18.0, 2000.0)];
        let all = |_| true;
        assert_eq!(marker_level(&list, [0.0, 0.0, 0.0], all), (18.0, Some(FormId(3))));
        // 1000 out: 18 × 0.5 = 9 beats 10 × (1 − 1000/6000) = 8.33.
        assert!(near(marker_level(&list, [1000.0, 0.0, 0.0], all), 9.0, 3));
        // 1500 out: 10 × 0.75 = 7.5 beats 18 × 0.25 = 4.5.
        assert!(near(marker_level(&list, [0.0, 1500.0, 0.0], all), 7.5, 2));
        // 7000 out, past the 6000 and 2000 radii: 5 × 0.3.
        assert!(near(marker_level(&list, [7000.0, 0.0, 0.0], all), 1.5, 1));
        assert_eq!(marker_level(&list, [0.0, 0.0, 10001.0], all), (0.0, None));
        // A disabled marker gives nothing.
        assert!(near(marker_level(&list, [1000.0, 0.0, 0.0], |id| id != FormId(3)), 10.0 * 5.0 / 6.0, 2));
        // No radius: nothing.
        assert_eq!(level_at(&m(4, 10.0, 0.0), [0.0; 3]), 0.0);
    }

    #[test]
    fn the_test_site_shack_matches_the_original() {
        // Issue #894: marker 0x00154611 (XRAD 15, XRDS 2000) and the original's player at
        // the shack's coc spot (GetPos), Rad Resistance 8: GetRadiationLevel 9.013784.
        let mk = RadMarker { id: FormId(0x154611), pos: [4358.328, -718.8923, -88.0], level: 15.0, radius: 2000.0 };
        let resisted = taken(level_at(&mk, [4318.96, -27.96, -135.1107]), 8.0, 1.0);
        assert!((resisted - 9.013784).abs() < 1e-4, "{resisted}");
        // The original's HUD: +9.
        assert_eq!(resisted.round(), 9.0);
    }

    #[test]
    fn an_explosion_zone_fades_linearly_within_its_radiation_radius() {
        // FatManNukeExplosion: 10 rads/s, 12 s, radiation radius 616, blast radius 1700.
        let mut z = zone_of(FormId(0x14C06), [0.0, 0.0, 0.0], 10.0, 616.0, 1700.0, 12.0).expect("a zone");
        assert_eq!(z.radius, 616.0);
        assert_eq!(z.level_now(), 10.0);
        z.age = 6.0;
        assert!((z.level_now() - 5.0).abs() < 1e-9);
        z.age = 12.0;
        assert_eq!(z.level_now(), 0.0);
        // Without a radiation radius, the blast's.
        assert_eq!(zone_of(FormId(1), [0.0; 3], 12.0, 0.0, 1200.0, 616.0).map(|z| z.radius), Some(1200.0));
        // No radiation or no time: no zone.
        assert!(zone_of(FormId(1), [0.0; 3], 0.0, 0.0, 1200.0, 616.0).is_none());
        assert!(zone_of(FormId(1), [0.0; 3], 12.0, 0.0, 1200.0, 0.0).is_none());
    }

    #[test]
    fn zones_count_in_their_own_space_within_their_radius() {
        let w = FormId(0xDA726);
        let mut a = zone_of(FormId(1), [0.0, 0.0, 0.0], 12.0, 0.0, 1700.0, 616.0).unwrap();
        a.space = w;
        let mut b = zone_of(FormId(2), [0.0, 0.0, 0.0], 2.0, 0.0, 450.0, 200.0).unwrap();
        b.space = w;
        let zones = [a, b];
        assert_eq!(zone_level(&zones, w, [100.0, 0.0, 0.0]), (12.0, Some(FormId(1))));
        assert_eq!(zone_level(&zones, w, [1800.0, 0.0, 0.0]), (0.0, None));
        assert_eq!(zone_level(&zones, FormId(7), [100.0, 0.0, 0.0]), (0.0, None));
    }

    #[test]
    fn rad_resistance_cuts_the_rads_up_to_its_cap() {
        assert_eq!(taken(10.0, 0.0, 1.0), 10.0);
        // Endurance 5: (5 - 1) x 2 = 8 %.
        assert!((taken(10.0, 8.0, 1.0) - 9.2).abs() < 1e-9);
        // Rad-X's +50 % on top.
        assert!((taken(10.0, 58.0, 1.0) - 4.2).abs() < 1e-9);
        assert!((taken(10.0, 120.0, 1.0) - 1.5).abs() < 1e-9);
        assert_eq!(taken(0.0, 0.0, 1.0), 0.0);
    }
}
