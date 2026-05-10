/// Graphic EQ — 11-band parametric equalizer.
///
/// Fixed center frequencies: 31, 62, 125, 250, 500, 1k, 2k, 4k, 8k, 16k Hz
/// plus one additional band at 10 kHz (high shelf style via peak).
/// All bands are peak filters except band 0 (low shelf at 31 Hz) and
/// band 10 (high shelf at 16 kHz).  Gains range from -12 to +12 dB.
///
/// Uses Robert Bristow-Johnson Audio EQ Cookbook biquad coefficients.
///
/// Inspired by Surge XT's Graphic EQ effect.
use super::dsp_utils::{rbj_low_shelf, rbj_high_shelf, rbj_peaking};

/// Center frequencies for the 11 bands (Hz).
const BAND_FREQS: [f32; 11] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0,
                                2000.0, 4000.0, 8000.0, 10000.0, 16000.0];

/// Q for all peak bands.
const BAND_Q: f32 = 1.41;

#[derive(Clone, Copy, Default)]
pub struct BandState {
    // Normalised coefficients (a0 divided out)
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
    // Per-channel biquad state
    pub x1_l: f32,
    pub x2_l: f32,
    pub y1_l: f32,
    pub y2_l: f32,
    pub x1_r: f32,
    pub x2_r: f32,
    pub y1_r: f32,
    pub y2_r: f32,
}

impl BandState {
    /// Compute peak EQ biquad coefficients (RBJ cookbook).
    fn set_peak(freq: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let (b0, b1, b2, a1, a2) = rbj_peaking(freq, gain_db, q, sample_rate);
        Self { b0, b1, b2, a1, a2, ..Default::default() }
    }

    fn set_low_shelf(freq: f32, gain_db: f32, sample_rate: f32) -> Self {
        let (b0, b1, b2, a1, a2) = rbj_low_shelf(freq, gain_db, sample_rate);
        Self { b0, b1, b2, a1, a2, ..Default::default() }
    }

    fn set_high_shelf(freq: f32, gain_db: f32, sample_rate: f32) -> Self {
        let (b0, b1, b2, a1, a2) = rbj_high_shelf(freq, gain_db, sample_rate);
        Self { b0, b1, b2, a1, a2, ..Default::default() }
    }

    #[inline(always)]
    fn process_l(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1_l + self.b2 * self.x2_l
              - self.a1 * self.y1_l - self.a2 * self.y2_l;
        self.x2_l = self.x1_l;
        self.x1_l = x;
        self.y2_l = self.y1_l;
        self.y1_l = y + 1e-30;
        y
    }

    #[inline(always)]
    fn process_r(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1_r + self.b2 * self.x2_r
              - self.a1 * self.y1_r - self.a2 * self.y2_r;
        self.x2_r = self.x1_r;
        self.x1_r = x;
        self.y2_r = self.y1_r;
        self.y1_r = y + 1e-30;
        y
    }
}

pub struct GraphicEq {
    sample_rate: f32,
    bands: [BandState; 11],
    last_gains: [f32; 11],
}

impl GraphicEq {
    pub fn new(sample_rate: f32) -> Self {
        let mut eq = Self {
            sample_rate,
            bands: [BandState::default(); 11],
            last_gains: [0.0; 11],
        };
        eq.rebuild_all(&[0.0; 11]);
        eq
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        let gains = self.last_gains;
        self.rebuild_all(&gains);
    }

    fn rebuild_all(&mut self, gains: &[f32; 11]) {
        for (i, &gain_db) in gains.iter().enumerate() {
            let freq = BAND_FREQS[i];
            let gain_db = gain_db.clamp(-12.0, 12.0);
            self.bands[i] = if i == 0 {
                BandState::set_low_shelf(freq, gain_db, self.sample_rate)
            } else if i == 10 {
                BandState::set_high_shelf(freq, gain_db, self.sample_rate)
            } else {
                BandState::set_peak(freq, gain_db, BAND_Q, self.sample_rate)
            };
        }
        self.last_gains = *gains;
    }

    /// Process one stereo sample.
    ///
    /// `gains` — per-band gain in dB (-12..+12), length 11.
    /// `output_gain` — linear output gain applied after all bands.
    ///
    /// Call this once per block (not per sample) when gains change; the
    /// coefficients are recomputed only when the gains array differs from
    /// the last call.
    #[allow(dead_code)]
    pub fn update_gains(&mut self, gains: &[f32; 11]) {
        if gains != &self.last_gains {
            self.rebuild_all(gains);
        }
    }

    pub fn tick(&mut self, in_l: f32, in_r: f32, gains: &[f32; 11], output_gain: f32) -> (f32, f32) {
        // Lazily recompute coefficients if gains changed
        if gains != &self.last_gains {
            self.rebuild_all(gains);
        }

        let mut l = in_l;
        let mut r = in_r;
        for band in self.bands.iter_mut() {
            l = band.process_l(l);
            r = band.process_r(r);
        }
        (l * output_gain, r * output_gain)
    }
}
