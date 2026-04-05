/// Vocoder — 16-band channel vocoder.
/// Analyzes the modulator (typically voice) in 16 frequency bands,
/// and applies that amplitude envelope to corresponding bands of the carrier (typically synth).
/// Creates the classic "robot voice" / "talking synth" effect.
///
/// Architecture:
/// - 16 bandpass filter channels, logarithmically spaced from 50 Hz to 8 kHz
/// - Modulator path: input → band BPF → envelope follower (1-pole LP)
/// - Carrier path: input → same band BPF → multiply by modulator envelope
/// - Sum all channels for output
///
/// Parameters:
/// - `env_follow` (0..1): envelope follower speed (10ms..200ms)
/// - `gate` (0..1): minimum energy gate to suppress silent channels
/// - `mix` (0..1): wet/dry

use std::f32::consts::PI;

const NUM_BANDS: usize = 16;

/// State for a single vocoder band using a 2-pole state-variable filter (SVF).
/// Two independent SVF instances per band: one for the modulator, one for the carrier.
struct VocoderBand {
    // Modulator BPF state (SVF integrator states)
    mod_ic1: f32,
    mod_ic2: f32,
    // Carrier BPF state (SVF integrator states)
    car_ic1: f32,
    car_ic2: f32,
    // SVF coefficients (shared between modulator and carrier — same centre frequency)
    g: f32,   // integrator gain = tan(π * fc / sr)
    k: f32,   // damping = 1/Q
    a1: f32,  // coefficient a1
    a2: f32,  // coefficient a2
    a3: f32,  // coefficient a3
    // Envelope follower state (1-pole LP)
    envelope: f32,
}

impl Default for VocoderBand {
    fn default() -> Self {
        VocoderBand {
            mod_ic1: 0.0,
            mod_ic2: 0.0,
            car_ic1: 0.0,
            car_ic2: 0.0,
            g: 0.0,
            k: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            envelope: 0.0,
        }
    }
}

impl VocoderBand {
    /// Compute SVF bandpass coefficients for the given centre frequency and sample rate.
    /// Q is fixed at ~2.0 for reasonable band separation without ringing.
    fn compute_coefficients(&mut self, fc: f32, sr: f32) {
        let q = 2.0_f32;
        // Clamp fc away from Nyquist to keep g stable
        let fc = fc.min(sr * 0.49);
        self.g = (PI * fc / sr).tan();
        self.k = 1.0 / q;
        // Standard SVF (Andrew Simper / Cytomic):
        //   a1 = 1 / (1 + g*(g+k))
        //   a2 = g * a1
        //   a3 = g * a2
        let a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a1 = a1;
        self.a2 = self.g * a1;
        self.a3 = self.g * self.a2;
    }

    /// Process one sample through the modulator BPF. Returns bandpass output.
    #[inline(always)]
    fn process_mod_svf(&mut self, v0: f32) -> f32 {
        let v3 = v0 - self.mod_ic2;
        let v1 = self.a1 * self.mod_ic1 + self.a2 * v3;
        let v2 = self.mod_ic2 + self.a2 * self.mod_ic1 + self.a3 * v3;
        self.mod_ic1 = 2.0 * v1 - self.mod_ic1;
        self.mod_ic2 = 2.0 * v2 - self.mod_ic2;
        // Bandpass output
        v1
    }

    /// Process one sample through the carrier BPF. Returns bandpass output.
    #[inline(always)]
    fn process_car_svf(&mut self, v0: f32) -> f32 {
        let v3 = v0 - self.car_ic2;
        let v1 = self.a1 * self.car_ic1 + self.a2 * v3;
        let v2 = self.car_ic2 + self.a2 * self.car_ic1 + self.a3 * v3;
        self.car_ic1 = 2.0 * v1 - self.car_ic1;
        self.car_ic2 = 2.0 * v2 - self.car_ic2;
        v1
    }
}

pub struct Vocoder {
    sample_rate: f32,
    bands: [VocoderBand; NUM_BANDS],
    /// Centre frequencies for each band (Hz)
    centre_freqs: [f32; NUM_BANDS],
}

impl Vocoder {
    pub fn new(sr: f32) -> Self {
        let mut v = Vocoder {
            sample_rate: sr,
            bands: std::array::from_fn(|_| VocoderBand::default()),
            centre_freqs: [0.0; NUM_BANDS],
        };
        v.compute_band_freqs();
        v.recompute_coefficients();
        v
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.recompute_coefficients();
    }

    /// Compute logarithmically spaced band centre frequencies: 50 Hz to 8 kHz.
    fn compute_band_freqs(&mut self) {
        let f_low = 50.0_f32;
        let f_high = 8000.0_f32;
        let ratio = f_high / f_low;
        for n in 0..NUM_BANDS {
            self.centre_freqs[n] = f_low * ratio.powf(n as f32 / (NUM_BANDS - 1) as f32);
        }
    }

    /// Recompute SVF coefficients for all bands after a sample rate change.
    fn recompute_coefficients(&mut self) {
        let sr = self.sample_rate;
        for i in 0..NUM_BANDS {
            self.bands[i].compute_coefficients(self.centre_freqs[i], sr);
        }
    }

    /// Process one stereo sample through the vocoder.
    ///
    /// - `car_l`, `car_r`: carrier (synthesizer) signal
    /// - `mod_in`: modulator (voice) signal — mono; pass average of L+R if stereo
    /// - `env_follow` (0..1): follower speed (0=fast 10ms, 1=slow 200ms)
    /// - `gate` (0..1): energy gate threshold — channels below this fraction of the
    ///   maximum band energy are silenced
    /// - `mix` (0..1): wet/dry ratio
    pub fn tick(
        &mut self,
        car_l: f32,
        car_r: f32,
        mod_in: f32,
        env_follow: f32,
        gate: f32,
        mix: f32,
    ) -> (f32, f32) {
        let sr = self.sample_rate;

        // Envelope follower time constant: 10ms..200ms
        let follow_ms = 10.0 + env_follow * 190.0;
        let follow_coeff = (-1.0 / (follow_ms * 0.001 * sr)).exp();

        // Process each band
        let mut band_env = [0.0_f32; NUM_BANDS];
        let mut band_car_l = [0.0_f32; NUM_BANDS];
        let mut band_car_r = [0.0_f32; NUM_BANDS];

        for i in 0..NUM_BANDS {
            let band = &mut self.bands[i];

            // Modulator BPF + envelope follower
            let mod_bp = band.process_mod_svf(mod_in);
            let mod_rectified = mod_bp.abs();
            // 1-pole LP: attack when rising, release when falling (asymmetric)
            let env_prev = band.envelope;
            band.envelope = if mod_rectified > env_prev {
                // Attack: faster (use 1/4 of follow time)
                let att = (-1.0 / (follow_ms * 0.001 * sr * 0.25)).exp();
                att * env_prev + (1.0 - att) * mod_rectified
            } else {
                // Release: full follow time
                follow_coeff * env_prev + (1.0 - follow_coeff) * mod_rectified
            };
            band_env[i] = band.envelope;

            // Carrier BPF (L and R share the same filter state — use mono average)
            let car_mono = (car_l + car_r) * 0.5;
            let car_bp_mono = band.process_car_svf(car_mono);
            // Reconstruct stereo by maintaining the L/R difference
            let car_diff = (car_l - car_r) * 0.5;
            band_car_l[i] = car_bp_mono + car_diff;
            band_car_r[i] = car_bp_mono - car_diff;
        }

        // Energy gate: find the peak envelope across all bands
        let max_env = band_env.iter().cloned().fold(0.0_f32, f32::max);
        let gate_thresh = gate * max_env;

        // Sum all bands, gated and modulated
        let mut wet_l = 0.0_f32;
        let mut wet_r = 0.0_f32;
        // Scale gain to compensate for NUM_BANDS summing
        let band_gain = 1.0 / (NUM_BANDS as f32).sqrt();

        for i in 0..NUM_BANDS {
            if band_env[i] < gate_thresh {
                continue;
            }
            let env = band_env[i];
            wet_l += band_car_l[i] * env * band_gain;
            wet_r += band_car_r[i] * env * band_gain;
        }

        let dry_l = car_l * (1.0 - mix);
        let dry_r = car_r * (1.0 - mix);
        (dry_l + wet_l * mix, dry_r + wet_r * mix)
    }
}
