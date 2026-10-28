//! Bonsai saturation — asymmetric multi-mode soft clipper with tone shaping.
//!
//! Inspired by Surge XT's Bonsai effect. Provides several saturation modes,
//! adjustable asymmetry, and a pre/post tone filter.

use std::f32::consts::PI;
use super::dsp_utils::fast_tanh;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BonsaiMode {
    Soft,     // 0 — gentle tanh saturation
    Hard,     // 1 — harder clipping with knee
    Fuzz,     // 2 — asymmetric diode-style fuzz
    Fold,     // 3 — wavefolder
}

impl BonsaiMode {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            1 => Self::Hard,
            2 => Self::Fuzz,
            3 => Self::Fold,
            _ => Self::Soft,
        }
    }
}

pub struct Bonsai {
    sample_rate: f32,
    // Post-filter (lowpass tone)
    post_lp: (f32, f32),
    // DC blocker
    dc_x: (f32, f32),
    dc_y: (f32, f32),
}

impl Bonsai {
    pub fn new(sr: f32) -> Self {
        Self {
            sample_rate: sr,
            post_lp: (0.0, 0.0),
            dc_x: (0.0, 0.0),
            dc_y: (0.0, 0.0),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
    }

    /// Process stereo input.
    /// `drive` (0..1): amount of saturation (0=clean, 1=heavy).
    /// `tone` (0..1): post-saturation tone (0=dark, 1=bright).
    /// `asym` (0..1): asymmetry (0=symmetric, 1=asymmetric).
    /// `mode` (0..3): saturation character.
    /// `mix` (0..1): wet/dry.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        in_l: f32, in_r: f32,
        drive: f32, tone: f32, asym: f32, mode: BonsaiMode, mix: f32,
    ) -> (f32, f32) {
        if mix < 0.001 {
            return (in_l, in_r);
        }

        let sr = self.sample_rate;

        // Drive: 1..32x
        let drive_lin = (drive * 4.0).exp2(); // 1..16x gain
        let dl = in_l * drive_lin;
        let dr = in_r * drive_lin;

        // Saturation
        let (sl, sr_s) = Self::saturate(dl, dr, asym, mode);

        // Gain compensation (approximately)
        let comp = 1.0 / drive_lin.sqrt().max(1.0);
        let sl = sl * comp;
        let sr_s = sr_s * comp;

        // Tone filter: blend LP cutoff from 1kHz (dark) to 12kHz (bright)
        let lp_freq = 1000.0 * (tone * 3.5).exp2(); // 1kHz..12kHz
        let lp_coef = 1.0 - (-2.0 * PI * lp_freq / sr).exp();
        self.post_lp.0 += lp_coef * (sl - self.post_lp.0);
        self.post_lp.1 += lp_coef * (sr_s - self.post_lp.1);

        // DC blocker (6 Hz HP) — post-saturation to remove DC offset from asymmetric clipping
        let dc_coef = 1.0 - (-2.0 * PI * 6.0 / sr).exp();
        let (bl, br) = Self::dc_block(&mut self.dc_x, &mut self.dc_y, self.post_lp.0, self.post_lp.1, dc_coef);

        let dry = 1.0 - mix;
        (in_l * dry + bl * mix, in_r * dry + br * mix)
    }

    fn dc_block(
        x: &mut (f32, f32), y: &mut (f32, f32),
        in_l: f32, in_r: f32, coef: f32,
    ) -> (f32, f32) {
        // y[n] = x[n] - x[n-1] + (1-coef)*y[n-1]
        let yl = in_l - x.0 + (1.0 - coef) * y.0;
        let yr = in_r - x.1 + (1.0 - coef) * y.1;
        x.0 = in_l; x.1 = in_r;
        y.0 = yl;   y.1 = yr;
        (yl, yr)
    }

    fn saturate(l: f32, r: f32, asym: f32, mode: BonsaiMode) -> (f32, f32) {
        (Self::sat_mono(l, asym, mode), Self::sat_mono(r, asym, mode))
    }

    fn sat_mono(x: f32, asym: f32, mode: BonsaiMode) -> f32 {
        match mode {
            BonsaiMode::Soft => {
                // Asymmetric tanh: positive clips softer, negative clips harder
                let pos_drive = 1.0 + asym * 1.5;
                let neg_drive = 1.0 + asym * 0.5;
                if x >= 0.0 {
                    fast_tanh(x * pos_drive) / pos_drive
                } else {
                    fast_tanh(x * neg_drive) / neg_drive
                }
            }
            BonsaiMode::Hard => {
                // Hard clip with soft knee
                let knee = 0.7 - asym * 0.2;
                if x.abs() < knee {
                    x
                } else {
                    let sign = x.signum();
                    let excess = (x.abs() - knee) / (1.0 - knee);
                    let soft = knee + (1.0 - knee) * fast_tanh(excess * 3.0) / 3.0;
                    sign * soft * (1.0 + asym * 0.3)
                }
            }
            BonsaiMode::Fuzz => {
                // Diode-style asymmetric fuzz
                if x >= 0.0 {
                    // Hard clip positive (forward diode)
                    (1.0 - (-x * (2.0 + asym * 3.0)).exp()).min(1.0)
                } else {
                    // Soft negative
                    fast_tanh(x * (0.5 + asym * 0.5))
                }
            }
            BonsaiMode::Fold => {
                // Wavefolder with asymmetric bias, DC-compensated
                let bias = asym * 0.3;
                fold(x + bias) - fold(bias)
            }
        }
    }
}

/// Simple wavefolder: reflects signal back at ±1.
fn fold(x: f32) -> f32 {
    // Fold into [-1, 1] with triangle wave reflection
    let x = x * 0.5 + 0.5; // shift to [0,1]
    let x = x - x.floor();  // wrap to [0,1)
    let x = if x < 0.5 { x * 2.0 } else { 2.0 - x * 2.0 }; // triangle
    x * 2.0 - 1.0 // shift back to [-1,1]
}

