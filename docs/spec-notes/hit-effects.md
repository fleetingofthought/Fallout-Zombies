# Hit effects: fire and poison

What a hit applies besides its damage: a weapon's object effect (the flamer's burning), a
blast's (the incinerator's), a creature's poison (a cazador's or a radscorpion's sting).
Implementation: `fnv_game::Game::apply_hit_effect` (the rules below), nvf-play
`combat/hit_effects.rs` (which hit applies what, the log), the drain by the second is the
aid items' (`crate::aid`, aid-items.md §3); the look is the effect shader's
(effect-shaders.md §3, §6). Evidence tags as in the charter.

## 1. What carries an effect

- **WEAP `EITM`** "the object effect (ENCH) applied on a hit" (combat-data.md §1.1)
  [documented: xEdit]. In FalloutNV.esm [observed, `fnvtool combat form`, 2026-10-16]:
  `WeapFlamer` 0x0000432D and `WeapShishkebab` 0x0000434E name `EnchFlamerEffect`
  0x00044B30 ("Burning").
- **EXPL `EITM`**: the object effect applied to what the blast catches [documented:
  xEdit, combat-data.md §1.4]. `WeapNVIncinerator` names no EITM; its projectile's
  explosion `NVIncineratorFireBurst` 0x001519F0 names `NVEnchIncineratorEffect`
  0x001519EF; the incendiary grenade's `GrenadeIncendiaryExplosion` 0x0014DDE3 names
  `EnchIncendiaryGrenadeEffect` 0x0014DDDC [observed].
- **NPC_ / CREA `EITM` "Unarmed Attack Effect"** and **`EAMT` "Unarmed Attack
  Animation"** [documented: xEdit field names, schema `gameplay.json`; GECK wiki
  GetActorUnarmedEffect "Returns the actor's Unarmed Attack effect"]. Observed
  (2026-10-16): `NVCrCazador` 0x000E584D: `CazadorPoison` 0x0012A22E, animation "Any";
  `CrRadscorpion1Small` 0x0001CF9D: `RadScorpion1Poison` 0x0004D843, `CrRadscorpion2Large`
  0x0001CF9E: `RadScorpion2Poison` 0x0004D844, `CrBarkScorpion` 0x000EB401:
  `BarkScorpionPoison` 0x000EB402, all three animation "AttackForwardPower". The leveled
  variants (`VCrTier4CazadorMed` 0x0014F3FE, `VCrTier1RadscorpionSmall` 0x0014F482) carry
  the same EITM and EAMT themselves, with the template flag "Actor Effect List" set.
- Ammunition: no AMEF of the masters holds a damage over time; `.50 MG, Incendiary` and
  `40mm Grenade, Incendiary` carry no effects [observed]; not covered here.

Rules here:

- A hit with a weapon applies its object effect; a blow without a weapon (fists, a
  creature's own attack) applies the attacker's unarmed attack effect, read from the
  actor base that supplies its actor effects (template flag "Actor Effect List")
  [hypothesis: EITM belongs to that group]; a blast applies its EXPL's to every actor it
  damages.
- **EAMT**: "Any" goes with every blow. A power attack's group (`AttackPower`,
  `AttackForwardPower`) goes only with that power attack: creatures make power attacks
  (npc_combat.rs, by the combat style's Power Attack % Chance, else a 25 % fallback
  [design: owner 2026-10-16]) with their own clips, the radscorpion's sting being
  `h2hattackforwardpower.kf` and `h2hattackpower.kf` [observed: the base meshes archive],
  so a radscorpion poisons with its forward power attack only [hypothesis from the
  name]. An ordinary group (`AttackLeft` …) is not told apart [design]: every ordinary
  blow applies it, logged once per item (`hit effect: … names the attack …`).
- The weapon's critical effect (`CRDT`, e.g. `FlamerCritEffect02`) is not applied here.

## 2. The effects and the refresh rule

The magic items' effects are ordinary effect items (ENCH or SPEL `EFID` + `EFIT`, combat-
data.md §1.6), started by the target's `Game::apply_magic_item_with` as an ingestible's
are (aid-items.md §3, §6), with the attacker as the caster: a Value Modifier without
Recover and with a duration drains its magnitude a second while it lasts; Script effects
run their blocks. [observed, 2026-10-16:]

| Item | Effects |
|---|---|
| `EnchFlamerEffect` "Burning" | `FlamerFireDamage` 0x000BD474 Health 2/s for 5 s; `FlamerMagicEffect` 0x00044AF7 (script `FlamerEffectScript`) 5 s |
| `NVEnchIncineratorEffect` | the same two, 4/s for 8 s |
| `EnchIncendiaryGrenadeEffect` | the same two, 6/s for 15 s (area 15) |
| `CazadorPoison` (SPEL Poison) | `Poison` 0x00031851 Health 5/s for 30 s *if* the target is the player or Hardcore is on; `ReduceActionPoints` 25 for 30 s; `CazadorPoisonEffect` (script) 30 s; `Poison` Health 18/s for 8 s *if neither* |
| `RadScorpion1Poison` / `RadScorpion2Poison` | `Poison` Health 3/s for 5 s / 8/s for 10 s |

The conditions of CazadorPoison are `GetIsReference player` (function 136) and
`IsHardcore` (586) [observed: the CTDA records; the function numbers from the catalogue];
they run on the target (the effect's subject), as every effect condition here.

`FlamerFireDamage` names the effect shader `Flames01` 0x0005F9B6 with "FX Persist", so the
burn shows for as long as it lasts (effect-shaders.md §3, §6). `FlamerEffectScript`
plays `Smoke01` 0x0005F9B8 (and `Flames01` on a target already dead) at its start and
stops both at its finish (its SCTX, read 2026-10-16).

**Repeated hits** [design]: a hit with an item whose effect is still active on the
target does not start a second one; it starts the active one's time again (the effect
lasts at least the item's duration from this hit; its magnitude does not add up). The
flamer hits eight times a second; the rule ingestibles follow (a second use lengthens the
effect by its duration, aid-items.md §6) would burn for minutes. Whether the original
stacks poisons (separate effects adding up) is not documented; owner review.

## 3. Resistances

MGEF `DATA` "Resistance Type" names an actor value; a hit's harmful effect (Hostile or
Detrimental) loses that share of its magnitude: magnitude × (1 − resistance / 100), the
resistance held to 0–100 [documented for poison: GECK wiki Stats Tab - Creatures, "Poison
Resist: The percentage of poison damage that is ignored by the creature"; hypothesis for
the others: Fire Resistance alike]. A fully resisted effect does not start (and so shows
no shader). `FlamerFireDamage` and `FlamerMagicEffect` name Fire Resistance, `Poison`
Poison Resistance [observed]. The player's Poison Resistance is (Endurance − 1) × 5
(actor-values.md §3).

The hit's own damage keeps the weapon's resistance as before (combat-rules.md).

## 4. Deaths

- Health drained by an effect is damage as a hit's is: an essential actor keeps 1 and
  falls unconscious, anyone else dies at 0 with the effect's caster as the killer, and
  the player gets the kill's XP for its own effects (combat.rs `effect_health_damage`)
  [design: as a hit, `Game::hit`].
- **A death ends the actor's effects** but those whose base effect has the MGEF flag
  "No Death Dispel" (bit 28) and abilities [documented: the flag's name, xEdit;
  hypothesis: what it means]. `FlamerFireDamage` and `FlamerMagicEffect` have it, so a
  corpse burns on for the rest of the burn (the flamer's script plays `Flames01` on a
  target that is already dead, which suggests the original shows burning corpses);
  `Poison` has not, so a poison ends with its target.
- The effects of one hit go on before its damage: a killing hit leaves only those that
  outlast a death.
- A dead or unharmable (child, Invulnerable) target takes no hit effect; the player in
  god mode takes none.

## 5. Log

`hit effect: <item> on <target> by <attacker>: <effect> <±n> <value>/s for <s> s, …`
when an item starts on a target (a repeat that only refreshes it is counted), `hit
effect: <item> on <target> ends (…)` when its last effect ends, `hit effects summary: …`
at exit. Smoke: `flamer-burns-player`, `flamer-burns-npc`, `radscorpion-poison`,
`cazador-poison`, `placeatme-fights`.

## 6. To capture

- How long the flamer's burn lasts after the last hit, and whether a corpse burns.
- The Incinerator's burn (NVEnchIncineratorEffect, magnitude 4 for 8 s) drained NPCs 1
  or 2 Health a second in the original's NCR-Fiend fights, for 16–30 s, on past death
  (npc-behaviour-compare.md §9.1); ours drains 4. A burst on its own drains 4: one
  `NVIncineratorFireBurst` placed by script on Ringo (Fire Resistance 0, Health 120),
  then after a full heal one 100 units in front of him, each hit for 18 and then drained
  exactly 4 a second for 8.0 s (32), whatever the distance; in the first the rate fell
  to about 3 a second from the fourth second while he moved (unexplained)
  [observed: campaign o12_burn, 2026-11-01, issue #973]. So neither the effect nor the
  explosion's distance makes the troopers' 1-2 a second; something of the shot (the
  glob fired by Cook-Cook, its caster or the projectile) is what is left to capture.
  A glob fired by the player burns the same: an unarmed, restrained NCR trooper (Health
  50) 350 units in front, hit by one glob from `player.FireWeapon` on the Incinerator,
  lost about 9.7 at the hit and then **exactly 4 a second for 8.0 s** (32); two globs
  0.2 s apart from the attack key held 60 loops hit for about 9.8 and 8.3, and the burn
  went on at 4 a second (renewed, not doubled) until he died 8 s after the second hit
  [observed: campaign o26_burn, 2026-11-06, `nvfairport`, issue #973]. So the glob, its
  projectile and a player caster all give ours; what is left is an NPC caster (an NPC
  firing at a target crashed the game three times in 2026-11-01 setups). A 2026-11-08
  try (campaign o29_burn, `nvfairport`) gave a spawned NCR trooper the Incinerator, turned
  it to a restrained trooper about 250 units off with SetAngle and called `FireWeapon`
  on it: in both sessions the glob went off at the shooter at once (OnHit on two
  non-actor references and on the god-mode player 100 units behind it), and the target
  kept its 50 Health, so nothing was learned of the burn [observed]. Next: the shooter
  with the player out of the way and no other actor within 300 units of its muzzle, or
  StartCombat on the restrained target instead of FireWeapon.
- Several cazador stings: one poison refreshed, or several adding up?
- Does a radscorpion poison with every attack or only its forward power attack (built:
  only the forward power attack)? How often does it sting?

## 7. Seen in this engine (smoke runs, 2026-10-16)

Not evidence about the original; what the runs showed: Cook-Cook's flamer burned the
player (2/s, refreshed by each later hit) and a fiend standing by; the player's flamer
burned Cook-Cook; a large radscorpion (VERGSRadscorpion5REF, RadScorpion2Poison) drained
the player 6.4/s for 10 s (8 less 20 % Poison Resistance) and another one Victor; a
Novac cazador drained the player 4/s for 30 s with 25 AP (CazadorPoison's player lines).
