# Spec note: JIP variable stores (auxiliary variables and RefMap arrays)

JIP LN NVSE keeps two named stores outside script variables: **auxiliary variables**
(named lists of values owned by a form or a reference) and **RefMap arrays** (named
maps from forms to values). Task J2 of [jip-tasks.md](../handoff/jip-tasks.md); input
for J5.

Wiki statements are `[documented: <page>]` (private export `C:\fnv-private\geckwiki\`, in
our own words). The concept pages *Auxiliary Variables* and *RefMap Array* are **not in
the export** [observed]; *Template:ModIndexOverride* is (`Templates.xml`, exported
2026-11-08). So the name rules below
come from the game: `[observed: jip-stores/<id>]`, runtime-compiled probes on the player
(`tools/capture/jip_probes.py --set jip-stores`, JIP 57.30, 2026-10-26, 32 rows, the 17
documented values all match; golden private).

## 1. Auxiliary variables

A variable is a name plus an owner (a reference, or the base form given as the last
argument), holding an ordered list of elements. Each element is a float, a reference or
a string [documented: AuxiliaryVariableGetType].

| Function (alias) | Arguments after the name | Returns | Behaviour |
|---|---|---|---|
| AuxiliaryVariableSetFloat (AuxVarSetFlt), …SetRef (AuxVarSetRef), …SetString (AuxVarSetStr) | value; index (default 0; -1 appends); base form (optional) | success | sets an element [documented: AuxiliaryVariableSetFloat] |
| AuxiliaryVariableGetFloat (AuxVarGetFlt), …GetRef, …GetString | index (default 0; -1 the last); base form | the value | 0 when the variable, the index or the type does not match [documented: AuxiliaryVariableGetFloat] |
| AuxiliaryVariableGetType (AuxVarType) | index; base form | 0 none, 1 float, 2 reference, 4 string | [documented: AuxiliaryVariableGetType] |
| AuxiliaryVariableGetSize (AuxVarSize) | base form | element count, 0 if undefined | [documented: AuxiliaryVariableGetSize] |
| AuxiliaryVariableErase (AuxVarErase) | index (missing or -1: the whole variable); base form | the new element count | [documented: AuxiliaryVariableErase] |
| AuxiliaryVariableGetAll (AuxVarGetAll) | class 0–3; base form | a string map: variable name → its values | classes: 0 permanent-private, 1 temporary-private, 2 permanent-public, 3 temporary-public [documented: AuxiliaryVariableGetAll] |
| AuxiliaryVariableGetAsArray (AuxVarGetAsArr) | base form | array of the values | [documented] |
| AuxVarGetFltCond | quest, string-variable index | element 0 as a float | a condition function: the name comes from a string variable of the quest's script [documented: AuxVarGetFltCond] |

Observed rules:
- Set and get round-trip floats and strings: 3.5 and "abc" [observed: aux.float,
  aux.string]. Appending with index -1 grows the list (three sets give size 3), and index
  -1 reads the last element [observed: aux.append.size, aux.last].
- **Setting an index beyond the end does nothing**: index 5 on an undefined variable
  leaves size 0 [observed: aux.gap.size].
- Setting a different type at an index replaces the element, type included: float then
  string gives type 4 [observed: aux.overwrite.type]. Reading a float from a string
  element gives 0 [observed: aux.float.of.string].
- Missing: GetFloat 0, GetType 0, GetSize 0 [observed: aux.missing.*].
- Erasing one element returns the new count (2 of 3), and later elements move down
  (element 0 is then the old element 1) [observed: aux.erase.returns, aux.erase.shifts].
  **Erasing the whole variable returns -1** [observed: aux.erase.all]. The page only says
  "the new number of elements".
- **Names ignore case**: "*NVFc" reads "*nvfC" [observed: aux.case].
- **Owners are separate**: a variable set on Doc Mitchell's base form is not seen on the
  player reference (base 7, player 0) [observed: aux.owner.base].
- **Prefix `*` selects the temporary classes**. A name without it shows up in classes 0
  and 2 (permanent); with `*` it shows up in 1 and 3 (temporary) [observed:
  aux.class.nvfP = bits 0+2, aux.class.tnvfP = bits 1+3]. A leading `_` (`_nvfP`,
  `*_nvfP`) changed nothing here [observed: aux.class.unvfP, aux.class.tunvfP].
  Private/public was not separable from a runtime-compiled script: each variable showed
  in both its private and its public class.
  **A private variable belongs to the mod index of the plugin whose script made it.** A
  patch plugin (ESM or ESP) whose script works on the same variable takes the variable
  over with its own mod index, and the original plugin's scripts can no longer reach it
  [documented: Template:ModIndexOverride]. Erasing private variables removes only those of
  the calling mod; erasing public ones removes them all [documented:
  AuxiliaryVariableEraseAll]. That fits the runtime-compiled probes: such a script belongs
  to no plugin, so nothing set it apart [hypothesis: a script with no plugin sees every
  variable as its own]. Still open: what makes a variable public rather than private (the
  name rule); the *Auxiliary Variables* page or a GECK-compiled probe from a plugin
  settles it.
- Temporary means not written to the save [documented by the class names; persistence
  not probed: it needs a save and a reload].

## 2. RefMap arrays

A RefMap is a named map whose keys are forms or references (the calling reference, or
the base form given last) and whose values are a float, a form or a string
[documented: RefMapArrayGetType].

| Function (alias) | Behaviour |
|---|---|
| RefMapArraySetFloat (RefMapSetFlt), …SetRef, …SetString | sets the key's value, creating the entry [documented: RefMapArraySetFloat] |
| RefMapArrayGetFloat (RefMapGetFlt), …GetRef, …GetString | the key's value; 0 when the map, the key or the type does not match [documented: RefMapArrayGetFloat] |
| RefMapArrayGetType (RefMapGetType) | 0 none, 1 float, 2 form, 4 string [documented] |
| RefMapArrayGetSize (RefMapSize), RefMapArrayGetKeys (RefMapKeys), RefMapArrayGetAll (RefMapGetAll) | count, keys array, everything [documented: their pages] |
| RefMapArrayGetFirst (RefMapFirst), RefMapArrayGetNext (RefMapNext) | a two-element array [key, value]; Next continues from the last First/Next call and gives ar_Null after the last pair [documented: RefMapArrayGetNext] |
| RefMapArrayErase (RefMapErase), RefMapArrayDestroy (RefMapDestroy) | removes a key / the whole map [documented] |
| RefMapArrayValidate (RefMapValidate) | removes keys whose references no longer exist and returns the new size [documented] |

Observed: set/get 4.5; two sets on the same key keep one entry (player twice plus Doc
Mitchell = size 2); GetType of a string 4; missing type and float 0; First's key is the
player (00000014); Keys has 2 entries; First has 2 elements, and **Next after the last
pair gives ar_Null, whose ar_Size is -1**; a destroyed map has size 0; Validate on a
live key returns 1 [observed: refmap.*]. The wiki's own loop example uses a `*` name
("*SomeRefMap") [documented: RefMapArrayGetNext], so the prefix rule of §1 probably
applies to RefMaps too [hypothesis].

## 3. For NVForever (J5)

- One store per save, per owner form ID, per lower-cased name. Keep the `*`
  (temporary) entries out of the save, and save the others in NVForever's own save
  section. Never write JIP's co-save: NVForever writes no files the original game reads
  (install-layout.md).
- Iteration order of RefMap keys, GetAll and GetKeys is not documented. NVForever uses
  insertion order [decision; not observed: each probe used one or two keys].
- The not-probed setters AuxiliaryVariableSetFromArray, AuxiliaryVariableEraseAll and
  RefMapArrayErase follow their pages.
