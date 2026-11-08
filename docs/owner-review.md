# Owner review: open gameplay decisions

Only decisions that change how the game plays and that need **you**: design choices,
feel, and places where NVForever chose to differ from the original. Most important first.
Anything that asks "what does the original do here?" is not here: the observer can
answer it, and it is in [observer-checks.md](handoff/observer-checks.md).

Condensed 2026-10-24 from the full running log (8,300 lines, 2026-10-09 to 2026-10-23).
Merge records, tooling, look-only guesses and items already ruled were dropped. The full
log is in git history: `git show 85dcaace:docs/owner-review.md`. Each spec note still
lists its own guesses.

The #691 merge (2026-10-25) put the full log back above this list by mistake; it was taken
out again on 2026-10-25, and the decisions from the eleven log entries written on 2026-10-24
and 2026-10-25 were folded in below (items 27-32). That last full copy is
`git show 2622e6ce:docs/owner-review.md`. Their questions about what the original does stay
in their spec notes.

**Adding an item:** one bullet in the right tier, what was chosen and what changes if you
rule otherwise, with its spec link. If the original game can answer it, add it to
observer-checks.md instead. **Ruling on one:** delete the bullet and note the ruling in
the spec it links to.

## Tier 1: felt in every fight or every session

1. **Crippled torso staggers on every hit.** An NPC with a crippled torso staggers on
   any hit (3 s cooldown), which goes further than your "crippling hit and later hits
   to that part" rule. The player flinches the same way: the view jumps up to 4°, the
   swing breaks, and there is no attack for 0.6 s. This happens even in god mode.
   If you rule otherwise, torso crippling stops causing repeated staggers.
   ([crippled-limbs](spec-notes/crippled-limbs.md), [combat-ai §18](design/combat-ai.md))
2. **NPC gunfire is maybe too deadly.** NPCs react in 0.6 s, their first shots are 4°
   wider, and the spread settles over 3 s. They aim from the eyes. Violet hit 12 of 15
   shots at 1,600 units. If you rule otherwise, openers get slower or less accurate.
   ([combat-ai](design/combat-ai.md))
3. **Combat AI tuning, all our own numbers.** Combat is not a parity target, so play and
   say which of these feels wrong ([combat-ai §12–§17](design/combat-ai.md)):
   - Crouch beyond 600 units, rolled every 1.5–4 s at 25–90 % by temperament. Up to
     40 % extra cover chance.
   - Melee block 80 %/40 % with a 0.12–0.35 s reaction. A blocked blow does 60 % less
     damage and the attacker recoils for 1 s.
   - Dodges about half the time while holding, about 87 units. Combos run 2–3 blows
     (55 % chance to chain).
   - Gunmen stop at 80 % of their reach (400–2,500 units). Flamers stop at 320, shotguns
     at about 480, and launchers just clear of their own blast; launchers never fire into
     it. Cover is at most 1,024 units away.
   - Alarm calls allies within 4,096 units. A fled NPC re-checks after 30 s. Group
     searches give up after two sweeps.
   - Fire only with the target within 30° ahead, turning at 240°/s.
   - Creatures start blows on the run from up to 200 units beyond reach (minimum
     closing speed 100 units/s).
   - Comrades and the player block an NPC's shot; animals in the way do not.
   - A fighter in cover hides from whoever shoots at it but fires at its own target.
   - Combat barks: only the 5–30 s timer, no group cooldown (a pack of dogs all yelp).
4. **Melee with a crippled arm.** Uses the game formula's numbers: 40 % less damage
   with a fully crippled arm, 20 % at half. Your notes said −10 % / −5 %. Which one?
   ([crippled-limbs](spec-notes/crippled-limbs.md))
5. **Crippled arms and head, the player's side.** Aim sways in a figure-eight: 1.5° with
   one crippled arm, 3° with both, every 2.5 s. A crippled head adds 0.2 spread on top of
   Perception −4. All numbers are mine. ([crippled-limbs](spec-notes/crippled-limbs.md))
6. **Weapon jamming.** Below 75 % condition a gun may jam: up to 4 % per shot at 0 %
   (0.77 % at 50 %), on a 1.5-power curve. NPCs never jam. More, less, or NPCs too?
   (`crates/fnv-combat/src/tuning.rs`)
7. **Lockpicking and Force Lock tuning.** The sweet spot is 30/20/12/8/5° from Very Easy
   to Very Hard, +1 % per skill point over the lock's need. A pin lasts 1 s of strain
   (+0.02 s per point over). The sweet spot is rolled again each time the menu opens.
   Force Lock succeeds 3 % per point over the need, at most 95 %; failing breaks the lock
   for good, and forcing gives no XP. ([lockpicking](spec-notes/lockpicking.md))
8. **Melee controls.** Holding Use for 0.4 s makes a power attack (you described "about
   a second"). During a round-by-round reload, a press only closes the weapon; a press
   during closing is kept for 0.25 s and fires if closing ends in time.
   ([combat-rules §4, §10](spec-notes/combat-rules.md))
9. **Every bullet is drawn.** The original draws only the record's tracer share (minigun
   30 %, 10mm 0 %). Ours draws every projectile at the record's speed, plus a 0.08 s line.
   Should the original's shares come back? ([combat §5.6](design/combat.md))
10. **True iron sights on by default.** Guns with a sight node aim down the real sights,
    and the crosshair is hidden while aiming. A Gameplay switch gives the original aim.
    ([camera §4.1](spec-notes/camera.md))
11. **Scope sway.** Sways by `scopewobble.nif`, at most half of spread × 5.955°, halved
    unless you fired in the last 0.5 s (about 1° for a new character's sniper rifle).
    The scope comes up 0.25 s after Block. ([weapon-scopes](spec-notes/weapon-scopes.md))
12. **Blood on the screen when hurt.** ceil(lost/max × 25) splats for 20 s, at most 25,
    and it can cover the view in fights. Less? ([screen-splatter §4](spec-notes/screen-splatter.md))
27. **NPCs use their stimpaks.** A hurt NPC (in a fight, or hit within the last 15 s) uses
    a stimpak it carries when at most 25 % of the heal would be wasted, or at any amount
    below 35 % health, 3 s apart at least; the idle plays on the upper body so it keeps
    moving. All our numbers: should NPCs heal more, less, or not at all?
    ([combat-ai §21](design/combat-ai.md))
36. **Lunges carry the attacker.** A forward power attack (a lunge) whose clip moves its
    root more than 100 units carries the actor that far (a Deathclaw's 523 units, your
    forward power attacks 240-280, Legion Assault 332), stopped by walls and by whoever it
    lunges at; a lunging actor pushes nobody aside, as you ruled for bites. Other blows
    still step in place (carried, the dogs ended up biting from inside the player's
    reach); the yao guai's standing power attack (305 units) so still slides back. If you
    rule otherwise: every travelling blow carries, and the AI must step back after it.
    NPCs start their lunges once they touch their target, so an NPC's lunge carries it
    little or nothing until the AI lunges from farther out. ([actors §8.13](spec-notes/actors.md))
37. **Melee knockback distance.** A melee blow that leaves its target standing pushes it
    back through its collision: the knockback formula's force read as a speed falling to
    rest over 1 s, so force / 2 units (a Sledgehammer hit about 57, a punch 24, a
    deathclaw swipe up to 256). A blocked or 0-damage blow still pushes 15 units (the
    formula's base force). Our reading of the formula, not measured in the original:
    more, less, only on power attacks, and should a block take the push too?
    ([combat-rules, Knockback](spec-notes/combat-rules.md))
46. **NPC automatic fire comes in bursts (#1448).** An NPC's automatic gun (Securitrons'
    and other embedded weapons too) fires a V.A.T.S.-sized burst (0.4 s of its fire rate,
    3 to 8 rounds), then lets go for 0.5–1.5 s (scaled by the combat style's semi-auto
    delay multipliers) before the next. Before, it held the trigger for as long as it saw
    its target. The original's rule is unknown (the WEAP Short/Long Burst flags are not
    understood). Longer or shorter bursts, longer pauses, or Long Burst weapons firing
    longer? ([combat-ai](design/combat-ai.md), `fnv_combat::burst`)

## Tier 2: movement and controls feel

13. **Grab.** Z held, or E held for 0.4 s, picks up an object or a corpse. It drops when
    lagging 90 units and is thrown at up to 600 units/s. Nothing over 100 kg can be
    picked up, and you can only walk while holding over 25 kg. All guessed.
    ([grab](spec-notes/grab.md))
14. **Steps and road bumps.** Body and view follow the ground through a 0.2 s speed
    prediction and a spring (2026-11-07, after "every bump" on roads): road ripples are
    about 2.5-3x smoother, but running onto a steep hill the body is drawn up to 8 units
    below the feet for a moment (up to 24 units allowed; over 48 counts as a teleport). A
    step-up counts from 0.25 units of rise. Where a slope or stairs end the body carries
    on up: up to 9.9 units over the top of a 30° slope and 4.2 over the top stair at a
    run (6.1 and 1.2 walking; the old rule never overshot) [observed: replays, physics
    §6.11]; cutting it halves the slope's overshoot but doubles the road roughness. Smooth enough, or too floaty?
    ([physics §6.8.1, §6.9.1](spec-notes/physics.md))
15. **Third-person aim.** Shots and the activate prompt go to what is under the
    third-person crosshair, but the shot still leaves from the eye, so cover near you
    blocks it. Shoot from the camera's line instead?
16. **Releasing camera pan.** Letting go of the free-look key returns you straight to
    first person if you were in it. Did your item mean "stay in third person"?
17. **Controller.** Grab is on the right-stick press, Toggle POV on LB, and the D-pad is
    Hotkey 1 / Ammo Swap / Hotkey 3 / Hotkey 4. Look speed is 1.5/1.2 rad/s, squared, and
    you walk under 0.6 stick push. Keyboard and mouse are off when Controller is chosen:
    should both work? Rumble strengths are guesses. ([controller](design/controller.md))
18. **Seated view limits.** With "Body in first person" on, a seated view turns at most
    80° to each side and 75° down; with it off, all the way round. Limit both?
    ([player-body §1.5](design/player-body.md))
28. **NPC clips speed up with Speed Mult.** NPCs move at their Speed Mult (Legion raiders
    130-150, Lanius 180) and their clips play faster so the feet keep up. The wiki says
    the original's clips stay at 100 % (sliding feet), and one AI observer capture
    (Cook-Cook, Speed Mult 110) ran at the people's speed without his Speed Mult. Keep the
    matched feet, take the sliding feet, or have people ignore Speed Mult?
    ([actor-flags §3](spec-notes/actor-flags.md), [actors §8.10](spec-notes/actors.md))
38. **Variable refresh on your screen.** Display sync has a new "Variable refresh" choice
    for G-Sync: vsync stays on and the frame rate is held under your monitor's refresh rate
    (138 at 144 Hz). Does G-SYNC engage in borderless and in exclusive full screen (the
    NVIDIA G-SYNC indicator), and does Primm feel smoother than plain vsync? Should it be
    the default? ([display-sync](spec-notes/display-sync.md))
39. **Controller aim assist strengths.** With a controller, the stick turns 45% slower
    while the crosshair is on a hostile within 1800 units, and aiming down the sights
    eases the view onto the hostile nearest the crosshair (within about 8.7°) for 0.3 s.
    On by default, both strengths 0.6 on the settings page's Controller group. Too
    strong, too weak, or should either be off by default?
    ([controller §5](design/controller.md))

44. **No keyboard drop key on the Pip-Boy's ITEMS page (#1393).** Dropping is a right
    click on a row (Shift + right click the stack), and X is only the mod menu's, as the
    original's PC file gives it; the controller drops with X. A keyboard-only player can't
    drop. If you want a key, name it and it drops one (Shift with it the stack); free on
    that page are, for example, G or Delete. ([pipboy §6](spec-notes/pipboy.md))
48. **Controller buttons for menus whose files are not read (#1662).** Menu hints now
    name the controller button that presses their key. Where no menu file gives a button,
    these are guesses: Caravan bet Accept A, Auto Match Y, Raise RT, Exit B; deck Add A,
    Remove X, Randomize Y, Play Caravan RT (B leaves); game Select A, Discard Card X,
    Discard Track Y, Cancel B. Poker Check/Call A, Raise RT, All In Y, Fold X, Next Hand
    A, Leave B. The pause menu's X deletes (a save on the Load page, a binding on the
    Controls tab). The STATS page's RAD view: RadAway A, Rad-X X. Right buttons?
    ([controller §3](design/controller.md))
49. **PlayStation button names (#1452).** With a controller, Button style Auto (settings
    page, Controller group) shows Cross/Circle/Square/Triangle, L1-R3, Select/Start when
    the pad says it is a PlayStation one (DualSense, DualShock, "Wireless Controller",
    PS3/PS4/PS5, Sony), else the Xbox names; Xbox or PlayStation forces them. The INI keeps
    the Xbox names. The crosshair prompt draws NVForever's own outline glyphs for the four
    face buttons; menu hints write the names (the game fonts have no symbols). Select and
    Start rather than Share/Create and Options. Right choices, and do the glyphs look
    right? ([controller §3a](design/controller.md))

## Tier 3: rules where we chose to differ, or that the original has no answer for

19. **Walking away from a conversation.** The world freezes while you talk, as in the
    original, and walking off does not end it. Should walking off end it where the line
    allows leaving? ([dialogue](design/dialogue.md))
20. ~~NPCs opening scripted doors skip the door's OnActivate~~ (closed 2026-10-25: NPCs
    run the door's OnActivate first and go through only when it activates the door for
    them, as the original does; #729, [doors §5](spec-notes/doors.md)).
21. **Hidden Valley intercom.** Ours says Ramos's "[You hear nothing.]" and waits; the
    original says nothing and ends the conversation. Keep ours?
    ([talking-activators §3](spec-notes/talking-activators.md))
22. **Character revision traits.** Traits the script does not name (Skilled, add-on
    traits) stay, so the trait menu can grant up to two more. Clear them instead?
    ([chargen §9](spec-notes/chargen.md))
23. **Difficulty is an NVForever setting, not saved per game.** Companions are not
    scaled by it. ([difficulty](spec-notes/difficulty.md))
24. **Loading a save with missing plugins.** It loads with a notice and drops the
    missing content. It is refused only when you stand in a missing plugin's cell. Prompt
    first? ([save-load §4.1](design/save-load.md))
25. **Delete-save key.** Delete on the load page asks Yes/No. Which key do you want?
    ([save-load §6.1](design/save-load.md))
26. **Cloaked actors in VATS mode.** A Stealth Boy user or a cloaked Nightkin
    (Invisibility or Chameleon at least 1) can no longer be marked in VATS mode unless
    you have Enhanced Sensors (ED-E), which the perk's entry point names. Before, VATS
    mode marked them like anyone. Keep the refusal? ([perk-entry-points §13](spec-notes/perk-entry-points.md))
29. **VATS kill cameras on every last kill.** When the last mark of a VATS-mode run kills
    its actor, the game's camera paths play a kill camera every time: no chance roll and
    no setting to turn it off; the HUD stays up. Add a roll or a switch?
    ([vats-mode §19](design/vats-mode.md))
30. **The game's death cameras.** The masters' death cameras are loaded but not played,
    because your own death camera plays instead. Should the game's replace yours?
    ([vats-mode §19](design/vats-mode.md))
31. **VATS special attacks.** The weapons' special VATS attacks (the Machete's Back
    Slash, Mauler, Lights Out, Grand Slam: WEAP VATS/VANM) need a choice per mark, which
    VATS mode does not have, so they are not built. Wanted, and how would you pick one
    (a key cycling the mark's attack)? ([weapon-data-fields §7](spec-notes/weapon-data-fields.md))
32. **No message on a sneak attack critical.** Sneak attacks (your rule: always a
    critical on an unseen target) show nothing on screen; the game files have only the
    icon setting `sCriticalStrikeIcon`. Show a message or the icon?
    ([combat-rules §4](spec-notes/combat-rules.md))
33. **3D scopes (test version built, off by default).** The gun stays on screen and the
    zoomed world shows in its scope's lens: "3D scopes (test)" on the settings page, or
    `--scope-3d`. It works for 17 scope models (sniper, hunting, varmint/Ratslayer, 9mm,
    laser, trail carbine); other scoped guns keep the original overlay. It costs about
    1.7 ms a frame while aiming (5.7 → 7.4 ms in Goodsprings). Keep it, make it the default, and should the view around
    the lens zoom a little (now 0.2 of the way) or not at all? ([scope-3d-lens](design/scope-3d-lens.md))
33. **3D scopes (not built).** You asked about the mod look: the gun stays on screen and
    the zoomed world shows in the scope's lens. It can be built. Weapons with no lens in
    their model get a measured disc, and a few scopes keep the original overlay. It
    costs a second world render while aiming. Planned as a Gameplay switch, off = the
    original overlay. Should it be on by default, and should the view around the lens
    zoom a little or not at all? ([scope-3d-lens](design/scope-3d-lens.md))
34. **MCM and the pause menu (UI tile tree).** MCM lives in the original's StartMenu,
    which is where our own pause and settings pages are. Chosen (not built yet): our
    column and settings stay as they are. The game's StartMenu is built from its files
    behind them while the pause or main menu is open, so MCM's scripts find their
    tiles. A "Mods" entry joins our column only when a mod added one, and opens MCM's
    page drawn from the game's files. If you rule otherwise, the whole pause menu is
    drawn from the game's (or a mod's) start_menu.xml and our settings become one entry
    in it. ([ui-tile-tree §7](design/ui-tile-tree.md))

35. **Small corpses pass through each other.** Corpses under 64 units (rats, geckos,
   radroaches, bloatflies, mantis nymphs) no longer collide with each other, only with
   the ground, objects and bigger corpses; people, dogs and bigger still pile up. It
   cuts the physics cost of a heap of 20+ small corpses (#910). Look at a nest of nymphs
   killed in one spot: if corpses lying inside each other look wrong, the size limit can
   come down or the rule go. [ragdoll.md](spec-notes/ragdoll.md), "Corpse against corpse".

40. **Hotkeyed weapons come in holstered (your ruling, #1106).** A weapon picked by a
   number key or the weapon wheel is equipped holstered; a weapon in hand is put away
   first, and Ready Item draws the new one. The original may draw it. Kept here only so
   it is on record as a difference; if you want a drawn weapon to stay drawn on a swap,
   it is one function (`holster_for_new_weapon`). [weapon-wheel.md §4](design/weapon-wheel.md)
42. **Cheaper moon shadows (#1369, #1371): a look by eye.** By night the moon's shadows
   reach 3,000 units in one cascade at 2048² (by day: the settings' cascades and reach,
   2048² since #1367), and while the light is under a tenth of its strength at the
   horizon there are none. It saves about 2.5 ms on the Strip at 22:00. Not checked by
   eye: walk the Strip or open desert at night and see whether shadows ending at 3,000
   units or their softer edges look wrong, and watch sunrise and sunset for a visible pop
   when shadows switch off or back on at the fade. If they do, the reach can grow or the
   moon keep the day's cascades. [graphics-enhanced.md §3.2](design/graphics-enhanced.md)

43. **The first-person arms' shadows on themselves cost about 0.75 ms (#1374).** The
   arms' shade light now draws only what lies by the eye toward the sun, and far
   shadows (hills, buildings) come from the sun's own map: 15.4 → 12.4 ms at Novac
   07:00 with finer shadows. What is left is the light's own pass, about 0.75 ms
   whatever it draws, at 4096² maps (less at 2048² since #1367); it is what lets the weapon shade the hand and a
   roof right at the head shade the arms. If you rule it out, the light goes (12.4 →
   about 11.4 ms there) and the arms take only the sun's shadows from past the body.
   See [player-body.md §1.6](design/player-body.md).
45. **A second tutorial message: pages or replace.** NVForever keeps each message
    `ShowTutorialMenu` shows as a page, with a count, Next and Previous, so none goes
    unread. In the original the second replaces the first, with none of those. Keep the pages,
    or follow the original? ([tutorial-menu §4](spec-notes/tutorial-menu.md))
47. **Nobody picks a cross on their own (#1522).** Sandboxes and Sleep or Eat searches leave
    the crucifix (marker 10) out for everyone, people as well as creatures. A package that
    names a cross or links to one still works, so the crucified stay on theirs. This is
    [design]: whether a person in the original sandboxes onto an empty cross is still
    unanswered ([observer check 86](handoff/observer-checks.md)). If you want people choosing
    crosses again, the exclusion can apply to creatures alone.
    ([furniture.md §9](spec-notes/furniture.md))
50. **Every thrown grenade with a fuse rolls (#1785).** You said only thrown frag
    grenades roll. The rule takes every hand-thrown projectile with a fuse (a Lobber
    with an Alt. Trigger timer and no proximity), not mines, C-4 or launcher rounds, since
    nothing in the data sets the frag grenade apart. If you want the frag grenade alone, the rule can key on
    its PROJ. ([combat-rules §5](spec-notes/combat-rules.md))

51. **Corpses shot again move for 1 s (#1798), decided 2026-11-08.** A corpse moves for at
   most 3 s and is then held. A shot or blow on a held corpse now shoves it and lets it
   move for at most 1 s (`MAX_AWAKE_SHOVED`; it was 3 s). A blast, a fresh death, a body
   in water or one carried keep their limits. In the 50-a-side battle the ragdoll solver
   went from 4.4–6.2 to 3.4–3.6 ms a frame. Say if re-shot corpses should move longer
   (or not at all). ([ragdoll §4](spec-notes/ragdoll.md),
   [frame-rate-audit §5](design/frame-rate-audit.md))

## Not a gameplay decision: what the extender work needs from you (2026-10-25)


The extender plan (docs/handoff/extender-core-tasks.md, docs/design/extender-compat.md)
takes its facts only from the wiki and from captures of our own scripts in the original
game. Nothing here is urgent; each item unblocks part of the work.
1. **The rest of the wiki export.** `NVSE.xml` (2026-10-25) brought String Formatting,
   Hexadecimal Notation, User Defined Function, String/Array Variable, NVSE Error, the
   event priority and filtering pages and User-Defined Event, and `FOSE.xml` settled
   GetType (it is GetObjectType's alias), and you pasted Form Type IDs, the DirectX
   scancodes, the controller button and trigger codes, the known plugin names,
   GetCurrentEventName, DispatchEventAlt, sUActn and Player Character String Codes:
   thank you.
2. *Done 2026-10-25/26:* the GECK compile cases (results in ext-bytecode.md, #812), the
   probe sets with `ExtProbeList` (goldens collected; the L5.ext.nvse lab now matches
   225 of 228). Two newer GECK cases (24, 25) are in #831.
4. *Answered 2026-10-26:* target xNVSE 6.4.9.
5. *Done 2026-10-26:* the N2 scan ran over your MO2 profile `Talon` (128 plugins):
   docs/handoff/modlist-scan-2026-10-26.md. NVForever builds 44% of the extender uses
   there. The most used functions not built are the UI ones (SetUIFloat and friends,
   16 plugins). *Answered 2026-10-26:* a tile tree (§5.6 option A), to make the UI
   modular for modders; its own session writes docs/design/ui-tile-tree.md and the tasks. Your saves with their `.nvse` co-saves
   (`F:\Modding\MO2\profiles\Talon\saves`) may be read, read-only, for save rows.

## xNVSE probe results: decisions taken while you were away (2026-10-26)

The probe sets ran unattended in your game (tools/capture/ext_probe_run.py, 3 launches);
results are in the ext-* spec notes under "Observed in the original game".
- **Two probes crash xNVSE 6.4.9**, and NVForever does not copy a crash. *Decision*
  (session "NVSE core functionality tasks"): `Sv_Replace` with an empty old text replaces
  nothing and returns 0; `Ar_Range` with a step of 0 is a run-time error the script
  survives (an extender error line, as other failed calls). *Check:* say if you would
  rather mirror something else.
- **Three results differ from the GECK wiki; NVForever follows the game:** `Ar_Sort` of
  mixed types keeps the elements of the first element's type (the wiki says empty);
  `Ar_SortAlpha` sorts numbers by value (the wiki says as text); under `Begin _GameMode`
  a call's argument takes following arithmetic but not a comparison (the wiki's
  `Sin 14 == 1` example says the comparison is taken).
- **`Return 7`** compiles but returns nothing (the Call gives 0), so NVForever ignores a
  `Return` value as the game does.

## JIP LN: decisions taken while you were away (2026-10-26)

The JIP plan is docs/handoff/jip-tasks.md; read-only JIP getters were probed in your game
by runtime-compiled queries (tools/capture/jip_probes.py), results in the jip-* spec notes.
- **File functions (J9).** JIP reads and writes files in the game folder; NVForever never
  writes there. *Proposed:* writers write only under NVForever's own engine-home folder,
  mirroring the relative path; readers prefer that copy, then the game folder (read-only).
  docs/spec-notes/jip-math-utility.md §4. The same rule would cover mods' INI writes
  (SetINIFloat, the MCM settings files under `Data\Config`) and Con_SCOF's console log
  (docs/handoff/nvse-next-batch.md N3). *Check:* say if you want something else.
- **Arc tangents are approximate in JIP:** `fatan 1` and `fatan2 1 1` give 44.9998, not 45.
  *Default:* NVForever computes the exact value and the lab accepts a small tolerance on
  these two functions only, unless a sweep of probe points lets us fit the game's curve
  from observations alone. jip-math-utility.md §1.
- **`GetLNVersion` returns 57.3** in your game, where the wiki says it always returns 0;
  NVForever follows the game.
- **UI functions (J8):** *answered 2026-10-26*, a tile tree (extender-compat.md §5.6 A).
- **Two wiki pages missing from the export:** *Auxiliary Variables* and *RefMap Array*
  (export them by exact title: the 2026-11-08 category exports held only the function
  pages). *Template:ModIndexOverride* came in 2026-11-08: a private variable belongs to its
  plugin's mod index. The `*` name prefix makes a variable temporary [observed]; what makes
  it public is still open (docs/spec-notes/jip-variable-stores.md §1).
- **JIP implementation choices (J4–J7, branch claude/ext-compiler, 2026-10-26).**
  *Check* any you would rather have differently:
  - An invalid regular expression is an extender run-time error (the statement fails,
    the script goes on); the wiki says the game crashes.
  - JIP event registrations are not saved and a load starts with none (documented for
    the ragdoll event only). A second identical registration is kept once.
  - A lambda made inside a function script reads that script's variables **as they were
    when the lambda was made** (the function has ended by the time an event runs the
    lambda). The game showed the variables are seen; whether later changes are seen
    too was not observed.
  - SetGameMainLoopCallback counts frames (as documented), not time.
  - Not built yet: projectile and explosion hit data (GetHitProjectile, GetHit3DData),
    explosion damage in headless runs, the fire/reload/menu/key event firing points
    (engine side), GetFontFile (fonts, J8), the file functions (J9 waits on the
    proposal above).
- **xNVSE choices made while you were away (branch claude/ext-compiler, 2026-10-26).**
  *Check* any you would rather have differently:
  - The base-form setters (SetWeapon…, SetWeight, SetBaseItemValue, SetName …) change
    what the getters read for the session only. Combat, the menus and saves keep the
    record's values. Whether the original saves them was not observed.
  - DisableKey/DisableControl hide a key from the player's controls (the engine's
    ControlInput) and from IsKeyPressed. GetControl of an unbound control gives 255
    (the INI's "none"). Neither is observed.
  - The delayed calls count game seconds, which stop while a menu is open, so a
    CallAfterSeconds with runWhen 1 does not count down in a menu. The page says
    the time counts there too.
  - GetFirstRef and the …InCell functions treat the player's cell, and outside the
    cells within two squares of it, as loaded (uGridsToLoad 5). Elsewhere only
    persistent references count, which matches your save's rows for the saloon.
  - The remaining xNVSE functions, by family, are in docs/handoff/xnvse-remaining.md.
    The UI ones wait on the tile tree (§5.6 A, chosen 2026-10-26).

## Extender lab: rows that depend on your save (2026-10-26)

The L5.ext.nvse.* lab run of the extender branch (ext-compiler 1d2ee60b) matched 220 of
228 in-game probe rows. Three of the eight left depend on the save the probes ran in
(`nvfairport`): ToString of the player (your character's name) and the two counts of
the player's inventory (27 item kinds). `fnvtool ext probes` loads no save, so it cannot
match them. Nothing is marked or loosened in the lab: they stay failing until the lab
can start from a save. *Answered 2026-10-26:* the lab may read your save (read-only,
private, like the goldens).

## Two GECK compile cases for when you are back (2026-10-26)

Runtime-compiled scripts in your game refuse two things the GECK may accept:
`QFromEulerEx` and the other xNVSE 6.4.8 "Ex" quaternion functions, and a variable named
like a command alias (`ref rK`, ReleaseKey's alias). NVForever refuses the alias name
and accepts the Ex forms until the GECK says otherwise. Cases 24 and 25 in
`tools/capture/ext-compile/` (README there says how): note whether each saves.

## INI-file probes wrote into your game's Data folder, then cleaned up (2026-10-26)

To probe the INI-file functions (next batch N3) in your game, the probe session created
`Data\Config\` (it did not exist), put two test files there (`NVFExtProbe.ini`,
`NVFExtProbe\probe.ini`), and let the game's LU functions write to them and create
`NVFExtProbe\created.ini`. After the run the probe files were removed;
copies of what the game wrote are in `C:\fnv-private\ext\ini-files-after`. Nothing else
in Data was touched; NVForever itself still never writes there. *Answered 2026-10-26:*
probing the base game install (including writes the original game makes there) is
fine; only the NVForever application must never write into it.

*Answered 2026-10-26:* you installed MCM (MCM.dll and its ESP, not in the load order);
its script functions are probed through the DLL. MCM.dll made `Data\Config\MCM.ini` on
its own at start-up, so `Data\Config` now stays, holding only that file; the probe files
are still removed after each run.

## Probing extender state across saves needs a save in your saves folder (2026-10-26)

The extender core asked for N12 probes: set aux variables, ref maps, arrays and string
variables in the game, save, load, and read them back, to learn what the extenders keep in
a save. In the original game a save is written to `Documents\My Games\FalloutNV\Saves`,
which you have not cleared for probes the way you cleared the game install. Not done.
*Answered 2026-10-26:* yes, one save named `nvfprobe`, removed after the run, the owner's
own saves untouched.

## Code mods: the script host's language (2026-11-02)

Not a gameplay decision, and not needed before phase M5. The script host (an authoring
route with no compiler) runs either Lua or GECK/xNVSE-style script compiled by
NVForever's own compiler. Hooks for ESP scripts are a third option.
[code-mods](design/code-mods.md) §12.1, §17 item 8.
