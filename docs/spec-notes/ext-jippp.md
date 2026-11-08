# JIP PP LN's functions (2026-11-01, #1019)

The ten JIP PP LN functions, built in `crates/fnv-game/src/nvse_jippp.rs`. Sources: the
GECK wiki export [documented: the page named], and probe set `jipppln` of
`tools/capture/jip_probes.py`, captured in the game (§3) [observed: jipppln, golden
`jip/jipppln.json`, private]. Rows not tagged `observed` are the wiki's, or NVForever's
own choice where the wiki says nothing.

## 1. Runtime node lists

JIP PP LN keeps, for each base form or reference, the list of its InsertNode and
AttachModel calls ("RuntimeNodes") [documented: HasRuntimeNode, CopyRuntimeNodes].
NVForever keeps the same list (`Game::jippp.nodes`): every InsertNode and AttachModel
call (`crate::nif_blocks`) adds its `"Target|Name"` spec, and removes it with modes 0 and
2 [documented: AttachFormModel's addOrRemove table; InsertNode's], with the plugin whose
script called. A spec without `|` is refused (0) and not kept. The scene mirror adds the
node only with modes 1 and 3; a remove leaves the mirror as it is (mode 0 waits for the
3D to load again; mode 2's instant removal of an inserted node is a follow-up).

| Function | NVForever |
|---|---|
| HasRuntimeNode `object query [mode]` | 2 an AttachModel entry, 1 an InsertNode entry, of exactly that spec; 0 any entry whose target path (the part before `\|`) holds the text [documented: the modes, "nodes that contain this path"; observed: jipppln, the new node's own name and `Bip01\|` read 0; hypothesis: mode 0 by default, exact case] |
| CopyRuntimeNodes `to from [mode]` | 0 replaces `to`'s list with a copy of `from`'s, 1 appends it; 1 when `to` is a form or reference [documented; hypothesis: an empty source succeeds]. A script of no plugin (the console, a runtime-compiled script) copies nothing and gives 0 [observed: jipppln; hypothesis: the reason, as ClearRuntimeNodes's] |
| ClearRuntimeNodes `object` | the calling plugin's entries dropped, the others kept; the number dropped [documented: "Nodes created by other mods are left intact", the `> 0` example]. A script of no plugin clears nothing and gives 0, even right after its own InsertNode [observed: jipppln; hypothesis: its entries count as no mod's]. A reference's inserted nodes go from its drawn tree at once [hypothesis] |
| AttachFormModel `target form mode "node\|suffix"` | `form`'s model (MODL) as an entry under `node` (the root without one), and `form`'s own entries after it, the suffix kept on them; modes 1/3 attach, 0/2 remove; mode 3 on a drawn reference attaches at once (`SceneEdit::Attach`); 1 on success [documented]. Success is the form having a model; a remove gives 1 whether or not the entry was there [observed: jipppln, a second remove gives 1 again] |

Follow-up: the engine does not yet rename attached blocks with the suffix, or rebuild a
base form's references from the list on their next 3D load.

## 2. The rest

| Function | NVForever |
|---|---|
| GetActiveEffectsInfo `[filter] [temp]` | a list of string maps, one per active effect on the actor: Form, BaseEffect, Caster, MetaData (the effect's id), Magnitude, TimeLeft, Duration, Flags [documented]. Filter 1 enchantments, 2 spells, 3 ingestibles, or a base effect / magic item form; temp 0 permanent only (abilities, worn enchantments), 1 temporary only, 2 both (default) [documented]. Flags 0, and a permanent effect's TimeLeft and Duration 0 [hypothesis: NVForever keeps no effect flags]. With nothing to list, no array (`ar_Size` −1), not an empty one [observed: jipppln] |
| SetActiveEffectsInfo `array` | each map's Magnitude, TimeLeft (at most the duration) and Duration written to the effect of that MetaData [documented]; a per-second value change scales with the magnitude [hypothesis; follow-up: a modifier already applied keeps its amount] |
| GetCalculatedWeaponDegradation `[weapon] [ammo]` | health points lost per shot: the WEAP override (DNAM flags2 bit 7) else fDamageToWeaponValue, then the ammunition's Weapon Condition Mod effects in order (combat-rules.md §17). Without arguments the actor's weapon and the ammunition it uses [documented: the examples]; with a weapon and no ammunition, the weapon's own (NAM0) [hypothesis] |
| GetControlCodeByID `control type` | type 0 the key, 255 for a control with no key [observed: jipppln, Attack]; 1 the mouse button as GetControl gives it (256 + n), −1 for none [documented: the types; observed: jipppln, −1; hypothesis: the mouse code's form]; 2 and 3 −1 [decision: NVForever binds no controller or joystick to the controls]. The game gives the controller's map for type 2 (0, 0, 4096 and 16384 for controls 0, 4, 5 and 7) [observed: jipppln], so those four lab rows differ by this decision |
| ClampToGround | on a reference, waits for the engine, which knows the ground (`Game::take_clamp_requests`) [documented: the page; follow-up: the engine does not take it yet]; 0. A base form as the calling object stops the script, as a function called on no reference does (`VmError::NoReference`, which non-strict runs do not skip either) [observed: jipppln, "script call failed"] |
| RefreshInventoryWeight | nothing [decision: NVForever reckons the carried weight at each read, `World::carried_weight`] |

## 3. The capture

[observed: jipppln, 2026-11-01] The probe game's `jip_nvse.dll` is JIP LN's and names
none of these functions, so for the run only, with both game locks held, JIP PP LN was put
in its place and JohnnyGuitar's `bJIPFixes` set to 0 (JohnnyGuitar's version check
otherwise stops the game with a message box). Afterwards JIP LN was put back and checked
by its hash, and the ini line restored. `bJIPFixes` changes JohnnyGuitar's fixes to JIP
LN's functions; it names none of these ten [hypothesis]. Every writing step clears what it
added in the same step, but ClearRuntimeNodes from the probe's own script cleared
nothing (§1), so the probe's entries stayed in that game session; nothing was saved.
31 rows came back, and one script error (ClampToGround on a base form).

Lab `L5.ext.jipppln` (other machine, 2026-11-01, before the ClampToGround fix): 27 of 31
rows equal; the four GetControlCodeByID type 2 rows differ by NVForever's decision (§2),
and NVForever gave `jpp.clamp.base` a value. Since the fix `fnvtool ext queries` lists
that step as an error, as the golden does, and the other 31 rows are as before
[observed: 2026-11-06, full data; the golden is not on this machine, so the lab was not
run again].
