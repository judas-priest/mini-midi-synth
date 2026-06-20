//! Dattorro plate reverb (from "Effect Design Part 1" by Jon Dattorro).
//! Figure-8 recirculating allpass network with modulated tank for smooth,
//! dense reverb with excellent stereo image.

use super::dsp_utils::advance_phase;

/// Fast sin approximation for LFO use (Bhaskara I formula). Max error ~0.17%.
#[inline(always)]
fn fast_sin(x: f32) -> f32 {
    // Map x (0..1 phase) to radians and use Bhaskara approximation
    // Phase is 0..1 representing 0..2π
    let x = x - x.floor(); // ensure 0..1
    let t = x * 4.0;
    let (t, sign) = if t < 2.0 { (t, 1.0_f32) } else { (t - 2.0, -1.0_f32) };
    let t = if t > 1.0 { 2.0 - t } else { t }; // 0..1 in half-cycle
    sign * (4.0 * t * (1.0 - t)) / (0.225 + t * (1.0 - t) * 3.55)
}

const REFERENCE_SR: f32 = 29761.0;

// Input diffuser delay lengths and coefficients (at 29761 Hz)
const INPUT_AP_DELAYS: [usize; 4] = [142, 107, 379, 277];
const INPUT_AP_COEFFS: [f32; 4] = [0.750, 0.750, 0.625, 0.625];

// Tank delay lengths (at 29761 Hz)
const MOD_AP_L_DELAY: usize = 672;
const DELAY_L1_LEN: usize = 4453;
const AP_L2_DELAY: usize = 1800;
const DELAY_L2_LEN: usize = 3720;

const MOD_AP_R_DELAY: usize = 908;
const DELAY_R1_LEN: usize = 4217;
const AP_R2_DELAY: usize = 2656;
const DELAY_R2_LEN: usize = 3163;

const MOD_AP_COEFF: f32 = 0.70;
const TANK_AP2_COEFF: f32 = 0.50;

// LFO excursion at reference rate
const LFO_EXCURSION_BASE: f32 = 8.0;
// Two rates with irrational ratio to prevent periodic artifacts (FV-1 approach)
const LFO_RATE_L_HZ: f32 = 0.97;
const LFO_RATE_R_HZ: f32 = 1.13;

// Output tap positions (at 29761 Hz)
// Left output: taps from R and L delay lines
const TAP_L_DR1_A: usize = 266;
const TAP_L_DR1_B: usize = 2974;
const TAP_L_APR2: usize = 1913; // subtract
const TAP_L_DR2: usize = 1996;
const TAP_L_DL1: usize = 1990; // subtract
const TAP_L_APL2: usize = 187;  // subtract
const TAP_L_DL2: usize = 1066; // subtract

// Right output: taps from L and R delay lines
const TAP_R_DL1_A: usize = 353;
const TAP_R_DL1_B: usize = 3627;
const TAP_R_APL2: usize = 1228; // subtract
const TAP_R_DL2: usize = 2673;
const TAP_R_DR1: usize = 2111; // subtract
const TAP_R_APR2: usize = 335;  // subtract
const TAP_R_DR2: usize = 121;  // subtract

const OUTPUT_SCALE: f32 = 0.6;

fn scale_len(base: usize, sr: f32) -> usize {
    ((base as f32 * sr / REFERENCE_SR) as usize).max(1)
}

// ---------------------------------------------------------------------------
// Allpass filter
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DattorroAP {
    buffer: Vec<f32>,
    pos: usize,
    coeff: f32,
}

impl DattorroAP {
    fn new(len: usize, coeff: f32) -> Self {
        Self {
            buffer: vec![0.0; len.max(1)],
            pos: 0,
            coeff,
        }
    }

    #[inline]
    fn tick(&mut self, input: f32) -> f32 {
        let delayed = self.buffer[self.pos];
        let v = input - self.coeff * delayed;
        self.buffer[self.pos] = v;
        self.pos += 1;
        if self.pos >= self.buffer.len() {
            self.pos = 0;
        }
        delayed + self.coeff * v
    }

    /// Modulated tick: reads from a fractionally-offset position using linear
    /// interpolation, then writes at the current position.
    #[inline]
    fn tick_mod(&mut self, input: f32, lfo_offset: f32) -> f32 {
        let len = self.buffer.len();
        // Nominal read position (where we'd normally read)
        // Read from `len` samples back, offset by LFO modulation
        let read_f = self.pos as f32 + (len as f32) - lfo_offset;
        let read_floor = read_f as usize % len;
        let frac = read_f - read_f.floor();
        let a = self.buffer[read_floor];
        let b = self.buffer[(read_floor + 1) % len];
        let delayed = a + frac * (b - a);

        let v = input - self.coeff * delayed;
        self.buffer[self.pos] = v;
        self.pos += 1;
        if self.pos >= len {
            self.pos = 0;
        }
        delayed + self.coeff * v
    }

    /// Read from the buffer at `offset` samples behind the write head.
    #[inline]
    fn read_at(&self, offset: usize) -> f32 {
        let len = self.buffer.len();
        let idx = (self.pos + len - offset.min(len)) % len;
        self.buffer[idx]
    }
}

// ---------------------------------------------------------------------------
// Simple delay line with read_at
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DelayLine {
    buffer: Vec<f32>,
    pos: usize,
}

impl DelayLine {
    fn new(len: usize) -> Self {
        Self {
            buffer: vec![0.0; len.max(1)],
            pos: 0,
        }
    }

    #[inline]
    fn push(&mut self, sample: f32) {
        self.buffer[self.pos] = sample;
        self.pos += 1;
        if self.pos >= self.buffer.len() {
            self.pos = 0;
        }
    }

    /// Read the sample that is `offset` samples behind the write head.
    #[inline]
    fn read_at(&self, offset: usize) -> f32 {
        let len = self.buffer.len();
        let idx = (self.pos + len - offset.min(len)) % len;
        self.buffer[idx]
    }

    /// Read the oldest sample in the buffer (at current write position, about to be overwritten).
    /// This is the sample that has traveled through the entire delay line.
    #[inline]
    fn oldest(&self) -> f32 {
        self.buffer[self.pos]
    }
}

// ---------------------------------------------------------------------------
// Pre-computed output tap indices (scaled to current sample rate)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DattorroTaps {
    // Left output
    l_dr1_a: usize,
    l_dr1_b: usize,
    l_apr2: usize,
    l_dr2: usize,
    l_dl1: usize,
    l_apl2: usize,
    l_dl2: usize,
    // Right output
    r_dl1_a: usize,
    r_dl1_b: usize,
    r_apl2: usize,
    r_dl2: usize,
    r_dr1: usize,
    r_apr2: usize,
    r_dr2: usize,
}

impl DattorroTaps {
    fn new(sr: f32) -> Self {
        Self {
            l_dr1_a: scale_len(TAP_L_DR1_A, sr),
            l_dr1_b: scale_len(TAP_L_DR1_B, sr),
            l_apr2: scale_len(TAP_L_APR2, sr),
            l_dr2: scale_len(TAP_L_DR2, sr),
            l_dl1: scale_len(TAP_L_DL1, sr),
            l_apl2: scale_len(TAP_L_APL2, sr),
            l_dl2: scale_len(TAP_L_DL2, sr),
            r_dl1_a: scale_len(TAP_R_DL1_A, sr),
            r_dl1_b: scale_len(TAP_R_DL1_B, sr),
            r_apl2: scale_len(TAP_R_APL2, sr),
            r_dl2: scale_len(TAP_R_DL2, sr),
            r_dr1: scale_len(TAP_R_DR1, sr),
            r_apr2: scale_len(TAP_R_APR2, sr),
            r_dr2: scale_len(TAP_R_DR2, sr),
        }
    }
}

// ---------------------------------------------------------------------------
// Dattorro Plate Reverb
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Reverb {
    sample_rate: f32,
    // Input bandwidth filter state
    bw_state: f32,
    // Pre-delay
    pre_delay_buf: Vec<f32>,
    pre_delay_pos: usize,
    // Input diffusers (4 allpass cascade)
    input_ap: [DattorroAP; 4],
    // Left tank
    mod_ap_l: DattorroAP,
    delay_l1: DelayLine,
    damp_state_l: f32,
    ap_l2: DattorroAP,
    delay_l2: DelayLine,
    // Right tank
    mod_ap_r: DattorroAP,
    delay_r1: DelayLine,
    damp_state_r: f32,
    ap_r2: DattorroAP,
    delay_r2: DelayLine,
    // Tank feedback
    left_out: f32,
    right_out: f32,
    // Modulation LFOs (two with irrational ratio for smoother modulation)
    lfo_phase_l: f32,
    lfo_phase_r: f32,
    lfo_excursion: f32,
    // Output taps
    taps: DattorroTaps,
}

impl Reverb {
    pub fn new(sample_rate: f32) -> Self {
        let input_ap = std::array::from_fn(|i| {
            DattorroAP::new(scale_len(INPUT_AP_DELAYS[i], sample_rate), INPUT_AP_COEFFS[i])
        });

        // Max pre-delay ~100ms
        let pre_delay_max = (sample_rate * 0.1) as usize + 1;

        Self {
            sample_rate,
            bw_state: 0.0,
            pre_delay_buf: vec![0.0; pre_delay_max],
            pre_delay_pos: 0,
            input_ap,
            // Left tank
            mod_ap_l: DattorroAP::new(scale_len(MOD_AP_L_DELAY, sample_rate), MOD_AP_COEFF),
            delay_l1: DelayLine::new(scale_len(DELAY_L1_LEN, sample_rate)),
            damp_state_l: 0.0,
            ap_l2: DattorroAP::new(scale_len(AP_L2_DELAY, sample_rate), TANK_AP2_COEFF),
            delay_l2: DelayLine::new(scale_len(DELAY_L2_LEN, sample_rate)),
            // Right tank
            mod_ap_r: DattorroAP::new(scale_len(MOD_AP_R_DELAY, sample_rate), MOD_AP_COEFF),
            delay_r1: DelayLine::new(scale_len(DELAY_R1_LEN, sample_rate)),
            damp_state_r: 0.0,
            ap_r2: DattorroAP::new(scale_len(AP_R2_DELAY, sample_rate), TANK_AP2_COEFF),
            delay_r2: DelayLine::new(scale_len(DELAY_R2_LEN, sample_rate)),
            // Feedback
            left_out: 0.0,
            right_out: 0.0,
            // LFO
            lfo_phase_l: 0.0,
            lfo_phase_r: 0.25, // start 90° offset
            lfo_excursion: LFO_EXCURSION_BASE * sample_rate / REFERENCE_SR,
            // Taps
            taps: DattorroTaps::new(sample_rate),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        *self = Self::new(sr);
    }

    /// Process one stereo sample.
    /// `room_size` 0..1, `damping` 0..1, `width` 0..1, `pre_delay` in seconds, `mix` 0..1.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        room_size: f32,
        damping: f32,
        width: f32,
        pre_delay: f32,
        mix: f32,
    ) -> (f32, f32) {
        if mix < 0.001 {
            return (in_l, in_r);
        }

        let input = (in_l + in_r) * 0.5;

        // --- Pre-delay ---
        let pd_samples = ((pre_delay * self.sample_rate) as usize)
            .min(self.pre_delay_buf.len() - 1);
        let pd_len = self.pre_delay_buf.len();
        let pd_read = {
            let p = self.pre_delay_pos + pd_len - pd_samples;
            if p >= pd_len { p - pd_len } else { p }
        };
        let delayed_input = self.pre_delay_buf[pd_read];
        self.pre_delay_buf[self.pre_delay_pos] = input;
        self.pre_delay_pos += 1;
        if self.pre_delay_pos >= pd_len {
            self.pre_delay_pos = 0;
        }

        // --- Input bandwidth filter (one-pole LPF) ---
        // bandwidth: higher = brighter input. Fixed at 0.9995 for musical results.
        let bw = 0.9995_f32;
        self.bw_state = bw * delayed_input + (1.0 - bw) * self.bw_state;
        let mut sig = self.bw_state;

        // --- Input diffusers (4 allpass cascade) ---
        for ap in &mut self.input_ap {
            sig = ap.tick(sig);
        }
        let diffused = sig;

        // --- Decay parameter ---
        let decay = room_size * 0.9999; // 0..~0.9999

        // --- Damping coefficient ---
        let damp = damping * 0.9; // 0..0.9

        // --- LFO (two independent LFOs with irrational rate ratio) ---
        advance_phase(&mut self.lfo_phase_l, LFO_RATE_L_HZ, self.sample_rate);
        advance_phase(&mut self.lfo_phase_r, LFO_RATE_R_HZ, self.sample_rate);
        let lfo_sin = fast_sin(self.lfo_phase_l);
        let lfo_cos = fast_sin(self.lfo_phase_r);

        let excursion_l = self.lfo_excursion * lfo_sin;
        let excursion_r = self.lfo_excursion * lfo_cos;

        // --- Left tank ---
        // Input: diffused signal + decayed feedback from right tank
        let tank_in_l = diffused + decay * self.right_out;
        let mod_ap_out_l = self.mod_ap_l.tick_mod(tank_in_l, excursion_l);
        self.delay_l1.push(mod_ap_out_l);
        let dl1_out = self.delay_l1.oldest();
        // Damping LPF
        self.damp_state_l = (1.0 - damp) * dl1_out + damp * self.damp_state_l;
        let damped_l = self.damp_state_l * decay;
        let ap_l2_out = self.ap_l2.tick(damped_l);
        self.delay_l2.push(ap_l2_out);
        // The output that feeds to the right tank is the end of delay_l2
        self.left_out = self.delay_l2.oldest();

        // --- Right tank ---
        let tank_in_r = diffused + decay * self.left_out;
        let mod_ap_out_r = self.mod_ap_r.tick_mod(tank_in_r, excursion_r);
        self.delay_r1.push(mod_ap_out_r);
        let dr1_out = self.delay_r1.oldest();
        // Damping LPF
        self.damp_state_r = (1.0 - damp) * dr1_out + damp * self.damp_state_r;
        let damped_r = self.damp_state_r * decay;
        let ap_r2_out = self.ap_r2.tick(damped_r);
        self.delay_r2.push(ap_r2_out);
        self.right_out = self.delay_r2.oldest();

        // --- Output taps ---
        let t = &self.taps;

        let out_l = self.delay_r1.read_at(t.l_dr1_a)
            + self.delay_r1.read_at(t.l_dr1_b)
            - self.ap_r2.read_at(t.l_apr2)
            + self.delay_r2.read_at(t.l_dr2)
            - self.delay_l1.read_at(t.l_dl1)
            - self.ap_l2.read_at(t.l_apl2)
            - self.delay_l2.read_at(t.l_dl2);

        let out_r = self.delay_l1.read_at(t.r_dl1_a)
            + self.delay_l1.read_at(t.r_dl1_b)
            - self.ap_l2.read_at(t.r_apl2)
            + self.delay_l2.read_at(t.r_dl2)
            - self.delay_r1.read_at(t.r_dr1)
            - self.ap_r2.read_at(t.r_apr2)
            - self.delay_r2.read_at(t.r_dr2);

        let wet_l = out_l * OUTPUT_SCALE;
        let wet_r = out_r * OUTPUT_SCALE;

        // --- Stereo width crossmix ---
        let final_l = wet_l * (0.5 + width * 0.5) + wet_r * (0.5 - width * 0.5);
        let final_r = wet_r * (0.5 + width * 0.5) + wet_l * (0.5 - width * 0.5);

        // --- Dry/wet blend ---
        let out_l = in_l * (1.0 - mix) + final_l * mix;
        let out_r = in_r * (1.0 - mix) + final_r * mix;
        (out_l, out_r)
    }
}
