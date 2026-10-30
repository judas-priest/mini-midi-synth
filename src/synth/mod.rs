//! Synth engine: layered polyphonic voice pools + MIDI event dispatch.

pub mod dsp_utils;
mod airwindows;
use airwindows::Airwindows;
pub mod fx_chain;
pub mod bass;
pub mod bbd_ensemble;
pub mod bitcrusher;
pub mod bonsai;
pub mod chorus;
pub mod compressor;
pub mod conditioner;
pub mod delay;
pub mod drum;
pub mod envelope;
pub mod eq;
pub mod exciter;
pub mod filter;
pub mod flanger;
pub mod formant;
pub mod freq_shift;
pub mod graphic_eq;
pub mod lfo;
pub mod arpeggiator;
pub mod looper;
pub mod mod_matrix;
pub mod ms_tool;
pub mod mseg;
pub mod resonator;
pub mod rotary;
pub mod sampler;
pub mod epiano;
pub mod neuron;
pub mod oscillator;
pub mod overdrive;
pub mod piano;
pub mod phaser;
pub mod combulator;
pub mod convolution_reverb;
pub mod floaty_delay;
pub mod nimbus;
pub mod reverb;
pub mod reverb2;
pub mod ring_mod;
pub mod treemonster;
pub mod spring_reverb;
pub mod vocoder;
pub mod midi_player;
pub mod step_seq;
pub mod tape;
pub mod tremolo;
pub mod voice;
pub mod wave_shaper;
mod patch_params;
pub use patch_params::PatchParams;

use crate::cc_map::{CcMap, ParamScope};
use crate::preset::Patch;
use bbd_ensemble::BbdEnsemble;
use bitcrusher::Bitcrusher;
use bonsai::Bonsai;
use wave_shaper::WaveShaper;
use ms_tool::MsTool;
use graphic_eq::GraphicEq;
use conditioner::Conditioner;
use exciter::Exciter;
use floaty_delay::FloatyDelay;
use reverb2::Reverb2;
use combulator::Combulator;
use treemonster::Treemonster;
use nimbus::Nimbus;
use vocoder::Vocoder;
use convolution_reverb::ConvolutionReverb;
use chorus::Chorus;
use compressor::Compressor;
use delay::StereoDelay;
use resonator::Resonator;
use rotary::RotarySpeaker;
use drum::{DrumEngine, DrumPattern, DrumSlotParams, NUM_DRUM_SLOTS};
use eq::ParametricEq;
use flanger::Flanger;
use freq_shift::FreqShift;
use lfo::{Lfo, LfoStepSeqData, LfoWaveform};
use arpeggiator::Arpeggiator;
use looper::MidiLooper;
use neuron::Neuron;
use sampler::SamplerEngine;
use overdrive::Overdrive;
use phaser::Phaser;
use reverb::Reverb;
use ring_mod::RingMod;
use spring_reverb::SpringReverb;
use tape::Tape;
use tremolo::Tremolo;
use mod_matrix::ModMatrix;
use mseg::{Mseg, MsegState};
use voice::{Voice, VoiceParams};
use std::sync::Arc;

/// Lock-free oscilloscope buffer shared between audio and GUI threads.
/// Audio writes samples; GUI reads them for display. Uses atomic f32 encoding.
pub const SCOPE_SIZE: usize = 256;
pub struct ScopeBuffer {
    pub data: [std::sync::atomic::AtomicU32; SCOPE_SIZE],
    pub write_pos: std::sync::atomic::AtomicUsize,
}

impl Default for ScopeBuffer {
    fn default() -> Self { Self::new() }
}

impl ScopeBuffer {
    pub fn new() -> Self {
        Self {
            data: std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0)),
            write_pos: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    #[inline]
    pub fn push(&self, sample: f32, pos: &mut usize) {
        self.data[*pos].store(sample.to_bits(), std::sync::atomic::Ordering::Relaxed);
        *pos = (*pos + 1) & (SCOPE_SIZE - 1);
        self.write_pos.store(*pos, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn read(&self) -> [f32; SCOPE_SIZE] {
        let mut out = [0.0f32; SCOPE_SIZE];
        let wp = self.write_pos.load(std::sync::atomic::Ordering::Relaxed);
        for (i, out_sample) in out.iter_mut().enumerate() {
            let idx = (wp + i) & (SCOPE_SIZE - 1);
            *out_sample = f32::from_bits(self.data[idx].load(std::sync::atomic::Ordering::Relaxed));
        }
        out
    }
}

/// MIDI events sent from the MIDI thread to the audio thread.
#[allow(dead_code)]
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    PitchBend { channel: u8, value: f32 },
    ModWheel { channel: u8, value: f32 },
    ProgramChange { channel: u8, program: u8 },
    Aftertouch { channel: u8, value: f32 },
    PolyAftertouch { channel: u8, note: u8, pressure: f32 },
    ControlChange { channel: u8, cc: u8, value: u8 },
    /// SMK-37 Pro left/right button (SysEx F0 35 59 10 00 xx F7)
    Navigate { pressed: bool }, // true = press, false = release
}

/// Control events sent from GUI to the audio thread.
#[allow(dead_code)]
/// Heavyweight payload for `ControlEvent::LoadPatch`, boxed to keep the
/// `ControlEvent` enum slot small (~2 KB → ~32 B). Built on the GUI thread,
/// shipped through the SPSC queue, consumed once by the audio thread.
pub struct LoadPatchEvent {
    pub part: usize,
    pub params: PatchParams,
    pub mod_matrix: ModMatrix,
    pub mseg1: Option<Mseg>,
    pub mseg2: Option<Mseg>,
    pub pitch_seq: Option<step_seq::PitchSequencer>,
    pub lfo_step_seq: Option<[LfoStepSeqData; 4]>,
    pub wavetable: Option<(Arc<Vec<f32>>, usize, usize)>,
}

/// Root-only gate decision for one note-on (TECHDEBT #6).
pub(crate) enum RootDecision {
    /// Suppressed: an equal-or-lower key is already the sounding root.
    Suppress,
    /// Sound `note`; `release` (raw) is the old root to release first.
    Sound { release: Option<u8>, note: u8 },
}

pub enum ControlEvent {
    LoadPatch(Box<LoadPatchEvent>),
    SetPartEnabled { part: usize, enabled: bool },
    SetPartMute { part: usize, mute: bool },
    /// Root-note-only mode (bass split): part sounds only the lowest held key
    SetPartRootOnly { part: usize, on: bool },
    SetPartVolume { part: usize, volume: f32 },
    SetPartRange { part: usize, min_note: u8, max_note: u8 },
    SetPartVelRange { part: usize, vel_min: u8, vel_max: u8 },
    SetPartPan { part: usize, pan: f32 },
    SetPartTranspose { part: usize, semitones: i8 },
    /// Mono mode per part (last-note priority). true: poly/piano → mono
    /// (1), Mono-ST (2) → mono (1); false: mono/mono-ST → poly (0). Latch
    /// (3) / Poly-High / Poly-Low are left untouched. Clears the stale
    /// mono note stack on every transition.
    SetPartMono { part: usize, mono: bool },
    /// Legato per part: mono (1) ↔ mono-ST (2). No-op in other modes.
    SetPartLegato { part: usize, legato: bool },
    /// Portamento glide time in milliseconds (0 = instant). Clamped to
    /// 0..2000 ms (patch param `portamento_time` stays in seconds). A
    /// non-zero value also raises portamento_mode 0→1 ("Always").
    SetPartGlide { part: usize, ms: f32 },
    SetMacro { part: usize, index: usize, value: f32 },
    SetCcMap { map: Box<CcMap> },
    SetGlobalParam { key: &'static str, value: f32 },
    // Drum engine controls
    DrumSetStep { slot: u8, step: u8, velocity: u8 },
    DrumSetParam { slot: u8, param: DrumParam },
    DrumSetVolume { volume: f32 },
    DrumSeqPlay { playing: bool },
    DrumSeqBpm { bpm: f32 },
    DrumSeqSwing { swing: f32 },
    DrumSeqPattern { pattern: u8 },
    DrumSeqLength { length: u8 },
    /// Time signature for the current pattern: sets `beats_per_bar` and
    /// `length = beats_per_bar * 4` (16th steps). Clamped to 2..=4 —
    /// MAX_STEPS = 16 caps the bar at 4/4 (see docs/TECHDEBT.md §21).
    DrumSeqTimeSig { beats_per_bar: u8 },
    DrumSeqRecord { recording: bool },
    DrumSeqClear,
    DrumSeqUndo,
    DrumSeqRedo,
    DrumLoadKit { patterns: Box<[DrumPattern; 8]>, params: Box<[DrumSlotParams; NUM_DRUM_SLOTS]>, bpm: f32, swing: f32, volume: f32 },
    // Looper controls
    LooperRecord,
    LooperStopRecord,
    LooperTogglePlay,
    LooperUndo,
    LooperClear,
    LooperSetBars { bars: u8 },
    LooperSetQuantize { quantize: u8 },
    /// Restore recorded looper content after an audio reconnect. The payload
    /// owns heap memory and is handed back from handle_control for disposal.
    LooperRestore {
        events: Box<[crate::synth::looper::LooperEventSnapshot]>,
        loop_length_ticks: u32,
    },
    LooperSetLayerMute { layer: u8, mute: bool },
    LooperSetSolo { layer: Option<u8> },
    /// Per-part replay mute (TECHDEBT #30/#34): a muted part's recorded
    /// events are skipped on looper playback; its sounding notes are flushed.
    LooperSetPartMute { part: u8, mute: bool },
    /// Looper record source: true = Mix (every sounding enabled part is
    /// recorded, each event tagged with its own part id), false = Part
    /// (only the active part). Live transport state — not persisted.
    LooperSetSource { mix: bool },
    /// Master transport (#32): toggle play drums + looper together
    MasterPlay,
    /// Master transport: stop drums + looper (drain note-offs)
    MasterStop,
    /// Master transport: arm/disarm recording on drums + looper
    MasterRecord,
    SetSustain { on: bool },
    /// Static pedal-up release duration in seconds (None = patch release)
    SetSustainRelease { seconds: Option<f32> },
    AllNotesOff,
    /// GUI sustain toggle (F1): same path as CC64
    /// Drums: force every hit to velocity 127
    DrumSetFixedVelocity { fixed: bool },
    /// Drums: metronome click on every beat while the sequencer plays
    DrumSetMetronome { on: bool },
    /// Drums: pad-note -> internal slot table (index = in_note-36, value = slot)
    DrumSetRemap { table: Box<[u8; 16]> },
    /// Drums: per-note choke group ids for the SF2 path, indexed (note-30) (0 = none)
    DrumSetChokeGroups { table: Box<[u8; 26]> },
    /// Drums: per-pad velocity override (index = in_note-36, 0 = off)
    DrumSetPadVel { table: Box<[u8; 16]> },
    // SF2 sampler controls
    LoadKeysSoundFont { soundfont: std::sync::Arc<rustysynth::SoundFont> },
    LoadDrumsSoundFont { soundfont: std::sync::Arc<rustysynth::SoundFont> },
    UnloadKeysSoundFont,
    UnloadDrumsSoundFont,
    SetPartSf2Mode { part: usize, enabled: bool },
    SetPartSf2Program { part: usize, program: u8, bank: u8 },
    SetDrumsSf2Mode { enabled: bool },
    SetSf2BlockSize { size: usize },
    SetSf2SampleOffset { ms: f32 },
    /// Global SF2 release multiplier (GM2 CC72 semantics, 1.0 = file defaults)
    SetSf2ReleaseScale { scale: f32 },
    // Pitch step sequencer
    SeqSetEnabled { part: usize, enabled: bool },
    SeqSetStep { part: usize, step: u8, pitch: i8, gate: bool, velocity: u8 },
    SeqSetLength { part: usize, length: u8 },
    SeqSetRate { part: usize, rate: u8 },
    SeqSetScale { part: usize, scale: u8 },
    SeqSetSwing { part: usize, swing: f32 },
    // MIDI file sequencer
    MidiSeqLoad { data: Box<midi_player::MidiSeqData> },
    MidiSeqPlay { playing: bool },
    MidiSeqSetBpm { bpm: Option<f32> },
    MidiSeqSetLooping { looping: bool },
    MidiSeqSetTrackInstrument { track_idx: usize, instrument: midi_player::TrackInstrument },
    MidiSeqSetTrackMute { track_idx: usize, muted: bool },
    // Arpeggiator
    SetArpParams { enabled: bool, mode: u8, rate: u8, octaves: u8, gate: f32 },
    /// Arp latch (TECHDEBT 14.4): true — arp keeps playing after keys are
    /// released; false — a frozen latched set is stopped immediately.
    SetArpLatch { latch: bool },
}

impl ControlEvent {
    /// Build a LoadPatch event from a Patch + part index.
    /// Extracts PatchParams, ModMatrix, MSEG, PitchSequencer, wavetable in one place.
    pub fn load_patch_from(part: usize, patch: &crate::preset::Patch) -> Self {
        let params = PatchParams::from_map(&patch.params);
        let mut mod_matrix = ModMatrix::default();
        mod_matrix.load_from_params(&patch.params);
        let mseg1 = if patch.params.get("mseg_enabled").copied().unwrap_or(0.0) > 0.5 {
            Some(Mseg::load_from_params(&patch.params))
        } else {
            None
        };
        let mut pitch_seq = step_seq::PitchSequencer::new();
        pitch_seq.load_from_params(&patch.params);
        let lfo_step_seq = Some([
            LfoStepSeqData::from_params(0, &patch.params),
            LfoStepSeqData::from_params(1, &patch.params),
            LfoStepSeqData::from_params(2, &patch.params),
            LfoStepSeqData::from_params(3, &patch.params),
        ]);
        let wavetable = patch.wavetable_data.as_ref().map(|d| {
            (d.clone(), patch.wavetable_frames, patch.wavetable_frame_size)
        });
        Self::LoadPatch(Box::new(LoadPatchEvent {
            part, params, mod_matrix, mseg1, mseg2: None,
            pitch_seq: Some(pitch_seq), lfo_step_seq, wavetable,
        }))
    }

    /// Build a LoadPatch from edited params (BTreeMap) + optional wavetable from original Patch.
    pub fn load_patch_from_edited(
        part: usize,
        edited: &std::collections::BTreeMap<String, f32>,
        original_patch: Option<&crate::preset::Patch>,
    ) -> Self {
        let params = PatchParams::from_map(edited);
        let mut mod_matrix = ModMatrix::default();
        mod_matrix.load_from_params(edited);
        let mseg1 = if edited.get("mseg_enabled").copied().unwrap_or(0.0) > 0.5 {
            Some(Mseg::load_from_params(edited))
        } else {
            None
        };
        let mut pitch_seq = step_seq::PitchSequencer::new();
        pitch_seq.load_from_params(edited);
        let lfo_step_seq = Some([
            LfoStepSeqData::from_params(0, edited),
            LfoStepSeqData::from_params(1, edited),
            LfoStepSeqData::from_params(2, edited),
            LfoStepSeqData::from_params(3, edited),
        ]);
        let wavetable = original_patch.and_then(|p| {
            p.wavetable_data.as_ref().map(|d| (d.clone(), p.wavetable_frames, p.wavetable_frame_size))
        });
        Self::LoadPatch(Box::new(LoadPatchEvent {
            part, params, mod_matrix, mseg1, mseg2: None,
            pitch_seq: Some(pitch_seq), lfo_step_seq, wavetable,
        }))
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum DrumParam {
    Level(f32),
    Pan(f32),
    Tune(f32),
    Decay(f32),
}

/// Feedback from audio thread to GUI.
pub enum ParamFeedback {
    ParamChanged { key: &'static str, value: f32 },
    CcReceived { cc: u8 },
    ProgramChanged { program: u8 },
    DrumStepRecorded { pattern: u8, slot: u8, step: u8, velocity: u8 },
    /// Pickup indicator: knob position (0-127) vs param for a CC that hasn't picked up yet.
    PickupPending { key: &'static str, cc_position: f32 },
    /// CC has picked up — clear the indicator.
    PickupDone { key: &'static str },
    /// SysEx navigate button press/release.
    NavigatePress,
    NavigateRelease,
}

/// Modulation state computed per-part, passed to voices.
#[derive(Clone, Copy)]
pub struct ModulationState {
    pub pitch_mult: f32,
    pub filter_offset: f32,       // Hz (from LFOs, aftertouch)
    pub filter_offset_semis: f32, // semitones (from mod matrix — applied as 2^(s/12) multiplier)
    pub amp_mod: f32,
}

pub const MAX_PARTS: usize = 16;
const VOICES_PER_LAYER: usize = 8;
pub const BLOCK_SIZE: usize = 32;

/// Global parameters — only truly global controls that persist across everything.
struct GlobalParams {
    master_volume: f32,
    master_tone: f32, // simple LP cutoff (20-20000 Hz), post-effects
    // Fader-controlled global (persist across patch changes)
    reverb_mix: Option<f32>,       // None = use preset value
    delay_mix: Option<f32>,        // None = use preset value
    pitch_bend_range: f32,         // semitones, default 2 (used when up/down not set)
    pitch_bend_up: f32,            // semitones up (0 = use pitch_bend_range)
    pitch_bend_down: f32,          // semitones down (0 = use pitch_bend_range)
    // One-pole LP state for master tone
    tone_lp_l: f32,
    tone_lp_r: f32,
}

impl Default for GlobalParams {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            master_tone: 20000.0,
            reverb_mix: None,
            delay_mix: None,
            pitch_bend_range: 2.0,
            pitch_bend_up: 0.0,
            pitch_bend_down: 0.0,
            tone_lp_l: 0.0,
            tone_lp_r: 0.0,
        }
    }
}

impl GlobalParams {
    fn set(&mut self, key: &str, value: f32) {
        match key {
            "master_volume" => self.master_volume = value,
            "master_tone" => self.master_tone = value,
            "reverb_mix" => self.reverb_mix = Some(value),
            "delay_mix" => self.delay_mix = Some(value),
            "pitch_bend_range" => self.pitch_bend_range = value.clamp(1.0, 24.0),
            "pitch_bend_up"    => self.pitch_bend_up    = value.clamp(0.0, 48.0),
            "pitch_bend_down"  => self.pitch_bend_down  = value.clamp(0.0, 48.0),
            _ => {}
        }
    }

    /// Apply master tone (one-pole LP) to output. Cheap, no resonance.
    #[inline]
    fn apply_tone(&mut self, l: f32, r: f32, sample_rate: f32) -> (f32, f32) {
        let freq = self.master_tone.clamp(20.0, 20000.0);
        if freq >= 19000.0 { return (l, r); } // bypass when wide open
        let coeff = (std::f32::consts::PI * freq / sample_rate).sin().min(0.999);
        self.tone_lp_l += coeff * (l - self.tone_lp_l);
        self.tone_lp_r += coeff * (r - self.tone_lp_r);
        (self.tone_lp_l, self.tone_lp_r)
    }
}

/// Pickup state for a single CC — prevents jumps on patch change.
#[derive(Clone, Copy)]
struct PickupState {
    last_cc_value: Option<u8>,
    picked_up: bool,
}

impl Default for PickupState {
    fn default() -> Self {
        Self { last_cc_value: None, picked_up: true }
    }
}

struct Part {
    voices: Vec<Voice>,
    age_counter: u64,
    volume: f32,
    enabled: bool,   // part is in scene (has patch, receives MIDI)
    mute: bool,      // temporary silence without removing from scene
    /// Mix gate for mute: 1.0 open; ramps to 0 over ~5 ms when muted, back to
    /// 1.0 on unmute (click-free). Per-sample, audio thread only.
    mute_gain: f32,
    min_note: u8,
    max_note: u8,
    vel_min: u8,     // velocity range (1-127)
    vel_max: u8,
    pan: f32,        // -1.0 (L) .. 0.0 (C) .. +1.0 (R)
    transpose: i8,   // semitone offset per part (-24..+24)
    params: PatchParams,
    lfos: [Lfo; 4],
    /// Scene LFOs — free-running, never reset on note-on (unlike voice LFOs).
    /// tick every block regardless of note activity.
    scene_lfos: [Lfo; 2],
    scene_lfo_routed: [bool; 2],  // true when used in mod matrix
    /// External wavetable loaded from .wt file (set via LoadPatch)
    wavetable_data: Option<Arc<Vec<f32>>>,
    wavetable_frames: usize,
    wavetable_frame_size: usize,
    /// Current macro knob values (0..1). Updated by SetMacro event.
    macro_vals: [f32; 8],
    mod_matrix: ModMatrix,
    mseg1: Mseg,
    mseg1_state: MsegState,
    mseg2: Mseg,
    mseg2_state: MsegState,
    last_note_freq: Option<f32>,
    last_velocity: f32,
    last_note: u8,
    sf2_mode: bool,
    pitch_seq: step_seq::PitchSequencer,
    // Mono / Latch voice mode state
    note_stack: Vec<u8>,   // held notes for mono mode (newest last)
    latch_held: Vec<u8>,   // latched notes (toggle mode)
    // Random/Alternate mod source state
    rng_state: u64,         // LCG PRNG state
    alt_counter: u32,       // alternation counter (even=+, odd=-)
    rand_bipolar: f32,      // current random bipolar value
    rand_unipolar: f32,     // current random unipolar value
    alt_bipolar: f32,       // current alternate bipolar value
    alt_unipolar: f32,      // current alternate unipolar value
    release_vel: f32,       // last note-off velocity
    sustained_notes: [bool; 128], // notes held by sustain pedal
    keys_down: [bool; 128],   // raw key physically held (pedal-up must spare these)
    /// Root-only mode (TECHDEBT #6): gate + per-part lowest-note tracking.
    root_only: bool,
    root_held: [bool; 128],   // keys currently physically held, for re-keying
    root_active: Option<u8>,  // raw note currently sounding as the root
    breath: f32,            // CC2, 0..1
    expression: f32,        // CC11, 0..1
    sustain_pedal: f32,     // CC64: 0.0 or 1.0
    lowest_held: u8,        // lowest active note (default 60)
    highest_held: u8,       // highest active note (default 60)
    // Cached mod sources (control-rate, with poly_aftertouch=0) for per-voice re-evaluation
    cached_mod_sources: mod_matrix::ModSources,
    poly_at_in_matrix: bool,  // true if any mod slot uses PolyAftertouch
    // Cached routing flags — updated on patch load, not per-block
    lfo2_routed: bool,
    lfo3_routed: bool,
    lfo4_routed: bool,
    // Cached control-rate base values for per-voice ModulationState reconstruction
    cached_lfo_pitch_offset: f32,  // pitch_offset minus mod_offsets.pitch
    cached_filter_offset: f32,
    cached_lfo_amp_base: f32,      // (1 - lfo1_amp) * (1 - lfo2_amp) * seq_amp
}

impl Part {
    fn new(sample_rate: f32) -> Self {
        let voices = (0..VOICES_PER_LAYER).map(|_| Voice::new(sample_rate)).collect();
        Self {
            voices, age_counter: 0, volume: 0.8, enabled: false, mute: false, mute_gain: 1.0,
            min_note: 0, max_note: 127, vel_min: 1, vel_max: 127,
            pan: 0.0, transpose: 0,
            params: PatchParams::default(),
            lfos: [Lfo::new(sample_rate), Lfo::new(sample_rate), Lfo::new(sample_rate), Lfo::new(sample_rate)],
            scene_lfos: [Lfo::new(sample_rate), Lfo::new(sample_rate)],
            scene_lfo_routed: [false; 2],
            wavetable_data: None,
            wavetable_frames: 0,
            wavetable_frame_size: 0,
            macro_vals: [0.0; 8],
            mod_matrix: ModMatrix::default(),
            mseg1: Mseg::default(), mseg1_state: MsegState::new(sample_rate),
            mseg2: Mseg::default(), mseg2_state: MsegState::new(sample_rate),
            last_note_freq: None, last_velocity: 1.0, last_note: 60,
            sf2_mode: false, pitch_seq: step_seq::PitchSequencer::new(),
            note_stack: Vec::new(), latch_held: Vec::new(),
            rng_state: 0x5851f42d4c957f2d, alt_counter: 0,
            rand_bipolar: 0.0, rand_unipolar: 0.5,
            alt_bipolar: 1.0, alt_unipolar: 1.0, release_vel: 0.0,
            sustained_notes: [false; 128],
            keys_down: [false; 128],
            root_only: false, root_held: [false; 128], root_active: None,
            breath: 0.0, expression: 1.0, sustain_pedal: 0.0,
            lowest_held: 60, highest_held: 60,
            cached_mod_sources: mod_matrix::ModSources {
                lfo_outputs: [0.0; 4], amp_env: 0.0, filter_env: 0.0,
                mseg_outputs: [0.0; 2], mod_wheel: 0.0, aftertouch: 0.0,
                velocity: 1.0, key_track: 0.0, step_seq: 0.0,
                random_bipolar: 0.0, random_unipolar: 0.5,
                alt_bipolar: 1.0, alt_unipolar: 1.0, release_vel: 0.0,
                pitch_bend: 0.0, cc: [0.0; 4], breath: 0.0, expression: 1.0,
                sustain_pedal: 0.0, lowest_key: 0.0, highest_key: 0.0,
                latest_key: 0.0, poly_aftertouch: 0.0,
                scene_lfo_outputs: [0.0; 2],
                macro_vals: [0.0; 8],
            },
            poly_at_in_matrix: false,
            lfo2_routed: false,
            lfo3_routed: false,
            lfo4_routed: false,
            cached_lfo_pitch_offset: 0.0,
            cached_filter_offset: 0.0,
            cached_lfo_amp_base: 1.0,
        }
    }

    /// Update cached routing flags from mod matrix slots. Call after loading a patch
    /// or changing mod matrix routing to avoid per-block scanning.
    fn update_routing_cache(&mut self) {
        self.lfo2_routed = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::Lfo2 && s.depth.abs() > 0.001);
        self.lfo3_routed = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::Lfo3 && s.depth.abs() > 0.001);
        self.lfo4_routed = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::Lfo4 && s.depth.abs() > 0.001);
        self.poly_at_in_matrix = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::PolyAftertouch && s.depth.abs() > 0.001);
        self.scene_lfo_routed[0] = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::SceneLfo1 && s.depth.abs() > 0.001);
        self.scene_lfo_routed[1] = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::SceneLfo2 && s.depth.abs() > 0.001);
    }

    /// LCG random float 0..1
    fn next_rand(&mut self) -> f32 {
        self.rng_state = self.rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.rng_state >> 33) as f32) / (u32::MAX as f32)
    }

    /// Start a voice from a RAW (pre-transpose) MIDI note. Transposition is
    /// applied internally: `voice.note` keeps the sounding pitch, `voice.src_note`
    /// keeps the raw key so note-offs match across transpose changes.
    fn note_on(&mut self, raw_note: u8, velocity: u8) {
        if !self.enabled { return; }
        let note = (raw_note as i16 + self.transpose as i16).clamp(0, 127) as u8;
        if note < self.min_note || note > self.max_note { return; }
        debug_assert!(raw_note < 128);
        self.keys_down[raw_note as usize] = true;
        self.last_velocity = velocity as f32 / 127.0;
        self.last_note = note;

        // Update random/alternate mod sources on each note-on
        let r = self.next_rand();
        self.rand_bipolar = r * 2.0 - 1.0;
        self.rand_unipolar = r;
        self.alt_counter = self.alt_counter.wrapping_add(1);
        let is_even = self.alt_counter.is_multiple_of(2);
        self.alt_bipolar = if is_even { 1.0 } else { -1.0 };
        self.alt_unipolar = if is_even { 1.0 } else { 0.0 };

        let play_mode = self.params.play_mode as u8;

        // Latch mode: toggle note on/off
        if play_mode == 3 {
            if let Some(pos) = self.latch_held.iter().position(|&n| n == raw_note) {
                self.latch_held.remove(pos);
                self.keys_down[raw_note as usize] = false;
                for v in &mut self.voices { if v.active && v.src_note == raw_note { v.note_off(); } }
            } else {
                self.latch_held.push(raw_note);
                // fall through to start voice below
            }
            if self.latch_held.contains(&raw_note) {
                // Start voice for new latch note
                self.age_counter += 1;
                let age = self.age_counter;
                let idx = self.voices.iter().position(|v| !v.active).unwrap_or_else(|| {
                    self.voices.iter().enumerate().min_by_key(|(_, v)| v.age).map(|(i, _)| i).unwrap_or(0)
                });
                let prev_freq = self.last_note_freq;
                let voice_params = self.build_voice_params();
                self.voices[idx].note_on(note, raw_note, velocity, age, &voice_params, prev_freq);
                self.last_note_freq = Some(440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0));
                self.lfos[0].trigger_mode(self.params.lfo1_trigger_mode as u8);
                self.lfos[1].trigger_mode(self.params.lfo2_trigger_mode as u8);
                self.lfos[2].trigger_mode(self.params.lfo3_trigger_mode as u8);
                self.lfos[3].trigger_mode(self.params.lfo4_trigger_mode as u8);
                if self.params.mseg_enabled > 0.5 { self.mseg1_state.trigger(); self.mseg2_state.trigger(); }
            }
            self.update_held_range();
            return;
        }

        // Mono modes (1=mono, 2=mono-st single trigger, 3 handled above)
        if play_mode == 1 || play_mode == 2 {
            self.note_stack.retain(|&n| n != raw_note);
            self.note_stack.push(raw_note);
            self.age_counter += 1;
            let age = self.age_counter;
            // Retrigger the single mono voice
            let idx = self.voices.iter().position(|v| v.active).unwrap_or_else(|| {
                self.voices.iter().position(|v| !v.active).unwrap_or(0)
            });
            // Same gating as the poly path: portamento_mode 0 (Off) never
            // glides; 2 (Fingered) glides only while another key is held
            // (the new note is already pushed, so len > 1 means a key was
            // held); 1 (Always) glides from the last pitch.
            let porta_mode = self.params.portamento_mode as u8;
            let prev_freq = match porta_mode {
                1 => self.last_note_freq,
                2 => if self.note_stack.len() > 1 { self.last_note_freq } else { None },
                _ => None,
            };
            let voice_params = self.build_voice_params();
            let retrigger = play_mode == 2 && self.note_stack.len() > 1; // ST: don't retrigger env
            if retrigger {
                self.voices[idx].retrigger_note(note, raw_note, &voice_params);
            } else {
                self.voices[idx].note_on(note, raw_note, velocity, age, &voice_params, prev_freq);
            }
            self.last_note_freq = Some(440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0));
            if !retrigger {
                self.lfos[0].trigger_mode(self.params.lfo1_trigger_mode as u8);
                self.lfos[1].trigger_mode(self.params.lfo2_trigger_mode as u8);
                self.lfos[2].trigger_mode(self.params.lfo3_trigger_mode as u8);
                self.lfos[3].trigger_mode(self.params.lfo4_trigger_mode as u8);
            }
            if self.params.mseg_enabled > 0.5 && !retrigger { self.mseg1_state.trigger(); self.mseg2_state.trigger(); }
            self.update_held_range();
            return;
        }

        // Piano mode: if same key is already playing, don't retrigger — let it continue
        if play_mode == 6 && self.voices.iter().any(|v| v.active && v.src_note == raw_note) {
            return;
        }

        self.age_counter += 1;
        let age = self.age_counter;
        let idx = self.voices.iter().position(|v| !v.active).unwrap_or_else(|| {
            match play_mode {
                4 => {
                    // Poly-High: steal lowest note (highest note wins)
                    self.voices.iter().enumerate()
                        .filter(|(_, v)| v.active)
                        .min_by_key(|(_, v)| v.note)
                        .map(|(i, _)| i)
                        .unwrap_or(0)
                }
                5 => {
                    // Poly-Low: steal highest note (lowest note wins)
                    self.voices.iter().enumerate()
                        .filter(|(_, v)| v.active)
                        .max_by_key(|(_, v)| v.note)
                        .map(|(i, _)| i)
                        .unwrap_or(0)
                }
                _ => {
                    // Default (Poly / Piano / Latch): JUCE-style steal order —
                    // voices already in release phase die first (their tails
                    // are almost done), then the oldest sustained voice.
                    let releasing = self.voices.iter().enumerate()
                        .filter(|(_, v)| v.active && v.is_releasing())
                        .min_by_key(|(_, v)| v.age);
                    releasing
                        .or_else(|| self.voices.iter().enumerate()
                            .min_by_key(|(_, v)| v.age))
                        .map(|(i, _)| i)
                        .unwrap_or(0)
                }
            }
        });
        let porta_mode = self.params.portamento_mode as u8;
        let prev_freq = match porta_mode {
            1 => self.last_note_freq,
            2 => if self.has_active_notes() { self.last_note_freq } else { None },
            _ => None,
        };
        let voice_params = self.build_voice_params();
        self.voices[idx].note_on(note, raw_note, velocity, age, &voice_params, prev_freq);
        self.last_note_freq = Some(440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0));
        // Retrigger LFOs on note-on if flagged
        self.lfos[0].trigger_mode(self.params.lfo1_trigger_mode as u8);
        self.lfos[1].trigger_mode(self.params.lfo2_trigger_mode as u8);
        self.lfos[2].trigger_mode(self.params.lfo3_trigger_mode as u8);
        self.lfos[3].trigger_mode(self.params.lfo4_trigger_mode as u8);
        // Trigger MSEGs on note-on (per-part, not per-voice)
        if self.params.mseg_enabled > 0.5 {
            self.mseg1_state.trigger();
            self.mseg2_state.trigger();
        }
        self.update_held_range();
    }

    fn build_voice_params(&self) -> VoiceParams {
        VoiceParams {
            osc_type: self.params.osc_type, osc_detune: self.params.osc_detune,
            fm_ratio: self.params.fm_ratio, fm_index: self.params.fm_index,
            fm_env_amount: self.params.fm_env_amount,
            osc_count: self.params.osc_count, osc1_level: self.params.osc1_level,
            osc2_type: self.params.osc2_type, osc2_detune: self.params.osc2_detune, osc2_level: self.params.osc2_level,
            osc3_type: self.params.osc3_type, osc3_detune: self.params.osc3_detune, osc3_level: self.params.osc3_level,
            filter_cutoff: self.params.filter_cutoff, filter_resonance: self.params.filter_resonance,
            filter_type: self.params.filter_type, filter_env_amount: self.params.filter_env_amount,
            filter_key_track: self.params.filter_key_track,
            filter_routing: self.params.filter_routing, filter2_type: self.params.filter2_type,
            filter2_cutoff: self.params.filter2_cutoff, filter2_resonance: self.params.filter2_resonance,
            noise_level: self.params.noise_level,
            amp_attack: self.params.amp_attack, amp_decay: self.params.amp_decay,
            amp_sustain: self.params.amp_sustain, amp_release: self.params.amp_release,
            amp_hold: self.params.amp_hold,
            filter_attack: self.params.filter_attack, filter_decay: self.params.filter_decay,
            filter_sustain: self.params.filter_sustain, filter_release: self.params.filter_release,
            filter_hold: self.params.filter_hold,
            ks_brightness: self.params.ks_brightness, ks_feedback: self.params.ks_feedback,
            organ_drawbars: self.params.organ_drawbars,
            formant_voice: self.params.formant_voice, formant_vowel: self.params.formant_vowel,
            drum_pitch_amount: self.params.drum_pitch_amount, drum_pitch_decay: self.params.drum_pitch_decay,
            drum_noise_level: self.params.drum_noise_level, drum_noise_decay: self.params.drum_noise_decay,
            drum_noise_color: self.params.drum_noise_color,
            bass_style: self.params.bass_style, bass_tone: self.params.bass_tone, bass_body: self.params.bass_body, bass_pickup: self.params.bass_pickup,
            bow_pressure: self.params.bow_pressure, bow_position: self.params.bow_position, body_type: self.params.body_type,
            lip_tension: self.params.lip_tension, blowing_pressure: self.params.blowing_pressure, bell_type: self.params.bell_type,
            pd_shape: self.params.pd_shape, pd_depth: self.params.pd_depth, pd_env_amount: self.params.pd_env_amount,
            fold_amount: self.params.fold_amount, fold_symmetry: self.params.fold_symmetry, fold_source: self.params.fold_source,
            modal_material: self.params.modal_material, modal_brightness: self.params.modal_brightness,
            modal_damping: self.params.modal_damping, modal_strike_pos: self.params.modal_strike_pos,
            sync_ratio: self.params.sync_ratio, sync_shape: self.params.sync_shape,
            supersaw_detune: self.params.supersaw_detune, supersaw_mix: self.params.supersaw_mix,
            pulse_width: self.params.pulse_width,
            accordion_register: self.params.accordion_register, accordion_bellows: self.params.accordion_bellows,
            sax_reed_stiffness: self.params.sax_reed_stiffness, sax_embouchure: self.params.sax_embouchure,
            sax_blow_pressure: self.params.sax_blow_pressure, sax_type: self.params.sax_type,
            epiano_type: self.params.epiano_type,
            alias_wave_type: self.params.alias_wave_type, alias_crush: self.params.alias_crush,
            window_type: self.params.window_type, window_morph: self.params.window_morph,
            window_formant: self.params.window_formant,
            wavetable_data: self.wavetable_data.clone(),
            wavetable_frames: self.wavetable_frames,
            wavetable_frame_size: self.wavetable_frame_size,
            twist_engine: self.params.twist_engine as u32,
            twist_harmonics: self.params.twist_harmonics,
            twist_timbre: self.params.twist_timbre,
            twist_morph: self.params.twist_morph,
            twist_lpg_decay: self.params.twist_lpg_decay,
            twist_lpg_colour: self.params.twist_lpg_colour,
            twist_aux_mix: self.params.twist_aux_mix,
            env_attack_shape: self.params.env_attack_shape, env_decay_shape: self.params.env_decay_shape,
            env_release_shape: self.params.env_release_shape,
            filter_env_attack_shape: self.params.filter_env_attack_shape,
            filter_env_decay_shape: self.params.filter_env_decay_shape,
            filter_env_release_shape: self.params.filter_env_release_shape,
            velocity_curve: self.params.velocity_curve, vel_to_filter: self.params.vel_to_filter,
            portamento_time: self.params.portamento_time, _portamento_mode: self.params.portamento_mode,
            unison_voices: self.params.unison_voices, unison_detune: self.params.unison_detune,
            unison_spread: self.params.unison_spread,
            fm_cross_depth: self.params.fm_cross_depth,
            filter_env_semitones: self.params.filter_env_semitones,
            osc_ws_mode: self.params.osc_ws_mode as u32,
            osc_ws_drive: self.params.osc_ws_drive * 4.0 + 0.5,
            osc_ws_mix: self.params.osc_ws_mix,
            inter_ws_mode: self.params.inter_ws_mode as u32,
            inter_ws_drive: self.params.inter_ws_drive * 4.0 + 0.5,
            inter_ws_mix: self.params.inter_ws_mix,
            svf_morph: self.params.svf_morph,
            filter_drive: self.params.filter_drive,
            filter_starve: self.params.filter_starve,
        }
    }

    /// Release voices matching the RAW (pre-transpose) note — see `note_on`.
    fn note_off(&mut self, raw_note: u8) {
        self.keys_down[raw_note as usize] = false;
        let note = (raw_note as i16 + self.transpose as i16).clamp(0, 127) as u8;
        let play_mode = self.params.play_mode as u8;

        // Latch mode: notes released by toggling via note_on, not note_off
        if play_mode == 3 { return; }

        // Mono modes: manage note stack
        if play_mode == 1 || play_mode == 2 {
            self.note_stack.retain(|&n| n != raw_note);
            if let Some(&prev_note) = self.note_stack.last() {
                // Retrigger with the previous held note
                let vp = self.build_voice_params();
                let prev_transposed = (prev_note as i16 + self.transpose as i16).clamp(0, 127) as u8;
                if let Some(v) = self.voices.iter_mut().find(|v| v.active) {
                    v.retrigger_note(prev_transposed, prev_note, &vp);
                    if play_mode == 1 { v.retrigger_envelope(); }
                }
            } else {
                for voice in &mut self.voices {
                    if voice.active && voice.src_note == raw_note { voice.note_off(); }
                }
                if !self.has_active_notes() {
                    self.mseg1_state.release();
                    self.mseg2_state.release();
                }
            }
            self.update_held_range();
            return;
        }

        self.release_vel = 0.5; // default; would need MIDI note-off velocity from caller

        if self.sustain_pedal > 0.5 {
            match self.params.sustain_mode as u32 {
                0 => {
                    // Hold all notes — don't release while pedal is down
                    self.sustained_notes[raw_note as usize] = true;
                }
                1 => {
                    // Release note only if other active non-releasing notes are held
                    let other_active = self.voices.iter()
                        .any(|v| v.note != note && v.active && !v.is_releasing());
                    if other_active {
                        for voice in &mut self.voices {
                            if voice.active && voice.src_note == raw_note { voice.note_off(); }
                        }
                    } else {
                        // Held by pedal — must be flagged so pedal-up releases it
                        self.sustained_notes[raw_note as usize] = true;
                    }
                }
                _ => {
                    // Default: hold all
                    self.sustained_notes[raw_note as usize] = true;
                }
            }
        } else {
            for voice in &mut self.voices {
                if voice.active && voice.src_note == raw_note { voice.note_off(); }
            }
        }

        // Release MSEGs when no active held notes remain
        if !self.has_active_notes() {
            self.mseg1_state.release();
            self.mseg2_state.release();
        }
        self.update_held_range();
    }

    /// Release all voices playing `note` with an explicit amp-release duration
    /// (matches by voice.note — the caller builds its needs bitmap by note).
    /// Used for the static pedal-up release (TECHDEBT #7).
    fn note_off_for(&mut self, note: u8, seconds: f32) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note { voice.note_off_for(seconds); }
        }
    }

    fn has_active_notes(&self) -> bool {
        self.voices.iter().any(|v| v.active && !v.is_releasing())
    }

    /// Root-only gate, note-on side. Called for EVERY in-range note-on routed
    /// to this part (both engines) via SynthEngine::route_note_on.
    fn root_on(&mut self, raw: u8) -> RootDecision {
        self.root_held[raw as usize] = true;
        // Latch mode is NOT gated: latch toggles on note-on and the gate would
        // break the toggle contract (tap-off suppressed under root-only).
        if !self.root_only || self.params.play_mode as u8 == 3 {
            return RootDecision::Sound { release: None, note: raw };
        }
        match self.root_active {
            None => { self.root_active = Some(raw); RootDecision::Sound { release: None, note: raw } }
            Some(cur) if raw < cur => {
                self.root_active = Some(raw);
                RootDecision::Sound { release: Some(cur), note: raw }
            }
            Some(_) => RootDecision::Suppress, // duplicate or higher key
        }
    }

    /// Root-only gate, note-off side. Returns the next-lowest still-held key
    /// to re-key to (Some only when `raw` WAS the sounding root).
    fn root_off(&mut self, raw: u8) -> Option<u8> {
        self.root_held[raw as usize] = false;
        if !self.root_only || self.root_active != Some(raw) { return None; }
        self.root_active = (0u8..128).find(|&n| self.root_held[n as usize]);
        self.root_active
    }

    /// Drains a part on state changes / panic: releases (or kills) DSP voices,
    /// releases this part's SF2 voices, and clears every held-note tracking
    /// field — pedal-held notes, keys down, mono/latch stacks, and the
    /// root-only rekey state. Without the root clear a stale root_active
    /// ghost-suppresses the next note after MIDI-stop / AllNotesOff on
    /// root_only patches (see root_on/root_off). `kill`: CC120 semantics
    /// (immediate) vs release (CC123 / state change). Caller must have lifted
    /// engine-level sustain via set_sustain(0) first — this fn does not touch
    /// part.sustain_pedal. Zero-allocation (audio thread).
    fn drain_notes(&mut self, sampler: &mut SamplerEngine, idx: usize, kill: bool) {
        sampler.part_all_notes_off(idx);
        for voice in &mut self.voices {
            if voice.active {
                if kill { voice.kill(); } else { voice.note_off(); }
            }
        }
        self.sustained_notes = [false; 128];
        self.keys_down = [false; 128];
        self.note_stack.clear();
        self.latch_held.clear();
        self.root_held = [false; 128];
        self.root_active = None;
    }

    fn update_held_range(&mut self) {
        let mut lo = 255u8;
        let mut hi = 0u8;
        for v in &self.voices {
            if v.active && !v.is_releasing() {
                if v.note < lo { lo = v.note; }
                if v.note > hi { hi = v.note; }
            }
        }
        if lo > hi {
            // No active non-releasing voices — keep last known values
        } else {
            self.lowest_held = lo;
            self.highest_held = hi;
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        for voice in &mut self.voices { voice.set_sample_rate(sample_rate); }
        for lfo in &mut self.lfos { lfo.set_sample_rate(sample_rate); }
        self.mseg1_state.set_sample_rate(sample_rate);
        self.mseg2_state.set_sample_rate(sample_rate);
    }

    fn tick(&mut self, mods: &ModulationState) -> (f32, f32) {
        if !self.enabled { return (0.0, 0.0); }
        let (mut out_l, mut out_r) = (0.0, 0.0);
        for voice in &mut self.voices {
            // Per-voice poly aftertouch: compute only the AT delta, not full matrix
            let voice_mods = if self.poly_at_in_matrix && voice.active && voice.poly_aftertouch > 0.001 {
                let delta = self.mod_matrix.evaluate_poly_at_delta(voice.poly_aftertouch);
                let pitch_mult = if (mods.pitch_mult - 1.0).abs() < 0.001 && delta.pitch.abs() < 0.001 {
                    1.0
                } else {
                    mods.pitch_mult * if delta.pitch.abs() > 0.001 { (delta.pitch / 12.0).exp2() } else { 1.0 }
                };
                let amp_mod = mods.amp_mod * (1.0 + delta.amplitude).max(0.0);
                ModulationState {
                    pitch_mult,
                    filter_offset: mods.filter_offset,
                    filter_offset_semis: mods.filter_offset_semis + delta.filter_cutoff,
                    amp_mod,
                }
            } else {
                *mods
            };
            let (l, r) = voice.tick(&voice_mods);
            out_l += l; out_r += r;
        }
        let vol = self.volume * self.volume; // perceptual curve (quadratic)
        (out_l * vol, out_r * vol)
    }

    fn set_param(&mut self, key: &str, value: f32) {
        match key {
            "filter_cutoff" => self.params.filter_cutoff = value,
            "filter_resonance" => self.params.filter_resonance = value,
            "filter_env_amount" => self.params.filter_env_amount = value,
            "filter_key_track" => self.params.filter_key_track = value,
            "amp_attack" => self.params.amp_attack = value,
            "amp_decay" => self.params.amp_decay = value,
            "amp_sustain" => self.params.amp_sustain = value,
            "amp_release" => self.params.amp_release = value,
            "filter_attack" => self.params.filter_attack = value,
            "filter_decay" => self.params.filter_decay = value,
            "filter_sustain" => self.params.filter_sustain = value,
            "filter_release" => self.params.filter_release = value,
            "noise_level" => self.params.noise_level = value,
            "osc_detune" => self.params.osc_detune = value,
            "lfo_rate" => self.params.lfo_rate = value,
            "lfo_pitch_depth" => self.params.lfo_pitch_depth = value,
            "lfo_filter_depth" => self.params.lfo_filter_depth = value,
            "lfo_amp_depth" => self.params.lfo_amp_depth = value,
            "vel_to_filter" => self.params.vel_to_filter = value,
            "portamento_time" => self.params.portamento_time = value,
            "unison_detune" => self.params.unison_detune = value,
            "chorus_mix" => self.params.chorus_mix = value,
            "delay_mix" => self.params.delay_mix = value,
            "delay_time_l" => self.params.delay_time_l = value,
            "delay_time_r" => self.params.delay_time_r = value,
            "delay_feedback" => self.params.delay_feedback = value,
            "delay_ping_pong" => self.params.delay_ping_pong = value,
            "delay_filter" => self.params.delay_filter = value,
            "reverb_mix" => self.params.reverb_mix = value,
            "reverb_room_size" => self.params.reverb_room_size = value,
            "reverb_damping" => self.params.reverb_damping = value,
            "reverb_width" => self.params.reverb_width = value,
            "reverb_pre_delay" => self.params.reverb_pre_delay = value,
            "ring_mod_freq" => self.params.ring_mod_freq = value,
            "ring_mod_shape" => self.params.ring_mod_shape = value,
            "ring_mod_bias" => self.params.ring_mod_bias = value,
            "ring_mod_linear" => self.params.ring_mod_linear = value,
            "ring_mod_mix" => self.params.ring_mod_mix = value,
            "freq_shift_hz" => self.params.freq_shift_hz = value,
            "freq_shift_feedback" => self.params.freq_shift_feedback = value,
            "freq_shift_delay" => self.params.freq_shift_delay = value,
            "freq_shift_mix" => self.params.freq_shift_mix = value,
            "tape_drive" => self.params.tape_drive = value,
            "tape_saturation" => self.params.tape_saturation = value,
            "tape_bias" => self.params.tape_bias = value,
            "tape_tone" => self.params.tape_tone = value,
            "tape_speed" => self.params.tape_speed = value,
            "tape_mix" => self.params.tape_mix = value,
            "neuron_drive" => self.params.neuron_drive = value,
            "neuron_squash" => self.params.neuron_squash = value,
            "neuron_stab" => self.params.neuron_stab = value,
            "neuron_asym" => self.params.neuron_asym = value,
            "neuron_bias" => self.params.neuron_bias = value,
            "neuron_comb_freq" => self.params.neuron_comb_freq = value,
            "neuron_comb_sep" => self.params.neuron_comb_sep = value,
            "neuron_mix" => self.params.neuron_mix = value,
            "spring_size" => self.params.spring_size = value,
            "spring_decay" => self.params.spring_decay = value,
            "spring_reflections" => self.params.spring_reflections = value,
            "spring_damping" => self.params.spring_damping = value,
            "spring_spin" => self.params.spring_spin = value,
            "spring_chaos" => self.params.spring_chaos = value,
            "spring_mix" => self.params.spring_mix = value,
            "reverb_type" => self.params.reverb_type = value,
            "env_attack_shape" => self.params.env_attack_shape = value,
            "env_decay_shape" => self.params.env_decay_shape = value,
            "env_release_shape" => self.params.env_release_shape = value,
            "filter_env_attack_shape" => self.params.filter_env_attack_shape = value,
            "filter_env_decay_shape" => self.params.filter_env_decay_shape = value,
            "filter_env_release_shape" => self.params.filter_env_release_shape = value,
            "lfo1_retrigger" => self.params.lfo1_retrigger = value,
            "lfo2_retrigger" => self.params.lfo2_retrigger = value,
            "lfo3_retrigger" => self.params.lfo3_retrigger = value,
            "lfo4_retrigger" => self.params.lfo4_retrigger = value,
            "lfo1_trigger_mode" => self.params.lfo1_trigger_mode = value,
            "lfo2_trigger_mode" => self.params.lfo2_trigger_mode = value,
            "lfo3_trigger_mode" => self.params.lfo3_trigger_mode = value,
            "lfo4_trigger_mode" => self.params.lfo4_trigger_mode = value,
            "lfo_deform" => self.params.lfo_deform = value,
            "slfo1_rate"       => self.params.slfo1_rate = value,
            "slfo1_waveform"   => self.params.slfo1_waveform = value,
            "slfo1_deform"     => self.params.slfo1_deform = value,
            "slfo1_tempo_sync" => self.params.slfo1_tempo_sync = value,
            "slfo1_unipolar"   => self.params.slfo1_unipolar = value,
            "slfo2_rate"       => self.params.slfo2_rate = value,
            "slfo2_waveform"   => self.params.slfo2_waveform = value,
            "slfo2_deform"     => self.params.slfo2_deform = value,
            "slfo2_tempo_sync" => self.params.slfo2_tempo_sync = value,
            "slfo2_unipolar"   => self.params.slfo2_unipolar = value,
            "twist_engine"     => self.params.twist_engine = value,
            "twist_harmonics"  => self.params.twist_harmonics = value,
            "twist_timbre"     => self.params.twist_timbre = value,
            "twist_morph"      => self.params.twist_morph = value,
            "twist_lpg_decay"  => self.params.twist_lpg_decay = value,
            "twist_lpg_colour" => self.params.twist_lpg_colour = value,
            "twist_aux_mix"    => self.params.twist_aux_mix = value,
            "lfo2_rate" => self.params.lfo2_rate = value,
            "lfo2_pitch_depth" => self.params.lfo2_pitch_depth = value,
            "lfo2_filter_depth" => self.params.lfo2_filter_depth = value,
            "lfo2_amp_depth" => self.params.lfo2_amp_depth = value,
            "lfo2_deform" => self.params.lfo2_deform = value,
            "lfo3_rate" => self.params.lfo3_rate = value,
            "lfo3_deform" => self.params.lfo3_deform = value,
            "lfo4_rate" => self.params.lfo4_rate = value,
            "lfo4_deform" => self.params.lfo4_deform = value,
            "fm_cross_depth" => self.params.fm_cross_depth = value,
            "seq_pitch_depth" => self.params.seq_pitch_depth = value,
            // In debug builds, fail loud on unknown keys so typos are caught
            // by tests instead of silently no-op'ing live parameter updates.
            // In release builds, ignore (cheap, no audio-thread side effects).
            _ => {
                debug_assert!(false, "GlobalParams::set: unknown key '{key}'");
            }
        }
    }
}

pub struct SynthEngine {
    parts: Vec<Part>,
    pub drum_engine: DrumEngine,
    pub sampler: SamplerEngine,
    pitch_bend_semitones: f32,
    mod_wheel: f32,
    vibrato_phase: f32,
    aftertouch: f32,
    aftertouch_smooth: f32,
    cc_values: [f32; 4],   // assignable CC sources for mod matrix (CC1..CC4)
    pub cc_numbers: [u8; 4],  // which MIDI CC numbers are assigned (default: 1,2,3,4)
    sample_rate: f32,
    // Effects chain (order: EQ → Compressor → Overdrive → Tape → Neuron → Phaser → Flanger → Tremolo → Bitcrusher → RingMod → FreqShift → Chorus → Delay → Reverb/Spring)
    pub eq: ParametricEq,
    pub compressor: Compressor,
    pub overdrive: Overdrive,
    pub tape: Tape,
    pub neuron: Neuron,
    pub phaser: Phaser,
    pub flanger: Flanger,
    pub tremolo: Tremolo,
    pub bitcrusher: Bitcrusher,
    pub ring_mod: RingMod,
    pub freq_shift: FreqShift,
    chorus: Chorus,
    rotary: RotarySpeaker,
    bbd_ensemble: BbdEnsemble,
    resonator: Resonator,
    bonsai: Bonsai,
    wave_shaper: WaveShaper,
    ms_tool: MsTool,
    graphic_eq_fx: GraphicEq,
    conditioner: Conditioner,
    exciter: Exciter,
    floaty_delay: FloatyDelay,
    reverb2: Reverb2,
    combulator: Combulator,
    treemonster: Treemonster,
    nimbus: Nimbus,
    vocoder: Vocoder,
    conv_reverb: ConvolutionReverb,
    airwindows: Airwindows,
    delay: StereoDelay,
    reverb: Reverb,
    pub spring_reverb: SpringReverb,
    pub arpeggiator: Arpeggiator,
    pub looper: MidiLooper,
    cc_map: CcMap,
    patches: Vec<Patch>,
    preset_params_cache: Vec<PatchParams>,
    feedback_tx: Option<rtrb::Producer<ParamFeedback>>,
    /// Where displaced heap objects go so the audio thread never calls free().
    trash: Option<crate::trash::TrashTx>,
    program_change: Option<std::sync::Arc<std::sync::atomic::AtomicU8>>,
    global_params: GlobalParams,
    pickup_states: [PickupState; 128],
    /// Drum-note mode gate for live pads: 0 = pads 36-51 trigger drums on ANY
    /// channel (drum_mode), nonzero = pads play parts. NOT a transport target —
    /// transport notes (81/82/83/123/124) always drive the master transport.
    seq_target: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Active part index (0-7), shared with GUI for looper part recording
    active_part: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Looper record source: true = Mix (all sounding enabled parts,
    /// default), false = only the active part (LooperSetSource; live
    /// transport state, not persisted).
    looper_source_mix: bool,
    /// Sustain pedal state (CC64 from pedal/fader or GUI toggle), shared with GUI
    sustain: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Static pedal-up release duration (None = patch release)
    sustain_release: Option<f32>,
    /// Oscilloscope ring buffer: 256 mono samples shared with GUI via atomics
    scope_buf: std::sync::Arc<ScopeBuffer>,
    scope_write: usize,
    /// MIDI file sequencer player
    pub midi_player: midi_player::MidiPlayer,
    /// Reusable event buffer for midi_player.process_block (avoids per-block allocation)
    midi_seq_buf: Vec<(u8, midi_player::SeqEventData)>,
    /// Per MIDI-file channel: 0 = DSP layer 0 took the last note-on,
    /// 1 = SF2 took it, 2 = drums. Note-offs must follow the note-on's
    /// route, even if the track instrument changed mid-note.
    seq_route: [u8; 16],
    /// Per-part modulation state, refilled each block in `tick_block`.
    /// Held as a struct field to avoid per-block `Vec` allocation.
    layer_mods: [ModulationState; MAX_PARTS],
}

/// Cached effect parameters — extracted once per block from PatchParams.
#[derive(Clone, Copy)]
struct CachedFxParams {
    // Tape
    tape_drive: f32, tape_saturation: f32, tape_bias: f32,
    tape_tone: f32, tape_speed: f32, tape_mix: f32,
    // Neuron
    neuron_drive: f32, neuron_squash: f32, neuron_stab: f32,
    neuron_asym: f32, neuron_bias: f32, neuron_comb_freq: f32,
    neuron_comb_sep: f32, neuron_mix: f32,
    // Ring mod
    ring_mod_freq: f32, ring_mod_shape: f32, ring_mod_bias: f32,
    ring_mod_linear: f32, ring_mod_mix: f32,
    // Freq shift
    freq_shift_hz: f32, freq_shift_feedback: f32,
    freq_shift_delay: f32, freq_shift_mix: f32,
    // Chorus
    chorus_mix: f32,
    // Delay
    delay_time_l: f32, delay_time_r: f32, delay_feedback: f32,
    delay_filter: f32, delay_ping_pong: f32, delay_mix: f32,
    // Reverb
    reverb_type: f32, reverb_mix: f32,
    reverb_room_size: f32, reverb_damping: f32,
    reverb_width: f32, reverb_pre_delay: f32,
    // Spring reverb
    spring_size: f32, spring_decay: f32, spring_reflections: f32,
    spring_damping: f32, spring_spin: f32, spring_chaos: f32,
    spring_mix: f32,
    // Rotary Speaker
    rotary_speed: f32, rotary_mix: f32,
    // BBD Ensemble
    ensemble_depth: f32, ensemble_rate: f32, ensemble_mix: f32,
    // Resonator
    resonator_freq: f32, resonator_decay: f32, resonator_mix: f32,
    // Bonsai
    bonsai_drive: f32, bonsai_tone: f32, bonsai_asym: f32,
    bonsai_mode: f32, bonsai_mix: f32,
    // WaveShaper
    wave_shaper_drive: f32, wave_shaper_mode: f32, wave_shaper_bias: f32, wave_shaper_mix: f32,
    // MS Tool
    ms_mid_gain: f32, ms_side_gain: f32, ms_rotation: f32, ms_mix: f32,
    // Graphic EQ
    graphic_eq_gains: [f32; 11], graphic_eq_output: f32,
    // Conditioner
    conditioner_bass_cut: f32, conditioner_width: f32, conditioner_threshold: f32, conditioner_mix: f32,
    // Exciter
    exciter_drive: f32, exciter_freq: f32, exciter_presence: f32, exciter_mix: f32,
    // Floaty Delay
    floaty_time: f32, floaty_feedback: f32, floaty_wobble: f32, floaty_rate: f32, floaty_damp: f32, floaty_mix: f32,
    // Reverb2
    reverb2_decay: f32, reverb2_damping: f32, reverb2_size: f32, reverb2_mix: f32,
    // Combulator
    combulator_freq: f32, combulator_offset2: f32, combulator_offset3: f32,
    combulator_feedback: f32, combulator_tone: f32, combulator_mix: f32,
    // Treemonster
    treemonster_threshold: f32, treemonster_shift: f32, treemonster_ring_mix: f32, treemonster_mix: f32,
    // Nimbus
    nimbus_position: f32, nimbus_size: f32, nimbus_pitch: f32, nimbus_density: f32,
    nimbus_spread: f32, nimbus_texture: f32, nimbus_mix: f32,
    // Vocoder
    vocoder_env_follow: f32, vocoder_gate: f32, vocoder_mix: f32,
    // Convolution Reverb
    conv_reverb_room: f32, conv_reverb_damping: f32, conv_reverb_predelay: f32, conv_reverb_mix: f32,
    // Airwindows
    airwindows_mode: u32, airwindows_drive: f32, airwindows_mix: f32,
}

impl CachedFxParams {
    fn from_preset(p: Option<&PatchParams>, global: &GlobalParams) -> Self {
        Self {
            tape_drive: p.map(|p| p.tape_drive).unwrap_or(0.0),
            tape_saturation: p.map(|p| p.tape_saturation).unwrap_or(0.5),
            tape_bias: p.map(|p| p.tape_bias).unwrap_or(0.5),
            tape_tone: p.map(|p| p.tape_tone).unwrap_or(0.5),
            tape_speed: p.map(|p| p.tape_speed).unwrap_or(0.5),
            tape_mix: p.map(|p| p.tape_mix).unwrap_or(0.0),
            neuron_drive: p.map(|p| p.neuron_drive).unwrap_or(0.0),
            neuron_squash: p.map(|p| p.neuron_squash).unwrap_or(0.5),
            neuron_stab: p.map(|p| p.neuron_stab).unwrap_or(0.5),
            neuron_asym: p.map(|p| p.neuron_asym).unwrap_or(0.0),
            neuron_bias: p.map(|p| p.neuron_bias).unwrap_or(0.5),
            neuron_comb_freq: p.map(|p| p.neuron_comb_freq).unwrap_or(200.0),
            neuron_comb_sep: p.map(|p| p.neuron_comb_sep).unwrap_or(0.5),
            neuron_mix: p.map(|p| p.neuron_mix).unwrap_or(0.0),
            ring_mod_freq: p.map(|p| p.ring_mod_freq).unwrap_or(440.0),
            ring_mod_shape: p.map(|p| p.ring_mod_shape).unwrap_or(0.0),
            ring_mod_bias: p.map(|p| p.ring_mod_bias).unwrap_or(0.5),
            ring_mod_linear: p.map(|p| p.ring_mod_linear).unwrap_or(0.5),
            ring_mod_mix: p.map(|p| p.ring_mod_mix).unwrap_or(0.0),
            freq_shift_hz: p.map(|p| p.freq_shift_hz).unwrap_or(0.0),
            freq_shift_feedback: p.map(|p| p.freq_shift_feedback).unwrap_or(0.0),
            freq_shift_delay: p.map(|p| p.freq_shift_delay).unwrap_or(0.0),
            freq_shift_mix: p.map(|p| p.freq_shift_mix).unwrap_or(0.0),
            chorus_mix: p.map(|p| p.chorus_mix).unwrap_or(0.0),
            delay_time_l: p.map(|p| p.delay_time_l).unwrap_or(0.3),
            delay_time_r: p.map(|p| p.delay_time_r).unwrap_or(0.4),
            delay_feedback: p.map(|p| p.delay_feedback).unwrap_or(0.4),
            delay_filter: p.map(|p| p.delay_filter).unwrap_or(0.3),
            delay_ping_pong: p.map(|p| p.delay_ping_pong).unwrap_or(0.0),
            delay_mix: global.delay_mix
                .unwrap_or_else(|| p.map(|p| p.delay_mix).unwrap_or(0.0)),
            reverb_type: p.map(|p| p.reverb_type).unwrap_or(0.0),
            reverb_mix: global.reverb_mix
                .unwrap_or_else(|| p.map(|p| p.reverb_mix).unwrap_or(0.0)),
            reverb_room_size: p.map(|p| p.reverb_room_size).unwrap_or(0.5),
            reverb_damping: p.map(|p| p.reverb_damping).unwrap_or(0.5),
            reverb_width: p.map(|p| p.reverb_width).unwrap_or(1.0),
            reverb_pre_delay: p.map(|p| p.reverb_pre_delay).unwrap_or(0.02),
            spring_size: p.map(|p| p.spring_size).unwrap_or(0.5),
            spring_decay: p.map(|p| p.spring_decay).unwrap_or(0.5),
            spring_reflections: p.map(|p| p.spring_reflections).unwrap_or(0.5),
            spring_damping: p.map(|p| p.spring_damping).unwrap_or(0.5),
            spring_spin: p.map(|p| p.spring_spin).unwrap_or(0.3),
            spring_chaos: p.map(|p| p.spring_chaos).unwrap_or(0.0),
            spring_mix: p.map(|p| p.spring_mix).unwrap_or(0.0),
            rotary_speed: p.map(|p| p.rotary_speed).unwrap_or(0.0),
            rotary_mix: p.map(|p| p.rotary_mix).unwrap_or(0.0),
            ensemble_depth: p.map(|p| p.ensemble_depth).unwrap_or(0.5),
            ensemble_rate: p.map(|p| p.ensemble_rate).unwrap_or(0.5),
            ensemble_mix: p.map(|p| p.ensemble_mix).unwrap_or(0.0),
            resonator_freq: p.map(|p| p.resonator_freq).unwrap_or(440.0),
            resonator_decay: p.map(|p| p.resonator_decay).unwrap_or(0.7),
            resonator_mix: p.map(|p| p.resonator_mix).unwrap_or(0.0),
            bonsai_drive: p.map(|p| p.bonsai_drive).unwrap_or(0.5),
            bonsai_tone: p.map(|p| p.bonsai_tone).unwrap_or(0.5),
            bonsai_asym: p.map(|p| p.bonsai_asym).unwrap_or(0.0),
            bonsai_mode: p.map(|p| p.bonsai_mode).unwrap_or(0.0),
            bonsai_mix: p.map(|p| p.bonsai_mix).unwrap_or(0.0),
            wave_shaper_drive: p.map(|p| p.wave_shaper_drive).unwrap_or(0.5),
            wave_shaper_mode: p.map(|p| p.wave_shaper_mode).unwrap_or(0.0),
            wave_shaper_bias: p.map(|p| p.wave_shaper_bias).unwrap_or(0.0),
            wave_shaper_mix: p.map(|p| p.wave_shaper_mix).unwrap_or(0.0),
            ms_mid_gain: p.map(|p| p.ms_mid_gain).unwrap_or(1.0),
            ms_side_gain: p.map(|p| p.ms_side_gain).unwrap_or(1.0),
            ms_rotation: p.map(|p| p.ms_rotation).unwrap_or(0.0),
            ms_mix: p.map(|p| p.ms_mix).unwrap_or(0.0),
            graphic_eq_gains: p.map(|p| p.graphic_eq_gains).unwrap_or([0.0; 11]),
            graphic_eq_output: p.map(|p| p.graphic_eq_output).unwrap_or(1.0),
            conditioner_bass_cut: p.map(|p| p.conditioner_bass_cut).unwrap_or(20.0),
            conditioner_width: p.map(|p| p.conditioner_width).unwrap_or(1.0),
            conditioner_threshold: p.map(|p| p.conditioner_threshold).unwrap_or(1.0),
            conditioner_mix: p.map(|p| p.conditioner_mix).unwrap_or(0.0),
            exciter_drive: p.map(|p| p.exciter_drive).unwrap_or(0.5),
            exciter_freq: p.map(|p| p.exciter_freq).unwrap_or(3000.0),
            exciter_presence: p.map(|p| p.exciter_presence).unwrap_or(0.5),
            exciter_mix: p.map(|p| p.exciter_mix).unwrap_or(0.0),
            floaty_time: p.map(|p| p.floaty_time).unwrap_or(0.3),
            floaty_feedback: p.map(|p| p.floaty_feedback).unwrap_or(0.3),
            floaty_wobble: p.map(|p| p.floaty_wobble).unwrap_or(0.3),
            floaty_rate: p.map(|p| p.floaty_rate).unwrap_or(0.5),
            floaty_damp: p.map(|p| p.floaty_damp).unwrap_or(0.5),
            floaty_mix: p.map(|p| p.floaty_mix).unwrap_or(0.0),
            reverb2_decay: p.map(|p| p.reverb2_decay).unwrap_or(0.5),
            reverb2_damping: p.map(|p| p.reverb2_damping).unwrap_or(0.5),
            reverb2_size: p.map(|p| p.reverb2_size).unwrap_or(0.5),
            reverb2_mix: p.map(|p| p.reverb2_mix).unwrap_or(0.0),
            combulator_freq: p.map(|p| p.combulator_freq).unwrap_or(440.0),
            combulator_offset2: p.map(|p| p.combulator_offset2).unwrap_or(5.0),
            combulator_offset3: p.map(|p| p.combulator_offset3).unwrap_or(-7.0),
            combulator_feedback: p.map(|p| p.combulator_feedback).unwrap_or(0.5),
            combulator_tone: p.map(|p| p.combulator_tone).unwrap_or(0.5),
            combulator_mix: p.map(|p| p.combulator_mix).unwrap_or(0.0),
            treemonster_threshold: p.map(|p| p.treemonster_threshold).unwrap_or(0.5),
            treemonster_shift: p.map(|p| p.treemonster_shift).unwrap_or(0.0),
            treemonster_ring_mix: p.map(|p| p.treemonster_ring_mix).unwrap_or(0.5),
            treemonster_mix: p.map(|p| p.treemonster_mix).unwrap_or(0.0),
            nimbus_position: p.map(|p| p.nimbus_position).unwrap_or(0.5),
            nimbus_size: p.map(|p| p.nimbus_size).unwrap_or(0.5),
            nimbus_pitch: p.map(|p| p.nimbus_pitch).unwrap_or(0.0),
            nimbus_density: p.map(|p| p.nimbus_density).unwrap_or(0.5),
            nimbus_spread: p.map(|p| p.nimbus_spread).unwrap_or(0.5),
            nimbus_texture: p.map(|p| p.nimbus_texture).unwrap_or(0.5),
            nimbus_mix: p.map(|p| p.nimbus_mix).unwrap_or(0.0),
            vocoder_env_follow: p.map(|p| p.vocoder_env_follow).unwrap_or(0.5),
            vocoder_gate: p.map(|p| p.vocoder_gate).unwrap_or(0.0),
            vocoder_mix: p.map(|p| p.vocoder_mix).unwrap_or(0.0),
            conv_reverb_room: p.map(|p| p.conv_reverb_room).unwrap_or(0.5),
            conv_reverb_damping: p.map(|p| p.conv_reverb_damping).unwrap_or(0.5),
            conv_reverb_predelay: p.map(|p| p.conv_reverb_predelay).unwrap_or(0.02),
            conv_reverb_mix: p.map(|p| p.conv_reverb_mix).unwrap_or(0.0),
            airwindows_mode: p.map(|p| p.airwindows_mode as u32).unwrap_or(0),
            airwindows_drive: p.map(|p| p.airwindows_drive).unwrap_or(0.5),
            airwindows_mix: p.map(|p| p.airwindows_mix).unwrap_or(0.0),
        }
    }
}

impl SynthEngine {
    pub fn new(sample_rate: f32) -> Self {
        // All 8 parts pre-allocated; only part 0 enabled by default
        let mut parts: Vec<Part> = (0..MAX_PARTS).map(|_| Part::new(sample_rate)).collect();
        parts[0].enabled = true; // parts 1-7 start disabled (zero CPU cost)
        Self {
            parts, drum_engine: DrumEngine::new(sample_rate),
            sampler: SamplerEngine::new(sample_rate),
            pitch_bend_semitones: 0.0, mod_wheel: 0.0, vibrato_phase: 0.0,
            aftertouch: 0.0, aftertouch_smooth: 0.0,
            cc_values: [0.0; 4], cc_numbers: [1, 2, 3, 4],
            sample_rate,
            eq: ParametricEq::new(sample_rate),
            compressor: Compressor::new(sample_rate),
            overdrive: Overdrive::new(sample_rate),
            tape: Tape::new(sample_rate),
            neuron: Neuron::new(sample_rate),
            phaser: Phaser::new(sample_rate),
            flanger: Flanger::new(sample_rate),
            tremolo: Tremolo::new(sample_rate),
            bitcrusher: Bitcrusher::new(sample_rate),
            ring_mod: RingMod::new(sample_rate),
            freq_shift: FreqShift::new(sample_rate),
            arpeggiator: Arpeggiator::new(sample_rate),
            looper: MidiLooper::new(sample_rate),
            chorus: Chorus::new(sample_rate),
            rotary: RotarySpeaker::new(sample_rate),
            bbd_ensemble: BbdEnsemble::new(sample_rate),
            resonator: Resonator::new(sample_rate),
            bonsai: Bonsai::new(sample_rate),
            wave_shaper: WaveShaper::new(sample_rate),
            ms_tool: MsTool::new(sample_rate),
            graphic_eq_fx: GraphicEq::new(sample_rate),
            conditioner: Conditioner::new(sample_rate),
            exciter: Exciter::new(sample_rate),
            floaty_delay: FloatyDelay::new(sample_rate),
            reverb2: Reverb2::new(sample_rate),
            combulator: Combulator::new(sample_rate),
            treemonster: Treemonster::new(sample_rate),
            nimbus: Nimbus::new(sample_rate),
            vocoder: Vocoder::new(sample_rate),
            conv_reverb: ConvolutionReverb::new(sample_rate),
            airwindows: Airwindows::new(sample_rate),
            delay: StereoDelay::new(sample_rate),
            reverb: Reverb::new(sample_rate),
            spring_reverb: SpringReverb::new(sample_rate),
            cc_map: CcMap::default(),
            patches: Vec::new(), preset_params_cache: Vec::new(),
            feedback_tx: None, trash: None, program_change: None,
            global_params: GlobalParams::default(),
            pickup_states: [PickupState::default(); 128],
            seq_target: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(1)), // default: non-drum mode
            active_part: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            looper_source_mix: true,
            sustain: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sustain_release: None,
            scope_buf: std::sync::Arc::new(ScopeBuffer::new()),
            scope_write: 0,
            midi_player: midi_player::MidiPlayer::new(),
            midi_seq_buf: Vec::new(),
            seq_route: [0u8; 16],
            layer_mods: [ModulationState {
                pitch_mult: 1.0, filter_offset: 0.0, filter_offset_semis: 0.0, amp_mod: 1.0,
            }; MAX_PARTS],
        }
    }

    pub fn seq_target_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.seq_target.clone()
    }

    pub fn active_part_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.active_part.clone()
    }

    pub fn sustain_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.sustain.clone()
    }

    /// Current SF2 release-time scale (TECHDEBT #7 introspection/tests).
    pub fn sampler_release_scale(&self) -> f32 {
        self.sampler.release_scale()
    }

    /// Apply sustain pedal state (CC64 from pedal/fader, or GUI toggle).
    /// On release, voices whose key is STILL HELD (`keys_down`) keep sounding —
    /// their own key-off will release them. Voices held by the pedal only
    /// (`sustained_notes`) and stale mono/latch entries are released/cleared;
    /// stale flags from a patch switch while the pedal was down would otherwise
    /// leave notes ringing forever.
    fn set_sustain(&mut self, value: u8) {
        let on = value >= 64;
        self.sustain.store(on, std::sync::atomic::Ordering::Relaxed);
        // SF2 voices: rustysynth applies hold-pedal per channel itself
        self.sampler.sustain_pedal(on);
        for part in &mut self.parts {
            part.sustain_pedal = if on { 1.0 } else { 0.0 };
            if !on {
                // Latch voices are released unconditionally (as before): a
                // latched note is defined by latch_held, not by a held key —
                // clearing latch_held below without releasing its keys_down
                // would leave a voice that its own key-off can never stop
                // (latch note_off early-returns).
                for &n in &part.latch_held {
                    part.keys_down[n as usize] = false;
                }
                if let Some(secs) = self.sustain_release {
                    // Static pedal-up release (TECHDEBT #7): pedal-released
                    // voices take `secs` instead of the patch release.
                    // Key-held voices (`keys_down`) are untouched — same
                    // predicate as the plain path below. Two passes over a
                    // stack bitmap keep the borrows of `part` disjoint.
                    let mut needs = [false; 128];
                    for voice in &part.voices {
                        if voice.active && !voice.is_releasing()
                            && !part.keys_down[voice.src_note as usize]
                        {
                            needs[voice.note as usize] = true;
                        }
                    }
                    for (note, &held) in needs.iter().enumerate() {
                        if held { part.note_off_for(note as u8, secs); }
                    }
                } else {
                    for voice in &mut part.voices {
                        if voice.active && !voice.is_releasing()
                            && !part.keys_down[voice.src_note as usize]
                        {
                            voice.note_off();
                        }
                    }
                }
                part.sustained_notes = [false; 128];
                // Pedal-only holds are gone; stale mono/latch entries would
                // resurrect on the next key press (mono retrigger).
                part.note_stack.clear();
                part.latch_held.clear();
            }
        }
    }



    /// Dispatches an arp note-off to every enabled part with part transpose
    /// applied: SF2 channels are keyed by transposed pitch, DSP parts receive
    /// the raw note (they match offs by src_note). Shared by flush_arp_note_off
    /// and the panic paths (CC120/123, AllNotesOff, SetArpParams-off) so the
    /// SF2 gate voice always gets a matching off, transposed like its on.
    fn dispatch_arp_note_off(&mut self, off: u8) {
        for (i, part) in self.parts.iter_mut().enumerate() {
            if !part.enabled { continue; }
            let transposed = (off as i16 + part.transpose as i16).clamp(0, 127) as u8;
            if part.sf2_mode { self.sampler.note_off(i, transposed); }
            else { part.note_off(off); } // raw: part matches by src_note
        }
    }

    /// Shared part note-on dispatch: range/velocity gate, root-only gate
    /// (TECHDEBT #6), DSP voice or SF2 sampler. The enabled/mute guards stay
    /// at the call sites, which intentionally differ (offs skip mute).
    fn route_note_on(&mut self, i: usize, raw: u8, velocity: u8) {
        let (sf2_mode, transpose, lo, hi, vmin, vmax) = {
            let p = &self.parts[i];
            (p.sf2_mode, p.transpose, p.min_note, p.max_note, p.vel_min, p.vel_max)
        };
        if raw < lo || raw > hi || velocity < vmin || velocity > vmax { return; }
        let RootDecision::Sound { release, note } = self.parts[i].root_on(raw) else { return };
        let tr = |n: u8| (n as i16 + transpose as i16).clamp(0, 127) as u8;
        if sf2_mode {
            if let Some(old) = release { self.sampler.note_off(i, tr(old)); }
            self.sampler.note_on(i, tr(note), velocity);
        } else {
            if let Some(old) = release {
                // The replaced root leaves outright: a bass layer must not
                // pile up pedal-sustained roots.
                let part = &mut self.parts[i];
                for v in &mut part.voices {
                    if v.active && v.src_note == old { v.note_off(); }
                }
                part.sustained_notes[old as usize] = false;
            }
            self.parts[i].note_on(note, velocity);
        }
    }

    /// Shared part note-off dispatch with root-only re-keying. Offs skip the
    /// range check (matching today's behavior) so a range change mid-note
    /// cannot orphan a voice.
    fn route_note_off(&mut self, i: usize, raw: u8) {
        let (sf2_mode, transpose, root_only, last_vel) = {
            let p = &self.parts[i];
            (p.sf2_mode, p.transpose, p.root_only, p.last_velocity)
        };
        let next = self.parts[i].root_off(raw);
        let tr = |n: u8| (n as i16 + transpose as i16).clamp(0, 127) as u8;
        if sf2_mode {
            self.sampler.note_off(i, tr(raw));
            if let Some(n) = next { self.sampler.note_on(i, tr(n), (last_vel * 127.0) as u8); }
        } else {
            self.parts[i].note_off(raw);
            if root_only {
                // Bass-split semantics (TECHDEBT #6): key-up = the note is DONE.
                // Force-release the root voice even when the pedal holds it and
                // clear its sustained_notes flag — the pedal must not resurrect
                // a bass root whose key the player has already let go (that
                // would drone the old root under the re-keyed one). Same for
                // the replaced root in route_note_on above.
                let part = &mut self.parts[i];
                for v in &mut part.voices {
                    if v.active && v.src_note == raw { v.note_off(); }
                }
                part.sustained_notes[raw as usize] = false;
                if let Some(n) = next { part.note_on(n, (last_vel * 127.0) as u8); }
            }
        }
    }

    /// Last held arp key released: the arpeggiator resets pattern_len, so
    /// tick() will never dispatch the pending gate-off — the sounding voice
    /// would hang forever. Dispatch the off here (same routing as tick_block).
    fn flush_arp_note_off(&mut self) {
        if self.arpeggiator.keeps_playing() {
            return; // tick() still owns gate-offs (includes a latched/frozen set)
        }
        if let Some(off) = self.arpeggiator.flush_current_note() {
            self.dispatch_arp_note_off(off);
            // Record per part (same source-consistency as the note dispatch
            // loop) — used to be a single event tagged with the active part.
            for i in 0..self.parts.len() {
                if !self.parts[i].enabled { continue; }
                self.record_for_looper(off, 0, i as u8);
            }
        }
    }

    /// Routes the note-offs returned by looper mute/solo mutators to the
    /// recorded part (SF2 or DSP), so muting stops what is sounding.
    fn route_looper_note_offs(&mut self, note_offs: &[(u8, u8, u8)]) {
        for &(note, vel, part_id) in note_offs {
            if part_id == 255 { break; }
            if vel == 0 {
                if let Some(part) = self.parts.get_mut(part_id as usize) {
                    if part.sf2_mode {
                        self.sampler.note_off(part_id as usize, note);
                    } else {
                        part.note_off(note);
                    }
                }
            }
        }
    }

    /// Looper record gate (Source toggle): in Mix source (default) every
    /// sounding part is recorded — the note dispatch loop tags each event
    /// with its own part id; in Part source only the active part is
    /// recorded. RT-safe: one atomic load + branch.
    #[inline]
    fn record_for_looper(&mut self, note: u8, velocity: u8, part: u8) {
        if !self.looper_source_mix
            && part != self.active_part.load(std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        self.looper.record_event(note, velocity, part);
    }

    /// Master transport (#32): start both transports (drums + looper).
    fn master_start(&mut self) {
        self.drum_engine.set_playing(true);
        if matches!(self.looper.state, crate::synth::looper::LooperState::Idle) {
            self.looper.toggle_play();
        }
    }

    /// Master transport: stop both; a recording looper finishes its layer
    /// first, then stops (stop() flags needs_note_offs; actual note-offs
    /// drain on the next looper tick, как и в ручном Stop).
    pub fn master_stop(&mut self) {
        self.drum_engine.set_playing(false);
        self.drum_engine.sequencer.reset();
        match self.looper.state {
            crate::synth::looper::LooperState::Recording
            | crate::synth::looper::LooperState::Overdubbing => {
                self.looper.toggle_record();
                self.looper.toggle_play();
            }
            crate::synth::looper::LooperState::Playing => self.looper.toggle_play(),
            _ => {}
        }
    }

    /// Master transport: toggle play — if anything plays, stop everything;
    /// otherwise start everything.
    pub fn master_toggle_play(&mut self) {
        let any = self.drum_engine.sequencer.playing
            || matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Playing
                    | crate::synth::looper::LooperState::Overdubbing
                    | crate::synth::looper::LooperState::Recording
            );
        if any {
            self.master_stop();
        } else {
            self.master_start();
        }
    }

    /// Master transport: toggle record arm on both (drums auto-play, looper
    /// Idle→Recording / Playing→Overdubbing).
    pub fn master_toggle_record(&mut self) {
        let rec = self.drum_engine.sequencer.recording
            || matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Recording
                    | crate::synth::looper::LooperState::Overdubbing
            );
        if rec {
            self.drum_engine.set_recording(false);
            if matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Recording
                    | crate::synth::looper::LooperState::Overdubbing
            ) {
                self.looper.toggle_record();
            }
        } else {
            self.drum_engine.set_recording(true);
            if !self.drum_engine.sequencer.playing {
                self.drum_engine.set_playing(true);
            }
            if matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Idle | crate::synth::looper::LooperState::Playing
            ) {
                self.looper.toggle_record();
            }
        }
    }

    pub fn pitch_seq_step_atoms(&self) -> Vec<std::sync::Arc<std::sync::atomic::AtomicU8>> {
        self.parts.iter().map(|l| l.pitch_seq.step_atom()).collect()
    }

    pub fn midi_player_play_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.midi_player.play_atom.clone()
    }

    pub fn midi_player_pos_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU32> {
        self.midi_player.position_atom.clone()
    }

    pub fn set_patches(&mut self, patches: Vec<Patch>) {
        self.preset_params_cache = patches.iter().map(|p| PatchParams::from_map(&p.params)).collect();
        self.patches = patches;
    }
    pub fn set_feedback_tx(&mut self, tx: rtrb::Producer<ParamFeedback>) { self.feedback_tx = Some(tx); }
    /// Gives the engine somewhere to put heap objects it displaces (soundfonts,
    /// rustysynth instances) so they are not freed on the audio thread.
    pub fn set_trash_tx(&mut self, tx: crate::trash::TrashTx) { self.trash = Some(tx); }

    /// Sends a displaced soundfont to the GUI thread for freeing. A no-op when
    /// nothing was displaced, and a plain drop when no queue has been wired up
    /// (offline tests and benchmarks, where the audio thread does not exist).
    fn dispose_sf2(&mut self, displaced: sampler::Displaced) {
        if displaced.is_empty() { return; }
        match self.trash.as_mut() {
            Some(t) => t.dispose(crate::trash::Trash::Sf2(displaced)),
            None => drop(displaced),
        }
    }
    pub fn scope_buffer(&self) -> std::sync::Arc<ScopeBuffer> { self.scope_buf.clone() }
    pub fn set_program_change_atom(&mut self, atom: std::sync::Arc<std::sync::atomic::AtomicU8>) {
        self.program_change = Some(atom);
    }

    fn send_feedback(&mut self, fb: ParamFeedback) {
        if let Some(tx) = &mut self.feedback_tx { let _ = tx.push(fb); }
    }

    /// Reset pickup for patch-scoped CCs (call after patch change).
    fn reset_preset_pickups(&mut self) {
        for (cc, state) in self.pickup_states.iter_mut().enumerate() {
            if let Some(binding) = self.cc_map.bindings[cc] {
                if binding.scope == ParamScope::Patch {
                    state.picked_up = false;
                }
            }
        }
    }

    /// Check if a patch-scoped CC has "picked up" the current value (crossed over it).
    /// binding is Copy — no heap allocation.
    fn check_pickup(&mut self, cc: u8, new_cc_value: u8, binding: crate::cc_map::CcBinding) -> bool {
        if binding.scope == ParamScope::Global {
            return true; // globals always apply immediately
        }
        let state = &mut self.pickup_states[cc as usize];
        if state.picked_up {
            state.last_cc_value = Some(new_cc_value);
            return true;
        }
        // Get current param value and convert to CC equivalent
        let current_val = self.parts.first()
            .and_then(|l| match binding.param_key {
                "filter_cutoff" => Some(l.params.filter_cutoff),
                "filter_resonance" => Some(l.params.filter_resonance),
                "amp_attack" => Some(l.params.amp_attack),
                "amp_release" => Some(l.params.amp_release),
                _ => None,
            })
            .unwrap_or(0.0);
        let target_cc = CcMap::param_to_cc(&binding, current_val);

        if let Some(last) = state.last_cc_value {
            let crossed = (last <= target_cc && new_cc_value >= target_cc)
                || (last >= target_cc && new_cc_value <= target_cc);
            if crossed {
                state.picked_up = true;
                state.last_cc_value = Some(new_cc_value);
                // Notify GUI that pickup completed
                if let Some(ref mut tx) = self.feedback_tx {
                    let _ = tx.push(ParamFeedback::PickupDone { key: binding.param_key });
                }
                return true;
            }
        }
        // Send knob position so GUI shows direction indicator
        let cc_pos = CcMap::cc_to_param(&binding, new_cc_value);
        if let Some(ref mut tx) = self.feedback_tx {
            let _ = tx.push(ParamFeedback::PickupPending { key: binding.param_key, cc_position: cc_pos });
        }
        state.last_cc_value = Some(new_cc_value);
        false
    }

    pub fn handle_event(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::NoteOn { channel, note, velocity } => {
                // SMK-37 Pro transport: SEQ buttons (any channel, notes
                // 123/124) and bank-2 pads ch9 (82 = pad 25 play, 83 = pad 27
                // rec, 81 = pad 26 stop) always drive the master transport.
                // Pad notes must not fall into the trigger path.
                if velocity > 0 && (note == 123 || (channel == 9 && note == 82)) {
                    // Master transport (#32 фаза 4): single transport, no target
                    self.master_toggle_play();
                    return;
                }
                if velocity > 0 && (note == 124 || (channel == 9 && note == 83)) {
                    self.master_toggle_record();
                    return;
                }
                if channel == 9 && velocity > 0 && note == 81 {
                    self.master_stop();
                    return;
                }

                // Route drum notes (36-51): always on ch10, or any channel when drum mode active
                let is_drum_note = (36..=51).contains(&note);
                let drum_mode = self.seq_target.load(std::sync::atomic::Ordering::Relaxed) == 0;
                let sampler_drums = self.sampler.drums_enabled();
                if channel == 9 || (is_drum_note && drum_mode) {
                    // Pad bank 2 legacy undo (64, E4) lives here; 83/82/81
                    // moved above — master transport now, no target.
                    if channel == 9 && velocity > 0 && note == 64 {
                        self.drum_engine.sequencer.undo();
                        return;
                    }
                    if velocity > 0 {
                        let velocity = if self.drum_engine.fixed_velocity { 127 } else { velocity };
                        let velocity = self.drum_engine.pad_velocity(note, velocity);
                        let note = self.drum_engine.remapped_note(note);
                        // Record into sequencer if recording
                        // `note` here is the REMAPPED target (30..=55 legal). Only
                        // 36..=51 maps to sequencer slots; skip recording other
                        // targets (drop, don't clamp — clamping would corrupt
                        // a neighbouring slot's pattern).
                        if is_drum_note && (36..=51).contains(&note) {
                            let slot = (note - 36) as usize;
                            let sr = self.sample_rate;
                            if let Some((pat, _slot, step)) =
                                self.drum_engine.sequencer.record_hit(slot, velocity, sr)
                            {
                                if let Some(ref mut tx) = self.feedback_tx {
                                    let _ = tx.push(ParamFeedback::DrumStepRecorded {
                                        pattern: pat, slot: _slot, step, velocity,
                                    });
                                }
                            }
                        }
                        // Drum hits into the looper too (#32): tagged with a
                        // drum marker (part 254) — replay triggers the drum
                        // engine, not a part. In Source=Part mode the
                        // record_for_looper filter drops them (254 != active
                        // part), so drums are captured in Mix mode only.
                        self.record_for_looper(note, velocity, 254);
                        if sampler_drums {
                            let choke = self.drum_engine.sf2_choke_take(note);
                            if choke != 0 { self.sampler.drum_note_off(choke); }
                            self.sampler.drum_note_on(note, velocity);
                        } else {
                            self.drum_engine.note_on(note, velocity);
                        }
                    } else {
                        let note = self.drum_engine.remapped_note(note);
                        if sampler_drums {
                            self.sampler.drum_note_off(note);
                        } else {
                            self.drum_engine.note_off(note);
                        }
                    }
                } else if self.arpeggiator.enabled {
                    // Route through arpeggiator — it generates notes in tick_block
                    if velocity == 0 {
                        self.arpeggiator.note_off(note);
                        self.flush_arp_note_off();
                    } else {
                        self.arpeggiator.note_on(note, velocity);
                    }
                } else if velocity == 0 {
                    for i in 0..self.parts.len() {
                        if !self.parts[i].enabled { continue; }
                        self.route_note_off(i, note);
                        // Whole-performance recording: tag with the part that
                        // receives the off (looper replay routes per part id).
                        self.record_for_looper(note, 0, i as u8);
                    }
                } else {
                    for i in 0..self.parts.len() {
                        if !self.parts[i].enabled || self.parts[i].mute { continue; }
                        self.route_note_on(i, note, velocity);
                        // Whole-performance recording: tag the event with the
                        // part that actually sounds it (looper replay routes
                        // per part id).
                        //
                        // Accepted effects: (1) the matching off events below
                        // are also recorded for parts that never sounded the
                        // note (off loops don't range-check) — replayed offs
                        // are harmless no-ops but consume looper event slots;
                        // (2) split overlap (two parts covering the same note)
                        // overwrites looper's single active_notes[note] slot —
                        // the forced off at loop wrap releases only the last
                        // part.
                        self.record_for_looper(note, velocity, i as u8);
                    }
                }
            }
            MidiEvent::NoteOff { channel, note } => {
                if channel == 9 {
                    let note = self.drum_engine.remapped_note(note);
                    if self.sampler.drums_enabled() {
                        self.sampler.drum_note_off(note);
                    } else {
                        self.drum_engine.note_off(note);
                    }
                } else if self.arpeggiator.enabled {
                    self.arpeggiator.note_off(note);
                    self.flush_arp_note_off();
                } else {
                    for i in 0..self.parts.len() {
                        self.route_note_off(i, note);
                        // Whole-performance recording: tag with the part that
                        // receives the off (looper replay routes per part id).
                        self.record_for_looper(note, 0, i as u8);
                    }
                }
            }
            MidiEvent::PitchBend { value, .. } => {
                // Asymmetric bend: per-part patch overrides global range
                let layer_up   = self.parts.iter().find(|l| l.enabled).map(|l| l.params.pitch_bend_up).unwrap_or(0.0);
                let layer_down = self.parts.iter().find(|l| l.enabled).map(|l| l.params.pitch_bend_down).unwrap_or(0.0);
                self.pitch_bend_semitones = if value >= 0.0 {
                    let up = if layer_up > 0.0 { layer_up }
                        else if self.global_params.pitch_bend_up > 0.0 { self.global_params.pitch_bend_up }
                        else { self.global_params.pitch_bend_range };
                    value * up
                } else {
                    let down = if layer_down > 0.0 { layer_down }
                        else if self.global_params.pitch_bend_down > 0.0 { self.global_params.pitch_bend_down }
                        else { self.global_params.pitch_bend_range };
                    value * down
                };
                for i in 0..2 {
                    if self.parts.get(i).map(|l| l.sf2_mode).unwrap_or(false) {
                        self.sampler.pitch_bend(i, value);
                    }
                }
            }
            MidiEvent::ModWheel { value, .. } => {
                self.mod_wheel = value;
                // Forward to SF2 sampler as CC1 on active SF2 parts
                for i in 0..2 {
                    if self.parts.get(i).map(|l| l.sf2_mode).unwrap_or(false) {
                        self.sampler.mod_wheel(i, value);
                    }
                }
            }
            MidiEvent::ProgramChange { program, .. } => {
                // In SF2 mode, change the SF2 program; in synth mode, change the synth patch
                if self.parts.first().map(|l| l.sf2_mode).unwrap_or(false) {
                    self.sampler.set_part_program(0, program, 0);
                } else if let Some(&params) = self.preset_params_cache.get(program as usize) {
                    if let Some(l) = self.parts.get_mut(0) {
                        // Seamless switch: same latch-mismatch release as
                        // LoadPatch — must run BEFORE the params swap, while
                        // l.params still holds the OLD play_mode.
                        if l.params.play_mode as u8 == 3 && params.play_mode as u8 != 3 {
                            for voice in &mut l.voices {
                                if voice.active && l.latch_held.contains(&voice.src_note) {
                                    voice.note_off();
                                }
                            }
                            l.latch_held.clear();
                        }
                        l.params = params;
                        l.min_note = params.min_note as u8;
                        l.max_note = params.max_note as u8;
                        if let Some(patch) = self.patches.get(program as usize) {
                            l.mod_matrix.load_from_params(&patch.params);
                            l.pitch_seq.load_from_params(&patch.params);
                        }
                        l.update_routing_cache();
                    }
                    self.reset_preset_pickups();
                }
                if let Some(atom) = &self.program_change {
                    atom.store(program, std::sync::atomic::Ordering::Relaxed);
                }
                self.send_feedback(ParamFeedback::ProgramChanged { program });
            }
            MidiEvent::Aftertouch { channel, value } => {
                if channel != 9 { self.aftertouch = value; }
            }
            MidiEvent::PolyAftertouch { channel, note, pressure } => {
                if channel != 9 {
                    for part in &mut self.parts {
                        for voice in &mut part.voices {
                            // src_note = raw key; poly AT was always sent raw
                            if voice.src_note == note && voice.active {
                                voice.poly_aftertouch = pressure;
                            }
                        }
                    }
                }
            }
            MidiEvent::ControlChange { cc, value, .. } => {
                // Channel mode messages. 120 = All Sound Off, 123 = All Notes
                // Off. Hardware sends these from the panic button and when the
                // transport stops, so they take priority over any CC mapping.
                if cc == 120 || cc == 123 {
                    // Lift the sustain state everywhere first: rustysynth
                    // gates CC123 on the channel hold pedal, and the GUI
                    // sustain atom must not stay stuck ON.
                    self.set_sustain(0);
                    let all_sound_off = cc == 120; // All Sound Off kills, All Notes Off releases
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        part.drain_notes(&mut self.sampler, i, all_sound_off);
                    }
                    if all_sound_off {
                        self.sampler.all_sound_off();
                    } else {
                        self.sampler.all_notes_off();
                    }
                    self.drum_engine.reset_choke_state();
                    if let Some(off_note) = self.arpeggiator.flush_current_note() {
                        // Same transposed routing as flush_arp_note_off.
                        self.dispatch_arp_note_off(off_note);
                    }
                    // Panic must also drop held/latched arp notes — held
                    // notes surviving CC120 would resume the old pattern on
                    // the next key (AllNotesOff already clears, mod.rs:2211).
                    self.arpeggiator.clear();
                    return;
                }
                // Check assignable mod sources (CC1..CC4)
                for i in 0..4 {
                    if cc == self.cc_numbers[i] {
                        self.cc_values[i] = value as f32 / 127.0;
                    }
                }
                // Dedicated mod sources: Breath (CC2), Expression (CC11), Sustain (CC64)
                if cc == 64 {
                    self.set_sustain(value);
                } else {
                    for part in &mut self.parts {
                        match cc {
                            2  => part.breath = value as f32 / 127.0,
                            11 => part.expression = value as f32 / 127.0,
                            _ => {}
                        }
                    }
                }
                // CcBinding is Copy — no heap allocation
                // Defense in depth: even a hand-edited config binding on CC64
                // must not steal the sustain pedal.
                let binding = if cc == 64 { None } else { self.cc_map.bindings[cc as usize] };
                if let Some(binding) = binding {
                    if self.check_pickup(cc, value, binding) {
                        let param_val = CcMap::cc_to_param(&binding, value);
                        if binding.scope == ParamScope::Global {
                            self.global_params.set(binding.param_key, param_val);
                        } else {
                            for part in &mut self.parts { part.set_param(binding.param_key, param_val); }
                        }
                        self.send_feedback(ParamFeedback::ParamChanged { key: binding.param_key, value: param_val });
                    }
                }
                self.send_feedback(ParamFeedback::CcReceived { cc });
            }
            MidiEvent::Navigate { pressed } => {
                self.send_feedback(if pressed {
                    ParamFeedback::NavigatePress
                } else {
                    ParamFeedback::NavigateRelease
                });
            }
        }
    }

    /// Applies a control event.
    ///
    /// Returns the event back when it owns heap memory, so the caller can hand
    /// it to the trash queue instead of calling free() on the audio thread.
    /// Returns `None` for the plain-data variants, which cost nothing to drop.
    #[must_use = "heap-owning events must be sent to the trash queue"]
    pub fn handle_control(&mut self, event: ControlEvent) -> Option<ControlEvent> {
        match event {
            ControlEvent::LoadPatch(mut ev) => {
                // Take the contents out of the box rather than unboxing it:
                // `*ev` would drop the allocation right here, on the audio
                // thread. The emptied box travels back to the GUI thread.
                let part = ev.part;
                let params = ev.params; // PatchParams is Copy
                let mod_matrix = std::mem::take(&mut ev.mod_matrix);
                let mseg1 = ev.mseg1.take();
                let mseg2 = ev.mseg2.take();
                let pitch_seq = ev.pitch_seq.take();
                let lfo_step_seq = ev.lfo_step_seq.take();
                let wavetable = ev.wavetable.take();
                if let Some(l) = self.parts.get_mut(part) {
                    // Seamless switch (TECHDEBT: политика «хвосты живы»):
                    // sounding voices keep the OLD patch (params are copied
                    // into the voice at note_on); pedal-up / key-offs release
                    // them naturally (set_sustain:1484-1518, note_off:835).
                    // Only a latch-mode mismatch releases the latch-held
                    // voices — a non-latch patch cannot toggle them off, and
                    // they would ring forever (audit fact 6).
                    if l.params.play_mode as u8 == 3 && params.play_mode as u8 != 3 {
                        for voice in &mut l.voices {
                            if voice.active && l.latch_held.contains(&voice.src_note) {
                                voice.note_off();
                            }
                        }
                        l.latch_held.clear();
                    }
                    l.min_note = params.min_note as u8;
                    l.max_note = params.max_note as u8;
                    // Sync macro values from preset into live part state
                    // Load wavetable data if provided. `replace` hands the
                    // previous Arc back into the event instead of dropping the
                    // last reference here — a wavetable is a Vec<f32> of tens
                    // of thousands of samples.
                    let previous_wt = match wavetable {
                        Some((data, frames, fsize)) => {
                            l.wavetable_frames = frames;
                            l.wavetable_frame_size = fsize;
                            l.wavetable_data.replace(data)
                        }
                        None => l.wavetable_data.take(),
                    };
                    ev.wavetable = previous_wt.map(|d| (d, 0, 0));
                    l.macro_vals = params.macro_vals;
                    l.params = params;
                    l.mod_matrix = mod_matrix;
                    // Mseg owns a Vec<MsegSegment>, so plain assignment would
                    // free the outgoing envelope on the audio thread. Swap it
                    // into the event and let the GUI thread drop it.
                    if let Some(mut m) = mseg1 {
                        std::mem::swap(&mut l.mseg1, &mut m);
                        ev.mseg1 = Some(m);
                    }
                    if let Some(mut m) = mseg2 {
                        std::mem::swap(&mut l.mseg2, &mut m);
                        ev.mseg2 = Some(m);
                    }
                    // Copy the pattern, keep our own step atom: see
                    // PitchSequencer::adopt.
                    if let Some(sq) = pitch_seq {
                        l.pitch_seq.adopt(&sq);
                        ev.pitch_seq = Some(sq);
                    }
                    if let Some(ref sp) = lfo_step_seq {
                        for (i, lfo) in l.lfos.iter_mut().enumerate().take(4) {
                            lfo.apply_step_seq(&sp[i]);
                        }
                    }
                    l.update_routing_cache();
                    // Sync arpeggiator params from loaded patch
                    self.arpeggiator.set_params(
                        params.arp_enabled > 0.5,
                        params.arp_mode as u8,
                        params.arp_rate as u8,
                        params.arp_octaves as u8,
                        params.arp_gate,
                    );
                    self.arpeggiator.set_latch(params.arp_latch > 0.5);
                }
                self.reset_preset_pickups();
                return Some(ControlEvent::LoadPatch(ev));
            }
            ControlEvent::SetPartEnabled { part, enabled } => {
                if let Some(l) = self.parts.get_mut(part) {
                    if !enabled {
                        l.drain_notes(&mut self.sampler, part, false);
                    }
                    l.enabled = enabled;
                }
            }
            ControlEvent::SetPartMute { part, mute } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.mute = mute;
                    if mute {
                        // Drain both voice owners + held-note state; SF2 output
                        // is summed globally in tick_block, so also gate at the
                        // source with CC7 (instant; the release drain above
                        // keeps the tail short).
                        l.drain_notes(&mut self.sampler, part, false);
                        self.sampler.set_layer_volume(part, 0.0);
                    } else {
                        // Restore the SF2 channel volume zeroed on mute.
                        self.sampler.set_layer_volume(part, l.volume);
                    }
                }
            }
            ControlEvent::SetPartRootOnly { part, on } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.root_only = on;
                    // Reset tracking both ways: a stale root_active across the
                    // toggle would re-key on the next unrelated note-off.
                    l.root_held = [false; 128];
                    l.root_active = None;
                }
            }
            ControlEvent::SetPartVolume { part, volume } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.volume = volume;
                    // Muted parts are gated at the sampler with CC7 (see
                    // SetPartMute); keep the gate closed or a volume-knob
                    // tweak would un-mute the part while l.mute stays true.
                    self.sampler.set_layer_volume(part, if l.mute { 0.0 } else { volume });
                }
            }
            ControlEvent::SetPartRange { part, min_note, max_note } => {
                if let Some(l) = self.parts.get_mut(part) { l.min_note = min_note; l.max_note = max_note; }
            }
            ControlEvent::SetPartVelRange { part, vel_min, vel_max } => {
                if let Some(l) = self.parts.get_mut(part) { l.vel_min = vel_min; l.vel_max = vel_max; }
            }
            ControlEvent::SetPartPan { part, pan } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pan = pan.clamp(-1.0, 1.0);
                    // SF2: pass pan through as CC10 on the part's sampler channel
                    // (rustysynth applies per-channel pan internally).  DSP parts
                    // get constant-power pan in tick_block.
                    if l.sf2_mode {
                        let cc = (l.pan.clamp(-1.0, 1.0) * 0.5 + 0.5) * 127.0;
                        self.sampler.set_channel_pan(part, cc as i32);
                    }
                }
            }
            ControlEvent::SetPartTranspose { part, semitones } => {
                // DSP voices match note-offs by src_note (raw), so held voices
                // stay releasable across the change. SF2 voices transpose
                // engine-side and are covered by the state-change drains
                // (SetPartSf2Mode/ProgramChange/panic) — no extra action here.
                if let Some(l) = self.parts.get_mut(part) { l.transpose = semitones; }
            }
            ControlEvent::SetPartMono { part, mono } => {
                if let Some(l) = self.parts.get_mut(part) {
                    let cur = l.params.play_mode as u8;
                    match (mono, cur) {
                        (true, 0) | (true, 6) => l.params.play_mode = 1.0, // poly/piano → mono
                        (true, 2) => l.params.play_mode = 1.0,             // mono-ST → mono
                        (false, 1) | (false, 2) => l.params.play_mode = 0.0, // → poly
                        _ => {} // already in the requested state; latch/high/low untouched
                    }
                    // A stale mono stack from the previous mode must not
                    // resurrect notes in the new mode.
                    l.note_stack.clear();
                }
            }
            ControlEvent::SetPartLegato { part, legato } => {
                if let Some(l) = self.parts.get_mut(part) {
                    let cur = l.params.play_mode as u8;
                    if legato && cur == 1 {
                        l.params.play_mode = 2.0;
                    } else if !legato && cur == 2 {
                        l.params.play_mode = 1.0;
                    }
                }
            }
            ControlEvent::SetPartGlide { part, ms } => {
                if let Some(l) = self.parts.get_mut(part) {
                    let secs = (ms / 1000.0).clamp(0.0, 2.0);
                    l.params.portamento_time = secs;
                    if secs > 0.0 && l.params.portamento_mode < 0.5 {
                        l.params.portamento_mode = 1.0; // "Always" — a glide time without a mode is dead config
                    }
                }
            }
            ControlEvent::SetMacro { part, index, value } => {
                if let Some(l) = self.parts.get_mut(part) {
                    if index < 8 {
                        l.macro_vals[index] = value.clamp(0.0, 1.0);
                        l.params.macro_vals[index] = value.clamp(0.0, 1.0);
                    }
                }
            }
            ControlEvent::SetCcMap { map } => {
                // CcMap is Copy, so this reads through the box without
                // consuming it; the allocation goes back to the GUI thread.
                self.cc_map = *map;
                return Some(ControlEvent::SetCcMap { map });
            }
            ControlEvent::SetGlobalParam { key, value } => {
                self.global_params.set(key, value);
                if key == "pitch_bend_range" {
                    self.sampler.set_pitch_bend_range(value.clamp(1.0, 24.0) as u8);
                }
            }
            ControlEvent::DrumSetStep { slot, step, velocity } => {
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                self.drum_engine.sequencer.patterns[pat].steps[slot as usize][step as usize].velocity = velocity;
            }
            ControlEvent::DrumSetParam { slot, param } => {
                let s = slot as usize;
                match param {
                    DrumParam::Level(v) => self.drum_engine.params[s].level = v,
                    DrumParam::Pan(v) => self.drum_engine.params[s].pan = v,
                    DrumParam::Tune(v) => self.drum_engine.params[s].tune = v,
                    DrumParam::Decay(v) => self.drum_engine.params[s].decay = v,
                }
            }
            ControlEvent::DrumSetVolume { volume } => {
                self.drum_engine.volume = volume;
                self.sampler.drum_volume = volume;
            }
            ControlEvent::DrumSeqPlay { playing } => {
                self.drum_engine.sequencer.playing = playing;
                if playing { self.drum_engine.sequencer.reset(); }
            }
            ControlEvent::DrumSeqBpm { bpm } => { self.drum_engine.sequencer.bpm = bpm; }
            ControlEvent::DrumSeqSwing { swing } => { self.drum_engine.sequencer.swing = swing; }
            ControlEvent::DrumSeqPattern { pattern } => {
                self.drum_engine.sequencer.current_pattern = pattern;
            }
            ControlEvent::DrumSeqLength { length } => {
                // Clamp: record_hit делит на len (drum.rs) — ноль из кольца
                // это div-by-zero; 0..=255 → 1..=16.
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                self.drum_engine.sequencer.patterns[pat].length = length.clamp(1, 16);
            }
            ControlEvent::DrumSeqTimeSig { beats_per_bar } => {
                // Signature = beats_per_bar; the bar IS `length` steps of 16ths,
                // so length follows: 4 steps per beat at any signature.
                // Clamp 2..=4: MAX_STEPS = 16 caps the bar at 4/4.
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                let bpb = beats_per_bar.clamp(2, 4);
                self.drum_engine.sequencer.patterns[pat].beats_per_bar = bpb;
                self.drum_engine.sequencer.patterns[pat].length = bpb * 4;
            }
            ControlEvent::DrumSeqRecord { recording } => {
                if recording && !self.drum_engine.sequencer.recording {
                    self.drum_engine.sequencer.save_snapshot();
                }
                self.drum_engine.sequencer.recording = recording;
                if recording && !self.drum_engine.sequencer.playing {
                    self.drum_engine.set_playing(true);
                }
            }
            ControlEvent::DrumSeqClear => {
                self.drum_engine.sequencer.clear_pattern();
            }
            ControlEvent::DrumSeqUndo => {
                self.drum_engine.sequencer.undo();
            }
            ControlEvent::DrumSeqRedo => {
                self.drum_engine.sequencer.redo();
            }
            ControlEvent::DrumLoadKit { patterns, params, bpm, swing, volume } => {
                self.drum_engine.sequencer.cancel_redo();
                // Both arrays are Copy; the boxes travel back for freeing.
                self.drum_engine.sequencer.patterns = *patterns;
                self.drum_engine.params = *params;
                self.drum_engine.sequencer.bpm = bpm;
                self.drum_engine.sequencer.swing = swing;
                self.drum_engine.volume = volume;
                self.sampler.drum_volume = volume;
                return Some(ControlEvent::DrumLoadKit { patterns, params, bpm, swing, volume });
            }
            ControlEvent::LooperRecord => { self.looper.start_record(); }
            ControlEvent::LooperStopRecord => { self.looper.stop_record(); }
            ControlEvent::LooperTogglePlay => { self.looper.toggle_play(); }
            ControlEvent::LooperUndo => { self.looper.undo(); }
            ControlEvent::LooperClear => { self.looper.clear(); }
            ControlEvent::LooperSetBars { bars } => { self.looper.bars = bars; }
            ControlEvent::LooperSetQuantize { quantize } => {
                self.looper.quantize = looper::Quantize::from_index(quantize);
            }
            ControlEvent::LooperRestore { events, loop_length_ticks } => {
                self.looper.restore(&events, loop_length_ticks);
                // Heap-owning payload: hand it back for GUI-thread disposal.
                return Some(ControlEvent::LooperRestore { events, loop_length_ticks });
            }
            ControlEvent::LooperSetLayerMute { layer, mute } => {
                let note_offs = self.looper.set_layer_mute(layer, mute);
                self.route_looper_note_offs(&note_offs);
            }
            ControlEvent::LooperSetSolo { layer } => {
                let note_offs = self.looper.set_solo(layer);
                self.route_looper_note_offs(&note_offs);
            }
            ControlEvent::LooperSetPartMute { part, mute } => {
                let note_offs = self.looper.set_part_mute(part, mute);
                self.route_looper_note_offs(&note_offs);
            }
            ControlEvent::LooperSetSource { mix } => {
                self.looper_source_mix = mix;
            }
            ControlEvent::MasterPlay => {
                self.master_toggle_play();
            }
            ControlEvent::MasterStop => {
                self.master_stop();
            }
            ControlEvent::MasterRecord => {
                self.master_toggle_record();
            }
            ControlEvent::DrumSetFixedVelocity { fixed } => {
                self.drum_engine.fixed_velocity = fixed;
            }
            ControlEvent::DrumSetMetronome { on } => {
                self.drum_engine.metronome = on;
            }
            ControlEvent::DrumSetRemap { table } => {
                // The box travels back for freeing on the GUI thread.
                self.drum_engine.apply_remap_vec(&*table);
                return Some(ControlEvent::DrumSetRemap { table });
            }
            ControlEvent::DrumSetChokeGroups { table } => {
                self.drum_engine.set_choke_groups(&table);
                return Some(ControlEvent::DrumSetChokeGroups { table });
            }
            ControlEvent::DrumSetPadVel { table } => {
                self.drum_engine.apply_pad_vel_vec(&*table);
                return Some(ControlEvent::DrumSetPadVel { table });
            }
            ControlEvent::SetSustain { on } => {
                self.set_sustain(if on { 127 } else { 0 });
            }
            ControlEvent::SetSustainRelease { seconds } => {
                self.sustain_release = seconds;
            }
            ControlEvent::AllNotesOff => {
                self.set_sustain(0);
                for (i, part) in self.parts.iter_mut().enumerate() {
                    part.drain_notes(&mut self.sampler, i, false);
                }
                self.sampler.all_notes_off();
                self.drum_engine.reset_choke_state();
                // Clear arpeggiator held notes
                if let Some(off_note) = self.arpeggiator.flush_current_note() {
                    // Same transposed routing as flush_arp_note_off.
                    self.dispatch_arp_note_off(off_note);
                }
                self.arpeggiator.clear();
            }
            ControlEvent::LoadKeysSoundFont { soundfont } => {
                // Clone bumps the Arc refcount; the original travels back to
                // the GUI thread with the event, and whatever the sampler
                // displaced goes to the trash queue.
                let displaced = self.sampler.load_keys_soundfont(soundfont.clone());
                self.dispose_sf2(displaced);
                self.sampler.set_pitch_bend_range(self.global_params.pitch_bend_range as u8);
                return Some(ControlEvent::LoadKeysSoundFont { soundfont });
            }
            ControlEvent::LoadDrumsSoundFont { soundfont } => {
                let displaced = self.sampler.load_drums_soundfont(soundfont.clone());
                self.dispose_sf2(displaced);
                return Some(ControlEvent::LoadDrumsSoundFont { soundfont });
            }
            ControlEvent::UnloadKeysSoundFont => {
                let displaced = self.sampler.unload_keys();
                self.dispose_sf2(displaced);
            }
            ControlEvent::UnloadDrumsSoundFont => {
                let displaced = self.sampler.unload_drums();
                self.dispose_sf2(displaced);
            }
            ControlEvent::SetPartSf2Mode { part, enabled } => {
                // No-op switch (perf load pushes current state for every part):
                // draining here would cut SF2 tails on every perf change.
                if let Some(l) = self.parts.get(part) {
                    if l.sf2_mode == enabled { return None; }
                }
                // Engine switch: both voice owners must be drained
                if let Some(l) = self.parts.get_mut(part) {
                    l.drain_notes(&mut self.sampler, part, false);
                    l.sf2_mode = enabled;
                    // (Re)apply the part's pan on the SF2 channel — CC10 was
                    // never sent while the part was in DSP mode.
                    if enabled {
                        let cc = (l.pan.clamp(-1.0, 1.0) * 0.5 + 0.5) * 127.0;
                        self.sampler.set_channel_pan(part, cc as i32);
                    }
                }
                self.sampler.set_part_mode(part, enabled);
            }
            ControlEvent::SetPartSf2Program { part, program, bank } => {
                // NO drain: rustysynth 0xC0 = channel.set_patch only — sounding
                // voices keep their captured region data (seamless tails);
                // new notes use the new program.
                self.sampler.set_part_program(part, program, bank);
            }
            ControlEvent::SetDrumsSf2Mode { enabled } => {
                self.sampler.set_drums_enabled(enabled);
                self.drum_engine.sf2_mode = enabled;
            }
            ControlEvent::SetSf2BlockSize { size } => {
                let (keys, drums) = self.sampler.set_block_size(size);
                self.dispose_sf2(keys);
                self.dispose_sf2(drums);
            }
            ControlEvent::SetSf2ReleaseScale { scale } => {
                self.sampler.set_release_time_scale(scale);
            }
            ControlEvent::SetSf2SampleOffset { ms } => {
                self.sampler.set_sample_offset_ms(ms);
            }
            ControlEvent::SeqSetEnabled { part, enabled } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pitch_seq.enabled = enabled;
                    if enabled { l.pitch_seq.reset(); }
                }
            }
            ControlEvent::SeqSetStep { part, step, pitch, gate, velocity } => {
                if let Some(l) = self.parts.get_mut(part) {
                    if (step as usize) < step_seq::MAX_STEPS {
                        l.pitch_seq.steps[step as usize] = step_seq::PitchStep { pitch, gate, velocity };
                    }
                }
            }
            ControlEvent::SeqSetLength { part, length } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pitch_seq.length = length.clamp(1, 16);
                }
            }
            ControlEvent::SeqSetRate { part, rate } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pitch_seq.rate = step_seq::StepRate::from_index(rate);
                }
            }
            ControlEvent::SeqSetScale { part, scale } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pitch_seq.scale = step_seq::ScaleType::from_index(scale);
                }
            }
            ControlEvent::SeqSetSwing { part, swing } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pitch_seq.swing = swing;
                }
            }
            ControlEvent::MidiSeqLoad { data } => {
                // The old file's sounding notes will never see their note-offs
                // from the new event stream. Lift sustain everywhere FIRST:
                // all_notes_off alone clears only the sampler's pedal state —
                // the engine atom, part.sustain_pedal and sustained_notes
                // would stay ON and DSP would keep sustaining after key-offs
                // until the next CC64 edge (same ordering as the CC120/123 arm).
                self.set_sustain(0);
                self.sampler.all_notes_off();
                // The sequencer no longer owns any channel program
                self.sampler.seq_channels_release();
                // Full per-part drain: closes the root_held/root_active gap
                // (ghost rekey on root_only patches after loading a new MIDI
                // file). The global all_notes_off above stays for drum/seq
                // channels not mapped to parts; the per-part CC123 is
                // redundant but idempotent.
                for (i, part) in self.parts.iter_mut().enumerate() {
                    part.drain_notes(&mut self.sampler, i, false);
                }
                self.seq_route = [0u8; 16];
                // The Box moves into the player; what it displaced comes back
                // out and goes to the GUI thread for freeing.
                let (old_data, old_tracks) = self.midi_player.load(data);
                if old_data.is_some() || !old_tracks.is_empty() {
                    match self.trash.as_mut() {
                        Some(t) => t.dispose(crate::trash::Trash::MidiSeq {
                            data: old_data,
                            tracks: old_tracks,
                        }),
                        None => drop((old_data, old_tracks)),
                    }
                }
            }
            ControlEvent::MidiSeqPlay { playing } => {
                self.midi_player.playing = playing;
                self.midi_player.play_atom.store(playing as u8, std::sync::atomic::Ordering::Relaxed);
                if !playing {
                    self.midi_player.reset_position();
                    // Lift sustain everywhere first (see MidiSeqLoad): the
                    // engine atom and part flags must agree with the sampler.
                    self.set_sustain(0);
                    // All notes off on all engines
                    self.sampler.all_notes_off();
                    // Playback stopped: give the shared channels back to the parts
                    self.sampler.seq_channels_release();
                    // Full per-part drain: closes the root_held/root_active gap
                    // (ghost rekey on root_only patches after MIDI-stop).
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        part.drain_notes(&mut self.sampler, i, false);
                    }
                }
            }
            ControlEvent::MidiSeqSetBpm { bpm } => {
                self.midi_player.bpm_override = bpm;
            }
            ControlEvent::MidiSeqSetLooping { looping } => {
                self.midi_player.looping = looping;
            }
            ControlEvent::MidiSeqSetTrackInstrument { track_idx, instrument } => {
                if let Some(track) = self.midi_player.tracks.get_mut(track_idx) {
                    track.instrument = instrument;
                    // Pre-set the SF2 program for this channel so notes play correctly
                    if let midi_player::TrackInstrument::Sf2 { program } = instrument {
                        self.sampler.seq_program_set(track.channel, program);
                    }
                }
            }
            ControlEvent::MidiSeqSetTrackMute { track_idx, muted } => {
                if let Some(track) = self.midi_player.tracks.get_mut(track_idx) {
                    track.muted = muted;
                }
            }
            ControlEvent::SetArpParams { enabled, mode, rate, octaves, gate } => {
                // If disabling, send note-off for current arp note
                if !enabled {
                    if let Some(off_note) = self.arpeggiator.flush_current_note() {
                        // Same transposed routing as flush_arp_note_off.
                        self.dispatch_arp_note_off(off_note);
                    }
                }
                self.arpeggiator.set_params(enabled, mode, rate, octaves, gate);
            }
            ControlEvent::SetArpLatch { latch } => {
                if !latch {
                    // Stop the frozen set first so the sounding arp voice
                    // gets a matching note-off before the set is cleared.
                    if let Some(off_note) = self.arpeggiator.flush_current_note() {
                        // Same transposed routing as flush_arp_note_off.
                        self.dispatch_arp_note_off(off_note);
                    }
                }
                self.arpeggiator.set_latch(latch);
            }
        }

        // Every variant that owns heap memory returns early above; reaching
        // here means the event was plain data and costs nothing to drop.
        None
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for part in &mut self.parts { part.set_sample_rate(sample_rate); }
        self.drum_engine.set_sample_rate(sample_rate);
        // Called from the audio callback when JACK reports a rate change, so
        // the rebuilt soundfonts must not be freed here.
        let (keys, drums) = self.sampler.set_sample_rate(sample_rate);
        self.dispose_sf2(keys);
        self.dispose_sf2(drums);
        self.eq.set_sample_rate(sample_rate);
        self.compressor.set_sample_rate(sample_rate);
        self.overdrive.set_sample_rate(sample_rate);
        self.tape.set_sample_rate(sample_rate);
        self.neuron.set_sample_rate(sample_rate);
        self.phaser.set_sample_rate(sample_rate);
        self.flanger.set_sample_rate(sample_rate);
        self.tremolo.set_sample_rate(sample_rate);
        self.bitcrusher.set_sample_rate(sample_rate);
        self.ring_mod.set_sample_rate(sample_rate);
        self.freq_shift.set_sample_rate(sample_rate);
        self.chorus.set_sample_rate(sample_rate);
        self.rotary.set_sample_rate(sample_rate);
        self.bbd_ensemble.set_sample_rate(sample_rate);
        self.resonator.set_sample_rate(sample_rate);
        self.bonsai.set_sample_rate(sample_rate);
        self.delay.set_sample_rate(sample_rate);
        self.reverb.set_sample_rate(sample_rate);
        self.spring_reverb.set_sample_rate(sample_rate);
        self.wave_shaper.set_sample_rate(sample_rate);
        self.ms_tool.set_sample_rate(sample_rate);
        self.graphic_eq_fx.set_sample_rate(sample_rate);
        self.conditioner.set_sample_rate(sample_rate);
        self.exciter.set_sample_rate(sample_rate);
        self.floaty_delay.set_sample_rate(sample_rate);
        self.reverb2.set_sample_rate(sample_rate);
        self.combulator.set_sample_rate(sample_rate);
        self.treemonster.set_sample_rate(sample_rate);
        self.nimbus.set_sample_rate(sample_rate);
        self.vocoder.set_sample_rate(sample_rate);
        self.conv_reverb.set_sample_rate(sample_rate);
        self.airwindows.set_sample_rate(sample_rate);
    }

    /// Process a block of audio samples. Control-rate computations (LFO, MSEG,
    /// mod matrix, effect params) happen once per block for efficiency.
    pub fn tick_block(&mut self, buf_l: &mut [f32], buf_r: &mut [f32]) {
        let block_len = buf_l.len();

        // --- Control rate (once per block) ---
        let at_alpha = 1.0 - (-(block_len as f32) / (0.010 * self.sample_rate)).exp();
        self.aftertouch_smooth += at_alpha * (self.aftertouch - self.aftertouch_smooth);

        let active_layer_params = self.parts.iter()
            .find(|l| l.enabled)
            .map(|l| &l.params);
        let lfo_rate = active_layer_params.map(|p| p.lfo_rate).unwrap_or(5.0);

        // Vibrato for mod wheel
        let vibrato_val = if self.mod_wheel > 0.001 {
            self.vibrato_phase += 6.0 / self.sample_rate * block_len as f32;
            if self.vibrato_phase >= 1.0 { self.vibrato_phase -= 1.0; }
            (self.vibrato_phase * std::f32::consts::TAU).sin()
        } else {
            self.vibrato_phase = 0.0;
            0.0
        };

        let seq_bpm = self.drum_engine.sequencer.bpm;
        self.arpeggiator.set_bpm(seq_bpm);
        // Looper follows the same canonical clock (REFACTOR_AUDIT S5): bpm +
        // current pattern's beats_per_bar, synced every block like arp/LFO.
        let cur_pat = self.drum_engine.sequencer.current_pattern as usize;
        let beats_per_bar = self.drum_engine.sequencer.patterns[cur_pat].beats_per_bar;
        self.looper.sync_clock(seq_bpm, beats_per_bar);

        // Pre-compute modulation state for each part (control rate).
        // `layer_mods` lives on the engine (zero-alloc); disabled parts retain
        // whatever previous-block values they had — never read while disabled.
        { let layer_mods = &mut self.layer_mods;
        for (li, part) in self.parts.iter_mut().enumerate() {
            if !part.enabled { continue; }

            // Step sequencer (control rate)
            let seq_evt = part.pitch_seq.tick(self.sample_rate, seq_bpm, block_len);
            if seq_evt.stepped && seq_evt.gate {
                for voice in &mut part.voices {
                    if voice.active && !voice.is_releasing() {
                        voice.retrigger_envelope();
                    }
                }
            }
            if seq_evt.stepped && !seq_evt.gate {
                for voice in &mut part.voices {
                    if voice.active && !voice.is_releasing() {
                        voice.note_off();
                    }
                }
            }
            let seq_raw = if part.pitch_seq.enabled { seq_evt.pitch_offset / 24.0 } else { 0.0 };
            let seq_pitch = seq_raw * 24.0 * part.params.seq_pitch_depth;
            let seq_amp = if part.pitch_seq.enabled && !seq_evt.gate { 0.0 } else { 1.0 };

            let bl = block_len as f32;

            // Set BPM, tempo sync, unipolar on all LFOs
            let lfo_tempo_syncs = [part.params.lfo1_tempo_sync, part.params.lfo2_tempo_sync,
                                   part.params.lfo3_tempo_sync, part.params.lfo4_tempo_sync];
            let lfo_unipolars = [part.params.lfo1_unipolar, part.params.lfo2_unipolar,
                                 part.params.lfo3_unipolar, part.params.lfo4_unipolar];
            for i in 0..4 {
                part.lfos[i].set_bpm(seq_bpm);
                part.lfos[i].tempo_sync = lfo_tempo_syncs[i] > 0.5;
                part.lfos[i].unipolar = lfo_unipolars[i] > 0.5;
            }

            // LFO 1
            let lfo1_wf = LfoWaveform::from_param(part.params.lfo_waveform);
            let lfo1_active = lfo1_wf == LfoWaveform::StepSeq
                || part.params.lfo_pitch_depth > 0.001
                || part.params.lfo_filter_depth > 0.001
                || part.params.lfo_amp_depth > 0.001
                || self.aftertouch_smooth > 0.001;
            let lfo1_val = if lfo1_active {
                part.lfos[0].tick_with_deform(lfo_rate * bl, lfo1_wf, part.params.lfo_deform)
            } else { 0.0 };

            // LFO 2 — also active when routed through mod matrix or in StepSeq mode
            let lfo2_wf = LfoWaveform::from_param(part.params.lfo2_waveform);
            let lfo2_active = lfo2_wf == LfoWaveform::StepSeq || part.lfo2_routed
                || part.params.lfo2_pitch_depth > 0.001
                || part.params.lfo2_filter_depth > 0.001
                || part.params.lfo2_amp_depth > 0.001;
            let lfo2_val = if lfo2_active {
                part.lfos[1].tick_with_deform(part.params.lfo2_rate * bl, lfo2_wf, part.params.lfo2_deform)
            } else { 0.0 };

            // LFO 3 & 4 — tick when routed or in StepSeq mode
            let lfo3_wf = LfoWaveform::from_param(part.params.lfo3_waveform);
            let lfo3_val = if lfo3_wf == LfoWaveform::StepSeq || part.lfo3_routed {
                part.lfos[2].tick_with_deform(part.params.lfo3_rate * bl, lfo3_wf, part.params.lfo3_deform)
            } else { 0.0 };
            let lfo4_wf = LfoWaveform::from_param(part.params.lfo4_waveform);
            let lfo4_val = if lfo4_wf == LfoWaveform::StepSeq || part.lfo4_routed {
                part.lfos[3].tick_with_deform(part.params.lfo4_rate * bl, lfo4_wf, part.params.lfo4_deform)
            } else { 0.0 };

            // Step sequencer envelope retrigger from any LFO in StepSeq mode
            let lfo_wfs = [lfo1_wf, lfo2_wf, lfo3_wf, lfo4_wf];
            for (i, &wf) in lfo_wfs.iter().enumerate() {
                if wf == LfoWaveform::StepSeq {
                    let evt = part.lfos[i].last_step_event();
                    if evt.stepped && (evt.retrigger_aeg || evt.retrigger_feg) {
                        for voice in &mut part.voices {
                            if voice.active && !voice.is_releasing() {
                                if evt.retrigger_aeg && evt.retrigger_feg {
                                    voice.retrigger_envelope();
                                } else if evt.retrigger_aeg {
                                    voice.retrigger_amp_env();
                                } else {
                                    voice.retrigger_filter_env();
                                }
                            }
                        }
                    }
                }
            }

            // Scene LFOs — always tick regardless of note state, never retrigger
            let slfo1_wf = LfoWaveform::from_param(part.params.slfo1_waveform);
            part.scene_lfos[0].set_bpm(seq_bpm);
            part.scene_lfos[0].tempo_sync = part.params.slfo1_tempo_sync > 0.5;
            part.scene_lfos[0].unipolar  = part.params.slfo1_unipolar  > 0.5;
            let slfo1_val = if part.scene_lfo_routed[0] || slfo1_wf == LfoWaveform::StepSeq {
                let rate = if part.scene_lfos[0].tempo_sync {
                    part.params.slfo1_rate
                } else {
                    part.params.slfo1_rate * bl
                };
                part.scene_lfos[0].tick_with_deform(rate, slfo1_wf, part.params.slfo1_deform)
            } else { 0.0 };

            let slfo2_wf = LfoWaveform::from_param(part.params.slfo2_waveform);
            part.scene_lfos[1].set_bpm(seq_bpm);
            part.scene_lfos[1].tempo_sync = part.params.slfo2_tempo_sync > 0.5;
            part.scene_lfos[1].unipolar  = part.params.slfo2_unipolar  > 0.5;
            let slfo2_val = if part.scene_lfo_routed[1] || slfo2_wf == LfoWaveform::StepSeq {
                let rate = if part.scene_lfos[1].tempo_sync {
                    part.params.slfo2_rate
                } else {
                    part.params.slfo2_rate * bl
                };
                part.scene_lfos[1].tick_with_deform(rate, slfo2_wf, part.params.slfo2_deform)
            } else { 0.0 };

            // MSEGs
            let mseg1_val = if part.params.mseg_enabled > 0.5 {
                part.mseg1_state.tick(&part.mseg1)
            } else { 0.0 };
            let mseg2_val = if part.params.mseg_enabled > 0.5 {
                part.mseg2_state.tick(&part.mseg2)
            } else { 0.0 };

            // Mod matrix
            let key_track = (part.last_note as f32 - 60.0) / 48.0;
            let mod_sources = mod_matrix::ModSources {
                lfo_outputs: [lfo1_val, lfo2_val, lfo3_val, lfo4_val],
                scene_lfo_outputs: [slfo1_val, slfo2_val],
                macro_vals: part.macro_vals,
                amp_env: 0.0, filter_env: 0.0,
                mseg_outputs: [mseg1_val, mseg2_val],
                mod_wheel: self.mod_wheel,
                aftertouch: self.aftertouch_smooth,
                velocity: part.last_velocity,
                key_track,
                step_seq: seq_raw,
                random_bipolar: part.rand_bipolar,
                random_unipolar: part.rand_unipolar,
                alt_bipolar: part.alt_bipolar,
                alt_unipolar: part.alt_unipolar,
                release_vel: part.release_vel,
                pitch_bend: self.pitch_bend_semitones / 2.0, // normalize -1..+1
                cc: self.cc_values,
                breath: part.breath,
                expression: part.expression,
                sustain_pedal: part.sustain_pedal,
                lowest_key: (part.lowest_held as f32 - 60.0) / 48.0,
                highest_key: (part.highest_held as f32 - 60.0) / 48.0,
                latest_key: (part.last_note as f32 - 60.0) / 48.0,
                poly_aftertouch: 0.0, // per-voice; 0 for part-level evaluation
            };
            let mod_offsets = part.mod_matrix.evaluate(&mod_sources);

            let lfo_pitch_offset_base = self.pitch_bend_semitones
                + lfo1_val * part.params.lfo_pitch_depth * 2.0
                + lfo2_val * part.params.lfo2_pitch_depth * 2.0
                + self.mod_wheel * 0.5 * vibrato_val
                + self.aftertouch_smooth * 0.3 * lfo1_val
                + seq_pitch;
            let pitch_offset = lfo_pitch_offset_base + mod_offsets.pitch;
            let pitch_mult = if pitch_offset.abs() < 0.001 { 1.0 } else { (pitch_offset / 12.0).exp2() };
            // filter_offset is Hz-only (LFO direct routing, aftertouch)
            // filter_offset_semis is exponential (mod matrix)
            let filter_offset = lfo1_val * part.params.lfo_filter_depth * 4000.0
                + lfo2_val * part.params.lfo2_filter_depth * 4000.0
                + self.aftertouch_smooth * 2000.0;
            let lfo_amp_base = (1.0 - part.params.lfo_amp_depth * 0.5 * (1.0 - lfo1_val))
                * (1.0 - part.params.lfo2_amp_depth * 0.5 * (1.0 - lfo2_val))
                * seq_amp;
            let amp_mod = lfo_amp_base * (1.0 + mod_offsets.amplitude).max(0.0);

            // Cache mod sources and base values for per-voice poly AT re-evaluation
            part.cached_mod_sources = mod_sources;
            part.cached_lfo_pitch_offset = lfo_pitch_offset_base;
            part.cached_filter_offset = filter_offset;
            part.cached_lfo_amp_base = lfo_amp_base;

            layer_mods[li] = ModulationState {
                pitch_mult, filter_offset, filter_offset_semis: mod_offsets.filter_cutoff, amp_mod,
            };
        }
        } // end of layer_mods borrow scope

        // Snapshot of per-part modulation for the voice tick loop below.
        // `ModulationState` is `Copy` and the array is small (8 × 16 B);
        // this stack copy lets us iterate `self.parts.iter_mut()` without
        // a borrow conflict on `self.layer_mods`.
        let layer_mods_snapshot = self.layer_mods;

        // Cache effect params (once per block)
        let ep = self.parts.iter().find(|l| l.enabled).map(|l| &l.params);
        let fx = CachedFxParams::from_preset(ep, &self.global_params);
        let fx_chain = ep.map(|p| p.fx_chain).unwrap_or_default();

        // --- MIDI file sequencer: fire block-level events ---
        // (process_block clears midi_seq_buf itself before filling it)
        {
            self.midi_player.process_block(block_len, self.sample_rate, &mut self.midi_seq_buf);
            for i in 0..self.midi_seq_buf.len() {
                let ch = self.midi_seq_buf[i].0;
                let evt = self.midi_seq_buf[i].1.clone();
                let instr = self.midi_player.instrument_for_channel(ch);
                match evt {
                    midi_player::SeqEventData::NoteOn { note, velocity } => match instr {
                        midi_player::TrackInstrument::Drums => {
                            self.seq_route[ch as usize] = 2;
                            let sf2 = self.drum_engine.sf2_mode;
                            if sf2 {
                                let choke = self.drum_engine.sf2_choke_take(note);
                                if choke != 0 { self.sampler.drum_note_off(choke); }
                                self.sampler.drum_note_on(note, velocity);
                            } else {
                                self.drum_engine.note_on(note, velocity);
                            }
                        }
                        midi_player::TrackInstrument::DspLayer0 => {
                            self.seq_route[ch as usize] = 0;
                            if self.parts.first().map(|p| p.root_only).unwrap_or(false) {
                                // Root-only gate (range/velocity checked inside)
                                if !self.parts.is_empty() { self.route_note_on(0, note, velocity); }
                            } else if let Some(part) = self.parts.get_mut(0) {
                                part.note_on(note, velocity);
                            }
                        }
                        midi_player::TrackInstrument::Sf2 { program } => {
                            self.seq_route[ch as usize] = 1;
                            self.sampler.seq_program_set(ch, program);
                            self.sampler.seq_note_on(ch, note, velocity);
                        }
                    },
                    midi_player::SeqEventData::NoteOff { note } => match self.seq_route[ch as usize] {
                        0 => {
                            if self.parts.first().map(|p| p.root_only).unwrap_or(false) {
                                if !self.parts.is_empty() { self.route_note_off(0, note); }
                            } else if let Some(part) = self.parts.get_mut(0) {
                                part.note_off(note);
                            }
                        }
                        1 => self.sampler.seq_note_off(ch, note),
                        _ => {
                            if let midi_player::TrackInstrument::Drums = instr {
                                let sf2 = self.drum_engine.sf2_mode;
                                if sf2 { self.sampler.drum_note_off(note); }
                            }
                        },
                    },
                    midi_player::SeqEventData::ProgramChange { program } => {
                        // Follow file program changes when in SF2 mode
                        if let midi_player::TrackInstrument::Sf2 { .. } = instr {
                            self.sampler.seq_program_set(ch, program);
                        }
                    }
                    _ => {}
                }
            }
        }

        // --- Audio rate (per sample) ---
        for s in 0..block_len {
            // Arpeggiator: generate note events from held notes
            {
                let (arp_on, arp_off) = self.arpeggiator.tick();
                if let Some(off_note) = arp_off {
                    for i in 0..self.parts.len() {
                        if !self.parts[i].enabled { continue; }
                        self.route_note_off(i, off_note);
                        // Source-consistent recording: per part, like the
                        // note dispatch loop (used to be one active-part
                        // event — the loop replayed arps on one part only).
                        self.record_for_looper(off_note, 0, i as u8);
                    }
                }
                if let Some((note, vel)) = arp_on {
                    for i in 0..self.parts.len() {
                        if !self.parts[i].enabled || self.parts[i].mute { continue; }
                        self.route_note_on(i, note, vel);
                        self.record_for_looper(note, vel, i as u8);
                    }
                }
            }

            // Looper: replay recorded events (routed to specific part)
            let looper_events = self.looper.tick();
            for &(note, vel, part_id) in &looper_events {
                if note == 0 && vel == 0 && part_id == 255 { break; }
                if part_id == 254 {
                    // Drum event: trigger the drum engine (SF2 choke + sampler,
                    // или DSP), never a part — mirror of the live pad path.
                    if vel > 0 {
                        if self.drum_engine.sf2_mode {
                            let choke = self.drum_engine.sf2_choke_take(note);
                            if choke != 0 { self.sampler.drum_note_off(choke); }
                            self.sampler.drum_note_on(note, vel);
                        } else {
                            self.drum_engine.note_on(note, vel);
                        }
                    }
                    continue;
                }
                let pi = part_id as usize;
                if self.parts.get(pi).map(|p| !p.enabled).unwrap_or(true) { continue; }
                if self.parts[pi].root_only {
                    // Root-only gate (range/velocity checked inside)
                    if vel > 0 { self.route_note_on(pi, note, vel); }
                    else { self.route_note_off(pi, note); }
                } else if let Some(part) = self.parts.get_mut(pi) {
                    let transposed = (note as i16 + part.transpose as i16).clamp(0, 127) as u8;
                    if vel > 0 {
                        if part.sf2_mode {
                            self.sampler.note_on(pi, transposed, vel);
                        } else {
                            part.note_on(note, vel); // raw: part transposes internally
                        }
                    } else {
                        if part.sf2_mode {
                            self.sampler.note_off(pi, transposed);
                        } else {
                            part.note_off(note); // raw: part matches by src_note
                        }
                    }
                }
            }

            let (mut out_l, mut out_r) = (0.0_f32, 0.0_f32);

            // Voice ticks (audio rate, using cached modulation)
            let mute_fade_step = 1.0 / (0.005 * self.sample_rate);
            for (li, part) in self.parts.iter_mut().enumerate() {
                if !part.enabled { continue; }
                let (l, r) = part.tick(&layer_mods_snapshot[li]);
                // Mute is a mix gate: a muted part contributes zero within ~5 ms
                // (linear fade both ways, so the gate itself does not click).
                // Voices keep ticking so their release envelopes reach idle and
                // the voice slot is freed.
                part.mute_gain = if part.mute {
                    (part.mute_gain - mute_fade_step).max(0.0)
                } else {
                    (part.mute_gain + mute_fade_step).min(1.0)
                };
                let fade = part.mute_gain;
                // Apply per-part pan (constant-power: sqrt of (0.5 ± pan*0.5))
                let pan = part.pan.clamp(-1.0, 1.0);
                let gain_l = ((0.5 - pan * 0.5) as f64).sqrt() as f32;
                let gain_r = ((0.5 + pan * 0.5) as f64).sqrt() as f32;
                out_l += l * gain_l * fade;
                out_r += r * gain_r * fade;
            }

            // SF2 + drums
            let (sf2_l, sf2_r) = self.sampler.tick();
            out_l += sf2_l; out_r += sf2_r;
            let ((drum_l, drum_r), drum_triggers) = self.drum_engine.tick();
            out_l += drum_l; out_r += drum_r;
            for i in 0..drum_triggers.count {
                let (slot, vel) = drum_triggers.triggers[i];
                let note = drum::DRUM_NOTE_BASE + slot;
                if self.drum_engine.sf2_mode {
                    let choke = self.drum_engine.sf2_choke_take(note);
                    if choke != 0 { self.sampler.drum_note_off(choke); }
                }
                self.sampler.drum_note_on(note, vel);
            }

            // Effects chain with cached params — skip when mix=0 to save CPU
            let (out_l, out_r) = self.eq.tick(out_l, out_r);
            let (out_l, out_r) = self.compressor.tick(out_l, out_r);
            let (out_l, out_r) = self.overdrive.tick(out_l, out_r);
            let (out_l, out_r) = if fx.tape_mix > 0.001 {
                self.tape.tick(out_l, out_r, fx.tape_drive, fx.tape_saturation, fx.tape_bias, fx.tape_tone, fx.tape_speed, fx.tape_mix)
            } else { (out_l, out_r) };
            let (out_l, out_r) = if fx.neuron_mix > 0.001 {
                self.neuron.tick(out_l, out_r, fx.neuron_drive, fx.neuron_squash, fx.neuron_stab, fx.neuron_asym, fx.neuron_bias, fx.neuron_comb_freq, fx.neuron_comb_sep, fx.neuron_mix)
            } else { (out_l, out_r) };
            let (out_l, out_r) = self.phaser.tick(out_l, out_r);
            let (out_l, out_r) = self.flanger.tick(out_l, out_r);
            let (out_l, out_r) = self.tremolo.tick(out_l, out_r);
            let (out_l, out_r) = self.bitcrusher.tick(out_l, out_r);
            let (out_l, out_r) = if fx.ring_mod_mix > 0.001 {
                self.ring_mod.tick(out_l, out_r, fx.ring_mod_freq, fx.ring_mod_shape, fx.ring_mod_bias, fx.ring_mod_linear, fx.ring_mod_mix)
            } else { (out_l, out_r) };
            let (out_l, out_r) = if fx.freq_shift_mix > 0.001 {
                self.freq_shift.tick(out_l, out_r, fx.freq_shift_hz, fx.freq_shift_feedback, fx.freq_shift_delay, fx.freq_shift_mix)
            } else { (out_l, out_r) };
            // Bonsai saturation (after neuron, before chorus)
            let (out_l, out_r) = if fx.bonsai_mix > 0.001 {
                let mode = bonsai::BonsaiMode::from_param(fx.bonsai_mode);
                self.bonsai.tick(out_l, out_r, fx.bonsai_drive, fx.bonsai_tone, fx.bonsai_asym, mode, fx.bonsai_mix)
            } else { (out_l, out_r) };
            // Resonator (pitched comb bank)
            let (out_l, out_r) = if fx.resonator_mix > 0.001 {
                // Set 4 voices to harmonic series of resonator_freq
                let rf = fx.resonator_freq;
                let rd = fx.resonator_decay;
                self.resonator.set_freq(0, rf);
                self.resonator.set_freq(1, rf * 1.5);
                self.resonator.set_freq(2, rf * 2.0);
                self.resonator.set_freq(3, rf * 3.0);
                for i in 0..4 { self.resonator.set_decay(i, rd); }
                let mono = (out_l + out_r) * 0.5;
                self.resonator.tick(mono, fx.resonator_mix)
            } else { (out_l, out_r) };
            let mono = (out_l + out_r) * 0.5;
            let (cl, cr) = if fx.chorus_mix > 0.001 {
                self.chorus.tick(mono, fx.chorus_mix)
            } else { (mono, mono) };
            // BBD Ensemble (separate from Juno chorus)
            let (cl, cr) = if fx.ensemble_mix > 0.001 {
                let mono2 = (cl + cr) * 0.5;
                self.bbd_ensemble.tick(mono2, fx.ensemble_depth, fx.ensemble_rate, fx.ensemble_mix)
            } else { (cl, cr) };
            let diff = (out_l - out_r) * 0.5;
            let (dl, dr) = if fx.delay_mix > 0.001 {
                self.delay.tick(cl + diff, cr - diff, fx.delay_time_l, fx.delay_time_r, fx.delay_feedback, fx.delay_filter, fx.delay_ping_pong > 0.5, fx.delay_mix)
            } else { (cl + diff, cr - diff) };

            let (rl, rr) = if fx.reverb_type > 0.5 {
                let mix = if fx.spring_mix > 0.001 { fx.spring_mix } else { fx.reverb_mix };
                if mix > 0.001 {
                    self.spring_reverb.tick(dl, dr, fx.spring_size, fx.spring_decay, fx.spring_reflections, fx.spring_damping, fx.spring_spin, fx.spring_chaos, mix)
                } else { (dl, dr) }
            } else if fx.reverb_mix > 0.001 {
                self.reverb.tick(dl, dr, fx.reverb_room_size, fx.reverb_damping, fx.reverb_width, fx.reverb_pre_delay, fx.reverb_mix)
            } else { (dl, dr) };
            // Rotary Speaker (after reverb, before master tone)
            let (rl, rr) = if fx.rotary_mix > 0.001 {
                self.rotary.tick(rl, rr, fx.rotary_speed, fx.rotary_mix)
            } else { (rl, rr) };
            // WaveShaper
            let (rl, rr) = if fx.wave_shaper_mix > 0.001 {
                self.wave_shaper.tick(rl, rr, fx.wave_shaper_drive, fx.wave_shaper_mode as u32, fx.wave_shaper_bias, fx.wave_shaper_mix)
            } else { (rl, rr) };
            // MS Tool
            let (rl, rr) = if fx.ms_mix > 0.001 {
                self.ms_tool.tick(rl, rr, fx.ms_mid_gain, fx.ms_side_gain, fx.ms_rotation, fx.ms_mix)
            } else { (rl, rr) };
            // Graphic EQ (always runs if output_gain != 1.0 or any band != 0.0)
            let any_geq = fx.graphic_eq_gains.iter().any(|&g| g.abs() > 0.001) || (fx.graphic_eq_output - 1.0).abs() > 0.001;
            let (rl, rr) = if any_geq {
                self.graphic_eq_fx.tick(rl, rr, &fx.graphic_eq_gains, fx.graphic_eq_output)
            } else { (rl, rr) };
            // Conditioner
            let (rl, rr) = if fx.conditioner_mix > 0.001 {
                self.conditioner.tick(rl, rr, fx.conditioner_bass_cut, fx.conditioner_width, fx.conditioner_threshold, fx.conditioner_mix)
            } else { (rl, rr) };
            // Exciter
            let (rl, rr) = if fx.exciter_mix > 0.001 {
                self.exciter.tick(rl, rr, fx.exciter_drive, fx.exciter_freq, fx.exciter_presence, fx.exciter_mix)
            } else { (rl, rr) };
            // Floaty Delay
            let (rl, rr) = if fx.floaty_mix > 0.001 {
                self.floaty_delay.tick(rl, rr, fx.floaty_time, fx.floaty_feedback, fx.floaty_wobble, fx.floaty_rate, fx.floaty_damp, fx.floaty_mix)
            } else { (rl, rr) };
            // Reverb2 (FDN)
            let (rl, rr) = if fx.reverb2_mix > 0.001 {
                self.reverb2.tick(rl, rr, fx.reverb2_decay, fx.reverb2_damping, fx.reverb2_size, fx.reverb2_mix)
            } else { (rl, rr) };
            // Combulator
            let (rl, rr) = if fx.combulator_mix > 0.001 {
                self.combulator.tick(rl, rr, fx.combulator_freq, fx.combulator_offset2, fx.combulator_offset3, fx.combulator_feedback, fx.combulator_tone, fx.combulator_mix)
            } else { (rl, rr) };
            // Treemonster
            let (rl, rr) = if fx.treemonster_mix > 0.001 {
                self.treemonster.tick(rl, rr, fx.treemonster_threshold, fx.treemonster_shift, fx.treemonster_ring_mix, fx.treemonster_mix)
            } else { (rl, rr) };
            // Nimbus granular
            let (rl, rr) = if fx.nimbus_mix > 0.001 {
                self.nimbus.tick(rl, rr, fx.nimbus_position, fx.nimbus_size, fx.nimbus_pitch, fx.nimbus_density, fx.nimbus_spread, fx.nimbus_texture, fx.nimbus_mix)
            } else { (rl, rr) };
            // Vocoder
            let (rl, rr) = if fx.vocoder_mix > 0.001 {
                let mod_in = (rl + rr) * 0.5;
                self.vocoder.tick(rl, rr, mod_in, fx.vocoder_env_follow, fx.vocoder_gate, fx.vocoder_mix)
            } else { (rl, rr) };
            // Convolution Reverb
            let (rl, rr) = if fx.conv_reverb_mix > 0.001 {
                self.conv_reverb.tick(rl, rr, fx.conv_reverb_room, fx.conv_reverb_damping, fx.conv_reverb_predelay, fx.conv_reverb_mix)
            } else { (rl, rr) };
            // Airwindows
            let (rl, rr) = if fx.airwindows_mix > 0.001 {
                self.airwindows.tick(rl, rr, fx.airwindows_mode, fx.airwindows_drive, fx.airwindows_mix)
            } else { (rl, rr) };

            // ── New 16-slot FX chain (runs after legacy chain when active) ──
            let (rl, rr) = if fx_chain.active {
                let mut sl = rl;
                let mut sr = rr;
                for slot in &fx_chain.slots {
                    if slot.enabled && slot.slot_type != fx_chain::FxSlotType::None && slot.mix > 0.001 {
                        let slot_copy = *slot;
                        (sl, sr) = self.apply_fx_slot(&slot_copy, sl, sr);
                    }
                }
                (sl, sr)
            } else {
                (rl, rr)
            };

            let sample_rate = self.sample_rate;
            let (tl, tr) = self.global_params.apply_tone(rl, rr, sample_rate);
            let vol = self.global_params.master_volume;
            buf_l[s] = tl * vol;
            buf_r[s] = tr * vol;
            // Write mono mix to oscilloscope buffer
            self.scope_buf.push((buf_l[s] + buf_r[s]) * 0.5, &mut self.scope_write);
        }
    }

    /// Process one sample through a single FX chain slot. Returns (out_l, out_r).
    /// `mix` is the slot's dry/wet (0..1). Slot's own mix is already checked by caller.
    #[inline]
    fn apply_fx_slot(&mut self, slot: &fx_chain::FxSlot, in_l: f32, in_r: f32) -> (f32, f32) {
        use fx_chain::FxSlotType;
        let p = slot.params;
        let mix = slot.mix;
        let dry_l = in_l;
        let dry_r = in_r;
        let (wet_l, wet_r) = match slot.slot_type {
            FxSlotType::None => return (in_l, in_r),
            FxSlotType::Overdrive => {
                self.overdrive.drive = p[0] * 48.0 + 1.0;
                self.overdrive.tone  = p[1];
                self.overdrive.dist_type = overdrive::DistortionType::from_param(p[2]);
                self.overdrive.mix   = 1.0;
                self.overdrive.tick(in_l, in_r)
            }
            FxSlotType::Tape => {
                self.tape.tick(in_l, in_r, p[0]*3.0, p[1], p[2], p[3], 0.5, 1.0)
            }
            FxSlotType::Neuron => {
                self.neuron.tick(in_l, in_r, p[0], p[1], p[2], 0.0, 0.5, p[3].max(10.0), 0.5, 1.0)
            }
            FxSlotType::Bonsai => {
                let mode = bonsai::BonsaiMode::from_param(p[3]);
                self.bonsai.tick(in_l, in_r, p[0], p[1], p[2], mode, 1.0)
            }
            FxSlotType::WaveShaper => {
                self.wave_shaper.tick(in_l, in_r, p[0]*4.0+0.5, p[1] as u32, p[2]*2.0-1.0, 1.0)
            }
            FxSlotType::Airwindows => {
                self.airwindows.tick(in_l, in_r, p[0] as u32, p[1], 1.0)
            }
            FxSlotType::Chorus => {
                let mono = (in_l + in_r) * 0.5;
                self.chorus.tick(mono, 1.0)
            }
            FxSlotType::BbdEnsemble => {
                let mono = (in_l + in_r) * 0.5;
                self.bbd_ensemble.tick(mono, p[0], p[1], 1.0)
            }
            FxSlotType::Flanger => {
                self.flanger.mix = 1.0;
                self.flanger.tick(in_l, in_r)
            }
            FxSlotType::Phaser => {
                self.phaser.mix = 1.0;
                self.phaser.tick(in_l, in_r)
            }
            FxSlotType::Tremolo => {
                self.tremolo.mix = 1.0;
                self.tremolo.tick(in_l, in_r)
            }
            FxSlotType::Rotary => {
                self.rotary.tick(in_l, in_r, p[0], 1.0)
            }
            FxSlotType::Bitcrusher => {
                self.bitcrusher.mix = 1.0;
                self.bitcrusher.tick(in_l, in_r)
            }
            FxSlotType::Delay => {
                self.delay.tick(in_l, in_r, p[0], p[1], p[2], p[3], false, 1.0)
            }
            FxSlotType::FloatyDelay => {
                self.floaty_delay.tick(in_l, in_r, p[0], p[1], p[2], p[3]*5.0, 0.3, 1.0)
            }
            FxSlotType::Reverb => {
                self.reverb.tick(in_l, in_r, p[0], p[1], p[2], p[3], 1.0)
            }
            FxSlotType::Reverb2 => {
                self.reverb2.tick(in_l, in_r, p[0], p[1], p[2], 1.0)
            }
            FxSlotType::SpringReverb => {
                self.spring_reverb.tick(in_l, in_r, p[0], p[1], p[2], p[3], 0.3, 0.0, 1.0)
            }
            FxSlotType::ConvReverb => {
                self.conv_reverb.tick(in_l, in_r, p[0], p[1], p[2]*0.2, 1.0)
            }
            FxSlotType::Nimbus => {
                self.nimbus.tick(in_l, in_r, p[0], p[1], p[2], p[3], 0.5, 0.5, 1.0)
            }
            FxSlotType::RingMod => {
                self.ring_mod.tick(in_l, in_r, p[0], p[1], p[2], 0.5, 1.0)
            }
            FxSlotType::FreqShift => {
                self.freq_shift.tick(in_l, in_r, p[0]*1000.0, p[1], p[2], 1.0)
            }
            FxSlotType::Resonator => {
                let rf = p[0];
                let rd = p[1];
                self.resonator.set_freq(0, rf);
                self.resonator.set_freq(1, rf * 1.5);
                self.resonator.set_freq(2, rf * 2.0);
                self.resonator.set_freq(3, rf * 3.0);
                for i in 0..4 { self.resonator.set_decay(i, rd); }
                let mono = (in_l + in_r) * 0.5;
                self.resonator.tick(mono, 1.0)
            }
            FxSlotType::Combulator => {
                self.combulator.tick(in_l, in_r, p[0], p[1], p[2], p[3], 0.5, 1.0)
            }
            FxSlotType::Treemonster => {
                self.treemonster.tick(in_l, in_r, p[0], p[1]*24.0-12.0, p[2], 1.0)
            }
            FxSlotType::Vocoder => {
                let mod_in = (in_l + in_r) * 0.5;
                self.vocoder.tick(in_l, in_r, mod_in, p[0], p[1], 1.0)
            }
            FxSlotType::GraphicEq => {
                // p0=low p1=mid p2=high p3=output — map to 11-band array
                let gains = [p[0]*24.0-12.0, p[0]*24.0-12.0, p[0]*24.0-12.0,
                             p[1]*24.0-12.0, p[1]*24.0-12.0, p[1]*24.0-12.0, p[1]*24.0-12.0,
                             p[2]*24.0-12.0, p[2]*24.0-12.0, p[2]*24.0-12.0, p[2]*24.0-12.0];
                self.graphic_eq_fx.tick(in_l, in_r, &gains, p[3])
            }
            FxSlotType::Compressor => {
                self.compressor.tick(in_l, in_r)
            }
            FxSlotType::MsTool => {
                self.ms_tool.tick(in_l, in_r, p[0]*2.0-1.0, p[1]*2.0-1.0, p[2], 1.0)
            }
            FxSlotType::Conditioner => {
                self.conditioner.tick(in_l, in_r, p[0], p[1], p[2], 1.0)
            }
            FxSlotType::Exciter => {
                self.exciter.tick(in_l, in_r, p[0], p[1]*8000.0+200.0, p[2], 1.0)
            }
        };
        // Apply slot's dry/wet mix
        (dry_l + (wet_l - dry_l) * mix, dry_r + (wet_r - dry_r) * mix)
    }

    #[cfg(test)]
    pub fn tick(&mut self) -> (f32, f32) {
        let mut bl = [0.0f32; 1];
        let mut br = [0.0f32; 1];
        self.tick_block(&mut bl, &mut br);
        (bl[0], br[0])
    }
}


#[cfg(test)]
mod tests;
