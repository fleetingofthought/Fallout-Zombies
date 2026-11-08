# JIP LN NVSE, batch 7 (2026-11-01, #1006)

Ammunition, ammo effects, projectiles, explosions, impact data, weathers and climates, and
armour, in `crates/fnv-game/src/nvse_jip7.rs`. Sources: the GECK wiki export, each
function's page [documented: the page named]; the fields: xEdit's record definitions as
the schema keeps them (`schemas/fnv`). Not probed in the original (the extender goldens
are kept on the other machine); choices the pages leave open are tagged [hypothesis].
Setters change the form for the session and are read by this batch's getters and the
earlier ones (GetAmmoSpeed, GetAmmoConsumedPercent, GetAmmoCasing, GetNumAmmoEffects,
GetNthAmmoEffect, GetAmmoEffectTraitNumeric, GetClimateNumWeatherTypes,
GetClimateNthWeather…). The setters of a trait return 0 [hypothesis: as the record
setters of batch 5 were observed to, jip-records-5.md].

## 1. Ammunition and ammo effects

| Function | NVForever |
|---|---|
| SetAmmoTraitNumeric `ammo trait value` | 0 speed (DATA), 1 projectiles per shot (DAT2), 2 ammo consumed percent (DAT2), 3 clip rounds (DATA) [documented: the page's table, which lists 0-3 though its argument says 0-2] |
| SetAmmoProjectile, SetAmmoCasing `ammo [form]` | DAT2's projectile and consumed ammo; left out, none [documented] |
| AddAmmoEffect, RemoveAmmoEffect `ammo effect` | the RCIL list: 1 when it changed, 0 for an effect already there or not there [documented: `successful`; hypothesis: the 0 cases]; GetNumAmmoEffects, GetNthAmmoEffect, ApplyAmmoEffects and GetCalculatedWeaponDegradation read the changed list |
| SetAmmoEffectTraitNumeric `effect trait value` | AMEF DATA: 0 type, 1 operation, 2 value [documented] |

Combat builds each ammunition's effects once, when the data loads (`fnv_combat::data`,
`ammo`), so a list changed by these functions does not reach a shot's damage, DT or
spread (follow-up: combat's ammo effects through `Game::ammo_effects`).

## 2. Projectiles and explosions

| Function | NVForever |
|---|---|
| SetProjectileTraitNumeric `projectile trait value` | DATA: 0 type, 1 gravity, 2 speed, 3 range, 4 tracer chance, 5 explosion proximity, 6 explosion timer, 7 muzzle flash duration, 8 fade duration, 9 impact force, 10-12 rotation x/y/z, 13 bouncy mult; 14 the detection sound level (VNAM) [documented: the page's table] |
| SetProjectileLight `projectile [light]` | DATA's light; left out, none [documented] |
| SetProjectileMuzzleFlash `projectile path` | NAM1, the muzzle flash model |
| SetExplosionTraitNumeric `explosion trait value` | DATA: 0 force, 1 damage, 2 radius, 3 IS radius, 4 radiation level, 5 dissipation time, 6 radiation radius, 7 sound level [documented] |

The engine's projectiles and explosions still read the records as loaded (follow-up: the
game's projectile and explosion data through the session's changes).

## 3. Impact data and materials

| Function | NVForever |
|---|---|
| GetImpactDataTrait `ipct trait` | 0-4 the impact data (DATA: duration, orientation, angle threshold, placement radius, sound level), 5-7 the texture set (DNAM) and sounds 1 and 2 (SNAM, NAM1), 8-15 the decal data (DODT: widths, heights, depth, shininess, scale, passes), 16 the flags, 17 the colour as R × 1000000 + G × 1000 + B [documented: the page's table]; past 17, 0 |
| SetImpactDataTraitNumeric `ipct trait value` | the same without the forms: 0-4, 5-12 the decal data, 13 the flags, 14 the colour [documented] |
| SetImpactDataTraitForm `ipct trait form` | 0 texture set, 1 sound 1, 2 sound 2 [documented]; a form of the wrong type (a creature for a sound) sets none [observed: jip7 ipct.setform.sound1] |
| SetImpactDataSet `weapon dataset` | WEAP INAM |
| Get/SetImpactMaterialType `[material] [baseActor]` | NAM4 of the NPC or creature, 0-11 [documented: the table]; a number past 11 is not set |

The flags combine two records' bits: 1 No Decal Data is DATA's flag bit 0, 2 Parallax, 4
Alpha Blending and 8 Alpha Testing are the decal data's flag bits 0-2 [documented: the
table; the bits: xEdit IPCT and the decal data].

## 4. Weathers and climates

| Function | NVForever |
|---|---|
| Get/SetWeatherPrecipitationModel `weather [path]` | MODL |
| Get/SetWeatherTexture `weather layer [path]` | the cloud layers 0-3: DNAM, CNAM, ANAM, BNAM [documented: layers 0-3; the fields: xEdit WTHR] |
| SetWeatherTransitionTimeOverride `[seconds]` | kept (`Game::weather_transition_override`); none or 0 removes it [documented] |
| ClimateAddWeatherType `climate weather chance [global]` | adds to WLST, or sets the chance and global of a weather already there [documented]; 1 [documented: `successful`] |
| ClimateRemoveWeatherType `climate weather` | 1 when it was there [hypothesis: 0 otherwise] |

The sky still reads the climates' weathers and the weathers' textures as loaded, and its
transitions take their own time (follow-up: the sky through these changes).

## 5. Armour

| Function | NVForever |
|---|---|
| SetArmorClass `armor class` | 1 light, 2 medium, 3 heavy [documented]: BMDT's general flags Medium (bit 3) and Heavy (bit 7), read by `Game::armor_class` |
| SetArmorRCT `armor path` | BMCT, the ragdoll constraint template |
| SetArmorConditionPenalty `[multiplier]` | kept; returns the value before the call; left out, unchanged; 1.0 until set [documented; observed: jip7 armor.condpenalty]; any number is taken (3, -1) [observed: jip7b armor.condpenalty.neg] |
| SetDamageToArmorMaxPercent `[share]` | kept, 0-1 only (2 and -1 set nothing); returns the value before the call; left out, unchanged; 1 until set [observed: jip7 armor.maxpercent, jip7b armor.maxpercent.range] |

Combat still uses the fixed multiplier 1.0 and no cap (`fnv_combat::condition`), and the
ragdolls and movement read the records as loaded (follow-up: these through the changes).
SetDamageToArmorMaxPercent "is saved in the game" [documented]; NVForever keeps it for the
session only for now.

## 6. Values a field cannot hold

Every setter of this batch passes its number through one check
(`crate::nvse::field_number`): NaN and infinity set nothing; any other number is clamped
to the field's range: an f32 field's, a u8's (clip rounds, parallax passes), a u32's
(projectiles per shot), the documented ranges of an enumeration (effect type 0-5,
operation 0-2, projectile type 1-16, sound level 0-2, orientation 0-2, a climate chance
0-100, impact flags 0-15, the colour up to 255255255); and a whole number for every field
that is not an f32 [hypothesis: the game keeps its record fields in range]. Also:
SetWeatherTransitionTimeOverride's seconds (0 and up), SetArmorConditionPenalty's
multiplier (0-2: past 2 the page's formula `1.0 - (0.5 - condition) × multiplier` goes below
zero at 0% condition [hypothesis]) and SetDamageToArmorMaxPercent's share (0-1
[documented]). The later batches use the same check.

## Probed in the original (2026-11-08, #1006)

Sets jip7 (108 rows) and jip7b (46 rows) of `tools/capture/jip_probes.py`, run from the
`nvfairport` save on the 10mm round, AmmoEffectMagnumDTReduction, 10mmBulletProjectile,
GrenadeFragExplosion, BaseBallBatMetalImpact (no decal data) and RipperImpactOrganic,
NVWastelandClear, NVDefaultClimate and ArmorLeather; goldens private (lab L5.ext.jip7,
L5.ext.jip7b). What §6 left open is settled field by field [observed]:
- the setters of a trait return 0, as supposed;
- ammunition: projectiles per shot keep their sign, cut to whole (-5, -2.5 reads -2); clip
  rounds wrap as a byte (300 reads 44, 256 reads 0, -1 reads 255);
- ammo effects: a type past 0-5 and a negative value set nothing;
- projectiles: the type takes 0 or a single type bit (1, 2, 4, 8, 16; 3 and 17 set
  nothing), the sound level only 1 or 2 (0, 3, 5 and -1 set nothing);
- explosions: the sound level wraps as a byte with no range (9 reads 9, 300 reads 44);
- impact data: the flags are kept as set, to nine bits (99, 256; -1 reads 511); a colour
  past 0-255255255 sets nothing, a channel past 255 below it is kept (300 reads 300); an
  impact without decal data reads widths and heights 8 to 32, depth 32, shininess 4,
  parallax scale 1 and 4 passes.
Equal after the fixes: jip7 105 of 108, jip7b 44 of 46. Left: the colour of an impact
without decal data (BaseBallBatMetalImpact reads 255320535, which no RRRGGGBBB channel set
explains), and 16777215 set on a decal reading 16009218 (a channel past 255 seems to carry
into the next): three and two rows, no rule found.
