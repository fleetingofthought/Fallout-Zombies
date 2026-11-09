//! Perks' activate choices, Cannibal's and Mister Sandman's acts
//! (docs/spec-notes/perk-activation.md).
//!
//! **The choices.** A perk entry of entry point Activate with the function Add Activate
//! Choice gives the player a button when activating something: its label (EPF2), its
//! script (EPFT 4, the entry's SCHR/SCDA) and a Run Immediately flag (EPF3) [documented:
//! GECK wiki Entry Point, "Activate: run when the player activates something; presents
//! a choice list to the player that can trigger a script"; xEdit PERK]. The masters have
//! four, all entry point code 27 with function 9, three conditions tabs' worth of DATA
//! [observed: FalloutNV.esm Cannibal "Devour", Ghastly Scavenger "Devour", Dine and Dash
//! "Dine and Dash", Mister Sandman "Murder"; HonestHearts.esm one more Devour]: so code
//! 27 is Activate and function 9 Add Activate Choice [hypothesis by elimination]. An
//! entry is offered when the player owns the perk (the rank rule of `crate::perks`) and
//! its conditions pass: tab 0 on the player, tab 1 on the activated reference
//! [hypothesis: the tab-1 conditions — GetDead, HasBeenEaten, GetSleeping, IsChild of the
//! target — only make sense on it]. The choice list holds the offered labels in the
//! perks' order, then the default action (`crate::functions` none: the engine names
//! it, Search or Talk) [design: a list of choices is what the wiki describes; whether
//! the original adds the default action is a capture question, owner-review]. A chosen
//! entry's script runs with the activated reference as its calling reference and the
//! player as the action reference [observed: the fragments call `StartCannibal Player`
//! and `StartMisterSandman Player` without a reference, so the act starts on the
//! target]. Run Immediately is kept but changes nothing here: the masters' four are off.
//!
//! **Cannibal** (`victim.StartCannibal eater`): the eater is in the cannibalism
//! animation for [`CANNIBAL_SECONDS`] (GetCannibal 1 at once) [documented: GetCannibal
//! "returns 1 while the calling actor is in the cannibalism animation"; observed:
//! capture ac4, 1 at once and still 1 after 600 frames], and plays the IDLE
//! `StartCannibal` (`3rdP_Cannibal.kf`, conditioned on GetCannibal 1 [observed]) and the
//! sound `NPCHumanPerkCannibal` [observed: the SOUN's name; hypothesis: that it is the
//! act's]. The victim has been eaten (HasBeenEaten 1 from then on, saved) only when the
//! act is over [observed: ac4, HasBeenEaten still 0 600 frames after StartCannibal, the
//! act not yet over; hypothesis: that it turns 1 at the end]. StartCannibal itself adds
//! no health and no karma [observed: ac4, both unchanged]. It is a hostile act [documented: catalogue notes];
//! seen, "a crime against nature" [documented: Cannibal's DESC]: every witness turns on
//! the player as for an assault [hypothesis]. Health, Hunger, radiation, karma, the
//! Corpses Eaten statistic and Meat of Champions are the perk scripts' own [observed:
//! SCTX].
//!
//! **Mister Sandman** (`victim.StartMisterSandman killer`) needs a victim asleep in a bed
//! (GetSleeping 3): on Ringo asleep in his bed it worked; called by a script on an awake
//! actor, and on one a script made unconscious, the original crashed [observed: captures
//! ac5c, and ac5 and ac5b on Joe Cobb, 2026-10-18, once each]. So on any other victim it
//! is refused and nothing happens [design: the crash is not copied]. The killer is in the
//! act for [`SANDMAN_SECONDS`] (GetSandman 1 at once) and the victim dies at the act's
//! end, silently [observed: ac5c, GetDead 0 at once and after 60 and 180 frames, 1
//! after 420 frames, GetSandman 1 until then and 0 at 420; documented: GetSandman, "the
//! silent sleeping-NPC kill"; hypothesis: that the end is the clip's 5.967 s]. The killer
//! plays the IDLE `SandMan` tree's clip for
//! the victim's furniture marker (bed left 1, bed right 2, floor left 3, floor right 4:
//! the children's GetFurnitureMarkerID conditions, judged on the victim [hypothesis]),
//! and the sound `NPCHumanPerkMrSandman`. The kill is the player's: karma and murder are
//! judged as for any kill (`Game::kill`). XP and the Sandman Kills statistic are the
//! script's [observed].

use crate::data::PerkEffect;
use crate::game::{menus, Game, RunCtx};
use crate::idles::IdleSource;
use crate::sound::SoundRequest;
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};

/// Entry point code of Activate [hypothesis, module notes].
pub const ACTIVATE: u8 = 27;
/// Entry point function Add Activate Choice [hypothesis, module notes].
pub const ADD_ACTIVATE_CHOICE: u8 = 9;
/// How long the eater is in the cannibalism animation, seconds of game time: the clip's
/// length [observed: `3rdP_Cannibal.kf`, sequence SpecialIdle_Cannibal 7.667 s;
/// hypothesis: that the act lasts the clip; capture ac4 saw it last more than 600 frames
/// with the player not at the corpse].
pub const CANNIBAL_SECONDS: f64 = 7.667;
/// How long the killer is in the Mister Sandman act: the clips' length [observed: the
/// four `SandMan*.kf`, 5.967 s each; hypothesis, as above].
pub const SANDMAN_SECONDS: f64 = 5.967;

/// One offered choice: the perk, the entry (its PRKE place) and its label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateChoice {
    pub perk: FormId,
    pub entry: u32,
    pub label: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub run_immediately: bool,
}

/// The choice list open for an activation (module notes): the choices, then the
/// default action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateMenu {
    pub target: FormId,
    pub choices: Vec<ActivateChoice>,
}

/// What the player picked in the choice list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    /// A perk's choice: its script ran.
    Choice(ActivateChoice),
    /// The default action: the engine activates the target as E does, without the list.
    Default(FormId),
}

/// A perk's act under way (GetCannibal, GetSandman).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Act {
    Cannibal,
    Sandman,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PerkAct {
    pub actor: FormId,
    pub victim: FormId,
    pub act: Act,
    /// Game time it ends.
    pub until: f64,
}

impl Game<'_> {
    /// The choices the player's perks offer on activating `target` now (module notes).
    pub fn activate_choices(&mut self, target: FormId) -> Vec<ActivateChoice> {
        let player = FormId::PLAYER_REF;
        let data = self.data;
        let mut found = Vec::new();
        if self.in_act_as_victim(target) {
            return Vec::new();
        }
        let owned: Vec<(FormId, u8)> = self.player.perks.iter().map(|(&p, &r)| (p, r)).collect();
        for (perk, rank) in owned {
            let edited = self.lu3.perk_edits.get(&perk).cloned();
            let Some(def) = edited.as_ref().or_else(|| data.perks.get(&perk)) else { continue };
            for e in def.entries.iter().filter(|e| e.rank < rank.max(1)) {
                let PerkEffect::EntryPoint(ep) = &e.effect else { continue };
                if ep.point != ACTIVATE || ep.function != ADD_ACTIVATE_CHOICE || ep.unit.is_none() {
                    continue;
                }
                let Some(conds) = ep.conditions.as_ref() else { continue };
                let label = format!("{} activate choice", data.name(perk));
                let tab = |t: i8| conds.iter().filter(|(k, _)| *k == t).map(|(_, c)| c.clone()).collect::<Vec<_>>();
                if conds.iter().any(|(t, _)| !matches!(t, 0 | 1)) {
                    continue;
                }
                if !self.conditions_pass(&tab(0), Some(player), Some(target), &label) || !self.conditions_pass(&tab(1), Some(target), Some(player), &label) {
                    continue;
                }
                let button = ep.button.clone().unwrap_or_else(|| data.name(perk));
                found.push((e.priority, perk, ActivateChoice { perk, entry: e.ordinal, label: button, run_immediately: ep.run_immediately }));
            }
        }
        found.sort_by_key(|(p, perk, c)| (*p, *perk, c.entry));
        found.into_iter().map(|(_, _, c)| c).collect()
    }

    /// The player activates `target` by hand: when perks offer choices, the choice list
    /// opens (a message box menu, MenuMode 1001 [hypothesis]) and the activation waits
    /// for it. Returns whether it opened.
    pub fn open_activate_menu(&mut self, target: FormId) -> bool {
        if self.activate_menu.is_some() {
            return false;
        }
        let choices = self.activate_choices(target);
        if choices.is_empty() {
            return false;
        }
        self.activate_menu = Some(ActivateMenu { target, choices });
        self.open_menu(menus::MESSAGE);
        true
    }

    /// The player picks button `index` of the open choice list: a choice runs its
    /// script; the one after the choices is the default action. The list closes.
    pub fn pick_activate_choice(&mut self, index: usize) -> Option<Picked> {
        let m = self.activate_menu.take()?;
        if self.menu == Some(menus::MESSAGE) && self.boxes.is_empty() {
            self.close_menu();
        }
        match m.choices.get(index) {
            Some(c) => {
                self.run_activate_choice(m.target, c);
                Some(Picked::Choice(c.clone()))
            }
            None => Some(Picked::Default(m.target)),
        }
    }

    /// Run a choice's script on `target` (module notes). Whether it had one.
    pub fn run_activate_choice(&mut self, target: FormId, c: &ActivateChoice) -> bool {
        let data = self.data;
        let unit = data.perks.get(&c.perk).and_then(|p| p.entries.iter().find(|e| e.ordinal == c.entry)).and_then(|e| match &e.effect {
            PerkEffect::EntryPoint(ep) => ep.unit,
            _ => None,
        });
        let Some(u) = unit else { return false };
        self.emit(crate::trace::EventKind::ResultScript { unit: data.unit(u).key(), on: Some(target) });
        self.run_result(u, RunCtx { self_ref: Some(target), action_ref: Some(FormId::PLAYER_REF), quest: None });
        true
    }

    /// Whether `actor` is in `act` now (GetCannibal, GetSandman).
    pub fn in_perk_act(&self, actor: FormId, act: Act) -> bool {
        self.perk_acts.iter().any(|a| a.actor == actor && a.act == act && a.until > self.time)
    }

    fn begin_act(&mut self, actor: FormId, victim: FormId, act: Act, seconds: f64) {
        self.finish_perk_acts();
        self.perk_acts.retain(|a| a.actor != actor);
        self.perk_acts.push(PerkAct { actor, victim, act, until: self.time + seconds });
    }

    /// The acts whose time is over end: a cannibalism's victim has been eaten (module
    /// notes). Each frame (`Game::tick`).
    pub fn finish_perk_acts(&mut self) {
        let now = self.time;
        let (over, going): (Vec<PerkAct>, Vec<PerkAct>) = self.perk_acts.drain(..).partition(|a| a.until <= now);
        self.perk_acts = going;
        for a in over {
            match a.act {
                Act::Cannibal => self.extra.entry(a.victim).or_default().eaten = true,
                Act::Sandman => {
                    self.kill(a.victim, Some(a.actor));
                }
            }
        }
    }

    /// Whether `r` is the victim of an act under way: no choice is offered on it then.
    fn in_act_as_victim(&self, r: FormId) -> bool {
        self.perk_acts.iter().any(|a| a.victim == r && a.until > self.time)
    }

    fn act_sound(&mut self, edid: &str, at: FormId) {
        if let Some(sound) = self.data.by_edid(edid) {
            self.ask_sound(SoundRequest { sound, at: Some(at), system: false, id: 0 });
        }
    }

    /// `victim.StartCannibal eater` (module notes).
    pub fn start_cannibal(&mut self, victim: FormId, eater: FormId) {
        self.begin_act(eater, victim, Act::Cannibal, CANNIBAL_SECONDS);
        if let Some(idle) = self.data.by_edid("StartCannibal") {
            self.ask_idle(eater, idle, IdleSource::Script);
        }
        self.act_sound("NPCHumanPerkCannibal", eater);
        if eater == FormId::PLAYER_REF {
            self.player_cannibalized(victim);
        }
    }

    /// `victim.StartMisterSandman killer` (module notes).
    /// Whether it began: a victim that is not asleep in a bed (GetSleeping 3,
    /// `Game::asleep`), or dead already, is refused.
    pub fn start_mister_sandman(&mut self, victim: FormId, killer: FormId) -> bool {
        if !self.asleep(victim) || self.is_dead(victim) {
            return false;
        }
        self.begin_act(killer, victim, Act::Sandman, SANDMAN_SECONDS);
        if let Some(idle) = self.sandman_idle(victim) {
            self.ask_idle(killer, idle, IdleSource::Script);
        }
        self.act_sound("NPCHumanPerkMrSandman", killer);
        true
    }

    /// The Mister Sandman clip for where the victim lies: the first child of the
    /// `SandMan` and `SandManFloor` idles whose conditions hold on the victim, else the
    /// first with a file (module notes).
    fn sandman_idle(&mut self, victim: FormId) -> Option<FormId> {
        let data = self.data;
        let parents: Vec<FormId> = ["SandMan", "SandManFloor"].iter().filter_map(|e| data.by_edid(e)).collect();
        let mut children: Vec<FormId> = data.idles.iter().filter(|(_, d)| d.parent.is_some_and(|p| parents.contains(&p)) && d.has_file()).map(|(&id, _)| id).collect();
        children.sort();
        let first = children.first().copied();
        children.into_iter().find(|&c| self.idle_allowed(victim, c)).or(first)
    }
}
