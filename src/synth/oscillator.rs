/// Oscillator with multiple waveform types.

use std::f32::consts::PI;

const TAU: f32 = 2.0 * PI;

#[derive(Clone, Copy, PartialEq)]
pub enum OscType {
    Sine,          // 0
    Saw,           // 1
    Square,        // 2
    Triangle,      // 3
    Fm,            // 4
    Noise,         // 5
    KarplusStrong, // 6
    Organ,         // 7
    FmPiano,       // 8  — 4-operator FM piano (DX7-style)
    CommutedPiano, // 9  — commuted waveguide piano
    BandedWG,      // 10 — banded waveguide piano
    AdditivePiano, // 11 — additive synthesis piano (16 partials with per-partial decay)
}

impl OscType {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            0 => Self::Sine,
            1 => Self::Saw,
            2 => Self::Square,
            3 => Self::Triangle,
            4 => Self::Fm,
            5 => Self::Noise,
            6 => Self::KarplusStrong,
            7 => Self::Organ,
            8 => Self::FmPiano,
            9 => Self::CommutedPiano,
            10 => Self::BandedWG,
            11 => Self::AdditivePiano,
            _ => Self::Sine,
        }
    }

    /// Returns true for simple waveform types that can be used as Osc 2/3.
    pub fn is_simple(self) -> bool {
        matches!(self, Self::Sine | Self::Saw | Self::Square | Self::Triangle | Self::Fm)
    }
}

/// Hammond organ drawbar harmonic ratios (16', 5⅓', 8', 4', 2⅔', 2', 1⅗', 1⅓', 1').
const ORGAN_HARMONICS: [f32; 9] = [0.5, 1.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];

/// Per-oscillator-type runtime state. Only the active variant is allocated.
#[derive(Clone)]
pub enum OscState {
    Simple {
        phase: f32,
    },
    Fm {
        phase: f32,
        mod_phase: f32,
    },
    Noise,
    KarplusStrong {
        buffer: Vec<f32>,
        pos: usize,
        filter_state: f32,
        allpass_prev_in: f32,
        allpass_prev_out: f32,
        allpass_coeff: f32,
    },
    Organ {
        phases: [f32; 9],
    },
    FmPiano {
        phases: [f32; 4],
        time: f32,
    },
    CommutedPiano {
        buffer: Vec<f32>,
        buffer2: Vec<f32>,
        pos: usize,
        pos2: usize,
        filter_state: f32,
        filter_state2: f32,
        allpass_coeff: f32,
        allpass_prev_in: f32,
        allpass_prev_out: f32,
        strike_offset: usize,
    },
    BandedWG {
        buffers: [Vec<f32>; 4],
        positions: [usize; 4],
        filter_states: [f32; 4],
        gains: [f32; 4],
    },
    AdditivePiano {
        phases: [f32; 16],
        freqs: [f32; 16],
        init_amps: [f32; 16],
        decay_rates: [f32; 16],
        time: f32,
        noise_amp: f32,
    },
}

#[derive(Clone)]
pub struct Oscillator {
    pub osc_type: OscType,
    pub sample_rate: f32,
    detune: f32,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub ks_brightness: f32,
    pub ks_feedback: f32,
    pub organ_drawbars: [f32; 9],
    noise_state: u32,
    state: OscState,
}

impl Oscillator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            osc_type: OscType::Sine,
            sample_rate,
            detune: 0.0,
            fm_ratio: 3.5,
            fm_index: 5.0,
            ks_brightness: 0.5,
            ks_feedback: 0.996,
            organ_drawbars: [0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            noise_state: 0x12345678,
            state: OscState::Simple { phase: 0.0 },
        }
    }

    pub fn set_detune(&mut self, detune: f32) {
        self.detune = detune;
    }

    pub fn reset(&mut self) {
        self.state = match self.osc_type {
            OscType::Sine | OscType::Saw | OscType::Square | OscType::Triangle => {
                OscState::Simple { phase: 0.0 }
            }
            OscType::Fm => OscState::Fm {
                phase: 0.0,
                mod_phase: 0.0,
            },
            OscType::Noise => OscState::Noise,
            OscType::KarplusStrong => OscState::KarplusStrong {
                buffer: Vec::new(),
                pos: 0,
                filter_state: 0.0,
                allpass_prev_in: 0.0,
                allpass_prev_out: 0.0,
                allpass_coeff: 0.0,
            },
            OscType::Organ => OscState::Organ {
                phases: [0.0; 9],
            },
            OscType::FmPiano => OscState::FmPiano {
                phases: [0.0; 4],
                time: 0.0,
            },
            OscType::CommutedPiano => OscState::CommutedPiano {
                buffer: Vec::new(),
                buffer2: Vec::new(),
                pos: 0,
                pos2: 0,
                filter_state: 0.0,
                filter_state2: 0.0,
                allpass_coeff: 0.0,
                allpass_prev_in: 0.0,
                allpass_prev_out: 0.0,
                strike_offset: 0,
            },
            OscType::BandedWG => OscState::BandedWG {
                buffers: Default::default(),
                positions: [0; 4],
                filter_states: [0.0; 4],
                gains: [0.53, 0.27, 0.13, 0.07],
            },
            OscType::AdditivePiano => OscState::AdditivePiano {
                phases: [0.0; 16],
                freqs: [0.0; 16],
                init_amps: [0.0; 16],
                decay_rates: [0.0; 16],
                time: 0.0,
                noise_amp: 0.0,
            },
        };
    }

    /// Initialize Karplus-Strong delay line for a given frequency.
    pub fn init_ks(&mut self, freq: f32) {
        let delay_total = self.sample_rate / freq;
        let n = (delay_total as usize).max(2);
        let frac = delay_total - n as f32;
        let allpass_coeff = (1.0 - frac) / (1.0 + frac);

        let mut buffer = vec![0.0; n];
        let mut state = self.noise_state;
        for s in buffer.iter_mut() {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *s = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
        }
        self.noise_state = state;

        self.state = OscState::KarplusStrong {
            buffer,
            pos: 0,
            filter_state: 0.0,
            allpass_prev_in: 0.0,
            allpass_prev_out: 0.0,
            allpass_coeff,
        };
    }

    /// Initialize Commuted Piano: dual detuned waveguides with hammer model and dispersion.
    pub fn init_commuted_piano(&mut self, freq: f32) {
        // String 1: exact pitch
        let delay1 = self.sample_rate / freq;
        let n1 = (delay1 as usize).max(2);
        let frac1 = delay1 - n1 as f32;
        let allpass_coeff = (1.0 - frac1) / (1.0 + frac1);

        // String 2: slightly detuned (~1 cent sharp) — creates beating/double decay
        let detune_cents = 0.8 + 0.5 * (freq / 1000.0);
        let freq2 = freq * 2.0_f32.powf(detune_cents / 1200.0);
        let delay2 = self.sample_rate / freq2;
        let n2 = (delay2 as usize).max(2);

        // Hammer-position comb filter offset (strike at ~1/8 string length)
        let strike_offset = (n1 / 8).max(1);

        // Hammer excitation: windowed noise burst (bipolar, zero-mean)
        let hammer_samples = ((0.001 + 3.0 / freq) * self.sample_rate) as usize;
        let max_excite = (n1.min(n2) * 3) / 10;
        let excite_len = hammer_samples.clamp(4, max_excite.max(4));

        let mut buffer = vec![0.0; n1];
        let mut buffer2 = vec![0.0; n2];

        let mut state = self.noise_state;
        for i in 0..excite_len {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let w = 0.5 * (1.0 - (TAU * i as f32 / excite_len as f32).cos());
            let sample = noise * w * 0.8;
            if i < n1 {
                buffer[i] = sample;
            }
            if i < n2 {
                buffer2[i] = sample;
            }
        }
        self.noise_state = state;

        self.state = OscState::CommutedPiano {
            buffer,
            buffer2,
            pos: 0,
            pos2: 0,
            filter_state: 0.0,
            filter_state2: 0.0,
            allpass_coeff,
            allpass_prev_in: 0.0,
            allpass_prev_out: 0.0,
            strike_offset,
        };
    }

    /// Initialize Banded Waveguide: 4 parallel delay lines at inharmonic partials.
    pub fn init_banded_wg(&mut self, freq: f32) {
        let inharmonicity = 0.0003;
        let mut buffers: [Vec<f32>; 4] = Default::default();
        let mut positions = [0usize; 4];
        let mut filter_states = [0.0f32; 4];

        let mut state = self.noise_state;
        for i in 0..4 {
            let partial = (i + 1) as f32;
            let partial_freq = freq * partial * (1.0 + inharmonicity * partial * partial).sqrt();
            let delay = (self.sample_rate / partial_freq) as usize;
            let delay = delay.max(2);

            buffers[i].resize(delay, 0.0);

            let excite_len = ((0.002 * self.sample_rate) as usize).clamp(2, delay);
            let amp_scale = 1.0 / partial;
            for j in 0..delay {
                if j < excite_len {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    let noise = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                    let w = (PI * j as f32 / excite_len as f32).sin();
                    buffers[i][j] = noise * w * amp_scale;
                } else {
                    buffers[i][j] = 0.0;
                }
            }

            positions[i] = 0;
            filter_states[i] = 0.0;
        }
        self.noise_state = state;

        self.state = OscState::BandedWG {
            buffers,
            positions,
            filter_states,
            gains: [0.53, 0.27, 0.13, 0.07],
        };
    }

    /// Initialize Additive Piano: 16 partials with per-partial decay and inharmonicity.
    pub fn init_additive_piano(&mut self, freq: f32) {
        let midi_note = (12.0 * (freq / 440.0).log2() + 69.0).clamp(21.0, 108.0);
        let normalized = (midi_note - 21.0) / 87.0;

        let brightness = self.ks_brightness;

        let inharm_base = 0.0001 * (3.5 * normalized).exp();
        let b = inharm_base * (1.0 + 3.0 * (1.0 - brightness));

        let sustain_factor = 1.0 / (1.0 - self.ks_feedback).max(0.001);
        let decay_mult = 100.0 / sustain_factor;
        let base_decay = (0.15 + 0.4 * normalized) * decay_mult;

        const GRAND_AMPS: [f32; 16] = [
            1.00, 0.85, 0.70, 0.55, 0.40, 0.30, 0.22, 0.05, 0.14, 0.11, 0.08, 0.06, 0.05, 0.04,
            0.03, 0.02,
        ];
        const UPRIGHT_AMPS: [f32; 16] = [
            1.00, 0.65, 0.40, 0.28, 0.15, 0.10, 0.06, 0.02, 0.04, 0.03, 0.02, 0.01, 0.01,
            0.005, 0.003, 0.002,
        ];

        let mut phases = [0.0f32; 16];
        let mut freqs = [0.0f32; 16];
        let mut init_amps = [0.0f32; 16];
        let mut decay_rates = [0.0f32; 16];

        for n in 0..16 {
            let partial = (n + 1) as f32;
            freqs[n] = freq * partial * (1.0 + b * partial * partial).sqrt();

            let t = brightness.clamp(0.0, 1.0);
            init_amps[n] = UPRIGHT_AMPS[n] * (1.0 - t) + GRAND_AMPS[n] * t;

            let partial_freq = freqs[n];
            let freq_decay = 2.0 * (partial_freq / 4000.0).powi(2);
            let tilt = 1.0 + 2.0 * (1.0 - brightness);
            decay_rates[n] = base_decay + freq_decay * tilt;

            phases[n] = 0.0;
        }

        let noise_amp = 0.04 + brightness * 0.18;

        self.state = OscState::AdditivePiano {
            phases,
            freqs,
            init_amps,
            decay_rates,
            time: 0.0,
            noise_amp,
        };
    }

    /// Generate next noise sample using xorshift32.
    #[inline]
    pub fn next_noise(&mut self) -> f32 {
        self.noise_state ^= self.noise_state << 13;
        self.noise_state ^= self.noise_state >> 17;
        self.noise_state ^= self.noise_state << 5;
        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    pub fn tick(&mut self, freq: f32) -> f32 {
        match self.osc_type {
            OscType::KarplusStrong => self.tick_ks(),
            OscType::Organ => self.tick_organ(freq),
            OscType::Noise => self.next_noise(),
            OscType::FmPiano => self.tick_fm_piano(freq),
            OscType::CommutedPiano => self.tick_commuted_piano(),
            OscType::BandedWG => self.tick_banded_wg(),
            OscType::AdditivePiano => self.tick_additive_piano(),
            _ => self.tick_standard(freq),
        }
    }

    fn tick_standard(&mut self, freq: f32) -> f32 {
        let freq = freq * (1.0 + self.detune);
        let dt = freq / self.sample_rate;

        match &mut self.state {
            OscState::Simple { phase } => {
                let out = match self.osc_type {
                    OscType::Sine => (*phase * TAU).sin(),
                    OscType::Saw => 2.0 * *phase - 1.0,
                    OscType::Square => {
                        if *phase < 0.5 {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                    OscType::Triangle => {
                        if *phase < 0.5 {
                            4.0 * *phase - 1.0
                        } else {
                            3.0 - 4.0 * *phase
                        }
                    }
                    _ => 0.0,
                };
                *phase += dt;
                *phase -= phase.floor();
                out
            }
            OscState::Fm { phase, mod_phase } => {
                let fm_index = self.fm_index;
                let fm_ratio = self.fm_ratio;
                let sample_rate = self.sample_rate;
                let modulator = (*mod_phase * TAU).sin();
                let out = ((*phase + fm_index * modulator) * TAU).sin();
                *mod_phase += freq * fm_ratio / sample_rate;
                *mod_phase -= mod_phase.floor();
                *phase += dt;
                *phase -= phase.floor();
                out
            }
            _ => 0.0,
        }
    }

    fn tick_ks(&mut self) -> f32 {
        let brightness = self.ks_brightness;
        let feedback = self.ks_feedback;

        if let OscState::KarplusStrong {
            buffer,
            pos,
            filter_state,
            allpass_prev_in,
            allpass_prev_out,
            allpass_coeff,
        } = &mut self.state
        {
            if buffer.is_empty() {
                return 0.0;
            }

            let sample = buffer[*pos];

            let filtered = *filter_state + brightness * (sample - *filter_state);
            *filter_state = filtered;

            let c = *allpass_coeff;
            let ap_out = c * (filtered - *allpass_prev_out) + *allpass_prev_in;
            *allpass_prev_in = filtered;
            *allpass_prev_out = ap_out;

            buffer[*pos] = ap_out * feedback;
            *pos = (*pos + 1) % buffer.len();

            sample
        } else {
            0.0
        }
    }

    fn tick_organ(&mut self, freq: f32) -> f32 {
        let drawbars = self.organ_drawbars;
        let sample_rate = self.sample_rate;

        if let OscState::Organ { phases } = &mut self.state {
            let mut out = 0.0;
            let normalize = 1.0 / 8.0;

            for i in 0..9 {
                let level = drawbars[i];
                if level < 0.01 {
                    continue;
                }

                let harmonic_freq = freq * ORGAN_HARMONICS[i];
                let dt = harmonic_freq / sample_rate;
                out += (phases[i] * TAU).sin() * level * normalize;
                phases[i] += dt;
                if phases[i] >= 1.0 {
                    phases[i] -= 1.0;
                }
            }

            out
        } else {
            0.0
        }
    }

    /// 4-operator FM Piano (DX7 Algorithm 5 — E.PIANO 1 style).
    fn tick_fm_piano(&mut self, freq: f32) -> f32 {
        let sample_rate = self.sample_rate;
        let detune = self.detune;
        let fm_index = self.fm_index;
        let fm_ratio = self.fm_ratio;

        if let OscState::FmPiano { phases, time } = &mut self.state {
            let dt = 1.0 / sample_rate;
            *time += dt;
            let t = *time;

            let f = freq * (1.0 + detune);

            // Stack 1: Attack brightness — 1:1 ratio, high index decays fast
            let attack_idx = fm_index * fast_exp(-t * 12.0);
            let mod1 = (phases[1] * TAU).sin();
            let car1 = ((phases[0] + attack_idx * mod1) * TAU).sin();

            // Stack 2: Body — 14:1 ratio, low index for bell-like shimmer
            let body_idx = 0.7 * fast_exp(-t * 0.5) + 0.05;
            let mod2 = (phases[3] * TAU).sin();
            let car2 = ((phases[2] + body_idx * mod2) * TAU).sin();

            // Advance phases
            phases[0] += f / sample_rate; // carrier 1: 1x
            phases[1] += f * fm_ratio / sample_rate; // mod 1: preset ratio (1.0)
            phases[2] += f / sample_rate; // carrier 2: 1x
            phases[3] += f * 14.0 / sample_rate; // mod 2: 14x (DX7 shimmer)

            for p in phases.iter_mut() {
                *p -= p.floor();
            }

            // Mix: percussive attack + sustained body
            let attack_amp = fast_exp(-t * 10.0);
            let out = car1 * attack_amp * 0.35 + car2 * 0.65;
            out.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    /// Commuted Piano: dual detuned waveguides.
    fn tick_commuted_piano(&mut self) -> f32 {
        let brightness = self.ks_brightness;
        let feedback = self.ks_feedback;

        if let OscState::CommutedPiano {
            buffer,
            buffer2,
            pos,
            pos2,
            filter_state,
            filter_state2,
            allpass_coeff,
            allpass_prev_in,
            allpass_prev_out,
            ..
        } = &mut self.state
        {
            let n1 = buffer.len();
            let n2 = buffer2.len();
            if n1 == 0 {
                return 0.0;
            }

            // --- String 1 ---
            let s1 = buffer[*pos];

            let f1 = *filter_state + brightness * (s1 - *filter_state);
            *filter_state = f1;

            let c = *allpass_coeff;
            let ap1 = c * (f1 - *allpass_prev_out) + *allpass_prev_in;
            *allpass_prev_in = f1;
            *allpass_prev_out = ap1;

            buffer[*pos] = ap1 * feedback;
            *pos = (*pos + 1) % n1;

            // --- String 2 (detuned for beating/double-decay) ---
            let mut s2 = 0.0;
            if n2 > 0 {
                s2 = buffer2[*pos2];

                let f2 = *filter_state2 + brightness * (s2 - *filter_state2);
                *filter_state2 = f2;

                buffer2[*pos2] = f2 * feedback;
                *pos2 = (*pos2 + 1) % n2;
            }

            ((s1 + s2) * 0.5).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    /// Banded Waveguide: 4 parallel delay lines at inharmonic partial frequencies.
    fn tick_banded_wg(&mut self) -> f32 {
        let brightness = self.ks_brightness;
        let feedback = self.ks_feedback;

        if let OscState::BandedWG {
            buffers,
            positions,
            filter_states,
            gains,
        } = &mut self.state
        {
            let mut out = 0.0;

            for i in 0..4 {
                let n = buffers[i].len();
                if n == 0 {
                    continue;
                }

                let sample = buffers[i][positions[i]];

                let br = brightness * (1.0 - 0.12 * i as f32);
                let filtered = filter_states[i] + br * (sample - filter_states[i]);
                filter_states[i] = filtered;

                let fb = feedback - 0.0015 * i as f32;
                buffers[i][positions[i]] = filtered * fb;
                positions[i] = (positions[i] + 1) % n;

                out += sample * gains[i];
            }

            out.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    /// Additive Piano: 16 partials with independent exponential decay.
    fn tick_additive_piano(&mut self) -> f32 {
        let sample_rate = self.sample_rate;
        let mut noise_state = self.noise_state;

        let result = if let OscState::AdditivePiano {
            phases,
            freqs,
            init_amps,
            decay_rates,
            time,
            noise_amp,
        } = &mut self.state
        {
            let dt = 1.0 / sample_rate;
            *time += dt;
            let t = *time;

            let mut out = 0.0;

            for n in 0..16 {
                let amp = init_amps[n];
                if amp < 0.001 {
                    continue;
                }

                let r = decay_rates[n];
                let env = 0.65 * fast_exp(-t * r * 3.0) + 0.35 * fast_exp(-t * r * 0.4);

                out += (phases[n] * TAU).sin() * amp * env;

                phases[n] += freqs[n] / sample_rate;
                phases[n] -= phases[n].floor();
            }

            // Hammer attack noise burst (~3ms)
            let noise_env = *noise_amp * fast_exp(-t * 300.0);
            if noise_env > 0.001 {
                noise_state ^= noise_state << 13;
                noise_state ^= noise_state >> 17;
                noise_state ^= noise_state << 5;
                let noise = (noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                out += noise * noise_env;
            }

            out *= 0.4;
            out.clamp(-1.0, 1.0)
        } else {
            0.0
        };

        self.noise_state = noise_state;
        result
    }
}

/// Fast exponential approximation for negative arguments.
/// Uses the identity exp(x) ≈ (1 + x/256)^256 via repeated squaring.
#[inline]
fn fast_exp(x: f32) -> f32 {
    let x = x.max(-20.0);
    let mut y = 1.0 + x / 256.0;
    y *= y;
    y *= y;
    y *= y;
    y *= y; // 2^4 = 16
    y *= y;
    y *= y;
    y *= y;
    y *= y; // 2^8 = 256
    y.max(0.0)
}
