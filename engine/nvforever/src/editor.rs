//! The editor (`--editor`): the free-camera world with panels over it. Select, move, rotate,
//! scale, delete, duplicate and place objects, with undo, and save them to the plugin that
//! `fnv_world::edit_plugin` writes in the home's mods folder. The game folder stays as it is.
//!
//! The editor is the game with its world held (`nvf_play::free_cam::FreeCam::editor_hold`).
//! Play in editor lets go of the hold and drops the player at the camera or a chosen spot,
//! with no loading screen; F8 takes the hold back and returns the camera. Edits reach the
//! simulation as they are made, unsaved ones too: what stands in the plugins moves through
//! the pose a script's SetPos would set (`nvf_play::moved` draws it, physics follows), so
//! the player collides with what the viewport shows. Going out puts back what play moved
//! among placed objects; the rest of the game's state (actors, quests, the player's items)
//! carries over to the next play until the editor starts again.

use crate::fly::FlyCam;
use crate::world::{Streamer, WorldData, WorldRoot};
use bevy::camera::primitives::Aabb;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::window::WindowCloseRequested;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext};
use fnv_esm::raw::visit_records;
use fnv_world::edit_plugin::{self, Edits, NewRef, NewStatic, Pose, RefChange, EDIT_PLUGIN};
use fnv_world::grass::height_at;
use fnv_world::land::{load_land, LandParts};
use fnv_world::placed::{base_object, placement};
use fnv_world::transform::ref_rotation;
use fnv_world::{cell_of, CELL_SIZE};
use nvf_assets::model::ModelCtx;
use nvf_assets::GameData;
use nvf_play::activate::pick::ray_box;
use fnv_sim::FormId;
use nvf_play::control::PlayerControl;
use nvf_play::free_cam::FreeCam;
use nvf_play::phys::PhysState;
use nvf_play::script::ScriptState;
use nvf_play::sim::SimState;
use nvf_play::{space, DropPlayer, Location, PlayState, RefChanged, RefRepose, RefVisual, Travel};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

/// New object names worked out in one frame, so a newly loaded cell does not stall a frame.
const NAMES_PER_FRAME: usize = 200;
/// A ray entry nearer than this (game units) is the camera inside the box, not a click on it.
const MIN_PICK_DISTANCE: f32 = 1.0;
const ROW_HEIGHT: f32 = 18.0;
/// The gizmo's length as a share of its distance from the camera: a constant size on screen.
const GIZMO_SCREEN_SHARE: f32 = 0.18;
/// How near the pointer must be to a gizmo line, in pixels.
const GIZMO_GRAB_PIXELS: f32 = 10.0;
const RING_SEGMENTS: usize = 64;
const MIN_SCALE: f32 = 0.01;
const MAX_SCALE: f32 = 10.0;
const SCALE_SNAP: f32 = 0.1;
/// How far in front of the camera an object lands when the pointer's ray meets nothing.
const PLACE_FALLBACK_DISTANCE: f32 = 800.0;
const DUPLICATE_OFFSET: f32 = 64.0;
const AXES: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];
/// The ray's step over the landscape: under its 128-unit grid spacing, game units.
const GROUND_STEP: f32 = 64.0;
const GROUND_REACH: f32 = 60000.0;
const GROUND_REFINE_STEPS: usize = 8;
/// The height above an interior's objects' middle at which the camera enters it.
const INTERIOR_CAMERA_LIFT: f32 = 100.0;
/// A right click that moved less than this many pixels opens the menu, not the look.
const RIGHT_CLICK_SLOP: f32 = 4.0;
/// The player is dropped on what lies within this distance under a chosen spot.
const DROP_ON_SPOT_UP: f32 = 60.0;
const DROP_ON_SPOT_DOWN: f32 = 1000.0;
/// And on what lies under the camera, from the camera down.
const DROP_UNDER_CAMERA: f32 = 100_000.0;
/// Frames that going back to the editor waits for an open menu to close.
const LEAVE_WAIT_FRAMES: u32 = 30;
/// `NVF_EDITOR_TEST_PLAY=<in>,<out>`: seconds after the start at which the player goes in
/// and comes back out. The check that needs no one at the mouse (tools/smoke).
const TEST_AID: &str = "NVF_EDITOR_TEST_PLAY";

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .insert_resource(EguiGlobalSettings { auto_create_primary_context: false, ..default() })
            .init_resource::<EditorState>()
            .add_systems(Startup, (attach_ui_to_camera.after(crate::setup), list_assets, draw_gizmos_on_top, start_lists, hold_world_at_start))
            .add_systems(
                Update,
                (
                    (shortcuts, gizmo_drag.after(shortcuts), click_to_select.after(gizmo_drag), place_objects.after(click_to_select), focus_selection, draw_selection, draw_gizmo, drop_to_ground, change_space, right_click, enter_play).run_if(editing),
                    (leave_play, follow_player_space, placed_collision).run_if(playing),
                    (sync_edits.after(place_objects), save, close_request, play_test_aid),
                ),
            )
            .add_systems(EguiPrimaryContextPass, panels);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum AssetKind {
    #[default]
    Meshes,
    Textures,
    Sounds,
}

impl AssetKind {
    const ALL: [(AssetKind, &'static str); 3] = [(AssetKind::Meshes, "Meshes"), (AssetKind::Textures, "Textures"), (AssetKind::Sounds, "Sounds")];

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tool {
    #[default]
    Move,
    Rotate,
    Scale,
}

struct ObjectName {
    label: String,
    lower: String,
}

/// One object's state in this session.
#[derive(Clone, Copy)]
struct Edit {
    /// What the entities that streaming spawns show: the load order's pose.
    original: Pose,
    current: Pose,
    deleted: bool,
    /// Whether the load order has it deleted: the saved plugin does, from an earlier session.
    loaded_deleted: bool,
}

/// An undo step: an object's pose and deleted state before and after.
struct Step {
    id: u32,
    before: (Pose, bool),
    after: (Pose, bool),
}

struct Drag {
    axis: usize,
    start: Pose,
    /// Where the pointer's ray was on the axis (move, scale) or round it (rotate).
    grabbed: f32,
    /// The gizmo's length when the drag began, so that a scale drag has a unit.
    size: f32,
}

enum PlaceRequest {
    Asset { path: String, cursor: Vec2 },
    Duplicate(u32),
    /// Bring back an object deleted by an earlier session: streaming does not spawn it.
    Restore(u32),
}

/// Where the player goes in.
#[derive(Clone, Copy)]
enum PlayFrom {
    /// On what lies under the camera, looking where it looks.
    Camera,
    /// On a spot in the world (game space).
    Spot([f32; 3]),
    /// Where the player was left.
    Resume,
}

/// What going back to the editor gives back.
struct BeforePlay {
    camera: Transform,
    yaw: f32,
    pitch: f32,
    space: Option<u32>,
    world: u32,
}

enum SpaceRequest {
    Interior(u32),
    /// Back to the worldspace, at this game position or where the camera left it.
    Exterior(Option<[f32; 3]>),
}

/// An object the editor spawned, with the interior it stands in. Streaming does not unload
/// it when the space changes, so the editor hides it.
#[derive(Component)]
struct Placed(Option<u32>);

/// The entity shows this pose, which is not the one streaming spawned it with.
#[derive(Component)]
struct Applied(Pose);

#[derive(Resource, Default)]
pub struct EditorState {
    selected: Option<u32>,
    /// A frame-the-selection request from the panel's button or the F key.
    focus: bool,
    names: HashMap<u32, ObjectName>,
    object_filter: String,
    assets: [Vec<String>; 3],
    asset_kind: AssetKind,
    asset_filter: String,
    /// Rows of `assets[asset_kind]` that pass `asset_filter`, as indices.
    asset_rows: Vec<usize>,
    assets_dirty: bool,
    asset_selected: Option<String>,
    goto: (i32, i32),
    /// Last frame's answer from the UI: the viewport does not take this input.
    pointer_over_ui: bool,
    typing: bool,

    tool: Tool,
    snap: bool,
    snap_move: f32,
    snap_degrees: f32,
    drag: Option<Drag>,
    /// The gizmo took this frame's click, so it does not select.
    click_taken: bool,

    edits: HashMap<u32, Edit>,
    /// Objects that the session made, with their base forms.
    new_refs: HashMap<u32, u32>,
    /// The interiors of the objects that the session placed or brought back.
    placed_in: HashMap<u32, Option<u32>>,
    /// Objects that the saved plugin deletes, when the session started.
    deleted_by_plugin: Vec<u32>,
    /// Interior cells by name.
    interiors: Vec<(u32, String)>,
    interior_filter: String,
    /// The interior the camera is in.
    space: Option<u32>,
    space_request: Option<SpaceRequest>,
    exterior_camera: Option<Transform>,
    /// Landscape heights by grid square, read when first needed.
    land: HashMap<(i32, i32), Option<Vec<f32>>>,
    drop_requested: bool,
    /// The window's close button was pressed with unsaved changes.
    close_prompt: bool,
    quit_after_save: bool,
    new_statics: Vec<NewStatic>,
    next_object_id: u32,
    /// Base forms by model path (`fnv_vfs::normalize`, under `meshes\`), read when first needed.
    model_bases: Option<HashMap<String, u32>>,
    /// The state from before the change that is in progress.
    pending: Option<(u32, (Pose, bool))>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Objects whose entities must show their edit.
    sync: HashSet<u32>,
    place: Option<PlaceRequest>,
    /// The player is in the world and the editor stands aside.
    playing: bool,
    /// The player has been in the world, so there is somewhere to resume from.
    played: bool,
    play_request: Option<PlayFrom>,
    /// Going back to the editor: frames spent waiting for a menu to close.
    leaving: Option<u32>,
    before_play: Option<BeforePlay>,
    /// Where a right click began, until it moves too far to be a click.
    right_press: Option<Vec2>,
    /// The right-click menu: where it stands on screen and the spot in the world.
    drop_menu: Option<(Vec2, [f32; 3])>,
    /// The models of the objects the plugins do not hold.
    placed_models: HashMap<u32, String>,
    /// The collision built for them: at which pose, and whether it had any bodies.
    placed_built: HashMap<u32, (Pose, bool)>,
    unsaved: bool,
    save_requested: bool,
    status: String,
}

/// Whether the fly camera may read its input: as in Unreal, while the right mouse button is
/// down. The game's other modes have no `EditorState`.
pub fn camera_may_move(state: Option<Res<EditorState>>, buttons: Res<ButtonInput<MouseButton>>) -> bool {
    state.is_none_or(|s| !s.playing && !s.typing && buttons.pressed(MouseButton::Right))
}

fn editing(state: Res<EditorState>) -> bool {
    !state.playing
}

fn playing(state: Res<EditorState>) -> bool {
    state.playing
}

fn hold_world_at_start(mut free: ResMut<FreeCam>, mut control: ResMut<PlayerControl>) {
    free.editor_hold(true, &mut control);
}

/// Drop the player in and let the world go.
#[allow(clippy::too_many_arguments)]
fn enter_play(
    mut state: ResMut<EditorState>,
    mut free: ResMut<FreeCam>,
    mut control: ResMut<PlayerControl>,
    mut next: ResMut<NextState<PlayState>>,
    camera: Query<(&Transform, &FlyCam)>,
    location: Res<Location>,
    mut drop_player: MessageWriter<DropPlayer>,
    script: Option<ResMut<ScriptState>>,
) {
    let Some(from) = state.play_request.take() else { return };
    let Ok((t, cam)) = camera.single() else { return };
    state.commit();
    state.before_play = Some(BeforePlay { camera: *t, yaw: cam.yaw, pitch: cam.pitch, space: state.space, world: location.world });
    state.drop_menu = None;
    let look = (-cam.yaw, cam.pitch);
    match from {
        PlayFrom::Camera => {
            drop_player.write(DropPlayer { feet: space::to_game(t.translation), up: 0.0, down: DROP_UNDER_CAMERA, heading: look.0, pitch: look.1 });
        }
        PlayFrom::Spot(feet) => {
            drop_player.write(DropPlayer { feet, up: DROP_ON_SPOT_UP, down: DROP_ON_SPOT_DOWN, heading: look.0, pitch: 0.0 });
        }
        PlayFrom::Resume => {}
    }
    // Died on the last play: this one starts alive.
    if let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) {
        if rt.game.is_dead(FormId::PLAYER_REF) {
            rt.game.resurrect(FormId::PLAYER_REF, false);
        }
    }
    free.editor_hold(false, &mut control);
    next.set(PlayState::Playing);
    (state.playing, state.played, state.leaving) = (true, true, None);
    info!("editor: playing");
}

/// F8, or a load or the main menu asked for from the game's own menus: back to the editor.
#[allow(clippy::too_many_arguments)]
fn leave_play(
    keys: Res<ButtonInput<KeyCode>>,
    (mut loads, mut main_menu): (MessageReader<nvf_play::save::LoadRequest>, MessageReader<nvf_play::pause::MainMenuRequest>),
    (game_state, paused, menu, console): (Res<State<PlayState>>, Res<nvf_play::pause::Paused>, Res<nvf_play::menu::ActiveMenu>, Res<nvf_play::console::Console>),
    (mut free, mut control, mut next): (ResMut<FreeCam>, ResMut<PlayerControl>, ResMut<NextState<PlayState>>),
    mut camera: Query<(&mut Transform, &mut FlyCam)>,
    location: Res<Location>,
    (mut travel, mut repose): (MessageWriter<Travel>, MessageWriter<RefRepose>),
    mut sim: ResMut<SimState>,
    mut state: ResMut<EditorState>,
) {
    let asked = loads.read().count() + main_menu.read().count() > 0;
    if keys.just_pressed(KeyCode::F8) || asked {
        state.leaving = Some(state.leaving.unwrap_or(0));
    }
    let Some(waited) = state.leaving else { return };
    match *game_state.get() {
        PlayState::Loading => return,
        PlayState::Playing if !paused.0 && menu.0.is_none() && !console.open => {}
        _ if waited >= LEAVE_WAIT_FRAMES => {
            state.leaving = None;
            state.status = "Close the open menu, then press F8".to_string();
            warn!("editor: a menu stayed open; still playing");
            return;
        }
        _ => {
            next.set(PlayState::Playing);
            state.leaving = Some(waited + 1);
            return;
        }
    }
    free.editor_hold(true, &mut control);
    (state.playing, state.leaving) = (false, None);
    let Some(before) = state.before_play.take() else { return };
    if let Ok((mut t, mut cam)) = camera.single_mut() {
        *t = before.camera;
        (cam.yaw, cam.pitch) = (before.yaw, before.pitch);
    }
    if location.interior != before.space || location.world != before.world {
        let feet = space::to_game(before.camera.translation);
        travel.write(Travel { world: Some(before.world), interior: before.space, feet, heading_deg: 0.0 });
    }
    state.space = before.space;
    let placed: Vec<u32> = state.placed_in.keys().copied().collect();
    state.sync.extend(placed);
    // The viewport shows the level, not what play knocked over.
    let refs = sim.refs;
    let mut put_back = 0;
    for id in refs.ids() {
        let Some(placed) = refs.get(id).filter(|p| &p.sig != b"ACHR" && &p.sig != b"ACRE") else { continue };
        if sim.moved_pose(id.0).is_none() {
            continue;
        }
        let edit = state.edits.get(&id.0).map(|e| e.current);
        let (pos, rot) = edit.map_or((placed.pos, placed.rot), |p| (p.position, p.rotation));
        if let Ok(s) = sim.world.get(id) {
            if s.pos != pos || s.rot != rot {
                (s.pos, s.rot) = (pos, rot);
                repose.write(RefRepose { form_id: id.0 });
                put_back += 1;
            }
        }
    }
    info!("editor: back from playing, {put_back} moved objects put back");
}

/// Walking through a door takes the editor's idea of the space along, so that what it
/// placed shows only where it stands.
fn follow_player_space(location: Res<Location>, mut state: ResMut<EditorState>) {
    if location.interior != state.space {
        state.space = location.interior;
        let placed: Vec<u32> = state.placed_in.keys().copied().collect();
        state.sync.extend(placed);
    }
}

/// What the session placed is not in the plugins, so physics cannot read it from there.
fn placed_collision(data: Option<Res<GameData>>, location: Res<Location>, mut phys: ResMut<PhysState>, mut state: ResMut<EditorState>) {
    let Some(data) = data else { return };
    let state = &mut *state;
    for (id, path) in &state.placed_models {
        let Some(edit) = state.edits.get(id) else { continue };
        let wanted = !edit.deleted && state.placed_in.get(id).copied().flatten() == location.interior;
        match (wanted, state.placed_built.get(id).copied()) {
            (false, None) => {}
            (false, Some(_)) => {
                phys.set_model_collision(&data, *id, None);
                state.placed_built.remove(id);
            }
            // A travel emptied the physics world; a model with no bodies is not tried again.
            (true, Some((pose, built))) if pose == edit.current && (!built || phys.has_collision(*id)) => {}
            (true, _) => {
                let p = edit.current;
                let built = phys.set_model_collision(&data, *id, Some((path, p.position, p.rotation, p.scale)));
                state.placed_built.insert(*id, (p, built));
            }
        }
    }
}

/// A right click without a drag opens the menu that drops the player on the spot.
#[allow(clippy::too_many_arguments)]
fn right_click(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<FlyCam>>,
    root: Query<&GlobalTransform, With<WorldRoot>>,
    visuals: Query<Pickable>,
    meshes: Res<Assets<Mesh>>,
    world: Res<WorldData>,
    mut state: ResMut<EditorState>,
) {
    let Ok(window) = windows.single() else { return };
    let cursor = window.cursor_position();
    if mouse.just_pressed(MouseButton::Right) {
        state.right_press = cursor.filter(|_| !state.pointer_over_ui);
        state.drop_menu = None;
    }
    if let (Some(press), Some(now)) = (state.right_press, cursor) {
        if press.distance(now) > RIGHT_CLICK_SLOP {
            state.right_press = None;
        }
    }
    if !mouse.just_released(MouseButton::Right) {
        return;
    }
    let (Some(press), Ok((cam, cam_t)), Ok(root)) = (state.right_press.take(), camera.single(), root.single()) else { return };
    let Ok(ray) = cam.viewport_to_world(cam_t, press) else { return };
    let object = pick_object(ray.origin, *ray.direction, &visuals, &meshes).map(|(_, d)| d);
    let Some((origin, dir)) = pointer_ray_in_game(press, cam, cam_t, root) else { return };
    let ground = ground_distance(&mut state, &world, origin, dir);
    match [object, ground].into_iter().flatten().min_by(f32::total_cmp) {
        Some(distance) => state.drop_menu = Some((press, (origin + dir * distance).to_array())),
        None => state.status = "Nothing there to stand on".to_string(),
    }
}

/// `NVF_EDITOR_TEST_PLAY` ([`TEST_AID`]).
fn play_test_aid(time: Res<Time<Real>>, mut times: Local<Option<Vec<f32>>>, mut state: ResMut<EditorState>) {
    let times = times.get_or_insert_with(|| {
        let v = std::env::var(TEST_AID).unwrap_or_default();
        v.split(',').filter_map(|s| s.trim().parse().ok()).rev().collect()
    });
    if times.last().is_some_and(|t| time.elapsed_secs() >= *t) {
        times.pop();
        if state.playing {
            state.leaving = Some(0);
        } else {
            state.play_request = Some(PlayFrom::Camera);
        }
    }
}

/// The plugin's path, when the home has a place.
pub fn edit_plugin_path() -> Option<PathBuf> {
    Some(fnv_install::EngineHome::default_root()?.join("mods").join(EDIT_PLUGIN))
}

fn attach_ui_to_camera(mut commands: Commands, cameras: Query<Entity, With<FlyCam>>) {
    for camera in &cameras {
        commands.entity(camera).insert(PrimaryEguiContext);
    }
}

fn draw_gizmos_on_top(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<DefaultGizmoConfigGroup>();
    config.depth_bias = -1.0;
    config.line.width = 3.0;
}

fn start_lists(world: Res<WorldData>, streamer: Res<Streamer>, mut state: ResMut<EditorState>) {
    state.next_object_id = edit_plugin::first_free_object_id(world.lo);
    state.snap_move = 64.0;
    state.snap_degrees = 15.0;
    state.space = streamer.interior;
    let own = edit_plugin::own_index(world.lo);
    state.deleted_by_plugin = world
        .index
        .refs
        .keys()
        .copied()
        .filter(|id| world.lo.winner(*id).is_some_and(|(p, _)| own.is_some_and(|o| std::ptr::eq(p, &world.lo.plugins[o]))))
        .filter(|id| placement(world.lo, world.schema, *id).is_some_and(|p| p.deleted()))
        .collect();
    state.deleted_by_plugin.sort_unstable();
    state.interiors = world
        .index
        .cells
        .values()
        .filter(|c| c.world.is_none())
        .map(|c| (c.form_id, format!("{}  {:08X}", c.edid.as_deref().or(c.name.as_deref()).unwrap_or("(no name)"), c.form_id)))
        .collect();
    state.interiors.sort_by_key(|(_, name)| name.to_lowercase());
}

fn list_assets(game: Res<GameData>, mut state: ResMut<EditorState>) {
    let under = |dir: &'static str, exts: &'static [&'static str]| {
        game.vfs.paths(move |p| p.starts_with(dir) && exts.iter().any(|e| p.ends_with(e)))
    };
    state.assets = [under("meshes\\", &[".nif"]), under("textures\\", &[".dds"]), under("sound\\", &[".wav", ".ogg", ".mp3"])];
    state.assets_dirty = true;
    info!("editor: {} meshes, {} textures, {} sounds", state.assets[0].len(), state.assets[1].len(), state.assets[2].len());
}

fn pose_affine(p: &Pose) -> Affine3A {
    let m = Mat3::from_cols_array(&ref_rotation(p.rotation)).transpose(); // row-major
    Affine3A::from_mat3_translation(m * p.scale, Vec3::from_array(p.position))
}

fn loaded_pose(world: &WorldData, id: u32) -> Option<Pose> {
    placement(world.lo, world.schema, id).map(|p| Pose { position: p.position, rotation: p.rotation, scale: p.scale })
}

impl EditorState {
    fn pose(&self, world: &WorldData, id: u32) -> Option<Pose> {
        self.edits.get(&id).map(|e| e.current).or_else(|| loaded_pose(world, id))
    }

    /// Change an object. An undo step comes from [`Self::commit`], so a drag is one step.
    fn change(&mut self, world: &WorldData, id: u32, pose: Pose, deleted: bool) {
        let Some(original) = loaded_pose(world, id).or_else(|| self.edits.get(&id).map(|e| e.original)) else { return };
        let loaded_deleted = placement(world.lo, world.schema, id).is_some_and(|p| p.deleted());
        let edit = self.edits.entry(id).or_insert(Edit { original, current: original, deleted: loaded_deleted, loaded_deleted });
        let before = (edit.current, edit.deleted);
        edit.current = pose;
        edit.deleted = deleted;
        if self.pending.is_none() {
            self.pending = Some((id, before));
        }
        self.sync.insert(id);
    }

    fn commit(&mut self) {
        let Some((id, before)) = self.pending.take() else { return };
        let Some(edit) = self.edits.get(&id) else { return };
        let after = (edit.current, edit.deleted);
        if before != after {
            self.undo.push(Step { id, before, after });
            self.redo.clear();
            self.unsaved = true;
        }
    }

    fn restore(&mut self, id: u32, (pose, deleted): (Pose, bool)) {
        if let Some(edit) = self.edits.get_mut(&id) {
            edit.current = pose;
            edit.deleted = deleted;
            self.sync.insert(id);
            self.unsaved = true;
        }
    }

    fn undo(&mut self) {
        self.commit();
        if let Some(step) = self.undo.pop() {
            self.restore(step.id, step.before);
            self.selected = (!step.before.1).then_some(step.id);
            self.redo.push(step);
        }
    }

    fn redo(&mut self) {
        self.commit();
        if let Some(step) = self.redo.pop() {
            self.restore(step.id, step.after);
            self.selected = (!step.after.1).then_some(step.id);
            self.undo.push(step);
        }
    }

    fn delete_selected(&mut self, world: &WorldData) {
        let Some(id) = self.selected else { return };
        let Some(pose) = self.pose(world, id) else { return };
        self.change(world, id, pose, true);
        self.commit();
        self.selected = None;
    }

    /// The interior an object stands in.
    fn interior_of(&self, world: &WorldData, id: u32) -> Option<u32> {
        if let Some(space) = self.placed_in.get(&id) {
            return *space;
        }
        let cell = world.index.refs.get(&id)?.cell;
        world.index.cells.get(&cell).filter(|c| c.world.is_none()).map(|c| c.form_id)
    }

    /// The places whose navmesh and distant objects no longer match the objects: an interior,
    /// or a grid square of the worldspace.
    fn out_of_date_cells(&self, world: &WorldData) -> BTreeSet<(Option<u32>, (i32, i32))> {
        let mut places = BTreeSet::new();
        for (id, e) in &self.edits {
            if e.deleted == e.loaded_deleted && e.current == e.original && !self.new_refs.contains_key(id) {
                continue;
            }
            match self.interior_of(world, *id) {
                Some(cell) => {
                    places.insert((Some(cell), (0, 0)));
                }
                None => places.extend([e.original, e.current].map(|p| (None, cell_of(p.position[0], p.position[1])))),
            }
        }
        places
    }

    /// What a save writes.
    fn edits_to_save(&self, world: &WorldData) -> Edits {
        let mut edits = Edits { next_object_id: self.next_object_id, ..Edits::default() };
        for (id, e) in &self.edits {
            match self.new_refs.get(id) {
                Some(base) if !e.deleted => {
                    let interior = self.placed_in.get(id).copied().flatten();
                    edits.new_refs.push(NewRef { form_id: *id, base: *base, pose: e.current, world: world.world, interior });
                }
                Some(_) => {}
                None if e.deleted != e.loaded_deleted || e.current != e.original => {
                    edits.changes.insert(*id, RefChange { pose: e.current, deleted: e.deleted });
                }
                None => {}
            }
        }
        edits.new_refs.sort_by_key(|r| r.form_id);
        let used: HashSet<u32> = edits.new_refs.iter().map(|r| r.base).collect();
        edits.new_statics = self.new_statics.iter().filter(|s| used.contains(&s.form_id)).cloned().collect();
        edits
    }

    fn take_object_id(&mut self, world: &WorldData) -> u32 {
        let id = edit_plugin::new_form_id(world.lo, self.next_object_id);
        self.next_object_id += 1;
        id
    }
}

fn object_name(world: &WorldData, id: u32) -> ObjectName {
    let base = placement(world.lo, world.schema, id).and_then(|p| p.base).and_then(|b| base_object(world.lo, world.schema, b));
    let label = match base {
        Some(b) => format!("{}  [{}]  {id:08X}", b.edid.as_deref().unwrap_or("(no name)"), String::from_utf8_lossy(&b.sig)),
        None => format!("(no record)  {id:08X}"),
    };
    ObjectName { lower: label.to_lowercase(), label }
}

fn select(state: &mut EditorState, id: Option<u32>) {
    state.commit();
    state.selected = id;
}

fn shortcuts(keys: Res<ButtonInput<KeyCode>>, buttons: Res<ButtonInput<MouseButton>>, world: Res<WorldData>, mut state: ResMut<EditorState>) {
    if state.typing || buttons.pressed(MouseButton::Right) {
        return;
    }
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if ctrl {
        if keys.just_pressed(KeyCode::KeyZ) {
            if shift { state.redo() } else { state.undo() }
        }
        if keys.just_pressed(KeyCode::KeyY) {
            state.redo();
        }
        if keys.just_pressed(KeyCode::KeyS) {
            state.save_requested = true;
        }
        if keys.just_pressed(KeyCode::KeyD) {
            state.place = state.selected.map(PlaceRequest::Duplicate);
        }
        return;
    }
    for (key, tool) in [(KeyCode::KeyW, Tool::Move), (KeyCode::KeyE, Tool::Rotate), (KeyCode::KeyR, Tool::Scale)] {
        if keys.just_pressed(key) {
            state.tool = tool;
        }
    }
    if keys.just_pressed(KeyCode::KeyF) {
        state.focus = true;
    }
    if keys.just_pressed(KeyCode::F5) {
        state.play_request = Some(PlayFrom::Camera);
    }
    if keys.just_pressed(KeyCode::Delete) {
        state.delete_selected(&world);
    }
    if keys.just_pressed(KeyCode::Escape) {
        select(&mut state, None);
    }
}

/// The ray under the pointer in game space, as (origin, unit direction).
fn pointer_ray_in_game(cursor: Vec2, cam: &Camera, cam_t: &GlobalTransform, root: &GlobalTransform) -> Option<(Vec3, Vec3)> {
    let ray = cam.viewport_to_world(cam_t, cursor).ok()?;
    let to_game = root.affine().inverse();
    Some((to_game.transform_point3(ray.origin), to_game.transform_vector3(*ray.direction).normalize()))
}

/// How far along the line through `at` with unit direction `axis` the ray comes nearest.
fn nearest_on_axis(origin: Vec3, dir: Vec3, at: Vec3, axis: Vec3) -> Option<f32> {
    let along = axis.dot(dir);
    let denominator = 1.0 - along * along;
    if denominator < 1e-5 {
        return None;
    }
    let to_ray = origin - at;
    Some((axis.dot(to_ray) - along * dir.dot(to_ray)) / denominator)
}

/// The angle round `axis` at which the ray meets the plane through `at` normal to `axis`.
fn angle_round_axis(origin: Vec3, dir: Vec3, at: Vec3, axis: Vec3) -> Option<f32> {
    let facing = axis.dot(dir);
    if facing.abs() < 1e-4 {
        return None;
    }
    let distance = axis.dot(at - origin) / facing;
    if distance <= 0.0 {
        return None;
    }
    let on_plane = origin + dir * distance - at;
    let (u, v) = axis.any_orthonormal_pair();
    Some(on_plane.dot(v).atan2(on_plane.dot(u)))
}

fn rotated(rotation: [f32; 3], axis: Vec3, angle: f32) -> [f32; 3] {
    let start = Mat3::from_cols_array(&ref_rotation(rotation)).transpose();
    let turned = Mat3::from_axis_angle(axis, angle) * start;
    edit_plugin::rotation_from_matrix(turned.transpose().to_cols_array())
}

fn snapped(value: f32, step: f32) -> f32 {
    if step > 0.0 { (value / step).round() * step } else { value }
}

/// The gizmo's length in game units for a camera at `camera_game`.
fn gizmo_size(at: Vec3, camera_game: Vec3) -> f32 {
    (at - camera_game).length() * GIZMO_SCREEN_SHARE
}

fn ring_points(at: Vec3, axis: Vec3, size: f32) -> impl Iterator<Item = Vec3> {
    let (u, v) = axis.any_orthonormal_pair();
    (0..=RING_SEGMENTS).map(move |i| {
        let a = i as f32 / RING_SEGMENTS as f32 * std::f32::consts::TAU;
        at + (u * a.cos() + v * a.sin()) * size
    })
}

fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let along = b - a;
    let t = if along.length_squared() > 0.0 { ((p - a).dot(along) / along.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + along * t)
}

/// The axis whose handle lies under the pointer.
fn axis_under_pointer(cursor: Vec2, tool: Tool, at: Vec3, size: f32, cam: &Camera, cam_t: &GlobalTransform, root: &GlobalTransform) -> Option<usize> {
    let screen = |game: Vec3| cam.world_to_viewport(cam_t, root.affine().transform_point3(game)).ok();
    let mut best: Option<(usize, f32)> = None;
    for (i, axis) in AXES.iter().enumerate() {
        let points: Vec<Vec3> = match tool {
            Tool::Move | Tool::Scale => vec![at, at + *axis * size],
            Tool::Rotate => ring_points(at, *axis, size).collect(),
        };
        for pair in points.windows(2) {
            let (Some(a), Some(b)) = (screen(pair[0]), screen(pair[1])) else { continue };
            let d = distance_to_segment(cursor, a, b);
            if d < GIZMO_GRAB_PIXELS && best.is_none_or(|(_, nearest)| d < nearest) {
                best = Some((i, d));
            }
        }
    }
    best.map(|(i, _)| i)
}

#[allow(clippy::too_many_arguments)]
fn gizmo_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<FlyCam>>,
    root: Query<&GlobalTransform, With<WorldRoot>>,
    world: Res<WorldData>,
    mut state: ResMut<EditorState>,
) {
    state.click_taken = false;
    let Some(id) = state.selected else { return };
    let Some(pose) = state.pose(&world, id) else { return };
    let (Ok(window), Ok((cam, cam_t)), Ok(root)) = (windows.single(), camera.single(), root.single()) else { return };
    if state.drag.is_some() && !mouse.pressed(MouseButton::Left) {
        state.drag = None;
        state.commit();
        return;
    }
    let Some(cursor) = window.cursor_position() else { return };
    let Some((origin, dir)) = pointer_ray_in_game(cursor, cam, cam_t, root) else { return };
    let at = Vec3::from_array(pose.position);

    if let Some(drag) = &state.drag {
        let (axis_index, start, grabbed, size) = (drag.axis, drag.start, drag.grabbed, drag.size);
        let axis = AXES[axis_index];
        let start_at = Vec3::from_array(start.position);
        let mut next = start;
        match state.tool {
            Tool::Move => {
                let Some(t) = nearest_on_axis(origin, dir, start_at, axis) else { return };
                let mut moved = start_at + axis * (t - grabbed);
                if state.snap {
                    moved[axis_index] = snapped(moved[axis_index], state.snap_move);
                }
                next.position = moved.to_array();
            }
            Tool::Rotate => {
                let Some(angle) = angle_round_axis(origin, dir, start_at, axis) else { return };
                let mut turn = angle - grabbed;
                if state.snap {
                    turn = snapped(turn, state.snap_degrees.to_radians());
                }
                next.rotation = rotated(start.rotation, axis, turn);
            }
            Tool::Scale => {
                let Some(t) = nearest_on_axis(origin, dir, start_at, axis) else { return };
                let mut scale = start.scale * (1.0 + (t - grabbed) / size);
                if state.snap {
                    scale = snapped(scale, SCALE_SNAP);
                }
                next.scale = scale.clamp(MIN_SCALE, MAX_SCALE);
            }
        }
        state.change(&world, id, next, false);
        return;
    }

    if !mouse.just_pressed(MouseButton::Left) || state.pointer_over_ui {
        return;
    }
    let camera_game = root.affine().inverse().transform_point3(cam_t.translation());
    let size = gizmo_size(at, camera_game);
    let Some(axis_index) = axis_under_pointer(cursor, state.tool, at, size, cam, cam_t, root) else { return };
    let axis = AXES[axis_index];
    let grabbed = match state.tool {
        Tool::Move | Tool::Scale => nearest_on_axis(origin, dir, at, axis),
        Tool::Rotate => angle_round_axis(origin, dir, at, axis),
    };
    if let Some(grabbed) = grabbed {
        state.commit();
        state.drag = Some(Drag { axis: axis_index, start: pose, grabbed, size });
        state.click_taken = true;
    }
}

type Pickable<'a> = (&'a RefVisual, &'a Aabb, &'a GlobalTransform, &'a InheritedVisibility, Option<&'a Mesh3d>, Has<SkinnedMesh>);

/// The nearest hit of the ray with a mesh's triangles, as a distance along `dir`.
fn ray_mesh(mesh: &Mesh, origin: Vec3, dir: Vec3) -> Option<f32> {
    let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)? else { return None };
    let corner = |i: usize| positions.get(i).map(|p| Vec3::from_array(*p));
    let triangle = |a: usize, b: usize, c: usize| Some((corner(a)?, corner(b)?, corner(c)?));
    let hit = |t: Option<(Vec3, Vec3, Vec3)>| t.and_then(|(a, b, c)| ray_triangle(origin, dir, a, b, c));
    let nearest = |best: Option<f32>, d: Option<f32>| match (best, d) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    };
    match mesh.indices() {
        Some(Indices::U16(i)) => i.as_chunks::<3>().0.iter().map(|t| hit(triangle(t[0] as usize, t[1] as usize, t[2] as usize))).fold(None, nearest),
        Some(Indices::U32(i)) => i.as_chunks::<3>().0.iter().map(|t| hit(triangle(t[0] as usize, t[1] as usize, t[2] as usize))).fold(None, nearest),
        None => (0..positions.len() / 3).map(|t| hit(triangle(t * 3, t * 3 + 1, t * 3 + 2))).fold(None, nearest),
    }
}

/// Moeller-Trumbore, from both sides.
fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (ab, ac) = (b - a, c - a);
    let p = dir.cross(ac);
    let det = ab.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let to_origin = origin - a;
    let u = to_origin.dot(p) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = to_origin.cross(ab);
    let v = dir.dot(q) / det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(ac.dot(q) / det).filter(|t| *t > 0.0)
}

/// The nearest object on the ray: by its triangles where it has them, or else by its render
/// bounds (actors, whose shape depends on their animation). `dir` is a unit vector.
fn pick_object(origin: Vec3, dir: Vec3, visuals: &Query<Pickable>, meshes: &Assets<Mesh>) -> Option<(u32, f32)> {
    let mut boxes: Vec<(f32, u32, Affine3A, Option<&Mesh3d>)> = visuals
        .iter()
        .filter(|(_, _, _, shown, _, _)| shown.get())
        .filter_map(|(r, aabb, t, _, mesh, skinned)| {
            let d = ray_box(origin, dir, &t.affine(), Vec3::from(aabb.center), Vec3::from(aabb.half_extents))?;
            Some((d, r.0, t.affine(), mesh.filter(|_| !skinned)))
        })
        .collect();
    boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut best: Option<(u32, f32)> = None;
    for (entry, id, to_world, mesh) in boxes {
        if best.is_some_and(|(_, d)| entry > d) {
            break;
        }
        let distance = match mesh.and_then(|m| meshes.get(&m.0)) {
            Some(mesh) => {
                let to_local = to_world.inverse();
                match ray_mesh(mesh, to_local.transform_point3(origin), to_local.transform_vector3(dir)) {
                    Some(d) => d,
                    None => continue,
                }
            }
            None if entry > MIN_PICK_DISTANCE => entry,
            None => continue,
        };
        if best.is_none_or(|(_, d)| distance < d) {
            best = Some((id, distance));
        }
    }
    best
}

/// The landscape's height at a game position of the worldspace.
fn ground_height(state: &mut EditorState, world: &WorldData, x: f32, y: f32) -> Option<f32> {
    let grid = cell_of(x, y);
    let heights = state.land.entry(grid).or_insert_with(|| {
        let land = world.grid_cell(grid)?.land?;
        load_land(world.lo, world.schema, land, LandParts::HEIGHTS).and_then(|l| l.heights)
    });
    let heights = heights.as_ref()?;
    Some(height_at(heights, x - grid.0 as f32 * CELL_SIZE, y - grid.1 as f32 * CELL_SIZE))
}

/// How far along the ray (game space, unit direction) it meets the landscape.
fn ground_distance(state: &mut EditorState, world: &WorldData, origin: Vec3, dir: Vec3) -> Option<f32> {
    if state.space.is_some() {
        return None;
    }
    let mut above = 0.0;
    let mut t = GROUND_STEP;
    while t < GROUND_REACH {
        let p = origin + dir * t;
        match ground_height(state, world, p.x, p.y) {
            Some(h) if p.z <= h => {
                let (mut low, mut high) = (above, t);
                for _ in 0..GROUND_REFINE_STEPS {
                    let middle = (low + high) / 2.0;
                    let q = origin + dir * middle;
                    if ground_height(state, world, q.x, q.y).is_some_and(|h| q.z <= h) {
                        high = middle;
                    } else {
                        low = middle;
                    }
                }
                return Some(high);
            }
            _ => above = t,
        }
        t += GROUND_STEP;
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn click_to_select(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<FlyCam>>,
    root: Query<&GlobalTransform, With<WorldRoot>>,
    visuals: Query<Pickable>,
    meshes: Res<Assets<Mesh>>,
    world: Res<WorldData>,
    mut state: ResMut<EditorState>,
) {
    if !mouse.just_pressed(MouseButton::Left) || state.pointer_over_ui || state.click_taken {
        return;
    }
    let (Ok(window), Ok((cam, cam_t)), Ok(root)) = (windows.single(), camera.single(), root.single()) else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok(ray) = cam.viewport_to_world(cam_t, cursor) else { return };
    let object = pick_object(ray.origin, *ray.direction, &visuals, &meshes);
    let Some((game_origin, game_dir)) = pointer_ray_in_game(cursor, cam, cam_t, root) else { return };
    let ground = ground_distance(&mut state, &world, game_origin, game_dir);
    // The ground in front of an object takes the click: nothing is selected.
    let picked = object.filter(|(_, d)| ground.is_none_or(|g| *d <= g)).map(|(id, _)| id);
    // A click on what is selected, or on nothing, unselects.
    let next = picked.filter(|id| state.selected != Some(*id));
    select(&mut state, next);
}

/// Stand the selection on the landscape: its lowest point at the ground under its middle.
fn drop_to_ground(
    visuals: Query<(&RefVisual, &Aabb, &GlobalTransform)>,
    root: Query<&GlobalTransform, With<WorldRoot>>,
    world: Res<WorldData>,
    mut state: ResMut<EditorState>,
) {
    if !std::mem::take(&mut state.drop_requested) {
        return;
    }
    let Some(id) = state.selected else { return };
    if state.space.is_some() {
        state.status = "Interiors have no landscape to drop onto".to_string();
        return;
    }
    let (Some(mut pose), Ok(root)) = (state.pose(&world, id), root.single()) else { return };
    let to_game = root.affine().inverse();
    let lowest = visuals
        .iter()
        .filter(|(r, _, _)| r.0 == id)
        .flat_map(|(_, a, t)| corners(a, t))
        .map(|p| to_game.transform_point3(p).z)
        .min_by(f32::total_cmp);
    let (Some(lowest), Some(ground)) = (lowest, ground_height(&mut state, &world, pose.position[0], pose.position[1])) else {
        state.status = "No landscape here".to_string();
        return;
    };
    pose.position[2] += ground - lowest;
    state.commit();
    state.change(&world, id, pose, false);
    state.commit();
}

/// Enter an interior, or go back outside.
#[allow(clippy::too_many_arguments)]
fn change_space(
    world: Res<WorldData>,
    mut travel: MessageWriter<Travel>,
    mut camera: Query<&mut Transform, With<FlyCam>>,
    mut state: ResMut<EditorState>,
) {
    let Some(request) = state.space_request.take() else { return };
    let Ok(mut cam) = camera.single_mut() else { return };
    let interior = match request {
        SpaceRequest::Interior(cell) => {
            if state.space.is_none() {
                state.exterior_camera = Some(*cam);
            }
            let refs = world.index.cells.get(&cell).map_or(&[][..], |c| &c.refs);
            let spots: Vec<Vec3> = refs.iter().filter_map(|r| placement(world.lo, world.schema, *r)).map(|p| Vec3::from_array(p.position)).collect();
            let middle = spots.iter().copied().sum::<Vec3>() / spots.len().max(1) as f32;
            cam.translation = space::to_bevy((middle + Vec3::Z * INTERIOR_CAMERA_LIFT).to_array());
            Some(cell)
        }
        SpaceRequest::Exterior(to) => {
            match (to, state.exterior_camera) {
                (Some(to), _) => cam.translation = space::to_bevy(to),
                (None, Some(back)) => *cam = back,
                (None, None) => {}
            }
            None
        }
    };
    let feet = space::to_game(cam.translation);
    travel.write(Travel { world: None, interior, feet, heading_deg: 0.0 });
    select(&mut state, None);
    state.space = interior;
    let placed: Vec<u32> = state.placed_in.keys().copied().collect();
    state.sync.extend(placed);
}

/// The window's close button: ask first when there is unsaved work.
fn close_request(mut requests: MessageReader<WindowCloseRequested>, mut exit: MessageWriter<AppExit>, mut state: ResMut<EditorState>) {
    if requests.read().count() == 0 {
        return;
    }
    state.commit();
    // The question needs the editor's panels and a free pointer.
    if state.playing {
        state.leaving = Some(0);
    }
    if state.unsaved {
        state.close_prompt = true;
    } else {
        exit.write(AppExit::Success);
    }
}

/// Base forms by model path: the statics of the load order.
fn model_bases(world: &WorldData) -> HashMap<String, u32> {
    let mut bases = HashMap::new();
    for (i, plugin) in world.lo.plugins.iter().enumerate() {
        visit_records(&plugin.nodes[1..], &mut |r, _| {
            if &r.header.sig != b"STAT" {
                return;
            }
            let Some(id) = world.lo.to_global(i, r.header.form_id) else { return };
            if let Some(model) = base_object(world.lo, world.schema, id).and_then(|b| b.model) {
                bases.insert(fnv_vfs::normalize(&model), id);
            }
        });
    }
    bases
}

/// Spawn an object's model at `pose`, as streaming spawns a static one.
fn spawn_object(commands: &mut Commands, root: Entity, models: &mut ModelCtx, path: &str, id: u32, pose: Pose, space: Option<u32>) -> bool {
    let Some(model) = models.model(path) else { return false };
    let at = pose_affine(&pose);
    for s in &model.shapes {
        commands.entity(root).with_child((
            Mesh3d(s.mesh.clone()),
            MeshMaterial3d(s.material.clone()),
            Transform::from_matrix(Mat4::from(at * s.transform)),
            RefVisual(id),
            Applied(pose),
            Placed(space),
        ));
    }
    !model.shapes.is_empty()
}

#[allow(clippy::too_many_arguments)]
fn place_objects(
    mut commands: Commands,
    mut models: ModelCtx,
    world: Res<WorldData>,
    camera: Query<(&Camera, &GlobalTransform), With<FlyCam>>,
    root: Query<(Entity, &GlobalTransform), With<WorldRoot>>,
    visuals: Query<Pickable>,
    mut state: ResMut<EditorState>,
) {
    let Some(request) = state.place.take() else { return };
    let (Ok((cam, cam_t)), Ok((root_entity, root_t))) = (camera.single(), root.single()) else { return };
    let (path, base, pose, space) = match request {
        PlaceRequest::Restore(id) => {
            let path = placement(world.lo, world.schema, id).and_then(|p| p.base).and_then(|b| base_object(world.lo, world.schema, b)).and_then(|b| b.world_model_path());
            let (Some(path), Some(pose)) = (path, state.pose(&world, id)) else {
                state.status = "This object has no model to bring back".to_string();
                return;
            };
            let space = state.interior_of(&world, id);
            if !spawn_object(&mut commands, root_entity, &mut models, &path, id, pose, space) {
                state.status = format!("{path} has nothing to draw");
                return;
            }
            state.commit();
            state.placed_in.insert(id, space);
            state.placed_models.insert(id, path);
            state.change(&world, id, pose, false);
            state.commit();
            state.deleted_by_plugin.retain(|d| *d != id);
            state.selected = Some(id);
            return;
        }
        PlaceRequest::Duplicate(from) => {
            let Some(mut pose) = state.pose(&world, from) else { return };
            pose.position[0] += DUPLICATE_OFFSET;
            let base = state.new_refs.get(&from).copied().or_else(|| placement(world.lo, world.schema, from).and_then(|p| p.base));
            let Some(base) = base else {
                state.status = "This object has no base form to copy".to_string();
                return;
            };
            let new_static = state.new_statics.iter().find(|s| s.form_id == base).map(|s| format!("meshes\\{}", fnv_vfs::normalize(&s.model)));
            let path = new_static.or_else(|| base_object(world.lo, world.schema, base).filter(|b| b.sig != *b"NPC_" && b.sig != *b"CREA").and_then(|b| b.world_model_path()));
            let Some(path) = path else {
                state.status = "Only objects with one model can be duplicated for now".to_string();
                return;
            };
            let space = state.interior_of(&world, from);
            (path, base, pose, space)
        }
        PlaceRequest::Asset { path, cursor } => {
            let Ok(ray) = cam.viewport_to_world(cam_t, cursor) else { return };
            let (origin, dir) = (ray.origin, *ray.direction);
            let to_game = root_t.affine().inverse();
            let (game_origin, game_dir) = (to_game.transform_point3(origin), to_game.transform_vector3(dir).normalize());
            let object = pick_object(origin, dir, &visuals, &models.meshes).map(|(_, d)| d);
            let ground = ground_distance(&mut state, &world, game_origin, game_dir);
            let distance = [object, ground].into_iter().flatten().min_by(f32::total_cmp).unwrap_or(PLACE_FALLBACK_DISTANCE);
            let mut position = game_origin + game_dir * distance;
            // Stand it on what the ray met, not with its middle in it.
            if let Some((min, _)) = models.model(&path).and_then(|m| m.bounds) {
                position.z -= min.z;
            }
            let pose = Pose { position: position.to_array(), rotation: [0.0; 3], scale: 1.0 };
            let Some(relative) = path.strip_prefix("meshes\\").map(fnv_vfs::normalize) else { return };
            if state.model_bases.is_none() {
                state.model_bases = Some(model_bases(&world));
            }
            let known = state.model_bases.as_ref().and_then(|b| b.get(&relative)).copied();
            let base = match known {
                Some(base) => base,
                None => {
                    let form_id = state.take_object_id(&world);
                    let stem: String = relative.rsplit('\\').next().unwrap_or("").trim_end_matches(".nif").chars().filter(char::is_ascii_alphanumeric).collect();
                    state.new_statics.push(NewStatic { form_id, edid: format!("NVFEd{stem}{form_id:08X}"), model: relative.clone() });
                    state.model_bases.get_or_insert_default().insert(relative, form_id);
                    form_id
                }
            };
            let space = state.space;
            (path, base, pose, space)
        }
    };
    let id = state.take_object_id(&world);
    if !spawn_object(&mut commands, root_entity, &mut models, &path, id, pose, space) {
        state.status = format!("{path} has nothing to draw");
        return;
    }
    state.commit();
    state.new_refs.insert(id, base);
    state.placed_in.insert(id, space);
    state.placed_models.insert(id, path.clone());
    state.edits.insert(id, Edit { original: pose, current: pose, deleted: true, loaded_deleted: true });
    state.change(&world, id, pose, false);
    state.commit();
    let stem = path.rsplit('\\').next().unwrap_or(&path);
    let label = format!("{stem}  [new]  {id:08X}");
    state.names.insert(id, ObjectName { lower: label.to_lowercase(), label });
    state.selected = Some(id);
    state.status = format!("Placed {stem}");
}

type ObjectParts<'a> = (Entity, &'a RefVisual, &'a ChildOf, &'a mut Transform, Option<&'a Applied>, Option<&'a Placed>, Option<&'a mut Visibility>);

/// Make the entities of edited objects show their edit: those a change asks for, and those
/// that streaming spawned last frame (their parents' transforms are ready by now).
#[allow(clippy::too_many_arguments)]
fn sync_edits(
    mut commands: Commands,
    mut state: ResMut<EditorState>,
    root: Query<&GlobalTransform, With<WorldRoot>>,
    mut objects: Query<ObjectParts>,
    has_ref: Query<(), With<RefVisual>>,
    globals: Query<&GlobalTransform>,
    added: Query<&RefVisual, Added<RefVisual>>,
    mut spawned_last_frame: Local<Vec<u32>>,
    mut sim: ResMut<SimState>,
    (mut repose, mut changed): (MessageWriter<RefRepose>, MessageWriter<RefChanged>),
) {
    let mut ids = std::mem::take(&mut state.sync);
    for id in &ids {
        if let Some(edit) = state.edits.get(id) {
            edit_to_simulation(&mut sim, *id, edit, &mut repose, &mut changed);
        }
    }
    ids.extend(spawned_last_frame.drain(..));
    spawned_last_frame.extend(added.iter().map(|r| r.0).filter(|id| state.edits.contains_key(id)));
    if ids.is_empty() {
        return;
    }
    let Ok(root) = root.single() else { return };
    let (world_from_game, game_from_world) = (root.affine(), root.affine().inverse());
    for (entity, r, parent, mut transform, applied, placed, visibility) in &mut objects {
        // A part under another part moves with it.
        if !ids.contains(&r.0) || has_ref.contains(parent.parent()) {
            continue;
        }
        let Some(edit) = state.edits.get(&r.0).copied() else { continue };
        let shown = applied.map_or(edit.original, |a| a.0);
        // `nvf_play::moved` draws a placed object where the simulation holds it.
        let drawn_by_game = placed.is_none() && sim.refs.get(FormId(r.0)).is_some_and(|p| &p.sig != b"ACHR" && &p.sig != b"ACRE");
        if !drawn_by_game && shown != edit.current {
            let Ok(parent_global) = globals.get(parent.parent()) else { continue };
            let delta = world_from_game * pose_affine(&edit.current) * pose_affine(&shown).inverse() * game_from_world;
            let local = parent_global.affine().inverse() * delta * parent_global.affine() * transform.compute_affine();
            *transform = Transform::from_matrix(Mat4::from(local));
            commands.entity(entity).insert(Applied(edit.current));
        }
        if let Some(mut visibility) = visibility {
            let elsewhere = placed.is_some_and(|p| p.0 != state.space);
            let wanted = if edit.deleted || elsewhere { Visibility::Hidden } else { Visibility::Inherited };
            if *visibility != wanted {
                *visibility = wanted;
            }
        }
    }
}

/// Put an edit where the game reads it: the pose and scale a script would set, and whether
/// the object is in the world. `restored`: no one did this in the game, so no crime or sound.
fn edit_to_simulation(sim: &mut SimState, id: u32, edit: &Edit, repose: &mut MessageWriter<RefRepose>, changed: &mut MessageWriter<RefChanged>) {
    let form = FormId(id);
    let Some(placed) = sim.refs.get(form) else { return };
    let scaled = edit.current.scale != placed.scale.unwrap_or(1.0);
    let enabled = !edit.deleted;
    let was_enabled = sim.world.is_enabled(form).ok();
    let Ok(s) = sim.world.get(form) else { return };
    (s.pos, s.rot) = (edit.current.position, edit.current.rotation);
    let rescale = scaled || s.scale.is_some();
    if rescale {
        s.scale = Some(edit.current.scale);
    }
    if was_enabled != Some(enabled) {
        let _ = sim.world.set_enabled(form, enabled);
    }
    repose.write(RefRepose { form_id: id });
    if rescale || was_enabled != Some(enabled) {
        changed.write(RefChanged { form_id: id, restored: true });
    }
}

/// The world-space corners of a render bound.
fn corners(aabb: &Aabb, t: &GlobalTransform) -> [Vec3; 8] {
    let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
    std::array::from_fn(|i| {
        let sign = Vec3::new(
            if i & 1 == 0 { -1.0 } else { 1.0 },
            if i & 2 == 0 { -1.0 } else { 1.0 },
            if i & 4 == 0 { -1.0 } else { 1.0 },
        );
        t.affine().transform_point3(c + h * sign)
    })
}

fn draw_selection(state: Res<EditorState>, visuals: Query<(&RefVisual, &Aabb, &GlobalTransform)>, mut gizmos: Gizmos) {
    let Some(selected) = state.selected else { return };
    const EDGES: [(usize, usize); 12] = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];
    for (_, aabb, t) in visuals.iter().filter(|(r, _, _)| r.0 == selected) {
        let c = corners(aabb, t);
        for (a, b) in EDGES {
            gizmos.line(c[a], c[b], Color::srgb(1.0, 0.65, 0.1));
        }
    }
}

fn draw_gizmo(
    state: Res<EditorState>,
    world: Res<WorldData>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<FlyCam>>,
    root: Query<&GlobalTransform, With<WorldRoot>>,
    mut gizmos: Gizmos,
) {
    let Some(pose) = state.selected.and_then(|id| state.pose(&world, id)) else { return };
    let (Ok(window), Ok((cam, cam_t)), Ok(root)) = (windows.single(), camera.single(), root.single()) else { return };
    let at = Vec3::from_array(pose.position);
    let size = gizmo_size(at, root.affine().inverse().transform_point3(cam_t.translation()));
    let hovered = match &state.drag {
        Some(drag) => Some(drag.axis),
        None if state.pointer_over_ui => None,
        None => window.cursor_position().and_then(|c| axis_under_pointer(c, state.tool, at, size, cam, cam_t, root)),
    };
    let to_world = |p: Vec3| root.affine().transform_point3(p);
    const COLOURS: [Color; 3] = [Color::srgb(0.95, 0.2, 0.2), Color::srgb(0.25, 0.85, 0.25), Color::srgb(0.25, 0.45, 1.0)];
    for (i, axis) in AXES.iter().enumerate() {
        let colour = if hovered == Some(i) { Color::srgb(1.0, 0.95, 0.2) } else { COLOURS[i] };
        match state.tool {
            Tool::Move => {
                gizmos.arrow(to_world(at), to_world(at + *axis * size), colour);
            }
            Tool::Scale => {
                let end = at + *axis * size;
                gizmos.line(to_world(at), to_world(end), colour);
                gizmos.sphere(Isometry3d::from_translation(to_world(end)), size * 0.05, colour);
            }
            Tool::Rotate => {
                let points: Vec<Vec3> = ring_points(at, *axis, size).map(to_world).collect();
                gizmos.linestrip(points, colour);
            }
        }
    }
}

/// F, or the inspector's button: put the selection in front of the camera.
fn focus_selection(visuals: Query<(&RefVisual, &Aabb, &GlobalTransform)>, mut camera: Query<&mut Transform, With<FlyCam>>, mut state: ResMut<EditorState>) {
    let asked = std::mem::take(&mut state.focus);
    let (true, Some(selected)) = (asked, state.selected) else { return };
    let Ok(mut cam) = camera.single_mut() else { return };
    let points: Vec<Vec3> = visuals.iter().filter(|(r, _, _)| r.0 == selected).flat_map(|(_, a, t)| corners(a, t)).collect();
    if points.is_empty() {
        return;
    }
    let (min, max) = points.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    let centre = (min + max) / 2.0;
    let radius = (max - min).length() / 2.0;
    let back = cam.rotation * Vec3::Z;
    cam.translation = centre + back * (radius * 2.5).max(100.0);
}

/// Write the plugin: to a temporary file first, so a failure leaves the last save whole.
/// The file it replaces stays beside it as `.bak`.
fn write_plugin(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let describe = |e: std::io::Error| format!("{}: {e}", path.display());
    let dir = path.parent().ok_or("the plugin's path has no folder")?;
    std::fs::create_dir_all(dir).map_err(describe)?;
    fnv_install::guard_write(path, None).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("esp.tmp");
    std::fs::write(&temporary, bytes).map_err(describe)?;
    if path.exists() {
        std::fs::copy(path, path.with_extension("esp.bak")).map_err(describe)?;
    }
    std::fs::rename(&temporary, path).map_err(describe)
}

fn save(world: Res<WorldData>, mut exit: MessageWriter<AppExit>, mut state: ResMut<EditorState>) {
    if !std::mem::take(&mut state.save_requested) {
        return;
    }
    state.commit();
    let edits = state.edits_to_save(&world);
    let result = edit_plugin_path().ok_or_else(|| "NVForever has no home folder".to_string()).and_then(|path| {
        let bytes = edit_plugin::build(world.lo, world.schema, world.index, &edits)?;
        write_plugin(&path, &bytes)?;
        Ok(path)
    });
    match result {
        Ok(path) => {
            info!("editor: saved {} changes and {} new objects to {}", edits.changes.len(), edits.new_refs.len(), path.display());
            state.status = format!("Saved {} changes, {} new objects", edits.changes.len(), edits.new_refs.len());
            state.unsaved = false;
            if state.quit_after_save {
                exit.write(AppExit::Success);
            }
        }
        Err(e) => {
            error!("editor: save failed: {e}");
            state.status = format!("Save failed: {e}");
            state.quit_after_save = false;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn panels(
    mut contexts: EguiContexts,
    mut state: ResMut<EditorState>,
    world: Res<WorldData>,
    game: Res<GameData>,
    loaded: Query<&RefVisual>,
    camera: Query<&Transform, With<FlyCam>>,
    root: Query<&GlobalTransform, With<WorldRoot>>,
    mut exit: MessageWriter<AppExit>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    if state.playing {
        egui::Area::new("editor_playing".into()).anchor(egui::Align2::CENTER_TOP, [0.0, 6.0]).interactable(false).show(&ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| ui.label("Playing in the editor. F8 goes back."));
        });
        (state.pointer_over_ui, state.typing) = (false, false);
        return Ok(());
    }
    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "editor".into(),
        egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()),
    );

    let hidden = |id: &u32| state.edits.get(id).is_some_and(|e| e.deleted) || state.placed_in.get(id).is_some_and(|s| *s != state.space);
    let ids: BTreeSet<u32> = loaded.iter().map(|r| r.0).filter(|id| !hidden(id)).collect();
    let mut named = 0;
    for id in &ids {
        if named == NAMES_PER_FRAME {
            break;
        }
        if !state.names.contains_key(id) {
            state.names.insert(*id, object_name(&world, *id));
            named += 1;
        }
    }

    let game_from_world = root.single().map(|r| r.affine().inverse()).ok();
    let camera_game = camera.single().ok().zip(game_from_world).map_or(Vec3::ZERO, |(t, g)| g.transform_point3(t.translation));
    egui::Panel::top("editor_toolbar").show(&mut viewport, |ui| toolbar(ui, &mut state, &world, camera_game));
    egui::Panel::bottom("editor_assets").resizable(true).default_size(200.0).show(&mut viewport, |ui| asset_drawer(ui, &mut state, &game));
    egui::Panel::left("editor_objects").resizable(true).default_size(320.0).show(&mut viewport, |ui| object_list(ui, &mut state, &world, &ids));
    egui::Panel::right("editor_inspector").resizable(true).default_size(320.0).show(&mut viewport, |ui| inspector(ui, &mut state, &world));

    if state.close_prompt {
        close_prompt(&ctx, &mut state, &mut exit);
    }
    if let Some((at, spot)) = state.drop_menu {
        let menu = egui::Area::new("editor_drop_menu".into()).fixed_pos([at.x, at.y]).show(&ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| ui.button("Drop player here").clicked()).inner
        });
        if menu.inner {
            state.play_request = Some(PlayFrom::Spot(spot));
        }
        if menu.inner || menu.response.clicked_elsewhere() {
            state.drop_menu = None;
        }
    }

    state.pointer_over_ui = ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui();
    state.typing = ctx.egui_wants_keyboard_input();
    // A change from the inspector's fields is one undo step when its drag or typing ends.
    if state.drag.is_none() && !state.typing && !ctx.input(|i| i.pointer.any_down()) {
        state.commit();
    }
    Ok(())
}

fn close_prompt(ctx: &egui::Context, state: &mut EditorState, exit: &mut MessageWriter<AppExit>) {
    egui::Modal::new(egui::Id::new("editor_close")).show(ctx, |ui| {
        ui.heading("Save your changes before closing?");
        ui.label("Changes that you do not save are lost.");
        ui.horizontal(|ui| {
            if ui.button("Save and close").clicked() {
                state.save_requested = true;
                state.quit_after_save = true;
                state.close_prompt = false;
            }
            if ui.button("Close without saving").clicked() {
                exit.write(AppExit::Success);
            }
            if ui.button("Cancel").clicked() {
                state.close_prompt = false;
            }
        });
    });
}

fn toolbar(ui: &mut egui::Ui, state: &mut EditorState, world: &WorldData, camera_game: Vec3) {
    ui.horizontal(|ui| {
        ui.strong("NVForever Editor");
        ui.separator();
        for (tool, label) in [(Tool::Move, "Move (W)"), (Tool::Rotate, "Rotate (E)"), (Tool::Scale, "Scale (R)")] {
            if ui.selectable_label(state.tool == tool, label).clicked() {
                state.tool = tool;
            }
        }
        ui.separator();
        ui.checkbox(&mut state.snap, "Snap");
        ui.add(egui::DragValue::new(&mut state.snap_move).range(1.0..=4096.0).suffix(" u"));
        ui.add(egui::DragValue::new(&mut state.snap_degrees).range(1.0..=90.0).suffix("°"));
        ui.separator();
        if ui.button("Play (F5)").on_hover_text("Drop the player under the camera. Right click a spot to drop them there. F8 comes back.").clicked() {
            state.play_request = Some(PlayFrom::Camera);
        }
        if ui.add_enabled(state.played, egui::Button::new("Resume")).on_hover_text("Back to where the player was left").clicked() {
            state.play_request = Some(PlayFrom::Resume);
        }
        ui.separator();
        if ui.add_enabled(!state.undo.is_empty(), egui::Button::new("Undo")).clicked() {
            state.undo();
        }
        if ui.add_enabled(!state.redo.is_empty(), egui::Button::new("Redo")).clicked() {
            state.redo();
        }
        if ui.button(if state.unsaved { "Save *" } else { "Save" }).clicked() {
            state.save_requested = true;
        }
        ui.separator();
        match state.space {
            Some(cell) => {
                let name = world.index.cells.get(&cell).and_then(|c| c.edid.clone()).unwrap_or_else(|| format!("{cell:08X}"));
                ui.label(format!("Interior {name}"));
                if ui.button("Go outside").clicked() {
                    state.space_request = Some(SpaceRequest::Exterior(None));
                }
            }
            None => {
                let (cx, cy) = cell_of(camera_game.x, camera_game.y);
                ui.label(format!("Cell {cx}, {cy}"));
            }
        }
        ui.add(egui::DragValue::new(&mut state.goto.0).prefix("x "));
        ui.add(egui::DragValue::new(&mut state.goto.1).prefix("y "));
        if ui.button("Go").clicked() {
            let half = CELL_SIZE / 2.0;
            let to = [state.goto.0 as f32 * CELL_SIZE + half, state.goto.1 as f32 * CELL_SIZE + half, camera_game.z];
            state.space_request = Some(SpaceRequest::Exterior(Some(to)));
        }
        ui.separator();
        let stale = state.out_of_date_cells(world).len();
        if stale > 0 {
            ui.colored_label(egui::Color32::from_rgb(240, 180, 60), format!("Navmesh and distant objects out of date in {stale} cells"));
            ui.separator();
        }
        ui.label(state.status.as_str());
    });
    ui.weak("Hold right mouse: look, WASD/QE move, Shift fast, wheel speed.   Click select, click again or Esc unselect.   F frame.   Ctrl+Z undo, Ctrl+Y redo, Ctrl+S save, Ctrl+D duplicate, Del delete.   Drag a mesh into the viewport to place it.");
}

fn deleted_list(ui: &mut egui::Ui, state: &mut EditorState, world: &WorldData) {
    let mut session: Vec<u32> = state.edits.iter().filter(|(_, e)| e.deleted && !e.loaded_deleted).map(|(id, _)| *id).collect();
    session.sort_unstable();
    let from_plugin = state.deleted_by_plugin.clone();
    egui::CollapsingHeader::new(format!("Deleted ({})", session.len() + from_plugin.len())).id_salt("editor_deleted").show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("editor_deleted_rows").max_height(140.0).show(ui, |ui| {
            for id in session.iter().chain(&from_plugin) {
                ui.horizontal(|ui| {
                    if ui.small_button("Restore").clicked() {
                        if from_plugin.contains(id) {
                            state.place = Some(PlaceRequest::Restore(*id));
                        } else if let Some(pose) = state.pose(world, *id) {
                            state.change(world, *id, pose, false);
                            state.commit();
                        }
                    }
                    ui.label(state.names.get(id).map_or_else(|| format!("{id:08X}"), |n| n.label.clone()));
                });
            }
        });
    });
}

fn interior_list(ui: &mut egui::Ui, state: &mut EditorState) {
    egui::CollapsingHeader::new(format!("Interiors ({})", state.interiors.len())).id_salt("editor_interiors").show(ui, |ui| {
        ui.add(egui::TextEdit::singleline(&mut state.interior_filter).hint_text("Search interiors"));
        let filter = state.interior_filter.to_lowercase();
        let rows: Vec<(u32, String)> = state.interiors.iter().filter(|(_, n)| n.to_lowercase().contains(&filter)).cloned().collect();
        egui::ScrollArea::vertical().id_salt("editor_interior_rows").max_height(160.0).show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
            for (cell, name) in &rows[range] {
                if ui.selectable_label(state.space == Some(*cell), name.as_str()).clicked() {
                    state.space_request = Some(SpaceRequest::Interior(*cell));
                }
            }
        });
    });
}

fn object_list(ui: &mut egui::Ui, state: &mut EditorState, world: &WorldData, ids: &BTreeSet<u32>) {
    interior_list(ui, state);
    deleted_list(ui, state, world);
    ui.separator();
    ui.heading("Objects");
    ui.add(egui::TextEdit::singleline(&mut state.object_filter).hint_text("Search loaded objects"));
    let filter = state.object_filter.to_lowercase();
    let rows: Vec<u32> = ids.iter().copied().filter(|id| state.names.get(id).is_some_and(|n| n.lower.contains(&filter))).collect();
    ui.weak(format!("{} of {} loaded", rows.len(), ids.len()));
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
        for id in &rows[range] {
            let label = state.names[id].label.clone();
            let chosen = state.selected == Some(*id);
            if ui.selectable_label(chosen, label).clicked() {
                select(state, (!chosen).then_some(*id));
            }
        }
    });
}

fn inspector(ui: &mut egui::Ui, state: &mut EditorState, world: &WorldData) {
    ui.heading("Inspector");
    ui.separator();
    let Some(id) = state.selected else {
        ui.weak("Click an object in the viewport or the list.");
        ui.allocate_space(ui.available_size());
        return;
    };
    let Some(pose) = state.pose(world, id) else {
        ui.label(format!("{id:08X} has no record: the game made it at run time, and it cannot be edited."));
        ui.allocate_space(ui.available_size());
        return;
    };
    let mut next = pose;
    egui::Grid::new("editor_pose").num_columns(2).show(ui, |ui| {
        ui.label("Position");
        ui.horizontal(|ui| {
            for v in &mut next.position {
                ui.add(egui::DragValue::new(v).speed(1.0).max_decimals(2));
            }
        });
        ui.end_row();
        ui.label("Rotation");
        ui.horizontal(|ui| {
            for v in &mut next.rotation {
                let mut degrees = v.to_degrees();
                if ui.add(egui::DragValue::new(&mut degrees).speed(1.0).max_decimals(2).suffix("°")).changed() {
                    *v = degrees.to_radians();
                }
            }
        });
        ui.end_row();
        ui.label("Scale");
        ui.add(egui::DragValue::new(&mut next.scale).speed(0.01).range(MIN_SCALE..=MAX_SCALE).max_decimals(3));
        ui.end_row();
    });
    if next != pose {
        state.change(world, id, next, false);
    }
    ui.horizontal(|ui| {
        if ui.button("Unselect (Esc)").clicked() {
            select(state, None);
        }
        if ui.button("Frame (F)").clicked() {
            state.focus = true;
        }
        if ui.button("Drop to ground").clicked() {
            state.drop_requested = true;
        }
        if ui.button("Duplicate").clicked() {
            state.place = Some(PlaceRequest::Duplicate(id));
        }
        if ui.button("Delete").clicked() {
            state.delete_selected(world);
        }
    });
    ui.separator();
    egui::Grid::new("editor_details").num_columns(2).striped(true).show(ui, |ui| {
        let (cx, cy) = cell_of(pose.position[0], pose.position[1]);
        let mut row = |field: &str, value: String| {
            ui.label(field);
            ui.add(egui::Label::new(value).wrap());
            ui.end_row();
        };
        row("Form ID", format!("{id:08X}"));
        row("Cell", format!("{cx}, {cy}"));
        if let Some((plugin, _)) = world.lo.winner(id) {
            row("Defined in", plugin.name.clone());
        }
        let base = state.new_refs.get(&id).copied().or_else(|| placement(world.lo, world.schema, id).and_then(|p| p.base));
        if let Some(b) = base {
            row("Base form", format!("{b:08X}"));
            if let Some(b) = base_object(world.lo, world.schema, b) {
                row("Base type", String::from_utf8_lossy(&b.sig).into_owned());
                if let Some(e) = b.edid {
                    row("Base name", e);
                }
                if let Some(m) = b.model {
                    row("Model", m);
                }
            }
        }
    });
    ui.allocate_space(ui.available_size());
}

fn asset_drawer(ui: &mut egui::Ui, state: &mut EditorState, game: &GameData) {
    ui.horizontal(|ui| {
        ui.heading("Assets");
        for (kind, name) in AssetKind::ALL {
            if ui.selectable_label(state.asset_kind == kind, format!("{name} ({})", state.assets[kind.index()].len())).clicked() {
                state.asset_kind = kind;
                state.assets_dirty = true;
            }
        }
        ui.separator();
        if ui.add(egui::TextEdit::singleline(&mut state.asset_filter).hint_text("Search assets")).changed() {
            state.assets_dirty = true;
        }
        if let Some(path) = &state.asset_selected {
            ui.separator();
            ui.label(path.as_str());
            if let Some(source) = game.vfs.source_of(path) {
                ui.weak(format!("from {source}"));
            }
        }
    });
    if std::mem::take(&mut state.assets_dirty) {
        let filter = state.asset_filter.to_lowercase();
        state.asset_rows = state.assets[state.asset_kind.index()]
            .iter()
            .enumerate()
            .filter(|(_, p)| p.contains(&filter))
            .map(|(i, _)| i)
            .collect();
    }
    ui.separator();
    let kind = state.asset_kind;
    egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, ROW_HEIGHT, state.asset_rows.len(), |ui, range| {
        for row in range {
            let path = state.assets[kind.index()][state.asset_rows[row]].clone();
            let chosen = state.asset_selected.as_deref() == Some(path.as_str());
            let response = ui.add(egui::Button::selectable(chosen, path.as_str()).sense(egui::Sense::click_and_drag()));
            if response.clicked() {
                state.asset_selected = Some(path.clone());
            }
            if kind == AssetKind::Meshes && response.dragged() {
                egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::new("editor_drag"), egui::PopupAnchor::Pointer).show(|ui| ui.label(format!("Place {path}")));
            }
            if kind == AssetKind::Meshes && response.drag_stopped() {
                // Over the viewport: the drop is not on any panel.
                if let Some(p) = ui.ctx().pointer_latest_pos().filter(|_| !ui.ctx().is_pointer_over_egui()) {
                    state.place = Some(PlaceRequest::Asset { path, cursor: Vec2::new(p.x, p.y) });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_on_axis_finds_the_crossing_point() {
        // A ray straight down through x = 5 meets the X axis at 5.
        let t = nearest_on_axis(Vec3::new(5.0, 0.0, 10.0), Vec3::NEG_Z, Vec3::ZERO, Vec3::X).unwrap();
        assert!((t - 5.0).abs() < 1e-4);
        assert!(nearest_on_axis(Vec3::ZERO, Vec3::X, Vec3::ZERO, Vec3::X).is_none());
    }

    #[test]
    fn angle_round_axis_follows_the_right_hand() {
        let (u, v) = Vec3::Z.any_orthonormal_pair();
        let from_above = |p: Vec3| angle_round_axis(p + Vec3::Z * 10.0, Vec3::NEG_Z, Vec3::ZERO, Vec3::Z).unwrap();
        assert!(from_above(u).abs() < 1e-4);
        assert!((from_above(v) - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    }

    #[test]
    fn rotated_turns_a_vector_about_the_world_axis() {
        // A quarter turn about Z from no rotation takes X to Y.
        let r = rotated([0.0; 3], Vec3::Z, std::f32::consts::FRAC_PI_2);
        let m = Mat3::from_cols_array(&ref_rotation(r)).transpose();
        assert!((m * Vec3::X - Vec3::Y).length() < 1e-5);
    }

    #[test]
    fn rotated_composes_with_an_existing_rotation() {
        let start = [0.4, -0.3, 1.1];
        let turned = rotated(start, Vec3::X, 0.5);
        let expected = Mat3::from_axis_angle(Vec3::X, 0.5) * Mat3::from_cols_array(&ref_rotation(start)).transpose();
        let got = Mat3::from_cols_array(&ref_rotation(turned)).transpose();
        assert!((got * Vec3::new(1.0, 2.0, 3.0) - expected * Vec3::new(1.0, 2.0, 3.0)).length() < 1e-4);
    }

    #[test]
    fn ray_triangle_hits_inside_and_misses_outside() {
        let (a, b, c) = (Vec3::ZERO, Vec3::X, Vec3::Y);
        let hit = ray_triangle(Vec3::new(0.2, 0.2, 5.0), Vec3::NEG_Z, a, b, c);
        assert!((hit.unwrap() - 5.0).abs() < 1e-5);
        assert!(ray_triangle(Vec3::new(0.9, 0.9, 5.0), Vec3::NEG_Z, a, b, c).is_none());
        assert!(ray_triangle(Vec3::new(0.2, 0.2, 5.0), Vec3::Z, a, b, c).is_none());
    }

    #[test]
    fn snapped_rounds_to_the_step() {
        assert_eq!(snapped(70.0, 64.0), 64.0);
        assert_eq!(snapped(100.0, 64.0), 128.0);
        assert_eq!(snapped(7.0, 0.0), 7.0);
    }

    #[test]
    fn distance_to_segment_clamps_to_the_ends() {
        let (a, b) = (Vec2::ZERO, Vec2::new(10.0, 0.0));
        assert_eq!(distance_to_segment(Vec2::new(5.0, 3.0), a, b), 3.0);
        assert_eq!(distance_to_segment(Vec2::new(-4.0, 0.0), a, b), 4.0);
    }
}
