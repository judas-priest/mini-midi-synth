/// Physical model piano synthesis — modal/additive approach.
///
/// Based on research from:
/// - Conklin (1996/1999): measured inharmonicity B coefficients
/// - Weinreich (1977): double-decay / coupled string beating
/// - Bank & Sujbert (2003): frequency-dependent decay model
/// - Chaigne & Askenfelt (1994): hammer force model
/// - Rauhala & Välimäki (2007): dispersion and T60 modeling
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
///
/// Fit to concert grand piano measurements (Conklin 1996, Bank & Sujbert 2003).
/// B is smaller on a concert grand than an upright because the strings are longer.
/// Using exp(4.5) growth (B increases ~90× across the keyboard):
///
///   A0  (MIDI  21): B ≈ 0.00020
///   C3  (MIDI  48): B ≈ 0.00081
///   C4  (MIDI  60): B ≈ 0.00150
///   C5  (MIDI  72): B ≈ 0.00280
///   C6  (MIDI  84): B ≈ 0.00520
///   C7  (MIDI  96): B ≈ 0.00968
///   C8  (MIDI 108): B ≈ 0.01800
///
/// This keeps partial-12 stretch below ~50 cents at all positions,
/// which is the practical limit for pleasant-sounding additive synthesis.
/// (Real upright piano B at C4 is ~0.001-0.003, but those large values push
///  the upper partials into metallic/noise-like territory with 24 partials.)
fn b_coefficient(midi_note: f32) -> f32 {
    let n = (midi_note - 21.0).clamp(0.0, 87.0) / 87.0;
    0.00020 * (4.5 * n).exp()
}

/// Hammer hardness exponent p across the keyboard.
/// Bass hammers: heavy felt (p ≈ 2.0), treble: hard felt (p ≈ 3.5).
#[allow(dead_code)]
fn hammer_exponent(midi_note: f32) -> f32 {
    let n = (midi_note - 21.0).clamp(0.0, 87.0) / 87.0;
    2.0 + 1.5 * n
}

/// Hammer contact time in seconds at the given note and brightness.
/// brightness replaces velocity as the timbral control.
fn hammer_contact_time(midi_note: f32, brightness: f32) -> f32 {
    let n = (midi_note - 21.0).clamp(0.0, 87.0) / 87.0;
    // Bass: 3-6ms, treble: 0.5-2ms (Askenfelt & Jansson 1990)
    let base_ms = 4.5 - 3.5 * n;
    // Brightness shortens the pulse (brighter = harder strike)
    let factor = 0.3 + 0.7 * (1.0 - brightness);
    (base_ms * factor * 0.001).max(0.0005)
}

/// T60 decay times for a given partial.
///
/// Two-component model (Weinreich 1977):
///   T1 = fast decay (symmetric coupled-string mode)  — soundboard-coupled
///   T2 = slow decay (anti-symmetric mode) — the long sustain component
///
/// T60 base from measured data (Askenfelt 1990, roughly):
///   A0: ~20s, C4: ~8s, C8: ~1.5s
///   Formula: t60_base = 20 * 2^(-(midi-21)/36)
///
/// feedback (0..1) acts as a direct sustain multiplier: 0.2 + 1.8*feedback
/// So feedback=0.2 → 0.56× damped, feedback=0.8 → 1.64× (bright/sustained).
///
/// T1 is always shorter than T2: T1 ≈ T2 / ratio, where ratio ≈ 6 (bass) to 2 (treble).
/// Frequency-dependent decay: higher partials decay faster by 1/(1 + 0.015 * n^1.5).
fn decay_times(midi_note: f32, partial_n: usize, feedback: f32) -> (f32, f32) {
    let p = partial_n as f32;

    // Base T60 at fundamental: fits ~20s at A0, ~8s at C4, ~1.5s at C8
    let t60_base = 20.0 * 2.0_f32.powf(-(midi_note - 21.0) / 36.0);

    // feedback maps to a sustain multiplier: soft→short, bright→long
    // feedback range in presets: ~0.2-0.9
    let sustain_mult = 0.2 + 1.8 * feedback;
    let t60 = t60_base * sustain_mult;

    // Frequency-dependent decay: physically motivated by string stiffness losses
    // Higher partials lose energy faster. Bank (2006): ∝ 1/(c1 + c3*f^1.5) ≈ 1/(1 + 0.015*n^1.5)
    let freq_factor = 1.0 / (1.0 + 0.015 * p.powf(1.5));

    // T2 (slow, sustained component)
    let t2 = (t60 * freq_factor).max(0.05);

    // T1 (fast component, soundboard-coupled)
    // Ratio T2/T1: ~6 at bass, ~2 at treble
    let n = (midi_note - 21.0).clamp(0.0, 87.0) / 87.0;
    let t1_ratio = 6.0 - 4.0 * n; // 6 at bass, 2 at treble
    let t1 = (t2 / t1_ratio).max(0.03);

    (t1, t2)
}

/// Strike position: fraction of string length where hammer hits.
/// ~1/7 for bass, ~1/8 for middle, ~1/10 for treble.
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

    // Slow-decay bank (anti-symmetric mode, for double-decay / beating)
    phases2: [f32; NUM_PARTIALS],
    amps2: [f32; NUM_PARTIALS],
    decay2: [f32; NUM_PARTIALS],
    detune2: [f32; NUM_PARTIALS], // per-partial frequency ratio for slow bank (creates beating)

    // Partial frequencies (inharmonic)
    freqs: [f32; NUM_PARTIALS],

    // Hammer excitation envelope (decaying noise burst)
    hammer_time: f32,
    hammer_duration: f32,
    hammer_brightness: f32,
    noise_state: u32,
    noise_lp: f32,

    // Soundboard resonance (2-pole SVF bandpass)
    sb_lp: f32,
    sb_bp: f32,
    sb_g: f32,
    sb_k: f32,

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
            detune2: [1.0; NUM_PARTIALS],
            freqs: [0.0; NUM_PARTIALS],
            hammer_time: 999.0,
            hammer_duration: 0.003,
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
    /// `brightness` = ks_brightness (0..1), controls hammer hardness and spectral rolloff.
    /// `feedback`   = ks_feedback   (0..1), controls decay length (sustain multiplier).
    pub fn init(&mut self, freq: f32, sample_rate: f32, brightness: f32, feedback: f32) {
        self.sample_rate = sample_rate;
        self.active = true;

        let midi_note = (12.0 * (freq / 440.0).log2() + 69.0).clamp(21.0, 108.0);
        let b = b_coefficient(midi_note);
        let strike_pos = strike_position(midi_note);
        let nyquist = sample_rate * 0.5;

        // Detuning for slow bank (creates Weinreich double-decay beating).
        // Physical range: 0.2-1.5 cents — varies with register.
        // Bass strings have more coupling and larger beat rates in absolute Hz.
        // We want a beat period of ~5-15 seconds in the mid range.
        let n_kbd = (midi_note - 21.0).clamp(0.0, 87.0) / 87.0;
        let detune_cents = 1.5 - 1.2 * n_kbd; // 1.5 cents (bass) → 0.3 cents (treble)
        // The slow bank frequency ratio: f2 = f1 * 2^(detune_cents/1200)
        let slow_ratio = 2.0_f32.powf(detune_cents / 1200.0);

        // Spectral envelope exponent: alpha controls high-partial rolloff.
        // Power law: amp_n = 1/n^alpha
        //   soft (brightness=0): alpha=1.8 → steep rolloff, dark tone
        //   bright (brightness=1): alpha=0.5 → shallow rolloff, bright tone
        let alpha = 1.8 - 1.3 * brightness;

        // ── Compute partial frequencies and amplitudes ──
        for n in 0..NUM_PARTIALS {
            let partial = (n + 1) as f32;

            // Inharmonic frequency: f_n = n * f0 * sqrt(1 + B * n²)
            // B is now correctly small: C4 ≈ 0.0015, C6 ≈ 0.014, C8 ≈ 0.055
            let f_n = freq * partial * (1.0 + b * partial * partial).sqrt();

            if f_n >= nyquist {
                self.freqs[n] = 0.0;
                self.amps1[n] = 0.0;
                self.amps2[n] = 0.0;
                self.decay1[n] = 0.0;
                self.decay2[n] = 0.0;
                self.phases1[n] = 0.0;
                self.phases2[n] = 0.0;
                self.detune2[n] = 1.0;
                continue;
            }

            self.freqs[n] = f_n;

            // Strike position filtering: sin(n * π * α) — notches partials
            // with a node at the hammer strike point (Conklin 1996).
            let strike_gain = (partial * PI * strike_pos).sin().abs();

            // Power-law spectral envelope: 1/n^alpha
            // Brighter presets preserve more high-partial energy.
            let spectral = 1.0 / partial.powf(alpha);

            let base_amp = spectral * strike_gain;

            // Dual decay amplitude split (Weinreich 1977):
            //   Fast bank (symmetric coupled mode): carries ~60% of energy
            //   Slow bank (anti-symmetric, survives longer): ~40%
            // The beating between the two creates the characteristic
            // piano "double decay" and gentle amplitude modulation.
            self.amps1[n] = base_amp * 0.60;
            self.amps2[n] = base_amp * 0.40;

            // Decay rates: T60 → per-sample multiplier via e^(-ln(1000)/T60*sr)
            // (T60 is the time to decay to -60 dB = 1/1000 amplitude)
            let (t1, t2) = decay_times(midi_note, n + 1, feedback);
            self.decay1[n] = (-6.908 / (t1 * sample_rate)).exp();
            self.decay2[n] = (-6.908 / (t2 * sample_rate)).exp();

            // Detuning for slow bank: frequency ratio per partial.
            // Slow bank runs at slightly different pitch to create beating.
            // Use the same ratio for all partials — this means partial n's
            // beating rate is n × fundamental_beat_hz (higher partials beat faster).
            self.detune2[n] = slow_ratio;

            // Start phases aligned — beating emerges naturally from frequency difference
            self.phases1[n] = 0.0;
            self.phases2[n] = 0.0;
        }

        // ── Hammer excitation ──
        self.hammer_time = 0.0;
        self.hammer_duration = hammer_contact_time(midi_note, brightness);
        self.hammer_brightness = brightness;
        self.noise_lp = 0.0;

        // ── Soundboard resonance ──
        // Dense modes concentrated around 80-200 Hz.
        // Brighter settings shift resonance up slightly.
        let sb_freq = 100.0 + 80.0 * brightness; // 100-180 Hz
        let sb_q = 1.5 + brightness * 0.8;        // Q 1.5-2.3
        self.sb_g = (PI * sb_freq / sample_rate).tan();
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
        // This models the felt hammer striking the string.
        // The excitation is added DIRECTLY to the output (the "thunk"),
        // NOT fed back into the partial amplitudes — the partial amplitudes
        // were already set at init() from the hammer spectrum.
        let mut excitation = 0.0;
        if self.hammer_time < self.hammer_duration {
            let t_norm = self.hammer_time / self.hammer_duration; // 0..1

            // Half-sine force pulse (Chaigne & Askenfelt 1994)
            let pulse = (PI * t_norm).sin();

            // Hann window for smooth attack and release
            let window = 0.5 * (1.0 - (TAU * t_norm).cos());

            // Noise component (hammer felt surface texture)
            self.noise_state ^= self.noise_state << 13;
            self.noise_state ^= self.noise_state >> 17;
            self.noise_state ^= self.noise_state << 5;
            let noise = (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;

            // LP filter on noise to shape its color
            // Low brightness = muffled noise, high = bright crack
            let lp_coeff = 0.15 + 0.75 * self.hammer_brightness;
            self.noise_lp += lp_coeff * (noise - self.noise_lp);

            // Noise is mixed with the pulse; brighter = more noise (harder felt)
            let noise_mix = 0.08 + 0.25 * self.hammer_brightness;
            let raw = pulse * (1.0 - noise_mix) + self.noise_lp * noise_mix;
            excitation = raw * window;

            self.hammer_time += dt;
        }

        // ── Sum partials (both decay banks) ──
        let mut sum = 0.0;
        let mut any_active = false;

        for n in 0..NUM_PARTIALS {
            let f = self.freqs[n];
            if f <= 0.0 {
                continue;
            }

            let a1 = self.amps1[n];
            let a2 = self.amps2[n];

            if a1 < 1e-7 && a2 < 1e-7 {
                self.amps1[n] = 0.0;
                self.amps2[n] = 0.0;
                continue;
            }
            any_active = true;

            let phase_inc = f / sr;

            // Fast-decay bank (symmetric mode)
            let s1 = (self.phases1[n] * TAU).sin();
            self.phases1[n] = (self.phases1[n] + phase_inc).fract();
            sum += s1 * a1;

            // Slow-decay bank (anti-symmetric, detuned for beating)
            // detune2[n] = 2^(cents/1200) ≈ 1.000289 for 0.5 cents
            let s2 = (self.phases2[n] * TAU).sin();
            self.phases2[n] = (self.phases2[n] + phase_inc * self.detune2[n]).fract();
            sum += s2 * a2;

            // Apply per-sample amplitude decay
            self.amps1[n] *= self.decay1[n];
            self.amps2[n] *= self.decay2[n];
            // NOTE: No hammer excitation back-coupling here.
            // Partial amplitudes are set once at init() and only decay thereafter.
            // The hammer energy IS in those initial amplitudes.
        }

        if !any_active && self.hammer_time >= self.hammer_duration {
            self.active = false;
            return 0.0;
        }

        // Mix partials with direct hammer transient.
        // The "thunk" (excitation * 0.25) gives the percussive click attack.
        let out = sum + excitation * 0.25;

        // ── Soundboard resonance (TPT SVF bandpass) ──
        // Adds low-frequency body and warmth, approximately modeling the
        // dense coupled resonances of the piano soundboard below ~200 Hz.
        let g = self.sb_g;
        let k = self.sb_k;
        let hp = out - self.sb_lp - k * self.sb_bp;
        self.sb_bp += g * hp;
        self.sb_lp += g * self.sb_bp;
        // Mix: mostly direct + 15% body resonance
        let with_sb = out * 0.85 + self.sb_bp * 0.15;

        // ── DC blocker ──
        let dc_out = with_sb - self.dc_x + 0.9975 * self.dc_y;
        self.dc_x = with_sb;
        self.dc_y = dc_out;

        dc_out.clamp(-1.0, 1.0)
    }

    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        self.active
    }
}
