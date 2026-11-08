//! The controller (docs/design/controller.md): its buttons and stick directions, their
//! names, the game's default controller buttons, and the menu keys a controller stands
//! in for.
//!
//! **Which device works the controls** is the player's choice, `Input Device` in
//! `[Controls]` of `NVForever.ini` and the first row of the settings page's Controls tab
//! (`crate::bindings::InputDevice`) [design; owner 2026-10-16: keyboard and mouse, or a
//! controller, "depending on a setting under controls"]. With Controller chosen the
//! controls answer the controller only (`crate::bindings::ControlInput`), the left stick
//! moves, the right stick looks; Esc and the console's key still work.
//!
//! **The game's controller buttons** are the fourth byte `JJ` of each `[Controls]` value
//! of the owner's `FalloutPrefs.ini` (`crate::bindings::GAME_CONTROLS`) [observed
//! 2026-10-09; the byte's meaning is hypothesis, [`game_pad`]].
//!
//! **Menus** read keys, so while a controller works the controls its buttons stand in
//! for them ([`menu_keys`]): Start is Esc everywhere; in a menu (not playing) the D-pad
//! is the arrow keys, A Enter, B Esc, LB and RB, LT and RT Page Up and Page Down
//! [design]. On top of that each menu's own buttons press the key NVForever gives the
//! same button there ([`menu_buttons`]), as the original's menu files assign them
//! (`_xbox_button` on each button tile, e.g. `IM_DropButton` `&xbuttonx;`) [documented:
//! the vanilla menu files, read with `fnvtool ui tree`]; a menu's own button wins over
//! the shared ones (on the ITEMS page LB is Mod, not Page Up).

use bevy::prelude::*;

/// A controller input a control can be bound to: a button, or the left stick pushed one
/// way (the movement controls).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadInput {
    Button(GamepadButton),
    Stick(StickDir),
}

/// A way the left stick is pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StickDir {
    Up,
    Down,
    Left,
    Right,
}

impl StickDir {
    pub const ALL: [StickDir; 4] = [StickDir::Up, StickDir::Down, StickDir::Left, StickDir::Right];

    fn index(self) -> usize {
        self as usize
    }

    /// How far a stick at `v` is pushed this way (0 to 1).
    pub fn amount(self, v: Vec2) -> f32 {
        match self {
            StickDir::Up => v.y,
            StickDir::Down => -v.y,
            StickDir::Left => -v.x,
            StickDir::Right => v.x,
        }
        .max(0.0)
    }
}

/// A stick pushed this far one way counts as that way's button pressed [design].
pub const STICK_PRESS: f32 = 0.5;
/// Below this push of the left stick the player walks, beyond it runs (with Always Run
/// on) [design].
pub const STICK_WALK_BELOW: f32 = 0.6;
/// A push of a stick under this counts as none [design; Bevy's own dead zone comes
/// first].
pub const STICK_DEAD: f32 = 0.12;

/// The game's controller code `JJ` of a `[Controls]` value as a controller input
/// [hypothesis]. The fit: the original's console layout puts Activate on A, the Pip-Boy
/// on B, Ready/Reload on X, Jump on Y, Aim on LT, Attack on RT, V.A.T.S. on RB, Wait on
/// Back and Sneak on a press of the left stick [documented: published Xbox 360 control
/// lists for the game, e.g. magicgameworld.com "Fallout: New Vegas Controls"], and the
/// owner's values give those controls `0A`, `0B`, `0C`, `0D`, `10`, `11`, `0E`, `07`,
/// `08` [observed]. A to Y follow the order of XInput's button bits, as Back then the
/// left then the right stick's press do [documented: Microsoft `XINPUT_GAMEPAD`
/// `wButtons`], so `09` (Grab) is taken for the right stick's press and `0F` (Toggle POV)
/// for the one shoulder left, LB. `13`, `14`, `16`, `17` (Forward, Back, Slide Right,
/// Slide Left) are taken for the left stick's directions. `01` (Ammo Swap) is not
/// placed; `FF` is none.
pub fn game_pad(code: u8) -> Option<PadInput> {
    use GamepadButton::*;
    Some(match code {
        0x07 => PadInput::Button(Select),
        0x08 => PadInput::Button(LeftThumb),
        0x09 => PadInput::Button(RightThumb),
        0x0A => PadInput::Button(South),
        0x0B => PadInput::Button(East),
        0x0C => PadInput::Button(West),
        0x0D => PadInput::Button(North),
        0x0E => PadInput::Button(RightTrigger),
        0x0F => PadInput::Button(LeftTrigger),
        0x10 => PadInput::Button(LeftTrigger2),
        0x11 => PadInput::Button(RightTrigger2),
        0x13 => PadInput::Stick(StickDir::Up),
        0x14 => PadInput::Stick(StickDir::Down),
        0x16 => PadInput::Stick(StickDir::Right),
        0x17 => PadInput::Stick(StickDir::Left),
        _ => return None,
    })
}

/// The inputs' names in the INI and on screen ("A)" under the crosshair) [design: an
/// Xbox controller's, as the original's button pictures are (`general_button_a.dds` …
/// `general_button_rs.dds`) [observed: `button_xbox_hint.xml`]].
pub const PAD_NAMES: &[(PadInput, &str)] = {
    use GamepadButton::*;
    use PadInput::{Button as B, Stick as S};
    &[
        (B(South), "A"), (B(East), "B"), (B(West), "X"), (B(North), "Y"),
        (B(LeftTrigger), "LB"), (B(RightTrigger), "RB"), (B(LeftTrigger2), "LT"), (B(RightTrigger2), "RT"),
        (B(LeftThumb), "LS"), (B(RightThumb), "RS"), (B(Select), "Back"), (B(Start), "Start"),
        (B(DPadUp), "D-Pad Up"), (B(DPadDown), "D-Pad Down"), (B(DPadLeft), "D-Pad Left"), (B(DPadRight), "D-Pad Right"),
        (S(StickDir::Up), "Left Stick Up"), (S(StickDir::Down), "Left Stick Down"),
        (S(StickDir::Left), "Left Stick Left"), (S(StickDir::Right), "Left Stick Right"),
    ]
};

/// Start opens and closes the pause menu: no control may take it.
pub const RESERVED_PAD: [PadInput; 1] = [PadInput::Button(GamepadButton::Start)];

/// The input's name in the INI ([`PAD_NAMES`]: the Xbox names whatever the button style,
/// so every INI keeps loading) [design, #1452].
pub fn pad_name(p: PadInput) -> &'static str {
    PAD_NAMES.iter().find(|(x, _)| *x == p).map_or("?", |(_, n)| n)
}

/// The PlayStation controller's names of the buttons whose Xbox names differ [design,
/// #1452: Select and Start as on the PS3-era pad, the game's era; the face buttons by
/// their shapes, drawn as NVForever's own glyphs where a picture fits ([`Glyph`])].
pub const PLAYSTATION_NAMES: &[(GamepadButton, &str)] = {
    use GamepadButton::*;
    &[
        (South, "Cross"), (East, "Circle"), (West, "Square"), (North, "Triangle"),
        (LeftTrigger, "L1"), (RightTrigger, "R1"), (LeftTrigger2, "L2"), (RightTrigger2, "R2"),
        (LeftThumb, "L3"), (RightThumb, "R3"), (Select, "Select"), (Start, "Start"),
    ]
};

/// The input's name on screen with the labels `l`: the PlayStation name of a button that
/// has one, else [`pad_name`] (the Xbox names, the D-pad's and the stick's).
pub fn pad_label(p: PadInput, l: Labels) -> &'static str {
    match (p, l) {
        (PadInput::Button(b), Labels::PlayStation) => PLAYSTATION_NAMES.iter().find(|(x, _)| *x == b).map_or_else(|| pad_name(p), |(_, n)| n),
        _ => pad_name(p),
    }
}

/// The labels a controller's names take now: those in use, or the Xbox ones while the
/// keyboard's are (a name asked for before the frame's [`sync_labels`]).
pub fn pad_labels() -> Labels {
    match labels() {
        Labels::Keys => Labels::Xbox,
        l => l,
    }
}

/// [`pad_label`] with [`pad_labels`].
pub fn pad_shown(p: PadInput) -> &'static str {
    pad_label(p, pad_labels())
}

/// Which names a controller's buttons go by: the settings page's Button style (Controller
/// group), kept in `[Settings]` of NVForever.ini as `button-style` [design, #1452]. Auto
/// follows the controller in use ([`PadInUse`]); Xbox and PlayStation hold whatever the
/// controller (one whose name is not recognised).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonStyle {
    #[default]
    Auto,
    Xbox,
    PlayStation,
}

impl ButtonStyle {
    pub const ALL: [ButtonStyle; 3] = [ButtonStyle::Auto, ButtonStyle::Xbox, ButtonStyle::PlayStation];

    pub fn name(self) -> &'static str {
        match self {
            ButtonStyle::Auto => "Auto",
            ButtonStyle::Xbox => "Xbox",
            ButtonStyle::PlayStation => "PlayStation",
        }
    }

    pub fn by_name(name: &str) -> Option<ButtonStyle> {
        ButtonStyle::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The labels with a controller; `playstation`: the controller in use is one.
    pub fn labels(self, playstation: bool) -> Labels {
        match self {
            ButtonStyle::Xbox => Labels::Xbox,
            ButtonStyle::PlayStation => Labels::PlayStation,
            ButtonStyle::Auto if playstation => Labels::PlayStation,
            ButtonStyle::Auto => Labels::Xbox,
        }
    }
}

fn style_pick(w: &mut World, name: &str) -> bool {
    match ButtonStyle::by_name(name) {
        Some(s) => {
            info!("controller: button style {}", s.name());
            *w.resource_mut::<ButtonStyle>() = s;
            true
        }
        None => false,
    }
}

/// Whether a controller's reported name is a PlayStation pad's [design, #1452: the names
/// the platform's drivers and SDL's controller list give them, e.g. "DualSense Wireless
/// Controller", "DUALSHOCK 4", the bare "Wireless Controller" of a DualShock 4 or
/// DualSense, "PS3 Controller"; a name that says Xbox never is].
pub fn playstation_pad(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.contains("xbox") || n.contains("xinput") {
        return false;
    }
    ["dualsense", "dualshock", "playstation", "sony", "ps3", "ps4", "ps5"].iter().any(|w| n.contains(w)) || n.trim() == "wireless controller"
}

/// The controller in use: the last one connected, or the last one whose button went down,
/// so the labels follow a pad picked up mid-game [design, #1452].
#[derive(Resource, Debug, Default, Clone)]
pub struct PadInUse {
    pub name: Option<String>,
    pub playstation: bool,
}

fn track_pad(pads: Query<(Entity, &Gamepad, Option<&Name>)>, added: Query<Entity, Added<Gamepad>>, mut in_use: ResMut<PadInUse>) {
    let now = pads.iter().find(|(e, g, _)| added.contains(*e) || g.get_just_pressed().next().is_some());
    let Some((e, _, name)) = now else { return };
    let name = name.map_or_else(|| format!("controller {e}"), |n| n.as_str().to_string());
    if in_use.name.as_deref() != Some(name.as_str()) {
        let playstation = playstation_pad(&name);
        info!("controller: {name} in use ({} names)", if playstation { "PlayStation" } else { "Xbox" });
        *in_use = PadInUse { name: Some(name), playstation };
    }
}

// ---- NVForever's own PlayStation glyphs [design, #1452] --------------------------------

/// A PlayStation face button's shape, drawn by NVForever (no game or Sony art).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Cross,
    Circle,
    Square,
    Triangle,
}

impl Glyph {
    pub const ALL: [Glyph; 4] = [Glyph::Cross, Glyph::Circle, Glyph::Square, Glyph::Triangle];

    /// The face button's glyph under the PlayStation labels.
    pub fn of(b: GamepadButton) -> Option<Glyph> {
        Some(match b {
            GamepadButton::South => Glyph::Cross,
            GamepadButton::East => Glyph::Circle,
            GamepadButton::West => Glyph::Square,
            GamepadButton::North => Glyph::Triangle,
            _ => return None,
        })
    }

    /// The glyph a label names ("Cross"), for the drawing that shows the picture in
    /// place of the word.
    pub fn named(label: &str) -> Option<Glyph> {
        Glyph::ALL.into_iter().find(|g| g.name() == label)
    }

    pub fn name(self) -> &'static str {
        match self {
            Glyph::Cross => "Cross",
            Glyph::Circle => "Circle",
            Glyph::Square => "Square",
            Glyph::Triangle => "Triangle",
        }
    }

    /// The signed distance from the point `p` (the picture's middle at 0, its half side 1)
    /// to the glyph's ink: negative inside the drawn shape, positive outside. The four
    /// shapes are NVForever's own PlayStation glyphs — a solid cross and three bold ring
    /// outlines — tuned to read as one family at the size they are drawn [design].
    fn distance(self, p: Vec2) -> f32 {
        match self {
            Glyph::Cross => {
                // A solid X: two flat-ended arms crossing the middle, each a box of
                // half length `l` along its diagonal and half width `t` [design].
                let (l, t) = (0.90, 0.09);
                let s = std::f32::consts::FRAC_1_SQRT_2; // cos and sin of 45 degrees.
                let arm1 = box_distance(Vec2::new(p.x * s + p.y * s, -p.x * s + p.y * s), l, t);
                let arm2 = box_distance(Vec2::new(p.x * s - p.y * s, p.x * s + p.y * s), l, t);
                arm1.min(arm2)
            }
            Glyph::Circle => {
                // A ring: outer radius `outer`, the band `thickness` wide.
                let (outer, thickness) = (0.68, 0.17);
                (p.length() - outer).max(outer - thickness - p.length())
            }
            Glyph::Square => {
                // A ring: outer half side `outer`, `thickness` wide, corners eased a touch.
                let (outer, thickness, radius) = (0.66, 0.17, 0.04);
                let inner = outer - thickness;
                rounded_box_distance(p, outer, radius).max(-rounded_box_distance(p, inner, (radius - thickness).max(0.0)))
            }
            Glyph::Triangle => {
                // A ring of three edges, each of half-thickness `t`; the apex points up
                // (y down in the picture, so the apex's y is negative).
                let t = 0.085;
                let (a, b, c) = (Vec2::new(0.0, -0.5625), Vec2::new(0.625, 0.5625), Vec2::new(-0.625, 0.5625));
                segment(p, a, b).min(segment(p, b, c)).min(segment(p, c, a)) - t
            }
        }
    }

    /// The glyph's coverage (0 to 1) of the pixel at `x`, `y` of a `size` square picture:
    /// the ink of [`distance`] with a soft edge and an amber halo, matching the game's
    /// fonts, which bake their glow into the glyph bitmaps [design, matched by eye to
    /// font 7]. Distances are normalized to the picture's half side so the falloff scales
    /// with the glyph as it is drawn.
    pub fn coverage(self, x: u32, y: u32, size: u32) -> f32 {
        let half = size as f32 / 2.0;
        let p = (Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - Vec2::splat(half)) / half;
        // The signed distance, normalized to the picture's half side, negative inside the
        // ink.
        let d = self.distance(p);
        // A solid core with a narrow soft edge: the font's stroke holds a solid band, so
        // the glyph must too, or it reads as blurred rather than lit. The ramp is narrower
        // than the stroke, so the thinner strokes above keep a solid core.
        let x = ((d + 0.02) / 0.12).clamp(0.0, 1.0);
        let body = 1.0 - x * x * (3.0 - 2.0 * x);
        // A faint amber halo outside the body, the glow the fonts bake in. It fades out
        // before the picture's edge so the tile's boundary never shows.
        let halo = if d > 0.0 { 0.22 * (1.0 - d / 0.30).max(0.0) } else { 0.0 };
        (body + halo).clamp(0.0, 1.0)
    }

    /// The glyph as a white picture, its ink in the alpha (tinted by the node's colour).
    pub fn image(self, size: u32) -> Image {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        let mut data = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let a = (self.coverage(x, y, size) * 255.0).round() as u8;
                data.extend_from_slice(&[255, 255, 255, a]);
            }
        }
        Image::new(Extent3d { width: size, height: size, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)
    }
}

/// The distance from `p` to the line segment `a`..`b`.
fn segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// The signed distance to a sharp axis-aligned box of half extents `(hx, hy)`, negative
/// inside.
fn box_distance(p: Vec2, hx: f32, hy: f32) -> f32 {
    let q = p.abs() - Vec2::new(hx, hy);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
}

/// The signed distance to a rounded square of half side `h` and corner radius `r`,
/// negative inside.
fn rounded_box_distance(p: Vec2, h: f32, r: f32) -> f32 {
    let q = p.abs() - Vec2::splat(h - r);
    q.max(Vec2::ZERO).length() - r + q.x.max(q.y).min(0.0)
}


/// The input of a name (without case); `None` for none or an unknown name.
pub fn parse_pad(name: &str) -> Option<PadInput> {
    PAD_NAMES.iter().find(|(_, n)| n.eq_ignore_ascii_case(name.trim())).map(|(p, _)| *p)
}

/// The left stick's directions as buttons, this frame and the last (for a just-pressed),
/// any controller's.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct StickButtons {
    pub now: [bool; 4],
    pub before: [bool; 4],
}

impl StickButtons {
    pub fn pressed(&self, d: StickDir) -> bool {
        self.now[d.index()]
    }

    pub fn just_pressed(&self, d: StickDir) -> bool {
        self.now[d.index()] && !self.before[d.index()]
    }

    pub fn just_released(&self, d: StickDir) -> bool {
        !self.now[d.index()] && self.before[d.index()]
    }
}

/// The furthest-pushed stick of the connected controllers (`left` or right), dead zone
/// taken off.
pub fn stick(pads: &Query<&Gamepad>, left: bool) -> Vec2 {
    let v = pads.iter().map(|g| if left { g.left_stick() } else { g.right_stick() }).max_by(|a, b| a.length_squared().total_cmp(&b.length_squared())).unwrap_or(Vec2::ZERO);
    if v.length() < STICK_DEAD {
        Vec2::ZERO
    } else {
        v.clamp_length_max(1.0)
    }
}

fn update_stick_buttons(pads: Query<&Gamepad>, mut s: ResMut<StickButtons>) {
    let v = stick(&pads, true);
    s.before = s.now;
    for d in StickDir::ALL {
        s.now[d.index()] = d.amount(v) >= STICK_PRESS;
    }
}

/// The menu keys a controller's buttons hold down now (module notes).
#[derive(Resource, Debug, Default)]
pub struct MenuKeys {
    held: Vec<KeyCode>,
}

/// Start: Esc, always.
const ALWAYS: [(GamepadButton, KeyCode); 1] = [(GamepadButton::Start, KeyCode::Escape)];
/// In a menu.
const IN_MENUS: [(GamepadButton, KeyCode); 10] = [
    (GamepadButton::DPadUp, KeyCode::ArrowUp),
    (GamepadButton::DPadDown, KeyCode::ArrowDown),
    (GamepadButton::DPadLeft, KeyCode::ArrowLeft),
    (GamepadButton::DPadRight, KeyCode::ArrowRight),
    (GamepadButton::South, KeyCode::Enter),
    (GamepadButton::East, KeyCode::Escape),
    (GamepadButton::LeftTrigger, KeyCode::PageUp),
    (GamepadButton::RightTrigger, KeyCode::PageDown),
    (GamepadButton::LeftTrigger2, KeyCode::PageUp),
    (GamepadButton::RightTrigger2, KeyCode::PageDown),
];

/// The menu open on top ([`fnv_game::Game::top_menu`]'s MenuMode code) and the Pip-Boy's
/// tab, for [`menu_buttons`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuAt {
    pub code: Option<u16>,
    pub tab: usize,
}

/// The page of a menu whose own buttons rest while a help box is up over it.
pub const HELP_PAGE: usize = 100;
/// The STATS page's status tab on its RAD view (`crate::pipboy::stats::StatsView`).
pub const STATS_RAD: usize = 10;
/// The caravan screen's pages: its stages (`crate::caravan::Stage`).
pub const CARAVAN_BET: usize = 0;
pub const CARAVAN_DECK: usize = 1;
pub const CARAVAN_PLAY: usize = 2;
/// Poker has no MenuMode code (NVForever's own game): this stands for it in [`MenuAt`]
/// [design], above any code the game's menus use (1001 to 1090).
pub const POKER_MENU: u16 = 0xF001;
/// The pause menu (NVForever's own, over any menu): this stands for it in [`MenuAt`]
/// [design].
pub const PAUSE_MENU: u16 = 0xF002;
pub const PAUSE_AT: MenuAt = MenuAt { code: Some(PAUSE_MENU), tab: 0 };
/// Poker's page between hands (Next Hand).
pub const POKER_BETWEEN_HANDS: usize = 1;

/// The screens a menu's controller buttons depend on beyond its MenuMode code: the
/// Pip-Boy's tab, a minigame's help box or stage, the poker table.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MenuPages<'w> {
    pip: Option<Res<'w, crate::pipboy::PipBoy>>,
    caravan: Option<Res<'w, crate::caravan::CaravanScreen>>,
    blackjack: Option<Res<'w, crate::blackjack::BlackjackScreen>>,
    roulette: Option<Res<'w, crate::roulette::RouletteScreen>>,
    slots: Option<Res<'w, crate::slots::SlotsScreen>>,
    lockpick: Option<Res<'w, crate::lockpick::LockpickScreen>>,
    poker: Option<Res<'w, crate::poker::PokerScreen>>,
    stats: Option<Res<'w, crate::pipboy::stats::StatsView>>,
}

impl MenuPages<'_> {
    /// Where the menus are, the game's top menu being `code`.
    pub fn at(&self, code: Option<u16>) -> MenuAt {
        if let Some(s) = self.poker.as_ref().and_then(|p| p.0.as_ref()) {
            return s.hint_at();
        }
        if let Some(s) = self.caravan.as_ref().and_then(|c| c.0.as_ref()).filter(|_| code == Some(crate::caravan::MENU_CARAVAN)) {
            return s.hint_at();
        }
        let help = self.blackjack.as_ref().is_some_and(|b| b.0.as_ref().is_some_and(|s| s.help.is_some()))
            || self.roulette.as_ref().is_some_and(|b| b.0.as_ref().is_some_and(|s| s.help.is_some()))
            || self.slots.as_ref().is_some_and(|b| b.0.as_ref().is_some_and(|s| s.help.is_some()))
            || self.lockpick.as_ref().is_some_and(|b| b.0.as_ref().is_some_and(|s| s.help.is_some()));
        let tab = if help { HELP_PAGE } else { self.pip.as_ref().map_or(0, |p| p.tab()) };
        let rad = code == Some(fnv_game::game::menus::STATS) && tab == 0 && self.stats.as_ref().is_some_and(|s| s.status == 1);
        MenuAt { code, tab: if rad { STATS_RAD } else { tab } }
    }
}

/// A menu's own controller buttons: the key each stands in for there. The button each
/// menu file gives a control [documented: `_xbox_button` of the button tiles, below as
/// "file"], pressing the key NVForever's hand-built menu answers for that control.
pub fn menu_buttons(at: MenuAt) -> &'static [(GamepadButton, KeyCode)] {
    use fnv_game::game::menus;
    use GamepadButton::{East, LeftTrigger, LeftTrigger2, North, RightTrigger2, South, West};
    // A help box over a minigame: its own buttons rest, so A (Enter) and B (Esc) close
    // the box as its keys do (blackjack's A is otherwise its F) [design, #1662].
    if at.tab == HELP_PAGE {
        return &[];
    }
    // The Pip-Boy: LT and RT step through its pages, STATS, ITEMS, DATA and round again,
    // through their keys F1-F3 (`crate::pipboy`); the D-pad's left and right step the tabs
    // as the arrow keys do [design: owner 2026-11-02; the original's page buttons are not in
    // its menu files].
    use KeyCode::*;
    let Some(code) = at.code else { return &[] };
    match code {
        // container_menu.xml: Take All X.
        crate::script::MENU_CONTAINER => &[(West, KeyR)],
        // barter_menu.xml: Accept X (B exits, as Esc).
        menus::BARTER => &[(West, KeyA)],
        // inventory_menu.xml: Repair Y, Mod LB (Drop X: `crate::pipboy::items`, which has
        // no key for it; Hotkey RB: the hotkeys are held digits, not built for the pad).
        menus::INVENTORY => &[(North, KeyR), (LeftTrigger, KeyX), (LeftTrigger2, F1), (RightTrigger2, F3)],
        // stats_menu.xml: Stimpak A, Doctor's Bag Y, Rad-X X on the status tab; the
        // General/Reputation switch X on the General tab.
        // The status tab's RAD view: RadAway A, Rad-X X (RadAway is not used from the
        // page yet: its button does what its key does, nothing).
        menus::STATS if at.tab == STATS_RAD => &[(South, KeyA), (West, KeyX), (LeftTrigger2, F3), (RightTrigger2, F2)],
        menus::STATS if at.tab == 0 => &[(South, KeyS), (North, KeyE), (West, KeyX), (LeftTrigger2, F3), (RightTrigger2, F2)],
        menus::STATS if at.tab == 4 => &[(West, KeyR), (LeftTrigger2, F3), (RightTrigger2, F2)],
        menus::STATS => &[(LeftTrigger2, F3), (RightTrigger2, F2)],
        // map_menu.xml: the Challenges toggle Y (R on the Notes tab).
        menus::MAP => &[(North, KeyR), (LeftTrigger2, F2), (RightTrigger2, F1)],
        // recipe_menu.xml: Accept X.
        fnv_game::crafting::RECIPE_MENU => &[(West, KeyA)],
        // repair_services_menu.xml: Repair All X.
        fnv_game::repair::REPAIR_SERVICES_MENU => &[(West, KeyA)],
        // levelup_menu.xml, trait_menu.xml: Reset Y, Continue X, Back B.
        menus::LEVEL_UP | menus::TRAITS => &[(North, KeyR), (West, KeyA), (East, KeyE)],
        // char_gen_menu.xml (the tag skills): Reset Y, Done X.
        menus::TAG_SKILLS => &[(North, KeyR), (West, KeyA)],
        // lockpick_menu.xml: Force Lock X.
        menus::LOCKPICK => &[(West, KeyF)],
        // black_jack_menu.xml: Hit A, Double/Deal Y, Split/Raise RT, Switch/Lower LT,
        // Surrender/Max Bet X, Stay/Exit B.
        fnv_game::blackjack::MENU_CODE => &[(South, KeyF), (North, KeyW), (RightTrigger2, KeyE), (LeftTrigger2, KeyQ), (West, KeyS), (East, KeyR)],
        // roulette_menu.xml: Place A, Remove Y, Finish X, Lower LT, Raise RT, Exit B.
        fnv_game::roulette::MENU_CODE => &[(South, KeyW), (North, KeyF), (West, KeyS), (LeftTrigger2, KeyQ), (RightTrigger2, KeyE), (East, KeyR)],
        // slot_machine_menu.xml: Spin A, Raise RT, Lower LT, Payouts Y, Max Bet X, Exit B.
        fnv_game::slots::MENU_CODE => &[(South, KeyW), (RightTrigger2, KeyE), (LeftTrigger2, KeyQ), (North, KeyF), (West, KeyS), (East, KeyR)],
        // tutorial_menu.xml: Next RT, Previous LT.
        menus::TUTORIAL => &[(RightTrigger2, KeyA), (LeftTrigger2, KeyS)],
        // A conversation (1009): B (East) leaves as Tab does (the Goodbye choice, or the
        // topic list's end). While the actor speaks Tab does nothing, so B does nothing
        // there too, matching the original [observed: play, issue #1657].
        menus::DIALOGUE => &[(East, Tab)],
        // Caravan (its menu files are not read yet: NVForever's choice, by the other
        // card games' pattern of A the main action, Y the second, X the third, B leave)
        // [design, #1662]. The page is the screen's stage (`crate::caravan::Session::
        // hint_at`). Bet: Accept A, Auto Match Y, Raise RT, Exit B.
        crate::caravan::MENU_CARAVAN if at.tab == CARAVAN_BET => &[(South, KeyF), (North, KeyW), (RightTrigger2, KeyA), (East, KeyR)],
        // Deck: Add A, Remove X, Randomize Y, Play Caravan RT (B is the shared Esc, which
        // leaves).
        crate::caravan::MENU_CARAVAN if at.tab == CARAVAN_DECK => &[(South, KeyW), (West, KeyA), (North, KeyS), (RightTrigger2, KeyF)],
        // Play: Select Card A, Discard Card X, Discard Track Y, Cancel B (the D-pad's left
        // and right step the hand, as the arrow keys).
        crate::caravan::MENU_CARAVAN if at.tab == CARAVAN_PLAY => &[(South, KeyW), (West, KeyQ), (North, KeyE), (East, KeyR)],
        // Poker (NVForever's own game, no menu file): Check/Call A, Raise RT, All In Y,
        // Fold X; between hands Next Hand A; Leave is B (the shared Esc), the D-pad's up
        // and down the raise [design, #1662].
        // The pause menu: X is Delete (a save on the Load page, a control's binding on
        // the Controls tab), which no other button gives [design, #1662].
        PAUSE_MENU => &[(West, Delete)],
        POKER_MENU if at.tab == POKER_BETWEEN_HANDS => &[(South, Space)],
        POKER_MENU => &[(South, KeyE), (RightTrigger2, KeyR), (North, KeyA), (West, KeyF)],
        _ => &[],
    }
}

/// Press and release the keys the controller's buttons stand for, after the input
/// systems have read the keyboard (so a menu sees a just-pressed key this frame). A key
/// goes down only as its button does (a button still held from before, as after binding
/// it on the Controls tab, presses nothing); none while the Controls tab waits for a new
/// button, which takes the buttons themselves.
#[allow(clippy::type_complexity)]
pub(crate) fn menu_keys(
    pads: Query<&Gamepad>,
    bindings: Option<Res<crate::bindings::Bindings>>,
    (state, paused, page): (Option<Res<State<crate::PlayState>>>, Option<Res<crate::pause::Paused>>, Option<Res<crate::pause::Page>>),
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mk: ResMut<MenuKeys>,
    (script, pages): (Option<Res<crate::script::ScriptState>>, MenuPages),
) {
    let waiting = page.is_some_and(|p| matches!(*p, crate::pause::Page::Controls { listening: true, .. }));
    let on = bindings.is_some_and(|b| b.device == crate::bindings::InputDevice::Controller) && !waiting;
    let in_menu = state.is_some_and(|s| *s.get() != crate::PlayState::Playing) || paused.as_ref().is_some_and(|p| p.0);
    let mut want: Vec<KeyCode> = Vec::new();
    if on {
        let at = if paused.as_ref().is_some_and(|p| p.0) { PAUSE_AT } else { pages.at(script.as_ref().and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.top_menu())) };
        let own = if in_menu { menu_buttons(at) } else { &[] };
        let shared = ALWAYS.iter().chain(if in_menu { IN_MENUS.iter() } else { [].iter() }).filter(|(b, _)| !own.iter().any(|(o, _)| o == b));
        for (b, k) in own.iter().chain(shared) {
            let down = if mk.held.contains(k) { pads.iter().any(|g| g.pressed(*b)) } else { pads.iter().any(|g| g.just_pressed(*b)) };
            if down && !want.contains(k) {
                want.push(*k);
            }
        }
    }
    for k in mk.held.clone() {
        if !want.contains(&k) {
            keys.release(k);
        }
    }
    for k in &want {
        if !mk.held.contains(k) {
            keys.press(*k);
        }
    }
    mk.held = want;
}

/// The key a controller button stands in for in a menu ([`menu_keys`]'s tables), when the
/// controller works the controls: what the menus' own key handlers then see.
pub(crate) fn menu_key(button: GamepadButton) -> Option<KeyCode> {
    ALWAYS.iter().chain(IN_MENUS.iter()).find(|(b, _)| *b == button).map(|(_, k)| *k)
}

/// [`menu_key`] in the menu `at`: what its own buttons stand for there first, as
/// [`menu_keys`] has them ([`menu_buttons`]; the tutorial menu's triggers are its A and S, not
/// Page Up and Page Down; the container menu's X is its R).
pub(crate) fn menu_key_at(button: GamepadButton, at: MenuAt) -> Option<KeyCode> {
    menu_buttons(at).iter().find(|(b, _)| *b == button).map(|(_, k)| *k).or_else(|| menu_key(button))
}

/// Which names the hints and prompts give the controls: the keyboard's keys, or the
/// controller's buttons while one works the controls (`crate::bindings::InputDevice`).
/// One setting for every menu, as a language is, so the menus' drawing code reads it
/// without each taking the bindings [design, #1662].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Labels {
    #[default]
    Keys,
    Xbox,
    /// A PlayStation controller's names (Cross, Circle, Square, Triangle, L1 … R3, Select,
    /// Start) [design, #1452].
    PlayStation,
}

impl Labels {
    fn code(self) -> u8 {
        self as u8
    }

    fn of(code: u8) -> Labels {
        match code {
            1 => Labels::Xbox,
            2 => Labels::PlayStation,
            _ => Labels::Keys,
        }
    }
}

#[cfg(not(test))]
static LABELS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

#[cfg(test)]
thread_local! {
    /// Under test each test thread has its own: tests run side by side, and a test's
    /// app may set the controller while another reads a keyboard hint. The systems of a
    /// test's app never write it ([`sync_labels`]).
    static LABELS: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// The names hints use now ([`Labels`]).
pub fn labels() -> Labels {
    #[cfg(not(test))]
    {
        Labels::of(LABELS.load(std::sync::atomic::Ordering::Relaxed))
    }
    #[cfg(test)]
    {
        Labels::of(LABELS.with(|c| c.get()))
    }
}

/// Set the names hints use (by [`sync_labels`] each frame; by a test on its own thread).
pub fn set_labels(l: Labels) {
    #[cfg(not(test))]
    LABELS.store(l.code(), std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    LABELS.with(|c| c.set(l.code()));
}

/// The labels follow the Input Device setting and, with a controller, the Button style
/// ([`ButtonStyle`]; Auto: the controller in use, [`PadInUse`]).
fn sync_labels(bindings: Option<Res<crate::bindings::Bindings>>, style: Option<Res<ButtonStyle>>, in_use: Option<Res<PadInUse>>) {
    let controller = bindings.is_some_and(|b| b.device == crate::bindings::InputDevice::Controller);
    let l = if controller { style.map_or(ButtonStyle::Auto, |s| *s).labels(in_use.is_some_and(|u| u.playstation)) } else { Labels::Keys };
    if cfg!(not(test)) && labels() != l {
        info!("hints: {l:?} labels");
        set_labels(l);
    }
}

/// The controller button that presses `key` in the menu `at` ([`menu_keys`]): the menu's
/// own buttons first, then the shared ones (B before Start for Esc).
pub fn button_for(key: KeyCode, at: MenuAt) -> Option<GamepadButton> {
    let own = menu_buttons(at);
    own.iter()
        .chain(IN_MENUS.iter().chain(ALWAYS.iter()).filter(|(b, _)| !own.iter().any(|(o, _)| o == b)))
        .find(|(_, k)| *k == key)
        .map(|(b, _)| *b)
}

/// A key's name in a hint: the Controls page's name, short for the few long ones the
/// hints use ("Esc", "Del").
pub fn key_hint_name(key: KeyCode) -> String {
    match key {
        KeyCode::Escape => "Esc".into(),
        KeyCode::Delete => "Del".into(),
        k => crate::bindings::key_name(k).map_or_else(|| format!("{k:?}"), String::from),
    }
}

/// The name a hint gives an action of the menu `at` that the keys `keys` work (the first
/// is the one a keyboard hint names): with [`Labels::Keys`] the first key's name; with a
/// controller the name of the button that presses the first of them that has one there
/// ([`button_for`]), the key's name when none does (a gap the tests look for).
pub fn hint_name_as(keys: &[KeyCode], at: MenuAt, l: Labels) -> String {
    let key = || keys.first().map_or_else(|| "?".into(), |k| key_hint_name(*k));
    match l {
        Labels::Keys => key(),
        Labels::Xbox | Labels::PlayStation => keys.iter().find_map(|k| button_for(*k, at)).map_or_else(key, |b| pad_label(PadInput::Button(b), l).to_string()),
    }
}

/// [`hint_name_as`] with the labels in use.
pub fn hint_name(keys: &[KeyCode], at: MenuAt) -> String {
    hint_name_as(keys, at, labels())
}

/// The hint after a button's words, as the original writes it: the name and ")" ("R)",
/// with a controller "Y)") [observed: "Take All R)", "Exit E)"].
pub fn hint(keys: &[KeyCode], at: MenuAt) -> String {
    hint_as(keys, at, labels())
}

/// [`hint`] with the labels `l` (a view that keeps the labels it was made with, so a
/// change of device draws it again).
pub fn hint_as(keys: &[KeyCode], at: MenuAt, l: Labels) -> String {
    format!("{})", hint_name_as(keys, at, l))
}

/// [`hint`] in brackets, the help box's and the tiled menus' `_PCButtonText` shape
/// ("[E]", with a controller "[B]") [observed: "Close [E]"].
pub fn hint_bracketed(keys: &[KeyCode], at: MenuAt) -> String {
    format!("[{}]", hint_name(keys, at))
}

/// A help box over any menu: no own buttons, so B (Esc) closes it.
pub const HELP_AT: MenuAt = MenuAt { code: None, tab: HELP_PAGE };

/// A hint the game's strings give for the keyboard (`sPCMenuHintE`, "[E]"; a
/// `_PCButtonText`): as it is on a keyboard, and with a controller the button's name in
/// the same shape ("[B]", or "B)") [design, #1662: the gamepad strings are the
/// original's button pictures, which are not drawn].
pub fn game_hint(game: &str, keys: &[KeyCode], at: MenuAt) -> String {
    match labels() {
        Labels::Keys => game.to_string(),
        _ if game.trim_start().starts_with('[') => hint_bracketed(keys, at),
        _ => hint(keys, at),
    }
}

/// The menu `code` (tab or page 0) for a hint.
pub fn at_menu(code: u16) -> MenuAt {
    MenuAt { code: Some(code), tab: 0 }
}

/// Whether `button` went down this frame on a controller that works the controls: for a
/// menu's own controller buttons beyond the keys [`menu_keys`] stands in for (the ITEMS
/// page's X drop, `crate::pipboy::items`).
pub(crate) fn menu_button(pads: &Query<&Gamepad>, bindings: Option<&crate::bindings::Bindings>, button: GamepadButton) -> bool {
    bindings.is_some_and(|b| b.device == crate::bindings::InputDevice::Controller) && pads.iter().any(|g| g.just_pressed(button))
}

/// A controller's arrival and departure, in the log.
fn log_pads(added: Query<(Entity, Option<&Name>), Added<Gamepad>>, mut gone: RemovedComponents<Gamepad>) {
    for (e, name) in &added {
        info!("controller: {} connected ({e})", name.map_or("a controller", |n| n.as_str()));
    }
    for e in gone.read() {
        info!("controller: {e} disconnected");
    }
}

pub struct PadPlugin;

impl Plugin for PadPlugin {
    fn build(&self, app: &mut App) {
        use bevy::input::InputSystems;
        app.init_resource::<StickButtons>()
            .init_resource::<MenuKeys>()
            .init_resource::<ButtonStyle>()
            .init_resource::<PadInUse>()
            .add_systems(PreUpdate, (update_stick_buttons, menu_keys, track_pad, sync_labels.after(track_pad)).after(InputSystems))
            .add_systems(Update, log_pads);
        crate::settings::add_choice_list(
            app,
            crate::settings::Setting {
                key: "button-style",
                group: crate::aim_assist::GROUP,
                label: "Button style",
                note: "Which names and pictures the controller's buttons go by in prompts, menu hints and the Controls tab. Auto: a PlayStation controller's (Cross, Circle, Square, Triangle, L1, R2, Select…) when the controller in use says it is one, else an Xbox controller's. Xbox or PlayStation: always those. The INI keeps the Xbox names either way.",
                get: |_| false,
                set: |_, _| {},
                slider: None,
            },
            crate::settings::ChoiceList {
                names: |_| ButtonStyle::ALL.iter().map(|s| s.name().to_string()).collect(),
                chosen: |w| w.resource::<ButtonStyle>().name().to_string(),
                pick: style_pick,
                save: |w| w.resource::<ButtonStyle>().name().to_string(),
                load: style_pick,
                ask_keep: false,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_own_buttons_follow_the_files() {
        use fnv_game::game::menus;
        use GamepadButton::*;
        let at = |code, tab| MenuAt { code: Some(code), tab };
        // The ITEMS page: Repair Y, Mod LB (the keyboard's R and X); X is the drop button.
        assert!(menu_buttons(at(menus::INVENTORY, 0)).starts_with(&[(North, KeyCode::KeyR), (LeftTrigger, KeyCode::KeyX)]));
        assert!(menu_buttons(at(menus::INVENTORY, 0)).iter().all(|(b, _)| *b != West));
        assert!(menu_buttons(at(crate::script::MENU_CONTAINER, 0)).contains(&(West, KeyCode::KeyR)));
        assert!(menu_buttons(at(menus::BARTER, 0)).contains(&(West, KeyCode::KeyA)));
        assert!(menu_buttons(at(menus::STATS, 4)).contains(&(West, KeyCode::KeyR)));
        // The triggers step the Pip-Boy's pages, round from DATA to STATS.
        for (code, lt, rt) in [(menus::STATS, KeyCode::F3, KeyCode::F2), (menus::INVENTORY, KeyCode::F1, KeyCode::F3), (menus::MAP, KeyCode::F2, KeyCode::F1)] {
            for tab in 0..5 {
                let t = menu_buttons(at(code, tab));
                assert!(t.contains(&(LeftTrigger2, lt)) && t.contains(&(RightTrigger2, rt)), "menu {code} tab {tab}");
            }
        }
        assert!(menu_buttons(MenuAt::default()).is_empty());
        // No menu gives one button two keys.
        for code in (1000..1200).chain([POKER_MENU, PAUSE_MENU]) {
            for tab in (0..5).chain([STATS_RAD]) {
                let t = menu_buttons(at(code, tab));
                for (i, (b, _)) in t.iter().enumerate() {
                    assert!(t[i + 1..].iter().all(|(o, _)| o != b), "menu {code} tab {tab}: {b:?} twice");
                }
            }
        }
    }

    #[test]
    fn hints_name_the_key_or_the_button_that_presses_it() {
        use fnv_game::game::menus;
        use KeyCode::*;
        let caravan = |tab| MenuAt { code: Some(crate::caravan::MENU_CARAVAN), tab };
        // (where, keys, keyboard, controller): the hints #1662 names and the rest of the sweep.
        let cases: &[(MenuAt, &[KeyCode], &str, &str)] = &[
            (caravan(CARAVAN_BET), &[KeyW], "W", "Y"),
            (caravan(CARAVAN_BET), &[KeyA], "A", "RT"),
            (caravan(CARAVAN_BET), &[KeyF], "F", "A"),
            (caravan(CARAVAN_BET), &[KeyR, Escape], "R", "B"),
            (caravan(CARAVAN_DECK), &[KeyW], "W", "A"),
            (caravan(CARAVAN_DECK), &[KeyA], "A", "X"),
            (caravan(CARAVAN_DECK), &[KeyS], "S", "Y"),
            (caravan(CARAVAN_DECK), &[KeyF], "F", "RT"),
            (caravan(CARAVAN_PLAY), &[KeyW], "W", "A"),
            (caravan(CARAVAN_PLAY), &[KeyQ], "Q", "X"),
            (caravan(CARAVAN_PLAY), &[KeyE], "E", "Y"),
            (caravan(CARAVAN_PLAY), &[KeyR], "R", "B"),
            (HELP_AT, &[KeyE, Escape], "E", "B"),
            (MenuAt { code: Some(menus::MAP), tab: 3 }, &[KeyR], "R", "Y"),
            (at_menu(menus::INVENTORY), &[KeyX], "X", "LB"),
            (at_menu(menus::INVENTORY), &[KeyR], "R", "Y"),
            (at_menu(menus::INVENTORY), &[KeyE, Escape], "E", "B"),
            (at_menu(menus::INVENTORY), &[Enter], "Enter", "A"),
            (at_menu(menus::STATS), &[KeyS], "S", "A"),
            (at_menu(menus::STATS), &[KeyE], "E", "Y"),
            (MenuAt { code: Some(menus::STATS), tab: STATS_RAD }, &[KeyA], "A", "A"),
            (MenuAt { code: Some(menus::STATS), tab: STATS_RAD }, &[KeyX], "X", "X"),
            (MenuAt { code: Some(menus::STATS), tab: 4 }, &[KeyR], "R", "X"),
            (at_menu(menus::BARTER), &[KeyA], "A", "X"),
            (at_menu(menus::BARTER), &[KeyX, Escape], "X", "B"),
            (at_menu(crate::script::MENU_CONTAINER), &[KeyR], "R", "X"),
            (at_menu(crate::script::MENU_CONTAINER), &[KeyE, Escape], "E", "B"),
            (MenuAt::default(), &[KeyA, Enter], "A", "A"),
            (at_menu(fnv_game::crafting::RECIPE_MENU), &[KeyA], "A", "X"),
            (at_menu(fnv_game::crafting::RECIPE_MENU), &[KeyX, Escape], "X", "B"),
            (at_menu(fnv_game::repair::REPAIR_SERVICES_MENU), &[KeyA], "A", "X"),
            (at_menu(menus::LEVEL_UP), &[KeyR], "R", "Y"),
            (at_menu(menus::LEVEL_UP), &[KeyA], "A", "X"),
            (at_menu(menus::LEVEL_UP), &[KeyE], "E", "B"),
            (at_menu(menus::TAG_SKILLS), &[KeyA], "A", "X"),
            (at_menu(menus::TRAITS), &[KeyR], "R", "Y"),
            (at_menu(menus::LOCKPICK), &[KeyF], "F", "X"),
            (at_menu(menus::LOCKPICK), &[KeyE, Escape], "E", "B"),
            (at_menu(menus::DIALOGUE), &[Tab], "Tab", "B"),
            (at_menu(crate::script::MENU_SLEEP_WAIT), &[KeyE, Enter], "E", "A"),
            (at_menu(crate::script::MENU_SLEEP_WAIT), &[KeyX, Escape], "X", "B"),
            (at_menu(fnv_game::blackjack::MENU_CODE), &[KeyF], "F", "A"),
            (at_menu(fnv_game::blackjack::MENU_CODE), &[KeyQ], "Q", "LT"),
            (at_menu(fnv_game::roulette::MENU_CODE), &[KeyS], "S", "X"),
            (at_menu(fnv_game::slots::MENU_CODE), &[KeyE], "E", "RT"),
            (MenuAt { code: Some(POKER_MENU), tab: 0 }, &[KeyE], "E", "A"),
            (MenuAt { code: Some(POKER_MENU), tab: 0 }, &[KeyF], "F", "X"),
            (MenuAt { code: Some(POKER_MENU), tab: POKER_BETWEEN_HANDS }, &[Space], "Space", "A"),
            (MenuAt { code: Some(POKER_MENU), tab: 0 }, &[Tab, Escape], "Tab", "B"),
            (PAUSE_AT, &[Delete], "Del", "X"),
            (PAUSE_AT, &[KeyY, Enter], "Y", "A"),
            (PAUSE_AT, &[KeyN, Escape], "N", "B"),
        ];
        for (at, keys, kb, pad) in cases {
            assert_eq!(hint_name_as(keys, *at, Labels::Keys), *kb, "{at:?} {keys:?} keyboard");
            assert_eq!(hint_name_as(keys, *at, Labels::Xbox), *pad, "{at:?} {keys:?} controller");
            // The button found presses one of the keys there, as menu_keys has it.
            let b = keys.iter().find_map(|k| button_for(*k, *at)).expect("a button");
            assert!(keys.contains(&menu_key_at(b, *at).unwrap()), "{at:?} {b:?}");
        }
        // No button presses a key: the key's name stays (the test's gap finder).
        assert_eq!(hint_name_as(&[KeyZ], at_menu(menus::BARTER), Labels::Xbox), "Z");
    }

    #[test]
    fn hints_follow_the_labels_in_use() {
        let at = at_menu(fnv_game::game::menus::MAP);
        assert_eq!(labels(), Labels::Keys);
        assert_eq!(hint(&[KeyCode::KeyR], at), "R)");
        assert_eq!(hint_bracketed(&[KeyCode::KeyE, KeyCode::Escape], HELP_AT), "[E]");
        assert_eq!(game_hint("[E]", &[KeyCode::KeyE, KeyCode::Escape], HELP_AT), "[E]");
        set_labels(Labels::Xbox);
        assert_eq!(hint(&[KeyCode::KeyR], at), "Y)");
        assert_eq!(game_hint("[E]", &[KeyCode::KeyE, KeyCode::Escape], HELP_AT), "[B]");
        assert_eq!(game_hint("E)", &[KeyCode::KeyE, KeyCode::Escape], HELP_AT), "B)");
        assert_eq!(crate::dialogue_menu::goodbye_hint(), "[B] Goodbye");
        set_labels(Labels::Keys);
        assert_eq!(crate::dialogue_menu::goodbye_hint(), crate::dialogue_menu::GOODBYE_HINT);
    }

    #[test]
    fn playstation_pads_are_told_by_name() {
        for n in ["DualSense Wireless Controller", "DUALSHOCK 4 Wireless Controller", "Wireless Controller", "PS3 Controller", "Sony PLAYSTATION(R)3 Controller", "PS4 Controller", "PS5 Controller"] {
            assert!(playstation_pad(n), "{n}");
        }
        for n in ["Xbox Wireless Controller", "Xbox 360 Controller", "XInput Controller #1", "Logitech Dual Action", "8BitDo Pro 2", "Controller (Xbox One For Windows)"] {
            assert!(!playstation_pad(n), "{n}");
        }
    }

    #[test]
    fn the_button_style_picks_the_names() {
        assert_eq!(ButtonStyle::Auto.labels(true), Labels::PlayStation);
        assert_eq!(ButtonStyle::Auto.labels(false), Labels::Xbox);
        assert_eq!(ButtonStyle::Xbox.labels(true), Labels::Xbox);
        assert_eq!(ButtonStyle::PlayStation.labels(false), Labels::PlayStation);
        assert_eq!(ButtonStyle::by_name(" playstation "), Some(ButtonStyle::PlayStation));
        use GamepadButton::*;
        let ps = |b| pad_label(PadInput::Button(b), Labels::PlayStation);
        assert_eq!([ps(South), ps(East), ps(West), ps(North)], ["Cross", "Circle", "Square", "Triangle"]);
        assert_eq!([ps(LeftTrigger), ps(RightTrigger), ps(LeftTrigger2), ps(RightTrigger2), ps(LeftThumb), ps(RightThumb)], ["L1", "R1", "L2", "R2", "L3", "R3"]);
        assert_eq!([ps(Select), ps(Start), ps(DPadUp)], ["Select", "Start", "D-Pad Up"]);
        assert_eq!(pad_label(PadInput::Stick(StickDir::Up), Labels::PlayStation), "Left Stick Up");
        // The INI's names stay the Xbox ones, so every INI keeps loading.
        for (p, n) in PAD_NAMES {
            assert_eq!(pad_name(*p), *n);
            assert_eq!(parse_pad(n), Some(*p));
        }
        // Hints with each style.
        let at = MenuAt { code: Some(fnv_game::game::menus::MAP), tab: 3 };
        assert_eq!(hint_as(&[KeyCode::KeyR], at, Labels::Keys), "R)");
        assert_eq!(hint_as(&[KeyCode::KeyR], at, Labels::Xbox), "Y)");
        assert_eq!(hint_as(&[KeyCode::KeyR], at, Labels::PlayStation), "Triangle)");
        let bet = MenuAt { code: Some(crate::caravan::MENU_CARAVAN), tab: CARAVAN_BET };
        assert_eq!(hint_as(&[KeyCode::KeyA], bet, Labels::PlayStation), "R2)");
        assert_eq!(hint_name_as(&[KeyCode::KeyE, KeyCode::Escape], HELP_AT, Labels::PlayStation), "Circle");
        set_labels(Labels::PlayStation);
        assert_eq!(crate::dialogue_menu::goodbye_hint(), "[Circle] Goodbye");
        assert_eq!(pad_shown(PadInput::Button(South)), "Cross");
        set_labels(Labels::Keys);
        assert_eq!(pad_shown(PadInput::Button(South)), "A");
    }

    #[test]
    fn the_glyphs_draw_solid_and_ring_shapes_with_a_glow() {
        let n = 64;
        for g in Glyph::ALL {
            let img = g.image(n);
            assert_eq!(img.data.as_ref().map(|d| d.len()), Some((n * n * 4) as usize));
            // Something drawn, the corners clear.
            assert!((0..n).any(|x| g.coverage(x, n / 2, n) > 0.9 || g.coverage(n / 2, x, n) > 0.9), "{g:?}");
            assert_eq!(g.coverage(0, 0, n), 0.0, "{g:?}");
            assert_eq!(Glyph::named(g.name()), Some(g));
        }
        // The cross is solid through its middle, not a thin outline; the circle, square and
        // triangle are rings: a clear opening at the middle (the triangle's hole is small
        // enough that the glow reaches faintly into it, but it is not solid).
        assert!(Glyph::Cross.coverage(n / 2, n / 2, n) > 0.9);
        assert!(Glyph::Cross.coverage(n / 2 + 4, n / 2, n) > 0.5, "the cross is solid, not an outline");
        assert!(Glyph::Circle.coverage(n / 2, n / 2, n) < 0.05);
        assert!(Glyph::Square.coverage(n / 2, n / 2, n) < 0.05);
        assert!(Glyph::Triangle.coverage(n / 2, n / 2, n) < 0.4, "the triangle's small hole may glow faintly");
        // A soft glow just past the circle's ring (the falloff the fonts bake in).
        assert!(Glyph::Circle.coverage(n / 2 + 24, n / 2, n) > 0.1, "the ring has a glow");
        assert_eq!(Glyph::of(GamepadButton::North), Some(Glyph::Triangle));
        assert_eq!(Glyph::of(GamepadButton::Start), None);
    }

    #[test]
    fn a_help_box_rests_the_menus_own_buttons() {
        for code in [fnv_game::blackjack::MENU_CODE, crate::caravan::MENU_CARAVAN, fnv_game::game::menus::LOCKPICK] {
            let at = MenuAt { code: Some(code), tab: HELP_PAGE };
            assert!(menu_buttons(at).is_empty());
            assert_eq!(menu_key_at(GamepadButton::South, at), Some(KeyCode::Enter));
            assert_eq!(menu_key_at(GamepadButton::East, at), Some(KeyCode::Escape));
        }
    }

    #[test]
    fn dialogue_owns_b_as_tab_not_escape() {
        use fnv_game::game::menus;
        use GamepadButton::East;
        let at = MenuAt { code: Some(menus::DIALOGUE), tab: 0 };
        // B (East) is the conversation's own leave key (Tab/Goodbye), not the shared Esc
        // that opens the pause menu (#1657).
        assert!(menu_buttons(at).contains(&(East, KeyCode::Tab)));
        assert_eq!(menu_key_at(East, at), Some(KeyCode::Tab));
        // Outside a conversation East is still the shared Esc.
        assert_eq!(menu_key(East), Some(KeyCode::Escape));
    }

    #[test]
    fn the_games_codes_give_the_console_layout() {
        use GamepadButton::*;
        assert_eq!(game_pad(0x0A), Some(PadInput::Button(South)));
        assert_eq!(game_pad(0x11), Some(PadInput::Button(RightTrigger2)));
        assert_eq!(game_pad(0x0E), Some(PadInput::Button(RightTrigger)));
        assert_eq!(game_pad(0x13), Some(PadInput::Stick(StickDir::Up)));
        assert_eq!(game_pad(0xFF), None);
        assert_eq!(game_pad(0x01), None);
    }

    #[test]
    fn names_read_back() {
        for (p, n) in PAD_NAMES {
            assert_eq!(parse_pad(n), Some(*p));
            assert_eq!(pad_name(*p), *n);
        }
        assert_eq!(parse_pad(" left stick up "), Some(PadInput::Stick(StickDir::Up)));
        assert_eq!(parse_pad("Z"), None);
    }

    #[test]
    fn a_stick_pushed_one_way() {
        let v = Vec2::new(0.3, -0.8);
        assert_eq!(StickDir::Down.amount(v), 0.8);
        assert_eq!(StickDir::Up.amount(v), 0.0);
        assert!((StickDir::Right.amount(v) - 0.3).abs() < 1e-6);
    }
}
