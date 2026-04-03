/// Physical model piano synthesis — modal/additive approach.
///
/// Based on research from:
/// - Smith/Van Duyne commuted synthesis (CCRMA Stanford)
/// - Chaigne & Askenfelt hammer model
/// - Bank & Sujbert inharmonicity / double decay
/// - Rauhala & Välimäki dispersion modeling
///
/// Architecture: 24 inharmonic partials × dual decay banks + hammer excitation
/// + soundboard resonance + strike position comb filtering.
///
/// Key design: brightness controlled by `ks_brightness` parameter (NOT velocity),
/// because the target MIDI keyboard sends constant velocity=127.

use std::f32::consts::PI;

const TAU: f32 = 2.0 * PI;
const NUM_PARTIALS: usize = 24;

/// Inharmonicity coefficient B interpolated across the keyboard.
/// Measured values from Conklin (1999) / Fletcher & Rossing.
/// Index by (midi_note - 21), clamped to 0..87.
fn b_coefficient(midi_note: f32) -> f32 {
    // B grows roughly exponentially from bass to treble
    // A0 (21) ≈ 0.00015, A4 (69) ≈ 0.005, C8 (108) ≈ 0.15
    let n = (midi_note - 21.0).clamp(0.0, 87.0) / 87.0; // 0..1
    0.00015 * (7.0 * n).exp()
}

/// Hammer hardness exponent p across the keyboard.
/// Bass hammers: soft felt (p≈2.0), treble: hard felt (p≈3.5).
fn hammer_exponent(midi_note: f32) -> f32 {
    let n = (midi_note - 21.0).clamp(0.0, 87.0) / 87.0;
    2.0 + 1.5 * n
}

/// Hammer contact time in seconds at the given note and brightness.
/// brightness replaces velocity as the timbral control.
fn hammer_contact_time(midi_note: f32, brightness: f32) -> f32 {
    let n = (midi_note - 21.0).clamp(0.0, 87.0) / 87.0;
    // Base contact time: bass 5ms, treble 1ms
    let base_ms = 5.0 - 4.0 * n;
    // Brightness shortens the pulse (like harder hammer / higher velocity)
    // brightness 0 = very soft (long pulse), 1 = very hard (short pulse)
    let factor = 0.4 + 0.6 * (1.0 - brightness);
    base_ms * factor * 0.001 // convert to seconds
}

/// T60 decay time in seconds for a given partial.
/// Two-component model: T1 (fast, soundboard-coupled) and T2 (slow, sustained).
fn decay_times(midi_note: f32, partial_n: usize, feedback: f32) -> (f32, f32) {
    let n = (midi_note - 21.0).clamp(0.0, 87.0) / 87.0;
    let p = partial_n as f32;

    // Base T60 at fundamental: bass ~20s, treble ~3s
    // feedback parameter scales this (0.99 = short, 0.999 = long)
    let sustain_mult = 1.0 / (1.0 - feedback).max(0.0001);
    let base_t60 = (20.0 - 17.0 * n) * sustain_mult / 250.0;

    // Frequency-dependent decay: higher partials decay faster
    // tau(f) ∝ 1/(c1 + c3 * f²)
    let freq_factor = 1.0 / (1.0 + 0.002 * p * p);

    // T1 (fast decay from soundboard coupling): ~0.3-2s
    let t1 = base_t60 * 0.15 * freq_factor;
    // T2 (slow sustain from anti-symmetric mode): 3-10× longer
    let t2 = base_t60 * freq_factor;

    (t1.max(0.05), t2.max(0.1))
}

/// Strike position: fraction of string length where hammer hits.
/// ~1/8 for most notes, moving toward bridge for treble.
fn strike_position(midi_note: f32) -> f32 {
    let n = (midi_note - 21.0).clamp(0.0, 87.0) / 87.0;
    1.0 / (7.0 + 3.0 * n) // 1/7 (bass) to 1/10 (treble)
}

/// Piano model state — all fixed-size arrays, zero heap allocation.
#[derive(Clone)]
pub struct PianoModel {
    // Fast-decay bank (soundboard-coupled symmetric mode)
    phases1: [f32; NUM_PARTIALS],
    amps1: [f32; NUM_PARTIALS],
    decay1: [f32; NUM_PARTIALS], // per-sample decay multiplier

    // Slow-decay bank (anti-symmetric mode, for double-decay)
    phases2: [f32; NUM_PARTIALS],
    amps2: [f32; NUM_PARTIALS],
    decay2: [f32; NUM_PARTIALS],

    // Partial frequencies (inharmonic)
    freqs: [f32; NUM_PARTIALS],

    // Hammer excitation envelope (decaying noise burst)
    hammer_time: f32,
    hammer_duration: f32,
    hammer_brightness: f32, // controls noise LP filter
    noise_state: u32,
    noise_lp: f32,

    // Soundboard resonance (2-pole SVF bandpass)
    sb_lp: f32,
    sb_bp: f32,
    sb_g: f32,  // SVF integrator coeff
    sb_k: f32,  // 1/Q

    // DC blocker
    dc_x: f32,
    dc_y: f32,

    sample_rate: f32,
    active: bool,
}

impl PianoModel {
    pub fn new() -> Self {
        Self {
            phases1: [0.0; NUM_PARTIALS],
            amps1: [0.0; NUM_PARTIALS],
            decay1: [0.0; NUM_PARTIALS],
            phases2: [0.0; NUM_PARTIALS],
            amps2: [0.0; NUM_PARTIALS],
            decay2: [0.0; NUM_PARTIALS],
            freqs: [0.0; NUM_PARTIALS],
            hammer_time: 999.0,
            hammer_duration: 0.004,
            hammer_brightness: 0.5,
            noise_state: 0xCAFEBABE,
            noise_lp: 0.0,
            sb_lp: 0.0,
            sb_bp: 0.0,
            sb_g: 0.0,
            sb_k: 0.0,
            dc_x: 0.0,
            dc_y: 0.0,
            sample_rate: 48000.0,
            active: false,
        }
    }

    /// Initialize for a new note.
    /// `brightness` = ks_brightness (0-1), controls hammer hardness.
    /// `feedback` = ks_feedback (0.99-0.999), controls decay length.
    pub fn init(&mut self, freq: f32, sample_rate: f32, brightness: f32, feedback: f32) {
        self.sample_rate = sample_rate;
        self.active = true;

        let midi_note = (12.0 * (freq / 440.0).log2() + 69.0).clamp(21.0, 108.0);
        let b = b_coefficient(midi_note);
        let strike_pos = strike_position(midi_note);
        let nyquist = sample_rate * 0.5;

        // ── Compute partial frequencies and amplitudes ──
        for n in 0..NUM_PARTIALS {
            let partial = (n + 1) as f32;

            // Inharmonic frequency: f_n = n * f0 * sqrt(1 + B * n²)
            let f_n = freq * partial * (1.0 + b * partial * partial).sqrt();

            if f_n >= nyquist {
                // Above Nyquist — silence this partial
                self.freqs[n] = 0.0;
                self.amps1[n] = 0.0;
                self.amps2[n] = 0.0;
                self.decay1[n] = 0.0;
                self.decay2[n] = 0.0;
                self.phases1[n] = 0.0;
                self.phases2[n] = 0.0;
                continue;
            }

            self.freqs[n] = f_n;

            // Strike position filtering: sin(n * π * α) — suppresses harmonics
            // with a node at the hammer strike point
            let strike_gain = (partial * PI * strike_pos).sin().abs();

            // Spectral envelope: 1/n rolloff, shaped by brightness
            // Brighter = more high partials preserved
            let spectral = 1.0 / (1.0 + (partial - 1.0) * (0.3 + 0.7 * (1.0 - brightness)));

            let base_amp = spectral * strike_gain;

            // Dual decay: fast bank gets 70% amplitude, slow bank gets 30%
            // (Weinreich double-decay from coupled strings)
            self.amps1[n] = base_amp * 0.7;
            self.amps2[n] = base_amp * 0.3;

            // Decay rates: convert T60 to per-sample multiplier
            // decay_per_sample = 10^(-3 / (T60 * sample_rate))
            let (t1, t2) = decay_times(midi_note, n + 1, feedback);
            self.decay1[n] = if t1 > 0.01 {
                (-6.908 / (t1 * sample_rate)).exp() // 10^(-3/T60*sr) = e^(-ln(1000)/T60*sr)
            } else {
                0.0
            };
            self.decay2[n] = if t2 > 0.01 {
                (-6.908 / (t2 * sample_rate)).exp()
            } else {
                0.0
            };

            // Randomize initial phases slightly for natural sound
            // (slight phase offset between fast/slow banks creates beating)
            self.phases1[n] = 0.0;
            self.phases2[n] = 0.01 * (n as f32); // ~1 cent detuning effect
        }

        // ── Hammer excitation ──
        self.hammer_time = 0.0;
        self.hammer_duration = hammer_contact_time(midi_note, brightness);
        self.hammer_brightness = brightness;
        self.noise_lp = 0.0;

        // ── Soundboard resonance ──
        // Dense modes around 100-200 Hz, simple SVF bandpass approximation
        let sb_freq = 130.0 + 70.0 * brightness; // 130-200 Hz
        let sb_q = 1.8 + 0.7 * brightness; // Q 1.8-2.5
        self.sb_g = PI * sb_freq / sample_rate;
        self.sb_k = 1.0 / sb_q;
        self.sb_lp = 0.0;
        self.sb_bp = 0.0;

        // ── DC blocker ──
        self.dc_x = 0.0;
        self.dc_y = 0.0;
    }

    /// Generate one sample.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }

        let sr = self.sample_rate;
        let dt = 1.0 / sr;

        // ── Hammer excitation (noise burst shaped by contact time) ──
        let mut excitation = 0.0;
        if self.hammer_time < self.hammer_duration {
            let t_norm = self.hammer_time / self.hammer_duration; // 0..1

            // Half-sine force pulse envelope (Chaigne & Askenfelt)
            let pulse = (PI * t_norm).sin();

            // Hann window for smooth edges
            let window = 0.5 * (1.0 - (TAU * t_norm).cos());

            // Noise component (hammer felt texture)
            self.noise_state ^= self.noise_state << 13;
            self.noise_state ^= self.noise_state >> 17;
            self.noise_state ^= self.noise_state << 5;
            let noise = (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;

            // Mix deterministic pulse with noise
            // Brighter = more noise (harder felt = more textured attack)
            let noise_mix = 0.1 + 0.3 * self.hammer_brightness;
            let raw = pulse * (1.0 - noise_mix) + noise * noise_mix;

            // LP filter on excitation (brightness-dependent)
            // Low brightness = dark muffled attack, high = bright percussive
            let lp_coeff = 0.2 + 0.8 * self.hammer_brightness;
            self.noise_lp += lp_coeff * (raw - self.noise_lp);

            excitation = self.noise_lp * window * 0.8;
            self.hammer_time += dt;
        }

        // ── Sum partials (both decay banks) ──
        let mut sum = 0.0;
        let mut any_active = false;

        for n in 0..NUM_PARTIALS {
            let f = self.freqs[n];
            if f <= 0.0 { continue; }

            let a1 = self.amps1[n];
            let a2 = self.amps2[n];

            // Skip if both banks are silent
            if a1 < 1e-7 && a2 < 1e-7 {
                self.amps1[n] = 0.0;
                self.amps2[n] = 0.0;
                continue;
            }
            any_active = true;

            let phase_inc = f / sr;

            // Fast-decay bank
            let s1 = (self.phases1[n] * TAU).sin();
            self.phases1[n] += phase_inc;
            self.phases1[n] -= self.phases1[n].floor();
            sum += s1 * a1;

            // Slow-decay bank (slightly detuned for beating/double-decay)
            let s2 = (self.phases2[n] * TAU).sin();
            // Phase2 runs at very slightly different rate (~1 cent)
            self.phases2[n] += phase_inc * 1.0006; // ~1 cent sharp
            self.phases2[n] -= self.phases2[n].floor();
            sum += s2 * a2;

            // Apply per-sample decay
            self.amps1[n] *= self.decay1[n];
            self.amps2[n] *= self.decay2[n];

            // Hammer excitation feeds into both banks
            if excitation.abs() > 1e-8 {
                self.amps1[n] = (self.amps1[n] + excitation.abs() * 0.02).min(1.0);
                self.amps2[n] = (self.amps2[n] + excitation.abs() * 0.008).min(1.0);
            }
        }

        if !any_active && self.hammer_time >= self.hammer_duration {
            self.active = false;
            return 0.0;
        }

        // Add direct hammer noise to output (the "thunk")
        let out = sum + excitation * 0.3;

        // ── Soundboard resonance (SVF bandpass — adds body) ──
        let hp = out - self.sb_lp - self.sb_k * self.sb_bp;
        self.sb_bp += self.sb_g * hp;
        self.sb_lp += self.sb_g * self.sb_bp;
        // Mix: mostly direct + resonance body
        let with_sb = out * 0.82 + self.sb_bp * 0.18;

        // ── DC blocker ──
        let dc_out = with_sb - self.dc_x + 0.9975 * self.dc_y;
        self.dc_x = with_sb;
        self.dc_y = dc_out;

        dc_out.clamp(-1.0, 1.0)
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}
