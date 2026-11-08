//! The HUD's script reads that need no menu, and the weapon-out functions (the peer's
//! split, hud1 and controls1; docs/spec-notes/ext-hud.md) [documented: their pages;
//! observed: hud1 and controls1 in the original game, 2026-10-27]. The engine takes the
//! weapon asks ([`HudState::weapon_asks`]) as it takes `gaps.view_request`.

use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

/// Every HUD visibility flag, bits 0-16 [documented: GetHUDVisibilityOverride's flag
/// list; observed: hud1 reads 131071].
pub const HUD_ALL: u32 = 0x1FFFF;

/// Frames a scripted draw or holster takes before IsWeaponOut changes [observed:
/// controls1, SetWeaponOut 1 reads 0 at 10 frames and 1 at 30; the draw after
/// HolsterWeapon reads 0 at 10 and 1 at 15; SetWeaponOut 0 reads 1 at 10 and 0 at 30.
/// 12 fits all three; hypothesis: the length is the animation's].
pub const WEAPON_FRAMES: u64 = 12;

/// Seconds a corner message stays [hypothesis: the queue's pace is not measured].
pub const CORNER_SECONDS: f64 = 2.0;

#[derive(Debug)]
pub struct HudState {
    /// JohnnyGuitar's HUD visibility override.
    pub visibility: u32,
    /// Corner messages waiting, the one shown included, and when the shown one ends.
    pub corner_queue: usize,
    pub corner_until: f64,
    /// Actors SetWeaponOut 1 holds drawn: a HolsterWeapon puts the weapon away and it
    /// comes out again [observed: controls1, after SetWeaponOut 1 and HolsterWeapon the
    /// player reads 0 for 10 frames and 1 from 15 to 120; SetWeaponOut 0 puts it away
    /// for good; hypothesis: that the drawn hold is what brings it back].
    pub forced_out: BTreeSet<FormId>,
    /// IsWeaponOut changes due: actor -> (out, the frame, whether to ask the engine then).
    pub weapon_due: BTreeMap<FormId, (bool, u64, bool)>,
    /// Draws (true) and holsters for the engine to take.
    pub weapon_asks: Vec<(FormId, bool)>,
}

impl Default for HudState {
    fn default() -> Self {
        HudState { visibility: HUD_ALL, corner_queue: 0, corner_until: 0.0, forced_out: BTreeSet::new(), weapon_due: BTreeMap::new(), weapon_asks: Vec::new() }
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn list(g: &mut Game, items: Vec<XVal>) -> XVal {
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

impl Game<'_> {
    /// A corner message joins the queue (MessageEx, ShowMessage without a box)
    /// [documented: GetNumQueuedCornerMessages counts the one shown too; observed: hud1,
    /// three MessageEx calls raise the count by 3 in their frame].
    /// ShowOff's corner-message handlers get the text, the icon path and the seconds
    /// (`nvse_showoff4`, run on the next frame).
    pub(crate) fn queue_corner_message(&mut self, text: &str, icon: &str, seconds: f64) {
        if !self.showoff.corner_handlers.is_empty() {
            self.showoff.corner_pending.push((text.to_string(), icon.to_string(), seconds));
        }
        if self.hud.corner_queue == 0 {
            self.hud.corner_until = self.time + CORNER_SECONDS;
        }
        self.hud.corner_queue += 1;
    }

    /// Each frame (`Game::tick`): the shown corner message ends after its time, and the
    /// weapon changes due take effect.
    pub(crate) fn hud_tick(&mut self) {
        self.corner_handlers_run();
        if self.hud.corner_queue > 0 && self.time >= self.hud.corner_until {
            self.hud.corner_queue -= 1;
            self.hud.corner_until = self.time + CORNER_SECONDS;
        }
        if self.hud.weapon_due.is_empty() {
            return;
        }
        let frame = self.frame;
        let (due, wait): (BTreeMap<_, _>, BTreeMap<_, _>) = std::mem::take(&mut self.hud.weapon_due).into_iter().partition(|(_, (_, f, _))| *f <= frame);
        self.hud.weapon_due = wait;
        for (a, (out, _, ask)) in due {
            self.extra.entry(a).or_default().weapon_out = out;
            if ask {
                self.hud.weapon_asks.push((a, out));
            }
        }
    }

    /// The actors fighting the player that the compass shows, nearest first, with their
    /// distance [documented: GetCompassHostiles; hypothesis: every loaded, enabled, living
    /// actor whose combat targets hold the player, at any range].
    pub(crate) fn compass_hostiles(&self) -> Vec<(FormId, f32)> {
        let Some(me) = crate::awareness::position(self, FormId::PLAYER_REF) else { return Vec::new() };
        let mut out: Vec<(FormId, f32)> = self
            .awareness
            .actors
            .iter()
            .filter(|(&a, w)| w.state.in_combat() && w.targets.contains_key(&FormId::PLAYER_REF) && !self.is_dead(a) && self.world.is_enabled(a).unwrap_or(false))
            .filter_map(|(&a, _)| {
                let p = crate::awareness::position(self, a)?;
                Some((a, ((p[0] - me[0]).powi(2) + (p[1] - me[1]).powi(2) + (p[2] - me[2]).powi(2)).sqrt()))
            })
            .collect();
        out.sort_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)));
        out
    }

    /// Start a scripted draw or holster: IsWeaponOut changes [`WEAPON_FRAMES`] later.
    fn weapon_change(&mut self, a: FormId, out: bool, ask_at_due: bool) {
        let due = self.frame + WEAPON_FRAMES;
        self.hud.weapon_due.insert(a, (out, due, ask_at_due));
    }
}

/// The functions; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let who = subject.unwrap_or(FormId::PLAYER_REF);
    let v = match name {
        // ---- the HUD ----
        "gethudvisibilityoverride" => XVal::Num(f64::from(g.hud.visibility)),
        // [observed: hud1, 3 reads back in its frame and the next]
        "sethudvisibilityoverride" => {
            g.hud.visibility = (num(args, 0, f64::from(HUD_ALL)) as i64).clamp(0, i64::from(u32::MAX)) as u32;
            XVal::Num(0.0)
        }
        // Nothing shakes the HUD yet [observed: hud1 reads 0 at rest].
        "gethudshudderpower" => XVal::Num(0.0),
        // [documented: GetReticlePos, X Y Z, ar_Null when the spot is not valid] The point
        // the engine's crosshair ray hits; a headless run has none.
        "getreticlepos" => match g.senses.reticle.and_then(|r| r.point(crate::functions::senses::RETICLE_REACH)) {
            Some(p) => {
                let items = p.iter().map(|&c| XVal::Num(f64::from(c))).collect();
                list(g, items)
            }
            None => XVal::Array(0),
        },
        // -1 with no actor in the crosshair [observed: hud1]; the engine does not tell the
        // limb the ray meets, so an actor reads -1 too [not built: the limb].
        "getreticletargetlimb" => XVal::Num(-1.0),
        // ---- corner messages ----
        "getnumqueuedcornermessages" => XVal::Num(g.hud.corner_queue as f64),
        // [observed: hud1, 0 in its frame and the next]
        "clearmessagequeue" => {
            g.hud.corner_queue = 0;
            XVal::Num(0.0)
        }
        // ---- the compass ----
        // `GetNumCompassHostiles [maxRange] [flags]`; no range, or 0, counts all
        // [documented; observed: hud1, a fighting NPC 500 units off counts 1, and 0 within
        // 100]. Flag 1 (no invisible actors) changes nothing: nothing is invisible to the
        // compass yet [hypothesis].
        "getnumcompasshostiles" => {
            let range = num(args, 0, 0.0);
            XVal::Num(g.compass_hostiles().iter().filter(|(_, d)| range <= 0.0 || f64::from(*d) <= range).count() as f64)
        }
        "getcompasshostiles" => {
            let items = g.compass_hostiles().into_iter().map(|(a, _)| XVal::Ref(a.0)).collect();
            list(g, items)
        }
        "getnearestcompasshostile" => XVal::Ref(g.compass_hostiles().first().map_or(0, |(a, _)| a.0)),
        // ShowOff's list stays empty, in a fight too [observed: hud1, sizes 0 for 0, 1 and
        // 2 while GetCompassHostiles held the NPC; the page lists bugs].
        "getcompasstargets" => list(g, Vec::new()),
        // ---- the weapon ----
        // `SetWeaponOut 1/0` [documented]; IsWeaponOut follows [`WEAPON_FRAMES`] later
        // [observed: controls1]. The engine is asked at once.
        "setweaponout" => {
            let out = num(args, 0, 1.0) != 0.0;
            if out {
                g.hud.forced_out.insert(who);
            } else {
                g.hud.forced_out.remove(&who);
            }
            g.weapon_change(who, out, false);
            g.hud.weapon_asks.push((who, out));
            XVal::Num(0.0)
        }
        // [documented: holsters a weapon that is out] IsWeaponOut reads 0 in the same frame
        // [observed: controls1]; an actor SetWeaponOut 1 holds drawn draws again.
        "holsterweapon" => {
            g.extra.entry(who).or_default().weapon_out = false;
            g.hud.weapon_due.remove(&who);
            g.hud.weapon_asks.push((who, false));
            if g.hud.forced_out.contains(&who) {
                g.weapon_change(who, true, true);
            }
            XVal::Num(0.0)
        }
        _ => return None,
    };
    Some(Ok(v))
}
