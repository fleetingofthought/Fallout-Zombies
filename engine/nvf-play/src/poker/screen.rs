//! The poker table's text and choices over the table's picture ([`super::PokerView`], drawn
//! by the engine), in the HUD's colour with the game's fonts (docs/design/poker.md §4).
//! Lengths are the interface's units (960 to the view's height) [design: the places are
//! NVForever's, set out as on the mock-up the owner approved, 2026-10-16].

use super::{PokerInput, PokerScreen, PokerView, Session, BIG_BLIND, PLAYER, SMALL_BLIND};
use crate::interface::{amber, at, face, plain_font, Face};
use crate::pause::AidTime;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use crate::ui_pointer::Button;
use bevy::window::PrimaryWindow;
use fnv_font::Line;
use fnv_game::poker::Street;
use nvf_assets::GameData;

/// The glowing font of the headings and choices, and the plain one of the rest (fonts 1
/// and 7, as the lockpicking menu's help box and menu use them).
const GLOW_FONT: &str = "textures\\fonts\\glow_monofonto_large.fnt";
const PLAIN_FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";
const GLOW_BASE: f32 = 15.0;
const PLAIN_BASE: f32 = 19.5;
/// From the view's edges to the text [design: as the lockpicking menu's main rectangle].
const EDGE: f32 = 50.0;
const LINE: f32 = 36.0;
/// The choices' row: its top above the bottom edge, and the room between choices.
const CHOICES_UP: f32 = 60.0;
const CHOICE_GAP: f32 = 60.0;
/// The result line, its top from the view's top.
const STATUS_Y: f32 = 128.0;
/// Text not in use, and secondary lines.
const DIM: f32 = 120.0;
const SOFT: f32 = 190.0;

#[derive(Component)]
pub struct PokerRoot;

/// A choice and what it does.
#[derive(Component, Clone, Copy)]
pub struct PokerButton(pub PokerInput);

#[derive(Component, Clone, Copy)]
pub struct DrawnAlpha(f32);

pub struct Kit {
    glow: Face,
    plain: Face,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Font {
    Glow,
    Plain,
}

struct Ctx<'a> {
    kit: &'a Kit,
    glyphs: &'a crate::pad::PadGlyphs,
}

impl Ctx<'_> {
    fn face(&self, f: Font) -> (&Face, f32) {
        match f {
            Font::Glow => (&self.kit.glow, GLOW_BASE),
            Font::Plain => (&self.kit.plain, PLAIN_BASE),
        }
    }

    fn line(&self, f: Font, s: &str) -> Line {
        self.face(f).0.font.lines(s, None).into_iter().next().unwrap_or_default()
    }

    fn width(&self, f: Font, s: &str) -> f32 {
        self.line(f, s).width
    }

    /// One line, its top at `top`; `align` 0 left, 0.5 centred, 1 right of `x`. Its width.
    #[allow(clippy::too_many_arguments)]
    fn text(&self, p: &mut ChildSpawnerCommands, f: Font, s: &str, x: f32, top: f32, align: f32, color: Color) -> f32 {
        let (face, base) = self.face(f);
        let width = crate::pad::hint_width(face, s, self.glyphs);
        crate::pad::set_hint(p, face, s, x - width * align, top + base, color, self.glyphs);
        width
    }
}

fn view_width(window: Option<&Window>) -> f32 {
    window.map_or(960.0 * 16.0 / 9.0, |w| 960.0 * w.width() / w.height().max(1.0))
}

/// A seat's two label lines: name and chips; its last choice (or how it stands).
pub fn seat_lines(s: &Session, i: usize) -> (String, String) {
    let seat = &s.table.seats[i];
    let mut name = format!("{}   {}", seat.name.to_uppercase(), seat.stack);
    if s.table.button == i && !seat.out {
        name.push_str("   D");
    }
    let state = if seat.out {
        "OUT".to_string()
    } else if seat.folded {
        "FOLDED".to_string()
    } else if s.table.street == Street::Over && s.table.showdown && seat.in_hand() {
        s.table.best_hand(i).map(|v| v.describe().to_uppercase()).unwrap_or_default()
    } else if s.table.to_act == Some(i) {
        "...".to_string()
    } else {
        seat.last.clone().unwrap_or_default().to_uppercase()
    };
    (name, state)
}

/// The choices the bottom row offers: label, what it does, usable.
pub fn choices(s: &Session) -> Vec<(String, PokerInput, bool)> {
    let turn = s.players_turn();
    let legal = s.table.legal(PLAYER);
    let mut out = Vec::new();
    // The keys, with a controller the buttons that press them (`crate::pad::menu_buttons`)
    // [#1662].
    let at = s.hint_at();
    let h = |k: KeyCode| crate::pad::hint(&[k], at);
    if s.table.street != Street::Over {
        let e = h(KeyCode::KeyE);
        let call = if legal.to_call == 0 { format!("{e} CHECK") } else { format!("{e} CALL {}", legal.to_call) };
        out.push((call, PokerInput::CheckCall, turn));
        // The amount as the raise would be made: never below the least raise.
        let r = h(KeyCode::KeyR);
        let raise = match legal.raise.map(|(lo, hi)| s.raise_to.clamp(lo, hi)) {
            Some(to) if s.table.current_bet == 0 => format!("{r} BET {to}"),
            Some(to) => format!("{r} RAISE TO {to}"),
            None => format!("{r} RAISE"),
        };
        out.push((raise, PokerInput::Raise(None), turn && legal.raise.is_some()));
        out.push((format!("{} ALL IN", h(KeyCode::KeyA)), PokerInput::AllIn, turn));
        out.push((format!("{} FOLD", h(KeyCode::KeyF)), PokerInput::Fold, turn));
    } else if s.ended.is_none() {
        out.push((format!("{} NEXT HAND", upper_unless_glyph(h(KeyCode::Space))), PokerInput::Next, s.leaving.is_none()));
    }
    out.push((format!("{} LEAVE", upper_unless_glyph(crate::pad::hint(&[KeyCode::Tab, KeyCode::Escape], at))), PokerInput::Exit, s.leaving.is_none()));
    out
}

/// A hint in the all-caps choices row: the key's name is uppercased, but a checked button's
/// name is left as it is, or `pad::find_glyph` (which matches "Circle)" and not "CIRCLE)")
/// would not draw its picture.
fn upper_unless_glyph(hint: String) -> String {
    if crate::pad::has_glyph(&hint) {
        hint
    } else {
        hint.to_uppercase()
    }
}

/// What the screen shows; drawn again when it changes.
fn view_key(s: &Session, view: Option<&PokerView>, view_w: f32) -> String {
    let seats: Vec<String> = (0..s.table.seats.len()).map(|i| format!("{:?}", seat_lines(s, i))).collect();
    let anchors: Vec<String> = view.map(|v| v.anchors.iter().map(|a| format!("{:.3},{:.3}", a.x, a.y)).collect()).unwrap_or_default();
    format!(
        "{}|{}|{:?}|{:?}|{}|{:?}|{:?}|{}|{view_w:.0}|{}|{:?}",
        s.table.hand,
        s.table.pot(),
        s.table.street,
        s.status,
        seats.join(";"),
        choices(s).iter().map(|c| (&c.0, c.2)).collect::<Vec<_>>(),
        s.table.best_hand(PLAYER),
        view.is_some(),
        anchors.join(";"),
        s.leaving.is_some()
    )
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    screen: Res<PokerScreen>,
    view: Option<Res<PokerView>>,
    data: Option<Res<GameData>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    glyphs: Res<crate::pad::PadGlyphs>,
    roots: Query<Entity, With<PokerRoot>>,
    (mut kit, mut shown): (Local<Option<Kit>>, Local<Option<String>>),
) {
    let view_w = view_width(window.single().ok());
    let key = screen.0.as_ref().map(|s| view_key(s, view.as_deref(), view_w));
    if key == *shown {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    *shown = key;
    let Some(s) = screen.0.as_ref() else { return };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let kit = kit.get_or_insert_with(|| Kit {
        glow: face(GLOW_FONT, plain_font(31.0, 12.0), data.as_deref(), &mut images, bc),
        plain: face(PLAIN_FONT, plain_font(36.0, 15.0), data.as_deref(), &mut images, bc),
    });
    spawn(&mut commands, &Ctx { kit, glyphs: &glyphs }, s, view.as_deref(), view_w);
}

fn spawn(commands: &mut Commands, c: &Ctx, s: &Session, view: Option<&PokerView>, view_w: f32) {
    let bright = amber(255.0);
    let soft = amber(SOFT);
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    let mut root = commands.spawn((PokerRoot, full.clone(), GlobalZIndex(106), BackgroundColor(Color::BLACK)));
    if let Some(v) = view {
        root.with_children(|r| {
            r.spawn((full.clone(), ImageNode::new(v.image.clone())));
        });
    }
    let t = &s.table;
    // The menu at the UI scale, in the middle of the view (`crate::interface::menu_frame`, #1631).
    root.with_children(|r| crate::interface::in_menu_frame(r, crate::interface::MIDDLE, |r| {
        // The heading, top left; the pot, top right.
        let title = format!("{}  -  TEXAS HOLD'EM", s.casino.title().to_uppercase());
        c.text(r, Font::Glow, &title, EDGE, EDGE, 0.0, bright);
        c.text(r, Font::Plain, &format!("BLINDS {SMALL_BLIND}/{BIG_BLIND}   HAND {}", t.hand), EDGE, EDGE + LINE, 0.0, soft);
        c.text(r, Font::Glow, &format!("POT {}", t.pot()), view_w - EDGE, EDGE, 1.0, bright);
        let street = if t.street == Street::Over && !t.showdown { "" } else { t.street.name() };
        c.text(r, Font::Plain, &street.to_uppercase(), view_w - EDGE, EDGE + LINE, 1.0, soft);
        // The opponents' labels at their seats.
        for i in 1..t.seats.len() {
            let Some(a) = view.and_then(|v| v.anchors.get(i)).copied().or_else(|| fallback_anchor(i)) else { continue };
            let (name, state) = seat_lines(s, i);
            let (x, y) = (a.x * view_w, a.y * 960.0);
            let half = c.width(Font::Plain, &name).max(c.width(Font::Plain, &state)) / 2.0;
            let x = x.clamp(EDGE + half, view_w - EDGE - half);
            let lit = if t.to_act == Some(i) { bright } else if t.seats[i].in_hand() { amber(230.0) } else { amber(DIM) };
            c.text(r, Font::Plain, &name, x, y - LINE, 0.5, lit);
            c.text(r, Font::Plain, &state, x, y, 0.5, lit);
        }
        // The hand's result.
        if let Some(st) = &s.status {
            c.text(r, Font::Glow, &st.to_uppercase(), view_w / 2.0, STATUS_Y, 0.5, bright);
        }
        // The player's chips, the call and the best hand, bottom left.
        let p = s.player();
        let bottom = 960.0 - EDGE;
        c.text(r, Font::Glow, &format!("CHIPS {}", p.stack), EDGE, bottom - 3.0 * LINE, 0.0, bright);
        let legal = t.legal(PLAYER);
        let call_line = if t.street != Street::Over && p.in_hand() && legal.to_call > 0 { format!("TO CALL {}", legal.to_call) } else { String::new() };
        c.text(r, Font::Plain, &call_line, EDGE, bottom - 2.0 * LINE, 0.0, soft);
        let hand = if p.in_hand() { t.best_hand(PLAYER).map(|v| v.describe().to_uppercase()).unwrap_or_default() } else { String::new() };
        c.text(r, Font::Plain, &hand, EDGE, bottom - LINE, 0.0, soft);
        // The choices, bottom right, right to left.
        let mut x = view_w - EDGE;
        let y = 960.0 - EDGE - CHOICES_UP;
        for (label, cmd, on) in choices(s).into_iter().rev() {
            // The drawn width, not the text's: a hint that names a button draws its
            // picture, so the click box and the gap must follow that width.
            let w = c.text(r, Font::Glow, &label, x, y, 1.0, amber(if on { 255.0 } else { DIM }));
            if on {
                r.spawn((Button, PokerButton(cmd), at(x - w - 8.0, y - 8.0, w + 16.0, LINE + 16.0)));
            }
            x -= w + CHOICE_GAP;
        }
    }));
}

/// Seat labels without the engine's picture: round the top of the view [design].
fn fallback_anchor(i: usize) -> Option<Vec2> {
    [None, Some(Vec2::new(0.12, 0.45)), Some(Vec2::new(0.3, 0.16)), Some(Vec2::new(0.7, 0.16)), Some(Vec2::new(0.88, 0.45))].get(i).copied().flatten()
}

type Faded = (Option<&'static mut ImageNode>, Option<&'static mut BackgroundColor>, Option<&'static mut TextColor>, Option<&'static DrawnAlpha>);

/// As the table closes, everything it draws fades out.
pub fn fade(mut commands: Commands, screen: Res<PokerScreen>, time: AidTime, roots: Query<Entity, With<PokerRoot>>, children: Query<&Children>, mut nodes: Query<Faded>) {
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
