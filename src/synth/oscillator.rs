/// Oscillator with multiple waveform types.

use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq)]
pub enum OscType {
    Sine,
    Saw,
    Square,
    Triangle,
    /// Simple 2-op FM
    Fm,
}

impl OscType {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            0 => Self::Sine,
            1 => Self::Saw,
            2 => Self::Square,
            3 => Self::Triangle,
            _ => Self::Fm,
        }
    }
}

#[derive(Clone)]
pub struct Oscillator {
    pub osc_type: OscType,
    phase: f32,
    mod_phase: f32,
    sample_rate: f32,
    detune: f32,
    pub fm_ratio: f32,
    pub fm_index: f32,
}

impl Oscillator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            osc_type: OscType::Sine,
            phase: 0.0,
            mod_phase: 0.0,
            sample_rate,
            detune: 0.0,
            fm_ratio: 3.5,
            fm_index: 5.0,
        }
    }

    pub fn set_detune(&mut self, detune: f32) {
        self.detune = detune;
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.mod_phase = 0.0;
    }

    pub fn tick(&mut self, freq: f32) -> f32 {
        let freq = freq * (1.0 + self.detune);
        let dt = freq / self.sample_rate;

        let out = match self.osc_type {
            OscType::Sine => (self.phase * 2.0 * PI).sin(),
            OscType::Saw => 2.0 * self.phase - 1.0,
            OscType::Square => {
                if self.phase < 0.5 { 1.0 } else { -1.0 }
            }
            OscType::Triangle => {
                if self.phase < 0.5 {
                    4.0 * self.phase - 1.0
                } else {
                    3.0 - 4.0 * self.phase
                }
            }
            OscType::Fm => {
                let modulator = (self.mod_phase * 2.0 * PI).sin();
                let out = ((self.phase + self.fm_index * modulator) * 2.0 * PI).sin();
                self.mod_phase += freq * self.fm_ratio / self.sample_rate;
                self.mod_phase -= self.mod_phase.floor();
                out
            }
        };

        self.phase += dt;
        self.phase -= self.phase.floor();

        out
    }
}
