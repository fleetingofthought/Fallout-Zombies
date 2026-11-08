//! Saving and loading (docs/design/save-load.md): NVForever's own save files
//! (`fnv_save`), never the original's `.fos`.
//!
//! - **Saving** takes the game snapshot (`fnv_game::GameSnapshot`, the simulation's inside
//!   it) and the engine's own state ([`EngineSnapshot`]) at the end of a frame, after the
//!   script frame's effects have reached the world, so nothing a script asked for is
//!   still queued (design §5 q1). It is allowed in the world, from the pause menu and from
//!   the console; not in a conversation, another menu, a script's message box or under a
//!   loading screen. Requests: [`SaveRequest`] (the console's `save <name>`), the
//!   QuickSave key (F5, slot `quicksave`) and autosaves (slot `autosave`): after each
//!   travel behind a loading screen (a fast travel has its own switch), after a wait and
//!   after a sleep that passed at least an hour (`crate::wait::RestEnded`), and when a
//!   script calls `Autosave` [documented: GECK wiki Autosave], each behind a switch on the
//!   settings page. A script's `SystemSave`
//!   writes the one system slot (`systemsave`), overwritten by each later call; it waits
//!   for a moment saving is allowed, as autosaves do [documented: GECK wiki SystemSave;
//!   the waiting is a hypothesis]. A script's `ForceSave` is a save by hand under a new
//!   name (`Save <n>`, as the pause menu names one) and fails, as one by hand does, when
//!   saving is not allowed [documented: GECK wiki ForceSave]. Both are behind the
//!   scripts' autosave switch. A rolling slot keeps its previous file as `.bak`.
//! - **Loading** starts the world from the plugins at the saved place and applies the save
//!   before the scripts' first frame: nvforever `--load <name|path>` reads the header,
//!   starts there, and hands the body over as [`PendingLoad`]; `crate::script` restores the
//!   game and the simulation when its runtime is made and sends `RefChanged` (marked
//!   `restored`) for every reference the save changed, so the drawn world follows it;
//!   [`apply_engine`] then puts back the engine's part. A save that cannot be restored is
//!   refused as it is read ([`read_for_load`]: nvforever opens the main menu and says why),
//!   before anything starts from it; were a restore still to fail, [`LoadRefused`] ends the
//!   same way. Loading while playing ([`LoadRequest`]: the console's
//!   `load <name>`, the QuickLoad key F9) is nvforever's: it starts itself again with
//!   `--load`, so the save always lands in a world built fresh from the plugins.
//! - Not saved (design §3): physics in motion (objects load at rest), menus, music and
//!   radio playback position, AI agents' patrol and sandbox progress (packages start
//!   again from where each actor stands).
//! - **Thumbnail** (design §5 q8): a 512×288 PNG of the main camera's picture, for the load
//!   menu (`crate::pause`). The snapshot is still taken at the end of the frame the save
//!   was asked in; that frame is drawn without the UI (the root nodes are hidden for it
//!   and shown again the next frame) and captured from the window. When the picture
//!   arrives (a frame or two later), a thread crops it to 16:9, scales it down by area
//!   averaging, encodes the PNG and writes the file, so the frame does not wait for the
//!   encoder or the disk. No picture within [`SHOT_WAIT_FRAMES`] frames: the save is
//!   written without one. A load or quit waits for the saves in flight ([`SaveWrites`]).
//!
//! The random numbers are not restored as saved: a load re-seeds them, so a reload does
//! not replay the same rolls (design §5 q3); test aid `NVF_PLAY_TEST_SAVE_SAME_ROLLS=1`
//! keeps the saved positions.
//!
//! Log: `save: wrote <path> (<kind>, <bytes> bytes, <n> changed references), thumbnail
//! 512x288 (<bytes> bytes PNG) in <ms> ms (snapshot <ms> ms)` (`no thumbnail` without one),
//! `save: not now: <why>`, `load: restored <path>: …`.

use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{Location, PlaySet, PlayState, PlayWorld};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use fnv_save::{PluginEntry, SaveHeader, SaveKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};

pub const BODY_FORMAT: &str = "nvf.save.body/1";
pub const ENGINE_FORMAT: &str = "nvf.engine.state/1";

/// The thumbnail's size (design §5 q8).
pub const THUMB_W: u32 = 512;
pub const THUMB_H: u32 = 288;
/// Frames to wait for the window's picture before a save is written without a thumbnail.
pub const SHOT_WAIT_FRAMES: u32 = 30;

/// Save now (at the end of this frame, when saving is allowed).
#[derive(Message, Debug, Clone)]
pub struct SaveRequest {
    /// The save's name; ignored for the rolling slots (quick, auto).
    pub name: String,
    pub kind: SaveKind,
}

/// Load this save: nvforever starts again with `--load` (module notes).
#[derive(Message, Debug, Clone)]
pub struct LoadRequest {
    pub path: PathBuf,
}

/// Where saves go and the autosave switches.
#[derive(Resource, Debug, Clone)]
pub struct SaveConfig {
    /// `fnv_install::EngineHome::saves` (`Documents\My Games\NVForever\saves`).
    pub dir: Option<PathBuf>,
    pub autosave_travel: bool,
    pub autosave_fast_travel: bool,
    pub autosave_wait: bool,
    pub autosave_sleep: bool,
    pub autosave_scripts: bool,
}

impl Default for SaveConfig {
    fn default() -> Self {
        SaveConfig::with_dir(fnv_install::EngineHome::default_root().map(|r| fnv_install::EngineHome::new(r).saves()))
    }
}

impl SaveConfig {
    /// Saves in `dir`, every autosave switch on.
    pub fn with_dir(dir: Option<PathBuf>) -> SaveConfig {
        SaveConfig { dir, autosave_travel: true, autosave_fast_travel: true, autosave_wait: true, autosave_sleep: true, autosave_scripts: true }
    }

    /// The file of a save by name, or a path given as is (it has a folder or the
    /// extension).
    pub fn path_of(&self, name: &str) -> Result<PathBuf, String> {
        let p = PathBuf::from(name);
        if p.components().count() > 1 || name.ends_with(&format!(".{}", fnv_save::EXTENSION)) {
            return Ok(p);
        }
        let dir = self.dir.as_ref().ok_or("no NVForever home folder (set NVFOREVER_HOME)")?;
        fnv_save::path_of(dir, name).map_err(|e| e.to_string())
    }
}

/// A part of the engine's state another crate keeps (nvforever's sky): saved under `key`
/// in [`EngineSnapshot::extra`] and put back on load.
#[derive(Clone, Copy)]
pub struct SaveHook {
    pub key: &'static str,
    pub save: fn(&World) -> Option<serde_json::Value>,
    pub load: fn(&mut World, &serde_json::Value),
    /// For a save made with another load order (design §4.1): move the FormIDs the part
    /// holds to the current load order ([`fnv_save::PluginRemap::map`]; `None` for a
    /// plugin no longer loaded). `None` for a part that holds none.
    pub remap: Option<fn(&mut serde_json::Value, &fnv_save::PluginRemap)>,
}

#[derive(Resource, Default, Clone)]
pub struct SaveHooks(pub Vec<SaveHook>);

/// Register a [`SaveHook`].
pub fn add_save_hook(app: &mut App, h: SaveHook) {
    app.init_resource::<SaveHooks>();
    app.world_mut().resource_mut::<SaveHooks>().0.push(h);
}

/// The engine's own part of a save (design §3.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EngineSnapshot {
    pub format: String,
    #[serde(default)]
    pub third_person: bool,
    /// The third-person camera's distance.
    #[serde(default)]
    pub distance: f32,
    #[serde(default)]
    pub sneak: bool,
    #[serde(default)]
    pub always_run: bool,
    #[serde(default)]
    pub flashlight: bool,
    /// The weapon in hand and the rounds in its magazine.
    #[serde(default)]
    pub magazine: Option<(u32, u32)>,
    /// The type of round in that magazine (an Ammo Swap's pick; #1574). Absent in older
    /// saves: the weapon's default pick, as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magazine_ammo: Option<u32>,
    /// The Pip-Boy's active quest.
    #[serde(default)]
    pub active_quest: Option<u32>,
    /// Parts kept by other crates ([`SaveHook`]).
    #[serde(default)]
    pub extra: BTreeMap<String, serde_json::Value>,
    /// The same part of players 2–4 of a co-op game (docs/design/coop-save.md); absent
    /// for one player.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coop: Vec<CoopEngine>,
}

/// The engine's part of one of players 2–4 (docs/design/coop-save.md): the fields of
/// [`EngineSnapshot`] that belong to one player. The Pip-Boy's active quest and the other
/// crates' parts stay shared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CoopEngine {
    /// The player's number, 2–4 (`fnv_game::coop::CoopPlayer::slot`).
    pub slot: u8,
    #[serde(default)]
    pub third_person: bool,
    #[serde(default)]
    pub distance: f32,
    #[serde(default)]
    pub sneak: bool,
    #[serde(default)]
    pub always_run: bool,
    #[serde(default)]
    pub flashlight: bool,
    #[serde(default)]
    pub magazine: Option<(u32, u32)>,
}

/// Players 2–4's engine part: kept from the save loaded and written into the next one
/// (nothing plays them yet; the game layer keeps the rest, `fnv_game::Game::coop`).
#[derive(Resource, Debug, Clone, Default)]
pub struct CoopEngineState(pub Vec<CoopEngine>);

/// A save's body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveBody {
    pub format: String,
    pub game: fnv_game::game::GameSnapshot,
    pub engine: EngineSnapshot,
}

impl SaveBody {
    /// Whether the runtime can restore this body: every format in it, and all that the
    /// snapshots' own restores check.
    fn check(&self) -> Result<(), String> {
        if self.format != BODY_FORMAT {
            return Err(format!("body format {:?}, this NVForever reads {BODY_FORMAT}", self.format));
        }
        if self.engine.format != ENGINE_FORMAT {
            return Err(format!("engine format {:?}, this NVForever reads {ENGINE_FORMAT}", self.engine.format));
        }
        self.game.check()
    }
}

/// A save [`read_for_load`] let through that the runtime could not restore, and why:
/// nvforever starts again on the main menu and says why, as for a save refused when it was
/// read [owner 2026-11-01, #1357].
#[derive(Message, Debug, Clone)]
pub struct LoadRefused(pub String);

/// The save to apply as the game starts (nvforever `--load`).
#[derive(Resource)]
pub struct PendingLoad {
    pub path: PathBuf,
    pub body: SaveBody,
    /// The save was made with another load order ([`read_for_load`]).
    pub order: Option<OrderChange>,
}

/// How a save's load order differs from the current one, once its body was remapped.
#[derive(Debug, Clone)]
pub struct OrderChange {
    pub remap: fnv_save::PluginRemap,
    /// The save's plugins no longer loaded.
    pub missing: Vec<String>,
}

/// The engine part waiting for the runtime to be restored.
#[derive(Resource)]
pub struct PendingEngine(pub EngineSnapshot, pub PathBuf, pub Option<OrderChange>);

/// Whether the save given at the start (`--load`) is still being put back (scripts that
/// cannot run never put it back: not waited for then).
#[derive(bevy::ecs::system::SystemParam)]
pub struct Restoring<'w> {
    engine: Option<Res<'w, PendingEngine>>,
    script: Option<Res<'w, ScriptState>>,
}

impl Restoring<'_> {
    pub fn busy(&self) -> bool {
        self.engine.is_some() && self.script.as_ref().is_none_or(|s| s.off.is_none())
    }
}

/// The loading screen up now is a fast travel's (`crate::pipboy` sets it as the trip
/// leaves): the autosave after it follows the fast-travel switch.
#[derive(Resource, Default)]
pub struct FastTravelLanding(pub bool);

/// Saves asked for and not yet written (one waits while saving is not allowed).
#[derive(Resource, Default)]
struct SaveQueue(Vec<SaveRequest>);

/// The outcome of each save and load, for the console.
#[derive(Resource, Default)]
pub struct SaveLog(pub Vec<String>);

/// A save written (or failed), for the pause menu's status line.
#[derive(Message, Debug, Clone)]
pub struct SaveDone {
    pub path: PathBuf,
    pub result: Result<(), String>,
}

/// A save whose snapshot is taken, waiting for its picture.
struct Unwritten {
    path: PathBuf,
    header: SaveHeader,
    body: Vec<u8>,
    kind: SaveKind,
    refs: usize,
    t0: std::time::Instant,
    snapshot_ms: f64,
}

/// A file written on the writer thread: body and thumbnail bytes, or why not.
struct Written {
    path: PathBuf,
    kind: SaveKind,
    result: Result<(usize, usize), String>,
    refs: usize,
    t0: std::time::Instant,
    snapshot_ms: f64,
}

/// The window's picture as captured.
struct Shot {
    w: u32,
    h: u32,
    bgra: bool,
    px: Vec<u8>,
}

/// Saves between their snapshot and their file: waiting for the picture, or being written
/// on another thread.
#[derive(Resource)]
pub struct SaveWrites {
    waiting: Vec<Unwritten>,
    /// The frame the picture was asked for (the UI is hidden in it), until it comes.
    asked: Option<u32>,
    shot: Option<Shot>,
    /// UI roots hidden for the picture, and their visibility before.
    hidden: Vec<(Entity, Visibility)>,
    threads: Vec<std::thread::JoinHandle<()>>,
    in_flight: usize,
    tx: mpsc::Sender<Written>,
    rx: Mutex<mpsc::Receiver<Written>>,
}

impl Default for SaveWrites {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        SaveWrites { waiting: Vec::new(), asked: None, shot: None, hidden: Vec::new(), threads: Vec::new(), in_flight: 0, tx, rx: Mutex::new(rx) }
    }
}

impl SaveWrites {
    /// Whether a save is not on disk yet (a load waits for it).
    pub fn busy(&self) -> bool {
        !self.waiting.is_empty() || self.in_flight > 0
    }
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveConfig>()
            .init_resource::<SaveHooks>()
            .init_resource::<SaveQueue>()
            .init_resource::<SaveLog>()
            .init_resource::<SaveWrites>()
            .init_resource::<CoopEngineState>()
            .init_resource::<FastTravelLanding>()
            .add_message::<crate::wait::RestEnded>()
            .add_message::<SaveRequest>()
            .add_message::<SaveDone>()
            .add_message::<LoadRequest>()
            .add_message::<LoadRefused>()
            .add_systems(Update, (requests, autosaves).in_set(PlaySet::Ui))
            .add_systems(Update, (write_saves, take_shot, finish_writes, apply_engine).chain().after(PlaySet::Sync))
            .add_systems(Last, flush_on_exit);
        if let Some(p) = app.world_mut().remove_resource::<PendingLoad>() {
            let PendingLoad { path, body, order } = p;
            app.insert_resource(PendingEngine(body.engine, path.clone(), order));
            app.insert_resource(crate::script::PendingGame(Box::new(body.game), path));
        }
        let setting = |key, label, note, get: fn(&World) -> bool, set: fn(&mut World, bool)| crate::settings::Setting { key, group: GROUP, label, note, get, set, slider: None };
        crate::settings::add_setting(
            app,
            setting("autosave-travel", "Autosave after travel", "On: the game saves to the autosave slot after each loading screen (doors, coc, a script moving the player); fast travel has its own switch.", |w| w.resource::<SaveConfig>().autosave_travel, |w, on| w.resource_mut::<SaveConfig>().autosave_travel = on),
        );
        crate::settings::add_setting(
            app,
            setting("autosave-fast-travel", "Save on fast travel", "On: the game saves to the autosave slot when a fast travel arrives.", |w| w.resource::<SaveConfig>().autosave_fast_travel, |w, on| w.resource_mut::<SaveConfig>().autosave_fast_travel = on),
        );
        crate::settings::add_setting(
            app,
            setting("autosave-wait", "Save on wait", "On: the game saves to the autosave slot when a wait ends with at least one hour passed.", |w| w.resource::<SaveConfig>().autosave_wait, |w, on| w.resource_mut::<SaveConfig>().autosave_wait = on),
        );
        crate::settings::add_setting(
            app,
            setting("autosave-sleep", "Save on sleep", "On: the game saves to the autosave slot when the player wakes from a sleep of at least one hour.", |w| w.resource::<SaveConfig>().autosave_sleep, |w, on| w.resource_mut::<SaveConfig>().autosave_sleep = on),
        );
        crate::settings::add_setting(
            app,
            setting("autosave-scripts", "Save when a script asks", "On: a script's Autosave saves to the autosave slot, as quests do at their milestones; SystemSave to the system slot; ForceSave as a new save.", |w| w.resource::<SaveConfig>().autosave_scripts, |w, on| w.resource_mut::<SaveConfig>().autosave_scripts = on),
        );
    }
}

const GROUP: &str = "Saving";

/// The QuickSave and QuickLoad keys and the [`SaveRequest`]s.
fn requests(input: crate::bindings::ControlInput, state: Res<State<PlayState>>, mut asked: MessageReader<SaveRequest>, mut queue: ResMut<SaveQueue>, cfg: Res<SaveConfig>, mut load: MessageWriter<LoadRequest>, mut log: ResMut<SaveLog>) {
    queue.0.extend(asked.read().cloned());
    if *state.get() != PlayState::Playing {
        return;
    }
    if input.just_pressed(crate::bindings::Control::QuickSave) {
        info!("save: QuickSave key");
        queue.0.push(SaveRequest { name: fnv_save::QUICK_NAME.into(), kind: SaveKind::Quick });
    }
    if input.just_pressed(crate::bindings::Control::QuickLoad) {
        match cfg.path_of(fnv_save::QUICK_NAME) {
            Ok(p) if p.exists() => {
                info!("load: QuickLoad key: {}", p.display());
                load.write(LoadRequest { path: p });
            }
            Ok(_) => log.0.push("No quicksave yet".into()),
            Err(e) => log.0.push(e),
        }
    }
}

/// Autosaves: after a loading screen comes down, after a wait or a sleep, and on a
/// script's `Autosave`; a script's `SystemSave` and `ForceSave`.
#[allow(clippy::too_many_arguments)]
fn autosaves(
    loading: Res<crate::loading::Loading>,
    mut was: Local<bool>,
    mut fast: ResMut<FastTravelLanding>,
    mut rested: MessageReader<crate::wait::RestEnded>,
    script: Option<ResMut<ScriptState>>,
    cfg: Res<SaveConfig>,
    mut queue: ResMut<SaveQueue>,
    pending: Option<Res<PendingEngine>>,
) {
    let auto = || SaveRequest { name: fnv_save::AUTO_NAME.into(), kind: SaveKind::Auto };
    let active = loading.travelling();
    // A loading screen comes up only for travel (the start has none); not while a save
    // is still being put back.
    if *was && !active && pending.is_none() {
        let fast_travel = std::mem::take(&mut fast.0);
        if fast_travel && cfg.autosave_fast_travel {
            info!("save: autosave after fast travel");
            queue.0.push(auto());
        } else if !fast_travel && cfg.autosave_travel {
            info!("save: autosave after travel");
            queue.0.push(auto());
        }
    }
    *was = active;
    for r in rested.read() {
        if (r.sleep && cfg.autosave_sleep) || (!r.sleep && cfg.autosave_wait) {
            info!("save: autosave after {} ({} hour(s))", if r.sleep { "sleep" } else { "wait" }, r.hours);
            queue.0.push(auto());
        }
    }
    let Some(mut st) = script else { return };
    let Some(rt) = st.rt.as_mut() else { return };
    let n = std::mem::take(&mut rt.game.ui.autosaves);
    if n > 0 && cfg.autosave_scripts {
        info!("save: autosave asked by a script");
        queue.0.push(auto());
    }
    let system = std::mem::take(&mut rt.game.ui.system_saves);
    let force = std::mem::take(&mut rt.game.ui.force_saves);
    if !cfg.autosave_scripts {
        return;
    }
    if system > 0 {
        info!("save: SystemSave asked by a script");
        queue.0.push(SaveRequest { name: fnv_save::SYSTEM_NAME.into(), kind: SaveKind::System });
    }
    if force > 0 {
        let name = crate::pause::new_save_name(&cfg);
        info!("save: ForceSave asked by a script: {name:?}");
        queue.0.push(SaveRequest { name, kind: SaveKind::Manual });
    }
}

/// Why saving is refused from the player's death on (#1572).
const DEAD: &str = "the player is dead";

/// The player is dead (the death camera has the view until a load).
fn player_dead(world: &World) -> bool {
    world.get_resource::<ScriptState>().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.is_dead(fnv_sim::FormId::PLAYER_REF))
}

/// Why saving is not allowed now, if it is not.
fn not_now(world: &World) -> Option<String> {
    if world.get_resource::<crate::free_cam::FreeCam>().is_some_and(|f| f.editor) {
        return Some("playing inside the editor".into());
    }
    let st = world.get_resource::<ScriptState>()?;
    let Some(rt) = st.rt.as_ref() else { return Some("the scripts are not running yet".into()) };
    if world.resource::<crate::loading::Loading>().active() {
        return Some("a loading screen is up".into());
    }
    if player_dead(world) {
        return Some(DEAD.into());
    }
    if rt.game.dialogue.is_some() {
        return Some("in a conversation".into());
    }
    // A box answered and never read by its script is not on screen (#1649).
    if rt.game.menu.is_some() || rt.game.message_box.as_ref().is_some_and(|b| b.waiting()) {
        return Some("a script's menu or message box is open".into());
    }
    let state = *world.resource::<State<PlayState>>().get();
    let paused = world.resource::<crate::pause::Paused>().0;
    let console = world.get_resource::<crate::console::Console>().is_some_and(|c| c.open);
    let menu = world.resource::<crate::menu::ActiveMenu>().0.is_some();
    match state {
        PlayState::Playing => None,
        PlayState::Menu if (paused || console) && !menu => None,
        PlayState::Menu => Some("a menu is open".into()),
        PlayState::Dialogue => Some("in a conversation".into()),
        PlayState::Loading => Some("a loading screen is up".into()),
    }
}

fn write_saves(world: &mut World) {
    if world.resource::<SaveQueue>().0.is_empty() || world.contains_resource::<PendingEngine>() {
        return;
    }
    if let Some(why) = not_now(world) {
        // Autosaves and the system save wait for their moment (one of each); a save asked
        // for by hand says why not. A dead player's game is never saved: what waits is
        // dropped, so it is not written over the save loaded next.
        let mut q = world.resource_mut::<SaveQueue>();
        let (waits, asked): (Vec<_>, Vec<_>) = std::mem::take(&mut q.0).into_iter().partition(|r| matches!(r.kind, SaveKind::Auto | SaveKind::System));
        if why == DEAD && !waits.is_empty() {
            info!("save: {} waiting autosave(s) dropped: {why}", waits.len());
        }
        for r in waits.into_iter().filter(|_| why != DEAD) {
            if !q.0.iter().any(|k| k.kind == r.kind) {
                q.0.push(r);
            }
        }
        for r in asked {
            let line = format!("Cannot save {:?} now: {why}", r.name);
            info!("save: not now: {why}");
            world.resource_mut::<SaveLog>().0.push(line);
            // The pause menu's status line says it too, instead of "Saving..." for good.
            let path = world.resource::<SaveConfig>().path_of(&r.name).unwrap_or_default();
            world.write_message(SaveDone { path, result: Err(format!("not now: {why}")) });
        }
        return;
    }
    let reqs = std::mem::take(&mut world.resource_mut::<SaveQueue>().0);
    // One file per slot and name in a frame.
    let mut done: Vec<(SaveKind, String)> = Vec::new();
    for r in reqs {
        let key = (r.kind, r.name.clone());
        if done.contains(&key) {
            continue;
        }
        done.push(key);
        if let Err(e) = save_now(world, &r) {
            warn!("save: failed: {e}");
            world.resource_mut::<SaveLog>().0.push(format!("Save failed: {e}"));
            let path = world.resource::<SaveConfig>().path_of(&r.name).unwrap_or_default();
            world.write_message(SaveDone { path, result: Err(e) });
        }
    }
}

/// A save's game date and hour as the load list shows it (day.month.year hour:00).
pub fn game_date(h: &SaveHeader) -> String {
    format!("{:02}.{:02}.{} {:02.0}:00", h.game_day, h.game_month + 1, h.game_year, h.game_hour.floor())
}

/// The plugins of the running load order.
pub fn plugin_list(d: &crate::PlayData) -> Vec<PluginEntry> {
    d.lo.plugins.iter().map(|p| PluginEntry { name: p.name.clone(), size: p.file_size }).collect()
}

fn save_now(world: &mut World, r: &SaveRequest) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    let cfg = world.resource::<SaveConfig>().clone();
    let name = match r.kind {
        SaveKind::Quick => fnv_save::QUICK_NAME,
        SaveKind::Auto => fnv_save::AUTO_NAME,
        SaveKind::System => fnv_save::SYSTEM_NAME,
        SaveKind::Manual => r.name.as_str(),
    };
    let path = cfg.path_of(name)?;
    let mut game = world.resource_scope(|world, mut sim: Mut<SimState>| {
        let mut st = world.resource_mut::<ScriptState>();
        let rt = st.rt.as_mut().ok_or("the scripts are not running")?;
        // The extender's SaveGame event, with the save's name [documented: NVSE Event
        // Handling], before the state is taken.
        Ok::<_, String>(crate::script::with_world(rt, &mut sim, |rt| {
            rt.game.dispatch_event("SaveGame", vec![fnv_script::vm::XVal::Str(name.to_string())], None);
            rt.game.snapshot()
        }))
    })?;
    // Test aid: this save reads as one from a later version (smoke `save-refused-load`).
    if std::env::var("NVF_PLAY_TEST_SAVE_UNKNOWN_FORMAT").is_ok_and(|n| n == name) {
        game.format = "fnv.game.state/2".into();
    }
    let engine = engine_snapshot(world);
    let header = header(world, r.kind, &game, &engine);
    let refs = game.sim.refs.len();
    let body = SaveBody { format: BODY_FORMAT.into(), game, engine };
    let bytes = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
    let snapshot_ms = t0.elapsed().as_secs_f64() * 1e3;
    world.resource_mut::<SaveWrites>().waiting.push(Unwritten { path, header, body: bytes, kind: r.kind, refs, t0, snapshot_ms });
    Ok(())
}

/// Ask for the picture of this frame, drawn without the UI, for the saves just taken.
fn take_shot(world: &mut World) {
    let w = world.resource::<SaveWrites>();
    if w.waiting.is_empty() || w.asked.is_some() {
        return;
    }
    let frame = world.resource::<bevy::diagnostic::FrameCount>().0;
    // The root nodes drawn on the window (not the Pip-Boy's screen, drawn by its own camera).
    let mut roots = world.query_filtered::<(Entity, &Visibility), (With<Node>, Without<ChildOf>, Without<bevy::ui::UiTargetCamera>)>();
    let hidden: Vec<(Entity, Visibility)> = roots.iter(world).filter(|(_, v)| **v != Visibility::Hidden).map(|(e, v)| (e, *v)).collect();
    for (e, _) in &hidden {
        world.entity_mut(*e).insert(Visibility::Hidden);
    }
    let mut w = world.resource_mut::<SaveWrites>();
    w.hidden = hidden;
    w.asked = Some(frame);
    w.shot = None;
    world.spawn(Screenshot::primary_window()).observe(|ev: On<ScreenshotCaptured>, mut w: ResMut<SaveWrites>| {
        let img = &ev.image;
        let bgra = match img.texture_descriptor.format {
            TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => true,
            TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => false,
            f => {
                warn!("save: the window's picture is {f:?}; no thumbnail");
                return;
            }
        };
        if let Some(px) = img.data.clone() {
            w.shot = Some(Shot { w: img.width(), h: img.height(), bgra, px });
        }
    });
}

/// Show the UI again the frame after the picture; once the picture is here (or did not
/// come in time) hand the saves to a writer thread; report the files written.
fn finish_writes(world: &mut World) {
    let frame = world.resource::<bevy::diagnostic::FrameCount>().0;
    let mut w = world.resource_mut::<SaveWrites>();
    let shown_again = if w.asked.is_none_or(|f| frame > f) { std::mem::take(&mut w.hidden) } else { Vec::new() };
    let ready = w.asked.is_some_and(|f| w.shot.is_some() || frame.saturating_sub(f) > SHOT_WAIT_FRAMES);
    if ready {
        let shot = w.shot.take();
        if shot.is_none() {
            warn!("save: no picture of the window within {SHOT_WAIT_FRAMES} frames; saving without a thumbnail");
        }
        w.asked = None;
        let batch = std::mem::take(&mut w.waiting);
        start_writer(&mut w, batch, shot);
    }
    for (e, v) in shown_again {
        if let Ok(mut ent) = world.get_entity_mut(e) {
            ent.insert(v);
        }
    }
    report_written(world);
}

fn start_writer(w: &mut SaveWrites, batch: Vec<Unwritten>, shot: Option<Shot>) {
    if batch.is_empty() {
        return;
    }
    w.in_flight += batch.len();
    let tx = w.tx.clone();
    w.threads.retain(|t| !t.is_finished());
    w.threads.push(std::thread::spawn(move || {
        let png = shot.and_then(|s| thumbnail_rgb(s.w, s.h, &s.px, s.bgra)).and_then(|rgb| encode_png(&rgb, THUMB_W, THUMB_H)).unwrap_or_default();
        for u in batch {
            let result = fnv_save::write(&u.path, &u.header, &png, &u.body, u.kind != SaveKind::Manual).map(|()| (u.body.len(), png.len())).map_err(|e| e.to_string());
            let _ = tx.send(Written { path: u.path, kind: u.kind, result, refs: u.refs, t0: u.t0, snapshot_ms: u.snapshot_ms });
        }
    }));
}

/// Log the files the writer thread finished, for the console, the corner and the pause menu.
fn report_written(world: &mut World) {
    let done: Vec<Written> = {
        let w = world.resource::<SaveWrites>();
        let rx = w.rx.lock().expect("the writer channel");
        rx.try_iter().collect()
    };
    for d in done {
        world.resource_mut::<SaveWrites>().in_flight -= 1;
        let line = match &d.result {
            Ok((bytes, shot)) => {
                let kind = format!("{:?}", d.kind).to_lowercase();
                let thumb = if *shot > 0 { format!("thumbnail {THUMB_W}x{THUMB_H} ({shot} bytes PNG)") } else { "no thumbnail".into() };
                info!("save: wrote {} ({kind}, {bytes} bytes, {} changed references), {thumb} in {:.0} ms (snapshot {:.0} ms)", d.path.display(), d.refs, d.t0.elapsed().as_secs_f64() * 1e3, d.snapshot_ms);
                if let Some(mut c) = world.get_resource_mut::<crate::corner_messages::CornerMessages>() {
                    c.push(if d.kind == SaveKind::Auto { "Autosaved".to_string() } else { "Game saved".to_string() }.into());
                }
                format!("Saved {}", d.path.display())
            }
            Err(e) => {
                warn!("save: failed: {e}");
                format!("Save failed: {e}")
            }
        };
        world.resource_mut::<SaveLog>().0.push(line);
        world.write_message(SaveDone { path: d.path, result: d.result.map(|_| ()) });
    }
}

/// Quitting (or starting again for a load) while saves are not on disk: write those still
/// waiting for their picture without one and wait for the writer threads.
fn flush_on_exit(world: &mut World) {
    if world.resource::<Messages<AppExit>>().is_empty() || !world.resource::<SaveWrites>().busy() {
        return;
    }
    let mut w = world.resource_mut::<SaveWrites>();
    let batch = std::mem::take(&mut w.waiting);
    w.asked = None;
    start_writer(&mut w, batch, None);
    let threads = std::mem::take(&mut w.threads);
    for t in threads {
        let _ = t.join();
    }
    report_written(world);
}

/// The thumbnail's RGB pixels from the window's picture (`px` 4 bytes a pixel, BGRA or
/// RGBA, alpha left out): the middle 16:9 of it, each pixel the average of the area it
/// covers.
fn thumbnail_rgb(w: u32, h: u32, px: &[u8], bgra: bool) -> Option<Vec<u8>> {
    let (w, h) = (w as usize, h as usize);
    if w == 0 || h == 0 || px.len() < w * h * 4 {
        return None;
    }
    let (cw, ch) = if w * 9 > h * 16 { (h * 16 / 9, h) } else { (w, w * 9 / 16) };
    let (x0, y0) = ((w - cw) / 2, (h - ch) / 2);
    let (tw, th) = (THUMB_W as usize, THUMB_H as usize);
    // The source span of output pixel `i` of `n` over `len` source pixels, at least one wide.
    let span = |i: usize, n: usize, len: usize, at: usize, max: usize| {
        let a = at + i * len / n;
        (a, (at + (i + 1) * len / n).max(a + 1).min(max))
    };
    let mut out = Vec::with_capacity(tw * th * 3);
    for ty in 0..th {
        let (sy0, sy1) = span(ty, th, ch, y0, h);
        for tx in 0..tw {
            let (sx0, sx1) = span(tx, tw, cw, x0, w);
            let mut sum = [0u32; 3];
            for y in sy0..sy1 {
                for p in px[(y * w + sx0) * 4..(y * w + sx1) * 4].as_chunks::<4>().0 {
                    for (s, v) in sum.iter_mut().zip(p) {
                        *s += u32::from(*v);
                    }
                }
            }
            let n = ((sy1 - sy0) * (sx1 - sx0)) as u32;
            let avg = sum.map(|v| ((v + n / 2) / n) as u8);
            out.extend_from_slice(&if bgra { [avg[2], avg[1], avg[0]] } else { avg });
        }
    }
    Some(out)
}

fn encode_png(rgb: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, w, h);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let mut wr = e.write_header().ok()?;
    wr.write_image_data(rgb).ok()?;
    wr.finish().ok()?;
    Some(out)
}

/// A save's thumbnail as RGBA pixels (width, height, pixels), for the load menu.
pub fn decode_thumbnail(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes));
    d.set_transformations(png::Transformations::EXPAND);
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba = match (info.color_type, info.bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => buf,
        (png::ColorType::Rgb, png::BitDepth::Eight) => buf.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        _ => return None,
    };
    Some((info.width, info.height, rgba))
}

fn engine_snapshot(world: &World) -> EngineSnapshot {
    let mut e = EngineSnapshot { format: ENGINE_FORMAT.into(), ..default() };
    if let Some(c) = world.get_resource::<crate::control::PlayerControl>() {
        e.third_person = c.pov == crate::control::Pov::Third;
        e.distance = c.distance;
        e.sneak = c.sneak;
        e.always_run = c.always_run;
    }
    e.flashlight = world.get_resource::<crate::flashlight::Flashlight>().is_some_and(|f| f.on);
    e.magazine_ammo = world.get_resource::<crate::combat::Combat>().and_then(|c| c.magazine_ammo_for_save());
    e.magazine = world.get_resource::<crate::combat::Combat>().and_then(|c| c.magazine_for_save());
    let game = world.get_resource::<ScriptState>().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    e.active_quest = match world.get_resource::<crate::pipboy::data::DataState>() {
        Some(d) => d.active_quest(game),
        None => game.and_then(|g| g.ui.active_quest).map(|q| q.0),
    };
    e.coop = world.get_resource::<CoopEngineState>().map(|c| c.0.clone()).unwrap_or_default();
    let hooks = world.resource::<SaveHooks>().0.clone();
    for h in hooks {
        if let Some(v) = (h.save)(world) {
            e.extra.insert(h.key.to_string(), v);
        }
    }
    e
}

fn header(world: &World, kind: SaveKind, game: &fnv_game::game::GameSnapshot, engine: &EngineSnapshot) -> SaveHeader {
    let mut h = SaveHeader::new(kind);
    let d = &world.resource::<PlayWorld>().0;
    let sim = world.resource::<SimState>();
    h.plugins = plugin_list(d);
    h.play_seconds = game.time;
    if let Ok(c) = sim.world.calendar() {
        (h.game_year, h.game_month, h.game_day, h.game_hour) = (c.year as u32, c.month as u32, c.day as u32, c.hour);
        h.game_days_passed = Some(c.days_passed);
    }
    h.player_name = game.player.name.clone().unwrap_or_else(|| "Courier".into());
    if let Some(rt) = world.resource::<ScriptState>().rt.as_ref() {
        h.level = rt.game.player_level().max(1.0) as u32;
    }
    let loc = world.get_resource::<Location>().copied();
    let player = world.get_resource::<crate::player::Player>();
    let feet = sim.world.peek(fnv_sim::FormId::PLAYER_REF).map(|s| s.pos).or(player.map(|p| p.feet)).unwrap_or_default();
    h.place = fnv_save::Place {
        world: loc.map(|l| l.world),
        interior: loc.and_then(|l| l.interior),
        feet: player.map_or(feet, |p| p.feet),
        heading_deg: player.map_or(0.0, |p| p.heading.to_degrees()),
        pitch_deg: player.map_or(0.0, |p| p.pitch.to_degrees()),
    };
    let cell = loc.and_then(|l| l.interior).or_else(|| sim.world.peek(fnv_sim::FormId::PLAYER_REF).and_then(|s| s.cell).map(|c| c.0));
    h.location = cell.map(|c| crate::console::place_name(d, c)).unwrap_or_default();
    h.coop = game
        .coop
        .iter()
        .map(|p| fnv_save::CoopEntry { slot: p.slot, name: p.character.name.clone().unwrap_or_else(|| format!("Player {}", p.slot)), level: p.character.level.unwrap_or(game.sim.config.player_level as i32).max(1) as u32 })
        .collect();
    h.weather = engine.extra.get("sky").and_then(|v| v.get("weather")).and_then(|w| w.as_str()).map(str::to_string);
    h
}

/// Put back the engine's part once the runtime holds the restored game.
fn apply_engine(world: &mut World) {
    if !world.contains_resource::<PendingEngine>() {
        return;
    }
    if world.get_resource::<ScriptState>().is_none_or(|s| s.rt.is_none() || s.pending_game_waiting()) {
        return;
    }
    let Some(PendingEngine(e, path, order)) = world.remove_resource::<PendingEngine>() else { return };
    world.insert_resource(CoopEngineState(e.coop.clone()));
    if let Some(mut c) = world.get_resource_mut::<crate::control::PlayerControl>() {
        c.pov = if e.third_person { crate::control::Pov::Third } else { crate::control::Pov::First };
        if e.distance > 0.0 {
            c.distance = e.distance;
        }
        c.sneak = e.sneak;
        c.always_run = e.always_run;
    }
    if let Some(mut f) = world.get_resource_mut::<crate::flashlight::Flashlight>() {
        f.on = e.flashlight;
    }
    if let (Some(m), Some(mut c)) = (e.magazine, world.get_resource_mut::<crate::combat::Combat>()) {
        c.restore_magazine(m, e.magazine_ammo);
    }
    if let Some(q) = e.active_quest {
        crate::pipboy::data::DataState::restore_active_quest(world, q);
    }
    let hooks = world.resource::<SaveHooks>().0.clone();
    for h in hooks {
        if let Some(v) = e.extra.get(h.key) {
            let mut v = v.clone();
            if let (Some(remap), Some(o)) = (h.remap, &order) {
                remap(&mut v, &o.remap);
            }
            (h.load)(world, &v);
        }
    }
    info!("load: engine state of {} put back ({} part(s) from other modules)", path.display(), e.extra.len());
    world.resource_mut::<SaveLog>().0.push(format!("Loaded {}", path.display()));
    // Plugins the save was made with that are not loaded: said in a corner message as the
    // game comes up (our wording [hypothesis]: what the original shows is not documented
    // and its executable's strings cannot be read).
    if let Some(o) = order.filter(|o| !o.missing.is_empty()) {
        let text = missing_content_text(&o.missing);
        world.resource_mut::<SaveLog>().0.push(text.clone());
        if let Some(mut c) = world.get_resource_mut::<crate::corner_messages::CornerMessages>() {
            c.push(crate::script_events::Note { text, icon: None, seconds: Some(6.0) });
        }
    }
}

/// What the player is told when a save's plugins are missing (design §4.1).
pub fn missing_content_text(missing: &[String]) -> String {
    format!("This save relies on content that is no longer present: {}. What came from it was removed.", missing.join(", "))
}

/// Read a save for `--load`: its header (to start the world at its place) and its body,
/// refused here when the runtime could not restore it ([`SaveBody::check`]), before anything
/// starts from it. A save made with another load order is remapped by plugin name (design §4.1): every
/// FormID's mod index moves to its plugin's place now; what came from plugins no longer
/// loaded is dropped and reported. Called before the log is set up: its lines go straight
/// to the output, as nvforever's first `load:` line.
pub fn read_for_load(path: &std::path::Path, current: &[PluginEntry]) -> Result<(SaveHeader, SaveBody, Option<OrderChange>), String> {
    let f = fnv_save::read(path).map_err(|e| e.to_string())?;
    let remap = fnv_save::PluginRemap::new(&f.header.plugins, current);
    let body_of = |v: serde_json::Value| -> Result<SaveBody, String> {
        let body: SaveBody = serde_json::from_value(v).map_err(|e| format!("{}: {e}", path.display()))?;
        body.check().map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(body)
    };
    let raw: serde_json::Value = serde_json::from_slice(&f.body).map_err(|e| format!("{}: {e}", path.display()))?;
    if remap.is_identity() {
        if let fnv_save::PluginCheck::Appended { added } = fnv_save::check_plugins(&f.header.plugins, current) {
            eprintln!("load: plugins added since the save, after its own: {}", added.join(", "));
        }
        return Ok((f.header, body_of(raw)?, None));
    }
    let missing = remap.missing();
    if !missing.is_empty() && current.len() > fnv_save::GONE_INDEX as usize {
        return Err(format!("{} was made with plugins that are not loaded now ({missing:?}) and the load order has 255 plugins: unload one to load it", path.display()));
    }
    let added: Vec<&str> = current.iter().filter(|c| !f.header.plugins.iter().any(|p| p.name.eq_ignore_ascii_case(&c.name))).map(|c| c.name.as_str()).collect();
    let moved = remap.moved().iter().map(|(n, a, b)| format!("{n} {a:02X}->{b:02X}")).collect::<Vec<_>>();
    let mut raw = raw;
    let report = fnv_save::remap_json(&mut raw, &remap);
    let mut body = body_of(raw)?;
    let gone = |id: fnv_sim::FormId| fnv_save::is_gone(id.0);
    let dropped_game = body.game.drop_gone(gone);
    let e = &mut body.engine;
    e.magazine = e.magazine.and_then(|(w, n)| remap.map(w).map(|w| (w, n)));
    e.magazine_ammo = e.magazine_ammo.and_then(|a| remap.map(a));
    e.active_quest = e.active_quest.and_then(|q| remap.map(q));
    for c in &mut e.coop {
        c.magazine = c.magazine.and_then(|(w, n)| remap.map(w).map(|w| (w, n)));
    }
    eprintln!(
        "load: {} was made with another load order: moved {:?}, missing now {:?}, added {:?}; {} FormIDs remapped, {} entries of missing plugins dropped ({:?}), {} fields named a missing plugin's form, {} entries holding them dropped by the game layer",
        path.display(),
        moved,
        missing,
        added,
        report.moved,
        report.dropped_total(),
        report.dropped,
        report.gone_total(),
        dropped_game,
    );
    if !missing.is_empty() {
        eprintln!("load: {}", missing_content_text(&missing));
    }
    Ok((f.header, body, Some(OrderChange { remap, missing })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_snapshot_without_new_fields_still_reads() {
        let e: EngineSnapshot = serde_json::from_str(r#"{"format":"nvf.engine.state/1"}"#).unwrap();
        assert_eq!(e, EngineSnapshot { format: ENGINE_FORMAT.into(), ..default() });
        let full = EngineSnapshot { format: ENGINE_FORMAT.into(), third_person: true, distance: 150.0, sneak: true, always_run: true, flashlight: true, magazine: Some((0x1234, 7)), magazine_ammo: Some(0x2345), active_quest: Some(0x104C1C), extra: [("sky".to_string(), serde_json::json!({"weather": "NVWastelandClear"}))].into(), coop: vec![CoopEngine { slot: 3, third_person: true, distance: 90.0, sneak: true, always_run: false, flashlight: true, magazine: Some((0x1235, 2)) }] };
        let back: EngineSnapshot = serde_json::from_str(&serde_json::to_string(&full).unwrap()).unwrap();
        assert_eq!(back, full);
        // One player writes no `coop`.
        assert!(!serde_json::to_string(&EngineSnapshot { format: ENGINE_FORMAT.into(), ..default() }).unwrap().contains("coop"));
    }

    #[test]
    fn the_thumbnail_is_the_middle_16_9_scaled_down() {
        // A 4:3 picture, 1024x768, BGRA: the bands above and below 16:9 (green) are cut;
        // red on the left half, blue on the right.
        let (w, h) = (1024u32, 768u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let c = if !(96..672).contains(&y) { [0, 255, 0] } else if x < w / 2 { [0, 0, 255] } else { [255, 0, 0] };
                px[i..i + 3].copy_from_slice(&c);
                px[i + 3] = 7;
            }
        }
        let rgb = thumbnail_rgb(w, h, &px, true).unwrap();
        assert_eq!(rgb.len(), (THUMB_W * THUMB_H * 3) as usize);
        let at = |x: u32, y: u32| rgb[((y * THUMB_W + x) * 3) as usize..][..3].to_vec();
        assert_eq!(at(0, 0), [255, 0, 0]);
        assert_eq!(at(THUMB_W - 1, THUMB_H - 1), [0, 0, 255]);
        let png = encode_png(&rgb, THUMB_W, THUMB_H).unwrap();
        let (tw, th, rgba) = decode_thumbnail(&png).unwrap();
        assert_eq!((tw, th), (THUMB_W, THUMB_H));
        assert_eq!(&rgba[..4], [255, 0, 0, 255]);
        assert!(thumbnail_rgb(10, 10, &px[..8], true).is_none());
        assert!(decode_thumbnail(b"not a png").is_none());
    }

    #[test]
    fn a_save_name_or_a_path_is_taken() {
        let cfg = SaveConfig::with_dir(Some(PathBuf::from("saves")));
        assert_eq!(cfg.path_of("Before Primm").unwrap(), PathBuf::from("saves").join("Before Primm.nvfsave"));
        assert_eq!(cfg.path_of("C:/x/y.nvfsave").unwrap(), PathBuf::from("C:/x/y.nvfsave"));
        assert!(cfg.path_of("bad:name").is_err());
    }

    /// A change that spoils a save's body.
    type BodyChange = fn(&mut SaveBody);

    /// The restore's own refusals come at the read, before the world starts at the save's
    /// place, and the player is told why (#1590).
    #[test]
    fn a_save_whose_snapshots_cannot_be_restored_is_refused_as_it_is_read() {
        let st = ScriptState::over_no_plugins();
        let good = SaveBody { format: BODY_FORMAT.into(), game: st.rt.as_ref().unwrap().game.snapshot(), engine: EngineSnapshot { format: ENGINE_FORMAT.into(), ..default() } };
        let dir = std::env::temp_dir().join(format!("nvf-refused-save-{}", std::process::id()));
        let read = |name: &str, body: &SaveBody| {
            let path = dir.join(format!("{name}.nvfsave"));
            fnv_save::write(&path, &SaveHeader::new(SaveKind::Manual), &[], &serde_json::to_vec(body).unwrap(), false).unwrap();
            read_for_load(&path, &[]).map(|_| ())
        };
        assert_eq!(read("good", &good), Ok(()));
        let changed: [(&str, BodyChange, &str); 6] = [
            ("body", |b| b.format = "nvf.save.body/2".into(), "body format"),
            ("engine", |b| b.engine.format = "nvf.engine.state/2".into(), "engine format"),
            ("game", |b| b.game.format = "fnv.game.state/2".into(), "unsupported snapshot format \"fnv.game.state/2\""),
            ("sim", |b| b.game.sim.format = "fnv.sim.state/2".into(), "unsupported snapshot format \"fnv.sim.state/2\""),
            ("game-rng", |b| b.game.rng_word_pos = "x".into(), "bad rng_word_pos"),
            ("sim-rng", |b| b.game.sim.rng_word_pos = "y".into(), "bad rng_word_pos"),
        ];
        for (name, change, why) in changed {
            let mut bad = good.clone();
            change(&mut bad);
            let e = read(name, &bad).unwrap_err();
            assert!(e.contains(why) && e.contains(&format!("{name}.nvfsave")), "{name}: {e}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A restore refused late leaves no runtime: the engine part stays unapplied, its
    /// success line unwritten, and no save is taken of what is there (#1590).
    #[test]
    fn with_no_runtime_a_pending_save_puts_nothing_back_and_saves_nothing() {
        let mut world = World::new();
        world.init_resource::<SaveHooks>();
        world.init_resource::<SaveLog>();
        world.init_resource::<SaveWrites>();
        let mut st = ScriptState::over_no_plugins();
        st.rt = None;
        world.insert_resource(st);
        world.insert_resource(SaveQueue(vec![SaveRequest { name: "after".into(), kind: SaveKind::Manual }]));
        world.insert_resource(PendingEngine(EngineSnapshot { format: ENGINE_FORMAT.into(), ..default() }, PathBuf::from("refused.nvfsave"), None));
        apply_engine(&mut world);
        write_saves(&mut world);
        assert!(world.contains_resource::<PendingEngine>());
        assert!(world.resource::<SaveLog>().0.is_empty());
        assert_eq!(world.resource::<SaveQueue>().0.len(), 1);
        assert!(!world.resource::<SaveWrites>().busy());
    }
}
