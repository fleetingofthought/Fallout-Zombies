# Spec note: the character-creation menus

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Rules: `crates/fnv-game/src/chargen.rs`; drawing:
`engine/nvf-play/src/chargen.rs` and `chargen/{draw,lists,name}.rs`; the Vigor Tester's
S.P.E.C.I.A.L. menu has its own note, `vigor-tester.md`; the race menu (1036) is NVForever's own
character creator, `character-creator.md`.

## 1. Sources and when the menus open

- **Eleven screenshots from the owner**, 2026-10-11, 1280 × 720 (kept privately): the
  trait menu twice (Four Eyes chosen, then Early Bird and Four Eyes taken), the tag-skill
  menu twice (Speech and Survival tagged with Lockpick chosen; then Guns tagged too), the
  name menu, and six of the Vigor Tester. The owner's words: up to two traits or none;
  three tag skills, +15 each, all three required; the name menu sets the character's
  name for the save; the Vigor Tester starts every attribute at 5 with 5 more to spend,
  1 to 10 each, all points spent before going on.
- **The menu files** in `Fallout - Misc.bsa` (described here in plain words; nothing of
  them is shipped): `menus\chargen\char_gen_menu.xml` (menu class CharGenMenu, the tag
  skills), `menus\trait_menu.xml` (TraitMenu), `menus\dialog\texteditmenu.xml`
  (TextEditMenu, the name), `menus\chargen\love_tester_menu.xml` (LoveTesterMenu), and
  the shared pieces they include (`list_box.xml`, `list_box_template.xml`,
  `scrollbar_vert.xml`, `text_box.xml`, `top_bracket.xml`, `bottom_bracket.xml`,
  `line.xml`, `vertical_fade_line.xml`, `menus\globals.xml`). `menus\trait_select_menu.xml`
  is a "Medical Questionnaire" on paper textures (`terminals\med_history_*.dds`) that the
  opening does not open [observed: no script opens its code].
- **Which function opens which menu** [observed, layer5-captures.md §10.2–§10.4]: stage 10
  of VCG01 calls `GetPlayerName` (menu 1051, the name), 36 `ShowRaceMenu` (1036), the
  Vigor Tester's OnActivate `ShowLoveTesterMenuParams` (1074), 90 `SetTagSkills` (1048,
  after the psych test has tagged three skills), 102 `ShowTraitMenu` (1084). Each stage's
  timer goes on once the menu closes.
- **Settings the masters hold** [observed: FalloutNV.esm GMST]: `sTraitMenuTitleText`
  "CHOOSE UP TO %d TRAITS (OR NONE)", `iTraitMenuMaxNumTraits` 2, `sDefaultPlayerName`
  "Courier", `fAVDTagSkillBonus` 15. The other words the menu files name (`sReset`,
  `sDone`, `sOk`, `sEnterName`, `sPCMenuHintR`, `sPCMenuHintA`) are not in the masters:
  they are the program's own defaults, which the encrypted program does not let us read,
  so the words drawn are those on the screenshots ("Reset", "Done", "Ok", "Enter character
  name.", "R)", "A)") [observed].

## 2. The data the menus show

- **Traits**: every PERK whose DATA says Trait, Playable and not Hidden [documented: xEdit
  `wbDefinitionsFNV.pas`, PERK DATA: Trait, Min Level, Ranks, Playable, Hidden, a byte
  each]. Each of the sixteen traits stores `01 01 01 01 00` [observed]: ten in
  FalloutNV.esm (Built to Destroy, Fast Shot, Four Eyes, Good Natured, Heavy Handed,
  Kamikaze, Loose Cannon, Small Frame, Trigger Discipline, Wild Wasteland) and six in
  OldWorldBlues.esm (Claustrophobia, Early Bird, Hoarder, Hot Blooded, Logan's Loophole,
  Skilled). The menu lists them by name [observed: the screenshots' alphabetical order,
  the Old World Blues ones among the others]. A trait's requirements are the CTDA before
  its first effect (PRKE); the CTDA and DATA after it belong to the effects [documented:
  xEdit record order]. Its picture is its ICON.
- **The "Req:" line** names each requirement on GetActorValue or GetPermanentActorValue
  by the actor value's short name (AVIF ANAM, "PER") with the comparison and number
  [observed: Four Eyes' "Req: PER < 10"; Early Bird's "Req: --" for none]. Four Eyes holds
  two requirements, Perception > 1 and < 10, and only the second is drawn; the menu here
  leaves out a S.P.E.C.I.A.L. "greater than 1" [hypothesis: why the original leaves it
  out is not known]. Small Frame's line comes out "AGL < 10".
- **Skills**: the tag-skill menu lists every skill but Big Guns, by name [observed: the
  thirteen on the screenshots in alphabetical order; Big Guns' AVIF is named "Big Guns -
  OBSOLETE" in the masters]. A skill's name, description and picture are its AVIF's FULL,
  DESC and ICON. The masters' AVIF FormIDs run in actor value code order, 0x3E8–0x3EE for
  Strength to Luck and 0x4B0–0x4BD for Barter to Unarmed [observed: each record's name is
  its code's; Guns is Editor ID AVSmallGuns and Survival AVThrowing].
- **Skill values**: the player's skill with the tag bonus as the menu has it (skill
  formula, actor-values.md; + `fAVDTagSkillBonus`). With the screenshots' attributes
  (5, 3, 7, 7, 5, 6, 7) and Speech, Survival and Guns tagged, every value matches
  (Barter 20, Energy Weapons 12, Explosives 12, Guns 33, Lockpick 12, Medicine 16, Melee
  Weapons 16, Repair 16, Science 16, Sneak 18, Speech 35, Survival 35, Unarmed 20)
  [observed: `fnvtool actor chargen`, §6].

## 3. The tag-skill menu (1048)

[documented: char_gen_menu.xml unless tagged] Lengths in the interface's units, 960 to
the view's height, from the top left of a 1024 × 800 rectangle centred on the screen.
- A dark picture behind (`interface\shared\background\solid_black.dds`), 1144 × 1070, 50
  left of and 125 above the rectangle.
- The headline: a line 50 long with a line fading down at its left end, the title (font
  6) at 70, 10 above the line, then the line again from 20 after the title to the right
  side, fading down there too. The title reads "SKILLS:  2/3 Selected" [observed].
- The list: 15 left of the rectangle, 450 wide, 14 rows; its top is "set in code", 66
  down [observed: fitted to the highlight box on the screenshot]. A row: the name at 40,
  20 down, wrapping at 390; the value centred at 360; a filled square at 15, 17.5 (25
  square) for a tagged skill; the chosen row in the highlight box (an outline at
  brightness 200, a fill of alpha 40, from the scroll bar's right side to 5 short of the
  list's). Rows are 52 apart [observed: 468 pixels over 12 rows at 720 lines].
- The chosen skill's picture: 400 square at 500, 20.
- The info rectangle at 445, 420, 580 wide: lines 560 long at its top and 300 down; the
  skill's description at 20, 20 wrapping at 540; the counter 20 under the second line
  ("Tag 1 Skill" at 2/3, nothing at 3/3 [observed]; "Tag 2 Skills" and "Tag 3 Skills"
  [hypothesis]).
- Reset and Done right-aligned at 1014, 60 down, Done 52 under Reset; each a text box (its
  text and 10 on every side) with its key after it ("R)", "A)"). Done can be pressed only
  when the points counted equal the most (`_CurrPoints` = `_MaxPoints`): three tagged;
  otherwise it is drawn at 127 of 255 [observed: dimmed at 2/3, bright at 3/3].
- The bottom bracket at 800: a line the rectangle's width with a line fading up at each
  end.
- **Opening state**: the skills the player has tagged (the psych test's three)
  [observed: the psych test tags three before stage 90]. Clicking a tagged skill untags
  it; an untagged one is tagged while fewer than three are [observed: the owner's
  screenshots, 2/3 then 3/3]. Reset clears every tag, those it opened with too: opened
  at 3/3 with the save's three tags (`SetTagSkills 3`), it reads 0/3, the skills' figures
  drop back and Done dims [observed 2026-11-06: private job ov_tags, 1280x720]. Tags an
  AddTagSkills opening locks stay [hypothesis: not captured; they may not be un-tagged].

- **The attributes page**: the same file's attribute rows, which AddSPECIALPoints opens
  (Intense Training): levelup.md §8.

### 3.1 The tag-skill list drawn from its file (`[UI] TileMenus=CharGenMenu`)

[design: `engine/nvf-play/src/tiles/{tag_menu,selection_menu}.rs`] Off by default.
The open `ListMenu` owns the sheet and all its rules. This view reads it, writes tiles,
and queues acts for `chargen::input`; it never constructs a sheet or consumes a script's
pending request. The hand-built drawing remains with the switch off or a failed readiness.
Sources: the file's tile-name contract [documented: `menus\chargen\char_gen_menu.xml`, read
privately], §3's geometry and screenshots, the trait contract in §4.1, and the private job
`ov_tags` [observed 2026-11-06: the original at 1280 x 720, the hudtest save, `SetTagSkills 3`,
read with JIP]. Assumptions below are not observations.

- **What the original was seen to do** [observed: `ov_tags`]. The list reads `y` 65, `x` -15,
  450 x 728, with 13 rows, each 52 high with `_y` the sum above it; the tree's order is not the
  list's, `listindex` is. `_highlight_y` is -1 with `_selected_height` 0 at the opening, where
  row 0's picture and text show, and the row's `_y` with 52 under the pointer; the highlight
  and text keep the last row when it leaves. A tagged row reads `_selected` 1 and shows its
  marker. `CGM_SelectionText` is at 20, 20, wrapping at 540; the counter is at 20, 317.33, 32
  high. At 3/3 the counter is empty and hidden and Done's `target` is 1; at 0/3 it is shown and
  Done's `target` is 0. **Reset took 3/3 to 0/3** with the save's three tags on the list, as §3
  says. `ClickMenuTile` on `*:k` moved nothing in this job, so taking and giving back by the
  pointer was not seen. This view's Reset is the open sheet's own (`ListMenu::reset`), which
  clears every tag except the locked ones, as §3 does; the view does not own the sheet.

- **What the program writes** [design: `tag_menu::snapshot`, `selection_menu::write`].
  `_CurrPoints` is the sheet's number tagged, `_MaxPoints` its `count`, and `_OnlyAdd` says
  whether it has locked tags [hypothesis: follows the sheet's AddTagSkills semantics;
  the file's runtime meaning awaits the capture]. The title, description and counter get
  `string`; the icon gets `filename` and `visible`; the list gets `visible` and its code-set
  `y` of 65 and `height` of 728, which the file leaves to code [observed: `ov_tags`]. Rows come from `CGM_SelectItemTemplate`, row `CGM_SelectItem`, with
  `ListItemText`, `CGM_Template_ItemValue`, and `CGM_Template_ItemMarker`. They get `string`,
  the value child's `string`, `_selected`, `_TextAlpha`, `id`, `listindex`, `_y`, and measured
  `height`. The list's highlight and its `lb_scrollbar` counts and value follow §4.1. Done's
  `target` follows the sheet's `complete()`, so an unfinished sheet cannot finish.
- **Every string this view writes** [design: `tag_menu.rs`, LEGAL.md,
  `crates/fnv-ui/src/strings.rs`]. The title is our own `Choose skills: {taken}/{count}`;
  the counter our own `{remaining} left to choose`, and empty with none left. Neither has a named
  setting in the strings table; this path does not reuse the hand-built title or counter.
  Row names are AVIF FULL through `ListView`; row values are decimal `TagSheet::value`
  (including this sheet's tag bonus); the description is AVIF DESC without an added label.
  The picture filename is AVIF ICON, or empty when absent. The file retains the button
  words and hints: the view writes none of them. Their `sReset` and `sDone` resolve through
  `fnv_ui::strings` (load-order GMST first, otherwise the project's built-in Reset and Done).
- **Uncaptured numbers and behavior** [hypothesis: follows `trait_menu.rs` and §4.1].
  Rows use `id` 10 and normal alpha 255 (128 for a faint row); the backing alpha is 193.8.
  `_selected` means tagged, independently of the highlight. Opening shows the first row's
  description with no highlight. Leaving the rows removes the highlight and keeps the text;
  a key move reveals its row, while a pointer choice or scrollbar move does not move the
  other. The list's counts, row measurement and pointer/key arbitration follow the shared
  trait machinery. These are candidates for comparison with the 2026-11-06 capture.
- **Counts and other uses of 1048** [design: `fnv_game::chargen::TagSheet`, `lists.rs::open`].
  Nothing assumes three: SetTagSkills supplies its total, and AddTagSkills supplies the
  old tags plus its request. Locked old tags remain locked, extra choices obey that total,
  and Reset clears every tag except the locked ones (§3). The same applies when a perk opens this sheet.
  `Sheet::Special`, including AddSPECIALPoints from Intense Training and the revision's
  SetSPECIALPoints, is excluded before readiness: its attribute template and hand-built
  drawing stay in place. No candidate tag rows, warning or tag blur run for that page.
- **Long descriptions** [documented: the file's name contract and §3]. The file has plain
  `CGM_SelectionText`, without a text window or scrollbar. Its wrap width is 540 interface
  units, its top 20 below the first divider, with the second divider 300 below it. At
  1280 x 720 these are 405, 15 and 225 pixels. The body font's line height 31 and wrapped
  pitch 35 (`chargen/draw.rs`, fonts.md) allow about eight lines before the lower divider;
  nine lines need about 311 units [hypothesis: this calculation, not a runtime measure of
  any particular AVIF or mod's description]. There is no guarantee every description fits.
  This path therefore measures the text and makes the info rectangle a clip ending at the
  lower divider, with its counter and decorations outside that clip [design:
  `tag_menu::settle_text`, `fnv_ui::input::OUTSIDE_CLIP`]. The wheel over it and Left/Right
  scroll by 20 units per step [hypothesis: the trait step], clamped exactly to the measured
  end; another skill starts at the top. No visible scrollbar is added: the file provides
  none. The initial text top belongs to the tree, surviving a fallback, so repeated writes
  never treat a scrolled position as its origin. The artificial long-text aid repeats the
  description eight times on this path only [design].
- **Readiness, lifetimes and hand-over** [design: `selection_menu.rs`, §4.1]. Every frame
  checks the paths, template children, all live row children, usable dimensions and measured
  text. Failure removes candidate rows and returns the unchanged open sheet to hand-built
  drawing, with one warning per reason; the next frame retries. An opening keeps its
  scrolls and highlight through fallback; the tree keeps its original counts and text top;
  readiness owns the rows. A new opening resets scrolls and highlight. A frame evaluates,
  writes changed state, measures, places rows and scrolls, and evaluates again. Input rewrites
  the scroll values after the file's click pulses. The first tiled frame removes hand-built
  rows; tile input starts after they are gone. Drawing, hand-built suppression and blur
  share the same opening/menu/tree decision, including a stage replaced by a script.
- **Checks** [design]. Synthetic unit fixtures contain no game expressions. The 22 trait
  tests remain; tag tests cover variable totals, locked tags, Reset, SpecialSheet exclusion,
  live-child readiness, a readable text end, fallback lifetimes and stale clicks. Smoke
  `tiles-tag-*` includes the six basic scenarios, AddTagSkills, attributes fallback, a long
  description and fallback while open; every new scenario requests 1280 x 720 shots.

## 4. The trait menu (1084)

[documented: trait_menu.xml unless tagged]
- The same dark picture, headline (title at 70: `sTraitMenuTitleText` with the most
  traits) and bottom bracket as §3.
- The list at -15, 60, 470 × 676 (13 rows of 52); a row's name at 45, 20 down, wrapping
  at 360; a filled square for a taken trait. With sixteen traits the scroll bar shows at
  the list's left: arrows at its ends, the marker 105 long with its top 23 under the up
  arrow at the list's top [observed: pixels 139–208 on the screenshot; hypothesis: it
  keeps that length and moves evenly].
- The picture: 400 square at 520, 0.
- The info rectangle 10 right of the list (465), 385 down, to the rectangle's right
  side: a line 3 short of its width at its top and at 335, each with a line fading down
  60 from its right end; the text 20 down in a clipped box 275 high, wrapping short of
  the box's scroll bar, whose arrows stand at its right, 95 down in the box [observed:
  432 and 515 pixels on the screenshot, shown with a short text]; the counter (font 6)
  20 under the second line.
- The text [observed]: "Req: …", "Ranks: 1", an empty line, the description.
- The counter [observed]: "2 TRAITS LEFT", "0 TRAITS LEFT"; "1 TRAIT LEFT" [hypothesis].
- Reset and Done as §3, Done right under Reset (51); Done can always be pressed [observed:
  bright with nothing taken].
- Clicking a taken trait gives it back; an untaken one is taken while fewer than the most
  are [observed]. A trait whose requirements the player does not meet cannot be taken
  and is drawn at half brightness [hypothesis: no screenshot shows one; the template's
  `_TextAlpha` allows a fainter row].

### 4.1 The trait menu drawn from its file (`[UI] TileMenus=TraitMenu`)

Off by default; the hand-built menu of §4 stays whole with the switch off and whenever
this does not hold (`engine/nvf-play/src/tiles/trait_menu.rs`). Sources: the vanilla files
[observed: by `fnvtool ui tree --menu menus\trait_menu.xml` and the shared pieces it
includes, read privately]; the original running, its job `trait` at 1280 x 720 [observed:
private capture, below "the capture"]. Taking a trait was not captured: the job's clicks took
none.

- **What the program writes.** The file leaves these open to code: the menu's `_CurrPoints`
  and `_MaxPoints` (the capture: 0 and 2 at the opening; how they follow a taken trait
  [hypothesis]); `string` of the title (`LUM_Headline_Title`), the description
  (`TM_DescriptionText`) and the counter (`LUM_PointCounter`), which NVForever words itself
  on this path ("Needs: …", "Most ranks: 1", "2 left to choose") [design: the string rule,
  LEGAL.md; the masters hold no setting for these lines; the title is their
  `sTraitMenuTitleText`]; `filename` and `visible` of
  the picture (`LUM_SelectionIcon`); the rows, made from the template `LUM_PerkTemplate`
  (row `LUM_Perk`, with `ListItemText` and `LUM_Template_ItemMarker`): `string`, `_selected`,
  `_TextAlpha`, `listindex`, `_y`, `height`; of the list (`LUM_PerkList`) its `visible`, which
  its `_enabled` follows (the capture: 1), `_highlight_y`, `_selected_height`, and of its
  `lb_scrollbar` the counts and value. The buttons are `LUM_ResetButton` and
  `LUM_ContinueButton` (the capture: both `target` 1).
- **Rows** [observed: the capture]: named `LUM_Perk`, `id` 10, the active component's name
  carrying an instance suffix (`LUM_Perk:1`, `LUM_Perk:8`, none) that is not the row's index;
  52 apart; `_TextAlpha` 255 and `_enabled` 1 on all read, so a fainter row is
  [hypothesis] (§4). The capture's list had **14 rows, not 16**: Fast Shot and Four Eyes were
  not in it, with `_num_filtered` 0. Why is not known [hypothesis: traits of that save's
  player, or its requirements]; here the sheet decides the rows, as §2.
- **`_selected` is "taken"**, the mark the row draws [hypothesis: the capture took none]; the
  row chosen is the highlight.
- **The highlight and the text are apart** [observed: the capture]: the menu opens with row
  0's picture and text and `_highlight_y` -1; on a row it is the row's place and
  `_selected_height` 52; off the rows it is -1 and 0, and the text and picture stay. Here the
  sheet's chosen row stands meanwhile, and the highlight comes back on a key's move [design].
- **The list's scroll** [observed: the capture, 14 rows of 52 in a list 470 x 676 at -15, 60]:
  `_scroll_delta` 52, the bar's `_number_of_items` 2, `_number_of_visible_items` 1, `_jump_size`
  13, `_step_size` 1, its marker 322 high at 0: so a value is one row, the last row whole at
  value 1. These are the container capture's counts (barter-menu.md §8) with whole rows,
  `tiles::list::counts`; sixteen rows would be 4 items, three to scroll, with the same step
  [design: the formula, not a read]. Up and Down move the highlight 52; Page Down took
  value 0 to 1 and the highlight to 624, row 13, 13 rows on, and Page Up back. A first Down from
  a row did nothing once in the capture, which is not understood and not modelled.
- **The file has a scroll bar for the description**: `TM_DescriptionScrollbar` in the box
  `TM_DescriptionBox`, moving `TM_DescriptionText` by its value times its own `_ScrollDelta`
  [observed: the capture: 20; `y` 5, -15, -35 at 0, 1, 2]. No control of NVForever's is added.
  Its `_number_of_items` read 9 on every row, short texts too, with `_number_of_visible_items`
  1, `_show_always` 0, `visible` 1: the file's own count, which **does not follow the text's
  length**, so the arrows and the marker always show, as on the capture's shots, and §4's
  "arrows" left out the marker. Its arrows moved it one value, `wheelmoved` one value, its page
  areas nothing (`_jump_size` 0). The box is 275 high, the texts read 207 to 277.
- **What ends that scroll**: only the count, at value 8, 160 down [documented: the prefab's
  upper bound; hypothesis: not driven there in the capture]. It does not stop at the text's
  end: a short text can be scrolled away. So every text up to 430 high is read to its end
  by the file's count alone, the capture's longest among them. A text longer than that (a
  mod's, or an aid's) gets the count raised just as far as brings its end into the box, and the
  count goes back for the next row [design: a description must be readable to its end; the
  original is taken to cut it off].
- The bar stands beside the box, outside its width, so that box's window does not crop it;
  windows further up still do (`fnv_ui::input::OUTSIDE_CLIP`) [hypothesis: a window crops what has `clips`; the capture's
  shots show the arrows and marker there].
- **Backing** [observed: the capture]: `LUM_Background` `alpha` 193.80, `brightness` 128, 1144 x
  1070 at 291.33, -45, written here as the container's; the world is blurred behind it by eye on
  the opening shot [hypothesis: the radius, the other tile menus']. The whole-screen dark
  sheet is the hand-built menu's only. §8's 0.4 was a reading of one screenshot's pixels.
- **`stats_icon_badge`** [observed: the capture]: not visible, `Interface\Shared\empty.dds`;
  nothing here writes it.
- **Keys** stay the hand-built menu's (`crate::chargen`), so a press acts once, and R and A
  through the file's `_PCButton_*` act only for a script's MenuTapKey or a controller not
  standing in for them. In the capture Left and Right moved nothing. Here, with tiles only,
  they scroll the description, NVForever's addition [design: the keyboard's and the
  controller's route to a description's end]. Page Up/Down step by the rows the list shows
  whole. The wheel over the description or its bar scrolls the text, elsewhere the list
  [documented: the file's `wheelable`; the capture's synthetic wheel moved neither, so a real
  wheel is unread]; scrolling never changes the chosen row, nor does the pointer resting on a
  row that moved under it.
- **Not known yet** (a retake is coming): how a trait is taken in the original: `_CurrPoints`,
  the marks, the counter; what Reset restores with traits taken; `_TextAlpha` and `_enabled` on
  a trait that cannot be taken; a real wheel; the description at value 8; why the capture's list
  lacks two traits (observer-checks.md, 91).
- **When it falls back** [design]: every frame the file's paths, each row and its children,
  the room and the measures are read from the tree. Short of any, the rows go and the
  hand-built menu draws its sheet as it stands, and the next frame tries again; a tile the
  model needs coming back brings the tiles back. One opening keeps its scrolls through that; a
  new opening starts from the top with no highlight; the counts the file gave are kept for as
  long as its tree stands, so what the engine wrote is not taken for them. A menu the scripts
  put in this one's place is drawn at once by its own drawing, tiles and blur gone that frame.
- **Evidence**: the unit tests (`tiles::trait_menu`) use a small layout written for them,
  which says nothing of the file's geometry; the smoke scenarios `tiles-trait-*` read the
  vanilla file.

[design: `chargen/lists.rs::info`] The hand-built trait description still uses its fixed
275-unit clip without scrolling. The hand-built tag description likewise keeps its existing
unscrolled drawing. The scrolling described in §3.1 and §4.1 belongs only to the opt-in tiles.

## 5. The name menu (1051)

[documented: texteditmenu.xml unless tagged]
- A box 720 × 180 centred on the screen; a dark picture 25 out on every side; the top
  bracket (a line with lines fading down 50 at its ends) and the bottom bracket at 180.
- The question centred, 32 down: "Enter character name." [observed]; the name centred
  in the middle of the box's height, wrapping at 250; Ok right-aligned 16 in from the
  right side, its box's bottom 16 above the box's.
- The question and the name glow [observed: the screenshot's letters]; they are drawn in
  font 1 (`glow_monofonto_large`) [hypothesis: the default of a text the file gives no
  font]; Ok in font 2, a text box's own [documented: text_box.xml].
- It offers the player's name, or `sDefaultPlayerName` ("Courier") the first time
  [observed]. Typing adds to the name, Backspace rubs out, Enter or Ok takes it
  [hypothesis: the keys; no caret is drawn, none is on the screenshot]. A name of spaces
  only is not taken; at most 24 characters [hypothesis].

## 6. What Done writes, and the survey

- Name: `PlayerState::name`. Tag skills: the tagged skills replace the player's tags.
  Traits: the player has exactly the taken ones among those on offer (perk rank 1); what
  a trait does is its perk's effects, which nothing runs yet. S.P.E.C.I.A.L.: each
  attribute's permanent value becomes the sheet's (vigor-tester.md).
- `fnvtool actor chargen --schema schemas/fnv <Data> <plugins…> [--special 5,3,7,7,5,6,7]
  [--tag Speech --tag Survival --tag Guns]` (`fnv.actor.chargen/1`) lists the name
  offered, the S.P.E.C.I.A.L. sheet, the tag-skill list with values and the traits with
  their requirement lines, after applying the given sheet as Done would.

## 7. Checks

- Unit tests: the sheets' rules (`fnv_game::chargen`), the list model, the texts, the
  name box, the test aid (`nvf-play` `chargen`).
- The trait menu's file, missing or cut down, without touching the game's folder:
  `NVF_PLAY_TEST_TRAIT_TILES=<s>:missing`, `<s>:cut:<tile name>`; `<s>:long` makes a
  description long enough to scroll (§4.1, `tiles::trait_menu`).
- Open a menu without the opening: `NVF_PLAY_TEST_SCRIPT=<s>:menu:<code>`; play it with
  `NVF_PLAY_TEST_CHARGEN=<s>:pick:<row>,<s>:type:<text>,<s>:clear,<s>:reset,<s>:done`;
  set `NVF_PLAY_TEST_SCRIPT_MENUS=off` so the unattended runs' auto-close leaves it open.
  The smoke test in CLAUDE.md runs the tag-skill and trait menus and the name menu.
- Screenshots of ours beside the owner's (kept privately): the rows, picture, text,
  lines, buttons, scroll bar and counter of the trait and tag-skill menus and the name
  box stand within a few pixels of the original's.

## 8. Not built, or guessed

- The world behind is not blurred (the original blurs it), except behind the trait menu
  drawn from its file (§4.1); the dark picture is 0.4 alpha [observed: 33 against 54
  brightness inside and just outside it, on one screenshot; the capture of §4.1 reads the
  picture's `alpha` 193.80].
- The hand-built trait menu's description is cropped at its box with no way to scroll it,
  and the other two pages' can run over what is below [design: read from `lists.rs`, not
  seen running]; §4.1 reads to the end, the hand-built drawing has its own fix to come.
- Sounds (list 5 items 41, 42; `nvf_play::ui_sounds`, menus `tag skills` and `traits`):
  the chosen row moving and the pointer onto Reset or Done `UIMenuFocus`, a row taken or
  given back `UIMenuPrevNext`, a row refused `UIMenuCancel`, Reset `UIMenuCancel`, Done
  `UIMenuOK`. The files silence the rows (CGM_SelectItem, LUM_SkillItem `&nosound;`)
  [observed]; the owner wants them, so the rows take the template's pointer sound and
  the trait file's perk-row click (LUM_Perk `UIMenuPrevNext`) [design]; Done's click
  and the text boxes' pointer sound are the files' [observed]; Reset's is [design].
- The keys besides the hints, Reset's meaning, the counters' other wordings, a trait the
  player cannot take, the name's length limit: [hypothesis], listed in
  docs/owner-review.md.

## 9. The character revision (VCG04)

- **The original** [observed: FalloutNV.esm quest VCG04 "Player Character Revision",
  script VCG04ActivatorScript 001164A4, `fnvtool script decompile`]: activating one of
  the quest's activators at Goodsprings' edges stores the player's XP and shows message
  VCG04Message. "Revise" calls `player.SetLevel 1`, `ResetXP`, `RemovePerk` on the ten
  traits of FalloutNV.esm and eighteen perks of the early levels (Intense Training,
  Swift Learner, Educated, …) and `ShowRaceMenu`; the next frames set each
  S.P.E.C.I.A.L. to 5 with `SetSPECIALPoints 40` (the Vigor Tester), each skill to 0
  with `SetActorValue` and open `SetTagSkills 3`, then `ShowTraitMenu`, then the
  message again. "Done" gives the stored XP back with `RewardXP`, so the player levels
  up again and spends the points and perks anew.
- **Here** the revision keeps the player's progress [design: the owner's wish, playtest
  list 7 item 1, "retain our currently selected perks and our skill levels … minus our 3
  tag skills that are reapplied"] (`fnv_game::revision`): from that script only,
  SetLevel, ResetXP and the closing RewardXP on the player do nothing, RemovePerk takes
  traits only (the trait menu offers them again), and SetActorValue of a skill on the
  player does nothing. A player's skill is the skill formula over the current
  S.P.E.C.I.A.L. + the tag bonus of the current tags + the points spent at level-ups
  (`fnv_game::actor_values`), so the spent points stay and the new S.P.E.C.I.A.L. and
  tags re-derive the rest. SetActorValue of an attribute on the player does nothing
  too, and the script's `SetSPECIALPoints 40` opens the character menu's attributes
  page (menu 1048, the one Intense Training's AddSPECIALPoints opens, levelup.md §8)
  with the points the player's attributes add up to (40 or more), so the player moves
  points around on the S.P.E.C.I.A.L. they have [design: owner, 2026-10-26, the rebuild
  still showed 5 in each; owner's playtest, #1031: the attributes page, not the Vigor
  Tester]. The Vigor Tester stays for Doc Mitchell's first visit
  (`ShowLoveTesterMenuParams`). The tag-skill menu starts with the old three tagged (the
  SetTagSkills rule of §1). Each kept call is logged `revision: keeps …`.
- The script is known by its record (SCPT 001164A4) whichever plugin edited it last:
  OldWorldBlues.esm overrides it with the same calls [observed: `fnvtool script
  decompile` of the base game and add-ons; the owner's game ran
  `OldWorldBlues.esm|SCPT 0x001164A4`, 2026-10-26], and a check on the
  FalloutNV.esm copy alone kept nothing with the add-ons loaded.
- Before this the revision zeroed every skill: SetActorValue overrides a value's base
  (`functions::actor`), and a player's skill was then 0 whatever its formula.
