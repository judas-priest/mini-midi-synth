/// Tremolo: amplitude modulation by LFO. Stereo mode offsets L/R phase.

use std::f32::consts::PI;

pub struct Tremolo {
    phase: f32,
    sample_rate: f32,
    pub rate: f32,      // Hz (1..15)
    pub depth: f32,     // 0..1
    pub stereo: f32,    // 0..1 (0=mono, 1=full stereo offset)
    pub mix: f32,
}

impl Tremolo {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0, sample_rate,
            rate: 4.0, depth: 0.5, stereo: 0.0, mix: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
    }

    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        if self.mix < 0.001 { return (in_l, in_r); }

        let lfo_l = ((self.phase * 2.0 * PI).sin() + 1.0) * 0.5; // 0..1
        let phase_r = self.phase + 0.25 * self.stereo; // offset for stereo
        let lfo_r = ((phase_r * 2.0 * PI).sin() + 1.0) * 0.5;

        self.phase += self.rate / self.sample_rate;
        if self.phase >= 1.0 { self.phase -= 1.0; }

        let gain_l = 1.0 - self.depth * (1.0 - lfo_l);
        let gain_r = 1.0 - self.depth * (1.0 - lfo_r);

        let m = self.mix;
        (in_l * (1.0 - m) + in_l * gain_l * m,
         in_r * (1.0 - m) + in_r * gain_r * m)
    }
}
