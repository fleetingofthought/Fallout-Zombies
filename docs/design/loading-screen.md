# Loading screens in `--play`

Owner request 2026-10-10: when the player changes cell through a door, pause the game and
show a loading screen while the destination loads; everything is on hold until loading
is done; the picture is one of the game's loading-screen textures, as the plugins list
them. Records and what is known about them: docs/spec-notes/loading-screens.md.

Code: `engine/nvf-play/src/loading.rs` (state, screen), `crates/fnv-world/src/loadscreen.rs`
(records, choice), `engine/nvforever/src/world.rs` (`loading_gate`, faster streaming
behind the screen).

## 1. What happens

1. A door (activation) or a script moving the player sends `TravelRequest` instead of
   `Travel`.
2. The same frame the loading screen is spawned, `PlayState` goes to `Loading` and Bevy's
   virtual clock is paused.
3. Two frames later (`SHOW_FRAMES`), when the screen has been drawn, the request goes on
   as `Travel`. Physics rebuilds its cells and places the player; the renderer drops its
   cells and streams the destination's. These are the long frames, and they now pass
   behind the picture instead of in front of a frozen view of the old place.
4. Behind the screen, streaming is faster than in play: the renderer loads cells for
   200 ms each frame instead of one cell a frame, physics takes up to 8 prepared cells a
   frame instead of 1.
   Until then the renderer loads the cells around the arrival's grid square
   (`Streamer::arrival`), not around the camera: in the frame of the travel the camera
   still has its transform of the place left, and around it (an interior's coordinates,
   far out in the worldspace) empty cells loaded at once and counted as the
   destination's; the screen then came down with the arrival's own cells still to load
   one a frame, and they faded in in front of the player (playtest list 4, items 9 and
   10, 2026-10-13) [observed: `--cell GSGasStation` through its door: 26 cell loads in
   all after the fix, no fade in the frame after the screen].
5. The screen comes down when physics has every cell it asked for around the player
   (`PhysState::loading_done`), the renderer has all the cells around the camera
   (`Streamer::settled`, reported through `Loading::render_busy`) and no actor in view is
   still waiting for its parts to be read (`readahead::actors_waiting`, reported through
   `Loading::actors_busy`; owner 2026-10-24: NPCs were seen being drawn in after the
   screen), the pipelines they are drawn with are ready (`pipeline_gate.rs`, reported
   through `Loading::pipelines_busy`, #1520), and that has been so
   for 0.25 s and 3 frames (`SETTLE_SECS`, `SETTLE_FRAMES`: time for textures and meshes
   to reach the graphics card). `PlayState` returns to `Playing`, the clock runs again.
6. If loading has not finished after 120 s (`TIMEOUT_SECS`) the screen comes down with a
   warning, so an unattended run cannot hang there.
   Pipelines have a shorter limit of their own: 15 s (`PIPELINE_CAP_SECS`) once they alone
   are waited for, with a warning naming those still waiting. One that has failed for good,
   or is still being retried 15 s after it was asked for, is logged with its label and not
   waited for again.

   Without this wait the view stayed flat under the HUD for 1-2 s after a start's screen
   came down, then the world appeared in one frame, with 104-125 pipelines still waiting as
   it came down [observed: Goodsprings, main e2517687 and the same with logging, all 19 starts with shots, across
   pool sizes]. A task holds Bevy's shader cache lock while it composes a shader [design:
   bevy_render 0.20.1 `pipeline_cache.rs`], and more threads did not shorten the wait
   [observed: 1 to 16 threads, about 3.7-4.1 s to clear in all but one batch]. Objects first seen after the screen is down are not covered.

The player arrives standing: physics puts the character on the support the arrival rule
found and counts it as on the ground until its first step, so the body does not play the
jump loop behind the screen and a landing when it comes down (`PhysState::arrive_player`).

## 1a. The start (owner 2026-10-24)

A `--play` run that starts in the world (not on the main menu, `--menu`) is held behind
the same screen from its first frame (`nvf_play::loading::StartCover`, asked for by
nvforever). Before, the world was in view while the script data still loaded on its
thread (4–9 s on an unbusy machine, up to 21 s with three smoke games starting at once),
and NPCs were drawn in as their parts were read. The screen comes down when what step 5
waits for is in, and the scripts are made (and a `--load`ed save put back into them).
A travel asked for under it goes out behind it and is waited for too.

- Its time counts on no clock: the test aids' clock (`pause::PlayClock`) and
  `--run-seconds` count from when it comes down, as if the run began then. A step at
  14 s comes 14 s after the world can be played, with the world and the scripts run for
  14 s; the world aids' late-start rule (`pause::WorldAidClock`, `SCRIPTS_START_SECONDS`)
  then never holds them back. A run lasts its seconds plus its start.
- **The picture first.** The first render frame blocks on a blit pipeline [design:
  bevy_core_pipeline 0.20.1 `prepare_view_upscaling_pipelines`], compiled on the pool that
  also reads models and cells, so the world's reads delayed the screen: its picture showed
  in at most one capture of three starts [observed]. So under the start's screen cells and
  actors start loading only once the screen has had `PICTURE_FRAMES` render frames with the
  pipelines it is drawn with ready, or after `PICTURE_CAP_SECS` (`Loading::world_held`). Log: `loading
  screen: the world starts loading <s> s after the screen (drawn)`.
- Log: `loading screen: the start is held until the world can be played`, `loading
  screen down after <s> s (the start: cells, actors and scripts ready); …`, or after
  120 s `the start still waits for <what> after <s> s; going on without it`.
- A run taking over from another (`--handoff`, loading a save while playing) shows its
  window once this screen is down.
- [hypothesis] The original never shows the world before its scripts run: it reads the
  plugins, scripts and all, before the main menu.

[design: `engine/nvforever/src/pipboy_arm.rs`, `player_body.rs`,
`engine/nvf-play/src/pipboy.rs`, `loading.rs`; #1803] Once the start's picture hold is
released and the real first-person rig exists, one hidden Pip-Boy warm-up borrows that
rig at the raise clip's hold pose, swaps the screen/glare materials and activates the
existing image camera. Without a worn Pip-Boy it warms only that camera and page. It
draws STATS Status under the cover, since ITEMS needs an open inventory, then restores
the player's page and STATS tab (`pipboy/items.rs::draw`, `pipboy/stats/mod.rs`). It
opens no inventory or script menu and borrows no player POV, weapon or input state.
The temporary animator pose is applied only after the normal body has supplied those
states. Transforms, root visibility and only the warm-up's culling overrides are restored
before the next body tick; materials and the inactive image camera follow in that tick's
UI pass. Loading waits explicitly for this teardown as well as its existing pipeline gate.

[design: `engine/nvforever/src/pipboy_arm.rs`, `pipboy_ready.rs`; #1803]
The start's hidden interval has a four-render-report floor and a 250 ms wall-clock
budget, followed by teardown. It waits for submission, not the global busy pipeline
count: this screen already owns the compilation wait. If submission is incomplete at
the budget, the rig stays cold. A first visible device opening verifies its own main
queue, prepared materials, bind groups and main/prepass pipelines behind the camera;
Tab/Escape can cancel it through the normal key handler. At three seconds it raises
anyway and logs incomplete readiness, bounding failed shaders/uploads or a lost rig.
The first visible report and device access sound wait for hidden-pose restoration.
Real held openings report their remembered script page immediately; startup stays silent.

[observed 2026-11-07, commit `4fbe5750d`, 8 alternating fresh launches, one machine] 4/4 clean first openings enabled and 4/4 blue or partial with `NVF_PLAY_TEST_NO_PIPBOY_WARM=1`. Screen-down medians 7.68 s enabled (7.38-7.98) and 6.81 s disabled (6.41-6.96): about +0.9 s in these launches.

[observed: 2026-11-07, commit `8fa3f8a04`, superseded by the set above] An earlier set of
eight alternating launches gave the same 4/4 and 4/4, with noisier screen-down medians
(8.84 s enabled, 8.27 s disabled). A later run without the vendored main-pass retries passed the same scenarios with a
clean first opening, so those retries were dropped (docs/spec-notes/pipboy.md §8).

[observed: earlier supplied first-open log and disabled twin, 2026-11-07]
The failed first attempt capped its startup warm-up at 3011 ms and took 12.98/10.79
seconds in one enabled/disabled pair. Zero new pipelines during the raise still left
blue silhouettes in the first three shots. See `docs/spec-notes/pipboy.md` section 8.

[design: `engine/nvforever/src/pipboy_arm.rs`, `pipboy_ready.rs`, `pipeline_gate.rs`]
The warm-up log's total requests still include concurrent world work. `own rig pipelines`
now names the actual main/prepass IDs for this rig. The existing image camera requests
four compatible UI/sprite/postprocessing/tonemapping keys; shared IDs are not extra
compilations. `NVF_PLAY_TEST_PIPBOY_DRAW=1` logs the first visible submission and changes
thereafter, including opaque entries and material/binding readiness; no logging system
is registered by default. `NVF_PLAY_TEST_NO_PIPBOY_WARM=1` remains the disabled twin.

[design: `tools/smoke/scenarios.toml`, `engine/nvforever/src/pipboy_arm.rs`]
Compare the four first-open shots and the restored-pose readiness report. The startup
hold is bounded by 250 ms plus teardown and scheduling; a cold first device opening
has a separate three-second cap. Check save load, no device, screen-menu frame, key
cancellation and a cold New Game gift at 16:9 and 4:3 after this review revision.

## 2. What is on hold

- Everything that runs only in `PlayState::Playing`: the player's input, movement and
  physics steps, activation, combat, the game clock, opening menus and the pause menu.
- Scripts: `run_scripts` returns while a loading screen is up, as it does under the
  pause menu. Loading-screen time does not count for scripts in the original either
  [observed: layer5-captures.md §8.3 e]. The two frozen frames after a change of place
  that stood in for a loading screen remain (they now follow the real one).
- Everything driven by `Time`: animations, doors, travelling NPCs, the sky's clock,
  on-screen messages, the `NVF_PLAY_TEST_*` aids and `--run-seconds` (a run lasts its
  seconds of game, plus the loading).
- All input (owner 2026-10-27: New Game could be clicked and the arrow keys moved the
  main menu while a save loaded): `loading::hold_input` lets go of every key and mouse
  button in `PreUpdate`, after the controller's menu keys and the test aids' presses and
  before the interface reads the pointer, and the screen blocks the pointer from what
  is under it. A key held through the screen counts only once pressed again.
- Not on hold: streaming (render and physics), distant LOD generation, and a voice line
  already playing (as under the pause menu; voice.rs).

## 3. The screen

- The record: `fnv_world::loadscreen::choose` over every LSCR of the load order. Screens
  that belong to the destination (its interior cell, or a grid square within 2 of the
  arrival square in its worldspace) and screens for anywhere are both allowed; when the
  destination has its own, one of them is taken every other time. Both rules are
  hypotheses (spec note §3).
- The picture: the record's ICON, read through the texture cache like any other
  texture. Stretched to the window, as the original's (spec note §4). A record without a
  picture, or one whose texture is missing, gives a black screen with the rest. The whole
  screen is over black from its first frame.
- The tip (`loading_look::fit_tip`): the record's DESC at the top, set by the LSCT its WMI1
  names: box, font, colour and alignment (`fnv_world::loadscreen::TipStyle`; without an LSCT
  of the tip type, the values of the base game's one). x and width follow the window's width
  as pixels of a window 1280 wide, y and the text's size its height as parts of 960. It lies
  on `loading_text_bar.dds` across the window, as high as the text. No text, no band. Drawn
  again when the window's size changes, since the wrapping follows its shape.
- The emblem (`loading_look::Emblem`): `meshes\interface\circular loading\loading01.nif`,
  the wheel's centre at (0.5 W, 0.794 H) and the wheel 0.172 H across, its nodes turned by
  the sequence `Idle` on real time, since the virtual clock is paused behind the screen. By
  elapsed time, so a long frame makes it jump to where it should be. Without the model (or
  with one that does not read) no emblem and one log line; nothing waits for it.
- Read once and kept (`loading_look::Look`): the emblem, its textures, the band and the font.
- The original's picture stage, objective, stats and level bar are not built (spec note §4).

## 4. Scope and open points

- Travel and a world start (§1a) show a loading screen. A New Game chosen on the main
  menu still loads behind the menu and the intro film.
- Travel without `--play` (the free camera following `--npc-travel` NPCs) is unchanged:
  no loading screen.
- The game's rules for choosing a screen are open (spec note §3).
- Whether a travel's screen should wait for the sun's shadow pipelines is open (#1732): test
  aid `NVF_PLAY_TEST_TRAVEL_SKIP_SUN_SHADOWS=1` has it not wait, so that both can be measured
  (`engine/nvforever/src/pipeline_gate.rs`, `sun_shadow`). The start's screen always waits.
- Test aid: `NVF_PLAY_TEST_LOADING_SHOT=<png>` saves the window when the travel goes out
  (the screen is up); later screens of the run go to `<name>-2.png`, and so on.

## 5. Checks

- `fnv-world`: `loadscreen::tests` (locations, choice), `tests/load_screen_type.rs` (LSCT
  on a plugin written for the test).
- `nvf-play`: `loading::tests` (cover, destination, a tip only with text, and the sequence:
  screen up and clock paused before the travel goes out, held while the renderer is busy,
  down and playing again after; held while actors are read; the start's screen and its
  clock; a travel under the start's screen), `loading_look::tests` (the tip's box and band
  and the emblem's place at 1280x720 and 1280x1024, its turning).
- Smoke: `loading-screen-look`.
- In the game (owner's data): Doc Mitchell's front door, both ways; see the pull request.
- Where a start's first seconds go (`engine/nvforever/src/start_times.rs`, every run): `start:
  Startup <s> s; frame 1 <s> s: <s> s before Update, <s> s in Update, <s> s after it` and
  `start: first render frame finished <s> s after frame 1 ended`. For the systems inside
  them: a `--features profile` build with `NVF_PLAY_TEST_PROFILE=1`, whose first `profile:`
  line covers the first frames.
