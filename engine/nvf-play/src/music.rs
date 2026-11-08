//! Music in the game (docs/spec-notes/audio.md §4, §13): the music of the place the
//! player is in, battle music, incidental phrases and the music scripts ask for.
//!
//! - **Where.** Audio markers (references with `MMRK` data) give places a radius and a
//!   media location controller (ALOC), which names media sets (MSET). The marker the
//!   player is deepest inside chooses (`fnv_world::audio::marker_at`); its controller's
//!   set is picked on entering it, at random among the list heard: with a conditional
//!   faction, the list for the faction's standing towards the player
//!   (`MediaController::sets_for`, GetFactionRelation's rule), else its location sets.
//!   A change of standing picks again.
//! - **Layers.** A location set has three tracks for the day and three for the night,
//!   files of one length under `music\`. All three play together from the start, mixed
//!   in one source so they stay in step ([`LayeredMusic`]); the outer one sounds
//!   anywhere inside the marker's radius and the middle and inner ones are faded in as
//!   the player comes nearer the marker (`fnv_world::audio::layers`) [hypothesis: the
//!   layers add up]. A layer fades over the set's layer crossfade time, and so does a
//!   change of set or between day and night [design]. A layer that came in stays at
//!   least the set's minimum time on [hypothesis: GECK wiki "Minimum Time on Layer"].
//! - **Dungeon sets** in a controller's lists play their explore track, add the suspense
//!   track while a hostile actor near has noticed the player and the battle track in a
//!   battle [hypothesis: the GECK wiki's descriptions; the three files are stems of one
//!   length like a location set's].
//! - **Battle** ([`MusicCues`]): a living hostile actor (`fnv_game::rest::hostile_to_player`)
//!   within `fCombatMusicNearCombatOuterRadius` that has the player detected
//!   [hypothesis; NVForever's actors do not fight on their own yet, design]. A battle
//!   set from the marker's controller's battle list, else from the region the cell's
//!   acoustic space or the cell names, picked at random; its intro sound after its wait
//!   time, its loop then, faded in and out over its loop fade-out time; the battle ends
//!   `fCombatMusicStopTime` seconds after the last threat, its outro sound plays, and
//!   the place's music comes back after the set's recovery time [documented: GECK wiki
//!   Media Set field descriptions, "unverified" there].
//! - **Incidental phrases**: where no music plays, the region's incidental set plays its
//!   day or night sound after a random wait between its two times [hypothesis: minutes,
//!   see audio.md §13.4].
//! - **Scripts** (`PlayMusic`, `fnv_game::sound`): the music type plays beside the
//!   place's music [documented: GECK wiki PlayMusic, "will not stop the currently playing
//!   music of a music marker"], once or looping by its `ANAM` sign; a type without a file
//!   (`1NoMusic`) stops it [hypothesis].
//! - **Cells and worldspaces** name a music type (MUSC); it plays when no marker reaches
//!   the player and its file exists [design]. The base game's cells name types whose
//!   folders it does not ship, so nothing is heard from them [observed].
//! - **Files** are MP3, decoded by rodio (symphonia) as they play; a set loops by
//!   starting its files again. Intro, outro and incidental sounds are SOUN records,
//!   played by `crate::sfx` at the listener.
//! - **The player's death** ([`MusicCues::dead`]): what plays fades out, a battle ends
//!   without its outro, and the SOUN `MUSDeath` plays once at the listener (FalloutNV.esm
//!   0x00078595, `fx\mus\mus_death.wav`, flagged 2D; the archives hold it as `.ogg`)
//!   [observed: the record and its file; hypothesis: that the death plays it, by its
//!   name]. No music plays while the player is dead; alive again, the place's music
//!   comes back [design].
//! - Music goes on in menus and behind loading screens [design].
//! - Not built: the main menu's music (no main menu yet; `SMainMenuMusicTrack`), the
//!   controllers' retrigger delay and `NAM1` (not understood), `IsMusicPlaying`-style
//!   queries.
//!
//! [`MusicSettings`] turns it off (`nvforever --no-music`) and sets the volume
//! (`--music-volume`). One log line per set started, per change of layers, per battle,
//! per script request and per incidental phrase; a `music summary` line at the end.

use crate::hitvol::HitVolumes;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sfx::PlaySound;
use crate::sim::SimState;
use crate::{Location, PlayState, PlayWorld};
use bevy::audio::{AudioSinkPlayback, ChannelCount, Decodable, Sample, SampleRate, Source};
use crate::audio_out::{AddAudioSource, SoundCategory};
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use fnv_sim::FormId;
use fnv_world::audio::{self, AudioForms, AudioMarker, MediaSetKind, SoundMarker};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How music behaves; insert before the play plugins to change the defaults.
#[derive(Resource, Clone, Copy, Debug)]
pub struct MusicSettings {
    pub play: bool,
    /// Linear volume, 1 = as recorded less the records' decibels.
    pub volume: f32,
}

impl Default for MusicSettings {
    fn default() -> Self {
        MusicSettings { play: true, volume: 1.0 }
    }
}

/// A space sound does not cross: a worldspace or an interior cell (FormID).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    World(u32),
    Interior(u32),
}

impl Space {
    pub fn of(loc: &Location) -> Space {
        loc.interior.map_or(Space::World(loc.world), Space::Interior)
    }
}

/// The music and acoustic-space forms and every audio marker and sound marker of the
/// load order, by the space they are in. Read once at the start.
#[derive(Resource, Default)]
pub struct AudioPlaces {
    pub forms: AudioForms,
    pub audio: HashMap<Space, Vec<AudioMarker>>,
    pub sounds: HashMap<Space, Vec<SoundMarker>>,
}

/// The cell the player is in: the interior, or the exterior cell under `eye`.
pub fn cell_at(d: &PlayWorld, loc: &Location, eye: [f32; 3]) -> Option<u32> {
    loc.interior.or_else(|| d.index.worldspaces.get(&loc.world)?.grid.get(&fnv_world::cell_of(eye[0], eye[1])).copied())
}

/// The game's hour of the day.
pub fn game_hour(sim: &SimState) -> f32 {
    sim.world.calendar().map_or(12.0, |c| c.hour).rem_euclid(24.0)
}

type Stem = rodio::Decoder<Cursor<Arc<[u8]>>>;

/// A panic opening a broken file (a WAV of rate 0) is that file's failure, not the game's
/// (nvf_assets::guard); one of a rate or channels the mixer cannot take is not opened
/// (crate::sfx::format_ok).
fn open(bytes: &Arc<[u8]>) -> Option<Stem> {
    nvf_assets::guard::guarded(|| Ok(rodio::Decoder::builder().with_byte_len(bytes.len() as u64).with_data(Cursor::new(bytes.clone())).build().ok()))
        .ok()
        .flatten()
        .filter(|d| crate::sfx::format_ok(d.channels().get(), d.sample_rate().get()).is_ok())
}

/// Up to three music files of one format played in step, each at its own level, which
/// the game changes while they play.
#[derive(Asset, TypePath, Clone)]
pub struct LayeredMusic {
    /// The files, each with the index of its level.
    stems: Vec<(usize, Arc<[u8]>)>,
    /// Linear gains as `f32` bits.
    levels: Arc<[AtomicU32; 3]>,
    channels: ChannelCount,
    rate: SampleRate,
    /// Start again when the first file ends.
    looping: bool,
    /// Where the first playing starts, as a part of the first file's length (a SOUN's
    /// Start at Random Position, audio.md §15.2); later rounds start at the start.
    start: f32,
}

impl LayeredMusic {
    /// One file played as it is decoded, with a level the caller changes while it plays
    /// (the first of the three); `None` when it is not decoded.
    pub(crate) fn single(bytes: Arc<[u8]>, looping: bool) -> Option<(LayeredMusic, Arc<[AtomicU32; 3]>)> {
        let d = open(&bytes)?;
        let levels: Arc<[AtomicU32; 3]> = Arc::new([AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)]);
        Some((LayeredMusic { stems: vec![(0, bytes)], levels: levels.clone(), channels: d.channels(), rate: d.sample_rate(), looping, start: 0.0 }, levels))
    }

    /// Start the first playing `part` (0 to 1) of the way into the first file.
    pub(crate) fn starting_at(self, part: f32) -> LayeredMusic {
        LayeredMusic { start: part.clamp(0.0, 1.0), ..self }
    }
}

pub struct LayeredDecoder {
    music: LayeredMusic,
    stems: Vec<(usize, Option<Stem>)>,
}

impl LayeredDecoder {
    /// Open every file; one that no longer opens, or whose format differs from the
    /// first's, stays silent.
    fn open(music: &LayeredMusic) -> Vec<(usize, Option<Stem>)> {
        music.stems.iter().map(|(k, b)| (*k, open(b).filter(|d| d.channels() == music.channels && d.sample_rate() == music.rate))).collect()
    }
}

impl Iterator for LayeredDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let lead = |stems: &mut Vec<(usize, Option<Stem>)>| stems.first_mut().and_then(|(_, s)| s.as_mut()).and_then(Iterator::next);
        let first = match lead(&mut self.stems) {
            Some(s) => s,
            None if self.music.looping => {
                self.stems = LayeredDecoder::open(&self.music);
                lead(&mut self.stems)?
            }
            None => return None,
        };
        let level = |k: usize| f32::from_bits(self.music.levels[k.min(2)].load(Ordering::Relaxed));
        let mut out = first * level(self.stems[0].0);
        for (k, stem) in self.stems.iter_mut().skip(1) {
            if let Some(s) = stem.as_mut().and_then(Iterator::next) {
                out += s * f32::from_bits(self.music.levels[(*k).min(2)].load(Ordering::Relaxed));
            }
        }
        Some(out)
    }
}

impl Source for LayeredDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.music.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.music.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for LayeredMusic {
    type Decoder = LayeredDecoder;

    fn decoder(&self) -> LayeredDecoder {
        let mut stems = LayeredDecoder::open(self);
        // A file whose length is not known starts at its start [design].
        let length = stems.first().and_then(|(_, s)| s.as_ref()).and_then(Source::total_duration);
        if let Some(at) = length.filter(|_| self.start > 0.0).map(|l| l.mul_f32(self.start)) {
            for stem in stems.iter_mut().filter_map(|(_, s)| s.as_mut()) {
                let _ = stem.try_seek(at);
            }
        }
        LayeredDecoder { music: self.clone(), stems }
    }
}

/// What is to be heard: a location or dungeon set by day or by night, a battle set's
/// loop, or a music type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Set { set: u32, night: bool },
    Battle(u32),
    Type(u32),
}

/// The music wanted at a place and hour.
#[derive(Clone, Debug, PartialEq)]
pub struct Wanted {
    pub key: Key,
    pub name: String,
    /// Files as the records store them, relative to `music\`.
    pub files: [Option<String>; 3],
    pub db: [f32; 3],
    /// Which of the three sound here.
    pub on: [bool; 3],
    /// Seconds a layer takes to fade in or out.
    pub fade: f32,
    /// Seconds a layer stays once it came in.
    pub min_on: f32,
    pub looping: bool,
    /// The layers' names for the log.
    pub layers: [&'static str; 3],
}

/// Seconds a music type fades in and out over, and a set without a crossfade time
/// [design].
const DEFAULT_FADE: f32 = 3.0;
/// Seconds music a script asks for takes to come in [design: at once, almost].
const SCRIPT_FADE: f32 = 0.25;
/// The names of the three layers of a dungeon set, quietest first, of a location set,
/// and of one track.
const DUNGEON_LAYERS: [&str; 3] = ["explore", "suspense", "battle"];
const LOCATION_LAYERS: [&str; 3] = ["outer", "middle", "inner"];
const ONE_LAYER: [&str; 3] = ["loop", "-", "-"];
/// Seconds between two looks at the actors near the player [design].
const CUE_INTERVAL: f32 = 0.25;
/// The sound the player's death plays (see the module notes) when the DOBJ's Death Music
/// (place 11, a MUSC) has no file to play. Found by Editor ID on purpose: no DOBJ place
/// names a death SOUN (default-objects.md §1), so there is no default object to read here.
const DEATH_SOUND: &str = "MUSDeath";
/// The game settings battle music reads, with the values FalloutNV.esm stores
/// [observed], used when a setting is missing.
const STOP_TIME: (&str, f32) = ("fCombatMusicStopTime", 3.0);
const NEAR_COMBAT: (&str, f32) = ("fCombatMusicNearCombatOuterRadius", 2000.0);
/// Seconds per unit of an incidental set's times [hypothesis: minutes; the GECK wiki
/// says seconds, which would leave pauses shorter than the phrases, audio.md §13.4].
pub const INCIDENTAL_UNIT: f32 = 60.0;
/// Day for incidental sets, which have no controller: the hours every controller of
/// FalloutNV.esm stores, 06:00 to 23:54 [observed].
const INCIDENTAL_DAY: (f32, f32) = (6.0, 23.906_25);

/// The tracks of a location or dungeon `set` for a listener `distance` units from
/// `marker`. A dungeon set plays its explore track, with the suspense track when
/// `alert` and the battle track when `battle`.
pub fn wanted_set(set: &audio::MediaSet, marker: &AudioMarker, distance: f32, day: bool, alert: bool, battle: bool) -> Wanted {
    let min_on = set.times[0].max(0.0);
    let fade = if set.times[2] > 0.0 { set.times[2] } else { DEFAULT_FADE };
    if set.kind == MediaSetKind::Dungeon {
        let tracks = audio::dungeon_tracks(set);
        return Wanted {
            key: Key::Set { set: set.form_id, night: false },
            name: set.edid.clone().unwrap_or_else(|| format!("{:08X}", set.form_id)),
            files: tracks.map(|t| t.map(str::to_string)),
            db: [set.db[1], set.db[2], set.db[0]],
            on: [true, alert || battle, battle],
            fade,
            min_on,
            looping: true,
            layers: DUNGEON_LAYERS,
        };
    }
    let l = audio::layers(set, marker, distance, day);
    let base = if day { 0 } else { 3 };
    Wanted {
        key: Key::Set { set: set.form_id, night: !day },
        name: format!("{} {}", set.edid.as_deref().unwrap_or("?"), if day { "day" } else { "night" }),
        files: [0, 1, 2].map(|k| set.track(base + k).map(str::to_string)),
        db: [0, 1, 2].map(|k| set.db[base + k]),
        on: l.on,
        fade,
        min_on,
        looping: true,
        layers: LOCATION_LAYERS,
    }
}

/// A battle set's loop (`NAM2`), faded over its loop fade-out time (`ENAM`) [documented:
/// GECK wiki "Loop Fade Out Time"]; the enable flags are a location set's and are not
/// read [observed: battle sets store values above 63 there].
pub fn wanted_battle(set: &audio::MediaSet) -> Wanted {
    Wanted {
        key: Key::Battle(set.form_id),
        name: set.edid.clone().unwrap_or_else(|| format!("{:08X}", set.form_id)),
        files: [set.tracks[0].clone(), None, None],
        db: [set.db[0], 0.0, 0.0],
        on: [true, false, false],
        fade: if set.times[1] > 0.0 { set.times[1] } else { DEFAULT_FADE },
        min_on: 0.0,
        looping: true,
        layers: ONE_LAYER,
    }
}

/// A music type as one track [hypothesis: `ANAM` is a level in decibels whose sign says
/// whether the file loops; its size is taken off].
pub fn wanted_type(m: &audio::MusicType) -> Option<Wanted> {
    let file = m.file.clone()?;
    Some(Wanted {
        key: Key::Type(m.form_id),
        name: m.edid.clone().unwrap_or_else(|| format!("{:08X}", m.form_id)),
        files: [Some(file), None, None],
        db: [-m.db.abs(), 0.0, 0.0],
        on: [true, false, false],
        fade: DEFAULT_FADE,
        min_on: 0.0,
        looping: m.db > 0.0,
        layers: ONE_LAYER,
    })
}

/// One step of a fade: `level` moved towards `target` by `dt` seconds of a fade that
/// takes `fade` seconds from silence to full.
pub fn fade_step(level: f32, target: f32, dt: f32, fade: f32) -> f32 {
    let step = if fade > 0.0 { dt / fade } else { 1.0 };
    level + (target - level).clamp(-step, step)
}

/// The layers that sound: those wanted, and those still within their minimum time on
/// (`on_since`: when each came in, `now` the time).
pub fn hold_layers(wanted: [bool; 3], playing: [bool; 3], on_since: [f32; 3], min_on: f32, now: f32) -> [bool; 3] {
    [0, 1, 2].map(|k| wanted[k] || (playing[k] && now - on_since[k] < min_on))
}

/// Day for a set without a controller (an incidental set).
fn incidental_day(hour: f32) -> bool {
    hour >= INCIDENTAL_DAY.0 && hour < INCIDENTAL_DAY.1
}

/// The wait before an incidental phrase, seconds: `r` (0–1) of the way between the
/// set's two times for the day or the night [documented: xEdit and GECK wiki, `DNAM`
/// Daytime Min, `FNAM` Daytime Max, `ENAM` Nighttime Min, `GNAM` Nighttime Max], in
/// [`INCIDENTAL_UNIT`]s.
pub fn incidental_wait(set: &audio::MediaSet, day: bool, r: f32) -> f32 {
    let (lo, hi) = if day { (set.times[0], set.times[2]) } else { (set.times[1], set.times[3]) };
    let (lo, hi) = (lo.min(hi).max(0.0), lo.max(hi).max(0.0));
    (lo + (hi - lo) * r) * INCIDENTAL_UNIT
}

/// Music being played.
struct Track {
    key: Key,
    name: String,
    /// `None`: no file of it could be played.
    entity: Option<Entity>,
    levels: Arc<[AtomicU32; 3]>,
    /// How far each layer is faded in, 0–1.
    level: [f32; 3],
    target: [f32; 3],
    /// Linear gain of each layer when fully in.
    gain: [f32; 3],
    fade: f32,
    min_on: f32,
    /// When each layer last came in (real seconds).
    on_since: [f32; 3],
    layer_names: [&'static str; 3],
}

#[derive(Component)]
struct MusicSound;

/// A battle and the music it brought.
#[derive(Default)]
struct Battle {
    active: bool,
    /// When it began, when the last threat was seen, when it ended (real seconds).
    since: f32,
    last: f32,
    ended: Option<f32>,
    /// The battle set playing it; `None` when the place's dungeon set plays it, or no
    /// set is named here.
    set: Option<u32>,
    /// The set whose intro and outro sound (the battle set, or the dungeon set).
    stinger_set: Option<u32>,
    intro_at: Option<f32>,
}

#[derive(Resource)]
pub struct Music {
    /// File bytes by archive path; `None`: not there.
    pub files: crate::file_bytes::FileBytes,
    folders: HashMap<String, Vec<String>>,
    /// CELL `XCMO` and WRLD `ZNAM` by FormID.
    types: HashMap<u32, Option<u32>>,
    /// The region whose music a cell takes (through its acoustic space, else its own
    /// region list), by cell.
    regions: HashMap<u32, Option<u32>>,
    current: Option<Track>,
    leaving: Vec<Track>,
    /// The music a script asked for.
    script: Option<Track>,
    /// The marker the player is in, the faction's standing towards the player there and
    /// the set picked for it.
    marker: Option<(u32, Option<i64>, Option<u32>)>,
    battle: Battle,
    /// The player is dead: no music plays.
    dead: bool,
    /// When the next incidental phrase is due, and the region it is for.
    incidental: Option<(u32, f32)>,
    reported: HashSet<String>,
    rng: SplitMix,
    pub started: u32,
    pub layer_changes: u32,
    pub failed: u32,
    pub battles: u32,
    pub battle_sets: u32,
    pub script_requests: u32,
    pub incidentals: u32,
    pub stingers: u32,
    pub deaths: u32,
    /// Level-up music started ([`MusicCues::level_up`]).
    pub level_ups: u32,
    /// SetMusicState 1 holds the music where it is until SetMusicState 2 (or 0)
    /// [documented: SetMusicState].
    paused: bool,
}

impl Default for Music {
    fn default() -> Self {
        Music {
            files: Default::default(),
            folders: HashMap::new(),
            types: HashMap::new(),
            regions: HashMap::new(),
            current: None,
            leaving: Vec::new(),
            script: None,
            marker: None,
            battle: Battle::default(),
            dead: false,
            incidental: None,
            reported: HashSet::new(),
            rng: SplitMix(0x6d75_7369),
            started: 0,
            layer_changes: 0,
            failed: 0,
            battles: 0,
            battle_sets: 0,
            script_requests: 0,
            incidentals: 0,
            stingers: 0,
            deaths: 0,
            level_ups: 0,
            paused: false,
        }
    }
}

/// What the game tells the music, measured by [`cues`]: whether a hostile actor near the
/// player has it detected (a battle) or noticed, how each controller's faction stands
/// towards the player, and the music types scripts asked for.
#[derive(Resource, Default)]
pub struct MusicCues {
    pub combat: bool,
    pub alert: bool,
    /// The hostile actor that made the battle, for the log.
    pub by: Option<u32>,
    /// The player is dead (`Game::is_dead`).
    pub dead: bool,
    /// GetFactionRelation of each conditional faction towards the player.
    pub relations: HashMap<u32, i64>,
    /// Some music plays (the place's or a script's), for IsMusicPlaying.
    pub playing: bool,
    /// A level was just earned (`crate::xp_meter`, as its level-up sound plays): the
    /// default objects' Level Up Music plays once, if its file is there (#991).
    pub level_up: bool,
    requests: Vec<FormId>,
    /// SetMusicState's requests (`fnv_game::nvse_ttw`): 0 stop, 1 pause, 2 resume.
    states: Vec<u8>,
    next: f32,
}

fn init(mut commands: Commands, d: Res<PlayWorld>) {
    let d = d.0;
    crate::start_read::start(&mut commands, "nvf-audio-read", move || read_places(d));
}

/// [`AudioPlaces`] from the plugins, on its own thread (`crate::start_read`).
fn read_places(d: crate::PlayData) -> AudioPlaces {
    let t = std::time::Instant::now();
    let forms = AudioForms::build(d.lo, d.schema);
    let (am, sm) = audio::markers(d.lo, d.schema, d.index, d.index.cells.keys().copied());
    let space = |cell: u32| d.index.cells.get(&cell).and_then(|c| c.world).map_or(Space::Interior(cell), Space::World);
    let mut places = AudioPlaces { forms, ..Default::default() };
    let (n_audio, n_sounds) = (am.len(), sm.len());
    for m in am {
        places.audio.entry(space(m.cell)).or_default().push(m);
    }
    for m in sm {
        places.sounds.entry(space(m.cell)).or_default().push(m);
    }
    info!(
        "music: {} media sets, {} controllers, {} acoustic spaces, {} regions with music, {n_audio} audio markers and {n_sounds} sound markers read in {:.0} ms",
        places.forms.sets.len(),
        places.forms.controllers.len(),
        places.forms.spaces.len(),
        places.forms.regions.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    places
}

fn setting(sim: &SimState, (name, default): (&str, f32)) -> f32 {
    match sim.forms.settings.get(&name.to_ascii_lowercase()) {
        Some(fnv_sim::settings::Setting::Float(v)) => *v,
        _ => default,
    }
}

/// Take the music scripts asked for; a few times a second, look at the actors near the
/// player and at how the controllers' factions stand towards the player.
#[allow(clippy::too_many_arguments)]
fn cues(
    time: Res<Time<Real>>,
    mut cues: ResMut<MusicCues>,
    places: Option<Res<AudioPlaces>>,
    script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    actors: Option<Res<HitVolumes>>,
    player: Option<Res<Player>>,
    state: Res<State<PlayState>>,
) {
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    cues.requests.append(&mut rt.game.music_requests);
    cues.states.append(&mut rt.game.take_music_states());
    rt.game.gaps.music_any = cues.playing;
    cues.dead = rt.game.is_dead(FormId::PLAYER_REF);
    let now = time.elapsed_secs();
    if now < cues.next || *state.get() != PlayState::Playing {
        return;
    }
    cues.next = now + CUE_INTERVAL;
    let (Some(places), Some(player)) = (places, player) else { return };
    let reach = setting(&sim, NEAR_COMBAT);
    let feet = player.feet;
    let near: Vec<u32> = actors.map_or(Vec::new(), |a| {
        a.living.iter().filter(|(_, p)| (0..3).map(|k| (p[k] - feet[k]).powi(2)).sum::<f32>() <= reach * reach).map(|(id, _)| *id).collect()
    });
    let factions: Vec<u32> = places.forms.controllers.values().filter_map(|c| c.faction).collect();
    let cues = &mut *cues;
    with_world(rt, &mut sim, |rt| {
        let g = &mut rt.game;
        let (mut combat, mut alert, mut by) = (None, false, None);
        for &id in &near {
            let Some(band) = g.detection.get(FormId(id), FormId::PLAYER_REF).map(|d| d.band) else { continue };
            if band == fnv_game::detection::Band::Hidden || !fnv_game::rest::hostile_to_player(g, FormId(id)) {
                continue;
            }
            if band == fnv_game::detection::Band::Detected {
                combat = combat.or(Some(id));
            } else {
                alert = true;
                by = by.or(Some(id));
            }
        }
        cues.combat = combat.is_some();
        cues.alert = alert || combat.is_some();
        cues.by = combat.or(by);
        for f in factions {
            cues.relations.insert(f, fnv_game::functions::factions_relation(g, &[FormId(f)], FormId::PLAYER_REF));
        }
    });
}

impl Music {
    /// The archive path of a music file as a record stores it; a folder stands for one
    /// of the `.mp3` files in it [hypothesis].
    fn path(&mut self, vfs: &fnv_vfs::Vfs, file: &str) -> Option<String> {
        let path = fnv_vfs::normalize(&format!("music\\{}", file.trim_start_matches(['\\', '/'])));
        if !path.ends_with('\\') {
            return Some(path);
        }
        let files = self.folders.entry(path.clone()).or_insert_with(|| vfs.paths(|p| p.starts_with(path.as_str()) && p.ends_with(".mp3") && !p[path.len()..].contains('\\')));
        if files.is_empty() {
            return None;
        }
        let k = (self.rng.next_f32() * files.len() as f32) as usize;
        Some(files[k.min(files.len() - 1)].clone())
    }

    fn bytes(&mut self, vfs: &fnv_vfs::Vfs, file: &str) -> Option<Arc<[u8]>> {
        let Some(path) = self.path(vfs, file) else {
            if self.reported.insert(file.to_string()) {
                warn!("music: no files in music\\{file}");
            }
            return None;
        };
        self.files.get_or_read(&path, || match vfs.read(&path) {
            Some(Ok(b)) => Some(Arc::<[u8]>::from(b)),
            Some(Err(e)) => {
                warn!("music: {path}: {e}");
                None
            }
            None => {
                warn!("music: {path}: not in the game's files");
                None
            }
        })
    }

    /// Start what is wanted, silent, with its layers fading in.
    fn start(&mut self, commands: &mut Commands, vfs: &fnv_vfs::Vfs, assets: &mut Assets<LayeredMusic>, w: &Wanted, now: f32) -> Track {
        let levels: Arc<[AtomicU32; 3]> = Arc::new([AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)]);
        let mut stems: Vec<(usize, Arc<[u8]>)> = Vec::new();
        let mut format: Option<(ChannelCount, SampleRate)> = None;
        for (k, file) in w.files.iter().enumerate() {
            let Some(file) = file else { continue };
            let Some(bytes) = self.bytes(vfs, file) else {
                self.failed += 1;
                continue;
            };
            match (open(&bytes).map(|d| (d.channels(), d.sample_rate())), format) {
                (Some(f), None) => format = Some(f),
                (Some(f), Some(first)) if f == first => {}
                (f, _) => {
                    self.failed += 1;
                    if self.reported.insert(file.clone()) {
                        warn!("music: {file}: {}", if f.is_some() { "another format than the set's first file" } else { "not decoded" });
                    }
                    continue;
                }
            }
            stems.push((k, bytes));
        }
        let entity = format.map(|(channels, rate)| {
            let music = assets.add(LayeredMusic { stems, levels: levels.clone(), channels, rate, looping: w.looping, start: 0.0 });
            commands.spawn((AudioPlayer::<LayeredMusic>(music), PlaybackSettings::DESPAWN, MusicSound, SoundCategory::Music)).id()
        });
        self.started += 1;
        let layers: Vec<&str> = (0..3).filter(|&k| w.on[k] && w.files[k].is_some()).map(|k| w.layers[k]).collect();
        match entity {
            Some(_) => info!("music: {} started, layers {}", w.name, layers.join("+")),
            None => info!("music: {}: none of its files can be played", w.name),
        }
        Track {
            key: w.key,
            name: w.name.clone(),
            entity,
            levels,
            level: [0.0; 3],
            target: w.on.map(|on| on as u8 as f32),
            gain: w.db.map(|db| 10f32.powf(db / 20.0)),
            fade: w.fade,
            min_on: w.min_on,
            on_since: [now; 3],
            layer_names: w.layers,
        }
    }

    /// Pick one of `sets` at random.
    fn pick(&mut self, sets: &[u32]) -> Option<u32> {
        (!sets.is_empty()).then(|| sets[((self.rng.next_f32() * sets.len() as f32) as usize).min(sets.len() - 1)])
    }
}

/// The region whose music `cell` takes: the one its acoustic space names (`RDAT`, which
/// xEdit calls "use sound from region (interiors only)"; exterior cells name it too
/// [observed]), else the first of its own regions (`XCLR`) naming music [design].
fn region_of(d: &PlayWorld, places: &AudioPlaces, cell: u32) -> Option<u32> {
    let with_music = |r: &u32| places.forms.regions.contains_key(r);
    let a = audio::cell_audio(d.lo, d.schema, cell);
    a.acoustic_space.and_then(|s| places.forms.spaces.get(&s)?.region).filter(with_music).or_else(|| audio::cell_regions(d.lo, d.schema, cell).into_iter().find(with_music))
}

/// Play a SOUN of a media set (an intro, an outro, an incidental phrase) at the listener.
fn stinger(sounds: &mut MessageWriter<PlaySound>, sound: Option<u32>, what: &str, set: &str) -> bool {
    match sound {
        Some(s) => {
            info!("music: {what} of {set}: {}", FormId(s));
            sounds.write(PlaySound { sound: FormId(s), at: None });
            true
        }
        None => false,
    }
}

/// The world the music reads: where the player is, the clock, the game files.
type Where<'a> = (Res<'a, PlayWorld>, Res<'a, Location>, Res<'a, SimState>, Option<Res<'a, Player>>, Option<Res<'a, nvf_assets::GameData>>);

#[allow(clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    time: Res<Time<Real>>,
    settings: Res<MusicSettings>,
    music: Option<ResMut<Music>>,
    places: Option<Res<AudioPlaces>>,
    (d, loc, sim, player, data): Where,
    mut cues: ResMut<MusicCues>,
    mut sounds: MessageWriter<PlaySound>,
    state: Res<State<PlayState>>,
    mut assets: ResMut<Assets<LayeredMusic>>,
    (main_menu, intro): (Option<Res<crate::main_menu::MainMenu>>, Option<Res<crate::intro::Intro>>),
    sfx: Option<Res<crate::sfx::Sfx>>,
    poker: Option<Res<crate::poker::PokerScreen>>,
    sinks: Query<&AudioSink, With<MusicSound>>,
) {
    let (Some(mut music), Some(places), Some(player), Some(data)) = (music, places, player, data) else { return };
    let music = &mut *music;
    cues.playing = music.current.is_some() || music.script.is_some();
    let now = time.elapsed_secs();
    let eye = player.eye();
    let hour = game_hour(&sim);
    // The place's music is silent under the main menu, which plays its own theme, under
    // the intro film, which has its own sound, under a scripted film that pauses it
    // (PlayBink), and at the poker table, which plays its
    // casino's station (`crate::poker`).
    let front = main_menu.is_some_and(|m| m.open) || intro.is_some_and(|i| i.silences_music()) || poker.is_some_and(|p| p.0.is_some());
    let audible = settings.play && settings.volume > 0.0 && !front;

    // SetMusicState [documented: SetMusicState]: 1 pauses what plays, 2 resumes it; 0
    // stops the music a script played (PlayMusic), otherwise the place's music starts
    // again from its beginning. Stopping also ends a pause [hypothesis].
    for s in std::mem::take(&mut cues.states) {
        match s {
            1 => music.paused = true,
            2 => music.paused = false,
            _ => {
                music.paused = false;
                if let Some(mut old) = music.script.take() {
                    info!("music: SetMusicState 0 stops {}", old.name);
                    old.target = [0.0; 3];
                    old.fade = 0.0;
                    music.leaving.push(old);
                } else if let Some(mut old) = music.current.take() {
                    info!("music: SetMusicState 0 starts {} again", old.name);
                    old.target = [0.0; 3];
                    old.fade = 0.0;
                    music.leaving.push(old);
                }
            }
        }
        info!("music: SetMusicState {s}{}", if music.paused { ", paused" } else { "" });
    }
    for sink in &sinks {
        match (music.paused, sink.is_paused()) {
            (true, false) => sink.pause(),
            (false, true) => sink.play(),
            _ => {}
        }
    }

    // Music scripts asked for: beside the place's music.
    for m in std::mem::take(&mut cues.requests) {
        music.script_requests += 1;
        let ty = places.forms.music.get(&m.0);
        let name = ty.and_then(|t| t.edid.clone()).unwrap_or_else(|| m.to_string());
        match ty.and_then(wanted_type) {
            Some(mut w) => {
                info!("music: a script plays {name} ({})", if w.looping { "looping" } else { "once" });
                w.fade = SCRIPT_FADE;
                if let Some(mut old) = music.script.take() {
                    old.target = [0.0; 3];
                    music.leaving.push(old);
                }
                let t = music.start(&mut commands, &data.vfs, &mut assets, &w, now);
                music.script = Some(t);
            }
            None => {
                info!("music: a script asks for {name}, which names no file: the script's music stops");
                if let Some(mut old) = music.script.take() {
                    old.target = [0.0; 3];
                    old.fade = DEFAULT_FADE;
                    music.leaving.push(old);
                }
            }
        }
    }

    // The marker the player is in, and the set picked when it was entered (or when the
    // faction's standing changed).
    let markers = places.audio.get(&Space::of(&loc)).map_or(&[][..], Vec::as_slice);
    let at = audio::marker_at(markers, eye, |m| sim.world.is_enabled(fnv_sim::FormId(m.ref_id)).unwrap_or(!m.initially_disabled));
    let controller = at.and_then(|(m, _)| places.forms.controllers.get(&m.controller?));
    // A faction not yet measured counts as neutral.
    let relation = controller.and_then(|c| c.faction.map(|f| cues.relations.get(&f).copied().unwrap_or(0)));
    if music.marker.map(|m| (m.0, m.1)) != at.map(|(m, _)| (m.ref_id, relation)) {
        let same_marker = music.marker.map(|m| m.0) == at.map(|(m, _)| m.ref_id);
        music.marker = at.map(|(m, _)| {
            let sets = controller.map_or(&[][..], |c| c.sets_for(relation));
            (m.ref_id, relation, music.pick(sets))
        });
        let standing = |r: Option<i64>| match r {
            Some(1) => "enemy",
            Some(2) => "ally",
            Some(3) => "friend",
            _ => "neutral",
        };
        match (at, controller) {
            (Some(_), Some(c)) if same_marker => info!("music: {} now stands {} towards the player", c.edid.as_deref().unwrap_or("?"), standing(relation)),
            (Some((m, distance)), Some(c)) => info!(
                "music: in marker {:08X} of {}, {distance:.0} of {:.0} units from it, {} sets to choose from{}",
                m.ref_id,
                c.edid.as_deref().unwrap_or("?"),
                m.radius,
                c.sets_for(relation).len(),
                c.faction.map_or(String::new(), |f| format!(" (faction {} {})", FormId(f), standing(relation)))
            ),
            (Some((m, _)), None) => info!("music: in marker {:08X}, whose controller is not in the load order", m.ref_id),
            (None, _) => info!("music: outside every audio marker"),
        }
    }
    let place_set = music.marker.and_then(|m| places.forms.sets.get(&m.2?));
    let cell = cell_at(&d, &loc, eye);
    let region = cell.and_then(|c| *music.regions.entry(c).or_insert_with(|| region_of(&d, &places, c))).and_then(|r| places.forms.regions.get(&r));

    // The player's death: the music stops and the death sound plays, once per death.
    if cues.dead != music.dead {
        music.dead = cues.dead;
        if music.dead {
            if let Some(mut old) = music.script.take() {
                old.target = [0.0; 3];
                old.fade = DEFAULT_FADE;
                music.leaving.push(old);
            }
            let b = &mut music.battle;
            b.active = false;
            b.ended = None;
            b.intro_at = None;
            music.incidental = None;
            // The default objects' Death Music (DOBJ) when its file is there, as a script's
            // music plays (docs/spec-notes/default-objects.md); FalloutNV.esm's names
            // `special\death.mp3`, which is not shipped, so the SOUN below plays.
            let dobj = places.forms.default_objects.get(audio::DEFAULT_DEATH_MUSIC).copied().filter(|&m| m != 0).and_then(|m| places.forms.music.get(&m));
            if let Some((ty, mut w)) = dobj.and_then(|t| Some((t, wanted_type(t)?))).filter(|(_, w)| w.files.iter().flatten().any(|f| music.bytes(&data.vfs, f).is_some())) {
                info!("music: the player died: the music stops, the default objects' death music {} plays", ty.edid.as_deref().unwrap_or("?"));
                if audible {
                    w.fade = SCRIPT_FADE;
                    let t = music.start(&mut commands, &data.vfs, &mut assets, &w, now);
                    music.script = Some(t);
                    music.deaths += 1;
                }
            } else {
                let death = sfx.as_deref().and_then(|x| x.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(DEATH_SOUND))).map(|(&id, _)| id));
                match death {
                    Some(s) => {
                        info!("music: the player died: the music stops, {DEATH_SOUND} ({s}) plays{}", if audible { "" } else { " [silent: music is off]" });
                        if audible {
                            sounds.write(PlaySound { sound: s, at: None });
                            music.deaths += 1;
                        }
                    }
                    None => info!("music: the player died: the music stops; no {DEATH_SOUND} in the load order"),
                }
            }
        } else {
            info!("music: the player lives again: the place's music comes back");
        }
    }

    // A level earned: the default objects' Level Up Music (DOBJ place 13) as a script's
    // music plays, when its file is there [documented: the slot's name, xEdit DOBJ;
    // hypothesis: it plays as the level-up notice shows]. FalloutNV.esm's names
    // `DefaultSuccess`, whose `special\success.mp3` is not shipped, so with the shipped
    // data nothing plays and the level-up sound alone is heard (default-objects.md).
    if std::mem::take(&mut cues.level_up) && !music.dead {
        let dobj = places.forms.default_objects.get(audio::DEFAULT_LEVEL_UP_MUSIC).copied().filter(|&m| m != 0).and_then(|m| places.forms.music.get(&m));
        match dobj.and_then(|t| Some((t, wanted_type(t)?))).filter(|(_, w)| w.files.iter().flatten().any(|f| music.bytes(&data.vfs, f).is_some())) {
            Some((ty, mut w)) => {
                info!("music: level up: the default objects' level-up music {} plays{}", ty.edid.as_deref().unwrap_or("?"), if audible { "" } else { " [silent: music is off]" });
                if audible {
                    if let Some(mut old) = music.script.take() {
                        old.target = [0.0; 3];
                        old.fade = DEFAULT_FADE;
                        music.leaving.push(old);
                    }
                    w.fade = SCRIPT_FADE;
                    let t = music.start(&mut commands, &data.vfs, &mut assets, &w, now);
                    music.script = Some(t);
                    music.level_ups += 1;
                }
            }
            None => info!("music: level up: the default objects' level-up music {} has no file to play", dobj.and_then(|t| t.edid.as_deref()).unwrap_or("(none)")),
        }
    }

    // Battle.
    let in_game = *state.get() == PlayState::Playing;
    if cues.combat && in_game && !music.dead {
        music.battle.last = now;
        if !music.battle.active {
            let dungeon = place_set.filter(|s| s.kind == MediaSetKind::Dungeon);
            let list = controller.map(|c| &c.battle[..]).filter(|l| !l.is_empty()).or(region.map(|r| &r.battle[..])).unwrap_or(&[]);
            let set = if dungeon.is_some() { None } else { music.pick(list) };
            let b = &mut music.battle;
            b.active = true;
            b.since = now;
            b.ended = None;
            b.set = set;
            b.stinger_set = set.or(dungeon.map(|s| s.form_id));
            let wait = set.and_then(|s| places.forms.sets.get(&s)).map_or(0.0, |s| s.times[0].max(0.0));
            b.intro_at = Some(now + wait);
            music.battles += 1;
            let by = cues.by.map_or(String::new(), |a| format!(" ({} has the player detected)", FormId(a)));
            match (set.and_then(|s| places.forms.sets.get(&s)), dungeon) {
                (Some(s), _) => info!("music: battle begins{by}: {} after {wait:.1} s, from {} battle sets", s.edid.as_deref().unwrap_or("?"), list.len()),
                (None, Some(s)) => info!("music: battle begins{by}: the battle layer of {}", s.edid.as_deref().unwrap_or("?")),
                (None, None) => info!("music: battle begins{by}: no battle set here"),
            }
        }
    } else if music.battle.active && now - music.battle.last >= setting(&sim, STOP_TIME) {
        let b = &mut music.battle;
        b.active = false;
        b.ended = Some(now);
        b.intro_at = None;
        let s = b.stinger_set.and_then(|s| places.forms.sets.get(&s));
        info!("music: battle over, {:.1} s after it began", now - b.since);
        if audible && stinger(&mut sounds, s.and_then(|s| s.outro), "outro", s.and_then(|s| s.edid.as_deref()).unwrap_or("?")) {
            music.stingers += 1;
        }
    }
    if music.battle.intro_at.is_some_and(|t| now >= t) {
        music.battle.intro_at = None;
        let s = music.battle.stinger_set.and_then(|s| places.forms.sets.get(&s));
        if audible && stinger(&mut sounds, s.and_then(|s| s.intro), "intro", s.and_then(|s| s.edid.as_deref()).unwrap_or("?")) {
            music.stingers += 1;
        }
    }
    let battle_set = music.battle.set.and_then(|s| places.forms.sets.get(&s));
    let recovering = !music.battle.active && battle_set.is_some_and(|s| music.battle.ended.is_some_and(|e| now - e < s.times[2].max(0.0)));

    // What is wanted: the battle loop, silence while a battle set recovers, else the
    // place's music.
    let battle_loop = music.battle.active && battle_set.is_some_and(|s| now >= music.battle.since + s.times[0].max(0.0));
    let place = |music: &mut Music| -> Option<Wanted> {
        let from_marker = at.zip(controller).and_then(|((m, distance), c)| {
            let set = place_set?;
            Some(wanted_set(set, m, distance, c.is_day(hour), cues.alert && in_game, music.battle.active))
        });
        from_marker.or_else(|| {
            let of_cell = cell.and_then(|c| *music.types.entry(c).or_insert_with(|| audio::cell_audio(d.lo, d.schema, c).music));
            let of_world = || loc.interior.is_none().then(|| *music.types.entry(loc.world).or_insert_with(|| audio::world_music(d.lo, d.schema, loc.world))).flatten();
            wanted_type(places.forms.music.get(&of_cell.or_else(of_world)?)?)
        })
    };
    let wanted = if music.dead {
        None
    } else if battle_loop {
        battle_set.map(wanted_battle)
    } else if recovering {
        None
    } else {
        // Also during a battle set's wait before its loop.
        place(music)
    };

    // A change of music: what plays fades out, what is wanted starts. A battle loop
    // takes over within its own fade time.
    if music.current.as_ref().map(|t| t.key) != wanted.as_ref().map(|w| w.key) {
        if let Some(mut old) = music.current.take() {
            old.target = [0.0; 3];
            if let Some(w) = wanted.as_ref().filter(|w| matches!(w.key, Key::Battle(_))) {
                old.fade = old.fade.min(w.fade);
            }
            info!("music: {} fades out", old.name);
            music.leaving.push(old);
        }
        if let Some(w) = &wanted {
            if matches!(w.key, Key::Battle(_)) {
                music.battle_sets += 1;
            }
            let t = music.start(&mut commands, &data.vfs, &mut assets, w, now);
            music.current = Some(t);
        }
    }
    if let (Some(t), Some(w)) = (music.current.as_mut(), &wanted) {
        let playing = t.target.map(|x| x > 0.0);
        let on = hold_layers(w.on, playing, t.on_since, t.min_on, now);
        let target = on.map(|on| on as u8 as f32);
        if t.target != target {
            for k in 0..3 {
                if on[k] && !playing[k] {
                    t.on_since[k] = now;
                }
            }
            t.target = target;
            music.layer_changes += 1;
            let layers: Vec<&str> = (0..3).filter(|&k| on[k] && w.files[k].is_some()).map(|k| t.layer_names[k]).collect();
            info!("music: {} layers {}", t.name, if layers.is_empty() { "none".to_string() } else { layers.join("+") });
        }
    }

    // Incidental phrases where nothing plays.
    let silent = music.current.is_none() && !music.battle.active && !recovering && !music.dead;
    match region.and_then(|r| Some((r.form_id, places.forms.sets.get(&r.incidental?)?))).filter(|_| silent && in_game) {
        Some((r, set)) => {
            let day = incidental_day(hour);
            match music.incidental {
                Some((at_region, due)) if at_region == r => {
                    if now >= due {
                        let sound = if day { set.intro } else { set.outro };
                        if audible && stinger(&mut sounds, sound, "incidental phrase", set.edid.as_deref().unwrap_or("?")) {
                            music.incidentals += 1;
                        }
                        let wait = incidental_wait(set, day, music.rng.next_f32());
                        music.incidental = Some((r, now + wait));
                    }
                }
                _ => {
                    let wait = incidental_wait(set, day, music.rng.next_f32());
                    info!("music: no music here; {} plays a phrase in {wait:.0} s", set.edid.as_deref().unwrap_or("?"));
                    music.incidental = Some((r, now + wait));
                }
            }
        }
        None => music.incidental = None,
    }

    let dt = time.delta_secs();
    let volume = if settings.play && !front { settings.volume.max(0.0) } else { 0.0 };
    for t in music.current.iter_mut().chain(music.script.iter_mut()).chain(music.leaving.iter_mut()) {
        for k in 0..3 {
            t.level[k] = fade_step(t.level[k], t.target[k], dt, t.fade);
            t.levels[k].store((t.level[k] * t.gain[k] * volume).to_bits(), Ordering::Relaxed);
        }
    }
    music.leaving.retain(|t| {
        let over = t.level.iter().all(|l| *l <= 0.0);
        if let (true, Some(e)) = (over, t.entity) {
            commands.entity(e).try_despawn();
        }
        !over
    });
}

fn report(mut exit: MessageReader<AppExit>, music: Option<Res<Music>>) {
    if let (Some(_), Some(m)) = (exit.read().last(), music) {
        info!(
            "music summary: {} started, {} changes of layers, {} files not played, {} battles ({} battle loops started), {} intro and outro sounds, {} script requests, {} incidental phrases, {} death sounds, {} level-up music{}",
            m.started,
            m.layer_changes,
            m.failed,
            m.battles,
            m.battle_sets,
            m.stingers,
            m.script_requests,
            m.incidentals,
            m.deaths,
            m.level_ups,
            m.current.as_ref().map_or(String::new(), |t| format!("; playing {}", t.name))
        );
    }
}

pub struct MusicPlugin;

/// Let go of the music's files nothing plays ([`crate::file_bytes`]).
fn release_files(real: Res<Time<Real>>, limits: Option<Res<nvf_assets::KeepLimits>>, x: Option<ResMut<Music>>) {
    if let Some(mut x) = x {
        x.files.sweep(real.elapsed_secs_f64(), crate::file_bytes::limits(limits));
    }
}

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        crate::start_read::add_collect::<AudioPlaces>(app);
        app.init_resource::<MusicSettings>()
            .init_resource::<Music>()
            .init_resource::<MusicCues>()
            .add_audio_source::<LayeredMusic>()
            .add_systems(Startup, init)
            // Its stingers are sound effects: before sfx plays them (crate::sfx::SfxSet).
            .add_systems(Update, (cues.after(crate::PlaySet::Scripts), play.after(crate::PlaySet::Activation)).chain().before(crate::sfx::SfxSet))
            .add_systems(Last, (release_files, report));
        crate::settings::add_setting(
            app,
            crate::settings::Setting {
                key: "music",
                group: crate::pause::GROUP_MENUS,
                label: "Music",
                note: "On: the music of the place you are in plays. Off: it is silent and goes on unheard. (--no-music)",
                get: |w| w.resource::<MusicSettings>().play,
                set: |w, on| w.resource_mut::<MusicSettings>().play = on,
                slider: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_world::audio::{MediaSet, MusicType};

    fn marker(radius: f32) -> AudioMarker {
        AudioMarker { ref_id: 1, cell: 1, position: [0.0; 3], radius, name: None, controller: Some(2), use_controller_values: true, layer2_percent: 0.33, layer3_percent: 0.67, initially_disabled: false }
    }

    #[test]
    fn a_set_by_day_and_by_night() {
        let set = MediaSet {
            form_id: 7,
            edid: Some("musSet".into()),
            kind: MediaSetKind::Location,
            tracks: ["d1", "d2", "d3", "n1", "n2", "n3"].map(|s| Some(s.to_string())),
            db: [-6.0, -6.0, -6.0, -3.0, -3.0, -6.0],
            boundary_percent: [100.0, 60.0, 20.0, 100.0, 60.0, 20.0],
            // The night's inner track is not enabled.
            enabled: 0b011111,
            times: [6.0, 15.0, 9.0, 0.0],
            ..Default::default()
        };
        let w = wanted_set(&set, &marker(1000.0), 500.0, true, false, false);
        assert_eq!(w.key, Key::Set { set: 7, night: false });
        assert_eq!(w.files, ["d1", "d2", "d3"].map(|s| Some(s.to_string())));
        assert_eq!((w.on, w.fade, w.min_on, w.looping), ([true, true, false], 9.0, 6.0, true));
        let w = wanted_set(&set, &marker(1000.0), 100.0, false, false, false);
        assert_eq!(w.key, Key::Set { set: 7, night: true });
        assert_eq!(w.files, [Some("n1".to_string()), Some("n2".to_string()), None]);
        assert_eq!((w.db, w.on), ([-3.0, -3.0, -6.0], [true, true, true]));
        assert_eq!(w.name, "musSet night");
    }

    #[test]
    fn a_dungeon_set_by_threat() {
        // As musSetDNGN1ABC stores it [observed]: battle, explore, suspense.
        let set = MediaSet {
            form_id: 8,
            edid: Some("musSetDNGN".into()),
            kind: MediaSetKind::Dungeon,
            tracks: [Some("high".into()), Some("low".into()), Some("mid".into()), None, None, None],
            db: [-2.5, -4.0, -4.0, 0.0, 0.0, 0.0],
            enabled: 80,
            times: [6.0, 10.0, 6.0, 0.0],
            ..Default::default()
        };
        let w = wanted_set(&set, &marker(1000.0), 900.0, false, false, false);
        assert_eq!(w.files, [Some("low".to_string()), Some("mid".to_string()), Some("high".to_string())]);
        assert_eq!((w.on, w.db, w.key), ([true, false, false], [-4.0, -4.0, -2.5], Key::Set { set: 8, night: false }));
        assert_eq!(wanted_set(&set, &marker(1000.0), 900.0, true, true, false).on, [true, true, false]);
        assert_eq!(wanted_set(&set, &marker(1000.0), 900.0, true, false, true).on, [true, true, true]);
        assert_eq!(w.layers, DUNGEON_LAYERS);
    }

    #[test]
    fn a_battle_loop() {
        let set = MediaSet { form_id: 9, edid: Some("musSetBTTL".into()), kind: MediaSetKind::Battle, tracks: [Some("bttl\\a.mp3".into()), None, None, None, None, None], db: [-10.0, 0.0, 0.0, 0.0, 0.0, 0.0], enabled: 0, times: [1.0, 3.0, 10.0, 0.0], ..Default::default() };
        let w = wanted_battle(&set);
        assert_eq!((w.key, w.files[0].as_deref(), w.db[0], w.fade, w.on), (Key::Battle(9), Some("bttl\\a.mp3"), -10.0, 3.0, [true, false, false]));
    }

    #[test]
    fn layers_stay_their_minimum_time() {
        // The middle layer came in at 10 s; the set keeps a layer 6 s.
        assert_eq!(hold_layers([true, false, false], [true, true, false], [0.0, 10.0, 0.0], 6.0, 13.0), [true, true, false]);
        assert_eq!(hold_layers([true, false, false], [true, true, false], [0.0, 10.0, 0.0], 6.0, 16.5), [true, false, false]);
        // Coming in is never held back.
        assert_eq!(hold_layers([true, true, true], [true, false, false], [0.0; 3], 6.0, 1.0), [true, true, true]);
    }

    #[test]
    fn incidental_waits() {
        // musSetINCDesert [observed]: day 4 to 16, night 6 to 12.
        let set = MediaSet { kind: MediaSetKind::Incidental, times: [4.0, 6.0, 16.0, 12.0], ..Default::default() };
        assert_eq!(incidental_wait(&set, true, 0.0), 4.0 * INCIDENTAL_UNIT);
        assert_eq!(incidental_wait(&set, true, 1.0), 16.0 * INCIDENTAL_UNIT);
        assert_eq!(incidental_wait(&set, false, 0.5), 9.0 * INCIDENTAL_UNIT);
        assert!(incidental_day(12.0) && !incidental_day(3.0) && !incidental_day(23.95));
    }

    #[test]
    fn a_music_type_is_one_track() {
        let w = wanted_type(&MusicType { form_id: 9, edid: Some("musDoc".into()), file: Some("loc\\a.mp3".into()), db: 6.0 }).unwrap();
        assert_eq!((w.key, w.db[0], w.looping, w.on), (Key::Type(9), -6.0, true, [true, false, false]));
        // A negative level: played once.
        assert!(!wanted_type(&MusicType { form_id: 9, edid: None, file: Some("scr\\b.mp3".into()), db: -7.74 }).unwrap().looping);
        // No file ("1NoMusic"): nothing to play.
        assert!(wanted_type(&MusicType::default()).is_none());
    }

    #[test]
    fn fades_take_their_time() {
        // A 9 second fade, a tenth of a second at a time.
        let mut l = 0.0;
        for _ in 0..45 {
            l = fade_step(l, 1.0, 0.1, 9.0);
        }
        assert!((l - 0.5).abs() < 1e-4);
        for _ in 0..60 {
            l = fade_step(l, 1.0, 0.1, 9.0);
        }
        assert_eq!(l, 1.0);
        assert!((fade_step(1.0, 0.0, 0.5, 2.0) - 0.75).abs() < 1e-6);
        // No fade time: at once.
        assert_eq!(fade_step(0.2, 1.0, 0.016, 0.0), 1.0);
    }

    /// A WAV file of 16-bit samples, for the decoder.
    fn wav(samples: &[i16]) -> Arc<[u8]> {
        let mut v = b"RIFF".to_vec();
        v.extend((36 + samples.len() as u32 * 2).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(8000u32.to_le_bytes());
        v.extend(16000u32.to_le_bytes());
        v.extend(2u16.to_le_bytes());
        v.extend(16u16.to_le_bytes());
        v.extend(b"data");
        v.extend((samples.len() as u32 * 2).to_le_bytes());
        for s in samples {
            v.extend(s.to_le_bytes());
        }
        v.into()
    }

    #[test]
    fn layers_are_mixed_in_step_and_loop() {
        let (a, b) = (wav(&[8192, 16384, -8192, 0]), wav(&[16384, 16384, 16384, 16384]));
        let first = open(&a).expect("the test's WAV decodes");
        let levels: Arc<[AtomicU32; 3]> = Arc::new([AtomicU32::new(1f32.to_bits()), AtomicU32::new(0), AtomicU32::new(0.5f32.to_bits())]);
        let music = LayeredMusic { stems: vec![(0, a), (2, b)], levels: levels.clone(), channels: first.channels(), rate: first.sample_rate(), looping: true, start: 0.0 };
        let mut d = music.decoder();
        let got: Vec<f32> = (0..6).map(|_| d.next().unwrap()).collect();
        // The first file at full level and the third at half; then both from the start.
        for (g, want) in got.iter().zip([0.5, 0.75, 0.0, 0.25, 0.5, 0.75]) {
            assert!((g - want).abs() < 1e-3, "{got:?}");
        }
        // A level changed while it plays.
        levels[0].store(0f32.to_bits(), Ordering::Relaxed);
        assert!((d.next().unwrap() - 0.25).abs() < 1e-3);
        // Not looping: over with its first file.
        let once = LayeredMusic { looping: false, ..music };
        assert_eq!(once.decoder().count(), 4);
    }
}
