//! The character creator: NVForever's own race menu (menu 1036, ShowRaceMenu; Doc
//! Mitchell's "let's have a look at you" in the opening), docs/spec-notes/character-creator.md.
//! A new design, built from the game's own data and fonts only [owner 2026-10-16]; the
//! choices and sliders are `fnv_actor::creator`'s, the tabs and rows [`model`]'s, the
//! drawing [`view`]'s.
//!
//! - **Opening.** When the scripts open menu 1036 and the engine gave the creator its
//!   data ([`CreatorData`], from the actor records), the creator opens on the player's
//!   look (`PlayerState::look`, else the player record's), puts the camera in front of
//!   the player ([`crate::control::Portrait`]) and leaves the placeholder panel
//!   (`crate::script_hud`) undrawn. Without the data the placeholder stays.
//! - **Live.** Every change is written into the game at once (`fnv_game::chargen::
//!   apply_look`), so the body drawn (the engine's `player_body.rs`) shows it.
//! - **Done** answers the menu (as the other character menus do, `chargen::finish`); the
//!   look stays. There is no cancel: Reset goes back to the look the menu opened with.
//! - **Keys** [design]: Q / E, Tab, Page Up / Down: tabs; Up / Down (W / S): rows, then
//!   the buttons; Left / Right (A / D): change the row, Shift for small steps; Space
//!   randomise, Z undo, R reset; Enter presses the chosen button, or goes to Done.
//!   **Pointer** (shown while the creator is open, whatever state the game is in):
//!   click a tab, a row, its arrows, a swatch, a button; drag along a slider; the wheel
//!   over the panel steps the rows. Over the picture [owner 2026-10-17: "a way to drag
//!   and pan our view not just zoom in and out"]: left-drag turns the camera about the
//!   player and tilts it, right- or middle-drag (or Shift + left-drag) pans, the wheel
//!   zooms (the point looked at follows the zoom from the body to the face,
//!   [`model::aim_drop`]); C or Home puts the tab's framing back. **Controller** (D-pad,
//!   A, LB / RB as the menu keys of `crate::pad`): X randomise, Y undo, Back reset; the
//!   right stick turns and tilts, the left stick pans, the triggers zoom, the right
//!   stick's click puts the framing back.
//! - **Test aid** `NVF_PLAY_TEST_CREATOR=<seconds>:<op>[,...]`: `tab:<name>`,
//!   `next:<row label>` / `prev:<row label>` (change a row once), `set:<row label>:<0–1>`
//!   (a slider's place in its range), `random`, `undo`, `reset`, `done`; the camera:
//!   `orbit:<degrees right>:<degrees up>`, `pan:<units right>:<units up>`,
//!   `zoom:<factor>` (below 1 nearer), `view` (the tab's framing again).
//!
//! Log lines start `creator:`: opening (the look and how many sliders), each change, Done
//! with the look.

pub mod model;
pub mod view;

use crate::pause::AidTime;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::mouse::{AccumulatedMouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_actor::creator::Creator;
use fnv_actor::records::FaceGen;
use fnv_game::chargen as rules;
use fnv_game::game::menus;
use model::{Action, Model, Row, RowView, Tab, ACTIONS, TABS};
use nvf_assets::GameData;
use std::sync::Arc;
use view::{Hit, HitWhat};

/// The creator's data, given by the engine once the actor records are read.
#[derive(Resource, Clone)]
pub struct CreatorData(pub Arc<Creator>);

/// The game's look (what saves keep) as the actor resolver takes it.
pub fn actor_look(l: &rules::Look) -> fnv_actor::Look {
    let w = |v: &[f32]| v.iter().map(|&x| f64::from(x)).collect();
    fnv_actor::Look {
        race: l.race,
        female: l.female,
        hair: l.hair,
        hair_color: l.hair_color,
        eyes: l.eyes,
        head_parts: l.head_parts.clone(),
        facegen: FaceGen { fggs: w(&l.fggs), fgga: w(&l.fgga), fgts: w(&l.fgts) },
    }
}

/// The resolver's look as the game keeps it.
pub fn game_look(l: &fnv_actor::Look) -> rules::Look {
    let n = |v: &[f64]| v.iter().map(|&x| x as f32).collect();
    rules::Look {
        race: l.race,
        female: l.female,
        hair: l.hair,
        hair_color: l.hair_color,
        eyes: l.eyes,
        head_parts: l.head_parts.clone(),
        fggs: n(&l.facegen.fggs),
        fgga: n(&l.facegen.fgga),
        fgts: n(&l.facegen.fgts),
    }
}

/// The player's look after SexChange with reset (`fnv_game::appearance`): `start` (the
/// look now, or the record's, with the new race and sex) with the race's first starting
/// face for that sex ([`Model::reset_to_preset`]).
pub fn preset_look(data: &CreatorData, start: fnv_actor::Look) -> fnv_actor::Look {
    let mut m = Model::new(data.0.clone(), start);
    m.reset_to_preset();
    m.look
}

/// Whether the creator draws this menu (else `crate::script_hud`'s placeholder does).
pub fn draws(code: u16, data: bool) -> bool {
    code == menus::RACE && data
}

#[derive(Resource, Default)]
pub struct CreatorMenu {
    model: Option<Model>,
    dirty: bool,
    answered: bool,
    hits: Vec<Hit>,
    /// The slider row being dragged.
    drag: Option<usize>,
    /// The scrollbar is being dragged.
    scroll_drag: bool,
    /// Where the pointer was last frame: hovering chooses only when it moves (or the
    /// rows scrolled under it), so it does not take the choice back from the keys.
    last_pointer: Option<Vec2>,
    hover_after_scroll: bool,
    /// The camera being dragged, and with which button.
    camera_drag: Option<CameraDrag>,
    camera_button: Option<MouseButton>,
    kit: Option<view::Kit>,
    /// The view's width the panel was last drawn for: it stands at the view's right.
    drawn_w: f32,
    /// The look last written into the game.
    written: Option<fnv_actor::Look>,
    /// Opened by the gene projector: drawn in a studio (`Game::race_menu_studio`).
    studio: bool,
}

impl CreatorMenu {
    pub fn is_open(&self) -> bool {
        self.model.is_some()
    }

    /// Open and drawn in a studio instead of the room (docs/spec-notes/
    /// character-creator.md §5, "The studio").
    pub fn in_studio(&self) -> bool {
        self.is_open() && self.studio
    }
}

#[derive(Component)]
struct CreatorRoot;

/// What a drag on the picture does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CameraDrag {
    Orbit,
    Pan,
}

/// The camera's distance now (for how far a pixel of panning goes).
fn control_distance(m: &mut Model) -> f32 {
    m.camera().distance
}

/// A change asked for.
#[derive(Debug, Clone, PartialEq)]
enum Op {
    Tab(Tab),
    StepTab(i32),
    StepRow(i32),
    Select(usize),
    /// The pointer over a bottom button.
    HoverAction(usize),
    /// The wheel: scroll the rows by so many (the chosen row stays).
    Scroll(i32),
    /// The scrollbar: the first row shown.
    ScrollTo(usize),
    Adjust(i32, bool),
    /// Change row `i` by a step.
    AdjustRow(usize, i32),
    Track(usize, f64),
    Swatch(usize),
    Act(Action),
    /// Enter: the chosen button, or Done.
    Enter,
    /// Test aid: change a row by label.
    Named(String, i32),
    SetNamed(String, f64),
    /// The camera: turn and tilt (radians), pan (game units right and up), zoom (factor),
    /// back to the tab's framing.
    Orbit(f32, f32),
    Pan(f32, f32),
    Zoom(f32),
    ViewReset,
}

#[derive(Resource, Default)]
struct CreatorAid(Vec<(f32, Op)>);

fn parse_aid(s: &str) -> Result<Vec<(f32, Op)>, String> {
    let mut out = Vec::new();
    for item in s.split(',').map(str::trim).filter(|i| !i.is_empty()) {
        let (t, rest) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <seconds>:<op>"))?;
        let t: f32 = t.trim().parse().map_err(|_| format!("{item:?}: bad seconds"))?;
        let op = match rest.split_once(':') {
            Some(("tab", name)) => Op::Tab(*TABS.iter().find(|x| x.title().eq_ignore_ascii_case(name)).ok_or_else(|| format!("{item:?}: no tab {name}"))?),
            Some(("next", row)) => Op::Named(row.to_string(), 1),
            Some(("prev", row)) => Op::Named(row.to_string(), -1),
            Some(("set", rest)) => {
                let (row, f) = rest.rsplit_once(':').ok_or_else(|| format!("{item:?}: expected set:<row>:<0-1>"))?;
                Op::SetNamed(row.to_string(), f.parse().map_err(|_| format!("{item:?}: bad fraction"))?)
            }
            Some(("orbit", rest)) | Some(("pan", rest)) => {
                let (a, b) = rest.split_once(':').ok_or_else(|| format!("{item:?}: expected two numbers"))?;
                let (a, b): (f32, f32) = (a.parse().map_err(|_| format!("{item:?}: bad number"))?, b.parse().map_err(|_| format!("{item:?}: bad number"))?);
                if rest_is(item, "orbit") {
                    Op::Orbit(a.to_radians(), b.to_radians())
                } else {
                    Op::Pan(a, b)
                }
            }
            Some(("zoom", f)) => Op::Zoom(f.parse().map_err(|_| format!("{item:?}: bad factor"))?),
            None if rest == "view" => Op::ViewReset,
            None if rest == "random" => Op::Act(Action::Randomise),
            None if rest == "undo" => Op::Act(Action::Undo),
            None if rest == "reset" => Op::Act(Action::Reset),
            None if rest == "done" => Op::Act(Action::Done),
            _ => return Err(format!("{item:?}: unknown step")),
        };
        out.push((t, op));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(out)
}

/// Whether a test aid item's step is `name` (`<seconds>:<name>:...`).
fn rest_is(item: &str, name: &str) -> bool {
    item.split(':').nth(1).is_some_and(|n| n.trim() == name)
}

pub struct CreatorPlugin;

impl Plugin for CreatorPlugin {
    fn build(&self, app: &mut App) {
        let aid = match std::env::var("NVF_PLAY_TEST_CREATOR") {
            Ok(v) => parse_aid(&v).unwrap_or_else(|e| {
                warn!("NVF_PLAY_TEST_CREATOR: {e}");
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        app.init_resource::<CreatorMenu>()
            .insert_resource(CreatorAid(aid))
            .add_systems(Update, (sync, input, write_look, camera, draw).chain().in_set(PlaySet::Ui))
            .add_systems(Startup, say_data);
    }
}

/// What the creator has to offer, once (smoke tests read it).
fn say_data(data: Option<Res<CreatorData>>) {
    match data {
        Some(d) => info!(
            "creator: data: {} races, {} sliders, {} authored faces{}",
            d.0.catalogue.races.len(),
            d.0.sliders[0].len(),
            d.0.catalogue.faces.len(),
            d.0.ctl_problem.as_deref().map_or(String::new(), |p| format!(" ({p})"))
        ),
        None => info!("creator: no data; the race menu stays the placeholder"),
    }
}

/// The label a row is found by (the test aid): the view's label, or a slider's file label.
fn row_label(m: &Model, r: Row) -> Vec<String> {
    let mut out = vec![match m.view(r) {
        RowView::Choice { label, .. } | RowView::Slider { label, .. } | RowView::Swatches { label, .. } => label,
    }];
    if let Row::Slider(i) = r {
        if let Some(s) = m.sliders().get(i) {
            out.push(s.label.clone());
        }
    }
    out
}

fn sync(mut st: ResMut<CreatorMenu>, data: Option<Res<CreatorData>>, script: Option<Res<ScriptState>>, mut control: ResMut<crate::control::PlayerControl>) {
    let open = script.as_deref().and_then(crate::chargen::open_code) == Some(menus::RACE) && data.is_some();
    if open == st.model.is_some() {
        return;
    }
    if !open {
        st.model = None;
        st.studio = false;
        st.answered = false;
        st.drag = None;
        st.dirty = true;
        control.portrait = None;
        info!("creator: closed");
        return;
    }
    let (Some(data), Some(script)) = (data, script) else { return };
    let Some(rt) = script.rt.as_ref() else { return };
    let start = match &rt.game.player.look {
        Some(l) => Some(actor_look(l)),
        None => data.0.player.clone(),
    }
    .or_else(|| {
        let r = data.0.catalogue.races.first()?;
        Some(fnv_actor::Look { race: r.id, female: false, hair: r.default_hair[0], hair_color: None, eyes: r.eyes[0].first().map(|e| e.id), head_parts: Vec::new(), facegen: fnv_actor::creator::full(&FaceGen::default()) })
    });
    let Some(start) = start else {
        warn!("creator: no race to start from; the placeholder stays");
        return;
    };
    let kind = rt.game.race_menu;
    let m = Model::of_kind(data.0.clone(), start, kind);
    if let Some(p) = &data.0.ctl_problem {
        warn!("creator: {p}; no sliders");
    }
    info!("creator: open on {} ({} sliders, {} races, {} beards, {} starting faces)", m.caption(), m.sliders().len(), data.0.catalogue.races.len(), data.0.catalogue.facial_hair.len(), data.0.catalogue.presets(m.look.race, m.look.female).len());
    match kind {
        model::RaceMenu::Barber => info!("creator: as the barber (ShowBarberMenu): the hair, its colour and the beard only"),
        model::RaceMenu::Surgeon => info!("creator: as the plastic surgeon (ShowPlasticSurgeonMenu): the face, eyes and brows only"),
        model::RaceMenu::Creator => {}
    }
    let mut m = m;
    let c = m.camera();
    let what = if kind == model::RaceMenu::Barber { "the hair" } else { "the face" };
    info!("creator: camera on opening: {:.0} units away, looking {:.0} under the eye ({what})", c.distance, c.drop);
    control.portrait = Some(c);
    st.studio = rt.game.race_menu_studio;
    if st.studio {
        info!("creator: in the studio (the gene projector)");
    }
    st.written = Some(m.look.clone());
    st.model = Some(m);
    st.answered = false;
    st.dirty = true;
}

/// The view's width in the interface's units.
fn view_width(window: &Window) -> f32 {
    960.0 * window.width() / window.height().max(1.0)
}

/// The pointer's place in the interface's units.
fn pointer(window: &Window) -> Option<Vec2> {
    // Where the pointer is on the menu as drawn at the UI scale (#1631).
    let p = crate::interface::from_menu_frame(window.cursor_position()?, Vec2::new(window.width(), window.height()), crate::interface::MIDDLE);
    let k = 960.0 / window.height().max(1.0);
    Some(p * k)
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut st: ResMut<CreatorMenu>,
    mut aid: ResMut<CreatorAid>,
    time: AidTime,
    real: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    windows: Query<&Window, With<PrimaryWindow>>,
    pads: Query<&Gamepad>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: Option<ResMut<SimState>>,
    mut sounds: MessageWriter<UiSound>,
) {
    let wheel: f32 = wheel.read().map(|w| if w.unit == MouseScrollUnit::Line { w.y } else { w.y / 40.0 }).sum();
    if st.model.is_none() || st.answered {
        return;
    }
    let mut ops: Vec<(Op, &str)> = Vec::new();
    // A world aid (`crate::pause::WorldAidClock`).
    let now = time.world_secs();
    while aid.0.first().is_some_and(|(t, _)| now.is_some_and(|now| *t <= now)) {
        let (_, op) = aid.0.remove(0);
        info!("test aid: creator {op:?}");
        ops.push((op, "test aid NVF_PLAY_TEST_CREATOR"));
    }
    use KeyCode::*;
    let shift = keys.any_pressed([ShiftLeft, ShiftRight]);
    for (k, op) in [
        ([KeyQ, PageUp], Op::StepTab(-1)),
        ([KeyE, PageDown], Op::StepTab(1)),
        ([Tab, Tab], Op::StepTab(if shift { -1 } else { 1 })),
        ([ArrowUp, KeyW], Op::StepRow(-1)),
        ([ArrowDown, KeyS], Op::StepRow(1)),
        ([ArrowLeft, KeyA], Op::Adjust(-1, shift)),
        ([ArrowRight, KeyD], Op::Adjust(1, shift)),
        ([Space, Space], Op::Act(Action::Randomise)),
        ([KeyZ, KeyZ], Op::Act(Action::Undo)),
        ([KeyR, KeyR], Op::Act(Action::Reset)),
        ([Enter, NumpadEnter], Op::Enter),
        ([KeyC, Home], Op::ViewReset),
    ] {
        if keys.any_just_pressed(k) {
            ops.push((op, "key"));
        }
    }
    // Held Left / Right keep moving a slider [design: after a third of a second].
    let held = |k: [KeyCode; 2]| keys.any_pressed(k) && !keys.any_just_pressed(k);
    let st_ref = &mut *st;
    let m = st_ref.model.as_mut().expect("open");
    let on_slider = matches!(m.chosen_row(), Some(Row::Slider(_)) | Some(Row::HairTone(_)));
    if on_slider && (held([ArrowLeft, KeyA]) || held([ArrowRight, KeyD])) {
        let by = if keys.any_pressed([ArrowLeft, KeyA]) { -1 } else { 1 };
        // Every frame, scaled to about 30 steps a second.
        if real.delta_secs() > 0.0 && (real.elapsed_secs() * 30.0) as i64 != ((real.elapsed_secs() - real.delta_secs()) * 30.0) as i64 {
            ops.push((Op::Adjust(by, shift), "key held"));
        }
    }
    // The controller's own buttons [design].
    for g in &pads {
        for (b, op) in [
            (GamepadButton::West, Op::Act(Action::Randomise)),
            (GamepadButton::North, Op::Act(Action::Undo)),
            (GamepadButton::Select, Op::Act(Action::Reset)),
        ] {
            if g.just_pressed(b) {
                ops.push((op, "controller"));
            }
        }
        if g.just_pressed(GamepadButton::RightThumb) {
            ops.push((Op::ViewReset, "controller"));
        }
        let dt = real.delta_secs();
        // The sticks [design: a full push turns 2 rad a second, tilts 1, pans 60 units].
        let dead = |v: Vec2| if v.length() > 0.2 { v } else { Vec2::ZERO };
        let right = dead(g.right_stick());
        if right != Vec2::ZERO {
            m.view.orbit(-right.x * 2.0 * dt, right.y * dt);
        }
        let left = dead(g.left_stick());
        if left != Vec2::ZERO {
            m.view.pan_by(-left.x * 60.0 * dt, -left.y * 60.0 * dt);
        }
        let zoom = g.get(GamepadButton::RightTrigger2).unwrap_or(0.0) - g.get(GamepadButton::LeftTrigger2).unwrap_or(0.0);
        if zoom.abs() > 0.1 {
            m.view.zoom_by(1.0 - zoom * dt);
        }
    }
    // The pointer.
    if let Some(p) = windows.single().ok().and_then(pointer) {
        let under = view::hit_at(&st_ref.hits, p);
        // A drag that starts on the picture moves the camera until the button is let go.
        let window_h = windows.single().map_or(800.0, |w| w.height().max(1.0));
        for b in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            if mouse.just_pressed(b) && under.is_none() {
                st_ref.camera_drag = Some(if b == MouseButton::Left && !shift { CameraDrag::Orbit } else { CameraDrag::Pan });
                st_ref.camera_button = Some(b);
            }
        }
        if let (Some(kind), Some(b)) = (st_ref.camera_drag, st_ref.camera_button) {
            if !mouse.pressed(b) {
                st_ref.camera_drag = None;
                st_ref.camera_button = None;
            } else if motion.delta != Vec2::ZERO {
                let d = motion.delta;
                match kind {
                    // Across the whole picture's height: about half a turn [design].
                    CameraDrag::Orbit => m.view.orbit(-d.x / window_h * 3.0, d.y / window_h * 1.5),
                    // The picture follows the pointer: a pixel is as many game units
                    // as the camera's distance over the picture's height [design: about
                    // the field of view's spread at the point looked at].
                    CameraDrag::Pan => {
                        let k = control_distance(m) / window_h;
                        m.view.pan_by(d.x * k, -d.y * k);
                    }
                }
            }
        }
        // Hovering chooses the row or button under the pointer [owner 2026-10-23: "add
        // mouse hover for selecting options"], when the pointer moved or the rows
        // scrolled under it, and not while something is dragged.
        let moved = st_ref.last_pointer.is_some_and(|q| q.distance(p) > 0.5);
        st_ref.last_pointer = Some(p);
        let dragging = st_ref.drag.is_some() || st_ref.scroll_drag || st_ref.camera_drag.is_some();
        if (moved || st_ref.hover_after_scroll) && !dragging {
            st_ref.hover_after_scroll = false;
            match under {
                Some(HitWhat::Row(i) | HitWhat::Arrow(i, _) | HitWhat::Track(i) | HitWhat::Swatch(i, _)) if m.on_actions.is_some() || m.row() != i => ops.push((Op::Select(i), "hover")),
                Some(HitWhat::Action(i)) if m.on_actions != Some(i) => ops.push((Op::HoverAction(i), "hover")),
                _ => {}
            }
        }
        if mouse.just_pressed(MouseButton::Left) {
            match under {
                Some(HitWhat::Scrollbar) => st_ref.scroll_drag = true,
                Some(HitWhat::Tab(i)) => ops.push((Op::Tab(m.tabs()[i]), "click")),
                Some(HitWhat::Arrow(i, by)) => ops.push((Op::AdjustRow(i, by), "click")),
                Some(HitWhat::Track(i)) => {
                    st_ref.drag = Some(i);
                    ops.push((Op::Select(i), "click"));
                }
                Some(HitWhat::Swatch(i, k)) => {
                    ops.push((Op::Select(i), "click"));
                    ops.push((Op::Swatch(k), "click"));
                }
                Some(HitWhat::Row(i)) => ops.push((Op::Select(i), "click")),
                Some(HitWhat::Action(i)) => ops.push((Op::Act(ACTIONS[i]), "click")),
                _ => {}
            }
        }
        if st_ref.scroll_drag {
            if mouse.pressed(MouseButton::Left) {
                ops.push((Op::ScrollTo(view::first_at(&view::heights(m), p.y)), "drag"));
            } else {
                st_ref.scroll_drag = false;
            }
        }
        if let Some(i) = st_ref.drag {
            if mouse.pressed(MouseButton::Left) {
                let left = windows.single().map_or(0.0, |w| view::panel_left(view_width(w)));
                ops.push((Op::Track(i, view::track_fraction(p.x - left)), "drag"));
            } else {
                st_ref.drag = None;
            }
        }
        if wheel.abs() >= 1.0 {
            match under {
                // The wheel scrolls the rows only [owner 2026-10-23: "scroll should only
                // scroll the page items not the bottom bar"]; on the tabs or buttons it
                // does nothing.
                Some(HitWhat::Panel | HitWhat::Tab(_) | HitWhat::Action(_)) => {}
                Some(_) => ops.push((Op::Scroll(-wheel.round() as i32), "wheel")),
                // A notch: a tenth nearer or further.
                None => m.view.zoom_by(0.9f32.powf(wheel)),
            }
        }
    }
    for (op, how) in ops {
        let event = apply(st_ref, &mut script, &mut sim, op, how);
        if let Some(event) = event {
            sounds.write(UiSound { menu: UiMenu::Creator, event });
        }
        if st_ref.answered {
            break;
        }
    }
}

/// Apply one change; the sound it makes.
fn apply(st: &mut CreatorMenu, script: &mut Option<ResMut<ScriptState>>, sim: &mut Option<ResMut<SimState>>, op: Op, how: &str) -> Option<UiEvent> {
    let m = st.model.as_mut()?;
    let before = (m.tab, m.row(), m.on_actions, m.look.clone());
    let mut event = None;
    // The row's shown value changed although the look did not (a starting face the same
    // as the face already worn: the player record's is the first Caucasian man's): it is
    // logged as a change all the same.
    let mut row_changed = false;
    match op {
        Op::Orbit(..) | Op::Pan(..) | Op::Zoom(_) | Op::ViewReset => {
            match op {
                Op::Orbit(x, y) => m.view.orbit(x, y),
                Op::Pan(x, y) => m.view.pan_by(x, y),
                Op::Zoom(f) => m.view.zoom_by(f),
                _ => m.view = model::View { turn: 0.0, ..model::View::default() },
            }
            let c = m.camera();
            info!(
                "creator: camera ({how}): turned {:.0}°, tilted {:.0}°, {:.0} units away, looking {:.0} under the eye, panned ({:.0}, {:.0})",
                c.yaw.to_degrees(),
                c.pitch.to_degrees(),
                c.distance,
                c.drop,
                m.view.pan[0],
                m.view.pan[1]
            );
            return None;
        }
        Op::Tab(t) => {
            if m.set_tab(t) {
                event = Some(UiEvent::Switch);
            }
        }
        Op::StepTab(by) => {
            m.step_tab(by);
            event = Some(UiEvent::Switch);
        }
        Op::StepRow(by) => {
            m.step_row(by);
        }
        Op::Select(i) => {
            m.select_row(i);
        }
        Op::HoverAction(i) => {
            m.on_actions = Some(i.min(ACTIONS.len() - 1));
        }
        Op::Scroll(_) | Op::ScrollTo(_) => {
            let heights = view::heights(m);
            let first = match op {
                Op::Scroll(by) => (m.scroll() as i64 + i64::from(by)).max(0) as usize,
                Op::ScrollTo(first) => first,
                _ => unreachable!(),
            };
            let first = first.min(view::max_first(&heights));
            if first != m.scroll() {
                m.set_scroll(first);
                st.hover_after_scroll = true;
                st.dirty = true;
            }
            return None;
        }
        Op::Adjust(by, fine) => {
            let shown = m.chosen_row().map(|r| m.view(r));
            if m.adjust(by, fine) && m.on_actions.is_none() {
                event = Some(UiEvent::Value { up: by > 0 });
            }
            row_changed = shown != m.chosen_row().map(|r| m.view(r));
        }
        Op::AdjustRow(i, by) => {
            m.select_row(i);
            let shown = m.chosen_row().map(|r| m.view(r));
            if m.adjust(by, false) {
                event = Some(UiEvent::Value { up: by > 0 });
            }
            row_changed = shown != m.chosen_row().map(|r| m.view(r));
        }
        Op::Track(i, f) => {
            m.select_row(i);
            if let Some(Row::Slider(k)) = m.chosen_row() {
                m.set_slider(k, f);
            } else if let Some(Row::HairTone(_)) = m.chosen_row() {
                // A tone row has no range: the drag steps it towards the pointer.
                let now = match m.view(m.chosen_row().expect("row")) {
                    RowView::Slider { fraction, .. } => f64::from(fraction),
                    _ => f,
                };
                if (f - now).abs() > 0.02 {
                    m.adjust(if f > now { 1 } else { -1 }, true);
                }
            }
        }
        Op::Swatch(k) => {
            if m.set_swatch(k) {
                event = Some(UiEvent::Picked);
            }
        }
        Op::Enter => match m.on_actions {
            Some(a) => return apply(st, script, sim, Op::Act(ACTIONS[a]), how),
            None => m.on_actions = Some(ACTIONS.len() - 1),
        },
        Op::Act(Action::Randomise) => {
            let mut rng = <rand_chacha::ChaCha8Rng as rand_core::SeedableRng>::seed_from_u64(fastrand_seed());
            m.randomise(&mut rng);
            info!("creator: randomised ({how}): {}", describe(m));
            event = Some(UiEvent::Picked);
        }
        Op::Act(Action::Undo) => {
            if m.undo() {
                info!("creator: undone ({how})");
                event = Some(UiEvent::Reset);
            } else {
                event = Some(UiEvent::Refused);
            }
        }
        Op::Act(Action::Reset) => {
            if m.reset() {
                info!("creator: reset ({how})");
            }
            event = Some(UiEvent::Reset);
        }
        Op::Act(Action::Done) => {
            let look = game_look(&m.look);
            let said = describe(m);
            let (Some(script), Some(sim)) = (script.as_deref_mut(), sim.as_deref_mut()) else { return None };
            if crate::chargen::finish(script, sim, menus::RACE, how, |g| rules::apply_look(g, look)) {
                info!("creator: done ({how}): {said}");
                st.answered = true;
                st.dirty = true;
                return Some(UiEvent::Accepted);
            }
            return None;
        }
        Op::Named(label, by) => {
            let Some((t, i)) = find_row(m, &label) else {
                warn!("test aid: creator: no row {label:?}");
                return None;
            };
            m.set_tab(t);
            m.select_row(i);
            let shown = m.chosen_row().map(|r| m.view(r));
            m.adjust(by, false);
            row_changed = shown != m.chosen_row().map(|r| m.view(r));
        }
        Op::SetNamed(label, f) => {
            let Some((t, i)) = find_row(m, &label) else {
                warn!("test aid: creator: no row {label:?}");
                return None;
            };
            m.set_tab(t);
            m.select_row(i);
            if let Some(Row::Slider(k)) = m.chosen_row() {
                m.set_slider(k, f);
            }
        }
    }
    if (before.0, before.1) != (m.tab, m.row()) || before.2 != m.on_actions {
        // The chosen row moved (keys, a click, a test aid): scroll it into view.
        if m.on_actions.is_none() {
            let first = view::reveal(&view::heights(m), m.scroll(), m.row());
            m.set_scroll(first);
        }
    }
    let after = (m.tab, m.row(), m.on_actions, &m.look);
    if (before.0, before.1, before.2) != (after.0, after.1, after.2) && event.is_none() {
        event = Some(UiEvent::Focus);
    }
    if before.3 != *after.3 || row_changed {
        if let Some(r) = m.chosen_row() {
            let shown = match m.view(r) {
                RowView::Choice { label, value, .. } => format!("{label} {value}"),
                RowView::Slider { label, value, .. } => format!("{label} {value}"),
                RowView::Swatches { label, .. } => format!("{label} {:?}", m.look.hair_color),
            };
            if how != "drag" && how != "key held" {
                info!("creator: {shown} ({how})");
            }
        }
    }
    st.dirty = true;
    event
}

/// A row by label on any tab (and any group of it): its tab and index.
fn find_row(m: &mut Model, label: &str) -> Option<(Tab, usize)> {
    let keep = (m.tab, m.on_actions);
    for &t in m.tabs() {
        m.set_tab(t);
        let groups = m.groups(t).len().max(1);
        for _ in 0..groups {
            let rows = m.rows();
            if let Some(i) = rows.iter().position(|&r| row_label(m, r).iter().any(|l| l.eq_ignore_ascii_case(label))) {
                return Some((t, i));
            }
            if let Some(g) = rows.iter().position(|&r| r == Row::Group) {
                m.adjust_row(rows[g], 1, false);
            }
        }
    }
    m.set_tab(keep.0);
    m.on_actions = keep.1;
    None
}

fn fastrand_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64)
}

/// The look in one line, for the log.
fn describe(m: &Model) -> String {
    let rows = [Row::Hair, Row::Eyes, Row::FacialHair, Row::Brows];
    let parts: Vec<String> = rows
        .iter()
        .filter_map(|&r| match m.view(r) {
            RowView::Choice { label, value, .. } => Some(format!("{} {value}", label.to_lowercase())),
            _ => None,
        })
        .collect();
    format!("{}, {}, hair colour {:?}", m.caption(), parts.join(", "), m.look.hair_color)
}

/// Write the look into the game when it changed.
fn write_look(mut st: ResMut<CreatorMenu>, mut script: Option<ResMut<ScriptState>>) {
    let Some(m) = st.model.as_ref() else { return };
    if st.answered || st.written.as_ref() == Some(&m.look) {
        return;
    }
    let look = m.look.clone();
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    rules::apply_look(&mut rt.game, game_look(&look));
    st.written = Some(look);
}

/// Ease the camera onto the framing [design: a tenth of a second, so a drag follows the
/// pointer closely and a tab's new framing still glides].
fn camera(mut st: ResMut<CreatorMenu>, real: Res<Time<Real>>, mut control: ResMut<crate::control::PlayerControl>) {
    let Some(m) = st.model.as_mut() else { return };
    let want = m.camera();
    let k = 1.0 - (-real.delta_secs() / 0.1).exp();
    let now = control.portrait.unwrap_or(want);
    let mix = |a: f32, b: f32| a + (b - a) * k;
    // The turn goes the short way round.
    let dyaw = (want.yaw - now.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    control.portrait = Some(crate::control::Portrait {
        yaw: now.yaw + dyaw * k,
        distance: mix(now.distance, want.distance),
        drop: mix(now.drop, want.drop),
        pitch: mix(now.pitch, want.pitch),
        shift: mix(now.shift, want.shift),
    });
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    mut st: ResMut<CreatorMenu>,
    roots: Query<Entity, With<CreatorRoot>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let view_w = windows.single().map_or(1707.0, view_width);
    if !st.dirty && st.drawn_w == view_w {
        return;
    }
    st.dirty = false;
    st.drawn_w = view_w;
    for e in &roots {
        commands.entity(e).despawn();
    }
    let st = st.into_inner();
    let Some(m) = st.model.as_ref().filter(|_| !st.answered) else {
        st.hits.clear();
        return;
    };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let kit = st.kit.get_or_insert_with(|| view::Kit::load(data.as_deref(), &mut images, bc));
    let root = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    let mut hits = Vec::new();
    // At the UI scale the menu keeps to the view's middle (#1631).
    commands.spawn((crate::interface::framed_root(root, crate::interface::MIDDLE), GlobalZIndex(30), CreatorRoot)).with_children(|r| hits = view::spawn(r, m, kit, view_w));
    st.hits = hits;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_aid_reads() {
        let steps = parse_aid("2:tab:face, 1:next:Hair style,3:set:Nose - short / long:0.8,4:random,5:done").unwrap();
        assert_eq!(steps[0], (1.0, Op::Named("Hair style".into(), 1)));
        assert_eq!(steps[1], (2.0, Op::Tab(Tab::Face)));
        assert_eq!(steps[2], (3.0, Op::SetNamed("Nose - short / long".into(), 0.8)));
        assert_eq!(steps[4].1, Op::Act(Action::Done));
        assert!(parse_aid("1:fly").is_err() && parse_aid("1:tab:nowhere").is_err());
        let cam = parse_aid("1:orbit:90:-10,2:pan:5:-2,3:zoom:0.5,4:view").unwrap();
        assert_eq!(cam[0].1, Op::Orbit(90f32.to_radians(), (-10f32).to_radians()));
        assert_eq!(cam[1].1, Op::Pan(5.0, -2.0));
        assert_eq!((cam[2].1.clone(), cam[3].1.clone()), (Op::Zoom(0.5), Op::ViewReset));
    }

    #[test]
    fn looks_convert_both_ways() {
        let l = rules::Look {
            race: fnv_sim::FormId(0x19),
            female: true,
            hair: None,
            hair_color: Some([1, 2, 3]),
            eyes: Some(fnv_sim::FormId(4)),
            head_parts: vec![fnv_sim::FormId(5)],
            fggs: vec![0.5; 50],
            fgga: vec![0.0; 30],
            fgts: vec![-1.25; 50],
        };
        assert_eq!(game_look(&actor_look(&l)), l);
    }

    #[test]
    fn only_the_race_menu_with_data() {
        assert!(draws(menus::RACE, true) && !draws(menus::RACE, false) && !draws(menus::NAME, true));
        // The menus scripts open stay script menus, so Done and the scripts can close
        // them: no engine menu may take one of their codes (the roulette table once had
        // the race menu's 1036).
        for m in [menus::MESSAGE, menus::RACE, menus::NAME, menus::TAG_SKILLS, menus::LOVE_TESTER, menus::TRAITS] {
            assert!(crate::script::is_script_menu(m), "{m}");
        }
    }
}
