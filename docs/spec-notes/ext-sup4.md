# SUP NVSE batch 4: arrays, delayed calls, saves, near cells and the rest (2026-11-09, #1008)

The last 17 SUP functions (the names `handover.md` of 2026-11-08 called SUP batch 4; the
branch `claude/sup-batch4` claimed for them was never pushed, so they are built here on
`claude/sup-batch4b`). Code: `crates/fnv-game/src/nvse_sup4.rs`, the kept calls in
`crates/fnv-game/src/delayed.rs`, the VM's hand-overs in `fnv_script::vm` (Ar_SetFloatsFrom's
output variables, MarkScriptOnLoad's calling script, the plugins' `ar_` functions passed to
the host), and the engine's save and load names in `engine/nvf-play/src/save.rs` and
`script.rs`. Tests: `crates/fnv-game/tests/sup4.rs` and the unit test of `nvse_sup4.rs`.

Source: the GECK wiki export [documented: the page named], in our own words. Not probed
yet: set `sup4` of `tools/capture/jip_probes.py` (lab `L5.ext.sup4`), questions in §3.

## 1. The functions

| Function | NVForever |
|---|---|
| AllocatePlayer | does nothing, 0 [documented: a joke page, "Two. Can play at that game. (no, they can't)"; decision: there is one player] |
| Ar_GetRandomKey `array` | a random key of the array, a number [documented]; -1 for an empty array [hypothesis] |
| Ar_GetRandomKeyMap `array` | a random key as a string, a number key as its text [documented: the note]; "" for an empty array [hypothesis] |
| Ar_HasInvalidRefs `array [dimension]` | 1 when the array holds an empty reference or one neither the load order nor the world knows [documented: "origin mod is disabled or temporary reference … removed"]; with a dimension the inner array at that key is searched [documented: the second example] |
| Ar_SetFloatsFrom `array f1 … f10` | the array's numbers in key order into the variables, up to 10; the VM writes them (`out_params`) [documented: the example]; an element that is no number gives 0 [hypothesis] |
| CallFunctionNextFrame `udf [frames] [removeOnLoad]` | the UDF that many frames later (1 to 1000, 1 by default) on the calling reference [documented: the notes]; frames count in menus too [hypothesis]; with removeOnLoad 0 it outlasts a load (`Game::take_session`), a function script's only [hypothesis: as the extender's other kept callbacks] |
| GetCurrentQuestObjectiveTeleportLinksAlt | the current objective's compass targets, one array: GetCurrentQuestObjectiveTeleportLinks's [documented: "the current compass marker objects"; hypothesis: the same objects] |
| GetLoadedSaveSize, GetSavedSaveSize `type` | type 0 the save file's bytes; -1 before any load or save this session [documented]; type 1, the .nvse co-save, 0 [decision: NVForever writes the extender's state into its one save file] |
| GetSaveName `type` | 0 the last save written, 1 the last loaded, without the extension; "" before any [documented]. The engine tells the game both (the save writer, the load) and they outlast a load |
| GetNearCells `depth` | the cells within `depth` of the player's (`scan_cells`, as GetFirstRef's depth); indoors the player's cell alone; -1 uGridsToLoad's, 2 [documented: "-1 … uGridsToLoad"; hypothesis: the default 5 cells across] |
| GetNearMapMarkers `depth` | the map marker references (REFR XMRK) in those cells [documented] |
| MarkScriptOnLoad, IsScriptMarkedOnLoad `script` | the calling script marked until the next load, which starts with none [documented]; the VM hands the running script to the host; a query or console line has none: 0 [hypothesis] |
| KillAll2 (KillAllAlt) | KillActor on every living actor whose combat target is the player, not the player's teammates [documented: "kills all current player combat enemies leaving companions … intact"; deprecated] |
| SetCaughtPCPickpocketting `[caught]` | the NPC's caught flag that a failed pickpocket sets (`crate::pickpocket`), set when given; the flag as it is now [documented] |
| TranslitText `text` | the text with Cyrillic letters in Latin ones, returned [hypothesis: the page names no return]; [decision: NVForever's own table, a common Russian romanisation with the Ukrainian and Belarusian letters; an upper-case letter starts its Latin letters in upper case] |

## 2. Left as nothing values

AllocatePlayer (a joke page) and the co-save size (no co-save). No engine follow-ups.

## 3. Probe questions (set sup4, not run)

At Doc Mitchell's house; Doc Mitchell's flag set back; the aux variable erased.

1. `randomkey`: a key in 0-4, a map key of the two, an empty list's (hypothesis `1 1 -1`).
2. `invalidrefs`: valid, an empty reference, an inner array without and with its
   dimension (hypothesis `0 1 0 1`).
3. `setfloats`: the page's example (hypothesis `3 8 5`).
4. `nextframe.set`, `.wait`, `.read`: a UDF two frames later (hypothesis `0` then `1`).
5. `saves`: the loaded save's size, its co-save's, its name, the last save's name before
   any save (hypothesis `1 1 1 0`; NVForever gives 0 for the co-save by decision; the lab's
   candidate loads no save, so it gives -1 sizes there).
6. `near`: cells at depth 0, 2 and -1 indoors, map markers (hypothesis `1 1 1 0`).
7. `markonload`: MarkScriptOnLoad from a query, and Doc Mitchell's script marked
   (hypothesis `0 0`, or the call refused outside a script).
8. `caught`: Doc Mitchell's flag read, set, read (hypothesis `0 1 1`).
9. `translit`: "Привет" (hypothesis `Privet`; the runtime compiler's code page may change
   the letters).

## 4. Probed in the original (2026-11-09, head af8a876c0)

The first run compiled nothing: the runtime compiler refuses the declaration
`string_var sA` [observed: the observer's diagnostic runs, other names compile], so the
probes use `sOut`. The second run answered 10 rows; 3 matched (invalidrefs,
nextframe.set, nextframe.wait). What the others say [observed]:

| row | original | NVForever now |
|---|---|---|
| randomkey | `1 0 0` | an empty array's key is 0 (was -1); the map key the original gave was neither "LALALA" nor "TROLOLO" by `==` [open: what it gives] |
| nextframe.read | `0` | the lambda's aux variable was not set two frames later [open: whether the original runs a lambda, or counts frames from the next one; NVForever runs it at the second frame] |
| saves | `1 1 1 0` | the probe game had loaded a save; the lab's candidate (fnvtool) loads none, so it cannot match; the co-save's size (1 there) stays 0 by decision |
| near | `1 20 25 1` | GetNearCells 2 gave 20 cells where -1 gave 25 [open: which 5 of the 5×5 square depth 2 leaves out; NVForever gives the 25] |
| markonload | `1 0` | MarkScriptOnLoad gives 1 from a query too (was 0) |
| caught | `0 0 0` | SetCaughtPCPickpocketting 1 on Doc Mitchell did not read back [open: NVForever sets and reads the flag a failed pickpocket sets] |
| translit | not the question | the probe's UTF-8 Cyrillic reached the game as cp1252 bytes; the literal is now "Привет"'s cp1251 bytes written as cp1252 characters. The lab's candidate reads that literal as Latin letters, so it keeps them: the game's own text is decoded by code page (`fnv_esm::text_guess`), Cyrillic in a Windows-1251 plugin is Cyrillic to TranslitText |
| setfloats | did not compile | `Ar_SetFloatsFrom aR fA fB fC` as a statement is refused by the runtime compiler, as SUP's other functions that write variables (ext-sup6.md §1); scripts compiled ahead keep the page's form |

The open rows wait for another run (the coordinator decides).
