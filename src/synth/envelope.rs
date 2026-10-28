//! Analog-style ADSR envelope generator.
//!
//! Sqrt and Exponential shapes use one-pole IIR (RC-circuit model) with overshoot targets.
//! Linear and Quadratic shapes use phase tracking for correct curve shapes.

#[derive(Clone, Copy, PartialEq)]
enum EnvStage {
    Idle,
    Attack,
    Hold,    // AHDSR: hold at peak before decay
    Decay,
    Sustain,
    Release,
}

/// Envelope shape: controls curvature of attack/decay/release.
/// 0=sqrt (fast start), 1=linear, 2=quadratic (slow start), 3=exponential (pure RC)
#[derive(Clone, Copy, PartialEq)]
pub enum EnvShape {
    Sqrt,        // 0 — concave (fast onset), IIR with overshoot
    Linear,      // 1 — straight line, phase-tracked
    Quadratic,   // 2 — convex (slow onset), phase-tracked
    Exponential, // 3 — true exponential: pure RC decay, no overshoot
}

impl EnvShape {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            1 => Self::Linear,
            2 => Self::Quadratic,
            3 => Self::Exponential,
            _ => Self::Sqrt,
        }
    }

    #[inline(always)]
    fn is_phase_based(self) -> bool {
        matches!(self, Self::Linear | Self::Quadratic)
    }
}

#[derive(Clone)]
pub struct Envelope {
    stage: EnvStage,
    output: f32,
    // IIR coefficients and targets (for Sqrt / Exponential)
    attack_coeff: f32,
    decay_coeff: f32,
    release_coeff: f32,
    // Phase tracking (for Linear / Quadratic)
    phase: f32,
    phase_step_a: f32,
    phase_step_d: f32,
    phase_step_r: f32,
    // Hold stage (AHDSR)
    hold_samples: u32,
    hold_counter: u32,
    // Remembered start levels for phase-based decay/release
    decay_start: f32,
    release_start: f32,
    sustain: f32,
    sample_rate: f32,
    // Shape selection
    attack_shape: EnvShape,
    decay_shape: EnvShape,
    release_shape: EnvShape,
}

/// Compute one-pole IIR coefficient from time in seconds.
fn time_to_coeff(time_secs: f32, sample_rate: f32) -> f32 {
    let t = time_secs.max(0.001);
    (-1.0_f32 / (t * sample_rate)).exp()
}

impl Envelope {
    pub fn new(sample_rate: f32) -> Self {
        let mut env = Self {
            stage: EnvStage::Idle,
            output: 0.0,
            attack_coeff: 0.0,
            decay_coeff: 0.0,
            release_coeff: 0.0,
            phase: 0.0,
            phase_step_a: 0.0,
            phase_step_d: 0.0,
            phase_step_r: 0.0,
            hold_samples: 0,
            hold_counter: 0,
            decay_start: 1.0,
            release_start: 0.0,
            sustain: 0.7,
            sample_rate,
            attack_shape: EnvShape::Sqrt,
            decay_shape: EnvShape::Sqrt,
            release_shape: EnvShape::Sqrt,
        };
        env.set_adsr(0.01, 0.1, 0.7, 0.3);
        env
    }

    pub fn set_adsr(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.sustain = sustain.clamp(0.0, 1.0);

        let a = attack.max(0.001);
        let d = decay.max(0.001);
        let r = release.max(0.001);

        // IIR coefficients (for Sqrt/Exponential)
        self.attack_coeff = time_to_coeff(a, self.sample_rate);
        self.decay_coeff = time_to_coeff(d, self.sample_rate);
        self.release_coeff = time_to_coeff(r, self.sample_rate);

        // Phase steps (for Linear/Quadratic) — phase advances 0→1 over the stage duration
        self.phase_step_a = 1.0 / (a * self.sample_rate);
        self.phase_step_d = 1.0 / (d * self.sample_rate);
        self.phase_step_r = 1.0 / (r * self.sample_rate);
    }

    /// Set hold time in seconds (0 = no hold, ADSR behaviour).
    pub fn set_hold(&mut self, hold_secs: f32) {
        self.hold_samples = (hold_secs.max(0.0) * self.sample_rate) as u32;
    }

    pub fn set_attack_shape(&mut self, shape: f32) {
        self.attack_shape = EnvShape::from_param(shape);
    }

    pub fn set_decay_shape(&mut self, shape: f32) {
        self.decay_shape = EnvShape::from_param(shape);
    }

    pub fn set_release_shape(&mut self, shape: f32) {
        self.release_shape = EnvShape::from_param(shape);
    }

    pub fn note_on(&mut self) {
        // For phase-based attack, initialize phase so the curve starts from current output
        // (avoids click when retriggering during decay/release).
        self.phase = if self.attack_shape.is_phase_based() {
            let cur = self.output.clamp(0.0, 1.0);
            match self.attack_shape {
                // output = phase  →  phase = output
                EnvShape::Linear => cur,
                // output = phase²  →  phase = sqrt(output)
                EnvShape::Quadratic => cur.sqrt(),
                _ => 0.0,
            }
        } else {
            0.0
        };
        self.stage = EnvStage::Attack;
    }

    pub fn note_off(&mut self) {
        self.release_start = self.output;
        self.phase = 0.0;
        self.stage = EnvStage::Release;
    }

    pub fn is_idle(&self) -> bool {
        self.stage == EnvStage::Idle
    }

    pub fn is_releasing(&self) -> bool {
        self.stage == EnvStage::Release
    }

    pub fn tick(&mut self) -> f32 {
        match self.stage {
            EnvStage::Idle => 0.0,

            EnvStage::Attack => {
                let shaped = if self.attack_shape.is_phase_based() {
                    self.phase = (self.phase + self.phase_step_a).min(1.0);
                    let out = match self.attack_shape {
                        EnvShape::Linear    => self.phase,
                        EnvShape::Quadratic => self.phase * self.phase,
                        _ => unreachable!(),
                    };
                    self.output = out;
                    if self.phase >= 1.0 {
                        self.output = 1.0;
                        self.decay_start = 1.0;
                        self.phase = 0.0;
                        self.stage = if self.hold_samples > 0 { EnvStage::Hold } else { EnvStage::Decay };
                        self.hold_counter = 0;
                    }
                    out
                } else {
                    // IIR with overshoot
                    let target = if self.attack_shape == EnvShape::Sqrt { 1.3 } else { 1.0 };
                    self.output = self.attack_coeff * self.output
                        + (1.0 - self.attack_coeff) * target;
                    if self.output >= 1.0 {
                        self.output = 1.0;
                        self.decay_start = 1.0;
                        self.phase = 0.0;
                        self.stage = if self.hold_samples > 0 { EnvStage::Hold } else { EnvStage::Decay };
                        self.hold_counter = 0;
                    }
                    self.output
                };
                shaped.min(1.0)
            }

            EnvStage::Hold => {
                self.hold_counter += 1;
                if self.hold_counter >= self.hold_samples {
                    self.stage = EnvStage::Decay;
                    self.phase = 0.0;
                    self.decay_start = 1.0;
                }
                1.0
            }

            EnvStage::Decay => {
                let out = if self.decay_shape.is_phase_based() {
                    self.phase = (self.phase + self.phase_step_d).min(1.0);
                    // norm goes 1→0 as phase goes 0→1
                    let norm = 1.0 - self.phase;
                    let v = match self.decay_shape {
                        EnvShape::Linear    => self.sustain + (self.decay_start - self.sustain) * norm,
                        EnvShape::Quadratic => self.sustain + (self.decay_start - self.sustain) * norm * norm,
                        _ => unreachable!(),
                    };
                    self.output = v;
                    if self.phase >= 1.0 {
                        self.output = self.sustain;
                        if self.sustain < 0.0001 {
                            self.stage = EnvStage::Idle;
                        } else {
                            self.stage = EnvStage::Sustain;
                        }
                    }
                    v
                } else {
                    // IIR decay
                    let target = if self.decay_shape == EnvShape::Exponential {
                        self.sustain
                    } else {
                        // Sqrt: undershoot slightly for analog feel
                        (self.sustain - 0.001).max(-0.001)
                    };
                    self.output = self.decay_coeff * self.output
                        + (1.0 - self.decay_coeff) * target;
                    if self.output <= self.sustain + 0.001 {
                        self.output = self.sustain;
                        if self.sustain < 0.0001 {
                            self.stage = EnvStage::Idle;
                        } else {
                            self.stage = EnvStage::Sustain;
                        }
                    }
                    self.output
                };
                out.max(0.0)
            }

            EnvStage::Sustain => {
                self.output = self.sustain;
                self.sustain
            }

            EnvStage::Release => {
                let out = if self.release_shape.is_phase_based() {
                    self.phase = (self.phase + self.phase_step_r).min(1.0);
                    let norm = 1.0 - self.phase;
                    let start = self.release_start.max(0.0);
                    let v = match self.release_shape {
                        EnvShape::Linear    => start * norm,
                        EnvShape::Quadratic => start * norm * norm,
                        _ => unreachable!(),
                    };
                    self.output = v;
                    if self.phase >= 1.0 {
                        self.output = 0.0;
                        self.stage = EnvStage::Idle;
                    }
                    v
                } else {
                    // IIR release
                    let target = if self.release_shape == EnvShape::Exponential {
                        0.0
                    } else {
                        -0.01 // Sqrt: slight undershoot for natural feel
                    };
                    self.output = self.release_coeff * self.output
                        + (1.0 - self.release_coeff) * target;
                    if self.output <= 0.001 {
                        self.output = 0.0;
                        self.stage = EnvStage::Idle;
                    }
                    self.output
                };
                out.max(0.0)
            }
        }
    }
}
