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
            _ => Self::Sine,
        }
    }
}

/// Hammond organ drawbar harmonic ratios (16', 5⅓', 8', 4', 2⅔', 2', 1⅗', 1⅓', 1').
const ORGAN_HARMONICS: [f32; 9] = [0.5, 1.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];

#[derive(Clone)]
pub struct Oscillator {
    pub osc_type: OscType,
    phase: f32,
    mod_phase: f32,
    pub sample_rate: f32,
    detune: f32,
    pub fm_ratio: f32,
    pub fm_index: f32,
    // Noise PRNG (xorshift32)
    noise_state: u32,
    // Karplus-Strong (also used by CommutedPiano for tuning allpass)
    ks_buffer: Vec<f32>,
    ks_pos: usize,
    ks_filter_state: f32,
    ks_allpass_prev_in: f32,
    ks_allpass_prev_out: f32,
    ks_allpass_coeff: f32,
    pub ks_brightness: f32,
    pub ks_feedback: f32,
    // Organ drawbars (0.0–8.0 each)
    organ_phases: [f32; 9],
    pub organ_drawbars: [f32; 9],
    // Multi-op FM Piano (4 operators)
    fm4_phases: [f32; 4],
    fm4_time: f32,
    // Commuted Piano (enhanced KS with dispersion)
    cp_buffer: Vec<f32>,
    cp_pos: usize,
    cp_filter_state: f32,
    cp_allpass_coeffs: [f32; 6],
    cp_allpass_x: [f32; 6],
    cp_allpass_y: [f32; 6],
    // Banded Waveguide (4 parallel delay lines)
    bw_buffers: [Vec<f32>; 4],
    bw_positions: [usize; 4],
    bw_filter_states: [f32; 4],
    bw_gains: [f32; 4],
}

impl Oscillator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            osc_type: OscType::Sine,
            phase: 0.0,
            mod_phase: 0.0,
            sample_rate,
            detune: 0.0,
            fm_ratio: 3.5,
            fm_index: 5.0,
            noise_state: 0x12345678,
            ks_buffer: Vec::new(),
            ks_pos: 0,
            ks_filter_state: 0.0,
            ks_allpass_prev_in: 0.0,
            ks_allpass_prev_out: 0.0,
            ks_allpass_coeff: 0.0,
            ks_brightness: 0.5,
            ks_feedback: 0.996,
            organ_phases: [0.0; 9],
            organ_drawbars: [0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            fm4_phases: [0.0; 4],
            fm4_time: 0.0,
            cp_buffer: Vec::new(),
            cp_pos: 0,
            cp_filter_state: 0.0,
            cp_allpass_coeffs: [0.0; 6],
            cp_allpass_x: [0.0; 6],
            cp_allpass_y: [0.0; 6],
            bw_buffers: Default::default(),
            bw_positions: [0; 4],
            bw_filter_states: [0.0; 4],
            bw_gains: [1.0, 0.5, 0.25, 0.12],
        }
    }

    pub fn set_detune(&mut self, detune: f32) {
        self.detune = detune;
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.mod_phase = 0.0;
        self.organ_phases = [0.0; 9];
        self.fm4_phases = [0.0; 4];
        self.fm4_time = 0.0;
    }

    /// Initialize Karplus-Strong delay line for a given frequency.
    pub fn init_ks(&mut self, freq: f32) {
        let delay_total = self.sample_rate / freq;
        let n = (delay_total as usize).max(2);
        let frac = delay_total - n as f32;

        self.ks_allpass_coeff = (1.0 - frac) / (1.0 + frac);
        self.ks_allpass_prev_in = 0.0;
        self.ks_allpass_prev_out = 0.0;

        self.ks_buffer.resize(n, 0.0);
        let mut state = self.noise_state;
        for s in self.ks_buffer.iter_mut() {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *s = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
        }
        self.noise_state = state;
        self.ks_pos = 0;
        self.ks_filter_state = 0.0;
    }

    /// Initialize Commuted Piano: shaped hammer excitation + dispersion allpass chain.
    pub fn init_commuted_piano(&mut self, freq: f32) {
        let delay_total = self.sample_rate / freq;
        let n = (delay_total as usize).max(2);
        let frac = delay_total - n as f32;

        // Fractional tuning allpass (reuse KS fields)
        self.ks_allpass_coeff = (1.0 - frac) / (1.0 + frac);
        self.ks_allpass_prev_in = 0.0;
        self.ks_allpass_prev_out = 0.0;

        self.cp_buffer.resize(n, 0.0);

        // Shaped excitation: half-sine windowed noise (hammer model)
        // Hammer contact time depends on pitch: shorter for higher notes
        let hammer_ms = 1.5 + 3000.0 / freq;
        let excite_len = ((hammer_ms * 0.001 * self.sample_rate) as usize).clamp(4, n);

        let mut state = self.noise_state;
        for i in 0..n {
            if i < excite_len {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let noise = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                let window = (PI * i as f32 / excite_len as f32).sin();
                self.cp_buffer[i] = noise * window;
            } else {
                self.cp_buffer[i] = 0.0;
            }
        }
        self.noise_state = state;

        // Dispersion allpass coefficients (model string stiffness/inharmonicity)
        // Higher notes have more inharmonicity
        let b = (0.00004 * freq).clamp(0.0, 0.6);
        for i in 0..6 {
            self.cp_allpass_coeffs[i] = b * (1.0 - 0.12 * i as f32);
        }
        self.cp_allpass_x = [0.0; 6];
        self.cp_allpass_y = [0.0; 6];

        self.cp_pos = 0;
        self.cp_filter_state = 0.0;
    }

    /// Initialize Banded Waveguide: 4 parallel delay lines at inharmonic partials.
    pub fn init_banded_wg(&mut self, freq: f32) {
        let inharmonicity = 0.0003; // piano string stiffness coefficient

        for i in 0..4 {
            let partial = (i + 1) as f32;
            // fn = n * f0 * sqrt(1 + B * n²)  — piano inharmonicity formula
            let partial_freq = freq * partial * (1.0 + inharmonicity * partial * partial).sqrt();
            let delay = (self.sample_rate / partial_freq) as usize;
            let delay = delay.max(2);

            self.bw_buffers[i].resize(delay, 0.0);

            // Shaped excitation per band (amplitude decreases for higher partials)
            let excite_len = ((0.002 * self.sample_rate) as usize).clamp(2, delay);
            let amp_scale = 1.0 / partial;
            let mut state = self.noise_state;
            for j in 0..delay {
                if j < excite_len {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    let noise = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                    let w = (PI * j as f32 / excite_len as f32).sin();
                    self.bw_buffers[i][j] = noise * w * amp_scale;
                } else {
                    self.bw_buffers[i][j] = 0.0;
                }
            }
            self.noise_state = state;

            self.bw_positions[i] = 0;
            self.bw_filter_states[i] = 0.0;
        }

        self.bw_gains = [1.0, 0.5, 0.25, 0.12];
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
            _ => self.tick_standard(freq),
        }
    }

    fn tick_standard(&mut self, freq: f32) -> f32 {
        let freq = freq * (1.0 + self.detune);
        let dt = freq / self.sample_rate;

        let out = match self.osc_type {
            OscType::Sine => (self.phase * TAU).sin(),
            OscType::Saw => 2.0 * self.phase - 1.0,
            OscType::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            OscType::Triangle => {
                if self.phase < 0.5 {
                    4.0 * self.phase - 1.0
                } else {
                    3.0 - 4.0 * self.phase
                }
            }
            OscType::Fm => {
                let modulator = (self.mod_phase * TAU).sin();
                let out = ((self.phase + self.fm_index * modulator) * TAU).sin();
                self.mod_phase += freq * self.fm_ratio / self.sample_rate;
                self.mod_phase -= self.mod_phase.floor();
                out
            }
            _ => 0.0,
        };

        self.phase += dt;
        self.phase -= self.phase.floor();

        out
    }

    fn tick_ks(&mut self) -> f32 {
        if self.ks_buffer.is_empty() {
            return 0.0;
        }

        let sample = self.ks_buffer[self.ks_pos];

        let filtered =
            self.ks_filter_state + self.ks_brightness * (sample - self.ks_filter_state);
        self.ks_filter_state = filtered;

        let c = self.ks_allpass_coeff;
        let allpass_out = c * (filtered - self.ks_allpass_prev_out) + self.ks_allpass_prev_in;
        self.ks_allpass_prev_in = filtered;
        self.ks_allpass_prev_out = allpass_out;

        self.ks_buffer[self.ks_pos] = allpass_out * self.ks_feedback;
        self.ks_pos = (self.ks_pos + 1) % self.ks_buffer.len();

        sample
    }

    fn tick_organ(&mut self, freq: f32) -> f32 {
        let mut out = 0.0;
        let normalize = 1.0 / 8.0;

        for i in 0..9 {
            let level = self.organ_drawbars[i];
            if level < 0.01 {
                continue;
            }

            let harmonic_freq = freq * ORGAN_HARMONICS[i];
            let dt = harmonic_freq / self.sample_rate;
            out += (self.organ_phases[i] * TAU).sin() * level * normalize;
            self.organ_phases[i] += dt;
            if self.organ_phases[i] >= 1.0 {
                self.organ_phases[i] -= 1.0;
            }
        }

        out
    }

    /// 4-operator FM Piano (DX7-style).
    /// Pair 1: bright attack (fast-decaying high index)
    /// Pair 2: sustained body (slow-decaying low index)
    fn tick_fm_piano(&mut self, freq: f32) -> f32 {
        let dt = 1.0 / self.sample_rate;
        self.fm4_time += dt;
        let t = self.fm4_time;

        let f = freq * (1.0 + self.detune);

        // Pair 1: Attack brightness — high index, fast exponential decay
        let attack_idx = self.fm_index * fast_exp(-t * 10.0);
        let mod1 = (self.fm4_phases[1] * TAU).sin();
        let car1 = ((self.fm4_phases[0] + attack_idx * mod1) * TAU).sin();

        // Pair 2: Body — lower index, much slower decay + residual warmth
        let body_idx = self.fm_index * 0.18 * fast_exp(-t * 0.6) + 0.25;
        let mod2 = (self.fm4_phases[3] * TAU).sin();
        let car2 = ((self.fm4_phases[2] + body_idx * mod2) * TAU).sin();

        // Advance phases
        self.fm4_phases[0] += f / self.sample_rate; // carrier 1
        self.fm4_phases[1] += f * self.fm_ratio / self.sample_rate; // mod 1
        self.fm4_phases[2] += f / self.sample_rate; // carrier 2
        self.fm4_phases[3] += f * 3.0 / self.sample_rate; // mod 2 (3rd harmonic)

        for p in &mut self.fm4_phases {
            *p -= p.floor();
        }

        // Mix: attack dominates early, body sustains
        let attack_amp = fast_exp(-t * 6.0);
        car1 * attack_amp * 0.6 + car2 * 0.4
    }

    /// Commuted Piano: KS with shaped hammer excitation + allpass dispersion chain.
    fn tick_commuted_piano(&mut self) -> f32 {
        let n = self.cp_buffer.len();
        if n == 0 {
            return 0.0;
        }

        let sample = self.cp_buffer[self.cp_pos];

        // One-pole lowpass damping
        let filtered =
            self.cp_filter_state + self.ks_brightness * (sample - self.cp_filter_state);
        self.cp_filter_state = filtered;

        // 6-stage allpass dispersion chain (creates piano-like inharmonicity)
        let mut sig = filtered;
        for i in 0..6 {
            let c = self.cp_allpass_coeffs[i];
            let y = c * sig + self.cp_allpass_x[i] - c * self.cp_allpass_y[i];
            self.cp_allpass_x[i] = sig;
            self.cp_allpass_y[i] = y;
            sig = y;
        }

        // Fractional delay tuning correction
        let c = self.ks_allpass_coeff;
        let ap = c * (sig - self.ks_allpass_prev_out) + self.ks_allpass_prev_in;
        self.ks_allpass_prev_in = sig;
        self.ks_allpass_prev_out = ap;

        // Write back with feedback
        self.cp_buffer[self.cp_pos] = ap * self.ks_feedback;
        self.cp_pos = (self.cp_pos + 1) % n;

        sample
    }

    /// Banded Waveguide: 4 parallel delay lines at inharmonic partial frequencies.
    fn tick_banded_wg(&mut self) -> f32 {
        let mut out = 0.0;

        for i in 0..4 {
            let n = self.bw_buffers[i].len();
            if n == 0 {
                continue;
            }

            let sample = self.bw_buffers[i][self.bw_positions[i]];

            // Damping increases for higher partials (natural behavior)
            let brightness = self.ks_brightness * (1.0 - 0.12 * i as f32);
            let filtered = self.bw_filter_states[i]
                + brightness * (sample - self.bw_filter_states[i]);
            self.bw_filter_states[i] = filtered;

            // Feedback decreases for higher partials
            let fb = self.ks_feedback - 0.0015 * i as f32;
            self.bw_buffers[i][self.bw_positions[i]] = filtered * fb;
            self.bw_positions[i] = (self.bw_positions[i] + 1) % n;

            out += sample * self.bw_gains[i];
        }

        out
    }
}

/// Fast exponential approximation for negative arguments.
/// Uses the identity exp(x) ≈ (1 + x/256)^256 via repeated squaring.
#[inline]
fn fast_exp(x: f32) -> f32 {
    // Clamp to avoid underflow
    let x = x.max(-20.0);
    let mut y = 1.0 + x / 256.0;
    y *= y; y *= y; y *= y; y *= y; // 2^4 = 16
    y *= y; y *= y; y *= y; y *= y; // 2^8 = 256
    y.max(0.0)
}
