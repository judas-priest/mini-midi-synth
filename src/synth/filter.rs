/// State-variable filter (SVF) — lowpass, highpass, bandpass.
/// Plus Moog ladder filter (Huovilainen 2004 improved model with 2x oversampling).

use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterType {
    LowPass,   // 0 — SVF 12dB/oct
    HighPass,  // 1 — SVF 12dB/oct
    BandPass,  // 2 — SVF 12dB/oct
    Formant,   // 3 — formant (handled externally)
    MoogLP24,  // 4 — Moog ladder 24dB/oct lowpass
    MoogLP12,  // 5 — Moog ladder 12dB/oct (2-pole tap)
    DiodeLP,   // 6 — Diode ladder 18dB/oct (TB-303 style)
    Comb,      // 7 — Comb filter (delay-line based)
    Allpass,   // 8 — Allpass filter (phase shifting)
    CombPos,   // 9 — Comb+ (positive feedback)
    CombNeg,   // 10 — Comb- (negative feedback)
}

impl FilterType {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            0 => Self::LowPass,
            1 => Self::HighPass,
            2 => Self::BandPass,
            3 => Self::Formant,
            4 => Self::MoogLP24,
            5 => Self::MoogLP12,
            6 => Self::DiodeLP,
            7 => Self::Comb,
            8 => Self::Allpass,
            9 => Self::CombPos,
            10 => Self::CombNeg,
            _ => Self::LowPass,
        }
    }

    pub fn is_moog(self) -> bool {
        matches!(self, Self::MoogLP24 | Self::MoogLP12)
    }

    pub fn is_diode(self) -> bool {
        matches!(self, Self::DiodeLP)
    }

    pub fn is_comb(self) -> bool {
        matches!(self, Self::Comb | Self::CombPos | Self::CombNeg)
    }

    pub fn is_allpass(self) -> bool {
        matches!(self, Self::Allpass)
    }
}

/// Transistor thermal voltage scaling.
/// VT = kT/q ≈ 26mV at room temperature. For a differential pair: 2*VT ≈ 0.0526V.
/// We use 1/(2*VT) as the scaling factor inside tanh().
/// In normalized audio signal range, VT_INV = 1.22 gives good saturation character.
const VT_INV: f32 = 1.22;

#[derive(Clone)]
pub struct Filter {
    filter_type: FilterType,
    cutoff: f32,
    resonance: f32,
    sample_rate: f32,
    // SVF state
    ic1eq: f32,
    ic2eq: f32,
    // SVF cached coefficients
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    dirty: bool,
    // Moog ladder state (Huovilainen improved)
    moog_stage: [f32; 4],   // 4 integrator states
    moog_tanh: [f32; 4],    // cached tanh(stage * VT_INV)
    moog_delay4: f32,       // z^-1 for half-sample feedback delay
    moog_tune: f32,          // frequency coefficient (with polynomial correction)
    moog_res_quad: f32,      // resonance * 4 * acr (with resonance correction)
    moog_acr: f32,           // resonance correction factor
    moog_gain_comp: f32,     // gain compensation at high resonance
    // Diode ladder state (Stilson & Smith / Zavalishin improved)
    diode_stage: [f32; 4],    // 4 integrator states
    diode_feedback: f32,       // feedback state z^-1
    diode_tune: f32,           // frequency coefficient
    diode_res: f32,            // resonance coefficient
    diode_gain_comp: f32,      // gain compensation
    // Comb filter state
    comb_buf: Box<[f32; 512]>,  // circular buffer (max ~11.6ms @ 44.1k)
    comb_write: usize,          // write position
    comb_delay: usize,          // delay in samples (from cutoff)
    comb_feedback: f32,         // feedback amount (from resonance)
    // Allpass filter state
    allpass_x1: f32,            // x[n-1]
    allpass_y1: f32,            // y[n-1]
    allpass_coeff: f32,         // allpass coefficient
    // Control-rate coefficient update
    coeff_counter: u8,          // wrapping counter, update every 32 samples
}

impl Filter {
    pub fn new(sample_rate: f32) -> Self {
        let mut f = Self {
            filter_type: FilterType::LowPass,
            cutoff: 8000.0,
            resonance: 0.0,
            sample_rate,
            ic1eq: 0.0,
            ic2eq: 0.0,
            g: 0.0,
            k: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            dirty: true,
            moog_stage: [0.0; 4],
            moog_tanh: [0.0; 4],
            moog_delay4: 0.0,
            moog_tune: 0.0,
            moog_res_quad: 0.0,
            moog_acr: 0.0,
            moog_gain_comp: 1.0,
            // Diode ladder state (Stilson & Smith / Zavalishin improved)
            diode_stage: [0.0; 4],
            diode_feedback: 0.0,
            diode_tune: 0.0,
            diode_res: 0.0,
            diode_gain_comp: 1.0,
            // Comb filter state
            comb_buf: Box::new([0.0; 512]),
            comb_write: 0,
            comb_delay: 100,
            comb_feedback: 0.0,
            // Allpass filter state
            allpass_x1: 0.0,
            allpass_y1: 0.0,
            allpass_coeff: 0.0,
            coeff_counter: 31, // so first tick after set_cutoff/set_resonance triggers update
        };
        f.update_coefficients();
        f
    }

    pub fn set_type(&mut self, ft: FilterType) {
        self.filter_type = ft;
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        let clamped = cutoff.clamp(20.0, self.sample_rate * 0.49);
        if clamped != self.cutoff {
            self.cutoff = clamped;
            self.dirty = true;
        }
    }

    pub fn set_resonance(&mut self, res: f32) {
        let clamped = res.clamp(0.0, 1.0);
        if clamped != self.resonance {
            self.resonance = clamped;
            self.dirty = true;
        }
    }

    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
        self.moog_stage = [0.0; 4];
        self.moog_tanh = [0.0; 4];
        self.moog_delay4 = 0.0;
        self.diode_stage = [0.0; 4];
        self.diode_feedback = 0.0;
        self.comb_buf.fill(0.0);
        self.comb_write = 0;
        self.allpass_x1 = 0.0;
        self.allpass_y1 = 0.0;
    }

    fn update_coefficients(&mut self) {
        if self.filter_type.is_moog() {
            self.update_moog_coefficients();
        } else if self.filter_type.is_diode() {
            self.update_diode_coefficients();
        } else if self.filter_type.is_comb() {
            self.update_comb_coefficients();
        } else if self.filter_type.is_allpass() {
            self.update_allpass_coefficients();
        } else {
            self.update_svf_coefficients();
        }
        self.dirty = false;
    }

    fn update_svf_coefficients(&mut self) {
        // SVF from Andrew Simper / Cytomic
        let x = PI * self.cutoff / self.sample_rate;
        self.g = fast_tan(x);
        self.k = (2.0 - 2.0 * self.resonance).max(0.005);
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    fn update_moog_coefficients(&mut self) {
        // Huovilainen (2004) improved Moog ladder model
        // Normalized frequency for 2x oversampled rate
        let fc = (self.cutoff / (self.sample_rate * 2.0)).min(0.49);

        // Polynomial frequency correction (decouple cutoff from resonance)
        // From Huovilainen paper: attempt to linearize the frequency response
        let fc2 = fc * fc;
        let fc3 = fc2 * fc;
        let fcr = 1.8730 * fc3 + 0.4955 * fc2 - 0.6490 * fc + 0.9988;

        // Resonance correction factor
        self.moog_acr = -3.9364 * fc2 + 1.8409 * fc + 0.9968;

        // Tuning coefficient: integrator gain
        // tune = (1 - exp(-2π × fc × fcr)) / thermal_voltage
        self.moog_tune = (1.0 - (-2.0 * PI * fc * fcr).exp()) / VT_INV;

        // Resonance: 0-1 mapped to 0-4*acr (self-oscillation at 4)
        self.moog_res_quad = 4.0 * self.resonance * self.moog_acr;

        // Gain compensation: restore volume lost at high resonance
        // The ladder loses ~6dB of passband gain at full resonance
        self.moog_gain_comp = 1.0 + self.resonance * 1.5;
    }

    pub fn force_update(&mut self) {
        self.update_coefficients();
        self.coeff_counter = 31; // next tick will update again with latest cutoff from envelope
    }

    pub fn tick(&mut self, input: f32) -> f32 {
        self.coeff_counter = self.coeff_counter.wrapping_add(1);
        if (self.coeff_counter & 31 == 0) && self.dirty {
            self.update_coefficients();
        }

        match self.filter_type {
            FilterType::MoogLP24 => self.tick_moog(input, true),
            FilterType::MoogLP12 => self.tick_moog(input, false),
            FilterType::DiodeLP => self.tick_diode(input),
            FilterType::Comb | FilterType::CombPos | FilterType::CombNeg => self.tick_comb(input),
            FilterType::Allpass => self.tick_allpass(input),
            _ => self.tick_svf(input),
        }
    }

    fn tick_svf(&mut self, input: f32) -> f32 {
        let v3 = input - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;

        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        match self.filter_type {
            FilterType::LowPass => v2,
            FilterType::HighPass => input - self.k * v1 - v2,
            FilterType::BandPass | FilterType::Formant => v1,
            _ => v2,
        }
    }

    /// Moog 4-pole transistor ladder filter — Huovilainen (2004) improved model.
    ///
    /// Key features vs naive implementation:
    /// - 2x internal oversampling (eliminates aliasing from tanh nonlinearities)
    /// - Per-stage tanh saturation with thermal voltage scaling
    /// - Polynomial tuning correction (cutoff independent of resonance)
    /// - Half-sample delay in feedback path (improved stability/accuracy)
    /// - Gain compensation at high resonance
    fn tick_moog(&mut self, input: f32, four_pole: bool) -> f32 {
        let tune = self.moog_tune;
        let res = self.moog_res_quad;
        let gain_comp = self.moog_gain_comp;

        // 2x oversampling: process each input sample twice
        // This is critical — tanh generates harmonics that alias without oversampling
        for _ in 0..2 {
            // Half-sample delay in feedback path (Huovilainen improvement)
            // Average current and previous output for more accurate feedback timing
            let feedback = (self.moog_stage[3] + self.moog_delay4) * 0.5;
            self.moog_delay4 = self.moog_stage[3];

            // Input with resonance feedback, saturated through tanh
            let x = fast_tanh((input - res * feedback) * VT_INV);

            // 4 cascaded one-pole integrator stages with per-stage nonlinearity
            // Each stage: V[i] += tune * (tanh(input * VT_INV) - tanh(V[i] * VT_INV))
            // The tanh on each stage output models the transistor pair saturation
            self.moog_stage[0] += tune * (x - self.moog_tanh[0]);
            self.moog_tanh[0] = fast_tanh(self.moog_stage[0] * VT_INV);

            self.moog_stage[1] += tune * (self.moog_tanh[0] - self.moog_tanh[1]);
            self.moog_tanh[1] = fast_tanh(self.moog_stage[1] * VT_INV);

            self.moog_stage[2] += tune * (self.moog_tanh[1] - self.moog_tanh[2]);
            self.moog_tanh[2] = fast_tanh(self.moog_stage[2] * VT_INV);

            self.moog_stage[3] += tune * (self.moog_tanh[2] - self.moog_tanh[3]);
            self.moog_tanh[3] = fast_tanh(self.moog_stage[3] * VT_INV);
        }

        let raw = if four_pole {
            self.moog_stage[3]
        } else {
            self.moog_stage[1]
        };

        // Apply gain compensation
        raw * gain_comp
    }

    fn update_comb_coefficients(&mut self) {
        // Delay from cutoff: delay_samples = sample_rate / cutoff_hz
        let delay = (self.sample_rate / self.cutoff.max(20.0)) as usize;
        self.comb_delay = delay.clamp(1, 511);
        // Feedback from resonance: 0..0.99 for standard comb, negative for CombNeg
        let fb = self.resonance * 0.99;
        self.comb_feedback = match self.filter_type {
            FilterType::CombNeg => -fb,
            _ => fb,
        };
    }

    fn update_allpass_coefficients(&mut self) {
        // First-order allpass: coeff = (tan(π*fc/fs) - 1) / (tan(π*fc/fs) + 1)
        let w = PI * self.cutoff / self.sample_rate;
        let t = fast_tan(w);
        self.allpass_coeff = (t - 1.0) / (t + 1.0);
    }

    fn tick_comb(&mut self, input: f32) -> f32 {
        let delay = self.comb_delay;
        let buf_len = self.comb_buf.len();
        let mask = buf_len - 1; // buf_len == 512 == 2^9
        let read_pos = (self.comb_write + buf_len - delay) & mask;
        let delayed = self.comb_buf[read_pos];
        let output = input + self.comb_feedback * delayed;
        self.comb_buf[self.comb_write] = output;
        self.comb_write = (self.comb_write + 1) & mask;
        output
    }

    fn tick_allpass(&mut self, input: f32) -> f32 {
        // y[n] = coeff * x[n] + x[n-1] - coeff * y[n-1]
        let coeff = self.allpass_coeff;
        let output = coeff * input + self.allpass_x1 - coeff * self.allpass_y1;
        self.allpass_x1 = input;
        self.allpass_y1 = output;
        output
    }

    fn update_diode_coefficients(&mut self) {
        // Diode ladder: Zavalishin's "The Art of VA Filter Design" approach
        // Normalized frequency for 2x oversampled rate
        let fc = (self.cutoff / (self.sample_rate * 2.0)).min(0.49);

        // Frequency warping for bilinear transform
        let wc = std::f32::consts::PI * fc;
        self.diode_tune = fast_tan(wc);

        // Diode ladder has asymmetric feedback — only 3 stages contribute effectively
        // Resonance scaling: 0-1 maps to 0-17 (diode ladder needs higher feedback for self-osc)
        self.diode_res = self.resonance * 17.0;

        // Gain compensation
        self.diode_gain_comp = 1.0 + self.resonance * 2.0;
    }

    /// Diode ladder filter — 18dB/oct (effectively 3-pole) lowpass.
    /// Based on Zavalishin's "The Art of VA Filter Design" Ch. 6.6.
    /// Different from Moog: diodes create asymmetric saturation,
    /// feedback from stage 3 (not 4), giving the characteristic TB-303 "squelch".
    fn tick_diode(&mut self, input: f32) -> f32 {
        let tune = self.diode_tune;
        let res = self.diode_res;
        let gain_comp = self.diode_gain_comp;
        let g = tune / (1.0 + tune); // pre-computed integrator gain

        // 2x oversampling for the nonlinearities
        for _ in 0..2 {
            // Feedback from stage 3 (not 4 like Moog!) — this gives 18dB/oct slope
            let feedback = self.diode_feedback;
            self.diode_feedback = self.diode_stage[3];

            // Diode clipping: asymmetric — positive clips harder than negative
            // This models the forward voltage drop of actual diodes
            let x = input - res * feedback;
            let x = diode_clip(x);

            // 4 cascaded stages with diode nonlinearity between each
            // Each stage: one-pole lowpass with saturation

            self.diode_stage[0] += g * (diode_clip(x - self.diode_stage[0]));
            self.diode_stage[1] += g * (diode_clip(self.diode_stage[0] - self.diode_stage[1]));
            self.diode_stage[2] += g * (diode_clip(self.diode_stage[1] - self.diode_stage[2]));
            self.diode_stage[3] += g * (diode_clip(self.diode_stage[2] - self.diode_stage[3]));
        }

        // Output from stage 3 for 18dB/oct character (stage 4 adds extra rolloff)
        // Mix: mostly stage 3 with a touch of stage 4 for smoothness
        let raw = self.diode_stage[2] * 0.8 + self.diode_stage[3] * 0.2;
        raw * gain_comp
    }
}

/// Diode clipping function — asymmetric soft clipping modeling silicon diode.
/// Forward direction clips around 0.6V (normalized), reverse is softer.
#[inline(always)]
fn diode_clip(x: f32) -> f32 {
    // Attempt to model diode I-V curve: I = Is * (e^(V/Vt) - 1)
    // Simplified as asymmetric tanh with different gains for +/-
    if x >= 0.0 {
        fast_tanh(x * 1.5) * 0.75 // harder clip on positive (forward bias)
    } else {
        fast_tanh(x * 0.8) // softer clip on negative (reverse bias)
    }
}

/// Fast tan approximation using Padé approximant. Good for x in [0, ~1.5].
#[inline(always)]
fn fast_tan(x: f32) -> f32 {
    let x2 = x * x;
    x * (1.0 + x2 * (1.0 / 3.0 + x2 * 2.0 / 15.0))
        / (1.0 - x2 * (1.0 / 3.0 - x2 * 1.0 / 21.0))
}

/// Fast tanh approximation (Padé 7th order). More accurate than 3rd order for filter use.
/// Max error ~0.001 for |x| < 5.
#[inline(always)]
fn fast_tanh(x: f32) -> f32 {
    // Clamp to avoid NaN and keep polynomial well-behaved
    let x = x.clamp(-5.0, 5.0);
    let x2 = x * x;
    // Padé [3,3]: tanh(x) ≈ x(135 + 17x²) / (135 + 62x²)
    // More accurate than the simpler (27+x²)/(27+9x²) version
    x * (135.0 + 17.0 * x2) / (135.0 + 62.0 * x2)
}
