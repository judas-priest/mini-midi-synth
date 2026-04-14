/// Per-part pitch step sequencer.
/// Synced to BPM, modulates pitch by semitone offsets, retriggers envelopes on each step.
/// Mirrors the drum StepSequencer's phase accumulator approach for drift-free timing.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

pub const MAX_STEPS: usize = 16;

/// Scale interval tables (semitones from root within one octave).
const SCALE_CHROMATIC: &[u8] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const SCALE_MAJOR: &[u8] = &[0, 2, 4, 5, 7, 9, 11];
const SCALE_MINOR: &[u8] = &[0, 2, 3, 5, 7, 8, 10];
const SCALE_PENTATONIC: &[u8] = &[0, 2, 4, 7, 9];
const SCALE_BLUES: &[u8] = &[0, 3, 5, 6, 7, 10];
const SCALE_DORIAN: &[u8] = &[0, 2, 3, 5, 7, 9, 10];
const SCALE_MIXOLYDIAN: &[u8] = &[0, 2, 4, 5, 7, 9, 10];

#[derive(Clone, Copy, PartialEq)]
pub enum StepRate {
    Quarter,      // 1 step per beat
    Eighth,       // 2 steps per beat
    Sixteenth,    // 4 steps per beat
    ThirtySecond, // 8 steps per beat
    EighthT,      // 3 steps per beat (triplet)
    SixteenthT,   // 6 steps per beat (triplet)
}

impl StepRate {
    pub fn from_index(i: u8) -> Self {
        match i {
            0 => Self::Quarter,
            1 => Self::Eighth,
            2 => Self::Sixteenth,
            3 => Self::ThirtySecond,
            4 => Self::EighthT,
            5 => Self::SixteenthT,
            _ => Self::Sixteenth,
        }
    }

    #[allow(dead_code)]
    pub fn index(self) -> u8 {
        match self {
            Self::Quarter => 0,
            Self::Eighth => 1,
            Self::Sixteenth => 2,
            Self::ThirtySecond => 3,
            Self::EighthT => 4,
            Self::SixteenthT => 5,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
            Self::ThirtySecond => "1/32",
            Self::EighthT => "1/8T",
            Self::SixteenthT => "1/16T",
        }
    }

    /// Steps per beat (quarter note).
    fn steps_per_beat(self) -> f64 {
        match self {
            Self::Quarter => 1.0,
            Self::Eighth => 2.0,
            Self::Sixteenth => 4.0,
            Self::ThirtySecond => 8.0,
            Self::EighthT => 3.0,
            Self::SixteenthT => 6.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ScaleType {
    Chromatic,
    Major,
    Minor,
    Pentatonic,
    Blues,
    Dorian,
    Mixolydian,
}

impl ScaleType {
    pub fn from_index(i: u8) -> Self {
        match i {
            0 => Self::Chromatic,
            1 => Self::Major,
            2 => Self::Minor,
            3 => Self::Pentatonic,
            4 => Self::Blues,
            5 => Self::Dorian,
            6 => Self::Mixolydian,
            _ => Self::Chromatic,
        }
    }

    #[allow(dead_code)]
    pub fn index(self) -> u8 {
        match self {
            Self::Chromatic => 0,
            Self::Major => 1,
            Self::Minor => 2,
            Self::Pentatonic => 3,
            Self::Blues => 4,
            Self::Dorian => 5,
            Self::Mixolydian => 6,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Chromatic => "Chromatic",
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::Pentatonic => "Pentatonic",
            Self::Blues => "Blues",
            Self::Dorian => "Dorian",
            Self::Mixolydian => "Mixolydian",
        }
    }

    fn intervals(self) -> &'static [u8] {
        match self {
            Self::Chromatic => SCALE_CHROMATIC,
            Self::Major => SCALE_MAJOR,
            Self::Minor => SCALE_MINOR,
            Self::Pentatonic => SCALE_PENTATONIC,
            Self::Blues => SCALE_BLUES,
            Self::Dorian => SCALE_DORIAN,
            Self::Mixolydian => SCALE_MIXOLYDIAN,
        }
    }
}

#[derive(Clone, Copy)]
pub struct PitchStep {
    pub pitch: i8,      // semitones offset, -24..+24
    pub gate: bool,     // note sounds on this step
    pub velocity: u8,   // 0-127 (0 = use held note velocity)
}

impl Default for PitchStep {
    fn default() -> Self {
        Self { pitch: 0, gate: true, velocity: 0 }
    }
}

/// Result of a step advance — tells the engine what to do.
#[allow(dead_code)]
pub struct StepEvent {
    pub stepped: bool,        // a new step just started
    pub gate: bool,           // current gate state
    pub pitch_offset: f32,    // semitones offset (after scale quantize)
    pub velocity: u8,         // step velocity (0 = passthrough)
    pub gate_changed: bool,   // gate transitioned (on→off or off→on)
}

pub struct PitchSequencer {
    pub steps: [PitchStep; MAX_STEPS],
    pub length: u8,
    pub enabled: bool,
    pub rate: StepRate,
    pub scale: ScaleType,
    pub swing: f32,           // 0.0..0.66

    // Runtime
    current_step: u8,
    phase_acc: f64,
    prev_gate: bool,
    step_atom: Arc<AtomicU8>,
}

const SEQ_PITCH_KEYS: [&str; 16] = [
    "seq_0_pitch","seq_1_pitch","seq_2_pitch","seq_3_pitch",
    "seq_4_pitch","seq_5_pitch","seq_6_pitch","seq_7_pitch",
    "seq_8_pitch","seq_9_pitch","seq_10_pitch","seq_11_pitch",
    "seq_12_pitch","seq_13_pitch","seq_14_pitch","seq_15_pitch",
];
const SEQ_GATE_KEYS: [&str; 16] = [
    "seq_0_gate","seq_1_gate","seq_2_gate","seq_3_gate",
    "seq_4_gate","seq_5_gate","seq_6_gate","seq_7_gate",
    "seq_8_gate","seq_9_gate","seq_10_gate","seq_11_gate",
    "seq_12_gate","seq_13_gate","seq_14_gate","seq_15_gate",
];
const SEQ_VEL_KEYS: [&str; 16] = [
    "seq_0_vel","seq_1_vel","seq_2_vel","seq_3_vel",
    "seq_4_vel","seq_5_vel","seq_6_vel","seq_7_vel",
    "seq_8_vel","seq_9_vel","seq_10_vel","seq_11_vel",
    "seq_12_vel","seq_13_vel","seq_14_vel","seq_15_vel",
];

impl PitchSequencer {
    pub fn new() -> Self {
        Self {
            steps: [PitchStep::default(); MAX_STEPS],
            length: 16,
            enabled: false,
            rate: StepRate::Sixteenth,
            scale: ScaleType::Chromatic,
            swing: 0.0,
            current_step: 0,
            phase_acc: 0.0,
            prev_gate: true,
            step_atom: Arc::new(AtomicU8::new(0)),
        }
    }

    pub fn step_atom(&self) -> Arc<AtomicU8> {
        self.step_atom.clone()
    }

    pub fn reset(&mut self) {
        self.current_step = 0;
        self.phase_acc = 0.0;
        self.prev_gate = true;
    }

    /// Advance by `block_len` samples. Returns step event info.
    #[inline]
    pub fn tick(&mut self, sample_rate: f32, bpm: f32, block_len: usize) -> StepEvent {
        if !self.enabled {
            return StepEvent {
                stepped: false, gate: true, pitch_offset: 0.0,
                velocity: 0, gate_changed: false,
            };
        }

        let samples_per_step = (sample_rate as f64 * 60.0)
            / (bpm as f64 * self.rate.steps_per_beat());

        // Swing: delay odd steps
        let swing_offset = if self.current_step % 2 == 1 {
            samples_per_step * self.swing as f64 * 0.5
        } else {
            0.0
        };

        self.phase_acc += block_len as f64;
        let mut stepped = false;

        if self.phase_acc >= samples_per_step + swing_offset {
            self.phase_acc -= samples_per_step + swing_offset;
            self.current_step = (self.current_step + 1) % self.length;
            self.step_atom.store(self.current_step, Ordering::Relaxed);
            stepped = true;
        }

        let step = &self.steps[self.current_step as usize];
        let pitch = self.quantize(step.pitch);
        let gate_changed = stepped && (step.gate != self.prev_gate);
        if stepped { self.prev_gate = step.gate; }

        StepEvent {
            stepped,
            gate: step.gate,
            pitch_offset: pitch,
            velocity: step.velocity,
            gate_changed,
        }
    }

    /// Quantize a semitone offset to the nearest scale degree.
    fn quantize(&self, semitones: i8) -> f32 {
        if self.scale == ScaleType::Chromatic {
            return semitones as f32;
        }
        let intervals = self.scale.intervals();
        let s = semitones as i32;
        // Decompose into octave + remainder
        let octave = if s >= 0 { s / 12 } else { (s - 11) / 12 };
        let rem = ((s % 12) + 12) % 12;
        // Find nearest scale degree
        let mut best = intervals[0] as i32;
        let mut best_dist = (rem - best).abs();
        for &iv in &intervals[1..] {
            let dist = (rem - iv as i32).abs();
            if dist < best_dist {
                best = iv as i32;
                best_dist = dist;
            }
        }
        (octave * 12 + best) as f32
    }

    /// Load from patch params map.
    #[allow(dead_code)]
    pub fn load_from_params(&mut self, params: &std::collections::BTreeMap<String, f32>) {
        self.enabled = params.get("seq_enabled").copied().unwrap_or(0.0) > 0.5;
        self.length = params.get("seq_length").copied().unwrap_or(16.0).clamp(1.0, 16.0) as u8;
        self.rate = StepRate::from_index(
            params.get("seq_rate").copied().unwrap_or(2.0) as u8
        );
        self.scale = ScaleType::from_index(
            params.get("seq_scale").copied().unwrap_or(0.0) as u8
        );
        self.swing = params.get("seq_swing").copied().unwrap_or(0.0);
        for i in 0..MAX_STEPS {
            self.steps[i].pitch = params.get(SEQ_PITCH_KEYS[i])
                .copied().unwrap_or(0.0) as i8;
            self.steps[i].gate = params.get(SEQ_GATE_KEYS[i])
                .copied().unwrap_or(1.0) > 0.5;
            self.steps[i].velocity = params.get(SEQ_VEL_KEYS[i])
                .copied().unwrap_or(0.0) as u8;
        }
    }

    /// Save to patch params map.
    #[allow(dead_code)]
    pub fn save_to_params(&self, params: &mut std::collections::BTreeMap<String, f32>) {
        params.insert("seq_enabled".into(), if self.enabled { 1.0 } else { 0.0 });
        params.insert("seq_length".into(), self.length as f32);
        params.insert("seq_rate".into(), self.rate.index() as f32);
        params.insert("seq_scale".into(), self.scale.index() as f32);
        params.insert("seq_swing".into(), self.swing);
        for i in 0..MAX_STEPS {
            params.insert(SEQ_PITCH_KEYS[i].to_string(), self.steps[i].pitch as f32);
            params.insert(SEQ_GATE_KEYS[i].to_string(), if self.steps[i].gate { 1.0 } else { 0.0 });
            params.insert(SEQ_VEL_KEYS[i].to_string(), self.steps[i].velocity as f32);
        }
    }
}
