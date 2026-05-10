/// Phaser: chain of first-order allpass filters with LFO-modulated frequency.

use std::f32::consts::PI;

const NUM_STAGES: usize = 12;

pub struct Phaser {
    ap_state_l: [f32; NUM_STAGES],
    ap_state_r: [f32; NUM_STAGES],
    lfo_phase: f32,
    sample_rate: f32,
    pub rate: f32,      // Hz (0.05..5.0)
    pub depth: f32,     // 0..1
    pub feedback: f32,  // -0.9..0.9
    pub mix: f32,
    feedback_l: f32,
    feedback_r: f32,
}

impl Phaser {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            ap_state_l: [0.0; NUM_STAGES],
            ap_state_r: [0.0; NUM_STAGES],
            lfo_phase: 0.0,
            sample_rate,
            rate: 0.5, depth: 0.5, feedback: 0.3, mix: 0.0,
            feedback_l: 0.0, feedback_r: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.ap_state_l = [0.0; NUM_STAGES];
        self.ap_state_r = [0.0; NUM_STAGES];
        self.feedback_l = 0.0;
        self.feedback_r = 0.0;
    }

    #[inline(always)]
    fn allpass_tick(input: f32, state: &mut f32, coeff: f32) -> f32 {
        let y = coeff * input + *state;
        *state = input - coeff * y;
        y
    }

    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        if self.mix < 0.001 { return (in_l, in_r); }

        // LFO
        let lfo = (self.lfo_phase * 2.0 * PI).sin();
        self.lfo_phase += self.rate / self.sample_rate;
        if self.lfo_phase >= 1.0 { self.lfo_phase -= 1.0; }

        // Map LFO to frequency range (100Hz - 8kHz, logarithmic)
        let min_freq = 100.0_f32;
        let max_freq = 8000.0_f32;
        let freq = min_freq * (max_freq / min_freq).powf((lfo * self.depth + 1.0) * 0.5);
        let coeff = {
            let t = (PI * freq / self.sample_rate).tan();
            (t - 1.0) / (t + 1.0)
        };

        // Process through allpass chain
        let mut xl = in_l + self.feedback_l * self.feedback;
        let mut xr = in_r + self.feedback_r * self.feedback;
        for i in 0..NUM_STAGES {
            xl = Self::allpass_tick(xl, &mut self.ap_state_l[i], coeff);
            xr = Self::allpass_tick(xr, &mut self.ap_state_r[i], coeff);
        }
        self.feedback_l = xl;
        self.feedback_r = xr;

        let m = self.mix;
        (in_l * (1.0 - m) + (in_l + xl) * 0.5 * m,
         in_r * (1.0 - m) + (in_r + xr) * 0.5 * m)
    }
}
