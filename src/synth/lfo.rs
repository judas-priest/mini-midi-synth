/// LFO oscillator with multiple waveforms and deform parameter.

use std::sync::OnceLock;

static SINE_TABLE: OnceLock<Box<[f32; 2048]>> = OnceLock::new();

fn sine_lut(phase: f32) -> f32 {
    let table = SINE_TABLE.get_or_init(|| {
        let mut t = Box::new([0.0f32; 2048]);
        for i in 0..2048 {
            t[i] = (i as f32 / 2048.0 * std::f32::consts::TAU).sin();
        }
        t
    });
    let idx = (phase * 2048.0) as usize & 2047;
    table[idx]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LfoWaveform {
    Sine,
    Triangle,
    Square,
    SampleHold,
    Sawtooth,
    Envelope,
    Noise,
    SmoothNoise,
}

impl LfoWaveform {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            1 => Self::Triangle,
            2 => Self::Square,
            3 => Self::SampleHold,
            4 => Self::Sawtooth,
            5 => Self::Envelope,
            6 => Self::Noise,
            7 => Self::SmoothNoise,
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
    // Envelope mode state
    env_stage: u8,       // 0=idle, 1=attack, 2=decay, 3=sustain, 4=release
    env_value: f32,
    #[allow(dead_code)]
    env_triggered: bool,
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
            env_stage: 0,
            env_value: 0.0,
            env_triggered: false,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
    }

    /// Reset LFO phase to 0 (retrigger on note-on). Does not affect noise state.
    pub fn reset_phase(&mut self) {
        self.phase = 0.0;
    }

    /// Trigger envelope mode (one-shot from LFO).
    #[allow(dead_code)]
    pub fn trigger_envelope(&mut self) {
        self.env_stage = 1; // attack
        self.env_triggered = true;
    }

    /// Release envelope mode.
    #[allow(dead_code)]
    pub fn release_envelope(&mut self) {
        if self.env_stage > 0 && self.env_stage < 4 {
            self.env_stage = 4; // release
        }
    }

    /// Apply deform to a base waveform value.
    /// bend1: exponential shift, bend2: sine modulation
    #[inline(always)]
    fn apply_deform(x: f32, deform: f32) -> f32 {
        if deform.abs() < 0.01 { return x; }
        let a = deform;
        if a.abs() < 0.5 {
            // bend1: x = x - a*x^2 + a (exponential shift)
            (x - a * x * x + a).clamp(-1.0, 1.0)
        } else {
            // bend2: x += 4.5*a*sin(2π*x) / (2π) (sine modulation)
            let tau = std::f32::consts::TAU;
            (x + 4.5 * a * sine_lut(x.rem_euclid(1.0)) / tau).clamp(-1.0, 1.0)
        }
    }

    /// Returns -1.0..+1.0
    #[allow(dead_code)]
    pub fn tick(&mut self, rate: f32, waveform: LfoWaveform) -> f32 {
        self.tick_with_deform(rate, waveform, 0.0)
    }

    /// Returns -1.0..+1.0 with deform parameter.
    pub fn tick_with_deform(&mut self, rate: f32, waveform: LfoWaveform, deform: f32) -> f32 {
        let dt = rate / self.sample_rate;
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let raw = match waveform {
            LfoWaveform::Sine => sine_lut(self.phase),
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

                    // Correlated noise when deform > 0: blend toward previous value
                    if deform > 0.01 {
                        let corr = deform.clamp(0.0, 0.95);
                        self.sh_value = self.sh_smooth * corr + self.sh_value * (1.0 - corr);
                    }
                }
                // Slew limiter: rate-dependent (~25% of LFO period for transition)
                let coeff = (rate * 4.0 / self.sample_rate).clamp(0.0001, 0.5);
                self.sh_smooth += coeff * (self.sh_value - self.sh_smooth);
                return self.sh_smooth; // skip deform for S&H (already applied)
            }
            LfoWaveform::Sawtooth => {
                2.0 * self.phase - 1.0
            }
            LfoWaveform::Noise => {
                // xorshift32 — new random value every sample
                self.noise_state ^= self.noise_state << 13;
                self.noise_state ^= self.noise_state >> 17;
                self.noise_state ^= self.noise_state << 5;
                (self.noise_state as i32 as f32) / i32::MAX as f32
            }
            LfoWaveform::SmoothNoise => {
                self.noise_state ^= self.noise_state << 13;
                self.noise_state ^= self.noise_state >> 17;
                self.noise_state ^= self.noise_state << 5;
                let raw = (self.noise_state as i32 as f32) / i32::MAX as f32;
                // deform 0=very smooth (coeff~0.999), 1=less smooth (coeff~0.9)
                let coeff = 0.999 - deform * 0.099;
                self.sh_smooth = self.sh_smooth * coeff + raw * (1.0 - coeff);
                return self.sh_smooth; // skip apply_deform, deform already used for pole
            }
            LfoWaveform::Envelope => {
                // One-shot ADSR driven by LFO rate
                // Attack = 25% of period, decay = 25%, sustain = 0.5 level, release = 50%
                let atk_time = 0.25;
                let dec_time = 0.25;
                let coeff = dt * 4.0; // speed relative to LFO rate

                match self.env_stage {
                    1 => { // Attack
                        self.env_value += coeff / atk_time;
                        if self.env_value >= 1.0 {
                            self.env_value = 1.0;
                            self.env_stage = 2;
                        }
                    }
                    2 => { // Decay
                        self.env_value -= coeff * (self.env_value - 0.5) / dec_time;
                        if self.env_value <= 0.52 {
                            self.env_value = 0.5;
                            self.env_stage = 3;
                        }
                    }
                    3 => { // Sustain — hold
                        self.env_value = 0.5;
                    }
                    4 => { // Release
                        self.env_value -= coeff;
                        if self.env_value <= 0.0 {
                            self.env_value = 0.0;
                            self.env_stage = 0;
                        }
                    }
                    _ => {
                        self.env_value = 0.0;
                    }
                }
                // Map 0..1 to -1..1
                return self.env_value * 2.0 - 1.0;
            }
        };

        Self::apply_deform(raw, deform)
    }
}
