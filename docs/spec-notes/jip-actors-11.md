# JIP LN NVSE, batch 11: actors (2026-11-09, #1006)

Combat allies and targets, detection, packages, the actor's traits (alpha, gravity, light,
breath, movement, velocity, leveled list, unarmed effect), base actor values of a base
form, kill XP, radiation at an NPC, falls, active and temporary effects and effect lists,
the equipped items, armour DR and DT by condition and whether a hit got through, base
faction ranks, ClearDeadActors, the hardcore stages' effects and the weapon condition
penalty, in `crates/fnv-game/src/nvse_jip11.rs`. Sources: the GECK wiki export, each
function's page [documented: the page named]. The probe set `jip11`
(`tools/capture/jip_probes.py`, lab `L5.ext.jip11`) asked the original what the
[hypothesis] values below leave open; its answers are in §6.

40 functions; JIP 142 → 102 [observed: the extender_census test, 2026-11-09].

List getters with nothing to give return no array, not an empty one [observed: the jip10
probes, 2026-11-08, for JIP's list getters; hypothesis: the same here].

## 1. Combat, detection and packages

| Function | NVForever |
|---|---|
| GetCombatAllies | for an actor in combat, the others of its combat group (`Game::combat_group`: those fighting a foe it fights) [documented; hypothesis: the group is the game's allies] |
| GetCombatTargets | for an actor in combat, its targets now (`awareness` targets) [documented] |
| GetDetectedActors | the actors whose latest detection by it is in the Detected band, as GetDetected [documented; hypothesis: the same test] |
| GetDetectionValue `target` | the latest detection value of the pair, rounded [documented: int]; -100 for a pair not measured [documented: the value a left cell sets; hypothesis: the same for one never measured] |
| ForceActorDetectionValue `[value] [asModifier]` | the calling actor's detection value by anyone is the value, or the value plus it as a modifier; none or 0 gives it back to the rules [documented]. Applied in `Game::detect`; the band follows the value |
| GetTargetUnreachable | 1 while a package the actor could not reach is set aside (`Game::block_package`) [hypothesis: NVForever's form of "unable to find a path"] |
| GetCurrentStablePackage | the package its list chose (`Game::current_package`) [documented] |
| GetInterruptPackage | 0 [decision: NVForever's interrupts (combat, dialogue, flight) are states, not package forms] |

## 2. The actor's traits

| Function | NVForever |
|---|---|
| GetActorAlpha | SetActorAlpha's alpha, 1 opaque [documented] |
| GetActorDiveBreath, SetActorDiveBreath `seconds` | the player's breath left, a full breath above water [observed: 10 on land]; the engine passes it each frame (`Game::sync_player_breath`) and takes a script's seconds back while under water, capped at the full breath [documented]; 0 for NPCs [decision: NVForever's NPCs do not drown] |
| GetActorGravityMult | SetActorGravityMult's multiplier; 0 for an actor no script set [observed: Doc Mitchell 0, against the page's "1.0 by default"] |
| GetActorLeveledList | the LVLN or LVLC the reference was placed from (NVForever keeps it as the base), else 0 [documented] |
| GetActorLightAmount | the light level at the actor, 0-100, as the last detection round measured it (`Situation::light`) [documented: the range]; 0 for an actor not measured [hypothesis] |
| GetActorMovementFlags | forward/backward/left/right 1/2/4/8, turning 16/32, walking 256, running 512, sneaking 1024, swimming 2048 [documented: the table]; jumping, flying, falling, sliding 0 [decision: not known game-side] |
| GetActorUnarmedEffect, SetActorUnarmedEffect `[effect] [baseForm]` | the EITM of the base supplying the actor's effects; the setter takes an object or actor effect [hypothesis: anything else refused], for the session; the engine's unarmed attacks read it (`Game::unarmed_effect`) |
| GetActorVelocityAlt `x y z [local]` | the velocity on the world axes, or on the actor's own (x right, y forward) [documented]; the actor is followed from the first call, which reads 0 [hypothesis], as GetActorVelocity |
| GetBaseActorValueAlt `av [baseForm]` | the base value of the reference, or of the base form through its first placed reference, 0 for a base placed nowhere [hypothesis: NVForever computes base values on references] |
| GetKillXP | the XP a kill gives (`Game::kill_xp`), before the perks' adjustment [documented] |
| GetRadiationLevelAlt | rads a second after Rad Resistance: the player's as GetRadiationLevel, an NPC's from the radiation markers and zones at its place [documented: any NPC; hypothesis: no water for NPCs] |
| GetFallTimeRemaining | 0 [decision: the time to the ground needs the ground below a falling actor, which only the engine's physics knows] |
| ResetFallTime | the engine measures the fall again from where the actor is (`nvf_play::falls`) [documented] |

## 3. Effects

| Function | NVForever |
|---|---|
| GetActiveEffects `[filter]` | the magic items with effects on the actor, once each, in the order they took; 1 object effects (ENCH), 2 actor effects (SPEL), 4 ingestibles (ALCH, INGR), summed [documented: the filter; hypothesis: the elements are the items] |
| RemoveNthEffect `item index` | the entry leaves the item's list (applying it and GetNumEffects skip it), for the session; the new count, -1 for no such entry or the last one [documented] |
| RemoveNthTempEffect `index` | the index-th of GetTempEffects' list is dispelled [documented] |

## 4. Equipped items and armour

| Function | NVForever |
|---|---|
| GetEquippedEx `list` | the first of the list's items equipped [documented] |
| GetEquippedWeaponPoison | 0 [decision: NVForever puts no poison on weapons] |
| IsEquippedWeaponScoped | 1 DNAM "Has Scope" without "Scope from Mod", 2 an Increase Zoom mod attached, 0 [documented] |
| IsEquippedWeaponSilenced | 1 VNAM Sound Level "Silent", 2 a Silence or Suppressor mod attached, 0 [documented; hypothesis: "silenced by default" is the sound level] |
| TestEquippedSlots `armor` | whether its BMDT biped slots overlap those of an armour the actor wears [documented] |
| SetNoUnequip `flag` | on the inventory reference of an equipped item: the world's NoUnequip lock (saved with the world), or off [documented] |
| GetArmorEffectiveDR, GetArmorEffectiveDT | DNAM DR or DT times `1 − (0.5 − condition) × penalty` below half condition (SetArmorConditionPenalty's penalty, else 1), of an inventory reference or a placed armour [documented: SetArmorConditionPenalty's formula] |
| GetArmourPenetrated | whether the actor's last hit's damage before armour was above its DT; the engine tells it before the hit is dealt (`Game::note_armour_hit`), so OnHit reads this hit's [documented] |
| SetConditionDamagePenalty `mult` | the weapon damage penalty below 75 % condition, `1 − (0.75 − condition) × mult` [documented]; the engine's damage rules take it (`Rules::condition_damage_penalty`) |

## 5. Bases, the dead and hardcore

| Function | NVForever |
|---|---|
| SetBaseFactionRank `faction rank [baseActor]` | the rank of the base supplying the factions, for the session [hypothesis: not saved]; the references' ranks, factions and GetBaseFactionRank read it; below 0 out [hypothesis: as SetFactionRank] |
| ClearDeadActors `[keepRespawning]` | every dead actor in the loaded cells is disabled, except a quest-item base, one carrying a quest item, and (1) one that respawns [documented] |
| GetHardcoreStageEffect, SetHardcoreStageEffect `stage [effect]` | the stage record's actor effect; the setter's (none: no effect [hypothesis]) is what the player gets from the next change of stage [hypothesis] |

## 6. The original's answers (probe set jip11, 2026-11-09)

The observer session ran set `jip11` on the probe save (nvfairport): 28 rows, all compiled,
no crash; lab `L5.ext.jip11` 17 of 28 at 588937cc4 [observed]. Followed since:
GetActorGravityMult's 0 and GetActorDiveBreath's full breath on land. The probes of the
form getters now go through GetFormIDString (the collector and fnvtool wrote a form
differently). Confirmed: GetActiveEffects' elements are the magic items (an armour's
object effect first); out of combat the list getters give no array; GetInterruptPackage,
GetActorLeveledList and GetActorUnarmedEffect on Doc gave none, as ours. Not matched, and
why: the player's light (100 outdoors in the save; the candidate runs no detection
rounds), player→Doc detection -28 (likewise), the player's active effects (the save's
worn jumpsuit and Concussion), Doc's stable package (the save's schedule), the fall time
on the ground (-0.004, the original's own residue).

## 7. Follow-ups

- GetInterruptPackage: package forms for NVForever's combat, dialogue and flight states.
- GetFallTimeRemaining: the engine could pass the ground's distance and the fall speed.
- GetEquippedWeaponPoison: poisons on weapons.
- GetActorMovementFlags: jumping, falling and sliding from the engine's character state.
- RemoveNthEffect: the other readers of effect lists (GetNthEffectBase and the HUD) still
  see the record's list.
