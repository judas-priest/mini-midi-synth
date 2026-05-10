/// BBD Ensemble chorus — 3-tap bucket brigade device chorus.
///
/// Models the Roland Juno-106 / Korg Poly-800 style ensemble effect.
/// Three chorus voices with slightly different delay times and LFO rates.
/// Anti-aliasing LP filters at each tap (BBD clock noise emulation).

use super::dsp_utils::buf_read_linear;

const BUF: usize = 4096;

pub struct BbdEnsemble {
    sample_rate: f32,
    buf: Vec<f32>,
    write: usize,
    // 3 LFO phases at slightly different rates for chorus spread
    phases: [f32; 3],
    // BBD input LP filter state (separate from per-tap output filters)
    input_lp: f32,
    // BBD LP filter states (per tap, per channel)
    lp_l: [f32; 3],
    lp_r: [f32; 3],
}

/// Tap configurations: [center_ms, depth_ms, lfo_rate_hz]
const TAPS: [(f32, f32, f32); 3] = [
    (5.0, 1.5, 0.518),
    (7.5, 2.0, 0.723),
    (10.0, 2.5, 1.031),
];

impl BbdEnsemble {
    pub fn new(sr: f32) -> Self {
        Self {
            sample_rate: sr,
            buf: vec![0.0; BUF],
            write: 0,
            phases: [0.0, 0.33, 0.67],
            input_lp: 0.0,
            lp_l: [0.0; 3],
            lp_r: [0.0; 3],
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.buf = vec![0.0; BUF];
        self.write = 0;
        self.input_lp = 0.0;
        self.lp_l = [0.0; 3];
        self.lp_r = [0.0; 3];
    }

    /// Process mono input, returns stereo output.
    /// `depth` (0..1): modulation depth.
    /// `rate` (0..1): LFO rate scaling (0.5 = normal, 1.0 = fast).
    pub fn tick(&mut self, input: f32, depth: f32, rate: f32, mix: f32) -> (f32, f32) {
        // BBD input LP filter (~8 kHz cutoff), SR-independent
        let bbd_coef = 1.0 - (-2.0 * std::f32::consts::PI * 8000.0 / self.sample_rate).exp();
        let filtered = self.input_lp + bbd_coef * (input - self.input_lp);
        self.input_lp = filtered;

        self.buf[self.write] = filtered;
        self.write = (self.write + 1) % BUF;

        if mix < 0.001 {
            return (input, input);
        }

        let sr = self.sample_rate;
        let rate_scale = rate * 2.0 + 0.2; // 0.2..2.2 Hz base
        let mut out_l = 0.0_f32;
        let mut out_r = 0.0_f32;

        for i in 0..3 {
            let (center_ms, depth_ms, lfo_hz) = TAPS[i];
            self.phases[i] += lfo_hz * rate_scale / sr;
            if self.phases[i] >= 1.0 { self.phases[i] -= 1.0; }

            let lfo = (self.phases[i] * std::f32::consts::TAU).sin();
            let center_samp = center_ms * 0.001 * sr;
            let depth_samp = depth_ms * 0.001 * sr * depth;

            let delay_l = center_samp + depth_samp * lfo;
            let delay_r = center_samp - depth_samp * lfo; // opposite phase for stereo

            let wet_l = buf_read_linear(&self.buf, self.write, delay_l);
            let wet_r = buf_read_linear(&self.buf, self.write, delay_r);

            // BBD output LP per tap (~8 kHz), SR-independent
            let bbd_out = 1.0 - (-2.0 * std::f32::consts::PI * 8000.0 / sr).exp();
            let wl = self.lp_l[i] + bbd_out * (wet_l - self.lp_l[i]);
            self.lp_l[i] = wl;
            let wr = self.lp_r[i] + bbd_out * (wet_r - self.lp_r[i]);
            self.lp_r[i] = wr;

            out_l += wl;
            out_r += wr;
        }

        out_l /= 3.0;
        out_r /= 3.0;

        let dry = 1.0 - mix;
        (input * dry + out_l * mix, input * dry + out_r * mix)
    }
}

