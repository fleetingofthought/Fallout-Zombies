# SUP NVSE batch 2: the dialogue setters (2026-11-07, #1008)

SUP's topic and line setters, with the JohnnyGuitar and ShowOff functions that change the
same records and ITR's SetDialogueInfoFlags, which now goes the same way: 16 names built
(SUP 8, JohnnyGuitar 6, ShowOff 2). Code: `crates/fnv-game/src/dialogue_edits.rs` (the
edits and how the dialogue reads them) and `crates/fnv-game/src/nvse_dialogue.rs` (the
functions). Source: the GECK wiki export [documented: the page named]; where a page leaves
a value open the choice is tagged `[hypothesis]`. The probe set `sup2`
(`tools/capture/jip_probes.py`, lab `L5.ext.sup2`) checks them in the original, where SUP,
JohnnyGuitar, ShowOff and ITR are all installed (§4).

## 1. The edits and the dialogue

The pages say the changes are "not saved within a savegame and persistent throughout the
game session" [documented: TopicSetFlags, TopicSetPriority and the other SUP setters].
NVForever's records (`GameData`) are shared and never change, so a script's edit is a copy
of the topic (DIAL) or line (INFO) held beside them, `Game::dialogue_edits`, and the
dialogue reads both through `Game::dialogue_view()`: the copy where there is one, else the
record [design]. Everything that reads a line or a topic for the dialogue goes through it:
choosing a line (Say Once, Random and Random End, Say Once a Day, Goodbye, follow-ups), the
choices offered and their order, the topic list, Link From, the choice text, the responses
said and their timing, the radio's lines, idles and the companion wheel.

What follows from an edit is worked out again when it is made [design]:
- a topic's flags or priority: the top-level topic list, by the rule
  `GameData::top_level` uses (top-level topics of type Topic, highest priority first, then
  record order);
- a Link From added to a line: the topic is offered after the topic named
  (`GameData::linked_from`'s list for it, in record order);
- a line's flags and prompt: the choice list reads them too (Goodbye, Always Darken, Info
  Refusal, the prompt).

Follow Ups (DialogResponseAddRelatedTopic type 2) take nothing [observed: sup2], so the
records' own follow-ups are the only ones read.

The edits ride across a load and a new game with the session's other state
(`Game::take_session`, `Session::dialogue_edits`) [documented: "persistent throughout the
game session"]. Unsaved said-once values (§3) do not: after a load the save's said lines
are the game's [hypothesis: the value lives on the line in the original and only a baked
change goes into the save].

ITR's SetDialogueInfoFlags (ext-itr.md §2) wrote a map the dialogue never read; it is now
the same edit as TopicInfoSetFlagsHigh/Low, so the dialogue's Say Once, Random and Goodbye
rules follow it (the follow-up ext-itr.md listed). It now also rides across a load, as SUP's
setters do [hypothesis: the same record in memory in the original].

## 2. Topics and lines

| Function | NVForever |
|---|---|
| TopicSetFlags `topic [flags]` | DIAL DATA's flags, 1 Rumors and 2 Top Level [documented: the page's table]: with `flags` the low byte is written and the call returns 0, without it returns the flags [observed: sup2 topic.flags.set and …wide, 511 read back 255]. A Top Level change takes the topic into or out of the topic list |
| TopicSetPriority `topic [priority]` | DIAL PNAM, which orders the topic list and a line's choices [documented: TopicSetPriority; Category:Dialogue "Priority"]; returns the priority, 0 when setting [hypothesis: as TopicSetFlags; the game's compiler refused the probe's 75.5 for the int argument] |
| TopicInfoSetFlagsHigh `info [flags]` | DATA's first flag byte: 1 Goodbye, 2 Random, 4 Say Once, 8 Run Immediately, 16 Info Refusal, 32 Random End [documented: the table]; the whole byte is written, the other kept, and setting returns 0; without `flags` returns the byte [observed: sup2 info.high.set 5 read back 5 and GetDialogueInfoFlags 5; …wide 511 read back 255] |
| TopicInfoSetFlagsLow `info [flags]` | DATA's second flag byte: 1 Say Once a Day, 2 Always Darken [documented: the table]; as …High [observed: 1 set gave GetDialogueInfoFlags 260 on a Say Once line] |
| GetDialogueInfoFlags, SetDialogueInfoFlags (ITR) | both bytes as one value (ext-itr.md §2); the setter is the same edit |
| TopicInfoSetPrompt `info [prompt]` | the text shown for the line instead of its topic's name; returns the prompt, "" for none and when setting [documented; observed: sup2 prompt.set]. An empty prompt leaves the topic's name, as an empty RNAM does [hypothesis] |
| TopicInfoAddChoice `info topic position` | the topic into the line's choices (TCLT): -1 the end, 0 the beginning [documented]; a topic already there goes in again, past the end (99 in a list of 13) adds nothing [observed: sup2 choices.add.*]; the list's length itself is the end [hypothesis]. Deprecated for DialogResponseAddRelatedTopic [documented] |
| TopicInfoReplaceChoice `info topic position` | the choice at `position` (from 0) becomes the topic; past the end nothing [documented; observed: sup2 choices.replace, …replace.past] |
| DialogResponseAddRelatedTopic `info topic type position` (JohnnyGuitar) | into the line's Link From (0) or Choices (1) [documented: the three types; observed: sup2 related.add.linkfrom and …choices]; Follow Ups (2) take nothing, neither a line nor a topic [observed: …followinfo and …followtopic]; positions as TopicInfoAddChoice [hypothesis] |
| DialogResponseRelatedGetAll `info type` (JohnnyGuitar) | the line's Link From, Choices or Follow Ups in order [documented; observed for 0 and 1]; the game's compiler refuses the call without a type [observed: sup2 related.get.notype], NVForever reads 0 then [hypothesis] |
| GetParentTopic `info` (JohnnyGuitar) | the line's topic [documented] |
| GetTopicInfo `topic [quest]` (JohnnyGuitar) | the topic's lines, only the quest's when given [documented], as SUP's TopicGetAllTopicInfos (ext-sup1.md) |
| TopicInfoSetNthResponseString `info index text` (ShowOff) | the response at `index` (from 0, as TopicInfoGetResponseStrings' array) says `text`; 1, or 0 with no response there [documented] |
| TopicInfoSetResponseStrings `info array` (ShowOff) | returns 1 and changes nothing [observed: sup2 responses, `ar_List "First", "Second"` on a one-response line kept its text]. The page says each element replaces the next response's text [documented]; the installed ShowOff did not, so NVForever follows the game (§5) |

The getters of batch 1 (TopicInfoGetAllChoices, …GetChoiceCount, …HasChoice) and ShowOff's
TopicInfoGetResponseStrings read the edits too. A changed response text also changes the
response's length for its timing when the line has no voice file
(`InfoDef::response_chars`, scripts.md §7.7); with a voice file the file's length counts,
as before.

## 3. Said once

A Say Once line is held back once any actor has said it (`Game::said`, saved;
dialogue.md). `Game::said_once(info)` is that, unless a script set a value for the session.

| Function | NVForever |
|---|---|
| GetSaidOnce `info` (JohnnyGuitar) | whether the line has been said [documented] |
| SetSaidOnce `info value [saveBake]` (JohnnyGuitar) | with `saveBake` the value goes into the saved said lines (set: under no actor, `FormId(0)`; cleared: every actor's), else it holds for the session; returns 0 [documented: the arguments; observed: sup2 said.jg, set and cleared read back by GetSaidOnce; hypothesis: how a bake is kept] |
| TopicInfoSetSaidOnce `info [set]` (SUP) | the session value, not saved [documented]; 0 when setting, the value without `set` [observed: sup2 said.sup] |

Saying the line drops a session value: the line is said now [hypothesis: saying it sets
the flag in the original].

## 4. Probes and the lab

Set `sup2` (40 rows, a writing set): the pages' own examples, Sunny Smiles's first
greeting (INFO 104E7E: Say Once, ten choices) and goodbye (15CB5B: Goodbye) and the topic
1055D2, with GREETING and GOODBYE [observed: fnvtool esm decode]. Flags and priorities are
put back; the choice and related lists only grow, so those rows run in order. Results:
see §5.

## 5. Results (2026-11-07)

Captured in the original from the save nvftest (golden `jip/sup2.json`, private; 38 rows,
2 refused by the game's compiler). Lab `L5.ext.sup2` before the fixes: 22 equal, 16
changed. What the game showed, built since:
- every setter returns 0 (or "") when it sets and the value only as a getter;
- the flag setters write the low byte of their value;
- TopicInfoAddChoice adds a topic already there again, and nothing past the end;
- DialogResponseAddRelatedTopic's Follow Ups take nothing;
- TopicInfoSetResponseStrings returns 1 and changes nothing, against its page.

After the fixes: 37 equal, 1 changed, 2 extra:
- `sup2.responses`: the probe puts the text back with
  `TopicInfoSetNthResponseString rObj 0 sSave`, and in the game the text became "sSave",
  the variable's name. That is the game's runtime compiler passing a `string_var`'s name
  where a parameter takes literal text, seen before with SetAmmoName and
  SetWeaponWorldModelPath (ext-showoff.md §5) [observed]; NVForever passes the value.
  Everything before it in the row matches.
- The 2 extra rows (`sup2.topic.priority.set`, `sup2.related.get.notype`) are steps the
  game's compiler refused (75.5 for TopicSetPriority's int, and DialogResponseRelatedGetAll
  without its type); NVForever compiles them. Nothing to change.

Not settled by the probes: whether the setters' edits reach the game's own conversation
menu the way NVForever's dialogue reads them (no probe opens a conversation), and the
follow-up list DialogResponseRelatedGetAll 2 reads (Sunny Smiles's lines have none).

## 6. Not in this batch

SUP's other 114 names: the free camera (TFC) and the calculated positions, the INI, Excel,
JSON and file readers, the cursor, mouse and HUD functions, debug text, the NIF block
interpolation, and the web, clipboard, screenshot and window functions (ext-sup1.md lists
why those need a policy first). JohnnyGuitar's IsInDialogueWithPlayer and the activation
prompt overrides, and ShowOff's SayTo_GetTopic/…Info, are not dialogue record edits.
