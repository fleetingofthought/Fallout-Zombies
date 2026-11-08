# UI tile tree: ranked tasks

The design is [ui-tile-tree](../design/ui-tile-tree.md), the HUD's part of it
[hud-xml](../design/hud-xml.md) (sources, captures, order of work), the format
[menu-xml](../spec-notes/menu-xml.md). Each task is sized for one session and ends in a
PR. The HUD and MCM come first: 16 plugins in the owner's lists use SetUIFloat and its
family (modlist-scan-2026-10-26.md). Mark a task done with its PR; move what it settled
into the spec.

Checks for every task: `fnvtool ui menus` over the vanilla Data folder still gives 49
menus (smoke `ui-menus`). Run it with the owner's MCM, Clean Vanilla HUD and Vanilla UI
Plus folders as `--overlay` when a task touches loading. A task that changes what is drawn
leaves before and after pictures of NVForever (main and the branch, same scene and size,
e.g. smoke `NVF_PLAY_TEST_SHOTS`) in the ignored `pr-pictures/<branch>/` folder, for the
owner to add to its pull request or issue (owner, 2026-10-29).

## Done

- **T1. Reader, tree, evaluation, API.** Done (claude/ui-tile-tree-f0fb06).
  - `crates/fnv-ui`: the forgiving XML reader, tiles and traits, includes and templates,
    paths with `*` and `:N`, the operators, change-tracked updates, injection.
  - `fnv_game::ui_functions` and `Game::ui_tree`.
  - `fnvtool ui menus|tree`, smoke `ui-menus`.
  - The spec note.
- **T2. The UI functions wired into the extender.** Done 2026-10-28
  (claude/ui-functions-extender). Upstream wired the same call on 2026-10-26
  (dc19c50e, claude/ext-compiler); the merge of 2026-10-30 keeps one call, at upstream's
  place, and the VM's one formatting path (`format_at`).
  - `nvse::call` hands the names no other family takes to `ui_functions::call`, before
    the `Set…` record setters, which would take SetUIFloat for one.
  - The VM formats the format string of SetUIStringEx, SetUIStringAlt,
    InjectUIComponent (after the path) and AddTileFromTemplate (the path itself) before
    the call, for the JIP and the xNVSE functions alike.
  - Not built list (xnvse-remaining.md §3) down by nine.
  - Test: `crates/fnv-game/tests/ui_functions.rs`, `compiled_scripts_reach_the_tree`
    (scripts compiled from source, run by the VM).
- **T3a. The tree in the running game: start-up and updates.** Done 2026-10-28
  (claude/ui-tree-in-game).
  - `fnv_game::ui_loading`: a file source over the VFS, with the load order's GMST
    strings copied in for `&-sName;`; the menu files indexed by `<menu name>`
    (`fnv_ui::MenuIndex`, prefabs left out); `globals.xml` and the six always-loaded
    menus built at start and again after a save is loaded (the tree is not saved, §9).
  - `nvf-play/src/tiles.rs`: `screen()`'s width and height from the window before the
    scripts run, `Ui::update()` after them (design §4.1). The window's size is what
    `fnvtool ui` uses [hypothesis: menu-xml.md §4.2, `screen()`]; `cropx`, `cropy` and
    `resolutionconverter` are not set until their units are known.
  - A file's mends are logged once; a missing menu is a warning; one summary line
    (`ui: 6 menus loaded …`).
  - Tests: `crates/fnv-game/tests/ui_loading.rs`, `fnv_ui::index`; smoke
    `ui-tree-in-game` (a console line reads the HUD's `depth`, sets it, reads it back).

- **T3b. The other menus' lifetime.** Done 2026-10-29 (claude/ui-menus-lifetime).
  - `MenuIndex` maps the menu's own `<class>` constant to its file; 40 menu class IDs
    from Hardcoded UI Shortcuts, revision 39903, also give `Value::Sym` its number.
  - `fnv_game::ui_loading::sync` builds open menus and drops closed ones, retaining the
    six persistent menus; missing classes/files warn once. Reopening starts fresh,
    including injected tiles [documented: InjectUIComponent notes].
  - `nvf-play/src/tiles.rs` writes before scripts; activation and menu keys are checked
    again before the script tick. `Paused.0` and `MainMenu.open` keep StartMenu behind
    our own pause/main menus (owner-review item 34). Drawing stays as it is;
    `ui_taps` stay queued for T8 [design].
  - Tests: `fnv_ui::index`, `fnv_ui::value`, `crates/fnv-game/tests/ui_loading.rs`;
    smoke `ui-menu-lifetime` reads an open message menu's depth and checks build/drop.

- **T4a. HUD places and visibility.** Done 2026-10-29 (claude/hud-code-traits).
  - `fnv_game::ui_hud` writes every observed piece's x/y, `CNDArrows` height and
    shown visibility values from hud.md §14; `screen()` supplies the aspect ratio.
    W and W/2 truncate before offsets [hypothesis: 1320 for AP at 16:9, keeping the
    reticle's half-unit offset]. Hidden pieces read 0 [observed: hud.md §14.3]. Undrawn pieces
    keep the file's visibility; the current HUD keeps drawing [design].
  - `nvf-play/src/tiles.rs` mirrors the current HUD's showing rules and writes after
    lifetime sync, before scripts [design: §4.1]. Subtitle selection is shared with
    `script_hud`. Script-set places/visibility last until the next frame's write
    [hypothesis; original overwrite frequency remains open, hud.md §14].
  - Tests: `crates/fnv-game/tests/ui_hud.rs` (synthetic XML, all three captured aspect
    ratios, visibility, script writes, no loaded HUD); smoke `ui-hud-code-traits`
    (console GetUIFloat at an explicit 1280 x 800).
  - 2026-10-29, from the states capture (hud.md §14.1): the pieces' `visible` keeps
    its value shown or not, except `ReticleCenter` 0 and `EnemyHealth` 8 while aiming
    (`Shown::aiming`); `sneak_nif` and the four radiation tiles show through their
    alpha. The root's `_…` traits never changed in any state, so nothing writes them.

- **T4b. HUD file tiles' strings.** Done 2026-10-29 (claude/hud-code-traits).
  - [design] `ui_hud::Texts` carries plain strings from the drawing's shared helpers
    and constants; the adapter writes them before scripts (ui-tile-tree.md §4.1).
    Paths and values follow hud.md §14.2 [observed]. Sneak and prompt strings retain
    their last value when absent; missing file tiles are skipped.
  - The dehydration fallback is `H20` (zero) [observed: hud.md §14.2], including the
    `hud-needs-words` and `hardcore-setting` smoke expectations.
  - Tests: `crates/fnv-game/tests/ui_hud.rs`, synthetic file tiles, every string,
    retained sneak/prompt strings, +0, plugin words and missing tiles.

- **T4c. The HUD's captured states, bars and compass.** Done 2026-10-29
  (claude/hud-code-traits). T4 is done with it; what it leaves is under T5.
  - Captures in the original (xNVSE, then JIP 57.3; private jobs and results in
    `C:\fnv-private\hud-capture`, builders in its `tools\`): states (hud.md §14.1),
    strings (§14.2), corners (§14.3), the compass and crosshair by name (§14.4), the
    heading sweep (§14.5). JIP changed none of xNVSE's numeric reads.
  - Fixed to match: the radiation word "RAD/SEC", the bars' ticks rounded down
    (`floor(share x 37)`), the condition fill `round(59 x condition)`, the compass's
    heading at x 148 of the window (was the middle) with marks shown within 139 units;
    smoke `hud-bars-partial`, `hud-compass-north`. Before/after pictures in
    `pr-pictures/claude-hud-code-traits/`.
  - Settled as they are: the strip's scale (1024 a turn); the original's crosshair does
    not spread (Dynamic crosshair off is its look, reticle.md §6).
  - Smoke `reputation-box` fixed 2026-10-29: the Goodsprings marker (0x000CDF51 at
    -70992, 2560) is about 53 degrees east of north from the start (-71680, 2048), outside
    the 139-unit (about 49 degree) band from the default heading 0 [observed: the log's
    positions]. The scenario now starts at `--heading 50`, facing the town; its
    expectation is unchanged (passes with `hud-compass-north`).

- **T5. Tile renderer v1, drawing injected tiles over the HUD.** Done 2026-10-29
  (claude/t5-renderer); parts A and B below.
  - `nvf-play/src/tiles/draw.rs`: each frame the mod roots of `HUDMainMenu` are laid
    out (`layout.rs`) into pieces (pure, `pieces()`), the Bevy nodes rebuilt only when
    they differ: boxes for rects with their own colour, atlas or file pictures, fnv-font
    text with justification and wrapping, alpha, visibility, the colour rules and clips,
    one root at `GlobalZIndex` 60 over the HUD. Units: 960 to the view's height
    (`interface::u`). Font n is `interface::FONT_FILES` (moved from `credits.rs`), none
    or 0 font 1. What each trait is taken to mean: menu-xml.md §3.1.2. Log line
    `tiles: HUDMainMenu: N mod tiles drawn (M items)` when the counts change.
  - Which tiles: `Ui::mod_tile_drawn` (Injected/Template always, Code never, File only
    against the archived file's name paths, `Ui::archived_name_paths`). The archived
    copy under a loose file is read since #975 (`fnv_vfs::Vfs::read_archived`,
    `Source::read_archived`, `fnv_ui::tree::Archived`), so a loose HUD replacement draws
    its own file tiles, the archived names read once per file (`draw.rs`
    `archived_names`); `fnvtool ui mod-tiles --menu …` shows the same comparison (test
    install: vanilla 0, Clean Vanilla Hud 0, Vanilla UI Plus 5, its `VUI+` outlines).
    Tests: `fnv_vfs` `the_archived_copy_under_a_loose_file_still_reads`, `fnv_ui`
    `the_archived_copy_under_a_loose_file_gives_the_names_compared_with`, `nvf-play`
    `a_loose_hud_over_an_archived_one_draws_its_own_tiles`.
  - Carried over from T4, done: text widths written back before scripts
    (`fnv_game::ui_hud::write_text_widths`): the sneak word, the prompt's `button_text`
    (its hotrect follows by the file's expression), the radiation value; the observed
    widths are the font files' advances (menu-xml.md §3.1.2).
  - Tests: `fnv_ui` `a_loose_replacement_draws_only_what_the_archived_file_lacks`;
    `nvf-play` `tiles::draw` (four: which tiles, colours, depth order and clips, loose
    HUD, font numbers and measuring); `crates/fnv-game/tests/ui_hud.rs`
    `text_widths_follow_the_observed_measures_and_the_hotrect_its_child` (a fake
    measurer with the font files' widths: the fonts are not available headless). Smoke
    `ui-mod-tile-drawn` (a box, picture and text injected by the console, shot
    `ui_mod_tile.png`) and `ui-mod-tiles-vanilla` (fnvtool, 0 tiles).
  - Left: nif and radial tiles; another `.tai` than the shared atlas, `zoom`, `tile`,
    `cropx`, `cropy`; numeric `justify` and system colour numbers; every hypothesis in menu-xml.md §3.1.2 against the original (screenshots of a HUD
    mod at the same view size); a HUD mod from a Viva New Vegas `mods` folder in game
    (JAM's meters after T6).

- **T6. UIO include lists.** Done 2026-10-29 (`claude/t6-uio`).
  - [design] `fnv_ui::uio` parses supported.txt and public/*.txt, both colon styles,
    menu/prefab/tile targets, and condition ASTs (`true`, `?plugin`, `#file`, `!`,
    `&&`, `||`). Malformed input records list/line warnings; normalized duplicates
    occur once, supported.txt first and public lists alphabetically. `Source::list`
    defaults empty and is implemented by MemSource, UiFiles and fnvtool's overlay
    source. `Ui::configure_uio` evaluates once; `Ui::uio` reports per-entry decisions
    and repeated application counts.
  - [design] Menu additions append after ordinary includes; prefab patches run before
    template expansion. Every included file keeps its own File origin and loose
    status. Explicit additions from loose files pass `mod_tile_drawn` even without
    archived names or with a vanilla-name collision; ordinary uses of the same file
    remain separate. The game uses its active load order at startup and menu reopen;
    fnvtool discovers .esm/.esp files in Data/overlays, or accepts `--plugin`.
  - [observed: the data-file excerpts in menu-xml.md §7 item 3] `||`, single-colon
    tile targets, semicolon comments, mixed folder case, and plugin names containing
    a single `&` all occur in the lists. [hypothesis] Append position, first match
    for duplicate tile names, mixed boolean precedence, and the prefab base for `#`
    still need original-engine checks. Unknown words such as `darnui` evaluate false
    and log; negation still applies. The existing `#UII\PerkBrowser.xml` file supports
    that base; optional `#..\ySI\ySI.xml` is absent from this install.
  - [observed: `target/t6-uio/*-menus.json`, `mod-tree.json`, `mod-tiles.json`, generated
    by the rebuilt release fnvtool over read-only `E:\Games\fo-testenv\Fallout New Vegas\Data`]
    The four requested overlays are UIO - User Interface Organizer, Just Assorted
    Mods, MCM Extender and UI Improvements under `E:\Games\Viva New Vegas\mods`.
    Counts below are unique entries, not repeated prefab applications; there were
    no malformed entries or duplicates and five warnings for unknown `darnui` atoms.

    | Inputs / command | Menus | Read | Applied | Skipped: false / file / target |
    |---|---:|---:|---:|---|
    | Vanilla, menus | 49 | 0 | 0 | 0 / 0 / 0 |
    | Four overlays, menus | 49 | 148 | 14 | 27 / 10 / 97 = 134 |
    | Four overlays, HUD tree or mod-tiles | — | 148 | 10 | 27 / 10 / 101 = 138 |
    | Four plus The Mod Configuration Menu, menus | 49 | 148 | 111 | 26 / 10 / 1 = 37 |
    | MCM + Clean Vanilla Hud + Vanilla UI Plus, menus | 49 | 0 | 0 | 0 / 0 / 0 |

    [observed: `target/t6-uio/with-mcm-menus.json`] Without MCM's base files, its
    prefab targets are absent; with them, the sole remaining missing target is
    `CharGenMenu::TM_DescriptionBox` from UII.txt. [documented: task brief] The upstream
    owner's other mods location is `F:\Modlists\VNV\mods`; [observed: these runs]
    only spooknik's `E:\Games\Viva New Vegas\mods` was inspected here.
  - [observed: `target/t6-uio/mod-tree.json`, `mod-tiles.json`] HUDMainMenu now contains
    JAM's JBT, JDC, JHI, JHM, JLM, JVO and JWH roots, including
    `HUDMainMenu/JBT/JBTRect/JBTMeter`. Eligible tile counts are 8, 21, 1, 1, 28, 4
    and 80 respectively: **143 total**; vanilla is **0**. This is renderer eligibility,
    not a claim that invisible tiles appear. The reader still reports mends in JHM's
    XML; JHI/JHM each have only their root in this tree, so visual parity remains
    unverified. [observed: `jam-plugin-check.json`, `jam-plugin-disabled.json`] A
    synthetic entry gated by `?JustAssortedMods.esp` applies with the actual JAM
    overlay's plugin and is false with `--plugin FalloutNV.esm` alone.
  - [observed: `target/t6-tests-final.log`, `t6-clippy.log`, `t6-build-final.log`]
    `cargo test --workspace --release`: **1,641 passed**, zero failed/ignored;
    fnv-ui 35 (seven new UIO tests), game ui_loading 9 (one new), and one new fnvtool
    overlay/plugin test. Workspace/all-targets release clippy with `-D warnings` and
    the release fnvtool build pass. [observed: `python tools/smoke verify`, 2026-10-29]
    **638 scenarios**, valid. [design] New tool scenario `ui-uio-includes` uses only
    synthetic fixtures plus vanilla Data. [observed: `target/t6-uio/synthetic-smoke.json`]
    Its exact fnvtool command applies one entry and draws one tile. No game or smoke
    runner was launched; engine/ was unchanged, so no engine check was required.
    [design] Main session: run `ui-menus`, `ui-mod-tiles-vanilla`, `ui-uio-includes`
    through smoke, and check JAM visually in game (including the XML mends above).

## Next, in order

1. **T5 parts A and B** (done with T5 above; kept for their notes).
   - [design] **Part A done:** tile origins, file paths and loose-file provenance;
     `mod_added_tiles` and code-owned template copies; pure layout in `tiles/layout.rs`
     (no Bevy drawing). [observed] Tests: three fnv-ui provenance/root-selection tests
     and seven headless layout tests for places, depth, alpha, visibility, clipping,
     defaults, kind inputs, measured text, justification and unloading. Assumptions
     are listed in menu-xml.md §3.1.1; drawing and HUD integration remain for later parts.
   - `nvf-play/src/tiles/`: layout as a pure function (tested headless: places,
     sizes, depth order from the traits), then Bevy nodes for rect, image (filename +
     texatlas), text (fnv-font, justify, wrap), alpha, visible, colour, clips;
   - draw only the tiles mods added to `HUDMainMenu` (injected, or not in the vanilla
     file) over NVForever's HUD;
   - test with a tile injected through the console in smoke (vanilla data), then a HUD
     mod from a Viva New Vegas `mods` folder (JAM's meters through UIO after T6).

   Shots in smoke.
   - **Carried over from T4** (hud.md §14-§14.5 have the observed values):
     - **Done: code-made copies** [design]: `fnv_game::ui_hud` builds the corner labels,
       ammo/bars/brackets, compass and marks, reticle and prompt texts from loaded
       templates; `tests/ui_hud.rs` checks names/`*:N`, rebuilds, frame writes and missing templates.
     - text widths written back (the sneak word 106/136, the prompt's hotrect, the
       radiation value), once T5 measures text with fnv-font;
    - **Done: radiation pointer x** [design]: T12b replaces the provisional scale with
       the paired capture rule in hud.md §14.8; tests cover the range and hidden hold.
     - **Done: Subtitles y** [design]: prompt height + 8 (§14.3 [hypothesis]);
       `tests/ui_hud.rs` checks both captured heights and retained y without a prompt (T12b).
     - not captured: an enemy under the crosshair (the enemy bar's children; §14.3 has
       why three tries failed: face the actor with script steps, not a drive script),
       and how the strip itself scrolls (no trait; §14.4; our drawing's scale and
       centre are measured, §14.5).

2. **T6. UIO include lists.** [design] Done; implementation, checks and both mods-folder
   locations are recorded above.
3. **T7. MCM, headless first** (owner decision in owner-review.md first):
   - StartMenu built from the files while the pause or main menu is open, with MCM's
     includes;
   - MCM's own functions (SetMCMString, SetMCMFloat, … origin MCM,
     modlist-scan-2026-10-26.md) as tree writes;
   - a test that runs one MCM mod's menu script in `MenuMode 1013` against the tree and
     checks the traits it sets.
   - **Built 2026-11-01** (claude/mcm-pause-976, issue #976): the MCM shorthands find
     their tile by counting the `MCM` rect's children from 1 in file order, 0 stopping
     at the level above [hypothesis, from MCM's files: menu-xml.md §10; observer check
     69], not through `*:N`; GetSelfModIndex gives the calling script's plugin index
     (it gave 0, which made every MCM mod take MCM's page as its own). Test
     `crates/fnv-game/tests/ui_mcm.rs`: StartMenu built through an MCM-shaped include
     chain with the start menu open, a quest script compiled from source whose
     `MenuMode 1013` block acts only on its own page, fills in its option and takes a
     new value (FOV Slider's way), and stops when the menu closes.
4. **T8. Input on tiles.**
   - Pointer hit-tests over hotrects (`target`, `id`, `depth`): `mouseover` and
     `clicked` traits written each frame, `ref` links followed, `_PCButton_*` keys,
     `xbutton*` from a controller;
   - GetActiveUIComponentName, GetActiveUIComponentFullName, GetActiveUIComponentID,
     ClickMenuButton, ClickMenuTile;
   - the menu click and mouseover events (extender events, design §5.2).
   - **Built 2026-10-30** (claude/t8-input-on-tiles, issue #948): `fnv_ui::input` (hit-test,
     `Pointer::step`: `mouseover`, `clicked` for one frame [hypothesis: `ClickedRule`],
     `shiftclicked`, `_PCButton_<letter>` and the `x…` traits' `<ref>`s),
     `fnv_game::ui_input` (the step with the JIP mouseover-change and menu-click events),
     the five functions in `ui_functions`, the engine feed `nvf-play/src/tiles/input.rs`
     (no menu takes input in play yet; test aid `NVF_PLAY_TEST_POINTER` for the HUD).
     What is settled and open: menu-xml.md §3.2, §8 items 7-10; observer check 54.
     Tests: `fnv_ui::input` unit tests, `crates/fnv-game/tests/ui_functions.rs` (the
     pointer functions, clicks, events), smoke `tiles-hud-pointer`. Not built:
     `wheelable`/`wheelmoved`, `draggable`, `xdefault`, list rows' `:index` in the click
     event, the click and mouseover sounds.
5. **T9. The rest of the tile functions** (design §5.2): SetUIFloatGradual and its
   readers (gradients stepped on real time), SetUIFloatMultiple, GetUIFloatInherited,
   AttachUIXML, AttachUIComponent, AddTraitToTile, GetUIValueType, GetUIElementType,
   SortUIListBox, GetStringUIDimensions (fnv-font metrics), the CacheUI family, the
   HudBar family, the DumpTileInfo family. Each with a headless test.
   - **Built** 2026-10-30 (claude/t9-tile-functions): `fnv_game::ui_more`,
     `ui_gradients`, `ui_cache`, `ui_hudbar`, tests `crates/fnv-game/tests/ui_more.rs`;
     the engine steps the gradients in `tiles::update`. What each page leaves open and
     what is not built (SetUITexOffset, SetUIAlphaMap, SetUITypewriter, the HUD bars'
     script-variable values and visibility conditions, the attached components'
     placing, the engine's text measure): menu-xml.md §9.
6. **T10. The game strings table.** The 145 `&-sName;` names of the vanilla files: the
   load order's GMST first; where the plugins have none, an English label NVForever
   writes itself from the setting's name and the menu's purpose (`sCloseButton` →
   "Close"). Not transcribed from screenshots or read out of the original with
   `GetGameSetting` (LEGAL.md; owner decision 2026-10-30). An observer dump may still
   say *which* names the original fills in, not their text.
   - **T10a done** 2026-10-29 (`claude/t10-strings`): `fnv_ui::strings::GameStrings`
     (the load order's string GMSTs, last plugin winning, then `BUILT_IN`, names without
     case) behind `UiFiles::game_string`; `Ui::game_strings_read` records each name read
     and whether it had text; `LoadReport::game_strings{,_unresolved}` and the start-up
     log line `ui: game strings: N read, M without text`. The names: 145 spellings (143
     without case); only `sMorphine` has a vanilla GMST [observed, menu-xml.md §2.4].
     Tests: `fnv-ui` `strings::tests` (3) and
     `tests::game_strings_resolve_from_the_load_order_then_the_built_in_table`;
     `fnv-game` `ui_loading` (`startup_and_rebuild`, `missing_menu_and_globals_warn`,
     `vfs_source_owns_strings_and_reads_files`).
   - **Readings** 2026-10-29: the private observer job `t10_gmst` ran in the original
     (`GetStringGameSetting`) [observed]: an unknown name reads empty; lookup ignores
     case (both spellings of `sOK` and `sQuit` read alike); 141 of the 143 names have
     text (`sMorphine` from its record); `sModffects` and `sReputationSwap` are empty
     there too. Only which names read text is used; what they read is not recorded and
     not used (the rule above). NVForever matches: unknown names read empty, lookups
     (including `MemSource`'s) ignore case.
   - **T10b done** 2026-10-30 (`claude/t10-strings-table`): `BUILT_IN` holds 140
     entries (name, label, where the label came from), every name the vanilla files read
     that the masters leave undefined except `sModffects` and `sReputationSwap` (empty
     in the original too), plus `sExplosive` (only in a comment); `strings::built_in`
     and `fnv_game::ui_loading::game_string` read it. The tutorial menu and the lockpick
     menu and its help take their words from it by the names their files use. Tests:
     `strings::tests::the_shipped_table_is_one_labelled_entry_per_name`,
     `the_load_order_wins_over_the_shipped_table`. Smoke `ui-tree-in-game` expects one
     name without text at start (`sModffects`).
7. **T11. MCM drawn.** MCM's page from tiles inside NVForever's pause menu (design §7,
    after the owner's ruling), mouse and keys through T8. Done when an MCM mod's
    settings change in game.
    - **Plan** (2026-11-01, after T7; issue #976 stays open for it). In order, each
      with a smoke scenario on the owner's MCM install (vanilla Data has MCM's ESP and
      files, not in the load order by default):
      1. Scripts under the pause menu: while NVForever's pause menu is open with a mod
         that added tiles to `StartMenu`, `Game::menu` is `menus::START` and the
         MenuMode blocks run (`nvf_play::script` skips scripts while paused today;
         `game::menus::START` says "not reported"). Whether every `Begin MenuMode`
         block should run under our pause menu, as the original's start menu does, or
         only with a Mods page open, is the first decision (vanilla has `MenuMode
         1007 || MenuMode 1013` effect scripts).
      2. The Mods entry in `pause.rs`'s column, shown only when
         `Ui::has_mod_added_tiles(StartMenu)`; choosing it opens a page that shows
         MCM the way its trigger does (MCM's own ESP scripts take it from there).
      3. The page drawn from tiles: `tiles::draw` selecting the `StartMenu/MCM`
         subtree (as T12 draws `HUDMainMenu`), sized by `screen()`.
      4. T8's pointer and keys fed to `StartMenu` while the page is open
         (`tiles/input.rs`: no menu takes input in play yet).
      5. Done when an MCM mod's option changes in game and its INI value is written.
    - **Built 2026-11-01** (claude/mcm-pause-976-t11, issue #976): steps 1-4 as the
      plumbing, behind `[UI] TileMenus=StartMenu` (off by default; `nvf-play
      tiles/mods.rs`, `pause.rs`). With it on and a mod's tiles in `StartMenu`, the column
      has a Mods entry before Quit (`pause::ModsEntry`; a tile counts as a mod's as T5
      tells the HUD's apart, so a loose `start_menu.xml` that only reskins the menu gives
      no entry); its page draws StartMenu's mod tiles over the pause menu (`tiles::draw`,
      as T5 draws the HUD's: a loose replacement's own tiles, not the archived file's;
      interface units), feeds them T8's pointer and keys, and each frame runs only the
      `MenuMode 1013` blocks, in the played world, with `MenuMode 1013` true
      (`Game::page_tick`: no GameMode block, no game time, no queued events or OnLoad
      blocks, the open menu left as a block's box or tutorial set it; only on that page:
      our pause menu still holds every script elsewhere, the first decision above kept
      narrow) [design]. Esc, Backspace or Back close it. Tests: `tiles::mods` (the
      entry's rule, a reskin), `pause` (the entries, the page), fnv-game
      `a_page_tick_runs_only_the_start_menus_blocks_and_keeps_the_open_menu`; smoke
      `pause-mods-page` (MCM's own 1013 block clears its `iInstallCheck`).
    - **Blocked: MCM's own page** (step 5). MCM's ESP shows MCM only when
      `StartMenu/NOGLOW_BRANCH/main_container/childcount` is 8 (the original's pause
      column, filled in by the game's code; NVForever's column is hand-built and the
      rule is not documented), places its trigger from that column's x/y/width, and
      fills its mod list with MCM.dll functions no source describes (SetMCMModList,
      GetMCMListWidth, SetMCMFloatMass, SetMCMStringMass: `{{Incomplete}}` wiki stubs,
      not in functions.json). Until a source for those turns up (an observer capture of
      the start menu's `main_container` traits, the functions' effects on the tree), the
      Mods page shows only what a mod's own MenuMode script draws.
8. **T12. The vanilla HUD on tiles**, behind `[UI] TileMenus=HUDMainMenu`. Compare
    with the hud.md screenshots; keep the old HUD the default until the owner agrees.
    - **T12a done** 2026-10-29 (claude/t12-hud-tiles): the switch and the drawing.
      `[UI] TileMenus=` (comma-separated menu names, empty by default; only
      `HUDMainMenu` acted on) read at start from the settings' INI, or the test aid
      `NVF_PLAY_TEST_TILE_MENUS`; resource `tiles::vanilla::TileMenus`. On, the renderer
      draws the whole `HUDMainMenu` tree (`draw::Select::Whole`: file, code-made copies,
      mods) and hides the hand-built corners (with the compass, condition row and
      ammo), sneak word, crosshair and radiation meter (`DrawnByTiles`, one PostUpdate
      system). Kept hand-built, their file/code tiles not drawn (`KEPT_PIECES`): the
      enemy bar, prompt, Hardcore words, XP meter, limbs, breath meter (the one `nif`
      tile), corner and quest messages, subtitles (the texts and the breath meter since
      the T12 leftovers below; none is kept now). The mapping: hud.md §14.7. Off,
      T5's mod-only drawing is unchanged.
      - Renderer [menu-xml.md §3.1.3]: `zoom` (a number: texels × zoom/100 from the box's
        top left, cut to the box [documented: GECK wiki Zoom; hypothesis: one texel a
        unit, the anchor]; none, -1 and `&scale;` stretch [hypothesis for none and
        `&scale;`]); `tile` repeats at one texel a unit [hypothesis]; any `.tai` a
        `texatlas` names, read once and kept (`interface::Pictures::locate`); `clips`
        as T5. The compass strip: the `compass_window` picture twice, moved by
        `hud::strip_left` (1024 a turn, heading at 148 [observed: §14.5; design]).
      - Written only while on (`vanilla::write_corners`): the corner labels' copies at
        the hand-built HUD's places [design: not captured], the right bracket at x -40
        and `AmmoTypeLabel` the ammo abbreviation [hypothesis], the condition ground's
        alpha 64 [design] and the row shown by alpha with a condition.
      - Colours checked against the hand-built HUD's: `&hudmain;` at brightness 255 and
        alpha 255 is `amber(255)`, `&hudalt;` the radiation meter's crime colour (test
        `with_the_switch_on_…`). Differences: hostile ticks and crosshair not red, no
        crosshair spread, warning sneak words in the HUD colour, the corners not hidden
        under a movement lock.
      - Tests: `tiles::vanilla` (3: the switch, aid/INI, corner writes); `tiles::draw`
        (3 new: whole-menu selection with kept pieces and a mod under one, zoom/tile
        fits, a texatlas lookup with a fake index). Smoke `tiles-hud-bars-partial`,
        `tiles-hud-compass-north`, `tiles-ui-hud-code-traits`,
        `tiles-environment-radiation` (shots `tiles_*.png`).
    - **T12b part B done** 2026-10-29 (`claude/t12b-handbuilt`) [design]: hand-built
      radiation scale, fixed need-word places/zoom, faded alpha 124/255, separate word
      and limb alpha flashes, enemy hostility colour and 500-wide bracket (scaled
      marks), XP 2/2/2 s phases with a separate LEVEL UP notice, and limb fades within
      five seconds. [observed: private jobs hud_radiation2, r_in_000-r_in_990;
      hud_hardcore2, hc0-hc11; hud_enemy3, n2_away, g2_gecko_half;
      hud_xp_limbs2, x9_level_up_long; hud_xp_limbs, l1_leg, l2_head]
      - [design: maintainer's choice; needs an issue] Kept M1 enemy 31-tick/current
        rounding rule ([owner, playtest list 5 item 32], hud.md §8); M2 LMB looks
        ([design: owner 2026-10-23/24], §13); M3 only newest limb flashing
        ([design: owner 2026-10-17], §12); M4 5 s LEVEL_UP_DELAY and filled-bar sound
        ([owner 2026-10-24], experience.md §6); M5 `[Empty]` ([owner 2026-10-27],
        reticle.rs). Draft issues and capture contradictions are in those sections.
      - [hypothesis: open] B9 whose health shows: unchanged pending player's-shot and
        longer hostile-aim captures (hud.md §8). Limb timing is one run only; word
        flash period unresolved. Concurrent XP gains remain open (experience.md §6).
      - [design] Main-session smoke: `hud-needs-words`, `hud-crippled-limbs`,
        `environment-radiation`, `xp-meter`, `hud-sneak-target`, `perk-meltdown-anatomy`.
        [documented: tools/smoke/scenarios.toml, paths] Other selected scenarios:
        `opening-no-hud`, `pipboy-general-locations`, `hud-ammo-label`,
        `hud-compass-north`, `hud-bars-partial`, `hardcore-setting`, `compass-reticle`,
        `explosion-radiation`, `tiles-environment-radiation`. [design] Game/smoke runs
        deliberately left to the main session; worker checks below.
      - [observed: T12b part B worker checks, 2026-10-29] With
        `CARGO_TARGET_DIR=E:/Games/NVForever/engine/target`,
        `cd engine && cargo test --release -p nvf-play`: 604 passed, 0 failed;
        0 doctests. `cargo clippy --release --all-targets -- -D warnings`: passed,
        no warnings. `python tools/smoke verify`: all 642 scenarios valid.
    - **Left for T12b, the comparison**: the `tiles_*.png` shots against the original's
      at the same sizes (hud.md §14's captures) and against the hand-built HUD's, every
      hypothesis above; then the kept pieces one group at a time (hud-xml.md step 5):
      the enemy bar's children, the prompt's places and divider, the Hardcore words'
      places, the XP and limb children's visibility, messages, subtitles; the `nif`
      breath meter; hostile colours; the corners' hiding rule.
    - **T12b part A done** 2026-10-29 (`claude/t12b-tiles`) [design]. Evidence and
      private job/label references, each fitted rule's range and unresolved cases:
      hud.md §14.8; menu-xml.md §3.1.3/§5/§8; extender-compat.md's indexing note.
      All ten part-A items of the private `t12b-fixes.md` are implemented:
      1. [design] Numeric systemcolor 1/2 matches hudmain/hudalt, including the
         symbols' numeric reads; unknown colour IDs keep the provisional rule.
      2. [design] Radiation pointer `15 + 0.3 × rads`, 0..1000, written while shown;
         hidden x holds, no 42.9 cap. Fit/range and >1000 hypothesis in §14.8.
      3. [design] Hardcore word alpha/colour by stage, fixed x/zoom in both modes;
         RAD/LMB in both. Unmeasured Fatal, 3+ limbs and flash period tagged in spec.
      4. [design] Subtitle y retained after prompt removal, reset with the tree;
         later prompt height replacement remains a hypothesis.
      5. [design] Hostile tick/crosshair systemcolor 2; hostile crosshair plain.
      6. [design] Info divider and seven text copies in the observed order/places;
         item figures, empty/locked alpha/visible, retained strings; verb only on
         the file hotrect/button_text. Unread name retains template name.
      7. [design] Enemy bracket/meter/name; centred even ticks, hostility colour,
         children hidden at full health. Other target eligibility stays provisional.
      8. [design] Tile-only XP fade/glide and separate LEVEL UP timeline; first gain
         retained during a show. Existing sound/menu scheduling stays in the hand HUD
         and can interrupt the longer visual sequence (hud.md §14.8).
      9. [observed: private jobs hud_hardcore hc0, hud_xp_limbs all, hud_prompt2
         p0-p8] Indexed paths count from the last matching child. [design] Reverse
         creation of the corners/compass/copies preserves observed paths; update
         fnv-ui tests, named and wildcard HUD assertions. Unindexed/draw order stays.
         No contrary vanilla script/menu dependency established; wider audit open.
      10. [design] Limb places/zoom, 0.85/3.3/0.85 s whole-piece fades, all broken
          pieces flashing together, retained broken pictures after healing;
          approximate sawtooth from one run only, documented as a hypothesis.
      - [design] Remove HardcoreMode, Info, EnemyHealth, XPMeter and
        crippled_limb_indicator from KEPT_PIECES. Share their visibility markers
        through tiles/vanilla.rs: only declarations/imports change in the five
        hand-built files. Leave sneak_gauges.rs alone. Text zoom draws scaled glyphs;
        prompt/enemy/XP code text widths use game fonts.
      - [design] Tests: `crates/fnv-game/tests/ui_hud.rs` capture paths/states,
        enemy widths/colour/full health, all needs stages/both modes, retained
        subtitles/pointer, XP timeline/repeat gain/pause, limb restart/healing;
        fnv-ui indexed paths and numeric colours; tiles renderer numeric/inherited
        colour, text zoom and kept selection. Required check results recorded below.
      - [design] Smoke: existing four `tiles-*` scenarios require the exact five
        remaining hand-built pieces; new `tiles-hud-hardcore` raises needs with the
        existing script aid, checks exact alphas/colours and fixed places in both
        modes, with two shots. No expectations loosened.
      - [observed: local `target/t12b/root-tests-final.log`,
        `root-clippy-verified.log`, `engine-tests-verified.log`,
        `engine-clippy-verified.log`, 2026-10-29] Required release checks pass:
        workspace tests **1,647 passed**, zero failed/ignored (HUD 24, fnv-ui 36);
        nvf-play `tiles` **18 passed**, zero failed/ignored, 582 filtered out;
        workspace/all-targets and engine/all-targets clippy with `-D warnings`.
        Engine commands use `E:/Games/NVForever/engine/target`, waiting for its lock;
        root commands use this worktree's target. [observed: `python tools/smoke
        verify`, same date] **643 scenarios**, valid. No game, smoke run,
        `cargo build -p nvforever` or push was performed [observed: this session's
        command log].
    - **Left for T12b's visual comparison** [design]: main session runs
      `tiles-hud-bars-partial`, `tiles-hud-compass-north`, `tiles-ui-hud-code-traits`,
      `tiles-environment-radiation`, `tiles-hud-hardcore`; compare the `tiles_*.png`
      shots against original captures at matching sizes. No game/smoke run in part A
      at the user's request. Still kept: breath NIF, message/quest/location/subtitle
      copies. Still open: enemy eligibility, near-full tick quantization, higher-level
      XP pointer/repeat gains, menu/sound coordination, flash period, a second limb
      capture, text/image zoom anchors, corners' hiding rule and warning sneak colour.
    - **T12c done** 2026-10-29 (`claude/t12c-layout`): the tile HUD's layout bugs, against
      the hand-built HUD and the original's own screenshots (private
      `C:\fnv-private\capture\shots`, `hud_corners2` c0/c4, `hud_corners` rad_coc_spot,
      `hud_jip` s4; read, not shipped). Rules in menu-xml.md §3.1.2-§3.1.3.
      - Ammo type label (white, over the condition bar): **colour** a tile with no
        `systemcolor` on or above it takes system colour 1, the HUD's [documented: GECK
        wiki Color UI directives]; **place** `write_corners` sets it right of the
        divider on the counter's line, as the hand-built HUD [design: its captured place,
        W - 236, was read with no abbreviation shown, and `&right;` puts one over the
        bar].
      - Condition fill (two bands): `vats_bar.dds`'s drawn block is 74 x 11 of its
        128 x 16 texels [observed], so tiled at a texel a unit its clear rows showed;
        the original's fill is solid over its 20 units [observed]. A tiled picture now
        takes a numeric `zoom` [hypothesis], and `write_corners` writes the fill's zoom
        100 × 20/11 (`CONDITION_FILL_ZOOM`) [design: how the original fills it is not
        traced].
      - Radiation bracket (ends and upright missing): an image without `texatlas` reads
        its own file, not the shared atlas, and no `zoom` stands an own file at its
        texels' size (atlas pieces still fill their boxes) [observed: the original's
        own-file wedges and its upright at W - 57, where 512 texels from x -135 put it;
        hypothesis: the rule]. `RAD/SEC` above `+14`: every font's first line then stood
        `TEXT_BASE` 24 under the text's top (was the font's tallest glyph, 33 for font 7,
        48 for font 8) [hypothesis: radiation texts, the sneak word and the prompt's
        lines on the original's screenshots]; the corner labels' writes use it, so they
        stay put. (Superseded by #979, 2026-10-31: a line per font, `draw::text_base`,
        hud.md §14.10; the one shared line was a mis-measurement.)
      - **Not changed: the brackets' shapes and the compass's doubled line.** The tile
        HUD draws the atlas pieces the files name, and the original's screenshots show
        those (the cut corners, the stepped arrow ends, the doubled long line, the
        strip's doubled line with thick middles) [observed]. The hand-built HUD draws the
        pictures' own files (`hud.rs` `load_art`), which are different art: it is the
        one that differs from the original there. Whether to move the hand-built HUD to
        the atlas pieces is the maintainer's call.
      - Other differences seen, not traced: the original's condition fill is 175/255 of
        the HUD colour and its ground an opaque 100/255 (the hand-built 255 and a quarter
        alpha, the tile 255 and alpha 64) [observed: `hud_corners2` c4, rows 665-679].
      - Tests (`tiles::draw`): `where_a_picture_stands_in_its_box`,
        `a_tile_with_no_system_colour_above_it_is_the_huds`,
        `texts_of_two_fonts_at_one_y_share_a_line`; `zoom_and_tile_…` and
        `with_the_switch_on_…` updated; `tiles::vanilla` corner test checks the label's
        place and the fill's zoom. Only `cargo check -p nvf-play --tests` was run.
    - **T12d done** 2026-10-29 (`claude/t12d-smooth`): three differences the owner saw
      playing, fixed in both drawings (hud.md §14.9, menu-xml.md §3.1.3).
      - Compass marks stepped: places looked at ten times a second and kept in whole
        units, and the tile HUD respawned every node when one moved. Now
        `compass::place` works them out every frame, unrounded, before the tile writes
        (actors follow their moving bodies), and both drawings move the nodes in place
        (`tiles::draw::change`, the hand-built marks while their kinds stay)
        [design].
      - Ticks flat: the picture's rows and alpha were already drawn; the original's
        ticks are about 0.7 of the line's colour [observed: `hud_corners2.007`], drawn
        now at brightness 175 (`hud::METER_BRIGHTNESS`, `write_corners`) [hypothesis:
        brightness.xml's meter entry; observer-checks.md item 34]. The sRGB-against-
        linear blending difference is left [hypothesis].
      - Ticks uneven: in the tile-drawn HUD every tiled picture (corner bars, enemy
        bar, condition fill and ground) is one image of the repeated picture, stretched
        and sampled linearly (`crate::repeat`): even, soft ticks as the original's
        [design]. The hand-built HUD (the default) keeps the maintainer's crisp
        whole-pixel ticks (`tick_pixels`/`tick_lefts`, a node per tick; hud.md §4, the
        owner's choices) [design]. The XP meter has no tiled picture. The menus'
        quantity and pause meters (`menu/look.rs`, `pause.rs`) still draw a node per
        tick.
      - Tests: `hud::ticks_stand_on_whole_pixels` (kept),
        `tiles::draw::a_tiled_meter_is_one_image_and_its_ticks_stand_evenly`,
        `tiles::draw::only_moved_pieces_are_moved_in_place`,
        `compass::the_marks_move_smoothly_with_the_heading_and_the_bodies`,
        `repeat::{a_piece_repeats_from_its_top_left, a_box_holds_whole_texels}`; the
        `tiles::vanilla` corner test checks the brightness. Only `cargo check -p nvf-play
        --tests` was run.
    - **T12e done** 2026-10-29 (`claude/t12e-vanilla`): the remaining differences of the
      three-way comparison at 1280 x 720 (hud.md §14.10; the original's frames private).
      Tile HUD (follows the original): `AmmoTypeLabel` at its captured W - 236 with the
      condition row moved left by the abbreviation's width + 20
      (`vanilla::condition_shift`) [observed; hypothesis: the rule]; text `zoom` no
      longer applied (the need words as tall as "HP") [observed]; the right bracket at
      x -36 (`vanilla::RIGHT_BRACKET_X`; at -40 its divider, upright and cap stood 3
      pixels left) [observed]; LMB alt red at three limbs [observed]; condition fill
      175 and ground an opaque 100 [observed, T12c]; the strip's N at 154 and its right
      end fading (`crate::compass_strip`) [observed]. Both: off-band quest targets held at
      the compass's end (`compass::pinned`) [observed; hypothesis: the side]. Hand-built
      (the default): the need words at the font's size; the AP ticks' clear strip after
      each tick and the health bar at x 20 (the "meets the upright" line was an
      observation, not the owner's); the atlas pieces for the brackets and the strip;
      the radiation bracket unstretched, the owner's place and texts kept on the screen;
      LEVEL UP 16 above the amount's line; the condition row's brightness; the strip as
      the tile HUD's [observed]. **Maintainer's choices kept:** the ammo abbreviation
      right of the divider (owner 2026-10-25), LMB's looks (owner 2026-10-23/24), and
      those T12b listed. **Needed a capture then:** the tile HUD's LEVEL UP (font 8's
      line under its `y` against `radiation_text`'s, §14.10), done since: captured
      2026-10-31, a line per font (#979). **Still needs a capture:** the side a target to
      the right is held at. Smoke: `tiles-hud-ammo-label` (new); `hud-compass-north` logs the
      strip's place. Only `cargo check -p nvf-play --tests` and `cargo check -p fnv-game
      --tests` were run, and `python tools/smoke verify` (644 scenarios, fine).
    - **T12 leftovers: the texts, done** 2026-10-29 (`claude/tile-hud-texts`): the corner
      messages, quest and discovery notices and subtitles drawn from tiles (hud.md
      §14.11; `engine/nvf-play/src/tiles/notices.rs`). `KEPT_PIECES` is empty (the
      breath meter, the last kept piece, is drawn from its `nif` tile since the entry
      below). `Region_Location` is no longer kept: at the first discovery a label and a
      name copy are written under it, at the quest banner's places moved to its top left,
      holding the banner's strings and alpha but `visible` 0, so a discovery is drawn
      once, from `QuestReminder`'s copies [hypothesis: hud.md §14 maps discoveries to
      `Region_Location`; what the original shows there on entering a named place is not
      known].
      - Copies [observed: private run hud_traits run1, `*:N` at sample 0]: `Messages`'
        bracket `*:0`, text `*:1` (x 130, y 5, depth 2) and icon `*:2`, made in that order
        before any message; `Subtitles`' centred text `*:0` at x 400 with " ", hidden;
        nothing under `QuestReminder` before a notice, so its copies are made at the first
        one, directly under the piece [hypothesis: their parent], at the hand-built
        HUD's places (quest-messages.md §4) less the piece's (45, 200); a discovery is a
        banner there, where the owner's screenshot shows it [observed].
      - Rules kept hand-built [design]: the queues, times, fades, AutoDisplayObjectives
        hold, sounds and the subtitle selection; the tile copies take the lines on screen
        and their strength each frame (`HudTiles::notices`), the hand-built areas are
        hidden (`vanilla::{MessagesArea, QuestArea, SubtitlesArea}`) and their clocks
        run. Only while the switch is on: off, nothing is made or written. In a menu the
        tile renderer draws nothing (the root's `visible` 0), and the hand-built
        subtitles show instead (`vanilla::{hide, root_drawn}`), so a line said while a
        message box is up is not lost (maintainer's review of #952).
      - Wraps 330/570/800 and the box's x -16, 26 x 26 are the hand-built HUD's
        [hypothesis for the wraps; observed on screenshots for the box]. Differences: the
        quest name fades as one tile (no letter sweep), wrapped lines inside one tile at
        the font's size, the subtitles under `Subtitles`' y (660, raised after a prompt)
        rather than the hand-built fixed base 633 [design].
      - Tests: `tiles::notices` (7),
        `tiles::vanilla::the_hand_built_subtitles_stay_while_the_tiles_draw_nothing`.
        Smoke `tiles-hud-notices`, `tiles-hud-discovered`, `tiles-hud-subtitles` (new,
        shots `tiles_hud_*.png`); every `tiles-*` start line now ends `hand-built HUD
        hidden` (no piece kept). **Needs a capture:** the
        copies' traits while each kind of line shows (alpha over time, `wrapwidth`,
        template, parents under `QuestReminder`, whether `Region_Location` gets children
        of its own on entering a named place).
    - **T12 leftovers: the breath meter, done** 2026-10-29 (`claude/tile-hud-breath`):
      `BreathMeter`, the HUD's one `nif` tile, drawn from its tile (hud.md §14.12;
      `engine/nvf-play/src/tiles/model.rs`); `KEPT_PIECES` is empty and the start line
      ends `hand-built HUD hidden`.
      - The tile [documented: the menu file]: `Interface\HUD\AirTimer01.NIF`, `animation`
        `Forward`, 64 x 64, `&hudmain;`, alpha 0, no place (T4 writes W/2, 110 [observed]).
      - The renderer draws `nif` tiles (whole menu only) by the hand-built meter's reader
        and spawner (`hud_breath::{MeterModel::read_sequence, spawn_pieces}`): a model's
        flat textured squares as turned pictures, not a model rendered to a texture (no
        3D path; a model without flat pieces draws nothing). The model's origin at the
        tile's x, y, a model unit a unit [hypothesis: the meter centred at W/2, as the
        original's]; the hand-built meter keeps the owner's place 104 down.
      - Rules kept hand-built [design]: when it shows (`hud_breath::shown`) and the share
        of the breath used, written only while the switch is on (`vanilla::write_breath`:
        `alpha` 255/0 [hypothesis], `_NVFSequenceFraction`, NVForever's own trait), the
        pieces set each frame by `model::animate` without new nodes; the hand-built root
        hidden (`vanilla::BreathRoot`).
      - Tests: `tiles::model` (4), `tiles::vanilla` (2 new). Smoke `tiles-hud-breath`
        (new, shot `tiles_hud_breath.png`); the six `tiles-*` scenarios that quote the
        start line expect `hand-built HUD hidden`. **Needs a capture:** `BreathMeter`'s
        traits through a dive and a screenshot of the original's meter (centre, size,
        which tick goes first).
9. **T13. Dependency index**, only if T3–T12 show `Ui::update` in a frame profile.
10. **T14. Observer captures for menu-xml.md §8**: the constants' numbers, mismatched
    end tags, whether a set trait holds, whether SetUIFloat makes a trait, the
    deprecated forms' bug. `name:N`/`*:N` counting and HUD colour IDs answered by
    T12b's captures [observed: hud.md §14.8]; deprecated GetUIFloat parity still open.
    Fold the remaining answers into
    the spec and the reader.
11. **T15 onward: one menu per task**, in the order of design §6.2 item 4. Each is
    switch, drawing, the model writing `io()` traits and template rows, screenshot
    comparison, removing the hand-copied numbers:
    - message box, quantity, sleep/wait, tutorial;
    - container, barter, inventory, recipe, repair;
    - Pip-Boy STATS, ITEMS, DATA (into the screen texture);
    - dialogue, level-up, character creation, casino games, terminals, lockpick,
      hacking, VATS.
    - **T15a plan: the message box and "How many?" from their files** (2026-10-31,
      `claude/t15-menus-a`, issue #978; T15b, sleep/wait and the tutorial menu, follows
      the same shape). One switch per menu, the hand-built drawing stays the default.
      **Builds on #979** (PR #1166, `claude/tile-text-baseline`) and its per-font
      follow-up (18088f1e, a text line, pitch and height for every font), merged into
      the branch: those must merge first.
      - **Switch**: `[UI] TileMenus=` takes `MessageMenu` and `QuantityMenu` beside
        `HUDMainMenu` (`tiles::vanilla::TileMenus`, its test aid
        `NVF_PLAY_TEST_TILE_MENUS`). On, and with the menu's file built into the tree,
        the hand-built box (`message_menu.rs`) or prompt (`menu/look.rs::spawn_quantity`)
        is not spawned and the tile renderer draws the menu's whole tree at the
        hand-built box's stacking level. Off, no writes, drawing or tile input for them;
        but `QuantityMenu`'s tree builds while the prompt shows (below), and the `_name_`
        rule holds for every menu (below). Hotrect alpha uses the T15 switch gate [design].
      - **The tree has the menu**: `MessageMenu` builds while a message box is the open
        menu (T3b). The prompt is no `Game::menu` (MenuMode does not report it), so
        `Game::ui_prompt` (new, set by the engine each frame from `ActiveMenu`) builds
        `QuantityMenu` (1016) while the prompt shows.
      - **What the model writes** (`tiles::menus`, only while its switch is on, before
        scripts): MessageMenu: the title and text strings, the icon's file and
        `visible`, one `MM_ButtonTemplate` row per button under `MM_ButtonList` (its
        `string`, `listindex`, `_y`, `height`), the list's `width`, `height`,
        `_highlight_y`, `_selected_height` and the scroll bar's item counts, and
        `MM_MainRect`'s `width` ("set in code" in the file); QuantityMenu: the meter's
        `user0` (the stack) and `user2` (the amount), from which the file works out the
        meter's fill and the amount text. Both: the background's opacity (set in code).
        Every text tile of a drawn menu gets its measured `width` and `height` (as the
        HUD's measured texts), only when its string, font or wrap changed.
      - **Tree rules the files need** (menu-xml.md): `screen()` in interface units (960
        high; both files centre their boxes by it) and `resolutionconverter` 960 / the
        window's height (lines 2 pixels), both only while a T15 menu is switched on;
        a hotrect's own `alpha` not passed to its children under that same gate (every
        text button is an alpha-0 hotrect over a visible text), and `copy … trait="_name_"` after a
        constant (`&false;`) reading `_name_0` (the fade lines' and the meter's picture).
      - **Input** (T8): the menus take tile input while drawn from tiles. A pointer or
        script click on a button row answers the box (`script_hud::menu_keys`), the
        pointer over a row frames it; on the prompt OK, Cancel and the arrows send their
        commands (`menu::MenuInput`) and a press or drag on the meter sets the amount.
        The hand-built keys stay the prompt's and the box's: a click a key makes through
        the files' `_PCButton_*` and `x…` refs acts unless that key came from the
        keyboard (or a controller standing in for it) and the hand-built handler took it
        that frame (`tiles::menus::key_click_acts`), so a script's MenuTapKey and a file's
        own bindings act (changed after review; the plan first ignored every key click).
      - **Hand-copied numbers**: the tile path reads none of `message_menu.rs`'s layout
        constants or `look.rs`'s `Q_*`, and draws the meter as one tiled picture (no node
        per tick). They stay for the hand-built drawing, the default, and go when the
        maintainer makes tiles the default.
      - **Tests**: `tiles::menus` (the writes on trees written for the tests),
        `tiles::layout` (hotrect alpha), `tiles::vanilla` (the switch), `fnv-ui`
        (`_name_` after a constant), `fnv-game` `ui_loading` (the prompt builds and
        drops). Smoke: `tiles-message-box`, `tiles-quantity` (new). The model's own
        tests (`message_menu`, `menu::model`) unchanged.
    - **T15a built** 2026-10-31 (`claude/t15-menus-a`), awaiting the screenshot
      comparison and the maintainer: as planned above (`tiles/menus.rs`,
      message-menu.md §5a, barter-menu.md §7, menu-xml.md §3.1.1, §4.2, §4.4).
      **Review safeguards** [design]: every model-written path and `MM_ButtonTemplate`
      (its row and text) is required; missing paths warn once and keep the whole
      hand-built menu. Message rows totalling more than the file's 420-unit list cap
      also warn once and keep the whole hand-built box; list scrolling is a follow-up.
      Hotrect alpha inheritance uses `tiles::interface_screen`'s T15 gate, preserving
      default mod HUD drawing. Unconditional hotrect alpha is a candidate for its own
      PR. The observed interface `screen()` units stay gated too; their unconditional
      adoption is a follow-up (the units [observed: hud_menus capture 2026-10-31, three
      window sizes]; the gate [design: default play unchanged]). The prompt can still draw by hand for its opening
      frame before the next tree sync; removing that flicker needs a scheduling or
      loading follow-up [design].
      **Also changes, with the T15 menus off:** `copy … _name_` after `&true;`/`&false;`
      reads `_name_1`/`_name_0` in every menu; `QuantityMenu` builds and drops with the prompt.
      **Font follow-up applied:** font 2 now uses its captured first-line place (16),
      pitch (35) and one-line height (32); the empty title keeps 14 x 38 [observed:
      hud_menus capture 2026-10-31; fonts.md §3.1]. **Open, for the comparison:**
      the highlight box's fill (hotrects draw nothing), the crafting menu's prompt (still
      hand-built); the per-frame layout of an open tile menu (the tree has no cheap
      revision to skip it; to measure with the perf harness). **Where it stands
      (2026-10-31):** implemented. On `d5fc28fa`: root tests no failures, root clippy
      clean, `nvf-play` tests 670 passed, engine clippy clean. Smoke on `6a95d7e7`: 256
      of 261 passed; of the fails, one a tool timeout, one #1181 (stale on main), and
      `tiles-message-box` from #1180 (`--no-messages` closed a form-less Yes/No box; fixed
      since, `1c16ec8a`, `d5fc28fa`, `3baf8595`). Later, 2026-10-31: root tests no
      failures (after `a9c5f151`'s test fix) and root clippy clean; on `bdaad547`
      `nvf-play` tests 672 passed and engine clippy clean; smoke on `947e9c7e` 251 of 253,
      the two fails explained in the run's report. Then, from the three-way pictures and
      the hud_menus capture [observed 2026-10-31]: the box opens with no row chosen, the
      empty title is 14 x 38, and font 2 stands on its own line (the #979 merge); only
      compiled since. The commits after `bdaad547`/
      `a9c5f151` (the tiled prompt's and box's pointer sheet, the late-font retry through
      `draw::whole_pieces` and its test) were only compiled (`cargo check`). The final
      run of 2026-10-31 on the merged head: root tests and clippy clean, `nvf-play` 690
      tests, engine clippy clean, smoke 271 of 273 (the two fails explained in the PR).
      The screenshot comparison was made against upstream main and the original (the
      PR's pictures); observer-checks.md item 64 remains, item 63 is answered by the
      capture.
      - Review fixes (2026-10-31, an independent review): navigation has one owner (a
        highlight a ref moved for a hand-built Up/Down is not the choice); a new box makes
        its rows again (a queued script click on the last box answers nothing); key clicks
        act unless the hand-built handler took that key this frame (MenuTapKey and the
        files' extra bindings act); the tile view only with the menu's file built and the
        prompt open; text sizes per axis; `screen()` gated; `_name_` only after a constant
        with a known number; the drop aid's `ask` holds the prompt; the message scenario
        closes the console.
    - **T15b, part 1 built** 2026-11-01 (`claude/tile-menus-978`, issue #978): the Wait /
      Sleep menu from `sleep_wait_menu.xml` behind `[UI] TileMenus=SleepWaitMenu`, in T15a's
      shape (`tiles/wait_menu.rs`, wait-menu.md §12). The model writes the three texts, the
      Wait button's bare word, the slider's `_current_value` (with `_SetInCode` poked) and
      the background's opacity; clicks on the buttons and the slider's pieces are the
      hand-built menu's commands (`crate::wait::WaitInput`), the slider's value written
      again after them (the prefab's own value counts clicks from 0 in this tree); the
      hand-built keys stay, W presses Wait through the file. Interface `screen()` units
      while it is switched on (`interface_screen`). Tests: `tiles::wait_menu`,
      `tiles::vanilla` (the switch). Smoke: `tiles-sleep-wait`.
    - **Pip-Boy, first slice built** 2026-11-04, checked 2026-11-06 (`pipboy-tiles-1513`, part of issue #1513): STATS'
      S.P.E.C.I.A.L. tab from `stats_menu.xml` behind `[UI] TileMenus=StatsMenu`
      (`tiles/stats_menu.rs`, pipboy.md §5.1.1). The first tile menu not in the window: `paint`
      takes a `Target` (camera, canvas, scales, level). The other STATS tabs stay fully hand-built.
      `stats::prepare` holds the page's model, so a page with no hand-built body keeps its keys,
      test aid and log lines. `StatsTiles::ready` is the one decision for painting, hand-built
      suppression and input. Left out on purpose: the other tabs, ITEMS, DATA, list scrolling,
      compositing. Unit fixtures are synthetic; the `tiles-stats-special*` smoke scenarios check
      structure and hits on the real file, and their logs cannot prove pixels. Not closed: the
      capture list at the end of pipboy.md §5.1.1.
    - **T15b, part 2 built** 2026-11-04 (`tutorial-menu-978`, issue #978): the tutorial menu
      from `tutorial_menu.xml` behind `[UI] TileMenus=TutorialMenu`, in the same shape
      (`tiles/tutorial_menu.rs`, tutorial-menu.md §5). Its text scrolls inside
      `TM_text_container`, a `clipwindow` hotrect. The tile renderer already clipped (`clips`,
      a clipping node, and the pointer's hit test); `clipwindow` is a new source of the same
      clip context in both [hypothesis on its meaning], read only where a menu's model turned
      it on (`fnv_ui::input::mark_clip_windows`), as mods' HUD tiles and the list prefabs carry
      it too; reading it everywhere is left to the other menus' tasks. The layout moves
      nothing: the file moves the text by the scrollbar's value. The model writes the page's
      strings, the count's and buttons' state, the symbol's `filewidth`, the background's
      opacity and the scroll every tiled frame; the sizes and range are the file's from the
      measured text; clicks are the hand-built menu's commands
      (`crate::tutorial_menu::TutorialInput`). Tests: `tiles::tutorial_menu`, `tiles::input`,
      `tiles::layout`, `fnv_ui::input`. Smoke: `tiles-tutorial`, `tutorial-default-drawing`.
    - **T15c, the first of the template lists: the container menu built as a candidate**
      2026-11-04 (`tile-container-menu`, issue #1468), behind `[UI] TileMenus=ContainerMenu`,
      not the default. `tiles/container_menu.rs` (the view),
      `tiles/list.rs` (rows by what they stand for, counts, scroll, highlight, bar pieces, the
      marker's drag; for the barter, inventory, recipe and repair lists next),
      `tiles/item_card.rs` (`item_stats_display.xml`), the marks `_NVFOwnAlpha` and
      `_NVFHotrectFill`. The model, keys and wheel stay the hand-built menu's. Notes and the
      captured numbers: barter-menu.md §8. Smoke: `tiles-container`,
      `tiles-container-quantity`, `container-default-drawing`. **Left**: the comparison with
      the original's shots (parts b to g of the capture retaken), DAM/DPS's fade, then the
      barter comparison (candidate below) and the tag-skill list on the same pieces.

    - **T15, barter built as a candidate** 2026-11-06 (issue #1468), behind
      `[UI] TileMenus=BarterMenu`, hand-built drawing still the default [design:
      `tiles/barter_menu.rs`, barter-menu.md section 9]. A first refactor commit shares
      row identity, measuring, counts, scroll, guards and marks through `tiles/item_menu.rs`
      with the container; its existing tests stay in place. Barter adds prices, caps,
      flow direction, offered squares and Accept/Exit, with all commands and sounds
      still the model's. Its quantity prompt follows automatically, or stays hand-built
      when its file is missing or short. Strings are audited in section 9: settings,
      record text, numbers or our own words. The container numbers reused for barter
      and root arrow direction were checked against a capture of the original at 1280 x 720
      (barter-menu.md section 9). Synthetic tests cover the adapter and a model sale. Smoke
      `tiles-barter`, `tiles-barter-quantity`, `barter-default-drawing` includes 1280x720 shots.

    - **T15, tag-skill candidate built** 2026-11-06 (issue #1519), behind
      `[UI] TileMenus=CharGenMenu` [design: `tiles/tag_menu.rs`, chargen.md §3.1].
      The preceding behavior-preserving refactor shares the trait menu's readiness,
      three lifetimes, frame order, hand-over, list scroll/input, logging and test aids in
      `tiles/selection_menu.rs`; trait snapshot wording remains separate. Tag rows use
      `CGM_SelectItemTemplate` and the existing open sheet, with variable totals and locked
      AddTagSkills tags. SpecialSheet remains hand-built. Title/counter use the project's
      own words, AVIF supplies names/descriptions/icons, and the sheet supplies values.
      A measured text scroll reaches long descriptions without adding a visible bar.
      Uncaptured numbers and behavior follow TraitMenu as hypotheses. All 22 existing
      trait tests compile; added tests cover tag rules, fallback, input, lifetimes and text.
      Smoke: `tiles-tag-menu`, `tiles-tag-pointer-done`, `tiles-tag-default-drawing`,
      `tiles-tag-missing-file`, `tiles-tag-cut-file`, `tiles-tag-stage-handover`,
      `tiles-tag-add-skills`, `tiles-tag-special-fallback`, `tiles-tag-long-description`,
      `tiles-tag-falls-back-open`. Compared with a capture of the original's tag menu at
      1280 x 720 (chargen.md §3.1).
