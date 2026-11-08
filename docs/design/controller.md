# Controller support (design)

The owner asked on 2026-10-16 for a controller layer: "both keyboard and mouse and the ability
to use a controller depending on a setting under controls", with prompts that follow the
device ("Press E) to open could become Press A) if on an xbox controller") and the base
game's defaults. Facts about the original are in docs/spec-notes/controls.md §2.1. Code:
`engine/nvf-play/src/pad.rs` (the controller), `bindings.rs` (the bindings and
`ControlInput`), `pause.rs` (the Controls tab), `aim_assist.rs` (§5).

## 1. One setting picks the device

`Input Device` in `[Controls]` of `NVForever.ini`, `Keyboard and Mouse` (the default) or
`Controller`, and the first row of the settings page's Controls tab. Every control is
asked through `ControlInput`, which answers from the keyboard and mouse or the controller
as the setting says, never both [design: the owner's "depending on a setting"]. Esc and
the console's key keep working with a controller chosen, so a player is never locked
out. Any connected controller counts (Bevy's gilrs backend, on by default); its arrival
and departure are logged (`controller: … connected`).

## 2. Defaults are the game's

Each control's controller input comes from the game's own `[Controls]` byte `JJ`
(spec-notes/controls.md §2.1, read as hypothesis until checked in the original):

| Control | Button | | Control | Button |
|---|---|---|---|---|
| Forward / Back / Slide Left / Right | left stick | | Vats | RB |
| Activate | A | | Toggle POV | LB |
| Menu Mode (Pip-Boy) | B | | Rest (Wait) | Back |
| Ready Item | X | | Crouch/Sneak | LS (press) |
| Jump | Y | | Grab | RS (press) |
| Block (aim) | LT | | Hotkey1, Ammo Swap, Hotkey3, Hotkey4 | D-pad up, right, down, left [design] |
| Use (attack) | RT | | Run, Always Run, Auto Move, Hotkey5–8, saves | none |

The right stick looks at `fXenonHorizLookSpeed` 1500 and `fXenonVertLookSpeed` 1200,
read as milliradians a second at full push [hypothesis], pushed a little turning slowly
(the push squared) [design]. The left stick moves the way it is pushed; under 0.6 of a
full push the player walks while Always Run is on [design]. A stick pushed past half way
counts as its direction's button (for Auto Move's stop and getting up from a seat).
`Invert Y` serves both mouse and stick.

The buttons are kept in `[Controller]` by the controls' INI names (`Activate=A`,
`Forward=Left Stick Up`, `Hotkey5=None`) with `Look Speed Horizontal` and `Look Speed
Vertical`; the Controls tab rebinds them (the next controller button pressed, a swap with
the control that had it, Start cancels, Delete clears, Defaults puts back the game's).
Start cannot be bound.

## 3. Menus and prompts

- Start is Esc (the pause menu) everywhere. In a menu the D-pad is the arrow keys, A
  Enter, B Esc, LB and RB, LT and RT Page Up and Page Down: the menus read keys, and the
  controller stands in for them (`pad::menu_keys`), pressing a key only as its button
  goes down.
- Each menu's own buttons follow the original's menu files (`_xbox_button` of the button
  tiles [documented: the vanilla menu files]) and press the key NVForever's menu gives
  that control (`pad::menu_buttons`, keyed by the open menu's MenuMode code); a menu's
  own button wins over the shared ones above (#1393):

  | Menu | Buttons |
  |------|---------|
  | Container | X Take All |
  | Barter | X Accept |
  | Pip-Boy (every page) | LT / RT the previous / next page (STATS, ITEMS, DATA, round again); D-pad left / right the tabs [design: owner 2026-11-02] |
  | Pip-Boy ITEMS | X Drop (`pipboy::items`, not while "How many?" is up), Y Repair, LB Mod |
  | Pip-Boy STATS | status tab: A Stimpak, Y Doctor's Bag, X Rad-X; General tab: X switch lists |
  | Pip-Boy DATA | Y Challenges (Notes tab); A Fast Travel on the World Map's marker (`pipboy::data`) |
  | Crafting | X Accept |
  | Repair services | X Repair All |
  | Level up, traits | Y Reset, X Continue, B Back |
  | Tag skills | Y Reset, X Done |
  | Lockpicking | X Force Lock |
  | Blackjack | A Hit, Y Double/Deal, RT Split/Raise, LT Switch/Lower, X Surrender/Max Bet, B Stay/Exit |
  | Roulette | A Place, Y Remove, X Finish, LT Lower, RT Raise, B Exit |
  | Slot machine | A Spin, RT Raise, LT Lower, Y Payouts, X Max Bet, B Exit |
  | Tutorial | RT Next, LT Previous |
  | Caravan bet [design, #1662] | A Accept, Y Auto Match, RT Raise, B Exit |
  | Caravan deck [design, #1662] | A Add, X Remove, Y Randomize, RT Play Caravan (B, the shared Esc, leaves) |
  | Caravan game [design, #1662] | A Select Card, X Discard Card, Y Discard Track, B Cancel; D-pad left / right the hand |
  | Poker [design, #1662] | A Check/Call (between hands Next Hand), RT Raise, Y All In, X Fold, B Leave; D-pad up / down the raise |
  | Pause menu [design, #1662] | X Delete (a save on the Load page, a binding on the Controls tab) |
  | Pip-Boy STATS, RAD view [design, #1662] | A RadAway (not used from the page, as its key), X Rad-X |

  Caravan has no menu file read yet and poker and the pause menu are NVForever's own,
  so those buttons are NVForever's, following the other card games (A the main action,
  Y and X the next, B leave). A minigame's help box rests the minigame's own buttons, so
  A (Enter) and B (Esc) close it as its keys do (blackjack's A is otherwise its F)
  (`pad::MenuPages`, `pad::HELP_PAGE`). Poker and the pause menu have no MenuMode code
  and stand in `pad::MenuAt` as codes of their own (`POKER_MENU`, `PAUSE_MENU`); while
  the pause menu is up its buttons are the menu's own, not those of a menu under it.

  On the ITEMS page the triggers are the pages and LB is Mod, so of the shared paging
  buttons only RB (Page Down) is left there; the D-pad steps the rows both ways.
  On the STATS status tab X is Rad-X, which only acts on the RAD view; on CND and EFF it
  does nothing.

  Not followed: the ITEMS page's RB Hotkey (the hotkeys are held digit keys).
- On the Pip-Boy DATA page's maps the right stick moves the cursor (the highlight box),
  pushed past an edge it pans the map, and the left stick's up and down zooms in and out
  smoothly while held [observed: the original; the speeds are design], built in
  `pipboy/data.rs` (`stick_cursor`, `scroll_map`, `stick_zoom`). On the World Map, A (the
  confirm button) on a highlighted marker asks to fast travel there, as a click does
  [observed: the original], also in `pipboy/data.rs`.
- The prompt under the crosshair names the Activate control's own key ("E)") or, with a
  controller, its button ("A)") (`Bindings::prompt`). The original shows the button's
  picture there (`&xbuttona;`, `general_button_a.dds`); letters are the first step.
- **Menu hints follow the device** (#1662). Every hint names the key its menu answers
  ("Take All R)", "Close [E]") or, with a controller, the button that presses that key
  there (`pad::hint`, looked up in the tables above through `pad::button_for`, the
  menu's own buttons first, then the shared ones, B before Start for Esc): "Take All X)",
  "Close [B]". A hint whose keys no button presses keeps the key's name (a unit test
  lists every hint and fails on such a gap). Which names are in use is one setting for
  every menu (`pad::labels`, kept in step with Input Device each frame) [design, #1662:
  as a language is; the drawing code reads it without each menu taking the bindings].
  Built: the container, barter and "How many?" buttons, the tiled container menu's Take
  All, the Pip-Boy's Repair/Maintain, Mod, mod menu, repair list, STATS hints and
  General/Reputation switch, DATA's Challenges/Notes, crafting, repair services,
  level-up and the tag-skill/trait lists, lockpicking and every minigame's help box
  Close, blackjack, roulette, slots, caravan, poker, the Wait/Sleep menu, the dialogue's
  Goodbye hint, the pause menu's Delete and Yes/No. The help texts' control strings
  (`&sUActnCrouch;` …, `lockpick::help::expand_keys`) name the control's button. Menus
  drawn with a view kept for change detection redraw when the device changes; the others
  (the container, barter and Pip-Boy pages) take the new names at their next redraw.
  Not followed: the hint words the tiled Wait/Sleep, tutorial and message menus take
  from their files' `_PCButtonText` (opt-in `[UI] TileMenus`), and the button pictures
  (letters stand in).

## 3a. PlayStation names (#1452)

The original shows only Xbox button pictures; a PlayStation controller's names are
NVForever's own [design, #1452]:

- **Which names.** The settings page's Button style (Controller group, `button-style` in
  `[Settings]` of NVForever.ini): Auto (the default), Xbox or PlayStation (`pad::ButtonStyle`).
  Auto takes the PlayStation names while the controller in use reports a PlayStation
  name (`pad::playstation_pad`: "DualSense", "DualShock", "PlayStation", "Sony", "PS3",
  "PS4", "PS5", or the bare "Wireless Controller" of a DualShock 4 or DualSense; a name
  with "Xbox" or "XInput" never), else the Xbox ones. The controller in use is the last
  connected or the last whose button went down, so the names follow a pad picked up
  mid-game (`pad::PadInUse`; the log: `controller: <name> in use (PlayStation names)`).
- **The names**: A Cross, B Circle, X Square, Y Triangle, LB L1, RB R1, LT L2, RT R2,
  LS L3, RS R3, Back Select, Start Start (the PS3-era pair, the game's era, not Share/
  Create and Options); the D-pad and the stick keep their names (`pad::PLAYSTATION_NAMES`).
  They are used everywhere a button is named on screen: the crosshair prompt, the menu
  hints (§3), the Controls tab's bindings and the help texts' control strings.
- **The INI keeps the Xbox names** (`[Controller] Jump=Y`) whatever the style, so every
  existing NVForever.ini keeps loading and an INI does not change when the pad does;
  the rebinding log names them so too.
- **Glyphs**: NVForever draws its own Cross, Circle, Square and Triangle — a solid cross
  and three ring outlines — computed at start into small pictures (`pad::Glyph`, nothing
  from the game's or Sony's art). The crosshair prompt shows the glyph and ")" in place of
  "Cross)"; the menu hints, drawn with the game's fonts, which have no such symbols, write
  the names. L1 … R3, Select and Start are text everywhere.

## 4. Rumble

The original rumbles the pad by its `bGamePadRumble` switch (`1` in the owner's
`FalloutPrefs.ini` [observed 2026-10-16]); how strongly and on what is not captured, so
everything below is [design] (combat is not a parity target). Built in
`engine/nvf-play/src/rumble.rs`.

- **When:** only while Controller works the controls and `Rumble=1` in `[Controller]` of
  NVForever.ini (default 1, the game's switch). With Controller chosen the settings page's
  Controls tab has a last row **Rumble** (ON/OFF). Each rumble is a Bevy
  `GamepadRumbleRequest` sent to every connected controller (gilrs plays it); pulses of one
  frame are joined (the strongest of each motor, the longest). The first of each kind is
  logged: `rumble: first shot (strong …, weak …, … s) on N controller(s)`.
- **The player fires** (each `ShotFired` of the player; melee swings fire no projectile
  and do not rumble): by the weapon's WEAP weight `w` (pounds), `h = min(w / 20, 1)`:
  strong `0.7 h`, weak `0.3 + 0.4 h`, `0.07 + 0.13 h` s. A 9mm pistol (2) is a short buzz,
  the minigun (25) and Fat Man (30) the full thump.
- **The player takes damage** (each `ActorHit` on the player that took health): by the
  share `s` of the maximum health it took, `t = min(4 s, 1)`: strong `0.35 + 0.65 t`, weak
  `0.5`, `0.15 + 0.30 t` s.
- **A hard landing** (the physics step's touchdown speed; fall damage is physics.md §6.12):
  from 600 units/s (a drop of about 260 units under gravity 687 units/s²; a jump lands at
  about 290), `t` up to 1 at 1200 (a drop of about 1050): strong `0.45 + 0.55 t`, weak
  `0.3`, `0.15 + 0.25 t` s.
- **An explosion near the player** (each `ExplosionFx`): full inside the EXPL's radius,
  fading to none 1400 units (about 20 m) beyond it, `t`: strong `t`, weak `0.6 t`,
  `0.25 + 0.35 t` s; none under `t = 0.05`.

## 5. Aim assist

Built in `engine/nvf-play/src/aim_assist.rs` (#1196, #1197). Controller only: with
Keyboard and Mouse chosen nothing changes, and the mouse never gets either part. All of it
is [design] (combat is not a parity target): the original's auto aim (`fAutoAim*`) bends
shots toward a target near the crosshair and leaves the view alone [documented: GECK wiki
"Auto Aim Settings", owner's export]; NVForever takes its range and on-target area from
those settings so a mod that changes them changes this too.

- **The target**, shared by both parts: a living actor drawn now, hostile to the player
  (fighting it, or would attack it: the rule that turns the crosshair red), within
  `fAutoAimMaxDistance` (1800) of the crosshair, inside a cone around the crosshair whose
  radius is `fAutoAimScreenPercentage` (10) % of the screen's width at a 75° field of view
  (about 8.7°) [hypothesis: how the original measures the percentage], and seen: a ray from
  the crosshair to its aim point meets it first, passing what shots pass (water, triggers,
  NPCs' walking capsules: a capsule is not the body), so no lock through walls. The nearest to the
  crosshair by angle wins; the aim point is the middle of its posed torso part, else 80
  units over its feet.
- **Slowdown** (#1196): while the crosshair is within 40 units of a target's aim point (as
  seen from the crosshair), the right stick's turn is scaled by `1 − 0.75 × strength`
  (45% slower at the default 0.6), eased in and out over 0.1 s. Friction only, no pull.
- **Lock-on** (#1197): when aiming down the sights begins (LT with the weapon out) and a
  target is picked, the view turns toward its aim point for 0.3 s, easing in with a time
  constant of 0.08 s / strength and at most 4 × strength radians a second, following the
  target as it moves. It lets go at the end, when the target dies or leaves sight, range or
  1.5 times the cone, when aiming ends, or when the right stick is pushed at least 0.7 of
  the way against the pull. A new lock needs 0.6 s since the last began.
- **Not while** the free camera, the weapon wheel, a kill camera, VATS mode, an animation
  driving the view, furniture holding the player, a script's look lock or a circling or
  detached camera has the look, without the cursor grabbed, or outside play.
- **Settings**: the settings page's Controller group, kept in `[Settings]` of
  NVForever.ini: `aim-assist` (on/off, on), `aim-slowdown` and `aim-lock` (strengths 0 to
  1, 0.6 each; 0 turns that part off).
- The log says when a lock starts and how it ends (`aim assist: lock-on to 0x…`).

## 6. Not built yet

- The button pictures from `Interface\Shared\Button\` in place of letters.
- The ITEMS page's RB Hotkey (§3's "not followed"); the tiled menus' file hints
  following the device (§3).
- A pointer moved by a stick for the mouse-only parts of menus still left: the dialogue
  list (the Pip-Boy map's stick cursor and zoom are built, §3).
- Several controllers told apart for the controls (any pad's buttons work them); only the
  button names follow the pad in use (§3a).
