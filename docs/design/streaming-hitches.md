# Open-world streaming hitches

What stalls a frame while the player moves through the Mojave, how it was measured, and
what has been moved off the main thread. Owner request 2026-10-22: heavy stutter while
new cells load around the player (`--radius 2`, the 5 × 5 grid of the game's
uGridsToLoad=5).

## 1. Measurements (2026-10-22, b03ad71a)

- **Tour stress run** (`python tools/stress run tour`, the towns route): frames mean
  18 ms, 1 % low 122 ms, 49 frames over 250 ms (worst 775 ms) [observed:
  target/stress/tour/b03ad71a, private]. The cell load itself (`world::load_cell`, which
  reads, inflates and parses the cell's models and textures) took 2–18 ms in those frames
  [observed: the `stress hitch:` lines]. Archive decompression is not the cause.
- **Bevy `trace_chrome`** (built with `--features bevy/trace_chrome`): blamed
  `residents::drive`, `outfits::follow_inventory`, `anim_state::tell`,
  `creature_pace::give_speeds` and `stress_sample::sample`. The last three show almost no
  CPU samples in the profile below, so its per-system attribution of hitches is not to be
  trusted [observed]. A full lap writes a 61 GB trace; tracing made frames 2.4 × slower.
- **CPU sampling** (xperf kernel profile with stacks, 1 kHz, the release build's PDB;
  `tools/stress` tour over waypoints 52–60 and 26–31 of that route): in the hitch frames
  the busiest thread was about 77 % in `nav::show_actor` → `actors::spawn` →
  `part_uses`, of which [observed]:
  - about 55 % `FaceGenCache::face_texture` / `skin_texture` → `fnv_actor::facegen::
    add_offset`: the per-NPC tinted face and body textures, a full-resolution pass per
    texture, two per NPC, made the frame the NPC is first drawn;
  - about 13 % `ActorCtx::prepare_part`: reading, inflating and parsing the NPC's body,
    armour and creature models and loading their textures;
  - `world::load_cell` about 4 %; pathfinding (`NavGraph::search`) under 1 %.
  The 1 s hitch just after the start is `lodgen::GenJob::run` (generated distant terrain)
  on every worker thread at once.

  The recipe, for the next profile: System32 `wpr` 10.0.19041 fails `-stop` with
  0x80010106 and drops the recording; use the Windows Kits `xperf -on
  PROC_THREAD+LOADER+PROFILE -stackwalk Profile` and `xperf -d`, elevated, then `xperf -a
  dumper -symbols` with `_NT_SYMBOL_PATH` at `engine/target/release`.

- **Cells read ahead and spawned over frames** (§2 "Cells"), `python tools/stress run
  tour`, one run after another on the same machine (other sessions' builds and games
  ran beside them, so single numbers are noisy) [observed: target/stress/tour/*,
  private]. "Main" is origin/main with the FaceGen and actor-part work below (b5333ddb,
  PR #587); "before #587" is f28edbaa.

  | run | build | mean | 1 % low | worst | frames over 33 ms (`physics streaming:` line) |
  |---|---|---|---|---|---|
  | towns lap | before #587 | 17.46 ms | 96.8 ms | 560 ms | 430 of 46,734 |
  | towns lap | main | 17.15 ms | 52.6 ms | 236 ms | 635 of 47,535 |
  | towns lap | this branch | 16.86 ms | 34.1 ms | 249 ms | 155 of 48,305 |
  | slice 52:60 | main | 16.93 ms | 57.8 ms | 1,137 ms | 78 of 9,766 |
  | slice 52:60 | this branch | 16.94 ms | 50.0 ms | 1,168 ms | 28 of 9,809 |
  | slice 26:31 | main | 17.78 ms | 81.6 ms | 1,005 ms | 73 of 5,519 |
  | slice 26:31 | this branch | 17.13 ms | 65.9 ms | 1,073 ms | 34 of 5,716 |

  The slices' worst frames are the start's `lodgen` burst (§3). On the lap, 1,190 cells
  loaded with 1,060 ms of main-thread time, over 1,201 frames; 9 waited for their
  read-ahead and none was drawn before it was done (`streaming summary:` at the exit).
  No count rose in "Possible leaks"; the caches end the lap higher by design (§2
  "Caches", docs/design/stress-tests.md §4).

## 2. What runs off the main thread now

- **FaceGen textures**: no longer made (§3). Between PR #587 and the shader tint the
  tinted copies were made on a worker thread and put into the materials a few frames
  after the NPC appeared.
- **Actor parts** (`nvf-assets/src/actor.rs`). Before an actor is drawn for the first
  time, `readahead::ready_to_draw` asks `ActorCtx::parts_ready` for its part models, its
  weapon's model and its skeleton; those not prepared or read yet are read, inflated and
  parsed on a worker thread, with the texture files their texture sets name
  (`TextureCache::offer`). The actor is drawn once they are in and, unless a loading
  screen is up, prepared by the read-ahead's queue within its budget (below) [engineering
  choice: a few frames later, at the edge of the loaded cells]. Drawn as soon as its parts
  were read, a resident who came into an interior after the start (which no ring lists)
  had its parts prepared in the frame it was drawn: one frame of 15.6 ms in the Sierra
  Madre lobby [observed 2026-11-03, #1429]. Residents and travellers
  (`nav::show_actor`) wait so; so do the placed actors of a cell loaded after the start
  (`world::load_cell` puts them in `Streamer::waiting`, `world::draw_waiting_actors`
  draws them); before the start's cells are in, behind the loading screen, they are drawn
  at once. Preparing a part (meshes, materials, GPU textures) needs the asset stores and
  stays on the main thread.
- **Read-ahead** (`nvforever/src/readahead.rs`). When the cell the loaded grid is around
  changes (outdoors, after the start), the actors of the band just outside the grid (the
  ring between the loaded 5 × 5 cells and 7 × 7: placed NPCs and creatures enabled there,
  and residents not drawn whose walkers stand there) are listed. Within 4 ms a frame
  (`PREPARE_BUDGET`) they are resolved, the reads above started for them, and what has
  been read prepared on the main thread (`ActorCtx::part`, `ActorCtx::skeleton`). An
  NPC's FaceGen files (the `.egm` beside each part, the head's `.egt`, the race's body
  and hand `.egt`) are read on a worker thread and offered to `FaceGenCache`
  (`facegen::read_file`, `FaceGenCache::offer`). Once an actor's skeleton is
  prepared, the clip sets and weapon stance it is drawn with (standing, sneaking,
  swimming, limping, a creature's alert set; `actors::Anims::warm`) are loaded from their
  KF files on a worker thread through `ClipReader` (`nvf_assets::actor::ClipSource`: the
  loaders in `locomotion.rs` and `weapons.rs` read clips through it). `Anims` does not
  hold its lock while loading, so drawing an actor never waits for a worker; both may
  load a set once and the first kept wins [engineering choice]. A weapon's `UPB` node
  strings are cached (`ModelCtx::node_strings`) instead of reading the model again for
  every NPC holding it.
- **Idle clips** (`idles.rs`, #1429). An idle asked for whose KF file is not loaded yet
  waits for it to be read on a worker thread (`ActorCtx::clip_ready`), for at most
  `CLIP_WAIT` (0.5 s), after which it is read in the frame it starts; a collection's next
  idle has its file read as soon as it is picked, while the actor waits to play it.
  Reading it in the frame it started cost one 8.4 ms frame at the Gomorrah floor
  [observed 2026-11-03]. A run ends with `idle clip summary: …` (idles that waited,
  the longest wait, those read late).

  An actor walking into the loaded cells, or a cell of them loading, then finds its
  parts, skeleton and clips ready: what is left on the frame it is drawn is resolving it,
  the per-NPC FaceGen shape (`FaceGenCache::morph`) and tint offsets (§3), retextured
  materials (`ActorCtx::material_with_texture`) and the entities. Parts prepared ahead and
  not used go after the caches' minute (`nvf_assets::release`): a player standing still at
  the edge of a ring has its actors read again when they come.

  Measuring: `stress hitch:` lines say how many actors a slow frame drew and their time
  (`readahead::clocked`); a run ends with `stress actors: …` (the frames that drew actors)
  and `actor read-ahead summary: …` (actors listed, parts read and prepared ahead, clip
  sets loaded ahead, actors ready at once and after waiting). `NVF_PLAY_TEST_NO_READ_AHEAD=1`
  turns the read-ahead off for a comparison (`python tools/stress run tour … --aid
  NVF_PLAY_TEST_NO_READ_AHEAD=1`). Smoke: `actor-read-ahead` (residents),
  `actor-read-ahead-placed` (cells' placed actors).

  Tour slices 52:60 and 26:31 of the towns route, one build (this branch, on the
  FaceGen shader tint), read-ahead on and off [observed, 2026-10-22,
  target/stress/tour/final-{on,off}-*, private]:

  | slice | read-ahead | actors drawn | their time | most in a frame | ready at once | worst frame drawing actors | 1 % low |
  |---|---|---|---|---|---|---|---|
  | 52:60 | on | 140 | 476 ms | 60 ms | 129 | 89 ms | 59.1 ms |
  | 52:60 | off | 140 | 1205 ms | 123 ms | 79 | 91 ms | 63.8 ms |
  | 26:31 | on | 133 | 420 ms | 63 ms | 127 | 92 ms | 68.8 ms |
  | 26:31 | off | 133 | 1208 ms | 125 ms | 64 | 126 ms | 74.4 ms |

  The read-ahead itself took 854 ms over 279 frames (52:60) and 870 ms over 208 frames
  (26:31), at most 19 and 26 ms in a frame (one part's preparation runs over the
  budget); it read 78 and 83 FaceGen files and 16 and 22 skeletons' clip sets ahead. Since PR #587 no frame over 250 ms comes from drawing actors in either slice,
  with or without the read-ahead; the one left in each is `lodgen` at the start (§1).
  With the read-ahead on, the actors that still waited for their parts had not stood in
  a ring before they came into range (a resident walking in from beyond it)
  [hypothesis: not traced per actor].

- **Cells** (`nvforever/src/world.rs`, `nvf-assets/src/model.rs`, `texture.rs`;
  `CellSpawn`, `ReadAhead`). Before the start square is in, and behind a loading screen,
  cells load whole in the frame they are reached, as before. After it:
  - *Read-ahead.* The cells up to one step beyond the loaded ones (the 7 × 7 square
    around the camera's cell with `--radius 2`) are read on a worker thread: the
    distinct models their references draw (`cell_models`) and their landscape (LAND
    read, quadrant meshes built: `terrain::prepare_terrain`). Each model not prepared yet
    is then prepared on a worker thread (`ModelCtx::models_ready`): read, inflated,
    parsed, meshes built (tangents included), textures decoded. The worker puts its
    meshes, materials and skins under handles reserved from the asset stores; the main
    thread puts them in (`ModelCtx::take_fetched`, PreUpdate, at most 2 ms a frame) and
    the model joins the cache. Textures a worker decodes go into a list shared with the
    other workers and the main thread (`texture::WorkerTextures`), so each is decoded
    once and keeps one handle; a texture the cache already holds is found in a copy of
    the cache's list sent with the work.
  - *Spawning over frames.* A cell is started once its models are all prepared (or after
    1.5 s, preparing what is missing on the main thread). Its references are spawned
    within 3 ms of main-thread time a frame, nearest cell first; the cell's entity is
    hidden until the last one is in, then joins `Streamer::loaded` and fades in (fade.rs)
    as a cell loaded in one frame did. Nothing that reads `loaded` (LOD, residents, cell
    buffer, decals) sees a half-drawn cell; the LOD stands in for it meanwhile.
  - *Despawning in slices.* A cell that has faded out is hidden, loses its `RenderCell`
    and is despawned 400 references a frame.
  - *GPU uploads* are capped at 24 MiB a frame (Bevy's `RenderAssetBytesPerFrame`), not
    behind a loading screen or before the start square is in.
  - *Caches.* Unused models, actor parts and textures stay 300 s (was 60 s), at most 2,048
    of each (was 256): models read ahead wait unused until their cell loads, and turning
    back finds the cells left behind still prepared.

- **The reads' own pool** (#1732, `nvf_assets::read_pool`). Everything above that runs "on
  a worker thread" for cells, models, textures, actor parts, clips and `.egt` files runs
  on a pool of the game's own, not on Bevy's async compute pool, where pipelines compile
  and a frame can wait on one (§3). It has as many threads as that pool, 25 % of the
  logical threads, 1 to 4 [documented: bevy_app 0.20.1 `task_pool_plugin.rs`] [design], so
  the reads keep the size they had and share it with nothing. Bevy's I/O pool was not
  used: the embedded shaders load on it at the same moment, and sound decodes went there
  to get away from these reads [design]. Size test aid: `NVF_PLAY_TEST_READ_THREADS`.
  Not run or measured yet when this was written.

## 3. Left to do

- Done: the FaceGen tint is added in the face and skin shader (`nvf-assets`
  `nif_material.rs`, "FaceGen tint"; `nvforever/src/facegen.rs`). Per NPC only the
  `.egt` offset image is made, on the main thread when the NPC's parts are put together:
  the face's 256 × 256 and the body's and hands' 32 × 32, as 16-bit floats (512 KB and
  8 KB), in place of a 1024² face and a 512² body copy with mipmaps (about 5.6 MB and
  1.4 MB). The shader adds the offset, upsampled bilinearly and flipped vertically, to the
  texel's stored sRGB value and holds it to 0–255, as the CPU add did; it no longer
  rounds to whole units, and it holds after the texture is filtered rather than per
  texel. A unit test checks the shader's formula against `add_offset` texel by texel
  (within the CPU's rounding). Close frames of Sunny Smiles from the dialogue camera
  (actors.md §4.3), 1280 × 720, shader against the CPU version, in display units: on
  her face a mean difference of 0.29 (99th percentile 2, largest 7), on her hands 0.09
  (largest 2); the tint itself (the same frames against `--no-facegen`) moves the face
  by 10.45 on average and the hands by 2.75 [observed, 2026-10-22]. The body skin is now
  drawn from the full 1024² texture (the CPU copy was halved to 512²), so it is
  slightly sharper up close. The NPC is drawn tinted from its first frame.
  Main-thread cost, measured in the Prospector Saloon (`facegen-tint-shader` smoke):
  5.1 ms for 3 face and 5 skin offsets (about 1.6 ms per face; the sum is per colour
  plane so it vectorises), and 27 ms reading the `.egt` files once per file (the head
  file is about 10 MB; the exit line's "FaceGen:" figures).
- The `.egt` files of actors in the read-ahead's ring are read on a worker thread (§2);
  the NPCs of the start's cells, and any not read ahead, still read theirs on the main
  thread the first time (up to about 20 ms per file, once).
- Actors re-dressed (`outfits.rs`) still prepare a newly worn part on first use; the
  read-ahead resolves an actor's appearance from its record, not its inventory. Interiors
  and doors' destinations are not read ahead.
- Cells: grass (`grass.rs` `place` and the merged meshes), SpeedTree trees, light and
  placeable-water models and actors' parts in `world::load_cell` are still made on the
  main thread when the cell is spawned (within the spawning budget, but one large piece
  can overrun it).
- ~~`lodgen` should leave cores free for the frame.~~ Done (branch claude/lodgen-throttle,
  2026-10-22):
  - **Why the frame stalled.** Not core starvation: Bevy's async compute pool has 4
    threads here (25 % of 32, at most 4), and the machine had cores to spare. The frame
    *waited* for the bakes. Bevy 0.19 compiles render pipelines as tasks on that same pool
    (`PipelineCache`, `create_pipeline_task`), and the upscaling pass `block_on`s its
    pipeline when a view first needs one ("ensure the pipeline is loaded before
    continuing the frame") [documented: bevy_render 0.19.1 `render_resource/
    pipeline_cache.rs`, bevy_core_pipeline 0.19.1 `upscaling/mod.rs`]. With every pool
    thread on a 200–600 ms bake, such a compile queues behind all of them [hypothesis:
    the blocked compile was not caught in a profile]; what was seen is that a 4.8 s frame
    on a full lap began as eight bakes were queued and ended 0.15 s after the last of
    them finished [observed: per-bake timing in a build of f28edbaa,
    target/stress/tour/diag-52, private]. The archive lock (`fnv_vfs::Vfs::read` holds an
    archive's mutex while it inflates) was ruled out: 12 waits over 5 ms in that lap
    [observed]. Since #997 the mutex covers only the seek and read of the stored bytes
    and the inflate runs after it is let go (`Archive::read_stored`, `StoredBlock::decode`);
    the waits were not measured again.
  - **What changed.** Bakes run on two threads of their own at the lowest thread priority
    (`lodgen::Baker`, `lod.rs` `BAKE_THREADS`), off the async compute pool, so pipeline
    compiles, FaceGen tints and actor part reads never queue behind them, and they only
    take cores nothing else wants. The queue is still nearest first and is replaced when
    the camera's cell changes; a chunk being baked finishes. Finished bakes are taken two
    a frame (`BAKES_PER_FRAME`) [engineering choices].
  - **Result** (`tools/stress` tour, the waypoint 52–60 and 26–31 routes of §1, b5333ddb
    with and without the change, fresh cache each run) [observed: target/stress/tour
    before-* and after-*, private]:

    | stretch | worst frame before | after | 1 % low before | after |
    |---|---|---|---|---|
    | 52–60 | 1214 ms (t 6.2 s) | 210 ms | 62 ms | 60 ms |
    | 26–31 | 1041 ms (t 5.5 s) | 995 ms (t 5.1 s) | 72 ms | 80 ms |

    The 26–31 frame left is the jump itself, under the loading screen: with `--no-lodgen`
    the same frame is 1005 ms [observed: nolod-26-31]. On the full lap the worst frame
    went from 4844 ms to 1243 ms (that one with no bake waited on). Generated terrain
    still fills in after a jump: `--screenshot` (which waits until nothing is baking)
    from two high views with a cold cache drew 56 generated chunks, the same picture as
    before the change [observed].
- The jump's own ~1 s frame (26–31 above, with or without generated terrain): not looked
  into yet.
- Done (branch claude/perf-hitch-capture, 2026-10-27): the 1.2 s frame entering cell
  -16,-13. The span profile's own line for that frame (`profile hitch:`, profile.rs) had
  1,305 of its 1,344 ms in Bevy's `upscaling::prepare_view_upscaling_pipelines`, which
  blocks on a view's blit pipeline the first time a view with a new key is drawn
  [documented: bevy_core_pipeline 0.19.1 `upscaling/mod.rs`; observed: tools/stress tour
  `--towns Goodsprings Primm`, a profile build]. The new view is the water reflection
  camera (reflection.rs), which switches on there as Primm's water comes near; its compile
  queued behind the place's own shader compiles. This is the stall the lodgen item above
  had as its hypothesis. `blit_warm.rs` specializes every upscaling key the game's cameras
  can ask for (6 formats, written and blended) on the render app's first frame, without
  waiting. Result, the same tour: worst frame 1,344 → 173 ms, 1 % low 56 → 31 ms
  [observed]. The same stall would have come with any camera's first use (scope, Pip-Boy,
  terminal and table views).
- Done (same branch): `lod::update_lod` rebuilt every terrain chunk and object block
  whose loaded cells changed in one frame, 60–71 ms three times on that tour [observed].
  The rebuilds now stop at 4 ms a frame (`REBUILD_BUDGET_MS`, at least one a frame); the
  old meshes stay drawn until theirs come round. Terrain runs first and could spend the
  whole budget, putting off every new object block while flying fast; since #1058 the
  first block of each frame goes through whatever terrain spent. On the `auto-fly` smoke
  run (Goodsprings, 4000 units/s, 40 s) a square was then put off at most 12 frames in a
  row and none was still waiting at the end [observed, 2026-10-31; not measured before
  the change].

## 3.1 Slower at night, and the longer the session (2026-10-28, 7c96203d)

The owner's report: heavy stutter about 22 minutes into a session, gone after a restart,
wherever it happened (the Powder Ganger camp, Novac). Both onsets came right after the
weather rolled to `NVWastelandClearNight` [observed: the owner's run logs, `weather roll`
at 17:28:58, stutter from 17:30:33]; loading the save made there ran at a flat 60 fps
[observed]. The frames went from 16.7 ms to 27-40 ms (vsync halving) with 250-800 ms
stalls, with the processor and the GPU both half idle [observed: thread CPU and
nvidia-smi during the stutter; 6 of 12 GB of video memory].

Measured on two-lap tours with the owner's settings (`tools/stress run tour --laps 2
--ini <NVForever.ini> --aid NVF_PLAY_TEST_PROFILE=5`, a profile build; lap 2 is the
evening and night, the tours start at noon) [observed, target/stress/tour/*, private]:

- `material::write_material_bind_group_buffers` follows the game hour: about 0.5 ms a
  frame at noon, 3-5 ms from 19:00 to midnight, with run-to-run spread. Bevy sends a whole
  bindless slab again when any material in it changes [documented: bevy_pbr 0.19.1
  `material_bind_groups.rs`, `RetainedRawBufferVec::write`], and materials changed every
  frame (animated glow, `material_anim::animate_rig_materials`; colours driven by the
  hour, `emittance.rs`) each dirty a slab of up to 2,048 resources shared with static
  ones. The render graph and extraction grow with them.
- **Done** (branch claude/novac-stutter): `NifSurface` slabs hold at most 512 resources
  (`#[bindless(limit(512))]`): render graph 30-40 % cheaper at every hour, material writes
  about halved, 1 % low 56 → 50 ms, no material-write stall; 154 multi-drawn sets against
  108 (more slabs, fewer resources bound per draw).
- Not the cause: the place (a fresh load there is smooth), the asset caches' keep (a 60 s,
  512-entry keep held 40 % fewer meshes and images and lap 2 still slowed), video memory
  (it levels off at 5.4-6 GB walking; it climbed to 7-9 GB only with the soak's events).
- Left to do, patches kept outside the repository (`F:/nvf-wt/perf-hitch-patches-2026-10-28`):
  - `emittance.rs` never let its driven material copies go (`EmittanceState::driven`
    only grew; each copy held its material and textures for the session) and wrote
    them all at every game minute and fade step whether the colour changed or not. Pruning
    them and writing only changes is right in itself, but a two-lap tour with it had 27
    frames of 100-470 ms material writes (none without it): freeing copies churned the
    slabs. **Done** in #1169 (issue #904), after #898 kept emptied GPU memory blocks:
    copies no entity holds go every 2 s, and only changed colours are written. Frame
    times against main: #1169 gives the tours, their dates and the machine's load.
  - Per-frame work over every actor the AI ever took in (`Core::list` and `Ai::agents`
    only grow): `Game::sync_loaded_actor_effects` syncs every actor that ever ran a
    package (`package_actors`), against its own comment (loaded actors only);
    `Ai::keep_apart` tests every pair of agents (a grid gives the same pairs); far
    residents run the whole draw-and-place step. Each fix measured alone on a dusk lap
    showed no harm, but together on two-lap tours they came with 45-154 frames of material
    write stalls; not understood yet.
  - Why material writes stall for 100-500 ms at all (slab growth and reallocation is the
    likeliest) and the 400-430 ms `window::prepare_windows` waits.

## 4. Combat and interiors (2026-10-23, 5022d345)

Measured the same way (xperf CPU sampling; `tools/stress run battle`, standing in five
busy interiors, `tools/stress run cells` over 120 interiors) [observed: target/stress/
battle/perf-battle*, private].

- **Interiors** (Gomorrah00, TOPSCasino, Vault3a, RocketLabMid, Vault19a): about 17 ms a
  frame, the 60 fps vsync cap, 1 % low 68–95 ms; the only frames over 250 ms are 1–2 s
  after the launch. Interior loads: mean 0.32 s, slowest 0.76 s (RocketLabBasement).
  Nothing there is held by the main thread.
- **Battles**: 25 a side, mean 24 ms and 1 % low 303 ms; 50 a side, mean 181 ms and 1 %
  low 1,487 ms. `detection::measure`, one system on one thread, took 262 and 774 ms of
  CPU a second. Inside it, per pair of actors: the actors' actor values, levels and armour
  (`detection_pair`) and their factions (`factions_of`, through leveled templates:
  `Picks::pick`, `World::level_at`) for every hostility check, and four sight rays
  (`Queries::ray`). The main thread was nearly idle; the frame waited on this system.

What changed (branch claude/detection-perf):

- A **detection round** (`Game::begin_detection_round`, `fnv-game` `detection.rs`): while
  the engine measures, each actor's detection inputs are worked out once, who would attack
  whom once per pair up to the hits and allies step (`end_detection_hostility`: hits that
  provoke and allies taking up a fight change it), and each actor's factions once for the
  whole round. Scripts that change factions forget the actor's (AddToFaction and the rest).
- **Sight rays at once**: the pair, camera, sun and event rays are gathered and cast on
  the compute pool (`detection::clear_sights`, read-only `Queries`), then the game logic
  runs over the results in the order it did.

Result (same battles, untraced) [observed: target/stress/battle/after2-*, private]:

| battle | mean before | after | 1 % low before | after | worst before | after |
|---|---|---|---|---|---|---|
| 25 a side | 24 ms | 17 ms | 303 ms | 65 ms | 382 ms | 100 ms |
| 50 a side | 181 ms | 54 ms | 1,487 ms | 427 ms | 1,596 ms | 453 ms |

At 50 a side the serial part of `measure` fell from 774 to 385 ms a second (traced);
what is left is mostly `Game::call_allies`, whose hostility checks cannot share the
round's (it adds grudges as it goes), and its faction reactions. The next largest cost is
the NPCs' physics movement (`PhysWorld::move_character`, about 300 ms a second).

Then (branch claude/combat-perf):

- **Allies** (`Game::call_allies`): who detects whom is indexed once a round instead of
  scanned per fighter (a million pair checks a round at 50 a side), each helper's
  Assistance read once, and each actor's faction relation to another
  (`functions::actor::faction_relation`) kept for the round. SetEnemy, SetAlly,
  SetPCEnemyOfFaction, ClearFactionPlayerEnemyFlag, a crime that makes the player a
  faction's enemy and the faction changes forget them.
- **Characters' moves** (`PhysWorld::plan_move` / `apply_move`, `fnv-phys`): a move only
  reads the world until it sets the character's next kinematic position (taken at the
  step) and pushes loose bodies (velocities no move reads), so the walked actors' moves
  are planned at once on the compute pool and applied in turn, as each would have been
  alone (`nvf-play` `player.rs`). The player's move is as before.

At 50 a side (traced, the same battle): mean frame 98 → 42 ms; `detection::measure` on its
thread 385 → 269 ms of CPU a second; the moves (`plan_move`, 337 ms a second) now spread
over the pool, 36 ms a second left on one thread (the player's own) [observed:
target/stress/battle/prof-*, private].

Then (branch claude/ai-rays-cache):

- **Fewer sight rays** (`nvforever` `detection.rs`), each with the answer the rays gave
  before:
  - A line of sight first casts one ray that ignores the actors (`Filter::no_actors`):
    where it meets nothing, the walk past the actors' capsules (one or two rays for every
    actor on the line, up to `PAST_ACTORS`) could only end clear, so it is not cast. Only
    a line that meets something walks as before.
  - One ray answers every asker of the same two points, either way round
    (`detection::sights`): A's eyes to B's eyes serves B's eyes to A's (the second step of
    both, after both chests were hidden), and a detection event's two points are one, so
    its second step is its first. That the world blocks a line whichever end it is cast
    from is a [hypothesis] checked with the sight check below.
- **Still pairs** keep their line of sight for the next measurement when neither the
  detector's eyes (or the player's camera) nor the target's chest and eyes moved more
  than `STILL` (4 units) since it was cast; a kept answer is not kept again, so every line
  is cast at least every second measurement and a door that opened between two shows at
  most one measurement (0.25 s) late [design].
- **Factions for the combat AI's frame** (`Game::begin_faction_frame` /
  `end_faction_frame`, `fnv-game`): `npc_combat::fight` asked every actor's factions
  afresh for each comrade and hostility check (`Game::comrades` → `factions_of`); they and
  the faction relations are now kept for the frame, in the same keep as the detection
  round's (`functions::actor::FactionKeep`), forgotten by the same script functions and
  crimes.
- **Sight check** (`NVF_PLAY_TEST_SIGHT_CHECK=1`): every line is also worked out the old
  way and the answers compared; the `detection summary, lines of sight:` line at the exit
  gives the counts.

At 50 a side, measured with the engine's span profile (`--features profile`,
`NVF_PLAY_TEST_PROFILE=10`, passed with the battle's new `--aid`) instead of xperf, which
needs the owner's elevation; #634's head (ce8574f4) and this branch built the same way and
run in turn, twice each, the start-up windows left out (ms of CPU a second, the two runs)
[observed: target/stress/battle/prof-*, private]:

| | #634 | this branch |
|---|---|---|
| ray casts (`cast_ray_and_get_normal`, all callers) | 621, 640 | 296, 316 |
| `detection::measure` (its span) | 400, 374 | 302, 340 |
| `npc_combat::fight` | 76, 97 | 23, 27 |
| the whole process | 4,014, 4,224 | 2,987, 3,182 |
| mean frame (profiled) | 62.5, 51.2 ms | 61.3, 47.2 ms |

Untraced, the same battle: #634 mean 39.7 and 36.6 ms, this branch 41.6 and 33.2 ms: the
machine's other load moves a run by more than the change. The sight check (one run, every
line also cast the old way): 3.8 million lines asked, 11.3 million rays where the old way
cast 20.2 million; 243 answers differ from the old way's (a line grazing an edge, cast from
a point a unit further along) and 5 of the 449,418 shared answers from the asker's own ray
[observed: target/stress/battle/check2-50]. 252,085 lines of still pairs could have been
kept, 6.6 % of those asked; 7,795 of them (3 %) would have answered otherwise than a
fresh ray. The battle's outcome is as before (Legion 48–49 dead, NCR 6–7).

The CPU is less but the frame barely shorter: the frame waits on `measure`'s serial part
(each pair's `Game::detect` and `observe_threat`, the events, the allies and alarms), not
on its rays. That is the next thing to cut.

**A bug found here, then fixed (branch claude/sight-own-capsule):** an actor's eyes (112
units up) are inside its own character hull (128.9 tall), and the collision world's ray
casts are solid, so a ray from the eyes met the actor's own hull at distance 0 every time;
the walk past actors stepped 1 unit and cast again, `PAST_ACTORS` times, and ended
"clear". Every actor's line of sight through the collision world was clear, and so was
the sun's from every actor's chest (the player's capsule is left out of the filter, so the
player's and the camera's were not) [observed: a ray from inside a character in a test
world met it at 0.0 ten times in a row, 2026-10-23]. The same walk crossed any other actor
on the line the same way, 1 unit a ray. Now the walk leaves out each actor it meets and
casts again from the start, up to `PAST_ACTORS` actors (`detection::walk_past_actors`);
`clear_sight` callers outside detection (the combat AI's cover and line-of-fire checks)
get the same fix: their cover tests from an NPC's eyes had all come out "seen", and
`combat-cover` passes again (it failed 4 of 4 runs before). A plain run of the battle after
the fix: mean 35.9 ms, 1 % low 208 ms; Legion 48 dead, NCR 8; cover taken 49 times.

The same battle with the sight check, after the fix [observed:
target/stress/battle/own-check, private]: 3.8 million lines, 5.6 million rays (11.3
million before the fix, 23.5 million the old way); 794,601 answers (21 %) differ from the
old walk's, the lines the bug had made clear; 1 of 729,805 shared answers differs from
the asker's own ray; of the still pairs that could have been kept, 164 of 169,577 (0.1 %)
would have answered otherwise. In the battle, fighters now lose each other behind cover:
Combat → Lost 38 times (5 before), Lost → Combat 36 (4), and detection events heard 8,934
times (166: an actor in Combat does not take alarms); band changes between actors fell
from 243,077 to 149,004; under the sun 433 of 26,026 measured in shade (690 of 27,870 before; why fewer is not looked at).
The outcome is close to before (Legion 46 dead, NCR 8).

### Option, not built: measure far pairs less often

Most pairs of a big battle are far from the player: at 50 a side nearly all of the
10,000 ordered pairs a measurement. Measuring the pairs where neither actor is within some
distance of the player (say beyond 2,048 units, or out of the player's detection reach)
every second or fourth measurement instead of every one, keeping their last values in
between, would cut the measuring and the rays of those pairs by half or three quarters.
It changes behaviour: far actors would notice each other, lose each other and react to
hits up to 0.25–0.75 s later than now, and a search or an alarm far away would start
later. The original's rate for actors far from the player is not documented. Not built:
the owner has not decided whether that delay is acceptable [design].

## 5. Asset caches and the machine's memory (#1170, 2026-10-31)

The release sweep (`nvf_assets::release`) keeps unused models, actor parts and textures
for five minutes, at most 2048 per cache. Those numbers were set on the owner's machine
(48 GB of RAM, a 12 GB card). They are now the `KeepLimits` resource, set at the start by
nvforever `memory_budget.rs` from the machine's RAM (`GlobalMemoryStatusEx`) and the GPU's
memory budget (`VK_EXT_memory_budget`, read through wgpu's Vulkan device: the device-local
heaps' `heapBudget`). Under 16 GB of RAM or an 8 GB budget they scale down by the smaller
share, to no less than 30 s and 128 entries. An integrated GPU has no GPU budget: its
memory is the RAM.

Every 2 s the GPU budget is read again (other programs change it) with this process's
use of the same heaps (`heapUsage`: what the driver gave the process, blocks the
allocator keeps empty included, #898), and the machine's free RAM. Over 85 % of the GPU
budget used, or under 10 % of the RAM free, presses the caches: 30 s and 128 entries
(the least the budgets scale to), until the GPU use is under 75 % and over 20 % of the RAM is free [design for every
number]. The RAM check follows what the machine has free, not the game's own commit: a
soak commits about 5 GB, which on an 8 GB machine still leaves about a quarter free. The
start logs `memory budget: RAM …, GPU budget …`; each change logs `near a budget, caches
pressed` or `back under the budgets`, with both uses.

Test aids: `NVF_PLAY_TEST_VRAM_MB` and `NVF_PLAY_TEST_RAM_MB` cap what was read (the RAM
cap takes its difference off the free RAM). Smoke: `memory-budget-capped` (512 MB: least
limits, pressed at once during the Goodsprings flight, 1,157 of 512 MB). [observed
2026-10-31, this machine: RAM 48,927 MB, GPU budget 11,242 MB, full limits.]

Tour slice 0:40 of the towns route, one lap, a 2 GB cap against none [observed
2026-10-31, the machine busy with smoke games, so both runs are noisy]: pressed at 10 s
and 32 entries (the first choice), the press came at 77 s (1,826 of 2,048 MB) and never
let go, because the vendored allocator keeps emptied blocks (#898) and the process's use
does not fall when assets go. Meshes kept fell from about 8,070 to 3,430, and
re-preparing what was dropped cost frame time: 1 % low 94.6 ms against 35.3 ms, worst
250 ms against 89, actors drawn 2.4 s against 0.8 s. Pressed now keeps 30 s and 128
entries instead. The same slice again with that (the machine was not reported busy, but a
smoke game ran beside it for part of the lap): pressed at 77 s again and held, meshes
kept about 3,990 at the end; mean 17.5 ms against 17.2, 1 % low 43.7 ms against 35.3,
one 432 ms frame, actors drawn 1.4 s against 0.8 s. Still not measured on an idle
machine, or with a real 2 GB card, which pages rather than pressing.
