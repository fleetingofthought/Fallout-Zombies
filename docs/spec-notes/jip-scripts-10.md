# JIP LN NVSE, batch 10 (2026-11-08, #1006)

Quest objectives and their targets, the quest lists, SetStageAlt, perks and factions,
script variables added at run time, scripts and event blocks switched off,
FakeScriptEvent, linked and enable children, the references' extra data, script map
markers and the text-file functions, in `crates/fnv-game/src/nvse_jip10.rs`. Sources:
the GECK wiki export, each function's page [documented: the page named]. Probed once (§7):
the probe set `jip10` (`tools/capture/jip_probes.py`, lab `L5.ext.jip10`) asks the
original what the [hypothesis] values below leave open.

36 functions; JIP 178 → 142 [observed: the extender_census test, 2026-11-08, on main 57f45e733 before; after, this branch].

## 1. Quests and objectives

| Function | NVForever |
|---|---|
| AddObjectiveTarget, RemoveObjectiveTarget `quest index ref` | the objective's target list, an objective already displayed only [documented: "these Objectives must be already Displayed"]; added at the end, once |
| SetObjectiveNthTarget `quest index n ref` | the n-th target (from 0) becomes the reference; past the end nothing [hypothesis] |
| GetObjectiveTargets, GetObjectiveHasTarget `quest index [ref]` | the list: the scripts' changes, else the record's QSTA targets in record order, for an objective shown and not done; otherwise no array [observed: VMS01's done objective 10 gave none; hypothesis: the rule] |
| SetObjectiveText `quest index "text" vars…` | the formatted text [documented: the example's `%g`, `%n`]; the Pip-Boy, the corner messages and GetObjectiveText read it (`Game::objective_text`) |
| GetActiveObjectives | the active quest's objectives shown and not done, by index [hypothesis: the page says only "active quest objectives"; indices, as objectives are no forms] |
| GetQuestObjectives `quest [completed]` | the objectives shown and not done, or (completed) the done ones, by index [documented: the examples; hypothesis: indices] |
| GetQuests `[completed]` | the quests in the Pip-Boy's list running and not done, or the done ones [documented; hypothesis: the list is the Pip-Boy's, a named quest with an objective shown or done] |
| ClearActiveQuest | no active quest; its targets leave the map and compass [documented] |
| SetStageAlt `quest stage` | SetStage, then GetStage is that stage even when a higher one was done: the stages above it are no longer done [documented: the summary; hypothesis: GetStageDone of the higher ones reads 0] |

The quest markers read the changed target lists (`Game::check_quest_targets`); a target a
script added has no conditions, and one from the record keeps its record's [hypothesis].

## 2. Perks and factions

| Function | NVForever |
|---|---|
| GetAllPerks `[forTeammates] [includeHidden]` | the player's own perks, or those for teammates; another actor's own perks (NVForever gives NPCs their perks, as JIP's uNPCPerks), with the teammate perks for a teammate when asked; hidden perks (PERK DATA hidden) only when asked [documented] |
| RemoveAllPerks `[forTeammates]` | the player's perks (each leaves as RemovePerk's does), or those for teammates; another actor's own [documented] |
| GetFactions `[baseActor]` | the reference's factions now (`functions::actor::factions_of`) in the base record's order, then any added [observed: Doc Mitchell], or the base's own (its template's when it takes its factions from one) [documented] |

## 3. Script variables, scripts and their blocks

| Function | NVForever |
|---|---|
| AddScriptVariable `"name" [quest]` | a new variable of the reference's or the quest's script, read and set by name with GetVariable, GetRefVariable, SetVariable, SetRefVariable [documented]; 1 when added and 1 again for a name added before [observed]; 0 for a name the script declares [hypothesis]; saved with the game [documented] |
| RemoveScriptVariable, HasVariableAdded `"name" [quest]` | the added variable goes (its value too), whether it was added [documented] |
| RemoveAllAddedVariables `[quest]` | every variable added to that script [documented] |
| ClearJIPSavedData `scriptVars linkedRefs auxVars refMaps` | the added variables, SetLinkedReference's links, the calling mod's (1) or the public (2) permanent auxiliary variables and RefMaps [documented]. NVForever does not keep which mod added a variable or a link, so 1 clears them all [hypothesis] |
| SetScriptDisabled, GetScriptDisabled `scriptOrList [disable]` | no block of the script runs; its variables still read and set [documented] |
| SetScriptEventDisabled, GetScriptEventDisabled `formOrList "events" [disable]` | the event blocks named (space-separated) do not run, for a reference or every reference of a base form, or a list's forms [documented] |
| FakeScriptEvent `"event" [filter]` | the reference's (or inventory reference's) block of that event runs with the filter as its object, a block switched off too; not OnActivate, not a quest's [documented]; 1 when a block ran |

## 4. References

| Function | NVForever |
|---|---|
| GetLinkedChildren | the references whose linked reference is this one (SetLinkedReference counted), in FormID order [documented] |
| GetEnableChildren | the placed references whose enable parent (XESP) is this one, by FormID [documented] |
| SetRefExtraData `[slot value]` | slots 0-17 a number or a form, 18-19 a string [documented]; owned by the calling script's mod (its FormID's file index); a script of no file keeps nothing [observed: the probe, from a script of no file, read nothing back; hypothesis: the reason]; a value of the wrong type is refused, no value clears the slot [hypothesis]; both left out clear this mod's data on the reference [documented]. Saved [documented] |
| GetRefExtraData `slot [modIndex]`, RefHasExtraData `[modIndex]` | another mod's by its index [documented]; unset: 0, or "" for 18-19 [hypothesis] |
| AddRefMapMarker, RemoveRefMapMarker, RefHasMapMarker | not on an inventory item; gone after a load [documented] |

## 5. Text files

Under the game's folder for the original; NVForever's layer for NVForever
(`crate::mod_files`; never the game folder: CLAUDE.md, install-layout.md).

| Function | NVForever |
|---|---|
| ReadStringFromFile `path [startLine] [count]` | the lines from startLine (1 the first), count of them, of at most the file's first 16384 characters [documented], joined by line breaks with none after the last [observed]; a missing file "" [hypothesis] |
| WriteStringToFile `path append "text" vars…` | the formatted text, appended or in place of the file; 1 when written [documented]; an append to a file with text starts on a new line, line breaks LF [observed] |
| ModLogPrint `indent "text" vars…` | a line in `Mod Logs\<plugin>.log` [documented: the folder and the name]; a tab per indent level [hypothesis]; nothing from a script of no plugin [hypothesis] |
| RunBatchScript `path` | the file's source runs at once on the calling reference, or the player; 1 when it was read and ran without an error [documented]. Form IDs in quotes as the page shows them compile only where NVForever's compiler takes them [hypothesis, probe] |

## 6. Follow-ups

- AddRefMapMarker: the Pip-Boy's map does not draw `Jip10State::map_markers` yet.
- The added variables' owning mods, for ClearJIPSavedData's per-mod clearing.

## 7. The original's answers (probe set jip10, 2026-11-08)

The observer session ran set `jip10` on the probe save (nvfairport); lab `L5.ext.jip10` 6 of
21 rows matched head 6e3ac0872 before these fixes [observed]. Followed since:

- A list getter with nothing to give returns no array (`ar_Size` -1), not an empty one:
  GetAllPerks, GetLinkedChildren, GetEnableChildren, GetQuestObjectives.
- GetObjectiveTargets on a done objective (VMS01 10): no array.
- GetFactions on a reference: Doc Mitchell's first faction is the record's first.
- AddScriptVariable on a name added before: 1.
- SetRefExtraData from the probe (a script of no file): nothing read back.
- WriteStringToFile's append and ReadStringFromFile's line cuts (§5).

Rows that read the save's own state (GetQuests, GetActiveObjectives, GetQuestObjectives on
VMS01, the player's perks) cannot match a candidate that starts from the records alone; they
show the elements are quest forms and objective indices as numbers, which NVForever gives.
