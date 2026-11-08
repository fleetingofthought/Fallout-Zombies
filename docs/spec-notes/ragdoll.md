# Spec notes: ragdolls

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525) with
`fnvtool collide ragdoll` over the eleven archives that hold meshes (collision.md lists
them), **[hypothesis]** needs verification, **[design]** is our own choice (ragdoll
behaviour is presentation of a death, like the rest of combat: docs/design/combat.md §1).

Block layouts are in `schemas/nif/havok.json` (re-expressed from niftools nif.xml, read as
documentation). Reader: `fnv_collide::ragdoll`. Physics: `fnv_phys::ragdoll`. In the game:
`engine/nvforever/src/ragdoll.rs`, `deaths.rs`.

## 1. Where a ragdoll lives

- A skeleton (`skeleton.nif`) carries a Havok **body** on some of its bones: the bone's
  node names a `bhkBlendCollisionObject` whose `body` is a `bhkRigidBody` with a shape
  (collision.md §1) [observed]. `characters\_male\skeleton.nif` has 18, all capsules, on
  the layer BIPED (8), stored with the motion system KEYFRAMED (6): `Bip01 NonAccum`
  (the pelvis), `Spine`, `Spine1`, `Spine2`, `Neck1`, `Head`, and per side `Thigh`,
  `Calf`, `Foot`, `UpperArm`, `Forearm`, `Hand`. Their masses add up to 81 [observed].
- A body lists **constraints** (`constraints`, block references) [documented: nif.xml
  bhkRigidBody]. A constraint names two bodies, `entity_a` and `entity_b`
  [documented: nif.xml bhkConstraintCInfo]. The constraint is listed by its `entity_a`,
  the body being attached; `entity_b` is the body it is attached to (towards the
  pelvis) [observed in the human skeleton; documented for the order: the GECK wiki page
  BhkRagdollConstraint, "the first is the rigid body you wish to attach and the second
  is the rigid body you wish to attach to"]. The human skeleton's 17 constraints make a
  tree rooted at the pelvis.
- 224 meshes have constraints: 1,823 bodies and 1,456 constraints. 73 are skeletons
  (51 named `skeleton.nif`, 957 bodies). The rest are ground models of armour, traps,
  hanging lights, gore, clutter skeletons [observed].
- Constraint blocks met: ragdoll 771 (63 wrapped in a malleable constraint), limited
  hinge 677 (80 wrapped), hinge 4, prismatic 3, stiff spring 1 [observed]. A
  `bhkMalleableConstraint` holds one constraint of another type and a `strength`
  (0.9 wherever skeletons have one) [documented: nif.xml; observed]. Two turret skeletons
  list a `bhkOrientHingedBodyAction` among their constraints; it is not a constraint and
  is left out [observed].
- Ragdoll Template files (`.rdt`), which override bodies and constraints per armour
  piece, are in §6. Not read: the RGDL record a body part data's `RAGA` names
  (combat-data.md).

## 2. Frames and units

- A body's frame is its **bone's frame** (for a bhkRigidBodyT, composed with the body
  transform, as for shapes, collision.md §4). Constraint pivots are points in the two
  bodies' frames in Havok units (× 7 × the node's scale for game units, collision.md §2).
  Placed by the bind pose, the two pivots of a constraint meet: the distance between
  them has a median of 0.004 game units over all 1,456 constraints, 0.010 over the 239
  on bodies with a body transform, and is under 2.6 units for 95 % [observed]. The larger
  gaps are bones whose bind pose differs from the pose the constraints were made in
  (the human arms: 1.1 to 2.2 units).
- The body's stored translation and rotation are its pose in the mesh root's frame when
  the file was made: in the human skeleton, translation × 7 is the bone's bind position
  to 0.01 units [observed]. They are not used to place anything.
- Constraint axes are unit vectors in the body's frame.

## 3. Constraint limits

### Ragdoll constraint (bhkRagdollConstraint)

Fields [documented: nif.xml bhkRagdollConstraintCInfo]: per body a `twist`, a `plane`
and a `motor` axis and a `pivot`; `cone_max_angle`, `plane_min_angle`, `plane_max_angle`,
`twist_min_angle`, `twist_max_angle` (radians), `max_friction`.

- In the bind pose body a's twist axis lies along body b's, and the plane axes likewise
  (dot products 0.94 to 1.00 in the human skeleton) [observed]. `motor` is
  twist × plane in both bodies [observed].
- The twist axis runs along the bone (the thigh's points down the leg) [observed].
- Reading of the limits [documented in outline: GECK wiki BhkRagdollConstraint: a twist
  range about the twist axis, "a cone of allowed movement, and a second set of cones
  (referred to as planes) to inhibit said movement"]:
  - **cone**: the angle between a's twist axis and b's is at most `cone_max_angle`.
  - **plane**: the angle by which a's twist axis leaves the plane square to b's plane
    axis, positive towards that axis, stays in [`plane_min_angle`, `plane_max_angle`]
    [hypothesis for the sign and for which body's axes]. Under this reading the human
    upper arms (b's plane axis points backwards; limits −1.31 to 0.09) swing 75° forward
    and 5° back, and the left thigh (plane axis to the right; −0.88 to 0.52) 50° outwards
    and 30° inwards; the opposite sign would give arms that swing backwards [observed
    values; the anatomy is the argument]. The bind poses do not settle it: 603 of 771
    are inside their limits with this sign, 601 with the other [observed].
  - **twist**: the turn of a against b about the twist axis stays in
    [`twist_min_angle`, `twist_max_angle`] [hypothesis for the sign].
- The right thigh's plane limits are symmetric (−0.52 to 0.52) where the left's are not
  [observed; an authoring slip by the look of it, used as stored].

### Limited hinge (bhkLimitedHingeConstraint), hinge

Fields [documented: nif.xml]: per body an `axis`, `perp_axis_in_*1`, `perp_axis_in_*2`
and a `pivot`; `min_angle`, `max_angle`, `max_friction`. `perp2` is axis × `perp1` in a
[observed].

- The hinge angle is the angle of a's `perp1` from b's `perp1`, positive about b's axis
  (towards b's `perp2`) [documented in outline: GECK wiki BhkLimitedHingeConstraint,
  perp 1 "defines the zero angle", perp 2 "the positive direction of rotation";
  observed: with this sign 635 of the 677 limited hinges have their bind pose inside
  their limits (2 % slack), with the opposite sign 579; all eight of the human
  skeleton's (knees 1.22 in −0.87..1.52, elbows −0.07 in −2.27..0) only with this one].
- A plain hinge has no limits.

### Others

- Ball and socket: two pivots. Stiff spring: two pivots and a length. Prismatic: read,
  not converted (held as a ball joint); none is on a body an actor's ragdoll uses except
  the Protectron's two [observed].
- `max_friction` on skeleton constraints is 100 (590), 20 (164), 40, 2, 5, … [observed].
  Its unit is not known; it is not used (§4 damping stands in).
- Bodies: `linear_damping` 0.1 (most) and `angular_damping` 0.05; friction 0.3 (847 of
  957), up to 10 on a few (the human pelvis); restitution 0.8 [observed]. The GECK wiki
  says of restitution "near nonfunctional in New Vegas" [documented: Ragdoll Template].

## 4. What NVForever does [design unless tagged]

**When.** An actor that dies becomes a ragdoll at once when its skeleton has at least
two bodies and a joint and the physics cell under it is loaded: its animation stops and
the bodies start at the bones' current poses. A dead actor whose ground is not loaded, or
that is already dead when it is drawn, shows the death pose as before (combat-rules.md)
until the ground is there, then its ragdoll starts from that pose. Skeletons without a
ragdoll keep the earlier presentation. [hypothesis: the original also drops placed
corpses when their cell loads.]

**Bodies** (`PhysWorld::add_ragdoll`). One dynamic body per ragdoll body with the stored
shape (scaled by the actor's scale), mass and damping; friction capped at 1; no bounce;
continuous collision detection. Colliders carry `Owner::Actor` on DEAD_BIP with the body
part (BPTD index) of their bone, so shots, the crosshair and **Search** find the corpse
on its ragdoll; the actor's kinematic hit volume is removed while the ragdoll is in the
world.

**Joints.** Body 1 is the constraint's b, body 2 its a; the joint frames are
(twist, plane, twist × plane) or (axis, perp1, axis × perp1) at the pivots.

- Ragdoll → a ball joint limited about x to the twist range, about z to the plane range
  cut to ± the cone, about y to ± the cone. rapier limits the components of the relative
  rotation's quaternion: equal to §3's angles for a turn about one axis, an
  approximation of the cones otherwise.
- Limited hinge → a revolute joint with the angle range; hinge → without.
- A range of zero width locks the axis (the human neck's twist). Limits are kept within
  ±3 radians.
- Every free angular axis is damped (`JOINT_DAMPING`, 6 per second) so limbs do not
  swing like chains; it stands in for `max_friction`. `strength` is not used.
- Tests (`crates/fnv-phys/tests/ragdoll.rs`) hold an arm on a hinge and on a ragdoll
  joint under gravity and check it stops at the stored limit on the side §3 says.

**Contacts.** With terrain, references and other ragdolls. Not with characters: the
player walks through corpses as through hit volumes. Two bodies of one ragdoll collide
only when no constraint joins them and their shapes are more than 1 unit apart in the
bind pose (an arm lies on the chest, not in it; neighbours that overlap by construction
do not fight their joints). Explosions push ragdoll bodies like other loose bodies.

**Corpse against corpse** (#910). Two *small* ragdolls of different actors pass
through each other [design; hypothesis: whether the original lets corpses pass through
each other is not known]. Small means the bind-pose shapes, actor scale included, fit
in a 64-unit box (`fnv_phys::ragdoll::SMALL_RAGDOLL_SPAN`): rats, geckos, radroaches,
bloatflies, mantis nymphs [hypothesis: sizes not measured creature by creature]. A small
corpse still meets the terrain, references, loose objects and every bigger corpse, and
two big corpses (people, dogs and up) still pile up, so no visible sinking where the eye
would notice it. Why: a heap of 20+ small corpses in one spot made the solver work on
their contacts for the 3 s before they are held at rest. Measured (test
`small_corpses_pass_through_each_other_big_ones_pile_up`, 25 three-ball chains dropped
on one spot, 3 s): 61 touching pairs between corpses at the peak and 0.38 ms a step
before, 0 pairs and 0.09 ms a step after; the same chains at 3× size (84 units) keep
colliding (57 pairs at the peak).

**Hits.** A shot or melee blow on an actor sends a shove (`nvf_play::combat::ActorPush`):
the ragdoll body nearest the hit point gets an impulse there along the shot (140 units
per second for a shot, 220 for a blow, × 1.6 for a power attack). The killing hit's shove
is the ragdoll's first.

**Bones.** After each physics step the bodies' poses are written to their bones. Bones
without a body (fingers, clavicles, twist bones, the weapon node) keep the pose they
died in under their parent. The actor's own transform stays where it died.

**Rest.** A ragdoll is at rest when the physics engine puts its bodies to sleep, or when
its bones all stay under 12 units per second and 0.6 radians per second for 1.5 seconds
(joints resting on their limits can keep a corpse creeping below what the eye sees). The
speeds are each bone's net motion over windows of 0.25 s (how far it moved and how far it
turned from the window's start to its end, over the window's length), not the bodies'
velocities [design, 2026-10-21]:

- [observed 2026-10-21] Dropping the human skeleton's ragdoll (`_male\skeleton.nif`, 18
  bodies) 100 times on flat ground from random heights, tilts, headings and starting
  velocities (a scratch program on `fnv_phys` with the shipped skeleton, not kept), the
  velocity rule set 77 of them at rest within 20 s; the other 23 had a body (most often
  a forearm, hand, upper arm or the neck) whose velocity flipped from step to step at up
  to 8 radians per second, while over the next whole second no bone moved more than
  1.4 units or turned more than 0.14 radians. Read as net motion over 0.25 s windows all
  100 came to rest, in 3.2 s on average and 8.5 s at most. Neither turning off the
  self-collision pairs nor doubling the solver's iterations nor changing the joint
  damping stopped the shiver [observed, the same program].
  [hypothesis] The shiver is the solver alternating between a ground contact and a
  joint limit (or the damping motor) on a light body; the hold at rest ends it.
- The dead player's ragdoll (smoke `death-cam-first`, `death-cam-third`) was one of
  these: on main it failed to rest in 2 of 2 runs, its left forearm and upper arm
  breaking each calm spell at up to 9.7 rad/s while the trunk lay still; with the window
  rule it rested 4.4 to 7.7 s of game time after the death in 6 of 6 runs (3 of each
  scenario) [observed 2026-10-21].
- A ragdoll still moving at exit logs its fastest bone's speed and spin over the last
  window, the longest it stayed calm, and the bones that ended its calm spells (how
  often, and how fast).

At rest it is **held** (`PhysWorld::set_ragdoll_at_rest`): its bodies become fixed, so it
lies still, costs nothing, and still stops rays and loose objects. A hit's shove or a
blast within reach makes it a ragdoll again. Roused by a hit's shove alone it moves for
at most 1 s before it is held again (`MAX_AWAKE_SHOVED`), not 3 s: a shot's shove has
fallen back within half a second [design: owner-review item 51, 2026-11-08; a cost guard
for battles, where corpses are shot again often]. Walking into it or rolling a barrel onto it
does not.

**Memory.** The bone poses of a ragdoll at rest are remembered by reference. A ragdoll
whose ground unloads, or whose actor is no longer drawn, leaves the world and is
remembered the same way; put back later it is held in that pose. The memory lasts for the
run (saves are not built yet). A corpse whose reference a script disables or deletes
(ravens do) leaves the world and is forgotten.

**Guard.** A corpse more than 150 units under the terrain is put back 20 above it, at
most three times.

**Checked** [observed 2026-10-10, `nvforever --cell Goodsprings --play` driven by the
test aids (`NVF_PLAY_TEST_FACE`, `aimnpc`, `fire`, `aimcorpse`), exit 0, "run finished";
screenshots kept privately]:

- The Goodsprings Settler (18 bodies, 17 joints), killed by the eighth 9mm hit at 110
  units: its knees buckle, it is on its back within a second and at rest a few seconds
  later, limbs inside their joints' ranges, the rifle still on its back. The crosshair
  offers **Search** on the body.
- Walking 13,500 units away (its physics cell unloads) and back: the body is put back at
  rest in the same pose.
- Three shots into the resting body: each shoves it a little; it comes to rest again.
- A Raven (3 bodies, 2 joints) shot on its perch drops to the ground and lies there;
  its script deletes it some seconds later and the ragdoll leaves the world.
- Exit report of the longest run: "ragdolls: 1 started, 1 put back at rest, 3 came to
  rest, 4 shoved by hits, 0 lifted back above the terrain; 1 in the world at exit (1 at
  rest)".
- Found on the way: changing a body's type in place (dynamic to fixed and back) loses
  the change when a scene query runs before the next step (rapier's collision-only
  update takes the change flags; the same trap as physics.md describes for new bodies),
  and a shot's impulse off the centre of a small body spun it through the terrain. Hence
  held bodies are built anew, and a shove sets velocities with a bounded spin
  (`MAX_PUSH_SPIN`).

Ragdoll Templates of worn armour are laid over the skeleton's data when the ragdoll is
built (§6). Not built: joint motors and `strength`, weapons dropping from dead
hands, knockdown of living actors (`PushActorAway`, explosions' Knock Down flags),
dismembered limbs, pushing corpses by walking into them, dragging them (grab).

## 5. `fnvtool collide ragdoll <sources>... --schema <schemas/nif> [--filter text] [--summary]` → `fnv.collide.ragdoll/1`

Observational (not a lab scenario), like `collide survey`.

- `summary`: `meshes_with_joints`, `bodies`, `joints` (count by kind, " (malleable)"
  when wrapped), `bind_within_limits` and `bind_within_limits_opposite_sign` (by kind:
  joints whose bind pose is inside the limits, 0.02 radians slack, as read in §3 and
  with plane and twist negated), `joints_on_transformed_bodies`, `pivot_gap` and
  `pivot_gap_on_transformed_bodies` (quantiles of the distance between a joint's two
  pivots in the bind pose, game units), `stored_translation_to_bind` (quantiles of the
  distance from a body's stored translation × 7 to its node's bind position),
  `problems`.
- `files` (left out with `--summary`): per mesh with at least one joint, its `bodies`
  (node, layer, mass, friction, restitution, dampings) and `joints` (constraint block,
  the nodes of a and b, the limits as read, `max_friction`, `strength`, `pivot_gap`,
  the bind pose's `cone`/`plane`/`twist` and whether it is inside the limits).

## 6. Ragdoll Templates (armour)

### What the data says

- An armour record's `BMCT` subrecord, "Ragdoll Constraint Template", is a zero-ended
  string [documented: xEdit wbDefinitionsFNV.pas ARMO; `schemas/fnv/objects.json`]: the
  path of a `.rdt` file [documented: GECK wiki Armor, "Ragdoll Constraint Template ...
  These files have the ".rdt" extension"] relative to `Data\Meshes\` [documented: GECK
  wiki SetArmorRCT notes]. "Each armor piece and each character will be able to select a
  single Ragdoll template file" [documented: GECK wiki Armor].
- A Ragdoll Template is a NIF: a root NiNode with no children whose extra data is a
  `bhkRagdollTemplate` named `RDT`, which lists `bhkRagdollTemplateData` blocks named
  after bones of the base skeleton [documented: GECK wiki Ragdoll Template]. Layouts
  [documented: nif.xml]: bhkRagdollTemplate is an NiExtraData with `num_bones` and
  `bones` (refs); bhkRagdollTemplateData has `name`, `mass`, `restitution`, `friction`,
  `radius` (floats), `material` (HavokMaterial), `num_constraints` and that many
  `bhkWrappedConstraintData` (a constraint type, the constraint info with its two
  entities, and the fields of that type, as in §3; type 13 holds a malleable
  constraint). nif.xml places the files in `meshes\ragdollconstraint\*.rdt` and
  calls them "Found in Fallout 3".
- Meaning of the fields [documented: GECK wiki Ragdoll Template]: the data "contains
  overrides for bhkRigidBody/bhkRigidBodyT and constraint blocks attached to bones on the
  base skeleton with the same name"; mass, restitution ("near nonfunctional in New
  Vegas"), friction; material "overrides the collision material of the shape attached to
  the RigidBody"; `radius` has no description. Templates "can affect either all or a
  handful of bones, and they can be layered using different equipment" (a body piece
  and a helmet each changing their own bones).
- Behaviour notes [documented: GECK wiki Ragdoll Template and Armor]: the Armor page says
  templates "do *not* work correctly" in Fallout 3 and New Vegas; the Ragdoll Template
  page says they work but do not reset when the armour is taken off (the reference needs
  `Update3D`), and that a template's collision material affects every loaded NPC, not
  only the wearers.
- **In this install** [observed 2026-10-20, FNV 1.4.0.525 with all DLC]: 17 ARMO records
  have a BMCT, all 30 bytes and all naming `RagdollConstraint\default.rdt`: the power
  armours (T-45d, T-51b, the Brotherhood's T-51b, Remnants, NCR Salvaged, Enclave,
  Tesla and its FX copy, the Gannons' Tesla, the Prototype Medic, the broken one, the
  unplayable one, Lonesome Road's Scorched Sierra), the Enclave Shocktrooper armour, the
  Raven Rock corpse's Enclave armour and the Dam war Enclave scientist's and officer's
  outfits. No `.rdt` file is in any of the 21 archives or loose under `Data`, so the
  path names a file the game does not have, and no format fact here has been checked
  against a real template.
- Not found: how the original matches a template's constraints to the skeleton's, what
  a template constraint's `entity_a`/`entity_b` point at in a file with no bodies, and
  what happens with a missing file [the last presumably nothing: hypothesis, since every
  power armour corpse names one].

### What NVForever does [design unless tagged]

- `fnv_collide::ragdoll::template` reads the bhkRagdollTemplate of a decoded `.rdt` and
  its bones (`schemas/nif/havok.json` has the two blocks); `Ragdoll::apply_template`
  lays one over a skeleton's ragdoll. Tests on a synthetic file:
  `crates/fnv-collide/tests/ragdoll_template.rs`.
- Per template bone, the body on the bone of that name (case ignored) takes its mass,
  friction, restitution and material. A mass that is not above 0 or a negative friction
  is not taken (a body without mass would not fall).
- Its constraints replace the joints by which that body is attached (those it is
  body a of, §1): one whose `entity_b` points at another bone of the template replaces
  the joint to that bone (or is added when the skeleton has none to it); the others
  replace the body's joints in order, the k-th the k-th [hypothesis: the template keeps
  the skeleton's constraint order]. Each joint is replaced once at most; constraints left
  over are logged and not used. Pivots are scaled as the skeleton's (§2).
- `radius` is read and not used (its meaning is not documented).
- When an actor's ragdoll is built (death, knock-out, knockdown, the player's too), the
  templates of the armour it wears are laid over in worn order, each path once
  (`engine/nvforever/src/ragdoll.rs`, `worn_templates`); a later one overrides an earlier
  one's bones. The template follows what is worn when the body falls, so taking armour
  off leaves no trace (the original's not resetting is not copied), and a material only
  changes the wearer's bodies.
- A path the data lacks is logged once and the skeleton's own data is used:
  `ragdoll template: meshes\ragdollconstraint\default.rdt: not in the data; the skeleton's own
  bodies and joints`. With the shipped game this is the only outcome; the code matters for
  mods that ship templates. Smoke scenario `ragdoll-armour-template` (Ringo killed in
  T-45d power armour) checks the line.

## 7. Open questions

1. The sign of the plane and twist limits (§3) rests on anatomy. A capture could settle
   it: kill an NPC on flat ground and read limb bone angles (`GetNifBlockRotation`-style
   xNVSE functions on the ragdolled skeleton), or simply compare how corpses lie.
2. `max_friction`'s unit and whether `strength` softens the limits.
3. Whether the original starts a ragdoll with the body's velocity at death, and how
   hard hits push (`fPushForce`-like settings were not looked for).
4. Which layers DEAD_BIP collides with in the original.
5. Templates (§6): how the original pairs a template's constraints with the skeleton's,
   and what their entities point at. A mod's `.rdt` made in NifSkope, opened next to the
   skeleton, would show it.
6. Bodies in water (§8): how high the original's items ride (their reference origin
   read 0-2 units under the surface; their tops were not read), the materials not yet
   captured (organic, grass, heavy wood, barrel, rubber ball and the others), why the hot
   plate sank much slower than the toaster, and whether the original's corpse lies on
   the bed or only its reference position is read there.

## 8. Bodies in water

The owner asked (2026-10-25) that anything ragdolled float to the surface in water, so
that a drowned or killed swimmer comes up, and then that loose items do the same.
Captures of the original were run first for the items (`tools/capture/jobs/make_float_jobs.py`,
jobs `fl_items_lake` and `fl_heavy_lake`, save nvfstrip, 2026-10-25): fifteen items
dropped by the player and moved by `SetPos` 150 units under Lake Mead's surface (2600)
over a bed 350-660 units down, and an NCR trooper placed there and killed, their
heights read for 30 s.

### What the original does [observed]

| Floated to the surface (reference origin 2597-2600) | Sank to the bed |
|---|---|
| pencil, sledgehammer (wood, 9) | sensor module, toaster (metal, 5) |
| teddy bear, Pre-War book, Wonderglue (cloth, 1) | hot plate (glass, 3) |
| cigarette carton (dirt, 2) | the killed NCR trooper (three runs) |
| empty and full whiskey bottles, Nuka-Cola (bottle, 24) | |
| tin can (soda can, 25) | |

- The numbers are the Havok material of the item's collision shape (nif.xml's
  `Fallout3HavokMaterial`). Neither the stored Havok mass over the shape's volume (the
  toaster, 3 over a large hull, sank; the pencil, 2 over a tiny box, floated) nor the
  record's Weight (bat 3 floated, sensor module 2 sank) separates the two columns; the
  material does.
- Rising took 1-5 s from 150 under (about 20-60 units a second). The sensor module and
  toaster sank at about 170 units a second; the hot plate at about 45.
- The trooper's reference position was on the bed (2160-2200) 1 s after the kill and
  stayed there: the original does not float corpses. Whether its bones lay there too
  was not read (§7 item 6).
- The first run's lead pipe and sledgehammer readings, and the second run's bat and
  lead pipe, were references the save already held or lost from the scan; they are not
  used. The bat read at the surface in the first run (likely ours, not certain).
- Trap for later jobs: scans for created items must keep to the player's surroundings
  (`GetDistance player`); the save holds dropped weapons of the same bases kilometres away.

### What NVForever does

- **What floats** (`crates/fnv-phys/src/buoyancy.rs`): ragdoll bodies and gore pieces,
  the owner's choice, unlike the original; a loose item (a dynamic placed or dropped
  reference) whose first collision shape's Havok material is in `FLOATING_MATERIALS`:
  cloth, dirt, wood, bottle and soda can [observed], organic, grass, baby rattle and
  rubber ball [hypothesis: light by their names]. Every other item sinks.
- **Where.** In the water the character swims in (`crates/fnv-phys/src/water.rs`): an
  exterior cell's surface, or a placed water volume. A body is in water when the water
  surface above its centre of mass is above the bottom of its colliders.
- **How much.** Before each physics step, each awake dynamic body in water gets an upward
  change of velocity of `lift` × gravity × the share of its height under the surface, and
  its velocity and spin die out at `WATER_DRAG` and `WATER_SPIN_DRAG` (2 per second) times
  that share. `lift` is `FLOAT_RATIO` (1.2) for a body that floats: all under water it
  rises at 0.2 g, at most about 70 units a second, and floats with about a sixth of its
  height above the surface [design]. For one that sinks it is `SINK_LIFT` (0.5): it sinks
  at about 170 units a second [fitted to the sensor module and toaster].
- **Rest.** A ragdoll that is still moving 3 s after it started is held at rest where it
  is (§4); one in water gets 30 s (`MAX_AWAKE_IN_WATER`, engine/nvforever/src/ragdoll.rs)
  so that it reaches the surface first. Once it is calm there it is held at rest at the
  surface as on land. A corpse put back at rest, or laid in the pose its reference stores
  (`XRGD`), stays where it lies, under water too, until a hit or a blast rouses it: the
  designers placed those bodies there. Items asleep on a lake bed when their cell loads
  stay there until something wakes them.
- **Script-made items.** An item a script drops or places (`Drop`, `DropMe`,
  `PlaceAtMe`) now gets its collision as one dropped from the inventory menu does
  (`engine/nvf-play/src/script_events.rs`, the `Placed` event); before, it hung where it
  appeared.
- Logs: `ragdoll: actor X floats at (x, y, z), its trunk N units under the surface at S`
  when a ragdoll in water comes to rest; at exit `physics: N loose item bodies in water at
  exit: <ref> (floats|sinks, its top N units from the surface), ...`. Smoke
  `ragdoll-floats`, `loose-items-float`.
