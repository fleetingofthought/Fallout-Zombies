//! Encounter zones in the game: a zone's level is set when its first cell loads, and the
//! leveled lists of every actor and container placed there are read at the reference's
//! level from then on (docs/spec-notes/encounter-zones.md; the rules are
//! `fnv_sim::zones`).

use crate::data::ActorDef;
use crate::game::Game;
use fnv_sim::zones::LevelAt;
use fnv_sim::FormId;
use serde::Serialize;

/// What a cell load did to levels: a zone's level set, or a leveled actor fixed at a
/// level (for the engine's log and its drawn actors).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LevelNews {
    /// `zone` was entered for the first time: its level is now `level` (its minimum
    /// `min_level`, the player at `player_level`).
    Zone { zone: FormId, level: u16, min_level: i8, player_level: u16 },
    /// The placed actor `actor`, whose template chain meets the leveled list `list`, was
    /// fixed at `at` and stands for `base` (the end of its template chain); `at_display` is what it stood for at the fixed level 1 used before
    /// encounter zones were read (for the log: which actors the zones changed).
    Actor { actor: FormId, list: FormId, at: LevelAt, base: Option<FormId>, at_display: Option<FormId>, zone: Option<FormId> },
}

/// News kept untaken at most.
const MAX_NEWS: usize = 4096;

impl<'a> Game<'a> {
    /// The player's level as leveled lists read it.
    fn list_player_level(&self) -> u16 {
        self.player_level().clamp(1.0, f64::from(u16::MAX)) as u16
    }

    /// Tell the world the player's level, for its item rolls (`World::set_player_level`;
    /// not locks, and not saved). `self.world` is the played world during a game
    /// turn (the engine's `script::with_world`); called outside one it reaches only the
    /// stand-in, and the next tick puts it right.
    pub fn sync_player_level(&mut self) {
        self.world.set_player_level(self.list_player_level());
    }

    /// The level `r` reads leveled lists at (`fnv_sim::World::level_at`): fixed when its
    /// cell was loaded, else what it would be fixed at now.
    pub fn leveled_at(&self, r: FormId) -> LevelAt {
        self.world.level_at(r, self.list_player_level())
    }

    /// The reference whose FormID seeds the leveled picks of `r`: itself until
    /// ResurrectActor rolled them again (`fnv_sim::leveled::roll_seed`, `crate::resurrect`),
    /// as the engine's drawn actor and the carried items are seeded. Every group of data
    /// then comes from the same pick (docs/spec-notes/actors.md §1.3).
    pub fn pick_seed(&self, r: FormId) -> FormId {
        fnv_sim::leveled::roll_seed(r, self.extra.get(&r).map_or(0, |e| e.rolls))
    }

    /// The actor base that supplies a group of the data of `r` (placed with `base`), its
    /// leveled lists read at its level with its pick seed
    /// ([`crate::data::GameData::template_source_at`]).
    pub fn template_source(&self, base: FormId, r: FormId, from_template: fn(&ActorDef) -> bool) -> Option<(FormId, &'a ActorDef)> {
        let data: &'a crate::data::GameData = self.data;
        data.template_source_at(base, self.pick_seed(r), self.leveled_at(r), from_template)
    }

    /// The actor base whose name `r` (placed with `base`) is shown by, at its level.
    pub fn display_base(&self, base: FormId, r: FormId) -> FormId {
        self.data.display_base_at(base, self.pick_seed(r), self.leveled_at(r))
    }

    /// The base form whose voice type `r` (placed with `base`) speaks with: the "Traits"
    /// source of its template chain, at its level, so an actor whose traits come from a
    /// leveled list speaks with the voice of the entry picked for it (the one whose race
    /// and sex it shows; docs/spec-notes/voice.md §7). `base` itself for a talking
    /// activator or an actor without such a template. `GetIsVoiceType` reads the same
    /// source (`Game::traits_def`), so a line chosen for a voice is played in that voice.
    pub fn voice_base(&self, base: FormId, r: FormId) -> FormId {
        self.voice_base_in(&self.world, base, r)
    }

    /// [`Game::voice_base`] with `r`'s level read from `world`: for callers that hold the
    /// world themselves between frames (the engine, `script::with_world`).
    pub fn voice_base_in(&self, world: &fnv_sim::World, base: FormId, r: FormId) -> FormId {
        let at = world.level_at(r, self.list_player_level());
        // Seeded as the drawn actor and its carried items are: by the reference until
        // ResurrectActor rolled its picks again, so a resurrected actor speaks with the
        // voice of the entry it now shows.
        let seed = fnv_sim::leveled::roll_seed(r, self.extra.get(&r).map_or(0, |e| e.rolls));
        self.data.template_source_at(base, seed, at, |d| d.template_traits).map_or(base, |(id, _)| id)
    }

    /// `cell` comes into the loaded area: its encounter zone gets its level if it has none,
    /// and the actors and containers placed there are fixed at their level
    /// (`fnv_sim::zones`). A placed actor whose carried items come from a leveled pick takes
    /// them from the pick at that level, unless they were rolled already.
    pub(crate) fn fix_cell_levels(&mut self, cell: FormId) {
        let player_level = self.list_player_level();
        let placed = self.world.placed;
        // A script's zone for the cell or its worldspace first (`nvse_jg5::cell_zone`).
        let mut zones: Vec<FormId> = crate::nvse_jg5::cell_zone(self, cell).into_iter().collect();
        let refs: Vec<FormId> = placed.by_cell.get(&cell).cloned().unwrap_or_default();
        for &r in &refs {
            zones.extend(placed.get(r).and_then(|p| p.encounter_zone));
        }
        for z in zones {
            if let Some(level) = self.world.enter_zone(z, player_level) {
                let min_level = self.world.forms.zones.get(&z).map_or(0, |d| d.min_level);
                self.push_level_news(LevelNews::Zone { zone: z, level, min_level, player_level });
            }
        }
        for r in refs {
            self.fix_ref_level(r, player_level, false);
        }
    }

    /// Fix the level of the placed actor or container `r` (when not fixed yet). `reroll`:
    /// its carried items are about to be rolled again, so their source follows the new
    /// pick even though they were rolled before.
    fn fix_ref_level(&mut self, r: FormId, player_level: u16, reroll: bool) {
        let placed = self.world.placed;
        let Some(p) = placed.get(r) else { return };
        let actor = matches!(&p.sig, b"ACHR" | b"ACRE");
        if !actor && self.world.forms.sig(p.base) != Some(*b"CONT") {
            return;
        }
        let Some(at) = self.world.fix_level(r, player_level) else { return };
        if !actor {
            return;
        }
        // Carried items not rolled yet come from the pick at this level.
        if reroll || self.world.peek(r).is_none() {
            let source = self.data.template_source_at(p.base, self.pick_seed(r), at, |d| d.template_inventory).map(|(s, _)| s).filter(|&s| s != p.base);
            self.world.set_inventory_source(r, source);
        }
        let (base, list) = self.chain_end(p.base, crate::templates::Picks::at(self.pick_seed(r), at));
        if let Some(list) = list {
            let (at_display, _) = self.chain_end(p.base, crate::templates::Picks::new(self.pick_seed(r)));
            self.push_level_news(LevelNews::Actor { actor: r, list, at, base, at_display, zone: placed.zone_of(r) });
        }
    }

    /// The actor at the end of `base`'s template chain (every `TPLT` followed, whatever the
    /// flags), with `picks`; and the first leveled list met on the way (`None`: the chain
    /// meets none, the level does not matter to it).
    fn chain_end(&self, base: FormId, mut picks: crate::templates::Picks) -> (Option<FormId>, Option<FormId>) {
        let mut id = self.data.actor_base(base, &mut picks);
        for _ in 0..16 {
            let Some(next) = id.and_then(|i| self.data.actors.get(&i)).and_then(|d| d.template).and_then(|t| self.data.actor_base(t, &mut picks)) else { break };
            if Some(next) == id {
                break;
            }
            id = Some(next);
        }
        (id, picks.picked().first().map(|p| p.0))
    }

    /// `r` is made anew by a cell reset: its level is fixed again at the player's level
    /// now (its zone's level stays as it was).
    pub(crate) fn refix_level(&mut self, r: FormId) {
        self.world.unfix_level(r);
        let player_level = self.list_player_level();
        self.fix_ref_level(r, player_level, true);
    }

    /// Keep `n` for [`Game::take_level_news`] (at most [`MAX_NEWS`]: a caller that never
    /// takes them does not grow the list for ever).
    fn push_level_news(&mut self, n: LevelNews) {
        if self.level_news.len() < MAX_NEWS {
            self.level_news.push(n);
        }
    }

    /// The zone levels set and leveled actors fixed since the last call.
    pub fn take_level_news(&mut self) -> Vec<LevelNews> {
        std::mem::take(&mut self.level_news)
    }
}
