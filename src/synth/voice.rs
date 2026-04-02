/// Single synth voice: oscillator → filter → amplitude envelope.

use super::envelope::Envelope;
use super::filter::{Filter, FilterType};
use super::oscillator::{OscType, Oscillator};

#[derive(Clone)]
pub struct Voice {
    pub note: u8,
    pub active: bool,
    /// Monotonic counter set at note-on for voice-stealing (oldest first).
    pub age: u64,
    osc: Oscillator,
    filter: Filter,
    amp_env: Envelope,
    filter_env: Envelope,
    freq: f32,
    velocity: f32,
    filter_base_cutoff: f32,
    filter_env_amount: f32,
}

impl Voice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            note: 0,
            active: false,
            age: 0,
            osc: Oscillator::new(sample_rate),
            filter: Filter::new(sample_rate),
            amp_env: Envelope::new(sample_rate),
            filter_env: Envelope::new(sample_rate),
            freq: 440.0,
            velocity: 0.0,
            filter_base_cutoff: 8000.0,
            filter_env_amount: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.osc = Oscillator::new(sample_rate);
        self.filter = Filter::new(sample_rate);
        self.amp_env = Envelope::new(sample_rate);
        self.filter_env = Envelope::new(sample_rate);
        self.active = false;
    }

    pub fn note_on(&mut self, note: u8, velocity: u8, age: u64, params: &VoiceParams) {
        self.note = note;
        self.active = true;
        self.age = age;
        self.freq = midi_to_freq(note);
        self.velocity = velocity as f32 / 127.0;

        self.osc.osc_type = OscType::from_param(params.osc_type);
        self.osc.set_detune(params.osc_detune);
        self.osc.fm_ratio = params.fm_ratio;
        self.osc.fm_index = params.fm_index;
        self.osc.reset();

        self.filter.set_type(FilterType::from_param(params.filter_type));
        self.filter.set_resonance(params.filter_resonance);
        self.filter.reset();
        self.filter_base_cutoff = params.filter_cutoff;
        self.filter_env_amount = params.filter_env_amount;

        self.amp_env.set_adsr(
            params.amp_attack,
            params.amp_decay,
            params.amp_sustain,
            params.amp_release,
        );
        self.filter_env.set_adsr(
            params.filter_attack,
            params.filter_decay,
            params.filter_sustain,
            params.filter_release,
        );

        self.amp_env.note_on();
        self.filter_env.note_on();
    }

    pub fn note_off(&mut self) {
        self.amp_env.note_off();
        self.filter_env.note_off();
    }

    pub fn tick(&mut self, pitch_mult: f32) -> f32 {
        if !self.active {
            return 0.0;
        }

        let amp = self.amp_env.tick();
        if self.amp_env.is_idle() {
            self.active = false;
            return 0.0;
        }

        let filter_mod = self.filter_env.tick();
        let cutoff = self.filter_base_cutoff + self.filter_env_amount * filter_mod;
        self.filter.set_cutoff(cutoff);

        let osc_out = self.osc.tick(self.freq * pitch_mult);
        let filtered = self.filter.tick(osc_out);

        filtered * amp * self.velocity
    }
}

/// Parameters extracted from preset, passed to voice on note-on.
pub struct VoiceParams {
    pub osc_type: f32,
    pub osc_detune: f32,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub filter_type: f32,
    pub filter_env_amount: f32,
    pub amp_attack: f32,
    pub amp_decay: f32,
    pub amp_sustain: f32,
    pub amp_release: f32,
    pub filter_attack: f32,
    pub filter_decay: f32,
    pub filter_sustain: f32,
    pub filter_release: f32,
}

fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}
