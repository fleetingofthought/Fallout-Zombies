# JohnnyGuitar NVSE, batch 7 (2026-11-08, #1007)

The switches and settings JohnnyGuitar gives scripts over the game's interface, music,
saves, camera and HUD, the queued screen texts, the colour conversions into script
variables, JohnnyGuitar.ini's patches and nvse_config.ini's heap size, in
`crates/fnv-game/src/nvse_jg7.rs`. Sources: the GECK wiki export [documented: the page
named, in our own words]; the original game through probe set `jg7`
(`tools/capture/jip_probes.py`, lab scenario `L5.ext.jg7`) [observed: "Observed
2026-11-08" at the end]. Tests: `crates/fnv-game/tests/jg7.rs`.

38 functions. The game keeps each value for the session (`OtherState::jg7`, not saved: a
loaded save starts with every switch at its default, as ToggleLevelUpMenu and
ToggleDisableSaves document [hypothesis for the others]). ToggleCombatMusic alone lasts
for the process.

## Functions

| Function | NVForever |
|---|---|
| ToggleCombatMusic `0/1`, IsCombatMusicEnabled | off or on "until the game is restarted" [documented], so for the process, not cleared by a load. The engine's music starts no battle music while it is off (`nvf-play` music.rs) |
| ToggleLevelUpMenu `0/1`, IsLevelUpMenuEnabled | the switch; loading a save turns it back on [documented]. The level-up menu does not read it yet (follow-up) |
| DisableMenuArrowKeys, EnableMenuArrowKeys | the switch [documented: scripts still see the keys]; the menus do not read it yet (follow-up) |
| SetYieldTimer `seconds`, GetYieldTimer | counts down from the value set, in game seconds (`Game::time`), 0 when run out or never set [documented: a positive value keeps the player from yielding; hypothesis: game time, and that it counts down]. NVForever's combat has no yielding yet, so nothing else reads it |
| SetViewmodelClipDistance `distance`, GetViewmodelClipDistance | the value, 0 the default ("resets the value") [documented]; below 0 reads 0 [hypothesis]. The first-person camera does not read it yet |
| SetNiLODLevel `level`, GetNiLODLevel | the global LOD level override for NiLODNodes [documented], -1 before any SetNiLODLevel [observed 2026-11-08]. The renderer does not read it yet |
| ToggleDisableSaves `0/1` | per plugin: the load-order index (high byte) of the calling script's quest or reference names the plugin; saving waits until every plugin that disabled it calls 0, or a save is loaded [documented: "stored on a per-mod basis"]. A query or console script with neither counts as one plugin (0xFF) [documented: "the console … share one mod index"]. The engine's saves (`nvf-play` save.rs) refuse with "a script disabled saving"; ForceSave and SystemSave wait with them [documented] |
| SetUIUpdateSound `sound type` | type 1 quest failed, 2 quest added, 3 quest completed, 4 location discovered [documented]; another type or a form that is no SOUN changes nothing [hypothesis]. The corner messages do not play it yet |
| SetCustomReputationChangeIcon `reputation tier path` | 1 when kept [documented: returns success]; a tier outside 1-4 or a form that is no REPU gives 0 [hypothesis: the tiers]. The corner message does not show it yet |
| SetDisablePlayerControlsHUDVisibilityFlags `flags` | the bit mask kept [documented]; the HUD does not read it yet |
| ToggleNthPipboyLight `button on` | 0 Stats, 1 Items, 2 Data [documented]; the Pip-Boy does not draw them yet |
| PushUIQuestToTop `quest` | the quest kept (a QUST only) [documented]; the Pip-Boy's quest list does not read it yet |
| HideItemBarterEx `item hide [side] [vendor]`, IsItemBarterHiddenEx `item [vendor]` | hide 0 shows the item again; side 1 keeps it shown on the player's side, 2 on the vendor's, none hides it on both [documented: HideItemBarterEx and the reader's notes, 0 shown … 3 hidden on both]; kept for the barter menu as that flag (`Game::barter_hide_flag`; with a vendor, its own entry first, then the one for every vendor [hypothesis]). IsItemBarterHiddenEx reads 0: in the original it read 0 after every HideItemBarterEx, on the caps and on the 10mm pistol, with and without a vendor [observed 2026-11-08: jg7.barter, jg7.barter.pistol, outside the barter menu]. The barter menu does not read the flag yet (follow-up) |
| QueueCinematicText `title subtitle [sound] [priority] [justification] [title font] [subtitle font]` | queued: priority 0 appends, 1 prepends, 2 clears the queue and shows at once; justification 0 left, 1 centre, 2 right; fonts are the [Fonts] slots [documented]. The HUD does not show the queue yet |
| QueueObjectiveText `text [completed] [multiple]` | queued [documented: QueueObjectiveText]; the HUD does not show it yet |
| TogglePipBoy `[tab]` | closes the Pip-Boy when it is open, else opens it on 1002 Inventory, 1003 Stats, 1023 Data [documented]; without a tab on the inventory [hypothesis: the page's "tab it closed on" is not kept by NVForever] |
| ExitGameAlt, RollCredits, StopVATSCam, SetCameraShake `multiplier seconds`, SetHUDShudderPower `power` | requests for the engine (`Game::take_jg7_requests`) [documented: each page's summary]; no engine system takes them yet (follow-up) |
| SetCustomMapMarkerIcon `marker|list path`, GetCustomMapMarkerIcon | the path for the marker reference, or for each member of a form list [documented]; the getter reads it on the calling marker. Without one, the marker type's icon file, `Interface\Icons\World Map\icon_map_<name>.dds` with the names of the world map's table (`MAP_MARKER_ICONS`, shared with the engine) [documented: "the default texture path"; observed 2026-11-08: the Goodsprings marker (type 3) read icon_map_encampment.dds in that spelling]; "" for a reference that is no map marker [hypothesis]. The map does not read the paths yet |
| HSVtoRGB `h s v rOut gOut bOut`, RGBtoHSV `r g b hOut sOut vOut` | hue 0-360 degrees, saturation and value 0-1, channels 0-255, through script variables [documented]; the standard hexcone conversion, HSVtoRGB rounded to whole channels [hypothesis: probed as jg7.hsv.*, jg7.rgb.*]. The three outputs go to their variables through the VM's out-parameter table (`fnv_script::vm::out_params`) |
| GetJohnnyPatch `id` | 1 for id 1 (bLoadEditorIDs, "no longer optional") [documented]; ids 3-8 read bFixFleeing, bFixItemStackCount, bFixNPCShootingAngle, bNoMuzzleFlashCooldown, bReset3rdPersonCamera and bJIPFixes from `Data\NVSE\Plugins\JohnnyGuitar.ini`, section MAIN [documented: the names; hypothesis: the file's place and section]; 0 without the file or for another id |
| GetDefaultHeapSize | DefaultHeapInitialAllocMB from `Data\NVSE\nvse_config.ini`, any section [documented]; 0 without it [hypothesis]. Read only, through `mod_files::data_dir` |
| GetLocationSpecificLoadScreensOnly | the switch SetLocationSpecificLoadScreensOnly sets [documented] |
| IsMenuPaused `[menu]` | NVForever has no Real Time Menus, so 1 for any menu code, and without one 1 in menu mode [documented: "if Real Time Menus is not installed"] |
| AsmBreak | nothing [decision: NVForever runs no native debugger hooks for scripts] |
| RefAddr | 0 [decision: NVForever's forms have no memory addresses a script could use] |

## Nothing values

AsmBreak and RefAddr, for the reasons above.

## Follow-ups (engine)

Read by the engine in this batch: ToggleCombatMusic (music), ToggleDisableSaves (saves),
TogglePipBoy (the menus). Still to read, each a change of its own system: the level-up
menu switch, the menu arrow keys, the yield timer (no yielding yet), the viewmodel clip
distance, the NiLOD override, the UI update sounds, the reputation and map marker icons,
the HUD visibility flags under DisablePlayerControls, the Pip-Boy button lights, the top
quest, barter hiding (`menu.rs` vendor_sells and the barter rows), the queued cinematic and
objective texts, and the requests (exit to the main menu, credits, the VATS camera, the
camera shake, the HUD shudder).

## Observed 2026-11-08

The first run in the original (save nvfairport, JohnnyGuitar 5.28, xNVSE 6.4.9): 6 of the 8
rows that compiled matched; 9 of 17 probes did not compile there. Fixed since: IsItemBarterHiddenEx (always 0; the
rerun with the 10mm pistol read 0 too) and the marker's default icon (above), GetNiLODLevel's default -1. The music,
levelup, yield, lod and menupaused probes wrote a zero-argument command bare as an
sv_Construct argument, which the original's compiler refuses; they now put it in
parentheses (diagnostic forms read IsCombatMusicEnabled 1, IsLevelUpMenuEnabled 1,
GetYieldTimer 0, IsMenuPaused 0 outside menus and 1 with 1002, as NVForever). HSVtoRGB and
RGBtoHSV compiled at run time in none of seven forms, so their conversion stays a
hypothesis until a plugin-compiled script probes it. NVForever's compiler takes all nine
forms the original refuses (a follow-up for the compiler, not this batch).

The rerun (head de7253958): 13 of 14 rows equal, the 10mm pistol's barter row read 0 too,
so IsItemBarterHiddenEx now reads 0; lab `L5.ext.jg7` 14 of 14 against the private golden
`jip/jg7.json`. HSVtoRGB and RGBtoHSV are out of the runtime set and wait for a
plugin-compiled probe (observer-checks).
