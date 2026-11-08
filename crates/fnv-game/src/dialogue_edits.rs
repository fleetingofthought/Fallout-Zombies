//! Scripts' edits to dialogue records, kept for the session (docs/spec-notes/ext-sup2.md
//! §1): SUP's TopicSetFlags, TopicSetPriority and TopicInfo… setters, JohnnyGuitar's
//! DialogResponseAddRelatedTopic, ShowOff's response-text setters and ITR's
//! SetDialogueInfoFlags change a topic or a line as the dialogue reads it. The pages say
//! the changes are "not saved within a savegame and persistent throughout the game
//! session" [documented: TopicSetFlags and the other SUP setters], so they live beside
//! the records, not in the save, and ride across a load (`Game::take_session`).
//!
//! [`GameData`] is shared and read-only, so an edited record is a copy: the dialogue
//! reads lines and topics through a [`DialogueView`], the copy where there is one, else
//! the record. A view borrows nothing from the [`Game`], so the dialogue's code can hold
//! one while it changes the game state.

use crate::data::{topic_type, GameData, InfoDef, TopicDef};
use crate::game::Game;
use fnv_sim::FormId;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

/// The edited copies and what follows from them.
#[derive(Debug, Clone, Default)]
pub struct Edited {
    infos: HashMap<FormId, InfoDef>,
    topics: HashMap<FormId, TopicDef>,
    /// The top-level topic list again once a topic's flags or priority changed
    /// ([`GameData::top_level`]).
    top_level: Option<Vec<FormId>>,
    /// [`GameData::linked_from`] again for the topics a line's added Link From names.
    linked_from: HashMap<FormId, Vec<FormId>>,
}

/// The session's dialogue edits ([`Game::dialogue_edits`]).
#[derive(Debug, Clone, Default)]
pub struct DialogueEdits {
    recs: Arc<Edited>,
    /// Said-once values set without being saved (SetSaidOnce without `saveBake`,
    /// TopicInfoSetSaidOnce), by line; a load drops them, the save's said lines are the
    /// game's own [hypothesis: the value lives on the line in the original and only a
    /// baked change goes into the save].
    pub(crate) said_once: HashMap<FormId, bool>,
}

impl DialogueEdits {
    pub fn is_empty(&self) -> bool {
        self.recs.infos.is_empty() && self.recs.topics.is_empty()
    }

    /// What rides across a load: the records' edits, not the said-once values.
    pub(crate) fn for_session(&mut self) -> DialogueEdits {
        DialogueEdits { recs: std::mem::take(&mut self.recs), said_once: HashMap::new() }
    }
}

/// Lines and topics as the dialogue reads them: a script's edited copy first, else the
/// record ([`Game::dialogue_view`]).
#[derive(Clone)]
pub struct DialogueView<'a> {
    data: &'a GameData,
    recs: Arc<Edited>,
}

impl<'a> DialogueView<'a> {
    pub fn info(&self, id: &FormId) -> Option<&InfoDef> {
        self.recs.infos.get(id).or_else(|| self.data.infos.get(id))
    }

    pub fn topic(&self, id: &FormId) -> Option<&TopicDef> {
        self.recs.topics.get(id).or_else(|| self.data.topic_defs.get(id))
    }

    /// The topics offered at the top level, highest priority first.
    pub fn top_level(&self) -> &[FormId] {
        self.recs.top_level.as_deref().unwrap_or(&self.data.top_level)
    }

    /// The topics whose lines name `topic` in their Link From.
    pub fn linked_from(&self, topic: FormId) -> &[FormId] {
        self.recs.linked_from.get(&topic).or_else(|| self.data.linked_from.get(&topic)).map(Vec::as_slice).unwrap_or_default()
    }

    /// Whether some line names `info` as a follow-up (no script adds one, §2).
    pub fn is_follow_up(&self, info: &FormId) -> bool {
        self.data.follow_up_targets.contains(info)
    }

    /// The text the player sees for a topic whose line would be `info`: the line's
    /// prompt, else the topic's name ([`GameData::choice_text`]).
    pub fn choice_text(&self, topic: FormId, info: FormId) -> Cow<'a, str> {
        if self.recs.infos.is_empty() && self.recs.topics.is_empty() {
            return Cow::Borrowed(self.data.choice_text(topic, info));
        }
        let text = match self.info(&info).and_then(|d| d.prompt.as_deref()) {
            Some(p) => p,
            None => self.topic(&topic).and_then(|t| t.name.as_deref()).unwrap_or(""),
        };
        Cow::Owned(text.to_string())
    }
}

/// The three lists a line links by [documented: DialogResponseAddRelatedTopic,
/// DialogResponseRelatedGetAll: 0 Link From, 1 Choices, 2 Follow Ups].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Related {
    LinkFrom,
    Choices,
    FollowUps,
}

impl Related {
    pub fn from_code(code: f64) -> Option<Related> {
        match code as i64 {
            0 => Some(Related::LinkFrom),
            1 => Some(Related::Choices),
            2 => Some(Related::FollowUps),
            _ => None,
        }
    }

    pub fn list(self, d: &InfoDef) -> &[FormId] {
        match self {
            Related::LinkFrom => &d.link_from,
            Related::Choices => &d.choices,
            Related::FollowUps => &d.follow_ups,
        }
    }
}

/// `position` as a place in a list of `len`: -1 (any negative) the end, 0 the
/// beginning [documented: TopicInfoAddChoice]; past the end, nowhere [observed: sup2
/// choices.add.past, 99 in a list of 13 added nothing; hypothesis: `len` itself is the
/// end].
pub fn insert_at(len: usize, position: f64) -> Option<usize> {
    if position < 0.0 {
        Some(len)
    } else {
        Some(position as usize).filter(|&at| at <= len)
    }
}

impl<'a> Game<'a> {
    pub fn dialogue_view(&self) -> DialogueView<'a> {
        DialogueView { data: self.data, recs: Arc::clone(&self.dialogue_edits.recs) }
    }

    /// Change a line for the session; false when `info` is no line.
    pub(crate) fn edit_info(&mut self, info: FormId, f: impl FnOnce(&mut InfoDef)) -> bool {
        let data = self.data;
        let recs = Arc::make_mut(&mut self.dialogue_edits.recs);
        let Some(def) = recs.infos.get_mut(&info) else {
            let Some(base) = data.infos.get(&info) else { return false };
            let mut def = base.clone();
            f(&mut def);
            recs.infos.insert(info, def);
            return true;
        };
        f(def);
        true
    }

    /// Change a topic for the session; false when `topic` is no topic. A change of its
    /// flags or priority orders the top-level list again.
    pub(crate) fn edit_topic(&mut self, topic: FormId, f: impl FnOnce(&mut TopicDef)) -> bool {
        let data = self.data;
        let recs = Arc::make_mut(&mut self.dialogue_edits.recs);
        let def = match recs.topics.entry(topic) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let Some(base) = data.topic_defs.get(&topic) else { return false };
                e.insert(base.clone())
            }
        };
        f(def);
        // [`GameData::top_level`]'s rule, with the edited topics in place of theirs.
        let topics = &recs.topics;
        let mut top: Vec<&TopicDef> = data
            .topic_defs
            .iter()
            .map(|(id, t)| topics.get(id).unwrap_or(t))
            .filter(|t| t.top_level() && t.topic_type == topic_type::TOPIC)
            .collect();
        top.sort_by(|a, b| b.priority.total_cmp(&a.priority).then(a.order.cmp(&b.order)));
        recs.top_level = Some(top.into_iter().map(|t| t.form).collect());
        true
    }

    /// Put `item` into a line's Link From or Choices at `position` ([`insert_at`]); a
    /// form already there goes in again [observed: sup2 choices.add.again, 11 entries
    /// became 12 with the topic first]. Link From also offers the line's topic after
    /// `item` ([`GameData::linked_from`]). Nothing goes into Follow Ups [observed: sup2
    /// related.add.followinfo and …followtopic, neither a line nor a topic was added].
    pub(crate) fn add_related(&mut self, info: FormId, item: FormId, kind: Related, position: f64) -> bool {
        let Some(len) = self.dialogue_view().info(&info).map(|d| kind.list(d).len()) else { return false };
        let Some(at) = insert_at(len, position).filter(|_| kind != Related::FollowUps) else { return true };
        let ok = self.edit_info(info, |d| match kind {
            Related::LinkFrom => d.link_from.insert(at, item),
            _ => d.choices.insert(at, item),
        });
        if kind != Related::LinkFrom {
            return ok;
        }
        let data = self.data;
        let Some(t) = self.dialogue_view().info(&info).and_then(|d| d.topic) else { return ok };
        let recs = Arc::make_mut(&mut self.dialogue_edits.recs);
        let list = recs.linked_from.entry(item).or_insert_with(|| data.linked_from.get(&item).cloned().unwrap_or_default());
        if !list.contains(&t) {
            list.push(t);
            let order = |t: &FormId| data.topic_defs.get(t).map_or((usize::MAX, 0), |d| d.order);
            list.sort_by_key(|t| (order(t), t.0));
        }
        ok
    }

    /// Replace the choice at `position`; false when there is none there [documented:
    /// TopicInfoReplaceChoice; hypothesis: nothing past the end].
    pub(crate) fn replace_choice(&mut self, info: FormId, topic: FormId, position: f64) -> bool {
        let at = position as i64;
        let fits = self.dialogue_view().info(&info).is_some_and(|d| at >= 0 && (at as usize) < d.choices.len());
        fits && self.edit_info(info, |d| d.choices[at as usize] = topic)
    }

    /// Write a line's DATA flags and the two rules read from them (Say Once, Goodbye).
    pub(crate) fn set_info_flags(&mut self, info: FormId, flags: u16) -> bool {
        self.edit_info(info, |d| {
            d.flags = flags;
            d.say_once = d.has(crate::data::info_flag::SAY_ONCE);
            d.goodbye = d.has(crate::data::info_flag::GOODBYE);
        })
    }

    /// Whether the line has been said, for its Say Once flag: a value a script set first,
    /// else whether any actor said it.
    pub fn said_once(&self, info: FormId) -> bool {
        self.dialogue_edits.said_once.get(&info).copied().unwrap_or_else(|| self.said.iter().any(|&(_, i)| i == info))
    }

    /// Set the line's said-once value: `bake` writes it into the said lines the save
    /// keeps (no actor's, `FormId(0)`), else it holds for the session only [documented:
    /// SetSaidOnce `saveBake`; SUP: not saved].
    pub(crate) fn set_said_once(&mut self, info: FormId, said: bool, bake: bool) {
        if bake {
            self.dialogue_edits.said_once.remove(&info);
            if said {
                self.said.insert((FormId(0), info));
            } else {
                self.said.retain(|&(_, i)| i != info);
            }
        } else {
            self.dialogue_edits.said_once.insert(info, said);
        }
    }
}
