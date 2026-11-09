//! SUP batch 6's engine side (`fnv_game::nvse_sup6`; docs/spec-notes/ext-sup6.md): what
//! only the engine knows goes into `Game::sup` each frame before the scripts (the mouse,
//! the window's size, the fonts' heights, the lockpick menu, how long the throw is
//! held), and the scripts' requests come back after them: FakeMouseMovement's moves (as
//! mouse movement of the next frame), a bobby pin's new health, the debug texts (drawn
//! as plain text over everything) and the cursor's traits (the arrow redrawn).

use crate::script::ScriptState;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseMotion, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_game::nvse_sup6::LockpickView;

pub struct SupScreenPlugin;

impl Plugin for SupScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrawnTexts>()
            .add_systems(PreUpdate, tell.after(bevy::input::InputSystems))
            .add_systems(Update, (take, draw_texts).after(crate::PlaySet::Scripts));
    }
}

/// Pixels a wheel notch is when the system gives the wheel in pixels [hypothesis].
const PIXELS_A_NOTCH: f32 = 40.0;

/// Before the scripts: the mouse, the window, the fonts, the lockpick menu, the throw.
#[allow(clippy::too_many_arguments)]
fn tell(
    mut script: Option<ResMut<ScriptState>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    windows: Query<&Window, With<PrimaryWindow>>,
    data: Option<Res<nvf_assets::GameData>>,
    lockpick: Option<Res<crate::lockpick::LockpickScreen>>,
    control: Option<Res<crate::control::PlayerControl>>,
    combat: Option<Res<crate::combat::Combat>>,
    time: Res<Time>,
    mut thrown_held: Local<bool>,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let thrown = combat.is_some_and(|c| c.player_weapon_is_throwable());
    let sup = &mut rt.game.sup;
    if let Ok(w) = windows.single() {
        sup.window = (f64::from(w.physical_width()), f64::from(w.physical_height()));
        if let Some(p) = w.physical_cursor_position() {
            sup.mouse.pos = (f64::from(p.x), f64::from(p.y));
        }
    }
    let d = motion.delta;
    sup.mouse.moved = (f64::from(d.x), f64::from(d.y));
    if d != Vec2::ZERO {
        sup.mouse.last_moved = sup.mouse.moved;
    }
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / PIXELS_A_NOTCH,
    };
    sup.mouse.wheel = f64::from(notches);
    if notches != 0.0 {
        sup.mouse.last_wheel = sup.mouse.wheel;
    }
    // The fonts' line heights, once (fonts 1-9, `crate::interface::FONT_FILES`, the
    // install's own files).
    if sup.fonts.is_empty() {
        if let Some(data) = data.as_deref() {
            sup.fonts = crate::interface::FONT_FILES
                .iter()
                .map(|f| {
                    let font = data.vfs.read(&crate::interface::font_file(f)).and_then(|r| r.ok()).and_then(|b| fnv_font::Font::parse(&b).ok());
                    font.map_or(0.0, |f| f64::from(f.size))
                })
                .collect();
        }
    }
    sup.lockpick = lockpick.and_then(|l| {
        let s = l.0.as_ref()?;
        let p = &s.game;
        Some(LockpickView { angle: p.pin, level: p.level.required_skill().unwrap_or(100.0), skill: p.skill, health: p.health })
    });
    // The throw: from the attack's press with a throwable in hand to its release; the
    // time stays until the next press [documented: GetGrenadeTimeHeld].
    let pressed = control.is_some_and(|c| c.trigger) && thrown;
    if pressed {
        if !*thrown_held {
            sup.grenade_held = 0.0;
        }
        sup.grenade_held += time.delta_secs_f64();
    }
    *thrown_held = pressed;
}

/// After the scripts: the fake moves and the pin's health.
fn take(mut script: Option<ResMut<ScriptState>>, mut moves: MessageWriter<MouseMotion>, mut lockpick: Option<ResMut<crate::lockpick::LockpickScreen>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    for (x, y) in std::mem::take(&mut rt.game.sup.fake_moves) {
        moves.write(MouseMotion { delta: Vec2::new(x, y) });
    }
    if let Some(h) = rt.game.sup.pin_health.take() {
        if let Some(s) = lockpick.as_deref_mut().and_then(|l| l.0.as_mut()) {
            s.game.health = h;
            info!("sup: the bobby pin's health set to {h}");
        }
    }
}

/// The debug texts drawn, by name.
#[derive(Resource, Default)]
struct DrawnTexts(std::collections::HashMap<String, (Entity, fnv_game::nvse_sup6::DebugText)>);

#[derive(Component)]
struct DebugTextNode;

/// The debug texts as plain white text over everything, at their pixel places
/// [hypothesis: the page shows no picture of one].
fn draw_texts(mut commands: Commands, script: Option<Res<ScriptState>>, mut drawn: ResMut<DrawnTexts>, windows: Query<&Window, With<PrimaryWindow>>) {
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else { return };
    let texts = &rt.game.sup.debug_texts;
    let scale = windows.single().map_or(1.0, |w| w.scale_factor());
    drawn.0.retain(|name, (e, _)| {
        let keep = texts.contains_key(name);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    for (name, t) in texts {
        if drawn.0.get(name).is_some_and(|(_, old)| old == t) {
            continue;
        }
        if let Some((e, _)) = drawn.0.remove(name) {
            commands.entity(e).despawn();
        }
        let node = Node { position_type: PositionType::Absolute, left: Val::Px(t.x / scale), top: Val::Px(t.y / scale), ..default() };
        let e = commands
            .spawn((node, Text::new(t.text.clone()), TextFont { font_size: bevy::text::FontSize::Px(t.zoom / scale), ..default() }, TextColor(Color::WHITE), GlobalZIndex(i32::MAX - 1), DebugTextNode))
            .id();
        drawn.0.insert(name.clone(), (e, t.clone()));
    }
}
