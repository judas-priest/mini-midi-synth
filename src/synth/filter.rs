//! State-variable filter (SVF) — lowpass, highpass, bandpass.
//! Plus Moog ladder filter (Huovilainen 2004 improved model with 2x oversampling).
//! Plus K35 (Korg MS-20 Sallen-Key), Notch, LP24, HP24.
//! Plus OB-Xd 2-pole/4-pole (saturating SVF), Tripole 18dB/oct, Sample&Hold,
//! Cutoff Warp and Resonance Warp variants.

use std::f32::consts::PI;
use super::dsp_utils::{fast_tan, fast_tanh};

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
    Notch,     // 11 — SVF Notch (band-reject)
    LP24,      // 12 — SVF 24dB/oct lowpass (2 cascaded SVF stages)
    HP24,      // 13 — SVF 24dB/oct highpass (2 cascaded SVF stages)
    K35LP,         // 14 — Korg MS-20 Sallen-Key lowpass (ZDF, self-oscillating)
    K35HP,         // 15 — Korg MS-20 Sallen-Key highpass (ZDF, self-oscillating)
    BP24,          // 16 — SVF 24dB/oct bandpass
    Notch24,       // 17 — SVF 24dB/oct notch
    OBXd2LP,       // 18 — OB-Xd 2-pole lowpass (saturating SVF)
    OBXd2HP,       // 19 — OB-Xd 2-pole highpass
    OBXd2BP,       // 20 — OB-Xd 2-pole bandpass
    OBXd2Notch,    // 21 — OB-Xd 2-pole notch
    OBXd4P,        // 22 — OB-Xd 4-pole lowpass
    Tripole,       // 23 — 3-pole 18dB/oct lowpass
    SampleHold,    // 24 — Sample & Hold (ZOH)
    CutoffWarpLP,  // 25 — SVF with tanh on integrator input (LP)
    CutoffWarpHP,  // 26 — SVF with tanh on integrator input (HP)
    CutoffWarpBP,  // 27 — SVF with tanh on integrator input (BP)
    CutoffWarpNotch, // 28 — SVF with tanh on integrator input (Notch)
    CutoffWarpAP,  // 29 — SVF with tanh on integrator input (Allpass)
    ResWarpLP,     // 30 — SVF with tanh on resonance feedback (LP)
    ResWarpHP,     // 31 — SVF with tanh on resonance feedback (HP)
    ResWarpBP,     // 32 — SVF with tanh on resonance feedback (BP)
    ResWarpNotch,  // 33 — SVF with tanh on resonance feedback (Notch)
    ResWarpAP,     // 34 — SVF with tanh on resonance feedback (Allpass)
    VintageLadderLP, // 35 — Moog ladder with tanh only at input (warmer, linear integrators)
    SVFMorph,      // 36 — SVF with LP/BP/HP morphing via svf_morph parameter
    PolivoksLP,    // 37 — Polivoks К140УД12 op-amp SVF, lowpass
    PolivoksBP,    // 38 — Polivoks К140УД12 op-amp SVF, bandpass
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
            11 => Self::Notch,
            12 => Self::LP24,
            13 => Self::HP24,
            14 => Self::K35LP,
            15 => Self::K35HP,
            16 => Self::BP24,
            17 => Self::Notch24,
            18 => Self::OBXd2LP,
            19 => Self::OBXd2HP,
            20 => Self::OBXd2BP,
            21 => Self::OBXd2Notch,
            22 => Self::OBXd4P,
            23 => Self::Tripole,
            24 => Self::SampleHold,
            25 => Self::CutoffWarpLP,
            26 => Self::CutoffWarpHP,
            27 => Self::CutoffWarpBP,
            28 => Self::CutoffWarpNotch,
            29 => Self::CutoffWarpAP,
            30 => Self::ResWarpLP,
            31 => Self::ResWarpHP,
            32 => Self::ResWarpBP,
            33 => Self::ResWarpNotch,
            34 => Self::ResWarpAP,
            35 => Self::VintageLadderLP,
            36 => Self::SVFMorph,
            37 => Self::PolivoksLP,
            38 => Self::PolivoksBP,
            _ => Self::LowPass,
        }
    }

    pub fn is_moog(self) -> bool {
        matches!(self, Self::MoogLP24 | Self::MoogLP12 | Self::VintageLadderLP)
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

    pub fn is_polivoks(self) -> bool {
        matches!(self, Self::PolivoksLP | Self::PolivoksBP)
    }

    #[allow(dead_code)]
    pub fn is_k35(self) -> bool {
        matches!(self, Self::K35LP | Self::K35HP)
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
    comb_dc_x1: f32,            // DC blocker state (previous input)
    comb_dc_y1: f32,            // DC blocker state (previous output)
    // Allpass filter state
    allpass_x1: f32,            // x[n-1]
    allpass_y1: f32,            // y[n-1]
    allpass_coeff: f32,         // allpass coefficient
    // LP24/HP24 second SVF stage state
    ic1eq2: f32,
    ic2eq2: f32,
    // K35 (Korg MS-20 Sallen-Key) one-pole stage states
    k35_s1: f32,
    k35_s2: f32,
    // OB-Xd 2-pole: separate integrator states (SVF with saturation on state updates)
    obxd_s1: f32,   // OB-Xd integrator state 1
    obxd_s2: f32,   // OB-Xd integrator state 2
    // OB-Xd 4-pole: second cascaded stage
    obxd4_s1: f32,
    obxd4_s2: f32,
    // Tripole: 3 one-pole integrator states
    tri_s1: f32,
    tri_s2: f32,
    tri_s3: f32,
    // Sample & Hold
    snh_phase: f32,  // phase accumulator 0..1
    snh_held: f32,   // held sample value
    // SVF Morph parameter (0=LP, 0.5=BP, 1=HP)
    pub svf_morph: f32,
    // Polivoks state
    pv_s1: f32,      // integrator 1 state (bandpass)
    pv_s2: f32,      // integrator 2 state (lowpass)
    pv_delay: f32,   // z^-1 for half-sample resonance feedback delay
    pv_tune: f32,    // frequency coefficient
    pv_res: f32,     // resonance feedback amount
    pub pv_drive: f32,   // 0..1 input drive (set from VoiceParams)
    pub pv_starve: f32,  // 0..1 power-supply starvation
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
            comb_dc_x1: 0.0,
            comb_dc_y1: 0.0,
            // Allpass filter state
            allpass_x1: 0.0,
            allpass_y1: 0.0,
            allpass_coeff: 0.0,
            // LP24/HP24 second SVF stage
            ic1eq2: 0.0,
            ic2eq2: 0.0,
            // K35 Sallen-Key stage states
            k35_s1: 0.0,
            k35_s2: 0.0,
            // OB-Xd 2-pole
            obxd_s1: 0.0,
            obxd_s2: 0.0,
            // OB-Xd 4-pole second stage
            obxd4_s1: 0.0,
            obxd4_s2: 0.0,
            // Tripole
            tri_s1: 0.0,
            tri_s2: 0.0,
            tri_s3: 0.0,
            // Sample & Hold
            snh_phase: 0.0,
            snh_held: 0.0,
            svf_morph: 0.0,
            pv_s1: 0.0,
            pv_s2: 0.0,
            pv_delay: 0.0,
            pv_tune: 0.0,
            pv_res: 0.0,
            pv_drive: 0.0,
            pv_starve: 0.0,
            coeff_counter: 7, // so first tick after set_cutoff/set_resonance triggers update
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
        self.ic1eq2 = 0.0;
        self.ic2eq2 = 0.0;
        self.k35_s1 = 0.0;
        self.k35_s2 = 0.0;
        self.obxd_s1 = 0.0;
        self.obxd_s2 = 0.0;
        self.obxd4_s1 = 0.0;
        self.obxd4_s2 = 0.0;
        self.tri_s1 = 0.0;
        self.tri_s2 = 0.0;
        self.tri_s3 = 0.0;
        self.snh_phase = 0.0;
        self.snh_held = 0.0;
        self.moog_stage = [0.0; 4];
        self.moog_tanh = [0.0; 4];
        self.moog_delay4 = 0.0;
        self.diode_stage = [0.0; 4];
        self.diode_feedback = 0.0;
        self.comb_buf.fill(0.0);
        self.comb_write = 0;
        self.comb_dc_x1 = 0.0;
        self.comb_dc_y1 = 0.0;
        self.allpass_x1 = 0.0;
        self.allpass_y1 = 0.0;
        self.pv_s1 = 0.0;
        self.pv_s2 = 0.0;
        self.pv_delay = 0.0;
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
        } else if self.filter_type.is_polivoks() {
            self.update_polivoks_coefficients();
        } else {
            // SVF, Notch, LP24/HP24/BP24/Notch24, K35, OBXd, Tripole, Warp filters
            // all use SVF g/k/a1/a2/a3 coefficients.
            // OBXd computes its own local k/a1/a2/a3 per tick; g is shared.
            // SampleHold uses cutoff directly in tick, so g still warmed up here.
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
        self.coeff_counter = 7; // next tick will update again with latest cutoff from envelope
    }

    pub fn tick(&mut self, input: f32) -> f32 {
        self.coeff_counter = self.coeff_counter.wrapping_add(1);
        if (self.coeff_counter & 7 == 0) && self.dirty {
            self.update_coefficients();
        }

        match self.filter_type {
            FilterType::MoogLP24 => self.tick_moog(input, true),
            FilterType::MoogLP12 => self.tick_moog(input, false),
            FilterType::DiodeLP => self.tick_diode(input),
            FilterType::Comb | FilterType::CombPos | FilterType::CombNeg => self.tick_comb(input),
            FilterType::Allpass => self.tick_allpass(input),
            FilterType::LP24 => self.tick_lp24(input),
            FilterType::HP24 => self.tick_hp24(input),
            FilterType::K35LP => self.tick_k35lp(input),
            FilterType::K35HP => self.tick_k35hp(input),
            FilterType::BP24 => self.tick_bp24(input),
            FilterType::Notch24 => self.tick_notch24(input),
            FilterType::OBXd2LP => self.tick_obxd2(input, 0),
            FilterType::OBXd2HP => self.tick_obxd2(input, 1),
            FilterType::OBXd2BP => self.tick_obxd2(input, 2),
            FilterType::OBXd2Notch => self.tick_obxd2(input, 3),
            FilterType::OBXd4P => self.tick_obxd4(input),
            FilterType::Tripole => self.tick_tripole(input),
            FilterType::SampleHold => self.tick_snh(input),
            FilterType::CutoffWarpLP => self.tick_cutoff_warp(input, 0),
            FilterType::CutoffWarpHP => self.tick_cutoff_warp(input, 1),
            FilterType::CutoffWarpBP => self.tick_cutoff_warp(input, 2),
            FilterType::CutoffWarpNotch => self.tick_cutoff_warp(input, 3),
            FilterType::CutoffWarpAP => self.tick_cutoff_warp(input, 4),
            FilterType::ResWarpLP => self.tick_resonance_warp(input, 0),
            FilterType::ResWarpHP => self.tick_resonance_warp(input, 1),
            FilterType::ResWarpBP => self.tick_resonance_warp(input, 2),
            FilterType::ResWarpNotch => self.tick_resonance_warp(input, 3),
            FilterType::ResWarpAP => self.tick_resonance_warp(input, 4),
            FilterType::VintageLadderLP => self.tick_vintage_ladder(input),
            FilterType::SVFMorph => self.tick_svf_morph(input),
            FilterType::PolivoksLP => self.tick_polivoks(input, false),
            FilterType::PolivoksBP => self.tick_polivoks(input, true),
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
            FilterType::Notch => input - self.k * v1,
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
        // DC blocker on feedback path (one-pole HPF ~10 Hz at any SR)
        // y[n] = R * y[n-1] + x[n] - x[n-1], R ≈ 0.9995
        let dc_in = self.comb_feedback * delayed;
        let dc_out = dc_in - self.comb_dc_x1 + 0.9995 * self.comb_dc_y1;
        self.comb_dc_x1 = dc_in;
        self.comb_dc_y1 = dc_out;
        let output = input + dc_out;
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

    fn update_polivoks_coefficients(&mut self) {
        use std::f32::consts::PI;
        // Slew rate per sample at 2x oversampled rate.
        // К140УД12 bandwidth is directly proportional to bias current → linear slew limit.
        // max_slew = 2π * fc / oversample_rate
        let fc = self.cutoff.min(self.sample_rate * 0.45);
        self.pv_tune = 2.0 * PI * fc / (self.sample_rate * 2.0);
        // Resonance: 0..1 → 0..1.8 (self-oscillation starts ~0.9, trapezoidal above that)
        self.pv_res = self.resonance * 1.8;
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

    /// LP24 — 24dB/oct lowpass: two cascaded SVF stages.
    fn tick_lp24(&mut self, input: f32) -> f32 {
        // First SVF stage (LP output feeds second stage)
        let v3 = input - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        let lp1 = v2;

        // Second SVF stage
        let v3b = lp1 - self.ic2eq2;
        let v1b = self.a1 * self.ic1eq2 + self.a2 * v3b;
        let v2b = self.ic2eq2 + self.a2 * self.ic1eq2 + self.a3 * v3b;
        self.ic1eq2 = 2.0 * v1b - self.ic1eq2;
        self.ic2eq2 = 2.0 * v2b - self.ic2eq2;
        v2b
    }

    /// HP24 — 24dB/oct highpass: two cascaded SVF stages.
    fn tick_hp24(&mut self, input: f32) -> f32 {
        // First SVF stage (HP output feeds second stage)
        let v3 = input - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        let hp1 = input - self.k * v1 - v2;

        // Second SVF stage
        let v3b = hp1 - self.ic2eq2;
        let v1b = self.a1 * self.ic1eq2 + self.a2 * v3b;
        let v2b = self.ic2eq2 + self.a2 * self.ic1eq2 + self.a3 * v3b;
        self.ic1eq2 = 2.0 * v1b - self.ic1eq2;
        self.ic2eq2 = 2.0 * v2b - self.ic2eq2;
        hp1 - self.k * v1b - v2b
    }

    /// BP24 — 24dB/oct bandpass: two cascaded SVF stages, bandpass output.
    fn tick_bp24(&mut self, input: f32) -> f32 {
        // First SVF stage → bandpass
        let v3 = input - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        let bp1 = v1;

        // Second SVF stage → bandpass
        let v3b = bp1 - self.ic2eq2;
        let v1b = self.a1 * self.ic1eq2 + self.a2 * v3b;
        let v2b = self.ic2eq2 + self.a2 * self.ic1eq2 + self.a3 * v3b;
        self.ic1eq2 = 2.0 * v1b - self.ic1eq2;
        self.ic2eq2 = 2.0 * v2b - self.ic2eq2;
        v1b
    }

    /// Notch24 — 24dB/oct notch: two cascaded SVF stages, notch output from each.
    fn tick_notch24(&mut self, input: f32) -> f32 {
        let v3 = input - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        let n1 = input - self.k * v1;

        let v3b = n1 - self.ic2eq2;
        let v1b = self.a1 * self.ic1eq2 + self.a2 * v3b;
        let v2b = self.ic2eq2 + self.a2 * self.ic1eq2 + self.a3 * v3b;
        self.ic1eq2 = 2.0 * v1b - self.ic1eq2;
        self.ic2eq2 = 2.0 * v2b - self.ic2eq2;
        n1 - self.k * v1b
    }

    /// OB-Xd 2-pole — State variable filter with tanh saturation on state updates.
    /// Inspired by the OB-Xd open source plugin (GPL). The distinguishing feature:
    /// state variables are updated through tanh, giving warmer self-oscillation and
    /// a more musical resonance character than a clean SVF.
    ///
    /// Resonance k goes from 2.0 (no resonance) to near 0 (self-oscillation),
    /// allowing the filter to ring freely at max resonance.
    fn tick_obxd2(&mut self, input: f32, mode: u8) -> f32 {
        let g = self.g;
        // OB-Xd: k approaches 0 for self-oscillation (resonance 0..1 → k 2..0.02)
        let k = (2.0 - self.resonance * 1.98).max(0.02);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = input - self.obxd_s2;
        let v1 = a1 * self.obxd_s1 + a2 * v3;
        let v2 = self.obxd_s2 + a2 * self.obxd_s1 + a3 * v3;

        // Saturate state updates — key OB character: prevents explosion, adds warmth
        self.obxd_s1 = fast_tanh(2.0 * v1 - self.obxd_s1);
        self.obxd_s2 = fast_tanh(2.0 * v2 - self.obxd_s2);

        match mode {
            0 => v2,                       // LP
            1 => input - k * v1 - v2,     // HP
            2 => v1 * k,                   // BP (gain-compensated)
            _ => input - k * v1,           // Notch
        }
    }

    /// OB-Xd 4-pole — Two cascaded OBXd2 stages for 24dB/oct lowpass.
    /// First stage carries all the resonance; second stage is neutral (k=2).
    fn tick_obxd4(&mut self, input: f32) -> f32 {
        let g = self.g;
        // First stage: resonance on this stage drives the character
        let k = (2.0 - self.resonance * 1.9).max(0.02);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = input - self.obxd_s2;
        let v1 = a1 * self.obxd_s1 + a2 * v3;
        let v2 = self.obxd_s2 + a2 * self.obxd_s1 + a3 * v3;
        self.obxd_s1 = fast_tanh(2.0 * v1 - self.obxd_s1);
        self.obxd_s2 = fast_tanh(2.0 * v2 - self.obxd_s2);
        let lp1 = v2;

        // Second stage: no resonance, just additional slope
        let k2 = 2.0_f32;
        let a1b = 1.0 / (1.0 + g * (g + k2));
        let a2b = g * a1b;
        let a3b = g * a2b;

        let v3b = lp1 - self.obxd4_s2;
        let v1b = a1b * self.obxd4_s1 + a2b * v3b;
        let v2b = self.obxd4_s2 + a2b * self.obxd4_s1 + a3b * v3b;
        self.obxd4_s1 = fast_tanh(2.0 * v1b - self.obxd4_s1);
        self.obxd4_s2 = fast_tanh(2.0 * v2b - self.obxd4_s2);

        // Gain compensation: recover passband level lost to resonance
        (v2b * (1.0 + self.resonance)).clamp(-2.0, 2.0)
    }

    /// Tripole — 3-pole 18dB/oct lowpass with resonance feedback.
    /// Three cascaded one-pole stages with global feedback from the output.
    fn tick_tripole(&mut self, input: f32) -> f32 {
        let g = self.g;
        // Resonance feedback gain (approaches self-oscillation near 1.0)
        let res = self.resonance * 3.5;

        // Subtract resonance feedback from input, clamp to prevent runaway
        let x = (input - res * self.tri_s3).clamp(-4.0, 4.0);

        // One-pole integrator gain (bilinear transform: g / (1 + g))
        let g1 = g / (1.0 + g);

        // Stage 1
        let lp1 = g1 * x + self.tri_s1;
        self.tri_s1 = (2.0 * lp1 - self.tri_s1).clamp(-4.0, 4.0) + 1e-30;

        // Stage 2
        let lp2 = g1 * lp1 + self.tri_s2;
        self.tri_s2 = (2.0 * lp2 - self.tri_s2).clamp(-4.0, 4.0) + 1e-30;

        // Stage 3
        let lp3 = g1 * lp2 + self.tri_s3;
        self.tri_s3 = (2.0 * lp3 - self.tri_s3).clamp(-4.0, 4.0) + 1e-30;

        (lp3 * (1.0 + self.resonance * 0.5)).clamp(-2.0, 2.0)
    }

    /// Sample & Hold — Zero-order hold at the cutoff frequency.
    /// Samples the input at rate = cutoff_hz; between samples the output is held constant.
    fn tick_snh(&mut self, input: f32) -> f32 {
        let inc = self.cutoff / self.sample_rate;
        self.snh_phase += inc;
        if self.snh_phase >= 1.0 {
            self.snh_phase -= 1.0;
            self.snh_held = input;
        }
        self.snh_held
    }

    /// Cutoff Warp filters — SVF with tanh saturation on v3 (the input to both integrators).
    /// Effect: at high drive levels the effective cutoff "warps" toward a softer ceiling,
    /// giving a rounded, warm character compared to a clean SVF.
    fn tick_cutoff_warp(&mut self, input: f32, mode: u8) -> f32 {
        let v3 = input - self.ic2eq;
        let v3_sat = fast_tanh(v3); // saturate before integration
        let v1 = self.a1 * self.ic1eq + self.a2 * v3_sat;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3_sat;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        match mode {
            0 => v2,                                    // LP
            1 => input - self.k * v1 - v2,             // HP
            2 => v1,                                    // BP
            3 => input - self.k * v1,                   // Notch
            _ => input - 2.0 * (v1 * self.k),          // AP
        }
    }

    /// Resonance Warp filters — SVF with tanh saturation on the resonance feedback term.
    /// The k*v1 term is run through tanh before being subtracted, which limits how
    /// aggressively the resonance peak can grow — warm, musical resonance soft-limiting.
    fn tick_resonance_warp(&mut self, input: f32, mode: u8) -> f32 {
        let v3 = input - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        // Saturate the resonance feedback — limits peak height musically
        let k_sat = fast_tanh(self.k * v1);
        match mode {
            0 => v2,                       // LP
            1 => input - k_sat - v2,       // HP
            2 => v1,                       // BP
            3 => input - k_sat,            // Notch
            _ => input - 2.0 * k_sat,     // AP
        }
    }

    /// Vintage Ladder LP — Moog ladder with tanh only at input (warmer, linear integrators).
    /// Simplified from tick_moog: only applies fast_tanh() at the input stage, not per-stage.
    /// Linear integrators give a warmer, less aggressive character than the full Huovilainen model.
    fn tick_vintage_ladder(&mut self, input: f32) -> f32 {
        for _ in 0..2 {
            let feedback = (self.moog_stage[3] + self.moog_delay4) * 0.5;
            self.moog_delay4 = self.moog_stage[3];
            let x = fast_tanh((input - self.moog_res_quad * feedback) * (1.0 / 1.220_703_1_f32));
            // Linear integrators (warmer character, no per-stage saturation)
            self.moog_stage[0] += self.moog_tune * (x - self.moog_stage[0]);
            self.moog_stage[1] += self.moog_tune * (self.moog_stage[0] - self.moog_stage[1]);
            self.moog_stage[2] += self.moog_tune * (self.moog_stage[1] - self.moog_stage[2]);
            self.moog_stage[3] += self.moog_tune * (self.moog_stage[2] - self.moog_stage[3]);
        }
        self.moog_stage[3] * self.moog_gain_comp
    }

    /// Polivoks — К140УД12 slew-rate limiting filter, 2x oversampled.
    ///
    /// The К140УД12 is a current-controlled programmable op-amp used as a slew limiter,
    /// not an exponential integrator. This is the fundamental difference from all other
    /// filters here: the integrator step is clamped (linear rate limit), not multiplied
    /// (exponential decay). This produces trapezoidal self-oscillation at high resonance —
    /// the defining sound of the Polivoks.
    ///
    /// Asymmetric slew: rising edge is ~30% faster than falling (FET output stage asymmetry
    /// in the original circuit — T15 low impedance sourcing vs T16 high impedance sinking).
    ///
    /// Starve = reduced supply current → reduced max slew at large signal amplitudes →
    /// the characteristic "bubble" instability when resonance meets power-supply sag.
    fn tick_polivoks(&mut self, input: f32, bandpass: bool) -> f32 {
        let max_slew = self.pv_tune;
        let res = self.pv_res;
        let drive_gain = 1.0 + self.pv_drive * 3.0;
        let starve = self.pv_starve;

        for _ in 0..2 {
            // Resonance feedback from LP output (half-sample delay for stability)
            let feedback = (self.pv_s2 + self.pv_delay) * 0.5;
            self.pv_delay = self.pv_s2;

            let x = (input * drive_gain) - res * feedback;

            // Stage 1: slew-rate limited integrator — asymmetric rise/fall (FET output stage)
            // Starve reduces max slew when signal is large (power-supply sag behavior)
            let sag = (1.0 - starve * self.pv_s1.abs() * 0.4).max(0.2);
            let delta1 = x - self.pv_s1;
            let slew1_up   = max_slew * 1.3 * sag;  // faster rise
            let slew1_down = max_slew * sag;          // slower fall
            self.pv_s1 += if delta1 > 0.0 {
                delta1.min(slew1_up)
            } else {
                delta1.max(-slew1_down)
            };

            // Stage 2: second slew-rate limited integrator (symmetric — no output FET asymmetry)
            let delta2 = self.pv_s1 - self.pv_s2;
            let slew2 = max_slew * (1.0 - starve * self.pv_s2.abs() * 0.2).max(0.2);
            self.pv_s2 += delta2.clamp(-slew2, slew2);
        }

        if bandpass { self.pv_s1 } else { self.pv_s2 }
    }

    /// SVF Morph — SVF with LP/BP/HP morphing via svf_morph parameter.
    /// svf_morph = 0.0 → pure LP, 0.5 → pure BP, 1.0 → pure HP.
    fn tick_svf_morph(&mut self, input: f32) -> f32 {
        let v3 = input - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;

        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        let lp = v2;
        let bp = v1;
        let hp = input - self.k * v1 - v2;

        // Morph: 0=LP, 0.5=BP, 1=HP
        let m = self.svf_morph.clamp(0.0, 1.0);
        if m < 0.5 {
            let t = m * 2.0;
            lp * (1.0 - t) + bp * t
        } else {
            let t = (m - 0.5) * 2.0;
            bp * (1.0 - t) + hp * t
        }
    }

    /// K35LP — Korg MS-20 Sallen-Key lowpass (ZDF, self-oscillating at high resonance).
    /// Uses two one-pole LP stages with resonance feedback from output.
    fn tick_k35lp(&mut self, input: f32) -> f32 {
        let g = self.g;
        let k = self.resonance * 3.5; // 0..3.5 feedback (approaches self-oscillation)
        let g1 = 1.0 + g;

        // ZDF: solve for lp2 algebraically then compute lp1
        let denom = (g1 * g1 + g * g * k).max(1e-10);
        let lp2 = (g * g * input + g * self.k35_s1 + self.k35_s2 * g1) / denom;
        let lp1 = (g * (input - k * lp2) + self.k35_s1) / g1;

        self.k35_s1 = 2.0 * lp1 - self.k35_s1 + 1e-30;
        self.k35_s2 = 2.0 * lp2 - self.k35_s2 + 1e-30;

        // Gain compensation (k=3.5 → div by 1+1.75=2.75)
        (lp2 / (1.0 + k * 0.5)).clamp(-2.0, 2.0)
    }

    /// K35HP — Korg MS-20 Sallen-Key highpass (ZDF, self-oscillating at high resonance).
    /// Uses two one-pole HP stages with resonance feedback from output.
    fn tick_k35hp(&mut self, input: f32) -> f32 {
        let g = self.g;
        let k = self.resonance * 3.5;
        let g1 = 1.0 + g;

        // ZDF: solve for hp2 algebraically
        // hp1 = (x1 - s1) / (1+g),  where x1 = input - k*hp2
        // hp2 = (hp1 - s2) / (1+g)
        // → hp2*((1+g)^2 + k) = input - s1 - s2*(1+g)
        let denom = (g1 * g1 + k).max(1e-10);
        let hp2 = (input - self.k35_s1 - self.k35_s2 * g1) / denom;
        let lp1 = (g * (input - k * hp2) + self.k35_s1) / g1;
        let hp1 = (input - k * hp2) - lp1;
        let lp2 = (g * hp1 + self.k35_s2) / g1;

        self.k35_s1 = 2.0 * lp1 - self.k35_s1 + 1e-30;
        self.k35_s2 = 2.0 * lp2 - self.k35_s2 + 1e-30;

        (hp2 / (1.0 + k * 0.5)).clamp(-2.0, 2.0)
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

