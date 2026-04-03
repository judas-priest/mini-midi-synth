#![allow(dead_code)]
/// Dedicated drum engine for MIDI channel 10 (GM drum map notes 36-51).
/// Lightweight per-instrument voices, choke groups, velocity-to-timbre,
/// and a built-in step sequencer. Zero heap allocation in the audio path.

use std::f32::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const NUM_DRUM_SLOTS: usize = 16; // notes 36..=51
const DRUM_NOTE_BASE: u8 = 36;

/// Choke group IDs. 0 = no choke group.
/// Closed HH (42), Pedal HH (44), Open HH (46) share group 1.
const CHOKE_GROUPS: [u8; NUM_DRUM_SLOTS] = [
    0, // 36 Kick
    0, // 37 Side Stick
    0, // 38 Snare
    0, // 39 Clap
    0, // 40 E-Snare
    0, // 41 Low Floor Tom
    1, // 42 Closed HH
    0, // 43 High Floor Tom
    1, // 44 Pedal HH
    0, // 45 Low Tom
    1, // 46 Open HH
    0, // 47 Low-Mid Tom
    0, // 48 Hi-Mid Tom
    2, // 49 Crash (choke group 2 — crash chokes on retrigger)
    0, // 50 High Tom
    0, // 51 Ride
];

pub const DRUM_NAMES: [&str; NUM_DRUM_SLOTS] = [
    "Kick", "Side Stick", "Snare", "Clap",
    "E-Snare", "Lo Floor Tom", "Closed HH", "Hi Floor Tom",
    "Pedal HH", "Low Tom", "Open HH", "Lo-Mid Tom",
    "Hi-Mid Tom", "Crash", "High Tom", "Ride",
];

/// 808-style metallic oscillator bank frequencies (Hz).
const METAL_FREQS: [f32; 6] = [204.68, 297.64, 369.47, 411.30, 521.77, 585.32];

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Cheap soft-clip: avoids tanh cost.
#[inline(always)]
fn soft_clip(x: f32) -> f32 {
    x / (1.0 + x.abs())
}

/// Xorshift32 PRNG — returns white noise in -1..1.
#[inline(always)]
fn xorshift32(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Pre-compute exponential decay coefficient from time in seconds.
/// `level *= coeff` each sample → reaches ~-60dB after `time_s`.
#[inline(always)]
fn decay_coeff(time_s: f32, sample_rate: f32) -> f32 {
    if time_s < 0.0001 {
        0.0
    } else {
        (-6.908 / (time_s * sample_rate)).exp() // -60dB = exp(-6.908)
    }
}

// ---------------------------------------------------------------------------
// Per-instrument voice structs (no heap, Copy)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct KickVoice {
    phase: f32,
    amp: f32,
    amp_coeff: f32,
    pitch_env: f32,
    pitch_coeff: f32,
    base_freq: f32,
    pitch_amount: f32, // in Hz above base
    click_level: f32,
    click_phase: f32,
    active: bool,
    sample_rate: f32,
}

impl KickVoice {
    fn new(sr: f32) -> Self {
        Self {
            phase: 0.0, amp: 0.0, amp_coeff: 0.0,
            pitch_env: 0.0, pitch_coeff: 0.0,
            base_freq: 50.0, pitch_amount: 250.0,
            click_level: 0.0, click_phase: 0.0,
            active: false, sample_rate: sr,
        }
    }

    fn trigger(&mut self, velocity: f32, params: &DrumSlotParams) {
        self.active = true;
        self.phase = 0.0;
        self.click_phase = 0.0;
        let vel = velocity.powf(0.7);
        self.amp = vel;
        self.amp_coeff = decay_coeff(params.decay * (0.7 + 0.3 * vel), self.sample_rate);
        self.base_freq = 50.0 * (params.tune / 12.0).exp2();
        // Velocity: harder hit = wider pitch sweep
        self.pitch_amount = 250.0 * (0.5 + 0.5 * velocity);
        self.pitch_coeff = decay_coeff(0.03 + 0.02 * (1.0 - velocity), self.sample_rate);
        self.pitch_env = 1.0;
        // Click transient
        self.click_level = 0.3 * velocity.powf(1.5);
    }

    #[inline(always)]
    fn tick(&mut self) -> f32 {
        if !self.active { return 0.0; }
        self.pitch_env *= self.pitch_coeff;
        let freq = self.base_freq + self.pitch_amount * self.pitch_env;
        let tone = (self.phase * TAU).sin();
        self.phase += freq / self.sample_rate;
        self.phase -= self.phase.floor();

        // Click transient (fast sine burst at ~3x frequency)
        let click = if self.click_level > 0.001 {
            let c = (self.click_phase * TAU).sin() * self.click_level;
            self.click_phase += (freq * 3.0) / self.sample_rate;
            self.click_phase -= self.click_phase.floor();
            self.click_level *= 0.995; // very fast decay
            c
        } else {
            0.0
        };

        let out = soft_clip(tone * self.amp + click);
        self.amp *= self.amp_coeff;
        if self.amp < 0.0001 { self.active = false; }
        out
    }

    fn kill(&mut self, fade_samples: f32) {
        if self.active {
            self.amp_coeff = decay_coeff(fade_samples / self.sample_rate, self.sample_rate);
        }
    }
}

#[derive(Clone, Copy)]
/// 909-style snare drum: 3 body oscillators (triangle) + bandpass-filtered noise
/// with separate wire rattle component and transient click.
///
/// Real 909 snare uses two triangle oscillators (180/330 Hz) for the drum head,
/// white noise through a multimode filter for snare wires, and a short click
/// transient for attack definition. We add a 3rd body oscillator for the
/// (0,2) drum head mode and proper 2-pole SVF bandpass for the noise.
struct SnareVoice {
    // 3 body oscillators (triangle waves — 909 uses triangle, not sine)
    phases: [f32; 3],
    body_freqs: [f32; 3],
    body_amp: f32,
    body_coeff: f32,
    // Pitch envelope for body
    pitch_env: f32,
    pitch_coeff: f32,
    // Snare wire noise — 2-pole SVF bandpass
    noise_state: u32,
    noise_amp: f32,
    noise_coeff: f32,
    svf_lp: f32,      // SVF lowpass state
    svf_bp: f32,      // SVF bandpass state (this is our output)
    svf_w: f32,       // SVF frequency coefficient
    svf_q_inv: f32,   // SVF 1/Q
    // Wire rattle — highpass filtered noise, longer tail
    wire_amp: f32,
    wire_coeff: f32,
    wire_hp: f32,
    wire_hp2: f32,     // 2-pole HP for cleaner wire sound
    // Transient click — very short noise burst
    click_amp: f32,
    click_coeff: f32,
    //
    active: bool,
    sample_rate: f32,
}

impl SnareVoice {
    fn new(sr: f32) -> Self {
        Self {
            phases: [0.0; 3],
            body_freqs: [180.0, 330.0, 525.0],
            body_amp: 0.0, body_coeff: 0.0,
            pitch_env: 0.0, pitch_coeff: 0.0,
            noise_state: 0xDEADBEEF,
            noise_amp: 0.0, noise_coeff: 0.0,
            svf_lp: 0.0, svf_bp: 0.0,
            svf_w: 0.0, svf_q_inv: 0.0,
            wire_amp: 0.0, wire_coeff: 0.0,
            wire_hp: 0.0, wire_hp2: 0.0,
            click_amp: 0.0, click_coeff: 0.0,
            active: false, sample_rate: sr,
        }
    }

    fn trigger(&mut self, velocity: f32, params: &DrumSlotParams) {
        self.active = true;
        self.phases = [0.0; 3];
        self.svf_lp = 0.0;
        self.svf_bp = 0.0;
        self.wire_hp = 0.0;
        self.wire_hp2 = 0.0;

        let vel = velocity.powf(0.7);
        let tune_mult = (params.tune / 12.0).exp2();

        // 3 drum head modes: (0,1)=180Hz, (1,1)=330Hz, (0,2)=525Hz
        self.body_freqs[0] = 180.0 * tune_mult;
        self.body_freqs[1] = 330.0 * tune_mult;
        self.body_freqs[2] = 525.0 * tune_mult;

        self.body_amp = vel * 0.55;
        self.body_coeff = decay_coeff(params.decay * 0.35, self.sample_rate);

        // Pitch envelope: fast pitch drop on body (909-style snap)
        self.pitch_env = 1.0;
        self.pitch_coeff = decay_coeff(0.006, self.sample_rate); // 6ms — faster than before

        // Snare wire noise: SVF bandpass centered at ~2.5kHz, Q=1.5
        // Velocity controls snare amount (harder hit = more wire)
        let noise_center = 2500.0 + velocity * 1500.0; // 2.5-4kHz based on velocity
        self.svf_w = (std::f32::consts::PI * noise_center / self.sample_rate).sin().min(0.95);
        self.svf_q_inv = 1.0 / 1.5; // Q = 1.5
        self.noise_amp = vel * (0.35 + 0.65 * velocity);
        self.noise_coeff = decay_coeff(params.decay * 0.55, self.sample_rate);

        // Wire rattle: highpass noise, longer tail, adds sizzle
        self.wire_amp = vel * 0.25 * velocity;
        self.wire_coeff = decay_coeff(params.decay * 0.9, self.sample_rate);

        // Transient click: very short burst for attack definition (909 characteristic)
        self.click_amp = vel * 0.8;
        self.click_coeff = decay_coeff(0.0015, self.sample_rate); // 1.5ms — extremely fast
    }

    #[inline(always)]
    fn tick(&mut self) -> f32 {
        if !self.active { return 0.0; }

        // Pitch envelope
        self.pitch_env *= self.pitch_coeff;
        let pitch_offset = 1.0 + self.pitch_env * 0.5;

        // 3 triangle body oscillators (909 uses triangle, warmer than sine)
        let sr = self.sample_rate;
        let mut body = 0.0_f32;
        let body_levels = [1.0_f32, 0.65, 0.35]; // decreasing amplitude for higher modes
        for i in 0..3 {
            let f = self.body_freqs[i] * pitch_offset;
            let p = self.phases[i];
            // Triangle wave from phase
            let tri = if p < 0.5 { 4.0 * p - 1.0 } else { 3.0 - 4.0 * p };
            body += tri * body_levels[i];
            self.phases[i] += f / sr;
            self.phases[i] -= self.phases[i].floor();
        }
        body *= self.body_amp;

        // Snare wire noise through 2-pole SVF bandpass
        let white = xorshift32(&mut self.noise_state);
        let hp = white - self.svf_lp - self.svf_q_inv * self.svf_bp;
        self.svf_bp += self.svf_w * hp;
        self.svf_lp += self.svf_w * self.svf_bp;
        let noise = self.svf_bp * self.noise_amp;

        // Wire rattle: 2-pole highpass at ~4kHz for sizzle
        let white2 = xorshift32(&mut self.noise_state);
        self.wire_hp += 0.25 * (white2 - self.wire_hp);
        self.wire_hp2 += 0.25 * (self.wire_hp - self.wire_hp2);
        let wire = (white2 - self.wire_hp2) * self.wire_amp;

        // Transient click
        let click = xorshift32(&mut self.noise_state) * self.click_amp;

        // Decay all envelopes
        self.body_amp *= self.body_coeff;
        self.noise_amp *= self.noise_coeff;
        self.wire_amp *= self.wire_coeff;
        self.click_amp *= self.click_coeff;

        if self.body_amp < 0.0001 && self.noise_amp < 0.0001
            && self.wire_amp < 0.0001 && self.click_amp < 0.0001
        {
            self.active = false;
        }

        soft_clip(body + noise + wire + click)
    }

    fn kill(&mut self, fade_samples: f32) {
        if self.active {
            let c = decay_coeff(fade_samples / self.sample_rate, self.sample_rate);
            self.body_coeff = c;
            self.noise_coeff = c;
            self.wire_coeff = c;
            self.click_coeff = c;
        }
    }
}

/// 808-style metallic oscillator: 6 square waves, multiply signs, then filter.
/// Uses dual bandpass filters at 3440 Hz and 7100 Hz for authentic 808 character.
/// Used for hi-hat (closed/open/pedal), crash, ride.
#[derive(Clone, Copy)]
struct MetalVoice {
    phases: [f32; 6],
    freqs: [f32; 6],
    amp: f32,
    amp_coeff: f32,
    bp1_state: f32,  // bandpass 1 at ~3440 Hz
    bp1_d1: f32,
    bp2_state: f32,  // bandpass 2 at ~7100 Hz
    bp2_d1: f32,
    hp_state: f32,   // final highpass
    active: bool,
    sample_rate: f32,
}

impl MetalVoice {
    fn new(sr: f32) -> Self {
        Self {
            phases: [0.0; 6],
            freqs: METAL_FREQS,
            amp: 0.0, amp_coeff: 0.0,
            bp1_state: 0.0, bp1_d1: 0.0,
            bp2_state: 0.0, bp2_d1: 0.0,
            hp_state: 0.0,
            active: false, sample_rate: sr,
        }
    }

    fn trigger(&mut self, velocity: f32, decay_time: f32, params: &DrumSlotParams) {
        self.active = true;
        self.phases = [0.0; 6];
        self.bp1_state = 0.0; self.bp1_d1 = 0.0;
        self.bp2_state = 0.0; self.bp2_d1 = 0.0;
        self.hp_state = 0.0;
        let vel = velocity.powf(0.7);
        self.amp = vel;
        self.amp_coeff = decay_coeff(decay_time * params.decay, self.sample_rate);
        let tune_mult = (params.tune / 12.0).exp2();
        for i in 0..6 {
            self.freqs[i] = METAL_FREQS[i] * tune_mult;
        }
    }

    #[inline(always)]
    fn tick(&mut self) -> f32 {
        if !self.active { return 0.0; }

        let sr = self.sample_rate;

        // 6 square oscillators, multiply signs
        let mut metallic = 1.0_f32;
        for i in 0..6 {
            let sign = if self.phases[i] < 0.5 { 1.0 } else { -1.0 };
            metallic *= sign;
            self.phases[i] += self.freqs[i] / sr;
            self.phases[i] -= self.phases[i].floor();
        }

        // Dual bandpass filters for authentic 808 hi-hat character

        // BP1 at ~3440 Hz (one-pole LP differenced for bandpass)
        let coeff1 = (std::f32::consts::PI * 3440.0 / sr).sin().min(0.99);
        self.bp1_state += coeff1 * (metallic - self.bp1_state);
        let bp1 = self.bp1_state - self.bp1_d1;
        self.bp1_d1 = self.bp1_state;

        // BP2 at ~7100 Hz
        let coeff2 = (std::f32::consts::PI * 7100.0 / sr).sin().min(0.99);
        self.bp2_state += coeff2 * (metallic - self.bp2_state);
        let bp2 = self.bp2_state - self.bp2_d1;
        self.bp2_d1 = self.bp2_state;

        // Mix parallel bandpasses and apply highpass
        let mixed = bp1 + bp2 * 0.7;
        self.hp_state += 0.15 * (mixed - self.hp_state);
        let out = mixed - self.hp_state;

        let result = out * self.amp;
        self.amp *= self.amp_coeff;
        if self.amp < 0.0001 { self.active = false; }
        result
    }

    fn kill(&mut self, fade_samples: f32) {
        if self.active {
            self.amp_coeff = decay_coeff(fade_samples / self.sample_rate, self.sample_rate);
        }
    }
}

/// Hand clap: 4 rapid noise bursts + decay tail.
#[derive(Clone, Copy)]
struct ClapVoice {
    noise_state: u32,
    amp: f32,
    amp_coeff: f32,
    // Burst sequencing
    burst_index: u8,    // 0-3 = bursts, 4 = tail
    burst_timer: f32,   // samples remaining in current burst/gap
    burst_amp: f32,     // envelope within burst
    bp_state: [f32; 2],
    filter_freq: f32,
    active: bool,
    sample_rate: f32,
}

impl ClapVoice {
    fn new(sr: f32) -> Self {
        Self {
            noise_state: 0xCAFEBABE,
            amp: 0.0, amp_coeff: 0.0,
            burst_index: 0, burst_timer: 0.0, burst_amp: 0.0,
            bp_state: [0.0; 2], filter_freq: 0.08,
            active: false, sample_rate: sr,
        }
    }

    fn trigger(&mut self, velocity: f32, params: &DrumSlotParams) {
        self.active = true;
        self.bp_state = [0.0; 2];
        let vel = velocity.powf(0.7);
        self.amp = vel;
        self.amp_coeff = decay_coeff(params.decay * 0.6, self.sample_rate);
        self.burst_index = 0;
        self.burst_timer = self.sample_rate * 0.008; // 8ms per burst
        self.burst_amp = 1.0;
        self.filter_freq = 0.05 + 0.06 * velocity;
    }

    #[inline(always)]
    fn tick(&mut self) -> f32 {
        if !self.active { return 0.0; }

        let white = xorshift32(&mut self.noise_state);

        // Bandpass filter (~1.1kHz center)
        self.bp_state[0] += self.filter_freq * (white - self.bp_state[0]);
        self.bp_state[1] += self.filter_freq * (self.bp_state[0] - self.bp_state[1]);
        let filtered = self.bp_state[0] - self.bp_state[1];

        let env = if self.burst_index < 4 {
            // Burst phase: 4 bursts with gaps
            self.burst_timer -= 1.0;
            if self.burst_timer <= 0.0 {
                self.burst_index += 1;
                if self.burst_index < 4 {
                    self.burst_timer = self.sample_rate * 0.011; // 11ms gap+burst
                    self.burst_amp = 1.0;
                }
            }
            self.burst_amp *= 0.997; // fast decay within burst
            self.burst_amp
        } else {
            // Tail phase
            self.amp *= self.amp_coeff;
            1.0
        };

        let out = filtered * env * self.amp;
        if self.burst_index >= 4 && self.amp < 0.0001 { self.active = false; }
        soft_clip(out)
    }

    fn kill(&mut self, fade_samples: f32) {
        if self.active {
            self.amp_coeff = decay_coeff(fade_samples / self.sample_rate, self.sample_rate);
            self.burst_index = 4; // skip to tail
        }
    }
}

/// Tom voice: sine + pitch envelope (like kick but higher freq, shorter).
#[derive(Clone, Copy)]
struct TomVoice {
    phase: f32,
    amp: f32,
    amp_coeff: f32,
    pitch_env: f32,
    pitch_coeff: f32,
    base_freq: f32,
    pitch_amount: f32,
    active: bool,
    sample_rate: f32,
}

impl TomVoice {
    fn new(sr: f32) -> Self {
        Self {
            phase: 0.0, amp: 0.0, amp_coeff: 0.0,
            pitch_env: 0.0, pitch_coeff: 0.0,
            base_freq: 150.0, pitch_amount: 80.0,
            active: false, sample_rate: sr,
        }
    }

    fn trigger(&mut self, velocity: f32, base_freq: f32, params: &DrumSlotParams) {
        self.active = true;
        self.phase = 0.0;
        let vel = velocity.powf(0.7);
        self.amp = vel;
        self.base_freq = base_freq * (params.tune / 12.0).exp2();
        self.amp_coeff = decay_coeff(params.decay * (0.6 + 0.4 * vel), self.sample_rate);
        self.pitch_amount = 80.0 * (0.5 + 0.5 * velocity);
        self.pitch_env = 1.0;
        self.pitch_coeff = decay_coeff(0.015, self.sample_rate);
    }

    #[inline(always)]
    fn tick(&mut self) -> f32 {
        if !self.active { return 0.0; }
        self.pitch_env *= self.pitch_coeff;
        let freq = self.base_freq + self.pitch_amount * self.pitch_env;
        let out = (self.phase * TAU).sin() * self.amp;
        self.phase += freq / self.sample_rate;
        self.phase -= self.phase.floor();
        self.amp *= self.amp_coeff;
        if self.amp < 0.0001 { self.active = false; }
        out
    }

    fn kill(&mut self, fade_samples: f32) {
        if self.active {
            self.amp_coeff = decay_coeff(fade_samples / self.sample_rate, self.sample_rate);
        }
    }
}

/// Click/rimshot: short bandpass noise burst.
#[derive(Clone, Copy)]
struct ClickVoice {
    noise_state: u32,
    amp: f32,
    amp_coeff: f32,
    bp_state: [f32; 2],
    filter_freq: f32,
    // Short triangle for tonal "ping"
    phase: f32,
    tone_freq: f32,
    tone_amp: f32,
    tone_coeff: f32,
    active: bool,
    sample_rate: f32,
}

impl ClickVoice {
    fn new(sr: f32) -> Self {
        Self {
            noise_state: 0xFEEDFACE,
            amp: 0.0, amp_coeff: 0.0,
            bp_state: [0.0; 2], filter_freq: 0.2,
            phase: 0.0, tone_freq: 400.0, tone_amp: 0.0, tone_coeff: 0.0,
            active: false, sample_rate: sr,
        }
    }

    fn trigger(&mut self, velocity: f32, params: &DrumSlotParams) {
        self.active = true;
        self.bp_state = [0.0; 2];
        self.phase = 0.0;
        let vel = velocity.powf(0.7);
        self.amp = vel * 0.8;
        self.amp_coeff = decay_coeff(0.02 * params.decay, self.sample_rate); // ~20ms * decay
        self.tone_freq = 400.0 * (params.tune / 12.0).exp2();
        self.tone_amp = vel * 0.5;
        self.tone_coeff = decay_coeff(0.003, self.sample_rate); // 3ms
        self.filter_freq = 0.15 + 0.15 * velocity;
    }

    #[inline(always)]
    fn tick(&mut self) -> f32 {
        if !self.active { return 0.0; }

        // Noise component
        let white = xorshift32(&mut self.noise_state);
        self.bp_state[0] += self.filter_freq * (white - self.bp_state[0]);
        self.bp_state[1] += self.filter_freq * (self.bp_state[0] - self.bp_state[1]);
        let noise = (self.bp_state[0] - self.bp_state[1]) * self.amp;

        // Triangle tone "ping"
        let tri = (2.0 * (2.0 * self.phase - 1.0).abs() - 1.0) * self.tone_amp;
        self.phase += self.tone_freq / self.sample_rate;
        self.phase -= self.phase.floor();
        self.tone_amp *= self.tone_coeff;

        self.amp *= self.amp_coeff;
        if self.amp < 0.0001 && self.tone_amp < 0.0001 { self.active = false; }
        soft_clip(noise + tri)
    }

    fn kill(&mut self, fade_samples: f32) {
        if self.active {
            self.amp_coeff = decay_coeff(fade_samples / self.sample_rate, self.sample_rate);
            self.tone_coeff = self.amp_coeff;
        }
    }
}

// ---------------------------------------------------------------------------
// Unified drum voice enum
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum DrumVoice {
    Kick(KickVoice),
    Snare(SnareVoice),
    Metal(MetalVoice),
    Clap(ClapVoice),
    Tom(TomVoice),
    Click(ClickVoice),
}

impl DrumVoice {
    #[inline(always)]
    fn tick(&mut self) -> f32 {
        match self {
            Self::Kick(v) => v.tick(),
            Self::Snare(v) => v.tick(),
            Self::Metal(v) => v.tick(),
            Self::Clap(v) => v.tick(),
            Self::Tom(v) => v.tick(),
            Self::Click(v) => v.tick(),
        }
    }

    fn kill_fade(&mut self, fade_samples: f32) {
        match self {
            Self::Kick(v) => v.kill(fade_samples),
            Self::Snare(v) => v.kill(fade_samples),
            Self::Metal(v) => v.kill(fade_samples),
            Self::Clap(v) => v.kill(fade_samples),
            Self::Tom(v) => v.kill(fade_samples),
            Self::Click(v) => v.kill(fade_samples),
        }
    }

    fn is_active(&self) -> bool {
        match self {
            Self::Kick(v) => v.active,
            Self::Snare(v) => v.active,
            Self::Metal(v) => v.active,
            Self::Clap(v) => v.active,
            Self::Tom(v) => v.active,
            Self::Click(v) => v.active,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-slot parameters (tunable from GUI)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DrumSlotParams {
    pub level: f32,   // 0..1
    pub pan: f32,     // -1..+1
    pub tune: f32,    // semitones offset (-24..+24)
    pub decay: f32,   // multiplier on default decay (0.1..4.0)
}

impl Default for DrumSlotParams {
    fn default() -> Self {
        Self { level: 0.8, pan: 0.0, tune: 0.0, decay: 1.0 }
    }
}

// ---------------------------------------------------------------------------
// Step sequencer
// ---------------------------------------------------------------------------

const MAX_STEPS: usize = 16;
const MAX_PATTERNS: usize = 8;

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct StepData {
    pub velocity: u8, // 0 = off, 1-127 = on
}

impl Default for StepData {
    fn default() -> Self { Self { velocity: 0 } }
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DrumPattern {
    pub steps: [[StepData; MAX_STEPS]; NUM_DRUM_SLOTS],
    pub length: u8, // 1..16
}

impl Default for DrumPattern {
    fn default() -> Self {
        Self {
            steps: [[StepData::default(); MAX_STEPS]; NUM_DRUM_SLOTS],
            length: 16,
        }
    }
}

pub struct StepSequencer {
    pub patterns: [DrumPattern; MAX_PATTERNS],
    pub current_pattern: u8,
    pub playing: bool,
    pub recording: bool,
    pub bpm: f32,
    pub swing: f32, // 0.0 = straight, 0.66 = triplet feel
    pub current_step: u8,
    phase_acc: f64, // fractional sample accumulator
    undo_snapshot: Option<Box<[DrumPattern; MAX_PATTERNS]>>,
}

impl StepSequencer {
    fn new() -> Self {
        Self {
            patterns: [DrumPattern::default(); MAX_PATTERNS],
            current_pattern: 0,
            playing: false,
            recording: false,
            bpm: 120.0,
            swing: 0.0,
            current_step: 0,
            phase_acc: 0.0,
            undo_snapshot: None,
        }
    }

    /// Clear all steps in the current pattern.
    pub fn clear_pattern(&mut self) {
        self.undo_snapshot = Some(Box::new(self.patterns.clone()));
        let pat = &mut self.patterns[self.current_pattern as usize];
        pat.steps = [[StepData::default(); MAX_STEPS]; NUM_DRUM_SLOTS];
    }

    /// Undo last clear or restore pre-recording state.
    pub fn undo(&mut self) {
        if let Some(snapshot) = self.undo_snapshot.take() {
            self.patterns = *snapshot;
        }
    }

    /// Save snapshot before destructive operation (e.g. recording start).
    pub fn save_snapshot(&mut self) {
        self.undo_snapshot = Some(Box::new(self.patterns.clone()));
    }

    /// Advance sequencer by one sample. Returns list of (slot_index, velocity) to trigger.
    /// Uses a fixed-size array to avoid allocation.
    #[inline]
    fn tick(&mut self, sample_rate: f32) -> [(u8, u8); NUM_DRUM_SLOTS] {
        let mut triggers = [(0u8, 0u8); NUM_DRUM_SLOTS];
        if !self.playing { return triggers; }

        let steps_per_beat = 4.0_f64; // 16th notes
        let samples_per_step = (sample_rate as f64 * 60.0) / (self.bpm as f64 * steps_per_beat);

        // Swing: odd steps delayed
        let swing_offset = if self.current_step % 2 == 1 {
            samples_per_step * self.swing as f64 * 0.5
        } else {
            0.0
        };

        self.phase_acc += 1.0;
        if self.phase_acc >= samples_per_step + swing_offset {
            self.phase_acc -= samples_per_step + swing_offset;
            let pattern = &self.patterns[self.current_pattern as usize];
            let step = self.current_step as usize;
            if step < pattern.length as usize {
                for slot in 0..NUM_DRUM_SLOTS {
                    let vel = pattern.steps[slot][step].velocity;
                    if vel > 0 {
                        triggers[slot] = (1, vel);
                    }
                }
            }
            self.current_step = (self.current_step + 1) % pattern.length;
        }
        triggers
    }

    /// Record a hit quantized to the nearest step.
    /// `current_step` points to the NEXT step (already advanced after trigger),
    /// so we compute the nearest step using phase_acc:
    /// - phase_acc small → just advanced → nearest is previous step (the one that just played)
    /// - phase_acc large → about to advance → nearest is current_step (the upcoming one)
    pub fn record_hit(&mut self, slot: usize, velocity: u8, sample_rate: f32) -> Option<(u8, u8, u8)> {
        if !self.recording || !self.playing { return None; }
        if slot >= NUM_DRUM_SLOTS { return None; }
        let pat_idx = self.current_pattern;
        let pat = &mut self.patterns[pat_idx as usize];
        let len = pat.length;

        let samples_per_step = (sample_rate as f64 * 60.0) / (self.bpm as f64 * 4.0);
        let step = if self.phase_acc < samples_per_step * 0.5 {
            // Just passed a step boundary → record to the step that just triggered
            (self.current_step as usize + len as usize - 1) % len as usize
        } else {
            // Closer to next step → record to upcoming step
            self.current_step as usize
        };

        if step < len as usize {
            pat.steps[slot][step].velocity = velocity;
            Some((pat_idx, slot as u8, step as u8))
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        self.current_step = 0;
        self.phase_acc = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Drum Engine
// ---------------------------------------------------------------------------

/// Default base frequencies for tom slots.
const TOM_FREQS: [(usize, f32); 6] = [
    (5, 80.0),   // 41 Low Floor Tom
    (7, 110.0),  // 43 High Floor Tom
    (9, 130.0),  // 45 Low Tom
    (11, 160.0), // 47 Low-Mid Tom
    (12, 200.0), // 48 Hi-Mid Tom
    (14, 240.0), // 50 High Tom
];

pub struct DrumEngine {
    voices: [DrumVoice; NUM_DRUM_SLOTS],
    pub params: [DrumSlotParams; NUM_DRUM_SLOTS],
    pub sequencer: StepSequencer,
    pub volume: f32,
    pub enabled: bool,
    sample_rate: f32,
    /// Shared atom for GUI to read current step position.
    step_atom: Arc<AtomicU8>,
    /// Shared atoms for GUI to read play/rec state (set by engine, read by GUI).
    play_atom: Arc<AtomicU8>,
    rec_atom: Arc<AtomicU8>,
}

impl DrumEngine {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate;
        let voices = [
            DrumVoice::Kick(KickVoice::new(sr)),        // 36 Kick
            DrumVoice::Click(ClickVoice::new(sr)),       // 37 Side Stick
            DrumVoice::Snare(SnareVoice::new(sr)),       // 38 Snare
            DrumVoice::Clap(ClapVoice::new(sr)),         // 39 Clap
            DrumVoice::Snare(SnareVoice::new(sr)),       // 40 E-Snare
            DrumVoice::Tom(TomVoice::new(sr)),           // 41 Low Floor Tom
            DrumVoice::Metal(MetalVoice::new(sr)),       // 42 Closed HH
            DrumVoice::Tom(TomVoice::new(sr)),           // 43 High Floor Tom
            DrumVoice::Metal(MetalVoice::new(sr)),       // 44 Pedal HH
            DrumVoice::Tom(TomVoice::new(sr)),           // 45 Low Tom
            DrumVoice::Metal(MetalVoice::new(sr)),       // 46 Open HH
            DrumVoice::Tom(TomVoice::new(sr)),           // 47 Low-Mid Tom
            DrumVoice::Tom(TomVoice::new(sr)),           // 48 Hi-Mid Tom
            DrumVoice::Metal(MetalVoice::new(sr)),       // 49 Crash
            DrumVoice::Tom(TomVoice::new(sr)),           // 50 High Tom
            DrumVoice::Metal(MetalVoice::new(sr)),       // 51 Ride
        ];
        Self {
            voices,
            params: [DrumSlotParams::default(); NUM_DRUM_SLOTS],
            sequencer: StepSequencer::new(),
            volume: 0.8,
            enabled: true,
            sample_rate,
            step_atom: Arc::new(AtomicU8::new(0)),
            play_atom: Arc::new(AtomicU8::new(0)),
            rec_atom: Arc::new(AtomicU8::new(0)),
        }
    }

    pub fn step_atom(&self) -> Arc<AtomicU8> {
        self.step_atom.clone()
    }

    pub fn play_atom(&self) -> Arc<AtomicU8> {
        self.play_atom.clone()
    }

    pub fn rec_atom(&self) -> Arc<AtomicU8> {
        self.rec_atom.clone()
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let step = self.step_atom.clone();
        let play = self.play_atom.clone();
        let rec = self.rec_atom.clone();
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
        self.step_atom = step;
        self.play_atom = play;
        self.rec_atom = rec;
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        if note < DRUM_NOTE_BASE || note > DRUM_NOTE_BASE + NUM_DRUM_SLOTS as u8 - 1 {
            return;
        }
        let slot = (note - DRUM_NOTE_BASE) as usize;
        let vel = velocity as f32 / 127.0;
        self.trigger_slot(slot, vel);
    }

    pub fn note_off(&mut self, _note: u8) {
        // Most drum sounds are one-shot, ignore note-off
    }

    fn trigger_slot(&mut self, slot: usize, velocity: f32) {
        if slot >= NUM_DRUM_SLOTS { return; }

        // Choke group: kill other voices in same group
        let group = CHOKE_GROUPS[slot];
        if group > 0 {
            let fade = self.sample_rate * 0.003; // 3ms fade
            for i in 0..NUM_DRUM_SLOTS {
                if i != slot && CHOKE_GROUPS[i] == group && self.voices[i].is_active() {
                    self.voices[i].kill_fade(fade);
                }
            }
        }

        let params = &self.params[slot];
        match &mut self.voices[slot] {
            DrumVoice::Kick(v) => v.trigger(velocity, params),
            DrumVoice::Snare(v) => v.trigger(velocity, params),
            DrumVoice::Metal(v) => {
                // Different decay for closed/open HH, crash, ride
                let decay_time = match slot {
                    6 => 0.05,   // Closed HH: 50ms
                    8 => 0.04,   // Pedal HH: 40ms
                    10 => 0.4,   // Open HH: 400ms
                    13 => 2.0,   // Crash: 2s
                    15 => 0.8,   // Ride: 800ms
                    _ => 0.3,
                };
                v.trigger(velocity, decay_time, params);
            }
            DrumVoice::Clap(v) => v.trigger(velocity, params),
            DrumVoice::Tom(v) => {
                let base_freq = TOM_FREQS.iter()
                    .find(|(i, _)| *i == slot)
                    .map(|(_, f)| *f)
                    .unwrap_or(150.0);
                v.trigger(velocity, base_freq, params);
            }
            DrumVoice::Click(v) => v.trigger(velocity, params),
        }
    }

    /// Returns (left, right) stereo pair.
    #[inline]
    pub fn tick(&mut self) -> (f32, f32) {
        if !self.enabled { return (0.0, 0.0); }

        // Process sequencer
        let triggers = self.sequencer.tick(self.sample_rate);
        self.step_atom.store(self.sequencer.current_step, Ordering::Relaxed);
        self.play_atom.store(self.sequencer.playing as u8, Ordering::Relaxed);
        self.rec_atom.store(self.sequencer.recording as u8, Ordering::Relaxed);
        for slot in 0..NUM_DRUM_SLOTS {
            let (fire, vel) = triggers[slot];
            if fire > 0 {
                self.trigger_slot(slot, vel as f32 / 127.0);
            }
        }

        // Mix all voices
        let mut out_l = 0.0_f32;
        let mut out_r = 0.0_f32;
        for i in 0..NUM_DRUM_SLOTS {
            let sample = self.voices[i].tick();
            if sample.abs() < 0.00001 { continue; }
            let level = self.params[i].level;
            let pan = self.params[i].pan;
            let s = sample * level;
            out_l += s * (0.5 - pan * 0.5).sqrt();
            out_r += s * (0.5 + pan * 0.5).sqrt();
        }
        let vol = self.volume * self.volume; // perceptual curve
        (out_l * vol, out_r * vol)
    }

    pub fn slot_name(slot: usize) -> &'static str {
        if slot < NUM_DRUM_SLOTS { DRUM_NAMES[slot] } else { "?" }
    }
}
