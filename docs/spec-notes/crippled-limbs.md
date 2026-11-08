# Crippled limbs: what they do

What a crippled limb does to the player and to every other actor, where each rule
comes from, and what NVForever builds. How limbs are hit and crippled (body part
volumes, limb damage, conditions counting down to 0) is combat-rules.md §12. The
crippled-limb picture on the HUD is hud.md §12, and limping is actors.md §8.11.

Combat mechanics are not a parity target (docs/design/combat.md), so where no source
gives a number NVForever chooses one, tagged [design], and docs/owner-review.md lists
them.

## 1. Sources

- **The ESM** (FalloutNV.esm, read with `fnvtool esm decode` from the owner's install,
  2026-10-18) [observed]:
  - MESG `HelpCrippling` (0x000BE8D4), the game's own help text. In summary: the
    effects depend on the limb and "apply to you as well as to your enemies".
    - Head: reduced Perception and a concussion effect.
    - Arms: reduced aiming, and "Enemies will be disarmed at the moment their arm is
      crippled".
    - Chest: a "higher chance to be staggered when taking damage".
    - Legs: reduced run speed, and "Enemies cannot perform leaping or charging
      attacks".
    - Antennae and combat inhibitors: crippling them sends the creature into a frenzy.
  - MESG `HelpHealingLimbs` (0x00072358), in summary: three ways to heal (sleep in a
    bed, find a doctor, or use a Stimpak). In Hardcore only Doctor's Bags and doctors
    heal crippled limbs. A Stimpak used for general healing also slightly heals
    crippling damage, and one can be applied to a limb from the Pip-Boy's Stats screen.
  - MESG `CGTutorialCripple` / `CGTutorialCrippleHardcore`: "You are crippled. Use a
    Stimpak, sleep, or find a doctor." / "… Use a Doctor's Bag or find a doctor."
  - SPEL `HeadCrippleEffect` (0x00067572) "Head Crippled", type Ability: MGEF
    `ReducePerception` (Value Modifier, Perception, Recover, Detrimental) magnitude 4,
    with the condition `GetActorValue PerceptionCondition <= 0` (CTDA type 0xA0,
    function 14, parameter 25). No RACE or NPC_ record lists it (Caucasian and the
    Player NPC_ have no SPLO).
  - SPEL `PlayerConcussed` (0x0006533A) "Concussion", an Actor Effect: MGEF `Concussion`
    (archetype Concussion) and the same Reduced Perception 4 with the same condition,
    both for 315 360 000 s (ten years). `GenericScript` (SCPT 0x00032DF1) casts it on
    the player (`CastImmediateOnSelf`) whenever the player lacks the Concussion effect,
    and the intro casts it too (scripts.md §8). The GMST `fConcussionTimer` is 40.0.
  - GMSTs: `fMoveOneCrippledLegSpeedMult` 0.85, `fMoveTwoCrippledLegsSpeedMult` 0.75,
    `fCombatPlayerLimbDamageMult` 0.5, `fMoveLegConditionMult` 0.9,
    `fGunSpreadArmBase` 0.5, `fGunSpreadArmMult` −0.5. The four crippled-arm spread
    settings and `fDamageArmCondition*` were already read (combat-rules.md §3, §1).
  - PERK entry point 6: the trait `SmallFrame` (0x00135EC6, "your limbs are more
    easily crippled") has Multiply 1.25, and the challenge perk
    `SelfLimbDamageChallengePerk` "Tough Guy" (0x0015EAD6, "Repeatedly breaking bones
    has led them to become stronger") has Multiply 0.8. Neither entry has conditions.
  - SPEL `DoctorLimbRestoration` (0x00172091): MGEF `RestoreAllLimbs` (archetype Script,
    script 0x000CB07F). Its begin, update and finish blocks each
    `RestoreActorValue` the six limb conditions (25–30) by 100.
  - AVIF `AVIgnoreCrippledLimbs` (0x5F6) "Ignore Negative Effects".
- **The owner's notes** [documented: Fallout wiki crippling text, owner chat
  2026-10-18], in summary:
  - Head: Perception −4 and lower accuracy.
  - Torso: the target flinches from hits much more often. For the player this throws
    off aim and interrupts attacks.
  - Arm: lower accuracy in VATS, and the first-person weapon sways, which shows most
    through scopes. With a one-handed weapon, a crippled right arm gives −10 % base
    melee damage. With a two-handed weapon or unarmed, one crippled arm gives −5 % and
    two give −10 %.
  - Leg: slow movement like being over-encumbered, and leap attacks suffer (deathclaws,
    for example). Two crippled legs slow the player more, give a limp in third person,
    and disable the special melee moves done outside VATS.
  - Weapon: crippling the held weapon breaks it (condition 0) or knocks it from the
    hand. A weapon that wears out is unequipped instead. Thrown explosives that are hit
    explode.
  - Healing outside Hardcore: a doctor's bag, sleeping or a doctor heals every limb,
    and a stimpak heals one limb from the Pip-Boy. In Hardcore stimpaks do not heal
    limbs, a doctor's bag heals one limb or all a little, and only an owned bed heals.
    Daturana, healing poultice and the weapon binding ritual heal all limbs a little.
  - Limb damage is changed by the Small Frame trait and the Tough Guy challenge.

## 2. Head

- **Perception −4** (built):
  - **NPCs** (`fnv_game::abilities::ENGINE_ABILITIES`): `HeadCrippleEffect` joins every
    actor's spell list but the player's the first time the actor is processed,
    alongside its authored abilities. Its condition is checked every second, as for any
    ability, so Perception drops by 4 within a second of the head's crippling and comes
    back as it heals [observed: the record; hypothesis: that the engine gives it to the
    actors, as no record lists it and the help text says the effects apply to enemies
    too]. A save made before this was built gets it only for actors processed for the
    first time.
  - **The player**: the masters' own data does it. `GenericScript` keeps
    `PlayerConcussed` on the player, and its Reduced Perception 4 holds while the head
    is crippled [observed]. Giving the player `HeadCrippleEffect` as well took 8
    (seen in the first smoke run), so the player does not get it. Before this pass the
    spell's effect, once it had started, was never checked again, and the −4 stayed for
    the spell's ten years after the head healed. The conditions of every listed effect
    are now checked every second, as an ability's (aid-items.md §5.3, §13)
    [hypothesis]. Test: `crippled_limbs_data` (needs the game data).
- **Lower accuracy** (built): a crippled head adds `spread_crippled_head` 0.2 spread
  units to the actor spread, the size of one crippled arm with a pistol, for the player
  and NPCs (`fnv_combat::spread::crippled_spread`) [documented: owner notes; design: the
  number].
- **Concussion** (built 2026-10-21, magic-archetypes.md §2): the help text names a
  "concussion effect". The masters' concussion spell is `PlayerConcussed`, which the
  player always carries; with its Concussion effect active a crippled head concusses for
  `fConcussionTimer` seconds (GetConcussed 1, `ImageSpaceConcussion` on the player)
  [hypothesis]. NPCs carry no Concussion effect, so they are never concussed.

## 3. Torso and arms

- **Staggers** (built, `fnv_combat::damage::staggers`): with the torso crippled, any hit
  staggers an NPC, wherever it lands, once the cooldown after its last reaction has run
  out (3 s, combat-rules.md §12). This extends the owner's rule, which staggers only on
  the crippling hit and later hits to a crippled part [observed: HelpCrippling "higher
  chance"; design: every hit after the cooldown, as that rule has no chance in it].
- **The player's flinch** (built, `nvf_play::combat::player_flinch`): the same rule for
  the player. On a staggering hit the view is thrown off by up to 4° sideways and 2° up
  or down (`flinch_kick_deg`), a melee swing under way is broken off, and for 0.6 s
  (`flinch_time`) no attack starts [documented: owner notes "throws off aim and
  interrupts attacks"; design: the numbers]. It also happens in god mode, which only
  stops damage.
- **Melee damage** (built, `fnv_combat::damage::arm_condition`, for every attacker): the
  formula's crippled-arm term (combat-rules.md §1) gets the arm condition by the
  weapon. With a one-handed weapon only the right arm counts. With two-handed weapons
  and unarmed, each crippled arm takes off half. Before this, any one crippled arm gave
  0.5 for the player only, and NPCs had no penalty.
  - **Where the sources differ:** the formula's term is
    `weapon damage × (fDamageArmConditionBase − 1 + fDamageArmConditionMult × condition) / 2`,
    which with the masters' 0.2 and 0.8 takes off 40 % of the weapon's damage at
    condition 0 and 20 % at 0.5. The owner's notes say −10 % and −5 % of base melee
    damage. NVForever keeps the formula's numbers (they come from the game's settings)
    with the notes' rule for which arm counts. Listed for the owner.
- **Spread** (already there): the documented crippled-arm settings, for the player and
  NPCs (combat-rules.md §3).
- **VATS mode** (built): a VATS mode shot leaves along its mark without spread, except
  the crippled limbs' part of the actor spread (arms and head), so crippled arms make
  VATS shots miss [documented: owner notes "lower accuracy in VATS"; design: VATS mode
  is NVForever's own, vats-mode.md].
- **The weapon sways** (built, `nvf_play::combat::arm_sway`): while the player aims down
  the sights or scope with a crippled arm, the view sways in a slow figure of eight
  (1.5° at its widest with one arm, 3° with both, a 2.5 s period). The shots follow it,
  and a scope's zoom makes it look larger [documented: owner notes; design: the
  numbers]. NPCs have the spread instead.
- **Disarmed** (built, `nvf_play::combat::disarm`): an actor other than the player drops
  the weapon in its hand when a hit cripples either arm. The weapon falls in front of it
  as a broken one does (combat-rules.md §17) [observed: HelpCrippling; hypothesis:
  either arm, and not the player, as the text says "enemies"].
  Arms crippled **by script** do not disarm [observed: o14_hazards, 2026-11-01,
  `nvfairport`, god-mode player]: an NCR trooper firing a 10mm Pistol at the player kept
  it equipped (GetEquippedObject 5) and drawn (IsWeaponOut 1) and went on firing for
  40 s after `DamageAV RightAttackCondition 200` (condition 0), and so did one with both
  arms at 0. So the drop, if the original has it, comes with a crippling **hit**, not
  with the condition reaching 0; ours disarms on the hit, which fits. Not yet checked
  with a crippling shot.
- **Hits on the held weapon** (already there, combat-rules.md §17): they wear the
  weapon. At 0 an NPC's falls and the player's is unequipped. This is the owner notes'
  "crippling the weapon".

## 4. Legs

- **Slower** (built, `fnv_game::Game::leg_speed_mult`): `fMoveOneCrippledLegSpeedMult`
  (0.85) with one leg crippled and `fMoveTwoCrippledLegsSpeedMult` (0.75) with both
  [observed: values; hypothesis: their role, from the names]. This holds for the
  player (walking and running, `PlayerLimbs::speed_mult`, which replaces the earlier
  0.7 and 0.5 [design]) and for every other actor. An NPC's walker keeps that share of
  whatever speed its package or fight asks for (`fnv_nav::follow::Walker::pace`, set
  by `limping.rs` through `fnv_ai::Ai::set_pace`), whether its skeleton has limping
  clips or not. `fMoveLegConditionMult` (0.9) is not used: what it multiplies is not
  known.
- **Limp** (already there): actors.md §8.11. One crippled leg is enough, for the player
  and NPCs (owner 2026-10-16). The notes say two legs for the player.
- **No leaps** (built, `npc_combat::no_leaps`): a creature with a crippled leg makes no
  forward power attack, its leap (a deathclaw's) [observed: HelpCrippling "cannot
  perform leaping or charging attacks"; hypothesis: the forward power attack is the
  leap]. The rolls are made as before, so other fights' random sequences do not shift.
- **No directional power attacks** (built): with both legs crippled, the player's power
  attack is the standing one whatever the movement keys say [documented: owner notes,
  "disable the special melee moves done outside VATS"].
- **Charging** (not built): people's "charge across open ground" (combat-ai.md §14.4)
  is a way of closing in, not an attack. Nothing changes it.

## 5. Limb damage

- **Small Frame and Tough Guy** (built, `fnv_game::perks::ADJUST_LIMB_DAMAGE`, entry
  point 6): the target's perks scale the limb damage it takes (×1.25 and ×0.8)
  [observed: the entries; documented: GECK wiki Entry Point names "Adjust Limb Damage";
  hypothesis: code 6 is that entry point, from the two perks' texts, and it runs for
  the actor hit].
- **The player's limbs** take `fCombatPlayerLimbDamageMult` (0.5) of the limb damage
  (`Game::limb_damage`) [observed: the value; hypothesis: it scales the damage the
  player's limbs take rather than the damage the player does to others' limbs].
  Observer check 3 (o17_air limbs_10mm, 38 hits from a trooper's 10mm Pistol on a
  mortal player) measured body Health loss 16.06, arm or leg condition loss 16.06,
  torso Endurance Condition loss 6.69, and head Health loss 32.12 with Perception
  Condition loss 20.075. A larger head hit lost 76.12 Health and 47.575 Perception
  Condition [observed]. With maximum Health 200 [hypothesis: that run did not record
  base Health], DefaultBodyPartData's shares (arms and legs 25 %, torso 60 %, head
  20 %, ×2 Health), and Limb Dmg Mult 1, the existing formula gives all four losses:
  `damage before the part multiplier × Limb Dmg Mult × 0.5 × 100 /
  (max Health × part health % / 100)` [design: `Game::damage_limb`, `limb_hit`].
  The ratios between parts follow from the shares; nothing cancels the halving and
  no formula change is needed under that maximum-Health hypothesis. The synthetic
  test `player_limb_losses_fit_the_10mm_capture_at_200_maximum_health` pins those
  values [design: `crates/fnv-game/tests/synthetic.rs`].
- **Shots and blows on the player** now select a part from the single character
  capsule's hit point in either view, then match its condition in the human body
  part data [design: `nvf_play::combat::player_body_part`, combat-rules.md §12,
  issue #1345]. The height and side bands are [hypothesis]; shots use the selected
  part's Health multiplier. Blows keep their existing Health calculation and use
  damage left after blocking for limb loss [hypothesis: melee's part multipliers
  are unmeasured]. Creatures keep the multiplier of the weapon this path already
  supplies (Fists when unarmed) [hypothesis]. God mode takes neither Health nor
  limb condition; zero-damage hits lose no limb condition [design: `hit_player`,
  `limb_hit`]. Every actual loss logs `combat: the player's <limb> takes <loss>,
  condition <condition>`; `track_limbs` already logs newly crippled player limbs
  [design: `combat.rs`].
- **Explosions** (built, `nvf_play::combat::blast_limb_hits`, `Game::blast_limbs`): a
  blast damages limbs as well as Health [documented: GECK wiki GetHitLimbDamage, which
  returns 0 for an explosion "even if the explosion dealt limb damage"]. It has no hit
  location (GetHitLocation −1, jip-hit-info.md §1). Since 2026-11-06 (#1741) it takes
  shares of the blast's damage to Health: the torso (Endurance Condition) 1/30, the
  legs (Left and Right Mobility Condition) 0.24 together, the head, arms and brain
  nothing, at any distance inside the blast [observed 2026-11-06, o25_limbs, observer
  check 74: frag mines and a grenade at a human's feet, one NPC, Ringo, after `SetAV
  Health 5000`]. NVForever takes the shares in condition points straight off the
  condition as a fall's leg (physics.md §6.12: no pool, no player limb multiplier, no
  perks), the same at any maximum Health [hypothesis: measured only at 5000 Health
  and only on an NPC; a share of the limb's pool (of maximum Health, as shots use)
  fits the same numbers and would do 25–50× more at a normal NPC's 100–200 Health, and
  the player was never measured; rerun open under observer check 74]. NVForever gives the left leg 0.16 and the right 0.08, all 0.24 to a
  body part data with one of them [hypothesis: the original put it all on the left leg
  twice and 2:1 three times; the simplest rule that fits the 2:1 runs], times the
  weapon's Limb Dmg Mult (1 for a blast without a weapon; 1 on both measured weapons)
  and the difficulty, as Health is [hypothesis], and the same shares for a blast at any
  height and on creatures [hypothesis: only a human was measured]. So one frag mine no
  longer cripples arms or the head. Before that, each limb of the actor's body part
  data, once per limb condition, took the blast's damage
  after armour times the weapon's Limb Dmg Mult (1 for a blast without a weapon), then
  the difficulty and the two rules above, as a shot's limb damage [hypothesis: every
  limb takes the full amount; no game setting or page gives the share; owner-review 41].
  The original does not [observed 2026-11-06, capture o25_limbs, `nvftest`: Ringo at
  Health 5000, the player in god mode 900 units off, MineFragExplosion 0x0007078B placed
  by Ringo's PlaceAtMe at 0, 64 and 128 units in front and 64 to his right, then
  GrenadeFragExplosion 0x000179DA at 0, conditions restored between]:

  | blast | Health lost | torso (Endurance) | left leg | right leg | head, arms, brain |
  |---|---|---|---|---|---|
  | mine, 0 | 99.95 | 3.33 | 23.99 | 0 | 0 |
  | mine, 64 in front | 96.99 | 3.27 | 15.68 | 7.84 | 0 |
  | mine, 128 in front | 98.97 | 3.33 | 16.00 | 8.00 | 0 |
  | mine, 64 to the right | 98.00 | 3.27 | 23.52 | 0 | 0 |
  | grenade, 0 | 123.0 | 4.10 | 19.68 | 9.84 | 0 |

  So the torso lost 1/30 of the Health damage every time and the legs 0.24 of it
  together, all on the left leg or split 2:1 left and right; the head, arms and brain
  lost nothing, the damage did not fall with distance out to 128 units, and Ringo was
  not knocked down. A reading that fits all five [hypothesis]: the blast's damage is
  split in 1/12.5 shares (0.08), each share hitting one body part with that part's
  limb damage share as a shot does (the torso's 0.4167 of observer check 3: 0.08 ×
  0.4167 = 1/30; a leg 1.0 per share, three shares to the legs). Which parts the
  shares reach (the legs only here, a blast at the feet) is open: a blast at head
  height would show it. A blast that kills or knocks out does no limb damage. The limb
  damage counts as the blast's hit (`nvf_play::combat::hit_actor`): a limb it cripples
  makes it a crippling hit, so the actor staggers once (the player flinches) by the rule
  in §3, an arm it cripples disarms, and the "limb crippled" HUD line shows for the
  player's blasts; an actor the blast knocks down does not also stagger [design]. The player in first person, whose body is not posed, uses
  the human skeleton's body part data. Before 2026-11-01 blasts did no limb damage, so a
  mine never crippled the player (#1338).

## 6. Healing

- **Sleep** (built, `fnv_game::needs`): outside Hardcore, the first hour slept heals
  every crippled limb of the player [observed: HelpHealingLimbs; hypothesis: the first
  hour]. In Hardcore sleep does not heal limbs. The owner's notes say an owned bed
  heals in Hardcore, but the game's help text says only Doctor's Bags and doctors do.
  NVForever follows the help text. Listed for the owner.
- **A doctor** (already there, from the data): the doctors' limb service casts
  `DoctorLimbRestoration`, whose effect script restores the six conditions (§1). Not
  checked in game here.
- **Stimpak and Doctor's Bag** (aid-items.md §3): the normal Stimpak gives each limb a
  sixth of the amount it heals (a design number); Doctor's Bag and Hydra restore every
  limb.
- **A Stimpak on one limb from the Pip-Boy** (built 2026-10-23, pipboy.md §5.1): that
  limb gets the whole amount; the Doctor's Bag is used from the same page.
- **Hardcore: Stimpaks do not heal limbs** (checked 2026-10-23): the Hardcore Stimpak's
  effect is a Value Modifier on Health alone, 5 a second for 6 s, no limbs [observed:
  `fnvtool actor aid --use Stimpak --hardcore`], as the help text says; the Pip-Boy's
  CND page refuses a Stimpak aimed at a limb in Hardcore.
- Daturana, healing poultice and the weapon binding ritual: their own effects (aid
  items); not looked at here.

## 7. Not built, and why

- The concussion screen effect on a head crippling (§2): no source says the engine
  casts it.
- Frenzy from crippled antennae or combat inhibitors: a separate behaviour (MGEF
  Frenzy, the creatures' body part data). It is not part of this pass.
- Thrown explosives that are hit going off: held explosives are not hit-tested.
- Over-encumbered speed (the notes compare the legs with it): no carry weight rule
  slows the player yet.
