//! Resonator bank — 4 tuned comb filters with independent frequencies.
//!
//! Like Surge XT's Resonator effect: 4 voices, each a comb filter (or 2-pole resonant LP)
//! at a configurable frequency with a decay/resonance parameter.
//! Great for metallic sounds, pitched reverb tails, and formant shaping.

use std::f32::consts::PI;
use super::dsp_utils::fast_tan;

const MAX_COMB: usize = 4096; // ~93ms at 44100 Hz

pub struct Resonator {
    sample_rate: f32,
    // 4 tuned resonator voices
    bufs: [Box<[f32; MAX_COMB]>; 4],
    write: [usize; 4],
    // Per-voice 2-pole bandpass (state variable filter)
    bp_ic1: [f32; 4],
    bp_ic2: [f32; 4],
    // Cached coefficients
    freqs: [f32; 4],     // resonator frequencies in Hz
    decays: [f32; 4],    // 0..1, 0=short decay, 1=long
    levels: [f32; 4],    // output mix levels
}

impl Resonator {
    pub fn new(sr: f32) -> Self {
        Self {
            sample_rate: sr,
            bufs: [
                Box::new([0.0; MAX_COMB]),
                Box::new([0.0; MAX_COMB]),
                Box::new([0.0; MAX_COMB]),
                Box::new([0.0; MAX_COMB]),
            ],
            write: [0; 4],
            bp_ic1: [0.0; 4],
            bp_ic2: [0.0; 4],
            freqs: [200.0, 400.0, 800.0, 1600.0],
            decays: [0.7, 0.7, 0.7, 0.7],
            levels: [0.25, 0.25, 0.25, 0.25],
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        for buf in &mut self.bufs { buf.fill(0.0); }
        self.write = [0; 4];
        self.bp_ic1 = [0.0; 4];
        self.bp_ic2 = [0.0; 4];
    }

    pub fn set_freq(&mut self, voice: usize, freq: f32) {
        if voice < 4 { self.freqs[voice] = freq.clamp(20.0, self.sample_rate * 0.49); }
    }

    pub fn set_decay(&mut self, voice: usize, decay: f32) {
        if voice < 4 { self.decays[voice] = decay.clamp(0.0, 0.9999); }
    }

    /// Process mono input, return stereo (alternates L/R per voice).
    pub fn tick(&mut self, input: f32, mix: f32) -> (f32, f32) {
        if mix < 0.001 {
            return (input, input);
        }

        let mut out_l = 0.0_f32;
        let mut out_r = 0.0_f32;
        let sr = self.sample_rate;

        for i in 0..4 {
            let freq = self.freqs[i];
            let decay = self.decays[i];

            // SVF bandpass coefficients
            let g = fast_tan(PI * freq / sr);
            let k = 0.5; // moderate Q (~2)
            let a1 = 1.0 / (1.0 + g * (g + k));
            let a2 = g * a1;
            let a3 = g * a2;

            // SVF tick
            let v3 = input - self.bp_ic2[i];
            let v1 = a1 * self.bp_ic1[i] + a2 * v3;
            let v2 = self.bp_ic2[i] + a2 * self.bp_ic1[i] + a3 * v3;
            self.bp_ic1[i] = 2.0 * v1 - self.bp_ic1[i];
            self.bp_ic2[i] = 2.0 * v2 - self.bp_ic2[i];
            let bp = v1; // bandpass output

            // Write to comb buffer (with decay feedback)
            let delay_samples = (sr / freq).round().clamp(1.0, (MAX_COMB - 1) as f32) as usize;
            let read_pos = (self.write[i] + MAX_COMB - delay_samples) % MAX_COMB;
            let feedback = self.bufs[i][read_pos] * decay;
            self.bufs[i][self.write[i]] = (bp + feedback).clamp(-4.0, 4.0);
            let out = self.bufs[i][self.write[i]];
            self.write[i] = (self.write[i] + 1) % MAX_COMB;

            let out = out * self.levels[i];
            // Alternate L/R for stereo spread
            if i % 2 == 0 { out_l += out; } else { out_r += out; }
        }

        // Balance L/R
        let orig_l = out_l;
        let out_l = out_l + out_r * 0.2;
        let out_r = out_r + orig_l * 0.2;
        let dry = 1.0 - mix;
        (input * dry + out_l * mix, input * dry + out_r * mix)
    }
}

