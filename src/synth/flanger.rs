/// Flanger: short modulated delay line with feedback.

use std::f32::consts::PI;
use super::dsp_utils::advance_phase;

const FLANGER_SIZE: usize = 2048; // ~42ms at 48kHz
const FLANGER_MASK: usize = FLANGER_SIZE - 1;

pub struct Flanger {
    buf_l: [f32; FLANGER_SIZE],
    buf_r: [f32; FLANGER_SIZE],
    write_pos: usize,
    lfo_phase: f32,
    sample_rate: f32,
    pub rate: f32,      // Hz (0.05..5.0)
    pub depth: f32,     // 0..1
    pub feedback: f32,  // -0.95..0.95
    pub delay_ms: f32,  // base delay in ms (1..10)
    pub mix: f32,
}

impl Flanger {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            buf_l: [0.0; FLANGER_SIZE],
            buf_r: [0.0; FLANGER_SIZE],
            write_pos: 0,
            lfo_phase: 0.0,
            sample_rate,
            rate: 0.3, depth: 0.5, feedback: 0.5, delay_ms: 3.0, mix: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.buf_l = [0.0; FLANGER_SIZE];
        self.buf_r = [0.0; FLANGER_SIZE];
    }

    /// Hermite cubic interpolation for smoother delay line reading.
    #[inline(always)]
    fn hermite(buf: &[f32; FLANGER_SIZE], pos: f32) -> f32 {
        let i = pos.floor() as isize;
        let frac = pos - i as f32;
        let idx = |offset: isize| -> f32 {
            buf[((i + offset) as usize) & FLANGER_MASK]
        };
        let xm1 = idx(-1);
        let x0 = idx(0);
        let x1 = idx(1);
        let x2 = idx(2);
        let c = (x1 - xm1) * 0.5;
        let v = x0 - x1;
        let w = c + v;
        let a = w + v + (x2 - x0) * 0.5;
        let b = w + a;
        ((a * frac - b) * frac + c) * frac + x0
    }

    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        if self.mix < 0.001 { return (in_l, in_r); }

        // LFO
        let lfo = (self.lfo_phase * 2.0 * PI).sin();
        advance_phase(&mut self.lfo_phase, self.rate, self.sample_rate);

        let base_samples = self.delay_ms * 0.001 * self.sample_rate;
        let mod_samples = base_samples * self.depth * lfo;
        let delay = (base_samples + mod_samples).max(1.0);

        let read_pos = self.write_pos as f32 - delay + FLANGER_SIZE as f32;
        let dl = Self::hermite(&self.buf_l, read_pos);
        let dr = Self::hermite(&self.buf_r, read_pos);

        self.buf_l[self.write_pos] = in_l + dl * self.feedback;
        self.buf_r[self.write_pos] = in_r + dr * self.feedback;
        self.write_pos = (self.write_pos + 1) & FLANGER_MASK;

        let m = self.mix;
        (in_l * (1.0 - m) + dl * m, in_r * (1.0 - m) + dr * m)
    }
}
