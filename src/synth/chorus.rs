//! Juno-style stereo chorus effect.

use super::dsp_utils::{buf_read_cubic, advance_phase};

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
    /// BBD-style lowpass filter state (one-pole per channel)
    bbd_lp_l: f32,
    bbd_lp_r: f32,
}

impl Chorus {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            buffer: vec![0.0; BUFFER_SIZE],
            write_pos: 0,
            lfo_phase1: 0.0,
            lfo_phase2: 0.25,
            sample_rate,
            bbd_lp_l: 0.0,
            bbd_lp_r: 0.0,
        }
    }

    /// Process mono input, return (left, right) stereo output.
    pub fn tick(&mut self, input: f32, mix: f32) -> (f32, f32) {
        self.buffer[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % BUFFER_SIZE;

        if mix < 0.001 {
            return (input, input);
        }

        // Triangle LFO for authentic Juno character (more linear sweep than sine)
        let lfo1 = 1.0 - 4.0 * (self.lfo_phase1 - 0.5).abs();  // triangle wave [-1,1]
        let lfo2 = 1.0 - 4.0 * (self.lfo_phase2 - 0.5).abs();
        advance_phase(&mut self.lfo_phase1, LFO_RATE1, self.sample_rate);
        advance_phase(&mut self.lfo_phase2, LFO_RATE2, self.sample_rate);

        let center = DELAY_CENTER_MS * 0.001 * self.sample_rate;
        let depth = DELAY_DEPTH_MS * 0.001 * self.sample_rate;

        let delay_l = center + depth * lfo1;
        let delay_r = center + depth * lfo2;

        let mut wet_l = buf_read_cubic(&self.buffer, self.write_pos, delay_l);
        let mut wet_r = buf_read_cubic(&self.buffer, self.write_pos, delay_r);

        // BBD lowpass emulation (~10kHz) — one-pole per channel, SR-aware
        let bbd_coeff = (2.0 * std::f32::consts::PI * 10000.0 / self.sample_rate).min(0.95);
        wet_l = self.bbd_lp_l + bbd_coeff * (wet_l - self.bbd_lp_l);
        self.bbd_lp_l = wet_l;
        wet_r = self.bbd_lp_r + bbd_coeff * (wet_r - self.bbd_lp_r);
        self.bbd_lp_r = wet_r;

        let dry = 1.0 - mix;
        (input * dry + wet_l * mix, input * dry + wet_r * mix)
    }


    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.buffer = vec![0.0; BUFFER_SIZE];
        self.write_pos = 0;
        self.bbd_lp_l = 0.0;
        self.bbd_lp_r = 0.0;
    }
}
