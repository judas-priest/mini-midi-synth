/// Exciter — harmonic exciter for adding brightness and "air".
///
/// Algorithm:
///   1. Split the input with a 1-pole HP filter above a tunable frequency
///      (1 kHz..8 kHz) to isolate high-frequency content.
///   2. Drive the high-frequency signal through a soft nonlinearity (tanh)
///      to generate new harmonics.
///   3. HP-filter the harmonics again to strip any sub content they produced.
///   4. Optionally boost a gentle presence shelf around the exciter frequency.
///   5. Mix the generated harmonics back with the dry signal.
///
/// Inspired by Surge XT's Exciter effect.
use std::f32::consts::PI;

pub struct Exciter {
    sample_rate: f32,
    // First HP filter (frequency split) — left and right
    hp1_l_x1: f32,
    hp1_l_y1: f32,
    hp1_r_x1: f32,
    hp1_r_y1: f32,
    // Second HP filter (harmonic cleanup) — left and right
    hp2_l_x1: f32,
    hp2_l_y1: f32,
    hp2_r_x1: f32,
    hp2_r_y1: f32,
    // Presence low-shelf state (used as a boost around exciter freq)
    ps_l_x1: f32,
    ps_l_y1: f32,
    ps_r_x1: f32,
    ps_r_y1: f32,
    // Cached 1-pole HP coefficient
    hp_coeff: f32,    // a = exp(-2*pi*f/fs), y = a*y_prev + a*(x - x_prev)
    last_freq: f32,
}

impl Exciter {
    pub fn new(sample_rate: f32) -> Self {
        let mut s = Self {
            sample_rate,
            hp1_l_x1: 0.0, hp1_l_y1: 0.0,
            hp1_r_x1: 0.0, hp1_r_y1: 0.0,
            hp2_l_x1: 0.0, hp2_l_y1: 0.0,
            hp2_r_x1: 0.0, hp2_r_y1: 0.0,
            ps_l_x1: 0.0, ps_l_y1: 0.0,
            ps_r_x1: 0.0, ps_r_y1: 0.0,
            hp_coeff: 0.0,
            last_freq: -1.0,
        };
        s.update_coeffs(0.5);
        s
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.hp1_l_x1 = 0.0; self.hp1_l_y1 = 0.0;
        self.hp1_r_x1 = 0.0; self.hp1_r_y1 = 0.0;
        self.hp2_l_x1 = 0.0; self.hp2_l_y1 = 0.0;
        self.hp2_r_x1 = 0.0; self.hp2_r_y1 = 0.0;
        self.ps_l_x1 = 0.0; self.ps_l_y1 = 0.0;
        self.ps_r_x1 = 0.0; self.ps_r_y1 = 0.0;
        let f = self.last_freq;
        self.last_freq = -1.0;
        self.update_coeffs(f);
    }

    fn update_coeffs(&mut self, freq_param: f32) {
        if (freq_param - self.last_freq).abs() < 1e-5 {
            return;
        }
        self.last_freq = freq_param;
        // Map 0..1 → 1000 Hz..8000 Hz (exponential)
        let freq = 1000.0 * (8.0_f32).powf(freq_param.clamp(0.0, 1.0));
        // 1-pole HP: a = exp(-2*pi*f/fs)
        self.hp_coeff = (-2.0 * PI * freq / self.sample_rate).exp();
    }

    /// 1-pole HP filter: y[n] = a*(y[n-1] + x[n] - x[n-1])
    #[inline(always)]
    fn hp1_l(&mut self, x: f32) -> f32 {
        let a = self.hp_coeff;
        let y = a * (self.hp1_l_y1 + x - self.hp1_l_x1);
        self.hp1_l_x1 = x;
        self.hp1_l_y1 = y + 1e-30;
        y
    }

    #[inline(always)]
    fn hp1_r(&mut self, x: f32) -> f32 {
        let a = self.hp_coeff;
        let y = a * (self.hp1_r_y1 + x - self.hp1_r_x1);
        self.hp1_r_x1 = x;
        self.hp1_r_y1 = y + 1e-30;
        y
    }

    #[inline(always)]
    fn hp2_l(&mut self, x: f32) -> f32 {
        let a = self.hp_coeff;
        let y = a * (self.hp2_l_y1 + x - self.hp2_l_x1);
        self.hp2_l_x1 = x;
        self.hp2_l_y1 = y + 1e-30;
        y
    }

    #[inline(always)]
    fn hp2_r(&mut self, x: f32) -> f32 {
        let a = self.hp_coeff;
        let y = a * (self.hp2_r_y1 + x - self.hp2_r_x1);
        self.hp2_r_x1 = x;
        self.hp2_r_y1 = y + 1e-30;
        y
    }

    /// Simple 1-pole LP used as a presence shelf smoother.
    /// Returns LP output; HP = input - LP is used as the "presence boost".
    #[inline(always)]
    fn presence_l(&mut self, x: f32, coeff: f32) -> f32 {
        let y = self.ps_l_y1 + coeff * (x - self.ps_l_y1);
        self.ps_l_y1 = y + 1e-30;
        // presence shelf = original + boost*(x - lp) = boosted mid shelf
        y
    }

    #[inline(always)]
    fn presence_r(&mut self, x: f32, coeff: f32) -> f32 {
        let y = self.ps_r_y1 + coeff * (x - self.ps_r_y1);
        self.ps_r_y1 = y + 1e-30;
        y
    }

    /// Process one stereo sample.
    ///
    /// - `drive`    0..1 — harmonics generation amount
    /// - `freq`     0..1 — exciter threshold frequency (1 kHz..8 kHz)
    /// - `presence` 0..1 — presence boost around the exciter frequency
    /// - `mix`      0..1 — wet/dry (0 = dry, 1 = harmonics fully added)
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        drive: f32,
        freq: f32,
        presence: f32,
        mix: f32,
    ) -> (f32, f32) {
        self.update_coeffs(freq);

        let drive = drive.clamp(0.0, 1.0);
        let presence = presence.clamp(0.0, 1.0);
        let mix = mix.clamp(0.0, 1.0);

        // 1. HP split — extract high-frequency content
        let hf_l = self.hp1_l(in_l);
        let hf_r = self.hp1_r(in_r);

        // 2. Nonlinear distortion — generate harmonics
        // drive 0..1 maps to gain 1..8
        let gain = 1.0 + drive * 7.0;
        let dist_l = (hf_l * gain).tanh();
        let dist_r = (hf_r * gain).tanh();

        // 3. Second HP pass — remove sub content generated by the nonlinearity
        let harm_l = self.hp2_l(dist_l);
        let harm_r = self.hp2_r(dist_r);

        // 4. Presence boost — gentle shelf around the exciter frequency.
        //    LP coeff shifted one octave below exciter freq.
        let pres_cutoff = 1000.0 * (8.0_f32).powf(freq.clamp(0.0, 1.0)) * 0.5;
        let pres_coeff = 1.0 - (-2.0 * std::f32::consts::PI * pres_cutoff / self.sample_rate).exp();
        // HP component = x - LP(x) = the "presence" region
        let pres_shelf_l = in_l - self.presence_l(in_l, pres_coeff);
        let pres_shelf_r = in_r - self.presence_r(in_r, pres_coeff);
        let presence_l = in_l + presence * pres_shelf_l;
        let presence_r = in_r + presence * pres_shelf_r;

        // 5. Mix harmonics into the (presence-boosted) dry signal
        let out_l = presence_l + mix * harm_l;
        let out_r = presence_r + mix * harm_r;
        (out_l, out_r)
    }
}
