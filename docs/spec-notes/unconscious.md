# Spec notes: unconscious actors (essential knockouts and fatigue knockouts)

Actors that fall unconscious instead of dying, and get up again: essential actors at zero
health, and any actor whose Fatigue runs out (boxing gloves). Owner request 2026-10-17.
Code: `crates/fnv-game/src/unconscious.rs` (the rules), `crates/fnv-game/src/combat.rs`
(where hits lead to them), `engine/nvf-play/src/knockouts.rs` (corner message, log lines,
the `NVF_PLAY_TEST_HIT` aid), `engine/nvforever/src/knockouts.rs` (falling and getting up
on screen).

## 1. What the data and the documentation say

- **Essential** is an NPC_/CREA ACBS flag [documented: xEdit flag name]; a script sets
  it on a base (SetEssential) or a reference (SetActorRefEssential) [documented:
  catalogue]. KillActor on an essential actor makes it "temporarily unconscious for a
  few seconds" and KillAllActors does the same, "in their normal temporary dead state"
  [documented: GECK wiki KillActor and KillAllActors notes, owner export
  `C:\fnv-private\geckwiki\functions.xml`].
- **Fatigue** is actor value 22 [documented: actor-values.md]. A creature's ACBS fatigue
  is its fatigue, "unconscious at zero in New Vegas" [documented: GECK wiki Stats Tab -
  Creatures, combat.md §4.2]; NPCs carry the same ACBS field (Goodsprings residents: 50
  [observed 2026-10-17, `fnvtool esm decode`: GSSunnySmiles, GSRingo, GSTrudy,
  GSEasyPete, DocMitchell]) and NVForever reads it as theirs [hypothesis, as
  actor-values.md].
- **Boxing gloves** deal fatigue through their weapon script, not through the EITM
  effect [observed 2026-10-17]: WEAP `WeapNVBoxingGloves` (0x000E5391) has SCRI
  `BoxingGlovesFatigueOnHitScript` (0x001489D0) and EITM `DummyNVBoxingGloves`, whose
  only effect `DummyNVFatigueDamage` ("Inflicts Fatigue Damage (KO)") is a Value
  Modifier on no actor value. The script's source (SCTX) is one `Begin OnHit` block: it
  takes `GetOwnerLastTarget` and, unless that is the player or a creature of type 6,
  does `DamageAV Fatigue 35` on it, else `DamageAV Health 10`. The Golden Gloves, the
  Boxing Tape and the cattle prod have scripts of the same kind
  (`BoxingGoldenGlovesFatigueOnHitScript`, `BoxingTapeFatigueOnHitScript`,
  `CattleProdFatigueOnHitScript`).
- **GetOwnerLastTarget** "gives the most recent target of whoever owns the calling
  reference" [documented: catalogue summary].
- **GetKnockedState** returns −1 for a fatigue knockout, 0 normal, 1 knocked out
  ("includes unconscious essential actor, paralyzed actor, and negative fatigue
  knockout"), 2 knocked down [documented: GECK wiki GetKnockedState].
- **GetUnconscious** reads only SetUnconscious's state; it "does not detect actors who
  have been rendered unconscious from fatigue damage or from being essential", for which
  GetKnockedState is meant [documented: GECK wiki GetUnconscious]. **GetDead** does not
  read 1 for an essential actor knocked out [documented: GECK wiki GetDead notes].
- **IsFacingUp** (bipeds) and **IsLeftUp** (quadrupeds) are the idle manager's checks of
  a knocked-down actor's side [documented: GECK wiki].
- **KOed** combat topic: "said when an actor loses all their fatigue points and becomes
  unconscious" [documented: GECK wiki Combat Tab].
- **Combat style**: "Unconscious Bonus to Attack / to Power Attack" are added when the
  opponent is unconscious [documented: GECK wiki Combat Style] — in the original an
  unconscious opponent can still be attacked. The owner asked that an unconscious actor
  no longer be targeted (§3) [design, owner request 2026-10-17].
- **Text**: GMST `sNoTalkUnConscious` (0x000A9D13) is "is unconscious." [observed
  2026-10-17, its DATA bytes]. No GMST names an essential's down time, the health it gets
  up with, or how fast fatigue returns: FalloutNV.esm has `fFatigueBlockBase` and
  `fFatigueBlockMult` and no other `Fatigue` setting, and no setting with `Unconscious`
  or `Essential` other than the text [observed 2026-10-17, `fnvtool esm inventory`].
- **Animations** [observed 2026-10-17, IDLE records and the BSA listing]:
  `KnockedDownIdles` (0x00003BD8, `Characters\_Male\IdleAnims`, conditions on functions
  391 and 107 — GetKnockedState) holds `GetUpFaceUp` (0x00003BD9,
  `MT_KnockDownFaceUp.kf`, condition IsFacingUp == 1) and `GetUpFaceDown` (0x00003BDA,
  `MT_KnockDownFaceDown.kf`, IsFacingUp == 0). The two clips are the sequences
  `SpecialIdle_KnockDownFaceUp` / `…FaceDown`, 2 s, clamped; the accumulation root
  `Bip01` has no track and `Bip01 NonAccum` carries the body from the ground to standing
  [observed: `fnvtool nif anim` / `nif decode`]. Creatures have their own:
  `idleanims\specialidle_getupfaceup.kf` / `…facedown.kf` (ghoul, mirelurk, behemoth,
  maintenance robot), `idleanims\mtspecialidle_getupright.kf` / `…left.kf` (dog,
  brahmin). `NPCUnconscious` (0x00060C7D, condition GetUnconscious == 1) and the
  `…Unconscious` creature idles are folders without a clip.

## 2. Rules (`crates/fnv-game/src/unconscious.rs`)

1. A knockout is a state of the reference (`RefExtra::knocked_out`) with a cause,
   essential or fatigue, and the game time it began. It is separate from SetUnconscious's
   flag (`RefExtra::unconscious`), which only scripts set and GetUnconscious reads (§1).
   The player is never knocked out [design: not asked for; the gloves' script spares the
   player anyway], but is knocked down (§5.4).
2. **Essential at zero health**: a hit or a harmful effect that would bring an essential
   actor's health to 0 leaves it at 1 and knocks it out (cause essential). It does not
   die: no Death line, no kill, no XP, no dead flag, no OnDeath, no loot [documented: §1
   GetDead; design: the rest]. KillActor on an essential actor knocks it out too
   [documented: KillActor notes]. It says its KOed line [design: the topic's text names
   fatigue only].
   **DamageActorValue** on Health that leaves an actor at 0 or less: a non-essential NPC
   dies, an essential actor is knocked out as by KillActor, and the player dies
   [non-essential NPC and player: observed, below; the essential actor: hypothesis,
   observer-checks.md check 89]. In the original, an unarmed NCR trooper alone on the
   runway with nobody shooting (Health 50), given `DamageAV Health 9999` by script, read
   GetDead 1 in the very next game loop, Health **-9949** from then on (no floor at 0), and
   OnDeath named no killer (0); the player's karma did not move. The player without god
   mode, given the same, read GetDead 1 and Health -9799 at once, OnDeath fired for the
   player with no killer, and the game went to its death reload. DamageAV on a corpse
   still lowers its Health (-9949 to -19948) [observed: campaign o29_dmg, 2026-11-08,
   `nvfairport`]. `SetEssential` on a spawned leveled trooper's base did not stop it dying
   the same way, so it tells nothing of an essential actor [observed, same run]. XP was not
   read. The only sighting is check 22: two super
   mutants hurt by DamageAV 300 died [observed, run o21_air: not isolated from the NCR
   troopers firing in the scene, no GetDead read, their Health not known to be below 300].
   Health stays where the damage left it, with no floor of 1, and an essential actor still
   gets up at half its maximum [design].
   The death has no killer: no XP, karma, murder or challenge, and OnDeath's killer is 0
   [hypothesis]. A ghost, a child or an Invulnerable creature does not die of it, as of a
   hit [hypothesis], nor a player in god mode, nor an actor whose record gives no Health
   (no DATA, so its Health is only a stand-in of 1) [design]. While a hit is being applied
   the hit decides: DamageAV in an OnHit script (the gloves' `DamageAV Health 10`, §1) or
   in the ScriptEffectStart of an effect the hit brings leaves the death to the hit, which
   credits its attacker [design]. A lasting effect's later script ticks and a Paralyzing
   Palm's effect, which comes after its hit, have no killer.
   ModActorValue, ForceActorValue and SetActorValue that bring Health to 0 kill nobody
   [design].
3. **Fatigue at zero**: whenever an actor's Fatigue is lowered (DamageAV, a fatigue
   effect) and its current Fatigue is 0 or less, it is knocked out (cause fatigue) and
   says its KOed line [documented: §1 Stats Tab and KOed].
4. **Fatigue returns** by `FATIGUE_RETURN` = 10 points a second of game time for every
   actor whose Fatigue is damaged [design: no setting in the data, §1].
5. **Getting up** [owner rules 2026-10-17]:
   - A **fatigue** knockout ends after `FATIGUE_DOWN` = 30 s of game time, whether or
     not a fight goes on [owner rule; design: 30 s is a test value — the data has no
     setting for it, §1]. The actor gets up with its Fatigue whole.
   - An **essential** knockout lasts at least `ESSENTIAL_DOWN` = 10 s of game time
     [design: "a few seconds", §1], and then until no fight goes on around the actor:
     no actor in combat (alive, not knocked out itself, not the knocked-out actor)
     within `FIGHT_RADIUS` = 4096 units in the same interior or worldspace [owner rule;
     design: the radius]. The first time a fight holds it down past its time the engine
     logs `knockout: <name> <id> stays down: a fight goes on near it`. It gets up with
     its health back to `ESSENTIAL_UP_HEALTH` = half its maximum (when it had less) and
     its Fatigue whole [design].
   - An actor that died while down (a non-essential knocked out by fatigue and then
     killed) stays dead.
   - **Getting up is part of the knockout** [owner rule 2026-10-17: NPCs do not move
     until their get-up animation is over]: when its time comes the knockout turns to
     getting up (`KnockOut::rising`) and the actor still counts as knocked out — no
     packages, no walking, no fighting, nobody targets it — until the engine has played
     its get-up clip (`Game::finish_getting_up`), or `GETUP_MAX` = 2.5 s of game time for
     an actor nobody draws [observed: the get-up's GetAnimAction 13 lasted 2.41-2.53 s,
     §5.5]. GetKnockedState keeps
     0 meanwhile [observed: §2.10]. Beaten to zero again while getting up, an
     essential actor goes down again.
6. A fatigue knockout of an essential actor that is then brought to zero health becomes
   an essential knockout (its time starts again).
7. While knocked out an actor does not fight, talk or run packages, and nobody targets
   it (§3); further hits still land (health damage on a fatigue knockout; an essential
   stays at 1).
8. **Scripts**: GetKnockedState is −1 for a fatigue knockout, 1 for an essential one or
   SetUnconscious [hypothesis for SetUnconscious, kept from before], else 0; GetUnconscious
   reads only SetUnconscious; GetDead is 0 for a knocked-out essential; IsFacingUp and
   IsLeftUp read the side the engine saw the body come to rest on (§4), 1 before that
   [hypothesis]. ResurrectActor ends a knockout as well. GetOwnerLastTarget is the actor
   the calling item's holder (or the calling actor) last hit [hypothesis: the hit target
   is the "target" the summary means].
9. **Weapon OnHit**: when an actor hits another with a weapon whose script has an OnHit
   block, that block runs on the weapon (its instance per holder and item, as the other
   item blocks), after the target's OnHit and OnHitWith [hypothesis: the gloves' script
   only works so; the order is a guess].
10. **An essential knocked out in the original** (2026-10-19, AI observer campaign
   `functions_round` session `essential_ko`: RingoRef made essential with SetEssential on
   his base, then KillActor, no fight about; every 2 game loops) [observed]:
   GetKnockedState read **1** from the next sample, GetUnconscious **0** throughout,
   GetDead 0, Health unchanged (120: KillActor took none); no OnHit, OnDeath or OnMurder
   event. He lay **10.0 s** (`ESSENTIAL_DOWN`, which had been a design value), then
   GetAnimAction read 13 for 2.2 s (the get-up) while GetKnockedState read **0**, then
   the AI procedure GET_UP for a moment and his sandbox package again. NVForever now
   reads 0 for an actor getting up (`functions::fighting::get_knocked_state`).

## 3. AI

- `Game::would_attack` already refuses an unconscious (or knocked-out) actor on either
  side; detection and awareness forget a knocked-out target as they forget a dead one
  (`Game::settle_awareness`), so fighters turn to other targets and come back to it only
  once it is up and detected again [design, owner request].
- Crime witnesses, aggression (`aggro.rs`) and the AI's movement (`fnv-ai` carry.rs)
  treat a knocked-out actor as they treat SetUnconscious's.
- Dialogue: `Game::start_dialogue` refuses a knocked-out or SetUnconscious actor; the
  player's activation shows "<Name> is unconscious." in the corner (the GMST's text after
  the name) [hypothesis: what the setting's name says it is for].

## 4. On screen

- **Corner message**: an essential knockout shows "<Name> is unconscious." at the top
  left (`sNoTalkUnConscious` after the name; "is unconscious." without the masters). A
  fatigue knockout shows nothing [owner request]. One log line per knockout and per
  getting up: `knockout: <name> <id> knocked out (essential|fatigue)`,
  `knockout: <name> <id> gets up`.
- **Falling**: exactly as if it were killed: the same ragdoll drop as a death
  (ragdoll.md; `engine/nvforever/src/ragdoll.rs`, the `Corpse` deaths.rs gives a dead
  actor), not a knockdown clip; a skeleton without a ragdoll plays its death pose (or is
  laid on its back) as deaths.rs does for the dead [owner rule 2026-10-17].
- **Lying**: it stays a ragdoll while unconscious; the ragdoll rests; the engine reads the side the body lies on from the
  ragdoll's pose — face up when the body's front (left upper arm → right upper arm
  crossed with pelvis → head, signs as in the bind pose) points up — and tells the game
  (IsFacingUp) [design].
- **Getting up**: the ragdoll leaves the physics world and the get-up clip of that side
  plays once from its first frame (`mt_knockdownfaceup.kf` or `…facedown.kf` from the
  skeleton's `idleanims` folder, the creature names of §1 for creatures). The actor is
  first turned and moved so that the clip's first frame lies where the ragdoll lay:
  pelvis on pelvis, the pelvis → head line along the ragdoll's [design]. Without a clip
  the actor simply stands again. After the clip the actor walks and fights as before.

## 5. Knockdowns: PushActorAway, explosions and SetUnconscious

Live actors knocked down without being knocked out (row 1 of docs/outstanding.md §1).
Code: `crates/fnv-game/src/unconscious.rs` (`Game::push_actor_away`, `Game::knock_down`),
`crates/fnv-game/src/functions/fighting.rs` (PushActorAway, GetKnockedState),
`engine/nvf-play/src/combat.rs` (explosions), `engine/nvforever/src/knockouts.rs` and
`ragdoll.rs` (the body thrown).

### 5.1 What the documentation and the scripts say

- **PushActorAway** `source.PushActorAway target force` "knocks down (ragdolls) one actor
  and pushes it away from another with a given force"; a negative force pulls the target
  towards the source; no calling reference launches the target "straight up"; it does not
  work on dead actors, on actors "flagged as immobile", or on actors with the "No
  Knockdowns" flag; "an actor that has been pushed can have its inventory accessed by the
  player until the actor gets up again" [documented: GECK wiki PushActorAway, owner export
  `C:\fnv-private\geckwiki\functions.xml`].
- **GetKnockedState** is 2 for "knocked down", but "will return 0 if called on actors
  knocked down by PushActorAway", and an essential actor knocked out before it recovers
  from a push still reads 0 [documented: GECK wiki GetKnockedState notes].
- **Explosions**: EXPL flags "Knock Down - Always" and "Knock Down - By Formula", and the
  Explosion Knockdown Formula with its seven settings [documented: GECK wiki; already in
  `fnv_combat::damage::knockdown_chance`].
- **No Knockdowns** is ACBS flag bit 26 of NPC_ and CREA, **Immobile** bit 23 of CREA
  [documented: xEdit flag names, `schemas/fnv/gameplay.json`]; template "Base Data" does
  not carry No Knockdowns [documented: GECK wiki Category:NPC, Template Data].
- **SetUnconscious** turns "conscious behavior" off: "Actors that are unconscious will not
  use AI behavior" [documented: GECK wiki SetUnconscious]. In FalloutNV.esm it is called
  107 times by 66 scripts and result scripts [observed 2026-10-18, `fnvtool script
  decompile`]: robots standing in pods (SCPT 0x0005BB12: `SetUnconscious 1` OnLoad,
  cleared when activated), scheduled sleepers (SCPT 0x00159A99), and the thugs that fake
  death (SCPT 0x00118A77 and the trigger SCPT 0x00118A7C: `SetActorValue Fatigue -1`,
  `SetUnconscious 1`, then `PushActorAway <thug> 1`; OnHit sets a variable and, once
  `GetKnockedState == 0`, `SetUnconscious 0`). So SetUnconscious alone must not lay an
  actor down; the lying comes from the push [hypothesis drawn from those scripts].
- **PushActorAway** is called 21 times by 18 scripts, forces 1 to 20, six of them on the
  player [observed 2026-10-18, the same decompile].

### 5.2 Rules

1. A knockdown is a knockout (§2) with cause **pushed** (PushActorAway) or **knocked
   down** (an explosion). It lasts `KNOCKDOWN_DOWN` = 2.4 s of game time, fight or not,
   then the actor gets up as in §2.5 (getting up stage, get-up clip) [observed for pushes,
   §5.5; an explosion's is assumed the same]. No KOed or Wake line, no corner message, no fatigue or health
   restored [design: a knockdown is no unconsciousness].
2. Refused (nothing happens) for dead actors, actors whose own record has No Knockdowns,
   and Immobile creatures [documented: §5.1], and for the player on furniture or in a
   conversation [design]; the player is knocked down otherwise (§5.4).
3. **Order**: an essential knockout replaces a fatigue one, and either replaces a
   knockdown (its time starts again); a knockdown never replaces a knockout. Knocked again
   while getting up: down again (unless only out of fatigue again, §2) [design].
4. **The push**: the body starts with a velocity of `PUSH_SPEED` = 30 game units a second
   per point of force, along the ground from the source to the target (towards the source
   for a negative force), plus `PUSH_RISE` = 220 units a second upwards (no more than the
   push's own speed); without a source, or with the source on the target, straight up
   [documented: the direction rules, §5.1; observed: the numbers, §5.5]. A push on an
   actor already lying throws its body again and marks it pushed. An explosion throws the
   body away from the blast at `BLAST_KNOCKDOWN_SPEED` = 300 units a second plus
   `PUSH_LIFT` = 0.25 of that upwards [design].
5. **Scripts**: GetKnockedState reads 0 while an actor pushed down lies, also once
   knocked out meanwhile; 2 for an explosion's knockdown; the other values as §2.8
   [documented: §5.1]. GetUnconscious still reads only SetUnconscious.
6. **SetUnconscious holds a lying actor down**: while the flag is set, no knockout or
   knockdown gets up, whatever its time; once cleared, it gets up at its next frame if its
   time is past [hypothesis: the fake-death thugs, §5.1]. SetUnconscious alone lays nobody
   down (pods, sleepers) [hypothesis, same scripts].
7. **Searching**: activating an actor that lies pushed down opens its inventory, as a
   corpse's does, until it begins to get up [documented: §5.1].

### 5.3 On screen

The body falls as §4 says (ragdoll, else the death pose); the ragdoll starts with the
knockdown's velocity (`Game::take_shove`, `Corpse::velocity`); a push on a body already
lying shoves the ragdoll at its pelvis (`ActorPush`). Log lines: `knockout: <name> <id>
knocked down (pushed|knocked_down)`, `knockout: <id> thrown at (x, y, z) units/s`, the
explosion's `…, knocked down` in its `combat: blast` line, and the exit count
`knockouts: … N knocked down, …`.

### 5.4 The player knocked down

Built 2026-10-18 (claude/nav-knockdown). Code: `crates/fnv-game/src/unconscious.rs` (the
rules), `engine/nvforever/src/player_knockdown.rs` (on screen), `engine/nvf-play/src/script.rs`
(the controls).

1. **Rules**: PushActorAway and an explosion's knockdown lay the player down as anyone
   (§5.2: the push's velocity, `KNOCKDOWN_DOWN`, GetKnockedState 0 pushed and 2 for a
   blast), unless the player sits on furniture or is in a conversation [design]. A blow
   that would knock the player out (essential, fatigue) does not (§2.1). Six shipped
   PushActorAway calls throw the player (§5.1) [observed].
2. **Enemies** may attack the player while down: `Game::would_attack` keeps a knocked-down
   player a target, where it drops a knocked-out NPC (§3) [design; how the original's
   attackers treat a downed player is not in our notes].
3. **Controls**: all held (movement, looking, fighting, the Pip-Boy, POV) from the fall
   until the get-up idle is over; log `player controls: all held, the player is knocked
   down` [design].
4. **The body**: the third-person rig drops as its ragdoll, thrown with the knockdown's
   velocity, as the dead player's does (`death_cam.rs`); a push while it lies shoves it
   again; the side it comes to rest on is told to the game (IsFacingUp, §4's rule).
5. **The view** [observed: the second run below]: while the body flies and lies, the
   camera stays exactly where it was when the player fell and does not turn: at the eye in
   first person (the view leaves first person: the body is drawn whole and seen from
   there), where the third-person camera was in third person. For the getting up the view
   is the ordinary third-person camera behind the player, in first person too; a
   first-person view comes back once the get-up clip is over.
6. **Getting up**: after `PLAYER_KNOCKDOWN_DOWN` = 2.4 s [observed; NPCs'
   `KNOCKDOWN_DOWN` is the same 2.4 s, observed §5.5] (`KnockOut::rising`) the ragdoll
   leaves the world, the character stands on the ground under the ragdoll's pelvis, and the
   IDLE `GetUpFaceUp` or `GetUpFaceDown` (§1) is asked for on the player as PlayIdle would
   (`Game::ask_idle`), so `idles.rs` plays its clip on the third-person body. Two frames
   into it the character is turned and moved so that the clip's pelvis and pelvis → head
   line lie where the ragdoll's did (the NPCs' rule, §4); when the idle has played (or
   after 4 s of real time) the rules are told (`finish_getting_up`), and the controls and
   the view are the player's again. Without a ragdoll the player stays where they are
   until the knockdown's time is over.
7. **Death while down**: the death camera takes over (its own ragdoll drop).

Log lines: `player knockdown: the player falls (<cause>, <first|third> person[, the view
leaves first person]): ragdoll thrown at (x, y, z) units/s`, `player knockdown: the player
gets up face up|down <s> s after the fall (<idle>)`, `player knockdown: laid over the
ragdoll: turned <deg>°, moved <n> units`, `player knockdown: the player is up <s> s after
the fall; the controls[ and the first-person view] are theirs again`, and at exit `player
knockdown: <n> knockdown(s) shown, <n> get-up clip(s)`. Smoke: `knockdown-player`,
`knockdown-player-third`.

Capture job `tools/capture/jobs/kd_player_knockdown.steps.json` (nvfstrip): the
original's view, camera and timing.

First run of the job (2026-10-18, attended, output `kd_player_knockdown.1792356303.jsonl`)
[observed]: the first-person trial is void: the game window was not in front (every
screenshot `foreground: false`) and the game stood still: GameDaysPassed 15.8393 and the
same position and heading in all 48 samples (the lost-focus pause). The third-person trial
ran (GameDaysPassed 15.8419 → 15.8462, about 12 s at timescale 30), but its 48 screenshots
failed (`BitBlt failed`). The player reference's own position moved only about 16 units
(away from the can's side was +y; it went −x, −y) and was still from about 1.75 s after the
push, its heading turned 7° at about 2.25 s, and GetKnockedState read 0 throughout, which is
what a pushed actor reads (§5.1). So the numbers neither show nor rule out the fall: the
reference may stay with the character while the body is a ragdoll. Nothing was fitted from
them; §5.4's camera and timing stay [hypothesis] until a run with pictures, with the game
window in front for the whole job.

Second run (2026-10-18, attended, the game in front throughout; output
`kd_player_knockdown.1792356663.jsonl`, 98 screenshots in `C:\fnv-private\capture\shots`)
[observed]. Samples are 0.30 s apart (the screenshots' clock: 27 samples in 8 s).

- **First person**: the save opened in conversation with a Securitron, so the push took
  effect when it closed (sample 10). From then the picture is no longer first person: the
  player's legs fill sample 10, the whole body is seen in the air beside a wall at 14 and
  lying at the bottom edge at 16–17, all from one camera place and direction (samples 11
  to 17 show the same view), at about eye height where the player stood. At sample 18
  (8 samples, about 2.4 s after the push) the player reference turned to a new heading
  (197.7° → 38.3°) and stopped moving, and the view became the third-person camera
  behind it: the body lies face down in front at 18–21, rises at 22–24 and stands at 25–26
  (about 2.1 s, the 2 s clip); at 27 the view is first person again.
- **Third person**: the body was thrown from sample 00; samples 01 to 08 show one camera
  place and direction (the body leaves the picture); the heading changed at 08 (0° →
  7.2°), the camera shifted with it at 09, and the body is seen rising at 11–14 and
  standing at 16–17 from the ordinary camera behind it; normal from 20.
- **Time down**: about 2.4 s from the push to the heading change in both trials; the
  player reference stopped moving one or two samples before it.
- **Distance**: the player reference moved about 50 units (first) and 25 (third), but not
  away from the can (§ the first run); the body flew farther than the reference in the
  pictures. Not fitted: `PUSH_SPEED` stays a guess.

### 5.5 NPC pushes measured in the original (2026-10-19)

AI observer campaign `tools/capture/observe/knockdown_look.campaign.json` (save `nvftest`,
GSGasStation; outputs `C:\fnv-private\capture\out\obs_knockdown_look_0[123]_push_*.jsonl`,
private): the player, 120 units east of RingoRef, runs `player.PushActorAway RingoRef
<force>` at a set sample (a `timed` hook, ai-observer.md §12) while Ringo's position,
heading, GetAnimAction and AI procedure are sampled every game loop (about 60 a second).
Five pushes: force 10 three times (the session restarted twice for a plugin fault, §12),
40 and 120 once [observed]:

- **The reference follows the body**: Ringo's GetPos moved with the ragdoll (230–264
  units for forces 10 and 40), unlike the player's reference (§5.4); GetAngle Z did not
  change while he flew and lay.
- **Speed**: the reference flew 330–420 units/s at force 10 (slowing to 330 before it
  landed) and 1,000–1,300 at force 40; at force 120 about 3,000 (50 units a sample) until
  a wall 275 units away threw him back to 65. About 30 units/s per point of force, as
  `PUSH_SPEED` had it.
- **Height**: the reference rose 42 units at force 10 (top at 0.13 s) and 31 at force 40
  (0.05 s), then fell as under gravity (landing at 0.47 s and 0.34 s); not at all at force
  120 (straight into the wall). The rise does not grow with the force: `PUSH_RISE` = 220
  units/s up whatever the force (no more than the push's own speed) replaces the quarter of
  the speed (which gave 4 units at force 10 and 66 at force 40).
- **Time down**: GetAnimAction became 13 (ANIM_ACTION_FORCE_SCRIPT_ANIM, which the wiki
  lists for "getting up from the ground after getting knocked down" [documented: GECK wiki
  GetAnimAction]) **2.38 s after the push** in four of the five, at forces 10 and 120 and
  whatever the distance flown, the player's own 2.4 s (§5.4); after the force-40 push
  (the body slid into a wall) at 5.40 s. `KNOCKDOWN_DOWN` is now 2.4 s (was 3 s, design).
- **Getting up**: 13 lasted 2.41 s (force 40) and 2.53 s (force 120), then −1; the AI
  procedure read 21 (GET_UP) for a moment 0.5 s later, then the package went on.
  `GETUP_MAX` (an undrawn actor's getting up) is now 2.5 s (was 3 s, design). Ringo stood
  up facing where his body lay (heading 272.7° → 343.3° after the force-40 push, 94.6° →
  85.2° after the force-120 one), as §4 lays the get-up clip over the ragdoll.
- **GetKnockedState read 0** throughout, as the wiki says for a pushed actor [documented,
  now observed].
- **Events** (xNVSE, ai-observer.md §12): each push was heard as an **OnHit** on Ringo with
  no attacker and an **OnHitWith** with the form `000001F4`, before any damage (Health 120);
  he lost 0.2 Health in the fall at force 40 and none at 10 or 120.
- Not measured: an explosion's knockdown (`BLAST_KNOCKDOWN_SPEED` and its lift stay
  design), the distance a body slides on open ground (every push here ended at a wall or
  inside the gas station), a negative force.
