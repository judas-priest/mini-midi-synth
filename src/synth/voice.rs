/// Single synth voice: multi-oscillator → filter routing → amplitude envelope.

use super::envelope::Envelope;
use super::filter::{Filter, FilterType};
use super::formant::{FormantFilter, VoiceType, Vowel};
use super::oscillator::{OscType, Oscillator};

#[derive(Clone, Copy, PartialEq)]
pub enum FilterRouting {
    Single,   // 0: mix → filter1 → amp
    Serial,   // 1: mix → filter1 → filter2 → amp
    Parallel, // 2: mix → (filter1 + filter2) / 2 → amp
}

impl FilterRouting {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            1 => Self::Serial,
            2 => Self::Parallel,
            _ => Self::Single,
        }
    }
}

#[derive(Clone)]
pub struct Voice {
    pub note: u8,
    pub active: bool,
    /// Monotonic counter set at note-on for voice-stealing (oldest first).
    pub age: u64,
    oscs: [Oscillator; 3],
    num_oscs: u8,
    osc_levels: [f32; 3],
    filter: Filter,
    filter2: Filter,
    filter_routing: FilterRouting,
    formant_filter: FormantFilter,
    use_formant: bool,
    amp_env: Envelope,
    filter_env: Envelope,
    freq: f32,
    velocity: f32,
    filter_base_cutoff: f32,
    filter_env_amount: f32,
    filter_key_track: f32,
    filter_key_mult: f32,
    filter2_base_cutoff: f32,
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
            oscs: [
                Oscillator::new(sample_rate),
                Oscillator::new(sample_rate),
                Oscillator::new(sample_rate),
            ],
            num_oscs: 1,
            osc_levels: [1.0, 0.0, 0.0],
            filter: Filter::new(sample_rate),
            filter2: Filter::new(sample_rate),
            filter_routing: FilterRouting::Single,
            formant_filter: FormantFilter::new(sample_rate),
            use_formant: false,
            amp_env: Envelope::new(sample_rate),
            filter_env: Envelope::new(sample_rate),
            freq: 440.0,
            velocity: 0.0,
            filter_base_cutoff: 8000.0,
            filter_env_amount: 0.0,
            filter_key_track: 0.0,
            filter_key_mult: 1.0,
            filter2_base_cutoff: 8000.0,
            fm_base_index: 5.0,
            fm_env_amount: 0.0,
            noise_level: 0.0,
            noise_state: 0xDEADBEEF,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.oscs = [
            Oscillator::new(sample_rate),
            Oscillator::new(sample_rate),
            Oscillator::new(sample_rate),
        ];
        self.filter = Filter::new(sample_rate);
        self.filter2 = Filter::new(sample_rate);
        self.formant_filter = FormantFilter::new(sample_rate);
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

        // Osc 1 (primary — all types allowed)
        self.oscs[0].osc_type = OscType::from_param(params.osc_type);
        self.oscs[0].set_detune(params.osc_detune);
        self.oscs[0].fm_ratio = params.fm_ratio;
        self.oscs[0].fm_index = params.fm_index;
        self.oscs[0].ks_brightness = params.ks_brightness;
        self.oscs[0].ks_feedback = params.ks_feedback;
        self.oscs[0].organ_drawbars = params.organ_drawbars;
        self.fm_base_index = params.fm_index;
        self.fm_env_amount = params.fm_env_amount;

        self.oscs[0].reset();

        // Initialize physical model delay lines for osc 1
        match self.oscs[0].osc_type {
            OscType::KarplusStrong => self.oscs[0].init_ks(self.freq),
            OscType::CommutedPiano => self.oscs[0].init_commuted_piano(self.freq),
            OscType::BandedWG => self.oscs[0].init_banded_wg(self.freq),
            OscType::AdditivePiano => self.oscs[0].init_additive_piano(self.freq),
            _ => {}
        }

        // Multi-osc setup
        self.num_oscs = (params.osc_count as u8).clamp(1, 3);
        self.osc_levels[0] = params.osc1_level;

        // Osc 2 (simple types only)
        if self.num_oscs >= 2 {
            let osc2_type = OscType::from_param(params.osc2_type);
            self.oscs[1].osc_type = if osc2_type.is_simple() {
                osc2_type
            } else {
                OscType::Saw
            };
            self.oscs[1].set_detune(params.osc2_detune);
            self.oscs[1].fm_ratio = params.fm_ratio;
            self.oscs[1].fm_index = params.fm_index;
            self.oscs[1].reset();
            self.osc_levels[1] = params.osc2_level;
        } else {
            self.osc_levels[1] = 0.0;
        }

        // Osc 3 (simple types only)
        if self.num_oscs >= 3 {
            let osc3_type = OscType::from_param(params.osc3_type);
            self.oscs[2].osc_type = if osc3_type.is_simple() {
                osc3_type
            } else {
                OscType::Saw
            };
            self.oscs[2].set_detune(params.osc3_detune);
            self.oscs[2].fm_ratio = params.fm_ratio;
            self.oscs[2].fm_index = params.fm_index;
            self.oscs[2].reset();
            self.osc_levels[2] = params.osc3_level;
        } else {
            self.osc_levels[2] = 0.0;
        }

        // Filter 1 setup
        let filter_type = FilterType::from_param(params.filter_type);
        self.use_formant = filter_type == FilterType::Formant;

        if self.use_formant {
            let voice_type = VoiceType::from_param(params.formant_voice);
            let vowel = Vowel::from_param(params.formant_vowel);
            self.formant_filter.set_voice_vowel(voice_type, vowel);
            self.formant_filter.reset();
            self.filter_routing = FilterRouting::Single;
        } else {
            self.filter.set_type(filter_type);
            self.filter.set_resonance(params.filter_resonance);
            self.filter.reset();

            // Filter routing + filter 2
            self.filter_routing = FilterRouting::from_param(params.filter_routing);
            if self.filter_routing != FilterRouting::Single {
                let f2_type = FilterType::from_param(params.filter2_type);
                if f2_type == FilterType::Formant {
                    // Filter 2 doesn't support formant, fall back to single
                    self.filter_routing = FilterRouting::Single;
                } else {
                    self.filter2.set_type(f2_type);
                    self.filter2.set_resonance(params.filter2_resonance);
                    self.filter2.reset();
                    self.filter2_base_cutoff = params.filter2_cutoff;
                }
            }
        }

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

        // FM index envelope: index decays with filter envelope (osc 1 only)
        if self.fm_env_amount > 0.001 {
            self.oscs[0].fm_index =
                self.fm_base_index * (1.0 - self.fm_env_amount + self.fm_env_amount * filter_mod);
        }

        let freq = self.freq * pitch_mult;

        // Mix oscillators
        let mut mix = self.oscs[0].tick(freq) * self.osc_levels[0];
        if self.num_oscs >= 2 {
            mix += self.oscs[1].tick(freq) * self.osc_levels[1];
        }
        if self.num_oscs >= 3 {
            mix += self.oscs[2].tick(freq) * self.osc_levels[2];
        }

        // Mix noise pre-filter (additive, for breathiness/texture)
        if self.noise_level > 0.001 && self.oscs[0].osc_type != OscType::Noise {
            mix += self.next_noise() * self.noise_level;
        }

        // Filter routing
        let filtered = if self.use_formant {
            self.formant_filter.tick(mix)
        } else {
            let cutoff1 =
                (self.filter_base_cutoff + self.filter_env_amount * filter_mod) * self.filter_key_mult;
            self.filter.set_cutoff(cutoff1);

            match self.filter_routing {
                FilterRouting::Single => self.filter.tick(mix),
                FilterRouting::Serial => {
                    let f1 = self.filter.tick(mix);
                    let cutoff2 = (self.filter2_base_cutoff + self.filter_env_amount * filter_mod)
                        * self.filter_key_mult;
                    self.filter2.set_cutoff(cutoff2);
                    self.filter2.tick(f1)
                }
                FilterRouting::Parallel => {
                    let f1 = self.filter.tick(mix);
                    let cutoff2 = (self.filter2_base_cutoff + self.filter_env_amount * filter_mod)
                        * self.filter_key_mult;
                    self.filter2.set_cutoff(cutoff2);
                    let f2 = self.filter2.tick(mix);
                    (f1 + f2) * 0.5
                }
            }
        };

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
    // Multi-osc
    pub osc_count: f32,
    pub osc1_level: f32,
    pub osc2_type: f32,
    pub osc2_detune: f32,
    pub osc2_level: f32,
    pub osc3_type: f32,
    pub osc3_detune: f32,
    pub osc3_level: f32,
    // Filter 1
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub filter_type: f32,
    pub filter_env_amount: f32,
    pub filter_key_track: f32,
    // Filter routing + filter 2
    pub filter_routing: f32,
    pub filter2_type: f32,
    pub filter2_cutoff: f32,
    pub filter2_resonance: f32,
    // Noise
    pub noise_level: f32,
    // Amp envelope
    pub amp_attack: f32,
    pub amp_decay: f32,
    pub amp_sustain: f32,
    pub amp_release: f32,
    // Filter envelope
    pub filter_attack: f32,
    pub filter_decay: f32,
    pub filter_sustain: f32,
    pub filter_release: f32,
    // Physical model params
    pub ks_brightness: f32,
    pub ks_feedback: f32,
    pub organ_drawbars: [f32; 9],
    // Formant
    pub formant_voice: f32,
    pub formant_vowel: f32,
}

fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}
