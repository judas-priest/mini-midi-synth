//! Bitcrusher: bit depth reduction + sample rate reduction.
//! Includes anti-aliasing lowpass filter and triangular PDF dithering.

pub struct Bitcrusher {
    held_l: f32,
    held_r: f32,
    counter: f32,
    sample_rate: f32,
    pub bits: f32,          // 1..16
    pub downsample: f32,    // target rate in Hz (100..48000)
    pub mix: f32,
    aa_state_l: f32,
    aa_state_r: f32,
    noise_state: u32,
}

impl Bitcrusher {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            held_l: 0.0, held_r: 0.0, counter: 0.0,
            sample_rate,
            bits: 16.0, downsample: 48000.0, mix: 0.0,
            aa_state_l: 0.0, aa_state_r: 0.0,
            noise_state: 0xDEADBEEF,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.held_l = 0.0;
        self.held_r = 0.0;
        self.counter = 0.0;
        self.aa_state_l = 0.0;
        self.aa_state_r = 0.0;
    }

    /// Xorshift32 PRNG, returns value in -1..1.
    #[inline(always)]
    fn next_noise(&mut self) -> f32 {
        self.noise_state ^= self.noise_state << 13;
        self.noise_state ^= self.noise_state >> 17;
        self.noise_state ^= self.noise_state << 5;
        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        if self.mix < 0.001 { return (in_l, in_r); }

        // Anti-aliasing lowpass filter before sample-rate reduction
        // Cutoff at target_sr / 2, mapped to one-pole coefficient
        let aa_coeff = (std::f32::consts::PI * self.downsample * 0.5 / self.sample_rate).min(0.99);
        self.aa_state_l += aa_coeff * (in_l - self.aa_state_l);
        self.aa_state_r += aa_coeff * (in_r - self.aa_state_r);

        // Sample rate reduction
        self.counter += self.downsample / self.sample_rate;
        if self.counter >= 1.0 {
            self.counter -= 1.0;
            // Bit depth reduction with triangular PDF dithering
            let levels = 2.0_f32.powf(self.bits);
            // Triangular dither: sum of two uniform random values
            let dither_l = (self.next_noise() + self.next_noise()) * 0.5 / levels;
            let dither_r = (self.next_noise() + self.next_noise()) * 0.5 / levels;
            self.held_l = ((self.aa_state_l + dither_l) * levels).round() / levels;
            self.held_r = ((self.aa_state_r + dither_r) * levels).round() / levels;
        }

        let m = self.mix;
        (in_l * (1.0 - m) + self.held_l * m,
         in_r * (1.0 - m) + self.held_r * m)
    }
}
