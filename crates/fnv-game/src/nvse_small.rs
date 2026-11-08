//! The small extender plugins (#1020), CacheUI's last eight (#1013), Anh's last two on
//! the game (#1015) and NX_GetQVEVFL (#1017): BookMenu, ButcherPete, ClimateControl,
//! Command Extender, HotReload, Overcharge, PostalOut, TTW, Stewie's Tweaks, hNVSE and
//! yUI [documented: each page in the wiki export; docs/spec-notes/ext-small.md]. None of
//! these plugins is installed in the probe game, so nothing here is observed. What the
//! game does not model (a plugin's own configuration, the GECK, hot reload, a file
//! written beside the game) returns the page's "nothing" value [decision, tagged at each
//! arm]. Changes last for the session.

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use std::collections::HashMap;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

/// A typewriter animation on a tile (CacheUI's SetUITypewriter).
#[derive(Debug, Clone)]
pub struct Typewriter {
    /// The tile's path, the parent of the `_TSn` and `_TAn` traits.
    pub tile: String,
    /// The order the letters come in: `rank[i]` is letter i's place, 0 first.
    pub rank: Vec<usize>,
    pub start: f64,
    pub duration: f64,
    pub fade: f64,
    /// Modes 1, 3 and 6: in, then out again.
    pub out: bool,
    /// Modes 5 and 6 inverted: the letters fade out (then in).
    pub reverse_alpha: bool,
}

/// CacheUI's SetFrameControllerLoop callback.
#[derive(Debug, Clone)]
pub struct FrameLoop {
    pub func: XVal,
    pub interval: f64,
    pub frames: u64,
    /// Seconds left before the next call is put on the frame delay.
    pub left: f64,
    /// Frames at which a call is due.
    pub due: Vec<u64>,
}

/// The session state of the plugins of this file.
#[derive(Debug, Default)]
pub struct SmallState {
    /// Seconds of play, menus included: each frame's own length added up (the engine
    /// changes `frame_seconds` every frame, so the frame count times it is no clock).
    pub real_seconds: f64,
    /// TTW_CRC32's answers by plugin name (lower case): a plugin does not change during
    /// a session, and FalloutNV.esm is hundreds of megabytes to read again.
    pub crc_cache: HashMap<String, f64>,
    /// CacheUI's SetTileScript: a trait path (lower case) and its UDF, in call order.
    pub tile_scripts: Vec<(String, XVal)>,
    pub frame_loops: Vec<FrameLoop>,
    pub typewriters: Vec<Typewriter>,
    /// SetFOVRatio's `current fov` and `strength`, for the engine's aiming field of view
    /// (follow-up: the engine does not read it yet).
    pub fov_ratio: Option<(f64, f64)>,
    /// Command Extender's switches, for the engine's menus and muzzle flashes
    /// (follow-up: the engine does not read them yet).
    pub container_transfer_off: bool,
    pub quantity_suffix_off: bool,
    pub muzzle_lights_off: bool,
    /// Overcharge: SetOCSettings's values by trait 1-12, SetOCWeaponState's by (actor,
    /// weapon, trait 1-19).
    pub oc_settings: HashMap<u8, f64>,
    pub oc_state: HashMap<(FormId, FormId, u8), f64>,
    /// yUI's ySI traits: a form's sorting tag, and an icon by tag (or by form when it
    /// has no tag).
    pub ysi_tag: HashMap<FormId, String>,
    pub ysi_icon_by_tag: HashMap<String, String>,
    pub ysi_icon: HashMap<FormId, String>,
}

/// CRC-32 (the IEEE polynomial, reflected, as zip and PNG use) [decision: the page's
/// "CRC32 checksum" names no variant; this is the common one].
pub fn crc32(bytes: &[u8]) -> u32 {
    // The byte table: one lookup a byte instead of eight shifts.
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (n, e) in t.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            }
            *e = c;
        }
        t
    });
    !bytes.iter().fold(0xFFFF_FFFFu32, |c, &b| table[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8))
}

impl Game<'_> {
    /// The per-frame part of these plugins: CacheUI's frame loops, tile scripts and
    /// typewriter animations. Called from [`Game::tick`].
    pub(crate) fn small_tick(&mut self) {
        let dt = self.cfg.frame_seconds;
        self.small.real_seconds += dt;
        let frame = self.frame;
        // SetFrameControllerLoop: the time counts down; at zero it starts again at once
        // and the call waits `frames` more frames [documented: "Time will be reseted when
        // reaching zero. It does not wait for the frame delay"].
        let mut calls = Vec::new();
        for l in &mut self.small.frame_loops {
            l.left -= dt;
            if l.left <= 1e-9 {
                l.left += l.interval.max(dt);
                l.due.push(frame + l.frames);
            }
            let before = l.due.len();
            l.due.retain(|&f| f > frame);
            for _ in l.due.len()..before {
                calls.push(l.func.clone());
            }
        }
        for f in calls {
            self.small_run(&f);
        }
        // SetTileScript: each frame while the trait is not zero [documented].
        let scripts = self.small.tile_scripts.clone();
        for (path, f) in scripts {
            if self.ui_tree.get_float(&path).is_some_and(|v| v != 0.0) {
                self.small_run(&f);
            }
        }
        self.typewriter_tick();
    }

    fn small_run(&mut self, f: &XVal) {
        if let Err(error) = self.run_function(f, None, Vec::new(), None, RunCtx::default()) {
            self.emit(crate::trace::EventKind::ScriptError { unit: "CacheUI callback".into(), error });
        }
    }

    /// Each typewriter's letters' alpha (`_TAn`) for the time now: a letter starts at its
    /// place's share of the duration and takes `fade` to reach 255 [documented:
    /// SetUITypewriter, "Fade define how long a single letter needs to fade in … the
    /// entire duration … is always duration + fade"]. The modes that go out again come in
    /// over the first half and go out over the second [hypothesis: the wait is not
    /// documented].
    fn typewriter_tick(&mut self) {
        let now = self.time_real();
        let mut writes = Vec::new();
        self.small.typewriters.retain(|t| {
            let n = t.rank.len().max(1) as f64;
            let total = t.duration + t.fade;
            let el = now - t.start;
            for (i, &r) in t.rank.iter().enumerate() {
                let span = if t.out { t.duration / 2.0 } else { t.duration };
                let begin = span * r as f64 / n;
                let fade = |e: f64| if t.fade <= 0.0 { f64::from(u8::from(e >= begin)) } else { ((e - begin) / t.fade).clamp(0.0, 1.0) };
                let mut a = fade(el);
                if t.out && el > t.duration / 2.0 {
                    a -= fade(el - t.duration / 2.0);
                }
                if t.reverse_alpha {
                    a = 1.0 - a;
                }
                writes.push((format!("{}\\_TA{i}", t.tile), (a * 255.0).round()));
            }
            el < total
        });
        for (p, v) in writes {
            self.ui_tree.set_float(&p, v);
        }
    }

    /// A worldspace's map cells (WRLD MNAM: north-west X and Y, south-east X and Y) as
    /// the record has them, after SetWorldspaceMapBounds; for the engine's map.
    pub fn world_map_bounds(&self, w: FormId) -> Option<[i32; 4]> {
        let mut out = [0; 4];
        for (k, field) in ["nw_cell_x", "nw_cell_y", "se_cell_x", "se_cell_y"].iter().enumerate() {
            match crate::nvse::record_value(self, w, "MNAM", field) {
                Some(XVal::Num(x)) => out[k] = x as i32,
                _ => return None,
            }
        }
        Some(out)
    }

    /// Seconds of play, menus included: the frames' lengths added up
    /// ([`SmallState::real_seconds`]).
    pub(crate) fn time_real(&self) -> f64 {
        self.small.real_seconds
    }
}

/// The letters' order for a typewriter mode [documented: SetUITypewriter's two tables]:
/// 0/1 from the left (inverted: the right), 2/3 from the middle out (inverted: from the
/// outer ends in), 5/6 in a Fisher-Yates shuffle [decision: a fixed-seed shuffle, so a
/// run repeats].
fn typewriter_rank(n: usize, mode: i64, invert: bool) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    match mode {
        2 | 3 => {
            let mid = (n as f64 - 1.0) / 2.0;
            order.sort_by(|&a, &b| {
                let (da, db) = ((a as f64 - mid).abs(), (b as f64 - mid).abs());
                let c = da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal);
                (if invert { c.reverse() } else { c }).then(a.cmp(&b))
            });
        }
        5 | 6 => {
            let mut s: u64 = 0x2545_F491_4F6C_DD1D;
            for i in (1..n).rev() {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                order.swap(i, (s % (i as u64 + 1)) as usize);
            }
        }
        _ if invert => order.reverse(),
        _ => {}
    }
    let mut rank = vec![0; n];
    for (place, &letter) in order.iter().enumerate() {
        rank[letter] = place;
    }
    rank
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let list = |g: &mut Game, items: Vec<XVal>| {
        let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
        XVal::Array(g.ext.new_array(ArrayKind::List, entries))
    };
    let v = match name {
        // ---- Anh ----
        // `1stPersonFOV`: the field of view the last SetCameraFOV gave, else the INI's
        // default (fDefaultWorldFOV, fDefault1stPersonFOV) [documented: GetCameraFOV,
        // "Returns the FOV value for the camera"; hypothesis: 75 and 55 when the INI
        // names none, the shipped Fallout_default.ini's values].
        "getcamerafov" => {
            let first = num(args, 0, 0.0) != 0.0;
            let set = if first { g.ui.camera.fov_set.1 } else { g.ui.camera.fov_set.0 };
            match set {
                Some(v) => XVal::Num(f64::from(v)),
                None => {
                    let key = if first { "fDefault1stPersonFOV:Display" } else { "fDefaultWorldFOV:Display" };
                    let ini = crate::nvse_settings::call(g, "getnumericinisetting", &[XVal::Str(key.into())]);
                    match ini {
                        Some(Ok(XVal::Num(x))) if x > 0.0 => XVal::Num(x),
                        _ => XVal::Num(if first { 55.0 } else { 75.0 }),
                    }
                }
            }
        }
        // [decision: NVForever builds no tube mesh; the page says only "Updates the
        // specified tube mesh"]: nothing, 0.
        "updatetubemesh" | "quitmeshinwithmeandupdatealready" => zero,
        // ---- CacheUI ----
        // [documented: GetFPS, "Not 100% Accurate"]: one over the last frame's seconds.
        "getfps" => XVal::Num(if g.cfg.frame_seconds > 0.0 { 1.0 / g.cfg.frame_seconds } else { 0.0 }),
        // `path script`: one script per trait [documented: SetTileScript, "Only one
        // script can be register per trait"]; RemoveTileScript drops it.
        "settilescript" => {
            let path = text(args, 0);
            let f = args.get(1).cloned().unwrap_or(XVal::Num(0.0));
            g.small.tile_scripts.retain(|(p, _)| !p.eq_ignore_ascii_case(&path));
            let callable = !matches!(f, XVal::Num(_) | XVal::Ref(0));
            tracing::info!("tile script: path={path:?} callable={callable}");
            if callable {
                g.small.tile_scripts.push((path, f));
            }
            zero
        }
        "removetilescript" => {
            let path = text(args, 0);
            g.small.tile_scripts.retain(|(p, _)| !p.eq_ignore_ascii_case(&path));
            zero
        }
        // `current strength`: kept for the engine's aiming field of view, "WeaponFOV +
        // ((Current - 75) * (Ratio))" [documented: SetFOVRatio; follow-up: the engine's
        // aiming camera does not read it yet].
        "setfovratio" => {
            g.small.fov_ratio = Some((num(args, 0, 75.0), num(args, 1, 0.0).clamp(0.0, 1.0)));
            zero
        }
        // `udf set [time] [frames]` (SFCL): registered or removed; called every `time`
        // seconds, `frames` frames after [documented: SetFrameControllerLoop; hypothesis:
        // the seconds are the frame's real seconds, menus included, and one udf has one
        // loop].
        "setframecontrollerloop" | "sfcl" => {
            let f = args.first().cloned().unwrap_or(XVal::Num(0.0));
            g.small.frame_loops.retain(|l| l.func != f);
            if num(args, 1, 0.0) != 0.0 {
                let interval = num(args, 2, 0.0).max(0.0);
                g.small.frame_loops.push(FrameLoop { func: f, interval, frames: num(args, 3, 0.0).max(0.0) as u64, left: interval, due: Vec::new() });
            }
            zero
        }
        // `path text time [fade] [mode] [invert]`: the letters go into the tile's `_TSn`
        // traits at once and their alphas (`_TAn`) are animated; no duration stops it and
        // sets every alpha to 0; at most 260 letters [documented: SetUITypewriter].
        "setuitypewriter" => {
            let (tile, words, duration) = (text(args, 0), text(args, 1), num(args, 2, 0.0));
            let (fade, mode, invert) = (num(args, 3, 0.0).max(0.0), num(args, 4, 0.0) as i64, num(args, 5, 0.0) != 0.0);
            g.small.typewriters.retain(|t| !t.tile.eq_ignore_ascii_case(&tile));
            let letters: Vec<char> = words.chars().take(260).collect();
            for (i, c) in letters.iter().enumerate() {
                crate::ui_functions::call(g, "setuistring", &[XVal::Str(format!("{tile}\\_TS{i}")), XVal::Str(c.to_string())]);
            }
            if duration <= 0.0 {
                for i in 0..letters.len() {
                    crate::ui_functions::call(g, "setuifloat", &[XVal::Str(format!("{tile}\\_TA{i}")), XVal::Num(0.0)]);
                }
            } else {
                let t = Typewriter {
                    tile,
                    rank: typewriter_rank(letters.len(), mode, invert),
                    start: g.time_real(),
                    duration,
                    fade,
                    out: matches!(mode, 1 | 3 | 6),
                    reverse_alpha: invert && matches!(mode, 5 | 6),
                };
                g.small.typewriters.push(t);
                g.typewriter_tick();
            }
            zero
        }
        // `text symbol`: the parts named by editor ID or "Plugin.esm:hex" (GetFormFromMod's
        // form), in order; a part that names nothing is left out [documented:
        // SplitStringToRef; hypothesis: left out, not a 0].
        "splitstringtoref" => {
            let (s, sym) = (text(args, 0), text(args, 1));
            let parts: Vec<&str> = if sym.is_empty() { vec![s.as_str()] } else { s.split(sym.as_str()).collect() };
            let mut out = Vec::new();
            for p in parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
                let f = match p.split_once(':') {
                    Some((plugin, hex)) => g.data.load_order.iter().position(|m| m.eq_ignore_ascii_case(plugin.trim())).and_then(|i| {
                        let low = u32::from_str_radix(hex.trim(), 16).ok()? & 0x00FF_FFFF;
                        let f = FormId(((i as u32) << 24) | low);
                        g.data.sigs.contains_key(&f).then_some(f)
                    }),
                    None if p.eq_ignore_ascii_case("player") || p.eq_ignore_ascii_case("playerref") => Some(FormId::PLAYER_REF),
                    None => g.data.by_edid(p),
                };
                out.extend(f.map(|f| XVal::Ref(f.0)));
            }
            list(g, out)
        }
        // [decision: NVForever never writes into the game folder, and the page's change
        // only shows after a restart]: nothing is written, 0.
        "writetoxml" => zero,
        // ---- NX ----
        // [decision: the page leaves its use to an outside tutorial and gives no
        // arguments (ext-nx.md §2)]: the "nothing" value, 0.
        "nx_getqvevfl" => zero,
        // ---- not a function ----
        // [documented: Sandbox, "This is NOT a function. It is a sandbox area to test page
        // changes"]: nothing.
        "sandbox" => zero,
        // ---- BookMenu ----
        // `[showTake] [book]` on a book reference: as `book.Activate player` [documented:
        // ShowBookMenu, "behaves similarly to 'bookref.activate player'"; decision: the
        // Take button is the engine's book screen's, which shows no such choice, and
        // without a reference nothing opens].
        "showbookmenu" => {
            if let Some(b) = subject.filter(|&r| crate::functions::base_of(g, r).is_some()) {
                if let Err(e) = crate::functions::console::call_words(g, Some(b), "Activate", &["PlayerREF".to_string()], "ShowBookMenu") {
                    g.console_out.push(format!("Script error: ShowBookMenu: {e}"));
                }
            }
            zero
        }
        // ---- ButcherPete (a Fallout 3 plugin) ----
        // [documented: GetButcherPeteVersion, "if the mod isn't installed it will simply
        // return 0"]: it is not, in New Vegas.
        "getbutcherpeteversion" => zero,
        // On a terminal: 0 open (no lock data, unlocked by a script, or hacked), 2 locked
        // out (a failed hack, or a lock level of Requires Key), else 1; anything else as
        // GetLocked [documented: GetLockedAlt's table; hypothesis: "player has note" is
        // not checked, the terminal's password note is not kept].
        "getlockedalt" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            if crate::functions::refs::is_terminal(g, r) {
                let v = if g.terminals.access.get(&r) == Some(&crate::terminal::Access::Hacked) {
                    0
                } else if g.terminals.access.get(&r) == Some(&crate::terminal::Access::LockedOut) {
                    2
                } else {
                    match crate::terminal::locked(g, r) {
                        0 => 0,
                        _ if crate::terminal::lock_level(g, r) >= 5 => 2,
                        l => l,
                    }
                };
                XVal::Num(f64::from(v))
            } else {
                let status = g.world.lock_status(r).unwrap_or(0);
                XVal::Num(if status == 1 && g.locks.broken.contains(&r) { 2.0 } else { f64::from(status) })
            }
        }
        // [documented: GetQuestFailedAlt, "Alternate version of GetQuestFailed"].
        "getquestfailedalt" => return crate::nvse::call(g, ctx, "getquestfailed", subject, args),
        // `code`: IsKeyPressed counting a key DisableKey turned off [documented:
        // IsKeyPressedAlt].
        "iskeypressedalt" => return crate::nvse::call(g, ctx, "iskeypressed", subject, &[args.first().cloned().unwrap_or(XVal::Num(-1.0)), XVal::Num(2.0)]),
        // ---- ClimateControl ----
        // The weather the sky showed before the one it shows now [documented:
        // GetOutgoingWeather, "Returns the outgoing weather"; hypothesis: kept after the
        // change has finished].
        "getoutgoingweather" => XVal::Ref(g.weather.outgoing.map_or(0, |w| w.0)),
        // ---- Command Extender (a Fallout 3 plugin) ----
        // [documented: FakeQuantityMenu, "for Fallout 3 only", ShowQuantityMenu being
        // New Vegas's]: nothing.
        "fakequantitymenu" => zero,
        // [decision: the plugin is not a New Vegas plugin; nothing is printed, 0].
        "getcommandextenderversion" => zero,
        // [decision: the Rock-It Launcher is Fallout 3's; New Vegas has none]: 0.
        "getrockitammocount" => zero,
        // GetSelectedItemRef's base form [documented: GetSelectedItemBaseRef; hypothesis:
        // the DATA and STATS pages' forms are not reported, as GetSelectedItemRef's].
        "getselecteditembaseref" => {
            let r = match crate::nvse::call(g, ctx, "getselecteditemref", subject, &[]) {
                Some(Ok(XVal::Ref(r))) if r != 0 => FormId(r),
                _ => return Some(Ok(XVal::Ref(0))),
            };
            XVal::Ref(crate::functions::base_of(g, r).unwrap_or(r).0)
        }
        // GetActiveUIComponentFullName to the console [documented].
        "printactiveuicomponentfullname" => {
            if let Some(Ok(XVal::Str(s))) = crate::ui_functions::call(g, "getactiveuicomponentfullname", &[]) {
                g.console_out.push(s);
            }
            zero
        }
        // `state` (1 off) [documented: their pages; hypothesis: 1 turns the item transfer
        // and the "(x)" suffix off, 0 back on]; without a state ToggleMuzzleFlashLights
        // switches [documented]. Kept for the engine (follow-up: its menus and muzzle
        // lights do not read them yet).
        "togglecontainermenuitemtransfer" => {
            g.small.container_transfer_off = num(args, 0, 0.0) != 0.0;
            zero
        }
        "toggleitemquantitysuffix" => {
            g.small.quantity_suffix_off = num(args, 0, 0.0) != 0.0;
            zero
        }
        "togglemuzzleflashlights" => {
            g.small.muzzle_lights_off = match args.first() {
                Some(XVal::Num(x)) => *x != 0.0,
                _ => !g.small.muzzle_lights_off,
            };
            zero
        }
        // ---- HotReload ----
        // [decision: NVForever does not hot-reload scripts]: never true; ToGECK opens
        // nothing (no GECK beside NVForever).
        "getgamehotreloaded" | "togeck" => zero,
        // ---- Overcharge ----
        // [decision: Overcharge's configuration files are not read (the plugin is not
        // installed)]: a setting reads what SetOCSettings put, else 0; a weapon's
        // configuration reads 0; a weapon's state what SetOCWeaponState put, else 0.
        "getocsettings" => XVal::Num(u8::try_from(num(args, 0, 0.0) as i64).ok().and_then(|t| g.small.oc_settings.get(&t).copied()).unwrap_or(0.0)),
        "setocsettings" => {
            if let Some(t) = u8::try_from(num(args, 0, 0.0) as i64).ok().filter(|t| (1..=12).contains(t)) {
                g.small.oc_settings.insert(t, num(args, 1, 0.0));
            }
            zero
        }
        "getocweaponconfig" => zero,
        "getocweaponstate" | "setocweaponstate" => {
            let actor = subject.unwrap_or(FormId::PLAYER_REF);
            let (Some(w), Some(t)) = (form(args, 0), u8::try_from(num(args, 1, 0.0) as i64).ok().filter(|t| (1..=19).contains(t))) else { return Some(Ok(zero)) };
            if name == "setocweaponstate" {
                // [hypothesis: it returns the value set; the page's return type is a value].
                let v = num(args, 2, 0.0);
                g.small.oc_state.insert((actor, w, t), v);
                XVal::Num(v)
            } else {
                XVal::Num(g.small.oc_state.get(&(actor, w, t)).copied().unwrap_or(0.0))
            }
        }
        // `light actor`: a temporary copy of the light [documented: CreateOCLightClone,
        // "Returns with the cloned light form"; decision: its colour stays, NVForever
        // keeps no Overcharge heat to take a colour from].
        "createoclightclone" => {
            let Some(l) = form(args, 0).filter(|&l| g.data.sigs.get(&l).is_some_and(|s| s == b"LIGH")) else { return Some(Ok(XVal::Ref(0))) };
            return crate::nvse::call(g, ctx, "tempcloneform", None, &[XVal::Ref(l.0)]);
        }
        // ---- PostalOut ----
        // `idle`: the player plays it [documented: IsPlayerIdlePlaying; NVForever keeps
        // one idle per actor, for both views].
        "isplayeridleplaying" => yes(form(args, 0).is_some_and(|i| g.idle_playing(FormId::PLAYER_REF) == Some(i))),
        // ---- TTW ----
        // `plugin`: the CRC-32 of the plugin file [documented: TTW_CRC32, "Returns CRC32
        // checksum of a plugin"]; 0 when it cannot be read [hypothesis].
        "ttw_crc32" => {
            let p = text(args, 0);
            let k = p.replace('/', "\\").to_ascii_lowercase();
            if let Some(&v) = g.small.crc_cache.get(&k) {
                return Some(Ok(XVal::Num(v)));
            }
            let v = crate::mod_files::read_data(&p).map_or(0.0, |b| f64::from(crc32(&b)));
            g.small.crc_cache.insert(k, v);
            XVal::Num(v)
        }
        // `skill`: 1 when the calling actor's (else the player's) equipped weapon uses
        // that skill (its actor value code, as GetWeaponSkill gives) [documented:
        // TTW_GetEquippedWeaponSkill, "true if weapon skill matches that of equipped
        // weapon"].
        "ttw_getequippedweaponskill" => {
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            let w = crate::functions::items::equipped_weapon(g, a);
            let skill = w.and_then(|w| crate::nvse::record_value(g, w, "DNAM", "skill"));
            yes(matches!(skill, Some(XVal::Num(s)) if s == num(args, 0, -1.0)))
        }
        // Each loaded plugin as [name, version, description] [documented: TTW_GetPlugins;
        // decision: version 0 and an empty description, NVForever keeps no plugin
        // header's version or description].
        "ttw_getplugins" => {
            let names = g.data.load_order.clone();
            let mut rows = Vec::new();
            for n in names {
                rows.push(list(g, vec![XVal::Str(n), XVal::Num(0.0), XVal::Str(String::new())]));
            }
            list(g, rows)
        }
        // TTW's gene projector is the race menu: the scripts that call it wait in a
        // `MenuMode 1036` block (MQ05AirlockSignSCRIPT's "change your face" button)
        // [observed: TaleOfTwoWastelands.esm source text; its CG00 stage 60 calls it where
        // Fallout 3's calls ShowRaceMenu]. It opens NVForever's character creator as
        // ShowRaceMenu does [design: owner 2026-11-02, "can we reuse our character
        // creator"; before, nothing opened and the birth scene went on without it].
        "ttw_showgeneprojector" => {
            g.race_menu = crate::game::RaceMenu::Creator;
            g.open_menu(crate::game::menus::RACE);
            zero
        }
        // ---- Stewie's Tweaks ----
        // `"key:section"` (StewVal): the value in the plugin's INI, a number when it reads
        // as one [documented: StewieGetValue; hypothesis: the INI is
        // Data\NVSE\Plugins\nvse_stewie_tweaks.ini]; 0 when there is none (the plugin is
        // not installed).
        "stewiegetvalue" | "stewval" => {
            let ks = text(args, 0);
            let Some((key, section)) = ks.split_once(':') else { return Some(Ok(zero)) };
            let ini = g.mod_files.read_text("NVSE\\Plugins\\nvse_stewie_tweaks.ini");
            match ini.and_then(|t| crate::mod_files::ini_get(&t, &section.trim().to_ascii_lowercase(), key.trim())) {
                Some(v) => v.parse::<f64>().map(XVal::Num).unwrap_or(XVal::Str(v.trim_matches('"').to_string())),
                None => zero,
            }
        }
        // ---- hNVSE ----
        // `cell`: an exterior cell's regions (CELL XCLR), in record order; an interior's
        // none [documented: GetCellRegions].
        "getcellregions" => {
            let mut out = Vec::new();
            if let Some(fields) = form(args, 0).and_then(|c| g.data.record_fields.get(&c)) {
                let interior = fields.iter().find(|(s, _)| s == b"DATA").and_then(|(_, v)| v.as_u64().or_else(|| v.get("value").and_then(|f| f.as_u64()))).is_some_and(|f| f & 1 != 0);
                if !interior {
                    for (s, v) in fields {
                        if s == b"XCLR" {
                            let ids = v.as_array().cloned().unwrap_or_else(|| vec![v.clone()]);
                            out.extend(ids.iter().filter_map(json_form).map(|f| XVal::Ref(f.0)));
                        }
                    }
                }
            }
            list(g, out)
        }
        // `world nwX nwY seX seY`: the worldspace's map cells (WRLD MNAM), read by every
        // reader of the record [documented: SetWorldspaceMapBounds; follow-up: the
        // engine's Pip-Boy map reads its bounds once].
        "setworldspacemapbounds" => {
            if let Some(w) = form(args, 0).filter(|&w| g.data.sigs.get(&w).is_some_and(|s| s == b"WRLD")) {
                for (k, field) in ["nw_cell_x", "nw_cell_y", "se_cell_x", "se_cell_y"].iter().enumerate() {
                    g.field_overrides.insert((w, format!("MNAM/{field}")), XVal::Num(num(args, k + 1, 0.0).trunc()));
                }
            }
            zero
        }
        // ---- yUI ----
        // `"tag"|"icon" [form]` and `trait value [form]`: ySI's sorting tag and icon of the
        // form, else of the calling reference's base [documented: ySIGetTrait,
        // ySISetTrait]. An icon set on a form with a tag goes to every form of that tag
        // [documented: "Changing the icon texture of a Form will also change also icon
        // texture of all Forms with the same sorting tag"]. Not read from ySI's own
        // files [decision: ySI is not installed]: an unset trait reads "".
        "ysigettrait" | "ysisettrait" => {
            let set = name == "ysisettrait";
            let trait_name = text(args, 0).to_ascii_lowercase();
            let at = if set { 2 } else { 1 };
            let f = form(args, at).or(subject).map(|f| crate::functions::base_of(g, f).unwrap_or(f));
            let Some(f) = f else { return Some(Ok(if set { zero } else { XVal::Str(String::new()) })) };
            let tag = g.small.ysi_tag.get(&f).cloned();
            if set {
                let v = text(args, 1);
                match trait_name.as_str() {
                    "tag" => {
                        g.small.ysi_tag.insert(f, v);
                    }
                    "icon" => match tag {
                        Some(t) => {
                            g.small.ysi_icon_by_tag.insert(t, v);
                        }
                        None => {
                            g.small.ysi_icon.insert(f, v);
                        }
                    },
                    _ => {}
                }
                zero
            } else {
                XVal::Str(match trait_name.as_str() {
                    "tag" => tag.unwrap_or_default(),
                    "icon" => tag.and_then(|t| g.small.ysi_icon_by_tag.get(&t).cloned()).or_else(|| g.small.ysi_icon.get(&f).cloned()).unwrap_or_default(),
                    _ => String::new(),
                })
            }
        }
        _ => return None,
    };
    Some(Ok(v))
}

/// A form in a decoded field: a number or a "0x…" string.
fn json_form(v: &serde_json::Value) -> Option<FormId> {
    match v {
        serde_json::Value::Number(n) => n.as_u64().and_then(|x| u32::try_from(x).ok()).filter(|&x| x != 0).map(FormId),
        serde_json::Value::String(s) => s.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok()).filter(|&x| x != 0).map(FormId),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_is_the_ieee_one() {
        // The check value of CRC-32/ISO-HDLC.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn typewriter_orders() {
        assert_eq!(typewriter_rank(4, 0, false), vec![0, 1, 2, 3]);
        assert_eq!(typewriter_rank(4, 0, true), vec![3, 2, 1, 0]);
        // From the middle: the two middle letters first, then the ends.
        assert_eq!(typewriter_rank(4, 2, false), vec![2, 0, 1, 3]);
        assert_eq!(typewriter_rank(4, 2, true), vec![0, 2, 3, 1]);
        let mut r = typewriter_rank(10, 5, false);
        r.sort_unstable();
        assert_eq!(r, (0..10).collect::<Vec<_>>());
    }
}
