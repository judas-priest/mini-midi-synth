/// Electric Piano physical model — Rhodes Mark II / Wurlitzer 200A / Stage 73.
///
/// Architecture: 5 inharmonic damped sinusoidal modes (Gordon-Smith resonators)
/// + hammer noise burst attack + pickup saturation ("bark").
///
/// References:
/// - Hatch (2008) measured Rhodes tine spectra
/// - Krekovic et al. (2016) inharmonicity ratios
/// - Bank (2010) magnetic pickup nonlinearity
///
/// Brightness (ks_brightness 0-1) controls strike hardness:
///   0 = soft/dark/quiet, 1 = hard/bright/barky
/// Feedback (ks_feedback 0-1) controls sustain length:
///   0 = damped (like pedal off), 1 = long ring
/// Type (epiano_type 0-2): 0=Rhodes MkII, 1=Wurlitzer 200A, 2=Stage73

use std::f32::consts::PI;

const NUM_MODES: usize = 5;

/// Inharmonic partial ratios (relative to fundamental).
/// Measured from real instruments.
///   Rhodes MkII:  tines slightly inharmonic upward (metal bar physics)
///   Wurlitzer:    reeds nearly harmonic, subtle stretch
///   Stage 73:     older Rhodes, more inharmonic / "raw"
const RATIOS: [[f32; NUM_MODES]; 3] = [
    [1.000, 2.021, 3.044, 4.074, 5.106], // Rhodes MkII
    [1.000, 2.004, 3.014, 4.028, 5.045], // Wurlitzer 200A
    [1.000, 2.028, 3.062, 4.100, 5.141], // Stage 73
];

/// Initial mode amplitude distribution (normalized sum ≈ 1).
/// Higher modes start relatively brighter at hard strikes.
const AMPS: [[f32; NUM_MODES]; 3] = [
    [0.58, 0.26, 0.10, 0.04, 0.02], // Rhodes: fundamental-heavy
    [0.50, 0.29, 0.13, 0.06, 0.02], // Wurlitzer: richer overtones
    [0.56, 0.27, 0.11, 0.05, 0.01], // Stage 73
];

/// Decay ratio per mode: mode_bandwidth = base_bandwidth * decay_ratio[i].
/// Higher modes decay faster (stiffness losses dominate).
const DECAY_RATIO: [f32; NUM_MODES] = [1.0, 2.8, 6.0, 11.0, 18.0];

#[derive(Clone)]
pub struct ElectricPianoModel {
    // Gordon-Smith resonator state
    y1: [f32; NUM_MODES],    // x[n-1]
    y2: [f32; NUM_MODES],    // x[n-2]
    coeff: [f32; NUM_MODES], // 2 * r * cos(ω)
    r_sq: [f32; NUM_MODES],  // r²
    init_amp: [f32; NUM_MODES],

    // Pickup saturation — the "bark" on hard strikes
    pickup_drive: f32,

    // Attack noise burst (hammer/key mechanism transient)
    attack_time: f32,
    attack_dur: f32,
    noise_state: u32,
    noise_lp: f32,
    attack_level: f32,

    // DC blocker
    dc_x: f32,
    dc_y: f32,

    sample_rate: f32,
    active: bool,
}

impl ElectricPianoModel {
    pub fn new() -> Self {
        Self {
            y1: [0.0; NUM_MODES],
            y2: [0.0; NUM_MODES],
            coeff: [0.0; NUM_MODES],
            r_sq: [1.0; NUM_MODES],
            init_amp: [0.0; NUM_MODES],
            pickup_drive: 1.0,
            attack_time: 999.0,
            attack_dur: 0.005,
            noise_state: 0xF00DCAFE,
            noise_lp: 0.0,
            attack_level: 0.0,
            dc_x: 0.0,
            dc_y: 0.0,
            sample_rate: 48000.0,
            active: false,
        }
    }

    /// Initialize for a new note.
    /// ep_type: 0=Rhodes MkII, 1=Wurlitzer 200A, 2=Stage 73
    pub fn init(&mut self, freq: f32, sample_rate: f32, brightness: f32, feedback: f32, ep_type: u8) {
        self.sample_rate = sample_rate;
        self.active = true;

        let t = ep_type.min(2) as usize;
        let nyquist = sample_rate * 0.5;

        // Base T60 at fundamental — measured from Rhodes at moderate strike:
        // Low C (~65 Hz): ~15s, Middle C (~261 Hz): ~4.5s, High C (~1047 Hz): ~1.0s
        // Approximation: T60 = 18 * exp(-2.5 * normalized_freq)
        let midi_approx = (12.0 * (freq / 440.0).log2() + 69.0).clamp(21.0, 108.0);
        let n_kbd = (midi_approx - 21.0) / 87.0;
        let base_t60 = 18.0 * (-2.5 * n_kbd).exp();

        // feedback (0-1) scales decay: 0→very short, 1→maximum sustain
        let sustain_mult = 0.25 + 1.5 * feedback;
        let t60 = base_t60 * sustain_mult;

        // Spectral brightness: higher brightness = more initial high-mode energy
        // At max brightness, high modes start ~3× louder than at minimum
        let brightness_boost = 1.0 + brightness * 2.0;

        // Initialize modes
        self.y1 = [0.0; NUM_MODES];
        self.y2 = [0.0; NUM_MODES];

        for i in 0..NUM_MODES {
            let mode_freq = freq * RATIOS[t][i];
            if mode_freq >= nyquist {
                self.coeff[i] = 0.0;
                self.r_sq[i] = 0.0;
                self.init_amp[i] = 0.0;
                continue;
            }

            // Per-mode T60: higher modes decay faster
            let mode_t60 = t60 / DECAY_RATIO[i];
            // bandwidth from T60: bw = ln(1000) / (π * T60)
            let bandwidth = 6.908 / (PI * mode_t60.max(0.02));

            let r = (-PI * bandwidth / sample_rate).exp();
            let omega = TAU * mode_freq / sample_rate;
            self.coeff[i] = 2.0 * r * omega.cos();
            self.r_sq[i] = r * r;

            // Mode amplitude: base spectrum + brightness boosts upper modes
            let mode_bright_factor = if i == 0 {
                1.0
            } else {
                1.0 + (brightness_boost - 1.0) * (i as f32 / (NUM_MODES - 1) as f32)
            };
            self.init_amp[i] = AMPS[t][i] * mode_bright_factor;
        }

        // Normalize amplitudes
        let amp_sum: f32 = self.init_amp.iter().sum();
        let norm = if amp_sum > 0.001 { 1.0 / amp_sum } else { 1.0 };
        for i in 0..NUM_MODES {
            self.init_amp[i] *= norm;
        }

        // Excite the resonators with initial displacement
        // Overall amplitude scales with brightness (hard strike = louder)
        let strike_amp = 0.4 + 0.6 * brightness;
        for i in 0..NUM_MODES {
            self.y1[i] = self.init_amp[i] * strike_amp;
            self.y2[i] = self.y1[i] * 0.999; // slightly offset so oscillation starts
        }

        // Pickup drive: brightness controls how far into saturation we go
        // At low brightness: nearly linear pickup (clean), high: soft-clip (bark)
        // Wurlitzer has naturally more aggressive pickup saturation
        let type_drive = match t {
            1 => 1.5, // Wurlitzer: more saturated
            _ => 1.0,
        };
        self.pickup_drive = 1.0 + brightness * 3.5 * type_drive;

        // Attack noise: key mechanism thump (Wurlitzer has stronger key click)
        self.attack_time = 0.0;
        let base_dur_ms = match t {
            1 => 6.0, // Wurlitzer: longer key click
            _ => 3.5, // Rhodes: shorter thump
        };
        self.attack_dur = base_dur_ms * 0.001;
        self.attack_level = (0.05 + 0.15 * brightness) * (1.0 + 0.5 * (t == 1) as u8 as f32);
        self.noise_lp = 0.0;
        self.noise_state = 0xF00DCAFE ^ (freq as u32).wrapping_mul(6271);

        // DC blocker
        self.dc_x = 0.0;
        self.dc_y = 0.0;
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }

        let dt = 1.0 / self.sample_rate;

        // ── Attack noise burst (key/hammer mechanism) ──
        let mut noise_out = 0.0;
        if self.attack_time < self.attack_dur {
            let t = self.attack_time / self.attack_dur;
            let env = (PI * t).sin() * (1.0 - t); // half-sine * linear fade

            self.noise_state ^= self.noise_state << 13;
            self.noise_state ^= self.noise_state >> 17;
            self.noise_state ^= self.noise_state << 5;
            let noise = (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;

            // LP-filter to shape noise color (more muffled = more piano-like thump)
            self.noise_lp += 0.25 * (noise - self.noise_lp);
            noise_out = self.noise_lp * env * self.attack_level;
            self.attack_time += dt;
        }

        // ── Gordon-Smith resonators ──
        let mut sum = 0.0;
        let mut any_active = false;

        for i in 0..NUM_MODES {
            if self.r_sq[i] < 1e-10 {
                continue;
            }

            let y0 = self.coeff[i] * self.y1[i] - self.r_sq[i] * self.y2[i];
            self.y2[i] = self.y1[i];
            self.y1[i] = y0;

            if y0.abs() > 1e-7 {
                any_active = true;
                sum += y0;
            }
        }

        if !any_active && self.attack_time >= self.attack_dur {
            self.active = false;
            return 0.0;
        }

        // ── Magnetic pickup saturation (bark) ──
        // Simulates the nonlinear electromagnetic pickup of a Rhodes:
        // at high tine displacement, the pickup saturates → harmonic distortion.
        // Model: soft clip via tanh approximation.
        let driven = sum * self.pickup_drive;
        let pickup_out = if driven.abs() < 0.5 {
            driven
        } else {
            driven.signum() * (1.0 - (-2.0 * driven.abs()).exp()) * 0.7
        };

        // Normalize pickup output back to reasonable level
        let out = pickup_out / self.pickup_drive.sqrt() + noise_out;

        // ── DC blocker ──
        let dc_out = out - self.dc_x + 0.9975 * self.dc_y;
        self.dc_x = out;
        self.dc_y = dc_out;

        dc_out.clamp(-1.0, 1.0)
    }
}

const TAU: f32 = 2.0 * PI;
