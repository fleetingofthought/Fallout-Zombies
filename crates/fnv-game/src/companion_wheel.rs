//! The companion wheel and the companion behaviour it sets (docs/spec-notes/companion-wheel.md).
//!
//! - **When it opens**: the player activates a teammate (SetPlayerTeammate 1) that would be
//!   talked to: the wheel opens instead of the conversation [documented: GECK wiki
//!   SetPlayerTeammate, "Enables the Companion Wheel (New Vegas)"]. While it is open it is
//!   the open menu, [`MENU_CODE`] (MenuMode 1075) [hypothesis: the code], and the game is
//!   held behind it like behind the other menus.
//! - **What its buttons do**: each command but Talk, Use StimPak and Back Up makes the
//!   companion say the matching topic of the quest VNPCFollowers ([`Command::topic`]), and
//!   the line's result scripts do the work: `Waiting`, `IsFollowingLong`,
//!   `IsFollowingDefault`, `FollowerSwitchAggressive`, `CombatStyleRanged` /
//!   `CombatStyleMelee` with SetCombatStyle, EvaluatePackage, OpenTeammateContainer, and
//!   the quest's own bookkeeping (`fFollowerWaitingLeaveDay`) [observed: FalloutNV.esm,
//!   the lines of FollowersWait, FollowersLetsGo, FollowersTactics…; hypothesis: that the
//!   wheel says those topics rather than setting the variables itself]. The line goes on
//!   in real time while the wheel is open ([`Game::update_wheel_line`]): its end script
//!   (FollowersLetsGo's `Waiting 0` is in its end script) runs when it is over.
//! - **What the variables then do**: `Waiting` and `IsFollowingLong` choose the companion's
//!   Wait (Guard) or Follow package by the packages' own conditions [observed:
//!   FollowersBooneFollowPlayerWAIT/LONG/DEFAULT]; the combat style's Melee Only / Ranged
//!   Only restriction is the combat AI's [observed: FollowersCombatStyleMelee and
//!   FollowersCombatStyleRanged]; `FollowerSwitchAggressive` is read here
//!   ([`Game::is_passive_teammate`], `Game::would_attack`): a passive companion fights only
//!   who fights the player or a teammate, or has hit it [hypothesis].
//! - **Use StimPak**: one of the player's stimpaks is used on the companion
//!   (`Game::ingest`) and it says Regenerating [documented: GECK wiki Combat dialogue,
//!   "Regenerating: Said by actor given a stimpak at the companion wheel"].
//! - **Back Up**: the companion steps back from the player ([`BACK_UP`] units), walked by
//!   `fnv_ai` ([`Game::take_back_up`]) [hypothesis: the distance].
//! - **Sneaking**: a teammate following the player sneaks while the player sneaks
//!   [documented: GECK wiki SetPlayerTeammate, "The teammate will imitate the player's
//!   sneak state"] (`Game::sneaks`); as it starts it says FollowersStealthing
//!   [hypothesis: when the topic is said].

use std::collections::{BTreeMap, BTreeSet};

use fnv_sim::FormId;
use serde::Serialize;

use crate::game::Game;
use crate::package::{PackageType, TargetKind};

/// The wheel's menu code [hypothesis: the GECK wiki's MenuMode list as remembered: 1075
/// "Companion Wheel", between the Vigor Tester's 1074 and the trait menu's 1084 which were
/// observed; no shipped script asks for it].
pub const MENU_CODE: u16 = 1075;

/// How far a companion told to back up steps away from the player, game units
/// [hypothesis].
pub const BACK_UP: f32 = 192.0;
/// A back-up walk not over after this long ends where it is, seconds [engineering choice].
pub const BACK_UP_SECONDS: f64 = 5.0;

/// The quest whose topics the wheel's commands say [observed: FalloutNV.esm, every
/// companion's command lines are VNPCFollowers lines].
pub const QUEST: &str = "VNPCFollowers";

/// The companion's script variables the wheel reads [observed: the names the companions'
/// packages, dialogue conditions and result scripts use].
pub mod var {
    pub const WAITING: &str = "Waiting";
    pub const AGGRESSIVE: &str = "FollowerSwitchAggressive";
    pub const FOLLOWING_LONG: &str = "IsFollowingLong";
    pub const RANGED: &str = "CombatStyleRanged";
}

/// A button of the wheel, by its id in the menu file (`menus\companion_wheel_menu.xml`,
/// the radials' `<id>` 0 to 7) [documented].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Command {
    AggressivePassive,
    UseStimpak,
    StayFollow,
    Talk,
    BackUp,
    NearFar,
    OpenInventory,
    RangedMelee,
}

impl Command {
    /// In the file's order (ids 0 to 7).
    pub const ALL: [Command; 8] =
        [Command::AggressivePassive, Command::UseStimpak, Command::StayFollow, Command::Talk, Command::BackUp, Command::NearFar, Command::OpenInventory, Command::RangedMelee];

    pub fn id(self) -> usize {
        Command::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    pub fn from_id(id: usize) -> Option<Command> {
        Command::ALL.get(id).copied()
    }

    /// The test aid's and the log's name.
    pub fn name(self) -> &'static str {
        match self {
            Command::AggressivePassive => "aggressive_passive",
            Command::UseStimpak => "stimpak",
            Command::StayFollow => "stay_follow",
            Command::Talk => "talk",
            Command::BackUp => "back_up",
            Command::NearFar => "near_far",
            Command::OpenInventory => "inventory",
            Command::RangedMelee => "ranged_melee",
        }
    }

    pub fn parse(s: &str) -> Option<Command> {
        Command::ALL.iter().copied().find(|c| c.name().eq_ignore_ascii_case(s))
    }

    /// The topic the command makes the companion say, given whether the button shows its
    /// other state now (`alt`, [`State::alt`]): Wait or Let's Go, Passive or Aggressive …
    /// [observed: the topics' result scripts; hypothesis: that the wheel says them].
    pub fn topic(self, alt: bool) -> Option<&'static str> {
        Some(match (self, alt) {
            (Command::AggressivePassive, false) => "FollowersTacticsCombatAggressive",
            (Command::AggressivePassive, true) => "FollowersTacticsCombatPassive",
            (Command::StayFollow, false) => "FollowersWait",
            (Command::StayFollow, true) => "FollowersLetsGo",
            (Command::NearFar, false) => "FollowersTacticsDistanceLong",
            (Command::NearFar, true) => "FollowersTacticsDistanceDefault",
            (Command::RangedMelee, false) => "FollowersTacticsCombatRanged",
            (Command::RangedMelee, true) => "FollowersTacticsCombatMelee",
            (Command::OpenInventory, _) => "FollowersTrade",
            (Command::UseStimpak, _) => "Regenerating",
            (Command::Talk | Command::BackUp, _) => return None,
        })
    }

    /// The button's words: what it does now [observed: "Use Stimpak" on the owner's
    /// screenshot of 2026-10-19 (the masters' sCWheelUseStimPack reads "Use StimPak");
    /// hypothesis: the others, named as the menu file's comments name the buttons].
    pub fn label(self, alt: bool) -> &'static str {
        match (self, alt) {
            (Command::AggressivePassive, false) => "Aggressive",
            (Command::AggressivePassive, true) => "Passive",
            (Command::UseStimpak, _) => "Use Stimpak",
            (Command::StayFollow, false) => "Wait",
            (Command::StayFollow, true) => "Follow",
            (Command::Talk, _) => "Talk",
            (Command::BackUp, _) => "Back Up",
            (Command::NearFar, false) => "Far",
            (Command::NearFar, true) => "Near",
            (Command::OpenInventory, _) => "Open Inventory",
            (Command::RangedMelee, false) => "Ranged",
            (Command::RangedMelee, true) => "Melee",
        }
    }
}

/// One button as the wheel shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct State {
    pub command: Command,
    /// The button shows its alternative icon (the file's `user11` 1): the companion
    /// waits, is aggressive, keeps far, fights at range [observed: the owner's screenshots
    /// of 2026-10-19 show a passive companion's peaceful icon and a ranged companion's
    /// rifle: the icon is the state the companion is in].
    pub alt: bool,
    /// It can be picked (the file's `_enabled`): the companion has a line of the topic it
    /// can say now; Use StimPak while the player carries a stimpak and the companion is
    /// hurt [hypothesis for both].
    pub enabled: bool,
    pub label: &'static str,
    /// The lines under the wheel (the file's `CWM_ButtonContextText`): Use Stimpak's are
    /// the companion's health and the stimpaks carried, "HP 94/285" and "Stimpak 84"
    /// [observed: the owner's screenshot of 2026-10-19]; Open Inventory's the companion's
    /// carry weight, "WG 95/100" [observed: the owner's word of 2026-10-19 for the
    /// figures; hypothesis: the "WG" before them, as the Pip-Boy's]; the others have none
    /// [hypothesis].
    pub context: Option<String>,
}

/// What a command did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Outcome {
    /// The companion says this line of the command's topic (its results run).
    Said { topic: FormId, info: FormId },
    /// The command's topic has no line the companion can say: nothing happens.
    NoLine { topic: &'static str },
    /// The wheel closed and the conversation started with this line.
    Talk { info: FormId },
    /// A stimpak was used on the companion; it says its line if it has one.
    Stimpak { item: FormId, line: Option<FormId> },
    /// The companion steps back.
    BackUp,
    /// The command cannot be used now (its button is not enabled).
    Refused,
}

/// The companions' state of this module, not saved: the open wheel, the back-up orders and
/// who sneaks with the player.
#[derive(Debug, Clone, Default)]
pub struct Companions {
    /// The companion whose wheel is open.
    pub wheel: Option<FormId>,
    /// Companions told to back up, and when (`Game::time`): `fnv_ai` takes each order.
    pub back_ups: BTreeMap<FormId, f64>,
    /// Teammates sneaking because the player sneaks.
    pub sneaking: BTreeSet<FormId>,
    /// The companion whose inventory the wheel's Open Inventory asked for: its line goes
    /// on while the container menu is open ([`Game::line_goes_on_in_menu`]).
    pub trading: Option<FormId>,
}

impl Game<'_> {
    /// A script variable of a reference by name, as a number (0 when never written);
    /// `None` when its script does not declare it.
    pub fn ref_var_named(&self, r: FormId, name: &str) -> Option<f64> {
        let (key, script) = self.instance_of(r)?;
        let index = self.data.script(script)?.var_index(name)?;
        Some(self.var(key, index).num())
    }

    /// Whether the player activating `actor` opens the wheel: a teammate, alive
    /// [documented: SetPlayerTeammate enables the Companion Wheel].
    pub fn has_companion_wheel(&self, actor: FormId) -> bool {
        self.is_teammate(actor)
    }

    /// Open the wheel on `companion`: the open menu from now on. False when it has none or
    /// a menu or conversation is open.
    pub fn open_companion_wheel(&mut self, companion: FormId) -> bool {
        if !self.has_companion_wheel(companion) || self.menu.is_some() || self.dialogue.is_some() {
            return false;
        }
        self.companions.wheel = Some(companion);
        self.open_menu(MENU_CODE);
        true
    }

    /// Close the wheel, if it is open. A line the companion is saying goes on in the
    /// world.
    pub fn close_companion_wheel(&mut self) -> bool {
        if self.companions.wheel.take().is_none() {
            return false;
        }
        if self.menu == Some(MENU_CODE) {
            self.close_menu();
        }
        true
    }

    /// The companion whose wheel is the open menu.
    pub fn wheel_companion(&self) -> Option<FormId> {
        self.companions.wheel.filter(|_| self.menu == Some(MENU_CODE))
    }

    /// Whether a line `speaker` says goes on while the menus hold the game: the wheel's
    /// companion's while its wheel is open [design: its reply is heard and read in the
    /// wheel, as the conversation's own lines go on], and its Open Inventory line while
    /// its inventory is open [owner, 2026-10-27: Boone's "Fine by me" is said as the
    /// inventory comes up, not after it closes].
    pub fn line_goes_on_in_menu(&self, speaker: FormId) -> bool {
        self.wheel_companion() == Some(speaker) || (self.companions.trading == Some(speaker) && self.menu == Some(crate::runner::CONTAINER_MENU))
    }

    fn var_on(&self, companion: FormId, name: &str) -> bool {
        self.ref_var_named(companion, name).is_some_and(|v| v != 0.0)
    }

    /// The stimpak the player would give: a carried ingestible of equip type Stimpak
    /// (ETYP 11), the DOBJ's Stimpak (place 0 [documented: xEdit DOBJ names]; FalloutNV.esm
    /// names `Stimpak` 0x00015169 there [observed]) first, else the one with Editor ID
    /// `Stimpak` [hypothesis: which one; observed: the masters' Stimpak and Super Stimpak
    /// both have equip type 11].
    pub fn wheel_stimpak(&self) -> Option<(FormId, i32)> {
        let data = self.data;
        let inv = self.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref())?;
        let plain = data.default_object(crate::data::DefaultObject::Stimpak).or_else(|| data.by_edid("Stimpak"));
        let mut found: Vec<(FormId, i32)> = inv.iter().filter(|(i, n)| *n > 0 && data.ingestibles.get(i).is_some_and(|d| d.equip_type == 11)).collect();
        found.sort_by_key(|(i, _)| (Some(*i) != plain, *i));
        let total = found.iter().map(|(_, n)| n).sum();
        found.first().map(|(i, _)| (*i, total))
    }

    /// Whether `companion` has a line of the topic named `topic` it can say to the player
    /// now (no random pick).
    fn can_say_topic(&mut self, companion: FormId, topic: &str) -> bool {
        let data = self.data;
        let Some(t) = data.by_edid(topic) else { return false };
        data.lines_tried(t).iter().any(|&i| self.can_say(companion, Some(FormId::PLAYER_REF), i))
    }

    /// The wheel's buttons for `companion`, in the file's order.
    pub fn wheel_buttons(&mut self, companion: FormId) -> Vec<State> {
        let mut out = Vec::new();
        for c in Command::ALL {
            let alt = match c {
                Command::AggressivePassive => self.var_on(companion, var::AGGRESSIVE),
                Command::StayFollow => self.var_on(companion, var::WAITING),
                Command::NearFar => self.var_on(companion, var::FOLLOWING_LONG),
                Command::RangedMelee => self.var_on(companion, var::RANGED),
                _ => false,
            };
            let mut context = None;
            let enabled = match c {
                Command::Talk | Command::BackUp => true,
                Command::OpenInventory => {
                    if let Ok(e) = self.world.encumbrance(companion) {
                        context = Some(format!("WG {:.0}/{:.0}", e.weight.floor(), e.capacity));
                    }
                    c.topic(alt).is_some_and(|t| self.can_say_topic(companion, t))
                }
                Command::UseStimpak => {
                    let carried = self.wheel_stimpak();
                    let (now, full) = self.health_now_and_full(companion);
                    context = Some(format!("HP {:.0}/{:.0}
Stimpak {}", now.max(0.0).ceil(), full, carried.map_or(0, |(_, n)| n)));
                    carried.is_some() && now < full
                }
                _ => c.topic(alt).is_some_and(|t| self.can_say_topic(companion, t)),
            };
            out.push(State { command: c, alt, enabled, label: c.label(alt), context });
        }
        out
    }

    /// The player picks a command on the open wheel. Talk closes the wheel and starts the
    /// conversation (the greeting); Open Inventory's line asks for the container menu,
    /// which the engine opens once the wheel is closed (`Game::teammate_containers`).
    pub fn wheel_command(&mut self, command: Command) -> Result<Outcome, String> {
        let c = self.wheel_companion().ok_or("the companion wheel is not open")?;
        let state = self.wheel_buttons(c).into_iter().find(|s| s.command == command).ok_or("no such button")?;
        if !state.enabled {
            return Ok(Outcome::Refused);
        }
        let player = Some(FormId::PLAYER_REF);
        match command {
            Command::Talk => {
                self.close_companion_wheel();
                let info = self.start_dialogue(c, None)?;
                Ok(Outcome::Talk { info })
            }
            Command::BackUp => {
                self.companions.back_ups.insert(c, self.time);
                Ok(Outcome::BackUp)
            }
            Command::UseStimpak => {
                let (item, _) = self.wheel_stimpak().ok_or("the player carries no stimpak")?;
                let n = self.world.remove_item(FormId::PLAYER_REF, item, 1).map_err(|e| e.to_string())?;
                if n > 0 {
                    self.emit(crate::trace::EventKind::ItemRemoved { from: FormId::PLAYER_REF, item, count: n, silent: false });
                }
                self.ingest(c, item);
                let topic = self.data.by_edid("Regenerating");
                let line = topic.and_then(|t| self.say(c, player, t, true));
                Ok(Outcome::Stimpak { item, line })
            }
            _ => {
                let name = command.topic(state.alt).expect("a topic");
                let topic = self.data.by_edid(name).ok_or_else(|| format!("the load order has no topic {name}"))?;
                Ok(match self.say(c, player, topic, true) {
                    Some(info) => {
                        self.run_end_script_now(c, info);
                        if command == Command::OpenInventory {
                            self.companions.trading = Some(c);
                        }
                        Outcome::Said { topic, info }
                    }
                    None => Outcome::NoLine { topic: name },
                })
            }
        }
    }

    /// While the wheel is open its companion's line goes on (`Game::tick`, in menu mode),
    /// and the Open Inventory line while the inventory is open.
    pub(crate) fn update_wheel_line(&mut self, dt: f64) {
        if let Some(c) = self.wheel_companion() {
            self.update_speech_of(dt, Some(c));
        }
        if let Some(c) = self.companions.trading {
            if self.menu == Some(crate::runner::CONTAINER_MENU) {
                self.update_speech_of(dt, Some(c));
            }
            if !self.speech.iter().any(|s| s.speaker == c) {
                self.companions.trading = None;
            }
        }
    }

    /// A wheel command takes effect as the companion starts its reply: the line's end
    /// result script (Let's Go's `Waiting 0`) runs now, not when the line is over
    /// [owner, 2026-10-27: Wait and Follow take effect and the wheel shows it as soon as
    /// the button is pressed]. It does not run again at the line's end.
    fn run_end_script_now(&mut self, speaker: FormId, info: FormId) {
        let data = self.data;
        let dv = self.dialogue_view();
        let Some(def) = dv.info(&info) else { return };
        let Some(s) = self.speech.iter_mut().find(|s| s.speaker == speaker && s.info == info) else { return };
        let Some(u) = def.end.filter(|_| !s.end_ran) else { return };
        s.end_ran = true;
        self.emit(crate::trace::EventKind::ResultScript { unit: data.unit(u).key(), on: Some(speaker) });
        self.run_result(u, crate::game::RunCtx { self_ref: Some(speaker), action_ref: None, quest: def.quest });
    }

    /// `fnv_ai` takes a back-up order for `actor`: when it was given.
    pub fn take_back_up(&mut self, actor: FormId) -> Option<f64> {
        self.companions.back_ups.remove(&actor)
    }

    /// A teammate set passive: `FollowerSwitchAggressive` declared and 0 [hypothesis: a
    /// companion whose script lacks the variable fights as before].
    pub fn is_passive_teammate(&self, actor: FormId) -> bool {
        self.is_teammate(actor) && self.ref_var_named(actor, var::AGGRESSIVE).is_some_and(|v| v == 0.0)
    }

    /// Whether `other` fights the player or one of the player's teammates now: in a combat
    /// state with one of them on its target list.
    pub fn fights_the_players_side(&self, other: FormId) -> bool {
        self.awareness.get(other).is_some_and(|a| a.state.in_combat() && a.targets.keys().any(|t| *t == FormId::PLAYER_REF || self.is_teammate(*t)))
    }

    /// Whether `actor`'s current package (as last chosen) follows the player.
    pub fn package_follows_player(&self, actor: FormId) -> bool {
        self.extra.get(&actor).and_then(|e| e.current_package).and_then(|p| self.package_def(&p)).is_some_and(|d| {
            matches!(d.ty, PackageType::Follow | PackageType::Accompany) && matches!(d.target.map(|t| t.kind), Some(TargetKind::Reference(Some(r))) if r == FormId::PLAYER_REF)
        })
    }

    /// The player sneaks, as the engine last told (`functions::senses`).
    pub fn player_sneaks(&self) -> bool {
        self.senses.motion.get(&FormId::PLAYER_REF).is_some_and(|m| m.sneaking)
    }

    /// A teammate following the player sneaks while the player sneaks (module notes).
    pub fn sneaks_with_player(&self, actor: FormId) -> bool {
        self.is_teammate(actor) && self.player_sneaks() && self.package_follows_player(actor)
    }

    /// Each frame: the teammates that start sneaking with the player say
    /// FollowersStealthing, when they have a line of it and are not saying another
    /// [hypothesis: when the topic is said].
    pub(crate) fn companions_sneak(&mut self) {
        let now: BTreeSet<FormId> = self.teammates().into_iter().filter(|&t| self.sneaks_with_player(t) && self.sneaks(t)).collect();
        let started: Vec<FormId> = now.difference(&self.companions.sneaking).copied().collect();
        self.companions.sneaking = now;
        let Some(topic) = self.data.by_edid("FollowersStealthing") else { return };
        for t in started {
            if !self.is_talking(t) {
                self.say(t, Some(FormId::PLAYER_REF), topic, true);
            }
        }
    }
}
