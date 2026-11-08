# Effect shaders (EFSH)

Effect shaders are the glows, shimmers and burns drawn over a reference's own meshes:
Dead Money's holograms, the Nightkin's cloak, the plasma "gooification" glow, Cateye's
green rim on actors. Scripts play them with `PlayMagicShaderVisuals` (205 calls in the
ten masters) and stop them with `StopMagicShaderVisuals` (79); magic effects (MGEF)
name one to play on their target. Implementation: `fnv_world::effect_shader` (record and
timing), `fnv_game::functions::misc` (the script functions, what plays where), nvforever
`effect_shaders.rs` (drawing). Evidence tags as in the charter.

## 1. Record layout

Sources: xEdit `wbDefinitionsFNV.pas` EFSH record and `wbDefinitionsCommon.pas`
(`wbBlendModeEnum`, `wbBlendOpEnum`, `wbZTestFuncEnum`), branch dev-4.1.6 [documented];
the schema is `schemas/fnv/effects.json`. Field meanings in prose come from the GECK
wiki page "EffectShader" (owner's local export, `Special-Effect.xml`) [documented].

Subrecords: `EDID`; `ICON` fill texture; `ICO2` particle shader texture; `NAM7` holes
texture (all zero-terminated paths under `textures\`); `DATA`.

`DATA`, little-endian, byte offsets (only the fields this engine reads are listed; the
full list is in the schema):

| Offset | Type | Field | Meaning (GECK wiki) |
|---|---|---|---|
| 0 | u8 | flags | bit 0 No Membrane Shader, bit 3 No Particle Shader, bit 4 Edge Effect Inverse, bit 5 Membrane Affect Skin Only |
| 4 | u32 | membrane source blend | blend mode enum (below) |
| 8 | u32 | membrane blend operation | 1 Add, 2 Subtract, 3 Reverse Subtract, 4 Minimum, 5 Maximum |
| 12 | u32 | membrane z test | 3 Equal To, 4 Normal, 5 Greater Than, 7 Greater or Equal, 8 Always Show |
| 16 | 4 × u8 | fill colour | R, G, B, unused |
| 20, 24, 28 | f32 | fill alpha fade in / full alpha / fade out time | seconds |
| 32 | f32 | fill persistent alpha ratio | alpha after the fade out, while the shader lasts |
| 36, 40 | f32 | fill alpha pulse amplitude, frequency | frequency in pulses per second |
| 44, 48 | f32 | fill texture animation speed U, V | |
| 52 | f32 | edge fall off | higher: the rim narrows ("sharper outline") |
| 56 | 4 × u8 | edge colour | |
| 60 … 80 | f32 × 6 | edge fade in, full, fade out, persistent ratio, pulse amplitude, pulse frequency | as the fill's |
| 84 | f32 | fill full alpha ratio | alpha at the top of the cycle |
| 88 | f32 | edge full alpha ratio | |
| 92 | u32 | membrane dest blend | blend mode enum |
| 96 … | | particle shader (§6), add-on models (§8), holes (§7) | |

Blend mode enum: 1 Zero, 2 One, 3 Source Colour, 4 Source Inverse Colour, 5 Source
Alpha, 6 Source Inverted Alpha, 7 Dest Alpha, 8 Dest Inverted Alpha, 9 Dest Colour,
10 Dest Inverse Colour, 11 Source Alpha SAT [documented].

`DATA` is shorter in some records: xEdit makes every field from the 57th element
(particle "initial speed along normal ±") on optional [documented]. Sizes seen in the
ten masters: 308 bytes (32 records), 300 (4), 284 (1), 248 (3), 244 (1), 224 (3); all
44 records decode with no bytes left over (`fnvtool esm coverage`) [observed]. Every
field this engine reads lies in the first 96 bytes, present in all of them.

## 2. What the masters hold [observed]

44 EFSH records (FalloutNV.esm 35, Dead Money 5, Old World Blues 4; dumped with a
private script over the masters, 2026-10-16). Every one uses membrane source blend 5
(Source Alpha), operation 1 (Add) and z test 3 (Equal To). The destination blend is 2
(One: additive, 20 records), 6 (Source Inverted Alpha: ordinary alpha blending, 18) or 5
(Source Alpha, the six holograms). Fill textures, where set, are under `effects\`,
`nvdlc01\effects\` and `dlcanch\effects\`. Examples:

- `NVDLC01HologramNeutral` (Dead Money): fill (119, 218, 255) with the panning texture
  `NVDLC01\effects\NVDLC01panningFX.dds` moving at U −0.5; fade in 0.2 s, full ratio 1,
  persistent 0.9, pulse 0.1 at 20 per second; edge (164, 206, 240), fall off 0.95,
  persistent 0.8; flags 0x18 (no particles, edge inverse).
- `fxCateye`: no fill (full ratio 0), green edge (4, 255, 4) at persistent 0.8.
- `PlasmaGooificationGlow`: fill (148, 255, 117) fading in 0.15 s, held 0.06 s, out over
  0.75 s to persistent 1.

## 3. Timing

GECK wiki [documented]: the fill alpha rises to the full alpha ratio over the fade-in
time, stays there for the full alpha time, then goes to the persistent alpha ratio over
the fade-out time; "the persistent effect will last for as long as the effect shader is
active". The pulse makes it vary with the given amplitude and frequency. The edge has
the same set of numbers of its own. `PlayMagicShaderVisuals`' summary: the visuals play
at least one cycle whatever the duration; without a duration they play until stopped
(or "until the art stops animating").

Read here as [hypothesis where marked]:

- alpha(t) = full × t / fade-in during the fade-in; full during the full time; linear
  from full to persistent during the fade-out; persistent afterwards.
- pulse: alpha × (1 + amplitude × sin(2π × frequency × t)), held to 0…1
  [hypothesis: the wiki gives only "how noticeable" and "how often"].
- With a duration d, the shader ends at max(d, fade-in + full + fade-out) seconds
  ("at least one cycle"); without one it lasts until stopped. When it ends, it is gone
  at once [hypothesis: no documented fade at the end].
- A shader whose persistent ratio is 0 shows nothing after its cycle while still
  "playing" [follows from the wiki's description].
- The fill colour multiplies the fill texture when there is one [hypothesis: the wiki
  says the colour is used "if a texture is not selected"; several records with a
  texture carry a non-white colour, e.g. the holograms, so the colour is read as a
  tint].
- Magic effects: an active effect whose base effect (MGEF `DATA` "effect shader")
  names an EFSH plays it on the effect's target for the effect's time; the GECK wiki
  says the persistent part shows only when the base effect has "FX Persist" (MGEF flag
  bit 10) [documented], so without it only the cycle plays.

## 4. Drawing (nvforever `effect_shaders.rs`) and what is not drawn

- Membrane: every mesh drawn for the reference gets an overlay drawn on the same
  surface (the same mesh and skin), unlit, with the fill texture and colour at the fill
  alpha, plus a second overlay for the edge: the edge colour at the edge alpha, scaled
  by the view angle so it shows where the surface turns away from the camera (the NIF
  material's view-angle falloff). The fall off value sets the angle at which the rim
  reaches half strength: cos θ = 1 − 0.5^(1/fall off), a linear ramp through it
  [design: the game's rim curve is not documented].
- Cut-out and blended meshes (hair, beards, lashes: alpha-tested or alpha-blended
  materials) get no overlay [design]. The overlays carry no texture alpha of the mesh,
  so on hair they covered every card whole in the membrane's colour: Flames01's edge
  (no fill) painted the player's hair flat orange with a bright rim and the head looked
  bald (owner's playtest, 2026-10-24; smoke `hair-under-fire`). Whether the original
  draws the membrane on hair, and through its alpha, is not known.
- Blend: destination 2 (One) and 5 (Source Alpha) are drawn additively, 6 (Source
  Inverted Alpha) as ordinary blending, 9 (Dest Colour) multiplied [design over the
  documented blend factors; 5 + 5 is "source × alpha + destination × alpha", nearest to
  additive on a dark backdrop]. The "Edge Effect Inverse" flag is not applied
  [design: the wiki's "darken rather than lighten" needs a subtractive blend].
- Z test: the overlay is drawn at the surface (Equal To is the only value in the
  masters' membranes); "Greater Than" (seen through walls) is not built.
- The particle shader: §6; holes: §7; add-on models: §8. Not built: "Affect Skin
  Only" (the whole actor is covered).
- A shader on the player covers only the third-person body's meshes, never the
  first-person arms [design: `effect_shaders.rs::play`]. Each overlay takes its covered
  mesh's render layers at birth. Player overlays follow later changes in both
  directions, after `player_body.rs::show_rigs` decides which parts the camera sees
  [design: `effect_shaders.rs::sync_player_effects`]. In ordinary first person the whole
  body is on `SHADOW_LAYER`; with `--first-person-body`, the head and any replaced arms
  stay there while the seen body uses `BODY_LAYER` [design: `player_body.rs::Seen`].
  Neither the main camera nor the reflection camera sees `SHADOW_LAYER`, and overlays
  have `NotShadowCaster`, so those overlays draw nothing, including in shadows
  [design: `main.rs` camera spawn, `reflection.rs`, `effect_shaders.rs::play`].
  Other actors' overlays copy their mesh's layers at birth without joining the player's
  per-frame synchronization [design: `effect_shaders.rs::PlayerOverlay`].
- The wiki notes the original keeps a shader on the perspective it started in
  [documented: GECK wiki, as this note read before #1772]. The original, Flames01 played on
  the player in first person with the weapon holstered, showed nothing of it in first
  person, flames at the head once in third person, and nothing again back in first person
  [observed 2026-11-06: private capture ov_burn, the Prospector Saloon, 1280 x 720]. So in that
  run a shader started in first person was seen in third.
- What the original draws on the first-person arms during burning remains unconfirmed, as
  no arms were in view in that capture [hypothesis]; observer-checks.md item 100 asks. The regression scenario
  `burn-first-person` starts Flames01 in first person, then switches to third person;
  `effect shader: ... on the player: the camera sees N of M covered meshes` reports
  changes in the camera's coverage [design: `tools/smoke/scenarios.toml`,
  `effect_shaders.rs::sync_player_effects`].

## 5. SetActorAlpha, Reset3DState, SetSecuritronExpression

- **SetActorAlpha** (127 calls) [documented: GECK wiki]: sets how transparent the
  calling actor is; "does not affect the weapon they carry". Drawn here by giving the
  actor's meshes blended copies of their materials with their alpha multiplied by the
  value; 1 gives the original materials back. Kept per actor, not saved [design].
- **Reset3DState** (12 calls) [documented: GECK wiki]: "clears any saved data
  regarding a reference's animation/Havok state", mainly to reset triggered traps; it
  does not move the reference back. Here: the reference's script animation state is
  forgotten (no group counts as playing, a group waiting for the object to be drawn is
  dropped) and a drawn object stops its group [hypothesis: what "saved data" covers].
- **SetSecuritronExpression** (37 calls) [documented: GECK wiki]: `SetSecuritronExpression
  ref face expression` sets the monitor of a Securitron; the function works from the
  file names in `textures\creatures\securitron\` (`<face>_<expression>.dds`, e.g.
  `victor_alert.dds`), and persists across saves (kept in the game snapshot,
  `GameSnapshot::securitron_faces`, 2026-10-19). [observed: the screen models
  `meshes\creatures\nvsecuritron\nvsecuritron*screen01.nif` draw `<face>_Neutral.dds`
  (`BlackTron.dds` for the blank one) on the shape `Screen01:0`; the folder holds
  `_neutral` and `_alert` files for victor, mrhouse, marilyn, jane, infantry, police and
  yesman.] Implemented as a texture swap of `Screen01:0` on that reference
  (`SwapTexture`'s machinery); not saved yet [design]. [observed 2026-10-16, smoke
  `securitron-expression`: `VStreetSecuritron` 0x0013BB1F on the Strip shows its police face,
  then Victor's alert face after the call.]

## 6. The particle shader (nvforever `effect_particles.rs`)

Layout [documented: xEdit EFSH, field order of `schemas/fnv/effects.json`; byte offsets
counted from it]: 96 particle source blend, 100 blend operation, 104 z test, 108 dest
blend (u32 each); 112 birth ramp up time, 116 full birth time, 120 ramp down time, 124
full birth ratio, 128 persistent birth ratio, 132 lifetime, 136 lifetime ±, 140 initial
speed along normal, 144 acceleration along normal, 148–156 initial velocity X, Y, Z,
160–168 acceleration X, Y, Z, 172 scale key 1, 176 scale key 2, 180 and 184 their times,
188–196 colour keys 1–3 (4 bytes each, RGB and an unused byte), 200–208 their alphas,
212–220 their times, 224 initial speed along normal ±, 228 initial rotation, 232 its ±,
236 rotation speed, 240 its ±, 244 add-on models (DEBR), …, 276 and 280 texture count U
and V, to 308 (all f32 but the counts and colours). The 224-byte records end before the
speed range: the particles of a record shorter than 244 bytes are not read; under 284
bytes the texture is one frame.

Meanings [documented: GECK wiki EffectShader, "Particle Shader"]: the birth ramps and
ratios as the membrane's alpha cycle; the lifetime ± range; the speed along "the mostly
random direction that a particle is given when it is spawned", its acceleration; the
velocity and acceleration "in world coordinates" ("to make particles drift upwards,
apply a positive velocity to the Z axis"); key times are shares of the particle's life;
"all particles are spawned with a scale of 0 and increase in size until" key 1; a first
colour key after 0 begins "with the raw color of the texture file", a last one before 1
returns to it; "if you do not select a DDS file, the particle will not display". The flag
"No Particle Shader" (bit 3) leaves the particles out.

Example [observed, `fnvtool esm decode` 2026-10-16]: `Flames01` 0x0005F9B6 (the flamer's
burning): texture `Effects\FXFlameSmallSub02.dds` in 4 × 2 frames, ramp up 0.5 s to
ratio 30, persistent 15; life 0.35 ± 0.1 s; along the normal 50 ± 50, −5; velocity (10,
10, 195), acceleration (0, 0, 50); scale 30 at 5 % of the life, 3 at its end; colour keys
(255, 107, 64), (255, 156, 23), (244, 80, 0) at 0.2, 0.5, 0.9; rotation ± 180°, spin 5 ±
5°/s; dest blend One. Its membrane: no fill, an orange edge (215, 86, 0), fall off 3,
persistent 1. Its add-on models are `debrisFlameMesh` 0x0005F9B7
(`Effects\ActorFireFX\FireBall01–04.NIF`), drawn as §8 says. `Smoke01` 0x0005F9B8: particles
only (No Membrane Shader), `Effects\FXDustSmoke01.dds` in 2 × 2 frames, dest blend Source
Inverted Alpha; ratio 15, life 2.3 ± 0.5 s; velocity (0, 0, 50), acceleration (0, 0, 65);
scale 25 at birth to 100 at death; colours (158, 106, 84), (84, 79, 69), (64, 64, 64) at
alphas 0, 0.5, 0 and times 0, 0.07, 0.9 [observed, `fnvtool esm decode` 2026-10-20].

Drawn here [hypothesis or design where marked]:

- Births: birth ratio × 6 particles a second [design: the wiki gives no unit; "a value of
  1 will cause the maximum amount of particles to spawn" does not fit Flames01's 30;
  fitted by eye], at most 600 alive per play; after the ramps the persistent ratio only
  while the persistent part shows ("FX Persist", §3), as for the membrane [hypothesis].
- Where: at one of the bones the reference's skinned meshes are bound to, 10 units out
  along the particle's normal (a bone lies inside the body), or anywhere in an unskinned
  mesh's bounds [design: the game's surface points are not documented]. The normal is a
  random direction.
- Hidden meshes give off no new particles. For the player, meshes on layers the camera
  does not see also give off none, including the head of `--first-person-body`; particles
  already alive finish their lives [design: `effect_particles.rs::draw`].
- Scale: linear from 0 to key 1 and to key 2, held after; the scale is the particle's
  width in game units [hypothesis]. Colour: linear between keys, from and back to white
  (the texture's own colour) [hypothesis: linear]; the alpha is the nearest key's outside
  the keys, not back to 1 [hypothesis: the wiki lists "Color Alpha" apart from "Color"
  and says only the colour returns; `Smoke01`'s alphas 0, 0.5, 0 at 0, 0.07, 0.9 fade its
  puffs out, and an alpha back to 1 flashed every puff white at full size before it went,
  owner's playtest 2026-10-20]; taken as display values as
  NIF particles are (particles.md §2). One frame of the U × V grid per particle, at
  random [hypothesis].
- Blend: as the membrane's destination blend (One: added) [design, §4]. Not built: the
  particle z test "Always Show" (seen through walls).
- When the play ends, no more are born; those alive live out their time.

## 7. Holes

Layout [documented: xEdit EFSH, field order of `schemas/fnv/effects.json`; byte offsets
counted from it]: `NAM7` the holes texture; in `DATA` 248 holes start time, 252 holes end
time, 256 holes start value, 260 holes end value, 264 edge width (alpha units), 268 edge
colour (RGB and an unused byte; the schema's `edge_color_2`), all f32 but the colour.
The GECK wiki's EffectShader page says nothing of holes [observed: the owner's export,
`Special-Effect.xml`], so the meanings below come from the names and the records.

What the masters hold [observed, a private script over the ten masters, 2026-10-21]: eight
records name a holes texture, seven of them `Effects\FXMagicDisolveAlpha01.dds`:

| Record | Start, end time (s) | Start, end value | Edge width, colour | Played by |
|---|---|---|---|---|
| `GooShader01` 0x0009BA49 | 0.2, 1.75 | 255, 0 | 1, white | `PlasmaGooificationEffectScript` (with GooShader02; `GooEnd` at 1.4 s) |
| `effectLaserDisintegration` 0x000BDA2D | 0.1, 1.4 | 255, 0 | 1, (255, 85, 13) | `LaserDisintegrationEffectScript` (ash pile at 1.3 s, `DisintegrateEnd` at 1.8 s) |
| `effectAlienDisintegration` 0x0005C6C0 | 0.1, 1.8 | 255, 0 | 3, (187, 255, 249) | |
| `effectAtomization` 0x00044AEF | 0, 0.6 | 255, 0 | 3, (248, 255, 234) | |
| `NukeAtomShader` 0x000446D3 | 0, 2 | 255, 0 | 3, (255, 64, 69) | `NukeCritEffectScript` |
| `PuffOfSmokeVanish` 0x000614F9 | 0, 0.3 | 255, 0 | 0 | |
| `HaroldLiniment` 0x00055B50 | 0, 4 | 0, 127 | 3, white | no script names it |
| `NVDLC03HoloRmShader` (Old World Blues) | 0, 10 | 255, 0 | 0.05, white | `NVDLC03X13HoloRmEffectsSCRIPT`; its holes texture is `dlcanch\effects\dlcanchrezeffectalpha.dds`, also its fill |

The other records carry (0, 10, 255, 0), edge width 0 and a white edge colour, and no
holes texture (`GlowingOneFXShader`'s 248-byte DATA ends before the holes fields).

Drawn here [hypothesis or design where marked]:

- Only a record with a holes texture has holes [hypothesis: the default numbers would
  otherwise dissolve every shader's target over 10 s].
- The value: the start value until the start time, linear to the end value by the end
  time, held after [hypothesis].
- A point of the reference's surface whose holes-texture alpha (0–255) is at or above
  the value is a hole: not drawn, in any pass (shadows too); at 255 or more there are
  none [hypothesis: the masters' dissolves go from 255 to 0, are played on deaths
  (gooification, disintegration, atomization, "vanish") and end with the body gone:
  `GooEnd` hides the reference after 1.4 s, by when GooShader01's value is about 70].
  The texture is read at the mesh's own texture coordinates [hypothesis].
- The band of "edge width" alpha units below the value (where the holes will open
  next) is drawn in the edge colour, unlit [hypothesis: the name "edge width (alpha
  units)" and the glowing colours of the death records]. The membrane's own overlays
  get the same holes.
- The holes cover the meshes the membrane covers (§4) and last while the play does;
  when it ends the meshes get their own materials back [hypothesis]. `HaroldLiniment`'s
  0 to 127 would start with the whole body a hole; no script plays it.
- Not combined: `SetActorAlpha` on a reference while it has holes takes the holed
  material as its own [design: no record does both].

## 8. Add-on models

Layout [documented: xEdit EFSH, as §7]: `DATA` 244 add-on models (a DEBR FormID), 284
fade in time, 288 fade out time, 292 scale start, 296 scale end, 300 scale in time, 304
scale out time (f32). A DEBR lists models with a percentage each [documented: GECK wiki
"Debris": "the chance that this particular kind of debris will appear"]. The EffectShader
page says nothing of add-on models.

What the masters hold [observed, 2026-10-21]: two records name one:

- `Flames01` 0x0005F9B6 (burning): `debrisFlameMesh` 0x0005F9B7, `FireBall01–04.NIF` at 25 %
  each; fade in 0.75 s, out 2 s, scale 0.5 at the start and 0.2 at the end, scale in and
  out 1 s.
- `GlowingOneFXShader` 0x0008AA59 (the Legion explorers' aura, the spore carriers, the
  Legendary Bloatfly, the tunnelers): `debrisGOGlowMesh` 0x0008AA5A,
  `Effects\GlowingOneFXShader\GOGlowMesh01–04.NIF` at 25 % each; its DATA is 248 bytes
  long, so no timing.

The models are small unskinned sprites [observed: `fnvtool nif decode`: a BSFadeNode over
an NiBillboardNode and one NiTriStrips each, with texture-transform controllers; the
fireballs also with a transform controller].

Drawn here [hypothesis or design where marked]:

- Where: one model on each of up to 8 bones of the reference's skinned meshes, spread
  evenly over them, at the bone itself; an unskinned reference gets one, at its first
  mesh [design: the record gives no count and no places].
- Which: a model of the DEBR by its percentages, picked once per place and play [design].
- Each is played as an effect model: its billboards face the camera, its controllers play
  (`shot_fx::spawn_attached`).
- Player add-ons follow the camera's decision for their attachment's part, including
  while fading after a shader ends. A mesh attachment uses that mesh's head or arm
  classification; a bone on or below `Bip01 Head` follows the head, and one on or below
  either `Bip01 L UpperArm` or `Bip01 R UpperArm` follows the arms. Other attachments
  follow whether the body is seen at all [design: `player_body.rs::effect_part`,
  `effect_bone_part`, `Seen::effect_part`]. This branch classification is an engine
  choice, not confirmed original behaviour [design]. Their holders use inherited
  visibility, and their own `AddonEmitter` or `ModelEmitter` checks the same part before
  births, even on its first frame. Other actors' add-ons keep their existing visibility
  and birth rules [design: `effect_shaders.rs::player_addon`, `sync_player_effects`,
  `particles.rs::emit`].
- Timing: from the play's start the model grows from the scale start to 1 over the scale
  in time and fades in over the fade in time; after the play ends it shrinks from where
  it was to the scale end over the scale out time and fades out over the fade out time,
  and goes when both are done [hypothesis: the field names; linear ramps]. Without the
  timing fields (248-byte DATA): full size and alpha at once, gone at once [hypothesis].
