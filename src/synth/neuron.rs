//! Neuron Distortion — GRU (Gated Recurrent Unit) nonlinearity with comb filters.
//! Algorithm inspired by Surge XT chowdsp NeuronEffect.

use super::dsp_utils::DcBlocker;

pub struct Neuron {
    sample_rate: f32,
    // GRU state per channel
    y_l: f32,
    y_r: f32,
    // Comb filter delay lines
    comb_buf_l: [f32; 4096],
    comb_buf_r: [f32; 4096],
    comb_pos: usize,
    // 2x oversampling previous
    os_prev_l: f32,
    os_prev_r: f32,
    dc: DcBlocker,
}

impl Neuron {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            y_l: 0.0, y_r: 0.0,
            comb_buf_l: [0.0; 4096],
            comb_buf_r: [0.0; 4096],
            comb_pos: 0,
            os_prev_l: 0.0, os_prev_r: 0.0,
            dc: DcBlocker::new(),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.y_l = 0.0; self.y_r = 0.0;
        self.comb_buf_l = [0.0; 4096];
        self.comb_buf_r = [0.0; 4096];
        self.comb_pos = 0;
        self.os_prev_l = 0.0; self.os_prev_r = 0.0;
        self.dc.reset();
    }

    /// Fast sigmoid approximation
    #[inline(always)]
    fn sigmoid(x: f32) -> f32 {
        x / (1.0 + x.abs())
    }

    /// Fast tanh approximation
    #[inline(always)]
    fn fast_tanh(x: f32) -> f32 {
        let x2 = x * x;
        x * (27.0 + x2) / (27.0 + 9.0 * x2)
    }

    /// GRU cell: f = sigmoid(Wf*x + Uf*y + bf), out = f*y + (1-f)*tanh(Wh*x + Uh*f*y)
    #[inline(always)]
    fn gru_step(x: f32, y_prev: f32, wf: f32, uf: f32, bf: f32, wh: f32, uh: f32) -> f32 {
        let f = Self::sigmoid(wf * x + uf * y_prev + bf);
        f * y_prev + (1.0 - f) * Self::fast_tanh(wh * x + uh * f * y_prev)
    }

    /// Lagrange 3rd-order interpolation for fractional delay
    #[inline]
    fn lagrange_read(buf: &[f32; 4096], pos: usize, delay: f32) -> f32 {
        let d = delay.clamp(1.0, 4094.0);
        let di = d as usize;
        let frac = d - di as f32;
        let mask = 4095;
        let s0 = buf[(pos + 4096 - di - 1) & mask];
        let s1 = buf[(pos + 4096 - di) & mask];
        let s2 = buf[(pos + 4096 - di + 1) & mask];
        let s3 = buf[(pos + 4096 - di + 2) & mask];

        let c0 = s1;
        let c1 = s2 - s0 * (1.0 / 3.0) - s1 * 0.5 - s3 * (1.0 / 6.0);
        let c2 = (s0 + s2) * 0.5 - s1;
        let c3 = (s3 - s0) * (1.0 / 6.0) + (s1 - s2) * 0.5;
        ((c3 * frac + c2) * frac + c1) * frac + c0
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self, in_l: f32, in_r: f32,
        drive: f32, squash: f32, stab: f32, asym: f32, bias: f32,
        comb_freq: f32, comb_sep: f32, mix: f32,
    ) -> (f32, f32) {
        if mix < 0.001 { return (in_l, in_r); }

        // GRU weights from drive/squash/stab/asym
        let wf = 1.0 + drive * 4.0;
        let uf = 0.5 + squash * 2.0;
        let bf = bias * 2.0 - 1.0 + asym;
        let wh = 1.0 + stab * 3.0;
        let uh = 0.5 + squash;

        // Process with 2x oversampling
        let process_gru = |input: f32, _prev: f32, y: &mut f32, os_prev: &mut f32| -> f32 {
            let mid = (input + *os_prev) * 0.5;
            *os_prev = input;
            *y = Self::gru_step(mid, *y, wf, uf, bf, wh, uh);
            *y = Self::gru_step(input, *y, wf, uf, bf, wh, uh);
            *y
        };

        let gru_l = process_gru(in_l, self.os_prev_l, &mut self.y_l, &mut self.os_prev_l);
        let gru_r = process_gru(in_r, self.os_prev_r, &mut self.y_r, &mut self.os_prev_r);

        // Write to comb buffers
        self.comb_buf_l[self.comb_pos] = gru_l;
        self.comb_buf_r[self.comb_pos] = gru_r;

        // Comb filter readout
        let comb_hz = comb_freq.clamp(20.0, 4000.0);
        let delay1 = self.sample_rate / comb_hz;
        let sep_ratio = 1.0 + comb_sep * 0.5;
        let delay2 = delay1 * sep_ratio;

        let comb_l = Self::lagrange_read(&self.comb_buf_l, self.comb_pos, delay1)
                   + Self::lagrange_read(&self.comb_buf_l, self.comb_pos, delay2) * 0.5;
        let comb_r = Self::lagrange_read(&self.comb_buf_r, self.comb_pos, delay1)
                   + Self::lagrange_read(&self.comb_buf_r, self.comb_pos, delay2) * 0.5;

        self.comb_pos = (self.comb_pos + 1) & 4095;

        // Makeup gain
        let makeup = (-0.119 * drive).exp() + 1.0;
        let wet_l = comb_l * makeup * 0.5;
        let wet_r = comb_r * makeup * 0.5;

        let dc_coeff = DcBlocker::coeff_for(35.0, self.sample_rate);
        let (out_l, out_r) = self.dc.process(wet_l, wet_r, dc_coeff);

        let m = mix;
        (in_l * (1.0 - m) + out_l * m, in_r * (1.0 - m) + out_r * m)
    }
}
