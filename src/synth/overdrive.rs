//! Overdrive/Distortion with pre-filter, waveshaping, tone control.
//! Includes simple 2x oversampling to reduce aliasing.

use super::dsp_utils::DcBlocker;

/// Cheap soft-clip: x / (1 + |x|)
#[inline(always)]
fn soft_clip(x: f32) -> f32 {
    x / (1.0 + x.abs())
}

/// Asymmetric tube-like distortion
#[inline(always)]
fn tube_clip(x: f32, drive: f32) -> f32 {
    let g = x * drive;
    if g >= 0.0 {
        1.0 - (-g).exp()
    } else {
        -(1.0 - (g * 0.6).exp())
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum DistortionType {
    SoftClip,  // 0
    Tube,      // 1
    HardClip,  // 2
    Fuzz,      // 3
}

impl DistortionType {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            0 => Self::SoftClip,
            1 => Self::Tube,
            2 => Self::HardClip,
            3 => Self::Fuzz,
            _ => Self::SoftClip,
        }
    }
}

pub struct Overdrive {
    // Pre HP filter state (remove sub-bass before distortion)
    hp_l: f32,
    hp_r: f32,
    prev_l: f32,
    prev_r: f32,
    // Tone LP filter state (post-distortion)
    tone_l: f32,
    tone_r: f32,
    dc: DcBlocker,
    // 2x oversampling: previous input for interpolation
    os_prev_l: f32,
    os_prev_r: f32,
    sample_rate: f32,
    pub drive: f32,        // 1.0..50.0
    pub tone: f32,         // 0.0..1.0 (LP cutoff: 0=dark, 1=bright)
    pub dist_type: DistortionType,
    pub mix: f32,
}

impl Overdrive {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            hp_l: 0.0, hp_r: 0.0, prev_l: 0.0, prev_r: 0.0,
            tone_l: 0.0, tone_r: 0.0,
            dc: DcBlocker::new(),
            os_prev_l: 0.0, os_prev_r: 0.0,
            sample_rate,
            drive: 1.0, tone: 0.5, dist_type: DistortionType::SoftClip, mix: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.hp_l = 0.0; self.hp_r = 0.0;
        self.prev_l = 0.0; self.prev_r = 0.0;
        self.tone_l = 0.0; self.tone_r = 0.0;
        self.dc.reset();
        self.os_prev_l = 0.0; self.os_prev_r = 0.0;
    }

    #[inline(always)]
    fn waveshape(&self, x: f32) -> f32 {
        match self.dist_type {
            DistortionType::SoftClip => soft_clip(x * self.drive),
            DistortionType::Tube => tube_clip(x, self.drive),
            DistortionType::HardClip => (x * self.drive).clamp(-1.0, 1.0),
            DistortionType::Fuzz => {
                let clipped = (x * self.drive).clamp(-1.0, 1.0);
                // Mix in octave-up (full-wave rectification)
                clipped * 0.7 + (x * self.drive).abs().min(1.0) * 0.3
            }
        }
    }

    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        if self.mix < 0.001 { return (in_l, in_r); }

        let hp_coeff = (2.0 * std::f32::consts::PI * 80.0 / self.sample_rate).min(0.5);
        let tone_coeff = 0.02 + 0.4 * self.tone;
        let tone_param = self.tone;

        // Left channel — 2x oversampled waveshaping
        self.hp_l += hp_coeff * (in_l - self.hp_l);
        let hp_out_l = in_l - self.hp_l;
        let mid_l = (hp_out_l + self.os_prev_l) * 0.5; // interpolated mid-sample
        self.os_prev_l = hp_out_l;
        let shaped_l1 = self.waveshape(hp_out_l);
        let shaped_l2 = self.waveshape(mid_l);
        let shaped_l = (shaped_l1 + shaped_l2) * 0.5;  // decimate
        self.tone_l += tone_coeff * (shaped_l - self.tone_l);
        let toned_l = self.tone_l * (1.0 - tone_param * 0.3) + shaped_l * tone_param * 0.3;

        // Right channel — 2x oversampled waveshaping
        self.hp_r += hp_coeff * (in_r - self.hp_r);
        let hp_out_r = in_r - self.hp_r;
        let mid_r = (hp_out_r + self.os_prev_r) * 0.5;
        self.os_prev_r = hp_out_r;
        let shaped_r1 = self.waveshape(hp_out_r);
        let shaped_r2 = self.waveshape(mid_r);
        let shaped_r = (shaped_r1 + shaped_r2) * 0.5;
        self.tone_r += tone_coeff * (shaped_r - self.tone_r);
        let toned_r = self.tone_r * (1.0 - tone_param * 0.3) + shaped_r * tone_param * 0.3;

        let (dc_out_l, dc_out_r) = self.dc.process(toned_l, toned_r, 0.997);

        let m = self.mix;
        (in_l * (1.0 - m) + dc_out_l * m, in_r * (1.0 - m) + dc_out_r * m)
    }
}
