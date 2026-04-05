/// Tape Saturation effect — magnetic hysteresis model with loss filter.
/// Algorithm inspired by Surge XT chowdsp TapeEffect.

use std::f32::consts::PI;

pub struct Tape {
    sample_rate: f32,
    // Hysteresis state (per channel)
    m_l: f32,
    m_r: f32,
    m_prev_l: f32,
    m_prev_r: f32,
    h_prev_l: f32,
    h_prev_r: f32,
    // Loss filter (one-pole LP per channel)
    loss_l: f32,
    loss_r: f32,
    // DC blocker
    dc_l: f32,
    dc_r: f32,
    dc_prev_l: f32,
    dc_prev_r: f32,
    // 2x oversampling: previous input
    os_prev_l: f32,
    os_prev_r: f32,
}

impl Tape {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            m_l: 0.0, m_r: 0.0,
            m_prev_l: 0.0, m_prev_r: 0.0,
            h_prev_l: 0.0, h_prev_r: 0.0,
            loss_l: 0.0, loss_r: 0.0,
            dc_l: 0.0, dc_r: 0.0,
            dc_prev_l: 0.0, dc_prev_r: 0.0,
            os_prev_l: 0.0, os_prev_r: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.m_l = 0.0; self.m_r = 0.0;
        self.m_prev_l = 0.0; self.m_prev_r = 0.0;
        self.h_prev_l = 0.0; self.h_prev_r = 0.0;
        self.loss_l = 0.0; self.loss_r = 0.0;
        self.dc_l = 0.0; self.dc_r = 0.0;
        self.dc_prev_l = 0.0; self.dc_prev_r = 0.0;
        self.os_prev_l = 0.0; self.os_prev_r = 0.0;
    }

    /// Simplified magnetic hysteresis: Langevin-like saturation with history.
    /// Uses RK2 (midpoint method) for the ODE integration.
    #[inline(always)]
    fn hysteresis_step(
        h: f32, m: f32, _m_prev: f32, h_prev: f32,
        m_sat: f32, alpha: f32, k: f32, c: f32, dt_inv: f32,
    ) -> f32 {
        // Effective field
        let h_eff = h + alpha * m;

        // Langevin saturation: M_anhyst = M_s * coth(H_eff/a) - a/H_eff
        // Simplified: tanh-based approximation
        let a = k.max(0.01);
        let m_an = m_sat * (h_eff / a).tanh();

        // Rate of change of H
        let dh_dt = (h - h_prev) * dt_inv;

        // Differential susceptibility
        let delta = if dh_dt >= 0.0 { 1.0 } else { -1.0 };
        let dm_an = m_an - m;

        // Irreversible and reversible components
        let denom = delta * k - alpha * dm_an;
        let dm_irr = if denom.abs() < 0.0001 { 0.0 } else { dm_an / denom };

        // RK2 midpoint
        let dm = ((1.0 - c) * dm_irr + c * (m_an - m) * 0.5) * dh_dt;

        // Clamp the step to prevent blowup
        (m + dm / dt_inv).clamp(-m_sat * 1.5, m_sat * 1.5)
    }

    #[inline]
    pub fn tick(
        &mut self, in_l: f32, in_r: f32,
        drive: f32, saturation: f32, bias: f32,
        tone: f32, speed: f32, mix: f32,
    ) -> (f32, f32) {
        if mix < 0.001 { return (in_l, in_r); }

        let drv = 1.0 + drive * 10.0;
        let m_sat = 1.0 + saturation * 2.0;
        let alpha = 0.01 + saturation * 0.05;
        let k = 0.5 + (1.0 - saturation) * 1.5;
        let c = 0.8 + bias * 0.15;
        let dt_inv = self.sample_rate * 2.0; // 2x oversampled

        // Loss filter coefficient (tape speed affects HF loss)
        let loss_freq = 2000.0 + speed * 16000.0 + tone * 4000.0;
        let loss_coeff = (PI * loss_freq / (self.sample_rate * 2.0)).sin().min(0.999);

        // DC blocker coefficient
        let dc_coeff = 1.0 - (PI * 35.0 / self.sample_rate);

        // Process left channel (2x oversampled)
        let input_l = in_l * drv;
        let mid_l = (input_l + self.os_prev_l) * 0.5;
        self.os_prev_l = input_l;

        // Two hysteresis steps per sample (oversampling)
        self.m_l = Self::hysteresis_step(mid_l, self.m_l, self.m_prev_l, self.h_prev_l, m_sat, alpha, k, c, dt_inv);
        self.m_prev_l = self.m_l;
        self.h_prev_l = mid_l;
        self.m_l = Self::hysteresis_step(input_l, self.m_l, self.m_prev_l, self.h_prev_l, m_sat, alpha, k, c, dt_inv);
        self.m_prev_l = self.m_l;
        self.h_prev_l = input_l;

        // Loss filter
        self.loss_l += loss_coeff * (self.m_l - self.loss_l);
        let wet_l = self.loss_l;

        // Makeup gain (~9dB to compensate saturation)
        let makeup = 2.8 / (1.0 + drive * 1.5);
        let wet_l = wet_l * makeup;

        // DC blocker
        let dc_out_l = wet_l - self.dc_prev_l + dc_coeff * self.dc_l;
        self.dc_prev_l = wet_l;
        self.dc_l = dc_out_l;

        // Process right channel (2x oversampled)
        let input_r = in_r * drv;
        let mid_r = (input_r + self.os_prev_r) * 0.5;
        self.os_prev_r = input_r;

        self.m_r = Self::hysteresis_step(mid_r, self.m_r, self.m_prev_r, self.h_prev_r, m_sat, alpha, k, c, dt_inv);
        self.m_prev_r = self.m_r;
        self.h_prev_r = mid_r;
        self.m_r = Self::hysteresis_step(input_r, self.m_r, self.m_prev_r, self.h_prev_r, m_sat, alpha, k, c, dt_inv);
        self.m_prev_r = self.m_r;
        self.h_prev_r = input_r;

        self.loss_r += loss_coeff * (self.m_r - self.loss_r);
        let wet_r = self.loss_r * makeup;

        let dc_out_r = wet_r - self.dc_prev_r + dc_coeff * self.dc_r;
        self.dc_prev_r = wet_r;
        self.dc_r = dc_out_r;

        let m = mix;
        (in_l * (1.0 - m) + dc_out_l * m, in_r * (1.0 - m) + dc_out_r * m)
    }
}
