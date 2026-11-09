# Lutana NVSE, batch 1 (2026-11-06, #1009)

Lutana NVSE (origin `LU` in the catalogue) is defunct; JIP LN NVSE took its functions in
(142 of the 145 names of #1009 are registered by the installed `jip_nvse.dll`
[observed: the plugin's name strings, #1009]). Sources: the GECK wiki export
(`C:\fnv-private\geckwiki\functions.xml`) [documented: the page named]; the record
layouts from `schemas/fnv` (xEdit wbDefinitionsFNV.pas). Probed in the original through
the installed JIP LN: set `lutana1` of `tools/capture/jip_probes.py` (233 rows, every change
put back; lab scenario L5.ext.lutana1); "[observed: lutana1 row]" names the row. Code:
`crates/fnv-game/src/nvse_lutana.rs`; tests: `crates/fnv-game/tests/lutana.rs`.

Setters change the record for the session (not saved), read by the getters. A form
argument that names a record of another type changes nothing and reads 0 [observed:
lutana1 chal.threshold.notchal, acti.setradio.term, lscr.settype.caps]. A whole number a
setter stores in a 32-bit field is kept as given, a negative one too, and read back signed
[observed: lutana1 chal.setthreshold.neg, csno.setmaxwinnings.neg, csno.setnumdecks.0,
radius.set.base.neg]; a byte keeps its low 8 bits [observed: lutana1
weap.setcritflags.300, 300 reads 44]; NaN is refused.

Not settled by lutana1 (the rows failed to compile in the original, "script did not
compile"): `ref.GetZone` and `player.GetZone`, `GetChanceNoneGlobal list`, a bare
`GetWaterFormEffect` and a float literal in SetChallengeThreshold. NVForever still compiles
and answers them as below; the original either does not register GetZone and
GetChanceNoneGlobal under these names or takes other arguments [hypothesis].

## 1. Challenges (CHAL)

| Function | NVForever |
|---|---|
| GetChallengeType `challenge` | DATA type, 0 KillInFormList … 13 Scripted [documented: the page's table] |
| GetChallengeThreshold, …Interval, …Flags, …Value1-3 `challenge` | DATA threshold, interval, flags (1 Start Disabled, 2 Recurring, 4 Show Zero Progress), values [documented] |
| GetChallengeForm1, …Form2 `challenge` | SNAM, XNAM [documented] |
| SetChallenge… `challenge value` (the seven above but the type) | the field for the session [documented]; a negative number is kept and read back signed [observed: lutana1 chal.setthreshold.neg] |

The challenge tracker (`crate::challenges`) still counts from the record; a changed
threshold or form reaches the getters only (follow-up: read `nvse_lutana::challenge`
there; the page's note says a lowered threshold completes on the next increment).

## 2. Casinos (CSNO)

| Function | NVForever |
|---|---|
| GetCasinoBJ17Stand, GetCasinoBJPayout, GetCasinoMaxWinnings, GetCasinoNumDecks, GetCasinoShufflePercent `casino` | DATA's soft-17 flag, payout, max winnings, decks, shuffle share (1.0 = 100%) [documented] |
| GetCasinoReelStops `casino symbol` | the stops of symbol 1-7 (7 the W) [documented]; another symbol -1 [observed: lutana1 csno.reel.0, csno.reel.8] |
| SetCasino… `casino value` | the field for the session [documented], as given: 0 decks, -1 max winnings, a shuffle share of 1.5 or -0.2 [observed: lutana1 csno.setnumdecks.0, csno.setmaxwinnings.neg, csno.setshufflepercent.over/neg]; soft 17 any number not 0 is 1 [observed: lutana1 csno.setbj17stand.5] |
| SetCasinoReelStops `casino symbol stops` | the count as given; the "14 stops" total is the record's rule, not checked [hypothesis] |

The tables' max winnings read the change (`casino::max_winnings`). The engine's blackjack
and slots read decks, payout, soft 17 and reel stops from the record (follow-up:
`engine/nvf-play/src/blackjack.rs` to read `nvse_lutana::casino`).

## 3. Recipes (RCPE)

| Function | NVForever |
|---|---|
| GetRecipeInputCount, GetRecipeOutputCount `recipe item` | the quantity of the item's first entry (RCQY after its RCIL/RCOD); 0 when not listed [documented; observed: lutana1 rcpe.addinput] |
| GetRecipeRequiredSkill, …SkillLevel `recipe` | DATA skill (an actor value code) and level [documented]; DATA's −1 "none" reads −1, level 0 [observed: lutana1 rcpe.skill.powder, rcpe.level.powder] |
| SetRecipeRequiredSkill `recipe code` | the skill; a code below 0 removes the skill and the level stays [observed: lutana1 rcpe.setskill.neg] |
| SetRecipeRequiredSkillLevel `recipe level` | the level, a recipe without a skill too [observed: lutana1 rcpe.setlevel.noskill] |
| AddRecipeInputForm, AddRecipeOutputForm `recipe item count` | a new entry put first, an item already listed too (it is then listed twice), a count of 0 too [observed: lutana1 rcpe.addinput, rcpe.addinput.order, rcpe.addinput.existing, rcpe.addinput.zero]; a negative count adds nothing [hypothesis] |
| RemoveRecipeInputForm, RemoveRecipeOutputForm `recipe item` | the item's first entry removed [observed: lutana1 rcpe.addinput, rcpe.addinput.order.restore] |
| ReplaceRecipeInputForm, ReplaceRecipeOutputForm `recipe old new` | the first entry's item replaced, its count kept [documented: the examples; observed: lutana1 rcpe.replaceinput] |
| SetRecipeInputCount, SetRecipeOutputCount `recipe item count` | the first entry's count, 0 too [observed: lutana1 rcpe.setincount.0, rcpe.setoutcount.0, rcpe.setincount.unlisted]; an unlisted item is not added, a negative count is refused [hypothesis] |
| GetFormRecipes `item [category]` | the recipes taking the item, those under the category (or subcategory) when given [documented]; the later plugin's recipes first, each plugin's in FormID order [observed: lutana1 rcpe.formrecipes.*] |
| GetFormRecipeOutputs `item [category]` | those recipes' outputs, each once [documented: the page; observed: lutana1 rcpe.recipeoutputs.drained]; the original's order is not the recipes' (lutana1 rcpe.recipeoutputs.spare: the FalloutNV.esm outputs mostly by FormID downwards, then others), no rule found; NVForever lists them in the recipes' order [hypothesis] |

GetRecipeInputForms and GetRecipeOutputForms (`nvse_plugins`, `nvse_gaps`) read the
changes. The engine's crafting menu (`engine/nvf-play/src/crafting.rs`) reads
`GameData::recipes` (follow-up: read `nvse_lutana::recipe`).

## 4. Actors

| Function | NVForever |
|---|---|
| ActorHasEffect `effect` | an active effect of that base effect on the calling actor [documented], as HasMagicEffect |
| GetServiceFlag `npc flag` | the AI data's services (1 Weapons, 2 Armor, 4 Alcohol, 8 Books, 16 Food, 32 Chems, 64 Stimpacks, 1024 Misc, 131072 Repair) [documented: the table; AIDT, xEdit]; 1 when every bit is set; a "Use AI Data" NPC reads its template's [observed: lutana1 actor.service.doc.3, actor.service.tender2.4] |
| SetServiceFlag `npc flag on` | sets or clears the flag in the NPC record's own services, which it then reads; its template is not changed [observed: lutana1 actor.setservice.tender2]; the vendors read the change (`Game::services`), so an NPC with "Autocalc Service" keeps trading its class's services [hypothesis: autocalc overrides the AI data] |
| LNGetAggroRadius, LNSetAggroRadius `[radius] [actorBase]` | AIDT aggro radius of the base given, else the calling reference's [documented]; the change is the NPC record's, as SetServiceFlag's [hypothesis]; a radius below 0 reads 0 [hypothesis: lutana1 actor.setaggro.neg agrees]; the aggro checks read the change (`Game::aggro_radius`), still gated by the record's "Aggro Radius Behavior" |
| GetCombatStyle `[actorBase]` | a reference: its package's, SetCombatStyle's, else its AI data's ZNAM (`Game::combat_style`'s order); a base: its ZNAM [documented: "the current Combat Style"]; none DefaultCombatstyle [observed: lutana1 actor.combatstyle.doc, actor.combatstyle.ref.docref, actor.combatstyle.player] |

## 5. Perks, worlds, cells, water, zones, leveled lists

| Function | NVForever |
|---|---|
| IsTrait `perk` | PERK DATA Trait, SetPerkFlag's change first [documented] |
| GetNthPerkEntryForm `perk index` | an ability entry's spell, a quest stage entry's quest [documented: "Ability for type 5, Quest for type 6"]; an entry point without a leveled list 0, a negative or missing index 0 [observed: lutana1 perk.entryform.builtto.1, perk.entryform.actionboy.2, perk.entryform.actionboy.-1]; an entry point's leveled list ("type 3") is not kept decoded [hypothesis: 0] |
| GetWorldspaceParentWorldspace (GetWorldParentWorld) `world` | WRLD WNAM [documented] |
| GetCellWaterForm `[cell]` | XCWT; an exterior cell its worldspace's NAM2 [documented]; the player's cell when none is given [documented] |
| GetCellWaterHeight `[cell]` | XCLW as the record has it: 3.4e38 in an exterior cell marked "no height", -2147483648 in an interior [observed: lutana1 cell.waterheight.startcell, cell.waterheight.doccell]; a cell without XCLW its worldspace's DNAM default water height [hypothesis] |
| SetCellWaterHeight `height [cell]` | XCLW for the session [documented]; the water drawn keeps the record's (follow-up) |
| GetCellCoords `[cell]` | a string map with "x" and "y", XCLC [documented]; an interior cell 0 and 0 [observed: lutana1 cell.coords.doccell] |
| GetWaterFormEffect `[water]` | WATR XNAM's actor effect [documented]; with no water, the player's cell's [hypothesis: the bare call does not compile in the original, lutana1 water.effect.none] |
| GetZone `ref` | the encounter zone of the reference's parent cell, SetCellEncounterZone's first [documented] |
| GetZoneFlag, SetZoneFlag `zone flag [on]` | ECZN DATA flags 1 Never Resets, 2 Match PC Below Minimum Level [documented: the table]; GetZoneFlag 1 when any bit of the mask is set [observed: lutana1 zone.flag.vault11zone.3]; set through `Game::zone`, so GetZoneMatchLevel reads it [hypothesis: the zone's respawn and level checks still read the record] |
| GetChanceNoneGlobal `list` | LVLG, SetChanceNoneGlobal's first [documented] |

## 6. Activators, terminals, locks

| Function | NVForever |
|---|---|
| GetActivatorRadioStation `[activator]` | ACTI RNAM of the argument, else the calling reference's base [documented] |
| SetActivatorRadioStation, SetActivatorSoundActivate, SetActivatorSoundLooping `form [activator]` | RNAM, VNAM, SNAM for the session [documented], read by GetActivatorSound… (`nvse_plugins`) |
| GetTerminalLock `[terminal]` | the base's DNAM difficulty, GetLockLevel's terminal number (0 Very Easy … 4 Very Hard, 5 Requires Key) [documented: "regardless of locked state"; `terminal::lock_level`]; a terminal whose DNAM flags say Unlocked -1 [observed: lutana1 term.lock.manny] |
| GetPasswordNote, SetPasswordNote `[note] [terminal]` | TERM PNAM [documented]; the hacking menu's password check still reads the record (follow-up: `terminal::has_password`) |
| GetLockedOut `[ref]` | 0 not broken, 1 broken (a forced lock, a terminal locked out), 2 broken again on the Infiltrator or Computer Whiz try, 3 an inaccessible door (record flag 0x100) [documented: the table; `lockpick::Locks`, `terminal::Terminals`] |
| SetLockedOut `state [ref]` | 0 mends, 1 breaks, 2 breaks with the perk's try spent, in the same state [documented]; another state nothing [observed: lutana1 lock.setlockedout.door] |
| PlayerHasKey | a door's or container's key carried (a load door without its own lock, the other side's), a terminal's password note among the notes or carried [documented; observed: lutana1 lock.haskey.door; hypothesis: the other side] |
| IsParentActivateOnly `[ref]` | XAPD "Parent Activate Only" [documented] |

## 7. Flags, playability, weapons, load screens, misc

| Function | NVForever |
|---|---|
| GetFormFlag, SetFormFlag `form flag [on]` | 1 when every bit of the mask is among the record flags, as GetFlagsLow/High read and SetFlagsLow/High store them [documented: the table] |
| LNIsPlayable, LNSetIsPlayable `[on] [form]` | weapons, armour, ammo as IsPlayable/SetIsPlayable; RACE DATA flags bit 0; HAIR and EYES DATA bit 0; PERK DATA Playable (SetPerkFlag's store) [documented: the form types; the fields xEdit]; the calling reference's base when no form |
| SetWeaponCritFlags, SetWeaponKillImpulse, SetWeaponImpulseDistance, SetWeaponVATSEffect `weapon value` | CRDT flags, DNAM kill impulse and impulse distance, VATS effect, for the session [documented], read through the record getters; combat still reads the record (follow-up) |
| GetLoadScreenType, SetLoadScreenType `screen [type]` | LSCR WMI1 [documented] |
| GetPCCanUsePowerArmor | SetPCCanUsePowerArmor's state [documented] |
| GetMessageDisplayTime (GetMDT), SetDefaultMessageTime (SetDMT) | 2 seconds, else the time set [documented: "defaults to 2.0"]; GetDefaultMessageTime (JIP) reads the same; 0 and -2 are kept as given [observed: lutana1 misc.setdmt]; the installed JIP LN registers neither GetMessageDisplayTime nor GetMDT, so in a script they read as their own text (`fnv-script` encodings.json `not_commands`) [observed: lutana1 misc.messagedisplaytime, misc.mdt]; the HUD's corner messages keep their own timing (follow-up) |
| AddNoteNS `note` | AddNote without the corner message [documented]: the note is filed and OnNoteAdded handlers run |
| GetTimeStamp | the computer's local time as "HH-mm-SS" [documented] |
| CrosshairRefInList `list` | 1 the reference under the crosshair is in the list, 2 its base is, else 0 [documented] |
| GetRadius, SetRadius `[radius] [light]` | a light base's DATA radius (whole, a negative one kept) [observed: lutana1 radius.set.base.frac, radius.set.base.neg]; else the calling reference's XRDS as given, -16.59 too [observed: lutana1 radius.ref.fire, radius.set.ref], else its light base's radius [hypothesis], else 0 [documented: "a reference (any type) or Light BaseForm"]; the radiation markers keep the record's radius (follow-up) |

## 8. Not built yet (53)

Left because NVForever lacks the system or the data, or the page does not say enough:

- Map markers (8: GetMapMarkerHidden, …Travel, …Type, IsMapMarker, SetMapMarkerHidden,
  …Travel, …Type, …Visible): the REFR XMRK data (flags, type) is decoded only in the
  engine's Pip-Boy (`engine/nvf-play/src/pipboy/data/read.rs`), not in `fnv-sim`'s
  `PlacedRef`.
- Radio references (9: GetRadioBroadcastType, …PosRef, …Radius, …Static, IsRadioRef and
  the four setters): the talking activator reference's radio data (XRDO) is not decoded.
- Hotkeys (4: ClearAllHotkeys, SetHotkey, SaveHotkeys, RestoreHotkeys): hotkeys keep an
  item only, not its health and mod flags; Save/Restore are deprecated (JIP keeps hotkeys).
- INI writing (3: SetINISection, RemoveINIKey, RemoveINISection): the pages write into
  `Data\config`, and NVForever never writes into the game folder (install-layout.md);
  needs a decision on where NVForever's copy lives.
- Appearance and base swaps (4: CopyFaceGenFrom, SetRace, SetBaseForm, LNSetActorTemplate)
  and SetActorTemplateFlag: the template resolution and the drawn actor read the record;
  a session change must reach both.
- Equipping (2: EquipItemData, SetEquippedData): equipping a stack of a given health and
  mod flags needs the inventory's per-instance data in EquipItem.
- SetPerkRank: perk ranks on NPCs (the page's uNPCPerks) are not kept.
- Perk entries (3: SetNthPerkEntryForm, …Function, …String): the perk effects read
  `GameData::perks`; a session copy must reach `crate::perks`.
- Records not decoded (7): GetActivatorPrompt and SetActivatorPrompt (ACTI XATO is not in
  the schema), GetAnimObjectIdle and SetAnimObjectIdle (ANIO is not among the decoded
  types), GetLoadScreenTypeTextRGB and SetLoadScreenTypeTextRGB (LSCT is not among them),
  SetLoadScreenTexture (the loading screen reads the record).
- IsSkillMagazine and SetIngestibleFlag: the magazine test needs the effects' HasPerk
  conditions; the ingestible flags feed `needs` and `services` from `GameData`.
- GetShowQuestItems: SetShowQuestItems is not built.
- GetFormCountType, GetFormMods: the form type numbering of the page and the load order's
  overriding plugins per form are not kept.
- GetPlayerRegions, OnCellEnter, OnCellExit: the player's regions are not tracked; the
  two events need the event dispatcher's cell-change hook.
- Deprecated (GetMessageDisplayTime, the third name JIP LN does not register, is built, §7):
  GetCellRefs, SetVariableFloat; BaseAddItemHealth (deprecated, "doesn't work properly").

## 9. Batch 2 (2026-11-08, #1009, `nvse_lu2.rs` and `mod_files.rs`)

The owner asked on 2026-11-08 to finish every function; this batch adds the pieces §8
named as missing for 29 of the 53: the map marker and radio data of a placed reference
(`fnv_sim::refs::PlacedRef::map_marker`, `::radio`, decoded from REFR XMRK/FNAM/TNAM and
XRDO by the schema's layout), the hotkeys' unit data, the INI writers through NVForever's
file layer, and the plugins that hold a form (`GameData::form_overrides`). From the pages
and the records [documented]. Probe set `lu2` (`tools/capture/jip_probes.py`, lab
`L5.ext.lu2`) ran in the original on 2026-11-08 (save nvfairport, JIP LN 57.30, which
carries Lutana's functions; private golden `lu2`): its 9 compiled rows all equal after the
XRDO fix below; the 6 the game refused are rewritten (below) and wait for the next run.
Tests: `crates/fnv-game/tests/lutana2.rs`.

| Function | NVForever |
|---|---|
| IsMapMarker, GetMapMarkerHidden, GetMapMarkerTravel, GetMapMarkerType | a reference with XMRK; FNAM 4 "Show All" Hidden, 2 Can Travel To; TNAM's type, 0 None … 14 Vault [documented: the pages' tables; xEdit REFR] |
| SetMapMarkerHidden, SetMapMarkerTravel, SetMapMarkerVisible, SetMapMarkerType | the flag (1 Visible) or type for the session (`Game::marker_flags`, `::marker_kind`); the Pip-Boy map's fast-travel check reads the flags as set (`pipboy::data::travel::travel_target`) [documented]. The map's icons and its visibility test still read the record [not built: follow-up]. The page's "permanent state" after ShowMap is not kept: nothing is saved [hypothesis] |
| IsRadioRef, GetRadioBroadcastType, GetRadioRadius, GetRadioStatic, GetRadioPosRef and the four setters | XRDO: the range type 0 Radius … 4 Current Cell, the radius, the static percentage, the exterior position reference; set for the session, GetAvailableRadios' copy too (`nvse_gaps::RadioSite`) [documented]. RNVTARef reads 1, Everywhere, as the page's example [observed 2026-11-08]; the decoder gives a range type the schema names as its name, read back to the number (`fnv_sim::refs::broadcast_range_code`; it read 0 before). The pages' AlienRadioBeaconREF is FO3's: FalloutNV.esm has the TACT AlienRadioBeacon but no reference by that name, so the game's compiler and NVForever's both refused the five rows naming it [observed 2026-11-08]; they now read Camp Guardian's radio, NVCGRadioREF. GetRadioPosRef is registered with no return type (the game's nvse.log, RegisterCommand), so its result goes through `set` before GetFormIDString (as GetCurrentFurnitureRef, ext-jg6.md). The engine's radio plays from the record [not built: follow-up] |
| ClearAllHotkeys, SetHotkey `index item health mods` | the eight keys cleared; SetHotkeyItem with the unit's health and mod flags kept beside it [documented]; an item the player carries [hypothesis, as SetHotkeyItem] |
| SaveHotkeys, RestoreHotkeys `array` | an array of eight [item, health, mods] lists and back [documented: deprecated, "returns an array that can be passed to RestoreHotkeys"; hypothesis: the layout] |
| SetINISection `section map [file]`, RemoveINIKey `Section:Key [file]`, RemoveINISection `section [file]` | under `Data\Config` in NVForever's layer (`mod_files.rs`), the calling mod's file by default, numbers as LU writes them; a missing key or section writes nothing [documented; hypothesis for the last] |
| IsSkillMagazine `ingestible` | an effect with a condition HasPerk Retention or Comprehension, the perks found by Editor ID [documented] |
| SetIngestibleFlag `ingestible flag value` | 2 Food, 4 Medicine on ENIT for the session; GetIngestibleFlag and the Medicine and Survival boosts read it (`Game::ingest_on`) [documented]; the vendors' categories read the record [not built] |
| GetShowQuestItems | SetShowQuestItems' switch, on by default [documented]; SetShowQuestItems runs by name in `nvse_lu2` (scripts compiled from text); it is not registered by opcode, as functions.json has none for it (only command_opcodes.json records 0x114E [observed], and the catalogue takes opcodes only from the masters' bytecode or xEdit); the engine's inventory lists quest items either way [not built] |
| GetFormCountType `array type` | the forms of the array of that GetType code [documented]. The first parameter is the array's ID as an integer since JIP v40 [documented: the page], so an array variable passes it, as the page's example, but an array expression in its place, `GetFormCountType (ar_List …) 47`, did not compile in the original [observed 2026-11-08]. NVForever's compiler does not check extender argument types and compiles both; the function takes the ID as an array or a number |
| GetFormMods `form` | the plugins defining or overriding it, in load order [documented: the example] |

Left for batch 3 (24, built in §10): appearance and base swaps (CopyFaceGenFrom, SetRace, SetBaseForm,
LNSetActorTemplate, SetActorTemplateFlag), EquipItemData, SetEquippedData, SetPerkRank,
the perk entry setters, the activator prompt, ANIO and LSCT fields, SetLoadScreenTexture,
GetPlayerRegions, OnCellEnter, OnCellExit, GetCellRefs, SetVariableFloat,
BaseAddItemHealth.

## 10. Batch 3 (2026-11-09, #1009, `nvse_lu3.rs`): the last 24

From the pages in the wiki export [documented: the page named] and the records (schemas
`fnv`). Tests: `crates/fnv-game/tests/lutana3.rs`. Probe set `lu3` of
`tools/capture/jip_probes.py` (lab `L5.ext.lu3`), not run yet; its questions are below.
Session edits are not saved [hypothesis: as batch 1's setters].

| Function | NVForever |
|---|---|
| GetActivatorPrompt, SetActivatorPrompt `[activator]` | ACTI XATO, the form given or the calling reference's base; "" without one [hypothesis]; set for the session (`Game::field_overrides`) [documented] |
| GetAnimObjectIdle, SetAnimObjectIdle | ANIO DATA, the idle (ANIO decoded with the record fields now); the setter takes an IDLE only [hypothesis] |
| GetLoadScreenTypeTextRGB, SetLoadScreenTypeTextRGB | LSCT DATA font 1's red, green, blue as a stringmap r g b [documented: the page's example 255 180 53, which are font 1's of StandardLoadingTip, loading-screens.md]; floats 0-255, a value outside clamped [hypothesis] |
| SetLoadScreenTexture | LSCR ICON for the session [documented]; the loading screen the engine draws reads the record [not built: engine follow-up] |
| SetRace `npc race` | the NPC base's RNAM for the session, GetRace of the base reads it; never the player's [documented: "doesn't work on the Player"]; the drawn actor and GetIsRace of its references read the record [not built] |
| LNSetActorTemplate `template [base]`, SetActorTemplateFlag `flag state [base]` | TPLT and ACBS template flags 1 … 512 for the session [documented: the page's table]; GetActorTemplate reads them; the template resolution of stats, factions and the rest reads the record [not built: follow-up, a design of its own] |
| CopyFaceGenFrom `[sourceNPC]` | the source NPC record's race and sex and its face geometry (FaceGen FGGS/FGGA, 100 %) through the appearance changes AgeRace, SexChange and MatchFaceGeometry use (`crate::appearance`), so the engine draws the actor again at once [documented: "Visual update is already performed"]; without a source the backup is put back [documented]. [decision: on the calling actor, not every reference of its base (the page changes the base); NVForever keeps a look per reference, saved with it.] The player's face geometry is not copied (its creator look is matched only to a face the game holds); hair, eyes and skin tone stay [not built: the drawn head reads them from the record] |
| SetBaseForm `newBase` | the reference's base for the session, of another type too [documented: JIP LN 56.70]; not the player; the drawn reference changes when its 3D is built again [hypothesis; engine follow-up] |
| EquipItemData `item [health] [mods] [noUnequip] [noMessage]`, SetEquippedData `array [noUnequip] [noMessage]` | the carried unit of that health in points (within half a point; -1 any) and mod flags, as GetEquippedData gives them [documented; hypothesis: points], equipped as EquipItem with OnActorEquip and OnEquip [documented: "Also triggers OnEquip"]; none such, nothing |
| SetPerkRank `perk rank [forTeammates]` | the player's rank (gained rank by rank up to it, with the perk's quest stages; lowered), the teammates' perk, or another actor's held rank [documented]; cut to the perk's ranks, 0 changes nothing [observed: lu3 perkrank, Educated set to 9 reads 1, then 0 still 1] |
| SetNthPerkEntryForm, SetNthPerkEntryFunction, SetNthPerkEntryString | a session copy of the perk with the entry's spell or quest, entry point code, or button label changed [documented]; the perk effects, abilities, activate choices and the GetNthPerkEntry… getters read the copy (`Game::perk_def`). The Nth entry is numbered by rank, then priority, entries alike in the reverse of the record's order (`nvse_lu3::nth_entry`), for the getters too [observed: lu3 perkentry, Fast Shot's entry 1 is 8, its third of four rank-0 entry points in the record (40, 43, 8, 34); gaps4 and jip2, Toughness's entry 0 is its rank-1 entry] |
| GetPlayerRegions | the regions the player stands in, IsPlayerInRegion's test, in FormID order [documented; hypothesis: the order] |
| OnCellEnter, OnCellExit | event names for SetEventHandler: sent with the cell when the player's cell changes, exit first, exterior cells too [documented: the notes; hypothesis: the order]; called as functions 2 and 3 [observed: lu3 events] |
| GetCellRefs `type depth taken [cell]` | GetRefsInCell's list, the player's cell by default [documented: deprecated by GetRefs] |
| SetVariableFloat `name value [quest]` | SetVariable's [documented: deprecated by SetVariable] |
| BaseAddItemHealth `item count health [base]` | the base container's entry with that health (a fraction 0-1) for the session [documented: deprecated, "NOT save baked"] |

Engine follow-ups: the loading screen's texture and text colour, the drawn race and the
template resolution of a changed base, hair, eyes and skin tone of CopyFaceGenFrom, the
3D of a reference after SetBaseForm.

Probed once in the original (2026-11-09, PR #1914's head, issue #1925): 2 of the 5 rows
it answered matched; the three that did not (perkrank, perkentry, events) are followed
above. Four did not compile there and are rewritten: the prompt is read only (the setter
with `$sOld` to put it back did not compile), the LSCT colour's elements go into floats
before sv_Construct, GetCellRefs is given its cell (the page lists it as not optional),
and SetVariableFloat uses a FalloutNV.esm quest variable (nGeckosKilled of 10A214) as the
probe plugin is not active in the probe game.

Probe questions (set `lu3`, every change put back):

1. `prompt`: BlackjackTableGomorrah's prompt before and after setting "NVF Probe".
2. `anio`: RepairWall01Wrench's idle (the page's example; it may be Fallout 3's only).
3. `lsct`: StandardLoadingTip's colour (hypothesis `255 180 53`) and after setting 59.3
   181 300 (hypothesis `59.3 181 255`: the blue clamped).
4. `perkrank`: Educated's rank set to 9, then 0, then put back (hypothesis: cut to its
   ranks, 0 takes it away).
5. `perkentry`: Fast Shot's second entry point 8 → 7 → 8 (the page's example).
6. `regions`: none indoors (hypothesis 0).
7. `cellrefs`: GetCellRefs equal to GetRefs in the player's cell (hypothesis 1).
8. `setvar`: SetVariableFloat's result and the value read back (hypothesis `0 7`).
9. `events`: OnCellEnter and OnCellExit called as functions (hypothesis `0 0`, or they
   do not compile).
