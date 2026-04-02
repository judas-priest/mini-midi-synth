/// Synth engine: polyphonic voice pool + MIDI event dispatch.

pub mod envelope;
pub mod filter;
pub mod oscillator;
pub mod voice;

use crate::preset::Preset;
use voice::{Voice, VoiceParams};

/// MIDI events sent from the MIDI thread to the audio thread.
pub enum MidiEvent {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8 },
}

/// Control events sent from TUI to the audio thread.
pub enum ControlEvent {
    LoadPreset(Preset),
}

const MAX_VOICES: usize = 16;

pub struct SynthEngine {
    voices: Vec<Voice>,
    age_counter: u64,
    master_volume: f32,
    params: PresetParams,
}

/// Cached preset parameters (updated when preset changes).
struct PresetParams {
    osc_type: f32,
    osc_detune: f32,
    fm_ratio: f32,
    fm_index: f32,
    filter_cutoff: f32,
    filter_resonance: f32,
    filter_type: f32,
    filter_env_amount: f32,
    amp_attack: f32,
    amp_decay: f32,
    amp_sustain: f32,
    amp_release: f32,
    filter_attack: f32,
    filter_decay: f32,
    filter_sustain: f32,
    filter_release: f32,
}

impl Default for PresetParams {
    fn default() -> Self {
        Self {
            osc_type: 0.0,
            osc_detune: 0.0,
            fm_ratio: 3.5,
            fm_index: 5.0,
            filter_cutoff: 8000.0,
            filter_resonance: 0.0,
            filter_type: 0.0,
            filter_env_amount: 0.0,
            amp_attack: 0.01,
            amp_decay: 0.1,
            amp_sustain: 0.7,
            amp_release: 0.3,
            filter_attack: 0.01,
            filter_decay: 0.2,
            filter_sustain: 0.5,
            filter_release: 0.3,
        }
    }
}

impl SynthEngine {
    pub fn new(sample_rate: f32) -> Self {
        let voices = (0..MAX_VOICES).map(|_| Voice::new(sample_rate)).collect();
        Self {
            voices,
            age_counter: 0,
            master_volume: 0.8,
            params: PresetParams::default(),
        }
    }

    pub fn load_preset(&mut self, preset: &Preset) {
        let p = |key: &str, default: f32| -> f32 {
            preset.params.get(key).copied().unwrap_or(default)
        };

        self.master_volume = p("master_volume", 0.8);
        self.params = PresetParams {
            osc_type: p("osc_type", 0.0),
            osc_detune: p("osc_detune", 0.0),
            fm_ratio: p("fm_ratio", 3.5),
            fm_index: p("fm_index", 5.0),
            filter_cutoff: p("filter_cutoff", 8000.0),
            filter_resonance: p("filter_resonance", 0.0),
            filter_type: p("filter_type", 0.0),
            filter_env_amount: p("filter_env_amount", 0.0),
            amp_attack: p("amp_attack", 0.01),
            amp_decay: p("amp_decay", 0.1),
            amp_sustain: p("amp_sustain", 0.7),
            amp_release: p("amp_release", 0.3),
            filter_attack: p("filter_attack", 0.01),
            filter_decay: p("filter_decay", 0.2),
            filter_sustain: p("filter_sustain", 0.5),
            filter_release: p("filter_release", 0.3),
        };
    }

    pub fn handle_event(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::NoteOn { note, velocity } => {
                if velocity == 0 {
                    self.note_off(note);
                } else {
                    self.note_on(note, velocity);
                }
            }
            MidiEvent::NoteOff { note } => self.note_off(note),
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.age_counter += 1;
        let age = self.age_counter;

        // Find a free voice, or steal the oldest active one
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
            filter_cutoff: self.params.filter_cutoff,
            filter_resonance: self.params.filter_resonance,
            filter_type: self.params.filter_type,
            filter_env_amount: self.params.filter_env_amount,
            amp_attack: self.params.amp_attack,
            amp_decay: self.params.amp_decay,
            amp_sustain: self.params.amp_sustain,
            amp_release: self.params.amp_release,
            filter_attack: self.params.filter_attack,
            filter_decay: self.params.filter_decay,
            filter_sustain: self.params.filter_sustain,
            filter_release: self.params.filter_release,
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

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        for voice in &mut self.voices {
            voice.set_sample_rate(sample_rate);
        }
    }

    /// Render one sample (mono).
    pub fn tick(&mut self) -> f32 {
        let mut out = 0.0;
        for voice in &mut self.voices {
            out += voice.tick();
        }
        out * self.master_volume
    }
}
