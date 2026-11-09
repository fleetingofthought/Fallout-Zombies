//! SUP's HUD bars (task T9, docs/spec-notes/menu-xml.md §9): meters a script makes on the
//! HUD "without creating and injecting XML files", each a structure of four tiles,
//! "Meter" (image), "Frame" (image), "Text" (text) and "ImageEx" (image) [documented:
//! GECK wiki, SUP HUD Bar Functions]. NVForever keeps each bar here and writes it into a
//! `rect` it injects into `HUDMainMenu` (so the renderer draws it as a mod's tiles); the
//! bar is written again after every change and after the tree is built again.
//!
//! Bars belong to the calling plugin: two mods may name bars alike; called from the
//! console, the first bar of the name is taken [documented: the category page]. Built:
//! the bars' making, values, look and debug output. Not built (menu-xml.md §9): the
//! visibility conditions and their gradual vanishing (bar traits 0 and 21-64 are kept and
//! read back, not acted on), the texture sizes read from the files. The script-variable
//! and UDF values are `crate::nvse_sup7`'s. The rotation traits are written into the tiles but the tile renderer
//! turns no tile yet (`engine/nvf-play/src/tiles/layout.rs`), so HudBarSetRotation shows
//! nothing until it does.

use crate::game::Game;
use crate::ui_more::{finite, num, opt, text};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_ui::{MemSource, Source, TileId, Value};
use std::collections::BTreeMap;

/// The menu the bars are made in [design: SUP's page names the HUD; its tiles are not
/// documented].
pub const HUD: &str = "HUDMainMenu";

/// The elements by number: 0 Meter, 1 Frame, 2 Text, 3 ImageEx; 10 is all of them (or
/// the bar, for HudBarSetPos and HudBarGetVisible) [documented: the HudBarSet… pages].
pub const ELEMENTS: [&str; 4] = ["Meter", "Frame", "Text", "ImageEx"];
pub const ALL: i64 = 10;

/// One element's look.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// From the bar's place [documented: HudBarSetPos, "Elements positions are locused to
    /// HUD Bar position"].
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub alpha: f64,
    /// 100 [documented: HudBarSetDepth, "Default depth of UI elements is 100"].
    pub depth: f64,
    pub zoom: Option<f64>,
    /// `rotateangle`, `rotateaxisx`, `rotateaxisy` [documented: HudBarSetRotation].
    pub rotate: [f64; 3],
    pub rgb: Option<[f64; 3]>,
    pub systemcolor: f64,
    pub visible: bool,
    pub texture: String,
    pub indent: f64,
    /// The drawn picture's size (HudBarSetSizeAlt).
    pub alt: (f64, f64),
    pub font: Option<f64>,
    pub justify: Option<f64>,
}

impl Element {
    /// [hypothesis] Alpha 255; the system colour 1, the interface's own (HudBarSetColor
    /// needs it set to 0 first, so it is not 0 by default [documented]); the Meter and
    /// the Text shown, the Frame and ImageEx hidden (the category page's examples call
    /// HudBarSetVisible for the Frame only).
    fn new(k: usize) -> Element {
        Element {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            alpha: 255.0,
            depth: 100.0,
            zoom: None,
            rotate: [0.0; 3],
            rgb: None,
            systemcolor: 1.0,
            visible: k == 0 || k == 2,
            texture: String::new(),
            indent: 0.0,
            alt: (0.0, 0.0),
            font: None,
            justify: None,
        }
    }

    /// Where its picture ends to the right and below, for HudBarSetAutoPos: its indent and
    /// drawn size when set, else its size [hypothesis: the page asks for both or neither].
    fn right(&self) -> f64 {
        self.x + if self.alt.0 > 0.0 { self.indent + self.alt.0 } else { self.width }
    }

    fn bottom(&self) -> f64 {
        self.y + if self.alt.1 > 0.0 { self.alt.1 } else { self.height }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    /// The calling plugin, lower case ("" from the console).
    pub owner: String,
    pub name: String,
    /// Numbers the bar's tile, `NVFHudBar<id>` [design].
    pub id: u32,
    /// 0 horizontal, 1 vertical, 2 alpha, 3 value text, 4 percentage text, 5 size
    /// [documented: the category page].
    pub kind: u8,
    pub x: f64,
    pub y: f64,
    /// Bars are hidden after creation [documented: HudBarCreate notes].
    pub shown: bool,
    pub elements: [Element; 4],
    /// The main text, the prefix and the postfix [documented: HudBarSetTextString].
    pub texts: [String; 3],
    /// HudBarSetBarTrait's traits by number.
    pub traits: BTreeMap<i64, f64>,
    /// HudBarSetMeterTrait's: alpha at 0 % and 100 %, show the max value, min and max
    /// width, min and max height [documented: HudBarSetMeterTrait; defaults 0 and 255].
    pub meter: [f64; 7],
    pub value: f64,
    pub max: f64,
    /// The meter's share, 0 to 1.
    pub share: f64,
    pub item: Option<u32>,
    pub list: Option<u32>,
}

impl Bar {
    /// The bar's rect: `HUDMainMenu/NVFHudBar<id>`.
    pub fn path(&self) -> String {
        format!("{HUD}/NVFHudBar{}", self.id)
    }

    /// A bar trait's value: the one set, else fUpdateTimer's 0.1 s [documented:
    /// HudBarSetValueScriptVar on the category page, "default is 0.1s"] or 0
    /// [hypothesis].
    pub fn bar_trait(&self, k: i64) -> f64 {
        self.traits.get(&k).copied().unwrap_or(if k == 2 { 0.1 } else { 0.0 })
    }

    /// The Text element's string: for types 3 and 4 the value [documented: "Value/MaxValue"
    /// with show-max on, "Player Inventory weight:50/210 wg" from prefix, value, max and
    /// postfix; hypothesis: type 4 is the rounded percentage between prefix and postfix,
    /// the page's example adds the "%" as a postfix], else the main text.
    pub fn text(&self) -> String {
        let n = |v: f64| Value::Num(v).text();
        match self.kind {
            3 => {
                let max = if self.meter[2] != 0.0 { format!("/{}", n(self.max)) } else { String::new() };
                format!("{}{}{}{}", self.texts[1], n(self.value), max, self.texts[2])
            }
            4 => format!("{}{}{}", self.texts[1], n((self.share * 100.0).round()), self.texts[2]),
            _ => self.texts[0].clone(),
        }
    }

    /// The Meter's look from the share: type 2's alpha between the meter traits' 0 % and
    /// 100 % values [documented: the category page's alpha example]; [hypothesis] type 0's
    /// width and type 1's height are the indent plus the share of the rest, type 5's
    /// width and height between the meter traits' minimum and maximum.
    pub fn meter(&self) -> Element {
        let mut m = self.elements[0].clone();
        let s = self.share;
        match self.kind {
            0 => m.width = m.indent + (m.width - m.indent).max(0.0) * s,
            1 => m.height = m.indent + (m.height - m.indent).max(0.0) * s,
            2 => m.alpha = self.meter[0] + (self.meter[1] - self.meter[0]) * s,
            5 => {
                m.width = self.meter[3] + (self.meter[4] - self.meter[3]) * s;
                m.height = self.meter[5] + (self.meter[6] - self.meter[5]) * s;
            }
            _ => {}
        }
        m
    }
}

#[derive(Default)]
pub struct Bars {
    pub list: Vec<Bar>,
    seq: u32,
}

/// The calling plugin's bar of a name; from the console the first of the name.
fn find(g: &Game, name: &str) -> Option<usize> {
    let owner = &g.other.plugin;
    g.ui_extra.bars.list.iter().position(|b| b.name.eq_ignore_ascii_case(name) && (owner.is_empty() || b.owner == *owner))
}

/// The id of the calling plugin's bar of a name (from the console the first of the
/// name), for the value sources of `crate::nvse_sup7`.
pub fn bar_id(g: &Game, name: &str) -> Option<u32> {
    find(g, name).map(|i| g.ui_extra.bars.list[i].id)
}

/// A value from a bar's source (SUP's HudBarSetValueScriptVar and HudBarSetValueUDF):
/// with iCalculateValueMax as HudBarSetValueFloat, the value against the max; without
/// it the raw value is the meter's share, 0.00-1.00 as HudBarSetValuePercentage's
/// [documented: "turned into percentage using <MaxValue> if iCalculateValueMax is set
/// to 1. If not - raw value will be passed"; hypothesis: "passed" as the share, the
/// value text showing the raw value]. Written only when it changed.
pub fn set_value(g: &mut Game, i: usize, v: f64, of_max: bool) {
    let b = &mut g.ui_extra.bars.list[i];
    let share = if of_max {
        if b.max != 0.0 { (v / b.max).clamp(0.0, 1.0) } else { 0.0 }
    } else {
        v.clamp(0.0, 1.0)
    };
    if b.value == v && b.share == share {
        return;
    }
    (b.value, b.share) = (v, share);
    apply(g, i);
}

/// A plugin's name by its load-order index, lower case.
fn plugin_at(g: &Game, index: f64) -> Option<String> {
    g.data.load_order.get(index.max(0.0) as usize).map(|p| p.to_ascii_lowercase())
}

/// Writes a bar into the tree, making its tiles in the HUD when they are not there; false
/// when the HUD is not loaded.
pub fn apply(g: &mut Game, i: usize) -> bool {
    let bar = g.ui_extra.bars.list[i].clone();
    let ui = &mut g.ui_tree;
    let rect = match ui.find_tile(&bar.path()) {
        Some(t) => t,
        None => {
            let xml = format!(
                "<rect name=\"NVFHudBar{}\"> <locus> &true; </locus> <image name=\"Meter\"> </image> <image name=\"Frame\"> </image> <text name=\"Text\"> </text> <image name=\"ImageEx\"> </image> </rect>",
                bar.id
            );
            let empty = MemSource::default();
            let src: &dyn Source = match g.ui_files.as_deref() {
                Some(s) => s,
                None => &empty,
            };
            if !ui.inject_text(src, HUD, &xml) {
                return false;
            }
            match ui.find_tile(&bar.path()) {
                Some(t) => t,
                None => return false,
            }
        }
    };
    ui.set_trait(rect, "x", Value::Num(bar.x));
    ui.set_trait(rect, "y", Value::Num(bar.y));
    ui.set_trait(rect, "visible", Value::from_bool(bar.shown));
    let children: Vec<TileId> = ui.tile(rect).children.clone();
    for (k, name) in ELEMENTS.iter().enumerate() {
        let Some(&t) = children.iter().find(|&&c| ui.tile(c).name == *name) else { continue };
        let e = if k == 0 { bar.meter() } else { bar.elements[k].clone() };
        let set = |ui: &mut fnv_ui::Ui, n: &str, v: Value| ui.set_trait(t, n, v);
        set(ui, "x", Value::Num(e.x));
        set(ui, "y", Value::Num(e.y));
        // The Text's size is its string's unless one was set (the layout measures a text
        // without a `width` or `height`, `engine/nvf-play/src/tiles/layout.rs`); the
        // pictures take theirs, 0 until set [design].
        if k != 2 || e.width > 0.0 {
            set(ui, "width", Value::Num(e.width));
        }
        if k != 2 || e.height > 0.0 {
            set(ui, "height", Value::Num(e.height));
        }
        set(ui, "alpha", Value::Num(e.alpha));
        set(ui, "depth", Value::Num(e.depth));
        set(ui, "visible", Value::from_bool(e.visible));
        set(ui, "systemcolor", Value::Num(e.systemcolor));
        for (r, n) in ["rotateangle", "rotateaxisx", "rotateaxisy"].iter().enumerate() {
            if e.rotate[r] != 0.0 || ui.tile(t).get(n).is_some() {
                set(ui, n, Value::Num(e.rotate[r]));
            }
        }
        if let Some(z) = e.zoom {
            set(ui, "zoom", Value::Num(z));
        }
        if let Some([r, gr, b]) = e.rgb {
            set(ui, "red", Value::Num(r));
            set(ui, "green", Value::Num(gr));
            set(ui, "blue", Value::Num(b));
        }
        if k == 2 {
            set(ui, "string", Value::Str(bar.text()));
            if let Some(f) = e.font {
                set(ui, "font", Value::Num(f));
            }
            // Justify 1 right, 2 center, 3 left, 0 unused [documented: HudBarSetTextTrait],
            // written as the menu files' constants [design].
            match e.justify.map(|j| j as i64) {
                Some(1) => set(ui, "justify", Value::Sym("right".into())),
                Some(2) => set(ui, "justify", Value::Sym("center".into())),
                Some(3) => set(ui, "justify", Value::Sym("left".into())),
                _ => {}
            }
        } else if !e.texture.is_empty() {
            set(ui, "filename", Value::Str(e.texture.clone()));
        }
    }
    true
}

/// The tree was built again: bars with iRemoveOnGameLoad (trait 1) go [documented:
/// HudBarSetBarTrait], the rest are made again in the new HUD.
pub fn rebuilt(g: &mut Game) {
    g.ui_extra.bars.list.retain(|b| b.bar_trait(1) == 0.0);
    for i in 0..g.ui_extra.bars.list.len() {
        apply(g, i);
    }
}

/// The element numbers a function takes, with 10 for all of them.
fn targets(e: i64, allowed: &[i64]) -> Vec<usize> {
    if e == ALL && allowed.contains(&ALL) {
        allowed.iter().filter(|&&k| k != ALL).map(|&k| k as usize).collect()
    } else if allowed.contains(&e) && e != ALL {
        vec![e as usize]
    } else {
        Vec::new()
    }
}

/// The debug lines of a bar (SUP writes them to SUPNVSE.log [documented:
/// HudBarGetDebugInfo]; NVForever to its own log).
fn debug_lines(b: &Bar) -> Vec<String> {
    vec![format!(
        "HUD bar {:?} of {:?}: type {}, at {} {}, shown {}, value {} of {} ({}), tile {}",
        b.name, b.owner, b.kind, b.x, b.y, b.shown, b.value, b.max, b.share, b.path()
    )]
}

/// A HudBar function by lower-case name; `None` for any other (and for
/// HudBarSetValueScriptVar and HudBarSetValueUDF, built in `crate::nvse_sup7`).
/// A non-finite number in a place, size, colour or trait writes 0; the value, max and
/// percentage setters refuse one with -1 [design: `ui_more::finite`].
pub(crate) fn call(g: &mut Game, lname: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    if !lname.starts_with("hudbar") {
        return None;
    }
    let name = text(args, 0);
    let found = find(g, &name);
    // 1 for success, -1 for failure [documented: HudBarSetBarTrait, HudBarSetColor and the
    // value setters; hypothesis for the other setters, whose pages give "success:int"].
    let ok = |b: bool| XVal::Num(if b { 1.0 } else { -1.0 });
    let zero = XVal::Num(0.0);
    // The element-wise setters: change the elements a number names, write the bar.
    let set_elements = |g: &mut Game, allowed: &[i64], f: &dyn Fn(&mut Element)| -> XVal {
        let Some(i) = found else { return ok(false) };
        let ks = targets(num(args, 1) as i64, allowed);
        if ks.is_empty() {
            return ok(false);
        }
        for k in ks {
            f(&mut g.ui_extra.bars.list[i].elements[k]);
        }
        apply(g, i);
        ok(true)
    };
    let imgs: &[i64] = &[0, 1, 3, ALL];
    let every: &[i64] = &[0, 1, 2, 3, ALL];
    let (a2, a3, a4) = (finite(args, 2), finite(args, 3), finite(args, 4));
    Some(Ok(match lname {
        // `name x y type`; false when the plugin has a bar of the name or the type is
        // not one of 0-5 [hypothesis].
        "hudbarcreate" => {
            let kind = num(args, 3);
            if found.is_some_and(|i| g.ui_extra.bars.list[i].owner == g.other.plugin) || !(0.0..=5.0).contains(&kind) {
                return Some(Ok(zero));
            }
            g.ui_extra.bars.seq += 1;
            let bar = Bar {
                owner: g.other.plugin.clone(),
                name,
                id: g.ui_extra.bars.seq,
                kind: kind as u8,
                x: finite(args, 1),
                y: finite(args, 2),
                shown: false,
                elements: [Element::new(0), Element::new(1), Element::new(2), Element::new(3)],
                texts: Default::default(),
                traits: BTreeMap::new(),
                meter: [0.0, 255.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                value: 0.0,
                // [hypothesis: 100; the page's example "d" sets 50 with no max and calls
                // it the bar above at 50 %].
                max: 100.0,
                share: 0.0,
                item: None,
                list: None,
            };
            g.ui_extra.bars.list.push(bar);
            let i = g.ui_extra.bars.list.len() - 1;
            apply(g, i);
            XVal::Num(1.0)
        }
        "hudbarexists" => XVal::Num(if found.is_some() { 1.0 } else { 0.0 }),
        "hudbardestroy" => {
            if let Some(i) = found {
                let bar = g.ui_extra.bars.list.remove(i);
                if let Some(t) = g.ui_tree.find_tile(&bar.path()) {
                    g.ui_tree.unload_tile(t);
                }
            }
            zero
        }
        // The calling plugin's bars [documented: HudBarDestroyAllBars].
        "hudbardestroyallbars" => {
            let owner = g.other.plugin.clone();
            let (gone, kept): (Vec<Bar>, Vec<Bar>) = std::mem::take(&mut g.ui_extra.bars.list).into_iter().partition(|b| b.owner == owner);
            g.ui_extra.bars.list = kept;
            for b in gone {
                if let Some(t) = g.ui_tree.find_tile(&b.path()) {
                    g.ui_tree.unload_tile(t);
                }
            }
            zero
        }
        // The calling plugin's bar names; with 1 every bar: [names, owning mods' names,
        // their numbers] [documented: HudBarGetAllBars]. A plugin not in the load order
        // numbers -1 [hypothesis].
        "hudbargetallbars" => {
            let all = num(args, 0) != 0.0;
            let owner = g.other.plugin.clone();
            let bars: Vec<Bar> = g.ui_extra.bars.list.iter().filter(|b| all || b.owner == owner).cloned().collect();
            let list = |g: &mut Game, items: Vec<XVal>| {
                let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
                XVal::Array(g.ext.new_array(ArrayKind::List, entries))
            };
            let names = list(g, bars.iter().map(|b| XVal::Str(b.name.clone())).collect());
            if all {
                let mods = list(g, bars.iter().map(|b| XVal::Str(b.owner.clone())).collect());
                let numbers: Vec<XVal> = bars.iter().map(|b| XVal::Num(g.data.load_order.iter().position(|p| p.eq_ignore_ascii_case(&b.owner)).map_or(-1.0, |n| n as f64))).collect();
                let numbers = list(g, numbers);
                list(g, vec![names, mods, numbers])
            } else {
                names
            }
        }
        // `name trait [modIndex]`: another mod's bars are read by its index [documented:
        // HudBarGetBarTrait]. 0 without the bar [hypothesis].
        "hudbargetbartrait" => {
            let i = match opt(args, 2).and_then(|m| plugin_at(g, m)) {
                Some(owner) => g.ui_extra.bars.list.iter().position(|b| b.owner == owner && b.name.eq_ignore_ascii_case(&name)),
                None => found,
            };
            XVal::Num(i.map_or(0.0, |i| g.ui_extra.bars.list[i].bar_trait(num(args, 1) as i64)))
        }
        "hudbarsetbartrait" => match found {
            Some(i) => {
                g.ui_extra.bars.list[i].traits.insert(num(args, 1) as i64, a2);
                ok(true)
            }
            None => ok(false),
        },
        "hudbargetdebuginfo" | "hudbargetdebuginfoall" => {
            let lines: Vec<String> = if lname == "hudbargetdebuginfo" { found.map(|i| debug_lines(&g.ui_extra.bars.list[i])).unwrap_or_default() } else { g.ui_extra.bars.list.iter().flat_map(debug_lines).collect() };
            crate::ui_more::emit(g, true, lines);
            zero
        }
        // The path of an element, 10 the bar's [documented: HudBarGetElementUIPath; design:
        // the tiles' names].
        "hudbargetelementuipath" => match found {
            Some(i) => {
                let b = &g.ui_extra.bars.list[i];
                let e = num(args, 1) as i64;
                XVal::Str(match e {
                    0..=3 => format!("{}/{}", b.path(), ELEMENTS[e as usize]),
                    ALL => b.path(),
                    _ => String::new(),
                })
            }
            None => XVal::Str(String::new()),
        },
        // `name element [modIndex]`; 10: 1 shown, 0 hidden by HudBarShowBar (-1, hidden by
        // its conditions, is not built) [documented: HudBarGetVisible]. 0 without the bar
        // [hypothesis].
        "hudbargetvisible" => {
            let i = match opt(args, 2).and_then(|m| plugin_at(g, m)) {
                Some(owner) => g.ui_extra.bars.list.iter().position(|b| b.owner == owner && b.name.eq_ignore_ascii_case(&name)),
                None => found,
            };
            XVal::Num(match (i, num(args, 1) as i64) {
                (Some(i), ALL) => f64::from(u8::from(g.ui_extra.bars.list[i].shown)),
                (Some(i), e @ (0 | 1 | 3)) => f64::from(u8::from(g.ui_extra.bars.list[i].elements[e as usize].visible)),
                _ => 0.0,
            })
        }
        "hudbarsetalpha" => set_elements(g, imgs, &|e| e.alpha = a2),
        "hudbarsetdepth" => set_elements(g, imgs, &|e| e.depth = a2),
        "hudbarsetindent" => set_elements(g, imgs, &|e| e.indent = a2),
        "hudbarsetvisible" => set_elements(g, imgs, &|e| e.visible = a2 != 0.0),
        "hudbarsetzoom" => set_elements(g, every, &|e| e.zoom = Some(a2)),
        "hudbarsetcolor" => set_elements(g, every, &|e| e.rgb = Some([a2, a3, a4])),
        "hudbarsetsystemcolor" => set_elements(g, &[0, 1, 2, 3], &|e| e.systemcolor = a2),
        "hudbarsetsize" => set_elements(g, &[0, 1, 3], &|e| (e.width, e.height) = (a2, a3)),
        "hudbarsetsizealt" => set_elements(g, &[0, 1, 3], &|e| e.alt = (a2, a3)),
        // `name element type value`, type 0 rotateangle, 1 rotateaxisx, 2 rotateaxisy
        // [documented: HudBarSetRotation].
        "hudbarsetrotation" => match a2 as i64 {
            r @ 0..=2 => set_elements(g, every, &|e| e.rotate[r as usize] = a3),
            _ => ok(false),
        },
        // `name element trait value`: element 0 is the Text, trait 0 the font, 1 the
        // justification [documented: HudBarSetTextTrait].
        "hudbarsettexttrait" => match (found, num(args, 1) as i64, a2 as i64) {
            (Some(i), 0, t @ (0 | 1)) => {
                let e = &mut g.ui_extra.bars.list[i].elements[2];
                if t == 0 {
                    e.font = Some(a3);
                } else {
                    e.justify = Some(a3);
                }
                apply(g, i);
                ok(true)
            }
            _ => ok(false),
        },
        // `name element path [calculateSize]` [documented: HudBarSetTexture]; reading the
        // size off the texture is not built.
        "hudbarsettexture" => {
            let path = text(args, 2);
            set_elements(g, &[0, 1, 3], &|e| e.texture = path.clone())
        }
        // `name element x y`, 10 the bar's place on the screen [documented: HudBarSetPos].
        "hudbarsetpos" => match found {
            Some(i) if num(args, 1) as i64 == ALL => {
                let b = &mut g.ui_extra.bars.list[i];
                (b.x, b.y) = (a2, a3);
                apply(g, i);
                ok(true)
            }
            _ => set_elements(g, &[0, 1, 2, 3], &|e| (e.x, e.y) = (a2, a3)),
        },
        // `name element text`: 0 the main text, 1 the prefix, 2 the postfix [documented:
        // HudBarSetTextString].
        "hudbarsettextstring" => match (found, num(args, 1) as i64) {
            (Some(i), k @ 0..=2) => {
                g.ui_extra.bars.list[i].texts[k as usize] = text(args, 2);
                apply(g, i);
                ok(true)
            }
            _ => ok(false),
        },
        "hudbarsetmetertrait" => match (found, num(args, 1) as i64) {
            (Some(i), k @ 0..=6) => {
                g.ui_extra.bars.list[i].meter[k as usize] = a2;
                apply(g, i);
                ok(true)
            }
            _ => ok(false),
        },
        // The value against the max [documented: HudBarSetValueFloat, "Value/<MaxValue>";
        // type 3 shows the raw value].
        "hudbarsetvaluefloat" | "hudbarsetvaluemax" => match (found, num(args, 1)) {
            // A value that is no number (a script's bad arithmetic) is refused [design].
            (Some(i), v) if v.is_finite() => {
                let b = &mut g.ui_extra.bars.list[i];
                if lname == "hudbarsetvaluefloat" {
                    b.value = v;
                } else {
                    b.max = v;
                }
                b.share = if b.max != 0.0 { (b.value / b.max).clamp(0.0, 1.0) } else { 0.0 };
                apply(g, i);
                ok(true)
            }
            _ => ok(false),
        },
        // 0.00-1.00, "normalized" when outside; not for type 3 [documented:
        // HudBarSetValuePercentage]. The value follows as that share of the max
        // [hypothesis]. A share that is no number is refused [design].
        "hudbarsetvaluepercentage" => match (found, num(args, 1)) {
            (Some(i), v) if g.ui_extra.bars.list[i].kind != 3 && v.is_finite() => {
                let b = &mut g.ui_extra.bars.list[i];
                b.share = v.clamp(0.0, 1.0);
                b.value = b.share * b.max;
                apply(g, i);
                ok(true)
            }
            _ => ok(false),
        },
        // [documented: HudBarSetItem, HudBarSetFormList: no form removes it; used by
        // iDisableWhenItemNotEquipped, not built].
        "hudbarsetitem" | "hudbarsetformlist" => match found {
            Some(i) => {
                let f = match args.get(1) {
                    Some(XVal::Ref(r)) if *r != 0 => Some(*r),
                    _ => None,
                };
                let b = &mut g.ui_extra.bars.list[i];
                if lname == "hudbarsetitem" {
                    b.item = f;
                } else {
                    b.list = f;
                }
                ok(true)
            }
            None => ok(false),
        },
        "hudbarshowbar" => match found {
            Some(i) => {
                g.ui_extra.bars.list[i].shown = num(args, 1) != 0.0;
                apply(g, i);
                XVal::Num(1.0)
            }
            None => zero,
        },
        // The Text beside the pictures: 0 to their right, 1 below, 2 above [documented:
        // HudBarSetAutoPos]. [hypothesis] The pictures are the visible Meter, Frame and
        // ImageEx; to the right at the top of the highest, below and above at the left of
        // the leftmost; above by the Text's height.
        "hudbarsetautopos" => match found {
            Some(i) => {
                let b = &mut g.ui_extra.bars.list[i];
                let pics: Vec<Element> = [0, 1, 3].iter().map(|&k| b.elements[k].clone()).filter(|e| e.visible).collect();
                if !pics.is_empty() {
                    let left = pics.iter().map(|e| e.x).fold(f64::INFINITY, f64::min);
                    let top = pics.iter().map(|e| e.y).fold(f64::INFINITY, f64::min);
                    let right = pics.iter().map(Element::right).fold(f64::NEG_INFINITY, f64::max);
                    let bottom = pics.iter().map(Element::bottom).fold(f64::NEG_INFINITY, f64::max);
                    let t = &mut b.elements[2];
                    (t.x, t.y) = match num(args, 1) as i64 {
                        1 => (left, bottom),
                        2 => (left, top - t.height),
                        _ => (right, top),
                    };
                }
                apply(g, i);
                zero
            }
            None => zero,
        },
        _ => return None,
    }))
}
