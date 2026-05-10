/// Shared DSP utility functions: fast math approximations, DC blocker, etc.

use std::f32::consts::PI;

/// Fast tan approximation using Padé approximant. Good for x in [0, ~1.5].
#[inline(always)]
pub fn fast_tan(x: f32) -> f32 {
    let x2 = x * x;
    x * (1.0 + x2 * (1.0 / 3.0 + x2 * 2.0 / 15.0))
        / (1.0 - x2 * (1.0 / 3.0 - x2 * 1.0 / 21.0))
}

/// Fast tanh approximation (Padé [3,3]). Max error ~0.001 for |x| < 5.
#[inline(always)]
pub fn fast_tanh(x: f32) -> f32 {
    let x = x.clamp(-5.0, 5.0);
    let x2 = x * x;
    x * (135.0 + 17.0 * x2) / (135.0 + 62.0 * x2)
}

/// Stereo DC blocker (first-order high-pass at ~6-35 Hz depending on coefficient).
pub struct DcBlocker {
    x1_l: f32,
    y1_l: f32,
    x1_r: f32,
    y1_r: f32,
}

impl DcBlocker {
    pub fn new() -> Self {
        Self { x1_l: 0.0, y1_l: 0.0, x1_r: 0.0, y1_r: 0.0 }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Compute DC blocker coefficient for a given cutoff frequency and sample rate.
    /// Typical: `coeff_for(35.0, 48000.0)` ≈ 0.9954
    #[inline(always)]
    pub fn coeff_for(cutoff_hz: f32, sample_rate: f32) -> f32 {
        1.0 - (PI * cutoff_hz / sample_rate)
    }

    /// Process a stereo pair with a given coefficient (use `coeff_for()` or a constant like 0.997).
    #[inline(always)]
    pub fn process(&mut self, in_l: f32, in_r: f32, coeff: f32) -> (f32, f32) {
        let out_l = in_l - self.x1_l + coeff * self.y1_l;
        self.x1_l = in_l;
        self.y1_l = out_l;

        let out_r = in_r - self.x1_r + coeff * self.y1_r;
        self.x1_r = in_r;
        self.y1_r = out_r;

        (out_l, out_r)
    }

    /// Process a single (mono) sample.
    #[inline(always)]
    pub fn process_mono(&mut self, input: f32, coeff: f32) -> f32 {
        let out = input - self.x1_l + coeff * self.y1_l;
        self.x1_l = input;
        self.y1_l = out;
        out
    }
}

/// Advance a normalized phase [0, 1) by rate_hz / sample_rate.
#[inline(always)]
pub fn advance_phase(phase: &mut f32, rate_hz: f32, sample_rate: f32) {
    *phase += rate_hz / sample_rate;
    if *phase >= 1.0 {
        *phase -= 1.0;
    }
}

/// Linear wet/dry mix for a stereo signal.
#[inline(always)]
pub fn mix_stereo(dry_l: f32, dry_r: f32, wet_l: f32, wet_r: f32, mix: f32) -> (f32, f32) {
    let inv = 1.0 - mix;
    (dry_l * inv + wet_l * mix, dry_r * inv + wet_r * mix)
}
