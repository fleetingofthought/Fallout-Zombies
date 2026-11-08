# The player's body (`--play`): animated locomotion, weapons, first- and third-person views

Status 2026-10-09: on `claude/player-body` for review. Owner requests 2026-10-09:

1. Using the NPC assembly and animation playback (docs/design/static-actors.md parts B,
   C, E), a player character that moves with the animations, with the camera tied to the
   head; locomotion forwards, backwards and sideways; crouching, and moving while
   crouched; drawing and holstering weapons.
2. Then (owner decisions on the first version's questions): **match all the New Vegas
   controls**; **use the game's separate first-person arms model** ("with armours in
   first person, if the nif is not called arms it's not rendered"); **holding the middle
   mouse button circles the camera around the player** without turning them.

Where it sits: the player's own body is Layer 4 play (`nvf-play` owns the controls and
the camera) drawn with the Layer 3 actor assembly (`nvforever`); choosing clips for an
actor under direct control is the small, player-only part of Layer 6 animation
selection that this needs. NPC animation stays as it was (one idle clip, actors.md §7).
Facts: docs/spec-notes/animation.md §9 (clips, first-person rig), docs/spec-notes/
controls.md (bindings).

## 1. What the player sees

### 1.1 Two rigs

- **Third person**: the player's NPC_ (FormID 7, `fnv_sim::FormId::PLAYER_BASE`)
  resolved like a placed actor (`fnv_actor::Resolver::resolve_wearing`) on
  `characters\_male\skeleton.nif`: race body, head and FaceGen face, hair, eyes, wearing
  the armour the simulation has equipped on the player. With nothing worn the race's
  upper body shows: its skin shape with the tinted skin texture and its underwear shape
  (grey T-shirt, dark shorts) with its own texture (actors.md §3.1, §4.1). It stands at
  the character's
  feet, faces the view's heading, and carries no `RefVisual` (the crosshair never
  targets it). Shown in third person and while the camera circles the player.
- **First person**: the game's arms rig, `characters\_1stperson\skeleton.nif`, with its
  own clips (`characters\_1stperson\`). It draws the hands (their `…1st.nif` meshes) and
  only the `Arms…` shapes of the upper body and of worn upper-body armour, with the
  outfit's `PipBoyOn…` sleeve (gloves and the Pip-Boy whole; no head, hair or other
  armour; skin tinted as in third person; `fnv_actor::first_person`), and the weapon's
  first-person model (WEAP `WNAM`, a STAT). `Bip01 Looking` turns with the view's pitch,
  so arms and weapon stay in front of the camera, which sits at the rig's `Camera1st`
  node: 118 units above the feet, 78 crouched. Shown in first person.
- An outfit change in the inventory menu rebuilds both. Without the first-person
  skeleton the third-person body stands in (its head's eye gives the view and its head
  is hidden), as in the first version.

### 1.2 Controls (the game's, docs/spec-notes/controls.md)

| Control | Default | Here |
|---|---|---|
| Forward / Back / Slide Left / Slide Right | W S A D | the forward, backward, left and right clips; diagonals blend two clips; clips play at the speed the character actually moves |
| Run / Always Run | Left Shift / Caps Lock | running is the default (Always Run on); Shift held walks; Caps Lock swaps the default |
| Auto Move | Q | walks forward until Forward or Back is pressed |
| Jump | Space | jump loop in the air, landing clip after 0.35 s of falling |
| Crouch/Sneak | Left Ctrl | crouched idle and movement at `fMoveSneakMult` (0.57) of the speed |
| Ready Item | R | weapon away: draw it; weapon out: tap = reload (combat C4), hold 0.5 s = holster |
| Use | left mouse | weapon away: draws it; weapon out: the trigger (combat C4: guns, launchers, thrown weapons; melee is C6) |
| Block | right mouse, Left Alt | held: guns aim down the sights (`AimIS`), fists and melee block (`blockidle`) |
| Toggle POV | F, middle mouse | tap: first/third person; held while the mouse moves: the camera circles the player (not turning them) and swings back when let go; wheel zooms in third person |
| Activate | E | as before |
| Menu Mode | Tab | tapped, the Pip-Boy (docs/spec-notes/pipboy.md; the inventory when this was written); held 1 s, the flashlight on or off (`nvf-play` `flashlight.rs`, owner request 2026-10-11) |
| Ammo Swap | 2 | the next carried ammo type down the weapon's list, then a reload (combat C4) |
| Rest | T | the Wait menu, or the Sleep menu while lying on a bed (docs/spec-notes/wait-menu.md) |
| Vats, Hotkeys 1 and 3–8, QuickSave F5, QuickLoad F9 | | bound; pressing one shows "… : not available yet" |

Scripts can turn groups of these controls off and on (`DisablePlayerControls`,
`EnablePlayerControls`): `control::ControlLocks`, docs/spec-notes/controls.md §5.

Mouse look uses the game's `fMouseSensitivity` (0.002 radians per count) and
`bInvertYValues` (off). The F9 test aid that opened the nearest container moved to F10
(F9 is Quick Load).

### 1.3 Animation

Both rigs play layered clips (`nvf_assets::anim_blend`) chosen by
`fnv_actor::player_clips`, each from its own folder: an idle, movement, sneaking, swimming,
jumping and landing, turning on the spot, and the weapon drawn (equip clip; at its
`Attach` key the weapon model moves from its holster to the right hand), held (Aim; in
third person bent up and down with the pitch by `aimup`/`aimdown`), aimed down the
sights or blocking, and holstered (unequip clip; at `Detach` it goes back: third person
to the right hip for pistols and across the back for rifles, first person to
`Bip01 Translate`, out of view). Crouched, the crouched variants are used where they
exist. The draw/holster timing follows the clips of the rig in view.

### 1.4 Camera

- First person: at the first-person rig's `Camera1st` node, looking along the view.
- Third person: behind the player at the wheel's distance (60–450, 170 to start), offset
  by `fOverShoulderPosX` (40, right) and `fOverShoulderPosZ` (−10, up) from a pivot above
  the feet at the third-person head's eye height; pulled in front of collision.
- Circling (Toggle POV held, mouse moving): the third-person camera turned by the
  circling angles, the shoulder offset fading to the centre as it turns away from
  behind; from first person the view shows the third-person body meanwhile. Let go, it
  swings back behind (time constant 0.15 s) and first person returns.
- Detached (the settings page's Gameplay switch "Detached third-person camera", off by
  default; the owner 2026-10-27): in third person with the weapon away the mouse turns
  the camera, kept at its full shoulder offset, and not the player; moving, the player
  turns to the pushed way (a sprung turn that eases in and out, a half turn
  settling in about 0.6 s) as the camera sees it and walks straight ahead, so
  Back runs towards the camera. Drawing the weapon (or first person, or the switch off)
  turns the player to the camera's heading at once. The crosshair follows the camera's
  line meanwhile [design: not in the original; `control::detach_camera`].
- The crosshair's ray starts at the eye (`Player::eye`, the `Camera1st` position), so it
  matches the first-person view; in third person it still starts there [engineering
  choice; the game's third-person pick is not known].

### 1.4a Sway and bob of the arms (owner request 2026-10-20)

The original's first-person arms and weapon lag behind a turn and bob as the player
walks, but none of it is in the files: the first-person movement clips move only the
root (`mt`) or have keyless tracks (`1hp`), there are no `2hr`/`2ha`/`2hh` movement
clips in `_1stperson\locomotion`, and `Camera1st` hangs from `Bip01 Looking` beside the
arms, so a clip moving that chain would move the view too [observed: `fnvtool nif
decode` of the clips and `_1stperson\skeleton.nif`]. The executable names
`fFirstPersonHandFollowMult`, `fFirstPersonHandChaseSeconds` and
`fFirstPersonHandChaseSecondsAttack` [observed: its strings], so the hands chase the
view by code [hypothesis; the values are not known]. NVForever's sway
(`engine/nvforever/src/view_sway.rs`) [design, constants tuned by eye]:

- Turn lag: the arms keep their aim and catch the view up with a 0.11 s time constant,
  at most 5° sideways and 3.5° up or down, the weapon tilting a little into the lag.
- Bob: a dip at each footfall and one sideways swing a stride, at the phase of the
  movement clips (the footstep keys), growing with the ground speed to a run's
  (300 units/s): 0.9 units down, 0.6 across, 1.2° of tilt. Sneaking 60 %, down the sights
  20 % (the turn lag 35 %), none in the air.
- Starting, stopping and strafing leave the weapon a little behind (at most 1.2 units)
  and lean it into a strafe (1.5° at a run).
- It turns and moves `Bip01 Translate` about the eye in the view's frame, after the
  clips are blended, so the camera does not move; it fades out (0.15 s) under the
  Pip-Boy, while an idle drives the view and in third person.
- A switch, on by default: `--no-weapon-sway`, "Weapon sway" on the settings page (key
  `weapon-sway`). The exit summary gives the most turn lag and the deepest bob.

### 1.4a.1 The camera's bob: first person yes, third person no (owner, 2026-11-07)

The owner, from a report: "we had headbob in third person, but none in first person?
it should be the other way around". Before: the first-person camera sat on the arms
rig's `Camera1st`, which no first-person movement clip moves (§1.4a), so only the arms
bobbed; the third-person camera's pivot was the walking body's head bone, which rises
and falls with each stride. Now (`engine/nvforever/src/head_bob.rs`) [design, constants
tuned by eye]:

- First person: the whole arms rig (camera node, arms and weapon together) dips 1.6
  units at each footfall and swings 0.5 across once a stride, at the movement clip's
  phase, growing with the ground speed to a run's; sneaking 60 %, down the sights 25 %,
  none in the air, under the Pip-Boy or while an idle drives the view. The arms' own bob
  (§1.4a) comes on top. A switch of the settings page's Display group (`head-bob`,
  `--no-head-bob`), on by default.
- Third person: the pivot's height above the feet is the head's through two
  first-order lags of 0.25 s: a run's stride bob is cut to about an eighth, a crouch
  still lowers the camera (90 % in a second); a change over 40 units in a frame goes
  through at once. The exit summary gives the deepest first-person dip and the
  third-person pivot's frame-to-frame movement against the head's; smoke
  `head-bob-first-person`, `head-bob-third-person`.

### 1.4b The weapon lowered near walls and friends (owner request 2026-10-26)

The original keeps the first-person weapon up whatever is ahead, so it pokes through
walls and companions; its `_1stperson` clips have no lowered pose [observed: the owner's
play; the folder's file list]. NVForever lowers it by code
(`engine/nvforever/src/weapon_lower.rs`) [design, constants tuned by eye with the
owner, 2026-10-27]:

- Two rays ahead of the view, from the eye and from about where the weapon is held. A
  surface within 40 units eases it down, fully at 22; a living actor within 72 units
  that would not attack the player (`fnv_game::rest::hostile_to_player`, by its hit
  volumes) lowers it fully. A hostile actor never does.
- Lower Weapon (H, NVForever's own control, rebindable) lowers or raises it by hand
  (`nvf_play::control`). It was Ready Item tapped twice until 2026-10-27; the owner
  moved it to its own key because waiting 0.25 s for a second tap held every reload
  back, which matters when R is spammed in a fight.
- Aiming, the trigger and an attack bring it up at once and keep it up 0.6 s; a reload
  keeps it up for the reload clip's length. Aiming, the trigger and putting the weapon
  away also end a lowering by hand. Shots still leave from the view; firing is never
  blocked while lowered (owner ruling 2026-10-27).
- The pose turns the arms about the eye (pitched 9° down, turned 10° in, canted 5°), on
  top of the sway, so nothing near the camera rises into the view: turned about the
  hands, the sleeves swung up across it. Rifles take 80 % of it, heavy weapons and
  launchers 35 % (they sit at the foot of the view or on the shoulder). Not for fists,
  under the Pip-Boy, while an idle drives the view or in third person.
- A switch for the automatic part, on by default: `--no-weapon-lower`, "Lower weapon near
  walls" on the settings page (key `weapon-lower`); by hand works either way. The exit
  summary gives how many times it lowered.

Lowering is the cue near walls; it no longer has to keep the weapon out of them: the arms
rig and its weapon are drawn with their depth squeezed in front of the world, so no wall,
door or actor hides them (owner report 2026-10-27; [first-person-depth.md](first-person-depth.md)).

### 1.5 The body in first person (owner request 2026-10-10; not the original's)

A switch, off by default as the original draws the arms rig alone: `--first-person-body`,
and "Body in first person" on the settings page (key `first-person-body`).
[engineering choices throughout; nothing here is a claim about the game]

- On, first person draws the third-person body too, without its head parts (head, hair,
  headgear: the `HEAD_SLOTS` rule of the stand-in body), so looking down shows the
  chest, legs and feet in the outfit worn. The view is unchanged: the camera stays at the
  arms rig's `Camera1st`.
- The body is moved to the view, not the view to the body: each frame it stands where
  its eyes are under the camera and 4 units behind (`OWN_BODY_BACK`), feet on the
  ground, so the open neck stays out of the picture; crouched and moving it follows its
  own head. It stays upright whatever the pitch (the aim up/down clips are not blended
  in), as the arms rig alone follows the view.
- While the arms rig's arms are up (weapon out, being drawn or put away) the body's own
  arms, hands, gloves and Pip-Boy are hidden (the shapes the arms rig draws,
  `fnv_actor::first_person::shape_shows`), and its weapon while it is in the hand. With
  the weapon away the body's arms hang at its sides and its weapon is in its holster.
- On furniture the seated body is drawn without its head and the view is its eye, as
  before, and (owner decision 2026-10-10: "limited to a real person's head movement")
  the view turns at most 80° to either side of the way the body faces (`NECK_TURN`)
  and tilts at most 75° down (`NECK_DOWN`); a view further round when sitting down
  comes round at 4 rad/s, and the body is drawn once it has. The way the body faces is
  its pelvis's (`Bip01 Pelvis`, its turn about the vertical since the bind pose): the
  clips of getting on and off turn the body inside the rig, and the view turns with it
  (owner report 2026-10-10: sitting down, the camera "didn't move in line with the rest
  of the body"). The eyes go round the neck (`Bip01 Head`) with the view, as a turned
  head's do. With the switch off the seated view is as it was: free, not turned by the
  body, and no body.
- While an idle drives the first-person view (its camera node places the view and mouse
  look is ignored: the opening's wake-up in Doc Mitchell's house) the body is not drawn,
  only the arms rig playing the idle, as with the switch off (owner report 2026-10-16:
  with the switch on, loading in showed "the players own body standing over
  themselves"). The body casts its shadow still, from where it stands.
- On a bed (owner requests 2026-10-10: lie down as a sleeping NPC does, "sideways the
  camera when laying") the view is the lying body's, with the switch on or off
  (`lying_view`). That code does not assume an upright body: the frame is the pelvis's
  whole turn since the bind pose (`bone_turn`), not only its turn about the vertical.
  The view keeps its turn and tilt in that frame while the body lies down and gets up,
  so the picture leans with the body (`Player::body_view`: the camera takes a whole
  rotation, not a heading and pitch, as it does for an idle that turns the view) and the mouse moves the
  head in the frame (`PlayerSeat::look`). Limits: the seated 80° and 75° about the
  body's own axes, and no more than 10° below level once lying (`LYING_DOWN`), as lower
  is the pillow. With the switch on the eyes go round the neck about the body's length.
  The player's heading and pitch are set to the level view nearest the leaning one.
  In third person the pivot is lifted up to 40 units (`LYING_PIVOT_UP`) so the camera
  clears the bed. Facts and what was seen: docs/spec-notes/furniture.md §7.
- The shadow is the whole body's, head and arms included: §1.6.

Checked 2026-10-10 on the owner's install [observed in NVForever: screenshots kept
privately], Goodsprings, Vault 21 jumpsuit `00104184` and varmint rifle `0007EA24`:
looking 88° down with the weapon away shows the chest, belt, both hands at the sides
and the boots; with the rifle out, the arms rig over the legs; sneaking and running
forward; seated on the saloon porch chair `00106349`, the lap and forearms ahead, the
shoulder and arm at either limit, a view that starts behind comes round, and through
the sitting-down clip the view turns with the body from facing the chair to facing away
from it.

### 1.6 The player's sun shadow (owner request 2026-10-10; not the original's)

[engineering choices throughout; the original casts no sun shadows from the world]

Before: in first person the arms rig and its weapon cast a shadow of floating arms and
there was no body shadow; with the body in first person (§1.5) the shadow had no head,
and no arms of its own while the weapon was out.

Now, in every view, the third-person body casts the player's shadow whole (head, arms
and hands, weapon), and the arms rig and its weapon cast none on the world. What the
camera sees is as it was.

- **Unseen but casting.** A hidden entity casts no shadow [documented: bevy_light 0.19.1
  `check_dir_light_mesh_visibility` skips entities whose `InheritedVisibility` is false,
  and takes those whose `RenderLayers` meet the light's]. So the parts of the
  third-person body the camera is not to see are no longer hidden: they are drawn on
  render layer 2 (`sky::SHADOW_LAYER`), which the sun casts from (the sun has layers 0
  and 2) and no camera has. The body's root is never hidden. `player_body.rs` `Seen`
  says what the camera sees of the body (all of it in third person; none in first
  person; with §1.5 all but the head, and but the arms and the weapon in the hand while
  the arms rig's are up; seated, as §1.5), and `show_rigs` puts each drawn entity on
  layer 0 or 2 when that changes.
- **The arms rig casts nothing on the world.** Its meshes and its weapon's are on render
  layer 3 (`sky::ARMS_LAYER`), which the main and the reflection camera have and the sun
  does not.
- **The arms rig is not shaded by the unseen body.** That body stands in the middle of
  the arms rig, with its own arms and weapon about where the arms rig's are, and in the
  sun's shadow map it shades them [observed in NVForever, first attempt: the Pip-Boy,
  the left hand and the rifle's stock came out darker at every heading tried]. So the
  arms rig takes its sun shadows from a second light, `ArmsShade`: a child of the sun
  (same direction), with the sun's colour at 1/1024 of its strength
  (`ARMS_SHADE_SHARE`), casting from layers 0 and 3 with one cascade of its own, 256 units around the camera
  (`ARMS_SHADE_REACH`, since #254; before, the sun's first cascade).
  Its shadow map therefore holds exactly what shaded the arms before this change: the
  world, the seen parts of the body, and the arms themselves. The arms rig's materials
  are copies with `NifSurface::shade_light` set; their fragment shader
  (`nif_material.wesl` `with_shade_light`) reads both shadow maps and works the lit
  colour out from two results of Bevy's lighting, with and without shadows (Bevy's
  lighting has no opening for another shadow value; the small share of light is what
  makes the sun's part of the light measurable that way). Since 2026-11-02 (#1374) the light draws only
  the casters whose bounds come within 64 units of the line from the eye 96 units toward
  the sun, and no land (`near_shade_casters`, `terrain::LandMesh`; so steep ground within the
  24-unit body clearance, crouched against a cliff with the sun behind it or the lying
  view, no longer shades the arms [engineering choice]): Bevy keeps every
  caster between a cascade and the light, so at a low sun the whole column toward the
  sun, hills and buildings far off, was drawn into a map as large as the sun's, up to
  3.6 ms a frame at Novac at 07:00; a cell's land box spans its hills and holds the eye
  wherever the player stands. What shades the arms from further off comes from the
  sun's own map (`nif_material.wesl` `shadow_past_body`), read at a depth moved toward
  the sun past the plane through the eye and 24 units more (`BODY_CLEARANCE`), so the
  unseen body round the eye is left out; the arms take the darker of the two. The sun's
  cascades reach 256 units further toward the sun for it (`sky.rs` `SUN_REACH_BACK`,
  `reach_toward_sun`): Bevy puts a cascade's near plane at the view slice's corner
  nearest the sun, just in front of the eye when the sun is behind the camera, and every
  caster before it lands on that plane, the body and a wall behind it alike. Measured
  [observed 2026-11-02, RTX 4070, 1920x1080, finer shadows, Novac 07:00, interleaved
  runs]: main 15.4 ms, now 12.4 ms, without the light 11.4 ms; the light's pass costs
  about 0.75 ms whatever it draws (one caster as three), 0.25 ms when it draws nothing
  (owner-review item 43). The light is there only while
  the arms rig is drawn and the sun casts shadows, and is despawned otherwise, never
  hidden: a hidden directional light keeps its shadow views in Bevy 0.19.1's render
  world and gets a second set when shown again, and the doubled views upset the shadow
  passes' shared indirect draw buffers, so the sun's cascades lost or gained whole
  batch sets from frame to frame [observed 2026-10-26: hillsides flickering between
  lit and shadowed in first person, worse the more often the arms had been hidden;
  gone with GPU culling off, without the light, or with it despawned; the shadow probe
  now counts views sharing a key and the `shadow-probe` smoke scenario switches view
  four times and wants none].
- **Cost.** While the arms rig is drawn: one more shadow cascade to render, one more
  directional light in every fragment's lighting, and a thousandth more sunlight on the
  world. Frame rates of 18 s runs before and after overlap (before 44.5 and 26.2 per
  second, after 38.9 and 38.6, on a machine busy with other work), so the cost was not
  measured.
- Point and spot lights cast no shadows (`lights.rs`). If one ever does it needs the
  sun's layers, or the player casts no shadow from it in first person.

Tried and dropped: (1) leaving the arms rig on the sun's map (the result above); (2) a
material that skips casters within 90 units of the eye: standing in tight shade (inside
the truck cab by the Goodsprings road) the arms stayed sunlit; (3) a shade light without
light of its own and an arms rig that casts no shadow at all: the arms lose the shadows
they cast on themselves.

Not built: in the water's reflection the first-person view still shows the arms rig
rather than the body, as before. With §1.5 on and the weapon out, the shadow's arms are
the body's own (upright, aiming level), not the arms rig's (which follow the pitch).

Checked 2026-10-10/11 on the owner's install [observed in NVForever: screenshots kept
privately, `C:\fnv-private\references\player_shadow\`], Goodsprings at 17:00 facing
east, jumpsuit `00104184` and varmint rifle `0007EA24`, before and after:

| View | Before | After |
|---|---|---|
| First person, weapon away | two arm shadows, no body | a whole body with its head |
| First person, weapon out | arms and rifle floating, no body | a whole body holding the rifle |
| Body in first person, weapon away | a body without a head | a whole body |
| Body in first person, weapon out | headless, armless body and floating arms | a whole body holding the rifle |
| Third person | whole body | the same (under 220 of 1,024,000 pixels differ) |

The arms and weapon themselves, in the part of the picture the shadow does not reach:
4, 49 and 152 pixels differ by more than 24 of 255 facing east, north and west, of
230,400. Standing in the truck's shade the arms are shaded as before. Seated on the
porch chair `00106349`, with and without §1.5, the picture is as before. On the Lake
Mead pier (cell 13,9) the water and its reflections differ only by the ripples.
Switching to third person and back keeps the right parts seen.

**Flicker after leaving an interior** (owner's playtest, 2026-10-11: "strange flickering on
shadows noticed 2 times after exiting interior cell", first person only). Not reproduced
[observed, owner's install, 2026-10-11]: `--play --cell GSProspectorSaloonInterior
--no-npc-packages --hour 15`, `NVF_PLAY_TEST_NEAR=0010618E NVF_PLAY_TEST_ACTIVATE=10`
(the front door), then bursts of 20–40 frames 0.05–0.1 s apart right after the loading
screen: on the porch, walking off it and standing in the sun with the .357 out. Frames of
a still view differ by at most a few hundred pixels (the idle's sway); no shadow comes
and goes. With `RUST_LOG=nvforever::player_body=debug` the arms shade light, the
first-person rig and what the camera sees of the body never switched through the door
(debug lines "arms shade light on/off", "first-person rig shown/hidden", "the camera sees
… of the body", kept for the next look). What the owner should say: the hour, which door,
whether "Body in first person" was on, and what flickered (the arms, the ground, the
body's own shadow).

## 2. Code

| Item | Where | Notes |
|---|---|---|
| Clip choice (headless) | `crates/fnv-actor/src/player_clips.rs` | candidate files per situation, covering the third- and first-person folder layouts; direction and aim weights; block clips; text keys (`ClipKeys`); the draw/holster state machine (`WeaponHands`). NPC locomotion and NPC weapons are `locomotion.rs` and `weapon.rs` (workstream 5 parts G and H); the weapon-type prefix is theirs (`weapon::AnimType::prefix`) |
| First-person parts (headless) | `crates/fnv-actor/src/first_person.rs` | which parts show, `…1st.nif` hands, the `Arms` shape rule |
| WEAP models | `crates/fnv-actor/src/records.rs` (`ActorForms::weapons`, part H's record) | adds WNAM and its STAT's MODL |
| Worn-armour override | `crates/fnv-actor/src/resolve.rs` (`resolve_wearing`) | `resolve` unchanged for NPCs |
| Layered playback | `engine/nvf-assets/src/anim_blend.rs` (`Animator`) | priority blending per bone, fades, a shared phase for movement clips, once/loop modes, model-space bone transforms |
| Clip data | `engine/nvf-assets/src/anim_play.rs` (`Clip`) | track priorities, text keys, `root_speed()` |
| Bindings | `engine/nvf-play/src/bindings.rs` | every `[Controls]` control with the game's default key and mouse button; `ControlInput` |
| Controls and camera | `engine/nvf-play/src/control.rs` | `PlayerControl` (sneak, weapon, Block, Always Run, Auto Move, view, distance, `Orbit`), `ControlSettings` (GMSTs), `BodyView`, `place_camera` |
| Movement keys, mouse look | `engine/nvf-play/src/player.rs` | movement through the bindings; mouse look turns the orbit while Toggle POV is held; `Player::body_eye`, `BodySet` |
| Body | `engine/nvforever/src/player_body.rs` (`PlayerBodyPlugin`, with `--play` and actors on) | both rigs, weapon models, clip caches, per-frame clip choice, eye and pivot, which rig is shown; the body in first person (§1.5: `PlayerBody::first_person_body`, `Seen`, `seated_heading`, `lying_view`); the shadow (§1.6: `Seen`, `show_rigs`, `ArmsShade`) |
| Shade light for the arms | `engine/nvf-assets/src/nif_material.rs` (`NifSurface::shade_light`), `nif_material.wesl` (`with_shade_light`); layers in `engine/nvforever/src/sky.rs` (`SHADOW_LAYER`, `ARMS_LAYER`) | §1.6 |
| Shared part building | `engine/nvforever/src/actors.rs` (`part_uses`) | NPCs and the player build parts the same way |
| Test aids | `engine/nvf-play/src/test_aid.rs` | `NVF_PLAY_TEST_EQUIP`, `NVF_PLAY_TEST_MOVE`, `NVF_PLAY_TEST_CONTROL` (`sneak`, `weapon`, `aim`, `view`, `jump`, `zoom=`, `orbit=<yaw>:<pitch>`, `release`); for a session played by hand, the debug switches `NVF_PLAY_DEBUG_GIVE` and `NVF_PLAY_DEBUG_EQUIP` (same values as the `TEST` ones, the run stays attended) |

Interface changes in `nvf-play` (coordinator, 2026-10-09): `Player::body_eye`
(`eye()` returns it when set); `place_camera` in `control.rs`; resources `Bindings`,
`PlayerControl`, `ControlSettings`, `BodyView`; set `player::BodySet`;
`activate::HudMessage` is public (other systems show lines with it); Activate (E) and
Menu Mode (Tab) are read through the bindings.

## 3. Rules and values [hypothesis unless tagged]

- Clip choice and fallbacks: animation.md §9 and `player_clips.rs`. Weapon-type movement
  clips are used while the weapon is out and exist for that direction; otherwise the
  unarmed movement clip with the aim pose over it. Female players use
  `locomotion\female\` (male clips as fallback).
- Blending: priority first, then weight (animation.md §9.4); fades from `Blend:N` keys
  (N/30 s), 0.2 s without one, 0.2 s out. The accumulation root is held at the identity:
  root motion is the character controller's.
- Aiming up and down (third person, animation.md §9.6): the level Aim clip always plays
  and shares the bones it has in common with `aimup`/`aimdown` by the pitch; an attack
  plays with its own up and down forms at the same weights. Bones only the level clip
  drives (a rifle's arms) stay with it.
- First person: `Bip01 Looking` turned about the actor's right axis by the pitch;
  no aim up/down blend there.
- Weapon placement: the `Weapon` node is re-parented to the holster bone or the right
  hand; the weapon model's shapes hang from it as in a placed reference.
- Engineering choices: jump loop after 0.15 s in the air, landing after 0.35 s; turn
  clips above 1 rad/s; third-person distances and the 8-unit wall margin; the orbit's
  return time and shoulder fade; aim up/down weights linear to ±90°; the 0.5 s holster
  hold.

## 4. Validation (2026-10-09, owner's machine, Goodsprings)

Unattended runs with the test aids [observed: logs and screenshots kept privately]:

- First version (third-person body as the only rig): pistol (`000E3778`) drawn at the
  `Attach` key, `1hpfastforward.kf` under `1hpaim.kf` at clip rate 0.97 (343 units/s),
  strafing, running backwards, crouching (`sneakmtidle.kf`), crouched running at 196
  units/s (197.7 measured in game), holstered to `Bip01 Pelvis` at `Detach`; varmint
  rifle (`0007EA24`) holstered across the back (`Bip01 Spine2`); jump loop and landing.
- Arms rig: built from `upperbody.nif` (`Arms:0`, `Arms:1`), `lefthand1st.nif`,
  `righthand1st.nif` on the 74-node first-person skeleton; the eye 118.0 above the feet
  standing and 78.0 crouched; the 9mm pistol (WNAM `1stPerson9mm`) held in the right hand
  in view, following the view looking down 50°; the varmint rifle held in both hands
  (`_1stperson\2hraim.kf`) and down the sights (`2hraimis.kf`); the rifle on
  `Bip01 Translate` while holstered (not in view).
- Circling: `orbit=150:-10` shows the crouched player from the front-right in third
  person without turning them; after `release` the camera is back behind.
- Checks: workspace tests and clippy (root and engine), the Python suites, the
  moving-camera smoke test (CLAUDE.md) — see the commit messages for the runs.
- The Pip-Boy (2026-10-10, owner report: it "looks off" on the arm, and its screen
  glows): the model hangs from `Bip01 L ForeTwist` by `Prn` and was given the rotation
  that head parts need, which put it on the back of the upper arm; and its light's glow
  shape was always drawn (actors.md §1.2). After the fix [observed: screenshots circling
  the player with `orbit=90:-5`, `-150:-10`, `30:-10`, bare and in a Vault 21 jumpsuit
  `00104184`]: a cuff on the left forearm from below the elbow to the glove, screen
  outwards and dark, the three button lights lit; in first person with the varmint
  rifle up, on the left forearm in view (it was not drawn there before).
- Rifle hands looking up and down (2026-10-10, owner report with a game screenshot:
  with a two-handed weapon the hands drift off it the further up or down the view
  goes, from the hip and down the sights): the blend gave the arms to the idle as the
  level Aim clip's weight fell (animation.md §9.4, §9.6). After the fix [observed:
  screenshots from the side with `orbit=90:<minus the pitch>`, varmint rifle
  `0007EA24`]: both hands on the rifle and the rifle along the view at 45° and 80° up
  and down, down the sights at 60° up and down, and while firing at 60° up and 45° down
  (`2hrattack8.kf` with `2hrattack8up.kf`/`down`). Walking or running with the rifle
  out the body does not bend with the pitch (open question 5).
- Minigun idle (2026-10-10, owner report with screenshots: right while firing at any
  pitch, but idle the left hand is too high, off the handle): `2hhaim.kf` does not
  drive the `Weapon` node and ours put it back at its bind pose (animation.md §9.7).
  After the fix [observed: screenshots from the front-left with `orbit=120:<minus the
  pitch>`, minigun `0000433F` and 9mm pistol `000E3778`]: idle, level and 50° up and
  down, the left hand is on the top handle as while firing, the same after firing
  stops and while running; the pistol is held as before.
- Minigun holstered (2026-10-10, owner question, then game screenshots: holstered only
  the backpack shows, on the back; drawn the gun is in the hands and the backpack
  stays): the weapon model's parts follow the clips' visibility tracks, and the
  `Backpack` node hangs from the bone its `UPB` string names (animation.md §9.7).
  After [observed: screenshots from behind, from the side and from the front with
  `orbit=90:0` and `orbit=150:0`]: as in the game screenshots; the gun appears half way
  through the draw and goes half way through putting it away.
- Moving parts (2026-10-10, owner report: no weapon's parts moved): the weapon model
  was drawn as loose shapes, so the clips' tracks on its `##` nodes had nothing to
  drive. It now keeps its node tree and the clips move its parts by name; an attack
  clip with a `Loop` key keeps going while the trigger is held (animation.md §9.8).
  After [observed: first-person screenshots of the 10mm pistol, and the body's debug
  lines]: the slide goes back at the shot, the magazine leaves the grip in the left
  hand during the reload, and the minigun's barrel turns for as long as it fires.

## 5. Open questions for the owner

1. Are the bindings in docs/spec-notes/controls.md §2 the game's defaults (they are your
   `FalloutPrefs.ini`'s)? Should NVForever's own config allow rebinding?
2. Toggle POV held in the game: does the camera stay where it was circled to after
   letting go? Does holding it in first person zoom out instead?
3. The Pip-Boy (drawn on the left forearm in both views since 2026-10-10, actors.md
   §1.2). Answered 2026-10-10: the three orange button lights are always lit; the
   outfit's `PipBoyOn` sleeve shows in first person too (now drawn). Open: (a) is the
   screen dark with the light off, as drawn now? (b) The light itself was not built then (the
   glow shape was hidden); holding Tab now switches a light (`nvf-play` `flashlight.rs`,
   2026-10-11).
4. In-game references would settle the hypotheses of animation.md §9 (captures listed
   in its §7): holster positions, aim poses, sneak speeds sideways and backwards.
5. Third person with a rifle out, walking and running while looking steeply up or down:
   does the body bend so the rifle follows the view, as it does standing? Ours keeps
   the rifle level while moving (animation.md §9.6, owner-review.md).
