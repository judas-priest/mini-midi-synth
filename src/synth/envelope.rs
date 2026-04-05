/// Analog-style ADSR envelope generator using one-pole IIR filters with
/// overshoot targets, modeled after RC charging circuits in analog synths.
/// Supports configurable attack/decay shapes (Surge XT analog mode).

#[derive(Clone, Copy, PartialEq)]
enum EnvStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Envelope shape: controls curvature of attack/decay/release.
/// 0=sqrt (fast start), 1=linear, 2=quadratic (slow start)
#[derive(Clone, Copy, PartialEq)]
pub enum EnvShape {
    Sqrt,      // 0 — concave (fast onset)
    Linear,    // 1 — straight line
    Quadratic, // 2 — convex (slow onset)
}

impl EnvShape {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            1 => Self::Linear,
            2 => Self::Quadratic,
            _ => Self::Sqrt,
        }
    }

    /// Apply shape to a 0..1 fraction.
    #[inline(always)]
    #[allow(dead_code)]
    fn apply(self, t: f32) -> f32 {
        match self {
            Self::Sqrt => t.sqrt(),
            Self::Linear => t,
            Self::Quadratic => t * t,
        }
    }
}

#[derive(Clone)]
pub struct Envelope {
    stage: EnvStage,
    output: f32,
    attack_coeff: f32,
    decay_coeff: f32,
    release_coeff: f32,
    attack_target: f32,
    decay_target: f32,
    release_target: f32,
    sustain: f32,
    sample_rate: f32,
    // Shape selection
    attack_shape: EnvShape,
    decay_shape: EnvShape,
}

/// Compute one-pole coefficient from time in seconds and sample rate.
/// For very short times, clamp to avoid numerical blow-up.
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
            attack_target: 0.0,
            decay_target: 0.0,
            release_target: 0.0,
            sustain: 0.7,
            sample_rate,
            attack_shape: EnvShape::Sqrt,
            decay_shape: EnvShape::Sqrt,
        };
        env.set_adsr(0.01, 0.1, 0.7, 0.3);
        env
    }

    pub fn set_adsr(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.sustain = sustain.clamp(0.0, 1.0);

        // Pre-compute coefficients
        self.attack_coeff = time_to_coeff(attack, self.sample_rate);
        self.decay_coeff = time_to_coeff(decay, self.sample_rate);
        self.release_coeff = time_to_coeff(release, self.sample_rate);

        // Overshoot targets for analog-style exponential curves
        self.attack_target = 1.0 + 0.3; // Charge past 1.0 for concave-up attack
        self.decay_target = (self.sustain - 0.001).max(-0.001); // Never undershoot below -0.001
        self.release_target = -0.01; // Slight undershoot below 0
    }

    /// Set attack shape (0=sqrt, 1=linear, 2=quadratic).
    pub fn set_attack_shape(&mut self, shape: f32) {
        self.attack_shape = EnvShape::from_param(shape);
    }

    /// Set decay/release shape (0=sqrt, 1=linear, 2=quadratic).
    pub fn set_decay_shape(&mut self, shape: f32) {
        self.decay_shape = EnvShape::from_param(shape);
    }

    pub fn note_on(&mut self) {
        // Start attack from current output value — no jump to 0, avoids clicks
        self.stage = EnvStage::Attack;
    }

    pub fn note_off(&mut self) {
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
                // One-pole filter charging toward overshoot target above 1.0
                self.output = self.attack_coeff * self.output
                    + (1.0 - self.attack_coeff) * self.attack_target;

                // Apply shape: transform the raw exponential curve
                let shaped = match self.attack_shape {
                    EnvShape::Sqrt => self.output,   // Default — already concave from overshoot
                    EnvShape::Linear => {
                        // Linearize by reducing overshoot effect
                        let raw = self.attack_coeff * self.output
                            + (1.0 - self.attack_coeff) * 1.05;
                        self.output = raw;
                        raw
                    }
                    EnvShape::Quadratic => {
                        // Convex — square the output for slower start
                        self.output * self.output
                    }
                };

                if self.output >= 1.0 {
                    self.output = 1.0;
                    self.stage = EnvStage::Decay;
                }
                shaped.min(1.0)
            }
            EnvStage::Decay => {
                // One-pole filter discharging toward target below sustain
                self.output = self.decay_coeff * self.output
                    + (1.0 - self.decay_coeff) * self.decay_target;

                let out = match self.decay_shape {
                    EnvShape::Sqrt => self.output,
                    EnvShape::Linear => self.output,  // Already close to linear with undershoot
                    EnvShape::Quadratic => {
                        // Slower initial decay, faster at end
                        let norm = if self.sustain < 0.99 {
                            ((self.output - self.sustain) / (1.0 - self.sustain)).clamp(0.0, 1.0)
                        } else { 0.0 };
                        self.sustain + (1.0 - self.sustain) * norm * norm
                    }
                };

                if self.output <= self.sustain + 0.001 {
                    self.output = self.sustain;
                    if self.sustain < 0.0001 {
                        self.stage = EnvStage::Idle; // sustain=0: note is dead
                    } else {
                        self.stage = EnvStage::Sustain;
                    }
                }
                out.max(0.0) // never output negative amplitude
            }
            EnvStage::Sustain => {
                self.output = self.sustain;
                self.sustain
            }
            EnvStage::Release => {
                // One-pole filter discharging toward target below 0
                self.output = self.release_coeff * self.output
                    + (1.0 - self.release_coeff) * self.release_target;

                let out = match self.decay_shape {
                    EnvShape::Quadratic => {
                        // Slower release curve
                        let norm = (self.output / self.sustain.max(0.01)).clamp(0.0, 1.0);
                        self.sustain.max(0.01) * norm * norm
                    }
                    _ => self.output,
                };

                if self.output <= 0.001 {
                    self.output = 0.0;
                    self.stage = EnvStage::Idle;
                }
                out
            }
        }
    }
}
