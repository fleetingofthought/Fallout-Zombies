//! The extenders' dialogue setters (#1008 SUP batch 2, with the JohnnyGuitar and ShowOff
//! functions that do the same; docs/spec-notes/ext-sup2.md): topic flags and priority,
//! a line's flags, prompt, choices, Link From and follow-ups, its response texts and
//! whether it has been said. The edits are the session's (`crate::dialogue_edits`) and
//! the dialogue reads them. What the `sup2` probe saw in the game is tagged [observed]
//! below; the rest follows the plugins' documentation pages.

use crate::dialogue_edits::Related;
use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize) -> Option<f64> {
    match args.get(k) {
        Some(XVal::Num(x)) => Some(*x),
        _ => None,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

fn list(g: &mut Game, items: Vec<XVal>) -> XVal {
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

/// `id` when it names a dialogue line (INFO).
fn line(g: &Game, id: Option<FormId>) -> Option<FormId> {
    id.filter(|i| g.data.infos.contains_key(i))
}

/// `id` when it names a topic (DIAL).
fn topic(g: &Game, id: Option<FormId>) -> Option<FormId> {
    id.filter(|t| g.data.topic_defs.contains_key(t))
}

/// A byte of a flag argument: the low 8 bits of the number [hypothesis: the pages'
/// tables list one byte each].
fn byte(x: f64) -> u16 {
    (x as i64 & 0xFF) as u16
}

pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let _ = (ctx, subject);
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- topics (SUP) ----
        // `topic [flags]`: DIAL DATA's flags, 1 Rumors and 2 Top Level [documented:
        // TopicSetFlags, "Sets or returns"]: with `flags` they are set (the low byte) and
        // the call returns 0, without it returns them [observed: sup2 topic.flags.set,
        // 3 set returned 0 and read back 3; …wide, 511 read back 255]. A Top Level change
        // takes the topic into or out of the topic list.
        "topicsetflags" => {
            let Some(t) = topic(g, form(args, 0)) else { return Some(Ok(zero)) };
            if let Some(f) = num(args, 1) {
                g.edit_topic(t, |d| d.flags = byte(f) as u8);
                return Some(Ok(zero));
            }
            XVal::Num(g.dialogue_view().topic(&t).map_or(0.0, |d| f64::from(d.flags)))
        }
        // `topic [priority]`: DIAL PNAM, the order of the topic list and of a line's
        // choices [documented: TopicSetPriority; Category:Dialogue "Priority"]; 0 when
        // setting, as TopicSetFlags [hypothesis: the game's compiler refused the probe's
        // 75.5, so the setter's return is not observed].
        "topicsetpriority" => {
            let Some(t) = topic(g, form(args, 0)) else { return Some(Ok(zero)) };
            if let Some(p) = num(args, 1).filter(|p| p.is_finite()) {
                g.edit_topic(t, |d| d.priority = p as f32);
                return Some(Ok(zero));
            }
            XVal::Num(g.dialogue_view().topic(&t).map_or(0.0, |d| f64::from(d.priority)))
        }
        // ---- a line's choices (SUP) and related topics (JohnnyGuitar) ----
        // `info topic position`: the topic into the line's choices, -1 the end, 0 the
        // beginning, past the end nowhere; a topic already there goes in again
        // [documented: TopicInfoAddChoice; observed: sup2 choices.add.*].
        "topicinfoaddchoice" => {
            if let (Some(i), Some(t)) = (line(g, form(args, 0)), topic(g, form(args, 1))) {
                g.add_related(i, t, Related::Choices, num(args, 2).unwrap_or(-1.0));
            }
            zero
        }
        // `info topic position`: the choice at `position` (from 0) becomes the topic
        // [documented: TopicInfoReplaceChoice; hypothesis: from 0, as
        // TopicInfoGetAllChoices' array, and nothing past the end].
        "topicinforeplacechoice" => {
            if let (Some(i), Some(t)) = (line(g, form(args, 0)), topic(g, form(args, 1))) {
                g.replace_choice(i, t, num(args, 2).unwrap_or(-1.0));
            }
            zero
        }
        // `info form type position`: into Link From (0) or Choices (1) a topic
        // [documented: DialogResponseAddRelatedTopic, the three types]; Follow Ups (2)
        // take nothing [observed: sup2 related.add.followinfo and …followtopic];
        // positions as TopicInfoAddChoice [hypothesis].
        "dialogresponseaddrelatedtopic" => {
            let Some(i) = line(g, form(args, 0)) else { return Some(Ok(zero)) };
            let Some(kind) = num(args, 2).and_then(Related::from_code) else { return Some(Ok(zero)) };
            let item = match kind {
                Related::FollowUps => line(g, form(args, 1)),
                _ => topic(g, form(args, 1)),
            };
            if let Some(item) = item {
                g.add_related(i, item, kind, num(args, 3).unwrap_or(-1.0));
            }
            zero
        }
        // `info [type]`: the line's Link From (0), Choices (1) or Follow Ups (2), in
        // order [documented: DialogResponseRelatedGetAll; hypothesis: no type is 0, the
        // first the page lists].
        "dialogresponserelatedgetall" => {
            let kind = Related::from_code(num(args, 1).unwrap_or(0.0));
            let items: Vec<XVal> = match (line(g, form(args, 0)), kind) {
                (Some(i), Some(k)) => g.dialogue_view().info(&i).map(|d| k.list(d).iter().map(|f| XVal::Ref(f.0)).collect()).unwrap_or_default(),
                _ => Vec::new(),
            };
            list(g, items)
        }
        // ---- a line's flags (SUP) ----
        // `info [flags]`: DATA's first flag byte (Goodbye, Random, Say Once, Run
        // Immediately, Info Refusal, Random End …) for …High, its second (Say Once a
        // Day, Always Darken) for …Low; set first when `flags` is given; returns the
        // byte, 0 when setting [documented: TopicInfoSetFlagsHigh, TopicInfoSetFlagsLow,
        // the tables; ITR's SetDialogueInfoFlags replaces them; observed: sup2
        // info.high.set, info.low.set, the low byte of 511 written]. The dialogue's Say
        // Once, Goodbye, Random and Say Once a Day rules read the change.
        "topicinfosetflagshigh" | "topicinfosetflagslow" => {
            let Some(i) = line(g, form(args, 0)) else { return Some(Ok(zero)) };
            let high = name == "topicinfosetflagshigh";
            let shift = if high { 0 } else { 8 };
            if let Some(f) = num(args, 1) {
                let old = g.info_flags(i).unwrap_or(0);
                let keep = if high { old & 0xFF00 } else { old & 0x00FF };
                g.set_info_flags(i, keep | (byte(f) << shift));
                return Some(Ok(zero));
            }
            XVal::Num(f64::from((g.info_flags(i).unwrap_or(0) >> shift) & 0xFF))
        }
        // `info [prompt]`: the text shown for the line instead of the topic's name; an
        // empty one leaves the topic's name, as an empty RNAM does; returns the prompt,
        // "" when it has none or when setting [documented: TopicInfoSetPrompt; observed:
        // sup2 prompt.set; hypothesis: the empty prompt].
        "topicinfosetprompt" => {
            let Some(i) = line(g, form(args, 0)) else { return Some(Ok(XVal::Str(String::new()))) };
            if let Some(XVal::Str(p)) = args.get(1) {
                let p = Some(p.clone()).filter(|p| !p.is_empty());
                g.edit_info(i, |d| d.prompt = p);
                return Some(Ok(XVal::Str(String::new())));
            }
            XVal::Str(g.dialogue_view().info(&i).and_then(|d| d.prompt.clone()).unwrap_or_default())
        }
        // ---- said once (SUP, JohnnyGuitar) ----
        // `info [set]`: whether the line has been said, which holds a Say Once line back;
        // set for the session when `set` is given, returning 0 [documented:
        // TopicInfoSetSaidOnce, "not saved"; observed: sup2 said.sup].
        "topicinfosetsaidonce" => {
            let Some(i) = line(g, form(args, 0)) else { return Some(Ok(zero)) };
            if let Some(s) = num(args, 1) {
                g.set_said_once(i, s != 0.0, false);
                return Some(Ok(zero));
            }
            yes(g.said_once(i))
        }
        // `info` [documented: GetSaidOnce].
        "getsaidonce" => yes(line(g, form(args, 0)).is_some_and(|i| g.said_once(i))),
        // `info value [saveBake]`: set the value; `saveBake` keeps it in the save, else
        // for the session; returns 0 [documented: SetSaidOnce; observed: sup2 said.jg].
        "setsaidonce" => {
            if let Some(i) = line(g, form(args, 0)) {
                g.set_said_once(i, num(args, 1).unwrap_or(0.0) != 0.0, num(args, 2).unwrap_or(0.0) != 0.0);
            }
            zero
        }
        // ---- topics and their lines (JohnnyGuitar) ----
        // `info`: the topic the line is under [documented: GetParentTopic].
        "getparenttopic" => XVal::Ref(line(g, form(args, 0)).and_then(|i| g.data.infos[&i].topic).map_or(0, |t| t.0)),
        // `topic [quest]`: the topic's lines, only the quest's when one is given
        // [documented: GetTopicInfo], as SUP's TopicGetAllTopicInfos.
        "gettopicinfo" => {
            let quest = form(args, 1);
            let infos: Vec<XVal> = form(args, 0)
                .and_then(|t| g.data.topics.get(&t))
                .into_iter()
                .flatten()
                .filter(|i| quest.is_none() || g.data.infos.get(i).is_some_and(|d| d.quest == quest))
                .map(|i| XVal::Ref(i.0))
                .collect();
            list(g, infos)
        }
        // ---- response texts (ShowOff) ----
        // `info index text`: the response at `index` (from 0, as
        // TopicInfoGetResponseStrings' array) says `text`; 1 when there is one there
        // [documented: TopicInfoSetNthResponseString].
        "topicinfosetnthresponsestring" => {
            let Some(i) = line(g, form(args, 0)) else { return Some(Ok(zero)) };
            let (Some(at), Some(XVal::Str(text))) = (num(args, 1), args.get(2)) else { return Some(Ok(zero)) };
            let fits = at >= 0.0 && g.dialogue_view().info(&i).is_some_and(|d| (at as usize) < d.responses.len());
            yes(fits && g.edit_info(i, |d| set_response(d, at as usize, text)))
        }
        // `info array`: 1 for a line and an array, and the texts stay as they are
        // [observed: sup2 responses, ar_List "First", "Second" on a one-response line
        // returned 1 and the response kept its text]. The page says each element
        // replaces the next response's text [documented]; the installed ShowOff did not
        // (ext-sup2.md §5).
        "topicinfosetresponsestrings" => yes(line(g, form(args, 0)).is_some() && matches!(args.get(1), Some(XVal::Array(_)))),
        _ => return None,
    };
    Some(Ok(v))
}

/// A response's text, and its length for the timing when there is no voice file
/// ([`crate::data::InfoDef::response_chars`]).
fn set_response(d: &mut crate::data::InfoDef, at: usize, text: &str) {
    d.responses[at].text = text.to_string();
    if let Some(n) = d.response_chars.get_mut(at) {
        *n = text.chars().count();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialogue_edits::insert_at;

    #[test]
    fn positions_and_bytes() {
        // [documented: TopicInfoAddChoice: -1 the end, 0 the beginning; observed: past
        // the end, nowhere].
        assert_eq!((insert_at(3, -1.0), insert_at(3, 0.0), insert_at(3, 3.0), insert_at(3, 9.0)), (Some(3), Some(0), Some(3), None));
        assert_eq!((byte(0x1FF as f64), byte(2.0)), (0xFF, 2));
    }
}
