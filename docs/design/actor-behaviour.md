# Actor behaviour design: AI packages

Owner request 2026-10-09: after navmeshes, "start up a branch for AI packages"; the AI
must be able to tell that an actor can get to where its package says, through several
interiors and worldspaces if need be. Facts and tags: `docs/spec-notes/packages.md`
(records, documented rules, what the game was observed doing) and
`docs/spec-notes/actor-behaviour.md` (the editor documentation on packages, detection,
factions, idles). Navigation: `docs/design/navmesh.md`. Combat AI is planned in
`docs/design/combat.md`.

Status 2026-10-10: steps 1 to 4c are **done**: the records, the typed reader, the
package choice, actors going where their package sends them over the navmesh (headless),
and in `nvforever` (`--npc-packages`, §5.4) for named actors or every actor of the loaded
cells, on the game's own state in `--play`, moved by the character controller near the
player. Dialogue packages aimed at the player (the force greet) are carried out since
2026-10-10 (§5.6); every package type is carried out since 2026-10-17 (§5.10). **Next**:
the behaviours not carried out yet, and the notes in §7. The six package types not yet seen running in a capture (Find, Follow, Escort,
Wander, Use Item At, Use Weapon) are set aside until the owner captures them.

## 1. The plan

| Step | What | State |
|---|---|---|
| 1 | PACK layout (`schemas/fnv/ai.json`), `fnvtool ai survey`, spec notes | done |
| 2 | Typed reader `fnv_game::package`; `ai packages`, `ai show`, `ai observed` | done |
| 3 | Package choice `fnv_game::ai`: schedules, list order, validity, flags, periodic choice; `ai day`, the replay in `ai observed` | done |
| 4 | Carrying packages out, headless: crate `fnv-ai` (a package's goal, reachability with keys, the walk, arrival reported with `package_goal_met`, unreachable places); `ai run`, `ai reach` | done |
| 4b | The same in the engine: `nvforever --npc-packages` (the named actors live by their packages, drawn and animated) | done |
| 4c | Every actor of the loaded cells (`--npc-packages all`); the package choice in `--play`'s own game state; the character controller near the player; the sky on the game's clock | done |
| 5 | Checks against the game: the observer timelines hour by hour, new captures | partly (the replay; `ai run --samples`) |

## 2. Where things live

- **`fnv-game`** (headless, no Bevy): `package` (the data), `ai` (the choice). The game
  state already holds each actor's current package and script packages (Layer 5).
- **`fnv-nav`**: paths and reachability. `fnv-game` does not depend on it.
- **`fnv-ai`** (headless, no Bevy): the layer that moves actors; it asks both.
- **`fnvtool ai`**: every rule can be inspected without the engine (spec section 7).
- **The engine** turns the AI on with `GameConfig::package_interval` once step 4 gives
  packages something to do.

## 3. Choosing a package (step 3)

`Game::select_package` and `Game::check_package`, `crates/fnv-game/src/ai.rs`. Each rule
with its source; "observed" figures are from the owner's eleven AI observer runs in
Goodsprings (packages.md §4).

1. **A script package first** (AddScriptPackage) [documented; observed].
2. **The list top to bottom, the first valid package** [documented; observed]. A package
   is valid when, in this order:
   - its **schedule** allows it: from its starting hour for its duration, across
     midnight; any time of day without a starting hour [observed]. The day of the week,
     month and date are checked against the calendar; a window that began before
     midnight belongs to the day it began on [hypothesis];
   - it does not need a **linked reference** the actor lacks [documented for Patrol and
     Ambush; observed for the default patrol; hypothesis for other types];
   - it is not a **Once Per Day** package chosen less than 24 game hours ago
     [documented]; while it is still running it may go on [hypothesis];
   - its **conditions** pass, the actor as the subject and the package's target as the
     target [documented]. Conditions are not evaluated outside the schedule
     [documented], which also keeps random conditions from being rolled needlessly.
3. **No valid package: keep the current one** if it is a package of the list
   [observed; hypothesis that it is the current rather than the last of the list].
   An actor that has none stays without a package.

Two ways of choosing:
- **`evaluate_package`** (the `EvaluatePackage` function, and the first choice): always
  chooses; even a Must Complete package is dropped [documented].
- **`update_package`** (the game's own periodic choice): leaves the current package
  alone while it is **held**: Must Complete or Must Reach Location and its goal not
  met, or Continue If PC Near with the player in the actor's cell [documented]. The
  goal is reported by the layer that carries the package out (`package_goal_met`).

**How long a package runs** (2026-10-13, playtest list 4 item 18; only with
`package_interval` set): a list package with a **duration and no starting hour** ends
that many game hours after the actor chose it, **once its goal was met** (until then it
runs on, and it ends as soon as the goal is met) [observed 2026-10-18/19: the Goodsprings
geckos' FindFood and patrol ran 4.7 to 13 h when unfinished; owner-decision-captures.md
§4, §7] (`start_schedule_duration`,
`end_timed_packages`, the same ending as SetPackDuration's: end script, then choose
again) [documented: GECK wiki SetPackDuration "sets the duration of the actor's current
package", the Schedule tab gives it in hours; Continue If PC Near holds a package "even
if the package should have expired"; hypothesis: counted from the choice]. A **Once Per
Day** package that ended is not chosen again that day ("not immediately selected again
after being completed" [documented: Category:Packages]), so the next valid package of the
list takes over; any other package may be chosen again at once and starts afresh. A held
package (Must Complete or Must Reach Location before its goal, Continue If PC Near with
the player in the cell) does not end by its duration [documented]. With nothing else
valid the ended package goes on [hypothesis]. Packages with a starting hour end with
their window, as before; script packages are left alone [our choice]. The game data is
built for this [observed]: the Goodsprings geckos' list is VCrGeckoFindFood (Once Per Day,
2 h), VCrGeckoPatrolRunningLinkedRef (3 h), VCrGeckoFindFood02 (2 h),
VCrGeckoPatrolRunning02LinkedRef (3 h), then a sandbox with no duration. A Find package
that finds its target is not ended early: its goal is met (§5.10), and no source read
says what "found" ends.

**When** (only with `package_interval` set): every actor in the loaded cells, and every
actor that already runs a package, chooses when first seen and then every interval
(10 seconds [documented: "seem to be"]). Each actor has its own phase within the
interval, from its FormID, so they do not all switch in one frame [our choice; observed
switch delays after a schedule boundary are spread from 0.4 to 26 real seconds]. Dead
and disabled actors do not choose. Menus stop the clock as they stop scripts.

**Package scripts** (only with `package_interval` set): a new package runs its begin
script and OnPackageStart; a package replaced before its goal runs its change script and
OnPackageChange; a package whose goal is met runs its end script and OnPackageDone
[documented: the three state changes; hypothesis: no change event after the end event].
Without the interval nothing runs by itself and the Layer 5 runner's `package` input
stands in, so Layer 5 scenarios behave as before apart from the validity rules.

## 4. What the checks show

- `fnvtool ai observed` replays the choice code over 234,335 samples of list packages:
  the same package in 186,563; in 47,721 a package with conditions lies between the
  two (the capture's quest state is not known to the replay); 51 differ with no
  condition involved, all of them an actor still in its old package during the seconds
  a switch takes.
- `fnvtool ai day --actor <ref>` prints an actor's day (package per hour, and why each
  list package is or is not valid) for the authored game.
- Unit tests on hand-built packages: `crates/fnv-game/tests/package_choice.rs`.

## 5. Carrying packages out (step 4)

Crate `fnv-ai`: `goal.rs` (where a package sends its actor) and `carry.rs` (the actors
the AI moves). `Ai::update(game, nav, dt)` runs after `Game::tick`, which makes the
periodic choice.

### 5.1 The goal of a package

| Package | Goal |
|---|---|
| Travel, Sandbox, Sleep, Eat, Guard, Wander | its location 1: go there and stay [documented: each type's page] |
| Patrol located near a reference, or near the actor's linked reference | walk that reference and the chain of references linked from it [documented; observed: Victor] |
| Patrol with another location | go to that location [hypothesis] |
| Dialogue | wait at location 1, go up to the target, talk (§5.6); a conversation between two actors is not carried out |
| Follow, Accompany | keep within the target's count / distance of the target (256 when 0), wherever it goes, through doors too; stand while it stands (since 2026-10-11, below) [documented: "Follow/Escort/Accompany: Idles play while not moving but waiting for target to move"; hypothesis: the distance] |
| Use Weapon | its location 1: go there, turn to target 2 and fire the PKW3 pattern (docs/design/use-weapon.md); the goal is met when the shots are spent |
| Escort, Find, Use Item At, Ambush, Flee Not Combat | since 2026-10-17 (§5.10, `fnv_ai::errand`): lead the target to location 1, waiting within PKE2; look for the target's objects around location 1 and take, use or go up to them; go to an object and use it; wait at location 1 until an enemy is in location 2, then fight; run from a threat, to location 1 or away |

| Location | Meaning |
|---|---|
| near reference | within the radius of where the reference is now |
| near editor location | within the radius of where the actor was placed |
| near linked reference | within the radius of the actor's linked reference |
| near current location, or none | where the actor is: nothing to walk |
| in cell | anywhere inside the cell (the radius is not used [documented]); the walk aims at the middle of the cell's largest navmesh area [our choice] and ends on entering the cell. An exterior cell: within 2,048 units of that point [our choice] |
| by object, "at package location" | not carried out |

A place counts as reached within the package's radius, and never needs more precision
than 32 units [our choice]. Travel "stops at the edge of the destination radius"
[documented]: the walk is planned to the centre and ends when the radius is entered.

**Follow and Accompany** (`goal::Follow`, `Ai::follow_step`; owner playtest
2026-10-11: Sunny Smiles's dog Cheyenne, whose CheyenneAccompany package targets
SunnyRef at 128 units, stayed behind when Sunny went to the wells and was missing from
the saloon afterwards, because the package type was not carried out). The actor stands
while its target is within the distance plus 64 units [our choice]; farther, it sets off
after the target (through the doors between, if the target is in another cell), walking,
or running when the package says Always Run or it is more than 384 units beyond its
distance (512 from the target at the usual 128) [our choice]; the way is planned again
when the target has moved 64 units, at most twice a second. On its way after a target
that moves it keeps step (`carry::follow_speed`): the target's own pace since the last
plan plus the gap beyond the distance closed in 3 seconds, between half its walk and its
run, so the walk clip plays faster or slower and the run clip comes only when it is well
behind; it stops once within the distance when the target has stopped (no new plan for a
second, or slower than 20 units/s), else only 64 units inside it [our choice; owner's
playtest 2026-10-24: the Novac pack brahmin, kept about 450 units back by its package, stood,
set off running past a fixed 512, caught up and stood again all the way after its
merchant]. In the original game the same brahmin walked at its merchant's pace 440–530
units behind it, ran only once it was about 800–1,060 behind, and closed to about 300
when the merchant stopped [observed: AI observer 2026-10-24, docs/spec-notes/actors.md
§8.10, Brahmin]: the rule above agrees except for that last stop, which is at the
distance here. A target that is gone or dead
is not followed. The goal counts as met at once (`package_goal_met`), so the periodic
choice may replace the package as it may any other. The target being attacked and the
Wait/StopWaiting functions (used by no script of the masters) are not built. The
player's companions (teammates) are docs/design/followers.md: brought along on a change
of cells, fighting the player's foes, waiting by their Guard packages. `fnvtool ai
reach`: 256 of the 265 follow-type actor-package pairs of FalloutNV.esm are now
"follows: goes after its target from where it is"; 5 name no reference (not carried
out) and 4 are Escort; every other verdict is unchanged. Log: `ai [h] <actor>: walks
after <target> (<package>): <n> units away, …` and `: has caught up with <target>`.

*Followers in a line* (`Ai::ahead_in_column`; owner playtest 2026-10-24: three
mercenaries following one leader should walk behind it in single file, not bunch up on
its heels). Several actors whose packages follow the same target form a column, all of
it our choice [the original's behaviour is not documented or observed]: the first
follows the target as above, each after it walks after the one ahead, keeping 128 units
(`COLUMN_GAP`, or the package's distance if that is smaller) plus the same 64 of slack.
The order is kept once made; a newcomer joins at the end, the nearer the target the
further forward, so followers do not overtake each other. Members are present and
alive; one ahead that is in neither the target's cell nor the follower's own (left
behind) is passed over, so a follower still goes through a door after the one ahead of
it. A follower runs when it is more than 384 units beyond its distance from the one ahead,
or the target is that much beyond where its place in the line would be (192 units per
place), so a long
line does not run while its leader walks. The player's companions on Follow packages
line up behind the player the same way. Combat does not take a fighter out of the line
(a follower may walk after one ahead that is fighting). Log: the `walks after` line
names the one ahead.

*Catching up* (`carry::catch_up`; owner playtest 2026-10-24: followers should get a
speed boost to close the distance, faster than their top speed). A follower more than
384 units beyond its distance (the gap that makes it run, as above; in another cell,
the length of its way) goes faster than its own pace, by up to half again (`CATCH_UP_BOOST`), rising
evenly over the next 1024 units (`CATCH_UP_SPAN`) [our choice]. The boost is worked out
again at every update while it walks, so it eases off as the gap closes; nearer, in
step with a moving target, the pace is the target's (Follow, above). The engine
plays the run clip faster to match (up to its 2.5 rate limit), so the feet do not slide.

### 5.2 Getting there

- **The path** is `fnv_nav`'s: across any number of interiors and worldspaces, doors
  included (navmesh.md). Everything the actor carries counts as a key for locked doors.
  Always Run packages run (344 units a second), others walk (86.8) [observed: the
  player's speeds; NPC speeds are not captured].
- **No way there** (no navmesh at either end, no chain of doors, a locked door without
  the key, or a location that names nothing): the package is set aside for one game
  hour (`Game::block_package`, `PackageCheck::Unreachable`) and the actor chooses again:
  the next valid package of its list [hypothesis: the game's behaviour is not
  documented].
- **Arrival** is reported to the game (`package_goal_met`), which lets a Must Complete
  or Must Reach Location package be replaced and runs the end script. It is reported for
  every type that goes somewhere, not only Travel [hypothesis]. A patrol reports it at
  its last point; a patrol that loops, or is repeatable, never does: a looping chain
  goes round, a repeatable chain that ends starts over at its first point [hypothesis:
  the documentation says "loop or go back and forth"]. A repeatable patrol starts at the
  nearest point, another at the first [documented].
- **Movement** is `fnv_nav::Walker` (on the navmesh surface, no physics), and the
  actor's cell and position in the game are kept up to date as it walks, so scripts and
  conditions (GetInCell, GetDistance) see it.
- **Put somewhere by a script** (MoveTo, MoveToMarkerWithFade; `Game::teleported`,
  `Ai::moved_by_scripts`, since 2026-10-11): the walker starts again where the game has
  the actor now, facing the marker's way, and takes up its package afresh (its walk,
  furniture, sandbox, search or random conversation are given up) [design]. In the
  engine the resident gets off its furniture at once and loses its character; with
  `--play` an actor moved into a cell from one that was never loaded becomes a resident
  there (`residents.rs` `take_moved`). Log: `ai [h] <actor>: put at (x, y, z) in <cell>
  by a script (MoveTo): goes on from there` (or `: a resident from there`). Before, the
  walker kept the old place, so the actor stayed drawn where it had stood and went on
  from there: Sunny Smiles stayed in the Prospector Saloon after Ringo's "All right,
  I'm ready." (INFO 0x00105CC2, whose end script moves her to SunnySpawnMarker in the
  gas station and gives her SunnyTriggerGunfightDialoguePackage) [owner playtest
  2026-10-11].

### 5.3 What the checks show

- **Unit tests** on a hand-built plugin and navmeshes (`crates/fnv-ai/tests/carry.rs`):
  through a door to another cell, a locked door and its key, "in cell", the radius edge,
  running, a patrol once and a patrol round a loop, packages not carried out.
- **`fnvtool ai run`**: Easy Pete's day in the authored game: from his chair to his
  house and bed through its door (4,253 units, 49 s), back to the chair at 08:00, into
  the saloon to eat at 12:00, the chair at 14:00, home at 22:00.
- **`fnvtool ai run --samples`** starts each actor where a capture first saw it and
  compares at every sample. Three captures, the longest 24 game hours:

  | Actor | Samples | Same package | In the same cell, of those |
  |---|---|---|---|
  | GSChetRef, RingoRef, CheyenneREF | 2,904 each | 98.8–100% | 100% |
  | GoodspringsSettler01Ref | 2,904 | 99.7% | 100% (608 units apart at the median) |
  | GoodspringsSettler04Ref | 2,904 | 98.7% | 97% |
  | EasyPeteRef | 2,904 | 98.9% | 91% (28 units apart at the median) |
  | GSVictorRef, given his patrol with `--force` | 2,785 | 13% | 36% |

  The package differences are the seconds a switch takes, run-time packages, and for
  Victor the quest state the run does not have. Victor's patrol is the weak point: ours
  walks the eleven points without stopping (§7, note 6).
- **`fnvtool ai reach`** asks, for every placed actor and every package of its list,
  whether the place can be reached from where the actor stands in the editor (15,603
  pairs, 17 seconds): 9,488 reachable (8,246 without a door, 762 through one, 257 two,
  144 three, 79 four or more, up to eight) and 18 with a key the actor carries; 1,571
  stay where they are; 512 cannot be reached (§7, notes 1 to 4); 2,810 are never chosen
  (a linked reference the actor lacks); 177 are located at the player; 126 are
  Dialogue packages without a wait location (the actor goes up to its target from
  where it is); 901 are of types not carried out yet. (Before Dialogue packages were
  carried out, 2026-10-10: 9,417 reachable, 17 with a key, 504 not reachable, 1,107
  not carried out. The whole difference is the 209 Dialogue pairs: the wait location
  of 71 is reachable, of 1 with a key, of 8 not; 126 have none; 3 are conversations
  between two actors.)

### 5.4 In the engine (step 4b)

`nvforever --npc-packages ACTOR[,ACTOR…]` (`engine/nvforever/src/residents.rs`) hands
placed actors to `fnv_ai`. Each chooses its package by the hour, walks to where the
package sends it through doors into other spaces, and stays until its package changes.

- **Drawing** is the travellers' (`nav::show_actor`, shared with `--npc-travel`): the
  actor is spawned while its space is the one drawn, placed at the walker's feet, and
  its locomotion blends walk, turn and idle from the speed. The cell streamer leaves
  these actors to the module.
- **Doors that open in place** are opened and waited for, as travellers do
  (`Ai::update_with`).
- **Hours passed in the Wait menu** (`--play`; docs/spec-notes/wait-menu.md §7): for
  each, every actor chooses its package (`Game::choose_packages_now`) and walks for an
  hour of game time on the navmesh (`Ai::pass_time`: no talking, no new sandbox
  behaviour, no patrol leaving its point); an actor on furniture that is to be
  elsewhere is off it at once (`furniture::LeaveNow`). [engineering choice]
- **The clock**: the package choice runs in a game state of its own, built at startup
  (about 6 seconds). Its clock starts at the sky's hour (`--hour`) and runs at the sky's
  `--time-scale`, or at `--npc-timescale` (default 30, the game's TimeScale) when the
  sky stands still. It does not stop indoors, as the sky's does.
- **`--follow-npc`** follows the first traveller, else the first resident.
- **The log**: `ai [12:03] EasyPeteRef: package EasyPeteEat12x2`, `… walks to (…) in
  GSProspectorSaloonInterior: 1508 units, 1 doors`, `… used door …`, `… arrived after
  16.2s, 1412 units`, and at exit `ai summary at 12:26: 1 residents, 2 packages chosen,
  2 trips, 2 arrivals, 0 unreachable; …`.

Smoke tests (windowed, 2026-10-10, all exit 0 with "run finished"):
- `--cell Goodsprings --npc-packages EasyPeteRef --hour 11.8 --follow-npc
  --run-seconds 80`: he takes his chair, at 12:03 gets up for EasyPeteEat12x2, walks
  1,508 units and through the saloon door (the camera follows him in), arrives at 12:12.
  A screenshot mid-walk shows him drawn and striding.
- `--npc-packages EasyPeteRef,GSChetRef,GoodspringsSettler04Ref --hour 21.9 --follow-npc
  --run-seconds 150`: at 22:02 to 22:04, one after another, the settler leaves the saloon
  for its house (7,460 units, two doors, 78 s), Easy Pete walks home (49 s), Chet stays
  in his store: 6 packages chosen, 4 trips, 4 arrivals, 0 unreachable.
- With `--play` the same resident run works beside the player and scripts.
- `--npc-travel` (whose drawing code was moved into the shared helper) still gives
  "nav summary: 2 of 2 travellers arrived".

Step 4c, first part (2026-10-10):
- **`--npc-packages all`** hands over every actor and creature of the cells loaded
  around the camera, as cells stream in (the cell streamer then spawns none,
  `Streamer::manage_actors`); each is drawn while the cell it stands in is loaded and
  the game has it enabled, at its reference's scale, at most three new ones a frame.
  Flying across the map (`--npc-packages all --auto-fly 4000 --run-seconds 40`): 154
  residents, 117 packages chosen, 16 trips (patrols walking their points), 0 unreachable.
- **With `--play` the package choice is the game's own state** (`nvf_play::script`):
  quests, script packages and deaths count, package scripts run in the game, and actors
  stand still while a menu or a conversation holds the game. The game's calendar is the
  clock (the simulation advances it at TimeScale); it is set to the sky's hour once.
  `NVF_PLAY_TEST_SCRIPT_MENUS=1 … --play --npc-packages all,EasyPeteRef --hour 11.9
  --run-seconds 100`: Easy Pete's noon walk into the saloon, 25 residents, exit 0.
- **`--play` alone is `--play --npc-packages all`** (2026-10-10, owner report: "none of
  the npc actors were doing anything" in `--play`; the packages were only on with the
  flag). `--no-npc-packages` gives the placed, standing actors back, and the actor test
  aids (`--actor-pose bind`, `--actor-pose-time`, `--actor-walk-test`,
  `--actor-turn-test`, `--actor-weapon-test`), `--no-actors`, `--npc-travel` and
  `--npc-sit` keep them too. A message box a script opens holds the game, and with it
  the actors, until the player answers it: at the start that is "Classic Pack items
  added to inventory." on an install with the pre-order packs (in a 100-second run
  that left it open the clock went from 11:53 to 11:55). Smoke test:
  `NVF_PLAY_TEST_SCRIPT_MENUS=1 … --cell Goodsprings --play --hour 11.9 --run-seconds
  100`: 25 residents, 14 packages chosen, Easy Pete's noon walk into the saloon, exit 0;
  `--play --new-game`: Doc Mitchell on his chair, 1 resident, exit 0.
- **The character controller near the player** (`--play`; `residents::PhysMover`, the
  `fnv_ai::Mover` hook, `nvf_play::phys::PhysState::npcs`): an actor walking where the
  collision world is loaded gets a character of its own; it steers along its path and
  the physics step moves it with the player's, so it goes round furniture and follows
  the floor. It walks slopes up to 60 degrees (navmeshes cross slopes a little above the
  player's 45) and passes through actor hit volumes (its own among them). Stuck (the
  `Unstuck` rule of the walk tests: detours, then giving up), fallen far off the path,
  through a door into another space, or within 64 units of its destination (chairs,
  beds and markers cannot be stood in), it is the walker's again: on the navmesh
  surface, as everywhere else. Smoke test: Easy Pete's noon walk, 243 updates moved by
  physics, gave up 0 times, arrived after 18.0 s (16.2 s on the navmesh surface).
- **The sky follows the game's clock in `--play`** (`sky_follows_game`): GameHour starts
  at `--hour`, runs at the TimeScale global, and the sky shows it (17:30 became 17:55
  in a 50-second run); an hour set with the `[` `]` keys becomes the game's.
  `--still-sky` keeps the old behaviour; `--time-scale` keeps the sky's own clock.
  With the residents running, GameDaysPassed starts at `--day` (the plugins' value
  without it), in whole days, in a new game; a loaded save keeps the hour and day it has
  (`residents::Core::prepare`) [design].

**Furniture** (2026-10-10, with `furniture.rs` from main; `residents::choose_seat`): an
actor that has arrived uses furniture when its package gives it some:
- the package's location, when that reference is furniture [observed: under the Travel
  packages EasyPeteChairPackage8x4 / 14x4 and GoodspringsSettlerSaloonPackage the game
  reported GetSitting 3 in 7,395 samples; under EasyPeteHideInSaloonPackage in all 968];
- a Sleep package: the nearest bed of the cell within the location's radius, anywhere
  in the cell for "in cell" [documented: Sleep Package; not seen in a capture: the
  sleepers there were in unloaded cells and reported GetSleeping 0];
- an Eat package: the nearest seat within 256 units [documented: "sitting down if
  furniture is available"; the distance is our choice].
It is told to sit once it is drawn (`SitOn`); when its package changes it is told to get
up (`GetUp`) and its walk waits until it is off. Furniture one resident was given is not
given to another. Told to sit and refused, it does not stay told [design, #1506;
furniture.md §7]: for occupied furniture it stands free and asks again every 5 s of active
play (an engineering number, no claim about the original) while it still wants it; for
any other reason it does not ask again until something has changed. This covers furniture
the renderer finds occupied; two residents contending in the AI's reservation are not
covered (the loser gets no order, and no new choice while its activity lasts). Smoke tests: Easy Pete sits in his porch chair, gets up at 12:03,
walks into the saloon and sits at his booth; at 22:04 he gets up, walks home and lies
down on his bed (`SpecialIdle_BedLeft_Enter`, `DynamicIdle_Sleep`); flying across the
map with `all`, other actors sit down where their packages send them.
Sandboxing actors choose seats, beds and idle markers themselves (§5.5). A creature the
IDLE tree gives no clip of its own for a piece does not get it, by a package or by its own
choice; a person is never ruled out for that. A sandbox and a Sleep or Eat package's search
leave crosses (marker ID 10) out for anyone; a package that names or links to one, a patrol
point, a Find and a Use Item At do not. All this before anything is chosen or kept, and the
engine refuses a getting-on or loop clip that does not fit before binding it, and plays no
exit clip that does not [design, interim: furniture.md §9, issue #1522; a few minutes'
watch of the original found no creature on furniture, observer-checks.md item 86].
Not done:
an actor that arrives while not drawn sits down only when it is next drawn (it walks
the last steps then, instead of being found seated); `fnvtool ai run` (headless) has no
furniture.

Limits that remain:
- Without `--play` there is no collision world: actors keep to the navmesh surface, and
  the package choice runs in a game state of its own (no quests).
- Two actors that both walk do not push each other; meeting in a doorway they rely on
  the stuck rule. An actor that stands still is pushed aside (§5.7).
- The chase camera can end up behind furniture indoors.

### 5.5 Sandboxing (2026-10-10, owner request)

`fnv_ai::sandbox` (headless; `crates/fnv-ai/src/sandbox.rs`), carried out by `carry.rs`
and shown by the engine's residents. A Sandbox or Wander package that has reached its
location starts a sandbox there; the actor then does one thing after another.

**The flags that bear on it** (`fnvtool ai packages`, `sandbox`; 791 Sandbox and 32
Wander packages in the masters):

| Flag | In the record | Sandbox packages with the behaviour on | What we do |
|---|---|---|---|
| Eating | type-specific bit 0 "No Eating" | 465 of 791 | meals at the rolled times, on a seat if one is free; no food is picked up or eaten |
| Sleeping | bit 1 "No Sleeping" | 305 | to a bed of the area for the rolled night; no bed, no sleep [hypothesis] |
| Conversation | bit 2 "No Conversation" | 654 | the actor takes part in no random conversation (`fnv_ai::social`, docs/spec-notes/dialogue.md §5.2) |
| Idle markers | bit 3 "No Idle Markers" | 706 | goes to a marker and faces its way, and plays the marker's idles there (docs/spec-notes/idles.md §5) |
| Furniture (other than beds) | bit 4 "No Furniture" | 668 | sits on a seat of the area |
| Wandering | bit 5 "No Wandering" | 489 | walks to a spot of the navmesh in the area |
| (all six off) | | 15 | stands |
| Fallout behaviour: Random Conversations | behaviour bit 1 | 34 | counts only with general bit 12 (Skip Fallout Behavior); without it the default allows them. Random conversations and hellos to the player: `fnv_ai::social`, docs/spec-notes/dialogue.md §5.2 |
| Fallout behaviour: Allow Idle Chatter | behaviour bit 7 | 118 | not carried out |
| No Idle Anims | general bit 24 | 7 | the package's idle collection is not played |
| Package idles | IDLA on the package | 8 | played while the actor stands with nothing else to do, each idle only if its own conditions pass (idles.md §5) |
| Must Complete | general bit 2 | 0 | "invalid" for a sandbox [documented]; none has it |
| Always Run / Always Sneak / Weapon Drawn | general bits | 31 / 6 / 36 | sandbox walks are walked; the three are not acted on |
| Location radius | PLDT | median 512, 71 "in cell" | objects and wander spots come from within it; the whole cell for "in cell"; 512 when it is 0 |

The six behaviour flags are the record's type-specific bits, read only for Sandbox and
Wander packages (`PackageDef::sandbox_flag`; the same bits on other types mean nothing,
packages.md §2). Wander packages are treated as sandboxes with their own flags
[hypothesis: xEdit gives both types the same flag names].

**Choosing** [documented shape, GECK wiki Sandbox Package; numbers ours]:
1. In the night's sleep window, with sleeping on and a bed in the area: sleep until the
   window ends. The window is rolled when the package starts and again each day: start
   between `fSandboxSleepStartMin` and `Max` (19:00 to 01:00), length between
   `fSandboxSleepDurationMin` and `Max` (6 to 10 hours) [documented defaults; the
   masters do not set them].
2. In a meal window, with eating on: a meal until the window ends (breakfast starting
   6 to 9, lunch 11 to 14, dinner 17 to 20, lasting 0.5 to 2 hours, rolled likewise).
3. Otherwise one of: a seat, an idle marker, a wander. Each kind has a weight that drops
   to zero when the kind was just chosen and grows back by half each time another is;
   the object just used is not chosen again at once [documented]. Low energy (AIDT
   energy level) leans towards sitting, high towards idle markers and wandering
   [documented idea]. Within a kind nearer objects are likelier [ours]. A seat lasts about
   45 to 165 seconds by energy (give or take a quarter), a marker its idle timer times its idles (15 to 90 s), a
   wander 4 to 12 seconds at the spot [ours].
4. Nothing open: stand, look again in 10 seconds.

Sandboxing goes on only where the game is loaded (the cells around the player; anywhere
in a headless run without a player) [observed: the sandboxers of unloaded cells in the
owner's captures stood on one spot for hours]; package changes and the walks they bring
still happen elsewhere.

Objects are references of the area's cell, present, at about the actor's height, not
given to another actor (a seat is kept for an actor from the moment it chooses it), and
not waiting out a failed way to them. Random numbers are the sandbox's own (seeded by
actor and package), so runs repeat and the script runtime's are not disturbed.

**An object with no way to it** [design; issue #1504; the numbers are ours, neither the
documentation nor the original gives any]: the actor stands 5 seconds and chooses again,
and that object stays out of the choices for 60 seconds of `Game::time` from the failure,
then 120, 240, 480 and from then on 600 for each further failure in a row. The wait ending
does nothing by itself (no search, no random number, no event): the object is simply among
the candidates at the sandbox's next choice, so a way that has come back is found, and one
that has not costs one search per wait (at most nine in the first 3,600 seconds, for one
object's consecutive failures in one retained sandbox; competing behaviours mean fewer). Its record goes only when the behaviour
with that object begins; a walk to the object begun is not enough, and combat interrupting
the sandbox leaves the record alone. The record is not saved: a load discards deadlines and
stages and restarts the backoff, as a new sandbox does. An object at the cap with the full
600 seconds left to wait gets up to four more searches in that time, for each failed object at each load (in the
retry tests' timing, about 0.5, 65.5, 190.5 and 435.5 seconds after it; other behaviours can
move those); a shorter wait left may mean fewer. No test loads a game here; this is from the
delays.

**Also in this step**: a patrol waits out a point's idle time (REFR `XPRD`) before going
on [documented: Idle Markers, Patrol Data]; the furniture a package gives its actor
(§5.4) is decided headless now (`goal::package_furniture`, `Agent::using`), so
`fnvtool ai run` reports it too.

**Checks**: 17 tests in `crates/fnv-ai/tests/carry.rs` (only-wander stays inside its
radius and uses nothing; no-wander alternates chair and idle marker; all off stands; the
night's bed and the meal's chair at the rolled hours; an idle marker's heading (turned to
on the spot at 120°/s since 2026-10-11, `Agent::face_to`, actors.md §8.9); two
sandboxers never on one chair; the same run twice; the patrol wait). `fnvtool ai run
--actor GSChetRef --start-hour 9`: a meal on a chair, an idle marker, a wander, a seat,
in his store. In `nvforever`: Chet walks about his store and sits down (screenshots);
flying over the map with `all`, 161 wanders, 37 meals, 6 idle markers at 26 frames a
second as before.

**Not done**: conversations; food; the idle manager's idles (package and marker idles
are played, docs/spec-notes/idles.md); sandboxing "at each
point of a linked patrol route" [documented]; energy changing over the day; the game's
real scores and durations (to be captured: an observer run in a loaded sandbox area
recording GetSitting, GetCurrentAIProcedure and positions).

### 5.6 Dialogue packages: the force greet (2026-10-10, owner request)

Owner request 2026-10-10: Sunny Smiles is supposed to greet the player who enters the
Prospector Saloon, and did not. The cause: nothing carried Dialogue packages out
(`Goal::NotCarriedOut("a dialogue package")`). What the data holds and what the game was
seen doing: packages.md §2 to §4. No trigger script is involved: the package itself
names a box around her spot as its trigger location.

`fnv_ai::goal::talk` turns the package into a `Talk` (whom, the activate distance, the
wait location, the trigger location, the topic, Say To or conversation) and
`Ai::talk_step` (`carry.rs`) carries it out, one `Talking` phase after another:

| Phase | What the actor does | Source |
|---|---|---|
| `ToWait` | walks to location 1, if the package has one and the actor is not there | [documented] |
| `Waiting` | stands, or sits when the wait location is furniture (`settle`, as at the end of a Travel package; since 2026-11-03). Seated it never sets off: it talks from its seat once the target is within the activate distance, and only its head turns while it speaks. Standing: without a trigger location it sets off at once (case 3). With one, it sets off when the target is in it (case 1). With one and a wait radius of 0 it talks as soon as the target is within the activate distance (case 2), and also sets off when the target is in the trigger location | [documented: cases 1 to 3; **hypothesis**: setting off in case 2]; the seated wait: [observed: Benny on his bar stool in the Tops' suite, owner's frame 2026-11-03, packages.md §3; **hypothesis** for other seated waiters] |
| `Approaching` | walks (runs with Always Run) to where the target is, through doors if need be; the way is planned again when the target has moved 64 units, at most twice a second | [documented: "begin moving towards the target"; the numbers are ours] |
| (start) | within the activate distance it stops: a conversation with the player starts (`Game::force_greet`, the package's topic or GREETING), or the Say To line is said (`Game::say`). A GREETING line with the Goodbye flag opens a conversation too: the force greet holds the player until the line is over (since 2026-10-11; before, it was said without one) | [documented]; the Goodbye greeting: [owner statement 2026-10-11, docs/spec-notes/dialogue.md §5] |
| `Speaking` | for as long as the conversation lasts, or the greeting said without one: turns to the target at the standing turn rate (120° a second), unless "No Headtracking" | [documented: they are "forced to look at each other"; observed: Sunny turned from 90° to 178° towards the player; the rate is ours; **hypothesis**: a greeting said without a conversation holds the package for the length of the line] |
| `Done` | the conversation is over, or the greeting is said: the package's goal is met (`package_goal_met`), so a Must Complete package can be replaced and the end script runs. The actor does not start over while it keeps the package | [documented: Must Complete; ours: no starting over] |

- **The trigger location** (`goal::Trigger::holds`): within the radius of the
  reference, the editor location or the actor; in the cell; and **inside the
  reference's own box or sphere** when it carries one (REFR XPRM, read into
  `PlacedRef::primitive`). The last is a [hypothesis] the data asks for: 32 of the 33
  trigger references with radius 0 are boxes placed for it (packages.md §2), and a
  radius of 0 around a point could never be entered. The target's feet, middle and head
  (64 units apart) are tested [our choice].
- **Why the actor sets off in case 2** [hypothesis]: the documentation says it stays
  until the target is within the activate distance. But the player comes into the saloon
  inside Sunny's box and 343 units from her, with an activate distance of 256, and the
  greeting is expected as the player comes in (owner, 2026-10-10). Both cannot hold
  unless she closes the distance. Staying is still what happens while the player is
  outside the box (as in the owner's `saloon_evening` capture: six game hours in the
  same cell, 773 units away, and she never moved). To be captured (packages.md §6).
- **The activate distance** is the target's count / distance; a stored 0 is taken as
  the standard 150 [documented value; hypothesis that 0 means it].
- **No way to the target**: the package is set aside for an hour like any other (§5.2).
- **Nothing to say** (no line of the topic passes its conditions): the package is done
  [our choice; otherwise the actor would try every frame].
- **A held game** (a menu, another conversation): the start waits; nobody walks.
- **A greeting with the Goodbye flag** (3,023 of GREETING's 5,579 lines,
  docs/spec-notes/dialogue.md §5). **Since 2026-10-11 a Dialogue package's force greet
  (`Game::force_greet`) opens a conversation with it like with any other line** [owner
  statement 2026-10-11, Sunny Smiles's gecko line]; what follows describes the way it
  was said before, which is now only the player's own activation's:
  `Game::start_dialogue` says it at the player as a
  `Speech` and leaves `Game::dialogue` empty, so the game is not held and `Ai::update`
  goes on. The actor is `Speaking` while `Game::speech` holds its greeting and `Done`
  when the line is over; `AiEvent::Talk` carries `greeting: true`. Until 2026-10-10
  (evening) `talk_step` took the empty `Game::dialogue` for a conversation that was
  over: the package was done on the next update, with the line still being said, and
  the actor did not turn to the player. What the game does with the package while such
  a line is said is not captured [hypothesis, above].

Not done:
- A **conversation between two actors** (39 of FalloutNV.esm's 332 Dialogue packages
  target someone else; 35 are conversations): not carried out, the actor stays.
- **The target is not controlled**: the player is not held while the actor comes
  ("Do Not Control Target Movement" off), and nobody is sent to the trigger location.
- **Wandering** within the wait location's radius; the dialogue menu's **FOV**.
- A package that its own conversation replaces (a result script calls
  EvaluatePackage, as Sunny's does) runs its change script, not its end script: the
  actor has another package before the AI sees the conversation is over.
- The game's run-time DIALOGUE package (0xFF…, type 6) has no counterpart: the actor
  keeps its list package (§7, note 20).

**In the engine**: an actor handed to the AI (`--npc-packages`) starts the conversation
in the game's own state with `--play`, so the dialogue screen opens as for
`StartConversation` (docs/design/dialogue-play.md §1). `Ai::update_held` keeps the
speaker turning while the conversation holds the game. Log: `ai [12:01] SunnyRef: walks
up to 0x00000014 to talk (SunnyMeetPlayerDialoguePackage): 343 units away, 343 units to
go, 0 doors`, `… starts a conversation with 0x00000014 from 255 units (…), line
0x00104E7E`, and the summary's `1 talks started by dialogue packages`. A greeting
said without a conversation logs `… greets 0x00000014 from 212 units without a
conversation (…), line …` instead, and both end with `… has talked (…)`. **Without
`--npc-packages` no actor runs packages, so nobody greets.**

**Checks** (2026-10-10):
- 10 tests in `crates/fnv-ai/tests/talk.rs` on a plugin built in memory: the box read
  from the record; an actor walking up to a player who steps into its box and greeting
  at the activate distance, once; the greeting on the spot within the distance (and
  the turn towards the player); a greeting with the Goodbye flag said without a
  conversation, the package lasting as long as the line; coming at once without a
  trigger location and following a player who moves; staying at a wait location with a
  radius until the trigger location is entered; a Say To line; two actors; nothing to
  say; a player who is nowhere.
- Headless against the captures (`fnvtool ai run --samples`): SunnyRef ran the same
  package as the game in all 2,904 samples of `town_day` and stood 0.0 units from where
  the game had her. The other actors' figures are those of §5.3; `fnvtool ai observed`
  gives the replay figures of §4 unchanged; `fnvtool ai packages` `all_agree` true.
- **In `nvforever`** (windowed, unattended, `--no-voice`, each exit 0 with "run
  finished"): `NVF_PLAY_TEST_SCRIPT_MENUS=1 NVF_PLAY_TEST_SCRIPT=14:activate:ProspectorSaloonDoor
  … --cell Goodsprings --play --npc-packages SunnyRef --run-seconds 45`: the player
  goes in by the door; Sunny sets off 343 units away and 1.06 s later, from 255 units,
  says "Cheyenne, stay. Don't worry, she won't bite unless I tell her to."; the choices
  are offered; the unattended run takes the first ("Doc Mitchell said you could teach
  me…"), which completes the objective "Talk to Sunny Smiles in the Prospector Saloon"
  and gives her VCG02SunnyTravelOutside: she walks out of the saloon. A screenshot
  during the greeting shows her standing in front of the player, facing them, Cheyenne
  beside her. With `--npc-packages all` the same (34 residents); with the player moved
  across the room while she came (`NVF_PLAY_TEST_GOTO`), she turned after them and
  greeted from 253 units.

**Sight and loading screens** (2026-10-23, owner's playtest list 7 items 24, 41/42):
- A force greet (and, since 2026-11-08, a Say To line to the player, below) needs a clear line from the actor to the player:
  nothing solid, a wall or a closed door, between them at the engine's last detection
  measurement of the pair (`Detection::clear`, the line-of-sight ray of
  `nvforever` detection.rs, whichever way the actor faces) [owner statement: Davison in
  the REPCON basement greeted the player through his closed door (Dialogue package
  REPCONDavisonDialogPackage, activate distance 600, trigger location with a wait
  location of radius 0: case 2 above), Harland through a wall (REPCONHarlandDialogPackage,
  activate distance 1200, the same case); "the door must block it"; hypothesis: the
  original asks for the line and not the field of view]. A pair not measured holds the
  greet back while the engine measures the player at all; without measuring (headless,
  `tdetect`, `NVF_PLAY_TEST_UNSEEN`) sight holds nobody back (`Game::greet_sight`). The
  actor waits where it is (at the end of its walk, or at its wait location), logged once:
  `… waits to talk to 0x00000014 (…): 586 units away, with something solid between`.
  The activate distances stay the records' (Davison 600, Harland 1200): the line is what
  was missing, not a shorter distance [decision].
- **A Say To line to the player** needs the same clear line (2026-11-08) [hypothesis, from
  an owner report: in Tale of Two Wastelands the escape from Vault 101 softlocked in
  Amata's wake-up, the player's wake-up idle playing over and over]. Officer Kendall's
  `CG04Security02Ambush` (Fallout3.esm; Dialogue, Say To, topic `CG04SecurityBarks`,
  activate distance 1500, wait location radius 0, no trigger location: case 3 above,
  condition `GetStage CG04 < 12`) said "There he is! Hold it right there!" from 855 units,
  through the walls, while the player was still in bed at stage 2 [observed in ours,
  TTW 3.4]. The line's end script (INFO 0x0602D4B2 / 0x0602D4B3) sets CG04 stage 12, and
  the quest script moves on to stage 3 only `if GetStage CG04 == 2` (SCPT 0x06029341, 6.3 s
  after stage 2) [observed: TTW load order, `fnvtool script decompile`]. Stage 3 removes the
  player's script package `CG04PlayerSection0`, so without it the package stayed, and its
  collection (Run In Sequence, timer 0) replayed the one-shot `LooseCG04PlayerWaking`
  (6.33 s, loop 0 / 0) for ever, the view held by its camera node. In the original the
  bark comes when the player steps into the hall. Say To lines to an NPC are not held back:
  only pairs with the player are measured. The 27 Say To packages aimed at the player in
  the ten New Vegas masters (follower barks such as `RaulHooverBark`, the Honest Hearts
  Chalk barks, `VVault21SarahStartTour`) wait the same way until the speaker can see the
  player [observed: `fnvtool ai show`]. Logged as the force greet's "waits to talk to …".
- Nobody acts under a loading screen: while `nvf_play::loading` holds the screen up the
  script runner sets `Game::loading`, which holds the residents (packages, force greets,
  hellos, combat), as menus and conversations do [owner: Johnson Nash greeted the player
  entering the Vikki and Vance casino with the screen still up; his package
  PrimmNashDialogueGreeting has activate distance 1024 and a trigger box by the door].
  Smoke: `force-greet-door`, `force-greet-after-loading`.

**Restrained actors and furniture** (2026-10-23, list 7 items 30, 34): a restrained actor
(SetRestrained 1) still picks its package but does not move [documented: GECK wiki
SetRestrained]: a new package whose place is within `RESTRAINED_REACH` (256 units, our
choice) counts as reached, and the furniture it gives is used (Deputy Beagle on
FFCaptiveMarker in the Bison Steve hotel); farther, it stays. Released, its package
begins again. A package placed at "Near linked reference" whose linked reference is
furniture gives that furniture, as a specific reference does (the crucified Powder
Gangers' crosses in Nipton). A patrol point that is furniture is used for the point's
idle time (Gomorrah's courtesans on DanceMarker03, 800 s) [hypothesis]. Smoke:
`restrained-furniture`, `crucified-stay`, `patrol-point-furniture`.

**Talking to a sleeper** (list 7 item 38): the player activating an actor asleep in its
bed (GetSleeping 3) wakes it (`Game::wake`): it gets off the bed and the conversation
starts once it is off (at most 8 s later); it stays up for 30 s of game time (the
conversation's own time not counted), then its package takes it back to bed [our
choice: both times]. Smoke: `wake-to-talk`.

### 5.7 Pushed aside (2026-10-10, owner request)

A standing actor gives way to one that walks into it (the rule, who pushes and how far:
docs/spec-notes/physics.md §6.8; the figures are placeholders until the capture
`pc_actor_push`). What the package layer adds:

- **Who stands** (`Ai::stands`): in the world, alive, not `Travelling`, with no
  furniture in use or waiting for it, not waiting at a door, not speaking in a
  conversation it started. The residents add: drawn, not getting off furniture, and
  not out of room a moment ago.
- **The push** (`Ai::push`): the walker's place moves over the navmesh (physics.md
  §6.8, "Room") and the game's reference with it (`Game::set_actor_place`), so scripts
  and the package choice see the actor where it now stands.
- **Afterwards** [hypothesis: not captured]: once nothing has pushed the actor for 2 s
  (`PUSHED_BACK_SECONDS`), an actor that is `There` under a package with a place
  (near a reference or a point, or inside a cell) and now outside it (the location's
  radius, at least 32 units) walks back (`AiEvent::PushedBack`, then a trip like any
  other; on arrival the package settles again). A sandboxing actor goes on from where
  it stands, a patrol from its next point, and a new package forgets the push.
- **Checked**: `fnv-ai` test `a_standing_actor_is_pushed_over_the_navmesh_and_goes_back`
  (not while on its way; all of a small push; to the navmesh's edge and along it, no
  further; back into its package's radius after the wait; a seated actor not at all).

### 5.8 Creatures that cannot walk; perched birds (2026-10-11, owner's playtest)

The owner: "ravens outside Doc Mitchell's house should not have collision; also one of
them was moving around on its own; it should stay where it is then fly off".

- **What they are** [observed, FalloutNV.esm]: four placed creatures (ACRE 0x0014C447,
  0x0014C448, 0x0014C449 initially disabled, 0x0014C44A) of `NVCrRaven` (CREA 0x00140DB0):
  skeleton `creatures\nvraven\skeleton.nif`, ACBS flags Flies, Walks, No Low Level
  Processing, Can't Open Doors, Tilt Front/Back; package `DefaultSandboxEditorLocation512`
  (sandbox within 512 of the editor location); object script `BlackbirdSinging`. The
  skeleton's folder holds an idle and the fly-away idle but no movement clips (the
  engine's locomotion log: "missing Forward, Backward, …"). The skeleton has three
  Biped-layer bodies (bhkBlendCollisionObject), as other creatures' skeletons do.
- **The fly-off is the script's** [documented: the script's source, SCTX]: on load it
  arms itself; in GameMode, once the player is within 512 units (`GetDistance Player`),
  or when hit (OnHit), it plays the idle `NVRavenFlyAway` and counts `fFlyTime` down from
  9, subtracting `GetSecondsPassed` twice a frame, so about 4.5 s; then `Disable` and
  `MarkForDelete`. The engine already ran it: "idle: 0x0014C447 plays NVRavenFlyAway
  (6.43 s)" and "disabled" 4.5 s later [observed, `--play`, 2026-10-11]; the idle's clip
  carries the bird up and away.
- **The stray motion was the sandbox**: the residents walked the ravens' sandbox
  (0x0014C448 "walked 820 units") with no movement clip to play, so they slid. Now a
  creature whose skeleton folder has no `Forward` or `FastForward` clip of its stance
  cannot walk: the residents hand it to the AI **rooted** (`fnv_ai::Ai::root`: no
  package, sandbox, walk, search or push moves it; its scripts still run) [hypothesis:
  the game moves actors at their movement clips' speed, so an actor without one stays
  put; the owner saw the ravens sit still in the game]. Test
  `a_rooted_actor_stays_where_it_is`.
- **No collision for perched birds** [engineering choice, from the owner's wish]: a
  rooted creature whose base has the Flies flag is in nobody's way
  (`ActorBlocks::in_nobodys_way`): it has no block (physics.md §6.8), so the player
  walks through it. It keeps its hit volume, so a shot still makes it fly off (OnHit).
  Nothing in the data read says how the original keeps the player and a raven apart.
- **Checked** [observed, `--play --cell Goodsprings --pos=-73500,2300,8800 --hour 12`,
  `NVF_PLAY_TEST_FACE=10:0014C447` and `NVF_PLAY_TEST_MOVE=10.2:1:0:walk,13:0:0`]: "ai:
  0x0014C447 has no movement animations: it stays where it is; it flies, so it is in
  nobody's way"; no raven in the ai summary's walks; the raven flies off as the player
  arrives and the player walks on through its place ("they changed the player's step 0
  times").

### 5.9 Process levels: the loaded area and the rest (2026-10-12, owner's playtest)

The owner: "check that npcs are not rendered/processing when outside of the cells not in
the vicinity: could hear fiend conversations between each other whilst near Crimson
Caravan". With `--npc-packages all` (the default of `--play`) every actor of every cell
the camera had loaded, and every actor a script moved, stayed with the AI for the rest of
the run with its full update: packages, sandboxes, searches, Dialogue packages, hellos
and random conversations (whose lines the game says), wherever it was. Drawing was
already limited to the loaded cells (`nav::show_actor`), and so were detection, combat
and object scripts (the drawn actors; `Game::loaded_cells`).

What the original does [documented: GECK wiki (JIP) GetActorsByProcessingLevel,
GetActorProcessingLevel; Category:NPC; SetSceneIsComplex]: actors in the loaded interior
or the uGridsToLoad cells around the player are in the **high** process, "processed every
frame"; actors of exterior cells visited and still in memory are **mid-high**, "every ~18
seconds"; unvisited interiors behind doors of the loaded cells **mid-low**; the rest
**low**, "every ~120 seconds". An actor whose base has "No Low Level Processing" "does
not update its AI unless it is in the loaded area".

Now (`fnv_ai::process`, the residents' `process_levels`):
- **High**: a resident whose walker is in a cell drawn now (the 5 × 5 grid of
  `--radius 2`, or the interior) gets every update, as before.
- **Out of the loaded area** (everyone else the AI looks after): an update now and then
  for the time owed, in 0.5 s steps on the navmesh, doors as if open, as an hour in the
  Wait menu: it takes up its current package and walks on. No sandbox choice, no search,
  no Dialogue package, no hello, no random conversation, no turning to a speaker. Leaving
  the loaded area ends a random conversation it walks to, waits for or is in, and a
  search. Updates are spread over the interval by FormID [design]. How often (2026-10-12,
  far-world leftovers):
  - **Mid-High**, every 18 s: the walker is in a cell of the **cell buffer**
    (`engine/nvforever/src/cell_buffer.rs`): the 36 exterior cells drawn most recently
    (the 25 drawn now among them) and the 3 interiors visited last [observed:
    `uExterior Cell Buffer=36`, `uInterior Cell Buffer=3`, `uGridsToLoad=5` in the owner's
    Fallout_default.ini; hypothesis: that the buffer keeps the cells drawn most recently
    and counts the drawn ones]. Going into an interior leaves the exterior buffer as it
    was, so the exterior actors around stay Mid-High (the wiki's Strip example).
  - **Mid-Low**: interiors behind the load doors of the exterior cells drawn this session
    ("accessible through teleport doors within player's loaded cell grid", and once
    there "it never seems to go back to Low") count as buffered, every 18 s [design: the
    original's rate is "unknown"; 18 s is what those actors had before].
  - An actor walking out of the loaded area into a cell the buffer does not hold goes
    to **Low**, not Mid-High: Victor, patrolling out to 15,508 units from the player,
    went from process level 0 straight to 3 and never read 1 [observed:
    `docs/handoff/observer-jip-audit-2026-10-25.md` §3.1, campaign jip_state_goodsprings,
    GetActorProcessingLevel; issue #958]. (Until 2026-11-01 such an actor took its cell
    into the buffer and stayed Mid-High.) This is for a walk out in a worldspace: one
    going from the loaded area through a door into an interior the buffer does not hold
    takes that interior into the buffer, since interiors behind the loaded area's doors
    read Mid-Low (2) in the original [observed: same §3.1; Benny riding the lift into
    2ETopsPresidentialSuite, smoke `scene-tops-suite`, whose Dialogue package must take
    over before the player follows]. One already at Mid-High walking on into
    another cell takes that cell into the buffer [hypothesis: it keeps its surroundings
    in memory], and a follower (Follow or Accompany package) out of the buffer is kept
    at Mid-High while its leader is processed at Mid-High or above [design].
  - **Low**, every 120 s: anywhere else ("neither currently loaded nor buffered").
  - **Coming** (2026-10-13, owner's playtest list 4 item 13: Ringo came out of the gas
    station only after the fight): an actor whose rest of path has a corner in a drawn
    cell is moved every frame, still without the high process's extras, so it comes
    through a load door when it would have in sight [design: the original's rule for
    actors heading to the player's area is not documented]. Smoke scenario
    `ringo-comes-out`.

  Moving between the two keeps the share of the interval the actor is owed.
- **No Low Level Processing**: no update at all out of the loaded area, not even for
  Wait-menu hours (`Ai::pass_time`).
- The level is set every frame from where the walker is; going back in, the actor takes
  up its full update from where the low updates left it.

Checked by the `far-actors` (camera flying north at 400 units/s: Mid-High updates) and
`far-actors-low` (1500 units/s for 150 s: actors falling out of the buffer, updates every
120 s) smoke scenarios (tools/smoke/scenarios.toml); the "process summary" line counts
who left the loaded area, who fell out of the buffer and the updates at each rate. Tests
in `crates/fnv-ai/tests/carry.rs` ("process levels") and `cell_buffer.rs`.

- **Far travel to the player** (2026-10-24, owner: Malcolm Holmes after the first star
  cap): the original moves such an actor in jumps once per game hour along a route, at
  about a walk's pace although his package runs, stops the jump at the edge of the loaded
  area and runs in from there; it kept to its first route for 3 hours after the player
  moved [observed: ai-observer.md §13]. Here: out of the high process a Dialogue package
  without a wait location (Holmes's has none) takes its actor towards its target (no
  talking), on the route planned when it set off, planned again only when that route ends
  with the target elsewhere [hypothesis]; one with a wait location keeps its actor there
  (Ringo's GSRingoDialoguePackage would otherwise follow the player out of the gas
  station) [hypothesis]; an actor a script puts out of the loaded area never takes its
  cell into the cell buffer (only leaving the loaded area does), so it is at the Low
  level; at the
  Low level actors walk whatever their package says [hypothesis: one actor seen]; a
  low-process walk stops where it comes into the loaded area (`Ai::set_loaded`, from the
  engine) and the high process takes over; the Coming level (moved every frame) is only
  for actors in the cell buffer, as from beyond the original moves them in its jumps.
  Unit tests in `crates/fnv-ai/tests/talk.rs` ("far away …") and `carry.rs`
  (`a_far_walk_stops_where_it_comes_into_the_loaded_area`); smoke scenario
  `holmes-comes-from-far`. Worldspaces: an actor only goes where a path of navmesh and
  load doors leads, so with no door link to the player's worldspace there is no route
  and the package is set aside (§5.2); Holmes's own package also needs the player in
  WastelandNV.

Not done: actors of cells never loaded are not handed to the AI at all, as before (an
actor a script moves with MoveTo is: `take_moved`, §5.2).

### 5.10 Escort, Find, Use Item At, Ambush, Flee Not Combat (2026-10-17)

The last five package types (190 of the masters' 4,885 packages: Find 88, Ambush 44, Use
Item At 30, Flee Not Combat 15, Escort 13). The rules, what the documentation says and
what is a hypothesis: docs/spec-notes/packages.md §3.1; the original's actors under them,
watched with the AI observer: §4 there (`tools/capture/observe/package_kinds.campaign.json`,
which gives Goodsprings residents the packages with AddScriptPackage). Code:
`fnv_ai::goal` (`Escort`, `Find`, `UseAt`, `Ambush`, `Flee`, `Wanted`, `Area`) and
`fnv_ai::errand` (`Errand`, the progress kept on the agent), on `crate::carry`'s walks;
`RefExtra::used_item` and `ambush_hidden` in `fnv-game`. Events (`AiEvent`):
`escort_waits`, `escort_on`, `find_goes`, `found` (with `act`: takes, loots, harvests,
uses, attacks, reaches), `nothing_found`, `uses_at`, `uses_item`, `ambush_waits`,
`ambush_sprung`, `flee_runs`, `flee_safe`; the engine logs them as `ai [h] <actor>:
escorts …: waits …`, `found …: takes (…)`, `uses … (…)`, `lies in wait, hidden (…)`,
`springs its ambush on …`, `flees from …: runs …`, `is safe … (…)`. Tests:
`crates/fnv-ai/tests/errands.rs`; smoke scenarios `package-escort`, `package-find`,
`package-use-item-at`, `package-ambush`, `package-flee`.

`fnvtool ai reach` over the ten masters [observed 2026-10-17]: the 362 actor/package
pairs that were "not carried out yet" (Escort 4, Find 243, Use Item At 20, Ambush 80, Flee
Not Combat 15) are now reachable 217 (Escort 4, Find 111, Use Item At 20, Ambush 80, Flee
2), "stays where it is" 144 (Finds searching where the actor stands, 131; Flees with no
destination, 13) and "no navmesh near the start" 1; every other verdict is unchanged
(reachable 10,403 → 10,620, staying 1,691 → 1,835).

With them came: GetIsUsedItem, GetIsUsedItemType, GetIsUsedItemEquipType and
GetUsedItemActivate (what an actor uses: its furniture, or what its package had it take
or use); Pretend In Combat (running, weapon out), Allow Swimming (water avoided without
it), Allow Falls (ledge links allowed with it) and No Warn/Attack Behavior (no
aggro-radius warnings); packages.md §3.1.

## 6. Decisions for the owner

1. **Keep-current when nothing is valid** (rule 3) rather than "no package": taken from
   one settler's nights; reversible in one line.
2. **The linked-reference rule applies to every package type** that names the actor's
   linked reference, not only Patrol and Ambush.
3. **10 seconds of real time** between choices, per-actor phases: until a capture
   measures it (packages.md §6).
4. **Script packages stay as in Layer 5** (a stack; one stays until removed): the
   documentation says an actor holds one and loses it at the next evaluation unless it
   is Must Complete, but RingoTravelPackage, which is not, ran for 887 samples. To be
   captured (actor-behaviour.md §8, questions 3 and 4).
5. **An unreachable package is set aside for one game hour** and the actor takes the
   next one (§5.2).
6. **Arrival meets the goal of every package type that goes somewhere** (§5.2).
7. **In the engine by name first** (`--npc-packages`), with a game state of its own;
   every actor and `--play`'s state come next (§5.4).
8. **A Dialogue package's actor goes up to a target inside its trigger box** also when
   its wait location has no radius (§5.6): the documentation has it stay; the data and
   the greeting in the saloon say otherwise. One capture settles it.
9. **`--play` runs no packages unless `--npc-packages` is given**, so the force greets
   of the game need `--play --npc-packages all`. Making that the default of `--play` is
   the owner's call (frame rate with every actor of the loaded cells: §5.4).

## 7. Notes to look into

Kept here so they are not lost; each is also a line of the owner-review section.

From `fnvtool ai reach` (the masters, doors and locks as authored):
1. **200 actor/package pairs have no chain of doors to the place**, 126 of them Travel
   and 44 Patrol packages (examples: the Primm deputies' patrols, the Nellis side-tower
   guards, SSHQMalcolmHolmesHeadsOutPackage, VMQYesMan01MoveToTerminal). Likely causes
   to check one by one: places reached by a ladder, lift or scripted move rather than a
   door; navmeshes or doors enabled by a quest; doors with no navmesh door link; a
   target moved by script before the package is used.
2. **112 pairs are behind a locked door the actor has no key for** (the REPCON ghouls
   and Harland, for example): doors a quest unlocks. Lock changes made in the game
   (scripts, the package door flags) are not passed to the nav graph yet.
3. **186 pairs start with no navmesh within 512 units of the actor's placement** (132
   Travel, 32 Sandbox, 15 Guard, 7 Patrol): actors placed off the navmesh. Are they
   enabled by quests into cells without navmesh, creatures that do not need one, or a
   snap distance that is too small?
4. **6 pairs have no navmesh near the goal** (two Legate's camp snipers, a Hoover Dam
   barricade guard, …).
5. **177 packages are located at the player**: they need the player's place, which a
   running game has; not exercised yet.

From the comparison with the captures:
6. **Patrols wait at their points now** (the point's idle time, `XPRD`, §5.5); the
   topic and result script a point can carry are not run, and what a repeatable patrol
   does at the end of a chain that does not loop is a guess. Victor against his capture
   is no test of it: there he had no package in 2,413 of 2,785 samples (quest state),
   and matches 30% of the time with the waits as without.
7. **Sandboxing is "go there and stay".** A settler in its sandbox area stands 608 units
   (median) from where the capture has it: wandering, furniture, idle markers, eating
   and sleeping inside a sandbox are not modelled. (In the engine, actors now sit and
   sleep where a package names the furniture or is a Sleep or Eat package, §5.4; the
   headless run still has Easy Pete standing 28 units from his chair.)
8. **NPC walking and running speeds are not captured**; the player's are used.

Rules that are our choice or a hypothesis (also §6):
9. What the game does with a package it cannot carry out (we set it aside for an hour).
10. Whether arrival ends Sleep, Eat, Sandbox and Guard packages that are Must Complete.
11. "In cell": the actor stops where it enters; the game probably goes on to a bed, a
    seat or a spot inside.
12. Travel is documented to prefer "preferred" navmesh triangles only outdoors; the path
    cost prefers them everywhere.
13. Keys: everything the actor carries opens what it fits; whether NPCs need keys at
    all, or open any door their package leads through, is not known.
14. Actors far from the player walk the whole way in real time. The game moves unloaded
    actors differently (they were seen standing still for hours and then at their new
    package's location; ai-observer.md §7).
15. The periodic choice every 10 seconds of real time, phases per actor (step 3); Once
    Per Day, Must Complete and Continue If PC Near follow the documentation and have not
    been seen in a capture; script packages are kept as in Layer 5.
16. Twelve packages have a starting hour and duration 0: they never run under the window
    rule.

Not done yet:
17. Package types not carried out: none since 2026-10-17 (§5.10), apart from Dialogue
    packages between two actors (3; the others are carried out, §5.6); the run-time
    type codes of the types not seen running are still open.
18. In the engine the residents' physics is a first version: the player's capsule
    with a steeper slope limit, no actor-to-actor avoidance, the walker for the last 64
    units and whenever physics gets stuck (§5.4).
19. Package flags not acted on: Pretend In Combat's cover moves (it runs with its weapon
    out, §5.10); Find's Allow Buying; begin / end / change topics (package idles and begin / end / change idles
    are played, docs/spec-notes/idles.md).
20. The game's own run-time packages (combat, dialogue, getting up from furniture;
    forms 0xFF…) have no counterpart.

From the owner watching the travel test (2026-10-10):
21. **Doc Mitchell leaves his room the long way round**: through the living room and
    back along the hall, although a doorway next to him leads straight to the front
    door. The game's navmesh for the house (NAVM 0x00108724, 123 triangles, read
    straight from FalloutNV.esm) has **no triangles in that doorway**: the room's
    navmesh ends at y ≈ 1,753–1,788 and the hall's begins at y ≈ 1,706, about 50 units
    apart, with boundary edges on both sides [observed]. The doorway is open in the
    collision world (a physics probe walks through at x ≈ 2,074) [observed]. So the path
    is the one the game's own navmesh gives, not a choice of the test or a doorway our
    pathfinder misses. Whether the original game's actors also go round is not known:
    the captures only have Doc standing in two spots. To settle: watch an NPC cross that
    house in the game, or look at the cell's navmesh in the GECK. If the game does walk
    through, navmesh edges facing each other across a small clear gap would need
    joining; not done, since it would depart from the data.
