//! The Caravan menu drawn flat from the game's textures: the table's planks
//! (`terminals\nv_caravan\nv_caravan-table01.dds`), the betting table's Interstate sign
//! (`…table02.dds`) and the cards' own faces and backs (their CCRD's TX00 and TX01). The
//! original draws 3D models of the same (caravan.md §8). Places and sizes are fitted to the
//! owner's screenshots (1280 × 720, here in the interface's units, 960 to the view's
//! height, in a frame 16:9 wide centred in the view, made smaller to fit a narrower view)
//! [observed]; the fonts, lines and help
//! box are the lockpicking menu's (`crate::lockpick::screen`).

use super::{CaravanInput, CaravanScreen, Pick, Session, Stage};
use crate::interface::{amber, at, u};
use crate::lockpick::screen::{Ctx, FontKind, Kit};
use crate::pause::AidTime;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use crate::ui_pointer::Button;
use bevy::window::PrimaryWindow;
use fnv_game::caravan::{Caravan, Game, Move, Phase, OPPONENT, PLAYER, SOLD_MAX, TRACKS};
use fnv_sim::FormId;
use nvf_assets::GameData;

/// The frame's width: 16:9 of the 960 high view.
const FRAME_W: f32 = 960.0 * 16.0 / 9.0;
const PLANKS: &str = "terminals\\nv_caravan\\nv_caravan-table01.dds";
const SIGN: &str = "terminals\\nv_caravan\\nv_caravan-table02.dds";
/// The sign's square, its left and top [observed: the screenshots' sign, -20 to 960 px
/// across, the top cut off].
const SIGN_AT: (f32, f32, f32) = (-27.0, -60.0, 1310.0);
/// A card on the table, in the hand, in the builder's rows and the builder's middle card.
const CARD: Vec2 = Vec2::new(111.0, 147.0);
const HAND_CARD: Vec2 = Vec2::new(125.0, 165.0);
const ROW_CARD: Vec2 = Vec2::new(150.0, 200.0);
const MIDDLE_CARD: Vec2 = Vec2::new(216.0, 299.0);
/// The middle of each track's caravan: the opponent's and the player's.
const OPPONENT_X: [f32; TRACKS] = [404.0, 667.0, 936.0];
const PLAYER_X: [f32; TRACKS] = [304.0, 576.0, 860.0];
/// The first card's top, and how far each next card lies (up for the opponent's).
const OPPONENT_TOP: f32 = 237.0;
const OPPONENT_STEP: f32 = -23.0;
const PLAYER_TOP: f32 = 480.0;
const PLAYER_STEP: f32 = 43.0;
/// A face card on a card: this far right and down of it, and of the face card before.
const FACE_STEP: Vec2 = Vec2::new(24.0, 12.0);
/// The track names and their values: the values' right ends, the names' left, the rows.
const VALUE_X: [f32; TRACKS] = [233.0, 500.0, 764.0];
const NAME_X: [f32; TRACKS] = [287.0, 551.0, 816.0];
const OPPONENT_ROW: f32 = 400.0;
const PLAYER_ROW: f32 = 440.0;
const OPPONENT_NAMES: [&str; TRACKS] = ["BONEYARD", "REDDING", "SHADY SANDS"];
const PLAYER_NAMES: [&str; TRACKS] = ["DAYGLOW", "NEW RENO", "THE HUB"];
/// Hands and decks.
const PLAYER_HAND: (f32, f32, f32) = (1067.0, 430.0, 28.0);
const OPPONENT_HAND: (f32, f32, f32) = (1080.0, 207.0, 22.0);
const PLAYER_DECK: (f32, f32) = (1360.0, 447.0);
const OPPONENT_DECK: (f32, f32) = (1340.0, 227.0);
/// The top right rows (titles' left, values' right) and the buttons' right end and tops.
const INFO_X: (f32, f32) = (1253.0, 1643.0);
const INFO_Y: [f32; 3] = [40.0, 84.0, 128.0];
const BUTTONS_RIGHT: f32 = 1664.0;
const PLAY_BUTTONS: [f32; 4] = [726.0, 778.0, 829.0, 879.0];
const BET_BUTTONS: [f32; 4] = [705.0, 756.0, 808.0, 858.0];
const HINT_GAP: f32 = 16.0;
/// The betting box: rows, titles' left, values' right, its bracket.
const BET_ROWS: [f32; 3] = [778.0, 824.0, 868.0];
const BET_X: (f32, f32) = (67.0, 533.0);
/// The builder: rows' tops, the middle card's tops, the step between cards.
const ROW_TOPS: (f32, f32) = (100.0, 600.0);
const MIDDLE_TOPS: (f32, f32) = (44.0, 560.0);
const ROW_STEP: f32 = 53.0;
const ROW_SHOWN: usize = 14;
const THICK: f32 = 2.0;
/// A place the chosen card can go: outlined, the one shown brighter.
const MARK: f32 = 3.0;
/// The carved names' colour [design: pale wood].
const CARVED: Color = Color::srgba(0.86, 0.83, 0.76, 0.55);
/// A caravan over 26 shows its value in red [observed].
const OVER: Color = Color::srgb(0.9, 0.15, 0.1);

#[derive(Component)]
pub struct CaravanRoot;

#[derive(Component, Clone, Copy)]
pub struct CaravanButton(pub CaravanInput);

#[derive(Component, Clone, Copy)]
pub struct DrawnAlpha(f32);

fn view_key(s: &Session, view_w: f32) -> String {
    let g = s.game.as_ref().map(|g| {
        let side = |i: usize| g.sides[i].caravans.iter().map(|c| c.cards.iter().map(|p| format!("{}{}", p.card.short(), p.faces.len())).collect::<String>()).collect::<Vec<_>>().join("|");
        format!("{}:{}:{}:{}:{}:{:?}", side(0), side(1), g.sides[0].hand.len(), g.sides[1].hand.len(), g.sides[0].deck.len() * 100 + g.sides[1].deck.len(), g.turn)
    });
    format!(
        "{:?}:{:?}:{:?}:{}:{}:{:?}:{:?}:{}:{:?}:{}:{}:{}:{view_w:.0}:{:?}",
        s.stage,
        s.bet,
        s.deck,
        s.cursor,
        s.hand,
        s.pick,
        s.status,
        s.help.is_some(),
        g,
        s.players_turn(),
        s.record.won,
        s.leaving.is_some(),
        crate::pad::labels(),
    )
}

fn view_width(window: Option<&Window>) -> f32 {
    window.map_or(FRAME_W, |w| 960.0 * w.width() / w.height().max(1.0))
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    screen: Res<CaravanScreen>,
    data: Option<Res<GameData>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    glyphs: Res<crate::pad::PadGlyphs>,
    roots: Query<Entity, With<CaravanRoot>>,
    _time: AidTime,
    (mut kit, mut shown, mut hints): (Local<Option<Kit>>, Local<Option<String>>, Local<String>),
) {
    let view_w = view_width(window.single().ok());
    let key = screen.0.as_ref().map(|s| view_key(s, view_w));
    if key == *shown {
        return;
    }
    // The hints, logged as they change (the device's names, #1662).
    if let Some(line) = screen.0.as_ref().map(|s| s.hint_line()).filter(|l| *l != *hints) {
        info!("caravan: hints: {line}");
        *hints = line;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    *shown = key;
    let Some(s) = screen.0.as_ref() else { return };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let kit = kit.get_or_insert_with(|| Kit::load(data.as_deref(), &mut images, bc));
    let mut c = Ctx { kit, data: data.as_deref(), images: &mut images, bc, glyphs: &glyphs };
    spawn(&mut commands, &mut c, s, view_w);
}

fn texture(s: &Session, f: FormId, face: bool) -> String {
    let (front, back) = s.textures.get(&f).cloned().unwrap_or_default();
    (if face { front } else { back }).unwrap_or_default()
}

fn spawn(commands: &mut Commands, c: &mut Ctx, s: &Session, view_w: f32) {
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    let dim = if s.help.is_some() { 0.47 } else { 1.0 };
    commands.spawn((CaravanRoot, full.clone(), GlobalZIndex(106), BackgroundColor(Color::BLACK))).with_children(|r| {
        c.image(r, PLANKS, full.clone(), Color::srgb(0.78 * dim, 0.74 * dim, 0.68 * dim));
        // The table at the UI scale, in the middle of the view (`crate::interface::menu_frame`,
        // #1631); the planks fill the view.
        crate::interface::in_menu_frame(r, crate::interface::MIDDLE, |r| {
            // A view narrower than 16:9 (16:10, 4:3) shows the whole frame, smaller.
            let frame = Node { position_type: PositionType::Absolute, left: u((view_w - FRAME_W) / 2.0), top: u(0.0), width: u(FRAME_W), height: u(960.0), ..default() };
            let fit = UiTransform { scale: Vec2::splat((view_w / FRAME_W).min(1.0)), ..default() };
            r.spawn((frame, fit)).with_children(|f| match s.stage {
                Stage::Bet => bet(f, c, s, dim),
                Stage::Deck => deck(f, c, s, dim),
                Stage::Play => play(f, c, s, dim),
                Stage::Results => results(f, c, s, dim),
        });
        if let Some(h) = &s.help {
            crate::lockpick::help::spawn(r, c, h, view_w);
        }
        });
    });
}

fn lit(on: bool) -> Color {
    amber(if on { 255.0 } else { 120.0 })
}

/// A button: its label right-aligned at `right`, its key after it; clickable when lit.
#[allow(clippy::too_many_arguments)]
fn button(p: &mut ChildSpawnerCommands, c: &mut Ctx, label: &str, key: &str, right: f32, top: f32, cmd: CaravanInput, on: bool) {
    let w = c.text_in(p, FontKind::Menu, label, right - HINT_GAP - 34.0, top, true, lit(on));
    c.text_in(p, FontKind::Menu, key, right - 34.0, top, false, lit(on));
    if on {
        p.spawn((Button, CaravanButton(cmd), at(right - HINT_GAP - 34.0 - w - 10.0, top - 9.0, w + HINT_GAP + 54.0, 54.0)));
    }
}

fn status(p: &mut ChildSpawnerCommands, c: &mut Ctx, s: &Session, top: f32) {
    if let Some(line) = &s.status {
        c.text_in(p, FontKind::Menu, line, BUTTONS_RIGHT, top, true, amber(255.0));
    }
}

fn sign(p: &mut ChildSpawnerCommands, c: &mut Ctx, dim: f32) {
    let (x, y, side) = SIGN_AT;
    c.image(p, SIGN, at(x, y, side, side), Color::srgb(0.85 * dim, 0.82 * dim, 0.76 * dim));
}

fn bracket(p: &mut ChildSpawnerCommands, c: &mut Ctx, left: f32, top: f32, w: f32, h: f32) {
    let b = amber(255.0);
    c.image(p, "interface\\shared\\line\\fade_to_top.dds", at(left, top, THICK, 30.0), b);
    c.image(p, "interface\\shared\\line\\solid.dds", at(left, top + 28.0, THICK, h - 28.0), b);
    c.image(p, "interface\\shared\\line\\solid.dds", at(left, top + h - THICK, w - 30.0, THICK), b);
    c.image(p, "interface\\shared\\line\\fade_to_right.dds", at(left + w - 32.0, top + h - THICK, 30.0, THICK), b);
}

fn bet(p: &mut ChildSpawnerCommands, c: &mut Ctx, s: &Session, dim: f32) {
    sign(p, c, dim);
    let b = amber(255.0);
    bracket(p, c, 40.0, 750.0, 545.0, 170.0);
    let rows = [(s.opponent_name.clone(), s.bet.bid), ("Current Ante:".into(), s.bet.ante), ("Total Funds:".into(), s.bet.funds)];
    for ((title, value), y) in rows.iter().zip(BET_ROWS) {
        c.text_in(p, FontKind::Menu, title, BET_X.0, y, false, b);
        c.text_in(p, FontKind::Menu, &value.to_string(), BET_X.1, y, true, b);
    }
    for ((label, key, cmd, on), top) in s.buttons().into_iter().zip(BET_BUTTONS) {
        button(p, c, label, &key, BUTTONS_RIGHT, top, cmd, on);
    }
    status(p, c, s, 10.0);
}

/// A card's texture (a plain card colour without it).
fn card(p: &mut ChildSpawnerCommands, c: &mut Ctx, path: &str, x: f32, y: f32, size: Vec2, light: f32) {
    c.image(p, path, at(x, y, size.x, size.y), Color::srgb(light, light, light));
}

fn deck(p: &mut ChildSpawnerCommands, c: &mut Ctx, s: &Session, dim: f32) {
    let b = amber(255.0);
    let mid = FRAME_W / 2.0;
    for (row, (top, middle_top)) in [(ROW_TOPS.0, MIDDLE_TOPS.0), (ROW_TOPS.1, MIDDLE_TOPS.1)].into_iter().enumerate() {
        // The cards outside in first, the middle one last, on top.
        let n = s.owned.len();
        let mut order: Vec<usize> = Vec::new();
        for d in (1..=ROW_SHOWN).rev() {
            if s.cursor >= d {
                order.push(s.cursor - d);
            }
            if s.cursor + d < n {
                order.push(s.cursor + d);
            }
        }
        if s.cursor < n {
            order.push(s.cursor);
        }
        for i in order {
            let f = s.owned[i];
            // The top row shows the deck's cards face up, the bottom row the others.
            let face = (row == 0) == s.in_deck(f);
            let path = texture(s, f, face);
            let off = i as f32 - s.cursor as f32;
            let (x, y, size) = if i == s.cursor {
                (mid - MIDDLE_CARD.x / 2.0, middle_top, MIDDLE_CARD)
            } else {
                let x = mid + off * ROW_STEP + if off < 0.0 { -MIDDLE_CARD.x / 2.0 - ROW_CARD.x + ROW_STEP } else { MIDDLE_CARD.x / 2.0 - ROW_STEP };
                (x, top, ROW_CARD)
            };
            card(p, c, &path, x, y, size, dim);
            if i != s.cursor {
                p.spawn((Button, CaravanButton(CaravanInput::CursorTo(i)), at(x, y, ROW_STEP.min(size.x), size.y)));
            }
        }
    }
    c.text_in(p, FontKind::Menu, "Caravan Deck", 87.0, 395.0, false, b);
    c.text_in(p, FontKind::Menu, &s.deck.len().to_string(), 507.0, 395.0, true, b);
    button(p, c, "Randomize Caravan Deck", &s.hint(&[KeyCode::KeyS]), 1380.0, 395.0, CaravanInput::Randomize, true);
    c.image(p, "interface\\shared\\line\\solid.dds", at(33.0, 480.0, FRAME_W - 66.0, THICK), b);
    c.text_in(p, FontKind::Menu, "<", 33.0, 462.0, false, b);
    c.text_in(p, FontKind::Menu, ">", FRAME_W - 33.0, 462.0, true, b);
    p.spawn((Button, CaravanButton(CaravanInput::Step(-1)), at(20.0, 455.0, 50.0, 50.0)));
    p.spawn((Button, CaravanButton(CaravanInput::Step(1)), at(FRAME_W - 70.0, 455.0, 50.0, 50.0)));
    c.text_in(p, FontKind::Menu, "Total Cards:", 87.0, 515.0, false, b);
    c.text_in(p, FontKind::Menu, &s.owned.len().to_string(), 507.0, 515.0, true, b);
    c.text_in(p, FontKind::Menu, "All Cards", 965.0, 527.0, false, b);
    let here = s.owned.get(s.cursor).copied();
    button(p, c, "Add", &s.hint(&[KeyCode::KeyW]), 600.0, 862.0, CaravanInput::Add, here.is_some_and(|f| !s.in_deck(f)));
    button(p, c, "Remove", &s.hint(&[KeyCode::KeyA]), 1330.0, 862.0, CaravanInput::Remove, here.is_some_and(|f| s.in_deck(f)));
    button(p, c, "Play Caravan", &s.hint(&[KeyCode::KeyF]), BUTTONS_RIGHT, 862.0, CaravanInput::PlayDeck, s.deck.len() >= fnv_game::caravan::MIN_DECK);
    status(p, c, s, 10.0);
}

/// Where a caravan's card `i` lies (its top left).
fn place(side: usize, track: usize, i: usize) -> Vec2 {
    if side == OPPONENT {
        Vec2::new(OPPONENT_X[track] - CARD.x / 2.0, OPPONENT_TOP + OPPONENT_STEP * i as f32)
    } else {
        Vec2::new(PLAYER_X[track] - CARD.x / 2.0, PLAYER_TOP + PLAYER_STEP * i as f32)
    }
}

fn caravan(p: &mut ChildSpawnerCommands, c: &mut Ctx, s: &Session, side: usize, track: usize, k: &Caravan, dim: f32) {
    for (i, placed) in k.cards.iter().enumerate() {
        let at0 = place(side, track, i);
        card(p, c, &texture(s, placed.card.form, true), at0.x, at0.y, CARD, dim);
        for (j, f) in placed.faces.iter().enumerate() {
            let o = at0 + FACE_STEP * (j + 1) as f32;
            card(p, c, &texture(s, f.form, true), o.x, o.y, CARD, dim);
        }
    }
}

/// An outline round a place: lit for the one shown; clickable.
fn mark(p: &mut ChildSpawnerCommands, c: &mut Ctx, at0: Vec2, size: Vec2, shown: bool, cmd: CaravanInput) {
    let col = if shown { amber(255.0) } else { amber(110.0) };
    let t = if shown { MARK + 1.0 } else { MARK };
    let solid = "interface\\shared\\line\\solid.dds";
    c.image(p, solid, at(at0.x - t, at0.y - t, size.x + 2.0 * t, t), col);
    c.image(p, solid, at(at0.x - t, at0.y + size.y, size.x + 2.0 * t, t), col);
    c.image(p, solid, at(at0.x - t, at0.y, t, size.y), col);
    c.image(p, solid, at(at0.x + size.x, at0.y, t, size.y), col);
    p.spawn((Button, CaravanButton(cmd), at(at0.x, at0.y, size.x, size.y)));
}

fn marks(p: &mut ChildSpawnerCommands, c: &mut Ctx, s: &Session, g: &Game) {
    match &s.pick {
        Pick::Place { moves, at: shown } => {
            for (n, m) in moves.iter().enumerate() {
                let Move::Play { side, track, on, .. } = *m else { continue };
                let k = &g.sides[side].caravans[track];
                let spot = match on {
                    None => place(side, track, k.cards.len()),
                    Some(i) => place(side, track, i) + FACE_STEP * (k.cards[i].faces.len() + 1) as f32,
                };
                mark(p, c, spot, CARD, n == *shown, CaravanInput::Place(n));
            }
        }
        Pick::Track { tracks, at: shown } => {
            for (n, &t) in tracks.iter().enumerate() {
                let len = g.sides[PLAYER].caravans[t].cards.len().max(1);
                let top = place(PLAYER, t, 0);
                let h = CARD.y + PLAYER_STEP * (len - 1) as f32;
                mark(p, c, top, Vec2::new(CARD.x, h), n == *shown, CaravanInput::Place(n));
            }
        }
        Pick::Hand => {}
    }
}

fn play(p: &mut ChildSpawnerCommands, c: &mut Ctx, s: &Session, dim: f32) {
    let Some(g) = s.game.as_ref() else { return };
    let b = amber(255.0);
    // The carved names and the caravans' values.
    for t in 0..TRACKS {
        for (side, row, names) in [(OPPONENT, OPPONENT_ROW, OPPONENT_NAMES), (PLAYER, PLAYER_ROW, PLAYER_NAMES)] {
            c.text_in(p, FontKind::Menu, names[t], NAME_X[t], row, false, CARVED);
            let k = &g.sides[side].caravans[t];
            let v = k.value();
            c.text_in(p, FontKind::Menu, &v.to_string(), VALUE_X[t], row, true, if v > SOLD_MAX { OVER } else { b });
        }
    }
    // The opponent's caravans are drawn first: the player's lie over them.
    for side in [OPPONENT, PLAYER] {
        for (t, k) in g.sides[side].caravans.iter().enumerate() {
            caravan(p, c, s, side, t, k, dim);
        }
    }
    // Hands and decks.
    let (ox, oy, ostep) = OPPONENT_HAND;
    for (i, card_) in g.sides[OPPONENT].hand.iter().enumerate() {
        card(p, c, &texture(s, card_.form, false), ox + ostep * i as f32, oy, CARD, dim);
    }
    if let Some(top) = g.sides[OPPONENT].deck.last() {
        card(p, c, &texture(s, top.form, false), OPPONENT_DECK.0, OPPONENT_DECK.1, CARD, dim);
    }
    c.text_in(p, FontKind::Menu, &g.sides[OPPONENT].deck.len().to_string(), OPPONENT_DECK.0 + CARD.x + 22.0, OPPONENT_DECK.1 + CARD.y - 40.0, true, b);
    let (hx, hy, hstep) = PLAYER_HAND;
    let mine = s.players_turn();
    let chosen = match &s.pick {
        Pick::Place { moves, .. } => moves.first().and_then(|m| if let Move::Play { card, .. } = m { Some(*card) } else { None }),
        _ => None,
    };
    let hand = &g.sides[PLAYER].hand;
    for (i, card_) in hand.iter().enumerate() {
        let raised = mine && (i == s.hand || chosen == Some(i));
        let y = hy - if raised { 28.0 } else { 0.0 };
        let x = hx + hstep * i as f32;
        card(p, c, &texture(s, card_.form, true), x, y, HAND_CARD, if raised || !mine { dim } else { 0.85 * dim });
        let w = if i + 1 == hand.len() { HAND_CARD.x } else { hstep };
        if mine {
            p.spawn((Button, CaravanButton(CaravanInput::PickCard(i)), at(x, y, w, HAND_CARD.y)));
        }
    }
    if let Some(top) = g.sides[PLAYER].deck.last() {
        card(p, c, &texture(s, top.form, false), PLAYER_DECK.0, PLAYER_DECK.1, HAND_CARD, dim);
    }
    c.text_in(p, FontKind::Menu, &g.sides[PLAYER].deck.len().to_string(), PLAYER_DECK.0 + HAND_CARD.x + 25.0, PLAYER_DECK.1 + HAND_CARD.y - 40.0, true, b);
    if mine {
        marks(p, c, s, g);
    }
    // The top right rows.
    let rows = [("Ante Pot:", s.bet.pot().to_string()), ("Net Total:", s.record.net().to_string()), ("Win/Loss:", format!("{}/{}", s.record.won, s.record.lost))];
    for ((title, value), y) in rows.iter().zip(INFO_Y) {
        c.text_in(p, FontKind::Menu, title, INFO_X.0, y, false, b);
        c.text_in(p, FontKind::Menu, value, INFO_X.1, y, true, b);
    }
    if g.phase != Phase::Over && !mine && s.help.is_none() {
        c.text_in(p, FontKind::Menu, &format!("{} is thinking…", s.opponent_name), BUTTONS_RIGHT, 660.0, true, amber(180.0));
    }
    for ((label, key, cmd, on), top) in s.buttons().into_iter().zip(PLAY_BUTTONS) {
        button(p, c, label, &key, BUTTONS_RIGHT, top, cmd, on);
    }
    status(p, c, s, 600.0);
}

fn results(p: &mut ChildSpawnerCommands, c: &mut Ctx, s: &Session, dim: f32) {
    sign(p, c, dim);
    let b = amber(255.0);
    c.text_in(p, FontKind::Menu, "Press Any Button to Continue", 436.0, 233.0, false, b);
    let r = &s.record;
    let rows = [
        ("Winnings to Date:", r.winnings.to_string()),
        ("Losses to Date:", r.losses.to_string()),
        ("Win/Loss Totals:", format!("{}/{}", r.won, r.lost)),
        ("Biggest Ante Pot Won:", r.biggest_pot.to_string()),
    ];
    for (i, (title, value)) in rows.iter().enumerate() {
        let y = 333.0 + 45.0 * i as f32;
        c.text_in(p, FontKind::Menu, title, 235.0, y, false, b);
        c.text_in(p, FontKind::Menu, value, 805.0, y, true, b);
    }
    status(p, c, s, 10.0);
}

type Faded = (Option<&'static mut ImageNode>, Option<&'static mut BackgroundColor>, Option<&'static mut TextColor>, Option<&'static DrawnAlpha>);

/// As the menu leaves, everything it draws fades out.
pub fn fade(mut commands: Commands, screen: Res<CaravanScreen>, time: AidTime, roots: Query<Entity, With<CaravanRoot>>, children: Query<&Children>, mut nodes: Query<Faded>) {
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
