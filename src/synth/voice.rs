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
    filter_key_track: f32,
    filter_key_mult: f32,
    fm_base_index: f32,
    fm_env_amount: f32,
    noise_level: f32,
    noise_state: u32,
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
            filter_key_track: 0.0,
            filter_key_mult: 1.0,
            fm_base_index: 5.0,
            fm_env_amount: 0.0,
            noise_level: 0.0,
            noise_state: 0xDEADBEEF,
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
        self.fm_base_index = params.fm_index;
        self.fm_env_amount = params.fm_env_amount;

        // KS params
        self.osc.ks_brightness = params.ks_brightness;
        self.osc.ks_feedback = params.ks_feedback;

        // Organ drawbars
        self.osc.organ_drawbars = params.organ_drawbars;

        self.osc.reset();

        // Initialize physical model delay lines if needed
        match self.osc.osc_type {
            OscType::KarplusStrong => self.osc.init_ks(self.freq),
            OscType::CommutedPiano => self.osc.init_commuted_piano(self.freq),
            OscType::BandedWG => self.osc.init_banded_wg(self.freq),
            _ => {}
        }

        self.filter.set_type(FilterType::from_param(params.filter_type));
        self.filter.set_resonance(params.filter_resonance);
        self.filter.reset();
        self.filter_base_cutoff = params.filter_cutoff;
        self.filter_env_amount = params.filter_env_amount;
        self.filter_key_track = params.filter_key_track;
        self.filter_key_mult = if params.filter_key_track > 0.001 {
            2.0_f32.powf(params.filter_key_track * (note as f32 - 60.0) / 12.0)
        } else {
            1.0
        };
        self.noise_level = params.noise_level;

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

    #[inline]
    fn next_noise(&mut self) -> f32 {
        self.noise_state ^= self.noise_state << 13;
        self.noise_state ^= self.noise_state >> 17;
        self.noise_state ^= self.noise_state << 5;
        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
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

        let cutoff = (self.filter_base_cutoff + self.filter_env_amount * filter_mod) * self.filter_key_mult;
        self.filter.set_cutoff(cutoff);

        // FM index envelope: index decays with filter envelope
        if self.fm_env_amount > 0.001 {
            self.osc.fm_index =
                self.fm_base_index * (1.0 - self.fm_env_amount + self.fm_env_amount * filter_mod);
        }

        let osc_out = self.osc.tick(self.freq * pitch_mult);

        // Mix noise pre-filter (additive, for breathiness/texture)
        let mixed = if self.noise_level > 0.001 && self.osc.osc_type != OscType::Noise {
            osc_out + self.next_noise() * self.noise_level
        } else {
            osc_out
        };

        let filtered = self.filter.tick(mixed);

        filtered * amp * self.velocity
    }
}

/// Parameters extracted from preset, passed to voice on note-on.
pub struct VoiceParams {
    pub osc_type: f32,
    pub osc_detune: f32,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub fm_env_amount: f32,
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub filter_type: f32,
    pub filter_env_amount: f32,
    pub filter_key_track: f32,
    pub noise_level: f32,
    pub amp_attack: f32,
    pub amp_decay: f32,
    pub amp_sustain: f32,
    pub amp_release: f32,
    pub filter_attack: f32,
    pub filter_decay: f32,
    pub filter_sustain: f32,
    pub filter_release: f32,
    pub ks_brightness: f32,
    pub ks_feedback: f32,
    pub organ_drawbars: [f32; 9],
}

fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}
