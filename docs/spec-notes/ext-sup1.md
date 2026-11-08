# SUP NVSE, batch 1 (2026-11-01, #1008)

Rounding, points and angles, numbers in words, the Cyrillic test, the actor finders and
the topic getters, in `crates/fnv-game/src/nvse_sup1.rs`. Source: the GECK wiki export
[documented: the page named]. `supNVSE.dll` is not installed in the probe game on this
machine, so nothing here is `[observed]`; where a page leaves a value open the choice is
tagged `[hypothesis]`. Tests: the unit tests in `nvse_sup1.rs` (the pages' examples) and
`crates/fnv-game/tests/sup1.rs` (called from scripts on a synthetic plugin).

| Function | NVForever |
|---|---|
| Round `x` | to the nearest whole number [documented: 37.4 is 37, 37.8 is 38; hypothesis: a half away from zero] |
| RoundAlt `x places round` | to 0-4 decimals, rounded when `round` is set, else cut [documented: every example on the page] |
| IsNumberNAN `[x]` | 1 for a NaN; with no number it returns a NaN [documented: both forms] |
| GetDistanceBetweenPoints `x1 y1 z1 x2 y2 z2` | the straight distance [documented] |
| GetHeadingAngleBetweenPoints `x y angleZ tx ty`, GetHeadingAngleAlt `x y angleZ ref` | degrees from the facing to the point (or the reference's position), clockwise, (-180, 180], as GetHeadingAngle [documented: "equivalent to GetHeadingAngle"; the angle in degrees as GetAngle Z gives it] |
| NumberToWords `n` | English words without "and" or hyphens [documented: the three examples; hypothesis: "zero", and "minus" before a negative number] |
| StringContainsCyrillic `text` | 1 when a letter is in the Cyrillic block U+0400-U+04FF [documented; hypothesis: the block] |
| GetSUPVersion | -1 [documented: "will always return -1"] |
| IsPlayerOverencumbered | carrying more than the carry weight, as ShowOff's GetIsPlayerOverencumbered [documented: the page points there] |
| FindClosestActor, FindRandomActor `x y z [dead] [teammates] [distance]` | among the actors in the loaded cells, not the player: the closest to the point, or a random one; the dead only with `dead`, the player's teammates only with `teammates`, within `distance` when one above 0 is given; none (0) when there is none [documented: the pages and their notes on the loaded cells and the player] |
| FindClosestActorFromRef, FindRandomActorFromRef `[dead] [teammates] [distance]` | the same around the calling reference, which is not its own find [documented; hypothesis: not itself] |
| TopicGetType `topic` | the topic's type, 0 Topic to 7 Radio (DIAL DATA) [documented: the page's table] |
| TopicGetAllTopicInfos `topic [quest]` | the topic's lines in topic order, only the quest's when one is given [documented] |
| TopicInfoGetAllChoices, TopicInfoGetChoiceCount `info`, TopicInfoHasChoice `info topic` | the line's choices (TCLT) [documented] |

Not in this batch: the topic and line setters (TopicSetPriority, TopicSetFlags,
TopicInfoAddChoice, …ReplaceChoice, …SetFlagsHigh/Low, …SetPrompt, …SetSaidOnce): built in
batch 2 (ext-sup2.md), with the dialogue reading the edits.
GetCalculatedPos and GetCalculatedPosAlt write into script variables. KillAll2 is
deprecated on its page as not working. The file, INI, Excel, JSON, clipboard, screenshot,
web and window functions touch the machine outside the game and need a policy first
(NVForever never writes into the game folder or `Documents\My Games\FalloutNV`).
