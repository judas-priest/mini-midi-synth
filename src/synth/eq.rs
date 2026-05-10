/// 3-band parametric EQ using RBJ biquad filters.
/// Low shelf + parametric mid + high shelf.

use super::dsp_utils::{rbj_low_shelf, rbj_high_shelf, rbj_peaking};

#[derive(Clone, Copy)]
struct Biquad {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    s1: f32, s2: f32, // Direct Form II Transposed state
}

impl Biquad {
    fn new() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, s1: 0.0, s2: 0.0 }
    }

    fn set_low_shelf(&mut self, freq: f32, gain_db: f32, sr: f32) {
        (self.b0, self.b1, self.b2, self.a1, self.a2) = rbj_low_shelf(freq, gain_db, sr);
    }

    fn set_high_shelf(&mut self, freq: f32, gain_db: f32, sr: f32) {
        (self.b0, self.b1, self.b2, self.a1, self.a2) = rbj_high_shelf(freq, gain_db, sr);
    }

    fn set_peaking(&mut self, freq: f32, gain_db: f32, q: f32, sr: f32) {
        (self.b0, self.b1, self.b2, self.a1, self.a2) = rbj_peaking(freq, gain_db, q, sr);
    }

    #[inline(always)]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }

    fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }
}

pub struct ParametricEq {
    low_l: Biquad, low_r: Biquad,
    mid_l: Biquad, mid_r: Biquad,
    high_l: Biquad, high_r: Biquad,
    sample_rate: f32,
    dirty: bool,
    // Parameters
    pub low_freq: f32,   // Hz
    pub low_gain: f32,   // dB
    pub mid_freq: f32,
    pub mid_gain: f32,
    pub mid_q: f32,
    pub high_freq: f32,
    pub high_gain: f32,
    pub mix: f32,
}

impl ParametricEq {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            low_l: Biquad::new(), low_r: Biquad::new(),
            mid_l: Biquad::new(), mid_r: Biquad::new(),
            high_l: Biquad::new(), high_r: Biquad::new(),
            sample_rate, dirty: true,
            low_freq: 200.0, low_gain: 0.0,
            mid_freq: 1000.0, mid_gain: 0.0, mid_q: 1.0,
            high_freq: 5000.0, high_gain: 0.0,
            mix: 0.0,
        }
    }

    fn update_coefficients(&mut self) {
        let sr = self.sample_rate;
        self.low_l.set_low_shelf(self.low_freq, self.low_gain, sr);
        self.low_r = self.low_l;
        self.low_r.reset();
        self.mid_l.set_peaking(self.mid_freq, self.mid_gain, self.mid_q, sr);
        self.mid_r = self.mid_l;
        self.mid_r.reset();
        self.high_l.set_high_shelf(self.high_freq, self.high_gain, sr);
        self.high_r = self.high_l;
        self.high_r.reset();
        self.dirty = false;
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.dirty = true;
    }

    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        if self.mix < 0.001 { return (in_l, in_r); }
        if self.dirty { self.update_coefficients(); }
        let wl = self.low_l.tick(self.mid_l.tick(self.high_l.tick(in_l)));
        let wr = self.low_r.tick(self.mid_r.tick(self.high_r.tick(in_r)));
        let m = self.mix;
        (in_l * (1.0 - m) + wl * m, in_r * (1.0 - m) + wr * m)
    }
}
