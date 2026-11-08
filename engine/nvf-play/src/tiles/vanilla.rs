//! T12a: the vanilla HUD drawn from its tiles, behind a switch [design: ui-tile-tree.md
//! §6.2 step 3; hud-xml.md step 5].
//!
//! - **The switch**: `[UI] TileMenus=` in the home's `NVForever.ini`, a comma-separated
//!   list of menu names, empty by default; `HUDMainMenu`, and since T15a `MessageMenu`
//!   and `QuantityMenu` (`super::menus`; the prompt also follows a container menu drawn from
//!   its tiles, `super::menus::quantity_tiled`), and since T11 `StartMenu` (the pause menu's
//!   Mods page, `crate::pause::ModsEntry`, `super::mods`), `TraitMenu` (`super::trait_menu`), `CharGenMenu` (tag skills, `super::tag_menu`),
//!   `StatsMenu` (the Pip-Boy's S.P.E.C.I.A.L. tab, `super::stats_menu`) and `DialogMenu`
//!   (`super::dialog_menu`) are acted on. It is
//!   read once at start, from the INI the settings page uses
//!   (`crate::settings::SettingsFile`: an unattended run in the player's own home reads
//!   none). The test aid `NVF_PLAY_TEST_TILE_MENUS=<list>` sets it instead, so a smoke
//!   scenario never writes the player's INI. [`TileMenus`] holds it.
//! - **With the HUD a tile menu**, `super::draw` draws the whole `HUDMainMenu` tree (the
//!   file's tiles, the code-made copies, the mods'; `super::draw::Select::Whole`) and
//!   the hand-built pieces that have a tile-drawn equivalent are hidden ([`DrawnByTiles`],
//!   [`hide_hand_built`]). The pieces whose tiles the engine does not fill yet stay
//!   hand-built and their file and code tiles are not drawn ([`KEPT_PIECES`], none since
//!   the breath meter's `nif` tile is drawn, `super::model`); the mapping is in
//!   docs/spec-notes/hud.md §14.7.
//! - [`write_corners`] gives the corner labels' code-made copies the places the
//!   hand-built HUD sets them at (the captures give none), the ammo type's label its
//!   abbreviation, and the condition row its showing rule, only while the switch is on
//!   [design: the tree's values for mods stay T4's when it is off].

use crate::settings::SettingsFile;
use bevy::prelude::*;
use fnv_install::ini_get;
use fnv_ui::{TileId, Ui, Value};

/// The INI section and key [design: ui-tile-tree.md §6.2].
pub const SECTION: &str = "UI";
pub const KEY: &str = "TileMenus";
/// The test aid that sets the list instead of the INI.
pub const AID: &str = "NVF_PLAY_TEST_TILE_MENUS";

/// Which menus are drawn from their tiles.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileMenus {
    /// `HUDMainMenu`: the vanilla HUD from tiles, the hand-built one hidden where a tile
    /// draws its piece.
    pub hud: bool,
    /// `MessageMenu` (T15a): a message box from its file, the hand-built box
    /// (`crate::message_menu`) not spawned (`super::menus`).
    pub message: bool,
    /// `QuantityMenu` (T15a): the "How many?" prompt of the container, barter and inventory menus from its
    /// file, the hand-built prompt (`crate::menu::look::spawn_quantity`) not spawned
    /// (`super::menus`). Off, an item menu drawn from its tiles still brings its own.
    pub quantity: bool,
    /// `StartMenu` (T11): the pause menu's Mods entry, when a mod added tiles to the
    /// start menu, opens a page drawing them (`super::mods`).
    pub start: bool,
    /// `SleepWaitMenu` (T15b): the Wait / Sleep menu from its file, the hand-built menu
    /// (`crate::wait`) not spawned (`super::wait_menu`).
    pub sleep_wait: bool,
    /// `TutorialMenu` (T15b): the tutorial menu from its file, the hand-built drawing
    /// (`crate::tutorial_menu`) not spawned (`super::tutorial_menu`).
    pub tutorial: bool,
    /// `StatsMenu`: the Pip-Boy's S.P.E.C.I.A.L. tab from its file, in the Pip-Boy's image,
    /// the other STATS tabs hand-built (`super::stats_menu`).
    pub stats: bool,
    /// `ContainerMenu` (T15c): the container menu from its file, the hand-built one
    /// (`crate::menu::look`) not spawned (`super::container_menu`), and its prompt from
    /// `QuantityMenu`'s while it is so drawn (`super::menus::quantity_tiled`).
    pub container: bool,
    /// `BarterMenu`: barter from its file, including its quantity prompt [design: section 9].
    pub barter: bool,
    /// `TraitMenu`: the trait menu from its file, the hand-built one (`crate::chargen::lists`)
    /// not spawned (`super::trait_menu`).
    pub traits: bool,
    /// [design: chargen.md §3.1] CharGenMenu tag skills only; attributes stay hand-built.
    pub tags: bool,
    /// `DialogMenu`: the dialogue menu from its file, the hand-built drawing
    /// (`crate::dialogue_menu`) not spawned (`super::dialog_menu`).
    pub dialog: bool,
}

impl TileMenus {
    /// A comma-separated list of menu names (case does not matter, blanks around a name
    /// neither): the switches, and the names not acted on.
    pub fn parse(list: &str) -> (TileMenus, Vec<String>) {
        let mut menus = TileMenus::default();
        let mut other = Vec::new();
        for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            if name.eq_ignore_ascii_case(super::draw::HUD_MENU) {
                menus.hud = true;
            } else if name.eq_ignore_ascii_case(super::menus::MESSAGE_MENU) {
                menus.message = true;
            } else if name.eq_ignore_ascii_case(super::menus::QUANTITY_MENU) {
                menus.quantity = true;
            } else if name.eq_ignore_ascii_case(super::mods::START_MENU) {
                menus.start = true;
            } else if name.eq_ignore_ascii_case(super::wait_menu::SLEEP_WAIT_MENU) {
                menus.sleep_wait = true;
            } else if name.eq_ignore_ascii_case(super::tutorial_menu::TUTORIAL_MENU) {
                menus.tutorial = true;
            } else if name.eq_ignore_ascii_case(super::stats_menu::STATS_MENU) {
                menus.stats = true;
            } else if name.eq_ignore_ascii_case(super::container_menu::CONTAINER_MENU) {
                menus.container = true;
            } else if name.eq_ignore_ascii_case(super::barter_menu::BARTER_MENU) {
                menus.barter = true;
            } else if name.eq_ignore_ascii_case(super::trait_menu::TRAIT_MENU) {
                menus.traits = true;
            } else if name.eq_ignore_ascii_case(super::tag_menu::TAG_MENU) {
                menus.tags = true;
            } else if name.eq_ignore_ascii_case(super::dialog_menu::DIALOG_MENU) {
                menus.dialog = true;
            } else {
                other.push(name.to_string());
            }
        }
        (menus, other)
    }

    /// The list for this run and where it was read: the test aid, else the INI's
    /// `[UI] TileMenus=`; `None` when neither gives one.
    pub fn list_for_run(aid: Option<String>, ini: &SettingsFile) -> Option<(String, String)> {
        if let Some(list) = aid {
            return Some((list, format!("test aid {AID}")));
        }
        let path = ini.0.as_ref()?;
        let text = std::fs::read_to_string(path).ok()?;
        ini_get(&text, SECTION, KEY).map(|list| (list, path.display().to_string()))
    }
}

/// Read the switch once at start and tell it.
pub(crate) fn setup(app: &mut App) {
    let unattended = crate::test_aid::Unattended::from_env().0;
    let list = TileMenus::list_for_run(std::env::var(AID).ok(), &SettingsFile::for_run(unattended));
    let (menus, other) = list.as_ref().map_or_else(|| (TileMenus::default(), Vec::new()), |(l, _)| TileMenus::parse(l));
    app.insert_resource(menus);
    app.add_systems(Startup, move || {
        let Some((list, from)) = &list else { return };
        info!("tiles: [{SECTION}] {KEY}={list} (from {from}): {}", menus.line());
        if !other.is_empty() {
            warn!("tiles: [{SECTION}] {KEY}: {} not drawn from tiles yet (only {} are)", other.join(", "), TileMenus::NAMES.join(", "));
        }
    });
}

impl TileMenus {
    /// The menus the switch acts on, in its order.
    pub const NAMES: [&str; 12] = [super::draw::HUD_MENU, super::menus::MESSAGE_MENU, super::menus::QUANTITY_MENU, super::wait_menu::SLEEP_WAIT_MENU, super::tutorial_menu::TUTORIAL_MENU, super::container_menu::CONTAINER_MENU, super::barter_menu::BARTER_MENU, super::trait_menu::TRAIT_MENU, super::tag_menu::TAG_MENU, super::mods::START_MENU, super::stats_menu::STATS_MENU, super::dialog_menu::DIALOG_MENU];

    /// The start-up line's end: the menus drawn from tiles, or none, and that an item menu's
    /// prompt follows it when its own switch is off (`super::menus::quantity_tiled`).
    pub fn line(&self) -> String {
        let on: Vec<&str> = Self::NAMES.into_iter().zip([self.hud, self.message, self.quantity, self.sleep_wait, self.tutorial, self.container, self.barter, self.traits, self.tags, self.start, self.stats, self.dialog]).filter(|(_, on)| *on).map(|(n, _)| n).collect();
        if on.is_empty() {
            "no menu drawn from tiles".into()
        } else if (self.container || self.barter) && !self.quantity {
            format!("{} drawn from tiles; its QuantityMenu prompt follows", on.join(", "))
        } else {
            format!("{} drawn from tiles", on.join(", "))
        }
    }
}

/// [design] Remaining hand-built pieces: none. Settled T12b pieces draw from tiles (hud.md
/// §14.8); the messages, quest notices and subtitles since their copies are written
/// (`super::notices`, hud.md §14.11); the breath meter since `nif` tiles are drawn
/// (`super::model`, hud.md §14.12). `Region_Location` is no longer kept: a discovery's
/// label and name copies are written under it (`super::notices`, hud.md §14.11)
/// [hypothesis: hud.md §14 maps discoveries to Region_Location; the capture saw no copies
/// there, so their layout is the quest banner's], hidden, as `QuestReminder`'s draw it.
/// Kept as a list for a piece that has to go back to being hand-built.
pub const KEPT_PIECES: [&str; 0] = [];

/// The start-up line telling the whole HUD is drawn from tiles, with the pieces still
/// hand-built (`kept`, [`KEPT_PIECES`]) when there are any.
pub fn drawn_line(file: usize, code: usize, kept: &[&str]) -> String {
    let but = if kept.is_empty() { String::new() } else { format!(" but for {}", kept.join(", ")) };
    format!("tiles: {} drawn from tiles ({file} file tiles, {code} code tiles); hand-built HUD hidden{but}", super::draw::HUD_MENU)
}

/// [design] Shared visibility markers moved here from the hand-built modules so the
/// tile switch can hide their roots without changing their drawing/timing logic.
#[derive(Component)]
pub(crate) struct EnemyPiece;
#[derive(Component)]
pub(crate) struct WordsRoot;
#[derive(Component)]
pub(crate) struct LimbRoot;
#[derive(Component)]
pub(crate) struct MeterRoot;
#[derive(Component)]
pub(crate) struct PromptTexts;
/// The hand-built corner messages', quest lines' and subtitles' areas
/// (`corner_messages.rs`, `quest_messages.rs`, `script_hud.rs`); their queues and clocks
/// still run, only their nodes are hidden [design].
#[derive(Component)]
pub(crate) struct MessagesArea;
#[derive(Component)]
pub(crate) struct QuestArea;
#[derive(Component)]
pub(crate) struct SubtitlesArea;
/// The hand-built breath meter's root (`hud_breath.rs`); its model is still read and its
/// ticks set, only its node is hidden [design].
#[derive(Component)]
pub(crate) struct BreathRoot;

/// The compass window, whose picture is the strip the heading moves [observed: hud.md
/// §14.4, the name read; §14.5].
pub const STRIP_TILE: &str = "HUDMainMenu/HitPoints/compass_window";

/// The [`KEPT_PIECES`] the tree has.
pub fn kept_pieces(ui: &Ui) -> Vec<TileId> {
    KEPT_PIECES.iter().filter_map(|p| ui.find_tile(&format!("{}/{p}", super::draw::HUD_MENU))).collect()
}

/// A hand-built HUD node whose piece the tiles draw while the HUD is a tile menu: the
/// health and action points corners (with the compass, the condition row and the ammo),
/// the sneak word, the crosshair, the radiation meter. The other pieces' roots carry their
/// own markers ([`EnemyPiece`], [`WordsRoot`], [`LimbRoot`], [`MeterRoot`],
/// [`PromptTexts`], [`MessagesArea`], [`QuestArea`], [`SubtitlesArea`], [`BreathRoot`]).
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct DrawnByTiles;

/// What [`hide_hand_built`] hides of the hand-built HUD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hide {
    /// The pieces the tiles draw ([`DrawnByTiles`] and the other roots' markers), the
    /// subtitles aside.
    pub pieces: bool,
    /// The hand-built subtitles ([`SubtitlesArea`]).
    pub subtitles: bool,
}

/// What to hide: with the HUD a tile menu and the tree having it (`loaded`), the pieces the
/// tiles draw; the subtitles only while the tile renderer draws something of the HUD
/// (`root_drawn`, [`root_drawn`]). In a menu the renderer draws nothing and the hand-built
/// subtitles stay, so a line said while a message box is up still shows [design: hud.md
/// §14.11; maintainer's review of #952].
pub fn hide(menus: TileMenus, loaded: bool, root_drawn: bool) -> Hide {
    let on = menus.hud && loaded;
    Hide { pieces: on, subtitles: on && root_drawn }
}

/// Whether the tile renderer draws anything of the HUD menu `menu`: the menu tile live,
/// visible and not transparent, as `super::layout::layout` passes them down to every tile
/// under it (`fnv_game::ui_hud::write` sets its `visible` from `Shown::root`, 0 in a menu)
/// [design].
pub fn root_drawn(ui: &Ui, menu: TileId) -> bool {
    let tile = ui.tile(menu);
    tile.live && tile.get("visible").is_none_or(Value::truthy) && tile.get("alpha").map_or(255.0, Value::num) > 0.0
}

/// Hide [`DrawnByTiles`] nodes while the HUD is a tile menu and the tree has it (without
/// the game's menu files the hand-built HUD stays), and the hand-built subtitles while the
/// tile renderer draws something ([`hide`]): after every drawing system has set its
/// visibility, before it is passed down [design: no drawer changes]. The subtitles' area
/// sets no visibility of its own, so it is given back (`Inherited`) when the tiles draw
/// nothing.
#[allow(clippy::type_complexity)]
pub(crate) fn hide_hand_built(
    menus: Res<TileMenus>,
    script: Res<crate::script::ScriptState>,
    mut nodes: Query<&mut Visibility, (Or<(With<DrawnByTiles>, With<EnemyPiece>, With<WordsRoot>, With<LimbRoot>, With<MeterRoot>, With<PromptTexts>, With<MessagesArea>, With<QuestArea>, With<BreathRoot>)>, Without<SubtitlesArea>)>,
    mut subtitles: Query<&mut Visibility, With<SubtitlesArea>>,
) {
    let menu = script.rt.as_ref().filter(|rt| rt.game.ui_files.is_some()).and_then(|rt| rt.game.ui_tree.menu(super::draw::HUD_MENU).map(|m| (&rt.game.ui_tree, m)));
    let hide = hide(*menus, menu.is_some(), menu.is_some_and(|(ui, m)| root_drawn(ui, m)));
    // Before any early return: the area has no drawer of its own to show it again, so
    // it would stay hidden once the tree lost the menu (a load's rebuild).
    subtitles.iter_mut().for_each(|mut v| {
        v.set_if_neq(subtitles_visibility(hide));
    });
    if !hide.pieces {
        return;
    }
    for mut v in &mut nodes {
        v.set_if_neq(Visibility::Hidden);
    }
}

/// The hand-built subtitles' area's visibility for a [`Hide`]: given back (`Inherited`)
/// whenever it is not hidden, the switch off or the menu gone included.
pub fn subtitles_visibility(hide: Hide) -> Visibility {
    if hide.subtitles {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    }
}

/// What [`write_corners`] writes, in the corner pieces' units.
#[derive(Clone, Debug, PartialEq)]
pub struct Corner {
    /// "HP"'s left and top; the right ends and tops of "AP", "CND" and the ammo counter
    /// (right-justified copies).
    pub hp: (f64, f64),
    pub ap: (f64, f64),
    pub cnd: (f64, f64),
    pub ammo: (f64, f64),
    /// The right bracket's x in `ActionPoints`.
    pub right_bracket: f64,
    /// The ammo type's abbreviation, empty without one.
    pub ammo_label: String,
    /// How far the condition row ("CND", the ground, the fill, `CNDArrows`) moves left for
    /// the abbreviation ([`condition_shift`]).
    pub condition_shift: f64,
    /// A weapon with a condition is equipped.
    pub condition: bool,
}

/// Between the condition row's right end and the ammo abbreviation's left [observed:
/// hud_compare hud-ammo-label at 1280 x 720: with "HP" (30 units wide, as `HitPoints`'
/// "HP" reads, hud.md §14) the original's "CND" stands at x 984 and its bar's right end
/// at 1064-1065 pixels, against 1021 and 1102 with no abbreviation: 49-51 units left; the
/// ground's right end, 150 of `ActionPoints` without one, is `AmmoTypeLabel`'s right end
/// W - 236; hypothesis: the gap is fixed and the row moves by the abbreviation's width,
/// one abbreviation measured].
pub const LABEL_ROOM: f64 = 20.0;

/// How far left the condition row moves for an ammo abbreviation `width` units wide
/// (`AmmoTypeLabel` right-justified at its captured place, the row ending [`LABEL_ROOM`]
/// before the abbreviation): none without one [observed: the original's row at x 90 of
/// `ActionPoints` with no abbreviation, hud.md §14; see [`LABEL_ROOM`]].
pub fn condition_shift(label: &str, width: f64) -> f64 {
    if label.trim().is_empty() {
        0.0
    } else {
        width + LABEL_ROOM
    }
}

/// The right bracket's x in `ActionPoints`: the hand-built piece's 40 in from the view's
/// edge, less the atlas piece's 4 units (`hud::RIGHT_BRACKET_X`) [observed: hud_compare at
/// 1280 x 720, the divider, upright and step cap 3 pixels left of the original's at -40,
/// T12a's hypothesis].
pub const RIGHT_BRACKET_X: f64 = -(crate::hud::MARGIN_X as f64) + crate::hud::RIGHT_BRACKET_X as f64;

/// The condition ground's and fill's x in `ActionPoints` with no abbreviation [observed:
/// hud.md §14], and `CNDArrows`' x from `ActionPoints`' (W - 253 against W - 386).
const CONDITION_X: f64 = 90.0;
const ARROWS_X: f64 = 133.0;

fn set(ui: &mut Ui, path: &str, name: &str, value: Value) {
    if let Some(tile) = ui.find_tile(&format!("{}/{path}", super::draw::HUD_MENU)) {
        ui.set_trait(tile, name, value);
    }
}

/// The condition ground's and fill's brightness, the hand-built HUD's
/// (`hud::GROUND_BRIGHTNESS`, `hud::CONDITION_BRIGHTNESS`) [observed: the original's ground
/// an opaque 100/255 and its fill 175/255 of the HUD's colour, private shots
/// `hud_corners2` c0, c4 (T12c)]. Until T12e the ground at alpha 64 and the fill at 255
/// [design].
pub const GROUND_BRIGHTNESS: f64 = crate::hud::GROUND_BRIGHTNESS as f64;
pub const CONDITION_BRIGHTNESS: f64 = crate::hud::CONDITION_BRIGHTNESS as f64;

/// The condition fill's `zoom`: its picture's drawn block (11 of `vats_bar.dds`'s 16 rows)
/// as high as its 20-unit box [observed: the texture's opaque block is 74 x 11 texels at
/// its top left; the original's screenshots `hud_corners2` c0 and c4 show the fill solid
/// over all 20 units; design: tiled at a texel a unit the picture's clear rows 11-15 drew
/// the fill as two bands (T12b), how the original fills the box from this picture is not
/// traced (its `zoom`, `cropx`, `cropy` were not read), and the hand-built fill is solid].
pub const CONDITION_FILL_ZOOM: f64 = 100.0 * 20.0 / 11.0;

/// Write what the tile-drawn corners need beyond T4's writes, before scripts, only while
/// the HUD is a tile menu [design: ui-tile-tree.md §4.1]:
/// - the corner labels' copies (`HitPoints/justify_left_text`,
///   `ActionPoints/justify_right_text:0`-`:2`, "CND", the ammo counter, "AP") where the
///   hand-built HUD sets its labels, and `ActionPoints/right_bracket` where it draws its
///   bracket [design: hud.rs `label_places`, the captures give none of these places,
///   hud.md §14.6; hypothesis: the original's right bracket stands where the hand-built
///   one does on the screen, hud.md §14 "the current HUD's right corner still lands where
///   the original's does"];
/// - `AmmoTypeLabel`'s string, the weapon's ammo abbreviation [hypothesis: by the tile's
///   name and place, the right piece's lower row; its string was not read, hud.md §14.2],
///   the tile left at T4's captured place (W - 236, 883, right-justified: its right end
///   just left of the divider), and the condition row moved left to make room for it
///   ([`condition_shift`]) [observed: hud_compare hud-ammo-label, the original's "HP" at
///   x 1085-1104 pixels, its right end W - 236, the divider at 1114]. T12c set it right
///   of the divider, where the hand-built HUD sets it at the owner's request [design];
/// - the condition fill's `zoom` ([`CONDITION_FILL_ZOOM`]) and the row's brightness
///   ([`CONDITION_BRIGHTNESS`], [`GROUND_BRIGHTNESS`]);
/// - the health and action points bars' `brightness`, the hand-built bars'
///   (`hud::METER_BRIGHTNESS`, 175) [hypothesis: `brightness.xml`'s "meter and compass in
///   HUD", the enemy bar's meter's read 175; the original's ticks are about 0.7 of its
///   lines' colour on `hud_corners2`, hud.md §14.9; their own `brightness` was not read]
///   (T12d);
/// - the condition row (the ground, "CND", `CNDArrows`) shown by alpha only with a
///   weapon's condition, as the hand-built HUD hides its row [design].
pub fn write_corners(ui: &mut Ui, c: &Corner) {
    for (path, (x, y)) in [
        ("HitPoints/justify_left_text", c.hp),
        ("ActionPoints/justify_right_text:0", c.cnd),
        ("ActionPoints/justify_right_text:1", c.ammo),
        ("ActionPoints/justify_right_text:2", c.ap),
    ] {
        set(ui, path, "x", Value::Num(x));
        set(ui, path, "y", Value::Num(y));
    }
    set(ui, "ActionPoints/right_bracket", "x", Value::Num(c.right_bracket));
    set(ui, "AmmoTypeLabel", "string", Value::Str(c.ammo_label.clone()));
    // The condition row, moved left for the abbreviation.
    let shift = c.condition_shift;
    set(ui, "ActionPoints/justify_right_text:0", "x", Value::Num(c.cnd.0 - shift));
    for path in ["ActionPoints/MeterBackground", "ActionPoints/meter:0"] {
        set(ui, path, "x", Value::Num(CONDITION_X - shift));
    }
    let piece_x = ui.get_float(&format!("{}/ActionPoints/x", super::draw::HUD_MENU)).unwrap_or(0.0);
    set(ui, "CNDArrows", "x", Value::Num(piece_x + ARROWS_X - shift));
    set(ui, "ActionPoints/meter:0", "zoom", Value::Num(CONDITION_FILL_ZOOM));
    set(ui, "ActionPoints/meter:0", "brightness", Value::Num(CONDITION_BRIGHTNESS));
    set(ui, "ActionPoints/MeterBackground", "brightness", Value::Num(GROUND_BRIGHTNESS));
    for path in ["HitPoints/meter", "ActionPoints/meter:1"] {
        set(ui, path, "brightness", Value::Num(f64::from(crate::hud::METER_BRIGHTNESS)));
    }
    let on = |alpha: f64| Value::Num(if c.condition { alpha } else { 0.0 });
    set(ui, "ActionPoints/MeterBackground", "alpha", on(255.0));
    set(ui, "ActionPoints/justify_right_text:0", "alpha", on(255.0));
    set(ui, "CNDArrows", "alpha", on(255.0));
}

/// The breath meter's tile [documented: hud_main_menu.xml, a `nif` tile].
pub const BREATH_TILE: &str = "BreathMeter";

/// Write what the tile-drawn breath meter needs, only while the HUD is a tile menu
/// [design: off, the tree stays T4's]:
/// - its `alpha`, 255 while the hand-built meter would show (`hud_breath::shown`: the head
///   under water, or the breath not yet whole) and 0 otherwise [hypothesis: as the sneak
///   word's and the radiation meter's children, whose alpha goes 0 to 255 while they show,
///   hud.md §14.1; the file sets the tile's alpha 0; the meter's own alpha under water was
///   not read];
/// - `_NVFSequenceFraction`, how far into its `animation` the model stands (0 its start, 1
///   its end): the share of the breath used, as the hand-built meter plays `Forward`
///   (`hud_breath::time_at`) [hypothesis: drowning.md §3; how the original's code drives
///   the model's sequence is not known, no trait of the file says it; the name is
///   NVForever's own, `super::model::FRACTION`].
pub fn write_breath(ui: &mut Ui, shown: bool, used: f64) {
    set(ui, BREATH_TILE, "alpha", Value::Num(if shown { 255.0 } else { 0.0 }));
    set(ui, BREATH_TILE, super::model::FRACTION, Value::Num(used.clamp(0.0, 1.0)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    #[test]
    fn the_start_line_names_the_kept_pieces_only_when_there_are_any() {
        assert_eq!(drawn_line(900, 120, &KEPT_PIECES), "tiles: HUDMainMenu drawn from tiles (900 file tiles, 120 code tiles); hand-built HUD hidden");
        assert_eq!(drawn_line(1, 2, &["BreathMeter"]), "tiles: HUDMainMenu drawn from tiles (1 file tiles, 2 code tiles); hand-built HUD hidden but for BreathMeter");
    }

    #[test]
    fn the_hand_built_subtitles_stay_while_the_tiles_draw_nothing() {
        let on = TileMenus { hud: true, ..TileMenus::default() };
        // The HUD drawn from tiles and shown: every hand-built piece hidden.
        assert_eq!(hide(on, true, true), Hide { pieces: true, subtitles: true });
        // In a menu the tile root draws nothing: the hand-built subtitles stay.
        assert_eq!(hide(on, true, false), Hide { pieces: true, subtitles: false });
        // The switch off, or no menu files: nothing hidden.
        for drawn in [true, false] {
            assert_eq!(hide(TileMenus::default(), true, drawn), Hide { pieces: false, subtitles: false });
            assert_eq!(hide(on, false, drawn), Hide { pieces: false, subtitles: false });
        }
        // Hidden only while the tiles draw them; given back otherwise, the menu gone (a
        // load's rebuild) included, where nothing else would show the area again.
        assert_eq!(subtitles_visibility(hide(on, true, true)), Visibility::Hidden);
        assert_eq!(subtitles_visibility(hide(on, false, true)), Visibility::Inherited);
        assert_eq!(subtitles_visibility(hide(on, true, false)), Visibility::Inherited);
        // The root's `visible` (written from `Shown::root`) and alpha decide whether
        // anything of the menu is drawn.
        let src = MemSource::default().with("hud.xml", r#"<menu name="HUDMainMenu"><text name="Subtitles"><string>Line</string></text></menu>"#);
        let mut ui = Ui::new();
        let menu = ui.load_menu(&src, "hud.xml").unwrap();
        assert!(root_drawn(&ui, menu));
        ui.set_trait(menu, "visible", Value::Num(0.0));
        assert!(!root_drawn(&ui, menu));
        ui.set_trait(menu, "visible", Value::Num(1.0));
        assert!(root_drawn(&ui, menu));
        ui.set_trait(menu, "alpha", Value::Num(0.0));
        assert!(!root_drawn(&ui, menu));
    }

    #[test]
    fn the_breath_meter_shows_by_alpha_and_stands_in_its_sequence_by_the_breath_used() {
        let src = MemSource::default().with(
            "hud.xml",
            r#"<menu name="HUDMainMenu"><nif name="BreathMeter"><filename>Interface\HUD\AirTimer01.NIF</filename><animation>Forward</animation>
              <visible>&false;</visible><alpha>0</alpha><width>64</width><height>64</height><systemcolor>&hudmain;</systemcolor></nif></menu>"#,
        );
        let mut ui = Ui::new();
        ui.load_menu(&src, "hud.xml").unwrap();
        let f = |ui: &Ui, t: &str| ui.get_float(&format!("HUDMainMenu/BreathMeter/{t}")).unwrap();
        // The file's alpha 0 and no fraction until written.
        assert_eq!(f(&ui, "alpha"), 0.0);
        assert!(ui.get_float(&format!("HUDMainMenu/BreathMeter/{}", super::super::model::FRACTION)).is_none());
        write_breath(&mut ui, true, 0.25);
        assert_eq!((f(&ui, "alpha"), f(&ui, super::super::model::FRACTION)), (255.0, 0.25));
        // Hidden: alpha 0, the place in the sequence still written; held to 0..=1.
        write_breath(&mut ui, false, 1.5);
        assert_eq!((f(&ui, "alpha"), f(&ui, super::super::model::FRACTION)), (0.0, 1.0));
        // A tree without the tile gets nothing.
        let mut empty = Ui::new();
        empty.load_menu(&MemSource::default().with("hud.xml", r#"<menu name="HUDMainMenu"/>"#), "hud.xml").unwrap();
        write_breath(&mut empty, true, 0.5);
        assert!(empty.find_tile("HUDMainMenu/BreathMeter").is_none());
    }

    #[test]
    fn the_switch_is_a_list_of_menu_names() {
        let hud = TileMenus { hud: true, ..TileMenus::default() };
        assert_eq!(TileMenus::parse(""), (TileMenus::default(), Vec::new()));
        assert_eq!(TileMenus::parse("HUDMainMenu"), (hud, Vec::new()));
        assert_eq!(TileMenus::parse(" hudmainmenu , PipBoyMenu,,"), (hud, vec!["PipBoyMenu".to_string()]));
        // T11: the start menu's Mods page.
        let start = TileMenus { start: true, ..TileMenus::default() };
        assert_eq!(TileMenus::parse(" startmenu "), (start, Vec::new()));
        assert_eq!(start.line(), "StartMenu drawn from tiles");
        assert_eq!(hud.line(), "HUDMainMenu drawn from tiles");
        assert_eq!(TileMenus::default().line(), "no menu drawn from tiles");
        // T15a: the message box and the prompt, each on its own.
        let (menus, other) = TileMenus::parse("messagemenu,QuantityMenu, SleepWaitMenu");
        assert_eq!(menus, TileMenus { message: true, quantity: true, sleep_wait: true, ..TileMenus::default() });
        assert!(other.is_empty());
        assert_eq!(menus.line(), "MessageMenu, QuantityMenu, SleepWaitMenu drawn from tiles");
        let (stats, other) = TileMenus::parse("statsmenu");
        assert_eq!((stats, other.is_empty()), (TileMenus { stats: true, ..TileMenus::default() }, true));
        assert_eq!(stats.line(), "StatsMenu drawn from tiles");
        // T15b: the Wait / Sleep and tutorial menus; a menu not built yet stays a name not
        // acted on.
        let t15b = TileMenus { sleep_wait: true, tutorial: true, ..TileMenus::default() };
        assert_eq!(TileMenus::parse("sleepwaitmenu, TutorialMenu, InventoryMenu"), (t15b, vec!["InventoryMenu".to_string()]));
        assert_eq!(t15b.line(), "SleepWaitMenu, TutorialMenu drawn from tiles");
        assert!(!TileMenus::parse("sleepwaitmenu").0.tutorial, "off unless named");
        assert_eq!(TileMenus::parse("QuantityMenu").0, TileMenus { quantity: true, ..TileMenus::default() });
        // [design] Each item menu can bring its own prompt.
        let barter = TileMenus { barter: true, ..TileMenus::default() };
        assert_eq!(TileMenus::parse(" barterMENU "), (barter, Vec::new()));
        assert_eq!(barter.line(), "BarterMenu drawn from tiles; its QuantityMenu prompt follows");
        assert!(!TileMenus::default().barter);
        let container = TileMenus { container: true, ..TileMenus::default() };
        assert_eq!(TileMenus::parse("containermenu"), (container, Vec::new()));
        assert_eq!(container.line(), "ContainerMenu drawn from tiles; its QuantityMenu prompt follows");
        let both = (TileMenus { container: true, quantity: true, barter: true, ..TileMenus::default() }, vec![]);
        assert_eq!(TileMenus::parse("ContainerMenu,QuantityMenu,BarterMenu"), both);
        assert_eq!(both.0.line(), "QuantityMenu, ContainerMenu, BarterMenu drawn from tiles");
        // [design] Each character selection page is independently opt-in.
        let traits = TileMenus { traits: true, ..TileMenus::default() };
        assert_eq!(TileMenus::parse("traitmenu, CharGenMenu"), (TileMenus { tags: true, ..traits }, Vec::new()));
        assert_eq!(TileMenus::parse(" chargenmenu "), (TileMenus { tags: true, ..Default::default() }, Vec::new()));
        assert!(!TileMenus::default().tags);
        assert_eq!(traits.line(), "TraitMenu drawn from tiles");
        // The dialogue menu, on its own and off unless named.
        let dialog = TileMenus { dialog: true, ..TileMenus::default() };
        assert_eq!(TileMenus::parse(" dialogmenu "), (dialog, Vec::new()));
        assert_eq!(dialog.line(), "DialogMenu drawn from tiles");
        assert!(!TileMenus::parse("HUDMainMenu,MessageMenu").0.dialog);
    }

    #[test]
    fn the_switch_comes_from_the_aid_else_the_ini() {
        let dir = std::env::temp_dir().join(format!("nvf-tile-menus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("NVForever.ini");
        std::fs::write(&path, "[Game]\nPath=F:\\FNV\n\n[UI]\nTileMenus=HUDMainMenu\n").unwrap();
        let ini = SettingsFile(Some(path.clone()));
        let (list, from) = TileMenus::list_for_run(None, &ini).unwrap();
        assert_eq!((list.as_str(), from), ("HUDMainMenu", path.display().to_string()));
        // The test aid wins, even when empty; no INI and no aid give none.
        assert_eq!(TileMenus::list_for_run(Some(String::new()), &ini).unwrap().0, "");
        assert!(TileMenus::list_for_run(None, &SettingsFile(None)).is_none());
        std::fs::write(&path, "[Settings]\ngrass=1\n").unwrap();
        assert!(TileMenus::list_for_run(None, &ini).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_corners_take_the_hand_built_places_and_the_condition_row_its_rule() {
        let src = MemSource::default().with(
            "hud.xml",
            r#"<menu name="HUDMainMenu"><rect name="HitPoints"/><rect name="ActionPoints"><x>1150</x><y>803</y></rect><image name="CNDArrows"/><text name="AmmoTypeLabel"/>
              <template name="template_justify_left_text"><text name="justify_left_text"><x>0</x></text></template>
              <template name="template_justify_right_text"><text name="justify_right_text"><x>0</x></text></template>
              <template name="template_meter"><image name="meter"><tile>&true;</tile></image></template>
              <template name="template_meter_background"><image name="MeterBackground"/></template>
              <template name="template_right_bracket"><image name="right_bracket"/></template></menu>"#,
        );
        let mut ui = Ui::new();
        ui.load_menu(&src, "hud.xml").unwrap();
        for (at, template) in [
            ("HUDMainMenu/HitPoints", "template_justify_left_text"),
            ("HUDMainMenu/HitPoints", "template_meter"),
            ("HUDMainMenu/ActionPoints", "template_meter_background"),
            ("HUDMainMenu/ActionPoints", "template_meter"),
            ("HUDMainMenu/ActionPoints", "template_justify_right_text"),
            ("HUDMainMenu/ActionPoints", "template_justify_right_text"),
            ("HUDMainMenu/ActionPoints", "template_justify_right_text"),
            ("HUDMainMenu/ActionPoints", "template_right_bracket"),
        ] {
            ui.add_code_tile_from_template(&src, at, template, None).unwrap();
        }
        let mut c = Corner { hp: (23.0, 10.0), ap: (315.0, 10.0), cnd: (79.0, 83.0), ammo: (317.0, 83.0), right_bracket: RIGHT_BRACKET_X, ammo_label: String::new(), condition_shift: 0.0, condition: true };
        write_corners(&mut ui, &c);
        let f = |ui: &Ui, p: &str| ui.get_float(&format!("HUDMainMenu/{p}")).unwrap();
        // No abbreviation: the condition row at its captured places; the condition fill's
        // picture zoomed to fill its box, the row at the original's brightness (T12e).
        assert_eq!((f(&ui, "ActionPoints/MeterBackground/x"), f(&ui, "ActionPoints/meter/x"), f(&ui, "CNDArrows/x")), (90.0, 90.0, 1283.0));
        assert_eq!(f(&ui, "ActionPoints/meter/zoom"), CONDITION_FILL_ZOOM);
        assert_eq!((f(&ui, "ActionPoints/meter/brightness"), f(&ui, "ActionPoints/MeterBackground/brightness")), (175.0, 100.0));
        // T12e: "HP" (30 wide) moves the row 50 left, as the original's; the label keeps
        // the place T4 writes (none in this tree).
        c.ammo_label = "HP".into();
        c.condition_shift = condition_shift(&c.ammo_label, 30.0);
        write_corners(&mut ui, &c);
        assert_eq!(c.condition_shift, 50.0);
        assert_eq!((f(&ui, "ActionPoints/MeterBackground/x"), f(&ui, "ActionPoints/meter/x"), f(&ui, "CNDArrows/x")), (40.0, 40.0, 1233.0));
        assert_eq!(f(&ui, "ActionPoints/justify_right_text:0/x"), 29.0);
        assert!(ui.get_float("HUDMainMenu/AmmoTypeLabel/x").is_none());
        assert_eq!(ui.get_string("HUDMainMenu/AmmoTypeLabel/string").as_deref(), Some("HP"));
        assert_eq!(condition_shift(" ", 13.0), 0.0);
        c.ammo_label.clear();
        c.condition_shift = 0.0;
        write_corners(&mut ui, &c);
        // The health bar's ticks at the meters' brightness (T12d).
        assert_eq!(f(&ui, "HitPoints/meter/brightness"), 175.0);
        assert_eq!((f(&ui, "HitPoints/justify_left_text/x"), f(&ui, "HitPoints/justify_left_text/y")), (23.0, 10.0));
        assert_eq!(f(&ui, "ActionPoints/justify_right_text:0/x"), 79.0);
        assert_eq!(f(&ui, "ActionPoints/justify_right_text:1/x"), 317.0);
        assert_eq!(f(&ui, "ActionPoints/justify_right_text:2/x"), 315.0);
        assert_eq!(f(&ui, "ActionPoints/right_bracket/x"), -36.0);
        assert_eq!(ui.get_string("HUDMainMenu/AmmoTypeLabel/string").as_deref(), Some(""));
        assert_eq!((f(&ui, "ActionPoints/MeterBackground/alpha"), f(&ui, "CNDArrows/alpha")), (255.0, 255.0));
        c.condition = false;
        write_corners(&mut ui, &c);
        assert_eq!((f(&ui, "ActionPoints/MeterBackground/alpha"), f(&ui, "ActionPoints/justify_right_text:0/alpha"), f(&ui, "CNDArrows/alpha")), (0.0, 0.0, 0.0));
    }
}
