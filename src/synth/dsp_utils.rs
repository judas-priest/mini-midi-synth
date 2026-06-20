//! Shared DSP utility functions: fast math approximations, DC blocker, etc.

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

}

/// Advance a normalized phase [0, 1) by rate_hz / sample_rate.
#[inline(always)]
pub fn advance_phase(phase: &mut f32, rate_hz: f32, sample_rate: f32) {
    *phase += rate_hz / sample_rate;
    if *phase >= 1.0 {
        *phase -= 1.0;
    }
}


// ---------------------------------------------------------------------------
// Circular buffer interpolated reads
// ---------------------------------------------------------------------------

/// Read from a circular buffer with linear interpolation.
/// `write_pos` is the current write head, `delay` is delay in samples.
#[inline(always)]
pub fn buf_read_linear(buf: &[f32], write_pos: usize, delay: f32) -> f32 {
    let n = buf.len();
    let delay = delay.clamp(1.0, (n - 2) as f32);
    let read_f = write_pos as f32 - delay;
    let read_f = if read_f < 0.0 { read_f + n as f32 } else { read_f };
    let i0 = read_f as usize % n;
    let i1 = (i0 + 1) % n;
    let frac = read_f.fract();
    buf[i0] * (1.0 - frac) + buf[i1] * frac
}

/// Read from a circular buffer with cubic (Hermite) interpolation.
/// `write_pos` is the current write head, `delay` is delay in samples.
#[inline(always)]
pub fn buf_read_cubic(buf: &[f32], write_pos: usize, delay: f32) -> f32 {
    let n = buf.len();
    let delay = delay.clamp(1.0, (n - 3) as f32);
    let pos = write_pos as f32 - delay;
    let pos = if pos < 0.0 { pos + n as f32 } else { pos };
    let idx = pos as usize % n;
    let frac = pos.fract();

    let y0 = buf[(idx + n - 1) % n];
    let y1 = buf[idx];
    let y2 = buf[(idx + 1) % n];
    let y3 = buf[(idx + 2) % n];

    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);

    ((c3 * frac + c2) * frac + c1) * frac + y1
}

// ---------------------------------------------------------------------------
// RBJ Biquad coefficient generators (Audio EQ Cookbook)
// Returns (b0, b1, b2, a1, a2) — a0-normalized.
// ---------------------------------------------------------------------------

/// RBJ low-shelf filter coefficients.
pub fn rbj_low_shelf(freq: f32, gain_db: f32, sr: f32) -> (f32, f32, f32, f32, f32) {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * freq / sr;
    let (sin_w, cos_w) = w0.sin_cos();
    let alpha = sin_w / 2.0 * 2.0_f32.sqrt();
    let two_sa = 2.0 * a.sqrt() * alpha;

    let a0 = (a + 1.0) + (a - 1.0) * cos_w + two_sa;
    (
        (a * ((a + 1.0) - (a - 1.0) * cos_w + two_sa)) / a0,
        (2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w)) / a0,
        (a * ((a + 1.0) - (a - 1.0) * cos_w - two_sa)) / a0,
        (-2.0 * ((a - 1.0) + (a + 1.0) * cos_w)) / a0,
        ((a + 1.0) + (a - 1.0) * cos_w - two_sa) / a0,
    )
}

/// RBJ high-shelf filter coefficients.
pub fn rbj_high_shelf(freq: f32, gain_db: f32, sr: f32) -> (f32, f32, f32, f32, f32) {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * freq / sr;
    let (sin_w, cos_w) = w0.sin_cos();
    let alpha = sin_w / 2.0 * 2.0_f32.sqrt();
    let two_sa = 2.0 * a.sqrt() * alpha;

    let a0 = (a + 1.0) - (a - 1.0) * cos_w + two_sa;
    (
        (a * ((a + 1.0) + (a - 1.0) * cos_w + two_sa)) / a0,
        (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w)) / a0,
        (a * ((a + 1.0) + (a - 1.0) * cos_w - two_sa)) / a0,
        (2.0 * ((a - 1.0) - (a + 1.0) * cos_w)) / a0,
        ((a + 1.0) - (a - 1.0) * cos_w - two_sa) / a0,
    )
}

/// RBJ peaking EQ filter coefficients.
pub fn rbj_peaking(freq: f32, gain_db: f32, q: f32, sr: f32) -> (f32, f32, f32, f32, f32) {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * freq / sr;
    let (sin_w, cos_w) = w0.sin_cos();
    let alpha = sin_w / (2.0 * q);

    let a0 = 1.0 + alpha / a;
    (
        (1.0 + alpha * a) / a0,
        (-2.0 * cos_w) / a0,
        (1.0 - alpha * a) / a0,
        (-2.0 * cos_w) / a0,
        (1.0 - alpha / a) / a0,
    )
}
