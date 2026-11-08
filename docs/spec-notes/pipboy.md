# Spec note: the Pip-Boy

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/pipboy.rs` (state, keys,
pointer, the image the menus are drawn into), `pipboy/frame.rs` (what every page shares),
`pipboy/items.rs`, `stats/`, `data.rs` and `data/` (the pages), `pipboy/test_aid.rs`;
`engine/nvforever/src/pipboy_arm.rs` (the model's screen, glows and buttons),
`player_body.rs` (the clip and the arms' field of view), `imagespace.rs` (the blur behind
it), `engine/nvf-assets/src/texture.rs` (`pc_texture`).

## 1. Sources

- **Fifteen screenshots from the owner**, 2026-10-11, 1280 × 720 (kept privately in
  `C:\fnv-private\references\pipboy\original\`): every tab of STATS (Status with CND,
  RAD and EFF, S.P.E.C.I.A.L., Skills), ITEMS (Weapons, Apparel, Aid, Misc, Ammo) and DATA
  (Local Map, World Map, Quests, Misc, Radio), in Goodsprings in daylight.
- **The game's interface files** in `Fallout - Misc.bsa`: `menus\main\inventory_menu.xml`,
  `stats_menu.xml`, `map_menu.xml`, `menus\globals.xml` and the pieces they include
  (`card_info.xml`, `tabline.xml`, `tabline_template.xml`, `super_text_box.xml`,
  `text_box.xml`, `box.xml`, `line.xml`, `vertical_fade_line.xml`, `list_box.xml`).
  Described here in plain words; nothing of them is shipped.
- **The models and clips**: `meshes\pipboy3000\pipboyarm.nif` (and the female and NPC
  forms), `meshes\characters\_1stperson\locomotion\male\pipboy.kf` (and
  `female\pipboyfemale.kf`), textures under `textures\pipboy3000\`.
- **`Fallout_default.ini`** in the game folder, and the owner's `Fallout.ini` (the same
  values): `fPipboy1stPersonFOV=47.0`, `fDefault1stPersonFOV=55.0000`; `[Pipboy]`
  `fBlurRadiusPipboy=3.5`, `fBlurIntensityPipboy=0.25`, `fScanlineScalePipboy=50`,
  `bEnableFlickerPipboy=1`; `[InterfaceFX]` `fBrightenPipboy=1.3000`,
  `bEnableScanlinesPipboy=1`, `fVertHoldChance=0.08`, `fShudderChance=0.20` (under
  ";pipboy menu transitions"). NVForever never reads the INIs at run time; the values
  are built in where used.
- The menus' own words (`&-sInventoryItems;` and the like) are game settings that the
  masters do not hold (only 130 string settings are in `FalloutNV.esm`, none of these)
  [observed]; the labels are taken from the screenshots.

## 2. The device

- **The clip** [observed]: `pipboy.kf` is one sequence `Pipboy`, 0.7333 s, clamped, with
  text keys `start` 0, `Blend: 8` 0.033, `Hit` 0.333, `end` 0.733. It drives
  `Camera1st` (a small move, no turn) and both arms from the spine out. Measured on the
  arms rig, the screen's place before the eye through the clip:

  | seconds | screen against the eye |
  |---|---|
  | 0.03 | 115° down (the arm hangs) |
  | 0.23 | 30° down, turning to face the eye |
  | 0.31–0.40 | straight ahead, 11 to 13 units off, facing the eye (0.96–0.99) |
  | 0.73 | 32° down, edge on, going away |

  So the clip both brings the Pip-Boy up (to `Hit`) and puts it away (after `Hit`).
  NVForever holds it at `Hit` while the menus are open and plays on to the end when they
  close [hypothesis: the original does the same; the owner saw it come up and go
  straight back down when it was played to the end]. It fades in over its first 8
  frames and out over its last 8 [hypothesis: frames at 30 a second].
- **Field of view** [documented: the INI]: the arms are drawn as through a view of 47°
  (`fPipboy1stPersonFOV`) while the Pip-Boy is up. NVForever draws them bigger about the
  eye across the view, by the ratio of the two views' half-widths (1.76 at the default
  75°), eased with the clip, instead of with a camera of their own [design]. On the
  owner's ITEMS screenshot and ours the device then stands in the same place to a few
  pixels (buttons, dial, screen corners) [observed]. The normal first-person field of
  view (`fDefault1stPersonFOV=55`) is not applied: the arms and weapons are drawn with
  the world's (open question, §8).
- **Texture** [observed]: the models name `textures\pipboy3000\xbox\english\
  PipBoyArm01.dds`, whose buttons carry the console's LT and RT; `pc\`, `ps3\` and
  `ps3o\` forms lie beside it, and the PC screenshots show no such labels. A texture a
  model names under `\xbox\` is read from `\pc\` when that file exists [hypothesis].
- **Parts** [observed: shape names]: the screen `pipboyscreen:0`; buttons
  `PipBoyButton01`–`03` at the glows `StatsGlow`, `ItemsGlow`, `DataGlow` (STATS, ITEMS,
  DATA from the elbow out); the glass: `ScreenLit:8` (a lit, glossy copy of the screen in
  the same place, `screen.dds` with a normal map) and `glare:0` (`ScreenGlare.dds`, added
  over it, unlit, 0.47 units in front of the screen; NiAlphaProperty flags 4109, blend on,
  SRC_ALPHA / ONE; both shapes vertex colour 0.81; `fnvtool nif decode`). Also `RadNeedle`, `TabKnob`, `ScrollKnob`, `PipboyLightEffect`.
- **The page's button** glows alone while the Pip-Boy is up [observed: the screenshots];
  down, all three glow (owner, 2026-10-10). The pointer on a button lights nothing: a
  player comparing the two reports that the original's buttons do not light under it
  (#1516; not observed again here).
- **The needle and knobs** (nvforever `pipboy_controls.rs`, all the numbers there). The
  needle, the wheel and the knob turn through their own origins, their nodes' in the model
  [observed: `fnvtool nif decode`; the model has no controllers on them and `pipboy.kf` does
  not name them]:
  - `RadNeedle` (two shapes), about its local Z, out of the dial: total rads, not the rate,
    linear, 18° per 100, the bind pose at 500: 0 straight down, 500 to the right, 1000 straight
    up [observed: private job pipboy_special_device_motion: shots at 0 to 990 within a few
    degrees; 58 rads with a rate of 9 reads 58]. It moves slowly, the same speed for a long
    way as a short one: about 0.6° a game loop, a drop of 690 rads still on its way two
    seconds on [observed]; the capture has neither a fine clock nor the loops' rate, so 30° a
    second is [hypothesis], within about 20 to 40. No start-up delay can be told from the
    shots' own stalls. Held at the dial's ends above 1000 [hypothesis]. While the Pip-Boy is
    up it quivers a little about where it rests, a small shiver [observed: playtest 2026-11-05,
    against the original]; two sines of unrelated rates, about 2° in all at 3.7 and 6 a second,
    are [hypothesis], judged by eye. The travel and the log are without the quiver.
  - `TabKnob`, about the Z of its node `TabKnob`, through the origin, the whole knob leaned.
    In that node's frame the knob is straight: its base flat (within 0.02), its two fins 180°
    apart and the dial's disc centred on the origin [observed: `fnvtool nif decode` of both
    arms, the plate's faces and UVs onto `PipBoyArm01.dds`, 2026-11-05]. The shape's own local Z
    leans 28° off that shaft. Stood square to the plate it was seen tilted too far against the
    original (playtest 2026-11-05), so it leans 17.6° with its top towards the top of the
    device, which is what dropping the node's own tilt would give (`pipboy_controls.rs`
    `KNOB_SHAFT`, `KNOB_LEAN_AXIS`). That the original does so is [hypothesis]: silhouettes
    against the private job's five tabs do not settle it (0.145 against 0.147 for square),
    though a free fit of the lean landed within 3° of it. The turn itself: the page's tab, not
    the page (tab 0 of STATS, ITEMS and DATA look the same, and the wrap comes back), five
    detents at the dial's five marks. Bringing the fin's tip onto each mark on the screen, by a
    camera fitted to the private job, takes 67°, 37°, 10°, −16°, −49° [observed]; a constant 28°
    a tab, 66° at tab 0 and −46° at tab 4, is within 3° of those, and that the step is constant
    is [hypothesis]. A step is over in about 8 game loops [observed]; 180° a second, and the wrap swinging back at that speed,
    are [hypothesis].
  - `ScrollKnob`, about its local X, its axle: the list's selected row, one ridge a row (the
    wheel has 26 ridges, its outermost vertices every 13.85° [observed: `fnvtool nif decode`]),
    200° a second. It comes back to the same place at the top of a list [observed: private job
    pipboy_special_device_motion: its ridges to within 0.3 pixels]; the rest is [hypothesis]:
    that it follows the selected row and not the list's scroll, which the capture does not
    separate; the ridges' 17 pixels hide whole ridges moved between shots, so the step is not
    in the capture (a first reading of 2° a row was far too little against the original,
    playtest 2026-11-05), and the sign is not resolved. The mouse wheel at the top of
    the list moved neither. Following the row means the pointer on a row turns it too [design].
  - Absolute `bind × rotation` on the shape's own transform, after the arm is posed; a rebuilt
    rig's shapes take a bind of their own. Only rigid shapes of the size of the game's own
    male and female arm turn: a replacement model can share the names with other pivots,
    axes or skinning, so others are left still, said once ("pipboy: needle left still: …").
    Only the arms rig: the third-person model's controls stay still and NPCs' arms are not
    touched [design]. Off the arm a control finishes travelling to the last reading, then
    holds [hypothesis]; the first reading is taken at once. Logged "pipboy: needle settled at …",
    "pipboy: tab knob settled at …", "pipboy: scroll wheel settled at …".
- **The view** is first person while it is up and goes back to what it was [hypothesis].
- **The world behind** is blurred by `fBlurRadiusPipboy` (3.5) in the image space
  modifiers' blur units [hypothesis: the same units], eased with the clip; what is
  nearer than 40 units is the player's arms and is neither blurred nor blurred into the
  world [design]. `fBlurIntensityPipboy` is not used (what it scales is not known).

## 3. The screen

- **Canvas** [documented: globals.xml `_pipboy_width` 1024, `_pipboy_height` 768]: the
  menu files lay out the Pip-Boy menus in 1024 × 768.
- **Mapping** [observed]: the screen's texture coordinates run from 0 to about 0.75
  (u −0.010 to 0.753, v 0.002 to 0.762) over a 4:3 shape. Fitted to the owner's ITEMS
  screenshot, the canvas fills 0.8 of the texture each way (the menus were 1.07 times as
  wide and 1.10 times as high as a fill of 0.75 gave): a 1024 × 768 canvas in the top
  left of a 1280 × 960 image. So the screen shows the canvas to about 964 across and 731
  down. NVForever draws the menus into such an image (interface lengths are its pixels)
  and puts it on the screen as it is.
- **Colour** [observed: private job pipboy_special_device_base]: a filled bar in the
  original at noon in Goodsprings, level view, is about (177, 133, 47), dimmer than the
  playtest shots below, about (206, 163, 57) indoors and (201, 161, 54) at dusk. Whether the
  hour or the capture's own settings make the difference is not established. `SCREEN_TINT`
  is fitted to the playtest shots (the static treatment below). `fBrightenPipboy` is not
  applied as such.
- **The glass** [observed: private job pipboy_special_device_base, 11 × 11 means at 1280 ×
  720]: at eight empty places the original runs from (7, 6, 2) at the far left edge, (15,
  11, 3) at the far right and about (20, 15, 4) beside the figure on the left, to (30, 25,
  11) lower right and (79, 77, 49) at the top middle above the figure: a strong falloff to
  the edges, warm, and brighter across the top. Through the engine's display-value add
  (nif.md §6) the glare lay over the menus as a pale grey-blue haze, so it was hidden.
- **The original's screen in playtest shots** [observed: 1920 × 1080 shots of the original,
  2026-11-05]: a filled bar reads about (206, 163, 57) indoors and (201, 161, 54) at dusk, the
  same screen in both. Across a scanline red stays almost flat (201 to 211) while green runs
  145 to 183 and blue 48 to 66, so the stripes read yellow and orange, not grey. Below the
  bars the ink is yellower than amber's own ratio: green about 0.74 of red and blue about 0.3
  at every mid brightness. The empty screen is about (35, 27, 15) beside the figure and (22,
  17, 7) at the left, the glare at the top about (80, 73, 50). These shots are brighter than the
  private job's above; the numbers below are fitted to them.
- **The static treatment** (nvf-play `pipboy/device_screen.rs`, all its numbers in that
  file): image × `SCREEN_TINT` × the shapes' vertex colour (0.624 in linear light), plus the
  glare, with red stopped at a ceiling, through Bevy's TonyMcMapface table. The scene's
  image-space grade leaves the screen alone: the shape marks its pixels in alpha
  (`NifSurface::pipboy_screen`, `imagespace.wesl` `SCREEN_MARK`), because the grade at noon
  brightens by 1.3 and the original's screen does not follow it; the fade and VATS still cover
  it [design]. In this engine at 1920 × 1080 a filled bar reads (207, 169, 64) indoors, red
  206 to 208, green 154 to 189, blue 58 to 73, and (200, 163, 62) at noon in Goodsprings; the ink
  has green at 0.73 and blue at 0.28 of red at mid brightness, the empty screen is (35, 28, 13)
  beside the figure and (25, 19, 8) at the left, the glare (79, 72, 49) [observed: this engine's
  shots, 2026-11-05]. Custom colours and other hours are not measured.
  The ceiling is [hypothesis] for how the original comes by its flat red; it is fitted, and
  the original's own path is not established:
  - The falloff is the background picture's vignette: `pipboy.dds` runs from about 9 at its
    corners to 93 near the top of its middle [observed: the file]. Drawn over a large
    undercoat, as the first fit had it, the vignette was lost. The picture is at brightness
    120 (the menu file's 90 at the first tint [documented: inventory_menu.xml]), its red in
    linear light at 0.7 of amber's as the original's empty screen is less red, covering
    the undercoat at alpha 0.4 and adding what it would over black.
  - The undercoat, the screen camera's clear colour, is next to nothing: (0.00047, 0.00055,
    0.00054) of light in the view.
  - The glow across the top and the patch on the right are `glare:0`'s: its texture's
    brightest band and patch lie there [observed: the file]. It is drawn over the menus on a
    copy of its material, unlit, added in linear light as the terminal's glare is (hacking.md
    §7), at a linear scale of (3.2, 1.4, 0.75) on the texture, warmer than the texture.
    The copy keeps the shape's squeezed first-person depth, which Bevy's plain additive
    material would not, and the sheet's soft intersection is off.
  - `ScreenLit:8` stays hidden: `screen.dds` is black, `Screen_n.dds` is flat and its
    specular colour black [observed: the files; `fnvtool nif decode`], so it carries no
    falloff. Both shapes used are unlit, so nothing here is lit by the world [design]; the
    hour does not move it here, the grade being left off.
  - One baked column of scanlines over the canvas, a value per row (no tiling): the band
    of `pipboyscanlines.dds`, rescaled to dim display values by 20% at its dark end, every
    9.5 of the canvas: about 22% on the empty screen (the original 16 to 22%) and about 18% in
    green on a filled bar (the original 21%). The undercoat and the picture are raised by what the lines take
    away on average, as the terminal's are; the ink is not, its bright rows keeping their
    value.
  - One owner around the shared image: the layers are not part of the pages' tree, so
    any painter of the pages gets them, once. The owner retints the picture itself when
    the Pip-Boy colour changes, and the generic recolouring leaves its nodes alone; the
    undercoat is the glass and keeps its colour [hypothesis]. Not in the screen menu (no background, no
    scanlines, no glass, docs/design/screen-menu.md §2). The device's image shown flat
    because the model is missing keeps the layers and has no glare.
- **Scanlines** [observed]: `pipboyscanlines.dds` is 64 × 64 DXT1, one soft dark band
  (white at the top to about 0.8 at the bottom). On the screenshot the solid bar's rows
  are bright and dim by about 15% every 6 pixels, which is 9.5 of the canvas. Flicker,
  vertical hold and shudder (the INI's transitions) are not built.
- **The beam**: a wave of light travels down the screen, its leading edge hard at the foot and
  a long fade behind it [observed: private job pipboy_special_device_motion, the same empty
  stretch of the screen frame by frame; playtest 2026-11-05]. Edges at the same place come 5 s
  apart. On those frames the light rises about straight over 95 rows of the 720 to about +14
  in display red, then drops within about 12 rows, the measure's smoothing included; it adds
  about (1, 0.58, 0.17) in linear light. The playtest shots of the original show the same step
  of 13 to 14. Frames go by at about 7 rows a frame, and at least 190 rows went by within one
  clock second, so the time is [hypothesis] within about 1 to 2 s. NVForever draws one column
  (`device_screen.rs`): 224 canvas units high with an edge of 24, the Pip-Boy's colour lightened
  0.2 towards white at a fitted alpha of 0.016 at the peak, over the pages and under the
  scanlines, 1.3 s from above the top edge to off the foot on the real clock, hidden between
  sweeps and held with the pause menu. Not in the screen menu.
- **Background**: `Interface\Shared\Background\pipboy.dds` over the whole canvas at
  brightness 90 [documented: inventory_menu.xml]; `ScreenGlare.dds` and this picture are
  512 × 512 DXT1, alpha 1 throughout [observed].

## 4. What every page shares [documented unless marked]

- The menus' rectangle: 50 in and down, 855 × 650.
- Heading: a vertical line fading down at its left (60 long), a line of 50 to the title
  (font 4, 70 in), a line from 20 after the title to the first card; cards of figures
  (`card_info.xml`): a line along the top, one fading down the right side, the title 5
  in and the value right-aligned 10 from the right end, both 20 down in font 2. ITEMS:
  Wg at 215 (155 wide), HP 380 (155), DT 545 (110), Caps 665 (190). STATS: LVL 270
  (100), HP 380 (155), AP 545 (135), XP 690 (205).
- Tab line (`tabline.xml`): 650 down, as wide as the menus, a vertical line rising at
  each end; each tab is its name in font 2 with 35 across and 15 up and down
  (`tabline_template.xml`, `text_box.xml`); the tab shown is boxed (lines at brightness
  200) and filled (brightness 128, alpha 32); the lines between the tabs are as long as
  each other [hypothesis: the file leaves their length to code; on the screenshots they
  are even].
- Lines are 2 thick on a PC; lines at brightness 200, text 255.
- A text's letters stand 15/31 of the font's size under its top [hypothesis for every
  font but 2, where it was measured: message-menu.md §3].

## 5. The pages

- **ITEMS** (`items.rs`): the inventory model (`menu::model`, `Mode::Inventory`), the
  tab choosing the kind of item. Built: the heading (Wg, HP, DT and Caps), the list
  (name, count in brackets, the filled square of what is equipped, the selected row
  boxed), 9 rows at once (51 a row in a list 468 high). A row (its box) runs from the
  scroll bar to 5 short of the list's right edge, as the DATA lists' [design: it was the
  Repair page's 393, which ran over the item's figures; owner's screenshot 2026-11-01].
  - Added 2026-10-23 (owner's playtest list 7, items 5, 6, 15, 20, 33):
    - **HP** is the STATS page's health out of its whole; **DT** the threshold a hit on
      the player meets (worn armour at its condition plus the Damage Threshold actor
      value, `combat::target_armor`), one decimal [observed: "HP 205/205", "DT 8.0" on
      the owner's screenshots; hypothesis: the figure is combat's].
    - A name over two lines makes its row taller; the first row drawn moves on so the
      selection is always in the list, and the model keeps it as its scroll position (the
      last of many weapons could not be brought into view: the model counts nine rows a
      page) [design].
    - An aid item's EFFECTS: aid-items.md §14.
    - Skill books (BOOK with a skill to teach) are on the Aid tab, other books on Misc
      [owner: "Skill books ... should be showing in the Aid tab"; hypothesis: the
      others].
    - The Misc tab's folders [owner's wish; design]: the keys (KEYM) are one "Keyring"
      row and the Caravan cards one "Caravan Cards" row, at the top, each with its count;
      using one lists what it holds under a "Back" row (Backspace too; another tab closes
      the folder). The cards are those AddCardToPlayer gave (kept outside the inventory,
      caravan.md §2), and any CCRD item carried; they are neither used nor dropped.
  - The scroll bar at the list's left (32 wide), only while some rows do not show: the
    arrows at its ends and a line for the part shown [observed: the owner's scroll
    screenshot; the line's look as the DATA lists' (hypothesis)].
  - The selected item (the row under the pointer is selected) at the right
    [documented: places, `inventory_menu.xml`, `item_stats_display.xml`]: its picture
    (ICON) in `IM_ItemIcon` (450, 10, 380 square); its figures in `IM_ItemInfoRect`
    (385, 340, 446 × 150), cards a third of it wide and high, 10 apart: DAM (WEAP base
    damage) or DT (ARMO DNAM) at the left, WG and VAL; a weapon's STR (its Strength
    requirement) over VAL; the second row CND (a bar 62 in, 15 down, 20 high, 72 short
    of the card) and beside it, two cards and 10 wide, the ammunition ("20ga (2/30)":
    the AMMO short name, then what a clip holds of what is carried over what is
    carried, hypothesis) or armour's class ("Light", "Medium", "Heavy"); EFFECTS across
    the three cards on the first free row: an aid item's own effects, apparel's or a
    weapon's enchantment's (EITM), written as the STATS EFF list writes them ("HP +40",
    "AGL -1") [observed: the owner's ITEMS screenshots, all five tabs]. WG and VAL are
    the stack's (the Stimpak's VAL 675 is 9 × 75; WG "--" for nothing) [observed: VAL;
    hypothesis: WG].
  - Added 2026-10-19 (`items.rs`):
    - **DPS** [documented: `item_stats_display.xml` `DPSInfo`, `sInventoryDamagePerSecond`;
      hypothesis: formula]: the damage a shot does at the item's condition
      (combat-rules.md §17) times its shots a second (an automatic weapon's fire rate,
      else its attack shots a second, else one), rounded [hypothesis: the GECK wiki's
      Weapons page names damage and fire rate as the DPS calculator's inputs]. Shown in
      the container and barter figures only: the original's ITEMS page has no DPS card
      (2026-10-24, below) [observed: the owner's ITEMS screenshots].
    - **The weapon's skill badge** (2026-10-24; replaces the ammunition's picture and
      the DPS card guessed on 2026-10-19) [documented: `IM_ItemIconBadge` at (390, 260),
      64 square, and the text `IM_StrReq` at (440, 300), font 7, left-justified;
      `textures\interface\icons\typeicons\weap_skill_icon_*.dds` (sm_arms, big_guns,
      energy, explosives, melee, unarmed), 64 square, a boxed picture in the lower left
      48; observed: the owner's screenshots show the bullets picture with 0, 25 and 50
      for the Single Shotgun, Service Rifle and Sawed-Off Shotgun (their Guns skill
      requirements, WEAP DNAM) and the knife picture with 25 for the Throwing Spear
      (Melee Weapons 25)]: the picture of the skill the weapon uses and, beside it, the
      skill it needs, "0" too. A weapon of another skill (binoculars: Barter) shows
      neither [hypothesis].
    - **The buttons' text** (Repair/Maintain R), Mod X), the repair and mod lists'
      Cancel/Exit) [documented: `text_box.xml`: the text sits half the box's 15 spare
      height plus 10 under the button's y; observed: "Maintain R)" clear of the heading's
      cards on the owner's screenshot]. Before 2026-10-24 it was drawn at the button's y
      and touched the Caps card (the owner's report).
    - **"Mod X)"** [documented: `IM_ModButton`, 47 under the Repair hint, `sInventoryMod`,
      PC key X; hypothesis: shown for weapons with mod slots (WMI1–3), bright when the
      player carries a mod that fits a free slot, dim otherwise]. X on a bright hint opens
      the mod menu (2026-10-20, weapon-mods.md §4–§5); elsewhere X does nothing, a right
      click drops (§6) [design: owner 2026-11-02, #1393].
    - The repair hint ("Repair R)" / "Maintain R)") is built (repair.md §5), and the
      repair list takes the pointer (repair.md §5).
  - The mod menu (`item_mod.rs`): weapon-mods.md §5.
  - The flat image (no model's screen) shows only once the Pip-Boy is all the way up:
    before, it flashed for a frame or more on every Tab while the body took the screen
    (the owner's list 5, item 2).
- **STATS** (`stats/`): §5.1.
- **DATA** (`data.rs`, `data/`): §5.3.

### 5.1 STATS

Implementation: `pipboy/stats/mod.rs` (layout, keys, pointer, test aid), `model.rs` (the
words and rules), `read.rs` (the records and the player's figures). Sources:
`menus\main\stats_menu.xml` and the prefabs it includes (`list_box.xml`,
`list_box_template.xml`, `super_text_box.xml`, `text_box.xml`, `meter.xml`,
`scrollbar_vert.xml`, `box.xml`) [documented]; the owner's screenshots 1–5 (Status CND,
RAD, EFF, S.P.E.C.I.A.L., Skills); none of Perks and General, which are laid out from the
file alone. Compared side by side at 1280 × 720 with a new run (default stats): every
tab with a screenshot stands within a few pixels of the original [observed].

**Images in the file** [hypothesis, supported by the file's comment "filewidth/fileheight
already adjusted for zoom"]: an image tile shows its file at the tile's zoom from the
top left, cut off at its width and height, not stretched. The Vault Boy's pieces then
fill exactly the parts of their files the file gives as their sizes (the head's outline
is the top left 85 × 92 of `head.dds`) [observed].

- **Heading** [documented: places, `stats_*_info`]: LVL (the player's level), HP and AP
  as current/maximum (actor values 16 and 12: base and permanent modifier, less damage),
  XP as experience/what the next level needs, (n − 1) × ((n − 2) / 2 × `iXPBumpBase` +
  `iXPBase`) [documented: GECK wiki Settings; 200 at level 1, as the screenshots].
  Without the scripts' game the figures are "--".
- **Status** (`stats_status_container`, 50, 110, 855 × 482):
  - CND, RAD, EFF down the left, 84 wide, 51 high (font 2 and 20), boxed when pointed
    at, boxed and filled when chosen [documented]. The hardcore buttons (H2O, FOD, SLP)
    under them are not built.
  - Hints at the top right, right-aligned at 900, a slot 51 high each: "(9) Stimpak S)",
    the controller's healing mode's slot empty, "(3) Doctor's Bag E)"; on RAD "RadAway
    A)", "Rad-X X)" [documented: places, `_PCButton_*`; observed: the words; hypothesis:
    the count is left out at 0 (the RAD screenshot has none), dim unless pointed at].
    The four items are the DOBJ list's Stimpak (place 0), Doctor's Bag (21), RadAway (3)
    and RadX (2) (default-objects.md §1), else the Editor IDs `Stimpak`, `NVDoctorBag`,
    `RadAway`, `RadX` [observed: FalloutNV.esm's list names those four, so nothing changes
    with the shipped data; ESM full audit 2026-10-24].
    **Using them on CND** (built 2026-10-23, owner's request) [design]: S or a click
    on the Stimpak hint uses one on the body, or on the limb chosen (keys 1-6: head,
    torso, left arm, right arm, left leg, right leg; the same key again the body; the
    pointer on a limb's bar chooses it while there, and a click on the bar uses a
    Stimpak on that limb); E or the Doctor's Bag hint uses one on every limb. An item is
    not used when it would not heal: a Stimpak on the body at full health, on a limb
    that is whole, a Stimpak on a limb in Hardcore mode (it mends no limb there
    [observed: FalloutNV.esm help text, "In Hardcore Mode, a Doctor's Bag or an actual
    doctor is required"]), a Doctor's Bag with no limb hurt, or none carried
    (`model::refusal`). The chosen or pointed-at limb's bracket is boxed. The item is
    taken, its consume sound plays and the runtime uses it as the inventory does
    (`stats::use_hints`); what it does to the limbs: aid-items.md §3. RadAway and Rad-X
    are not used from here (A also switches the tab; the keys are logged).
  - CND: the Vault Boy at zoom 145, head 40 down and centred, face, torso, arms and legs
    placed from it as the file gives; a bracket (`vats_*_divider.dds`, 38 high) with a
    solid bar (77 × 14 at 25 or 51, 24) at each of six body parts, the bar as long as the
    part's condition (actor values 25–30, current over permanent) [documented]. A part at
    0 shows "Crippled" in place of its bracket [documented: the text's visibility;
    hypothesis: the word] and its `*_broken.dds` piece [hypothesis: the name]. The face
    is `face_00.dds` always [hypothesis; when 01–04 and 10 show is not known]. Under him
    "<name> - Level <n>" centred on the area's foot [observed: "Loot - Level 1"].
    Limb damage comes from combat's actor value damage; a new run's parts are whole.
    The bars read the same actor values combat lowers and cripples by
    (`Game::damage_limb`, `PlayerLimbs`) [observed: smoke `pipboy-stats-heal`].
  - RAD (zoom 130): "RAD RESIST" and the value (actor value 20, "8%") in a window
    showing the right end of `rad_effects_bracket.dds`, 360 down; the meter
    (`rad_meter_bracket.dds`) beside it, its 0 at 186 and full at 482 of the picture,
    "500" and "1000" over it, the arrow (`pointer_tail.dds`) at the player's rads (actor
    value 54) [documented: places; observed: a maximum of 1000, and no "0" written at
    the zero mark or at the arrow, so a 0 is not written (hypothesis)]. The radiation
    effects list above (`stats_rad_effects_*`, hidden by the file) is not built.
  - EFF: one line per source, 56 high, the name 20 in and the changes 300 to its right,
    lines between [documented]; the list centred in the area [hypothesis: fitted, one
    line at the middle on the screenshot]. Sources: the enchantment (ARMO `EITM`, ENCH
    of type Apparel) of each worn piece of apparel, named by the enchantment's FULL
    ("Vault Jumpsuit"), its Value Modifier effects written "Speech +2, Melee Weap. +2"
    with the actor values' short names (AVIF `ANAM`) [observed: the screenshot and the
    records]; then aid items (ALCH) with active effects. A detrimental effect (MGEF flag)
    is written with a minus [hypothesis]. "No Effects" when there is none [hypothesis:
    the words].
- **Lists** (S.P.E.C.I.A.L., Skills, Perks): 35, 110, 325 / 350 / 420 wide (25 / 50 more
  when a row is marked), nine rows of 51; the row 52 in (scroll bar 32 and 20), its name
  15 in and 20 down, its number right-aligned 10 before a right buffer of 20 (55 when
  marked), the mark 10 after [documented]. Skills are in name order [observed: Guns
  between Explosives and Lockpick], Big Guns left out [observed: thirteen; its AVIF is
  "Big Guns - OBSOLETE"]. A value shown is the current value plus worn apparel's changes
  [design: the game does not apply worn enchantments to actor values yet], skills kept to
  0–100; "(+)" or "(-)" when it differs from the permanent value [observed: "34 (+)";
  hypothesis: "(-)"]. Perks: the player's perks, playable and not hidden by PERK DATA
  [hypothesis: its bytes as trait, minimum level, ranks, playable, hidden], traits
  included [hypothesis], in name order, the rank as the number when the perk has more
  than one [hypothesis].
  - Names, descriptions and pictures: AVIF FULL, DESC, ICON (the record of code c is
    FormID 0x514 + c, 0x3E8 + c − 5, 0x44C + c − 12, 0x4B0 + c − 32 or 0x5DC + c − 46 by
    range [observed in FalloutNV.esm]); PERK and REPU the same.
  - The selected row is boxed and filled (alpha 40) after a key moved the selection or
    while the pointer is on a row [hypothesis: neither screenshot shows a box, the
    pointer on the tab line or off the screen].
  - Scroll bar when the rows do not all show: arrows at the list's top and foot, the
    marker 6 wide, as high as the track over (hidden rows + 2) [hypothesis: fitted to
    the Skills screenshot; the file leaves these to code].
  - Beside the list (`stats_display_rect`, from the list's right side to 905): the
    picture at zoom 150 in 340 × 384, centred, 60 above; a line 22 in and 319 down with a
    fading line down from its end; the description (font 2, lines 35 apart) 22 in, 333
    down, 175 high, scrolled 20 a step with arrows at its right [documented]. It scrolls
    when its lines take more than 175 [observed: five lines fit on the S.P.E.C.I.A.L.
    screenshot, six do not on the Skills one].
- **General** (`stats_genrep_container`, 20, 50): the reputations (the file's first mode)
  or the statistics; R or the button "General R)" / "Reputation R)" at the top right
  switches [documented: `_PCButton_R`; hypothesis: the button names the other list].
  The list 500 wide from 40, 100, nine rows [hypothesis: the file's height, 542 from
  100, would reach the tab line]. Reputations: those with fame or infamy [hypothesis],
  the selected one's picture (REPU ICON, zoom 150) and its title by the levels of fame
  and infamy (Accepted, Liked, Mixed …) [documented: GECK wiki GetReputationThreshold;
  hypothesis: "Dark Hero" with more infamy, "Soft-Hearted Devil" with more fame], and
  "Fame n, Infamy n" [design]. Statistics: the 43 New Vegas names [documented: GECK wiki
  GetPCMiscStat] in the order of their codes [observed: the owner's screenshot of the
  original's list, 2026-10-14] with their counters (docs/spec-notes/misc-stats.md; "--"
  without the script runtime). Beside them the karma:
  "Karma: <alignment>", the picture, the karma title and the karma's number
  [hypothesis: alignments by the globals `KarmaVeryEvil`, `KarmaEvil`, `KarmaGood`,
  `KarmaVeryGood`; pictures `karma_evil`, `_bad`, `_neutral`, `_good`, `_saintly`].
  The karma title is the game setting `sKarmicTitle<Good|Neutral|Evil><level>` (levels
  1–30, 30 for 30 and over) [observed: FalloutNV.esm holds 50 of the 90, e.g.
  `sKarmicTitleNeutral1` "Drifter", `sKarmicTitleEvil29` "Demon Spawn"; the rest are
  presumably the executable's defaults], else the owner's table (`todo 5.txt`,
  `stats::model::KARMA_TITLES`); Good and Very Good take the good column, Evil and Very
  Evil the evil one [hypothesis]. The reputation list (the other mode) is the hook for
  the reputation workstream.
- **Keys** [design]: the arrows choose CND, RAD, EFF (S, E, A, X are the hints' keys
  there, logged only; A also steps the tab); Up/Down and W/S select in a list, Page
  Up/Down scroll the description; the wheel scrolls the list, or the description when
  the pointer is on it; R switches General. The pointer selects the row it moves onto
  (not a row drawn under a pointer that has not moved) and clicks the buttons and arrows.
- **Checking it**: `NVF_PLAY_TEST_PIPBOY_STATS=<seconds>:<step>,...` with `sub=<0-2>`,
  `down`, `up`, `mode`, `desc=down|up`, and `perk=<FormID>` / `fame=<FormID>=<n>` to give
  the player something to show; each step and each selection is logged ("pipboy stats:
  Skills: Science 15 selected"), and a summary when a tab is drawn ("pipboy stats: LVL 1,
  HP 200/200, AP 80/80, XP 0/200; 7 S.P.E.C.I.A.L., 13 skills, …").
- **Open**: the game settings the page's words come from (`sStatsCrippled`,
  `sStatsNoEffects`, `sPCMenuHint*`) are not in the masters; when each face shows;
  whether the hints count 0 and when they are bright; whether worn enchantments should
  change the actor values themselves (then the page needs no sum of its own); the
  highlight box's rule; General's list height, order and right buffer, the reputation
  list's rule and second line, the karma titles. (The misc statistics' codes: misc-stats.md §1.)

### 5.1.1 STATS: S.P.E.C.I.A.L. from its file: what the code writes

The contract for the first slice of issue #1513 (`[UI] TileMenus=StatsMenu`,
`tiles/stats_menu.rs`): what code writes into the tree of `menus\main\stats_menu.xml` for page 1
to show. **Implemented, not verified**: every write below is our design and no finding about the
original, and no comparison with the original has closed it. Where runtime expressions place or show something, this note says only "the file works
this out at run time"; numbers appear only where they explain an incompatibility. Sources, as
tagged: the tree by `fnvtool ui tree` (values, tile names, template names); the file and its
includes by `fnvtool bsa extract`, read privately; and a private copy of those files laid over
Data with `--overlay` and read back with `fnvtool ui tree`, called "the overlay". What the overlay
shows is that authored overlay under **NVForever's evaluator**: it is no reading of the original
and does not check the evaluator. `NOGLOW_BRANCH` is empty.

**What the original was seen to do** [observed: private job pipboy_special_a, the original
running at 1280 x 720, the Pip-Boy open on this page; one character, one window size]:

- `screen()` is 1706.67 x 960, its converter 1.33 and the line thickness 2.67, which agrees at this
  one size with what the shared tree holds here (960 / window height). One size does not establish
  that rule in general; hud_menus' three sizes [observed: `tiles::resolution_converter`] are the HUD's.
- The list stands at x 35.33 and is 325 x 468. A row of 52 is derived from that height and nine
  visible rows [design: the arithmetic; one resolution], not read. The display stands at x 360.33, 544.67
  wide. The LVL card is at x 270, the XP card at x 690, and the tail line at y 652.
- The tabs stand at x 84.50, 220.50, 423, 591.50 and 746, y 626, 116, 179, 109, 107 and 133 wide,
  their parent line at y 652; their `_selected` read 0, 1, 0, 0, 0.
- The root's `user0` reads 0 on opening, then 1. A click on id 1 sets it by `ClickMenuButton`.
- The root's `user5` to `user7` read as text. `user8` reads empty on a level 1 character, while the
  LVL card shows on screen. **How `user8` came to be empty is open**. This slice's write of the
  model's level there is an explicit provisional choice behind the switch [design], so the card
  shows the level; it is not blanked to imitate that read. Parity on it stays open.
- A row's tile carries its AVIF name and id -1, read with `GetActiveUIComponentName` under the
  pointer. This slice gives its rows both, and knows them itself by actor value [design].

This is what was read, not yet compared with what this slice draws; other sizes are not covered.

Three different `user0`s are in play: the root's picks the page, the list's says a row is
marked, a row's shows its mark.

**The root** [observed: the vanilla files, by fnvtool bsa extract; the file's comments say
what each trait holds]:

| Trait | Code writes | From [design] |
|---|---|---|
| `StatsMenu/user0` | the tab, 0 to 4; here 1 | `PipBoy::tab` (`pipboy.rs`), the order of `Page::tabs` |
| `StatsMenu/user8` | the level, as text | `Snapshot::level` (`stats/model.rs`, filled in `stats/read.rs`) |
| `StatsMenu/user5` | HP, as text | `Snapshot::hp`, formatted as the hand-built page's card |
| `StatsMenu/user6` | AP, as text | `Snapshot::ap` |
| `StatsMenu/user7` | XP, as text | `Snapshot::xp` |

The numbering is the file's: the level is `user8`. The root's other code traits are Status's and
stay unwritten here. The root's `_PCButton_*` traits name buttons of Status and General, hidden on
this page, so they should not fire [hypothesis: menu-xml.md §3.2, the tile "must be shown and a
target"]. The page's keys stay code's: Up/Down, W/S, Page Up/Down in `stats::prepare`; A/D and
Left/Right in `pipboy::keys` [design].

**The list**, `stats_special_container`, a `list_box.xml` [observed: by fnvtool bsa extract: the
prefab's comments give `_highlight_y`, `_selected_height`, `_scroll_delta`, `_num_filtered` and
the scroll bar's `_number_of_items` to code, and say that with `_number_of_visible_items` set
code overwrites `height` and `_scroll_delta`]:

| Trait | Code writes | From [design] |
|---|---|---|
| `height` | the file's visible rows times a row's measured height. **Required**: the tree reads it empty, and the list has `clipwindow` on | the rows' `height`, below. Not the hand-built list's height: `list_rect` multiplies by a caller's row height, the hand-built caller's being the font's size + 20 (`Ctx::line_height`), and a tile's text measures its tallest glyph (`tiles::draw::measure`), a different figure |
| `user0` | true when any row has a mark | any `Row::mark` |
| `_highlight_y`, `_selected_height` | the selected row's `y` and height while it is boxed, else -1 and 0 | `StatsView::selected[1]`, `StatsView::highlight` |

The scroll bar's `_number_of_items`, `_scroll_delta` and `_num_filtered` stay the file's: nothing
scrolls with seven rows of the file's visible count. That a list which fits keeps its bar hidden
is a [hypothesis]: what the original writes to these on this page is not known, and what a
container list does says nothing certain about it. A row count that does not fit (a replacement's
taller rows, fewer visible rows) is not handled: list scrolling is not in this slice, so the page
falls back to the hand-built one [design].

**The rows**: template `stats_list_template` [observed: the vanilla files, by fnvtool ui tree,
`templates`], one per row under `stats_special_container`. Its hotrect `stats_list_template_item`
has the children `ListItemText`, `stats_list_template_number` and `stats_list_template_extra`
[observed: by fnvtool bsa extract]. Seven rows, `SPECIALS` in order: `Snapshot::special`
(`stats/read.rs`).

| Row trait | Code writes | From [design] |
|---|---|---|
| `listindex` | 0 to 6 | the row's place |
| `string` | the name | `Row::name` (AVIF FULL) |
| `user1` | the value as text, or the template's -1 for none | `Row::value` (`model::whole`). "--" without the scripts' game: a text value keeps it (`fnv_ui::Value::text`) though it compares as 0 (`Value::num`), so a numeric write would lose it |
| `user0`, `user2` | whether the mark shows, and the mark | `Row::mark` (`model::modified_mark`) |
| `height` | a height the template gives is kept; otherwise the text's measured height + the row's `_VerticalSpacing`, or 20 where that reads 0 [hypothesis: the prefab's comment; the derived 52 above agrees at one size] | the row's text as measured |
| `_y` | the height of the rows above | the same |

Everything else about a row, the list and the highlight box: the file works this out at run time.
The hand-built constants are not transplanted. The template gives a row no `id` and no `xup` or
`xdown` [observed: by fnvtool ui tree]; the running original's rows have the id and name above, and
how its code tells a row's click is not known. A tab's prefab has its
own hover sound and `pipboy/sounds`' `TabHovered` is ours: one must stay silent [design].

**The display**, `stats_display_rect`: place and size, the file works this out at run time.

| Tile and trait | Code writes | From [design] |
|---|---|---|
| `stats_icon/filename` | the picture | `Row::icon` (AVIF ICON) |
| `stats_icon/visible` | true while a row is selected. **Trap**: `stats_icon_separator` copies it and `stats_description_rect` copies that [observed: by fnvtool bsa extract], so a row without a picture must keep it true with an empty `filename`; the hand-built page draws line and text without a picture (`listed`) | `listed` in `stats/mod.rs` |
| `stats_description/string` | the text | `Row::description` (AVIF DESC) |
| `stats_description_scrollbar/_number_of_items` | one more than the scroll steps the tile's own text overflows by (font, wrap and window as the tile has them, not the hand-built page's pitch). **Required**: the prefab's value shows the bar on every row [observed: `visible` 1 in the tree and under the overlay]. The +1 is our adaptation, no capture of the original | a bound the tile view supplies through `StatsView::set_desc_bound`, as `listed` does its own |
| the same tile, `_current_value`, `_SetInCode` | `StatsView::desc`, then a poke of `_SetInCode`, which the prefab asks for when code sets the value | `StatsView::select` zeroes it on a new row |

`stats_icon_badge` stays hidden: what it shows on this page is not known; capture. The
description's value has one owner: the file adds its own arrow clicks and `wheelmoved` to
`_current_value`, and the model moves it too. So tile controls become `StatsView::scroll_desc`
commands, the value is written back after input on every tiled frame, and `wheelmoved` is not
written [design].

**A tab's inner rect**, `stb_text_box`, repeats the tab's own place, the tab's being read from it,
and has `alpha` 0 over words and lines of their own `alpha` [observed: by fnvtool ui tree; a tab has
no `locus`, the rect has]. So the two places are not added and the rect's `alpha` is not passed down
in the original, going by where its tabs stand [hypothesis]. `tiles/layout.rs` and the tile input
read no `locus` and multiply a rect's `alpha`, for every menu, so unchanged they lost the tab line
here, words and box. Writing either place collapses the other, standing every tab at the line's
start [observed: fnvtool's tree with a test write]. So this slice alone writes marks, the file's
places untouched: the tab gives its children no place (`_NVFNoOrigin`), and what is under the rect
keeps its own `alpha` (`_NVFOwnAlpha`) (`write_tabs`) [design]. Whether to read `locus` in general is
a separate decision. The selected tab's fill stays undrawn: a hotrect, whose `alpha` the file
changes there, draws nothing in this renderer [design].

**Ids** [observed: the vanilla files, by fnvtool ui tree, `id`]: `stats_tailline_status` 0,
`stats_tailline_SPECIAL` 1, `stats_tailline_skills` 2, `stats_tailline_perks` 3,
`stats_tailline_general` 4. Each tab's `_selected` asks whether the root's `user0` equals the same
number, and nothing in the file writes `user0`: a click on id 1 setting it was seen, above; the same
for the other ids is a [hypothesis]. The hand-built `PipBoyInput::Tab(i)` uses the same numbers (`tab_line` in
`frame.rs`). Also `stats_special_container` 6, `stats_icon` 25, `stats_description_rect` 26,
`stats_icon_badge` 52, `stats_title` 39, the cards 40 to 43; the scroll pieces are all `&generic;`.

**The title**: the `string` of `stats_title` is an entity without its closing `;`, which the
reader keeps as plain text (menu-xml.md §2.4) [observed: the vanilla files, by fnvtool ui tree].
The tile would show those characters, and neither the masters nor `BUILT_IN` have a setting of that
name. Code replaces them with `Page::title`, the hand-built page's own word, only while the string
still is such an unresolved name, and leaves a replacement's or a script's text alone [design: the
string rule, LEGAL.md: a word NVForever writes, none taken from the original's screen].
What the original does with it: not known; capture.

**What the page needs of `screen()`**, the question menu-xml.md §4.2 leaves open for the Pip-Boy.
Neither `stats_menu.xml` nor any file it includes reads `screen()`; only `globals.xml` does, for
`_line_thickness`: 2 on a PC, times `resolutionconverter`. The canvas comes from that file's
`_pipboy_width` and `_pipboy_height` [observed: by fnvtool bsa extract, every include searched].
With `--screen` at five sizes none of the tree's traits changes [observed: by fnvtool ui tree], but
the tool writes no converter, so that run cannot say which converter is right: it reads thickness
0 and lines without height. The renderer's target stays its own matter, and a 1024 x 768 canvas is
not written into `screen()`, because other menus share the tree [design].

- `tiles::interface_screen` is true with `StatsMenu` [design]: the file's lines need a converter,
  and the same gate lets a hotrect's `alpha` 0 pass (`Context` in `tiles/layout.rs`), so the list's
  rows draw. The gate is global, not Stats's own. With only the tile HUD on, adding `StatsMenu`
  changes the shared screen traits and the HUD's hotrect-alpha handling, as the other T15 menus do;
  with another T15 switch already on, adding it changes no gate. This effect is tested and stated,
  not isolated.
- 960 / window height (`resolution_converter`) is what the shared tree holds, and the original
  reads the same with the Pip-Boy open, above. So lines differ from the hand-built `THICK` of 2
  behind the switch.

**Hand-built value, tile target, what must stay** [design: values come from `Snapshot` and
`StatsView`; a writer works no figure out again]:

| Hand-built | Tile target | Stays? |
|---|---|---|
| A value with worn apparel's changes, kept in range (`shown_and_permanent`, `av::capped`) | row `user1` | yes (§5.1) |
| "(+)" and "(-)" (`modified_mark`) | row `user0`, `user2`; list `user0` | yes |
| A row always selected, boxed only after a key or under the pointer (`StatsView::highlight`, fitted to the owner's screenshots) | `_highlight_y`, `_selected_height`; the display always filled | yes |
| Screen menu: no title, the line running on from the lead line, the page names as a header [design: the owner, 2026-10-31, `heading_title` and `header` in `frame.rs`] | hide `stats_title`; write `x` and `_length` of `stats_headline_h2` to close the gap; the header stays the shell's, clickable, no tile | yes `[owner 2026-10-31]` |
| The page's background and scanlines, on the device; neither in the screen menu (`device_screen.rs`, screen-menu.md) | none: `background_layout` stays hidden in both frames, so the tiles are page content only; the file has no scanline tile, so they stay the shell's, with the scanlines over the tiles | yes |
| Keys, wheel; A/D [design: `pipboy::keys`]; the controller's D-pad [design: owner 2026-11-02] | none: `stats`, `pipboy.rs` | yes `[owner 2026-11-02]` |
| The words "S.P.E.C.I.A.L." and "STATS" (`Page::tabs`, `Page::title`) | the tab reads `sStatsSpecial` (`fnv_ui::strings::BUILT_IN`'s "S.P.E.C.I.A.L." unless a plugin sets it, the name as FalloutNV.esm's own text spells it), left alone: code does not overwrite a plugin's or the built-in label | yes `[owner 2026-11-06, #1740]`: both pages read "S.P.E.C.I.A.L." |
| Tabs, cards, row pitch (`tab_boxes`, `heading`, `card`, `list`) | the file works these out at run time | not a choice; may differ behind the switch. The places read above are the original's; the comparison at 1280 x 720 lists the differences |
| The log lines and `NVF_PLAY_TEST_PIPBOY_STATS` | none | yes: the smoke scenarios read them; `stats::prepare` keeps them whichever view draws |

**Not known; capture**: how `user8` came to be empty, and where the LVL card's value then comes
from; a row's `_selected`; the list's `_scroll_delta` and scroll bar count; `stats_icon_badge`; the
description scroll bar's count; whether the other tabs' ids act as id 1's does; whether a tab's
words and the title show at all before code writes them; any of the above at other window sizes.
None of these stops a first working slice with a complete fallback; they cannot be called verified
when parity is declared.

### 5.3 DATA

`data.rs` (the page: state, input, what is shown and its drawing), `data/model.rs` (plain
rules, tested), `data/read.rs` (records). Sources: `menus\main\map_menu.xml` (places, sizes,
fonts) and the owner's five DATA screenshots (§1), compared at 1280 × 720 with
`NVF_PLAY_TEST_SHOTS` runs; px below are those screenshots' pixels, about 0.62 to a unit.

- **Heading** [documented: map_menu.xml]: the title 70 in after a line of 50; a line from
  180, 470 long; the place right-aligned at 630 and the date and time at 845, both 20 down
  in font 2; lines fading down at 650 and at the end, a line from 660 to the end.
  - The date: month, day, the year's last two digits, then the time, "10.25.81, 9:29"
    [observed: the screenshots, on 25 October 2281]; no leading zeros and a 24-hour clock
    as the wait menu's [hypothesis, wait-menu.md §4].
  - The place: indoors the cell's name [hypothesis]; outdoors the map name (`REGN RDMP`)
    of the regions the player's cell is in (`CELL XCLR`), the highest priority first, else
    the worldspace's name [hypothesis: the screenshots show "Goodsprings", the map name
    of `VMapGoodspringsRegion` (observed)].
  - Right-aligned texts of the original end about 5 px further right than ours (the time,
    "Show Location", "Place Marker") [observed]; not corrected.
- **Lists** (Quests, Misc, Radio) [documented: map_menu.xml, list_box.xml]: 15 left of the
  menus, 75 down, 400 × 468, the scroll bar of 32 on the left; a row's square of 25 at 15,
  20 and its text 40 in and 20 down in font 2, broken at 310. A row is 54 high and a text's
  further lines 34 lower [observed: fitted; rows 33.5 px apart, two-line rows 54.5 px];
  the squares start at the same pixel as the original's [observed]. The selection box
  starts 9 left of the row and ends 5 short of the list's edge [observed: the Radio
  screenshot boxes from 448 px; documented: the 5]. The box shows while the pointer is
  on a row or after Up/Down (W/S); a new tab starts without it [design: the screenshots
  show it only under the pointer]. Hover selects, a click or Enter acts. The wheel scrolls
  the list. The scroll bar: the file's arrows (32 × 16) at the ends and a line for the part
  shown [hypothesis: the marker's look].
- **Buttons at the right** [documented: map_menu.xml, text_box.xml]: right-aligned at
  855, from 75 down, each its font-2 text with 20 across and 15 up and down, the text 10
  under the middle of the spare height, a PC key's label 5 after the box. Their words are
  game settings the masters do not hold (§1): "Place Marker", "Show Location", "Show
  Active Quest Notes", "Challenges" with "R)" [observed]; "Show All Notes" [hypothesis:
  the setting `sShowAllNotes`]. The hover box is not drawn.
- **Quests**: the quests with a name and an objective shown or done (quest-messages.md §5);
  running quests before those done (completed or failed), each part newest first, quests
  first seen together by FormID from the highest [hypothesis: fits the screenshot's
  order, the add-ons' quests (01…–04…) on top, "Back in the Saddle" (0010A214) before
  "They Went That-a-Way" (000842DD), the completed "Ain't That a Kick in the Head" last].
  "Newest" is when NVForever first saw the quest listed (looked at four times a second;
  not saved). Done quests are drawn at half strength [hypothesis: greyed on the
  screenshot]. The active quest's square is filled; a click or Enter on a running quest
  makes it active [hypothesis: the PC's form of "Make Active Quest"]; until the player
  picks one, the newest running quest is active [design]. When the page comes up the
  active quest is selected [hypothesis: the screenshot shows its objectives with the
  pointer elsewhere].
  - Objectives at the right: 400 wide at the menus' right edge, under the buttons' place
    (75 + 120) less a button's height, the scroll bar on the right [documented]; each
    behind a square, filled and at half strength when done [hypothesis]; those to do
    first, each part from the latest objective down [hypothesis].
  - "Show Location" shows the World Map [design: the quest's place is not centred].
- **Misc**: the player's notes (AddNote; `NOTE` records) by name [hypothesis]; "(6 of 6)"
  is part of the note's name [observed: `VMQ01CourierNote` FULL "Mojave Express Delivery
  Order (6 of 6)"]. A text note's text is `TNAM`, a NUL-ended string [observed; the schema
  does not read `TNAM` of `NOTE`]: font 1, broken at 400, in a window 17 left of the item
  data, 20 down, 400 × 385, scrolled 20 a wheel step [documented]; Page Up/Down scroll a
  window [design]; arrows at its right when it is longer [documented]. The window reaches
  10 over the first line so its letters are not cut [design]. Our lines break exactly as
  the screenshot's [observed]. An image note shows its picture (`XNAM`) 360 square at the
  right [documented]; sound and voice notes (holotapes) play when used (a click or
  Enter; again to pause, again to go on) and show the audio meter (time remaining and a
  marker moving along the tick meter)
  ([holotapes](holotapes.md) §3). "Show Active Quest Notes" keeps the notes whose quest (`ONAM`) is the active one.
- **Radio**: the stations the Pip-Boy receives where the player is, by name (`crate::radio`,
  radio.md); the one playing has its square filled; a click or Enter tunes to a station,
  or turns the radio off on the one playing [hypothesis: "Tune"]. The waveform panel
  [documented]: 500, 125, 350 square, the tick meters along its foot and right side, the
  flat line (2 high, 5 short, 8 over the middle) while nothing plays [observed: the
  screenshot, the radio off]. While one plays the original's trace moves [observed: a
  60 fps capture of the Radio tab, 2026-10-27]: about 8 waves across the panel stand
  nearly still while their envelope runs to the left, some 7 panel widths a second;
  its height follows the sound (a steady tone draws even bursts with flat gaps, music
  peaks of mixed size, a quiet stretch small ripples), up to about a third of the panel
  either side of the line. Here the waves are two sines drawn under an envelope taken
  from the playing file's loudness (root mean square over 2048 samples) with a slow
  rise and fall of its own [design: whether the original draws the sound itself or a
  model is not known].
- **World Map**: the worldspace's picture `WRLD ICON`
  (`interface\worldmap\wasteland_nv_1024_no_map.dds`), drawn 2048 square [documented:
  `filewidth` 2048; `MNAM` usable 2048] at alpha 128 [documented]; the menu file names
  `wasteland_1024_no_map.dds`, which the plugin does not use [observed]. Where cells fall on it: the cells from `MNAM`'s north-west cell to its
  south-east cell (−35, 35 to 25, −25: 61 a side) fill the middle 0.7876 of the picture
  from (0.108, 0.102) [observed: fitted to Goodsprings as the screenshot shows its roads,
  the arc of Hoover Dam and the ring of Jacobstown, 309.8 units a texel, within 4 texels
  each; corner to corner on the whole picture put Goodsprings 11,000 units off its
  road]. The menu file keeps the picture's edges at most to the window's middle; it opens
  centred on the player [hypothesis: the owner's view was elsewhere]; dragged it pans,
  the wheel zooms 1.25 a step from 1 to 4 times [design].
  - Map markers: references of the `MapMarker` STAT (0x10) with `XMRK`; name `FULL`, type
    `TNAM` (0 None … 14 Vault [documented: xEdit; observed: vaults 14, a cave 5, Camp
    McCarran 8, Hoover Dam 7]), flags `FNAM`, radius `XRDS`. Shown when the player has been
    within the radius (`XRDS`, noted four times a second, not saved) with its type's
    icon, or when ShowMap or the Visible flag made it known, with the undiscovered icon
    [hypothesis]. Icons `interface\icons\world map\icon_map_<type>.dds`, the inverted form
    under the highlight box [hypothesis: by the files' names]; 24 wide, 1.75 times and
    brightness 255 (else 220) under the box [documented: the template's 1.75 and
    brightness; observed: 24 fitted].
  - The player's arrow: `glow_cursor.dds` [documented], 40 wide, 1.28 times as high,
    turned by the heading [observed: fitted, the screenshot's triangle 25 px; hypothesis:
    the turn's sense]. Drawn over the markers and under the highlighted one [documented:
    depths 1, 7, 11].
  - Quest markers: the targets (`QSTA`) of the active quest's objectives shown and not done,
    where they are now; a target indoors at the door outdoors into its interior
    [hypothesis: the screenshots' quest marker for Sunny Smiles stands by the saloon's
    door]; the picture is the HUD compass's objective marker
    `glow_hud_compass_objective_marker.dds` [hypothesis: the shape on the screenshots],
    56 wide [observed: fitted], 1.25 times as high, 15 right and 5 down [documented]. The
    marker is for the targets whose conditions pass [documented: Quest Objectives Tab],
    evaluated four times a second [hypothesis]; a
    target indoors is marked at a door into it that is not locked unless the target ignores
    locks [hypothesis] (quests-dialogue.md §2, target conditions).
  - The highlight box [documented]: 160 square, corners of 40 with 15-long fading tips, a
    dot of 8 (`waypoint_node.dds`) at its middle, the name in font 1 centred 180 down. It
    follows the pointer over the map and takes a marker's place and name within 40
    [hypothesis: the screenshots show it away from the middle, once round a door with
    the door's place name]; at first it is at the window's middle. A World Map marker
    with a reputation (REFR `WMI1`, 36 in FalloutNV.esm [observed]) shows, on the line
    under its name, the reputation's name and the player's standing title with it,
    "Goodsprings: Neutral" ("The reputation that is shown when the player hovers over the
    icon on the world map" [documented: GECK wiki Reference, Map Marker]; the wording and
    the second line are a hypothesis, owner-review.md).
  - A click on a marker (the button pressed and let go without the pointer moving
    more than 6 units [design]) asks whether to travel there: fast travel,
    `fast-travel.md`.
- **Local Map**: the same window, markers, arrow, quest markers (kept inside the window
  [documented]) and highlight box, on an empty map at 28 game units to a unit [observed:
  fitted, the gas station's door and the saloon's door 168 px apart on the screenshot; the
  zoom then is not known]. Doors leading elsewhere (`XTEL`) in the 7 × 5 cells round the
  view (or the interior) show `interface\icons\local map\icon_local_door.dds`, named by the
  place their destination door stands in [observed: the screenshot's door is named
  "Goodsprings Gas Station", the cell `GSGasStation`]. Left out: a door reference flagged
  Hidden From Local Map (header bit 6, "The reference will not be displayed on the local
  map" [documented: GECK wiki Reference]; 4 teleport doors, all in Dead Money
  [observed]), a door whose base DOOR is flagged Hidden (`FNAM` bit 2 [documented: xEdit];
  3 in FalloutNV.esm, all secret doors, none a teleport door [observed]; what the flag does
  is a hypothesis, owner-review.md), and a door disabled now [hypothesis, as a disabled
  marker]. Under them an outline of the place
  (`data/outline.rs`) [design: the original renders the area from above, its screenshot
  dark save a few shapes; the owner asked for an outline]: a grid of rays straight down
  through the physics world's collision (not actors, triggers or loose clutter), 96
  units apart outdoors from 8000 over the feet, 28 apart indoors from 150 over the feet
  (under the ceiling), cast a few hundred a frame (6 ms) and made into one picture when
  done; outdoors the ground shaded by its slope lit from the north-west, indoors the
  floor (a ray landing within 120 below and 40 above the feet) filled faintly, what
  stands on it brighter, and the floor's edge to nothing (the walls) or a step of 64
  down to a neighbour drawn bright on the higher side; outdoors a step of 192 (walls,
  cliffs, not the hills' slopes at that spacing). The grid covers the window at a
  magnification of 1 and 15% more round its anchor, kept while a pan stays within a
  quarter of it, made again for another floor (128 up or down) or place. Indoors the
  map is 8 units to a canvas unit (outdoors 28) [design]. What the physics world has not
  loaded stays empty. Not built: which doors and markers the original shows (its
  screenshot shows only the gas station's door), Place Marker, fog of the unexplored.
- **Dragging** (both maps): the left button held down on the map and the pointer moved
  pans it [design]; the pointer's own moves are followed, not Bevy's drag events, which
  ended at once because the map's nodes are drawn again as the highlight box follows the
  pointer (the owner's list 5, item 52). Test aid: `drag=<dx>:<dy>`.
- **On a controller** [observed: the original] the right stick moves the highlight box
  and, pushed past an edge, pans the map; the left stick's up and down zooms in and out
  smoothly while held (the cursor and zoom speeds are design); see controller.md §3.
- Checking it: as §7 with `page=data` and `tab=<0-4>`; `key=down|up|use` steps move and
  act on the DATA lists too. A run with a note, quests and the radio on:
  `NVF_PLAY_TEST_SCRIPT=3:info:0x001057E8:DocMitchellREF,4:start:VCG02,4.5:objective:VCG02:3,5:objective_done:VCG02:3,5.1:objective:VCG02:5`
  (Doc Mitchell's line whose result script adds the courier's note) and
  `NVF_PLAY_TEST_RADIO=5:next`. Log: "pipboy data: N map markers", "map marker … found",
  "local map: N doors leading elsewhere", "active quest …", "radio tuned to …".
- Open: the quest list's order and the active quest's default; the objectives' order and
  how done ones look; the hollow square's size (ours draws smaller than the original's,
  about 9 px against 14); which doors and markers the local map shows and its zoom;
  where the World Map opens; how the original turns the arrow; the radio's waveform model;
  Place Marker, Challenges, the audio notes' panel, the buttons' hover boxes.

## 6. Keys and pointer [hypothesis unless marked]

- Tab opens and closes; Esc closes. F1, F2, F3 show STATS, ITEMS, DATA; A/D and
  Left/Right the tabs [design]. On the controller LT/RT step the pages (round again)
  and the D-pad's left/right the tabs [design: owner 2026-11-02; controller.md §3]. ITEMS: Up/Down (W/S), Page Up/Down, the wheel, Enter or
  Space to equip, unequip or use [design]. A right click on a row drops one (a stack asks
  how many), Shift + right click the stack; it never equips or uses [design: owner
  2026-11-02, #1393]. X opens the mod menu on a weapon with mod slots and does nothing
  else (weapon-mods.md §4); the keyboard has no drop key. On the controller X drops one,
  the original's `IM_DropButton` button (`&xbuttonx;` [documented: inventory_menu.xml]):
  the PC's X for drop came from mixing up the controller's X with the keyboard's.
- The pointer is followed onto the model: on the screen, it is passed to the menus as
  a pointer of their own (Bevy's picking, as its `render_ui_to_texture` example does); a
  click on a button on the device shows its page. A tab and a row take clicks; the
  pointer on a row selects it, a click on the selected row acts on it.
- The pointer is found on the shapes as the arm's bones pose them, from CPU copies of
  the screen's and the buttons' meshes (nvforever `pipboy_arm.rs`, `Posed`) [design].
  Until 2026-10-11 Bevy's mesh ray cast was used, which sees neither the skinning nor a
  mesh kept on the GPU only, so no click on the model did anything (the owner's
  playtest). The pointer on a button does not light it (§2); a click still shows its
  page. `show_screen` logs "pipboy: buttons lit: <pages>" on each change.

## 6.1 Sounds [observed: the SOUN records, FalloutNV.esm; the owner's list 5]

At the listener: another tab `UIPipBoyTab` (`fx\ui\pipboy\ui_pipboy_tab.wav`); another
page (the device's buttons, F1–F3) `UIPipBoyMode` (`ui_pipboy_mode.wav`); the pointer
onto another tab of the tab line `UIMenuFocus` (`fx\ui\menu\ui_menu_focus.wav`), that
tab then boxed without a fill [hypothesis: the look]; the ITEMS rows sound as before
(`UIPipBoyScroll`, the same file); the Pip-Boy light (`crate::flashlight`)
`UIPipBoyLightOn` / `UIPipBoyLightOff`; while up, a static crackle `UIPipBoyStatic` (the
folder `fx\ui\pipboy\static\`, a file at random) 0.5–2 s after it is up and then every
4–10 s [hypothesis: how often the original plays them is not captured]. Coming up and
going down keep `UIPipBoyAccessUp` / `UIPipBoyAccessDown` (`crate::ui_sounds`). Each is
logged "pipboy sound: <EditorID>" (the focus sound at debug level).

## 7. Checking it

`NVF_PLAY_TEST_PIPBOY=<seconds>:<step>,...` with steps `open`, `close`,
`page=stats|items|data`, `tab=<0-4>`, `key=down|up|use|drop`, and for the pointer
`point=<x>:<y>` (the pointer on that canvas point, through the model's screen),
`point=off`, `click=<x>:<y>` and `click=stats|items|data` (a click on the device's
button; logged "pipboy: the STATS button pressed"), `point=stats|items|data` (the pointer
on that button, no click; logged "test aid: the pointer on the STATS button"); screenshots with
`NVF_PLAY_TEST_SHOTS`. A new game's add-on message boxes come first: answer them with
`NVF_PLAY_TEST_SCRIPT_MENUS=0.05` and open after about 12 s:

```
NVF_PLAY_TEST_SCRIPT_MENUS=0.05 NVF_PLAY_TEST_PIPBOY="12:open,15:page=stats,17:close" NVF_PLAY_TEST_SHOTS="14,16.5" nvforever --data <Data> --cell Goodsprings --play --no-voice --hour 12 --size 1280x720 --run-seconds 19
```

Expect "pipboy: up, page ITEMS tab Weapons", "pipboy: the menus are on the model's
screen", "pipboy: the glare is over the menus", "pipboy: screen treatment on (…)",
"pipboy: STATS Status", "pipboy: down". Compare at 1280 × 720 with the owner's
pictures.

The mouse (2026-10-11): `NVF_PLAY_TEST_PIPBOY="12:open,14:click=320:650,16:click=stats,18:click=600:650"`
gives "pipboy: ITEMS Apparel" (the second tab), "pipboy: the STATS button pressed",
"pipboy: STATS Status" and "pipboy: STATS Perks"; `point=200:330` on ITEMS boxes the
fourth row, and a click on a row equips what it selects.

## 8. Open

[observed 2026-11-07, commit `4fbe5750d`, 8 alternating fresh launches, one machine] 4/4 clean first openings enabled and 4/4 blue or partial with `NVF_PLAY_TEST_NO_PIPBOY_WARM=1`. Screen-down medians 7.68 s enabled (7.38-7.98) and 6.81 s disabled (6.41-6.96): about +0.9 s in these launches. They support the combined fix.

[observed: 2026-11-07, commit `8fa3f8a04`, superseded by the set above] An earlier set of
eight alternating launches gave the same 4/4 and 4/4, with noisier screen-down medians
(8.84 s enabled, 8.27 s disabled).

[observed: earlier supplied `target/smoke/pipboy-first-open.log` and shots, 2026-11-07]
The failed first attempt reached its three-second startup cap and asked zero new
pipelines during the raise, yet its first three shots were blue or partial. Its
single enabled/disabled pair took 12.98/10.79 seconds on a busy machine; that pair
was not a repeatable startup-cost estimate.

[design: Bevy 0.20.1 registry `bevy_render/src/camera.rs:910-1028`,
`bevy_pbr/src/material.rs`, `prepass/mod.rs:1086,1376`] Pipeline compilation and
per-entity phase submission are different: a cached PSO can be ready while an entity
has no colour entry yet. Upstream's main pass skips an entity whose uploaded mesh or
allocator slabs are not there yet without keeping it for a retry. An earlier revision
of this fix patched both branches to retry and dropped pending entries no longer
visible; hidden and raising rig meshes also mark themselves changed so retained bins
are refreshed after visibility/material changes.

[observed: 2026-11-07, `pipboy-first-open`, `-no-warm`, `pipboy-screen-menu`,
`-unworn`, one machine, `bevy_pbr` rebuilt clean] With those retries and the visibility
filter reverted (the probe kept), all four scenarios pass, the first raise held 61 ms
for 0 pipelines and the first-open shots show the textured device with no blue or
world-filled hole, so the retry patch was dropped. The warm-up and the first raise's
own readiness wait are what the fix rests on.

[hypothesis] Depth without colour came from the rig's main entries being missing or
stale on the first raise. Shader depth rejection, incorrect GPU instance data, or a
draw-command skip remain possible if a report shows complete submission/bindings
while a shot is blue.

[design: `engine/nvforever/src/pipboy_arm.rs`, `pipboy_ready.rs`] The fix normally
waits for the observable submission conditions, with a bounded failure release;
those conditions are not proof of correct pixels.

[design: `engine/nvforever/src/player_body.rs:1441,2189`,
`engine/nvforever/src/pipboy_arm.rs::copy_of`, `::wear_materials`;
`engine/nvf-assets/src/nif_spawn.rs:720,779`, `texture.rs:312`, `model.rs:1260`]
Hiding the arms only changes visibility. Model construction requests its textures even
when hidden; worker textures are inserted independently of visibility. The first-person
material copies are made with the rig. Menu/glare copies are added on first substitution,
then `Swapped.menus`/`glass` retain strong source, image and copied-material handles
across teardown. An unchanged source and target image reuse the same asset IDs at the
real raise. A recreated screen image or rebuilt rig does make new copies. No extra
keep-alive cache is needed. Pip-Boy page pictures are a separate on-demand path
(`nvf-play/src/pipboy/frame.rs`, `device_screen.rs::draw`); drawing STATS warms that
path without opening an inventory.

[design: Bevy 0.20.1 registry `bevy_render/src/material_bind_groups.rs:2762-2840`,
`erased_render_asset.rs:372-451`; `engine/vendor/bevy_pbr/src/material.rs:598,1658`;
`engine/nvforever/src/world.rs:651-694`] Material preparation retries when an image is
not yet a GPU asset. NifSurface uses bindless allocation when supported; both passes use
the same prepared material and allocation, and the colour command additionally needs a
live bind group. Allocator creation itself has no multi-second timer. The upload budget
is 24 MiB/frame in play and unlimited behind the loading cover; materials have no byte
size budget, but their images and meshes do. Thus image preparation can delay a material,
without explaining this run's depth/colour asymmetry on its own. Readiness now checks
prepared material, actual allocation and main queue, rather than assuming a ready PSO
proves them.

[design: `engine/nvforever/src/pipboy_arm.rs`, `pipboy_ready.rs`, `player_body.rs`]
The start borrows the real rig and image camera for four render reports, with a 250 ms
wall-clock budget plus teardown. It does not wait for the global queue to go quiet.
The existing loading gate waits for the pipelines already requested. If the rig could
not be submitted in that budget it remains cold. The first real device raise verifies
submission again, behind the camera with CPU culling disabled, even following a start
warm-up. It releases only after its own main/prepass pipelines and material bindings
are ready, or releases at a three-second wall-clock cap with a warning that readiness
is incomplete. The cap allows cold preparation while bounding failure from a shader,
upload or lost rig; scheduling and pose restoration can add a frame. A released raise
does not hold again during that opening. Tab/Escape dispatch Close through the normal
key handler during the hold; cancellation skips the lowering clip. Later opens reuse
the prepared rig; a new rig/material resets readiness.
A missing rig keeps the existing flat-image fallback.
A screen-menu warm-up draws only its page/image camera; it neither borrows the arm nor
creates device treatment. Only the startup warm-up stays script-silent: a held real
opening reports its remembered
STATS/ITEMS/DATA page immediately. The device access sound waits until the borrowed
pose has been restored and the visible raise begins; ordinary openings, screen menus
and the disabled-warm path keep their opening-frame sound. A cancelled hidden opening
has no access sound.
The treatment's visible log is independent of when its layers were prepared.

[design: `tools/smoke/scenarios.toml`, `engine/nvforever/src/pipboy_ready.rs`]
Run `pipboy-first-open` and `pipboy-first-open-no-warm`, then the unchanged
`pipboy-screen-menu` and `pipboy-screen-menu-unworn`. `NVF_PLAY_TEST_PIPBOY_DRAW=1`
installs the logging system only for the aid. Its first visible render report is
marked only after pose restoration and gives
`shapes`, actual colour `queued`/`opaque`, `prepared`, `bound`, `ready`, `missing`, `gpu_meshes`, `slabs` and `specialized`.
`missing=0` with a nonzero shape count means every watched shape has a main entry,
an uploaded mesh and allocator slabs, a GPU material bind group and compiled
main/prepass PSOs. `queued < shapes` isolates
submission; `prepared < shapes` isolates material/image preparation; `bound < prepared`
is material allocation/binding. `gpu_meshes < shapes` or `slabs < shapes`
identifies mesh upload/allocation; `specialized < shapes` identifies a missing main
specialisation. With those prerequisites complete, `ready < bound` indicates compilation.
`missing=0` with a wrong shot rules out those prerequisites, and requires draw/GPU/depth
investigation. The log cannot certify pixels. The own-rig pipeline IDs come from its
actual queued main entries and prepass cache, excluding the world's. IDs are local to
each run. Both scenarios enable shader definitions and the submission report.

[design: `engine/nvforever/src/pipboy_ready.rs`, `engine/nvf-play/src/pipboy.rs`,
`ui_sounds.rs`, `tools/smoke/scenarios.toml`]
`pipboy-first-open` asserts that the first report after pose restoration has every
shape prepared, bound and uploaded (shapes still out of view aren't queued yet), that a
later report has at least 15 queued and all of those ready, and forbids the first-raise
cap warning. If the cap ever releases, a partial rig can show; that's the chosen
trade-off over a locked player.
Its shots should show a textured arm/device without blue, a world-filled hole or blur
on the arm; during a hold the world stays unobstructed. The disabled twin remains a
negative control. Unit checks cover Tab/Escape dispatch during a hold, bounded failure
release, remembered page reporting, pose cleanup, and access-sound deferral/cancellation.
Cold New Game, save load, screen menu, no device, interruption combinations and 16:9/4:3
still need runtime checks after this review revision.

- Whether the original holds the clip at `Hit` (the measurements say the screen faces
  the eye there).
- The keys of the PC game for the pages and tabs.
- The needle's and knobs' speeds in time (the capture counts game loops at no known rate),
  the wheel's step and sign, whether the mouse wheel turns it off the top of a list, how the
  knob swings back at the wrap, and whether anything moves down (§2).
- The glass: the static treatment (§3) is fitted at noon, level view, one page. Whether
  the original's screen moves with the hour, the pose and the page beyond what the grade
  does here is not fitted.
- `fDefault1stPersonFOV=55` for the arms outside the Pip-Boy.
- What `fBlurIntensityPipboy`, `fScanlineScalePipboy` and `fBrightenPipboy` scale.
- The mouse pointer: the screenshots show an amber arrow (`interface\icons\misc\
  glow_cursor.dds` is in the archives); NVForever now shows the game's own `cursor.dds`
  tinted amber everywhere in the window (`engine/nvf-play/src/cursor.rs`, 2026-10-16).
