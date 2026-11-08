//! The Pip-Boy (docs/spec-notes/pipboy.md): Tab raises the player's left arm with the
//! Pip-Boy to the view, and its screen shows the Pip-Boy menus, under three buttons on
//! the device (STATS, ITEMS, DATA), each with five tabs along the screen's foot.
//!
//! - **The menus are drawn into a texture** ([`PipBoyScreen`]): a 2D camera renders the
//!   menu's interface nodes into an image, and the player's body (nvforever,
//!   `pipboy_arm.rs`) puts that image on the Pip-Boy model's screen. The menus are laid
//!   out on a canvas 1024 × 768 [documented: `menus\globals.xml` `_pipboy_width` and
//!   `_pipboy_height`; the three menu files lay out inside it]. The image is 960 high so
//!   that a length of the interface (`interface::u`, 960 to the height of the view it is
//!   drawn in) is one of its pixels, and the canvas is its top left 1024 × 768 [design].
//!   Where nothing shows the model's screen (no body, no Pip-Boy worn) the image is drawn
//!   flat in the middle of the window instead [design].
//! - **Opening and closing.** Tab (the control Menu Mode) opens the player's inventory
//!   model (`menu::open_menus`, `Mode::Inventory`), which pauses the game
//!   (`PlayState::Menu`); while that model is open the Pip-Boy is up. One first-person
//!   clip, `characters\_1stperson\locomotion\male\pipboy.kf` (0.73 s,
//!   [`CLIP_SECONDS`]), both brings it up and puts it away: up to its `Hit` key (0.33 s,
//!   [`HOLD_SECONDS`]) the arm comes up until the screen is straight before the eye,
//!   facing it; after the key the arm goes away down [observed: the key; the screen's
//!   place against the camera through the clip, docs/spec-notes/pipboy.md §1]. So the
//!   clip is held at `Hit` while the menus are open and plays on from there when they
//!   close [hypothesis: the original holds it there]. The view is first person while
//!   it is up and goes back to what it was [hypothesis]. A script that has turned the
//!   Pip-Boy off keeps it down (`menu::open_menus`).
//! - **Pages.** [`Page`]: STATS, ITEMS and DATA, chosen by the buttons on the device (the
//!   body reports a click on one as [`PipBoyInput::Page`]) or F1, F2, F3 [hypothesis: the
//!   PC keys are not captured]; each page remembers its tab. The tabs: the names along
//!   the screen's foot, clicked, or A/D and Left/Right [design].
//! - **The pointer.** The window's pointer moves over the device; where it lies on the
//!   screen, it is passed to the menu's nodes as a pointer of their own
//!   ([`PIPBOY_POINTER`], Bevy's picking) so the menus take hovers and clicks as any
//!   other interface does. The body finds the spot on the model's screen
//!   ([`PipBoyScreen::pointer`]) on the shape as the arm's bones pose it (nvforever
//!   `pipboy_arm.rs`; Bevy's mesh ray casts see neither the skinning nor meshes kept on
//!   the GPU only, which is why clicks on the model did nothing until 2026-10-11); for
//!   the flat image this module finds it.
//! - **The screen menu** (docs/design/screen-menu.md): the same menus without the arm,
//!   the image drawn full screen over the world blacked out, as the barter menu is, with
//!   the device's three buttons as a header in place of the page's title. It is the
//!   frame in use ([`Frame`]) whenever the player wears no Pip-Boy, so the inventory never
//!   needs one, and when the Gameplay setting Use Pip-Boy is off, which also takes the
//!   Pip-Boy and its glove off (on puts them back on) [design: owner, 2026-10-31].
//!   Nothing moves the arm, the view or the blur then; the pages do not know which frame
//!   they are in.
//! - **Drawing.** [`frame`] draws what every page shares (the background, the tab line)
//!   and gives the pages their tools; each page module ([`stats`], [`items`], [`data`])
//!   fills the [`PageBody`] node with its own system, so a page can read what it needs.

pub mod data;
pub mod device_screen;
pub mod frame;
pub mod hotkey_panel;
pub mod items;
pub mod item_mod;
pub mod repair;
pub mod sounds;
pub mod stats;
mod test_aid;

use crate::control::{PlayerControl, Pov};
use crate::interface::u;
use fnv_game::game::menus;
use crate::menu::model::Mode;
use crate::menu::ActiveMenu;
use crate::PlaySet;
use bevy::asset::{uuid::Uuid, RenderAssetUsages};
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::input::ButtonState;
use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
use bevy::picking::PickingSystems;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::{PrimaryWindow, WindowEvent};

/// The Pip-Boy clip's length, seconds [observed: `pipboy.kf` `Pipboy` sequence, stop
/// time 0.7333].
pub const CLIP_SECONDS: f32 = 0.733_333_3;

/// Where the clip is held while the Pip-Boy is up: its `Hit` key [observed: 0.3333 s].
pub const HOLD_SECONDS: f32 = 0.333_333_3;

/// The raise clip [observed: archive file list].
pub const RAISE_CLIP: &str = "meshes\\characters\\_1stperson\\locomotion\\male\\pipboy.kf";

/// The canvas the Pip-Boy menus are laid out on, interface units [documented:
/// globals.xml].
pub const CANVAS: Vec2 = Vec2::new(1024.0, 768.0);

/// The image the menus are drawn into, pixels (one per interface unit; module docs).
pub const TARGET: UVec2 = UVec2::new(1280, 960);

/// The screen menu: how high the image of the canvas is drawn, interface units (960 to
/// the view's height) [design: 92% of the view; docs/design/screen-menu.md §3.3].
pub const SCREEN_IMAGE_H: f32 = 884.0;
/// The part of the canvas the screen menu shows: the menus' rectangle (`frame::MAIN`, 50
/// in from the top left) and as much again right and below, so the menus sit centred and
/// the canvas's empty right and bottom are left out [design].
pub const SCREEN_CROP: Vec2 = Vec2::new(955.0, 750.0);
/// The image's magnification at most in the screen menu (a very high window).
const MAX_SCALE: f32 = 3.0;
/// The screen menu's shade: one even black over the whole window, square to its edges
/// [design: the owner, 2026-10-31: "black out the entire background", "black should extend
/// the entire screen", "square up against the edge of the screen"; the barter menu's 0.93
/// (`interface::TOP_MENU_SHADE`) let the world show through the dimmed hints and the map].
const SHADE: f32 = 0.98;

/// The pointer on the Pip-Boy's screen, for Bevy's picking.
pub const PIPBOY_POINTER: PointerId = PointerId::Custom(Uuid::from_u128(0x6e76_665f_7069_7062_6f79_5f70_6f69_6e74));

/// Where the pointer on the screen is found for the frame ([`PipBoyScreen::pointer`]): after
/// the body is posed and before the tile input (`crate::tiles::stats_menu::feed`), which takes
/// this frame's button with it, so a move and a press in one frame act at the new place. The
/// device's system is nvforever's `pipboy_arm`; the flat image's is here.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PointerSet;

/// Hidden draws are arranged before the clip advances [design: nvforever/pipboy_arm.rs].
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WarmSet;

/// Which system finds the pointer this frame ([`pointer_source`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerSource {
    Device,
    Flat,
}

/// One producer of [`PipBoyScreen::pointer`] for the frame, from the frame in use and whether
/// the model shows the image: the device only while both say so. The two can disagree for a
/// moment (`follow_frame` changes the frame before the body gives the model's screen up), and
/// the flat image then has it, so the two systems can never overwrite each other.
pub fn pointer_source(frame: Frame, on_model: bool) -> PointerSource {
    if frame == Frame::Device && on_model {
        PointerSource::Device
    } else {
        PointerSource::Flat
    }
}

/// The three pages, in the order of the device's buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Page {
    Stats,
    #[default]
    Items,
    Data,
}

impl Page {
    pub const ALL: [Page; 3] = [Page::Stats, Page::Items, Page::Data];

    /// The page's title at the screen's top left [observed: the owner's screenshots].
    pub fn title(self) -> &'static str {
        match self {
            Page::Stats => "STATS",
            Page::Items => "ITEMS",
            Page::Data => "DATA",
        }
    }

    /// The page's tabs, left to right [observed: the owner's screenshots].
    pub fn tabs(self) -> [&'static str; 5] {
        match self {
            Page::Stats => ["Status", "S.P.E.C.I.A.L.", "Skills", "Perks", "General"],
            Page::Items => ["Weapons", "Apparel", "Aid", "Misc", "Ammo"],
            Page::Data => ["Local Map", "World Map", "Quests", "Misc", "Radio"],
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Where the menus are shown: on the arm's device, or full screen without it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Frame {
    #[default]
    Device,
    Screen,
}

/// The Gameplay setting Use Pip-Boy: on, the Pip-Boy and its glove are worn and the menus
/// come up on the arm; off, both are taken off and the menus are on the screen.
#[derive(Resource, Clone, Copy, Debug)]
pub struct UsePipBoy {
    pub on: bool,
    /// Set by the setting; the player's Pip-Boy and glove are put on or taken off to match
    /// once there is a player ([`apply_use`]).
    pub pending: bool,
}

impl Default for UsePipBoy {
    fn default() -> UsePipBoy {
        UsePipBoy { on: true, pending: false }
    }
}

impl UsePipBoy {
    /// The setting changed (the settings page, the INI at start, the test aid).
    pub fn set(&mut self, on: bool) {
        self.on = on;
        self.pending = true;
    }
}

/// The setting's key in `NVForever.ini`.
pub const SETTING_KEY: &str = "UsePipBoy";

/// The player's Pip-Boy Glove, the hand worn with the Pip-Boy [observed: FalloutNV.esm
/// ARMO 00025B83 `PipBoyGlove`; the Pip-Boy itself is 00015038 `PipBoy`].
const PIPBOY_GLOVE: u32 = 0x0002_5B83;
const PIPBOY: u32 = 0x0001_5038;

/// The frame the menus use: the screen when the setting says so or no Pip-Boy is worn.
pub fn frame_for(on_screen: bool, wears_pipboy: bool) -> Frame {
    if on_screen || !wears_pipboy {
        Frame::Screen
    } else {
        Frame::Device
    }
}

/// The Pip-Boy's body slot among an armour's biped flags [documented: ARMO BMDT; the same
/// bit as `fnv_actor::resolve::slot::PIPBOY`, which the body draws on the arm].
const PIPBOY_SLOT: u32 = 1 << 6;

/// Whether the player wears a Pip-Boy: an equipped item in the Pip-Boy's body slot.
pub fn wears_pipboy(sim: &crate::sim::SimState) -> bool {
    let Some(inv) = sim.world.peek(fnv_sim::FormId::PLAYER_REF).and_then(|r| r.inventory.as_ref()) else { return false };
    inv.equipped().into_iter().any(|id| sim.forms.item(id).and_then(|i| i.slots).is_some_and(|s| s & PIPBOY_SLOT != 0))
}

/// Where the Pip-Boy is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Phase {
    /// Down: not shown.
    #[default]
    Down,
    /// Coming up to the view.
    Raising,
    /// Up, held before the view.
    Up,
    /// Going back down.
    Lowering,
}

/// The Pip-Boy's state.
#[derive(Resource, Debug, Default)]
pub struct PipBoy {
    pub phase: Phase,
    /// Seconds into the clip: from 0 coming up to [`HOLD_SECONDS`], held there while up,
    /// on to [`CLIP_SECONDS`] going down.
    pub clip_time: f32,
    pub page: Page,
    /// Where the menus are shown while up.
    pub frame: Frame,
    /// Each page's tab (0 to 4).
    pub tabs: [usize; 3],
    /// The view before the Pip-Boy came up, given back when it is down.
    saved_pov: Option<Pov>,
    /// The tab of the tab line under the pointer ([`frame::hover_boxes`]).
    pub hover_tab: Option<usize>,
    /// Hidden rendering interval; a real opening still reports its page [design: pipboy_arm.rs].
    pub warming: Option<Warming>,
    pub warmed: bool,
    /// [design: nvforever/player_body.rs] True until the borrowed pose is restored.
    pub pose_hidden: bool,
}

/// One hidden draw interval [design: nvforever/pipboy_arm.rs].
#[derive(Debug)]
pub struct Warming {
    pub since: std::time::Instant,
    pub frames: u32,
    pub asked_at: u32,
    pub covered: bool,
    pub device: bool,
    pub saved_frame: Frame,
    pub saved_page: Option<(Page, usize)>,
    pub drawn: bool,
}

impl PipBoy {
    /// Whether to draw the Pip-Boy, including hidden warming [design: nvforever/pipboy_arm.rs].
    pub fn shown(&self) -> bool {
        self.warming.is_some() || self.phase != Phase::Down
    }

    /// Whether the menus take input (coming up or up, after hidden warming) [design: this file].
    pub fn open(&self) -> bool {
        self.warming.is_none() && matches!(self.phase, Phase::Raising | Phase::Up)
    }

    /// [design: this file] A real opening reports its page and accepts Close during a hold.
    pub fn opening(&self) -> bool {
        matches!(self.phase, Phase::Raising | Phase::Up) && !self.warming.as_ref().is_some_and(|w| w.covered)
    }

    /// Whether to draw the device on the arm, including hidden warming [design: this file].
    pub fn on_arm(&self) -> bool {
        self.warming.as_ref().map_or(self.shown() && self.frame == Frame::Device, |w| w.device)
    }

    /// The hidden pose's time, independent of the player's clip [design: player_body.rs].
    pub fn warm_clip(&self) -> Option<f32> {
        self.warming.as_ref().filter(|w| w.device).map(|w| if w.covered { HOLD_SECONDS } else { 0.0 })
    }

    /// A new rig/material must be drawn again, within the original cap [design: pipboy_arm.rs].
    pub fn invalidate_warm(&mut self) {
        self.warmed = false;
        if let Some(w) = &mut self.warming {
            w.drawn = false;
        }
    }

    /// The tab of the page shown.
    pub fn tab(&self) -> usize {
        self.tabs[self.page.index()]
    }

    /// Move the clip on by `dt` seconds; true when the Pip-Boy reached the top or the
    /// bottom in this step.
    pub fn advance(&mut self, dt: f32) -> bool {
        if self.warming.is_some() {
            return false;
        }
        match self.phase {
            Phase::Raising => {
                self.clip_time = (self.clip_time + dt).min(HOLD_SECONDS);
                if self.clip_time >= HOLD_SECONDS {
                    self.phase = Phase::Up;
                    return true;
                }
            }
            Phase::Lowering => {
                self.clip_time = (self.clip_time + dt).min(CLIP_SECONDS);
                if self.clip_time >= CLIP_SECONDS {
                    self.phase = Phase::Down;
                    self.clip_time = 0.0;
                    return true;
                }
            }
            Phase::Down | Phase::Up => {}
        }
        false
    }

    /// Come up. Put back up on the way down, it comes up from as far down as it got
    /// [design: the way down mirrored onto the way up].
    pub fn raise(&mut self) {
        // The screen menu has no clip: up at once.
        if self.frame == Frame::Screen {
            (self.phase, self.clip_time) = (Phase::Up, HOLD_SECONDS);
            return;
        }
        match self.phase {
            Phase::Down => (self.phase, self.clip_time) = (Phase::Raising, 0.0),
            Phase::Lowering => (self.phase, self.clip_time) = (Phase::Raising, (2.0 * HOLD_SECONDS - self.clip_time).max(0.0)),
            Phase::Raising | Phase::Up => {}
        }
    }

    /// Go down: on from `Hit` (from where it got to, if it was still coming up
    /// [design]).
    pub fn lower(&mut self) {
        if self.frame == Frame::Screen && self.shown() {
            (self.phase, self.clip_time) = (Phase::Down, 0.0);
            return;
        }
        if matches!(self.phase, Phase::Raising | Phase::Up) {
            self.phase = Phase::Lowering;
            self.clip_time = self.clip_time.max(HOLD_SECONDS);
        }
    }

    /// Show a page, or step a page's tab.
    pub fn apply(&mut self, input: PipBoyInput) {
        match input {
            PipBoyInput::Page(p) => self.page = p,
            PipBoyInput::Tab(t) => self.tabs[self.page.index()] = t.min(4),
            PipBoyInput::StepTab(by) => {
                let t = &mut self.tabs[self.page.index()];
                *t = (*t as i32 + by).rem_euclid(5) as usize;
            }
        }
    }
}

/// What the player asks of the Pip-Boy's pages.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum PipBoyInput {
    /// Show this page (a button on the device, F1–F3).
    Page(Page),
    /// Show this tab of the page shown.
    Tab(usize),
    /// The next (1) or previous (-1) tab, round the ends.
    StepTab(i32),
}

/// The image the menus are drawn into, and where it is shown.
#[derive(Resource)]
pub struct PipBoyScreen {
    pub image: Handle<Image>,
    pub camera: Entity,
    /// The model's screen shows the image (the body sets it each frame).
    pub on_model: bool,
    /// Where the window's pointer lies on the canvas (interface units from its top left),
    /// if on it: set by the body from the model's screen, or here from the flat image.
    pub pointer: Option<Vec2>,
    /// The image's pixels to a canvas unit: 1 on the device, more in the screen menu so
    /// the larger picture stays sharp ([`image_scale`]).
    pub scale: f32,
}

impl PipBoyScreen {
    /// The render target the screen camera draws into, and the pointer is located on.
    pub fn target(&self) -> ImageRenderTarget {
        ImageRenderTarget { handle: self.image.clone(), scale_factor: self.scale }
    }
}

/// Test aid (`NVF_PLAY_TEST_PIPBOY`'s `point=` and `click=`): what the pointer is put on
/// in place of the window's, and a click to make there.
#[derive(Resource, Debug, Default)]
pub struct PipBoyAid {
    pub target: Option<AidTarget>,
    /// A click on the target in progress: 3 asked, 2 the pointer is on it, 1 pressed.
    pub click: u8,
    /// The click is with the right button (`rclick=`).
    pub right: bool,
    /// A drag of the map shown (canvas units), as the left button held and moved would
    /// (`drag=`), taken by the DATA page.
    pub drag: Vec2,
    /// A note of the Notes list to select and use (`note=`), taken by the DATA page.
    pub note: Option<u32>,
}

/// Where the test aid points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AidTarget {
    /// The middle of a page's button on the device.
    Button(Page),
    /// A point of the canvas, on the screen (through the model where it is shown).
    Canvas(Vec2),
}

/// The node a page fills, under the menus' root (canvas units from its top left).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageBody {
    pub page: Page,
    pub tab: usize,
}

/// The root of the menus drawn into the image.
#[derive(Component)]
pub struct PipBoyRoot;

/// The flat image in the window (no model screen).
#[derive(Component)]
pub struct FlatScreen;

/// The screen menu's black shade over the window, which holds the header and the image.
#[derive(Component)]
pub struct ScreenShade;

pub struct PipBoyPlugin;

impl Plugin for PipBoyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PipBoy>()
            .init_resource::<UsePipBoy>()
            .init_resource::<PipBoyAid>()
            .init_resource::<frame::Kit>()
            .init_resource::<stats::Prepared>()
            .init_resource::<device_screen::ScreenFx>()
            .init_resource::<repair::PipRepair>()
            .add_message::<repair::RepairInput>()
            .init_resource::<item_mod::PipItemMod>()
            .init_resource::<hotkey_panel::HotkeyHold>()
            .add_message::<item_mod::ModInput>()
            .add_message::<PipBoyInput>()
            .add_message::<stats::UseHint>()
            .add_message::<sounds::TabHovered>()
            .add_message::<data::travel::FastTravelAsk>()
            .add_message::<data::travel::FastTravelGo>()
            .init_resource::<data::travel::Pending>()
            .init_resource::<data::WaveScope>()
            .add_systems(Startup, setup_screen)
            .add_systems(
                Update,
                (
                    (apply_use, follow_menu.after(crate::menu::MenuSystems), follow_frame).chain(),
                    reset_manager,
                    close_keys.run_if(|pip: Res<PipBoy>| pip.opening()).run_if(takes_keys),
                    (keys, apply_input).chain().run_if(pipboy_open).run_if(takes_keys),
                    repair::keys.run_if(takes_keys),
                    item_mod::keys.run_if(takes_keys),
                    hotkey_panel::track,
                    items::keys.run_if(takes_keys),
                    report_menu,
                    advance,
                    camera_on,
                    (image_scale, flat_screen),
                    stats::prepare,
                    frame::draw,
                    (device_screen::draw, device_screen::sweep).chain(),
                    (stats::draw, items::draw, data::draw),
                    data::wave_moves,
                    stats::use_hints,
                    (data::travel::answer, data::travel::go, data::travel::depart),
                    (frame::hover_boxes, sounds::play),
                )
                    .chain()
                    .in_set(PlaySet::Ui),
            )
            .configure_sets(Update, WarmSet.after(follow_frame).before(reset_manager))
            .configure_sets(Update, PointerSet.after(crate::player::BodySet).before(PlaySet::Activation))
            .add_systems(Update, flat_pointer.in_set(PointerSet))
            .add_systems(First, drive_pointer.in_set(PickingSystems::Input));
        test_aid::setup(app);
        crate::settings::add_setting(
            app,
            crate::settings::Setting {
                key: SETTING_KEY,
                group: crate::pause::GROUP_MENUS,
                label: "Use Pip-Boy",
                note: "On: the Pip-Boy and its glove are worn and the menus come up on the arm. Off: both are taken off and the menus show full screen. Without a Pip-Boy worn the menus are always full screen.",
                get: |w| w.resource::<UsePipBoy>().on,
                set: |w, on| w.resource_mut::<UsePipBoy>().set(on),
                slider: None,
            },
        );
    }
}

/// Whether the Pip-Boy's menus take input.
pub fn pipboy_open(pip: Res<PipBoy>) -> bool {
    pip.open()
}

/// Whether the Pip-Boy's keys are its own: not while the tutorial menu is open over it
/// (`crate::tutorial_menu`, whose A, S and E are Next, Previous and Close), and with the
/// page drawn from its tiles not in the frame a menu over it closes either: that key was the
/// menu's (`crate::tiles::stats_menu::StatsTiles::modal`).
fn takes_keys(script: Option<Res<crate::script::ScriptState>>, tiles: Option<Res<crate::tiles::stats_menu::StatsTiles>>) -> bool {
    !script.as_ref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.menu == Some(menus::TUTORIAL)) && !tiles.is_some_and(|t| t.modal)
}

/// The menu code the Pip-Boy shows (`MenuMode`): its page's, or the repair list's or the
/// mod menu's while one is open [documented: GECK wiki GetSelectedItemRef, "Inventory Menu
/// (1002)", "Pipboy STATS Menu (1003)", "Pipboy DATA Menu (1023)", "Pipboy Repair Menu
/// (1035)"; GetMenuTargetRef, "1061 The weapon being modded"].
pub fn menu_code(page: Page, repairing: bool, modding: bool) -> u16 {
    match page {
        _ if repairing => menus::REPAIR,
        _ if modding => menus::ITEM_MOD,
        Page::Stats => menus::STATS,
        Page::Items => menus::INVENTORY,
        Page::Data => menus::MAP,
    }
}

/// While the Pip-Boy is up, the scripts' open menu is its page's code ([`menu_code`];
/// it opens as the inventory, 1002, `crate::script::engine_menu_opened`). Logged:
/// `pipboy: menu code <code>`.
fn report_menu(pip: Res<PipBoy>, repair: Res<repair::PipRepair>, modding: Res<item_mod::PipItemMod>, script: Option<ResMut<crate::script::ScriptState>>) {
    if !pip.opening() {
        return;
    }
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    let code = menu_code(pip.page, repair.is_open(), modding.is_open());
    if rt.game.menu != Some(code) && rt.game.set_pipboy_menu(code) {
        info!("pipboy: menu code {code}");
    }
}

/// ResetPipboyManager (`fnv_game::functions::ui::reset_pipboy_manager`): the Pip-Boy's
/// pages start again on their defaults (ITEMS, every page on its first tab) and its
/// light (NVForever's flashlight, `crate::flashlight`) goes off [documented: GECK wiki
/// ResetPipboyManager, the look back to its default and the light off; hypothesis: the
/// pages]. Logged: `pipboy: manager reset (…)`.
fn reset_manager(
    script: Option<Res<crate::script::ScriptState>>,
    mut seen: Local<u64>,
    mut pip: ResMut<PipBoy>,
    light: Option<ResMut<crate::flashlight::Flashlight>>,
) {
    let Some(n) = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| rt.game.ui.pipboy_resets) else { return };
    if n <= *seen {
        *seen = n;
        return;
    }
    *seen = n;
    pip.page = Page::default();
    pip.tabs = [0; 3];
    let lit = light.is_some_and(|mut l| {
        let on = l.on;
        if on {
            l.toggle();
        }
        on
    });
    info!("pipboy: manager reset (ResetPipboyManager): page {} tab {}{}", pip.page.title(), pip.page.tabs()[pip.tab()], if lit { ", light off" } else { "" });
}

fn setup_screen(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let size = Extent3d { width: TARGET.x, height: TARGET.y, ..default() };
    let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    let camera = commands
        .spawn((
            Name::new("Pip-Boy screen camera"),
            Camera2d,
            Camera { order: -2, is_active: false, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
            RenderTarget::Image(ImageRenderTarget::from(image.clone())),
        ))
        .id();
    commands.insert_resource(PipBoyScreen { image, camera, on_model: false, pointer: None, scale: 1.0 });
}

/// The Pip-Boy is up while the player's inventory is open, and in first person (on the
/// arm; the screen menu leaves the view alone).
fn follow_menu(active: Res<ActiveMenu>, mut pip: ResMut<PipBoy>, mut control: ResMut<PlayerControl>, use_pipboy: Res<UsePipBoy>, sim: Option<Res<crate::sim::SimState>>) {
    let inventory = active.0.as_ref().is_some_and(|m| m.mode == Mode::Inventory);
    if inventory && !matches!(pip.phase, Phase::Raising | Phase::Up) {
        if pip.phase == Phase::Down {
            pip.frame = frame_for(!use_pipboy.on, sim.as_deref().is_some_and(wears_pipboy));
            info!("pipboy: frame {:?}", pip.frame);
            if pip.frame == Frame::Device {
                pip.saved_pov = Some(control.pov);
            }
            info!("pipboy: up, page {} tab {}", pip.page.title(), pip.page.tabs()[pip.tab()]);
        }
        pip.raise();
        if pip.frame == Frame::Device {
            control.pov = Pov::First;
            control.orbit = default();
        }
    } else if !inventory && matches!(pip.phase, Phase::Raising | Phase::Up) {
        info!("pipboy: down");
        pip.lower();
    }
}

/// While the menus are up, follow the setting and the Pip-Boy being put on or taken off
/// (ITEMS > Apparel): to the screen menu, the view goes back to what it was and the arm
/// is gone; to a warm device, the arm is up at once. A cold device raises from the
/// bottom so the hidden hold can precede it [design: nvforever/pipboy_arm.rs, #1803].
fn follow_frame(mut pip: ResMut<PipBoy>, mut control: ResMut<PlayerControl>, use_pipboy: Res<UsePipBoy>, sim: Option<Res<crate::sim::SimState>>) {
    if pip.phase != Phase::Up {
        return;
    }
    let want = frame_for(!use_pipboy.on, sim.as_deref().is_some_and(wears_pipboy));
    if want == pip.frame {
        return;
    }
    pip.frame = want;
    info!("pipboy: frame {want:?}");
    match want {
        Frame::Screen => {
            if let Some(p) = pip.saved_pov.take() {
                control.pov = p;
            }
        }
        Frame::Device => {
            pip.saved_pov = Some(control.pov);
            if pip.warmed {
                pip.clip_time = HOLD_SECONDS;
            } else {
                (pip.phase, pip.clip_time) = (Phase::Raising, 0.0);
            }
            control.pov = Pov::First;
            control.orbit = default();
        }
    }
}

/// What the setting asks of the player's equipment, as (item, put on) pairs. Off, at any
/// time: take off whatever is worn in the Pip-Boy's slot and the Pip-Boy Glove, so a loaded
/// save or a script that equips them (Doc Mitchell's) does not bring them back while the
/// setting is off. On, only when the setting has just changed (`changed`): put on a carried
/// Pip-Boy (the Pip-Boy 3000 first) if none is worn, and the glove with it if carried; the
/// glove never alone [design: owner, 2026-10-31; the coordinator's review of #1229].
pub fn use_changes(on: bool, changed: bool, worn: &[fnv_sim::FormId], carried: &[fnv_sim::FormId], in_slot: impl Fn(fnv_sim::FormId) -> bool) -> Vec<(fnv_sim::FormId, bool)> {
    let glove = fnv_sim::FormId(PIPBOY_GLOVE);
    if !on {
        return worn.iter().filter(|&&id| in_slot(id) || id == glove).map(|&id| (id, false)).collect();
    }
    if !changed {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut device = worn.iter().any(|&id| in_slot(id));
    if !device {
        let mut pipboys: Vec<fnv_sim::FormId> = carried.iter().copied().filter(|&id| in_slot(id)).collect();
        pipboys.sort_by_key(|id| id.0 != PIPBOY);
        if let Some(&id) = pipboys.first() {
            out.push((id, true));
            device = true;
        }
    }
    if device && !worn.contains(&glove) && carried.contains(&glove) {
        out.push((glove, true));
    }
    out
}

/// Make the player's equipment follow the setting ([`use_changes`]): while it is off every
/// frame, while on only once it changed and there is a player. A change the game refuses
/// (a script's equip lock) is logged once.
fn apply_use(mut use_pipboy: ResMut<UsePipBoy>, mut pip: ResMut<PipBoy>, sim: Option<ResMut<crate::sim::SimState>>, mut refused: Local<Vec<fnv_sim::FormId>>) {
    if use_pipboy.on && !use_pipboy.pending {
        return;
    }
    let Some(mut sim) = sim else { return };
    let player = fnv_sim::FormId::PLAYER_REF;
    let Some(inv) = sim.world.peek(player).and_then(|r| r.inventory.as_ref()) else { return };
    let changed = std::mem::take(&mut use_pipboy.pending);
    if changed && use_pipboy.on {
        pip.invalidate_warm();
    }
    let worn = inv.equipped();
    let carried: Vec<fnv_sim::FormId> = if use_pipboy.on { inv.iter().filter(|&(_, n)| n > 0).map(|(id, _)| id).collect() } else { Vec::new() };
    let changes = use_changes(use_pipboy.on, changed, &worn, &carried, |id| sim.forms.item(id).and_then(|i| i.slots).is_some_and(|s| s & PIPBOY_SLOT != 0));
    if changed {
        refused.clear();
    }
    for (id, on) in changes {
        if refused.contains(&id) {
            continue;
        }
        let done = if on { sim.world.equip(player, id).map(|_| true) } else { sim.world.unequip(player, id) };
        match done {
            Ok(_) => info!("pipboy: Use Pip-Boy {}: {} {id}", if on { "on" } else { "off" }, if on { "put on" } else { "took off" }),
            Err(e) => {
                warn!("pipboy: Use Pip-Boy: {id} not changed: {e:?}");
                refused.push(id);
            }
        }
    }
}

/// The clip runs on the menus' clock ([`crate::pause::AidTime`]): the inventory is a menu,
/// and under `--menu-pause original` (the setting "menu pause") its opening stops the
/// virtual clock, which held the Pip-Boy at the bottom until it was closed, when only
/// the way down played (#1330). Logged: `pipboy: raised`.
fn advance(time: crate::pause::AidTime, mut pip: ResMut<PipBoy>, mut control: ResMut<PlayerControl>) {
    if pip.phase == Phase::Down {
        if let Some(p) = pip.saved_pov.take() {
            control.pov = p;
        }
    }
    if !matches!(pip.phase, Phase::Raising | Phase::Lowering) {
        return;
    }
    if pip.advance(time.delta_secs_f64() as f32) {
        match pip.phase {
            Phase::Down => {
                if let Some(p) = pip.saved_pov.take() {
                    control.pov = p;
                }
            }
            _ => info!("pipboy: raised"),
        }
    }
}

/// The screen camera draws only while the Pip-Boy shows; in the screen menu onto a clear
/// image, so the menus lie on the shade behind them and not on a black box; on the device
/// onto the screen's undercoat (`device_screen.rs`).
fn camera_on(pip: Res<PipBoy>, screen: Option<Res<PipBoyScreen>>, fx: Res<device_screen::ScreenFx>, mut cams: Query<&mut Camera>) {
    let Some(screen) = screen else { return };
    if let Ok(mut c) = cams.get_mut(screen.camera) {
        if c.is_active != pip.shown() {
            c.is_active = pip.shown();
            if !c.is_active {
                info!("pipboy: screen camera off");
            }
        }
        let clear = if pip.frame == Frame::Screen { Color::NONE } else { device_screen::undercoat(fx.kept) };
        if !matches!(c.clear_color, ClearColorConfig::Custom(now) if now == clear) {
            c.clear_color = ClearColorConfig::Custom(clear);
        }
    }
}

/// Keys of the Pip-Boy: Tab, Esc and E close it; F1–F3 the pages; A/D and Left/Right
/// the tabs; none while the menus' "How many?" prompt is open. The ITEMS page's own keys
/// are in [`items`].
fn close_keys(
    keys: Res<ButtonInput<KeyCode>>,
    prompt: Res<crate::menu::PromptKeys>,
    active: Res<crate::menu::ActiveMenu>,
    repair: Res<repair::PipRepair>,
    modding: Res<item_mod::PipItemMod>,
    mut menu: MessageWriter<crate::menu::MenuInput>,
) {
    use KeyCode::*;
    // "How many?" has the keys (`crate::menu`).
    if prompt.held(&active) {
        return;
    }
    // Esc goes back from the repair list (`repair`) and the mod menu (`item_mod`), Tab
    // still puts the Pip-Boy away.
    if keys.just_pressed(Tab) || (keys.just_pressed(Escape) && !repair.is_open() && !modding.is_open()) {
        menu.write(crate::menu::MenuInput(crate::menu::model::Command::Close));
    }
}

fn keys(keys: Res<ButtonInput<KeyCode>>, prompt: Res<crate::menu::PromptKeys>, active: Res<ActiveMenu>, mut out: MessageWriter<PipBoyInput>) {
    use KeyCode::*;
    if prompt.held(&active) || keys.any_just_pressed([Tab, Escape]) {
        return;
    }
    for (k, p) in [(F1, Page::Stats), (F2, Page::Items), (F3, Page::Data)] {
        if keys.just_pressed(k) {
            out.write(PipBoyInput::Page(p));
        }
    }
    if keys.any_just_pressed([KeyA, ArrowLeft]) {
        out.write(PipBoyInput::StepTab(-1));
    }
    if keys.any_just_pressed([KeyD, ArrowRight]) {
        out.write(PipBoyInput::StepTab(1));
    }
}

fn apply_input(mut input: MessageReader<PipBoyInput>, mut pip: ResMut<PipBoy>) {
    for &i in input.read() {
        let before = (pip.page, pip.tab());
        pip.apply(i);
        if (pip.page, pip.tab()) != before {
            info!("pipboy: {} {}", pip.page.title(), pip.page.tabs()[pip.tab()]);
        }
    }
}

/// The image's scale the flat image's crop was made for.
#[derive(Component, Clone, Copy)]
struct FlatCrop(f32);

/// The scale of the image for the frame in use: 1 on the device; in the screen menu as
/// many of the window's pixels as the canvas is drawn over, so the picture is not
/// magnified, at most [`MAX_SCALE`]. The image is made again at that size, and the
/// camera told, so the menus still lay out on the canvas's units.
fn image_scale(pip: Res<PipBoy>, screen: Option<ResMut<PipBoyScreen>>, windows: Query<&Window, With<PrimaryWindow>>, mut images: ResMut<Assets<Image>>, mut targets: Query<&mut RenderTarget>) {
    let Some(mut screen) = screen else { return };
    let want = match (pip.shown() && pip.warming.is_none() && pip.frame == Frame::Screen, windows.single()) {
        (true, Ok(w)) => (w.physical_height() as f32 * SCREEN_IMAGE_H / 960.0 / SCREEN_CROP.y).clamp(1.0, MAX_SCALE),
        _ => 1.0,
    };
    // Steps of an eighth, so a window dragged a pixel does not remake the image.
    let want = (want * 8.0).round() / 8.0;
    if want == screen.scale {
        return;
    }
    let size = Extent3d { width: (TARGET.x as f32 * want).round() as u32, height: (TARGET.y as f32 * want).round() as u32, ..default() };
    let Some(mut image) = images.get_mut(&screen.image) else { return };
    image.resize(size);
    screen.scale = want;
    if let Ok(mut t) = targets.get_mut(screen.camera) {
        *t = RenderTarget::Image(screen.target());
    }
    info!("pipboy: image scale {want}");
}

/// Where nothing shows the model's screen, and always in the screen menu, the image is
/// drawn flat in the middle of the window over the world blacked out, under the header's
/// strip (docs/design/screen-menu.md §3); [`flat_pointer`] finds the pointer on it.
fn flat_screen(
    mut commands: Commands,
    pip: Res<PipBoy>,
    screen: Option<Res<PipBoyScreen>>,
    flat: Query<&FlatCrop, With<FlatScreen>>,
    shades: Query<Entity, With<ScreenShade>>,
) {
    let Some(screen) = screen else { return };
    // Only once the arm is all the way up: the body takes the screen a frame or more after
    // the Pip-Boy starts up (later still from third person, the first-person body not yet
    // drawn), and the flat image flashed in the middle of the window until it did (the
    // owner's playtest list 5, item 2).
    let want = pip.phase == Phase::Up && (pip.frame == Frame::Screen || !screen.on_model);
    // A change of the image's scale needs the image node's crop made again.
    let stale = flat.iter().next().is_some_and(|crop| crop.0 != screen.scale);
    match (want && !stale, flat.iter().next()) {
        (true, None) => {
            for e in &shades {
                commands.entity(e).despawn();
            }
            // The world blacked out; in it, the canvas's image, centred.
            let shade = Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            };
            let width = SCREEN_IMAGE_H * SCREEN_CROP.x / SCREEN_CROP.y;
            let crop = Rect::new(0.0, 0.0, SCREEN_CROP.x * screen.scale, SCREEN_CROP.y * screen.scale);
            commands.spawn((ScreenShade, shade, BackgroundColor(Color::BLACK.with_alpha(SHADE)), GlobalZIndex(90), Pickable::IGNORE, Name::new("Pip-Boy screen menu"))).with_children(|s| {
                s.spawn((FlatScreen, FlatCrop(screen.scale), Node { width: u(width), height: u(SCREEN_IMAGE_H), flex_shrink: 0.0, ..default() }, ImageNode { rect: Some(crop), ..ImageNode::new(screen.image.clone()) }));
            });
        }
        (false, Some(_)) => {
            for e in &shades {
                commands.entity(e).despawn();
            }
        }
        (true, Some(_)) | (false, None) => {}
    }
}

/// Where a cursor in physical pixels lies on the canvas shown in a node of `size` around
/// `centre`, if on it.
pub fn crop_point(cursor: Vec2, centre: Vec2, size: Vec2) -> Option<Vec2> {
    let p = (cursor - (centre - size / 2.0)) / size * SCREEN_CROP;
    (p.x >= 0.0 && p.y >= 0.0 && p.x < SCREEN_CROP.x && p.y < SCREEN_CROP.y).then_some(p)
}

/// The pointer on the flat image: the test aid's point, else the window's cursor.
fn flat_point(aid: &PipBoyAid, windows: &Query<&Window, With<PrimaryWindow>>, computed: &ComputedNode, at: &UiGlobalTransform) -> Option<Vec2> {
    if let Some(AidTarget::Canvas(p)) = aid.target {
        return Some(p);
    }
    let cursor = windows.single().ok().and_then(|w| w.physical_cursor_position())?;
    crop_point(cursor, at.translation, computed.size())
}

/// The flat image's pointer for this frame's tile input ([`PointerSet`]), from the cursor now
/// and the image's place as last laid out. Nothing while the device has the pointer
/// ([`pointer_source`]). Until the image stands up the last place stays: the hand-built page
/// compares it with a row's hover, so a reopening under a resting cursor selects nothing
/// (`stats::View::drawn_at`).
pub fn flat_pointer(
    pip: Res<PipBoy>,
    screen: Option<ResMut<PipBoyScreen>>,
    flat: Query<(&ComputedNode, &UiGlobalTransform), With<FlatScreen>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    aid: Res<PipBoyAid>,
) {
    let Some(mut screen) = screen else { return };
    if pointer_source(pip.frame, screen.on_model) != PointerSource::Flat {
        return;
    }
    let Some((computed, at)) = flat.iter().next().filter(|_| pip.phase == Phase::Up) else { return };
    let now = flat_point(&aid, &windows, computed, at);
    if screen.pointer != now {
        screen.pointer = now;
    }
}

/// Pass the pointer on the screen, and the mouse's buttons, to the menus' nodes (as Bevy's
/// `render_ui_to_texture` example does).
#[allow(clippy::too_many_arguments)]
fn drive_pointer(
    mut commands: Commands,
    pip: Res<PipBoy>,
    screen: Option<Res<PipBoyScreen>>,
    mut last: Local<Option<Vec2>>,
    mut spawned: Local<bool>,
    mut window_events: MessageReader<WindowEvent>,
    mut out: MessageWriter<PointerInput>,
    mut aid: ResMut<PipBoyAid>,
) {
    let Some(screen) = screen else { return };
    if !*spawned {
        commands.spawn(PIPBOY_POINTER);
        *spawned = true;
    }
    let target = RenderTarget::Image(screen.target());
    let Some(target) = target.normalize(None) else { return };
    let at = screen.pointer.filter(|_| pip.open());
    // Off the screen the pointer goes to a corner no node is in.
    let position = at.unwrap_or(Vec2::new(TARGET.x as f32 - 1.0, TARGET.y as f32 - 1.0));
    if Some(position) != *last {
        let delta = last.map_or(Vec2::ZERO, |l| position - l);
        out.write(PointerInput::new(PIPBOY_POINTER, Location { target: target.clone(), position }, PointerAction::Move { delta }));
        *last = Some(position);
    }
    for e in window_events.read() {
        let WindowEvent::MouseButtonInput(input) = e else { continue };
        if at.is_none() {
            continue;
        }
        let button = match input.button {
            MouseButton::Left => PointerButton::Primary,
            MouseButton::Right => PointerButton::Secondary,
            MouseButton::Middle => PointerButton::Middle,
            _ => continue,
        };
        let action = match input.state {
            ButtonState::Pressed => PointerAction::Press(button),
            ButtonState::Released => PointerAction::Release(button),
        };
        out.write(PointerInput::new(PIPBOY_POINTER, Location { target: target.clone(), position }, action));
    }
    // The test aid's click: a frame on the spot first, then a press, then a release.
    let aid_button = if aid.right { PointerButton::Secondary } else { PointerButton::Primary };
    match aid.click {
        3 => aid.click = 2,
        1 | 2 if at.is_none() => {
            warn!("test aid: pipboy click at {:?}: not on the screen", aid.target);
            aid.click = 0;
        }
        2 => {
            out.write(PointerInput::new(PIPBOY_POINTER, Location { target: target.clone(), position }, PointerAction::Press(aid_button)));
            aid.click = 1;
        }
        1 => {
            out.write(PointerInput::new(PIPBOY_POINTER, Location { target, position }, PointerAction::Release(aid_button)));
            info!("test aid: pipboy clicked the screen at ({:.0}, {:.0})", position.x, position.y);
            aid.click = 0;
        }
        _ => {}
    }
}

/// The page or tab clicked on the screen: a node with this is clickable.
#[derive(Component, Clone, Copy, Debug)]
pub struct Clickable(pub PipBoyInput);

/// Send a [`Clickable`]'s input when it is clicked.
pub fn on_click(click: On<PointerClick>, nodes: Query<&Clickable>, mut out: MessageWriter<PipBoyInput>) {
    if let Ok(c) = nodes.get(click.entity) {
        out.write(c.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warming(covered: bool, device: bool) -> Warming {
        Warming { since: std::time::Instant::now(), frames: 0, asked_at: 0, covered, device, saved_frame: Frame::Device, saved_page: None, drawn: false }
    }

    #[test]
    fn hidden_warming_draws_without_opening_a_menu_or_advancing_the_clip() {
        let mut p = PipBoy { warming: Some(warming(true, true)), ..default() };
        assert!(p.shown() && p.on_arm());
        assert!(!p.open());
        assert_eq!(p.warm_clip(), Some(HOLD_SECONDS));
        assert!(!p.advance(9.0));
        assert_eq!((p.phase, p.clip_time, p.saved_pov), (Phase::Down, 0.0, None));
        p.warming = None;
        assert!(!p.shown() && !p.on_arm());
    }

    #[test]
    fn a_held_raise_can_close_and_a_screen_only_warm_has_no_arm() {
        let mut p = PipBoy::default();
        p.raise();
        p.warming = Some(warming(false, true));
        assert_eq!(p.warm_clip(), Some(0.0));
        assert!(!p.advance(9.0));
        assert_eq!(p.clip_time, 0.0);
        p.lower();
        assert_eq!(p.phase, Phase::Lowering);
        p.warming = Some(warming(true, false));
        assert!(!p.on_arm());
        assert_eq!(p.warm_clip(), None);
    }

    #[test]
    fn tab_and_escape_dispatch_close_through_the_held_raise_key_path() {
        for key in [KeyCode::Tab, KeyCode::Escape] {
            let mut app = App::new();
            app.init_resource::<ButtonInput<KeyCode>>()
                .init_resource::<crate::menu::PromptKeys>()
                .init_resource::<ActiveMenu>()
                .init_resource::<repair::PipRepair>()
                .init_resource::<item_mod::PipItemMod>()
                .add_message::<crate::menu::MenuInput>()
                .insert_resource(PipBoy { phase: Phase::Raising, warming: Some(warming(false, true)), ..default() })
                .add_systems(Update, close_keys.run_if(|pip: Res<PipBoy>| pip.opening()).run_if(takes_keys));
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
            app.update();
            let messages: Vec<_> = app.world_mut().resource_mut::<Messages<crate::menu::MenuInput>>().drain().collect();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].0, crate::menu::model::Command::Close);
        }
    }

    #[test]
    fn a_held_opening_reports_the_real_page_but_startup_is_silent() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.insert_resource(repair::PipRepair::default());
        world.insert_resource(item_mod::PipItemMod::default());
        world.insert_resource(crate::script::ScriptState::over_no_plugins());
        for page in [Page::Stats, Page::Data] {
            world.resource_mut::<crate::script::ScriptState>().rt.as_mut().unwrap().game.open_menu(menus::INVENTORY);
            world.insert_resource(PipBoy { page, phase: Phase::Raising, warming: Some(warming(false, true)), ..default() });
            world.run_system_once(report_menu).unwrap();
            assert_eq!(world.resource::<crate::script::ScriptState>().rt.as_ref().unwrap().game.menu,
                Some(if page == Page::Stats { menus::STATS } else { menus::MAP }));
            world.resource_mut::<crate::script::ScriptState>().rt.as_mut().unwrap().game.close_menu();
            world.resource_mut::<PipBoy>().warming.as_mut().unwrap().covered = true;
            world.run_system_once(report_menu).unwrap();
            assert_eq!(world.resource::<crate::script::ScriptState>().rt.as_ref().unwrap().game.menu, None);
        }
    }

    #[test]
    fn the_screen_camera_stops_after_a_hidden_warm() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let camera = world.spawn(Camera { is_active: false, ..default() }).id();
        world.insert_resource(PipBoyScreen { camera, image: Handle::default(), on_model: false, pointer: None, scale: 1.0 });
        world.insert_resource(device_screen::ScreenFx::default());
        world.insert_resource(PipBoy { warming: Some(warming(true, true)), ..default() });
        world.run_system_once(camera_on).unwrap();
        assert!(world.get::<Camera>(camera).unwrap().is_active);
        world.resource_mut::<PipBoy>().warming = None;
        world.run_system_once(camera_on).unwrap();
        assert!(!world.get::<Camera>(camera).unwrap().is_active);
    }

    #[test]
    fn one_system_finds_the_pointer_whatever_the_frame_and_the_model_say() {
        assert_eq!(pointer_source(Frame::Device, true), PointerSource::Device);
        assert_eq!(pointer_source(Frame::Device, false), PointerSource::Flat, "nothing shows the model's screen");
        assert_eq!(pointer_source(Frame::Screen, true), PointerSource::Flat, "the frame changed, the body has not let go yet");
        assert_eq!(pointer_source(Frame::Screen, false), PointerSource::Flat);
    }

    #[test]
    fn the_flat_pointer_keeps_its_last_place_while_the_image_is_not_up() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.insert_resource(PipBoy::default());
        world.insert_resource(PipBoyAid::default());
        let camera = world.spawn_empty().id();
        let last = Some(Vec2::new(200.0, 240.0));
        world.insert_resource(PipBoyScreen { image: Handle::default(), camera, on_model: false, pointer: last, scale: 1.0 });
        assert_ne!(world.resource::<PipBoy>().phase, Phase::Up);
        world.run_system_once(flat_pointer).unwrap();
        assert_eq!(world.resource::<PipBoyScreen>().pointer, last, "down, with no image");
    }

    #[test]
    fn a_cursor_is_found_on_the_flat_image_by_its_crop() {
        // An image twice the crop's size, its middle at 1000, 800.
        let size = SCREEN_CROP * 2.0;
        let centre = Vec2::new(1000.0, 800.0);
        assert_eq!(crop_point(centre, centre, size), Some(SCREEN_CROP / 2.0));
        assert_eq!(crop_point(centre - size / 2.0, centre, size), Some(Vec2::ZERO));
        assert_eq!(crop_point(centre - size / 2.0 - Vec2::X, centre, size), None);
        assert_eq!(crop_point(centre + size / 2.0, centre, size), None);
    }

    #[test]
    fn each_page_has_its_menu_code() {
        assert_eq!(Page::ALL.map(|p| menu_code(p, false, false)), [1003, 1002, 1023]);
        assert!(Page::ALL.iter().all(|&p| menu_code(p, true, false) == 1035));
        assert!(Page::ALL.iter().all(|&p| menu_code(p, false, true) == 1061));
        assert!(Page::ALL.iter().all(|&p| menus::answers(menus::PIPBOY, menu_code(p, false, false))));
        assert!(menus::answers(menus::PIPBOY, menus::ITEM_MOD));
    }

    #[test]
    fn the_clip_holds_at_hit_while_up() {
        let mut p = PipBoy::default();
        p.raise();
        assert_eq!(p.phase, Phase::Raising);
        assert!(!p.advance(0.2));
        assert!(p.advance(0.2));
        assert_eq!((p.phase, p.clip_time), (Phase::Up, HOLD_SECONDS));
        // Held there.
        assert!(!p.advance(5.0));
        assert_eq!(p.clip_time, HOLD_SECONDS);
        // Down: on from Hit to the end.
        p.lower();
        assert!(!p.advance(0.1));
        assert!((p.clip_time - (HOLD_SECONDS + 0.1)).abs() < 1e-6);
        // Put back up on the way down: up from as far down as it got.
        p.raise();
        assert_eq!(p.phase, Phase::Raising);
        assert!((p.clip_time - (HOLD_SECONDS - 0.1)).abs() < 1e-6);
        assert!(p.advance(0.1));
        p.lower();
        assert!(p.advance(1.0));
        assert_eq!((p.phase, p.clip_time), (Phase::Down, 0.0));
    }

    /// Under `--menu-pause original` the inventory stops the virtual clock: the Pip-Boy
    /// still comes up, on real time (#1330).
    #[test]
    fn the_pipboy_comes_up_while_a_menu_holds_the_picture() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .insert_resource(crate::pause::PlayClock::frozen_frame(0.2))
            .init_resource::<PlayerControl>()
            .init_resource::<PipBoy>()
            .add_systems(Update, advance);
        app.world_mut().resource_mut::<PipBoy>().raise();
        // The virtual clock gives no time (its delta is 0); the menus' clock does.
        app.update();
        assert_eq!(app.world().resource::<PipBoy>().phase, Phase::Raising);
        app.update();
        assert_eq!(app.world().resource::<PipBoy>().phase, Phase::Up);
    }

    #[test]
    fn use_pipboy_off_always_takes_the_pipboy_and_glove_off() {
        use fnv_sim::FormId;
        let (pip, glove, pimp, shirt) = (FormId(PIPBOY), FormId(PIPBOY_GLOVE), FormId(0x0001_0001), FormId(0x0002_0002));
        let in_slot = |id: FormId| id == pip || id == pimp;
        // Off: whatever is worn in the slot and the glove come off, changed or not (a
        // loaded save, a script's equip); other clothes stay.
        for changed in [true, false] {
            assert_eq!(use_changes(false, changed, &[pimp, glove, shirt], &[], in_slot), vec![(pimp, false), (glove, false)]);
            assert!(use_changes(false, changed, &[shirt], &[pip, glove], in_slot).is_empty());
        }
    }

    #[test]
    fn use_pipboy_on_puts_them_on_once_and_the_glove_only_with_a_pipboy() {
        use fnv_sim::FormId;
        let (pip, glove, pimp) = (FormId(PIPBOY), FormId(PIPBOY_GLOVE), FormId(0x0001_0001));
        let in_slot = |id: FormId| id == pip || id == pimp;
        // Turned on: the Pip-Boy 3000 before another carried Pip-Boy, then the glove.
        assert_eq!(use_changes(true, true, &[], &[pimp, glove, pip], in_slot), vec![(pip, true), (glove, true)]);
        // A Pip-Boy already worn: only the glove.
        assert_eq!(use_changes(true, true, &[pimp], &[pimp, glove], in_slot), vec![(glove, true)]);
        // No Pip-Boy carried: not the glove alone either.
        assert!(use_changes(true, true, &[], &[glove], in_slot).is_empty());
        // On and unchanged: the player may take them off by hand.
        assert!(use_changes(true, false, &[], &[pip, glove], in_slot).is_empty());
    }

    #[test]
    fn the_screen_menu_is_used_without_a_pipboy() {
        assert_eq!(frame_for(false, true), Frame::Device);
        assert_eq!(frame_for(true, true), Frame::Screen);
        assert_eq!(frame_for(false, false), Frame::Screen);
        assert_eq!(frame_for(true, false), Frame::Screen);
    }

    #[test]
    fn the_screen_menu_opens_and_shuts_at_once() {
        let mut p = PipBoy { frame: Frame::Screen, ..default() };
        p.raise();
        assert_eq!(p.phase, Phase::Up);
        assert!(p.open() && p.shown() && !p.on_arm());
        p.apply(PipBoyInput::Page(Page::Data));
        p.apply(PipBoyInput::Tab(2));
        p.lower();
        assert_eq!((p.phase, p.clip_time), (Phase::Down, 0.0));
        assert!(!p.shown());
        // The page and its tab are kept for the next time, in either frame.
        p.frame = Frame::Device;
        p.raise();
        assert_eq!(p.phase, Phase::Raising);
        assert!(p.on_arm());
        assert_eq!((p.page, p.tab()), (Page::Data, 2));
    }

    #[test]
    fn each_page_keeps_its_tab() {
        let mut p = PipBoy::default();
        p.apply(PipBoyInput::Tab(2));
        p.apply(PipBoyInput::Page(Page::Stats));
        assert_eq!(p.tab(), 0);
        p.apply(PipBoyInput::StepTab(-1));
        assert_eq!(p.tab(), 4);
        p.apply(PipBoyInput::Page(Page::Items));
        assert_eq!(p.tab(), 2);
        p.apply(PipBoyInput::StepTab(3));
        assert_eq!(p.tab(), 0);
    }
}
