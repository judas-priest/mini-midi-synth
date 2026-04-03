/// Physical model bass guitar using Digital Waveguide synthesis.
///
/// Based on Julius Smith III DWG theory, Rank-Kubin slapbass model,
/// and Kramer/Abesser/Dittmar (Fraunhofer IDMT) multi-technique model.
///
/// Key improvements over basic Karplus-Strong:
/// - Välimäki robust loss filter (one-zero/one-pole) for freq-dependent damping
/// - Dispersion allpass for string inharmonicity
/// - Pickup position comb filter (bridge/neck/both)
/// - Pickup electrical resonance model
/// - 3-mode body resonance
/// - Fret collision for slap buzz (Rank-Kubin)
/// - Sympathetic string

use std::f32::consts::PI;

/// Playing style.
#[derive(Clone, Copy, PartialEq)]
pub enum BassStyle {
    Finger = 0,
    Pick = 1,
    Slap = 2,
}

impl BassStyle {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            1 => Self::Pick,
            2 => Self::Slap,
            _ => Self::Finger,
        }
    }
}

/// Pickup configuration.
#[derive(Clone, Copy, PartialEq)]
pub enum PickupConfig {
    Bridge = 0,    // J-bass bridge ~0.08
    Neck = 1,      // P-bass neck ~0.20
    Both = 2,      // J-bass both blended
}

impl PickupConfig {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            1 => Self::Neck,
            2 => Self::Both,
            _ => Self::Bridge,
        }
    }
}

/// State for one body resonance mode (SVF biquad).
#[derive(Clone)]
struct BodyMode {
    lp: f32,
    bp: f32,
    freq: f32,  // Hz
    q: f32,
    gain: f32,  // linear
}

impl BodyMode {
    fn new(freq: f32, q: f32, gain_db: f32) -> Self {
        Self { lp: 0.0, bp: 0.0, freq, q, gain: 10.0_f32.powf(gain_db / 20.0) - 1.0 }
    }

    #[inline]
    fn tick(&mut self, input: f32, sr: f32) -> f32 {
        let w = (PI * self.freq / sr).sin() * 2.0;
        let q_inv = 1.0 / self.q;
        let hp = input - self.lp - q_inv * self.bp;
        self.bp += w * hp;
        self.lp += w * self.bp;
        self.bp * self.gain
    }
}

#[derive(Clone)]
pub struct BassModel {
    // Main string waveguide
    buffer: Vec<f32>,
    pos: usize,

    // Loop filter: Välimäki one-zero/one-pole
    lf_b0: f32,
    lf_b1: f32,
    lf_a1: f32,
    lf_x1: f32,  // previous input
    lf_y1: f32,  // previous output
    loop_gain: f32,

    // Fractional delay allpass (Thiran)
    ap_coeff: f32,
    ap_x1: f32,
    ap_y1: f32,

    // Dispersion allpass (inharmonicity)
    disp_coeff: f32,
    disp_x1: f32,
    disp_y1: f32,

    // Sympathetic string (slight detune)
    buffer2: Vec<f32>,
    pos2: usize,
    sym_filter: f32,
    sym_gain: f32,

    // Pickup comb filter taps (offset from pos in samples)
    pickup_bridge_tap: usize,
    pickup_neck_tap: usize,
    pickup_config: PickupConfig,
    pickup_blend: f32, // 0=bridge, 1=neck (for Both mode)

    // Pickup electrical resonance (2-pole resonant LP via SVF)
    pu_lp: f32,
    pu_bp: f32,
    pu_freq: f32,
    pu_q: f32,

    // Body resonance (3 modes)
    body_modes: [BodyMode; 3],
    body_mix: f32,

    // Fret collision (slap)
    fret_height: f32,      // collision threshold
    fret_damping: f32,     // energy loss on collision

    // Slap click state
    slap_click_amp: f32,
    noise_state: u32,
    time: f32,

    sample_rate: f32,
}

impl BassModel {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            pos: 0,
            lf_b0: 0.5, lf_b1: 0.5, lf_a1: 0.0,
            lf_x1: 0.0, lf_y1: 0.0, loop_gain: 0.999,
            ap_coeff: 0.0, ap_x1: 0.0, ap_y1: 0.0,
            disp_coeff: 0.0, disp_x1: 0.0, disp_y1: 0.0,
            buffer2: Vec::new(), pos2: 0, sym_filter: 0.0, sym_gain: 0.15,
            pickup_bridge_tap: 0, pickup_neck_tap: 0,
            pickup_config: PickupConfig::Bridge, pickup_blend: 0.5,
            pu_lp: 0.0, pu_bp: 0.0, pu_freq: 7000.0, pu_q: 3.0,
            body_modes: [
                BodyMode::new(95.0, 2.0, 4.0),
                BodyMode::new(250.0, 2.5, 3.0),
                BodyMode::new(950.0, 4.0, 2.0),
            ],
            body_mix: 0.3,
            fret_height: 0.0, fret_damping: 0.9,
            slap_click_amp: 0.0, noise_state: 0xBA55BA55u32,
            time: 0.0, sample_rate: 44100.0,
        }
    }

    /// Return internal buffers for reuse by the pool.
    pub fn return_buffers(self, pool_a: &mut Vec<f32>, pool_b: &mut Vec<f32>) {
        *pool_a = self.buffer;
        *pool_b = self.buffer2;
    }

    /// Initialize with pre-allocated buffers (no heap allocation).
    pub fn init_with_buffers(
        &mut self,
        freq: f32,
        velocity: f32,
        style: f32,
        tone: f32,
        body: f32,
        pickup: f32,
        sample_rate: f32,
        buf_a: Vec<f32>,
        buf_b: Vec<f32>,
    ) {
        self.buffer = buf_a;
        self.buffer2 = buf_b;
        self.init_inner(freq, velocity, style, tone, body, pickup, sample_rate);
    }

    fn init_inner(
        &mut self,
        freq: f32,
        velocity: f32,
        style: f32,
        tone: f32,
        body: f32,
        pickup: f32,
        sample_rate: f32,
    ) {
        self.sample_rate = sample_rate;
        self.time = 0.0;
        let style = BassStyle::from_param(style);
        self.pickup_config = PickupConfig::from_param(pickup);

        // --- Delay line ---
        // Account for filter group delay (~1 sample for loss filter + allpass)
        let delay_total = sample_rate / freq;
        let filter_delay = 1.0; // approximate group delay of loop filter chain
        let delay_corrected = delay_total - filter_delay;
        let n = (delay_corrected as usize).max(2);
        let frac = delay_corrected - n as f32;
        self.ap_coeff = (1.0 - frac) / (1.0 + frac);
        self.ap_x1 = 0.0;
        self.ap_y1 = 0.0;

        // --- Dispersion (inharmonicity) ---
        // B ≈ 0.0001 for low E, scales with string/freq
        let b = 0.00015 * (1.0 + (freq / 100.0 - 1.0).max(0.0) * 0.5);
        let sqrt_b = b.sqrt();
        self.disp_coeff = -(1.0 - sqrt_b) / (1.0 + sqrt_b);
        self.disp_x1 = 0.0;
        self.disp_y1 = 0.0;

        // --- Välimäki robust loss filter ---
        // Design: match T60 at DC and at 4 kHz
        let t60_dc = 8.0 + (1.0 - freq / 100.0).max(0.0) * 4.0; // 8-12s for low notes
        let brightness_factor = 0.3 + tone * 0.6; // tone knob effect
        let t60_hf = (0.3 + brightness_factor * 0.5) * (1.0 + (freq / 200.0).min(1.0) * 0.5);

        // Per-sample gains at DC and target freq
        let g_dc = 10.0_f32.powf(-3.0 / (sample_rate * t60_dc));
        let g_hf = 10.0_f32.powf(-3.0 / (sample_rate * t60_hf));

        // One-zero/one-pole: H(z) = g * (b0 + b1*z^-1) / (1 - a1*z^-1)
        // At DC: g * (b0+b1)/(1-a1) = g_dc^N
        // At pi: g * (b0-b1)/(1+a1) = g_hf^N
        let target_dc = g_dc.powf(n as f32);
        let target_hf = g_hf.powf(n as f32);

        // Solve for coefficients
        // Simplified: use one-pole for now with corrected gain
        let rho = target_hf / target_dc;
        let a1 = -(1.0 - rho) / (1.0 + rho);
        let a1 = a1.clamp(-0.95, 0.95);
        let b0 = (1.0 + a1) * 0.5;
        let b1 = b0; // symmetric = LP
        let gain = target_dc / ((b0 + b1) / (1.0 - a1));
        let gain = gain.clamp(0.9, 1.0);

        self.lf_b0 = b0;
        self.lf_b1 = b1;
        self.lf_a1 = a1;
        self.lf_x1 = 0.0;
        self.lf_y1 = 0.0;
        self.loop_gain = gain;

        // --- Pickup positions ---
        self.pickup_bridge_tap = ((n as f32 * 0.08) as usize).max(1);
        self.pickup_neck_tap = ((n as f32 * 0.20) as usize).max(1);
        self.pickup_blend = 0.5;

        // --- Pickup electrical resonance ---
        match self.pickup_config {
            PickupConfig::Bridge => {
                // Single-coil J-bass bridge: bright
                self.pu_freq = 8000.0 + tone * 2000.0;
                self.pu_q = 3.0;
            }
            PickupConfig::Neck => {
                // Split-coil P-bass: darker
                self.pu_freq = 5500.0 + tone * 1500.0;
                self.pu_q = 2.5;
            }
            PickupConfig::Both => {
                // Blended J-bass
                self.pu_freq = 7000.0 + tone * 1500.0;
                self.pu_q = 2.8;
            }
        }
        self.pu_lp = 0.0;
        self.pu_bp = 0.0;

        // --- Body resonance ---
        self.body_mix = body * 0.35;
        self.body_modes = [
            BodyMode::new(
                (80.0 + freq * 0.2).min(130.0),
                2.0,
                3.5,
            ),
            BodyMode::new(
                (200.0 + freq * 0.3).min(350.0),
                2.5,
                2.5,
            ),
            BodyMode::new(
                950.0,
                4.0,
                2.0,
            ),
        ];

        // --- Fret collision (slap only) ---
        self.fret_height = if style == BassStyle::Slap { 0.05 } else { 0.0 };
        self.fret_damping = 0.85;

        // --- Excitation ---
        self.buffer.resize(n, 0.0);
        self.buffer.fill(0.0);
        self.slap_click_amp = 0.0;

        match style {
            BassStyle::Finger => {
                // Pure half-sine displacement at 1/7 string position
                let strike_pos = n / 7;
                let excite_len = (n / 4).max(4);
                for i in 0..excite_len {
                    let w = (PI * i as f32 / excite_len as f32).sin();
                    let idx = (strike_pos + i) % n;
                    self.buffer[idx] = w * velocity * 0.85;
                }
            }
            BassStyle::Pick => {
                // Narrow triangular + pick click
                let strike_pos = n / 10;
                let excite_len = (n / 12).max(3);

                // Pick click: 0.8ms noise burst at strike point
                let click_len = ((sample_rate * 0.0008) as usize).min(excite_len).max(2);
                for i in 0..click_len {
                    let noise = self.next_noise();
                    let env = 1.0 - (i as f32 / click_len as f32);
                    let idx = (strike_pos + i) % n;
                    self.buffer[idx] += noise * env * velocity * 0.15;
                }

                // Main triangular excitation
                for i in 0..excite_len {
                    let t = i as f32 / excite_len as f32;
                    let w = if t < 0.3 { t / 0.3 } else { (1.0 - t) / 0.7 };
                    let idx = (strike_pos + i) % n;
                    self.buffer[idx] += w * velocity * 0.9;
                }
            }
            BassStyle::Slap => {
                // Velocity injection (not displacement) at 1/12 position
                let strike_pos = n / 12;
                let slap_len = ((sample_rate * 0.0015) as usize).min(n / 3).max(2);

                for i in 0..slap_len {
                    let env = 1.0 - (i as f32 / slap_len as f32);
                    let noise = self.next_noise();
                    let idx = (strike_pos + i) % n;
                    // Velocity injection: energy into both wave directions
                    self.buffer[idx] = (env * 0.7 + noise * 0.3) * velocity * 1.1;
                }

                // Percussive click
                self.slap_click_amp = velocity * 0.5;
            }
        }

        // --- Sympathetic string (slight detune for thickness) ---
        let freq2 = freq * 2.0_f32.powf(0.6 / 1200.0);
        let n2 = (sample_rate / freq2) as usize;
        let n2 = n2.max(2);
        self.buffer2.resize(n2, 0.0);
        self.buffer2.fill(0.0);
        // Seed sympathetic from main string (quieter)
        for i in 0..n2.min(n) {
            self.buffer2[i] = self.buffer[i] * 0.15;
        }
        self.pos = 0;
        self.pos2 = 0;
        self.sym_filter = 0.0;
        self.sym_gain = 0.12;
    }

    #[inline]
    fn next_noise(&mut self) -> f32 {
        self.noise_state ^= self.noise_state << 13;
        self.noise_state ^= self.noise_state >> 17;
        self.noise_state ^= self.noise_state << 5;
        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    pub fn tick(&mut self) -> f32 {
        let n = self.buffer.len();
        if n == 0 {
            return 0.0;
        }

        let sr = self.sample_rate;
        self.time += 1.0 / sr;

        // === Read from delay line ===
        let sample = self.buffer[self.pos];

        // === Pickup position comb filter ===
        let pickup_out = match self.pickup_config {
            PickupConfig::Bridge => {
                let tap = (self.pos + n - self.pickup_bridge_tap) % n;
                sample - self.buffer[tap]
            }
            PickupConfig::Neck => {
                let tap = (self.pos + n - self.pickup_neck_tap) % n;
                sample - self.buffer[tap]
            }
            PickupConfig::Both => {
                let tap_b = (self.pos + n - self.pickup_bridge_tap) % n;
                let tap_n = (self.pos + n - self.pickup_neck_tap) % n;
                let bridge = sample - self.buffer[tap_b];
                let neck = sample - self.buffer[tap_n];
                bridge * (1.0 - self.pickup_blend) + neck * self.pickup_blend
            }
        };

        // === Loop filter: Välimäki one-zero/one-pole ===
        let lf_out = self.lf_b0 * sample + self.lf_b1 * self.lf_x1 + self.lf_a1 * self.lf_y1;
        self.lf_x1 = sample;
        self.lf_y1 = lf_out;

        // === Dispersion allpass ===
        let d = self.disp_coeff;
        let disp_out = d * (lf_out - self.disp_y1) + self.disp_x1;
        self.disp_x1 = lf_out;
        self.disp_y1 = disp_out;

        // === Fractional delay allpass (tuning) ===
        let c = self.ap_coeff;
        let ap_out = c * (disp_out - self.ap_y1) + self.ap_x1;
        self.ap_x1 = disp_out;
        self.ap_y1 = ap_out;

        // === Fret collision (slap buzz) ===
        let mut loop_out = ap_out * self.loop_gain;
        if self.fret_height > 0.0 && loop_out.abs() > self.fret_height {
            // Reflect excess displacement, apply collision damping
            let excess = loop_out.abs() - self.fret_height;
            loop_out = loop_out.signum() * (self.fret_height - excess * self.fret_damping);
        }

        // Write back to delay line
        self.buffer[self.pos] = loop_out;
        self.pos = (self.pos + 1) % n;

        // === Sympathetic string ===
        let sym_out = if !self.buffer2.is_empty() {
            let n2 = self.buffer2.len();
            let s2 = self.buffer2[self.pos2];
            // Simple one-pole LP + damping
            self.sym_filter += 0.4 * (s2 - self.sym_filter);
            self.buffer2[self.pos2] = self.sym_filter * 0.997;
            self.pos2 = (self.pos2 + 1) % n2;
            s2 * self.sym_gain
        } else {
            0.0
        };

        // === Slap click (percussive transient, decays outside loop) ===
        let click = if self.slap_click_amp > 0.001 {
            let noise = self.next_noise();
            let env = self.slap_click_amp * fast_exp(-self.time * 200.0);
            if env < 0.001 { self.slap_click_amp = 0.0; }
            noise * env
        } else {
            0.0
        };

        // === Combine string outputs ===
        let raw = pickup_out + sym_out + click;

        // === Pickup electrical resonance (SVF resonant LP) ===
        let w_pu = (PI * self.pu_freq / sr).sin() * 2.0;
        let q_inv = 1.0 / self.pu_q;
        let hp = raw - self.pu_lp - q_inv * self.pu_bp;
        self.pu_bp += w_pu * hp;
        self.pu_lp += w_pu * self.pu_bp;
        // Mix: mostly LP (like a real pickup) with some resonant peak
        let pickup_filtered = self.pu_lp;

        // === Body resonance (3 parallel modes, mixed in) ===
        let body_out = if self.body_mix > 0.001 {
            let mut b = 0.0_f32;
            for mode in &mut self.body_modes {
                b += mode.tick(pickup_filtered, sr);
            }
            b * self.body_mix
        } else {
            0.0
        };

        let out = pickup_filtered + body_out;
        out.clamp(-1.0, 1.0)
    }
}

/// Fast exponential approximation for negative arguments.
#[inline]
fn fast_exp(x: f32) -> f32 {
    let x = x.max(-20.0);
    let mut y = 1.0 + x / 256.0;
    y *= y; y *= y; y *= y; y *= y;
    y *= y; y *= y; y *= y; y *= y;
    y.max(0.0)
}
