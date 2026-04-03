/// LFO oscillator with multiple waveforms.

#[derive(Clone, Copy, PartialEq)]
pub enum LfoWaveform {
    Sine,
    Triangle,
    Square,
    SampleHold,
}

impl LfoWaveform {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            1 => Self::Triangle,
            2 => Self::Square,
            3 => Self::SampleHold,
            _ => Self::Sine,
        }
    }
}

#[derive(Clone)]
pub struct Lfo {
    phase: f32,
    sample_rate: f32,
    sh_value: f32,
    sh_smooth: f32,
    noise_state: u32,
    prev_phase_int: u32,
}

impl Lfo {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            sh_value: 0.0,
            sh_smooth: 0.0,
            noise_state: 0xCAFEBABE,
            prev_phase_int: 0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
    }

    /// Returns -1.0..+1.0
    pub fn tick(&mut self, rate: f32, waveform: LfoWaveform) -> f32 {
        let dt = rate / self.sample_rate;
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        match waveform {
            LfoWaveform::Sine => (self.phase * std::f32::consts::TAU).sin(),
            LfoWaveform::Triangle => {
                if self.phase < 0.5 {
                    4.0 * self.phase - 1.0
                } else {
                    3.0 - 4.0 * self.phase
                }
            }
            LfoWaveform::Square => {
                if self.phase < 0.5 { 1.0 } else { -1.0 }
            }
            LfoWaveform::SampleHold => {
                let phase_int = self.phase as u32;
                if phase_int != self.prev_phase_int || self.phase < dt * 1.5 {
                    self.prev_phase_int = phase_int;
                    // xorshift32
                    self.noise_state ^= self.noise_state << 13;
                    self.noise_state ^= self.noise_state >> 17;
                    self.noise_state ^= self.noise_state << 5;
                    self.sh_value =
                        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                }
                // Slew limiter: rate-dependent (~25% of LFO period for transition)
                let coeff = (rate * 4.0 / self.sample_rate).clamp(0.0001, 0.5);
                self.sh_smooth += coeff * (self.sh_value - self.sh_smooth);
                self.sh_smooth
            }
        }
    }
}
