# Spec notes: actor values

What the 77 actor values of an NPC, a creature and the player start at, and how scripts
change them. Code: `crates/fnv-game/src/actor_values.rs` (the base values) and
`crates/fnv-game/src/functions/actor.rs` (the pools and the script functions). Tool:
`fnvtool actor values` (§7).

Sources, and what each tag means here:

- **[documented]**: the owner's GECK wiki export (pages ActorValue, Derived Skill
  Settings, Stats Tab - NPC, Stats Tab - Creatures, Class, Settings, Critical Hit Chance
  Formula, Weapon Damage Formula, Confidence, Movement Settings and the actor value
  function pages) or xEdit's record definitions (through `schemas/fnv`).
- **[observed]**: counted in the owner's FalloutNV.esm with `fnvtool actor values
  --survey` (§7), or captured in game where a capture is named.
- **[hypothesis]**: our reading, to be settled by a capture (§8).

The wiki's "Stats List" and "Actor Value Codes" pages and the pages of the single
`fAVD…` settings are not in the export, and the live wiki turns automated readers away.
What they would settle is listed in §8.

## 1. The model

An actor value is a base value and three pools added to it [documented: ActorValue]:

| Part | What changes it | What reads it |
|---|---|---|
| Base | the record or a formula (§2 to §5); `SetActorValue` puts a number in its place | `GetBaseActorValue` |
| Permanent modifier | `ModActorValue`; `ForceActorValue` moves it so the current value is the number asked for; skill books; for the player, abilities without conditions | `GetPermanentActorValue` = base + this |
| Temporary modifier | active effects and worn enchantments ("Value Modifier" effects with Recover) | `GetActorValue` (aid-items.md §3) |
| Damage | `DamageActorValue`, `RestoreActorValue` (never past zero); combat's health and limb damage; effects without Recover | `GetActorValue` = base + both modifiers − damage |

- `DamageActorValue` on Health at 0 or less kills, or knocks out an essential actor
  [hypothesis: unconscious.md §2.2].
- NVForever keeps the base override, the permanent modifier, the temporary modifier and
  the damage. Active effects (aid items, spells cast or added) feed the temporary pool and
  the damage as aid-items.md §3 says; worn enchantments do not yet (the Pip-Boy adds them
  itself), and the abilities in an actor's SPLO list are not applied.
- `SetActorValue` replaces the base only: a modifier and damage stay on top of the new
  number [documented: ActorValue, "the game uses this base value as a starting point …
  it then adds three pools"].
- `ResetHealth` clears the damage of Health and of the seven limb conditions
  [documented: ResetHealth, "restores the actor's health and cures crippling damage"].
- The game calculates with a value kept inside its range: S.P.E.C.I.A.L. 1 to 10, skills
  0 to 100; the stored value may lie outside [documented: GetThresholdedActorValue;
  ModActorValue, "can exceed the maximum cap"]. The formulas below read their inputs
  that way (`actor_values::capped`).
- The formulas read the *current* value of the attribute or skill they use, so damage
  to Charisma lowers Barter while it lasts [hypothesis; the wiki does not say which
  value feeds them]. Health's maximum is the exception: it reads permanent Endurance, as
  it did before this note (combat-rules.md §9).
- `SetActorValue` on a player skill sets the part beside the attributes' formula: the
  value and the base read the number given plus the formula's result. From the console
  and from a script alike; set twice it stays (no compounding); `SetAV 0` leaves the
  formula's part; a `ModAV` stays on top [observed: o26_setav, 2026-11-06, save
  `nvfdoc`, level 1, every skill 17 from the attributes: `player.setav repair 35` read
  52 (GetBaseAV 52), Guns 50 read 67, Science 0 read 17, Barter ModAV +10 then SetAV 35
  read 62 with base 52; gaps6 saw the same, Guns 17 to 67 after SetActorValue 50; issue
  #1575]. This matches the wiki's remark that the player's skill points sit in a SetAV
  override added to the derived base. The points spent go: on `nvfLanius` (level 50,
  Intelligence read 20 and counted as 10, 27 from the attributes) Repair 127 read 62
  after SetAV 35 and Science 127 read 27 after SetAV 0; Guns 142 read 92 after SetAV
  50, 15 more than the rest [observed: o26_setav, 2026-11-06]. Here the 15 is taken
  for Guns' tag bonus (fAVDTagSkillBonus), which stays: SetActorValue sets the number
  plus the formula plus the tag bonus (`functions::actor::set_actor_value`)
  [hypothesis: the save's tagged skills were not read].
- Not modelled: `UnsetAV` and `UnforceAV`.

Where the numbers live: the permanent value of S.P.E.C.I.A.L. is in the Layer 4 world
(`World::actor_value`), because the carry weight rule and the menus read and write it
there; `functions::actor` subtracts the permanent modifier to get the base. Everything
else, and the attributes of creatures (the world holds none for them), is in
`Game::extra` (`actor_values`, `av_permanent`, `av_damage`).

## 2. S.P.E.C.I.A.L. (codes 5 to 11)

- NPC_ DATA and CREA DATA "Attributes", seven bytes in the order Strength, Perception,
  Endurance, Charisma, Intelligence, Agility, Luck [documented: xEdit].
- All 3,816 NPC_ and all 1,578 CREA records of FalloutNV.esm have them [observed].
- An actor whose ACBS template flag "Stats" is set takes them (and the rest of §3 and
  §4) from its template when the template is an NPC_ or CREA [documented: xEdit flag
  name]. A template that is a leveled list is not followed: the actor's own record is
  read [hypothesis; such records hold placeholder values].
- Auto-calculated NPCs keep the attributes their record stores. 2,124 of the 2,283 have
  exactly their class's attributes (CLAS ATTR) and 159 do not [observed]. How the GECK
  auto-calculates them from class and level [documented: Stats Tab - NPC, in words only]
  is not needed while the stored bytes are used.

## 3. Skills (codes 32 to 45)

### 3.1 The formula

`fAVDSkill<Name>Base` + attribute × `fAVDSkillPrimaryBonusMult` + ⌈Luck ×
`fAVDSkillLuckBonusMult`⌉, with defaults 2, 2 and 0.5 [documented: Derived Skill
Settings]. A tagged skill gains `fAVDTagSkillBonus` (15) [documented].

- The masters set the fourteen `…Base` settings (all 2.0) and `fAVDTagSkillBonus`
  (15); they set neither multiplier, so the defaults apply [observed]. Guns reads
  `fAVDSkillSmallGunsBase` (its Fallout 3 name; there is no `fAVDSkillGunsBase`) and
  Survival reads `fAVDSkillSurvivalBase` [observed: the masters' settings].
- Which attribute each skill uses is not in the export. It is read off the data: for
  every auto-calculated NPC whose class does not tag the skill, the formula is tried
  with each of the seven attributes against the stored skill value [observed]:

| Skill | NPCs | Attribute that fits | Share it fits | Next best |
|---|---|---|---|---|
| Barter | 1,923 | Charisma | 100 % | Intelligence 72 % |
| Energy Weapons | 1,945 | Perception | 98.7 % | Luck 50 % |
| Explosives | 1,582 | Perception | 99.1 % | Luck 49 % |
| Lockpick | 2,234 | Perception | 100 % | Luck 52 % |
| Medicine | 2,190 | Intelligence | 100 % | Charisma 72 % |
| Melee Weapons | 854 | Strength | 100 % | Luck 64 % |
| Repair | 2,133 | Intelligence | 100 % | Charisma 72 % |
| Science | 2,063 | Intelligence | 100 % | Charisma 70 % |
| Guns | 367 | Agility | 100 % | Perception 39 % |
| Sneak | 1,946 | Agility | 100 % | Strength 42 % |
| Speech | 2,070 | Charisma | 100 % | Intelligence 69 % |
| Unarmed | 1,439 | Endurance | 97.3 % | Strength 44 % |
| Big Guns | 2,240 | Endurance | 40.8 % | Agility 10 % |
| Survival | 2,239 | Intelligence | 85.6 % | Charisma 61 % |

- Twelve skills are settled by that table. Big Guns (unused in New Vegas) and Survival
  are not: no attribute fits all their stored values. The code uses Endurance for both
  [hypothesis for Big Guns: its best fit]. For the player, Survival follows Endurance
  [observed: the tag-skill screenshot, Endurance 7 and Intelligence 5, Survival tagged
  at 35 = 2 + 14 + 4 + 15, chargen.md §2; the Fallout Wiki's skills page pairs it with
  Endurance too], although the NPCs' stored Survival values fit Intelligence more often
  (85.6 % against 25.6 %). Only the player and creatures are affected (§3.2).

### 3.2 NPCs

- NPC_ DNAM holds fourteen "Skill Values" and fourteen "Skill Offsets", one byte each,
  Barter to Unarmed in actor value order [documented: xEdit]. Every NPC_ has it
  [observed].
- The skill's base is **value + offset** [hypothesis]. What supports it:
  - The wiki says a skill's base "includes any GECK entered offsets" and that
    `GetActorValueInfo` shows "the amount from the actor base (i.e. the amount
    autocalced or entered into the GECK)" [documented].
  - For auto-calculated NPCs the stored value of an untagged skill is the formula's
    result (§3.1), and a tagged skill holds more (Sunny Smiles, level 2: Guns 35 where
    the formula gives 16 and the tag bonus 15) [observed], so the value is what the
    GECK calculated and showed.
  - Offsets appear on 284 NPCs, none of them auto-calculated, 1,550 skill entries in
    all. Value + offset stays at or under 100 in all but 17 of them, and no value
    alone exceeds 100 [observed]. Some sums land on exactly 100 (a stored 77 with an
    offset of 23), which reads as a designer topping a skill up.
- NPCs whose level follows the player's ("PC Level Mult", 268 NPCs, all
  auto-calculated [observed]) would have their tagged skills recalculated for the
  level in the original; here they keep the stored value [hypothesis: open].

### 3.3 Creatures

- CREA DATA "Combat Skill" is the creature's Guns, Big Guns, Energy Weapons, Explosives
  and Melee Weapons [documented: Stats Tab - Creatures, "used for the matching skill"
  of a creature with a weapon; not for its unarmed attack].
- "Stealth Skill" is its Sneak [hypothesis from the field name; every creature of
  FalloutNV.esm has 50, observed]. "Magic Skill" (50 everywhere) is not used.
- The other skills come from the formula and the creature's attributes [hypothesis].

### 3.4 The player

- The formula, plus the tag bonus for the skills in `PlayerState::tag_skills`
  [documented: ActorValue, skills "are derived from a formula"; Derived Skill Settings].
  A new character with every attribute at 5 has 15 in every skill.
- The player record's DNAM (values between 12 and 30) is not read [hypothesis].

## 4. Derived statistics

| Code | Value | Base | Tag |
|---|---|---|---|
| 12 | Action Points | `fAVDActionPointsBase` (65) + `fAVDActionPointsMult` (3) × Agility | names [documented: Settings]; values [observed: masters]; the form [hypothesis] |
| 13 | Carry Weight | `fAVDCarryWeightsBase` (150) + `fAVDCarryWeightMult` (10) × Strength | [observed in game: 200 at Strength 5, 160 at Strength 1, simulation.md §3] |
| 14 | Critical Chance | `fAVDCritLuckBase` (0) + `fAVDCritLuckMult` (1) × Luck | [documented: Critical Hit Chance Formula] |
| 15 | Heal Rate | 0 | [hypothesis]; the masters hold `fAVDHealRateEndurance6Bonus` to `…10Bonus` (5, 5, 5, 10, 10) [observed], unread |
| 16 | Health | the maximum health of combat-rules.md §9 | as there |
| 17 | Melee Damage | `fAVDMeleeDamageStrengthMult` (0.5) × (`fAVDMeleeDamageStrengthOffset` (0) + Strength) | [documented: Weapon Damage Formula] |
| 56 | Unarmed Damage | `fAVDUnarmedDamageBase` (0.5) + `fAVDUnarmedDamageMult` (0.05) × Unarmed | [documented: Weapon Damage Formula] |
| 19 | Poison Resistance | (Endurance − 1) × 5 | "derived from Endurance" [documented: Stats Tab - NPC]; the figures [documented: the Fallout Wiki's derived statistics table, END × 5 − 5; no setting in the masters] |
| 20 | Rad Resistance | (Endurance − 1) × 2 | as Poison Resistance (the table: END × 2 − 2) |
| 21 | Speed Multiplier | ACBS "Speed Multiplier" (percent) | [documented: xEdit; Stats Tab pages, "Speed Mult %"] |
| 22 | Fatigue | ACBS "Fatigue" | creatures [documented: Stats Tab - Creatures]; NPCs [hypothesis: the page calls the field unused] |
| 23 | Karma | ACBS "Karma" of the actor's own record | [documented: xEdit; GetBaseKarma] |
| 46 | Inventory Weight | the weight the actor carries | an actor value [documented: GetCalculatedMaxCarryWeight]; that it is the carried weight [hypothesis] |
| 18, 76 | Damage Resistance, Damage Threshold | 0 | worn armour is added by the engine's combat, outside the actor value |
| — | Companion Nerve | a teammate's damage and armour × (1 + 0.05 × the player's Charisma) | [documented: Fallout Wiki, derived statistics "(CHA × 5)%" and Charisma (Fallout: New Vegas) "+5% to their damage and armor, up to … +50% at 10 Charisma"]; no setting in the masters [observed]; not an actor value. `Game::companion_nerve`; combat-rules.md §2 says where it applies [hypothesis] |
| — | Reload speed | the reload clips play at (0.5 + 0.1 × Agility) × the Reload Speed perks (Rapid Reload × 1.25) | [documented: Fallout Wiki, Agility (Fallout: New Vegas); observed: Rapid Reload's PRKE, entry point 37, Multiply Value 1.25]; no setting in the masters [observed]. `fnv_combat::attack::reload_rate`; combat-rules.md §2 |

Settings not in the load order take the number in brackets: the wiki's default where it
gives one [documented], else the masters' value [observed].

## 5. The rest

- **AI attributes** (0 Aggression, 1 Confidence, 2 Energy, 3 Responsibility, 4 Mood, 57
  Assistance): the AIDT fields; for the enumerated ones the code of the choice
  (Confidence 0 Cowardly to 4 Foolhardy) [documented: xEdit; Confidence for that
  scale; hypothesis: that the actor value is the code]. From the template when the
  ACBS template flag "AI Data" is set.
- **Limb conditions** (25 to 31): 100; combat lowers them (combat-rules.md §9).
- **Everything else** (XP, Paralysis, Invisibility, Chameleon, Night Eye, Turbo, the
  other resistances, Water Breathing, Rad Level, Bloody Mess, Variable01 to 10, Ignore
  Crippled Limbs, Dehydration, Hunger, Sleep Deprivation): 0 until a script sets it.
  Variable01 to 10 have no game function; quests use them as per-actor data
  [documented: ActorValue].
- Something that is no actor has no attributes (reading one is an error) and 0 for the
  rest, 100 for limb conditions.

## 6. Who reads the values

- Scripts and conditions (`GetActorValue` and its family, dialogue's skill checks).
- The player's attacks (`engine/nvf-play/src/combat.rs`): the weapon's skill, Strength,
  Luck and Unarmed, and whatever Melee Damage, Unarmed Damage and Critical Chance hold
  above their formulas. Before this, the skill was a guess from the attribute.
- Hits on an actor: its Agility for knockdown and knockback (creatures now have one),
  and its own Damage Threshold and Damage Resistance on top of its armour.
- Not yet: NPC attacks, Action Points, poison and
  radiation.

## 7. Tool

```
fnvtool actor values --schema schemas/fnv <Data> FalloutNV.esm \
    --actor player --actor SunnyRef --actor DocMitchellREF --survey
```

- `--actor`: `player`, a placed actor, or an actor base (its first placed reference).
  Prints the 77 base values in a game where nothing has happened (`fnv.actor.values/1`).
- `--survey`: the counts of §2 and §3 (the fit table of §3.1, the offsets of §3.2, the
  creature skills of §3.3) and the settings of §4 as the load order has them (`null`
  where it has none).

Checked with it on 2026-10-10 (FalloutNV.esm): Sunny Smiles has Guns, Melee Weapons and
Sneak 35 and Health 100 (90 since the NPC Health form was corrected to the original's,
2026-10-19, combat-rules.md §9); Doc Mitchell has Medicine and Science 33 and Energy Weapons 50
(a stored 16 and an offset of 34); the player has 15 in every skill, 80 Action Points
and 200 Carry Weight.

## 8. To capture

The job `tools/capture/jobs/av_actor_values.steps.json` (written by
`make_actor_value_jobs.py`; unattended, save `nvfdoc`, under a minute) reads the level
and `GetActorValue` and `GetBaseActorValue` of all 77 codes on the player, Sunny Smiles,
Doc Mitchell, Easy Pete, Chet, Ringo, Cheyenne and Victor, then changes three values on
the player. It has not been run yet. Compared with `fnvtool actor values` for the same
actors, it answers:

1. NPC skills: stored value + offset, or the formula + offset (§3.2). Easy Pete tells
   them apart: his stored Big Guns is 5 where the formula gives 12, and his Explosives
   is a stored 14 with an offset of 36.
2. Survival's and Big Guns' attribute on the player (every attribute at 5 will not
   show it; a later job on a character with uneven attributes will); a creature's
   skills with and without a field, on Cheyenne and Victor (§3.1, §3.3).
3. Action Points, Poison and Rad Resistance, Heal Rate, Fatigue of the player and of
   NPCs (§4).
4. Whether the AI attributes read as the enumeration codes (§5).
5. Whether the formulas follow a damaged attribute (§1): Charisma damaged by 2, then
   Barter and Speech.
6. The pools: Barter after `ModActorValue 5` (current, base, permanent), and Carry
   Weight after Strength is modified.
7. `SetActorValue` on a player skill: answered (§1, o26_setav): added to the formula.

Not covered: an NPC whose level follows the player's (§3.2), and the template rule for
actors whose template is a leveled list (§2).
