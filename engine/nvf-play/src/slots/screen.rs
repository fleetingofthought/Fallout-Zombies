//! The slot machine menu: its text and buttons as `menus\slot_machine_menu.xml` sets them
//! out [documented], and the reels drawn flat from the casino record's symbol pictures
//! (slots.md §6) [design]. Lengths are the interface's units (960 to the view's height);
//! the main rectangle is 30 in from the view's edges, as lockpicking's
//! (`crate::lockpick::screen`).
//!
//! - The information box: 400 × 175, 10 in from the main rectangle's left and bottom, the
//!   bracket of lines, three rows in font 7 at 20 in, at 35, 80 and 125 down: "Current
//!   Bet:", "Chips:", "<casino> Earnings:", each value right-aligned 80 short of the box's
//!   width [documented: `SMM_InfoRect`; hypothesis: the titles, set in code].
//! - Six buttons right-aligned 50 from the main rectangle's right, under each other: Spin,
//!   Increase Bet, Decrease Bet, Payout List, Bet Max, Exit, each with its key
//!   [documented: the order, `_PCButton_W/E/Q/F/S/R`; hypothesis: the words]; placed as
//!   roulette's (roulette.md §5) [hypothesis]. Dimmed when they cannot be used.
//! - The status text right-aligned at the buttons' x, 10 down [documented: `SMM_StatusText`].
//! - The three reels side by side in the middle, each a window three symbols high, the
//!   payline across the middle row; the payout list in their place when asked for.
//! - The help box over all of it the first time (`crate::lockpick::help`).

use super::{Phase, Session, SlotsInput, SlotsScreen};
use crate::interface::{amber, at, u};
use crate::lockpick::screen::{Ctx, FontKind, Kit};
use crate::pause::AidTime;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use crate::ui_pointer::Button;
use bevy::window::PrimaryWindow;
use fnv_game::slots::{self as rules, REELS};
use nvf_assets::GameData;

const EDGE: f32 = 30.0;
const INFO: Vec2 = Vec2::new(400.0, 175.0);
const INFO_IN: f32 = 10.0;
const ROW_X: f32 = 20.0;
const ROWS_Y: [f32; 3] = [35.0, 80.0, 125.0];
const VALUE_X: f32 = ROW_X + INFO.x - 80.0;
const THICK: f32 = 2.0;
const FADE: f32 = 30.0;
const BUTTONS_RIGHT: f32 = 50.0;
const BUTTONS_TOP: f32 = 549.0;
const BUTTON_H: f32 = 52.0;
const HINT_GAP: f32 = 16.0;
const STATUS_TOP: f32 = 10.0;
const DIM: f32 = 110.0;
const UNDER_HELP: f32 = 0.47;
/// A symbol on a reel (the pictures are 256 × 128), the gap between reels and the reels'
/// top [design].
const SYMBOL: Vec2 = Vec2::new(240.0, 120.0);
const REEL_GAP: f32 = 30.0;
const REELS_TOP: f32 = 170.0;
/// The reels' ground and the rows off the line, darker [design].
const GROUND: (f32, f32, f32) = (0.86, 0.84, 0.80);
const OFF_LINE: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
/// The payout list: a row's height, a symbol in it [design].
const PAY_ROW: f32 = 50.0;
const PAY_SYMBOL: Vec2 = Vec2::new(84.0, 42.0);

#[derive(Component)]
pub struct SlotsRoot;

/// A button and what it does.
#[derive(Component, Clone)]
pub struct SlotsButton(pub SlotsInput);

fn view_key(s: &Session, now: f32, view_w: f32) -> String {
    // While the reels turn, a new picture every sixth of a stop.
    let reels = s.positions(now).map(|p| (p * 6.0).round() as i64);
    format!(
        "{}:{}:{}:{:?}:{:?}:{:?}:{}:{}:{view_w:.0}:{:?}",
        s.bet,
        s.chips,
        s.earnings,
        s.status,
        std::mem::discriminant(&s.phase),
        reels,
        s.help.is_some(),
        s.payouts,
        crate::pad::labels(),
    )
}

fn view_width(window: Option<&Window>) -> f32 {
    window.map_or(960.0 * 16.0 / 9.0, |w| 960.0 * w.width() / w.height().max(1.0))
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    screen: Res<SlotsScreen>,
    data: Option<Res<GameData>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    glyphs: Res<crate::pad::PadGlyphs>,
    roots: Query<Entity, With<SlotsRoot>>,
    time: AidTime,
    (mut kit, mut shown): (Local<Option<Kit>>, Local<Option<String>>),
) {
    let view_w = view_width(window.single().ok());
    let now = time.elapsed_secs();
    let key = screen.0.as_ref().map(|s| view_key(s, now, view_w));
    if key == *shown {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    *shown = key;
    let Some(s) = screen.0.as_ref() else { return };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let kit = kit.get_or_insert_with(|| Kit::load(data.as_deref(), &mut images, bc));
    let mut c = Ctx { kit, data: data.as_deref(), images: &mut images, bc, glyphs: &glyphs };
    spawn(&mut commands, &mut c, s, now, view_w);
}

fn picture(s: &Session, symbol: u8) -> &str {
    s.symbols.get(symbol as usize).map_or("", |p| p.as_str())
}

fn spawn(commands: &mut Commands, c: &mut Ctx, s: &Session, now: f32, view_w: f32) {
    let bright = amber(255.0);
    let dim = amber(DIM);
    let light = if s.help.is_some() { UNDER_HELP } else { 1.0 };
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    // The menu at the UI scale, in the middle of the view (`crate::interface::menu_frame`, #1631).
    commands.spawn((SlotsRoot, full.clone(), GlobalZIndex(106), BackgroundColor(Color::BLACK))).with_children(|r| crate::interface::in_menu_frame(r, crate::interface::MIDDLE, |r| {
        // The payout list takes the reels' place while it shows.
        if s.payouts {
            payouts(r, c, s, view_w);
        } else {
            reels(r, c, s, now, view_w, light);
        }
        let main = Node { position_type: PositionType::Absolute, left: u(EDGE), right: u(EDGE), top: u(EDGE), bottom: u(EDGE), ..default() };
        r.spawn(main).with_children(|m| {
            let info = Node { position_type: PositionType::Absolute, left: u(INFO_IN), bottom: u(INFO_IN), width: u(INFO.x), height: u(INFO.y), ..default() };
            m.spawn(info).with_children(|b| {
                c.image(b, "interface\\shared\\line\\fade_to_top.dds", at(0.0, 0.0, THICK, FADE), bright);
                c.image(b, "interface\\shared\\line\\solid.dds", at(0.0, FADE - THICK, THICK, INFO.y - FADE), bright);
                c.image(b, "interface\\shared\\line\\solid.dds", at(0.0, INFO.y - THICK, INFO.x - FADE, THICK), bright);
                c.image(b, "interface\\shared\\line\\fade_to_right.dds", at(INFO.x - FADE - THICK, INFO.y - THICK, FADE, THICK), bright);
                let rows = [("Current Bet:".to_string(), s.bet.to_string()), ("Chips:".into(), s.chips.to_string()), (format!("{} Earnings:", s.casino_name), s.earnings.to_string())];
                for ((title, value), y) in rows.iter().zip(ROWS_Y) {
                    c.text_in(b, FontKind::Menu, title, ROW_X, y, false, bright);
                    c.text_in(b, FontKind::Menu, value, VALUE_X, y, true, bright);
                }
            });
            let betting = s.betting();
            let top = s.limits.top(s.chips);
            let h = |k: KeyCode| crate::pad::hint(&[k], crate::pad::at_menu(fnv_game::slots::MENU_CODE));
            let buttons = [
                ("Spin", h(KeyCode::KeyW), SlotsInput::Spin, betting && s.chips >= s.bet && s.chips > 0),
                ("Increase Bet", h(KeyCode::KeyE), SlotsInput::Increase, betting && s.bet < top),
                ("Decrease Bet", h(KeyCode::KeyQ), SlotsInput::Decrease, betting && s.bet > s.limits.min_bet),
                ("Payout List", h(KeyCode::KeyF), SlotsInput::Payouts, betting),
                ("Bet Max", h(KeyCode::KeyS), SlotsInput::Max, betting && s.bet < top),
                ("Exit", h(KeyCode::KeyR), SlotsInput::Exit, betting),
            ];
            let column = Node { position_type: PositionType::Absolute, right: u(BUTTONS_RIGHT), top: u(BUTTONS_TOP), width: u(0.0), height: u(6.0 * BUTTON_H), ..default() };
            m.spawn(column).with_children(|b| {
                for (i, (label, hint, cmd, on)) in buttons.into_iter().enumerate() {
                    let color = if on { bright } else { dim };
                    let y = i as f32 * BUTTON_H;
                    let w = c.text_in(b, FontKind::Menu, label, 0.0, y + 9.0, true, color);
                    c.text_in(b, FontKind::Menu, &hint, HINT_GAP, y + 9.0, false, color);
                    if on {
                        b.spawn((Button, SlotsButton(cmd), at(-w - 10.0, y, w + 20.0 + HINT_GAP + 30.0, BUTTON_H)));
                    }
                }
            });
            if let Some(line) = &s.status {
                let status = Node { position_type: PositionType::Absolute, right: u(BUTTONS_RIGHT), top: u(STATUS_TOP), width: u(0.0), height: u(40.0), ..default() };
                m.spawn(status).with_children(|b| {
                    c.text_in(b, FontKind::Menu, line, 0.0, 0.0, true, bright);
                });
            }
        });
        if let Some(h) = &s.help {
            crate::lockpick::help::spawn(r, c, h, view_w);
        }
    }));
}

/// The three reels: each a window three symbols high, the symbols running down while it
/// turns; the line's row framed, the others darker.
fn reels(r: &mut ChildSpawnerCommands, c: &mut Ctx, s: &Session, now: f32, view_w: f32, light: f32) {
    let width = REELS as f32 * SYMBOL.x + (REELS - 1) as f32 * REEL_GAP;
    let left = (view_w - width) / 2.0;
    let n = s.reel.len().max(1) as i64;
    let tint = Color::srgb(light, light, light);
    let ground = Color::srgb(GROUND.0 * light, GROUND.1 * light, GROUND.2 * light);
    c.text_in(r, FontKind::Menu, &s.casino_name, left, REELS_TOP - 60.0, false, amber(255.0 * light));
    for (i, pos) in s.positions(now).into_iter().enumerate() {
        let x = left + i as f32 * (SYMBOL.x + REEL_GAP);
        let window = Node { overflow: Overflow::clip(), ..at(x, REELS_TOP, SYMBOL.x, 3.0 * SYMBOL.y) };
        r.spawn((window, BackgroundColor(ground))).with_children(|w| {
            let p0 = pos.floor();
            let f = pos - p0;
            for k in -2i64..=2 {
                let stop = (p0 as i64 + k).rem_euclid(n) as usize;
                let symbol = s.reel.get(stop).copied().unwrap_or(0);
                let top = SYMBOL.y - (k as f32 - f) * SYMBOL.y;
                c.image(w, picture(s, symbol), at(0.0, top, SYMBOL.x, SYMBOL.y), tint);
            }
            w.spawn((at(0.0, 0.0, SYMBOL.x, SYMBOL.y), BackgroundColor(OFF_LINE)));
            w.spawn((at(0.0, 2.0 * SYMBOL.y, SYMBOL.x, SYMBOL.y), BackgroundColor(OFF_LINE)));
        });
    }
    // The payline's frame.
    let b = amber(255.0 * light);
    for y in [REELS_TOP + SYMBOL.y, REELS_TOP + 2.0 * SYMBOL.y - THICK] {
        c.image(r, "interface\\shared\\line\\solid.dds", at(left - 20.0, y, width + 40.0, THICK), b);
    }
    // The bet under the reels while they stand.
    if s.phase == Phase::Betting {
        let line = format!("Bet: {}", s.bet);
        c.text_in(r, FontKind::Menu, &line, left, REELS_TOP + 3.0 * SYMBOL.y + 20.0, false, b);
    }
}

/// The payout list over the reels, as the machine's payout screen sets it out: each row's
/// symbols in their places, "= BET x" and the number (slots.md §3).
fn payouts(r: &mut ChildSpawnerCommands, c: &mut Ctx, s: &Session, view_w: f32) {
    let rows = rules::payout_rows();
    let w = 660.0;
    let h = rows.len() as f32 * PAY_ROW + 40.0;
    let left = (view_w - w) / 2.0;
    let top = REELS_TOP - 20.0;
    let b = amber(255.0);
    r.spawn((at(left, top, w, h), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.9)))).with_children(|p| {
        c.image(p, "interface\\shared\\line\\solid.dds", at(0.0, 0.0, w, THICK), b);
        c.image(p, "interface\\shared\\line\\solid.dds", at(0.0, h - THICK, w, THICK), b);
        for (i, (symbols, pay)) in rows.iter().enumerate() {
            let y = 20.0 + i as f32 * PAY_ROW;
            for (j, &sym) in symbols.iter().enumerate() {
                c.image(p, picture(s, sym), at(30.0 + j as f32 * (PAY_SYMBOL.x + 6.0), y + (PAY_ROW - PAY_SYMBOL.y) / 2.0, PAY_SYMBOL.x, PAY_SYMBOL.y), Color::WHITE);
            }
            c.text_in(p, FontKind::Menu, &format!("= BET x {pay}"), 340.0, y + 6.0, false, b);
        }
    });
}
