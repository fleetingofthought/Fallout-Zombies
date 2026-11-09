# JohnnyGuitar NVSE, batch 4 (2026-11-01, #1007)

Idle markers' animations and traits, regions' weather lists, race flags and faction
members, in `crates/fnv-game/src/nvse_jg4.rs`. Sources: the GECK wiki export
[documented: the page named]; the record layouts from `schemas/fnv` (xEdit
wbDefinitionsFNV.pas). Probed in the game as set jg4 (`tools/capture/jip_probes.py`, lab
scenario `L5.ext.jg4`; JohnnyGuitar with xNVSE 6.4.9, 2026-11-08); "jg4 row" below names
that set's rows. Setters change the record for the session, read by the getters. Tests:
`crates/fnv-game/tests/jg4.rs`.

| Function | NVForever |
|---|---|
| GetIdleMarkerAnimations `marker` | the marker's idles (IDLA) as an array, in record order [documented; observed: jg4 idlm.anims.tops, .hunter]; an empty array for a marker without them or another form [observed: jg4 idlm.anims.nodata, .ringo] |
| GetIdleMarkerTraitNumeric `marker trait` | 1 the flags (1 run in sequence, 4 do once; IDLF), 2 the idle timer (IDLT), 3 the number of animations; 0 for another trait [documented: the page's table; observed: jg4 idlm.trait.*] |
| SetIdleMarkerTraitNumeric `marker trait value` | 1 the flags, kept as a whole byte and read back as set (2 reads 2) [observed: jg4 idlm.settrait.hunter.1.2]; the marker plays by bits 0 and 2. 2 the timer, a negative one kept [observed: jg4 idlm.settrait.hunter.2.neg]; one not finite as a float is refused [hypothesis]. The count is not set here [documented: the page and its note; observed: jg4 idlm.settrait.hunter.3.1] |
| SetIdleMarkerAnimation `marker index idle` | the entry at the index replaced [documented], by any form, an idle or not [observed: jg4 idlm.setanim.hunter.notidle listed a race]; an index past the list changes nothing [hypothesis: the rows that would show it, idlm.setanim.hunter.9 and .neg, did not compile in the game, see below] |
| SetIdleMarkerAnimations `marker array` | the idles become the array's, in its order, and the count follows [documented: the note]; entries that are not IDLE forms are left out [hypothesis]. Not seen in the game: both rows did not compile there (`let iA := aSave` then `SetIdleMarkerAnimations rObj iA`) [observed: jg4 idlm.setanims.tops.*] |
| GetRegionWeathers `region` | the weathers of the region's weather block (RDWT after an RDAT of type 3) as an array [documented; the block: xEdit wbRegionDataEntries], in the game's order: by chance, highest first, weathers of the same chance in the opposite of the record's order [observed: jg4 regn.weathers.nv (100, 100) and .megaton (0, 0) reversed, .urban (85, 15) in record order; hypothesis: the rule that joins them] |
| GetNthRegionWeatherType, …Chance, …Global `region index` | index 0 reads none and 0, whatever the list [observed: jg4 regn.nth.nv.0, .megaton.0, regn.add.gs.dup]; 1 on the entry at that place in GetRegionWeathers' order [observed: jg4 regn.nth.nv.1, .megaton.1, .urban.1]; none past the list [observed: jg4 regn.nth.nv.2, .nv.-1]. GetNthRegionWeatherGlobal on an entry without a global crashed the original game, on NVUrbanRegion 1 and on GSWeatherRegion 1 after AddRegionWeather [observed: jg4, 2026-11-08; not probed since]; NVForever reads none. WeatherMegaton's entry 1 read no global where the record has MegatonNormalWeatherChance [observed: jg4 regn.nth.megaton.1.global]; not followed (NVForever reads the record's) |
| IsWeatherInRegion `region weather` | 1 when the block lists the weather [documented; observed: jg4 regn.in.*] |
| AddRegionWeather `region weather chance [global]` | the weather listed first, with its chance and global [documented; observed: jg4 regn.add.gs, the place]; a weather already listed is not listed again and keeps its entry [observed: jg4 regn.add.gs.dup]; a region without a weather block gains none, the call changes nothing [observed: jg4 regn.add.dc01]. The chance kept to 0-100 [hypothesis: index 0 reads 0, so the new entry's chance was not seen] |
| RemoveRegionWeather `region weather`, ClearRegionWeathers `region` | the weather's entry removed [documented; observed: jg4 regn.add.gs undone]; ClearRegionWeathers removes every entry but the first [observed: jg4 regn.clear.megaton kept its first weather] |
| GetRegionWeatherOverride, SetRegionWeatherOverride `region [flag]` | the weather block's RDAT flags bit 0 [documented: "an override flag"; the bit: `fnv_world::sky::REGION_WEATHER_OVERRIDE`]; any non-zero flag sets it [observed: jg4 regn.setoverride.gs.2] |
| GetRegionWeatherPriority, SetRegionWeatherPriority `region [priority]` | the weather block's RDAT priority, a byte; a priority outside 0-255 changes nothing [observed: jg4 regn.setpriority.gs.300, .neg]. A fraction (20.6) as the argument does not compile in the game [observed: jg4 regn.setpriority.gs.frac]; NVForever's compiler takes it (not followed, as for SetCasinoWinnings in ext-jg3.md) |
| SetRaceFlag `race flag on` | 0 playable, 2 child: the RACE DATA flag set or cleared [documented: the page's table]; the other bits are set the same way [observed: jg4 race.set.testqa.1.1, .3.1], 0-31 [hypothesis]; any non-zero `on` sets [observed: jg4 race.set.testqa.0.5]. GetRaceFlag (`nvse_plugins`) reads each bit as its value, not 1 (child 2 reads 4) [observed: jg4 race.get.child.2] |
| GetFactionMembers `faction [rank]` | the base actors whose factions hold the faction, at that rank when one is given [documented]; the factions are an actor's template's when the template supplies them [observed: jg4 fact.members.mercfac, the 16 mercenaries that take NVMercenaryMale's]; a rank of -1 is no rank [observed: jg4 fact.members.militia.-1; hypothesis: any negative]. In the game's order: the NPC_ records, then the CREA records, each in the plugins' record order [observed: jg4 fact.members.militia; hypothesis: across plugins] |

The engine's idles play a marker's changed collection (`nvse_jg4::idle_marker`, read by
`engine/nvforever/src/idles.rs`); an entry that is not an idle is skipped there
(`Game::idle_allowed` refuses a form that is not an IDLE); what the original does with
one is not known [hypothesis: skipped].

Follow-ups: the sky (`engine/nvforever/src/sky.rs`) loads the regions' weather lists once
from the plugins when it is set up, so a script's region weather changes reach the
getters only, not the weather drawn; it needs a reload when `OtherState::region_weathers`
changes; the light a region's weathers give (`fnv-world/src/lighting.rs`, `emittance`)
reads RDWT from the plugins too. The sandbox (`fnv-ai` sandbox.rs) still picks idle markers
and the stay at one from the record, so a marker a script emptied still draws actors, who
then fall back to their package's idles. The race flags reach GetRaceFlag only. Fractions
given to an int argument: both such rows (jg3 casino.setwin.tops.frac, jg4
regn.setpriority.gs.frac) did not compile in the game; a compiler rule for it needs more
rows than these two.

Not in this batch: GetLightingTemplateCell and SetLightingTemplateCell (the page names a
cell the template copies its values from; the LGTM record has only DATA in the schema, so
where that cell is kept is not known), GetRegionMapName and SetRegionMapName (RDMP is not decoded yet).
