# Reproducible stress comparisons

[design] The Python harness implements repeated baseline/candidate comparisons for `tour`, `battle`
and `soak` (`tools/stress/stress/cli.py`, `comparison.py`). Every launch retains its ordinary results,
provenance, raw log and early schema-1 capture. The parent report lists per-run measurements,
exclusions, medians, ranges and variation. Issue #1277 remains partly complete: a stationary scene
interface, continuous player movement and a fixed-duration long-session memory case remain pending.
No `scene` subcommand or `tools/stress/scenes.toml` exists.

## Runnable sequence

[design] Run from the repository root with Python 3.11 or later and two already built executables.
Use absolute paths supplied by the main session. Both builds need the frame telemetry exporter
(`engine/nvforever/src/frame_telemetry/`); older builds remain readable but cannot supply percentiles.
Use the same supplied INI for both sides, and choose a fresh output name for each attempt
(`tools/stress/stress/lib.py:new_folder`, `cli.py:main`).

```powershell
$baseExe = '<absolute baseline nvforever.exe path>'
$candidateExe = '<absolute candidate nvforever.exe path>'
$dataPath = '<absolute read-only data path>'
$settingsPath = '<absolute comparison NVForever.ini path>'
$runName = 'battle-abba-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
python tools/stress compare battle --exe-base $baseExe --exe-candidate $candidateExe --base-label baseline --candidate-label candidate --data $dataPath --ini $settingsPath --name $runName --per-side 10 --seconds 60 --every 2 --repetitions 3 --order ABBA --warmup 1 --settle-seconds 10 --measure-seconds 30 --telemetry-seconds 60
Get-Content "target/stress/battle/$runName/comparison.md"
```

[design] This small sequence requests eight launches: warm-up A, warm-up B, then `A B B A A B`.
There are three measured runs per side; the last ABBA block is partial. Runtime requests total eight
minutes plus loading, shutdown and hashing, so allow about ten minutes. Prefer four or more measured
runs per side for a full ABBA block (`comparison.py:launch_plan`). Elapsed wall time depends on the
machine; this sequence has not been run as part of this merge.

[design] Default comparisons request four measured runs per side and one warm-up per side. Warm-ups
are retained and excluded from statistics. A single sequence holds both existing game locks, uses
one process at a time and gives every launch a fresh home. Do not pass `--no-locks` on a shared
machine. Each child gets unique telemetry run/scenario tokens (`comparison.py:run`). Keep builds
and other games off the machine while measuring.

[design] `run tour`, `run battle` and `run soak` still accept one `--exe`, an optional caller
`--build-label`, and `--repetitions N`; their original coarse aggregate uses mean/min/max of eligible
per-run whole-run metrics (`cli.py:comparison_index`). This does not request frame telemetry.

## Coverage and measurement windows

[design] The following are supported scenario configurations, not claims of runtime validation
(`cli.py:configure_test`, `tour.py:run`, `battle.py:run`, `soak.py:run`).

| Case | Current coverage and limits |
|---|---|
| Small combat comparison | The sequence above places ten fighters per side. Comparisons require the start and placement logs to confirm every requested fighter. Combat is not seeded; equal placement does not establish equal survivor counts. |
| Larger combat | Use the same sequence with `--per-side 25` and a fresh name. Creation costs remain visible in the complete gameplay capture; the settled window is not explicitly anchored to placement. |
| Town traversal | `compare tour` accepts `--towns Goodsprings Primm --laps 2 --step 1024 --dwell 1 --every 5` and the same comparison/common options. Supply `--fnvtool` with an absolute fnvtool path. The materialized route is hashed per launch and compared across measured launches. |
| Dusk traversal | `compare tour` additionally accepts `--hour 19`. The requested hour is recorded; effective weather and clock state are unknown. |
| Long memory exercise | `compare soak` accepts `--towns Goodsprings Primm --laps 10 --step 1024 --dwell 1 --every 5 --event-every 20` and requires fnvtool. It retains memory samples, matched-waypoint growth and lap-end leak reports in each child. This is a lap-count exercise, not a guaranteed session duration. Inspect `elapsed_seconds`, `laps_done`, `samples.csv` and memory CSVs before claiming a long-session result. |
| Continuous player movement | Pending. The tour steps by relocation when streaming is ready (`engine/nvf-play/src/stress.rs:tour`); it does not hold movement controls. It cannot establish continuous-walk frame distributions. |
| Stationary scenes | The existing profile campaign already covers these. A dedicated repeated scene interface is pending; do not use `compare scene`. |

[design] A tour/soak comparison checks that all requested laps completed. Soak also preserves its
materialized event schedule and its outcome counts; scheduling an event does not prove it executed.
A fixed-duration soak, fitted memory slopes and minimum long-session duration checks are pending.
Memory remains contextual and receives no new release gate (`comparison.py:scenario_failures`,
`soak.py:outcomes`, `tour.py:growth`, `tour.py:not_released`).

[design] Capture is bounded to the first 60 real seconds, including startup. It cannot restart later
in the same process (`engine/nvforever/src/frame_telemetry/mod.rs`, `plugin.rs`). The default selected
window begins ten seconds after entry into uninterrupted gameplay and lasts thirty seconds. A loading
transition restarts settling; the first complete window is selected. A frame and its preceding frame
must both be inside the window. First gameplay transition frames are reported separately; slow frames
inside the window are retained. Short captures yield `insufficient_window` (`telemetry.py:window`).

[design] Startup, loading and full captured gameplay statistics are retained separately from the
selected window. Early telemetry on a long tour/soak describes only that early interval; later laps
have the existing coarse and memory reports. Warm-up processes prime only shared caches. OS file
cache, driver shader cache and generated-cache state remain unknown; fresh homes do not establish a
cold machine (`comparison.py:run`).

## Existing profiling campaign

[design] `tools/profile/campaign.py` and `campaign.toml` are the maintainer's ranked campaign. They
already cover stationary, interior, combat, movement and streaming cases and rank CPU/GPU spans.
Use that campaign to find costly scenarios; use this harness to repeat supported tour/battle/soak
cases across two builds. Both reuse `tools/stress/stress/lib.py` for launch isolation, game locks,
crash handling, memory and CPU sampling, and both reuse `battle.layout` for placement. This change
does not duplicate the campaign or claim its scenarios are accepted by `compare`.

## Capture contract and current log inputs

[design] The consumer follows PR #1306's schema 1, including subsequent provenance and shutdown
changes. The implementation reference is `engine/nvforever/src/frame_telemetry/records.rs`,
`stats.rs`, `plugin.rs` and `git_paths.rs`; the contract is [frame telemetry](frame-telemetry.md).
`telemetry.py` validates UTF-8 JSONL, required fields, bounded records, finite values, sequence and
collection-time ordering, phase markers, counters, byte cuts and recomputed phase statistics.
Unknown fields are ignored; unsupported versions and record types are rejected.

[design] Matching uses only the launch's fresh home. Exactly one capture must match its assigned
run/scenario tokens, capture filename and both start and terminal log paths. A hash is retained.
Missing/ambiguous files, malformed input, drops, truncation, absent summaries and unconfirmed flushes
are excluded. A complete trailer alone cannot confirm successful finalization. `requested=false`
legacy inputs explicitly lack percentiles; no values are reconstructed from coarse sample means
(`telemetry.py:validate`, `telemetry.py:consume`).

[design] Input compatibility was checked against these code producers; these are source checks,
not observations of a running engine:

| Input | Producer and consumer |
|---|---|
| Capture header/frame/diagnostic/trailer | `frame_telemetry/records.rs`, `stats.rs`; `stress/telemetry.py` |
| Capture start, complete/truncated summaries, GPU counts and final-flush uncertainty | `frame_telemetry/plugin.rs:log_summary`; `stress/telemetry.py:consume` |
| `stress sample`, `stress start-up`, `stress hitch`, `stress frames`, `stress actors` | `engine/nvforever/src/stress_sample.rs`; existing `stress/tour.py` parsers, also used by battle and soak. Loading-screen hitches remain excluded from legacy visible-frame hitches. |
| Tour lap and waypoint logs, battle start and placement | `engine/nvf-play/src/stress.rs`; `stress/tour.py`, `stress/battle.py`, `stress/comparison.py:scenario_failures` |
| Profile windows and hitch pipeline suffix | `engine/nvforever/src/profile.rs`; `stress/comparison.py:profile_values`, `stress/lib.py:profile_hitches` |

[design] Current aid spellings are `NVF_PLAY_TEST_FRAME_TELEMETRY`, its `_GPU`, `_RUN_ID` and
`_SCENARIO_ID` companions, `NVF_PLAY_TEST_SAMPLE`, `NVF_PLAY_TEST_PROFILE`, `NVF_PLAY_TEST_TOUR`,
`NVF_PLAY_TEST_BATTLE`, `NVF_PLAY_TEST_GOD`, `NVF_PLAY_TEST_UNSEEN`, `NVF_PLAY_TEST_SCRIPT_MENUS`
and `NVF_PLAY_TEST_STRESS`. Soak's clock aid is `NVF_PLAY_AIDS_FROM_SCRIPTS`, without `TEST`, and
its events use `NVF_PLAY_TEST_SCRIPT` and `NVF_PLAY_TEST_KEYS` (`engine/nvf-play/src/test_aid.rs`,
`pause.rs`, `stress.rs`; `engine/nvforever/src/frame_telemetry/mod.rs`, `stress_sample.rs`, `profile.rs`).
The launcher still uses `--data`, `--play`, `--no-messages`, `--run-seconds`, `--cell`, `--pos`,
`--heading` and `--hour` (`engine/nvforever/src/main.rs`).
The route/event helpers still use `fnvtool world list`, `world cells --world`, `world refs --world`,
`nav cells --space` and `sim door-models`, with positional data/plugins and `--schema`
(`crates/fnvtool/src/main.rs:WorldCmd`, `LoadOrderArgs`, `SimCmd`; `nav_cmd.rs:NavCmd`).

## Provenance and interpretation

[design] Every child records runner Git revision/dirty state, supplied executable and fnvtool hashes,
requested INI hash, requested options/aids/masters, OS and CPU facts. Missing metadata has an explicit
unknown value or unavailable reason (`metadata.py:collect_manifest`). Executables and supplied INI
are rehashed after the sequence. Route/event hashes are compared across measured launches. Engine
revision and features come only from a validated telemetry header (`comparison.py:run`).

[design] Main reads the engine checkout revision at runtime, guarded against a checkout changed since
build; it is not embedded at build time (`frame_telemetry/git_paths.rs`). Null revision and its
unavailable reason remain intact. The runner revision, labels and executable filename never substitute
for engine provenance. Engine dirty state is unknown. Renderer/backend/driver and presentation values
come from the header. An Auto present mode is not the driver's concrete mode, and reported refresh is
not measured cadence. Other effective graphics settings, plugin/asset identity and effective weather
remain unknown. Final home INI is hashed; a requested INI hash does not prove every setting took effect.

[design] Background CPU quality uses main's elapsed-time-weighted launch intervals. When other programs
exceed 25% of all cores for more than 10% of sampled time the launch is busy. Unknown CPU quality is
excluded. `below_busy_threshold` describes only that check, not a quietness guarantee. Sampling covers
the launch, not just the selected frame window (`lib.py:CpuLoad`, `machine_load`, `cli.py:comparison_quality`).

[design] For each new metric, the report lists eligible per-run values, median, min, max, spread and
excluded counts. Headline p99 is the median of per-run p99 values; there is no pooled percentile.
Quantiles use nearest rank; hitch thresholds are strict `>` at 33/50/100/250 ms. Legacy `1% low` remains
the mean of the slowest `max(1, floor(N/100))` durations and is not p99 (`telemetry.py:statistics`,
`engine/nvforever/src/stress_sample.rs:frame_stats`).

[design] Descriptive comparison rules require three eligible measurements per side. A change exceeds
`max(relative_threshold * abs(base_median), base_spread, candidate_spread, absolute_floor)`; equality
is within noise. Rules stored in the report are mean/p50: 3%, 0.05 ms; p95/p99: 10%, 0.10 ms;
CPU: 5%, 0.20 ms/frame; spans: 10%, 0.02 ms/frame; hitch rates: 10%, 1 event/1000 frames
(`comparison.py:THRESHOLDS`, `judge`). These are not significance tests or release budgets.

[design] Optional `--aid NVF_PLAY_TEST_PROFILE=5` requests process CPU and spans on both sides. Only
complete profile windows inside the selected interval, with a 0.1 s boundary guard, are weighted by
frame count. Missing spans remain unavailable. GPU mode requires
`--aid NVF_PLAY_TEST_FRAME_TELEMETRY_GPU=1` equally on both sides; asynchronous GPU observations
lack source-frame attribution and are not summed into per-frame totals (`comparison.py:profile_values`,
`frame_telemetry/plugin.rs`).

[design] Existing `--budget-mean-ms`, `--budget-low1-ms` and `--budget-worst-ms` still operate on the
unchanged whole-run post-startup visible-frame population, including placement and traversal costs
(`lib.py:over_budget`, `cli.py:run_once`). No release thresholds are added or changed. Failed budgets,
scenario checks, crashes or hangs prevent an overall no-change result. Unfinished plans, condition
mismatches or missing required metrics are inconclusive. Comparison exit is zero only for no change;
inspect the retained per-metric verdicts for improvements, regressions and mixed results.

## Validation

[design] Synthetic fixtures in `tools/stress/tests/test_comparisons.py`, `test_metadata.py` and
`test_stress.py` cover missing metadata, legacy coarse logs, current schema-1 captures, malformed and
missing captures, completion uncertainty, aggregation, variation, windows, ordering and interruption.
They contain no copied menu fixtures. The new smoke scenario checks joint log inputs; it does not
validate JSONL or the complete repeated command sequence.

```powershell
python -m unittest discover -s tools/stress/tests
python tools/smoke verify
```

[design] The main session should build its baseline and candidate release executables, run the stress
and profile Python suites, then run smoke scenarios `stress-comparison-inputs`, `frame-telemetry-frames`
and `frame-telemetry-gpu` with its own game-data configuration. Finally run the small comparison above
and inspect each launch's capture eligibility and placement checks. Runtime comparison, continuous
movement and a duration-controlled long-session memory case remain unverified or pending.
