# UI tile tree: menus built from the game's menu XML

**Owner decision, 2026-10-26** (extender-compat.md §5.6, option A): "lets do a tile tree
i want to make the UI modular for modders in the future to make changes". So the menus
move onto a runtime that does what the original's menu engine does: menu files read into
a tree of **tiles**, each with **traits**, addressed by path (`HUDMainMenu/HitPoints/x`).
Mods then change and add UI the way they do in the original, and the extender's UI
functions act on something real.

The file format is in [menu-xml](../spec-notes/menu-xml.md); the ranked work is in
[ui-tile-tree-tasks](../handoff/ui-tile-tree-tasks.md). Built so far: tasks T1 and T2,
the reader, the tree, trait evaluation and the script-facing API (`crates/fnv-ui`,
`fnv_game::ui_functions`, `fnvtool ui`), reached from scripts through the extender.

## 1. Why, and what it has to carry

- **Scripts.** 16 plugins in the owner's lists call SetUIFloat (1,716 calls), 15
  GetUIFloat, 11 SetUIString, 9 SetUIStringEx; SetUIFloatAlt and SetUIStringAlt add 6
  each [observed: modlist-scan-2026-10-26.md]. These are MCM menus and HUD mods. 307
  functions in `schemas/script/functions.json` are in the UI category; about 70 of them
  work on tiles and traits (§5.2).
- **Files.** The owner's mod folders change menus four ways (menu-xml.md §7):
  - replacing a menu file (Clean Vanilla HUD, Vanilla UI Plus, MCM's `start_menu.xml`);
  - include files a replaced file adds (`includes_StartMenu.xml`);
  - UIO's include lists (JAM, MCM Extender, UI Improvements);
  - InjectUIXML / InjectUIComponent at run time.
- **The game's own menus**, which NVForever draws today in Bevy code with numbers read
  from these same files by hand (status.md, Menus row). Those keep working while they
  move over one by one (§6).

## 2. The model

`fnv_ui::Ui` holds the tree. Plain data, no Bevy, tested headless.

- **Tile**: a kind (`menu`, `rect`, `image`, `text`, `nif`, `radial`, `hotrect`), a
  name, a parent, children in file order, traits, and the templates written in it.
- **Roots**: the loaded menus, plus three pseudo-tiles:
  - `screen()`, the view's size, which the engine sets;
  - `globals()`, `menus\globals.xml`;
  - `strings()`, interface strings.
- **Trait**: a name and a value, a number, a string or a named constant
  (`Value::Num/Str/Sym`), read as the other kinds on demand. A trait may carry an
  expression: the operator list of menu-xml.md §4 (`copy`, `add`, `onlyif`, …), with
  arguments that are literals, other tiles' traits (`src="parent()" trait="width"`) or
  nested lists.
- **Ids**: tiles live in one arena of slots (`TileId`: a slot and its generation).
  Unloading frees a subtree's slots for later tiles and moves each slot's generation on,
  so opening and closing menus leaves the arena its size (#1039). An id kept past its
  tile's unloading never names the slot's new tile: `Ui::tile` answers a dead, empty tile
  (`live` false), `Ui::is_live` false, and writes and unloads through it do nothing, so
  the engine can hold one across frames and check `live`.
- **Skipping unchanged traits**: an expression remembers the traits it read when last
  worked out; while the tree's shape (tiles made, unloaded, renamed, traits added) is
  unchanged and none of those traits changed since, `Ui::update` skips it without
  working it out (#1039). A changed shape works every expression out once more. If the
  shape changed before each of 120 updates in a row (the skip lost), the engine logs one
  warning naming the last call that changed it and where from (`Ui::take_reshape_warning`,
  #1061). Each tree also has its own number in its ids, so an id kept from a tree since
  replaced names no tile of the new one.
- **Paths**: `Menu/Tile/…/trait`, any case, `/` or `\`, `*` and `name:N` (menu-xml.md
  §5): `Ui::find_tile`, `Ui::find_trait`, `Ui::path_of`.

## 3. Loading

### 3.1 Where files come from

Always from the owner's install at run time, never shipped (charter). `fnv_ui::Source`
is the one door:

- the engine passes its VFS: archives and loose `Data` files, and with MO2 the mod
  folders through usvfs or NVForever's own MO2 support (PR #435);
- `fnvtool ui` passes the VFS with `--overlay` mod folders;
- tests pass `MemSource`, files written for the test.

Replaced menu files therefore work without any code: the VFS already prefers the loose
file. `Source::read_archived` reads the copy a loose file hides (`fnv_vfs::Vfs::read_archived`,
the highest archive's), whose names tell a loose replacement's own tiles from the ones it
kept (`Ui::archived_name_paths`, §6.1, #975). Game strings (`&-sName;`) come from `Source::game_string`. The engine answers from
`fnv_ui::GameStrings`: the load order's GMSTs (last plugin wins), then the built-in table
`fnv_ui::strings::BUILT_IN`, since the masters hold only one of the 145 names
(`sMorphine`). The table holds NVForever's own English labels, written from each
setting's name and where the menu files use it, never from the original's text (the
maintainer's decision 2026-10-30, LEGAL.md; menu-xml.md §2.4). A name with no text reads
empty, as an unknown name does in the original, and names ignore case (task T10).

### 3.2 Which menu file

`Ui::load_menu(src, path)` reads a file and builds its `<menu>`. The engine indexes
`menus\**\*.xml` once at start by the `<menu name>` each holds (49 vanilla files), so a
mod's renamed or added menu file is found the same way.

The index also maps the menu's own `<class>` constant to its file. MenuMode codes
reach files through the documented menu class IDs [documented: Hardcoded UI
Shortcuts, revision 39903, "Menu class ID" rows; GetActiveMenuMode].

- `menus\prefabs\` is never indexed: its files are only included.
- Loading a menu that is already loaded replaces it.

### 3.3 Includes, templates, injection

- **Includes** are read where they stand, each file on its own (menu-xml.md §2.1).
  Template bodies get their includes spliced in at load, so `add_from_template` needs
  no files later.
- **Injection**:
  - `Ui::inject_text` (InjectUIComponent) and `Ui::inject_file` (InjectUIXML) read XML
    into a tile as if the tile's file had it at its end;
  - `Ui::add_from_template` (AddTileFromTemplate) makes a tile from a template;
  - `Ui::unload_tile` (UnloadUIComponent) removes one.

  Injected tiles work at once: their traits are worked out on the next update.
- **UIO lists** (task T6): at a menu's load, the entries of `uio\public\*.txt` and
  `uio\supported.txt` whose target is that menu (or a prefab file it includes) and whose
  condition holds are included at the target tile. The same entries are applied when a
  prefab named `@Prefabs\…` is included.
  [design] `Source::list(dir)` discovers the lists; `Ui::configure_uio(src, plugin)`
  parses and evaluates them before menu loading. `Ui::uio` keeps per-entry outcomes,
  warnings and application counts. Prefab patches happen before template expansion;
  menu patches happen after ordinary includes. Insertion order, condition assumptions
  and short data-file excerpts are in menu-xml.md §7 item 3. Explicit loose additions
  retain File origins and are eligible for drawing even without archived HUD names.
- **Lifetime**, as the original [documented: InjectUIXML notes]:
  - `HUDMainMenu`, `InventoryMenu`, `MapMenu`, `RepairMenu`, `StatsMenu` and
    `ItemModMenu` are loaded once and stay;
  - the other menus are built when they open and dropped when they close, so what a
    mod injected into them goes with them, and mods inject again on each open (they
    expect to).

  NVForever builds `StartMenu` when the pause or main menu opens (MCM injects there).

### 3.4 Mending

The files are not strict XML (menu-xml.md §1.1). The reader never fails and lists every
mend in `Ui::warnings`; the engine logs them once per file. A file that does not exist
or holds no `<menu>` is an error for that menu only.

## 4. Evaluation and updates

### 4.1 One frame

1. **The engine writes code-side traits**: what the original's code puts into a menu.
   - `screen()`'s `width`, `height`, `cropx`, `cropy`, `resolutionconverter`.
   - The open menu's `io()` traits (`user0`…, `_IsContainerListSelected`, the HUD's
     `_SneakLevel`, …).
   - List rows made from templates (`add_from_template`), with their `user` traits.
   - Pointer state: `mouseover` and `clicked` on targets, the keys' clicks and the menu
     events (task T8, `tiles::input::feed` → `fnv_game::ui_input::step`; menu-xml.md
     §3.2), only for the menus drawn from tiles that take input, so the default game
     does no tree work for it.

   The HUD's writes (`fnv_game::ui_hud::write`) are made only when something reads
   them: the tile-drawn HUD (`[UI] TileMenus=HUDMainMenu`), mod tiles in the HUD
   (`Ui::has_mod_added_tiles`: a count per menu kept where tiles are made, unloaded or
   change origin, so neither this check nor the drawing walks the HUD a frame, #938), or a
   script that has called a UI function on the HUD (a path whose menu is `HUDMainMenu`:
   a trait read or set there, a tile injected, made from a template or unloaded there;
   calls on other menus and MCM's shorthands do not count, #937). Otherwise the frame
   keeps them (`ui_hud::defer`), and the first such call makes them before it reads
   (`ui_hud::flush`); from then on they are written every frame [design: maintainer's
   review of #902, so the default game does no tree work a frame]. The measured text
   widths are kept with them as their measure (the engine's fonts, owned, a
   `ui_hud::Measure`): they are measured from the strings the write puts in the tree, so
   the flush makes the strings and then the widths, in a frame's order (#936). The
   corners are written only with the tile-drawn HUD, when nothing is kept back.
2. **Scripts run**, and their Set/Mod/Inject calls land in the tree at once (the
   functions read what they set in the same frame).
3. **`Ui::update()`** works out every trait whose sources changed, until nothing
   changes. When no value, tile or file trait changed since it last finished (writes of
   the same value do not count), it returns at once.
4. **The engine draws** the visible menus from the traits' values (§6).

### 4.2 Change tracking

Each trait records the tree's change count when its value last changed and when its
expression was last worked out. A trait is worked out again only when something it read
changed after that.

- A value a script set therefore holds until a source of its expression changes
  [hypothesis; menu-xml.md §8 item 3].
- Loops of traits are stopped after 32 passes and reported.
- Today every expression is read on each update to find its sources' change counts:
  - all 49 vanilla menus together are 19,541 expressions, loaded together in 0.4 s;
  - the HUD alone has 64 (182 with Clean Vanilla HUD).

  A dependency index (who reads whom) is task T13, done only if the update shows in a
  profile.

### 4.3 Animation

SetUIFloatGradual (JIP) moves a trait over time in four modes [documented]. It becomes
a list of running gradients in the tree, stepped by the engine's real-time clock before
`update()` (task T9).

## 5. The modding surface

### 5.1 For the extender coordinator

`fnv_game::Game` carries:

- `ui_tree: fnv_ui::Ui`, the tree;
- `ui_files: Option<Box<dyn Source>>`, the engine's files (`None` headless);
- `ui_taps: Vec<u32>`, MenuTapKey's keys, for the engine to take.

`fnv_game::ui_functions::call(g, name, args) -> Option<Result<XVal, String>>` answers
these functions by name:

| Functions | Does |
|---|---|
| `GetUIFloat`, `GetUIFloatAlt` | the trait as a number, -999 if not found [documented] |
| `GetUIString` | the trait as a string, empty if not found |
| `SetUIFloat`, `SetUIFloatAlt` | sets a number; 1 when the trait is there |
| `SetUIString`, `SetUIStringAlt`, `SetUIStringEx` | sets a string; 1 when the trait is there |
| `ModUIFloat` | adds to a number; 1 when the trait is there |
| `IsComponentLoaded` | a menu, tile or trait is there |
| `InjectUIXML`, `InjectUIComponent` | XML into the named tile; 1 on success |
| `UnloadUIComponent` | removes a tile |
| `AddTileFromTemplate` | `"path\|template\|name"` |
| `MenuTapKey` | queues a key in `ui_taps` (a menu taking tile input reads them as presses) |
| `GetActiveUIComponentName`, `…FullName`, `…ID` | the highlighted tile's name, path, `<id>`; "" / -1 with none |
| `ClickMenuButton`, `ClickMenuTile` | click a tile (path or `Menu#id`) as a press does; 1 when clicked |

Wiring it is one line in `crate::nvse::call` on claude/ext-compiler (task T2):

```rust
if let Some(r) = crate::ui_functions::call(g, name, args) { return Some(r); }
```

The VM is expected to format the format-string arguments (SetUIStringEx,
InjectUIComponent's `%g`/`%q`/`%z`) before the call.

The deprecated forms (SetUIFloat, GetUIFloat, SetUIString, SetUIStringEx) act as the
Alt forms for now. The wiki says they keep a bug "for templated children tiles" that MCM
depends on, but the bug's page is not in the export (observer check, menu-xml.md §8
item 5).

### 5.2 What else the tree enables (task T9)

From the UI category of functions.json, the ones that need tiles:

- JIP: `SetUIFloatGradual` and its Get… readers, `SetUIFloatMultiple`,
  `GetUIFloatInherited`, `AttachUIXML`, `AttachUIComponent`, `AddTraitToTile`,
  `GetUIValueType`, `GetUIElementType`, `SortUIListBox`, `GetStringUIDimensions`,
  `SetUIStringReveal`, `SetUITypewriter`, `SetUITexOffset`, `SetUIAlphaMap`,
  `SetTileScript`, `RemoveTileScript`;
- the CacheUI family;
- the HudBar family (a HUD bar library made of tiles);
- `GetActiveUIComponentName`, `GetActiveUIComponentFullName`,
  `GetActiveUIComponentID`, `ClickMenuButton`, `ClickMenuTile`;
- `DumpTileInfo`, `DumpTileInfoAll`, `DumpTileInfoToArray`;
- the cursor and screen traits;
- the menu events `SetOnMenuClickEventHandler` and `SetOnMouseoverChangeEventHandler`.

Built (T9) and left open, function by function: menu-xml.md §9. The gradual changes'
state is `Game::ui_gradients`, stepped by the engine on real time; CacheUI's caches, the
HUD bars and the attached components are `Game::ui_extra`.

### 5.3 For modders writing for NVForever

The same as for the original: the menu files, includes, UIO lists, the functions.

- NVForever adds a mod-facing log: every mend and every include not found, with the
  file and line (`nvforever.log`, and `fnvtool ui menus` for a whole setup).
- `fnvtool ui tree --menu … --at …` shows a menu's tiles and worked-out values, which
  the original does not offer outside DumpTileInfo.

## 6. Drawing, and moving the menus over

### 6.1 The tile renderer (task T5)

`nvf-play/src/tiles/` turns a menu's live tiles into Bevy UI nodes each frame, by
traits:

- `x`/`y` relative to the parent's place, `width`/`height`;
- `depth` for stacking, `alpha`, `visible`;
- `clips`, `zoom`, `tile` for pictures;
- `image`: `filename` with the interface atlas (`texatlas`, the `.tai` index the HUD
  code already reads);
- `text`: `string`, `font` (fnv-font), `justify`, `wrapwidth`;
- colour: `systemcolor`, `red`/`green`/`blue`, `brightness`;
- `nif`: a model drawn into the menu; since the T12 leftovers a model of flat textured
  squares (the breath meter) is drawn as pictures (hud.md §14.12), other models later.

The layout is a pure function from the tree to rectangles (tested headless). What each
drawing trait means exactly is checked against the original menu by menu (screenshots
at the same view size). That is the work of each migration task, not guessed up front.

### 6.2 One menu at a time, nothing broken

1. **The tree loads beside the existing menus** (tasks T3, T4). The six always-loaded
   menus are built at start. While NVForever still draws a menu its own way, the engine
   writes that menu's code-side traits into the tree, so a mod reading
   `HUDMainMenu/...` gets live values, and a mod's Set… lands somewhere harmless.
2. **Injected and added tiles are drawn over the existing menus** (task T5). The tile
   renderer draws only the subtrees mods injected (and replaced files' tiles that are
   not vanilla) on top of NVForever's own HUD. That alone makes JIP-style HUD mods
   (JAM's meters, a hardcore needs HUD) show up.
3. **A menu moves to the tile renderer** when it matches the original as well as the
   current code does: compared against the same screenshots as the current spec notes.
   - Each menu has a switch, `NVForever.ini` `[UI] TileMenus=`, a list of menu names.
     The old drawing stays the default until the owner agrees a menu has moved.
     `HUDMainMenu` (T12), `MessageMenu` and `QuantityMenu` (T15a), `SleepWaitMenu` (T15b) and
     `ContainerMenu` (T15c, a candidate: barter-menu.md §8; its prompt follows from
     `QuantityMenu`'s file with no switch of its own, while the container is so drawn) are
     acted on, and `BarterMenu` (candidate: barter-menu.md section 9, with the same
     prompt rule) [design: `tiles/barter_menu.rs`]. Also `TraitMenu` (chargen.md §4.1), `CharGenMenu` (tag skills only, chargen.md §3.1), `DialogMenu` (its contract and fallback:
     spec-notes/dialogue-menu.md §6), and `StatsMenu` for the Pip-Boy's S.P.E.C.I.A.L. tab
     alone, painted in the Pip-Boy's image (issue #1513, pipboy.md §5.1.1).
   - The game rules stay where they are (`menu::model`, `pipboy::items`,
     `wait::model`, …). Only the view changes: the model writes `io()` traits and
     template rows, input comes back as clicked tiles.
4. **The order** (task list): HUD and StartMenu/MCM first, because that is what the
   scripts touch; then the message box, quantity, sleep/wait and tutorial menus (small,
   whole files); then container, barter and inventory (template lists); then the
   Pip-Boy (the tile renderer into the screen texture it already uses); then dialogue,
   character creation and the casino games.
5. When a menu moves, its hand-copied numbers in the old module go with it. The spec
   note keeps the measurements; the code reads the file.

## 7. What stays NVForever's own

- The **settings pages** (Display, Gameplay, Sound, Controls), with their INI keys and
  live switches. They are NVForever's design (settings.rs), not the original's.
- The main menu's loading-screen background; the character creator; the poker table;
  the debug and test aids.
- These may later be written as NVForever's own menu XML, so modders can restyle them
  too. That is NVForever's own text, so it can ship. Until then they stay Bevy code.
- **MCM** lives in the original's `StartMenu`, which is also where NVForever's pause and
  settings pages live. How the two meet is the owner's call (owner-review.md):
  - recommended: build `StartMenu` from the game's files whenever the pause or main menu
    is open (so MCM's scripts find their tiles), and draw MCM's page from tiles when
    its entry is chosen. NVForever's column and settings stay as they are, with a
    "Mods" entry added to the column only when a mod added one to `StartMenu`.

## 8. Clean room

- The game's menu files are read at run time from the owner's install and never
  committed; tests use files written for them.
- `fnvtool ui` prints counts, warnings and values, not file text.
- UIO and MCM are described from their data files only (their text lists and XML),
  never their plugin code; UIO's behaviour is implemented as engine code, as for the
  other extenders (extender-compat.md).
- Where the original's rule is unknown (constants' numbers, end-tag mending, update
  order), the choice is tagged and listed as an observer check (menu-xml.md §8).

## 9. Decided here (design)

- **A forgiving reader of our own** instead of `quick-xml`, which dependencies.md
  planned: the files need undeclared entities kept, fragments, mismatched end tags and
  broken tags mended in particular ways, which is most of a reader anyway (~400 lines).
- **Constants kept by name** (`Value::Sym`) instead of guessed numbers, so
  comparisons work whatever the original's numbers are; the documented menu class IDs,
  `&true;`/`&false;` and the platform flags get a number (menu-xml.md §2.3).
- **No dependency graph yet**: change counts give the right answers, and the cost is
  small for the menus open at one time.
- **The tree is not saved.** Menus are built again after a load, as the original
  rebuilds them; mods inject again on load.
