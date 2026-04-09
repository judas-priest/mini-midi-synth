/// LFO oscillator with multiple waveforms, step sequencer mode, tempo sync,
/// unipolar output, and deform parameter.

use std::sync::OnceLock;

static SINE_TABLE: OnceLock<Box<[f32; 2048]>> = OnceLock::new();

fn sine_lut(phase: f32) -> f32 {
    let table = SINE_TABLE.get_or_init(|| {
        let mut t = Box::new([0.0f32; 2048]);
        for i in 0..2048 {
            t[i] = (i as f32 / 2048.0 * std::f32::consts::TAU).sin();
        }
        t
    });
    let idx = (phase * 2048.0) as usize & 2047;
    table[idx]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LfoWaveform {
    Sine,
    Triangle,
    Square,
    SampleHold,
    Sawtooth,
    Envelope,
    Noise,
    SmoothNoise,
    StepSeq,
}

impl LfoWaveform {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            1 => Self::Triangle,
            2 => Self::Square,
            3 => Self::SampleHold,
            4 => Self::Sawtooth,
            5 => Self::Envelope,
            6 => Self::Noise,
            7 => Self::SmoothNoise,
            8 => Self::StepSeq,
            _ => Self::Sine,
        }
    }
}

/// Tempo-sync rate divisions. Index maps to musical subdivision.
pub const TEMPO_SYNC_NAMES: &[&str] = &[
    "4 bars", "2 bars", "1 bar", "1/2", "1/4", "1/8", "1/16", "1/32",
    "1/2T", "1/4T", "1/8T", "1/16T", "1/2D", "1/4D", "1/8D", "1/16D",
];

/// Convert tempo sync index to cycles per beat (quarter note).
fn tempo_sync_cycles_per_beat(index: u8) -> f32 {
    // cycles_per_beat = 1 / (note_duration_in_beats)
    match index {
        0  => 1.0 / 16.0,  // 4 bars = 16 beats per cycle
        1  => 1.0 / 8.0,   // 2 bars
        2  => 1.0 / 4.0,   // 1 bar
        3  => 1.0 / 2.0,   // 1/2 note = 2 beats
        4  => 1.0,          // 1/4 = 1 beat
        5  => 2.0,          // 1/8 = 0.5 beats
        6  => 4.0,          // 1/16 = 0.25 beats
        7  => 8.0,          // 1/32 = 0.125 beats
        // Triplets: duration = base * 2/3
        8  => 3.0 / 4.0,   // 1/2T = 2 * 2/3 = 4/3 beats → 3/4 cpb
        9  => 3.0 / 2.0,   // 1/4T = 1 * 2/3 = 2/3 beat → 3/2 cpb
        10 => 3.0,          // 1/8T = 0.5 * 2/3 = 1/3 beat → 3 cpb
        11 => 6.0,          // 1/16T = 0.25 * 2/3 = 1/6 beat → 6 cpb
        // Dotted: duration = base * 3/2
        12 => 1.0 / 3.0,   // 1/2D = 2 * 3/2 = 3 beats → 1/3 cpb
        13 => 2.0 / 3.0,   // 1/4D = 1 * 3/2 = 1.5 beats → 2/3 cpb
        14 => 4.0 / 3.0,   // 1/8D = 0.5 * 3/2 = 0.75 beats → 4/3 cpb
        15 => 8.0 / 3.0,   // 1/16D = 0.25 * 3/2 = 0.375 beats → 8/3 cpb
        _  => 4.0,          // default 1/16
    }
}

/// Event returned when step sequencer crosses a step boundary.
#[derive(Clone, Copy, Default)]
pub struct StepSeqEvent {
    pub stepped: bool,
    pub retrigger_aeg: bool,
    pub retrigger_feg: bool,
}

pub const STEP_SEQ_LEN: usize = 16;

#[derive(Clone)]
pub struct Lfo {
    phase: f32,
    sample_rate: f32,
    sh_value: f32,
    sh_smooth: f32,
    noise_state: u32,
    prev_phase_int: u32,
    // Envelope mode state
    env_stage: u8,       // 0=idle, 1=attack, 2=decay, 3=sustain, 4=release
    env_value: f32,
    #[allow(dead_code)]
    env_triggered: bool,
    // Step sequencer data
    pub step_values: [f32; STEP_SEQ_LEN],
    pub step_count: u8,
    pub loop_start: u8,
    pub loop_end: u8,
    pub trigmask_aeg: u16,
    pub trigmask_feg: u16,
    step_current: u8,
    step_phase: f32,
    step_prev_value: f32,
    // Tempo sync
    pub tempo_sync: bool,
    pub bpm: f32,
    // Unipolar
    pub unipolar: bool,
    // Cached last step event
    last_step_event: StepSeqEvent,
}

impl Lfo {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            sh_value: 0.0,
            sh_smooth: 0.0,
            noise_state: 0xCAFEBABE,
            prev_phase_int: 0,
            env_stage: 0,
            env_value: 0.0,
            env_triggered: false,
            step_values: [0.0; STEP_SEQ_LEN],
            step_count: 16,
            loop_start: 0,
            loop_end: 15,
            trigmask_aeg: 0,
            trigmask_feg: 0,
            step_current: 0,
            step_phase: 0.0,
            step_prev_value: 0.0,
            tempo_sync: false,
            bpm: 120.0,
            unipolar: false,
            last_step_event: StepSeqEvent::default(),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
    }

    pub fn set_bpm(&mut self, bpm: f32) {
        self.bpm = bpm.max(20.0);
    }

    /// Reset LFO phase to 0 (retrigger on note-on). Does not affect noise state.
    pub fn reset_phase(&mut self) {
        self.phase = 0.0;
        self.step_current = 0;
        self.step_phase = 0.0;
    }

    /// Trigger envelope mode (one-shot from LFO).
    #[allow(dead_code)]
    pub fn trigger_envelope(&mut self) {
        self.env_stage = 1; // attack
        self.env_triggered = true;
    }

    /// Release envelope mode.
    #[allow(dead_code)]
    pub fn release_envelope(&mut self) {
        if self.env_stage > 0 && self.env_stage < 4 {
            self.env_stage = 4; // release
        }
    }

    /// Get the last step sequencer event (valid after tick_with_deform).
    pub fn last_step_event(&self) -> StepSeqEvent {
        self.last_step_event
    }

    /// Compute effective rate in Hz, handling tempo sync.
    fn effective_rate(&self, rate: f32) -> f32 {
        if self.tempo_sync {
            let cpb = tempo_sync_cycles_per_beat(rate as u8);
            self.bpm / 60.0 * cpb
        } else {
            rate
        }
    }

    /// Apply deform to a base waveform value.
    /// bend1: exponential shift, bend2: sine modulation
    #[inline(always)]
    fn apply_deform(x: f32, deform: f32) -> f32 {
        if deform.abs() < 0.01 { return x; }
        let a = deform;
        if a.abs() < 0.5 {
            // bend1: x = x - a*x^2 + a (exponential shift)
            (x - a * x * x + a).clamp(-1.0, 1.0)
        } else {
            // bend2: x += 4.5*a*sin(2π*x) / (2π) (sine modulation)
            let tau = std::f32::consts::TAU;
            (x + 4.5 * a * sine_lut(x.rem_euclid(1.0)) / tau).clamp(-1.0, 1.0)
        }
    }

    /// Step sequencer tick. Returns output value and advances step state.
    /// `deform`: 0 = staircase, >0 = smoothing/glide between steps.
    fn tick_step_seq(&mut self, rate: f32, deform: f32) -> f32 {
        let hz = self.effective_rate(rate);
        // In step seq mode, one full LFO cycle = one step
        let dt = hz / self.sample_rate;
        self.step_phase += dt;

        let mut event = StepSeqEvent::default();

        if self.step_phase >= 1.0 {
            self.step_phase -= 1.0;
            // Save previous value for smoothing
            self.step_prev_value = self.step_values[self.step_current as usize];
            // Advance step
            self.step_current += 1;
            if self.step_current > self.loop_end {
                self.step_current = self.loop_start;
            }
            // Check trigmask
            let bit = 1u16 << self.step_current;
            event.stepped = true;
            event.retrigger_aeg = (self.trigmask_aeg & bit) != 0;
            event.retrigger_feg = (self.trigmask_feg & bit) != 0;
        }

        self.last_step_event = event;

        let current_val = self.step_values[self.step_current as usize];

        // Smoothing: deform controls interpolation between prev and current step
        if deform.abs() > 0.01 {
            let t = self.step_phase.clamp(0.0, 1.0);
            let blend = t * deform.clamp(0.0, 1.0);
            self.step_prev_value + (current_val - self.step_prev_value) * blend
        } else {
            current_val
        }
    }

    /// Returns -1.0..+1.0
    #[allow(dead_code)]
    pub fn tick(&mut self, rate: f32, waveform: LfoWaveform) -> f32 {
        self.tick_with_deform(rate, waveform, 0.0)
    }

    /// Returns -1.0..+1.0 (or 0..1 if unipolar) with deform parameter.
    pub fn tick_with_deform(&mut self, rate: f32, waveform: LfoWaveform, deform: f32) -> f32 {
        // Clear step event for non-step-seq waveforms
        self.last_step_event = StepSeqEvent::default();

        if waveform == LfoWaveform::StepSeq {
            let val = self.tick_step_seq(rate, deform);
            return if self.unipolar { (val + 1.0) * 0.5 } else { val };
        }

        let hz = self.effective_rate(rate);
        let dt = hz / self.sample_rate;
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let raw = match waveform {
            LfoWaveform::Sine => sine_lut(self.phase),
            LfoWaveform::Triangle => {
                if self.phase < 0.5 {
                    4.0 * self.phase - 1.0
                } else {
                    3.0 - 4.0 * self.phase
                }
            }
            LfoWaveform::Square => {
                if self.phase < 0.5 { 1.0 } else { -1.0 }
            }
            LfoWaveform::SampleHold => {
                let phase_int = self.phase as u32;
                if phase_int != self.prev_phase_int || self.phase < dt * 1.5 {
                    self.prev_phase_int = phase_int;
                    // xorshift32
                    self.noise_state ^= self.noise_state << 13;
                    self.noise_state ^= self.noise_state >> 17;
                    self.noise_state ^= self.noise_state << 5;
                    self.sh_value =
                        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;

                    // Correlated noise when deform > 0: blend toward previous value
                    if deform > 0.01 {
                        let corr = deform.clamp(0.0, 0.95);
                        self.sh_value = self.sh_smooth * corr + self.sh_value * (1.0 - corr);
                    }
                }
                // Slew limiter: rate-dependent (~25% of LFO period for transition)
                let coeff = (hz * 4.0 / self.sample_rate).clamp(0.0001, 0.5);
                self.sh_smooth += coeff * (self.sh_value - self.sh_smooth);
                let val = self.sh_smooth;
                return if self.unipolar { (val + 1.0) * 0.5 } else { val };
            }
            LfoWaveform::Sawtooth => {
                2.0 * self.phase - 1.0
            }
            LfoWaveform::Noise => {
                // xorshift32 — new random value every sample
                self.noise_state ^= self.noise_state << 13;
                self.noise_state ^= self.noise_state >> 17;
                self.noise_state ^= self.noise_state << 5;
                (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
            }
            LfoWaveform::SmoothNoise => {
                self.noise_state ^= self.noise_state << 13;
                self.noise_state ^= self.noise_state >> 17;
                self.noise_state ^= self.noise_state << 5;
                let raw = (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                // deform 0=very smooth (coeff~0.999), 1=less smooth (coeff~0.9)
                let coeff = 0.999 - deform * 0.099;
                self.sh_smooth = self.sh_smooth * coeff + raw * (1.0 - coeff);
                let val = self.sh_smooth;
                return if self.unipolar { (val + 1.0) * 0.5 } else { val };
            }
            LfoWaveform::Envelope => {
                // One-shot ADSR driven by LFO rate
                let atk_time = 0.25;
                let dec_time = 0.25;
                let coeff = dt * 4.0;

                match self.env_stage {
                    1 => {
                        self.env_value += coeff / atk_time;
                        if self.env_value >= 1.0 {
                            self.env_value = 1.0;
                            self.env_stage = 2;
                        }
                    }
                    2 => {
                        self.env_value -= coeff * (self.env_value - 0.5) / dec_time;
                        if self.env_value <= 0.52 {
                            self.env_value = 0.5;
                            self.env_stage = 3;
                        }
                    }
                    3 => { self.env_value = 0.5; }
                    4 => {
                        self.env_value -= coeff;
                        if self.env_value <= 0.0 {
                            self.env_value = 0.0;
                            self.env_stage = 0;
                        }
                    }
                    _ => { self.env_value = 0.0; }
                }
                let val = self.env_value * 2.0 - 1.0;
                return if self.unipolar { (val + 1.0) * 0.5 } else { val };
            }
            LfoWaveform::StepSeq => unreachable!(), // handled above
        };

        let val = Self::apply_deform(raw, deform);
        if self.unipolar { (val + 1.0) * 0.5 } else { val }
    }

    /// Load step sequencer data from preset params.
    pub fn load_step_seq_from_params(&mut self, lfo_idx: u8, params: &std::collections::BTreeMap<String, f32>) {
        let prefix = format!("lfo{}", lfo_idx + 1);
        for i in 0..STEP_SEQ_LEN {
            self.step_values[i] = params.get(&format!("{prefix}_step_{i}"))
                .copied().unwrap_or(0.0);
        }
        self.step_count = params.get(&format!("{prefix}_step_count"))
            .copied().unwrap_or(16.0).clamp(1.0, 16.0) as u8;
        self.loop_start = params.get(&format!("{prefix}_loop_start"))
            .copied().unwrap_or(0.0).clamp(0.0, 15.0) as u8;
        self.loop_end = params.get(&format!("{prefix}_loop_end"))
            .copied().unwrap_or(15.0).clamp(0.0, 15.0) as u8;
        self.trigmask_aeg = params.get(&format!("{prefix}_trigmask_aeg"))
            .copied().unwrap_or(0.0) as u16;
        self.trigmask_feg = params.get(&format!("{prefix}_trigmask_feg"))
            .copied().unwrap_or(0.0) as u16;
    }
}
