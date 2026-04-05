/// Spring Reverb — Schroeder allpass network with dispersive delay lines.
/// Algorithm inspired by Surge XT chowdsp SpringReverbEffect.

use std::f32::consts::PI;

const MAX_DELAY: usize = 8192;
const NUM_ALLPASS: usize = 8;

struct AllpassFilter {
    buffer: Vec<f32>,
    pos: usize,
    len: usize,
}

impl AllpassFilter {
    fn new(len: usize) -> Self {
        Self { buffer: vec![0.0; len.max(1)], pos: 0, len: len.max(1) }
    }

    fn set_len(&mut self, new_len: usize) {
        let new_len = new_len.max(1);
        self.buffer.resize(new_len, 0.0);
        self.buffer.fill(0.0);
        self.len = new_len;
        self.pos = 0;
    }

    #[inline]
    fn process(&mut self, input: f32, gain: f32) -> f32 {
        let delayed = self.buffer[self.pos % self.len];
        let out = -input * gain + delayed;
        self.buffer[self.pos % self.len] = input + delayed * gain;
        self.pos = (self.pos + 1) % self.len;
        out
    }
}

pub struct SpringReverb {
    sample_rate: f32,
    // Main delay lines (L/R)
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    delay_pos: usize,
    // Allpass dispersion network
    allpasses_l: Vec<AllpassFilter>,
    allpasses_r: Vec<AllpassFilter>,
    // Damping SVF (one-pole LP per channel)
    damp_l: f32,
    damp_r: f32,
    // Early reflection taps
    er_gains: [f32; 4],
    er_delays: [usize; 4],
    // Feedback state
    fb_l: f32,
    fb_r: f32,
}

impl SpringReverb {
    pub fn new(sample_rate: f32) -> Self {
        let mut s = Self {
            sample_rate,
            delay_l: vec![0.0; MAX_DELAY],
            delay_r: vec![0.0; MAX_DELAY],
            delay_pos: 0,
            allpasses_l: Vec::new(),
            allpasses_r: Vec::new(),
            damp_l: 0.0, damp_r: 0.0,
            er_gains: [0.4, -0.3, 0.2, -0.15],
            er_delays: [113, 337, 557, 773],
            fb_l: 0.0, fb_r: 0.0,
        };
        // Initialize allpass filters with prime-number delays for dispersion
        let ap_delays = [139, 193, 263, 349, 421, 509, 601, 701];
        for &d in &ap_delays {
            let scaled = (d as f32 * sample_rate / 48000.0) as usize;
            s.allpasses_l.push(AllpassFilter::new(scaled));
            s.allpasses_r.push(AllpassFilter::new(scaled));
        }
        s
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.delay_l = vec![0.0; MAX_DELAY];
        self.delay_r = vec![0.0; MAX_DELAY];
        self.delay_pos = 0;
        self.damp_l = 0.0; self.damp_r = 0.0;
        self.fb_l = 0.0; self.fb_r = 0.0;

        let ap_delays = [139, 193, 263, 349, 421, 509, 601, 701];
        for (i, &d) in ap_delays.iter().enumerate() {
            let scaled = (d as f32 * sr / 48000.0) as usize;
            if i < self.allpasses_l.len() {
                self.allpasses_l[i].set_len(scaled);
                self.allpasses_r[i].set_len(scaled);
            }
        }
    }

    /// Lagrange 3rd-order interpolation for fractional delay read.
    #[inline]
    fn interp_read(buf: &[f32], pos: usize, delay: f32) -> f32 {
        let max = buf.len();
        let d = delay.clamp(1.0, (max - 2) as f32);
        let di = d as usize;
        let frac = d - di as f32;
        let i0 = (pos + max - di - 1) % max;
        let i1 = (pos + max - di) % max;
        let i2 = (pos + max - di + 1) % max;
        let i3 = (pos + max - di + 2) % max;
        let s0 = buf[i0]; let s1 = buf[i1]; let s2 = buf[i2]; let s3 = buf[i3];
        let c0 = s1;
        let c1 = s2 - s0 * (1.0 / 3.0) - s1 * 0.5 - s3 * (1.0 / 6.0);
        let c2 = (s0 + s2) * 0.5 - s1;
        let c3 = (s3 - s0) * (1.0 / 6.0) + (s1 - s2) * 0.5;
        ((c3 * frac + c2) * frac + c1) * frac + c0
    }

    #[inline]
    pub fn tick(
        &mut self, in_l: f32, in_r: f32,
        size: f32, decay: f32, reflections: f32,
        damping: f32, spin: f32, _chaos: f32, mix: f32,
    ) -> (f32, f32) {
        if mix < 0.001 { return (in_l, in_r); }

        // Size controls main delay length
        let delay_samples = 200.0 + size * (MAX_DELAY as f32 - 400.0);

        // Feedback gain from decay (T60)
        let t60 = 0.2 + decay * 5.0;
        let fb_gain = 10.0_f32.powf(-3.0 * delay_samples / (t60 * self.sample_rate)).min(0.98);

        // Allpass gain from spin
        let ap_gain = 0.3 + spin * 0.35;

        // Damping coefficient
        let damp_coeff = (PI * (1000.0 + (1.0 - damping) * 15000.0) / self.sample_rate).sin().min(0.999);

        // Input + feedback
        let sig_l = in_l + self.fb_l * fb_gain;
        let sig_r = in_r + self.fb_r * fb_gain;

        // Write to delay line
        self.delay_l[self.delay_pos] = sig_l;
        self.delay_r[self.delay_pos] = sig_r;

        // Read from main delay
        let main_l = Self::interp_read(&self.delay_l, self.delay_pos, delay_samples);
        let main_r = Self::interp_read(&self.delay_r, self.delay_pos, delay_samples * 1.07);

        // Early reflections
        let mut er_l = 0.0;
        let mut er_r = 0.0;
        let ref_level = reflections;
        for i in 0..4 {
            let d = (self.er_delays[i] as f32 * size * 2.0 + 10.0).min(MAX_DELAY as f32 - 2.0);
            er_l += Self::interp_read(&self.delay_l, self.delay_pos, d) * self.er_gains[i] * ref_level;
            er_r += Self::interp_read(&self.delay_r, self.delay_pos, d) * self.er_gains[i] * ref_level;
        }

        self.delay_pos = (self.delay_pos + 1) % MAX_DELAY;

        // Dispersion via allpass cascade
        let mut disp_l = main_l;
        let mut disp_r = main_r;
        for i in 0..NUM_ALLPASS {
            disp_l = self.allpasses_l[i].process(disp_l, ap_gain);
            disp_r = self.allpasses_r[i].process(disp_r, ap_gain);
        }

        // Cross-feed for spring-like behavior
        let cross_l = disp_l * 0.8 + disp_r * 0.2;
        let cross_r = disp_r * 0.8 + disp_l * 0.2;

        // Damping (one-pole LP)
        self.damp_l += damp_coeff * (cross_l - self.damp_l);
        self.damp_r += damp_coeff * (cross_r - self.damp_r);

        // Store feedback
        self.fb_l = self.damp_l;
        self.fb_r = self.damp_r;

        let wet_l = self.damp_l + er_l;
        let wet_r = self.damp_r + er_r;

        let m = mix;
        (in_l * (1.0 - m) + wet_l * m, in_r * (1.0 - m) + wet_r * m)
    }
}
