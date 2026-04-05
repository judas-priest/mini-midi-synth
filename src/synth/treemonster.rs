/// Treemonster — Pitch-tracking ring modulator.
/// Detects the pitch of the input via zero-crossing rate, generates a sine at
/// the detected pitch (with optional transposition), ring-modulates the signal.
/// Creates metallic, robotic harmonics.
///
/// Inspired by Surge XT's Treemonster effect. The pitch detector watches the L
/// channel for zero crossings and estimates the fundamental frequency from the
/// average period between them. A slow IIR smoother prevents glitchy pitch jumps.
/// Both L and R are ring-modulated with the same tracked sine oscillator.

use std::f32::consts::PI;

pub struct Treemonster {
    /// Sample index of the last detected zero crossing.
    last_zc_sample: i64,
    /// Running sample counter (monotonically increasing).
    sample_count: i64,
    /// Previous sample value for edge detection.
    prev_sample: f32,
    /// Smoothed detected frequency (Hz).
    freq_smooth: f32,
    /// Ring modulator oscillator phase (0..1).
    osc_phase: f32,
    sample_rate: f32,
}

impl Treemonster {
    pub fn new(sr: f32) -> Self {
        Self {
            last_zc_sample: 0,
            sample_count: 0,
            prev_sample: 0.0,
            freq_smooth: 440.0, // default to A4 until pitch is detected
            osc_phase: 0.0,
            sample_rate: sr,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.last_zc_sample = 0;
        self.sample_count = 0;
        self.prev_sample = 0.0;
        self.freq_smooth = 440.0;
        self.osc_phase = 0.0;
    }

    /// Process one stereo sample pair.
    ///
    /// - `threshold` (0..1): zero-crossing noise gate threshold (0..0.5 amplitude)
    /// - `shift`     (-24..+24 semitones): pitch shift of the ring mod oscillator
    /// - `ring_mix`  (0..1): blend between dry (0) and ring-modulated (1) wet signal
    /// - `mix`       (0..1): final wet/dry ratio between input and processed signal
    ///
    /// Pitch detection uses the L channel only; both channels share the detected pitch.
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        threshold: f32,
        shift: f32,
        ring_mix: f32,
        mix: f32,
    ) -> (f32, f32) {
        let sr = self.sample_rate;

        // Map threshold to amplitude gate (0..0.5)
        let gate = threshold * 0.5;

        // --- Pitch detection on L channel ---

        // Detect positive-going zero crossing: prev < 0, current >= 0
        let curr = in_l;
        let prev = self.prev_sample;
        let crossed = prev < 0.0 && curr >= 0.0 && curr.abs() > gate;

        if crossed {
            let period_samples = self.sample_count - self.last_zc_sample;
            if period_samples > 0 {
                // Each positive ZC represents one full cycle
                let detected_freq = sr / period_samples as f32;
                // Clamp to a musically sensible range (20Hz..4kHz)
                let detected_clamped = detected_freq.clamp(20.0, 4000.0);
                // Smooth with a 1-pole IIR to prevent pitch glitches
                // Smoothing constant: ~0.45s time constant
                self.freq_smooth = 0.95 * self.freq_smooth + 0.05 * detected_clamped;
            }
            self.last_zc_sample = self.sample_count;
        }

        self.prev_sample = curr;
        self.sample_count += 1;

        // --- Ring modulator oscillator ---

        // Apply pitch shift: f_osc = f_smooth * 2^(shift/12)
        let shift_clamped = shift.clamp(-24.0, 24.0);
        let osc_freq = self.freq_smooth * 2.0_f32.powf(shift_clamped / 12.0);

        // Generate sine at detected (and shifted) pitch
        let sine = (2.0 * PI * self.osc_phase).sin();

        // Advance oscillator phase
        let phase_inc = osc_freq / sr;
        self.osc_phase = (self.osc_phase + phase_inc) % 1.0;

        // --- Ring modulation ---
        // ring_mix blends between the dry signal and the ring-modulated signal
        let rm_l = in_l * sine;
        let rm_r = in_r * sine;

        let wet_l = in_l * (1.0 - ring_mix) + rm_l * ring_mix;
        let wet_r = in_r * (1.0 - ring_mix) + rm_r * ring_mix;

        // Final wet/dry blend with original input
        let out_l = in_l * (1.0 - mix) + wet_l * mix;
        let out_r = in_r * (1.0 - mix) + wet_r * mix;

        (out_l, out_r)
    }
}
