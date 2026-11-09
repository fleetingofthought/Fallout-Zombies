# Spec note: the character creator (NVForever's race menu)

Tags as in `chargen.md`: `[documented]` cites a source, `[observed]` says what was
measured on the owner's install (FNV 1.4.0.525, the ten masters), `[hypothesis]` is not
confirmed, `[design]` is NVForever's own choice, `[owner]` is the owner's word.

The owner asked for it (2026-10-16) [owner]: "design me a character creator for the game
i really dont like the original so i want something new in design without having to add
new assets the player needs to be able to change skin tone, face shape data check the
facegen values for what can be changed via sliders, hair styles, beards, eye colour.
anything else you can think of using the base assets for characters that we could
customise or extend to customise".

So this menu does **not** follow the original's race menu (`menus\chargen\race_sex_menu.xml`,
the Reflectron): it is a new design, drawn only from the game's own fonts, menu pieces
and plain shapes, offering what the game's own character data allows. **Restyled 2026-11-06**
[owner, asked directly by a contributor: free to restyle it as we see fit; issue #1731]: the
layout of §4 and the camera's side in §5 move closer to the arrangement of the original's
portrait and terminal; what the player can change, the tabs, the amber and the fonts stay
the owner's. The original's menu itself is a separate option (#1730).

Code: data `crates/fnv-actor/src/creator.rs` (with the controls file reader in
`facegen.rs`); the look kept by the game `crates/fnv-game/src/chargen.rs` (`Look`,
`apply_look`); the menu `engine/nvf-play/src/creator.rs`, `creator/model.rs`,
`creator/view.rs`; the camera `engine/nvf-play/src/control.rs` (`Portrait`); the body
`engine/nvforever/src/player_body.rs`; the report `fnvtool actor creator` (§7).

## 1. When it opens

- Stage 36 of VCG01 calls `ShowRaceMenu`, which opens menu 1036 [observed:
  layer5-captures.md §10.3]; stage 40 is set inside it by VCG01's `MenuMode 1036` block
  [observed, same]. The creator draws menu 1036 whenever the engine has given it its
  data (`CreatorData`, built from the actor records at start in `--play`); without it
  the placeholder panel stays (`script_hud.rs`).
- `ShowRaceMenu` is also how later scenes offer a new face [documented: GECK wiki
  "ShowRaceMenu"; hypothesis: which scenes, e.g. Doc Mitchell's last question and the
  New Vegas Medical Clinic, not checked]. The creator always opens on the player's
  current look, so reopening it edits rather than starts over [design].
- `ShowBarberMenu` and `ShowPlasticSurgeonMenu` open the same menu 1036 [documented:
  GECK wiki "MenuMode (Function)", notes: "ShowRaceMenu, ShowPlasticSurgeonMenu, and
  ShowBarberMenu all return true for MenuMode 1036"]. The game keeps which function
  opened it (`Game::race_menu`), and the creator shows what that one may change (§10,
  §11).

## 2. What the game data offers [observed unless tagged]

Survey 2026-10-16 over the ten masters (`fnvtool actor creator`, §7, and a scratch
reader; full tables kept in the session notes, not in the repository).

### 2.1 Races

- **Four playable races**: Caucasian (0x19), Hispanic (0x38E5), Asian (0x38E6),
  AfricanAmerican (0x424A) — the only RACE records with DATA flag Playable; Ghoul and
  the DLC races are not. The four `*Child` races carry the Child flag (height 0.8) and
  are not offered.
- All four use the same head and body meshes and textures (`headhuman.nif` /
  `headfemale.nif`, `male\headhuman.dds` / `female\headhuman.dds`, the
  `upperbodyhuman{male,female}.egt` body files); they differ only in their FaceGen
  values (the race's baseline, actors.md §4.2), lists and defaults. So skin tone and
  race are not tied: any race can take any skin tone with the skin sliders.
- Default hair (male / female): Caucasian HairDefault / HairMessy03F; Hispanic
  HairMessy01 / HairCurlyF; Asian HairDefault / FemaleC; AfricanAmerican
  HairAfricanAmericanBase / HairAfricanAmericanF. Default hair colour (CNAM, an enum of
  16 names [documented: xEdit RACE CNAM]): Brown / Brown for Caucasian and Hispanic,
  Black / Brown for Asian and AfricanAmerican.
- Age chain: each race names an older one (ONAM: `<Race>Old`) and a younger one (YNAM:
  `<Race>Child`); the `<Race>Old` races use `headhumanold.dds`, an aged face texture.
  The `<Race>OldAged` races are outside the chain.

### 2.2 Hair, beards, brows, eyes

- **Hair (HAIR)**: 41 playable styles, 18 for men (Not Female) and 23 for women (Not
  Male); every playable race lists all of them (Asian lists one more, not playable).
  No hair is facial hair, none is Fixed.
- **Beards (HDPT)**: 47 playable head parts, all beards and moustaches: 32 with a model
  (14 distinct meshes, some with another texture: "Survivalist", "Goatee", "Muttonstache",
  "Honest Abe", …), 15 "BeardMix" records without a model that list two or three others.
  Head parts carry no sex or race; no female NPC wears a beard. The creator offers all 47:
  a model-less part counts as a beard when every part it lists is one [observed:
  BeardMix6a "Ronin" lists three beards]. Starting faces wear BeardMix records
  (`CGPresetHispanicM03` wears "Ronin" [observed]); left out of the list, the Facial hair
  row read "Clean shaven" and could not take such a beard off.
- **Brows (HDPT)**: `EyebrowM` and `EyebrowF` are not flagged playable, yet the player's
  own record wears `EyebrowM`, and 1,203 NPCs wear one of them.
- **Eyes (EYES)**: each playable race allows four: Blue, Dark Brown, Hazel
  (EyeDefault), Green; none is tied to a sex.
- **Hair colour**: NPC_ HCLR is free RGB; 4,220 NPCs use about 14 clusters (browns,
  blacks, greys, blond, red-brown, white); `(75, 50, 25)` alone 1,292 times
  [hypothesis: the editor's default]. The 16 colour names are GMSTs `sHairColor0…15`
  (Bleached … Champagne) with no RGB in the data. HCLR also tints beards (actors.md §1.2).

### 2.3 FaceGen: the face itself

- A face is 50 symmetric shape values (FGGS), 30 asymmetric shape values (FGGA) and 50
  texture values (FGTS), in standard deviations of FaceGen's sample [documented:
  FaceGen SDK manual, "Overview"]. Shape comes only from the `.egm` files (275, all 50 +
  30 modes), colour from the `.egt` files (36, all 50 + 0) [observed]. The heads'
  `.tri` files hold speech, expression and blink morphs only, nothing for the face's
  shape. The modes are statistical: each moves the whole face, none is "the nose"
  [observed: mode 0's largest moves are at the head's side, mode 1's at the chin…].
- **The named controls**: `facegen\si.ctl` in `Fallout - Misc.bsa` (338,421 bytes) is a
  FaceGen controls file [documented layout: FaceGen SDK manual, "File Formats", CTL;
  observed: the layout accounts for every byte, and its geometry and texture basis
  versions, 0x7745C425 and 0x51, are those of every `.egm` and `.egt`]. It holds:
  - **56 shape controls**: Brow Ridge 3, Cheekbones 3, Cheeks 2, Chin 7, Eyes 4, Face 5,
    Forehead 3, Jaw 2, Jaw-Neck slope 1, Jawline 1, Mouth 9, Mouth-Chin distance 1,
    Nose 14 (one label twice), Temples 1 — "Nose - bridge shallow / deep", "Chin - wide /
    thin", …
  - **26 asymmetry controls**: twists and shifts ("Nose - tip transverse shift", "Face -
    coronal bend", …).
  - **33 texture controls**: skin ("Skin Shade Dark / Light", "Skin Flushed / Pale",
    three "Skin Tint" pairs), beard shading (6), cheek blush, eye sockets, eyebrows (7),
    eyelids, eyeliner, eyeshadow, "Eyes Dark Brown / Light Blue", eye whites, lips and
    lipstick, naso-labial lines, nares, nose.
  - **Age and gender** per race group (All, Afro, Asia, Eind, Euro): each a shape and a
    texture direction with an offset (age 29.7 / 32.1, gender −0.20 / −0.58), the five
    groups identical [observed]; race-morph controls (20); and density data.
  - Each linear control's direction has length 1 [observed].
- **Which way a control goes** [observed, `fnvtool actor creator`'s per-unit effects on
  the male head and body files]: +1 moves towards the label's *second* word: "Skin
  Shade Dark / Light" +1 adds (+20, +23, +26) to the body skin; "Skin Tint Orange / Blue"
  +1 adds blue (+4.6); "Nose - short / long" +1 pushes the nose tip 0.11 forward; "Chin -
  forward / backward" +1 pulls the chin 0.13 back; "Eyes - together / apart" +1 moves the
  eyes outwards. So a control reads `direction · coefficients` and setting it moves the
  coefficients along the direction [hypothesis: FaceGen's convention, consistent with
  these effects]; the age control reads `direction · coefficients + offset`, in years
  [hypothesis: the offset is the sample's mean age].
- **Starting faces**: 80 NPC_ records carry ACBS "Is CharGen Face Preset" [documented:
  xEdit], ten per playable race and sex (`CGPreset<Race><M|F>01…10`).
- **The player record** (NPC_ 7): Caucasian male, hair HairWavy ("Smooth Wave"), HCLR
  (84, 70, 46), EyeBlue, head part EyebrowM, FGGS of length 14.3, FGGA all zero, FGTS of
  length 6.8; it matches the preset CGPresetCaucasianM01's race, hair, eyes, brow and
  hair colour.
- **What the authored faces span**: 4,180 NPC faces with full FaceGen data (non-child
  races). 98% have no asymmetry at all (actors.md §4.2).

### 2.4 Other base assets

- **No detail textures**: no scar, tattoo, freckle, dirt or complexion textures exist;
  complexion is the texture controls (above) and the old races' head texture.
- **No body shape**: no `.egm` or `.tri` for bodies, only colour `.egt`; RACE weights are
  all 1.0, the player's NAM7 weight 0. Height: RACE DATA and NPC_ NAM6 only.
- **Pre-baked faces**: `textures\characters\facemods\` holds 3,034 per-NPC face textures
  (and `bodymods\`); the player has none [hypothesis: the game draws NPCs from them and
  computes the player's face]. NVForever computes every face from the `.egt` (actors.md
  §4), so nothing changes for the creator.
- **Glasses and hats**: 14 eyeglasses (ARMO, Eye Glasses slot), 155 playable headwear
  items — gear, not appearance.
- **Voices**: the player record names PlayerVoiceMale; PlayerVoiceFemale exists; the
  player does not speak lines in New Vegas [hypothesis for every scene].

## 3. The sliders [design unless tagged]

- Every control of `si.ctl` is a slider, in the file's order, grouped by the label's
  first part ("Nose", "Skin"); the label's two words are the slider's ends (the low end
  the first). The repeated label is offered once. Age is a slider of its own on the
  Basics tab, moving the shape and texture parts together.
- **What a slider reads**: the face drawn — the player's own values plus the race's
  (actors.md §4.2, §4.3). It moves only the player's own values. So changing race keeps
  the player's own face on the new race's baseline, and a slider's reading moves with it;
  each track shows where the race alone puts the face (a sand tick).
- **Ranges**: from the authored faces of that sex (own + race values): the 1st to the
  99th percentile, a quarter of that span added at each end. Asymmetry: at least ±2
  (the authored faces have almost none); age: no younger than 16. Shown as −100 … +100
  of the range (age in years).
- **Not offered**: the gender control (the head mesh is per sex), the race-morph
  controls (the race is chosen as a whole; its baseline is the race's own FaceGen
  values), the hair length (LNAM: NVForever does not draw it).
- Without `si.ctl` the creator still offers races, hair, beards, brows, eyes, colours,
  starting faces and Randomise, with no sliders (and says so in the log).

## 4. The look of the menu [design unless tagged]

A sketch: the published mockup (the owner's review). The layout below is the restyle of
2026-11-06 [design; owner, asked directly: free to restyle it]: the first version had the panel on the left, 640 wide, on a flat black
0.8 with a bar along its edge, the tabs on top, and the hints "Space / X". It moves towards
the original's arrangement [observed, the original at 1280 x 720: the portrait on the left,
a narrow terminal on the right; sliders as a label over a `<--->` bar; a square on the
chosen list row; the pages' lamps along the bottom], not its machine: that is not drawn.
The rules:

- **No new art**: the game menus' own fonts, its shared menu pieces
  (`Interface\Shared`: `fade_to_bottom`, `fade_to_top`, `arrow_left`, `arrow_right`,
  `square_filled`, `background\pipboy.dds`) and plain filled rectangles; no picture of a
  machine, no Pip-Boy model.
- **Fonts** [owner 2026-10-17: "if we can also get the font to match our game"]: the
  menus' title font (font 6, `glow_monofonto_vl_dialogs`, the tag-skill and trait menus'
  headlines) for the title, the tabs and the buttons, `monofonto_large` (font 2) for the
  rest. The first version had Futura (`glow_futura_caps_large`) for those.
- **Colours** [owner 2026-10-17: "can we also change the colour scheme to match the UI
  amber look im not a fan of the light blue trim"]: everything in the interface's amber
  (`interface::AMBER`): text at brightness 215 (fainter by transparency where it is a
  hint or not chosen), values, the frame, the chosen tab's underline, the chosen
  row's square, the slider's fill and knob, the race
  tick and Done's outline at full brightness; the chosen row's band and button's fill dim amber (brightness 128) a quarter opaque, as the menus' highlight; dividers at 18%. The first version was bone white with a turquoise
  accent and sand values.
- **Backing and frame**: `background\pipboy.dds`, the grey vignette behind the Pip-Boy's
  and terminal's screens, in amber at brightness 120, 0.98 opaque so the text reads over a
  bright room (a black 0.8 if the picture cannot be read) [design; at 0.9 a bright wall
  could be read behind the text, observed on the first shots: transparency mixes in
  linear light]. The frame is the menus' brackets (`chargen::draw`,
  top_bracket.xml and bottom_bracket.xml): a line along the top and the bottom, a line
  fading away from each of their ends, 50 down and 40 up.
- **Layout** (interface units, 960 high): one panel on the right, 480 wide, 32 in from the
  top and the view's right; the rest of the picture is the player, framed by the camera
  (§5). 480 fits the longest words [observed, measured with the two font files against
  `fnvtool actor creator`'s names: "Face forehead-sellion-nose ratio" and its value 399 of
  the 424 between the margins; "Facial hair 47/47" 165 and "Shaggy Sideburns" 184 in a
  row].
  - Title "APPEARANCE", the look's name on the right ("Female Hispanic"), and under them a
    line saying what the tab is for.
  - The rows, in the middle (below).
  - Six tabs, along the panel's bottom over the buttons [design: the original's pages
    stand along the bottom; there was room, the words at most 59 wide in 70, and the rows
    keep the seven sliders they had]: **BASICS** (sex, origin, starting face, age), **FACE** (the 56 shape
    sliders, one region at a time), **DETAIL** (the 26 asymmetry sliders by region),
    **SKIN** (tone and tints first, then blush, lips, lipstick, nose, naso-labial lines),
    **HAIR** (style, a strip of 16 authored colours, shade / warmth / grey fine-tuning,
    facial hair for men with the beard's shading sliders under it, then the brows with
    theirs; no Region row [owner 2026-10-23: it switched sliders out of sight and seemed
    to do nothing]), **EYES**
    (eye colour, then eye sockets, eyelids, eyeliner, eyeshadow, the eyes' tint).
  - Rows: a list row is its label, "n/m", and `< value >`; a slider row its label and
    value over a thin bar with an arrow at each end (the game's `arrow_left` and
    `arrow_right`; a click steps it, as a list row's arrows do), a centre mark, the race
    tick, an amber fill from the centre to the knob, and the two end words under the ends.
    The chosen row has a small square at its left (the game's `square_filled`, as its
    lists mark a row) and a band of dim amber, a quarter opaque (the menus' highlight fill). "more above / below" when a region has
    more rows than fit, and a scrollbar in the panel's right margin (track and a thumb the
    size of the part shown) [owner 2026-10-23: "add a scroll to show there are more
    options"]. The list scrolls apart from the chosen row; the buttons never scroll.
  - Four buttons along the bottom: RANDOM, UNDO, RESET, DONE (outlined brighter), each
    with its key and button under it in the form the other menus give a button's key
    ("Done A)"): "Space) X)", "Z) Y)", "R) Back)", "Enter) Start)".
  - A hint in the bottom left of the picture, on the same backing: turning, panning and
    zooming in words ("Drag to turn"), since a drag or a stick is not a key; the same form
    only for the keys and buttons ("Reset View C)").
- **Sounds**: the game's own UI sound records (`ui_sounds.rs`): rows and tabs
  `UIMenuFocus` / `UIMenuPrevNext`, Reset and Undo `UIMenuCancel`, Done `UIMenuOK`.

## 5. The camera and the preview [design unless tagged]

- While the creator is open the camera stands in front of the player looking back
  (`control::Portrait`). It **opens on the face** [owner 2026-10-17, todo 6: "the
  camera starts framed on the face instead of the whole body"]: on the Basics tab, with
  the face tabs' distance and tilt, straight on (`model::View::face_on`); zooming out from
  there shows the whole body. Each tab has a framing: the **whole body** on Basics (175
  units away), the head turned 0.45 rad and seen a little from above on Hair (58), the
  **face** on the others (42, turned 0.15). The camera steps right a third of its
  distance so the player stands left of the picture's middle, clear of the panel, the
  face turned towards it as in the original's portrait [the restyle of §4; it
  stepped left while the panel was on the left: observed, the smoke run's screenshots at
  1280 × 800 had the face behind the panel without a step]. No collision: the player's
  own space.
- **Aimed from the drawn body's eye** (`BodyView::portrait_eye`), not the first-person
  view's: in the real opening the first-person eye still sat where the wake-up idle had
  put it while the body already stood, and the whole-body framing showed the chest to
  the feet [observed: the `character-creator-opening` screenshot, 2026-10-17].
- **The point looked at follows the zoom**: 2 units under the eye (the face) at 42
  units or nearer, 58 under it (the middle of the body) at 175 or further, in between
  in proportion. So zooming in on the whole body ends on the face [owner 2026-10-17:
  zooming went "in and out on the pelvis"].
- **Looking round** [owner 2026-10-17: "we need a way to drag and pan our view not just
  zoom in and out"]: over the picture (not the panel), left-drag turns the camera about
  the player (all the way round; half a turn across the picture's height) and tilts it
  (−0.35 to 0.8 rad, a little below the eye to well above the head); right- or
  middle-drag, or Shift + left-drag, pans: the picture follows the pointer, up to 40
  units to either side, 30 up and 80 down; the wheel zooms a tenth a notch, from 20 to
  260 units. C or Home puts the tab's framing back. On the controller the right stick
  turns and tilts, the left stick pans, the triggers zoom, the right stick's click puts
  the framing back. A new tab starts its own zoom and pan again and keeps the turn. The
  camera eases onto where it is asked to be over about 0.1 s, the short way round.
- The third-person body is drawn while the creator is open (`third_person_view`).
- **Light** [owner 2026-10-17: "we need some kind of light its hard to see details you
  are changing in the shadow"; design: the rig, `engine/nvforever/src/creator_view.rs`]:
  two point lights of the engine's own go with the camera while the creator is open, a
  warm key (fade 1.6) above and to the camera's left and a cool fill (0.55) to its right
  and lower, 320 units' reach, no shadows, in the units of the game's own lights and
  scaled by `--light-scale` like them. They are removed when the creator closes, so the
  room's lighting is as before.
- **Where the player stands** [owner 2026-10-17: "maybe we move the character to a
  different spot during the creation opening instead of doc mitchells bed"; design:
  stay, decided 2026-10-17]: the player stays where the opening has them. Menu 1036
  opens at stage 36, at the end of one of Doc Mitchell's lines, with the player by the
  bed after the wake-up idle; stages 45, 50 and 55 follow at the end of his next lines
  and by timers, and the player is next asked to walk to the Vigor Tester
  [observed: layer5-captures.md §10.3]. Moving the player would have to undo itself
  without upsetting the bed's furniture state, the idle that had just driven the view,
  Doc's head turning to the player and his packages, for nothing the picture needs.
  Instead the creator lights the player (above) and clears the line of sight: an actor
  whose root lies within 40 units of the line from the camera to the point looked at,
  between them, is hidden while it is in the way and shown again when it is not, or
  when the creator closes (Doc Mitchell in his chair, between the camera and the bed in
  the owner's screenshot).
- **The studio** [owner 2026-11-02: "we might need a dummy cell for this specific one the
  hospital room you are born might be an awkward spot to show the character creator";
  design: a picture, not a place]: when Tale of Two Wastelands' gene projector opens the
  menu (`TTW_ShowGeneProjector`: the birth scene, the Vault 101 exit's "change your
  face"; `Game::race_menu_studio`), the creator is drawn in a studio instead of the
  room. The camera sees only the player's body and the studio's own layer
  (`creator_view::STUDIO_LAYER`): a closed backdrop round the player in the interface's
  dark tones, a floor and a paler pool of floor under the player, all of plain colour
  and unlit, with the creator's two lights on the body. Nothing in the game moves: the
  player stays where the scene has them, the scene and its scripts go on as for the
  room, and the camera's layers are put back when the creator closes. The rest of the
  game's race menus (New Vegas's ShowRaceMenu at Doc Mitchell's, the barber, the plastic
  surgeon) stay in the room, as decided above.
  - **Seen in game, 2026-11-08** (owner: "pure black"; the studio commit had been pushed
    after #1407 merged and is brought back here). Two things kept it dark: the creator's
    lights were on the world's render layer only, and a light lights only the views whose
    layers meet its own, so the studio's camera (body and studio layers) got none of them;
    and the scene's image-space modifiers stayed on the picture (in the birth scene
    CG00BirthBaseISFX, Bright Scale ×2 and Bright Clamp ×0.2, with the eye adaptation it
    drives), leaving the player a silhouette. Now the lights are on the world's and the
    body's layers, and while the studio stands the scene's modifiers are left off the
    grade (`imagespace.rs`; they go on playing and are back when it closes), the cell's own
    grade kept [design]. Checked in the birth scene with stage 60 set once the black
    opening modifier had run out: the face lit, the backdrop and floor seen.
- **Nothing else over it** [owner 2026-10-17: "hide the UI so no health bar or AP or
  dialogue"]: the HUD (health, action points, compass, the weapon's condition), the
  activation prompt ("E) Talk"), the dialogue menu's line and the subtitles are not
  drawn while the creator is open (the subtitles are still logged), whatever state the
  opening leaves the game in (it can be Playing with menu 1036 open).
- **Live preview**: each change is written into the game (`PlayerState::look`); the
  player body resolves with it (`Resolver::resolve_as`: race, sex, hair, hair colour,
  eyes, head parts and FaceGen values in place of the record's; no hair is bald, not the
  race's default) and is rebuilt when it changes, at most every 0.1 s while a slider is
  dragged. Face shape, the face and body skin and the hands follow (actors.md §4); hair
  and beards take the hair colour.
- **No T-pose between bodies** [owner 2026-10-17: "theres a few frames where the model
  is t posing can we blend the transitions together"]: a rebuilt player body (a new
  look, another outfit) goes on with the old body's clips, their times and fades and
  its last pose (`anim_blend::Animator::continue_from`); before, its clips faded in from
  nothing over 0.2 s and the bind pose showed through. NPCs are not rebuilt when their
  outfit changes: the parts are swapped on their posed skeleton (`outfits.rs`).

## 6. Keys, pointer, controller, flow [design]

- **Keyboard**: Q / E, Tab (Shift+Tab), Page Up / Down: tabs. Up / Down, W / S: rows,
  then the buttons. Left / Right, A / D: change the row (held, a slider keeps moving, 30
  steps a second; Shift for steps a quarter as big: 0.6% of the range instead of 2.5%).
  Space randomise, Z undo, R reset, Enter: the chosen button, or (from a row) go to Done
  — a second Enter is Done.
- **Pointer**: shown and free whenever the creator is open [owner 2026-10-17: "we
  should have mouse pointer in this menu"]. In the real opening menu 1036 opens at the
  end of Doc Mitchell's line, still in the conversation (the Dialogue state, not Menu)
  and with the wake-up idle still driving the view, and the rule that hides the pointer
  while an idle drives the view kept it hidden [observed: a `--new-game` run, "pointer:
  hidden (an idle drives the view)" and no "shown" after "creator: open"]; the creator
  now overrides it (`player::grab_cursor`). Click a tab, a row, its arrows, a
  hair-colour swatch or a button; press on a slider's track and drag; hovering over a
  row or button chooses it when the pointer moves or the rows scroll under it [owner
  2026-10-23]; the wheel over the rows scrolls them (a notch a row, the chosen row
  stays; nothing over the tabs or buttons), a click or drag on the scrollbar scrolls to
  that place, keys that move the chosen row out of sight scroll it just into view; over
  the picture, drag and the wheel move the camera (§5).
- **Controller** (the menu keys of `crate::pad`: D-pad arrows, A Enter, LB / RB Page Up /
  Down): X randomise, Y undo, Back reset; the sticks and triggers move the camera (§5).
- **Changing sex** starts the face from the race's first starting face for that sex: a
  face shaped for one sex reads as that sex on the other's head [observed: the smoke
  run's screenshot of the record's male values on the female head]. Changing race keeps
  the face's own values (the race's baseline changes under them).
- **Randomise**: two authored faces of the race and sex, blended at a random point
  between them, with the hair, hair colour, beard and brows of the first and one of the
  race's eyes; the race and sex stay. **Undo** (64 deep) takes back a change; the steps
  of one row in a run are one change. **Reset**: the look the menu opened with.
- **Done** answers menu 1036 as the other character menus do (`chargen::finish`); the
  scripts go on (stage 40 was set inside the menu, VCG01's timer then moves on). The look
  stays in `PlayerState::look`, which saves keep; SexChange's `female` is set with it,
  so GetIsSex and GetPCIsSex follow, and GetIsRace on the player reads the look's race.

## 7. Tools and checks

- `fnvtool actor creator --schema schemas/fnv <Data> <plugins…>` (`fnv.actor.creator/1`):
  `ctl_problem`, `counts`, `races` (per sex: hairs, eyes, default hair, presets, how many
  authored faces), `facial_hair`, `brows`, `hair_colours` (the 16 most used), `sliders`
  (label, group, aspect, low, high, space, `range_male`, `range_female`, `player` —
  the player record's reading — and one unit's effect: `body_colour_per_unit` for skin
  controls, `most_moved` {vertex, at, by} on the male head for shape ones).
- Unit tests: the CTL reader, labels, sliders (reading, setting, ranges), the catalogue
  helpers (`fnv_actor::creator`); the model's tabs, rows, choices, presets, undo, reset,
  randomise, buttons, camera, hair tones (`nvf_play::creator::model`); hit tests and
  scrolling (`view`); the test aid, look conversion (`creator`); the portrait camera
  (`control`); the look's rules (`fnv_game::chargen`).
- Test aid `NVF_PLAY_TEST_CREATOR=<s>:<op>[,…]` (`creator.rs`), the camera's steps
  `orbit:<deg>:<deg>`, `pan:<right>:<up>`, `zoom:<factor>`, `view`; each camera change
  logs "creator: camera …" with the turn, tilt, distance, the point looked at and the
  pan. Smoke scenarios (`tools/smoke/scenarios.toml`): `character-creator` opens menu
  1036 in Doc Mitchell's house, changes sex, origin, hair, two sliders, eyes,
  randomises and undoes, turns, zooms and pans the camera on the Face tab, zooms in on
  the whole body (aimed near the face), Done, with screenshots of each view;
  `character-creator-opening` plays the real opening to menu 1036 and checks the
  pointer is shown there.

## 8. Not built, and ideas for later

- **Older look**: the race's ONAM race (`<Race>Old`) draws an aged head texture; a
  "Weathered" switch could swap to it [design idea; whether the game lets a player be
  one is not known].
- **Height**: RACE DATA height × a player scale (e.g. 0.95–1.05) would be easy in
  NVForever; the original keeps the player at the race's height [hypothesis].
- **Hair colour names**: the swatches could carry the `sHairColor` names if a mapping
  from name to colour is settled (the data has none).
- **Brows per sex**: both brows are offered to both sexes; `EyebrowM` / `EyebrowF` by
  sex would be the authored use.
- **Race mixing**: the controls file's race-morph controls could offer an "ancestry"
  blend instead of four fixed races.
- **Gear preview**: show the eyeglasses (Four Eyes) or a hat on the preview.
- **Faster preview**: each change rebuilds the whole body; in the smoke run the rebuilt
  body's log line followed a change by 0.5–0.9 s [observed: `target/smoke/
  character-creator.log`, Doc Mitchell's house]. Dragging a slider shows the face in
  steps. Re-morphing only the head and the skin textures of the body already drawn
  would make it smooth.
- **The world behind**: not blurred or darkened outside the panel; the room's light is
  the face's light.
- Not checked in the game: the controls' convention and the age offset (§2.3,
  hypothesis), how the original's sliders map to these, whether the original blends
  presets.

## 9. Questions for the owner

1. Colours: settled, the interface's amber (owner 2026-10-17).
2. Brows: offer both brows to both sexes, or the authored one per sex?
3. Should a woman be allowed facial hair (the data does not forbid it)?
4. Ranges: the authored faces' spread plus a quarter at each end — wider or narrower?
5. Should Randomise also change the race?
6. Add the "Weathered" (older race) switch and a height slider?

## 10. The barber (ShowBarberMenu)

- ShowBarberMenu opens menu 1036 as the barber's (`fnv_game::game::RaceMenu::Barber`;
  the function was a stub before). What it changes: the hair style and the hair colour
  [documented: GECK wiki ShowBarberMenu, "Brings up the menu which allows the player to
  select hair color and style"] and the facial hair [owner 2026-10-17, todo 6: "can
  change only the hair and beard options"]. The race, the sex and the name can not be
  changed [owner, same].
- Not the brows [design: the GECK wiki names only the hair; the brows are a head part of
  the face, as the eyes are, and stay with the plastic surgeon], nor the face's beard
  shading (a FaceGen texture value of the face).
- The panel is the creator's with the title BARBER and one tab, HAIR: hair style, the
  hair-colour swatches, shade, warmth and grey, and facial hair for a man; the camera
  opens on the Hair tab's framing (the head, turned a little). Random draws the hair,
  its colour and the beard from an authored face of the race and sex and keeps
  everything else; Undo, Reset and Done as in the creator. Other tabs can not be chosen
  (keys, pointer or the test aid).
- Log: "creator: as the barber (ShowBarberMenu): the hair, its colour and the beard
  only" after "creator: open on …". Smoke scenario `barber-menu` (the console runs
  ShowBarberMenu in Doc Mitchell's house).

## 11. The plastic surgeon (ShowPlasticSurgeonMenu)

- ShowPlasticSurgeonMenu opens menu 1036 as the plastic surgeon's
  (`fnv_game::game::RaceMenu::Surgeon`; the function was a stub before). What it changes:
  the face [documented: GECK wiki ShowPlasticSurgeonMenu, "allows the player to change
  their face"]: its shape and asymmetry, its skin, the starting faces and the age, the
  eyes and the brows [owner 2026-10-18, todo 6: the face sliders, starting faces, eyes,
  and the brows left out of the barber]. Not the hair, its colour or the beard, nor the
  race, the sex or the name [owner, same].
- The panel is the creator's with the title SURGEON and five tabs: BASICS (the starting
  face and the age, no sex or origin), FACE, DETAIL, SKIN, EYES. With no Hair tab the
  brows are on the Eyes tab (the row and their shading sliders) and the beard's shading
  sliders on the Skin tab (both are the face's texture) [design]. It opens framed on the
  face, as the creator does.
- A starting face brings its face, eyes and brows and leaves the hair, its colour and
  the beard as they are; Random draws a new face (two authored faces blended, §6) and
  leaves the hair and beard; Undo, Reset and Done as in the creator. The Hair tab can not
  be chosen.
- Log: "creator: as the plastic surgeon (ShowPlasticSurgeonMenu): the face, eyes and
  brows only" after "creator: open on …". Smoke scenario `plastic-surgeon-menu` (the
  console runs ShowPlasticSurgeonMenu in Doc Mitchell's house).
