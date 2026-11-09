//! The blackjack menu's text and buttons as `menus\black_jack_menu.xml` sets them out
//! [documented, unless tagged], over the table's picture ([`super::BlackjackView`], drawn
//! by the engine). Lengths are the interface's units (960 to the view's height); the
//! fonts, lines and help box are the lockpicking menu's (`crate::lockpick::screen`), whose
//! file lays out its information box the same way.
//!
//! - The information box: 400 × 175, 10 in from the main rectangle's left and bottom, the
//!   bracket of lines, three rows in font 7 at 20 in and 35, 80 and 125 down: "Current
//!   Bet:", "Chips:" and "<casino> Earnings:" [observed: the owner's screenshot], each
//!   value right-aligned 80 short of the box's width.
//! - Six buttons right-aligned 50 from the main rectangle's right, one under the other,
//!   each its label and its key (blackjack.md §5); a button that does not apply now is not
//!   drawn, the others keep their places [hypothesis].
//! - The status line: font 7, right-aligned with the buttons, 10 down the main rectangle.
//! - All in the HUD's colour.

use super::{BlackjackInput, BlackjackScreen, BlackjackView, Session};
use crate::interface::{amber, at, u};
use crate::lockpick::screen::{Ctx, FontKind, Kit};
use crate::pause::AidTime;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use crate::ui_pointer::Button;
use bevy::window::PrimaryWindow;
use nvf_assets::GameData;

const EDGE: f32 = 30.0;
const INFO: Vec2 = Vec2::new(400.0, 175.0);
const INFO_IN: f32 = 10.0;
const ROW_X: f32 = 20.0;
const ROWS_Y: [f32; 3] = [35.0, 80.0, 125.0];
const VALUE_X: f32 = INFO.x - 80.0 + ROW_X;
const THICK: f32 = 2.0;
const FADE: f32 = 30.0;
const BUTTONS_RIGHT: f32 = 50.0;
/// A button is its text and 9 above and below [documented: `_verbuf` 9].
const BUTTON_H: f32 = 36.0 + 18.0;
/// The first button's top, down the main rectangle [observed: fitted to the owner's
/// screenshot, the six labels from about 536 to 790 down the screen; the file's own
/// anchor, 144 above the information box, puts them lower].
const BUTTONS_TOP: f32 = 489.0;
const HINT_GAP: f32 = 16.0;
const STATUS_Y: f32 = 10.0;
/// How dark the world is behind the table's picture [design: as the lockpicking menu].
const SHADE: f32 = 1.0;
const SHADE_WITHOUT_VIEW: f32 = 0.88;
/// The table under the help box [observed: lockpicking's help, as bright].
const UNDER_HELP: f32 = 0.47;

#[derive(Component)]
pub struct BlackjackRoot;

/// A button and what it does.
#[derive(Component, Clone, Copy)]
pub struct BlackjackButton(pub BlackjackInput);

/// The alpha a node was drawn with, before the menu fades out.
#[derive(Component, Clone, Copy)]
pub struct DrawnAlpha(f32);

/// What the screen shows; drawn again when it changes.
fn view_key(s: &Session, now: f32, has_view: bool, view_w: f32) -> String {
    let labels: Vec<&str> = s.buttons().iter().map(|b| b.as_ref().map_or("-", |b| b.0)).collect();
    format!(
        "{}:{}:{}:{:?}:{}:{}:{}:{}:{view_w:.0}:{:?}",
        shown_bet(s),
        s.chips,
        s.earnings,
        s.status,
        labels.join("|"),
        s.busy(now),
        has_view,
        s.help.is_some(),
        crate::pad::labels(),
    )
}

/// The bet the box shows: the bet being set between rounds, the chips on the table in one.
fn shown_bet(s: &Session) -> u32 {
    if s.betting() {
        s.bet
    } else {
        s.table.staked()
    }
}

fn view_width(window: Option<&Window>) -> f32 {
    window.map_or(960.0 * 16.0 / 9.0, |w| 960.0 * w.width() / w.height().max(1.0))
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    screen: Res<BlackjackScreen>,
    view: Option<Res<BlackjackView>>,
    data: Option<Res<GameData>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    glyphs: Res<crate::pad::PadGlyphs>,
    roots: Query<Entity, With<BlackjackRoot>>,
    time: AidTime,
    (mut kit, mut shown): (Local<Option<Kit>>, Local<Option<String>>),
) {
    let view_w = view_width(window.single().ok());
    let now = time.elapsed_secs();
    let key = screen.0.as_ref().map(|s| view_key(s, now, view.is_some(), view_w));
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
    spawn(&mut commands, &mut c, s, view.as_deref(), view_w, now);
}

fn spawn(commands: &mut Commands, c: &mut Ctx, s: &Session, view: Option<&BlackjackView>, view_w: f32, now: f32) {
    let bright = amber(255.0);
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    let mut root = commands.spawn((BlackjackRoot, full.clone(), GlobalZIndex(106)));
    match view {
        Some(v) => {
            root.insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, SHADE)));
            let light = if s.help.is_some() { UNDER_HELP } else { 1.0 };
            root.with_children(|r| {
                r.spawn((full.clone(), ImageNode { color: Color::srgb(light, light, light), ..ImageNode::new(v.image.clone()) }));
            });
        }
        None => {
            root.insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, SHADE_WITHOUT_VIEW)));
        }
    }
    // The menu at the UI scale, in the middle of the view (`crate::interface::menu_frame`, #1631).
    root.with_children(|r| crate::interface::in_menu_frame(r, crate::interface::MIDDLE, |r| {
        let main = Node { position_type: PositionType::Absolute, left: u(EDGE), right: u(EDGE), top: u(EDGE), bottom: u(EDGE), ..default() };
        r.spawn(main).with_children(|m| {
            // The information box.
            let info = Node { position_type: PositionType::Absolute, left: u(INFO_IN), bottom: u(INFO_IN), width: u(INFO.x), height: u(INFO.y), ..default() };
            m.spawn(info).with_children(|b| {
                c.image(b, "interface\\shared\\line\\fade_to_top.dds", at(0.0, 0.0, THICK, FADE), bright);
                c.image(b, "interface\\shared\\line\\solid.dds", at(0.0, FADE - THICK, THICK, INFO.y - FADE), bright);
                c.image(b, "interface\\shared\\line\\solid.dds", at(0.0, INFO.y - THICK, INFO.x - FADE, THICK), bright);
                c.image(b, "interface\\shared\\line\\fade_to_right.dds", at(INFO.x - FADE - THICK, INFO.y - THICK, FADE, THICK), bright);
                let rows = [("Current Bet:".to_string(), shown_bet(s).to_string()), ("Chips:".into(), s.chips.to_string()), (s.earnings_title(), s.earnings.to_string())];
                for ((title, value), y) in rows.iter().zip(ROWS_Y) {
                    c.text_in(b, FontKind::Menu, title, ROW_X, y, false, bright);
                    c.text_in(b, FontKind::Menu, value, VALUE_X, y, true, bright);
                }
            });
            // The buttons, while the cards are down.
            if !s.busy(now) {
                let column = Node { position_type: PositionType::Absolute, right: u(BUTTONS_RIGHT), top: u(BUTTONS_TOP), width: u(0.0), height: u(6.0 * BUTTON_H), ..default() };
                m.spawn(column).with_children(|b| {
                    for (i, button) in s.buttons().into_iter().enumerate() {
                        let Some((label, hint, cmd)) = button else { continue };
                        let y = i as f32 * BUTTON_H;
                        let w = c.text_in(b, FontKind::Menu, label, 0.0, y + 9.0, true, bright);
                        c.text_in(b, FontKind::Menu, &hint, HINT_GAP, y + 9.0, false, bright);
                        b.spawn((Button, BlackjackButton(cmd), at(-w - 10.0, y, w + 20.0 + HINT_GAP + 30.0, BUTTON_H)));
                    }
                });
            }
            if let Some(status) = &s.status {
                let line = Node { position_type: PositionType::Absolute, right: u(BUTTONS_RIGHT), top: u(STATUS_Y), width: u(0.0), height: u(36.0), ..default() };
                m.spawn(line).with_children(|b| {
                    c.text_in(b, FontKind::Menu, status, 0.0, 0.0, true, bright);
                });
            }
        });
        if let Some(h) = &s.help {
            crate::lockpick::help::spawn(r, c, h, view_w);
        }
    }));
}

type Faded = (Option<&'static mut ImageNode>, Option<&'static mut BackgroundColor>, Option<&'static mut TextColor>, Option<&'static DrawnAlpha>);

/// As the menu leaves, everything it draws fades out (the table's picture with it).
pub fn fade(mut commands: Commands, screen: Res<BlackjackScreen>, time: AidTime, roots: Query<Entity, With<BlackjackRoot>>, children: Query<&Children>, mut nodes: Query<Faded>) {
    let Some(s) = screen.0.as_ref() else { return };
    if s.leaving.is_none() {
        return;
    }
    let f = s.fade(time.elapsed_secs());
    for root in &roots {
        for e in std::iter::once(root).chain(children.iter_descendants(root)) {
            let Ok((image, background, text, drawn)) = nodes.get_mut(e) else { continue };
            let current = image.as_ref().map(|i| i.color.alpha()).or(background.as_ref().map(|b| b.0.alpha())).or(text.as_ref().map(|t| t.0.alpha()));
            let Some(current) = current else { continue };
            let base = match drawn {
                Some(d) => d.0,
                None => {
                    commands.entity(e).insert(DrawnAlpha(current));
                    current
                }
            };
            let a = base * f;
            if let Some(mut i) = image {
                i.color.set_alpha(a);
            }
            if let Some(mut b) = background {
                b.0.set_alpha(a);
            }
            if let Some(mut t) = text {
                t.0.set_alpha(a);
            }
        }
    }
}
