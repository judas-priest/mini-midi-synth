/// Juno-style stereo chorus effect.

use std::f32::consts::TAU;

const BUFFER_SIZE: usize = 4096;
const LFO_RATE1: f32 = 0.513;
const LFO_RATE2: f32 = 0.863;
const DELAY_CENTER_MS: f32 = 7.0;
const DELAY_DEPTH_MS: f32 = 3.0;

pub struct Chorus {
    buffer: Vec<f32>,
    write_pos: usize,
    lfo_phase1: f32,
    lfo_phase2: f32,
    sample_rate: f32,
}

impl Chorus {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            buffer: vec![0.0; BUFFER_SIZE],
            write_pos: 0,
            lfo_phase1: 0.0,
            lfo_phase2: 0.25,
            sample_rate,
        }
    }

    /// Process mono input, return (left, right) stereo output.
    pub fn tick(&mut self, input: f32, mix: f32) -> (f32, f32) {
        self.buffer[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % BUFFER_SIZE;

        if mix < 0.001 {
            return (input, input);
        }

        let lfo1 = (self.lfo_phase1 * TAU).sin();
        let lfo2 = (self.lfo_phase2 * TAU).sin();
        self.lfo_phase1 += LFO_RATE1 / self.sample_rate;
        if self.lfo_phase1 >= 1.0 {
            self.lfo_phase1 -= 1.0;
        }
        self.lfo_phase2 += LFO_RATE2 / self.sample_rate;
        if self.lfo_phase2 >= 1.0 {
            self.lfo_phase2 -= 1.0;
        }

        let center = DELAY_CENTER_MS * 0.001 * self.sample_rate;
        let depth = DELAY_DEPTH_MS * 0.001 * self.sample_rate;

        let delay_l = center + depth * lfo1;
        let delay_r = center + depth * lfo2;

        let wet_l = self.read_cubic(delay_l);
        let wet_r = self.read_cubic(delay_r);

        let dry = 1.0 - mix;
        (input * dry + wet_l * mix, input * dry + wet_r * mix)
    }

    fn read_cubic(&self, delay: f32) -> f32 {
        let pos = self.write_pos as f32 - delay;
        let pos = if pos < 0.0 {
            pos + BUFFER_SIZE as f32
        } else {
            pos
        };
        let idx = pos as usize % BUFFER_SIZE;
        let frac = pos.fract();

        let n = BUFFER_SIZE;
        let y0 = self.buffer[(idx + n - 1) % n];
        let y1 = self.buffer[idx];
        let y2 = self.buffer[(idx + 1) % n];
        let y3 = self.buffer[(idx + 2) % n];

        let c1 = 0.5 * (y2 - y0);
        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);

        ((c3 * frac + c2) * frac + c1) * frac + y1
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.buffer = vec![0.0; BUFFER_SIZE];
        self.write_pos = 0;
    }
}
