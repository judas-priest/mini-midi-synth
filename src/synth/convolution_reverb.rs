/// Convolution Reverb — convolves audio with a built-in room impulse response.
/// Uses direct convolution with a short IR (≤512 samples = ~12ms at 44.1kHz).
/// For longer reverb, use the reverb or reverb2 effects.
///
/// The built-in IR approximates a small-medium room with:
/// - Early reflections at ~3ms, ~7ms, ~11ms (samples 132, 308, 485 at 44.1kHz)
/// - Exponential decay envelope
/// - Bandpass character (low-cut ~100Hz, high-cut ~6kHz)
///
/// Parameters:
/// - `room_size` (0..1): scales the IR length used (0=64 samples, 1=512 samples)
/// - `damping` (0..1): attenuates the late tail, favouring early reflections
/// - `pre_delay` (0..1): pre-delay 0..40ms (0..1764 samples at 44.1kHz)
/// - `mix` (0..1): wet/dry

const IR_LEN: usize = 512;
/// Maximum pre-delay: 40ms at 44.1kHz
const MAX_PRE_DELAY: usize = 1764;

pub struct ConvolutionReverb {
    sample_rate: f32,
    /// Built-in impulse response (generated procedurally)
    ir: [f32; IR_LEN],
    /// Damped IR cache — rebuilt only when damping or ir_len changes
    damped_ir: Box<[f32; IR_LEN]>,
    /// Damping value used when `damped_ir` was last built (sentinel -1.0 = never built)
    cached_damping: f32,
    /// IR length used when `damped_ir` was last built
    cached_ir_len: usize,
    /// Left channel input history ring buffer (length = IR_LEN)
    hist_l: [f32; IR_LEN],
    /// Right channel input history ring buffer
    hist_r: [f32; IR_LEN],
    /// Write position in history buffers
    hist_pos: usize,
    /// Pre-delay line (left)
    pre_l: [f32; MAX_PRE_DELAY],
    /// Pre-delay line (right)
    pre_r: [f32; MAX_PRE_DELAY],
    /// Write position in the pre-delay buffers
    pre_pos: usize,
    /// LCG PRNG state for IR generation
    rng_state: u64,
}

impl ConvolutionReverb {
    pub fn new(sr: f32) -> Self {
        let mut cr = ConvolutionReverb {
            sample_rate: sr,
            ir: [0.0f32; IR_LEN],
            damped_ir: Box::new([0.0f32; IR_LEN]),
            cached_damping: -1.0,
            cached_ir_len: 0,
            hist_l: [0.0f32; IR_LEN],
            hist_r: [0.0f32; IR_LEN],
            hist_pos: 0,
            pre_l: [0.0f32; MAX_PRE_DELAY],
            pre_r: [0.0f32; MAX_PRE_DELAY],
            pre_pos: 0,
            rng_state: 0xDEADBEEFCAFEBABEu64,
        };
        cr.generate_ir();
        // Copy the raw IR into damped_ir as the initial state
        cr.damped_ir.copy_from_slice(&cr.ir);
        cr
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        // IR is defined in samples relative to 44100 Hz, so no need to regenerate
        // unless the caller wants SR-dependent scaling (not implemented here).
    }

    fn next_rand(&mut self) -> f32 {
        self.rng_state = self.rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.rng_state >> 33) as f32 / u32::MAX as f32
    }

    /// Generate the built-in room IR procedurally.
    ///
    /// Steps:
    /// 1. Decaying bandpass noise via LCG
    /// 2. Early reflection spikes at 132, 308, 485 samples (~3ms, ~7ms, ~11ms at 44.1kHz)
    /// 3. Exponential decay envelope: exp(-6 * n / N)
    /// 4. 1-pole bandpass shaping (low-cut 100Hz, high-cut 6kHz)
    /// 5. Normalise peak to 0.1
    fn generate_ir(&mut self) {
        let n = IR_LEN;

        // Step 1: generate raw noise with exponential decay
        let mut raw = [0.0_f32; IR_LEN];
        for i in 0..n {
            let noise = self.next_rand() * 2.0 - 1.0; // -1..1
            let decay = (-6.0 * i as f32 / n as f32).exp();
            raw[i] = noise * decay;
        }

        // Step 2: early reflection spikes
        // Use amplitude matching the local noise level at those positions
        const REFLECTIONS: [(usize, f32); 3] = [
            (132, 0.8),  // ~3ms
            (308, 0.55), // ~7ms
            (485, 0.30), // ~11ms
        ];
        for (tap, amp) in REFLECTIONS.iter() {
            if *tap < n {
                let decay = (-6.0 * *tap as f32 / n as f32).exp();
                raw[*tap] += amp * decay;
            }
        }

        // Step 3 is already baked into step 1 (decay applied per sample).

        // Step 4a: high-pass filter at ~100Hz (removes DC / low rumble)
        // 1-pole HP: y[n] = alpha * (y[n-1] + x[n] - x[n-1])
        // alpha = 1 / (1 + 2*pi*fc/sr)  — bilinear-ish, use constant for 44.1kHz
        // at 44100: alpha_hp ≈ 0.9857 for 100Hz
        let alpha_hp = 0.9857_f32;
        let mut hp_prev_in = 0.0_f32;
        let mut hp_prev_out = 0.0_f32;
        for i in 0..n {
            let x = raw[i];
            let y = alpha_hp * (hp_prev_out + x - hp_prev_in);
            hp_prev_in = x;
            hp_prev_out = y;
            raw[i] = y;
        }

        // Step 4b: low-pass filter at ~6kHz (softens the IR)
        // 1-pole LP: y[n] = (1-alpha)*x[n] + alpha*y[n-1]
        // at 44100: alpha_lp ≈ 0.5560 for 6000Hz (alpha = exp(-2*pi*fc/sr))
        let alpha_lp = (-2.0 * std::f32::consts::PI * 6000.0_f32 / 44100.0_f32).exp();
        let mut lp_prev = 0.0_f32;
        for i in 0..n {
            let y = (1.0 - alpha_lp) * raw[i] + alpha_lp * lp_prev;
            lp_prev = y;
            raw[i] = y;
        }

        // Step 5: normalise peak to 0.1
        let peak = raw.iter().cloned().map(f32::abs).fold(0.0_f32, f32::max);
        if peak > 1e-10 {
            let scale = 0.1 / peak;
            for i in 0..n {
                self.ir[i] = raw[i] * scale;
            }
        } else {
            self.ir.copy_from_slice(&raw);
        }
    }

    /// Rebuild `damped_ir` for the given damping and IR length.
    /// Called lazily in `tick()` only when parameters change.
    fn rebuild_damped_ir(&mut self, damping: f32, ir_len: usize) {
        for k in 0..IR_LEN {
            let damp_factor = if damping > 1e-6 && k < ir_len {
                (-damping * 5.0 * k as f32 / ir_len as f32).exp()
            } else {
                1.0
            };
            self.damped_ir[k] = self.ir[k] * damp_factor;
        }
        self.cached_damping = damping;
        self.cached_ir_len = ir_len;
    }

    /// Process one stereo sample through the convolution reverb.
    ///
    /// - `room_size` (0..1): fraction of the IR to use (0=64 samples, 1=512 samples)
    /// - `damping` (0..1): attenuates the late portion of the IR
    /// - `pre_delay` (0..1): pre-delay 0..40ms
    /// - `mix` (0..1): wet/dry ratio
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        room_size: f32,
        damping: f32,
        pre_delay: f32,
        mix: f32,
    ) -> (f32, f32) {
        // --- Pre-delay ---
        // Compute read position in pre-delay buffer
        let pre_samples = ((pre_delay * 40.0 * self.sample_rate / 1000.0) as usize)
            .min(MAX_PRE_DELAY - 1);
        let read_pos = (self.pre_pos + MAX_PRE_DELAY - pre_samples) % MAX_PRE_DELAY;
        let pre_out_l = self.pre_l[read_pos];
        let pre_out_r = self.pre_r[read_pos];

        // Write current input into pre-delay
        self.pre_l[self.pre_pos] = in_l;
        self.pre_r[self.pre_pos] = in_r;
        self.pre_pos = (self.pre_pos + 1) % MAX_PRE_DELAY;

        // --- Write pre-delayed signal into convolution history ---
        self.hist_l[self.hist_pos] = pre_out_l;
        self.hist_r[self.hist_pos] = pre_out_r;

        // --- Determine IR length from room_size ---
        // room_size: map 0..1 → 64..512 samples
        let ir_len = (64 + (room_size * (IR_LEN - 64) as f32) as usize).min(IR_LEN);

        // Rebuild damped IR cache only when damping or ir_len changes
        if (damping - self.cached_damping).abs() > 1e-6 || ir_len != self.cached_ir_len {
            self.rebuild_damped_ir(damping, ir_len);
        }

        // --- Convolution using the precomputed damped IR ---
        let mut acc_l = 0.0_f32;
        let mut acc_r = 0.0_f32;
        let hist_pos = self.hist_pos;
        for k in 0..ir_len {
            let idx = (hist_pos + IR_LEN - k) % IR_LEN;
            let damp_factor = self.damped_ir[k]; // precomputed
            acc_l += damp_factor * self.hist_l[idx];
            acc_r += damp_factor * self.hist_r[idx];
        }

        self.hist_pos = (self.hist_pos + 1) % IR_LEN;

        let dry_l = in_l * (1.0 - mix);
        let dry_r = in_r * (1.0 - mix);
        (dry_l + acc_l * mix, dry_r + acc_r * mix)
    }
}
