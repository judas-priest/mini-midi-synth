#![allow(dead_code)]
/// Feed-forward compressor with peak detection, attack/release, knee.

pub struct Compressor {
    envelope: f32,
    sample_rate: f32,
    pub threshold: f32,  // dB (-60..0)
    pub ratio: f32,      // 1..20 (20 = limiter)
    attack: f32,         // seconds (0.001..0.1)
    release: f32,        // seconds (0.01..1.0)
    attack_coeff: f32,
    release_coeff: f32,
    pub makeup: f32,     // dB (0..24)
    pub mix: f32,
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Self {
        let attack = 0.001_f32;
        let release = 0.08_f32;
        let mut s = Self {
            envelope: 0.0,
            sample_rate,
            threshold: -6.0,
            ratio: 20.0,
            attack,
            release,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            makeup: 0.0,
            mix: 1.0,
        };
        s.update_coeffs();
        s
    }

    fn update_coeffs(&mut self) {
        self.attack_coeff  = 1.0 - (-1.0 / (self.attack  * self.sample_rate)).exp();
        self.release_coeff = 1.0 - (-1.0 / (self.release * self.sample_rate)).exp();
    }

    pub fn set_attack(&mut self, v: f32) {
        self.attack = v;
        self.update_coeffs();
    }

    pub fn set_release(&mut self, v: f32) {
        self.release = v;
        self.update_coeffs();
    }

    pub fn attack(&self) -> f32 { self.attack }
    pub fn release(&self) -> f32 { self.release }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.envelope = 0.0;
        self.update_coeffs();
    }

    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        if self.mix < 0.001 { return (in_l, in_r); }

        // Peak detection
        let input_level = in_l.abs().max(in_r.abs());

        // Envelope follower
        let coeff = if input_level > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope += coeff * (input_level - self.envelope);

        // Gain computation in dB
        let env_db = if self.envelope > 0.00001 {
            20.0 * self.envelope.log10()
        } else {
            -100.0
        };

        // Soft-knee gain computation (6dB knee width)
        let knee_width: f32 = 6.0;
        let knee_half = knee_width * 0.5;
        let over = env_db - self.threshold;
        let gain_reduction_db = if over < -knee_half {
            // Below knee — no compression
            0.0
        } else if over < knee_half {
            // In knee region — quadratic interpolation
            let t = over + knee_half;
            (1.0 / self.ratio - 1.0) * t * t / (2.0 * knee_width)
        } else {
            // Above knee — full compression
            (1.0 / self.ratio - 1.0) * over
        };
        let gain = 10.0_f32.powf((gain_reduction_db + self.makeup) / 20.0);

        let wl = in_l * gain;
        let wr = in_r * gain;
        let m = self.mix;
        (in_l * (1.0 - m) + wl * m, in_r * (1.0 - m) + wr * m)
    }
}
