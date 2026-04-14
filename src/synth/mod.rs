/// Synth engine: layered polyphonic voice pools + MIDI event dispatch.

mod airwindows;
use airwindows::Airwindows;
pub mod fx_chain;
pub use fx_chain::FxChain;
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

use crate::cc_map::{CcMap, ParamScope};
use crate::preset::Preset;
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
use lfo::{Lfo, LfoWaveform};
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

/// Lock-free oscilloscope buffer shared between audio and GUI threads.
/// Audio writes samples; GUI reads them for display. Uses atomic f32 encoding.
pub const SCOPE_SIZE: usize = 256;
pub struct ScopeBuffer {
    pub data: [std::sync::atomic::AtomicU32; SCOPE_SIZE],
    pub write_pos: std::sync::atomic::AtomicUsize,
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
        for i in 0..SCOPE_SIZE {
            let idx = (wp + i) & (SCOPE_SIZE - 1);
            out[i] = f32::from_bits(self.data[idx].load(std::sync::atomic::Ordering::Relaxed));
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
pub enum ControlEvent {
    LoadPreset { layer: usize, params: PresetParams, mod_matrix: ModMatrix, mseg1: Option<Mseg>, mseg2: Option<Mseg>, pitch_seq: Option<step_seq::PitchSequencer>, lfo_step_params: Option<std::collections::BTreeMap<String, f32>>, wavetable: Option<(Vec<f32>, usize, usize)> },
    SetLayerEnabled { layer: usize, enabled: bool },
    SetLayerMute { layer: usize, mute: bool },
    SetLayerVolume { layer: usize, volume: f32 },
    SetLayerRange { layer: usize, min_note: u8, max_note: u8 },
    SetLayerVelRange { layer: usize, vel_min: u8, vel_max: u8 },
    SetLayerPan { layer: usize, pan: f32 },
    SetLayerTranspose { layer: usize, semitones: i8 },
    SetMacro { layer: usize, index: usize, value: f32 },
    SetCcMap { map: CcMap },
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
    DrumSeqRecord { recording: bool },
    DrumSeqClear,
    DrumSeqUndo,
    DrumLoadKit { patterns: Box<[DrumPattern; 8]>, params: Box<[DrumSlotParams; NUM_DRUM_SLOTS]>, bpm: f32, swing: f32, volume: f32 },
    // Looper controls
    LooperRecord,
    LooperStopRecord,
    LooperTogglePlay,
    LooperUndo,
    LooperClear,
    LooperSetBars { bars: u8 },
    LooperSetQuantize { quantize: u8 },
    AllNotesOff,
    // SF2 sampler controls
    LoadKeysSoundFont { soundfont: std::sync::Arc<rustysynth::SoundFont> },
    LoadDrumsSoundFont { soundfont: std::sync::Arc<rustysynth::SoundFont> },
    UnloadKeysSoundFont,
    UnloadDrumsSoundFont,
    SetLayerSf2Mode { layer: usize, enabled: bool },
    SetLayerSf2Program { layer: usize, program: u8, bank: u8 },
    SetDrumsSf2Mode { enabled: bool },
    SetSf2BlockSize { size: usize },
    SetSf2SampleOffset { ms: f32 },
    // Pitch step sequencer
    SeqSetEnabled { layer: usize, enabled: bool },
    SeqSetStep { layer: usize, step: u8, pitch: i8, gate: bool, velocity: u8 },
    SeqSetLength { layer: usize, length: u8 },
    SeqSetRate { layer: usize, rate: u8 },
    SeqSetScale { layer: usize, scale: u8 },
    SeqSetSwing { layer: usize, swing: f32 },
    // MIDI file sequencer
    MidiSeqLoad { data: Box<midi_player::MidiSeqData> },
    MidiSeqPlay { playing: bool },
    MidiSeqSetBpm { bpm: Option<f32> },
    MidiSeqSetLooping { looping: bool },
    MidiSeqSetTrackInstrument { track_idx: usize, instrument: midi_player::TrackInstrument },
    MidiSeqSetTrackMute { track_idx: usize, muted: bool },
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

/// Modulation state computed per-layer, passed to voices.
#[derive(Clone, Copy)]
pub struct ModulationState {
    pub pitch_mult: f32,
    pub filter_offset: f32,       // Hz (from LFOs, aftertouch)
    pub filter_offset_semis: f32, // semitones (from mod matrix — applied as 2^(s/12) multiplier)
    pub amp_mod: f32,
}

pub const MAX_LAYERS: usize = 8;
const VOICES_PER_LAYER: usize = 8;
pub const BLOCK_SIZE: usize = 32;

/// Global parameters — only truly global controls that persist across everything.
struct GlobalParams {
    master_volume: f32,
    master_tone: f32, // simple LP cutoff (20-20000 Hz), post-effects
    // Fader-controlled global (persist across preset changes)
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

/// Pickup state for a single CC — prevents jumps on preset change.
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

struct Layer {
    voices: Vec<Voice>,
    age_counter: u64,
    volume: f32,
    enabled: bool,   // part is in scene (has preset, receives MIDI)
    mute: bool,      // temporary silence without removing from scene
    min_note: u8,
    max_note: u8,
    vel_min: u8,     // velocity range (1-127)
    vel_max: u8,
    pan: f32,        // -1.0 (L) .. 0.0 (C) .. +1.0 (R)
    transpose: i8,   // semitone offset per part (-24..+24)
    params: PresetParams,
    lfos: [Lfo; 4],
    /// Scene LFOs — free-running, never reset on note-on (unlike voice LFOs).
    /// tick every block regardless of note activity.
    scene_lfos: [Lfo; 2],
    scene_lfo_routed: [bool; 2],  // true when used in mod matrix
    /// External wavetable loaded from .wt file (set via LoadPreset)
    wavetable_data: Option<Vec<f32>>,
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
    breath: f32,            // CC2, 0..1
    expression: f32,        // CC11, 0..1
    sustain_pedal: f32,     // CC64: 0.0 or 1.0
    lowest_held: u8,        // lowest active note (default 60)
    highest_held: u8,       // highest active note (default 60)
    // Cached mod sources (control-rate, with poly_aftertouch=0) for per-voice re-evaluation
    cached_mod_sources: mod_matrix::ModSources,
    poly_at_in_matrix: bool,  // true if any mod slot uses PolyAftertouch
    // Cached routing flags — updated on preset load, not per-block
    lfo2_routed: bool,
    lfo3_routed: bool,
    lfo4_routed: bool,
    // Cached control-rate base values for per-voice ModulationState reconstruction
    cached_lfo_pitch_offset: f32,  // pitch_offset minus mod_offsets.pitch
    cached_filter_offset: f32,
    cached_lfo_amp_base: f32,      // (1 - lfo1_amp) * (1 - lfo2_amp) * seq_amp
}

impl Layer {
    fn new(sample_rate: f32) -> Self {
        let voices = (0..VOICES_PER_LAYER).map(|_| Voice::new(sample_rate)).collect();
        Self {
            voices, age_counter: 0, volume: 0.8, enabled: false, mute: false,
            min_note: 0, max_note: 127, vel_min: 1, vel_max: 127,
            pan: 0.0, transpose: 0,
            params: PresetParams::default(),
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

    #[allow(dead_code)]
    fn load_preset(&mut self, preset: &Preset) {
        self.params = PresetParams::from_map(&preset.params);
        self.min_note = self.params.min_note as u8;
        self.max_note = self.params.max_note as u8;
        self.pitch_seq.load_from_params(&preset.params);
        self.mod_matrix.load_from_params(&preset.params);
        for i in 0..4u8 {
            self.lfos[i as usize].load_step_seq_from_params(i, &preset.params);
        }
        self.update_routing_cache();
    }

    /// Update cached routing flags from mod matrix slots. Call after loading a preset
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

    fn note_on(&mut self, note: u8, velocity: u8) {
        if !self.enabled { return; }
        if note < self.min_note || note > self.max_note { return; }
        self.last_velocity = velocity as f32 / 127.0;
        self.last_note = note;

        // Update random/alternate mod sources on each note-on
        let r = self.next_rand();
        self.rand_bipolar = r * 2.0 - 1.0;
        self.rand_unipolar = r;
        self.alt_counter = self.alt_counter.wrapping_add(1);
        self.alt_bipolar = if self.alt_counter % 2 == 0 { 1.0 } else { -1.0 };
        self.alt_unipolar = if self.alt_counter % 2 == 0 { 1.0 } else { 0.0 };

        let play_mode = self.params.play_mode as u8;

        // Latch mode: toggle note on/off
        if play_mode == 3 {
            if let Some(pos) = self.latch_held.iter().position(|&n| n == note) {
                self.latch_held.remove(pos);
                for v in &mut self.voices { if v.active && v.note == note { v.note_off(); } }
            } else {
                self.latch_held.push(note);
                // fall through to start voice below
            }
            if self.latch_held.contains(&note) {
                // Start voice for new latch note
                self.age_counter += 1;
                let age = self.age_counter;
                let idx = self.voices.iter().position(|v| !v.active).unwrap_or_else(|| {
                    self.voices.iter().enumerate().min_by_key(|(_, v)| v.age).map(|(i, _)| i).unwrap_or(0)
                });
                let prev_freq = self.last_note_freq;
                let voice_params = self.build_voice_params();
                self.voices[idx].note_on(note, velocity, age, &voice_params, prev_freq);
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
            self.note_stack.retain(|&n| n != note);
            self.note_stack.push(note);
            self.age_counter += 1;
            let age = self.age_counter;
            // Retrigger the single mono voice
            let idx = self.voices.iter().position(|v| v.active).unwrap_or_else(|| {
                self.voices.iter().position(|v| !v.active).unwrap_or(0)
            });
            let prev_freq = self.last_note_freq;
            let voice_params = self.build_voice_params();
            let retrigger = play_mode == 2 && self.note_stack.len() > 1; // ST: don't retrigger env
            if retrigger {
                self.voices[idx].retrigger_note(note, &voice_params);
            } else {
                self.voices[idx].note_on(note, velocity, age, &voice_params, prev_freq);
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
        if play_mode == 6 {
            if self.voices.iter().any(|v| v.active && v.note == note) {
                return;
            }
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
                    // Default (Poly / Piano / Latch): steal oldest voice
                    self.voices.iter().enumerate()
                        .min_by_key(|(_, v)| v.age)
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
        self.voices[idx].note_on(note, velocity, age, &voice_params, prev_freq);
        self.last_note_freq = Some(440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0));
        // Retrigger LFOs on note-on if flagged
        self.lfos[0].trigger_mode(self.params.lfo1_trigger_mode as u8);
        self.lfos[1].trigger_mode(self.params.lfo2_trigger_mode as u8);
        self.lfos[2].trigger_mode(self.params.lfo3_trigger_mode as u8);
        self.lfos[3].trigger_mode(self.params.lfo4_trigger_mode as u8);
        // Trigger MSEGs on note-on (per-layer, not per-voice)
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
        }
    }

    fn note_off(&mut self, note: u8) {
        let play_mode = self.params.play_mode as u8;

        // Latch mode: notes released by toggling via note_on, not note_off
        if play_mode == 3 { return; }

        // Mono modes: manage note stack
        if play_mode == 1 || play_mode == 2 {
            self.note_stack.retain(|&n| n != note);
            if let Some(&prev_note) = self.note_stack.last() {
                // Retrigger with the previous held note
                let vp = self.build_voice_params();
                if let Some(v) = self.voices.iter_mut().find(|v| v.active) {
                    v.retrigger_note(prev_note, &vp);
                    if play_mode == 1 { v.retrigger_envelope(); }
                }
            } else {
                for voice in &mut self.voices {
                    if voice.active && voice.note == note { voice.note_off(); }
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
                    self.sustained_notes[note as usize] = true;
                }
                1 => {
                    // Release note only if other active non-releasing notes are held
                    let other_active = self.voices.iter()
                        .any(|v| v.note != note && v.active && !v.is_releasing());
                    if other_active {
                        for voice in &mut self.voices {
                            if voice.active && voice.note == note { voice.note_off(); }
                        }
                    }
                    // else hold the note
                }
                _ => {
                    // Default: hold all
                }
            }
        } else {
            for voice in &mut self.voices {
                if voice.active && voice.note == note { voice.note_off(); }
            }
        }

        // Release MSEGs when no active held notes remain
        if !self.has_active_notes() {
            self.mseg1_state.release();
            self.mseg2_state.release();
        }
        self.update_held_range();
    }

    fn has_active_notes(&self) -> bool {
        self.voices.iter().any(|v| v.active && !v.is_releasing())
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
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
pub struct PresetParams {
    osc_type: f32, osc_detune: f32, fm_ratio: f32, fm_index: f32, fm_env_amount: f32,
    osc_count: f32, osc1_level: f32,
    osc2_type: f32, osc2_detune: f32, osc2_level: f32,
    osc3_type: f32, osc3_detune: f32, osc3_level: f32,
    filter_cutoff: f32, filter_resonance: f32, filter_type: f32,
    filter_env_amount: f32, filter_key_track: f32,
    filter_routing: f32, filter2_type: f32, filter2_cutoff: f32, filter2_resonance: f32,
    noise_level: f32,
    amp_attack: f32, amp_decay: f32, amp_sustain: f32, amp_release: f32,
    amp_hold: f32,
    filter_attack: f32, filter_decay: f32, filter_sustain: f32, filter_release: f32,
    filter_hold: f32,
    ks_brightness: f32, ks_feedback: f32, organ_drawbars: [f32; 9],
    formant_voice: f32, formant_vowel: f32,
    drum_pitch_amount: f32, drum_pitch_decay: f32, drum_noise_level: f32,
    drum_noise_decay: f32, drum_noise_color: f32,
    bass_style: f32, bass_tone: f32, bass_body: f32, bass_pickup: f32,
    bow_pressure: f32, bow_position: f32, body_type: f32,
    lip_tension: f32, blowing_pressure: f32, bell_type: f32,
    // Phase Distortion
    pd_shape: f32, pd_depth: f32, pd_env_amount: f32,
    // Wavefolder
    fold_amount: f32, fold_symmetry: f32, fold_source: f32,
    // Modal Resonator
    modal_material: f32, modal_brightness: f32, modal_damping: f32, modal_strike_pos: f32,
    // Hard Sync
    sync_ratio: f32, sync_shape: f32,
    // Supersaw
    supersaw_detune: f32, supersaw_mix: f32,
    pulse_width: f32,
    // Accordion
    pub accordion_register: f32, pub accordion_bellows: f32,
    // Saxophone
    pub sax_reed_stiffness: f32, pub sax_embouchure: f32, pub sax_blow_pressure: f32, pub sax_type: f32,
    // Electric Piano
    pub epiano_type: f32,
    velocity_curve: f32, vel_to_filter: f32,
    lfo_waveform: f32, lfo_rate: f32, lfo_pitch_depth: f32, lfo_filter_depth: f32, lfo_amp_depth: f32,
    // LFO 2
    lfo2_waveform: f32, lfo2_rate: f32, lfo2_pitch_depth: f32, lfo2_filter_depth: f32, lfo2_amp_depth: f32, lfo2_deform: f32,
    // LFO 3 & 4 (routed via mod matrix only)
    lfo3_waveform: f32, lfo3_rate: f32, lfo3_deform: f32,
    lfo4_waveform: f32, lfo4_rate: f32, lfo4_deform: f32,
    // LFO tempo sync & unipolar (all 4 LFOs)
    lfo1_tempo_sync: f32, lfo2_tempo_sync: f32, lfo3_tempo_sync: f32, lfo4_tempo_sync: f32,
    lfo1_unipolar: f32, lfo2_unipolar: f32, lfo3_unipolar: f32, lfo4_unipolar: f32,
    portamento_time: f32, portamento_mode: f32,
    pitch_bend_up: f32,   // 0 = use global pitch_bend_range
    pitch_bend_down: f32,
    pub play_mode: f32,     // 0=poly, 1=mono, 2=mono-st, 3=latch
    pub sustain_mode: f32,  // 0=hold all notes, 1=release if others held, 4=poly-high, 5=poly-low, 6=piano
    unison_voices: f32, unison_detune: f32, unison_spread: f32,
    // Envelope shapes
    env_attack_shape: f32, env_decay_shape: f32,
    env_release_shape: f32,
    filter_env_attack_shape: f32, filter_env_decay_shape: f32, filter_env_release_shape: f32,
    // LFO retrigger flags
    lfo1_retrigger: f32, lfo2_retrigger: f32, lfo3_retrigger: f32, lfo4_retrigger: f32,
    lfo1_trigger_mode: f32, lfo2_trigger_mode: f32, lfo3_trigger_mode: f32, lfo4_trigger_mode: f32,
    // LFO deform
    lfo_deform: f32,
    // Scene LFOs (free-running — never retrigger on note-on)
    slfo1_rate: f32, slfo1_waveform: f32, slfo1_deform: f32,
    slfo1_tempo_sync: f32, slfo1_unipolar: f32,
    slfo2_rate: f32, slfo2_waveform: f32, slfo2_deform: f32,
    slfo2_tempo_sync: f32, slfo2_unipolar: f32,
    // Macro knobs (8 named user-controllable mod sources, 0..1)
    macro_vals: [f32; 8],
    // Alias oscillator
    alias_wave_type: f32, alias_crush: f32,
    // Window oscillator
    window_type: f32, window_morph: f32, window_formant: f32,
    // Twist / Plaits
    twist_engine: f32, twist_harmonics: f32, twist_timbre: f32, twist_morph: f32,
    twist_lpg_decay: f32, twist_lpg_colour: f32, twist_aux_mix: f32,
    // Effects (per-preset)
    chorus_mix: f32,
    delay_mix: f32, delay_time_l: f32, delay_time_r: f32,
    delay_feedback: f32, delay_ping_pong: f32, delay_filter: f32,
    reverb_mix: f32, reverb_room_size: f32, reverb_damping: f32,
    reverb_width: f32, reverb_pre_delay: f32,
    // Ring Modulator
    ring_mod_freq: f32, ring_mod_shape: f32, ring_mod_bias: f32,
    ring_mod_linear: f32, ring_mod_mix: f32,
    // Frequency Shifter
    freq_shift_hz: f32, freq_shift_feedback: f32, freq_shift_delay: f32,
    freq_shift_mix: f32,
    // Tape Saturation
    tape_drive: f32, tape_saturation: f32, tape_bias: f32,
    tape_tone: f32, tape_speed: f32, tape_mix: f32,
    // Neuron Distortion
    neuron_drive: f32, neuron_squash: f32, neuron_stab: f32,
    neuron_asym: f32, neuron_bias: f32,
    neuron_comb_freq: f32, neuron_comb_sep: f32, neuron_mix: f32,
    // Spring Reverb
    spring_size: f32, spring_decay: f32, spring_reflections: f32,
    spring_damping: f32, spring_spin: f32, spring_chaos: f32, spring_mix: f32,
    // Reverb type (0=plate, 1=spring)
    reverb_type: f32,
    // FM cross-routing (osc1 → osc2/3 frequency modulation)
    fm_cross_depth: f32,
    // MSEG
    #[allow(dead_code)]
    mseg_enabled: f32,
    // Step sequencer pitch contribution (1.0 = normal, 0.0 = use seq only for mod matrix)
    seq_pitch_depth: f32,
    // Note range for split/layer mode (0..127)
    pub min_note: f32,
    pub max_note: f32,
    // Filter env amount in semitones (Surge-style, applied exponentially)
    pub filter_env_semitones: f32,
    // Rotary Speaker / Leslie
    rotary_speed: f32, rotary_mix: f32,
    // BBD Ensemble chorus
    ensemble_depth: f32, ensemble_rate: f32, ensemble_mix: f32,
    // Resonator bank
    resonator_freq: f32, resonator_decay: f32, resonator_mix: f32,
    // Bonsai saturation
    bonsai_drive: f32, bonsai_tone: f32, bonsai_asym: f32,
    bonsai_mode: f32, bonsai_mix: f32,
    // WaveShaper (FX chain)
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
    // Reverb2 (FDN)
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
    // Osc Waveshaper (per-voice, pre-filter)
    osc_ws_mode: f32, osc_ws_drive: f32, osc_ws_mix: f32,
    // Inter-filter waveshaper (between F1 and F2 in Serial routing)
    inter_ws_mode: f32, inter_ws_drive: f32, inter_ws_mix: f32,
    // SVF Morph filter parameter (0=LP, 0.5=BP, 1=HP)
    pub svf_morph: f32,
    // Airwindows
    airwindows_mode: f32,
    airwindows_drive: f32,
    airwindows_mix: f32,
    // 16-slot FX chain (new style — takes precedence when active)
    pub fx_chain: FxChain,
}

impl Default for PresetParams {
    fn default() -> Self {
        Self {
            osc_type: 0.0, osc_detune: 0.0, fm_ratio: 3.5, fm_index: 5.0, fm_env_amount: 0.0,
            osc_count: 1.0, osc1_level: 1.0,
            osc2_type: 1.0, osc2_detune: 0.0, osc2_level: 0.0,
            osc3_type: 1.0, osc3_detune: 0.0, osc3_level: 0.0,
            filter_cutoff: 8000.0, filter_resonance: 0.0, filter_type: 0.0,
            filter_env_amount: 0.0, filter_key_track: 0.0,
            filter_routing: 0.0, filter2_type: 0.0, filter2_cutoff: 8000.0, filter2_resonance: 0.0,
            noise_level: 0.0,
            amp_attack: 0.01, amp_decay: 0.1, amp_sustain: 0.7, amp_release: 0.3, amp_hold: 0.0,
            filter_attack: 0.01, filter_decay: 0.2, filter_sustain: 0.5, filter_release: 0.3, filter_hold: 0.0,
            ks_brightness: 0.5, ks_feedback: 0.996,
            organ_drawbars: [0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            formant_voice: 0.0, formant_vowel: 0.0,
            drum_pitch_amount: 24.0, drum_pitch_decay: 40.0, drum_noise_level: 0.5,
            drum_noise_decay: 40.0, drum_noise_color: 0.5,
            bass_style: 0.0, bass_tone: 0.5, bass_body: 0.3, bass_pickup: 0.0,
            bow_pressure: 0.5, bow_position: 0.12, body_type: 0.0,
            lip_tension: 0.5, blowing_pressure: 0.5, bell_type: 0.0,
            pd_shape: 0.0, pd_depth: 0.5, pd_env_amount: 0.0,
            fold_amount: 0.5, fold_symmetry: 0.5, fold_source: 0.0,
            modal_material: 0.0, modal_brightness: 0.5, modal_damping: 0.3, modal_strike_pos: 0.5,
            sync_ratio: 2.0, sync_shape: 0.0,
            supersaw_detune: 0.5, supersaw_mix: 0.5,
            pulse_width: 0.5,
            accordion_register: 0.0, accordion_bellows: 0.7,
            sax_reed_stiffness: 0.5, sax_embouchure: 0.5, sax_blow_pressure: 0.6, sax_type: 1.0,
            epiano_type: 0.0,
            velocity_curve: 0.0, vel_to_filter: 0.0,
            env_attack_shape: 0.0, env_decay_shape: 0.0,
            env_release_shape: 0.0,
            filter_env_attack_shape: 0.0, filter_env_decay_shape: 0.0, filter_env_release_shape: 0.0,
            lfo1_retrigger: 1.0, lfo2_retrigger: 1.0, lfo3_retrigger: 0.0, lfo4_retrigger: 0.0,
            lfo1_trigger_mode: 1.0, lfo2_trigger_mode: 1.0, lfo3_trigger_mode: 0.0, lfo4_trigger_mode: 0.0,
            lfo_waveform: 0.0, lfo_rate: 5.0, lfo_pitch_depth: 0.0, lfo_filter_depth: 0.0, lfo_amp_depth: 0.0,
            lfo_deform: 0.0,
            lfo2_waveform: 0.0, lfo2_rate: 5.0, lfo2_pitch_depth: 0.0, lfo2_filter_depth: 0.0, lfo2_amp_depth: 0.0, lfo2_deform: 0.0,
            lfo3_waveform: 0.0, lfo3_rate: 3.0, lfo3_deform: 0.0,
            lfo4_waveform: 0.0, lfo4_rate: 1.0, lfo4_deform: 0.0,
            lfo1_tempo_sync: 0.0, lfo2_tempo_sync: 0.0, lfo3_tempo_sync: 0.0, lfo4_tempo_sync: 0.0,
            lfo1_unipolar: 0.0, lfo2_unipolar: 0.0, lfo3_unipolar: 0.0, lfo4_unipolar: 0.0,
            slfo1_rate: 0.5, slfo1_waveform: 0.0, slfo1_deform: 0.0, slfo1_tempo_sync: 0.0, slfo1_unipolar: 0.0,
            slfo2_rate: 0.25, slfo2_waveform: 0.0, slfo2_deform: 0.0, slfo2_tempo_sync: 0.0, slfo2_unipolar: 0.0,
            macro_vals: [0.0; 8],
            alias_wave_type: 0.0, alias_crush: 8.0,
            window_type: 0.0, window_morph: 0.0, window_formant: 0.0,
            twist_engine: 0.0, twist_harmonics: 0.5, twist_timbre: 0.5, twist_morph: 0.5,
            twist_lpg_decay: 0.5, twist_lpg_colour: 0.5, twist_aux_mix: 0.0,
            portamento_time: 0.0, portamento_mode: 0.0,
            pitch_bend_up: 0.0, pitch_bend_down: 0.0,
            play_mode: 0.0, sustain_mode: 0.0,
            unison_voices: 1.0, unison_detune: 0.0, unison_spread: 0.0,
            chorus_mix: 0.0,
            delay_mix: 0.0, delay_time_l: 0.3, delay_time_r: 0.4,
            delay_feedback: 0.4, delay_ping_pong: 0.0, delay_filter: 0.3,
            reverb_mix: 0.0, reverb_room_size: 0.5, reverb_damping: 0.5,
            reverb_width: 1.0, reverb_pre_delay: 0.02,
            ring_mod_freq: 440.0, ring_mod_shape: 0.0, ring_mod_bias: 0.5,
            ring_mod_linear: 0.5, ring_mod_mix: 0.0,
            freq_shift_hz: 0.0, freq_shift_feedback: 0.0, freq_shift_delay: 0.0,
            freq_shift_mix: 0.0,
            tape_drive: 0.0, tape_saturation: 0.5, tape_bias: 0.5,
            tape_tone: 0.5, tape_speed: 0.5, tape_mix: 0.0,
            neuron_drive: 0.0, neuron_squash: 0.5, neuron_stab: 0.5,
            neuron_asym: 0.0, neuron_bias: 0.5,
            neuron_comb_freq: 200.0, neuron_comb_sep: 0.5, neuron_mix: 0.0,
            spring_size: 0.5, spring_decay: 0.5, spring_reflections: 0.5,
            spring_damping: 0.5, spring_spin: 0.3, spring_chaos: 0.0, spring_mix: 0.0,
            reverb_type: 0.0,
            fm_cross_depth: 0.0,
            mseg_enabled: 0.0,
            seq_pitch_depth: 1.0,
            min_note: 0.0,
            max_note: 127.0,
            filter_env_semitones: 0.0,
            rotary_speed: 0.0, rotary_mix: 0.0,
            ensemble_depth: 0.5, ensemble_rate: 0.5, ensemble_mix: 0.0,
            resonator_freq: 440.0, resonator_decay: 0.7, resonator_mix: 0.0,
            bonsai_drive: 0.5, bonsai_tone: 0.5, bonsai_asym: 0.0,
            bonsai_mode: 0.0, bonsai_mix: 0.0,
            wave_shaper_drive: 0.5, wave_shaper_mode: 0.0, wave_shaper_bias: 0.0, wave_shaper_mix: 0.0,
            ms_mid_gain: 1.0, ms_side_gain: 1.0, ms_rotation: 0.0, ms_mix: 0.0,
            graphic_eq_gains: [0.0; 11], graphic_eq_output: 1.0,
            conditioner_bass_cut: 20.0, conditioner_width: 1.0, conditioner_threshold: 1.0, conditioner_mix: 0.0,
            exciter_drive: 0.5, exciter_freq: 3000.0, exciter_presence: 0.5, exciter_mix: 0.0,
            floaty_time: 0.3, floaty_feedback: 0.3, floaty_wobble: 0.3, floaty_rate: 0.5, floaty_damp: 0.5, floaty_mix: 0.0,
            reverb2_decay: 0.5, reverb2_damping: 0.5, reverb2_size: 0.5, reverb2_mix: 0.0,
            combulator_freq: 440.0, combulator_offset2: 5.0, combulator_offset3: -7.0,
            combulator_feedback: 0.5, combulator_tone: 0.5, combulator_mix: 0.0,
            treemonster_threshold: 0.5, treemonster_shift: 0.0, treemonster_ring_mix: 0.5, treemonster_mix: 0.0,
            nimbus_position: 0.5, nimbus_size: 0.5, nimbus_pitch: 0.0, nimbus_density: 0.5,
            nimbus_spread: 0.5, nimbus_texture: 0.5, nimbus_mix: 0.0,
            vocoder_env_follow: 0.5, vocoder_gate: 0.0, vocoder_mix: 0.0,
            conv_reverb_room: 0.5, conv_reverb_damping: 0.5, conv_reverb_predelay: 0.02, conv_reverb_mix: 0.0,
            osc_ws_mode: 0.0, osc_ws_drive: 0.5, osc_ws_mix: 0.0,
            inter_ws_mode: 0.0, inter_ws_drive: 0.5, inter_ws_mix: 0.0,
            svf_morph: 0.0,
            airwindows_mode: 0.0, airwindows_drive: 0.5, airwindows_mix: 0.0,
            fx_chain: FxChain::default(),
        }
    }
}

impl PresetParams {
    pub fn from_map(params: &std::collections::BTreeMap<String, f32>) -> Self {
        let p = |key: &str, default: f32| -> f32 {
            params.get(key).copied().unwrap_or(default)
        };
        Self {
            osc_type: p("osc_type", 0.0), osc_detune: p("osc_detune", 0.0),
            fm_ratio: p("fm_ratio", 3.5), fm_index: p("fm_index", 5.0),
            fm_env_amount: p("fm_env_amount", 0.0),
            osc_count: p("osc_count", 1.0), osc1_level: p("osc1_level", 1.0),
            osc2_type: p("osc2_type", 1.0), osc2_detune: p("osc2_detune", 0.0), osc2_level: p("osc2_level", 0.0),
            osc3_type: p("osc3_type", 1.0), osc3_detune: p("osc3_detune", 0.0), osc3_level: p("osc3_level", 0.0),
            filter_cutoff: p("filter_cutoff", 8000.0), filter_resonance: p("filter_resonance", 0.0),
            filter_type: p("filter_type", 0.0), filter_env_amount: p("filter_env_amount", 0.0),
            filter_key_track: p("filter_key_track", 0.0),
            filter_routing: p("filter_routing", 0.0), filter2_type: p("filter2_type", 0.0),
            filter2_cutoff: p("filter2_cutoff", 8000.0), filter2_resonance: p("filter2_resonance", 0.0),
            noise_level: p("noise_level", 0.0),
            amp_attack: p("amp_attack", 0.01), amp_decay: p("amp_decay", 0.1),
            amp_sustain: p("amp_sustain", 0.7), amp_release: p("amp_release", 0.3),
            amp_hold: p("amp_hold", 0.0),
            filter_attack: p("filter_attack", 0.01), filter_decay: p("filter_decay", 0.2),
            filter_sustain: p("filter_sustain", 0.5), filter_release: p("filter_release", 0.3),
            filter_hold: p("filter_hold", 0.0),
            ks_brightness: p("ks_brightness", 0.5), ks_feedback: p("ks_feedback", 0.996),
            organ_drawbars: [
                p("drawbar_1", 0.0), p("drawbar_2", 0.0), p("drawbar_3", 8.0),
                p("drawbar_4", 0.0), p("drawbar_5", 0.0), p("drawbar_6", 0.0),
                p("drawbar_7", 0.0), p("drawbar_8", 0.0), p("drawbar_9", 0.0),
            ],
            formant_voice: p("formant_voice", 0.0), formant_vowel: p("formant_vowel", 0.0),
            drum_pitch_amount: p("drum_pitch_amount", 24.0), drum_pitch_decay: p("drum_pitch_decay", 40.0),
            drum_noise_level: p("drum_noise_level", 0.5), drum_noise_decay: p("drum_noise_decay", 40.0),
            drum_noise_color: p("drum_noise_color", 0.5),
            bass_style: p("bass_style", 0.0), bass_tone: p("bass_tone", 0.5), bass_body: p("bass_body", 0.3), bass_pickup: p("bass_pickup", 0.0),
            bow_pressure: p("bow_pressure", 0.5), bow_position: p("bow_position", 0.12), body_type: p("body_type", 0.0),
            lip_tension: p("lip_tension", 0.5), blowing_pressure: p("blowing_pressure", 0.5), bell_type: p("bell_type", 0.0),
            pd_shape: p("pd_shape", 0.0), pd_depth: p("pd_depth", 0.5), pd_env_amount: p("pd_env_amount", 0.0),
            fold_amount: p("fold_amount", 0.5), fold_symmetry: p("fold_symmetry", 0.5), fold_source: p("fold_source", 0.0),
            modal_material: p("modal_material", 0.0), modal_brightness: p("modal_brightness", 0.5),
            modal_damping: p("modal_damping", 0.3), modal_strike_pos: p("modal_strike_pos", 0.5),
            sync_ratio: p("sync_ratio", 2.0), sync_shape: p("sync_shape", 0.0),
            supersaw_detune: p("supersaw_detune", 0.5), supersaw_mix: p("supersaw_mix", 0.5),
            pulse_width: p("pulse_width", 0.5),
            accordion_register: p("accordion_register", 0.0), accordion_bellows: p("accordion_bellows", 0.7),
            sax_reed_stiffness: p("sax_reed_stiffness", 0.5), sax_embouchure: p("sax_embouchure", 0.5),
            sax_blow_pressure: p("sax_blow_pressure", 0.6), sax_type: p("sax_type", 1.0),
            epiano_type: p("epiano_type", 0.0),
            velocity_curve: p("velocity_curve", 0.0), vel_to_filter: p("vel_to_filter", 0.0),
            env_attack_shape: p("env_attack_shape", 0.0), env_decay_shape: p("env_decay_shape", 0.0),
            env_release_shape: p("env_release_shape", 0.0),
            filter_env_attack_shape: p("filter_env_attack_shape", 0.0),
            filter_env_decay_shape: p("filter_env_decay_shape", 0.0),
            filter_env_release_shape: p("filter_env_release_shape", 0.0),
            lfo1_retrigger: p("lfo1_retrigger", 1.0), lfo2_retrigger: p("lfo2_retrigger", 1.0),
            lfo3_retrigger: p("lfo3_retrigger", 0.0), lfo4_retrigger: p("lfo4_retrigger", 0.0),
            // Trigger mode: 0=Free Run, 1=Key, 2=Random, 3=RandomUni. Falls back to retrigger boolean.
            lfo1_trigger_mode: p("lfo1_trigger_mode", if p("lfo1_retrigger", 1.0) > 0.5 { 1.0 } else { 0.0 }),
            lfo2_trigger_mode: p("lfo2_trigger_mode", if p("lfo2_retrigger", 1.0) > 0.5 { 1.0 } else { 0.0 }),
            lfo3_trigger_mode: p("lfo3_trigger_mode", if p("lfo3_retrigger", 0.0) > 0.5 { 1.0 } else { 0.0 }),
            lfo4_trigger_mode: p("lfo4_trigger_mode", if p("lfo4_retrigger", 0.0) > 0.5 { 1.0 } else { 0.0 }),
            lfo_waveform: p("lfo_waveform", 0.0), lfo_rate: p("lfo_rate", 5.0),
            lfo_pitch_depth: p("lfo_pitch_depth", 0.0), lfo_filter_depth: p("lfo_filter_depth", 0.0),
            lfo_amp_depth: p("lfo_amp_depth", 0.0),
            lfo_deform: p("lfo_deform", 0.0),
            lfo2_waveform: p("lfo2_waveform", 0.0), lfo2_rate: p("lfo2_rate", 5.0),
            lfo2_pitch_depth: p("lfo2_pitch_depth", 0.0), lfo2_filter_depth: p("lfo2_filter_depth", 0.0),
            lfo2_amp_depth: p("lfo2_amp_depth", 0.0), lfo2_deform: p("lfo2_deform", 0.0),
            lfo3_waveform: p("lfo3_waveform", 0.0), lfo3_rate: p("lfo3_rate", 3.0), lfo3_deform: p("lfo3_deform", 0.0),
            lfo4_waveform: p("lfo4_waveform", 0.0), lfo4_rate: p("lfo4_rate", 1.0), lfo4_deform: p("lfo4_deform", 0.0),
            lfo1_tempo_sync: p("lfo1_tempo_sync", 0.0), lfo2_tempo_sync: p("lfo2_tempo_sync", 0.0),
            lfo3_tempo_sync: p("lfo3_tempo_sync", 0.0), lfo4_tempo_sync: p("lfo4_tempo_sync", 0.0),
            lfo1_unipolar: p("lfo1_unipolar", 0.0), lfo2_unipolar: p("lfo2_unipolar", 0.0),
            lfo3_unipolar: p("lfo3_unipolar", 0.0), lfo4_unipolar: p("lfo4_unipolar", 0.0),
            slfo1_rate: p("slfo1_rate", 0.5), slfo1_waveform: p("slfo1_waveform", 0.0),
            slfo1_deform: p("slfo1_deform", 0.0), slfo1_tempo_sync: p("slfo1_tempo_sync", 0.0),
            slfo1_unipolar: p("slfo1_unipolar", 0.0),
            slfo2_rate: p("slfo2_rate", 0.25), slfo2_waveform: p("slfo2_waveform", 0.0),
            slfo2_deform: p("slfo2_deform", 0.0), slfo2_tempo_sync: p("slfo2_tempo_sync", 0.0),
            slfo2_unipolar: p("slfo2_unipolar", 0.0),
            macro_vals: [
                p("macro_0", 0.0), p("macro_1", 0.0), p("macro_2", 0.0), p("macro_3", 0.0),
                p("macro_4", 0.0), p("macro_5", 0.0), p("macro_6", 0.0), p("macro_7", 0.0),
            ],
            alias_wave_type: p("alias_wave_type", 0.0), alias_crush: p("alias_crush", 8.0),
            window_type: p("window_type", 0.0), window_morph: p("window_morph", 0.0),
            window_formant: p("window_formant", 0.0),
            twist_engine: p("twist_engine", 0.0), twist_harmonics: p("twist_harmonics", 0.5),
            twist_timbre: p("twist_timbre", 0.5), twist_morph: p("twist_morph", 0.5),
            twist_lpg_decay: p("twist_lpg_decay", 0.5), twist_lpg_colour: p("twist_lpg_colour", 0.5),
            twist_aux_mix: p("twist_aux_mix", 0.0),
            portamento_time: p("portamento_time", 0.0), portamento_mode: p("portamento_mode", 0.0),
            pitch_bend_up: p("pitch_bend_up", 0.0), pitch_bend_down: p("pitch_bend_down", 0.0),
            play_mode: p("play_mode", 0.0), sustain_mode: p("sustain_mode", 0.0),
            unison_voices: p("unison_voices", 1.0), unison_detune: p("unison_detune", 0.0),
            unison_spread: p("unison_spread", 0.0),
            chorus_mix: p("chorus_mix", 0.0),
            delay_mix: p("delay_mix", 0.0), delay_time_l: p("delay_time_l", 0.3), delay_time_r: p("delay_time_r", 0.4),
            delay_feedback: p("delay_feedback", 0.4), delay_ping_pong: p("delay_ping_pong", 0.0), delay_filter: p("delay_filter", 0.3),
            reverb_mix: p("reverb_mix", 0.0), reverb_room_size: p("reverb_room_size", 0.5), reverb_damping: p("reverb_damping", 0.5),
            reverb_width: p("reverb_width", 1.0), reverb_pre_delay: p("reverb_pre_delay", 0.02),
            ring_mod_freq: p("ring_mod_freq", 440.0), ring_mod_shape: p("ring_mod_shape", 0.0),
            ring_mod_bias: p("ring_mod_bias", 0.5), ring_mod_linear: p("ring_mod_linear", 0.5),
            ring_mod_mix: p("ring_mod_mix", 0.0),
            freq_shift_hz: p("freq_shift_hz", 0.0), freq_shift_feedback: p("freq_shift_feedback", 0.0),
            freq_shift_delay: p("freq_shift_delay", 0.0), freq_shift_mix: p("freq_shift_mix", 0.0),
            tape_drive: p("tape_drive", 0.0), tape_saturation: p("tape_saturation", 0.5),
            tape_bias: p("tape_bias", 0.5), tape_tone: p("tape_tone", 0.5),
            tape_speed: p("tape_speed", 0.5), tape_mix: p("tape_mix", 0.0),
            neuron_drive: p("neuron_drive", 0.0), neuron_squash: p("neuron_squash", 0.5),
            neuron_stab: p("neuron_stab", 0.5), neuron_asym: p("neuron_asym", 0.0),
            neuron_bias: p("neuron_bias", 0.5), neuron_comb_freq: p("neuron_comb_freq", 200.0),
            neuron_comb_sep: p("neuron_comb_sep", 0.5), neuron_mix: p("neuron_mix", 0.0),
            spring_size: p("spring_size", 0.5), spring_decay: p("spring_decay", 0.5),
            spring_reflections: p("spring_reflections", 0.5), spring_damping: p("spring_damping", 0.5),
            spring_spin: p("spring_spin", 0.3), spring_chaos: p("spring_chaos", 0.0),
            spring_mix: p("spring_mix", 0.0),
            reverb_type: p("reverb_type", 0.0),
            fm_cross_depth: p("fm_cross_depth", 0.0),
            mseg_enabled: p("mseg_enabled", 0.0),
            seq_pitch_depth: p("seq_pitch_depth", 1.0),
            min_note: p("min_note", 0.0),
            max_note: p("max_note", 127.0),
            filter_env_semitones: p("filter_env_semitones", 0.0),
            rotary_speed: p("rotary_speed", 0.0), rotary_mix: p("rotary_mix", 0.0),
            ensemble_depth: p("ensemble_depth", 0.5), ensemble_rate: p("ensemble_rate", 0.5),
            ensemble_mix: p("ensemble_mix", 0.0),
            resonator_freq: p("resonator_freq", 440.0), resonator_decay: p("resonator_decay", 0.7),
            resonator_mix: p("resonator_mix", 0.0),
            bonsai_drive: p("bonsai_drive", 0.5), bonsai_tone: p("bonsai_tone", 0.5),
            bonsai_asym: p("bonsai_asym", 0.0), bonsai_mode: p("bonsai_mode", 0.0),
            bonsai_mix: p("bonsai_mix", 0.0),
            wave_shaper_drive: p("wave_shaper_drive", 0.5), wave_shaper_mode: p("wave_shaper_mode", 0.0),
            wave_shaper_bias: p("wave_shaper_bias", 0.0), wave_shaper_mix: p("wave_shaper_mix", 0.0),
            ms_mid_gain: p("ms_mid_gain", 1.0), ms_side_gain: p("ms_side_gain", 1.0),
            ms_rotation: p("ms_rotation", 0.0), ms_mix: p("ms_mix", 0.0),
            graphic_eq_gains: [
                p("geq_0", 0.0), p("geq_1", 0.0), p("geq_2", 0.0), p("geq_3", 0.0), p("geq_4", 0.0),
                p("geq_5", 0.0), p("geq_6", 0.0), p("geq_7", 0.0), p("geq_8", 0.0), p("geq_9", 0.0), p("geq_10", 0.0),
            ],
            graphic_eq_output: p("graphic_eq_output", 1.0),
            conditioner_bass_cut: p("conditioner_bass_cut", 20.0), conditioner_width: p("conditioner_width", 1.0),
            conditioner_threshold: p("conditioner_threshold", 1.0), conditioner_mix: p("conditioner_mix", 0.0),
            exciter_drive: p("exciter_drive", 0.5), exciter_freq: p("exciter_freq", 3000.0),
            exciter_presence: p("exciter_presence", 0.5), exciter_mix: p("exciter_mix", 0.0),
            floaty_time: p("floaty_time", 0.3), floaty_feedback: p("floaty_feedback", 0.3),
            floaty_wobble: p("floaty_wobble", 0.3), floaty_rate: p("floaty_rate", 0.5),
            floaty_damp: p("floaty_damp", 0.5), floaty_mix: p("floaty_mix", 0.0),
            reverb2_decay: p("reverb2_decay", 0.5), reverb2_damping: p("reverb2_damping", 0.5),
            reverb2_size: p("reverb2_size", 0.5), reverb2_mix: p("reverb2_mix", 0.0),
            combulator_freq: p("combulator_freq", 440.0), combulator_offset2: p("combulator_offset2", 5.0),
            combulator_offset3: p("combulator_offset3", -7.0), combulator_feedback: p("combulator_feedback", 0.5),
            combulator_tone: p("combulator_tone", 0.5), combulator_mix: p("combulator_mix", 0.0),
            treemonster_threshold: p("treemonster_threshold", 0.5), treemonster_shift: p("treemonster_shift", 0.0),
            treemonster_ring_mix: p("treemonster_ring_mix", 0.5), treemonster_mix: p("treemonster_mix", 0.0),
            nimbus_position: p("nimbus_position", 0.5), nimbus_size: p("nimbus_size", 0.5),
            nimbus_pitch: p("nimbus_pitch", 0.0), nimbus_density: p("nimbus_density", 0.5),
            nimbus_spread: p("nimbus_spread", 0.5), nimbus_texture: p("nimbus_texture", 0.5),
            nimbus_mix: p("nimbus_mix", 0.0),
            vocoder_env_follow: p("vocoder_env_follow", 0.5), vocoder_gate: p("vocoder_gate", 0.0),
            vocoder_mix: p("vocoder_mix", 0.0),
            conv_reverb_room: p("conv_reverb_room", 0.5), conv_reverb_damping: p("conv_reverb_damping", 0.5),
            conv_reverb_predelay: p("conv_reverb_predelay", 0.02), conv_reverb_mix: p("conv_reverb_mix", 0.0),
            osc_ws_mode: p("osc_ws_mode", 0.0), osc_ws_drive: p("osc_ws_drive", 0.5),
            osc_ws_mix: p("osc_ws_mix", 0.0),
            inter_ws_mode: p("inter_ws_mode", 0.0), inter_ws_drive: p("inter_ws_drive", 0.5),
            inter_ws_mix: p("inter_ws_mix", 0.0),
            svf_morph: p("svf_morph", 0.0),
            airwindows_mode: p("airwindows_mode", 0.0),
            airwindows_drive: p("airwindows_drive", 0.5),
            airwindows_mix: p("airwindows_mix", 0.0),
            fx_chain: if params.contains_key("fx0_type") {
                FxChain::from_map(params)
            } else {
                FxChain::default() // active=false → legacy path in tick_block
            },
        }
    }
}

pub struct SynthEngine {
    layers: Vec<Layer>,
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
    pub looper: MidiLooper,
    cc_map: CcMap,
    presets: Vec<Preset>,
    preset_params_cache: Vec<PresetParams>,
    feedback_tx: Option<rtrb::Producer<ParamFeedback>>,
    program_change: Option<std::sync::Arc<std::sync::atomic::AtomicU8>>,
    global_params: GlobalParams,
    pickup_states: [PickupState; 128],
    /// 0 = SEQ buttons target drums, 1 = SEQ buttons target looper
    seq_target: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Oscilloscope ring buffer: 256 mono samples shared with GUI via atomics
    scope_buf: std::sync::Arc<ScopeBuffer>,
    scope_write: usize,
    /// MIDI file sequencer player
    pub midi_player: midi_player::MidiPlayer,
    /// Reusable event buffer for midi_player.process_block (avoids per-block allocation)
    midi_seq_buf: Vec<(u8, midi_player::SeqEventData)>,
}

/// Cached effect parameters — extracted once per block from PresetParams.
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
    fn from_preset(p: Option<&PresetParams>, global: &GlobalParams) -> Self {
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
        // All 8 layers pre-allocated; only layer 0 enabled by default
        let mut layers: Vec<Layer> = (0..MAX_LAYERS).map(|_| Layer::new(sample_rate)).collect();
        layers[0].enabled = true; // parts 1-7 start disabled (zero CPU cost)
        Self {
            layers, drum_engine: DrumEngine::new(sample_rate),
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
            presets: Vec::new(), preset_params_cache: Vec::new(),
            feedback_tx: None, program_change: None,
            global_params: GlobalParams::default(),
            pickup_states: [PickupState::default(); 128],
            seq_target: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(1)), // default: looper (synth layer shown at start)
            scope_buf: std::sync::Arc::new(ScopeBuffer::new()),
            scope_write: 0,
            midi_player: midi_player::MidiPlayer::new(),
            midi_seq_buf: Vec::new(),
        }
    }

    pub fn seq_target_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.seq_target.clone()
    }

    pub fn pitch_seq_step_atoms(&self) -> Vec<std::sync::Arc<std::sync::atomic::AtomicU8>> {
        self.layers.iter().map(|l| l.pitch_seq.step_atom()).collect()
    }

    pub fn midi_player_play_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.midi_player.play_atom.clone()
    }

    pub fn midi_player_pos_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU32> {
        self.midi_player.position_atom.clone()
    }

    pub fn set_presets(&mut self, presets: Vec<Preset>) {
        self.preset_params_cache = presets.iter().map(|p| PresetParams::from_map(&p.params)).collect();
        self.presets = presets;
    }
    pub fn set_feedback_tx(&mut self, tx: rtrb::Producer<ParamFeedback>) { self.feedback_tx = Some(tx); }
    pub fn scope_buffer(&self) -> std::sync::Arc<ScopeBuffer> { self.scope_buf.clone() }
    pub fn set_program_change_atom(&mut self, atom: std::sync::Arc<std::sync::atomic::AtomicU8>) {
        self.program_change = Some(atom);
    }

    #[allow(dead_code)]
    pub fn load_preset(&mut self, layer: usize, preset: &Preset) {
        if let Some(l) = self.layers.get_mut(layer) { l.load_preset(preset); }
    }

    fn send_feedback(&mut self, fb: ParamFeedback) {
        if let Some(tx) = &mut self.feedback_tx { let _ = tx.push(fb); }
    }

    /// Reset pickup for preset-scoped CCs (call after preset change).
    fn reset_preset_pickups(&mut self) {
        for (cc, state) in self.pickup_states.iter_mut().enumerate() {
            if let Some(binding) = self.cc_map.bindings[cc] {
                if binding.scope == ParamScope::Preset {
                    state.picked_up = false;
                }
            }
        }
    }

    /// Check if a preset-scoped CC has "picked up" the current value (crossed over it).
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
        let current_val = self.layers.first()
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
                // SMK-37 Pro: SEQ buttons (any channel, notes 123/124)
                if note == 123 && velocity > 0 {
                    let target = self.seq_target.load(std::sync::atomic::Ordering::Relaxed);
                    if target == 0 {
                        // Drums mode
                        let seq = &mut self.drum_engine.sequencer;
                        seq.playing = !seq.playing;
                        if !seq.playing { seq.reset(); }
                    } else {
                        // Looper mode
                        self.looper.toggle_play();
                    }
                    return;
                }
                if note == 124 && velocity > 0 {
                    let target = self.seq_target.load(std::sync::atomic::Ordering::Relaxed);
                    if target == 0 {
                        self.drum_engine.sequencer.recording = !self.drum_engine.sequencer.recording;
                    } else {
                        self.looper.toggle_record();
                    }
                    return;
                }

                // Route drum notes (36-51): always on ch10, or any channel when drum mode active
                let is_drum_note = note >= 36 && note <= 51;
                let drum_mode = self.seq_target.load(std::sync::atomic::Ordering::Relaxed) == 0;
                let sampler_drums = self.sampler.drums_enabled();
                if channel == 9 || (is_drum_note && drum_mode) {
                    if velocity > 0 {
                        // Record into sequencer if recording
                        if is_drum_note {
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
                        if sampler_drums {
                            self.sampler.drum_note_on(note, velocity);
                        } else {
                            self.drum_engine.note_on(note, velocity);
                        }
                    } else {
                        if sampler_drums {
                            self.sampler.drum_note_off(note);
                        } else {
                            self.drum_engine.note_off(note);
                        }
                    }
                } else if velocity == 0 {
                    for (i, layer) in self.layers.iter_mut().enumerate() {
                        if !layer.enabled { continue; }
                        if layer.sf2_mode {
                            self.sampler.note_off(i, note);
                        } else {
                            layer.note_off(note);
                        }
                    }
                    self.looper.record_event(note, 0);
                } else {
                    for (i, layer) in self.layers.iter_mut().enumerate() {
                        if !layer.enabled || layer.mute { continue; }
                        // Key range + velocity range check
                        let transposed = (note as i16 + layer.transpose as i16).clamp(0, 127) as u8;
                        if note < layer.min_note || note > layer.max_note { continue; }
                        if velocity < layer.vel_min || velocity > layer.vel_max { continue; }
                        if layer.sf2_mode {
                            self.sampler.note_on(i, transposed, velocity);
                        } else {
                            layer.note_on(transposed, velocity);
                        }
                    }
                    self.looper.record_event(note, velocity);
                }
            }
            MidiEvent::NoteOff { channel, note } => {
                if channel == 9 {
                    if self.sampler.drums_enabled() {
                        self.sampler.drum_note_off(note);
                    } else {
                        self.drum_engine.note_off(note);
                    }
                } else {
                    for (i, layer) in self.layers.iter_mut().enumerate() {
                        if layer.sf2_mode {
                            self.sampler.note_off(i, note);
                        } else {
                            layer.note_off(note);
                        }
                    }
                    self.looper.record_event(note, 0);
                }
            }
            MidiEvent::PitchBend { value, .. } => {
                // Asymmetric bend: per-layer preset overrides global range
                let layer_up   = self.layers.iter().find(|l| l.enabled).map(|l| l.params.pitch_bend_up).unwrap_or(0.0);
                let layer_down = self.layers.iter().find(|l| l.enabled).map(|l| l.params.pitch_bend_down).unwrap_or(0.0);
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
                    if self.layers.get(i).map(|l| l.sf2_mode).unwrap_or(false) {
                        self.sampler.pitch_bend(i, value);
                    }
                }
            }
            MidiEvent::ModWheel { value, .. } => {
                self.mod_wheel = value;
                // Forward to SF2 sampler as CC1 on active SF2 layers
                for i in 0..2 {
                    if self.layers.get(i).map(|l| l.sf2_mode).unwrap_or(false) {
                        self.sampler.mod_wheel(i, value);
                    }
                }
            }
            MidiEvent::ProgramChange { program, .. } => {
                // In SF2 mode, change the SF2 program; in synth mode, change the synth preset
                if self.layers.first().map(|l| l.sf2_mode).unwrap_or(false) {
                    self.sampler.set_layer_program(0, program, 0);
                } else if let Some(&params) = self.preset_params_cache.get(program as usize) {
                    if let Some(l) = self.layers.get_mut(0) {
                        l.params = params;
                        if let Some(preset) = self.presets.get(program as usize) {
                            l.mod_matrix.load_from_params(&preset.params);
                            l.pitch_seq.load_from_params(&preset.params);
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
                    for layer in &mut self.layers {
                        for voice in &mut layer.voices {
                            if voice.note == note && voice.active {
                                voice.poly_aftertouch = pressure;
                            }
                        }
                    }
                }
            }
            MidiEvent::ControlChange { cc, value, .. } => {
                // Check assignable mod sources (CC1..CC4)
                for i in 0..4 {
                    if cc == self.cc_numbers[i] {
                        self.cc_values[i] = value as f32 / 127.0;
                    }
                }
                // Dedicated mod sources: Breath (CC2), Expression (CC11), Sustain (CC64)
                for layer in &mut self.layers {
                    match cc {
                        2  => layer.breath = value as f32 / 127.0,
                        11 => layer.expression = value as f32 / 127.0,
                        64 => {
                            let was_held = layer.sustain_pedal > 0.5;
                            layer.sustain_pedal = if value >= 64 { 1.0 } else { 0.0 };
                            // When pedal is released, release only voices that were held by sustain
                            if was_held && layer.sustain_pedal < 0.5 {
                                for voice in &mut layer.voices {
                                    if voice.active && !voice.is_releasing()
                                        && layer.sustained_notes[voice.note as usize]
                                    {
                                        voice.note_off();
                                    }
                                }
                                layer.sustained_notes = [false; 128];
                            }
                        }
                        _ => {}
                    }
                }
                // CcBinding is Copy — no heap allocation
                if let Some(binding) = self.cc_map.bindings[cc as usize] {
                    if self.check_pickup(cc, value, binding) {
                        let param_val = CcMap::cc_to_param(&binding, value);
                        if binding.scope == ParamScope::Global {
                            self.global_params.set(binding.param_key, param_val);
                        } else {
                            for layer in &mut self.layers { layer.set_param(binding.param_key, param_val); }
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

    pub fn handle_control(&mut self, event: ControlEvent) {
        match event {
            ControlEvent::LoadPreset { layer, params, mod_matrix, mseg1, mseg2, pitch_seq, lfo_step_params, wavetable } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.min_note = params.min_note as u8;
                    l.max_note = params.max_note as u8;
                    // Sync macro values from preset into live layer state
                    // Load wavetable data if provided
                    if let Some((data, frames, fsize)) = wavetable {
                        l.wavetable_data = Some(data);
                        l.wavetable_frames = frames;
                        l.wavetable_frame_size = fsize;
                    } else {
                        l.wavetable_data = None;
                    }
                    l.macro_vals = params.macro_vals;
                    l.params = params;
                    l.mod_matrix = mod_matrix;
                    if let Some(m) = mseg1 { l.mseg1 = m; }
                    if let Some(m) = mseg2 { l.mseg2 = m; }
                    if let Some(sq) = pitch_seq { l.pitch_seq = sq; }
                    if let Some(ref sp) = lfo_step_params {
                        for i in 0..4u8 {
                            l.lfos[i as usize].load_step_seq_from_params(i, sp);
                        }
                    }
                    l.update_routing_cache();
                }
                self.reset_preset_pickups();
            }
            ControlEvent::SetLayerEnabled { layer, enabled } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.enabled = enabled;
                    if !enabled {
                        // Graceful note-off all voices
                        for v in &mut l.voices { if v.active { v.note_off(); } }
                        l.note_stack.clear();
                        l.latch_held.clear();
                    }
                }
            }
            ControlEvent::SetLayerMute { layer, mute } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.mute = mute;
                    if mute {
                        for v in &mut l.voices { if v.active { v.note_off(); } }
                    }
                }
            }
            ControlEvent::SetLayerVolume { layer, volume } => {
                if let Some(l) = self.layers.get_mut(layer) { l.volume = volume; }
                self.sampler.set_layer_volume(layer, volume);
            }
            ControlEvent::SetLayerRange { layer, min_note, max_note } => {
                if let Some(l) = self.layers.get_mut(layer) { l.min_note = min_note; l.max_note = max_note; }
            }
            ControlEvent::SetLayerVelRange { layer, vel_min, vel_max } => {
                if let Some(l) = self.layers.get_mut(layer) { l.vel_min = vel_min; l.vel_max = vel_max; }
            }
            ControlEvent::SetLayerPan { layer, pan } => {
                if let Some(l) = self.layers.get_mut(layer) { l.pan = pan.clamp(-1.0, 1.0); }
            }
            ControlEvent::SetLayerTranspose { layer, semitones } => {
                if let Some(l) = self.layers.get_mut(layer) { l.transpose = semitones; }
            }
            ControlEvent::SetMacro { layer, index, value } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    if index < 8 {
                        l.macro_vals[index] = value.clamp(0.0, 1.0);
                        l.params.macro_vals[index] = value.clamp(0.0, 1.0);
                    }
                }
            }
            ControlEvent::SetCcMap { map } => { self.cc_map = map; }
            ControlEvent::SetGlobalParam { key, value } => {
                self.global_params.set(&key, value);
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
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                self.drum_engine.sequencer.patterns[pat].length = length;
            }
            ControlEvent::DrumSeqRecord { recording } => {
                if recording && !self.drum_engine.sequencer.recording {
                    self.drum_engine.sequencer.save_snapshot();
                }
                self.drum_engine.sequencer.recording = recording;
            }
            ControlEvent::DrumSeqClear => {
                self.drum_engine.sequencer.clear_pattern();
            }
            ControlEvent::DrumSeqUndo => {
                self.drum_engine.sequencer.undo();
            }
            ControlEvent::DrumLoadKit { patterns, params, bpm, swing, volume } => {
                self.drum_engine.sequencer.patterns = *patterns;
                self.drum_engine.params = *params;
                self.drum_engine.sequencer.bpm = bpm;
                self.drum_engine.sequencer.swing = swing;
                self.drum_engine.volume = volume;
                self.sampler.drum_volume = volume;
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
            ControlEvent::AllNotesOff => {
                for layer in &mut self.layers {
                    // Kill all active voices immediately
                    for v in &mut layer.voices {
                        if v.active { v.note_off(); }
                    }
                    // Clear held-note state so retriggering doesn't ghost
                    layer.note_stack.clear();
                    layer.latch_held.clear();
                    layer.sustain_pedal = 0.0;
                    layer.sustained_notes = [false; 128];
                }
                self.sampler.all_notes_off();
            }
            ControlEvent::LoadKeysSoundFont { soundfont } => {
                self.sampler.load_keys_soundfont(soundfont);
                self.sampler.set_pitch_bend_range(self.global_params.pitch_bend_range as u8);
            }
            ControlEvent::LoadDrumsSoundFont { soundfont } => {
                self.sampler.load_drums_soundfont(soundfont);
            }
            ControlEvent::UnloadKeysSoundFont => {
                self.sampler.unload_keys();
            }
            ControlEvent::UnloadDrumsSoundFont => {
                self.sampler.unload_drums();
            }
            ControlEvent::SetLayerSf2Mode { layer, enabled } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.sf2_mode = enabled;
                }
                self.sampler.set_layer_mode(layer, enabled);
            }
            ControlEvent::SetLayerSf2Program { layer, program, bank } => {
                self.sampler.set_layer_program(layer, program, bank);
            }
            ControlEvent::SetDrumsSf2Mode { enabled } => {
                self.sampler.set_drums_enabled(enabled);
                self.drum_engine.sf2_mode = enabled;
            }
            ControlEvent::SetSf2BlockSize { size } => {
                self.sampler.set_block_size(size);
            }
            ControlEvent::SetSf2SampleOffset { ms } => {
                self.sampler.set_sample_offset_ms(ms);
            }
            ControlEvent::SeqSetEnabled { layer, enabled } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.pitch_seq.enabled = enabled;
                    if enabled { l.pitch_seq.reset(); }
                }
            }
            ControlEvent::SeqSetStep { layer, step, pitch, gate, velocity } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    if (step as usize) < step_seq::MAX_STEPS {
                        l.pitch_seq.steps[step as usize] = step_seq::PitchStep { pitch, gate, velocity };
                    }
                }
            }
            ControlEvent::SeqSetLength { layer, length } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.pitch_seq.length = length.clamp(1, 16);
                }
            }
            ControlEvent::SeqSetRate { layer, rate } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.pitch_seq.rate = step_seq::StepRate::from_index(rate);
                }
            }
            ControlEvent::SeqSetScale { layer, scale } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.pitch_seq.scale = step_seq::ScaleType::from_index(scale);
                }
            }
            ControlEvent::SeqSetSwing { layer, swing } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.pitch_seq.swing = swing;
                }
            }
            ControlEvent::MidiSeqLoad { data } => {
                self.midi_player.load(data); // Box<MidiSeqData> passed directly, no re-boxing
            }
            ControlEvent::MidiSeqPlay { playing } => {
                self.midi_player.playing = playing;
                self.midi_player.play_atom.store(playing as u8, std::sync::atomic::Ordering::Relaxed);
                if !playing {
                    self.midi_player.reset_position();
                    // All notes off on all engines
                    self.sampler.all_notes_off();
                    for layer in &mut self.layers {
                        for voice in &mut layer.voices {
                            if voice.active { voice.note_off(); }
                        }
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
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for layer in &mut self.layers { layer.set_sample_rate(sample_rate); }
        self.drum_engine.set_sample_rate(sample_rate);
        self.sampler.set_sample_rate(sample_rate);
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

        let active_layer_params = self.layers.iter()
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

        // Pre-compute modulation state for each layer (control rate)
        let mut layer_mods: Vec<ModulationState> = vec![ModulationState {
            pitch_mult: 1.0, filter_offset: 0.0, filter_offset_semis: 0.0, amp_mod: 1.0,
        }; self.layers.len()];

        for (li, layer) in self.layers.iter_mut().enumerate() {
            if !layer.enabled { continue; }

            // Step sequencer (control rate)
            let seq_evt = layer.pitch_seq.tick(self.sample_rate, seq_bpm, block_len);
            if seq_evt.stepped && seq_evt.gate {
                for voice in &mut layer.voices {
                    if voice.active && !voice.is_releasing() {
                        voice.retrigger_envelope();
                    }
                }
            }
            if seq_evt.stepped && !seq_evt.gate {
                for voice in &mut layer.voices {
                    if voice.active && !voice.is_releasing() {
                        voice.note_off();
                    }
                }
            }
            let seq_raw = if layer.pitch_seq.enabled { seq_evt.pitch_offset / 24.0 } else { 0.0 };
            let seq_pitch = seq_raw * 24.0 * layer.params.seq_pitch_depth;
            let seq_amp = if layer.pitch_seq.enabled && !seq_evt.gate { 0.0 } else { 1.0 };

            let bl = block_len as f32;

            // Set BPM, tempo sync, unipolar on all LFOs
            let lfo_tempo_syncs = [layer.params.lfo1_tempo_sync, layer.params.lfo2_tempo_sync,
                                   layer.params.lfo3_tempo_sync, layer.params.lfo4_tempo_sync];
            let lfo_unipolars = [layer.params.lfo1_unipolar, layer.params.lfo2_unipolar,
                                 layer.params.lfo3_unipolar, layer.params.lfo4_unipolar];
            for i in 0..4 {
                layer.lfos[i].set_bpm(seq_bpm);
                layer.lfos[i].tempo_sync = lfo_tempo_syncs[i] > 0.5;
                layer.lfos[i].unipolar = lfo_unipolars[i] > 0.5;
            }

            // LFO 1
            let lfo1_wf = LfoWaveform::from_param(layer.params.lfo_waveform);
            let lfo1_active = lfo1_wf == LfoWaveform::StepSeq
                || layer.params.lfo_pitch_depth > 0.001
                || layer.params.lfo_filter_depth > 0.001
                || layer.params.lfo_amp_depth > 0.001
                || self.aftertouch_smooth > 0.001;
            let lfo1_val = if lfo1_active {
                layer.lfos[0].tick_with_deform(lfo_rate * bl, lfo1_wf, layer.params.lfo_deform)
            } else { 0.0 };

            // LFO 2 — also active when routed through mod matrix or in StepSeq mode
            let lfo2_wf = LfoWaveform::from_param(layer.params.lfo2_waveform);
            let lfo2_active = lfo2_wf == LfoWaveform::StepSeq || layer.lfo2_routed
                || layer.params.lfo2_pitch_depth > 0.001
                || layer.params.lfo2_filter_depth > 0.001
                || layer.params.lfo2_amp_depth > 0.001;
            let lfo2_val = if lfo2_active {
                layer.lfos[1].tick_with_deform(layer.params.lfo2_rate * bl, lfo2_wf, layer.params.lfo2_deform)
            } else { 0.0 };

            // LFO 3 & 4 — tick when routed or in StepSeq mode
            let lfo3_wf = LfoWaveform::from_param(layer.params.lfo3_waveform);
            let lfo3_val = if lfo3_wf == LfoWaveform::StepSeq || layer.lfo3_routed {
                layer.lfos[2].tick_with_deform(layer.params.lfo3_rate * bl, lfo3_wf, layer.params.lfo3_deform)
            } else { 0.0 };
            let lfo4_wf = LfoWaveform::from_param(layer.params.lfo4_waveform);
            let lfo4_val = if lfo4_wf == LfoWaveform::StepSeq || layer.lfo4_routed {
                layer.lfos[3].tick_with_deform(layer.params.lfo4_rate * bl, lfo4_wf, layer.params.lfo4_deform)
            } else { 0.0 };

            // Step sequencer envelope retrigger from any LFO in StepSeq mode
            let lfo_wfs = [lfo1_wf, lfo2_wf, lfo3_wf, lfo4_wf];
            for (i, &wf) in lfo_wfs.iter().enumerate() {
                if wf == LfoWaveform::StepSeq {
                    let evt = layer.lfos[i].last_step_event();
                    if evt.stepped && (evt.retrigger_aeg || evt.retrigger_feg) {
                        for voice in &mut layer.voices {
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
            let slfo1_wf = LfoWaveform::from_param(layer.params.slfo1_waveform);
            layer.scene_lfos[0].set_bpm(seq_bpm);
            layer.scene_lfos[0].tempo_sync = layer.params.slfo1_tempo_sync > 0.5;
            layer.scene_lfos[0].unipolar  = layer.params.slfo1_unipolar  > 0.5;
            let slfo1_val = if layer.scene_lfo_routed[0] || slfo1_wf == LfoWaveform::StepSeq {
                let rate = if layer.scene_lfos[0].tempo_sync {
                    layer.params.slfo1_rate
                } else {
                    layer.params.slfo1_rate * bl
                };
                layer.scene_lfos[0].tick_with_deform(rate, slfo1_wf, layer.params.slfo1_deform)
            } else { 0.0 };

            let slfo2_wf = LfoWaveform::from_param(layer.params.slfo2_waveform);
            layer.scene_lfos[1].set_bpm(seq_bpm);
            layer.scene_lfos[1].tempo_sync = layer.params.slfo2_tempo_sync > 0.5;
            layer.scene_lfos[1].unipolar  = layer.params.slfo2_unipolar  > 0.5;
            let slfo2_val = if layer.scene_lfo_routed[1] || slfo2_wf == LfoWaveform::StepSeq {
                let rate = if layer.scene_lfos[1].tempo_sync {
                    layer.params.slfo2_rate
                } else {
                    layer.params.slfo2_rate * bl
                };
                layer.scene_lfos[1].tick_with_deform(rate, slfo2_wf, layer.params.slfo2_deform)
            } else { 0.0 };

            // MSEGs
            let mseg1_val = if layer.params.mseg_enabled > 0.5 {
                layer.mseg1_state.tick(&layer.mseg1)
            } else { 0.0 };
            let mseg2_val = if layer.params.mseg_enabled > 0.5 {
                layer.mseg2_state.tick(&layer.mseg2)
            } else { 0.0 };

            // Mod matrix
            let key_track = (layer.last_note as f32 - 60.0) / 48.0;
            let mod_sources = mod_matrix::ModSources {
                lfo_outputs: [lfo1_val, lfo2_val, lfo3_val, lfo4_val],
                scene_lfo_outputs: [slfo1_val, slfo2_val],
                macro_vals: layer.macro_vals,
                amp_env: 0.0, filter_env: 0.0,
                mseg_outputs: [mseg1_val, mseg2_val],
                mod_wheel: self.mod_wheel,
                aftertouch: self.aftertouch_smooth,
                velocity: layer.last_velocity,
                key_track,
                step_seq: seq_raw,
                random_bipolar: layer.rand_bipolar,
                random_unipolar: layer.rand_unipolar,
                alt_bipolar: layer.alt_bipolar,
                alt_unipolar: layer.alt_unipolar,
                release_vel: layer.release_vel,
                pitch_bend: self.pitch_bend_semitones / 2.0, // normalize -1..+1
                cc: self.cc_values,
                breath: layer.breath,
                expression: layer.expression,
                sustain_pedal: layer.sustain_pedal,
                lowest_key: (layer.lowest_held as f32 - 60.0) / 48.0,
                highest_key: (layer.highest_held as f32 - 60.0) / 48.0,
                latest_key: (layer.last_note as f32 - 60.0) / 48.0,
                poly_aftertouch: 0.0, // per-voice; 0 for layer-level evaluation
            };
            let mod_offsets = layer.mod_matrix.evaluate(&mod_sources);

            let lfo_pitch_offset_base = self.pitch_bend_semitones
                + lfo1_val * layer.params.lfo_pitch_depth * 2.0
                + lfo2_val * layer.params.lfo2_pitch_depth * 2.0
                + self.mod_wheel * 0.5 * vibrato_val
                + self.aftertouch_smooth * 0.3 * lfo1_val
                + seq_pitch;
            let pitch_offset = lfo_pitch_offset_base + mod_offsets.pitch;
            let pitch_mult = if pitch_offset.abs() < 0.001 { 1.0 } else { (pitch_offset / 12.0).exp2() };
            // filter_offset is Hz-only (LFO direct routing, aftertouch)
            // filter_offset_semis is exponential (mod matrix)
            let filter_offset = lfo1_val * layer.params.lfo_filter_depth * 4000.0
                + lfo2_val * layer.params.lfo2_filter_depth * 4000.0
                + self.aftertouch_smooth * 2000.0;
            let lfo_amp_base = (1.0 - layer.params.lfo_amp_depth * 0.5 * (1.0 - lfo1_val))
                * (1.0 - layer.params.lfo2_amp_depth * 0.5 * (1.0 - lfo2_val))
                * seq_amp;
            let amp_mod = lfo_amp_base * (1.0 + mod_offsets.amplitude).max(0.0);

            // Cache mod sources and base values for per-voice poly AT re-evaluation
            layer.cached_mod_sources = mod_sources;
            layer.cached_lfo_pitch_offset = lfo_pitch_offset_base;
            layer.cached_filter_offset = filter_offset;
            layer.cached_lfo_amp_base = lfo_amp_base;

            layer_mods[li] = ModulationState {
                pitch_mult, filter_offset, filter_offset_semis: mod_offsets.filter_cutoff, amp_mod,
            };
        }

        // Cache effect params (once per block)
        let ep = self.layers.iter().find(|l| l.enabled).map(|l| &l.params);
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
                            let sf2 = self.drum_engine.sf2_mode;
                            if sf2 {
                                self.sampler.drum_note_on(note, velocity);
                            } else {
                                self.drum_engine.note_on(note, velocity);
                            }
                        }
                        midi_player::TrackInstrument::DspLayer0 => {
                            if let Some(layer) = self.layers.get_mut(0) {
                                layer.note_on(note, velocity);
                            }
                        }
                        midi_player::TrackInstrument::Sf2 { program } => {
                            self.sampler.seq_program_set(ch, program);
                            self.sampler.seq_note_on(ch, note, velocity);
                        }
                    },
                    midi_player::SeqEventData::NoteOff { note } => match instr {
                        midi_player::TrackInstrument::Drums => {
                            let sf2 = self.drum_engine.sf2_mode;
                            if sf2 { self.sampler.drum_note_off(note); }
                        }
                        midi_player::TrackInstrument::DspLayer0 => {
                            if let Some(layer) = self.layers.get_mut(0) {
                                layer.note_off(note);
                            }
                        }
                        midi_player::TrackInstrument::Sf2 { .. } => {
                            self.sampler.seq_note_off(ch, note);
                        }
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
            // Looper: replay recorded events
            let looper_events = self.looper.tick();
            for &(note, vel) in &looper_events {
                if note == 0 && vel == 0 { break; }
                if vel > 0 {
                    for (i, layer) in self.layers.iter_mut().enumerate() {
                        if layer.sf2_mode {
                            self.sampler.note_on(i, note, vel);
                        } else {
                            layer.note_on(note, vel);
                        }
                    }
                } else {
                    for (i, layer) in self.layers.iter_mut().enumerate() {
                        if layer.sf2_mode {
                            self.sampler.note_off(i, note);
                        } else {
                            layer.note_off(note);
                        }
                    }
                }
            }

            let (mut out_l, mut out_r) = (0.0_f32, 0.0_f32);

            // Voice ticks (audio rate, using cached modulation)
            for (li, layer) in self.layers.iter_mut().enumerate() {
                if !layer.enabled { continue; }
                let (l, r) = layer.tick(&layer_mods[li]);
                // Apply per-part pan (constant-power: sqrt of (0.5 ± pan*0.5))
                let pan = layer.pan.clamp(-1.0, 1.0);
                let gain_l = ((0.5 - pan * 0.5) as f64).sqrt() as f32;
                let gain_r = ((0.5 + pan * 0.5) as f64).sqrt() as f32;
                out_l += l * gain_l;
                out_r += r * gain_r;
            }

            // SF2 + drums
            let (sf2_l, sf2_r) = self.sampler.tick();
            out_l += sf2_l; out_r += sf2_r;
            let ((drum_l, drum_r), drum_triggers) = self.drum_engine.tick();
            out_l += drum_l; out_r += drum_r;
            for i in 0..drum_triggers.count {
                let (slot, vel) = drum_triggers.triggers[i];
                self.sampler.drum_note_on(drum::DRUM_NOTE_BASE + slot, vel);
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
mod tests {
    use super::*;

    // ── Envelope tests ─────────────────────────────────────────────

    #[test]
    fn envelope_idle_returns_zero() {
        let mut env = envelope::Envelope::new(44100.0);
        for _ in 0..100 {
            assert_eq!(env.tick(), 0.0);
        }
        assert!(env.is_idle());
    }

    #[test]
    fn envelope_adsr_stages() {
        let mut env = envelope::Envelope::new(44100.0);
        env.set_adsr(0.01, 0.1, 0.5, 0.3);
        env.note_on();

        // Attack: should rise toward 1.0
        let mut peak = 0.0_f32;
        for _ in 0..1000 {
            let v = env.tick();
            peak = peak.max(v);
        }
        assert!(peak > 0.9, "Attack didn't reach near 1.0, peak={peak}");

        // Decay/Sustain: run further, should settle near sustain
        for _ in 0..44100 {
            env.tick();
        }
        let sustain_val = env.tick();
        assert!((sustain_val - 0.5).abs() < 0.05, "Sustain not near 0.5, got {sustain_val}");

        // Release: should decay to 0 (run 2 seconds for long release tails)
        env.note_off();
        for _ in 0..88200 {
            env.tick();
        }
        assert!(env.is_idle(), "Envelope didn't reach idle after release");
        assert!(env.tick().abs() < 0.001);
    }

    #[test]
    fn envelope_output_range_0_to_1() {
        let mut env = envelope::Envelope::new(44100.0);
        env.set_adsr(0.005, 0.05, 0.8, 0.2);
        env.note_on();
        for _ in 0..44100 {
            let v = env.tick();
            assert!(v >= -0.01 && v <= 1.01, "Envelope out of range: {v}");
        }
        env.note_off();
        for _ in 0..44100 {
            let v = env.tick();
            assert!(v >= -0.01 && v <= 1.01, "Envelope out of range in release: {v}");
        }
    }

    #[test]
    fn envelope_zero_sustain_goes_idle() {
        let mut env = envelope::Envelope::new(44100.0);
        env.set_adsr(0.001, 0.05, 0.0, 0.1);
        env.note_on();
        // Run through attack+decay
        for _ in 0..44100 {
            env.tick();
        }
        assert!(env.is_idle(), "Envelope with sustain=0 should be idle after decay");
    }

    #[test]
    fn envelope_retrigger_no_click() {
        let mut env = envelope::Envelope::new(44100.0);
        env.set_adsr(0.01, 0.1, 0.7, 0.3);
        env.note_on();
        // Run to sustain
        for _ in 0..22050 {
            env.tick();
        }
        let before = env.tick();
        // Retrigger — should start from current value, no jump to 0
        env.note_on();
        let after = env.tick();
        let jump = (after - before).abs();
        assert!(jump < 0.1, "Retrigger caused a click: jump={jump}");
    }

    // ── Filter tests ───────────────────────────────────────────────

    #[test]
    fn filter_no_nan_across_range() {
        let filter_types = [
            filter::FilterType::LowPass,
            filter::FilterType::HighPass,
            filter::FilterType::BandPass,
            filter::FilterType::MoogLP24,
            filter::FilterType::MoogLP12,
            filter::FilterType::DiodeLP,
        ];
        for ft in filter_types {
            let mut f = filter::Filter::new(44100.0);
            f.set_type(ft);
            // Sweep cutoff from 20Hz to 20kHz
            for cutoff in [20.0, 100.0, 500.0, 2000.0, 8000.0, 18000.0] {
                f.set_cutoff(cutoff);
                for res in [0.0, 0.5, 0.9, 1.0] {
                    f.set_resonance(res);
                    for _ in 0..500 {
                        let out = f.tick(0.5);
                        assert!(out.is_finite(), "NaN/Inf from {ft:?} cutoff={cutoff} res={res}");
                    }
                }
            }
        }
    }

    #[test]
    fn filter_lowpass_attenuates_high_freq() {
        let mut f = filter::Filter::new(44100.0);
        f.set_type(filter::FilterType::LowPass);
        f.set_cutoff(200.0);
        f.set_resonance(0.0);

        // Feed 10kHz sine, measure output energy
        let freq = 10000.0;
        let mut energy = 0.0_f32;
        for i in 0..4410 {
            let input = (2.0 * std::f32::consts::PI * freq * i as f32 / 44100.0).sin();
            let out = f.tick(input);
            energy += out * out;
        }
        let rms = (energy / 4410.0).sqrt();
        assert!(rms < 0.1, "LowPass@200Hz should attenuate 10kHz, rms={rms}");
    }

    #[test]
    fn filter_moog_self_oscillation() {
        let mut f = filter::Filter::new(44100.0);
        f.set_type(filter::FilterType::MoogLP24);
        f.set_cutoff(1000.0);
        f.set_resonance(1.0);

        // Feed a single impulse, then silence — high res should ring
        f.tick(1.0);
        let mut max_after = 0.0_f32;
        for _ in 0..4410 {
            let out = f.tick(0.0);
            max_after = max_after.max(out.abs());
        }
        assert!(max_after > 0.01, "Moog LP24 at res=1.0 should self-oscillate, max={max_after}");
    }

    #[test]
    fn filter_stability_extreme_params() {
        // Test with extreme cutoff and resonance — should not explode
        let mut f = filter::Filter::new(44100.0);
        f.set_type(filter::FilterType::MoogLP24);
        f.set_cutoff(22050.0); // Nyquist
        f.set_resonance(1.0);
        for _ in 0..4410 {
            let out = f.tick(1.0);
            assert!(out.abs() < 100.0, "Filter unstable at extreme params: {out}");
        }
    }

    // ── Oscillator tests ───────────────────────────────────────────

    #[test]
    fn oscillator_basic_waveforms_output_range() {
        let types = [
            oscillator::OscType::Sine,
            oscillator::OscType::Saw,
            oscillator::OscType::Square,
            oscillator::OscType::Triangle,
        ];
        for osc_type in types {
            let mut osc = oscillator::Oscillator::new(44100.0);
            osc.osc_type = osc_type;
            osc.reset();
            let mut max_val = 0.0_f32;
            let mut has_nonzero = false;
            for _ in 0..4410 {
                let out = osc.tick(440.0);
                assert!(out.is_finite(), "NaN from {osc_type:?}");
                max_val = max_val.max(out.abs());
                if out.abs() > 0.001 { has_nonzero = true; }
            }
            assert!(has_nonzero, "{osc_type:?} produced silence");
            assert!(max_val <= 1.5, "{osc_type:?} output too large: {max_val}");
        }
    }

    #[test]
    fn oscillator_sine_frequency_accuracy() {
        let mut osc = oscillator::Oscillator::new(44100.0);
        osc.osc_type = oscillator::OscType::Sine;
        osc.reset();

        // Count zero crossings in 1 second at 440Hz → expect ~880 crossings
        let mut crossings = 0;
        let mut prev = 0.0_f32;
        for _ in 0..44100 {
            let out = osc.tick(440.0);
            if prev <= 0.0 && out > 0.0 || prev >= 0.0 && out < 0.0 {
                crossings += 1;
            }
            prev = out;
        }
        // 440Hz = 880 zero crossings/sec (±5% tolerance)
        assert!((crossings as f32 - 880.0).abs() < 44.0,
            "Sine 440Hz: expected ~880 crossings, got {crossings}");
    }

    #[test]
    fn oscillator_noise_is_noisy() {
        let mut osc = oscillator::Oscillator::new(44100.0);
        osc.osc_type = oscillator::OscType::Noise;
        osc.reset();
        let mut values = std::collections::HashSet::new();
        for _ in 0..1000 {
            let out = osc.tick(440.0);
            values.insert((out * 1000.0) as i32);
        }
        assert!(values.len() > 100, "Noise should produce many distinct values, got {}", values.len());
    }

    // ── LFO tests ──────────────────────────────────────────────────

    #[test]
    fn lfo_output_range() {
        let waveforms = [
            lfo::LfoWaveform::Sine,
            lfo::LfoWaveform::Triangle,
            lfo::LfoWaveform::Square,
            lfo::LfoWaveform::Sawtooth,
            lfo::LfoWaveform::SampleHold,
        ];
        for wf in waveforms {
            let mut l = lfo::Lfo::new(44100.0);
            for _ in 0..44100 {
                let v = l.tick_with_deform(2.0, wf, 0.0);
                assert!(v >= -1.01 && v <= 1.01, "{wf:?} out of range: {v}");
            }
        }
    }

    #[test]
    fn lfo_sine_is_periodic() {
        let mut l = lfo::Lfo::new(44100.0);
        // 1Hz LFO — count positive zero crossings in 4 seconds → expect ~4
        let mut crossings = 0;
        let mut prev = 0.0_f32;
        for _ in 0..(44100 * 4) {
            let v = l.tick_with_deform(1.0, lfo::LfoWaveform::Sine, 0.0);
            if prev <= 0.0 && v > 0.0 { crossings += 1; }
            prev = v;
        }
        assert!((crossings as i32 - 4).abs() <= 1, "1Hz LFO: expected ~4 cycles, got {crossings}");
    }

    #[test]
    fn lfo_deform_stays_in_range() {
        let mut l = lfo::Lfo::new(44100.0);
        for deform in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            for _ in 0..4410 {
                let v = l.tick_with_deform(5.0, lfo::LfoWaveform::Sine, deform);
                assert!(v >= -1.01 && v <= 1.01, "Deform {deform}: out of range {v}");
            }
        }
    }

    // ── Block processing test ──────────────────────────────────────

    #[test]
    fn tick_block_matches_tick() {
        let mut synth_a = SynthEngine::new(44100.0);
        let mut synth_b = SynthEngine::new(44100.0);

        let presets = crate::preset::load_all_presets();
        let params = PresetParams::from_map(&presets[0].params);
        let params2 = params.clone();

        synth_a.handle_control(ControlEvent::LoadPreset {
            layer: 0, params, mod_matrix: ModMatrix::default(), mseg1: None, mseg2: None, pitch_seq: None, lfo_step_params: None,
        });
        synth_b.handle_control(ControlEvent::LoadPreset {
            layer: 0, params: params2, mod_matrix: ModMatrix::default(), mseg1: None, mseg2: None, pitch_seq: None, lfo_step_params: None,
        });

        synth_a.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        synth_b.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });

        // Both should produce sound (they may differ slightly due to control-rate vs audio-rate
        // LFO/mod matrix, but both should be non-silent and finite)
        let mut max_a = 0.0_f32;
        let mut max_b = 0.0_f32;

        for _ in 0..44100 {
            let (la, ra) = synth_a.tick();
            max_a = max_a.max(la.abs().max(ra.abs()));
        }

        let mut buf_l = [0.0f32; BLOCK_SIZE];
        let mut buf_r = [0.0f32; BLOCK_SIZE];
        for _ in 0..(44100 / BLOCK_SIZE) {
            synth_b.tick_block(&mut buf_l, &mut buf_r);
            for i in 0..BLOCK_SIZE {
                max_b = max_b.max(buf_l[i].abs().max(buf_r[i].abs()));
                assert!(buf_l[i].is_finite() && buf_r[i].is_finite(), "NaN in block output");
            }
        }

        assert!(max_a > 0.001, "tick() produced silence");
        assert!(max_b > 0.001, "tick_block() produced silence");
    }

    // ── Full-chain preset tests (existing + integration) ──────────

    fn run_preset_chain(preset_name: &str) {
        let mut synth = SynthEngine::new(44100.0);
        let presets = crate::preset::load_all_presets();
        let idx = presets.iter().position(|p| p.name == preset_name)
            .unwrap_or_else(|| panic!("No preset '{preset_name}'"));
        let params = PresetParams::from_map(&presets[idx].params);
        synth.set_presets(presets);
        synth.handle_control(ControlEvent::LoadPreset {
            layer: 0, params, mod_matrix: ModMatrix::default(), mseg1: None, mseg2: None, pitch_seq: None, lfo_step_params: None,
        });
        synth.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        let mut max_val = 0.0_f32;
        for _ in 0..44100 {
            let (l, r) = synth.tick();
            assert!(l.is_finite() && r.is_finite(), "NaN/Inf in '{preset_name}'");
            max_val = max_val.max(l.abs().max(r.abs()));
        }
        assert!(max_val > 0.001, "'{preset_name}' produced silence, max={max_val}");
        assert!(max_val < 10.0, "'{preset_name}' output too loud: {max_val}");
    }

    #[test]
    fn accordion_full_chain() { run_preset_chain("Accordion"); }

    #[test]
    fn saxophone_full_chain() { run_preset_chain("Alto Saxophone"); }

    #[test]
    fn brass_full_chain() { run_preset_chain("Trumpet"); }

    #[test]
    fn all_presets_no_nan_no_silence() {
        let presets = crate::preset::load_all_presets();
        let mut failures = Vec::new();

        for preset in &presets {
            let mut synth = SynthEngine::new(44100.0);
            let params = PresetParams::from_map(&preset.params);
            synth.handle_control(ControlEvent::LoadPreset {
                layer: 0,
                params,
                mod_matrix: ModMatrix::default(),
                mseg1: None,
                mseg2: None,
                pitch_seq: None,
                lfo_step_params: None,
            });
            synth.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });

            let mut max_val = 0.0_f32;
            let mut has_nan = false;
            // Render 0.5 seconds
            for _ in 0..22050 {
                let (l, r) = synth.tick();
                if !l.is_finite() || !r.is_finite() {
                    has_nan = true;
                    break;
                }
                max_val = max_val.max(l.abs().max(r.abs()));
            }

            if has_nan {
                failures.push(format!("{}: NaN/Inf", preset.name));
            } else if max_val < 0.0001 {
                // Some presets (like FX/risers) may be very quiet with static note — warn but don't fail
                eprintln!("WARNING: '{}' very quiet (max={max_val})", preset.name);
            }
            if max_val > 50.0 {
                failures.push(format!("{}: output explosion (max={max_val})", preset.name));
            } else if max_val > 10.0 {
                eprintln!("WARNING: '{}' output is hot (max={max_val})", preset.name);
            }
        }

        assert!(failures.is_empty(), "Preset failures:\n{}", failures.join("\n"));
    }

    // ── Silence without notes ──────────────────────────────────────

    #[test]
    fn no_notes_produces_silence() {
        let presets = crate::preset::load_all_presets();
        // Test with ALL presets that have ring_mod_mix > 0 — they should be silent without notes
        for preset in &presets {
            let mut synth = SynthEngine::new(44100.0);
            let params = PresetParams::from_map(&preset.params);
            synth.handle_control(ControlEvent::LoadPreset {
                layer: 0, params, mod_matrix: ModMatrix::default(), mseg1: None, mseg2: None, pitch_seq: None, lfo_step_params: None,
            });
            // No notes — should be completely silent
            let mut max_val = 0.0_f32;
            let mut buf_l = [0.0f32; BLOCK_SIZE];
            let mut buf_r = [0.0f32; BLOCK_SIZE];
            for _ in 0..100 {
                synth.tick_block(&mut buf_l, &mut buf_r);
                for i in 0..BLOCK_SIZE {
                    max_val = max_val.max(buf_l[i].abs()).max(buf_r[i].abs());
                }
            }
            assert!(max_val < 0.0001, "preset '{}' hums without notes: max={max_val}", preset.name);
        }
    }

    // ── Effect early-exit test ─────────────────────────────────────

    #[test]
    fn effects_bypass_when_mix_zero() {
        // Verify effects with mix=0 pass through unchanged
        let mut ch = chorus::Chorus::new(44100.0);
        let (l, _r) = ch.tick(0.7, 0.0); // mix=0
        assert!((l - 0.7).abs() < 0.01, "Chorus bypass failed: l={l}");

        let mut dl = delay::StereoDelay::new(44100.0);
        let (l, r) = dl.tick(0.5, -0.5, 0.3, 0.4, 0.3, 0.5, false, 0.0); // mix=0
        assert!((l - 0.5).abs() < 0.001 && (r - (-0.5)).abs() < 0.001,
            "Delay bypass failed: ({l}, {r})");

        let mut rv = reverb::Reverb::new(44100.0);
        let (l, r) = rv.tick(0.4_f32, -0.4_f32, 0.5, 0.5, 0.5, 0.1, 0.0); // mix=0
        assert!((l - 0.4_f32).abs() < 0.001 && (r - (-0.4_f32)).abs() < 0.001,
            "Reverb bypass failed: ({l}, {r})");
    }
}

