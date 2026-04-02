/// State-variable filter (SVF) — lowpass, highpass, bandpass.

use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq)]
pub enum FilterType {
    LowPass,
    HighPass,
    BandPass,
    Formant,
}

impl FilterType {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            0 => Self::LowPass,
            1 => Self::HighPass,
            2 => Self::BandPass,
            _ => Self::Formant,
        }
    }
}

#[derive(Clone)]
pub struct Filter {
    filter_type: FilterType,
    cutoff: f32,
    resonance: f32,
    sample_rate: f32,
    ic1eq: f32,
    ic2eq: f32,
    // Cached coefficients — recomputed only when cutoff/resonance change
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    dirty: bool,
}

impl Filter {
    pub fn new(sample_rate: f32) -> Self {
        let mut f = Self {
            filter_type: FilterType::LowPass,
            cutoff: 8000.0,
            resonance: 0.0,
            sample_rate,
            ic1eq: 0.0,
            ic2eq: 0.0,
            g: 0.0,
            k: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            dirty: true,
        };
        f.update_coefficients();
        f
    }

    pub fn set_type(&mut self, ft: FilterType) {
        self.filter_type = ft;
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        let clamped = cutoff.clamp(20.0, self.sample_rate * 0.49);
        if clamped != self.cutoff {
            self.cutoff = clamped;
            self.dirty = true;
        }
    }

    pub fn set_resonance(&mut self, res: f32) {
        let clamped = res.clamp(0.0, 1.0);
        if clamped != self.resonance {
            self.resonance = clamped;
            self.dirty = true;
        }
    }

    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    fn update_coefficients(&mut self) {
        // SVF from Andrew Simper / Cytomic
        // Fast tan approximation: accurate enough for audio, avoids expensive libm tan()
        let x = PI * self.cutoff / self.sample_rate;
        self.g = fast_tan(x);
        self.k = 2.0 - 2.0 * self.resonance;
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
        self.dirty = false;
    }

    pub fn tick(&mut self, input: f32) -> f32 {
        if self.dirty {
            self.update_coefficients();
        }

        let v3 = input - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;

        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        match self.filter_type {
            FilterType::LowPass => v2,
            FilterType::HighPass => input - self.k * v1 - v2,
            FilterType::BandPass | FilterType::Formant => v1,
        }
    }
}

/// Fast tan approximation using Padé approximant. Good for x in [0, ~1.5].
#[inline(always)]
fn fast_tan(x: f32) -> f32 {
    let x2 = x * x;
    x * (1.0 + x2 * (1.0 / 3.0 + x2 * 2.0 / 15.0))
        / (1.0 - x2 * (1.0 / 3.0 - x2 * 1.0 / 21.0))
}
