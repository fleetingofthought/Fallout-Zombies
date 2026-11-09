//! Radios in the runtime (docs/spec-notes/radio.md): which receivers are switched on,
//! whether a station broadcasts, the station's programme as dialogue (which line it
//! says next, with the line's begin and end result scripts), the conversations scripts
//! start, the Pip-Boy radio and actors made radios. No engine types; how a receiver
//! sounds, what the Pip-Boy receives and when lines play are the engine's (`nvf-play`
//! `radio.rs`).
//!
//! - **A receiver** (an activator whose base names a radio station) starts switched off
//!   and each default activation switches it on or off [hypothesis: the receivers'
//!   scripts — VintageRadioStartOnSCRIPT activates its own reference once on load, so a
//!   radio "starts on" only by being activated; radio.md §4]. The runtime counts the
//!   default activations of every reference whose activation is "activated"
//!   (activators, furniture), as scripts can activate a radio on the first frame, before
//!   the engine could say which references are receivers; the engine reads the counts of
//!   its receivers ([`Game::radio_on`]).
//! - **A station broadcasts** when SetBroadcastState last said so, else as its base's
//!   Continuous Broadcast flag says [documented: GECK wiki Radio Stations, "the default
//!   state … is set using the Continuous Broadcast checkbox"; hypothesis for which
//!   header bit that is, radio.md §2]. `GetBroadcastState` reads the same
//!   ([`Game::broadcasting`]).
//! - **The programme**: a station says lines as an actor does, with its placed reference
//!   as the speaker, so the radio quests' own conditions (GetIsID of the station) pick
//!   whose lines it has [observed: every radio quest of the masters gates its lines that
//!   way, radio.md §3]. It starts with RadioHello [documented: GECK wiki Radio Tab and
//!   Radio Stations: RadioHello starts radio conversations as HELLO does]. After a line
//!   it goes on with the topics the line leads to (its choices, TCLT, and the topics
//!   whose lines name its topic in their Link From, dialogue.md §3.5): one of them that
//!   has a line the station can say, picked at random [hypothesis: radio.md §5]; with
//!   none, back to RadioHello [hypothesis]. Within a topic the line is picked as for any
//!   speaker ([`Game::select_line`]: quest priority, stored order, Random pools), except
//!   that a line's fixed speaker does not keep the station from saying it: it names
//!   whose voice the line is in [documented: SetNPCRadio; observed: Black Mountain
//!   Radio's segments name Tabitha and Raul Tejada].
//! - **StartRadioConversation [topic]** asks for one conversation on the station: from
//!   the topic, else RadioHello, along the links until a line links nowhere; a
//!   broadcasting station is interrupted and goes on with its programme afterwards, a
//!   silent one plays the conversation and falls silent again, and the broadcast state
//!   is not changed [documented: GECK wiki StartRadioConversation]. The calls are kept
//!   for the engine ([`Radios::conversations`]), which times the lines.
//! - **The Pip-Boy radio** ([`PipBoyRadio`]): `PipBoyRadio 0` and `PipBoyRadioOff` turn it
//!   off, `PipBoyRadio 1 [station]` on (to the station given, else the last one listened
//!   to), `PipBoyRadio tune <station>` tunes it only while it is on [documented: GECK
//!   wiki PipBoyRadio, PipBoyRadioOff]. A disabled or out-of-range station "cannot be
//!   selected" [documented]: the tuning asked for waits in [`PipBoyRadio::tune`] for the
//!   engine, which knows where the player is.
//! - **SetNPCRadio [0/1] station**: the actor plays the station as a radio; 0 turns it
//!   off whatever station is named; an actor playing one keeps it until turned off
//!   [documented: GECK wiki SetNPCRadio notes]. The masters always give the 0/1 first
//!   [observed: the six calls of the masters' compiled scripts].
//! - `ForceRadioStationUpdate` "forces radio update" [documented: GECK wiki]: the engine
//!   looks at every radio each frame, so it is counted only [design].

use crate::dialogue::Says;
use crate::functions::{base_of, none, yes, Args, Ctx};
use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::FormId;
use rand_core::RngCore;
use std::collections::BTreeMap;

/// The topic radio programmes start from [documented: GECK wiki Radio Tab].
pub const RADIO_HELLO: &str = "RadioHello";

/// TACT record-header bit of the editor's "Continuous Broadcast" box [hypothesis:
/// radio.md §2; the same bit as `fnv_world::radio::station_flags::CONTINUOUS`].
pub const CONTINUOUS: u32 = 0x4000_0000;

/// TACT record-header bit of the editor's "Radio Station" box [hypothesis: radio.md §2,
/// set on every station a receiver names].
pub const RADIO_STATION: u32 = 0x0002_0000;

#[derive(Debug, Clone, Default)]
pub struct Radios {
    /// Default activations whose outcome was "activated", by reference.
    pub activations: BTreeMap<FormId, u32>,
    pub pipboy: PipBoyRadio,
    /// StartRadioConversation calls the engine has not taken yet: the station's placed
    /// reference and the topic named.
    pub conversations: Vec<(FormId, Option<FormId>)>,
    /// SetNPCRadio: actor → the station's placed reference it plays.
    pub npc_radios: BTreeMap<FormId, FormId>,
    /// ForceRadioStationUpdate calls.
    pub force_updates: u32,
}

/// The Pip-Boy radio as scripts left it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PipBoyRadio {
    pub on: bool,
    /// The station's placed reference it is tuned to; kept while off, as `PipBoyRadio 1`
    /// goes back to "the last radio station the player was listening to" [documented].
    pub station: Option<FormId>,
    /// A station a script asked for, not yet checked by the engine.
    pub tune: Option<FormId>,
    /// Calls of PipBoyRadio and PipBoyRadioOff.
    pub script_calls: u32,
}

impl Game<'_> {
    /// A default activation of `target` that "activated" it.
    pub(crate) fn radio_activated(&mut self, target: FormId) {
        *self.radio.activations.entry(target).or_default() += 1;
    }

    /// Whether the receiver `r` was switched: activated an odd number of times. A
    /// receiver that starts on (the engine's rule) is off then.
    pub fn radio_on(&self, r: FormId) -> bool {
        self.radio.activations.get(&r).is_some_and(|n| n % 2 == 1)
    }

    /// How many times the receiver `r` was switched on or off.
    pub fn radio_switches(&self, r: FormId) -> u32 {
        self.radio.activations.get(&r).copied().unwrap_or(0)
    }

    /// Whether the base of the placed station `station` has the Continuous Broadcast flag.
    pub fn station_continuous(&self, station: FormId) -> bool {
        base_of(self, station).and_then(|b| self.data.station_flags.get(&b)).is_some_and(|f| f & CONTINUOUS != 0)
    }

    /// Whether the station whose placed reference is `station` broadcasts: SetBroadcastState's
    /// last word, else its base's Continuous Broadcast flag.
    pub fn broadcasting(&self, station: FormId) -> bool {
        self.extra.get(&station).and_then(|e| e.broadcast).unwrap_or_else(|| self.station_continuous(station))
    }

    /// The StartRadioConversation calls made since the last time.
    pub fn take_radio_conversations(&mut self) -> Vec<(FormId, Option<FormId>)> {
        std::mem::take(&mut self.radio.conversations)
    }

    /// A line of `topic` (RadioHello when `None`) the station can say now: the start of a
    /// conversation.
    pub fn radio_start(&mut self, station: FormId, topic: Option<FormId>) -> Option<(FormId, FormId)> {
        let topic = topic.or_else(|| self.data.by_edid(RADIO_HELLO))?;
        self.select_line_as(station, Some(FormId::PLAYER_REF), topic, |_| true, Says::Station).map(|i| (topic, i))
    }

    /// The line that follows `after` in its conversation: one of the topics it leads to
    /// (its choices and the topics whose lines name its topic in their Link From,
    /// [`Game::links_after`]) that has a line the station can say now, at random.
    /// `None`: the conversation ends.
    pub fn radio_follow(&mut self, station: FormId, after: FormId) -> Option<(FormId, FormId)> {
        let dv = self.dialogue_view();
        let def = dv.info(&after)?;
        let topic = def.topic?;
        let mut linked: Vec<(FormId, FormId)> = Vec::new();
        for link in self.links_after(topic, def) {
            if let Some(info) = self.link_line(station, Some(FormId::PLAYER_REF), link, Says::Station) {
                linked.push((link.topic, info));
            }
        }
        match linked.len() {
            0 => None,
            1 => Some(linked[0]),
            n => Some(linked[self.rng.next_u32() as usize % n]),
        }
    }

    /// The topic and line the station `station` (a placed reference) says next in its
    /// continuous programme, after `after` (the line it said last, `None` at the start):
    /// the conversation goes on, else a new one starts from RadioHello. `None`: nothing
    /// it can say.
    pub fn radio_next(&mut self, station: FormId, after: Option<FormId>) -> Option<(FormId, FormId)> {
        after.and_then(|a| self.radio_follow(station, a)).or_else(|| self.radio_start(station, None))
    }

    /// The station starts saying `info`: Say Once remembers it and its begin result
    /// script runs, then its Run Immediately end result, with the station as the script's
    /// reference (dialogue.md section 3.6 [design]).
    pub fn radio_line_begins(&mut self, station: FormId, info: FormId) {
        let dv = self.dialogue_view();
        let Some(def) = dv.info(&info) else { return };
        self.itr_radio_line(station, info, true);
        self.mark_said(station, info, false);
        if let Some(u) = def.begin {
            self.run_result(u, RunCtx { self_ref: Some(station), action_ref: None, quest: def.quest });
        }
        if let Some(u) = def.end.filter(|_| def.has(crate::data::info_flag::RUN_IMMEDIATELY)) {
            self.run_result(u, RunCtx { self_ref: Some(station), action_ref: None, quest: def.quest });
        }
    }

    /// The station finished saying `info`: its end result runs unless Run Immediately
    /// already ran it at the start (dialogue.md section 3.6 [design]).
    pub fn radio_line_ends(&mut self, station: FormId, info: FormId) {
        self.itr_radio_line(station, info, false);
        let dv = self.dialogue_view();
        let Some(def) = dv.info(&info) else { return };
        if let Some(u) = def.end.filter(|_| !def.has(crate::data::info_flag::RUN_IMMEDIATELY)) {
            self.run_result(u, RunCtx { self_ref: Some(station), action_ref: None, quest: def.quest });
        }
    }
}

// ---- script functions -------------------------------------------------------------------

/// `GetBroadcastState` on a station: 1 while it broadcasts ([`Game::broadcasting`]).
pub fn get_broadcast_state(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.broadcasting(r))
}

/// `station.StartRadioConversation [topic]`.
pub fn start_radio_conversation(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let station = ctx.subject()?;
    g.radio.conversations.push((station, a.opt_form(0)));
    none()
}

/// The first parameter of PipBoyRadio as a word: `0`, `1` or `tune`.
fn command(a: &Args) -> Option<String> {
    match a.get(0)? {
        ArgValue::Str(s) => Some(s.trim().to_ascii_lowercase()),
        ArgValue::Enum(c) => Some(c.to_string()),
        ArgValue::Value(v) => Some(format!("{}", v.num() as i64)),
        _ => None,
    }
}

/// `PipBoyRadio 0|1|tune [station]`.
pub fn pipboy_radio(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let station = a.opt_form(1);
    let p = &mut g.radio.pipboy;
    p.script_calls += 1;
    match command(a).as_deref() {
        Some("0") => p.on = false,
        Some("1") => {
            p.on = true;
            if station.is_some() {
                p.tune = station;
            }
        }
        // Only while on; otherwise the station is not changed [documented].
        Some("tune") => {
            if p.on {
                p.tune = station;
            }
        }
        other => return Err(format!("PipBoyRadio: unknown command {other:?}")),
    }
    none()
}

/// `PipBoyRadioOff`: as `PipBoyRadio 0` [documented].
pub fn pipboy_radio_off(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.radio.pipboy.script_calls += 1;
    g.radio.pipboy.on = false;
    none()
}

/// `actor.SetNPCRadio [0|1] station`.
pub fn set_npc_radio(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    let (on, station) = if a.len() >= 2 { (a.num(0)? != 0.0, a.opt_form(1)) } else { (true, a.opt_form(0)) };
    if !on {
        g.radio.npc_radios.remove(&actor);
    } else if let Some(s) = station {
        g.radio.npc_radios.entry(actor).or_insert(s);
    }
    none()
}

/// `ForceRadioStationUpdate` (see the module notes).
pub fn force_radio_station_update(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.radio.force_updates += 1;
    none()
}
