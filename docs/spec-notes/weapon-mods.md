# Spec note: weapon mods

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own.

A weapon can take up to three mod items (IMOD records); the player attaches one from the
Pip-Boy's ITEMS page ("Mod X)"), which uses the item up and changes the weapon's figures
and its model. Code: `crates/fnv-combat/src/data.rs` (`WeaponStats::modded`, the slots),
`crates/fnv-sim/src/inventory.rs` (`Extra::mods`, `Inventory::set_mods_one`),
`crates/fnv-actor/src/records.rs` (`Weapon::model_for`), `engine/nvf-play/src/pipboy/
item_mod.rs` (the menu), `engine/nvf-play/src/combat.rs` (the player's modded figures),
`engine/nvforever/src/player_body.rs` (the modded model in hand).

## 1. Sources

- **The game's interface file** in `Fallout - Misc.bsa`: `menus\item_mod_menu.xml`
  ("Pipboy mod menu. Used within Inventory Menu."), with the shared pieces it includes
  (`list_box.xml`, `list_box_template.xml`, `card_info.xml`, `meter.xml`,
  `text_box.xml`, `bracket_vert.xml`, `vertical_fade_line.xml`, `line.xml`), and the
  inventory's `menus\main\inventory_menu.xml` (its Mod button). Described here in plain
  words; nothing of them is shipped.
- **The GECK wiki** (owner's export, `C:\fnv-private\geckwiki`): "Weapon Mods", "Item
  Mods", GetMenuTargetRef, GetEquippedWeaponModFlags, SetEquippedWeaponModFlags,
  WeaponHasModType, SetItemModFlags.
- **xEdit's record definitions** as the schema has them (`schemas/fnv/objects.json`, WEAP
  and IMOD).
- **FalloutNV.esm**, read with `fnvtool esm decode` (Varmint Rifle, 10mm Pistol, their
  mods) and the in-game help (MESG `HelpPipBoyItems`).

## 2. The records

**IMOD (Item Mod)** [documented: schema, xEdit IMOD]: the item fields every carried item
has (EDID, OBND, FULL, MODL, ICON, DESC, sounds YNAM/ZNAM) and `DATA` value (u32) and
weight (f32). "Weapon Mods have no effect in and of themselves - the only data they have
is text description. Multiple different weapons can therefore use the same mod and grant
different bonuses" [documented: GECK wiki Weapon Mods]. FalloutNV.esm has 50 [observed];
`ModNVVarmintRifleExtMags`: FULL "Varmint Rifle Extended Mags", DESC "Increases
ammunition capacity (+3).", ICON `interface\icons\pipboyimages\items\
items_mod_vrifle_mags.dds`, value 75, weight 0 [observed]. A weight on an item mod
crashes the original when the mod is loaded into the world [documented: GECK wiki Item
Mods].

**WEAP, the mod fields** [documented: schema, xEdit WEAP]:

| Field | Meaning |
|---|---|
| `WMI1`, `WMI2`, `WMI3` | the IMOD for slot 1, 2, 3 |
| `DNAM` `effect_mod1..3` | the slot's effect (enum `WeaponModEffect`, below) |
| `DNAM` `value_a_mod1..3`, `value_b_mod1..3` | the effect's two values (f32) |
| `DNAM` `reload_animation_mod` | a reload animation named "(Mod)" |
| `MWD1`…`MWD7` | third-person model paths: Mod 1; Mod 2; Mod 1 and 2; Mod 3; Mod 1 and 3; Mod 2 and 3; Mod 1, 2 and 3 |
| `WNM1`…`WNM7` | first-person model forms (statics), the same seven combinations |
| `WMS1`, `WMS2` | "Sound - Mod 1 - Shoot 3D / Shoot Dist", "Sound - Mod 1 - Shoot 2D" |

- The seven model fields run in the order of the flags as a number: field N is the model
  for the mods whose flags (§3) add up to N [observed: the names above, mask 3 = mods 1
  and 2 = `MWD3`; the Varmint Rifle's `MWD1` `VarmintRifleExt.NIF`, `MWD2`
  `VarmintRifleScp.NIF`, `MWD3` `VarmintRifleScpExt.NIF`, `MWD4` `VarmintRifleSil.NIF`,
  `MWD7` `VarmintRifleScopedSilencedExt.NIF`, its mods 1 extended magazines, 2 scope, 3
  silencer]. "Mods are implemented by swapping the model, not by adding components ...
  8 models for 3 mods" [documented: GECK wiki Weapon Mods].
- A weapon with no `WNAM` draws its `MODL` in first person; with mods on it draws the
  combination's `MWD` model there when it has no `WNM` form for it [observed: a mod's
  weapon, `CM4A1` from MMP, has `MWD` models and no `WNAM`/`WNM`, and showed its mods in
  third person only; hypothesis: the original shows them in both views]. A weapon with a
  `WNAM` but no `WNM` for the combination keeps its plain first-person model.
- The 10mm Pistol [observed]: slot 1 `ModNV10mmPistolExtMags` Increase Clip Capacity 4,
  slot 2 `ModNV10mmPistolLaserSight` Decrease Spread 0.4, slot 3 `ModNV10mmPistolSilencer`
  Silence; clip 12, min spread 0.8, `reload_animation_mod` ReloadA. The Varmint Rifle:
  Increase Clip Capacity 3, Increase Zoom 15, Silence; `reload_animation_mod` ReloadM.
- The `WMS` sounds name mod 1, yet both weapons' silencers are mod 3 and their `WMS1` is
  a `…ModFire3D` sound [observed: Varmint Rifle `WMS1` `WPNRifleVarmintModFire3D`]
  [hypothesis: the mod sounds play once a Silence or Suppressor mod is on, whatever its
  slot]. Not built.

**The effects** (`WeaponModEffect`, 1–16) [documented: schema enum; GECK wiki
WeaponHasModType's list], with what the values do [documented: GECK wiki Weapon Mods,
"Weapon Mod Effects"] and what NVForever does with them:

| Effect | GECK wiki | NVForever (`WeaponStats::modded`) |
|---|---|---|
| Increase Weapon Damage | A: flat bonus to damage | `base_damage + A` |
| Increase Clip Capacity | A: flat bonus to clip size | `clip_size + A` |
| Decrease Spread | A: flat reduction of min spread | `min_spread − A`, not below 0 |
| Decrease Weight | A: flat reduction of weight | `weight − A`, not below 0; the carried weight counts it too (§3) |
| Increase Max Condition | A: flat bonus to max condition | `health + A`; condition stays a fraction of it |
| Increase Rate of Fire | A: extra multiplier, 0.2 = 20 % faster | `fire_rate`, `attack_shots_per_sec` and `attack_mult` × (1 + A) [hypothesis: which figures "rate of fire" means] |
| Increase Zoom | A: flat value subtracted from the zoom field of view | `sight_fov − A`, not below 1 |
| Split Beam | A: projectiles added (negative: taken); B: min spread multiplier (1 keeps it); "the 30% bonus the modification already applies" | `projectile_count + A` (at least 1), `min_spread × B` when B ≠ 0; the 30 % is not applied [hypothesis: the wiki does not say what it applies to] |
| Silence | no values; silences the weapon | `sound_level` "Silent" [hypothesis: the weapon's own noise, the wiki calls it untested] |
| Suppressor | (no description) | as Silence [hypothesis] |
| Increase Projectile Speed | A: multiplier of the projectile speed | the shot's speed × A, not for hitscan projectiles ([weapon-data-fields](weapon-data-fields.md) §5) |
| Regenerate Ammo (shots / seconds) | shots: every 4th shot is free; seconds: A added to the recharge time | shots: the player's every fourth shot uses no ammunition; seconds: `regen_rate + A` on a recharging weapon, at least 0.1 s (weapon-data-fields.md §5) |
| Decrease Equip Time, VATS Bonus | (VATS Bonus "unused in game") | kept, not applied (no master mod uses either) |

- The "(Mod)" reload animation is used once a mod with Increase Clip Capacity is on
  [hypothesis: the magazine is the part the reload handles]; NVForever swaps
  `reload_anim` for it then.

## 3. What a weapon carries: the mod flags

- Which mods a weapon has on is three bits: 1 mod 1, 2 mod 2, 4 mod 3 [documented: GECK
  wiki GetEquippedWeaponModFlags, SetEquippedWeaponModFlags]. The flags belong to the
  weapon in the inventory, not the base form [documented: GetWeaponRefModFlags /
  SetItemModFlags work on "this inventory reference's item"].
- NVForever keeps them in the unit's extra data (`Extra::mods`, beside condition, owner
  and equipped), so units with different mods are separate groups, saved with the
  inventory (the save's inventory is the simulation's serde form, `mods` written only
  when not 0) [design].
- "Attached Weapon Mods are effectively destroyed and merged into the weapon itself - they
  do not run their scripts, and cannot be detected using the GetEquipped function"; a
  mod's OnDrop and OnEquip blocks do not run when it is attached [documented: GECK wiki
  Weapon Mods]. NVForever removes one unit of the mod item without running anything
  [design, as documented]. In the original a mod cannot be taken off again [hypothesis:
  the menu offers no way; the wiki names no function but NVSE's
  SetEquippedWeaponModFlags]; NVForever's mod menu takes one off and gives one unit of
  the mod item back, nothing run either [design: the owner's rule, 2026-10-27].
- A modded weapon's row in the inventory lists reads its name with " +" [hypothesis: the
  GECK wiki's SetItemModFlags calls the flag "the inventory '+'"]; units with other mods
  are their own rows.
- A row's actions take that row's units: equipping, using, dropping, moving to a
  container and putting up for barter pick a unit with the row's condition and the row's
  mod flags (`fnv_sim::Pick`), so a modded rifle's row equips or drops the modded rifle
  and the plain row a plain one [design: the owner's rule for rows at one condition,
  2026-10-16, carried over to mods]. Units put up for barter are kept by item,
  condition and mods.
- The carried weight (the inventory's total, encumbrance, `GetAV InventoryWeight`)
  counts a modded weapon at its weight after its Decrease Weight mods, as the figures do
  (`weight − A` per mod on, not below 0) [hypothesis: the effect lowers the weapon's
  weight, the one the inventory adds up; not captured]. The simulation reads the effect
  and its value from WEAP `DNAM` itself (`Item::weight_mods`), as `fnv-combat` does,
  since the inventory's crate sits below combat's.
- **In the world.** A dropped (or script-placed) weapon reference keeps its unit's extra
  data, mods included (simulation.md §5), and a modded one draws the third-person model
  for its flags (`MWD1`…`MWD7`, `Weapon::model_for`), the plain world model when the
  record has none for that combination [hypothesis: "Mods are implemented by swapping the
  model" (GECK wiki Weapon Mods) holds for every view of the weapon; the `MWD` fields are
  the third-person ones]. An NPC holding a modded weapon (the unit it has equipped, else
  the one it would take) draws the same model in its hand [hypothesis, as above]. Its
  shots use the plain figures (only the player's weapon is modded in combat yet).

## 4. The ITEMS page's Mod hint and key

- `IM_ModButton` [documented: inventory_menu.xml]: text `sInventoryMod`, PC key X
  (`_PCButton_X`), 47 under the Repair button, shown whenever the Repair button is.
  The help: "Repair / Mod: These options take you to the Repair and Mod submenus, where
  you may repair damaged equipment or attach weapon modifications, respectively"
  [documented: MESG `HelpPipBoyItems`].
- NVForever shows it for a weapon with mod slots [hypothesis], always bright, since the
  menu takes mods off as well as putting them on [design: the owner's rule, 2026-10-27].
  X on a weapon with mod slots opens the mod menu; on anything else, and with Shift, X
  does nothing; a right click drops [design: owner 2026-11-02, #1393; pipboy.md §6].
  The original's controller button for Mod is LB (`&xbuttonlb;`) and for Drop X
  (`IM_DropButton`, `&xbuttonx;`) [documented: inventory_menu.xml]; NVForever's
  controller follows both (`pad::menu_buttons`, controller.md §3).

## 5. The mod menu

The menu is MenuMode **1061** [documented: GECK wiki GetMenuTargetRef, "1061 The weapon
being modded (Inventory Reference)"]. NVForever counts it among the Pip-Boy's codes, so
`MenuMode 1` holds under it [hypothesis, as for the repair list's 1035].

**What the file says** [documented: item_mod_menu.xml], in the Pip-Boy's menu rectangle
(855 x 650 at (50, 50), as the inventory's):

- Headline 50 high: the inventory's title (`sInventoryItems`, font 4) between lines as on
  every page, and `IMM_ItemName` (font 4, right-justified) from x 200 to 100 short of the
  rectangle's right edge: the weapon's name [hypothesis: the name; the file calls it
  "Player skill info display", copied from the repair menu].
- The list `IMM_ItemModList` at (−15, 75), 430 x 468, with brackets at 33 and 422 round
  the selected row. Rows (`IMM_ItemModListTemplate`): 32 in, 393 wide, the name in font 2
  at (40, 20) wrapped at 270, the equipped square 25 x 25 at (15, 17.5); rows make no
  sound on pointer or click (`&nosound;`). A row trait `_IsItemModItem` is true for "this
  item" [documented; hypothesis: as the repair list, the first row is the weapon and the
  others the mods].
- The picture `IMM_ItemIcon` 200 square at (450, 20) [hypothesis: the chosen row's
  picture, the weapon's on the first row]; `IMM_ModDesc` at (450, 220), wrapped at 350,
  100 high: the mod's description (IMOD DESC) [hypothesis: "Desc"].
- The comparison `IMM_CompareRect` from x 415 (the list's right edge), y 400, 427 x 180:
  two cards half its width, 60 high, at the top (condition with a 60 x 16 meter 15 from
  its right edge, and a stat), a down arrow 80 square
  (`interface\shared\arrow\repair_arrow_down.dds`) in the middle 5 up, gains in font 4 at
  a quarter and three quarters, 30 under the arrow's top, the two cards after the change
  at the bottom; a `IMM_ChooseItemText` (font 2, centred, wrapped at 400) "set in code".
- Buttons from (855, 75), right-aligned: the Mod button (Xbox only) and `IMM_CancelButton`
  `sExit` with the PC key E (`_PCButton_E`). NVForever writes "Exit E)" [hypothesis: the
  word, as the companion wheel's].

**NVForever's menu** [design unless tagged]:

- The first row is the weapon (boxed always), then one row per mod on it (marked with
  the equipped square) and per carried mod that fits a slot whose mod is not on yet, by
  name, with the count in brackets when more than one is carried. Under each mod's name
  its effect in words ("Ammunition capacity + 4", "Weight - 1.5", "Rate of fire + 20%",
  "Silenced": `item_mod::effect_text`) [design: the owner's picture of a modded menu,
  2026-10-27]. Under "Exit E)" on a mod: "Attach Enter)" or "Remove Enter)", clickable.
- On the weapon's row: its picture and the top cards (its condition, and DAM). On a mod's
  row: the mod's picture and description, the top cards for the weapon as it is (CND and
  the figure the effect changes: DAM, Clip, Spread, WG, CND max, Rate, Zoom, Beams), the
  arrow, the change in font 4 at three quarters ("+3"), and the bottom cards after
  attaching the mod, or after removing it for one that is on.
- Keys: Up/Down (W/S) choose, Enter or Space attaches or removes the chosen mod, E or
  Esc goes back. The pointer as in the repair list: a row under it is chosen, a click on
  the chosen mod attaches or removes it, a click on "Exit E)" goes back. The menu stays
  open after attaching or removing, on the weapon as it is now with the same mod chosen,
  until the player leaves it (or closes the Pip-Boy) [design: the owner's rule,
  2026-10-27].
- Attaching: one unit of the mod item leaves the inventory, the weapon unit (the
  equipped one first, as the repair list takes it) gets the slot's flag, the ITEMS list
  is refreshed. Removing: the flag comes off and one unit of the mod item comes back.
  If the player is holding that weapon, its figures and model change at once
  (`crate::combat`, the player body). Sound: `UIRepairWeapon` [hypothesis: no mod sound
  is named in the masters' UI sounds; the menu is the repair menu's sibling].
- Log lines: `pipboy mod: open <weapon>: <mods>`, `pipboy mod: <weapon> + <mod> (slot n,
  flags m)`, `pipboy mod: <weapon> - <mod> (slot n, flags m)`, `pipboy mod: cancel`, `pipboy mod: clicked row <n>`.
- Test aid: the Pip-Boy aid's `mod=open|down|up|use|cancel` (`test_aid.rs`).

## 6. Not built

- An NPC's modded figures in combat.
- Built 2026-11-06 (#982): **a modded weapon's value** is the weapon's value at its
  condition (barter.rs, the GECK wiki's "Item Condition And Value") plus the IMOD DATA
  value of each attached mod, which the condition does not lower [observed: campaign
  o26_value, 2026-11-06, `nvfairport`, ShowOff's GetCalculatedItemValue on the player's
  inventory reference, mods set with JIP SetWeaponRefModFlags and condition with
  SetItemRefCurrentHealth: a Varmint Rifle (value 75; Extended Mags 75, Night Scope 125,
  Silencer 100) read 75, 150, 200, 175, 275, 375 with mods 0, 1, 2, 4, 3, 7 at full
  condition; 26, 9, 49 without mods at 50, 25, 75 %; 326, 309, 349 with all three]. The
  page of GetCalculatedItemValue says it returns what the screen shows; the barter
  screen itself was not read [hypothesis that they agree]. NVForever: `Row::mods_value`
  (`FormStore::mods_value`, WEAP WMI1-3) added in `Row::worth`, which the barter prices,
  the item card and the totals read.
- Equip time (no master mod has it) and VATS bonus (unused in game). Increase Projectile
  Speed and both Regenerate Ammo effects are built (weapon-data-fields.md, 2026-10-21).
- Scripts' view: GetWeaponHasMod-style functions are NVSE/JIP (not vanilla) and not
  built. GetMenuTargetRef 1061 answers the weapon being modded and GetSelectedItemRef
  the highlighted mod (or the weapon), as inventory references (ext-menus.md).

## 7. The mod shoot sounds (ESM gap audit row 12)

- **The records** [documented: xEdit wbDefinitionsFNV WEAP; schemas/fnv `objects.json`]:
  `WMS1` "Sound - Mod 1 - Shoot 3D / Shoot Dist" (one or two form IDs in a row, as the
  plain `SNAM` pair) and `WMS2` "Sound - Mod 1 - Shoot 2D". The GECK's Mod Info tab names
  them "Attack Sound(s): third-person attack sounds (close and distant) used when either
  a Suppressor or a Silencer weapon mod is equipped" and "Attack Sound (2D): first-person
  attack sound used when either a Suppressor or a Silencer weapon mod is equipped"; the
  Silence effect "sets the weapon's Sound Detection Level to Silent and switches to a new
  set of firing sounds specified on the Mod Info tab", Suppressor the same with Normal
  [documented: GECK wiki Weapons, Mod Info tab and mod effects]. JIP's GetWeaponSound
  numbers them 9 "Attack Sound 3D (Mod)", 10 "Attack Sound Dist. (Mod)", 11 "Attack
  Sound 2D (Mod)" [documented: GECK wiki GetWeaponSound].
- **The masters** [observed, FalloutNV.esm and the add-ons, 2026-10-21]: `WMS1` carries
  one form ID only (no distant mod sound anywhere); e.g. `Weap10mmPistol` WMS1
  `WPNPistolSilenced22Fire3D`, WMS2 `WPNPistolSilenced22Fire2D`; the varmint rifle,
  sniper rifle, 12.7mm pistol and SMG `WPNRifleVarmintMod*`; Honest Hearts' .45 pistol
  `WPNPistol45Silenced*`. The grenade rifles also carry WMS1/WMS2 (the silenced 10mm's),
  a weapon whose mods are not Silence or Suppressor, so the sounds are not switched by
  any mod. No weapon has a mod looping sound. The weapons silenced from the start (the
  silenced .22 pistol `WeapNVSilenced22Pistol`, the silenced .22 SMG
  `WeapNVSilenced22SMG`) carry their quiet sounds in the plain `SNAM`/`XNAM` and no
  `WMS1`/`WMS2`, so they need nothing more.
- **The rule** (`fnv_combat::fx::WeaponFx::shoot_sounds`): a weapon unit with a mod on
  whose effect is Silence or Suppressor (`WeaponStats::sound_mod_on`) shoots with the mod
  set, when the record has one: 3D `WMS1` first, distant `WMS1` second, 2D `WMS2`; any
  other unit with the plain set (`SNAM` first, `SNAM` second, `XNAM`) [documented, as
  above]. With the mod set, a missing 2D falls back to the mod 3D and the reverse
  [hypothesis, as the plain set's 2D-else-3D]; a silenced weapon whose record has no
  `WMS1`/`WMS2` at all keeps its plain sounds [hypothesis: nothing else to play]; a
  silenced weapon has no distant sound when `WMS1` has no second entry (all the masters')
  [hypothesis: a silenced shot is not heard from afar; the plain distant boom would give
  it away]. The looping sound (`NAM7`) is the same with or without the mod
  [hypothesis: no mod loop field exists].
- **Who hears which**: the player's own shot plays the 2D sound (else the 3D), as before
  (combat-rules.md §16); another actor's shot plays the 3D sound at the muzzle, or its
  distant sound when the player's eye is at or beyond the 3D sound's maximum
  attenuation distance [documented: GECK wiki Weapons, "Attack Sound (Dist): the sound
  form played when the player is outside the Maximum Attenuation Distance of the 3D
  sound"] (the medium list's "WEAP Shoot Dist", built with this row). An NPC's mods are
  those of the unit it has equipped, else the one it would take
  (`Inventory::mods_in_use`), so an NPC with a silenced pistol sounds silenced
  [hypothesis: the same rule as the player's; its other figures stay plain, §3].
- **Log**: `combat: <weapon> shoots its silenced sound <SOUN>` once each time the
  player's sound changes to a mod sound [design], and at debug level each NPC
  shot's choice (`combat: npc <ref> shot sound <SOUN> (silenced|plain, 3D|distant)`).

