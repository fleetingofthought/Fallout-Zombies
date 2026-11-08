# NIF/KF engine gap audit (2026-10-23)

Every block type, every behaviour-changing field value and every controller in the base game's
`.nif` and `.kf` files, checked against what NVForever's engine does with it. The owner kept
finding these one effect at a time (white dust, missing fades, motion not played, lights not
blinking); this audit lists them in one pass and ends with a ranked work queue (§10) split into
workstreams that do not touch the same files. Tags as in CLAUDE.md. No engine code was changed.

Builds on, and does not repeat: [nif-node-audit](nif-node-audit.md) (names, extra data, text keys,
2026-10-20), [effects-animation-audit](effects-animation-audit.md) (controllers that animate
materials and nodes, 2026-10-19), [particles](particles.md) §5-§7, [nif](nif.md), `docs/status.md`.
Where a row here corrects one of those, §11 says so.

**In progress elsewhere, not analysed here:** Idle visibility tracks on plain placed rigs, and
`NiLightColorController` / `NiLightDimmerController` for placed lights (two sessions started
2026-10-23). PR #621 (branch `claude/dust-whirlwind`, not merged at the time of writing) adds
`NiPathInterpolator`, emitters moved by their sequence, emitters keyed by the right sequence,
particle material alpha keys and Random Anim Start phase; rows that it fixes say "#621".

## 0. Sources and method

- **Files** [observed]: `crates/fnv-nif/examples/engine_gap_survey.rs` (new) over
  `Fallout - Meshes.bsa`, `Update.bsa` and `Fallout - Misc.bsa`: 19,277 files (4,341 `.kf`:
  2,038 `characters\_male`, 435 `characters\_1stperson`, 1,369 `creatures`, the rest in the Fallout
  3 `dlc*` folders), 160 block types, every block decoded, none failed. For each file it lists
  each block type, each integer/bool field value (counts and sizes left out), each bit set in a
  field named `*flags*`, each controller as type × run mode × target × interpolator × data, each
  sequence controlled block as controller type × interpolator × property type (× controller ID for
  material, texture, light, morph and particle controllers), each extra data name/value and each
  text key. No add-on archive was read (the lab holdout is in one).
- **Placed references** [observed]: `fnvtool world refs` over every worldspace of FalloutNV.esm
  that has references and `--interiors`, deleted refs dropped: 296,122 refs with a model, 277,979
  of them to a model in these archives (7,514 files). "Refs" below = placed references whose
  base's model is that file. Actors count through their skeleton
  (`characters\_male\skeleton.nif` = 1,823 NPC refs); weapons, armour and clips are not placed and
  show 0, so their impact is given in files or clips.
- **Floats** (turbulence, phase, ranges, transforms) for the particle, phantom and action files
  came from `fnvtool nif decode` [observed].
- **Engine**: every type name was searched in `engine/` and `crates/` (`git grep -w`), then the
  code paths that read the type, or reach it through a reference field, were read. Status words:
  **read+used**, **read, unused**, **not read**, **partly**, **wrong**. Code at `b30b8eff` (main,
  2026-10-23).
- Dumps (`survey.jsonl`, the refs, the per-area extracts) are game-derived and stay private
  (session scratchpad); none is committed. Rerun:
  `cargo run --release -p fnv-nif --example engine_gap_survey -- schemas/nif <the three BSAs> > survey.jsonl`.

Path shorthands: `nif_spawn.rs`, `nif_material.rs`, `model.rs`, `anim_play.rs`,
`material_anim.rs`, `morph.rs`, `model_light.rs`, `texture.rs` = `engine/nvf-assets/src/`;
`anim.rs` = `crates/fnv-nif/src/anim.rs`; `player_clips.rs`, `idle_play.rs`,
`anim_sounds.rs` = `crates/fnv-actor/src/`; `shot_fx.rs`, `world.rs`, `anim_blend.rs`,
`player_body.rs`, `object_anims.rs`, `attack_anims.rs` = `engine/nvforever/src/`. In §4 and §5:
`A` = `engine/nvf-assets/src/particles.rs` (reader), `E` = `engine/nvforever/src/particles.rs`
(simulation, drawing), `C` = `crates/fnv-collide/src/nif.rs`, `R` =
`crates/fnv-collide/src/ragdoll.rs`, `P` = `crates/fnv-phys/src/`. Flag and enum names are from
nif.xml as the schemas carry them (`schemas/nif/*.json`) [documented]. NiTimeController flags:
bit 0 app-init, bits 1-2 cycle (0 loop, 1 reverse, 2 clamp), bit 3 active, bit 4 play backwards,
bit 5 manager controlled; so 72 = active loop, 76 = active clamp, 104/108 = sequence loop/clamp,
69/65 = not active [documented: nif.xml TimeControllerFlags]. Key types: 1 linear, 2 quadratic,
3 TBC, 4 XYZ rotation, 5 constant [documented: nif.xml KeyType].


## 1. Geometry and properties

### 1.1 NiTexturingProperty / NiSourceTexture / NiMaterialProperty / NiStencilProperty

| What | Files / refs | Example | Engine status | Should do | Fix |
|---|---|---|---|---|---|
| NiTexturingProperty base texture (`source.file_name`) | 1,218 / 11,836 | fxglowsimpfillrndflthalf (866) | read+used `nif_spawn.rs:345-350` | — | — |
| `flags` 4 / 5 = apply mode 2 MODULATE (bits 1-3), 5 adds multitexture bit 0 | 1,218 / 11,836; 1 / 0 | — | not read; StandardMaterial modulates anyway | MODULATE = what is drawn [documented TexturingFlags] | none |
| base_texture.flags 12800 / 512 / 8704 (TexturingMapFlags: filter bits 8-11 = 2 TRILERP; clamp bits 12-15 = 3 / 0 / 2) | 1,213 / 11,789; 18 / 15; 2 / 47 | ecvcontrolroomzaxmonitor01 (14, clamp 0); computerrack01 (47, clamp 2) | not read (Repeat, trilinear default) | Clamp as the §2.1 `texture_clamp_mode` row | `apply_property` |
| `has_texture_transform`=1, `transform_method`=1 | 467 / 1,724 | fxsmokewisps01 (180) | **not read**: stored translation/scale/rotation/centre ignored; `Look::stored` is identity (`material_anim.rs:82-84`), only NiTextureTransformController tracks move UVs | Apply the stored transform as the base UV transform [documented field; whether values differ from identity: not surveyed] | `apply_property` → `Look::stored` / `uv_transform` |
| dark_texture | 1 / 0 | dlc05alienhologramhead | not read | DLC05 only | — |
| NiSourceTexture `use_external`=1, `pixel_layout`=6, `use_mipmaps`=1, `alpha_format` 1/3 | 1,218 / 11,836 | utldoor01 (195, alpha 1) | not read; DDS header decides | No gap (external DDS) [hypothesis] | — |
| NiMaterialProperty `alpha`, `emissive_color`, `emissive_mult`, `glossiness` | 12,046 / 276,838 | nvn_rockcanyon12 | read+used `nif_spawn.rs:352-357`, :694, :697 | — | — |
| NiMaterialProperty `specular_color` | same | — | not read (F0 fixed / mask only, `nif_spawn.rs:548-565`) | Tint/strength of the highlight [documented field; game use hypothesis] | `specular()` |
| NiMaterialProperty `diffuse_color`, `ambient_color` | same | — | not read | Unknown whether BS shaders use them [hypothesis] | check against frames first |
| NiStencilProperty flags 19840 = enable 0, pass action 3, draw mode 3 DRAW_BOTH (bits 10-11), test func 4 | 577 / 21,685 | yuccacnv (1,662) | read+used `nif_spawn.rs:360-364` → `double_sided`, no culling (:716-717); stencil disabled so nothing else needed | — | — |

### 1.2 Geometry data

| What | Files / refs | Example | Engine status | Should do | Fix |
|---|---|---|---|---|---|
| vertices, normals, triangles/strips, UV set 0, vertex colours (`has_vertex_colors`=1: strips 8,939 / 215,484; shapes 234 / 93) | 11,190 / 274,663 strips; 3,508 / 2,201 shapes | nvn_rockcanyon12; flatmop01 (55) | read+used `nif_spawn.rs:756-783` (`build_mesh`), colours cleaned `fix_vertex_colors` | — | — |
| `bs_data_flags` bit 0 has UV, bits 6-11 Havok material, bit 12 has tangents (e.g. 4097, 4737, 5121) | 11,151 / 274,617 with bit 12 | nvn_rockcanyon12 | UV read; stored tangents/bitangents decoded (`fnv-nif/src/scene.rs:185,256`) but **unused**: tangents regenerated (`nif_spawn.rs:691` `generate_tangents`); Havok material bits not used by rendering | Using stored tangents would match the game's normal-map shading exactly [hypothesis: small difference] | `build_mesh` insert `ATTRIBUTE_TANGENT` |
| `has_normals`=0 | strips 3 / 3; shapes 2,308 / 0 (LOD) | terminaldesktrap01 (3) | normals computed (`nif_spawn.rs:781`) | — | — |
| `consistency_flags` (CT_MUTABLE / CT_STATIC 16384), `keep_flags`, `compress_flags`, `group_id` | all | — | not read | Runtime update hints; none for drawing [documented ConsistencyType] | — |
| NiAdditionalGeometryData (block type 1, stride 4, flags 2) | 2,308 / 0 | bouldercityworld.level4.* (land LOD) | not read | Meaning of the extra per-vertex stream unknown | — (LOD only) |

### 1.3 Lights as blocks

| What | Files / refs | Example | Engine status | Should do | Fix |
|---|---|---|---|---|---|
| NiPointLight `diffuse_color` × `dimmer` | 38 / 92 | lamplightwarm (57) | read+used `model_light.rs:11-18`, only when a LIGH's own colour is black (`lights.rs:172`) | — | — |
| NiPointLight `ambient_color`, `specular_color`, constant/linear/quadratic attenuation, position (`AttachLight` node), `switch_state` (0 on dlcpitt_cauldron01, 0 refs) | 38 / 92 | lamplightwarm | not read (LIGH radius/falloff used instead) | Attenuation/position may matter for lights placed by models alone [hypothesis] | `model_light.rs` |
| NiAmbientLight (switch_state 1) | 8 / 126 | lamplightwarm (57) | not read | Ambient term of the model's light [documented type; effect hypothesis] | `model_light.rs` |
| NiLightColorController / NiLightDimmerController | — | monumentlight01, klaxon | **in progress elsewhere** | — | — |


## 2. Shaders and flags

### 2.1 Shader flags (BSShaderPPLightingProperty = PP, BSShaderNoLightingProperty = NL)

| What | Files / refs | Example | Engine status (file:line) | What it should do [tag] | Fix location |
|---|---|---|---|---|---|
| SF bit 0 `specular` | PP 10,176 / 235,516; NL 171 / 1,155 | nvn_rockcanyon12 (10,747); terminaldeskhulk01 (146) | read+used: `nif_spawn.rs:323`, `specular()` :548 (mask from normal-map alpha). NL: set but unlit, no effect | Specular highlight from normal-map alpha [documented nif.xml name; nif.md §6] | — |
| SF bit 1 `skinned` | PP 773 / 345; NL 11 / 0 | teddybear01 (74) | not read; skinning comes from `skin_instance` (`nif_spawn.rs:846`) | Same information as the skin instance [hypothesis] | none needed |
| SF bit 3 `vertex_alpha` | PP 38 / 180; NL 1,720 / 30,385 | mopbucket01 (101); markerxheading (2,790) | read+used `nif_spawn.rs:294,324` | — | — |
| SF bit 6 `empty` (falloff) | NL 426 / 7,752; PP 175 / 4,901 | fxglowsimpfillrndflthalf (866); tincan01 (1,292) | NL read+used `nif_spawn.rs:283,343`; PP has no falloff fields, not read | NL: view-angle falloff [hypothesis, nif.md §6]; PP meaning unknown | — |
| SF bit 7 `environment_mapping` | **PP 1,663 / 19,243**; NL 38 / 68 | ssbottle02 (921), pplight02 (21) | **not read**. Texture-set slots 4 (cube) / 5 (mask) and `environment_map_scale` (BSShaderProperty field) unread; `shape_material_with` uses slots 0,1,2 only (`nif_spawn.rs:687-688`) | Cube-map reflection × mask × env scale on bottles, metal, glass [documented: nif.md:231 slot list; nif.md:1003 "not drawn yet"; project-audit §World lists it] | new env-map path in `nif_material.rs` + read slots 4/5 in `apply_property` |
| SF bit 8 `alpha_texture` | PP 5,887 / 101,999; NL 783 / 10,426 | creosotebushsm (2,892) | read only by `sky.rs:1109`; NIF shapes: not read (alpha from NiAlphaProperty) | Unknown whether the game needs it beyond NiAlphaProperty [hypothesis: no] | — |
| SF bit 10 `face_gen` + shader_type 14 `SHADER_SKIN` | 224 / 22 (both) | outfitm (lingerie, 6) | read+used `nif_spawn.rs:228` (slot 2 not glow), :327 skin | — | — |
| SF bit 11 `parallax_shader_index_15`, bit 28 `parallax_occlusion` | 255 / 4,427; 7 / 3 | rubblepile02 (367); rwstairsstone01_nv (2) | **not read**; slot 3 (height) and `parallax_max_passes`/`parallax_scale` unread | Parallax offset of the base map by the `_p` height map [documented: flag name; nif.md:952] | `nif_material.rs` (Bevy `depth_map` + `parallax_depth_scale`) |
| SF bit 12 `unknown_3`, bit 13 `non_projective_shadows`; SF2 bit 1 `lod_landscape` | 2,308-2,669 / 0 | bouldercityworld.level4.* | not read (land/object LOD, own path `lod.rs`) | LOD only | — |
| SF bit 15 `refraction`, 16 `fire_refraction`, `refraction_fire_period` | PP 75 / 125, 66 / 125; NL 1/0, 3/2; periods -22…200 | trapgasfire01 (124, period 10); fortfirefx (1) | read+used `nif_spawn.rs:252,263,329-330` (tenths of s [hypothesis, refraction.md §3]) | — | — |
| SF bit 17 `eye_environment_mapping` | 21 / 13 | glassesreading (9) | not read | Env map on eyes/lenses [documented name] | with bit 7 |
| SF bit 18 `hair` | 92 / 19 | nv_headpikemale01 (8) | read+used `nif_spawn.rs:326`, `specular()` | — | — |
| SF bit 19 `dynamic_alpha` | NL 234 / 770; PP 36 / 1,112 | klaxon (174); maize_healthy01 (233) | NL read+used `nif_spawn.rs:295,325,436`; PP not read | PP meaning unknown | — |
| SF bit 20 `localmap_hide_secret` | 1 / 3 | blackrefblock256 (3) | not read | Hidden from the Pip-Boy local map [documented name] | local-map render (if it draws placed geometry) |
| SF bit 21 `window_environment_mapping` | PP 490 / 2,639; NL 5 / 34 | beerbottle01 (314); pplight02 (21) | **not read** (nif.md:1003) | Window/glass env map [documented name] | with bit 7 |
| SF bit 25 `remappable_textures` | PP 10,879 / 195,027; NL 2,486 / 37,244 | creosotebushsm | not read | Meaning for rendering unknown [hypothesis: texture-swap permission] | — |
| SF bits 26/27 decals | PP 1,099 / 25,127, 1,059 / 23,891; NL 685 / 8,369 | nvn_cliffcanyonl01 (1,471); metalbox01 (743) | read+used `nif_spawn.rs:241,329`, `nif_material.rs:428` depth bias | — | — |
| SF bit 29 `external_emittance` | PP 213 / 6,268; NL 316 / 8,892 | nvfungusstalk03 (830); fxglowsimpfillrndflthalf (866) | read+used `nif_spawn.rs:328`, `model.rs:64` | — | — |
| SF bit 31 `zbuffer_test` | PP 11,299 / 258,037; NL 2,815 / 40,216 | everywhere | not read; engine always tests depth. 4 NL files without it (0 refs listed) | Depth test off on those 4 [documented name] | low |
| SF2 bit 0 `zbuffer_write` | off on PP ~160 / ~2,800 (values 0, 8, 32768; LOD files aside) and NL ~650 / ~4,500 (2,819 − 2,167 files with the bit) | whitehorsenettledead (585); fxglowsimpfillrndflthalf (866) | **not read**: depth writes follow Bevy's alpha mode (opaque/mask write, blend never: `nif_material.rs:374` only for refraction) | Opaque/tested shapes without the bit should not write depth; blended shapes with it should [documented name; visible as sorting of plants/glows, hypothesis] | `apply_property` → material key → `specialize` depth_write |
| SF2 bit 2 `lod_building` | 361 / 0 | LOD blocks | read+used `nif_spawn.rs:332,433` | — | — |
| SF2 bit 3 `no_fade` | PP 63 / 1,944; NL 9 / 287 | nv_water-stain (370); officeshelfwoodsolid128a (59) | not read | Never faded out by distance [documented name; hypothesis for effect] | world fade ranges (`world.rs` visibility ranges) |
| SF2 bits 13, 14, 15 (`billboard_and_envmap_light_fade`, `no_lod_land_blend`, `envmap_light_fade`) | 15 / 437; 5 / 61; PP 337 / 3,159, NL 1 / 1 | dinercounter01 (107); sodabottle01 (704) | not read | Env-map fades; useful only once env maps exist | with bit 7 |
| SF2 bit 22 `no_transparency_multisampling` | 4 / 21 | wastelandhedgerow02 (13) | not read; every masked shape gets alpha-to-coverage (`nif_material.rs:169-180`) | No alpha-to-coverage on these [documented name] | `soft_edges` key |
| `texture_clamp_mode` (TexClampMode 0 CLAMP_S_CLAMP_T, 1 CLAMP_S_WRAP_T, 2 WRAP_S_CLAMP_T, 3 WRAP_S_WRAP_T) | PP 0: 2,795 / 2,325, 1: 5 / 0, 3: 11,263 / 256,770; NL 0: 18 / 15, 1: 3 / 0, 2: 2 / 47, 3: 2,811 / 40,169 | nvseaplant01 (682, mode 0); computerrack01 (47, mode 2) | **not read**: every NIF texture samples Repeat (`texture.rs:122-125`); only grass uses `get_clamped` (`grass.rs:515`), both axes | Clamp the named axes: no wrap bleed at card edges [documented enum; visible effect hypothesis] | `apply_property` (read field) + per-axis `TextureCache::get_clamped` |
| `environment_map_scale` | all BSShaderProperty | — | not read | Scales env reflection | with bit 7 |
| NL `file_name`; falloff fields | NL 2,819 / 40,216 | markerxheading | read+used `nif_spawn.rs:339-343` | — | — |
| `shader_type` 1 / 14 / 33 | PP 13,954 / 258,037 (1), 224 / 22 (14); NL 33 all | — | 14 read (`nif_spawn.rs:327`); others need nothing | — | — |
| SkyShader / TallGrass / Water / TileShaderProperty | 3/0, 17/0, 8/102, 5/0 | atmosphere, grasslawn01, waterp2048x2048 (54), loading01 | sky.rs, grass.rs, world.rs (water); Tile unused (HUD from XML) | — | — |

### 2.2 NiAlphaProperty (7,788 / 152,360)

Decode: bit 0 blend, src = bits 1-4, dst = bits 5-8, bit 9 test, func = bits 10-12, bit 13 no-sort [documented: core.json AlphaFlags].
Engine: `alpha_mode` `nif_spawn.rs:400-412`, `adds_whole` :418, `blend_cut` :450. Test func 4 (GREATER) on every value but 237 (func 0, test off) [observed]; GREATER handled (+0.5 step), other funcs as ≥. **No-sort (bit 13) never set** (max value 4845) [observed].

| Flags | src / dst, test | Files / refs | Example | Engine result | Correct? |
|---|---|---|---|---|---|
| 237, 4333 | SRC_ALPHA / INV_SRC_ALPHA | 10 / 299; 2,151 / 35,761 | utldoor01 (195); markerxheading | Blend | yes |
| 4097 | ONE / ONE | 179 / 242 | neoclassicalsconce (130) | Add + `add_whole` | yes |
| 4109 | SRC_ALPHA / ONE | 494 / 7,871 | fxglowsimpfillrndflthalf (866) | Add | yes |
| 4163 | ZERO / SRC_COLOR | 574 / 5,005 | utlrmcorin01 (196) | Multiply | yes (dst × src) |
| 4608, 4620, 4640, 4674, 4844 | blend off, test on | 9/152, 52/5, 22/24, 62/1,012, 5,322/92,831 | vdoorslidingrr01 (74), dlcpittpipe1waylng01 (195) | Mask(threshold) | yes |
| 4845 | SRC_ALPHA / INV, test | 727 / 27,148 | creosotebushsm (2,892) | Mask if threshold ≥ 64 else Blend (+`blend_cut`) | [hypothesis, nif.md §6] |
| 4609 | ONE / ONE, test | 3 / 3 | archivesdisplaycases (1) | Add; test ignored | partly (test dropped) |
| 4621 | SRC_ALPHA / ONE, test | 129 / 816 | fxlightdustparticleswide02 (161) | Add; test ignored | partly: texels under the threshold still add [hypothesis: small visible effect] |
| 4641 | ONE / ZERO, blend, test | 7 / 0 (impact effects) | impactballisticblood01 | threshold ≥ 64 → Mask; else **Blend** | wrong below 64: ONE/ZERO is opaque replace, should be Mask/Opaque |
| 4129 | ONE / ZERO, blend | 2 / 0 | testmultiboundno (test file) | Blend | wrong (should be opaque); test file only |
| 4131 | ZERO / ZERO | 1 / 0 | dlc05spacewindowcover01 | Multiply | wrong (draws nothing/black); DLC05 only |
| 4133 | SRC_COLOR / ZERO | 1 / 0 | dlc05spacedomecore01 | Blend | wrong (src²); DLC05 only |
| 4173 | SRC_ALPHA / SRC_COLOR | 1 / 0 | dlc05mzdoorsmfieldeffect01 | Blend | wrong (rare factor pair) |
| 4323 | ZERO / INV_SRC_ALPHA | 2 / 0 | dlcanchgaussscope01 (HUD scope) | Multiply (by colour) | wrong: should darken by alpha (dst × (1−a)) |

Fix location for the wrong rows: `alpha_mode` in `nif_spawn.rs` (or a custom blend state in `nif_material.rs` specialize). Only 4641 (impact blood) can show in base-game play.


## 3. Animation and controllers (placed models, effects, actors, player)

### 3.1 Key types per data block

| What | Files / placed refs | Example | Engine status (file:line) | What it should do | Fix location |
|---|---|---|---|---|---|
| NiTransformData rotation_type 1 linear (quat) | 3811 / 3686 kf / 2659 | `_male\skeleton.nif`(1823), `utldoor01.nif`(195) | read+used: slerp, shorter arc, `anim.rs:309-327` | slerp [documented: Shoemake; nif.md §5 observed] | — |
| rotation_type 3 TBC (quat) | 137 / 118 kf / 83 | `creatures\nvbighorner\skeleton.nif`(35), `vault\halls\vdoor01.nif`(24) | **partly**: TBC parameters read but quaternions slerped key to key, `anim.rs:305-327` (the code tags it a hypothesis) | a quaternion Kochanek–Bartels spline (ease in/out between keys) [hypothesis] | `anim.rs` `sample_quat_keys` (squad with TBC tangents) |
| rotation_type 4 XYZ euler | 3213 / 2230 kf / 10830 | `nvseaplant01.nif`(682), `footlocker01.nif`(553) | read+used: three float groups each sampled by its own kind, `anim.rs:579-591`; `euler_xyz_to_quat` X then Y then Z about fixed axes, `anim.rs:331-342` | order is [hypothesis] in code; no data check found. If one axis group is empty it gives 0, and all-empty gives identity, not the static pose [observed code] | `anim.rs:265-268` (fall back to `rotation` when all three are empty) |
| rotation_type 5 constant (quat) | 96 / 96 kf / 0 | `_1stperson\2hhequip.kf`, `2hhunequip.kf` | read+used: step, `anim.rs:321` | step [documented: KeyType] | — |
| translations interp 1 / 2 / 3 / 5 | 4232 / 116 / 68 / 105 files; refs 3144 / 6 / 45 / 1 | `cabinet01.nif`(328); `nv_hooverdam_aagun.nif`; `vdoor01.nif`(24); `dlc05ufocrash.nif`(1) | read+used, `Keys::sample` `anim.rs:94-120`: lerp; Hermite with backward = leaving, forward = arriving (checked on 181,454 keys, nif.md §5 [observed]); TBC; step | as is. TBC tangents do not scale for uneven key spacing (`anim.rs:142-150`) [hypothesis: KB's spacing factor 2Δt_i/(Δt_{i-1}+Δt_i) may apply] | `anim.rs:142` if a capture shows it |
| scales interp 1 / 2 / 3 | 81 / 2123 / 40 files; refs 1 / 6640 / 48 | `helioslaser.nif`; `desk01.nif`(323); `vdoor01.nif` | read+used (same sampler, `anim.rs:574`) | — | — |
| xyz_rotations interp 1 / 2 / 3 | 131 / 3110 / 72 files; refs 501 / 10723 / 2 | `slot_machine01.nif`(169); `nvseaplant01.nif`; `fortfirefx.nif`(1) | read+used (`Keys<1>::from_group` per axis) | — | — |
| NiFloatData interp 1 / 2 / 3 | 282 / 1777 / 54 files; refs 2406 / 5242 / 2 | `neoclassicalsconcefire01.nif`(76); `maize_healthy01.nif`(233); `fortfirefx.nif`, `secanimfx.nif`, VATS cameras | read+used via `value_track` `anim.rs:398-410` | — | — |
| NiPosData interp 1 / 2 | 6 / 547 files; refs 0 / 6180 | roulette tables; `nvfungusstalk03.nif`(830) | read+used `anim.rs:411-416` | — | — |
| NiBoolData interp 5 | 728 / 239 kf / 3400 | `skeleton.nif`, `fxlightdustparticleswide02.nif`(161) | read+used: step whatever the kind, `anim.rs:451-483` | step [documented: constant] | — |
| NiColorData interp 1 | 10 / 0 / 18 | `fxhelios_steam.nif`(10), `fortfirefx.nif` | not read in the engine (no code names the type); particle colour modifier data | §4 | §4 |
| Missing interpolation field (`num_keys` 0) | — | — | defaults to 1 and is then dropped because there are no keys, `anim.rs:63`, `405` | — | — |

Nothing falls to a default silently: an unknown KeyType makes `from_raw` return None and the
group is dropped (`anim.rs:22-30`) [observed code].

### 3.2 Free-controller timing (NiTimeController fields)

| What | Files / placed refs | Example | Engine status | What it should do | Fix |
|---|---|---|---|---|---|
| Cycle bits 1-2: loop / reverse / clamp | flags 72: NiTransformController 119/286, NiTextureTransformController 231/1021; 76: 605/4133; 74 (reverse): 2/0 | `fxsmokewisps01.nif`(180); `dlc05hololivingquarters.nif` (74) | read+used: `own_cycle` `model.rs:473-481` → `cycle_time` `anim_play.rs:683-697` (reverse = ping-pong) | [documented: CycleType] | — |
| start / stop / frequency / phase | all free ctlrs | — | read+used: `OwnCycle::time` = cycle(clock·freq + phase) `anim_play.rs:108-110`; stop ≤ start is dropped `model.rs:422`, `533` | order of freq and phase is a [hypothesis] (every surveyed phase is 0) | — |
| Bit 0 app-init vs app-time | 73: NiTransformController 2/1092 (`nvseaplant01.nif` 682); 69/65/101 on off/seq ctlrs | `nvseaplant01.nif` | **not read**. Every free controller runs on its own instance clock, started at `at_phase` (`anim_play.rs:312`, `344-347`) | APP_TIME controllers may share the game clock, so copies of one model would move in step; APP_INIT ones start when shown [hypothesis] | `model.rs:473` (keep bit 0); `anim_play.rs:344` |
| Bit 3 active clear ("off") | NiTransformController off 52/289 (ghoul and brahmin skeletons), 14+7+2+1 effects/0; NiAlpha off 28/1; NiMaterialColor off 16/5; NiTextureTransform off 8/0; NiVis off 2/0 | `ghoul\skeleton.nif`(97), `mainframer01.nif`(1), `nv_thetops-sign.nif`(2) | ignored on placed objects (`model.rs:375`, `503`); played once in effect models (`effect_clip` `model.rs:586-611`, `inactive=true`) | rightly ignored [hypothesis: started by code]; skeletons' own controllers are overridden by clips | — |
| Bit 5 manager controlled | all seq ctlrs | — | excluded from free reading, played through sequences (`model.rs:375`, `503`; `GeomMorpher::is_free` `anim.rs:927`) | — | — |
| Flags 106 (seq + reverse) | NiMaterialColorController 4/19 | `nvwallcontrolpanel01.nif`(9) | the sequence's own cycle_type is used (`Clip::from_sequence` `anim_play.rs:170`) | [hypothesis] sequence wins | — |
| Bit 4 play backwards | none in the data | — | not read | — | — |
| Bit 6 compute scaled time | almost all | — | not read; frequency is always applied | [hypothesis] no gap | — |

### 3.3 Controllers by type × run mode × target

| What (ctl) | Files / kf / placed | Example | Engine status | What it should do | Fix |
|---|---|---|---|---|---|
| NiTransformController free on NiNode, NiTransformInterpolator+data | 582/0/4598 | `nvseaplant01.nif`(682), `skeleton.nif`(1823) | read+used for placed objects: `free_transform_tracks` `model.rs:529-537`, merged into Idle `model.rs:1115-1125`, played `anim_play.rs:345-357`. Items lying in the world are skipped by design (effects audit §3) | — | — |
| … free on BSFadeNode (+data) | 18/0/34 | `schooldeskadirty02.nif`(19) | same | — | — |
| … free, interpolator without data (static pose) | NiNode 140/2645, BSFadeNode 19/72 | `dlcpittcablestraightlong01.nif`(45), `vprojectorr01.nif`(19) | dropped by `moves()` `model.rs:540-556`; the node keeps its NIF transform | the same unless the interpolator's pose differs from the node's [hypothesis, unchecked] | — |
| … free, no interpolator | NiNode 45/2935, NiTriShape 1/1823 (skeleton) | `skeleton.nif` | ignored (`model.rs:506`) | nothing to play | — |
| … free on NiBillboardNode / BSValueNode | 24/3; 1/0 | `dlc03teslacannon.nif`, `fxfiremeshsmall.nif`; `muzzleflashpressure.nif` | played as a node track; billboard turning may override the rotation [hypothesis] | — | — |
| … free with NiPathInterpolator | 11/0/59 | `nvlakemeadfish04.nif`(24), `nvlakemeadfish01.nif`(13) | **not read on main** (`transform_track` has no case, `anim.rs:641`). PR #621 (claude/dust-whirlwind) adds `PathChannel`: place along the curve, constant velocity (bit 4); follow/bank not read | fish: flags 16 (constant velocity only), so no follow needed [documented: PathFlags schemas/nif/anim.json:37]; follow (bit 5) only in 0-ref files (flags 48, 114) | merge #621 |
| … seq on NiNode / BSFadeNode (via NiMultiTargetTransformController) | 161/0/3095; 6/0/0 | `ammobox01.nif`(401), `utldoor01.nif`(195) | played through sequences (Idle, doors, PlayGroup groups) `model.rs:1096-1133` | — | — |
| NiVisController free on NiNode / BSFadeNode / NiBillboardNode | 130/0/2177; 12/0/0; 13/0/0 | `skeleton.nif`(1823), `minefrag.nif`(138), `dlcanchactorshock01.nif` | **not played on placed objects**: `free_transform_tracks` drops `NodeTrack::Visibility` (`model.rs:533`); effects take them (`model.rs:590-593`). Weapon parts are shown and hidden by clips; mines by `mines.rs` | a free visibility track should switch its node on its own cycle [hypothesis]; which files besides weapons, mines and skeletons have it is unchecked (about 216 refs) | `model.rs:529` (keep Visibility in `Clip::free`) |
| NiVisController seq (BSFadeNode, BSValueNode, NiBillboardNode, NiNode) | 5/0/81, 3/0/0, 37/0/17, 73/0/71 | `forcefieldrightimpact01.nif`(66), `protectronpod01.nif`(15), `fxelectricarc01.nif`(25) | read into `Clip::visibility`; groups and actors use it (`object_anims.rs`, `anim_blend.rs:345-360`); **Idle on plain rigs: in progress elsewhere** | — | — |
| NiVisController off | 2/0/0 | `explosionsplash.nif` | effects only | — | — |
| NiAlphaController free / off / seq | 46/0/138; 28/0/1; 186/0/1631 | `neoclassicalsconcefire01.nif`(76); `maize_healthy01.nif`(233) | read+used: free `model.rs:400`; seq `MaterialTrack::of` `anim_play.rs:129` | — | — |
| NiMaterialColorController target_color 3 (SELF_ILLUM) free / seq | free 118/0/817 (all colours); seq 226 nif/5157 refs | `terminaldeskhulk01.nif`(146); `nvfungusstalk03.nif`(830) | read+used `model.rs:403`, `anim_play.rs:127` | — | — |
| NiMaterialColorController target_color 2 (SPEC) | seq 15 nif/472 refs; 18 kf; free about 6 files (effects audit §3) | `nvpinto_fruited.nif`(138), `nv_pricklypearcactus.nif`(129); `2hareloadj.kf`, `sitstooldrinktalka.kf` | **not read**: free needs `target_color == 3` (`model.rs:403`), seq needs id SELF_ILLUM (`anim_play.rs:127`) | the specular colour over time [documented: MaterialColor TC_SPECULAR]; drawing needs the material's specular colour in nif_material | `anim_play.rs:46` (add a Specular target), `material_anim.rs:87` |
| NiTextureTransformController ops 0-4 free / off / seq | 276/0/1142; 8/0/0; 184/0/582 | `fxsmokewisps01.nif`(180), `trapgasfire01.nif`(124); `computerrack01.nif`(47) | read+used: free ops 0-4 on slot 0, shader_map 0 (`model.rs:384-394`); seq by id `0-0-TT_*` (`anim_play.rs:131-138`); held identity rotate/scale skipped `model.rs:413-420` | other slots never appear (survey: only `0-0-`) [observed] | — |
| BSMaterialEmittanceMultController free / off / seq | 18/0/21; 4/0/4; 122/0/792 | `nvashpile01.nif`(14); `slot_machine01.nif`(169) | read+used `model.rs:404`, `anim_play.rs:128` | — | — |
| BSRefractionStrengthController free / off / seq | 2/0/0; 2/0/0; 51/0/452 | `fxmrhandyjetaddon.nif`; `nvpinto_fruited.nif`(138) | read+used (`model.rs:397`, `anim_play.rs:130`); drawn only on shapes with the refraction flag (refraction.md) | — | — |
| BSRefractionFirePeriodController seq | 13/0/451; kf 2 | `nvpinto_fruited.nif`(138), `nv_pricklypearcactus.nif`(129); `2hareloadj.kf` | **not read**: the period is read once from the property (`nif_spawn.rs:263`); effects audit §5 says "not played" | the fire-refraction period over time [hypothesis]; invisible on shapes without the fire-refraction flag (the plants, probably) [hypothesis] | `anim_play.rs:122` |
| NiGeomMorpherController | see §7.2 | | | | |
| NiFloatExtraDataController free on NiNode / BSFadeNode, off | 14/0/1949; 2/0/0; 3/0/97 | `skeleton.nif`(1823), `deathclaw\skeleton.nif`(55); `legionhelmetbase_go.nif`; `ghoul\skeleton.nif` | **not read** (no code names the type). Drives `HeadTrack` (`NiFloatExtraData`, 56 files/2080 refs); "Not read" in docs/design/dialogue-play.md:530-532, 662 | head-tracking weight [hypothesis] | with head tracking (dialogue-play.md §8) |
| NiLightColorController / NiLightDimmerController seq | 9/0/91; 1/0/0 | `lamplightwarm.nif`(57), `klaxonglow.nif`(17); `monumentlight01.nif` | **in progress elsewhere** | — | — |
| BSFrustumFOVController free | 56/0/0 | `vatscameras\bulletflybycam01.nif` | not read | VATS cameras' FOV (VATS not built) | VATS |
| NiBSBoneLODController free | 34/0/2821 | `skeleton.nif`(1823), `nvsecuritron\skeleton.nif`(104) | not read | bone LOD, speed only (nif-node-audit row 22) | — |
| bhkBlendController free | 43/0/2927 | `skeleton.nif` | not read | blends Havok and keyframed bodies [hypothesis]; ragdoll work | ragdoll |
| NiMultiTargetTransformController seq | 847/0/14010 (BSFadeNode); 8/0/0 (NiNode, `interface\testloadanim01.nif`) | `nvfungusstalk03.nif` | container only; tracks reach nodes by name (`anim.rs:684`) | — | — |
| NiControllerManager (flags 76, cumulative 0) | 855/0/14010 | `nvfungusstalk03.nif` | read implicitly: Idle autoplays from load (`model.rs:1106`); cumulative is 0 everywhere [observed], so it needs no handling | — | — |
| NiLookAtInterpolator (flags 4 = look_z_axis) | 6/0/0 | `nv_roulette-table.nif` | partly: place read (`anim.rs:610-617`), aim done by `roulette_view.rs:234`, `325` | [documented: LookAtFlags schemas/nif/anim.json:41] | — |

### 3.4 Sequences and interpolators

| What (seq) | Files / kf / placed | Example | Placed (idle/groups) | Actors (NPC/creature) | Player 1st/3rd | Fix |
|---|---|---|---|---|---|---|
| NiTransformController × NiTransformInterpolator | nif 573/0/8475; kf 4316 | `footlocker01.nif`(553); `_1stperson\1gtaim.kf` | played (Idle/door/groups `model.rs:1096-1133`, `Playing::step` `anim_play.rs:323`) | played (`anim_blend.rs`, priorities) | played (`player_body.rs`) | — |
| × NiBSplineCompTransformInterpolator | kf 3516 (+4 old-style) | `1gtattackthrow.kf` | — | played (`anim.rs:618-640`, handle 65535 = static `anim.rs:541`) | played | — |
| × NiBSplineTransformInterpolator (uncompressed) | kf 4 | `dynamicidle_nvdieingsoldier.kf`, `dlc05\creatures\alien\locomotion\1hmforwardwalk.kf` | — | read (`float_control_points`, `anim.rs:553`); scale_handle 65535 → static [observed survey] | — | — |
| × BSRotAccumTransfInterpolator | kf 21 | `_1stperson\2hmattackspin.kf`, `2hmattackspin2.kf` (`##SawBlade`, nif-node-audit row 5) | — | **not played**: `TrackData::Other`, skipped (`anim.rs:641`, `anim_play.rs:162`) | **not played** | it is an NiTransformInterpolator with no own fields [documented: schemas/nif/anim.json:273]; reading it as one gives the keys; the accumulation of rotation across loops is a [hypothesis] | `anim.rs:563` (add it to the NiTransformInterpolator case, then accumulate per cycle) |
| BSTreadTransfController × BSTreadTransfInterpolator | kf 5 | `_male\1hmidle.kf`, `_1stperson\1hmidle.kf`, `sneak1hmidle.kf` (node `Ripper`) | — | **not played** | **not played** | tread transforms (`tread_transforms`, `data` ref [documented: schemas/nif/anim.json:325-331]) move the Ripper's chain; the formula is unknown | new case in `anim.rs` |
| NiTransformController × NiPathInterpolator (seq) | nif 6/0/51 | `fxdustwhirlwind01.nif`(47), `fxambdust01.nif`(4) | not on main; PR #621 | — | — | merge #621 |
| × NiLookAtInterpolator | nif 6/0/0 | roulette | `roulette_view.rs` | — | — | — |
| NiMaterialColorController × Point3 / BSplineCompPoint3, SELF_ILLUM | nif 226/5157; kf 159 + 154 | `nvfungusstalk03.nif`; Securitron `mtidle.kf`, `1hpattack3.kf` | played (`material_anim.rs:193`) | played (`actor_materials.rs`, `material_anim.rs:273-282`) | played (effects audit §1) | — |
| … SPEC | nif 15/472; kf 18 | `nvpinto_fruited.nif`; `2hareloadj.kf` | **dropped** `anim_play.rs:127` | dropped | dropped | §3.3 SPEC row |
| NiTextureTransformController × Float / BSplineCompFloat, TT_* | nif 10-140 per member (47-427 refs); kf 27-65 + 2-9 | `forcefieldrightimpact01.nif`(66); `1hpequip.kf` | played | played | played | — |
| BSMaterialEmittanceMultController × Float / BSplineCompFloat | nif 122/792; kf 21 + 24 | `slot_machine01.nif`; `1hpattack8.kf` | played | played | played | — |
| NiAlphaController × Float / BSplineCompFloat | nif 186/1631; kf 21 + 17 | `maize_healthy01.nif`; `_male\1hpequip.kf` | played | played | played | — |
| BSRefractionStrengthController × Float / BSplineComp | nif 51/452; kf 3 + 1 | `nvpinto_fruited.nif`; `mistergutsy\idleanims\specialidle_activate.kf` | played | played (`MaterialTarget::Refraction`) | — | — |
| BSRefractionFirePeriodController × Float | nif 13/451; kf 2 | `nvpinto_fruited.nif`; `2hareloadj.kf`, `smspinebreaker\2hareloadf.kf` | **dropped** | dropped | dropped | `anim_play.rs:122` |
| NiVisController × NiBoolInterpolator / NiBoolTimelineInterpolator | nif 96/88; kf 583 (+1 timeline) | `forcefieldrightglow02.nif`(36); `1hpaim.kf` | groups yes; **Idle on plain rigs in progress elsewhere** | weapon parts (`anim_blend.rs:345`) | weapon parts (`player_body.rs`) | — |
| NiFloatExtraDataController × Float / BSplineCompFloat | kf 1137 + 44 | `_male\1gtattackthrow.kf`, `1hpattack3is.kf` | — | **not read** (`Track.values` kept, no consumer) | **not read** | the HeadTrack weight per clip [hypothesis: dialogue-play.md:530] | head tracking |
| NiGeomMorpherController × Float / BSplineCompFloat | kf 288 + 22; nif 18/69 | `_1stperson\1hpaim.kf`, `1hpequip.kf`; `tripwire01.nif`(44) | Idle/SpecialIdle only (§7.2) | **not played** | **not played** | §7.2 | §7.2 |
| NiLightColor / NiLightDimmer | nif 9/91, 1/0 | `lamplightwarm.nif` | in progress elsewhere | — | — | — |
| NiPSys*Ctlr tracks (emitter on/off by NiBoolTimelineInterpolator etc.) | 97/147 timeline | `fxdustwhirlwind01.nif`, `fxsprayjet01.nif`(30) | §4 | — | — | — |
| Old-style kf (NiStringPalette offsets, controller type "?" in survey) | 5 kf (+4 BSplineComp, 5 Transform) | `_male\idleanims\eatidle.kf`, `pistolvariant01.kf`, `talk_handsatside_moving.kf` | — | read: `cb_string` follows `*_offset` into the palette (`anim.rs:501-518`); `eatidle.kf` binds 27 of its 50 tracks, the rest name bones FNV lacks [observed nif.md §5] | — | — |
| Sequence cycle_type 0 / 2 | 1576/1156 kf/5462; 3742/3190/9251 | `nvfungusstalk03.nif`; `footlocker01.nif` | used `cycle_time` (`anim_play.rs:319`) | used `anim_blend.rs` | used | type 1 is absent from the data [observed] |
| Sequence frequency | all | — | used `anim_play.rs:169`, `325` | used `anim_blend.rs:393-396` | used | — |
| Accum root name | all kf | `mtforward.kf` | the root track is not applied on objects [design, `object_anims.rs` header] | used: in-place root motion (`anim_play.rs:297-309`, BoundClip) | used | — |
| Sequence weight; anim_notes / anim_note_arrays | — / 125 kf (BSAnimNotes) | `intromovieturn.kf`, `3rdp_brahminpush.kf` | — | **not read** (schema fields `anim.json:561`, `570-572`) | not read | notes of type GRABIK (arm) and LOOKIK (gain, state) at a time [documented: AnimNoteType, schemas/nif/anim.json:45-47, 253-262]: when a hand IK grabs and how strongly the look IK applies | sequences() `anim.rs:646` + IK work |
| NiBlend{Bool,Float,Point3}Interpolator own values | 206/384, 380/2546, 236/5249 (flags 1 = manager controlled, so no blend state stored [documented: anim.json:338]) | `nvbouldersm.nif`(89) | not read (no code names the type); undriven members keep the NIF's stored values | [hypothesis] no gap: with flag bit 0 set nothing is stored to read | — |


## 4. Particles

| What (type/field/value) | Files / placed refs | Example file | Engine status (code path) | What it should do [tag] | Fix location |
|---|---|---|---|---|---|
| **NiPSysSphereEmitter** volume | 48 / 85 | neoclassicalsconcefire01 (76, r 3), fortfirefx (1, r 192), explosiongrenadefrag (2, r 12) | **wrong**: treated as "round" like the cylinder (A:938) with `size.z = f("height")`, a field the sphere does not have → 0, so births come from a flat disc at z=0 (E:449-452) | Birth points inside a sphere of `radius` [documented: nif.xml NiPSysSphereEmitter "within a sphere"]. Invisible at the sconce (r 3); visible on large explosion/fire spheres (r 12–474) | A:938 (keep a sphere flag), E:448 (sample a ball) |
| Gravity `turbulence` / `turbulence_scale` | 64 / 126 (turbulence>0) | neoclassicalsconcefire01 (76), dlc03fxdarksmoke (13), fxhelios_steam (10), fxlightsmoke (10) | not read (A:1095-1106) | Random jitter of the particle's velocity, strength by `turbulence`, spatial/temporal frequency by `turbulence_scale` [hypothesis: names only; already "not built" in particles.md §5] | A:1095, E:528 |
| Gravity `world_aligned` | 1 → 38 / 20; 0 → 180 / 256 | 1: fxwaterspray01 (13); 0: neoclassicalsconcefire01 (76), oasistorch01 (26) | not read. Planar gravity uses the axis through the file's node chain (A:1098) and is **not** turned by the reference's rotation (E:529 uses `g.direction` as game space) | world_aligned=1: axis in world axes; 0: axis follows the gravity object, i.e. turns with the reference [hypothesis: name]. Visible only on tilted refs (most refs turn about z, axis is z in all 211 files) | E:529 (apply `q.frame` rotation unless world_aligned) |
| Gravity `decay` | 4 / 2 (>0) | ul_fountainnewfx (1), hchamberaftereffect (1) | not read | Fall-off of strength with distance [hypothesis: name] | A:1095 |
| Gravity `force_type` 0/1 | 200/266, 20/13 | 1: fxhelios_steam (10) | read+used (A:1103, E:529) | — | — |
| Drag `range` / `range_falloff` | 4 / 2 finite (others 3.4e38) | ul_fountainnewfx (1) | not read (A:1107-1111) | Drag only within `range` of the drag object, fading to `range_falloff` [hypothesis: names] | A:1107 |
| Drag `percentage`, axis | 79 / 97 | fxsprayjet01 (30) | read+used, fitted steps/s (E:535-539, `DRAG_STEPS_PER_SECOND` hypothesis) | — | — |
| Planar collider `spawn_on_collide=1` + spawn modifier (`min/max_num_to_spawn`, `spawn_speed/dir_variation`, `life_span*`, `percentage_spawned`=1, `num_spawn_generations`) | 9 / 24 (spawn on collide); 19 / 29 (generations>0) | fxdrippingsewage01 (7, spawn 19–25), fxdrippingsewage02/03 (4+4), fxdrippingblood02/03 | not read: colliders read bounce/die only (A:1015-1056); spawn modifier ignored (A:1114) | A drip that hits the floor dies and bursts into `min..max` new particles with the spawn modifier's speed/direction chaos and life [hypothesis: names; particles.md §6.4 "not built"]. Visible: sewage/blood drips make no splash | A:1015, E:542 |
| AgeDeath `spawn_on_death` | 0 in all 307 / 1239 | — | ignored (A:1114) | **No file sets it** [observed] → "spawn on death" in particles.md §5/§7.6 is moot; drop from the open list | docs |
| GrowFade `fade_generation`=1 / `grow_generation`=0 | 3 / 5 (fade gen 1) | nv_waterfall (4), dlc03fxppwaterfall (1) | read grow/fade time + base_scale (A:1066-1070, E via `scale()` A:631); generations not read | Fade (shrink) applies only to particles of that generation [hypothesis: name]; with no spawning, gen-0 waterfall drops would not shrink at death in game, they do in ours | A:1066 |
| Rotation `random_axis`, `axis`, `random_rot_speed_sign` | axis: 273 random / 3 fixed (0 refs); sign 259 / 1151 | fxlightdustparticleswide02 (161) | sign read+used (A:1092, E:466); axis fields not read | Axis matters only for non-camera-facing particles [hypothesis]; all FNV particles are drawn camera-facing → no visible gap | — |
| `NiPSysInitialRotAngleCtlr` / `RotSpeedCtlr` / `RotSpeedVarCtlr` | 1 / 0 each | animobjects\nvvomit.nif (idle anim object) | not read → logged as `unread` (A:1148) | Key the rotation modifier's initial angle/speed/variation over the controller's time, like the emitter float controllers [hypothesis: names, particles.md §6.6] | A:52 `EmitterParam` + rotation |
| `NiPSysResetOnLoopCtlr` | 1 / 1 | nvlucky38reactor (start 1.167, stop 3.5) | not read (`unread`, A:1148); particles.md §7.6 lists it | Clear the system's particles when the controller loops [hypothesis] | A:1128 |
| `BSWindModifier.strength` | 4 / 2 | explosiongrenadefrag (2), explosionnukacaps; strength 0.03 / 0.5 | not read (`unread`, A:1115) | Push by the weather's wind × strength [hypothesis: name] | A:1115, E:simulate |
| `BSParentVelocityModifier.damping` | 2 / 0 | bloodspraylimb01 (0.05), explosionincendiarygrenade (0.75) | not read (`unread`) | Particles inherit the emitter's (parent's) velocity, damped [hypothesis: name]. Shot-effect only | A:1115 |
| `NiParticleSystem.world_space` | true 279 / 1215; **false 29 / 24** | false: fxhelios_steam (10), snowglobes_* (2 each, 14) | not read: every particle lives in game space from birth (E:440-480 `start` = node × emitter) | world_space=0: particles move with the system's node [documented name: nif.xml]. Same look while the ref stands still; a knocked/carried snow globe (MISC) would leave its snow behind | E:emit/draw (store local, draw through the node) |
| `NiParticleSystem.flags` bit 18 (786446) | 44 / 13 | effects\nv\sanddust\sanddust02 (8), nvvomit | not read | Meaning not documented in our schema/nif.xml bits for NiAVObject [unknown]; bits 1,2,3,19 are the usual 524302 | — (find out) |
| NiPSysData / BSStripPSysData `bs_max_vertices` | 304 / 1239 | wide02 (60), sconce (14) | read: system cap (A:867) | — | — |
| NiPSysData `has_texture_indices` + `num_subtexture_offsets` (1/2/4/8/16), `subtexture_offsets` | 226 / 928 with indices; consistent: indices=1 ⇔ offsets>0 [observed] | wide02 (16 cells) | read; a cell picked at random at birth (A:887-889, E:479) | Random pick per particle [hypothesis]; no flipbook animation block in FNV data | — |
| NiPSysData `has_rotation_speeds`/`has_rotation_angles`/`has_radii`/`has_sizes`/`has_vertex_colors`, `num_active`=0, keep/compress/consistency flags=0 | 304 / 1239 | — | not read | Run-time buffers: `num_active` 0 in all [observed]; spin and angle come from the rotation modifier. No gap | — |
| NiPSysData aspect ratio / `bs_data_flags` | not in this version's schema (`bs_data_flags`=0 all) | — | — | No gap | — |
| BSStripPSysData `max_point_count` 4–33, `start/end_cap_size` 0, `do_z_prepass` 0 | 9 / 13 | fxwaterspray01 (13, 9 points), flamerflame01 | max points read (A:886), caps 0 everywhere | — | — |
| Mesh emitter `emission_type` 3/4, `initial_velocity_type` 0 | 64 / 108 | fxdustwhirlwind01 (47) | read+used (A:958-1001) | Dust devil path: #621 | #621 |
| Box / cylinder emitters, all emitter fields | 191/608, 84/535 | nvbouldersm (89), wide02 (161) | read+used (A:935-956). Cylinder height taken along emitter z [hypothesis, not checked] | — | — |
| Bomb `decay_type` 0/1/2, `symmetry_type` 0/1/2 | 28 / 59 | fxdustwhirlwind01 (47), nvnellisartillery (3, exponential), helioslaser (1, planar) | read+used (A:1002-1014) | — | — |
| NiPSysColorModifier + NiColorData (linear keys) | 10 / 18 | fxhelios_steam (10) | read+used (A:1058) | — | — |
| BSPSysSimpleColorModifier fades/colours | 289 / 1180 | wide02 (161) | read+used (A:1071-1085; percentages in file order) | — | — |
| Spherical collider | 9 / 14 | snowglobes_* | read+used (A:1036-1047, particles.md §7.3) | — | — |
| Modifier `active` | all 1 | — | read: inactive modifiers skipped (A:931) | — | — |
| Modifier `order` (0, 1000, 2500, 3000, 4000, 5000, 6000, 7000) | all | — | not read; fixed order in `simulate` (forces → collide → move, E:528-560) | Data only uses the standard NiPSysModifierOrder values [observed] in the order the sim applies them → no gap | — |
| NiPSysEmitterCtlr flags 72 (active) / 104,108 (+manager-controlled, clamp) | 77/887, 83/166, 108/199 | wide02; nvbouldersm (89, clamp, 20 s) | cycle bits read (A:1142); bit 5 → keys from sequence when own are empty (A:1162-1173) | nvbouldersm emits from the start (§7.5, open) | — |
| NiPSysEmitterCtlr / NiPSysUpdateCtlr **inactive** (flags 65/69: app-init, no bit 3) | 25 / 0 (same 25 files) | impactsparks, impactzap, impactdisplacer, laerprojectile, dynamitethrown | active bit not read; these systems emit and update as if on | Shot-effect models with the app-init bit: the game starts their controllers when it places the effect [hypothesis] → ours matches; placed models never have it. Answer to "should they not update": no change | — |
| NiPSysEmitterCtlr `phase` ≠ 0 | 165 / 1053 | wide02 (161), wide01 (129), fxradiationdust1x1 (80), sconce (76) | not applied: every reference's emitter clock starts at 0 (E:378 `em.age`) | Controller time = t × frequency + phase [documented: NiTimeController fields]; copies of a file run in step. Visible only where visibility/rate keys vary in the cycle | E:emit (start age at phase) |
| NiPSysEmitterCtlr `start_time` ≠ 0 | 13 / 3 | forcefieldleftglow02 (1), nvlucky38reactor (1), enclavemainentrance01 (1) | `period = stop_time` (A:1140), start ignored | Period = stop − start, keys from start [documented: NiTimeController] | A:1140 |
| NiPSysEmitterCtlr `visibility_interpolator` | 270 / 1239 | — | read+used (A:1139) | — | — |
| NiPSysModifierActiveCtlr | 96 / 147 | fxdustwhirlwind01 (47), protectronpod01 (15), fxsprayjet01 (30) | read for gravity, drag, bomb (A:907, 929) | Other modifiers it may name (none checked) | — |
| Emitter float ctlrs (Speed, InitialRadius, LifeSpan, Declination(Var), PlanarAngle(Var)), GravityStrength | 1–43 / ≤38 | forcefieldrightglow02 (36), nv_hooverdam_aagun (1), ul_fountainnewfx (1) | read+used (A:52-62, 901-920) | — | — |
| BSPSysMultiTargetEmitterCtlr `max_emitters` 1–15 | 37 / 0 (ADDN models) | mpsfiresmall01 (10) | read+used, nearest first (A:1143, E:398-420) | — | — |
| BSMasterParticleSystem `max_emitter_objects`=20 | 37 / 0 | — | not read | All controllers' `max_emitters` ≤ 15 < 20 [observed] → no gap | — |
| NiPSysBoundUpdateModifier `update_skip`=0, NiPSysPositionModifier | 307 / 1239 | — | implicit (A:1114) | — | — |
| Emitter moved by a sequence | 47 + | fxdustwhirlwind01 | not on main; built in #621 (A `Mover`) | — | #621 |


## 5. Collision and Havok

| What (type/field/value) | Files / placed refs | Example file | Engine status (code path) | What it should do [tag] | Fix location |
|---|---|---|---|---|---|
| Body `havok_filter.layer` (values 0–28 seen) | all bodies | 1 STATIC (nvn_rockcanyon12 10,747), 13 TERRAIN, 26 TRANSPARENT_SMALL (yuccacnv 1,662), 15 NONCOLLIDABLE (nv_barrelcactus01 191, maize 233), 4 CLUTTER, 8 BIPED | read (C:474); used by ray/character filters only: `character_ignores` (P/layers.rs:144), projectile rays (nvf-play combat.rs:1532-1586), actor volumes | Layer matrix not known (physics.md §4.2) — gap: **body vs body has no layer filter** (all rigid bodies collide, P/world.rs:404-470), so dynamic clutter, thrown items and ragdolls rest on NONCOLLIDABLE plants (1,564 + 685 body refs) and on DEBRIS_SMALL bodies [hypothesis: the game does not] | P/world.rs `attach` (collision groups from layer) |
| Rigid-body-info copy of the layer / `collision_response` 0 | 58 / 826 (copy 0) | scolroadstraightlongbcapped02b (150) | not read; `entity_info.collision_response` is 1 in all 6,125 bodies [observed] | Copy unused [observed: collision.md §4] → no gap | — |
| `havok_filter.flags` bits 0–4 biped part | 73 / 3,550 | _male\skeleton (1823) | not read; ragdoll parts come from bone chains (nvforever/ragdoll.rs:280) | No gap | — |
| filter bit 5 `mopp_scaled` | 24 / 257 + 21 / 301 | scolparkinglotchunk04b (127) | not read; MOPP code is skipped (C:283) | No gap | — |
| filter bit 7 `linked_group` | 102 / 2,022 + 118 / 645 | sscratetallstatic (373), bucket01 (292) | not read | Bodies joined by constraints do not collide with each other [hypothesis: name]. Moot while jointed clutter is held fixed (physics.md §4.3.1); needed once constraints for placed refs are built | P/world.rs |
| filter bit 6 `no_collision`, `group` | never set; group 0 everywhere | — | — | No gap | — |
| Sub-part layer (`hkPackedNiTriStripsData.sub_shapes.havok_filter.layer`) | 6,294 / 160,951 | — | not read | Equals the body's layer in every file but one (tlfencebackyard, 5 refs: sub-part 0 under body 1) [observed] → no gap | — |
| `motion_system` 2,4,6,7,8 | 64/1,022; 835/27,783; 194/6,473; 5,094/110,179; 2/179 | glasspitcher01, tincan01, skeleton, rocks, woodplank02 (8 THIN_BOX) | read+mapped (C:577-584): 6 keyframed, 7 fixed, others dynamic; STAT/SCOL dynamic bodies fixed (physics.md §4.3.2) | — | — |
| `quality_type` 1/2/3 (62 junk) | 5,094 / 110,179; 194 / 6,473; 880 / 28,805 | tincan01 = 3 DEBRIS | read (C:476), unused | Havok quality picks continuous collision (bullet/critical vs debris) [documented names]; debris could skip CCD [hypothesis] — no visible gap known | P/world.rs |
| `deactivator_type` 1 NEVER / 2 SPATIAL; `solver_deactivation` 1 OFF / 2 LOW (25 junk) | 1,054/28,506; 5,102/116,560; 5,058/108,638; 1,109/36,818 | sandbag_single01 (NEVER); skeleton (LOW) | not read; rapier's own sleeping, clutter added asleep (P/world.rs:404-420) | When a body may fall asleep [documented names]; behaviour difference not measured [hypothesis] | — |
| Rigid body `linear_damping`, `angular_damping`, inertia, centre, max velocities, penetration depth | all dynamic bodies | — | damping read for ragdolls only (R, `ragdoll()` body loop); placed dynamic clutter gets mass/friction/restitution (P/world.rs:404-470), no damping | Clutter should slow as stored (0.1 / 0.05 typical) [hypothesis: Havok semantics]; rolling cans may roll on longer | C:445 (`RawCollision`), P/world.rs insert_ref |
| `bhkRigidBodyT.body_flags`=1 | 3 / 57 | clutter	umbleweednv (56) | read: bit 0 makes a dynamic body respond to the weather's wind (`CollisionObject::wind`, since 2026-11-06) | "1 = respond to wind" [documented: nif.xml bhkRigidBody Body Flags]; the original's tumbleweeds roll downwind after a load (o27_tumble) | done: [wind](wind.md) |
| `bhkCollisionObject.flags` 1 vs 41 | 8,011 / 209,463 vs 828 / 16,728 | 41: yuccacnv (1,662), skeletons (bhkBlend, 41) | not read | Bits [documented: nif.xml bhkCOFlags]: 41 = active + set_local + use_vel. Spread over layers 26, 1, 2, 3 [observed]; no behaviour known → no action | — |
| `bhkSPCollisionObject` flags=1 + `bhkSimpleShapePhantom` layer 11 WATER | 8 / 102 | waterp2048x2048 (54) | read as volumes (C:449), water used (prepare.rs:141, P/water.rs) | — | — |
| phantom layer 12 TRIGGER | 29 / 120 | beartrap01 (46), tripwire01 (44), grenadebouquet01 (19), pressureplate (7) | used: script trigger volumes (prepare.rs:142, nvf-play script_trigger.rs) | — | — |
| phantom layer **24 GASTRAP** | 2 / 124 | traps\trapgasfire01 (124), dlc04biogastrap01 | **dropped** (prepare.rs:142 keeps only WATER and TRIGGER) | Gas volume that ignites on fire/explosions [hypothesis: layer name + model name; not in traps.md] | nvf-play phys/prepare.rs:142, traps |
| Phantom `transform` (mat44) | 39 / 346 | tripwire01 (z −1.5), trapmailboxbomb01 (z 12.15) | not applied (C:118-123 comment) | Its translation equals the target node's own place ÷ 7 (tripwire node "Trigger" z −10.5, mailbox "Box01" z 85.05) [observed] → applying it would double the offset; current rule correct, but the comment's reason (water identity) should cite this | docs only (collision.md / C:118) |
| Shapes: box, sphere, capsule, convex vertices, list, (convex) transform, MOPP, packed strips, NiTriStrips | 640/17,391; 22/679; 97/3,996; 1,538/43,528; 126/2,546; 41+31; 6,295/160,799; 6,294/160,951; 1/3 | — | all read (C:240-290); MOPP code skipped; convex `normals` (planes) unused; `radius` → contact shell | — | — |
| `bhkPackedNiTriStripsShape.user_data`=12 | 3 / 324 | trees\nv_jtown_trees\nv_euroaspen02 (138) | not read | Meaning unknown [unknown] | — |
| `hkPackedNiTriStripsData.compressed` | 0 in all | — | compressed path flagged (C:301) | No gap | — |
| Havok material per convex shape (`material`, 0–31) | e.g. 25 SODA_CAN tincan01 1,292; 24 BOTTLE ssbottle02 921 | — | **partly**: only the first convex shape's material per body (C:225, `convex_material`) | A list/transform shape with children of different materials gives each hit its own child's material [documented: nif.xml material per bhkSphereRepShape] | C:225, P/world.rs `material_of` |
| Material per packed triangle (sub-part) | 6,294 / 160,951 | footlocker01 16 HOLLOW_METAL | read per triangle (C:327-346), used: impact IPCT, knock sounds, footsteps (`HAVOK_MATERIALS` mod 32, nvf-play world_sounds.rs:73; anim_sounds.rs:266) | — | — |
| Material 32–127 (PLATFORM / STAIRS variants) | 64 STONE_STAIRS 24/109, 69 METAL_STAIRS 31/137, 80 HOLLOW_METAL_STAIRS 63/200, 73, 76, 81, 127 | utlcatstairsendground01 (30, REPCONN catwalk stairs), vhallsmstairsr01 (46) | folded mod 32 for sounds; **stairs flag unused** by the character controller (P/character.rs) | Stairs materials may be what lets the game step smoothly up stair meshes [hypothesis]; relevant to physics.md §6.9.1 (REPCONN catwalk stairs) | P/character.rs |
| bhkRagdollConstraint, LimitedHinge, Hinge, StiffSpring, BallAndSocket | 62/3,376; 150/2,037; 4/128; 1/0 | skeleton; bucket01 (292); globe01 (104) | actor death ragdolls: read+built (R:120-240, P/ragdoll.rs:200-220); placed clutter: not modelled, jointed parts held fixed (physics.md §4.3.1) | Clutter joints (globe spin, bucket handle, hanging lights 131) [documented: physics.md §4.3.1 open] | P/placed.rs |
| bhkPrismaticConstraint | 2 / 26 | creatures\protectron\skeleton (26) | **wrong-ish**: `JointKind::Other` → ball joint (R:156, P/ragdoll.rs:214) | Slide along an axis between limits [documented: nif.xml name; ragdoll.md §3 "held as a ball joint"] | R:150, P/ragdoll.rs |
| bhkMalleableConstraint `strength` (inner types 2 and 7) | 4 / 1,823 | _male\skeleton | read (R:180), unused by the joint builder | Joint softness [hypothesis]; ragdoll.md §4 uses damping instead | — |
| bhkBreakableConstraint `threshold`=1, `remove_when_broken`=0 | 1 / 26 | protectron skeleton | inner limited hinge read; threshold not read (R:226 reads `strength` from the wrapped data, which has none) | Break above threshold [hypothesis] | R:226 |
| Constraint motors (`motor_type` 1 position, `motor_enabled`) | 2 / 24 (limited hinge), 1 / 11 (ragdoll, disabled) | sentryturret skeleton (22), nvcazadores (11) | not read | Powered joints on the sentry turret's head [hypothesis] | R:read_info |
| `bhkOrientHingedBodyAction` (hinge axis, forward, strength 2–3, damping) | 2 / 24 | sentryturret (22), minisentryturret (2) | not read (listed among constraints, skipped, ragdoll.md §1) | Keeps a hinged body turned to its forward axis [hypothesis: name] — turret ragdolls only | P/ragdoll.rs |
| `bhkLiquidAction` (stick force 25, strength 100, neighbour 128/500) | 2 / 0 | dlc03flamerexplosion, nukagrenadeexplosion01 | not read | Sticky blob debris [hypothesis]; explosion models only, no placed refs | — (low) |
| `bhkBlendCollisionObject` (heir_gain 1, vel_gain 1) + `bhkBlendController` (keys 0, flags 76) | 43 / 2,927 | _male\skeleton (1823) | bodies read as ordinary collision objects for ragdolls (C:426, R:200); blend gains and controller not read | Blend between animation and physics for live actors (hit reactions, get-up) [hypothesis]; NVForever's reactions are animation-only by design (hit-reactions rule) | — |
| bhkNiTriStripsShape filters layer 1, `grow_by` | 1 / 3 | traps\terminaldesktrap01 | read (C:353-368) | — | — |


## 6. Special nodes, extra data and markers

### 6.1 NiAVObject flags, nodes and special nodes

| What | Files / refs | Example | Engine status | Should do | Fix |
|---|---|---|---|---|---|
| flags bit 0 hidden on shapes (NiTriStrips 524303 etc.) | 115 / 443 | utldoor01 (195) | read+used: `model.rs:1069-1070,1229-1248` (skipped unless a vis track shows it), `nif_spawn.rs:920` | [documented: nif.xml NiAVObject flag bit 0] | — |
| flags bit 0 on BSDamageStage / BSBlastNode / BSDebrisNode (524303) | 48 / 184; 47 / 361; 6 / 2 | electricbox1 (21), car01 (20), trapgasfire01 (124) | read: node and **all shapes below are dropped for good** (`model.rs:1239-1248`). Add-on emitters below still gated by stage (`particles.rs:103`) | **destruction-stages.md §4 "shapes below damage-stage nodes are still drawn at every stage" is stale/wrong**: they are never drawn (hidden flag), so damaged-stage looks never appear [observed code + data] | `model.rs` keep shapes under BSDamageStage, show by `damage_stages_of` like `StagedAddon` |
| BSDamageStage min/max | 49 / 184 | car01 (min 2-3, max 3) | read `model.rs:1083-1086` (nodes only) | Shapes gated by model damage stage [hypothesis, destruction-stages.md §2] | as above |
| BSBlastNode, BSDebrisNode (BSRangeNode min/max/current) | 52 / 540; 6 / 2 | minefrag (138, flags 524302 not hidden, max 255); vertibirdencount01_hd (1) | not read as range nodes (`model.rs:150` only `BSDamageStage`); minefrag's children drawn always | Unknown: blast/debris behaviour [hypothesis: explosion/debris pieces at a stage] | later |
| other NiAVObject bits: 1-3 & 19 (nearly all), 4 (NiNode 29 / 330, nv_euroaspen02 138), 7 (NiNode 570 / 8,526, footlocker01 553; NiBillboardNode 51 / 178), 10 (BSFadeNode 77 / 5,143, drinkingglass01 532), 18 (NiTriStrips 804 / 8,342, nvfungusstalk03 830; NiTriShape 23 / 12) | — | — | not read | Meanings not in the schema; unknown — do not guess | — |
| BSFadeNode vs NiNode | 11,075 / 277,738 vs 4,271 / 52,376 | nvn_rockcanyon12 | both plain nodes; distance fade by cell visibility ranges | nif.xml: fade root node [documented] — covered by draw-distance sliders | — |
| **NiBillboardNode on placed models** (modes 1 ROTATE_ABOUT_UP 119 / 107, 2 RIGID_FACE_CAMERA 82 / 784, 3 ALWAYS_FACE_CENTER 23 / 2, 4 RIGID_FACE_CENTER 59 / 110, 5 BSROTATE_ABOUT_UP 18 / 27) | 229 / 1,000 | fxsmokewisps01 (180, mode 2), klaxon (174), neoclassicalsconcefire01 (76, mode 4), oasistorch01 (26, mode 5) | **partly**: mode read `model.rs:1077-1078`; turned to the camera only for shot/effect-shader/weather effect models (`shot_fx.rs:227,283,748-779`). Placed world models bake shapes at the file rotation (`world.rs` has no Billboard; `spawn_emitters` :1083 makes nodes only for add-ons) → smoke wisps, torch/sconce flames, klaxon glows stay flat cards | Turn to camera per mode [documented BillboardMode names; axis hypothesis shot_fx.rs:24-27]. Modes 2/3/4 share one rule in `face_camera` (rigid vs face-centre not told apart) | make placed shapes under billboard nodes node-parented (like rigs) and reuse `face_camera` |
| BSOrderedNode `alpha_sort_bound`, `static_bound` (0: 3 / 174; 1: 75 / 8) | 78 / 182 | klaxon (174), nv_hooverdam_aagun | not read (plain node; nif-node-audit #7 still true) | Children drawn in stored order, not depth-sorted [hypothesis from nif.xml name] | `model.rs` → per-shape sort bias (`nif_material::sort_first`) |
| BSValueNode value; `value_node_flags` bit 0 `billboard_world_z` | 117 / 686; 6 / 21 | barrel02firelight (278); motorcycle01 (20) | value read (`model.rs:149`, ADDN emitters); flag not read | Add-on effect kept world-Z up [documented name] | `particles::place_addon` |
| BSMultiBound / AABB / Sphere / Node | 2,678 / 102 | waterp2048x2048 (54) | not read | Culling bounds; nothing visible [hypothesis] | — |
| BSSegmentedTriShape | 624 / 0 | object LOD blocks | read+used `model.rs:59`, `lod.rs` | — | — |
| NiCamera | 159 / 0 | gambler_fedora_01_go (Camera3rd), vatscameras\stationary | only roulette `Camera01` (`roulette_view.rs`); VATS cameras unused | VATS camera paths (not this area) | VATS |
| BSWArray | 1 / 0 | libertyprimeprojectile | not read | unknown | — |

### 6.2 Extra data (confirming / updating nif-node-audit.md)

| What | Files / refs | Example | Engine status now | Note |
|---|---|---|---|---|
| BSXFlags `BSX` values 1-579 (e.g. 2: 5,853 / 163,558; 66: 608 / 29,867; 10: 580 / 7,493; 198: 41 / 2,908) | 9,441 / 238,077 | nvn_rockcanyon12 | **not read anywhere** (only a doc comment `nif_spawn.rs:368`) — audit #20 still true | nif.xml BSXFlags: 0 animated, 1 havok, 2 ragdoll, 3 complex, 4 addon, 5 editor marker, 6 dynamic, 7 articulated, 8 needs transform updates, 9 external emit [documented: nif.xml, not in schemas/]. Engine derives each from blocks present (controllers, bhk, names, shader bit 29). Possible gap: game may skip controllers when bit 0 is clear [hypothesis] |
| BSBound `BBX` | 50 / 2,935 | skeleton.nif (1,823) | unused (audit #13 unchanged) | — |
| BSFurnitureMarker `FRN` positions | 134 / 8,257 | hotelchair02 (521) | read+used `fnv-actor/src/furniture.rs:33-48`; orientation = millirad [hypothesis, :25-29], fits all values (24…6283); `ref_1` used as marker id (:440); `ref_2` read, equal to ref_1 in every count [observed] | no gap |
| NiStringExtraData `Prn` (Bip01 Head 153 / 68, Weapon 190 / 658, …) | 466 / 726 | glassesblackrimmed (35) | used (`fnv-actor/src/mesh.rs:157`, `actor.rs:403`) | — |
| `UPB` (billboardRigid 6 / 242, billboardCenter, billboardUp, sgoKeep 448 / 3,048, Unyielding, ColGroupInfo, BSBoneLOD, BSPriority, KFAccumRoot, NoDecals, worldaligned 28 / 18) | 9,263 / 230,476 | klaxon (billboardRigid) | partly (weapon bone lines `model.rs:845`) | exporter settings [hypothesis]; `billboard*` lines sit on billboard nodes — mode already in `billboard_mode` |
| NiIntegerExtraData `SkeletonID` | 44 / 2,935 | skeleton.nif | unused (audit #21) | — |
| NiFloatExtraData `HeadTrack` | 56 / 2,080 | skeleton.nif | unused (audit #14; `head_look.rs` uses package flags, not this) | — |
| BSDecalPlacementVectorExtraData `DVPG` | 45 / 272 | poolcue (75), rustyknife (45) | unused (audit #6) | blood decal placement on melee weapons |
| NiBinaryExtraData "Tangent space" | 8 / 0 | testloadanim01 | unused | test files |
| Audit items re-checked | — | — | `SightingNode` (no ##): **now used** (`iron_sights.rs:37`); `ProjectileNode_HeadLaser`: still unused (`player_body.rs:462` MUZZLE_NODES); `BSOrderedNode`: still unused; `AttachLight`: still unused (`model_light.rs` colour only) | update audit #1 to "used" |


## 7. Skinning and morphs

### 7.1 Skinning and morphs

| What | Files / refs | Example | Engine status | Should do | Fix |
|---|---|---|---|---|---|
| NiSkinInstance bones + NiSkinData per-bone `skin_transform`, vertex weights (`has_vertex_weights`=1 everywhere) | 778 / 345 | teddybear01 (74) | read+used `nif_spawn.rs:786-859` (top 4 weights, renormalised) | — | — |
| NiSkinData overall `skin_transform` | 778 / 345 | — | not read (only per-bone) | Applied before bone transforms [documented field]; values not surveyed, likely identity [hypothesis] | `shape_skin` |
| NiSkinPartition (has_vertex_map/weights/bone_indices/faces = 1) | 778 / 345 | — | used only for dismember triangles (`actor.rs:376-418`, strips handled) | Weights taken from NiSkinData instead: same data [hypothesis] | — |
| BSDismemberSkinInstance body_part, part_flag 0 / 1 / 256 / 257 (BSPartFlag bit 0 editor_visible, bit 8 start_net_boneset) | 635 / 139 | kings_outfit_m (32) | read+used `actor.rs:393` (parts), :420-428 (cap = no bit 0) | — | — |
| Skinned clutter placed in the world | teddybear01 74, flatmop01 55 (skin data 345 refs total) | — | partly: drawn as the static bind-pose copy without joints (`model.rs:1302-1304`); only MSTT skeletons with XRGD are posed (`clutter_pose.rs`) | Correct if the file's bone nodes sit at the bind pose; not checked against game frames [hypothesis] | compare a teddy/mop frame |
| NiMorphData `relative_targets`=1 | 39 / 1,950 | skeleton.nif (1,823), nv_ncr_flag | read (`fnv-nif/src/anim.rs:1043`); applied base + Σ w·offset (`morph.rs:42-50`) | — | — |

### 7.2 Morph controllers (NiGeomMorpherController)

| What | Files / kf / placed | Example | Engine status | What it should do | Fix |
|---|---|---|---|---|---|
| free, flags 72 (loop) on NiTriStrips | 12/0/35 | `nv_legionflag.nif`(31), `nvgreatkhanflag.nif`(2) | played: `shape_morphs` `model.rs:443-470`, `morph.rs`, `OwnCycle` `model.rs:1280` | — | — |
| free, flags 76 (clamp) on NiTriShape | 8/0/1846 | `nv_ncr_flag.nif`(9); `_male\skeleton.nif`(1823) | flags: played looping when the weights end where they start (`model.rs:457-463`) [hypothesis there]; the skeleton's morph shape is not a placed model (actors build their own rig) | unknown what the skeleton's morph shape is | — |
| seq, flags 108, in the Idle/SpecialIdle sequence | part of 18/0/69 | `greenhousenvint.nif`(13) | played when Idle drives it (`take_sequence` `anim.rs:964-984`, `model.rs:449-454`) | — | — |
| seq in another named group (Forward/Backward…) | part of 18/0/69 | `traps\tripwire01.nif`(44) | **not played**: only Idle/SpecialIdle feed morphs (`model.rs:445`, `452`); PlayGroup clips carry no morph data (`Clip` has no morph field, `anim_play.rs:16-42`) | the tripwire's snap when triggered (vertex-morphs.md §3 "left out") | `Clip`: add morph tracks; `object_anims.rs` drive them |
| off, flags 69 | 1/0/0 | `effects\explosionsplash.nif` | not played (`effect_clip` takes no morphs) | an effect's splash [hypothesis] | `model.rs:586` |
| In .kf clips | 288 + 22 (BSplineComp) kf | `_1stperson\1hpaim.kf`, `1hpequip.kf` (`##dgRubberBand:0`, effects audit §1, §5) | **not played** on the player, NPCs or creatures: no consumer of `NiGeomMorpherController` tracks outside `model.rs` (git grep) | the rubber band on the weapon in the 1st-person clips [observed in the effects audit]; FaceGen and lip-sync use .tri/.egm/.lip, not this controller | `player_body.rs` + `anim_blend.rs` (morph weights by interpolator ID) |
| always_update = 0, morpher_flags = 0 | 39/0/1950 | all | read (`update_normals` `anim.rs:1042`); no file asks for normal updates [observed survey] | — | — |
| NiMorphData relative_targets = 1 | 39/0/1950 | all | read; absolute morphs are coded but never met | — | — |


## 8. `.kf` clips: text keys

The controller × interpolator rows for `.kf` clips (actors, player, weapons) are in §3.4 "Sequences and interpolators" (columns Actors and Player); morph tracks in clips in §7.2.

### 8.1 Text keys (tk:)

| Key | kf files / placed | Example | Status (file:line) |
|---|---|---|---|
| `start`, `end` | 5306/15126 | every sequence | read: sequence bounds, `fnv-collide/src/anim.rs` `end` |
| `Sound:` / `sound:` | 1285/5538; 88/1131 | `footlocker01.nif`(553), `utldoor01.nif`(195) | read: actors `crates/fnv-actor/src/anim_sounds.rs:67-91`; doors and containers `nvf-play/src/doors.rs:39`; PlayGroup groups `object_anims.rs:265-276`. **Not heard while a placed object plays its own Idle** (sounds need `rig.current`, `object_anims.rs:265`) [observed code]; the second word (node) is dropped (nif-node-audit row 9) |
| `Enum:` (Left/Right/Armor…/Foley…) | 1023/144 | `creatures\dog\skeleton.nif` | read `anim_sounds.rs:76-90`, creature sound types (`engine anim_sounds.rs:482`) |
| `Hit`, `Fire`, `Release` | 827, 53, 38 | `1gtattackthrow.kf` | read `player_clips.rs:437` (`hit`) |
| `Loop` | 53 | `1hmattackspin.kf` | read `player_clips.rs:439` |
| `Eject` | 86 | `2hrattack3.kf` | read `player_clips.rs:441` (nif-node-audit row 2: built) |
| `a:` | 660 | `1gtattackthrow8.kf` | read `player_clips.rs:443` |
| `Attach`/`Detach` (+ lower case) | 161+4 / 122+5 | `1gtunequip.kf` | read `player_clips.rs:433-436` |
| `prn:`/`Prn:`/`PRN:` | 271/67/6 | `1gtaim.kf` | read `player_clips.rs:429` |
| `Blend:` | 1583 (+186 lower case) | `1gtattackthrow8.kf` | read by both `ClipKeys` (`player_clips.rs:425`) and `IdleKeys` (`idle_play.rs:51`) |
| `BlendIn:`/`Blendin:`/`blendin:` | 642/10/3 | `_1stperson\1hpattack3.kf` | **partly**: `IdleKeys` reads them (`idle_play.rs:51`); `ClipKeys` (attacks, movement, player) does not, so attack clips fade in by the default (`attack_anims.rs:387`, `player_body.rs:344-350`) |
| `BlendOut:`/`Blendout` | 52/4 | `1hpattackloop.kf`, `1lmplacemine2.kf` | partly: idles only (`idle_play.rs:49`); attack and loop clips ignore it |
| `Blemd`, `Blen`, `BlendI`, `Blendn` (misspellings) | 7, 4, 4, 2 | `dog\h2haim.kf`, `nvsecuritron\2hhattack4.kf` | not read (only `blenin:` is tolerated, `idle_play.rs:51`) |
| `StartLoop`/`EndLoop` (all cases) | 153+236+9 / 154+231+13 | `1stp_ms18_fixelevator.kf` | read `idle_play.rs:45-48` (whitespace removed, case ignored) |
| `LoopStart`/`LoopStop` | 3 | `nvsporecarrier\idleanims\specialidle_unconsciousfacedown.kf` | **not read** (nif-node-audit row 19: still true) |
| `Hold` | 30 | `1gtattackthrow.kf`, `mt_hithead.kf` | **not read** (row 19: still true) |
| `PM` | 4 | `queenant\2hl_equip.kf` | **not read** (row 19) |
| `Decal:` | 127 | `1hmattackbackpower.kf` | **not read** (row 6: still true; only a test, `player_clips.rs:582`) |
| `m:` (`m:L`, `m:1`, …) | 248/26 | `protectron\skeleton.nif`, `swimmtforward.kf` | **not read** (row 16: still true) |
| `-GlobalRatio`, `-ObjRatio`, `-AttackLeftRatio`/`-AttackRightRatio` | 192/217; 10; 2+2 | `nvsecuritron\skeleton.nif`(104), `nv_dealercards.nif` | **not read**; only fnvtool counts them as options (`crates/fnvtool/src/actor_anims.rs:120`); meaning unknown |
| `A` | 2 | `_male\1hpattack6isdown.kf` | not read; unknown [hypothesis: a misspelt `a:`] |


## 9. Checked, no gap

### Geometry, shaders, extra data, skinning

- Specular flag + normal-map alpha mask (`nif_spawn.rs:548`).
- Vertex alpha (SF bit 3, NL bit 19) and NL falloff (bit 6 + fields).
- Decal bits 26/27 depth bias; external emittance bit 29; refraction 15/16 + fire period; hair 18; face-gen 10; skin shader type 14; LOD building SF2 bit 2.
- NiAlphaProperty: 237, 4097, 4109, 4163, 4333, 4608, 4620, 4640, 4674, 4844, 4845; test func GREATER; no-sort bit never set in data.
- NiStencilProperty 19840 → DRAW_BOTH → double sided; stencil itself disabled (bit 0 = 0).
- NiTexturingProperty base texture + MODULATE apply mode; NiSourceTexture external DDS fields.
- NiMaterialProperty alpha, emissive colour, emissive mult, glossiness.
- Hidden bit 0 on nodes and shapes; editor markers by name.
- Vertex colours, UV set 0, normals; consistency/keep/compress flags; BSSegmentedTriShape segments.
- Skin weights from NiSkinData; dismember partitions and cap flag; NiMorphData relative targets.
- BSFurnitureMarker orientation/refs; Prn; SightingNode now used.
- BSMultiBound*, NiAdditionalGeometryData, NiCamera (except VATS), BSWArray, TileShaderProperty, NiBinaryExtraData: no visible loss (LOD/test/HUD files, 0 placed refs).


### Animation

- Linear, quadratic (Hermite, tangent order checked on the data, nif.md §5), TBC floats and points, constant step, all sampled in `Keys::sample` (`anim.rs:94-120`).
- XYZ euler: each axis by its own key kind (2: 3110 files), `anim.rs:579-591`.
- NiBoolData (always 5) steps (`anim.rs:455`); NiBoolTimelineInterpolator reads as a bool (`anim.rs:676`).
- B-spline compressed float/point3/transform; handle 65535 = static (`anim.rs:541`); uncompressed transform (4 kf) through `float_control_points`.
- Cycle types loop/reverse/clamp for sequences and free controllers (`anim_play.rs:683`); frequency on objects, actors, blackjack, roulette and lockpicking.
- Off controllers ignored on placed objects, played once in effects (`model.rs:375`, `503`, `590`).
- Manager-controlled controllers excluded from free reading; their sequences play them.
- NiControllerManager cumulative 0 everywhere; NiMultiTargetTransformController is a container; NiDefaultAVObjectPalette is replaced by node names.
- Old-style kf string-palette offsets (`anim.rs:501-518`; `eatidle.kf` checked in nif.md §5).
- Material tracks: SELF_ILLUM, emittance multiplier, alpha, texture translate/scale/rotate, refraction strength, free and in sequences, on placed objects, actors and the player.
- NiTextureTransformController: only slot 0 / `0-0-` met, and that is what is read.
- NiLookAtInterpolator: roulette camera aimed in `roulette_view.rs`.
- Morph always_update / morpher_flags / relative_targets: single values everywhere, read.
- Text keys `Eject`, `Attach`, `Detach`, `prn`, `Hit`/`Fire`/`Release`, `Loop`, `a:`, `Enum:`, `Sound:`, `StartLoop`/`EndLoop`, `Blend:`: read where they matter.


### Particles and Havok

- Modifier `order`: only standard values, applied in that order by the sim.
- Modifier `active`: all 1; read anyway (A:931).
- Inactive (app-init) emitter/update controllers: 25 shot-effect files, 0 placed refs; emitting them matches the game's start-on-place [hypothesis].
- NiPSysData run-time arrays (`num_active` 0), compress/keep/consistency flags 0, `bs_data_flags` 0.
- Subtexture offsets vs `has_texture_indices`: always consistent.
- BSMasterParticleSystem `max_emitter_objects` 20 ≥ every `max_emitters`.
- NiPSysBoundUpdateModifier `update_skip` 0; strip caps 0; `do_z_prepass` 0.
- Rotation axis fields: irrelevant for camera-facing particles.
- AgeDeath `spawn_on_death`: never set.
- Sub-part layers = body layers (1 exception, 5 refs).
- Filter bits: biped part (parts from bones), mopp_scaled (MOPP skipped), no_collision never set, group 0.
- `collision_response` copy in rigid-body info: unused duplicate.
- Phantom transform: duplicates the node's place; leaving it out is right.
- MOPP code, convex planes, packed `compressed`=0, bhkNiTriStripsShape: read or not needed.
- Motion system mapping 2/4/8 → dynamic, 6 keyframed, 7 fixed.

In progress elsewhere (not analysed): idle visibility tracks on placed rigs; NiLightColorController
for placed lights; PR #621 moving emitters (dust devils, 47 refs).

## 10. Ranked work queue

Ranked by what shows in play: placed references (or clips, weapons, actors) affected, look
before behaviour, then by cost. Each line is one fix. The workstreams own disjoint files, so
they can run in parallel; within a workstream, top to bottom. "Check first" = a value or rule
that is a [hypothesis] and wants a game frame or capture before the engine changes.

| # | Fix | Impact | Workstream |
|---|---|---|---|
| 1 | Environment maps: read texture-set slots 4 (cube) and 5 (mask), `environment_map_scale`, shader flags 7 / 17 / 21 (and flags-2 bits 13 / 15, env light fade); draw cube reflection × mask × scale | 1,663 files / 19,243 refs + window 490 / 2,639 (bottles, glass, metal) | M |
| 2 | Placed `NiBillboardNode`s turn to the camera (all modes; tell rigid-face-camera from face-centre); shapes below billboards become node-parented and reuse `face_camera` | 229 files / 1,000 refs (smoke wisps 180, klaxon 174, sconce flames 76, torches 26) | S |
| 3 | `BlendIn:` / `BlendOut:` (and `blendin:`, `blenin:`) read by `ClipKeys`, not only `IdleKeys` | 642 + 52 `.kf`: attack, loop and placing clips of every NPC and the player | K |
| 4 | Body-vs-body collision groups from the Havok layer (non-collidable plants, debris, clutter, ragdolls) | non-collidable bodies on 1,564 + 685 refs (barrel cactus 191, maize 233); every thrown item and ragdoll | H |
| 5 | Depth write from shader flags 2 bit 0, not from the alpha mode | off on ~810 files / ~7,300 refs (nettle 585, water stain 370, NoLighting glows) | M |
| 6 | Shapes below `BSDamageStage` (hidden in the file) shown at their stage through `damage_stages_of`, as add-ons already are | 49 files / 184 refs (cars 20 each, electric boxes 21); every destructible's damaged look | S |
| 7 | Parallax from flags 11 / 28 and slot 3 (`parallax_max_passes`, `parallax_scale`) | 262 files / 4,430 refs (rubble piles 367) | M |
| 8 | Texture clamp: `texture_clamp_mode` and the TexDesc clamp bits, per axis | mode 0 on 2,795 files / 2,325 refs (sea plants 682); NoLighting 47 refs | M |
| 9 | Gravity turbulence (`turbulence`, `turbulence_scale`) | 64 files / 126 refs (Gomorrah sconces 76, Primm dark smoke 13, HELIOS steam 10) | P |
| 10 | Stored NiTexturingProperty texture transform as the base UV transform (check first: how many are not the identity) | 467 files / 1,724 refs (smoke wisps 180) | M |
| 11 | Free `NiVisController` on placed objects kept in `Clip::free` (check first: what the ~216 refs outside skeletons and mines are) | 130 files / 2,177 refs | S |
| 12 | Emitter controller `phase` and `start_time` (period = stop − start) | 165 files / 1,053 refs run in step from 0; 3 refs with the wrong period | P |
| 13 | Sphere emitters emit inside a sphere, not on a flat disc | 48 files / 85 refs (explosion spheres of radius 12-192) | P |
| 14 | Morph tracks in `.kf` clips (the first-person rubber band) and in non-Idle groups (`tripwire01.nif` snap) | 288 + 22 `.kf`; 18 files / 69 refs | K |
| 15 | `BSRotAccumTransfInterpolator` read as an NiTransformInterpolator with rotation added up per loop (saw blade); `BSTreadTransfInterpolator` after a spec note (Ripper chain) | 21 + 5 `.kf`, both views | K |
| 16 | Gas-trap phantoms (layer 24) kept as volumes, then the trap's ignition (spec first) | 2 files / 124 refs | H |
| 17 | `BSOrderedNode`: children drawn in stored order | 78 files / 182 refs (klaxon 174) | S |
| 18 | TBC quaternion keys as a spline, not slerp | 137 files / 83 refs (vault doors 24, bighorner clips) | K |
| 19 | Alpha 4621 / 4609 keep their test; 4641 (ONE/ZERO + test) cuts instead of blending; the DLC05 factor pairs | 132 files / 819 refs (dust motes 161); impact blood | M |
| 20 | `no_fade` (shader flags 2 bit 3): never faded by distance (check first) | 72 files / 2,231 refs (decals) | S |
| 21 | Spawn on collide: drips burst on the floor (planar collider + spawn modifier) | 9 files / 24 refs (sewage and blood drips) | P |
| 22 | Specular colour: NiMaterialProperty `specular_color` and NiMaterialColorController target 2 tracks | 15 files / 472 refs (pinto, prickly pear); 18 `.kf` | M |
| 23 | Planar gravity turned with the reference unless `world_aligned`; `world_space = 0` systems move with their node | 180 files / 256 refs (only tilted refs differ); 29 files / 24 refs (snow globes) | P |
| 24 | `Sound:` keys heard while a placed object plays its own Idle | not counted | K |
| 25 | Havok material per child shape of list/transform shapes; stairs materials given to the character controller (check first, physics.md §6.9.1) | multi-material bodies; ~450 refs of stairs | H |
| 26 | Placed clutter damping from its rigid body | every dynamic clutter body | H |
| 27 | XYZ rotation with all three axis groups empty falls back to the stored rotation | robustness | K |
| 28 | `ProjectileNode_HeadLaser` in `MUZZLE_NODES` | 4 files: eyebot, Mr Gutsy, robobrain, protectron head lasers | S |
| 29 | Small particle fields: BSWindModifier, NiPSysResetOnLoopCtlr, gravity decay, drag range, grow/fade generations, NiPSysInitialRot*Ctlr, BSParentVelocityModifier | 0-5 refs each | P |
| 30 | Smaller Havok: prismatic joint, breakable threshold, motors, `bhkOrientHingedBodyAction` | protectron 26, turrets 24 (ragdolls) | H |
| 31 | Lights as blocks: NiAmbientLight, attenuation, `AttachLight` position (after the NiLightColorController session lands) | 38 + 8 files / 92 + 126 refs | L |
| 32 | App-init vs app-time controller clocks (check first) | `nvseaplant01.nif` 682 refs (flags 73) | S |
| — | Waiting on other features: HeadTrack float tracks (head tracking), BSAnimNotes (IK), `Decal:` keys + DVPG (melee blood), `m:` / `Hold` / `LoopStart` / `PM` keys, `BSFrustumFOVController` (VATS), bone LOD, `bhkBlendController` | — | — |

**Workstreams** (the files each owns; none shares a file with another):

- **M, materials and shaders**: `engine/nvf-assets/src/nif_spawn.rs` (`apply_property`,
  `alpha_mode`, `specular`), `nif_material.rs`, `texture.rs`, `material_anim.rs`, and the
  `MaterialTarget` enum of `anim_play.rs` (#1, 5, 7, 8, 10, 19, 22; then stored tangents and
  no-A2C from §2.1).
- **S, placed-model scene graph**: `engine/nvf-assets/src/model.rs` (prepare loop, `Clip::free`),
  `engine/nvforever/src/world.rs`, `shot_fx.rs` (`face_camera`), `player_body.rs`
  `MUZZLE_NODES` (#2, 6, 11, 17, 20, 28, 32). Starts after the "Idle visibility tracks" session
  merges (it edits `model.rs`).
- **K, clip playback**: `crates/fnv-nif/src/anim.rs`, `crates/fnv-actor/src/player_clips.rs`,
  `engine/nvforever/src/anim_blend.rs`, `object_anims.rs`, and the `Clip` struct of
  `anim_play.rs` (not its `MaterialTarget`) (#3, 14, 15, 18, 24, 27). #3 is a one-line fix and
  goes first.
- **P, particles**: `engine/nvf-assets/src/particles.rs`, `engine/nvforever/src/particles.rs`
  (#9, 12, 13, 21, 23, 29). Starts after PR #621 merges (same two files).
- **H, physics**: `crates/fnv-phys`, `crates/fnv-collide`, `nvf-play` `phys/prepare.rs` and the
  traps code (#4, 16, 25, 26, 30).
- **L, model lights**: `model_light.rs`, `lights.rs` (#31), after the NiLightColorController
  session.

## 11. Corrections to other notes

- `destruction-stages.md` §4 says shapes below damage-stage nodes "are still drawn at every
  stage". They are never drawn: 48 of 49 `BSDamageStage` nodes (and 47 of 52 `BSBlastNode`s)
  carry the hidden bit (flags 524303), and `model.rs:1226-1249` drops a hidden node's shapes for
  good when no visibility track shows it [observed: survey and code].
- `nif-node-audit.md` §1 row 1: `SightingNode` without `##` is now used
  (`engine/nvforever/src/iron_sights.rs:37`) [observed].
- `particles.md` §5 / §7.6 "spawn on death": no base-game file sets `spawn_on_death` [observed],
  so it can come off the open list. To add there: sphere emitters, emitter phase and start time,
  `world_aligned`, spawn-on-collide counts (§4).
- `collision.md` §4 and `crates/fnv-collide/src/nif.rs` near line 118 (phantom transform left
  out): right, but the reason is that the phantom's translation repeats its node's own place
  divided by 7 (tripwire `Trigger` z −10.5 → −1.5; mailbox bomb `Box01` z 85.05 → 12.15)
  [observed], so applying it would add the offset twice.
- `effects-animation-audit.md` §1 "NiMaterialColorController colour 2, specular: 6 files": that
  counts free ones only; with sequences it is 15 `.nif` (472 refs) and 18 `.kf` [observed].
