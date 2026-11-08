//! Conversations between the player and an actor (docs/design/dialogue.md step D2), with
//! no engine types: which line an actor says, the greeting, the choices offered, choosing
//! one, the responses said in order with the begin and end result scripts, added topics,
//! Say Once, Say Once a Day, Goodbye, Random stacks, Run Immediately, follow-up lines and
//! the speech-challenge tag. The state ([`Conversation`]) lives in [`Game::dialogue`], so
//! snapshots and traces include it.
//!
//! Rules with their sources: docs/spec-notes/dialogue.md §4–§6. Tags used below:
//! `[documented: page]` is a page of the owner's GECK wiki export (2026-10-09) or an xEdit
//! definition; `[owner statement 2026-10-10]` is the owner's ruling from GECK use, not yet
//! captured; `[hypothesis]` is the simplest rule where the documentation is silent, each
//! one a capture question (dialogue.md §7).
//!
//! A conversation is a menu: while one runs the game is in menu mode
//! ([`Game::in_menu`]) [owner statement 2026-10-10], and only the conversation's own
//! lines go on, in real time ([`Game::update_dialogue`]).
//!
//! Conversations between two actors (`StartConversation` with an actor as the target,
//! [`Game::start_actor_conversation`]) are no menu: their lines are said in the world as
//! SayTo lines are ([`Game::speech`]), each line's Link To topics give the next one
//! (dialogue.md §5.1).

use crate::cond::{Operand, RunOn};
use crate::data::{info_flag, InfoDef};
use crate::game::{Game, LineKind, LineLook, RunCtx, Speech};
use crate::trace::EventKind;
use fnv_sim::{clock, FormId};
use rand_core::RngCore;
use serde::{Deserialize, Serialize};

/// How far from the player a StartConversation speaker may be and still talk at once,
/// game units ([`Game::walk_up_greet`]) [design: twice the AI's standard talk distance].
pub const WALK_UP_REACH: f32 = 300.0;

/// The dialogue menu's code [documented: GECK wiki GetMenuTargetRef, owner export: menu
/// 1009's target is the actor being spoken to; observed: one script of the masters
/// tests `MenuMode 1009`].
pub const DIALOGUE_MENU: u16 = 1009;

/// Hours an actor waits before saying a Say Once a Day line again when the game setting
/// `fSayOncePerDayInfoTimer` is not defined [documented: Category:Dialogue, 12 hours].
pub const SAY_ONCE_A_DAY_HOURS: f64 = 12.0;
pub const SAY_ONCE_A_DAY_SETTING: &str = "fSayOncePerDayInfoTimer";

/// The line an actor is saying in a conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub topic: FormId,
    pub info: FormId,
    /// Position in the line's response order ([`InfoDef::response_order`]) of the
    /// response being said.
    pub response: usize,
    /// Seconds of game time left of that response.
    pub remaining: f64,
}

/// The tag of a choice whose line names an actor value or perk (KNAM): the
/// "[Speech 40]" or "[Speech 37/40]" the player sees [documented: Category:Dialogue,
/// "AV/Perk": the field only controls what is shown; whether the line is available
/// depends on its own conditions].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Challenge {
    /// The actor value (AVIF) or perk (PERK).
    pub form: FormId,
    /// DNAM difficulty code (the Speech Challenge flag's die roll of Fallout 3; three
    /// lines of the masters carry it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<u32>,
    /// The level the line asks for and the player's: the comparison value and the
    /// current value of the line's first GetActorValue-family condition run on the
    /// listener [hypothesis: the game shows the numbers of that condition; observed:
    /// the masters' KNAM lines come in pairs whose conditions are `GetActorValue <skill>
    /// >= N` and `< N` on the target, dialogue.md §3.3].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub have: Option<f64>,
    /// The actor value code that condition tests (43 Speech for a Speech check).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<u16>,
}

/// One entry of the list the player chooses from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub topic: FormId,
    /// The line the actor says when this entry is chosen (picked when the list is built).
    pub info: FormId,
    /// Characters of the text shown ([`Game::choice_text`]).
    pub chars: usize,
    /// The text is the line's prompt (else the topic's name).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prompt: bool,
    /// The line has the Goodbye flag: choosing it ends the conversation at once, and the
    /// actor finishes the line in the world (`Game::choose`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub goodbye: bool,
    /// The entry is shaded as already chosen: the actor said this line before and the
    /// line has no Info Refusal flag, or the line has Always Darken
    /// [documented: Category:Dialogue flags].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub darkened: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge: Option<Challenge>,
    /// The text is the topic's Dumb Response: the player's Intelligence is at or below
    /// `iDialogueDummySpeakThisIntOrBelow` ([`Game::player_dim`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dumb: bool,
}

/// A conversation between the player and an actor: the actor is saying a line
/// (`line`), or the player is choosing (`choices`), or, inside a line's result script,
/// neither.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    /// Who speaks: the actor, or the talking actor of the activator `through`.
    pub actor: FormId,
    /// The talking activator (TACT) the conversation goes through: the actor's lines are
    /// heard from it (`crate::talking_activators`). `None`: face to face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through: Option<FormId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<Line>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
    /// The choices are the topic list (the line had no choices of its own), not a line's
    /// own choices: the player may leave it even when no Goodbye is offered, as at
    /// Julie Farkas's, whose top level has none in the data (#1595; the engine's Tab).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub topic_list: bool,
    /// Lines said so far.
    #[serde(default)]
    pub lines: u32,
    /// A force greet (`StartConversation player`) asked for by the end result script of
    /// the line that just ended, as (speaker, topic): it starts once this conversation
    /// closes ([`Game::hold_force_greet`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then: Option<(FormId, Option<FormId>)>,
    /// A line's end result script is running ([`Game::finish_line`]).
    #[serde(skip)]
    pub ending: bool,
}

/// Which screen of which conversation is up, as one number, for input queued on it: the
/// conversation (`serial`, counted up for each started), the lines said so far, and the
/// response being said, with choices or nothing being said as 0 [design: a stale click must
/// not act on what replaced its screen, dialogue-menu.md §6.5]. 0 without a conversation.
pub fn conversation_stamp(serial: u32, c: Option<&Conversation>) -> u64 {
    let Some(c) = c else { return 0 };
    let response = c.line.as_ref().map_or(0, |l| (l.response as u64 & 0xF_FFFF) + 1);
    (u64::from(serial) << 40) | ((u64::from(c.lines) & 0xF_FFFF) << 20) | response | (1 << 63)
}

/// Whether a line may be said in a conversation between two actors: only Conversation
/// lines (INFO DATA type Conversation, the Conversation tab), never the Topics tab's
/// lines, which are said to the player [owner ruling 2026-10-11: "the dialogue between
/// npcs should ONLY use the conversation options not the Topics", after Sunny Smiles was
/// heard greeting another actor with lines for the player; documented: Conversation Tab,
/// "A conversation is a series of infos that are played between two actors"; observed:
/// HELLO holds 569 Topic lines and 505 Conversation lines in FalloutNV.esm]. Applies to
/// the opening line, follow-ups and Link To lines; hellos to the player are not
/// conversations between actors and take either kind.
pub fn between_actors(d: &InfoDef) -> bool {
    d.info_type == crate::data::topic_type::CONVERSATION
}

/// Why a conversation ended (trace `dialogue_ended`).
pub mod ended {
    /// A line with the Goodbye flag finished.
    pub const GOODBYE: &str = "goodbye";
    /// The actor had nothing to offer after a line.
    pub const NO_CHOICES: &str = "no_choices";
    /// [`super::Game::end_dialogue`] was called (the player left).
    pub const LEFT: &str = "left";
    /// The actor died, was disabled or deleted.
    pub const ACTOR_GONE: &str = "actor_gone";
    /// Conversations between actors: the line just said links to nothing either side can
    /// say.
    pub const NO_LINKS: &str = "no_links";
    /// Conversations between actors: a line was cut off (its speaker started another
    /// line, or was disabled or died).
    pub const CUT: &str = "cut";
}

/// Whether a New Vegas Speech check passes: the player's level against the needed
/// one, when the line's condition tests Speech (`None` for other skills, perks and
/// lines without the numbers).
fn speech_check_passed(c: &Challenge) -> Option<bool> {
    if c.skill != Some(crate::actor_values::SPEECH) {
        return None;
    }
    Some(c.have? >= c.needed?)
}

/// GetActorValue, GetBaseActorValue, GetPermanentActorValue.
const ACTOR_VALUE_OPCODES: [u16; 3] = [0x100E, 0x1115, 0x11EF];

/// Who says a line ([`Game::can_say_as`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Says {
    Actor,
    /// A radio station's placed reference (`crate::radio`).
    Station,
    /// The speaker a script's Say or SayTo names, when no line of the topic passes the
    /// fixed-speaker check: it says a fixed speaker's line in that speaker's stead
    /// ([`Game::say_by_script`]). The other checks are an actor's.
    InStead,
}

/// A topic a line leads to ([`Game::links_after`]): by the line's own choices (TCLT,
/// `from` none), or only by the Link From of the topic's lines naming `from`, the topic
/// the line was said under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Link {
    pub topic: FormId,
    pub from: Option<FormId>,
}

/// The condition functions that let a line be said by a voice type without "Allow
/// Default Dialog": GetIsVoiceType, GetIsID, GetInFaction, GetIsClass, GetIsRace,
/// IsInList [documented: GECK wiki VoiceType] (function index = opcode − 0x1000, the
/// function catalogue) (docs/spec-notes/actor-flags.md §9).
const VOICE_GATE_FUNCTIONS: [u16; 6] = [0x1AB, 0x48, 0x47, 0x44, 0x45, 0x174];

/// GetIsID (function 0x48) on the subject: a line's own condition naming who says it.
fn is_own_id_condition(c: &crate::cond::Condition) -> bool {
    c.function == 0x48 && matches!(c.run_on, crate::cond::RunOn::Subject)
}

impl<'a> Game<'a> {
    // ---- which line ---------------------------------------------------------------

    /// The game clock in hours, for Say Once a Day: GameDaysPassed × 24, as GameDaysPassed
    /// already grows by the fraction of the day [design: simulation.md §5b; hypothesis: the
    /// timer runs on game time]. Saved in `said_day`: `GameSnapshot::said_day_days_clock`.
    fn game_hours(&self) -> f64 {
        self.world.forms.global_by_edid(clock::GAME_DAYS_PASSED).and_then(|(id, _)| self.world.global(id).ok()).unwrap_or(0.0) as f64 * 24.0
    }

    fn say_once_a_day_hours(&self) -> f64 {
        self.world.setting_f32(SAY_ONCE_A_DAY_SETTING).map(|h| h as f64).unwrap_or(SAY_ONCE_A_DAY_HOURS)
    }

    /// Whether `speaker` can say the line `info` to `listener` now:
    /// - a Say Once line no actor has said, a Say Once a Day line the speaker has not said
    ///   in the last `fSayOncePerDayInfoTimer` (12) game hours. The wiki has both per
    ///   actor [documented: Category:Dialogue], but a Say Once line two actors share was
    ///   not said again by the second [observed: observer check 31, save nvf31mainframe:
    ///   the Yes Man mainframe did not repeat 0x001164F1, the Tops Yes Man's first-meeting
    ///   greeting; hypothesis: one case, and the two may count as one speaker] (#1113);
    /// - the speaker is the line's fixed speaker, if it has one (a reference of that
    ///   base form) [documented: Category:Dialogue: "Speaker" acts like GetIsID], unless
    ///   the line has a GetIsID condition on the subject of its own: then that condition
    ///   decides. The wiki has the field act like GetIsID "when dialogue is exported";
    ///   262 lines of FalloutNV.esm name a fixed speaker other than the actor of their
    ///   own GetIsID condition, 167 of them the end slides (fixed speaker a Boomer or
    ///   another character, GetIsID Narrator), which the narrator says
    ///   [hypothesis: the condition is the run-time rule and the field a leftover;
    ///   docs/handoff/mq-walkthrough-2026-10-27.md]. A script's Say or SayTo whose topic
    ///   has no line passing this check, judged from the data alone
    ///   ([`Game::no_line_for_speaker`]), tries the lines without it
    ///   ([`Game::say_by_script`], [`Says::InStead`]);
    /// - the line's quest is running and the quest's own conditions pass [owner
    ///   statement 2026-10-10: every actor has the line as long as the quest is active
    ///   and the conditions are met; documented: Quest Data Tab: quest conditions gate
    ///   all of the quest's dialogue, checked first]. The quest's conditions run like
    ///   the line's, with the speaker as subject and the listener as target
    ///   [hypothesis: the page does not say on whom];
    /// - the line's conditions pass with the speaker as subject and the listener as
    ///   target [documented: Category:Conditions, Run On].
    pub fn can_say(&mut self, speaker: FormId, listener: Option<FormId>, info: FormId) -> bool {
        self.can_say_as(speaker, listener, info, Says::Actor)
    }

    /// The fixed-speaker (ANAM) check alone, from the data: the line names a fixed speaker
    /// that is neither `speaker` nor its base, and has no GetIsID condition of its own
    /// ([`Game::can_say`]). It does not look at run-time state (Say Once, conditions).
    pub(crate) fn fixed_speaker_refuses(&self, def: &InfoDef, speaker: FormId) -> bool {
        def.speaker.is_some_and(|s| !def.conditions.iter().any(is_own_id_condition) && s != speaker && Some(s) != crate::functions::base_of(self, speaker))
    }

    /// Whether every line of `topic` ([`GameData::lines_tried`]) fails the fixed-speaker
    /// check for `speaker` ([`Game::fixed_speaker_refuses`]), decided from the data alone:
    /// the case in which a script's Say or SayTo says a fixed speaker's line in the
    /// speaker's stead ([`Game::say_by_script`]). A topic with no lines is not one.
    pub(crate) fn no_line_for_speaker(&self, speaker: FormId, topic: FormId) -> bool {
        let data = self.data;
        let dv = self.dialogue_view();
        let lines = data.lines_tried(topic);
        !lines.is_empty() && lines.iter().all(|i| dv.info(i).is_some_and(|def| self.fixed_speaker_refuses(def, speaker)))
    }

    /// [`Game::can_say`] for an actor or a radio station. A station is not held to a
    /// line's fixed speaker: radio lines name the actor whose voice they are in
    /// [documented: GECK wiki SetNPCRadio: a line an actor-radio is not the speaker of
    /// "plays with a filter effect to indicate it's not him"; observed: Black Mountain
    /// Radio's segment lines name Tabitha or Raul Tejada, radio.md §5].
    pub(crate) fn can_say_as(&mut self, speaker: FormId, listener: Option<FormId>, info: FormId, says: Says) -> bool {
        let data = self.data;
        let dv = self.dialogue_view();
        let Some(def) = dv.info(&info) else { return false };
        if def.say_once && self.said_once(info) {
            return false;
        }
        // Low / High Intelligence lines: for a dim player, and for one who is not
        // ([`Game::player_dim`]).
        let (low, high) = (def.has(info_flag::LOW_INTELLIGENCE), def.has(info_flag::HIGH_INTELLIGENCE));
        if (low || high) && self.player_dim() != low {
            return false;
        }
        if def.has(info_flag::SAY_ONCE_A_DAY) {
            if let Some(&at) = self.said_day.get(&speaker).and_then(|m| m.get(&info)) {
                if self.game_hours() - at < self.say_once_a_day_hours() {
                    return false;
                }
            }
        }
        if says == Says::Actor && self.fixed_speaker_refuses(def, speaker) {
            return false;
        }
        if let Some(q) = def.quest {
            if !self.quests.get(&q).is_some_and(|s| s.running) {
                return false;
            }
            if let Some(c) = data.quests.get(&q).map(|d| &d.conditions).filter(|c| !c.is_empty()) {
                if !self.conditions_pass(c, Some(speaker), listener, &format!("QUST {q} dialogue")) {
                    return false;
                }
            }
        }
        // A voice type without Allow Default Dialog says only lines conditioned on it; not
        // the player's own lines (its voice types lack the flag, observed).
        if says != Says::Station && speaker != FormId::PLAYER_REF && def.speaker.is_none() && self.voice_without_default(speaker) && !def.conditions.iter().any(|c| VOICE_GATE_FUNCTIONS.contains(&c.function)) {
            return false;
        }
        self.conditions_pass(&def.conditions, Some(speaker), listener, &format!("INFO {info}"))
    }

    /// Whether `speaker`'s voice type (from the definition supplying its traits) leaves
    /// "Allow Default Dialog" off (docs/spec-notes/actor-flags.md §9).
    fn voice_without_default(&self, speaker: FormId) -> bool {
        self.traits_def(speaker).and_then(|d| d.voice).is_some_and(|v| self.data.voices_without_default.contains(&v))
    }

    /// The line of `topic` that `speaker` says to `listener` now, for greetings, chosen
    /// topics, SayTo and Say alike:
    /// 1. The topic's lines, its own and those it shares from other topics
    ///    ([`GameData::lines_tried`]), are taken with the lines of higher-priority quests first
    ///    [documented: Quest Data Tab: priority decides whose dialogue wins, higher
    ///    wins; owner statement 2026-10-10].
    /// 2. Lines of quests of the same priority, and lines of one quest, keep their
    ///    stored order, top to bottom as the editor lists them ([`GameData::topics`]:
    ///    the file order of the plugin that first defines each line), and the first
    ///    line the speaker can say ([`Game::can_say`]) is the one
    ///    [owner statement 2026-10-10, from GECK use; not yet captured; observed for
    ///    SayTo: Doc Mitchell's VCG01Intro lines, layer5-captures.md §10.3].
    /// 3. Unless lines carry the Random flag. Random lines that sit together in that
    ///    order form a pool; a line without the flag between two groups of Random lines
    ///    splits them into two pools, whether or not that line can be said
    ///    [owner statement 2026-10-10; not yet captured]. A Random End line closes its
    ///    pool at that line [documented: Category:Dialogue, flag Random End; hypothesis
    ///    that it does so even when it cannot be said itself]. Going down the list, a
    ///    plain line is taken if the speaker can say it; a pool is taken if the speaker
    ///    can say any of its lines, and then one of those is picked at random
    ///    [documented: Category:Dialogue, flag Random, for the pick].
    ///
    /// The pick uses the game's seeded generator, and only when more than one line of
    /// the pool can be said.
    pub fn select_line(&mut self, speaker: FormId, listener: Option<FormId>, topic: FormId) -> Option<FormId> {
        self.select_line_where(speaker, listener, topic, |_| true)
    }

    /// Whether `speaker` has a GREETING line it can say to the player now (what talking
    /// to it would start with, [`Game::start_dialogue`]), without picking among Random
    /// lines (no random draw).
    pub fn has_greeting(&mut self, speaker: FormId) -> bool {
        let data = self.data;
        let Some(topic) = data.greeting else { return false };
        data.lines_tried(topic).iter().any(|&i| self.can_say(speaker, Some(FormId::PLAYER_REF), i))
    }

    /// [`Game::select_line`] among the lines `keep` accepts: the others count as lines
    /// the speaker cannot say.
    pub(crate) fn select_line_where(&mut self, speaker: FormId, listener: Option<FormId>, topic: FormId, keep: impl Fn(&InfoDef) -> bool) -> Option<FormId> {
        self.select_line_as(speaker, listener, topic, keep, Says::Actor)
    }

    /// [`Game::select_line_where`] for an actor or a radio station ([`Game::can_say_as`]).
    pub(crate) fn select_line_as(&mut self, speaker: FormId, listener: Option<FormId>, topic: FormId, keep: impl Fn(&InfoDef) -> bool, says: Says) -> Option<FormId> {
        let data = self.data;
        let lines = data.lines_tried(topic);
        self.select_among(speaker, listener, &lines, &keep, false, says)
    }

    /// [`Game::select_line_where`] for a topic the player chooses from a list: lines that
    /// another line names as its follow-up (TCFU) are passed over while a line of the
    /// topic that is not one can be said (they count as lines the speaker cannot say, so
    /// Random pools keep their bounds); they are said after the line that leads to them
    /// (§3.4). Only when no other line can be said is a follow-up line taken
    /// [hypothesis: Old World Blues' Think Tank answers the chosen "Topic001" with its
    /// chip line 0x01013559, stored after the five-line chain that Topic000's line leads
    /// into; that chip line is the only way to the Sink; dialogue.md §3.4, observer check
    /// 34]. Greetings and SayTo keep the plain rule: there the stored first line is said
    /// (the Legion's and Yes Man's chains need it).
    pub(crate) fn select_choice_line(&mut self, speaker: FormId, listener: Option<FormId>, topic: FormId, keep: impl Fn(&InfoDef) -> bool) -> Option<FormId> {
        let data = self.data;
        let dv = self.dialogue_view();
        let lines = data.lines_tried(topic);
        if lines.iter().any(|i| dv.is_follow_up(i)) {
            if let Some(i) = self.select_among(speaker, listener, &lines, &keep, true, Says::Actor) {
                return Some(i);
            }
        }
        self.select_among(speaker, listener, &lines, &keep, false, Says::Actor)
    }

    /// Steps 2 and 3 of [`Game::select_line`] over `lines` in their order; with
    /// `lead_only`, follow-up lines ([`GameData::follow_up_targets`]) count as lines the
    /// speaker cannot say.
    fn select_among(&mut self, speaker: FormId, listener: Option<FormId>, lines: &[FormId], keep: &impl Fn(&InfoDef) -> bool, lead_only: bool, says: Says) -> Option<FormId> {
        let dv = self.dialogue_view();
        let flag = |i: &FormId, f: u16| dv.info(i).is_some_and(|d| d.has(f));
        let kept = |i: &FormId| !(lead_only && dv.is_follow_up(i)) && dv.info(i).is_some_and(keep);
        let mut at = 0;
        while at < lines.len() {
            if !flag(&lines[at], info_flag::RANDOM) {
                if kept(&lines[at]) && self.can_say_as(speaker, listener, lines[at], says) {
                    return Some(lines[at]);
                }
                at += 1;
                continue;
            }
            // A pool: the Random lines from here to the first line without the flag, or
            // through the first one with Random End.
            let mut end = at;
            while end < lines.len() && flag(&lines[end], info_flag::RANDOM) {
                end += 1;
                if flag(&lines[end - 1], info_flag::RANDOM_END) {
                    break;
                }
            }
            let pool: Vec<FormId> = lines[at..end].to_vec();
            let sayable: Vec<FormId> = pool.into_iter().filter(|i| kept(i) && self.can_say_as(speaker, listener, *i, says)).collect();
            match sayable.len() {
                0 => {}
                1 => return Some(sayable[0]),
                n => return Some(sayable[self.rng.next_u32() as usize % n]),
            }
            at = end;
        }
        None
    }

    /// The topics a line said under `topic` leads to, in player dialogue, conversations
    /// between actors and on the radio alike (dialogue.md §3.5): its choices (TCLT), in
    /// record order, then the topics whose lines name `topic` in their Link From
    /// ([`GameData::linked_from`]) that are not among its choices [documented:
    /// Category:Dialogue: Link From "can be used to allow many topics to link to this
    /// topic"; Conversation Tab: it chains a conversation's lines; hypothesis: the two
    /// kinds of link make one list].
    pub(crate) fn links_after(&self, topic: FormId, def: &InfoDef) -> Vec<Link> {
        let mut out: Vec<Link> = Vec::new();
        for &t in &def.choices {
            if !out.iter().any(|l| l.topic == t) {
                out.push(Link { topic: t, from: None });
            }
        }
        for &t in self.dialogue_view().linked_from(topic) {
            if !out.iter().any(|l| l.topic == t) {
                out.push(Link { topic: t, from: Some(topic) });
            }
        }
        out
    }

    /// The line `speaker` says to `listener` for a link ([`Game::links_after`]): for a
    /// choice, the topic's line as [`Game::select_line`] picks it; for a topic linked only
    /// by Link From, one of its lines whose Link From names the topic just said
    /// [hypothesis: those are the replies written for it, e.g. the children's Goodbye
    /// replies to HELLO in GenericKids, while their HELLO openers have it blank or `ANY`;
    /// dialogue.md §3.5].
    pub(crate) fn link_line(&mut self, speaker: FormId, listener: Option<FormId>, link: Link, says: Says) -> Option<FormId> {
        self.link_line_where(speaker, listener, link, |_| true, says)
    }

    /// [`Game::link_line`] among the lines `keep` accepts (conversations between actors:
    /// Conversation lines only, [`between_actors`]).
    pub(crate) fn link_line_where(&mut self, speaker: FormId, listener: Option<FormId>, link: Link, keep: impl Fn(&InfoDef) -> bool, says: Says) -> Option<FormId> {
        match link.from {
            None => self.select_line_as(speaker, listener, link.topic, keep, says),
            Some(x) => self.select_line_as(speaker, listener, link.topic, |d| keep(d) && d.link_from.contains(&x), says),
        }
    }

    /// The speaker said the line: Say Once and Say Once a Day remember it per actor
    /// [documented: Category:Dialogue]; [`Game::can_say_as`] holds a Say Once line back
    /// from every actor once any has said it. `always` remembers every line
    /// (conversations: the shading of choices already made).
    pub(crate) fn mark_said(&mut self, speaker: FormId, info: FormId, always: bool) {
        let dv = self.dialogue_view();
        let Some(def) = dv.info(&info) else { return };
        // Said now: a value a script set for the session gives way [hypothesis: saying
        // the line sets the flag in the original].
        self.dialogue_edits.said_once.remove(&info);
        if def.say_once || always {
            self.said.insert((speaker, info));
        }
        if def.has(info_flag::SAY_ONCE_A_DAY) {
            let now = self.game_hours();
            self.said_day.entry(speaker).or_default().insert(info, now);
        }
    }

    // ---- how long a response lasts ---------------------------------------------------

    /// Seconds one response of a line lasts when `speaker` says it. **The one place a
    /// response is timed**: SayTo, Say and conversations all come here. It is the length
    /// of the response's voice file in the speaker's voice when the game was given the
    /// files ([`Game::voice`], `crate::voice`, docs/spec-notes/voice.md §8); without the
    /// file it is the length fitted to the response text
    /// ([`crate::game::fitted_seconds`], scripts.md §7.7).
    pub fn response_seconds(&self, speaker: FormId, info: FormId, response: usize) -> f64 {
        self.response_seconds_voiced(crate::functions::base_of(self, speaker), self.speaker_voice_base(speaker), info, response)
    }

    /// Seconds a whole line lasts: its responses one after another; a line without
    /// responses lasts as long as an empty response.
    pub fn line_seconds(&self, speaker: FormId, info: FormId) -> f64 {
        let n = self.dialogue_view().info(&info).map_or(0, |d| d.responses.len());
        if n == 0 {
            return self.response_seconds(speaker, info, 0);
        }
        (0..n).map(|r| self.response_seconds(speaker, info, r)).sum()
    }

    // ---- conversations -------------------------------------------------------------

    /// The player starts talking to `actor` (activation), or the actor starts talking to
    /// the player (StartConversation, a dialogue package): the actor says the line of
    /// `topic` it can say now ([`Game::select_line`]), GREETING when `topic` is none
    /// [documented: Topics Tab: GREETING is what an actor says when spoken to;
    /// StartConversation: the named topic replaces the greeting]. Returns the line.
    ///
    /// **A greeting with the Goodbye flag starts no conversation**: when the line picked
    /// from GREETING carries the flag, the actor just says it at the player, as a SayTo
    /// line is said ([`Game::say_line`]). [`Game::dialogue`] stays empty, so there is no
    /// menu mode, no pause and nothing to choose or leave: the player is not held and
    /// the game goes on while the line is heard [owner statement 2026-10-10: such a line
    /// "does not draw the player into a dialogue exchange, it just is spoken at the
    /// player without locking them in"; not yet captured]. The begin result script runs
    /// at once and the end result script when the line is over, in game time; its Add
    /// Topics are applied then, the actor counts as having talked to the player
    /// (GetTalkedToPC) and no SayToDone block runs [hypothesis for these three: what a
    /// conversation of that one line did]. Talking to the actor again while it speaks
    /// cuts the line and starts the greeting afresh [hypothesis, as for a second
    /// SayTo]. A Goodbye line of a topic named by StartConversation still opens a
    /// conversation that ends with it [hypothesis: the statement is about GREETING;
    /// capture question, dialogue.md §7]. That rule is for the player talking to the
    /// actor: when the actor starts it ([`Game::force_greet`]) a Goodbye greeting is a
    /// conversation too.
    ///
    /// An actor with no line to say cannot be talked to: nothing starts
    /// [hypothesis]. Neither does a dead, disabled or deleted actor [documented for
    /// StartConversation: nothing happens when either side is disabled]. One
    /// conversation at a time: a second start while one runs is refused [hypothesis].
    ///
    /// While the conversation runs the game is paused behind it, as behind the other
    /// menus: GameMode blocks do not run, game time and GameMode timers stand still,
    /// `MenuMode` and `MenuMode 1009` read 1 and MenuMode blocks run
    /// [owner statement 2026-10-10: a back-and-forth dialogue with the player pauses the
    /// game in the background, unlike lines spoken outside dialogue; not yet captured;
    /// the details follow the menu rules observed for other menus, scripts.md §7.7].
    /// A conversation started from a GameMode block holds the rest of that frame's
    /// GameMode scripts, as any menu does. [`Game::menu`] stays as it is: the
    /// conversation is the menu ([`Game::in_menu`], dialogue.md §6).
    pub fn start_dialogue(&mut self, actor: FormId, topic: Option<FormId>) -> Result<FormId, String> {
        self.open_dialogue(actor, topic, false)
    }

    /// The actor starts the conversation with the player: a force greet, by
    /// StartConversation or by its Dialogue package (not Say To). As
    /// [`Game::start_dialogue`], except that **a greeting with the Goodbye flag opens a
    /// conversation too**: the player is held, as in any conversation, until the line is
    /// over, and the Goodbye flag then ends it [owner statement 2026-10-11, of Sunny
    /// Smiles's "Hear that up on the ridge…" (INFO 0010A1ED, a Goodbye greeting said by
    /// her Dialogue package VCG02SunnySmilesDialogueSneakStart): it "needs to lock the
    /// player into dialogue even though there are no responses from the player";
    /// documented: StartConversation's allowMovement, "Enter 1 to enable movement
    /// during conversation", so by default the player does not move; hypothesis: the
    /// same for a Dialogue package; docs/spec-notes/dialogue.md §5].
    pub fn force_greet(&mut self, actor: FormId, topic: Option<FormId>) -> Result<FormId, String> {
        self.open_dialogue(actor, topic, true)
    }

    fn open_dialogue(&mut self, actor: FormId, topic: Option<FormId>, forced: bool) -> Result<FormId, String> {
        let data = self.data;
        let dv = self.dialogue_view();
        if let Some(c) = &self.dialogue {
            return Err(format!("a conversation with {} is running", data.name(c.actor)));
        }
        // A talking activator speaks for its talking actor, else for itself
        // (`crate::talking_activators`): the activator must be there, the actor alive.
        let (actor, through) = self.talker(actor);
        let place = through.unwrap_or(actor);
        if !self.present(place) || self.extra.get(&actor).is_some_and(|e| e.dead) {
            return Err(format!("{} is not there to talk to", data.name(actor)));
        }
        // Knocked out or set unconscious: no conversation (docs/spec-notes/unconscious.md
        // §3); the player's attempt is told (`EventKind::TalkedToUnconscious`).
        if through.is_none() && self.is_out_cold(actor) {
            // Knocked down for a moment (unconscious.md §5): no conversation, but not
            // told as unconscious [design].
            if self.knockout(actor).is_some_and(|k| k.cause.knockdown()) && !self.extra.get(&actor).is_some_and(|e| e.unconscious) {
                return Err(format!("{} is knocked down", data.name(actor)));
            }
            if !forced {
                self.emit(EventKind::TalkedToUnconscious { actor });
            }
            return Err(format!("{} is unconscious", data.name(actor)));
        }
        let topic = topic.or(data.greeting).ok_or("the load order has no GREETING topic")?;
        let listener = Some(FormId::PLAYER_REF);
        // IsActorTalkingThroughActivator holds while its lines are chosen.
        self.talking_through = through.map(|t| (actor, t));
        let info = self.select_line(actor, listener, topic);
        self.talking_through = None;
        let Some(info) = info else {
            self.emit(EventKind::NoLine { speaker: actor, topic });
            return Err(format!("{} has no line of {} to say", data.name(actor), data.name(topic)));
        };
        // Through a talking activator a Goodbye greeting opens a conversation too, which
        // the line ends (`crate::talking_activators`).
        if !forced && through.is_none() && Some(topic) == data.greeting && dv.info(&info).is_some_and(|d| d.goodbye) {
            self.extra.entry(actor).or_default().talked_to_pc = true;
            self.say_line(actor, listener, topic, info, LineKind::Greeting);
            return Ok(info);
        }
        self.dialogue_serial = self.dialogue_serial.wrapping_add(1);
        self.dialogue = Some(Conversation { actor, through, line: None, choices: Vec::new(), topic_list: false, lines: 0, then: None, ending: false });
        self.extra.entry(actor).or_default().talked_to_pc = true;
        self.emit(EventKind::DialogueStarted { actor, topic });
        self.speak(actor, topic, info);
        Ok(info)
    }

    /// The actor of the running conversation starts saying a line: the begin result
    /// script runs now and the end result script when the last response is over
    /// [documented: Category:Dialogue, Result Script (Begin) / (End)]. With Run
    /// Immediately both run now [documented: the results run when the conversation is
    /// generated instead of when the line is spoken; hypothesis: for a line the player
    /// asked for, that moment is when the line is picked; the masters set the flag only
    /// on Radio and Conversation lines].
    fn speak(&mut self, actor: FormId, topic: FormId, info: FormId) {
        let dv = self.dialogue_view();
        let Some(def) = dv.info(&info) else { return };
        // A SayTo line the actor was saying stops [hypothesis, as for a second SayTo].
        self.cut_line(actor);
        self.mark_said(actor, info, true);
        let order = def.response_order();
        let seconds = self.line_seconds(actor, info);
        let first = order.first().map_or(seconds, |&r| self.response_seconds(actor, info, r));
        self.emit(EventKind::LineStarted { speaker: actor, target: Some(FormId::PLAYER_REF), topic, info, seconds });
        self.cover_info(info);
        if let Some(c) = self.dialogue.as_mut() {
            c.line = Some(Line { topic, info, response: 0, remaining: first });
            c.choices.clear();
            c.lines += 1;
        }
        // The line is up, whether or not a script of it runs: whoever started it reads it next.
        self.dialogue_transition();
        self.emit_response(actor, def, info, 0);
        self.run_line_result(actor, def, false);
        if def.has(info_flag::RUN_IMMEDIATELY) {
            self.run_line_result(actor, def, true);
        }
    }

    fn emit_response(&mut self, actor: FormId, def: &InfoDef, info: FormId, position: usize) {
        let Some(&r) = def.response_order().get(position) else { return };
        let resp = &def.responses[r];
        let seconds = self.response_seconds(actor, info, r);
        self.emit(EventKind::ResponseSaid { speaker: actor, info, number: resp.number, chars: def.response_chars[r], emotion: resp.emotion, seconds });
        // The listener of a conversation's line is the player.
        self.ask_response_idles(actor, Some(FormId::PLAYER_REF), info, r);
    }

    fn run_line_result(&mut self, actor: FormId, def: &InfoDef, end: bool) {
        let data = self.data;
        if let Some(u) = if end { def.end } else { def.begin } {
            // An end script runs as the line goes: what it reads is up by then. A begin script
            // comes right after the line's own transition.
            if end {
                self.dialogue_transition();
            }
            self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(actor) });
            self.run_result(u, RunCtx { self_ref: Some(actor), action_ref: None, quest: def.quest });
        }
    }

    /// One frame of the conversation, in real time: the game is paused behind it but its
    /// own lines go on [owner statement 2026-10-10]; a menu opened on top of it (a
    /// message box from a result script) holds it [hypothesis]. The responses of the
    /// line are said one after another, in
    /// response-number order [documented: xEdit "Response number"]; when the last is
    /// over the line ends ([`Game::finish_line`]). An actor that died, was disabled or
    /// deleted ends the conversation [hypothesis].
    pub(crate) fn update_dialogue(&mut self, dt: f64) {
        let Some(c) = self.dialogue.as_ref() else { return };
        let actor = c.actor;
        if !self.present(c.through.unwrap_or(actor)) || self.extra.get(&actor).is_some_and(|e| e.dead) {
            self.end_dialogue_as(ended::ACTOR_GONE);
            return;
        }
        let dv = self.dialogue_view();
        let Some(mut line) = c.line.clone() else { return };
        let Some(def) = dv.info(&line.info) else { return };
        let order = def.response_order();
        line.remaining -= dt;
        while line.remaining <= 1e-9 {
            line.response += 1;
            match order.get(line.response) {
                Some(&r) => {
                    line.remaining += self.response_seconds(actor, line.info, r);
                    self.emit_response(actor, def, line.info, line.response);
                }
                None => {
                    self.finish_line(actor, line.topic, line.info);
                    return;
                }
            }
        }
        let moved = self.dialogue.as_ref().and_then(|c| c.line.as_ref()).is_some_and(|l| l.response != line.response);
        if let Some(c) = self.dialogue.as_mut() {
            c.line = Some(line);
        }
        if moved {
            self.dialogue_transition();
        }
    }

    /// Tell [`Game::ui_dialogue_hook`] of a conversation that has moved on: the line or
    /// response begun or over, the choices offered. In the middle of the runtime's own work, as
    /// a script that runs next is to read what is up already.
    pub fn dialogue_transition(&mut self) {
        if self.dialogue.is_none() {
            return;
        }
        if let Some(mut hook) = self.ui_dialogue_hook.take() {
            hook(self);
            self.ui_dialogue_hook = Some(hook);
        }
    }

    /// A line of the conversation is over. In order:
    /// 1. its end result script runs (unless Run Immediately ran it at the start);
    /// 2. the topics of its Add Topics list join the topics the player knows
    ///    ([`crate::game::PlayerState::topics`], the set `AddTopic` fills)
    ///    [documented: xEdit INFO "Add Topics"; hypothesis: they are added when the
    ///    line ends, and stay known after the conversation];
    /// 3. a Goodbye line ends the conversation [documented: Category:Dialogue] (one the
    ///    player chose has ended it already, as it started, and ends as a farewell:
    ///    `Game::choose`, `Game::end_line`);
    /// 4. otherwise, if the line has follow-up lines (TCFU), the first one the actor can
    ///    say is said next, with no choice by the player [owner statement 2026-10-10:
    ///    follow-ups have no player response, as in Doc Mitchell's farewell; not yet
    ///    captured; hypothesis: the first sayable of several, dialogue.md §3.4];
    /// 5. otherwise the player is offered choices ([`Game::build_choices`]); with
    ///    nothing to offer the conversation ends [hypothesis].
    ///
    /// A line with both follow-ups and choices (15 in FalloutNV.esm, dialogue.md §3.4)
    /// passes over its follow-ups with the Goodbye flag: they would close the conversation
    /// and leave its choices unreachable [design, issue #964: the original's order is not
    /// known (observer check 65); the follow-ups of the 11 others are Say Once or lead to
    /// choices of their own].
    ///
    /// A `StartConversation player` in the end script of a line that ends the
    /// conversation (Goodbye, or nothing to offer) starts once it has closed
    /// ([`Game::hold_force_greet`]) [design, issue #1107; observer check 39].
    fn finish_line(&mut self, actor: FormId, topic: FormId, info: FormId) {
        let dv = self.dialogue_view();
        let Some(def) = dv.info(&info) else { return };
        if let Some(c) = self.dialogue.as_mut() {
            c.line = None;
            c.ending = true;
        }
        self.emit(EventKind::LineEnded { speaker: actor, info, cut: false });
        if !def.has(info_flag::RUN_IMMEDIATELY) {
            self.run_line_result(actor, def, true);
        }
        for t in self.player.topics.learn(&def.add_topics) {
            self.emit(EventKind::TopicAdded { topic: t, by: Some(info) });
        }
        // The end script may have ended the conversation.
        if !self.dialogue.as_ref().is_some_and(|c| c.actor == actor && c.line.is_none()) {
            return;
        }
        if let Some(c) = self.dialogue.as_mut() {
            c.ending = false;
        }
        if def.goodbye {
            self.end_dialogue_as(ended::GOODBYE);
            return;
        }
        let listener = Some(FormId::PLAYER_REF);
        for &f in &def.follow_ups {
            let Some(fd) = dv.info(&f) else { continue };
            if !(fd.goodbye && !def.choices.is_empty()) && self.can_say(actor, listener, f) {
                self.drop_held_greet();
                self.speak(actor, fd.topic.unwrap_or(FormId(0)), f);
                return;
            }
        }
        let choices = self.build_choices(actor, topic, def);
        if choices.is_empty() {
            self.end_dialogue_as(ended::NO_CHOICES);
            return;
        }
        self.drop_held_greet();
        self.emit(EventKind::ChoicesOffered { actor, choices: choices.clone() });
        if let Some(c) = self.dialogue.as_mut() {
            c.choices = choices;
            c.topic_list = def.choices.is_empty();
        }
        self.dialogue_transition();
    }

    /// StartConversation with the player from `speaker` farther than [`WALK_UP_REACH`]
    /// while the AI runs packages (`GameConfig::package_interval`): the speaker walks up
    /// to the player first and talks once there (`fnv_ai`), "the actor will walk to the
    /// target" [documented: GECK wiki StartConversation; how fast and how near is our
    /// choice]. Until 2026-11-05 the conversation opened at once from any distance: the
    /// Legion explorers about Nelson, whose script calls it once as their cell loads,
    /// greeted the player from 10,000 units off, halfway to Novac (issue #1544). Returns
    /// whether the walk is recorded; otherwise the caller starts the conversation now
    /// (headless runs without the AI, a speaker already near).
    pub fn walk_up_greet(&mut self, speaker: FormId, topic: Option<FormId>) -> bool {
        if self.cfg.package_interval.is_none() || self.dialogue.is_some() {
            return false;
        }
        let far = self.walk_up_far(speaker);
        if far {
            self.walk_up_greets.insert(speaker, topic);
        }
        far
    }

    /// Whether `speaker` is too far from the player to talk at once
    /// ([`Game::walk_up_greet`]): more than [`WALK_UP_REACH`] away, or in another space
    /// (an interior and the world, two interiors). Not when either has no place.
    pub fn walk_up_far(&self, speaker: FormId) -> bool {
        // Its space: the worldspace of an exterior cell, else the interior cell itself.
        // A reference nothing has changed yet is where it was placed.
        let at = |r: FormId| {
            let (cell, pos) = match self.world.peek(r) {
                Some(s) => (s.cell, s.pos),
                None => self.world.placed.get(r).map(|p| (p.cell, p.pos))?,
            };
            Some((self.world.placed.world_of(cell?).unwrap_or(cell?), pos))
        };
        let (Some((space, from)), Some((pspace, to))) = (at(speaker), at(FormId::PLAYER_REF)) else { return false };
        let away = ((from[0] - to[0]).powi(2) + (from[1] - to[1]).powi(2) + (from[2] - to[2]).powi(2)).sqrt();
        space != pspace || away > WALK_UP_REACH
    }

    /// The walk-up greeting of `speaker` ([`Game::walk_up_greet`]), still to be said.
    pub fn walk_up_pending(&self, speaker: FormId) -> Option<Option<FormId>> {
        self.walk_up_greets.get(&speaker).copied()
    }

    /// `speaker` has walked up (or cannot): its conversation starts now, as a force
    /// greet. Returns the line it opens with.
    pub fn walk_up_arrived(&mut self, speaker: FormId) -> Option<FormId> {
        let topic = self.walk_up_greets.remove(&speaker)?;
        self.force_greet(speaker, topic).ok()
    }

    /// `StartConversation player` while the player's conversation runs: inside the end
    /// result script of the line that just ended, the force greet is held and starts
    /// once the conversation closes, if the line closes it (Goodbye, or nothing to offer
    /// after it); returns whether it was held. Anywhere else it is refused, as before
    /// [design, issue #1107: 8 end scripts in FalloutNV.esm call it, 4 of them of Goodbye
    /// lines, e.g. Anders' thanks in VMS46 (INFO 0x00142A9C); the other 4 lines have no
    /// choices, so theirs starts when nothing is offered after them [observed]; whether the original
    /// opens the new conversation is observer check 39 [hypothesis]]. A later call in the
    /// same script replaces an earlier one [design].
    ///
    /// The same inside the end script of a farewell, the Goodbye line the player chose
    /// that ends after its conversation closed ([`Speech::farewell`], `Game::end_line`):
    /// the force greet is held and starts after the line's Add Topics, the order a
    /// Goodbye line that ends in its conversation has [design].
    ///
    /// The farewell is asked first: `farewell_ending` is set and put back by each
    /// `end_line`, so it holds exactly while the innermost end script running is a
    /// farewell's. A farewell that ends inside a conversation line's end script (that
    /// script made the farewell's speaker say a line) keeps its request in its own slot
    /// and does not overwrite the conversation's. Its force greet is tried inside the
    /// outer script, after its Add Topics, and is refused while a conversation exists;
    /// likewise, a nested farewell's force greet that starts inside an outer farewell's
    /// end script leaves the outer one's to be refused when it is tried [design].
    pub fn hold_force_greet(&mut self, speaker: FormId, topic: Option<FormId>) -> bool {
        if self.farewell_ending {
            self.farewell_then = Some((speaker, topic));
            return true;
        }
        if let Some(c) = self.dialogue.as_mut().filter(|c| c.ending) {
            c.then = Some((speaker, topic));
            return true;
        }
        false
    }

    /// The conversation goes on: a held force greet is refused after all [design].
    fn drop_held_greet(&mut self) {
        if let Some(c) = self.dialogue.as_mut() {
            c.then = None;
        }
    }

    /// The list the player chooses from after the line `def`, said under `topic`:
    /// - the line's choices (TCLT), if it has any, and nothing else
    ///   [documented: Category:Dialogue: top-level topics are "the ones that come up when
    ///   an info isn't linked to anything in particular"; Quest and Dialogue Tutorial;
    ///   owner statement 2026-10-11: a line linked by choices offers only the topics given
    ///   as its choices]. When none of them can be said nothing is offered and the
    ///   conversation ends [hypothesis; earlier the topic list stood in, which showed the
    ///   top-level topics after lines whose choices only had shared lines];
    /// - otherwise the top-level topics and the topics the player knows (lines' Add
    ///   Topics lists and `AddTopic`) [documented: the same pages]; a known topic is
    ///   offered by every actor that can say a line of it [owner statement 2026-10-10;
    ///   not yet captured].
    ///
    /// Either list also holds the topics whose lines name `topic` in their Link From
    /// ([`Game::links_after`]) [documented: Category:Dialogue: Link From "can be used to
    /// allow many topics to link to this topic"; hypothesis: they join the list the line
    /// offers, so a line without choices of its own still offers the topic list, and
    /// they do not make it a line with choices; dialogue.md §3.5]. Such a topic is
    /// offered with one of its lines that names `topic` ([`Game::link_line`]); a topic
    /// that is also a choice of the line, or in the topic list, with any of its lines.
    /// Only player lines (type Topic) of topics of type Topic count here; the
    /// Conversation and radio lines' Link From are for conversations between actors and
    /// the radio [hypothesis, as before the three shared the links].
    ///
    /// Each topic is offered only if the actor has a line of it to say now, its own or
    /// one it shares ([`GameData::lines_tried`]), and that line is the one chosen later
    /// [hypothesis; the editor's filtered dialogue window lists only what an actor
    /// qualifies to say, Category:Dialogue].
    ///
    /// Order: topic priority (DIAL PNAM), highest first [documented: Category:Dialogue,
    /// "Priority"], then for a line's choices their order in the record [documented:
    /// "Sorting order of links"] and for the topic list the known topics newest first
    /// ([`crate::game::KnownTopics`]; owner's choice 2026-10-31, the original's order
    /// following no rule we can copy), then the others in record order.
    ///
    /// When the topic list holds no line with the Goodbye flag, the actor's line of the
    /// GOODBYE topic is offered last, if it has one [documented: Quest and Dialogue
    /// Tutorial: the generic GOODBYE of the Conversation tab appears when no listed
    /// topic says goodbye; Conversation Tab: the topic is also what NPCs say when the
    /// player leaves the menu]. A listed Goodbye line with an end script does not count:
    /// it ends the conversation to do something else (start a game of Caravan, hire a
    /// follower), not to say goodbye [hypothesis, owner playtest 2026-10-12: Johnson
    /// Nash's list, whose only Goodbye line is CaravanOffer's, should end with a
    /// Goodbye].
    pub fn build_choices(&mut self, actor: FormId, topic: FormId, def: &InfoDef) -> Vec<Choice> {
        let data = self.data;
        let dv = self.dialogue_view();
        let priority = |t: &FormId| dv.topic(t).map_or(50.0, |d| d.priority);
        let mut links = self.links_after(topic, def);
        if !def.choices.is_empty() {
            links.sort_by(|a, b| priority(&b.topic).total_cmp(&priority(&a.topic)));
            let out = self.offer(actor, &links);
            return self.itr_menu(actor, out);
        }
        let mut listed: Vec<Link> = dv.top_level().iter().map(|&t| Link { topic: t, from: None }).collect();
        let known = self.player.topics.iter().filter(|t| !dv.top_level().contains(t)).map(|&t| Link { topic: t, from: None });
        let mut known: Vec<Link> = known.collect();
        links.retain(|l| !dv.top_level().contains(&l.topic) && !self.player.topics.contains(&l.topic));
        known.append(&mut links);
        if !known.is_empty() {
            listed.append(&mut known);
            // Ties: known topics newest first, then the rest (the top-level topics, even
            // one the player also learned) in record order.
            let key = |t: &FormId| ListKey {
                priority: priority(t),
                learned: (!dv.top_level().contains(t)).then(|| self.player.topics.iter().position(|k| k == t)).flatten(),
                order: dv.topic(t).map_or((usize::MAX, 0), |d| d.order),
            };
            listed.sort_by(|a, b| key(&a.topic).list_cmp(&key(&b.topic)));
        }
        let mut out = self.offer(actor, &listed);
        if !out.iter().any(|c| c.goodbye && dv.info(&c.info).is_some_and(|d| d.end.is_none())) {
            if let Some(bye) = data.goodbye.filter(|b| !out.iter().any(|c| c.topic == *b)) {
                out.extend(self.offer(actor, &[Link { topic: bye, from: None }]));
            }
        }
        // ITR's hide and order rules and custom entries (`crate::nvse_itr2`).
        self.itr_menu(actor, out)
    }

    /// The entries for the topics of `links` the actor has a line for ([`Game::link_line`]),
    /// in that order.
    fn offer(&mut self, actor: FormId, links: &[Link]) -> Vec<Choice> {
        let dv = self.dialogue_view();
        // A dim player's lines read as their topics' Dumb Responses ([`Game::player_dim`]).
        let dim = self.player_dim();
        let listener = Some(FormId::PLAYER_REF);
        let mut out: Vec<Choice> = Vec::new();
        for &link in links {
            let topic = link.topic;
            if out.iter().any(|c| c.topic == topic) {
                continue;
            }
            // Link From reaches player dialogue only from the Topics tab: a player line
            // (type Topic) of a topic of type Topic, as before conversations between actors
            // and the radio shared [`GameData::linked_from`].
            if link.from.is_some() && dv.topic(&topic).is_none_or(|d| d.topic_type != crate::data::topic_type::TOPIC) {
                continue;
            }
            let player_line = |d: &InfoDef| link.from.is_none() || d.info_type == crate::data::topic_type::TOPIC;
            let found = match link.from {
                None => self.select_choice_line(actor, listener, topic, player_line),
                Some(_) => self.link_line_where(actor, listener, link, player_line, Says::Actor),
            };
            let Some(info) = found else { continue };
            // The script-edited line, so a flag or prompt set by SUP's setters shows here too.
            let Some(def) = dv.info(&info) else { continue };
            let challenge = def.challenge_form.map(|form| {
                let (needed, have, skill) = self.challenge_levels(def);
                Challenge { form, difficulty: def.challenge_difficulty, needed, have, skill }
            });
            let said = self.said.contains(&(actor, info));
            let dumb = dim && dv.topic(&topic).is_some_and(|t| t.dumb_response.is_some());
            out.push(Choice {
                topic,
                info,
                chars: if dumb { dv.topic(&topic).expect("topic").dumb_response.as_deref().unwrap_or("").chars().count() } else { dv.choice_text(topic, info).chars().count() },
                dumb,
                prompt: def.prompt.is_some(),
                goodbye: def.goodbye,
                darkened: def.has(info_flag::ALWAYS_DARKEN) || (said && !def.has(info_flag::INFO_REFUSAL)),
                challenge,
            });
        }
        out
    }

    /// The needed and current level of a KNAM line and the actor value its condition
    /// tests ([`Challenge::needed`], [`Challenge::skill`]).
    fn challenge_levels(&mut self, def: &InfoDef) -> (Option<f64>, Option<f64>, Option<u16>) {
        let Some(c) = def.conditions.iter().find(|c| c.run_on == RunOn::Target && ACTOR_VALUE_OPCODES.contains(&c.opcode())) else {
            return (None, None, None);
        };
        let skill = match c.params.first() {
            Some(fnv_script::vm::ArgValue::Enum(av)) => Some(*av),
            _ => None,
        };
        let needed = match c.rhs {
            Operand::Value(x) => Some(x),
            Operand::Global(g) => g.and_then(|g| self.world.global(FormId(g)).ok()).map(|v| v as f64),
        };
        let have = self.condition_value(c, Some(FormId::PLAYER_REF), "dialogue challenge");
        (needed, Some(have), skill)
    }

    /// The choices offered now (empty while the actor speaks and outside conversations).
    pub fn dialogue_choices(&self) -> &[Choice] {
        self.dialogue.as_ref().map_or(&[], |c| c.choices.as_slice())
    }

    /// The text of a choice: the topic's Dumb Response for a dim player, else the line's
    /// prompt, else the topic's name [documented: Category:Dialogue, "Prompt"; for the
    /// Dumb Response see [`Game::player_dim`]].
    /// A script's edits show (`crate::dialogue_edits`): TopicInfoSetPrompt's prompt.
    pub fn choice_text(&self, c: &Choice) -> std::borrow::Cow<'a, str> {
        let data = self.data;
        match data.topic_defs.get(&c.topic).and_then(|t| t.dumb_response.as_deref()).filter(|_| c.dumb) {
            Some(d) => std::borrow::Cow::Borrowed(d),
            None => self.dialogue_view().choice_text(c.topic, c.info),
        }
    }

    /// [`Game::choice_text`] as the menu shows it, its player string codes expanded
    /// ([`Game::expand_codes`]; FalloutNV.esm's topics read "The name's &PCName;."
    /// [observed]).
    pub fn shown_choice_text(&self, c: &Choice) -> String {
        self.expand_codes(&self.choice_text(c))
    }

    /// Whether the player is dim: Intelligence at or below
    /// `iDialogueDummySpeakThisIntOrBelow` (3 in FalloutNV.esm [observed]). A dim player's
    /// lines show their topic's Dumb Response (DIAL TDUM: "Peace." is "Pizza.", "Where
    /// should I route the power?" is "Where me send shiny juice?" [observed: the 135
    /// topics that have one]), and lines flagged Low Intelligence are said to it only, those
    /// flagged High Intelligence only to a player who is not [observed: the masters' Low
    /// Intelligence lines carry the acting note "{Dummy}", "realizing the player is not very
    /// smart", their High Intelligence twins "{Non-Dummy}"]. That the setting is the bound
    /// for all three is a [hypothesis] from its name (dialogue.md §2).
    /// A player whose Intelligence cannot be read (no player record) is not dim.
    pub fn player_dim(&mut self) -> bool {
        let bound = crate::combat::setting(self, "iDialogueDummySpeakThisIntOrBelow", 3.0);
        let int = crate::functions::actor::actor_value(self, FormId::PLAYER_REF, crate::actor_values::INTELLIGENCE);
        int.is_ok_and(|v| crate::actor_values::capped(crate::actor_values::INTELLIGENCE, v) <= bound)
    }

    /// The player picks entry `index` of the offered list: the actor says its line. A
    /// Goodbye line closes the conversation at once and goes on in the world
    /// ([`Game::say_farewell`]).
    pub fn choose(&mut self, index: usize) -> Result<FormId, String> {
        let c = self.dialogue.as_ref().ok_or("no conversation is running")?;
        if c.line.is_some() {
            return Err("the actor is still speaking".into());
        }
        let (actor, lines) = (c.actor, c.lines);
        let choice = c.choices.get(index).cloned().ok_or_else(|| format!("no choice {index} ({} offered)", c.choices.len()))?;
        self.emit(EventKind::ChoiceMade { actor, index, topic: choice.topic, info: choice.info });
        // A Fallout 3 style speech challenge (the flag and a difficulty) gives its XP when
        // its line is said. The roll is not modelled, so the line always passes
        // [hypothesis, dialogue.md §3.3]. New Vegas's skill checks (KNAM without the
        // flag) give none here: their pass lines' result scripts carry RewardXP (638 of
        // the 1,499 in the masters [observed]), which would count twice.
        if let Some(d) = self.dialogue_view().info(&choice.info).filter(|d| d.has(info_flag::SPEECH_CHALLENGE)).and_then(|d| d.challenge_difficulty) {
            self.count_stat(crate::misc_stats::SPEECH_SUCCESSES, 1.0);
            let x = crate::experience::speech_challenge_xp(self, d);
            crate::experience::award(self, crate::experience::Source::SpeechChallenge, x);
        } else if let Some(passed) = choice.challenge.as_ref().and_then(speech_check_passed) {
            // A New Vegas Speech check: the line offered is the pass line when the player
            // meets the level, the fail line otherwise (dialogue.md §3.3), so choosing it
            // counts a Speech Success or a Speech Failure [hypothesis: by the names of
            // stats 12 and 31; whether the original counts these checks, and checks of
            // other skills and perks, is a capture question, observer-checks.md]. No XP:
            // the pass lines' result scripts carry RewardXP.
            self.count_stat(if passed { crate::misc_stats::SPEECH_SUCCESSES } else { crate::misc_stats::SPEECH_FAILURES }, 1.0);
        }
        self.speak(actor, choice.topic, choice.info);
        self.say_farewell(actor, choice.topic, choice.info, lines + 1);
        Ok(choice.info)
    }

    /// A Goodbye line the player chose closes the conversation at once: the menu goes and
    /// the actor finishes the line in the world, said at the player as a farewell
    /// ([`Speech::farewell`]) [observed: owner's playtest 2026-11-05, against the
    /// original: choosing a Goodbye closes the menu at once and the actor finishes the
    /// line as the player walks away]. The line keeps the time it has left: the rest of
    /// the response being said and the responses after it, so it lasts as long as it
    /// would have in the conversation. Its begin script, `line_started`, its first
    /// `response_said` and Say Once already came with [`Game::speak`] and do not come
    /// again; with Run Immediately its end script ran there too and does not run again.
    /// When the line ends its end script runs, its Add Topics apply, and a
    /// `StartConversation player` from that script starts after them
    /// ([`Game::hold_force_greet`]), as for a Goodbye line that ends in its conversation.
    /// The conversation's line is taken before it closes, so no cut `line_ended` comes;
    /// `dialogue_ended` (goodbye) counts the line.
    ///
    /// Left as it is: a line the begin script ended or replaced. The conversation must be
    /// the one the line was chosen in: the same actor, saying this line, and `lines` lines
    /// said (the count before the choice and this line). A conversation the begin script
    /// started afresh with the same actor and line is at 1 line, this one at 2 or more,
    /// since the line the choices followed counts [design].
    ///
    /// Left in the conversation until it is over, as before:
    /// - a conversation through a talking activator, whose line stays heard from the
    ///   activator [hypothesis: the owner's playtest was face to face; a line said in the
    ///   world is heard from its speaker, and the activator is not carried by
    ///   [`Speech`]; open, docs/spec-notes/dialogue.md §5];
    /// - a line whose begin script (or Run Immediately end script) made the actor say
    ///   another line ([`Game::speech`] has one of the actor's after [`Game::speak`],
    ///   which cut any earlier one): that line is kept and not cut for the farewell
    ///   [design: a begin script that makes the actor say another line keeps today's
    ///   behaviour].
    ///
    /// A Goodbye line that opens a
    /// conversation (a force greet's greeting, a StartConversation topic) and a Goodbye
    /// follow-up are not chosen: they end their conversation when they are over, as
    /// before (`Game::finish_line`) [owner statement 2026-10-11 for the force greet].
    fn say_farewell(&mut self, actor: FormId, topic: FormId, info: FormId, lines: u32) {
        let dv = self.dialogue_view();
        let Some(def) = dv.info(&info).filter(|d| d.goodbye) else { return };
        if self.speech.iter().any(|s| s.speaker == actor) {
            return;
        }
        let Some(c) = self.dialogue.as_mut() else { return };
        if c.actor != actor || c.lines != lines || c.through.is_some() || !c.line.as_ref().is_some_and(|l| l.info == info) {
            return;
        }
        let Some(line) = c.line.take() else { return };
        let lengths = self.response_lengths(actor, info);
        let later: f64 = def.response_order().iter().skip(line.response + 1).map(|&r| lengths.get(r).copied().unwrap_or(0.0)).sum();
        self.speech.push(Speech {
            speaker: actor,
            target: Some(FormId::PLAYER_REF),
            topic,
            info,
            remaining: line.remaining + later,
            say_to: false,
            greeting: false,
            farewell: true,
            conversation: None,
            earlier_topics: Vec::new(),
            hello: false,
            // Who turns to whom as for a Goodbye greeting at the player
            // ([`Game::facings`]) [design].
            look: LineLook::Both,
            response: line.response,
            force_subtitles: false,
            two_d: false,
            end_ran: def.has(info_flag::RUN_IMMEDIATELY),
        });
        self.farewell_said = Some((actor, info));
        self.end_dialogue_as(ended::GOODBYE);
    }

    /// The player skips the response the actor is saying (a click or a key on the
    /// dialogue screen): that response ends now, as if its time had run out. The next
    /// response of the line starts at once; after the last one the line ends as a line
    /// said to the end does ([`Game::finish_line`]): end result script, Add Topics,
    /// Goodbye, follow-ups, choices [hypothesis: skipping only shortens a line. The
    /// other reading, a skipped line counted as cut and running no end script, as a line
    /// cut by leaving does ([`Game::end_dialogue`]), would stop every quest whose stages
    /// are set by end scripts whenever the player clicks through; capture question,
    /// dialogue.md §7 item 8]. No trace event of its own: the next `response_said` or
    /// the `line_ended` (not cut) simply comes early.
    ///
    /// Returns whether a response was skipped: not outside a conversation, not while the
    /// player chooses, and not while a loading screen or a menu opened on top of the
    /// conversation (a message box a result script showed, [`Game::menu`]) holds its
    /// lines ([`Game::tick`]).
    pub fn skip_response(&mut self) -> bool {
        if self.loading || self.menu.is_some() {
            return false;
        }
        let Some(line) = self.dialogue.as_mut().and_then(|c| c.line.as_mut()) else { return false };
        line.remaining = 0.0;
        self.update_dialogue(0.0);
        true
    }

    /// The player leaves the conversation. A line being said is cut off without its end
    /// result script [hypothesis, as for a SayTo line that is cut]. Whether the game
    /// lets the player leave at any point is not modelled: the engine decides when to
    /// call this.
    pub fn end_dialogue(&mut self) -> bool {
        self.end_dialogue_as(ended::LEFT)
    }

    fn end_dialogue_as(&mut self, reason: &'static str) -> bool {
        let Some(c) = self.dialogue.take() else { return false };
        if let Some(l) = c.line {
            self.emit(EventKind::LineEnded { speaker: c.actor, info: l.info, cut: true });
        }
        self.emit(EventKind::DialogueEnded { actor: c.actor, reason, lines: c.lines });
        // A force greet held by the last line's end script ([`Game::hold_force_greet`]),
        // only when that line closed the conversation: not when the end script itself ended
        // it some other way (a ForceFlee after the StartConversation) [design]. Refusals (the
        // speaker gone, no line) are not errors, as for StartConversation.
        if let Some((speaker, topic)) = c.then.filter(|_| reason == ended::GOODBYE || reason == ended::NO_CHOICES) {
            let _ = self.force_greet(speaker, topic);
        }
        true
    }

    // ---- conversations between actors (dialogue.md §5.1) --------------------------------

    /// `speaker.StartConversation target [topic]` with an actor as the target: the two
    /// talk to each other and the player overhears them [documented: GECK wiki
    /// StartConversation: it "forces a conversation between two NPCs, optionally starting
    /// with a specified topic"; Conversation Tab: a conversation is "a series of infos
    /// that are played between two actors"]. The speaker says the line of `topic` it can
    /// say to the target ([`Game::select_line`], the target as the conditions' target),
    /// HELLO when `topic` is none [hypothesis, from the Conversation Tab: random
    /// conversations start with HELLO]. Returns the line.
    ///
    /// The lines are said in the world as SayTo lines are ([`Game::say_line`],
    /// [`LineKind::Conversation`]): no menu, nothing paused, the player stays free
    /// [hypothesis: the player is not part of it], and only the first response of each
    /// line is said [documented: Conversation Tab: "Infos in conversations cannot have
    /// more than one response. Extra responses are ignored"]. When a line ends the next
    /// one follows ([`Game::next_conversation_line`]).
    ///
    /// In the game the speaker first goes to the target ("no matter where the two NPCs
    /// are"); that walk needs AI packages, so here the conversation starts at once
    /// [stand-in, as for `StartConversation player`; Joe Cobb starts next to Trudy in the
    /// owner's footage, owner statement 2026-10-11]. Nothing starts when either side is
    /// disabled [documented: StartConversation], dead or deleted [hypothesis], when either
    /// is already in a conversation, with the player or another actor [hypothesis], or
    /// when the speaker has no line to say [hypothesis].
    pub fn start_actor_conversation(&mut self, speaker: FormId, target: FormId, topic: Option<FormId>) -> Result<FormId, String> {
        let data = self.data;
        if speaker == target {
            return Err(format!("{} cannot talk to itself", data.name(speaker)));
        }
        for who in [speaker, target] {
            if !self.present(who) || self.extra.get(&who).is_some_and(|e| e.dead) {
                return Err(format!("{} is not there to talk", data.name(who)));
            }
            if self.dialogue_actor() == Some(who) || self.in_actor_conversation(who) {
                return Err(format!("{} is already in a conversation", data.name(who)));
            }
        }
        let hello = self.hello_topic();
        let topic = topic.or(hello).ok_or("the load order has no HELLO topic")?;
        // A conversation starts only with a HELLO line whose Link From is blank
        // [documented: Conversation Tab], and only with a Conversation line
        // ([`between_actors`]).
        let starts = |d: &InfoDef| between_actors(d) && (Some(topic) != hello || d.link_from.is_empty());
        let Some(info) = self.select_line_where(speaker, Some(target), topic, starts) else {
            self.emit(EventKind::NoLine { speaker, topic });
            return Err(format!("{} has no line of {} to say to {}", data.name(speaker), data.name(topic), data.name(target)));
        };
        self.emit(EventKind::ActorConversationStarted { speaker, target, topic });
        self.say_line(speaker, Some(target), topic, info, LineKind::Conversation(1));
        Ok(info)
    }

    /// The HELLO line `speaker` would open a conversation with `target` with now (Link
    /// From blank), without saying it: whether there is anything to go up to `target`
    /// for (`fnv_ai::social`). The line said when they meet is chosen again then.
    pub fn conversation_opener(&mut self, speaker: FormId, target: FormId) -> Option<FormId> {
        let topic = self.hello_topic()?;
        self.select_line_where(speaker, Some(target), topic, |d| between_actors(d) && d.link_from.is_empty())
    }

    /// The HELLO topic, when the load order has it.
    pub fn hello_topic(&self) -> Option<FormId> {
        let data = self.data;
        data.by_edid("HELLO").filter(|t| data.topic_defs.contains_key(t))
    }

    /// `actor` says hello to the player as the player passes (docs/spec-notes/dialogue.md
    /// §5.2): its line of HELLO for the player as the conditions' target, Link From blank
    /// as for the start of a conversation [documented: Conversation Tab, HELLO; the
    /// Topics-tab GREETING is what an actor says when spoken to, Topics Tab]. The line is
    /// said in the world ([`LineKind::Hello`]): no conversation, no menu. A line that opens
    /// an exchange between actors (Link To) is said to the player all the same, and its
    /// links are not followed: nobody replies and the actor goes back to what it was doing
    /// [owner statement 2026-10-11, of the Fiends' lines in Vault 3]. Returns the line;
    /// none when the actor has no line for the player now. Whether to say hello at all
    /// (the package's "Hellos to player", the distance) is the AI's
    /// (`fnv_ai::social`).
    pub fn say_hello(&mut self, actor: FormId) -> Option<FormId> {
        let topic = self.hello_topic()?;
        let listener = Some(FormId::PLAYER_REF);
        let info = self.select_line_where(actor, listener, topic, |d| d.link_from.is_empty())?;
        self.say_line(actor, listener, topic, info, LineKind::Hello);
        Some(info)
    }

    /// Whether an actor is saying, or listening to, a line of a conversation between
    /// actors.
    pub fn in_actor_conversation(&self, actor: FormId) -> bool {
        self.speech.iter().any(|s| s.conversation.is_some() && (s.speaker == actor || s.target == Some(actor)))
    }

    /// Whether `actor` talks to another actor in a conversation a script started with
    /// allowMovement 1 (StartConversation): it goes on with its package while it talks.
    /// The engine keeps the others in a conversation where they stand (`nvforever`'s
    /// residents).
    pub fn conversation_moves(&self, actor: FormId) -> bool {
        self.moving_talkers.contains(&actor) && self.in_actor_conversation(actor)
    }

    /// Line `lines` of a conversation between actors (`said`) is over and its end result
    /// script has run: the next line, or the end of the conversation. In order:
    /// 1. a line with the Goodbye flag ends it [documented: Conversation Tab: a
    ///    conversation ends with an info with the Goodbye flag];
    /// 2. so does either side being gone (dead, disabled, deleted) [hypothesis];
    /// 3. the line's next speaker (INFO DATA: Target, the listener; Self, the speaker
    ///    again; Either, the listener and else the speaker) [documented: xEdit
    ///    wbNextSpeaker, the names; hypothesis: what they mean, and the order for Either]
    ///    says the first of the line's follow-ups it can say to the other
    ///    [hypothesis, as in conversations with the player];
    /// 4. else one of the topics the line leads to ([`Game::links_after`]: its Link To
    ///    topics, the choices (TCLT), and the topics whose lines name the line's topic in
    ///    their Link From, then said with such a line, [`Game::link_line`]) of which it
    ///    has a line to say, at random when there are several [documented: Conversation
    ///    Tab: with more than one Link To topic "the game [selects] one of the Link To
    ///    topics at random", and Link From chains a conversation; hypothesis: topics with
    ///    no line it can say are passed over, and both kinds of link are one list,
    ///    dialogue.md §3.5];
    /// 5. else the conversation ends. A GOODBYE line is not added [owner statement
    ///    2026-10-11: no goodbye follows Trudy's answer to Joe Cobb, a line that links
    ///    nowhere; the Conversation Tab's GOODBYE rule is about random conversations,
    ///    dialogue.md §5.1].
    pub(crate) fn next_conversation_line(&mut self, said: &Speech, lines: u32) {
        let data = self.data;
        let dv = self.dialogue_view();
        let (speaker, Some(listener)) = (said.speaker, said.target) else { return };
        let end = |g: &mut Self, reason: &'static str| g.emit(EventKind::ActorConversationEnded { speaker, target: listener, reason, lines });
        let Some(def) = dv.info(&said.info) else { return end(self, ended::NO_LINKS) };
        if def.goodbye {
            return end(self, ended::GOODBYE);
        }
        if [speaker, listener].iter().any(|&a| !self.present(a) || self.extra.get(&a).is_some_and(|e| e.dead)) {
            return end(self, ended::ACTOR_GONE);
        }
        let turns: &[(FormId, FormId)] = match def.next_speaker {
            1 => &[(speaker, listener)],
            2 => &[(listener, speaker), (speaker, listener)],
            _ => &[(listener, speaker)],
        };
        let next = LineKind::Conversation(lines + 1);
        // "If this box is unchecked, conversations will never use the same topic twice
        // (in the same conversation)" [documented: GECK wiki Quest Data Tab, Allow
        // Repeated Conversation Topics]: a topic used already is passed over unless the
        // line's quest allows it. A follow-up is named outright, so it is not passed over,
        // but it carries the history on.
        let earlier: Vec<FormId> = said.earlier_topics.iter().copied().chain(std::iter::once(said.topic)).collect();
        for &(who, to) in turns {
            if let Some(&f) = def.follow_ups.iter().find(|&&f| dv.info(&f).is_some_and(between_actors) && self.can_say(who, Some(to), f)) {
                let topic = dv.info(&f).and_then(|d| d.topic).unwrap_or(FormId(0));
                self.say_line(who, Some(to), topic, f, next);
                self.set_line_look(who, said.look);
                if let Some(s) = self.speech.iter_mut().find(|s| s.speaker == who && s.info == f) {
                    s.earlier_topics = earlier;
                }
                return;
            }
            let mut linked: Vec<(FormId, FormId)> = Vec::new();
            for link in self.links_after(said.topic, def) {
                if let Some(i) = self.link_line_where(who, Some(to), link, between_actors, Says::Actor) {
                    let repeats_ok = dv.info(&i).and_then(|d| d.quest).and_then(|q| data.quests.get(&q)).is_some_and(|q| q.repeated_topics());
                    if earlier.contains(&link.topic) && !repeats_ok {
                        continue;
                    }
                    linked.push((link.topic, i));
                }
            }
            let (topic, info) = match linked.len() {
                0 => continue,
                1 => linked[0],
                n => linked[self.rng.next_u32() as usize % n],
            };
            self.say_line(who, Some(to), topic, info, next);
            self.set_line_look(who, said.look);
            if let Some(s) = self.speech.iter_mut().find(|s| s.speaker == who && s.info == info) {
                s.earlier_topics = earlier;
            }
            return;
        }
        end(self, ended::NO_LINKS)
    }

    /// The actor the player is in a conversation with.
    pub fn dialogue_actor(&self) -> Option<FormId> {
        self.dialogue.as_ref().map(|c| c.actor)
    }

    /// The emotion type code of the response an actor is saying (GetDialogueEmotion):
    /// in a conversation, or the response of a SayTo line being said; 0 (Neutral) otherwise
    /// [documented: GECK wiki GetDialogueEmotion, the codes; hypothesis: 0 when silent].
    pub fn dialogue_emotion(&self, actor: FormId) -> u8 {
        let dv = self.dialogue_view();
        if let Some(l) = self.dialogue.as_ref().filter(|c| c.actor == actor).and_then(|c| c.line.as_ref()) {
            if let Some(def) = dv.info(&l.info) {
                return def.response_order().get(l.response).map_or(0, |&r| def.responses[r].emotion);
            }
        }
        self.speech
            .iter()
            .find(|s| s.speaker == actor)
            .and_then(|s| dv.info(&s.info).map(|def| (def, s.response)))
            .and_then(|(def, at)| {
                let order = def.response_order();
                order.get(at).or(order.first()).map(|&r| def.responses[r].emotion)
            })
            .unwrap_or(0)
    }
}

/// Where a topic goes in the topic list ([`Game::build_choices`]): topic priority,
/// highest first; then a known topic by when it was learned, newest first (`learned`,
/// its place in [`crate::game::KnownTopics`]); then the others in record order.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ListKey {
    priority: f32,
    learned: Option<usize>,
    order: (usize, usize),
}

impl ListKey {
    fn list_cmp(&self, b: &ListKey) -> std::cmp::Ordering {
        let learned = |k: &ListKey| k.learned.unwrap_or(usize::MAX);
        b.priority.total_cmp(&self.priority).then(learned(self).cmp(&learned(b))).then(self.order.cmp(&b.order))
    }
}

#[cfg(test)]
mod list_order_tests {
    #[test]
    fn the_conversation_stamp_tells_screens_apart() {
        use super::{conversation_stamp, Conversation, Line};
        use fnv_sim::FormId;
        let line = |response| Line { topic: FormId(1), info: FormId(2), response, remaining: 1.0 };
        let talk = |l: Option<Line>, lines| Conversation { actor: FormId(3), through: None, line: l, choices: Vec::new(), topic_list: false, lines, then: None, ending: false };
        assert_eq!(conversation_stamp(7, None), 0);
        let first = conversation_stamp(7, Some(&talk(Some(line(0)), 1)));
        assert_ne!(first, 0);
        assert_eq!(first, conversation_stamp(7, Some(&talk(Some(line(0)), 1))));
        // The next response of the line; the choices after it; the next line; the same actor's
        // next conversation at the same place.
        assert_ne!(first, conversation_stamp(7, Some(&talk(Some(line(1)), 1))));
        assert_ne!(first, conversation_stamp(7, Some(&talk(None, 1))));
        assert_ne!(first, conversation_stamp(7, Some(&talk(Some(line(0)), 2))));
        assert_ne!(first, conversation_stamp(8, Some(&talk(Some(line(0)), 1))));
    }

    use super::ListKey;

    #[test]
    fn priority_then_newest_learned_then_record_order() {
        let k = |priority: f32, learned: Option<usize>, order: usize| ListKey { priority, learned, order: (0, order) };
        let mut list = [
            ("top-level, later record", k(50.0, None, 9)),
            ("known, older", k(50.0, Some(3), 1)),
            ("top-level, earlier record", k(50.0, None, 2)),
            ("known, newest", k(50.0, Some(0), 8)),
            ("higher priority, top-level", k(60.0, None, 99)),
        ];
        list.sort_by(|a, b| a.1.list_cmp(&b.1));
        let names: Vec<&str> = list.iter().map(|e| e.0).collect();
        assert_eq!(names, ["higher priority, top-level", "known, newest", "known, older", "top-level, earlier record", "top-level, later record"]);
    }
}
