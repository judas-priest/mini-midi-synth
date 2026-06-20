//! Oscillator with multiple waveform types.

use std::f32::consts::PI;
use std::sync::Arc;

use super::bass::BassModel;
use super::epiano::ElectricPianoModel;
use super::piano::PianoModel;

const TAU: f32 = 2.0 * PI;

// ─── Plaits (Twist) integration ─────────────────────────────────────────────

/// Block size for Plaits rendering. 12 = native Plaits block; we use 32 to
/// amortize the overhead more cheaply. Must be ≤ TWIST_BLOCK.
const TWIST_BLOCK: usize = 32;

/// Newtype wrapping a Plaits Voice so we can derive Clone for Oscillator.
/// Cloning re-initialises the voice (state is not meaningful to copy).
struct TwistVoice(Box<mi_plaits_dsp::voice::Voice<'static>>);

impl TwistVoice {
    fn new(sample_rate: f32) -> Self {
        let mut v = Box::new(mi_plaits_dsp::voice::Voice::new(TWIST_BLOCK, sample_rate));
        v.init();
        TwistVoice(v)
    }
}

impl Clone for TwistVoice {
    fn clone(&self) -> Self {
        // Don't copy DSP state — create a fresh voice (Plaits always runs at 48kHz).
        Self::new(48000.0)
    }
}

// Make Oscillator cloneable even with TwistVoice inside.
// The Voice field lives on Oscillator, not inside OscState, so OscState
// stays #[derive(Clone)] cleanly.

/// PolyBLEP residual — call for each discontinuity point.
/// `t` = phase position (0..1), `dt` = phase increment per sample.
#[inline(always)]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        // Just past the discontinuity
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        // Just before the discontinuity
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OscType {
    Sine,          // 0
    Saw,           // 1
    Square,        // 2
    Triangle,      // 3
    Fm,            // 4
    Noise,         // 5
    KarplusStrong, // 6
    Organ,         // 7
    FmPiano,       // 8  — 4-operator FM piano (DX7-style)
    CommutedPiano, // 9  — commuted waveguide piano
    BandedWG,      // 10 — banded waveguide piano
    AdditivePiano, // 11 — additive synthesis piano (16 partials with per-partial decay)
    DrumSynth,     // 12 — dedicated drum synthesis (pitch env + noise env)
    BassGuitar,    // 13 — physical model bass guitar (finger/pick/slap)
    BowedString,   // 14 — bowed string physical model (violin/viola/cello/double bass)
    Brass,            // 15 — brass lip reed physical model (trumpet/horn/trombone/tuba)
    PhaseDistortion,  // 16 — Casio CZ-style phase distortion synthesis
    Wavefolder,       // 17 — West Coast wavefolding synthesis
    ModalResonator,   // 18 — modal resonator bank (Plaits-style)
    HardSync,         // 19 — oscillator hard sync (master resets slave)
    Supersaw,         // 20 — Roland JP-8000 style 7-voice supersaw
    PianoModel,       // 21 — inharmonic additive piano with dual-decay and hammer model
    Accordion,        // 22 — free reed physical model
    Saxophone,        // 23 — single reed waveguide
    ElectricPiano,    // 24 — Rhodes/Wurlitzer physical model (tine + pickup)
    Alias,            // 25 — intentionally aliasing 8-bit oscillator
    Window,           // 26 — windowed oscillator with formant control
    Wavetable,        // 27 — morphing wavetable (8 waveforms, sine→saw)
    Fm3,              // 28 — 3-operator FM (stacked: op3→op2→op1)
    Twist,            // 29 — Mutable Instruments Plaits (16 synthesis engines)
}

impl OscType {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            0 => Self::Sine,
            1 => Self::Saw,
            2 => Self::Square,
            3 => Self::Triangle,
            4 => Self::Fm,
            5 => Self::Noise,
            6 => Self::KarplusStrong,
            7 => Self::Organ,
            8 => Self::FmPiano,
            9 => Self::CommutedPiano,
            10 => Self::BandedWG,
            11 => Self::AdditivePiano,
            12 => Self::DrumSynth,
            13 => Self::BassGuitar,
            14 => Self::BowedString,
            15 => Self::Brass,
            16 => Self::PhaseDistortion,
            17 => Self::Wavefolder,
            18 => Self::ModalResonator,
            19 => Self::HardSync,
            20 => Self::Supersaw,
            21 => Self::PianoModel,
            22 => Self::Accordion,
            23 => Self::Saxophone,
            24 => Self::ElectricPiano,
            25 => Self::Alias,
            26 => Self::Window,
            27 => Self::Wavetable,
            28 => Self::Fm3,
            29 => Self::Twist,
            _ => Self::Sine,
        }
    }

    /// Returns true for simple waveform types that can be used as Osc 2/3.
    pub fn is_simple(self) -> bool {
        matches!(self, Self::Sine | Self::Saw | Self::Square | Self::Triangle | Self::Fm)
    }
}

/// Hammond organ drawbar harmonic ratios (16', 5⅓', 8', 4', 2⅔', 2', 1⅗', 1⅓', 1').
const ORGAN_HARMONICS: [f32; 9] = [0.5, 1.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];

/// Per-oscillator-type runtime state. Only the active variant is allocated.
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum OscState {
    Simple {
        phase: f32,
    },
    Fm {
        phase: f32,
        mod_phase: f32,
    },
    Noise,
    KarplusStrong {
        buffer: Vec<f32>,
        pos: usize,
        filter_state: f32,
        allpass_prev_in: f32,
        allpass_prev_out: f32,
        allpass_coeff: f32,
    },
    Organ {
        phases: [f32; 9],
    },
    FmPiano {
        phases: [f32; 4],
        time: f32,
    },
    CommutedPiano {
        buffer: Vec<f32>,
        buffer2: Vec<f32>,
        pos: usize,
        pos2: usize,
        // Per-string loss filters (one-pole, frequency-dependent)
        loss_state1: f32,
        loss_state2: f32,
        loss_coeff: f32,       // base loss coefficient (brightness + freq dependent)
        // Fractional delay allpass per string
        frac_z1: [f32; 2],
        frac_coeff: [f32; 2],
        // Dispersion filter: 3 second-order allpass sections for inharmonicity
        disp_z1: [f32; 3],
        disp_z2: [f32; 3],
        _disp_b0: [f32; 3],    // allpass coefficients (used in init)
        _disp_b1: [f32; 3],
        disp_a1: [f32; 3],
        // Soundboard resonance (2-pole bandpass)
        sb_lp: f32,
        sb_bp: f32,
        sb_freq: f32,          // soundboard resonance frequency
        sb_q: f32,
        // DC blocker
        dc_x: f32,
        dc_y: f32,
    },
    BandedWG {
        buffers: [Vec<f32>; 4],
        positions: [usize; 4],
        filter_states: [f32; 4],
        gains: [f32; 4],
    },
    AdditivePiano {
        phases: [f32; 16],
        freqs: [f32; 16],
        init_amps: [f32; 16],
        decay_rates: [f32; 16],
        time: f32,
        noise_amp: f32,
    },
    DrumSynth {
        phase: f32,
        time: f32,
        noise_state: u32,
        base_freq: f32,
        noise_lp_state: f32,
        // Config (set during init)
        pitch_amount: f32,  // semitones of pitch sweep
        pitch_decay: f32,   // pitch envelope decay rate
        noise_level: f32,   // noise amplitude (0-1)
        noise_decay: f32,   // noise envelope decay rate
        noise_color: f32,   // noise brightness (0=dark, 1=bright)
    },
    BassGuitar(BassModel),
    BowedString {
        nut_delay: Vec<f32>,
        bridge_delay: Vec<f32>,
        nut_pos: usize,
        bridge_pos: usize,
        nut_filter: f32,
        bridge_lp: f32,
        bridge_bp: f32,
        allpass_prev_in: f32,
        allpass_prev_out: f32,
        allpass_coeff: f32,
        bow_velocity: f32,
        _bow_position: f32,
        bow_pressure: f32,
        body_freq: f32,
        body_q: f32,
    },
    Brass {
        bore_delay: Vec<f32>,
        bore_pos: usize,
        lip_x: f32,
        lip_v: f32,
        lip_freq: f32,
        lip_damping: f32,
        lip_mass_inv: f32,
        bell_lp_state: f32,
        bell_hp_state: f32,
        bell_cutoff: f32,
        bore_lp_state: f32,
        blowing_pressure: f32,
        _bore_length_ratio: f32,
    },
    PhaseDistortion {
        phase: f32,
        pd_shape: u8,
        pd_depth: f32,
    },
    Wavefolder {
        phase: f32,
        prev_input: f32,
        prev_adf: f32,
        fold_amount: f32,
        fold_symmetry: f32,
        fold_source: u8,
    },
    ModalResonator {
        y1: [f32; 16],
        y2: [f32; 16],
        cosw: [f32; 16],
        r_sq: [f32; 16],
        amps: [f32; 16],
        excitation_time: f32,
        noise_state: u32,
    },
    HardSync {
        master_phase: f32,
        slave_phase: f32,
        sync_ratio: f32,      // slave/master frequency ratio (1-16)
        sync_shape: u8,       // 0=Saw, 1=Square, 2=Triangle
    },
    Supersaw {
        phases: [f32; 7],     // 7 detuned saws
        detunes: [f32; 7],    // detune factors (precomputed)
        mix_center: f32,      // center saw level
        mix_side: f32,        // side saws level
    },
    PianoModel(PianoModel),   // 24-partial inharmonic additive with dual decay
    ElectricPiano(ElectricPianoModel), // Rhodes/Wurlitzer tine model
    Accordion {
        // Phase-driven free reed model
        reed_x: f32,          // phase (0..1)
        reed_v: f32,          // phase increment per sample
        reed_damping: f32,    // bellows pressure (drive amplitude)
        // Second reed register
        reed2_x: f32,
        reed2_v: f32,
        // Register mix
        register_mix: f32,    // 0=fundamental only, 1=full register
        // Tone shaping
        lp_state: f32,
        dc_x: f32,
        dc_y: f32,
    },
    Saxophone {
        // Single delay line (STK-style Saxofony)
        delay: Vec<f32>,
        delay_pos: usize,
        // Reed model
        reed_stiffness: f32,
        reed_table_offset: f32,
        blowing_pressure: f32,
        // Bell reflection filter (one-pole LP)
        bell_lp: f32,
        bell_coeff: f32,  // LP coefficient for bell reflection
        // DC blocker
        dc_x: f32,
        dc_y: f32,
    },
    Alias {
        phase: u32,       // 8.24 fixed-point phase accumulator
        crush_bits: f32,  // bit depth (1-8)
        wave_type: u8,    // 0=sine, 1=ramp, 2=pulse, 3=noise, 4=additive
        noise_state: u8,
        table: [u8; 256], // precomputed wavetable
    },
    Window {
        phase: f32,
        window_type: u8,  // 0=triangle, 1=cosine, 2=half-sine, 3=hann
        morph: f32,       // blend between adjacent windows (0-1)
        formant: f32,     // formant shift in semitones
    },
    Wavetable {
        phase: f32,                // 0..1 oscillator phase
        table: Arc<Vec<f32>>,      // wt_frames * wt_frame_size samples; shared (cheap clone on note-on)
        wt_frames: usize,
        wt_frame_size: usize,
    },
    Fm3 {
        carrier_phase: f32,  // op1 (output) phase
        mod1_phase: f32,     // op2 (modulates carrier) phase
        mod2_phase: f32,     // op3 (modulates op2) phase
    },
    /// Plaits (Twist): ring-buffer of pre-rendered samples from block processing.
    Twist {
        buf_out: [f32; TWIST_BLOCK],
        buf_aux: [f32; TWIST_BLOCK],
        pos: usize,    // read position in buf
        note: f32,     // current MIDI note (for re-render check)
    },
}

/// Maximum delay line length: covers MIDI note 21 (A0 ≈ 27.5 Hz) at 192 kHz.
const MAX_DELAY: usize = 7000;

#[derive(Clone)]
pub struct Oscillator {
    pub osc_type: OscType,
    pub sample_rate: f32,
    detune: f32,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub ks_brightness: f32,
    pub ks_feedback: f32,
    pub organ_drawbars: [f32; 9],
    pub pulse_width: f32,
    pub wavetable_morph: f32,
    noise_state: u32,
    state: OscState,
    /// Pre-allocated delay line buffers — reused across note-ons to avoid RT allocation.
    pool_a: Vec<f32>,
    pool_b: Vec<f32>,
    pool_c: Vec<f32>,
    pool_d: Vec<f32>,
    // ── Plaits (Twist) parameters ──────────────────────────────────────────
    pub twist_engine:    u32,   // 0-23 engine index
    pub twist_harmonics: f32,   // 0..1
    pub twist_timbre:    f32,   // 0..1
    pub twist_morph:     f32,   // 0..1
    pub twist_lpg_decay: f32,   // 0..1 (LPG envelope decay)
    pub twist_lpg_colour:f32,   // 0..1 (LPG LP/VCA colour)
    pub twist_aux_mix:   f32,   // 0..1 (blend main ↔ aux output)
    twist_voice: Option<TwistVoice>,
}

impl Oscillator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            osc_type: OscType::Sine,
            sample_rate,
            detune: 0.0,
            fm_ratio: 3.5,
            fm_index: 5.0,
            ks_brightness: 0.5,
            ks_feedback: 0.996,
            organ_drawbars: [0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            pulse_width: 0.5,
            wavetable_morph: 0.0,
            noise_state: 0x12345678,
            state: OscState::Simple { phase: 0.0 },
            pool_a: Vec::with_capacity(MAX_DELAY),
            pool_b: Vec::with_capacity(MAX_DELAY),
            pool_c: Vec::with_capacity(MAX_DELAY),
            pool_d: Vec::with_capacity(MAX_DELAY),
            twist_engine: 0,
            twist_harmonics: 0.5,
            twist_timbre: 0.5,
            twist_morph: 0.5,
            twist_lpg_decay: 0.5,
            twist_lpg_colour: 0.5,
            twist_aux_mix: 0.0,
            twist_voice: None,
        }
    }

    /// Take a buffer from pool, resized to `n` and cleared.
    /// The pool Vec is left empty (moved out); caller must put it into OscState.
    fn take_buf(pool: &mut Vec<f32>, n: usize) -> Vec<f32> {
        let mut buf = std::mem::take(pool);
        buf.resize(n, 0.0);
        buf.fill(0.0);
        buf
    }

    /// Return a buffer back to a pool slot (for reuse on next note-on).
    fn return_buf(pool: &mut Vec<f32>, buf: Vec<f32>) {
        *pool = buf;
    }

    pub fn set_detune(&mut self, detune: f32) {
        self.detune = detune;
    }

    /// Update phase distortion depth (for envelope modulation).
    pub fn set_pd_depth(&mut self, depth: f32) {
        if let OscState::PhaseDistortion { pd_depth, .. } = &mut self.state {
            *pd_depth = depth;
        }
    }

    /// Reclaim any Vec buffers from current state back to pools before switching.
    fn reclaim_buffers(&mut self) {
        let old = std::mem::replace(&mut self.state, OscState::Simple { phase: 0.0 });
        match old {
            OscState::KarplusStrong { buffer, .. } => {
                Self::return_buf(&mut self.pool_a, buffer);
            }
            OscState::CommutedPiano { buffer, buffer2, .. } => {
                Self::return_buf(&mut self.pool_a, buffer);
                Self::return_buf(&mut self.pool_b, buffer2);
            }
            OscState::BandedWG { buffers, .. } => {
                let [b0, b1, b2, b3] = buffers;
                Self::return_buf(&mut self.pool_a, b0);
                Self::return_buf(&mut self.pool_b, b1);
                Self::return_buf(&mut self.pool_c, b2);
                Self::return_buf(&mut self.pool_d, b3);
            }
            OscState::BassGuitar(model) => {
                model.return_buffers(&mut self.pool_a, &mut self.pool_b);
            }
            OscState::BowedString { nut_delay, bridge_delay, .. } => {
                Self::return_buf(&mut self.pool_a, nut_delay);
                Self::return_buf(&mut self.pool_b, bridge_delay);
            }
            OscState::Brass { bore_delay, .. } => {
                Self::return_buf(&mut self.pool_a, bore_delay);
            }
            OscState::Saxophone { delay, .. } => {
                Self::return_buf(&mut self.pool_a, delay);
            }
            // Wavetable storage is Arc-shared; let Drop free it when the
            // last reference goes away (cannot recycle into the pool).
            OscState::Wavetable { .. } => {}
            _ => {}
        }
    }

    pub fn reset(&mut self) {
        self.reclaim_buffers();
        self.state = match self.osc_type {
            OscType::Sine | OscType::Saw | OscType::Square | OscType::Triangle => {
                OscState::Simple { phase: 0.0 }
            }
            OscType::Fm => OscState::Fm {
                phase: 0.0,
                mod_phase: 0.0,
            },
            OscType::Noise => OscState::Noise,
            OscType::KarplusStrong => OscState::KarplusStrong {
                buffer: Vec::new(),
                pos: 0,
                filter_state: 0.0,
                allpass_prev_in: 0.0,
                allpass_prev_out: 0.0,
                allpass_coeff: 0.0,
            },
            OscType::Organ => OscState::Organ {
                phases: [0.0; 9],
            },
            OscType::FmPiano => OscState::FmPiano {
                phases: [0.0; 4],
                time: 0.0,
            },
            OscType::CommutedPiano => OscState::CommutedPiano {
                buffer: Vec::new(),
                buffer2: Vec::new(),
                pos: 0,
                pos2: 0,
                loss_state1: 0.0,
                loss_state2: 0.0,
                loss_coeff: 0.0,
                frac_z1: [0.0; 2],
                frac_coeff: [0.0; 2],
                disp_z1: [0.0; 3],
                disp_z2: [0.0; 3],
                _disp_b0: [0.0; 3],
                _disp_b1: [0.0; 3],
                disp_a1: [0.0; 3],
                sb_lp: 0.0,
                sb_bp: 0.0,
                sb_freq: 0.0,
                sb_q: 0.0,
                dc_x: 0.0,
                dc_y: 0.0,
            },
            OscType::BandedWG => OscState::BandedWG {
                buffers: Default::default(),
                positions: [0; 4],
                filter_states: [0.0; 4],
                gains: [0.53, 0.27, 0.13, 0.07],
            },
            OscType::AdditivePiano => OscState::AdditivePiano {
                phases: [0.0; 16],
                freqs: [0.0; 16],
                init_amps: [0.0; 16],
                decay_rates: [0.0; 16],
                time: 0.0,
                noise_amp: 0.0,
            },
            OscType::BassGuitar => OscState::BassGuitar(BassModel::new()),
            OscType::DrumSynth => OscState::DrumSynth {
                phase: 0.0,
                time: 0.0,
                noise_state: self.noise_state,
                base_freq: 440.0,
                noise_lp_state: 0.0,
                pitch_amount: 0.0,
                pitch_decay: 40.0,
                noise_level: 0.0,
                noise_decay: 40.0,
                noise_color: 0.5,
            },
            OscType::BowedString => OscState::BowedString {
                nut_delay: Vec::new(),
                bridge_delay: Vec::new(),
                nut_pos: 0,
                bridge_pos: 0,
                nut_filter: 0.0,
                bridge_lp: 0.0,
                bridge_bp: 0.0,
                allpass_prev_in: 0.0,
                allpass_prev_out: 0.0,
                allpass_coeff: 0.0,
                bow_velocity: 0.0,
                _bow_position: 0.12,
                bow_pressure: 0.5,
                body_freq: 300.0,
                body_q: 2.0,
            },
            OscType::Brass => OscState::Brass {
                bore_delay: Vec::new(),
                bore_pos: 0,
                lip_x: 0.0,
                lip_v: 0.0,
                lip_freq: 440.0,
                lip_damping: 0.5,
                lip_mass_inv: 1.0,
                bell_lp_state: 0.0,
                bell_hp_state: 0.0,
                bell_cutoff: 2000.0,
                bore_lp_state: 0.0,
                blowing_pressure: 0.5,
                _bore_length_ratio: 1.0,
            },
            OscType::PhaseDistortion => OscState::PhaseDistortion {
                phase: 0.0,
                pd_shape: 0,
                pd_depth: 0.0,
            },
            OscType::Wavefolder => OscState::Wavefolder {
                phase: 0.0,
                prev_input: 0.0,
                prev_adf: 0.0,
                fold_amount: 0.5,
                fold_symmetry: 0.5,
                fold_source: 0,
            },
            OscType::ModalResonator => OscState::ModalResonator {
                y1: [0.0; 16],
                y2: [0.0; 16],
                cosw: [0.0; 16],
                r_sq: [0.0; 16],
                amps: [0.0; 16],
                excitation_time: 0.0,
                noise_state: self.noise_state,
            },
            OscType::HardSync => OscState::HardSync {
                master_phase: 0.0,
                slave_phase: 0.0,
                sync_ratio: 2.0,
                sync_shape: 0,
            },
            OscType::Supersaw => OscState::Supersaw {
                phases: [0.0; 7],
                detunes: [1.0; 7],
                mix_center: 1.0,
                mix_side: 0.75,
            },
            OscType::PianoModel => OscState::PianoModel(PianoModel::new()),
            OscType::ElectricPiano => OscState::ElectricPiano(ElectricPianoModel::new()),
            OscType::Accordion => OscState::Accordion {
                reed_x: 0.0, reed_v: 0.0, reed_damping: 0.0,
                reed2_x: 0.0, reed2_v: 0.0,
                register_mix: 0.0,
                lp_state: 0.0, dc_x: 0.0, dc_y: 0.0,
            },
            OscType::Saxophone => OscState::Saxophone {
                delay: Vec::new(), delay_pos: 0,
                reed_stiffness: 0.0, reed_table_offset: 0.0, blowing_pressure: 0.0,
                bell_lp: 0.0, bell_coeff: 0.0,
                dc_x: 0.0, dc_y: 0.0,
            },
            OscType::Alias => OscState::Alias {
                phase: 0, crush_bits: 8.0, wave_type: 0, noise_state: 0x42,
                table: [0; 256],
            },
            OscType::Window => OscState::Window {
                phase: 0.0, window_type: 0, morph: 0.0, formant: 0.0,
            },
            OscType::Wavetable => OscState::Wavetable {
                phase: 0.0,
                table: Arc::new(Vec::new()),
                wt_frames: Self::WT_COUNT,
                wt_frame_size: Self::WT_SIZE,
            },
            OscType::Fm3 => OscState::Fm3 {
                carrier_phase: 0.0, mod1_phase: 0.0, mod2_phase: 0.0,
            },
            OscType::Twist => OscState::Twist {
                buf_out: [0.0; TWIST_BLOCK],
                buf_aux: [0.0; TWIST_BLOCK],
                pos: TWIST_BLOCK, // force render on first tick
                note: 60.0,
            },
        };
    }

    /// Initialize Karplus-Strong delay line for a given frequency.
    pub fn init_ks(&mut self, freq: f32) {
        self.reclaim_buffers();
        let delay_total = self.sample_rate / freq;
        let n = (delay_total as usize).max(2);
        let frac = delay_total - n as f32;
        let allpass_coeff = (1.0 - frac) / (1.0 + frac);

        let mut buffer = Self::take_buf(&mut self.pool_a, n);
        let mut state = self.noise_state;
        for s in buffer.iter_mut() {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *s = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
        }
        self.noise_state = state;

        self.state = OscState::KarplusStrong {
            buffer,
            pos: 0,
            filter_state: 0.0,
            allpass_prev_in: 0.0,
            allpass_prev_out: 0.0,
            allpass_coeff,
        };
    }

    /// Initialize Commuted Piano: dual detuned waveguides with hammer model and dispersion.
    /// Initialize Commuted Piano with velocity-dependent hammer, dispersion, and soundboard.
    ///
    /// Physical model based on Smith/Van Duyne commuted synthesis with:
    /// - Velocity-dependent hammer hardness (F = K*x^p, softer = darker)
    /// - Allpass dispersion filter for string inharmonicity
    /// - Dual detuned strings for double-decay envelope
    /// - Frequency-dependent loss filter
    /// - Soundboard resonance coloring
    pub fn init_commuted_piano(&mut self, freq: f32, velocity: f32) {
        self.reclaim_buffers();
        let sr = self.sample_rate;
        let brightness = self.ks_brightness;
        let feedback = self.ks_feedback;

        // ── Note parameters ──
        let midi_note = (12.0 * (freq / 440.0).log2() + 69.0).clamp(21.0, 108.0);
        let normalized = (midi_note - 21.0) / 87.0; // 0=A0, 1=C8

        // ── Inharmonicity coefficient B (measured piano values) ──
        // B grows exponentially from bass to treble
        let b_coeff = 0.00012 * (7.0 * normalized).exp();

        // ── Dispersion filter design ──
        // 3 second-order allpass sections to approximate frequency-dependent phase delay
        // from string stiffness. Based on Rauhala & Välimäki (2006).
        let mut disp_b0 = [0.0f32; 3];
        let mut disp_b1 = [0.0f32; 3];
        let mut disp_a1 = [0.0f32; 3];
        {
            // Total phase delay at Nyquist should match B coefficient
            let phase_delay_total = b_coeff * 200.0; // empirical scaling
            for i in 0..3 {
                // Distribute across sections with different center frequencies
                let section_delay = phase_delay_total / 3.0;
                // Thiran-style allpass: a1 ≈ (1-d)/(1+d) where d is fractional delay
                let d = section_delay * (1.0 + i as f32 * 0.5);
                let a1 = (1.0 - d) / (1.0 + d);
                disp_b0[i] = a1;
                disp_b1[i] = 1.0;
                disp_a1[i] = a1;
            }
        }

        // ── Delay line lengths (compensate for dispersion filter delay) ──
        let disp_delay_comp = b_coeff * 100.0; // samples of extra delay from dispersion
        let delay1_f = sr / freq - disp_delay_comp;
        let n1 = (delay1_f as usize).max(4);
        let frac1 = delay1_f - n1 as f32;

        // String 2: detuned for double-decay (Weinreich coupling)
        // Bass: ~1.5 cents, treble: ~0.5 cents
        let detune_cents = 1.5 - 1.0 * normalized;
        let freq2 = freq * 2.0_f32.powf(detune_cents / 1200.0);
        let delay2_f = sr / freq2 - disp_delay_comp;
        let n2 = (delay2_f as usize).max(4);
        let frac2 = delay2_f - n2 as f32;

        // Fractional delay allpass coefficients (Thiran first-order)
        let frac_coeff = [
            (1.0 - frac1) / (1.0 + frac1),
            (1.0 - frac2) / (1.0 + frac2),
        ];

        // ── Loss filter coefficient ──
        // Higher notes decay faster, lower brightness = more damping
        let base_loss = feedback; // from patch (ks_feedback)
        let freq_loss = 1.0 - (freq / (sr * 0.5)).min(0.3); // reduce at high freq
        let loss_coeff = (base_loss * freq_loss).clamp(0.8, 0.9999);

        // ── Hammer excitation ──
        // Velocity-dependent: harder hit = shorter, brighter pulse
        // Hammer hardness exponent: p ranges from ~2 (bass, soft) to ~4 (treble, hard)
        let p_base = 2.0 + 2.0 * normalized; // bass=2, treble=4
        let p_vel = p_base * (0.5 + 0.5 * velocity); // soft hit reduces hardness

        // Hammer contact time: inversely proportional to velocity^((p-1)/2p)
        // Softer hit = longer contact = darker spectrum
        let hammer_base_ms = 4.0 - 2.5 * normalized; // bass: 4ms, treble: 1.5ms
        let vel_factor = (0.2 + 0.8 * velocity).powf((p_vel - 1.0) / (2.0 * p_vel));
        let hammer_ms = hammer_base_ms / vel_factor;
        let hammer_samples = ((hammer_ms * 0.001 * sr) as usize).clamp(4, n1.min(n2) / 2);

        // Strike position: ~1/8 of string length (suppresses 8th harmonic)
        let strike_pos = 1.0 / (7.0 + normalized * 2.0); // moves toward bridge for treble

        // Generate hammer force pulse with soundboard coloring
        let mut buffer = Self::take_buf(&mut self.pool_a, n1);
        let mut buffer2 = Self::take_buf(&mut self.pool_b, n2);

        let mut state = self.noise_state;
        let hammer_amp = 0.3 + 0.5 * velocity; // louder with velocity

        for i in 0..hammer_samples {
            let t = i as f32 / hammer_samples as f32;

            // Raised cosine window (Hann) for the force pulse
            let window = 0.5 * (1.0 - (TAU * t).cos());

            // Add some noise for attack transient (hammer felt noise)
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let noise_amount = 0.15 + 0.25 * velocity; // more noise at higher velocity

            // Force pulse: windowed sine half-period + noise
            let pulse = (PI * t).sin(); // half-sine, like real hammer force
            let sample = (pulse * (1.0 - noise_amount) + noise * noise_amount) * window * hammer_amp;

            // Apply strike position comb filter (notch at harmonics that have a node there)
            // This shapes the excitation spectrum
            let comb_delay = (strike_pos * n1 as f32) as usize;
            if i < n1 {
                buffer[i] += sample;
                // Comb filter: subtract delayed copy
                if i >= comb_delay {
                    buffer[i] -= buffer[i - comb_delay] * 0.3;
                }
            }
            let comb_delay2 = (strike_pos * n2 as f32) as usize;
            if i < n2 {
                buffer2[i] += sample;
                if i >= comb_delay2 {
                    buffer2[i] -= buffer2[i - comb_delay2] * 0.3;
                }
            }
        }

        // ── Brightness filter on excitation (velocity-dependent) ──
        // Softer hits get LP-filtered excitation
        let lp_coeff_excite = 0.3 + 0.7 * velocity * brightness;
        let mut prev = 0.0f32;
        for s in buffer.iter_mut() {
            prev += lp_coeff_excite * (*s - prev);
            *s = prev;
        }
        prev = 0.0;
        for s in buffer2.iter_mut() {
            prev += lp_coeff_excite * (*s - prev);
            *s = prev;
        }

        self.noise_state = state;

        // ── Soundboard resonance parameters ──
        // Soundboard has dense modes 50-200Hz, radiation cutoff ~100Hz
        // Simple model: resonant peak at a frequency that depends on piano size
        let sb_freq = 120.0 + 80.0 * brightness; // 120-200 Hz range
        let sb_q = 1.5 + brightness; // Q = 1.5-2.5

        self.state = OscState::CommutedPiano {
            buffer,
            buffer2,
            pos: 0,
            pos2: 0,
            loss_state1: 0.0,
            loss_state2: 0.0,
            loss_coeff,
            frac_z1: [0.0; 2],
            frac_coeff,
            disp_z1: [0.0; 3],
            disp_z2: [0.0; 3],
            _disp_b0: disp_b0,
            _disp_b1: disp_b1,
            disp_a1,
            sb_lp: 0.0,
            sb_bp: 0.0,
            sb_freq,
            sb_q,
            dc_x: 0.0,
            dc_y: 0.0,
        };
    }

    /// Initialize Banded Waveguide: 4 parallel delay lines at inharmonic partials.
    pub fn init_banded_wg(&mut self, freq: f32) {
        self.reclaim_buffers();
        let inharmonicity = 0.0003;
        let mut buffers: [Vec<f32>; 4] = [
            std::mem::take(&mut self.pool_a),
            std::mem::take(&mut self.pool_b),
            std::mem::take(&mut self.pool_c),
            std::mem::take(&mut self.pool_d),
        ];
        let mut positions = [0usize; 4];
        let mut filter_states = [0.0f32; 4];

        let mut state = self.noise_state;
        for i in 0..4 {
            let partial = (i + 1) as f32;
            let partial_freq = freq * partial * (1.0 + inharmonicity * partial * partial).sqrt();
            let delay = (self.sample_rate / partial_freq) as usize;
            let delay = delay.max(2);

            buffers[i].resize(delay, 0.0);

            let excite_len = ((0.002 * self.sample_rate) as usize).clamp(2, delay);
            let amp_scale = 1.0 / partial;
            for (j, buf_sample) in buffers[i].iter_mut().enumerate().take(delay) {
                if j < excite_len {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    let noise = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                    let w = (PI * j as f32 / excite_len as f32).sin();
                    *buf_sample = noise * w * amp_scale;
                } else {
                    *buf_sample = 0.0;
                }
            }

            positions[i] = 0;
            filter_states[i] = 0.0;
        }
        self.noise_state = state;

        self.state = OscState::BandedWG {
            buffers,
            positions,
            filter_states,
            gains: [0.53, 0.27, 0.13, 0.07],
        };
    }

    /// Initialize Additive Piano: 16 partials with per-partial decay and inharmonicity.
    pub fn init_additive_piano(&mut self, freq: f32) {
        let midi_note = (12.0 * (freq / 440.0).log2() + 69.0).clamp(21.0, 108.0);
        let normalized = (midi_note - 21.0) / 87.0;

        let brightness = self.ks_brightness;

        let inharm_base = 0.0001 * (3.5 * normalized).exp();
        let b = inharm_base * (1.0 + 3.0 * (1.0 - brightness));

        let sustain_factor = 1.0 / (1.0 - self.ks_feedback).max(0.001);
        let decay_mult = 100.0 / sustain_factor;
        let base_decay = (0.15 + 0.4 * normalized) * decay_mult;

        const GRAND_AMPS: [f32; 16] = [
            1.00, 0.85, 0.70, 0.55, 0.40, 0.30, 0.22, 0.05, 0.14, 0.11, 0.08, 0.06, 0.05, 0.04,
            0.03, 0.02,
        ];
        const UPRIGHT_AMPS: [f32; 16] = [
            1.00, 0.65, 0.40, 0.28, 0.15, 0.10, 0.06, 0.02, 0.04, 0.03, 0.02, 0.01, 0.01,
            0.005, 0.003, 0.002,
        ];

        let mut phases = [0.0f32; 16];
        let mut freqs = [0.0f32; 16];
        let mut init_amps = [0.0f32; 16];
        let mut decay_rates = [0.0f32; 16];

        for n in 0..16 {
            let partial = (n + 1) as f32;
            freqs[n] = freq * partial * (1.0 + b * partial * partial).sqrt();

            let t = brightness.clamp(0.0, 1.0);
            init_amps[n] = UPRIGHT_AMPS[n] * (1.0 - t) + GRAND_AMPS[n] * t;

            let partial_freq = freqs[n];
            let freq_decay = 2.0 * (partial_freq / 4000.0).powi(2);
            let tilt = 1.0 + 2.0 * (1.0 - brightness);
            decay_rates[n] = base_decay + freq_decay * tilt;

            phases[n] = 0.0;
        }

        let noise_amp = 0.04 + brightness * 0.18;

        self.state = OscState::AdditivePiano {
            phases,
            freqs,
            init_amps,
            decay_rates,
            time: 0.0,
            noise_amp,
        };
    }

    /// Initialize DrumSynth with independent pitch and noise envelopes.
    pub fn init_drum_synth(
        &mut self,
        freq: f32,
        pitch_amount: f32,
        pitch_decay: f32,
        noise_level: f32,
        noise_decay: f32,
        noise_color: f32,
    ) {
        self.state = OscState::DrumSynth {
            phase: 0.0,
            time: 0.0,
            noise_state: self.noise_state,
            base_freq: freq,
            noise_lp_state: 0.0,
            pitch_amount,
            pitch_decay,
            noise_level,
            noise_decay,
            noise_color,
        };
    }

    /// Generate next noise sample using xorshift32.
    #[inline]
    pub fn next_noise(&mut self) -> f32 {
        self.noise_state ^= self.noise_state << 13;
        self.noise_state ^= self.noise_state >> 17;
        self.noise_state ^= self.noise_state << 5;
        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    pub fn tick(&mut self, freq: f32) -> f32 {
        match self.osc_type {
            OscType::KarplusStrong => self.tick_ks(),
            OscType::Organ => self.tick_organ(freq),
            OscType::Noise => self.next_noise(),
            OscType::FmPiano => self.tick_fm_piano(freq),
            OscType::CommutedPiano => self.tick_commuted_piano(),
            OscType::BandedWG => self.tick_banded_wg(),
            OscType::AdditivePiano => self.tick_additive_piano(),
            OscType::DrumSynth => self.tick_drum_synth(),
            OscType::BassGuitar => self.tick_bass_guitar(),
            OscType::BowedString => self.tick_bowed_string(),
            OscType::Brass => self.tick_brass(),
            OscType::PhaseDistortion => self.tick_phase_distortion(freq),
            OscType::Wavefolder => self.tick_wavefolder(freq),
            OscType::ModalResonator => self.tick_modal_resonator(),
            OscType::HardSync => self.tick_hard_sync(freq),
            OscType::Supersaw => self.tick_supersaw(freq),
            OscType::PianoModel => self.tick_piano_model(),
            OscType::ElectricPiano => self.tick_electric_piano(),
            OscType::Accordion => self.tick_accordion(),
            OscType::Saxophone => self.tick_saxophone(),
            OscType::Alias => self.tick_alias(freq),
            OscType::Window => self.tick_window(freq),
            OscType::Wavetable => self.tick_wavetable(freq),
            OscType::Fm3 => self.tick_fm3(freq),
            OscType::Twist => self.tick_twist(freq),
            _ => self.tick_standard(freq),
        }
    }

    fn tick_standard(&mut self, freq: f32) -> f32 {
        let freq = freq * (1.0 + self.detune);
        let dt = freq / self.sample_rate;

        match &mut self.state {
            OscState::Simple { phase } => {
                let out = match self.osc_type {
                    OscType::Sine => (*phase * TAU).sin(),
                    OscType::Saw => {
                        let mut out = 2.0 * *phase - 1.0;
                        out -= poly_blep(*phase, dt);
                        out
                    }
                    OscType::Square => {
                        let pw = self.pulse_width;
                        let mut out = if *phase < pw { 1.0 } else { -1.0 };
                        out += poly_blep(*phase, dt);
                        let mut shifted = *phase - pw;
                        if shifted < 0.0 { shifted += 1.0; }
                        out -= poly_blep(shifted, dt);
                        out
                    }
                    OscType::Triangle => {
                        if *phase < 0.5 {
                            4.0 * *phase - 1.0
                        } else {
                            3.0 - 4.0 * *phase
                        }
                    }
                    _ => 0.0,
                };
                *phase += dt;
                *phase -= phase.floor();
                out
            }
            OscState::Fm { phase, mod_phase } => {
                let fm_index = self.fm_index;
                let fm_ratio = self.fm_ratio;
                let sample_rate = self.sample_rate;
                let modulator = (*mod_phase * TAU).sin();
                let out = ((*phase + fm_index * modulator) * TAU).sin();
                *mod_phase += freq * fm_ratio / sample_rate;
                *mod_phase -= mod_phase.floor();
                *phase += dt;
                *phase -= phase.floor();
                out
            }
            _ => 0.0,
        }
    }

    fn tick_ks(&mut self) -> f32 {
        let brightness = self.ks_brightness;
        let feedback = self.ks_feedback;

        if let OscState::KarplusStrong {
            buffer,
            pos,
            filter_state,
            allpass_prev_in,
            allpass_prev_out,
            allpass_coeff,
        } = &mut self.state
        {
            if buffer.is_empty() {
                return 0.0;
            }

            let sample = buffer[*pos];

            let filtered = *filter_state + brightness * (sample - *filter_state);
            *filter_state = filtered;

            let c = *allpass_coeff;
            let ap_out = c * (filtered - *allpass_prev_out) + *allpass_prev_in;
            *allpass_prev_in = filtered;
            *allpass_prev_out = ap_out;

            buffer[*pos] = ap_out * feedback;
            *pos = (*pos + 1) % buffer.len();

            sample
        } else {
            0.0
        }
    }

    fn tick_organ(&mut self, freq: f32) -> f32 {
        let drawbars = self.organ_drawbars;
        let sample_rate = self.sample_rate;

        if let OscState::Organ { phases } = &mut self.state {
            let mut out = 0.0;
            let normalize = 1.0 / 8.0;

            for i in 0..9 {
                let level = drawbars[i];
                if level < 0.01 {
                    continue;
                }

                let harmonic_freq = freq * ORGAN_HARMONICS[i];
                let dt = harmonic_freq / sample_rate;
                out += (phases[i] * TAU).sin() * level * normalize;
                phases[i] += dt;
                if phases[i] >= 1.0 {
                    phases[i] -= 1.0;
                }
            }

            out
        } else {
            0.0
        }
    }

    /// 4-operator FM Piano (DX7 Algorithm 5 — E.PIANO 1 style).
    fn tick_fm_piano(&mut self, freq: f32) -> f32 {
        let sample_rate = self.sample_rate;
        let detune = self.detune;
        let fm_index = self.fm_index;
        let fm_ratio = self.fm_ratio;

        if let OscState::FmPiano { phases, time } = &mut self.state {
            let dt = 1.0 / sample_rate;
            *time += dt;
            let t = *time;

            let f = freq * (1.0 + detune);

            // DX7-style 3-operator cascade FM: Op3 → Op2 → Op1 (carrier)
            // Op3: high-frequency modulator (attack brightness)
            // Op2: mid-frequency modulator (body/harmonic content)
            // Op1: carrier at fundamental
            // Plus Op4: feedback modulator for metallic character

            // Op4: self-feedback operator (adds metallic inharmonicity)
            let fb_level = 0.3 * fast_exp(-t * 2.0);
            let op4_fb = (phases[3] * TAU).sin();
            let op4 = ((phases[3] + fb_level * op4_fb) * TAU).sin();

            // Op3: high-ratio modulator — attack transient (decays fast)
            let op3_idx = fm_index * 0.7 * fast_exp(-t * 15.0);
            let op3 = ((phases[2] + op3_idx * 0.1 * op4) * TAU).sin();

            // Op2: mid-ratio modulator — body harmonics (slower decay)
            let op2_idx = fm_index * 0.4 * fast_exp(-t * 1.5) + 0.08;
            let op2 = ((phases[1] + op2_idx * op3) * TAU).sin();

            // Op1: carrier at fundamental — modulated by Op2 cascade
            let op1_idx = fm_index * 0.5 * fast_exp(-t * 2.0) + 0.15;
            let op1 = ((phases[0] + op1_idx * op2) * TAU).sin();

            // Advance phases: carrier 1×, modulator at fm_ratio, high mod at 14×, feedback at 1×
            phases[0] += f / sample_rate;
            phases[1] += f * fm_ratio / sample_rate;
            phases[2] += f * 14.0 / sample_rate;
            phases[3] += f * 1.0 / sample_rate; // feedback op at 1×

            for p in phases.iter_mut() {
                *p -= p.floor();
            }

            let out = op1 * 0.85 + op4 * 0.08; // mostly carrier + tiny metallic tinge
            out.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    /// Commuted Piano: dual coupled waveguides with dispersion, loss, soundboard.
    ///
    /// Signal flow per sample:
    ///   read delay → loss LP → dispersion allpass cascade → fractional delay AP
    ///   → bridge coupling → write back to delay
    ///   Output: mix strings → soundboard resonance → DC blocker
    fn tick_commuted_piano(&mut self) -> f32 {
        let sr = self.sample_rate;

        if let OscState::CommutedPiano {
            buffer, buffer2, pos, pos2,
            loss_state1, loss_state2, loss_coeff,
            frac_z1, frac_coeff,
            disp_z1, disp_z2, _disp_b0: _, _disp_b1: _, disp_a1,
            sb_lp, sb_bp, sb_freq, sb_q,
            dc_x, dc_y,
        } = &mut self.state
        {
            let n1 = buffer.len();
            let n2 = buffer2.len();
            if n1 == 0 { return 0.0; }

            // ── Read from delay lines ──
            let s1 = buffer[*pos];
            let s2 = if n2 > 0 { buffer2[*pos2] } else { 0.0 };

            // ── Loss filter (one-pole LP — frequency-dependent decay) ──
            // One-pole: y += c * (x - y). Higher partials attenuated more per round trip.
            let lc = *loss_coeff;
            *loss_state1 += lc * (s1 - *loss_state1);
            *loss_state2 += lc * (s2 - *loss_state2);

            // ── Dispersion filter (3 cascaded first-order allpass sections) ──
            // Models string stiffness: higher partials get extra phase delay → stretched tuning
            // First-order allpass: y[n] = a * (x[n] - y[n-1]) + x[n-1]
            // disp_z1[i] = previous input x[n-1], disp_z2[i] = previous output y[n-1]
            let mut x = *loss_state1;
            for i in 0..3 {
                let a = disp_a1[i];
                let y = a * (x - disp_z2[i]) + disp_z1[i];
                disp_z1[i] = x;
                disp_z2[i] = y;
                x = y;
            }

            // Also apply dispersion to string 2 (reuse same coefficients)
            let x2 = *loss_state2;
            // Use a lightweight single-section dispersion for string 2
            {
                let a = disp_a1[0]; // just the first section
                // We need separate state for string 2 — abuse frac_z1 trick:
                // Actually, let's skip dispersion for string 2 since its detuning
                // already provides inharmonicity-like beating. The key perceptual
                // effect is on the primary string.
                let _ = a;
            }

            // ── Fractional delay allpass (per string, for pitch accuracy) ──
            // First-order Thiran: y[n] = c * (x[n] - y[n-1]) + x[n-1]
            let fo1 = frac_coeff[0] * (x - frac_z1[0]) + frac_z1[0];
            frac_z1[0] = x;

            let fo2 = frac_coeff[1] * (x2 - frac_z1[1]) + frac_z1[1];
            frac_z1[1] = x2;

            // ── Write back to delay lines (with bridge coupling) ──
            // Weinreich coupling: strings exchange energy through bridge
            let coupling = 0.004;
            buffer[*pos] = fo1 + s2 * coupling;
            if n2 > 0 {
                buffer2[*pos2] = fo2 + s1 * coupling;
            }

            *pos = (*pos + 1) % n1;
            if n2 > 0 {
                *pos2 = (*pos2 + 1) % n2;
            }

            // ── Mix strings ──
            let raw = s1 * 0.55 + s2 * 0.45;

            // ── Soundboard resonance (SVF bandpass, adds body/warmth) ──
            let g = PI * *sb_freq / sr; // SVF integrator coefficient
            let k = 1.0 / *sb_q;
            let hp = raw - *sb_lp - k * *sb_bp;
            *sb_bp += g * hp;
            *sb_lp += g * *sb_bp;
            // Mostly direct signal + some resonance coloring
            let with_sb = raw * 0.82 + *sb_bp * 0.18;

            // ── DC blocker ──
            let dc_out = with_sb - *dc_x + 0.9975 * *dc_y;
            *dc_x = with_sb;
            *dc_y = dc_out;

            dc_out.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    /// Banded Waveguide: 4 parallel delay lines at inharmonic partial frequencies.
    fn tick_banded_wg(&mut self) -> f32 {
        let brightness = self.ks_brightness;
        let feedback = self.ks_feedback;

        if let OscState::BandedWG {
            buffers,
            positions,
            filter_states,
            gains,
        } = &mut self.state
        {
            let mut out = 0.0;

            for i in 0..4 {
                let n = buffers[i].len();
                if n == 0 {
                    continue;
                }

                let sample = buffers[i][positions[i]];

                let br = brightness * (1.0 - 0.12 * i as f32);
                let filtered = filter_states[i] + br * (sample - filter_states[i]);
                filter_states[i] = filtered;

                let fb = feedback - 0.0015 * i as f32;
                buffers[i][positions[i]] = filtered * fb;
                positions[i] = (positions[i] + 1) % n;

                out += sample * gains[i];
            }

            out.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    /// Additive Piano: 16 partials with independent exponential decay.
    fn tick_additive_piano(&mut self) -> f32 {
        let sample_rate = self.sample_rate;
        let mut noise_state = self.noise_state;

        let result = if let OscState::AdditivePiano {
            phases,
            freqs,
            init_amps,
            decay_rates,
            time,
            noise_amp,
        } = &mut self.state
        {
            let dt = 1.0 / sample_rate;
            *time += dt;
            let t = *time;

            let mut out = 0.0;

            for n in 0..16 {
                let amp = init_amps[n];
                if amp < 0.001 {
                    continue;
                }

                let r = decay_rates[n];
                let env = 0.65 * fast_exp(-t * r * 3.0) + 0.35 * fast_exp(-t * r * 0.4);

                out += (phases[n] * TAU).sin() * amp * env;

                phases[n] += freqs[n] / sample_rate;
                phases[n] -= phases[n].floor();
            }

            // Hammer attack noise burst (~3ms)
            let noise_env = *noise_amp * fast_exp(-t * 300.0);
            if noise_env > 0.001 {
                noise_state ^= noise_state << 13;
                noise_state ^= noise_state >> 17;
                noise_state ^= noise_state << 5;
                let noise = (noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                out += noise * noise_env;
            }

            out *= 0.4;
            out.clamp(-1.0, 1.0)
        } else {
            0.0
        };

        self.noise_state = noise_state;
        result
    }

    /// Initialize bass guitar physical model.
    pub fn init_bass_guitar(
        &mut self,
        freq: f32,
        velocity: f32,
        style: f32,
        tone: f32,
        body: f32,
        pickup: f32,
    ) {
        self.reclaim_buffers();
        let mut model = BassModel::new();
        let buf_a = std::mem::take(&mut self.pool_a);
        let buf_b = std::mem::take(&mut self.pool_b);
        model.init_with_buffers(freq, velocity, style, tone, body, pickup, self.sample_rate, buf_a, buf_b);
        self.state = OscState::BassGuitar(model);
    }

    /// Bass guitar tick: delegates to BassModel.
    fn tick_bass_guitar(&mut self) -> f32 {
        if let OscState::BassGuitar(model) = &mut self.state {
            model.tick()
        } else {
            0.0
        }
    }

    /// Dedicated drum synthesis: sine with pitch envelope + shaped noise.
    /// Inspired by Nord Drum / Mutable Instruments Plaits drum models.
    fn tick_drum_synth(&mut self) -> f32 {
        let sample_rate = self.sample_rate;

        if let OscState::DrumSynth {
            phase,
            time,
            noise_state,
            base_freq,
            noise_lp_state,
            pitch_amount,
            pitch_decay,
            noise_level,
            noise_decay,
            noise_color,
        } = &mut self.state
        {
            let dt = 1.0 / sample_rate;
            *time += dt;
            let t = *time;

            // --- Tone component: sine with exponential pitch sweep ---
            // pitch_amount in semitones, decays from base_freq * 2^(amount/12) down to base_freq
            let pitch_env = fast_exp(-t * *pitch_decay);
            let freq = *base_freq * (pitch_env * *pitch_amount / 12.0).exp2();

            let tone = (*phase * TAU).sin();
            *phase += freq / sample_rate;
            *phase -= (*phase).floor();

            // --- Noise component: white noise → one-pole LP → envelope ---
            let noise_env = fast_exp(-t * *noise_decay);
            let noise_out = if *noise_level > 0.001 && noise_env > 0.001 {
                // Xorshift32 noise
                *noise_state ^= *noise_state << 13;
                *noise_state ^= *noise_state >> 17;
                *noise_state ^= *noise_state << 5;
                let white = (*noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;

                // One-pole LP: color 0 = dark (coeff ~0.05), color 1 = bright (coeff ~0.95)
                let coeff = 0.05 + 0.9 * *noise_color;
                *noise_lp_state += coeff * (white - *noise_lp_state);

                *noise_lp_state * *noise_level * noise_env
            } else {
                0.0
            };

            (tone + noise_out).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }
    /// Initialize bowed string waveguide.
    /// body_type: 0=Violin, 1=Viola, 2=Cello, 3=Double Bass
    pub fn init_bowed_string(
        &mut self,
        freq: f32,
        velocity: f32,
        bow_pressure: f32,
        bow_position: f32,
        body_type: f32,
    ) {
        self.reclaim_buffers();
        let sample_rate = self.sample_rate;
        let delay_total = sample_rate / freq;
        let n_total = (delay_total as usize).max(4);

        // bow_position splits the string into two segments
        let bp = bow_position.clamp(0.02, 0.98);
        let n_nut = ((n_total as f32 * bp) as usize).max(1);
        let n_bridge = (n_total - n_nut).max(1);

        // Fractional delay allpass
        let frac = delay_total - n_total as f32;
        let allpass_coeff = (1.0 - frac) / (1.0 + frac);

        // Body resonance depends on instrument type
        let body_i = body_type as u32;
        let (body_freq, body_q) = match body_i {
            0 => (440.0, 3.0),   // Violin — high resonance
            1 => (330.0, 2.5),   // Viola
            2 => (180.0, 2.0),   // Cello
            _ => (90.0, 1.5),    // Double Bass
        };

        let bow_vel = velocity * 0.3;

        self.state = OscState::BowedString {
            nut_delay: Self::take_buf(&mut self.pool_a, n_nut),
            bridge_delay: Self::take_buf(&mut self.pool_b, n_bridge),
            nut_pos: 0,
            bridge_pos: 0,
            nut_filter: 0.0,
            bridge_lp: 0.0,
            bridge_bp: 0.0,
            allpass_prev_in: 0.0,
            allpass_prev_out: 0.0,
            allpass_coeff,
            bow_velocity: bow_vel,
            _bow_position: bp,
            bow_pressure: bow_pressure.clamp(0.01, 1.0),
            body_freq,
            body_q,
        };
    }

    /// Bowed string tick: McIntyre-Schumacher-Woodhouse friction model.
    fn tick_bowed_string(&mut self) -> f32 {
        let sample_rate = self.sample_rate;

        if let OscState::BowedString {
            nut_delay, bridge_delay, nut_pos, bridge_pos,
            nut_filter, bridge_lp, bridge_bp,
            allpass_prev_in, allpass_prev_out, allpass_coeff,
            bow_velocity, bow_pressure, body_freq, body_q, ..
        } = &mut self.state
        {
            let n_nut = nut_delay.len();
            let n_bridge = bridge_delay.len();
            if n_nut == 0 || n_bridge == 0 {
                return 0.0;
            }

            // Read incoming waves at bow point
            let incoming_nut = nut_delay[*nut_pos];
            let incoming_bridge = bridge_delay[*bridge_pos];

            // String velocity at bow point
            let v_string = incoming_nut + incoming_bridge;

            // Relative velocity between bow and string
            let v_rel = *bow_velocity - v_string;

            // Bow friction: f(v_rel) = v_rel * exp(-a * v_rel² + 0.5)
            let a = 100.0; // friction curve steepness
            let friction = *bow_pressure * v_rel * fast_exp(-a * v_rel * v_rel + 0.5);

            // Reflected waves = incoming + friction contribution
            let to_nut = incoming_bridge + friction;
            let to_bridge = incoming_nut + friction;

            // Nut reflection: invert + lowpass
            let nut_reflected = -to_nut;
            let nut_lp_coeff = 0.85;
            *nut_filter += nut_lp_coeff * (nut_reflected - *nut_filter);
            nut_delay[*nut_pos] = *nut_filter;
            *nut_pos = (*nut_pos + 1) % n_nut;

            // Bridge side: body filter (one-pole LP + bandpass resonance)
            let bridge_lp_coeff = 0.7;
            *bridge_lp += bridge_lp_coeff * (to_bridge - *bridge_lp);

            // Body bandpass resonance
            let f_norm = *body_freq / sample_rate;
            let w = (PI * f_norm).sin() * 2.0;
            let q_inv = 1.0 / *body_q;
            *bridge_bp = *bridge_lp - *bridge_bp * q_inv;
            let body_out = *bridge_bp;
            *bridge_bp += w * body_out;

            // Allpass fractional delay on bridge output
            let ap_in = *bridge_lp + body_out * 0.15;
            let c = *allpass_coeff;
            let ap_out = c * (ap_in - *allpass_prev_out) + *allpass_prev_in;
            *allpass_prev_in = ap_in;
            *allpass_prev_out = ap_out;

            bridge_delay[*bridge_pos] = ap_out * 0.995; // slight loss
            *bridge_pos = (*bridge_pos + 1) % n_bridge;

            // Output from bridge side
            (to_bridge + body_out * 0.2).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    /// Initialize brass lip reed + bore waveguide.
    ///
    /// Algorithm: Perry Cook STK Brass.cpp (1995-2023).
    /// Key: BiQuad resonator at playing frequency (r=0.997), output SQUARED → rich harmonics.
    /// Bore = 2*period delay (lips lock onto 2nd harmonic of bore, like real brass).
    ///
    /// bell_type: 0=Trumpet, 1=French Horn, 2=Trombone, 3=Tuba
    ///
    /// Field repurposing:
    ///   lip_x        → bq_y1      BiQuad output y[n-1]
    ///   lip_v        → bq_y2      BiQuad output y[n-2]
    ///   lip_freq     → bq_a1      -2*r*cos(2π*f/sr)
    ///   lip_damping  → bq_a2      r² = 0.994009
    ///   lip_mass_inv → bq_b0      gain (0.03, STK default)
    ///   bell_lp_state→ dc_x1      DC blocker: prev input
    ///   bell_hp_state→ dc_y1      DC blocker: prev output
    ///   bell_cutoff  → bell_a1    bell post-LP coefficient (shapes timbre per instrument)
    ///   bore_lp_state→ bell_state bell post-LP filter state
    pub fn init_brass(
        &mut self,
        freq: f32,
        velocity: f32,
        lip_tension: f32,
        blowing_pressure: f32,
        bell_type: f32,
    ) {
        self.reclaim_buffers();
        let sr = self.sample_rate;

        // Bore: 2-period delay line (STK formula: sr/f * 2.0 + 3.0)
        let n = ((sr / freq) * 2.0 + 3.0).round() as usize;
        let n = n.max(8);

        // Lip BiQuad resonator: 2 poles at lip frequency with r=0.997 (very high Q)
        // Tuned slightly above fundamental; lip_tension shifts it.
        let lip_freq_hz = freq * (0.95 + lip_tension * 0.10);
        let r = 0.997_f32;
        let bq_a1 = -2.0 * r * (TAU * lip_freq_hz / sr).cos();
        let bq_a2 = r * r;
        let bq_b0 = 0.03_f32;  // STK: lipFilter_.setGain(0.03)

        // Blowing pressure → STK maxPressure
        let max_pressure = (0.55 + blowing_pressure * 0.40) * velocity.clamp(0.0, 1.0);

        // Bell post-LP: one-pole LP that shapes radiated timbre per instrument
        let bell_i = bell_type as u32;
        let bell_fc: f32 = match bell_i {
            0 => 7000.0, // Trumpet — bright, cutting
            1 => 3500.0, // French Horn — warm, dark
            2 => 5000.0, // Trombone — broad, mid-bright
            _ => 2200.0, // Tuba — very dark, tubby
        };
        let bell_a1 = 1.0 - (-TAU * bell_fc / sr).exp();

        // Seed bore with tiny noise burst to kickstart self-oscillation
        let mut bore = Self::take_buf(&mut self.pool_a, n);
        let mut rng = 0xDEADBEEFu32;
        for s in bore.iter_mut() {
            rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5;
            *s = (rng as f32 / u32::MAX as f32 - 0.5) * 0.002 * velocity;
        }

        self.state = OscState::Brass {
            bore_delay: bore,
            bore_pos: 0,
            lip_x: 0.0,           // bq_y1
            lip_v: 0.0,           // bq_y2
            lip_freq: bq_a1,      // BiQuad a1 coefficient
            lip_damping: bq_a2,   // BiQuad a2 coefficient
            lip_mass_inv: bq_b0,  // BiQuad b0 gain
            bell_lp_state: 0.0,   // dc_x1
            bell_hp_state: 0.0,   // dc_y1
            bell_cutoff: bell_a1, // bell post-LP a1
            bore_lp_state: 0.0,   // bell post-LP state
            blowing_pressure: max_pressure,
            _bore_length_ratio: 1.0,
        };
    }

    /// Brass tick: STK Brass.cpp algorithm (Cook 1995).
    ///
    /// Signal flow (per-sample):
    ///   bore_out * 0.85 → p_bore
    ///   dp = p_mouth*0.3 − p_bore
    ///   bq_y = b0*dp − a1*y1 − a2*y2   ← 2nd-order resonator
    ///   delta = clamp(bq_y², 0, 1)       ← squaring = harmonic generation
    ///   sig = delta*p_mouth + (1−delta)*p_bore  ← scattering junction
    ///   bore_in = DC_block(sig)
    ///   output = bell_LP(bore_out)        ← radiated: shapes timbre
    fn tick_brass(&mut self) -> f32 {
        if let OscState::Brass {
            bore_delay, bore_pos,
            lip_x,          // bq_y1
            lip_v,          // bq_y2
            lip_freq,       // bq_a1
            lip_damping,    // bq_a2
            lip_mass_inv,   // bq_b0 gain
            bell_lp_state,  // dc_x1 (DC blocker prev input)
            bell_hp_state,  // dc_y1 (DC blocker prev output)
            bell_cutoff,    // bell post-LP a1
            bore_lp_state,  // bell post-LP state
            blowing_pressure,
            ..
        } = &mut self.state
        {
            let n = bore_delay.len();
            if n == 0 { return 0.0; }

            // 1. Read bore return (oldest sample = signal from 2 periods ago)
            let bore_out = bore_delay[*bore_pos];

            // 2. Pressures
            let p_bore  = bore_out * 0.85;
            let p_mouth = *blowing_pressure * 0.3;
            let dp = p_mouth - p_bore;

            // 3. Lip BiQuad resonator (high-Q bandpass at playing frequency)
            //    y[n] = b0*x - a1*y[n-1] - a2*y[n-2]
            let bq_y = *lip_mass_inv * dp - *lip_freq * *lip_x - *lip_damping * *lip_v;
            *lip_v = *lip_x;
            *lip_x = bq_y;

            // 4. Nonlinear: square + hard saturate  → harmonic generation
            let delta = (bq_y * bq_y).min(1.0);

            // 5. Scattering junction (STK formula)
            let sig = delta * p_mouth + (1.0 - delta) * p_bore;

            // 6. DC blocker: y[n] = x[n] - x[n-1] + 0.995*y[n-1]
            let dc_out = sig - *bell_lp_state + 0.995 * *bell_hp_state;
            *bell_lp_state = sig;
            *bell_hp_state = dc_out;

            // 7. Write DC-blocked signal into bore, advance pointer
            bore_delay[*bore_pos] = dc_out;
            *bore_pos = (*bore_pos + 1) % n;

            // 8. Bell post-LP: radiate bore_out through bell filter (shapes timbre)
            *bore_lp_state += *bell_cutoff * (bore_out - *bore_lp_state);

            (*bore_lp_state * 3.5).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    // ─── Phase Distortion (Casio CZ) ────────────────────────────────

    pub fn init_phase_distortion(&mut self, pd_shape: u8, pd_depth: f32) {
        self.state = OscState::PhaseDistortion {
            phase: 0.0,
            pd_shape: pd_shape.min(7),
            pd_depth,
        };
    }

    fn tick_phase_distortion(&mut self, freq: f32) -> f32 {
        let sample_rate = self.sample_rate;

        if let OscState::PhaseDistortion { phase, pd_shape, pd_depth } = &mut self.state {
            // 2x oversampling to reduce aliasing from nonlinear phase mapping
            let dt = freq / (sample_rate * 2.0);
            let d = *pd_depth;
            let shape = *pd_shape;
            let mut sum = 0.0;
            for _ in 0..2 {
                *phase += dt;
                *phase -= phase.floor();
                let distorted = distort_phase(*phase, shape, d, freq, sample_rate);
                sum += (distorted * TAU).sin();
            }
            sum * 0.5
        } else {
            0.0
        }
    }

    // ─── Wavefolder (West Coast) ─────────────────────────────────────

    pub fn init_wavefolder(&mut self, fold_source: u8, fold_amount: f32, fold_symmetry: f32) {
        self.state = OscState::Wavefolder {
            phase: 0.0,
            prev_input: 0.0,
            prev_adf: sine_fold_ad(0.0),
            fold_amount,
            fold_symmetry,
            fold_source: fold_source.min(2),
        };
    }

    fn tick_wavefolder(&mut self, freq: f32) -> f32 {
        let sample_rate = self.sample_rate;

        if let OscState::Wavefolder {
            phase, prev_input, prev_adf,
            fold_amount, fold_symmetry, fold_source,
        } = &mut self.state
        {
            let dt = freq / sample_rate;
            *phase += dt;
            *phase -= phase.floor();
            let t = *phase;

            // Source waveform
            let source = match *fold_source {
                0 => (t * TAU).sin(),
                1 => if t < 0.5 { 4.0 * t - 1.0 } else { 3.0 - 4.0 * t },
                _ => 2.0 * t - 1.0,
            };

            // Apply gain and symmetry offset
            let gain = 1.0 + *fold_amount * 9.0;
            let x = source * gain + (*fold_symmetry - 0.5) * 2.0;

            // ADAA 1st order
            let adf = sine_fold_ad(x);
            let diff = x - *prev_input;
            let out = if diff.abs() > 1e-5 {
                (adf - *prev_adf) / diff
            } else {
                sine_fold((x + *prev_input) * 0.5)
            };

            *prev_input = x;
            *prev_adf = adf;

            out.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    // ─── Modal Resonator ─────────────────────────────────────────────

    pub fn init_modal_resonator(
        &mut self,
        freq: f32,
        material: u8,
        brightness: f32,
        damping: f32,
        strike_pos: f32,
    ) {
        let sample_rate = self.sample_rate;
        let mat = (material as usize).min(MODAL_MATERIALS - 1);
        let ratios = &MODAL_RATIOS[mat];

        let mut y1 = [0.0f32; 16];
        let mut y2 = [0.0f32; 16];
        let mut cosw = [0.0f32; 16];
        let mut r_sq = [0.0f32; 16];
        let mut amps = [0.0f32; 16];

        let base_bandwidth = 2.0 + damping * 12.0;

        for i in 0..16 {
            let mode_freq = freq * ratios[i];
            if mode_freq > sample_rate * 0.5 || mode_freq < 1.0 {
                amps[i] = 0.0;
                cosw[i] = 0.0;
                r_sq[i] = 0.0;
                continue;
            }

            let bw = base_bandwidth * (1.0 + i as f32 * (1.0 - brightness) * 0.5);
            let r = fast_exp(-PI * bw / sample_rate);
            let w = TAU * mode_freq / sample_rate;
            cosw[i] = 2.0 * r * w.cos();
            r_sq[i] = r * r;

            // Strike position filtering + 1/(i+1) amplitude rolloff
            let strike = (PI * (i + 1) as f32 * strike_pos.clamp(0.01, 0.99)).sin().abs();
            amps[i] = strike / (1 + i) as f32;

            y1[i] = 0.0;
            y2[i] = 0.0;
        }

        self.state = OscState::ModalResonator {
            y1, y2, cosw, r_sq, amps,
            excitation_time: 0.0,
            noise_state: self.noise_state,
        };
    }

    fn tick_modal_resonator(&mut self) -> f32 {
        let sample_rate = self.sample_rate;

        if let OscState::ModalResonator {
            y1, y2, cosw, r_sq, amps,
            excitation_time, noise_state,
        } = &mut self.state
        {
            // Excitation: 2ms noise burst with linear decay
            let burst_dur = 0.002;
            let excitation = if *excitation_time < burst_dur {
                let env = 1.0 - *excitation_time / burst_dur;
                *noise_state ^= *noise_state << 13;
                *noise_state ^= *noise_state >> 17;
                *noise_state ^= *noise_state << 5;
                let noise = (*noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                noise * env
            } else {
                0.0
            };

            let mut output = 0.0;
            for i in 0..16 {
                if amps[i] < 0.0001 { continue; }
                let y0 = cosw[i] * y1[i] - r_sq[i] * y2[i] + excitation * amps[i];
                y2[i] = y1[i];
                y1[i] = y0;
                output += y0;
            }

            *excitation_time += 1.0 / sample_rate;

            // Scale output to reasonable level
            (output * 0.5).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    // ─── Hard Sync (Prophet-5 / Moog Prodigy style) ──────────────────

    pub fn init_hard_sync(&mut self, sync_ratio: f32, sync_shape: u8) {
        self.state = OscState::HardSync {
            master_phase: 0.0,
            slave_phase: 0.0,
            sync_ratio: sync_ratio.clamp(1.0, 16.0),
            sync_shape: sync_shape.min(2),
        };
    }

    fn tick_hard_sync(&mut self, freq: f32) -> f32 {
        let sample_rate = self.sample_rate;

        if let OscState::HardSync {
            master_phase, slave_phase, sync_ratio, sync_shape,
        } = &mut self.state
        {
            let master_dt = freq / sample_rate;
            let slave_dt = freq * *sync_ratio / sample_rate;

            // Advance master
            *master_phase += master_dt;

            let mut sync_correction = 0.0;

            // Check for master reset (hard sync)
            if *master_phase >= 1.0 {
                *master_phase -= 1.0;
                // Compute slave value just before reset for discontinuity correction
                let old_slave = *slave_phase;
                let old_val = match *sync_shape {
                    0 => 2.0 * old_slave - 1.0,
                    1 => if old_slave < 0.5 { 1.0 } else { -1.0 },
                    _ => if old_slave < 0.5 { 4.0 * old_slave - 1.0 } else { 3.0 - 4.0 * old_slave },
                };
                // Reset slave phase
                let overshoot = *master_phase / master_dt;
                *slave_phase = overshoot * slave_dt;
                // Compute slave value just after reset
                let new_val = match *sync_shape {
                    0 => 2.0 * *slave_phase - 1.0,
                    1 => if *slave_phase < 0.5 { 1.0 } else { -1.0 },
                    _ => if *slave_phase < 0.5 { 4.0 * *slave_phase - 1.0 } else { 3.0 - 4.0 * *slave_phase },
                };
                // BLIT-style correction: band-limited step at discontinuity
                // The discontinuity magnitude scaled by polyBLEP-like window
                let d = overshoot; // 0-1 fractional position in sample
                let step = new_val - old_val;
                // 2nd-order polynomial smoothing of the step discontinuity
                if d < 1.0 {
                    sync_correction = step * (d * d * 0.5 - d + 0.5);
                }
            } else {
                *slave_phase += slave_dt;
                if *slave_phase >= 1.0 {
                    *slave_phase -= slave_phase.floor();
                }
            }

            let t = *slave_phase;
            // Slave waveform with polyBLEP on natural edges
            let out = match *sync_shape {
                0 => {
                    // Saw with polyBLEP
                    2.0 * t - 1.0 - poly_blep(t, slave_dt)
                }
                1 => {
                    // Square with polyBLEP
                    let pw = 0.5;
                    let mut sq = if t < pw { 1.0 } else { -1.0 };
                    sq += poly_blep(t, slave_dt);
                    sq -= poly_blep((t - pw + 1.0) % 1.0, slave_dt);
                    sq
                }
                _ => {
                    // Triangle (integrated square — naturally band-limited)
                    if t < 0.5 { 4.0 * t - 1.0 } else { 3.0 - 4.0 * t }
                }
            };

            out - sync_correction
        } else {
            0.0
        }
    }

    // ─── Supersaw (Roland JP-8000) ───────────────────────────────────

    pub fn init_supersaw(&mut self, detune_amount: f32, mix: f32) {
        // JP-8000 supersaw: 7 saws with specific detuning distribution
        // Center saw + 3 pairs symmetrically detuned
        // Detune curve from Adam Szabo's analysis of JP-8000
        let detune = detune_amount.clamp(0.0, 1.0);

        // Detune in cents: follows a roughly quadratic curve
        // At max detune ~1 semitone = 100 cents spread
        let max_cents = 70.0 * detune + 30.0 * detune * detune;
        let offsets = [-0.11002313, -0.06288439, -0.01952356, 0.0,
                        0.01991221,  0.06216538,  0.10745242];

        let mut detunes = [0.0f32; 7];
        for i in 0..7 {
            let cents = offsets[i] * max_cents * 2.0;
            detunes[i] = 2.0_f32.powf(cents / 1200.0);
        }

        // Mix: center vs side voices
        // At low detune, center dominates. At high detune, sides dominate.
        let mix_center = 1.0 - mix * 0.3;
        let mix_side = 0.3 + mix * 0.7;

        // Randomize initial phases for richness
        let mut phases = [0.0f32; 7];
        let mut state = self.noise_state;
        for p in &mut phases {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *p = state as f32 / u32::MAX as f32;
        }
        self.noise_state = state;

        self.state = OscState::Supersaw {
            phases,
            detunes,
            mix_center,
            mix_side,
        };
    }

    fn tick_supersaw(&mut self, freq: f32) -> f32 {
        let sample_rate = self.sample_rate;

        if let OscState::Supersaw {
            phases, detunes, mix_center, mix_side,
        } = &mut self.state
        {
            let mut out = 0.0;
            let norm = 1.0 / 7.0_f32.sqrt(); // normalize for 7 voices

            for i in 0..7 {
                let dt = freq * detunes[i] / sample_rate;
                phases[i] += dt;
                phases[i] -= phases[i].floor();

                // Saw wave with polyBLEP anti-aliasing
                let saw = 2.0 * phases[i] - 1.0 - poly_blep(phases[i], dt);

                // Center voice (index 3) gets different mix level
                let level = if i == 3 { *mix_center } else { *mix_side };
                out += saw * level;
            }

            (out * norm).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    // ── PianoModel ──

    pub fn init_piano_model(&mut self, freq: f32) {
        self.reclaim_buffers();
        let brightness = self.ks_brightness;
        let feedback = self.ks_feedback;
        let sample_rate = self.sample_rate;
        self.state = OscState::PianoModel(PianoModel::new());
        if let OscState::PianoModel(ref mut piano) = self.state {
            piano.init(freq, sample_rate, brightness, feedback);
        }
    }

    fn tick_piano_model(&mut self) -> f32 {
        if let OscState::PianoModel(ref mut piano) = self.state {
            piano.tick()
        } else {
            0.0
        }
    }

    // ── ElectricPiano ──

    pub fn init_electric_piano(&mut self, freq: f32, ep_type: u8) {
        self.reclaim_buffers();
        let brightness = self.ks_brightness;
        let feedback = self.ks_feedback;
        let sample_rate = self.sample_rate;
        self.state = OscState::ElectricPiano(ElectricPianoModel::new());
        if let OscState::ElectricPiano(ref mut ep) = self.state {
            ep.init(freq, sample_rate, brightness, feedback, ep_type);
        }
    }

    fn tick_electric_piano(&mut self) -> f32 {
        if let OscState::ElectricPiano(ref mut ep) = self.state {
            ep.tick()
        } else {
            0.0
        }
    }

} // impl Oscillator

// ─── Modal Resonator frequency ratio tables ──────────────────────────

const MODAL_MATERIALS: usize = 10;

/// 10 materials × 16 modes. Ratios relative to fundamental.
#[allow(clippy::approx_constant)]
const MODAL_RATIOS: [[f32; 16]; MODAL_MATERIALS] = [
    // 0: Steel Bar
    [1.0, 2.756, 5.404, 8.933, 13.344, 18.637, 24.812, 31.869,
     39.808, 48.630, 58.333, 68.919, 80.387, 92.737, 105.970, 120.085],
    // 1: Aluminum
    [1.0, 2.83, 5.65, 9.42, 14.13, 19.78, 26.37, 33.90,
     42.37, 51.78, 62.13, 73.42, 85.65, 98.82, 112.93, 127.98],
    // 2: Glass
    [1.0, 2.32, 4.15, 6.45, 9.20, 12.40, 16.05, 20.15,
     24.70, 29.70, 35.15, 41.05, 47.40, 54.20, 61.45, 69.15],
    // 3: Wood Block
    [1.0, 2.572, 4.644, 6.984, 9.723, 12.0, 15.0, 18.2,
     21.7, 25.4, 29.3, 33.5, 37.9, 42.5, 47.3, 52.3],
    // 4: Marimba
    [1.0, 3.98, 9.22, 16.0, 23.0, 31.0, 40.0, 50.0,
     61.0, 73.0, 86.0, 100.0, 115.0, 131.0, 148.0, 166.0],
    // 5: Vibraphone (quadratic ratios: n²)
    [1.0, 4.0, 9.0, 16.0, 25.0, 36.0, 49.0, 64.0,
     81.0, 100.0, 121.0, 144.0, 169.0, 196.0, 225.0, 256.0],
    // 6: Tubular Bell
    [1.0, 2.66, 5.0, 8.0, 11.55, 15.66, 20.33, 25.55,
     31.33, 37.66, 44.55, 52.0, 60.0, 68.55, 77.66, 87.33],
    // 7: Church Bell
    [1.0, 1.65, 2.0, 2.57, 3.14, 4.0, 4.78, 5.63,
     6.55, 7.55, 8.62, 9.76, 10.97, 12.25, 13.60, 15.02],
    // 8: Membrane (kick)
    [1.0, 1.593, 2.136, 2.296, 2.653, 2.917, 3.156, 3.501,
     3.600, 3.882, 4.059, 4.241, 4.601, 4.903, 5.132, 5.367],
    // 9: Timpani
    [1.0, 1.504, 1.741, 2.0, 2.245, 2.494, 2.740, 2.985,
     3.228, 3.470, 3.710, 3.949, 4.187, 4.424, 4.660, 4.895],
];

// ─── Phase distortion helper ─────────────────────────────────────────

/// Apply phase distortion function.
#[inline]
fn distort_phase(t: f32, shape: u8, depth: f32, freq: f32, sample_rate: f32) -> f32 {
    if depth < 0.001 {
        return t; // no distortion → pure sine
    }
    let d = depth;
    match shape {
        // 0: Saw — fast read then hold
        0 => {
            if t < 0.5 {
                let r = 1.0 + d;
                (t * r).min(1.0) * 0.5
            } else {
                0.5
            }
        }
        // 1: Square — two compressed half-periods
        1 => {
            if t < 0.5 {
                let r = 1.0 + d;
                (t * r).min(0.5)
            } else {
                let r = 1.0 + d;
                let tt = t - 0.5;
                0.5 + (tt * r).min(0.5)
            }
        }
        // 2: Pulse — narrow impulse
        2 => {
            let w = 0.25 / (1.0 + d * 3.0);
            if t < w {
                t / w * 0.5
            } else {
                0.5
            }
        }
        // 3: DoubleSine — double frequency morph
        3 => {
            let r = 1.0 + d;
            (t * r) % 1.0
        }
        // 4: SawPulse — combination
        4 => {
            let saw = if t < 0.5 { t * (1.0 + d) } else { 0.5 };
            let pulse_w = 0.25 / (1.0 + d * 2.0);
            let pulse = if t < pulse_w { t / pulse_w * 0.5 } else { 0.5 };
            saw * (1.0 - d * 0.5) + pulse * d * 0.5
        }
        // 5: Reso1 — resonant harmonic N
        5 => {
            let max_n = (sample_rate / (2.0 * freq)).min(16.0);
            let n = 1.0 + d * (max_n - 1.0);
            (t * n) % 1.0
        }
        // 6: Reso2 — windowed resonance
        6 => {
            let max_n = (sample_rate / (2.0 * freq)).min(16.0);
            let n = 1.0 + d * (max_n - 1.0);
            let raw = (t * n) % 1.0;
            // Window function: smoothly blend with original phase
            let window = (t * PI).sin();
            t * (1.0 - window * d) + raw * window * d
        }
        // 7: Reso3 — resonance with inverted second half
        7 => {
            let max_n = (sample_rate / (2.0 * freq)).min(16.0);
            let n = 1.0 + d * (max_n - 1.0);
            if t < 0.5 {
                (t * n) % 1.0
            } else {
                1.0 - ((t * n) % 1.0)
            }
        }
        _ => t,
    }
}

// ─── Wavefolder helpers ──────────────────────────────────────────────

/// Sine fold: f(x) = sin(π/2 × x)
#[inline]
fn sine_fold(x: f32) -> f32 {
    (x * PI * 0.5).sin()
}

/// Antiderivative of sine fold: F(x) = -2/π² × cos(π/2 × x)
#[inline]
fn sine_fold_ad(x: f32) -> f32 {
    -2.0 / (PI * PI) * (x * PI * 0.5).cos()
}

/// Fast exponential approximation for negative arguments.
/// Uses the identity exp(x) ≈ (1 + x/256)^256 via repeated squaring.
#[inline]
fn fast_exp(x: f32) -> f32 {
    let x = x.max(-20.0);
    let mut y = 1.0 + x / 256.0;
    y *= y;
    y *= y;
    y *= y;
    y *= y; // 2^4 = 16
    y *= y;
    y *= y;
    y *= y;
    y *= y; // 2^8 = 256
    y.max(0.0)
}

impl Oscillator {
    pub fn init_accordion(
        &mut self,
        freq: f32,
        velocity: f32,
        register: f32,    // 0=fundamental, 1=octave+, 2=musette (detuned), 3=master
        bellows: f32,      // 0-1 bellows pressure
    ) {
        let sr = self.sample_rate;
        let phase_inc = freq / sr; // phase increment per sample (0..1)

        // Second reed: depends on register
        let (phase_inc2, mix) = match register as u32 {
            0 => (phase_inc, 0.0),                              // single reed
            1 => (phase_inc * 2.0, 0.4),                        // octave up
            2 => (phase_inc * (1.0 + 0.003), 0.5),              // musette (slightly detuned for beating)
            _ => (phase_inc * 2.0, 0.3),                        // master (mix)
        };

        // Bellows pressure controls amplitude and harmonic content
        let bp = (0.5 + bellows * 0.5) * velocity;

        self.state = OscState::Accordion {
            reed_x: 0.0,
            reed_v: phase_inc,
            reed_damping: bp,
            reed2_x: 0.0,
            reed2_v: phase_inc2,
            register_mix: mix,
            lp_state: 0.0,
            dc_x: 0.0,
            dc_y: 0.0,
        };
    }

    fn tick_accordion(&mut self) -> f32 {
        // Accordion: phase-driven free reed model.
        // Fields repurposed: reed_x/reed2_x = phase, reed_v/reed2_v = phase_inc,
        // reed_damping = bellows pressure (amplitude).

        if let OscState::Accordion {
            reed_x, reed_v, reed_damping,
            reed2_x, reed2_v,
            register_mix,
            lp_state, dc_x, dc_y,
        } = &mut self.state
        {
            let bp = *reed_damping; // bellows pressure = drive amplitude

            // Reed 1: phase accumulator with nonlinear waveshaping
            *reed_x += *reed_v;
            if *reed_x >= 1.0 { *reed_x -= 1.0; }
            let phase1 = *reed_x;
            // Free reed waveform: asymmetric — mostly sinusoidal with odd harmonics
            // sin + slight cubic distortion for that reedy buzz
            let sin1 = (phase1 * TAU).sin();
            let out1 = sin1 + 0.3 * sin1 * sin1 * sin1.signum(); // adds 3rd harmonic

            // Reed 2 (register reed)
            *reed2_x += *reed2_v;
            if *reed2_x >= 1.0 { *reed2_x -= 1.0; }
            let phase2 = *reed2_x;
            let sin2 = (phase2 * TAU).sin();
            let out2 = sin2 + 0.3 * sin2 * sin2 * sin2.signum();

            // Mix registers
            let mix = *register_mix;
            let mixed = out1 * (1.0 - mix) + out2 * mix;

            // Apply bellows pressure as amplitude
            let driven = mixed * bp;

            // Body resonance LP filter — warms the tone
            *lp_state += 0.2 * (driven - *lp_state);
            let shaped = driven * 0.55 + *lp_state * 0.45;

            // DC blocker
            let dc_out = shaped - *dc_x + 0.995 * *dc_y;
            *dc_x = shaped;
            *dc_y = dc_out;

            dc_out.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    // ─── Saxophone (Single Reed Waveguide) ─────────────────────────

    /// STK-style single delay line reed instrument.
    /// Reed at input, bell reflection at output, feedback loop.
    pub fn init_saxophone(
        &mut self,
        freq: f32,
        velocity: f32,
        reed_stiffness: f32,  // 0-1
        embouchure: f32,      // 0-1
        blow_pressure: f32,   // 0-1
        sax_type: f32,        // 0=soprano, 1=alto, 2=tenor, 3=bari
    ) {
        self.reclaim_buffers();
        let sr = self.sample_rate;
        let n = (sr / freq).round() as usize;
        let n = n.max(4);

        // Bell LP coefficient: lower = darker (bari), higher = brighter (soprano)
        let bell_coeff = match sax_type as u32 {
            0 => 0.7,  // soprano — bright
            1 => 0.55, // alto
            2 => 0.4,  // tenor
            _ => 0.3,  // bari — dark
        };

        // Reed table: offset + stiffness * pressure_diff, clamped to [-1,1]
        // Negative stiffness = reed closes when pressure_diff increases (normal reed behavior)
        let stiff = -(0.3 + reed_stiffness * 0.4);
        // Offset controls rest position of reed (how open it is)
        let offset = 0.6 + embouchure * 0.1;
        let pressure = (0.4 + blow_pressure * 0.35) * velocity;

        let mut buf = Self::take_buf(&mut self.pool_a, n);
        // Seed with noise burst to kickstart oscillation
        let mut rng = 0xDEADBEEFu32;
        for s in buf.iter_mut() {
            rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5;
            *s = (rng as f32 / u32::MAX as f32 - 0.5) * 0.01 * velocity;
        }

        self.state = OscState::Saxophone {
            delay: buf,
            delay_pos: 0,
            reed_stiffness: stiff,
            reed_table_offset: offset,
            blowing_pressure: pressure,
            bell_lp: 0.0,
            bell_coeff,
            dc_x: 0.0,
            dc_y: 0.0,
        };
    }

    fn tick_saxophone(&mut self) -> f32 {
        if let OscState::Saxophone {
            delay, delay_pos,
            reed_stiffness, reed_table_offset, blowing_pressure,
            bell_lp, bell_coeff,
            dc_x, dc_y,
        } = &mut self.state
        {
            let n = delay.len();
            if n == 0 { return 0.0; }

            // 1. Read from delay line end (returning wave from bell)
            let delay_out = delay[*delay_pos];

            // 2. Bell reflection LP filter (models bell radiation loss)
            *bell_lp += *bell_coeff * (delay_out - *bell_lp);
            let reflected = -*bell_lp;  // invert at open end (pressure node)

            // 3. Reed reflection: STK reed table model
            // pressure_diff drives the reed; reed_table gives reflection coefficient
            let bp = *blowing_pressure;
            let pressure_diff = bp + reflected;  // mouth pressure + returning wave
            // Reed table: piecewise linear, clamped to [-1, 1]
            // When pressure_diff is small, reed is open → positive reflection
            // When pressure_diff is large, reed closes → negative reflection (cuts off)
            let reed_out = *reed_table_offset + *reed_stiffness * pressure_diff;
            let reed_out = reed_out.clamp(-1.0, 1.0);

            // 4. New traveling wave into bore = reflected * reed_reflection + driving pressure
            let bore_in = reflected + reed_out * pressure_diff;

            // 5. Bore loss (very small — long tube is mostly lossless)
            let bore_in = bore_in * 0.995;

            // 6. Write into delay line
            delay[*delay_pos] = bore_in;
            *delay_pos = (*delay_pos + 1) % n;

            // 7. Output: radiated sound from bell (high-pass characteristic)
            // Output is the difference between outgoing and reflected waves
            let radiated = (delay_out - *bell_lp) * 4.0;

            // DC blocker
            let dc_out = radiated - *dc_x + 0.995 * *dc_y;
            *dc_x = radiated;
            *dc_y = dc_out;

            dc_out.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn init_alias(&mut self, wave_type: u8, crush_bits: f32) {
        if let OscState::Alias { wave_type: wt, crush_bits: cb, .. } = &mut self.state {
            *wt = wave_type;
            *cb = crush_bits;
        }
    }

    pub fn init_window(&mut self, wtype: u8, morph: f32, formant: f32) {
        if let OscState::Window { window_type, morph: m, formant: f, .. } = &mut self.state {
            *window_type = wtype;
            *m = morph;
            *f = formant;
        }
    }

    // ─── Alias oscillator ──────────────────────────────────────────────

    fn tick_alias(&mut self, freq: f32) -> f32 {
        if let OscState::Alias { phase, crush_bits, wave_type, noise_state, table } = &mut self.state {
            // 8.24 fixed-point phase increment
            let inc = ((freq / self.sample_rate) * (1u64 << 24) as f32) as u32;
            *phase = phase.wrapping_add(inc);
            let index = (*phase >> 24) as u8; // top 8 bits = table index

            // Generate raw 8-bit sample based on wave type
            let raw_byte = match *wave_type {
                0 => { // Sine table lookup
                    let s = (index as f32 / 256.0 * TAU).sin();
                    ((s * 127.0) + 128.0) as u8
                }
                1 => index, // Ramp (naturally aliasing)
                2 => if index < 128 { 255 } else { 0 }, // Pulse
                3 => { // 8-bit XOR noise
                    *noise_state ^= noise_state.wrapping_shl(7);
                    *noise_state ^= noise_state.wrapping_shr(5);
                    *noise_state ^= noise_state.wrapping_shl(3);
                    *noise_state
                }
                4 => { // Additive: 16 partials
                    let mut sum: f32 = 0.0;
                    let base_phase = index as f32 / 256.0;
                    for h in 1..=16u32 {
                        let amp = 1.0 / h as f32;
                        sum += amp * (base_phase * h as f32 * TAU).sin();
                    }
                    let norm = sum * 0.25; // rough normalization
                    ((norm.clamp(-1.0, 1.0) * 127.0) + 128.0) as u8
                }
                _ => index,
            };

            // Store in table for potential visualization
            table[index as usize] = raw_byte;

            // Bit-crushing: reduce bit depth
            let bits = crush_bits.clamp(1.0, 8.0);
            let quant = 2.0_f32.powf(bits);
            let sample = raw_byte as f32 / 255.0 * 2.0 - 1.0; // normalize to -1..1
            let crushed = (sample * quant).round() / quant;

            crushed.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    // ─── Wavetable oscillator ────────────────────────────────────────

    /// Number of single-cycle waveforms in the wavetable bank.
    const WT_COUNT: usize = 8;
    /// Samples per waveform.
    const WT_SIZE: usize = 256;

    /// Initialize built-in wavetable (8 additive waveforms, sine→saw).
    pub fn init_wavetable(&mut self, morph: f32) {
        self.wavetable_morph = morph.clamp(0.0, 1.0);
        self.reclaim_buffers();

        let wt_frames = Self::WT_COUNT;
        let wt_frame_size = Self::WT_SIZE;
        let total = wt_frames * wt_frame_size;
        let mut table = Self::take_buf(&mut self.pool_a, total);

        let harmonics: [usize; 8] = [1, 2, 3, 4, 6, 8, 12, 16];
        for (wt, &n_harm) in harmonics.iter().enumerate() {
            let offset = wt * wt_frame_size;
            let mut peak = 0.0_f32;
            for i in 0..wt_frame_size {
                let p = i as f32 / wt_frame_size as f32;
                let mut s = 0.0f32;
                for h in 1..=n_harm {
                    s += (p * h as f32 * TAU).sin() / h as f32;
                }
                table[offset + i] = s;
                peak = peak.max(s.abs());
            }
            if peak > 0.001 {
                for x in &mut table[offset..offset + wt_frame_size] { *x /= peak; }
            }
        }

        self.state = OscState::Wavetable { phase: 0.0, table: Arc::new(table), wt_frames, wt_frame_size };
    }

    /// Initialize wavetable from external .wt data (parsed Surge wavetable).
    /// `data` = flat array of [wt_frames * wt_frame_size] samples.
    /// Takes `Arc` so the (potentially MB-sized) buffer is shared, not copied.
    pub fn init_wavetable_from_data(&mut self, morph: f32, data: Arc<Vec<f32>>, wt_frames: usize, wt_frame_size: usize) {
        self.wavetable_morph = morph.clamp(0.0, 1.0);
        self.reclaim_buffers();
        self.state = OscState::Wavetable { phase: 0.0, table: data, wt_frames, wt_frame_size };
    }

    fn tick_wavetable(&mut self, freq: f32) -> f32 {
        if let OscState::Wavetable { phase, table, wt_frames, wt_frame_size } = &mut self.state {
            let dt = freq * (1.0 + self.detune) / self.sample_rate;
            let morph = self.wavetable_morph;
            let frames = *wt_frames;
            let fsize = *wt_frame_size;

            let pos = morph * (frames - 1) as f32;
            let t0 = (pos.floor() as usize).min(frames - 1);
            let t1 = (t0 + 1).min(frames - 1);
            let tf = pos.fract();

            let p = *phase * fsize as f32;
            let i0 = p as usize % fsize;
            let i1 = (i0 + 1) % fsize;
            let frac = p.fract();

            let s0 = table[t0 * fsize + i0] * (1.0 - frac) + table[t0 * fsize + i1] * frac;
            let s1 = table[t1 * fsize + i0] * (1.0 - frac) + table[t1 * fsize + i1] * frac;

            *phase += dt;
            *phase -= phase.floor();

            s0 * (1.0 - tf) + s1 * tf
        } else {
            0.0
        }
    }

    // ─── FM3 (3-operator stacked FM) ─────────────────────────────────

    fn tick_fm3(&mut self, freq: f32) -> f32 {
        if let OscState::Fm3 { carrier_phase, mod1_phase, mod2_phase } = &mut self.state {
            let sr = self.sample_rate;
            let dt = freq / sr;
            let fm_ratio = self.fm_ratio;   // mod1 frequency ratio
            let fm_index = self.fm_index;   // modulation index (mod1→carrier)

            // op3: secondary modulator at fm_ratio * 1.5 (harmonic)
            let mod2_ratio = fm_ratio * 1.5;
            let mod2_index = fm_index * 0.4; // secondary modulation depth

            let mod2 = (*mod2_phase * TAU).sin();
            let mod1 = ((*mod1_phase + mod2_index * mod2) * TAU).sin();
            let out = ((*carrier_phase + fm_index * mod1) * TAU).sin();

            *mod2_phase += freq * mod2_ratio / sr;
            *mod2_phase -= mod2_phase.floor();
            *mod1_phase += freq * fm_ratio / sr;
            *mod1_phase -= mod1_phase.floor();
            *carrier_phase += dt;
            *carrier_phase -= carrier_phase.floor();

            out
        } else {
            0.0
        }
    }

    // ─── Window oscillator ────────────────────────────────────────────

    fn tick_window(&mut self, freq: f32) -> f32 {
        if let OscState::Window { phase, window_type, morph, formant } = &mut self.state {
            let dt = freq / self.sample_rate;
            *phase += dt;
            if *phase >= 1.0 { *phase -= 1.0; }

            // Formant control: scale the inner waveform independently
            let formant_mult = 2.0_f32.powf(*formant / 12.0);
            let inner_phase = (*phase * formant_mult).fract();

            // Base waveform: saw
            let base = 2.0 * inner_phase - 1.0;

            // Window function (applied to the outer phase)
            let wp = *phase;
            let w0 = match *window_type {
                0 => { // Triangle
                    if wp < 0.5 { wp * 2.0 } else { 2.0 - wp * 2.0 }
                }
                1 => { // Cosine
                    0.5 - 0.5 * (wp * TAU).cos()
                }
                2 => { // Half-sine
                    (wp * PI).sin()
                }
                3 => { // Hann
                    0.5 * (1.0 - (wp * TAU).cos())
                }
                _ => 1.0,
            };

            // Morphing: blend between current and next window
            let next_type = (*window_type + 1) % 4;
            let w1 = match next_type {
                0 => { if wp < 0.5 { wp * 2.0 } else { 2.0 - wp * 2.0 } }
                1 => { 0.5 - 0.5 * (wp * TAU).cos() }
                2 => { (wp * PI).sin() }
                3 => { 0.5 * (1.0 - (wp * TAU).cos()) }
                _ => 1.0,
            };

            let window = w0 * (1.0 - *morph) + w1 * *morph;

            (base * window).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }
}

// ─── Plaits / Twist ────────────────────────────────────────────────

impl Oscillator {
    /// Initialise Plaits voice. Called from voice.rs note_on for OscType::Twist.
    #[allow(clippy::too_many_arguments)]
    pub fn init_twist(
        &mut self,
        engine: u32,
        harmonics: f32,
        timbre: f32,
        morph: f32,
        lpg_decay: f32,
        lpg_colour: f32,
        aux_mix: f32,
    ) {
        self.twist_engine    = engine;
        self.twist_harmonics = harmonics;
        self.twist_timbre    = timbre;
        self.twist_morph     = morph;
        self.twist_lpg_decay = lpg_decay;
        self.twist_lpg_colour= lpg_colour;
        self.twist_aux_mix   = aux_mix;

        // Create voice if not yet initialised (lazy, at 48kHz per Plaits spec)
        if self.twist_voice.is_none() {
            self.twist_voice = Some(TwistVoice::new(48000.0));
        }

        self.state = OscState::Twist {
            buf_out: [0.0; TWIST_BLOCK],
            buf_aux: [0.0; TWIST_BLOCK],
            pos: TWIST_BLOCK, // force render immediately on first tick
            note: 60.0,
        };
    }

    /// Sample-by-sample tick for Plaits. Renders a block internally when buffer
    /// is exhausted and returns one sample.
    #[inline]
    fn tick_twist(&mut self, freq: f32) -> f32 {
        let voice = match &mut self.twist_voice {
            Some(v) => &mut v.0,
            None => return 0.0,
        };

        // Convert Hz → MIDI note (Plaits uses MIDI note number)
        let midi_note = 69.0 + 12.0 * (freq / 440.0).log2();

        if let OscState::Twist { buf_out, buf_aux, pos, note } = &mut self.state {
            // Re-render block when exhausted
            if *pos >= TWIST_BLOCK {
                use mi_plaits_dsp::voice::{Patch, Modulations};
                let patch = Patch {
                    note: midi_note,
                    harmonics: self.twist_harmonics,
                    timbre: self.twist_timbre,
                    morph: self.twist_morph,
                    frequency_modulation_amount: 0.0,
                    timbre_modulation_amount: 0.0,
                    morph_modulation_amount: 0.0,
                    engine: self.twist_engine as usize,
                    decay: self.twist_lpg_decay,
                    lpg_colour: self.twist_lpg_colour,
                };
                let mods = Modulations::default();
                voice.render(&patch, &mods, buf_out, buf_aux);
                *pos = 0;
                *note = midi_note;
            }

            let out = buf_out[*pos];
            let aux = buf_aux[*pos];
            *pos += 1;

            // Blend main + aux outputs
            let mix = self.twist_aux_mix;
            out * (1.0 - mix) + aux * mix
        } else {
            0.0
        }
    }
}

// ─── Free functions ────────────────────────────────────────────────

/// Soft clipping / saturation: tanh approximation for growl/fret buzz.
#[inline]
#[allow(dead_code)]
fn soft_clip(x: f32) -> f32 {
    if x.abs() < 0.5 {
        x
    } else {
        x.signum() * (1.0 - fast_exp(-x.abs() * 2.0)) * 0.5 + x * 0.5
    }
}
