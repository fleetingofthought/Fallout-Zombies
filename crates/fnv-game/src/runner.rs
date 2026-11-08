//! The headless script runner: `fnv.script.actions/1` in, `fnv.script.trace/1` out.
//!
//! The contract is docs/spec-notes/layer5-captures.md §4 (written by workstream 5C for
//! the `L5.quest.*` scenarios): the same operations, the same frames between them, and
//! observations keyed by `label`. Operations beyond that contract stand in for Layer 6
//! systems in headless walk-throughs (dialogue lines, package events, menus, message
//! box buttons) and are listed in docs/spec-notes/scripts.md §8.

use crate::functions::{actor_value, av_code, set_actor_value_current, Ctx};
use crate::game::{event, menus, ActMode, Game, GameConfig, InstKey};
use crate::trace::EventKind;
use crate::GameData;
use fnv_script::vm::{literal, to_int_storage, ArgValue, Value};
use fnv_sim::world::SimConfig;
use fnv_sim::{FormId, FormStore, RefTable, World};
use serde::Deserialize;
use serde_json::{json, Map, Value as Json};
use std::collections::BTreeMap;

pub const ACTIONS_FORMAT: &str = "fnv.script.actions/1";
pub const TRACE_FORMAT: &str = "fnv.script.trace/1";
/// `fnvtool dialogue talk`: the same input document under its own name, and the trace
/// it prints (docs/spec-notes/dialogue.md §8).
pub const DIALOGUE_ACTIONS_FORMAT: &str = "fnv.dialogue.actions/1";
pub const DIALOGUE_TRACE_FORMAT: &str = "fnv.dialogue.trace/1";

fn default_frame() -> f64 {
    1.0 / 60.0
}
fn one() -> u16 {
    1
}
fn yes() -> bool {
    true
}
/// Frames `choose` and `wait_talk` wait for the actor to finish speaking (2 minutes).
fn talk_frames() -> u64 {
    7200
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub format: String,
    #[serde(default)]
    pub seed: u64,
    #[serde(default = "default_frame")]
    pub frame_seconds: f64,
    #[serde(default)]
    pub title: Option<String>,
    /// Informational: the capture save a golden came from.
    #[serde(default)]
    pub save: Option<String>,
    #[serde(default = "one")]
    pub player_level: u16,
    #[serde(default)]
    pub hardcore: bool,
    /// Start from the new-game state (start-game-enabled quests running). Default true.
    #[serde(default = "yes")]
    pub new_game: bool,
    /// Trace writes to an instance's own locals too.
    #[serde(default)]
    pub trace_locals: bool,
    #[serde(default)]
    pub given: Vec<Given>,
    pub actions: Vec<Action>,
    /// References whose final state is reported.
    #[serde(default)]
    pub report: Vec<String>,
    /// The load order the chain is written for (`fnvtool script cover` runs it over
    /// these plugins; empty: the first plugin of the load order it is given).
    /// `script run` uses the plugins on its command line and ignores this.
    #[serde(default)]
    pub plugins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Given {
    Quest {
        quest: String,
        #[serde(default)]
        running: Option<bool>,
        #[serde(default)]
        completed: Option<bool>,
        #[serde(default)]
        stage: Option<i32>,
        #[serde(default)]
        done: Vec<i32>,
        #[serde(default)]
        not_done: Vec<i32>,
        #[serde(default)]
        objectives: Vec<GivenObjective>,
        #[serde(default)]
        vars: BTreeMap<String, f64>,
        /// Reference variables: name → form.
        #[serde(default)]
        refs: BTreeMap<String, String>,
    },
    Global { name: String, value: f32 },
    ItemCount { of: String, item: String, count: i32 },
    /// Items added (or, negative, removed) on top of what the start gives.
    ItemChange { of: String, item: String, change: i32 },
    Disabled { of: String, disabled: bool },
    /// A reference's object-script variables, dead and disabled state (round 2,
    /// layer5-captures.md §8.2).
    Ref {
        #[serde(rename = "ref")]
        reference: String,
        #[serde(default)]
        vars: BTreeMap<String, f64>,
        #[serde(default)]
        dead: Option<bool>,
        #[serde(default)]
        disabled: Option<bool>,
    },
    /// The player's fame and infamy with a reputation (round 3, layer5-captures.md §9.2).
    Reputation {
        rep: String,
        #[serde(default)]
        fame: Option<f64>,
        #[serde(default)]
        infamy: Option<f64>,
    },
    /// An actor value's current value, undamaged (round 3).
    ActorValue { of: String, av: String, value: f64 },
    /// A menu open (or not) before action 0 (round 4, layer5-captures.md §10.4): a
    /// message box (1001) of a message the input does not name, or another menu code.
    Menu { menu: u16, open: bool },
    /// Where a reference stands: an interior cell, or a worldspace (the exterior cell at
    /// `pos`); rotation in radians (a save's place, `ext queries --save`).
    Place {
        #[serde(rename = "ref")]
        reference: String,
        cell: String,
        pos: [f32; 3],
        #[serde(default)]
        rot: [f32; 3],
    },
    /// The player's tag skills (actor value names or codes), replacing the class's.
    TagSkills { skills: Vec<String> },
    /// Skill points the player has spent on a skill.
    SkillPoints { skill: String, value: f64 },
}

/// One read of `use_item` (round 3): an actor value of the user (`av.<name>`) or the
/// count of an item it carries (`count.<form>`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UseRead {
    #[serde(default)]
    pub av: Option<String>,
    #[serde(default)]
    pub item: Option<String>,
}

/// One series of `sample_jumps` (round 3): a global, an object-script variable of a
/// reference, or a quest variable.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JumpRead {
    #[serde(default)]
    pub global: Option<String>,
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
    #[serde(default)]
    pub quest: Option<String>,
    #[serde(default)]
    pub var: Option<String>,
}

/// What `activate_read` reads: a reference's field (`get_ref`) or a quest's
/// (`get_quest`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadSpec {
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
    #[serde(default)]
    pub quest: Option<String>,
    pub field: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GivenObjective {
    pub index: i32,
    #[serde(default)]
    pub displayed: Option<bool>,
    #[serde(default)]
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    // ---- the 5C contract ----
    SetStage { quest: String, stage: i32 },
    SetStageRead { quest: String, stage: i32, field: String, label: String },
    StartQuest { quest: String },
    StopQuest { quest: String },
    /// With `stopwatch` (round 4), the stopwatch starts in the same frame.
    SetQuestVar { quest: String, var: String, value: f64, #[serde(default)] stopwatch: bool },
    SetLocal { #[serde(rename = "type")] ty: String, value: f64, #[serde(default)] label: Option<String> },
    SetObjective { quest: String, index: i32, #[serde(default)] displayed: Option<bool>, #[serde(default)] completed: Option<bool> },
    GetQuest { quest: String, fields: Vec<String>, #[serde(default)] label: Option<String> },
    SampleVar { quest: String, var: String, frames: u64, #[serde(default)] runs: Option<usize>, #[serde(default)] skip: usize, #[serde(default)] values: bool, #[serde(default)] label: Option<String> },
    WaitFrames { frames: u64 },
    AddItem { to: String, item: String, count: i32 },
    RemoveItem { from: String, item: String, count: i32 },
    RemoveAllItems { from: String, #[serde(default)] to: Option<String> },
    Enable { target: String },
    Disable { target: String },
    SetGlobal { name: String, value: f32 },
    /// `target.Activate actor [1]`: with `run_on_activate` the OnActivate block runs
    /// instead of the default action (round 2).
    Activate { #[serde(default)] actor: Option<String>, target: String, #[serde(default)] run_on_activate: bool, #[serde(default)] label: Option<String> },
    GetItemCount { of: String, item: String, #[serde(default)] label: Option<String> },
    GetDisabled { of: String, #[serde(default)] label: Option<String> },
    GetGlobal { name: String, #[serde(default)] label: Option<String> },
    // ---- round 2 of the contract (layer5-captures.md §8.2) ----
    /// A variable of the reference's object script, by name (xNVSE SetVariable).
    SetRefVar { #[serde(rename = "ref")] reference: String, var: String, value: f64 },
    /// Activate and one read in the same script.
    ActivateRead { target: String, #[serde(default)] actor: Option<String>, #[serde(default)] run_on_activate: bool, read: ReadSpec, label: String },
    /// `sample_var` for an object-script variable of a reference.
    SampleRefVar { #[serde(rename = "ref")] reference: String, var: String, frames: u64, #[serde(default)] runs: Option<usize>, #[serde(default)] skip: usize, #[serde(default)] values: bool, #[serde(default)] label: Option<String> },
    /// AddItem (silent) and item counts in the same script.
    AddItemRead { to: String, item: String, count: i32, read: Vec<String>, label: String },
    /// Item counts once per frame.
    SampleItems { of: String, items: Vec<String>, frames: u64, label: String },
    /// A container menu held open (opened by the player's activation of `via`), then
    /// closed; the first change of a quest variable after it.
    MenuGap { via: String, frames: u64, quest: String, var: String, after: u64, label: String },
    EvaluatePackage { of: String },
    IsCurrentPackage { of: String, package: String, label: String },
    // ---- round 3 of the contract (layer5-captures.md §9.2) ----
    /// `SetReputation rep axis value` (`axis`: "fame" or "infamy").
    SetReputation { rep: String, axis: String, value: f64 },
    /// `AddReputation rep axis size`.
    AddReputation { rep: String, axis: String, size: i32 },
    /// `RemoveReputation rep axis size`.
    RemoveReputation { rep: String, axis: String, size: i32 },
    /// Reputation fields, one frame each.
    GetReputation { rep: String, fields: Vec<String>, #[serde(default)] label: Option<String> },
    /// SetReputation fame, then infamy, and the fields in the same script.
    SetReputationRead { rep: String, fame: f64, infamy: f64, fields: Vec<String>, #[serde(default)] label: Option<String> },
    /// The actor uses an aid item (`EquipItem item 0 1`) and the reads follow in the
    /// same script; with `stopwatch`, the stopwatch's zero is set there.
    UseItem {
        #[serde(default)]
        actor: Option<String>,
        item: String,
        #[serde(default)]
        read: Vec<UseRead>,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        stopwatch: bool,
    },
    /// Waits until an actor value equals `value` (checked once per frame, at most
    /// `max_frames` frames); observation `found` and the stopwatch `seconds`.
    WaitAv { #[serde(default)] of: Option<String>, av: String, value: f64, max_frames: u64, #[serde(default)] label: Option<String> },
    /// The player's activation of the container `via` opens its menu for `frames` frames.
    HoldMenu { via: String, frames: u64 },
    DamageAv { #[serde(default)] of: Option<String>, av: String, value: f64 },
    RestoreAv { #[serde(default)] of: Option<String>, av: String, value: f64 },
    GetAv { #[serde(default)] of: Option<String>, name: String, #[serde(default)] label: Option<String> },
    SetAv { #[serde(default)] of: Option<String>, name: String, value: f64 },
    AddSpell { #[serde(default)] of: Option<String>, spell: String },
    RemoveSpell { #[serde(default)] of: Option<String>, spell: String },
    /// An actor value and the stopwatch once per frame: `changes`, `per_second`.
    SampleAv { #[serde(default)] of: Option<String>, av: String, frames: u64, #[serde(default)] label: Option<String> },
    /// Once per frame: the value is read, then damaged by `damage`; `values`.
    SampleAvDamage { #[serde(default)] of: Option<String>, av: String, damage: f64, frames: u64, #[serde(default)] label: Option<String> },
    /// The console's `coc <cell>`: the player moves to the cell's COC marker.
    GoToCell { cell: String },
    /// Several series once per frame; `lags` of each series' first jump behind the
    /// first series'.
    SampleJumps { reads: Vec<JumpRead>, frames: u64, min_step: f64, #[serde(default)] label: Option<String> },
    // ---- round 4 of the contract (layer5-captures.md §10.4) ----
    /// The stopwatch's zero is now (seconds of game time, the time scripts count).
    StartStopwatch {},
    /// Up to `max` times: wait (checked each frame, at most `wait_frames`) for a message
    /// box, then close menus as the player's OK would; `closed`: the boxes that were open.
    DismissMessageBoxes { max: u32, wait_frames: u64, #[serde(default)] label: Option<String> },
    /// Close the open menu as the player's OK / Done would (CloseAllMenus).
    CloseMenus {},
    /// Wait (checked each frame, at most `max_frames`) until a quest field equals
    /// `value`; one frame later `found` (the field read again) and, with `stopwatch`,
    /// `seconds` on the stopwatch.
    WaitQuest { quest: String, field: String, value: Json, max_frames: u64, label: String, #[serde(default)] stopwatch: bool },
    /// The same for "menu `menu` is open".
    WaitMenu { menu: u16, max_frames: u64, label: String, #[serde(default)] stopwatch: bool },
    /// Whether each menu is open (`menu.<code>`), one frame each.
    GetMenus { menus: Vec<u16>, label: String },
    /// IsPlayerTagSkill of each skill (`tag.<Skill>`), one frame each.
    GetTagSkills { skills: Vec<String>, label: String },
    // ---- headless stand-ins (scripts.md §8) ----
    /// Seconds of game loop at the fixed frame step.
    AdvanceSeconds { seconds: f64 },
    /// An actor (the player by default) activates by hand: OnActivate instead of the
    /// default action.
    /// With `label`, observation `info` when the activation opened a conversation.
    PlayerActivate { target: String, #[serde(default)] actor: Option<String>, #[serde(default)] label: Option<String> },
    /// The player activates `target` by hand and picks the perks' activate choice whose
    /// label is `choice` (Robot Compliance's "Deactivate", Cannibal's "Devour"): the
    /// choice list opens as in play (`Game::open_activate_menu`) and the choice's script
    /// runs on the target (`Game::pick_activate_choice`). A step error when the list
    /// does not open or does not offer the choice. Observation `choices`: the labels.
    PerkChoice { target: String, choice: String, #[serde(default)] label: Option<String> },
    /// The player crouches (`sneaking` true) or stands up, standing still: the engine's
    /// measured motion of the player (`Game::set_motion`), as the play loop tells it from
    /// the sneak control; IsSneaking and the perks' IsSneaking conditions read it.
    PlayerSneak { sneaking: bool },
    /// A dialogue line (INFO) is said: its result scripts run on the speaker.
    SayInfo { info: String, #[serde(default)] speaker: Option<String>, #[serde(default)] force: bool },
    /// The first of several lines whose conditions pass is said (the speaker picks the
    /// line that fits, as dialogue does); observation `said`: the INFO, or none.
    SayFirst { infos: Vec<String>, #[serde(default)] speaker: Option<String>, #[serde(default)] label: Option<String> },
    /// A package starts, finishes or changes on an actor.
    Package { actor: String, package: String, event: String },
    /// An actor's Dialogue package reaches the one it is to talk to, as the AI layer
    /// carries it out (fnv-ai `carry.rs`, docs/spec-notes/packages.md): a Say To package
    /// says its topic at the target, any other force-greets the player with the
    /// package's topic (a conversation opens); the package's goal is met. `package`: the
    /// actor's current package when left out. With `label`, observation `info`: the
    /// line said.
    DialoguePackage { actor: String, #[serde(default)] package: Option<String>, #[serde(default)] label: Option<String> },
    /// A `SayTo` line finished (SayToDone blocks of the speaker).
    SayToDone { actor: String, #[serde(default)] topic: Option<String> },
    /// An actor walking a patrol reaches one of its points: the point's patrol script
    /// runs on the actor and its topic is said (`Game::patrol_point_reached`, as the AI
    /// layer calls it).
    PatrolPoint { actor: String, point: String },
    /// An actor (the player by default) fires a weapon it carries: the weapon's OnFire
    /// blocks run (`Game::weapon_fired`, as the play loop calls it).
    Fire { weapon: String, #[serde(default)] actor: Option<String> },
    /// An actor enters a trigger (OnTriggerEnter of the trigger reference), or with
    /// `event` "inside" / "leave" is in it this frame (OnTrigger) / leaves it
    /// (OnTriggerLeave). With `frames` ("inside" only) the actor stays in it that many
    /// frames, OnTrigger running in each, the game going on between them (a trigger that
    /// counts the time spent inside).
    TriggerEnter {
        trigger: String,
        #[serde(default)]
        actor: Option<String>,
        #[serde(default)]
        event: Option<String>,
        #[serde(default)]
        frames: Option<u64>,
    },
    /// The player picks a button of the open message box.
    PressButton { button: i32 },
    /// A menu opens (MenuMode with its code) or closes (`null`).
    Menu { menu: Option<u16> },
    /// The player moves to a reference (and its cells load).
    MovePlayer { to: String },
    LoadCell { cell: String },
    /// A script variable of a quest or reference, by name.
    GetVar { of: String, var: String, #[serde(default)] label: Option<String> },
    /// With `fields` (round 2, `ref`): `var.<name>`, `dead`, `disabled`, one frame each.
    /// Without (stand-in, `of`): enabled state, cell, position and Layer 5 flags.
    GetRef {
        #[serde(default)]
        of: Option<String>,
        #[serde(default, rename = "ref")]
        reference: Option<String>,
        #[serde(default)]
        fields: Option<Vec<String>>,
        #[serde(default)]
        label: Option<String>,
    },
    /// `target.Kill` (round 2) or `actor.KillActor killer` (stand-in).
    Kill {
        #[serde(default)]
        target: Option<String>,
        #[serde(default)]
        actor: Option<String>,
        #[serde(default)]
        killer: Option<String>,
    },
    /// A shot or blow landing on a reference (stand-in for an attacker's weapon reaching
    /// it, the player's by default). On a reference that is not an actor its OnHit and
    /// OnHitWith blocks run (`Game::hit_object`); observation `ran`: whether a block ran.
    /// On an actor it is a hit of `damage` health (0 by default: the blow provokes and
    /// nothing more), `Game::hit`, and one it lives through may turn on the attacker as
    /// in play (`provoked`); observations `health`, `died`.
    Hit {
        target: String,
        #[serde(default)]
        attacker: Option<String>,
        #[serde(default)]
        weapon: Option<String>,
        #[serde(default)]
        damage: Option<f64>,
        #[serde(default)]
        label: Option<String>,
    },
    /// An actor (the player by default) consumes an ingestible it carries (the engine's
    /// use of an aid item): one is removed and its effects start, effect scripts
    /// included (`Game::consume`); observation `effects`: the effects started.
    Consume { item: String, #[serde(default)] actor: Option<String>, #[serde(default)] label: Option<String> },
    /// The player plays a holotape (a NOTE) from the Pip-Boy: its line's begin script,
    /// then its end script (`Game::holotape_script`; the voice's length is not waited
    /// for); observation `info`: the line, null for a note that plays none.
    PlayNote { note: String, #[serde(default)] label: Option<String> },
    /// A comment in the input (why the next stand-in is there); no effect, no frames.
    Note { text: String },
    /// A console line run as the console runs it (`functions::console::call_words`):
    /// `reference.Function words…`. Observation `value` (a number, or a reference as its
    /// FormID text), and `stub` when the function is one; a statement leaves out `label`.
    /// The function probes' side of ours (tools/capture/function_probe.py).
    Call { line: String, #[serde(default)] label: Option<String> },
    /// A whole function script compiled with the extender's compiler and run once on no
    /// reference, as nvf_capture's query step (`Game::run_query`). Observation `value` (a
    /// number, a reference as its FormID text, or text), or `error`: "script did not
    /// compile" or the run-time error. What it changes stays for later actions.
    Query { script: String, label: String, #[serde(default)] text: bool },
    /// A console line run once per frame until it gives a non-zero number, at most
    /// `max_frames` times (the capture plugin's `until`). Observation `frames`, or an
    /// error when it never did.
    CallUntil { line: String, max_frames: u64, label: String },
    // ---- conversations (docs/spec-notes/dialogue.md §8) ----
    /// A conversation between the player and `actor` starts (`Game::start_dialogue`):
    /// the player activating the actor, or the actor's dialogue package; with `topic`,
    /// that topic instead of GREETING. Observation `info`: the line said.
    Talk { actor: String, #[serde(default)] topic: Option<String>, #[serde(default)] label: Option<String> },
    /// Waits (at most `max_frames`) until the actor has finished speaking, then picks
    /// one of the choices offered: by `topic` (Editor ID or FormID), by `prompt` (the
    /// text shown, ignoring case; a part of it when no entry matches whole) or by
    /// `index` (0-based). Observation `info`: the line said in answer.
    Choose {
        #[serde(default)]
        topic: Option<String>,
        #[serde(default)]
        prompt: Option<String>,
        #[serde(default)]
        index: Option<usize>,
        #[serde(default = "talk_frames")]
        max_frames: u64,
        #[serde(default)]
        label: Option<String>,
    },
    /// Waits (at most `max_frames`) until choices are offered or the conversation has
    /// ended; with `start`, first until a conversation is running (one a script starts
    /// with StartConversation). Observations `state` (`choosing`, `ended`), `choices`
    /// (how many) and, with `start`, `actor`.
    WaitTalk {
        #[serde(default = "talk_frames")]
        max_frames: u64,
        #[serde(default)]
        start: bool,
        #[serde(default)]
        label: Option<String>,
    },
    /// The player leaves the conversation (`Game::end_dialogue`); nothing when none is
    /// running.
    EndTalk {},
    // ---- terminals (docs/spec-notes/hacking.md §6) ----
    /// The player picks entry `item` (0-based) of a terminal menu at reference `target`
    /// (`terminal::choose`): of `menu` (a TERM, for a sub menu) or the reference's own
    /// base; refused when the entry is not listed now (its conditions fail). Observations
    /// `submenu`, `note` and `back`.
    Terminal {
        target: String,
        item: usize,
        #[serde(default)]
        menu: Option<String>,
        #[serde(default)]
        label: Option<String>,
    },
    /// The player saving and loading that save at once: the game's snapshot written into
    /// a save file's bytes (`fnv_save`, as the engine's saves), read back and restored
    /// in place of the running game (docs/design/save-load.md). The random numbers keep
    /// their position, so a chain goes on as it would have; the trace, the coverage and
    /// the session's event handlers carry over. Observation `bytes` (the save's size).
    SaveLoad {
        #[serde(default)]
        label: Option<String>,
    },
}

/// [`Action::SaveLoad`]: the save's size in bytes.
fn save_and_load(g: &mut Game) -> Result<usize, String> {
    let snap = g.snapshot();
    let body = serde_json::to_vec(&snap).map_err(|e| e.to_string())?;
    let bytes = fnv_save::encode(&fnv_save::SaveHeader::new(fnv_save::SaveKind::Quick), &[], &body);
    let file = fnv_save::decode(std::path::Path::new("runner save"), &bytes, true).map_err(|e| e.to_string())?;
    let back: crate::game::GameSnapshot = serde_json::from_slice(&file.body).map_err(|e| e.to_string())?;
    let world = World::restore(g.world.forms, g.world.placed, &back.sim)?;
    let mut n = Game::restore(g.data, world, g.cfg, &back)?;
    n.world.put_session(g.world.take_session());
    n.put_session(g.take_session());
    n.voice = g.voice.take();
    n.coverage = g.coverage.take();
    n.events = std::mem::take(&mut g.events);
    n.unhandled = std::mem::take(&mut g.unhandled);
    n.stubbed = std::mem::take(&mut g.stubbed);
    *g = n;
    Ok(bytes.len())
}

/// Frames that pass after an action (layer5-captures.md §4.1).
fn frames_after(a: &Action) -> u64 {
    match a {
        Action::SetStage { .. } | Action::SetStageRead { .. } => 31,
        Action::StartQuest { .. } | Action::StopQuest { .. } | Action::SetQuestVar { .. } | Action::SetObjective { .. } => 11,
        Action::SetLocal { .. } => 1,
        Action::Activate { .. } | Action::RemoveAllItems { .. } => 31,
        Action::AddItem { .. } | Action::RemoveItem { .. } | Action::Enable { .. } | Action::Disable { .. } | Action::SetGlobal { .. } => 11,
        // A read is one query step with no pause after it (capture_steps.py adds the
        // pause only after steps that change the game).
        Action::GetItemCount { .. } | Action::GetDisabled { .. } | Action::GetGlobal { .. } => 1,
        Action::SetRefVar { .. } => 11,
        Action::ActivateRead { .. } => 31,
        Action::Kill { .. } | Action::EvaluatePackage { .. } => 61,
        Action::Hit { .. } => 11,
        Action::AddItemRead { .. } | Action::IsCurrentPackage { .. } => 1,
        Action::GetRef { fields: Some(_), .. } => 0,
        Action::GetQuest { .. } | Action::SampleVar { .. } | Action::WaitFrames { .. } | Action::AdvanceSeconds { .. } => 0,
        Action::SampleRefVar { .. } | Action::SampleItems { .. } | Action::MenuGap { .. } => 0,
        Action::Note { .. } => 0,
        // Saving and loading take no game time: the chain goes on in the same frame.
        Action::SaveLoad { .. } => 0,
        // As the probes' game jobs: a query has no pause after it, an action 10 frames.
        Action::Call { label: Some(_), .. } | Action::CallUntil { .. } | Action::Query { .. } => 0,
        Action::Call { label: None, .. } => 10,
        // Round 3 (layer5-captures.md §9.2).
        Action::SetReputation { .. } | Action::AddReputation { .. } | Action::RemoveReputation { .. } => 11,
        Action::GetReputation { .. } => 0,
        Action::SetReputationRead { .. } => 1,
        Action::UseItem { .. } | Action::DamageAv { .. } | Action::RestoreAv { .. } | Action::GetAv { .. } => 11,
        Action::AddSpell { .. } | Action::RemoveSpell { .. } => 11,
        Action::SetAv { .. } => 31,
        Action::WaitAv { .. } | Action::SampleAv { .. } | Action::SampleAvDamage { .. } | Action::SampleJumps { .. } => 0,
        // 1 + frames + 5: the menu's frames pass inside the step.
        Action::HoldMenu { .. } => 6,
        Action::GoToCell { .. } => 600,
        // Round 4 (layer5-captures.md §10.4).
        Action::StartStopwatch { .. } => 3,
        Action::CloseMenus { .. } => 4,
        Action::DismissMessageBoxes { .. } | Action::WaitQuest { .. } | Action::WaitMenu { .. } => 0,
        Action::GetMenus { .. } | Action::GetTagSkills { .. } => 0,
        // Stand-ins: one frame.
        _ => 1,
    }
}

/// Name resolution: `player`, a hex FormID, or an Editor ID (any record);
/// `scripted:<script>[#n]`, `placed:<base>` and `created:<base>` (the newest reference
/// made at run time from that base, e.g. by PlaceAtMe) name references by what they are.
pub fn resolve(g: &Game, s: &str) -> Result<FormId, String> {
    if s.eq_ignore_ascii_case("player") || s.eq_ignore_ascii_case("playerref") {
        return Ok(FormId::PLAYER_REF);
    }
    if s.starts_with("0x") || s.starts_with("0X") {
        return s.parse().map_err(|e: fnv_sim::form::BadFormId| e.to_string());
    }
    if let Some(script) = s.strip_prefix("scripted:") {
        // The placed reference whose base form runs this script (the first in FormID
        // order when several do).
        // `scripted:<script>#<n>`: the n-th (1-based) of them.
        let (script, n) = match script.split_once('#') {
            Some((s, n)) => (s, n.parse::<usize>().map_err(|_| format!("bad index in {s:?}"))?),
            None => (script, 1),
        };
        let sc = resolve(g, script)?;
        return g
            .world
            .placed
            .refs
            .values()
            .filter(|p| g.data.script_of_base(p.base) == Some(sc))
            .nth(n.saturating_sub(1))
            .map(|p| p.id)
            .ok_or_else(|| format!("no placed reference number {n} runs {script}"));
    }
    if let Some(base) = s.strip_prefix("placed:") {
        // The first placed reference (FormID order) of a base form.
        let b = resolve(g, base)?;
        return g.world.placed.refs.values().find(|p| p.base == b).map(|p| p.id).ok_or_else(|| format!("no placed reference of {base}"));
    }
    if let Some(base) = s.strip_prefix(fnv_sim::actions::CREATED_PREFIX) {
        // The newest reference made at run time (PlaceAtMe, a drop) of a base form that
        // is still there, as the sim runner names it: its FormID is not known in advance.
        let b = resolve(g, base)?;
        return g.world.newest_created(b).ok_or_else(|| format!("no reference of {base} was created"));
    }
    if let Some(r) = g.world.placed.by_edid(s) {
        return Ok(r);
    }
    g.data.by_edid(s).or_else(|| g.world.placed.cell_by_edid(s)).ok_or_else(|| format!("no form named {s:?}"))
}

fn quest(g: &Game, s: &str) -> Result<FormId, String> {
    let q = resolve(g, s)?;
    if g.data.is_quest(q) {
        Ok(q)
    } else {
        Err(format!("{s:?} is not a quest"))
    }
}

/// A quest variable by name: (instance, index).
fn quest_var(g: &Game, q: FormId, var: &str) -> Result<(InstKey, u16), String> {
    let def = &g.data.quests[&q];
    let script = def.script.and_then(|s| g.data.script(s)).ok_or_else(|| format!("{} has no script", g.data.name(q)))?;
    let i = script.var_index(var).ok_or_else(|| format!("{} declares no variable {var:?}", g.data.name(q)))?;
    Ok((InstKey::Quest { quest: q }, i))
}

fn var_of(g: &Game, of: FormId, var: &str) -> Result<(InstKey, FormId, u16), String> {
    let (key, script) = g.instance_of(of).ok_or_else(|| format!("{} runs no script", g.data.name(of)))?;
    let i = g.data.script(script).and_then(|s| s.var_index(var)).ok_or_else(|| format!("no variable {var:?}"))?;
    Ok((key, script, i))
}

/// Store a number in a quest variable as given (xNVSE SetVariable in the capture),
/// with the variable's declared storage.
fn set_quest_var(g: &mut Game, q: FormId, var: &str, value: f64) -> Result<(), String> {
    let (key, i) = quest_var(g, q, var)?;
    let script = g.data.quests[&q].script;
    g.store(key, script, i, Value::Num(value), "runner", false);
    Ok(())
}

/// An object-script variable of a reference by name: (instance, script, index), or
/// `None` for a name its script does not declare.
fn ref_var(g: &Game, r: FormId, var: &str) -> Result<(InstKey, FormId, Option<u16>), String> {
    let (key, script) = g.instance_of(r).ok_or_else(|| format!("{} runs no script", g.data.name(r)))?;
    Ok((key, script, g.data.script(script).and_then(|s| s.var_index(var))))
}

/// A field of a reference (`get_ref`, layer5-captures.md §8.2): `dead`, `disabled`, or
/// `var.<name>`. A name the reference's script does not declare reads 0 [observed:
/// xNVSE's `ref.GetVariable` of an undeclared name returned 0, §8.3 f].
fn ref_field(g: &mut Game, r: FormId, f: &str) -> Result<Json, String> {
    Ok(match f {
        "dead" => json!(g.extra.get(&r).is_some_and(|e| e.dead)),
        "destroyed" => json!(g.extra.get(&r).is_some_and(|e| e.destroyed)),
        "disabled" => json!(!g.world.is_enabled(r).map_err(|e| e.to_string())?),
        _ => match f.strip_prefix("var.") {
            Some(v) => {
                let (key, _, i) = ref_var(g, r, v)?;
                json!(i.map(|i| g.var(key, i).num()).unwrap_or(0.0))
            }
            None => return Err(format!("unknown reference field {f:?}")),
        },
    })
}

/// Whether an activation since event `since` opened a container (its menu).
fn opened_since(g: &Game, since: usize) -> bool {
    g.events[since..]
        .iter()
        .any(|e| matches!(&e.kind, EventKind::Activated { outcome: Some(v), .. } if v["result"] == "opened_container"))
}

/// The containers the player's activations since event `since` opened (their menus).
fn containers_opened_since(g: &Game, since: usize) -> Vec<FormId> {
    g.events[since..]
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Activated { by, outcome: Some(v), .. } if *by == FormId::PLAYER_REF && v["result"] == "opened_container" => serde_json::from_value(v["container"].clone()).ok(),
            _ => None,
        })
        .collect()
}

/// The first of the player's activations since event `since` whose outcome is to talk
/// (`fnv_sim::world::Outcome::Talk`): the actor or talking activator talked to.
fn talk_since(g: &Game, since: usize) -> Option<FormId> {
    g.events[since..].iter().find_map(|e| match &e.kind {
        EventKind::Activated { by, outcome: Some(v), .. } if *by == FormId::PLAYER_REF && v["result"] == "talk" => serde_json::from_value(v["actor"].clone()).ok(),
        _ => None,
    })
}

/// A script made the player activate something whose default action is to talk (since
/// event `since`): the conversation opens, as the play loop's `talk_after` opens it
/// (nvf-play script.rs). The REPCONN intercom's trigger (SCPT 0x000CD04E) runs
/// `<intercom>.Activate player` after SetTalkingActivatorActor: the player is the
/// activator and the intercom answers Talk [observed: `fnvtool script decompile`
/// FalloutNV.esm]. An actor with nothing to say opens none. Returns who was talked to.
fn scripted_talk(g: &mut Game, since: usize) -> Option<FormId> {
    if g.dialogue.is_some() {
        return None;
    }
    let who = talk_since(g, since)?;
    g.start_dialogue(who, None).ok().map(|_| who)
}

/// The menu code of the container menu [observed: `MenuMode 1008` was true while a
/// container menu opened by Activate was up, layer5-captures.md §8.3].
pub const CONTAINER_MENU: u16 = 1008;

fn activation_mode(run_on_activate: bool) -> ActMode {
    if run_on_activate {
        ActMode::Block
    } else {
        ActMode::Default
    }
}

/// A sample series as `sample_var` reports it (layer5-captures.md §4.1, §8.2):
/// `changes`, or `drops` with `runs`/`skip`; with `values`, the successive distinct
/// values of the samples.
fn sample_observation(o: &mut Json, start: f64, samples: &[f64], runs: Option<usize>, skip: usize, values: bool) {
    let mut prev = start;
    let mut drops = Vec::new();
    for &v in samples {
        if v != prev {
            drops.push(prev - v);
            prev = v;
        }
    }
    match runs {
        Some(n) => o["drops"] = json!(drops.iter().skip(skip).take(n).collect::<Vec<_>>()),
        None => o["changes"] = json!(drops.len()),
    }
    if values {
        let mut distinct: Vec<f64> = Vec::new();
        for &v in samples {
            if distinct.last() != Some(&v) {
                distinct.push(v);
            }
        }
        o["values"] = json!(distinct);
    }
}

fn field(g: &Game, q: FormId, f: &str) -> Result<Json, String> {
    let s = g.quest_state(q);
    Ok(match f {
        "running" => json!(s.running),
        "completed" => json!(s.completed || s.failed),
        // The quest's failed flag alone (a stage with the Fail Quest flag ran), which
        // `completed` (GetQuestCompleted) folds in; the walkthroughs' fail paths read it.
        // No script function reads it [observed: QSDT flag 2 "Fail Quest", FalloutNV.esm].
        "failed" => json!(s.failed),
        "stage" => json!(s.stage()),
        _ => {
            if let Some(n) = f.strip_prefix("done.") {
                let n: i32 = n.parse().map_err(|_| format!("bad field {f:?}"))?;
                json!(s.stages_done.contains(&n))
            } else if let Some(rest) = f.strip_prefix("objective.") {
                let (n, which) = rest.split_once('.').ok_or_else(|| format!("bad field {f:?}"))?;
                let n: i32 = n.parse().map_err(|_| format!("bad field {f:?}"))?;
                let o = s.objectives.get(&n).copied().unwrap_or_default();
                match which {
                    "displayed" => json!(o.displayed),
                    "completed" => json!(o.completed),
                    _ => return Err(format!("bad field {f:?}")),
                }
            } else if let Some(v) = f.strip_prefix("var.") {
                let (key, i) = quest_var(g, q, v)?;
                json!(g.var(key, i).num())
            } else {
                return Err(format!("unknown quest field {f:?}"));
            }
        }
    })
}

/// The literal text the capture writes for a number (layer5-captures.md §4.1): a whole
/// number without a decimal point, anything else with one.
fn literal_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e18 {
        format!("{}", v.abs() as u64)
    } else {
        let s = format!("{}", v.abs());
        if s.contains('.') {
            s
        } else {
            format!("{s}.0")
        }
    }
}

/// `set x to <literal>` into a fresh local of the given type, read back (the compiler
/// writes a negative literal as the positive one and a unary minus, scripts.md §2.3).
pub fn set_local(ty: &str, value: f64) -> Result<f64, String> {
    let text = literal_text(value);
    let mut v = literal(&text, value.abs());
    if value < 0.0 {
        v = -v;
    }
    match ty {
        "short" | "long" | "int" => Ok(to_int_storage(v) as f64),
        "float" | "ref" => Ok(v),
        _ => Err(format!("unknown variable type {ty:?}")),
    }
}

fn label(l: &Option<String>, i: usize) -> String {
    l.clone().unwrap_or_else(|| format!("step{i}"))
}

/// Run a console line (`reference.Function words…`) through the console's reading of
/// its words.
fn call_line(g: &mut Game, line: &str) -> Result<crate::functions::console::Called, String> {
    let (on, func, args) = crate::functions::console::split_call(line).ok_or("an empty line")?;
    let on = match on {
        Some(n) => Some(crate::functions::console::form(g, &n)?),
        None => None,
    };
    crate::functions::console::call_words(g, on, &func, &args, "runner")
}

/// Call a script function as a script would (`on.Func args`).
fn call(g: &mut Game, opcode: u16, on: Option<FormId>, args: Vec<ArgValue>) -> Value {
    let ctx = Ctx {
        unit: "runner".into(),
        opcode,
        on,
        self_ref: None,
        action_ref: None,
        quest: None,
        inst: None,
        in_condition: false,
    };
    g.call_function(&ctx, &args)
}

fn form_arg(f: FormId) -> ArgValue {
    ArgValue::Value(Value::Ref(f.0))
}
fn num_arg(x: f64) -> ArgValue {
    ArgValue::Value(Value::Num(x))
}

fn apply_given(g: &mut Game, gv: &Given) -> Result<(), String> {
    match gv {
        Given::Quest { quest: name, running, completed, stage, done, not_done, objectives, vars, refs } => {
            let q = quest(g, name)?;
            for (k, v) in vars {
                set_quest_var(g, q, k, *v)?;
            }
            for (k, v) in refs {
                let (key, i) = quest_var(g, q, k)?;
                let r = resolve(g, v)?;
                g.store(key, g.data.quests[&q].script, i, Value::Ref(r.0), "runner", false);
            }
            let s = g.quest(q);
            if let Some(r) = running {
                s.running = *r;
            }
            if let Some(c) = completed {
                s.completed = *c;
                if !c {
                    s.failed = false;
                }
            }
            for d in done {
                s.stages_done.insert(*d);
            }
            for d in not_done {
                s.stages_done.remove(d);
            }
            if let Some(st) = stage {
                if s.stage() != *st {
                    s.stage_override = Some(*st);
                }
            }
            for o in objectives {
                let e = s.objectives.entry(o.index).or_default();
                if let Some(d) = o.displayed {
                    e.displayed = d;
                }
                if let Some(c) = o.completed {
                    e.completed = c;
                }
            }
        }
        Given::Global { name, value } => {
            let id = resolve(g, name)?;
            g.world.set_global(id, *value).map_err(|e| e.to_string())?;
        }
        Given::ItemCount { of, item, count } => {
            let (of, item) = (resolve(g, of)?, resolve(g, item)?);
            let have = g.world.item_count(of, item).map_err(|e| e.to_string())?;
            if have < *count {
                g.world.add_item(of, item, count - have).map_err(|e| e.to_string())?;
            } else if have > *count {
                g.world.remove_item(of, item, have - count).map_err(|e| e.to_string())?;
            }
        }
        Given::ItemChange { of, item, change } => {
            let (of, item) = (resolve(g, of)?, resolve(g, item)?);
            if *change > 0 {
                g.world.add_item(of, item, *change).map_err(|e| e.to_string())?;
            } else if *change < 0 {
                g.world.remove_item(of, item, -change).map_err(|e| e.to_string())?;
            }
        }
        Given::Disabled { of, disabled } => {
            let r = resolve(g, of)?;
            g.world.set_enabled(r, !disabled).map_err(|e| e.to_string())?;
        }
        Given::Ref { reference, vars, dead, disabled } => {
            let r = resolve(g, reference)?;
            for (k, v) in vars {
                let (key, script, i) = ref_var(g, r, k)?;
                let i = i.ok_or_else(|| format!("the script of {reference} declares no variable {k:?}"))?;
                g.store(key, Some(script), i, Value::Num(*v), "runner", false);
            }
            if let Some(d) = dead {
                g.extra.entry(r).or_default().dead = *d;
            }
            if let Some(d) = disabled {
                g.world.set_enabled(r, !d).map_err(|e| e.to_string())?;
            }
        }
        Given::Reputation { rep: name, fame, infamy } => {
            let r = rep(g, name)?;
            for (axis, v) in [(1, fame), (0, infamy)] {
                if let Some(v) = v {
                    crate::functions::reputation_store(g, r, axis, *v);
                }
            }
        }
        Given::ActorValue { of, av, value } => {
            let who = resolve(g, of)?;
            let code = av_code(g, av)?;
            set_actor_value_current(g, who, code, *value)?;
        }
        Given::Place { reference, cell, pos, rot } => {
            let r = resolve(g, reference)?;
            let at = resolve(g, cell)?;
            // A worldspace stands for its exterior cell at the position; anything else must
            // be a cell (as `go_to_cell`).
            let c = match g.world.placed.world_cell_at(at, *pos) {
                Some(c) => c,
                None if g.world.placed.by_cell.contains_key(&at) || g.world.placed.world_of(at).is_some() => at,
                None => return Err(format!("{cell:?} is neither a cell nor a worldspace with a cell at {pos:?}")),
            };
            {
                let p = g.world.get(r).map_err(|e| e.to_string())?;
                p.cell = Some(c);
                p.pos = *pos;
                p.rot = *rot;
            }
            g.emit(EventKind::Moved { reference: r, cell: Some(c), pos: *pos });
            if r == FormId::PLAYER_REF {
                g.player_entered(Some(c));
                g.settle_start_weather();
            }
        }
        Given::TagSkills { skills } => {
            let codes = skills.iter().map(|s| av_code(g, s)).collect::<Result<Vec<_>, _>>()?;
            g.player.tag_skills = codes.into_iter().collect();
        }
        Given::SkillPoints { skill, value } => {
            let code = av_code(g, skill)?;
            g.player.skill_points.insert(code, *value);
        }
        Given::Menu { menu, open } => {
            if *open {
                if *menu == menus::MESSAGE {
                    // A box of a message the input does not name, in front of any other.
                    g.put_box_in_front(FormId(0));
                }
                g.open_menu(*menu);
            } else if g.menu == Some(*menu) {
                g.close_menu();
            }
        }
    }
    Ok(())
}

// ---- round 3 helpers (layer5-captures.md §9.2) ----

/// A reputation (REPU) by name.
fn rep(g: &Game, s: &str) -> Result<FormId, String> {
    let r = resolve(g, s)?;
    if g.data.reputations.contains_key(&r) || g.data.sig(r).as_ref() == Some(b"REPU") {
        Ok(r)
    } else {
        Err(format!("{s:?} is not a reputation"))
    }
}

/// The reputation axis argument: 1 fame, 0 infamy [observed: layer5-captures.md §9.3 a].
fn rep_axis(s: &str) -> Result<f64, String> {
    match s {
        "fame" => Ok(1.0),
        "infamy" => Ok(0.0),
        o => Err(format!("reputation axis {o:?} is not \"fame\" or \"infamy\"")),
    }
}

/// A reputation field (§9.2): `fame`, `infamy` (GetReputation), `pct.fame`,
/// `pct.infamy` (GetReputationPct), `threshold.mixed|good|bad` (GetReputationThreshold
/// 0, 1, 2), read through the script functions.
fn rep_field(g: &mut Game, r: FormId, f: &str) -> Result<Json, String> {
    let (opcode, axis) = match f {
        "fame" => (0x123D, 1.0),
        "infamy" => (0x123D, 0.0),
        "pct.fame" => (0x123E, 1.0),
        "pct.infamy" => (0x123E, 0.0),
        "threshold.mixed" => (0x123F, 0.0),
        "threshold.good" => (0x123F, 1.0),
        "threshold.bad" => (0x123F, 2.0),
        _ => return Err(format!("unknown reputation field {f:?}")),
    };
    let v = call(g, opcode, None, vec![form_arg(r), num_arg(axis)]).num();
    Ok(if opcode == 0x123F { json!(v as i64) } else { json!(v) })
}

fn rep_fields(g: &mut Game, r: FormId, fields: &[String], obs: &mut Map<String, Json>, tick_each: bool) {
    let mut errors = Map::new();
    for f in fields {
        match rep_field(g, r, f) {
            Ok(v) => {
                obs.insert(f.clone(), v);
            }
            Err(e) => {
                errors.insert(f.clone(), json!(e));
            }
        }
        if tick_each {
            frame(g);
        }
    }
    if !errors.is_empty() {
        obs.insert("errors".into(), Json::Object(errors));
    }
}

fn actor_or_player(g: &Game, of: &Option<String>) -> Result<FormId, String> {
    match of {
        Some(a) => resolve(g, a),
        None => Ok(FormId::PLAYER_REF),
    }
}

/// Where `coc <cell>` puts the player: the cell's COCMarkerHeading reference (FalloutNV
/// STAT 0x00000032 [observed: `esm inventory`]) when it has one; otherwise, outdoors, the
/// middle of the cell at height 0, and indoors the first placed reference of the cell
/// [hypothesis, all of it: the position matters here only for which cells load].
fn coc_destination(g: &Game, cell: FormId) -> Result<[f32; 3], String> {
    const COC_MARKER: FormId = FormId(0x0000_0032);
    let placed = g.world.placed;
    let in_cell: Vec<FormId> = placed.by_cell.get(&cell).cloned().unwrap_or_default();
    if let Some(p) = in_cell.iter().filter_map(|r| placed.get(*r)).find(|p| p.base == COC_MARKER) {
        return Ok(p.pos);
    }
    if let Some(grid) = placed.world_of(cell).and_then(|w| placed.world_grid.get(&w)) {
        if let Some((&(x, y), _)) = grid.iter().find(|(_, &c)| c == cell) {
            const CELL: f32 = 4096.0;
            return Ok([(x as f32 + 0.5) * CELL, (y as f32 + 0.5) * CELL, 0.0]);
        }
    }
    in_cell.iter().filter_map(|r| placed.get(*r)).map(|p| p.pos).next().ok_or_else(|| format!("{} has no place to go to", g.data.name(cell)))
}

/// One `sample_jumps` series as a reader of the current value.
fn jump_value(g: &mut Game, r: &JumpRead) -> Result<f64, String> {
    match (&r.global, &r.reference, &r.quest, &r.var) {
        (Some(name), None, None, None) => {
            let id = resolve(g, name)?;
            Ok(g.world.global(id).map_err(|e| e.to_string())? as f64)
        }
        (None, Some(rf), None, Some(v)) => {
            let rf = resolve(g, rf)?;
            let (key, _, i) = ref_var(g, rf, v)?;
            let i = i.ok_or_else(|| format!("the script of {rf} declares no variable {v:?}"))?;
            Ok(g.var(key, i).num())
        }
        (None, None, Some(q), Some(v)) => {
            let q = quest(g, q)?;
            let (key, i) = quest_var(g, q, v)?;
            Ok(g.var(key, i).num())
        }
        _ => Err("a sample_jumps read is {\"global\"}, {\"ref\", \"var\"} or {\"quest\", \"var\"}".into()),
    }
}

/// Whether a read field equals a wanted value: booleans as booleans, numbers to 1e-6
/// (a boolean field also matches 1 / 0).
fn same_value(got: &Json, want: &Json) -> bool {
    match (got, want) {
        (Json::Bool(a), Json::Bool(b)) => a == b,
        (Json::Bool(a), Json::Number(n)) | (Json::Number(n), Json::Bool(a)) => n.as_f64() == Some(if *a { 1.0 } else { 0.0 }),
        (Json::Number(a), Json::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-6,
            _ => false,
        },
        _ => got == want,
    }
}

/// One frame of the run: the awareness update the engine's detection pass ends with
/// (`Game::settle_awareness`), with nothing measured, then the game's frame
/// (`Game::tick`). Headless there is no line of sight, so nobody is detected anew, but
/// what the update does with what is known happens as in play: a fight whose targets
/// all died or went ends (Combat to Normal, OnCombatEnd), alarms time out, a target
/// detected by a hit or StartCombat starts the fight (OnStartCombat). Not while a menu
/// or a conversation holds the game [hypothesis: the detection pass runs in game time,
/// as the AI does].
fn frame(g: &mut Game) {
    if !g.in_menu() {
        g.settle_awareness();
    }
    g.tick();
}

/// Run frames until `holds`, checked before each frame, at most `max` frames.
fn wait_until(g: &mut Game, max: u64, holds: impl Fn(&Game) -> Result<bool, String>) -> Result<bool, String> {
    for k in 0..=max {
        if holds(g)? {
            return Ok(true);
        }
        if k < max {
            frame(g);
        }
    }
    Ok(false)
}

/// The observation of `wait_quest` / `wait_menu`: `found`, and with the stopwatch and
/// found, `seconds`.
fn waited(g: &Game, st: &RunState, obs: &mut Map<String, Json>, found: bool, stopwatch: bool) -> Result<(), String> {
    obs.insert("found".into(), json!(found));
    if stopwatch && found {
        let t0 = st.stopwatch.ok_or("no stopwatch was started before this wait")?;
        obs.insert("seconds".into(), json!(g.time - t0));
    }
    Ok(())
}

/// Run frames until the conversation offers choices or has ended. A menu that is open
/// (a message box a result script showed) holds the conversation: that is an error here,
/// the input has to close it. A Goodbye greeting said at the player opens no
/// conversation (`Game::start_dialogue`): the wait goes on until its line has ended and
/// its end script has run, as the player hears it out (issue #1114). The same for a
/// chosen Goodbye line, which closes its conversation at once and is said on as a
/// farewell (`Game::choose`, `Speech::farewell`).
fn wait_talk(g: &mut Game, max: u64) -> Result<(), String> {
    for _ in 0..=max {
        match &g.dialogue {
            None if !goodbye_at_player(g) => return Ok(()),
            Some(c) if !c.choices.is_empty() => return Ok(()),
            _ => {}
        }
        if let Some(m) = g.menu {
            return Err(format!("menu {m} is open and holds the conversation"));
        }
        frame(g);
    }
    Err(format!("the actor was still speaking after {max} frames"))
}

/// Whether a Goodbye line is being said at the player outside a conversation: a greeting
/// or a farewell (`Speech::goodbye_at_player`).
fn goodbye_at_player(g: &Game) -> bool {
    g.speech.iter().any(|s| s.goodbye_at_player())
}

/// What the engine's detection pass does with a hit an actor lives through (`nvforever`
/// detection.rs): the actor may turn on its attacker (`Game::attacked_by`: assault,
/// friendly fire past the hits a friend forgives) and, when it would attack it, is
/// alarmed where the attacker stands (`Game::hurt_by`). Headless nothing is measured,
/// so the actor is taken to see whoever hit it, and the next frame's awareness update
/// starts the fight (`frame`) [hypothesis: a hit from out of sight leaves it searching
/// instead, which only the engine's line of sight can tell].
fn provoked(g: &mut Game, actor: FormId, by: FormId) {
    g.attacked_by(actor, by);
    let at = crate::awareness::position(g, by).unwrap_or_default();
    if g.hurt_by(actor, by, at) {
        let seen = crate::detection::Detection { value: 100.0, band: crate::detection::Band::Detected, seen: true, ..Default::default() };
        g.observe_threat(actor, by, &seen, at);
    }
}

/// The entry of the offered list an input names.
fn pick_choice(g: &Game, topic: &Option<String>, prompt: &Option<String>, index: &Option<usize>) -> Result<usize, String> {
    let choices = g.dialogue_choices();
    if choices.is_empty() {
        return Err("no choices are offered (the conversation has ended)".into());
    }
    let listed = || choices.iter().map(|c| format!("{} {:?}", g.data.name(c.topic), g.choice_text(c))).collect::<Vec<_>>().join(", ");
    match (topic, prompt, index) {
        (Some(t), None, None) => {
            let t = resolve(g, t)?;
            choices.iter().position(|c| c.topic == t).ok_or_else(|| format!("{} is not offered (offered: {})", g.data.name(t), listed()))
        }
        (None, Some(p), None) => {
            let want = p.trim().to_lowercase();
            let text = |c: &crate::dialogue::Choice| g.choice_text(c).trim().to_lowercase();
            let whole: Vec<usize> = (0..choices.len()).filter(|&i| text(&choices[i]) == want).collect();
            let part: Vec<usize> = (0..choices.len()).filter(|&i| text(&choices[i]).contains(&want)).collect();
            match (whole.as_slice(), part.as_slice()) {
                ([i, ..], _) => Ok(*i),
                ([], [i]) => Ok(*i),
                ([], []) => Err(format!("no choice reads {p:?} (offered: {})", listed())),
                _ => Err(format!("{p:?} matches several choices (offered: {})", listed())),
            }
        }
        (None, None, Some(i)) if *i < choices.len() => Ok(*i),
        (None, None, Some(i)) => Err(format!("no choice {i} ({} offered: {})", choices.len(), listed())),
        _ => Err("choose needs exactly one of 'topic', 'prompt' and 'index'".into()),
    }
}

/// The runner's own state across actions.
#[derive(Default)]
struct RunState {
    /// Game time ([`Game::time`], the script time GetSecondsPassed counts) at the last
    /// `use_item` with `stopwatch` (layer5-captures.md §9.2, "The stopwatch"), the last
    /// `start_stopwatch` or `set_quest_var` with `stopwatch` (§10.4).
    stopwatch: Option<f64>,
}

/// Run one action; returns its observation fields (for labelled actions).
/// A labelled observation: label and fields.
type Observed = Option<(String, Map<String, Json>)>;

fn step(g: &mut Game, st: &mut RunState, a: &Action) -> Result<Observed, String> {
    let mut obs = Map::new();
    let mut lab: Option<String> = None;
    match a {
        Action::SetStage { quest: q, stage } => {
            let q = quest(g, q)?;
            g.set_stage(q, *stage);
        }
        Action::SetStageRead { quest: q, stage, field: f, label: l } => {
            let q = quest(g, q)?;
            g.set_stage(q, *stage);
            obs.insert(f.clone(), field(g, q, f)?);
            lab = Some(l.clone());
        }
        Action::StartQuest { quest: q } => {
            let q = quest(g, q)?;
            g.start_quest(q);
        }
        Action::StopQuest { quest: q } => {
            let q = quest(g, q)?;
            g.stop_quest(q);
        }
        Action::SetQuestVar { quest: q, var, value, stopwatch } => {
            let q = quest(g, q)?;
            set_quest_var(g, q, var, *value)?;
            if *stopwatch {
                st.stopwatch = Some(g.time);
            }
        }
        Action::SetLocal { ty, value, label: l } => {
            obs.insert("value".into(), json!(set_local(ty, *value)?));
            lab = l.clone();
        }
        Action::SetObjective { quest: q, index, displayed, completed } => {
            let q = quest(g, q)?;
            g.set_objective(q, *index, *displayed, *completed);
        }
        Action::GetQuest { .. } | Action::SampleVar { .. } | Action::SampleRefVar { .. } => unreachable!("handled by the caller"),
        Action::WaitFrames { .. } | Action::AdvanceSeconds { .. } | Action::Note { .. } => {}
        Action::Call { line, label: l } => {
            let c = call_line(g, line)?;
            match c.value {
                Value::Ref(r) => obs.insert("value".into(), json!(FormId(r).to_string())),
                Value::Num(n) => obs.insert("value".into(), json!(n)),
            };
            if let Some(why) = c.stub {
                obs.insert("stub".into(), json!(why));
            }
            lab = l.clone();
        }
        Action::Query { script, label: l, text } => {
            use fnv_script::vm::XVal;
            // A runtime-compiled query runs on no reference [observed: GetSelfAlt gives none
            // in the original's capture queries].
            let ctx = crate::game::RunCtx::default();
            match g.run_query(script, ctx) {
                Ok(XVal::Str(s)) => obs.insert("value".into(), json!(s)),
                Ok(XVal::Ref(r)) if !*text => obs.insert("value".into(), json!(FormId(r).to_string())),
                Ok(v) if *text => obs.insert("value".into(), json!(query_text(&v))),
                Ok(XVal::Num(n)) => obs.insert("value".into(), json!(n)),
                Ok(v) => obs.insert("value".into(), json!(query_text(&v))),
                Err(e) => obs.insert("error".into(), json!(e)),
            };
            lab = Some(l.clone());
        }
        Action::CallUntil { line, max_frames, label: l } => {
            lab = Some(l.clone());
            let mut frames = 0;
            loop {
                if call_line(g, line)?.value.num() != 0.0 {
                    break;
                }
                if frames >= *max_frames {
                    return Err(format!("{line:?} still 0 after {max_frames} frames"));
                }
                frame(g);
                frames += 1;
            }
            obs.insert("frames".into(), json!(frames));
        }
        Action::AddItem { to, item, count } => {
            let (to, item) = (resolve(g, to)?, resolve(g, item)?);
            call(g, 0x1002, Some(to), vec![form_arg(item), num_arg(*count as f64)]);
        }
        Action::RemoveItem { from, item, count } => {
            let (from, item) = (resolve(g, from)?, resolve(g, item)?);
            call(g, 0x1052, Some(from), vec![form_arg(item), num_arg(*count as f64)]);
        }
        Action::RemoveAllItems { from, to } => {
            let from = resolve(g, from)?;
            let args = match to {
                Some(t) => vec![form_arg(resolve(g, t)?)],
                None => vec![],
            };
            call(g, 0x10AD, Some(from), args);
        }
        Action::Enable { target } => {
            let t = resolve(g, target)?;
            call(g, 0x1021, Some(t), vec![]);
        }
        Action::Disable { target } => {
            let t = resolve(g, target)?;
            call(g, 0x1022, Some(t), vec![]);
        }
        Action::SetGlobal { name, value } => {
            let id = resolve(g, name)?;
            g.world.set_global(id, *value).map_err(|e| e.to_string())?;
            g.emit(crate::trace::EventKind::GlobalSet { global: id, value: *value });
        }
        Action::Activate { actor, target, run_on_activate, label: l } => {
            // `target.Activate actor`: the default action, bypassing OnActivate; with
            // the flag, the block instead (how the capture issues it; GECK wiki
            // Activate; layer5-captures.md §8.3 f).
            let actor = match actor {
                Some(a) => resolve(g, a)?,
                None => FormId::PLAYER_REF,
            };
            let t = resolve(g, target)?;
            let since = g.events.len();
            g.activate(actor, t, activation_mode(*run_on_activate))?;
            if l.is_some() {
                obs.insert("opened".into(), json!(opened_since(g, since)));
                lab = l.clone();
            }
        }
        Action::SetRefVar { reference, var, value } => {
            let r = resolve(g, reference)?;
            let (key, script, i) = ref_var(g, r, var)?;
            let i = i.ok_or_else(|| format!("the script of {reference} declares no variable {var:?}"))?;
            g.store(key, Some(script), i, Value::Num(*value), "runner", false);
        }
        Action::ActivateRead { target, actor, run_on_activate, read, label: l } => {
            let actor = match actor {
                Some(a) => resolve(g, a)?,
                None => FormId::PLAYER_REF,
            };
            let t = resolve(g, target)?;
            g.activate(actor, t, activation_mode(*run_on_activate))?;
            let v = match (&read.reference, &read.quest) {
                (Some(r), None) => {
                    let r = resolve(g, r)?;
                    ref_field(g, r, &read.field)?
                }
                (None, Some(q)) => {
                    let q = quest(g, q)?;
                    field(g, q, &read.field)?
                }
                _ => return Err("'read' needs one of 'ref' or 'quest'".into()),
            };
            obs.insert(read.field.clone(), v);
            lab = Some(l.clone());
        }
        Action::AddItemRead { to, item, count, read, label: l } => {
            let (to, it) = (resolve(g, to)?, resolve(g, item)?);
            call(g, 0x1002, Some(to), vec![form_arg(it), num_arg(*count as f64), num_arg(1.0)]);
            for name in read {
                let i = resolve(g, name)?;
                obs.insert(format!("count.{name}"), json!(g.world.item_count(to, i).map_err(|e| e.to_string())?));
            }
            lab = Some(l.clone());
        }
        Action::SampleItems { of, items, frames, label: l } => {
            // One sample per frame, the first in the frame after the previous action.
            let of = resolve(g, of)?;
            let ids = items.iter().map(|n| resolve(g, n)).collect::<Result<Vec<_>, _>>()?;
            let mut series: Vec<Vec<i32>> = vec![Vec::new(); ids.len()];
            for _ in 0..*frames {
                for (k, &i) in ids.iter().enumerate() {
                    series[k].push(g.world.item_count(of, i).map_err(|e| e.to_string())?);
                }
                frame(g);
            }
            for (name, v) in items.iter().zip(&series) {
                let (Some(&first), Some(&last)) = (v.first(), v.last()) else { continue };
                let mut settled = v.len() - 1;
                while settled > 0 && v[settled - 1] == last {
                    settled -= 1;
                }
                obs.insert(format!("first.{name}"), json!(first));
                obs.insert(format!("final.{name}"), json!(last));
                obs.insert(format!("settled.{name}"), json!(settled));
            }
            lab = Some(l.clone());
        }
        Action::MenuGap { via, frames, quest: qn, var, after, label: l } => {
            lab = Some(l.clone());
            let via = resolve(g, via)?;
            let q = quest(g, qn)?;
            let (key, idx) = quest_var(g, q, var)?;
            let since = g.events.len();
            g.activate(FormId::PLAYER_REF, via, ActMode::Default)?;
            if !opened_since(g, since) {
                return Err(format!("activating {} opened no container menu", g.data.name(via)));
            }
            // The menu is open for `frames` frames counting the opening one; the value
            // is read in its last frame, then the menu closes (CloseAllMenus).
            g.menu = Some(CONTAINER_MENU);
            for _ in 1..*frames {
                frame(g);
            }
            let v0 = g.var(key, idx).num();
            frame(g);
            g.menu = None;
            let mut drop = None;
            for _ in 0..*after {
                frame(g);
                let v = g.var(key, idx).num();
                if drop.is_none() && v != v0 {
                    drop = Some(v0 - v);
                }
            }
            match drop {
                Some(d) => {
                    obs.insert("drop".into(), json!(d));
                }
                None => return Err(format!("no change in {after} frames after the menu")),
            }
        }
        Action::EvaluatePackage { of } => {
            let a = resolve(g, of)?;
            call(g, 0x105E, Some(a), vec![]);
        }
        Action::IsCurrentPackage { of, package, label: l } => {
            let (a, p) = (resolve(g, of)?, resolve(g, package)?);
            obs.insert("current".into(), json!(g.current_package(a) == Some(p)));
            lab = Some(l.clone());
        }
        Action::GetItemCount { of, item, label: l } => {
            let (of, item) = (resolve(g, of)?, resolve(g, item)?);
            obs.insert("count".into(), json!(g.world.item_count(of, item).map_err(|e| e.to_string())?));
            lab = l.clone();
        }
        Action::GetDisabled { of, label: l } => {
            let r = resolve(g, of)?;
            obs.insert("disabled".into(), json!(!g.world.is_enabled(r).map_err(|e| e.to_string())?));
            lab = l.clone();
        }
        Action::GetGlobal { name, label: l } => {
            let id = resolve(g, name)?;
            obs.insert("value".into(), json!(g.world.global(id).map_err(|e| e.to_string())? as f64));
            lab = l.clone();
        }
        Action::PlayerActivate { target, actor, label: l } => {
            lab = l.clone();
            let actor = match actor {
                Some(a) => resolve(g, a)?,
                None => FormId::PLAYER_REF,
            };
            let t = resolve(g, target)?;
            let from = g.events.len();
            g.activate(actor, t, ActMode::Hand)?;
            // The player's activation whose default action is to talk (an actor, or a
            // talking activator whose OnActivate ends with `Activate`) opens the
            // conversation, as the play loop's does (nvf-play `talk_after`); an actor
            // with nothing to say opens none. Observation `info`: the greeting said.
            if actor == FormId::PLAYER_REF && g.dialogue.is_none() {
                if let Some(who) = talk_since(g, from) {
                    if let Ok(info) = g.start_dialogue(who, None) {
                        obs.insert("info".into(), json!(info.to_string()));
                    }
                }
            }
            // A container the player's activation opened: its menu opens and closes with
            // nothing taken (`remove_all_items` stands in for taking), so its OnOpen and
            // then OnClose run at the start of the next frame, as the play loop queues
            // them from the container menu (nvf-play `engine_menu_opened` / `_closed`)
            // [hypothesis: the menu's moments, game.rs `event::ON_OPEN`]. The Fort's
            // player storage turns the Legion hostile this way (VMQ02 110).
            for c in containers_opened_since(g, from) {
                g.queue_ref_event(c, crate::game::event::ON_OPEN, Some(FormId::PLAYER_REF));
                g.queue_ref_event(c, crate::game::event::ON_CLOSE, Some(FormId::PLAYER_REF));
                obs.insert("opened".into(), json!(c.to_string()));
            }
        }
        Action::PlayerSneak { sneaking } => {
            let m = g.senses.motion.get(&FormId::PLAYER_REF).copied().unwrap_or_default();
            g.set_motion(FormId::PLAYER_REF, crate::functions::senses::Motion { sneaking: *sneaking, moving: false, running: false, ..m });
        }
        Action::PerkChoice { target, choice, label: l } => {
            lab = l.clone();
            let t = resolve(g, target)?;
            if !g.open_activate_menu(t) {
                return Err(format!("activating {} offers no perk choices", g.data.name(t)));
            }
            let labels: Vec<String> = g.activate_menu.iter().flat_map(|m| m.choices.iter().map(|c| c.label.clone())).collect();
            obs.insert("choices".into(), json!(labels));
            let Some(i) = labels.iter().position(|c| c.eq_ignore_ascii_case(choice)) else {
                g.pick_activate_choice(labels.len());
                return Err(format!("{choice:?} is not offered on {} (offered: {labels:?})", g.data.name(t)));
            };
            g.pick_activate_choice(i);
        }
        Action::SayInfo { info, speaker, force } => {
            let i = resolve(g, info)?;
            let s = match speaker {
                Some(s) => Some(resolve(g, s)?),
                None => None,
            };
            g.run_info(i, s, *force)?;
        }
        Action::SayFirst { infos, speaker, label: l } => {
            let s = match speaker {
                Some(s) => Some(resolve(g, s)?),
                None => None,
            };
            let mut said = Json::Null;
            let mut why = Vec::new();
            for name in infos {
                let i = resolve(g, name)?;
                let dv = g.dialogue_view();
                let def = dv.info(&i).ok_or_else(|| format!("{name} is not a dialogue line"))?;
                let sp = s.or(def.speaker);
                let pass = g.conditions_pass(&def.conditions, sp, Some(FormId::PLAYER_REF), &format!("INFO {i}"));
                if pass {
                    g.run_info(i, s, true)?;
                    said = json!(i.to_string());
                    break;
                }
                why.push(format!("{i}: {}", g.explain_conditions(&def.conditions, sp, Some(FormId::PLAYER_REF)).join("; ")));
            }
            if said.is_null() {
                return Err(format!("no line's conditions pass ({})", why.join(" | ")));
            }
            obs.insert("said".into(), said);
            lab = l.clone();
        }
        Action::Package { actor, package, event: ev } => {
            let (a, p) = (resolve(g, actor)?, resolve(g, package)?);
            let which = match ev.as_str() {
                "start" => 0,
                "done" | "end" => 1,
                "change" => 2,
                _ => return Err(format!("package event {ev:?} is not start, done or change")),
            };
            // "done" on the actor's current package is its goal met, as the AI layer
            // reports it (`Game::package_goal_met`): a script package is then dropped at
            // the next choice, and a Must Complete one may be replaced (ai.rs).
            let current = g.extra.get(&a).is_some_and(|e| e.current_package == Some(p) && !e.package_goal_met);
            if which == 1 && current {
                g.package_goal_met(a);
            }
            // With the AI running packages, `package_goal_met` ran the end event already.
            if !(which == 1 && current && g.runs_packages()) {
                g.package_event(a, p, which);
            }
        }
        Action::DialoguePackage { actor, package, label: l } => {
            use crate::package::{PackageType, TargetKind};
            lab = l.clone();
            let a = resolve(g, actor)?;
            let p = match package {
                Some(p) => resolve(g, p)?,
                None => g.current_package(a).ok_or_else(|| format!("{} has no current package", g.data.name(a)))?,
            };
            let def = g.package_def(&p).ok_or_else(|| format!("{} is not a package", g.data.name(p)))?;
            if def.ty != PackageType::Dialogue {
                return Err(format!("{} is not a Dialogue package", g.data.name(p)));
            }
            let talk = def.dialogue.unwrap_or_default();
            let target = match def.target.map(|t| t.kind) {
                Some(TargetKind::Reference(Some(r))) => r,
                Some(TargetKind::LinkedReference) => {
                    g.world.placed.get(a).and_then(|r| r.linked).ok_or_else(|| format!("{} has no linked reference to talk to", g.data.name(a)))?
                }
                _ => return Err(format!("{} names no reference to talk to", g.data.name(p))),
            };
            let line = if talk.say_to {
                let topic = talk.topic.ok_or_else(|| format!("{} is Say To without a topic", g.data.name(p)))?;
                g.say(a, Some(target), topic, true).ok_or_else(|| format!("{} has no line of {} to say", g.data.name(a), g.data.name(topic)))?
            } else if target == FormId::PLAYER_REF {
                // A conversation also for a greeting with the Goodbye flag (`Game::force_greet`).
                g.force_greet(a, talk.topic)?
            } else {
                return Err("a conversation between two actors is not carried out (fnv-ai goal::talk)".into());
            };
            g.package_goal_met(a);
            obs.insert("info".into(), json!(line.to_string()));
        }
        Action::SayToDone { actor, topic } => {
            let a = resolve(g, actor)?;
            let t = match topic {
                Some(t) => Some(Value::Ref(resolve(g, t)?.0)),
                None => None,
            };
            g.ref_event(a, event::SAY_TO_DONE, t, None);
        }
        Action::PatrolPoint { actor, point } => {
            let (a, p) = (resolve(g, actor)?, resolve(g, point)?);
            let has_topic = g.world.placed.get(p).and_then(|r| r.patrol_topic).is_some();
            if !g.data.patrol_scripts.contains_key(&p) && !has_topic {
                return Err(format!("{} has no patrol script or topic", g.data.name(p)));
            }
            g.patrol_point_reached(a, p);
        }
        Action::Fire { weapon, actor } => {
            let (a, w) = (actor_or_player(g, actor)?, resolve(g, weapon)?);
            if g.world.item_count(a, w).unwrap_or(0) <= 0 {
                return Err(format!("{} carries no {}", g.data.name(a), g.data.name(w)));
            }
            if !g.weapon_fired(a, w) {
                return Err(format!("{} has no OnFire block", g.data.name(w)));
            }
        }
        Action::TriggerEnter { trigger, actor, event: ev, frames } => {
            let t = resolve(g, trigger)?;
            let a = match actor {
                Some(a) => resolve(g, a)?,
                None => FormId::PLAYER_REF,
            };
            let code = match ev.as_deref() {
                None | Some("enter") => event::ON_TRIGGER_ENTER,
                Some("inside") => event::ON_TRIGGER,
                Some("leave") => event::ON_TRIGGER_LEAVE,
                Some(o) => return Err(format!("trigger event {o:?} is not enter, inside or leave")),
            };
            let n = match frames {
                Some(n) if code != event::ON_TRIGGER => return Err(format!("frames ({n}) is for the event \"inside\" only")),
                Some(n) => (*n).max(1),
                None => 1,
            };
            for k in 0..n {
                if k > 0 {
                    frame(g);
                }
                if code == event::ON_TRIGGER {
                    g.trigger_frame(t);
                }
                if !g.ref_event(t, code, Some(Value::Ref(a.0)), Some(a)) {
                    // A block that stops answering (the script disabled the trigger,
                    // moved the actor) ends the stay; only the first frame must answer.
                    if k == 0 {
                        return Err(format!("no block of {} accepted the trigger event", g.data.name(t)));
                    }
                    break;
                }
            }
        }
        Action::PressButton { button } => {
            // As the play loop answers a box (nvf-play script.rs `answer`/`finish_answer`):
            // the button is set, one frame runs with the box still open, so MenuMode 1001
            // blocks read it with GetButtonPressed, then the box closes.
            let m = g.message_box.as_mut().ok_or("no message box is open")?;
            m.can_press(*button)?;
            m.pressed = Some(*button);
            let message = m.message;
            // A box the input left modeless (no `given` menu) is shown without a menu: the
            // press frame runs with menu 1001 open all the same, as on screen.
            let modeless = g.menu.is_none();
            if modeless {
                g.open_menu(menus::MESSAGE);
            }
            frame(g);
            if g.menu == Some(menus::MESSAGE) && (modeless || g.boxes.first() == Some(&message)) {
                g.close_menu();
            }
        }
        Action::Menu { menu } => match menu {
            Some(m) => g.open_menu(*m),
            None => {
                g.close_menu();
            }
        },
        // ---- round 4 (layer5-captures.md §10.4) ----
        Action::StartStopwatch {} => st.stopwatch = Some(g.time),
        Action::CloseMenus {} => {
            g.close_all_menus();
        }
        Action::DismissMessageBoxes { max, wait_frames, label: l } => {
            // As the capture does it: wait for a box (checked before each frame), note
            // whether one is open, close menus, then three frames.
            let mut closed = 0;
            for _ in 0..*max {
                for k in 0..=*wait_frames {
                    if g.menu == Some(menus::MESSAGE) {
                        break;
                    }
                    if k < *wait_frames {
                        frame(g);
                    }
                }
                if g.menu == Some(menus::MESSAGE) {
                    closed += 1;
                }
                g.close_menu();
                g.frames(3);
            }
            if l.is_some() {
                obs.insert("closed".into(), json!(closed));
                lab = l.clone();
            }
        }
        Action::WaitQuest { quest: qn, field: f, value, max_frames, label: l, stopwatch } => {
            lab = Some(l.clone());
            let q = quest(g, qn)?;
            let holds = |g: &Game| field(g, q, f).map(|v| same_value(&v, value));
            wait_until(g, *max_frames, |g| holds(g))?;
            frame(g);
            let found = holds(g)?;
            waited(g, st, &mut obs, found, *stopwatch)?;
        }
        Action::WaitMenu { menu, max_frames, label: l, stopwatch } => {
            lab = Some(l.clone());
            let holds = |g: &Game| Ok(g.menu_open(*menu));
            wait_until(g, *max_frames, holds)?;
            frame(g);
            let found = g.menu_open(*menu);
            waited(g, st, &mut obs, found, *stopwatch)?;
        }
        Action::GetMenus { menus: codes, label: l } => {
            for c in codes {
                obs.insert(format!("menu.{c}"), json!(g.menu_open(*c)));
                frame(g);
            }
            lab = Some(l.clone());
        }
        Action::GetTagSkills { skills, label: l } => {
            lab = Some(l.clone());
            for name in skills {
                let code = av_code(g, name)?;
                let v = call(g, 0x11CE, None, vec![ArgValue::Enum(code)]).num();
                obs.insert(format!("tag.{name}"), json!(v != 0.0));
                frame(g);
            }
        }
        Action::MovePlayer { to } => {
            let t = resolve(g, to)?;
            call(g, 0x109E, Some(FormId::PLAYER_REF), vec![form_arg(t)]);
        }
        Action::LoadCell { cell } => {
            let c = resolve(g, cell)?;
            let mut cells = g.loaded_cells.clone();
            cells.extend(g.cells_around(c));
            g.set_loaded(cells);
        }
        Action::GetVar { of, var, label: l } => {
            let f = resolve(g, of)?;
            let (key, _, i) = var_of(g, f, var)?;
            obs.insert("value".into(), json!(g.var(key, i).num()));
            lab = l.clone();
        }
        Action::GetRef { fields: Some(fields), of, reference, label: l } => {
            // One frame per field, as get_quest.
            let name = reference.as_ref().or(of.as_ref()).ok_or("get_ref needs 'ref'")?;
            let r = resolve(g, name)?;
            let mut errors = Map::new();
            for f in fields {
                match ref_field(g, r, f) {
                    Ok(v) => {
                        obs.insert(f.clone(), v);
                    }
                    Err(e) => {
                        errors.insert(f.clone(), json!(e));
                    }
                }
                frame(g);
            }
            if !errors.is_empty() {
                obs.insert("errors".into(), Json::Object(errors));
            }
            lab = l.clone();
        }
        Action::GetRef { fields: None, of, reference, label: l } => {
            let name = reference.as_ref().or(of.as_ref()).ok_or("get_ref needs 'of'")?;
            let r = resolve(g, name)?;
            let enabled = g.world.is_enabled(r).map_err(|e| e.to_string())?;
            let s = g.world.get(r).map_err(|e| e.to_string())?.clone();
            obs.insert("disabled".into(), json!(!enabled));
            obs.insert("cell".into(), json!(s.cell));
            obs.insert("pos".into(), json!(s.pos));
            let x = g.extra.get(&r).cloned().unwrap_or_default();
            obs.insert("dead".into(), json!(x.dead));
            obs.insert("destroyed".into(), json!(x.destroyed));
            lab = l.clone();
        }
        Action::Hit { target, attacker, weapon, damage, label: l } => {
            let t = resolve(g, target)?;
            let attacker = match attacker {
                Some(x) => resolve(g, x)?,
                None => FormId::PLAYER_REF,
            };
            let weapon = match weapon {
                Some(w) => Some(resolve(g, w)?),
                None => None,
            };
            let info = crate::combat::HitInfo { attacker: Some(attacker), weapon, location: -1, critical: false, limb: None };
            let actor = matches!(crate::functions::base_of(g, t).and_then(|b| g.data.sig(b)).as_ref(), Some(b"NPC_") | Some(b"CREA"));
            if actor {
                let r = g.hit(t, damage.unwrap_or(0.0), info);
                if !r.died && !g.is_dead(t) {
                    provoked(g, t, attacker);
                }
                obs.insert("health".into(), json!(r.health));
                obs.insert("died".into(), json!(r.died));
            } else {
                obs.insert("ran".into(), json!(g.hit_object(t, info)));
            }
            lab = l.clone();
        }
        Action::Kill { target, actor, killer } => {
            let a = resolve(g, target.as_ref().or(actor.as_ref()).ok_or("kill needs 'target'")?)?;
            let args = match killer {
                Some(k) => vec![form_arg(resolve(g, k)?)],
                None => vec![],
            };
            call(g, 0x108B, Some(a), args);
        }
        // ---- round 3 (layer5-captures.md §9.2) ----
        Action::SetReputation { rep: name, axis, value } => {
            let r = rep(g, name)?;
            call(g, 0x1238, None, vec![form_arg(r), num_arg(rep_axis(axis)?), num_arg(*value)]);
        }
        Action::AddReputation { rep: name, axis, size } | Action::RemoveReputation { rep: name, axis, size } => {
            let r = rep(g, name)?;
            let opcode = if matches!(a, Action::AddReputation { .. }) { 0x1239 } else { 0x123A };
            call(g, opcode, None, vec![form_arg(r), num_arg(rep_axis(axis)?), num_arg(*size as f64)]);
        }
        Action::GetReputation { rep: name, fields, label: l } => {
            // One frame per field, as get_quest.
            let r = rep(g, name)?;
            rep_fields(g, r, fields, &mut obs, true);
            lab = l.clone();
        }
        Action::SetReputationRead { rep: name, fame, infamy, fields, label: l } => {
            let r = rep(g, name)?;
            call(g, 0x1238, None, vec![form_arg(r), num_arg(1.0), num_arg(*fame)]);
            call(g, 0x1238, None, vec![form_arg(r), num_arg(0.0), num_arg(*infamy)]);
            rep_fields(g, r, fields, &mut obs, false);
            lab = l.clone();
        }
        Action::UseItem { actor, item, read, label: l, stopwatch } => {
            let who = actor_or_player(g, actor)?;
            let it = resolve(g, item)?;
            if g.world.item_count(who, it).map_err(|e| e.to_string())? < 1 {
                return Err(format!("{} carries no {}", g.data.name(who), g.data.name(it)));
            }
            // `EquipItem item 0 1`: an ingestible is consumed and its effects start
            // inside the call (Game::apply_magic_item).
            call(g, 0x10EE, Some(who), vec![form_arg(it), num_arg(0.0), num_arg(1.0)]);
            for r in read {
                match (&r.av, &r.item) {
                    (Some(av), None) => {
                        let code = av_code(g, av)?;
                        obs.insert(format!("av.{av}"), json!(actor_value(g, who, code)?));
                    }
                    (None, Some(i)) => {
                        let id = resolve(g, i)?;
                        obs.insert(format!("count.{i}"), json!(g.world.item_count(who, id).map_err(|e| e.to_string())?));
                    }
                    _ => return Err("a use_item read is {\"av\"} or {\"item\"}".into()),
                }
            }
            if *stopwatch {
                st.stopwatch = Some(g.time);
            }
            lab = l.clone();
        }
        Action::WaitAv { of, av, value, max_frames, label: l } => {
            lab = l.clone();
            let who = actor_or_player(g, of)?;
            let code = av_code(g, av)?;
            let mut found = false;
            for k in 0..=*max_frames {
                if (actor_value(g, who, code)? - value).abs() < 1e-6 {
                    found = true;
                    break;
                }
                if k < *max_frames {
                    frame(g);
                }
            }
            obs.insert("found".into(), json!(found));
            if found {
                let t0 = st.stopwatch.ok_or("wait_av before any use_item with a stopwatch")?;
                obs.insert("seconds".into(), json!(g.time - t0));
            }
        }
        Action::HoldMenu { via, frames } => {
            let via = resolve(g, via)?;
            let since = g.events.len();
            g.activate(FormId::PLAYER_REF, via, ActMode::Default)?;
            if !opened_since(g, since) {
                return Err(format!("activating {} opened no container menu", g.data.name(via)));
            }
            g.menu = Some(CONTAINER_MENU);
            g.frames(*frames);
            g.menu = None;
        }
        Action::DamageAv { of, av, value } | Action::RestoreAv { of, av, value } => {
            let who = actor_or_player(g, of)?;
            let code = av_code(g, av)?;
            let opcode = if matches!(a, Action::DamageAv { .. }) { 0x1181 } else { 0x1182 };
            call(g, opcode, Some(who), vec![num_arg(code as f64), num_arg(*value)]);
        }
        Action::GetAv { of, name, label: l } => {
            let who = actor_or_player(g, of)?;
            let code = av_code(g, name)?;
            obs.insert("value".into(), json!(actor_value(g, who, code)?));
            lab = l.clone();
        }
        Action::SetAv { of, name, value } => {
            let who = actor_or_player(g, of)?;
            let code = av_code(g, name)?;
            call(g, 0x100F, Some(who), vec![num_arg(code as f64), num_arg(*value)]);
        }
        Action::AddSpell { of, spell } | Action::RemoveSpell { of, spell } => {
            let who = actor_or_player(g, of)?;
            let s = resolve(g, spell)?;
            if !g.data.effect_items.contains_key(&s) {
                return Err(format!("{spell} is not a spell"));
            }
            let opcode = if matches!(a, Action::AddSpell { .. }) { 0x101C } else { 0x101D };
            call(g, opcode, Some(who), vec![form_arg(s)]);
        }
        Action::SampleAv { of, av, frames, label: l } => {
            // One sample per frame, read before the frame runs (as sample_items).
            lab = l.clone();
            let who = actor_or_player(g, of)?;
            let code = av_code(g, av)?;
            let mut vals = Vec::new();
            let mut times = Vec::new();
            for _ in 0..*frames {
                vals.push(actor_value(g, who, code)?);
                times.push(g.time);
                frame(g);
            }
            let changes = vals.windows(2).filter(|w| w[0] != w[1]).count();
            obs.insert("changes".into(), json!(changes));
            if let (Some(&v0), Some(&v1), Some(&t0), Some(&t1)) = (vals.first(), vals.last(), times.first(), times.last()) {
                if t1 > t0 {
                    obs.insert("per_second".into(), json!((v1 - v0) / (t1 - t0)));
                }
            }
        }
        Action::SampleAvDamage { of, av, damage, frames, label: l } => {
            lab = l.clone();
            let who = actor_or_player(g, of)?;
            let code = av_code(g, av)?;
            let mut vals = Vec::new();
            for _ in 0..*frames {
                vals.push(actor_value(g, who, code)?);
                call(g, 0x1181, Some(who), vec![num_arg(code as f64), num_arg(*damage)]);
                frame(g);
            }
            obs.insert("values".into(), json!(vals));
        }
        Action::GoToCell { cell } => {
            let c = resolve(g, cell)?;
            if !g.world.placed.by_cell.contains_key(&c) && g.world.placed.world_of(c).is_none() {
                return Err(format!("{cell:?} is not a cell"));
            }
            let pos = coc_destination(g, c)?;
            let from = g.world.peek(FormId::PLAYER_REF).map(|p| (p.cell, p.pos));
            {
                let p = g.world.get(FormId::PLAYER_REF).map_err(|e| e.to_string())?;
                p.cell = Some(c);
                p.pos = pos;
            }
            g.emit(EventKind::Moved { reference: FormId::PLAYER_REF, cell: Some(c), pos });
            g.player_entered(Some(c));
            g.settle_start_weather();
            // "Changing cells will teleport the teammate to the player" [documented: GECK
            // wiki SetPlayerTeammate notes], as MoveTo and doors do (`Game::bring_teammates`).
            g.bring_teammates(from);
        }
        Action::SampleJumps { reads, frames, min_step, label: l } => {
            // Every read once per frame, after the frame's scripts ran.
            lab = l.clone();
            if reads.len() < 2 {
                return Err("sample_jumps needs at least two reads".into());
            }
            let mut series: Vec<Vec<f64>> = vec![Vec::new(); reads.len()];
            for _ in 0..*frames {
                frame(g);
                for (k, r) in reads.iter().enumerate() {
                    series[k].push(jump_value(g, r)?);
                }
            }
            let jump = |s: &[f64]| (1..s.len()).find(|&i| (s[i] - s[i - 1]).abs() >= *min_step);
            let first = jump(&series[0]);
            let lags: Vec<Json> = series[1..]
                .iter()
                .map(|s| match (first, jump(s)) {
                    (Some(a), Some(b)) => json!(b as i64 - a as i64),
                    _ => Json::Null,
                })
                .collect();
            obs.insert("lags".into(), json!(lags));
        }
        Action::Talk { actor, topic, label: l } => {
            let a = resolve(g, actor)?;
            let t = match topic {
                Some(t) => Some(resolve(g, t)?),
                None => None,
            };
            let info = g.start_dialogue(a, t)?;
            obs.insert("info".into(), json!(info.to_string()));
            lab = l.clone();
        }
        Action::Choose { topic, prompt, index, max_frames, label: l } => {
            wait_talk(g, *max_frames)?;
            let i = pick_choice(g, topic, prompt, index)?;
            let info = g.choose(i)?;
            obs.insert("info".into(), json!(info.to_string()));
            lab = l.clone();
        }
        Action::WaitTalk { max_frames, start, label: l } => {
            if *start {
                if !wait_until(g, *max_frames, |g| Ok(g.dialogue.is_some()))? {
                    return Err(format!("no conversation started in {max_frames} frames"));
                }
                obs.insert("actor".into(), json!(g.dialogue_actor().map(|a| a.to_string())));
            }
            wait_talk(g, *max_frames)?;
            obs.insert("state".into(), json!(if g.dialogue.is_some() { "choosing" } else { "ended" }));
            obs.insert("choices".into(), json!(g.dialogue_choices().len()));
            lab = l.clone();
        }
        Action::EndTalk {} => {
            // A conversation that already ended (its last line, a script) leaves
            // nothing to do (issue #1114).
            g.end_dialogue();
        }
        Action::Terminal { target, item, menu, label: l } => {
            let r = resolve(g, target)?;
            let base = match menu {
                Some(m) => resolve(g, m)?,
                None => g.world.placed.refs.get(&r).map(|p| p.base).ok_or_else(|| format!("{target} is not a placed reference"))?,
            };
            let listed = crate::terminal::items(g, r, base);
            if !listed.contains(item) {
                return Err(format!("entry {item} of terminal {base} is not listed now (listed: {listed:?})"));
            }
            let c = crate::terminal::choose(g, r, base, *item).ok_or_else(|| format!("terminal {base} has no entry {item}"))?;
            obs.insert("submenu".into(), json!(c.submenu.map(|f| f.to_string())));
            obs.insert("note".into(), json!(c.note.map(|f| f.to_string())));
            obs.insert("back".into(), json!(c.back));
            lab = l.clone();
        }
        Action::SaveLoad { label: l } => {
            let bytes = save_and_load(g)?;
            obs.insert("bytes".into(), json!(bytes));
            lab = l.clone();
        }
        Action::Consume { item, actor, label: l } => {
            let who = match actor {
                Some(a) => resolve(g, a)?,
                None => FormId::PLAYER_REF,
            };
            let it = resolve(g, item)?;
            let ids = g.consume(who, it)?;
            obs.insert("effects".into(), json!(ids.len()));
            lab = l.clone();
        }
        Action::PlayNote { note, label: l } => {
            let n = resolve(g, note)?;
            let info = g.holotape_script(n, 0);
            g.holotape_script(n, 1);
            obs.insert("info".into(), info.map_or(Json::Null, |i| json!(i.to_string())));
            lab = l.clone();
        }
    }
    Ok(lab.map(|l| (l, obs)))
}

fn quests_json(g: &Game) -> Json {
    let mut out = Map::new();
    for (q, s) in &g.quests {
        let script = g.data.quests.get(q).and_then(|d| d.script);
        let vars: Map<String, Json> = g
            .instances
            .get(&InstKey::Quest { quest: *q })
            .map(|i| {
                i.vars
                    .iter()
                    .map(|(k, v)| (g.var_name(script, *k).unwrap_or_else(|| format!("#{k}")), json!(v.num())))
                    .collect()
            })
            .unwrap_or_default();
        let objectives: Map<String, Json> =
            s.objectives.iter().map(|(k, o)| (k.to_string(), json!({"displayed": o.displayed, "completed": o.completed}))).collect();
        out.insert(
            q.to_string(),
            json!({"edid": g.data.edid_of.get(q), "running": s.running, "completed": s.completed, "failed": s.failed,
                   "stage": s.stage(), "stages_done": s.stages_done, "objectives": objectives, "vars": vars}),
        );
    }
    Json::Object(out)
}

/// Where a new game puts the player: the marker the opening moves the player to
/// (VCG00 stage 0, `player.moveto VCG01PlayerStartMarkerREF` [observed in the masters]),
/// in Doc Mitchell's house, where the new-game save `nvfnew` has the player [observed:
/// layer5-captures.md §10.2]. Without a place the player is in no cell, and the add-ons'
/// start quests (which wait for the player to leave Doc Mitchell's house) would act at
/// once. Inputs that need the player elsewhere move it (`go_to_cell`, `move_player`).
pub const NEW_GAME_START: &str = "VCG01PlayerStartMarkerREF";

fn new_game_start(g: &mut Game) {
    if let Some(m) = g.world.placed.by_edid(NEW_GAME_START) {
        call(g, 0x109E, Some(FormId::PLAYER_REF), vec![form_arg(m)]);
    }
}

/// Run an input document against a fresh world and game state.
pub fn run(data: &GameData, forms: &FormStore, refs: &RefTable, input: &Input) -> Result<Json, String> {
    run_with_voice(data, forms, refs, input, None)
}

/// [`run`], with dialogue responses as long as their voice files where `voice` knows
/// them (`crate::voice`, docs/spec-notes/voice.md §8).
pub fn run_with_voice(data: &GameData, forms: &FormStore, refs: &RefTable, input: &Input, voice: Option<Voice>) -> Result<Json, String> {
    run_as(data, forms, refs, input, ACTIONS_FORMAT, voice, false).map(|(t, _)| t)
}

/// [`run_with_voice`], recording what the run reached of the scripts
/// (`crate::coverage`): the trace and the coverage.
pub fn run_with_coverage(
    data: &GameData,
    forms: &FormStore,
    refs: &RefTable,
    input: &Input,
    voice: Option<Voice>,
) -> Result<(Json, crate::coverage::Coverage), String> {
    run_as(data, forms, refs, input, ACTIONS_FORMAT, voice, true).map(|(t, c)| (t, c.unwrap_or_default()))
}

/// The line-length hook a run is given ([`crate::game::Game::voice`]).
pub type Voice = std::sync::Arc<dyn crate::voice::VoiceLengths>;

/// Trace kinds a dialogue trace keeps (docs/spec-notes/dialogue.md §8).
const DIALOGUE_KINDS: &[&str] = &[
    "dialogue_started",
    "line_started",
    "response_said",
    "line_ended",
    "result_script",
    "choices_offered",
    "choice_made",
    "topic_added",
    "dialogue_ended",
    "no_line",
    "stage_set",
    "menu_opened",
    "menu_closed",
];

/// `fnvtool dialogue talk`: run a `fnv.dialogue.actions/1` document (the fields and
/// operations of `fnv.script.actions/1`, the conversation operations among them) and
/// reduce the run to a `fnv.dialogue.trace/1`: the conversation's events in order,
/// with Editor IDs and text lengths. With `text`, the texts themselves are included
/// (for reading at home; a trace with texts holds game data and is never committed).
pub fn run_dialogue(data: &GameData, forms: &FormStore, refs: &RefTable, input: &Input, text: bool) -> Result<Json, String> {
    run_dialogue_with_voice(data, forms, refs, input, text, None)
}

/// [`run_dialogue`], with responses as long as their voice files where `voice` knows
/// them.
pub fn run_dialogue_with_voice(
    data: &GameData,
    forms: &FormStore,
    refs: &RefTable,
    input: &Input,
    text: bool,
    voice: Option<Voice>,
) -> Result<Json, String> {
    let (full, _) = run_as(data, forms, refs, input, DIALOGUE_ACTIONS_FORMAT, voice, false)?;
    let form = |v: &Json| v.as_str().and_then(|s| s.parse::<FormId>().ok());
    let edid = |v: &Json| form(v).and_then(|f| data.edid_of.get(&f).cloned());
    let mut events = Vec::new();
    for e in full["events"].as_array().into_iter().flatten() {
        let kind = e["kind"].as_str().unwrap_or_default();
        if !DIALOGUE_KINDS.contains(&kind) {
            continue;
        }
        let mut e = e.clone();
        if let Some(n) = edid(&e["topic"]) {
            e["topic_edid"] = json!(n);
        }
        let info = form(&e["info"]).and_then(|i| data.infos.get(&i));
        match kind {
            "line_started" => {
                if let Some(d) = info {
                    e["chars"] = json!(d.response_chars.iter().sum::<usize>());
                    e["responses"] = json!(d.responses.len());
                    e["flags"] = json!(crate::data::info_flag::NAMES.iter().filter(|(b, _)| d.has(*b)).map(|(_, n)| *n).collect::<Vec<_>>());
                }
            }
            "response_said" if text => {
                let number = e["number"].as_u64();
                if let Some(r) = info.and_then(|d| d.responses.iter().find(|r| Some(r.number as u64) == number)) {
                    e["text"] = json!(r.text);
                }
            }
            "choices_offered" => {
                for c in e["choices"].as_array_mut().into_iter().flatten() {
                    if let Some(n) = edid(&c["topic"]) {
                        c["topic_edid"] = json!(n);
                    }
                    if let (true, Some(t), Some(i)) = (text, form(&c["topic"]), form(&c["info"])) {
                        c["text"] = json!(data.choice_text(t, i));
                    }
                }
            }
            _ => {}
        }
        events.push(e);
    }
    Ok(json!({
        "format": DIALOGUE_TRACE_FORMAT,
        "seed": full["seed"],
        "frame_seconds": full["frame_seconds"],
        "title": full["title"],
        "observations": full["observations"],
        "steps": full["steps"],
        "given_errors": full["given_errors"],
        "events": events,
        "final": {
            "frames": full["final"]["frames"],
            "dialogue": full["final"]["dialogue"],
            "topics": full["final"]["player"]["topics"],
            "quests": full["final"]["quests"],
        },
    }))
}

fn run_as(
    data: &GameData,
    forms: &FormStore,
    refs: &RefTable,
    input: &Input,
    format: &str,
    voice: Option<Voice>,
    cover: bool,
) -> Result<(Json, Option<crate::coverage::Coverage>), String> {
    if input.format != format {
        return Err(format!("unsupported actions format {:?} (expected {format})", input.format));
    }
    if input.frame_seconds.is_nan() || input.frame_seconds <= 0.0 {
        return Err("frame_seconds must be positive".into());
    }
    let world = World::new(
        forms,
        refs,
        SimConfig { seed: input.seed, player_level: input.player_level, hardcore: input.hardcore, ..SimConfig::default() },
    );
    // Message boxes hold GameMode (round 4) when the input states the menus of its save
    // (`given` kind `menu`, layer5-captures.md §10.4); inputs of rounds 1–3 do not, and
    // their saves had long closed the boxes a new game shows (the pack plugins' and the
    // add-ons' "loaded" messages), so there message boxes are only traced
    // (docs/spec-notes/scripts.md §8; owner review).
    let modal = input.given.iter().any(|g| matches!(g, Given::Menu { .. }));
    let cfg = GameConfig {
        frame_seconds: input.frame_seconds,
        trace_locals: input.trace_locals,
        seed: input.seed,
        modal_message_boxes: modal,
        ..GameConfig::default()
    };
    let mut g = Game::new(data, world, cfg);
    g.voice = voice;
    crate::ui_loading::load_headless(&mut g);
    if cover {
        g.coverage = Some(Default::default());
    }
    if input.new_game {
        g.new_game();
        new_game_start(&mut g);
    }
    let mut given_errors = Vec::new();
    for (i, gv) in input.given.iter().enumerate() {
        if let Err(e) = apply_given(&mut g, gv) {
            given_errors.push(json!({"given": i, "error": e}));
        }
    }
    let mut observations = Vec::new();
    let mut steps = Vec::new();
    let mut st = RunState::default();
    for (i, a) in input.actions.iter().enumerate() {
        let start_frame = g.frame;
        let events_from = g.events.len();
        let res = match a {
            Action::GetQuest { quest: q, fields, label: l } => match quest(&g, q) {
                Ok(q) => {
                    let mut o = json!({"label": label(l, i), "step": i});
                    let mut errors = Map::new();
                    for f in fields {
                        match field(&g, q, f) {
                            Ok(v) => o[f] = v,
                            Err(e) => {
                                errors.insert(f.clone(), json!(e));
                            }
                        }
                        frame(&mut g);
                    }
                    if !errors.is_empty() {
                        o["errors"] = Json::Object(errors);
                    }
                    observations.push(o);
                    Ok(())
                }
                Err(e) => Err(e),
            },
            Action::SampleVar { quest: q, var, frames, runs, skip, values, label: l } => match quest(&g, q).and_then(|q| quest_var(&g, q, var)) {
                Ok((key, idx)) => {
                    let start = g.var(key, idx).num();
                    let mut samples = Vec::with_capacity(*frames as usize);
                    for _ in 0..*frames {
                        frame(&mut g);
                        samples.push(g.var(key, idx).num());
                    }
                    let mut o = json!({"label": label(l, i), "step": i});
                    sample_observation(&mut o, start, &samples, *runs, *skip, *values);
                    observations.push(o);
                    Ok(())
                }
                Err(e) => Err(e),
            },
            Action::SampleRefVar { reference, var, frames, runs, skip, values, label: l } => {
                match resolve(&g, reference).and_then(|r| ref_var(&g, r, var)) {
                    Ok((key, _, Some(idx))) => {
                        let start = g.var(key, idx).num();
                        let mut samples = Vec::with_capacity(*frames as usize);
                        for _ in 0..*frames {
                            frame(&mut g);
                            samples.push(g.var(key, idx).num());
                        }
                        let mut o = json!({"label": label(l, i), "step": i});
                        sample_observation(&mut o, start, &samples, *runs, *skip, *values);
                        observations.push(o);
                        Ok(())
                    }
                    Ok((_, _, None)) => Err(format!("the script of {reference} declares no variable {var:?}")),
                    Err(e) => Err(e),
                }
            }
            _ => match step(&mut g, &mut st, a) {
                Ok(Some((l, fields))) => {
                    let mut o = json!({"label": l, "step": i});
                    for (k, v) in fields {
                        o[k] = v;
                    }
                    observations.push(o);
                    Ok(())
                }
                Ok(None) => Ok(()),
                Err(e) => {
                    // A labelled action that fails still answers, with its error.
                    if let Some(l) = action_label(a) {
                        observations.push(json!({"label": l, "step": i, "errors": {"action": e.clone()}}));
                    }
                    Err(e)
                }
            },
        };
        let wait = match a {
            Action::WaitFrames { frames } => *frames,
            Action::AdvanceSeconds { seconds } => (seconds / input.frame_seconds).round().max(0.0) as u64,
            _ => frames_after(a),
        };
        // A script's activation by the player whose default is to talk opens the
        // conversation, in the frame it happened, as the play loop's `talk_after` does
        // (`scripted_talk`). The activation ops answer for their own activation, but not
        // for one a script makes in the frames after it: the Sink's appliances
        // (OldWorldBlues.esm) talk from GameMode once their OnActivate has run (#1111).
        let opens = !matches!(a, Action::PlayerActivate { .. } | Action::Activate { .. } | Action::ActivateRead { .. });
        if opens {
            scripted_talk(&mut g, events_from);
        }
        for _ in 0..wait {
            let from = g.events.len();
            frame(&mut g);
            scripted_talk(&mut g, from);
        }
        let mut s = json!({"step": i, "frame": start_frame});
        if let Err(e) = res {
            s["error"] = json!(e);
        }
        steps.push(s);
    }
    let mut report = Map::new();
    for n in &input.report {
        let r = resolve(&g, n)?;
        let st = g.world.get(r).map_err(|e| e.to_string())?.clone();
        let mut v = serde_json::to_value(&st).map_err(|e| e.to_string())?;
        if let Some(x) = g.extra.get(&r) {
            v["layer5"] = serde_json::to_value(x).map_err(|e| e.to_string())?;
        }
        report.insert(r.to_string(), v);
    }
    let name = |op: &u16| g.data.table.get(*op).map(|f| f.name.clone()).unwrap_or_default();
    let unhandled: Map<String, Json> =
        g.unhandled.iter().map(|(op, n)| (format!("0x{op:04X}"), json!({"name": name(op), "calls": n}))).collect();
    let stubbed: Map<String, Json> =
        g.stubbed.iter().map(|(op, n)| (format!("0x{op:04X}"), json!({"name": name(op), "calls": n}))).collect();
    let trace = json!({
        "format": TRACE_FORMAT,
        "seed": input.seed,
        "frame_seconds": input.frame_seconds,
        "title": input.title,
        "observations": observations,
        "steps": steps,
        "given_errors": given_errors,
        "events": g.events,
        "final": {
            "frames": g.frame,
            "quests": quests_json(&g),
            "player": g.player,
            "dialogue": g.dialogue,
            "references": report,
            "unhandled": unhandled,
            "stubbed": stubbed,
        },
    });
    Ok((trace, g.coverage.take()))
}

fn action_label(a: &Action) -> Option<String> {
    match a {
        Action::SetStageRead { label, .. } => Some(label.clone()),
        Action::SetLocal { label, .. }
        | Action::Activate { label, .. }
        | Action::GetItemCount { label, .. }
        | Action::GetDisabled { label, .. }
        | Action::GetGlobal { label, .. }
        | Action::GetVar { label, .. }
        | Action::GetRef { label, .. }
        | Action::SampleRefVar { label, .. }
        | Action::SayFirst { label, .. }
        | Action::GetReputation { label, .. }
        | Action::SetReputationRead { label, .. }
        | Action::UseItem { label, .. }
        | Action::WaitAv { label, .. }
        | Action::GetAv { label, .. }
        | Action::SampleAv { label, .. }
        | Action::SampleAvDamage { label, .. }
        | Action::Talk { label, .. }
        | Action::PlayerActivate { label, .. }
        | Action::PerkChoice { label, .. }
        | Action::DialoguePackage { label, .. }
        | Action::Hit { label, .. }
        | Action::Choose { label, .. }
        | Action::WaitTalk { label, .. }
        | Action::SampleJumps { label, .. }
        | Action::Call { label, .. } => label.clone(),
        Action::CallUntil { label, .. } | Action::Query { label, .. } => Some(label.clone()),
        Action::DismissMessageBoxes { label, .. } => label.clone(),
        Action::WaitQuest { label, .. }
        | Action::WaitMenu { label, .. }
        | Action::GetMenus { label, .. }
        | Action::GetTagSkills { label, .. }
        | Action::ActivateRead { label, .. }
        | Action::AddItemRead { label, .. }
        | Action::SampleItems { label, .. }
        | Action::MenuGap { label, .. }
        | Action::IsCurrentPackage { label, .. } => Some(label.clone()),
        _ => None,
    }
}

/// A query's value as text (`text: true`): numbers as the extender prints them.
fn query_text(v: &fnv_script::vm::XVal) -> String {
    use fnv_script::vm::XVal;
    match v {
        XVal::Num(n) => fnv_script::vm::number_text(*n),
        XVal::Str(s) => s.clone(),
        XVal::Ref(r) => FormId(*r).to_string(),
        XVal::Array(id) => format!("array #{id}"),
    }
}
