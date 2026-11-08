# What is left (2026-10-18, brought up to main 2026-10-25)

> Rows below are struck or marked as later PRs closed them, up to #800 (2026-10-25). The concrete
> whole-project picture is [outstanding-audit-2026-10-23](handoff/archive/outstanding-audit-2026-10-23.md)
> and [project-audit-2026-10-23](handoff/project-audit-2026-10-23.md); what ESM data is still
> unused is [esm-audit-2026-10-25](handoff/esm-audit-2026-10-25.md).

Everything NVForever still lacks, in one place, ranked by what blocks a full playthrough
of the base game. Drawn from [status.md](status.md) (per system), the script function
coverage table ([design/script-function-coverage.md](design/script-function-coverage.md),
every function the shipped scripts use, probed against the original), the open items of
[owner-review.md](owner-review.md) and the outcome sections of the playtest lists in
[handoff/](handoff/). It stated the project's own state as of PR #417 (release v0.2.0) and is kept up since; facts about the
original game are tagged in the linked notes. Update it when a PR closes an item.

## 1. Playthrough blockers (systems the quests need)

Ranked by the quests whose scripts, stages, objectives or dialogue use the missing or
wrong functions (the coverage table's §6 count). Fixed since that table: NPC equipment
drawn from the rules (82 quests, #374), detection by and of the player (153, #376, #389),
abilities and worn enchantments (80, #377), corpses stood up again (76, #391), fast
travel (60, #393), knock-outs (essential and fatigue, #403; row 1 below keeps what is
left of knockdowns), used items and the remaining package types and flags (#400),
GetOwnerLastTarget (#403), the repair menus (#408, #412), the barber and plastic surgeon
(#414, #417), VATS mode (NVForever's own Dead Eye-style design, #413), race and sex
changes, SetLevel, power armour training, Cannibal and Mister Sandman, talking activators
(claude/actor-changes), Caravan, and IsSwimming, PickIdle, SetAllVisible, SetNoAvoidance and EnableLoadingMenu (script-stubs.md §3.19), IsLimbGone (dismemberment), AttachAshPile (ash-piles.md) and IsPlayerGrabbedRef with the grab (2026-10-21, [grab](spec-notes/grab.md)), ShowSlotMachineMenuParams with the slot machines (2026-10-21, [slots](spec-notes/slots.md)) and GetRadiationLevel (environment radiation, [radiation](spec-notes/radiation.md)). No function is a stub any more (trapUpdate with the swinging traps 2026-10-25, traps.md §4; GetPlantedExplosive went with pickpocketing, four more 2026-10-25, script-stubs.md §3.21). Saves from scripts and PlaceAtReticle
are built (rows 2 and 12, 2026-10-18,
[script-saves-and-reticle](spec-notes/script-saves-and-reticle.md)).

| # | System | Quests | What is missing | Functions |
|---|---|---|---|---|
| 1 | Knockdowns (done 2026-10-18; settled against the original 2026-10-19) | 64 (the intro chain among them) | Built ([unconscious](spec-notes/unconscious.md) §2, §5): knock-outs (essential, fatigue) and knockdowns (PushActorAway thrown from the source, explosions' knockdown) drop as ragdolls and get up with the game's clips; SetUnconscious holds a lying actor down until cleared and alone lays nobody down (robot pods, sleepers); a pushed actor's inventory can be searched. The player is knocked down too (§5.4, 2026-10-18): ragdoll, camera on the body (first person goes to third person for it), controls held, the get-up idle. The push's speed (30 units/s a force point), its rise (220 units/s), NPCs' 2.4 s down and the 2.5 s get-up are now [observed] (AI observer `knockdown_look`, unconscious.md §5.5), as are an essential's 10 s down and GetKnockedState 0 while getting up (§2.10); the player's camera and 2.4 s down are from the capture `kd_player_knockdown`. Left: an explosion's knockdown (tried in the original 2026-10-19: four placed explosions knocked nobody down, two on actors in furniture; npc-behaviour-compare.md §14), a body sliding on open ground | (none) |
| 2 | Saves from scripts (done 2026-10-18) | 29 | Built ([script-saves-and-reticle](spec-notes/script-saves-and-reticle.md)): SystemSave writes the one system slot, ForceSave a new save as by hand; both behind the scripts' save switch | (none) |
| 3 | Look turning the body (done 2026-10-18) | 26 | Built: with Look's rotate-body flag the whole body turns to the target until StopLook (an actor the AI runs at its walker's turn rate, a standing one on the spot with a conversation's turn; script-stubs.md §3). Settled 2026-10-19 [observed, AI observer `knockdown_look`]: 145°/s, the body kept on the target within a degree, a walker walks on facing it, no seat until StopLook, nothing turns it back after (script-stubs.md §3) | (none) |
| 4 | Animation state told to the rules (done 2026-10-18) | 22 | Built ([anim state](spec-notes/anim-state.md)): the engine tells each drawn actor's and the player's body every frame; GetAnimAction answers shots (2, 3, 4), blows, reloads (9), blocks and special idles by the New Vegas codes, checked against the AI observer; IsMoving gives the way, IsAnimPlaying an actor's movement group, IsIdlePlaying an actor on furniture, IsPlayerActionActive action 9 (iron sights). 2026-10-19 [observed, AI observer]: IsPlayerActionActive 4 is laying a mine (3.0 s, as 2 for a grenade; built), and the player's Cowboy Repeater read 17 when its reload began, the NPCs' round-by-round reload code (anim-state.md §4). Left: indices 1, 3, 5–8, 10 (firing, jumping, swinging and sneaking answer none, so a jump has no index to wire; standing on a table, grabbing and knocking things over were not tried) | (none) |
| 5 | Repair menu (done 2026-10-17) | 10 (Raul's quests among them) | Built ([repair](spec-notes/repair.md)): ShowRepairMenu opens a vendor's repair services (worn weapons and armour repaired for caps up to the vendor's skill), and the Pip-Boy's Repair (R on the ITEMS page) uses a like item or one on the item's repair list, reporting MenuMode 1035 (2026-10-18). The Pip-Boy's repair list takes the pointer (2026-10-19) | (none) |
| 6 | Barber, plastic surgeon and tutorial menus (done 2026-10-18) | 9 | Built: the character creator as the barber (hair, colour and beard only) and as the surgeon (face, eyes and brows only), character-creator.md §10, §11; the tutorial menu from the game's menu file ([tutorial-menu](spec-notes/tutorial-menu.md)), over the Pip-Boy for CGTutorial's crippled-limb help. Its code 1059 is confirmed by capture (mh1_tutorial); its words are guesses (owner-review) | (none) |
| 7 | Pip-Boy reset (done 2026-10-18) | 8 | Built (script-stubs.md §3.15): ResetPipboyManager puts the Pip-Boy's pages back to their defaults and its light off. The Pip-Boy can still be used without a reset (no "NPC Pip-Boy" state) | (none) |
| 8 | Talking activators (done 2026-10-18, awaiting the game's comparison) | 6 | Built ([talking-activators](spec-notes/talking-activators.md)): activating a talking activator talks to it; its talking actor (SetTalkingActivatorActor) speaks through it with its own lines and voice, IsActorTalkingThroughActivator holds meanwhile, the view turns to the activator. Capture ac6 matched (IsTalkingActivatorActor 1 for a script-set actor, against the wiki); the original's intercom, 2026-10-19 [observed]: the dialogue camera turns to the intercom, but no line is said and the conversation ends by itself within 10 game loops (saves nvftest and nvfstrip), where NVForever says "[You hear nothing.]" (talking-activators.md §3; open for the owner) | (none) |
| 9 | Menu codes (done 2026-10-18) | 6 | Built (script-stubs.md §3.15, `fnv_game::game::menus`): MenuMode 1 for the Pip-Boy, its pages' codes (1002, 1003, 1023, the repair list 1035), loading screens 1007 (with their `Begin MenuMode 1007` blocks), VATS marking 1056, the tutorial 1059. Left: the pause menu's 1013 (scripts do not run under it); the item mod menu reports 1061 since 2026-10-20, the slot machines 1103 since 2026-10-21 (a hypothesis, [slots](spec-notes/slots.md)); the companion wheel reports 1075 (a hypothesis) | (none) |
| 10 | Caravan (done 2026-10-19, awaiting the owner) | 5 | Built ([caravan](spec-notes/caravan.md)): ShowCaravanMenu opens the bet, the deck builder (30 cards at least), the game and the results against the speaker; the rules from the game's help messages and the owner's rulings (Kings double again, out of cards loses, caps paid out); AddCardToPlayer keeps the player's cards, saved. Guesses: the opponent (its skill from its deck's face cards, owner ruling), the raise step, the layout (drawn flat, the original's is 3D). Bets in NCR dollars and Legion coins built (2026-10-19, caravan.md §3.1). The Caravan challenges count since claude/e3-challenges (Caravan Games Won/Lost, [challenges](spec-notes/challenges.md)) | (none) |
| 11 | Race and sex changes (done 2026-10-18) | 5 | Built ([appearance-changes](spec-notes/appearance-changes.md) §2): AgeRace along the RACE age links and SexChange on any actor, drawn anew; SexChange's reset gives the player the first starting face (a guess). Every shipped call is on the player (the opening's VCG00 and VCG01) | (none) |
| 12 | Placing at the crosshair (done 2026-10-18) | 5 | Built ([script-saves-and-reticle](spec-notes/script-saves-and-reticle.md) §4): the new reference goes where the crosshair's ray first hits, within maxDistance | (none) |
| 13 | A leveled actor's level re-rolled (settled 2026-10-18) | 4 | Nothing to re-roll: the original keeps an NPC's inventory on SetLevel (capture ac1, appearance-changes.md §3), as NVForever does; every shipped call is on the player | (none) |
| 14 | Cannibal and Mister Sandman (done 2026-10-18, awaiting the owner) | 3 | Built ([perk-activation](spec-notes/perk-activation.md)): the perks' Activate choices open a choice list on E (Devour, Murder, then the default action); StartCannibal, StartMisterSandman, GetCannibal, GetSandman, HasBeenEaten; idles and sounds. Guesses: how the list looks, the Cannibal act lasting its 7.7 s clip, the crime (Mister Sandman settled by captures: a sleeper dies at the act's end, anyone else is refused) | (none) |
| 15 | VATS | 3 quests, 250 conditions | VATS mode built (vats-mode.md), awaiting the owner | GetVATSValue, GetVATSMode and GetVATSTargetHeight answer for VATS mode; SetVatsTarget makes placed objects VATS mode's targets (2026-10-25, vats-mode.md §20) |
| 16 | Miscellaneous stats, scale | 3 each | Built (misc-stats.md, 2026-10-18): all 43 statistics have codes, 28 counted by the rules and the engine, the Pip-Boy shows them; SetScale drawn and collided. Not counted: the 14 of misc-stats.md §3 (pickpocketing, dismemberment, New Vegas speech checks; Books Read counted since 2026-10-21, claude/e4-skill-books; Slots Games Played counted since 2026-10-21) | (GetPCMiscStat, SetScale) |
| 17 | Dismemberment, films, cell image space (films and cell image space done 2026-10-18) | 2 each | PlayBink built ([bink-films](spec-notes/bink-films.md)): a scripted film plays full screen with its sound over the held game, then the game goes on; the 4 shipped calls are all the opening's (the intro film, which the main menu already shows, and three films not in the install). SetCellImageSpace built: kept in the save, shown when the cell loads again as the original does. Dismemberment built 2026-10-20 ([dismemberment](spec-notes/dismemberment.md)): sever and explode by the weapon's On Hit, crippling and overkill; stumps from the skins' partitions and caps; severed parts cut from the ragdoll; exploded parts throw their gore models' pieces and debris, lootable; IsLimbGone, GetKillingBlowLimb, KillActor's limb, the Dismembered Limbs statistic. Not built: blasts and NPC melee dismembering, decals, pinned limbs | (dismemberment) |
| 18 | One quest each | 1 each | Done 2026-10-18: screen splatter ([screen-splatter](spec-notes/screen-splatter.md); fitted to the original's capture; script-stubs.md §3.18); AutoDisplayObjectives (the HUD keeps the objectives on screen), ExitGame (back to the main menu; no credits) (script-stubs.md §3.15); the AI package codes (Sandbox, Travel-at-goal) and the navmesh switches (script-stubs.md §3.16); SetPCCanUsePowerArmor (power armour needs the training, appearance-changes.md §4) | |

Rows the coverage table ranks with 0 quests are not listed. The table's own §3.3 lists
six functions still waiting for an in-game capture to settle their answers.

## 2. Systems not started or partly built

From [status.md](status.md); the playthrough blockers above are not repeated.

- **Gameplay**: VATS mode built for guns, bursts, melee (teleport and swing within 5 m) and thrown weapons (2026-10-20; smoke scenarios vats-mode-burst, -melee, -thrown, melee run 2026-10-23; hit chances per mark, the VATS perks and kill cameras since 2026-10-23/24, vats-mode.md §17–§19; left: mines, power attacks; perk AP changes built in claude/e2-perk-combat-entry-points); ~~melee
  weapon use through UseWeapon~~ (built 2026-10-20, use-weapon.md §3.1) (the companion wheel, aggressive/passive, sneaking with the
  player and a melee-only companion drawing its melee weapon are built,
  [companion-wheel](spec-notes/companion-wheel.md)); ~~perks that change skill points~~ (built 2026-10-20, levelup.md §9; reading skill books with Comprehension built 2026-10-21, [skill-books](spec-notes/skill-books.md)); ~~traps~~ (built 2026-10-21, [traps](spec-notes/traps.md): model trigger volumes, activate parents, rigged shotguns firing; the swinging traps' bodies and `trapUpdate` built 2026-10-25, §4); ~~grab~~ (built 2026-10-21, [grab](spec-notes/grab.md): Z held or E held picks up loose objects and corpses, carries, drops, throws; OnGrab, OnRelease, IsPlayerGrabbedRef; every constant guessed, the settings' values are not in the plugins); ~~slot machines~~ (built 2026-10-21, [slots](spec-notes/slots.md); the payout rules are guesses); ~~encounter zones~~ (built 2026-10-21, [encounter-zones](spec-notes/encounter-zones.md): zone levels on first entry, leveled actors and carried items at the reference's level, level modifiers, Never Resets; leveled locks not built, no rule documented); ~~death items~~ (built 2026-10-21, [death-items](spec-notes/death-items.md): NPC_/CREA INAM through the Traits template, rolled into the body at the reference's level as it dies, placed corpses owed theirs, and the perk entry point Add Leveled List On Death; JIP GetDeathItem/SetDeathItem not built); ~~vendor services~~ (built 2026-10-21, [vendor-services](spec-notes/vendor-services.md): vendors sell only their AIDT service categories, GetOffersServicesNow follows Offers Services; the ingestible categories and Autocalc Service are guesses); reputation disguises and resets.
- **Animation**: open in [actor jumps](spec-notes/actor-jumps.md) (built 2026-10-20): how the original takes ledge-up links and obstacles.
- **Rendering**: the clouds' placement against the game's frames (2 of 8 exterior
  reference frames meet the threshold, all 7 interior ones; looked into 2026-10-21,
  layer3.md §5 item 5: the light's strength is not the gap, the clouds are; needs the
  owner's frame timings and still-sky captures); ~~depth of field~~ (built 2026-10-20, image-modifiers.md §8; constants to set against game frames),
  ~~motion blur~~ (built 2026-10-20, image-modifiers.md §9; constants to set against game frames), ~~vertex morphs~~ (built 2026-10-20, vertex-morphs.md: flags and holograms; morphs a script or a non-idle sequence starts are not), ~~the screen splatter's flare and the blood when the player is hurt~~ (built 2026-10-20, all guessed, screen-splatter.md §4); ~~effect shaders' holes and add-on models~~ (built 2026-10-21, effect-shaders.md §7-§8; meanings guessed from xEdit's names), ~~ash piles~~ (built 2026-10-21, ash-piles.md: crit effects On Death, AttachAshPile, the pile searched; GooStart/DisintegrateStart ragdoll changes not built); ~~about
  66 particle-system smokes not drawn~~ (playtest list 4 item 16; 2026-10-21, particles.md §7:
  placed models' systems per reference, nearest add-on emitters served first, script-enabled
  references and snow globes emit; open there: the reactor's reset-on-loop controller,
  placed dynamite's fuse, the rock-slide boulders).
- **Interface**: ~~credits~~ (ExitGame rolls Credits.txt since 2026-10-25, `nvf-play` credits.rs). ~~Weapon scopes~~ (built 2026-10-21, [weapon-scopes](spec-notes/weapon-scopes.md): the MOD3 picture, night-vision scopes and the sway; left: the scope in third person, the original's timing). The weapon mod menu is built (2026-10-20,
  [weapon-mods](spec-notes/weapon-mods.md)): X on a weapon lists the carried mods that
  fit a free slot, attaching one uses it up and sets the weapon's mod flags (kept in
  saves), the player's figures and model in hand change; MenuMode 1061. ~~Modded models in the world and NPCs' hands, Decrease Weight's carried weight, rows
  picking units by mods~~ (#499), ~~projectile speed and ammo regeneration~~ (claude/em3, weapon-data-fields.md),
  the mod shoot sounds (claude/e12-silencer-sounds, weapon-mods.md §7). ~~A modded weapon's value~~ (o26, #982). Left (weapon-mods.md §6):
  NPCs' modded figures in combat, equip time, GetMenuTargetRef 1061. (The ITEMS page's DPS, ammo badge and Mod hint,
  the clickable Pip-Boy repair list, the +10 XP line and Locations Discovered are built,
  2026-10-19.)
- **Audio and video**: spatial sinks; ~~holotapes~~ (built 2026-10-21, [holotapes](spec-notes/holotapes.md); left: subtitles, the JIP note functions, GetNoteRead); ~~reverb's SOUN `GNAM` attenuation~~ (built 2026-10-24, #667, audio.md §15.5), ~~voices in rooms~~ (built 2026-11-01, #987, audio.md §14.6); ~~a film's `muteAudio 0` (world sounds under a film)~~ (done 2026-10-21, [bink-films](spec-notes/bink-films.md) §2).
- **Magic effect archetypes** (built 2026-10-21, claude/e15-mgef-archetypes,
  [magic-archetypes](spec-notes/magic-archetypes.md)): left are guesses for the owner:
  Turbo's 0.5, what of the player Turbo exempts (movement and its clips; not jumps, fire
  rate or reloads), the concussion's rules, paralysis of "No Knockdowns" actors.
- **AI and packages, ESM data** (built 2026-10-21, claude/em1-ai-packages, packages.md §7):
  "Can't Open Doors" creatures kept from load doors, package Begin/End/Change topics and combat style, patrol
  points' topic and script, Run On "Combat Target", Friendly Fire Comments. Left
  (packages.md §7.7): ~~Observe Combat, Reaction To Player Actions and Idle Chatter (their
  topics are not said yet), Avoid Radiation (paths)~~ (built 2026-10-25, #757,
  packages.md §7.6b; Observe Combat's running away is not), Guard "Remain Near Reference" and
  stay-at places by object (undocumented), No AI Acquire (no actor picks things up), DOOR
  Minimal Use (undocumented), what such creatures do at a shut hinged door. Owner check: the Camp Searchlight feral trooper ghouls'
  FollowEdwards travel now needs a door they cannot open (`fnvtool ai reach`).
- **Saves**: ~~AI progress~~ (patrol points and sandbox days kept since 2026-10-25, save-load.md §5 item 4; waits, walks and errands start again) (deleting saves from the load page built 2026-10-19, save-load.md §6.1; loading into another load order built 2026-10-21, save-load.md §4.1).
- **World** (done 2026-10-20): clutter skeletons' collision lies in their stored `XRGD`
  pose with their drawing (playtest list 5 item 71, dead-on-spawn.md §5.1); not checked in
  game against the original.
- **Combat cover from the shooter** (done 2026-10-19): a gunman takes cover from the
  enemy gunmen firing at it first, nearest first, then from its own target
  (docs/design/combat-ai.md §12.6.1); smoke combat-cover passes again without its three
  known_failing marks.
- ~~**AI packages**~~: done (the package types and flags are carried out since #400;
  Find's Allow Buying and an escort joining its charge's fights since 2026-10-20,
  packages.md §3.1, mostly hypotheses: no shipped Find package has the flag).
  - Find's Allow Killing: settled 2026-10-19 [observed, AI observer `gecko_find`]: an
    Unaggressive gecko on FindFood started fights with bloatflies, radroaches and a
    protectron that carries no food at all, so a creature on Find with Allow Killing now
    attacks any actor of its area that is not kin, ally or friend (`fnv_ai::errand`;
    owner-decision-captures.md §6). People keep attacking only one carrying what they look
    for [hypothesis].

## 3. Decisions waiting for the owner

[owner-review.md](owner-review.md) is the full list, condensed on 2026-10-24 to the open
gameplay decisions (the old running log: `git show 85dcaace:docs/owner-review.md`). On 2026-10-19, 54 items that later
work had overtaken were struck through, each with what overtook it, and the second run of
owner-decision captures ([spec-notes/owner-decision-captures.md](spec-notes/owner-decision-captures.md)
§7) answered or narrowed the factual ones. Those still open are listed in §4. What is left
here needs your ruling: a lab rule, or a choice the original's behaviour does not make for us.

**Lab rules** (only you may change them, parity-lab/AGENT.md):
- the `L5.script.decode` choice;
- the `delay_*` scenario inputs;
- the holdouts to demote;
- where `encodings.json` lives;
- the `combat-violet` and `combat-dogs` known-failing marks.

**Your choice** (the original's behaviour is known, or does not decide it):
- **Pausing in conversations** (owner-review.md item 19; item 34 of the old log).
  - **What the original does.** It stops the game clock, and an attacker does not act
    while the menu is open (captured 2026-10-19). NVForever does the same now.
  - **To decide.** Whether the "live world" proposal should change that, and whether
    walking away ends a conversation.
- **Layer 5's runtime rules where the sources were unclear:** stage conditions, timers,
  reputation tolerances, `ref` handling and Return. Each is a reading of the wiki;
  no capture was designed for them.
- **Assault and the faction** ("Karma and crime" item 8 of the old log,
  `git show 85dcaace:docs/owner-review.md`). The original agrees
  with our decision (captured 2026-10-19); only your sign-off is left.
- **Looks and constants by area** (not quest-critical), to compare by eye in the game:
  - rendering, menus and interface, combat, AI, dialogue, animation, world and physics,
    gameplay, sound;
  - the repository and install: old remote branches to delete, figures to place in docs.

**Not reproduced** from the playtest lists: the first-person shadow flicker after leaving
an interior; the Crimson Caravan crash. **Never checked in the game** after fixing: Victor's
and Sunny's stop-and-go (list 4 items 2–3).

## 4. Found by today's comparisons with the original

From the owner-decision captures (2026-10-18 [observed],
[spec-notes/owner-decision-captures.md](spec-notes/owner-decision-captures.md) §4). Both
confirm rules the runtime already had; the owner-review wording that said otherwise was
out of date (#441 adds tests):
- **AddScriptPackage replaces** the actor's script package; it does not stack. One
  RemoveScriptPackage brings back the list package.
- **Disabled references still run their GameMode blocks** (the skull's timer kept falling
  while it was disabled).

From [spec-notes/npc-behaviour-compare.md](spec-notes/npc-behaviour-compare.md) §8:
- Fights end far sooner in ours (NCR against Fiends over in 32 s; the original's still
  going after 120 s). Measured again 2026-10-19 (npc-behaviour-compare.md §9): the
  original's MoveTo reaches only four of the six troopers (two are non-persistent); with
  the same four ours lasts 47 s and Cook-Cook dies, where the original's Cook-Cook (215
  Health, a flamer) kills all four by 89–99 s. Hit rates are alike (0.28–0.49 a shot in
  the original); but a hit does two to fifteen times the original's damage in ours (a
  median 6.2 on Cook-Cook against 2.0; 22–30 a Fiend's shot on a trooper against under 2),
  measured with smoke `battle-ncr-fiends-four`'s `npc hits summary`. **Settled**
  (2026-10-19, combat-rules.md §18). PR #475 fixed the causes: worn leveled items, the
  documented condition penalties, no NPC armour wear and NPC Health. Ours now gives
  Cook-Cook a median 1.9 a hit against 2.0 and he wins, as in the original. Offline
  (`fnvtool combat damage` with the records' values), a trooper's Service Rifle does
  1.84–2.19 on Cook-Cook, the original's figures.
  - Blasts were all credited to the player until the same day (combat-rules.md §11):
    an NPC's Incinerator burst on a trooper gave the player XP, ran OnHit with the
    player as the attacker, and was missing from `npc hits summary`. They are now
    their shooter's.
  - Left:
    - the troopers die sooner than in the original (16–31 s after the first fighter
      engaged against 30–132 s). Explained (npc-behaviour-compare.md §9.1): Cook-Cook
      drew his Flamer instead of his Incinerator (fixed 2026-10-20, branch
      claude/q9-trooper-time: the weapon choice counts a gun's explosion,
      actors.md §9.3); left, the Incinerator's burn drains 4 Health a second in ours
      against 1–2 in the original, cause unknown: needs a capture of one burn;
    - NPC weapons now wear from their own shots and blows by the documented per-shot
      rule, the player's (2026-10-20, combat-rules.md §17–18). The original's Incinerator
      losing 2.19 a burst is still not explained by it; 2.19 is a trooper's hit on
      Cook-Cook to two places, so the falls may come from hits he took. Settle it by
      running campaign fight_stats_round again and reading it with
      `tools/capture/fight_stats.py` (now splits weapon wear into firing and hits).
      Ours drew Cook-Cook's Flamer where the original's drew the Incinerator until the
      weapon choice counted a gun's explosion (2026-10-20, above);
    - the original's hits split by attacker, to check the Hunting Rifle's 22 a hit on a
      trooper.
- Melee attackers keeping away from a god-mode player: not god mode's doing. In the same
  room a knife, a sledgehammer and a dog closed in and swung alike with and without god
  mode (npc-behaviour-compare.md §9), as ours do; the outdoor hold-off is unexplained.
- **The Strip's crowd** was observed on 2026-10-18 [observed]: campaign `strip`, session
  `strip_evening`, from `nvfstrip`, 5.9 game hours, 1,442 samples, 900 state changes. The
  timeline is private, on the coordinator's desktop
  (`C:\fnv-private\capture\observe\strip.timeline.json`). The crowd ran on its packages:
  - Travellers and gamblers walk between Gomorrah, the Lucky 38 and the gate on
    VStripFluff1PortRNDTo{Top,Gom}Package (90–107 segments each).
  - Hookers 01 and 03 hold their Dance packages; 02 patrols.
  - The NCR MPs patrol and check (VStripMPABPatrol38, VStripFluff1MPCheckA), with escorts
    (VStripSS2Start*Escort).
  - NCR troopers A–D ran the SS2 travel / drunk-wall chain for 0.3–1.0 h, then left the
    sampled area.
  - Mr. Holdout sat on VMrHoldoutHangOutSell, and the securitrons held their gates.

  ~~It is not compared with our engine yet.~~ Compared 2026-10-20 (queue item 13,
  npc-behaviour-compare.md §10.1, smoke `strip-crowd-evening`: the player comes through
  the gate door, whose script starts the crowd and the SS2 scene): the crowd's packages,
  the travellers' fade moves and troopers C/D's SS2 chain now match within a few game
  minutes (fixed: a timed script package stays until its duration is up). Still open:
  what ended MP A's escort patrol and troopers A–C's patrols mid-way in one sample in the
  original (ours end 0.4–0.5 h later); positions not compared per sample. Of the
  casinos, only Gomorrah's first 12 game hours were observed.
- No spawn aid for creature fights in our engine's test runs.
- **Owner-decision captures still open after the second run** (2026-10-19,
  owner-decision-captures.md §7). Each has the reason it could not be settled:
  - **Teammates through doors.** A teammate on a PATROL package came along by MoveTo and
    through a door; one with `Waiting` 1 did not. Ours now brings every teammate not on a
    Guard (Wait) package. That `Waiting` is the reason, and where they arrive through
    other doors (left 160 through the room's door out, on the spot coming in), are open.
  - ~~**SetActorsAI 0.** In the original the actor keeps walking on its last heading; ours
    stops.~~ Built 2026-10-20 (set-actors-ai.md): it walks on through the character
    controller. Open: what happens at walls and whether the walk clip plays (guessed).
  - **Scripted items in NPC inventories.** The Legion aura token's script did nothing even
    in the player's inventory, so it cannot answer; another item is needed.
  - **Topic lists: a generic Goodbye beside a scripted one; Info Connection's shared
    lines.** The original's lists match ours in their first three entries, but the plugin
    cannot scroll the list. A look by hand at Johnson Nash and Red Lucy settles both.
  - **The end of a Random pool (#120).** The plugin could not see which GOODBYE line Ralph
    said: no subtitle, and SayToDone names the topic only. A look by hand settles it.
  - **ResetAI on a sleeper.** Ringo never lay down in the gas station.
  - **Actors with no greeting.** Two of the three had one after all, so the rule still
    has no clean test case.
  - **SetPackDuration's unit.** Hidden Valley's knights, whose packages run
    `SetPackDuration 60`, changed package every 10 to 70 game minutes, not every 60.
  - **IsInCombat while on Alert.** The detection Alert was never seen. IsInCombat stays
    1 while a fighter has lost sight of its target, as ours does.
  - **Force-greets' Goodbye camera.** No capture was designed for it.
  - **A quest's own conditions on the speaking actor.** No capture was designed for it.

## 5. Process

- **Smoke scenarios on main** (2026-10-18): the failures #403 found are fixed (#411,
  #415, #416). `death-cam-first`'s and `death-cam-third`'s rest checks, long marked
  `known_failing`, pass again (2026-10-21): the rest rule read the bodies' velocities, which
  a limb shivering between the ground and a joint limit kept high while it went nowhere; it
  now reads the bones' net motion ([ragdoll](spec-notes/ragdoll.md) §4 "Rest"). Several scenarios
  failed only because the scripts started late on a loaded machine and a test aid fired
  first; scenarios now put console and script steps at 14 s or later and use
  `--no-messages` where the add-on packs' start-up boxes could get in the way. The
  runner-side fix is built (2026-10-18): `--play` runs set NVF_PLAY_AIDS_FROM_SCRIPTS=1,
  and the world test aids wait for the scripts' first frame, held back by as long as it
  came after 6.5 s (`nvf-play` pause.rs WorldAidClock). The steps moved to 14 s only for
  the race (barber-menu, anim-state) can go back once coordinator runs pass with it.
- **Coordinator-run smoke**: since todo 6 (2026-10-17) worker sessions do not run the game;
  the coordinator builds each PR merged with main and runs its scenarios in one ordered
  queue, so the shared game is never contended.

- **Game lock queue** (done 2026-10-19): smoke runners that wait for the game locks take
  a ticket in `smoke-queue` beside the first lock and try the locks only while theirs is
  the oldest live ticket, keeping it for the whole run, so a multi-scenario run is no
  longer split up and early waiters do not starve (`tools/smoke/smoke/runner.py` `Queue`).
  Plain `mkdir` lockers (captures) take no ticket; the runner at the head of the queue
  polls every 5 s so they do not keep winning the race.
- **Smoke slots** (2026-10-24): up to 3 nvforever smoke games at once on the machine
  (`tools/smoke/smoke/slots.py`). A slot is an OS file lock in `.claude\smoke-slots\`
  (it goes with a crashed runner); while any is held, `.claude\smoke.lock` is a shared
  lock (owner file `shared …`) so a capture's `mkdir` fails and it waits. Smoke only waits
  on `run.lock`, never takes it. The first 3 slot runners in the queue split the slots
  (oldest gets the remainder); an old-style ticket ahead (function_probe, the stress tool,
  `--exclusive`, an older runner) gets its turn alone. Launches with the same
  `NVFOREVER_HOME` run in order on one worker (save-write before save-load).
- **`scenarios.toml` merges**: git's merge of two branches that both append scenarios at
  the end drops the `]` and `[[scenario]]` lines between them (at least five times on
  2026-10-16/17). After a merge, check the file parses and holds every scenario of both
  sides.
- **The README and this file** go stale fast with several sessions merging a day; update
  them with the PR that changes a system's state.
