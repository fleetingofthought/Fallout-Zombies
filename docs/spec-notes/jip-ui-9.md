# JIP LN NVSE, batch 9 (2026-11-08, #1006)

The HUD cursor and the mouse, camera and menu switches, the hotkeys' inventory
references, the Pip-Boy radio, the game volumes, fonts and interface colours, the enemy
health bar's actor, the player's immortal mode, interaction by form type, GetPosEx, and
the NIF block values (material properties; translation and rotation into script
variables). The owner's mod lists call ToggleHUDCursor (20 calls) and
SetMaterialPropertyValue (21) most of what JIP had left (xnvse-remaining.md §3).

Game side: `crates/fnv-game/src/nvse_jip9.rs` (the switches, `Jip9State`) and
`crates/fnv-game/src/nif_blocks.rs` (the NIF block values, on the scene mirror). Engine
side: `engine/nvf-play/src/script_switches.rs` (the switches reach the controls, the
engine tells the cursor, the window, the volumes and the health bar's actor) and
`engine/nvforever/src/nif_mirror.rs` (material values read from the model file, drawn
when a script changes them). Sources: the GECK wiki export, each function's page
[documented: the page named]. The probe set `jip9` (`tools/capture/jip_probes.py`, lab
`L5.ext.jip9`) ran in the original on 2026-11-08 (save nvfairport, JIP 57.30): its
answers are §4. Nothing here is saved [hypothesis: the pages say
nothing of saves].

23 functions; JIP 201 → 178.

## 1. The cursor, the mouse, the camera

| Function | NVForever |
|---|---|
| ToggleHUDCursor `[toggle]` | the state before the call, so without an argument the state [observed: hudcursor `0 0 1 0`]; on, the pointer shows over the world, mouse look stops and the HUD takes the pointer as a menu does (clicks, mouseover) [documented]. Any menu opening turns it off [documented: "switched off upon opening any game menu"]; it cannot be turned on while a menu is open [hypothesis] |
| GetCursorPos `axis [useXML]` | `X` or `Y`: the window's pixels from the top left [documented: "(0, 0) is at the top-left corner"], or (useXML) the interface's units, 960 to the window's height, the tile tree's pointer. Outside MenuMode and the HUD cursor, the system cursor's position over the window [observed: one run gave 640 / 360 (X1 853.333, Y1 480), a rerun 1278.5 / 431.94, on a 1280 by 720 window: where the mouse was]; with no cursor known (headless), the window's centre [decision]. A headless run, with no window, takes the INI's `iSize W`/`iSize H` ([Display], FalloutPrefs.ini) when the host gives INI text, else 1280 by 720 [observed: the probe install's FalloutPrefs.ini says 1280 and 720; decision: that size without an INI] |
| SetCursorPos `x y [useXML]` | the engine moves the pointer there, in MenuMode or with the HUD cursor only [documented]; it reads back at once |
| ToggleMouseMovement `[flags]` | 0 both axes off, 1 horizontal only, 2 vertical only, 3 both [documented]; the state before the call; the two low bits of the value are kept, 9 reads back 1 [observed: mouse.1 and mouse.9 `3 3 1 3`]. The mouse look drops the axes turned off; a controller's stick is not held [hypothesis: the page names the mouse] |
| ToggleVanityWheel `[toggle]` | the wheel zooms the third-person camera, or not [documented]; the state before the call [observed: vanity `1 1 0 1`] |
| ToggleCameraCollision `[toggle]` | the third-person camera stops in front of walls, or goes through them [documented: "the collision box of the player's camera"]; the state before the call [observed: camcoll `1 1 0 1`] |
| CloseActiveMenu `[closeAll]` | the open menu closes as with the player's OK (`Game::close_menu`), or every open one [documented] |

## 2. Hotkeys, radio, volumes, fonts, colours, the health bar

| Function | NVForever |
|---|---|
| GetHotkeyItemRef `key` | an inventory reference of the key's item in the player's inventory (the same eight keys GetHotkeyItem reads); 0 for an empty key or an item no longer carried [documented: "an Inventory Reference"; hypothesis: not carried] |
| SetHotkeyItemRef `key` on an inventory reference | keys 1 and 3-8 [documented: the page's values; 2 is Ammo Swap]; the player's items only [hypothesis]; 1 when set |
| GetPipboyRadio | the station's placed reference while the Pip-Boy radio plays, else 0 [documented] |
| GetGameVolume `type` | 0 master, 1 footsteps, 2 voice, 3 effects, 4 music, 5 radio, 0-100 [documented], as the engine plays them; past 5, 0 |
| SetGameVolume `type [level]` | a level past 100 changes nothing [observed: volume.set, 30 then 150 reads 30], nor one below 0 [hypothesis: likewise]; without a level the default: master 100, footsteps 50, voice 60, effects 100, music 60, radio 50 [documented: the page's "the value defined in the game's INI file"; observed: these are the original's sliders on an untouched install, `audio_out::DEFAULT_VOLUMES`]. The engine plays the new level at once; it is not written into the settings [hypothesis] |
| SetFontFile `type path` | types 1-8 and 10-89, a file under Data [documented]; 1 when the file is there [hypothesis: the page's "on success"]; GetFontFile reads it after |
| SetSystemColor `interface r g b` | 1 HUD, 2 HUD alt, 3 terminal, 4 Pip-Boy, 5 main menu [documented]; each part rounded into 0-255 [hypothesis]; the HUD's and the Pip-Boy's colours change at once |
| GetEnemyHealthTarget | the actor whose health bar the HUD shows, else none [documented], as the engine's `hud_target` decided this frame |

## 3. Immortal mode, interaction, GetPosEx, the NIF block values

| Function | NVForever |
|---|---|
| ToggleImmortalMode `[mode]` | 0 off, 1 the player's Health never below 1 HP, 2 no Health or limb damage [documented]; the mode before the call [observed: immortal.1 `0 0 1 0`]; any other value is kept and read back [observed: immortal.5 `0 0 5 0`] and holds nothing [hypothesis]. Mode 1 holds the player at 1 HP for hits and effects, with no death and no knockout; mode 2 deals hits no Health or limb damage and effects no Health damage. Script functions that set or damage the value (DamageActorValue, SetActorValue) are not held [hypothesis: the page speaks of taking damage] |
| Set/GetInteractionDisabledType `disable type` / `type` | the player cannot activate objects of the type by hand and sees no rollover text; scripted Activate still works [documented]. The type codes: GetIsUsedItemType's and RemoveAllTypedItems' (`functions::furniture`), with NPC_ 42 and CREA 43 [hypothesis: the same numbering]. SetInteractionDisabled's references (built before) now count too |
| GetPosEx `x y z` | the reference's position into the three variables [documented] |
| GetMaterialPropertyValue `block id` | the block's NiMaterialProperty: 0-2 specular R G B, 3-5 emissive R G B, 6 glossiness, 7 alpha, 8 emissive mult [documented: the value IDs]; read from the reference's model file (the emissive mult 1 in files without it); 0 for a block without one, a missing block, an ID past 8 or before the 3D is told [hypothesis: the page names no failure value] |
| SetMaterialPropertyValue `block id value` | the value reads back at once; the engine draws that shape with its own copy of its material: the specular tint, the emitted light (colour × mult), the roughness of the glossiness, the alpha [hypothesis: the spawn's own tuning of the emitted light is not redone]. This reference's only [hypothesis]. 0 [hypothesis: as the other setters, though the page names a result] |
| GetNifBlockTranslationAlt `block x y z [getWorld] [pcModel]` | 1 and the translation to the parent, or (getWorld) in the world through the nodes above and the reference's place; a missing block 0, the variables kept [documented] |
| GetNifBlockRotationAlt `block x y z [which] [pcModel]` | 0 the angles about the parent's fixed axes (GetNifBlockRotation's), 1 the same in the world, 2 about the turning axes (X, then Y, then Z) to the parent, 3 the same in the world [documented: the table]; a missing block 0 |

The NIF block values use the scene mirror (docs/design/nif-blocks.md): the tree is the
engine's, told once a script asks, so the first call on a reference reads 0. pcModel is
not read: the mirror has the drawn model. The world place turns the model by the
reference's angles as a node's are turned [hypothesis]. A block name with a `:n` suffix
the mirror gave a repeated name reads the material of its unsuffixed name in the file
[hypothesis].

## 4. The original's answers (set jip9, 2026-11-08)

34 of 47 rows were equal before this section's changes; the switches, the cursor and
SetGameVolume's range follow the original now (§1-§3). [observed] unless marked:

- The switches answer the state before the call (above).
- GetGameVolume 0-6 on the probe install: 100 50 60 100 60 50 and 0 past 5, the defaults
  as assumed; SetGameVolume 4 without a level went back to 60.
- SetFontFile 9 (no such type) and with a missing file: 0; GetFontFile 2 the game's
  `Textures\Fonts\Monofonto_Large.fnt`.
- Interaction type 28 set reads 1, type 21 stays 0.
- No hotkey held an item at the save: every key `none 00000000`, SetHotkeyItemRef not
  called (`-1 -1`); GetPipboyRadio and GetEnemyHealthTarget 0.
- The player's UpperBody material values 0-9 read 0, SetMaterialPropertyValue gave 0 and
  0.5 read back 0, a missing block 0: on the player's body the original's material
  values are 0 as here (§6, actors).
- posex.getpos: the player stood at 37000.297 -117159.648 6057.049 in the save; the
  headless run starts at the start cell's corner. Save-dependent, the code left as is.
- **Did not compile in the original** ("script did not compile"): posex, niftrans.head.0
  and .1, nifrot.head.0-3 and niftrans.missing, every row that hands GetPosEx,
  GetNifBlockTranslationAlt or GetNifBlockRotationAlt script variables declared at the
  top for its outputs. The sup3 set's `…Ex` TFC getters and GetAngleEx did the same
  (ext-sup3.md §1): the runtime compiler refuses declared script variables as output
  parameters [observed]. NVForever's compiler still takes them, as the editor does for
  mods' scripts [hypothesis: the editor's compiler takes them; mods ship them compiled].
  Whether the inline declaration (`(float fP)`) mods' script-runner files use compiles
  at run time is asked by posex.inline and nifrot.head.inline.
- niftrans.head.plain (`GetNifBlockTranslation "Bip01 Head" X`) did not compile either:
  the axis form is the Fallout 3 plugin's page; JIP's function takes getWorld and gives an
  array [documented: GetNifBlockTranslation]. NVForever's compiler took `X` in the getWorld
  parameter as an operand and compiled the row; why the original refused it (the word
  in an int parameter, or the array result in sv_Construct) is not known. The row is
  replaced by niftrans.head.array (the array's size).

### The probe questions as asked


- The switches without an argument, after one and put back; ToggleMouseMovement 9 and
  ToggleImmortalMode 5 (out-of-range values).
- GetCursorPos outside MenuMode (0?).
- GetGameVolume 0-6 on the probe install; SetGameVolume 4 150 (clamped?), SetGameVolume 4
  without a level (the INI's 60?).
- SetFontFile with type 9 and with a missing file; GetFontFile 2 after.
- The hotkeys' references against GetHotkeyItem; SetHotkeyItemRef's result on key 1 and on
  key 2.
- GetPipboyRadio and GetEnemyHealthTarget at the save.
- GetPosEx against GetPos; GetNifBlockTranslationAlt and GetNifBlockRotationAlt on the
  player's Bip01 Head (all modes, the world place against GetPos); a missing block (the
  variables kept?).
- The player's UpperBody material values 0-9, SetMaterialPropertyValue's result and the
  value read back.

## 5. Not in this batch

The text input and quantity menus (ShowTextInputMenu, SetTextInputString,
SetTextInputExtendedProps, ShowQuantityMenu) need a script-opened menu with a UDF
callback; GetMenuItemFilter and GetMenuItemListRefs the engine's inventory, container and
barter list state told to the game; the barter and recipe menu getters likewise. They are
the next UI batch.

## 6. Follow-ups

- SetFontFile: the engine loads its fonts at start; a font a script swaps is read by
  GetFontFile only until the engine reloads fonts from `Game::font_file`.
- SetSystemColor: NVForever draws the HUD's alt colour, the terminal and the main menu in
  fixed colours (`reticle.rs`, `terminal/screen.rs`, `main_menu.rs`); interfaces 2, 3 and 5
  are kept by the game (`Jip9State::colours`) for when those read it.
- The hotkeys: the engine's hotkey slots (`nvf_play::hotkeys`) and the scripts' table
  (GetHotkeyItem, SetHotkeyItem, this batch) are not one yet.
- SetMaterialPropertyValue on actors: the actor meshes are spawned by `nvf_assets::actor`,
  which the mirror's material table (read from the reference's MODL) does not cover; an
  actor's body parts read 0.
