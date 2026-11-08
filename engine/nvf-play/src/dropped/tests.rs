//! Created references: which cell they belong to, what to draw, and the physics pose
//! written back to the simulation (no game data, no Bevy app).

use super::*;
use crate::phys::{CreatedBody, PhysState};
use fnv_collide::{CollisionObject, Motion, Shape};
use fnv_phys::{PhysParams, PhysWorld, RefPose};
use fnv_sim::world::{Snapshot, SNAPSHOT_FORMAT};
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use fnv_world::transform::{axis, max_diff, ref_angles};
use fnv_world::{Cell, Worldspace, CELL_SIZE};

const WORLD: u32 = 0x3C;
const CELL_00: u32 = 0x200; // exterior (0, 0)
const CELL_10: u32 = 0x201; // exterior (1, 0)
const CELL_INT: u32 = 0x202;
const OTHER_WORLD_CELL: u32 = 0x203;
const DROPPED: u32 = 0xFF00_0000;

fn index() -> WorldIndex {
    let mut ix = WorldIndex::default();
    for (id, world, grid) in [(CELL_00, Some(WORLD), Some((0, 0))), (CELL_10, Some(WORLD), Some((1, 0))), (CELL_INT, None, None), (OTHER_WORLD_CELL, Some(0x99), Some((0, 0)))] {
        ix.cells.insert(id, Cell { form_id: id, world, grid, ..Default::default() });
    }
    let mut ws = Worldspace { form_id: WORLD, ..Default::default() };
    ws.grid.insert((0, 0), CELL_00);
    ws.grid.insert((1, 0), CELL_10);
    ix.worldspaces.insert(WORLD, ws);
    ix
}

/// A state in no World, read from its saved form: its flags are private to `fnv-sim`.
fn state(cell: Option<u32>, pos: [f32; 3]) -> RefState {
    serde_json::from_value(serde_json::json!({
        "base": FormId(0x100),
        "cell": cell.map(FormId),
        "pos": pos,
        "rot": [0.0, 0.0, 0.0],
        "enabled": true,
        "deleted": false,
        "count": 1,
        "open": false,
    }))
    .expect("a whole saved state")
}

/// `s` with other flags, through its saved form as [`state`].
fn with_flags(s: &RefState, enabled: bool, deleted: bool) -> RefState {
    let mut saved = serde_json::to_value(s).expect("a state saves");
    saved["enabled"] = enabled.into();
    saved["deleted"] = deleted.into();
    serde_json::from_value(saved).expect("a whole saved state")
}

#[test]
fn cells_of_a_created_reference() {
    let ix = index();
    // Outdoors: the grid square containing it, whatever cell the drop recorded.
    let s = state(Some(CELL_00), [CELL_SIZE + 10.0, 5.0, 0.0]);
    assert_eq!(render_cell(&ix, &s), Some(RenderCell::Exterior { world: WORLD, x: 1, y: 0 }));
    assert_eq!(phys_cell(&ix, WORLD, Space::Exterior, &s), Some(PhysCell::Exterior(1, 0)));
    assert_eq!(phys_cell(&ix, WORLD, Space::Interior(CELL_INT), &s), None, "not while the player is inside");
    // Another worldspace's cell is not this worldspace's square.
    let s = state(Some(OTHER_WORLD_CELL), [5.0, 5.0, 0.0]);
    assert_eq!(phys_cell(&ix, WORLD, Space::Exterior, &s), None);
    // Indoors: the interior, only while the player is in it.
    let s = state(Some(CELL_INT), [-5.0, 0.0, 0.0]);
    assert_eq!(render_cell(&ix, &s), Some(RenderCell::Interior(CELL_INT)));
    assert_eq!(phys_cell(&ix, WORLD, Space::Interior(CELL_INT), &s), Some(PhysCell::Interior(CELL_INT)));
    assert_eq!(phys_cell(&ix, WORLD, Space::Interior(0x777), &s), None);
    assert_eq!(phys_cell(&ix, WORLD, Space::Exterior, &s), None);
    // No cell, or one the index does not know: nowhere.
    assert_eq!(render_cell(&ix, &state(None, [0.0; 3])), None);
    assert_eq!(render_cell(&ix, &state(Some(0x999), [0.0; 3])), None);
}

#[test]
fn a_drop_becomes_a_spawn_and_a_take_a_despawn() {
    let ix = index();
    let int = RenderCell::Interior(CELL_INT);
    let mut s = state(Some(CELL_INT), [1.0, 2.0, 3.0]);
    let created = |s: &RefState| Placed::wanted(&ix, [(FormId(DROPPED), s)].into_iter());
    let loaded = |c: RenderCell| c == int;
    // Dropped in the loaded interior: drawn there, at the simulation's pose.
    let wanted = created(&s);
    assert_eq!(wanted[&DROPPED], Placed { cell: int, pos: [1.0, 2.0, 3.0], rot: [0.0; 3] });
    let p = plan(&wanted, &BTreeMap::new(), loaded);
    assert_eq!(p, Plan { spawn: vec![DROPPED], ..Default::default() });
    let drawn = wanted.clone();
    assert_eq!(plan(&wanted, &drawn, loaded), Plan::default(), "nothing to do once drawn");
    // It fell: the drawn pose follows.
    s.pos[2] = -20.0;
    s.rot = [0.1, 0.0, 0.0];
    assert_eq!(plan(&created(&s), &drawn, loaded), Plan { moved: vec![DROPPED], ..Default::default() });
    // Taken back: no longer wanted.
    let taken = with_flags(&s, true, true);
    assert!(created(&taken).is_empty());
    assert_eq!(plan(&created(&taken), &drawn, loaded), Plan { despawn: vec![DROPPED], ..Default::default() });
    // A disabled one is not drawn either.
    assert!(created(&with_flags(&s, false, false)).is_empty());
}

#[test]
fn cells_streaming_out_and_in_and_rolling_into_the_next_square() {
    let ix = index();
    let c10 = RenderCell::Exterior { world: WORLD, x: 1, y: 0 };
    let mut s = state(Some(CELL_00), [CELL_SIZE - 1.0, 10.0, 0.0]);
    let wanted = Placed::wanted(&ix, [(FormId(DROPPED), &s)].into_iter());
    // Its cell is not loaded: nothing to draw yet.
    assert_eq!(plan(&wanted, &BTreeMap::new(), |c| c == c10), Plan::default());
    // Drawn in (0, 0); the cell streams out: despawn, nothing spawned.
    let drawn = wanted.clone();
    assert_eq!(plan(&wanted, &drawn, |_| false), Plan { despawn: vec![DROPPED], ..Default::default() });
    // It rolls into (1, 0) while both are loaded: moved to the other cell's entity.
    s.pos[0] = CELL_SIZE + 1.0;
    let wanted = Placed::wanted(&ix, [(FormId(DROPPED), &s)].into_iter());
    assert_eq!(wanted[&DROPPED].cell, c10);
    assert_eq!(plan(&wanted, &drawn, |_| true), Plan { despawn: vec![DROPPED], spawn: vec![DROPPED], moved: vec![] });
}

fn object(motion: Motion, half: f32, body_transform: Iso) -> CollisionObject {
    CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform,
        motion,
        layer: fnv_phys::layers::CLUTTER,
        mass: 1.0,
        friction: 0.5,
        restitution: 0.0,
        shape: Shape::Box { half_extents: [half; 3] }.into(),
        shell: 0.0,
        material: None,
        wind: false,
    }
}

#[test]
fn the_body_pose_gives_back_the_reference_pose() {
    let mut w = PhysWorld::new(PhysParams::default());
    let angles = [0.3, -0.2, 1.0];
    let pose = RefPose { form_id: DROPPED, position: [10.0, 20.0, 30.0], rotation: ref_rotation(angles), scale: 1.0 };
    let local = Iso { translation: [1.0, 2.0, 3.0], rotation: axis(2, 0.5) };
    w.add_ref(&pose, &[object(Motion::Static, 2.0, local)]);
    let b = w.ref_bodies(DROPPED)[0];
    // The body is not at the reference's origin ...
    assert!((b.translation[0] - 10.0).abs() > 0.5);
    // ... but gives it back.
    let (pos, rot) = ref_pose_from_body(&b, &local, 1.0);
    for i in 0..3 {
        assert!((pos[i] - pose.position[i]).abs() < 1e-3, "{pos:?}");
    }
    assert!(max_diff(&rot, &pose.rotation) < 1e-5);
    assert!(max_diff(&ref_rotation(ref_angles(&rot)), &pose.rotation) < 1e-5);
}

/// A world holding one created reference (as a snapshot would restore it).
fn world_with(s: RefState) -> World<'static> {
    let forms: &'static FormStore = Box::leak(Box::default());
    let refs: &'static RefTable = Box::leak(Box::default());
    let snap = Snapshot {
        format: SNAPSHOT_FORMAT.into(),
        config: SimConfig::default(),
        rng_word_pos: "0".into(),
        next_dynamic: DROPPED + 1,
        refs: [(FormId(DROPPED), s)].into_iter().collect(),
        ..Default::default()
    };
    World::restore(forms, refs, &snap).unwrap()
}

#[test]
fn a_dynamic_drop_falls_settles_and_is_written_back() {
    let ix = index();
    // Dropped 30 above a floor whose top is at z 10, near the east edge of square (0, 0).
    let start = [CELL_SIZE - 40.0, 100.0, 40.0];
    let mut sim = world_with(state(Some(CELL_00), start));
    let mut phys = PhysState::default();
    let floor = RefPose { form_id: 0x500, position: [CELL_SIZE - 40.0, 100.0, 0.0], rotation: ref_rotation([0.0; 3]), scale: 1.0 };
    phys.world.add_ref(&floor, &[object(Motion::Static, 10.0, Iso::IDENTITY).clone()]);
    // The collision body sits 2 above the reference's origin.
    let local = Iso { translation: [0.0, 0.0, 2.0], rotation: Iso::IDENTITY.rotation };
    let pose = RefPose { form_id: DROPPED, position: start, rotation: ref_rotation([0.0; 3]), scale: 1.0 };
    phys.world.add_ref(&pose, &[object(Motion::Dynamic, 3.0, local)]);
    phys.created.insert(DROPPED, CreatedBody { local: Some(local), dynamic: true, start, added_step: 0, settled: false });
    let mut moved_once = false;
    for _ in 0..600 {
        phys.world.step();
        moved_once |= phys.settle_created(&ix, WORLD, &mut sim).contains(&DROPPED);
    }
    assert!(moved_once);
    let s = sim.peek(FormId(DROPPED)).unwrap().clone();
    // Body centre on the floor top + its half size; the reference 2 below its body.
    assert!((s.pos[2] - (10.0 + 3.0 - 2.0)).abs() < 1.0, "rests at {:?}", s.pos);
    // Straight down (rapier lets the box drift about a unit on landing).
    assert!((s.pos[0] - start[0]).abs() < 2.0 && (s.pos[1] - start[1]).abs() < 2.0, "fell straight: {:?}", s.pos);
    assert_eq!(s.cell, Some(FormId(CELL_00)));
    assert!(phys.created[&DROPPED].settled, "its body went to sleep");
    // At rest nothing more is written.
    phys.world.step();
    assert!(phys.settle_created(&ix, WORLD, &mut sim).is_empty());
    // A snapshot keeps where it came to rest.
    let again = World::restore(sim.forms, sim.placed, &sim.snapshot()).unwrap();
    assert_eq!(again.peek(FormId(DROPPED)).unwrap().pos, s.pos);
}

#[test]
fn a_body_that_moves_into_the_next_square_changes_cell() {
    let ix = index();
    let start = [CELL_SIZE - 1.0, 100.0, 40.0];
    let mut sim = world_with(state(Some(CELL_00), start));
    let mut phys = PhysState::default();
    let pose = RefPose { form_id: DROPPED, position: start, rotation: ref_rotation([0.0; 3]), scale: 1.0 };
    // Not dynamic in the model: nothing is written back.
    phys.world.add_ref(&pose, &[object(Motion::Static, 3.0, Iso::IDENTITY)]);
    phys.created.insert(DROPPED, CreatedBody { local: Some(Iso::IDENTITY), dynamic: false, start, added_step: 0, settled: false });
    assert!(phys.settle_created(&ix, WORLD, &mut sim).is_empty());
    // Dynamic, with no floor, pushed east: written back with the new square's cell.
    phys.world.remove_ref(DROPPED);
    phys.world.add_ref(&pose, &[object(Motion::Dynamic, 3.0, Iso::IDENTITY)]);
    phys.created.get_mut(&DROPPED).unwrap().dynamic = true;
    phys.world.step();
    let mut pushed = pose;
    pushed.position[0] = CELL_SIZE + 5.0;
    phys.world.add_ref(&pushed, &[object(Motion::Dynamic, 3.0, Iso::IDENTITY)]);
    phys.world.step();
    assert_eq!(phys.settle_created(&ix, WORLD, &mut sim), vec![DROPPED]);
    assert_eq!(sim.peek(FormId(DROPPED)).unwrap().cell, Some(FormId(CELL_10)));
}

#[test]
fn falling_through_the_world_puts_it_back_without_collision() {
    let ix = index();
    let start = [100.0, 100.0, 40.0];
    let mut sim = world_with(state(Some(CELL_00), start));
    let mut phys = PhysState::default();
    let mut pose = RefPose { form_id: DROPPED, position: start, rotation: ref_rotation([0.0; 3]), scale: 1.0 };
    pose.position[2] = start[2] - crate::phys::CREATED_FELL_THROUGH - 100.0;
    phys.world.add_ref(&pose, &[object(Motion::Dynamic, 3.0, Iso::IDENTITY)]);
    phys.created.insert(DROPPED, CreatedBody { local: Some(Iso::IDENTITY), dynamic: true, start, added_step: 0, settled: false });
    phys.world.step();
    phys.settle_created(&ix, WORLD, &mut sim);
    assert!(!phys.has_collision(DROPPED));
    assert!(!phys.created.contains_key(&DROPPED));
    assert_eq!(sim.peek(FormId(DROPPED)).unwrap().pos, start);
}

#[test]
fn drops_in_another_worldspace_are_not_drawn_or_colliding() {
    let ix = index();
    // Dropped in WORLD's square (0, 0); the player then travels into worldspace 0x99,
    // whose square (0, 0) is loaded instead.
    let s = state(Some(CELL_00), [10.0, 10.0, 0.0]);
    let here = RenderCell::Exterior { world: 0x99, x: 0, y: 0 };
    let wanted = Placed::wanted(&ix, [(FormId(DROPPED), &s)].into_iter());
    assert_eq!(wanted[&DROPPED].cell, RenderCell::Exterior { world: WORLD, x: 0, y: 0 });
    assert_eq!(plan(&wanted, &BTreeMap::new(), |c| c == here), Plan::default());
    // Drawn before the travel: despawned when only the other worldspace is loaded.
    assert_eq!(plan(&wanted, &wanted, |c| c == here), Plan { despawn: vec![DROPPED], ..Default::default() });
    assert_eq!(phys_cell(&ix, 0x99, Space::Exterior, &s), None);
    assert_eq!(phys_cell(&ix, WORLD, Space::Exterior, &s), Some(PhysCell::Exterior(0, 0)));
}

/// A dropped modded weapon draws the third-person model of its mods; no mods (or no
/// weapon record) leaves the plain world model (weapon-mods.md §3 "In the world").
#[test]
fn a_dropped_modded_weapon_draws_its_mods_model() {
    let mut w = fnv_actor::records::Weapon {
        edid: None,
        model: Some(r"Weapons\2HandRifle\VarmintRifle.NIF".into()),
        anim_type: None,
        damage: 0,
        projectile: None,
        explosion_damage: 0.0,
        skill: None,
        embedded: false,
        non_playable: false,
        not_for_npcs: false,
        first_person: None,
        first_person_model: Some("1st.nif".into()),
        mod_models: Default::default(),
        mod_first_person: [None; 7],
        mod_first_person_models: Default::default(),
        held_world_model: None,
        grip: None,
        hide_backpack: false,
        addons: Vec::new(),
    };
    w.mod_models[1] = Some(r"Weapons\2HandRifle\VarmintRifleScp.NIF".into());
    assert_eq!(modded_model(Some(&w), 2).as_deref(), Some(r"meshes\weapons\2handrifle\varmintriflescp.nif"));
    // A combination without its own model: the plain MODL; never the first-person one.
    assert_eq!(modded_model(Some(&w), 4).as_deref(), Some(r"meshes\weapons\2handrifle\varmintrifle.nif"));
    assert_eq!(modded_model(Some(&w), 0), None);
    assert_eq!(modded_model(None, 2), None);
}

/// Issue #1617: created references go by the filter placed ones do. Markers and bases of
/// a type not drawn (PlaceAtMe of an explosion) draw nothing; items keep their models,
/// a modded weapon its mods' one, and the collision's model is not touched.
#[test]
fn a_created_marker_or_explosion_draws_nothing_and_items_their_models() {
    use crate::script_draw::tests::{play_over, record, sub, top};
    const HEADING: u32 = 0x100; // meshes\markerxheading.nif
    const IN_MARKERS: u32 = 0x101; // meshes\markers\...
    const CLUTTER: u32 = 0x102;
    const MINE_BLAST: u32 = 0x110;
    const CAN: u32 = 0x120;
    const RIFLE: u32 = 0x130;
    const NOTHING: u32 = 0x999;
    let string = |sig: &[u8; 4], s: &str| sub(sig, [s.as_bytes(), &[0]].concat());
    let play = play_over(&[
        top(b"STAT", &[record(b"STAT", 0, HEADING, &[string(b"MODL", "MarkerXHeading.NIF")]), record(b"STAT", 0, IN_MARKERS, &[string(b"MODL", "Markers\\Spot.NIF")]), record(b"STAT", 0, CLUTTER, &[string(b"MODL", "Clutter\\Rock.NIF")])].concat()),
        top(b"EXPL", &record(b"EXPL", 0, MINE_BLAST, &[string(b"MODL", "Effects\\ExplosionGrenadeFrag.NIF")])),
        top(b"MISC", &record(b"MISC", 0, CAN, &[string(b"MODL", "Clutter\\Can.NIF")])),
        top(b"WEAP", &record(b"WEAP", 0, RIFLE, &[string(b"MODL", "Weapons\\Rifle.NIF"), string(b"MWD2", "Weapons\\RifleScope.NIF")])),
    ]);
    for (base, why) in [(HEADING, "a marker by its file name"), (IN_MARKERS, "in the markers folder"), (MINE_BLAST, "an explosion")] {
        assert!(!draws_model(&play, FormId(base), 0), "{why}");
        assert!(model_path(&play, FormId(base), 0).is_some(), "{why}: physics still asks for its model");
    }
    assert!(draws_model(&play, FormId(CLUTTER), 0));
    assert!(draws_model(&play, FormId(CAN), 0));
    assert_eq!(model_path(&play, FormId(CAN), 0).as_deref(), Some(r"meshes\clutter\can.nif"));
    assert!(draws_model(&play, FormId(RIFLE), 0));
    assert_eq!(model_path(&play, FormId(RIFLE), 0).as_deref(), Some(r"meshes\weapons\rifle.nif"));
    assert!(draws_model(&play, FormId(RIFLE), 2));
    assert_eq!(model_path(&play, FormId(RIFLE), 2).as_deref(), Some(r"meshes\weapons\riflescope.nif"));
    // No record at all: left to the warning that has no model to draw.
    assert!(draws_model(&play, FormId(NOTHING), 0));
    assert!(model_path(&play, FormId(NOTHING), 0).is_none());
}
