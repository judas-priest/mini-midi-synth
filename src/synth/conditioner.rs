#![allow(dead_code)]
/// Conditioner — mastering-style signal conditioner.
///
/// Combines:
///   1. Two-pole high-pass filter for bass cut (20 Hz..400 Hz).
///   2. Mid/Side stereo width control.
///   3. Soft limiter based on tanh.
///
/// Inspired by Surge XT's Conditioner effect.
use std::f32::consts::PI;

pub struct Conditioner {
    sample_rate: f32,
    // 2-pole HP filter state — left channel
    hp_x1_l: f32,
    hp_x2_l: f32,
    hp_y1_l: f32,
    hp_y2_l: f32,
    // 2-pole HP filter state — right channel
    hp_x1_r: f32,
    hp_x2_r: f32,
    hp_y1_r: f32,
    hp_y2_r: f32,
    // Cached HP coefficients (biquad, normalised by a0)
    hp_b0: f32,
    hp_b1: f32,
    hp_b2: f32,
    hp_a1: f32,
    hp_a2: f32,
    last_bass_cut: f32,
}

impl Conditioner {
    pub fn new(sample_rate: f32) -> Self {
        let mut s = Self {
            sample_rate,
            hp_x1_l: 0.0, hp_x2_l: 0.0, hp_y1_l: 0.0, hp_y2_l: 0.0,
            hp_x1_r: 0.0, hp_x2_r: 0.0, hp_y1_r: 0.0, hp_y2_r: 0.0,
            hp_b0: 1.0, hp_b1: 0.0, hp_b2: 0.0, hp_a1: 0.0, hp_a2: 0.0,
            last_bass_cut: -1.0, // force first update
        };
        s.update_hp(0.0);
        s
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.hp_x1_l = 0.0; self.hp_x2_l = 0.0;
        self.hp_y1_l = 0.0; self.hp_y2_l = 0.0;
        self.hp_x1_r = 0.0; self.hp_x2_r = 0.0;
        self.hp_y1_r = 0.0; self.hp_y2_r = 0.0;
        let bc = self.last_bass_cut;
        self.last_bass_cut = -1.0;
        self.update_hp(bc);
    }

    /// Recompute 2nd-order Butterworth HP biquad coefficients (RBJ cookbook).
    fn update_hp(&mut self, bass_cut: f32) {
        if (bass_cut - self.last_bass_cut).abs() < 1e-5 {
            return;
        }
        self.last_bass_cut = bass_cut;

        if bass_cut <= 0.0 {
            // Bypass: identity filter
            self.hp_b0 = 1.0;
            self.hp_b1 = 0.0;
            self.hp_b2 = 0.0;
            self.hp_a1 = 0.0;
            self.hp_a2 = 0.0;
            return;
        }

        // Map 0..1 → 20 Hz..400 Hz (linear)
        let freq = 20.0 + bass_cut.clamp(0.0, 1.0) * 380.0;
        let w0 = 2.0 * PI * freq / self.sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        // Q = sqrt(2)/2 for Butterworth 2nd order
        let q = std::f32::consts::SQRT_2 * 0.5;
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 + cos_w0) * 0.5;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        self.hp_b0 = b0 / a0;
        self.hp_b1 = b1 / a0;
        self.hp_b2 = b2 / a0;
        self.hp_a1 = a1 / a0;
        self.hp_a2 = a2 / a0;
    }

    #[inline(always)]
    fn hp_process(&mut self, x_l: f32, x_r: f32) -> (f32, f32) {
        let y_l = self.hp_b0 * x_l + self.hp_b1 * self.hp_x1_l + self.hp_b2 * self.hp_x2_l
                - self.hp_a1 * self.hp_y1_l - self.hp_a2 * self.hp_y2_l;
        self.hp_x2_l = self.hp_x1_l; self.hp_x1_l = x_l;
        self.hp_y2_l = self.hp_y1_l; self.hp_y1_l = y_l;

        let y_r = self.hp_b0 * x_r + self.hp_b1 * self.hp_x1_r + self.hp_b2 * self.hp_x2_r
                - self.hp_a1 * self.hp_y1_r - self.hp_a2 * self.hp_y2_r;
        self.hp_x2_r = self.hp_x1_r; self.hp_x1_r = x_r;
        self.hp_y2_r = self.hp_y1_r; self.hp_y1_r = y_r;

        (y_l, y_r)
    }

    /// Process one stereo sample.
    ///
    /// - `bass_cut`        0..1   — HP filter amount (0 = bypass, 1 = 400 Hz)
    /// - `width`           0..2   — stereo width (0 = mono, 1 = unity, 2 = extra wide)
    /// - `limit_threshold` 0..1   — soft limiter ceiling (maps to 0.5..1.0 linear)
    /// - `mix`             0..1   — wet/dry
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        bass_cut: f32,
        width: f32,
        limit_threshold: f32,
        mix: f32,
    ) -> (f32, f32) {
        self.update_hp(bass_cut);

        // 1. HP filter
        let (hp_l, hp_r) = if bass_cut > 0.0 {
            self.hp_process(in_l, in_r)
        } else {
            (in_l, in_r)
        };

        // 2. M/S width
        let m = (hp_l + hp_r) * 0.5;
        let s = (hp_l - hp_r) * 0.5;
        let s_wide = s * width.clamp(0.0, 2.0);
        let wide_l = m + s_wide;
        let wide_r = m - s_wide;

        // 3. Soft limiter — tanh(x/thresh)*thresh
        let thresh = 0.5 + limit_threshold.clamp(0.0, 1.0) * 0.5;
        let lim_l = (wide_l / thresh).tanh() * thresh;
        let lim_r = (wide_r / thresh).tanh() * thresh;

        let mix = mix.clamp(0.0, 1.0);
        let out_l = in_l + mix * (lim_l - in_l);
        let out_r = in_r + mix * (lim_r - in_r);
        (out_l, out_r)
    }
}
