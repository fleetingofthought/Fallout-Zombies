//! Layer 5 in the game (workstream 5D, docs/design/layer5-play.md): the `fnv-game`
//! runtime runs next to the simulation in `nvforever --play`.
//!
//! - [`ScriptState`] holds the `fnv_game::Game` (script instances, quests, dispatch).
//!   The Layer 4 world stays in [`SimState`]; for each script run it is swapped into the
//!   game and back ([`with_world`]), so every other part of the game keeps reading
//!   `SimState::world` as before.
//! - The script data (`fnv_game::GameData`: every unit decoded, quests, messages) is
//!   built on its own thread at the start; scripts start once it is ready and the player
//!   stands in the world. The new-game state starts every quest flagged "Start game
//!   enabled" (`Game::new_game`).
//! - Each frame (`PlaySet::Scripts`, after activation, before the menus): inventory
//!   changes made by the menus become OnAdd / OnEquip / OnUnequip / OnDrop; a new player
//!   cell loads cells (OnLoad); trigger volumes give OnTriggerEnter / OnTrigger /
//!   OnTriggerLeave ([`crate::script_trigger`]); then `Game::tick` with the frame's real
//!   time: quest scripts by their delay, then GameMode (or MenuMode while a menu or a
//!   message box is open) of the scripted references in loaded cells, then the dialogue
//!   lines being said.
//! - The trace events of the frame become engine effects
//!   ([`crate::script_events::effects`]): `RefChanged` for enabled / disabled / deleted
//!   references (and their enable children), `Travel` when a script moves the player or
//!   activates a door for them, `OpenContainer`, lines on screen, the text of message
//!   boxes and dialogue lines for the overlay ([`crate::script_hud`]).
//! - Menus and message boxes go through the runtime's API (scripts.md §7.2, §7.7): the
//!   container and inventory menus call [`engine_menu_opened`] / [`engine_menu_closed`]
//!   (`Game::open_menu` / `close_menu` with 1008 / 1002); a script's message box or
//!   character-creation menu stays open until the player closes it
//!   ([`ScriptState::answer`], keys in [`crate::script_hud`]); the player is held in
//!   `PlayState::Menu` meanwhile ([`hold_player`]). The engine no longer sets
//!   `Game::menu` itself.
//! - Activation by hand goes through [`activate_by_hand`] (called by `activate.rs`): a
//!   reference whose script has an OnActivate block runs it *instead of* the default
//!   action, which then runs only if the script calls `Activate` (`fnv_game`'s rule).
//! - Conversations (`crate::dialogue`, docs/design/dialogue-play.md): activating an actor
//!   whose default action is to talk starts one ([`Runtime::talk_after`]: by hand, from an
//!   OnActivate block's `Activate` and from any script's `Activate player`); scripts start
//!   them with StartConversation. The dialogue screen reads `Game::dialogue` and acts on
//!   it between ticks through [`Runtime::player_action`].
//! - `--new-game` ([`NewGame`]): the opening starts as the New Game menu starts it (VCG00
//!   at stage 0, or the INI's `sCharGenQuest`, [`Runtime::start_opening`],
//!   `crate::game_start`); the player starts at
//!   `VCG01PlayerStartMarkerREF` ([`new_game_start`]).
//! - `DisablePlayerControls` / `EnablePlayerControls` hold the player: after each frame's
//!   scripts the runtime's seven flags go to `control::ControlLocks` ([`sync_locks`]),
//!   which the input, activation, the HUD and the inventory key obey
//!   (docs/spec-notes/controls.md §5).
//!
//! Switches and test aids (environment):
//! - `NVF_PLAY_SCRIPTS=0` turns scripts off (Layer 4 play as before).
//! - `NVF_PLAY_SCRIPT_TRACE=<path>` writes the trace events as JSON lines (one
//!   `fnv.script.trace/1` event per line, with `t` = seconds since scripts started and `gt`
//!   = the runtime's game time, `Game::time`), the test aids (`aid`), the player's
//!   answers to menus (`menu_answer`) and the control groups scripts have off whenever
//!   they change (`controls`), and a `final` line at exit (quests touched, unhandled and
//!   stubbed calls, tick times).
//! - `NVF_PLAY_SCRIPT_FUNCTIONS=<functions.json>` overrides the function catalogue
//!   (default: the repository's `schemas/script/functions.json`).
//! - `NVF_PLAY_TEST_SCRIPT=<when>:<op>[,...]`: stand-ins for Layer 6 and the player
//!   ([`crate::script_events::ScriptAid`]; `<when>` is seconds after the start, or
//!   `<seconds>@<quest>=<stage>`, seconds after the stage was first seen done; when the
//!   scripts first run after the first timed step's time, the timed steps all come that
//!   much later, [`script_events::delay_late_aids`]; with `NVF_PLAY_AIDS_FROM_SCRIPTS=1`
//!   the seconds are on the world aids' clock, [`crate::pause::WorldAidClock`]):
//!   `start:<quest>`, `stop:<quest>`, `stage:<quest>:<n>`, `objective:<quest>:<n>`,
//!   `objective_done:<quest>:<n>`, `info:<INFO>[:<speaker>]`, `button:<n>`, `close`,
//!   `message:<MESG>`, `menu:<code>`,
//!   `var:<form>:<variable>:<value>`, `walk:<reference>`, `activate:<reference>`,
//!   `enable:<reference>`, `combat:<actor>:<target>`, `moveto:<reference>:<marker>[:x:y:z]`,
//!   `say_to:<speaker>:<target>:<topic>[:<noTargetLookFlag>]`.
//! - `NVF_PLAY_TEST_SCRIPT_MENUS=<seconds>|off`: every menu or message box a script opens
//!   is closed (OK / Done, button 0) after it has been on screen that long. Unattended
//!   runs (any `NVF_PLAY_TEST_*` variable, or `--auto-walk`) default to
//!   [`UNATTENDED_MENU_SECONDS`], so a check that does not answer the boxes is not held by
//!   them [decision]; other runs to off.
//! - `nvforever --play --no-messages` ([`NoMessages`]): a message box that asks nothing
//!   (an OK button only) closes as soon as it is on screen, as OK would; a box with two
//!   or more buttons stays for the player [decision: the flag never chooses an answer].

use crate::activate::outcome::{self, Destination, Effect};
use crate::control::ControlLocks;
use crate::menu::model::Mode;
use crate::menu::ActiveMenu;
use crate::phys::PhysState;
use crate::player::Player;
use crate::script_events::{self, Capsule, EffectCtx, InvView, ScriptAid, ScriptEffect, TimedAid};
use crate::script_hud::ScriptHud;
use crate::sim::SimState;
use crate::loading::TravelRequest;
use crate::{ItemUsed, Location, OpenContainer, PlayData, PlaySet, PlayState, PlayWorld, RefChanged};
use bevy::prelude::*;
use fnv_game::game::{event, menus, ActMode};
use fnv_game::script_runner::Moment;
use fnv_game::trace::{Event, EventKind};
use fnv_game::{Game, GameConfig, GameData, InstKey};
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::world::{Activation, Outcome};
use crate::game_start::GameStart;
use fnv_sim::{FormId, RefTable, World};
use fnv_world::WorldIndex;
use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::time::Instant;

/// MenuMode code while a message box is open (`fnv_game::game::menus::MESSAGE`): shipped
/// scripts read the chosen button in `Begin MenuMode 1001` blocks [observed: FalloutNV
/// SCPT 0x00105236, the Goodsprings chemistry set] as well as in GameMode (SCPT
/// 0x0011F291).
pub const MENU_MESSAGE: u16 = menus::MESSAGE;
/// The SayTo function's opcode [observed: schemas/script/functions.json].
const SAY_TO: u16 = 0x1034;
/// `MoveTo` [observed: schemas/script/functions.json, 0x109E].
const MOVE_TO: u16 = 0x109E;
/// `PlayGroup` [observed: activators.md §5].
const PLAY_GROUP: u16 = 0x1013;
/// The container menu [documented: GECK wiki MenuMode, as used in
/// docs/spec-notes/xnvse-capture.md and simulation.md captures].
pub const MENU_CONTAINER: u16 = 1008;
/// The inventory menu, the Pip-Boy (`menus::INVENTORY`, documented). While the Pip-Boy is
/// up its page's code is reported instead (`crate::pipboy`: STATS 1003, ITEMS 1002, DATA
/// 1023, the repair list 1035), `MenuMode 1` for any of them.
pub const MENU_INVENTORY: u16 = menus::INVENTORY;

/// The Sleep / Wait menu (`crate::wait`) [observed: `Begin MenuMode 1012` blocks of six
/// FalloutNV.esm scripts, one commented as the rest menu; docs/spec-notes/wait-menu.md
/// §5].
pub const MENU_SLEEP_WAIT: u16 = 1012;

/// The barter menu (`crate::menu`), which scripts open with ShowBarterMenu
/// (`fnv_game::game::menus::BARTER`).
pub const MENU_BARTER: u16 = menus::BARTER;

/// The MenuMode code of an engine menu.
pub fn menu_code(mode: Mode) -> u16 {
    match mode {
        Mode::Container(_) => MENU_CONTAINER,
        Mode::Inventory => MENU_INVENTORY,
        Mode::Barter(_) => MENU_BARTER,
    }
}

/// A menu the scripts opened (a message box, a character-creation menu), as opposed to
/// the engine's container, inventory, barter, wait, recipe, roulette and slot machine menus: the script overlay shows it
/// and the player closes it there.
pub fn is_script_menu(code: u16) -> bool {
    code != MENU_CONTAINER
        && !menus::is_pipboy(code)
        && code != MENU_SLEEP_WAIT
        && code != MENU_BARTER
        && code != crate::terminal::MENU_HACKING
        && code != crate::terminal::MENU_TERMINAL
        && code != crate::lockpick::MENU_LOCKPICK
        && code != crate::blackjack::MENU_BLACKJACK
        && code != crate::caravan::MENU_CARAVAN
        && code != fnv_game::crafting::RECIPE_MENU
        && code != fnv_game::repair::REPAIR_SERVICES_MENU
        && code != crate::roulette::MENU_ROULETTE
        && code != crate::slots::MENU_SLOTS
        && code != fnv_game::companion_wheel::MENU_CODE
}

/// Default of `NVF_PLAY_TEST_SCRIPT_MENUS` in unattended runs, seconds.
pub const UNATTENDED_MENU_SECONDS: f32 = 0.25;

/// The game's periodic package choice, seconds [documented: GECK wiki; packages.md §3].
pub const PACKAGE_INTERVAL: f64 = 10.0;

/// `nvforever --play --no-messages`: message boxes that ask nothing close at once
/// (inserted before the play plugin).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct NoMessages;

/// `nvforever --play --new-game`: the game starts as a new game does (inserted before
/// the play plugin).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct NewGame;

/// The interior a new game starts in: VCG00 stage 0 moves the player to
/// `VCG01PlayerStartMarkerREF` [observed in the masters], which the new-game save `nvfnew`
/// has in Doc Mitchell's house, cell 0x00103DF9 [observed: layer5-captures.md §10.2].
pub const NEW_GAME_CELL: &str = "GSDocMitchellHouse";

/// Where `--new-game` puts the player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewGameStart {
    pub interior: u32,
    pub feet: [f32; 3],
    pub heading_deg: f32,
}

/// The new game's start: the marker the opening moves the player to
/// (`fnv_game::runner::NEW_GAME_START`) in [`NEW_GAME_CELL`].
pub fn new_game_start(lo: &fnv_esm::LoadOrder, schema: &fnv_esm::Schema, index: &WorldIndex) -> Result<NewGameStart, String> {
    let cell = index.cell_by_name(NEW_GAME_CELL).filter(|c| c.world.is_none()).ok_or_else(|| format!("no interior cell {NEW_GAME_CELL}"))?;
    for &r in &cell.refs {
        let Some((pi, rec)) = fnv_world::fields::winner(lo, r) else { continue };
        let Ok(f) = fnv_world::fields::Fields::new(rec, pi, schema) else { continue };
        if f.string(b"EDID").is_some_and(|e| e.eq_ignore_ascii_case(fnv_game::runner::NEW_GAME_START)) {
            let p = fnv_world::placed::placement(lo, schema, r).ok_or_else(|| format!("{r:08X}: no placement"))?;
            return Ok(NewGameStart { interior: cell.form_id, feet: p.position, heading_deg: outcome::heading_deg(p.rotation) });
        }
    }
    Err(format!("no {} in {NEW_GAME_CELL}", fnv_game::runner::NEW_GAME_START))
}

/// Where the function catalogue is when `NVF_PLAY_SCRIPT_FUNCTIONS` is not set.
const DEFAULT_FUNCTIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/script/functions.json");

type Loaded = Result<(&'static GameData, f64), String>;

/// The Layer 5 runtime of the game being played.
#[derive(Resource)]
pub struct ScriptState {
    loader: Option<Mutex<Receiver<Loaded>>>,
    pub rt: Option<Runtime>,
    /// Why scripts are not running (switched off, data failed), for the log.
    pub off: Option<String>,
    trace: Option<TraceLog>,
    aids: Vec<TimedAid>,
    /// The timed aids were looked at in a frame the scripts ran
    /// ([`script_events::delay_late_aids`] then).
    aids_started: bool,
    /// Gated aids: when each (quest, stage) was first seen done (seconds since start).
    gates: HashMap<(String, i32), f32>,
    /// `NVF_PLAY_TEST_SCRIPT_MENUS`: close a script's menu after this many seconds.
    auto_close: Option<f32>,
    /// `--no-messages`: close a message box without a choice at once.
    no_messages: bool,
    /// The script menu on screen (code, front box) and since when (seconds).
    on_screen: Option<(u16, Option<FormId>, f32)>,
    /// `walk:` test aids due: where to put the player's feet.
    walks: Vec<(String, [f32; 3])>,
    new_game: bool,
    /// What a new game starts with (`crate::game_start`, the INI's `[General]`).
    start: GameStart,
    /// Aid items the player used that the runtime has not taken yet (used before the
    /// scripts were ready, or under a loading screen or the pause menu).
    used: Vec<(FormId, Option<u16>)>,
    /// Console lines for the next script frame (`crate::console`).
    pub console: Vec<crate::console::ConsoleCall>,
    /// What the script frame answered to them.
    pub console_out: Vec<crate::console::ConsoleReply>,
    /// A save to restore when the runtime is made (`crate::save`, nvforever `--load`).
    pending_game: Option<PendingGame>,
    /// The runtime is not made (nor the save put back) while this is set: a load while
    /// playing holds it until the old run has ended (nvforever `handoff`; owner
    /// 2026-11-01, #1357, #1651).
    pub held: bool,
}

/// A save's game snapshot for the runtime to restore when it is made (`crate::save`).
#[derive(Resource)]
pub struct PendingGame(pub Box<fnv_game::game::GameSnapshot>, pub PathBuf);

impl ScriptState {
    /// The scripts are being loaded: not running yet, and not switched off.
    pub fn loading(&self) -> bool {
        self.rt.is_none() && self.off.is_none()
    }

    /// A save is waiting to be restored into the runtime.
    pub fn pending_game_waiting(&self) -> bool {
        self.pending_game.is_some()
    }
}

/// The running game and what the engine side remembers between frames.
pub struct Runtime {
    pub game: Game<'static>,
    /// The cell last handed to `Game::player_entered`.
    player_cell: Option<FormId>,
    /// The player's inventory after the last script run (changes since are the menus').
    inventory: InvView,
    /// The newest created reference seen (a newer one of a removed item is a drop).
    created_seen: Option<FormId>,
    /// References seen marked for delete (MarkForDelete has no trace event).
    marked: BTreeSet<FormId>,
    /// Indices into `game.events` whose effects the caller applied (activation by hand).
    skip: BTreeSet<usize>,
    /// The player answered the open script menu: it closes after this frame's tick (so a
    /// `MenuMode 1001` block still runs once with the button pressed). The menu and the
    /// box that was in front.
    closing: Option<(u16, Option<FormId>)>,
    /// The game in place is a save's, not a new game's: its clock is the save's (the
    /// residents leave it alone, `Core::prepare` in `nvforever::residents`). A restore that
    /// fails installs no runtime ([`run_scripts`]), so `false` is always a new game.
    pub restored_from_save: bool,
    /// `--new-game`: the opening starts on the first frame scripts run.
    opening_pending: bool,
    /// A run without a save: the Script Runner's new-game files (`gn_`, `ln_`) run on the
    /// first frame scripts run, when the new game begins, not when the runtime is made:
    /// behind the main menu no game has begun yet, and Continue or Load there starts
    /// another run that never runs them (#1548) [hypothesis: the original runs them when
    /// a new game starts, docs/spec-notes/ext-script-runner.md §2].
    new_game_files: bool,
    /// An actor the player woke by talking to it, and when (`Game::time`): the
    /// conversation starts once it is off its bed ([`Runtime::talk_when_awake`]).
    wake_talk: Option<(FormId, f64)>,
    /// The quest the opening starts (`crate::game_start`).
    start: GameStart,
    /// Where the player was last frame, and frames left whose time does not count (a
    /// door's loading screen).
    location: Option<Location>,
    frozen_frames: u32,
    pub triggers: crate::script_trigger::Triggers,
    /// Enable parent → references that follow it.
    children: HashMap<FormId, Vec<FormId>>,
    started: Instant,
    /// The scripts have run a frame (`Game::tick`). Under the start's loading screen the
    /// runtime is made but holds its first frame, so OnLoad has not run and the state
    /// read from it is the stored one, not yet the scripts' (`crate::loading`).
    pub ticked: bool,
    pub stats: TickStats,
    /// What the player's aid items did (`crate::aid`).
    pub aid: crate::aid::AidStats,
    /// The perks' choice list answered with the default action: the target is activated
    /// again as E does (`crate::activate`), without the list (`fnv_game::activate_choices`).
    pub activate_again: Option<FormId>,
    choices_passed: Option<FormId>,
    /// The button picked in the perks' choice list, taken in the next tick (its script
    /// needs the world).
    activate_pick: Option<usize>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TickStats {
    pub frames: u64,
    pub total_ms: f64,
    pub worst_ms: f64,
    pub events: u64,
}

impl ScriptState {
    fn new(d: PlayData, refs: &'static RefTable, new_game: bool, unattended: bool, no_messages: bool) -> ScriptState {
        let off = match std::env::var("NVF_PLAY_SCRIPTS").as_deref() {
            Ok("0") | Ok("off") | Ok("false") => Some("NVF_PLAY_SCRIPTS=0".to_string()),
            _ => None,
        };
        let aids = match std::env::var("NVF_PLAY_TEST_SCRIPT") {
            Ok(v) => match script_events::parse_aids(&v) {
                Ok(a) => a,
                Err(e) => {
                    warn!("NVF_PLAY_TEST_SCRIPT: {e}");
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        };
        let auto_close = match std::env::var("NVF_PLAY_TEST_SCRIPT_MENUS").as_deref() {
            Ok("off") | Ok("false") => None,
            Ok(v) => match v.trim().parse::<f32>() {
                Ok(s) if s >= 0.0 => Some(s),
                _ => {
                    warn!("NVF_PLAY_TEST_SCRIPT_MENUS={v}: expected seconds or off");
                    None
                }
            },
            Err(_) => (unattended || d.auto_walk > 0.0).then_some(UNATTENDED_MENU_SECONDS),
        };
        if let Some(s) = auto_close {
            info!("test aid: menus and message boxes that scripts open close themselves after {s} s");
        }
        if no_messages {
            info!("--no-messages: message boxes without a choice close at once");
        }
        let trace = std::env::var_os("NVF_PLAY_SCRIPT_TRACE").map(PathBuf::from).and_then(|p| match TraceLog::create(&p) {
            Ok(t) => Some(t),
            Err(e) => {
                warn!("NVF_PLAY_SCRIPT_TRACE {}: {e}", p.display());
                None
            }
        });
        source_compile_mode(unattended);
        // Mods' files are written under NVForever's own mods layer, never the game folder;
        // an unattended run in the player's home keeps them in memory, as the settings
        // (fnv_game::mod_files).
        let own_home = std::env::var_os(fnv_install::HOME_ENV).is_some_and(|h| !h.is_empty());
        let write = (!unattended || own_home).then(|| fnv_install::EngineHome::default_root().map(|r| fnv_install::EngineHome::new(r).mods())).flatten();
        fnv_game::mod_files::update_roots(|r| r.write = write);
        let loader = off.is_none().then(|| Mutex::new(start_loading(d, refs)));
        ScriptState { loader, rt: None, off, trace, aids, aids_started: false, gates: HashMap::new(), auto_close, no_messages, on_screen: None, walks: Vec::new(), new_game, start: GameStart::default(), used: Vec::new(), console: Vec::new(), console_out: Vec::new(), pending_game: None, held: false }
    }

    /// The player answers the script's open menu: `Some(b)` picks button b (0 = first) of
    /// the front message box, `None` is OK / Done. The menu closes after this frame's
    /// scripts ran (`Game::close_menu`; the next queued box then opens). `how` is for the
    /// trace. Returns whether there was a script menu to answer.
    pub fn answer(&mut self, button: Option<i32>, how: &str) -> bool {
        let Some(rt) = self.rt.as_mut() else { return false };
        let menu = rt.game.menu;
        let ok = rt.answer(button);
        if ok {
            if let Some(log) = self.trace.as_mut() {
                log.write(&serde_json::json!({"kind": "menu_answer", "frame": rt.game.frame, "menu": menu, "button": button, "by": how,
                    "t": secs(rt.started), "gt": rt.game.time}));
            }
        }
        ok
    }

    /// A new game whose opening has not started yet (the scripts are loading, or wait for
    /// the player to stand in the world).
    fn before_opening(&self) -> bool {
        match self.rt.as_ref() {
            Some(rt) => rt.opening_pending,
            None => self.new_game && self.loader.is_some(),
        }
    }

    /// The control groups scripts have off now (`Game::player.controls_disabled`). A new
    /// game holds every control until its opening has started: VCG00's first frame turns
    /// the controls off, and the scripts take about a second to load [decision: the
    /// player must not walk off meanwhile].
    pub fn locks(&self) -> ControlLocks {
        match self.rt.as_ref() {
            _ if self.before_opening() => ControlLocks::ALL,
            // The player is dead: nothing more to do [design] (`crate::combat`).
            Some(rt) if rt.game.is_dead(FormId::PLAYER_REF) => ControlLocks::ALL,
            // Knocked down: until the get-up clip is over (docs/spec-notes/unconscious.md
            // §5.4; `nvforever` player_knockdown.rs).
            Some(rt) if rt.game.knockout(FormId::PLAYER_REF).is_some() => ControlLocks::ALL,
            Some(rt) => ControlLocks::from_flags(rt.game.controls_locked()),
            None => ControlLocks::default(),
        }
    }

    /// A line of the engine's own in the trace log (a test aid's step, the player's
    /// input on the dialogue screen), with `frame`, `t` and `gt` added.
    pub(crate) fn trace_note(&mut self, mut v: serde_json::Value) {
        let (Some(rt), Some(log)) = (self.rt.as_ref(), self.trace.as_mut()) else { return };
        v["frame"] = serde_json::json!(rt.game.frame);
        v["t"] = serde_json::json!(secs(rt.started));
        v["gt"] = serde_json::json!(rt.game.time);
        log.write(&v);
        let _ = log.out.flush();
    }
}

fn secs(since: Instant) -> f64 {
    (since.elapsed().as_secs_f64() * 1000.0).round() / 1000.0
}

/// The container or inventory menu opened (menu.rs): it is the runtime's open menu
/// (`Game::open_menu`), MenuMode 1008 / 1002 from the next frame.
pub fn engine_menu_opened(script: Option<&mut ScriptState>, mode: Mode) {
    if let Some(rt) = script.and_then(|s| s.rt.as_mut()) {
        rt.game.open_menu(menu_code(mode));
        // The container's OnOpen, next frame with the world in place (`Game::queue_ref_event`).
        if let Mode::Container(c) = mode {
            rt.game.queue_ref_event(c, event::ON_OPEN, Some(FormId::PLAYER_REF));
        }
    }
}

/// The container or inventory menu closed (menu.rs): `Game::close_menu` when it is still
/// the runtime's open menu (the Pip-Boy by any of its page codes; a message box shown
/// meanwhile then opens), or forgotten when it waits under the tutorial menu
/// (`Game::engine_menu_closed`).
pub fn engine_menu_closed(script: Option<&mut ScriptState>, mode: Mode) {
    if let Some(rt) = script.and_then(|s| s.rt.as_mut()) {
        rt.game.engine_menu_closed(menu_code(mode));
        if let Mode::Container(c) = mode {
            rt.game.queue_ref_event(c, event::ON_CLOSE, Some(FormId::PLAYER_REF));
        }
    }
}

/// The extender's LoadGame (the save's file name) and PostLoadGame (success) events after a
/// load [documented: NVSE Event Handling, session events].
pub(crate) fn loaded_events(rt: &mut Runtime, sim: &mut SimState, path: &std::path::Path) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    with_world(rt, sim, |rt| {
        rt.game.dispatch_event("LoadGame", vec![fnv_script::vm::XVal::Str(name)], None);
        rt.game.dispatch_event("PostLoadGame", vec![fnv_script::vm::XVal::Num(1.0)], None);
    });
}

/// JIP's Script Runner files of a moment (`fnv_game::script_runner`), in the played world;
/// each file's outcome in the log.
fn script_runner(rt: &mut Runtime, sim: &mut SimState, m: Moment) {
    let runs = with_world(rt, sim, |rt| rt.game.run_script_runner(m));
    log_script_runner(runs, m);
}

/// Each Script Runner file's outcome in the log.
fn log_script_runner(runs: Vec<fnv_game::script_runner::SrRun>, m: Moment) {
    for r in runs {
        match r.result {
            Ok(()) => info!("script runner: {} ran ({})", r.path, m.name()),
            Err(e) => warn!("script runner: {} skipped ({}): {e}", r.path, m.name()),
        }
    }
}

/// `[Extenders] CompileSource` of NVForever.ini: which units run from their source text
/// (fnv_game::source_compile; `NVF_COMPILE_SOURCE` overrides it). Read before the script
/// data is built; an unattended run in the player's home leaves the INI alone, as for every
/// setting (crate::settings).
fn source_compile_mode(unattended: bool) {
    use fnv_game::source_compile::{self as sc, Mode};
    let Some(path) = crate::settings::SettingsFile::for_run(unattended).0 else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let Some(v) = fnv_install::ini_get(&text, sc::SECTION, sc::KEY) else { return };
    match Mode::parse(&v) {
        Some(m) => {
            info!("[{}] {}={v}: scripts compiled from source: {m:?}", sc::SECTION, sc::KEY);
            sc::set_mode(m);
        }
        None => warn!("[{}] {}={v}: expected auto, always or never", sc::SECTION, sc::KEY),
    }
}

/// Build the script data on its own thread (decoding every unit takes about a second;
/// render streaming and physics start meanwhile).
fn start_loading(d: PlayData, refs: &'static RefTable) -> Receiver<Loaded> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("nvf-scripts".into()).spawn(move || {
        let t = Instant::now();
        let path = std::env::var_os("NVF_PLAY_SCRIPT_FUNCTIONS").map_or_else(|| PathBuf::from(DEFAULT_FUNCTIONS), PathBuf::from);
        let r = (|| {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            info!("script build: functions.json read and parsed: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
            let step = Instant::now();
            let table = FunctionTable::load(&path).map_err(|e| e.to_string())?;
            info!("script build: function table: {:.0} ms", step.elapsed().as_secs_f64() * 1e3);
            let data: &'static GameData = Box::leak(Box::new(GameData::build(d.lo, d.schema, table, &json, refs)));
            info!("script build: the whole thread: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
            Ok((data, t.elapsed().as_secs_f64()))
        })();
        let _ = tx.send(r);
    });
    if let Err(e) = spawned {
        let (tx2, rx2) = mpsc::channel();
        let _ = tx2.send(Err(format!("cannot start the script loader thread: {e}")));
        return rx2;
    }
    rx
}

/// Run `f` with the simulation's world inside the game (and put it back after).
/// Run `f` with scripts' ray casts going through the physics (`fnv_game::ray`).
/// Neither the player's own capsule nor the terrain is hit [observed: engine2, a cast
/// from 500 above the player meets the floor at the feet; engine3, casts from 100 to
/// 1000 above the ground 200 away read past it with every flag; hypothesis: terrain
/// is left out].
/// The speed one unit of PushObject's force gives a body, game units per second
/// [observed: phys2, a tin can gained about 23 units a frame at 50 frames a second from a
/// force of 500 and 237 from 5000, up or sideways; hypothesis: the same speed whatever
/// the body's mass].
const SCRIPT_PUSH_SPEED: f32 = 2.3;
/// The most speed one PushObject gives a body, game units per second [design: above the
/// fastest observed, 11,900 from a force of 5000].
const SCRIPT_PUSH_MAX_SPEED: f32 = 20_000.0;

/// Scripts' PushObject pushes (`fnv_game::nvse_gaps`): each loose body is pushed along the
/// line from the push's origin to its centre (straight up from the origin itself).
fn apply_pushes(g: &mut Game, phys: &mut PhysState) {
    for (r, origin, force) in std::mem::take(&mut g.gaps.pushes) {
        let Some(c) = phys.world.ref_center(r.0) else { continue };
        let d = [c[0] - origin[0], c[1] - origin[1], c[2] - origin[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let dir = if len > 1e-3 { [d[0] / len, d[1] / len, d[2] / len] } else { [0.0, 0.0, 1.0] };
        let Some(m) = phys.world.ref_mass(r.0) else { continue };
        let k = force * SCRIPT_PUSH_SPEED * m;
        let pushed = phys.world.push_ref(r.0, c, [dir[0] * k, dir[1] * k, dir[2] * k], SCRIPT_PUSH_MAX_SPEED);
        debug!("script: PushObject {r} force {force}: {}", pushed.map_or("no loose body".to_string(), |v| format!("{v:.0} units/s")));
    }
}

fn with_ray_caster<R>(phys: &mut PhysState, f: impl FnOnce() -> R) -> R {
    let filter = fnv_phys::world::Filter { exclude: phys.player.as_ref().map(|c| vec![c.key()]).unwrap_or_default(), no_terrain: true, ..Default::default() };
    let world = std::cell::RefCell::new(&mut phys.world);
    let caster = |a: [f32; 3], b: [f32; 3], kind: fnv_game::ray::CastKind| {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let mut w = world.borrow_mut();
        let hit = match kind {
            fnv_game::ray::CastKind::Scene => w.queries().ray(a, d, len, &filter),
            fnv_game::ray::CastKind::World => w.queries().ray(a, d, len, &fnv_phys::world::Filter { no_terrain: false, ..filter.clone() }),
            // The terrain's own hit, past anything standing on it.
            fnv_game::ray::CastKind::Terrain => w.queries().ray_all(a, d, len, &fnv_phys::world::Filter::default()).into_iter().find(|h| matches!(h.owner, fnv_phys::world::Owner::Terrain { .. })),
        };
        hit.map(|h| {
            let form = match h.owner {
                fnv_phys::world::Owner::Ref { form_id, .. } | fnv_phys::world::Owner::Actor { form_id, .. } => Some(form_id),
                fnv_phys::world::Owner::Character { id } => Some(id),
                fnv_phys::world::Owner::Terrain { .. } => None,
            };
            fnv_game::ray::Hit { distance: h.distance, point: h.point, form, material: h.material }
        })
    };
    fnv_game::ray::with_caster(&caster, f)
}

pub fn with_world<R>(rt: &mut Runtime, sim: &mut SimState, f: impl FnOnce(&mut Runtime) -> R) -> R {
    std::mem::swap(&mut rt.game.world, &mut sim.world);
    let r = f(rt);
    std::mem::swap(&mut rt.game.world, &mut sim.world);
    r
}

/// The placed actors whose carried items are another actor base's (template flag
/// "Inventory", or a leveled pick), each pick made as the game makes it: at the level the
/// reference reads leveled lists at in `sim`'s world, with its pick seed (a saved
/// ResurrectActor roll included; `SimState::pick`, docs/spec-notes/actors.md §1.3).
fn inventory_sources(data: &GameData, sim: &SimState, game: &Game<'_>) -> std::collections::BTreeMap<FormId, FormId> {
    data.inventory_sources_at(sim.refs, |r| sim.pick(game, r))
}

impl Runtime {
    fn new(data: &'static GameData, sim: &mut SimState, new_game: bool, packages: bool) -> Runtime {
        // Message boxes are menus until the player closes them (the default,
        // `GameConfig::modal_message_boxes`). With actors living by their packages, the
        // packages' scripts and idles run from the first frame (the residents ask for the
        // same interval when they start).
        let cfg = GameConfig { frame_seconds: 1.0 / 60.0, package_interval: packages.then_some(PACKAGE_INTERVAL), ..GameConfig::default() };
        let game = Game::new(data, World::new(sim.forms, sim.refs, sim.world.config()), cfg);
        // The simulation's world is the one played (`with_world`): its placed actors carry
        // their template's items too (template flag "Inventory"), each picked at the
        // level it reads leveled lists at, as `Game::new` picks them (not level 1).
        sim.world.set_inventory_sources(inventory_sources(data, sim, &game));
        let mut children: HashMap<FormId, Vec<FormId>> = HashMap::new();
        for (id, p) in &sim.refs.refs {
            if let Some(ep) = p.enable_parent {
                children.entry(ep.reference).or_default().push(*id);
            }
        }
        let mut rt = Runtime {
            game,
            player_cell: None,
            inventory: InvView::default(),
            created_seen: None,
            marked: BTreeSet::new(),
            skip: BTreeSet::new(),
            closing: None,
            restored_from_save: false,
            opening_pending: new_game,
            new_game_files: false,
            wake_talk: None,
            start: GameStart::default(),
            location: None,
            frozen_frames: 0,
            triggers: Default::default(),
            children,
            started: Instant::now(),
            ticked: false,
            stats: TickStats::default(),
            aid: crate::aid::AidStats::default(),
            activate_again: None,
            choices_passed: None,
            activate_pick: None,
        };
        with_world(&mut rt, sim, |rt| {
            rt.game.new_game();
            rt.snapshot_inventory();
        });
        // Placed actors with Base Health 0 lie dead from the start (docs/spec-notes/dead-on-spawn.md).
        let corpses = rt.game.extra.values().filter(|e| e.dead).count();
        info!("new game: {corpses} placed actors start dead (Base Health 0)");
        rt
    }

    /// Put a save's game state in place of the new game's (`crate::save`): the
    /// simulation's world and the game's own state. The random numbers start from a
    /// fresh position unless `NVF_PLAY_TEST_SAVE_SAME_ROLLS=1` (docs/design/save-load.md
    /// §5 q3). Returns the references the save changed or a saved fade move moved (to redraw).
    pub(crate) fn restore_save(&mut self, sim: &mut SimState, mut s: fnv_game::game::GameSnapshot) -> Result<Vec<FormId>, String> {
        if std::env::var_os("NVF_PLAY_TEST_SAVE_SAME_ROLLS").is_none_or(|v| v == "0") {
            let jump = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()) << 16;
            for pos in [&mut s.rng_word_pos, &mut s.sim.rng_word_pos] {
                let p: u128 = pos.parse().map_err(|_| format!("bad rng_word_pos {pos:?}"))?;
                *pos = p.wrapping_add(jump).to_string();
            }
        }
        let world = World::restore(sim.forms, sim.refs, &s.sim)?;
        // Moves the save caught during their fade are made as the game restores: those
        // references are redrawn too, changed by the save or not.
        let changed: Vec<FormId> = s.sim.refs.keys().chain(s.fade_moves.keys()).copied().collect::<BTreeSet<_>>().into_iter().collect();
        let data = self.game.data;
        let cfg = self.game.cfg;
        let session = self.game.take_session();
        // The game restores on the played world, so that what it does there stays (the
        // moves above), and hands it back for `sim`; it keeps a stand-in between turns
        // (`with_world`).
        let stand_in = World::new(sim.forms, sim.refs, world.config());
        self.game = Game::restore(data, world, cfg, &s)?;
        self.game.put_session(session);
        // The engine's seat is not saved, so a load stands the player up
        // (docs/design/save-load.md §3.4); the saved furniture use goes with it, or
        // GetSitting and its kin go on reading the seat (#1573).
        self.game.set_furniture_use(FormId::PLAYER_REF, None);
        let mut world = std::mem::replace(&mut self.game.world, stand_in);
        // The world's session edits (game settings, leveled lists, base containers changed
        // by script) last for the run, not in the save (`fnv_sim::world::WorldSession`).
        // Empty here as the game's session is: a load while playing starts a new run
        // (`crate::save` module notes), where GetGameRestarted is true again and scripts
        // make their session edits anew; kept for a restore within a run.
        world.put_session(sim.world.take_session());
        sim.world = world;
        sim.world.set_inventory_sources(inventory_sources(data, sim, &self.game));
        self.opening_pending = false;
        self.new_game_files = false;
        self.player_cell = None;
        self.restored_from_save = true;
        with_world(self, sim, |rt| {
            // `Game::restore` above reached only the stand-in, and the Script Runner's files
            // and the LoadGame events run ahead of the first tick.
            rt.game.sync_player_level();
            rt.snapshot_inventory();
        });
        Ok(changed)
    }

    /// The New Game menu starts the character-generation quest, VCG00 unless the INI's
    /// `sCharGenQuest` names another (`crate::game_start`) [documented: GECK wiki SetStage
    /// notes, scripts.md §5; Fallout_default.ini], and its MenuMode blocks run once, as
    /// in the main menu as the game starts [hypothesis] (`Game::start_chargen_quest`).
    /// VCG00's MenuMode block sets its stage 0 while the stage is below 5 [observed in the
    /// masters: SCPT 0x00102035]; stage 0 moves the player to the start marker and goes
    /// on to stage 90 and its 3 s countdown [observed in the masters].
    pub fn start_opening(&mut self) {
        let g = &mut self.game;
        match self.start.quest(g) {
            Ok(q) => {
                g.start_chargen_quest(q);
                let name = g.data.edid_of.get(&q).cloned().unwrap_or_else(|| q.to_string());
                let state = g.quest_state(q);
                let at = if state.stages_done.contains(&0) { " at stage 0" } else { "" };
                info!("new game: {name} started{at} (now at stage {})", state.stage());
            }
            Err(e) => warn!("new game: {}={}: {e}; no opening quest started", crate::game_start::CHARGEN_KEY, self.start.chargen_quest),
        }
    }

    /// See [`ScriptState::answer`].
    pub fn answer(&mut self, button: Option<i32>) -> bool {
        if self.closing.is_some() {
            return false;
        }
        // The perks' choice list (`fnv_game::activate_choices`): OK is its first button;
        // taken in the next tick, with the world.
        if self.game.activate_menu.is_some() {
            if self.activate_pick.is_some() {
                return false;
            }
            self.activate_pick = Some(button.unwrap_or(0).max(0) as usize);
            return true;
        }
        let Some(menu) = self.game.menu.filter(|&m| is_script_menu(m)) else { return false };
        let front = if menu == MENU_MESSAGE { self.game.boxes.first().copied() } else { None };
        if let Some(f) = front {
            if let Some(m) = self.game.message_box.as_mut().filter(|m| m.message == f && m.pressed.is_none()) {
                let b = button.unwrap_or(0);
                if b < 0 || b as usize >= m.buttons {
                    return false;
                }
                m.pressed = Some(b);
            }
        }
        info!(
            "script: menu {menu}{} answered ({})",
            front.map_or_else(String::new, |f| format!(" (message {})", self.game.data.name(f))),
            button.map_or("OK".to_string(), |b| format!("button {b}"))
        );
        self.closing = Some((menu, front));
        true
    }

    /// After the tick: close the menu the player answered, if it is still the one open.
    fn finish_answer(&mut self) {
        if let Some((menu, front)) = self.closing.take() {
            if self.game.menu == Some(menu) && (menu != MENU_MESSAGE || self.game.boxes.first().copied() == front) {
                self.game.close_menu();
            }
        }
    }

    /// The player's inventory and the newest created reference, as scripts left them.
    fn snapshot_inventory(&mut self) {
        let w = &self.game.world;
        self.inventory = InvView::of(w.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()));
        self.created_seen = w.created().map(|(id, _)| id).last().or(self.created_seen);
    }

    /// Inventory changes since the last script run (menus): OnAdd for items added,
    /// OnEquip / OnUnequip, OnDrop for items that left as new created references
    /// [hypothesis: the item's script runs on the player's copy, per change, not per
    /// unit; items stored into a container run nothing here].
    fn observe_inventory(&mut self) {
        let g = &mut self.game;
        let now = InvView::of(g.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()));
        let changes = script_events::inventory_changes(&self.inventory, &now);
        if changes.is_empty() {
            return;
        }
        let seen = self.created_seen;
        let dropped: BTreeSet<FormId> = g.world.created().filter(|(id, _)| seen.is_none_or(|s| *id > s)).map(|(_, s)| s.base).collect();
        let player = Some(Value::Ref(FormId::PLAYER_REF.0));
        for (item, n) in &changes.added {
            debug!("script: {n} x {item} reached the player's inventory outside scripts");
            g.item_event(FormId::PLAYER_REF, *item, event::ON_ADD, player);
        }
        // What was taken off before what was put on, as the other equip paths do
        // (docs/spec-notes/ext-events.md; #1616): on a swap the old item's OnUnequip
        // must not undo what the new item's OnEquip has set.
        for item in &changes.unequipped {
            g.item_event(FormId::PLAYER_REF, *item, event::ON_UNEQUIP, player);
        }
        for item in &changes.equipped {
            g.equip_events(FormId::PLAYER_REF, *item, false);
        }
        for (item, _) in changes.removed.iter().filter(|(i, _)| dropped.contains(i)) {
            g.item_event(FormId::PLAYER_REF, *item, event::ON_DROP, player);
        }
        self.snapshot_inventory();
    }

    /// The player is in a new cell: the cells around it load (OnLoad).
    fn follow_player(&mut self) {
        let cell = self.game.world.peek(FormId::PLAYER_REF).and_then(|s| s.cell);
        if cell.is_some() && cell != self.player_cell {
            self.player_cell = cell;
            self.game.player_entered(cell);
            debug!("script: player in cell {:?}; {} cells loaded for scripts", cell, self.game.loaded_cells.len());
            self.log_level_news();
        }
    }

    /// Encounter zones entered and leveled actors fixed by the cell load
    /// (`fnv_game::zones`; the smoke test `encounter-zone-level` reads these lines).
    fn log_level_news(&mut self) {
        let g = &mut self.game;
        let edid = |g: &fnv_game::Game, id: FormId| g.data.edid_of.get(&id).cloned().unwrap_or_else(|| id.to_string());
        for n in g.take_level_news() {
            match n {
                fnv_game::zones::LevelNews::Zone { zone, level, min_level, player_level } => {
                    info!("zones: encounter zone {} ({zone}) entered: level {level} (minimum {min_level}, player level {player_level})", edid(g, zone));
                }
                fnv_game::zones::LevelNews::Actor { actor, list, at, base, at_display, zone } => {
                    let name = |b: Option<FormId>| b.map_or_else(|| "nothing".to_string(), |b| edid(g, b));
                    let line = format!(
                        "zones: leveled actor {actor} ({}) at level {}{} in zone {} stands for {} ({} at level 1)",
                        edid(g, list),
                        at.level,
                        if at.all_levels { " (all levels)" } else { "" },
                        zone.map_or_else(|| "none".to_string(), |z| edid(g, z)),
                        name(base),
                        if base == at_display { "the same".to_string() } else { name(at_display) },
                    );
                    if zone.is_some() {
                        info!("{line}");
                    } else {
                        debug!("{line}");
                    }
                }
            }
        }
    }

    /// References newly marked for delete (no trace event for it).
    fn newly_marked(&mut self) -> Vec<FormId> {
        let now: Vec<FormId> = self.game.extra.iter().filter(|(_, e)| e.marked_for_delete).map(|(r, _)| *r).collect();
        now.into_iter().filter(|r| self.marked.insert(*r)).collect()
    }

    /// The player picks button `index` of the perks' choice list: a perk's choice runs
    /// its script now; the default action activates the target again as E does
    /// ([`Runtime::activate_again`]). Whether a list was open.
    pub fn pick_activate_choice(&mut self, index: usize) -> bool {
        self.observe_inventory();
        let picked = self.game.pick_activate_choice(index);
        let g = &self.game;
        match &picked {
            Some(fnv_game::activate_choices::Picked::Choice(c)) => info!("activate: the player chose {} ({})", c.label, g.data.name(c.perk)),
            Some(fnv_game::activate_choices::Picked::Default(t)) => {
                info!("activate: the player chose the default action on {}", g.data.name(*t));
                self.activate_again = Some(*t);
                self.choices_passed = Some(*t);
            }
            None => return false,
        }
        self.snapshot_inventory();
        true
    }

    fn by_hand(&mut self, target: FormId) -> Result<ByHand, String> {
        // A destroyed door or activator cannot be activated (`Game::activation_refused`).
        if self.game.activation_refused(target) {
            return Ok(ByHand::Refused);
        }
        // Only its activate parents set off a "Parent Activate Only" reference
        // (`fnv_game::traps`).
        if self.game.parent_activate_only(target) {
            info!("activate: {} is set off by its activate parents only", self.game.data.name(target));
            return Ok(ByHand::Refused);
        }
        // The player's perks offer choices on it (Cannibal, Mister Sandman): the list
        // opens first, unless it was just answered with the default action.
        if self.choices_passed.take() != Some(target) && self.game.open_activate_menu(target) {
            let labels: Vec<&str> = self.game.activate_menu.iter().flat_map(|m| m.choices.iter().map(|c| c.label.as_str())).collect();
            info!("activate: {} offers the perks' choices {labels:?} and its default action", self.game.data.name(target));
            return Ok(ByHand::Choices);
        }
        self.observe_inventory();
        let player = FormId::PLAYER_REF;
        let from = self.game.events.len();
        let g = &mut self.game;
        let has_block = g.ref_script(target).and_then(|s| g.data.script(s)).is_some_and(|u| u.has_block(event::ON_ACTIVATE));
        let r = if has_block {
            info!("script: activating {target} runs its OnActivate block instead of the default action");
            g.activate(player, target, ActMode::Hand).map(|_| ByHand::Script)
        } else {
            // The shared default action; what is this path's own is that `activate.rs`
            // applies the outcome, so these events do not become effects as well.
            match g.default_activation_by_hand(target) {
                Ok(Some(done)) => {
                    self.skip.insert(done.activated_event);
                    self.skip.extend(done.item_added_event);
                    // A script without OnActivate does not run on activation; the default
                    // action was all there is.
                    let mut a = done.activation;
                    a.script = None;
                    Ok(ByHand::Default(a))
                }
                Ok(None) => Ok(ByHand::Refused),
                Err(e) => Err(e),
            }
        };
        // An actor whose default action ran (no OnActivate block, or its block called
        // `Activate`) is talked to, if it has a greeting.
        let r = match r {
            Ok(done) => Ok(self.talk_after(from).unwrap_or(done)),
            r => r,
        };
        self.snapshot_inventory();
        r
    }

    /// The player's activations among the events from index `from` on whose default
    /// action is to talk (`Outcome::Talk`: a living actor): the conversation starts with
    /// the actor's greeting (`Game::start_dialogue`). An actor with nothing to say starts
    /// none, and the activation keeps the on-screen line it had ("… has nothing to say").
    /// An actor whose greeting has the Goodbye flag says it at the player and starts none
    /// either (the runtime's rule, docs/spec-notes/dialogue.md §5): the line is heard and
    /// shown like a SayTo line and the player stays free. Returns [`ByHand::Talk`] when a
    /// conversation started, else [`ByHand::Greeted`] when a greeting was said.
    fn talk_after(&mut self, from: usize) -> Option<ByHand> {
        let player = FormId::PLAYER_REF;
        let wanted: Vec<(usize, FormId)> = self
            .game
            .events
            .iter()
            .enumerate()
            .skip(from)
            .filter_map(|(i, e)| match &e.kind {
                EventKind::Activated { by, outcome: Some(o), .. } if *by == player => match script_events::activation_from_json(o)?.outcome {
                    Outcome::Talk { actor } => Some((i, actor)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let mut done = None;
        for (i, actor) in wanted {
            let g = &mut self.game;
            // A teammate: the companion wheel opens instead (`crate::companion_wheel`).
            if g.has_companion_wheel(actor) {
                if g.open_companion_wheel(actor) {
                    self.skip.insert(i);
                    info!("companion wheel: the player activates {}: the wheel opens", g.data.name(actor));
                    done = Some(ByHand::Wheel);
                    continue;
                }
                info!("companion wheel: {} is a teammate, but a menu or conversation is open: talked to instead", g.data.name(actor));
            }
            // Asleep in its bed: it wakes and gets up first; the conversation starts
            // once it is up ([`Runtime::talk_when_awake`]; `Game::wake`).
            if g.asleep(actor) && self.wake_talk.is_none() {
                g.wake(actor);
                self.skip.insert(i);
                self.wake_talk = Some((actor, g.time));
                info!("dialogue: the player talks to {}, asleep: it wakes and gets up first", g.data.name(actor));
                done = Some(ByHand::Woken);
                continue;
            }
            match g.start_dialogue(actor, None) {
                Ok(info) => {
                    // The activation's own effect (the "nothing to say" line) is void.
                    self.skip.insert(i);
                    if g.dialogue.is_some() {
                        info!("dialogue: the player talks to {}: its greeting is {}", g.data.name(actor), g.data.name(info));
                        done = Some(ByHand::Talk);
                    } else {
                        info!("dialogue: {} says its greeting {} at the player (a Goodbye line: no conversation)", g.data.name(actor), g.data.name(info));
                        done = done.or(Some(ByHand::Greeted));
                    }
                }
                Err(e) => {
                    // Unconscious: the corner says so instead of "nothing to say"
                    // (`EventKind::TalkedToUnconscious`, docs/spec-notes/unconscious.md §3).
                    if g.is_out_cold(actor) {
                        self.skip.insert(i);
                    }
                    info!("dialogue: no conversation: {e}");
                }
            }
        }
        done
    }

    /// The conversation with an actor the player woke ([`ByHand::Woken`]) starts once it
    /// is off its bed (getting off played), or after [`WAKE_TALK_SECONDS`] whatever
    /// the engine did; not while a menu or another conversation holds the game.
    pub(crate) fn talk_when_awake(&mut self) {
        let Some((actor, since)) = self.wake_talk else { return };
        let g = &mut self.game;
        if g.dialogue.is_some() || g.in_menu() {
            return;
        }
        let up = g.furniture_use(actor).is_none();
        if !up && g.time - since < WAKE_TALK_SECONDS {
            return;
        }
        self.wake_talk = None;
        match g.start_dialogue(actor, None) {
            Ok(info) if g.dialogue.is_some() => {
                info!("dialogue: {} is up after {:.1} s{}: the player talks to it, its greeting is {}", g.data.name(actor), g.time - since, if up { "" } else { " (still on its bed)" }, g.data.name(info))
            }
            Ok(info) => info!("dialogue: {} is up and says its greeting {} at the player (a Goodbye line: no conversation)", g.data.name(actor), g.data.name(info)),
            Err(e) => info!("dialogue: {} is up, no conversation: {e}", g.data.name(actor)),
        }
    }

    /// The Sleep menu opens for the player lying on `bed`: a bed with an OnActivate
    /// block is activated by the player again, without its default action (the player
    /// is on it already). Returns whether a block ran.
    ///
    /// [engineering choice] The game's bed script (FalloutNV.esm SCPT 0x000B5431) notes
    /// the activation in a variable that its next GameMode frame clears, and gives Well
    /// Rested only if it sees the player sleeping in a menu before that: in the original
    /// the menu must therefore open with the activation [hypothesis]. In NVForever the
    /// player lies down first and the menu opens seconds later, so the activation is
    /// told again when it does (docs/spec-notes/wait-menu.md §8).
    pub(crate) fn bed_activated_for_sleep(&mut self, bed: FormId) -> bool {
        let g = &mut self.game;
        if !g.ref_script(bed).and_then(|s| g.data.script(s)).is_some_and(|u| u.has_block(event::ON_ACTIVATE)) {
            return false;
        }
        let from = g.events.len();
        if let Err(e) = g.activate(FormId::PLAYER_REF, bed, ActMode::Hand) {
            warn!("wait menu: the bed's OnActivate block: {e}");
        }
        let again = g.events.iter().enumerate().skip(from).filter(|(_, e)| matches!(&e.kind, EventKind::Activated { reference, .. } if *reference == bed)).map(|(i, _)| i);
        self.skip.extend(again);
        true
    }

    /// Something the player does to the game between ticks (a choice on the dialogue
    /// screen, a skipped response): inventory changes the menus made are seen first, and
    /// what the action's scripts change is the scripts' own.
    pub(crate) fn player_action<R>(&mut self, f: impl FnOnce(&mut Game<'static>) -> R) -> R {
        self.observe_inventory();
        let r = f(&mut self.game);
        self.snapshot_inventory();
        r
    }

    /// The player activates `r` as E does, wherever the crosshair is (test aids). Nobody
    /// else applies a default action's outcome here: its trace events become effects like
    /// a script's.
    pub(crate) fn activate_as_player(&mut self, r: FormId) -> Result<ByHand, String> {
        let from = self.game.events.len();
        let done = self.by_hand(r)?;
        if !matches!(done, ByHand::Talk | ByHand::Greeted | ByHand::Wheel) {
            self.skip.retain(|&i| i < from);
        }
        Ok(done)
    }

    /// One test-aid step (`walk:` is the caller's: it needs physics).
    fn aid(&mut self, a: &ScriptAid) -> Result<(), String> {
        use fnv_game::runner::resolve;
        let g = &mut self.game;
        match a {
            ScriptAid::Start(q) => {
                let q = resolve(g, q)?;
                g.start_quest(q);
            }
            ScriptAid::Combat(a, t) => {
                let (a, t) = (resolve(g, a)?, resolve(g, t)?);
                g.start_combat(a, t);
            }
            ScriptAid::MoveTo(r, m, off) => {
                let (r, m) = (resolve(g, r)?, resolve(g, m)?);
                let ctx = fnv_game::functions::Ctx { unit: "test aid".into(), opcode: MOVE_TO, on: Some(r), self_ref: Some(r), action_ref: None, quest: None, inst: None, in_condition: false };
                let args: Vec<ArgValue> = std::iter::once(ArgValue::Value(Value::Ref(m.0))).chain(off.iter().map(|v| ArgValue::Value(Value::Num(f64::from(*v))))).collect();
                g.call_function(&ctx, &args);
            }
            ScriptAid::Stop(q) => {
                let q = resolve(g, q)?;
                g.stop_quest(q);
            }
            ScriptAid::Stage(q, s) => {
                let q = resolve(g, q)?;
                g.set_stage(q, *s);
            }
            ScriptAid::Objective(q, i) => {
                let q = resolve(g, q)?;
                g.set_objective(q, *i, Some(true), None);
            }
            ScriptAid::ObjectiveDone(q, i) => {
                let q = resolve(g, q)?;
                g.set_objective(q, *i, None, Some(true));
            }
            ScriptAid::Info(i, speaker) => {
                let i = resolve(g, i)?;
                let sp = speaker.as_deref().map(|s| resolve(g, s)).transpose()?;
                g.run_info(i, sp, true)?;
            }
            ScriptAid::Button(b) => {
                if !self.answer(Some(*b)) {
                    return Err("no script menu or message box to answer with that button".into());
                }
            }
            ScriptAid::Close => {
                if !self.answer(None) {
                    return Err("no script menu or message box is open".into());
                }
            }
            ScriptAid::Message(m) => {
                let m = resolve(g, m)?;
                let def = g.data.messages.get(&m).ok_or_else(|| format!("{} is not a message", g.data.name(m)))?;
                let (text, buttons, is_box) = (def.text.clone(), def.buttons.clone(), def.message_box);
                let hidden = g.hidden_buttons(m, None);
                g.emit(EventKind::MessageShown { message: m, text, buttons: buttons.clone(), hidden });
                if is_box || !buttons.is_empty() {
                    g.show_box(m);
                }
            }
            ScriptAid::Menu(code) => g.open_menu(*code),
            ScriptAid::Activate(r) => {
                let r = resolve(g, r)?;
                let water = g.drink_water(r);
                if let ByHand::Default(a) = self.activate_as_player(r)? {
                    debug!("test aid: activated {r}: {:?}", a.outcome);
                    if let Some(w) = water {
                        info!("test aid: the player drinks from {r}: water {w}");
                    }
                }
            }
            ScriptAid::Kill(r) => {
                let r = resolve(g, r)?;
                if !g.kill(r, Some(FormId::PLAYER_REF)) {
                    return Err(format!("{r} is dead already"));
                }
                info!("test aid: the player killed {r}");
            }
            ScriptAid::Challenge(c, p) => {
                let c = resolve(g, c)?;
                if !g.set_challenge_progress(c, *p) {
                    return Err(format!("{} is not a challenge", g.data.name(c)));
                }
                info!("test aid: challenge {} at {}", g.data.name(c), g.player.challenges[&c].progress);
            }
            ScriptAid::Enable(r) => {
                let r = resolve(g, r)?;
                let ctx = fnv_game::functions::Ctx { unit: "test aid".into(), opcode: 0x1021, on: Some(r), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
                g.call_function(&ctx, &[]);
            }
            ScriptAid::SayTo(speaker, target, topic, no_look) => {
                let (speaker, target, topic) = (resolve(g, speaker)?, resolve(g, target)?, resolve(g, topic)?);
                let ctx = fnv_game::functions::Ctx { unit: "test aid".into(), opcode: SAY_TO, on: Some(speaker), self_ref: Some(speaker), action_ref: None, quest: None, inst: None, in_condition: false };
                let flag = if *no_look { 1.0 } else { 0.0 };
                g.call_function(&ctx, &[ArgValue::Value(Value::Ref(target.0)), ArgValue::Value(Value::Ref(topic.0)), ArgValue::Value(Value::Num(0.0)), ArgValue::Value(Value::Num(flag))]);
                if !g.speech.iter().any(|s| s.speaker == speaker && s.target == Some(target)) {
                    return Err(format!("{} has no line of {} to say to {}", g.data.name(speaker), g.data.name(topic), g.data.name(target)));
                }
            }
            ScriptAid::PlayGroup(r, group, flag) => {
                let r = resolve(g, r)?;
                let code = match group.parse::<u16>() {
                    Ok(c) => c,
                    Err(_) => (0..=u16::from(u8::MAX))
                        .find(|c| g.anim_group_name(*c).is_some_and(|n| n.eq_ignore_ascii_case(group)))
                        .ok_or_else(|| format!("{group} is not an animation group"))?,
                };
                let ctx = fnv_game::functions::Ctx { unit: "test aid".into(), opcode: PLAY_GROUP, on: Some(r), self_ref: Some(r), action_ref: None, quest: None, inst: None, in_condition: false };
                g.call_function(&ctx, &[ArgValue::Enum(code), ArgValue::Value(Value::Num(f64::from(*flag)))]);
            }
            ScriptAid::PlayIdle(r, idle) => {
                let (r, idle) = (resolve(g, r)?, resolve(g, idle)?);
                if !g.ask_idle(r, idle, fnv_game::idles::IdleSource::Script) {
                    return Err(format!("{} does not play {}", g.data.name(r), g.data.name(idle)));
                }
            }
            ScriptAid::Call(on, func, args) => {
                let on = if on == "-" { None } else { Some(resolve(g, on)?) };
                let Some(opcode) = g.data.table.by_name(func) else {
                    // An extender's function has no number in the table: called by name, as a
                    // compiled script's does (`fnv_game::nvse::call`, in lower case). Numbers and
                    // quoted strings as below, anything else a reference.
                    use fnv_script::vm::XVal;
                    let mut values = Vec::new();
                    for a in args {
                        values.push(match a.parse::<f64>() {
                            Ok(n) => XVal::Num(n),
                            Err(_) if a.len() >= 2 && a.starts_with('"') && a.ends_with('"') => XVal::Str(a[1..a.len() - 1].to_string()),
                            Err(_) => XVal::Ref(resolve(g, a)?.0),
                        });
                    }
                    let ctx = fnv_game::game::RunCtx { self_ref: on, ..Default::default() };
                    return match fnv_game::nvse::call(g, &ctx, &func.to_ascii_lowercase(), on, &values) {
                        Some(Ok(value)) => {
                            info!("script: test aid calls {func}{} = {value:?}", on.map(|r| format!(" on {}", g.data.name(r))).unwrap_or_default());
                            Ok(())
                        }
                        Some(Err(error)) => Err(format!("{func}: {error}")),
                        // One the script machine holds itself (MessageBoxEx): a line of script,
                        // as the console's. A line that compiles but stops on an error when it
                        // runs (a `ScriptError` in the trace) fails the aid, so a scenario's
                        // `call:` cannot pass on a line that did nothing.
                        None => {
                            let from = g.events.len();
                            match g.run_source(&format!("{func} {}", args.join(" ")), ctx) {
                                Ok(_) => match g.events[from..].iter().find_map(|e| match &e.kind {
                                    EventKind::ScriptError { error, .. } => Some(error.clone()),
                                    _ => None,
                                }) {
                                    Some(error) => Err(format!("{func}: {error}")),
                                    None => {
                                        info!("script: test aid calls {func}{}", on.map(|r| format!(" on {}", g.data.name(r))).unwrap_or_default());
                                        Ok(())
                                    }
                                },
                                Err(e) => Err(format!("{func} is not a script function: {e}")),
                            }
                        }
                    };
                };
                let mut values = Vec::new();
                for a in args {
                    values.push(match a.parse::<f64>() {
                        Ok(n) => ArgValue::Value(Value::Num(n)),
                        // An axis (Rotate, SetAngle): X, Y or Z.
                        Err(_) if matches!(a.as_str(), "x" | "y" | "z" | "X" | "Y" | "Z") => ArgValue::Axis(a.chars().next().unwrap_or('Z').to_ascii_uppercase()),
                        // A string ("…" in the aid; no comma or colon inside).
                        Err(_) if a.len() >= 2 && a.starts_with('"') && a.ends_with('"') => ArgValue::Str(a[1..a.len() - 1].to_string()),
                        Err(_) => ArgValue::Value(Value::Ref(resolve(g, a)?.0)),
                    });
                }
                let ctx = fnv_game::functions::Ctx { unit: "test aid".into(), opcode, on, self_ref: on, action_ref: None, quest: None, inst: None, in_condition: false };
                let from = g.events.len();
                let value = g.call_function(&ctx, &values);
                if let Some(EventKind::CallFailed { error, .. }) = g.events[from..].iter().map(|e| &e.kind).find(|k| matches!(k, EventKind::CallFailed { .. })) {
                    return Err(format!("{func}: {error}"));
                }
                // What it returned, for the smoke checks of what functions answer.
                let value = match value {
                    Value::Num(x) => format!("{x}"),
                    Value::Ref(f) => format!("{f:08X}"),
                };
                info!("script: test aid calls {func}{} = {value}", on.map(|r| format!(" on {}", g.data.name(r))).unwrap_or_default());
            }
            ScriptAid::Damage(r, value, amount) => {
                let r = resolve(g, r)?;
                let code = fnv_game::functions::av_code(g, value)?;
                fnv_game::functions::add_actor_value_damage(g, r, code, *amount);
                let now = fnv_game::functions::actor_value(g, r, code)?;
                info!("test aid: {} {value} damaged by {amount}: now {now:.1}", g.data.name(r));
            }
            ScriptAid::SetAv(r, value, v) => {
                let r = resolve(g, r)?;
                let code = fnv_game::functions::av_code(g, value)?;
                fnv_game::functions::set_actor_value_current(g, r, code, *v)?;
                info!("test aid: {} {value} set to {v}", g.data.name(r));
            }
            ScriptAid::Hardcore(on) => {
                g.set_hardcore(*on);
                info!("test aid: Hardcore mode {}", if *on { "on" } else { "off" });
            }
            ScriptAid::Perk(p) => {
                let p = resolve(g, p)?;
                if !g.data.perks.contains_key(&p) {
                    return Err(format!("{} is not a perk", g.data.name(p)));
                }
                *g.player.perks.entry(p).or_default() += 1;
                info!("test aid: the player has the perk {}", g.data.name(p));
            }
            ScriptAid::Walk(_) => unreachable!("handled by the caller"),
            ScriptAid::Var(of, var, value) => {
                let f = resolve(g, of)?;
                let (key, script) = g.instance_of(f).ok_or_else(|| format!("{of} runs no script"))?;
                let index = g.data.script(script).and_then(|c| c.var_index(var)).ok_or_else(|| format!("{of} has no variable {var}"))?;
                g.store(key, Some(script), index, Value::Num(*value), "test aid", false);
                if let InstKey::Quest { quest } = key {
                    debug!("script: test aid set {}.{var} = {value}", g.data.name(quest));
                }
            }
        }
        Ok(())
    }
}

/// What an activation by hand did.
pub enum ByHand {
    /// The target's OnActivate block ran instead of the default action (its effects
    /// arrive with the frame's script events).
    Script,
    /// The default action ran (no OnActivate block, or scripts are off).
    Default(Activation),
    /// The target is an actor with a greeting: a conversation with it started
    /// (`crate::dialogue` shows it).
    Talk,
    /// The target is an actor whose greeting has the Goodbye flag: it said the line at
    /// the player and no conversation started (`Runtime::talk_after`).
    Greeted,
    /// The player's perks offer choices on the target: their list opened
    /// (`fnv_game::activate_choices`, `crate::script_hud`).
    Choices,
    /// The target is the player's teammate: the companion wheel opened
    /// (`crate::companion_wheel`).
    Wheel,
    /// The target is a destroyed door or activator: nothing happened
    /// (`fnv_game::Game::activation_refused`).
    Refused,
    /// The target is an actor asleep in its bed: it wakes and gets up, and the
    /// conversation starts once it is up (`Runtime::talk_when_awake`).
    Woken,
}

/// The longest wait for a woken actor to get off its bed before the conversation starts
/// anyway, seconds of game time [our choice: the getting-off clips of beds last a few
/// seconds].
pub const WAKE_TALK_SECONDS: f64 = 8.0;

/// The player activates `target` by hand (E): through the scripts when they run.
pub fn activate_by_hand(script: Option<&mut ScriptState>, sim: &mut SimState, target: u32) -> Result<ByHand, String> {
    match script.and_then(|s| s.rt.as_mut()) {
        Some(rt) => with_world(rt, sim, |rt| rt.by_hand(FormId(target))),
        None => sim.world.activate(FormId::PLAYER_REF, FormId(target)).map(ByHand::Default).map_err(|e| e.to_string()),
    }
}

// ---- effects context -----------------------------------------------------------------

struct PlayCtx<'a> {
    sim: &'a SimState,
    data: &'a GameData,
    index: &'a WorldIndex,
    children: &'a HashMap<FormId, Vec<FormId>>,
    quests: &'a std::collections::BTreeMap<FormId, fnv_game::quest::QuestState>,
    /// The game, for what scripts changed of a form (SetMessageFlags, SetMessageDisplayTime).
    game: &'a Game<'static>,
}

impl PlayCtx<'_> {
    fn display(&self, form: FormId) -> String {
        let base = self.sim.base_of(form.0).unwrap_or(form);
        self.sim.name(base).map(str::to_string).unwrap_or_else(|| self.data.name(form))
    }
}

impl EffectCtx for PlayCtx<'_> {
    fn name(&self, form: FormId) -> String {
        self.display(form)
    }

    fn is_actor_base(&self, base: FormId) -> bool {
        fnv_game::ai::is_actor_base(self.sim.world.forms, base)
    }

    fn setting_text(&self, name: &str) -> Option<String> {
        match self.sim.forms.settings.get(&name.to_ascii_lowercase()) {
            Some(fnv_sim::settings::Setting::Str(s)) => Some(s.clone()),
            _ => None,
        }
    }

    fn objective_text(&self, quest: FormId, index: i32) -> Option<String> {
        self.data.quests.get(&quest)?.objectives.get(&index)?.text.as_deref().map(|t| self.game.expand_codes(t))
    }

    fn quest_title(&self, quest: FormId) -> Option<String> {
        self.data.quests.get(&quest)?.name.clone().filter(|n| !n.trim().is_empty())
    }

    fn quest_listed(&self, quest: FormId, besides: &[i32]) -> bool {
        let Some(q) = self.quests.get(&quest) else { return false };
        q.objectives.iter().any(|(i, o)| (o.displayed || o.completed) && !besides.contains(i))
    }

    fn is_message_box(&self, message: FormId) -> bool {
        self.data.messages.contains_key(&message) && self.game.message_flags(message) & 1 != 0
    }

    fn message_look(&self, message: FormId) -> (Option<String>, Option<f32>) {
        let Some(m) = self.data.messages.get(&message) else { return (None, None) };
        (m.icon.and_then(|i| self.data.menu_icons.get(&i).cloned()), self.game.message_seconds(message).map(|s| s as f32))
    }

    fn skill_look(&self, code: u16) -> (String, Option<String>) {
        let info = self.data.actor_value_info(code);
        let name = info.map(|i| i.name.clone()).filter(|n| !n.trim().is_empty());
        (name.unwrap_or_else(|| fnv_game::functions::AV_NAMES.get(code as usize).copied().unwrap_or("?").to_string()), info.and_then(|i| i.icon.clone()))
    }

    fn pops_in(&self, r: FormId) -> bool {
        self.sim.refs.refs.get(&r).and_then(|p| p.enable_parent).is_some_and(|ep| ep.pop_in)
    }

    fn enable_children(&self, r: FormId) -> Vec<FormId> {
        let mut out = Vec::new();
        let mut todo = vec![r];
        let mut seen = BTreeSet::from([r]);
        while let Some(p) = todo.pop() {
            for &c in self.children.get(&p).map_or(&[][..], |v| v.as_slice()) {
                if seen.insert(c) {
                    out.push(c);
                    todo.push(c);
                }
            }
        }
        out
    }

    fn player_heading_deg(&self) -> f32 {
        self.sim.world.peek(FormId::PLAYER_REF).map_or(0.0, |s| s.rot[2].to_degrees().rem_euclid(360.0))
    }

    fn destination(&self, cell: Option<FormId>) -> Destination {
        outcome::destination(self.index, cell)
    }

    fn activation_effects(&self, a: &Activation, target: FormId) -> Vec<Effect> {
        outcome::effects(a, target.0, &self.display(target), self.index, |id| self.display(id))
    }

    fn is_furniture(&self, r: FormId) -> bool {
        self.sim.base_of(r.0).and_then(|b| self.sim.forms.get(b)).is_some_and(|b| matches!(b.data, fnv_sim::forms::BaseData::Furniture))
    }

    fn is_terminal(&self, r: FormId) -> bool {
        self.sim.base_of(r.0).and_then(|b| self.sim.forms.get(b)).is_some_and(|b| &b.sig == b"TERM")
    }
}

/// The response texts (NAM1) of a dialogue line, in order.
/// The response texts (NAM1) of a dialogue line, in the record's order.
pub(crate) fn responses(play: &PlayWorld, info: FormId) -> Vec<String> {
    let Some((pi, rec)) = fnv_world::fields::winner(play.lo, info.0) else { return Vec::new() };
    let Ok(f) = fnv_world::fields::Fields::new(rec, pi, play.schema) else { return Vec::new() };
    f.subs.iter().filter(|s| &s.sig == b"NAM1").filter_map(|s| f.decode(s)).filter_map(|v| v.as_str().map(str::to_string)).collect()
}

// ---- trace log -------------------------------------------------------------------------

struct TraceLog {
    out: std::io::BufWriter<std::fs::File>,
    path: PathBuf,
    lines: u64,
}

impl TraceLog {
    fn create(path: &std::path::Path) -> std::io::Result<TraceLog> {
        fnv_install::guard_write(path, None).map_err(std::io::Error::other)?;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
        let head = serde_json::json!({"format": "nvforever.play.script_log/1",
            "events": "one fnv.script.trace/1 event per line (frame, kind, ...), with t = seconds since scripts started and gt = game time (Game::time); aid, menu_answer and controls lines; a final line at exit"});
        writeln!(out, "{head}")?;
        Ok(TraceLog { out, path: path.to_path_buf(), lines: 0 })
    }

    fn write(&mut self, v: &serde_json::Value) {
        if writeln!(self.out, "{v}").is_ok() {
            self.lines += 1;
        }
    }
}

// ---- plugin and systems --------------------------------------------------------------

pub struct ScriptPlugin;

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        let d = app.world().resource::<PlayWorld>().0;
        let refs = app.world().resource::<SimState>().refs;
        let new_game = app.world().contains_resource::<NewGame>();
        let unattended = crate::test_aid::Unattended::from_env().0;
        let no_messages = app.world().contains_resource::<NoMessages>();
        let mut st = ScriptState::new(d, refs, new_game, unattended, no_messages);
        st.start = GameStart::of_app(app);
        app.insert_resource(st)
            .insert_resource(crate::pause::WorldAidClock::from_env())
            .init_resource::<ClockScale>()
            .init_resource::<PlayerTimeScale>()
            .add_plugins((crate::script_hud::ScriptHudPlugin, crate::script_draw::ScriptDrawPlugin, crate::dialogue::DialoguePlugin))
            .add_systems(Update, (take_pending_game, crate::script_hud::menu_keys, crate::tutorial_menu::keys, run_scripts, start_world_aids, sync_locks, walk_player, hold_player, time_multiplier, exit_game).chain().in_set(PlaySet::Scripts))
            .add_systems(Last, (crate::aid::report, write_final));
    }
}

/// ExitGame (`UiRequests::exit_game`): back to the main menu, as the pause menu's Quit
/// to the main menu (`crate::pause::MainMenuRequest`) [documented: GECK wiki ExitGame,
/// "plays credits and returns the game to the main menu": `crate::credits` rolls them and
/// asks for the main menu at their end]. A run
/// a test aid drives quits instead, as the console's `qqq` does, so that it ends where
/// its log can be read [design]. Logged: `script: ExitGame: …`.
fn exit_game(
    mut script: ResMut<ScriptState>,
    unattended: Option<Res<crate::test_aid::Unattended>>,
    mut to_main: MessageWriter<crate::pause::MainMenuRequest>,
    mut exit: MessageWriter<AppExit>,
    (mut credits, data, mut images, formats): crate::credits::Start,
) {
    let Some(rt) = script.rt.as_mut() else { return };
    if std::mem::take(&mut rt.game.ui.exit_game) == 0 {
        return;
    }
    // Test aid `NVF_PLAY_TEST_CREDITS=1`: a driven run rolls the credits too.
    if unattended.is_some_and(|u| u.0) && std::env::var("NVF_PLAY_TEST_CREDITS").as_deref() != Ok("1") {
        info!("script: ExitGame: quit (a test aid drives this run)");
        exit.write(AppExit::Success);
    } else {
        let bc = crate::interface::bc_supported(formats.as_deref());
        if crate::credits::start(&mut credits, data.as_deref(), &mut images, bc) {
            info!("script: ExitGame: the credits, then the main menu");
        } else {
            info!("script: ExitGame: back to the main menu (Credits.txt could not be read)");
            to_main.write(crate::pause::MainMenuRequest);
        }
    }
}

/// A save handed over by `crate::save` waits in the script state until the runtime is
/// made.
fn take_pending_game(world: &mut bevy::prelude::World) {
    if let Some(p) = world.remove_resource::<PendingGame>() {
        world.resource_mut::<ScriptState>().pending_game = Some(p);
    }
}

/// SetGlobalTimeMultiplier (`UiRequests::time_multiplier`): the virtual clock runs at that
/// speed, so everything driven by `Time` (animations, physics, NPCs, the scripts' frame
/// time, the game clock) slows down or speeds up together [documented: GECK wiki
/// SetGlobalTimeMultiplier, "0.5 is slow mo"]. Sounds play at the multiplier too, as tape
/// played faster or slower, unless the call asked for them unchanged
/// (`UiRequests::time_keeps_sound`, the shipped slow motion's choice, QUST 0x00102037
/// stage 27) [documented: same page, keepSoundUnchanged; hypothesis: the sounds that
/// follow are those the engine's own slow motion slows, `audio_out::SoundCategory::slows`,
/// and they follow the multiplier itself].
/// Pausing (loading screens, menus) is the clock's own state and is not touched. The
/// engine's own slow motion ([`ClockScale`]) multiplies it, and so does VATS mode's
/// (`crate::vats_mode`, docs/design/vats-mode.md §4; a kill camera's shot in its place,
/// `crate::kill_cam`), and so does the weapon wheel's (`crate::weapon_wheel`,
/// docs/design/weapon-wheel.md §5), and so does the player's Turbo
/// ([`PlayerTimeScale`] keeps the player at full speed); those also slow sound
/// (`audio_out::SoundSpeed`, [`crate::audio_out::slowed_sound`] of their product).
#[allow(clippy::too_many_arguments)]
fn time_multiplier(
    mut script: ResMut<ScriptState>,
    scale: Option<Res<ClockScale>>,
    (vats, wheel): (Option<Res<crate::vats_mode::VatsMode>>, Option<Res<crate::weapon_wheel::WeaponWheel>>),
    kill_cam: Option<Res<crate::kill_cam::KillCam>>,
    sound: Option<ResMut<crate::audio_out::SoundSpeed>>,
    mut clock: ResMut<Time<Virtual>>,
    mut player_scale: ResMut<PlayerTimeScale>,
    mut told: Local<Option<f32>>,
) {
    let Some(rt) = script.rt.as_mut() else { return };
    let asked = rt.game.ui.time_multiplier.unwrap_or(1.0) as f32;
    let before = told.replace(asked).unwrap_or(1.0);
    if before != asked {
        info!("time multiplier: {before} -> {asked} (SetGlobalTimeMultiplier)");
    }
    // Turbo slows the world but for the player (docs/spec-notes/magic-archetypes.md §2).
    let turbo = rt.game.turbo_time_scale() as f32;
    let exempt = 1.0 / turbo.max(0.01);
    if (player_scale.0 - exempt).abs() > 1e-6 {
        if turbo < 1.0 {
            info!("magic: Turbo on: the world runs at {turbo:.2}, the player at full speed");
        } else {
            info!("magic: Turbo off");
        }
        player_scale.0 = exempt;
    }
    // A kill camera's shot sets the world's speed in place of VATS mode's (vats-mode.md §19).
    let vats_scale = kill_cam.and_then(|k| k.time_scale()).unwrap_or_else(|| vats.map_or(1.0, |v| v.time_scale()));
    let engine = scale.map_or(1.0, |s| s.0.max(0.0)) * vats_scale * wheel.map_or(1.0, |w| w.time_scale()) * turbo;
    let want = asked * engine;
    if (clock.relative_speed() - want).abs() > 1e-6 {
        clock.set_relative_speed(want);
    }
    if let Some(mut sound) = sound {
        let script_sound = if rt.game.ui.time_keeps_sound { 1.0 } else { asked };
        sound.set_if_neq(crate::audio_out::SoundSpeed(crate::audio_out::slowed_sound(engine) * script_sound));
    }
}

/// How much faster than the game's clock the player moves and animates (1 for the same):
/// Turbo slows the world's clock and the player keeps its own speed by this factor, the
/// clock's inverse (`fnv_game::magic_archetypes`, docs/spec-notes/magic-archetypes.md §2).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct PlayerTimeScale(pub f32);

impl Default for PlayerTimeScale {
    fn default() -> Self {
        PlayerTimeScale(1.0)
    }
}

/// The engine's own slow motion on top of SetGlobalTimeMultiplier (1 for none): the
/// death camera slows the game while the player dies (`nvforever` `death_cam.rs`).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ClockScale(pub f32);

impl Default for ClockScale {
    fn default() -> Self {
        ClockScale(1.0)
    }
}

/// The test aids due now: timed ones, and gated ones `at` seconds after their stage was
/// first seen done.
fn due_aids(st: &mut ScriptState, now: f32) -> Vec<ScriptAid> {
    let Some(rt) = st.rt.as_ref() else { return Vec::new() };
    // The first frame the scripts run: steps already due then (the script data took
    // long to load) keep their distances instead of all running in this frame.
    if !std::mem::replace(&mut st.aids_started, true) {
        if let Some(late) = script_events::delay_late_aids(&mut st.aids, now) {
            info!("test aid: the scripts run from {now:.1} s on, after the first script step's time: the timed script steps come {late:.1} s later, as far apart as asked");
        }
    }
    let g = &rt.game;
    for a in &st.aids {
        if let Some(gate) = &a.after {
            if !st.gates.contains_key(gate) {
                let done = fnv_game::runner::resolve(g, &gate.0).ok().is_some_and(|q| g.quests.get(&q).is_some_and(|s| s.stages_done.contains(&gate.1)));
                if done {
                    st.gates.insert(gate.clone(), now);
                }
            }
        }
    }
    let gates = &st.gates;
    let mut due = Vec::new();
    st.aids.retain(|a| {
        let start = match &a.after {
            None => Some(0.0),
            Some(gate) => gates.get(gate).copied(),
        };
        if start.is_some_and(|s| now >= s + a.at) {
            due.push(a.aid.clone());
            false
        } else {
            true
        }
    });
    due
}

/// Start the world aids' clock ([`crate::pause::WorldAidClock`]) in the first frame the
/// scripts ran (as [`run_scripts`] lets them: the player in the world, no pause menu, no
/// loading screen), or, with the scripts off, the first such frame. It reads the aids'
/// clock from its parts, not through `AidTime`, which holds the clock this system writes
/// (one system may not have both: Bevy's B0002, every `--play` run panicked on it).
pub(crate) fn start_world_aids(
    (time, play_clock): (Res<Time>, Res<crate::pause::PlayClock>),
    script: Res<ScriptState>,
    phys: Res<PhysState>,
    paused: Res<crate::pause::Paused>,
    loading: Res<crate::loading::Loading>,
    clock: Option<ResMut<crate::pause::WorldAidClock>>,
) {
    let Some(mut clock) = clock else { return };
    let ran = script.rt.is_some() || script.loader.is_none();
    if !ran || paused.0 || loading.active() || phys.player.is_none() {
        return;
    }
    // The aids' clock (`AidTime::elapsed_secs`).
    let now = time.elapsed_secs() + play_clock.stopped_secs();
    if let Some(held) = clock.start(now) {
        let why = if script.rt.is_some() { "the scripts first run" } else { "the player first stands in the world (scripts are off)" };
        if held > 0.0 {
            info!("test aids: clock starts when {why}, at {now:.1} s: the world aids' steps come {held:.1} s later than asked (a start after {} s), as far apart as asked", crate::pause::SCRIPTS_START_SECONDS);
        } else {
            info!("test aids: clock starts when {why}, at {now:.1} s: the world aids' steps come at the times asked");
        }
    }
}

/// [`run_scripts`]'s loading screen, keys, menu files, the pause menu's page and the
/// count of MenuMode frames under its Mods page (`crate::tiles::mods`), and the main menu
/// still to open (`--menu`).
type ScreenAndMenus<'w, 's> = (
    ResMut<'w, crate::loading::Loading>,
    Option<Res<'w, crate::bindings::Bindings>>,
    crate::tiles::MenuFiles<'w, 's>,
    Res<'w, crate::pause::Page>,
    Local<'s, u64>,
    Option<Res<'w, crate::main_menu::MainMenuStart>>,
);

/// Whether the runtime waits, its data ready or not: held, or with the main menu still to
/// open. `--menu` opens the menu in [`PlaySet::Ui`](crate::PlaySet), after the scripts in the
/// same first frame, and that frame takes seconds. Data ready within it would otherwise
/// start the runtime unpaused and run one frame of a new game behind the menu: its `ln_` files
/// and its opening, before New Game (#1548, #1732; the build used to finish well after it).
fn runtime_waits(held: bool, menu_to_open: bool) -> bool {
    held || menu_to_open
}

/// One frame of scripts, then their effects.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_scripts(
    time: crate::pause::AidTime,
    play: Res<PlayWorld>,
    mut phys: ResMut<PhysState>,
    location: Res<Location>,
    mut sim: ResMut<SimState>,
    mut script: ResMut<ScriptState>,
    mut hud: ResMut<ScriptHud>,
    mut corner: ResMut<crate::corner_messages::CornerMessages>,
    mut quest_lines: ResMut<crate::quest_messages::QuestMessages>,
    (mut changed, mut fades, mut repose): (MessageWriter<RefChanged>, MessageWriter<crate::fade::RefFade>, MessageWriter<crate::RefRepose>),
    mut travel: MessageWriter<TravelRequest>,
    mut open: MessageWriter<OpenContainer>,
    mut used: MessageReader<ItemUsed>,
    (mut sit, mut terminals, mut doors, mut refused): (MessageWriter<crate::furniture::UseFurniture>, MessageWriter<crate::terminal::OpenTerminal>, Option<ResMut<crate::doors::Doors>>, MessageWriter<crate::save::LoadRefused>),
    paused: Res<crate::pause::Paused>,
    (mut screen, bindings, tiles, page, mut mods_ticks, menu_to_open): ScreenAndMenus,
) {
    // Items the player used up in the inventory menu (the menu already removed them),
    // kept until the runtime runs.
    let st = &mut *script;
    st.used.extend(used.read().map(|u| (FormId(u.item), u.limb)));
    if st.rt.is_none() {
        if runtime_waits(st.held, menu_to_open.is_some()) {
            return;
        }
        let Some(rx) = st.loader.as_ref() else { return };
        let got = rx.lock().ok().and_then(|r| r.try_recv().ok());
        match got {
            None => return,
            Some(Err(e)) => {
                warn!("scripts are off: {e}");
                st.off = Some(e);
                st.loader = None;
                return;
            }
            Some(Ok((data, secs))) => {
                st.loader = None;
                let mut rt = Runtime::new(data, &mut sim, st.new_game, play.0.packages);
                rt.start = st.start.clone();
                tiles.install_measure(&mut rt.game);
                let mut restored = false;
                if let Some(PendingGame(snap, path)) = st.pending_game.take() {
                    // Test aid: `save::read_for_load` lets no save through that fails here.
                    let restore = if std::env::var_os("NVF_PLAY_TEST_LOAD_FAILS_LATE").is_some() { Err("test aid NVF_PLAY_TEST_LOAD_FAILS_LATE".to_string()) } else { rt.restore_save(&mut sim, *snap) };
                    match restore {
                        Ok(changed_refs) => {
                            info!("load: restored {}: {} changed references, {} quests, {} script instances", path.display(), changed_refs.len(), rt.game.quests.len(), rt.game.instances.len());
                            restored = true;
                            if let Some(doors) = doors.as_deref_mut() {
                                doors.rest_as_restored(&sim);
                            }
                            tiles.load(&mut rt.game, &sim.world);
                            // The Script Runner's files after the save is in place (the
                            // restore takes the game's state, lambdas included, from the
                            // save): the game restart's, then the load's after LoadGame.
                            script_runner(&mut rt, &mut sim, Moment::Restart);
                            loaded_events(&mut rt, &mut sim, &path);
                            script_runner(&mut rt, &mut sim, Moment::Load);
                            // The frames after a load are the loading screen's for the
                            // scripts, as after travel: their `Begin MenuMode 1007`
                            // blocks run (`game::menus::LOADING`) [hypothesis].
                            rt.frozen_frames = 2;
                            // The drawn world follows the restored simulation.
                            for id in changed_refs {
                                changed.write(RefChanged { form_id: id.0, restored: true });
                            }
                        }
                        Err(e) => {
                            // No game goes on: without a runtime no script runs, the engine
                            // part is not put back, nothing can be saved and the loading
                            // screen stays up until the next run has the main menu.
                            let why = format!("{}: {e}", path.display());
                            warn!("load: {why}; starting again on the main menu");
                            refused.write(crate::save::LoadRefused(why));
                            return;
                        }
                    }
                }
                if rt.game.ui_files.is_none() {
                    tiles.load(&mut rt.game, &sim.world);
                }
                if !restored {
                    script_runner(&mut rt, &mut sim, Moment::Restart);
                    // The new game's files wait for its first frame (`new_game_files`).
                    rt.new_game_files = true;
                }
                let from_source = data.issues.count("script_compiled_from_source");
                let failed = data.issues.count("script_source_does_not_compile");
                if from_source + failed > 0 {
                    info!("scripts: {from_source} units run from their source text, {failed} kept their bytes (the source does not compile)");
                    let issues = data.issues.to_json();
                    let samples = issues.as_array().into_iter().flatten().find(|x| x["kind"] == "script_source_does_not_compile").map(|x| x["samples"].clone());
                    if let Some(s) = samples {
                        info!("scripts: sources that do not compile, for example {s}");
                    }
                }
                let running = rt.game.quests.values().filter(|q| q.running).count();
                info!(
                    "scripts ready in {secs:.2} s: {} units, {} scripts, {} quests ({running} start-game-enabled quests running)",
                    data.units.len(),
                    data.scripts.len(),
                    data.quests.len()
                );
                st.rt = Some(rt);
            }
        }
    }
    // Activation, menu keys or a new runtime may have opened a menu since tiles' first
    // write. Build it before this tick's MenuMode blocks [design: ui-tile-tree.md §4.1].
    if let Some(rt) = st.rt.as_mut() {
        tiles.write(&mut rt.game);
    }
    // Scripts wait until the player stands in the world (its cell is known), and do not
    // run under the pause menu (`crate::pause`): no MenuMode code of the original's pause
    // menu is known, so script time simply stops there [decision]. Nor do they run under
    // a loading screen (`crate::loading`) [observed: loading-screen time does not count,
    // layer5-captures.md §8.3 e].
    // The exception: the pause menu's Mods page (`crate::tiles::mods`, T11), under which
    // the start menu's MenuMode blocks run each frame, as under the original's start menu,
    // so a mod's script fills in its page; nothing else of the frame runs [design].
    if crate::pause::mods_open(&paused, &page) && !screen.active() {
        if let Some(rt) = st.rt.as_mut() {
            rt.game.cfg.frame_seconds = time.delta_secs_f64();
            // In the played world, as every run of the game's scripts (`with_world`).
            with_world(rt, &mut sim, |rt| with_ray_caster(&mut phys, || crate::tiles::mods::tick(&mut rt.game)));
            *mods_ticks += 1;
            if *mods_ticks == 1 {
                info!("scripts: MenuMode {} blocks run under the pause menu's Mods page", fnv_game::game::menus::START);
            }
        }
        return;
    }
    if *mods_ticks > 0 {
        info!("scripts: the Mods page closed after {} MenuMode frames", *mods_ticks);
        *mods_ticks = 0;
    }
    if paused.0 || screen.active() {
        // Under a loading screen the game is held for everyone, not only for the
        // scripts: the AI (`nvforever` residents: packages, Dialogue packages' force
        // greets, hellos, combat) reads `Game::loading` and does nothing until the
        // screen is down [owner's playtest list 7 item 24: Johnson Nash's Dialogue
        // package greeted the player in the Vikki and Vance casino while the loading
        // screen was still up]. The frozen frames after it (below) clear it again.
        if screen.active() {
            if let Some(rt) = st.rt.as_mut() {
                rt.game.loading = true;
            }
        }
        return;
    }
    let Some(character) = phys.player.as_ref() else { return };
    let player_capsule = Some(Capsule { feet: character.feet, radius: character.params.radius, height: character.params.height });
    // The aids' clock, which goes on while a menu holds the picture (`crate::pause`).
    let now = time.elapsed_secs();
    // The script steps are world aids (`crate::pause::WorldAidClock`): with
    // NVF_PLAY_AIDS_FROM_SCRIPTS they count on the clock [`start_world_aids`] starts,
    // and are moved as before when they start after the first step's time.
    let mut due = time.world_secs().map_or_else(Vec::new, |w| due_aids(st, w));
    let used = std::mem::take(&mut st.used);
    let console = std::mem::take(&mut st.console);
    let mut console_out = Vec::new();
    // What the console's lines moved (SetPos ...): the tick clears `Game::turned` first.
    let mut console_turned: BTreeSet<FormId> = BTreeSet::new();
    // A script's menu or box left open by an unattended run: answered with OK / Done.
    let on_screen = st.rt.as_ref().and_then(|rt| {
        let m = rt.game.menu.filter(|&m| is_script_menu(m))?;
        Some((m, if m == MENU_MESSAGE { rt.game.boxes.first().copied() } else { None }))
    });
    match (on_screen, st.on_screen) {
        (Some((m, f)), Some((m0, f0, _))) if (m, f) == (m0, f0) => {}
        (Some((m, f)), _) => st.on_screen = Some((m, f, now)),
        (None, _) => st.on_screen = None,
    }
    // A reputation's box (`crate::reputation`) is the engine's, not a plugin's message.
    let reputation_box = |st: &ScriptState, f: FormId| st.rt.as_ref().is_some_and(|rt| crate::reputation::is_reputation(&rt.game, f));
    if let (true, Some((MENU_MESSAGE, Some(front), _))) = (st.no_messages, st.on_screen.filter(|o| !o.1.is_some_and(|f| reputation_box(st, f)))) {
        // The buttons the front box declared, every one whatever its conditions (OK
        // answers button 0, `ScriptState::answer`, without asking whether it shows); else
        // its Message form's; neither known: left for the player.
        let buttons = st.rt.as_ref().and_then(|rt| rt.game.front_box().and_then(|b| b.buttons).or_else(|| rt.game.data.messages.get(&front).map(|m| m.buttons.len())));
        if buttons.is_some_and(asks_nothing) && st.answer(None, "--no-messages") {
            info!("--no-messages: closed message box {front}");
            st.on_screen = None;
        }
    }
    if let (Some(limit), Some((m, _, since))) = (st.auto_close, st.on_screen) {
        if now - since >= limit && st.answer(None, "test aid NVF_PLAY_TEST_SCRIPT_MENUS") {
            info!("test aid: closed menu {m} after {:.2} s", now - since);
            // The next box may show the same message: its time starts when it opens.
            st.on_screen = None;
        }
    }
    let Some(rt) = st.rt.as_mut() else { return };
    // Loading-screen time does not count [observed: layer5-captures.md §8.3 e]: the
    // runtime runs no script under a loading screen (`game.loading`): here, the two
    // frames after the player's place changed (the destination loads on the spot in one
    // of them). docs/spec-notes/scripts.md §7.2. Menus are the runtime's own
    // (`Game::menu`, opened by scripts and by the engine's menus).
    if rt.location.is_some_and(|l| l != *location) {
        rt.frozen_frames = 2;
    }
    rt.location = Some(*location);
    let loading = rt.frozen_frames > 0;
    rt.frozen_frames = rt.frozen_frames.saturating_sub(1);
    // The frame's time; real time while the virtual clock stands still under a menu
    // (`--menu-pause original`), so MenuMode scripts and a conversation's lines go on.
    let dt = time.delta_secs_f64();
    let t0 = Instant::now();
    let mut marked = Vec::new();
    // EnableLoadingMenu this frame (`fnv_game::functions::ui::enable_loading_menu`).
    let mut loading_menu = false;
    let mut aid_lines = Vec::new();
    let mut walks = Vec::new();
    with_world(rt, &mut sim, |rt| {
        rt.observe_inventory();
        let aid_from = rt.game.events.len();
        for &(item, limb) in &used {
            // A skill book is read (`fnv_game::books`); anything else is an aid item.
            if crate::aid::read_book(rt, item) {
                continue;
            }
            let r = crate::aid::use_item(rt, item, limb);
            info!("script: the player used {item}: {} effect(s) started", r.effects.len());
        }
        rt.follow_player();
        if std::mem::take(&mut rt.new_game_files) {
            log_script_runner(rt.game.run_script_runner(Moment::NewGame), Moment::NewGame);
        }
        if std::mem::take(&mut rt.opening_pending) {
            rt.start_opening();
        }
        for a in &due {
            let r = match a {
                ScriptAid::Walk(target) => fnv_game::runner::resolve(&rt.game, target).and_then(|r| {
                    let pos = rt.game.world.peek(r).map(|s| s.pos).or_else(|| rt.game.world.placed.get(r).map(|p| p.pos));
                    let pos = pos.ok_or_else(|| format!("{target} has no position"))?;
                    walks.push((target.clone(), pos));
                    Ok(())
                }),
                a => rt.aid(a),
            };
            match &r {
                Ok(()) => info!("test aid: script {a:?}"),
                Err(e) => warn!("test aid: script {a:?}: {e}"),
            }
            aid_lines.push(serde_json::json!({"kind": "aid", "frame": rt.game.frame, "op": format!("{a:?}"), "error": r.err(),
                "t": secs(rt.started), "gt": rt.game.time}));
        }
        due.clear();
        with_ray_caster(&mut phys, || {
            for c in &console {
                console_out.push(crate::console::run_call(rt, c));
            }
        });
        console_turned.extend(rt.game.turned.iter().copied());
        if let Some(i) = rt.activate_pick.take() {
            rt.pick_activate_choice(i);
        }
        let from = rt.game.events.len();
        // A conversation with the player pauses the game: nobody moves, and no trigger
        // block runs (OnTrigger would, every frame) until it is over.
        if rt.game.dialogue.is_none() {
            crate::script_trigger::update(rt, &play, &phys, player_capsule);
        }
        rt.game.loading = loading;
        rt.game.cfg.frame_seconds = dt;
        // Scripts' GetFallTimeElapsed (`fnv_game::nvse_gaps`).
        rt.game.gaps.air_times = phys
            .player
            .iter()
            .map(|c| (FormId::PLAYER_REF, c.air_time))
            .chain(phys.npcs.iter().map(|(&r, b)| (FormId(r), b.ch.air_time)))
            .filter(|(_, t)| *t > 0.0)
            .collect();
        with_ray_caster(&mut phys, || rt.game.tick());
        apply_pushes(&mut rt.game, &mut phys);
        rt.ticked = true;
        crate::aid::after_tick(rt, aid_from);
        // A script's `Activate player` on an actor: the player talks to it.
        rt.talk_after(from);
        rt.talk_when_awake();
        rt.finish_answer();
        rt.snapshot_inventory();
        marked = rt.newly_marked();
        loading_menu = std::mem::take(&mut rt.game.ui.loading_menu);
    });
    st.walks.extend(walks);
    st.console_out.extend(console_out);
    let ms = t0.elapsed().as_secs_f64() * 1e3;
    rt.stats.frames += 1;
    rt.stats.total_ms += ms;
    if ms > rt.stats.worst_ms {
        rt.stats.worst_ms = ms;
        if rt.stats.frames > 1 && ms > 4.0 {
            debug!("scripts took {ms:.1} ms this frame (the longest so far)");
        }
    }
    // Effects of everything since the last frame (this frame's tick and any activation
    // by hand earlier in the frame).
    let events = std::mem::take(&mut rt.game.events);
    let skip = std::mem::take(&mut rt.skip);
    rt.stats.events += events.len() as u64;
    // References a `Disable 1` held enabled while they faded, disabled now: their visuals,
    // collision and enable children follow.
    let late = std::mem::take(&mut rt.game.script_looks.disabled_after_fade);
    // References a critical stage culled or brought back (SetCriticalStage,
    // `activate::drawn`), and MoveToMarkerWithFade's fades (`fade`).
    let culled = std::mem::take(&mut rt.game.script_looks.critical_changed);
    let move_fades = std::mem::take(&mut rt.game.script_looks.fades);
    let ctx = PlayCtx { sim: &sim, data: rt.game.data, index: play.index, children: &rt.children, quests: &rt.game.quests, game: &rt.game };
    let mut effects = script_events::effects(&events, &skip, &ctx);
    // Objects a script moved, turned or resized: their collision follows (`crate::phys`).
    for r in rt.game.turned.union(&console_turned) {
        repose.write(crate::RefRepose { form_id: r.0 });
    }
    for r in late {
        info!("script: {} disabled after its fade", rt.game.data.name(r));
        effects.push(ScriptEffect::RefChanged(r.0));
        effects.extend(ctx.enable_children(r).into_iter().map(|c| ScriptEffect::RefChanged(c.0)));
    }
    for r in culled {
        info!("script: {} {} by its critical stage", rt.game.data.name(r), if rt.game.culled(r) { "culled" } else { "drawn again" });
        effects.push(ScriptEffect::RefChanged(r.0));
    }
    for (r, fade_in) in move_fades {
        info!("script: {} fades {} for MoveToMarkerWithFade", rt.game.data.name(r), if fade_in { "in after its move" } else { "out before its move" });
        effects.push(ScriptEffect::Fade { form_id: r.0, fade_in });
    }
    for r in marked {
        info!("script: {r} marked for delete");
        effects.push(ScriptEffect::RefChanged(r.0));
        effects.extend(ctx.enable_children(r).into_iter().map(|c| ScriptEffect::RefChanged(c.0)));
    }
    let t = secs(rt.started);
    let gt = rt.game.time;
    for e in &events {
        log_event(rt.game.data, e);
        crate::dialogue::log_farewell(&mut rt.game, e);
        if let EventKind::LineStarted { info, .. } = &e.kind {
            if !hud.knows_line(*info) {
                hud.remember_line(*info, responses(&play, *info));
            }
        }
    }
    if let Some(log) = st.trace.as_mut() {
        for l in &aid_lines {
            log.write(l);
        }
        for e in &events {
            if let Ok(mut v) = serde_json::to_value(e) {
                v["t"] = serde_json::json!(t);
                v["gt"] = serde_json::json!(gt);
                log.write(&v);
            }
        }
        if !events.is_empty() || !aid_lines.is_empty() {
            let _ = log.out.flush();
        }
    }
    if loading_menu && !effects.iter().any(|e| matches!(e, ScriptEffect::Travel(_) | ScriptEffect::DoorTravel(_))) {
        info!("script: EnableLoadingMenu with no move of the player: no loading screen (a screen without travel is not built)");
    }
    let mut sent = BTreeSet::new();
    let feet = phys.player.as_ref().map(|c| c.feet);
    for e in effects {
        // The player's own trip through a door, not a script's MoveTo (`Loading::arriving`).
        let door = matches!(e, ScriptEffect::DoorTravel(_));
        match e {
            ScriptEffect::RefChanged(id) => {
                if sent.insert(id) {
                    changed.write(RefChanged::new(id));
                }
            }
            ScriptEffect::Fade { form_id, fade_in } => {
                fades.write(crate::fade::RefFade { form_id, fade_in });
            }
            ScriptEffect::Travel(t) | ScriptEffect::DoorTravel(t) if crate::stress::holds_player() => {
                info!("test aid NVF_PLAY_TEST_STRESS: a script's move of the player to {:?} / {:?} at {:?} left out", t.interior, t.world, t.feet);
            }
            ScriptEffect::Travel(t) | ScriptEffect::DoorTravel(t) => {
                // A move to where the player already stands (VCG00 stage 0's MoveTo to the
                // new game's start) needs no loading.
                let here = t.destination(*location) == *location
                    && feet.is_some_and(|f| (f[0] - t.feet[0]).hypot(f[1] - t.feet[1]) < 32.0 && (f[2] - t.feet[2]).abs() < 64.0);
                if here {
                    info!("script: the player is moved to where they stand ({:?}); no travel", t.feet);
                } else {
                    info!("script: travel to {:?} / {:?} at {:?}", t.interior, t.world, t.feet);
                    if loading_menu {
                        // EnableLoadingMenu before the move: its screen is one for
                        // anywhere, not the destination's [documented: GECK wiki
                        // EnableLoadingMenu].
                        info!("script: EnableLoadingMenu: the travel's loading screen is one for anywhere");
                        screen.generic_next = true;
                        loading_menu = false;
                    }
                    screen.arriving = door;
                    travel.write(TravelRequest(t));
                }
            }
            ScriptEffect::OpenContainer(c) => {
                open.write(OpenContainer { container: c });
            }
            ScriptEffect::Notify(mut note) => {
                // The player string codes (`&PCName;`) as they read now; ShowMessage's
                // text has them already, MessageEx's and the notes' not (`fnv_game::text_codes`).
                note.text = rt.game.expand_codes(&note.text);
                // Key strings (`&sUActnVats;`) name the keys bound now.
                if let Some(b) = bindings.as_deref() {
                    note.text = crate::lockpick::help::expand_keys(&note.text, b);
                }
                info!("on screen: {}", note.text.trim().replace('\n', " / "));
                corner.push(note);
            }
            // ShowOff's ToggleQuestMessages 0: no quest line is shown (`UiRequests::quest_messages_off`).
            ScriptEffect::Quest(n) if rt.game.ui.quest_messages_off => info!("quest message dropped (ToggleQuestMessages 0): {}", crate::quest_messages::describe(&n)),
            ScriptEffect::Quest(n) => {
                info!("quest message: {}", crate::quest_messages::describe(&n));
                quest_lines.push(n);
            }
            ScriptEffect::MessageBox(mut b) => {
                // The player string codes in the text and the buttons (`fnv_game::text_codes`).
                b.text = rt.game.expand_codes(&b.text);
                b.buttons = b.buttons.iter().map(|t| rt.game.expand_codes(t)).collect();
                if let Some(k) = bindings.as_deref() {
                    b.text = crate::lockpick::help::expand_keys(&b.text, k);
                }
                hud.message_box(b)
            }
            ScriptEffect::UseFurniture(reference) => {
                info!("script: the player uses furniture {reference:08X} (a script's Activate)");
                sit.write(crate::furniture::UseFurniture { reference });
            }
            ScriptEffect::OpenTerminal(reference) => {
                info!("script: terminal {reference:08X} opens (a script's Activate)");
                terminals.write(crate::terminal::OpenTerminal { reference });
            }
        }
    }
}

/// The control groups scripts turned off or on this frame reach the player's controls
/// (`control::ControlLocks`; the next frame's input obeys them), the log and the trace
/// (a `controls` line with the groups now disabled).
fn sync_locks(mut script: ResMut<ScriptState>, mut locks: ResMut<ControlLocks>, mut stats: ResMut<crate::control::LockStats>) {
    let now = script.locks();
    if *locks == now {
        return;
    }
    *locks = now;
    let names = now.names();
    if script.before_opening() {
        info!("player controls: all held until the new game's opening starts");
        return;
    }
    if script.rt.as_ref().is_some_and(|rt| rt.game.is_dead(FormId::PLAYER_REF)) {
        info!("player controls: all held, the player is dead");
        return;
    }
    if script.rt.as_ref().is_some_and(|rt| rt.game.knockout(FormId::PLAYER_REF).is_some()) {
        info!("player controls: all held, the player is knocked down");
        return;
    }
    stats.changes += 1;
    if names.is_empty() {
        info!("player controls: all enabled");
    } else {
        info!("player controls: {} disabled by scripts", names.join(", "));
    }
    script.trace_note(serde_json::json!({"kind": "controls", "disabled": names}));
}

/// `walk:` test aids: the player's feet on the floor under the point (a stand-in for
/// walking there), as `NVF_PLAY_TEST_GOTO` does.
fn walk_player(mut script: ResMut<ScriptState>, mut phys: ResMut<PhysState>, mut player: ResMut<Player>) {
    if script.walks.is_empty() || phys.player.is_none() {
        return;
    }
    for (target, at) in std::mem::take(&mut script.walks) {
        let hit = phys.place_player(at, 60.0, 1000.0);
        let feet = phys.player.as_ref().map_or(at, |c| c.feet);
        player.feet = feet;
        info!("test aid: walk to {target} ({:.0}, {:.0}, {:.0}): feet ({:.1}, {:.1}, {:.1}) on {:?}", at[0], at[1], at[2], feet[0], feet[1], feet[2], hit.map(|h| h.owner));
    }
}

/// The engine's own menus that hold `PlayState::Menu` themselves (`hold_player`).
type EngineMenus<'w> = (
    Res<'w, crate::terminal::TerminalScreen>,
    Res<'w, crate::lockpick::LockpickScreen>,
    Res<'w, crate::console::Console>,
    Option<Res<'w, crate::crafting::RecipeMenu>>,
    Option<Res<'w, crate::roulette::RouletteScreen>>,
    Option<Res<'w, crate::repair::RepairServices>>,
);

/// While a script's menu or message box is open the player is in `PlayState::Menu`: no
/// moving, looking or activating, and the game clock stops (sim.rs) [observed: the pack
/// boxes held the world clock, layer5-captures.md §10.1]. The engine's own menus manage
/// the state themselves (menu.rs).
#[allow(clippy::type_complexity)]
fn hold_player(
    script: Res<ScriptState>,
    active: Res<ActiveMenu>,
    (paused, wait): (Res<crate::pause::Paused>, Res<crate::wait::WaitMenu>),
    (terminal, lockpick, console, recipe, roulette, repair): EngineMenus,
    (blackjack, poker, caravan, wheel, slots): (Option<Res<crate::blackjack::BlackjackScreen>>, Res<crate::poker::PokerScreen>, Option<Res<crate::caravan::CaravanScreen>>, Option<Res<crate::companion_wheel::WheelScreen>>, Option<Res<crate::slots::SlotsScreen>>),
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
) {
    // The pause menu is also a `PlayState::Menu` without an engine menu: it is its own.
    // So are the wait menu (`crate::wait`), a terminal (`crate::terminal`), the
    // lockpicking menu (`crate::lockpick`), the console (`crate::console`), the recipe
    // menu (`crate::crafting`), the repair services (`crate::repair`), the blackjack table (`crate::blackjack`), the roulette
    // table (`crate::roulette`), the slot machines (`crate::slots`), the poker table (`crate::poker`), the Caravan table (`crate::caravan`) and the
    // companion wheel (`crate::companion_wheel`).
    let table = blackjack.is_some_and(|b| b.0.is_some()) || roulette.is_some_and(|r| r.0.is_some()) || poker.0.is_some() || caravan.is_some_and(|c| c.0.is_some()) || wheel.is_some_and(|w| w.0.is_some()) || slots.is_some_and(|s| s.0.is_some());
    if active.0.is_some() || paused.0 || wait.is_open() || terminal.0.is_some() || lockpick.0.is_some() || console.open || recipe.is_some_and(|r| r.is_open()) || repair.is_some_and(|r| r.is_open()) || table {
        return;
    }
    let held = script.rt.as_ref().and_then(|rt| rt.game.menu).is_some_and(is_script_menu);
    match (*state.get(), held) {
        (PlayState::Playing, true) => next.set(PlayState::Menu),
        (PlayState::Menu, false) => next.set(PlayState::Playing),
        _ => {}
    }
}

/// The trace events worth a log line.
fn log_event(data: &GameData, e: &Event) {
    let n = |f: FormId| data.name(f);
    match &e.kind {
        EventKind::StageSet { quest, stage, entries } => info!("script: {} stage {stage} set (entries {entries:?})", n(*quest)),
        EventKind::StageIgnored { quest, stage, reason } => debug!("script: {} stage {stage} ignored ({reason})", n(*quest)),
        EventKind::QuestCompleted { quest } => info!("script: quest {} completed", n(*quest)),
        EventKind::QuestStopped { quest } => debug!("script: quest {} stopped", n(*quest)),
        EventKind::ObjectiveDisplayed { quest, index, on } => info!("script: {} objective {index} displayed {on}", n(*quest)),
        EventKind::ObjectiveCompleted { quest, index, on, .. } => info!("script: {} objective {index} completed {on}", n(*quest)),
        EventKind::MessageShown { message, text, buttons, hidden } => {
            let hidden = if hidden.is_empty() { String::new() } else { format!(" (hidden by their conditions: {hidden:?})") };
            info!("script: message {} {:?}{}{hidden}", n(*message), text.as_deref().unwrap_or(""), if buttons.is_empty() { String::new() } else { format!(" buttons {buttons:?}") })
        }
        EventKind::RefEnabled { reference, fade } => info!("script: {} enabled{}", n(*reference), if *fade { " (fading in)" } else { "" }),
        EventKind::RefDisabled { reference, fade } => info!("script: {} disabled{}", n(*reference), if *fade { " (fading out)" } else { "" }),
        EventKind::Scaled { reference, scale } => info!("script: {} scaled to {scale}", n(*reference)),
        EventKind::Moved { reference, cell, pos } => info!("script: {} moved to {cell:?} {pos:?}", n(*reference)),
        EventKind::Activated { reference, by, outcome } => {
            info!("script: {} activated by {}: {}", n(*reference), n(*by), outcome.as_ref().map_or_else(String::new, |o| o.to_string()))
        }
        EventKind::ActivateChild { parent, child, delay } => info!("script: activate parent {} sets off {} after {delay:.2} s", n(*parent), n(*child)),
        EventKind::BlockRun { unit, block, on } if block != "OnTrigger" => info!("script: {block} of {unit} ran on {on:?}"),
        EventKind::MenuOpened { menu } => info!("script: menu {menu} open"),
        EventKind::MenuClosed { menu } => info!("script: menu {menu} closed"),
        EventKind::LineStarted { speaker, info, seconds, .. } => info!("script: {} says {} ({seconds:.2} s)", n(*speaker), n(*info)),
        EventKind::LineEnded { speaker, info, cut } => info!("script: {} {} line {}", n(*speaker), if *cut { "cut" } else { "ended" }, n(*info)),
        EventKind::ItemAdded { to, item, count, .. } => debug!("script: {count} x {} added to {}", n(*item), n(*to)),
        EventKind::ItemRemoved { from, item, count, .. } => debug!("script: {count} x {} removed from {}", n(*item), n(*from)),
        EventKind::Unhandled { name, unit, .. } => debug!("script: unhandled {name} in {unit}"),
        EventKind::ScriptError { unit, error } => warn!("script error in {unit}: {error}"),
        EventKind::ScriptPrint { unit, text } => info!("script: print {text:?} ({unit})"),
        EventKind::CallFailed { name, unit, error, .. } => debug!("script: {name} failed in {unit}: {error}"),
        EventKind::ArgUnread { name, unit, param, got, wanted, .. } => {
            debug!("script: {name} in {unit} could not read parameter {} ({got} as {wanted}) and went on with a default", param + 1)
        }
        _ => crate::dialogue::log_event(data, e),
    }
}

/// At exit: the trace's final line and a summary in the log.
fn write_final(mut exit: MessageReader<AppExit>, mut script: ResMut<ScriptState>) {
    if exit.read().next().is_none() {
        return;
    }
    let st = &mut *script;
    let Some(rt) = st.rt.as_ref() else {
        info!("scripts: never ran ({})", st.off.as_deref().unwrap_or("not ready before exit"));
        return;
    };
    let g = &rt.game;
    let s = rt.stats;
    let name = |op: &u16| g.data.table.get(*op).map(|f| f.name.clone()).unwrap_or_default();
    let counts = |m: &std::collections::BTreeMap<u16, u64>| -> serde_json::Map<String, serde_json::Value> {
        m.iter().map(|(op, n)| (format!("0x{op:04X}"), serde_json::json!({"name": name(op), "calls": n}))).collect()
    };
    // Quests something happened to (most of the hundreds started at the new game only run).
    let quests: serde_json::Map<String, serde_json::Value> = g
        .quests
        .iter()
        .filter(|(_, q)| !q.stages_done.is_empty() || q.completed || !q.objectives.is_empty() || !q.running)
        .map(|(id, q)| {
            let objectives: Vec<serde_json::Value> =
                q.objectives.iter().map(|(i, o)| serde_json::json!({"index": i, "displayed": o.displayed, "completed": o.completed})).collect();
            (
                id.to_string(),
                serde_json::json!({"edid": g.data.edid_of.get(id), "running": q.running, "completed": q.completed,
                    "stage": q.stage(), "stages_done": q.stages_done, "objectives": objectives}),
            )
        })
        .collect();
    let mean = if s.frames > 0 { s.total_ms / s.frames as f64 } else { 0.0 };
    info!(
        "scripts: {} frames, {:.3} ms per frame on average, longest {:.1} ms; {} events; {} quests running; menu {:?}, {} boxes waiting; unhandled calls {}, stubbed {}",
        s.frames,
        mean,
        s.worst_ms,
        s.events,
        g.quests.values().filter(|q| q.running).count(),
        g.menu,
        g.boxes.len(),
        g.unhandled.values().sum::<u64>(),
        g.stubbed.values().sum::<u64>()
    );
    if let Some(log) = st.trace.as_mut() {
        let v = serde_json::json!({"kind": "final", "frame": g.frame, "game_seconds": g.time, "frames": s.frames,
            "tick_ms": {"mean": mean, "worst": s.worst_ms}, "events": s.events,
            "quests_running": g.quests.values().filter(|q| q.running).count(), "menu": g.menu, "boxes": g.boxes,
            "quests": quests, "player": g.player, "unhandled": counts(&g.unhandled), "stubbed": counts(&g.stubbed)});
        log.write(&v);
        let _ = log.out.flush();
        info!("script trace: {} lines in {}", log.lines, log.path.display());
    }
}

/// Whether a box of `buttons` declared buttons asks nothing (none, or an OK alone), so
/// `--no-messages` closes it ([`NoMessages`]): a box with two or more stays for the player.
/// The count comes with the box (`fnv_game::Game::front_box`), else from its Message form;
/// a box with neither (a form-less box whose count was lost) is left open. A form-less
/// MessageBoxEx box has no form to count from, and until 2026-10-31 counted 0 and closed.
pub fn asks_nothing(buttons: usize) -> bool {
    buttons <= 1
}

#[cfg(test)]
impl ScriptState {
    /// A runtime over an empty load order, for tests of what works the open menu: nothing
    /// has run, no menu is open.
    pub(crate) fn over_no_plugins() -> ScriptState {
        Self::over_plugins(&[])
    }

    /// As [`ScriptState::over_no_plugins`], over these plugins.
    pub(crate) fn over_plugins(plugins: &[fnv_esm::Plugin]) -> ScriptState {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        let lo = fnv_esm::LoadOrder::new(plugins).unwrap();
        let schema = fnv_esm::Schema::load_dir(root.join("fnv")).unwrap();
        let functions = root.join("script/functions.json");
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
        let forms: &'static fnv_sim::FormStore = Box::leak(Box::new(fnv_sim::FormStore::build(&lo, &schema)));
        let refs: &'static RefTable = Box::leak(Box::new(RefTable::build(&lo, &schema)));
        let data: &'static GameData = Box::leak(Box::new(GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, refs)));
        let game = Game::new(data, World::new(forms, refs, fnv_sim::SimConfig::default()), GameConfig::default());
        let rt = Runtime {
            game,
            player_cell: None,
            inventory: InvView::default(),
            created_seen: None,
            marked: BTreeSet::new(),
            skip: BTreeSet::new(),
            closing: None,
            restored_from_save: false,
            opening_pending: false,
            new_game_files: false,
            wake_talk: None,
            start: GameStart::default(),
            location: None,
            frozen_frames: 0,
            triggers: Default::default(),
            children: HashMap::new(),
            started: Instant::now(),
            ticked: false,
            stats: TickStats::default(),
            aid: crate::aid::AidStats::default(),
            activate_again: None,
            choices_passed: None,
            activate_pick: None,
        };
        ScriptState { loader: None, rt: Some(rt), off: None, trace: None, aids: Vec::new(), aids_started: false, gates: HashMap::new(), auto_close: None, no_messages: false, on_screen: None, walks: Vec::new(), new_game: false, start: GameStart::default(), used: Vec::new(), console: Vec::new(), console_out: Vec::new(), pending_game: None, held: false }
    }

    /// The player answered the open menu: it closes after this frame's scripts.
    pub(crate) fn answered(&self) -> bool {
        self.rt.as_ref().is_some_and(|rt| rt.closing.is_some())
    }
}

#[cfg(test)]
#[path = "script_hand_tests.rs"]
mod hand_tests;

#[cfg(test)]
mod restore_tests {
    use super::*;
    use crate::activate::tests::placed;

    const ACTOR: FormId = FormId(0x100);
    const MARKER: FormId = FormId(0x200);
    const MARKER_CELL: FormId = FormId(0x301);
    const SAVED_AT: [f32; 3] = [100.0, 200.0, 300.0];

    /// A runtime and a played world over two placed references: an actor no save changed
    /// and a marker. The snapshot: the marker moved by the save, so a world without the
    /// save's changes sends the actor to the wrong place, and the actor in its fade.
    fn in_fade() -> (Runtime, SimState, fnv_game::game::GameSnapshot) {
        let mut rt = ScriptState::over_no_plugins().rt.unwrap();
        let mut refs = RefTable::default();
        refs.refs.insert(ACTOR, placed(ACTOR.0, 0x300, 0x302));
        refs.refs.insert(MARKER, placed(MARKER.0, 0x300, MARKER_CELL.0));
        let refs: &'static RefTable = Box::leak(Box::new(refs));
        let forms = rt.game.world.forms;
        let plugins = || World::new(forms, refs, fnv_sim::SimConfig::default());
        let mut sim = SimState { forms, refs, world: plugins() };
        let mut s = with_world(&mut rt, &mut sim, |rt| {
            rt.game.world.get(MARKER).unwrap().pos = SAVED_AT;
            rt.game.snapshot()
        });
        s.fade_moves.insert(ACTOR, fnv_game::functions::misc::FadeMove { marker: MARKER, offset: [1.0, 0.0, 0.0], at: s.time + 1.0 });
        sim.world = plugins();
        (rt, sim, s)
    }

    /// #1589: the move a save caught during its fade is on the played world from the
    /// restore, before any tick, and is redrawn though the save did not change the actor.
    #[test]
    fn a_fade_move_in_a_save_is_made_on_the_played_world() {
        let (mut rt, mut sim, s) = in_fade();
        let changed = rt.restore_save(&mut sim, s).unwrap();
        assert_eq!(changed, [ACTOR, MARKER], "the actor once, with what the save changed");
        let there = [SAVED_AT[0] + 1.0, SAVED_AT[1], SAVED_AT[2]];
        let moved = sim.world.peek(ACTOR).expect("on the played world");
        assert_eq!((moved.pos, moved.cell), (there, Some(MARKER_CELL)));
        assert!(rt.game.world.peek(ACTOR).is_none(), "the game holds its stand-in between turns");
        assert!(rt.game.script_looks.fade_moves.is_empty(), "nothing is left to make it again");
        // What a script reads in its turn.
        with_world(&mut rt, &mut sim, |rt| assert_eq!(rt.game.world.peek(ACTOR).unwrap().pos, there));
    }

    /// #1590: a refused restore touches neither world.
    #[test]
    fn a_refused_restore_leaves_the_played_world_as_it_was() {
        let (mut rt, mut sim, mut s) = in_fade();
        s.format = "fnv.game.state/2".into();
        let e = rt.restore_save(&mut sim, s).unwrap_err();
        assert!(e.contains("unsupported snapshot format"), "{e}");
        assert!(sim.world.peek(ACTOR).is_none() && sim.world.peek(MARKER).is_none());
        assert!(!rt.restored_from_save);
    }

    #[test]
    fn a_loaded_game_rolls_at_the_players_level_before_any_tick() {
        let mut state = ScriptState::over_no_plugins();
        let rt = state.rt.as_mut().unwrap();
        let (forms, refs) = (rt.game.world.forms, rt.game.world.placed);
        let mut sim = SimState { forms, refs, world: World::new(forms, refs, fnv_sim::SimConfig::default()) };
        rt.game.player.level = Some(20);
        let save = rt.game.snapshot();
        assert_eq!(save.sim.config.player_level, 1);
        rt.restore_save(&mut sim, save).unwrap();
        assert_eq!(sim.world.roll_level(), 20);
        assert_eq!(sim.world.config().player_level, 1);
    }
}

#[cfg(test)]
mod no_messages_tests {
    use super::*;

    #[test]
    fn a_box_of_one_button_asks_nothing() {
        assert!(asks_nothing(0) && asks_nothing(1));
        assert!(!asks_nothing(2));
    }
}

#[cfg(test)]
mod runtime_start_tests {
    use super::runtime_waits;

    #[test]
    fn the_runtime_waits_for_the_main_menu_to_open() {
        assert!(runtime_waits(false, true), "the menu opens later this frame");
        assert!(runtime_waits(true, false));
        assert!(!runtime_waits(false, false), "the menu is open, paused, or was never asked for");
    }
}
