# Spec notes: NIF meshes and KF animations (Layer 2)

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525, all 25,760 `.nif` and
`.kf` files in the 11 archives that hold meshes), **[hypothesis]** needs verification.

Main public source: niftools `nif.xml` (https://github.com/niftools/nifxml, read as
documentation; its `since`/`until` version bounds are both inclusive, as the many
Bethesda-only fields bounded `since=until=20.2.0.7` show). Field layouts per block type
are kept as data in `schemas/nif/*.json`; this file describes the container and the
object model that tools rely on.

## 1. Versions in the corpus

| Version | User version | Bethesda stream version | Files |
|---|---|---|---|
| 20.2.0.7 | 11 | 34 | 25,090 |
| 20.2.0.7 | 11 | 33, 32, 27, 31, 25, 21, 30, 26, 14, 24, 28 | 664 |
| 20.0.0.4 | 11 | 11 | 3 |
| 20.0.0.4 | 10 | 11 | 3 |

[observed] nif.xml lists 20.2.0.7 / user 11 / stream version 34 as the Fallout 3 and New
Vegas format [documented: nif.xml `<version id="V20_2_0_7_FO3">`]. The lower stream
versions are older exports that the game still loads [hypothesis: they decode with the same
rules, see the coverage report]. The six 20.0.0.4 files are five idle animations
(`meshes\characters\_male\idleanims\*.kf`) and `meshes\triggers\collisionboxstatic.nif`
[observed].

## 2. Container (header, blocks, footer)

All numbers are little-endian.

1. A text line `Gamebryo File Format, Version 20.2.0.7` ended by a line feed (0x0A)
   [documented, observed in every file].
2. Version as a u32 packing the four dotted parts into bytes (`20.2.0.7` = 0x14020007)
   [documented].
3. One byte endianness: 1 = little-endian, the only value present [observed].
4. User version u32 (11; 10 in three of the 20.0.0.4 files [observed]), number of blocks
   u32 [documented].
5. Bethesda export block (present for these versions): stream version u32, then three
   short strings, each a u8 length followed by that many bytes including a terminating
   NUL: author, "process script", "export script" [documented; NUL termination observed
   in all 77,262 strings]. Typical values: author is an artist login or empty; process
   script `Do Nothing` or empty; export script names an export preset such as ` PE Static
   Art` [observed]. Nothing in the engine is known to depend on them [hypothesis].
6. Number of block types u16, then that many type names, each a u32 length plus the bytes
   without terminator [documented]. The list has no duplicates, every entry is used, and
   entries appear in order of first use [observed].
7. One u16 per block: index into the type-name list [documented]. nif.xml notes the top
   bit as a flag used by other games; it is never set here [observed].
8. Version 20.2.0.5 and later: one u32 per block giving its size in bytes [documented].
   The sizes always account for exactly the bytes up to the footer [observed in all
   25,754 files that have them]. This makes every block skippable without understanding
   it — the basis of the tolerant parser and of coverage measurement.
9. Version 20.1.0.1 and later: string table — count u32, the length of the longest string
   u32, then the strings (u32 length + bytes, no terminator) [documented]. The stored
   maximum always equals the longest entry; entries are unique and contain no NUL
   [observed].
10. Group count u32 (+ one u32 per group) [documented]; always 0 [observed].
11. The blocks, back to back, in index order.
12. Footer: root count u32, then that many block indices (i32). One root in 25,720 files,
    two to five in 34; the first root is always block 0 [observed]. Nothing follows the
    footer [observed].

Version 20.0.0.4 differs [documented]: no block sizes (8) and no string table (9); string
fields are stored inline (u32 length + bytes) instead of as string-table indices. Such a
file can only be split into blocks by decoding each block completely.

Block references are i32 block indices with -1 meaning "none" [documented]. nif.xml
distinguishes downward references (ownership, e.g. a node's children) from upward
pointers (e.g. a controller's target); both are stored the same way [documented].

String fields in 20.2.0.7 files are u32 indices into the string table; 0xFFFFFFFF means
"no string" [documented].

## 3. Object model used by the scene tools

Every block type has a fixed field layout that may depend on the file version, the stream
version, and earlier fields of the same block [documented]. Types form a single-
inheritance hierarchy and a derived type's data starts with all of its parents' data, so
the leading fields of any block can be read knowing only an ancestor's layout
[documented].

**Named object** (nif.xml NiObjectNET, the ancestor of every scene object, property and
controller): name (string), number of extra-data references (u32) and the references,
then one controller reference [documented].

**Scene object** (NiAVObject), after the named-object fields:
- flags: u32 when the stream version is above 26, otherwise u16 [documented; both forms
  observed];
- translation: three f32; rotation: 3×3 f32 matrix stored row by row; scale: one f32
  [documented];
- property count u32 and property references; then one collision-object reference
  [documented].

**Node** (NiNode), after the scene-object fields: child count u32 and child references,
then effect count u32 and effect references [documented]. Types derived from NiNode that
occur here: BSFadeNode, BSMultiBoundNode, BSValueNode, NiBillboardNode, BSOrderedNode,
BSMasterParticleSystem, and through BSRangeNode: BSBlastNode, BSDamageStage (derived from
BSBlastNode) and BSDebrisNode [documented: nif.xml `inherit` chains].

**Geometry objects** (NiTriShape, NiTriStrips, and BSSegmentedTriShape which derives from
NiTriShape) are scene objects followed by: a data reference (to the geometry data block),
a skin-instance reference, and material data [documented].

**Geometry data** (NiTriShapeData, NiTriStripsData), in order [documented, verified by
exact consumption on every such block in the corpus, observed]:
1. group id i32; vertex count u16; two bytes (keep flags, compress flags);
2. has-vertices byte; if non-zero, vertex count × three f32 positions;
3. a u16 of flags: in 20.2.0.7 files bit 0 means "one UV set present" and bit 12 means
   "tangents and bitangents present"; in 20.0.0.4 files the low six bits are the number
   of UV sets and bit 12 again means tangents [documented];
4. has-normals byte; if non-zero, vertex count × three f32 normals, and when the tangent
   bit is set, vertex count × three f32 tangents followed by the same number of
   bitangents;
5. bounding sphere: centre (three f32) and radius (f32);
6. has-vertex-colours byte; if non-zero, vertex count × four f32 (r, g, b, a);
7. UV sets: (number of UV sets) × vertex count × two f32;
8. consistency u16; additional-data reference i32;
9. triangle count u16;
10. for triangle lists (NiTriShapeData): number of triangle points u32 (= 3 × triangle
    count), has-triangles byte, and if non-zero triangle count × three u16 vertex
    indices; then a "match group" count u16 and that many groups (u16 count + u16
    indices);
11. for strips (NiTriStripsData): strip count u16, one u16 length per strip, has-points
    byte, and if non-zero each strip's indices (u16) back to back. A strip of n indices
    describes n − 2 triangles, every second one with reversed winding [documented:
    Microsoft Direct3D 9 documentation, "Triangle Strips"]; some of these are degenerate
    (repeated index) joins [hypothesis].

## 4. Deviations from nif.xml found in the data

- Particle emitters (NiPSysEmitter and its Box, Cylinder, Sphere and Mesh subtypes): nif.xml
  lists two extra floats without a version condition; FNV files do not contain them, and
  all 1,599 emitter blocks decode exactly without them [observed].
- Three of the 20.0.0.4 files carry user version 10, not 11 [observed]; they still have the
  Bethesda export block (stream version 11).
- Bethesda 20.2 particle data (NiPSysData, BSStripPSysData) stores a maximum particle count
  where other geometry data stores the vertex count, and no per-vertex arrays
  [documented: nif.xml; observed: required for all 1,599 such blocks].
- Byte runs that nif.xml calls unused (for example in Havok shapes) contain non-zero
  leftovers; they are kept verbatim [observed].

Havok rigid-body rotations are stored x, y, z, w — unlike the w, x, y, z order of scene
quaternions [documented: nif.xml hkQuaternion vs Quaternion].

## 5. Animation (KF files and embedded sequences)

- A KF file's root is a controller sequence (NiControllerSequence): a name, a list of
  controlled blocks, weight, text-key reference, cycle type (0 loop, 1 reverse
  (ping-pong), 2 clamp), frequency, start and stop times, and an accumulation root name
  [documented: nif.xml]. Each controlled block names the target node, the controller
  type and an interpolator [documented]. In 20.0.0.4 files those names are byte offsets
  into a string palette block (NUL-separated text); in 20.2.0.7 files they are strings
  [documented, observed: the five old KF files decode with the palette].
- Corpus counts in `Fallout - Meshes.bsa` [observed: `fnvtool nif anim`]: 4,360
  sequences, 219,033 controlled blocks; 213,701 drive transforms (88,683 B-spline,
  58,459 linear keys, 13,365 Euler XYZ keys, 180 TBC, 96 constant, 52,918 static poses);
  the rest drive floats, booleans and points.
- Key groups store an interpolation kind per group: 1 linear, 2 quadratic (Hermite with
  stored tangents), 3 TBC (Kochanek–Bartels tension/bias/continuity), 4 XYZ rotation (three
  float key groups, Euler angles), 5 constant [documented: nif.xml KeyType]. Rotation
  keys are quaternions stored w, x, y, z [documented]. Interpolating them component-wise
  gives non-unit results where consecutive keys lie in opposite hemispheres (356 of
  1.59 M samples); spherical interpolation along the shorter arc removes all of them
  [observed].
- Quadratic key tangents: each key stores a value and two tangents, which nif.xml names
  forward and backward, in that order [documented: nif.xml]. Despite the names, the
  forward tangent is the one *arriving* at the key and the backward tangent the one
  *leaving* it, both expressed per key interval (the Hermite basis over u in [0, 1])
  [observed, `crates/fnv-nif/examples/quad_key_survey.rs` over `Fallout - Meshes.bsa`]:
  at the 181,454 inner keys of 32,959 quadratic key groups whose neighbouring segments
  change by clearly different amounts, the forward tangent matches the previous segment's
  change and the backward tangent the next segment's in 35,664 cases, the opposite in
  4,553, and 141,237 are within 10% either way. The clearest case is
  `architecture\ncr\spinningwindmill.nif`: its blades turn 0 → 4π over 7 s in two keys,
  the first with backward 4π and forward 0, the last with forward 4π and backward 0. Read
  this way the blades turn at a steady two turns per 7 s; read the other way round both
  tangents are 0 and they would stop and restart every cycle.
- B-spline interpolators [documented: nif.xml]: an open uniform B-spline per channel; a
  control-point data block holds float and compact (i16) points; the basis block holds the
  number of control points; each channel has a handle (start index, 0xFFFF = channel not
  animated), and for compact points an offset and half-range. Decompression
  `value = offset + (point / 32767) × half_range` and degree 3 are **[hypothesis]**,
  supported by the data: evaluated rotation quaternions have unit length within 10% in
  all but 241 of ~900,000 B-spline samples (cause of the 241 unknown; they are
  normalised) [observed]. Time maps linearly from [start, stop] to the parameter range.
- Text keys (NiTextKeyExtraData): (time, string) pairs such as `start`, `end`, sound and
  hit events [documented; event meaning is engine behaviour, Layer 6].
- Unbound tracks in an old file: `characters\_male\eatidle.kf` (one of the 20.0.0.4 KF
  files) has 50 controlled blocks; played on `characters\_male\upperbody.nif` bound to
  `characters\_male\skeleton.nif`, 27 find their node and 23 name nodes the FNV human
  skeleton does not have: tail bones, `Torch`, `Quiver`, `BackWeapon`, `SideWeapon`, and
  thumb and twist bones [observed 2026-10-07, viewer]. That this is a rig from an older
  game left in the file is a [hypothesis]; whether the original game ignores such tracks
  silently (as the viewer does) is not checked. Recorded as-is at the Layer 2 close-out
  (2026-10-09); no further evidence was collected.

## 6. Conventions verified on the data

- **Rotation matrices** are stored row by row and rotate column vectors (p' = R·p); a
  child's world transform is parent ∘ child with uniform scale [observed: the skin check
  below holds for 401 of 417 sampled skinned shapes under this convention and for 15
  under the transposed one; scene node matrices also equal the rotation of the
  quaternions animating them in all 8 unambiguous cases].
- **Skinning**: for each bone of a skin instance, the stored bone skin transform equals
  inverse(bone world transform) ∘ (shape world transform) in the file's bind pose
  [observed, same sample]. So it is directly the inverse bind matrix in shape space.
- **Body meshes store bones flat**: in creature and character body meshes (e.g.
  `deathclaw.nif`, `upperbody.nif`) every bone node is a direct child of the root and
  carries its bind-pose world transform; the bone hierarchy lives in the creature's
  `skeleton.nif` [observed]. Animations are relative to the skeleton: for
  `deathclaw\mtidle.kf` against `deathclaw\skeleton.nif`, every keyed translation at the
  start time equals the skeleton's local translation (within 1e-5) and every static
  rotation is identical, while against the body mesh's flat bones everything differs by
  ~90° and ~100 units [observed, `crates/fnv-nif/examples/kf_bind_check.rs`]. So skinned
  shapes bind to the skeleton's bones by name. The same check confirms the w, x, y, z
  order of B-spline rotation control points (other orders are ~140° off) [observed].
- **Winding**: triangles (and unrolled strips, alternate triangles reversed) are
  counter-clockwise seen from the side their vertex normals point to: 44,932 of 44,960
  triangles in 40 random meshes [observed].
- **Up axis**: Z is up [observed: rendered with Z mapped to up, bottles, weapons, creatures
  and bodies stand upright and label text reads correctly, so UVs need no flip].
- **Texture paths** in meshes [observed, `fnvtool nif textures`, 137,908 references in
  20,542 meshes]: 94% start with `textures\`, 6% with `data\textures\`, 7 are
  absolute (`d:\...`) and 7 bare file names; case varies. Normalising (lower case,
  backslashes, cut everything before `textures\`) resolves 98.1%; the remaining 2,607
  name textures that exist in no archive (e.g. `hospital*.nif`), presumably meshes the
  game never loads [hypothesis].
- **Texture-set slots** (BSShaderTextureSet): 0 diffuse, 1 normal (+ gloss in alpha),
  2 glow / skin / hair, 3 height, 4 environment cube, 5 environment mask [documented:
  nif.xml].
- **Editor markers**: 327 meshes in `Fallout - Meshes.bsa` (155 under `effects\`) have a
  node named `EditorMarker` (or `EditorMarker01`, ...) holding helper geometry such as
  wireframe boxes and arrows; most are *not* flagged hidden [observed]. nif.xml documents a
  BSXFlags bit "EditorMarkers present" [documented]. Drawn, they appear as bright boxes
  and lines around effects (observed in the first world render); the renderers skip them
  [hypothesis: the game never draws them; consistent with in-game appearance].
- **Helper geometry without a shader** [observed, `crates/fnv-nif/examples/shape_survey.rs`
  over `Fallout - Meshes.bsa`]: of 48,996 visible shapes (not hidden, no hidden
  ancestor, not under an editor marker), 982 carry no BSShaderProperty-derived property
  on themselves or an ancestor. 532 carry no property at all (524 of them landscape LOD
  meshes, which the engine textures itself), 16 have a NiTexturingProperty (debris,
  doors) and 434 only a NiMaterialProperty. The last group is helper geometry by its
  names: bone display boxes (`TTail1:0`, `Bone01:0`), collision proxies (`c_Minigun`,
  `DummyCol:3`) and invisible markers (`##PLRInvis:0`). The renderers skip shapes with
  properties but neither a shader property nor a texture [hypothesis: the game does
  not draw them; consistent with the Goodsprings flag in game, whose bone boxes are not
  visible].
- **Animated statics** [observed, same survey]: 164 meshes embed a controller sequence
  named `Idle` (others: `Forward` 226, `Open`/`Close` ~149, `SpecialIdle` 159, ...).
  Flags (`clutter\flags\*.nif`) are skinned cloth: e.g. `nv_nevadaflag.nif` has two
  skinned shapes (front and back) bound to a 10-bone chain that a looping 42.9 s `Idle`
  sequence (NiControllerManager → NiMultiTargetTransformController) animates, plus a
  NiGeomMorpherController adding small ripples (8 relative morph targets, deltas up to
  ~5 units, weights keyed 0..1). 75 meshes outside the character, creature and armor
  folders have visible skinned shapes (flags, gore, corpses, teddy bear, hand-to-hand
  weapons); 63 of them have no sequence. The world renderer plays a placed object's
  `Idle` sequence in a loop from load [hypothesis: consistent with waving flags and
  circling vultures in game] and draws skinned shapes without a sequence in their bind
  pose. Morph controllers are not evaluated yet.

  Windmills name their only sequence `SpecialIdle` [observed]:
  `clutter\farm\windmill*.nif` (looping 17.3 s or 20 s: the blades rock by up to
  ±0.34 rad about the shaft and the head yaws ±0.054 rad, with `Sound: SFXWindmill`
  text keys) and `architecture\ncr\spinningwindmill.nif` (STAT `WindmillSpinningNV`,
  placed in the Goodsprings cell: blades turn two full turns every 7 s, text keys
  `Sound: SFXWindmillHiSpdCrkNew`). In `Fallout - Meshes.bsa`, 116 meshes have
  `SpecialIdle` as their only sequence, all looping (bobbleheads 20, `dlc05\...\mz\misc`
  18, the solar-system orrery 11, ...), and 43 have it next to other sequences (signs,
  posters, control panels) [observed, same survey]. The world renderer plays `Idle`
  from load, or `SpecialIdle` when there is no `Idle` [hypothesis: windmills turn in
  game]. Whether the game also plays it from load for every other `SpecialIdle` mesh
  (bobbleheads nod in a 1 s loop) or only on a `PlayGroup SpecialIdle` or similar
  event is open.
- **Alpha** (NiAlphaProperty flags, layout documented in nif.xml; bits 10-12 hold the
  test function, 4 = GREATER, the nif.xml default). Survey of visible shapes in
  `Fallout - Meshes.bsa` and every `* - Main.bsa` [observed,
  `crates/fnv-nif/examples/alpha_survey.rs`]: 18,934 only alpha-test (foliage, fences),
  7,422 only blend, and 1,554 set both, all with TEST_GREATER. Of those, 855 have a
  threshold of 64 or more (mostly weapon parts, e.g. `lasergatlingcfframe.nif`). The other
  699 have 0-63, e.g. scope glass, the Vit-o-matic's glowing labels and the dirty glass
  of the NV poster marquees. `Marquee.dds` keeps grime as partial alpha with a fully
  transparent break.

  The renderers draw blend+test as alpha-tested from threshold 64 and as blended below
  [hypothesis: below that the test only trims nearly transparent texels]. Drawn
  alpha-tested, the marquee glass covered the posters with an opaque grey panel
  [observed: Prospector Saloon]. Destination factor ONE (additive) is kept as additive.
  TEST_GREATER keeps alpha strictly above the threshold; Bevy's mask keeps alpha ≥ the
  cutoff, so the cutoff moves half a step up [hypothesis]. Order-dependent blending
  remains possible for the blended shapes.

  Soft edges. The owner's game runs with `iMultiSample=8` and
  `bTransparencyMultisampling=1` [observed: `FalloutPrefs.ini`, 2026-10-10], and in its
  screenshots the cut-out edges of plants are smooth where a per-pixel alpha test gives
  steps [observed]. How the game's transparency multisampling turns alpha into coverage is
  not known. The renderers anti-alias alpha-tested shapes where the view is multisampled
  (`--msaa`, default 4; `nvf_assets::soft_edges`, `nif_material.wesl`, and `tree.wesl` for
  leaf cards and tree billboards): the fragment covers a share of the pixel's samples that
  is 1 at and above the cutoff and falls to 0 within one pixel outside it,
  (alpha − cutoff) / (change of alpha across the pixel) + 1 [chosen]. The depth prepass
  still tests at the cutoff, so every pixel it has written depth for is fully covered and
  nothing behind a shape is wrongly hidden; Bevy's own alpha-to-coverage mode was not
  used because it centres the edge on the cutoff and its depth prepass drops the alpha
  test. A shape's outline grows by at most a pixel. Not softened: object LOD geometry,
  whose atlases hold near-black colour where the alpha is 0 [observed], so a soft edge
  would outline every cut-out in black; and blended shapes, which have no edge to
  soften. Grass uses a wider rule of its own (grass.md §5). Without multisampling
  (`--msaa 1`, and in the water's reflection) the alpha is tested per pixel as before.

  Vertex alpha and the test. Survey of the visible shapes in `Fallout - Meshes.bsa` that
  set the test bit, set the shader's vertex-alpha flag (see "Vertex colours") and have
  some vertex alpha below 1 [observed,
  `crates/fnv-nif/examples/vertex_alpha_test_survey.rs`]: 202 shapes, every one drawn by
  a BSShaderNoLightingProperty. 163 are additive (SRC_ALPHA / ONE; 154 with a threshold
  of 0-63, 9 with 64 or more), 38 SRC_ALPHA / INV_SRC_ALPHA (30 with 0-63, 8 with 64 or
  more) and 1 is only tested (`nvgiantrat.nif`, `NV_GiantRat:3`, threshold 70, vertex
  alpha up to 0.60). 12 shapes (all additive, threshold 0-63) have every vertex above
  their threshold, 188 some and 2 none: the test, if it compares the vertex alpha, cuts
  into 190 of them.
  - Seven of the eight SRC_ALPHA / INV_SRC_ALPHA shapes with a threshold of 64 or more
    name no texture and are black on every vertex: shadow strips laid along the inner
    edges of the destroyed Old Town buildings (`architecture\urban\oldtown\`), flags
    0x12ED, threshold 128, both decal bits set. `otbldgdestroy01nv.nif`, shape
    `OTBldgDestroy01NV:5` (the house at the NCR camp in Primm, cell −16,−14): 561
    vertices, alpha 0.55 on 211, 0 on 169, 0.3 on 100, 0.35 on 56, 0.75 on 12, 0.25 to
    0.27 on 13; all UVs are (0, 1). 223 vertices are above 128/255. The others:
    `otbldgdestroy02nv.nif` (223 of 561 above), `otbldgdestroy01top01nv.nif` (136 of
    769), `otbldgdestroy01top02nv.nif` and `otbldgdestroy02top02nv.nif` (18 of 195
    each), and `otbldgdescorner01.nif` and `otbldgdescorner03.nif`, whose highest vertex
    alpha is 0.45: none of their 24 and 14 vertices is above. The eighth is textured:
    `terminals\nv_reflectron_ui.nif`, `NV_Reflectron:2`, threshold 64, alpha up to 0.65.
  - What the game draws there is not known: no game frame of these buildings was
    compared. Read literally, blend and test both apply to a fragment's alpha
    [documented: nif.xml AlphaFlags], and the alpha the blend uses includes the vertex
    alpha, so the strips would darken the wall by 50 to 75% where their alpha is above
    128/255, end there with a hard edge, and the two corner pieces' strips would not
    show at all [hypothesis]. The other reading, that the gradients show in full as
    soft shadows down to alpha 0, needs the test not to see the vertex alpha.

  The renderers follow the literal reading for shapes that set blend and test with a
  threshold of 64 or more, use vertex alpha and have some vertex alpha below 1: they
  are drawn blended, and fragments whose alpha (material × texture × vertex alpha ×
  view-angle falloff) is below the cutoff are dropped (`nif_spawn::blend_cut`,
  `NifSurface::alpha_cut`). By the threshold rule above they were alpha-tested, which
  draws the part above the cutoff opaque: a shadow strip of 55% became solid black.
  Additive shapes keep ignoring their test. Most of each strip falls from 0.55 to 0 or
  0.3 across its width, so what is above the cutoff is a line a pixel or two wide in
  the corners of the rooms [observed, `nvforever`, with the strips drawn alpha-tested].
  The strips keep the default view-angle falloff values but do not set the shader flag
  that switches the falloff on (bit 6, see "View-angle falloff"), so the renderers draw
  them without it, and the lines show: in the owner's view of that house they run along
  the ceiling edges, down the room corners and along the skirting, 4,151 pixels of
  1280 × 800 darker than without the strips and none lighter [observed, `nvforever`].
  While the renderers applied the falloff to every blended shape of this property
  (alpha × cosine), 0.55 × cosine was above the cutoff only within 24° of facing a
  strip, and from that viewpoint no strip was drawn at all [observed].

  The depth prepass has to test the same alpha as the main pass. Bevy's tests material
  alpha × texture alpha without the vertex alpha, so an alpha-tested shape with vertex
  alpha below its cutoff wrote depth over its whole area, the main pass then dropped its
  fragments, and what lay behind failed the depth test: the clear colour showed. The
  shadow strips above appeared as flat pale blue frames round the ceilings, door and
  window openings and skirting of the house in Primm [observed: owner's screenshot,
  2026-10-10, and `nvforever --cell=-16,-14 --pos=-62714,-54035,5919 --heading 150
  --pitch -5`]. NIF shapes now use a prepass fragment that includes the vertex alpha
  (`nif_prepass.wesl`); the giant rat's shape is the other one in the archive it
  changes. The same symptom, from vertex alphas that are not numbers, is in "Vertex
  colours" below.

  Static collections can lose the threshold [observed, `fnvtool nif decode`]: the single
  maize plants (`landscape\plants\maize_healthyp.nif`, `maize_withered.nif` and their
  `_group1` models) have flags 4845 (blend, test GREATER) with threshold 124, while the
  merged rows built from them (`scol\scolmaizerow1b.nif`, `scolmaizerow2b.nif`,
  `maizewitheredgroup2b.nif`) have the same flags with threshold 0. So by the rule above
  the single plants are alpha-tested and the rows blended. Their texture `Maize_D.dds`
  (DXT3) is a cut-out with soft edges: 31.8% of its texels have alpha 0, 60.2% alpha
  255 and 8.1% lie between. Blended, the rows look right as long as nothing blended is
  drawn over them. The terrain's texture layers were: they are blended meshes too, a
  blended shape writes no depth, and the layers of the ground behind a row were drawn
  after the row wherever their quadrant sorted as the nearer, which left the corn faint
  and hid the row behind it entirely [observed: Goodsprings, `--pos -70710,-764,8350
  --heading 175 --pitch -8 --fov 75`]. Terrain layers are now drawn before every other
  blended mesh (docs/spec-notes/world.md §"Texture layers"); the rows are solid.
  Whether the game writes depth for blended shapes is open; within one blended mesh
  the triangles are drawn in their stored order.

  One kind of shape is cut by alpha without a NiAlphaProperty: object LOD blocks, whose
  shader sets BSShaderFlags2 bit 2 "LOD_Building" [documented: nif.xml]. The renderers
  alpha-test those at alpha above 128 [hypothesis; evidence in world.md §7 "Objects"].

  **Shadows of see-through shapes** (2026-10-11). Shapes drawn blended, added or
  multiplied (glass, light rays, glows, smoke) cast no shadows from any light, the sun
  and the flashlight included; alpha-tested shapes, and blended ones with an alpha cut
  (`blend_cut`), still do (`nvf_assets::nif_material::see_through`, which gives the mesh
  `NotShadowCaster` when it gets its material) [design, owner's request after the
  playtest: "anything see-through should not be generating a shadow"; the original casts
  no sun shadows from the world at all, world.md §6]. Before, the depth-only shadow pass
  drew every fragment of such a shape with alpha above 0.05, so the Prospector Saloon's
  window rays threw dark wedges onto the wall under the flashlight [observed, `--play
  --cell GSProspectorSaloonInterior`, flashlight on, facing the east windows; picture
  `C:\fnv-private\references\playtest_world\flashlight_rays_before_after.png`].
- **Skin shader type** [observed, `crates/fnv-nif/examples/skin_survey.rs` over
  `Fallout - Meshes.bsa`]: 255 visible shapes have BSShaderPPLightingProperty shader
  type 14 (`SHADER_SKIN` [documented: nif.xml BSShaderType]); the same 255 and no others
  set the "FaceGen" shader flag (bit 10). All but 2 name a head, upper body or hand
  texture under `textures\characters\`. The renderers shade them like other shapes (the
  `_sk` maps beside the skin textures are not used); actors use the type to tell an
  actor's skin from the clothing in the same model (docs/spec-notes/actors.md §3.1).
- **Emissive light** [observed, `crates/fnv-nif/examples/glow_survey.rs` over
  `Fallout - Meshes.bsa`]: of 42,074 visible shapes drawn by a
  BSShaderPPLightingProperty, 2,687 have a non-black NiMaterialProperty emissive colour;
  2,030 of those carry a `_g` texture in texture-set slot 2 (13 `_e`, 119 other names,
  523 none). Glow maps are black except for the parts meant to shine: e.g.
  `NV_24-Sign_e.dds` (Goodsprings "OPEN 24 HOURS" sign) holds the red, blue, white and
  yellow neon tubes on black, while the diffuse map under them is flat bright paint.
  Face-gen and hair shapes use slot 2 for `_sk`/hair maps, all with a black emissive
  colour. Emissive multipliers range from 0 to 12 (906 at 1.0, 484 at 2.0, 463 at 10.0).
  The renderers emit emissive colour × emissive multiplier × glow map (slot 2, unless the
  face-gen or hair flag is set) × the diffuse texture and vertex colour, i.e. emissive
  light is modulated by the surface like lit light is [hypothesis; classic Gamebryo adds
  the material emissive to the lighting sum that multiplies the texture]. Evidence: the
  Prospector Saloon's string lights (`nv_prospectorsaloon.nif`, shapes `QueenPalm03:06:0`
  and `Shape03:0`, emissive white) put their *normal* map in slot 2
  (`NVQueenPalmLights_n.dds`, flat (128, 128, 254)); their diffuse
  `NVQueenPalmLights.dds` holds pastel bulbs (peach, lilac, green, pink, teal, yellow,
  white, blue) and a dark cord, vertex colours are white. Emitting glow alone made the
  whole string, cord included, lavender; in game the cord is dark and the bulbs glow
  white, light blue and purple [observed, owner's close-up screenshot], which is pastel
  × lavender. The OPEN 24 HOURS tubes (diffuse red/blue/yellow under an alpha-tested tube
  outline, glow map the same colours) and the SALOON sign's glowing O keep their look.
  Applying the emissive colour to the whole surface, untextured (the first rule), turned
  the OPEN sign white. The two string-light shapes are the only visible shapes with a
  `_n` texture in slot 2 and a non-black emissive colour; their shader flags equal the
  OPEN sign's (0x82000101), so no flag tells them apart [observed].
  The initial reading left BSShaderNoLightingProperty shapes unlit and ignored the
  emissive colour: 2,663 of them have a black emissive colour (hangar lights, casino
  stairs) yet show in game,
  so it is not a plain colour multiplier there. How their multipliers (up to 10, e.g.
  the saloon's neon tubes ×7.5) and the 717 NiMaterialColorController / 317
  BSMaterialEmittanceMultController blocks (blinking bulbs) affect brightness is
  addressed by the unlit rules below [design: `nif_spawn.rs`, `material_anim.rs`].
- **Emissive multiplier of unlit shapes** (2026-10-23). Gomorrah's neon
  (`architecture\strip\gomorrahlights.nif`, `gomorrahlightslowerlevel.nif` [observed,
  `crates/fnv-nif/examples/fx_audit.rs`]) is BSShaderNoLightingProperty shapes on
  `StripLights01.dds`, SRC_ALPHA / INV_SRC_ALPHA, emissive white × multiplier 25
  (`LightsNeon`, `GamorrahLowLights`), 15 (`OverHangGlow`), 5 or 3. In the original's
  night frames of the Strip (owner's HDR on) such neon burns white at the core and
  throws large coloured halos; drawn at its surface colour it was a dim strip. Rule in
  the renderers [hypothesis from those frames; engineering choice where it goes beyond
  them]: in an HDR view (the light glow, docs/design/graphics-enhanced.md §3.11) an
  unlit shape is drawn as bright as its emissive light, surface colour × emissive colour
  × multiplier, and never darker than its surface colour (so the 2,663 unlit shapes with
  a black emissive colour keep their look). Multiply shapes are left alone. Without the
  light glow (LDR view, tonemapped in the shader) nothing changes.
- **Emissive colour of unlit shapes** (2026-10-23). Dead Money's wall lantern
  (`nvdlc01\clutter\lights\nvdlc01_sconceon.nif`) has an additive glow card
  (`SconceON:4`, `fxsoftglowspot01.dds`, SRC_ALPHA / ONE, view-angle falloff, 58 units
  across around the 14-unit glass) whose NiMaterialProperty emissive is warm (0.85, 0.65,
  0.47) × 1. Drawn by the rule above (never darker than the surface colour) the white
  texture won and the glow was white beside an orange lamp; the owner: "the lamp is
  orange but the light beam is white, it does not match". Survey of the unlit
  (BSShaderNoLightingProperty) shapes [observed, `fx_audit.rs`]: `Fallout - Meshes.bsa` and
  `Update.bsa` 7,147 (3,305 black emissive, 2,514 at or over 1 in every channel after
  the multiplier, 291 grey under 1, 1,037 coloured under 1); `DeadMoney - Main.bsa` 356
  (174, 153, 7, 22). Among the grey ones Lucky 38's base and stair lights store 0.1 ×
  1.25, yet are bright in game, so the colour cannot simply multiply the texture. Rule
  in the renderers [hypothesis]: a non-black emissive colour tints an unlit shape by its
  hue (the colour over its brightest channel, a display colour, raised to the display
  gamma), and its brightness is max(1, multiplier × brightest channel); a black colour
  leaves the surface colour. The lantern's card turns warm orange, Dead Money's
  holograms ((0, 0.87, 1) × 6) cyan, grey colours change nothing, Gomorrah's white × 25
  is as before. Without the light glow the result is held to the surface colour. An
  additive shape takes the brightness in display values (raised to the display gamma
  like its hue; "Additive blend"); additive shapes skip the image space's Emissive
  Mult (2026-10-25, "Additive blend" below).

  Until 2026-10-23 neither rule reached unlit shapes: Bevy's
  `pbr_input_from_standard_material` fills the emissive light in only for lit materials
  [observed: bevy_pbr 0.19 `pbr_fragment.wgsl`], so an unlit shape's stayed black. The
  shader now reads the material's emissive light for unlit shapes itself.
- **Unlit shapes that are pictures, and cut-out signs** (2026-11-06, #1540).
  [observed: the original and NVForever compared at matching 1280×720
  views, private jobs `ov_strip_n`, `ov_strip_d`, `ui_talk_house`]: House's screen and
  the Securitrons' faces retain detail in the original, while main burns them into a
  white-green blur; holding their colour times multiplier to 1 matches those frames. Their
  BSShaderNoLightingProperty shapes are opaque, store multipliers 10–45 and lack
  External Emittance (shader flag bit 29); the Strip neon that must burn has the flag.
  The Tops marquee (`nv_thetops-casino_neon.nif`, `Yellow_Neon1`, cut-out, yellow ×4,
  with the flag) has distinct bright stripes and a small halo in the original; main
  draws a flat yellow blob many times their width. The Lucky 38 stair lights
  (`lucky38stairlights.nif`, blended, ×1.25, without the flag) become white slabs on
  main at the Strip's night emissive scale, 2.67 over the worldspace's.

  Rules [hypothesis, fitted to those frames], only for unlit shapes:
  an opaque surface without External Emittance is a picture, with its colour times
  multiplier held to at most 1 (a Securitron's screen stores grey 0.05 × 30, and shows
  its face at the texture's brightness, as the original does); House's authored opaque screen remains a picture when its actor
  alpha gate selects a mask [design: `nif_spawn.rs` `shape_alpha_mode`].
  Blended and additive surfaces keep their multipliers: laser sights at ×40, muzzle
  flashes and beams without the flag must retain their light. Unlit shapes without
  the flag skip the image space's Emissive Mult, as additive shapes already do.
  An authored cut-out unlit surface, whatever its multiplier and flag, also skips that
  scale and is drawn on the plain path of blended shapes instead of `hot_light`, which
  clips each channel and would turn the Lucky 38's pink letters (`L38sign_anim:0`, × 6)
  to one neutral white.

  [design: `nif_spawn.rs` `shape_emissive_mult`, `shape_emissive_rule`]: the picture
  hold applies in `shape_emissive`, to the look a shape is built with; `Props::emissive_parts`
  gives material animation's `Look` the stored multiplier, so a Securitron's idle track
  (0.032 × 30) and its screen-change flash (0.99 × 30) draw as authored. `NifSurface` and
  `NifSurfaceUniform` carry `emissive_rule` (x 1 skips the image-space scale; y 1 takes
  the plain path). Both shaders read the same struct from
  `nif_surface_bindings.wesl`, in the Rust uniform's field order. Materials built by
  hand, including overlays, default to zero and keep their previous behaviour.

  [observed 2026-11-07: the same views, with the light glow off]: the Lucky 38's and the
  Tops' letters read and the night sky is dark, so what ran them together was the glow;
  its settings are in world.md section 6 "HDR and bloom".

  Survey of the 7,121 visible shapes with the blend bit in `Fallout - Meshes.bsa`
  [observed, `crates/fnv-nif/examples/blend_survey.rs`], by source / destination factor:
  4,616 SRC_ALPHA / INV_SRC_ALPHA, 1,730 SRC_ALPHA / ONE, 584 ZERO / SRC_COLOR, 182 ONE /
  ONE, and 9 others (ONE / ZERO 4, ZERO / INV_SRC_ALPHA 2, SRC_ALPHA / SRC_COLOR 1,
  SRC_COLOR / ZERO 1, ZERO / ZERO 1). The 584 multiply shapes:
  - 578 are drawn by a BSShaderNoLightingProperty. 571 of them name no texture and carry
    vertex colours: overlays laid on interior kit pieces (118 under `dlcanch\dungeons`,
    110 `dlcpitt\dungeons`, 83 `dungeons\utility`, 40 `clutter\consoles`, ...). Example:
    `utlhallsm1waypipe01.nif`, shape `UtlHallSm1WayPipe01:8`, 46 vertices, white on 31
    (no change) and browns such as (0.54, 0.40, 0.32) and (0.33, 0.13, 0) on the rest: a
    rust stain on the wall under the pipes. The other 7 have a texture (a shade
    gradient, the vault projector's slides, a scope reticle).
  - 5 are interface shapes (TileShaderProperty).
  - 1 is drawn by a BSShaderPPLightingProperty, i.e. lit: `##SkullCap:0` of
    `creatures\dog\rex_skullcap.nif`, the glass over Rex's brain. It is also the only one
    of the 584 with a non-black emissive colour.

  The skull caps [observed, `fnvtool nif decode`, textures decoded from the archives]:
  - Rex: texture `rex\skullcap.dds`, one flat colour, sRGB (49, 87, 117); a flat normal
    map with alpha 50/255; environment cube `chrome_e.dds`; no glow map. Emissive colour
    0.190 × multiplier 12 = 2.28. A NiMaterialColorController with target colour 3
    (TC_SELF_ILLUM [documented: nif.xml MaterialColor]) keys the emissive colour between
    0.128 and 0.208 (39 keys, a 4.0 s loop, mean 0.158). The brain under it
    (`##BrainMesh:0`) is opaque, emissive 0.099 × 1 keyed 0.088–0.127 the same way.
  - The cyberdogs' caps lie in `OldWorldBlues - Main.bsa` (the two files were decoded by
    name; that archive was not surveyed). `cpskullcap.nif` has Rex's properties with a
    flat texture of (117, 49, 52), the same texture again in the glow slot, and emissive
    0.816 × 12 = 9.79. `cmskullcap.nif` is unlit (BSShaderNoLightingProperty) with a flat
    texture of (117, 102, 49).

  How the renderers draw it [hypothesis]. The shape's colour is a factor, formed the way
  the game is taken to form a lit colour, on display values: texture × (light +
  emissive colour × emissive multiplier × glow map), and limited to 0..1 when it is
  written. So
  - Rex's cap, in the dark: 2.28 × (0.19, 0.34, 0.46) = (0.44, 0.78, 1.05), limited to
    (0.44, 0.78, 1): a light blue glass. Light on the cap adds to the factor (the glass
    clears where it is lit), and the keyed emissive colour would move the red factor
    between 0.30 and 0.48;
  - the police cyberdog's: 9.79 × (0.46, 0.19, 0.20)² = (2.06, 0.36, 0.41), limited to
    (1, 0.36, 0.41): a red glass;
  - the military cyberdog's: its texture, (0.46, 0.40, 0.19): a yellow glass.

  That the three come out as blue, red and yellow glass of about the same depth is the
  support for this reading: without the limit the red cap would double the brightness
  of the brain behind it, and without the glow map in the product it would be clear.
  No game screenshot was compared.

  In the engine (`nif_material.wesl`, CPU mirror `nif_material::multiply_factor`): the
  lit colour and the emissive light are each taken to display values (gamma 2.2), summed,
  limited to 0..1 and taken back; the emissive value of a multiply shape is raised to
  the gamma first, so that the product with the linear texture is the display-value
  product above (`nif_spawn::shape_emissive`). The factor is neither fogged nor
  tonemapped: it is not light, and what lies behind already carries both [hypothesis].
  Unlit multiply shapes draw their texture × vertex colour as the factor.

  Before, the emissive light was added as light (2.28 × the *linear* texture, a factor
  of (0.07, 0.22, 0.41)) and the factor then went through fog and the tonemapper. In
  the King's School of Impersonation the frame was 0 over the whole cap and the brain
  could not be seen [observed, `nvforever` screenshots]; with the rule above the brain
  shows through a blue tint.

  Open: the emissive colour controllers are not played (the stored colour is used);
  the environment map is not drawn (it would add reflections to the factor); whether the
  game limits the colour depends on its frame format; vertex colours enter as linear
  factors here as on every other shape, which leaves the overlays lighter than a product
  of display values would; ZERO / INV_SRC_ALPHA (two scope lens shapes, darkening by
  alpha) is still drawn as a multiply by colour. The overlays are decals: see "Decal
  flags" below (an earlier version of this note had them in the plane of the surface
  under them and flickering against it; the survey there shows neither).
- **Additive blend** (NiAlphaProperty with the blend bit and destination factor ONE;
  1,730 SRC_ALPHA / ONE and 182 ONE / ONE shapes, see "Multiply blend"): the shape's
  colour × alpha is added to the frame [documented: nif.xml AlphaFlags].

  The Prospector Saloon (`GSProspectorSaloonInterior`) shows how much it adds
  [observed, `fnvtool world refs`, `fnvtool nif decode`, textures decoded from the
  archives]:
  - 12 window rays (`IndFXLightRays01`, `IndFXLightRaysLeft01`,
    `IndFXLightRaysNLeftSm01`, scale 0.34 to 0.42) along the window walls. In
    `effects\ambient\industrial\indfxlightrays01.nif` the one drawn shape is
    BSShaderNoLightingProperty (flags 0xA2000148: vertex alpha, falloff bit 6, external
    emittance), SRC_ALPHA / ONE, texture `FXSoftGlowBeamMulti.dds` (128 × 4, white,
    alpha 0 to 255 across the beam), falloff 0.985 → 0.174, opacity 1 → 0. Its 258
    vertices are white with alpha 0 (172) or 0.1 (86): a ray adds at most a tenth of
    white.
  - 10 lamp glows (`FXBasicGlowBillboard`, scale 0.2 and 0.28) on the lanterns and
    chandeliers. `effects\ambient\fxbasicglowbillboard.nif`: a billboard quad,
    SRC_ALPHA / ONE, texture `FXPartTinyGlowy.dds` (64 × 64, grey 189 to 255, alpha 0 to
    252, mean 20: a small bright core and a long faint tail), vertex colours white, no
    falloff bit.
  - In game at 8:00 (owner's screenshots, 2026-10-11): the rays are faint streaks, seen
    near the jukebox window only; a lantern on a booth table has a small glow on its
    glass and no ring on the table.

  How the renderers add it [hypothesis]: the game adds colour × alpha to display values
  (a DirectX 9 frame without sRGB blending is taken to hold display values; the same
  reading as "Multiply blend"). Bevy adds to linear light, where 0.1 added to black
  shows as 0.35 on the screen. The linear light a display-value sum needs depends on the
  frame behind the shape: for a white ray of 0.1, 0.1^2.2 on black, 0.032 on a wall of
  display value 0.15, 0.1 on one of about 0.45.

  Rule (2026-10-23, replacing the fixed power below): an additive shape reads the opaque
  frame behind it, `behind` (Bevy's view transmission texture: the copy of the frame
  after the opaque pass that water and refracting shapes already read), and adds
  `(behind^(1/2.2) + colour^(1/2.2) × alpha)^2.2 − behind`: the display-value sum, as
  linear light (`nif_material.wesl` `add_display`, CPU mirror `nif_material::add_display`).
  The colour is fogged first (at alpha 1). The emissive colour × multiplier of an additive
  shape counts in display values (raised to 2.2 before the shader, as for "Multiply
  blend"; `display_emissive`; for an unlit shape its brightness under "Emissive colour of
  unlit shapes"), so the texture × multiplier × alpha is the game's term.
  The sum is held to display 2.0 (`ADD_DISPLAY_MAX`) or the frame's own value if
  brighter [engineering choice: it lets the Big MT's sky beams, display 1.5 at the core,
  through whole and keeps a multiplier of 30 (the Securitron's screen flash) from adding
  thousands of times white in the HDR view]. Bevy copies the frame only for a view with
  something in its transmissive phase, so the engine keeps a material that reads the
  copy on a triangle of no area in every render layer (`TransmissionAnchor`); otherwise
  the shader would read a white stand-in. Cost: one copy of the frame per view and frame.

  Why the fixed power (alpha^1.5, `ADD_ALPHA_POWER`, fitted in the Saloon) was replaced
  [observed, `nvforever` against the original's frames, `C:\fnv-private\references\owb`
  and `fx_audit`, 2026-10-23]:
  - The Big MT's sky beams (`nvdlc03\clutter\imposters\nvdlc03imposterbeamblue.nif`:
    two crossed planes, SRC_ALPHA / ONE, texture `NVDLC03glowbeamblue.dds` 32 × 4 of
    colour (0, 207, 255) with alpha 16 to 255 across the beam, vertex alpha 0.15 on the
    centre line and 0 at the edges, emissive white × 10, view-angle falloff) are broad
    bright cyan beams in the original: over a sky of (17, 17, 33) the core reads (102,
    171, 204) halfway up. The power took 0.15 to 0.058 and the multiplier did not reach
    the shader at all ("Emissive multiplier of unlit shapes"): the beams added about (8,
    20, 20). With the display-value sum they are bright cyan beams as in the original.
  - The Strip's lamp beams (`effects\ambient\fxlightbeamnvstrip.nif`: white, vertex
    alpha 0.25 at most, multiplier 1) on the night sky: the game adds 0.25 at most, the
    power added 0.39 (fx-lighting-audit-2026-10-23.md item 4, "too strong at night").
    Now 0.25; in the frames of the Strip the beams are the faint haze of the original.
    That held only before the image space's Emissive Mult reached unlit shapes (#643):
    at night the Strip's adapter NVTheStripNightIS sets Emissive Mult 4 (the Mojave's
    1.5), and raised to the display gamma for additive shapes it took the beams ×8.7 in
    linear light, on top of their emittance (stripStreetLight (150, 198, 102) × fade 4).
    The gate lamp's beams (0x0015DB0B, 0x0015DB0E) then added +65 grey levels in the
    gate's box and blew out white, where the original's frames (owner's, 2026-10-25, and
    `fx_strip_b`) show no cone at all [observed]. Rule since 2026-10-25 [hypothesis]:
    additive shapes do not take the image space's Emissive Mult (`nif_material.wesl`);
    the gate's box is then 94 against the original's 76 (79 with the beams not drawn).
  - X-13's door lamps (`fxlightbeambrt03`) show as two beams, as in the original; Primm
    and the Strip's other effects are otherwise unchanged.
  - The Prospector Saloon's window rays, from the camera of the table below (shown minus
    `--hide indfxlightrays --hide fxbasicglow`): mean 2.8 grey levels added, 8 or more
    on 14 % of the frame, the display-value-sum target's own numbers (alpha^1.5: 2.0 and
    10 %).

  Open: overlapping additive shapes. Each is summed with the opaque frame alone, so
  where several overlap (crossed beam planes) the sum falls short of the game's: n equal
  faint shapes on black add n × d^2.2 instead of (n × d)^2.2 (the Saloon's rays, which
  overlap little over lit walls, did not show it). Exact would
  be adding every additive shape into a buffer of display values of its own and adding
  that to the frame once. The Sink's balcony force fields
  (`nvdlc03x13fxforcefield01.nif`, emissive × 1.2 and 2.0, animated alpha) are brighter
  than before but still faint panels where the original shows a solid cyan band; the
  original's frame was taken from above the ring and ours from the balcony, so they have
  not been compared from the same place.

  The fitted power it replaced, kept for the record: the engine raised the alpha of
  additive shapes to a single power, 1.5; a ray of 0.1 then added 0.21 on black (game
  0.1) and 0.05 on a wall of 0.3 (game 0.1). Power 2.2 would be exact on black but add
  0.01 on that wall.

  The 182 ONE / ONE shapes (source factor ONE) add their colour whole: their alpha is
  not part of the blend [documented: nif.xml AlphaFlags source blend mode 0 ONE], so the
  engine draws them with alpha 1 (only the view-angle falloff still fades them
  [hypothesis]; `NifSurface::add_whole`, 2026-10-19). Before, their alpha took them
  out almost entirely where the texture's alpha is low: the Securitron's screen noise
  `whitenoise01.dds` has mean alpha 3 of 255 [observed] (effects-animation-audit.md §2).

  The power is fitted [observed, `nvforever` screenshots from the owner's camera, and
  the same view with `--hide indfxlightrays --hide fxbasicglow`]. Target: the view
  without the shapes, plus, in display values, the linear light the shapes added when
  drawn with their stored alpha (which is the shapes' colour × alpha: the game's sum).
  In grey levels added to the frame (0 to 255, mean of the three channels), mean (max)
  over the frame:

  | view | stored alpha | alpha^1.5 | alpha^2.2 | display-value sum (target) |
  |---|---|---|---|---|
  | window rays, `--pos 179,-780,3575 --heading 15 --pitch -12` | 9.3 (83) | 2.0 (42) | 0.2 (20) | 2.8 (34) |
  | booth table from above, inside the ray volumes, `--pos -333,-570,3577 --heading 270 --pitch -55` | 47.1 (129) | 14.5 (93) | 1.7 (65) | 10.8 (71) |
  | the same, lantern glow only | 0.4 (114) | 0.2 (91) | — | 0.1 (55) |

  The target frame of the rays matches the owner's game frame in look: faint streaks at
  the jukebox window, no haze over the room. With the stored alpha the rays read as
  solid shafts and a haze (8 or more grey levels on 31% of the frame; 10% with power
  1.5, 14% for the target), and the lantern glow drew a ring about 80 pixels across on
  the table, the edge of its texture's faint tail; with power 1.5 the streaks are there, and the lantern keeps a small halo on its
  glass with no ring. Pictures: `C:\fnv-private\references\light_rays\`.

  Open in 2026-10-11 (the display-value sum against the opaque frame, above, now covers
  one shape at a time): an exact display-value sum (drawing additive shapes into a buffer of their own
  and adding it to the frame in display values afterwards; additive shapes commute, so
  the sum would be exact up to the blended shapes in front of them); whether the
  game's streaks are as strong as ours (the owner's frame and ours differ in field of
  view, so no pixel was compared); additive particles (flames, muzzle flashes) are drawn
  by the same material and change the same way, and none was compared with the game;
  SRC_ALPHA / INV_SRC_ALPHA shapes of lit shaders are still blended in linear light
  (they agree with the game at alpha 0 and 1 and differ between; unlit ones: "Alpha
  blend of unlit shapes"); ONE / ONE shapes are drawn as SRC_ALPHA / ONE, as before.
- **Alpha blend of unlit shapes** (SRC_ALPHA / INV_SRC_ALPHA on a
  BSShaderNoLightingProperty shape: smoke and mist sheets, dust, and every particle
  system, which `nvf_assets::particles` draws with the same material). The blend is
  colour × alpha + behind × (1 − alpha) [documented: nif.xml AlphaFlags]; the game forms
  it in display values [hypothesis, as "Additive blend"], Bevy in linear light. Light
  smoke over a dark room is where they differ most: a grey of display 0.5 at alpha 0.3
  over black is 0.15 in the game and 0.29 in linear light.

  The Bison Steve Hotel's lobby (`BisonSteve01`, the main entrance) [observed, `fnvtool
  world refs`, `fnvtool nif decode`]: 2 `FXMistLow01` and 1 `FXSmokeWisps01`
  (`effects\ambient\fxmistlow01.nif`, `fxsmokewisps01.nif`), unlit, SRC_ALPHA /
  INV_SRC_ALPHA, texture `SmokeWispsTile.dds`, vertex alpha flag, vertex colours grey
  0.5 with alpha 0 to 0.5 (mean 0.11 and 0.17). Owner's screenshots of the same view
  (2026-10-23): the game shows the room clearly with almost no haze; `nvforever` drew a
  grey fog over the whole room.

  How the renderers blend it [hypothesis, fitted]: the colour of an unlit
  alpha-blended shape is scaled by √alpha in the shader, so the blend adds colour ×
  alpha^1.5 while the share of what lies behind stays 1 − alpha
  (`nif_material.wesl` `blend_colour_scale`, CPU mirror
  `nif_material::blend_colour_scale`). The power was chosen by the display-value error
  of `colour·alpha^p + behind·(1 − alpha)^q` against the display-value blend, over
  behind 0 to 1, colour 0.15 to 0.85 and alpha 0.01 to 0.6 (a grid, root mean square
  in display values): p = 1, q = 1 (the old blend) 0.052, and 0.096 where behind is
  0.2 or darker; p = 1.5, q = 1 0.037, and 0.037 there; the best, p = 1.4, q = 1.1,
  0.036. The same power as additive shapes; the share behind is kept so smoke still
  hides as much of the room as before. Lit blended shapes (glass, hair edges) are not
  changed. Open: no frame was measured against the game's yet.
- **Soft intersections** (unlit effect shapes that meet an opaque surface). The Strip's
  street lamp beam `effects\ambient\fxlightbeamnvstrip.nif` [observed, `fnvtool nif
  decode`, `fnvtool world refs --world TheStripWorldNew`] is two crossed vertical panels
  (normals -Y and -X), 1,800 units from the lamp head down, plus a glow disc under the
  head and a glow shell around it; BSShaderNoLightingProperty, SRC_ALPHA / ONE, flags
  0xA2000148 (vertex alpha, falloff bit 6, external emittance, z test), vertex alpha 0.25
  at the top falling to 0 at the bottom. Its 32 references (`StripLightBeamAct`) are
  placed at scale 0.6, about 460 units above the road, so the panels pass about 620
  units into the road while still a third as bright as at the top. Drawn with a plain
  depth test, the road cuts them along a straight line (a "+" seen from above). The
  owner's game frame of the same street at night (2026-10-23) shows a faint haze above the
  bulb and a soft pool on the road with no line where the panels meet road or pavement
  [observed]. How the game hides it is not known (no depth-fade setting is documented for
  this shader).

  Burning shapes are not faded (2026-10-25): an unlit shape whose emissive colour ×
  multiplier is over 1 in its brightest channel (neon, glowing signs) keeps its full
  strength against the surface it is mounted on [engineering choice]. Gomorrah's porch
  tube (`gomorrahlightslowerlevel.nif` `OverHangGlow:0`, white × 15, a flat strip tilted
  45° a few units off the fascia, its alpha pulsing 1 ↔ 0.18 on a 2 s cycle by the
  model's SpecialIdle) was faded to a dull red line; the original burns it bright with a
  halo (owner's frames). Whether the original pulses it is not known: its frames show it
  bright.

  What the renderers do [engineering choice, nif_material.rs module notes "Soft
  intersections"]: every unlit shape that adds or blends (additive SRC_ALPHA / ONE and
  ONE / ONE, SRC_ALPHA / INV_SRC_ALPHA; light beams, glows, smoke and dust sheets and all
  particle systems, and since 2026-10-23 decal-flagged ones; not multiply shapes or
  refracting shells) fades out linearly
  over `SOFT_DISTANCE` (160 units, view depth) in front of the opaque surface behind it,
  read from the view's depth prepass. Where the shape lies along that surface (the
  shape's and the surface's facings, both from screen derivatives, within about 25°;
  full fade-out from 18° to 32°) it is not faded, so light spots and haze cards laid on
  a floor or a wall stay. The rule is by blend and shader only: no model is named. Survey
  of the shapes it covers in the placed models of `FalloutNV.esm` [observed,
  `crates/fnv-nif/examples/fx_audit.rs` over `Fallout - Meshes.bsa` and `Update.bsa`,
  joined with `fnvtool world refs` of every worldspace and interior]: 316 additive
  shapes in 145 models (7,624 references), 1,129 unlit SRC_ALPHA / INV_SRC_ALPHA shapes
  in 819 models (editor markers among them, which are not drawn), 1,262 particle
  systems.
- **Decal flags** (BSShaderFlags bit 26 `decal_single_pass` and bit 27
  `dynamic_decal_single_pass`; BSShaderFlags2 bit 21 `alpha_decal` [documented: nif.xml,
  schemas/nif/scene.json]). Survey of the visible shapes in `Fallout - Meshes.bsa`
  [observed, `crates/fnv-nif/examples/decal_survey.rs`]:
  - 2,130 shapes in 1,517 files set a decal bit: 2,089 both, 41 only bit 26, none only
    bit 27. No visible shape sets `alpha_decal`. All 2,130 also set bit 31
    (`zbuffer_test`); 1,958 set `zbuffer_write` (BSShaderFlags2 bit 0) and 172 do not.
  - By shader property: 1,435 BSShaderPPLightingProperty (lit), 695
    BSShaderNoLightingProperty; none of any other type.
  - By blend: 1,301 alpha-tested only, 458 SRC_ALPHA / INV_SRC_ALPHA, 336 multiply, 20
    additive, 15 neither blended nor tested. So the flags are not tied to a blend mode.
    Of the 578 multiply shapes of BSShaderNoLightingProperty (see "Multiply blend"), 336
    set the bits and 242 do not.
  - What they are: dirt, stains, cracks, road markings, signs and insignia on kit
    pieces, most under `dlc03\architecture` (250), `architecture\urban` (244),
    `dlcanch\dungeons` (214), `dungeons\utility` (125), `dlcpitt\dungeons` (110) and
    `dungeons\caves` (109). 120 are alone in their file (e.g.
    `architecture\urban\decals\dirtdecal01.nif`): decals placed in the world as
    objects, which the editor's guidance puts one unit above their surface
    (level-design.md §7 "Clutter settling").
  - Where they lie. For each flagged shape with another shape in its file (2,010), the
    survey takes the centres of its triangles, finds the nearest triangle of any shape
    without a decal bit, and takes the median distance along that triangle's normal over
    the centres within one unit of it. **They are not in the plane of the surface: they
    are lifted off it.** 1,473 lie in front of it: 19 by 0.001 to 0.05 units, 288 by
    0.05 to 0.15, 638 by 0.15 to 0.3, 326 by 0.3 to 0.6 and 202 by 0.6 to 1. 16 lie in
    the plane (within 0.001), 59 behind it, and 462 have no triangle centre within one
    unit of another shape. The multiply overlays sit mostly at 0.1 (190 of 336 in the
    0.05 to 0.15 bin) and the alpha-tested decals mostly at 0.2 to 0.3 (490 of 1,301 in
    the 0.15 to 0.3 bin).
  - Example: in `dungeons\utility\hall\utlhallsm1waypipe01.nif` the walls of shape `:0`
    are at y = ±112. The rust stain `UtlHallSm1WayPipe01:8` (multiply, flags 0x8c000000)
    has every vertex at y = ±111.9, 0.1 in front, and the textured decal `:24`
    (alpha-tested, flags 0x8e000101) at ±111.8, 0.2 in front.
  - Shapes that are in the plane: `landscape\nv_rocks\nvn_rocksloperrdrk.nif`
    `NVN_RockSlopeRRED:2` (the ground layer on a rock slope, 204 triangles, every centre
    within 0.0001 of the rock), `architecture\wasteland\highway\overpassroad01.nif`
    `OverpassRoad01:61` (the road surface on the overpass, median 0, at most 0.07), and
    the shadows under the Caravan money (`terminals\nv_caravan\currency-*.nif`, within
    0.002).
  - Control: the flag does not mean "lies on another shape". Of the 6,295 blended
    shapes without a decal bit, 1,018 have a median centre distance of 0.001 or less to
    another shape (697 of 4,146 SRC_ALPHA / INV_SRC_ALPHA, 317 of 1,710 additive),
    against 12 of the 2,130 flagged ones. Those were not looked at.

  What the game does with the bits is not known from the data [hypothesis: it draws
  such shapes with a depth offset towards the camera; an offset of 0.1 units is below
  the step of a 24-bit depth buffer at some distance, so the art's offset alone would
  not hold everywhere, and the shapes in the plane need one at any distance].

  How the renderers draw them [hypothesis]: a shape with either bit gets a depth offset
  towards the camera in the rasteriser (`nif_material::decal_bias`: 64 steps of the
  depth buffer plus one pixel's depth slope), in every pass, whatever its blend mode. In
  the 32-bit reversed depth buffer the constant part is at most 7.6e-6 of the distance
  to the camera (0.008 units at 1,000), so it cannot pull a decal through anything a
  decal's own offset would not. The draw order of blended decals is not changed
  (`StandardMaterial::depth_bias` would move it too, so the offset goes through the
  material's pipeline key instead).

  Effect [observed, `nvf-viewer --screenshot`, before and after]:
  - the lifted decals drew correctly before: the reversed 32-bit depth buffer resolves
    0.1 units at these distances. `utlhallsm1waypipe01.nif` differs in 8 pixels of
    1280 × 800, and two runs of the same build in none;
  - the shapes in the plane change: on `nvn_rocksloperrdrk.nif` the ground layer's edge
    and on `overpassroad01.nif` the road surface's edge and one corner were partly
    covered by the shape under them, and are whole now.

  The dark jagged streaks across `utlhallsm1waypipe01.nif` in the viewer are not the
  stain and not depth fighting. They are the pipe and cable cards of shape `:1`
  (`PipeFlatsSheet01.dds`, alpha-tested with threshold 128, no decal bit), which stand
  1.3 units off the walls (y = ±110.7) and are seen nearly edge-on from the viewer's
  default camera [observed, `fnvtool nif decode`]. The offset leaves them as they were.

  Open: whether the game offsets, and by how much (a screenshot of a wall stain at a
  distance would not tell: the art's offset already holds in our depth buffer); decals
  on a blended surface are ordered by distance only; the 1,018 unflagged blended shapes
  that coincide with another shape; at world coordinates beyond about 65,000 units
  single-precision positions step by 0.008 units, which leaves little of a 0.05 unit
  offset, and no exterior decal was examined there.
- **View-angle falloff** (BSShaderNoLightingProperty, stream version > 26) is stored as
  four floats: a start angle and a stop angle, each the cosine of an angle, and an
  "alpha falloff multiplier" opacity for each [documented: nif.xml].

  A survey of the 7,373 visible shapes drawn by this property in `Fallout - Meshes.bsa`
  and every `* - Main.bsa` found the following
  [observed, `crates/fnv-nif/examples/falloff_survey.rs`]:
  - 5,676 keep the nif.xml defaults (1 → 0 over cosines 1 → 0).
  - 1,528 use other values. They include light beams such as
    `effects\ambient\fxlightbeambrtw02.nif`, the lamp beam on the Prospector Saloon porch
    (0.985 → 0.174, opacity 1 → 0: full within 10° of facing the camera, gone past 80°).
    Force fields, dust storms and light rays also use them. Some fade *in* towards the
    edges (0.866 → 0.174, opacity 0 → 1).
  - 169 have an empty range (start = stop). For 166 of these all four fields are 0,
    e.g. the vertex-coloured quads in `dungeons\office\pit`.

  By blending: 6,230 set the NiAlphaProperty blend bit (2,311 of them with destination
  factor ONE), 183 only alpha-test, and 960 have no NiAlphaProperty.

  How the renderers apply it [hypothesis]: only when the property sets BSShaderFlags
  bit 6 (below), they multiply alpha by the opacity for |N·V|. The opacity is linear in
  the cosine between the two angles and holds its end values outside them. It is applied
  to blended materials only. An empty range or a constant opacity of 1 means no falloff;
  read literally, the all-zero shapes would be invisible.

  Without falloff, the Saloon lamp beam drew as a solid grey cone with hard edges. In
  game it is a faint haze [observed, owner's screenshots].

  The switch: BSShaderFlags bit 6 [hypothesis; no game frame of a shape without the bit
  was compared]. nif.xml names the bit "Empty" for this game. For Skyrim and Fallout 4
  it names the same bit "Use_Falloff" ("Use Falloff value in EffectShaderProperty"),
  in flag sets that keep this game's names at bits 0, 1, 3, 7, 15, 16, 17, 29 and 31
  and whose effect shader property stores the same four falloff fields [documented:
  nif.xml SkyrimShaderPropertyFlags1, Fallout4ShaderPropertyFlags1,
  BSEffectShaderProperty].

  The same survey over `Fallout - Meshes.bsa` alone (5,861 shapes; `--list` prints the
  shapes named here) [observed]: the bit is set on 1,013 of the 1,193 shapes with
  values of their own, on 95 of the 4,495 with the defaults, on 3 of the 166 with an
  empty range and on none of the 7 whose property has no falloff fields (stream
  version 26 or lower). The Saloon lamp beam's one drawn shape sets it, with values of
  its own; the beam file's editor marker sets it too, with the defaults, and is never
  drawn. No shape with a decal bit has values of its own.
  - With values and with the bit (1,013): all but 4 are blended, 675 of them additive.
    Light beams, smoke, dust, muzzle flashes, energy effects.
  - Defaults without the bit (4,400): 3,467 blended, 758 without a NiAlphaProperty, 175
    only alpha-tested. 1,936 name no texture (vertex-coloured shade and shadow
    overlays: the Old Town shadow strips, and the drop shadows of the cards, coins and
    bills of the Caravan table, 258 of the 262 blended shapes under
    `terminals\nv_caravan`); the rest are sign lights (`nv_neon-lights.dds`, `striplights01.dds`), screens,
    holograms and glows. Faded by the cosine, the shadow under a playing card on a
    table would be half gone when seen from 60° off the vertical.
  - Values without the bit (180, in 88 files): 38 hold opacity 1 → 1, which changes
    nothing (scope overlays, ghoul glows; 38 of the 39 shapes with these values). 38
    hold opacity 0.5 → 1 over cosine 1 → 0 (brighter towards the edges), all 38 shapes
    with these values: the fluid and field tubes of the brain and stasis pods. 34 are
    force field panels with 0.985 → 0.174, in files whose glow mesh has the same values
    and the bit. The rest are sun glare, scan lines, projectiles and beams. 51 of the
    180 are in the 29 files that also hold a shape with values and the bit. Nothing
    else separates them from the 1,013: not the blend factors, not another flag bit,
    not the folder. Read with the bit as the switch, these are values left behind
    where the falloff is off. The other reading, that values other than the defaults
    apply without the bit as well, cannot be told from the files; it would draw 140
    of these shapes differently (the blended ones whose values change something).
  - Defaults with the bit (95): 84 blended. 38 are the glass of Enclave light panels
    (`glasstile01.dds`), 26 name no texture, 21 set the decal bits. With the bit they
    fade by the cosine of the view angle.

  The renderers applied the falloff to every blended shape of this property before,
  which faded the 3,467 blended default shapes without the bit by the cosine of the
  view angle. In `nvforever` [observed, same camera before and after]: the Saloon lamp
  beam is identical to the pixel in two views; the neon tubes on the Saloon's letters
  and the lit letters of the Bison Steve sign in Primm, seen at a grazing angle, were
  nearly out and are lit; the Old Town shadow strips show as thin lines
  (see "Alpha", "Vertex alpha and the test").
- **Unlit shader texture over NiTexturingProperty.** A shape may carry both a
  `BSShaderNoLightingProperty` with a file name and an `NiTexturingProperty` with a base
  texture. In Fallout - Meshes.bsa and Update.bsa 3,783 shapes hold both; two name
  different textures [observed 2026-10-27, a one-off survey]: the jukebox's light tubes
  (`clutter/jukebox/jukeboxdirtynv.nif`, `JukeBox01:1`: shader `JBLightTubesClean.dds`,
  texturing `jblighttubesdirty.dds`, listed after it) and a REPCONN mist layer
  (`architecture/repcon/repfxdustmyst.nif`). The shader's texture is drawn and the
  NiTexturingProperty only adds its texture transform [hypothesis: the owner's frames of
  the original show the tubes bright cyan, the clean texture; `nif_spawn.rs`
  `shape_props`]. Switched off, the original's tubes go dark [observed: owner
  2026-10-30, #1079]: NVForever draws them black while off rather than with the dirty
  texture (activators.md §3).
- **Controllers outside sequences.** Many effect meshes animate with controllers attached
  straight to a block, with no NiControllerManager or sequence [observed, `fnvtool nif
  decode`]: `effects/ambient/fxsmokewisps01.nif` (180 placed in FalloutNV.esm), `02`,
  `fxsmokewispslgvol01`, `fxsmokewispslg2x1` and `dlcpitt/effects/dlcpittfxgroundsmoke01.nif`
  (29 placed, among them the camp fire south of Goodsprings) each hold one or two
  `NiTextureTransformController`s on the shape's `NiTexturingProperty`: base texture
  (slot 0), operation TT_TRANSLATE_V (1) or TT_TRANSLATE_U (0), a `NiFloatInterpolator`
  with two keys 0 → 1 over the controller's own start and stop time (13.3 s for the
  ground smoke; 36.67 s V and 44.1 s U for the wisps), flags 72 (Active, loop),
  frequency 1, phase 0. Fields and flag bits are those of NiTimeController
  [documented: nif.xml: bits 1-2 cycle type, bit 3 active, bit 5 manager controlled].
  How `nvf_assets` plays them [hypothesis: the game runs such a controller from load on
  its own clock]: every active controller without the manager-controlled bit that moves
  the base texture or drives a `NiMaterialProperty`'s alpha becomes a material track
  with its own cycle (`model::free_material_tracks`, `anim_play::OwnCycle`); a model
  with no Idle sequence gets a clip of its own for them, which loops over the longest
  cycle while each track keeps its own time. The texture moves by `uv + value`, as for
  the sequences' offset tracks (activators.md §2); with the ground smoke's UVs (V about
  0.84–1.0 at the foot) the pattern rises. Effect models (explosions, impacts,
  muzzle flashes) play all of these, active or not: explosion-effects.md §2. Placed
  objects play moving transform controllers (effects-animation-audit.md §3) and
  visibility controllers ("Visibility controllers" below) outside sequences too;
  particle systems inside ordinary models: particles.md.
- **Visibility controllers.** A `NiVisController` on a node, with a `NiBoolInterpolator`
  whose `NiBoolData` keys say shown (1) or hidden (0) over time [documented: nif.xml],
  shows and hides that node and everything under it. Two kinds on placed models
  [observed, `fnvtool nif decode` over `Fallout - Meshes.bsa`, refs from `fnvtool world
  refs` over FalloutNV.esm]:
  - **In the Idle sequence**: `effects\ambient\fxelectricarc01.nif` (25 placed, in vault
    interiors: 11 in Vault22d, 5 in 2EOVault21, …) and `02.nif` (4 placed) hold an Idle
    (loop, 8.77 s and 8.03 s) whose controlled blocks include a `NiVisController` on the
    node `BrightSpots01`/`02` (the controller's flags 108: manager controlled), with 265
    and 243 keys one frame (1/30 s) apart, 25 and 29 of them shown: the bright spots at
    the arc's ends blink on about one frame in ten. The same Idle moves `Electricity01`
    and scrolls its texture. `gomorrahlightslowerlevel.nif` (1 placed) has an Idle visibility track
    of the same kind [observed by the effect-animation audit of 2026-10-23; not decoded
    again here].
  - **Running by themselves**: `effects\ambient\fxdustmeshtube01.nif` (1 placed,
    FXDustMeshTube01 ref 0017175D in WastelandNV south of Goodsprings) holds one
    `NiVisController` on its node `DustTube`, flags 72 (Active, loop), start 0, stop
    56.67 s, frequency 1, phase 0, no sequence; its 14 keys show the tube, hide it from
    4.03 s to 7.43 s, show it to 17.47 s, hide it from 17.53 s to 29.8 s, show it to
    43.93 s, hide it from 43.97 s to 50.47 s and show it to the loop's end: about 60 %
    of the time shown.

  How `nvf_assets` plays them: every clip a model plays (`anim_play::Playing`) carries
  the visibility tracks of its sequence, sampled at the sequence time, and those of the
  Active, not manager-controlled `NiVisController`s outside sequences
  (`model::free_visibility_tracks`, `Clip::free_visibility`), each on its own cycle from
  the clip's start (`OwnCycle`, as the free material and transform tracks); a model
  with only these gets a clip of its own. `anim_play::advance_instances` writes the
  node's `Visibility` when a track's value changes (`Playing::show`); between keys the
  last key holds and before the first key the first holds [hypothesis: keys step, they
  do not blend; `BoolTrack::at`]. A node the file hides that a track shows is built
  and starts hidden (`vis_nodes` in `model.rs`). Random Anim Start bases start the
  free tracks at the same phase as the clip [hypothesis]. An item lying in the world
  whose only animation is such controllers (a gun's `##` parts, its slide) stays as
  stored (`Clip::only_free_motion`, effects-animation-audit.md §3 "Items"). Free
  controllers on `##` nodes are not played at all: they are the parts the holder's clips
  show and hide (animation.md §9.7), and a placed frag mine's one on `##FMine`
  (`minefrag.nif`, Active, clamped, shown from 0 s [observed]) turned the mine into an
  animated rig that lost the light parts `nvf_play::mines` shows from its armed state. A reference
  a script enables (`nvf_play::script_draw`) does not apply the item rule yet, for free
  transform or visibility tracks. Before this, only
  `object_anims` (script groups, needing the script runtime) and `shot_fx` (effects)
  applied the sequence tracks, and nothing applied the free ones: the arcs' spots
  were always on in world-only runs and the dust tube never hid. Those two appliers are
  now this one. The original's timing of the blink was not compared with game frames
  [hypothesis: the keys play at the sequence's rate, as the transform keys do].
- **Unlit vertex colours as display values.** An unlit shape (BSShaderNoLightingProperty)
  draws texture × vertex colour; its vertex colours are taken as display values and
  converted to linear before the renderer's linear product (`nif_spawn::unlit_vertex_colors`)
  [hypothesis, as for the landscape and grass vertex colours]. The ground smoke's vertex
  colour (0.57, 0.54, 0.51) on a white texture (`DLCPittSideClouds.dds`, mean alpha 31 of
  255) is then a grey close to the desert floor's, as in the owner's game screenshot
  where the smoke can barely be seen (playtest list 4 item 16); read as linear it drew
  as a display 0.78, a white plume. Lit shapes still take their vertex colours as linear
  factors (see "Multiply blend" for what that leaves).
- **Vertex colours** [observed, `crates/fnv-nif/examples/vertex_alpha_survey.rs` over
  `Fallout - Meshes.bsa`, visible shapes with vertex colours and a shader property]:
  with BSShaderFlags bit 3 "Vertex_Alpha" [documented: nif.xml] set, 2,445 shapes have
  some vertex alpha below 1, 393 all 1 and 12 non-finite or out-of-range values; without
  it, 587 have some alpha below 1, 29,882 all 1 and 331 non-finite or out-of-range values.
  Example: Victor's shack (`nv_shackvictor.nif`, shape `NV_ShackVictor:2`, no
  vertex-alpha flag, alpha-tested) has sane colours but alphas of NaN on 104 of its
  1,173 vertices, about −3e38 on 476, denormals on 129 and 1.0 on 464; drawn with them its trim boards came out flat
  pale blue (owner's fly-cam screenshot). The renderers use vertex alpha only with the
  flag, clamp colours to 0..1 and replace non-finite values by 1 [hypothesis].
- **Normal maps** use DirectX-style green: +G points *down* the image (towards
  increasing v), +R to the right [observed]. Seven texture sets in `Fallout -
  Textures.bsa` ship a parallax height map (`_p`) beside the normal map (`_n`): McCarran
  flagstone, rubble piles, REPCONN HQ, concrete impact decal, Citadel rubble, station
  chips. Correlating the normal map with the height-map gradient (image y downward, 256²
  mip) gives corr(R, dH/dx) from −0.33 to −0.88 (as expected, brighter = higher) and
  corr(G, dH/dy) from −0.24 to −0.93, the sign of a downward green axis. Bevy assumes
  upward green (glTF), so the renderers set `flip_normal_map_y`. Unflipped, bumps were
  lit from below: rock faces looked inside-out and glistened at the wrong edges.
- **Specular** [observed, 500 random meshes from `Fallout - Meshes.bsa`]: of 738
  NiMaterialProperty blocks on BSShaderPPLightingProperty shapes, 697 (94%) have a
  black specular colour, 18 white, 23 other; 1,011 of 1,279 such shader properties set
  BSShaderFlags bit 0 "Specular" [documented: nif.xml]. Black specular colours include
  weapons (20 of 30 weapon materials) and the canyon rocks; the recurring value
  (0.682, 0.729, 0.796) with glossiness 1 looks like an exporter default. Normal-map
  alpha holds an authored mask [documented: nif.xml calls slot 1 "normal (+ gloss in
  alpha)"]: `NV_RockFormations_01_N.dds` (DXT5) averages 22/255 (median 18, 90th
  percentile 49). Normal maps are 2,873 DXT5, 425 DXT1 (alpha reads 1; see below),
  125 DXT3 and 5 R8G8B8. Glossiness spans 0–320 and clusters at 10 and 60–70
  (rocks: 67). The renderers ignore the specular colour, scale specular by the
  normal-map alpha when the Specular flag is set (none when it is clear), and read
  glossiness as a Blinn-Phong exponent for the highlight size [hypothesis: the
  per-texture masks are authored while the colour is almost always black, so the colour
  is presumably unused]. Reading glossiness 67 as a plain roughness of 0.33 and
  ignoring the mask made the canyon rocks look wet (owner's screenshot, Goodsprings
  rain); with the mask they are matte. To confirm against in-game captures.
  **Normal maps without alpha** (2026-10-11): Doc Mitchell's bed
  (`furniture\nv_bedtwin01.nif`, `NV_Bed01_N.dds`), the Goodsprings gas station's
  Sunset Sarsaparilla machine (`clutter\ssvendingmachine\ssvendingmachine.nif`,
  `ssVendingMachine_n.dds`) and its metal door (`dungeons\office\doors\
  offdoormetalstatic.nif`, `offdoorMetal_n.dds`) all have the Specular flag, a black
  specular colour, glossiness 10, no environment map, and a DXT1 normal map [observed:
  `fnvtool nif decode`, `fnvtool bsa extract`]. Read as a mask, DXT1's alpha of 1 gave
  them reflectance 1.0 (F0 0.16, four times a usual dielectric), and with Bevy's
  Fresnel they turned mirror-like at grazing angles: "very shiny, almost reflective"
  (owner's playtest). Over the 2,998 normal maps with an alpha channel the per-file mean
  alpha has quartiles 21 / 35 / 58 and 10th / 90th percentiles 11 / 91 (of 255)
  [observed: `crates/fnv-nif/examples/normal_mask_survey.rs` over `Fallout - Textures.bsa`
  and `Textures2.bsa`], so a full mask is far beyond what any authored mask asks for.
  The renderers give a shape whose normal map is DXT1 the median authored mask, 35/255,
  as a constant (`nif_spawn::UNMASKED_SPECULAR`) [hypothesis: the game's highlight on
  these objects is that of an ordinary surface; how its shader reads a DXT1 normal map's
  alpha is not known, and an in-game picture of the vending machine in sunlight from the
  side would settle it]. The 5 R8G8B8 normal maps decode with alpha 255 and are not
  covered.
  Ambient light is diffuse only in the renderers: Bevy adds a specular part for its
  ambient light whenever F0 ≥ 0.02, which the NIF material shader subtracts again
  [hypothesis: the game's ambient is a flat colour term, highlights come from lights and
  environment maps]. Without that, the Prospector Saloon's glass
  (`NV_StoreGlass.dds`, diffuse ≈ (34, 37, 34), specular mask in the normal-map alpha
  with bright cracked regions, mean 80/255) showed the mask as a frosted grey-white
  sheen, brighter at night with the brighter night ambient; in game the panes are dark
  [observed, owner's screenshot]. The windows' environment map (slot 4
  `shinybright_e.dds`, mask slot 5 `_m`, window-environment flag) is not drawn yet.
- **Hair** [observed, `characters\hair\hairbun.nif`]: both shapes have a
  NiMaterialProperty with specular colour (0, 0, 0) and glossiness 10, and a
  BSShaderPPLightingProperty with shader flags 0x82040101 — Specular (bit 0), Alpha
  texture (8), Hair (18), Remappable textures (25), Z-buffer test (31) [documented:
  nif.xml BSShaderFlags] — and the textures `HairBun.dds`, `HairBun_n.dds` and, in slot
  2, `HairBun_hl.dds`. The normal map's alpha is high: mean 95 of 255, 90th percentile
  184, against a mean of 16 for the female head's `headhuman_n.dds` and 21 for
  `hairbase_n.dds`; the `_hl` map's alpha is nearly the same (mean 93, 90th percentile
  181) over a grey strand pattern. Read as the plain specular mask of "Specular" above,
  that alpha made the hair glint under the Prospector Saloon's window light — white
  strands beside the face and a sheen on the crown — where the owner's in-game picture
  of Sunny Smiles shows matte auburn hair. The renderers give shapes with the Hair flag
  (hair, brows, beards) no specular [hypothesis: the hair shader does not use the
  normal-map alpha as an ordinary specular mask; what it does with it and with the
  `_hl` map is open — an in-game picture of hair under a strong light, from the side,
  would show whether there is any highlight to rebuild].
- **Hair and depth** [observed, `characters\hair\hairbase.nif`]: the hair shape's
  NiAlphaProperty is flags 4333 (0x10ED), threshold 0: blend on, SRC_ALPHA /
  INV_SRC_ALPHA, alpha test off [documented: nif.xml AlphaFlags], so the renderers draw
  it blended. A blended shape writes no depth in the renderers, so fire and dust drawn
  after the hair covered it wherever the room lay behind, and the player's head showed
  bald in third person while burning or in a dust devil (owner's playtest, 2026-10-24).
  Hair-shader shapes that are blended with no test now write depth where their alpha
  is 0.25 or more and drop fainter fragments [engineering choice; whether the game's
  hair writes depth is not known: the file has no NiZBufferProperty, and the Z-buffer
  test flag (bit 31) above says nothing of writing]. Smoke check `hair-under-fire`.
- **Hair models: a solid core and a blended layer** (2026-11-06, #1576; before that
  cut-outs, #1454): the HAIR record's model is drawn twice. A cut-out at alpha 0.8 is its
  solid core: it draws in the opaque passes, writes depth and casts the shadow. The same
  shape is then drawn blended (SRC_ALPHA / INV_SRC_ALPHA, as its NiAlphaProperty asks),
  writing no depth, so the fainter texels (shaved scalp, the soft ends and edges of the
  strands) blend over the core and over what lies behind; over the core it draws the
  core's own colour again. Beards and brows (head parts with the hair shader) stay
  blended as in "Hair and depth" above. What was measured:
  - `hairwavy.nif` (Chet) maps its outer layer onto the soft edge of the texture's
    alpha: sampled per triangle, the outer tenth averages 0.43 where the inner averages
    0.92, and the texture's alpha is otherwise near 0 or 1 at every mip level
    [observed: `characters\hair\hairwavy.dds`, 71% of texels 255, 15% 0]. Blended with
    the depth write of "Hair and depth", that layer drew as a see-through shell around
    his head against the store's window; in the original his hair is a solid cap with a
    clean hairline, and his `beardthin.nif` beard is thin [observed: owner's frame of
    the original, Chet in dialogue in front of the GSGeneralStore window, 2026-11-03].
    As a cut-out at 0.5 the beard became a full one, hence beards stay blended.
  - As a cut-out at 0.25 everywhere (#1458), the Mohawk's shaved sides drew as solid
    hair [observed: owner, #1576]. `hairraiderspike.nif` (Warhawk; Jessup) maps the
    sides of the head onto one region of `hairraider01.dds` whose alpha is a flat
    plateau, 166–190 of 255 (5th to 95th percentile, median 185, about 0.72), and its
    colour is that of the strands [observed: the texture; the UV layout of the NoHat
    shape]. 9.7% of the texture's texels lie in 179–191; 28% are 255. The eleven raider
    hair models (the Mohawks and the female raider styles, among them `HairBaseF`
    "Shaved") all use that texture [observed: MODL and texture sets of the 67 HAIR
    records of FalloutNV.esm]. Blended, the plateau lets about a quarter of the scalp
    show through a dark layer; cut out, it is solid.
  - `hairbase.dds` (`hairbase.nif`: Buzz Saw, Balding, Combover, Buzz Cut) is a plateau
    at 217–242 (0.85–0.95) with 7.5% of texels at 0 and none above 242 [observed].
  - All hair models' NiAlphaProperty blends with no threshold: flags 4333 (0x10ED: blend,
    no test) or 4845 (0x12ED: blend and test GREATER 0, which drops only alpha 0)
    [observed over the NoHat shapes of the 67 HAIR records; documented: nif.xml AlphaFlags]. The flags do not
    tell shaved parts from solid ones; only the texture alpha does.
  The cut of 0.8 is an engineering choice from those plateaus: below it the raider
  scalp (≤ 0.75) stays see-through, above it the short hair of `hairbase.dds` (≥ 0.85)
  keeps a solid core (a cut above 0.85 would leave those hairs no depth and no shadow).
  Chet's hair stays solid because the core (his texels at 255) is drawn first and his
  soft outer band blends over it instead of hiding it. Whether the game draws its hair
  this way, alpha-tests it, or does something else that comes out the same is not known
  [hypothesis]. Smoke checks `hair-solid-chet`, `hair-shaved-mohawk`.

## 7. Coverage status

Measured with `fnvtool nif coverage` (exact consumption of every block's stored size and
byte-identical re-encoding). See `docs/design/layer2.md` for current numbers.
