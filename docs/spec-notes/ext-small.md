# The small extender plugins, CacheUI's and Anh's last (2026-11-01)

Built in `crates/fnv-game/src/nvse_small.rs` (ArrayToString in the VM,
`crates/fnv-script/src/vm/anh.rs`): the plugins of #1020 (BookMenu, ButcherPete,
ClimateControl, Command Extender, HotReload, Overcharge, PostalOut, TTW, Stewie's Tweaks,
hNVSE, yUI, and the catalogue's "Sandbox" row), CacheUI's last eight (#1013), Anh's last
three (#1015, ext-anh.md) and NX_GetQVEVFL (#1017, ext-nx.md). Sources: the GECK wiki
export [documented: the page named]. None of these plugins is installed in the probe
game, so nothing here is `[observed]`. Changes last for the session.

## 1. Anh and NX

| Function | NVForever |
|---|---|
| ArrayToString `array` | PrintVar's text of the array (`[1, 2]`, a map's `key: value`) [documented: "Returns the specified array variable as a string"; decision: the form, the page shows none]; "" for no array [hypothesis] |
| GetCameraFOV `1stPerson` | the last SetCameraFOV's value, else the INI's fDefaultWorldFOV / fDefault1stPersonFOV ([Display]), else 75 / 55 [documented: "Returns the FOV value for the camera"; hypothesis: the defaults] |
| UpdateTubeMesh (QuitMeshinWithMeAndUpdateAlready) | 0 [decision: NVForever builds no tube mesh] |
| NX_GetQVEVFL | 0 [decision: the page gives no arguments and leaves its use to an outside tutorial] |

## 2. CacheUI

| Function | NVForever |
|---|---|
| GetFPS | 1 / the last frame's seconds [documented: "Not 100% Accurate"] |
| SetTileScript `path udf`, RemoveTileScript `path` | the UDF runs each frame while the trait is not 0; one per trait [documented] |
| SetFrameControllerLoop `udf set [time] [frames]` (SFCL) | registered or removed; every `time` seconds the call waits `frames` frames, the timer starting again at once [documented]; real frame seconds, menus included; one loop per UDF [hypothesis] |
| SetUITypewriter `path text time [fade] [mode] [invert]` | the letters into `_TS0…` at once (at most 260); the alphas `_TA0…` from 0 to 255, letter `i` starting at its place's share of the duration and taking `fade`; modes 0/1 from the left, 2/3 from the middle, 5/6 shuffled; 1, 3 and 6 go out again; invert mirrors the order (5/6: fades out instead); no duration stops and sets the alphas to 0 [documented: the mode tables, "the entire duration … is always duration + fade"]. The out modes come in over the first half and go out over the second, and the shuffle has a fixed seed [hypothesis; decision]. The clock is real seconds, menus included: each frame's own length added up (`SmallState::real_seconds`), as the engine gives every frame its own `frame_seconds` [decision] |
| SetFOVRatio `current strength` | kept for the aiming field of view [documented: the formula; follow-up: the engine's camera does not read it] |
| SplitStringToRef `text symbol` | the parts by editor ID, `player`, or `Plugin.esm:hex`, in order; a part that names nothing is left out [documented; hypothesis: left out] |
| WriteToXML | 0, nothing written [decision: NVForever never writes into the game folder, and the change only shows after a restart] |

## 3. The small plugins

| Function | NVForever |
|---|---|
| Sandbox | 0 [documented: "This is NOT a function"; it is in the catalogue as a page] |
| ShowBookMenu `[showTake] [book]` | on a reference, `Activate player` [documented: "behaves similarly to 'bookref.activate player'"; decision: no Take choice, the engine's book screen has none] |
| GetButcherPeteVersion | 0 [documented: "if the mod isn't installed it will simply return 0"] |
| GetLockedAlt | a terminal: 0 open (no lock data, a script unlocked it, hacked), 2 locked out (a failed hack, a Requires Key level), else 1; anything else as GetLocked [documented: the table; hypothesis: the password note is not checked] |
| GetQuestFailedAlt `quest` | GetQuestFailed [documented] |
| IsKeyPressedAlt `code` | IsKeyPressed counting disabled keys [documented] |
| GetOutgoingWeather | the weather the sky showed before the one it shows (`WeatherState::outgoing`, set when the engine tells a change) [documented: "Returns the outgoing weather"; hypothesis: kept after the change ends] |
| FakeQuantityMenu | nothing [documented: "for Fallout 3 only"] |
| GetCommandExtenderVersion, GetRockItAmmoCount | 0 [decision: a Fallout 3 plugin; New Vegas has no Rock-It Launcher] |
| GetSelectedItemBaseRef | GetSelectedItemRef's base form [documented] |
| PrintActiveUIComponentFullName | GetActiveUIComponentFullName to the console [documented] |
| ToggleContainerMenuItemTransfer, ToggleItemQuantitySuffix `state`; ToggleMuzzleFlashLights `[state]` | kept (1 off); the last switches without a state [documented; hypothesis: 1 means off; follow-up: the engine's menus and lights do not read them] |
| GetGameHotReloaded, ToGECK | 0 [decision: NVForever does not hot-reload scripts and has no GECK beside it] |
| GetOCSettings / SetOCSettings `trait [value]` | what SetOCSettings put (traits 1-12), else 0 [decision: Overcharge's files are not read] |
| GetOCWeaponConfig | 0 [decision: the same] |
| GetOCWeaponState / SetOCWeaponState `weapon trait [value]` | by actor, weapon and trait (1-19); the setter returns the value [hypothesis] |
| CreateOCLightClone `light actor` | a TempCloneForm copy of the light [documented: "Returns with the cloned light form"; decision: its colour stays, no Overcharge heat is kept] |
| IsPlayerIdlePlaying `idle` | the player plays the idle (one idle per actor, both views) [documented] |
| TTW_CRC32 `plugin` | the CRC-32 (IEEE) of the plugin file; 0 when it cannot be read [documented; decision: the variant]. Table-driven, and kept per plugin name for the session (a plugin does not change while the game runs; FalloutNV.esm is too large to read on every call) [decision] |
| TTW_GetEquippedWeaponSkill `skill` | 1 when the equipped weapon's skill (DNAM, an actor value code) is that one [documented] |
| TTW_GetPlugins | `[name, version, description]` per loaded plugin [documented]: the version is the TES4 HEDR version (as GetPluginHeaderVersion), the description "" [decision]. Until 2026-11-08 the version was 0, and TTW's start-up check (a plugin below 1.3 is an unconverted Fallout 3 mod) showed its "Fallout 3 mods installed" message for every load order |
| TTW_ShowGeneProjector | opens the race menu (1036), NVForever's character creator, as ShowRaceMenu does [observed: TTW's scripts wait in `MenuMode 1036` after it; design: owner 2026-11-02] |
| StewieGetValue `"key:section"` (StewVal) | the value in `Data\NVSE\Plugins\nvse_stewie_tweaks.ini`, a number when it reads as one; 0 without [documented; hypothesis: the file's name] |
| GetCellRegions `cell` | an exterior cell's regions (XCLR); none for an interior [documented] |
| SetWorldspaceMapBounds `world nwX nwY seX seY` | the WRLD MNAM cells, read by every reader of the record (`Game::world_map_bounds`) [documented; follow-up: the engine's map reads its bounds once] |
| ySIGetTrait / ySISetTrait `"tag"\|"icon" [value] [form]` | ySI's tag and icon, of the form else the calling reference's base; an icon set on a tagged form goes to every form of that tag [documented]; "" until set [decision: ySI's own files are not read] |
