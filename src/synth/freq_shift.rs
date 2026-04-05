/// Frequency Shifter effect — SSB frequency shifting via quadrature oscillators.
/// Algorithm inspired by Surge XT FrequencyShifterEffect.

use std::f32::consts::{PI, TAU};

/// Simple 2-stage allpass for Hilbert-like quadrature approximation.
struct QuadAllpass {
    z1: [f32; 2],
    z2: [f32; 2],
}

impl QuadAllpass {
    fn new() -> Self {
        Self { z1: [0.0; 2], z2: [0.0; 2] }
    }

    fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }

    /// Process one sample through a cascade of allpass sections.
    /// Returns (in-phase, quadrature) pair.
    #[inline]
    fn process(&mut self, input: f32, coeffs: &[f32; 2]) -> (f32, f32) {
        // Stage 0: allpass for I path
        let ap0_out = coeffs[0] * (input - self.z2[0]) + self.z1[0];
        self.z2[0] = ap0_out;
        self.z1[0] = input;

        // Stage 1: allpass for Q path
        let ap1_out = coeffs[1] * (input - self.z2[1]) + self.z1[1];
        self.z2[1] = ap1_out;
        self.z1[1] = input;

        (ap0_out, ap1_out)
    }
}

pub struct FreqShift {
    sample_rate: f32,
    // Quadrature oscillator state
    osc_re: f32,
    osc_im: f32,
    // Cached phasor rotation coefficients
    phasor_cos: f32,
    phasor_sin: f32,
    prev_shift_hz: f32,
    // Hilbert approximation allpass
    allpass_l: QuadAllpass,
    allpass_r: QuadAllpass,
    // Feedback delay buffer (small, ~128 samples)
    delay_buf_l: [f32; 256],
    delay_buf_r: [f32; 256],
    delay_pos: usize,
    // DC blocker
    dc_l: f32,
    dc_r: f32,
    dc_prev_l: f32,
    dc_prev_r: f32,
    dc_coeff: f32,
}

impl FreqShift {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            osc_re: 1.0, osc_im: 0.0,
            phasor_cos: 1.0, phasor_sin: 0.0,
            prev_shift_hz: f32::NAN,
            allpass_l: QuadAllpass::new(),
            allpass_r: QuadAllpass::new(),
            delay_buf_l: [0.0; 256],
            delay_buf_r: [0.0; 256],
            delay_pos: 0,
            dc_l: 0.0, dc_r: 0.0, dc_prev_l: 0.0, dc_prev_r: 0.0,
            dc_coeff: 1.0 - (PI * 35.0 / sample_rate),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.osc_re = 1.0;
        self.osc_im = 0.0;
        self.allpass_l.reset();
        self.allpass_r.reset();
        self.delay_buf_l = [0.0; 256];
        self.delay_buf_r = [0.0; 256];
        self.dc_l = 0.0; self.dc_r = 0.0;
        self.dc_prev_l = 0.0; self.dc_prev_r = 0.0;
        self.dc_coeff = 1.0 - (PI * 35.0 / sr);
        self.prev_shift_hz = f32::NAN;
    }

    #[inline]
    pub fn tick(
        &mut self, in_l: f32, in_r: f32,
        shift_hz: f32, feedback: f32, delay: f32, mix: f32,
    ) -> (f32, f32) {
        if mix < 0.001 { return (in_l, in_r); }

        let fb = feedback.clamp(0.0, 0.9);
        let delay_samples = (delay * self.sample_rate * 0.01).clamp(1.0, 255.0) as usize;

        // Read from delay buffer for feedback
        let read_pos = (self.delay_pos + 256 - delay_samples) & 255;
        let fb_l = self.delay_buf_l[read_pos] * fb;
        let fb_r = self.delay_buf_r[read_pos] * fb;

        // Input with feedback (soft-saturate feedback)
        let sat = |x: f32| -> f32 { x / (1.0 + x.abs() * 0.3) };
        let sig_l = in_l + sat(fb_l);
        let sig_r = in_r + sat(fb_r);

        // Quadrature oscillator: complex rotation
        if shift_hz != self.prev_shift_hz {
            let omega = TAU * shift_hz / self.sample_rate;
            self.phasor_cos = omega.cos();
            self.phasor_sin = omega.sin();
            self.prev_shift_hz = shift_hz;
        }
        let cos_w = self.phasor_cos;
        let sin_w = self.phasor_sin;
        let new_re = self.osc_re * cos_w - self.osc_im * sin_w;
        let new_im = self.osc_re * sin_w + self.osc_im * cos_w;
        self.osc_re = new_re;
        self.osc_im = new_im;
        // Renormalize periodically to prevent drift
        let mag_sq = self.osc_re * self.osc_re + self.osc_im * self.osc_im;
        if (mag_sq - 1.0).abs() > 0.001 {
            let inv_mag = 1.0 / mag_sq.sqrt();
            self.osc_re *= inv_mag;
            self.osc_im *= inv_mag;
        }

        // Hilbert transform approximation (allpass-based)
        // Coefficients for wideband 90-degree phase split
        let coeffs = [0.4021921162, 0.8561710882_f32];
        let (i_l, q_l) = self.allpass_l.process(sig_l, &coeffs);
        let (i_r, q_r) = self.allpass_r.process(sig_r, &coeffs);

        // SSB modulation: shifted = I*cos - Q*sin (upper sideband for positive shift)
        let wet_l = i_l * self.osc_re - q_l * self.osc_im;
        let wet_r = i_r * self.osc_re - q_r * self.osc_im;

        // Write to delay buffer
        self.delay_buf_l[self.delay_pos] = wet_l;
        self.delay_buf_r[self.delay_pos] = wet_r;
        self.delay_pos = (self.delay_pos + 1) & 255;

        // DC blocker (35 Hz HP)
        let dc_coeff = self.dc_coeff;
        let out_l = wet_l - self.dc_prev_l + dc_coeff * self.dc_l;
        self.dc_prev_l = wet_l;
        self.dc_l = out_l;
        let out_r = wet_r - self.dc_prev_r + dc_coeff * self.dc_r;
        self.dc_prev_r = wet_r;
        self.dc_r = out_r;

        let m = mix;
        (in_l * (1.0 - m) + out_l * m, in_r * (1.0 - m) + out_r * m)
    }
}
