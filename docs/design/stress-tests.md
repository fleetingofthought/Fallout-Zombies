# Stress tests

The smoke tests (`tools/smoke`) check one behaviour each in a short run. The stress tests
(`tools/stress`) push the engine the way long play does: every cell, long trips, big
fights. They look for crashes, hangs, memory that is never given back, and slow frames.
They are not parity tests. Nothing in them is compared with the original game, except
the combat measures that docs/spec-notes/npc-behaviour-compare.md compares.

Run them on a machine with the game installed (read-only, as every run). They take the
game locks as smoke runs do, so a stress run and a smoke run wait for each other.

```
python tools/stress list
python tools/stress run cells  --exe <engine target>/release/nvforever.exe --data <Data> --fnvtool <root target>/release/fnvtool.exe [options]
python tools/stress run tour   ... [--laps N]
python tools/stress run battle ... [--per-side 10|25|50] [--third N] [--seconds S]
python tools/stress run soak   ... [--towns ...] [--event-every S] [--aid NVF_PLAY_TEST_PROFILE=5]
```

`NVF_EXE`, `NVF_DATA` and `NVF_FNVTOOL` stand in for the three paths, as for smoke.
`--no-locks` skips the locks on a machine of your own. `--name` names the run's folder.
`tour`, `battle` and `soak` also take `--repetitions N` (a positive integer) and an
optional caller-supplied `--build-label TEXT`. Repetitions run serially in `run-001`,
`run-002`, … below the named run folder; existing names are never reused. Do not use
`--no-locks` for a shared machine.

## 1. How a run is made (tools/stress/stress/lib.py)

- **One launch.** `lib.run` starts nvforever with `--play --no-messages` and these test
  aids: `NVF_PLAY_TEST_GOD`, `NVF_PLAY_TEST_UNSEEN`, `NVF_PLAY_TEST_SCRIPT_MENUS=0.05`
  and `NVF_PLAY_TEST_STRESS=1` (§2). The test adds its own aids on top.
- **Its own home.** Each launch gets its own NVForever home, `<run>/<launch>.home`
  (`NVFOREVER_HOME`). Its run log and any crash reports
  (`logs/crash-*.txt`, engine/nvforever/src/crash.rs) stay with it.
- **Watched for progress.** Each test says which log lines count as progress. When no
  such line comes for `--hang` seconds, the run counts as hung: every thread's stack goes
  to `<launch>.stacks.txt` (tools/smoke `stacks.py`), and the process tree is killed.
- **Memory.** On Windows, the runner reads the process's working set and private bytes
  every few seconds (`<launch>.memory.csv`).
- **Status.** A launch ends `ok`, `crashed` (a crash report was written), `hung`, or
  `failed` (a non-zero exit with no report: a native crash, which no Rust hook sees).
- **Shared code.** The locks (`Locks`, `default_lock_paths`), the aid-free environment
  and the stack dump are imported from tools/smoke, not copied.
- **Crash grouping.** Crashes are grouped by cause (`lib.group_crashes`). The key is the
  panic message with its numbers blanked, the source file it was raised in, and the top
  five backtrace frames that are the engine's own (std, core and the crash hook left
  out). Each group lists the cells or runs that hit it.

Every run writes `target/stress/<test>/<run>/results.json` (everything) and `report.md`
(the summary to read first). The launch logs and command lines sit beside them. Tour,
battle and soak results have a provenance manifest: the runner checkout identity,
executable and fnvtool file hashes, requested INI hash, requested arguments/aids, masters,
and platform facts. It records unavailable information explicitly; an executable's hash is
not treated as its source revision and its build features are unknown unless the caller
labels the build. Effective saved settings, adapter/backend/driver, and cache state are
likewise unknown until the engine reports them.

With `--repetitions`, the top-level folder has `comparison.json` and `comparison.md`; each
child keeps the ordinary `results.json` and `report.md`. The comparison retains every row,
but aggregates only successful rows with background-CPU samples below the runner's busy
threshold. Rows are `busy`, `unknown`, or `below_busy_threshold`; the latter means only
that the runner's warning did not trigger, not that the machine was quiet. An aggregate
with no eligible rows is unavailable, never zero. It summarizes only the existing mean,
slowest-1%-mean, and worst frame metrics; the slowest-1% mean is not p99.
After each completed child the top-level comparison index is written as `incomplete`, so a
stopped sequence still names its completed repetitions; it becomes `complete` only after
the requested sequence finishes.

## 2. The engine's aids (engine/nvf-play/src/stress.rs, engine/nvforever/src/stress_sample.rs)

- **`NVF_PLAY_TEST_STRESS=1`.** Nothing holds an unattended run up, and scripts still run.
  - A conversation that starts (a force-greet, StartConversation, a package's dialogue)
    is ended at once.
  - A script's move of the player (MoveTo player, a trigger's travel) is left out.
  - Both are logged; the conversations ended are counted in `stress summary:`.
- **`NVF_PLAY_TEST_CELLS=<list>[;<dwell>]`.** Visits the cells of a list file in turn:
  `interior <cell>` and `exterior <world> <x> <y> <cell>`, FormIDs in hex.
  - Each visit is a travel behind the loading screen, as a door or `coc` makes.
  - An exterior's ground is found once the cell is in.
  - Each visit logs `stress cell: i/n start …` before it goes, and `stress cell: i/n
    loaded in <s> s …` when the loading screen comes down.
  - An interior with no door or marker is entered by the placed reference nearest the
    middle of its references (its origin when it has none), so it is load-tested too.
  - A grid cell that is not the listed one (a list made with another load order) logs
    `skipped:` with the reason.
- **`NVF_PLAY_TEST_TOUR=<route>`.** Steps the player along a closed route. The route file
  has `world`, `step`, `dwell` and `laps` lines, and `<x> <y>` waypoints.
  - A step within the loaded ground puts the player on it at once; a step beyond it goes
    by a travel.
  - Logs each lap's start and end, and each waypoint reached.
- **`NVF_PLAY_TEST_BATTLE=<at>;<base>:<count>:<x>:<y>;…`.** At `<at>` seconds, puts each
  group's fighters in a block on the ground round (x, y), facing the other groups
  (`Game::create_at`, made as PlaceAtMe makes references).
  - Each fighter is set on the nearest fighter of another group (StartCombat).
  - The placing line ends with each group's FormIDs (`0xFF000000..0xFF000009`): created
    actors have no Editor ID, and the runner tells the sides' deaths apart by them.
  - Created actors become residents and fight as placed ones do.
- **`NVF_PLAY_TEST_SAMPLE=<seconds>`.** Every that many seconds, logs one `stress sample:`
  line with what the engine holds and where the camera is:
  - frame time over the window: mean and worst;
  - entities, drawn references, meshes, images and materials (NifMaterials);
  - audio sinks, physics cells, physics bodies (fixed/kinematic/dynamic);
  - actors, actors with a non-finite position, ragdolls, projectiles in flight;
  - where the backend reports it (Vulkan, DX12), the GPU allocator's allocated and
    reserved megabytes and its memory blocks. The tour watches allocated and reserved
    for leaks;
  - the window's own counts, before the place: `startup frames`, `hidden frames`
    (behind a loading screen), `phase changes` (frames a loading screen went up or came
    down in) and `window <ms> ms`, its real length. The frame mean and worst cover only
    the seen frames; a window with `phase changes` over 0 held a transition, and its
    place is where it ended (the tour report counts such windows). The window starts
    again when start-up ends.

  Each frame is judged at one pair of boundaries (issue #1415). The sampler runs once a
  frame in `First` and takes its own instant there; a frame is the time from one run to
  the next, and its phase (start-up, behind a loading screen, seen), its cell and actor
  counts and the place come from the same two runs [design:
  `engine/nvforever/src/stress_sample.rs`, `Samples::step`, `frame`]. A frame whose two
  boundaries saw different phases is in no frame time; when slow it is logged as
  `stress hitch (across a phase change, hidden to seen): …, from <place> to <place>`,
  its raw time without a claim on whether the loading or the game took it (the tour
  report lists those apart). The first run only sets the baseline: no frame is counted
  for it [observed #1415: a 674 ms seen hitch logged just after the start's screen came
  down, and the first window carried the travel destination's place].

  **What the frame time measures.** The `stress sample:` frame mean and worst, the
  `stress frames:` figures, the `stress actors:` frame figures, `stress start-up:` and
  every `stress hitch` line measure the main thread's loop from one `First` to the next:
  the frame's own schedules, the wait at extraction for the render thread to finish the
  frame before, extraction and the window events [design: bevy_render 0.20.1
  `pipelined_rendering.rs:115-136`]. Before issue #1415 they used `Time<Real>`'s delta,
  the time between the instants the render world sends after each render schedule
  [design: bevy_render 0.20.1 `lib.rs:464,522-524`], which `First` receives, or
  `Instant::now()` when none is queued [design: bevy_time 0.20.1 `lib.rs:163,180`]. Under
  pipelined rendering (bevy's default plugins keep it, and so does nvforever [design:
  bevy_internal 0.20.1 `default_plugins.rs:56-57`]) that delta can be the frame before's,
  so it cannot be paired with what one boundary saw: a long frame a loading screen came
  down in could arrive one boundary late and count as a seen hitch [design: code reading,
  not reproduced live]. Both clocks tick once a frame and a run of frames' total telescopes
  to the time between its end instants, so means over a run of seen frames should agree up
  to the instants at its ends [hypothesis]; single frames, the worst and the 1 % low can differ by
  which frame a stall lands in and by the two clocks' jitter, and the first frame (zero
  under `Time<Real>`) is no longer counted. Figures from before and after the change are
  not strictly comparable. Frame telemetry's `duration_ns` still uses `Time<Real>`'s
  delta (docs/design/frame-telemetry.md §2).

  Beside each sample, `stress gpu labels:` lists the allocator's memory by label, largest
  first. Game textures are labelled by their folder (`textures\clutter`, `generated lod`).
  Every 30 s, `stress render:` gives the render world's GPU images by the same labels,
  and the resources held in the NifMaterial bindless slabs. When a label has more
  allocations than live images, something still holds textures the game let go of (#903).

  Every 30 s, `stress caches:` lists the caches the release sweep (`release.rs`) does not
  own, and the texture cache: per cache its entries, those nothing outside it holds
  (idle), and the images its materials sample with their GPU bytes (from the images'
  descriptors; an image two caches share counts in both). Grass models, trees and water
  materials are counted only (one per GRAS, TREE, WATR form). Interface pictures are
  read from `stress gpu labels` (`textures\interface`) (#904).

  At the exit, `stress frames:` gives the whole run's frame-time mean, 1 % low and worst,
  and the frames left out: behind loading screens, and across a phase change.

## 3. cells

Every interior cell of the ten masters (tools/smoke's `masters` list, the engine's load
order) is visited, so a crash anywhere in loading shows up. Exteriors are left to the tour,
which streams them on its way (owner, 2026-10-21); `--kinds all` or `--kinds exterior` takes
them in again.

- **Where the cells come from.** fnvtool's `world list`, then `world cells` for each
  worldspace and for the interiors. Persistent cells (no grid) are left out.
- **Batches.** `--batch` cells per launch, 100 by default. A crash or a hang ends its
  batch. The next batch starts after the cell it was on, so one bad cell costs one
  launch, not the sweep.
- **Choosing cells.** `--kinds interior|exterior|all` (interior by default), `--worlds`, `--stride N` (exteriors
  on every N-th grid line both ways) and `--limit` pick a sample.
- **Long sweeps.** All 44,517 cells (`--kinds all`) take about 30 hours at 50 a launch; batches of 200
  are fine. After every batch the runner saves `state.json` and rewrites
  `results.json` and `report.md`, so a sweep can be read while it runs.
  `--resume <run>` goes on with a stopped sweep in the same folder: cells it already has
  a row for are skipped, and its rows and crash groups stay in the results.
- **Whose crash.** A crash or hang counts against the cell the launch last went to,
  whether it came while that cell loaded or after.
- **A game that does not start.** A launch that ends before its first cell (an exe
  being rebuilt: 0xC0000142, DLL init failed) blames no cell: its cells are tried again.
  After `--abort-after` (3) such launches in a row the sweep stops with status
  "aborted", its state saved and the cells left not reached; `--resume` goes on.
- **Results.** Totals (OK, crashed, hung, skipped, not reached), the mean and slowest
  loads, the crash groups with their cells, and each bad cell's last log lines.
- **Missing files and script errors.** The engine's own warnings while a cell is the one
  visited (from its start line to the next cell's) are that cell's: `model not loaded:` and
  `texture not loaded:` (nvf-assets `guard::failed`), `script error in` (nvf-play
  script.rs). Each OK row keeps them; the results group them by file (a model's or
  texture's path) or by script and error (numbers taken out), with the cells that met each,
  and the report lists the commonest.
- **Stray entity commands.** A command the engine sends to an entity that is gone is
  logged, not fatal (nvforever `stray_commands.rs`; Bevy's `debug` feature names the
  command). Tour and battle reports list them, with the first place each was seen.

## 3b. markers

Fast travel to every map marker of the ten masters (`python tools/stress run markers`),
through the same path a click on the World Map takes.

- **Where the markers come from.** fnvtool's `world list`, then `world refs` for each
  worldspace: the references of the MapMarker static (0x00000010). `--worlds` and
  `--limit` pick a sample.
- **A launch.** `--batch` markers (25) from `--start` (GSGeneralStore). The script aid
  `call:-:ShowAllMapMarkers:1:1:0` makes every marker a fast-travel target; then per
  marker, `--every` seconds (30) apart, the Pip-Boy aid opens on DATA, asks to travel
  there (`travel=`) and answers Yes (`key=use`).
- **What each marker gets.** From its ask to the next: refused (the refusal, e.g.
  HostilesNear, or not a named marker), no trip (no departure logged), no arrival (the
  trip went out but its loading screen did not come down), or OK with the seconds the
  screen was up; missing files and script errors as in cells. A crash or hang counts
  against the marker last asked and ends the batch; the next starts after it.
- **Results.** Totals, the refusals by reason, the crash groups, the problems grouped as
  in cells, the slowest arrivals.

## 4. tour

Long play's memory: what the engine holds, sampled along laps of the Mojave.

- **The route.** By default (`--path towns`) a lap through Goodsprings, Primm, Mojave
  Outpost, Nipton, Novac, Camp McCarran, Nellis and Jacobstown and back to Goodsprings
  (owner, 2026-10-18), each town found by its exterior cell's Editor ID (`--towns` to
  change them).
  - The legs follow the walkable cells: the cells of the worldspace's largest connected
    navmesh area (`fnvtool nav cells --space WastelandNV`, fnv.nav.cells/1), with a
    point every two cells.
  - `--path ring` is a ring on those cells instead (`--waypoints`, `--inset`).
  - `--slice START:END` takes only those points of the route, to go over one stretch
    again; the numbers are the log's `at waypoint i/n`.
  - `--route` takes a file of your own.
  - Steps put the player down rather than walk, so the game's invisible walls
    (CollisionMarker boxes) do not stop the tour. The route has to stay inside on its
    own, which the towns route does by going from town to town over walkable cells. A
    ring on the grid's bounds (then the ring on the whole navmesh area) took the player
    past the playable area (owner, 2026-10-18).
- **The pace.** The player moves `--step` units every `--dwell` seconds. The defaults
  (1024 units a second) stream cells about as fast as running would load them.
- **The samples.** Each sample is tagged with its lap and the waypoint it is on. The
  process's memory is added from the runner's readings. All of them go to `samples.csv`.
- **Possible leaks.** A count that ends each lap higher than the last ("rising every
  lap"), or that is higher at the same waypoint on a later lap than on the first ("did
  not fall back on coming back"). Each is listed with where it grew (the sample's
  place). The tolerance is 5 %.

  A count that settles after the first lap is caching. One that keeps rising is the one
  to look into.
- **Assets not released.** Within one lap: a lap ends where it began, so meshes, images
  and the asset caches (`models cached`, `textures cached`) should end near their
  lap-start counts. One whose mean over the lap's last tenth of samples is above its first
  tenth's by more than 25 % plus 100 is listed. The caches drop what no live entity uses
  after five minutes, keeping at most 2,048 unused entries each; a texture's five minutes
  count from when the model or actor part that held it went unused (#904; before, it
  waited the model's five minutes and then its own)
  (`engine/nvf-assets/src/release.rs`; one minute and 256 before 2026-10-22, when this
  check came back clean). Since then the caches, and the meshes and images they hold,
  end a lap up by the last five minutes' places and are listed here by design [observed:
  towns lap 2026-10-22, textures cached 945 at the start, 3,443 at the end; peak working
  set 5,151 MB against 5,107 MB on origin/main]. A count listed in "Possible leaks" is
  still the one to look into. Before that sweep existed, the
  2026-10-18 lap went from 816 meshes and 618 images to 4,726 and 2,962 at the start's
  place [observed].
- **Start-up and hitches.** The frames until the start's cells are in are start-up. They
  are left out of the frame statistics and reported on their own line, because the first
  frame carries the app's build and the Startup systems (about 2.5 s in the 2026-10-18 tour
  and battle runs) [observed]. After start-up, every frame over 250 ms is logged with the
  cells loaded in it and the place. The report lists the slowest ten, so a cell load can be
  told from a stall.

This does not walk on the navmesh as an actor would: it steps over the ground. That
streams the same cells, but finds no path or collision trouble. A walking tour, with the
residents' walker driving the player, is left for later.

### 4.1 soak

The tour's lap with play mixed in (`tools/stress/stress/soak.py`), to find where hitches
and growth come from. Every `--event-every` seconds (default 20) one event in turn, on
references the game data places near the route (`fnvtool world refs`, `sim door-models`):

- **container**: the player is moved to a placed container, opens it (the script aid's
  `activate`, as E opens it) and closes it with Tab 3 s on;
- **door**: the player is moved to a teleport door and goes through it; the tour's next
  step brings them back outdoors;
- **fight**: a persistent hostile NPC (Powder Ganger, Fiend, Viper) is moved beside the
  player and set on them (StartCombat), and killed 15 s on.

The events are `NVF_PLAY_TEST_SCRIPT` steps and `NVF_PLAY_TEST_KEYS` presses on the clock
that starts with the scripts (`NVF_PLAY_AIDS_FROM_SCRIPTS=1`), so the report puts each
hitch down to the event before it (within 8 s) or to walking ("Hitches by cause"), next
to what happened ("containers opened", loads, kills, aid steps that failed) and the
tour's samples, leak checks and slowest frames. `NVF_PLAY_TEST_STRESS=0`: scripts may move
the player, which the events need. `events.json` lists the schedule.

Frames behind a loading screen are left out of the frame times and their stalls logged
apart (`stress hitch (behind a loading screen):`, `profile hitch (behind a loading
screen):`): every `world::stream` stall after a door or teleport on the first soak was
behind the screen [observed 2026-10-27].

**The machine's load.** Every tour, battle and soak report says how much of the machine's
cores the game and other programs used, sampled with the memory; when others held over a
quarter for a tenth of the run it says the hitches may be theirs. The first counter read is
a baseline only, each later read ends an interval, and the means weight each interval by
its elapsed time (`lib.CpuLoad`, `<name>.cpu.csv`); a read with no elapsed time keeps the
baseline (its time is not lost), and an interval with a share over 105 % is left out and
counted. The figures cover the launch's sampled intervals, not a steady-state window a
trace was cut from [observed, issue #1412: the old first sample covered the moment after
the counters were made and came out at 195 % and 390 % of the machine]. The first full soak
(2026-10-27) ran beside about thirty compilers of other sessions: systems that take well
under a millisecond (`fog::prepare_fog`, `view::prepare_view_uniforms`) took 75–150 ms in
its slow frames [observed]. Run the soak, and every budget check, on an idle machine.

## 5. battle

NCR troopers against Legion soldiers in Goodsprings, with an unseen, god-mode player
watching from the south.

- **The fighters.** The leveled lists the AI observer's npc_battles campaign spawned in
  the original game: `LvlNCRTrooper`, `LvlLegionRaiderMelee`, and `VCrTier1CoyoteMed`
  for `--third` [observed: tools/capture/observe/npc_battles.campaign.json].
- **The layout.** The sides stand `--gap` units apart, west and east of `--centre`;
  `--per-side` sets their size (10, 25, 50).
- **Frames and memory.** Frame-time mean, 1 % low and worst; the peaks of memory,
  projectiles, ragdolls and actors; actors with a non-finite position.
- **Per system.** The engine's own per-frame timings, from the summaries that report one
  (actor animation, scripts, foot IK, head tracking …).
- **Combat**, for AI tuning:
  - the sides' standing, dead, hits, kills and flights (`battle summary:`);
  - time to kill, counted from the placing;
  - guns: shots against projectile hits on actors, and reloads; melee: blows and impacts
    (the sides' hits in `battle summary:` count both);
  - deaths by group, with the median and last time to kill;
  - flee checks and flights, cover taken, target switches;
  - combat lines a minute by kind;
  - melee spacing (swing distances), and stuck actors.

## 6. Reading results

Start with `report.md`.

- **cells.** Each crash group is one bug: its message, where it was raised, and the cells
  that hit it. The launch logs (`batch-NNN.log`) and homes (`batch-NNN.home/logs`) hold
  the full reports.
- **tour.** "Possible leaks" names the counts to look at; `samples.csv` plots them over
  time.
- **battle.** Numbers to compare between builds and with npc-behaviour-compare.md's rows.
- **Slowest frames by span.** With a build that has the spans (`cargo build --release -p
  nvforever --features nvforever/profile`, in a target folder of its own: it rebuilds
  Bevy) and `--aid NVF_PLAY_TEST_PROFILE=5`, every frame of 100 ms or more logs its own
  spans (`profile hitch:`, nvforever profile.rs). Tour and battle reports list the ten
  slowest with their costliest spans: read each row down to the first span that explains
  the time (a schedule holds its systems).

A hung launch's `.stacks.txt` shows where every thread stood.

### 6.1 Frame-time budget (the release check)

`tour` and `battle` take `--budget-mean-ms`, `--budget-low1-ms` and `--budget-worst-ms`
(after start-up). A run over any of them fails (exit 1) and its report says
`Budget: FAIL` with what broke it, right under the status line. Before a release, on an
idle machine (no other games, smoke runs or builds), with a normal release build:

```
python tools/stress run tour   --towns Goodsprings Primm --budget-low1-ms 50 --budget-worst-ms 250 [--exe ... --data ... --fnvtool ...]
python tools/stress run battle --per-side 10 --seconds 90 --budget-low1-ms 50 --budget-worst-ms 500 [--exe ... --data ...]
```

The limits are what the build of 2026-10-27 kept with room to spare [observed: Goodsprings–
Primm tour 1 % low 31 ms, worst 173 ms; 10-a-side battle 1 % low 33–44 ms, worst
262–348 ms]; before that build's fixes (streaming-hitches.md §3) the tour's worst frame
was 1,344 ms. When one fails, run it again with a profile build and
`--aid NVF_PLAY_TEST_PROFILE=5` for the spans of each slow frame. The battle places all its
fighters at once (`ai::draw_and_place`, about 130 ms), which play does not.

### 6.2 Comparing two builds

[design] `compare tour`, `compare battle` and `compare soak` use four measured launches per side in ABBA order after one discarded warm-up each, with separate homes, metadata, early frame captures and retained coarse metrics (`tools/stress/stress/comparison.py:1`). The versioned `comparison.json` and `comparison.md` retain every launch and explain exclusions, spread and verdicts. See [reproducible stress comparisons](stress-comparisons.md) for the ten-second settle, thirty-second window, validation rules, accepted thresholds and the pending scene and continuous-movement coverage; captures cover only the first sixty seconds, so later laps retain coarse results.

```powershell
python tools/stress compare battle --exe-base '<base.exe>' --exe-candidate '<candidate.exe>' --base-label base --candidate-label candidate --data '<Data>' --ini '<NVForever.ini>' --name battle-abba --per-side 10 --seconds 90 --repetitions 4 --order ABBA --warmup 1 --settle-seconds 10 --measure-seconds 30 --telemetry-seconds 60
```

## 7. Adding a test

1. Write a module in `tools/stress/stress/` with `run(ctx, …) -> dict` and
   `report(results) -> list[str]`.
   - Launch with `lib.run(ctx, lib.Launch(...))`, giving the aids, the run's length and
     the progress pattern.
   - Read what you need from `Result.lines`. Give `lib.group_crashes` the crashes.
2. Add it to `TESTS` and to the parser and dispatch in `cli.py`.
3. When the engine has to do something new, add a test aid in nvf-play `stress.rs` (or a
   counter in nvforever `stress_sample.rs`). Keep its parsing a pure function with a unit
   test, and log one line per step for the runner to read.
4. Add unit tests in `tools/stress/tests/` that run without the game: feed the module
   made-up log lines, and use a fake game (a script behind a `.cmd`) for the launcher.

The runner's own tests are a push check (`stress runner tests` in
tools/smoke/scenarios.toml): `python -m unittest discover -s tools/stress/tests`.
