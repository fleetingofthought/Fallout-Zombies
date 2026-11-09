# Spec note: the interface's fonts (`.fnt`)

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. Implementation:
`crates/fnv-font`.

## 1. Sources

- **The nine font files** of the base archive `Fallout - Textures2.bsa`
  (`textures\fonts\*.fnt`) and their textures (`textures\fonts\*_0_lod_a.dds`), read
  with `fnvtool bsa extract` and looked at as numbers and as pictures. Nothing of them
  is shipped.
- **`Fallout_default.ini`** in the game folder: the lines `sFontFile_1` to
  `sFontFile_9` name the font file of each font number.
- **The owner's screenshot** of a message box in the original game (2026-10-11, kept
  privately), against which lines set with these numbers were measured
  (`message-menu.md` §3).
- No description of the format was used. Every field meaning below is a reading of the
  numbers, checked as said.

## 2. The file

Every one of the nine files is 14 632 bytes [observed]. Numbers are little-endian.

| Bytes | What | Tag |
|---|---|---|
| 0 to 3 | a float: 31 for `monofonto_large`, 34 for `glow_monofonto_vl_dialogs`, 25 to 52 in the others. The height of one line of text: a one-line button's row in the screenshot is this and the interface file's 20 | [hypothesis, one font checked] |
| 4 to 7 | an integer, 1 in all nine. The number of textures | [hypothesis] |
| 8 to 11 | an integer, 1 in all nine | unknown |
| 12 to 295 | the texture's name, ended by a zero byte, the rest zero: the texture file's name without folder and extension (`Monofonto_Large_0_Lod_A` for `textures\fonts\monofonto_large_0_lod_a.dds`) | [observed] |
| 296 to the end | 256 glyphs of 56 bytes, one per character code | [observed: 256 x 56 is what is left] |

The parser refuses a file of another size, or one whose second number is not 1: where a
second texture's name would stand is not known.

A glyph is 14 floats [observed]:

| Number | What | Tag |
|---|---|---|
| 0 | zero in every glyph of all nine fonts | unknown |
| 1 to 8 | four corners of the glyph's picture on the texture, each a pair from 0 to 1 across and down: top left, top right, bottom left, bottom right | [observed: cutting these rectangles out of the texture gives the letters] |
| 9, 10 | the picture's width and height; equal to the rectangle's size in texels | [observed] |
| 11 | room before the picture. Zero in eight fonts; 0 to 3 in `fixedsys_comp_uniform_width` | [hypothesis, not checked on a picture] |
| 12 | room after the picture. The space has no picture and only this number (7 in `monofonto_large`). In the fonts whose letters have a glow around them it is negative (-3 in `glow_monofonto_vl_dialogs`), so the glows of neighbours overlap | [observed, §3] |
| 13 | how far the picture's top is above the line the letters stand on: 24 for a capital, 17 for a small letter without ascender in `monofonto_large`, where "g" is 25 high and so reaches 8 under the line | [observed: letters set by it stand on one line] |

The codes are those of the game's text, Windows-1252 [hypothesis: the letters at the
codes of plain English text are right; the accented ones were not checked]. About 113
codes have no picture in each font.

### 2.1 Localized installs: the code page (#1439)

Most of this section was written before any localized install was looked at; what one
Russian install showed is in "Seen in a Russian install" below.

- **What is assumed.** A localized release keeps the 256-glyph file of §2 and changes
  two things: the code page of the 8-bit text in its plugins (and its menu files, credits
  and terminal word list), and the glyphs its font files and textures carry at those codes
  (Windows-1251 for Russian, Windows-1250 for Polish and Czech). So a Cyrillic letter
  is drawn with the glyph at its 1251 code [hypothesis: no localized `.fnt`, texture
  or plugin looked at].
- **What NVForever does.** All the game's 8-bit text (plugin strings, script strings,
  archive and model names, settings, menu files that are not UTF-8, credits, the
  terminal word list) is read in one code page per run, and a character is set with the
  glyph at its code in that code page (`fnv_util::text`, `fnv_font::code_of`). Typed
  text (console, character name) takes only the characters that code page has. Archive
  and model names use the same code page as the plugins, so a path in a plugin still
  finds its file [hypothesis: that a localized install's file names use the plugins'
  code page; every vanilla English name is ASCII, the same in all three code pages].
- **How it is chosen.** `NVF_CODE_PAGE`, else `[Text] CodePage=` in NVForever's own
  `NVForever.ini` (1252, 1251 or 1250), else read from the plugins' text, else
  Windows-1252 (the English install's, the release this engine targets) [hypothesis].
  No setting of the install names it: a Russian install's `Fallout_default.ini` says
  `sLanguage=ENGLISH` [observed: the owner's Russian install, 2026-11-08]. So the text
  of the plugins loaded is sampled (their names and dialogue lines, `FULL` and `NAM1`,
  `fnv_esm::text_guess`, #1443): a plugin whose letters are mostly bytes 0xC0 to 0xFF
  is Cyrillic in Windows-1251, where those bytes are the Cyrillic letters [documented:
  WHATWG windows-1251 index], and Western European text has only its few accented
  letters there; 1251 is chosen when such plugins hold more of those bytes than the
  others [design; hypothesis: one Russian install looked at]. Windows-1250 is not told
  apart from 1252 this way and is still set by hand. The start's log says when the
  plugins chose it. The game's language plugins (`<master>_lang.esp`) are turned on in
  NVForever's plugin list when it first lists them (`fnv_install::plugins`).
- **Every code page here maps all 256 bytes** to a character (the WHATWG tables in
  `encoding_rs`; an undefined slot becomes the C1 control of the same value), so text read
  and written again keeps its bytes [documented: WHATWG Encoding Standard, the
  windows-1250/1251/1252 indexes].

**Seen in a Russian install** [observed: the owner's Russian install, 2026-11-08, read
with `fnvtool` and looked at as pictures; one install, not a Steam download]:

- `FalloutNV.esm` and the add-ons' masters carry the English text; the Russian text is in
  one more plugin per master, `<master>_lang.esp` (master: that `.esm` alone), whose
  records replace the masters' names and lines. Its strings read as Russian in
  Windows-1251 and as accented Latin letters in Windows-1252.
- Its fonts are loose files in `Data\Textures\fonts`, under the English files' names
  (`Fallout_default.ini` names the English files). Each `.fnt` is 14 632 bytes and
  differs from the English one; its texture name is the English one.
- Next to each font's `.dds` is a `.tex` of the same name (§2.2). The `.dds` is
  byte-identical to the English archive's: Latin and Western European letters only. The
  `.tex` holds Cyrillic letters, and they are where the `.fnt` glyph table says. So the
  letters to draw are in the `.tex`.

Still to check: whether file names in its archives use high bytes; which setting names
the language.

The texture of `monofonto_large` is 256 x 256, 16 bits a texel with 4 of them alpha
[observed]; the letters are in the alpha, the colour is a light grey.

All eight font textures of `Fallout - Textures2.bsa` are A4R4G4B4 with one level, no
smaller ones: 256 x 256 for `monofonto_large`, `glow_monofonto_medium` and
`fixedsys_comp_uniform_width`, 512 x 512 for the other five [observed: `fnvtool dds
list`, 2026-11-05]. NVForever draws a font from its texture's largest level only, so a
texture that brings smaller levels (a localized or modded one) cannot blend a glyph with
its neighbours [design, #1439].

**Which files.** Font n is read from the file `sFontFile_n` of `[Fonts]` names in the
install's `Fallout_default.ini` (in the game's folder; the per-user copy in `My
Games\FalloutNV` is never read, install-layout.md), else from the English install's
file for n [documented: Fallout_default.ini `[Fonts]`; hypothesis: a localized install
names its own files there, not checked]. The start's log names each font read from a
file of its own.

**Without the font.** When a font file or its texture cannot be read, the text is drawn
with Bevy's font, its characters read back from their codes in the run's code page
(`fnv_font::Line::text`), not as Latin-1 [design, #1439].

### 2.2 The `.tex` picture

Every font of `Fallout - Textures2.bsa` has a `textures\fonts\<texture name>.tex` beside
its `.dds` (font 9 has only the `.tex`) [observed: archive listing, 2026-11-08].

| Bytes | What | Tag |
|---|---|---|
| 0 to 3 | an integer, the width: 512 in the files read | [observed] |
| 4 to 7 | an integer, the height: 512 | [observed] |
| 8 to the end | width x height texels of 4 bytes, rows from the top: the colour in bytes 0 to 2 (equal to each other in every texel read, so their order is not known) and the alpha in byte 3 | [observed] |

How it was checked: the English `glow_monofonto_large` and `glow_futura_caps_large`
`.tex` against their A4R4G4B4 `.dds`. Every texel's four values, rounded to 4 bits, are
within one step of the `.dds` texel at the same place, with bytes 0 to 2 against red,
green and blue and byte 3 against alpha. So in an English install the two are the same
picture, the `.tex` with 8 bits a channel. The files are 1 048 584 bytes, 8 more than
512 x 512 x 4.

**What NVForever draws** [design, #1439]: a font's `.tex` when its `.dds` is there too,
else the `.dds` (`interface::font_tex`). This draws a Russian install's letters (§2.1),
and an English install's fonts change only in getting 8 bits of alpha instead of 4.
Which file the original reads is not known [hypothesis: the `.tex`, since a localized
release changed only it]. A font with a `.tex` alone (font 9) is left undrawn, as §3.1
says the original does. A `.tex` that does not parse (wrong size for its header) is
skipped with a warning, and the `.dds` is drawn.

## 3. Setting text

- A character takes its room before, its width and its room after; the next starts
  there [observed: the screenshot's widest line, "under some suitcases in the trunk of
  his car during", is 620 units by this sum, which is exactly the interface file's wrap
  width for that text, and its measured width agrees with the box's to 1%; the three
  lines above it agree as well].
- A line is broken between words, before the word that would end beyond the wrap width;
  a line exactly as wide as the wrap width is not broken [observed: the same line].
  The space at the break is not set [design]. What the game does with a word wider
  than the wrap width is not known; NVForever leaves it whole on its own line.
- How far apart the lines are, and where a text's first line stands under its top, is
  with the menu that was measured (`message-menu.md` §3). The interface files' text
  tiles: menu-xml.md §3.1.2, hud.md §14.10 (a line per font, #979).
- [observed: hud_textbase capture 2026-10-31] A one-line text tile's `height`, read in
  the original, is its font's tallest glyph picture: 36 for font 1, 42 for font 7, 64 for
  font 8, the largest number 10 of their files (menu-xml.md §3.1.2); every font, and
  more lines: §3.1.
- [observed: the textures of fonts 1, 7 and 8, 2026-10-31, decoded and measured row by
  row in alpha] Where the ink of a capital or a digit stands against the line (a
  picture set with its top number 13 over it): font 7 (`baked-in_monofonto_large`, a
  capital 30 over, 36 high) from 29 over to 5 under at alpha 16, 25 over to 1 under at
  128, 23 over to 1 over at 192; font 8 (`glow_futura_caps_large`, a capital 47 over,
  58 high) 44 over to 8 under at 16, 39 over to 3 under at 128, 37 over to 1 under at
  240; font 1 (`glow_monofonto_large`, a capital 25 over, 30 high) 24 over to 4 under
  at 16, 22 over to 2 under at 128, 21 over to 1 under at 192. The glow fades as far
  under the letters as over them, so the middle of the ink is the same at every alpha:
  12 over the line in font 7, 18 in font 8 (17.5 for its digits), 10 in font 1.
- For comparison [observed: the files]: the size less the deepest glyph's reach under
  the line (number 10 less number 13) is 21, 24 and 37 for fonts 1, 7 and 8 (31 - 10,
  36 - 12, 52 - 15); the tallest reach over it 29, 34, 50.

### 3.1 Every font's line, pitch and height (hud_menus capture, 2026-10-31)

[observed: hud_menus capture 2026-10-31, private and not shipped: text tiles of fonts 1-9
injected into `HUDMainMenu`, one line and two (a line break in the string), some wrapped
to three; their `width` and `height` read back through xNVSE/JIP at 1280x720, 1920x1080
and 1280x1024 (the same at all three); the ink of four capitals measured on the shots as
for fonts 1, 7 and 8 above (the ink's middle against the glyph pictures' middle on the
texture, texture middles decoded from each font's `.dds`)]

| font | file size (number 0) | tallest glyph = one-line `height` | two-line `height` | pitch | line under the top, measured | on NVForever's convention | `text_base` | `line_pitch` |
|---|---|---|---|---|---|---|---|---|
| 1 | 31 | 36 | 67 | 31 | 20.3 | 19.8 | 20 | 31 |
| 2 | 31 | 32 | 67 | 35 | 16.5 | 16.0 | 16 | 35 |
| 3 | 26 | 31 | 63 | 32 | 16.5 | 16.0 | 16 | 32 |
| 4 | 36 | 37 | 73 (three lines 109) | 36 | 16.5 | 16.0 | 16 | 36 |
| 5 | 25 | 28 | 53 | 25 | 16.5 | 16.0 | 16 | 25 |
| 6 | 34 | 38 | 76 | 38 | 22.4 | 21.9 | 22 | 38 |
| 7 | 36 | 42 | 80 (three lines 118) | 38 | 24.5 | 24 (anchor) | 24 | 38 |
| 8 | 52 | 64 | 116 (three lines 168) | 52 | 30.5 | 30.0 | 30 | 52 |
| 9 | - | no `width`, no `height` | - | - | nothing drawn | - | 24 [design] | file size [hypothesis] |

- [observed] A text tile's `height` is its font's tallest glyph picture plus one pitch
  for each line after the first, in all eight drawn fonts at one to three lines. The
  pitch is a whole number per font, and the ink's line-to-line distance agrees with it
  (within one pixel row). For fonts 2, 3, 6 and 7 it is not the file's size (+4, +6, +4,
  +2); no number of the files gives all eight (the size, the tallest glyph, the deepest
  reach over plus under the line fit two fonts at most); where it comes from is not
  known.
- [observed] Fonts 2-5 stand their first line about 16.5 under the tile's top, font 6
  22.4; in a real message box the font-2 message text and its button's label agree
  (16.1-16.7). Precision ±0.7 unit at 1280x720 (a pixel row is 1.33 units), ±0.5 at
  1280x1024.
- [design: NVForever's convention] `tiles::draw::text_base` anchors font 7 at 24 (the
  earlier captures, menu-xml.md §3.1.2; this capture reads it 24.5, hud_textbase 24.9)
  and stands every other font as far from it as this capture shows, rounded to the
  nearest half unit. Font 8 was 29.5 from hud_textbase (5.6 under its font 7); this
  capture measures it 6.0 under its font 7. Both read font 8 itself at 30.5, so the two
  differ in font 7 only; their mean distance, 5.8, is nearer 30, and 30 keeps every font
  on one capture's font-7 reading.
- [design] `tiles::draw::line_pitch` is the pitch above for fonts 1-8 and the file's
  size for any other font; `tiles::draw::measure` gives a text the observed `height`
  (`tallest_glyph`; the file's size for a font without pictures, the stand-in).
  NVForever reads the default font files (§4), so the numbers go by font number; when
  it reads the player's INI fonts, a replaced file's pitch is not known [hypothesis: its
  size].
- [observed: the archives] Font 9 (`NVFont_Test`) has its `.fnt` and a `.tex` but no
  `.dds`; in the original its tiles get no `width` or `height` and draw nothing.
  NVForever draws a font-9 text in Bevy's stand-in font (`interface::face` finds no
  texture), on font 7's line, and a font-9 tile among the texts whose widths the tile
  HUD writes stops those writes (`FontKit::game_fonts`) [design: not yet the original's
  nothing].
- Not determined: an empty string's measure. The message box's empty title read 14 x 38
  in font 6, one space wide and one line high; NVForever's `measure` gives 0 wide and
  one line high.

### 3.2 Drawing the glyph pictures

- [observed: issue #1221, original and NVForever message-box captures at 1280x720,
  2026-10-31] In the original, the x-height letters in "NVF box without buttons"
  share their top and bottom ink rows. In NVForever, even the two occurrences of
  `u` differ: one is 12 rows high and the other 10. This is a drawing error after
  the font's measurements have been read, not a different ascent for each occurrence.
- [documented: Bevy 0.20.1, `bevy_ui_render/src/lib.rs`, `NodeImageMode::Auto`
  extraction; `bevy_ui/src/layout/mod.rs`, `ui_layout_system`] The default image
  mode fits a texture rectangle inside its node while preserving its aspect ratio.
  Layout rounding can give equal glyphs at different x positions different box
  widths; the fit then changes both their width and height. `Stretch` fills the
  specified box, and `LayoutConfig { use_rounding: false }` retains its fractional
  dimensions and position.
- [observed: issue #1221 temporary Bevy probe, 2026-10-31] The raw font-2 file and
  texture, at 1280×720, were drawn at physical x 532 with a rounded shared baseline
  plus each of 0, 0.125, 0.25, 0.375, 0.5, 0.625 and 0.75 pixels. Ink rows at red > 80
  and green > 30 were: at 0, capitals 17, x-height letters 12 and 11, `t` 14; at
  0.125, capitals 17, all x-height letters 11, `t` 14; at 0.25, capitals 16, all
  x-height letters 11, `t` 14; and at 0.375 through 0.75, capitals 17, all x-height
  letters 12, `t` 15. The 0.25 result is the original capture's row count.
- [design: #1221] Snap one zero-sized line anchor to physical pixels, then draw
  its glyphs without layout rounding, at their fractional horizontal positions and
  their file ascent above the anchor. Give the rounded anchor a fixed quarter-pixel
  sampling phase. The game uses a 1:1 logical-to-physical pixel scale [documented:
  `engine/nvforever/src/main.rs`, `WindowResolution::with_scale_factor_override(1.0)`],
  so the offset is expressed in Bevy's `Px`. Stretch each texture rectangle into its
  measured box. This retains a common baseline, including after resizing, without
  changing the per-font baseline offsets measured in #979. The terminal's coloured
  characters and the character creator's scaled text use the same rule. Stand-in
  text without a game font texture keeps its existing Bevy text path.

## 4. Font numbers

[documented: `Fallout_default.ini`] 1 `Glow_Monofonto_Large`, 2 `Monofonto_Large`,
3 `Glow_Monofonto_Medium`, 4 `Monofonto_VeryLarge02_Dialogs2`,
5 `Fixedsys_Comp_uniform_width`, 6 `Glow_Monofonto_VL_dialogs`,
7 `Baked-in_Monofonto_Large`, 8 `Glow_Futura_Caps_Large`, 9 `NVFont_Test`.
NVForever uses these defaults; it does not read the player's own INI for them yet.
