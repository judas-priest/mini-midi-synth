//! MS Tool — Mid/Side processor.
//! Encodes stereo to M/S, applies per-channel gain, optionally rotates the
//! M/S plane, then decodes back to L/R.
//!
//! Inspired by Surge XT's MS Tool effect.
use std::f32::consts::PI;

pub struct MsTool {
    pub sample_rate: f32,
}

impl MsTool {
    pub fn new(sample_rate: f32) -> Self {
        Self { sample_rate }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
    }

    /// Process one stereo sample.
    ///
    /// - `mid_gain`  0..2  — Mid channel gain (1.0 = unity)
    /// - `side_gain` 0..2  — Side channel gain (1.0 = unity)
    /// - `rotation`  -1..1 — Stereo rotation; 0 = no rotation
    /// - `mix`       0..1  — Wet/dry blend
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        mid_gain: f32,
        side_gain: f32,
        rotation: f32,
        mix: f32,
    ) -> (f32, f32) {
        // Encode to M/S
        let m = (in_l + in_r) * 0.5;
        let s = (in_l - in_r) * 0.5;

        // Apply per-channel gain
        let m_gained = m * mid_gain.clamp(0.0, 2.0);
        let s_gained = s * side_gain.clamp(0.0, 2.0);

        // Rotation in the M/S plane
        let angle = rotation.clamp(-1.0, 1.0) * (PI * 0.5);
        let (sin_a, cos_a) = angle.sin_cos();
        let m_rot = m_gained * cos_a - s_gained * sin_a;
        let s_rot = m_gained * sin_a + s_gained * cos_a;

        // Decode back to L/R
        let wet_l = m_rot + s_rot;
        let wet_r = m_rot - s_rot;

        let mix = mix.clamp(0.0, 1.0);
        let out_l = in_l + mix * (wet_l - in_l);
        let out_r = in_r + mix * (wet_r - in_r);
        (out_l, out_r)
    }
}
