//! Floaty Delay — pitch-modulated stereo delay with gentle LFO wobble.
//! The delay time slowly drifts, creating a subtle pitch shift / vibrato on the echo.
//!
//! Inspired by Surge XT's Floaty Delay. Uses two independent delay lines with
//! quadrature LFO modulation on the read position, creating a floating/drifting
//! stereo sensation. A 1-pole LP filter in the feedback path provides damping.

use std::f32::consts::PI;

const BUF_SIZE: usize = 384000; // ~8s at 48kHz, supports high sample rates

pub struct FloatyDelay {
    buf_l: Box<[f32; BUF_SIZE]>,
    buf_r: Box<[f32; BUF_SIZE]>,
    write_pos: usize,
    /// LFO phase for left channel (0..1)
    lfo_phase_l: f32,
    /// LFO phase for right channel — offset by 0.25 (quadrature)
    lfo_phase_r: f32,
    /// 1-pole LP state for damping filter (left feedback path)
    damp_l: f32,
    /// 1-pole LP state for damping filter (right feedback path)
    damp_r: f32,
    sample_rate: f32,
}

impl FloatyDelay {
    pub fn new(sr: f32) -> Self {
        Self {
            buf_l: Box::new([0.0f32; BUF_SIZE]),
            buf_r: Box::new([0.0f32; BUF_SIZE]),
            write_pos: 0,
            lfo_phase_l: 0.0,
            lfo_phase_r: 0.25, // quadrature offset for stereo width
            damp_l: 0.0,
            damp_r: 0.0,
            sample_rate: sr,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write_pos = 0;
        self.lfo_phase_l = 0.0;
        self.lfo_phase_r = 0.25;
        self.damp_l = 0.0;
        self.damp_r = 0.0;
    }

    /// Read from a circular buffer with linear interpolation for fractional positions.
    #[inline]
    fn read_interpolated(buf: &[f32], pos: f32, len: usize) -> f32 {
        let i0 = pos as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = pos.fract();
        buf[i0] * (1.0 - frac) + buf[i1] * frac
    }

    /// Process one stereo sample pair.
    ///
    /// - `time`     (0..1): base delay time mapped to 0.05s..1.0s
    /// - `feedback` (0..1): feedback amount, clamped to 0..0.9
    /// - `wobble`   (0..1): LFO modulation depth (0..20% of delay time)
    /// - `rate`     (0..1): LFO rate mapped to 0.1..3.0 Hz
    /// - `damp`     (0..1): feedback damping (LP cutoff 200Hz..8kHz)
    /// - `mix`      (0..1): wet/dry ratio
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        time: f32,
        feedback: f32,
        wobble: f32,
        rate: f32,
        damp: f32,
        mix: f32,
    ) -> (f32, f32) {
        let sr = self.sample_rate;

        // Map parameters to DSP ranges
        let base_time_s = 0.05 + time * 0.95;             // 0.05..1.0 seconds
        let base_delay = base_time_s * sr;                 // delay in samples
        let fb = (feedback * 0.9).clamp(0.0, 0.9);        // 0..0.9 max to avoid runaway
        let lfo_depth = wobble * 0.20 * base_delay;        // 0..20% of delay time in samples
        let lfo_rate_hz = 0.1 + rate * 2.9;               // 0.1..3.0 Hz
        let lfo_inc = lfo_rate_hz / sr;

        // Damping LP coefficient: cutoff 200Hz..8kHz
        let damp_cutoff = 200.0 + damp * 7800.0;
        let damp_coeff = 1.0 - (-2.0 * PI * damp_cutoff / sr).exp();

        // LFO: sine modulation on delay read position
        let lfo_l = (2.0 * PI * self.lfo_phase_l).sin();
        let lfo_r = (2.0 * PI * self.lfo_phase_r).sin();

        // Compute modulated read positions (as floating-point sample offsets back from write_pos)
        let mod_delay_l = (base_delay + lfo_l * lfo_depth).max(1.0).min((BUF_SIZE - 2) as f32);
        let mod_delay_r = (base_delay + lfo_r * lfo_depth).max(1.0).min((BUF_SIZE - 2) as f32);

        // Convert delay offset to absolute read position in circular buffer
        let read_pos_l = (self.write_pos as f32 - mod_delay_l + BUF_SIZE as f32) % BUF_SIZE as f32;
        let read_pos_r = (self.write_pos as f32 - mod_delay_r + BUF_SIZE as f32) % BUF_SIZE as f32;

        // Read delayed samples with interpolation
        let delayed_l = Self::read_interpolated(&self.buf_l[..], read_pos_l, BUF_SIZE);
        let delayed_r = Self::read_interpolated(&self.buf_r[..], read_pos_r, BUF_SIZE);

        // Apply 1-pole LP damping filter in the feedback path
        // y[n] = y[n-1] + coeff * (x[n] - y[n-1])
        let new_damp_l = self.damp_l + damp_coeff * (delayed_l - self.damp_l);
        let new_damp_r = self.damp_r + damp_coeff * (delayed_r - self.damp_r);
        self.damp_l = new_damp_l + 1e-30;
        self.damp_r = new_damp_r + 1e-30;
        let damp_out_l = new_damp_l;
        let damp_out_r = new_damp_r;

        // Stereo cross-feedback: L feeds a little into R and vice versa
        // Cross-feed at 10% for stereo spreading
        let cross = 0.10;
        let write_l = in_l + fb * (damp_out_l * (1.0 - cross) + damp_out_r * cross);
        let write_r = in_r + fb * (damp_out_r * (1.0 - cross) + damp_out_l * cross);

        // Write new samples to circular buffers
        self.buf_l[self.write_pos] = write_l;
        self.buf_r[self.write_pos] = write_r;

        // Advance write position
        self.write_pos = (self.write_pos + 1) % BUF_SIZE;

        // Advance LFO phases
        self.lfo_phase_l = (self.lfo_phase_l + lfo_inc) % 1.0;
        self.lfo_phase_r = (self.lfo_phase_r + lfo_inc) % 1.0;

        // Wet/dry mix
        let out_l = in_l * (1.0 - mix) + damp_out_l * mix;
        let out_r = in_r * (1.0 - mix) + damp_out_r * mix;

        (out_l, out_r)
    }
}
