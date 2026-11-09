# JohnnyGuitar NVSE, batch 8 (2026-11-09, #1007)

The remaining event handlers, the player's custom map marker, file hashes, loaded
archives, DDS texture headers, BMP pixels, array shuffling and sorting by editor ID, the
system colours, the sleep/wait menu's state and radio tuning, in
`crates/fnv-game/src/nvse_jg8.rs`. Sources: the GECK wiki export [documented: the page
named, in our own words]; RFC 1321 (MD5) and RFC 3174 (SHA-1); the DDS layout of
`dds.md` §1; the BMP file and info headers [documented: the format's public headers].
Not probed yet: probe set `jg8` (`tools/capture/jip_probes.py`) and lab scenario
`L5.ext.jg8` are written, not run. Tests: `crates/fnv-game/tests/jg8.rs`.

31 functions. Census with batch 7 (#1908): JohnnyGuitar 109 → 78.

## 1. Event handlers

Every one takes `set|remove function flag [filters…]` [documented]. Setting the same
function with the same filters again replaces it; `0 …` with the same function and filters
removes it. The handlers are the session's state (`OtherState::jg8`), so a loaded save
starts without them [documented: "flushed every game load"]. A filter given as a form
matches that form, a reference of that base, or a member of that form list [documented:
"Formlists are also accepted as filters"]. A number filter matches an equal number. A 0
where a form goes means no filter [hypothesis: scripts pass 0 for "any"]. A handler may take
fewer arguments than it is given.

| Function | Fired by | Arguments |
|---|---|---|
| SetJohnnyOnFailQuestEventHandler (SetOnFailQuestEventHandler) `[quest]` | a quest's first failure (FailQuest, a Fail Quest stage), through `Game::emit` | quest [documented] |
| SetJohnnyOnLimbGoneEventHandler (SetOnLimbGoneEventHandler) `[actor] [limb]` | each part newly gone in `Game::dismember` | actor, limb [documented]; the limb is the body-part location the dismemberment uses [hypothesis: the page's 0-14 list is that index] |
| SetJohnnySeenDataEventHandler (SetSeenDataEventHandler) `[cell]` | a cell the player's loaded area holds for the first time (`Game::set_loaded`) | cell [documented]; "marked as seen" is read as first loaded [hypothesis: NVForever keeps no local-map fog] |
| SetOnProcessLevelChangeEventHandler `[actor] [new level]` | an actor entering or leaving the loaded area: 3 → 0 or 0 → 3, as GetActorsByProcessingLevel reads the levels (`nvse_gaps`) | actor, old, new [documented]; levels 1 and 2 never occur [hypothesis, as there] |
| SetOnNPCActorValueChangeEventHandler `[actor] [code]` | the watched actor value functions (`functions::actor`), never for the player [documented] | actor, code, old, new; whole numbers unless flag bit 0 [documented] |
| SetOnReputationChangeEventHandler `[reputation]` | each reputation axis stored (`functions::reputation::store`) | reputation, axis (0 fame, 1 infamy), value [hypothesis: the page gives no arguments] |
| SetOnNPCResponseEventHandler `[emotion]` | each response of the player's conversation (`dialogue.rs` emit_response) | text, voice path, emotion, emotion value, response number [documented]; the path from the host's voice index (`VoiceLengths::response_path`, `fnv_voice::VoiceBank`), "" without one |
| SetJohnnyOnSleepWaitEventHandler `[mode]` | the hours of the wait menu starting to pass (`Game::begin_rest`) | 1 wait, 2 sleep; filter 0 both [documented] |
| SetOnGeneralSubtitleEventHandler `[speaker]` | a line said outside the player's conversation (Say, SayTo, greetings, `Game::say_line`) | speaker, the first response's text, topic [hypothesis: the page gives no arguments] |
| SetJohnnyOnCrosshairEventHandler `[form] [form type]` | the activation reach coming onto a reference (`nvf-play` activate.rs) | the reference [documented]; the type filter is the base's form type code (`functions::items::form_type_code`) |
| SetJohnnyOnSettingsUpdateEventHandler | `Game::jg8_settings_closed`, for the engine's settings menu | none [documented] |
| SetJohnnyOnKeyboardControllerSelectionChangeEventHandler `[menu]`, SetJohnnyOnRadioPostSoundAttachEventHandler, SetOnTakeBackItemEventHandler `[form] [form]` | kept; nothing fires them yet (follow-up) | delta; radio, mode; [hypothesis] item, count |

## 2. Other functions

| Function | NVForever |
|---|---|
| SetCustomMapMarker `x y z` | kept with the player's worldspace (or interior cell), 1; 0 with the player in no cell [hypothesis] (`Game::custom_map_marker`, for the Pip-Boy's map) |
| ClearCustomMapMarker | 1 when there was one, then none [hypothesis]. GetCustomMapMarker (`nvse_gaps`) stays a null reference: NVForever's marker is no reference [observed there: plugins2, 00000000 without one] |
| MD5File, SHA1File `path` | the hash of a file "relative to the Fallout folder" as upper-case hex [documented; observed: the case, probe set jg8], "" when it cannot be read. `Data\…` reads through NVForever's mod files (archives too), other paths the game folder, read only; no drive, leading slash or `..` |
| IsBSALoaded `name` | 1 for an archive the host loaded, by file name, any case [documented]; with `Data\` before the name, 0 [observed: probe set jg8]; the host's list (`mod_files::Roots::archives`: the engine's and fnvtool's archives) |
| GetTextureWidth, GetTextureHeight, GetTextureMipMapCount, GetTextureFormat `path` | from the DDS header of `Data\Textures\<path>`, loose or archived [documented]; format 1-5 for DXT1-DXT5, 0 for another [hypothesis]; 0 without the file |
| GetPixelFromBMP `path rOut gOut bOut x y` | a 24-bit BMP's pixel into three variables [documented]; x and y counted from the top-left [hypothesis]; 0 0 0 without the file or pixel |
| Ar_Shuffle `array` | a new list in random order (the game's random source) [documented]. The VM hands array functions it does not know to the host now |
| Ar_SortEditor `array [reverse]` | a new list of the forms by editor ID, reversed with 1 [documented]; IDs compared without case, forms without one first [hypothesis] |
| GetSystemColor `id`, GetSystemColorAlt `id rOut gOut bOut` | 1 HUD, 2 HUD alt, 3 terminal, 4 Pip-Boy, 5 main menu [documented]: SetSystemColor's colour (`nvse_jip9`), else FFB642 for 1, 4 and 5, FF432A for 2 and 21E779 for 3 [observed: probe set jg8 on the owner's install; hypothesis: these are the owner's interface colour settings, which NVForever does not read]. GetSystemColor packs 0xRRGGBB [hypothesis]; 0 for another id |
| GetSleepWaitMenuState | 0 closed, 1 wait, 2 sleep [documented]; the engine marks the sleep as it opens menu 1012 (`nvf-play` wait.rs) |
| TuneRadioRef `[station]` | kept for the radio reference [documented]; the engine's radios do not read it yet |
| IsRadioRefPlaying | 0 [decision: the page says it "is currently broken and will crash the game", so no script relies on an answer] |

## 3. Follow-ups (engine)

Fire the keyboard selection, radio sound and take-back handlers, and call
`jg8_settings_closed` when the settings menu closes after a change; draw the custom map
marker on the Pip-Boy map; let the radios read TuneRadioRef. The sound-by-path family
(PlaySoundFile, PlaySoundFromPath, PlaySound3DFromPath, PlaySoundFade, the Stop… calls,
StopSoundLooping, PlayHolotape, StopHolotape) is the next batch: it needs the engine's
audio to play files by path.

## 4. Observed (probe set jg8, 2026-11-09, the observer session)

- md5 `079F82235C010DDC16B9D5C794B76C4A|` (a missing file ""), sha1
  `FEE3C26918309D0AFCFA308B1ECCC307FD3979E3`: upper-case hex. NVForever's hex was lower
  case; now upper.
- bsa `1 1 0 0`: any case, but `Data\` before the name reads 0. NVForever took the
  prefix off; now it does not.
- colour `16758338 16728874 2221945 16758338 16758338 0`: 1, 4 and 5 FFB642, 2 FF432A,
  3 21E779, 6 nothing. NVForever answered amber for all five; now per id.
- These did not compile in the original, so they are written again: `texture` (`$sText`
  for a literal: the path is spelled out), `colour.alt` (as a statement: now `let`),
  `sorteditor` (array elements as GetFormIDString's argument: through ref variables
  first), `shuffle` (a sum of elements inside sv_Construct: summed with `let` first).
  Their answers wait for the rerun.

## 5. Probe questions (set jg8)

1. `md5`, `sha1` of `Data\Credits.txt` (answered above).
2. `bsa`: IsBSALoaded with "Fallout - Meshes.bsa", lower case, with `Data\`, and a missing name.
3. `texture`: width, height, mip count and format of `interface\icons\world map\icon_map_encampment.dds`.
4. `colour`: GetSystemColor 1-5 and 6 (packed 0xRRGGBB?), GetSystemColorAlt 1.
5. `sleepwait`: GetSleepWaitMenuState outside the menu (0).
6. `sorteditor`: Ar_SortEditor on Stimpak, Caps001 and the 10mm pistol, and reversed.
7. `shuffle`: Ar_Shuffle keeps size and elements.
