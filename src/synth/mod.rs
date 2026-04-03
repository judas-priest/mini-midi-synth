/// Synth engine: layered polyphonic voice pools + MIDI event dispatch.

pub mod bass;
pub mod bitcrusher;
pub mod chorus;
pub mod compressor;
pub mod delay;
pub mod drum;
pub mod envelope;
pub mod eq;
pub mod filter;
pub mod flanger;
pub mod formant;
pub mod lfo;
pub mod looper;
pub mod sampler;
pub mod epiano;
pub mod oscillator;
pub mod overdrive;
pub mod piano;
pub mod phaser;
pub mod reverb;
pub mod tremolo;
pub mod voice;

use crate::cc_map::{CcMap, ParamScope};
use crate::preset::Preset;
use bitcrusher::Bitcrusher;
use chorus::Chorus;
use compressor::Compressor;
use delay::StereoDelay;
use drum::{DrumEngine, DrumPattern, DrumSlotParams, NUM_DRUM_SLOTS};
use eq::ParametricEq;
use flanger::Flanger;
use lfo::{Lfo, LfoWaveform};
use looper::MidiLooper;
use sampler::SamplerEngine;
use overdrive::Overdrive;
use phaser::Phaser;
use reverb::Reverb;
use tremolo::Tremolo;
use voice::{Voice, VoiceParams};

/// MIDI events sent from the MIDI thread to the audio thread.
#[allow(dead_code)]
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    PitchBend { channel: u8, value: f32 },
    ModWheel { channel: u8, value: f32 },
    ProgramChange { channel: u8, program: u8 },
    Aftertouch { channel: u8, value: f32 },
    ControlChange { channel: u8, cc: u8, value: u8 },
    /// SMK-37 Pro left/right button (SysEx F0 35 59 10 00 xx F7)
    Navigate { pressed: bool }, // true = press, false = release
}

/// Control events sent from GUI to the audio thread.
#[allow(dead_code)]
pub enum ControlEvent {
    LoadPreset { layer: usize, params: PresetParams },
    SetLayerEnabled { layer: usize, enabled: bool },
    SetLayerVolume { layer: usize, volume: f32 },
    SetLayerRange { layer: usize, min_note: u8, max_note: u8 },
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
    LoadSoundFont { soundfont: std::sync::Arc<rustysynth::SoundFont> },
    SetLayerSf2Mode { layer: usize, enabled: bool },
    SetLayerSf2Program { layer: usize, program: u8, bank: u8 },
    SetDrumsSf2Mode { enabled: bool },
    SetSf2BlockSize { size: usize },
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
    pub filter_offset: f32,
    pub amp_mod: f32,
}

pub const MAX_LAYERS: usize = 2;
const VOICES_PER_LAYER: usize = 12;

/// Global parameters — only truly global controls that persist across everything.
struct GlobalParams {
    master_volume: f32,
    master_tone: f32, // simple LP cutoff (20-20000 Hz), post-effects
    // One-pole LP state for master tone
    tone_lp_l: f32,
    tone_lp_r: f32,
}

impl Default for GlobalParams {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            master_tone: 20000.0,
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
    enabled: bool,
    min_note: u8,
    max_note: u8,
    params: PresetParams,
    lfo: Lfo,
    last_note_freq: Option<f32>,
    sf2_mode: bool,
}

impl Layer {
    fn new(sample_rate: f32) -> Self {
        let voices = (0..VOICES_PER_LAYER).map(|_| Voice::new(sample_rate)).collect();
        Self {
            voices, age_counter: 0, volume: 0.8, enabled: true,
            min_note: 0, max_note: 127, params: PresetParams::default(),
            lfo: Lfo::new(sample_rate), last_note_freq: None,
            sf2_mode: false,
        }
    }

    #[allow(dead_code)]
    fn load_preset(&mut self, preset: &Preset) {
        self.params = PresetParams::from_map(&preset.params);
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if !self.enabled { return; }
        if note < self.min_note || note > self.max_note { return; }
        self.age_counter += 1;
        let age = self.age_counter;
        let idx = self.voices.iter().position(|v| !v.active).unwrap_or_else(|| {
            self.voices.iter().enumerate().min_by_key(|(_, v)| v.age).map(|(i, _)| i).unwrap_or(0)
        });
        let porta_mode = self.params.portamento_mode as u8;
        let prev_freq = match porta_mode {
            1 => self.last_note_freq,
            2 => if self.has_active_notes() { self.last_note_freq } else { None },
            _ => None,
        };
        let voice_params = VoiceParams {
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
            filter_attack: self.params.filter_attack, filter_decay: self.params.filter_decay,
            filter_sustain: self.params.filter_sustain, filter_release: self.params.filter_release,
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
            velocity_curve: self.params.velocity_curve, vel_to_filter: self.params.vel_to_filter,
            portamento_time: self.params.portamento_time, _portamento_mode: self.params.portamento_mode,
            unison_voices: self.params.unison_voices, unison_detune: self.params.unison_detune,
            unison_spread: self.params.unison_spread,
        };
        self.voices[idx].note_on(note, velocity, age, &voice_params, prev_freq);
        self.last_note_freq = Some(440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0));
    }

    fn note_off(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note { voice.note_off(); }
        }
    }

    fn has_active_notes(&self) -> bool {
        self.voices.iter().any(|v| v.active && !v.is_releasing())
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        for voice in &mut self.voices { voice.set_sample_rate(sample_rate); }
        self.lfo.set_sample_rate(sample_rate);
    }

    fn tick(&mut self, mods: &ModulationState) -> (f32, f32) {
        if !self.enabled { return (0.0, 0.0); }
        let (mut out_l, mut out_r) = (0.0, 0.0);
        for voice in &mut self.voices {
            let (l, r) = voice.tick(mods);
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
    filter_attack: f32, filter_decay: f32, filter_sustain: f32, filter_release: f32,
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
    portamento_time: f32, portamento_mode: f32,
    unison_voices: f32, unison_detune: f32, unison_spread: f32,
    // Effects (per-preset)
    chorus_mix: f32,
    delay_mix: f32, delay_time_l: f32, delay_time_r: f32,
    delay_feedback: f32, delay_ping_pong: f32, delay_filter: f32,
    reverb_mix: f32, reverb_room_size: f32, reverb_damping: f32,
    reverb_width: f32, reverb_pre_delay: f32,
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
            amp_attack: 0.01, amp_decay: 0.1, amp_sustain: 0.7, amp_release: 0.3,
            filter_attack: 0.01, filter_decay: 0.2, filter_sustain: 0.5, filter_release: 0.3,
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
            lfo_waveform: 0.0, lfo_rate: 5.0, lfo_pitch_depth: 0.0, lfo_filter_depth: 0.0, lfo_amp_depth: 0.0,
            portamento_time: 0.0, portamento_mode: 0.0,
            unison_voices: 1.0, unison_detune: 0.0, unison_spread: 0.0,
            chorus_mix: 0.0,
            delay_mix: 0.0, delay_time_l: 0.3, delay_time_r: 0.4,
            delay_feedback: 0.4, delay_ping_pong: 0.0, delay_filter: 0.3,
            reverb_mix: 0.0, reverb_room_size: 0.5, reverb_damping: 0.5,
            reverb_width: 1.0, reverb_pre_delay: 0.02,
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
            filter_attack: p("filter_attack", 0.01), filter_decay: p("filter_decay", 0.2),
            filter_sustain: p("filter_sustain", 0.5), filter_release: p("filter_release", 0.3),
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
            lfo_waveform: p("lfo_waveform", 0.0), lfo_rate: p("lfo_rate", 5.0),
            lfo_pitch_depth: p("lfo_pitch_depth", 0.0), lfo_filter_depth: p("lfo_filter_depth", 0.0),
            lfo_amp_depth: p("lfo_amp_depth", 0.0),
            portamento_time: p("portamento_time", 0.0), portamento_mode: p("portamento_mode", 0.0),
            unison_voices: p("unison_voices", 1.0), unison_detune: p("unison_detune", 0.0),
            unison_spread: p("unison_spread", 0.0),
            chorus_mix: p("chorus_mix", 0.0),
            delay_mix: p("delay_mix", 0.0), delay_time_l: p("delay_time_l", 0.3), delay_time_r: p("delay_time_r", 0.4),
            delay_feedback: p("delay_feedback", 0.4), delay_ping_pong: p("delay_ping_pong", 0.0), delay_filter: p("delay_filter", 0.3),
            reverb_mix: p("reverb_mix", 0.0), reverb_room_size: p("reverb_room_size", 0.5), reverb_damping: p("reverb_damping", 0.5),
            reverb_width: p("reverb_width", 1.0), reverb_pre_delay: p("reverb_pre_delay", 0.02),
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
    sample_rate: f32,
    // Effects chain (order: EQ → Compressor → Overdrive → Phaser → Flanger → Tremolo → Bitcrusher → Chorus → Delay → Reverb)
    pub eq: ParametricEq,
    pub compressor: Compressor,
    pub overdrive: Overdrive,
    pub phaser: Phaser,
    pub flanger: Flanger,
    pub tremolo: Tremolo,
    pub bitcrusher: Bitcrusher,
    chorus: Chorus,
    delay: StereoDelay,
    reverb: Reverb,
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
}

impl SynthEngine {
    pub fn new(sample_rate: f32) -> Self {
        let mut layers: Vec<Layer> = (0..MAX_LAYERS).map(|_| Layer::new(sample_rate)).collect();
        if layers.len() > 1 { layers[1].enabled = false; }
        Self {
            layers, drum_engine: DrumEngine::new(sample_rate),
            sampler: SamplerEngine::new(sample_rate),
            pitch_bend_semitones: 0.0, mod_wheel: 0.0, vibrato_phase: 0.0,
            aftertouch: 0.0, aftertouch_smooth: 0.0, sample_rate,
            eq: ParametricEq::new(sample_rate),
            compressor: Compressor::new(sample_rate),
            overdrive: Overdrive::new(sample_rate),
            phaser: Phaser::new(sample_rate),
            flanger: Flanger::new(sample_rate),
            tremolo: Tremolo::new(sample_rate),
            bitcrusher: Bitcrusher::new(sample_rate),
            looper: MidiLooper::new(sample_rate),
            chorus: Chorus::new(sample_rate), delay: StereoDelay::new(sample_rate),
            reverb: Reverb::new(sample_rate), cc_map: CcMap::default(),
            presets: Vec::new(), preset_params_cache: Vec::new(),
            feedback_tx: None, program_change: None,
            global_params: GlobalParams::default(),
            pickup_states: [PickupState::default(); 128],
            seq_target: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(1)), // default: looper (synth layer shown at start)
        }
    }

    pub fn seq_target_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.seq_target.clone()
    }

    pub fn set_presets(&mut self, presets: Vec<Preset>) {
        self.preset_params_cache = presets.iter().map(|p| PresetParams::from_map(&p.params)).collect();
        self.presets = presets;
    }
    pub fn set_feedback_tx(&mut self, tx: rtrb::Producer<ParamFeedback>) { self.feedback_tx = Some(tx); }
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
                        if !layer.enabled { continue; }
                        if layer.sf2_mode {
                            if note >= layer.min_note && note <= layer.max_note {
                                self.sampler.note_on(i, note, velocity);
                            }
                        } else {
                            layer.note_on(note, velocity);
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
                self.pitch_bend_semitones = value * 2.0;
                for i in 0..2 {
                    if self.layers.get(i).map(|l| l.sf2_mode).unwrap_or(false) {
                        self.sampler.pitch_bend(i, value);
                    }
                }
            }
            MidiEvent::ModWheel { value, .. } => { self.mod_wheel = value; }
            MidiEvent::ProgramChange { program, .. } => {
                // In SF2 mode, change the SF2 program; in synth mode, change the synth preset
                if self.layers.first().map(|l| l.sf2_mode).unwrap_or(false) {
                    self.sampler.set_layer_program(0, program, 0);
                } else if let Some(&params) = self.preset_params_cache.get(program as usize) {
                    if let Some(l) = self.layers.get_mut(0) { l.params = params; }
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
            MidiEvent::ControlChange { cc, value, .. } => {
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
            ControlEvent::LoadPreset { layer, params } => {
                if let Some(l) = self.layers.get_mut(layer) { l.params = params; }
                self.reset_preset_pickups();
            }
            ControlEvent::SetLayerEnabled { layer, enabled } => {
                if let Some(l) = self.layers.get_mut(layer) { l.enabled = enabled; }
            }
            ControlEvent::SetLayerVolume { layer, volume } => {
                if let Some(l) = self.layers.get_mut(layer) { l.volume = volume; }
                self.sampler.set_layer_volume(layer, volume);
            }
            ControlEvent::SetLayerRange { layer, min_note, max_note } => {
                if let Some(l) = self.layers.get_mut(layer) { l.min_note = min_note; l.max_note = max_note; }
            }
            ControlEvent::SetCcMap { map } => { self.cc_map = map; }
            ControlEvent::SetGlobalParam { key, value } => {
                self.global_params.set(&key, value);
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
            ControlEvent::DrumSetVolume { volume } => { self.drum_engine.volume = volume; }
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
                    for note in 0..128u8 {
                        layer.note_off(note);
                    }
                }
                self.sampler.all_notes_off();
            }
            ControlEvent::LoadSoundFont { soundfont } => {
                self.sampler.load_soundfont(soundfont);
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
            }
            ControlEvent::SetSf2BlockSize { size } => {
                self.sampler.set_block_size(size);
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
        self.phaser.set_sample_rate(sample_rate);
        self.flanger.set_sample_rate(sample_rate);
        self.tremolo.set_sample_rate(sample_rate);
        self.bitcrusher.set_sample_rate(sample_rate);
        self.chorus.set_sample_rate(sample_rate);
        self.delay.set_sample_rate(sample_rate);
        self.reverb.set_sample_rate(sample_rate);
    }

    pub fn tick(&mut self) -> (f32, f32) {
        self.aftertouch_smooth += 0.002 * (self.aftertouch - self.aftertouch_smooth);

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

        let (mut out_l, mut out_r) = (0.0, 0.0);
        // Gather effect params from active layer (use first enabled layer's params)
        let active_layer_params = self.layers.iter()
            .find(|l| l.enabled)
            .map(|l| &l.params);
        let lfo_rate = active_layer_params.map(|p| p.lfo_rate).unwrap_or(5.0);

        // Dedicated vibrato oscillator for mod wheel (6 Hz sine, independent of preset LFO)
        let vibrato_val = if self.mod_wheel > 0.001 {
            self.vibrato_phase += 6.0 / self.sample_rate;
            if self.vibrato_phase >= 1.0 { self.vibrato_phase -= 1.0; }
            (self.vibrato_phase * std::f32::consts::TAU).sin()
        } else {
            self.vibrato_phase = 0.0;
            0.0
        };

        for layer in &mut self.layers {
            if !layer.enabled { continue; }
            let lfo_active = layer.params.lfo_pitch_depth > 0.001
                || layer.params.lfo_filter_depth > 0.001
                || layer.params.lfo_amp_depth > 0.001
                || self.aftertouch_smooth > 0.001;
            let lfo_val = if lfo_active {
                let lfo_wf = LfoWaveform::from_param(layer.params.lfo_waveform);
                layer.lfo.tick(lfo_rate, lfo_wf)
            } else {
                0.0
            };
            let pitch_offset = self.pitch_bend_semitones
                + lfo_val * layer.params.lfo_pitch_depth * 2.0
                + self.mod_wheel * 0.5 * vibrato_val
                + self.aftertouch_smooth * 0.3 * lfo_val;
            let pitch_mult = if pitch_offset.abs() < 0.001 { 1.0 } else { (pitch_offset / 12.0).exp2() };
            let filter_offset = lfo_val * layer.params.lfo_filter_depth * 4000.0 + self.aftertouch_smooth * 2000.0;
            let amp_mod = 1.0 - layer.params.lfo_amp_depth * 0.5 * (1.0 - lfo_val);
            let mods = ModulationState { pitch_mult, filter_offset, amp_mod };
            let (l, r) = layer.tick(&mods);
            out_l += l; out_r += r;
        }

        // Mix in SF2 sampler
        let (sf2_l, sf2_r) = self.sampler.tick();
        out_l += sf2_l;
        out_r += sf2_r;

        // Mix in drum engine
        let (drum_l, drum_r) = self.drum_engine.tick();
        out_l += drum_l;
        out_r += drum_r;

        // Effects chain: EQ → Compressor → Overdrive → Phaser → Flanger → Tremolo → Bitcrusher → Chorus → Delay → Reverb
        let (out_l, out_r) = self.eq.tick(out_l, out_r);
        let (out_l, out_r) = self.compressor.tick(out_l, out_r);
        let (out_l, out_r) = self.overdrive.tick(out_l, out_r);
        let (out_l, out_r) = self.phaser.tick(out_l, out_r);
        let (out_l, out_r) = self.flanger.tick(out_l, out_r);
        let (out_l, out_r) = self.tremolo.tick(out_l, out_r);
        let (out_l, out_r) = self.bitcrusher.tick(out_l, out_r);

        // Effect params from active layer
        let ep = self.layers.iter().find(|l| l.enabled).map(|l| &l.params);
        let chorus_mix = ep.map(|p| p.chorus_mix).unwrap_or(0.0);
        let mono = (out_l + out_r) * 0.5;
        let (cl, cr) = self.chorus.tick(mono, chorus_mix);
        let diff = (out_l - out_r) * 0.5;

        let delay_time_l = ep.map(|p| p.delay_time_l).unwrap_or(0.3);
        let delay_time_r = ep.map(|p| p.delay_time_r).unwrap_or(0.4);
        let delay_feedback = ep.map(|p| p.delay_feedback).unwrap_or(0.4);
        let delay_filter = ep.map(|p| p.delay_filter).unwrap_or(0.3);
        let delay_ping_pong = ep.map(|p| p.delay_ping_pong).unwrap_or(0.0);
        let delay_mix = ep.map(|p| p.delay_mix).unwrap_or(0.0);

        let (dl, dr) = self.delay.tick(
            cl + diff, cr - diff,
            delay_time_l, delay_time_r, delay_feedback,
            delay_filter, delay_ping_pong > 0.5, delay_mix,
        );

        let reverb_room = ep.map(|p| p.reverb_room_size).unwrap_or(0.5);
        let reverb_damp = ep.map(|p| p.reverb_damping).unwrap_or(0.5);
        let reverb_width = ep.map(|p| p.reverb_width).unwrap_or(1.0);
        let reverb_pre = ep.map(|p| p.reverb_pre_delay).unwrap_or(0.02);
        let reverb_mix = ep.map(|p| p.reverb_mix).unwrap_or(0.0);

        let (rl, rr) = self.reverb.tick(dl, dr, reverb_room, reverb_damp, reverb_width, reverb_pre, reverb_mix);

        // Master tone (simple LP, post-effects) + master volume
        let sample_rate = self.sample_rate;
        let (tl, tr) = self.global_params.apply_tone(rl, rr, sample_rate);
        let vol = self.global_params.master_volume;
        (tl * vol, tr * vol)
    }
}
