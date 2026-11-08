//! How long a response of a dialogue line lasts (docs/design/dialogue.md step D3,
//! docs/spec-notes/voice.md §8): as long as its voice file when the game was given a
//! way to ask ([`Game::voice`], crate `fnv-voice`), else the length fitted to its text
//! ([`crate::game::fitted_seconds`], docs/spec-notes/scripts.md §7.7). Every response is
//! timed through `Game::response_seconds` (`crate::dialogue`), which comes here.

use crate::game::{fitted_seconds, Game};
use fnv_sim::FormId;

/// The lengths of voice files. Implemented outside this crate (`fnv_voice::VoiceBank`),
/// so the rules stay free of archives and sound formats.
pub trait VoiceLengths: Send + Sync {
    /// Seconds of the sound of response `response` (index in record order, as in
    /// `InfoDef::response_chars`) of line `info`, said by an actor or talking activator
    /// whose base form is `speaker_base`. `None`: no sound is known for it.
    fn response_seconds(&self, info: FormId, response: usize, speaker_base: Option<FormId>) -> Option<f64>;
}

impl Game<'_> {
    /// [`Game::response_seconds`] for a speaker given by its base form (for callers that
    /// hold the world themselves, as the engine does between frames): the length of the
    /// response's voice file [hypothesis: the game ends a line when its sound ends; the
    /// intro's stage timings compare both lengths with the capture, voice.md §8], else
    /// the length fitted to its text.
    ///
    /// A line with a fixed speaker (INFO `ANAM`) said by another actor (its own GetIsID
    /// condition lets that actor say it, `Game::can_say_as`) has its file in the fixed
    /// speaker's voice folder, not in the actor's: the speaker's own voice is tried
    /// first, then the fixed speaker's [observed: Old World Blues' Think Tank lines
    /// spoken through Dr. Klein with ANAM Dr. 8 or Dr. O (INFO 010134BA) and the end
    /// slides with ANAM a Boomer said by the narrator (INFO 001614B2) exist only in the
    /// fixed speaker's folder (`fnvtool voice line`); voice.md §3.1: 22,252 of 22,589
    /// fixed-speaker responses are in that speaker's folder; hypothesis: the game plays
    /// that file; found by walkthrough/old-world-blues.actions.json].
    pub fn response_seconds_as(&self, speaker_base: Option<FormId>, info: FormId, response: usize) -> f64 {
        self.response_seconds_voiced(speaker_base, speaker_base, info, response)
    }

    /// [`Game::response_seconds_as`] for a speaker placed with `placed_base` that speaks
    /// with `voice_base`'s voice ([`Game::voice_base`]). The fixed speaker's file stands in
    /// only for a line said by another actor: a fixed speaker that is the placed base
    /// itself is no other actor, and its file (in the record's own voice type) would bring
    /// back the voice the actor does not have (issue #1051).
    pub fn response_seconds_voiced(&self, placed_base: Option<FormId>, voice_base: Option<FormId>, info: FormId, response: usize) -> f64 {
        let voice = self.voice.as_ref();
        let sound = voice.and_then(|v| v.response_seconds(info, response, voice_base)).or_else(|| {
            let fixed = self.dialogue_view().info(&info).and_then(|d| d.speaker).filter(|s| Some(*s) != voice_base && Some(*s) != placed_base)?;
            voice?.response_seconds(info, response, Some(fixed))
        });
        sound.unwrap_or_else(|| {
            let chars = self.dialogue_view().info(&info).and_then(|d| d.response_chars.get(response)).copied().unwrap_or(0);
            fitted_seconds(chars)
        })
    }

    /// The length of each response of a line, in record order. A line without responses
    /// lasts as one empty response does ([`Game::line_seconds`] is their sum).
    pub fn response_lengths(&self, speaker: FormId, info: FormId) -> Vec<f64> {
        self.response_lengths_voiced(crate::functions::base_of(self, speaker), self.speaker_voice_base(speaker), info)
    }

    /// [`Game::response_lengths`] for a speaker placed with `placed_base` speaking with
    /// `voice_base`'s voice ([`Game::response_seconds_voiced`]).
    pub fn response_lengths_voiced(&self, placed_base: Option<FormId>, voice_base: Option<FormId>, info: FormId) -> Vec<f64> {
        let n = self.dialogue_view().info(&info).map_or(0, |d| d.response_chars.len());
        (0..n.max(1)).map(|k| self.response_seconds_voiced(placed_base, voice_base, info, k)).collect()
    }

    /// The base form whose voice `speaker` (a reference) speaks with
    /// ([`Game::voice_base`] of its base).
    pub fn speaker_voice_base(&self, speaker: FormId) -> Option<FormId> {
        crate::functions::base_of(self, speaker).map(|b| self.voice_base(b, speaker))
    }

    /// [`Game::response_lengths`] for a speaker given by its base form.
    pub fn response_lengths_as(&self, speaker_base: Option<FormId>, info: FormId) -> Vec<f64> {
        let n = self.dialogue_view().info(&info).map_or(0, |d| d.response_chars.len());
        (0..n.max(1)).map(|k| self.response_seconds_as(speaker_base, info, k)).collect()
    }
}
