#![allow(dead_code)]
/// Wave Shaper — multi-mode waveshaping distortion.
/// Modes: Tanh, HardClip, Asymmetric, SinFold, TriFold, Digital, Diode, Rectify
///
/// Inspired by Surge XT's Wave Shaper effect.
use std::f32::consts::PI;

pub struct WaveShaper {
    sample_rate: f32,
    // DC blocker state (1-pole HP at ~6 Hz) — per channel
    dc_x1_l: f32,
    dc_y1_l: f32,
    dc_x1_r: f32,
    dc_y1_r: f32,
    dc_coeff: f32, // R = 1 - 2*pi*f/fs
}

pub struct WaveShaperMode;

impl WaveShaperMode {
    pub fn from_param(v: f32) -> u32 {
        (v.clamp(0.0, 1.0) * 7.0).round() as u32
    }
}

#[inline(always)]
fn fast_tanh(x: f32) -> f32 {
    let x = x.clamp(-5.0, 5.0);
    let x2 = x * x;
    x * (135.0 + 17.0 * x2) / (135.0 + 62.0 * x2)
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
    fn shape(x: f32, mode: u32, drive_lin: f32) -> f32 {
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
