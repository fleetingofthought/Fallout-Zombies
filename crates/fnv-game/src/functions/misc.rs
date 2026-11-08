//! Messages, time, globals and numbers.

use super::{none, yes, Args, Ctx};
use crate::game::{menus, Game, LineLook, MessageBox};
use crate::trace::EventKind;
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::clock;
use rand_core::RngCore;

/// `ShowMessage msg [vars…] [duration]`: traced with the message text, its format
/// specifiers filled from the variables ([`format_message`]); a message with buttons
/// waits for one (runner input `press_button`, read by GetButtonPressed of the script
/// that showed it). Duration does nothing [documented: GECK wiki ShowMessage notes].
pub fn show_message(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let m = a.form(0)?;
    let def = g.data.messages.get(&m).ok_or_else(|| format!("{} is not a message", g.data.name(m)))?;
    // SetMessageFlags' Message Box bit (`nvse_jip6`).
    let (text, buttons, is_box) = (def.text.clone(), def.buttons.clone(), g.message_flags(m) & 1 != 0);
    let (icon, seconds) = (def.icon, def.display_seconds);
    // "A disabled message will never be displayed by the game" [documented:
    // SetMessageDisabled].
    if g.message_disabled(m) {
        return none();
    }
    // Too few variables for the text: the message does not show [documented: GECK wiki
    // ShowMessage notes]. Button texts take no variables [documented: same notes].
    let vars = message_vars(a);
    let text = match text {
        Some(t) => Some(format_message(&t, &vars).ok_or_else(|| format!("{} needs more variables than given", g.data.name(m)))?),
        None => None,
    };
    let hidden = g.hidden_buttons(m, ctx.self_ref);
    if !buttons.is_empty() {
        let owner = Some(MessageBox::owner_of(ctx.inst, &ctx.unit));
        g.message_box = Some(MessageBox { message: m, buttons: buttons.len(), pressed: None, owner, hidden: hidden.clone() });
    }
    // Player character string codes (`&PCName;` …) [documented: ext-format.md §6].
    let text = text.map(|t| g.expand_codes(&t));
    let shown = text.clone().unwrap_or_default();
    g.emit(EventKind::MessageShown { message: m, text, buttons: buttons.clone(), hidden });
    // A message box (MESG flag Message Box, or buttons) is a menu until the player closes
    // it [observed: MenuMode 1001 while the pack plugins' boxes were open,
    // layer5-captures.md §10.1]; other messages are corner notifications.
    if is_box || !buttons.is_empty() {
        g.show_box(m);
    } else {
        let icon = match icon.and_then(|i| crate::nvse::record_value(g, i, "ICON", "")) {
            Some(fnv_script::vm::XVal::Str(p)) => p,
            _ => String::new(),
        };
        g.queue_corner_message(&shown, &icon, seconds.map_or(crate::nvse_hud::CORNER_SECONDS, f64::from));
    }
    none()
}

/// The numbers for a message's format specifiers: the arguments after the message. A
/// compiled script passes them as one list (its `format_vars` trailer, `ArgValue::Vars`)
/// followed by the duration (`Int32`), which is not a variable; source run as text passes
/// plain values. Before, the list read as one 0, so every compiled ShowMessage printed 0
/// for its numbers (the concealed mine's "Explosives 0 or higher required", owner's
/// playtest 2026-10-31; the requirement it tests was 28).
fn message_vars(a: &Args) -> Vec<f64> {
    let mut out = Vec::new();
    for v in a.0.iter().skip(1) {
        match v {
            ArgValue::Vars(xs) => out.extend(xs.iter().map(|x| x.num())),
            ArgValue::Int32(_) => {}
            other => out.push(other.value().map_or(0.0, |x| x.num())),
        }
    }
    out
}

/// The chosen button once, then −1 [documented: GECK wiki GetButtonPressed]. Only the
/// script that showed the box receives it; for any other script the box is not there
/// (−1) and stays open [hypothesis, from the GECK wiki's ShowMessage and
/// GetButtonPressed pages; the `--play` integration saw an unrelated script (Doc
/// Mitchell's chemistry set) take every box's button while the button was global].
pub fn get_button_pressed(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let me = MessageBox::owner_of(ctx.inst, &ctx.unit);
    if let Some(m) = g.message_box.as_ref() {
        if let (Some(b), true) = (m.pressed, m.owner.as_ref().is_none_or(|o| *o == me)) {
            g.message_box = None;
            return Ok(Value::Num(b as f64));
        }
    }
    Ok(Value::Num(-1.0))
}

/// Writes to the warnings log only.
pub fn show_warning(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    if let Ok(s) = a.str(0) {
        tracing::info!(warning = %s, "script ShowWarning");
    }
    let _ = g;
    none()
}

/// 0 to 99 from the game's seeded generator (deterministic per seed).
pub fn get_random_percent(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num((g.rng.next_u32() % 100) as f64))
}

/// The resolution of the game clock GetSecondsPassed reads: 1/64 s [observed: every
/// drop in the delay captures is a multiple of 0.015625 s, layer5-captures.md §5.4;
/// hypothesis: the clock, not the frame, is quantised].
pub const CLOCK_TICK: f64 = 1.0 / 64.0;

/// The game clock in 1/64 s steps.
pub fn clock_now(t: f64) -> f64 {
    (t / CLOCK_TICK + 1e-9).floor() * CLOCK_TICK
}

/// The clock time between the start of this instance's previous per-frame run
/// (GameMode, ScriptEffectUpdate) and the start of the current one; 0 on the first run.
/// Every call in one run gives the same value [documented: GECK wiki GetSecondsPassed],
/// and the value does not depend on when the script last called the function
/// [observed: VCG01's countdowns restarted after a pause in its calls ran their full
/// time, layer5-captures.md §10.7; see [`Game::mark_run`]]. Outside a per-frame run (an
/// event block of the same instance), the time since the latest run; result scripts (no
/// instance) get 0.
pub fn get_seconds_passed(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let Some(key) = ctx.inst else { return none() };
    let now = clock_now(g.time);
    let Some(inst) = g.instances.get(&key) else { return none() };
    let v = match (inst.run_last, inst.run_prev) {
        (Some(last), prev) if last == now => prev.map_or(0.0, |p| now - p),
        (Some(last), _) => now - last,
        (None, _) => 0.0,
    };
    Ok(Value::Num(v))
}

pub fn get_global_value(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let gl = a.form(0)?;
    Ok(Value::Num(g.world.global(gl).map_err(|e| e.to_string())? as f64))
}

/// The time of day in hours: the GameHour global.
pub fn get_current_time(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let (id, _) = g.world.forms.global_by_edid(clock::GAME_HOUR).ok_or("no GameHour global")?;
    Ok(Value::Num(g.world.global(id).map_err(|e| e.to_string())? as f64))
}

/// `GetGameSetting name`: the setting's value in the load order, else the game's own
/// default for the few settings the masters do not define that the shipped scripts read
/// ([`EXE_SETTINGS`]).
pub fn get_game_setting(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let name = a.str(0)?;
    let v = g.world.setting_f32(name).or_else(|| EXE_SETTINGS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, v)| v));
    let v = v.ok_or_else(|| format!("game setting {name:?} is not defined"))?;
    Ok(Value::Num(v as f64))
}

/// Game settings the shipped scripts read that no master defines (the game keeps their
/// defaults itself), with the values the original returned [observed: `GetGameSetting`
/// in the capture `fa_batch1_queries`, 2026-10-16; the 27 trap-plate scripts read the
/// five `iLockLevelMax…` settings, docs/design/script-function-audit.md].
pub const EXE_SETTINGS: [(&str, f32); 5] = [
    ("iLockLevelMaxVeryEasy", 0.0),
    ("iLockLevelMaxEasy", 25.0),
    ("iLockLevelMaxAverage", 50.0),
    ("iLockLevelMaxHard", 75.0),
    ("iLockLevelMaxVeryHard", 100.0),
];

pub fn abs(_g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::Num(a.num(0)?.abs()))
}

/// The day of the week, 0 Sunday … 6 Saturday [documented: GECK wiki GetDayOfWeek]: the
/// whole days of GameDaysPassed modulo 7, day 0 (13 October 2281) a Sunday [observed,
/// function probes `fp1_queries` / `fp6_calendar` 2026-10-17: 5 at GameDaysPassed 12.39;
/// setting GameYear / GameMonth / GameDay to 28 February 2284 left it at 5, and each 24
/// hours after moved it to 6 and then 0 with GameDaysPassed 13 and 14]. The calendar
/// globals are not read: a script that sets them does not change the weekday. On a
/// calendar the clock keeps, both give the same day ([`weekday`]).
pub fn get_day_of_week(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let c = g.world.calendar().map_err(|e| e.to_string())?;
    Ok(Value::Num((c.days_passed as f64).floor().rem_euclid(7.0)))
}

/// The weekday of a game date (month from 1), 0 Sunday: the days since 13 October 2281,
/// the date GameDaysPassed counts from [documented: GECK wiki GetGameDaysPassed], on the
/// game's calendar of 365 days every year (docs/spec-notes/simulation.md §5b), that day
/// being a Sunday [observed: GetDayOfWeek read 5 on 25 October 2281 (capture
/// `fa_batch1_queries`, 2026-10-16), and the owner's screenshot of the wait menu shows
/// 19 October 2281 as a Saturday; the Gregorian calendar gives Tuesday and Wednesday].
/// The engine's wait menu counts the same way (`nvf-play` `wait::model::weekday`). There is
/// no 29 February [observed: 2280 and 2284, `fnv_sim::clock::days_in_month`].
pub fn weekday(year: i64, month: i64, day: i64) -> i64 {
    const BEFORE: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let day_number = |year: i64, month: i64, day: i64| year * 365 + BEFORE[(month - 1).clamp(0, 11) as usize] + day - 1;
    (day_number(year, month, day) - day_number(2281, 10, 13)).rem_euclid(7)
}

pub fn sqrt(_g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::Num(a.num(0)?.max(0.0).sqrt()))
}

/// 1: the PC version [decision, as IsXBox and IsPS3 read 0].
pub fn one(_g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(1.0))
}

/// A Layer 6 state with no Layer 6 yet: always 0 (not in combat, not talking).
pub fn zero(_g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    none()
}

/// 1 while a menu is open, or that menu with a menu code [documented: catalogue summary].
/// A conversation with the player is a menu: `MenuMode` and `MenuMode 1009` read 1 from
/// its greeting to its end [owner statement 2026-10-10: such a dialogue pauses the game;
/// documented: GECK wiki GetMenuTargetRef, owner export, for the code 1009]. `MenuMode 1`
/// is any of the Pip-Boy's menus; a loading screen is a menu (1007) [hypothesis]; the
/// codes: `Game::menu_open`, `game::menus`.
pub fn menu_mode(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::bool(match a.num(0).ok().filter(|c| *c != 0.0) {
        None => g.in_menu() || g.loading,
        Some(c) => c.fract() == 0.0 && (0.0..=u16::MAX as f64).contains(&c) && g.menu_open(c as u16),
    }))
}

/// `speaker.SayTo target topic [forceSubtitleFlag] [noTargetLookFlag]`: the speaker
/// says the topic's first line it can say to the target (`Game::say`,
/// docs/spec-notes/scripts.md §7.7). The two turn to each other while it is said; with
/// `noTargetLookFlag` 1 the target is not made to look at the speaker [documented: GECK
/// wiki SayTo, owner export] and only the speaker turns (`Game::facings`). The
/// subtitle flag shows the subtitle even out of hearing, as Say's ForceSubtitles.
pub fn say_to(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let speaker = ctx.subject()?;
    let (target, topic) = (a.form(0)?, a.form(1)?);
    if g.say_by_script(speaker, Some(target), topic, true).is_some() {
        if a.num_or(3, 0.0) != 0.0 {
            g.set_line_look(speaker, LineLook::Speaker);
        }
        // forceSubtitleFlag as Say's ForceSubtitles (207 shipped calls pass 1).
        g.set_line_flags(speaker, a.num_or(2, 0.0) != 0.0, false);
    }
    none()
}

/// `speaker.Say topic [ForceSubtitles] [Actor] [is2D] [isVoice]`: as SayTo, with no
/// listener [documented: GECK wiki Say, the parameters' names only]. ForceSubtitles 1
/// shows the line's subtitle even out of hearing (NVForever has no subtitle setting for
/// it to override); is2D 1 plays it as a 2D sound: at full volume, not panned, heard
/// wherever the player is (`Speech::force_subtitles`, `Speech::two_d`, read by
/// `nvf_play::voice` and `script_hud`) [hypothesis: what the names mean; the shipped
/// calls pass ForceSubtitles 1 and nothing after it, docs/design/script-function-params.md
/// §3]. Actor and isVoice are not read: no page says what they do and no shipped script
/// passes them.
pub fn say(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let speaker = ctx.subject()?;
    let topic = a.form(0)?;
    if g.say_by_script(speaker, None, topic, false).is_some() {
        g.set_line_flags(speaker, a.num_or(1, 0.0) != 0.0, a.num_or(3, 0.0) != 0.0);
    }
    none()
}

/// `speaker.StartConversation target [topic] [speakerLocation] [targetLocation]
/// [headTrack] [allowMovement]`: with the player as the target, the speaker starts a
/// conversation with the named topic, GREETING without one [documented: GECK wiki
/// StartConversation, owner export: a "force greet"; nothing happens when either actor
/// is disabled]. The speaker first walks up to the player when it is not near and the AI
/// runs (`Game::walk_up_greet`); without the AI the conversation starts at once. A call while a
/// conversation is running (except from the end script of the line that closes it:
/// `Game::hold_force_greet`), or when the speaker has no line to say, does nothing
/// [hypothesis]. With another actor as the target the two talk to each other and the
/// player overhears them (`Game::start_actor_conversation`, docs/spec-notes/dialogue.md
/// §5.1). There `headTrack` 0 turns nobody for the whole conversation [documented:
/// "Enter 0 to disable" head tracking during the conversation; hypothesis: left out, it
/// is on] (`Game::facings`). Both stand still while they talk unless `allowMovement` is 1
/// [documented: "Enter 1 to enable movement during conversation. Enter 0 to disable";
/// hypothesis: left out, they stand, as before the parameter was read]
/// (`Game::conversation_moves`). The location references are not read, nor are
/// head-track and allow-movement in a conversation with the player (the player's
/// conversation holds the world anyway).
pub fn start_conversation(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let speaker = ctx.subject()?;
    let target = a.form(0)?;
    if target != fnv_sim::FormId::PLAYER_REF {
        // Refusals are not errors of the script, as below.
        if g.start_actor_conversation(speaker, target, a.opt_form(1)).is_ok() {
            if a.num_or(4, 1.0) == 0.0 {
                g.set_line_look(speaker, LineLook::Nobody);
            }
            let talkers = [speaker, target];
            g.moving_talkers.retain(|&r| !talkers.contains(&r) && g.speech.iter().any(|s| s.conversation.is_some() && (s.speaker == r || s.target == Some(r))));
            if a.num_or(5, 0.0) != 0.0 {
                g.moving_talkers.extend(talkers);
            }
        }
        return none();
    }
    // A disabled player (the speaker's side is checked by `Game::start_dialogue`).
    if g.world.is_enabled(target).is_ok_and(|on| !on) {
        return none();
    }
    // Refusals (a conversation is running, the speaker is gone or has no line) are not
    // errors of the script. The speaker starts it: a force greet, which holds the player
    // also for a Goodbye greeting (`Game::force_greet`). Called from the end script of
    // the line that closes the running conversation, it starts once that has closed
    // (`Game::hold_force_greet`).
    if !g.hold_force_greet(speaker, a.opt_form(1)) && !g.walk_up_greet(speaker, a.opt_form(1)) {
        let _ = g.force_greet(speaker, a.opt_form(1));
    }
    none()
}

/// 1 once the actor has been in a conversation with the player [documented: GECK wiki
/// GetTalkedToPC, owner export: whether the NPC has ever spoken to the player;
/// hypothesis: SayTo lines do not count; a greeting with the Goodbye flag, said at the
/// player without a conversation (`Game::start_dialogue`), does].
pub fn get_talked_to_pc(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let who = ctx.subject()?;
    Ok(Value::bool(g.extra.get(&who).is_some_and(|e| e.talked_to_pc)))
}

/// The emotion type code (0 Neutral … 7 Pained) of the response the actor is saying
/// (`Game::dialogue_emotion`) [documented: GECK wiki GetDialogueEmotion, owner export].
pub fn get_dialogue_emotion(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let who = ctx.subject()?;
    Ok(Value::Num(g.dialogue_emotion(who) as f64))
}

/// 1 while the actor is saying a line (SayTo, Say, or a line of a conversation with the
/// player; 0 while the player chooses) (`Game::is_talking`).
pub fn is_talking(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let who = ctx.subject()?;
    Ok(Value::bool(g.is_talking(who)))
}

/// The menu each character-creation function opens (`Game::open_menu`) [observed: the
/// code of each was open after the stage whose script calls it, save nvfnew,
/// layer5-captures.md §10.2–10.3]. What the menu does (the name typed, the race chosen,
/// the tag skills and traits picked, the S.P.E.C.I.A.L. points) is the player's, outside
/// the scripts: closing it keeps the state as it is.
fn open(g: &mut Game, code: u16) -> Result<Value, String> {
    g.open_menu(code);
    none()
}

/// GetPlayerName: the name menu.
pub fn get_player_name(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::NAME)
}

/// ShowRaceMenu: the race menu.
pub fn show_race_menu(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.race_menu = crate::game::RaceMenu::Creator;
    open(g, menus::RACE)
}

/// ShowPlasticSurgeonMenu: the race menu as the plastic surgeon's, the face only (menu
/// 1036, `crate::game::RaceMenu::Surgeon`).
pub fn show_plastic_surgeon_menu(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.race_menu = crate::game::RaceMenu::Surgeon;
    open(g, menus::RACE)
}

/// ShowBarberMenu: the race menu as the barber's, the hair and beard only (menu 1036,
/// `crate::game::RaceMenu::Barber`).
pub fn show_barber_menu(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.race_menu = crate::game::RaceMenu::Barber;
    open(g, menus::RACE)
}

/// `SetTagSkills addedPoints [showInitialTaggedSkills]`: the tag-skill menu, with
/// `addedPoints` skills tagged in all; the second parameter defaults to 1 [documented:
/// GECK wiki SetTagSkills] (`crate::chargen::TagSheet::for_menu`).
pub fn set_tag_skills(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let n = a.num(0)?.max(0.0) as usize;
    g.ui.set_tags = Some((n, a.num_or(1, 1.0) != 0.0));
    open(g, menus::TAG_SKILLS)
}

/// ShowTraitMenu: the trait menu.
pub fn show_trait_menu(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::TRAITS)
}

/// `merchant.ShowBarterMenu [discount]`: the barter menu with the calling actor as the
/// merchant (`Game::barter_requests`; the engine draws it and closes the menu). The
/// discount is clamped to its documented range, -100 to 100 [documented: GECK wiki
/// ShowBarterMenu].
pub fn show_barter_menu(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let merchant = ctx.subject()?;
    let discount = a.num_or(0, 0.0).clamp(-100.0, 100.0) as i32;
    g.barter_requests.push(crate::game::BarterRequest { merchant, discount });
    open(g, menus::BARTER)
}

/// `station.ShowRecipeMenu category` (alias `Craft`): the recipe menu listing the
/// recipes of a category or subcategory (`Game::recipe_requests`; the engine draws it and
/// closes the menu) [documented: GECK wiki ShowRecipeMenu]. A form that is no recipe
/// category is an error of the call.
pub fn show_recipe_menu(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let category = a.form(0)?;
    if !g.data.recipe_categories.contains_key(&category) {
        return Err(format!("{category} is not a recipe category"));
    }
    let station = ctx.subject().ok();
    g.recipe_requests.push(crate::crafting::RecipeRequest { station, category });
    open(g, crate::crafting::RECIPE_MENU)
}

/// `vendor.ShowRepairMenu` (alias `srm`): the repair services menu with the calling
/// actor's Repair skill, the caps paid going to it (`Game::repair_requests`; the engine
/// draws it and closes the menu) [documented: GECK wiki ShowRepairMenu]. Without a
/// calling reference nothing happens [documented: the same page].
pub fn show_repair_menu(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let Ok(vendor) = ctx.subject() else { return none() };
    g.repair_requests.push(crate::repair::RepairRequest { vendor });
    open(g, crate::repair::REPAIR_SERVICES_MENU)
}

/// `ShowBlackJackMenuParams casino [minBet] [maxBet] [maxWinnings]`: the blackjack menu
/// at a casino's table (`Game::blackjack_requests`; the engine checks the chips and the
/// ban and opens it) [documented: GECK wiki ShowBlackJackMenuParams; docs/spec-notes/
/// blackjack.md §1]. Left out, the bets run from 1 to 200 (every table's script passes
/// those [observed]); a max winnings of 0 is the casino's own [hypothesis].
pub fn show_blackjack_menu(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let casino = a.form(0)?;
    if !g.data.casinos.contains_key(&casino) {
        return Err(format!("{casino} is not a casino"));
    }
    let min_bet = a.num_or(1, 1.0).max(1.0) as u32;
    let max_bet = (a.num_or(2, 200.0).max(0.0) as u32).max(min_bet);
    let max_winnings = a.num_or(3, 0.0).max(0.0) as u32;
    g.blackjack_requests.push(crate::blackjack::BlackjackRequest { casino, min_bet, max_bet, max_winnings });
    none()
}

/// `ShowRouletteMenuParams casino [minBet] [maxBet] [maxWinnings]`: the roulette menu at
/// a casino's table (`Game::roulette_requests`; the engine checks the chips and the ban and
/// opens it) [documented: GECK wiki ShowRouletteMenuParams; docs/spec-notes/roulette.md
/// §1]. Left out, the bets run from 1 to 200 (every table's script passes those
/// [observed]); a max winnings of 0 is the casino's own [hypothesis].
pub fn show_roulette_menu(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let casino = a.form(0)?;
    if !g.data.casinos.contains_key(&casino) {
        return Err(format!("{casino} is not a casino"));
    }
    let min_bet = a.num_or(1, 1.0).max(1.0) as u32;
    let max_bet = (a.num_or(2, 200.0).max(0.0) as u32).max(min_bet);
    let max_winnings = a.num_or(3, 0.0).max(0.0) as u32;
    g.roulette_requests.push(crate::roulette::RouletteRequest { casino, min_bet, max_bet, max_winnings });
    none()
}

/// `ShowSlotMachineMenuParams casino [minBet] [maxBet] [maxWinnings]`: the slot machine
/// menu for a casino (`Game::slots_requests`; the engine opens it when the player carries
/// the casino's chips and is not banned there) [documented: GECK wiki
/// ShowSlotMachineMenuParams; docs/spec-notes/slots.md §1]. Left out, the bets are 1 to 75
/// [hypothesis: The Tops' machines'].
pub fn show_slot_machine_menu(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let casino = a.form(0)?;
    if !g.data.casinos.contains_key(&casino) {
        return Err(format!("{casino} is not a casino"));
    }
    let min_bet = a.num_or(1, 1.0).max(1.0) as u32;
    let max_bet = (a.num_or(2, 75.0).max(0.0) as u32).max(min_bet);
    let max_winnings = a.num_or(3, 0.0).max(0.0) as u32;
    g.slots_requests.push(crate::slots::SlotsRequest { casino, min_bet, max_bet, max_winnings });
    none()
}

/// `ShowCaravanMenu deck difficulty maxBetPercent`: a game of Caravan against the calling
/// reference (`Game::caravan_requests`; the engine opens it) [documented: GECK wiki
/// ShowCaravanMenu; docs/spec-notes/caravan.md §1]. Left out, the share is a quarter
/// [hypothesis: the most common in the shipped calls].
pub fn show_caravan_menu(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let opponent = ctx.subject()?;
    let deck = a.form(0)?;
    if !g.data.caravan_decks.contains_key(&deck) {
        return Err(format!("{deck} is not a Caravan deck"));
    }
    let difficulty = a.num_or(1, 0.0) as i32;
    let max_bet_share = a.num_or(2, 0.25) as f32;
    g.caravan_requests.push(crate::caravan::CaravanRequest { opponent, deck, difficulty, max_bet_share });
    none()
}

/// `CardRef.AddCardToPlayer`: the card joins the player's Caravan cards [documented: GECK
/// wiki AddCardToPlayer]. Every shipped call is in the cards' own script, on the card in
/// the player's inventory (OnAdd) or in the world (OnActivate) [observed: SCPT
/// 0x0015D7B0; caravan.md §2].
pub fn add_card_to_player(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let card = match (ctx.on, ctx.inst) {
        (None, Some(crate::game::InstKey::Item { item, .. })) => item,
        _ => {
            let r = ctx.subject()?;
            g.world.get(r).map(|s| s.base).unwrap_or(r)
        }
    };
    if !crate::caravan::add_card(g, card) {
        return Err(format!("{card} is not a Caravan card"));
    }
    none()
}

/// `ShowLoveTesterMenuParams points`: the Vigor Tester's S.P.E.C.I.A.L. menu, the seven
/// attributes adding up to `points` [documented: GECK wiki ShowLoveTesterMenuParams,
/// "the total number of SPECIAL points the player will have to distribute"], as
/// SetSPECIALPoints (`crate::chargen::SpecialSheet::for_menu`). Without points: the
/// menu's own total [hypothesis].
pub fn show_love_tester_menu(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    if let Ok(n) = a.int(0) {
        g.ui.special = Some(super::ui::SpecialMenu::Set(n));
    }
    open(g, menus::LOVE_TESTER)
}

/// `ApplyImageSpaceModifier modifier`: the image space modifier starts playing on the
/// screen [documented: GECK wiki ApplyImageSpaceModifier, owner export]
/// (`Game::apply_image_modifier`, docs/spec-notes/image-modifiers.md §3). A form that is
/// not an image space modifier is an error of the call.
pub fn apply_image_space_modifier(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let m = a.form(0)?;
    if !g.data.image_modifiers.contains_key(&m) {
        return Err(format!("{m} is not an image space modifier"));
    }
    g.apply_image_modifier(m);
    none()
}

/// `RemoveImageSpaceModifier modifier`: the modifier leaves the screen [documented: GECK
/// wiki RemoveImageSpaceModifier, owner export]; nothing happens when it is not on it.
pub fn remove_image_space_modifier(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.remove_image_modifier(a.form(0)?);
    none()
}

/// `IsImageSpaceActive modifier`: 1 while the modifier is on the screen [documented:
/// GECK wiki IsImageSpaceActive, owner export].
pub fn is_image_space_active(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(g.image_modifier_active(a.form(0)?))
}

/// `PlayBink "file" [interruptable] [muteAudio] [pauseMusic] [letterboxed]`: a Bink film
/// under `Data\Video` plays over the screen [documented: GECK wiki PlayBink; the flags'
/// defaults 0, 1, 1, 1 and meanings are the page's]. The film is handed to the engine
/// (`UiRequests::films`, `nvf_play::intro`), which holds the game while it plays (the
/// page: it "appears to stop game time"); the script goes on with its next line at
/// once [hypothesis: whether the original's call returns before or after the film is
/// not documented; with the game held the order of the script's next lines is the
/// same]. docs/spec-notes/bink-films.md.
pub fn play_bink(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let name = a.str(0)?;
    let path = super::ui::film_path(name).ok_or_else(|| format!("{name:?} names no file under Data"))?;
    let flag = |i: usize, default: f64| a.num_or(i, default) != 0.0;
    let film = super::ui::FilmRequest { path, interruptible: flag(1, 0.0), mute_audio: flag(2, 1.0), pause_music: flag(3, 1.0), letterboxed: flag(4, 1.0) };
    tracing::info!(film = %film.path, interruptible = film.interruptible, "PlayBink");
    g.ui.films.push(film);
    none()
}

/// `TriggerScreenSplatter count duration size opacity`: blood splats over the screen
/// (docs/spec-notes/screen-splatter.md §2) [hypothesis: the parameters are those the
/// JohnnyGuitar NVSE extension `TriggerScreenSplatterEx` documents before its texture
/// names (splatterCount, splatterDuration, sizeMultiplier, opacityMultiplier; GECK
/// wiki, owner export), which reads as the vanilla call with textures added; the
/// vanilla page is empty]. A missing or 0 duration is the game setting's. Out-of-range
/// values are bounded [engineering choice: no shipped call goes beyond count 100, 25 s,
/// size 3, opacity 2]: count at least 0 (the engine draws at most 1,000),
/// duration at most an hour, the factors 0 to [`SPLATTER_MAX_FACTOR`]; a value that is not
/// a number counts as missing.
pub fn trigger_screen_splatter(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let finite = |i: usize, d: f64| Some(a.num_or(i, d)).filter(|v| v.is_finite()).unwrap_or(d);
    let count = finite(0, 0.0).clamp(0.0, f64::from(u32::MAX)) as u32;
    let duration = Some(finite(1, 0.0).min(3600.0) as f32).filter(|d| *d > 0.0);
    let factor = |i: usize| finite(i, 1.0).clamp(0.0, SPLATTER_MAX_FACTOR) as f32;
    let t = super::ui::SplatterTrigger { count, duration, size: factor(2), opacity: factor(3) };
    g.ui.splatter.triggers.push(t);
    none()
}

/// The largest size or opacity factor TriggerScreenSplatter takes.
pub const SPLATTER_MAX_FACTOR: f64 = 10.0;

/// `ClearScreenSplatter`: the splats leave the screen [documented: GECK wiki
/// ClearScreenSplatter, "Clears the screen splatter"]; calls not yet drawn go too.
pub fn clear_screen_splatter(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.ui.splatter.triggers.clear();
    g.ui.splatter.clears += 1;
    none()
}

/// `SetScreenSplatterFade start fade`: from `start` (a share of each splat's time) the
/// splats fade out, when `fade` is not 0 [hypothesis: the page is empty; the only
/// caller passes 0.5 and 1 before its triggers (`QUST 0x00102037` stages 36 and 50
/// [observed]), the game setting fBloodSplatterFadeStart is 0.4, and the Ex variant's
/// last parameter is an int `fade`; screen-splatter.md §2]. The original game quit at
/// this call, made as the first splatter call of a session from a capture job's function
/// script, twice [observed 2026-10-18, screen-splatter.md §5]; here it only records the
/// two values, with or without splats on the screen: a start that is not a number is 0
/// (the original's splats fade from their start, screen-splatter.md §5), any other is
/// held to 0–1.
pub fn set_screen_splatter_fade(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let start = Some(a.num_or(0, f64::NAN)).filter(|v| v.is_finite()).map_or(0.0, |v| v.clamp(0.0, 1.0)) as f32;
    let on = Some(a.num_or(1, 1.0)).filter(|v| v.is_finite()).is_none_or(|v| v != 0.0);
    g.ui.splatter.fade = Some((start, on));
    none()
}

/// `SetCellImageSpace cell imageSpace`: the cell's image space becomes this one, kept in
/// the save (`FormChanges::cell_image_spaces`) [documented: GECK wiki
/// SetCellImageSpace]. The page's note: on a cell that is loaded the new image space
/// shows only once the cell loads again (left and entered again, or a save made there
/// loaded); the engine follows it (`nvforever` `cell_image_space`).
pub fn set_cell_image_space(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let (cell, space) = (a.form(0)?, a.form(1)?);
    if g.data.sig(cell) != Some(*b"CELL") {
        return Err(format!("{} is not a cell", g.data.name(cell)));
    }
    if g.data.sig(space) != Some(*b"IMGS") {
        return Err(format!("{} is not an image space", g.data.name(space)));
    }
    g.forms.cell_image_spaces.insert(cell, space);
    none()
}

/// `IsDLCInstalled "name"`: 1 when the named add-on is in the load order. No wiki page or
/// catalogue summary describes it; the only caller (`SCPT 0x0017912E`, the pack items'
/// quest script) asks for "tribal pack", "caravan pack", "mercenary pack" and "classic
/// pack" [observed: decoded scripts], which are the pack plugins' names without the
/// space and the extension (TribalPack.esm …). So: a plugin of the load order whose name,
/// without its extension, equals the argument once spaces and case are ignored
/// [hypothesis: how the original finds an add-on by name is not documented].
pub fn is_dlc_installed(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
    let want = squash(a.str(0)?);
    yes(!want.is_empty() && g.data.load_order.iter().any(|p| squash(p.rsplit_once('.').map_or(p.as_str(), |(stem, _)| stem)) == want))
}

/// `SetGlobalTimeMultiplier mult [keepSoundUnchanged]`: the speed of the whole game,
/// 1 normal, 0.5 slow motion [documented: GECK wiki SetGlobalTimeMultiplier]. The
/// engine takes it (`UiRequests::time_multiplier`, read by `nvf_play`). Sounds play at
/// the multiplier too unless keepSoundUnchanged is 1 ("sounds will not be sped up/slowed
/// down when the time mult is changed" [documented: same page]); the shipped slow motion
/// asks for unchanged sound (`QUST 0x00102037` stage 27 [observed]). 0 "will break the
/// game" [documented]: values at or below 0 are refused.
pub fn set_global_time_multiplier(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let m = a.num(0)?;
    if m <= 0.0 || !m.is_finite() {
        return Err(format!("time multiplier {m} refused (must be above 0)"));
    }
    g.ui.time_multiplier = Some(m);
    g.ui.time_keeps_sound = a.num_or(1, 0.0) != 0.0;
    none()
}

/// The textures scripts swapped on references' shapes: per reference, the shape's name
/// (lower case, without the leading `#`s) and the texture path under `textures\` with
/// its extension, the last call per shape kept; `generation` counts the calls, so the
/// engine sees new ones.
#[derive(Debug, Clone, Default)]
pub struct TextureSwaps {
    pub by_ref: std::collections::BTreeMap<fnv_sim::FormId, Vec<(String, String)>>,
    pub generation: u64,
    /// The screen textures SetSecuritronExpression set, by target: unlike SwapTexture's
    /// they are saved (`GameSnapshot::securitron_faces`) and put back on a load.
    pub securitron: std::collections::BTreeMap<fnv_sim::FormId, String>,
}

impl TextureSwaps {
    /// Draw `texture` on `on`'s shape `shape` (the last call per shape kept).
    fn swap(&mut self, on: fnv_sim::FormId, shape: String, texture: String) {
        let swaps = self.by_ref.entry(on).or_default();
        swaps.retain(|(s, _)| *s != shape);
        swaps.push((shape, texture));
        self.generation += 1;
    }

    /// Show `texture` (a path as [`texture_swap_names`] gives it) on the Securitron
    /// `on`'s screen, and keep it for the save.
    pub fn set_securitron(&mut self, on: fnv_sim::FormId, texture: String) {
        let shape = SECURITRON_SCREEN.to_ascii_lowercase();
        self.securitron.insert(on, texture.clone());
        self.swap(on, shape, texture);
    }
}

/// The shape name and texture path a `SwapTexture` call names, as the engine looks
/// them up: `"##gs01:0"` → `gs01:0`; `"interface\endgame\endscrn_pf05"` →
/// `textures\interface\endgame\endscrn_pf05.dds` [hypothesis: the calls name a shape
/// of the reference's model (`pipboycard01:0` is the only shape of
/// `pipboycard01.nif`) and a texture without `textures\` and extension (the files exist
/// so: `textures\clutter\rorchachtest\inkblot03.dds`); the meaning of `##` is not
/// known, read as "the shape of that name anywhere in the model"].
pub fn texture_swap_names(shape: &str, texture: &str) -> (String, String) {
    let shape = shape.trim().trim_start_matches('#').to_ascii_lowercase();
    let mut t = texture.trim().replace('/', "\\").to_ascii_lowercase();
    if !t.starts_with("textures\\") {
        t = format!(r"textures\{t}");
    }
    if !t.ends_with(".dds") {
        t.push_str(".dds");
    }
    (shape, t)
}

/// `SwapTexture <ref> "shape" "texture"`: draw `texture` on the named shape of the model
/// of the reference given as the first parameter (the end slides, the Rorschach cards of
/// Doc Mitchell's test, the Euclid's C-Finder's screen on the player) [observed: the
/// decoded calls have no calling reference and a reference as the first parameter, all
/// 242 in FalloutNV.esm, e.g. `INFO 0x00105599`'s on `TempRorschachTest01REF`;
/// docs/design/script-stubs.md §4; no GECK wiki page]. Kept per reference for the engine
/// (`TextureSwaps`).
pub fn swap_texture(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let on = a.form(0)?;
    let (shape, texture) = texture_swap_names(a.str(1)?, a.str(2)?);
    g.texture_swaps.swap(on, shape, texture);
    none()
}

/// `ref.SetSecuritronExpression target face expression`: the Securitron's monitor shows
/// `textures\creatures\securitron\<face>_<expression>.dds` [documented: GECK wiki
/// SetSecuritronExpression; observed: the screen models draw `<face>_Neutral.dds` on the
/// shape `Screen01:0`, docs/spec-notes/effect-shaders.md §5]. A texture swap of that
/// shape on the target (`TextureSwaps::set_securitron`), kept in the save: "this function
/// persists across saves and game restarts" [documented: GECK wiki
/// SetSecuritronExpression]. A later SwapTexture of the same shape shows until a load,
/// which puts the last expression back [hypothesis].
pub fn set_securitron_expression(g: &mut Game, _: &Ctx, a: &Args) -> Result<Value, String> {
    let on = a.form(0)?;
    let texture = format!(r"creatures\securitron\{}_{}", a.str(1)?.trim(), a.str(2)?.trim());
    let (_, texture) = texture_swap_names(SECURITRON_SCREEN, &texture);
    g.texture_swaps.set_securitron(on, texture);
    none()
}

/// The shape of a Securitron's screen models that shows its face [observed:
/// `nvsecuritron*screen01.nif`, effect-shaders.md §5].
pub const SECURITRON_SCREEN: &str = "Screen01:0";

/// An effect shader a script plays on a reference (`PlayMagicShaderVisuals`).
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderPlay {
    pub on: fnv_sim::FormId,
    /// The EFSH record.
    pub shader: fnv_sim::FormId,
    /// Seconds asked for; `None`: until stopped. The engine ends it after
    /// max(duration, one cycle) (docs/spec-notes/effect-shaders.md §3).
    pub duration: Option<f64>,
    /// Counts the plays, so the engine can tell a shader played again from one playing on.
    pub serial: u64,
}

/// What scripts asked of references' looks (effect shaders, actor alpha), for the engine
/// to draw. Not part of the saved state [design].
#[derive(Debug, Clone, Default)]
pub struct ScriptLooks {
    /// Effect shaders playing by script: one per (reference, shader), the last call kept.
    pub shaders: Vec<ShaderPlay>,
    next_serial: u64,
    /// Actors' alpha (`SetActorAlpha`), below 1 only.
    pub alpha: std::collections::BTreeMap<fnv_sim::FormId, f32>,
    /// Counts the changes of `shaders` and `alpha`.
    pub generation: u64,
    /// References a `Disable 1` is fading out, still enabled until this [`Game::time`]
    /// (`refs::disable`).
    pub fading_out: std::collections::BTreeMap<fnv_sim::FormId, f64>,
    /// References whose fade-out hold ended and that are now disabled, for the engine to
    /// take (their collision and enable children follow).
    pub disabled_after_fade: Vec<fnv_sim::FormId>,
    /// References whose critical stage turned to or from one that culls them
    /// (`objects::set_critical_stage`), for the engine to draw again or hide.
    pub critical_changed: Vec<fnv_sim::FormId>,
    /// `MoveToMarkerWithFade` moves waiting for their reference's fade out
    /// (`refs::move_to_marker_with_fade`); saved, so a save during the fade still moves.
    pub fade_moves: std::collections::BTreeMap<fnv_sim::FormId, FadeMove>,
    /// Fades for the engine to start, (reference, in): those of `MoveToMarkerWithFade`.
    pub fades: Vec<(fnv_sim::FormId, bool)>,
}

/// A `MoveToMarkerWithFade` waiting for its fade out: the marker and offsets of the call,
/// and the [`Game::time`] at which the move is made.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FadeMove {
    pub marker: fnv_sim::FormId,
    pub offset: [f64; 3],
    pub at: f64,
}

impl ScriptLooks {
    /// The engine has ended a play (its time is up).
    pub fn end(&mut self, serial: u64) {
        self.shaders.retain(|p| p.serial != serial);
        self.generation += 1;
    }
}

/// `ref.PlayMagicShaderVisuals shader [duration]`: play an effect shader on the calling
/// reference (the player without one) for the duration, or until stopped [documented:
/// GECK wiki PlayMagicShaderVisuals]. The same shader played again on the same reference
/// starts over [hypothesis].
pub fn play_magic_shader_visuals(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let on = ctx.subject_or_player();
    let shader = a.form(0)?;
    let duration = a.num(1).ok().filter(|d| *d > 0.0);
    let looks = &mut g.script_looks;
    looks.shaders.retain(|p| !(p.on == on && p.shader == shader));
    looks.next_serial += 1;
    looks.shaders.push(ShaderPlay { on, shader, duration, serial: looks.next_serial });
    looks.generation += 1;
    none()
}

/// `ref.StopMagicShaderVisuals shader`: stop the effect shader a script plays on the
/// calling reference [documented: GECK wiki StopMagicShaderVisuals].
pub fn stop_magic_shader_visuals(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let on = ctx.subject_or_player();
    let shader = a.form(0)?;
    let looks = &mut g.script_looks;
    looks.shaders.retain(|p| !(p.on == on && p.shader == shader));
    looks.generation += 1;
    none()
}

/// `ref.SetActorAlpha alpha`: how opaque the calling actor is drawn, 0 (unseen) to 1
/// [documented: GECK wiki SetActorAlpha]; held to 0..1, 1 draws it as before.
pub fn set_actor_alpha(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let on = ctx.subject()?;
    let alpha = (a.num(0)? as f32).clamp(0.0, 1.0);
    let looks = &mut g.script_looks;
    if alpha >= 1.0 {
        looks.alpha.remove(&on);
    } else {
        looks.alpha.insert(on, alpha);
    }
    looks.generation += 1;
    none()
}

/// `ref.Reset3DState`: forget the calling reference's animation state (traps triggered
/// before are set again by their scripts) [documented: GECK wiki Reset3DState; what the
/// state covers here, docs/spec-notes/effect-shaders.md §5, hypothesis].
pub fn reset_3d_state(g: &mut Game, ctx: &Ctx, _: &Args) -> Result<Value, String> {
    let on = ctx.subject()?;
    g.anims.reset(on);
    none()
}

/// The widest field and the most digits a message's specifier writes: a larger width or
/// precision is cut to this, so a broken plugin's `%.4000000000f` cannot build a huge
/// string [design: #1134; the same limit as the extender's formatter, `fnv_script`'s
/// `MAX_FORMAT_FIELD`]. Base-game messages use far smaller values [observed: `%.0f`, `%.2f`].
pub const MAX_MESSAGE_FIELD: usize = 512;

/// A message text with its format specifiers filled from `vars` in order, or None when
/// there are fewer variables than specifiers [documented: GECK wiki ShowMessage,
/// "Formatting messages"]: `%[flags][width][.precision]` then `f`, `e` or `g`, flags `+`
/// (sign on positives), space (a space before positives), `-` (left-aligned) and `0`
/// (zero fill); `%%` is a percent sign. The rules follow C's printf, which the page's
/// examples match [hypothesis]; `e` writes a three-digit exponent (`1.23e+005`) as the
/// page shows [documented], its letter case following the specifier [hypothesis]. A `%`
/// not starting a specifier is kept as written [hypothesis]. Width and precision are cut
/// to [`MAX_MESSAGE_FIELD`].
pub fn format_message(text: &str, vars: &[f64]) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut next = vars.iter();
    let mut rest = text;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        let spec = &rest[at + 1..];
        if let Some(after) = spec.strip_prefix('%') {
            out.push('%');
            rest = after;
            continue;
        }
        let flags_len = spec.find(|c: char| !"+- 0".contains(c)).unwrap_or(spec.len());
        let flags = &spec[..flags_len];
        let after_flags = &spec[flags_len..];
        let width_len = after_flags.find(|c: char| !c.is_ascii_digit()).unwrap_or(after_flags.len());
        // Digits past usize parse as the cap, not as no width.
        let width: usize = after_flags[..width_len].parse().unwrap_or(if width_len > 0 { MAX_MESSAGE_FIELD } else { 0 }).min(MAX_MESSAGE_FIELD);
        let mut tail = &after_flags[width_len..];
        let mut precision = None;
        if let Some(p) = tail.strip_prefix('.') {
            let n = p.find(|c: char| !c.is_ascii_digit()).unwrap_or(p.len());
            precision = Some(p[..n].parse().unwrap_or(if n > 0 { MAX_MESSAGE_FIELD } else { 0 }).min(MAX_MESSAGE_FIELD));
            tail = &p[n..];
        }
        let Some(conv) = tail.chars().next().filter(|c| "feEgG".contains(*c)) else {
            out.push('%');
            rest = spec;
            continue;
        };
        let v = *next.next()?;
        let body = match conv {
            'f' => format!("{:.*}", precision.unwrap_or(6), v.abs()),
            'e' | 'E' => exp_form(v.abs(), precision.unwrap_or(6), conv == 'E'),
            _ => general_form(v.abs(), precision.unwrap_or(6), conv == 'G'),
        };
        let sign = if v < 0.0 && body.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
            "-"
        } else if flags.contains('+') {
            "+"
        } else if flags.contains(' ') {
            " "
        } else {
            ""
        };
        let pad = width.saturating_sub(sign.len() + body.len());
        if flags.contains('-') {
            out.push_str(sign);
            out.push_str(&body);
            out.extend(std::iter::repeat_n(' ', pad));
        } else if flags.contains('0') {
            out.push_str(sign);
            out.extend(std::iter::repeat_n('0', pad));
            out.push_str(&body);
        } else {
            out.extend(std::iter::repeat_n(' ', pad));
            out.push_str(sign);
            out.push_str(&body);
        }
        rest = &tail[1..];
    }
    out.push_str(rest);
    Some(out)
}

/// `%e` of a non-negative number: one digit, `precision` decimals, a signed exponent of
/// at least three digits.
fn exp_form(v: f64, precision: usize, upper: bool) -> String {
    let s = format!("{:.*e}", precision, v);
    let (mantissa, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let e = if upper { 'E' } else { 'e' };
    format!("{mantissa}{e}{}{:03}", if exp < 0 { '-' } else { '+' }, exp.abs())
}

/// `%g` of a non-negative number: `precision` significant digits (at least 1), fixed
/// notation unless the exponent is below −4 or not below the precision, trailing zeros
/// dropped (C's rule).
fn general_form(v: f64, precision: usize, upper: bool) -> String {
    let p = precision.max(1);
    let exp = if v == 0.0 {
        0
    } else {
        let s = format!("{:.*e}", p - 1, v);
        s.split_once('e').and_then(|(_, e)| e.parse::<i32>().ok()).unwrap_or(0)
    };
    let trim = |s: &str| if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s.to_string() };
    if exp < -4 || exp >= p as i32 {
        let s = exp_form(v, p - 1, upper);
        let at = s.find(['e', 'E']).unwrap_or(s.len());
        format!("{}{}", trim(&s[..at]), &s[at..])
    } else {
        trim(&format!("{:.*}", (p as i32 - 1 - exp).max(0) as usize, v))
    }
}

#[cfg(test)]
mod tests {
    use super::{format_message, message_vars, Args, MAX_MESSAGE_FIELD};
    use fnv_script::vm::{ArgValue, Value};

    #[test]
    fn huge_widths_and_precisions_are_cut() {
        // #1134: a broken plugin's specifier must not build a gigabyte string.
        let t = std::time::Instant::now();
        for spec in ["%.4000000000f", "%4000000000f", "%099999999999999999999999.3e", "%-4000000000g"] {
            let s = format_message(spec, &[1.5]).unwrap();
            assert!(s.len() <= MAX_MESSAGE_FIELD + 16, "{spec}: {}", s.len());
        }
        assert_eq!(format_message("%.4000000000f", &[1.5]).unwrap().len(), 2 + MAX_MESSAGE_FIELD);
        assert_eq!(format_message("%4000000000f", &[1.5]).unwrap().len(), MAX_MESSAGE_FIELD);
        assert!(t.elapsed().as_secs() < 2);
        // Ordinary specifiers are unchanged.
        assert_eq!(format_message("%5.1f|%.0f", &[2.25, 28.0]).as_deref(), Some("  2.2|28"));
    }

    #[test]
    fn compiled_format_variables_are_unpacked_and_the_duration_skipped() {
        // ShowMessage TrapLowSkillExplosivesMsg NQlvl.TRAPMineSkillReq, as compiled.
        let compiled = [ArgValue::Value(Value::Ref(0x38888)), ArgValue::Vars(vec![Value::Num(28.0)]), ArgValue::Int32(0)];
        assert_eq!(message_vars(&Args(&compiled)), [28.0]);
        let text = [ArgValue::Value(Value::Ref(0x38888)), ArgValue::Value(Value::Num(3.0)), ArgValue::Value(Value::Num(5.0))];
        assert_eq!(message_vars(&Args(&text)), [3.0, 5.0]);
        let msg = "(Explosives %.0f or higher required)";
        assert_eq!(format_message(msg, &message_vars(&Args(&compiled))).as_deref(), Some("(Explosives 28 or higher required)"));
    }

    #[test]
    fn fills_specifiers_in_order() {
        assert_eq!(format_message("Rank %.0f of %.0f", &[3.0, 5.0]).as_deref(), Some("Rank 3 of 5"));
        assert_eq!(format_message("%.2f caps", &[1.5]).as_deref(), Some("1.50 caps"));
        assert_eq!(format_message("No values", &[2.0]).as_deref(), Some("No values"));
        assert_eq!(format_message("100%%", &[]).as_deref(), Some("100%"));
    }

    #[test]
    fn too_few_variables_shows_nothing() {
        assert_eq!(format_message("%.0f and %.0f", &[1.0]), None);
    }

    #[test]
    fn width_and_flags() {
        // The wiki's own example: "Number    12 wins".
        assert_eq!(format_message("Number %5.0f wins", &[12.0]).as_deref(), Some("Number    12 wins"));
        assert_eq!(format_message("%-5.0f|", &[12.0]).as_deref(), Some("12   |"));
        assert_eq!(format_message("%05.0f", &[12.0]).as_deref(), Some("00012"));
        assert_eq!(format_message("%+.0f %+.0f", &[3.0, -3.0]).as_deref(), Some("+3 -3"));
        assert_eq!(format_message("% .0f", &[3.0]).as_deref(), Some(" 3"));
        assert_eq!(format_message("%.0f", &[-0.2]).as_deref(), Some("0"));
    }

    #[test]
    fn general_and_exponent_forms() {
        assert_eq!(format_message("%g", &[25.0]).as_deref(), Some("25"));
        assert_eq!(format_message("%g", &[0.5]).as_deref(), Some("0.5"));
        // The wiki: 1000000 or more shows in scientific notation (1E+006).
        assert_eq!(format_message("%g", &[1_000_000.0]).as_deref(), Some("1e+006"));
        assert_eq!(format_message("%.2e", &[123000.0]).as_deref(), Some("1.23e+005"));
    }

    #[test]
    fn a_lone_percent_is_kept() {
        assert_eq!(format_message("50% off", &[]).as_deref(), Some("50% off"));
    }
}
