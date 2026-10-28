//! Wave Shaper — multi-mode waveshaping distortion.
//! Modes: Tanh, HardClip, Asymmetric, SinFold, TriFold, Digital, Diode, Rectify,
//!        Harm2, Harm3, Harm4, Harm5,
//!        Softfold, Singlefold, Dualfold, WestCoast,
//!        FuzzSoft, FuzzHeavy, FuzzCenter, FuzzEdge, FuzzSoftEdge, FuzzRect,
//!        Sin+x, Sin2x+x, Atan
//!
//! Inspired by Surge XT's Wave Shaper effect.
use std::f32::consts::PI;
use super::dsp_utils::fast_tanh;

pub struct WaveShaper {
    sample_rate: f32,
    // DC blocker state (1-pole HP at ~6 Hz) — per channel
    dc_x1_l: f32,
    dc_y1_l: f32,
    dc_x1_r: f32,
    dc_y1_r: f32,
    dc_coeff: f32, // R = 1 - 2*pi*f/fs
}


/// Triangle wave fold: reflects x to stay in [-1, 1] with tri-fold shape.
#[inline(always)]
fn tri_fold(x: f32) -> f32 {
    // Period 4, fold at ±1
    let x = x - 4.0 * ((x + 1.0) * 0.25).floor();
    if x > 1.0 {
        2.0 - x
    } else if x < -1.0 {
        -2.0 - x
    } else {
        x
    }
}

/// Per-voice oscillator waveshaper — mono sample, no DC blocker (caller handles it).
/// `mode` 0..24 selects waveshaping character (matches WaveShaper modes).
/// `drive` is a linear pre-gain (e.g. 0.5..4.5 mapped from a 0..1 param).
#[inline(always)]
pub fn shape_sample(input: f32, mode: u32, drive: f32) -> f32 {
    WaveShaper::shape(input, mode, drive)
}

impl WaveShaper {
    pub fn new(sample_rate: f32) -> Self {
        let dc_coeff = 1.0 - (2.0 * PI * 6.0 / sample_rate);
        Self {
            sample_rate,
            dc_x1_l: 0.0,
            dc_y1_l: 0.0,
            dc_x1_r: 0.0,
            dc_y1_r: 0.0,
            dc_coeff,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.dc_coeff = 1.0 - (2.0 * PI * 6.0 / sr);
        self.dc_x1_l = 0.0;
        self.dc_y1_l = 0.0;
        self.dc_x1_r = 0.0;
        self.dc_y1_r = 0.0;
    }

    #[inline(always)]
    fn dc_block(&mut self, x_l: f32, x_r: f32) -> (f32, f32) {
        let r = self.dc_coeff;
        let y_l = x_l - self.dc_x1_l + r * self.dc_y1_l;
        let y_r = x_r - self.dc_x1_r + r * self.dc_y1_r;
        self.dc_x1_l = x_l;
        self.dc_y1_l = y_l;
        self.dc_x1_r = x_r;
        self.dc_y1_r = y_r;
        (y_l, y_r)
    }

    #[inline(always)]
    pub fn shape(x: f32, mode: u32, drive_lin: f32) -> f32 {
        let gained = x * drive_lin;
        match mode {
            // 0: Tanh — classic soft saturation
            0 => fast_tanh(gained),
            // 1: HardClip
            1 => gained.clamp(-1.0, 1.0),
            // 2: Asymmetric — positive tanh, negative hard clip
            2 => {
                if gained >= 0.0 {
                    fast_tanh(gained)
                } else {
                    gained.max(-1.0)
                }
            }
            // 3: SinFold — sine wavefolder
            3 => (gained * PI).sin(),
            // 4: TriFold — triangle wave reflection at ±1
            4 => tri_fold(gained),
            // 5: Digital — floor quantization (bit-crush-style)
            5 => {
                let steps = drive_lin.max(1.0);
                (gained * steps).floor() / steps
            }
            // 6: Diode — half-wave rectifier style
            6 => (1.0 - (-gained * 4.0).exp()).max(0.0),
            // 7: Rectify — full-wave rectification
            7 => gained.abs(),

            // Mode 8-11: Chebyshev Harmonic Shapers
            // 8: Harm2 — T2(x) = 2x²-1
            8 => {
                let x = gained.clamp(-1.0, 1.0);
                2.0 * x * x - 1.0
            }
            // 9: Harm3 — T3(x) = 4x³-3x
            9 => {
                let x = gained.clamp(-1.0, 1.0);
                4.0 * x * x * x - 3.0 * x
            }
            // 10: Harm4 — T4(x) = 8x⁴-8x²+1
            10 => {
                let x = gained.clamp(-1.0, 1.0);
                let x2 = x * x;
                8.0 * x2 * x2 - 8.0 * x2 + 1.0
            }
            // 11: Harm5 — T5(x) = 16x⁵-20x³+5x
            11 => {
                let x = gained.clamp(-1.0, 1.0);
                let x2 = x * x;
                16.0 * x2 * x2 * x - 20.0 * x2 * x + 5.0 * x
            }

            // Mode 12-15: Wavefolders
            // 12: Softfold — fold with soft knee via tanh
            12 => {
                let folded = if gained.abs() < 1.0 { gained } else { 2.0 - gained.abs() };
                fast_tanh(folded * 2.0)
            }
            // 13: Singlefold — fold once at ±1
            13 => {
                if gained > 1.0 { 2.0 - gained }
                else if gained < -1.0 { -2.0 - gained }
                else { gained }
            }
            // 14: Dualfold — fold at ±0.5 and ±1.5
            14 => {
                let y = (gained + 1.0).rem_euclid(4.0) - 2.0;
                if y > 1.0 { 2.0 - y } else if y < -1.0 { -2.0 - y } else { y }
            }
            // 15: WestCoast — Buchla-style sine folder
            15 => {
                let x = gained * 0.5;
                (x * PI).sin()
            }

            // Mode 16-21: Fuzz variants
            // 16: FuzzSoft — gentle transistor fuzz
            16 => gained / (1.0 + gained.abs()),
            // 17: FuzzHeavy — hard transistor fuzz
            17 => {
                let x = gained * 2.0;
                fast_tanh(x * 3.0)
            }
            // 18: FuzzCenter — symmetric hard clip with soft knee
            18 => {
                let thresh = 0.7_f32;
                if gained.abs() < thresh {
                    gained
                } else {
                    gained.signum() * (thresh + (gained.abs() - thresh) / (1.0 + (gained.abs() - thresh)))
                }
            }
            // 19: FuzzEdge — asymmetric fuzz (positive harder)
            19 => {
                if gained >= 0.0 { fast_tanh(gained * 4.0) }
                else { gained / (1.0 - gained * 0.5) }
            }
            // 20: FuzzSoftEdge — very gentle saturation
            20 => gained * (1.0 + gained.abs()).recip().sqrt(),
            // 21: FuzzRect — asymmetric rectifier fuzz
            21 => fast_tanh(gained + gained.abs() * 0.5),

            // Mode 22-24: Trigonometric
            // 22: Sin+x — sin(x)+x normalized
            22 => (gained.sin() + gained) * 0.5,
            // 23: Sin2x+x
            23 => ((2.0 * gained).sin() + gained) * 0.5,
            // 24: Atan — smooth limiter
            24 => gained.atan() * std::f32::consts::FRAC_2_PI,

            _ => gained.clamp(-1.0, 1.0),
        }
    }

    /// Process one stereo sample.
    /// `drive` 0..1 maps to 1x..32x pre-gain.
    /// `mode` 0..7 selects waveshaping character.
    /// `bias` -1..1 applies DC offset before shaping.
    /// `mix` 0..1 wet/dry.
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        drive: f32,
        mode: u32,
        bias: f32,
        mix: f32,
    ) -> (f32, f32) {
        // drive 0..1 => 1x..32x (exponential)
        let drive_lin = (drive.clamp(0.0, 1.0) * 5.0).exp2(); // 2^(drive*5): 1..32

        let biased_l = in_l + bias;
        let biased_r = in_r + bias;

        let shaped_l = Self::shape(biased_l, mode, drive_lin);
        let shaped_r = Self::shape(biased_r, mode, drive_lin);

        // DC block to remove offset from asymmetric modes
        let (clean_l, clean_r) = self.dc_block(shaped_l, shaped_r);

        let mix = mix.clamp(0.0, 1.0);
        let out_l = in_l + mix * (clean_l - in_l);
        let out_r = in_r + mix * (clean_r - in_r);
        (out_l, out_r)
    }
}
