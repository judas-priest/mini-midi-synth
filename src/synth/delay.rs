/// Stereo delay with ping-pong option and feedback filtering.

const MAX_DELAY_SAMPLES: usize = 131072; // ~2.7s at 48kHz, must be power of 2
const DELAY_MASK: usize = MAX_DELAY_SAMPLES - 1;

#[derive(Clone)]
pub struct StereoDelay {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write_pos: usize,
    sample_rate: f32,
    lp_state_l: f32,
    lp_state_r: f32,
    /// Smoothed delay times to prevent clicks on time changes
    smooth_delay_l: f32,
    smooth_delay_r: f32,
}

impl StereoDelay {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            buf_l: vec![0.0; MAX_DELAY_SAMPLES],
            buf_r: vec![0.0; MAX_DELAY_SAMPLES],
            write_pos: 0,
            sample_rate,
            lp_state_l: 0.0,
            lp_state_r: 0.0,
            smooth_delay_l: 0.0,
            smooth_delay_r: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write_pos = 0;
        self.lp_state_l = 0.0;
        self.lp_state_r = 0.0;
        self.smooth_delay_l = 0.0;
        self.smooth_delay_r = 0.0;
    }

    /// Process one stereo sample.
    /// `time_l`/`time_r` in seconds, `feedback` 0..0.95, `filter` 0..1 (LP amount),
    /// `ping_pong`: cross-feed L↔R, `mix` 0..1.
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        time_l: f32,
        time_r: f32,
        feedback: f32,
        filter: f32,
        ping_pong: bool,
        mix: f32,
    ) -> (f32, f32) {
        if mix < 0.001 {
            return (in_l, in_r);
        }

        let feedback = feedback.min(0.95);

        let target_delay_l = (time_l * self.sample_rate).clamp(1.0, (MAX_DELAY_SAMPLES - 2) as f32);
        let target_delay_r = (time_r * self.sample_rate).clamp(1.0, (MAX_DELAY_SAMPLES - 2) as f32);

        // Smooth delay time with one-pole filter to prevent clicks on time changes
        // Coefficient ~0.001 gives a gentle slew (~1ms at 48kHz)
        const SMOOTH_COEFF: f32 = 0.001;
        if self.smooth_delay_l == 0.0 {
            self.smooth_delay_l = target_delay_l;
            self.smooth_delay_r = target_delay_r;
        }
        self.smooth_delay_l += SMOOTH_COEFF * (target_delay_l - self.smooth_delay_l);
        self.smooth_delay_r += SMOOTH_COEFF * (target_delay_r - self.smooth_delay_r);

        let delay_l = self.smooth_delay_l;
        let delay_r = self.smooth_delay_r;

        // Linear interpolation for fractional delay read
        let len = MAX_DELAY_SAMPLES as f32;
        let read_pos_l = self.write_pos as f32 - delay_l;
        let read_pos_r = self.write_pos as f32 - delay_r;

        let idx_l_f = if read_pos_l < 0.0 { read_pos_l + len } else { read_pos_l };
        let idx_r_f = if read_pos_r < 0.0 { read_pos_r + len } else { read_pos_r };

        let idx_l0 = idx_l_f as usize & DELAY_MASK;
        let idx_l1 = (idx_l0 + 1) & DELAY_MASK;
        let frac_l = idx_l_f.fract();
        let tap_l = self.buf_l[idx_l0] * (1.0 - frac_l) + self.buf_l[idx_l1] * frac_l;

        let idx_r0 = idx_r_f as usize & DELAY_MASK;
        let idx_r1 = (idx_r0 + 1) & DELAY_MASK;
        let frac_r = idx_r_f.fract();
        let tap_r = self.buf_r[idx_r0] * (1.0 - frac_r) + self.buf_r[idx_r1] * frac_r;

        // One-pole LP in feedback path
        let lp_coeff = filter.clamp(0.0, 0.95);
        self.lp_state_l += lp_coeff * (tap_l - self.lp_state_l);
        self.lp_state_r += lp_coeff * (tap_r - self.lp_state_r);

        let filt_l = if lp_coeff > 0.001 { self.lp_state_l } else { tap_l };
        let filt_r = if lp_coeff > 0.001 { self.lp_state_r } else { tap_r };

        // Write with feedback
        if ping_pong {
            self.buf_l[self.write_pos] = in_l + filt_r * feedback;
            self.buf_r[self.write_pos] = in_r + filt_l * feedback;
        } else {
            self.buf_l[self.write_pos] = in_l + filt_l * feedback;
            self.buf_r[self.write_pos] = in_r + filt_r * feedback;
        }

        self.write_pos = (self.write_pos + 1) & DELAY_MASK;

        let out_l = in_l + tap_l * mix;
        let out_r = in_r + tap_r * mix;
        (out_l, out_r)
    }
}
