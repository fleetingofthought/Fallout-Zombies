//! Radios play their station (docs/spec-notes/radio.md): receivers in the world, actors
//! made radios by SetNPCRadio, and the Pip-Boy radio.
//!
//! - **Receivers** are placed activators whose base names a radio station
//!   (`fnv_world::radio`). One plays while it is switched on (`fnv_game::radio`: a default
//!   activation switches it, the "StartOn" scripts activate their radio once on load),
//!   is enabled and is not destroyed (SetDestroyed: Goodsprings' broken radio). A
//!   receiver whose base has neither a script nor a name (the Tops performers'
//!   `marker_audio.nif` speakers, one Gomorrah speaker) starts switched on [hypothesis:
//!   nothing could switch them on otherwise; the player cannot activate an unnamed
//!   activator].
//! - **Actors made radios** (SetNPCRadio: Lily's recording, EDE) play like a receiver
//!   at the actor's position, heard by the station's radio template. The filter the game
//!   puts on lines the actor is not the speaker of is not built.
//! - **The Pip-Boy radio** (`fnv_game::radio::PipBoyRadio`, scripts' PipBoyRadio and
//!   PipBoyRadioOff) plays its station at the listener, with no falloff and the song's
//!   own (stereo) file, not the `_mono` copy [documented: GECK wiki Creating Radio
//!   Stations: the mono file "will be what is played out of speakers and radios"]. It
//!   receives the stations whose base is a radio station without the Non-Pipboy flag,
//!   whose placed reference is enabled and in range ([`fnv_world::radio::reception`]);
//!   a script may tune it to any enabled station in range [documented: GECK wiki
//!   PipBoyRadio: a disabled or out-of-range station "cannot be selected"; observed:
//!   Repcon's launch script tunes the Pip-Boy to its Non-Pipboy station]. There is no
//!   Pip-Boy screen yet: `]` tunes to the next station received (off → first → … → last
//!   → off) and `[` to the previous, unless a script has turned the Pip-Boy off
//!   (DisablePlayerControls) [design]. Turned on without a station it takes the first
//!   one received [design]. Out of range it falls silent until the station is received
//!   again [design]. In a Radius station's static band the station fades into the
//!   `UIRadioStaticLP` loop [hypothesis: the static sound by its name; GECK wiki
//!   Reference: "plays static over this percentage of the radius"].
//! - **The station's programme** is dialogue: its placed reference says the lines of its
//!   radio quests one after another (`Game::radio_next`, begin and end result scripts
//!   run, so the quests' "last played" variables rotate the songs) while it broadcasts
//!   (SetBroadcastState, else the Continuous Broadcast flag). One programme per station,
//!   shared by everything playing it: what starts hearing it mid-line joins at the point
//!   the line has reached [design]. A station's programme starts when it is first heard
//!   and runs on from then, heard or not [design: a station nobody has heard yet runs no
//!   scripts]. A station that stops broadcasting finishes the line it is saying [design].
//! - **The poker table** ([`TableStation`]): its casino's station plays at the listener,
//!   flat, with the song's own file as the Pip-Boy plays it; while it does, nothing else
//!   plays a station [design].
//! - **StartRadioConversation** (`Game::take_radio_conversations`): the conversation
//!   starts at once, heard or not, interrupting the programme's line (whose end script
//!   then does not run) [documented: GECK wiki StartRadioConversation, "interrupt the
//!   broadcast with the new conversation"]; it follows the lines' links until one links
//!   nowhere, and the station then goes back to its programme or, not broadcasting, falls
//!   silent [documented]. A call while the station is still saying a conversation's
//!   line is not taken [design: the NCR Emergency Radio's quest asks every ten seconds].
//! - **Silence and static**: what plays a station that is saying nothing plays the
//!   station's own looping sound (TACT `SNAM`; Hanlon's recording has `OBJRadioStaticLP`)
//!   [hypothesis: GECK wiki SetBroadcastState, off, "the station will play static except
//!   when playing scripted conversations"]; the Pip-Boy plays `UIRadioStaticLP` then.
//! - **What a line plays**: a response with a sound form plays that sound's file — for a
//!   song from a receiver its `_mono.ogg` beside the `.mp3` the record names [documented:
//!   GECK wiki "Creating Radio Stations"; observed: all 48 songs have one in
//!   `Fallout - Sound.bsa`], and the sound wins over a voice file [documented: GECK wiki
//!   Radio Stations]; otherwise the response's voice file in the voice type of the
//!   line's fixed speaker when it names one, else the station's [observed: Black
//!   Mountain Radio's segment lines name Tabitha or Raul Tejada and their files are in
//!   those voice types' folders; the station has no voice type; documented: GECK wiki
//!   SetNPCRadio, a radio plays lines whose speaker is someone else]; with neither, the
//!   line lasts its fitted length in silence.
//! - **Loudness** [design]: the sound's static attenuation (songs, loops), DJ lines at
//!   full level; by distance from a receiver or actor the radio template's (the station's
//!   `INAM`, else the receiver's) minimum and maximum distance and attenuation curve as
//!   `crate::sfx::distance_gain` uses them; without either template 150 and 1800 units
//!   (the AMLRadio template's). Receivers and actors are panned as the sound effects are
//!   (`crate::sfx::Panned`, `SoundAt`); the Pip-Boy plays flat, at the listener. A
//!   receiver is heard only within that maximum distance.
//! - **No subtitles**: a station's talk lines are heard, never subtitled (owner, list 5
//!   item 27).
//! - The Esc pause menu and a loading screen hold the radios and the programmes, as for
//!   the placed loops (`crate::ambient`).
//! - Test aid: `NVF_PLAY_TEST_RADIO=<seconds>:<next|previous|on|off|tune=<station>>[,...]`
//!   presses `]` or `[`, turns the Pip-Boy radio on or off, or tunes it as a script's
//!   `PipBoyRadio 1 <station>` does (a placed station or its base, by Editor ID or hex
//!   FormID). Seconds of the radios' clock: from the first frame the scripts run, less
//!   pauses and loading screens.
//! - Log: `radio: <who> tuned to <station> playing <file>` when something starts a
//!   response or a loop, `radio: <station> says <topic> <info>: …`, `pipboy radio: …`,
//!   and at the end `radio summary: …` and `pipboy radio summary: …`.

use crate::control::ControlLocks;
use crate::music::Space;
use crate::pause::Paused;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sfx::{distance_gain, static_gain, Panned, Sfx, SfxSettings, SoundAt};
use crate::sim::SimState;
use crate::voice::VoiceState;
use crate::{Location, PlayState, PlayWorld};
use bevy::audio::{AudioSinkPlayback, ChannelCount, Decodable, Sample, SampleRate, Source};
use crate::audio_out::{AddAudioSource, SoundCategory};
use bevy::prelude::*;
use fnv_combat::fx::Sound;
use fnv_sim::FormId;
use fnv_world::radio::{self, Broadcast, Listener, RadioForms, Receiver};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Minimum and maximum distance without a radio template [design: AMLRadio's values].
const DEFAULT_MIN: f32 = 150.0;
const DEFAULT_MAX: f32 = 1800.0;
/// A receiver stops once the player is this many times its maximum distance away.
const LEAVE: f32 = 1.05;
/// Seconds a station with nothing to say waits before it asks again.
const RETRY: f64 = 5.0;
/// Silence between two lines, seconds [design].
const GAP: f64 = 0.5;
/// Something joining a response later than this many seconds into it seeks to there.
const JOIN: f64 = 0.25;
/// The keys of the Pip-Boy and its static among the receivers.
const PIPBOY: u32 = u32::MAX;
const PIPBOY_STATIC: u32 = u32::MAX - 1;
const TABLE: u32 = u32::MAX - 2;

/// A station heard at the listener, flat, wherever the player is: the casino's music at
/// NVForever's poker table (`crate::poker`, docs/design/poker.md §4). While it is set
/// the world's receivers, actors and the Pip-Boy are not heard [design: the player is
/// at the table, away from them].
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct TableStation(pub Option<u32>);
/// The Pip-Boy's static (see the module notes).
const STATIC_SOUND: &str = "UIRadioStaticLP";

// ---- playback -------------------------------------------------------------------------

type Stem = rodio::Decoder<Cursor<Arc<[u8]>>>;

/// A panic opening a broken file (a WAV of rate 0) is that file's failure, not the game's
/// (nvf_assets::guard); one of a rate or channels the mixer cannot take is not opened
/// (crate::sfx::format_ok).
fn open(bytes: &Arc<[u8]>) -> Option<Stem> {
    nvf_assets::guard::guarded(|| Ok(rodio::Decoder::builder().with_byte_len(bytes.len() as u64).with_seekable(true).with_data(Cursor::new(bytes.clone())).build().ok()))
        .ok()
        .flatten()
        .filter(|d| crate::sfx::format_ok(d.channels().get(), d.sample_rate().get()).is_ok())
}

/// One file played from a point in it (or round and round), at a level changed while it
/// plays.
#[derive(Asset, TypePath, Clone)]
pub struct RadioSound {
    bytes: Arc<[u8]>,
    start: Duration,
    looping: bool,
    level: Arc<AtomicU32>,
    /// How loud the file is where it plays (the root mean square of its last samples,
    /// before the level), for the Pip-Boy's waveform panel.
    meter: Arc<AtomicU32>,
    channels: ChannelCount,
    rate: SampleRate,
}

/// Samples the meter of a playing file is measured over.
const METER_SAMPLES: u32 = 2048;

impl RadioSound {
    fn new(bytes: Arc<[u8]>, start: f64, looping: bool) -> Option<(RadioSound, Arc<AtomicU32>)> {
        let d = open(&bytes)?;
        let level = Arc::new(AtomicU32::new(0));
        Some((RadioSound { bytes, start: Duration::from_secs_f64(start.max(0.0)), looping, level: level.clone(), meter: Arc::default(), channels: d.channels(), rate: d.sample_rate() }, level))
    }
}

pub struct RadioDecoder {
    stem: Option<Stem>,
    bytes: Arc<[u8]>,
    looping: bool,
    level: Arc<AtomicU32>,
    meter: Arc<AtomicU32>,
    /// The squares summed and the samples counted towards the next meter reading.
    squares: f32,
    counted: u32,
    channels: ChannelCount,
    rate: SampleRate,
}

impl Iterator for RadioDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let s = match self.stem.as_mut()?.next() {
            Some(s) => s,
            None if self.looping => {
                self.stem = open(&self.bytes);
                self.stem.as_mut()?.next()?
            }
            None => return None,
        };
        self.squares += s * s;
        self.counted += 1;
        if self.counted == METER_SAMPLES {
            self.meter.store((self.squares / METER_SAMPLES as f32).sqrt().to_bits(), Ordering::Relaxed);
            (self.squares, self.counted) = (0.0, 0);
        }
        Some(s * f32::from_bits(self.level.load(Ordering::Relaxed)))
    }
}

impl Source for RadioDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for RadioSound {
    type Decoder = RadioDecoder;

    fn decoder(&self) -> RadioDecoder {
        let mut stem = open(&self.bytes);
        if let Some(s) = stem.as_mut().filter(|_| !self.start.is_zero()) {
            if s.try_seek(self.start).is_err() {
                // Decode and drop up to the point instead.
                let n = (self.start.as_secs_f64() * self.rate.get() as f64) as usize * self.channels.get() as usize;
                for _ in 0..n {
                    if s.next().is_none() {
                        break;
                    }
                }
            }
        }
        RadioDecoder { stem, bytes: self.bytes.clone(), looping: self.looping, level: self.level.clone(), meter: self.meter.clone(), squares: 0.0, counted: 0, channels: self.channels, rate: self.rate }
    }
}

// ---- the data -----------------------------------------------------------------------------

/// A placed station: its base, whether it starts disabled, and where it broadcasts from.
#[derive(Debug, Clone)]
pub struct Site {
    pub base: u32,
    pub initially_disabled: bool,
    pub broadcast: Option<Broadcast>,
}

/// The radio forms and the placed receivers by space; the placed stations.
#[derive(Resource, Default)]
pub struct RadioPlaces {
    pub forms: RadioForms,
    pub receivers: HashMap<Space, Vec<Receiver>>,
    /// Station base → its placed reference (the lowest FormID when there are several).
    pub station_ref: HashMap<u32, u32>,
    /// Every placed station by reference.
    pub sites: HashMap<u32, Site>,
}

impl RadioPlaces {
    /// The station base of a placed station.
    fn base_of(&self, station_ref: u32) -> Option<u32> {
        self.sites.get(&station_ref).map(|s| s.base)
    }

    /// Whether receivers of `base` start switched on (see the module notes).
    fn starts_on(&self, base: u32) -> bool {
        self.forms.receivers.get(&base).is_some_and(|b| b.script.is_none() && b.name.is_none())
    }
}

fn init(mut commands: Commands, d: Res<PlayWorld>) {
    let d = d.0;
    crate::start_read::start(&mut commands, "nvf-radio-read", move || read_places(d));
}

/// [`RadioPlaces`] from the plugins, on its own thread (`crate::start_read`).
fn read_places(d: crate::PlayData) -> RadioPlaces {
    let t = std::time::Instant::now();
    let forms = RadioForms::build(d.lo, d.schema);
    let (stations, receivers) = radio::placed(d.lo, d.schema, d.index, &forms, d.index.cells.keys().copied());
    let space = |cell: u32| d.index.cells.get(&cell).and_then(|c| c.world).map_or(Space::Interior(cell), Space::World);
    let mut places = RadioPlaces { forms, ..Default::default() };
    for s in &stations {
        let first = places.station_ref.entry(s.base).or_insert(s.ref_id);
        *first = (*first).min(s.ref_id);
        let broadcast = radio::broadcast(d.lo, d.schema, d.index, s);
        places.sites.insert(s.ref_id, Site { base: s.base, initially_disabled: s.initially_disabled, broadcast });
    }
    let n = receivers.len();
    for r in receivers {
        places.receivers.entry(space(r.cell)).or_default().push(r);
    }
    info!(
        "radio: {} stations ({} placed, {} on the Pip-Boy), {} receiver forms, {n} receivers read in {:.0} ms",
        places.forms.stations.values().filter(|s| s.is_radio()).count(),
        places.station_ref.len(),
        places.station_ref.keys().filter(|b| places.forms.stations.get(b).is_some_and(|s| s.on_pipboy())).count(),
        places.forms.receivers.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    places
}

// ---- the programmes -----------------------------------------------------------------------

/// One response of a line on the air.
#[derive(Debug, Clone)]
struct Part {
    response: usize,
    /// The file a receiver plays (a song's `_mono` copy when there is one).
    path: Option<String>,
    /// The file the Pip-Boy plays (the song's own file).
    stereo: Option<String>,
    seconds: f64,
    song: bool,
    gain: f32,
}

#[derive(Debug, Clone)]
struct OnAir {
    info: FormId,
    parts: Vec<Part>,
    part: usize,
    /// Radio clock at which the current part started.
    started: f64,
}

#[derive(Debug, Default)]
struct Programme {
    on_air: Option<OnAir>,
    last: Option<FormId>,
    /// Radio clock before which the station does not look for a line.
    wait_until: f64,
    /// Saying a conversation a script started.
    conversation: bool,
    /// A conversation asked for (the topic named), not yet started.
    pending: Option<Option<FormId>>,
    /// Saying nothing, in no conversation and not broadcasting: its loop (static) plays.
    quiet: bool,
}

/// What something plays.
#[derive(Debug, Clone, PartialEq)]
enum Track {
    Line { info: FormId, part: usize },
    Loop(String),
}

/// Something sounding.
struct Playing {
    entity: Entity,
    level: Arc<AtomicU32>,
    meter: Arc<AtomicU32>,
    track: Track,
    /// A song (a response with a sound form), not talk or a loop.
    song: bool,
}

#[derive(Component)]
struct RadioEntity;

/// A Pip-Boy radio control: a key or the test aid.
#[derive(Debug, Clone, PartialEq)]
pub enum RadioAid {
    Next,
    Previous,
    On,
    Off,
    Tune(String),
}

/// `NVF_PLAY_TEST_RADIO`: `<seconds>:<next|previous|on|off|tune=<station>>` items
/// separated by commas, in time order.
pub fn parse_aids(v: &str) -> Result<Vec<(f32, RadioAid)>, String> {
    let mut out = v
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (t, what) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <seconds>:<what>"))?;
            let at: f32 = t.trim().parse().map_err(|_| format!("{item:?}: bad seconds {t:?}"))?;
            let aid = match what.trim() {
                "next" => RadioAid::Next,
                "previous" => RadioAid::Previous,
                "on" => RadioAid::On,
                "off" => RadioAid::Off,
                w => match w.strip_prefix("tune=") {
                    Some(s) if !s.trim().is_empty() => RadioAid::Tune(s.trim().to_string()),
                    _ => return Err(format!("{item:?}: expected next, previous, on, off or tune=<station>")),
                },
            };
            Ok((at, aid))
        })
        .collect::<Result<Vec<_>, String>>()?;
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(out)
}

#[derive(Resource, Default)]
pub struct Radio {
    /// Seconds the programmes have run (stands still while held).
    clock: f64,
    programmes: HashMap<u32, Programme>,
    playing: HashMap<u32, Playing>,
    pub files: crate::file_bytes::FileBytes,
    lengths: HashMap<String, f64>,
    reported: HashSet<String>,
    space: Option<Space>,
    /// Times each receiver was switched, as last seen.
    switched: HashMap<u32, u32>,
    /// Destroyed receivers already reported.
    silent: HashSet<u32>,
    /// The listener's worldspace and position when last outdoors.
    outdoors: Option<(u32, [f32; 3])>,
    /// The Pip-Boy's stations received now (bases, in the list's order), and as logged.
    pub pipboy_list: Vec<u32>,
    logged_list: Option<Vec<u32>>,
    /// The Pip-Boy radio as last logged: on, the station, received.
    logged_pipboy: Option<(bool, Option<FormId>, bool)>,
    aids: VecDeque<(f32, RadioAid)>,
    static_sound: Option<Option<Sound>>,
    /// The Pip-Boy's played loudness (RMS after its gain), summed per frame, and the
    /// frames counted: songs, then talk (#1077).
    loudness: [(f64, u64); 2],
    pub heard: BTreeSet<u32>,
    pub npc_heard: BTreeSet<u32>,
    pub songs: u32,
    pub dj_lines: u32,
    pub silent_lines: u32,
    pub missing: u32,
    pub loops: u32,
    pub pipboy_parts: u32,
    pub tunes: u32,
    pub script_tunes: u32,
    pub refused: u32,
    pub conversations: u32,
    pub conversations_ignored: u32,
}

impl Radio {
    /// How loud the Pip-Boy radio's sound is now (its station, else its static; 0 while
    /// it plays nothing), for the waveform panel.
    pub fn pipboy_loudness(&self) -> f32 {
        self.playing.get(&PIPBOY).or_else(|| self.playing.get(&PIPBOY_STATIC)).map_or(0.0, |p| f32::from_bits(p.meter.load(Ordering::Relaxed)))
    }

    fn bytes(&mut self, vfs: &fnv_vfs::Vfs, path: &str) -> Option<Arc<[u8]>> {
        self.files.get_or_read(path, || match vfs.read(path) {
            Some(Ok(b)) => Some(b.into()),
            Some(Err(e)) => {
                warn!("radio: {path}: {e}");
                None
            }
            None => None,
        })
    }

    /// Seconds a file lasts: from the Ogg or WAV headers, else by decoding it.
    fn length(&mut self, vfs: &fnv_vfs::Vfs, path: &str) -> Option<f64> {
        if let Some(&s) = self.lengths.get(path) {
            return Some(s);
        }
        let bytes = self.bytes(vfs, path)?;
        let secs = match fnv_voice::sound_info(&bytes) {
            Ok(i) => i.seconds,
            Err(_) => {
                let d = open(&bytes)?;
                let (ch, rate) = (d.channels().get() as f64, d.sample_rate().get() as f64);
                d.count() as f64 / (ch * rate)
            }
        };
        self.lengths.insert(path.to_string(), secs);
        Some(secs)
    }
}

/// The file a song plays from a receiver: the `_mono.ogg` beside the named file when the
/// archives have it, else the named file (see the module notes).
pub fn receiver_file(named: &str, exists: impl Fn(&str) -> bool) -> String {
    if let Some(stem) = named.strip_suffix(".mp3").or_else(|| named.strip_suffix(".wav")) {
        let mono = format!("{stem}_mono.ogg");
        if exists(&mono) {
            return mono;
        }
    }
    named.to_string()
}

/// A sound form's file as an archive path.
fn sound_path(s: &Sound) -> Option<String> {
    s.file.as_deref().map(|f| fnv_vfs::normalize(&format!("sound\\{}", f.trim_start_matches(['\\', '/']))))
}

/// The template a station is heard by from a receiver of base `receiver_base` (or an
/// actor): the station's, the receiver's, or the default [documented: GECK wiki
/// Activator: the template "must be applied to Talking Activator to have proper effect"].
fn template(places: &RadioPlaces, sfx: &Sfx, station: u32, receiver_base: Option<u32>) -> Sound {
    let base = receiver_base.and_then(|b| places.forms.receivers.get(&b));
    let id = places.forms.stations.get(&station).and_then(|s| s.template).or_else(|| base.and_then(|b| b.template));
    let found = id.and_then(|id| sfx.forms.sounds.get(&FormId(id))).filter(|s| s.max_distance > 0.0);
    found.cloned().unwrap_or(Sound { min_distance: DEFAULT_MIN, max_distance: DEFAULT_MAX, ..Sound::default() })
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn name(places: &RadioPlaces, station: u32) -> String {
    places.forms.stations.get(&station).and_then(|s| s.edid.clone()).unwrap_or_else(|| format!("{station:08X}"))
}

/// The level a song a line plays (a response's sound form) is played at: full, its sound
/// record's static attenuation left out (#1077). [hypothesis, from the data: the radio
/// songs' records take 12.33 dB off, but four (MUSNVStreetsOfNewReno, New Vegas Valley,
/// Home on the Wastes, Cobwebs and Rainbows) take none, while every song file is about as
/// loud as the others (-30 to -34 dB mean, ffmpeg volumedetect); applied, those four would
/// play 12 dB above the rest. Applied, the music also sat about 26 dB under the DJ lines
/// on the Pip-Boy (RMS 0.004 against 0.080), the owner's "too quiet"; without it, about 13
/// dB under, which is the files' own difference.]
pub const SONG_GAIN: f32 = 1.0;

/// The parts of a line: per response in the order said, its files and length.
#[allow(clippy::too_many_arguments)]
fn parts(radio: &mut Radio, g: &fnv_game::Game, sfx: &Sfx, bank: &fnv_voice::VoiceBank, vfs: &fnv_vfs::Vfs, base: u32, info: FormId) -> Vec<Part> {
    let Some(def) = g.data.infos.get(&info) else { return Vec::new() };
    let mut out = Vec::new();
    for r in def.response_order() {
        let resp = &def.responses[r];
        if let Some(s) = resp.sound.and_then(|id| sfx.forms.sounds.get(&id)) {
            let named = sound_path(s);
            let path = named.as_deref().map(|n| receiver_file(n, |p| vfs.exists(p)));
            let seconds = path.as_deref().and_then(|p| radio.length(vfs, p));
            match (path, seconds) {
                (Some(p), Some(secs)) => {
                    let stereo = named.filter(|n| vfs.exists(n)).or_else(|| Some(p.clone()));
                    out.push(Part { response: r, path: Some(p), stereo, seconds: secs, song: true, gain: SONG_GAIN });
                    continue;
                }
                (p, _) => {
                    radio.missing += 1;
                    let what = p.unwrap_or_else(|| format!("{:?}", s.edid));
                    if radio.reported.insert(what.clone()) {
                        warn!("radio: {info}: the sound's file {what} is not in the archives");
                    }
                }
            }
        }
        let voice = def.speaker.map_or(base, |s| s.0);
        let path = bank.file(info.0, r, voice);
        let seconds = path.as_deref().and_then(|p| bank.seconds(p)).unwrap_or_else(|| g.response_seconds_as(Some(FormId(voice)), info, r));
        out.push(Part { response: r, stereo: path.clone(), path, seconds, song: false, gain: 1.0 });
    }
    out
}

/// Put `info` on the air on station `st` from radio clock `start`, with its log line.
#[allow(clippy::too_many_arguments)]
fn put_on_air(radio: &mut Radio, places: &RadioPlaces, g: &fnv_game::Game, sfx: &Sfx, bank: &fnv_voice::VoiceBank, vfs: &fnv_vfs::Vfs, st: u32, topic: FormId, info: FormId, start: f64) {
    let parts = parts(radio, g, sfx, bank, vfs, st, info);
    let topic_name = g.data.topic_defs.get(&topic).and_then(|t| t.edid.clone()).unwrap_or_default();
    for part in &parts {
        match (&part.path, part.song) {
            (Some(_), true) => radio.songs += 1,
            (Some(_), false) => radio.dj_lines += 1,
            (None, _) => radio.silent_lines += 1,
        }
    }
    info!(
        "radio: {} says {topic_name} {info}: {}",
        name(places, st),
        parts
            .iter()
            .map(|p| format!("{} {:.1} s", p.path.as_deref().unwrap_or(if p.song { "(no file)" } else { "(no voice file)" }), p.seconds))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let now = radio.clock;
    let p = radio.programmes.entry(st).or_default();
    p.last = Some(info);
    p.on_air = if parts.is_empty() { None } else { Some(OnAir { info, parts, part: 0, started: start }) };
    if p.on_air.is_none() {
        p.wait_until = now + GAP;
    }
}

/// Whether the placed station `station_ref` is enabled now.
fn enabled(sim: &SimState, places: &RadioPlaces, station_ref: u32) -> bool {
    let initially = places.sites.get(&station_ref).is_some_and(|s| s.initially_disabled);
    sim.world.is_enabled(FormId(station_ref)).unwrap_or(!initially)
}

/// The static share at which `station_ref` is received by `l`, `None` when it is not
/// (disabled or out of range).
fn received(sim: &SimState, places: &RadioPlaces, station_ref: u32, l: &Listener) -> Option<f32> {
    if !enabled(sim, places, station_ref) {
        return None;
    }
    places.sites.get(&station_ref)?.broadcast.as_ref().and_then(|b| radio::reception(b, l))
}

/// The Pip-Boy's list: radio stations without the Non-Pipboy flag received at `l`, by
/// name.
fn pipboy_list(sim: &SimState, places: &RadioPlaces, l: &Listener) -> Vec<u32> {
    let mut list: Vec<(String, u32)> = places
        .station_ref
        .iter()
        .filter(|(b, _)| places.forms.stations.get(b).is_some_and(|s| s.on_pipboy()))
        .filter(|(_, &r)| received(sim, places, r, l).is_some())
        .map(|(&b, _)| {
            let s = &places.forms.stations[&b];
            (s.name.clone().or_else(|| s.edid.clone()).unwrap_or_default().to_ascii_lowercase(), b)
        })
        .collect();
    list.sort();
    list.into_iter().map(|(_, b)| b).collect()
}

/// The Pip-Boy's controls: `]` and `[` while playing (unless scripts turned the Pip-Boy
/// off), and the test aid.
#[allow(clippy::too_many_arguments)]
fn pipboy_controls(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<PlayState>>,
    locks: Option<Res<ControlLocks>>,
    mut radio: ResMut<Radio>,
    places: Option<Res<RadioPlaces>>,
    mut script: ResMut<ScriptState>,
) {
    let (Some(places), Some(rt)) = (places, script.rt.as_mut()) else { return };
    // The placed stations, for scripts' GetAvailableRadios (`fnv_game::nvse_gaps`).
    if rt.game.gaps.radio_sites.is_none() {
        let mut sites: Vec<_> = places
            .sites
            .iter()
            .filter_map(|(&r, s)| {
                Some(fnv_game::nvse_gaps::RadioSite {
                    station: FormId(r),
                    initially_disabled: s.initially_disabled,
                    on_pipboy: places.forms.stations.get(&s.base).is_some_and(|f| f.on_pipboy()),
                    broadcast: s.broadcast.clone()?,
                })
            })
            .collect();
        sites.sort_by_key(|s| s.station);
        rt.game.gaps.radio_sites = Some(sites);
    }
    let radio = &mut *radio;
    let mut steps: Vec<(RadioAid, &str)> = Vec::new();
    if *state.get() == PlayState::Playing && !locks.is_some_and(|l| l.pipboy) {
        if keys.just_pressed(KeyCode::BracketRight) {
            steps.push((RadioAid::Next, "key"));
        }
        if keys.just_pressed(KeyCode::BracketLeft) {
            steps.push((RadioAid::Previous, "key"));
        }
    }
    while let Some((t, _)) = radio.aids.front() {
        if *t as f64 > radio.clock {
            break;
        }
        if let Some((_, aid)) = radio.aids.pop_front() {
            steps.push((aid, "test aid"));
        }
    }
    for (aid, by) in steps {
        info!("pipboy radio: {aid:?} ({by})");
        let p = &mut rt.game.radio.pipboy;
        let list = &radio.pipboy_list;
        let at = p.station.and_then(|s| places.base_of(s.0)).and_then(|b| list.iter().position(|&x| x == b)).filter(|_| p.on);
        let pick = |i: usize| Some(FormId(places.station_ref[&list[i]]));
        match aid {
            RadioAid::Next | RadioAid::Previous if list.is_empty() => info!("pipboy radio: no station is received here"),
            RadioAid::Next => {
                match at {
                    Some(i) if i + 1 >= list.len() => p.on = false,
                    Some(i) => p.station = pick(i + 1),
                    None => (p.on, p.station) = (true, pick(0)),
                }
                radio.tunes += 1;
            }
            RadioAid::Previous => {
                match at {
                    Some(0) => p.on = false,
                    Some(i) => p.station = pick(i - 1),
                    None => (p.on, p.station) = (true, pick(list.len() - 1)),
                }
                radio.tunes += 1;
            }
            RadioAid::On => p.on = true,
            RadioAid::Off => p.on = false,
            RadioAid::Tune(name) => {
                let id = match u32::from_str_radix(name.trim_start_matches("0x"), 16) {
                    Ok(id) => Some(id),
                    Err(_) => rt.game.data.by_edid(&name).map(|f| f.0),
                };
                // A base names its placed station.
                let station = id.map(|i| places.station_ref.get(&i).copied().unwrap_or(i));
                match station.filter(|s| places.sites.contains_key(s)) {
                    Some(s) => (p.on, p.tune) = (true, Some(FormId(s))),
                    None => warn!("NVF_PLAY_TEST_RADIO: {name:?} is not a placed station"),
                }
            }
        }
    }
}

/// What something plays this frame.
struct Want {
    key: u32,
    station: u32,
    track: Track,
    path: String,
    level: f32,
    into: f64,
    /// A line's response: the line, the response, a song.
    response: Option<(FormId, usize, bool)>,
    /// Where it is, for panning; `None`: flat (the Pip-Boy).
    at: Option<[f32; 3]>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play(
    mut commands: Commands,
    time: Res<Time<Real>>,
    settings: Res<SfxSettings>,
    mut radio: ResMut<Radio>,
    places: Option<Res<RadioPlaces>>,
    sfx: Option<Res<Sfx>>,
    voice: Res<VoiceState>,
    mut script: ResMut<ScriptState>,
    mut sim: ResMut<SimState>,
    (loc, state, paused, pw, table): (Res<Location>, Res<State<PlayState>>, Res<Paused>, Res<PlayWorld>, Res<TableStation>),
    (player, intro): (Option<Res<Player>>, Option<Res<crate::intro::Intro>>),
    data: Option<Res<nvf_assets::GameData>>,
    (mut assets, mut panned): (ResMut<Assets<RadioSound>>, ResMut<Assets<Panned<RadioSound>>>),
    (sinks, mut placed): (Query<&AudioSink, With<RadioEntity>>, Query<&mut SoundAt, With<RadioEntity>>),
) {
    let (Some(places), Some(sfx), Some(player), Some(data)) = (places, sfx, player, data) else { return };
    let Some(bank) = voice.bank.clone() else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let radio = &mut *radio;
    // A film with `muteAudio 0` leaves the world's radios playing (`crate::intro`).
    let film_keeps = intro.as_deref().and_then(|i| i.film_keeps_world_sound());
    let held = crate::intro::world_sound_held(paused.0, film_keeps) || *state.get() == PlayState::Loading;
    if !held {
        radio.clock += time.delta_secs_f64();
    }
    let now = radio.clock;
    let eye = player.eye();
    let space = Space::of(&loc);
    if radio.space != Some(space) {
        for (_, p) in radio.playing.drain() {
            commands.entity(p.entity).try_despawn();
        }
        radio.space = Some(space);
    }
    if loc.interior.is_none() {
        radio.outdoors = Some((loc.world, eye));
    }
    let listener = Listener {
        world: loc.world,
        interior: loc.interior,
        position: radio.outdoors.filter(|(w, _)| *w == loc.world).map_or(eye, |(_, p)| p),
    };
    let volume = settings.volume.max(0.0);
    let static_sound = radio.static_sound.get_or_insert_with(|| sfx.forms.sounds.values().find(|s| s.edid.as_deref() == Some(STATIC_SOUND)).cloned()).clone();

    // Receivers of this place switched on or off since the last frame.
    for r in places.receivers.get(&space).map_or(&[][..], Vec::as_slice) {
        let n = rt.game.radio_switches(FormId(r.ref_id));
        if radio.switched.get(&r.ref_id).copied().unwrap_or(0) != n {
            radio.switched.insert(r.ref_id, n);
            let on = rt.game.radio_on(FormId(r.ref_id)) != places.starts_on(r.base);
            info!("radio: {:08X} ({}) switched {}", r.ref_id, name(&places, r.station), if on { "on" } else { "off" });
        }
    }

    // Conversations scripts started.
    for (station_ref, topic) in rt.game.take_radio_conversations() {
        let Some(base) = places.base_of(station_ref.0) else {
            warn!("radio: StartRadioConversation on {station_ref}, which is not a placed station");
            continue;
        };
        radio.programmes.entry(base).or_default().pending = Some(topic);
    }

    // The Pip-Boy: its list, a script's tuning, and its station.
    radio.pipboy_list = pipboy_list(&sim, &places, &listener);
    if radio.logged_list.as_ref() != Some(&radio.pipboy_list) {
        info!("pipboy radio: stations received: {}", radio.pipboy_list.iter().map(|&b| name(&places, b)).collect::<Vec<_>>().join(", "));
        radio.logged_list = Some(radio.pipboy_list.clone());
    }
    let pip = &mut rt.game.radio.pipboy;
    if let Some(t) = pip.tune.take() {
        if received(&sim, &places, t.0, &listener).is_some() {
            pip.station = Some(t);
            radio.script_tunes += 1;
        } else {
            radio.refused += 1;
            info!("pipboy radio: {} cannot be tuned to here (disabled or out of range)", places.base_of(t.0).map_or(t.to_string(), |b| name(&places, b)));
        }
    }
    if pip.on && pip.station.is_none() {
        pip.station = radio.pipboy_list.first().map(|b| FormId(places.station_ref[b]));
    }
    let pip_station = pip.station.filter(|_| pip.on).and_then(|s| Some((s, places.base_of(s.0)?)));
    let pip_share = pip_station.and_then(|(s, _)| received(&sim, &places, s.0, &listener));
    let logged = (pip.on, pip.station, pip_share.is_some());
    if radio.logged_pipboy != Some(logged) {
        match pip_station {
            None => info!("pipboy radio: off"),
            Some((_, b)) if pip_share.is_some() => {
                info!("pipboy radio: on, tuned to {} (static {:.0} %)", name(&places, b), pip_share.unwrap_or(0.0) * 100.0)
            }
            Some((_, b)) => info!("pipboy radio: on, tuned to {}, which is not received here: silent", name(&places, b)),
        }
        radio.logged_pipboy = Some(logged);
    }

    // Everything that plays a station now: (key, station base, gain by distance, Pip-Boy,
    // where it is: `None` for the Pip-Boy, which is not panned).
    let mut heard: Vec<(u32, u32, f32, bool, Option<[f32; 3]>)> = Vec::new();
    // At the poker table only its station is heard.
    let table = table.0.filter(|_| settings.play);
    if let Some(base) = table {
        if places.station_ref.contains_key(&base) {
            heard.push((TABLE, base, 1.0, false, None));
        } else if radio.reported.insert(format!("table {base:08X}")) {
            warn!("radio: the poker table's station {base:08X} is not a placed station: silent");
        }
    }
    let receivers = if settings.play && table.is_none() { places.receivers.get(&space).map_or(&[][..], Vec::as_slice) } else { &[] };
    for r in receivers {
        let t = template(&places, &sfx, r.station, Some(r.base));
        let d = distance(r.position, eye);
        let playing = radio.playing.contains_key(&r.ref_id);
        if d >= t.max_distance * if playing { LEAVE } else { 1.0 } {
            continue;
        }
        let id = FormId(r.ref_id);
        let g = &rt.game;
        if g.extra.get(&id).is_some_and(|e| e.destroyed) {
            if radio.silent.insert(r.ref_id) {
                info!("radio: {:08X} ({}) is destroyed: silent", r.ref_id, name(&places, r.station));
            }
            continue;
        }
        let on = g.radio_on(id) != places.starts_on(r.base);
        if !on || !sim.world.is_enabled(id).unwrap_or(!r.initially_disabled) || !places.station_ref.contains_key(&r.station) {
            continue;
        }
        heard.push((r.ref_id, r.station, distance_gain(&t, d), false, Some(r.position)));
    }
    if settings.play && table.is_none() {
        for (&actor, &station_ref) in &rt.game.radio.npc_radios {
            let Some(base) = places.base_of(station_ref.0) else { continue };
            let Some(s) = sim.world.peek(actor) else { continue };
            let here = match (space, s.cell) {
                (Space::Interior(c), Some(cell)) => c == cell.0,
                (Space::World(w), Some(cell)) => pw.index.cells.get(&cell.0).and_then(|c| c.world) == Some(w),
                _ => false,
            };
            let t = template(&places, &sfx, base, None);
            let d = distance(s.pos, eye);
            if !here || !s.enabled() || d >= t.max_distance {
                continue;
            }
            if radio.npc_heard.insert(actor.0) {
                info!("radio: {actor} plays {} (SetNPCRadio)", name(&places, base));
            }
            heard.push((actor.0, base, distance_gain(&t, d), false, Some(s.pos)));
        }
        if let (Some((_, base)), Some(share)) = (pip_station, pip_share) {
            heard.push((PIPBOY, base, 1.0 - share, true, None));
        }
    }

    // The programmes move on: of the stations heard, of every station heard before, and
    // of those a script started a conversation on.
    let mut stations: BTreeSet<u32> = heard.iter().map(|h| h.1).collect();
    stations.extend(radio.programmes.keys().copied());
    if !held {
        for &st in &stations {
            let Some(&sref) = places.station_ref.get(&st) else { continue };
            let station_ref = FormId(sref);
            let broadcasting = with_world(rt, &mut sim, |rt| rt.game.broadcasting(station_ref));
            for _ in 0..8 {
                let p = radio.programmes.entry(st).or_default();
                if let Some(topic) = p.pending.take() {
                    if p.conversation && p.on_air.is_some() {
                        radio.conversations_ignored += 1;
                        info!("radio: {} is still saying a conversation: StartRadioConversation not taken", name(&places, st));
                    } else {
                        if let Some(on) = p.on_air.take() {
                            info!("radio: {} interrupted at {} for a conversation", name(&places, st), on.info);
                        }
                        let next = with_world(rt, &mut sim, |rt| {
                            let n = rt.game.radio_start(station_ref, topic);
                            if let Some((_, info)) = n {
                                rt.game.radio_line_begins(station_ref, info);
                            }
                            n
                        });
                        match next {
                            Some((topic, info)) => {
                                radio.conversations += 1;
                                info!("radio: {} starts a conversation (StartRadioConversation)", name(&places, st));
                                radio.programmes.entry(st).or_default().conversation = true;
                                put_on_air(radio, &places, &rt.game, &sfx, &bank, &data.vfs, st, topic, info, now);
                                continue;
                            }
                            None => info!("radio: {} has nothing to say for StartRadioConversation", name(&places, st)),
                        }
                    }
                }
                let p = radio.programmes.entry(st).or_default();
                if let Some(on) = p.on_air.as_mut() {
                    let secs = on.parts[on.part].seconds;
                    if now < on.started + secs {
                        break;
                    }
                    if on.part + 1 < on.parts.len() {
                        on.started += secs;
                        on.part += 1;
                        continue;
                    }
                    let (info, ended) = (on.info, on.started + secs);
                    p.on_air = None;
                    p.wait_until = ended + GAP;
                    with_world(rt, &mut sim, |rt| rt.game.radio_line_ends(station_ref, info));
                    continue;
                }
                if now < p.wait_until {
                    break;
                }
                let (last, conversation) = (p.last, p.conversation);
                let (next, ended) = with_world(rt, &mut sim, |rt| {
                    let mut n = None;
                    if conversation {
                        n = last.and_then(|l| rt.game.radio_follow(station_ref, l));
                    }
                    let ended = conversation && n.is_none();
                    if n.is_none() && broadcasting {
                        n = rt.game.radio_next(station_ref, if conversation { None } else { last });
                    }
                    if let Some((_, info)) = n {
                        rt.game.radio_line_begins(station_ref, info);
                    }
                    (n, ended)
                });
                let p = radio.programmes.entry(st).or_default();
                if ended {
                    p.conversation = false;
                    info!("radio: {} ends its conversation{}", name(&places, st), if broadcasting { "" } else { ": silent" });
                }
                let Some((topic, info)) = next else {
                    p.wait_until = now + if broadcasting { RETRY } else { GAP };
                    break;
                };
                let start = p.wait_until.max(now - 1.0).min(now);
                put_on_air(radio, &places, &rt.game, &sfx, &bank, &data.vfs, st, topic, info, start);
            }
            let p = radio.programmes.entry(st).or_default();
            p.quiet = !broadcasting && !p.conversation && p.on_air.is_none();
        }
    }

    // What each plays: the station's part, joining it where it has got to, else the
    // station's loop (the Pip-Boy: static).
    let mut wants: Vec<Want> = Vec::new();
    let loop_of = |st: u32| places.forms.stations.get(&st).and_then(|s| s.looping).and_then(|id| sfx.forms.sounds.get(&FormId(id))).cloned();
    for &(key, st, dgain, pipboy, at) in &heard {
        let p = radio.programmes.get(&st);
        match p.and_then(|p| p.on_air.as_ref()) {
            Some(on) => {
                let part = &on.parts[on.part];
                // The Pip-Boy and the table play the song's own (stereo) file.
                let Some(path) = (if pipboy || key == TABLE { part.stereo.clone() } else { part.path.clone() }) else { continue };
                wants.push(Want {
                    key,
                    station: st,
                    track: Track::Line { info: on.info, part: on.part },
                    path,
                    level: part.gain * dgain,
                    into: now - on.started,
                    response: Some((on.info, part.response, part.song)),
                    at,
                });
            }
            None if p.is_some_and(|p| p.quiet) => {
                let s = if pipboy { static_sound.clone() } else { loop_of(st) };
                let Some(s) = s else { continue };
                let Some(path) = sound_path(&s) else { continue };
                wants.push(Want { key, station: st, track: Track::Loop(path.clone()), path, level: static_gain(&s) * dgain, into: 0.0, response: None, at });
            }
            // Between two lines.
            None => {}
        }
    }
    if let (Some((_, base)), Some(share), Some(s)) = (pip_station, pip_share.filter(|&x| x > 0.0 && settings.play && table.is_none()), static_sound.as_ref()) {
        if let Some(path) = sound_path(s) {
            wants.push(Want { key: PIPBOY_STATIC, station: base, track: Track::Loop(path.clone()), path, level: static_gain(s) * share, into: 0.0, response: None, at: None });
        }
    }

    let mut sounding: HashSet<u32> = HashSet::new();
    for w in &wants {
        let gain = w.level * volume;
        sounding.insert(w.key);
        if let Some(p) = radio.playing.get(&w.key) {
            if p.track == w.track {
                p.level.store(gain.to_bits(), Ordering::Relaxed);
                // An actor playing a station moves.
                if let (Some(at), Ok(mut s)) = (w.at, placed.get_mut(p.entity)) {
                    if s.at != at {
                        s.at = at;
                    }
                }
                continue;
            }
            commands.entity(p.entity).try_despawn();
            radio.playing.remove(&w.key);
        }
        if held {
            continue;
        }
        let looping = matches!(w.track, Track::Loop(_));
        let Some(bytes) = radio.bytes(&data.vfs, &w.path) else { continue };
        let from = if w.into > JOIN { w.into } else { 0.0 };
        let Some((sound, level)) = RadioSound::new(bytes, from, looping) else {
            if radio.reported.insert(w.path.clone()) {
                warn!("radio: {}: not decoded", w.path);
            }
            continue;
        };
        level.store(gain.to_bits(), Ordering::Relaxed);
        let meter = sound.meter.clone();
        let entity = match w.at {
            // Panned where the receiver or actor is (`crate::sfx`).
            Some(at) => {
                let at = SoundAt::new(at, Some(&player));
                let handle = panned.add(Panned::new(sound, at.pan.clone()));
                commands.spawn((AudioPlayer::<Panned<RadioSound>>(handle), PlaybackSettings::DESPAWN, at, RadioEntity, SoundCategory::Radio)).id()
            }
            None => commands.spawn((AudioPlayer::<RadioSound>(assets.add(sound)), PlaybackSettings::DESPAWN, RadioEntity, SoundCategory::Radio)).id(),
        };
        let who = match w.key {
            PIPBOY => "the Pip-Boy".to_string(),
            PIPBOY_STATIC => "the Pip-Boy's static".to_string(),
            TABLE => "the poker table".to_string(),
            k => format!("{k:08X}"),
        };
        let what = match (&w.track, w.response) {
            (Track::Line { .. }, Some((_, r, _))) => format!("response {r}"),
            _ => "loop".to_string(),
        };
        info!(
            "radio: {who} tuned to {} playing {}{} ({what}, gain {gain:.3})",
            name(&places, w.station),
            w.path,
            if from > 0.0 { format!(" from {from:.1} s") } else { String::new() },
        );
        match w.key {
            PIPBOY => radio.pipboy_parts += 1,
            PIPBOY_STATIC | TABLE => {}
            k if rt.game.radio.npc_radios.contains_key(&FormId(k)) => {}
            k => {
                radio.heard.insert(k);
            }
        }
        if looping {
            radio.loops += 1;
        }
        radio.playing.insert(w.key, Playing { entity, level, meter, track: w.track.clone(), song: w.response.is_some_and(|(_, _, song)| song) });
    }
    // The Pip-Boy station's loudness as played, for the exit summary (#1077).
    if let Some((song, loud)) = radio.playing.get(&PIPBOY).map(|p| (p.song, f32::from_bits(p.meter.load(Ordering::Relaxed)) * f32::from_bits(p.level.load(Ordering::Relaxed)))).filter(|(_, l)| *l > 0.0) {
        let slot = &mut radio.loudness[usize::from(!song)];
        slot.0 += f64::from(loud);
        slot.1 += 1;
    }
    radio.playing.retain(|r, p| {
        let keep = sounding.contains(r);
        if !keep {
            commands.entity(p.entity).try_despawn();
        }
        keep
    });
    for sink in &sinks {
        match (held, sink.is_paused()) {
            (true, false) => sink.pause(),
            (false, true) => sink.play(),
            _ => {}
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, radio: Res<Radio>, script: Res<ScriptState>, places: Option<Res<RadioPlaces>>) {
    if exit.read().last().is_some() {
        let (mut switches, mut on, mut start_on) = (0, 0, 0);
        let mut pipboy = "off".to_string();
        let (mut script_calls, mut npc_radios, mut forced) = (0, 0, 0);
        if let (Some(rt), Some(places)) = (script.rt.as_ref(), places.as_ref()) {
            for r in places.receivers.values().flatten() {
                let n = rt.game.radio_switches(FormId(r.ref_id));
                switches += n;
                let is_on = rt.game.radio_on(FormId(r.ref_id)) != places.starts_on(r.base);
                if n > 0 {
                    on += is_on as u32;
                } else {
                    start_on += is_on as u32;
                }
            }
            let p = &rt.game.radio.pipboy;
            if let Some(b) = p.station.filter(|_| p.on).and_then(|s| places.base_of(s.0)) {
                pipboy = format!("on ({})", name(places, b));
            }
            (script_calls, npc_radios, forced) = (p.script_calls, rt.game.radio.npc_radios.len(), rt.game.radio.force_updates);
        }
        let mean = |(sum, n): (f64, u64)| if n > 0 { format!("{:.3}", sum / n as f64) } else { "-".into() };
        info!("pipboy radio loudness (RMS as played, before the Radio and master volumes): songs {}, talk {}", mean(radio.loudness[0]), mean(radio.loudness[1]));
        info!(
            "radio summary: {} receivers heard, {} songs started, {} DJ lines, {} lines without a file; receivers switched {switches} times, {on} on at exit ({start_on} more start on); {} sound files missing",
            radio.heard.len(),
            radio.songs,
            radio.dj_lines,
            radio.silent_lines,
            radio.missing
        );
        info!(
            "pipboy radio summary: {pipboy} at exit, {} stations received; tuned {} times by next/previous, {} by tune requests of scripts or the test aid ({} refused), {script_calls} script calls; {} responses played; {} conversations started by scripts ({} not taken); {} actors played as radios ({npc_radios} at exit); {} loops started; {forced} ForceRadioStationUpdate calls",
            radio.pipboy_list.len(),
            radio.tunes,
            radio.script_tunes,
            radio.refused,
            radio.pipboy_parts,
            radio.conversations,
            radio.conversations_ignored,
            radio.npc_heard.len(),
            radio.loops
        );
    }
}

fn load_aids(mut radio: ResMut<Radio>) {
    if let Ok(v) = std::env::var("NVF_PLAY_TEST_RADIO") {
        match parse_aids(&v) {
            Ok(a) => radio.aids = a.into(),
            Err(e) => warn!("NVF_PLAY_TEST_RADIO: {e}"),
        }
    }
}

pub struct RadioPlugin;

/// Let go of the radio's files nothing plays ([`crate::file_bytes`]).
fn release_files(real: Res<Time<Real>>, limits: Option<Res<nvf_assets::KeepLimits>>, x: Option<ResMut<Radio>>) {
    if let Some(mut x) = x {
        x.files.sweep(real.elapsed_secs_f64(), crate::file_bytes::limits(limits));
    }
}

impl Plugin for RadioPlugin {
    fn build(&self, app: &mut App) {
        crate::start_read::add_collect::<RadioPlaces>(app);
        app.init_resource::<Radio>()
            .init_resource::<TableStation>()
            .add_audio_source::<RadioSound>()
            .add_audio_source::<Panned<RadioSound>>()
            .add_systems(Startup, (init, load_aids))
            .add_systems(Update, (pipboy_controls, play).chain().after(crate::PlaySet::Scripts))
            .add_systems(Last, (release_files, report));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A radio song plays at full level whatever its record's static attenuation (#1077).
    #[test]
    fn songs_play_at_full_level() {
        assert_eq!(SONG_GAIN, 1.0);
        let song = Sound { static_attenuation_db: 12.33, ..Default::default() };
        assert!(static_gain(&song) < 0.25, "what the record alone would give");
    }

    #[test]
    fn a_song_plays_its_mono_file() {
        let have = |p: &str| p == "sound\\songs\\radionv\\mus_big_iron_mono.ogg";
        assert_eq!(receiver_file("sound\\songs\\radionv\\mus_big_iron.mp3", have), "sound\\songs\\radionv\\mus_big_iron_mono.ogg");
        // Without a mono file, the named file.
        assert_eq!(receiver_file("sound\\songs\\radio\\x.mp3", have), "sound\\songs\\radio\\x.mp3");
        assert_eq!(receiver_file("sound\\voice\\a.ogg", have), "sound\\voice\\a.ogg");
    }

    #[test]
    fn radio_aids_parse() {
        let a = parse_aids("20:off, 3:next,10:tune=RNVTARef,12:previous,14:on").unwrap();
        assert_eq!(
            a,
            vec![
                (3.0, RadioAid::Next),
                (10.0, RadioAid::Tune("RNVTARef".into())),
                (12.0, RadioAid::Previous),
                (14.0, RadioAid::On),
                (20.0, RadioAid::Off)
            ]
        );
        assert!(parse_aids("3:louder").is_err());
        assert!(parse_aids("x:next").is_err());
        assert!(parse_aids("3:tune=").is_err());
    }
}
