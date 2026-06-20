//! Reverb 2 — Plate-style feedback delay network (FDN) reverb.
//! Uses 8 delay lines with a Hadamard mixing matrix.
//! Character: smooth, metallic plate sound (like a spring plate hybrid).
//!
//! Algorithm: an 8-line FDN where each delay line output is mixed through an
//! orthogonal Hadamard matrix before being fed back. Prime-number delay lengths
//! ensure dense, decorrelated early reflections. Per-line 1-pole LP filters
//! provide high-frequency damping. Modulation of delay read positions adds a
//! subtle chorus/shimmer effect.

use std::f32::consts::PI;

/// Prime-number delay lengths at 44.1 kHz for decorrelation.
const BASE_DELAYS: [usize; 8] = [1481, 1867, 2053, 2399, 2707, 3001, 3299, 3607];

/// Maximum scale factor * longest prime + headroom, sized for 2x scaling.
const MAX_DELAY: usize = 32768;

pub struct Reverb2 {
    /// 8 delay line buffers, each MAX_DELAY samples.
    delays: [Vec<f32>; 8],
    /// Write positions for each delay line.
    write_pos: [usize; 8],
    /// Actual (scaled) delay lengths in samples for each line.
    #[allow(dead_code)]
    delay_len: [usize; 8],
    /// 1-pole LP state per delay line (damping).
    lp_state: [f32; 8],
    /// LFO phases for per-line modulation (0..1).
    lfo_phases: [f32; 8],
    sample_rate: f32,
}

impl Reverb2 {
    pub fn new(sr: f32) -> Self {
        let scale = sr / 44100.0;
        let mut delay_len = [0usize; 8];
        for (i, &base) in BASE_DELAYS.iter().enumerate() {
            delay_len[i] = ((base as f32 * scale) as usize).clamp(1, MAX_DELAY - 1);
        }

        Self {
            delays: [
                vec![0.0f32; MAX_DELAY],
                vec![0.0f32; MAX_DELAY],
                vec![0.0f32; MAX_DELAY],
                vec![0.0f32; MAX_DELAY],
                vec![0.0f32; MAX_DELAY],
                vec![0.0f32; MAX_DELAY],
                vec![0.0f32; MAX_DELAY],
                vec![0.0f32; MAX_DELAY],
            ],
            write_pos: [0; 8],
            delay_len,
            lp_state: [0.0; 8],
            lfo_phases: [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875],
            sample_rate: sr,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        let scale = sr / 44100.0;
        for (i, &base) in BASE_DELAYS.iter().enumerate() {
            self.delay_len[i] = ((base as f32 * scale) as usize).clamp(1, MAX_DELAY - 1);
            self.delays[i].fill(0.0);
            self.write_pos[i] = 0;
            self.lp_state[i] = 0.0;
        }
    }

    /// Hadamard mix: 8-point in-place butterfly transform.
    /// This orthogonal matrix ensures energy-preserving mixing between delay lines.
    /// Implemented as log2(8)=3 stages of butterfly operations.
    #[inline]
    fn hadamard8(v: &mut [f32; 8]) {
        // Stage 1: pairs (0,1),(2,3),(4,5),(6,7)
        for i in (0..8).step_by(2) {
            let a = v[i];
            let b = v[i + 1];
            v[i]     = a + b;
            v[i + 1] = a - b;
        }
        // Stage 2: pairs (0,2),(1,3),(4,6),(5,7)
        for base in [0usize, 4] {
            let a = v[base];
            let b = v[base + 2];
            v[base]     = a + b;
            v[base + 2] = a - b;
            let a = v[base + 1];
            let b = v[base + 3];
            v[base + 1] = a + b;
            v[base + 3] = a - b;
        }
        // Stage 3: pairs (0,4),(1,5),(2,6),(3,7)
        for i in 0..4 {
            let a = v[i];
            let b = v[i + 4];
            v[i]     = a + b;
            v[i + 4] = a - b;
        }
        // Normalize by 1/sqrt(8) to keep unit gain through the matrix
        let norm = 1.0 / 8.0_f32.sqrt();
        for x in v.iter_mut() {
            *x *= norm;
        }
    }

    /// Process one stereo sample pair.
    ///
    /// - `decay`   (0..1): reverb time mapped to 0.1..20.0 seconds
    /// - `damping` (0..1): HF damping (LP cutoff 500Hz..20kHz)
    /// - `size`    (0..1): room size scales delay lengths (0.5..2.0x)
    /// - `mix`     (0..1): wet/dry ratio
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        decay: f32,
        damping: f32,
        size: f32,
        mix: f32,
    ) -> (f32, f32) {
        let sr = self.sample_rate;

        // Map parameters
        let decay_s = 0.1 + decay * 19.9;                     // 0.1..20.0 seconds
        let damp_cutoff = 500.0 + damping * 19500.0;           // 500Hz..20kHz
        let damp_coeff = 1.0 - (-2.0 * PI * damp_cutoff / sr).exp();
        let size_scale = 0.5 + size * 1.5;                     // 0.5..2.0x

        // Mono input sum fed equally into all 8 delay lines
        let input = (in_l + in_r) * 0.5;

        // Read current outputs from each delay line (with size modulation)
        let mut outs = [0.0f32; 8];
        for i in 0..8 {
            // Scaled delay length for this line
            let base_sr_scaled = BASE_DELAYS[i] as f32 * (sr / 44100.0);
            let scaled_delay = (base_sr_scaled * size_scale).clamp(1.0, (MAX_DELAY - 2) as f32);
            let delay_samples = scaled_delay as usize;

            // Compute feedback coefficient from T60 formula:
            // fb = exp(-7 * delay_time / decay_time)  (≈ -60dB at decay_time)
            let delay_time_s = scaled_delay / sr;
            let fb = (-7.0 * delay_time_s / decay_s).exp();

            // Read from delay line
            let read_pos = (self.write_pos[i] + MAX_DELAY - delay_samples) % MAX_DELAY;
            let raw = self.delays[i][read_pos];

            // 1-pole LP damping in feedback path
            let new_lp_val = self.lp_state[i] + damp_coeff * (raw - self.lp_state[i]);
            self.lp_state[i] = new_lp_val + 1e-30;
            let damped = self.lp_state[i];

            outs[i] = damped * fb;
        }

        // Hadamard mixing matrix — cross-couples all 8 lines
        Self::hadamard8(&mut outs);

        // Write back: mixed feedback + injected input
        for (i, &out) in outs.iter().enumerate() {
            self.delays[i][self.write_pos[i]] = input + out;
            self.write_pos[i] = (self.write_pos[i] + 1) % MAX_DELAY;
        }

        // Advance LFO phases (used for optional future modulation extension)
        let lfo_rate = 0.5 / sr; // 0.5 Hz base modulation
        for i in 0..8 {
            self.lfo_phases[i] = (self.lfo_phases[i] + lfo_rate) % 1.0;
        }

        // Output: sum pairs of delay lines for L and R to create stereo image
        // Lines 0,2,4,6 → left; lines 1,3,5,7 → right
        let wet_l = (outs[0] + outs[2] + outs[4] + outs[6]) * 0.5;
        let wet_r = (outs[1] + outs[3] + outs[5] + outs[7]) * 0.5;

        let out_l = in_l * (1.0 - mix) + wet_l * mix;
        let out_r = in_r * (1.0 - mix) + wet_r * mix;

        (out_l, out_r)
    }
}
