# JohnnyGuitar NVSE, batch 6 (2026-11-08, #1007)

Actor values by code, actors' states and temporary effects, perks, karma, cells (visited,
expired, lighting templates), the moon, radios and talking activators, references'
activation prompts and primitives, effect shaders, mines, and the per-reference calls, in
`crates/fnv-game/src/nvse_jg6.rs`. Sources: the GECK wiki export [documented: the page
named]; the record layouts from `schemas/fnv` (xEdit wbDefinitionsFNV.pas); the base
game's records read with `fnvtool esm inventory` and `decode` [observed]. Probe set `jg6`
(`tools/capture/jip_probes.py`) ran in the original on 2026-11-08 (save nvfairport,
JohnnyGuitar 5.28, xNVSE 6.4.9; private golden `jg6`): 24 of its 28 rows equal after the
fixes below, the other 4 hang on the save or have no rule yet ("Observed 2026-11-08" at
the end). Lab scenario `L5.ext.jg6`. Tests: `crates/fnv-game/tests/jg6.rs`.

| Function | NVForever |
|---|---|
| DamageActorValueAlt, ForceActorValueAlt, ModActorValueAlt, RestoreActorValueAlt, SetActorValueAlt `code value` (and their …AVAlt aliases) | the game's own DamageAV, ForceAV, ModAV, RestoreAV and SetAV with a numeric actor value code [documented: each page's summary and example, e.g. ModActorValueAlt 5 3 raising Strength by 3]; JohnnyGuitar's actor value change handlers see the change as for the plain function. A code past the actor value table changes nothing [hypothesis] |
| UnforceAV `av` | nothing [documented: "currently broken and does nothing"] |
| UnsetAV `av` | the base goes back to what it was before the first SetAV on that actor value; the modifiers and the damage stay; nothing without a SetAV [documented: UnsetAV; hypothesis: "its previous state" read as the base SetAV changed, the first one kept] |
| GetActorValueModifierAlt (GetAVModAlt) `av [filter]` | the modifier the actor's active effects hold ("gained from Effects"): 1 the permanent one (an ability's), 2 the temporary one, 3 or none both [documented]. ModAV's change is in none of them, and another filter reads 0 [observed 2026-11-08: ModActorValueAlt Luck 1 on the player read 0 with filters 1, 2, 3, 4 and none] |
| RewardKarmaAlt `value` | RewardKarma's change without the corner message: the karma note for the engine is dropped [documented] |
| GetLifeState | 0 alive, 2 dead, 3 unconscious (SetUnconscious or knocked down), 5 restrained, 6 an essential actor knocked down at no health [documented: the codes; hypothesis: which knock-downs read 6]. 1 (dying) is never read: NVForever's actors die at once [hypothesis: no dying timer] |
| IsInDialogueWithPlayer | 1 for the speaker of the player's conversation [documented] |
| GetCurrentFurnitureRef | the furniture reference the actor uses, 0 for none [documented; hypothesis: the 0]. Registered with no return type (the game's nvse.log lists it under RegisterCommand, the form getters that compile into GetFormIDString under RegTypedCommand), so `GetFormIDString (rP.GetCurrentFurnitureRef)` did not compile in the original [observed 2026-11-08]; the probe now goes through `set rObj to …`. NVForever's compiler does not check extender argument types and compiles the first form |
| HasHealthDamageEffect | an active effect on the actor whose value change lowers Health [documented; hypothesis: what counts as health-damaging] |
| GetTempIngestibleEffects | the player's ingestibles (ALCH) with effects running, each `[item, seconds left, duration]` of its effect with the longest duration [documented], in the order of their first effect [hypothesis] |
| ModNthTempEffectTimeLeft `index modifier` | GetTempEffects' Nth entry (`nvse_jip3`): its elapsed time goes down by `modifier`; 1 when there is such an entry, else 0 [documented: "modifies time elapsed"] |
| GetAvailablePerks | the perks the level-up menu offers now: the perks page's bright rows, in its order (`levelup::PerkSheet`) [documented] |
| GetHotkeySlot | the hotkey (1-8) the inventory reference's item is on, 0 for none [documented], over the hotkey table of SetHotkeyItem (`nvse_other`) |
| SendTrespassAlarmAlt | the trespass alarm for the owner of the calling reference (any reference): each witness who stands for that owner counts the trespass and turns on the player (`Game::trespass_alarm_for`) [documented: "uses the calling reference's owner"; hypothesis: the witnesses are the ones told]. Nothing for a reference no one else owns |
| IsCellVisited `cell` | 1 when the player's loaded area has ever held the cell [documented; hypothesis: every cell loaded around the player counts]. Kept in `FormChanges::visited_cells` (saved). Doc Mitchell's house read 1 and the Goodsprings schoolhouse 0 in the test save [observed 2026-11-08; the first is save-dependent: a run from the plugins alone has visited neither] |
| IsCellExpired `cell` | -1 not visited or the player's own cell, 1 after ResetInterior or when left for iHoursToRespawnCell game hours (not in a zone that never resets), else 0 [documented; the timing: `crate::respawn`]. The test save read 1 for Doc's house (left days before), -1 for the schoolhouse and for the player's own cell [observed 2026-11-08; the first is save-dependent] |
| GetLightingTemplateCell, SetLightingTemplateCell `template [cell]` | the cell the setter gave the template, 0 before; no cell clears it [documented; hypothesis: a lighting template record holds DATA and DALC only (schema), so the link is the session's]. A form that is not an LGTM or a cell changes nothing. **Open**: in the original the getter read 0 for NovacInterior and RepconnFacility, and still 0 after `SetLightingTemplateCell` NovacInterior to GSDocMitchellHouse [observed 2026-11-08: jg6.ltmp.set]. No rule found (a setter that does nothing, a getter that reads something else, or another argument order), so NVForever keeps the documented link |
| GetMoonPhase | 0 full … 4 new … 7 waxing gibbous [documented]: the days passed over the climate's phase length (TNAM, low six bits) as the sky draws it (`fnv_world::sky::moon_phase`); the climate of the player's worldspace, phase 0 indoors [hypothesis]. The test save read 5 [observed 2026-11-08; save-dependent: it hangs on the save's days passed, a run from the plugins alone starts on day 0] |
| GameGetSecondsPassed (GGetSecPass) | the frame step [documented: "the time elapsed since the game's last frame"] |
| GetBufferedCellsAlt `interiors` | no array, for 0 and 1 alike [observed 2026-11-08: ar_Size -1 for both, the player outdoors at the airport, JohnnyGuitar 5.28]. The page describes the buffered cells and calls the function deprecated for JIP's GetBufferedCells [documented] |
| GetAllGameRadios | the placed references of every talking activator with the Radio Station header flag (0x00020000, radio.md §2), by FormID [documented] |
| GetTalkingActivatorActor `activator` | the actor SetTalkingActivatorActor gave the reference (`crate::talking_activators`); a base form reads its first placed reference with one, by FormID [hypothesis]; 0 for none |
| GetRefActivationPromptOverride, SetRefActivationPromptOverride `[prompt]` | the reference's XATO, "" for none; the setter keeps the new prompt for the session (`nvse_jg6::activation_prompt`) [documented]. A string variable as the prompt (`$sSave`) did not compile in the original [observed 2026-11-08, as jip8's SetBodyPartName trial], so the probe sets "NvfProbe" and leaves it for the session |
| GetPrimitiveType | 1 box, 2 sphere [documented]; 0 for none or one RemovePrimitive took away [hypothesis]. The reference table keeps box and sphere primitives only, so type 3 reads 0 |
| RemovePrimitive | 1 when the reference had a primitive [documented]; GetPrimitiveType reads 0 after |
| GetPlayingEffectShaders | the effect shaders scripts play on the reference, in the order they started [documented] |
| GetMineArmedEx | a live mine not going off, the player's included [documented: "works as expected on player-placed mines"]; the engine's mines fill the set (`nvf-play` mines.rs) |
| CallPerRef, CallPerRefEx `function [type(s)] [distance] [angle] [cell]` | the function runs once per reference of the loaded cells (or of `cell`; a `cell` that is not loaded gives no calls [observed: jg6 rerun 2026-11-08, Doc's house 0 calls against GetRefsInCell 21]), the reference its argument, each call made before CallPerRef returns [observed: the player's cell 0xDEB16, type 0, 13 calls against 14 references, counted in the same script and unchanged a frame later]; not the calling reference or disabled ones; a distance or an angle (from the calling reference, else the player) leaves out those farther or outside the heading [documented]. Order: the reference walk's (GetFirstRef) [hypothesis]. An error in the function stops the calls and the calling script [hypothesis]. The arrow lambda `({ref r} => ar_Append aList r)` as the function did not compile in the original's runtime compiler [observed 2026-11-08, CallPerRef and CallPerMobileObject alike; the cause, the arrow form or the one-letter name, not isolated], so the probes pass `(Begin Function {ref rA} … End)` blocks, the form the event handler probes compile |
| CallPerMobileObject, CallPerMobileObjectEx `function level [type(s)]` | once per actor of the processing level: 0 the actors of the loaded cells, 3 the others, 1 and 2 none, as GetActorsByProcessingLevel (`nvse_gaps`), by FormID; not the calling reference or disabled ones [documented: the levels' meanings; hypothesis: NVForever runs every loaded actor fully]. The original's level 0 also calls some references that are not actors [observed: jg6 rerun, 20 calls = the 16 of GetActorsByProcessingLevel 0 and 4 others, not identified]; NVForever calls the actors only [not built] |

Follow-ups (engine): the crosshair reads a reference's prompt from the plugins, not
`nvse_jg6::activation_prompt`; triggers and collision keep a removed primitive; the
lighting template's cell link changes no lighting (the engine reads the template's own
DATA); NVForever has no dying state.

Not in this batch, still on the census (147 JohnnyGuitar names): GetRunSpeed (the run
speed is the engine's animation measure) and GetLandTextureUnderFeet (the game holds no
landscape layers at run time), with the UI, sound, camera, NiNode, texture, navmesh and
event-handler families for the batches after.

## Observed 2026-11-08

The first run in the original (save nvfairport): 22 of 28 rows equal, 5 refused by the
game's compiler. Fixed since: GetActorValueModifierAlt (effects only) and
GetBufferedCellsAlt (no array), 24 of 28 now. The three per-reference probes, the
furniture probe and the prompt probe were rewritten to forms the game compiles (above)
and wait for the next run. Left as they are: IsCellVisited and IsCellExpired on Doc
Mitchell's house and GetMoonPhase hang on the save; GetLightingTemplateCell after the
setter has no rule yet.
