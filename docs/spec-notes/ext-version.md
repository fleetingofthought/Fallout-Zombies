# Spec note: extender version and plugin checks

Covers how scripts ask which script extender and which extender plugins are present:
GetNVSEVersion, GetNVSERevision, GetNVSEBeta, IsPluginInstalled, GetPluginVersion, the
plugins' own getters (GetLNVersion/GetJIPLNVersion, GetNVSEVersionFull, …), and what
NVForever should answer.

Written for task C5 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export. Everything the pages state is tagged `[documented: <page>]`;
what they leave out is **open (to observe)**. Origins are from
`schemas/script/functions.json` [observed]. The list of known plugin names that
GetPluginVersion and IsPluginInstalled include is in §2a [documented: the GetPluginVersion page's list, pasted by the owner from geckwiki.com, 2026-10-25].

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. The extender's own version (NVSE)

| Function | Arguments | Returns | Behaviour |
|---|---|---|---|
| GetNVSEVersion | — | int | the installed extender's version number [documented: GetNVSEVersion] |
| GetNVSERevision | — | int | its revision number [documented: GetNVSERevision] |
| GetNVSEBeta | — | int | its beta number; **0 for a non-beta release** [documented: GetNVSEBeta] |

- The pages' checks: `if GetNVSEVersion < 4` (needs NVSE 4), `if GetNVSEVersion == 4 &&
  GetNVSERevision < 3` (needs 4 revision 3), `if GetNVSEBeta != 0 && GetNVSEBeta < 3`
  (needs beta 3) [documented: each page's example]. The NVSE Expressions page's example
  prints `$(GetNVSEVersion)` as `4` [documented: NVSE Expressions], an old example.
- **A version check only works in a script that uses nothing else from the extender**:
  without the extender, a script using any extender feature fails before the check runs
  [documented: GetNVSEVersion, "Usage Warning"].
- How xNVSE 6.4.9 splits into these three numbers (6 / 4 / 9?): open [hypothesis: the
  three getters map to the three parts of the dotted version]. A probe printing the three
  under the owner's xNVSE settles it.

## 2. Extender plugins

| Function | Arguments | Returns | Behaviour |
|---|---|---|---|
| IsPluginInstalled | plugin name (string) | bool | 1 when that NVSE plugin is loaded [documented: IsPluginInstalled] |
| GetPluginVersion | plugin name (string) | float | the plugin's version, **-1 when it is not loaded** [documented: GetPluginVersion] |

- **The name is the one the plugin registers** (shown in `nvse.log`), **not its file
  name** [documented: IsPluginInstalled; GetPluginVersion].
- **The number is usually the version times 100** (ShowOff 1.80 → 180), **except JIP LN,
  which returns its real version as a float** (57.30) [documented: GetPluginVersion].
- Matching of the name (case, spaces): open.

**Plugin names the pages give** (exact strings):

| Plugin | Name to test | Evidence |
|---|---|---|
| JIP LN NVSE | `"JIP NVSE Plugin"` | [documented: GetPluginVersion; IsPluginInstalled] |
| JohnnyGuitar NVSE | `"JohnnyGuitarNVSE"` | [documented: IsPluginInstalled] |
| SUP NVSE | `"SUP NVSE Plugin"` | [documented: IsPluginInstalled; GetSUPVersion] |
| MCM | `"MCM Extensions"` | [documented: IsPluginInstalled] |
| NX (New Vegas Extender) | `"Extensions for NVSE"` | [documented: IsPluginInstalled] |
| ShowOff NVSE | `"ShowOffNVSE Plugin"` | [documented: GetPluginVersion; Category:Functions (ShowOff NVSE)] |
| JIP PP LN | `"JIP PP LN"` | [documented: AttachModel and the other JIP PP LN pages] |
| CacheUI | `"CUI"` | [documented: Category:Functions (CacheUI Plugin)] |
| hNVSE | `hNVSE` (unquoted on its page; `"hNVSE"` in §2a) | [documented: Category:Functions (hNVSE)] |

Two names are only on category pages, not in the list below: `"JIP PP LN"` and `"CUI"`.

## 2a. Known plugin names

The wiki's list of plugins and the name each registers [documented: the GetPluginVersion page's list, pasted by the owner from geckwiki.com, 2026-10-25]. These are
the strings mods pass to IsPluginInstalled and GetPluginVersion; C5 keys its answers on
them.

| Plugin | Registered name | Note |
|---|---|---|
| Aim Transition Patch | `"AimTransitionFix"` |  |
| AnhNVSE | `"AnhNVSE"` |  |
| ARES_NVSE | `"ARESPlugin"` |  |
| Book Menu Restored Plugin | `"Book Menu Restored"` |  |
| Charisma Tweaks | `"Charisma Overhaul"` |  |
| Climate Control NVSE | `"ClimateControl"` |  |
| Console Paste Support | `"Console Clipboard"` |  |
| Controller Support NVSE | `"Controller Support"` |  |
| Crash Logger | `"CobbCrashLogger"` |  |
| Cursor Unilaterally Matched | `"CUM"` |  |
| Fog-Based Object Culling | `"FogCulling"` |  |
| GaryGaryGary | `"GAAARY"` |  |
| GECK Extender | `"ZeGaryHax"` | cannot be tested with IsPluginInstalled or GetPluginVersion |
| hNVSE | `"hNVSE"` |  |
| Height Randomizer | `"HeightRandomizer"` |  |
| High Res Local Maps | `"HighResLocalMaps"` |  |
| High Resolution Bloom | `"HighResBloom"` |  |
| High Resolution Screens | `"HDPipboy"` |  |
| High Resolution Water Fog | `"HighResWaterDepth"` |  |
| Hot Reload (NVSE) | `"hot_reload"` | see also GetGameHotReloaded |
| Improved Console (NVSE) | `"Improved Console"` |  |
| Improved Lighting Shaders | `"Improved Lighting Shaders for FNV"` |  |
| JIP LN NVSE | `"JIP NVSE Plugin"` |  |
| JIP Realistic Weapon Overheating | `"RWO Plugin"` |  |
| JohnnyGuitar NVSE | `"JohnnyGuitarNVSE"` |  |
| kNVSE Animation Plugin | `"kNVSE"` |  |
| kNVSE JSON Generator | `"MyFirstPlugin"` |  |
| LStewieAl's Tweaks | `"lStewieAl's Tweaks"` |  |
| Manual Reload | `"Manual Reload"` |  |
| MCM | `"MCM Extensions"` |  |
| FNV Mod Limit Fix | `"MLF"` |  |
| MoonlightNVSE | `"MoonlightNVSE"` |  |
| NVAC - New Vegas Anti Crash | `"NVAC - New Vegas Anti Crash"` |  |
| New Vegas - Enhanced Camera | `"EnhancedCamera"` |  |
| New Vegas Reloaded | `"NewVegasReloaded"` |  |
| New Vegas Stutter Remover | `"sr_New_Vegas_Stutter_Remover"` |  |
| No Skill Tags | `"NO TAGS REDONE"` |  |
| NVTF - New Vegas Tick Fix | `"NVTF"` |  |
| NX | `"Extensions for NVSE"` |  |
| OneTweak | `"OneTweak"` |  |
| Pip-Boy Shading Fix | `"PipBoyShadingFix"` |  |
| Plugins+ | `"Plugins+"` |  |
| Project Nevada | `"pn_nvse"` |  |
| FNV ReShade Helper | `"FNVReShadeHelper"` |  |
| ROOG FNV - ROOG TTW | `"Hard Hardcore"` |  |
| ROOGNVSE Plugin | `"ttw_nvse"` |  |
| ShowOff NVSE | `"ShowOffNVSE Plugin"` |  |
| Simple Exterior Fog Remover | `"ExteriorFogRemover"` |  |
| SUP NVSE | `"SUP NVSE Plugin"` |  |
| SUPDate Checker | `"SUPDate Checker"` |  |
| UIO - User Interface Organizer | `"UI Organizer Plugin"` |  |
| YAPR - Yet Another Pip-Boy Remover | `"PipBoyTweaks"` |  |
| yGTM - Gameplay Tweaks and Madness | `"yGTM"` |  |
| yUI - User Interface | `"yUI"` |  |

## 3. Plugins' own version getters

| Function | Origin | Returns | Behaviour |
|---|---|---|---|
| GetLNVersion (alias GetJIPLNVersion) | LU | int | **always 0 now**; deprecated since NVSE 4.5.7 in favour of GetPluginVersion [documented: GetLNVersion] |
| GetNVSEVersionFull | JIP | float | the full extender version (version, revision and beta) as one float, e.g. `>= 5.02`; registered on an unused base-game opcode, so it returns 0 without NVSE and JIP and works as an NVSE check; broken before JIP 57.05 [documented: GetNVSEVersionFull] |
| GetPluginHeaderVersion | JIP | float | a plugin file's header version, e.g. of FalloutNV.esm [documented: GetPluginHeaderVersion] |
| GetSUPVersion | SUP | -1 always | prints SUP's version to the console; not a check [documented: GetSUPVersion] |
| GetNVSEOnlineVersion, GetNexusModVersion… | SUP | — | fetch versions from the internet; out of scope (NVForever does not reach the network for scripts) [documented: their pages] |
| IsDLLLoaded | JohnnyGuitar | bool | whether a named DLL is loaded into the game [documented: IsDLLLoaded] |

No "IsScriptExtenderLoaded"-style function is in the export; GetNVSEVersionFull is the
documented way to test for the extender from a script that must also run without it.

## 4. How the wiki's examples check versions (survey)

Counted over the private export [observed: every page's text]:

| Check | Mentions |
|---|---|
| GetPluginVersion "…" | 24 (mostly "use GetPluginVersion "X"" lines on plugin category pages) |
| GetNVSEVersion | 15 |
| IsPluginInstalled "…" | 9 |
| GetNVSEBeta | 5 |
| GetNVSERevision | 4 |
| GetLNVersion / GetJIPLNVersion | 1 each (its own deprecation page) |

So mods mostly gate on **GetPluginVersion with a plugin name** and on **GetNVSEVersion**
(compared with 4, the version that brought let, arrays and strings). Examples compare
with "at least" (`<` → warn), so reporting a version that is too high is safer than too
low.

## 5. What NVForever answers (for C5)

- **GetNVSEVersion / Revision / Beta**: xNVSE 6.4.9 [decision: the owner, 2026-10-26],
  split as §1 settles (the probe is asked for).
- **IsPluginInstalled / GetPluginVersion**: report a plugin as loaded only when NVForever
  implements that plugin's functions (C2's per-plugin modules), with the version that
  module claims; otherwise 0 and -1 [decision: claiming a plugin whose functions are
  missing would let mods take paths that then fail]. The names in §2 are the keys.
  Reported (`fnv_script::vm::jip::PLUGINS`): JIP LN 57.3, ShowOff 184, kNVSE 37,
  JohnnyGuitar 528, MCM Extensions 1 [observed: the owner's install], and since
  2026-11-08 `"lStewieAl's Tweaks"` 941 and `"ttw_nvse"` 1 [design, owner: "technically
  the dll is present, just in engine"]: Tale of Two Wastelands' start-up checks
  (`TTWGenericHandlerScript`, TTW 3.4) show a "missing" message for Stewie's Tweaks
  below 941 and for `IsPluginInstalled ttw_nvse` 0, while NVForever runs the functions
  TTW calls from both (`TTW_*`, StewieGetValue, `fnv_game::nvse_small`). Neither plugin
  is on the owner's install, so neither version was read: 941 is the lowest TTW accepts,
  and TTW never asks ttw_nvse's. Most of Stewie's tweaks are not built (ext-small.md).
- **GetIsLAA** (the 4 GB patch): 1 [design, 2026-11-08: NVForever is a 64-bit program with
  no 2 GB limit; the unpatched original answers 0, observed in the gaps4 set; TTW's check
  showed "4GB patch missing"].
- Left as it is: TTW's `GetNumericIniSetting "bBackgroundLoadLipFiles:BackgroundLoad"`
  check reads the game's INI as NVForever reads it; its message ("bad INI") shows only
  when that setting is off or missing there.
- **GetNVSEVersionFull**: a JIP function; answer only once JIP's module exists.
- **GetLNVersion**: 0, as documented.

## 6. Open (to observe)

The version/revision/beta split of 6.4.9; GetPluginVersion's name matching (case,
spaces); plugins newer than the wiki's list.
