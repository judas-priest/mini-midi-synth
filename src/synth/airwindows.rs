//! Airwindows-inspired DSP algorithms, modes 0-32.
#![allow(dead_code)] // Ported 1:1 from Airwindows C — unused fields are kept for algorithmic completeness.

const HALF_PI_64: f64 = std::f64::consts::FRAC_PI_2;

// ─── helpers ────────────────────────────────────────────────────────────────

#[inline(always)]
fn xorshift(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as f32) / (u32::MAX as f32)
}

// ─── Mode 0: Tape2 ──────────────────────────────────────────────────────────

pub struct AirwindowsTape2 {
    stage_l: f32, stage_r: f32,
    lp_l: f32, lp_r: f32,
    wow_phase: f32,
    sample_rate: f32,
}
impl AirwindowsTape2 {
    pub fn new(sample_rate: f32) -> Self {
        Self { stage_l: 0.0, stage_r: 0.0, lp_l: 0.0, lp_r: 0.0, wow_phase: 0.0, sample_rate }
    }
    pub fn set_sample_rate(&mut self, sr: f32) { self.sample_rate = sr; }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let gain = drive * 3.0 + 1.0;
        let s1_l = (in_l * gain).tanh();
        let s1_r = (in_r * gain).tanh();
        let s2_l = (s1_l * gain * 0.5).tanh() / gain.sqrt();
        let s2_r = (s1_r * gain * 0.5).tanh() / gain.sqrt();
        let lp = 0.85;
        self.lp_l = self.lp_l * lp + s2_l * (1.0 - lp) + 1e-30;
        self.lp_r = self.lp_r * lp + s2_r * (1.0 - lp) + 1e-30;
        let _ = (self.stage_l, self.stage_r, self.wow_phase, self.sample_rate);
        (self.lp_l, self.lp_r)
    }
}

// ─── Mode 1: Density ────────────────────────────────────────────────────────

pub struct AirwindowsDensity;
impl AirwindowsDensity {
    pub fn new() -> Self { Self }
    pub fn tick(&self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let d = drive.clamp(0.0, 1.0);
        let gain = d * 4.0 + 1.0;
        let out_l = (1.0 - d) * in_l + d * (in_l * gain).tanh() / gain;
        let out_r = (1.0 - d) * in_r + d * (in_r * gain).tanh() / gain;
        (out_l, out_r)
    }
}

// ─── Mode 2: Console ────────────────────────────────────────────────────────

pub struct AirwindowsConsole;
impl AirwindowsConsole {
    pub fn new() -> Self { Self }
    pub fn tick(&self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let k = drive * 2.0 + 0.5;
        let out_l = in_l / (1.0 + in_l.abs() * k);
        let out_r = in_r / (1.0 + in_r.abs() * k);
        (out_l, out_r)
    }
}

// ─── Mode 3: ToVinyl4 ───────────────────────────────────────────────────────

pub struct AirwindowsToVinyl4 {
    bass_lp_l: f32, bass_lp_r: f32,
    hf_lp_l: f32, hf_lp_r: f32,
    sample_rate: f32,
}
impl AirwindowsToVinyl4 {
    pub fn new(sample_rate: f32) -> Self {
        Self { bass_lp_l: 0.0, bass_lp_r: 0.0, hf_lp_l: 0.0, hf_lp_r: 0.0, sample_rate }
    }
    pub fn set_sample_rate(&mut self, sr: f32) { self.sample_rate = sr; }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let sr = self.sample_rate;
        let bass_coeff = (-2.0 * std::f32::consts::PI * 35.0 / sr).exp();
        let hf_base = 5000.0 + drive * 2000.0;
        let hf_coeff = (-2.0 * std::f32::consts::PI * hf_base / sr).exp();
        self.bass_lp_l = self.bass_lp_l * bass_coeff + in_l * (1.0 - bass_coeff);
        self.bass_lp_r = self.bass_lp_r * bass_coeff + in_r * (1.0 - bass_coeff);
        self.hf_lp_l = self.hf_lp_l * hf_coeff + in_l * (1.0 - hf_coeff);
        self.hf_lp_r = self.hf_lp_r * hf_coeff + in_r * (1.0 - hf_coeff);
        let out_l = self.hf_lp_l + (self.bass_lp_l * drive * 0.2);
        let out_r = self.hf_lp_r + (self.bass_lp_r * drive * 0.2);
        (out_l, out_r)
    }
}

// ─── Mode 4: Atmosphere ─────────────────────────────────────────────────────

pub struct AirwindowsAtmosphere { hs_l: f32, hs_r: f32 }
impl AirwindowsAtmosphere {
    pub fn new() -> Self { Self { hs_l: 0.0, hs_r: 0.0 } }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let coeff = 0.55 - drive * 0.15;
        self.hs_l = self.hs_l * coeff + in_l * (1.0 - coeff) + 1e-30;
        self.hs_r = self.hs_r * coeff + in_r * (1.0 - coeff) + 1e-30;
        let air_l = in_l - self.hs_l;
        let air_r = in_r - self.hs_r;
        let out_l = in_l + air_l * drive * 0.5 + air_r * drive * 0.1;
        let out_r = in_r + air_r * drive * 0.5 + air_l * drive * 0.1;
        (out_l, out_r)
    }
}

// ─── Mode 5: Pressure5 ──────────────────────────────────────────────────────

pub struct AirwindowsPressure5 {
    env_l: f32, env_r: f32,
    sample_rate: f32,
}
impl AirwindowsPressure5 {
    pub fn new(sample_rate: f32) -> Self {
        Self { env_l: 0.0, env_r: 0.0, sample_rate }
    }
    pub fn set_sample_rate(&mut self, sr: f32) { self.sample_rate = sr; }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let sr = self.sample_rate;
        let att = (-1.0 / (0.00045 * sr)).exp();
        let rel = (-1.0 / (0.00227 * sr)).exp();
        let level_l = in_l.abs();
        let level_r = in_r.abs();
        self.env_l = if level_l > self.env_l { att * self.env_l + (1.0 - att) * level_l }
                     else { rel * self.env_l + (1.0 - rel) * level_l };
        self.env_r = if level_r > self.env_r { att * self.env_r + (1.0 - att) * level_r }
                     else { rel * self.env_r + (1.0 - rel) * level_r };
        let threshold = 1.0 - drive * 0.8;
        let gain_l = if self.env_l > threshold { threshold / self.env_l.max(0.001) } else { 1.0 };
        let gain_r = if self.env_r > threshold { threshold / self.env_r.max(0.001) } else { 1.0 };
        let out_l = (in_l * gain_l * (1.0 + drive * 0.5)).tanh();
        let out_r = (in_r * gain_r * (1.0 + drive * 0.5)).tanh();
        (out_l, out_r)
    }
}

// ─── Mode 6: Drive ──────────────────────────────────────────────────────────

pub struct AirwindowsDrive {
    iir_al: f64, iir_bl: f64,
    iir_ar: f64, iir_br: f64,
    flip: bool,
}
impl AirwindowsDrive {
    pub fn new() -> Self { Self { iir_al: 0.0, iir_bl: 0.0, iir_ar: 0.0, iir_br: 0.0, flip: false } }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let drive_one = (drive as f64 * 2.0).powi(2);
        let iir_amount = (0.3f64).powi(3) / overallscale;
        let glitch: f64 = 0.60;

        let mut sl = in_l as f64;
        let mut sr2 = in_r as f64;

        if self.flip {
            self.iir_al = self.iir_al * (1.0 - iir_amount) + sl * iir_amount;
            sl -= self.iir_al;
            self.iir_ar = self.iir_ar * (1.0 - iir_amount) + sr2 * iir_amount;
            sr2 -= self.iir_ar;
        } else {
            self.iir_bl = self.iir_bl * (1.0 - iir_amount) + sl * iir_amount;
            sl -= self.iir_bl;
            self.iir_br = self.iir_br * (1.0 - iir_amount) + sr2 * iir_amount;
            sr2 -= self.iir_br;
        }
        self.flip = !self.flip;

        sl = sl.clamp(-1.0, 1.0);
        sr2 = sr2.clamp(-1.0, 1.0);

        let mut out = drive_one;
        while out > glitch {
            out -= glitch;
            sl -= sl * (sl.abs() * glitch) * (sl.abs() * glitch);
            sl *= 1.0 + glitch;
            sr2 -= sr2 * (sr2.abs() * glitch) * (sr2.abs() * glitch);
            sr2 *= 1.0 + glitch;
        }
        sl -= sl * (sl.abs() * out) * (sl.abs() * out);
        sl *= 1.0 + out;
        sr2 -= sr2 * (sr2.abs() * out) * (sr2.abs() * out);
        sr2 *= 1.0 + out;

        (sl as f32, sr2 as f32)
    }
}

// ─── Mode 7: HardVacuum ─────────────────────────────────────────────────────

pub struct AirwindowsHardVacuum {
    last_l: f64,
    last_r: f64,
}
impl AirwindowsHardVacuum {
    pub fn new() -> Self { Self { last_l: 0.0, last_r: 0.0 } }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let mut multistage = drive as f64 * 2.0;
        if multistage > 1.0 { multistage *= multistage; }
        let warmth: f64 = 0.2;
        let inv_warmth = 1.0 - warmth;
        let warmth_div = warmth / std::f64::consts::FRAC_PI_2;
        let aura: f64 = 0.4 * std::f64::consts::PI;

        let mut sl = in_l as f64;
        let mut sr = in_r as f64;

        let mut skew_l = sl - self.last_l;
        let mut skew_r = sr - self.last_r;
        self.last_l = sl;
        self.last_r = sr;

        let mut br_l = skew_l.abs().min(std::f64::consts::PI);
        let mut br_r = skew_r.abs().min(std::f64::consts::PI);
        br_l = br_l.sin();
        br_r = br_r.sin();
        skew_l = if skew_l > 0.0 { br_l * aura } else { -br_l * aura };
        skew_r = if skew_r > 0.0 { br_r * aura } else { -br_r * aura };
        skew_l *= sl;
        skew_r *= sr;
        skew_l *= 1.557079633;
        skew_r *= 1.557079633;

        let mut countdown = multistage;
        while countdown > 0.0 {
            let drv = if countdown > 1.0 { 1.557079633 } else { countdown * (1.0 + 0.557079633 * inv_warmth) };
            let positive = drv - warmth_div;
            let negative = drv + warmth_div;

            let mut brl = (sl.abs() + skew_l).min(HALF_PI_64);
            brl = brl.sin() * drv;
            brl = (brl + skew_l).min(HALF_PI_64);
            brl = brl.sin();

            let mut brr = (sr.abs() + skew_r).min(HALF_PI_64);
            brr = brr.sin() * drv;
            brr = (brr + skew_r).min(HALF_PI_64);
            brr = brr.sin();

            if sl > 0.0 {
                sl = sl * (1.0 - positive + skew_l) + brl * (positive + skew_l);
            } else {
                sl = sl * (1.0 - negative + skew_l) - brl * (negative + skew_l);
            }
            if sr > 0.0 {
                sr = sr * (1.0 - positive + skew_r) + brr * (positive + skew_r);
            } else {
                sr = sr * (1.0 - negative + skew_r) - brr * (negative + skew_r);
            }
            countdown -= 1.0;
        }
        (sl as f32, sr as f32)
    }
}

// ─── Mode 8: Spiral2 ────────────────────────────────────────────────────────

pub struct AirwindowsSpiral2 {
    iir_al: f64, iir_bl: f64,
    iir_ar: f64, iir_br: f64,
    prev_l: f64, prev_r: f64,
    flip: bool,
}
impl AirwindowsSpiral2 {
    pub fn new() -> Self {
        Self { iir_al: 0.0, iir_bl: 0.0, iir_ar: 0.0, iir_br: 0.0,
               prev_l: 0.0, prev_r: 0.0, flip: false }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let gain = (drive as f64 * 2.0).powi(2);
        let iir_amount = 0.3f64.powi(3) / overallscale;

        let dry_l = in_l as f64;
        let dry_r = in_r as f64;
        let mut sl = dry_l * gain;
        let mut sr = dry_r * gain;
        let prev_sl = self.prev_l * gain;
        let prev_sr = self.prev_r * gain;

        if self.flip {
            self.iir_al = self.iir_al * (1.0 - iir_amount) + sl * iir_amount;
            sl -= self.iir_al;
            self.iir_ar = self.iir_ar * (1.0 - iir_amount) + sr * iir_amount;
            sr -= self.iir_ar;
        } else {
            self.iir_bl = self.iir_bl * (1.0 - iir_amount) + sl * iir_amount;
            sl -= self.iir_bl;
            self.iir_br = self.iir_br * (1.0 - iir_amount) + sr * iir_amount;
            sr -= self.iir_br;
        }

        let presence_l = if prev_sl == 0.0 { (sl * prev_sl.abs()).sin() } else { (sl * prev_sl.abs()).sin() / prev_sl.abs() };
        let presence_r = if prev_sr == 0.0 { (sr * prev_sr.abs()).sin() } else { (sr * prev_sr.abs()).sin() / prev_sr.abs() };
        let out_l = if sl == 0.0 { sl } else { (sl * sl.abs()).sin() / sl.abs() };
        let out_r = if sr == 0.0 { sr } else { (sr * sr.abs()).sin() / sr.abs() };

        // presence blend 0.0 (no presence)
        self.prev_l = dry_l;
        self.prev_r = dry_r;
        self.flip = !self.flip;

        let _ = (presence_l, presence_r);
        (out_l as f32, out_r as f32)
    }
}

// ─── Mode 9: Fracture ───────────────────────────────────────────────────────

pub struct AirwindowsFracture;
impl AirwindowsFracture {
    pub fn new() -> Self { Self }
    pub fn tick(&self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let mut density = drive as f64 * 4.0;
        density *= density.abs();
        let fracture = ((drive as f64 * 2.999) + 1.0) * std::f64::consts::PI;

        let mut sl = in_l as f64 * density;
        let mut sr = in_r as f64 * density;

        let mut br = sl.abs() * fracture;
        if br > fracture { br = fracture; }
        br = br.sin();
        sl = if sl > 0.0 { br } else { -br };

        br = sr.abs() * fracture;
        if br > fracture { br = fracture; }
        br = br.sin();
        sr = if sr > 0.0 { br } else { -br };

        (sl as f32, sr as f32)
    }
}

// ─── Mode 10: Mojo ──────────────────────────────────────────────────────────

pub struct AirwindowsMojo;
impl AirwindowsMojo {
    pub fn new() -> Self { Self }
    pub fn tick(&self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let gain = 10f64.powf((drive as f64 * 24.0 - 12.0) / 20.0);
        let mut sl = in_l as f64 * gain;
        let mut sr = in_r as f64 * gain;

        let mojo_l = sl.abs().powf(0.25);
        if mojo_l > 0.0 { sl = (sl * mojo_l * std::f64::consts::PI * 0.5).sin() / mojo_l * 0.987654321; }
        let mojo_r = sr.abs().powf(0.25);
        if mojo_r > 0.0 { sr = (sr * mojo_r * std::f64::consts::PI * 0.5).sin() / mojo_r * 0.987654321; }

        (sl as f32, sr as f32)
    }
}


// ─── Mode 11: ADClip7 (simplified adaptive clipper) ─────────────────────────

pub struct AirwindowsADClip7 {
    env_l: f64, env_r: f64,
    refclip: f64,
}
impl AirwindowsADClip7 {
    pub fn new() -> Self { Self { env_l: 0.0, env_r: 0.0, refclip: 1.0 } }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let input_gain = 10f64.powf(drive as f64 * 18.0 / 20.0);
        let softness = 0.618033988749894848f64 * (1.0 - drive as f64);
        let hardness = 1.0 - softness;
        let att = 1.0 - (-1.0 / (0.0001 * overallscale * sr as f64)).exp();
        let rel = 1.0 - (-1.0 / (0.01 * overallscale * sr as f64)).exp();

        let mut sl = in_l as f64 * input_gain;
        let mut sr = in_r as f64 * input_gain;

        // envelope follower for overshoot
        let ov_l = (sl.abs() - self.refclip).max(0.0);
        let ov_r = (sr.abs() - self.refclip).max(0.0);
        let ov_max = ov_l.max(ov_r);
        if ov_max > self.env_l {
            self.env_l = self.env_l * (1.0 - att) + ov_max * att;
        } else {
            self.env_l = self.env_l * (1.0 - rel);
        }
        self.env_r = self.env_l;

        // adaptive clip threshold
        let clip = (self.refclip - self.env_l * 0.5).max(0.5);

        if sl > clip { sl = clip * hardness + sl.min(1.0) * softness; }
        if sl < -clip { sl = -clip * hardness + sl.max(-1.0) * softness; }
        if sr > clip { sr = clip * hardness + sr.min(1.0) * softness; }
        if sr < -clip { sr = -clip * hardness + sr.max(-1.0) * softness; }

        (sl as f32, sr as f32)
    }
}

// ─── Mode 12: Loud ──────────────────────────────────────────────────────────

pub struct AirwindowsLoud {
    last_l: f64,
    last_r: f64,
}
impl AirwindowsLoud {
    pub fn new() -> Self { Self { last_l: 0.0, last_r: 0.0 } }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let boost = (drive as f64 + 1.0).powi(5);

        let process = |x: f64, last: &mut f64| -> f64 {
            let mut s = x * boost;
            let mut clamp = s - *last;
            if clamp > 0.0 {
                let mut t = -(s - 1.0) * 1.2566108;
                t = t.clamp(0.0, 3.141527);
                let t2 = t.sin() * overallscale;
                if clamp > t2 { clamp = t2; }
            }
            if clamp < 0.0 {
                let mut t = (s + 1.0) * 1.2566108;
                t = t.clamp(0.0, 3.141527);
                let t2 = -t.sin() * overallscale;
                if clamp < t2 { clamp = t2; }
            }
            s = *last + clamp;
            *last = s;
            s
        };

        let out_l = process(in_l as f64, &mut self.last_l);
        let out_r = process(in_r as f64, &mut self.last_r);
        (out_l as f32, out_r as f32)
    }
}

// ─── Mode 13: IronOxide5 (simplified) ───────────────────────────────────────

pub struct AirwindowsIronOxide5 {
    iir_al: f64, iir_bl: f64,
    iir_ar: f64, iir_br: f64,
    flip: bool,
}
impl AirwindowsIronOxide5 {
    pub fn new() -> Self {
        Self { iir_al: 0.0, iir_bl: 0.0, iir_ar: 0.0, iir_br: 0.0, flip: false }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let input_gain = 10f64.powf((drive as f64 * 36.0 - 18.0) / 20.0);
        let iir_amount = 0.03 / overallscale;

        let mut sl = in_l as f64 * input_gain;
        let mut sr = in_r as f64 * input_gain;

        if self.flip {
            self.iir_al = self.iir_al * (1.0 - iir_amount) + sl * iir_amount;
            sl -= self.iir_al;
            self.iir_ar = self.iir_ar * (1.0 - iir_amount) + sr * iir_amount;
            sr -= self.iir_ar;
        } else {
            self.iir_bl = self.iir_bl * (1.0 - iir_amount) + sl * iir_amount;
            sl -= self.iir_bl;
            self.iir_br = self.iir_br * (1.0 - iir_amount) + sr * iir_amount;
            sr -= self.iir_br;
        }
        self.flip = !self.flip;

        // bridge rectifier sin saturation
        let mut br = sl.abs().min(HALF_PI_64);
        br = br.sin();
        sl = if sl > 0.0 { br } else { -br };

        br = sr.abs().min(HALF_PI_64);
        br = br.sin();
        sr = if sr > 0.0 { br } else { -br };

        (sl as f32, sr as f32)
    }
}

// ─── Mode 14: ToTape6 ───────────────────────────────────────────────────────

pub struct AirwindowsToTape6 {
    iir_mid_roller_al: f64, iir_mid_roller_bl: f64,
    iir_mid_roller_ar: f64, iir_mid_roller_br: f64,
    iir_head_bump_al: f64, iir_head_bump_bl: f64,
    iir_head_bump_ar: f64, iir_head_bump_br: f64,
    // biquad state: [freq, reso, b0, b1, b2, a1, a2, z1, z2]
    biquad_al: [f64; 9], biquad_bl: [f64; 9],
    biquad_cl: [f64; 9], biquad_dl: [f64; 9],
    biquad_ar: [f64; 9], biquad_br: [f64; 9],
    biquad_cr: [f64; 9], biquad_dr: [f64; 9],
    flip: bool,
    last_sample_l: f64, last_sample_r: f64,
    rateof: f64, sweep: f64, nextmax: f64,
    d_l: Vec<f64>, d_r: Vec<f64>,
    gcount: i32,
}
impl AirwindowsToTape6 {
    pub fn new() -> Self {
        Self {
            iir_mid_roller_al: 0.0, iir_mid_roller_bl: 0.0,
            iir_mid_roller_ar: 0.0, iir_mid_roller_br: 0.0,
            iir_head_bump_al: 0.0, iir_head_bump_bl: 0.0,
            iir_head_bump_ar: 0.0, iir_head_bump_br: 0.0,
            biquad_al: [0.0; 9], biquad_bl: [0.0; 9],
            biquad_cl: [0.0; 9], biquad_dl: [0.0; 9],
            biquad_ar: [0.0; 9], biquad_br: [0.0; 9],
            biquad_cr: [0.0; 9], biquad_dr: [0.0; 9],
            flip: false,
            last_sample_l: 0.0, last_sample_r: 0.0,
            rateof: 0.0, sweep: 0.0, nextmax: 0.5,
            d_l: vec![0.0f64; 502],
            d_r: vec![0.0f64; 502],
            gcount: 0,
        }
    }

    fn setup_biquads(&mut self, overallscale: f64) {
        // Biquad A/B: bandpass at 0.007/overallscale, Q=0.0009
        let freq_ab = 0.007 / overallscale;
        let q_ab = 0.0009f64;
        let k = (std::f64::consts::PI * freq_ab).tan();
        let norm = 1.0 / (1.0 + k / q_ab + k * k);
        let b0 = k / q_ab * norm;
        let b2 = -b0;
        let a1 = 2.0 * (k * k - 1.0) * norm;
        let a2 = (1.0 - k / q_ab + k * k) * norm;
        for bq in [&mut self.biquad_al, &mut self.biquad_bl,
                   &mut self.biquad_ar, &mut self.biquad_br] {
            bq[2] = b0; bq[3] = 0.0; bq[4] = b2; bq[5] = a1; bq[6] = a2;
        }
        // Biquad C/D: bandpass at 0.032/overallscale, Q=0.0007
        let freq_cd = 0.032 / overallscale;
        let q_cd = 0.0007f64;
        let k2 = (std::f64::consts::PI * freq_cd).tan();
        let norm2 = 1.0 / (1.0 + k2 / q_cd + k2 * k2);
        let b0_2 = k2 / q_cd * norm2;
        let b2_2 = -b0_2;
        let a1_2 = 2.0 * (k2 * k2 - 1.0) * norm2;
        let a2_2 = (1.0 - k2 / q_cd + k2 * k2) * norm2;
        for bq in [&mut self.biquad_cl, &mut self.biquad_dl,
                   &mut self.biquad_cr, &mut self.biquad_dr] {
            bq[2] = b0_2; bq[3] = 0.0; bq[4] = b2_2; bq[5] = a1_2; bq[6] = a2_2;
        }
    }

    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        self.setup_biquads(overallscale);

        let roll_amount = (1.0 - (drive as f64 * 0.45)) / overallscale;
        let head_bump_freq = 0.12 / overallscale;
        let head_bump_control = drive as f64 * 0.25;

        let mut sl = in_l as f64;
        let mut sr2 = in_r as f64;

        // flutter (simplified - skip for brevity, just use input)
        if self.gcount < 0 || self.gcount > 499 { self.gcount = 499; }
        self.d_l[self.gcount as usize] = sl;
        self.d_r[self.gcount as usize] = sr2;
        self.gcount -= 1;

        let (highs_l, highs_r, nonhighs_l, nonhighs_r);

        if self.flip {
            self.iir_mid_roller_al = self.iir_mid_roller_al * (1.0 - roll_amount) + sl * roll_amount;
            self.iir_mid_roller_ar = self.iir_mid_roller_ar * (1.0 - roll_amount) + sr2 * roll_amount;
            highs_l = sl - self.iir_mid_roller_al;
            highs_r = sr2 - self.iir_mid_roller_ar;
            nonhighs_l = self.iir_mid_roller_al;
            nonhighs_r = self.iir_mid_roller_ar;

            self.iir_head_bump_al += sl * 0.05;
            self.iir_head_bump_al -= self.iir_head_bump_al.powi(3) * head_bump_freq;
            self.iir_head_bump_al = self.iir_head_bump_al.sin();
            self.iir_head_bump_ar += sr2 * 0.05;
            self.iir_head_bump_ar -= self.iir_head_bump_ar.powi(3) * head_bump_freq;
            self.iir_head_bump_ar = self.iir_head_bump_ar.sin();

            // biquad A on head bump
            let tmp = self.iir_head_bump_al * self.biquad_al[2] + self.biquad_al[7];
            self.biquad_al[7] = self.iir_head_bump_al * self.biquad_al[3] - tmp * self.biquad_al[5] + self.biquad_al[8];
            self.biquad_al[8] = self.iir_head_bump_al * self.biquad_al[4] - tmp * self.biquad_al[6];
            self.iir_head_bump_al = tmp.clamp(-1.0, 1.0).asin();

            let tmp = self.iir_head_bump_ar * self.biquad_ar[2] + self.biquad_ar[7];
            self.biquad_ar[7] = self.iir_head_bump_ar * self.biquad_ar[3] - tmp * self.biquad_ar[5] + self.biquad_ar[8];
            self.biquad_ar[8] = self.iir_head_bump_ar * self.biquad_ar[4] - tmp * self.biquad_ar[6];
            self.iir_head_bump_ar = tmp.clamp(-1.0, 1.0).asin();

            // sin + biquad C on main
            sl = sl.sin();
            let tmp = sl * self.biquad_cl[2] + self.biquad_cl[7];
            self.biquad_cl[7] = sl * self.biquad_cl[3] - tmp * self.biquad_cl[5] + self.biquad_cl[8];
            self.biquad_cl[8] = sl * self.biquad_cl[4] - tmp * self.biquad_cl[6];
            sl = tmp.clamp(-1.0, 1.0).asin();

            sr2 = sr2.sin();
            let tmp = sr2 * self.biquad_cr[2] + self.biquad_cr[7];
            self.biquad_cr[7] = sr2 * self.biquad_cr[3] - tmp * self.biquad_cr[5] + self.biquad_cr[8];
            self.biquad_cr[8] = sr2 * self.biquad_cr[4] - tmp * self.biquad_cr[6];
            sr2 = tmp.clamp(-1.0, 1.0).asin();
        } else {
            self.iir_mid_roller_bl = self.iir_mid_roller_bl * (1.0 - roll_amount) + sl * roll_amount;
            self.iir_mid_roller_br = self.iir_mid_roller_br * (1.0 - roll_amount) + sr2 * roll_amount;
            highs_l = sl - self.iir_mid_roller_bl;
            highs_r = sr2 - self.iir_mid_roller_br;
            nonhighs_l = self.iir_mid_roller_bl;
            nonhighs_r = self.iir_mid_roller_br;

            self.iir_head_bump_bl += sl * 0.05;
            self.iir_head_bump_bl -= self.iir_head_bump_bl.powi(3) * head_bump_freq;
            self.iir_head_bump_bl = self.iir_head_bump_bl.sin();
            self.iir_head_bump_br += sr2 * 0.05;
            self.iir_head_bump_br -= self.iir_head_bump_br.powi(3) * head_bump_freq;
            self.iir_head_bump_br = self.iir_head_bump_br.sin();

            let tmp = self.iir_head_bump_bl * self.biquad_bl[2] + self.biquad_bl[7];
            self.biquad_bl[7] = self.iir_head_bump_bl * self.biquad_bl[3] - tmp * self.biquad_bl[5] + self.biquad_bl[8];
            self.biquad_bl[8] = self.iir_head_bump_bl * self.biquad_bl[4] - tmp * self.biquad_bl[6];
            self.iir_head_bump_bl = tmp.clamp(-1.0, 1.0).asin();

            let tmp = self.iir_head_bump_br * self.biquad_br[2] + self.biquad_br[7];
            self.biquad_br[7] = self.iir_head_bump_br * self.biquad_br[3] - tmp * self.biquad_br[5] + self.biquad_br[8];
            self.biquad_br[8] = self.iir_head_bump_br * self.biquad_br[4] - tmp * self.biquad_br[6];
            self.iir_head_bump_br = tmp.clamp(-1.0, 1.0).asin();

            sl = sl.sin();
            let tmp = sl * self.biquad_dl[2] + self.biquad_dl[7];
            self.biquad_dl[7] = sl * self.biquad_dl[3] - tmp * self.biquad_dl[5] + self.biquad_dl[8];
            self.biquad_dl[8] = sl * self.biquad_dl[4] - tmp * self.biquad_dl[6];
            sl = tmp.clamp(-1.0, 1.0).asin();

            sr2 = sr2.sin();
            let tmp = sr2 * self.biquad_dr[2] + self.biquad_dr[7];
            self.biquad_dr[7] = sr2 * self.biquad_dr[3] - tmp * self.biquad_dr[5] + self.biquad_dr[8];
            self.biquad_dr[8] = sr2 * self.biquad_dr[4] - tmp * self.biquad_dr[6];
            sr2 = tmp.clamp(-1.0, 1.0).asin();
        }

        self.flip = !self.flip;

        // recombine
        sl += highs_l;
        sr2 += highs_r;
        sl += self.iir_head_bump_al * head_bump_control;
        sr2 += self.iir_head_bump_ar * head_bump_control;

        let _ = (nonhighs_l, nonhighs_r);
        (sl as f32, sr2 as f32)
    }
}


// ─── Mode 15: ChromeOxide ───────────────────────────────────────────────────

pub struct AirwindowsChromeOxide {
    iir_al: f64, iir_bl: f64, iir_cl: f64, iir_dl: f64,
    iir_ar: f64, iir_br: f64, iir_cr: f64, iir_dr: f64,
    second_l: f64, third_l: f64, fourth_l: f64, fifth_l: f64,
    second_r: f64, third_r: f64, fourth_r: f64, fifth_r: f64,
    flip: bool,
    fpd: u32,
}
impl AirwindowsChromeOxide {
    pub fn new() -> Self {
        Self {
            iir_al: 0.0, iir_bl: 0.0, iir_cl: 0.0, iir_dl: 0.0,
            iir_ar: 0.0, iir_br: 0.0, iir_cr: 0.0, iir_dr: 0.0,
            second_l: 0.0, third_l: 0.0, fourth_l: 0.0, fifth_l: 0.0,
            second_r: 0.0, third_r: 0.0, fourth_r: 0.0, fifth_r: 0.0,
            flip: false,
            fpd: 1,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let intensity = 0.9 + (drive as f64).powi(2);
        let iir_amount = (1.0 - (intensity / 10.0)).powi(2) / overallscale;
        let density_a = intensity * 80.0 + 1.0;
        let glitch = if intensity > 1.0 { intensity - 1.0 } else { 0.0 };
        let indrive = if intensity > 1.0 { intensity * intensity } else { 1.0 };

        let mut sl = in_l as f64 * indrive;
        let mut sr = in_r as f64 * indrive;
        let mut bass_l = sl;
        let mut bass_r = sr;

        if self.flip {
            self.iir_al = self.iir_al * (1.0 - iir_amount) + sl * iir_amount;
            sl -= self.iir_al;
            self.iir_ar = self.iir_ar * (1.0 - iir_amount) + sr * iir_amount;
            sr -= self.iir_ar;
        } else {
            self.iir_bl = self.iir_bl * (1.0 - iir_amount) + sl * iir_amount;
            sl -= self.iir_bl;
            self.iir_br = self.iir_br * (1.0 - iir_amount) + sr * iir_amount;
            sr -= self.iir_br;
        }

        bass_l -= sl * (sl.abs() * glitch) * (sl.abs() * glitch);
        bass_r -= sr * (sr.abs() * glitch) * (sr.abs() * glitch);

        if self.flip {
            self.iir_cl = self.iir_cl * (1.0 - iir_amount) + bass_l * iir_amount;
            bass_l = self.iir_cl;
            self.iir_cr = self.iir_cr * (1.0 - iir_amount) + bass_r * iir_amount;
            bass_r = self.iir_cr;
        } else {
            self.iir_dl = self.iir_dl * (1.0 - iir_amount) + bass_l * iir_amount;
            bass_l = self.iir_dl;
            self.iir_dr = self.iir_dr * (1.0 - iir_amount) + bass_r * iir_amount;
            bass_r = self.iir_dr;
        }
        self.flip = !self.flip;

        // simple treble noise (no random, use deterministic)
        xorshift(&mut self.fpd);
        // skip interpolation noise for simplicity

        let mut br = sl.abs() * density_a;
        if br > HALF_PI_64 { br = HALF_PI_64; }
        br = br.sin();
        sl = if sl > 0.0 { br / density_a } else { -br / density_a };

        br = sr.abs() * density_a;
        if br > HALF_PI_64 { br = HALF_PI_64; }
        br = br.sin();
        sr = if sr > 0.0 { br / density_a } else { -br / density_a };

        sl += bass_l;
        sr += bass_r;

        (sl as f32, sr as f32)
    }
}

// ─── Mode 16: Pressure4 ─────────────────────────────────────────────────────

pub struct AirwindowsPressure4 {
    mu_speed_a: f64, mu_speed_b: f64,
    mu_coeff_a: f64, mu_coeff_b: f64,
    mu_vary: f64, mu_new_speed: f64,
    flip: bool,
}
impl AirwindowsPressure4 {
    pub fn new() -> Self {
        Self {
            mu_speed_a: 10000.0, mu_speed_b: 10000.0,
            mu_coeff_a: 1.0, mu_coeff_b: 1.0,
            mu_vary: 1.0, mu_new_speed: 0.0,
            flip: false,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let threshold = 1.0 - drive as f64 * 0.95;
        let mu_makeup_gain = 1.0 / threshold.max(0.0001);
        let release = (1.28f64 - 0.5).powi(5) * 32768.0 / overallscale;
        let fastest = release.sqrt();

        let mut sl = in_l as f64 * mu_makeup_gain;
        let mut sr = in_r as f64 * mu_makeup_gain;

        let input_sense = sl.abs().max(sr.abs());

        if self.flip {
            if input_sense > threshold {
                self.mu_vary = threshold / input_sense;
                let mu_attack = self.mu_speed_a.abs().sqrt();
                self.mu_coeff_a *= mu_attack - 1.0;
                self.mu_coeff_a += if self.mu_vary < threshold { threshold } else { self.mu_vary };
                self.mu_coeff_a /= mu_attack;
            } else {
                self.mu_coeff_a *= self.mu_speed_a * self.mu_speed_a - 1.0;
                self.mu_coeff_a += 1.0;
                self.mu_coeff_a /= self.mu_speed_a * self.mu_speed_a;
            }
            self.mu_new_speed = self.mu_speed_a * (self.mu_speed_a - 1.0);
            self.mu_new_speed += input_sense * release + fastest;
            self.mu_speed_a = self.mu_new_speed / self.mu_speed_a;
            let coeff = self.mu_coeff_a * self.mu_coeff_a;
            sl *= coeff; sr *= coeff;
        } else {
            if input_sense > threshold {
                self.mu_vary = threshold / input_sense;
                let mu_attack = self.mu_speed_b.abs().sqrt();
                self.mu_coeff_b *= mu_attack - 1.0;
                self.mu_coeff_b += if self.mu_vary < threshold { threshold } else { self.mu_vary };
                self.mu_coeff_b /= mu_attack;
            } else {
                self.mu_coeff_b *= self.mu_speed_b * self.mu_speed_b - 1.0;
                self.mu_coeff_b += 1.0;
                self.mu_coeff_b /= self.mu_speed_b * self.mu_speed_b;
            }
            self.mu_new_speed = self.mu_speed_b * (self.mu_speed_b - 1.0);
            self.mu_new_speed += input_sense * release + fastest;
            self.mu_speed_b = self.mu_new_speed / self.mu_speed_b;
            let coeff = self.mu_coeff_b * self.mu_coeff_b;
            sl *= coeff; sr *= coeff;
        }
        self.flip = !self.flip;

        // bridge rectifier limiter
        let mut br = sl.abs().min(HALF_PI_64);
        br = br.sin();
        sl = if sl > 0.0 { br } else { -br };
        br = sr.abs().min(HALF_PI_64);
        br = br.sin();
        sr = if sr > 0.0 { br } else { -br };

        (sl as f32, sr as f32)
    }
}

// ─── Mode 17: ButterComp2 ───────────────────────────────────────────────────

pub struct AirwindowsButterComp2 {
    last_out_l: f64, last_out_r: f64,
    target_pos_l: f64, target_neg_l: f64,
    target_pos_r: f64, target_neg_r: f64,
    ctrl_a_pos_l: f64, ctrl_a_neg_l: f64,
    ctrl_b_pos_l: f64, ctrl_b_neg_l: f64,
    ctrl_a_pos_r: f64, ctrl_a_neg_r: f64,
    ctrl_b_pos_r: f64, ctrl_b_neg_r: f64,
    flip: bool,
}
impl AirwindowsButterComp2 {
    pub fn new() -> Self {
        Self {
            last_out_l: 0.0, last_out_r: 0.0,
            target_pos_l: 1.0, target_neg_l: 1.0,
            target_pos_r: 1.0, target_neg_r: 1.0,
            ctrl_a_pos_l: 1.0, ctrl_a_neg_l: 1.0,
            ctrl_b_pos_l: 1.0, ctrl_b_neg_l: 1.0,
            ctrl_a_pos_r: 1.0, ctrl_a_neg_r: 1.0,
            ctrl_b_pos_r: 1.0, ctrl_b_neg_r: 1.0,
            flip: false,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let input_gain = 10f64.powf(drive as f64 * 14.0 / 20.0);
        let comp_factor = 0.012 * (drive as f64 / 135.0);
        let mut output_gain = input_gain;
        output_gain = (output_gain - 1.0) / 1.5 + 1.0;

        let mut sl = in_l as f64 * input_gain;
        let mut sr = in_r as f64 * input_gain;

        // L
        let divisor_l = (comp_factor / (1.0 + self.last_out_l.abs())) / overallscale;
        let remainder_l = divisor_l;
        let div_l = 1.0 - divisor_l;

        let inp_pos_l = (sl + 1.0).max(0.0);
        let out_pos_l = (inp_pos_l / 2.0).min(1.0);
        let inp_pos_l2 = inp_pos_l * inp_pos_l;
        self.target_pos_l = self.target_pos_l * div_l + inp_pos_l2 * remainder_l;
        let calc_pos_l = (1.0 / self.target_pos_l.max(1e-10)).powi(2);

        let inp_neg_l = (-sl + 1.0).max(0.0);
        let out_neg_l = (inp_neg_l / 2.0).min(1.0);
        let inp_neg_l2 = inp_neg_l * inp_neg_l;
        self.target_neg_l = self.target_neg_l * div_l + inp_neg_l2 * remainder_l;
        let calc_neg_l = (1.0 / self.target_neg_l.max(1e-10)).powi(2);

        if sl > 0.0 {
            if self.flip { self.ctrl_a_pos_l = self.ctrl_a_pos_l * div_l + calc_pos_l * remainder_l; }
            else { self.ctrl_b_pos_l = self.ctrl_b_pos_l * div_l + calc_pos_l * remainder_l; }
        } else {
            if self.flip { self.ctrl_a_neg_l = self.ctrl_a_neg_l * div_l + calc_neg_l * remainder_l; }
            else { self.ctrl_b_neg_l = self.ctrl_b_neg_l * div_l + calc_neg_l * remainder_l; }
        }

        // R
        let divisor_r = (comp_factor / (1.0 + self.last_out_r.abs())) / overallscale;
        let remainder_r = divisor_r;
        let div_r = 1.0 - divisor_r;

        let inp_pos_r = (sr + 1.0).max(0.0);
        let out_pos_r = (inp_pos_r / 2.0).min(1.0);
        let inp_pos_r2 = inp_pos_r * inp_pos_r;
        self.target_pos_r = self.target_pos_r * div_r + inp_pos_r2 * remainder_r;
        let calc_pos_r = (1.0 / self.target_pos_r.max(1e-10)).powi(2);

        let inp_neg_r = (-sr + 1.0).max(0.0);
        let out_neg_r = (inp_neg_r / 2.0).min(1.0);
        let inp_neg_r2 = inp_neg_r * inp_neg_r;
        self.target_neg_r = self.target_neg_r * div_r + inp_neg_r2 * remainder_r;
        let calc_neg_r = (1.0 / self.target_neg_r.max(1e-10)).powi(2);

        if sr > 0.0 {
            if self.flip { self.ctrl_a_pos_r = self.ctrl_a_pos_r * div_r + calc_pos_r * remainder_r; }
            else { self.ctrl_b_pos_r = self.ctrl_b_pos_r * div_r + calc_pos_r * remainder_r; }
        } else {
            if self.flip { self.ctrl_a_neg_r = self.ctrl_a_neg_r * div_r + calc_neg_r * remainder_r; }
            else { self.ctrl_b_neg_r = self.ctrl_b_neg_r * div_r + calc_neg_r * remainder_r; }
        }

        let (mult_l, mult_r) = if self.flip {
            (self.ctrl_a_pos_l * out_pos_l + self.ctrl_a_neg_l * out_neg_l,
             self.ctrl_a_pos_r * out_pos_r + self.ctrl_a_neg_r * out_neg_r)
        } else {
            (self.ctrl_b_pos_l * out_pos_l + self.ctrl_b_neg_l * out_neg_l,
             self.ctrl_b_pos_r * out_pos_r + self.ctrl_b_neg_r * out_neg_r)
        };

        sl = sl * mult_l / output_gain;
        sr = sr * mult_r / output_gain;

        self.last_out_l = sl;
        self.last_out_r = sr;
        self.flip = !self.flip;

        (sl as f32, sr as f32)
    }
}

// ─── Mode 18: VariMu ────────────────────────────────────────────────────────

pub struct AirwindowsVariMu {
    prev_l: f64, prev_r: f64,
    mu_speed_al: f64, mu_speed_bl: f64,
    mu_speed_ar: f64, mu_speed_br: f64,
    mu_coeff_al: f64, mu_coeff_bl: f64,
    mu_coeff_ar: f64, mu_coeff_br: f64,
    mu_vary_l: f64, mu_vary_r: f64,
    mu_attack_l: f64, mu_attack_r: f64,
    flip: bool,
}
impl AirwindowsVariMu {
    pub fn new() -> Self {
        Self {
            prev_l: 0.0, prev_r: 0.0,
            mu_speed_al: 10000.0, mu_speed_bl: 10000.0,
            mu_speed_ar: 10000.0, mu_speed_br: 10000.0,
            mu_coeff_al: 1.0, mu_coeff_bl: 1.0,
            mu_coeff_ar: 1.0, mu_coeff_br: 1.0,
            mu_vary_l: 1.0, mu_vary_r: 1.0,
            mu_attack_l: 1.0, mu_attack_r: 1.0,
            flip: false,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = 2.0 * sr as f64 / 44100.0;
        let d = drive as f64;
        let threshold = 1.001 - (1.0 - (1.0 - d).powi(3));
        let mu_makeup = (1.0 / threshold.max(1e-6)).sqrt();
        let mu_makeup = (mu_makeup + mu_makeup.sqrt()) / 2.0;
        let mu_makeup = mu_makeup.sqrt();
        let out_gain = mu_makeup.sqrt();
        let release = (1.15f64 - 0.5).powi(5) * 32768.0 / overallscale;
        let fastest = release.sqrt();

        // squared sample detection
        let sq_l = if in_l.abs() as f64 > self.prev_l.abs() { self.prev_l * self.prev_l }
                   else { (in_l as f64) * (in_l as f64) };
        self.prev_l = in_l as f64;
        let sq_r = if in_r.abs() as f64 > self.prev_r.abs() { self.prev_r * self.prev_r }
                   else { (in_r as f64) * (in_r as f64) };
        self.prev_r = in_r as f64;

        let mut sl = in_l as f64 * mu_makeup;
        let mut sr = in_r as f64 * mu_makeup;

        // L
        if self.flip {
            if sq_l.abs() > threshold {
                self.mu_vary_l = threshold / sq_l.abs();
                self.mu_attack_l = self.mu_speed_al.abs().sqrt();
                self.mu_coeff_al *= self.mu_attack_l - 1.0;
                self.mu_coeff_al += if self.mu_vary_l < threshold { threshold } else { self.mu_vary_l };
                self.mu_coeff_al /= self.mu_attack_l;
            } else {
                self.mu_coeff_al *= self.mu_speed_al * self.mu_speed_al - 1.0;
                self.mu_coeff_al += 1.0;
                self.mu_coeff_al /= self.mu_speed_al * self.mu_speed_al;
            }
            let mu_new = self.mu_speed_al * (self.mu_speed_al - 1.0) + sq_l.abs() * release + fastest;
            self.mu_speed_al = mu_new / self.mu_speed_al;
        } else {
            if sq_l.abs() > threshold {
                self.mu_vary_l = threshold / sq_l.abs();
                self.mu_attack_l = self.mu_speed_bl.abs().sqrt();
                self.mu_coeff_bl *= self.mu_attack_l - 1.0;
                self.mu_coeff_bl += if self.mu_vary_l < threshold { threshold } else { self.mu_vary_l };
                self.mu_coeff_bl /= self.mu_attack_l;
            } else {
                self.mu_coeff_bl *= self.mu_speed_bl * self.mu_speed_bl - 1.0;
                self.mu_coeff_bl += 1.0;
                self.mu_coeff_bl /= self.mu_speed_bl * self.mu_speed_bl;
            }
            let mu_new = self.mu_speed_bl * (self.mu_speed_bl - 1.0) + sq_l.abs() * release + fastest;
            self.mu_speed_bl = mu_new / self.mu_speed_bl;
        }

        // R
        if self.flip {
            if sq_r.abs() > threshold {
                self.mu_vary_r = threshold / sq_r.abs();
                self.mu_attack_r = self.mu_speed_ar.abs().sqrt();
                self.mu_coeff_ar *= self.mu_attack_r - 1.0;
                self.mu_coeff_ar += if self.mu_vary_r < threshold { threshold } else { self.mu_vary_r };
                self.mu_coeff_ar /= self.mu_attack_r;
            } else {
                self.mu_coeff_ar *= self.mu_speed_ar * self.mu_speed_ar - 1.0;
                self.mu_coeff_ar += 1.0;
                self.mu_coeff_ar /= self.mu_speed_ar * self.mu_speed_ar;
            }
            let mu_new = self.mu_speed_ar * (self.mu_speed_ar - 1.0) + sq_r.abs() * release + fastest;
            self.mu_speed_ar = mu_new / self.mu_speed_ar;
        } else {
            if sq_r.abs() > threshold {
                self.mu_vary_r = threshold / sq_r.abs();
                self.mu_attack_r = self.mu_speed_br.abs().sqrt();
                self.mu_coeff_br *= self.mu_attack_r - 1.0;
                self.mu_coeff_br += if self.mu_vary_r < threshold { threshold } else { self.mu_vary_r };
                self.mu_coeff_br /= self.mu_attack_r;
            } else {
                self.mu_coeff_br *= self.mu_speed_br * self.mu_speed_br - 1.0;
                self.mu_coeff_br += 1.0;
                self.mu_coeff_br /= self.mu_speed_br * self.mu_speed_br;
            }
            let mu_new = self.mu_speed_br * (self.mu_speed_br - 1.0) + sq_r.abs() * release + fastest;
            self.mu_speed_br = mu_new / self.mu_speed_br;
        }

        let (coeff_l, coeff_r) = if self.flip {
            ((self.mu_coeff_al + self.mu_coeff_al.powi(2)) / 2.0,
             (self.mu_coeff_ar + self.mu_coeff_ar.powi(2)) / 2.0)
        } else {
            ((self.mu_coeff_bl + self.mu_coeff_bl.powi(2)) / 2.0,
             (self.mu_coeff_br + self.mu_coeff_br.powi(2)) / 2.0)
        };
        sl *= coeff_l;
        sr *= coeff_r;
        self.flip = !self.flip;

        sl *= out_gain;
        sr *= out_gain;
        (sl as f32, sr as f32)
    }
}


// ─── Mode 19: PowerSag ──────────────────────────────────────────────────────

pub struct AirwindowsPowerSag {
    d_l: Vec<f64>, d_r: Vec<f64>,
    gcount: i32,
    control_l: f64, control_r: f64,
    fpd_l: u32, fpd_r: u32,
}
impl AirwindowsPowerSag {
    pub fn new() -> Self {
        Self {
            d_l: vec![0.0f64; 8001],
            d_r: vec![0.0f64; 8001],
            gcount: 0,
            control_l: 0.0, control_r: 0.0,
            fpd_l: 1, fpd_r: 2,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let intensity = (drive as f64).powi(5) * 80.0;
        let depth_a = 0.5f64 * 0.5;
        let offset_a = ((depth_a * 3900.0) as i32 + 1).max(1);

        if self.gcount < 0 || self.gcount > 4000 { self.gcount = 4000; }

        let gc = self.gcount as usize;
        let mut sl = in_l as f64;
        let mut sr = in_r as f64;

        let val_l = sl.abs() * intensity;
        self.d_l[gc] = val_l; self.d_l[gc + 4000] = val_l;
        self.control_l += self.d_l[gc] / offset_a as f64;
        self.control_l -= self.d_l[gc + offset_a as usize] / offset_a as f64;
        self.control_l -= 0.000001;
        let mut clamp_l = 1.0f64;
        if self.control_l < 0.0 { self.control_l = 0.0; }
        if self.control_l > 1.0 { clamp_l -= self.control_l - 1.0; self.control_l = 1.0; }
        if clamp_l < 0.5 { clamp_l = 0.5; }
        let thickness_l = (1.0 - self.control_l) * 2.0 - 1.0;
        let out_l = thickness_l.abs();
        let mut br_l = sl.abs().min(HALF_PI_64);
        br_l = if thickness_l > 0.0 { br_l.sin() } else { 1.0 - br_l.cos() };
        sl = if sl > 0.0 { sl * (1.0 - out_l) + br_l * out_l } else { sl * (1.0 - out_l) - br_l * out_l };
        sl *= clamp_l;

        let val_r = sr.abs() * intensity;
        self.d_r[gc] = val_r; self.d_r[gc + 4000] = val_r;
        self.control_r += self.d_r[gc] / offset_a as f64;
        self.control_r -= self.d_r[gc + offset_a as usize] / offset_a as f64;
        self.control_r -= 0.000001;
        let mut clamp_r = 1.0f64;
        if self.control_r < 0.0 { self.control_r = 0.0; }
        if self.control_r > 1.0 { clamp_r -= self.control_r - 1.0; self.control_r = 1.0; }
        if clamp_r < 0.5 { clamp_r = 0.5; }
        let thickness_r = (1.0 - self.control_r) * 2.0 - 1.0;
        let out_r = thickness_r.abs();
        let mut br_r = sr.abs().min(HALF_PI_64);
        br_r = if thickness_r > 0.0 { br_r.sin() } else { 1.0 - br_r.cos() };
        sr = if sr > 0.0 { sr * (1.0 - out_r) + br_r * out_r } else { sr * (1.0 - out_r) - br_r * out_r };
        sr *= clamp_r;

        self.gcount -= 1;
        (sl as f32, sr as f32)
    }
}

// ─── Mode 20: Galactic ──────────────────────────────────────────────────────

pub struct AirwindowsGalactic {
    a_ml: Vec<f32>, a_mr: Vec<f32>,
    a_il: Vec<f32>, a_jl: Vec<f32>, a_kl: Vec<f32>, a_ll: Vec<f32>,
    a_ir: Vec<f32>, a_jr: Vec<f32>, a_kr: Vec<f32>, a_lr: Vec<f32>,
    a_al: Vec<f32>, a_bl: Vec<f32>, a_cl: Vec<f32>, a_dl: Vec<f32>,
    a_ar: Vec<f32>, a_br: Vec<f32>, a_cr: Vec<f32>, a_dr: Vec<f32>,
    a_el: Vec<f32>, a_fl: Vec<f32>, a_gl: Vec<f32>, a_hl: Vec<f32>,
    a_er: Vec<f32>, a_fr: Vec<f32>, a_gr: Vec<f32>, a_hr: Vec<f32>,
    feedback_al: f64, feedback_bl: f64, feedback_cl: f64, feedback_dl: f64,
    feedback_ar: f64, feedback_br: f64, feedback_cr: f64, feedback_dr: f64,
    iir_al: f64, iir_ar: f64, iir_bl: f64, iir_br: f64,
    vib_m: f64, old_fpd: f64,
    cycle: i32,
    count_m: usize,
    count_i: usize, count_j: usize, count_k: usize, count_l: usize,
    count_a: usize, count_b: usize, count_c: usize, count_d: usize,
    count_e: usize, count_f: usize, count_g: usize, count_h: usize,
    last_ref_l: [f32; 7], last_ref_r: [f32; 7],
    fpd_l: u32,
}
impl AirwindowsGalactic {
    pub fn new() -> Self {
        Self {
            a_ml: vec![0.0f32; 257], a_mr: vec![0.0f32; 257],
            a_il: vec![0.0f32; 9700], a_jl: vec![0.0f32; 5200],
            a_kl: vec![0.0f32; 2500], a_ll: vec![0.0f32; 1100],
            a_ir: vec![0.0f32; 9700], a_jr: vec![0.0f32; 5200],
            a_kr: vec![0.0f32; 2500], a_lr: vec![0.0f32; 1100],
            a_al: vec![0.0f32; 9600], a_bl: vec![0.0f32; 5800],
            a_cl: vec![0.0f32; 2600], a_dl: vec![0.0f32; 1300],
            a_ar: vec![0.0f32; 9600], a_br: vec![0.0f32; 5800],
            a_cr: vec![0.0f32; 2600], a_dr: vec![0.0f32; 1300],
            a_el: vec![0.0f32; 15000], a_fl: vec![0.0f32; 8500],
            a_gl: vec![0.0f32; 4700], a_hl: vec![0.0f32; 3500],
            a_er: vec![0.0f32; 15000], a_fr: vec![0.0f32; 8500],
            a_gr: vec![0.0f32; 4700], a_hr: vec![0.0f32; 3500],
            feedback_al: 0.0, feedback_bl: 0.0, feedback_cl: 0.0, feedback_dl: 0.0,
            feedback_ar: 0.0, feedback_br: 0.0, feedback_cr: 0.0, feedback_dr: 0.0,
            iir_al: 0.0, iir_ar: 0.0, iir_bl: 0.0, iir_br: 0.0,
            vib_m: 0.0, old_fpd: 0.4294967295,
            cycle: 0,
            count_m: 0,
            count_i: 0, count_j: 0, count_k: 0, count_l: 0,
            count_a: 0, count_b: 0, count_c: 0, count_d: 0,
            count_e: 0, count_f: 0, count_g: 0, count_h: 0,
            last_ref_l: [0.0f32; 7], last_ref_r: [0.0f32; 7],
            fpd_l: 1,
        }
    }

    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let cycle_end = (overallscale.floor() as i32).clamp(1, 4);
        if self.cycle > cycle_end - 1 { self.cycle = cycle_end - 1; }

        let size = 1.87f64; // 1.77 * 1.0 + 0.1 with D=1.0
        let regen = 0.0625 + (1.0 - drive as f64) * 0.0625;
        let attenuate = (1.0 - regen / 0.125) * 1.333;
        let lowpass = (1.00001f64 - (1.0 - 0.5f64)).powi(2) / overallscale.sqrt();
        let drift = 0.03f64.powi(3) * 0.001;
        let wet = 1.0 - (1.0 - drive as f64).powi(3);

        let delay_i = (3407.0 * size) as usize;
        let delay_j = (1823.0 * size) as usize;
        let delay_k = (859.0 * size) as usize;
        let delay_l = (331.0 * size) as usize;
        let delay_a = (4801.0 * size) as usize;
        let delay_b = (2909.0 * size) as usize;
        let delay_c = (1153.0 * size) as usize;
        let delay_d = (461.0 * size) as usize;
        let delay_e = (7607.0 * size) as usize;
        let delay_f = (4217.0 * size) as usize;
        let delay_g = (2269.0 * size) as usize;
        let delay_h = (1597.0 * size) as usize;
        let delay_m: usize = 256;

        let dry_l = in_l;
        let dry_r = in_r;

        self.vib_m += self.old_fpd * drift;
        if self.vib_m > std::f64::consts::TAU {
            self.vib_m = 0.0;
            self.old_fpd = 0.4294967295 + xorshift(&mut self.fpd_l) as f64 * 1e-10;
        }

        self.a_ml[self.count_m] = in_l * attenuate as f32;
        self.a_mr[self.count_m] = in_r * attenuate as f32;
        self.count_m += 1; if self.count_m > delay_m { self.count_m = 0; }

        let offset_ml = (self.vib_m.sin() + 1.0) * 127.0;
        let offset_mr = ((self.vib_m + std::f64::consts::FRAC_PI_2).sin() + 1.0) * 127.0;
        let wml = self.count_m + offset_ml as usize;
        let wmr = self.count_m + offset_mr as usize;
        let fml = offset_ml - offset_ml.floor();
        let fmr = offset_mr - offset_mr.floor();
        let i1 = wml - if wml > delay_m { delay_m + 1 } else { 0 };
        let i2 = (wml + 1) - if wml + 1 > delay_m { delay_m + 1 } else { 0 };
        let interp_ml = self.a_ml[i1.min(self.a_ml.len()-1)] as f64 * (1.0 - fml)
                      + self.a_ml[i2.min(self.a_ml.len()-1)] as f64 * fml;
        let i1 = wmr - if wmr > delay_m { delay_m + 1 } else { 0 };
        let i2 = (wmr + 1) - if wmr + 1 > delay_m { delay_m + 1 } else { 0 };
        let interp_mr = self.a_mr[i1.min(self.a_mr.len()-1)] as f64 * (1.0 - fmr)
                      + self.a_mr[i2.min(self.a_mr.len()-1)] as f64 * fmr;

        let mut sl = interp_ml;
        let mut sr = interp_mr;

        if self.iir_al.abs() < 1.18e-37 { self.iir_al = 0.0; }
        self.iir_al = self.iir_al * (1.0 - lowpass) + sl * lowpass; sl = self.iir_al;
        if self.iir_ar.abs() < 1.18e-37 { self.iir_ar = 0.0; }
        self.iir_ar = self.iir_ar * (1.0 - lowpass) + sr * lowpass; sr = self.iir_ar;

        self.cycle += 1;
        if self.cycle == cycle_end {
            self.a_il[self.count_i] = (sl + self.feedback_ar * regen) as f32;
            self.a_jl[self.count_j] = (sl + self.feedback_br * regen) as f32;
            self.a_kl[self.count_k] = (sl + self.feedback_cr * regen) as f32;
            self.a_ll[self.count_l] = (sl + self.feedback_dr * regen) as f32;
            self.a_ir[self.count_i] = (sr + self.feedback_al * regen) as f32;
            self.a_jr[self.count_j] = (sr + self.feedback_bl * regen) as f32;
            self.a_kr[self.count_k] = (sr + self.feedback_cl * regen) as f32;
            self.a_lr[self.count_l] = (sr + self.feedback_dl * regen) as f32;

            self.count_i += 1; if self.count_i > delay_i { self.count_i = 0; }
            self.count_j += 1; if self.count_j > delay_j { self.count_j = 0; }
            self.count_k += 1; if self.count_k > delay_k { self.count_k = 0; }
            self.count_l += 1; if self.count_l > delay_l { self.count_l = 0; }

            let ril = self.count_i - if self.count_i > delay_i { delay_i + 1 } else { 0 };
            let rjl = self.count_j - if self.count_j > delay_j { delay_j + 1 } else { 0 };
            let rkl = self.count_k - if self.count_k > delay_k { delay_k + 1 } else { 0 };
            let rll = self.count_l - if self.count_l > delay_l { delay_l + 1 } else { 0 };

            let out_il = self.a_il[ril] as f64; let out_jl = self.a_jl[rjl] as f64;
            let out_kl = self.a_kl[rkl] as f64; let out_ll = self.a_ll[rll] as f64;
            let out_ir = self.a_ir[ril] as f64; let out_jr = self.a_jr[rjl] as f64;
            let out_kr = self.a_kr[rkl] as f64; let out_lr = self.a_lr[rll] as f64;

            self.a_al[self.count_a] = (out_il - (out_jl + out_kl + out_ll)) as f32;
            self.a_bl[self.count_b] = (out_jl - (out_il + out_kl + out_ll)) as f32;
            self.a_cl[self.count_c] = (out_kl - (out_il + out_jl + out_ll)) as f32;
            self.a_dl[self.count_d] = (out_ll - (out_il + out_jl + out_kl)) as f32;
            self.a_ar[self.count_a] = (out_ir - (out_jr + out_kr + out_lr)) as f32;
            self.a_br[self.count_b] = (out_jr - (out_ir + out_kr + out_lr)) as f32;
            self.a_cr[self.count_c] = (out_kr - (out_ir + out_jr + out_lr)) as f32;
            self.a_dr[self.count_d] = (out_lr - (out_ir + out_jr + out_kr)) as f32;

            self.count_a += 1; if self.count_a > delay_a { self.count_a = 0; }
            self.count_b += 1; if self.count_b > delay_b { self.count_b = 0; }
            self.count_c += 1; if self.count_c > delay_c { self.count_c = 0; }
            self.count_d += 1; if self.count_d > delay_d { self.count_d = 0; }

            let ral = self.count_a - if self.count_a > delay_a { delay_a + 1 } else { 0 };
            let rbl = self.count_b - if self.count_b > delay_b { delay_b + 1 } else { 0 };
            let rcl = self.count_c - if self.count_c > delay_c { delay_c + 1 } else { 0 };
            let rdl = self.count_d - if self.count_d > delay_d { delay_d + 1 } else { 0 };

            let out_al = self.a_al[ral] as f64; let out_bl = self.a_bl[rbl] as f64;
            let out_cl = self.a_cl[rcl] as f64; let out_dl = self.a_dl[rdl] as f64;
            let out_ar = self.a_ar[ral] as f64; let out_br = self.a_br[rbl] as f64;
            let out_cr = self.a_cr[rcl] as f64; let out_dr = self.a_dr[rdl] as f64;

            self.a_el[self.count_e] = (out_al - (out_bl + out_cl + out_dl)) as f32;
            self.a_fl[self.count_f] = (out_bl - (out_al + out_cl + out_dl)) as f32;
            self.a_gl[self.count_g] = (out_cl - (out_al + out_bl + out_dl)) as f32;
            self.a_hl[self.count_h] = (out_dl - (out_al + out_bl + out_cl)) as f32;
            self.a_er[self.count_e] = (out_ar - (out_br + out_cr + out_dr)) as f32;
            self.a_fr[self.count_f] = (out_br - (out_ar + out_cr + out_dr)) as f32;
            self.a_gr[self.count_g] = (out_cr - (out_ar + out_br + out_dr)) as f32;
            self.a_hr[self.count_h] = (out_dr - (out_ar + out_br + out_cr)) as f32;

            self.count_e += 1; if self.count_e > delay_e { self.count_e = 0; }
            self.count_f += 1; if self.count_f > delay_f { self.count_f = 0; }
            self.count_g += 1; if self.count_g > delay_g { self.count_g = 0; }
            self.count_h += 1; if self.count_h > delay_h { self.count_h = 0; }

            let rel = self.count_e - if self.count_e > delay_e { delay_e + 1 } else { 0 };
            let rfl = self.count_f - if self.count_f > delay_f { delay_f + 1 } else { 0 };
            let rgl = self.count_g - if self.count_g > delay_g { delay_g + 1 } else { 0 };
            let rhl = self.count_h - if self.count_h > delay_h { delay_h + 1 } else { 0 };

            let out_el = self.a_el[rel] as f64; let out_fl = self.a_fl[rfl] as f64;
            let out_gl = self.a_gl[rgl] as f64; let out_hl = self.a_hl[rhl] as f64;
            let out_er = self.a_er[rel] as f64; let out_fr = self.a_fr[rfl] as f64;
            let out_gr = self.a_gr[rgl] as f64; let out_hr = self.a_hr[rhl] as f64;

            self.feedback_al = out_el - (out_fl + out_gl + out_hl);
            self.feedback_bl = out_fl - (out_el + out_gl + out_hl);
            self.feedback_cl = out_gl - (out_el + out_fl + out_hl);
            self.feedback_dl = out_hl - (out_el + out_fl + out_gl);
            self.feedback_ar = out_er - (out_fr + out_gr + out_hr);
            self.feedback_br = out_fr - (out_er + out_gr + out_hr);
            self.feedback_cr = out_gr - (out_er + out_fr + out_hr);
            self.feedback_dr = out_hr - (out_er + out_fr + out_gr);

            sl = (out_el + out_fl + out_gl + out_hl) / 8.0;
            sr = (out_er + out_fr + out_gr + out_hr) / 8.0;

            match cycle_end {
                4 => {
                    self.last_ref_l[0] = self.last_ref_l[4];
                    self.last_ref_l[2] = (self.last_ref_l[0] as f64 + sl) as f32 / 2.0;
                    self.last_ref_l[1] = (self.last_ref_l[0] as f64 + self.last_ref_l[2] as f64) as f32 / 2.0;
                    self.last_ref_l[3] = (self.last_ref_l[2] as f64 + sl) as f32 / 2.0;
                    self.last_ref_l[4] = sl as f32;
                    self.last_ref_r[0] = self.last_ref_r[4];
                    self.last_ref_r[2] = (self.last_ref_r[0] as f64 + sr) as f32 / 2.0;
                    self.last_ref_r[1] = (self.last_ref_r[0] as f64 + self.last_ref_r[2] as f64) as f32 / 2.0;
                    self.last_ref_r[3] = (self.last_ref_r[2] as f64 + sr) as f32 / 2.0;
                    self.last_ref_r[4] = sr as f32;
                },
                3 => {
                    self.last_ref_l[0] = self.last_ref_l[3];
                    self.last_ref_l[2] = ((self.last_ref_l[0] as f64 * 2.0 + sl) / 3.0) as f32;
                    self.last_ref_l[1] = ((self.last_ref_l[0] as f64 + sl * 2.0) / 3.0) as f32;
                    self.last_ref_l[3] = sl as f32;
                    self.last_ref_r[0] = self.last_ref_r[3];
                    self.last_ref_r[2] = ((self.last_ref_r[0] as f64 * 2.0 + sr) / 3.0) as f32;
                    self.last_ref_r[1] = ((self.last_ref_r[0] as f64 + sr * 2.0) / 3.0) as f32;
                    self.last_ref_r[3] = sr as f32;
                },
                2 => {
                    self.last_ref_l[0] = self.last_ref_l[2];
                    self.last_ref_l[1] = (self.last_ref_l[0] as f64 + sl) as f32 / 2.0;
                    self.last_ref_l[2] = sl as f32;
                    self.last_ref_r[0] = self.last_ref_r[2];
                    self.last_ref_r[1] = (self.last_ref_r[0] as f64 + sr) as f32 / 2.0;
                    self.last_ref_r[2] = sr as f32;
                },
                _ => {
                    self.last_ref_l[0] = sl as f32;
                    self.last_ref_r[0] = sr as f32;
                }
            }
            self.cycle = 0;
        }

        sl = self.last_ref_l[self.cycle as usize] as f64;
        sr = self.last_ref_r[self.cycle as usize] as f64;

        if self.iir_bl.abs() < 1.18e-37 { self.iir_bl = 0.0; }
        self.iir_bl = self.iir_bl * (1.0 - lowpass) + sl * lowpass; sl = self.iir_bl;
        if self.iir_br.abs() < 1.18e-37 { self.iir_br = 0.0; }
        self.iir_br = self.iir_br * (1.0 - lowpass) + sr * lowpass; sr = self.iir_br;

        let out_l = (dry_l as f64 * (1.0 - wet) + sl * wet) as f32;
        let out_r = (dry_r as f64 * (1.0 - wet) + sr * wet) as f32;
        (out_l, out_r)
    }
}


// ─── Mode 21: Verbity ───────────────────────────────────────────────────────

pub struct AirwindowsVerbity {
    a_il: Vec<f32>, a_jl: Vec<f32>, a_kl: Vec<f32>, a_ll: Vec<f32>,
    a_ir: Vec<f32>, a_jr: Vec<f32>, a_kr: Vec<f32>, a_lr: Vec<f32>,
    a_al: Vec<f32>, a_bl: Vec<f32>, a_cl: Vec<f32>, a_dl: Vec<f32>,
    a_ar: Vec<f32>, a_br: Vec<f32>, a_cr: Vec<f32>, a_dr: Vec<f32>,
    a_el: Vec<f32>, a_fl: Vec<f32>, a_gl: Vec<f32>, a_hl: Vec<f32>,
    a_er: Vec<f32>, a_fr: Vec<f32>, a_gr: Vec<f32>, a_hr: Vec<f32>,
    feedback_al: f64, feedback_bl: f64, feedback_cl: f64, feedback_dl: f64,
    feedback_ar: f64, feedback_br: f64, feedback_cr: f64, feedback_dr: f64,
    prev_al: f64, prev_bl: f64, prev_cl: f64, prev_dl: f64,
    prev_ar: f64, prev_br: f64, prev_cr: f64, prev_dr: f64,
    iir_al: f64, iir_ar: f64, iir_bl: f64, iir_br: f64,
    thunder_l: f64, thunder_r: f64,
    cycle: i32,
    count_i: usize, count_j: usize, count_k: usize, count_l: usize,
    count_a: usize, count_b: usize, count_c: usize, count_d: usize,
    count_e: usize, count_f: usize, count_g: usize, count_h: usize,
    last_ref_l: [f32; 7], last_ref_r: [f32; 7],
}
impl AirwindowsVerbity {
    pub fn new() -> Self {
        Self {
            a_il: vec![0.0f32; 9700], a_jl: vec![0.0f32; 5200],
            a_kl: vec![0.0f32; 2500], a_ll: vec![0.0f32; 1100],
            a_ir: vec![0.0f32; 9700], a_jr: vec![0.0f32; 5200],
            a_kr: vec![0.0f32; 2500], a_lr: vec![0.0f32; 1100],
            a_al: vec![0.0f32; 9600], a_bl: vec![0.0f32; 5800],
            a_cl: vec![0.0f32; 2600], a_dl: vec![0.0f32; 1300],
            a_ar: vec![0.0f32; 9600], a_br: vec![0.0f32; 5800],
            a_cr: vec![0.0f32; 2600], a_dr: vec![0.0f32; 1300],
            a_el: vec![0.0f32; 15000], a_fl: vec![0.0f32; 8500],
            a_gl: vec![0.0f32; 4700], a_hl: vec![0.0f32; 3500],
            a_er: vec![0.0f32; 15000], a_fr: vec![0.0f32; 8500],
            a_gr: vec![0.0f32; 4700], a_hr: vec![0.0f32; 3500],
            feedback_al: 0.0, feedback_bl: 0.0, feedback_cl: 0.0, feedback_dl: 0.0,
            feedback_ar: 0.0, feedback_br: 0.0, feedback_cr: 0.0, feedback_dr: 0.0,
            prev_al: 0.0, prev_bl: 0.0, prev_cl: 0.0, prev_dl: 0.0,
            prev_ar: 0.0, prev_br: 0.0, prev_cr: 0.0, prev_dr: 0.0,
            iir_al: 0.0, iir_ar: 0.0, iir_bl: 0.0, iir_br: 0.0,
            thunder_l: 0.0, thunder_r: 0.0,
            cycle: 0,
            count_i: 0, count_j: 0, count_k: 0, count_l: 0,
            count_a: 0, count_b: 0, count_c: 0, count_d: 0,
            count_e: 0, count_f: 0, count_g: 0, count_h: 0,
            last_ref_l: [0.0f32; 7], last_ref_r: [0.0f32; 7],
        }
    }

    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let cycle_end = (overallscale.floor() as i32).clamp(1, 4);
        if self.cycle > cycle_end - 1 { self.cycle = cycle_end - 1; }

        let size = 1.87f64;
        let regen = 0.0625f64;
        let lowpass = (1.0 - (0.5f64).powi(2)) / overallscale.sqrt();
        let interpolate = (0.5f64).powi(2) * 0.618033988749894848f64;
        let thunder_amount = (0.3 - 0.0 * 0.22) * 0.5 * 0.1;

        let delay_i = (3407.0 * size) as usize;
        let delay_j = (1823.0 * size) as usize;
        let delay_k = (859.0 * size) as usize;
        let delay_l = (331.0 * size) as usize;
        let delay_a = (4801.0 * size) as usize;
        let delay_b = (2909.0 * size) as usize;
        let delay_c = (1153.0 * size) as usize;
        let delay_d = (461.0 * size) as usize;
        let delay_e = (7607.0 * size) as usize;
        let delay_f = (4217.0 * size) as usize;
        let delay_g = (2269.0 * size) as usize;
        let delay_h = (1597.0 * size) as usize;

        let wet = drive as f64;
        let dry = (1.0 - wet).max(0.0).min(1.0);
        let wet = wet.max(0.0).min(1.0);

        let dry_l = in_l;
        let dry_r = in_r;

        let mut sl = in_l as f64;
        let mut sr = in_r as f64;

        if self.iir_al.abs() < 1.18e-37 { self.iir_al = 0.0; }
        self.iir_al = self.iir_al * (1.0 - lowpass) + sl * lowpass; sl = self.iir_al;
        if self.iir_ar.abs() < 1.18e-37 { self.iir_ar = 0.0; }
        self.iir_ar = self.iir_ar * (1.0 - lowpass) + sr * lowpass; sr = self.iir_ar;

        self.cycle += 1;
        if self.cycle == cycle_end {
            self.feedback_al = self.feedback_al * (1.0 - interpolate) + self.prev_al * interpolate; self.prev_al = self.feedback_al;
            self.feedback_bl = self.feedback_bl * (1.0 - interpolate) + self.prev_bl * interpolate; self.prev_bl = self.feedback_bl;
            self.feedback_cl = self.feedback_cl * (1.0 - interpolate) + self.prev_cl * interpolate; self.prev_cl = self.feedback_cl;
            self.feedback_dl = self.feedback_dl * (1.0 - interpolate) + self.prev_dl * interpolate; self.prev_dl = self.feedback_dl;
            self.feedback_ar = self.feedback_ar * (1.0 - interpolate) + self.prev_ar * interpolate; self.prev_ar = self.feedback_ar;
            self.feedback_br = self.feedback_br * (1.0 - interpolate) + self.prev_br * interpolate; self.prev_br = self.feedback_br;
            self.feedback_cr = self.feedback_cr * (1.0 - interpolate) + self.prev_cr * interpolate; self.prev_cr = self.feedback_cr;
            self.feedback_dr = self.feedback_dr * (1.0 - interpolate) + self.prev_dr * interpolate; self.prev_dr = self.feedback_dr;

            self.thunder_l = self.thunder_l * 0.99 - self.feedback_al * thunder_amount;
            self.thunder_r = self.thunder_r * 0.99 - self.feedback_ar * thunder_amount;

            self.a_il[self.count_i] = (sl + (self.feedback_al + self.thunder_l) * regen) as f32;
            self.a_jl[self.count_j] = (sl + self.feedback_bl * regen) as f32;
            self.a_kl[self.count_k] = (sl + self.feedback_cl * regen) as f32;
            self.a_ll[self.count_l] = (sl + self.feedback_dl * regen) as f32;
            self.a_ir[self.count_i] = (sr + (self.feedback_ar + self.thunder_r) * regen) as f32;
            self.a_jr[self.count_j] = (sr + self.feedback_br * regen) as f32;
            self.a_kr[self.count_k] = (sr + self.feedback_cr * regen) as f32;
            self.a_lr[self.count_l] = (sr + self.feedback_dr * regen) as f32;

            self.count_i += 1; if self.count_i > delay_i { self.count_i = 0; }
            self.count_j += 1; if self.count_j > delay_j { self.count_j = 0; }
            self.count_k += 1; if self.count_k > delay_k { self.count_k = 0; }
            self.count_l += 1; if self.count_l > delay_l { self.count_l = 0; }

            let ri = self.count_i - if self.count_i > delay_i { delay_i + 1 } else { 0 };
            let rj = self.count_j - if self.count_j > delay_j { delay_j + 1 } else { 0 };
            let rk = self.count_k - if self.count_k > delay_k { delay_k + 1 } else { 0 };
            let rl = self.count_l - if self.count_l > delay_l { delay_l + 1 } else { 0 };
            let out_il = self.a_il[ri] as f64; let out_jl = self.a_jl[rj] as f64;
            let out_kl = self.a_kl[rk] as f64; let out_ll = self.a_ll[rl] as f64;
            let out_ir = self.a_ir[ri] as f64; let out_jr = self.a_jr[rj] as f64;
            let out_kr = self.a_kr[rk] as f64; let out_lr = self.a_lr[rl] as f64;

            self.a_al[self.count_a] = (out_il - (out_jl + out_kl + out_ll)) as f32;
            self.a_bl[self.count_b] = (out_jl - (out_il + out_kl + out_ll)) as f32;
            self.a_cl[self.count_c] = (out_kl - (out_il + out_jl + out_ll)) as f32;
            self.a_dl[self.count_d] = (out_ll - (out_il + out_jl + out_kl)) as f32;
            self.a_ar[self.count_a] = (out_ir - (out_jr + out_kr + out_lr)) as f32;
            self.a_br[self.count_b] = (out_jr - (out_ir + out_kr + out_lr)) as f32;
            self.a_cr[self.count_c] = (out_kr - (out_ir + out_jr + out_lr)) as f32;
            self.a_dr[self.count_d] = (out_lr - (out_ir + out_jr + out_kr)) as f32;

            self.count_a += 1; if self.count_a > delay_a { self.count_a = 0; }
            self.count_b += 1; if self.count_b > delay_b { self.count_b = 0; }
            self.count_c += 1; if self.count_c > delay_c { self.count_c = 0; }
            self.count_d += 1; if self.count_d > delay_d { self.count_d = 0; }

            let ra = self.count_a - if self.count_a > delay_a { delay_a + 1 } else { 0 };
            let rb = self.count_b - if self.count_b > delay_b { delay_b + 1 } else { 0 };
            let rc = self.count_c - if self.count_c > delay_c { delay_c + 1 } else { 0 };
            let rd = self.count_d - if self.count_d > delay_d { delay_d + 1 } else { 0 };
            let out_al = self.a_al[ra] as f64; let out_bl = self.a_bl[rb] as f64;
            let out_cl = self.a_cl[rc] as f64; let out_dl = self.a_dl[rd] as f64;
            let out_ar = self.a_ar[ra] as f64; let out_br = self.a_br[rb] as f64;
            let out_cr = self.a_cr[rc] as f64; let out_dr = self.a_dr[rd] as f64;

            self.a_el[self.count_e] = (out_al - (out_bl + out_cl + out_dl)) as f32;
            self.a_fl[self.count_f] = (out_bl - (out_al + out_cl + out_dl)) as f32;
            self.a_gl[self.count_g] = (out_cl - (out_al + out_bl + out_dl)) as f32;
            self.a_hl[self.count_h] = (out_dl - (out_al + out_bl + out_cl)) as f32;
            self.a_er[self.count_e] = (out_ar - (out_br + out_cr + out_dr)) as f32;
            self.a_fr[self.count_f] = (out_br - (out_ar + out_cr + out_dr)) as f32;
            self.a_gr[self.count_g] = (out_cr - (out_ar + out_br + out_dr)) as f32;
            self.a_hr[self.count_h] = (out_dr - (out_ar + out_br + out_cr)) as f32;

            self.count_e += 1; if self.count_e > delay_e { self.count_e = 0; }
            self.count_f += 1; if self.count_f > delay_f { self.count_f = 0; }
            self.count_g += 1; if self.count_g > delay_g { self.count_g = 0; }
            self.count_h += 1; if self.count_h > delay_h { self.count_h = 0; }

            let re = self.count_e - if self.count_e > delay_e { delay_e + 1 } else { 0 };
            let rf = self.count_f - if self.count_f > delay_f { delay_f + 1 } else { 0 };
            let rg = self.count_g - if self.count_g > delay_g { delay_g + 1 } else { 0 };
            let rh = self.count_h - if self.count_h > delay_h { delay_h + 1 } else { 0 };
            let out_el = self.a_el[re] as f64; let out_fl = self.a_fl[rf] as f64;
            let out_gl = self.a_gl[rg] as f64; let out_hl = self.a_hl[rh] as f64;
            let out_er = self.a_er[re] as f64; let out_fr = self.a_fr[rf] as f64;
            let out_gr = self.a_gr[rg] as f64; let out_hr = self.a_hr[rh] as f64;

            self.feedback_al = out_el - (out_fl + out_gl + out_hl);
            self.feedback_bl = out_fl - (out_el + out_gl + out_hl);
            self.feedback_cl = out_gl - (out_el + out_fl + out_hl);
            self.feedback_dl = out_hl - (out_el + out_fl + out_gl);
            self.feedback_ar = out_er - (out_fr + out_gr + out_hr);
            self.feedback_br = out_fr - (out_er + out_gr + out_hr);
            self.feedback_cr = out_gr - (out_er + out_fr + out_hr);
            self.feedback_dr = out_hr - (out_er + out_fr + out_gr);

            sl = (out_el + out_fl + out_gl + out_hl) / 8.0;
            sr = (out_er + out_fr + out_gr + out_hr) / 8.0;

            match cycle_end {
                4 => {
                    self.last_ref_l[0] = self.last_ref_l[4];
                    self.last_ref_l[2] = ((self.last_ref_l[0] as f64 + sl) / 2.0) as f32;
                    self.last_ref_l[1] = ((self.last_ref_l[0] as f64 + self.last_ref_l[2] as f64) / 2.0) as f32;
                    self.last_ref_l[3] = ((self.last_ref_l[2] as f64 + sl) / 2.0) as f32;
                    self.last_ref_l[4] = sl as f32;
                    self.last_ref_r[0] = self.last_ref_r[4];
                    self.last_ref_r[2] = ((self.last_ref_r[0] as f64 + sr) / 2.0) as f32;
                    self.last_ref_r[1] = ((self.last_ref_r[0] as f64 + self.last_ref_r[2] as f64) / 2.0) as f32;
                    self.last_ref_r[3] = ((self.last_ref_r[2] as f64 + sr) / 2.0) as f32;
                    self.last_ref_r[4] = sr as f32;
                },
                3 => {
                    self.last_ref_l[0] = self.last_ref_l[3];
                    self.last_ref_l[2] = ((self.last_ref_l[0] as f64 * 2.0 + sl) / 3.0) as f32;
                    self.last_ref_l[1] = ((self.last_ref_l[0] as f64 + sl * 2.0) / 3.0) as f32;
                    self.last_ref_l[3] = sl as f32;
                    self.last_ref_r[0] = self.last_ref_r[3];
                    self.last_ref_r[2] = ((self.last_ref_r[0] as f64 * 2.0 + sr) / 3.0) as f32;
                    self.last_ref_r[1] = ((self.last_ref_r[0] as f64 + sr * 2.0) / 3.0) as f32;
                    self.last_ref_r[3] = sr as f32;
                },
                2 => {
                    self.last_ref_l[0] = self.last_ref_l[2];
                    self.last_ref_l[1] = ((self.last_ref_l[0] as f64 + sl) / 2.0) as f32;
                    self.last_ref_l[2] = sl as f32;
                    self.last_ref_r[0] = self.last_ref_r[2];
                    self.last_ref_r[1] = ((self.last_ref_r[0] as f64 + sr) / 2.0) as f32;
                    self.last_ref_r[2] = sr as f32;
                },
                _ => {
                    self.last_ref_l[0] = sl as f32;
                    self.last_ref_r[0] = sr as f32;
                }
            }
            self.cycle = 0;
        }

        sl = self.last_ref_l[self.cycle as usize] as f64;
        sr = self.last_ref_r[self.cycle as usize] as f64;

        if self.iir_bl.abs() < 1.18e-37 { self.iir_bl = 0.0; }
        self.iir_bl = self.iir_bl * (1.0 - lowpass) + sl * lowpass; sl = self.iir_bl;
        if self.iir_br.abs() < 1.18e-37 { self.iir_br = 0.0; }
        self.iir_br = self.iir_br * (1.0 - lowpass) + sr * lowpass; sr = self.iir_br;

        let out_l = (sl * wet + dry_l as f64 * dry) as f32;
        let out_r = (sr * wet + dry_r as f64 * dry) as f32;
        (out_l, out_r)
    }
}


// ─── Mode 22: Capacitor ─────────────────────────────────────────────────────

pub struct AirwindowsCapacitor {
    iir_hpa_l: f64, iir_hpb_l: f64, iir_hpc_l: f64, iir_hpd_l: f64, iir_hpe_l: f64, iir_hpf_l: f64,
    iir_lpa_l: f64, iir_lpb_l: f64, iir_lpc_l: f64, iir_lpd_l: f64, iir_lpe_l: f64, iir_lpf_l: f64,
    iir_hpa_r: f64, iir_hpb_r: f64, iir_hpc_r: f64, iir_hpd_r: f64, iir_hpe_r: f64, iir_hpf_r: f64,
    iir_lpa_r: f64, iir_lpb_r: f64, iir_lpc_r: f64, iir_lpd_r: f64, iir_lpe_r: f64, iir_lpf_r: f64,
    lowpass_amount: f64, highpass_amount: f64, wet: f64,
    count: i32,
}
impl AirwindowsCapacitor {
    pub fn new() -> Self {
        Self {
            iir_hpa_l: 0.0, iir_hpb_l: 0.0, iir_hpc_l: 0.0, iir_hpd_l: 0.0, iir_hpe_l: 0.0, iir_hpf_l: 0.0,
            iir_lpa_l: 0.0, iir_lpb_l: 0.0, iir_lpc_l: 0.0, iir_lpd_l: 0.0, iir_lpe_l: 0.0, iir_lpf_l: 0.0,
            iir_hpa_r: 0.0, iir_hpb_r: 0.0, iir_hpc_r: 0.0, iir_hpd_r: 0.0, iir_hpe_r: 0.0, iir_hpf_r: 0.0,
            iir_lpa_r: 0.0, iir_lpb_r: 0.0, iir_lpc_r: 0.0, iir_lpd_r: 0.0, iir_lpe_r: 0.0, iir_lpf_r: 0.0,
            lowpass_amount: 1.0, highpass_amount: 0.0, wet: 1.0,
            count: 0,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let lowpass_chase = (drive as f64).powi(2);
        let highpass_chase = 0.0f64;
        let wet_chase = 1.0f64;

        let lowpass_speed = 300.0 / ((self.lowpass_amount - lowpass_chase).abs() + 1.0);
        let highpass_speed = 300.0 / ((self.highpass_amount - highpass_chase).abs() + 1.0);
        let wet_speed = 300.0 / ((self.wet - wet_chase).abs() + 1.0);

        self.lowpass_amount = (self.lowpass_amount * lowpass_speed + lowpass_chase) / (lowpass_speed + 1.0);
        self.highpass_amount = (self.highpass_amount * highpass_speed + highpass_chase) / (highpass_speed + 1.0);
        self.wet = (self.wet * wet_speed + wet_chase) / (wet_speed + 1.0);

        let inv_lp = 1.0 - self.lowpass_amount;
        let inv_hp = 1.0 - self.highpass_amount;
        let dry = 1.0 - self.wet;

        let dry_l = in_l;
        let dry_r = in_r;
        let mut sl = in_l as f64;
        let mut sr = in_r as f64;

        self.count += 1; if self.count > 5 { self.count = 0; }
        match self.count {
            0 => {
                self.iir_hpa_l = self.iir_hpa_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpa_l;
                self.iir_lpa_l = self.iir_lpa_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpa_l;
                self.iir_hpb_l = self.iir_hpb_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpb_l;
                self.iir_lpb_l = self.iir_lpb_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpb_l;
                self.iir_hpd_l = self.iir_hpd_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpd_l;
                self.iir_lpd_l = self.iir_lpd_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpd_l;
                self.iir_hpa_r = self.iir_hpa_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpa_r;
                self.iir_lpa_r = self.iir_lpa_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpa_r;
                self.iir_hpb_r = self.iir_hpb_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpb_r;
                self.iir_lpb_r = self.iir_lpb_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpb_r;
                self.iir_hpd_r = self.iir_hpd_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpd_r;
                self.iir_lpd_r = self.iir_lpd_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpd_r;
            },
            1 => {
                self.iir_hpa_l = self.iir_hpa_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpa_l;
                self.iir_lpa_l = self.iir_lpa_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpa_l;
                self.iir_hpc_l = self.iir_hpc_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpc_l;
                self.iir_lpc_l = self.iir_lpc_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpc_l;
                self.iir_hpe_l = self.iir_hpe_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpe_l;
                self.iir_lpe_l = self.iir_lpe_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpe_l;
                self.iir_hpa_r = self.iir_hpa_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpa_r;
                self.iir_lpa_r = self.iir_lpa_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpa_r;
                self.iir_hpc_r = self.iir_hpc_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpc_r;
                self.iir_lpc_r = self.iir_lpc_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpc_r;
                self.iir_hpe_r = self.iir_hpe_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpe_r;
                self.iir_lpe_r = self.iir_lpe_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpe_r;
            },
            2 => {
                self.iir_hpa_l = self.iir_hpa_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpa_l;
                self.iir_lpa_l = self.iir_lpa_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpa_l;
                self.iir_hpb_l = self.iir_hpb_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpb_l;
                self.iir_lpb_l = self.iir_lpb_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpb_l;
                self.iir_hpf_l = self.iir_hpf_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpf_l;
                self.iir_lpf_l = self.iir_lpf_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpf_l;
                self.iir_hpa_r = self.iir_hpa_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpa_r;
                self.iir_lpa_r = self.iir_lpa_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpa_r;
                self.iir_hpb_r = self.iir_hpb_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpb_r;
                self.iir_lpb_r = self.iir_lpb_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpb_r;
                self.iir_hpf_r = self.iir_hpf_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpf_r;
                self.iir_lpf_r = self.iir_lpf_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpf_r;
            },
            3 => {
                self.iir_hpa_l = self.iir_hpa_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpa_l;
                self.iir_lpa_l = self.iir_lpa_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpa_l;
                self.iir_hpc_l = self.iir_hpc_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpc_l;
                self.iir_lpc_l = self.iir_lpc_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpc_l;
                self.iir_hpd_l = self.iir_hpd_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpd_l;
                self.iir_lpd_l = self.iir_lpd_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpd_l;
                self.iir_hpa_r = self.iir_hpa_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpa_r;
                self.iir_lpa_r = self.iir_lpa_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpa_r;
                self.iir_hpc_r = self.iir_hpc_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpc_r;
                self.iir_lpc_r = self.iir_lpc_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpc_r;
                self.iir_hpd_r = self.iir_hpd_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpd_r;
                self.iir_lpd_r = self.iir_lpd_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpd_r;
            },
            4 => {
                self.iir_hpa_l = self.iir_hpa_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpa_l;
                self.iir_lpa_l = self.iir_lpa_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpa_l;
                self.iir_hpb_l = self.iir_hpb_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpb_l;
                self.iir_lpb_l = self.iir_lpb_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpb_l;
                self.iir_hpe_l = self.iir_hpe_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpe_l;
                self.iir_lpe_l = self.iir_lpe_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpe_l;
                self.iir_hpa_r = self.iir_hpa_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpa_r;
                self.iir_lpa_r = self.iir_lpa_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpa_r;
                self.iir_hpb_r = self.iir_hpb_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpb_r;
                self.iir_lpb_r = self.iir_lpb_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpb_r;
                self.iir_hpe_r = self.iir_hpe_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpe_r;
                self.iir_lpe_r = self.iir_lpe_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpe_r;
            },
            _ => {
                self.iir_hpa_l = self.iir_hpa_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpa_l;
                self.iir_lpa_l = self.iir_lpa_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpa_l;
                self.iir_hpc_l = self.iir_hpc_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpc_l;
                self.iir_lpc_l = self.iir_lpc_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpc_l;
                self.iir_hpf_l = self.iir_hpf_l * inv_hp + sl * self.highpass_amount; sl -= self.iir_hpf_l;
                self.iir_lpf_l = self.iir_lpf_l * inv_lp + sl * self.lowpass_amount; sl = self.iir_lpf_l;
                self.iir_hpa_r = self.iir_hpa_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpa_r;
                self.iir_lpa_r = self.iir_lpa_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpa_r;
                self.iir_hpc_r = self.iir_hpc_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpc_r;
                self.iir_lpc_r = self.iir_lpc_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpc_r;
                self.iir_hpf_r = self.iir_hpf_r * inv_hp + sr * self.highpass_amount; sr -= self.iir_hpf_r;
                self.iir_lpf_r = self.iir_lpf_r * inv_lp + sr * self.lowpass_amount; sr = self.iir_lpf_r;
            },
        }

        let out_l = (dry_l as f64 * dry + sl * self.wet) as f32;
        let out_r = (dry_r as f64 * dry + sr * self.wet) as f32;
        (out_l, out_r)
    }
}

// ─── Mode 23: Focus ─────────────────────────────────────────────────────────

pub struct AirwindowsFocus {
    fig_l: [f64; 9],
    fig_r: [f64; 9],
}
impl AirwindowsFocus {
    pub fn new() -> Self { Self { fig_l: [0.0; 9], fig_r: [0.0; 9] } }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let boost = 10f64.powf(drive as f64 * 12.0 / 20.0);
        let freq = 3515.775 / sr as f64;
        let reso = 0.1f64; // fixed Q
        let k = (std::f64::consts::PI * freq).tan();
        let norm = 1.0 / (1.0 + k / reso + k * k);
        let b0 = k / reso * norm;
        let b2 = -b0;
        let a1 = 2.0 * (k * k - 1.0) * norm;
        let a2 = (1.0 - k / reso + k * k) * norm;
        self.fig_l[2] = b0; self.fig_l[4] = b2; self.fig_l[5] = a1; self.fig_l[6] = a2;
        self.fig_r[2] = b0; self.fig_r[4] = b2; self.fig_r[5] = a1; self.fig_r[6] = a2;

        let dry_l = in_l as f64;
        let dry_r = in_r as f64;
        let mut sl = dry_l.sin();
        let mut sr = dry_r.sin();

        let tmp = sl * self.fig_l[2] + self.fig_l[7];
        self.fig_l[7] = -(tmp * self.fig_l[5]) + self.fig_l[8];
        self.fig_l[8] = sl * self.fig_l[4] - tmp * self.fig_l[6];
        sl = tmp;
        sl = sl.clamp(-1.0, 1.0).asin();

        let tmp = sr * self.fig_r[2] + self.fig_r[7];
        self.fig_r[7] = -(tmp * self.fig_r[5]) + self.fig_r[8];
        self.fig_r[8] = sr * self.fig_r[4] - tmp * self.fig_r[6];
        sr = tmp;
        sr = sr.clamp(-1.0, 1.0).asin();

        let ground_l = dry_l - sl;
        let ground_r = dry_r - sr;
        sl *= boost;
        sr *= boost;

        // Spiral mode
        sl = sl.clamp(-1.2533141373155, 1.2533141373155);
        sl = if sl == 0.0 { sl } else { (sl * sl.abs()).sin() / sl.abs() };
        sr = sr.clamp(-1.2533141373155, 1.2533141373155);
        sr = if sr == 0.0 { sr } else { (sr * sr.abs()).sin() / sr.abs() };

        sl += ground_l;
        sr += ground_r;

        (sl as f32, sr as f32)
    }
}


// ─── Mode 24: YLowpass (simplified interpolated biquad LP) ──────────────────

pub struct AirwindowsYLowpass {
    biquad: [f64; 15],
    fix: [f64; 15],
    b_l: [f64; 2],
    b_r: [f64; 2],
    pow_factor_a: f64, pow_factor_b: f64,
    wet_a: f64, wet_b: f64,
}
impl AirwindowsYLowpass {
    pub fn new() -> Self {
        let mut s = Self {
            biquad: [0.0; 15], fix: [0.0; 15],
            b_l: [0.0; 2], b_r: [0.0; 2],
            pow_factor_a: 1.0, pow_factor_b: 1.0,
            wet_a: 1.0, wet_b: 1.0,
        };
        s.biquad[0] = 1.0; // sane default
        s
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        // drive maps to cutoff freq: low drive = low cutoff = more LP
        let freq = ((drive as f64).powi(3) * 20000.0 + 30.0) / sr as f64;
        let freq = freq.clamp(0.0001, 0.499);
        let reso = 1.5f64;
        let k = (std::f64::consts::PI * freq).tan();
        let norm = 1.0 / (1.0 + k / reso + k * k);
        let b0 = k * k * norm;
        let b1 = 2.0 * b0;
        let a1 = 2.0 * (k * k - 1.0) * norm;
        let a2 = (1.0 - k / reso + k * k) * norm;

        self.biquad[2] = b0;
        self.biquad[3] = b1;
        self.biquad[4] = b0;
        self.biquad[5] = a1;
        self.biquad[6] = a2;

        let mut sl = in_l as f64;
        let mut sr = in_r as f64;

        // Direct form II biquad LP L
        let outl = self.biquad[2] * sl + self.b_l[0];
        self.b_l[0] = self.biquad[3] * sl - self.biquad[5] * outl + self.b_l[1];
        self.b_l[1] = self.biquad[4] * sl - self.biquad[6] * outl;
        sl = outl;

        let outr = self.biquad[2] * sr + self.b_r[0];
        self.b_r[0] = self.biquad[3] * sr - self.biquad[5] * outr + self.b_r[1];
        self.b_r[1] = self.biquad[4] * sr - self.biquad[6] * outr;
        sr = outr;

        // power soft-clip
        if sl > 1.0 { sl = 1.0; } if sl < -1.0 { sl = -1.0; }
        if sr > 1.0 { sr = 1.0; } if sr < -1.0 { sr = -1.0; }

        (sl as f32, sr as f32)
    }
}

// ─── Mode 25: DubSub (simplified sub-octave + bass enhancer) ────────────────

pub struct AirwindowsDubSub {
    iir_al: f64, iir_bl: f64, iir_cl: f64, iir_dl: f64,
    iir_ar: f64, iir_br: f64, iir_cr: f64, iir_dr: f64,
    iir_head_al: f64, iir_head_bl: f64,
    iir_head_ar: f64, iir_head_br: f64,
    iir_sub_al: f64, iir_sub_bl: f64,
    iir_sub_ar: f64, iir_sub_br: f64,
    flip: bool,
    count: i32,
}
impl AirwindowsDubSub {
    pub fn new() -> Self {
        Self {
            iir_al: 0.0, iir_bl: 0.0, iir_cl: 0.0, iir_dl: 0.0,
            iir_ar: 0.0, iir_br: 0.0, iir_cr: 0.0, iir_dr: 0.0,
            iir_head_al: 0.0, iir_head_bl: 0.0,
            iir_head_ar: 0.0, iir_head_br: 0.0,
            iir_sub_al: 0.0, iir_sub_bl: 0.0,
            iir_sub_ar: 0.0, iir_sub_br: 0.0,
            flip: false, count: 0,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let bass_gain = drive as f64 * 0.1;
        let sub_gain = drive as f64 * 0.05;
        let bass_freq = 0.01f64;
        let overallscale = sr as f64 / 44100.0;
        let iir_amt = bass_freq / overallscale;

        let mut sl = in_l as f64;
        let mut sr = in_r as f64;

        if self.flip {
            self.iir_al = self.iir_al * (1.0 - iir_amt) + sl * iir_amt;
            self.iir_ar = self.iir_ar * (1.0 - iir_amt) + sr * iir_amt;
            self.iir_head_al += sl * 0.05;
            self.iir_head_al -= self.iir_head_al.powi(3) * 0.02 / overallscale;
            self.iir_head_al = self.iir_head_al.sin();
            self.iir_head_ar += sr * 0.05;
            self.iir_head_ar -= self.iir_head_ar.powi(3) * 0.02 / overallscale;
            self.iir_head_ar = self.iir_head_ar.sin();
            self.iir_sub_al = self.iir_sub_al * (1.0 - iir_amt * 0.5) + self.iir_head_al * iir_amt * 0.5;
            self.iir_sub_ar = self.iir_sub_ar * (1.0 - iir_amt * 0.5) + self.iir_head_ar * iir_amt * 0.5;
        } else {
            self.iir_bl = self.iir_bl * (1.0 - iir_amt) + sl * iir_amt;
            self.iir_br = self.iir_br * (1.0 - iir_amt) + sr * iir_amt;
            self.iir_head_bl += sl * 0.05;
            self.iir_head_bl -= self.iir_head_bl.powi(3) * 0.02 / overallscale;
            self.iir_head_bl = self.iir_head_bl.sin();
            self.iir_head_br += sr * 0.05;
            self.iir_head_br -= self.iir_head_br.powi(3) * 0.02 / overallscale;
            self.iir_head_br = self.iir_head_br.sin();
            self.iir_sub_bl = self.iir_sub_bl * (1.0 - iir_amt * 0.5) + self.iir_head_bl * iir_amt * 0.5;
            self.iir_sub_br = self.iir_sub_br * (1.0 - iir_amt * 0.5) + self.iir_head_br * iir_amt * 0.5;
        }
        self.flip = !self.flip;

        let bass_l = if self.flip { self.iir_al } else { self.iir_bl };
        let bass_r = if self.flip { self.iir_ar } else { self.iir_br };
        let sub_l = if self.flip { self.iir_sub_al } else { self.iir_sub_bl };
        let sub_r = if self.flip { self.iir_sub_ar } else { self.iir_sub_br };

        sl += bass_l * bass_gain + sub_l * sub_gain;
        sr += bass_r * bass_gain + sub_r * sub_gain;

        (sl as f32, sr as f32)
    }
}

// ─── Mode 26: Melt ──────────────────────────────────────────────────────────

pub struct AirwindowsMelt {
    d_l: Vec<f32>,
    d_r: Vec<f32>,
    gcount: i32,
    position: [i32; 31],
    step_tap: [i32; 31],
    slow_count: f64,
    step_count: i32,
    combine_l: f64,
    combine_r: f64,
    scale_l: f64,
    scale_r: f64,
}
impl AirwindowsMelt {
    const PRIMES: [i32; 31] = [2,3,5,7,11,13,17,19,23,29,31,37,41,43,47,53,59,61,67,71,73,79,83,89,97,101,103,107,109,113,117];
    pub fn new() -> Self {
        Self {
            d_l: vec![0.0f32; 32001],
            d_r: vec![0.0f32; 32001],
            gcount: 0,
            position: [4; 31],
            step_tap: [1; 31],
            slow_count: 0.0,
            step_count: 0,
            combine_l: 0.0,
            combine_r: 0.0,
            scale_l: 1.0,
            scale_r: 1.0,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        let rate = 1.0 / ((drive as f64).powi(2) + 0.001);
        let depth_b = drive as f64 * 139.5 + 2.0;
        let depth_a = depth_b * (1.0 - drive as f64);
        let output = 0.6 * 0.05;
        let wet = drive as f64;

        let dry_l = in_l;
        let dry_r = in_r;

        if self.gcount < 0 || self.gcount > 16000 { self.gcount = 16000; }
        let gc = self.gcount as usize;
        self.d_l[gc + 16000] = in_l; self.d_l[gc] = in_l;
        self.d_r[gc + 16000] = in_r; self.d_r[gc] = in_r;

        if self.slow_count > rate || self.slow_count < 0.0 {
            self.slow_count = 0.0;
            self.step_count += 1;
            if self.step_count > 29 || self.step_count < 0 { self.step_count = 0; }
            let sc = self.step_count as usize;
            self.position[sc] += self.step_tap[sc];
            let min_tap = (Self::PRIMES[sc] as f64 * depth_a).floor() as i32;
            let max_tap = (Self::PRIMES[sc] as f64 * depth_b).floor() as i32;
            if self.position[sc] < min_tap { self.position[sc] = min_tap; self.step_tap[sc] = 1; }
            if self.position[sc] > max_tap { self.position[sc] = max_tap; self.step_tap[sc] = -1; }
        }

        // combfilter L
        self.scale_l *= 0.9999;
        self.scale_l += (100.0 - self.combine_l.abs()) * 0.000001;
        for i in (0..15).rev() {
            self.combine_l *= self.scale_l;
            self.combine_l -= self.d_l[gc + self.position[i*2+1] as usize] as f64;
            self.combine_l += self.d_l[gc + self.position[i*2] as usize] as f64;
        }

        // combfilter R
        self.scale_r *= 0.9999;
        self.scale_r += (100.0 - self.combine_r.abs()) * 0.000001;
        for i in (0..15).rev() {
            self.combine_r *= self.scale_r;
            self.combine_r -= self.d_r[gc + self.position[i*2+1] as usize] as f64;
            self.combine_r += self.d_r[gc + self.position[i*2] as usize] as f64;
        }

        self.gcount -= 1;
        self.slow_count += 1.0;

        let sl = self.combine_l * output;
        let sr = self.combine_r * output;
        let out_l = (dry_l as f64 * (1.0 - wet) + sl * wet) as f32;
        let out_r = (dry_r as f64 * (1.0 - wet) + sr * wet) as f32;
        (out_l, out_r)
    }
}

// ─── Mode 27: Pop (simplified compressor with thickening) ───────────────────

pub struct AirwindowsPop {
    mu_speed_a: f64, mu_speed_b: f64,
    mu_coeff_a: f64, mu_coeff_b: f64,
    mu_vary: f64, mu_new_speed: f64,
    flip: bool,
}
impl AirwindowsPop {
    pub fn new() -> Self {
        Self {
            mu_speed_a: 10000.0, mu_speed_b: 10000.0,
            mu_coeff_a: 1.0, mu_coeff_b: 1.0,
            mu_vary: 1.0, mu_new_speed: 0.0,
            flip: false,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let threshold = 1.0 - (1.0 - drive as f64).powi(5);
        let release = 32768.0 / overallscale;
        let fastest = release.sqrt();

        let mut sl = in_l as f64;
        let mut sr = in_r as f64;
        let input_sense = sl.abs().max(sr.abs());

        if self.flip {
            if input_sense > threshold {
                self.mu_vary = threshold / input_sense.max(1e-6);
                let mu_attack = self.mu_speed_a.abs().sqrt();
                self.mu_coeff_a *= mu_attack - 1.0;
                self.mu_coeff_a += if self.mu_vary < threshold { threshold } else { self.mu_vary };
                self.mu_coeff_a /= mu_attack;
            } else {
                self.mu_coeff_a *= self.mu_speed_a * self.mu_speed_a - 1.0;
                self.mu_coeff_a += 1.0;
                self.mu_coeff_a /= self.mu_speed_a * self.mu_speed_a;
            }
            self.mu_new_speed = self.mu_speed_a * (self.mu_speed_a - 1.0) + input_sense * release + fastest;
            self.mu_speed_a = self.mu_new_speed / self.mu_speed_a;
            sl *= self.mu_coeff_a; sr *= self.mu_coeff_a;
        } else {
            if input_sense > threshold {
                self.mu_vary = threshold / input_sense.max(1e-6);
                let mu_attack = self.mu_speed_b.abs().sqrt();
                self.mu_coeff_b *= mu_attack - 1.0;
                self.mu_coeff_b += if self.mu_vary < threshold { threshold } else { self.mu_vary };
                self.mu_coeff_b /= mu_attack;
            } else {
                self.mu_coeff_b *= self.mu_speed_b * self.mu_speed_b - 1.0;
                self.mu_coeff_b += 1.0;
                self.mu_coeff_b /= self.mu_speed_b * self.mu_speed_b;
            }
            self.mu_new_speed = self.mu_speed_b * (self.mu_speed_b - 1.0) + input_sense * release + fastest;
            self.mu_speed_b = self.mu_new_speed / self.mu_speed_b;
            sl *= self.mu_coeff_b; sr *= self.mu_coeff_b;
        }
        self.flip = !self.flip;
        (sl as f32, sr as f32)
    }
}

// ─── Mode 28: BitGlitter ────────────────────────────────────────────────────

pub struct AirwindowsBitGlitter {
    position_al: f64, position_bl: f64,
    position_ar: f64, position_br: f64,
    held_al: f64, held_bl: f64,
    held_ar: f64, held_br: f64,
    last_l: f64, last_r: f64,
    ata_last_l: f64, ata_last_r: f64,
    ata_halfway_l: f64, ata_halfway_r: f64,
    last_out_l: f64, last_out_r: f64,
}
impl AirwindowsBitGlitter {
    pub fn new() -> Self {
        Self {
            position_al: 0.0, position_bl: 0.0,
            position_ar: 0.0, position_br: 0.0,
            held_al: 0.0, held_bl: 0.0,
            held_ar: 0.0, held_br: 0.0,
            last_l: 0.0, last_r: 0.0,
            ata_last_l: 0.0, ata_last_r: 0.0,
            ata_halfway_l: 0.0, ata_halfway_r: 0.0,
            last_out_l: 0.0, last_out_r: 0.0,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let factor = drive as f64 + 1.0;
        let factor = factor.powi(7) + 2.0;
        let divvy = (factor * overallscale).max(1.0).floor();
        let rate_a = 1.0 / divvy;
        let rate_b = 1.61803398875 / divvy;
        let rez_a = 0.0016666666666667f64;
        let rez_b = 0.0026666666666667f64;
        let ingain = 10f64.powf((drive as f64 * 36.0 - 18.0) / 14.0);
        let wet = drive as f64;

        let dry_l = in_l;
        let dry_r = in_r;

        let mut sl = (in_l as f64 * ingain).clamp(-1.0, 1.0) * 1.2533141373155;
        let mut sr = (in_r as f64 * ingain).clamp(-1.0, 1.0) * 1.2533141373155;

        // Spiral2 distortion
        sl = if sl == 0.0 { sl } else { (sl * sl.abs()).sin() / sl.abs() };
        sr = if sr == 0.0 { sr } else { (sr * sr.abs()).sin() / sr.abs() };

        // oversampling halfway
        self.ata_halfway_l = (sl + self.ata_last_l) / 2.0;
        self.ata_last_l = sl;
        self.ata_halfway_r = (sr + self.ata_last_r) / 2.0;
        self.ata_last_r = sr;

        // rate A decimation L
        self.position_al += rate_a;
        let mut out_l = self.held_al;
        if self.position_al > 1.0 {
            self.position_al -= 1.0;
            self.held_al = self.last_l * self.position_al + sl * (1.0 - self.position_al);
            out_l = (out_l + self.held_al) * 0.5;
        }
        // quantize
        let mut q = out_l;
        if q > 0.0 { let mut t = q; while t > 0.0 { t -= rez_a; } q -= t; }
        else if q < 0.0 { let mut t = q; while t < 0.0 { t += rez_a; } q -= t; }
        q *= 1.0 - rez_a;
        if q.abs() < rez_a { q = 0.0; }
        sl = q;

        // rate A decimation R
        self.position_ar += rate_a;
        let mut out_r = self.held_ar;
        if self.position_ar > 1.0 {
            self.position_ar -= 1.0;
            self.held_ar = self.last_r * self.position_ar + sr * (1.0 - self.position_ar);
            out_r = (out_r + self.held_ar) * 0.5;
        }
        let mut q = out_r;
        if q > 0.0 { let mut t = q; while t > 0.0 { t -= rez_a; } q -= t; }
        else if q < 0.0 { let mut t = q; while t < 0.0 { t += rez_a; } q -= t; }
        q *= 1.0 - rez_a;
        if q.abs() < rez_a { q = 0.0; }
        sr = q;

        // rate B decimation + blend
        self.position_bl += rate_b;
        let mut out_hl = self.held_bl;
        if self.position_bl > 1.0 {
            self.position_bl -= 1.0;
            self.held_bl = self.last_l * self.position_bl + self.ata_halfway_l * (1.0 - self.position_bl);
            out_hl = (out_hl + self.held_bl) * 0.5;
        }
        let mut q = out_hl;
        if q > 0.0 { let mut t = q; while t > 0.0 { t -= rez_b; } q -= t; }
        else if q < 0.0 { let mut t = q; while t < 0.0 { t += rez_b; } q -= t; }
        q *= 1.0 - rez_b;
        if q.abs() < rez_b { q = 0.0; }
        self.ata_halfway_l = q;

        self.position_br += rate_b;
        let mut out_hr = self.held_br;
        if self.position_br > 1.0 {
            self.position_br -= 1.0;
            self.held_br = self.last_r * self.position_br + self.ata_halfway_r * (1.0 - self.position_br);
            out_hr = (out_hr + self.held_br) * 0.5;
        }
        let mut q = out_hr;
        if q > 0.0 { let mut t = q; while t > 0.0 { t -= rez_b; } q -= t; }
        else if q < 0.0 { let mut t = q; while t < 0.0 { t += rez_b; } q -= t; }
        q *= 1.0 - rez_b;
        if q.abs() < rez_b { q = 0.0; }
        self.ata_halfway_r = q;

        sl = (sl + self.ata_halfway_l) / 2.0;
        sr = (sr + self.ata_halfway_r) / 2.0;

        let out_sl = sl * (1.0 - wet / 2.0) + self.last_out_l * (wet / 2.0);
        let out_sr = sr * (1.0 - wet / 2.0) + self.last_out_r * (wet / 2.0);
        self.last_out_l = sl;
        self.last_out_r = sr;
        self.last_l = in_l as f64;
        self.last_r = in_r as f64;

        let out_l = (dry_l as f64 * (1.0 - wet) + out_sl * wet) as f32;
        let out_r = (dry_r as f64 * (1.0 - wet) + out_sr * wet) as f32;
        (out_l, out_r)
    }
}


// ─── Mode 29: DeRez2 ────────────────────────────────────────────────────────

pub struct AirwindowsDeRez2 {
    increment_a: f64, increment_b: f64,
    position: f64,
    held_l: f64, held_r: f64,
    last_l: f64, last_r: f64,
    last_dry_l: f64, last_dry_r: f64,
    last_out_l: f64, last_out_r: f64,
}
impl AirwindowsDeRez2 {
    pub fn new() -> Self {
        Self {
            increment_a: 1.0, increment_b: 0.0,
            position: 0.0,
            held_l: 0.0, held_r: 0.0,
            last_l: 0.0, last_r: 0.0,
            last_dry_l: 0.0, last_dry_r: 0.0,
            last_out_l: 0.0, last_out_r: 0.0,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        // drive=1 → most decimation (low freq), drive=0 → bypass
        let target_a = ((1.0 - drive as f64).powi(3) + 0.0005).min(1.0) / overallscale;
        let soften = (1.0 + target_a) / 2.0;
        let target_b = (drive as f64).powi(3) / 3.0;
        let hard = 1.0 - drive as f64 * 0.5;

        let dry_l = in_l as f64;
        let dry_r = in_r as f64;
        let mut sl = dry_l;
        let mut sr = dry_r;

        self.increment_a = (self.increment_a * 999.0 + target_a) / 1000.0;
        self.increment_b = (self.increment_b * 999.0 + target_b) / 1000.0;
        self.position += self.increment_a;

        let mut out_l = self.held_l;
        let mut out_r = self.held_r;
        if self.position > 1.0 {
            self.position -= 1.0;
            self.held_l = self.last_l * self.position + sl * (1.0 - self.position);
            out_l = out_l * (1.0 - soften) + self.held_l * soften;
            self.held_r = self.last_r * self.position + sr * (1.0 - self.position);
            out_r = out_r * (1.0 - soften) + self.held_r * soften;
        }
        sl = out_l; sr = out_r;

        // hard/dry interpolation on transitions
        if sl != self.last_out_l {
            let tmp = sl;
            sl = sl * hard + self.last_dry_l * (1.0 - hard);
            self.last_out_l = tmp;
        } else { self.last_out_l = sl; }
        if sr != self.last_out_r {
            let tmp = sr;
            sr = sr * hard + self.last_dry_r * (1.0 - hard);
            self.last_out_r = tmp;
        } else { self.last_out_r = sr; }

        self.last_dry_l = dry_l; self.last_dry_r = dry_r;

        // uLaw encode
        let mut tl = sl; let mut tr = sr;
        tl = tl.clamp(-1.0, 1.0);
        tr = tr.clamp(-1.0, 1.0);
        let encl = if tl > 0.0 { (1.0 + 255.0 * tl.abs()).ln() / 256f64.ln() }
                   else { -(1.0 + 255.0 * tl.abs()).ln() / 256f64.ln() };
        let encr = if tr > 0.0 { (1.0 + 255.0 * tr.abs()).ln() / 256f64.ln() }
                   else { -(1.0 + 255.0 * tr.abs()).ln() / 256f64.ln() };
        sl = tl * hard + encl * (1.0 - hard);
        sr = tr * hard + encr * (1.0 - hard);

        // bit crush
        if self.increment_b > 0.0005 {
            let crush_and_return = |x: f64, step: f64| -> f64 {
                let mut x = x;
                if x > 0.0 { let mut t = x; while t > 0.0 { t -= step; } x -= t; }
                else if x < 0.0 { let mut t = x; while t < 0.0 { t += step; } x -= t; }
                x * (1.0 - step)
            };
            sl = crush_and_return(sl, self.increment_b);
            sr = crush_and_return(sr, self.increment_b);
        }

        // uLaw decode
        let decl = if sl > 0.0 { (256f64.powf(sl.abs()) - 1.0) / 255.0 }
                   else { -(256f64.powf(sl.abs()) - 1.0) / 255.0 };
        let decr = if sr > 0.0 { (256f64.powf(sr.abs()) - 1.0) / 255.0 }
                   else { -(256f64.powf(sr.abs()) - 1.0) / 255.0 };
        sl = sl * hard + decl * (1.0 - hard);
        sr = sr * hard + decr * (1.0 - hard);

        self.last_l = dry_l; self.last_r = dry_r;

        (sl as f32, sr as f32)
    }
}

// ─── Mode 30: BussColors4 (simplified sin saturation) ───────────────────────

pub struct AirwindowsBussColors4 {
    slow_l: f64, slow_r: f64,
    fpd: u32,
}
impl AirwindowsBussColors4 {
    pub fn new() -> Self { Self { slow_l: 0.0, slow_r: 0.0, fpd: 1 } }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        // 8 console colors: varying amounts of saturation
        let color = (drive * 7.999) as u32 + 1; // 1-8
        let gain_factor = match color {
            1 => 0.436, 2 => 0.5, 3 => 0.6, 4 => 0.7,
            5 => 0.8, 6 => 0.9, 7 => 1.0, _ => 1.1,
        };
        let density = color as f64 * 0.3 + 0.5;

        let mut sl = in_l as f64 * gain_factor;
        let mut sr = in_r as f64 * gain_factor;

        // sin saturation
        let mut br = sl.abs() * density;
        if br > HALF_PI_64 { br = HALF_PI_64; }
        br = br.sin();
        sl = if sl > 0.0 { br / density } else { -br / density };

        br = sr.abs() * density;
        if br > HALF_PI_64 { br = HALF_PI_64; }
        br = br.sin();
        sr = if sr > 0.0 { br / density } else { -br / density };

        // envelope for slow dynamic
        self.slow_l = self.slow_l * 0.9999 + sl.abs() * 0.0001;
        self.slow_r = self.slow_r * 0.9999 + sr.abs() * 0.0001;

        (sl as f32, sr as f32)
    }
}

// ─── Mode 31: Hombre ────────────────────────────────────────────────────────

pub struct AirwindowsHombre {
    p_l: Vec<f32>,
    p_r: Vec<f32>,
    gcount: i32,
    slide: f64,
}
impl AirwindowsHombre {
    pub fn new() -> Self {
        Self {
            p_l: vec![0.0f32; 4001],
            p_r: vec![0.0f32; 4001],
            gcount: 0,
            slide: 0.0,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = sr as f64 / 44100.0;
        let target = drive as f64;

        self.slide = self.slide * 0.9997 + target * 0.0003;

        let offset_a = (self.slide.powi(2) * 77.0 + 3.2) * overallscale;
        let offset_b = (3.85 * (self.slide.powi(2) * 77.0 + 3.2) + 41.0) * overallscale;

        let width_a = (1.0 * overallscale) as usize;
        let width_b = (7.0 * overallscale) as usize;

        if self.gcount < 1 || self.gcount > 2000 { self.gcount = 2000; }
        let gc = self.gcount as usize;

        self.p_l[gc] = in_l;
        self.p_r[gc] = in_r;
        if gc + 2000 < self.p_l.len() {
            self.p_l[gc + 2000] = in_l;
            self.p_r[gc + 2000] = in_r;
        }

        let count_a = gc + offset_a as usize;
        let total_l_a = self.p_l[count_a.min(self.p_l.len()-1)] as f64 * 0.391
                      + self.p_l[(count_a + width_a).min(self.p_l.len()-1)] as f64
                      + self.p_l[(count_a + width_a*2).min(self.p_l.len()-1)] as f64 * 0.391;
        let total_r_a = self.p_r[count_a.min(self.p_r.len()-1)] as f64 * 0.391
                      + self.p_r[(count_a + width_a).min(self.p_r.len()-1)] as f64
                      + self.p_r[(count_a + width_a*2).min(self.p_r.len()-1)] as f64 * 0.391;

        let count_b = gc + offset_b as usize;
        let total_l_b = self.p_l[count_b.min(self.p_l.len()-1)] as f64 * 0.918
                      + self.p_l[(count_b + width_b).min(self.p_l.len()-1)] as f64
                      + self.p_l[(count_b + width_b*2).min(self.p_l.len()-1)] as f64 * 0.918;
        let total_r_b = self.p_r[count_b.min(self.p_r.len()-1)] as f64 * 0.918
                      + self.p_r[(count_b + width_b).min(self.p_r.len()-1)] as f64
                      + self.p_r[(count_b + width_b*2).min(self.p_r.len()-1)] as f64 * 0.918;

        let mut sl = in_l as f64 + total_l_a * 0.274 - total_l_b * 0.629;
        let mut sr = in_r as f64 + total_r_a * 0.274 - total_r_b * 0.629;
        sl /= 4.0; sr /= 4.0;

        self.gcount -= 1;
        (sl as f32, sr as f32)
    }
}

// ─── Mode 32: Slew2 ─────────────────────────────────────────────────────────

pub struct AirwindowsSlew2 {
    // L oversampling
    lata_last1: f64, lata_last2: f64, lata_last3: f64,
    lata_a: f64, lata_b: f64, lata_c: f64,
    lata_flip: bool,
    lata_prev_diff: f64,
    last_l: f64,
    // R oversampling
    rata_last1: f64, rata_last2: f64, rata_last3: f64,
    rata_a: f64, rata_b: f64, rata_c: f64,
    rata_flip: bool,
    rata_prev_diff: f64,
    last_r: f64,
}
impl AirwindowsSlew2 {
    const DECAY: f64 = 0.618;
    const TWEAK: f64 = 0.0105;
    pub fn new() -> Self {
        Self {
            lata_last1: 0.0, lata_last2: 0.0, lata_last3: 0.0,
            lata_a: 0.0, lata_b: 0.0, lata_c: 0.0,
            lata_flip: false, lata_prev_diff: 0.0, last_l: 0.0,
            rata_last1: 0.0, rata_last2: 0.0, rata_last3: 0.0,
            rata_a: 0.0, rata_b: 0.0, rata_c: 0.0,
            rata_flip: false, rata_prev_diff: 0.0, last_r: 0.0,
        }
    }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32, sr: f32) -> (f32, f32) {
        let overallscale = 2.0 * sr as f64 / 44100.0;
        let threshold = (1.0 - drive as f64).powi(4) / overallscale;

        let sl = in_l as f64;
        let sr2 = in_r as f64;
        let dry_l = sl;
        let dry_r = sr2;

        // L: first half (halfway sample)
        let lata_half_dry = (sl + self.lata_last1 + ((-self.lata_last2 + self.lata_last3) * Self::TWEAK)) / 2.0;
        let mut lata_halfway = lata_half_dry;
        self.lata_last3 = self.lata_last2;
        self.lata_last2 = self.lata_last1;
        self.lata_last1 = sl;

        let clamp = lata_halfway - lata_half_dry;
        if clamp > threshold { lata_halfway = self.last_l + threshold; }
        if -clamp > threshold { lata_halfway = self.last_l - threshold; }
        self.last_l = lata_halfway;

        self.lata_c = lata_halfway - lata_half_dry;
        if self.lata_flip {
            self.lata_a *= Self::DECAY; self.lata_b *= Self::DECAY;
            self.lata_a += self.lata_c; self.lata_b -= self.lata_c;
            self.lata_c = self.lata_a;
        } else {
            self.lata_b *= Self::DECAY; self.lata_a *= Self::DECAY;
            self.lata_b += self.lata_c; self.lata_a -= self.lata_c;
            self.lata_c = self.lata_b;
        }
        let lata_half_diff = self.lata_c * Self::DECAY;
        self.lata_flip = !self.lata_flip;

        // L second half
        let mut sl_out = sl;
        let clamp2 = sl_out - self.last_l;
        if clamp2 > threshold { sl_out = self.last_l + threshold; }
        if -clamp2 > threshold { sl_out = self.last_l - threshold; }
        self.last_l = sl_out;

        self.lata_c = sl_out - dry_l;
        if self.lata_flip {
            self.lata_a *= Self::DECAY; self.lata_b *= Self::DECAY;
            self.lata_a += self.lata_c; self.lata_b -= self.lata_c;
            self.lata_c = self.lata_a;
        } else {
            self.lata_b *= Self::DECAY; self.lata_a *= Self::DECAY;
            self.lata_b += self.lata_c; self.lata_a -= self.lata_c;
            self.lata_c = self.lata_b;
        }
        let lata_diff = self.lata_c * Self::DECAY;
        self.lata_flip = !self.lata_flip;
        let out_l = dry_l + (lata_diff + lata_half_diff + self.lata_prev_diff) / 0.734;
        self.lata_prev_diff = lata_diff / 2.0;

        // R: same
        let rata_half_dry = (sr2 + self.rata_last1 + ((-self.rata_last2 + self.rata_last3) * Self::TWEAK)) / 2.0;
        let mut rata_halfway = rata_half_dry;
        self.rata_last3 = self.rata_last2;
        self.rata_last2 = self.rata_last1;
        self.rata_last1 = sr2;

        let clamp = rata_halfway - rata_half_dry;
        if clamp > threshold { rata_halfway = self.last_r + threshold; }
        if -clamp > threshold { rata_halfway = self.last_r - threshold; }
        self.last_r = rata_halfway;

        self.rata_c = rata_halfway - rata_half_dry;
        if self.rata_flip {
            self.rata_a *= Self::DECAY; self.rata_b *= Self::DECAY;
            self.rata_a += self.rata_c; self.rata_b -= self.rata_c;
            self.rata_c = self.rata_a;
        } else {
            self.rata_b *= Self::DECAY; self.rata_a *= Self::DECAY;
            self.rata_b += self.rata_c; self.rata_a -= self.rata_c;
            self.rata_c = self.rata_b;
        }
        let rata_half_diff = self.rata_c * Self::DECAY;
        self.rata_flip = !self.rata_flip;

        let mut sr_out = sr2;
        let clamp2 = sr_out - self.last_r;
        if clamp2 > threshold { sr_out = self.last_r + threshold; }
        if -clamp2 > threshold { sr_out = self.last_r - threshold; }
        self.last_r = sr_out;

        self.rata_c = sr_out - dry_r;
        if self.rata_flip {
            self.rata_a *= Self::DECAY; self.rata_b *= Self::DECAY;
            self.rata_a += self.rata_c; self.rata_b -= self.rata_c;
            self.rata_c = self.rata_a;
        } else {
            self.rata_b *= Self::DECAY; self.rata_a *= Self::DECAY;
            self.rata_b += self.rata_c; self.rata_a -= self.rata_c;
            self.rata_c = self.rata_b;
        }
        let rata_diff = self.rata_c * Self::DECAY;
        self.rata_flip = !self.rata_flip;
        let out_r = dry_r + (rata_diff + rata_half_diff + self.rata_prev_diff) / 0.734;
        self.rata_prev_diff = rata_diff / 2.0;

        (out_l as f32, out_r as f32)
    }
}


// ─── Master Airwindows dispatcher ───────────────────────────────────────────

pub struct Airwindows {
    sample_rate: f32,
    // modes 0-5 (original)
    pub tape2: AirwindowsTape2,
    pub density: AirwindowsDensity,
    pub console: AirwindowsConsole,
    pub to_vinyl4: AirwindowsToVinyl4,
    pub atmosphere: AirwindowsAtmosphere,
    pub pressure5: AirwindowsPressure5,
    // modes 6-32 (new)
    pub drive: AirwindowsDrive,
    pub hard_vacuum: AirwindowsHardVacuum,
    pub spiral2: AirwindowsSpiral2,
    pub fracture: AirwindowsFracture,
    pub mojo: AirwindowsMojo,
    pub adclip7: AirwindowsADClip7,
    pub loud: AirwindowsLoud,
    pub iron_oxide5: AirwindowsIronOxide5,
    pub to_tape6: AirwindowsToTape6,
    pub chrome_oxide: AirwindowsChromeOxide,
    pub pressure4: AirwindowsPressure4,
    pub butter_comp2: AirwindowsButterComp2,
    pub vari_mu: AirwindowsVariMu,
    pub power_sag: AirwindowsPowerSag,
    pub galactic: AirwindowsGalactic,
    pub verbity: AirwindowsVerbity,
    pub capacitor: AirwindowsCapacitor,
    pub focus: AirwindowsFocus,
    pub y_lowpass: AirwindowsYLowpass,
    pub dub_sub: AirwindowsDubSub,
    pub melt: AirwindowsMelt,
    pub pop: AirwindowsPop,
    pub bit_glitter: AirwindowsBitGlitter,
    pub de_rez2: AirwindowsDeRez2,
    pub buss_colors4: AirwindowsBussColors4,
    pub hombre: AirwindowsHombre,
    pub slew2: AirwindowsSlew2,
}

impl Airwindows {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            tape2: AirwindowsTape2::new(sample_rate),
            density: AirwindowsDensity::new(),
            console: AirwindowsConsole::new(),
            to_vinyl4: AirwindowsToVinyl4::new(sample_rate),
            atmosphere: AirwindowsAtmosphere::new(),
            pressure5: AirwindowsPressure5::new(sample_rate),
            drive: AirwindowsDrive::new(),
            hard_vacuum: AirwindowsHardVacuum::new(),
            spiral2: AirwindowsSpiral2::new(),
            fracture: AirwindowsFracture::new(),
            mojo: AirwindowsMojo::new(),
            adclip7: AirwindowsADClip7::new(),
            loud: AirwindowsLoud::new(),
            iron_oxide5: AirwindowsIronOxide5::new(),
            to_tape6: AirwindowsToTape6::new(),
            chrome_oxide: AirwindowsChromeOxide::new(),
            pressure4: AirwindowsPressure4::new(),
            butter_comp2: AirwindowsButterComp2::new(),
            vari_mu: AirwindowsVariMu::new(),
            power_sag: AirwindowsPowerSag::new(),
            galactic: AirwindowsGalactic::new(),
            verbity: AirwindowsVerbity::new(),
            capacitor: AirwindowsCapacitor::new(),
            focus: AirwindowsFocus::new(),
            y_lowpass: AirwindowsYLowpass::new(),
            dub_sub: AirwindowsDubSub::new(),
            melt: AirwindowsMelt::new(),
            pop: AirwindowsPop::new(),
            bit_glitter: AirwindowsBitGlitter::new(),
            de_rez2: AirwindowsDeRez2::new(),
            buss_colors4: AirwindowsBussColors4::new(),
            hombre: AirwindowsHombre::new(),
            slew2: AirwindowsSlew2::new(),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.tape2.set_sample_rate(sr);
        self.to_vinyl4.set_sample_rate(sr);
        self.pressure5.set_sample_rate(sr);
    }

    /// mode: 0=Tape2, 1=Density, 2=Console, 3=ToVinyl4, 4=Atmosphere, 5=Pressure5,
    /// 6=Drive, 7=HardVacuum, 8=Spiral2, 9=Fracture, 10=Mojo, 11=ADClip7,
    /// 12=Loud, 13=IronOxide5, 14=ToTape6, 15=ChromeOxide, 16=Pressure4,
    /// 17=ButterComp2, 18=VariMu, 19=PowerSag, 20=Galactic, 21=Verbity,
    /// 22=Capacitor, 23=Focus, 24=YLowpass, 25=DubSub, 26=Melt, 27=Pop,
    /// 28=BitGlitter, 29=DeRez2, 30=BussColors4, 31=Hombre, 32=Slew2
    pub fn tick(&mut self, in_l: f32, in_r: f32, mode: u32, drive: f32, mix: f32) -> (f32, f32) {
        let sr = self.sample_rate;
        let (wet_l, wet_r) = match mode {
            0  => self.tape2.tick(in_l, in_r, drive),
            1  => self.density.tick(in_l, in_r, drive),
            2  => self.console.tick(in_l, in_r, drive),
            3  => self.to_vinyl4.tick(in_l, in_r, drive),
            4  => self.atmosphere.tick(in_l, in_r, drive),
            5  => self.pressure5.tick(in_l, in_r, drive),
            6  => self.drive.tick(in_l, in_r, drive, sr),
            7  => self.hard_vacuum.tick(in_l, in_r, drive),
            8  => self.spiral2.tick(in_l, in_r, drive, sr),
            9  => self.fracture.tick(in_l, in_r, drive),
            10 => self.mojo.tick(in_l, in_r, drive),
            11 => self.adclip7.tick(in_l, in_r, drive, sr),
            12 => self.loud.tick(in_l, in_r, drive, sr),
            13 => self.iron_oxide5.tick(in_l, in_r, drive, sr),
            14 => self.to_tape6.tick(in_l, in_r, drive, sr),
            15 => self.chrome_oxide.tick(in_l, in_r, drive, sr),
            16 => self.pressure4.tick(in_l, in_r, drive, sr),
            17 => self.butter_comp2.tick(in_l, in_r, drive, sr),
            18 => self.vari_mu.tick(in_l, in_r, drive, sr),
            19 => self.power_sag.tick(in_l, in_r, drive),
            20 => self.galactic.tick(in_l, in_r, drive, sr),
            21 => self.verbity.tick(in_l, in_r, drive, sr),
            22 => self.capacitor.tick(in_l, in_r, drive),
            23 => self.focus.tick(in_l, in_r, drive, sr),
            24 => self.y_lowpass.tick(in_l, in_r, drive, sr),
            25 => self.dub_sub.tick(in_l, in_r, drive, sr),
            26 => self.melt.tick(in_l, in_r, drive),
            27 => self.pop.tick(in_l, in_r, drive, sr),
            28 => self.bit_glitter.tick(in_l, in_r, drive, sr),
            29 => self.de_rez2.tick(in_l, in_r, drive, sr),
            30 => self.buss_colors4.tick(in_l, in_r, drive),
            31 => self.hombre.tick(in_l, in_r, drive, sr),
            32 => self.slew2.tick(in_l, in_r, drive, sr),
            _  => (in_l, in_r),
        };
        (in_l + (wet_l - in_l) * mix, in_r + (wet_r - in_r) * mix)
    }
}
