//! Ring Modulator effect — 4-diode bridge simulation with carrier oscillator.
//! Algorithm inspired by Surge XT RingModulatorEffect.

use std::f32::consts::TAU;

pub struct RingMod {
    sample_rate: f32,
    carrier_phase: f32,
}

impl RingMod {
    pub fn new(sample_rate: f32) -> Self {
        Self { sample_rate, carrier_phase: 0.0 }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
    }

    /// Diode simulation: piecewise transfer function.
    /// Region 1 (|v| < fwdbias): quadratic onset
    /// Region 2 (|v| >= fwdbias): linear with offset
    #[inline(always)]
    fn diode_sim(v: f32, fwdbias: f32, linregion: f32) -> f32 {
        let h = if linregion < 0.001 { 0.001 } else { linregion };
        if v.abs() < fwdbias {
            // Quadratic region
            let x = v / (fwdbias + 0.0001);
            x * x * v.signum() * h
        } else {
            // Linear region
            let sign = v.signum();
            let excess = v.abs() - fwdbias;
            sign * (h * fwdbias * fwdbias / (fwdbias + 0.0001) + excess)
        }
    }

    /// Generate carrier sample based on shape.
    /// 0=Sine, 1=Saw, 2=Square
    #[inline(always)]
    fn carrier_sample(phase: f32, shape: u8) -> f32 {
        match shape {
            1 => 2.0 * phase - 1.0,                          // Saw
            2 => if phase < 0.5 { 1.0 } else { -1.0 },      // Square
            _ => (phase * TAU).sin(),                         // Sine
        }
    }

    /// Soft saturation: 1.5x - 0.5x³
    #[inline(always)]
    fn soft_sat(x: f32) -> f32 {
        let c = x.clamp(-1.5, 1.5);
        1.5 * c - 0.5 * c * c * c
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self, in_l: f32, in_r: f32,
        carrier_freq: f32, carrier_shape: f32,
        diode_fwdbias: f32, diode_linregion: f32, mix: f32,
    ) -> (f32, f32) {
        if mix < 0.001 { return (in_l, in_r); }

        // Advance carrier
        let dt = carrier_freq / self.sample_rate;
        self.carrier_phase += dt;
        if self.carrier_phase >= 1.0 { self.carrier_phase -= 1.0; }
        let carrier = Self::carrier_sample(self.carrier_phase, carrier_shape as u8);

        // 4-diode bridge per channel
        let bias = diode_fwdbias.clamp(0.0, 2.0);
        let lin = diode_linregion.clamp(0.01, 2.0);

        let process = |input: f32| -> f32 {
            let d_pa = Self::diode_sim(input + carrier, bias, lin);
            let d_ma = Self::diode_sim(-input + carrier, bias, lin);
            let d_pb = Self::diode_sim(input - carrier, bias, lin);
            let d_mb = Self::diode_sim(-input - carrier, bias, lin);
            let raw = d_pa + d_ma - d_pb - d_mb;
            Self::soft_sat(raw * 0.5)
        };

        // Compute carrier-only bleed (what the bridge outputs with zero input)
        // and subtract it — simulates transformer balance in analog ring mods.
        let bleed = process(0.0);
        let wet_l = process(in_l) - bleed;
        let wet_r = process(in_r) - bleed;
        let m = mix;
        (in_l * (1.0 - m) + wet_l * m, in_r * (1.0 - m) + wet_r * m)
    }
}
