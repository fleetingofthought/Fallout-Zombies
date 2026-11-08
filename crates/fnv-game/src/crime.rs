//! Karma, ownership and crime (docs/spec-notes/crime-karma.md; owner's playtest list 2 of
//! 2026-10-12, "FACTION KARMA" and "CRIME").
//!
//! - **Karma** is the player's actor value Karma (23). An actor's alignment is its Karma
//!   against the globals `KarmaVeryEvil` (−750), `KarmaEvil` (−250), `KarmaGood` (250)
//!   and `KarmaVeryGood` (750) [observed: FalloutNV.esm; using them as the bounds:
//!   hypothesis by their names, §1]. Every change goes through [`Game::change_karma`],
//!   which leaves a [`Note::Karma`] for the engine's corner message ("A corner message is
//!   shown … indicating the change" [documented: GECK wiki RewardKarma]).
//! - **Ownership** (§2): a reference's own owner (XOWN, a faction with the rank XRNK
//!   needs), else its cell's (XOWN/XRNK of the CELL, or SetCellOwnership) [documented:
//!   GECK wiki IsOwner, "inherited ownership from the cell"; Reference, "Faction Rank
//!   required to own the reference"]. The player owns what is owned by the player's base
//!   or by a faction the player is in at the rank asked for.
//! - **Crimes** (§3) [documented: FalloutNV.esm message `HelpCrime`]: minor crimes (theft,
//!   and "trespass": opening an owned lock) make only the victim hostile, and only when
//!   it sees the crime; major crimes (assault, murder) the victim and the victim's
//!   friends. Who sees: an actor that has the player detected ([`crate::detection`],
//!   band Detected) [hypothesis: "sees" is detection]. A reacting actor becomes a
//!   **crime enemy** of the player (GetActorCrimePlayerEnemy): it attacks the player
//!   whatever its aggression ([`Game::crime_enemy`], read by `Game::would_attack`)
//!   [design, as an assaulted actor's grudge].
//! - **Karma for crimes and kills** (§4): stealing, and opening an owned lock, cost
//!   `fKarmaModStealing` (−5) unless the owner is Evil (an Evil faction, or an owning
//!   actor of Evil alignment) or a faction that does not track crime [documented: GECK
//!   wiki Category:Factions, Evil and Track Crime; Reference, "karma gain/loss is affected
//!   by the karma level of the owning actor"; the lock: design]; killing an Evil actor
//!   gives `fKarmaModKillingEvilActor` (100) and a Very Good one takes as much
//!   [observed: crime-karma.md §4.1, capture o6_02], a Very Evil one gives it and a Good
//!   one takes it [hypothesis], judged by the placed base's own ACBS karma, not a
//!   template's ([`Game::kill_alignment`]) [hypothesis, §4.1]; murdering a Neutral NPC
//!   takes a quarter of it [design]. NPCs only [design].
//! - **Trespassing** in a cell (§3.3): in an owned cell without the Public place flag
//!   that the player does not own [documented: GECK wiki SetCellPublicFlag, "Cells which
//!   are public cannot have trespassing"]; an owner who sees the player there warns
//!   (GuardTrespass) every `fAITrespassWarningTimer` (5 s) up to `iGuardWarnings` (2)
//!   times, then attacks [documented for Guard packages, GECK wiki Guard Package;
//!   hypothesis: the same for owned cells]. While it warns (it has warned and sees the
//!   player at the tick) it is alerted: it draws its weapon, stops and faces the
//!   player (`Game::warning_to`; `crate::aggro`, "The look") [hypothesis; owner
//!   2026-10-24: warning guards draw their weapon]. Guard packages' own warnings are not
//!   built.
//!   A door whose XTEL is flagged No Alarm
//!   sends "no trespass alarm … when the player uses the door" [documented: GECK wiki
//!   Reference, Teleport]: arriving through one, the player is not trespassing in the
//!   cell arrived in until leaving it ([`Game::arrived_through`]) [hypothesis: the alarm
//!   is the owners' warnings and attack; docs/owner-review.md].
//!
//! The engine tells the game what the player did ([`Game::player_stole`],
//! [`Game::player_opened_lock`], [`Game::crime_tick`]); kills and hits come through
//! `Game::kill` and `Game::attacked_by`.

use crate::detection::Band;
use crate::functions::actor::{actor_value, faction_rank, factions_of};
use crate::functions::{base_of, none, yes, Args, Ctx};
use crate::game::Game;
use crate::provocation::{ALLY, FRIEND};
use fnv_script::vm::Value;
use fnv_sim::{FormId, World};
use std::collections::BTreeMap;

const KARMA: u16 = crate::actor_values::KARMA;

/// FACT flags as [`crate::data::GameData::faction_flags`] keeps them.
pub mod faction_flags {
    pub const HIDDEN: u16 = 1;
    pub const EVIL: u16 = 2;
    pub const SPECIAL_COMBAT: u16 = 4;
    pub const TRACK_CRIME: u16 = 256;
    pub const ALLOW_SELL: u16 = 512;
}

/// CELL DATA flag "Public place" [documented: xEdit wbDefinitionsFNV.pas, schema
/// `world.json`].
pub const CELL_PUBLIC: u8 = 0x20;
pub const CELL_INTERIOR: u8 = 0x01;
/// The record header flag "Off limits" of an interior cell [documented: GECK wiki
/// GetFormFlag, bit 17].
pub const FORM_OFF_LIMITS: u32 = 0x0002_0000;
/// The record header flag "Can't wait" of a cell [documented: GECK wiki GetFormFlag,
/// bit 19] (docs/spec-notes/cells-refs.md §1).
pub const FORM_CANT_WAIT: u32 = 0x0008_0000;

/// What a cell's record says about who owns it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellDef {
    pub owner: Option<FormId>,
    pub rank: Option<i32>,
    pub public: bool,
    pub interior: bool,
    pub off_limits: bool,
    /// "Can't wait": the Wait menu refuses in it.
    pub cant_wait: bool,
}

impl CellDef {
    /// From the CELL's subrecords: DATA (flags), XOWN, XRNK, and the record header flags.
    pub fn from_raw(subs: &[fnv_esm::Subrecord], header_flags: u32, map: &dyn Fn(u32) -> Option<u32>) -> CellDef {
        let word = |sig: &[u8; 4]| subs.iter().find(|s| &s.sig == sig).and_then(|s| <[u8; 4]>::try_from(s.data.get(..4)?).ok()).map(u32::from_le_bytes);
        let flags = subs.iter().find(|s| &s.sig == b"DATA").and_then(|s| s.data.first().copied()).unwrap_or(0);
        CellDef {
            owner: word(b"XOWN").filter(|&l| l != 0).and_then(map).map(FormId),
            rank: word(b"XRNK").map(|r| r as i32),
            public: flags & CELL_PUBLIC != 0,
            interior: flags & CELL_INTERIOR != 0,
            off_limits: header_flags & FORM_OFF_LIMITS != 0,
            cant_wait: header_flags & FORM_CANT_WAIT != 0,
        }
    }

    /// Whether the cell is worth keeping: it has an owner or one of the flags.
    pub fn matters(&self) -> bool {
        self.owner.is_some() || self.public || self.off_limits || self.cant_wait
    }
}

/// Who owns something, and where that came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ownership {
    /// An NPC base or a faction.
    pub owner: FormId,
    /// For a faction: the rank needed to count as an owner.
    pub rank: Option<i32>,
    /// Inherited from the cell.
    pub from_cell: bool,
}

/// An alignment [documented: GECK wiki GetIsAlignment, "VeryGood, Good, Neutral, Evil,
/// VeryEvil"].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Alignment {
    VeryEvil,
    Evil,
    Neutral,
    Good,
    VeryGood,
}

impl Alignment {
    pub fn name(self) -> &'static str {
        match self {
            Alignment::VeryEvil => "Very Evil",
            Alignment::Evil => "Evil",
            Alignment::Neutral => "Neutral",
            Alignment::Good => "Good",
            Alignment::VeryGood => "Very Good",
        }
    }

    /// Karma against the bounds (very evil, evil, good, very good): at or beyond a bound
    /// is that side [hypothesis].
    pub fn of(karma: f64, b: [f64; 4]) -> Alignment {
        match karma {
            k if k <= b[0] => Alignment::VeryEvil,
            k if k <= b[1] => Alignment::Evil,
            k if k >= b[3] => Alignment::VeryGood,
            k if k >= b[2] => Alignment::Good,
            _ => Alignment::Neutral,
        }
    }

    /// GetIsAlignment's parameter code: 0 Good, 1 Neutral, 2 Evil, 3 Very Good, 4 Very
    /// Evil [hypothesis: xEdit's alignment enumeration as remembered; not checked].
    pub fn from_code(c: u16) -> Option<Alignment> {
        Some(match c {
            0 => Alignment::Good,
            1 => Alignment::Neutral,
            2 => Alignment::Evil,
            3 => Alignment::VeryGood,
            4 => Alignment::VeryEvil,
            _ => return None,
        })
    }

    pub fn evil(self) -> bool {
        self <= Alignment::Evil
    }

    pub fn good(self) -> bool {
        self >= Alignment::Good
    }
}

/// The crimes (§3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Taking an owned item.
    Theft,
    /// Opening an owned lock (picked or forced) ("Trespass" in the game's help).
    Lock,
    /// Being in an owned, not public cell.
    Trespass,
    Assault,
    Murder,
    /// Caught picking an actor's pocket (`crate::pickpocket`).
    Pickpocket,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Theft => "theft",
            Kind::Lock => "opening an owned lock",
            Kind::Trespass => "trespassing",
            Kind::Assault => "assault",
            Kind::Murder => "murder",
            Kind::Pickpocket => "pickpocketing",
        }
    }

    /// Assault and murder are major crimes [documented: HelpCrime].
    pub fn major(self) -> bool {
        matches!(self, Kind::Assault | Kind::Murder)
    }

    /// The topic a witness says [documented: GECK wiki Combat Tab, Steal, Murder,
    /// GuardTrespass; Assault is provocation's]. None: it attacks at once [documented:
    /// HelpCrime, an opened lock].
    pub fn topic(self) -> Option<&'static str> {
        match self {
            Kind::Theft => Some("Steal"),
            Kind::Murder => Some("Murder"),
            Kind::Trespass => Some("GuardTrespass"),
            Kind::Pickpocket => Some("Pickpocket"),
            Kind::Lock | Kind::Assault => None,
        }
    }

    /// The topic a witness that ignores crime says instead: "the same as …, but the actor
    /// doesn't regard it as a crime" [documented: GECK wiki Combat Tab, AssaultNoCrime,
    /// MurderNoCrime, PickpocketNoCrime, StealNoCrime, TrespassNoCrime; #974]. None for an
    /// opened lock (no such topic).
    pub fn no_crime_topic(self) -> Option<&'static str> {
        match self {
            Kind::Theft => Some("StealNoCrime"),
            Kind::Murder => Some("MurderNoCrime"),
            Kind::Trespass => Some("TrespassNoCrime"),
            Kind::Pickpocket => Some("PickpocketNoCrime"),
            Kind::Assault => Some("AssaultNoCrime"),
            Kind::Lock => None,
        }
    }
}

/// What happened, for the engine's log, corner messages and summary.
#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    /// The player's karma changed by `change` to `now`.
    Karma { change: f64, now: f64, why: String },
    /// A crime: against `victim` (an actor, an NPC base or a faction), seen by `seen_by`,
    /// who turned on the player: `hostile`.
    Crime { kind: Kind, victim: Option<FormId>, seen_by: Vec<FormId>, hostile: Vec<FormId> },
    /// `actor` warned the trespassing player (`level` warnings so far).
    Warned { actor: FormId, level: u32 },
    /// The player is now an enemy of this crime-tracking faction.
    FactionEnemy { faction: FormId },
    /// The player started or stopped trespassing in `cell` (owned by `owner`).
    Trespassing { cell: FormId, owner: FormId, now: bool },
    /// The player came through `door`, flagged No Alarm, into `cell`: no trespass alarm
    /// there until the player leaves it.
    NoAlarm { door: FormId, cell: FormId },
    /// A seen theft or kill raised the player's infamy with `reputation`
    /// (`functions::crime_infamy`).
    Infamy { reputation: FormId, before: f64, after: f64 },
}

/// The settings of karma and crime (§1, §4) with the values FalloutNV.esm gives them
/// [observed 2026-10-12]; the wiki's default for the trespass timer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub stealing: f64,
    pub killing_evil: f64,
    pub guard_warnings: f64,
    pub warning_timer: f64,
    /// KarmaVeryEvil, KarmaEvil, KarmaGood, KarmaVeryGood.
    pub bounds: [f64; 4],
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { stealing: -5.0, killing_evil: 100.0, guard_warnings: 2.0, warning_timer: 5.0, bounds: [-750.0, -250.0, 250.0, 750.0] }
    }
}

impl Settings {
    pub fn read(g: &Game) -> Settings {
        let d = Settings::default();
        let s = |name: &str, v: f64| crate::combat::setting(g, name, v);
        let global = |name: &str, v: f64| g.world.forms.global_by_edid(name).and_then(|(id, _)| g.world.global(id).ok()).map_or(v, f64::from);
        Settings {
            stealing: s("fKarmaModStealing", d.stealing),
            killing_evil: s("fKarmaModKillingEvilActor", d.killing_evil),
            guard_warnings: s("iGuardWarnings", d.guard_warnings),
            warning_timer: s("fAITrespassWarningTimer", d.warning_timer),
            bounds: [
                global("KarmaVeryEvil", d.bounds[0]),
                global("KarmaEvil", d.bounds[1]),
                global("KarmaGood", d.bounds[2]),
                global("KarmaVeryGood", d.bounds[3]),
            ],
        }
    }
}

/// Murdering a Neutral NPC costs this share of `fKarmaModKillingEvilActor` [design].
pub const NEUTRAL_MURDER_SHARE: f64 = 0.25;

/// The karma for the player's kill of an actor of `alignment` (the kill's,
/// [`Game::kill_alignment`]): an Evil or Very Evil NPC gives `fKarmaModKillingEvilActor`
/// [observed: −500 victims gave +100, capture o6_02 2026-10-31, crime-karma.md §4.1;
/// Very Evil: hypothesis, the setting's name], a Good or Very Good one takes as much
/// [observed for Very Good (900): −100, o6_02; Good: hypothesis], a `murder`ed Neutral one a
/// quarter of it [design]. Creatures give none: their karma includes plain animals (a
/// dead radroach is Evil in FalloutNV.esm) [design].
pub fn kill_karma(alignment: Alignment, murder: bool, npc: bool, s: &Settings) -> f64 {
    match alignment {
        _ if !npc => 0.0,
        a if a.evil() => s.killing_evil,
        a if a.good() => -s.killing_evil,
        _ if murder => -s.killing_evil * NEUTRAL_MURDER_SHARE,
        _ => 0.0,
    }
}

/// The run-time side of crime: notes for the engine, trespass warnings, the latest
/// victim. What lasts (counts, crime enemies, the murderer flag) is in
/// `PlayerState`.
#[derive(Debug, Clone, Default)]
pub struct Crimes {
    pub notes: Vec<Note>,
    pub settings: Option<Settings>,
    /// Warnings each actor gave the trespassing player, and when it gave the last.
    pub warnings: BTreeMap<FormId, (u32, f64)>,
    /// Actors warning the trespassing player now: they have warned it and saw it at the
    /// last crime tick, and have not turned on it (`Game::warning_to`).
    pub warning_now: std::collections::BTreeSet<FormId>,
    /// When each witness that ignores crime last said TrespassNoCrime: once per
    /// fAITrespassWarningTimer, as the warnings are paced (#974).
    pub no_crime_trespass: BTreeMap<FormId, f64>,
    /// The cell the player trespasses in, with its owner.
    pub trespassing: Option<(FormId, FormId)>,
    /// The cell the player arrived in through a No Alarm door, while the player is in it
    /// ([`Game::arrived_through`]).
    pub no_alarm_cell: Option<FormId>,
    /// The current crime victim (IsActorAVictim).
    pub victim: Option<FormId>,
}

impl Crimes {
    pub fn take_notes(&mut self) -> Vec<Note> {
        std::mem::take(&mut self.notes)
    }
}

impl Game<'_> {
    pub fn crime_settings(&mut self) -> Settings {
        match self.crimes.settings {
            Some(s) => s,
            None => {
                let s = Settings::read(self);
                self.crimes.settings = Some(s);
                s
            }
        }
    }

    // ---- karma ---------------------------------------------------------------------

    /// An actor's alignment by its Karma (module notes). For an NPC base (an owner), its
    /// record's karma.
    pub fn alignment(&mut self, r: FormId) -> Alignment {
        let b = self.crime_settings().bounds;
        let karma = if self.data.actors.contains_key(&r) {
            self.template_source(r, r, |d| d.template_traits).map_or(self.data.actors[&r].karma, |(_, d)| d.karma)
        } else {
            actor_value(self, r, KARMA).unwrap_or(0.0)
        };
        Alignment::of(karma, b)
    }

    /// The alignment a kill of `r` is judged by: its placed base's own ACBS karma (a
    /// leveled base read as drawn for `r`), not a template's whose traits it takes
    /// [hypothesis: an NCRCF convict holding 0 itself and −500 only through its template
    /// gave no karma, a Fiend and a Powder Ganger holding −500 themselves gave +100,
    /// crime-karma.md §4.1]. [`Game::alignment`] when `r` has no actor base.
    pub fn kill_alignment(&mut self, r: FormId) -> Alignment {
        let own = base_of(self, r).filter(|_| r != FormId::PLAYER_REF).and_then(|b| self.data.template_source_at(b, self.pick_seed(r), self.leveled_at(r), |_| false)).map(|(_, d)| d.karma);
        match own {
            Some(karma) => Alignment::of(karma, self.crime_settings().bounds),
            None => self.alignment(r),
        }
    }

    /// Change the player's karma by `change`, within iKarmaMin and iKarmaMax when a
    /// plugin defines them [documented: GECK wiki RewardKarma]; leaves a note for the
    /// corner message. Returns the karma after.
    pub fn change_karma(&mut self, change: f64, why: impl Into<String>) -> f64 {
        let p = FormId::PLAYER_REF;
        let before = actor_value(self, p, KARMA).unwrap_or(0.0);
        let mut v = before + change;
        if let Some(max) = self.world.setting_f32("iKarmaMax") {
            v = v.min(max as f64);
        }
        if let Some(min) = self.world.setting_f32("iKarmaMin") {
            v = v.max(min as f64);
        }
        if v != before {
            let _ = crate::functions::actor::shift_base(self, p, KARMA, v - before);
        }
        self.crimes.notes.push(Note::Karma { change: v - before, now: v, why: why.into() });
        v
    }

    // ---- ownership -----------------------------------------------------------------

    /// Who owns a cell: SetCellOwnership's owner, else its record's.
    pub fn cell_ownership(&self, cell: FormId) -> Option<Ownership> {
        if let Some(&o) = self.forms.cell_owners.get(&cell) {
            return Some(Ownership { owner: o, rank: None, from_cell: true });
        }
        let c = self.data.cells.get(&cell)?;
        c.owner.map(|owner| Ownership { owner, rank: c.rank, from_cell: true })
    }

    /// Whether a cell is public: SetCellPublicFlag's, else its record's flag.
    /// Whether the player's cell is flagged "Can't wait" (cells-refs.md §1).
    pub fn cant_wait_here(&self) -> Option<FormId> {
        self.cell_of(FormId::PLAYER_REF).filter(|c| self.data.cells.get(c).is_some_and(|d| d.cant_wait))
    }

    pub fn cell_public(&self, cell: FormId) -> bool {
        self.forms.public_cells.get(&cell).copied().unwrap_or_else(|| self.data.cells.get(&cell).is_some_and(|c| c.public))
    }

    /// Who owns a reference (module notes): its own owner, else its cell's. A reference
    /// made at run time (a dropped item) has no cell to inherit from [design].
    pub fn ownership(&self, r: FormId) -> Option<Ownership> {
        self.ownership_in(&self.world, r)
    }

    /// [`Game::ownership`] with the references' state of `world` (the engine keeps the
    /// world outside the game between script runs).
    pub fn ownership_in(&self, world: &World<'_>, r: FormId) -> Option<Ownership> {
        let state = world.peek(r);
        let own = match state {
            Some(s) => s.extra.owner,
            None => world.placed.get(r).and_then(|p| p.owner),
        };
        if let Some(owner) = own {
            return Some(Ownership { owner, rank: self.data.owner_ranks.get(&r).copied(), from_cell: false });
        }
        let placed = world.placed.get(r)?;
        let cell = state.and_then(|s| s.cell).or(placed.cell)?;
        self.cell_ownership(cell)
    }

    /// Whether `actor` (the player, or an actor reference) counts as an owner: the owner
    /// is its base, or a faction it is in at the rank asked for [documented: GECK wiki
    /// Reference ownership, Faction Rank; HasOwnership].
    pub fn counts_as_owner(&self, actor: FormId, o: &Ownership) -> bool {
        let base = if actor == FormId::PLAYER_REF { Some(FormId::PLAYER_BASE) } else { base_of(self, actor) };
        if base == Some(o.owner) || o.owner == actor {
            return true;
        }
        self.is_faction(o.owner) && faction_rank(self, actor, o.owner) >= o.rank.unwrap_or(0).max(0)
    }

    fn is_faction(&self, f: FormId) -> bool {
        self.data.sig(f).is_some_and(|s| &s == b"FACT")
    }

    /// The owner of `r` when it is someone other than the player: taking it is theft.
    pub fn owned_by_other(&self, r: FormId) -> Option<Ownership> {
        self.ownership(r).filter(|o| !self.counts_as_owner(FormId::PLAYER_REF, o))
    }

    /// Whether `witness` stands for the owner: it is the owning NPC, or a member of the
    /// owning faction [hypothesis: any rank].
    fn speaks_for(&self, witness: FormId, o: &Ownership) -> bool {
        if base_of(self, witness) == Some(o.owner) || witness == o.owner {
            return true;
        }
        self.is_faction(o.owner) && faction_rank(self, witness, o.owner) >= 0
    }

    /// Whether stealing from this owner costs karma (module notes).
    pub(crate) fn karma_for_theft_from(&mut self, o: &Ownership) -> bool {
        self.karma_for_theft(o)
    }

    /// `actor` turns on the player for its own crime: counted against it, and it says
    /// the crime's line (`crate::pickpocket`). Whether it turned now.
    pub(crate) fn crime_turn(&mut self, actor: FormId, kind: Kind) -> bool {
        self.count_crime(actor, kind);
        self.turn_on_player(actor, kind)
    }

    fn karma_for_theft(&mut self, o: &Ownership) -> bool {
        if self.is_faction(o.owner) {
            let f = self.data.faction_flags.get(&o.owner).copied().unwrap_or(0);
            return f & faction_flags::TRACK_CRIME != 0 && f & faction_flags::EVIL == 0;
        }
        !self.alignment(o.owner).evil()
    }

    // ---- witnesses and crime enemies ------------------------------------------------

    /// The actors that see the player now: they have the player detected (band
    /// Detected, measured within the last second), live, are present, are not the
    /// player's teammates and do not ignore crime.
    pub fn crime_witnesses(&self) -> Vec<FormId> {
        let now = self.time;
        // Those who saw the player while a planted explosive was live, when its kill is
        // judged (`crate::pickpocket`).
        let planted = self.pickpocket.blast_witnesses.iter().copied().filter(|w| !self.detection.pairs.get(&(*w, FormId::PLAYER_REF)).is_some_and(|(d, at)| d.band == Band::Detected && now - at <= 1.0));
        self.detection
            .pairs
            .iter()
            .filter(|((w, t), (d, at))| *t == FormId::PLAYER_REF && *w != FormId::PLAYER_REF && d.band == Band::Detected && now - at <= 1.0)
            .map(|((w, _), _)| *w)
            .chain(planted)
            .filter(|&w| self.present(w) && !self.is_dead(w) && !self.extra.get(&w).is_some_and(|e| e.teammate || e.ignore_crime || e.out_cold()))
            .collect()
    }

    /// The actors that see the player now and would be witnesses ([`Game::crime_witnesses`])
    /// but ignore crime: they say the crime's NoCrime topic instead (#974).
    pub fn crime_ignorers(&self) -> Vec<FormId> {
        let now = self.time;
        self.detection
            .pairs
            .iter()
            .filter(|((w, t), (d, at))| *t == FormId::PLAYER_REF && *w != FormId::PLAYER_REF && d.band == Band::Detected && now - at <= 1.0)
            .map(|((w, _), _)| *w)
            .filter(|&w| self.present(w) && !self.is_dead(w) && self.extra.get(&w).is_some_and(|e| e.ignore_crime && !e.teammate && !e.out_cold()))
            .collect()
    }

    /// `actor`, which ignores crime, says `kind`'s NoCrime topic to the player.
    fn say_no_crime(&mut self, actor: FormId, kind: Kind) {
        if let Some(topic) = kind.no_crime_topic() {
            self.detection_bark(actor, Some(FormId::PLAYER_REF), topic);
        }
    }

    /// Whether `actor` attacks the player because of crimes: the player's crimes made it
    /// an enemy, or the player is an enemy of one of its factions (SetPCEnemyOfFaction,
    /// a witnessed murder) [documented: GECK wiki GetActorCrimePlayerEnemy,
    /// GetActorFactionPlayerEnemy]; never when it ignores crime [documented: IgnoreCrime].
    pub fn crime_enemy(&self, actor: FormId) -> bool {
        if self.extra.get(&actor).is_some_and(|e| e.ignore_crime) {
            return false;
        }
        self.player.crime_enemies.contains(&actor) || (!self.player.enemy_of.is_empty() && factions_of(self, actor).iter().any(|f| self.player.enemy_of.contains(f)))
    }

    fn count_crime(&mut self, victim: FormId, kind: Kind) {
        let c = self.player.crime_counts.entry(victim).or_default();
        c[kind.major() as usize] += 1;
        self.crimes.victim = Some(victim);
    }

    /// `actor` turns on the player for a crime: it becomes a crime enemy and says the
    /// crime's line.
    fn turn_on_player(&mut self, actor: FormId, kind: Kind) -> bool {
        if !self.player.crime_enemies.insert(actor) {
            return false;
        }
        if let Some(topic) = kind.topic() {
            self.detection_bark(actor, Some(FormId::PLAYER_REF), topic);
        }
        true
    }

    // ---- the crimes ----------------------------------------------------------------

    /// The player took `count` of `item`: the reference `reference` from the world, or
    /// from the container `container`. Theft when someone else owns it (§3.1): karma
    /// unless the owner is Evil, and the owner's side turns on the player when it sees
    /// it. Returns whether it was theft.
    pub fn player_stole(&mut self, reference: Option<FormId>, container: Option<FormId>, item: FormId, count: i32) -> bool {
        let Some(o) = container.or(reference).and_then(|r| self.owned_by_other(r)) else { return false };
        let s = self.crime_settings();
        if self.karma_for_theft(&o) {
            let name = self.data.name(item);
            self.change_karma(s.stealing, format!("stealing {name} x{count}"));
        }
        self.witnessed(Kind::Theft, &o);
        true
    }

    /// The player picked or forced the lock of `reference` open (lockpicking's crime
    /// hook): when it is owned, karma as for theft [design] and the owner's side that
    /// sees it attacks at once [documented: HelpCrime, "Trespass"]. Returns the owner.
    pub fn player_opened_lock(&mut self, reference: FormId) -> Option<Ownership> {
        let o = self.owned_by_other(reference)?;
        let s = self.crime_settings();
        if self.karma_for_theft(&o) {
            self.change_karma(s.stealing, format!("opening the lock of {reference}"));
        }
        self.witnessed(Kind::Lock, &o);
        Some(o)
    }

    /// A minor crime against `o`: every witness that speaks for the owner turns on the
    /// player; the crime counts against each of them (or the owner when none saw it).
    fn witnessed(&mut self, kind: Kind, o: &Ownership) {
        let seen_by = self.crime_witnesses();
        let owners: Vec<FormId> = seen_by.iter().copied().filter(|&w| self.speaks_for(w, o)).collect();
        // A faction that does not track crime does not remember it [documented: GECK wiki
        // Category:Factions, Track Crime].
        let tracks = !self.is_faction(o.owner) || self.data.faction_flags.get(&o.owner).is_some_and(|f| f & faction_flags::TRACK_CRIME != 0);
        let mut hostile = Vec::new();
        if tracks {
            for &w in &owners {
                self.count_crime(w, kind);
                if self.turn_on_player(w, kind) {
                    hostile.push(w);
                }
            }
        }
        if owners.is_empty() {
            self.count_crime(o.owner, kind);
        }
        // Those that would speak for the owner but ignore crime say so (#974).
        for w in self.crime_ignorers() {
            if self.speaks_for(w, o) {
                self.say_no_crime(w, kind);
            }
        }
        let seen = !seen_by.is_empty();
        self.crimes.notes.push(Note::Crime { kind, victim: Some(o.owner), seen_by, hostile });
        self.crime_infamy(kind, o.owner, seen);
    }

    /// Infamy with the victim's reputations for a seen theft or kill, noted.
    fn crime_infamy(&mut self, kind: Kind, victim: FormId, seen: bool) {
        for (reputation, before, after) in crate::functions::crime_infamy(self, kind, victim, seen) {
            self.crimes.notes.push(Note::Infamy { reputation, before, after });
        }
    }

    /// The player is about to kill `victim` (`Game::kill`, before it is dead): karma by
    /// its alignment, and murder when it is a non-Evil NPC that was not fighting the
    /// player of its own accord (§3.4, §4).
    pub fn player_kills(&mut self, victim: FormId) {
        if victim == FormId::PLAYER_REF || self.is_dead(victim) {
            return;
        }
        let s = self.crime_settings();
        let alignment = self.alignment(victim);
        let npc = base_of(self, victim).and_then(|b| self.data.actors.get(&b)).is_some_and(|d| &d.sig == b"NPC_");
        // Self-defence: it attacked the player by its records, a script or a crime of the
        // player's; a fight the player started (a grudge) is no defence.
        // One fighting the player without a grudge chose that fight (item 22) [design].
        let grudge = self.provocations.holds(victim, FormId::PLAYER_REF);
        let fights_player = self.awareness.get(victim).is_some_and(|a| a.state.in_combat() && a.targets.contains_key(&FormId::PLAYER_REF));
        let aggressor = self.awareness.forced.contains(&(victim, FormId::PLAYER_REF)) || (!grudge && (fights_player || self.would_attack(victim, FormId::PLAYER_REF)));
        let murder = npc && !alignment.evil() && (!aggressor || self.player.crime_enemies.contains(&victim));
        let name = self.data.name(victim);
        // Karma by the victim's own record (§4.1): murder stays judged by its alignment
        // as scripts read it (GetIsAlignment) [design].
        let judged = self.kill_alignment(victim);
        let change = kill_karma(judged, murder, npc, &s);
        if change != 0.0 {
            self.change_karma(change, format!("killing {name} ({})", judged.name()));
        }
        // Any kill of a reputation's people while someone (the victim too) sees the
        // player, fighting or not [documented: Fallout wiki "Reputation (Fallout: New
        // Vegas)", killing members openly; only a kill from a hidden state gives none].
        let seen = !self.crime_witnesses().is_empty();
        self.crime_infamy(Kind::Murder, victim, seen);
        if !murder {
            return;
        }
        self.player.murderer = true;
        self.count_crime(victim, Kind::Murder);
        let seen_by: Vec<FormId> = self.crime_witnesses().into_iter().filter(|&w| w != victim).collect();
        // The victim's allies and friends that ignore crime say MurderNoCrime (#974).
        for w in self.crime_ignorers() {
            if w != victim && matches!(self.reaction_to(w, victim), ALLY | FRIEND) {
                self.say_no_crime(w, Kind::Murder);
            }
        }
        let mut hostile = Vec::new();
        for &w in &seen_by {
            if matches!(self.reaction_to(w, victim), ALLY | FRIEND) && self.turn_on_player(w, Kind::Murder) {
                hostile.push(w);
            }
        }
        // A major crime against a crime-tracking faction makes the player its enemy
        // [documented: GECK wiki SetPCEnemyOfFaction; hypothesis: when a witness who
        // cares saw it].
        if !hostile.is_empty() {
            for f in factions_of(self, victim) {
                let tracks = self.data.faction_flags.get(&f).is_some_and(|x| x & faction_flags::TRACK_CRIME != 0);
                if tracks && self.player.enemy_of.insert(f) {
                    self.crimes.notes.push(Note::FactionEnemy { faction: f });
                    crate::functions::actor::forget_relations(self);
                }
            }
        }
        self.crimes.notes.push(Note::Crime { kind: Kind::Murder, victim: Some(victim), seen_by, hostile });
    }

    /// The player assaulted `victim` (provocation turned it on the player): a major crime
    /// against it [documented: HelpCrime]. Its friends join through assistance
    /// (`crate::provocation`).
    pub fn player_assaulted(&mut self, victim: FormId) {
        // A victim that ignores crime says AssaultNoCrime (#974).
        if self.extra.get(&victim).is_some_and(|e| e.ignore_crime) {
            self.say_no_crime(victim, Kind::Assault);
        }
        self.count_crime(victim, Kind::Assault);
        self.crimes.notes.push(Note::Crime { kind: Kind::Assault, victim: Some(victim), seen_by: Vec::new(), hostile: vec![victim] });
    }

    /// The player ate `victim`'s corpse (Cannibal, `crate::activate_choices`): "a crime
    /// against nature" when seen [documented: PERK Cannibal DESC]; every witness turns
    /// on the player as for an assault [hypothesis]. Returns how many did.
    pub fn player_cannibalized(&mut self, victim: FormId) -> usize {
        let seen_by = self.crime_witnesses();
        let hostile: Vec<FormId> = seen_by.iter().copied().filter(|&w| self.turn_on_player(w, Kind::Assault)).collect();
        let n = hostile.len();
        if !seen_by.is_empty() {
            self.crimes.notes.push(Note::Crime { kind: Kind::Assault, victim: Some(victim), seen_by, hostile });
        }
        n
    }

    /// The player came through door `door` into `cell` (`None`: not known): a door
    /// flagged No Alarm (REFR XTEL) excuses the player's trespassing in that cell until
    /// the player leaves it; any other door ends an earlier excuse (module notes).
    pub fn arrived_through(&mut self, door: FormId, cell: Option<FormId>) {
        self.note_player_door(door);
        let no_alarm = self.world.placed.get(door).and_then(|p| p.teleport).is_some_and(|t| t.no_alarm);
        self.crimes.no_alarm_cell = cell.filter(|_| no_alarm);
        if let Some(cell) = self.crimes.no_alarm_cell {
            self.crimes.notes.push(Note::NoAlarm { door, cell });
        }
    }

    /// Where the player trespasses now: an owned cell, not public, that the player does
    /// not own, unless the player came in through a No Alarm door (module notes).
    pub fn trespass_cell(&self) -> Option<(FormId, Ownership)> {
        let cell = self.cell_of(FormId::PLAYER_REF)?;
        if self.crimes.no_alarm_cell == Some(cell) {
            return None;
        }
        let o = self.cell_ownership(cell)?;
        (!self.cell_public(cell) && !self.counts_as_owner(FormId::PLAYER_REF, &o)).then_some((cell, o))
    }

    /// After a round of detection measurements: trespass warnings and attacks (module
    /// notes). Returns how many actors turned on the player.
    pub fn crime_tick(&mut self) -> usize {
        let s = self.crime_settings();
        let now = self.time;
        // The No Alarm door's excuse ends once the player is out of its cell.
        if self.crimes.no_alarm_cell.is_some() && self.crimes.no_alarm_cell != self.cell_of(FormId::PLAYER_REF) {
            self.crimes.no_alarm_cell = None;
        }
        let here = self.trespass_cell();
        let key = here.map(|(c, o)| (c, o.owner));
        if key != self.crimes.trespassing {
            if let Some((cell, owner)) = self.crimes.trespassing {
                self.crimes.notes.push(Note::Trespassing { cell, owner, now: false });
            }
            if let Some((cell, owner)) = key {
                self.crimes.notes.push(Note::Trespassing { cell, owner, now: true });
            }
            self.crimes.trespassing = key;
            self.crimes.warnings.clear();
        }
        self.crimes.warning_now.clear();
        let Some((_, o)) = here else { return 0 };
        let mut turned = 0;
        // The owner's people that ignore crime say TrespassNoCrime, once per warning
        // interval (#974).
        for w in self.crime_ignorers() {
            let last = self.crimes.no_crime_trespass.get(&w).copied().unwrap_or(f64::NEG_INFINITY);
            if self.speaks_for(w, &o) && now - last >= s.warning_timer {
                self.crimes.no_crime_trespass.insert(w, now);
                self.say_no_crime(w, Kind::Trespass);
            }
        }
        for w in self.crime_witnesses() {
            if !self.speaks_for(w, &o) || self.player.crime_enemies.contains(&w) {
                continue;
            }
            let (level, last) = self.crimes.warnings.get(&w).copied().unwrap_or((0, f64::NEG_INFINITY));
            if now - last < s.warning_timer {
                self.crimes.warning_now.insert(w);
                continue;
            }
            if (level as f64) < s.guard_warnings {
                self.crimes.warnings.insert(w, (level + 1, now));
                self.crimes.warning_now.insert(w);
                self.detection_bark(w, Some(FormId::PLAYER_REF), "GuardTrespass");
                self.crimes.notes.push(Note::Warned { actor: w, level: level + 1 });
            } else {
                self.count_crime(w, Kind::Trespass);
                if self.player.crime_enemies.insert(w) {
                    turned += 1;
                    self.crimes.notes.push(Note::Crime { kind: Kind::Trespass, victim: Some(o.owner), seen_by: vec![w], hostile: vec![w] });
                }
            }
        }
        turned
    }

    /// A crime-enemy actor that StopCombat calmed forgets the player's crimes against it
    /// [design: as a grudge ends].
    pub fn forgive_crimes(&mut self, actor: FormId) {
        self.player.crime_enemies.remove(&actor);
        self.crimes.warnings.remove(&actor);
    }
}

// ---- script functions ----------------------------------------------------------------

/// `RewardKarma value` [documented: GECK wiki RewardKarma].
pub fn reward_karma(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let v = a.num(0)?;
    g.change_karma(v, "a script (RewardKarma)");
    none()
}

/// `actor.GetIsAlignment alignment` (module notes, [`Alignment::from_code`]).
pub fn get_is_alignment(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let want = Alignment::from_code(a.code(0)?).ok_or("unknown alignment code")?;
    yes(g.alignment(r) == want)
}

/// `IsPCAMurderer` [documented: "the player has ever murdered a non-Evil NPC"].
pub fn is_pc_a_murderer(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.player.murderer)
}

/// `actor.GetMinorCrimeCount` [documented].
pub fn get_minor_crime_count(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(g.player.crime_counts.get(&r).map_or(0, |c| c[0]) as f64))
}

/// `actor.GetMajorCrimeCount` [documented].
pub fn get_major_crime_count(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(g.player.crime_counts.get(&r).map_or(0, |c| c[1]) as f64))
}

/// `actor.GetActorCrimePlayerEnemy` [documented: an enemy because of crimes against that
/// actor].
pub fn get_actor_crime_player_enemy(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.player.crime_enemies.contains(&r))
}

/// `actor.GetAlarmed`: 1 while the actor is a crime enemy of the player [hypothesis: the
/// wiki says only "alarmed"].
pub fn get_alarmed(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.crime_enemy(r))
}

/// `actor.IsActorAVictim`: the current crime victim [documented].
pub fn is_actor_a_victim(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.crimes.victim == Some(r))
}

/// The owner a function's argument names: an actor reference stands for its base, the
/// player for the player's base; none means the player [documented: IsCellOwner,
/// SetOwnership].
fn owner_arg(g: &Game, a: &Args, i: usize) -> FormId {
    match a.opt_form(i) {
        None | Some(FormId::PLAYER_REF) => FormId::PLAYER_BASE,
        Some(f) if g.world.placed.get(f).is_some() => base_of(g, f).unwrap_or(f),
        Some(f) => f,
    }
}

/// `IsCellOwner cell [owner]` [documented].
pub fn is_cell_owner(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let cell = a.form(0)?;
    let want = owner_arg(g, a, 1);
    yes(g.cell_ownership(cell).is_some_and(|o| o.owner == want))
}

/// `actor.IsInMyOwnedCell`: the actor, or one of its factions, owns its cell
/// [documented: notes].
pub fn is_in_my_owned_cell(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    let o = g.cell_of(r).and_then(|c| g.cell_ownership(c));
    yes(o.is_some_and(|o| g.counts_as_owner(r, &Ownership { rank: None, ..o })))
}

/// `actor.IsTrespassing` (the player only; module notes).
pub fn is_trespassing(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    yes(r == FormId::PLAYER_REF && g.trespass_cell().is_some())
}

/// `actor.GetTrespassWarningLevel target`: the warnings the actor gave the player, by a
/// Guard package, for trespassing or for its aggro radius [documented: "the number of
/// warnings an actor on a Guard Package (or responding to trespassing or responding to an
/// aggro radius violation) has given"]; the larger count of the two kept.
pub fn get_trespass_warning_level(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if a.opt_form(0).is_some_and(|t| t != FormId::PLAYER_REF) {
        return Ok(Value::Num(0.0));
    }
    let trespass = g.crimes.warnings.get(&r).map_or(0, |w| w.0);
    let guard = g.provocations.aggro_warnings.get(&r).map_or(0, |w| w.0);
    Ok(Value::Num(trespass.max(guard) as f64))
}

/// `[reportedTo.]SendTrespassAlarm [reportedTo]`: that actor goes "all the way to fully
/// trespassed" at once [documented: notes] — it turns on the player.
pub fn send_trespass_alarm(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let to = a.opt_form(0).filter(|&f| f != FormId::PLAYER_REF).or(ctx.on.or(ctx.self_ref)).filter(|&f| f != FormId::PLAYER_REF).ok_or("no actor to report to")?;
    g.count_crime(to, Kind::Trespass);
    let hostile = if g.turn_on_player(to, Kind::Trespass) { vec![to] } else { Vec::new() };
    g.crimes.notes.push(Note::Crime { kind: Kind::Trespass, victim: Some(to), seen_by: vec![to], hostile });
    none()
}

/// `[criminal.]SendAssaultAlarm [victim] [faction]`: the victim reacts as to an assault;
/// with the player (or none) as the victim, a present member of the faction does
/// [documented: GECK wiki SendAssaultAlarm]. It says Assault "even if … already in
/// combat" [documented: notes].
pub fn send_assault_alarm(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let named = a.opt_form(0).filter(|&f| f != FormId::PLAYER_REF);
    let caller = ctx.on.or(ctx.self_ref).filter(|&f| f != FormId::PLAYER_REF);
    let victim = named.or_else(|| {
        let f = a.opt_form(1)?;
        let mut members: Vec<FormId> = g.world.placed.refs.iter().filter(|(_, p)| matches!(&p.sig, b"ACHR" | b"ACRE")).map(|(r, _)| *r).filter(|&r| g.present(r) && !g.is_dead(r) && faction_rank(g, r, f) >= 0).collect();
        members.sort();
        members.into_iter().next()
    });
    let victim = victim.or(caller).ok_or("no victim")?;
    g.count_crime(victim, Kind::Assault);
    g.player.crime_enemies.insert(victim);
    g.detection_bark(victim, Some(FormId::PLAYER_REF), "Assault");
    g.crimes.notes.push(Note::Crime { kind: Kind::Assault, victim: Some(victim), seen_by: Vec::new(), hostile: vec![victim] });
    none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_bounds() {
        let b = Settings::default().bounds;
        assert_eq!(Alignment::of(0.0, b), Alignment::Neutral);
        assert_eq!(Alignment::of(-249.0, b), Alignment::Neutral);
        assert_eq!(Alignment::of(-250.0, b), Alignment::Evil);
        assert_eq!(Alignment::of(-750.0, b), Alignment::VeryEvil);
        assert_eq!(Alignment::of(250.0, b), Alignment::Good);
        assert_eq!(Alignment::of(1000.0, b), Alignment::VeryGood);
        assert!(Alignment::VeryEvil.evil() && Alignment::Evil.evil() && !Alignment::Neutral.evil());
        assert!(Alignment::Good.good() && !Alignment::Neutral.good());
    }

    #[test]
    fn crime_kinds() {
        assert!(Kind::Murder.major() && Kind::Assault.major());
        assert!(!Kind::Theft.major() && !Kind::Lock.major() && !Kind::Trespass.major());
        assert_eq!(Kind::Theft.topic(), Some("Steal"));
        assert_eq!(Kind::Lock.topic(), None);
    }

    #[test]
    fn cell_flags_from_raw() {
        let sub = |sig: &[u8; 4], data: Vec<u8>| fnv_esm::Subrecord { sig: *sig, data, extended: false };
        let subs = vec![sub(b"DATA", vec![0x21]), sub(b"XOWN", 0x0010_4C78u32.to_le_bytes().to_vec()), sub(b"XRNK", 2i32.to_le_bytes().to_vec())];
        let c = CellDef::from_raw(&subs, FORM_OFF_LIMITS, &Some);
        assert_eq!(c, CellDef { owner: Some(FormId(0x0010_4C78)), rank: Some(2), public: true, interior: true, off_limits: true, cant_wait: false });
        assert!(c.matters());
        // Can't wait (bit 19) alone is kept (Gomorrah00 and eight more in FalloutNV.esm).
        let gomorrah = CellDef::from_raw(&[sub(b"DATA", vec![0x01])], FORM_CANT_WAIT, &Some);
        assert!(gomorrah.cant_wait && !gomorrah.off_limits && gomorrah.matters());
        assert!(!CellDef::from_raw(&[sub(b"DATA", vec![0x01])], 0, &Some).matters());
    }
}
