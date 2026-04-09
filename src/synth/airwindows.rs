//! Airwindows-inspired DSP algorithms: Tape2, Density, Console, ToVinyl4, Atmosphere, Pressure5.

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
        // Two-stage tanh saturation with LP smoothing (tape freq rolloff)
        let gain = drive * 3.0 + 1.0;
        let s1_l = (in_l * gain).tanh();
        let s1_r = (in_r * gain).tanh();
        let s2_l = (s1_l * gain * 0.5).tanh() / gain.sqrt();
        let s2_r = (s1_r * gain * 0.5).tanh() / gain.sqrt();
        // Slight high-freq rolloff (tape bias)
        let lp = 0.85;
        self.lp_l = self.lp_l * lp + s2_l * (1.0 - lp) + 1e-30;
        self.lp_r = self.lp_r * lp + s2_r * (1.0 - lp) + 1e-30;
        let _ = (self.stage_l, self.stage_r, self.wow_phase, self.sample_rate);
        (self.lp_l, self.lp_r)
    }
}

pub struct AirwindowsDensity;

impl AirwindowsDensity {
    pub fn new() -> Self { Self }
    pub fn tick(&self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        // Density: blend between linear and tanh-saturated signal
        // drive=0: pure linear, drive=1: full tanh
        let d = drive.clamp(0.0, 1.0);
        let gain = d * 4.0 + 1.0;
        let out_l = (1.0 - d) * in_l + d * (in_l * gain).tanh() / gain;
        let out_r = (1.0 - d) * in_r + d * (in_r * gain).tanh() / gain;
        (out_l, out_r)
    }
}

pub struct AirwindowsConsole;

impl AirwindowsConsole {
    pub fn new() -> Self { Self }
    pub fn tick(&self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        // Summing saturation (Neve-style): x / (1 + |x| * k)
        let k = drive * 2.0 + 0.5;
        let out_l = in_l / (1.0 + in_l.abs() * k);
        let out_r = in_r / (1.0 + in_r.abs() * k);
        (out_l, out_r)
    }
}

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
        // Bass enhancement + HF rolloff + subtle vinyl warmth
        // SR-independent: use exp(-2*PI*fc/sr) for filter coefficients
        let sr = self.sample_rate;
        let bass_coeff = (-2.0 * std::f32::consts::PI * 35.0 / sr).exp();
        let hf_base = 5000.0 + drive * 2000.0; // 5kHz..7kHz
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

pub struct AirwindowsAtmosphere {
    hs_l: f32, hs_r: f32,
}

impl AirwindowsAtmosphere {
    pub fn new() -> Self { Self { hs_l: 0.0, hs_r: 0.0 } }
    pub fn tick(&mut self, in_l: f32, in_r: f32, drive: f32) -> (f32, f32) {
        // Air: high-shelf presence boost + gentle stereo widening
        let coeff = 0.55 - drive * 0.15; // higher drive = more HF
        self.hs_l = self.hs_l * coeff + in_l * (1.0 - coeff) + 1e-30;
        self.hs_r = self.hs_r * coeff + in_r * (1.0 - coeff) + 1e-30;
        let air_l = in_l - self.hs_l;  // high shelf (subtract LP)
        let air_r = in_r - self.hs_r;
        let out_l = in_l + air_l * drive * 0.5 + air_r * drive * 0.1;
        let out_r = in_r + air_r * drive * 0.5 + air_l * drive * 0.1;
        (out_l, out_r)
    }
}

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
        // Soft compressor with character saturation
        // SR-independent: preserve 44.1kHz behavior at any sample rate
        // att_time ~0.45ms, rel_time ~2.27ms at 44100 Hz
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

/// Unified Airwindows effect dispatcher.
pub struct Airwindows {
    pub tape2: AirwindowsTape2,
    pub density: AirwindowsDensity,
    pub console: AirwindowsConsole,
    pub to_vinyl4: AirwindowsToVinyl4,
    pub atmosphere: AirwindowsAtmosphere,
    pub pressure5: AirwindowsPressure5,
}

impl Airwindows {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            tape2: AirwindowsTape2::new(sample_rate),
            density: AirwindowsDensity::new(),
            console: AirwindowsConsole::new(),
            to_vinyl4: AirwindowsToVinyl4::new(sample_rate),
            atmosphere: AirwindowsAtmosphere::new(),
            pressure5: AirwindowsPressure5::new(sample_rate),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.tape2.set_sample_rate(sr);
        self.to_vinyl4.set_sample_rate(sr);
        self.pressure5.set_sample_rate(sr);
    }

    /// mode: 0=Tape2, 1=Density, 2=Console, 3=ToVinyl4, 4=Atmosphere, 5=Pressure5
    pub fn tick(&mut self, in_l: f32, in_r: f32, mode: u32, drive: f32, mix: f32) -> (f32, f32) {
        let (wet_l, wet_r) = match mode {
            0 => self.tape2.tick(in_l, in_r, drive),
            1 => self.density.tick(in_l, in_r, drive),
            2 => self.console.tick(in_l, in_r, drive),
            3 => self.to_vinyl4.tick(in_l, in_r, drive),
            4 => self.atmosphere.tick(in_l, in_r, drive),
            5 => self.pressure5.tick(in_l, in_r, drive),
            _ => (in_l, in_r),
        };
        (in_l + (wet_l - in_l) * mix, in_r + (wet_r - in_r) * mix)
    }
}
