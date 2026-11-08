# Menu XML: the files the original builds its menus from

How the game's menu files are written and how NVForever reads them into the tile tree
(`crates/fnv-ui`, [design](../design/ui-tile-tree.md)). Written from the files in the
owner's install (read with `fnvtool ui`, never copied into the repo), from mods' files in
the owner's Mod Organizer folders, and from the GECK wiki export's UI function pages.
[design] Only short UIO data-list excerpts are quoted below; no menu XML or complete
mod file is shipped.

Tools: `fnvtool ui menus --data <Data> [--overlay <mod folder>]…` builds every menu and
lists counts and every spot the reader mended (`fnv.ui.menus/1`); `fnvtool ui tree
--menu <file> [--at <path>]` prints one menu's tiles and worked-out trait values
(`fnv.ui.tree/1`). Smoke scenario `ui-menus`.

## 1. The files

- The menu files are under `menus\` in the archives [observed]: 126 files, of them
  `menus\prefabs\` (pieces other files include), `menus\main\` (HUD, inventory, map,
  stats, quick keys, safe zone), `menus\options\` (start, main, pause, load, save,
  credits), `menus\chargen\`, `menus\dialog\`, `menus\generic\` and the rest at the top.
- **49 files hold a `<menu>`** [observed: fnvtool ui menus, 2026-10-26], one each, its
  `name` the menu's name (`HUDMainMenu`, `BarterMenu`, `StartMenu`, …). The engine finds
  a menu's file by that name, not by a table. Files under `menus\` without a `<menu>`
  are pieces (the caravan's four sub-menus, `globals.xml`).
- `menus\master_menu_file.txt` is every menu file joined into one [observed]; nothing
  needs it [hypothesis].
- `menus\globals.xml` is one `<rect name="Strings">` of shared constants (line
  thickness, brightnesses, the Pip-Boy canvas size 1024 × 768, `_Has360Controller`),
  read by other files as `globals()` [observed].

### 1.1 Not strict XML

43 of the 126 files fail a strict XML parser [observed]. What the reader has to take:

1. **Entities no file declares**: `&true;`, `&center;`, `&xbox;`, … (§2.3) and game
   setting strings `&-sName;` (§2.4).
2. **Fragments**: a prefab is several top-level elements (the content of the tile that
   includes it), not one root.
3. **Mismatched end tags**: three vanilla files close an element with another's end tag
   (`quest_added.xml` closes a `<hotrect>` with `</rect>`; `caravan_builddeck_menu.xml`
   and `main_menu.xml` have one end tag too many or too few). Mods do it too: MCM's
   `MCM.xml` opens `<_KeyInut>` and closes `</_KeyInput>`, its `ModListRect.xml` never
   closes one `<onlyif>`.
4. **A start tag cut short**: `<systemcolor</systemcolor>` in `hud_main_menu.xml` and
   `item_stats_display.xml`.
5. **An attribute missing its opening quote**: `name=item_listing_parent"` in
   `item_listing.xml`.

The reader (`fnv_ui::xml`) never fails. Its rules [hypothesis for each; the original's
own rules are not known]:

- an end tag closes the innermost open element of the name it gives, with the ones
  opened inside it; when none of that name is open it closes the innermost open element
  whatever its name; with nothing open it is dropped. Both MCM spots need this pair of
  rules to come out as MCM evidently works in the original (one needs each rule);
- what is still open at the end of a file is closed there (each included file is read
  on its own, so this stays inside the include);
- a start tag cut short by `<` ends there, open (its own end tag follows);
- an unquoted attribute value runs to a space or `>`, stray quotes dropped;
- `<!-- -->` comments, `<?…?>` and `<!…>` are skipped; `&amp;` `&lt;` `&gt;` `&quot;`
  `&apos;` `&#N;` are decoded; any other `&name;` is kept by name;
- bytes are UTF-8 when they are, else Latin-1 (every vanilla file is ASCII [observed]).

Every mend is listed (`Ui::warnings`, fnvtool's `warnings`). Vanilla, 2026-10-26: the
two cut-short tags, the three mismatched files, nothing else.

## 2. Elements

An element is one of four things, by its tag (any case):

| Tag | What it is |
|---|---|
| `menu`, `rect`, `image`, `text`, `nif`, `radial`, `hotrect` (and `window`) | a **tile**: a node of the tree, named by its `name` attribute |
| `include src="file.xml"` | the file's content put here, as if written in place |
| `template name="…"` | content kept to make tiles from later, not a tile now |
| anything else | a **trait** of the tile it is in |

[observed: the tags that carry a `name` attribute in the vanilla files are exactly the
seven tile kinds and `template`; `window` is in no vanilla file and is kept as a tile in
case a mod uses it, hypothesis]

### 2.1 Includes

- `src` names a file under `menus\prefabs\`, with a folder when it is in one
  (`MCM\MCM.xml`) [observed: every vanilla include names a bare file that is in
  `menus\prefabs\`; MCM's name files in its own folder there].
- Includes nest (a prefab includes others). NVForever stops at 16 deep [design].
- A missing file leaves nothing and a warning.

### 2.2 Templates

`<template name="…">` holds a tile (or several) to make later: the game's code fills
lists from them (the HUD's quest checkboxes, list items), and JIP's
`AddTileFromTemplate "path|template|name"` makes one under a tile [documented:
AddTileFromTemplate]. Templates sit on the tile they are written in; one is looked for
on the tile named and then on the tiles above it [hypothesis]. Their includes are
spliced in when the file is read, so making a tile later needs no file.

### 2.3 Constants

Entities without a minus are constants: 94 different names in the vanilla files
[observed], most used:

- `&true;` `&false;`;
- justification `&left;` `&center;` `&right;`;
- `&scale;` (a picture's sizing);
- platform flags `&xbox;` `&console;` (guarding console-only values);
- the system colours `&hudmain;` `&hudalt;` `&pipboy;` `&terminal;` `&mainmenu;`
  `&nosystemcolor;` `&uselocalcolor;`;
- tile ids `&noglow_branch;` `&glow_branch;` `&generic;`;
- the controller buttons `&xbuttona;` … `&xbuttonrt;`;
- each menu's class (`&HUDMainMenu;`, `&BarterMenu;`, … in `<class>`);
- `&does_not_stack;` `&no_click_past;` `&nosound;`.

NVForever keeps each as a named constant (`Value::Sym`): two compare equal when
their names do, so `<eq> &center; </eq>` works without a number. The 40 menu class
constants also read as numbers [documented: GECK wiki, Hardcoded UI Shortcuts,
revision 39903 (2026-04-02; the same rows as revision 30746), only the "Menu class ID"
rows]. These are MenuMode's
codes [documented: GetActiveMenuMode]. Other numbers stay as before [hypothesis]:

- `&true;` 1 and `&false;` 0: the meter prefab picks its picture as `_filename_` + its
  `_SolidMeter`, with `_filename_1` the solid one and a comment saying a true
  `_SolidMeter` draws solid;
- `&xbox;` `&console;` `&xenon;` 0, the PC build;
- `&hudmain;` 1, `&hudalt;` 2 [observed: private job hud_jip file_0032;
  hud_hardcore2 hc2-hc5; hud.md §14.8];
- every other constant reads 0 as a number [hypothesis].

### 2.4 Game setting strings

`&-sName;` is the text of the game setting `sName` [hypothesis: the names are game
settings' names, `s` for a string, as the wait menu's `sHowManyWait` and `sCancel`
(wait-menu.md)]. 145 different spellings are in the vanilla files, 143 without case
(`sOK`/`sOk`, `sQuit`/`squit`) [observed: the `&-…;` entities of the 121 `menus\**\*.xml`
in `Fallout - Misc.bsa`, 2026-10-29; no other vanilla archive holds menu files].

Only one has a record in the vanilla load order: `sMorphine`, a GMST of FalloutNV.esm.
The other 144 have none in FalloutNV.esm or the nine official add-on masters (DeadMoney,
HonestHearts, OldWorldBlues, LonesomeRoad, GunRunnersArsenal, Caravan, Classic,
Mercenary and Tribal packs) [observed: `fnvtool esm inventory`, GMST Editor IDs (649 in
all) against the names, 2026-10-29]. One spelling, `sExplosive`, stands only inside a
comment of `hud_main_menu.xml`, so the reader never asks for it: the reader reads 144
spellings, 142 without case [observed: the archive's menu files, 2026-10-30].
Four more uses lack the closing `;` (`&-sChallengeToggle` in `map_menu.xml`, `&-sStats`
and one `&-sStatsGeneral` in `stats_menu.xml`, `&-sNetTotalText` in
`prefabs\caravan_game_menu.xml`) [observed: same files]; the reader keeps them as plain
text, not names [design: `fnv_ui::xml`; what the original does with them is not known].

Which names the original fills in [observed: private capture `t10_gmst`,
`GetStringGameSetting`, 2026-10-29, the 10 official masters and fo_proof.esp loaded, 146
reads, no errors]: of the 142 names without a record (without case, `sExplosive` among
them), 140 read a text and two, `sModffects` and `sReputationSwap`, read empty. A name no game defines
(`sNvfT10NoSuchSetting`) read empty, and both spellings of `sOK`/`sOk` and of
`sQuit`/`squit` read alike, so names are compared without case. The capture is kept
privately for these points only: what the names read is not recorded anywhere in the
repository and is not used (the rule below). That a plugin's GMST overrides the built-in
text is [hypothesis: as `iLockLevelMax…`, read with `GetGameSetting` though no master
defines them, script-function-audit.md].

**The rule for the text** (the maintainer's decision, 2026-10-30, LEGAL.md "built-in
labels the data files lack are written by the project, not transcribed from the
original") [design]:

- The load order's string game settings win.
- Where the plugins define none, the text is an English label NVForever writes itself
  from two things only: the setting's name and where the menu files use it (the tile's
  kind, its menu, its neighbours, its place: a button row, a title, a prompt).
  `sCloseButton`, a button at the foot of the tutorial box, reads "Close"; `sNext` reads
  "Next".
- The original's text is not looked up by any route: not from screenshots, not from
  `GetGameSetting` or `GetStringGameSetting` readings, not from memory of the game, not
  from any list of game-setting values. An observer dump may say only which names the
  original fills in, not what they say.
- Wording is not changed on purpose either: a short generic label ("Close", "Next",
  "Back", "OK") stays what the use calls for even if it is the original's word. A longer
  label (a prompt, a sentence) is written fresh from what the menu needs. The menu files'
  comments are not copied into labels.

How NVForever answers `Source::game_string` (`fnv_ui::strings::GameStrings`, task T10)
[design]:

1. the load order's string settings, the last plugin's record winning (the form store's
   one value per setting, or a run-time change made before the files are read);
2. `fnv_ui::strings::BUILT_IN`, NVForever's own labels under the rule above, each entry
   with the name, the label and where the label came from (the setting's name and the
   file and place that use it). The two names the original leaves empty
   (`sModffects`, `sReputationSwap`) have no entry and read empty; `sMorphine` comes
   from its record. The engine's own menus that stand in for menu files (the tutorial
   menu, the lockpick menu and its help) read their words from the same table, by the
   names their files use;
3. a name in neither reads empty, as in the original. The reader puts the text in when
   the file is read (a later `SetStringSetting` does not change a tile already built)
   [design].

The tree records each name it read and whether it had text (`Ui::game_strings_read`); the
engine logs `ui: game strings: N read, M without text` at start for the menus it keeps
loaded (with the vanilla files, `sModffects` of `item_stats_display.xml` is the one name
without text there).

The names with no record in the vanilla masters, by spelling:

`sAccept`, `sAcceptText`, `sAddText`, `sAllCardsText`, `sAntePotText`, `sAutoAttempt`,
`sAutoMatchText`, `sBack`, `sCWheelTitle`, `sCancel`, `sCaravanBiggestAnteText`,
`sCaravanLossesText`, `sCaravanWinLossText`, `sCaravanWinningsText`, `sCardsInDeckText`,
`sCauterize`, `sCloseButton`, `sConfirmWarning`, `sContinue`, `sCrew`,
`sCurrentAnteText`, `sCurrentObjective`, `sData`, `sDevice`, `sDiscardCardText`,
`sDiscardTrackText`, `sDone`, `sEnterName`, `sExit`, `sExplosive`, `sForfeitGameText`,
`sHackingLockout3`, `sHackingLockout4`, `sHitPointsShort`, `sHowMany`, `sIngredients`,
`sInventoryCaps`, `sInventoryCondition`, `sInventoryDamage`,
`sInventoryDamagePerSecond`, `sInventoryDamageResistance`, `sInventoryDamageThreshold`,
`sInventoryDrop`, `sInventoryEffects`, `sInventoryEquip`, `sInventoryHotKey`,
`sInventoryItems`, `sInventoryMod`, `sInventoryRepair`, `sInventoryStrReq`,
`sInventoryValue`, `sInventoryWeight`, `sInventoryWeightUpper`, `sLevelProgress`,
`sLevelUp`, `sLoad`, `sLoadSavedGame`, `sLockLevelText`, `sLockpickSkillText`,
`sMadeAt`, `sMakeActiveQuest`, `sMedicineSkill`, `sModffects`, `sNavigate`,
`sNavigateText`, `sNew`, `sNext`, `sNextChallenge`, `sNoItemsToRepair`, `sOK`, `sOk`,
`sPCMenuHintA`, `sPCMenuHintE`, `sPCMenuHintF`, `sPCMenuHintQ`, `sPCMenuHintR`,
`sPCMenuHintRMB`, `sPCMenuHintS`, `sPCMenuHintW`, `sPCMenuHintX`, `sPicksRemainingText`,
`sPlaceMarker`, `sPlayAudio`, `sPlayCaravanText`, `sPressStart`, `sPrevious`, `sQuit`,
`sRaiseText`, `sRecipes`, `sRedeemCode`, `sRemoveText`, `sRepairAllItems`,
`sRepairItem`, `sRepairServicesTitle`, `sRepairSkill`, `sReputationSwap`, `sReset`,
`sReturn`, `sSave`, `sSaveGame`, `sSelect`, `sSelectCardText`, `sSettings`,
`sShowActiveNotes`, `sShowAllNotes`, `sShowLocation`, `sSkillRequirement`, `sSpace`,
`sStatsAP`, `sStatsCNDAbbrev`, `sStatsCrippled`, `sStatsEFFAbbrev`, `sStatsFODAbbrev`,
`sStatsGeneral`, `sStatsH20Abbrev`, `sStatsHP`, `sStatsLMBAbbrev`, `sStatsLVLAbbrev`,
`sStatsNoEffects`, `sStatsPerks`, `sStatsRADAbbrev`, `sStatsRadResist`,
`sStatsSLPAbbrev`, `sStatsSkills`, `sStatsSpecial`, `sStatsStatus`, `sStatsXP`,
`sStimpak`, `sStopAudio`, `sSwitchLimbs`, `sTakeAll`, `sTotalCardsText`,
`sTotalFundsText`, `sTravel`, `sTune`, `sVDSGManual`, `sVDSGPlate`, `sVatsAiming`,
`sVatsBodyPart`, `sVatsSelect`, `sVatsTarget`, `sWinLossText`, `sXPProgressMarker`,
`squit`.

## 3. Traits

- A trait holds a number or a string; in the original it is both at once [documented:
  AddTraitToTile notes]. NVForever holds one and reads it as the other on demand:
  - text reads as a number when it is one, else 0;
  - a whole number reads as text without a point [hypothesis].
- A trait is written either as a plain value (`<width> 400 </width>`, `<visible>
  &false; </visible>`, `<string> &-sTitle; </string>`) or as operators (§4).
- Names are compared without case. Traits beginning `_` are the files' own (`_Value`,
  `_SolidMeter`, `_Selected`); `user0` … `user23` are the slots the game's code fills
  [observed: the menus' comments, e.g. barter's `user0` "1 if on PS3"].
- A trait written twice in one tile (often: a prefab sets a default, the including file
  sets it again after) keeps the later one [hypothesis].

### 3.1 The traits the engine draws by

[observed: the most used names; what each does is for the drawing task,
docs/handoff/ui-tile-tree-tasks.md]: `x` `y` `width` `height` `depth` `alpha` `visible`
`locus` `id` `target` `clips` `string` `font` `justify` `wrapwidth` `filename`
`texatlas` `zoom` `tile` `cropx` `cropy` `brightness` `red` `green` `blue`
`systemcolor` `clicksound` `mouseoversound`, and the menu-level `class`
`stackingtype` `menufade` `explorefade`. Input: `xbuttona` …, `xdefault`, `xup` `xdown`
`xleft` `xright`, `_PCButton_*`, `wheelable`, `draggable`.

### 3.1.1 Layout interpretation (T5 part A)

- [design] `nvf-play/src/tiles/layout.rs` produces screen-unit rectangles without
  loading assets or drawing. A subtree root retains the contributions of all its
  ancestors. Hidden items remain in the result with `visible = false`.
- [hypothesis] `x` and `y` add to the parent's top-left; missing values are zero.
  `width` and `height` are the box's size, with zero for missing non-text dimensions.
  No conversion between window pixels and the original's interface units is inferred.
- [hypothesis] `depth` adds down the tree, with zero when absent. Parents precede
  children; siblings sort by increasing depth, preserving file order for ties. Neither
  this note's earlier sections nor hud.md establish cumulative depth or traversal order.
- [hypothesis, T15a] A `hotrect`'s own `alpha` is not passed to its children: every
  vanilla text button is a hotrect of alpha 0 (`text_box.xml`) whose text sets its own
  alpha (128, or 255 while it is a target), the message box's list and its rows
  (`list_box.xml`, `list_box_template.xml`) are alpha-0 hotrects over texts the original
  shows (the message box's OK, the prompt's Ok and Cancel on the owner's screenshots,
  message-menu.md §3, barter-menu.md §7), and the list's highlight box, alpha 40, draws
  its lines at full strength. Until T15a it multiplied through as the other tiles' does,
  which hid those texts. [design] The exception now applies only while a T15 menu is
  switched on (`tiles::interface_screen`, the same gate as interface `screen()` units).
  With both T15 menus off, including the tile HUD alone, hotrect alpha still multiplies
  through mod HUD tiles and other layouts as before T15a. Applying the exception
  unconditionally is a candidate for its own PR.
- [hypothesis] `alpha` is 0–255, defaults to 255, and multiplies through ancestors as
  fractions of 255. [design] Clamp each local alpha to that range and retain fractional
  results without rounding.
- [hypothesis] A false `visible` hides the entire subtree; missing means true. A true
  child cannot override a hidden parent.
- [hypothesis] True `clips` crops children to this tile's rectangle. Nested ancestor
  clips intersect; a disjoint intersection is empty. The tile's own box is unchanged.
- [observed] True `clipwindow` crops a tile's descendants to its rectangle in the
  drawing: the tutorial menu's scrolled text stops part way through a line at the column's
  top and bottom edges (private capture hud_tutorial_more, 1280 x 720, shots
  tutorial__scroll_bottom and tutorial__scroll_arrow). [hypothesis] The same for the pointer
  (`fnv_ui::input::clips_children`): `list_box.xml`'s highlight box takes its `clips` from
  its parent's `clipwindow` [documented: the file]; nothing captured says so (observer
  check 82). It stays apart from scrolling: nothing here moves a tile; a file moves its
  text by the `y` it writes. [documented: the vanilla files] It is true in the book,
  lockpick, map, stats, credits, load, save, trait and tutorial menus and in the prefabs
  `item_listing`, `list_box` and `load_and_save_menus`, so in every menu that includes
  them, the message box and the start menu among them; not in the HUD's files.
  [design] Mods' tiles carry it too (MCM's `ModList.xml`), and the default game draws
  mods' HUD tiles, so it is read only where a menu's model turned it on
  (`fnv_ui::input::mark_clip_windows`, the private trait `_NVFClipWindow`): today the
  tutorial menu alone, while drawn from tiles. Off, every menu lays out and takes the
  pointer as before. Reading it everywhere is a follow-up, with the other menus' checks.
- [design] `menu` and `hotrect` contribute layout to children but emit no drawing item;
  hotrect input is T8 (§3.2). `rect`, `image`, `text`, `nif`, `radial` and `window` retain
  distinct kinds; producing a layout item does not implement model or radial drawing.
- [design] Image `filename` is passed through (empty if absent); `texatlas` is retained
  as a raw value for later atlas resolution using interface.rs's existing `.tai` reader.
  [hypothesis] Missing `zoom` means 100 percent. [design] Explicit `zoom`, `tile`,
  `cropx` and `cropy` are passed as numbers without changing the box; absent tile/crop
  inputs remain absent. Their effects on texture sampling await the renderer.
- [design] Text `string` (empty if absent), numeric `font` (zero if absent) and optional
  `wrapwidth` go to a caller's measuring callback. Only absent width/height are replaced
  by the measured dimensions; explicit zero dimensions stay zero.
- [hypothesis] Named `justify` constants `&left;`, `&center;`, `&right;` anchor the text
  box's left, centre or right edge at x, leaving y at the top. [design] Numeric `justify`
  is retained for the renderer; its unknown mapping is not invented. Missing, unknown
  or numeric justification uses a left anchor. Named constants also retain their
  alignment separately from their numeric view (§2.3).
- [hypothesis] Local `red`, `green`, `blue` and `brightness` default to 255.
  [design] RGB clamps to 0–255; brightness passes unchanged. Colour does not multiply
  through ancestors. Numeric `systemcolor` is retained for the renderer to map; unknown
  named constants retain their names as well, and absence stays absent.
- [design] `locus`, `id`, `target`, input traits, menu fades and stacking traits have
  no extra layout effect yet. No undocumented meaning is assigned to them.

### 3.1.2 Drawing interpretation (T5 part C)

`nvf-play/src/tiles/draw.rs` draws the tiles mods added to `HUDMainMenu` over
NVForever's own HUD, from the layout of §3.1.1. What it takes the traits to mean:

- [documented: hud.md §14] Places are interface units, 960 to the view's height at
  every size (`screen()` is 960 × width/height wide); [design] one unit is
  height/960 window pixels (`interface::u`), as the current HUD.
- [design] Which tiles: §7's `mod_added_tiles` roots, and inside them each tile
  `mod_tile_drawn` accepts: Injected and Template always, Code never, File tiles only
  against the archived file's name paths. Since #975 `fnv_vfs::Vfs` keeps the highest
  archive's copy under each loose file (`Vfs::read_archived`), and the game and `fnvtool
  ui mod-tiles` both compare a loose replacement with it (`Ui::archived_name_paths`);
  before, the game drew no File tile, as the VFS kept one location per path. `fnvtool`'s
  comparison over the test install (2026-10-29, overlays [observed]): vanilla 0 tiles,
  Clean Vanilla Hud's loose HUD 0, Vanilla UI Plus 5, its `VUI+` outline tiles in the
  prompt box.
- [design] A `rect` draws a filled box only when it sets `red`, `green` or `blue`
  itself; otherwise it only places its children. `nif`, `radial` and `window` tiles,
  and empty pictures and strings, draw nothing in v1 (the whole HUD's drawing draws
  `nif` tiles, §3.1.3).
- [design] An `image` is its `filename` stretched over its box: the piece of
  `interfaceshared.tai` of that file name when the atlas has it, else the file under
  `textures\` (`interface::Pictures`). Another `.tai` named by `texatlas`, and `zoom`,
  `tile`, `cropx`, `cropy`, are not applied to the mods' tiles in v1 (the whole HUD's
  drawing applies `texatlas`, `zoom` and `tile`, §3.1.3).
- [hypothesis] `font`: 1 to 9 is the INI's `sFontFile_N` [documented:
  Fallout_default.ini]; none, 0 or another number is font 1. The sneak word's tile has no
  `font`, and the original's widths for it (hud.md §14.1: 106 "[HIDDEN]", 136, 14) are
  font 1's glyph advances summed for "[HIDDEN]", "[DETECTED]" and one space [observed:
  the font files, 2026-10-29].
- [observed: the font files against hud.md §14.1] A text's width is the sum of its
  glyphs' advances (fonts.md §3): font 7 "Button Text" 156 and "Talk" 54, font 8 "+0" 50
  and "+9" 49, as the original wrote. A `wrapwidth` of 0 does not wrap (the prompt's
  `button_text` has 0 and measured one line). [hypothesis] Each line is set left,
  centred or right inside the box by `justify`; [observed: hud_menus capture
  2026-10-31, fonts.md §3.1] the lines stand one pitch of their font apart
  (`draw::line_pitch`: 31, 35, 32, 36, 25, 38, 38, 52 for fonts 1-8), which for fonts 2,
  3, 6 and 7 is not the font file's size (until this change NVForever set them the size
  apart).
- A text's first line stands under its top by its font. [observed: hud_textbase capture
  2026-10-31, hud.md §14.10] Measured from the capitals' ink in the original against
  where NVForever's glyph pictures put it (fonts.md §3), the lines stand 24.9 units under
  the top in font 7, 30.5 in font 8 and 21.0 in font 1 (a tile with no `font`).
  [observed: hud_menus capture 2026-10-31, fonts.md §3.1] Every font, measured the same
  way at three window sizes: font 1 20.3, fonts 2-5 about 16.5, font 6 22.4, font 7
  24.5, font 8 30.5. [design: NVForever's calibration, anchored on font 7]
  `draw::text_base` keeps font 7 at the earlier readings' 24 and stands every other font
  as far from it as the hud_menus capture shows: font 1 20, fonts 2-5 16, font 6 22,
  font 8 30 (29.5 from hud_textbase until then; fonts.md §3.1 says why 30); font 9, which
  draws nothing in the original, takes font 7's 24 [design]. Texts of fonts 7 and 8 set at one `y` do not
  stand on one line: `radiation_text` ("RAD/SEC", font 7) and `radiation_text_value`
  ("+9", font 8), both at y 65, end 25.0 and 31.7 under it. T12c (2026-10-29) stood
  every font's line 24 under the top [hypothesis then: the two radiation texts read on
  one line 23.7 under their `y`, the sneak word (font 1, `SneakMeter` y 30) 22, the
  prompt's lines (font 7) 24.5-25.5 under the captures' places, private shots
  `hud_corners` rad_coc_spot, `hud_jip` s4_sneak_drawn, hud.md §14.8]; the font-8
  reading was wrong (#979). Until T12c each font's tallest glyph set the line
  (`interface::line_base`: 28, 33, 48 for fonts 1, 7, 8).
- [observed: hud_textbase and hud_menus captures 2026-10-31] A text tile's `height`,
  read in the original, is its font's tallest glyph picture whatever its string, plus
  one pitch for each line after the first: one line 36, 32, 31, 37, 28, 38, 42, 64 for
  fonts 1-8, two lines 67, 67, 63, 73, 53, 76, 80, 116 (fonts.md §3.1). NVForever's
  `draw::measure` gives that height (`tallest_glyph` + (lines - 1) × `line_pitch`; until
  this change lines × the font's size); the layout uses it for a text tile without its
  own `height` (§3.1.1), and the code writes no text tile's `height` into the tree
  [design: not yet needed].
- [hypothesis] Colour: a tile's own `systemcolor`; else its own `red`/`green`/`blue`
  when it sets any; else the nearest tile above's `systemcolor`; else system colour 1, the
  HUD's [documented: GECK wiki Color UI directives, "Systemcolor is by default 1 and always
  assign"; T12c: until then its own, white, which drew `AmmoTypeLabel`, a menu child with
  no colour above it, white].
  `&hudmain;` is the HUD's colour (`interface::hud_rgb`, the settings'), `&pipboy;` the
  Pip-Boy's, `&nosystemcolor;` and `&uselocalcolor;` the tile's own, other names and
  unknown non-zero numbers the HUD's [hypothesis]; numeric 1/2 map to hudmain/hudalt
  [observed: private job hud_jip file_0032; hud_hardcore2 hc4-hc5]. `&hudalt;` is 255 67 42 [observed: Fallout_default.ini
  `iSystemColorHUDAlt*`, radiation.md §6]. `brightness` (clamped to 0–255) scales the
  colour, `alpha` (§3.1.1's product) is its opacity.
- [hypothesis] Stacking: the pieces draw in order of their summed `depth`, ties in
  layout order; all over NVForever's HUD and its messages, under the dialogue menu and
  the Pip-Boy [design: `GlobalZIndex` 60]. `clips` crops through a clipping node at the
  clip rectangle.
- [observed: hud.md §14.1, the font files] The code writes three HUD text tiles' `width`
  as their text's measured width, before scripts: `SneakMeter/sneak_nif`,
  `Info/justify_center_hotrect/button_text` (its hotrect's width follows by the
  prefab's own expression, the child's width plus `_horbuf` [documented: text_box.xml])
  and `RadiationMeter/radiation_text_value` (`fnv_game::ui_hud::write_text_widths`),
  only with the game's fonts. A tile without a `string` keeps its width [design].

### 3.1.3 The whole menu, `zoom`, `tile`, `texatlas` (T12a)

With `[UI] TileMenus=HUDMainMenu` (ui-tile-tree.md §6.2; hud.md §14.7) the renderer draws
every tile of the HUD, not only the mods' (`nvf-play/src/tiles/draw.rs`,
`Select::Whole`); then, and only then, it applies these. With the switch off §3.1.2
stands unchanged: every picture stretched, the shared atlas only.

- [documented: GECK wiki Zoom, revision 39942] `zoom` multiplies an image's size and
  "cannot exceed the specified height and width"; the default is 100; -1 is free scaling
  ("Scales any texture to 256x256 regardless its source size"). [hypothesis] A number
  draws the picture at its texels' size times `zoom` / 100, one texel one interface unit
  at 100, from the box's top left, cut to the box: the compass window (345 x 64, zoom 100)
  then shows 345 of the 1024-texel strip, which turns 1024 units [observed: hud.md §14.4
  `filewidth` 1024, §14.5 the scale]. The anchor is not observed.
- [hypothesis, T12c] No `zoom` (`draw::Fit::Natural`, `draw::placed`): an atlas piece
  is stretched over its box, a picture read from its own file stands at its texels' size
  (zoom's default 100) from the box's top left, cut to the box. The corner brackets'
  atlas pieces are 255 x 127 texels [observed: interfaceshared.tai, an atlas of 1024 x
  1024 by its heading] in 512 x 256 boxes the original fills on the screen; the
  radiation pointer's 63 x 31 piece is drawn shrunk in its 46 x 24 box (its triangle
  about 10 pixels across at 720 lines, 15 at its texels' size); the radiation bracket,
  its own 512 x 128 file in a 567 x 120 box, stands unstretched: its notches 60 units
  apart where its file has them 60 texels apart, its upright at W - 57 on the screen,
  where x -135 + 494 texels puts it (stretched it stood at the view's edge, T12b)
  [observed: private shot `hud_corners` rad_coc_spot]. Until T12c every unzoomed picture
  was stretched.
  [hypothesis] `&scale;` (number 0, §2.3) stretches: its pictures (the compass ticks
  32 x 32 in 32 x 32 boxes, the condition arrows, the sneak bracket) fit their boxes.
- [hypothesis] `tile` true repeats the picture over the box from its top left at its
  texels' size times a numeric `zoom` / 100 (100 without one), both ways: the bars'
  8-texel tick picture in widths the code sets in steps of 8 [observed: hud.md §14.3];
  the template's `_ImageWidth` 8 says the same [documented: hudtemplates.xml
  `template_meter`]. Zoom as a multiplier of the picture's size [documented: GECK wiki
  Zoom]; no vanilla tiled picture has a zoom; T12c's condition fill is given one
  (`vanilla::CONDITION_FILL_ZOOM`, hud.md §14.7) [design].
  [design, T12d] The repeats are drawn as one image: the picture's texels (its atlas
  piece's or its own file's, decoded on the CPU) repeated into an RGBA image the box's
  size in whole texels, stretched over the box (cut to it) and sampled linearly
  (`nvf-play/src/repeat.rs`), so a repeat stands `width x scale` pixels from the next at
  any window size; a repeat per quad (Bevy's `NodeImageMode::Tiled`, T12a-T12c) put each
  on whole pixels, the bars' 8-unit ticks 6 and 7 pixels apart by turns at 1280 x 800
  [observed: the owner, T12d; hypothesis: the original's even, slightly soft ticks are
  one repeated texture filtered across the bar, hud.md §14.9]. A box over 4M texels, or
  a picture whose texels cannot be read, keeps Bevy's tiles.
- [design, T12d; maintainer's review of #902] The renderer keeps each piece's nodes in a
  slot of its own (by tile, stacked by the piece's place in the list) and compares the
  pieces with the last frame's one by one (`draw::change`): a piece that only moved or
  changed colour (the compass marks every frame while the player turns, the fades and
  flashes) has its nodes moved and recoloured; a piece of another look or size is
  spawned again alone; a piece that comes or goes adds or removes its slot.
- [observed, T12c] An image without `texatlas` reads its own file, not the shared atlas
  (`draw::Source::Own`): the radiation bracket, whose file comments its `texatlas` line
  out (radiation.md §6), shows its own file's slanted ends and single line on the
  original's screenshots, where the shared atlas's piece of that name has square ends
  and a doubled line. The mods' tiles with the switch off keep §3.1.2's lookup.
- [observed, T12c] **The atlas pieces are other art than the pictures' own files**, not
  smaller copies: the corner brackets' pieces have cut corners, stepped arrow ends and a
  doubled long line, the compass strip's a doubled line beside the letters and thick
  middles, where the own files have square corners and single lines (both decoded from
  `Fallout - Textures2.bsa`). The original's screenshots of the corners (`hud_corners2`
  c0, c4) show the atlas pieces' shapes, as corner-messages.md §2.2 found for the
  message bracket; the tile drawing draws them. The hand-built HUD (`hud.rs`) draws the
  own files.
- [documented: GECK wiki Texatlas; interfaceshared.tai's heading] `texatlas` names a
  `.tai` index under `textures\`; each line gives a picture's file name, the atlas file
  (`InterfaceShared<idx>.dds`) and its place as parts of the atlas. Any index a tile names
  is read once and kept (`interface::Pictures::locate`); [hypothesis] its atlas textures
  stand in the index's folder, as the shared one's. [design] A picture the named index
  lacks is looked up in the shared index, then as its own file.
- [design] `clips` as §3.1.2, a clipping node, and `clipwindow` where §3.1.1 reads it;
  the HUD's own clip tiles (`compass_icon`) clip their children only.
- [hypothesis, T12e] A text's `zoom` scales neither its glyphs nor its laid-out box: the
  Hardcore words read zoom 75 [observed: private job hud_hardcore2 hc0-hc11] and their
  tiles' widths are font 7's at 100 (45 for "H20", as `HitPoints`' "HP" 30) [observed:
  the capture, the font file], and on the original's screen they are as tall as "HP"
  (17 pixels at 720 lines) [observed: hud_compare needs_limbs, hud.md §13]. T12b scaled
  both by zoom/100 [hypothesis, corrected T12e]. The trait is kept as read
  (`layout::Text::zoom`).
- [design] The compass strip is the one picture moved by code with no trait (hud.md
  §14.4): `HUDMainMenu/HitPoints/compass_window`'s picture is drawn twice side by side
  and moved by the heading as the hand-built HUD moves its own (1024 units a turn, the
  marks' heading at x 148 of the window [observed: hud.md §14.5], the strip's N at 154 at
  heading 0 [observed: hud.md §14.10, T12e]), faded out from x 273 to 341 of the window
  as the original's [observed: hud.md §14.10]: the window's part of the strip baked into
  one image with each texel column's alpha on the ramp, as the hand-built HUD's
  (`crate::compass_strip`) [design: one image, no seams between parts].
- [design] `nif` tiles (the HUD's one, the breath meter) are drawn with the whole menu:
  the `filename` under `meshes\`, its flat textured squares as turned pictures in the
  tile's colour and alpha, the `animation` sequence's vis keys at the time NVForever's
  own `_NVFSequenceFraction` gives (hud.md §14.12, `tiles/model.rs`). [hypothesis] The
  model's origin stands at the tile's `x`, `y`, a model unit an interface unit; the
  tile's `width` and `height` do not scale it.

### 3.2 The pointer and keys (T8)

What the game's code writes for input, and how `fnv_ui::input` reads the files for it.
Wiki pages in our own words [documented: the GECK wiki's Target, Mouseover, Clicked,
Shiftclicked, Ref, Input, Hotrect, Wheelmoved pages and "Tutorial: Understanding XML"
§3.2 c), export of 2026-10-28].

- [documented] `target` true makes a tile react to the pointer, the keys and a
  controller; `mouseover`, `clicked`, `clicksound`, `mouseoversound`, `wheelable`,
  `draggable` need it. A hotrect with `target` true makes its children targets too,
  even with their own `target` 0. A hidden tile (`visible` false, its own or an
  ancestor's) is neither moused over nor clicked, but a script can still click it.
- [documented] `mouseover` is 1 while the tile has the pointer (or a controller's
  highlight) and 0 otherwise. [hypothesis] Only on the tile that takes the event (below),
  not on the tiles above it; observer check §8 item 9.
- [documented, disagreeing] `clicked`: "set to 1 by the engine for a single frame after
  the element is clicked by the mouse" (the tutorial) against "it will count how many
  times a element has been clicked" (the Clicked page). [hypothesis] The single frame
  (`fnv_ui::input::CLICKED_RULE`, `ClickedRule::OneFrame`; `Count` is the other); a
  press, not the release, clicks. Observer check §8 item 8. `shiftclicked` is 1 for a
  frame when Shift is held during the click [documented].
- [documented: tutorial] A click sets `clicked` on a tile with an `id`; the Clicked page
  says on any target. [hypothesis] `clicked` on any target, the `id` being what the
  menu's code reads.
- [documented: Target] `target` has no effect on a `rect` (the tutorial says "rect or
  image"); [design] `rect` and `menu` tiles are never under the pointer.
- [observed: the vanilla menu files, read privately] Every text button is the text_box
  prefab: a target hotrect with `alpha` 0 and its text, box and key hint as children;
  the save list's rows and the scrollbars are target hotrects at alpha 0 too.
  [hypothesis] `alpha` therefore does not stop the pointer; a hit on a child the hotrect
  made a target goes to the nearest tile above it whose own `target` is true.
- [hypothesis] Boxes are the layout's (§3.1.1): `x`/`y` added to the parent's top-left,
  `width`/`height`, a text measured when it has no size and moved by its `justify`,
  `clips` (or a `clipwindow` turned on, §3.1.1) cropping the children; `locus` is not
  read, as the layout does not.
- [documented: tutorial, the higher `depth` is drawn on top] [hypothesis] Where targets
  overlap, the highest summed `depth` takes the pointer, a tie going to the one drawn
  later; a menu opened later is over an earlier one whatever their depths. Observer
  check §8 item 7.
- [observed: the vanilla files] `_PCButton_<key>` appears only on the `<menu>` element
  (61 times, any case: `_PCButton_E`, `_PCBUTTON_S`), its value the name of a tile; the
  letters used are A, B, E, F, Q, R, S, W and X. Pressing the key clicks that tile
  [documented: the menus' key hints, e.g. lockpicking.md §2 "E is Exit"]; LStewieAl's
  Tweaks "extends vanilla XML keybindings" to digits, F1-F12, Caps, Tab and Alt
  [documented], so the unmodded game reads letters. [hypothesis] The tile is the first of
  that name in the menu, and it must be shown and a target.
- [documented: Input] The controller's traits `xbuttona` `xbuttonb` `xbuttonx`
  `xbuttony` `xbuttonlt` `xbuttonrt` `xbuttonlb` `xbuttonrb` `xbuttonstart`, and `xup`
  `xdown` `xleft` `xright`, which the arrow keys press too; the stick buttons are
  `&xbuttonls;` `&xbuttonrs;` [documented: Hardcoded UI Shortcuts]. `xdefault`'s meaning
  is unknown [documented: Input] and is not read. [observed: the vanilla files] Their
  value is a `<ref src="…" trait="clicked"/>` on the `<menu>` element (the button the
  key clicks) or `<ref … trait="mouseover"/>` on list items (`xup`/`xdown` to the
  neighbour); one `xscroll` refs `_value_adjust_poke`. [documented: Ref, Input] A `ref`
  to `mouseover` moves the highlight there; one to `clicked` clicks that tile.
  [hypothesis] The highlighted tile's trait first, else the menu's, also when the
  tile's ref does nothing (it names no tile, or one hidden or not a target); a key-moved
  highlight holds until the pointer moves or its tile is hidden or stops being a target;
  a ref to another trait is reported, not followed [design].
- [documented: ClickMenuButton, SetOnMenuClickEventHandler] `Menu#<id>` names the tile
  of that `<id>` in the menu. [hypothesis] The first in file order when two share one.
- [documented] Wheelmoved, Draggable (`dragx` …), Repeathorizontal: not built (T8 left
  them; `wheelable` is the next step for lists).
- [design] NVForever: `fnv_ui::input::hit` (the tile under a pointer in interface units,
  960 to the view's height as the tile renderer draws), `Pointer::step` (one frame:
  pulses ended, the pointer followed, the press, the keys), `fnv_game::ui_input::step`
  (that and the JIP events); the engine feeds only the menus drawn from tiles that take
  input (none in play yet: the HUD takes none; the test aid `NVF_PLAY_TEST_POINTER`
  makes it take input). GetActiveUIComponentName/FullName/ID read the highlighted tile,
  empty and -1 with none [hypothesis: observer check §8 item 10]; ClickMenuButton (a
  target with an `id`, `clickTimes`) and ClickMenuTile (any target, hidden too) click as
  a press does.

## 4. Operators

Inside a trait, operators work the value out from the first to the last, each on the
value so far.

### 4.1 The operators

[observed: every operator tag in the vanilla files; the meaning of each is a hypothesis
from its name and use unless marked]

| Tag | Does |
|---|---|
| `copy` | the value becomes the argument |
| `add` `sub` `mul` (`mult`) `div` `mod` | arithmetic with the argument; dividing by 0 leaves the value |
| `min` `max` | the smaller / larger of the value and the argument |
| `floor` `ceil` `round` `trunc` `abs` | round the value; the argument (always 0 or 1 in the files) is not used |
| `rand` | a whole number from 0 below the argument |
| `eq` `neq` `lt` `lte` `gt` `gte` | the value becomes 1 or 0 by comparing it with the argument |
| `and` `or` | 1 or 0 from the value and the argument as true/false |
| `not` | 1 when the argument is false, else 0 |
| `onlyif` / `onlyifnot` (`onlynotif`) | when the argument is false (true), the value becomes 0 (§4.3) |
| `ref` | the value becomes the argument; a live link (input traits use it) |

Both `mul` (193 uses) and `mult` (120) are in the files; `onlynotif` (3 uses) is read as
`onlyifnot`. MCM's `start_menu.xml` uses a `<neg>` no vanilla file has; it is ignored
with a warning.

### 4.2 Arguments

An operator's argument is one of:

- **a literal or constant** written inside it: `<add> 2 </add>`, `<eq> &center; </eq>`;
- **another tile's trait**: `<copy src="parent()" trait="width"/>`;
- **operators nested inside it**, worked out on their own first:
  `<sub> <copy src="parent()" trait="width"/> <div> 2 </div> </sub>`.

The `src` forms [observed: counts over the vanilla files; meanings hypothesis unless
marked]:

| `src` | Uses | The tile |
|---|---|---|
| `parent()` | 2473 | the tile's parent |
| `me()` | 1798 | the tile itself |
| `sibling(name)` | 1660 | the tile of that name under the same parent |
| `sibling()` | 2 | the tile just before it under the same parent |
| `screen()` | 451 | the view: traits `width`, `height`, `cropx`, `cropy`, `resolutionconverter`, set by the engine. Width is 960 × window width/height, height 960 and `resolutionconverter` 960 / window height [observed: hud_menus capture 2026-10-31, three window sizes] (1280 x 720, 1920 x 1080 and 1280 x 1024). NVForever uses those interface units only while `[UI] TileMenus` names a menu drawn from its file (`tiles::interface_screen`), and writes `cropx` and `cropy` only while `DialogMenu` draws a conversation (dialogue-menu.md §6.2); otherwise it keeps window pixels and no converter [design: preserve default play; applying the observed units unconditionally is a follow-up]. `fnvtool ui tree --screen` writes the size it is given [design] |
| `globals()` | 244 | `globals.xml`'s tile (`gloabls()`, once, read the same) |
| `io()` | 242 | the menu the tile is in, whose traits the game's code reads and writes (barter reads `io()`'s `_IsContainerListSelected`, which its `<menu>` declares) |
| `child(name)` | 148 | the child of that name; `child()` the first child |
| `grandparent()` | 87 | the parent's parent |
| `parent(name)` | 31 | the nearest tile above of that name |
| `strings()` | 10 | interface strings (`_weapons`, `_apparel`, `_button`, …); which ones, and from where, is not known |
| a bare name | ~1100 | the first tile of that name in the same menu, else the menu of that name (`StatsMenu`, `MM_Tabline`) |

A trait that is not there reads 0 [hypothesis].

### 4.3 `onlyif`

`onlyif` never comes first: 196 are last in their list and 99 in the middle, always
after a `copy` [observed]. Read as "when the condition is false the value so far
becomes 0, and the operators after it go on from there" [hypothesis], which fits every
use found:

- `copy 40, onlyif _box_visible` is 40 or 0;
- `copy X, onlyif C, add Y` is X + Y or Y;
- inside a nested `sub` (`copy width, div 2, onlyif centred`) the whole subtraction is
  0 when the condition fails;
- `globals.xml`'s `_line_thickness` (`copy 1, onlyif &console;, add 2, mul
  resolutionconverter`) is 2 × the converter on a PC.

### 4.4 A trait name that ends in `_`

`<copy src="me()" trait="_filename_"/>` after the value is a number n reads the trait
`_filename_n` [hypothesis: the meter and barter files pick a picture that way; 15
names end in `_` in the vanilla files, all after a numeric value]. After a constant it
reads the constant's number (§2.3; `&false;` 0, `&true;` 1) [hypothesis, T15a: the meter
prefab copies `_SolidMeter` and `vertical_fade_line.xml` copies `_bottom_to_top`, both
`&false;` or `&true;`, before the copy, and their `_filename_0`/`_Filename_1` are the two
pictures; until T15a the name was read without a number, and the message box's corner
lines and the prompt's meter had no picture].

## 5. Paths

[documented: the GetUIFloat, SetUIFloatAlt, ModUIFloat, GetUIString and InjectUIXML
pages]

- A path is `MenuName/Tile/…/Tile/trait`, segments split by `/` or `\`, names without
  case.
- `*` matches any child; `name:N` matches the N-th child of that name and `*:N` the
  N-th child [documented: "`*:0/*:5/*:9`"]. Both count from zero **from the last
  matching child**, reversing file/creation order [observed: private jobs hud_hardcore
  hc0, hud_xp_limbs all, hud_prompt2 p0-p8; hud.md §14.8]. Unindexed matching still
  searches in file order [hypothesis].
- With `*`, the first child that leads to a full match is taken [hypothesis].
- GetUIFloat returns -999 when the trait is not found [documented: GetUIFloat];
  SetUIFloatAlt, ModUIFloat return 1 on a change and 0 otherwise [documented].

## 6. Updates

The original's update order is not known. NVForever's: every trait with operators is
worked out on load; after that, a trait is worked out again when a trait it read has
changed since, until nothing changes (at most 32 passes; a loop of traits reading
each other is reported) [design]. A value a script or the engine sets holds until
something the trait's operators read changes [hypothesis: MCM and HUD mods set traits
that also have operators; observer check §8 item 3].

Vanilla, 2026-10-26: 3,529 tiles, 40,583 traits, 19,541 with operators, over the 49
menus; every menu settles [observed: fnvtool ui menus; loading all 49 takes 0.4 s].

## 7. How mods change menus

[observed in the owner's mod folders, 2026-10-26, F:\Modlists\VNV\mods]

1. **Replacing a file**: a loose `menus\…` file takes the archive's place (Clean
   Vanilla HUD's `hud_main_menu.xml`, Vanilla UI Plus's menus, MCM's
   `menus\options\start_menu.xml`).
2. **An include the replaced file adds**: MCM's `start_menu.xml` includes
   `includes_StartMenu.xml`, which includes `MCM\MCM.xml`, so other mods can add to the
   same `includes_` file.
3. **UIO include lists.** [observed: the data files below, read 2026-10-29 from
   `E:\Games\Viva New Vegas\mods`; no DLL or source read] The lists pair an include
   line with a condition. Folder case varies (`UIO\Public`, `uio\public`, `Menus`).
   Short literal pairs from those files:

   ```text
   ; UIO - User Interface Organizer/uio/supported.txt
   wmm\wmm.xml:inventorymenu:glow_branch
   ?The Weapon Mod Menu.esp
   mcm\mcm.xml:startmenu
   ?The Mod Configuration Menu.esp	||	?Project Nevada - Core.esm
   jetpack\b3jetpack_hud.xml:hudmainmenu
   ?Jetpack.esp	&&	!darnui
   ; Just Assorted Mods/uio/public/JHM.txt
   JHM\JHM.xml::HUDMainMenu
   true
   ; MCM Extender/UIO/Public/MCMExtender.txt
   MCMExtender\ListRect\text.xml::@Prefabs\MCM\ListRect.xml::text
   true
   ; UI Improvements/uio/public/UII.txt
   UII\CharGenBox.xml::CharGenMenu::TM_DescriptionBox
   true
   ; Pip-Boy UI Tweaks/UIO/Public/PipBoyUITweaks.txt
   Stentorious\PipBoyUITweaks\_ySI\ButtonIcon.xml::@Prefabs\UII\PerkBrowser.xml::PerkBrowser
   #UII\PerkBrowser.xml && #..\ySI\ySI.xml
   ```

   [observed: those lists] Both `:` and `::` separate include, target and optional
   tile name. Targets are menu names in mixed case or `@Prefabs\...` paths. `;`
   lines and blank lines separate groups. Conditions contain `true`, `?plugin`,
   `#path`, `!`, `&&`, and also `||` (the MCM alternative in supported.txt).
   [observed: Just Assorted Mods/Menus/Prefabs/JHM/JHM.xml;
   MCM Extender/Menus/Prefabs/MCMExtender/ListRect/text.xml;
   UI Improvements/menus/prefabs/UII/CharGenBox.xml] Include names resolve below
   `menus\prefabs`. [observed: UI Improvements/menus/prefabs/UII/PerkBrowser.xml;
   Pip-Boy UI Tweaks/UIO/Public/PipBoyUITweaks.txt] The `#UII\PerkBrowser.xml`
   operand names an existing file there. The advrecon/primaryneedshud/filmgrain
   operands of supported.txt have no matching files in the inspected mod folders.
   [hypothesis] `#` uses that same prefab base, with `..` resolved before the VFS
   lookup; `#..\ySI\ySI.xml` is consistent with a sibling directory under menus,
   but that optional file is absent here.

   [observed: MCMExtender.txt and supported.txt; directory listing of the installed
   mods' uio/public files] `darnui` is used both positive and negated; no
   `uio\public\darnui.txt` was found. [hypothesis] Its meaning remains unknown;
   NVForever treats the atom as false and logs it even in a short-circuited branch
   (`!darnui` therefore evaluates true). No filename-based alias is assumed.

   [design] `fnv_ui::uio::{Lists, Condition, Report}` parses forgivingly, records
   malformed lines with list/line locations, and evaluates against a `Source` plus
   an active-plugin predicate. `Source::list(dir)` defaults empty; `MemSource`, the
   game's `UiFiles` and fnvtool's overlay source discover files recursively with
   case/slash-insensitive paths. supported.txt goes first, then immediate
   public/*.txt alphabetically. Identical normalized include/target/tile/condition
   entries occur once. Negation binds before AND before OR [hypothesis: precedence
   for mixed expressions is not established by these lists].

   [design] `Ui::configure_uio` runs before loading menus. Menu entries apply after
   the file's ordinary includes; prefab entries modify the fragment before expansion,
   including template bodies. Named targets use the first matching tile in file order
   [hypothesis: duplicate-name selection is unmeasured]. An untargeted entry appends
   to the menu root or prefab fragment; a named one appends within that tile
   [hypothesis: UIO's insertion position is unmeasured; appending preserves existing
   order and allows later included traits to override earlier ones]. Includes retain
   their own `Origin::File { path, loose }`. A loose UIO addition and its file
   descendants pass `mod_tile_drawn` without an archived name comparison, including
   name collisions; the archived baseline is built without UIO [design].

   [design] Reports distinguish unique applied entries from application counts
   (prefabs can be read repeatedly), and skip reasons condition false, file missing,
   target missing. Target missing also means not loaded yet in the current tree.
   The game uses `GameData::load_order` and logs decisions at startup and new
   applications during menu lifetime sync. `fnvtool ui menus|tree|mod-tiles` includes
   a `uio` report; it discovers top-level .esm/.esp names in Data and overlays for
   `?`, or uses an explicit repeatable `--plugin` set. Overlays count for `#`.
4. **Scripts at run time**: JIP's `InjectUIXML "path" "data\menus\…xml"` and
   `InjectUIComponent "path" "<xml with %g>"` put XML into a loaded menu
   [documented: InjectUIXML, InjectUIComponent]. Most menus are loaded only while open
   and must be injected again each time; `HUDMainMenu`, `InventoryMenu`, `MapMenu`,
   `RepairMenu`, `StatsMenu` and `ItemModMenu` stay loaded [documented].
5. **Trait functions**: SetUIFloat, SetUIString and friends on paths (§5).

- [design] `Tile::origin` distinguishes `File { path, loose }`, `Injected`, `Template`
  and `Code`. Menu loading and ordinary includes record the exact path passed to
  `Source::read`, with each included file's own provenance. Injection and template
  creation stamp every tile they create, including included descendants, with their
  creator's origin; later trait writes do not change it. Pseudo-tiles start as Code;
  loading globals records its file origin.
- [documented: crates/fnv-vfs/src/lib.rs, Vfs::source_of] The VFS exposes whether the
  winning source is loose. [design] `Source::is_loose(&self, path: &str) -> bool`
  defaults to false; `UiFiles` implements it using that VFS query. The result is stored
  at load time, so the source need not remain available to classify tiles later.
- [design] `Ui::mod_added_tiles(&self, menu: TileId) -> Vec<TileId>` lists top-most
  live Injected, Template or loose File tiles in tree order, including the menu itself
  when loose. Descendants of a returned root are not listed separately; unloaded tiles
  and other menus are excluded. Loose-file replacement selects the whole replaced
  subtree; it is not a comparison against the vanilla file's contents.
- [design] `Ui::add_code_tile_from_template(&mut self, src: &dyn Source, at: &str,
  template: &str, name: Option<&str>) -> Option<TileId>` makes all copies from the body
  Code, including multiple top-level tiles, and returns the first as `add_from_template`
  does. Code tiles are not mod roots; injections underneath them can be. A future
  renderer drawing selected subtrees must still exclude Code items inside those roots.

## 8. Observer checks

What the original can settle (copied to docs/handoff/observer-checks.md):

1. **The numbers of `&true;` and `&false;`.** Answered 2026-10-28: 1 and 0, and
   `&does_not_stack;` 6008 [observed: hud.md §14]. `GetUIFloat` on a trait the file sets to
   `&true;` and nothing changes (e.g. `HUDMainMenu/locus`), and on one set to `&false;`
   (`HUDMainMenu/_CrosshairVisible` before any aiming). Also `&center;`, `&right;` (a
   `justify`) and `&scale;` while there.
2. **How the original reads mismatched end tags.** SUP's `DumpTileInfo`
   [documented: DumpTileInfo, origin SUP] (or `GetUIFloat` with `:N` paths) on
   `QuestAddedMenu`'s two buttons and on MCM's `StartMenu/MCM` tree: which tile ends up
   holding what.
3. **Whether a set trait holds.** `SetUIFloat` on a trait that has operators (a HUD
   piece's `x`), then change what it reads (a screen size change will do), and read it
   again.
4. **Whether SetUIFloat makes a missing trait.** `SetUIFloat "HUDMainMenu/_NewTrait" 5`
   then `GetUIFloat` on it.
5. **What the deprecated forms' "children tiles bug" is.** The wiki says SetUIFloat,
   GetUIFloat and SetUIStringEx are "bugged for templated children tiles" and MCM
   depends on the bug, but its "Bug: Children Tiles" page is not in the export. Compare
   SetUIFloat and SetUIFloatAlt on a list item made from a template (`BarterMenu` rows).
6. **`name:N` / `*:N` counting: answered 2026-10-29.** Last matching child first,
   zero-based [observed: private jobs hud_hardcore hc0 (LMBs *:0), hud_xp_limbs all
   (Right_Leg/XPLevelUp *:0), hud_prompt2 p0-p8 (VAL left:0, WG left:1, hotrect *:8);
   hud.md §14.8]. These reads used JIP GetUIFloatAlt; deprecated xNVSE GetUIFloat
   parity remains untested [hypothesis]. No contrary vanilla menu/script dependency
   was established by these captures; other menus/scripts still need checking.
7. **Which target takes the pointer where two overlap.** Ours: the highest summed `depth`,
   a tie to the one drawn later; a later menu over an earlier one (§3.2). In a menu with
   overlapping buttons (the level-up menu's list over its buttons, or two tiles injected
   with InjectUIComponent at the same place and different depths), GetActiveUIComponentName
   with the cursor over the overlap.
8. **How long `clicked` holds.** Ours: 1 for the frame of the press, then 0 (the
   tutorial), not a count (the Clicked page). A tile injected with
   `<_n> <copy src="me()" trait="_n"/> <add src="me()" trait="clicked"/> </_n>` and one
   with `<_c> <copy src="me()" trait="clicked"/> </_c>`, clicked twice; GetUIFloat on
   both after each click (a count: `_c` 1 then 2; a frame: `_c` back to 0, `_n` 1 then 2),
   and whether it is the press or the release that sets it.
9. **Whether `mouseover` reaches the tiles above.** With the cursor on a text_box button
   (its text child), GetUIFloat on the hotrect's, the text's and the parent rect's
   `mouseover` (ours: the hotrect 1, the others 0 or absent).
10. **The pointer functions with no menu, and on a tile without an id.**
    GetActiveUIComponentName, GetActiveUIComponentFullName and GetActiveUIComponentID in
    game mode with no menu open (ours: "", "" and -1), and over a target with no `<id>`
    (ours: its name and -1); ClickMenuTile's return value.
11. **Where MCM's shorthands point** (#976). With MCM installed and a mod's page open,
    GetUIFloat on `StartMenu/MCM/*:1/*:1/_optionID` and on
    `StartMenu/MCM/MCM_Options/Option1/_optionID`, and `GetMCMFloat 1 1 "_optionID"`,
    `GetMCMFloat 1 0 "_optionID"` with the pointer on the first row (ours: the
    shorthands count from 1 in file order, 0 stopping, §10; `*:N` from the last child).

## 9. The rest of the tile functions (task T9)

[design] Built headless in `crates/fnv-game`: `ui_more` (dispatch, after
`ui_functions` in `nvse::call`), `ui_gradients` (changes over time), `ui_cache`
(CacheUI), `ui_hudbar` (SUP's bars); tests `crates/fnv-game/tests/ui_more.rs`, the
units in those files, and `fnv_ui` `a_trait_by_a_path_under_a_tile`
(`Ui::find_trait_in`). Every page is the GECK wiki export of 2026-10-28; the tags per
rule are in the code beside it. Smoke: `ui-gradual-in-game`, `ui-string-dimensions`,
`ui-cursor-inspection`, `ui-typewriter-tile-script`.

| Functions | Page(s) | What NVForever does | Open |
|---|---|---|---|
| SetUIFloatGradual | SetUIFloatGradual [documented] | modes 0-3 on real time; stop with the path, stop and set with a start; the engine steps them after the scripts, before `Ui::update` (`engine/nvf-play/src/tiles/mod.rs`) | linear in time [hypothesis]; a missing duration ends at once [hypothesis] |
| GetUIFloatGradualActiveForTile, …ForTileTrait, …Mode, …Timer | the four SUP pages [documented] | read JIP's table; Mode -1 when none [documented] | Timer -1 when none, a repeating change's time left is its round's [hypothesis] |
| SetUIStringReveal, CacheUIStringReveal(Alt) | SetUIStringReveal, CacheUIStringReveal [documented] | letters by the time's share, from the left or right, inverted, ease in as the square [hypothesis for the rounding, the right and the ease] | none |
| SetUIFloatMultiple | SetUIFloatMultiple [documented] | every tile inside of the type, that has the trait; the count back [design] | whether it makes the trait [hypothesis: not] |
| AttachUIXML, AttachUIComponent | AttachUIXML, AttachUIComponent [documented] | a single `rect` into `HUDMainMenu`'s root, kept in `UiExtra::attached` with the reference and node; 1 or 0; the VM formats the component. Replaces the one attached before to the same (reference, node, rect name), keeps it hidden until placed, drops it when the reference is deleted, disabled or outside the loaded cells [design] | node validation, animated transforms and screen projection: its own issue, **Project attached UI components onto model nodes** |
| AddTraitToTile | AddTraitToTile [documented] | makes or sets the trait; an empty name makes none (0) [design] | the original's list of trait ids it allows is not known: any other name taken [hypothesis] |
| GetUIValueType, GetUIElementType | the SUP pages [documented] | 1/2/3 and -1; 1-5 by tile kind; GetUIElementType uses the live cursor tile for a missing or nonexistent path, -1 with none [design: `ui_more::inspection_tile`] | `nif` 4, other kinds 0 [hypothesis] |
| SortUIListBox | SortUIListBox [documented] | keys `-`, `#`, `@`; `$destination$increment` (default `_y`, `height`); numeric keys in a total order, NaN keys at the ends (`total_cmp`) [design] | which children are items (those with the destination), the first value (the smallest), the case [hypothesis each] |
| GetStringUIDimensions | GetStringUIDimensions [documented] | width, height and actual `Font::lines` count through the VM's output variables; owned metrics installed before startup/load scripts (`tiles::MenuFiles`, `script.rs`), using the renderer's localized `font_file`, font selection and wrapping [design] | unavailable metrics write three zeros; failed reads retry at most once a second on query, successful metrics are cached without GPU atlas uploads [design: NVForever policy, not documented original behavior] |
| DumpTileInfo, DumpTileInfoALL, DumpTileInfoToArray | the three SUP pages [documented] | `[ Tilename ] : name` and `[ trait ] : value` lines (the ToArray page's example output) to the console, or to NVForever's log for DumpToFile (never `SUPNVSE.log` in the game folder); ToArray a string map; DumpTileInfo uses the live cursor tile for a missing or nonexistent path (path argument 1); ALL lists menus; ToArray still requires a path [design: `ui_more`] | none |
| CacheUI, …Float, …String, …ModFloat, …GetFloat, …GetString, …Range, …RangeMod, …Erase, …Dump, …Gradual, …RangeGradual, …GradualDelay, …RangeGradualDelay, each with its Alt | the CacheUI pages (CacheUIModFloat's is `CacheUIMod`) [documented] | caches per calling plugin, Alt per key; delay pulls the value then, flags 0/1/2 and the lock, `match` ends a delay; modes 0-3, 5, 6 | mode 4 (rumble) mentioned but insufficiently specified: waveform, frequency, randomness, decay curve and final value are not given; remains refused (`ui_gradients::mode_built`); RangeMod's page and example differ in argument order, both taken [design]; the script runners' shared cache |
| CUICreateQueue, CUIGetQueue, CUIGetQueueSize, CUIQueueCycle, CUIQueueFloat | the five pages [documented] | per plugin; toggle, forced add and remove, duplicates | cycle direction 0 [hypothesis] |
| HudBar… (35 functions) | SUP HUD Bar Functions and each HudBar page [documented] | a `rect NVFHudBar<n>` with `Meter`, `Frame`, `Text`, `ImageEx` in `HUDMainMenu`, per plugin (console: first of the name), written after each change and after a load; types 0-5, value/max/percentage, text with prefix, postfix and max, alpha, colour, system colour, depth, zoom, rotation, size, indent, alt size, texture, text font and justify, auto position, bar and meter traits, all-bars arrays, element paths, debug lines to the log | live bindings, condition visibility and fades: its own issue, **HudBar live value bindings and conditional visibility**. ScriptVar needs retained variable identity (`fnv-script/src/vm.rs`, `host_function`); UDFs already have a callable representation but lack timed evaluation. Conditions and fade traits are stored, not evaluated (`ui_hudbar.rs`); reading a texture's size (`iCalculateSize`), the rotation on screen (the traits are written, the tile renderer turns no tile yet); the meter's width as indent plus the share of the rest, type 4 without a built-in `%`, the default max 100 and which elements show by default [hypothesis] |

[design] Numbers that are not numbers: SetUIFloatMultiple, AddTraitToTile, CacheUIFloat /
ModFloat / Range / RangeMod (and their Alt forms), a HUD bar's place at creation and its
element, bar, meter and text setters write 0 for a NaN or an infinity (`ui_more::finite`;
a CacheUI sum that is one too); HudBarSetValueFloat, …Max and …Percentage refuse one with
-1. The pages say nothing about such numbers; these are NVForever's own safeguards.

[design] `ui_more::inspection_tile` resolves GetUIElementType and DumpTileInfo before
flushing deferred HUD writes. The resolved tile's menu controls the flush, including a
cursor fallback from an invalid path; another menu or no live tile leaves writes pending.
DumpTileInfo retains `toFile path`, with the path at argument 1. `Pointer::active`
(`fnv-ui/src/input.rs`) rejects unloaded tiles.

Implemented consumers and remaining exclusions:

- **SetUITypewriter** [documented: [SetUITypewriter](https://geckwiki.com/index.php?title=SetUITypewriter)]
  requires authored XML linking `_TSn`/`_TAn` into text/alpha. [design: `nvse_small.rs`,
  `Game::small_tick`; `game.rs`, `Game::tick`] Letters and fades advance each frame;
  ordinary tile evaluation and drawing consume linked text/alpha (`tiles/mod.rs`,
  `tiles/draw.rs`). The command generates no XML. Ordering modes retain the hypotheses
  beside `typewriter_rank` and `typewriter_tick`.
- **SetTileScript, RemoveTileScript** [documented: [SetTileScript](https://geckwiki.com/index.php?title=SetTileScript)]
  register/replace/remove a callback per trait. [design: `nvse_small.rs`, `small_tick`]
  The callback runs each frame while the trait is nonzero and can change drawn traits;
  registration itself draws nothing. Menu mouseover/click events also have consumers
  (`ui_input.rs`, `step`), rather than a blanket menu-events exclusion.
- **SetUITexOffset, SetUIAlphaMap** [design: `nvse_itr1.rs`, dispatch] store offsets and
  alpha-map paths; the renderer has no consumers. Success does not establish texture
  loading or rendering. Drawing needs its own issue, **Render tile UV offsets and alpha
  masks**: wrapping, independent mask sampling, loading failures, canonical tile identity
  and cleanup, radial/repeated/cropped tiles and batching, with authored-texture regressions.
- **Attached components** [documented: [AttachUIComponent](https://geckwiki.com/index.php?title=AttachUIComponent)]
  require a loaded, case-sensitive model node and one parent rect under HUDMainMenu.
  [design: `ui_more.rs`, `attach`; `tiles/mod.rs`, update] Attachments remain hidden until
  placed. **Project attached UI components onto model nodes** needs its own issue for
  animated node transforms, camera projection/interface units and lifetime. Occlusion,
  clipping, distance scaling and offscreen behavior are not settled by that page.
- **HudBar live value bindings and conditional visibility** needs its own issue.
  [documented: [ScriptVar](https://geckwiki.com/index.php?title=HudBarSetValueScriptVar),
  [UDF](https://geckwiki.com/index.php?title=HudBarSetValueUDF),
  [bar traits](https://geckwiki.com/index.php?title=HudBarSetBarTrait),
  [GetVisible](https://geckwiki.com/index.php?title=HudBarGetVisible)] Values update on a
  timer with optional normalization; UDFs take no arguments and an optional caller.
  Conditions 21-27 cover movement, sneak, combat, aim, equipment, weapon and camera;
  41-44/61-64 describe fades/movement/timing for 21-26. Trait 1 concerns removal on load,
  trait 0 menu mode. GetVisible distinguishes condition-hidden -1, manually hidden 0,
  visible 1. [design: `ui_hudbar.rs`, binding/visibility dispatch] Timers, safe callbacks,
  variable lifetime, destruction/reentrancy and condition fades remain to implement.
  [hypothesis] Menu-mode polarity (the page repeats 1), contradictory combination prose,
  item/list precedence, empty lists, timer catch-up, fade easing/reversal, callback failures
  and full save persistence need explicit choices and tests, not silent assumptions.
- **CacheUI mode 4** [documented: [CacheUIRangeGradual](https://geckwiki.com/index.php?title=CacheUIRangeGradual)]
  mentions bounded, decaying rumble but is insufficiently specified: no waveform,
  frequency, randomness, decay curve or final value. [design: `ui_gradients.rs`,
  `mode_built`] It remains unimplemented; no waveform is invented.

## 10. MCM: its places and its menu scripts (tasks T7, T11; issue #976)

Described from MCM's data files (its `menus\prefabs\MCM\*.xml` and
`includes_StartMenu.xml`, version 1.5 as installed in the owner's Data folder) and one
mod's script source (FOV Slider 2.6's `FOVSliderConfigScript`); no extender or DLL code
read.

- **Where MCM lives** [observed: the files]: MCM's `start_menu.xml` replaces the
  vanilla one and includes `includes_StartMenu.xml`, which includes `MCM\MCM.xml`
  (§7 item 2). That file is one `rect` named `MCM`, hidden (`visible` false), with the
  traits the mods and MCM's own script talk through: `_ActiveMod`, `_ActiveMenu`,
  `_ActiveOption`, `_ActiveSubMenu`, `_Value`, `_NewValue`, `_Reset`, `_Default`,
  `_DefaultScale`, `_ShowScale`, `_ShowList`, `_ShowMultiScale`, `_Value1`-`_Value3` with
  their `Min`, `Max`, `Increment` and `Decimal` traits, `_MCM` (1.5, its version) and
  others. Its children, in file order: `MCM_Options`, `MCM_Scale`, `MCM_List` (from the
  includes it starts with), then `MCM_Trigger`, `MCM_ModList`, the background, brackets,
  titles and buttons, `MCM_Version` last (19 in all).
- **The rows** [observed: the files]: `MCM_Options` begins with `Option1`-`Option36`
  (hotrects, each with `_optionID` 1-36, `_title`, `_enable` and the rest from
  `OptionRect.xml`), then its arrows and scroll bar; its own `_optionID` adds up its
  rows' `_optionIDX`, which is a row's `_optionID` while the pointer is on it, so it is
  the id of the row under the pointer. `MCM_List` begins with `Item1`, `Item2`, ….
- **The shorthands' places** [hypothesis, from the above]: `GetMCMFloat child grandchild
  "setting"` and its setters count `child` and `grandchild` from 1 in file order under
  the `MCM` rect, and 0 stops at the level above: `0 0` is the `MCM` rect itself, `1 0`
  `MCM_Options`, `1 3` `Option3`. FOV Slider reads `GetMCMFloat 0 0 "_ActiveMod"` (a
  trait only the `MCM` rect has) and `GetMCMFloat 1 0 "_optionID"` (the row under the
  pointer) and sets `SetMCMFloat 0 0 "_NewValue" 0`. The GECK wiki writes `SetMCMFloat
  1 1 "_enable" 1` as `SetUIFloat "StartMenu/MCM/*:1/*:1/_enable" 1` [documented:
  SetMCMFloat], but NVForever's `*:N` counts from the last child (§5, observed on the
  HUD), which would reach `MCM_Version`, so the shorthands do not go through `*:N`
  (`fnv_game::ui_functions::mcm_trait`). Which of the two the original's `*:N` really does
  for MCM is an observer check (§8). A trait or place that is not there reads 0; the
  setters make nothing (as SetUIFloat, §5) [hypothesis].
- **A mod's menu script** [observed: FOV Slider's source]: a quest script with a
  `Begin MenuMode 1013` block (the start menu, `fnv_game::game::menus::START`). It
  returns unless `IsModLoaded "The Mod Configuration Menu.esp"`, and acts only while
  `GetMCMFloat 0 0 "_ActiveMod" == GetSelfModIndex`, i.e. while MCM shows this mod's page.
  Then, one at a time: `_Reset` 1 means fill in the page again; `_NewValue` 1 means a
  value was entered (`_Value`, for the option `_ActiveOption`), which the script takes,
  then sets `_NewValue` 0 and `_Reset` 1; `_ShowScale` 1 asks it to fill in the scale
  (it sets 2); `_DefaultScale` 1 asks for the default. At `GetGameRestarted` the mod adds
  a form to MCM's list (`ListAddForm` on MCM's form `ae6` from `GetFormFromMod`) and
  checks `_MCM`'s global (`b0d`) for version 1.5 or later.
- **GetSelfModIndex** is "the mod index of the script that calls the function"
  [documented: GetSelfModIndex]: the calling unit's plugin's place in the load order;
  0 for a script typed at the console or run as a query [observed: plugins1, 0]. MCM
  mods depend on it being their own index.
- **Built (#976, T7)**: the shorthands as above; GetSelfModIndex; a headless test
  (`crates/fnv-game/tests/ui_mcm.rs`) that builds `StartMenu` through MCM's include
  chain while the start menu is open and runs a compiled `MenuMode 1013` quest script
  written in FOV Slider's way, checking the traits it sets. The files and the script are
  written for the test; no mod data is used.
