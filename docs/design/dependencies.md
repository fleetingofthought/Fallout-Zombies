# Dependency plan: using the Rust ecosystem fully

Owner decision (2026-10-06): use Rust "to the full extent, external libraries and
everything". The first code was std-only only because its build sandbox could not reach
crates.io; that restriction was lifted the same day.

Migration happened crate by crate, each step behind the existing tests and an old-vs-new
comparison of `fnvtool` output on synthetic plugins and archives (strict: key order,
int-vs-float type, values, exit codes). The parity lab needs the owner's game data, so it
still has to be re-run there (84/84 Layer 1 scenarios expected).

## Layer 0/1: done

| Was | Now | Notes |
|---|---|---|
| `fnv-json` | `serde` + `serde_json` (`preserve_order`) | documents built with `json!`; `nvforever.json` was a derived struct (now `NVForever.ini`, hand-written, the old JSON still read); schema files keep duplicate-key rejection via a strict deserializer. Floats >= 1e16 are now written as floats (fnv-json printed them as integer tokens). |
| `fnv-inflate` decoder | `flate2`, **zlib-rs** backend | our zlib header + Adler-32 trailer diagnostics kept (`adler2`). miniz_oxide was rejected: in streaming mode it accepts back-references before the output start. |
| `fnv-util::sha256` | `sha2` (+ `hex`) | |
| `fnv-util::cp1252` | `encoding_rs` (WINDOWS_1252) | |
| hand-rolled CLI parsing | `clap` (derive) | `--help` everywhere; exits 0 for help, 2 for usage errors |
| ad-hoc error enums | `thiserror` (libs) + `anyhow` (fnvtool) | messages unchanged (they appear in JSON) |
| `std::fs::read` of masters | `memmap2` + `bytes::Bytes` | records are zero-copy slices; about half the peak memory |
| sequential scans | `rayon` | per-record/per-file work in parallel, folded in file order so output is identical |
| manual home-dir logic | `directories` | Windows known folders; a redirected Documents folder is followed |
| — | `tracing` + `tracing-subscriber` | debug events for data accepted without being reported; `NVFOREVER_LOG=debug` |
| — | `proptest` | malformed-input properties for every parser; `cargo-fuzz` (nightly) not set up |
| — | `cargo-deny` | `deny.toml`: licences, RustSec advisories, sources; runs in CI |
| — | `fancy-regex` 0.14 (MIT) | JIP's Sv_RegexMatch/Search/Replace and ValidateRegex in fnv-script (jip-strings.md §1): ECMAScript-style patterns need back-references, which the `regex` crate lacks |
| — | `toml` 1 (MIT/Apache-2.0), `parse` + `serde` only, the version the engine already has through Bevy | fnv-util `words` (#1443): the language files of NVForever's own words (`lang/<id>.toml`) |
| — | `chrono` 0.4 (MIT/Apache-2.0), `clock` only | xNVSE GetUserTime in fnv-game (the computer's local time); the standard library has no time zones |
| — | `png` 0.18 (MIT/Apache-2.0), the version the engine already builds, and `qrcodegen` 1.8 (MIT) | fnv-game `image_files.rs` (#1008, SUP batch 5): CaptureScreenshot's PNG files and TextToQRCode's code (ext-sup5.md) |
| — | `wgpu` 30 (`vulkan`) + `ash` 0.38, MIT/Apache-2.0, the versions Bevy already uses | nvforever `memory_budget.rs` (#1170): the GPU's `VK_EXT_memory_budget` through wgpu's Vulkan device (`Device::as_hal`); no wgpu patch |
| — | `tracy-client` 0.19 (MIT/Apache-2.0; its `tracy-client-sys` 0.30.1 carries the Tracy 0.14.1 C++ client, BSD-3-Clause) with `tracing-tracy` 0.12 (MIT/Apache-2.0), through Bevy's `trace_tracy`, the versions Bevy 0.20.1 pins | optional, only in the engine's `tracy`/`tracy-memory` build features (#1329): Bevy's spans sent live to the Tracy profiler; built `ondemand` + `only-localhost`; never in release or CI builds (frame-rate-audit.md §1) |

## Engine layers (candidates, to evaluate when each layer starts)

| Layer | Need | Candidates |
|---|---|---|
| 2 | NIF parsing | **done**: own schema-as-data interpreter (`schemas/nif`, re-expressed from nif.xml as documentation); `quick-xml` codegen rejected to keep the licence open |
| 2 | DDS / BCn textures | **done**: own header/layout parser (`fnv-dds`); GPU upload as BCn directly; CPU BCn decoders deferred (ADR-0001) |
| — | first-start dialogs (game folder) | **chosen**: `rfd` 0.15 (MIT), engine workspace, Windows only, no default features: the system's message boxes and folder picker before the game window opens (install-layout.md, "First start") |
| 3 | CPU BCn encode/decode (generated LOD) | **chosen**: `texpresso` 2.0 (pure Rust, MIT), engine workspace only: decodes terrain textures and encodes the generated LOD's BC1 textures (docs/design/generated-lod.md) |
| 2 | math, viewer | **done**: Bevy 0.20.1 (its `glam`; 0.19.1 until #934), viewer in its own workspace (ADR-0001) |
| 3 | rendering | `wgpu` (Vulkan/Metal/DX12), `winit` for windows/input |
| 3 | temporal upscaling (FSR 3.1) | **chosen (2026-11-05)**: `wgpu-ffx` 0.1.0 and `wgpu-ffx-shaders-spv` 0.1.0 (MIT; the FidelityFX SDK 1.1.4 FSR 3.1.4 shaders as SPIR-V). Not on crates.io, so vendored at commit `6fe7064` with the FSR 3.1 shader sources and the compile tool (the SDK's other effects left out), and moved to wgpu 30 (engine/vendor/README.md). It turns on wgpu's `spirv` and `wgsl` and brings `pollster` 0.4 (MIT/Apache-2.0) and `termcolor` 1.4 (Unlicense/MIT, through naga's `codespan-reporting`). No AMD DLL, no frame generation; Vulkan only ([fsr-upscaling](fsr-upscaling.md)) |
| 3 | scaled main passes (FSR) | `bevy_pbr` 0.20.1 patched (MIT/Apache-2.0, engine/vendor/README.md): the transmission pass and the clustered lights follow `MainPassResolutionOverride` |
| 3 | first Pip-Boy readiness (#1803) | `bevy_pbr` 0.20.1 patched (engine/vendor/README.md): the optional `MaterialQueueProbe` records watched main-pass entries for the rig's readiness check; nothing changes without it |
| 4 | physics / character controller | **chosen**: `rapier3d` 0.36 with `enhanced-determinism` (pure Rust, Apache-2.0, no Bevy coupling; Jolt bindings rejected for the C++ build and FFI, `avian3d` for its Bevy coupling; layer4.md §2.8). Used by `fnv-phys` only, which runs rapier in metres internally and exposes game units; terrain as triangle meshes (rapier's heightfield missed rays along grid lines). Pinned to `=0.36.0` (traces are compared bit for bit; licence Apache-2.0, its new dependencies `glamx`/`glam`/`parry3d` MIT/Apache-2.0, within deny.toml). docs/spec-notes/physics.md |
| 4 | entity model | **decided**: plain ordered maps in `fnv-sim`, no ECS in the core (layer4.md §2.2); `rand_chacha` for the seeded, reproducible RNG |
| 5 | script runtime | in-house bytecode interpreter (no off-the-shelf fit) |
| 7 | audio decode / playback | `symphonia` (WAV/MP3/OGG), `kira` or `cpal`. **Dialogue voice (2026-10-10)**: no new dependency. Playback uses Bevy's own audio (`bevy_audio`, already built by Bevy's default features: `rodio` with `lewton` for Ogg Vorbis; WAV, MP3 and FLAC decoding are not enabled and not needed, every voice line has an `.ogg`). File lengths come from `crates/fnv-voice`'s own header readers (Ogg pages and the Vorbis identification header, RIFF chunks), so the headless tools need no audio crate (docs/spec-notes/voice.md §5). **Music and ambient sound (2026-10-11)**: `nvf-play` depends on `rodio` 0.22 directly, the version Bevy's audio is built on (no second copy), with its `mp3`, `vorbis` and `wav` features; these bring in `symphonia` (MPL-2.0, already allowed in `deny.toml`). Music files are MP3 and many ambient sounds Ogg Vorbis; they are decoded while they play by a source of our own that mixes a media set's three layers (docs/spec-notes/audio.md §7) |
| 7 | save thumbnails | **chosen (2026-10-15)**: `png` 0.18 (MIT/Apache-2.0) in `nvf-play`, the version Bevy's image loading already builds (no second copy): encodes the 512×288 thumbnail and decodes it for the load page (docs/design/save-load.md §6) |
| — | code mods (WebAssembly) | **chosen (owner, 2026-11-02)**: `wasmtime` (Apache-2.0 WITH LLVM-exception) with the Component Model and `wasmtime-wasi` (granting nothing), pinned exactly for determinism; not added yet. Phase M1 adds it and allows `Apache-2.0 WITH LLVM-exception` in `deny.toml`. `wasmi` (an interpreter) was rejected: too slow for the per-frame visual tier. docs/design/code-mods.md §6, §17 item 5 |
| 7 | UI (Bethesda XML menus) | own forgiving reader (`crates/fnv-ui`; the files are not strict XML, ui-tile-tree.md §9, so `quick-xml` was not used) + own layout engine; `egui` for dev tools |
| video | Bink intro | **chosen (2026-10-15)**: `infinitier_bik_decoder` 0.0.14 (GPL-3.0-or-later, as NVForever; pure Rust, a port of FFmpeg's open-source Bink video 1 and Bink audio decoders, not of any leaked code) in `nvf-play`, pinned `=0.0.14`; with it `infinitier_datasource` (GPL-3.0-or-later), `hound` (Apache-2.0), `bitflags`. Plays `Video\FNVIntro.bik` before a new game (`engine/nvf-play/src/intro.rs`): about 4.3 ms a 1280×720 frame on the owner's machine, decoded on its own thread. No FFmpeg. Behind the cargo feature `intro-video`, **on by default** since the owner settled the licence as GPL-3.0-or-later (2026-10-15); `--no-default-features` leaves it out |

## Licence check

**NVForever is GPL-3.0-or-later** (owner decision 2026-10-15, settling the earlier
placeholder): dependencies may be permissive or GPL-3.0-compatible copyleft. Most are
MIT/Apache-2.0 or similar, one MPL-2.0 (`option-ext`, via `directories`), and the intro
film's decoder (`infinitier_bik_decoder`, `infinitier_datasource`) GPL-3.0-or-later.
`cargo deny check` enforces the allow-list in `deny.toml` and RustSec advisories; CI runs
it on every push to main.

## CI

`.github/workflows/ci.yml`: one Linux job, run on pushes to main (and manually), that does
`cargo deny`, tests and clippy (`-D warnings`) for both workspaces and the Python lab/capture
self-tests. Pull requests, other branches, Windows and macOS are not run in CI (billed
minutes); the local checks in CLAUDE.md cover them before each push. The parity job that needs game data stays in
`parity-lab/ci/github-actions.yml` (self-hosted runner only).
