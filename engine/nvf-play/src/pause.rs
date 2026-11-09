//! The pause menu: Esc in the world opens it instead of quitting (the original opens its
//! pause menu there [observed by the coordinator playing the original, 2026-10-09]). It is
//! a `PlayState::Menu` like the container and inventory menus, so the world, the game
//! clock and mouse look stop and the cursor is released (`crate::menu`).
//!
//! **The look is the original's** (`crate::start_menu`, the main menu shares it) [owner,
//! todo 5 item 8; observed: the owner's screenshots of the original's pause menu, its
//! settings pages and its quit question, C:\fnv-private\Pause Menu; start_menu.xml]: the
//! pause screen over the held world, the column of entries at the right (Continue, Save,
//! Load, Settings, Quit; the original's Help is left out), the highlight box round the
//! chosen one. A page opened from the column dims the column but for its own entry; a
//! click on that entry again closes the page (back to the column), and a click on Quit
//! while its question is up cancels it [owner 2026-10-16: the entries toggle their page].
//! Up/Down choose, Enter or Space take, a click takes, the pointer chooses the entry it
//! moves onto; Esc resumes; N saves, L opens the load page, S the settings, Q the quit
//! question.
//!
//! **Save** (N) saves to a new slot named `Save <n>` (one more than the highest such name
//! in the saves folder) through `crate::save::SaveRequest`; the line under the column
//! says when it is written. **The load page** (L) shows the chosen save's thumbnail and
//! details (player and level, place and game date, play time, when it was saved) over the
//! saves' rows, newest first (`fnv_save::list`), left of the column. Up/Down (W/S) or the
//! mouse wheel choose, the pointer chooses the row it moves onto, Enter, Space or a click
//! load it through `crate::save::LoadRequest` (nvforever starts again with `--load`); Esc,
//! Backspace or Back go back. Only the chosen save's thumbnail is read and decoded.
//! **Deleting a save** (2026-10-19; docs/design/save-load.md §6.1): Delete (or a click
//! on "Delete  Del)" right of the thumbnail) asks "Delete this save?" there, with "Yes
//! Y)" and "No  N)"; Y, Enter or a click on Yes deletes the chosen save's file from the
//! saves folder and lists the saves again; N, Esc, Backspace or a click on No keep it
//! [documented: the original's load and save menus have a Delete hint,
//! `load_and_save_menus.xml` `save_delete_text`, shown with a controller; design: the key,
//! the question and its place]. Log `pause menu: delete <name>?`, `pause menu: deleted
//! <path>`, `pause menu: delete cancelled`.
//!
//! **The settings page** lists the on/off switches of [`crate::settings`] the original's
//! way: their groups as categories left of the column (Gameplay, World, Display,
//! Graphics, Debug: NVForever's switches, not the original's), the chosen category's
//! switches in the middle over the darkened band (from near the view's top, as far left
//! as keeps the band clear of the categories: `options_left`, #1210), each with ON or
//! OFF as the game is using it now, the chosen switch's note under them (cut to the lines
//! that fit), Defaults and Back at the bottom
//! right, the version at the bottom left. Up/Down (or W/S) choose a row within the
//! category, Tab or Page Down the next category (Page Up the one before), a click on a
//! category shows it; Enter, Space, Left or Right flip the row, as does a click on it; a
//! flip acts at once and is kept in `NVForever.ini` (`crate::settings`). A slider's row
//! shows a meter as the "How many?" prompt's (`crate::menu::look`: a ground at a quarter
//! of the colour, tick marks 8 apart as far as the value, an arrow at each end) and the
//! value as a percentage; Left/Right or the arrows move it a step, a press or drag on the
//! meter sets the value under the pointer, and it is kept in the INI too. Defaults puts
//! back every row as the command line left it before the INI was read
//! (`crate::settings::StartStates`; without it, as the page was first shown in this run).
//! Esc or Backspace go back.
//!
//! A list's row (the resolution, `crate::settings::ChoiceList`) shows its choice in a box
//! with a down arrow, as a dropdown [design; owner 2026-10-18]: Enter, Space or a click
//! open the list under it (over it near the bottom), the chosen one boxed; Up/Down (W/S),
//! the wheel and the pointer choose, Enter, Space or a click take, Esc, Backspace or a
//! click outside close it. Left/Right on the row take the choice before or after without
//! opening it. A choice that asks to be kept puts a question over the page: "Keep this
//! resolution?" with the seconds left and Keep / Revert (Left/Right/Up/Down choose, Enter
//! or Space take, Esc reverts, a click takes); unanswered it goes back by itself
//! (`crate::settings::KEEP_SECS`).
//!
//! **The Controls tab**, the settings page's last category [design; owner 2026-10-16:
//! keys can be mapped from the settings], lists every control of `crate::bindings` with
//! its key and mouse button. Up/Down (W/S) choose; Enter, Space or a click on a row make
//! it wait for its new key ("Press a key..."): the next key or mouse button pressed is
//! bound (`crate::bindings::rebind`: a control that had it gets the old one), Esc
//! cancels. Delete clears the row's key; Defaults puts back the game's keys. Each change
//! acts at once and is kept in `[Controls]` of `NVForever.ini`. Tab after the last
//! category goes to Controls and from there to the first again. Its first row, Input
//! Device, switches between the keyboard and mouse and a controller (`crate::pad`);
//! with Controller chosen the list shows and rebinds each control's controller button
//! (kept in `[Controller]`), a row waiting for the next button pressed ("Press a
//! button..."), Start cancelling. After the controls, Invert Y turns the vertical look
//! upside down, and a slider as the settings page's sets how fast the view turns as a
//! percentage of the game's: Mouse Sensitivity with keyboard and mouse
//! (`fMouseSensitivity`), Stick Sensitivity with a controller (the right stick's look
//! speeds), each kept apart (Left/Right or the arrows a step of `crate::bindings::SENSITIVITY_STEPS`,
//! about a tenth apart, a press or drag on the logarithmic meter the value under the
//! pointer; `crate::bindings::SENSITIVITY_LEAST`..`SENSITIVITY_MOST`, 5%-300%)
//! [design: owner 2026-11-03, #1451; #1632]; with a controller Rumble comes last.
//!
//! **Quit** asks first [observed: the original's question and its choices]: Main Menu
//! (nvforever starts again with `--menu`, [`MainMenuRequest`]), Exit Game, Cancel.
//!
//! **Mods** (issue #976, T11; `crate::tiles::mods`): with `[UI] TileMenus=StartMenu` and a
//! mod's tiles in the start menu (MCM's include), a Mods entry stands before Quit
//! ([`ModsEntry`]). Its page draws the mod's tiles from the tree over this menu, gives them
//! the pointer and keys, and runs the start menu's MenuMode scripts each frame; Esc,
//! Backspace or Back go back to the column [design: owner-review.md #34]. Off by default.
//!
//! **What a menu holds** (docs/design/dialogue.md §8, Pausing). Every menu holds game
//! logic by its own means (`PlayState`, the runtime's menu mode). Whether the picture
//! stands still too is one switch for all of them, [`MenuPause`] (`nvforever
//! --menu-pause living|original`, default `living`): the dialogue screen, the inventory
//! and container menus, a script's message boxes and the menus scripts open [owner
//! decision 2026-10-10]. `original` pauses Bevy's virtual clock while such a menu is
//! open ([`freeze_picture`]), so animations, wind, water and clouds stand still.
//! **The pause menu is the exception: it always holds everything**, in both modes: the
//! picture (the virtual clock), scripts (`script.rs` does not tick under it) and sound
//! (`voice.rs` pauses the lines being said and resumes them).
//!
//! While the virtual clock stands still, what must go on reads [`AidTime`]: the timed
//! test aids, the menus' own timers and the scripts' frame time (MenuMode blocks and a
//! conversation's lines run in real time under an `original` menu). `nvforever
//! --run-seconds` counts real time then as well, so an unattended run always ends. The
//! test aids that act on the world can count from the scripts' first frame instead
//! ([`WorldAidClock`], `NVF_PLAY_AIDS_FROM_SCRIPTS=1`; which aids: `crate::test_aid`).
//!
//! Test aid `NVF_PLAY_TEST_PAUSE=<seconds>:<seconds>[,…]`: opens the pause menu at the
//! first time (as Esc) and resumes at the second (as Esc again, from either page), on
//! the aids' clock. `NVF_PLAY_TEST_SETTINGS=<seconds>:<key>[,…]`: at each time, with the
//! pause menu open, shows the settings page and flips the switch of that key (as a
//! click on its row); `<key>=<value>` sets a slider (e.g. `actor-distance=0.5`) or takes a
//! list's choice (`resolution=1024x768`, as a click on it in the open list); the key
//! `page` only shows the page, `controls` the Controls tab, and `bind:<control>=<input>`
//! (INI names, e.g. `bind:Jump=J`) binds a control there as Enter on its row and that key
//! would, `padbind:<control>=<button>` (`padbind:Jump=RB`) a controller button,
//! `device:<name>` (`device:Controller`) picks the input device, `rumble:1` or `rumble:0`
//! sets the controller's Rumble row, `invert:1` or `invert:0` the Invert Y row and
//! `sensitivity:<percent>` (`sensitivity:150`) the sensitivity slider of the device in use.
//! `NVF_PLAY_TEST_PAUSE_MENU=<seconds>:<action>[,…]`: with the pause menu open, `save` (the
//! Save button), `load` (the load page), `down`, `up`, `pick` (Enter on the load page, the
//! quit question, a list's row or its open dropdown, where `down` and `up` move too),
//! `back` (from the load or the settings page, the question or an open dropdown),
//! `defaults` (the settings page's Defaults), `wheel-down`, `wheel-up` (a notch of the
//! mouse wheel on the settings page or the Controls tab),
//! `keep` and `revert` (the keep question's answers),
//! `quit` (the question), `delete`, `yes` and `no` (the load page's delete question),
//! `mods` (the Mods page, when the column has its entry: [`ModsEntry`]),
//! `shot` (a screenshot `pause_<page>.png` in the working folder).

use crate::interface::{bc_supported, u, Face, Picture, Pictures};
use crate::settings::{add_setting, Setting, Settings};
use fnv_util::words::tr;
use crate::start_menu::{color, highlight_colors, text as sm_text, Faces, Shade, BASE, COLUMN, ITEM, LINE, RIGHT, SUB_RIGHT, SUB_WIDTH};
use crate::{PlaySet, PlayState};
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use crate::ui_pointer::{Button, PointerState};
use bevy::ui::RelativeCursorPosition;
use fnv_save::{SaveHeader, SaveKind};
use nvf_assets::GameData;
use std::path::{Path, PathBuf};

/// What an open menu holds besides game logic (`nvforever --menu-pause`; the old name
/// `--dialogue-pause` still works). Not the pause menu's business: that one always holds
/// everything.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuPause {
    /// Only game logic stands still; the picture lives on.
    #[default]
    Living,
    /// The virtual clock stops: animations, wind, water and sky stand still too.
    Original,
}

/// Whether the picture stands still: always under the pause menu and under a loading
/// screen (`crate::loading`: nothing happens in the world behind it); under any other
/// menu (every state but `Playing`) with [`MenuPause::Original`].
///
/// The loading screen's own pause of the clock once lasted a frame in `living`: the pause
/// menu or the main menu closing under it (a load from them) cleared [`Paused`], so this
/// let the clock run again, and the world behind the screen went on while the next run
/// loaded the save — from the main menu, the new game's opening in Doc Mitchell's house,
/// heard as his footsteps and his sitting down [owner 2026-10-23].
pub fn picture_held(mode: MenuPause, paused: bool, state: PlayState) -> bool {
    paused || state == PlayState::Loading || (mode == MenuPause::Original && state != PlayState::Playing)
}

/// Real time spent while the virtual clock stood still, and this frame's share of it.
#[derive(Resource, Default, Debug)]
pub struct PlayClock {
    stopped: f32,
    frame: Option<f32>,
}

impl PlayClock {
    /// Real seconds spent so far while the virtual clock stood still.
    pub fn stopped_secs(&self) -> f32 {
        self.stopped
    }

    /// A clock whose frame stood still for `secs` of real time (tests).
    #[cfg(test)]
    pub(crate) fn frozen_frame(secs: f32) -> PlayClock {
        PlayClock { stopped: 0.0, frame: Some(secs) }
    }
}

/// The clock of what goes on while a menu holds the picture: timed test aids, menu
/// timers, the scripts' frame time.
#[derive(SystemParam)]
pub struct AidTime<'w> {
    time: Res<'w, Time>,
    clock: Res<'w, PlayClock>,
    world: Option<Res<'w, WorldAidClock>>,
}

impl AidTime<'_> {
    /// Seconds since the start: the virtual clock's, plus the real time it stood still.
    pub fn elapsed_secs(&self) -> f32 {
        self.time.elapsed_secs() + self.clock.stopped
    }

    /// The clock of the test aids that act on the world ([`WorldAidClock`]): the aids'
    /// clock, or, with `NVF_PLAY_AIDS_FROM_SCRIPTS=1`, `None` until the scripts first run
    /// and then the aids' clock less what the scripts' late start held it back.
    pub fn world_secs(&self) -> Option<f32> {
        let now = self.elapsed_secs();
        self.world.as_deref().map_or(Some(now), |w| w.at(now))
    }

    /// The frame's time: the virtual clock's, or real time (at most 0.25 s, the virtual
    /// clock's own cap) while it stands still.
    pub fn delta_secs_f64(&self) -> f64 {
        self.clock.frame.map_or_else(|| self.time.delta_secs_f64(), f64::from)
    }
}

/// When the scripts start on a machine that is not busy, at the latest (seconds on the
/// aids' clock) [observed: the desktop's smoke logs of 2026-10-15/16 load the script data
/// in 4.2–6.5 s ("scripts ready in"); the scripts run from the first frame after it in
/// which the player stands in the world; busy runs started them at 8.7 s and 9.5 s].
/// Up to this the world aids' clock is not held back, so a scenario's times mean what
/// they meant before the switch.
pub const SCRIPTS_START_SECONDS: f32 = 6.5;

/// `NVF_PLAY_AIDS_FROM_SCRIPTS=1` (the smoke runner sets it for `--play` runs): the test
/// aids that act on the world count from when the scripts first run, not from the start,
/// so on a busy machine (the script data loads late) they do not run before the world
/// can take them [design; the console then answered "Scripts are not running", docs/
/// outstanding.md §5].
///
/// Such an aid waits until the scripts first run (`crate::script`; with scripts off,
/// until the player first stands in the world). From then its clock is the aids' clock
/// ([`AidTime::elapsed_secs`]) less what the late start held it back: the start's time
/// past [`SCRIPTS_START_SECONDS`]. A step due before then runs in the first frame the
/// scripts run; every later step keeps its time on an unbusy machine and its distance
/// from the others on a busy one. The script steps are still moved on their own when
/// the scripts start after the first one's time (`script_events::delay_late_aids`),
/// now by at most `SCRIPTS_START_SECONDS` less that time: a busy run then looks like
/// the slowest unbusy one [design; follower-boone failed when only its script steps
/// came 7.2 s late, 2026-10-17]. Aids that also work before the world (the main menu,
/// the intro, the pause menu and its settings page, an audio device reopen) keep the
/// aids' clock. `nvforever --run-seconds` is lengthened by the held-back seconds, so
/// the last steps still run.
///
/// Without the switch the world clock is the aids' clock, as before.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct WorldAidClock {
    from_scripts: bool,
    /// The seconds held back, once the scripts ran.
    held: Option<f32>,
}

impl WorldAidClock {
    pub fn new(from_scripts: bool) -> WorldAidClock {
        WorldAidClock { from_scripts, held: None }
    }

    pub fn from_env() -> WorldAidClock {
        let on = std::env::var("NVF_PLAY_AIDS_FROM_SCRIPTS").is_ok_and(|v| !matches!(v.trim(), "" | "0" | "off" | "false"));
        WorldAidClock::new(on)
    }

    /// The world aids' clock at `now` on the aids' clock (`None`: not started yet).
    pub fn at(&self, now: f32) -> Option<f32> {
        if !self.from_scripts {
            return Some(now);
        }
        self.held.map(|h| now - h)
    }

    /// The seconds the world aids' clock was held back (0 without the switch or before
    /// the start).
    pub fn held_secs(&self) -> f32 {
        self.held.unwrap_or(0.0)
    }

    /// The scripts first ran at `now` (the aids' clock): starts the clock and returns the
    /// seconds it is held back, the first time and with the switch only.
    pub fn start(&mut self, now: f32) -> Option<f32> {
        if !self.from_scripts || self.held.is_some() {
            return None;
        }
        let held = (now - SCRIPTS_START_SECONDS).max(0.0);
        self.held = Some(held);
        Some(held)
    }
}

/// The start's loading screen (`crate::loading::StartCover`) counts on neither clock: the
/// test aids count from when it comes down, as if the run began then.
pub(crate) fn tick_play_clock(virt: Res<Time<Virtual>>, real: Res<Time<Real>>, loading: Option<Res<crate::loading::Loading>>, mut clock: ResMut<PlayClock>) {
    clock.frame = virt.is_paused().then(|| real.delta_secs().min(0.25));
    if loading.is_some_and(|l| l.starting()) {
        return;
    }
    clock.stopped += clock.frame.unwrap_or(0.0);
}

/// Pause the virtual clock while the picture is held ([`picture_held`]), or while the
/// console's free camera holds the world (`crate::free_cam`, mode 1), and let it run
/// again after. Nothing else pauses it.
fn freeze_picture(mode: Res<MenuPause>, paused: Res<Paused>, state: Res<State<PlayState>>, free_cam: Option<Res<crate::free_cam::FreeCam>>, mut clock: ResMut<Time<Virtual>>) {
    let held_by_cam = free_cam.is_some_and(|f| f.holds_world() && !f.editor);
    let stop = picture_held(*mode, paused.0, *state.get()) || held_by_cam;
    if clock.is_paused() != stop {
        let why = if paused.0 {
            "the pause menu"
        } else if *state.get() == PlayState::Loading {
            "a loading screen"
        } else if held_by_cam {
            "the free camera"
        } else {
            "--menu-pause original"
        };
        if stop {
            info!("menu pause: the picture stands still ({why})");
            clock.pause();
        } else {
            info!("menu pause: the picture runs again");
            clock.unpause();
        }
    }
}

/// `NVF_PLAY_TEST_PAUSE`: when to open the pause menu and when to resume;
/// `NVF_PLAY_TEST_SETTINGS`: when to flip which switch of the settings page.
#[derive(Resource, Default, Debug, PartialEq)]
struct PauseAid {
    open: Vec<f32>,
    resume: Vec<f32>,
    switches: Vec<(f32, String)>,
    /// `NVF_PLAY_TEST_PAUSE_MENU`: save, load, down, up, pick, back.
    actions: Vec<(f32, String)>,
}

/// Parse `<open seconds>:<resume seconds>[,…]`.
fn parse_pause_aid(v: &str) -> Result<PauseAid, String> {
    let mut aid = PauseAid::default();
    for item in v.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (a, b) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <open seconds>:<resume seconds>"))?;
        let num = |s: &str| s.trim().parse::<f32>().map_err(|_| format!("{item:?}: bad seconds {s:?}"));
        let (a, b) = (num(a)?, num(b)?);
        if b <= a {
            return Err(format!("{item:?}: the menu resumes after it opens"));
        }
        aid.open.push(a);
        aid.resume.push(b);
    }
    Ok(aid)
}

/// Parse `<seconds>:<key>[,…]`.
fn parse_settings_aid(v: &str) -> Result<Vec<(f32, String)>, String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (t, key) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <seconds>:<key>"))?;
            let t = t.trim().parse::<f32>().map_err(|_| format!("{item:?}: bad seconds {t:?}"))?;
            Ok((t, key.trim().to_string()))
        })
        .collect()
}

/// Whether a time of `due` has come (each fires once).
fn fire(due: &mut Vec<f32>, now: f32) -> bool {
    let before = due.len();
    due.retain(|&t| t > now);
    due.len() < before
}

/// Whether the pause menu is the open menu.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paused(pub bool);

/// The page of the pause menu that is up; on the settings page, the row the keys are on.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    #[default]
    Main,
    Settings {
        selected: usize,
    },
    /// The load page; the chosen save (index in [`LoadList`]).
    Load {
        selected: usize,
    },
    /// The settings page's Controls tab: the chosen control (index in
    /// `crate::bindings::GAME_CONTROLS`), and whether it waits for its new key.
    Controls {
        selected: usize,
        listening: bool,
    },
    /// The Mods page: a mod's page of `StartMenu` drawn from its tiles (MCM's), with the
    /// start menu's MenuMode scripts running (issue #976, T11; [`ModsEntry`]).
    Mods,
}

impl Page {
    /// On the settings page, the chosen row (`crate::ui_sounds` hears it move).
    pub(crate) fn settings_row(self) -> Option<usize> {
        match self {
            Page::Main | Page::Load { .. } | Page::Mods => None,
            Page::Settings { selected } => Some(selected),
            Page::Controls { selected, .. } => Some(CONTROL_ROWS_BASE + selected),
        }
    }

    /// Whether `label`, an entry of the column, is the one this page stands for: a click
    /// on it closes the page again [owner 2026-10-16].
    fn opened_by(self, label: &str) -> bool {
        match self {
            Page::Main => false,
            Page::Settings { .. } | Page::Controls { .. } => label.starts_with("Settings"),
            Page::Load { .. } => label.starts_with("Load"),
            Page::Mods => label.starts_with(MODS_LABEL),
        }
    }
}

/// Whether the column has a Mods entry this frame: with `[UI] TileMenus=StartMenu` on, a
/// mod added tiles to `StartMenu` (MCM's include) [design: owner-review.md #34, "a Mods
/// entry appears only when a mod added one"; ui-tile-tree.md §7]. Written by
/// `crate::tiles` each frame; off by default, as the page shows only what the mod's own
/// scripts draw (docs/handoff/ui-tile-tree-tasks.md, T11).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModsEntry(pub bool);

/// Whether the Mods page is up: the pause menu open on [`Page::Mods`].
pub fn mods_open(paused: &Paused, page: &Page) -> bool {
    paused.0 && *page == Page::Mods
}

/// The Mods entry's label [design: NVForever's word].
const MODS_LABEL: &str = "Mods";

/// [`Page::settings_row`] of a control's row: past any switch's.
const CONTROL_ROWS_BASE: usize = 10_000;

/// The saves the load page lists, newest first, and the chosen one's thumbnail.
#[derive(Resource, Default)]
struct LoadList {
    saves: Vec<(PathBuf, SaveHeader)>,
    /// The thumbnail of the save at this index (None: it has none or it did not decode).
    thumb: Option<(usize, Option<Handle<Image>>)>,
}

impl LoadList {
    fn open(&mut self, dir: Option<&Path>) {
        self.saves = dir.map(fnv_save::list).unwrap_or_default();
        self.thumb = None;
        let names: Vec<String> = self.saves.iter().take(8).map(|(p, h)| format!("{:?} ({})", stem(p), kind_name(h.kind))).collect();
        info!("pause menu: load page: {} save(s), newest first: {}", self.saves.len(), names.join(", "));
    }

    /// Read and decode the thumbnail of save `i` unless it is the one held.
    fn ensure_thumb(&mut self, i: usize, images: &mut Assets<Image>) {
        if self.thumb.as_ref().is_some_and(|(t, _)| *t == i) {
            return;
        }
        let Some((path, _)) = self.saves.get(i) else { return };
        let decoded = fnv_save::read_header(path).ok().and_then(|f| crate::save::decode_thumbnail(&f.shot));
        let handle = decoded.and_then(|(w, h, rgba)| {
            info!("pause menu: load page row {i} {:?}: thumbnail {w}x{h}", stem(path));
            // A broken save's picture is left out rather than handed to the GPU.
            let format = bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb;
            let mut img = nvf_assets::texture::checked_image(w, h, 1, format, rgba).map_err(|e| warn!("pause menu: load page row {i}: thumbnail: {e}")).ok()?;
            img.asset_usage = bevy::asset::RenderAssetUsages::default();
            Some(images.add(img))
        });
        if handle.is_none() {
            info!("pause menu: load page row {i} {:?}: no thumbnail", stem(path));
        }
        self.thumb = Some((i, handle));
    }
}

/// The line under the main page's buttons: how the save asked from this menu went.
#[derive(Resource, Default)]
struct PauseStatus {
    waiting_for: Option<PathBuf>,
    line: String,
}

#[derive(Component)]
struct PauseStatusText;

/// What the Save button and the load page use.
#[derive(SystemParam)]
struct SaveMenu<'w> {
    list: ResMut<'w, LoadList>,
    images: ResMut<'w, Assets<Image>>,
    cfg: Res<'w, crate::save::SaveConfig>,
    saves: MessageWriter<'w, crate::save::SaveRequest>,
    loads: MessageWriter<'w, crate::save::LoadRequest>,
    status: ResMut<'w, PauseStatus>,
    motion: Option<Res<'w, AccumulatedMouseMotion>>,
    scroll: Option<Res<'w, AccumulatedMouseScroll>>,
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

fn kind_name(k: SaveKind) -> &'static str {
    match k {
        SaveKind::Manual => "manual",
        SaveKind::Quick => "quick",
        SaveKind::Auto => "auto",
        SaveKind::System => "system",
    }
}

/// Delete a save: its file, which holds the header, the thumbnail and the game
/// (`fnv_save`; docs/design/save-load.md §6.1), and the `.bak` a quick or auto slot keeps
/// of the one before. Only files of the saves' own extension are removed.
fn delete_save(path: &Path) -> std::io::Result<()> {
    if path.extension().is_none_or(|x| x != fnv_save::EXTENSION) {
        return Err(std::io::Error::other("not a save file"));
    }
    std::fs::remove_file(path)?;
    let _ = std::fs::remove_file(path.with_extension(format!("{}.bak", fnv_save::EXTENSION)));
    Ok(())
}

/// The name of a new save in the saves folder ([`next_save_name`] of the files there).
pub fn new_save_name(cfg: &crate::save::SaveConfig) -> String {
    let stems: Vec<String> = cfg.dir.as_deref().and_then(|d| std::fs::read_dir(d).ok()).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == fnv_save::EXTENSION)).map(|p| stem(&p)).collect();
    next_save_name(stems.iter().map(String::as_str))
}

/// The name of a new save: `Save <n>`, one more than the highest such name in `stems`.
fn next_save_name<'a>(stems: impl IntoIterator<Item = &'a str>) -> String {
    let n = stems.into_iter().filter_map(|s| s.strip_prefix("Save ")).filter_map(|n| n.parse::<u32>().ok()).max().unwrap_or(0);
    format!("Save {}", n + 1)
}

/// Play time as hours and minutes.
fn play_time(seconds: f64) -> String {
    let m = (seconds.max(0.0) / 60.0) as u64;
    if m < 60 {
        format!("{m} min")
    } else {
        format!("{} h {:02} min", m / 60, m % 60)
    }
}

/// How long ago a save was made (real time).
fn saved_ago(saved_unix: u64, now_unix: u64) -> String {
    let s = now_unix.saturating_sub(saved_unix);
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86400 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

#[derive(Component)]
struct PauseRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum PauseButton {
    /// An entry of the column at the right (its index).
    Column(usize),
    /// A category of the settings page (the index of its first switch).
    Category(usize),
    /// The row of a switch (its index in [`Settings`]).
    Switch(usize),
    /// The settings page's Controls category.
    ControlsTab,
    /// A control's row on the Controls tab (its index in `GAME_CONTROLS`).
    ControlRow(usize),
    /// A save's row on the load page (its index in [`LoadList`]).
    LoadRow(usize),
    /// A slider's arrow: its row and the steps it moves (-1 or 1).
    SliderStep(usize, i32),
    /// A slider's meter (its row): a press or drag sets the value under the pointer.
    Meter(usize),
    /// The scroll bar's arrow: the rows it moves the list (-1 or 1).
    ScrollStep(i32),
    /// The scroll bar's track: a press or drag puts the marker under the pointer.
    ScrollTrack,
    /// The settings page's Defaults.
    Defaults,
    /// From the settings or load page back to the first page.
    Back,
    /// A choice of the quit question (its index in [`CONFIRM`]).
    Confirm(usize),
    /// A choice in a list's open dropdown: its row and the choice's index.
    Choice(usize, usize),
    /// An answer of the keep question (its index in [`KEEP_ANSWERS`]).
    Keep(usize),
    /// The load page's "Delete  Del)".
    Delete,
    /// An answer of the delete question: yes or no.
    DeleteAnswer(bool),
}

/// A switch's state, kept as text too (tests read it).
#[derive(Component)]
struct SwitchState(#[allow(dead_code)] usize);

/// The chosen switch's note, kept as text too (tests read it).
#[derive(Component)]
struct SwitchNote;

/// The column's entries in the world [observed: the owner's screenshots of the original's
/// pause menu: Continue, Save, Load, Settings, Help, Quit; Help is left out, NVForever has
/// no help pages].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Continue,
    Save,
    Load,
    Settings,
    /// Only with [`ModsEntry`], before Quit [design].
    Mods,
    Quit,
}

/// The column's entries, with the Mods entry when `mods` ([`ModsEntry`]).
fn entries(mods: bool) -> Vec<Entry> {
    let mut v = vec![Entry::Continue, Entry::Save, Entry::Load, Entry::Settings];
    if mods {
        v.push(Entry::Mods);
    }
    v.push(Entry::Quit);
    v
}

impl Entry {
    fn label(self) -> &'static str {
        match self {
            Entry::Continue => "Continue",
            Entry::Save => "Save",
            Entry::Load => "Load",
            Entry::Settings => "Settings",
            Entry::Mods => MODS_LABEL,
            Entry::Quit => "Quit",
        }
    }
}

/// The quit question and its choices [observed: the owner's screenshot of the original's
/// quit question].
const WARNING: &str = "WARNING:  Any unsaved progress will be lost.";
const QUESTION: &str = "Are you sure you want to quit?";
const CONFIRM: [&str; 3] = ["Main Menu", "Exit Game", "Cancel"];
/// The keep question's answers [design].
const KEEP_ANSWERS: [&str; 2] = ["Keep", "Revert"];

/// The settings page's rows: this many shown at once [documented: `options_container`
/// `_number_of_visible_items` 9], each this high [observed: 39 of 720 pixels], the list
/// this wide and its left edge this far left of the view's middle, the switch's state
/// centred this far into a row [design: the original's list is 520 wide and centred, its
/// values at 360; NVForever's labels are longer].
const OPTION_ROWS: usize = 9;
/// The settings page's last category: the controls' keys [design].
const CONTROLS_TAB: &str = "Controls";
/// A control's value while it waits for its new key.
const WAITING: &str = "Press a key...";
/// The same with a controller.
const WAITING_PAD: &str = "Press a button...";
/// The Controls tab's first row.
const DEVICE_ROW: &str = "Input Device";
/// The Controls tab's last row with a controller chosen (`crate::rumble`).
const RUMBLE_ROW: &str = "Rumble";
/// The Controls tab's row after the controls (`bInvertYValues`).
const INVERT_ROW: &str = "Invert Y";
/// The Controls tab's slider after Invert Y, with keyboard and mouse and with a controller.
const SENSITIVITY_ROWS: [&str; 2] = ["Mouse Sensitivity", "Stick Sensitivity"];

/// The control on the Controls tab's row `row` (row 0 is the input device).
fn control_at(row: usize) -> Option<crate::bindings::Control> {
    row.checked_sub(1).and_then(|i| crate::bindings::GAME_CONTROLS.get(i)).map(|x| x.0)
}

/// The Controls tab's Invert Y row, after the controls.
fn invert_row() -> usize {
    crate::bindings::GAME_CONTROLS.len() + 1
}

/// The Controls tab's sensitivity slider, after Invert Y.
fn sensitivity_row() -> usize {
    invert_row() + 1
}

/// The Controls tab's Rumble row, the last (shown with a controller only).
fn rumble_row() -> usize {
    sensitivity_row() + 1
}

/// The Controls tab's rows: the device, the controls, Invert Y, the sensitivity and with
/// a controller Rumble.
fn control_rows(controller: bool) -> usize {
    rumble_row() + usize::from(controller)
}
const OPTION_ROW: f32 = 52.0;
/// The scroll bar right of the settings page's rows when not all of them fit, 32 wide
/// with 16-high arrows at its ends, its marker 6 wide, 12 in [design: as the barter
/// menu's (`crate::menu::look`), from the game's `interface\shared\scrollbar` pictures].
const BAR_W: f32 = 32.0;
const BAR_ARROW: f32 = 16.0;
const BAR_GAP: f32 = 4.0;
const BAR_MARKER_MIN: f32 = 32.0;
const BAR_PICTURES: [&str; 3] = ["interface\\shared\\scrollbar\\arrow_up.dds", "interface\\shared\\scrollbar\\arrow_down.dds", "interface\\shared\\scrollbar\\vert_marker.dds"];
/// Mouse wheel movement counted in pixels (a touchpad): this many make a notch.
const WHEEL_PIXELS: f32 = 20.0;
const OPTIONS_WIDTH: f32 = 820.0;
const OPTIONS_LEFT: f32 = -470.0;
/// The settings page's rows' top: higher than the column's (360 with the pause menu's
/// five entries), where nine rows and the note under them ran off the bottom [design:
/// #1210]. The categories stay beside the column.
const OPTIONS_TOP: f32 = 120.0;
/// The note under the rows: its lines this far apart, the last one above the version's
/// line [design].
const NOTE_STEP: f32 = 26.0;
const NOTE_BOTTOM: f32 = 900.0;
/// The room between the band's right edge and the categories [design: #1210, the scroll
/// bar and band ran under them at 16:9].
const TABS_GAP: f32 = 20.0;
/// The view's width in interface units when no window says (16:9).
const VIEW_W: f32 = 960.0 * 16.0 / 9.0;
const VALUE_AT: f32 = 700.0;
/// A slider's meter in its row: its left edge, width and height, the arrows' left edges
/// and the value's centre [design: the "How many?" meter's height and arrows
/// (`crate::menu::look`), fitted into the row's right part].
const METER_LEFT: f32 = 445.0;
const METER_WIDTH: f32 = 240.0;
const METER_HEIGHT: f32 = 24.0;
const METER_ARROWS: [f32; 2] = [420.0, 692.0];
const METER_ARROW: Vec2 = Vec2::new(17.5, 35.0);
const PERCENT_AT: f32 = 765.0;
/// A list's box in its row (its left edge and width, from the slider's left arrow to the
/// row's end), the open list's rows shown at once and their height [design].
const LIST_LEFT: f32 = 420.0;
const LIST_WIDTH: f32 = 386.0;
const DROP_ROWS: usize = 8;
const DROP_ITEM: f32 = 44.0;
/// A tick of the meter: 8 apart, the picture's top 8 × 20 texels stretched to the meter's
/// height (as the "How many?" meter's [observed]).
const METER_TICK: f32 = 8.0;
const TICK_TEXTURE: &str = "textures\\interface\\hud\\hud_tick_mark.dds";
const ARROW_TEXTURES: [&str; 2] = ["textures\\interface\\main\\main_arrow_left.dds", "textures\\interface\\main\\main_arrow_right.dds"];
/// The darkened band behind a page's list [documented: `container_background`
/// `fuzzy.dds` at alpha 160 the height of the screen, a little wider than the list].
const BAND: &str = "textures\\interface\\shared\\background\\fuzzy.dds";
const BAND_ALPHA: f32 = 160.0 / 255.0;
const BAND_MARGIN: f32 = 30.0;
/// The black under the band [design: the band alone left the main menu's logo and
/// pictures showing through, observed 2026-10-16].
const BAND_SHEET: f32 = 0.55;
/// The load page: rows shown at once, the panel's width, its rows' top, the thumbnail and
/// the details' lines [documented: `saveload_container` width 650, y 521; the player's
/// name, level and play time at y 347, 388, 429; the thumbnail above the name; design:
/// eight rows].
const LOAD_ROWS: usize = 8;
const LOAD_WIDTH: f32 = 650.0;
const LOAD_TOP: f32 = 521.0;
const THUMB_DRAWN: Vec2 = Vec2::new(448.0, 252.0);
const DETAILS_TOP: f32 = 347.0;
const DETAILS_STEP: f32 = 41.0;
/// "Delete  Del)" and its question: right of the thumbnail, in what the panel leaves
/// [design].
const DELETE_WIDTH: f32 = LOAD_WIDTH - THUMB_DRAWN.x - 20.0;
/// The page's Defaults and Back at the bottom right [observed: the owner's screenshots].
const BACK_TOP: f32 = 960.0 - 30.0 - ITEM;
/// The pause screen behind the menu in the world: the original shows a model,
/// `meshes\interface\pausescreen\pausescreen01.nif`, whose textures are these [observed:
/// the file's texture names]; drawn here flat over the held picture, the half picture
/// at the left and mirrored at the right [hypothesis: the model's two slides], the world
/// behind darkened [design]. The model's glare (`loading_glare.dds`) is left out: drawn
/// over the picture, not added to it, it came out a dark slab [observed 2026-10-16].
const SCREEN: &str = "textures\\interface\\pause\\pausescreen01.dds";
const SCREEN_ALPHA: f32 = 0.82;
/// How much of the pause screen picture lies past the view's edges, as parts of the drawn
/// picture: its outer side (the left of the half picture, mirrored at the right), its top
/// and its bottom, so the frame's edge meets the view's [observed: the owner's screenshot
/// of 2026-10-16 marking where the edge belongs, at about 7% in from the sides, 5% from
/// the top and 9% from the bottom of the stretched picture].
const SCREEN_CROP_SIDE: f32 = 0.147;
const SCREEN_CROP_TOP: f32 = 0.051;
const SCREEN_CROP_BOTTOM: f32 = 0.088;
const WORLD_SHADE: f32 = 0.35;

/// The settings page's group of switches about conversations, sound and menus.
pub const GROUP_MENUS: &str = "Gameplay";

/// The fonts and pictures of the pause menu, read once.
#[derive(Resource)]
struct PauseLook {
    faces: Faces,
    band: Option<Handle<Image>>,
    screen: Option<Handle<Image>>,
    /// The sliders' tick mark and arrows (left, right).
    tick: Option<Handle<Image>>,
    arrows: [Option<Handle<Image>>; 2],
    /// The scroll bar's arrows (up, down) and marker.
    bar: [Option<Picture>; 3],
}

impl PauseLook {
    fn load(world: &mut World) -> PauseLook {
        let bc = bc_supported(world.get_resource::<CompressedImageFormatSupport>());
        world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
            let data = world.get_resource::<GameData>();
            let pic = |path: &str, images: &mut Assets<Image>| data.and_then(|d| crate::interface::texture(path, d, images, bc)).map(|(h, _)| h);
            let look = PauseLook {
                faces: Faces::load(data, &mut images, bc),
                band: pic(BAND, &mut images),
                screen: pic(SCREEN, &mut images),
                tick: pic(TICK_TEXTURE, &mut images),
                arrows: ARROW_TEXTURES.map(|a| pic(a, &mut images)),
                bar: {
                    let mut pictures = Pictures::load(data);
                    BAR_PICTURES.map(|b| pictures.picture(b, data, &mut images, bc))
                },
            };
            info!("pause menu: the original's look (band {}, pause screen {})", look.band.is_some(), look.screen.is_some());
            look
        })
    }

    #[cfg(test)]
    fn plain() -> PauseLook {
        PauseLook { faces: Faces::plain(), band: None, screen: None, tick: None, arrows: [None, None], bar: [None, None, None] }
    }
}

/// What the keys have on the first page and in the quit question, and what Defaults goes
/// back to (taken when the settings page is first shown).
#[derive(Resource, Default)]
struct PauseView {
    main_selected: usize,
    confirm: Option<usize>,
    defaults: Option<Vec<f32>>,
    drawn: Option<Drawn>,
    /// The slider whose meter is being dragged (pressed and held).
    dragging: Option<usize>,
    /// The settings page's (or the Controls tab's) first row shown, counted from its
    /// category's first; on the load page, the saves' first row shown.
    scroll: usize,
    /// The scroll bar's marker is being dragged.
    bar_dragging: bool,
    /// The wheel's movement short of a notch.
    wheel: f32,
    /// A list's open dropdown.
    dropdown: Option<Dropdown>,
    /// The keep question's chosen answer.
    keep_selected: usize,
    /// The load page asks whether to delete the save at this row.
    delete_ask: Option<usize>,
}

/// A list's open dropdown: its row, the choice the keys are on and the first shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Dropdown {
    row: usize,
    at: usize,
    first: usize,
}

/// Everything the page drawn shows: drawn again when it changes.
#[derive(Clone, PartialEq, Debug)]
struct Drawn {
    page: Page,
    main_selected: usize,
    confirm: Option<usize>,
    /// Every row's state (`Setting::value`).
    states: Vec<f32>,
    /// Explanations replacing disabled rows' saved values.
    disabled: Vec<Option<&'static str>>,
    status: String,
    /// The column's labels and whether each can be taken.
    column: Vec<(&'static str, bool)>,
    /// Under the main menu (its entries make the column; no pause screen).
    under_main: bool,
    /// The load page's thumbnail: the row it is of and whether there is one.
    thumb: Option<(usize, bool)>,
    /// The Controls tab's values, in `GAME_CONTROLS`' order, on the device in use.
    controls: Vec<String>,
    /// The device in use: its name, and whether it is a controller.
    device: (&'static str, bool),
    /// The controller rumbles.
    rumble: bool,
    /// The vertical look is inverted.
    invert_y: bool,
    /// The device in use's look sensitivity, a multiple of the game's.
    sensitivity: f32,
    /// The settings page's (or the load page's) first row shown ([`PauseView::scroll`]).
    scroll: usize,
    /// The list rows: each one's row, choices and chosen one.
    lists: Vec<(usize, Vec<String>, String)>,
    dropdown: Option<Dropdown>,
    /// The keep question: its row's label, the seconds left and the chosen answer.
    keep: Option<(&'static str, u32, usize)>,
    /// The load page's delete question: the row it asks about.
    delete_ask: Option<usize>,
    /// The view's width in interface units (960 high), rounded.
    view_w: u32,
    /// The language's count of changes (`fnv_util::words::generation`): a new language
    /// draws the page again in it.
    language: u32,
}

impl Drawn {
    /// List row `i`'s choices and chosen one.
    fn list(&self, i: usize) -> Option<(&[String], &str)> {
        self.lists.iter().find(|l| l.0 == i).map(|l| (l.1.as_slice(), l.2.as_str()))
    }
}

impl Drawn {
    /// The column's entry that stands for the page.
    fn page_entry(&self) -> usize {
        let find = |label: &str| self.column.iter().position(|(l, _)| l.starts_with(label)).unwrap_or(0);
        match self.page {
            Page::Main if self.confirm.is_some() => find("Quit"),
            Page::Main => self.main_selected,
            Page::Settings { .. } | Page::Controls { .. } => find("Settings"),
            Page::Load { .. } => find("Load"),
            Page::Mods => find(MODS_LABEL),
        }
    }
}

/// The settings page's categories: each group's name and its first switch.
fn categories(list: &[Setting]) -> Vec<(&'static str, usize)> {
    let mut out: Vec<(&'static str, usize)> = Vec::new();
    for (i, s) in list.iter().enumerate() {
        if out.last().is_none_or(|(g, _)| *g != s.group) {
            out.push((s.group, i));
        }
    }
    out
}

/// The rows of `selected`'s group: its first and one past its last.
fn group_rows(list: &[Setting], selected: usize) -> std::ops::Range<usize> {
    let Some(g) = list.get(selected).map(|s| s.group) else { return 0..0 };
    let first = list.iter().position(|s| s.group == g).unwrap_or(0);
    let end = list.iter().rposition(|s| s.group == g).map_or(first, |i| i + 1);
    first..end
}

/// The first row shown of a list of `rows`, [`OPTION_ROWS`] at once: `first` moved as
/// little as keeps `selected` in view.
fn follow(first: usize, selected: usize, rows: usize) -> usize {
    crate::menu::model::keep_in_view(first, selected, rows, OPTION_ROWS)
}

/// A list's window: the rows shown at once and each one's height.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Window {
    shown: usize,
    row: f32,
}

/// The settings page's (and the Controls tab's) window of rows.
const SETTINGS_WINDOW: Window = Window { shown: OPTION_ROWS, row: OPTION_ROW };
/// The load page's window of saves.
const LOAD_WINDOW: Window = Window { shown: LOAD_ROWS, row: ITEM };

impl Window {
    /// The height of the scroll bar's track, between its arrows.
    fn track(self) -> f32 {
        self.shown as f32 * self.row - 2.0 * BAR_ARROW
    }

    /// The scroll bar's marker for a list of `rows` from `first`: its top in the track
    /// and its height, or `None` when all the rows fit.
    fn marker(self, rows: usize, first: usize) -> Option<(f32, f32)> {
        let last = rows.checked_sub(self.shown).filter(|l| *l > 0)?;
        let track = self.track();
        let h = (track * self.shown as f32 / rows as f32).clamp(BAR_MARKER_MIN, track);
        Some((first.min(last) as f32 / last as f32 * (track - h), h))
    }

    /// The first row shown with the marker's middle at `y` down the track (0 its top, 1
    /// its bottom).
    fn first_at(self, rows: usize, y: f32) -> usize {
        let Some((_, h)) = self.marker(rows, 0) else { return 0 };
        let last = rows - self.shown;
        let track = self.track();
        let at = ((y * track - h / 2.0) / (track - h)).clamp(0.0, 1.0);
        ((at * last as f32).round() as usize).min(last)
    }

    /// Row `i` of a list shown from `first` is in the window (a row the pointer can take).
    fn holds(self, first: usize, i: usize) -> bool {
        (first..first + self.shown).contains(&i)
    }
}

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        let mut aid = match std::env::var("NVF_PLAY_TEST_PAUSE") {
            Ok(v) => parse_pause_aid(&v).unwrap_or_else(|e| {
                warn!("NVF_PLAY_TEST_PAUSE: {e}");
                PauseAid::default()
            }),
            Err(_) => PauseAid::default(),
        };
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_SETTINGS") {
            match parse_settings_aid(&v) {
                Ok(s) => aid.switches = s,
                Err(e) => warn!("NVF_PLAY_TEST_SETTINGS: {e}"),
            }
        }
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_PAUSE_MENU") {
            match parse_settings_aid(&v) {
                Ok(s) => aid.actions = s,
                Err(e) => warn!("NVF_PLAY_TEST_PAUSE_MENU: {e}"),
            }
        }
        app.init_resource::<Paused>()
            .init_resource::<PageRequest>()
            .init_resource::<Page>()
            .init_resource::<LoadList>()
            .init_resource::<PauseStatus>()
            .init_resource::<PauseView>()
            .init_resource::<ModsEntry>()
            .add_message::<MainMenuRequest>()
            .init_resource::<MenuPause>()
            .init_resource::<PlayClock>()
            .insert_resource(aid)
            .add_systems(Update, tick_play_clock.before(PlaySet::Input))
            .add_systems(Update, freeze_picture.after(PlaySet::Ui).before(PlaySet::Sync))
            .add_systems(
                Update,
                // Esc pauses a conversation too; resuming goes back to it (`crate::dialogue`).
                // Over a conversation, a barter menu takes Esc for itself (`crate::menu`).
                (
                    open.run_if(in_state(PlayState::Playing).or_else(in_state(PlayState::Dialogue)).and_then(crate::menu::no_menu)),
                    (choose, show_status, draw).chain().run_if(in_state(PlayState::Menu)),
                )
                    .in_set(PlaySet::Ui),
            )
            .add_systems(OnExit(PlayState::Menu), close);
        add_setting(
            app,
            Setting {
                key: "menu-pause",
                group: GROUP_MENUS,
                label: "Freeze the picture in dialogue and menus",
                note: "On: while a conversation, the inventory, a container or a message box is open, animations, wind, water and sky stand still, as in the original. Off: only game logic waits and the world lives on. This menu always holds everything. (--menu-pause original|living)",
                get: |w| *w.resource::<MenuPause>() == MenuPause::Original,
                set: |w, on| *w.resource_mut::<MenuPause>() = if on { MenuPause::Original } else { MenuPause::Living },
                slider: None,
            },
        );
    }
}


fn full() -> Node {
    Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }
}

/// A node `width` wide whose right edge is `right` from the view's.
fn from_right(right: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, right: u(right), top: u(top), width: u(width), height: u(height), ..default() }
}

/// A node whose left edge is `left` from the view's middle.
fn from_middle(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(u(left)), top: u(top), width: u(width), height: u(height), ..default() }
}

/// A right-aligned list entry: the whole row takes the pointer, the box round it when
/// `boxed`.
#[allow(clippy::too_many_arguments)]
fn right_entry(p: &mut ChildSpawnerCommands, face: &Face, label: &str, node: Node, width: f32, shade: Shade, boxed_on: bool, b: PauseButton, glyphs: &crate::pad::PadGlyphs) {
    let w = crate::pad::hint_width(face, label, glyphs);
    p.spawn((Button, b, boxed(node, boxed_on))).with_children(|t| {
        crate::pad::set_hint(t, face, label, width - w - 10.0, BASE, color(shade), glyphs);
    });
}

/// A node with the highlight box round it when `on`.
fn boxed(node: Node, on: bool) -> (Node, BorderColor, BackgroundColor) {
    let (line, fill) = highlight_colors();
    if on {
        (Node { border: UiRect::all(u(LINE)), ..node }, BorderColor::all(line), BackgroundColor(fill))
    } else {
        (node, BorderColor::all(Color::NONE), BackgroundColor(Color::NONE))
    }
}

/// The darkened band behind a page's list.
fn band(root: &mut ChildSpawnerCommands, look: &PauseLook, node: Node) {
    // Under it a sheet of black, so the list reads over a bright main menu screen too.
    root.spawn((node.clone(), BackgroundColor(Color::BLACK.with_alpha(BAND_SHEET))));
    match &look.band {
        Some(b) => {
            root.spawn((node, ImageNode { color: Color::WHITE.with_alpha(BAND_ALPHA), image_mode: NodeImageMode::Stretch, ..ImageNode::new(b.clone()) }));
        }
        None => {
            root.spawn((node, BackgroundColor(Color::BLACK.with_alpha(BAND_ALPHA * 0.6))));
        }
    }
}

/// Draw the page `d` (the one before it is gone already).
fn spawn_page(commands: &mut Commands, d: &Drawn, look: &PauseLook, settings: &[Setting], list: &LoadList, glyphs: &crate::pad::PadGlyphs) {
    if d.under_main && d.page == Page::Main {
        return;
    }
    let f = &look.faces;
    // The settings rows' left edge, clear of the categories.
    let left = options_left(d.view_w as f32, tabs_width(f, settings));
    commands.spawn((PauseRoot, full(), GlobalZIndex(110))).with_children(|root| {
        // The pause screen over the held world.
        if !d.under_main {
            root.spawn((full(), BackgroundColor(Color::BLACK.with_alpha(WORLD_SHADE))));
            if let Some(s) = &look.screen {
                // Each half drawn larger than the view's half, its cropped parts off the view.
                let width = 50.0 / (1.0 - SCREEN_CROP_SIDE);
                let height = 100.0 / (1.0 - SCREEN_CROP_TOP - SCREEN_CROP_BOTTOM);
                let half = |left: bool| Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(if left { 50.0 - width } else { 50.0 }),
                    top: Val::Percent(-SCREEN_CROP_TOP * height),
                    width: Val::Percent(width),
                    height: Val::Percent(height),
                    ..default()
                };
                let img = |flip: bool| ImageNode { color: Color::WHITE.with_alpha(SCREEN_ALPHA), flip_x: flip, image_mode: NodeImageMode::Stretch, ..ImageNode::new(s.clone()) };
                root.spawn((half(true), img(false)));
                root.spawn((half(false), img(true)));
            } else {
                root.spawn((full(), BackgroundColor(Color::srgba(0.30, 0.24, 0.14, SCREEN_ALPHA))));
            }
        }
        // The page at the UI scale, in the middle of the view (`crate::interface::menu_frame`,
        // #1631); the sheet and the screen above keep to the view.
        crate::interface::in_menu_frame(root, crate::interface::MIDDLE, |root| {
            // The band behind a page's list, from the view's top to its bottom.
            let tall = |n: Node| crate::interface::over_view(n, crate::interface::MIDDLE, false);
            match d.page {
                Page::Settings { .. } | Page::Controls { .. } => {
                    band(root, look, tall(Node { top: u(0.0), height: Val::Percent(100.0), ..from_middle(left - BAND_MARGIN, 0.0, OPTIONS_WIDTH + 2.0 * BAND_MARGIN + BAR_GAP + BAR_W, 0.0) }))
                }
                Page::Load { .. } => band(root, look, tall(Node { height: Val::Percent(100.0), ..from_right(RIGHT + COLUMN, 0.0, LOAD_WIDTH + BAND_MARGIN, 0.0) })),
                // The mod's page is drawn from its tiles over this one (`crate::tiles::draw`).
                Page::Main | Page::Mods => {}
            }
            // The column: the keys' list on the first page, else half bright but for the page's entry.
            let top = (960.0 - d.column.len() as f32 * ITEM) / 2.0;
            let on_column = d.page == Page::Main && d.confirm.is_none();
            let lit = d.page_entry();
            for (i, (label, ok)) in d.column.iter().enumerate() {
                let shade = if (on_column && *ok) || (!on_column && i == lit) { Shade::Bright } else { Shade::Dim };
                right_entry(root, &f.entry, label, from_right(RIGHT, top + i as f32 * ITEM, COLUMN - 5.0, ITEM), COLUMN - 5.0, shade, on_column && i == d.main_selected, PauseButton::Column(i), glyphs);
            }
            match d.page {
                Page::Main => {
                    if let Some(sel) = d.confirm {
                        // The question: its lines end left of the choices [observed: the owner's screenshot].
                        let qw = f.entry.font.width(QUESTION);
                        let at = 280.0;
                        root.spawn(from_middle(at - 20.0 - qw, 480.0 - ITEM, 900.0, ITEM)).with_children(|t| sm_text(t, &f.entry, WARNING, 0.0, BASE, color(Shade::Bright)));
                        root.spawn(from_middle(at - 20.0 - qw, 480.0, qw, ITEM)).with_children(|t| sm_text(t, &f.entry, QUESTION, 0.0, BASE, color(Shade::Bright)));
                        let width = CONFIRM.iter().map(|c| f.entry.font.width(c)).fold(0.0, f32::max) + 30.0;
                        for (i, c) in CONFIRM.iter().enumerate() {
                            root.spawn((Button, PauseButton::Confirm(i), boxed(from_middle(at, 480.0 + i as f32 * ITEM, width, ITEM), i == sel)))
                            .with_children(|t| sm_text(t, &f.entry, c, 10.0, BASE, color(Shade::Bright)));
                        }
                    } else if !d.status.is_empty() {
                        root.spawn((PauseStatusText, from_right(RIGHT, top + d.column.len() as f32 * ITEM + 10.0, 700.0, ITEM))).with_children(|t| {
                            let w = f.small.font.width(&d.status);
                            sm_text(t, &f.small, &d.status, 700.0 - w - 10.0, 30.0, color(Shade::Bright));
                        });
                    }
                }
                Page::Settings { selected } => settings_page(root, d, look, settings, selected, top, left, glyphs),
                Page::Controls { selected, listening } => controls_page(root, d, look, settings, selected, listening, top, left, glyphs),
                Page::Load { selected } => load_page(root, d, look, list, selected, glyphs),
                Page::Mods => {}
            }
            // Back (and Defaults) at the bottom right.
            if d.page != Page::Main {
                right_entry(root, &f.entry, "Back", from_right(RIGHT, BACK_TOP, 200.0, ITEM), 200.0, Shade::Bright, false, PauseButton::Back, glyphs);
                if matches!(d.page, Page::Settings { .. } | Page::Controls { .. }) {
                    right_entry(root, &f.entry, "Defaults", from_right(RIGHT, BACK_TOP - ITEM, 200.0, ITEM), 200.0, Shade::Bright, false, PauseButton::Defaults, glyphs);
                }
            }
            if let Some((label, left, sel)) = d.keep {
                keep_question(root, f, label, left, sel);
            }
        });
    });
}

/// The keep question over the page: whether to keep the choice of row `label`, the
/// seconds left, Keep and Revert.
fn keep_question(root: &mut ChildSpawnerCommands, f: &Faces, label: &str, left: u32, sel: usize) {
    // A sheet over the page that takes the pointer, so only the answers can be clicked.
    root.spawn((crate::interface::over_view(full(), crate::interface::MIDDLE, true), BackgroundColor(Color::BLACK.with_alpha(0.6)), Button, GlobalZIndex(112)));
    let (w, h) = (640.0, 4.0 * ITEM + 40.0);
    let (line, _) = highlight_colors();
    let question = format!("Keep this {}?", label.to_lowercase());
    let count = format!("Going back in {left} s.");
    root.spawn((KeepLine, Text::new(format!("{question} {count}")), Node { display: Display::None, ..default() }));
    root.spawn((Node { border: UiRect::all(u(LINE)), ..from_middle(-w / 2.0, 480.0 - h / 2.0, w, h) }, BorderColor::all(line), BackgroundColor(Color::BLACK), GlobalZIndex(113))).with_children(|b| {
        for (k, t) in [&question, &count].into_iter().enumerate() {
            let tw = f.entry.font.width(t);
            b.spawn(at(0.0, 20.0 + k as f32 * ITEM, w, ITEM)).with_children(|r| sm_text(r, &f.entry, t, (w - tw) / 2.0, BASE, color(Shade::Bright)));
        }
        let bw = 180.0;
        for (k, a) in KEEP_ANSWERS.iter().enumerate() {
            let x = w / 2.0 + if k == 0 { -bw - 20.0 } else { 20.0 };
            let aw = f.entry.font.width(a);
            b.spawn((Button, PauseButton::Keep(k), boxed(at(x, 20.0 + 2.0 * ITEM + 20.0, bw, ITEM), k == sel))).with_children(|r| sm_text(r, &f.entry, a, (bw - aw) / 2.0, BASE, color(Shade::Bright)));
        }
    });
}

/// The keep question's lines, kept as text too (tests read it).
#[derive(Component)]
struct KeepLine;

/// The settings page: the categories left of the column, the chosen category's switches
/// in the middle with the chosen one's note under them, the version at the bottom left.
#[allow(clippy::too_many_arguments)]
fn settings_page(root: &mut ChildSpawnerCommands, d: &Drawn, look: &PauseLook, list: &[Setting], selected: usize, top: f32, left: f32, glyphs: &crate::pad::PadGlyphs) {
    let f = &look.faces;
    category_column(root, f, list, list.get(selected).map(|s| s.group), top, glyphs);
    let top = OPTIONS_TOP;
    if list.is_empty() {
        root.spawn(from_middle(left, top, OPTIONS_WIDTH, OPTION_ROW)).with_children(|t| sm_text(t, &f.entry, tr("Nothing to switch."), 10.0, BASE, color(Shade::Dim)));
    }
    let rows = group_rows(list, selected);
    // A window of the group's rows from the one scrolled to, the chosen one in it.
    let first = rows.start + follow(d.scroll, selected.saturating_sub(rows.start), rows.len());
    scroll_bar(root, look, rows.len(), first - rows.start, top, left);
    for (n, i) in (first..rows.end).take(OPTION_ROWS).enumerate() {
        let s = &list[i];
        let v = d.states.get(i).copied().unwrap_or(0.0);
        let disabled = d.disabled.get(i).copied().flatten();
        root.spawn((Button, PauseButton::Switch(i), boxed(from_middle(left, top + n as f32 * OPTION_ROW, OPTIONS_WIDTH, OPTION_ROW), i == selected)))
        .with_children(|r| {
            sm_text(r, &f.entry, tr(s.label), 10.0, BASE + 2.0, color(if disabled.is_some() { Shade::Dim } else { Shade::Bright }));
            if let Some(reason) = disabled {
                let reason = tr(reason);
                r.spawn((SwitchState(i), Text::new(reason), Node { display: Display::None, ..default() }));
                let vw = f.entry.font.width(reason);
                sm_text(r, &f.entry, reason, VALUE_AT - vw / 2.0, BASE + 2.0, color(Shade::Dim));
                return;
            }
            if let Some(slider) = &s.slider {
                slider_row(r, look, i, v, &slider.text(v));
                return;
            }
            if let Some((_, chosen)) = d.list(i) {
                list_row(r, look, i, chosen, d.dropdown.is_some_and(|o| o.row == i));
                return;
            }
            let value = if v > 0.5 { "ON" } else { "OFF" };
            let vw = f.entry.font.width(value);
            r.spawn((SwitchState(i), Text::new(value), Node { display: Display::None, ..default() }));
            sm_text(r, &f.entry, value, VALUE_AT - vw / 2.0, BASE + 2.0, color(Shade::Bright));
        });
    }
    // The chosen switch's note under the rows.
    let note = list.get(selected).map_or("", |s| tr(s.note));
    root.spawn((SwitchNote, Text::new(note), Node { display: Display::None, ..default() }));
    let shown = (rows.end - first).min(OPTION_ROWS) as f32;
    note_lines(root, f, note, left, top + shown * OPTION_ROW + 20.0);
    version(root, f);
    // The open dropdown over the rows under its own (over those above near the bottom).
    let Some((o, names)) = d.dropdown.and_then(|o| Some((o, d.list(o.row)?.0))) else { return };
    let Some(n) = (first..rows.end).take(OPTION_ROWS).position(|i| i == o.row) else { return };
    let shown = names.len().min(DROP_ROWS);
    let h = shown as f32 * DROP_ITEM + 2.0 * LINE;
    let row_top = top + n as f32 * OPTION_ROW;
    let y = if row_top + OPTION_ROW + h > 950.0 { row_top - h } else { row_top + OPTION_ROW };
    let (line, _) = highlight_colors();
    root.spawn((Node { border: UiRect::all(u(LINE)), ..from_middle(left + LIST_LEFT, y, LIST_WIDTH, h) }, BorderColor::all(line), BackgroundColor(Color::BLACK), GlobalZIndex(111))).with_children(|b| {
        for (k, name) in names.iter().enumerate().skip(o.first).take(shown) {
            let nw = f.entry.font.width(name);
            b.spawn((Button, PauseButton::Choice(o.row, k), boxed(at(0.0, (k - o.first) as f32 * DROP_ITEM, LIST_WIDTH - 2.0 * LINE, DROP_ITEM), k == o.at)))
            .with_children(|r| sm_text(r, &f.entry, name, (LIST_WIDTH - nw) / 2.0 - LINE, DROP_ITEM * 0.72, color(Shade::Bright)));
        }
    });
}

/// A list's row: its choice in a box with a down arrow (lit while its dropdown is open).
fn list_row(r: &mut ChildSpawnerCommands, look: &PauseLook, i: usize, chosen: &str, open: bool) {
    let bright = color(Shade::Bright);
    let (line, fill) = highlight_colors();
    r.spawn((Node { border: UiRect::all(u(LINE)), ..at(LIST_LEFT, 6.0, LIST_WIDTH, OPTION_ROW - 12.0) }, BorderColor::all(line.with_alpha(if open { 1.0 } else { 0.6 })), BackgroundColor(if open { fill } else { Color::NONE })));
    let vw = look.faces.entry.font.width(chosen);
    r.spawn((SwitchState(i), Text::new(chosen), Node { display: Display::None, ..default() }));
    sm_text(r, &look.faces.entry, chosen, LIST_LEFT + (LIST_WIDTH - 30.0 - vw) / 2.0, BASE + 2.0, bright);
    // The right arrow turned to point down.
    let node = at(LIST_LEFT + LIST_WIDTH - 30.0, (OPTION_ROW - METER_ARROW.y) / 2.0, METER_ARROW.x, METER_ARROW.y);
    let turned = UiTransform { rotation: Rot2::degrees(90.0), ..default() };
    match &look.arrows[1] {
        Some(image) => r.spawn((node, turned, ImageNode { color: bright, ..ImageNode::new(image.clone()) })),
        None => r.spawn((node, turned, BackgroundColor(bright.with_alpha(0.5)))),
    };
}

/// The categories left of the column: the switches' groups, then Controls; `current`
/// bright (`None`: Controls).
fn category_column(root: &mut ChildSpawnerCommands, f: &Faces, list: &[Setting], current: Option<&str>, top: f32, glyphs: &crate::pad::PadGlyphs) {
    let cats = categories(list);
    let tabs = cats.iter().map(|&(name, first)| (name, Some(name) == current, PauseButton::Category(first))).chain([(CONTROLS_TAB, current.is_none(), PauseButton::ControlsTab)]);
    for (i, (name, on, b)) in tabs.enumerate() {
        let name = tr(name);
        let w = SUB_WIDTH.max(f.entry.font.width(name) + 20.0);
        let shade = if on { Shade::Bright } else { Shade::Dim };
        right_entry(root, &f.entry, name, from_right(SUB_RIGHT, top + i as f32 * ITEM, w, ITEM), w, shade, false, b, glyphs);
    }
}

/// The settings page's Controls tab: every control with its key and mouse button, a
/// window of [`OPTION_ROWS`] round the chosen one, the chosen one's note under them.
#[allow(clippy::too_many_arguments)]
fn controls_page(root: &mut ChildSpawnerCommands, d: &Drawn, look: &PauseLook, list: &[Setting], selected: usize, listening: bool, top: f32, left: f32, glyphs: &crate::pad::PadGlyphs) {
    use crate::bindings::{label, Bindings};
    let f = &look.faces;
    category_column(root, f, list, None, top, glyphs);
    let top = OPTIONS_TOP;
    let (device, controller) = d.device;
    let n = control_rows(controller);
    let selected = selected.min(n - 1);
    let first = follow(d.scroll, selected, n);
    scroll_bar(root, look, n, first, top, left);
    for (row, i) in (first..n).take(OPTION_ROWS).enumerate() {
        let (name, value) = match control_at(i) {
            None if i == invert_row() => (INVERT_ROW, (if d.invert_y { "ON" } else { "OFF" }).to_string()),
            None if i == sensitivity_row() => (SENSITIVITY_ROWS[usize::from(controller)], crate::bindings::sensitivity_text(d.sensitivity)),
            None if i == rumble_row() => (RUMBLE_ROW, (if d.rumble { "ON" } else { "OFF" }).to_string()),
            None => (DEVICE_ROW, device.to_string()),
            Some(_) if listening && i == selected => ("", (if controller { WAITING_PAD } else { WAITING }).to_string()),
            Some(_) => ("", d.controls.get(i - 1).cloned().unwrap_or_default()),
        };
        let name = control_at(i).map_or(name, label);
        root.spawn((Button, PauseButton::ControlRow(i), boxed(from_middle(left, top + row as f32 * OPTION_ROW, OPTIONS_WIDTH, OPTION_ROW), i == selected)))
        .with_children(|r| {
            sm_text(r, &f.entry, name, 10.0, BASE + 2.0, color(Shade::Bright));
            if i == sensitivity_row() {
                slider_row(r, look, i, crate::bindings::sensitivity_part(d.sensitivity), &value);
                return;
            }
            let vw = crate::pad::button_width(&f.entry, &value, glyphs);
            r.spawn((SwitchState(i), Text::new(value.clone()), Node { display: Display::None, ..default() }));
            crate::pad::set_button(r, &f.entry, &value, (VALUE_AT - vw / 2.0).min(OPTIONS_WIDTH - vw - 14.0), BASE + 2.0, color(Shade::Bright), glyphs);
        });
    }
    let note = match control_at(selected) {
        None if selected == invert_row() => format!(
            "Enter or a click: look up and down the other way round (the mouse and the right stick). Kept as \"{INVERT_ROW}\" in [Controls] of NVForever.ini."
        ),
        None if selected == sensitivity_row() && controller => format!(
            "Left/Right or a click on the meter: how fast the right stick turns the view, a step of about a tenth ({} to {} of the game's). Kept as \"Stick Sensitivity\" in [Controller] of NVForever.ini.",
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_LEAST),
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_MOST)
        ),
        None if selected == sensitivity_row() => format!(
            "Left/Right or a click on the meter: how far the view turns as the mouse moves, a step of about a tenth ({} to {} of the game's). Kept as \"Mouse Sensitivity\" (radians a mouse count) in [Controls] of NVForever.ini.",
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_LEAST),
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_MOST)
        ),
        None if selected == rumble_row() => format!(
            "A or a click: the controller shakes when you fire (harder for a heavier weapon), take damage, land a hard fall or an explosion goes off near you. Kept as \"{RUMBLE_ROW}\" in [Controller] of NVForever.ini."
        ),
        None => format!(
            "Enter or a click: work the controls with the keyboard and mouse or with a controller. With a controller the left stick moves, the right stick looks, Start opens this menu, and in menus the D-pad, A and B are the arrow keys, Enter and Esc. Kept as \"{DEVICE_ROW}\" in [Controls] of NVForever.ini."
        ),
        Some(c) if listening && controller => format!("Press the new controller button for {}. Start cancels.", label(c)),
        Some(c) if listening => format!("Press the new key or mouse button for {}. Esc cancels.", label(c)),
        Some(c) if controller => format!(
            "A or a click: bind a new controller button to {} (a control that had it gets the old one). X or Delete clears it. Kept as \"{}\" in [Controller] of NVForever.ini.",
            label(c),
            Bindings::name(c)
        ),
        Some(c) => format!(
            "Enter or a click: bind a new key or mouse button to {} (a key replaces its key, a button its button; a control that had it gets the old one). Delete clears its key. Kept as \"{}\" in [Controls] of NVForever.ini.",
            label(c),
            Bindings::name(c)
        ),
    };
    root.spawn((SwitchNote, Text::new(note.clone()), Node { display: Display::None, ..default() }));
    let shown = (n - first).min(OPTION_ROWS) as f32;
    note_lines(root, f, &note, left, top + shown * OPTION_ROW + 20.0);
    version(root, f);
}

/// The note under the settings page's rows from `top`, wrapped to the rows' width and cut
/// to the lines that fit above [`NOTE_BOTTOM`].
fn note_lines(root: &mut ChildSpawnerCommands, f: &Faces, note: &str, left: f32, top: f32) {
    let width = OPTIONS_WIDTH - 20.0;
    let lines = f.small.font.lines(note, Some(width));
    let fit = note_fit(lines.len(), top);
    let bright = color(Shade::Bright);
    root.spawn(from_middle(left + 10.0, top, width, fit as f32 * NOTE_STEP)).with_children(|t| {
        for (i, line) in lines.iter().take(fit).enumerate() {
            crate::interface::set_line(t, &f.small, line, 0.0, NOTE_STEP * (i as f32 + 0.8), bright);
        }
    });
}

/// How many of a note's `lines` fit from `top` above [`NOTE_BOTTOM`].
fn note_fit(lines: usize, top: f32) -> usize {
    lines.min(((NOTE_BOTTOM - top) / NOTE_STEP).max(0.0) as usize)
}

/// The categories' width: the widest tab's ([`category_column`]).
fn tabs_width(f: &Faces, list: &[Setting]) -> f32 {
    categories(list).iter().map(|c| c.0).chain([CONTROLS_TAB]).map(|name| SUB_WIDTH.max(f.entry.font.width(name) + 20.0)).fold(SUB_WIDTH, f32::max)
}

/// The settings rows' left edge from the view's middle in a view `view_w` units wide
/// whose categories are `tabs_w` wide: [`OPTIONS_LEFT`] where that keeps the band
/// [`TABS_GAP`] clear of the categories, else as far left as does, but never so far that
/// the band leaves the view (a view narrower than about 16:10 cannot hold both).
fn options_left(view_w: f32, tabs_w: f32) -> f32 {
    let half = view_w / 2.0;
    let clear = half - (SUB_RIGHT + tabs_w + TABS_GAP) - (OPTIONS_WIDTH + BAR_GAP + BAR_W + BAND_MARGIN);
    OPTIONS_LEFT.min(clear).max(BAND_MARGIN - half)
}

/// The settings page's scroll bar, right of its rows.
fn scroll_bar(root: &mut ChildSpawnerCommands, look: &PauseLook, rows: usize, first: usize, top: f32, left: f32) {
    let bar = from_middle(left + OPTIONS_WIDTH + BAR_GAP, top, BAR_W, SETTINGS_WINDOW.shown as f32 * SETTINGS_WINDOW.row);
    window_bar(root, look, rows, first, SETTINGS_WINDOW, bar);
}

/// A list's scroll bar in `bar` when not all `rows` fit in `w`, from `first`: its arrows
/// step the list, a press or drag on its track puts the marker there.
fn window_bar(root: &mut ChildSpawnerCommands, look: &PauseLook, rows: usize, first: usize, w: Window, bar: Node) {
    let Some((y, h)) = w.marker(rows, first) else { return };
    let bright = color(Shade::Bright);
    let track = w.track();
    let piece = |p: &mut ChildSpawnerCommands, k: usize, node: Node, b: Option<PauseButton>| {
        let mut e = match &look.bar[k] {
            Some((image, rect)) => p.spawn((node, ImageNode { rect: *rect, color: bright, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) })),
            None => p.spawn((node, BackgroundColor(bright.with_alpha(if k == 2 { 1.0 } else { 0.5 })))),
        };
        if let Some(b) = b {
            e.insert((Button, b));
        }
    };
    root.spawn(bar).with_children(|p| {
        piece(p, 0, at(0.0, 0.0, BAR_W, BAR_ARROW), Some(PauseButton::ScrollStep(-1)));
        p.spawn((Button, PauseButton::ScrollTrack, RelativeCursorPosition::default(), at(0.0, BAR_ARROW, BAR_W, track))).with_children(|t| piece(t, 2, at(12.0, y, 6.0, h), None));
        piece(p, 1, at(0.0, BAR_ARROW + track, BAR_W, BAR_ARROW), Some(PauseButton::ScrollStep(1)));
    });
}

/// The version at the bottom left [observed: the original shows its own there].
fn version(root: &mut ChildSpawnerCommands, f: &Faces) {
    root.spawn(Node { position_type: PositionType::Absolute, left: u(30.0), top: u(960.0 - 60.0), width: u(400.0), height: u(40.0), ..default() }).with_children(|t| {
        sm_text(t, &f.small, concat!("NVForever ", env!("CARGO_PKG_VERSION")), 0.0, 26.0, color(Shade::Bright));
    });
}

/// A node at `x`, `y` in its parent, `w` × `h`.
fn at(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: u(x), top: u(y), width: u(w), height: u(h), ..default() }
}

/// How many ticks of a slider's meter show for value `v` (a tick partly in shows, cut).
fn meter_ticks(v: f32) -> usize {
    (METER_WIDTH * v.clamp(0.0, 1.0) / METER_TICK).ceil() as usize
}

/// A slider's meter, arrows and value (`value`: `v` as shown, e.g. a percentage) in its
/// row `i`.
fn slider_row(r: &mut ChildSpawnerCommands, look: &PauseLook, i: usize, v: f32, value: &str) {
    let bright = color(Shade::Bright);
    let my = (OPTION_ROW - METER_HEIGHT) / 2.0;
    // The ground, then the ticks as far as the value.
    r.spawn((at(METER_LEFT, my, METER_WIDTH, METER_HEIGHT), BackgroundColor(bright.with_alpha(0.25))));
    for t in 0..meter_ticks(v) {
        let x = METER_LEFT + t as f32 * METER_TICK;
        let w = METER_TICK.min(METER_LEFT + METER_WIDTH - x);
        match &look.tick {
            Some(image) => r.spawn((at(x, my, w, METER_HEIGHT), ImageNode { rect: Some(Rect::new(0.0, 0.0, w, 20.0)), color: bright, ..ImageNode::new(image.clone()) })),
            None => r.spawn((at(x, my, (w - 3.0).max(1.0), METER_HEIGHT), BackgroundColor(bright))),
        };
    }
    // Over them what takes the pointer, a little taller than the meter.
    r.spawn((Button, PauseButton::Meter(i), RelativeCursorPosition::default(), at(METER_LEFT, my - 6.0, METER_WIDTH, METER_HEIGHT + 12.0)));
    for (k, (x, step)) in [(METER_ARROWS[0], -1), (METER_ARROWS[1], 1)].into_iter().enumerate() {
        let node = at(x, (OPTION_ROW - METER_ARROW.y) / 2.0, METER_ARROW.x, METER_ARROW.y);
        match &look.arrows[k] {
            Some(image) => r.spawn((Button, PauseButton::SliderStep(i, step), node, ImageNode { color: bright, ..ImageNode::new(image.clone()) })),
            None => r.spawn((Button, PauseButton::SliderStep(i, step), node, BackgroundColor(bright.with_alpha(0.5)))),
        };
    }
    let vw = look.faces.entry.font.width(value);
    r.spawn((SwitchState(i), Text::new(value), Node { display: Display::None, ..default() }));
    sm_text(r, &look.faces.entry, value, PERCENT_AT - vw / 2.0, BASE + 2.0, bright);
}

/// The load page: the chosen save's thumbnail and details over the saves' rows (a window
/// of [`LOAD_ROWS`] from [`PauseView::scroll`], moved by the wheel, the scroll bar or the
/// keys, never by the pointer), left of the column; the scroll bar at the rows' right
/// when not all of them fit.
fn load_page(root: &mut ChildSpawnerCommands, d: &Drawn, look: &PauseLook, list: &LoadList, selected: usize, glyphs: &crate::pad::PadGlyphs) {
    let f = &look.faces;
    let panel = |top: f32, height: f32| from_right(RIGHT + COLUMN, top, LOAD_WIDTH, height);
    if list.saves.is_empty() {
        root.spawn(panel(LOAD_TOP, ITEM)).with_children(|t| sm_text(t, &f.entry, "No saves yet.", 10.0, BASE, color(Shade::Dim)));
        return;
    }
    let n = list.saves.len();
    let first = crate::menu::model::keep_in_view(d.scroll, selected, n, LOAD_ROWS);
    // The thumbnail, stretched to its frame.
    let frame = Node { position_type: PositionType::Absolute, right: u(RIGHT + COLUMN + LOAD_WIDTH - THUMB_DRAWN.x - 10.0), top: u(DETAILS_TOP - THUMB_DRAWN.y - 10.0), width: u(THUMB_DRAWN.x), height: u(THUMB_DRAWN.y), ..default() };
    match list.thumb.as_ref().filter(|(t, _)| *t == selected && d.thumb.is_some_and(|t| t.1)).and_then(|(_, h)| h.clone()) {
        Some(img) => {
            root.spawn((frame, ImageNode::new(img).with_mode(NodeImageMode::Stretch)));
        }
        None => {
            root.spawn((frame, BackgroundColor(Color::BLACK.with_alpha(0.5)))).with_children(|t| sm_text(t, &f.small, "No picture", 20.0, 40.0, color(Shade::Dim)));
        }
    }
    if let Some((path, h)) = list.saves.get(selected) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let place = if h.location.is_empty() { "Unknown place".to_string() } else { h.location.clone() };
        let lines = [
            format!("{}, level {}", h.player_name, h.level),
            format!("{place}, {}", crate::save::game_date(h)),
            format!("Played {}", play_time(h.play_seconds)),
            format!("{}: saved {} ({})", stem(path), saved_ago(h.saved_unix, now), kind_name(h.kind)),
        ];
        for (i, line) in lines.iter().enumerate() {
            let face = if i == 0 { &f.entry } else { &f.small };
            root.spawn((DetailLine, Text::new(line.clone()), Node { display: Display::None, ..default() }));
            root.spawn(panel(DETAILS_TOP + i as f32 * DETAILS_STEP, DETAILS_STEP)).with_children(|t| sm_text(t, face, line, 10.0, 30.0, color(Shade::Bright)));
        }
    }
    // Delete, right of the thumbnail; asked, the question and its answers under it.
    let side = |top: f32, height: f32| from_right(RIGHT + COLUMN, top, DELETE_WIDTH, height);
    let top = DETAILS_TOP - THUMB_DRAWN.y - 10.0;
    match d.delete_ask.filter(|r| *r == selected) {
        None => right_entry(root, &f.small, &format!("Delete  {}", crate::pad::hint(&[KeyCode::Delete], crate::pad::PAUSE_AT)), side(top, ITEM), DELETE_WIDTH, Shade::Bright, false, PauseButton::Delete, glyphs),
        Some(_) => {
            root.spawn((DetailLine, Text::new("Delete this save?"), Node { display: Display::None, ..default() }));
            root.spawn(side(top, ITEM)).with_children(|t| {
                let w = f.small.font.width("Delete this save?");
                sm_text(t, &f.small, "Delete this save?", DELETE_WIDTH - w - 10.0, BASE, color(Shade::Bright));
            });
            let (yes, no) = (crate::pad::hint(&[KeyCode::KeyY, KeyCode::Enter], crate::pad::PAUSE_AT), crate::pad::hint(&[KeyCode::KeyN, KeyCode::Escape], crate::pad::PAUSE_AT));
            right_entry(root, &f.small, &format!("Yes  {yes}"), side(top + ITEM, ITEM), DELETE_WIDTH, Shade::Bright, true, PauseButton::DeleteAnswer(true), glyphs);
            right_entry(root, &f.small, &format!("No  {no}"), side(top + 2.0 * ITEM, ITEM), DELETE_WIDTH, Shade::Bright, false, PauseButton::DeleteAnswer(false), glyphs);
        }
    }
    // With the scroll bar the rows leave room for it at their right.
    let bar = LOAD_WINDOW.marker(n, first).is_some();
    let width = if bar { LOAD_WIDTH - BAR_W - BAR_GAP } else { LOAD_WIDTH };
    window_bar(root, look, n, first, LOAD_WINDOW, from_right(RIGHT + COLUMN, LOAD_TOP, BAR_W, LOAD_WINDOW.shown as f32 * LOAD_WINDOW.row));
    for (row, (i, (path, h))) in list.saves.iter().enumerate().skip(first).take(LOAD_ROWS).enumerate() {
        let node = from_right(RIGHT + COLUMN + LOAD_WIDTH - width, LOAD_TOP + row as f32 * ITEM, width, ITEM);
        root.spawn((Button, PauseButton::LoadRow(i), boxed(node, i == selected))).with_children(|r| {
            sm_text(r, &f.entry, &stem(path), 10.0, BASE, color(Shade::Bright));
            let date = format!("{}  {}", kind_name(h.kind), crate::save::game_date(h));
            let w = f.small.font.width(&date);
            sm_text(r, &f.small, &date, width - w - 14.0, BASE, color(Shade::Dim));
        });
    }
}

/// A line of the details, kept as text too (tests read it).
#[derive(Component)]
struct DetailLine;


#[allow(clippy::too_many_arguments)]
fn open(
    keys: Res<ButtonInput<KeyCode>>,
    time: AidTime,
    mut aid: ResMut<PauseAid>,
    mut paused: ResMut<Paused>,
    mut page: ResMut<Page>,
    mut view: ResMut<PauseView>,
    mut status: ResMut<PauseStatus>,
    mut next: ResMut<NextState<PlayState>>,
    wheel: Option<Res<crate::weapon_wheel::WeaponWheel>>,
    (script, mut seen, mut request): (Option<Res<crate::script::ScriptState>>, Local<u64>, ResMut<PageRequest>),
) {
    let by_aid = fire(&mut aid.open, time.elapsed_secs());
    // ShowOff's ShowPauseMenu: each new ask opens the menu, on the page its mode names
    // (1 load, 3 settings; the others the main page) (`fnv_game::nvse_showoff5`).
    let asked = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| (rt.game.showoff.b5.pause_asks, rt.game.showoff.b5.pause_mode));
    let by_script = match asked {
        Some((n, mode)) if n != *seen => {
            *seen = n;
            (n > 0 && !paused.0).then_some(mode)
        }
        _ => None,
    };
    if !keys.just_pressed(KeyCode::Escape) && !by_aid && by_script.is_none() {
        return;
    }
    if let Some(mode) = by_script {
        info!("pause menu: opened by ShowPauseMenu {mode}");
        request.0 = match mode {
            1 => Some(Page::Load { selected: 0 }),
            3 => Some(Page::Settings { selected: 0 }),
            _ => None,
        };
    }
    // Esc closes the weapon wheel (`crate::weapon_wheel`), not the game.
    if wheel.is_some_and(|w| w.holds_input()) && !by_aid && by_script.is_none() {
        return;
    }
    if by_aid {
        info!("test aid NVF_PLAY_TEST_PAUSE: Esc");
    }
    info!("pause menu: open");
    paused.0 = true;
    *page = Page::Main;
    view.main_selected = 0;
    view.confirm = None;
    *status = PauseStatus::default();
    next.set(PlayState::Menu);
}

/// A page another menu asks the pause menu to show (the main menu's Load Game and
/// Settings, `crate::main_menu`); taken on the next frame.
#[derive(Resource, Default)]
pub struct PageRequest(pub Option<Page>);

/// Asked from the quit question: back to the main menu (nvforever starts again with
/// `--menu`).
#[derive(Message, Clone, Copy, Debug)]
pub struct MainMenuRequest;

/// The row after `selected` (`step` 1) or before it (-1) of `rows`, round the ends.
fn step_row(selected: usize, step: i32, rows: usize) -> usize {
    if rows == 0 {
        return 0;
    }
    (selected.min(rows - 1) as i32 + step).rem_euclid(rows as i32) as usize
}

/// The switch `step` rows from `selected` within its group, round its ends.
fn step_in_group(list: &[Setting], selected: usize, step: i32) -> usize {
    let rows = group_rows(list, selected);
    if rows.is_empty() {
        return 0;
    }
    rows.start + step_row(selected - rows.start, step, rows.len())
}

/// The first switch of the category after (`step` 1) or before (-1) `selected`'s.
fn step_category(list: &[Setting], selected: usize, step: i32) -> usize {
    let cats = categories(list);
    let Some(g) = list.get(selected).map(|s| s.group) else { return 0 };
    let at = cats.iter().position(|(n, _)| *n == g).unwrap_or(0);
    cats.get(step_row(at, step, cats.len())).map_or(0, |c| c.1)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn choose(
    keys: Res<ButtonInput<KeyCode>>,
    time: AidTime,
    mut aid: ResMut<PauseAid>,
    paused: Res<Paused>,
    mut page: ResMut<Page>,
    mut view: ResMut<PauseView>,
    settings: Res<Settings>,
    buttons: Query<(&PointerState, &PauseButton), Changed<PointerState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    mut sounds: MessageWriter<crate::ui_sounds::UiSound>,
    mut menu: SaveMenu,
    (mut request, main_menu, intro, mut to_main, mouse, free_cam): (ResMut<PageRequest>, Option<Res<crate::main_menu::MainMenu>>, Option<Res<crate::intro::Intro>>, MessageWriter<MainMenuRequest>, Res<ButtonInput<MouseButton>>, Res<crate::free_cam::FreeCam>),
    (bindings, pads, sticks, pending, mods): (Option<Res<crate::bindings::Bindings>>, Query<&Gamepad>, Option<Res<crate::pad::StickButtons>>, Option<Res<crate::settings::PendingKeep>>, Option<Res<ModsEntry>>),
    meters: Query<(&PauseButton, &RelativeCursorPosition)>,
) {
    use crate::bindings::{Bindings, Input, InputDevice, GAME_CONTROLS};
    use crate::pad::PadInput;
    use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
    // Held for the intro film, which takes the keys (`crate::intro`).
    if !paused.0 || intro.is_some_and(|i| i.is_playing()) {
        return;
    }
    // Under the main menu the first page is the main menu's own (`crate::main_menu`): its
    // pages come from here, and going back shows it again.
    let under_main = main_menu.as_ref().is_some_and(|m| m.open);
    let mods = mods.is_some_and(|m| m.0);
    let entries = entries(mods);
    // The Mods page goes when its entry does (the mod's tiles were unloaded).
    if *page == Page::Mods && !mods {
        info!("pause menu: no mod page any more, back to the column");
        *page = Page::Main;
    }
    if let Some(p) = request.0.take() {
        if matches!(p, Page::Load { .. }) {
            menu.list.open(menu.cfg.dir.as_deref());
        }
        info!("pause menu: {p:?} page for another menu");
        *page = p;
    }
    if under_main && *page == Page::Main {
        return;
    }
    let now = time.elapsed_secs();
    if fire(&mut aid.resume, now) {
        info!("test aid NVF_PLAY_TEST_PAUSE: Esc (resume)");
        info!("pause menu: resume");
        next.set(PlayState::Playing);
        return;
    }
    let list = settings.list();
    let mut flips = Vec::new();
    // Sliders moved: (row, steps), and set: (row, value).
    let mut steps: Vec<(usize, i32)> = Vec::new();
    let mut sets: Vec<(usize, f32)> = Vec::new();
    let is_slider = |i: usize| list.get(i).is_some_and(|s| s.slider.is_some());
    let is_list = |i: usize| settings.choices(i).is_some();
    // Lists' choices taken: (row, choice).
    let mut picks: Vec<(usize, String)> = Vec::new();
    // The settings aid: show the page and flip the switch, as a click on its row, or set
    // the slider (`key=value`).
    let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut aid.switches).into_iter().partition(|(t, _)| *t <= now);
    aid.switches = later;
    for (_, item) in due {
        info!("test aid NVF_PLAY_TEST_SETTINGS: {item}");
        if let Some(b) = item.strip_prefix("bind:") {
            // `bind:<control>=<key or button>`: as Enter on its row and that key.
            let parsed = b.split_once('=').and_then(|(c, i)| Some((Bindings::by_name(c)?, crate::bindings::parse_input(i)?)));
            match parsed {
                Some((c, input)) => {
                    let selected = GAME_CONTROLS.iter().position(|(k, _, _)| *k == c).map_or(0, |i| i + 1);
                    *page = Page::Controls { selected, listening: false };
                    commands.queue(move |world: &mut World| crate::bindings::rebind(world, c, input));
                }
                None => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected bind:<control>=<key or mouse button name>"),
            }
            continue;
        }
        if let Some(b) = item.strip_prefix("padbind:") {
            // `padbind:<control>=<button>`: as A on its row and that button.
            let parsed = b.split_once('=').and_then(|(c, i)| Some((Bindings::by_name(c)?, crate::pad::parse_pad(i)?)));
            match parsed {
                Some((c, p)) => {
                    let selected = GAME_CONTROLS.iter().position(|(k, _, _)| *k == c).map_or(0, |i| i + 1);
                    *page = Page::Controls { selected, listening: false };
                    commands.queue(move |world: &mut World| crate::bindings::rebind_pad(world, c, p));
                }
                None => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected padbind:<control>=<controller button name>"),
            }
            continue;
        }
        if let Some(v) = item.strip_prefix("rumble:") {
            // `rumble:1` or `rumble:0`: the Rumble row set so.
            match v {
                "1" | "0" => {
                    let on = v == "1";
                    *page = Page::Controls { selected: rumble_row(), listening: false };
                    commands.queue(move |world: &mut World| crate::bindings::set_rumble(world, on));
                }
                _ => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected rumble:1 or rumble:0"),
            }
            continue;
        }
        if let Some(v) = item.strip_prefix("invert:") {
            // `invert:1` or `invert:0`: the Invert Y row set so.
            match v {
                "1" | "0" => {
                    let on = v == "1";
                    *page = Page::Controls { selected: invert_row(), listening: false };
                    commands.queue(move |world: &mut World| crate::bindings::set_invert_y(world, on));
                }
                _ => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected invert:1 or invert:0"),
            }
            continue;
        }
        if let Some(v) = item.strip_prefix("sensitivity:") {
            // `sensitivity:<percent>`: the device in use's sensitivity slider set to it (onto its
            // steps).
            match v.trim_end_matches('%').parse::<f32>() {
                Ok(p) if p.is_finite() => {
                    let m = crate::bindings::step_sensitivity(p / 100.0, 0);
                    *page = Page::Controls { selected: sensitivity_row(), listening: false };
                    commands.queue(move |world: &mut World| crate::bindings::set_sensitivity(world, m));
                }
                _ => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected sensitivity:<percent>"),
            }
            continue;
        }
        if let Some(d) = item.strip_prefix("device:") {
            match InputDevice::parse(d) {
                Some(d) => {
                    *page = Page::Controls { selected: 0, listening: false };
                    commands.queue(move |world: &mut World| crate::bindings::set_device(world, d));
                }
                None => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected device:Keyboard and Mouse or device:Controller"),
            }
            continue;
        }
        let (key, value) = match item.split_once('=') {
            Some((k, v)) => (k.trim().to_string(), v.trim().parse::<f32>().ok()),
            None => (item.clone(), None),
        };
        match (settings.index(&key), key.as_str()) {
            (Some(i), _) if is_list(i) => {
                *page = Page::Settings { selected: i };
                match item.split_once('=') {
                    Some((_, v)) => picks.push((i, v.trim().to_string())),
                    None => warn!("NVF_PLAY_TEST_SETTINGS: {key} is a list: give it a choice, {key}=<choice>"),
                }
            }
            (Some(i), _) if is_slider(i) => {
                *page = Page::Settings { selected: i };
                match value {
                    Some(v) => sets.push((i, v)),
                    None => warn!("NVF_PLAY_TEST_SETTINGS: {key} is a slider: give it a value, {key}=0.5"),
                }
            }
            (Some(i), _) => {
                *page = Page::Settings { selected: i };
                flips.push(i);
            }
            (None, "page") => *page = Page::Settings { selected: 0 },
            (None, "controls") => *page = Page::Controls { selected: 0, listening: false },
            (None, _) => warn!("NVF_PLAY_TEST_SETTINGS: no switch {key:?}; there are {:?}", list.iter().map(|s| s.key).collect::<Vec<_>>()),
        }
    }
    // The menu aid's actions due now, as the keys they stand for.
    let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut aid.actions).into_iter().partition(|(t, _)| *t <= now);
    aid.actions = later;
    let mut acts: Vec<String> = Vec::new();
    for (_, a) in due {
        info!("test aid NVF_PLAY_TEST_PAUSE_MENU: {a}");
        if a == "shot" {
            let name = format!("{}pause_{}.png", if under_main { "main_menu_" } else { "" }, match *page {
                Page::Main if view.confirm.is_some() => "quit",
                Page::Main => "main",
                Page::Settings { .. } => "settings",
                Page::Controls { .. } => "controls",
                Page::Load { .. } => "load",
                Page::Mods => "mods",
            });
            info!("pause menu: screenshot {name}");
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(name));
        }
        acts.push(a);
    }
    let act = |name: &str| acts.iter().any(|a| a == name);
    let controller = bindings.as_ref().is_some_and(|b| b.device == InputDevice::Controller);
    let device = bindings.as_ref().map_or(InputDevice::KeyboardMouse, |b| b.device);
    // A control waiting for its new key takes the next key or mouse button, whatever it
    // is over (with a controller, the next controller button); Esc (Start) cancels.
    if let Page::Controls { selected, listening } = &mut *page {
        if *listening {
            let c = control_at(*selected);
            let key = keys.get_just_pressed().copied().find(|k| crate::bindings::key_name(*k).is_some() || *k == KeyCode::Escape);
            let start = pads.iter().any(|g| g.just_pressed(GamepadButton::Start));
            if key == Some(KeyCode::Escape) || (controller && start) {
                info!("controls: rebinding cancelled");
                *listening = false;
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Back });
                return;
            }
            let pad = crate::pad::PAD_NAMES.iter().map(|(p, _)| *p).filter(|p| !crate::pad::RESERVED_PAD.contains(p)).find(|p| match p {
                PadInput::Button(b) => pads.iter().any(|g| g.just_pressed(*b)),
                PadInput::Stick(d) => sticks.as_ref().is_some_and(|s| s.just_pressed(*d)),
            });
            let input = match key {
                _ if controller => None,
                Some(k) if crate::bindings::RESERVED_KEYS.contains(&k) => None,
                Some(k) => Some(Input::Key(k)),
                None => mouse.get_just_pressed().next().map(|m| Input::Mouse(*m)),
            };
            match (c, input, pad) {
                (Some(c), Some(input), _) => {
                    *listening = false;
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
                    commands.queue(move |world: &mut World| crate::bindings::rebind(world, c, input));
                }
                (Some(c), None, Some(p)) if controller => {
                    *listening = false;
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
                    commands.queue(move |world: &mut World| crate::bindings::rebind_pad(world, c, p));
                }
                _ => {}
            }
            return;
        }
    }
    // The Controls tab's first row switches the input device.
    let switch_device = |commands: &mut Commands| {
        let d = device.other();
        commands.queue(move |world: &mut World| crate::bindings::set_device(world, d));
    };
    // The Rumble row flips the controller's rumble.
    let rumble_on = bindings.as_ref().is_some_and(|b| b.rumble);
    let flip_rumble = |commands: &mut Commands| {
        commands.queue(move |world: &mut World| crate::bindings::set_rumble(world, !rumble_on));
    };
    // The Invert Y row flips the vertical look, the sensitivity slider steps.
    let invert_on = bindings.as_ref().is_some_and(|b| b.invert_y);
    let flip_invert = |commands: &mut Commands| {
        commands.queue(move |world: &mut World| crate::bindings::set_invert_y(world, !invert_on));
    };
    let sensitivity = bindings.as_ref().map_or(1.0, |b| b.sensitivity());
    let set_sensitivity = |commands: &mut Commands, m: f32| {
        commands.queue(move |world: &mut World| crate::bindings::set_sensitivity(world, m));
    };
    let mut clicked = None;
    // A list takes the row the pointer moves onto (not one it rests on while the keys
    // move the choice, nor the one a redrawn page puts under it).
    let moved = menu.motion.as_ref().is_some_and(|m| m.delta != Vec2::ZERO);
    let (main_before, confirm_before) = (view.main_selected, view.confirm);
    // The keep question or an open dropdown over the page: the rows under it do not follow
    // the pointer.
    let over = pending.is_some() || view.dropdown.is_some();
    // Each list's first row drawn: the pointer takes only a row in its window, so a row
    // left over from the window before (or one drawn past it) never moves the window.
    let shown_from = |rows: usize, w: Window| view.scroll.min(rows.saturating_sub(w.shown));
    let window = match *page {
        Page::Settings { selected } => {
            let r = group_rows(list, selected);
            Some((r.start + shown_from(r.len(), SETTINGS_WINDOW), SETTINGS_WINDOW))
        }
        Page::Controls { .. } => Some((shown_from(control_rows(controller), SETTINGS_WINDOW), SETTINGS_WINDOW)),
        Page::Load { .. } => Some((shown_from(menu.list.saves.len(), LOAD_WINDOW), LOAD_WINDOW)),
        Page::Main | Page::Mods => None,
    };
    let in_window = |b: &PauseButton| match (b, window) {
        (PauseButton::Switch(i) | PauseButton::ControlRow(i) | PauseButton::LoadRow(i), Some((first, w))) => w.holds(first, *i),
        _ => true,
    };
    for (interaction, b) in &buttons {
        match (interaction, b, &mut *page) {
            (PointerState::Hovered, b, _) if !in_window(b) => {}
            (PointerState::Hovered, PauseButton::Choice(r, k), _) if moved => {
                if let Some(o) = view.dropdown.as_mut().filter(|o| o.row == *r && (o.first..o.first + DROP_ROWS).contains(k)) {
                    o.at = *k;
                }
            }
            (PointerState::Hovered, PauseButton::Keep(k), _) if moved => view.keep_selected = *k,
            (PointerState::Hovered, ..) if over => {}
            (PointerState::Pressed, PauseButton::Meter(i), Page::Settings { selected } | Page::Controls { selected, .. }) => {
                *selected = *i;
                view.dragging = Some(*i);
            }
            (PointerState::Pressed, PauseButton::ScrollTrack, _) => view.bar_dragging = true,
            (PointerState::Pressed, ..) => clicked = Some(*b),
            (PointerState::Hovered, PauseButton::Switch(i), Page::Settings { selected }) if moved => *selected = *i,
            (PointerState::Hovered, PauseButton::ControlRow(i), Page::Controls { selected, .. }) if moved => *selected = *i,
            (PointerState::Hovered, PauseButton::LoadRow(i), Page::Load { selected }) if moved => *selected = *i,
            (PointerState::Hovered, PauseButton::Column(i), Page::Main) if moved && view.confirm.is_none() && !under_main => view.main_selected = *i,
            (PointerState::Hovered, PauseButton::Confirm(i), Page::Main) if moved && view.confirm.is_some() => view.confirm = Some(*i),
            _ => {}
        }
    }
    if act("defaults") && matches!(*page, Page::Settings { .. } | Page::Controls { .. }) {
        clicked = Some(PauseButton::Defaults);
    }
    // The keep question takes every key and click until it is answered.
    if pending.is_some() {
        let answer = match clicked {
            Some(PauseButton::Keep(k)) => Some(k),
            _ if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || act("revert") => Some(1),
            _ if act("keep") => Some(0),
            _ if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) => Some(view.keep_selected),
            _ => None,
        };
        let turn = [KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::KeyA, KeyCode::KeyD, KeyCode::KeyW, KeyCode::KeyS, KeyCode::Tab];
        match answer {
            Some(0) => {
                commands.queue(crate::settings::keep);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
            }
            Some(_) => {
                commands.queue(crate::settings::revert);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Back });
            }
            None if keys.any_just_pressed(turn) => {
                view.keep_selected = 1 - view.keep_selected.min(1);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
            }
            None => {}
        }
        if answer.is_some() {
            view.keep_selected = 0;
        }
        return;
    }
    // An open dropdown takes the keys and clicks until it closes.
    if let (Some(mut o), Page::Settings { .. }) = (view.dropdown, *page) {
        let names = view.drawn.as_ref().and_then(|d| d.list(o.row)).map(|(n, _)| n.to_vec()).unwrap_or_default();
        let n = names.len();
        let before = o.at;
        let mut take = None;
        let mut shut = false;
        match clicked {
            Some(PauseButton::Choice(r, k)) if r == o.row => take = Some(k),
            Some(_) => shut = true,
            None if mouse.just_pressed(MouseButton::Left) => shut = true,
            None if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || act("back") => shut = true,
            None if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || act("up") => o.at = o.at.saturating_sub(1),
            None if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || act("down") => o.at = (o.at + 1).min(n.saturating_sub(1)),
            None if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || act("pick") => take = Some(o.at),
            None => {}
        }
        let last = n.saturating_sub(DROP_ROWS);
        let wheel = menu.scroll.as_ref().map_or(0.0, |s| s.delta.y);
        if wheel > 0.0 {
            o.first = o.first.saturating_sub(1);
        } else if wheel < 0.0 {
            o.first = (o.first + 1).min(last);
        }
        if o.at != before {
            o.first = o.first.max((o.at + 1).saturating_sub(DROP_ROWS)).min(o.at).min(last);
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
        }
        if let Some(name) = take.and_then(|k| names.get(k)) {
            pick_choice(&mut commands, &mut sounds, o.row, name.clone());
            shut = true;
        } else if shut {
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Back });
        }
        view.dropdown = (!shut).then_some(o);
        return;
    }
    view.dropdown = None;
    // A click on the column's entry of another page (the page's own entry closes it), or
    // on a category.
    match clicked {
        Some(PauseButton::Column(i)) if !under_main && *page != Page::Main => {
            let closes = entries.get(i).is_some_and(|e| page.opened_by(e.label()));
            *page = Page::Main;
            view.main_selected = i;
            if closes {
                info!("pause menu: {} again, back to the column", entries[i].label());
                clicked = None;
            } else if entries.get(i).is_some_and(|e| matches!(e, Entry::Load | Entry::Settings | Entry::Mods)) {
                clicked = Some(PauseButton::Column(i));
            } else {
                clicked = None;
            }
        }
        Some(PauseButton::Column(i)) if under_main => {
            let label = main_menu.as_ref().and_then(|m| m.column().get(i).map(|c| c.0));
            match label {
                Some(l) if page.opened_by(l) => {
                    info!("pause menu: {l} again, back to the main menu");
                    *page = Page::Main;
                }
                Some(l) if l.starts_with("Load") => *page = Page::Load { selected: 0 },
                Some(l) if l.starts_with("Settings") => *page = Page::Settings { selected: 0 },
                _ => {}
            }
            if matches!(*page, Page::Load { .. }) {
                menu.list.open(menu.cfg.dir.as_deref());
            }
            clicked = None;
        }
        Some(PauseButton::Category(first)) => {
            *page = Page::Settings { selected: first };
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
        }
        Some(PauseButton::ControlsTab) => {
            *page = Page::Controls { selected: 0, listening: false };
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
        }
        Some(PauseButton::Defaults) if matches!(*page, Page::Controls { .. }) => {
            commands.queue(crate::bindings::reset);
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
        }
        Some(PauseButton::Defaults) => {
            if let Some(d) = view.defaults.clone() {
                info!("pause menu: settings back to their defaults");
                commands.queue(move |world: &mut World| {
                    for (i, v) in d.into_iter().enumerate() {
                        crate::settings::restore(world, i, v);
                    }
                });
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
            }
        }
        _ => {}
    }
    match &mut *page {
        Page::Main if view.confirm.is_some() => {
            let sel = view.confirm.unwrap_or(2);
            let mut take = None;
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || act("back") {
                take = Some(2);
            } else if let Some(PauseButton::Confirm(i)) = clicked {
                take = Some(i);
            } else if matches!(clicked, Some(PauseButton::Column(i)) if entries.get(i) == Some(&Entry::Quit)) {
                // Quit again: as Cancel.
                take = Some(2);
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || act("up") {
                view.confirm = Some(step_row(sel, -1, CONFIRM.len()));
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || act("down") {
                view.confirm = Some(step_row(sel, 1, CONFIRM.len()));
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || act("pick") {
                take = Some(sel);
            }
            match take {
                Some(0) => {
                    info!("pause menu: quit to the main menu");
                    to_main.write(MainMenuRequest);
                }
                Some(1) => {
                    info!("pause menu: quit");
                    if free_cam.editor {
                        to_main.write(MainMenuRequest);
                    } else {
                        exit.write(AppExit::Success);
                    }
                }
                Some(_) => {
                    info!("pause menu: quit cancelled");
                    view.confirm = None;
                    sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Back });
                }
                None => {}
            }
            if take.is_some_and(|t| t < 2) {
                sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Picked });
            }
        }
        Page::Main => {
            let n = entries.len();
            let mut take = match clicked {
                Some(PauseButton::Column(i)) => entries.get(i).copied(),
                _ => None,
            };
            if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
                view.main_selected = step_row(view.main_selected, -1, n);
            } else if keys.any_just_pressed([KeyCode::ArrowDown]) {
                view.main_selected = step_row(view.main_selected, 1, n);
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
                take = entries.get(view.main_selected).copied();
            }
            if keys.just_pressed(KeyCode::KeyQ) || act("quit") {
                take = Some(Entry::Quit);
            } else if keys.just_pressed(KeyCode::KeyN) || act("save") {
                take = Some(Entry::Save);
            } else if keys.just_pressed(KeyCode::KeyL) || act("load") {
                take = Some(Entry::Load);
            } else if keys.just_pressed(KeyCode::KeyS) {
                take = Some(Entry::Settings);
            } else if act("mods") && mods {
                take = Some(Entry::Mods);
            } else if keys.just_pressed(KeyCode::Escape) {
                take = Some(Entry::Continue);
            }
            if let Some(e) = take {
                view.main_selected = entries.iter().position(|x| *x == e).unwrap_or(0);
                if e != Entry::Continue {
                    sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Picked });
                }
            }
            match take {
                Some(Entry::Quit) => {
                    info!("pause menu: quit?");
                    view.confirm = Some(2);
                }
                Some(Entry::Save) => {
                    let name = new_save_name(&menu.cfg);
                    info!("pause menu: saving as {name:?}");
                    match menu.cfg.path_of(&name) {
                        Ok(path) => {
                            menu.status.waiting_for = Some(path);
                            menu.status.line = format!("Saving \"{name}\"...");
                            menu.saves.write(crate::save::SaveRequest { name, kind: SaveKind::Manual });
                        }
                        Err(e) => menu.status.line = format!("Cannot save: {e}"),
                    }
                }
                Some(Entry::Load) => {
                    info!("pause menu: load page");
                    menu.list.open(menu.cfg.dir.as_deref());
                    *page = Page::Load { selected: 0 };
                }
                Some(Entry::Settings) => {
                    info!("pause menu: settings");
                    *page = Page::Settings { selected: 0 };
                }
                Some(Entry::Mods) => {
                    info!("pause menu: mods page");
                    *page = Page::Mods;
                }
                Some(Entry::Continue) => {
                    info!("pause menu: resume");
                    next.set(PlayState::Playing);
                }
                None => {}
            }
        }
        Page::Settings { selected } => {
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back") {
                *page = Page::Main;
            } else if let Some(PauseButton::Switch(i)) = clicked {
                *selected = i;
                // A click on a slider's row only chooses it; on a list's it opens the list.
                if is_list(i) {
                    open_dropdown(&mut view, i);
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
                } else if !is_slider(i) {
                    flips.push(i);
                }
            } else if is_list(*selected) && keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA, KeyCode::ArrowRight, KeyCode::KeyD]) {
                let step = if keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) { -1 } else { 1 };
                if let Some(name) = neighbour(&view, *selected, step) {
                    picks.push((*selected, name));
                }
            } else if is_list(*selected) && (keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || act("pick")) {
                open_dropdown(&mut view, *selected);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
            } else if let Some(PauseButton::SliderStep(i, step)) = clicked {
                *selected = i;
                steps.push((i, step));
            } else if is_slider(*selected) && keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
                steps.push((*selected, -1));
            } else if is_slider(*selected) && keys.any_just_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
                steps.push((*selected, 1));
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
                *selected = step_in_group(list, *selected, -1);
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
                *selected = step_in_group(list, *selected, 1);
            } else if keys.any_just_pressed([KeyCode::Tab, KeyCode::PageDown]) {
                // After the last category, Controls.
                let next = step_category(list, *selected, 1);
                if next <= *selected {
                    *page = Page::Controls { selected: 0, listening: false };
                } else {
                    *selected = next;
                }
            } else if keys.just_pressed(KeyCode::PageUp) {
                let before = step_category(list, *selected, -1);
                if before >= group_rows(list, *selected).start {
                    *page = Page::Controls { selected: 0, listening: false };
                } else {
                    *selected = before;
                }
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space, KeyCode::ArrowLeft, KeyCode::ArrowRight]) && *selected < list.len() && !is_slider(*selected) {
                flips.push(*selected);
            }
            // A drag on a meter: the value under the pointer while the button is held
            // (the page is drawn again as the value changes, so the meter is looked up
            // by its row each frame).
            if let Some(i) = view.dragging {
                if mouse.pressed(MouseButton::Left) {
                    let under = meters.iter().find(|(b, _)| **b == PauseButton::Meter(i)).and_then(|(_, at)| at.normalized);
                    if let Some(p) = under {
                        let v = (p.x + 0.5).clamp(0.0, 1.0);
                        if list.get(i).and_then(|s| s.slider).is_some_and(|s| s.fit(v) != s.fit(view_value(&view, i))) {
                            sets.push((i, v));
                        }
                    }
                } else {
                    view.dragging = None;
                }
            }
        }
        Page::Controls { selected, listening } => {
            let n = control_rows(controller);
            // Back to keyboard and mouse on the Rumble row: the last control.
            *selected = (*selected).min(n - 1);
            let control = control_at(*selected);
            let on_rumble = *selected == rumble_row();
            let on_invert = *selected == invert_row();
            let on_sensitivity = *selected == sensitivity_row();
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back") {
                *page = Page::Main;
            } else if let Some(PauseButton::ControlRow(i)) = clicked {
                *selected = i;
                if control_at(i).is_some() {
                    *listening = true;
                } else if i == invert_row() {
                    flip_invert(&mut commands);
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
                } else if i == sensitivity_row() {
                    // A click on the slider's row away from its meter and arrows only chooses it.
                } else if i == rumble_row() {
                    flip_rumble(&mut commands);
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
                } else {
                    switch_device(&mut commands);
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
                }
            } else if on_rumble && keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space, KeyCode::ArrowLeft, KeyCode::ArrowRight]) {
                flip_rumble(&mut commands);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
            } else if on_invert && keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space, KeyCode::ArrowLeft, KeyCode::ArrowRight]) {
                flip_invert(&mut commands);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
            } else if let Some(PauseButton::SliderStep(i, step)) = clicked {
                *selected = i;
                set_sensitivity(&mut commands, crate::bindings::step_sensitivity(sensitivity, step));
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
            } else if on_sensitivity && keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA, KeyCode::ArrowRight, KeyCode::KeyD]) {
                let step = if keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) { -1 } else { 1 };
                set_sensitivity(&mut commands, crate::bindings::step_sensitivity(sensitivity, step));
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
            } else if on_sensitivity && keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
                // Enter on the slider does nothing, as on the settings page's sliders.
            } else if control.is_none() && keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space, KeyCode::ArrowLeft, KeyCode::ArrowRight]) {
                switch_device(&mut commands);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
                *selected = step_row(*selected, -1, n);
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
                *selected = step_row(*selected, 1, n);
            } else if keys.any_just_pressed([KeyCode::Tab, KeyCode::PageDown]) {
                // After Controls, the first category again.
                *page = Page::Settings { selected: 0 };
            } else if keys.just_pressed(KeyCode::PageUp) {
                *page = Page::Settings { selected: categories(list).last().map_or(0, |c| c.1) };
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
                *listening = true;
            } else if let (true, Some(c)) = (keys.just_pressed(KeyCode::Delete), control) {
                commands.queue(move |world: &mut World| crate::bindings::clear_key(world, c));
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
            }
            // A drag on the sensitivity meter: the value under the pointer while the button is
            // held, as the settings page's sliders.
            if let Some(i) = view.dragging {
                if mouse.pressed(MouseButton::Left) {
                    let under = meters.iter().find(|(b, _)| **b == PauseButton::Meter(i)).and_then(|(_, at)| at.normalized);
                    if let Some(p) = under {
                        let m = crate::bindings::sensitivity_at_part(p.x + 0.5);
                        if (m - sensitivity).abs() > 1e-4 {
                            set_sensitivity(&mut commands, m);
                        }
                    }
                } else {
                    view.dragging = None;
                }
            }
            if let (Page::Controls { listening: true, selected }, Some(_)) = (*page, control) {
                if let Some(c) = control_at(selected) {
                    info!("controls: {} waits for its new {}", Bindings::name(c), if controller { "controller button" } else { "key" });
                }
            }
        }
        // The mod's page takes the pointer and the keys through its tiles
        // (`crate::tiles::input`); Esc, Backspace or Back go back to the column.
        Page::Mods => {
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back") {
                info!("pause menu: mods page closed");
                *page = Page::Main;
            }
        }
        Page::Load { selected } if view.delete_ask.is_some() => {
            // The delete question has the keys (module notes).
            let yes = keys.any_just_pressed([KeyCode::KeyY, KeyCode::Enter, KeyCode::NumpadEnter]) || clicked == Some(PauseButton::DeleteAnswer(true)) || act("yes");
            let no = keys.any_just_pressed([KeyCode::KeyN, KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::DeleteAnswer(false)) || act("no") || act("back");
            if yes {
                let row = view.delete_ask.take().unwrap_or(*selected);
                if let Some((path, _)) = menu.list.saves.get(row).cloned() {
                    match delete_save(&path) {
                        Ok(()) => info!("pause menu: deleted {}", path.display()),
                        Err(e) => warn!("pause menu: could not delete {}: {e}", path.display()),
                    }
                    menu.list.open(menu.cfg.dir.as_deref());
                    *selected = (*selected).min(menu.list.saves.len().saturating_sub(1));
                }
            } else if no {
                view.delete_ask = None;
                info!("pause menu: delete cancelled");
            }
        }
        Page::Load { selected } => {
            let rows = menu.list.saves.len();
            let mut pick = None;
            if (keys.just_pressed(KeyCode::Delete) || clicked == Some(PauseButton::Delete) || act("delete")) && *selected < rows {
                view.delete_ask = Some(*selected);
                info!("pause menu: delete {}?", menu.list.saves.get(*selected).map(|(p, _)| stem(p)).unwrap_or_default());
            } else if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back") {
                *page = Page::Main;
            } else if let Some(PauseButton::LoadRow(i)) = clicked {
                *selected = i;
                pick = Some(i);
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || act("up") {
                *selected = (*selected).min(rows.saturating_sub(1)).saturating_sub(1);
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || act("down") {
                *selected = (*selected + 1).min(rows.saturating_sub(1));
            } else if (keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || act("pick")) && *selected < rows {
                pick = Some(*selected);
            }
            if let Some((path, _)) = pick.and_then(|i| menu.list.saves.get(i)) {
                info!("pause menu: load {}", path.display());
                menu.loads.write(crate::save::LoadRequest { path: path.clone() });
            }
        }
    }
    // The settings page's, the Controls tab's and the load page's window of rows: the
    // wheel, the scroll bar's arrows and its marker move it, and the choice with it where
    // it would leave the window; else the window follows the choice (the keys'; the
    // pointer takes only rows in the window, so it never moves it).
    let window = match *page {
        Page::Settings { selected } => {
            let r = group_rows(list, selected);
            Some((r.len(), selected.saturating_sub(r.start), SETTINGS_WINDOW))
        }
        Page::Controls { selected, .. } => Some((control_rows(controller), selected, SETTINGS_WINDOW)),
        Page::Load { selected } if view.delete_ask.is_none() => Some((menu.list.saves.len(), selected, LOAD_WINDOW)),
        Page::Load { selected } => {
            view.scroll = crate::menu::model::keep_in_view(view.scroll, selected, menu.list.saves.len(), LOAD_ROWS);
            None
        }
        Page::Main | Page::Mods => None,
    };
    if let Some((n, sel, w)) = window.filter(|w| w.0 > 0) {
        use bevy::input::mouse::MouseScrollUnit;
        view.wheel += menu.scroll.as_ref().map_or(0.0, |s| match s.unit {
            MouseScrollUnit::Line => s.delta.y,
            MouseScrollUnit::Pixel => s.delta.y / WHEEL_PIXELS,
        });
        let notches = view.wheel.trunc();
        view.wheel -= notches;
        // The wheel turned down shows the rows further down.
        let mut by = -(notches as i64) + i64::from(act("wheel-down")) - i64::from(act("wheel-up"));
        if let Some(PauseButton::ScrollStep(s)) = clicked {
            by += i64::from(s);
        }
        let last = n.saturating_sub(w.shown);
        let mut first = view.scroll.min(last);
        let mut scrolled = by != 0;
        if view.bar_dragging {
            if mouse.pressed(MouseButton::Left) {
                if let Some(p) = meters.iter().find(|(b, _)| **b == PauseButton::ScrollTrack).and_then(|(_, at)| at.normalized) {
                    first = w.first_at(n, p.y + 0.5);
                    scrolled = true;
                }
            } else {
                view.bar_dragging = false;
            }
        }
        let sel = if scrolled {
            first = (first as i64 + by).clamp(0, last as i64) as usize;
            sel.clamp(first, (first + w.shown).min(n) - 1)
        } else {
            first = crate::menu::model::keep_in_view(first, sel, n, w.shown);
            sel
        };
        if first != view.scroll {
            info!("pause menu: rows {}-{} of {} shown", first + 1, (first + w.shown).min(n), n);
            view.scroll = first;
        }
        match &mut *page {
            Page::Settings { selected } => *selected = group_rows(list, *selected).start + sel,
            Page::Controls { selected, .. } | Page::Load { selected } => *selected = sel,
            Page::Main | Page::Mods => {}
        }
    }
    if let Page::Load { selected } = *page {
        let SaveMenu { list, images, .. } = &mut menu;
        list.ensure_thumb(selected, images);
    }
    // The pointer or the keys moved the first page's choice or the question's: its sound.
    if view.main_selected != main_before && *page == Page::Main || (view.confirm.is_some() && confirm_before.is_some() && view.confirm != confirm_before) {
        sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Focus });
    }
    for i in flips {
        if view_disabled(&view, i) { continue; }
        sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
        commands.queue(move |world: &mut World| {
            crate::settings::flip(world, i);
        });
    }
    for (i, n) in steps {
        if view_disabled(&view, i) { continue; }
        sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
        commands.queue(move |world: &mut World| {
            crate::settings::step_slider(world, i, n);
        });
    }
    for (i, v) in sets {
        if view_disabled(&view, i) { continue; }
        commands.queue(move |world: &mut World| {
            crate::settings::set_slider(world, i, v);
        });
    }
    for (i, name) in picks {
        if view_disabled(&view, i) { continue; }
        pick_choice(&mut commands, &mut sounds, i, name);
    }
}

/// Take choice `name` of list row `i` (`crate::settings::pick`).
fn pick_choice(commands: &mut Commands, sounds: &mut MessageWriter<crate::ui_sounds::UiSound>, i: usize, name: String) {
    use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
    commands.queue(move |world: &mut World| {
        crate::settings::pick(world, i, &name);
    });
}

/// Open list row `i`'s dropdown on its chosen one.
fn open_dropdown(view: &mut PauseView, i: usize) {
    if view_disabled(view, i) { return; }
    let Some((names, chosen)) = view.drawn.as_ref().and_then(|d| d.list(i)) else { return };
    let at = names.iter().position(|n| n == chosen).unwrap_or(0);
    let first = (at + 1).saturating_sub(DROP_ROWS).min(names.len().saturating_sub(DROP_ROWS));
    info!("pause menu: the list of {} choice(s) open", names.len());
    view.dropdown = Some(Dropdown { row: i, at, first });
}

/// The choice `step` (-1 or 1) from list row `i`'s chosen one, not past the ends.
fn neighbour(view: &PauseView, i: usize, step: i32) -> Option<String> {
    if view_disabled(view, i) { return None; }
    let (names, chosen) = view.drawn.as_ref()?.list(i)?;
    let at = names.iter().position(|n| n == chosen)? as i32 + step;
    names.get(usize::try_from(at).ok()?).cloned()
}

/// Whether row `i` was disabled when the page was last drawn.
fn view_disabled(view: &PauseView, i: usize) -> bool {
    view.drawn.as_ref().and_then(|d| d.disabled.get(i)).is_some_and(Option::is_some)
}

/// Row `i`'s value as the page last drew it.
fn view_value(view: &PauseView, i: usize) -> f32 {
    view.drawn.as_ref().and_then(|d| d.states.get(i).copied()).unwrap_or(0.0)
}

/// The main page's status line: the save asked from this menu, once written.
fn show_status(mut done: MessageReader<crate::save::SaveDone>, mut status: ResMut<PauseStatus>) {
    for d in done.read() {
        if status.waiting_for.as_ref() != Some(&d.path) {
            continue;
        }
        status.waiting_for = None;
        status.line = match &d.result {
            Ok(()) => format!("Saved \"{}\"", stem(&d.path)),
            Err(e) => format!("Save failed: {e}"),
        };
    }
}

/// What the page shows now.
fn drawn_now(world: &mut World) -> Drawn {
    let view_w = world.query_filtered::<&bevy::window::Window, With<bevy::window::PrimaryWindow>>().iter(world).next().filter(|w| w.height() > 0.0).map_or(VIEW_W, |w| 960.0 * w.width() / w.height()).round() as u32;
    let page = *world.resource::<Page>();
    let list = world.resource::<Settings>().list().to_vec();
    let states: Vec<f32> = world.resource::<Settings>().values(world);
    let disabled: Vec<_> = (0..list.len()).map(|i| world.resource::<Settings>().disabled_status(world, i)).collect();
    let lists: Vec<(usize, Vec<String>, String)> = {
        let settings = world.resource::<Settings>();
        (0..list.len()).filter_map(|i| settings.choices(i).map(|c| (i, (c.names)(world), (c.chosen)(world)))).collect()
    };
    let keep = world.get_resource::<crate::settings::PendingKeep>().map(|p| {
        let now = world.get_resource::<Time<Real>>().map_or(0.0, |t| t.elapsed_secs_f64());
        (list.get(p.row).map_or("choice", |s| s.label), p.left(now), world.resource::<PauseView>().keep_selected)
    });
    if matches!(page, Page::Settings { .. }) && world.resource::<PauseView>().defaults.is_none() {
        let start = world.get_resource::<crate::settings::StartStates>().map(|s| s.0.clone()).filter(|s| s.len() == states.len());
        world.resource_mut::<PauseView>().defaults = Some(start.unwrap_or_else(|| states.clone()));
    }
    let main = world.get_resource::<crate::main_menu::MainMenu>().filter(|m| m.open).map(|m| m.column());
    let view = world.resource::<PauseView>();
    let thumb = world.resource::<LoadList>().thumb.as_ref().map(|(i, h)| (*i, h.is_some()));
    Drawn {
        page,
        main_selected: view.main_selected,
        confirm: view.confirm,
        states,
        disabled,
        status: world.resource::<PauseStatus>().line.clone(),
        under_main: main.is_some(),
        column: main.unwrap_or_else(|| entries(world.get_resource::<ModsEntry>().is_some_and(|m| m.0)).iter().map(|e| (e.label(), true)).collect()),
        thumb,
        controls: world.get_resource::<crate::bindings::Bindings>().map(|b| b.all().map(|(c, _)| b.text_on(c)).collect()).unwrap_or_default(),
        device: world.get_resource::<crate::bindings::Bindings>().map_or(("Keyboard and Mouse", false), |b| (b.device.name(), b.device == crate::bindings::InputDevice::Controller)),
        rumble: world.get_resource::<crate::bindings::Bindings>().is_some_and(|b| b.rumble),
        invert_y: world.get_resource::<crate::bindings::Bindings>().is_some_and(|b| b.invert_y),
        sensitivity: world.get_resource::<crate::bindings::Bindings>().map_or(1.0, |b| b.sensitivity()),
        scroll: view.scroll,
        lists,
        dropdown: view.dropdown.filter(|_| matches!(page, Page::Settings { .. })),
        keep,
        delete_ask: view.delete_ask.filter(|_| matches!(page, Page::Load { .. })),
        view_w,
        language: fnv_util::words::generation(),
    }
}

/// Draw the page again when what it shows changed.
fn draw(world: &mut World) {
    // A film holds the game as the pause menu does (`crate::intro`) but shows no page.
    if !world.resource::<Paused>().0 || world.get_resource::<crate::intro::Intro>().is_some_and(|i| i.is_playing()) {
        return;
    }
    let now = drawn_now(world);
    if world.resource::<PauseView>().drawn.as_ref() == Some(&now) {
        return;
    }
    if !world.contains_resource::<PauseLook>() {
        let look = PauseLook::load(world);
        world.insert_resource(look);
    }
    let roots: Vec<Entity> = world.query_filtered::<Entity, With<PauseRoot>>().iter(world).collect();
    for e in roots {
        world.despawn(e);
    }
    let settings = world.resource::<Settings>().list().to_vec();
    let glyphs = world.resource::<crate::pad::PadGlyphs>().clone();
    world.resource_scope(|world, look: Mut<PauseLook>| {
        world.resource_scope(|world, list: Mut<LoadList>| {
            let mut commands = world.commands();
            spawn_page(&mut commands, &now, &look, &settings, &list, &glyphs);
        });
    });
    world.flush();
    world.resource_mut::<PauseView>().drawn = Some(now);
}

fn close(mut commands: Commands, mut paused: ResMut<Paused>, mut page: ResMut<Page>, mut list: ResMut<LoadList>, mut view: ResMut<PauseView>, roots: Query<Entity, With<PauseRoot>>) {
    paused.0 = false;
    *page = Page::Main;
    *list = LoadList::default();
    view.drawn = None;
    view.confirm = None;
    view.dragging = None;
    view.bar_dragging = false;
    view.scroll = 0;
    view.dropdown = None;
    for e in &roots {
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_page_fits_the_view() {
        // 16:9 (1280x720, 1920x1080, 2560x1440 are all 1707 units wide), 16:10, 21:9.
        for (view_w, tabs_w) in [(VIEW_W, SUB_WIDTH), (VIEW_W, 220.0), (1536.0, 220.0), (2240.0, 220.0)] {
            let left = options_left(view_w, tabs_w);
            let band_right = view_w / 2.0 + left + OPTIONS_WIDTH + BAR_GAP + BAR_W + BAND_MARGIN;
            let tabs_left = view_w - SUB_RIGHT - tabs_w;
            assert!(band_right + TABS_GAP <= tabs_left + 0.01, "{view_w}: the band ends at {band_right}, the categories start at {tabs_left}");
            assert!(view_w / 2.0 + left - BAND_MARGIN >= 0.0, "{view_w}: the band leaves the view");
            assert!(left <= OPTIONS_LEFT);
        }
        // Wide views keep the old place; a 4:3 view keeps the band on it.
        assert_eq!(options_left(2240.0, 220.0), OPTIONS_LEFT);
        assert_eq!(options_left(1280.0, 220.0), BAND_MARGIN - 640.0);
        // Nine rows and a note of up to eleven lines fit above the version's line.
        let note_top = OPTIONS_TOP + OPTION_ROWS as f32 * OPTION_ROW + 20.0;
        assert_eq!(note_fit(4, note_top), 4);
        assert_eq!(note_fit(30, note_top), 11);
        assert!(note_top + 11.0 * NOTE_STEP <= NOTE_BOTTOM && NOTE_BOTTOM < 960.0);
    }

    /// The systems that read or write the world aids' clock (directly or through
    /// [`AidTime`]) build into a schedule: Bevy checks each system's parameters for a
    /// resource taken twice (B0002) when it is initialized, which unit tests of the clock
    /// alone never do. `start_world_aids` once took the clock in `AidTime` and as
    /// `ResMut` and every `--play` run panicked (2026-10-18). The parameters are the same
    /// whatever NVF_PLAY_AIDS_FROM_SCRIPTS says; it is set as the smoke runner sets it.
    #[test]
    fn the_world_aids_clock_systems_build_into_a_schedule() {
        // SAFETY: this test's own variable; no other test reads it.
        unsafe { std::env::set_var("NVF_PLAY_AIDS_FROM_SCRIPTS", "1") };
        let mut world = World::new();
        world.insert_resource(WorldAidClock::from_env());
        let mut schedule = Schedule::default();
        schedule.add_systems((crate::script::run_scripts, crate::script::start_world_aids, tick_play_clock).chain());
        schedule.initialize(&mut world).expect("the schedule builds");
        assert!(world.resource::<WorldAidClock>().from_scripts);
    }

    #[test]
    fn the_world_aids_clock_waits_for_the_scripts_and_is_held_back_only_by_a_late_start() {
        // Without the switch: the aids' clock, from the start.
        let mut off = WorldAidClock::new(false);
        assert_eq!(off.at(3.0), Some(3.0));
        assert_eq!(off.start(20.0), None);
        assert_eq!(off.at(25.0), Some(25.0));
        assert_eq!(off.held_secs(), 0.0);
        // With it: nothing before the scripts run.
        let mut on = WorldAidClock::new(true);
        assert_eq!(on.at(3.0), None);
        // An unbusy start (before SCRIPTS_START_SECONDS): not held back, the times mean
        // what they did; steps due already run in the first frame.
        assert_eq!(on.start(5.5), Some(0.0));
        assert_eq!(on.at(5.5), Some(5.5));
        assert_eq!(on.at(14.0), Some(14.0));
        // Only the first start counts.
        assert_eq!(on.start(30.0), None);
        assert_eq!(on.at(14.0), Some(14.0));
        // A busy start at 20 s: the clock reads SCRIPTS_START_SECONDS then and steps keep
        // their distances (a step 6 s after that runs 6 s after the start).
        let mut late = WorldAidClock::new(true);
        assert_eq!(late.start(20.0), Some(20.0 - SCRIPTS_START_SECONDS));
        assert_eq!(late.at(20.0), Some(SCRIPTS_START_SECONDS));
        assert_eq!(late.at(26.0), Some(SCRIPTS_START_SECONDS + 6.0));
        assert_eq!(late.held_secs(), 20.0 - SCRIPTS_START_SECONDS);
    }

    #[test]
    fn the_pause_menu_always_holds_the_picture_and_other_menus_by_the_switch() {
        use PlayState::*;
        for mode in [MenuPause::Living, MenuPause::Original] {
            // The pause menu (a `Menu` state with `Paused`), whatever the switch says.
            assert!(picture_held(mode, true, Menu));
            // In the world nothing is held.
            assert!(!picture_held(mode, false, Playing));
        }
        // The inventory, a container, a message box (`Menu`) and the dialogue screen.
        assert!(!picture_held(MenuPause::Living, false, Menu));
        assert!(!picture_held(MenuPause::Living, false, Dialogue));
        assert!(picture_held(MenuPause::Original, false, Menu));
        assert!(picture_held(MenuPause::Original, false, Dialogue));
    }

    #[test]
    fn a_loading_screen_holds_the_picture_in_both_modes() {
        // A load from the main menu or the pause menu clears `Paused` as the menu closes
        // under the screen; the world behind it must still stand still.
        for mode in [MenuPause::Living, MenuPause::Original] {
            assert!(picture_held(mode, false, PlayState::Loading));
        }
    }

    #[test]
    fn the_pause_aid_parses_and_fires_once() {
        let mut a = parse_pause_aid("20:24, 30:31.5").unwrap();
        assert_eq!(a, PauseAid { open: vec![20.0, 30.0], resume: vec![24.0, 31.5], ..default() });
        assert!(!fire(&mut a.open, 19.9));
        assert!(fire(&mut a.open, 20.0));
        assert!(!fire(&mut a.open, 20.1));
        assert!(fire(&mut a.open, 40.0) && a.open.is_empty());
        assert!(parse_pause_aid("20").is_err());
        assert!(parse_pause_aid("24:20").is_err());
        assert!(parse_pause_aid("x:20").is_err());
        assert_eq!(parse_pause_aid("").unwrap(), PauseAid::default());
    }

    #[test]
    fn the_settings_aid_parses() {
        assert_eq!(parse_settings_aid("21:page, 22.5:menu-pause").unwrap(), vec![(21.0, "page".to_string()), (22.5, "menu-pause".to_string())]);
        assert!(parse_settings_aid("21").is_err());
        assert!(parse_settings_aid("x:voice").is_err());
        assert!(parse_settings_aid("").unwrap().is_empty());
    }

    #[test]
    fn the_keys_stay_in_a_category_and_tab_goes_to_the_next() {
        let s = |key: &'static str, group: &'static str| Setting { key, group, label: key, note: "", get: |_| false, set: |_, _| {}, slider: None };
        let list = [s("a", "Gameplay"), s("b", "Gameplay"), s("c", "Display"), s("d", "Display"), s("e", "Display"), s("f", "Debug")];
        assert_eq!(categories(&list), vec![("Gameplay", 0), ("Display", 2), ("Debug", 5)]);
        assert_eq!(group_rows(&list, 3), 2..5);
        assert_eq!(step_in_group(&list, 4, 1), 2);
        assert_eq!(step_in_group(&list, 2, -1), 4);
        assert_eq!(step_in_group(&list, 5, 1), 5);
        assert_eq!(step_category(&list, 1, 1), 2);
        assert_eq!(step_category(&list, 5, 1), 0);
        assert_eq!(step_category(&list, 0, -1), 5);
    }

    #[test]
    fn the_mods_entry_stands_before_quit_only_when_a_mod_added_one() {
        let labels = |mods| entries(mods).iter().map(|e| e.label()).collect::<Vec<_>>();
        assert_eq!(labels(false), ["Continue", "Save", "Load", "Settings", "Quit"]);
        assert_eq!(labels(true), ["Continue", "Save", "Load", "Settings", "Mods", "Quit"]);
        assert!(Page::Mods.opened_by("Mods"));
        assert!(!Page::Mods.opened_by("Settings"));
        assert_eq!(Page::Mods.settings_row(), None);
        assert!(mods_open(&Paused(true), &Page::Mods));
        assert!(!mods_open(&Paused(false), &Page::Mods));
        assert!(!mods_open(&Paused(true), &Page::Main));
    }

    #[test]
    fn a_page_is_closed_by_its_own_entry() {
        // The pause menu's labels and the main menu's ("Load Game").
        assert!(Page::Settings { selected: 3 }.opened_by("Settings"));
        assert!(Page::Load { selected: 0 }.opened_by("Load"));
        assert!(Page::Load { selected: 0 }.opened_by("Load Game"));
        assert!(!Page::Load { selected: 0 }.opened_by("Settings"));
        assert!(!Page::Settings { selected: 0 }.opened_by("Load Game"));
        assert!(!Page::Main.opened_by("Settings"));
    }

    #[test]
    fn the_keys_go_round_the_rows() {
        assert_eq!(step_row(0, 1, 3), 1);
        assert_eq!(step_row(2, 1, 3), 0);
        assert_eq!(step_row(0, -1, 3), 2);
        assert_eq!(step_row(7, -1, 3), 1);
        assert_eq!(step_row(0, 1, 0), 0);
    }

    #[test]
    fn the_window_moves_only_as_far_as_keeps_the_choice_in_view() {
        // Down past the window's foot: it moves one row; back up inside it: it stays.
        assert_eq!(follow(0, 8, 20), 0);
        assert_eq!(follow(0, 9, 20), 1);
        assert_eq!(follow(1, 5, 20), 1);
        assert_eq!(follow(5, 2, 20), 2);
        // Never past the last full window, nor anywhere when all rows fit.
        assert_eq!(follow(15, 19, 20), 11);
        assert_eq!(follow(3, 2, 6), 0);
    }

    #[test]
    fn the_scroll_bar_shows_only_when_rows_are_hidden_and_its_marker_follows_the_list() {
        let w = SETTINGS_WINDOW;
        assert_eq!(w.marker(9, 0), None);
        assert_eq!(w.marker(4, 0), None);
        let (top, h) = w.marker(30, 0).unwrap();
        assert_eq!(top, 0.0);
        assert!((h - w.track() * 9.0 / 30.0).abs() < 1e-3);
        let (end, _) = w.marker(30, 21).unwrap();
        assert!((end + h - w.track()).abs() < 1e-3);
        // A press at the track's ends scrolls to the list's ends, its middle halfway.
        assert_eq!(w.first_at(30, 0.0), 0);
        assert_eq!(w.first_at(30, 1.0), 21);
        assert_eq!(w.first_at(30, 0.5), 11);
        assert_eq!(w.first_at(5, 0.7), 0);
        // The load page's bar shows past eight saves and reaches the last eight.
        assert_eq!(LOAD_WINDOW.marker(8, 0), None);
        assert!(LOAD_WINDOW.marker(21, 0).is_some());
        assert_eq!(LOAD_WINDOW.first_at(21, 1.0), 13);
    }

    #[test]
    fn the_pointer_takes_only_rows_in_the_window() {
        // Rows 5-12 shown: a row drawn before the window moved (4 or 13) is not taken.
        assert!(LOAD_WINDOW.holds(5, 5) && LOAD_WINDOW.holds(5, 12));
        assert!(!LOAD_WINDOW.holds(5, 4) && !LOAD_WINDOW.holds(5, 13));
        assert!(SETTINGS_WINDOW.holds(0, 8) && !SETTINGS_WINDOW.holds(0, 9));
    }

    #[test]
    fn a_new_save_takes_the_next_number() {
        assert_eq!(next_save_name([]), "Save 1");
        assert_eq!(next_save_name(["quicksave", "Save 2", "Save 10", "Save x", "autosave"]), "Save 11");
    }

    #[test]
    fn the_load_page_says_times_plainly() {
        assert_eq!(play_time(59.0), "0 min");
        assert_eq!(play_time(3.0 * 3600.0 + 7.0 * 60.0), "3 h 07 min");
        assert_eq!(saved_ago(1000, 1030), "just now");
        assert_eq!(saved_ago(1000, 1000 + 5 * 60), "5 min ago");
        assert_eq!(saved_ago(1000, 1000 + 2 * 86400), "2 days ago");
        assert_eq!(saved_ago(2000, 1000), "just now");
    }

    #[test]
    fn the_load_page_lists_the_saves_and_shows_the_chosen_one() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>().init_resource::<crate::pad::PadGlyphs>();
        let mut list = LoadList::default();
        for (name, t) in [("Save 2", 20), ("autosave", 10)] {
            let mut h = SaveHeader::new(if name == "autosave" { SaveKind::Auto } else { SaveKind::Manual });
            h.saved_unix = t;
            h.player_name = "Courier".into();
            h.location = "Goodsprings".into();
            list.saves.push((PathBuf::from(format!("{name}.nvfsave")), h));
        }
        app.insert_resource(list);
        let drawn = Drawn { page: Page::Load { selected: 1 }, main_selected: 0, confirm: None, states: vec![], disabled: vec![], status: String::new(), column: entries(false).iter().map(|e| (e.label(), true)).collect(), under_main: false, thumb: None, controls: vec![], device: ("Keyboard and Mouse", false), rumble: true, invert_y: false, sensitivity: 1.0, scroll: 0, lists: vec![], dropdown: None, keep: None, delete_ask: None, view_w: VIEW_W as u32, language: 0 };
        app.world_mut().run_system_cached_with(|In(d): In<Drawn>, mut commands: Commands, list: Res<LoadList>, glyphs: Res<crate::pad::PadGlyphs>| spawn_page(&mut commands, &d, &PauseLook::plain(), &[], &list, &glyphs), drawn).unwrap();
        let world = app.world_mut();
        let mut rows = world.query::<(&PauseButton, &BorderColor)>();
        let rows: Vec<(PauseButton, bool)> = rows.iter(world).filter(|(b, _)| matches!(b, PauseButton::LoadRow(_))).map(|(b, c)| (*b, c.top != Color::NONE)).collect();
        assert_eq!(rows.len(), 2);
        // The chosen row has the box round it, and only it.
        assert!(rows.contains(&(PauseButton::LoadRow(1), true)) && rows.contains(&(PauseButton::LoadRow(0), false)));
        // Load is lit in the column, the others dimmed.
        let mut texts = world.query::<(&Text, &TextColor)>();
        let all: Vec<(String, Color)> = texts.iter(world).map(|(t, c)| (t.0.clone(), c.0)).collect();
        assert!(all.contains(&("Load".to_string(), color(Shade::Bright))) && all.contains(&("Save".to_string(), color(Shade::Dim))), "{all:?}");
        let mut details = world.query_filtered::<&Text, With<DetailLine>>();
        let details: Vec<String> = details.iter(world).map(|t| t.0.clone()).collect();
        assert!(details.iter().any(|t| t == "Courier, level 1"), "{details:?}");
        // The stand-in font sets no spaces.
        assert!(all.iter().any(|t| t.0 == "Nopicture"), "{all:?}");
    }

    #[derive(Resource, Default)]
    struct Lamp(bool);

    #[test]
    fn the_settings_page_shows_each_switch_as_the_game_has_it() {
        let mut app = App::new();
        app.init_resource::<Lamp>().init_resource::<Paused>().insert_resource(Page::Settings { selected: 0 });
        add_setting(&mut app, Setting { key: "lamp", group: "Test", label: "Lamp", note: "A lamp.", get: |w| w.resource::<Lamp>().0, set: |w, on| w.resource_mut::<Lamp>().0 = on, slider: None });
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().init_resource::<crate::pad::PadGlyphs>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        let state = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.single(world).unwrap().1 .0.clone()
        };
        assert_eq!(state(&mut app), "OFF");
        crate::settings::flip(app.world_mut(), 0);
        assert_eq!(state(&mut app), "ON");
        // Defaults go back to the state the page was first shown with.
        assert_eq!(app.world().resource::<PauseView>().defaults, Some(vec![0.0]));
        let world = app.world_mut();
        let mut note = world.query_filtered::<&Text, With<SwitchNote>>();
        assert_eq!(note.single(world).unwrap().0, "A lamp.");
    }

    #[derive(Resource)]
    struct Reach(f32);

    #[test]
    fn a_disabled_slider_shows_its_reason_without_controls_and_restores_its_saved_value() {
        let mut app = App::new();
        app.insert_resource(Reach(0.5)).insert_resource(Lamp(true)).init_resource::<Paused>().insert_resource(Page::Settings { selected: 0 });
        let slider = crate::settings::Slider { get: |w| w.resource::<Reach>().0, set: |w, v| w.resource_mut::<Reach>().0 = v, min: 0.1, step: 0.05, shown: None };
        add_setting(&mut app, Setting::slider("reach", "Test", "Reach", "How far.", slider));
        crate::settings::set_disabled_status(&mut app, "reach", |w| w.resource::<Lamp>().0.then_some("Off (FSR active)"));
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().init_resource::<crate::pad::PadGlyphs>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<(&SwitchState, &Text)>().single(world).unwrap().1.0, "Off (FSR active)");
        let buttons: Vec<PauseButton> = world.query::<&PauseButton>().iter(world).copied().collect();
        assert!(!buttons.iter().any(|b| matches!(b, PauseButton::Meter(0) | PauseButton::SliderStep(0, _))), "disabled sliders have no clickable adjustment controls");
        assert!(view_disabled(world.resource::<PauseView>(), 0));
        assert!(crate::settings::step_slider(world, 0, 1).is_none());
        assert_eq!(world.resource::<Reach>().0, 0.5);

        world.resource_mut::<Lamp>().0 = false;
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<(&SwitchState, &Text)>().single(world).unwrap().1.0, "50%");
        assert!(!view_disabled(world.resource::<PauseView>(), 0));
        assert!(world.query::<&PauseButton>().iter(world).any(|b| *b == PauseButton::Meter(0)));
        assert_eq!(crate::settings::step_slider(world, 0, 1), Some(0.55));
    }

    #[test]
    fn a_slider_row_shows_its_value_as_a_meter_and_a_percentage() {
        let mut app = App::new();
        app.insert_resource(Reach(0.5)).init_resource::<Paused>().insert_resource(Page::Settings { selected: 0 });
        let slider = crate::settings::Slider { get: |w| w.resource::<Reach>().0, set: |w, v| w.resource_mut::<Reach>().0 = v, min: 0.1, step: 0.05, shown: None };
        add_setting(&mut app, Setting::slider("reach", "Test", "Reach", "How far.", slider));
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().init_resource::<crate::pad::PadGlyphs>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        let shown = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            let value = q.single(world).unwrap().1 .0.clone();
            let mut b = world.query::<&PauseButton>();
            let buttons: Vec<PauseButton> = b.iter(world).copied().collect();
            assert!(buttons.contains(&PauseButton::Meter(0)) && buttons.contains(&PauseButton::SliderStep(0, -1)) && buttons.contains(&PauseButton::SliderStep(0, 1)), "{buttons:?}");
            value
        };
        assert_eq!(shown(&mut app), "50%");
        crate::settings::step_slider(app.world_mut(), 0, -2);
        assert_eq!(shown(&mut app), "40%");
        assert_eq!(app.world().resource::<PauseView>().defaults, Some(vec![0.5]));
        assert_eq!(meter_ticks(1.0), 30);
        assert_eq!(meter_ticks(0.5), 15);
        assert_eq!(meter_ticks(0.0), 0);
    }

    #[test]
    fn the_controls_tab_shows_each_key_and_waits_for_a_new_one() {
        use crate::bindings::{Bindings, Control, Input};
        // Row 0 is the input device.
        let jump = crate::bindings::GAME_CONTROLS.iter().position(|(c, _, _)| *c == Control::Jump).unwrap() + 1;
        let mut app = App::new();
        app.init_resource::<Paused>().init_resource::<Settings>().init_resource::<Bindings>().insert_resource(Page::Controls { selected: jump, listening: false });
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().init_resource::<crate::pad::PadGlyphs>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        let value = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.iter(world).find(|(s, _)| s.0 == jump).map(|(_, t)| t.0.clone())
        };
        assert_eq!(value(&mut app).as_deref(), Some("Space"));
        crate::bindings::rebind(app.world_mut(), Control::Jump, Input::Key(KeyCode::KeyJ));
        assert_eq!(value(&mut app).as_deref(), Some("J"));
        *app.world_mut().resource_mut::<Page>() = Page::Controls { selected: jump, listening: true };
        assert_eq!(value(&mut app).as_deref(), Some(WAITING));
        assert!(Page::Controls { selected: 0, listening: false }.opened_by("Settings"));
        // With a controller the rows show its buttons: Jump is Y [hypothesis, `crate::pad`].
        crate::bindings::set_device(app.world_mut(), crate::bindings::InputDevice::Controller);
        *app.world_mut().resource_mut::<Page>() = Page::Controls { selected: jump, listening: false };
        assert_eq!(value(&mut app).as_deref(), Some("Y"));
        let device = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.iter(world).find(|(s, _)| s.0 == 0).map(|(_, t)| t.0.clone())
        };
        *app.world_mut().resource_mut::<Page>() = Page::Controls { selected: 0, listening: false };
        assert_eq!(device(&mut app).as_deref(), Some("Controller"));
        // With a controller the last row is Rumble, on by default.
        let rumble = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.iter(world).find(|(s, _)| s.0 == rumble_row()).map(|(_, t)| t.0.clone())
        };
        *app.world_mut().resource_mut::<Page>() = Page::Controls { selected: rumble_row(), listening: false };
        assert_eq!(rumble(&mut app).as_deref(), Some("ON"));
        crate::bindings::set_rumble(app.world_mut(), false);
        assert_eq!(rumble(&mut app).as_deref(), Some("OFF"));
        crate::bindings::set_device(app.world_mut(), crate::bindings::InputDevice::KeyboardMouse);
        assert_eq!(rumble(&mut app), None, "no Rumble row with keyboard and mouse");
        // Mouse Sensitivity, the game's at first, kept apart from the stick's.
        let sensitivity = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.iter(world).find(|(s, _)| s.0 == sensitivity_row()).map(|(_, t)| t.0.clone())
        };
        *app.world_mut().resource_mut::<Page>() = Page::Controls { selected: sensitivity_row(), listening: false };
        assert_eq!(sensitivity(&mut app).as_deref(), Some("100%"));
        crate::bindings::set_sensitivity(app.world_mut(), 1.5);
        assert_eq!(sensitivity(&mut app).as_deref(), Some("150%"));
        crate::bindings::set_device(app.world_mut(), crate::bindings::InputDevice::Controller);
        assert_eq!(sensitivity(&mut app).as_deref(), Some("100%"), "the stick's own");
        crate::bindings::set_sensitivity(app.world_mut(), 0.5);
        assert_eq!(sensitivity(&mut app).as_deref(), Some("50%"));
        crate::bindings::set_device(app.world_mut(), crate::bindings::InputDevice::KeyboardMouse);
        assert_eq!(sensitivity(&mut app).as_deref(), Some("150%"), "the mouse's kept");
        let buttons: Vec<PauseButton> = app.world_mut().query::<&PauseButton>().iter(app.world()).copied().collect();
        assert!(buttons.contains(&PauseButton::Meter(sensitivity_row())) && buttons.contains(&PauseButton::SliderStep(sensitivity_row(), 1)), "a meter with arrows: {buttons:?}");
        let invert = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.iter(world).find(|(s, _)| s.0 == invert_row()).map(|(_, t)| t.0.clone())
        };
        *app.world_mut().resource_mut::<Page>() = Page::Controls { selected: invert_row(), listening: false };
        assert_eq!(invert(&mut app).as_deref(), Some("OFF"));
        crate::bindings::set_invert_y(app.world_mut(), true);
        assert_eq!(invert(&mut app).as_deref(), Some("ON"));
    }
}
