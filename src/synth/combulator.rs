/// Combulator — 3 tuned comb filters creating metallic resonance.
/// Each comb filter tracks a musical pitch (Hz) with adjustable feedback.
/// Output mixed together with panning: comb1=center, comb2=left, comb3=right.
///
/// Inspired by Surge XT's Combulator effect. The three comb filters each produce
/// a pitched resonance at their respective frequencies. When driven with broadband
/// or noisy input, they ring at their tuned pitches, creating metallic timbres.
/// Adjusting the semitone offsets creates chords or dissonant clusters.

use std::f32::consts::PI;

/// Max comb buffer length. At 44.1kHz, 4096 samples ≈ 93ms, covers down to ~10.7Hz.
const COMB_BUF_SIZE: usize = 4096;

/// One comb filter: delay buffer + write position + LP state.
struct CombFilter {
    buf: Box<[f32; COMB_BUF_SIZE]>,
    write_pos: usize,
    lp_state: f32,
}

impl CombFilter {
    fn new() -> Self {
        Self {
            buf: Box::new([0.0f32; COMB_BUF_SIZE]),
            write_pos: 0,
            lp_state: 0.0,
        }
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.write_pos = 0;
        self.lp_state = 0.0;
    }

    /// Process one sample through this comb filter.
    ///
    /// - `input`:    dry input sample
    /// - `freq`:     comb resonant frequency in Hz
    /// - `feedback`: feedback gain (0..0.95)
    /// - `lp_coeff`: 1-pole LP coefficient for tone filtering in feedback path
    /// - `sr`:       sample rate
    ///
    /// Returns the comb output (the delayed+resonated sample).
    #[inline]
    fn tick(&mut self, input: f32, freq: f32, feedback: f32, lp_coeff: f32, sr: f32) -> f32 {
        // Delay length in samples (fractional for accurate tuning)
        let delay_f = (sr / freq.max(1.0)).clamp(1.0, (COMB_BUF_SIZE - 2) as f32);

        // Read position: write_pos - delay_length (wrapped)
        let read_pos_f =
            (self.write_pos as f32 - delay_f + COMB_BUF_SIZE as f32) % COMB_BUF_SIZE as f32;

        // Linear interpolation for fractional delay
        let i0 = read_pos_f as usize % COMB_BUF_SIZE;
        let i1 = (i0 + 1) % COMB_BUF_SIZE;
        let frac = read_pos_f.fract();
        let delayed = self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac;

        // 1-pole LP filter in the feedback path (tone control)
        // High lp_coeff = brighter (more HF in feedback); low = darker
        self.lp_state += lp_coeff * (delayed - self.lp_state);
        let filtered = self.lp_state;

        // Write: input + feedback * filtered delayed signal
        self.buf[self.write_pos] = input + feedback * filtered;

        // Advance write position
        self.write_pos = (self.write_pos + 1) % COMB_BUF_SIZE;

        delayed
    }
}

pub struct Combulator {
    combs: [CombFilter; 3],
    sample_rate: f32,
}

impl Combulator {
    pub fn new(sr: f32) -> Self {
        Self {
            combs: [CombFilter::new(), CombFilter::new(), CombFilter::new()],
            sample_rate: sr,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        for c in self.combs.iter_mut() {
            c.reset();
        }
    }

    /// Process one stereo sample pair.
    ///
    /// - `freq`     (Hz, 20..2000): base frequency for comb 1
    /// - `offset2`  (semitones, -12..+12): comb 2 pitch offset from comb 1
    /// - `offset3`  (semitones, -12..+12): comb 3 pitch offset from comb 1
    /// - `feedback` (0..1): feedback amount, internally clamped to 0..0.95
    /// - `tone`     (0..1): feedback LP brightness (0=dark, 1=bright)
    /// - `mix`      (0..1): wet/dry ratio
    ///
    /// Panning: comb1 = center, comb2 = left, comb3 = right.
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        freq: f32,
        offset2: f32,
        offset3: f32,
        feedback: f32,
        tone: f32,
        mix: f32,
    ) -> (f32, f32) {
        let sr = self.sample_rate;

        // Clamp feedback to safe range
        let fb = (feedback * 0.95).clamp(0.0, 0.95);

        // Tone: LP cutoff 200Hz..20kHz mapped from tone parameter
        let lp_cutoff = 200.0 + tone * 19800.0;
        let lp_coeff = 1.0 - (-2.0 * PI * lp_cutoff / sr).exp();

        // Compute frequencies for each comb filter via semitone offsets
        // f2 = f1 * 2^(offset2/12), f3 = f1 * 2^(offset3/12)
        let freq1 = freq.clamp(20.0, 2000.0);
        let freq2 = freq1 * 2.0_f32.powf(offset2 / 12.0);
        let freq3 = freq1 * 2.0_f32.powf(offset3 / 12.0);

        // Use mono input sum for all three combs
        let mono_in = (in_l + in_r) * 0.5;

        // Tick each comb filter
        let out1 = self.combs[0].tick(mono_in, freq1, fb, lp_coeff, sr);
        let out2 = self.combs[1].tick(mono_in, freq2, fb, lp_coeff, sr);
        let out3 = self.combs[2].tick(mono_in, freq3, fb, lp_coeff, sr);

        // Panning matrix:
        // comb1 (center): equal L+R
        // comb2 (left):   L only
        // comb3 (right):  R only
        let wet_l = out1 * 0.7071 + out2;
        let wet_r = out1 * 0.7071 + out3;

        // Wet/dry mix
        let out_l = in_l * (1.0 - mix) + wet_l * mix;
        let out_r = in_r * (1.0 - mix) + wet_r * mix;

        (out_l, out_r)
    }
}
