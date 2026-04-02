/// Synth engine: layered polyphonic voice pools + MIDI event dispatch.

pub mod chorus;
pub mod envelope;
pub mod filter;
pub mod formant;
pub mod oscillator;
pub mod voice;

use crate::preset::Preset;
use chorus::Chorus;
use voice::{Voice, VoiceParams};

/// MIDI events sent from the MIDI thread to the audio thread.
pub enum MidiEvent {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8 },
    /// Pitch bend: -1.0 to +1.0 (center = 0.0)
    PitchBend { value: f32 },
    /// Mod wheel: 0.0 to 1.0
    ModWheel { value: f32 },
}

/// Control events sent from GUI to the audio thread.
pub enum ControlEvent {
    /// Load preset into a specific layer
    LoadPreset { layer: usize, preset: Preset },
    /// Enable/disable a layer
    SetLayerEnabled { layer: usize, enabled: bool },
    /// Set layer volume (0.0..1.0)
    SetLayerVolume { layer: usize, volume: f32 },
    /// Set layer note range (min_note..=max_note)
    SetLayerRange { layer: usize, min_note: u8, max_note: u8 },
}

pub const MAX_LAYERS: usize = 2;
const VOICES_PER_LAYER: usize = 12;

/// A single sound layer with its own voice pool and preset parameters.
struct Layer {
    voices: Vec<Voice>,
    age_counter: u64,
    volume: f32,
    enabled: bool,
    min_note: u8,
    max_note: u8,
    params: PresetParams,
    chorus_mix: f32,
}

impl Layer {
    fn new(sample_rate: f32) -> Self {
        let voices = (0..VOICES_PER_LAYER).map(|_| Voice::new(sample_rate)).collect();
        Self {
            voices,
            age_counter: 0,
            volume: 0.8,
            enabled: true,
            min_note: 0,
            max_note: 127,
            params: PresetParams::default(),
            chorus_mix: 0.0,
        }
    }

    fn load_preset(&mut self, preset: &Preset) {
        let p = |key: &str, default: f32| -> f32 {
            preset.params.get(key).copied().unwrap_or(default)
        };

        self.volume = p("master_volume", 0.8);
        self.chorus_mix = p("chorus_mix", 0.0);
        self.params = PresetParams {
            osc_type: p("osc_type", 0.0),
            osc_detune: p("osc_detune", 0.0),
            fm_ratio: p("fm_ratio", 3.5),
            fm_index: p("fm_index", 5.0),
            fm_env_amount: p("fm_env_amount", 0.0),
            // Multi-osc (defaults = single osc, backward compat)
            osc_count: p("osc_count", 1.0),
            osc1_level: p("osc1_level", 1.0),
            osc2_type: p("osc2_type", 1.0),
            osc2_detune: p("osc2_detune", 0.0),
            osc2_level: p("osc2_level", 0.0),
            osc3_type: p("osc3_type", 1.0),
            osc3_detune: p("osc3_detune", 0.0),
            osc3_level: p("osc3_level", 0.0),
            // Filter 1
            filter_cutoff: p("filter_cutoff", 8000.0),
            filter_resonance: p("filter_resonance", 0.0),
            filter_type: p("filter_type", 0.0),
            filter_env_amount: p("filter_env_amount", 0.0),
            filter_key_track: p("filter_key_track", 0.0),
            // Filter routing + filter 2 (defaults = single filter, backward compat)
            filter_routing: p("filter_routing", 0.0),
            filter2_type: p("filter2_type", 0.0),
            filter2_cutoff: p("filter2_cutoff", 8000.0),
            filter2_resonance: p("filter2_resonance", 0.0),
            // Other
            noise_level: p("noise_level", 0.0),
            amp_attack: p("amp_attack", 0.01),
            amp_decay: p("amp_decay", 0.1),
            amp_sustain: p("amp_sustain", 0.7),
            amp_release: p("amp_release", 0.3),
            filter_attack: p("filter_attack", 0.01),
            filter_decay: p("filter_decay", 0.2),
            filter_sustain: p("filter_sustain", 0.5),
            filter_release: p("filter_release", 0.3),
            ks_brightness: p("ks_brightness", 0.5),
            ks_feedback: p("ks_feedback", 0.996),
            organ_drawbars: [
                p("drawbar_1", 0.0),
                p("drawbar_2", 0.0),
                p("drawbar_3", 8.0),
                p("drawbar_4", 0.0),
                p("drawbar_5", 0.0),
                p("drawbar_6", 0.0),
                p("drawbar_7", 0.0),
                p("drawbar_8", 0.0),
                p("drawbar_9", 0.0),
            ],
            formant_voice: p("formant_voice", 0.0),
            formant_vowel: p("formant_vowel", 0.0),
        };
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if !self.enabled {
            return;
        }
        if note < self.min_note || note > self.max_note {
            return;
        }
        self.age_counter += 1;
        let age = self.age_counter;

        let idx = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });

        let voice_params = VoiceParams {
            osc_type: self.params.osc_type,
            osc_detune: self.params.osc_detune,
            fm_ratio: self.params.fm_ratio,
            fm_index: self.params.fm_index,
            fm_env_amount: self.params.fm_env_amount,
            osc_count: self.params.osc_count,
            osc1_level: self.params.osc1_level,
            osc2_type: self.params.osc2_type,
            osc2_detune: self.params.osc2_detune,
            osc2_level: self.params.osc2_level,
            osc3_type: self.params.osc3_type,
            osc3_detune: self.params.osc3_detune,
            osc3_level: self.params.osc3_level,
            filter_cutoff: self.params.filter_cutoff,
            filter_resonance: self.params.filter_resonance,
            filter_type: self.params.filter_type,
            filter_env_amount: self.params.filter_env_amount,
            filter_key_track: self.params.filter_key_track,
            filter_routing: self.params.filter_routing,
            filter2_type: self.params.filter2_type,
            filter2_cutoff: self.params.filter2_cutoff,
            filter2_resonance: self.params.filter2_resonance,
            noise_level: self.params.noise_level,
            amp_attack: self.params.amp_attack,
            amp_decay: self.params.amp_decay,
            amp_sustain: self.params.amp_sustain,
            amp_release: self.params.amp_release,
            filter_attack: self.params.filter_attack,
            filter_decay: self.params.filter_decay,
            filter_sustain: self.params.filter_sustain,
            filter_release: self.params.filter_release,
            ks_brightness: self.params.ks_brightness,
            ks_feedback: self.params.ks_feedback,
            organ_drawbars: self.params.organ_drawbars,
            formant_voice: self.params.formant_voice,
            formant_vowel: self.params.formant_vowel,
        };

        self.voices[idx].note_on(note, velocity, age, &voice_params);
    }

    fn note_off(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                voice.note_off();
            }
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        for voice in &mut self.voices {
            voice.set_sample_rate(sample_rate);
        }
    }

    fn tick(&mut self, pitch_mult: f32) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let mut out = 0.0;
        for voice in &mut self.voices {
            out += voice.tick(pitch_mult);
        }
        out * self.volume
    }
}

/// Cached preset parameters (updated when preset changes).
struct PresetParams {
    osc_type: f32,
    osc_detune: f32,
    fm_ratio: f32,
    fm_index: f32,
    fm_env_amount: f32,
    // Multi-osc
    osc_count: f32,
    osc1_level: f32,
    osc2_type: f32,
    osc2_detune: f32,
    osc2_level: f32,
    osc3_type: f32,
    osc3_detune: f32,
    osc3_level: f32,
    // Filter 1
    filter_cutoff: f32,
    filter_resonance: f32,
    filter_type: f32,
    filter_env_amount: f32,
    filter_key_track: f32,
    // Filter routing + filter 2
    filter_routing: f32,
    filter2_type: f32,
    filter2_cutoff: f32,
    filter2_resonance: f32,
    // Other
    noise_level: f32,
    amp_attack: f32,
    amp_decay: f32,
    amp_sustain: f32,
    amp_release: f32,
    filter_attack: f32,
    filter_decay: f32,
    filter_sustain: f32,
    filter_release: f32,
    ks_brightness: f32,
    ks_feedback: f32,
    organ_drawbars: [f32; 9],
    formant_voice: f32,
    formant_vowel: f32,
}

impl Default for PresetParams {
    fn default() -> Self {
        Self {
            osc_type: 0.0,
            osc_detune: 0.0,
            fm_ratio: 3.5,
            fm_index: 5.0,
            fm_env_amount: 0.0,
            osc_count: 1.0,
            osc1_level: 1.0,
            osc2_type: 1.0,
            osc2_detune: 0.0,
            osc2_level: 0.0,
            osc3_type: 1.0,
            osc3_detune: 0.0,
            osc3_level: 0.0,
            filter_cutoff: 8000.0,
            filter_resonance: 0.0,
            filter_type: 0.0,
            filter_env_amount: 0.0,
            filter_key_track: 0.0,
            filter_routing: 0.0,
            filter2_type: 0.0,
            filter2_cutoff: 8000.0,
            filter2_resonance: 0.0,
            noise_level: 0.0,
            amp_attack: 0.01,
            amp_decay: 0.1,
            amp_sustain: 0.7,
            amp_release: 0.3,
            filter_attack: 0.01,
            filter_decay: 0.2,
            filter_sustain: 0.5,
            filter_release: 0.3,
            ks_brightness: 0.5,
            ks_feedback: 0.996,
            organ_drawbars: [0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            formant_voice: 0.0,
            formant_vowel: 0.0,
        }
    }
}

pub struct SynthEngine {
    layers: Vec<Layer>,
    /// Pitch bend in semitones (-2..+2 by default, standard range)
    pitch_bend_semitones: f32,
    /// Mod wheel 0..1, controls vibrato depth
    mod_wheel: f32,
    /// LFO phase for mod wheel vibrato
    lfo_phase: f32,
    sample_rate: f32,
    chorus: Chorus,
}

impl SynthEngine {
    pub fn new(sample_rate: f32) -> Self {
        let mut layers: Vec<Layer> = (0..MAX_LAYERS).map(|_| Layer::new(sample_rate)).collect();
        // Layer B disabled by default
        if layers.len() > 1 {
            layers[1].enabled = false;
        }
        Self {
            layers,
            pitch_bend_semitones: 0.0,
            mod_wheel: 0.0,
            lfo_phase: 0.0,
            sample_rate,
            chorus: Chorus::new(sample_rate),
        }
    }

    pub fn load_preset(&mut self, layer: usize, preset: &Preset) {
        if let Some(l) = self.layers.get_mut(layer) {
            l.load_preset(preset);
        }
    }

    pub fn handle_event(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::NoteOn { note, velocity } => {
                if velocity == 0 {
                    for layer in &mut self.layers {
                        layer.note_off(note);
                    }
                } else {
                    for layer in &mut self.layers {
                        layer.note_on(note, velocity);
                    }
                }
            }
            MidiEvent::NoteOff { note } => {
                for layer in &mut self.layers {
                    layer.note_off(note);
                }
            }
            MidiEvent::PitchBend { value } => {
                self.pitch_bend_semitones = value * 2.0;
            }
            MidiEvent::ModWheel { value } => {
                self.mod_wheel = value;
            }
        }
    }

    pub fn handle_control(&mut self, event: ControlEvent) {
        match event {
            ControlEvent::LoadPreset { layer, preset } => {
                self.load_preset(layer, &preset);
            }
            ControlEvent::SetLayerEnabled { layer, enabled } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.enabled = enabled;
                }
            }
            ControlEvent::SetLayerVolume { layer, volume } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.volume = volume;
                }
            }
            ControlEvent::SetLayerRange { layer, min_note, max_note } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.min_note = min_note;
                    l.max_note = max_note;
                }
            }
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for layer in &mut self.layers {
            layer.set_sample_rate(sample_rate);
        }
        self.chorus.set_sample_rate(sample_rate);
    }

    /// Render one sample, returns (left, right) stereo pair.
    pub fn tick(&mut self) -> (f32, f32) {
        // LFO for mod wheel vibrato (5 Hz sine, up to ~0.5 semitones depth)
        const LFO_FREQ: f32 = 5.0;
        const LFO_MAX_DEPTH: f32 = 0.5; // semitones
        self.lfo_phase += LFO_FREQ / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let vibrato_semitones =
            self.mod_wheel * LFO_MAX_DEPTH * (self.lfo_phase * std::f32::consts::TAU).sin();

        // Total pitch offset in semitones
        let pitch_offset = self.pitch_bend_semitones + vibrato_semitones;
        let pitch_mult = if pitch_offset.abs() < 0.001 {
            1.0
        } else {
            (pitch_offset / 12.0).exp2()
        };

        let mut mono = 0.0;
        let mut chorus_mix = 0.0_f32;
        for layer in &mut self.layers {
            mono += layer.tick(pitch_mult);
            if layer.enabled {
                chorus_mix = chorus_mix.max(layer.chorus_mix);
            }
        }

        self.chorus.tick(mono, chorus_mix)
    }
}
