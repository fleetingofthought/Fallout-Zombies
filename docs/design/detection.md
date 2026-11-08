# Detection design: who perceives whom

Owner request 2026-10-11: NPCs and creatures detect the player and each other, from
Perception and the target's Sneak when it sneaks, with the numbers that decide hidden or
detected in place. Combat encounters will be driven from it later; this step is the
basics. Facts and their sources: docs/spec-notes/actor-behaviour.md §2 (the GECK wiki
Category:Detection page in the owner's export, and the script catalogue notes).

Tags as in the spec notes: [documented: source], [observed], [hypothesis], and
[engineering choice] for what NVForever decides where the original's rule is unknown and
parity does not matter.

## 1. What is built

| Part | Where | What |
|---|---|---|
| The formula | `fnv_game::detection` | `Settings` (the 31 game settings with their documented defaults, read from the load order), `Pair` (what the two actors' records and actor values say), `Situation` (what the world shows), `Settings::evaluate` → `Detection { value, band, sound, visual, detector_skill, target_skill, … }` |
| Bands | same | hidden (≤ −20, fSneakNoticedMin), noticed (−20 < D ≤ 0), detected (> 0) [documented] |
| The table | `Game::detection` | the latest value of every (detector, target) pair the engine measured; not saved |
| Script functions | `fnv_game::detection` | `GetDetected` (band detected), `GetDetectionLevel` (0 not measured, 1 hidden, 2 noticed, 3 detected) |
| Armour class | `fnv_sim::forms::Item::armor_class` | ARMO BMDT general flags Medium and Heavy [documented: xEdit] |
| The engine | `engine/nvforever/src/detection.rs` (`--play`) | four times a second every living drawn actor (the loaded area) measures the player and the other actors within its reach, near the player or not, and the player measures the actors from the camera (§7); logs band changes and the player's highest band; `detection summary` at exit |
| Geometry | `fnv_game::sight` | the field of view, the pairs to measure, sight over the landscape (§7) |
| Numbers without the game | `fnvtool actor detect` | §4 |

## 2. The formula

    D = fSneakBaseValue + Sound + Visual + fSneakSkillMult · (DetectorSkill − TargetSkill)

exactly as actor-behaviour.md §2.1 gives it [documented: GECK wiki Category:Detection].
The doc comment of `fnv_game::detection` spells out every term. Two readings the page
leaves open:

- A running target also counts as moving in the visual movement multiplier
  (1 + 0.01 + 0.2) [documented since 2026-10-24: the Fallout Wiki's Sneak (New Vegas)
  page gives 0.21 for running, §9].
- The distance falloff is 0 at and beyond the reach, so only the base and the target's
  skill remain there: a target out of reach is hidden (−35 or lower).
- The target's Sneak (TargetSkill) counts only against a detector that sees the target
  or hears it (Sound above 0): an unseen, silent target reads the same sneaking or
  standing, by distance alone [observed: jip_detection, observer-jip-audit-2026-10-25.md
  §3.2: −12 to −14 within 500 units, −32 to −33 at 3,000–4,000, −34 to −35 beyond, day
  or night, standing or sneaking; #969]. Kept when the target is heard [hypothesis: no
  unseen, noisy case was captured].

## 3. What the engine measures

Each is also listed in the module notes of `detection.rs`.

| Input | How | Tag |
|---|---|---|
| Distance | feet to feet | — |
| Reach | fSneakMaxDistance, × fSneakExteriorDistanceMult outside | [documented] |
| Line of sight | a ray through the collision world from the detector's eyes to the target's chest (¾ of its eye height), and when that is blocked one to its eyes (§7); for the player's GetLineOfSight from the camera; world shapes block it; actors and their capsules do not (the ray goes on past them), nor do the transparent layers, invisible walls, clutter, small debris and shell casings (docs/design/alert.md §7). Where either end is out of the loaded collision (3 × 3 cells around the player), only the landscape blocks it (§7) | [engineering choice]; the layers: [hypothesis] |
| Field of view | 95° either side of where the actor's body faces, horizontally; for the player, of where the camera looks | [documented: GECK wiki GetLineOfSight]; using it for detection: [hypothesis] |
| Light at the target, 0–100 | outdoors 100 × the brightest channel of the weather's sunlight colour ÷ 255, its times of day blended on the stored bytes, day and night alike (the ambient is not counted); indoors the brightest channel of the cell's ambient colour; plus each point or spot light that reaches the target, its brightest channel × (1 − (d/r)²); capped at 100. Outdoors the sunlight counts only when a ray from the target's chest towards the sun meets nothing within 8,192 units (§8); a shaded target gets the weather's ambient instead. While the Pip-Boy light is on, the game's PipboyLight640 counts as a light at the player's chest (§9) | the sun and ambient terms: [observed: fitted to the original, below]; the lamps, the shade and the Pip-Boy light: [hypothesis] |
| Moving, running | from the distance moved since the last measurement: moving above 10 units/s; actors run above 215 units/s; the player runs when moving without holding Walk, sneaking or not | [engineering choice]; sneak-running counts as running: [hypothesis] |
| Sneaking | the player's sneak toggle; actors never sneak (Always Sneak packages are not acted on) | — |
| Action sound | the player's shots since the last measurement, at the weapon's VNAM sound level (100 / 50 / 10) | [documented] |
| Movement sound of a creature | the boot term × the creature's NAM5 sound level ÷ Normal: Loud × 2, Normal × 1, Silent × 0.2 (§8) | [hypothesis] |
| Detector asleep | lying on a bed (marker 1–10, state "on it") | [hypothesis] |
| Detector alert, or on its target | alert in a combat state (Alert, Combat, Lost); fSneakCombatMod when the detector is in Combat and the target is on its target list (IsCombatTarget), so 0.8 against its foes and 1.2 against anyone else (§9) | [documented] |
| Interval | 0.25 s | [engineering choice] |

Pairs not measured for 1 s of game time are dropped from the table.

**The original's light at the player** (GetActorLightAmount, JIP; no Pip-Boy light, god
mode, standing at the `coc Goodsprings` spot or the interiors' `coc` spots; weather as the
`nvftest` save had it) [observed: campaign o11_light, 2026-11-01, issue #970; earlier noon
100 and midnight 82.4 in observer-jip-audit-2026-10-25.md §3.2]:

| Where | Hour | Light |
|---|---|---|
| Goodsprings outdoors | 03:00, 05:00 | 82.35 (210/255) steady |
| Goodsprings outdoors | 06:00–06:06 | 82.35 falling to 79.2, then 81.9 |
| Goodsprings outdoors | 07:10, 08:00, 17:00, 18:00, 19:00 | 100 |
| Goodsprings outdoors | 20:05–20:12 | 77.0 falling to 74.4 |
| Goodsprings outdoors | 21:00 | 73.5 rising to 82.35 |
| Prospector Saloon, Doc Mitchell's house | 12:00 and 00:00 | 100 |

More readings (campaign o27_light, `nvfairport` save, 2026-11-06, the same `coc Goodsprings`
spot with `ForceWeather <w> 1`, and the interiors' `coc` spots) [observed]:

| Where | Weather | 12:00 | 00:00 |
|---|---|---|---|
| Goodsprings outdoors | NVWastelandClear | 100 | 82.35 |
| Goodsprings outdoors | NVWastelandHazy | 99.22 (253/255) | 64.31 (164/255) |
| Goodsprings outdoors | HVDustStorm | 0 | 0 |
| Goodsprings cave (SLGoodspringsCaveINT) | — | 36.08 (92/255) | 36.08 |
| Abandoned shack (GSAbandonedShack) | — | 99.30 | 99.30 |

**The fit (#970).** Every outdoor reading is 100 × the brightest channel of the weather's
sunlight colour (WTHR NAM0) ÷ 255 [observed: the readings against FalloutNV.esm]:
NVWastelandClear's day sunlight (255, 227, 170) gives 100 and its night sunlight (166, 191,
210) 210/255 = 82.35; NVWastelandHazy's (253, 244, 225) and (62, 92, 164) give 99.22 and
64.31; HVDustStorm's sunlight is black at every time and reads 0, though its night ambient
is (94, 94, 174). The ambient is not in it: NVWastelandClear's night ambient (172, 170, 215)
would give 84.3. The dusk dip of o11_light fits the same rule with the sunset and night
colours mixed on their bytes: the brightest channel of the mix of (255, 173, 83) and (166,
191, 210) is lowest, 187 or 73.4, about four fifths of the way, where 73.5 was read at
21:00. Indoors the cave reads the brightest channel of its cell's ambient (73, 92, 83),
92/255; its directional light (9, 7, 6) is not added. The shack (ambient (46, 41, 27),
directional (101, 99, 58)) and the two Goodsprings rooms of o11 read 99–100, so their lamps
must make up the rest [hypothesis: the point lights are added]. NVForever measures it so
since 2026-11-06 (engine/nvforever/src/detection.rs `light_at`): no daylight factor, and
the lights' brightest channel instead of their luminance.

A lamp's share [observed: job o29_light, 2026-11-08, `nvfairport`]: in SLGoodspringsCave
INT (ambient alone 36.08), the player moved by `MoveTo` to set offsets in x and y from one
RedLight (0x0015319B, radius 384, colour (255, 57, 47); the next lamp 511 units off) read
100 out to 192 units, about 95–97 at 256, 67–70 at 320 (one 97 on the -x side, where the
floor was 80 units lower), and the ambient 36.08 at 384 and 512 (one 67.6 at +384, the
same as the read before it, perhaps not yet refreshed). So a lamp's share ends at its
radius. Ours, ambient + 100 × (1 − (d/r)²) on the horizontal offset, gives 91.6 at 256
and 66.7 at 320: within about 5 [hypothesis: the falloff's exact form; the distance is
measured from the lamp to the player's position, not the chest].

A shaded spot outdoors is still open: at noon in NVWastelandClear, 14 spots from the
Goodsprings gas station's SCOLgsGasStation origin out to 800 units (and one 2,800 units
off) all read 100 [observed, same job], but which of them stood under its canopy is not
known, so this neither confirms nor rules out shade. Next: a spot whose roof is known
(an interior-like overhang checked by a frame).

## 4. `fnvtool actor detect`

```
fnvtool actor detect --schema schemas/fnv <Data> FalloutNV.esm --detector SunnyRef [--target player] [--interior] [--light 100] [--dark 82.35]
```

For one pair in the authored game, `fnv.actor.detect/1`: the settings, the inputs, and
for 13 situations (standing, walking, running, lit and dark, sneaking, behind the
detector, behind a wall, firing) the value and band at 100–5,000 units and the farthest
distance at which the target is detected and noticed.

The defaults are the original's light at Goodsprings at noon and at midnight (§3); until
2026-11-06 they were 90 and 10 [design], and the smoke scenario `detection-table` still
passes those to keep the table below. With the new defaults (2026-11-06) Sunny detects a
standing player out to 2,690 units lit and 2,500 dark, and one sneaking out to 1,980 and
1,730 (noticed 2,530 and 2,330): a Mojave night hides the player little.

Sunny Smiles (Perception 5, level 2) against a level-1 player with Sneak 15, outdoors,
lit 90, dark 10 [observed: 2026-10-11]:

| Situation | Detected out to | Noticed out to |
|---|---|---|
| standing, lit | 2,590 | 3,420 |
| running, lit | 2,940 | 3,650 |
| standing, dark | 260 | 1,890 |
| sneaking, lit | 1,840 | 2,420 |
| sneaking, dark | never | never |
| sneaking and walking, dark | never | 850 |
| behind a wall, walking | never | 1,260 |
| sneaking, firing a normal gun, lit | 2,800 | 3,200 |

The light level decides most of the visual term, and its measure is a hypothesis
(§3): it is the first number to tune against the game (§6).

## 5. Checks

- Unit tests: `fnv_game::detection` (the formula's terms, bands, the table) and the
  engine's field of view and light sum.
- In the game (`--play`, Goodsprings at noon, with `NVF_PLAY_TEST_SCRIPT_MENUS=1
  NVF_PLAY_TEST_EQUIP=000E3778 NVF_PLAY_TEST_GIVE=0008ED03:50
  NVF_PLAY_TEST_CONTROL=10:sneak,14:weapon,17:reload,22:fire,22.2:fire`):
  `nvforever --data <Data> --cell Goodsprings --play --hour 12 --no-voice --run-seconds 28`
  logs the settings line, band changes such as `detection: GSSettlerAM 00104F0A → the
  player: hidden → noticed (value -10.4: … skills 16.8 against 55.0; 2104 units, in
  sight, light 93, sneaking)`, the shot taking that settler to detected (sound 55.3),
  the player's meter (`the player is DANGER (NVCrRaven 0014C447: 43.3)`) and the
  `detection summary` line [observed: 2026-10-11].

## 6. Next and open

- **Detection states** (Normal, Alert, Combat, Lost), the target list and its timers,
  and detection events: built 2026-10-11, docs/design/alert.md. Explosions, impacts at
  their own sound level and projectiles in flight are events since 2026-10-23 (§8).
- **Captures** to tune against (actor-behaviour.md §8 items 7 and 8): GetDetected for the
  player standing, walking, running and sneaking at set distances, lit and dark, and the
  game's values of the fSneak* settings (the masters may override the documented
  defaults; `Settings::read` takes them when present).
  And for §9: a sneaking player, half hidden, read by an NPC in Combat with the player
  and by one in Combat with someone else (fSneakCombatMod's direction), and GetDetected
  at night with the Pip-Boy light on and off.
- Not modelled: perk entry point "Ignore Running During Detection", melee blows' own
  sound (a blow's impact counts, §8), SetAllVisible, player.GetLineOfSight on objects
  that are not actors, the sneak meter on the HUD, fights in the low process (§7).

## 7. The player as a detector, and pairs far from the player (2026-10-17)

Two gaps the function probes found (docs/design/script-function-coverage.md §5 item 1):
`player.GetDetected X` and `player.GetLineOfSight X` always read 0, and two hostile NPCs
out of the player's reach never noticed each other, so their fight never started.

**The player detects.** In the original `player.GetDetected Ringo`, `player.GetLineOfSight
Ringo` and `player.GetDetected Chet` read 1, the last with `tdetect` on [observed: function
probes fp1/fp2, 2026-10-17]. On the player GetLineOfSight is the camera's ("it returns
true if the *camera* can see the object", so in third person things behind the player
can count), with the same ±95° [documented: GECK wiki GetLineOfSight]. NVForever measures
the player against every drawn actor with the same formula as an actor: its Perception
against the target's Sneak, from its eyes, within ±95° of where its body faces, through
the same line of sight [hypothesis: the formula]. Facing matters as for an actor
[observed: capture detection-1 det1, 2026-10-17, in third person: Ringo 263 units away
read player.GetDetected and GetLineOfSight 1 at 45° either side of the view and 0 at
135° and 151°; capture detection-2 det4, Ringo 200 units away: GetDetected 1 to 90° off
the facing on both sides, 0 from 100°]. GetLineOfSight on the player is the camera's:
the same ±95° and wall check, from the camera along its view (`Situation::camera_sight`)
[observed: same capture, 0 at 90° to the left but 1 up to 110° to the right, 0 at 120°:
lopsided as the third-person camera sits off to one side]. In first person the two are
the same. Out of reach the player still measures (the target is hidden,
GetDetected 0) so that GetLineOfSight answers at any distance [observed: same capture
det2, player.GetLineOfSight Ringo 1 and GetDetected 0 at 6,682 units outdoors]. `tdetect` and `NVF_PLAY_TEST_UNSEEN` do not stop it. The
player's own measurements feed the two functions only: no detection state, no alarm.
Not built: player.GetLineOfSight on an object that is not an actor (the original allows
it on the player).

**Every pair of the loaded area.** All living, drawn actors (the loaded 5 × 5 exterior
cells, or the loaded interior) measure each other within detection reach, wherever the
player is (`fnv_game::sight::pairs_within`); the player is one of the points, no
longer the centre of the pass [observed: capture detection-1 det2, 2026-10-17: Chet made
Frenzied 1,400 units from Ringo and 6,682 from the player was in combat with Ringo within
1 s (IsInCombat 1 for both, combat target Ringo, GetDetected 1); the fight was over by
26 s]. The detection states, the combat
AI and alarms take these pairs as they did the near ones, so a far fight starts and
IsInCombat reads 1.

What it approximates:

- **Sight far away.** Collision is loaded only in the 3 × 3 cells around the player.
  When either end of a line of sight is out of it, the landscape alone blocks it,
  sampled every 96 units along the line from the cells' LAND heights (read once per
  cell) [engineering choice]: hills hide one camp from another, buildings and rocks do
  not. Inside the collision world nothing changed.
- **Out of the loaded area** (the low process, `fnv_ai::process`) nobody detects
  anyone: those actors keep walking their packages and have no combat state, so a fight
  there starts only once the player comes near enough to load their cells. So does the
  original, as far as one capture shows [observed: capture detection-1 det3: with the
  player in the gas station, Chet made Frenzied outdoors near Ringo never detected him
  (GetDetected 0) and neither was in combat at 5, 35 and 155 s; their GetDistance read
  the float maximum, as for references not loaded]. Only the cell buffer case (Mid-High,
  the player indoors next to them) was tried, not actors far out in the Low level.
- **Cost.** Pairs are found by distance among the drawn actors (all pairs, at most a few
  thousand distance checks four times a second); rays and land samples only for pairs
  within reach. The exit line `detection summary, out of the player's reach` counts the
  pairs measured with both actors out of the player's reach, and the actors the player
  detected.

**Walls and obstacles** [observed: capture detection-2, 2026-10-17; ours the same on each
spot, checked first in NVForever]: Ringo 200 units away in clear sight 1, behind the gas
station's checkout counter (a low obstacle) 1,
behind the grocery shelves 0, both ways (the player's and Ringo's GetLineOfSight and
GetDetected); Doc Mitchell through a doorway of his house 1. Two cases of the capture
did not place Doc where meant (a solid wall, a doorway edge: he ended in another room)
and show nothing; no glass or chain-link spot was tried. Looking from the player's
eyes, our one ray to Ringo's chest hit the counter (0), so a blocked ray to the chest is
followed by one to the target's eyes: a head over a low wall is seen [hypothesis, fitted
to this case; every detector, NPCs too].

Checks: `fnv_game::sight` tests (field of view, pairs far from the player, a ridge
between two actors), `crates/fnv-game/tests/functions.rs`
(`the_player_detects_and_far_pairs_fight`: player.GetDetected / GetLineOfSight from a
measured pair; a Frenzied actor out of the player's reach goes to Combat, IsInCombat 1);
smoke `detection-far-fight` (the NCR troopers and Cook-Cook's Fiends fight with the
player about 5,700 units away) and `detection-game` (the player detects actors).

## 8. The records' sound levels and the sun's shade (ESM audit group 6, 2026-10-23)

The records name a sound level (Loud, Normal, Silent: `SoundLevel` in the schema)
[documented: xEdit wbDefinitionsFNV.pas WEAP VNAM, PROJ VNAM, EXPL DATA, IPCT DATA,
CREA NAM5]. Only the weapon's fed detection. In FalloutNV.esm [observed 2026-10-23,
private survey]: WEAP 57 Loud / 119 Normal / 85 Silent; PROJ 1 / 90 / 4; EXPL 71 / 75 /
8; IPCT 11 / 96 / 18; CREA 0 / 1,554 / 24. A level is worth iSoundLevelLoud, Normal or
Silent (100 / 50 / 10), as a shot's.

- **Impacts** raise their detection event (alert.md §4) at the impact record's (IPCT)
  sound level [hypothesis: the impact's own loudness, its field's name]; an impact
  without one at the firing weapon's, as before. 85 weapons are Silent while their
  projectiles are Normal, so the PROJ level is not the shot's.
- **Projectiles in flight** raise an event where they are, at the PROJ's sound level,
  owned by the shooter, once a measurement (0.25 s) for each projectile seen since the
  last [hypothesis: PROJ VNAM is the noise of the projectile passing; a bullet passing an
  actor alerts it, as the bullet whiz is heard]. A beam's event is a Beam event, others
  Bullet.
- **Explosions** raise an Explosion event where they go off at the EXPL's sound level,
  owned by whoever set them off (the thrower, the shooter, the mine's placer); one with no
  known cause (a destruction stage) raises none [design]. Before, an explosion's dirt
  impact raised a Bullet event owned by the player for every explosion, the NPCs' too;
  that impact now belongs to the cause.
- **Creatures**: the movement sound of a moving creature (the boot term, its armour
  weight 0) is scaled by its NAM5 level ÷ Normal [hypothesis: NAM5 is how loud the
  creature moves; 1,554 of 1,578 are Normal, so most are unchanged; the 24 Silent ones
  move at a fifth]. People keep the boot term.
- **Sun shade** [hypothesis]: outdoors, the sunlight's share of the light at the target
  counts only when a ray from the target's chest towards the sun (above the horizon)
  meets nothing solid within 8,192 units [design: the reach]; a shaded target gets the
  weather's ambient in its place (§3, since 2026-11-06), point lights are unchanged.
  Where the target is out of the loaded collision the sun counts.

Checks: the `detection summary` line counts events by kind (impacts, projectiles,
explosions) and the targets measured in shade. Smoke `detection-shade`, `detection-events-levels`.

## 9. Audit against the Fallout Wiki's Sneak page (2026-10-24)

The owner asked for the detection system, mainly sneaking, to be checked against the
formulas of the Fallout Wiki's Sneak (Fallout: New Vegas) page (owner's saved copy,
`C:nv-privateallout fandom`), taking nothing the game data or the GECK wiki
contradicts. The page is a secondary source; its formulas are the GECK wiki's
Category:Detection with the default settings filled in.

**Agrees with what was built** (no change): the reach (2,500 inside, 5,000 outside);
the falloff squared; `D = A·(Sound + Visual + DetectorSkill/2) − TargetSkill/2 − 35`
(ours applies the falloff inside each term, the same sum); sound × 1.6 in sight and 0.16
out of it (fSneakSoundsMult × fSneakSoundLosMult); movement sound `(12 + weight/2) ×
1.5 running`; action sound 100 / 50 / 10 doubled; visual 0 out of sight or for an
invisible or chameleon target (fSneakStealthboyMult 0); night eye × 3 with the cap at
100; light × 1.4; visual movement 0 / 0.01 / 0.21 (the running reading of §2 was right);
detector skill `10 + 8·Perception`; sleeping 0.8; the target skill with the level terms,
the start bonus and the armour class penalty 10 / 20; the thresholds −20 and 0.

**Changed:**

- **The combat modifier was the wrong way round.** We took fSneakCombatMod (−0.4) for a
  detector in Combat with someone other than the target, from the setting's description
  on the GECK wiki ("in active combat against someone else"). The same page's formula
  writes it `IsCombatTarget * fSneakCombatMod`, IsCombatTarget being 1 when the target
  *is* one of the detector's combat targets, and the Fallout Wiki gives 0.8 for "In
  Combat and trying to Detect a Current Combat Target" and 1.2 for anyone else, or for
  an Alert or Lost detector [documented: both pages]. Two readings of the sources agree
  against one, so a fighter now perceives its own foes at 1 + 0.2 − 0.4 = 0.8 and
  everyone else at 1.2; a Lost detector at 1.2 (its targets are on its list, but it is not
  in Combat). It makes a sneaking target easier to lose in a fight. A capture could settle
  it: GetDetected of a sneaking player by a fighter it is shooting against one it is not
  (§6, captures).
- **The Pip-Boy light gives the player away.** We left the light out of detection as
  undecided. The game says it counts: a FalloutNV.esm loading screen, "The light
  generated from your Pip-Boy light actually makes you less stealthy", and Broad
  Daylight's description in LonesomeRoad.esm, "a sneak bonus to offset the light's sneak
  penalty" [documented: the records' text]. The game's light is the base effect PipLight
  (00021EC4, archetype Light) naming the LIGH PipboyLight640 (0002923E: radius 768,
  colour 194 245 209) [observed: FalloutNV.esm decode]. While our flashlight is on, that
  record counts as a point light at the player's chest, read from the load order
  [hypothesis: the penalty is that light raising the light level at the player; nothing
  gives it as a number]. At the player it adds about 91 of the 100, so a lit player is
  near the top of the light term day or night; it also lights actors near the player.
  Our own flashlight cone (a design light) is not counted. The `detection summary` line
  counts the measurements with it on and the most light at the player then.

**Not taken: contradicted.**

- "Movement Multiplier 0 … or has Silent Running": the GECK wiki says the Ignore Running
  During Detection entry point (Silent Running's) only drops fSneakRunningMult, so
  walking and running both make the walking sound. Built so since PR #672.
- "Equipped Weight", all equipped items with the weapon in hand: the GECK wiki's
  fSneakBootWeightMult "multiplies the weight of worn body armor". We keep the torso
  armour's weight.

**Not built, from the page's prose** (no number, or a feature of its own; none
contradicted): touching or bumping into an actor while sneaking detects you at once;
dead bodies seen put an actor in Caution and draw it to them; looking around counting
as movement; a crippled head lowering a detector's Perception by 4 (the player's comes
from GenericScript; NPCs' has no source in the data); pickpocketing and its chance
formula (capped 5–85 %), not built at all.

Checks: `fnv_game::detection` test `perception_night_eye_stealth_boys_and_the_detectors_state`
(a fighter's foe at 0.8, anyone else at 1.2); smoke `detection-pipboy-light`.

## 10. Light and noise gauges (owner request 2026-10-24)

An optional HUD addition, after the stealth games that show how lit and how loud the
player is [design: not the original's]. The settings page's Gameplay switch
`sneak-gauges` (off by default, kept in the INI) shows two segmented bars under the
sneak word while the player sneaks (`nvf_play::sneak_gauges`):

- **Light**: the light level at the player, 0–100, from the detection pass (§3, with
  the Pip-Boy light of §9), the bar following changes over half a second.
- **Noise**: the player's sound term before the distance falloff,
  `Settings::noise` (the same function the detection value uses): footsteps by body
  armour weight, × 1.5 running unless Silent Running, shots × 2 by their sound level,
  all × 1.6; full at 100. Standing still is 0, a walk in light armour about a third, a
  run about half, any Normal or Loud shot fills it, and it falls back over 1.5 s.

They show what the player gives off, not whether anyone notices (the sneak word says
that); sneaking itself makes no less noise in the formula, only the Sneak skill counts.
Smoke `sneak-gauges`.
