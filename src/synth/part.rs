//! Part engine (S4-synth 3b): per-part voice pool, control-rate mod
//! precompute (`Part::update_mods`), engine-global params and CC pickup
//! state. Move-only out of `mod.rs` — bodies verbatim; visibility widened
//! to `pub(super)` where the parent (`mod.rs`) reads fields/methods.

use super::events::{NOTE_MODE_POLY, NOTE_MODE_ROOT, RootDecision, effective_mw, effective_pb};
use super::lfo::{Lfo, LfoWaveform};
use super::mod_matrix::{self, ModMatrix};
use super::mseg::{Mseg, MsegState};
use super::step_seq;
use super::voice::{Voice, VoiceParams};
use super::sampler::SamplerEngine;
use super::PatchParams;
use std::sync::Arc;

/// Modulation state computed per-part, passed to voices.
#[derive(Clone, Copy)]
pub struct ModulationState {
    pub pitch_mult: f32,
    pub filter_offset: f32,       // Hz (from LFOs, aftertouch)
    pub filter_offset_semis: f32, // semitones (from mod matrix — applied as 2^(s/12) multiplier)
    pub amp_mod: f32,
}

pub const MAX_PARTS: usize = 16;
const VOICES_PER_LAYER: usize = 8;

/// Global parameters — only truly global controls that persist across everything.
pub(super) struct GlobalParams {
    pub(super) master_volume: f32,
    pub(super) master_tone: f32, // simple LP cutoff (20-20000 Hz), post-effects
    // Fader-controlled global (persist across patch changes)
    pub(super) reverb_mix: Option<f32>,       // None = use preset value
    pub(super) delay_mix: Option<f32>,        // None = use preset value
    pub(super) pitch_bend_range: f32,         // semitones, default 2 (used when up/down not set)
    pub(super) pitch_bend_up: f32,            // semitones up (0 = use pitch_bend_range)
    pub(super) pitch_bend_down: f32,          // semitones down (0 = use pitch_bend_range)
    // One-pole LP state for master tone
    pub(super) tone_lp_l: f32,
    pub(super) tone_lp_r: f32,
}

impl Default for GlobalParams {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            master_tone: 20000.0,
            reverb_mix: None,
            delay_mix: None,
            pitch_bend_range: 2.0,
            pitch_bend_up: 0.0,
            pitch_bend_down: 0.0,
            tone_lp_l: 0.0,
            tone_lp_r: 0.0,
        }
    }
}

impl GlobalParams {
    pub(super) fn set(&mut self, key: &str, value: f32) {
        match key {
            "master_volume" => self.master_volume = value,
            "master_tone" => self.master_tone = value,
            "reverb_mix" => self.reverb_mix = Some(value),
            "delay_mix" => self.delay_mix = Some(value),
            "pitch_bend_range" => self.pitch_bend_range = value.clamp(1.0, 24.0),
            "pitch_bend_up"    => self.pitch_bend_up    = value.clamp(0.0, 48.0),
            "pitch_bend_down"  => self.pitch_bend_down  = value.clamp(0.0, 48.0),
            _ => {
                debug_assert!(false, "GlobalParams::set: unknown key '{key}'");
            }
        }
    }

    /// Apply master tone (one-pole LP) to output. Cheap, no resonance.
    #[inline]
    pub(super) fn apply_tone(&mut self, l: f32, r: f32, sample_rate: f32) -> (f32, f32) {
        let freq = self.master_tone.clamp(20.0, 20000.0);
        if freq >= 19000.0 { return (l, r); } // bypass when wide open
        let coeff = (std::f32::consts::PI * freq / sample_rate).sin().min(0.999);
        self.tone_lp_l += coeff * (l - self.tone_lp_l);
        self.tone_lp_r += coeff * (r - self.tone_lp_r);
        (self.tone_lp_l, self.tone_lp_r)
    }
}

/// Pickup state for a single CC — prevents jumps on patch change.
#[derive(Clone, Copy)]
pub(super) struct PickupState {
    pub(super) last_cc_value: Option<u8>,
    pub(super) picked_up: bool,
}

impl Default for PickupState {
    fn default() -> Self {
        Self { last_cc_value: None, picked_up: true }
    }
}

pub(super) struct Part {
    pub(super) voices: Vec<Voice>,
    pub(super) age_counter: u64,
    pub(super) volume: f32,
    pub(super) enabled: bool,   // part is in scene (has patch, receives MIDI)
    pub(super) mute: bool,      // temporary silence without removing from scene
    /// Mix gate for mute: 1.0 open; ramps to 0 over ~5 ms when muted, back to
    /// 1.0 on unmute (click-free). Per-sample, audio thread only.
    pub(super) mute_gain: f32,
    pub(super) min_note: u8,
    pub(super) max_note: u8,
    pub(super) vel_min: u8,     // velocity range (1-127)
    pub(super) vel_max: u8,
    pub(super) pan: f32,        // -1.0 (L) .. 0.0 (C) .. +1.0 (R)
    pub(super) transpose: i8,   // semitone offset per part (-24..+24)
    pub(super) params: PatchParams,
    pub(super) lfos: [Lfo; 4],
    /// Scene LFOs — free-running, never reset on note-on (unlike voice LFOs).
    /// tick every block regardless of note activity.
    pub(super) scene_lfos: [Lfo; 2],
    pub(super) scene_lfo_routed: [bool; 2],  // true when used in mod matrix
    /// External wavetable loaded from .wt file (set via LoadPatch)
    pub(super) wavetable_data: Option<Arc<Vec<f32>>>,
    pub(super) wavetable_frames: usize,
    pub(super) wavetable_frame_size: usize,
    /// Current macro knob values (0..1). Updated by SetMacro event.
    pub(super) macro_vals: [f32; 8],
    pub(super) mod_matrix: ModMatrix,
    pub(super) mseg1: Mseg,
    pub(super) mseg1_state: MsegState,
    pub(super) mseg2: Mseg,
    pub(super) mseg2_state: MsegState,
    pub(super) last_note_freq: Option<f32>,
    pub(super) last_velocity: f32,
    pub(super) last_note: u8,
    pub(super) sf2_mode: bool,
    pub(super) pitch_seq: step_seq::PitchSequencer,
    // Mono / Latch voice mode state
    pub(super) note_stack: Vec<u8>,   // held notes for mono mode (newest last)
    pub(super) latch_held: Vec<u8>,   // latched notes (toggle mode)
    // Random/Alternate mod source state
    pub(super) rng_state: u64,         // LCG PRNG state
    pub(super) alt_counter: u32,       // alternation counter (even=+, odd=-)
    pub(super) rand_bipolar: f32,      // current random bipolar value
    pub(super) rand_unipolar: f32,     // current random unipolar value
    pub(super) alt_bipolar: f32,       // current alternate bipolar value
    pub(super) alt_unipolar: f32,      // current alternate unipolar value
    pub(super) release_vel: f32,       // last note-off velocity
    pub(super) sustained_notes: [bool; 128], // notes held by sustain pedal
    pub(super) keys_down: [bool; 128],   // raw key physically held (pedal-up must spare these)
    /// Note mode: NOTE_MODE_POLY / NOTE_MODE_BASS (the old root-only) /
    /// NOTE_MODE_ROOT (chord root via detect_root). Gates note routing
    /// through root_on/root_off unless Poly.
    pub(super) note_mode: u8,
    /// Sustain pedal (CC64) affects this part. false: bass without sustain
    /// while a pad part keeps sustaining.
    pub(super) sustain_enabled: bool,
    /// SF2 mono: последняя играющая нота (play_mode 1/2). SF2-ветка
    /// route_note_on минует DSP mono-логику — моно для сэмплера делаем
    /// здесь: новая нота глушит предыдущую (#55-фидбек «моно не реагирует
    /// на SF2»).
    pub(super) sf2_mono_stack: Vec<(u8, u8)>,
    /// Part ignores incoming Pitch Bend (TECHDEBT #55) — чужие контроллеры
    /// шлют мусорный PB, слои не должны дёргаться.
    pub(super) ignore_pb: bool,
    /// Part ignores incoming Mod Wheel (TECHDEBT #55).
    pub(super) ignore_mw: bool,
    pub(super) root_held: [bool; 128],   // keys currently physically held, for re-keying
    pub(super) root_active: Option<u8>,  // raw note currently sounding as the root
    pub(super) breath: f32,            // CC2, 0..1
    pub(super) expression: f32,        // CC11, 0..1
    pub(super) sustain_pedal: f32,     // CC64: 0.0 or 1.0
    pub(super) lowest_held: u8,        // lowest active note (default 60)
    pub(super) highest_held: u8,       // highest active note (default 60)
    pub(super) poly_at_in_matrix: bool,  // true if any mod slot uses PolyAftertouch
    // Cached routing flags — updated on patch load, not per-block
    pub(super) lfo1_routed: bool,
    pub(super) lfo2_routed: bool,
    pub(super) lfo3_routed: bool,
    pub(super) lfo4_routed: bool,
}

/// Control-rate входы движка для мод-прекомпьюта (Copy; S4-synth 2).
#[derive(Clone, Copy)]
pub struct EngineModInputs {
    pub mod_wheel: f32,
    pub aftertouch_smooth: f32,
    pub cc_values: [f32; 4],
    pub pitch_bend_semitones: f32,
    pub seq_bpm: f32,
    pub block_len: usize,
    pub sample_rate: f32,
    /// Сквозной инпут: lfo_rate ПЕРВОЙ включённой части (cross-part,
    /// tick_block) — Part не может пересчитать сам.
    pub lfo_rate: f32,
    /// Готовое значение вибрато LFO (фаза vibrato_phase живёт в engine,
    /// инкремент остаётся в tick_block).
    pub vibrato_val: f32,
}

impl Part {
    pub(super) fn new(sample_rate: f32) -> Self {
        let voices = (0..VOICES_PER_LAYER).map(|_| Voice::new(sample_rate)).collect();
        Self {
            voices, age_counter: 0, volume: 0.8, enabled: false, mute: false, mute_gain: 1.0,
            min_note: 0, max_note: 127, vel_min: 1, vel_max: 127,
            pan: 0.0, transpose: 0,
            params: PatchParams::default(),
            lfos: [Lfo::new(sample_rate), Lfo::new(sample_rate), Lfo::new(sample_rate), Lfo::new(sample_rate)],
            scene_lfos: [Lfo::new(sample_rate), Lfo::new(sample_rate)],
            scene_lfo_routed: [false; 2],
            wavetable_data: None,
            wavetable_frames: 0,
            wavetable_frame_size: 0,
            macro_vals: [0.0; 8],
            mod_matrix: ModMatrix::default(),
            mseg1: Mseg::default(), mseg1_state: MsegState::new(sample_rate),
            mseg2: Mseg::default(), mseg2_state: MsegState::new(sample_rate),
            last_note_freq: None, last_velocity: 1.0, last_note: 60,
            sf2_mode: false, pitch_seq: step_seq::PitchSequencer::new(),
            note_stack: Vec::new(), latch_held: Vec::new(),
            rng_state: 0x5851f42d4c957f2d, alt_counter: 0,
            rand_bipolar: 0.0, rand_unipolar: 0.5,
            alt_bipolar: 1.0, alt_unipolar: 1.0, release_vel: 0.0,
            sustained_notes: [false; 128],
            sf2_mono_stack: Vec::new(),
            keys_down: [false; 128],
            note_mode: NOTE_MODE_POLY, root_held: [false; 128], root_active: None,
            sustain_enabled: true,
            ignore_pb: false, ignore_mw: false,
            breath: 0.0, expression: 1.0, sustain_pedal: 0.0,
            lowest_held: 60, highest_held: 60,
            poly_at_in_matrix: false,
            lfo1_routed: false,
            lfo2_routed: false,
            lfo3_routed: false,
            lfo4_routed: false,
        }
    }

    /// Update cached routing flags from mod matrix slots. Call after loading a patch
    /// or changing mod matrix routing to avoid per-block scanning.
    pub(super) fn update_routing_cache(&mut self) {
        self.lfo1_routed = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::Lfo1 && s.depth.abs() > 0.001);
        self.lfo2_routed = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::Lfo2 && s.depth.abs() > 0.001);
        self.lfo3_routed = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::Lfo3 && s.depth.abs() > 0.001);
        self.lfo4_routed = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::Lfo4 && s.depth.abs() > 0.001);
        self.poly_at_in_matrix = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::PolyAftertouch && s.depth.abs() > 0.001);
        self.scene_lfo_routed[0] = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::SceneLfo1 && s.depth.abs() > 0.001);
        self.scene_lfo_routed[1] = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::SceneLfo2 && s.depth.abs() > 0.001);
    }

    /// LCG random float 0..1
    fn next_rand(&mut self) -> f32 {
        self.rng_state = self.rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.rng_state >> 33) as f32) / (u32::MAX as f32)
    }

    /// Start a voice from a RAW (pre-transpose) MIDI note. Transposition is
    /// applied internally: `voice.note` keeps the sounding pitch, `voice.src_note`
    /// keeps the raw key so note-offs match across transpose changes.
    pub(super) fn note_on(&mut self, raw_note: u8, velocity: u8) {
        if !self.enabled { return; }
        let note = (raw_note as i16 + self.transpose as i16).clamp(0, 127) as u8;
        if note < self.min_note || note > self.max_note { return; }
        debug_assert!(raw_note < 128);
        self.keys_down[raw_note as usize] = true;
        self.last_velocity = velocity as f32 / 127.0;
        self.last_note = note;

        // Update random/alternate mod sources on each note-on
        let r = self.next_rand();
        self.rand_bipolar = r * 2.0 - 1.0;
        self.rand_unipolar = r;
        self.alt_counter = self.alt_counter.wrapping_add(1);
        let is_even = self.alt_counter.is_multiple_of(2);
        self.alt_bipolar = if is_even { 1.0 } else { -1.0 };
        self.alt_unipolar = if is_even { 1.0 } else { 0.0 };

        let play_mode = self.params.play_mode as u8;

        // Latch mode: toggle note on/off
        if play_mode == 3 {
            if let Some(pos) = self.latch_held.iter().position(|&n| n == raw_note) {
                self.latch_held.remove(pos);
                self.keys_down[raw_note as usize] = false;
                for v in &mut self.voices { if v.active && v.src_note == raw_note { v.note_off(); } }
            } else {
                self.latch_held.push(raw_note);
                // fall through to start voice below
            }
            if self.latch_held.contains(&raw_note) {
                // Start voice for new latch note
                self.age_counter += 1;
                let age = self.age_counter;
                let idx = self.voices.iter().position(|v| !v.active).unwrap_or_else(|| {
                    self.voices.iter().enumerate().min_by_key(|(_, v)| v.age).map(|(i, _)| i).unwrap_or(0)
                });
                let prev_freq = self.last_note_freq;
                let voice_params = self.build_voice_params();
                self.voices[idx].note_on(note, raw_note, velocity, age, &voice_params, prev_freq);
                self.last_note_freq = Some(440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0));
                self.lfos[0].trigger_mode(self.params.lfo1_trigger_mode as u8);
                self.lfos[1].trigger_mode(self.params.lfo2_trigger_mode as u8);
                self.lfos[2].trigger_mode(self.params.lfo3_trigger_mode as u8);
                self.lfos[3].trigger_mode(self.params.lfo4_trigger_mode as u8);
                if self.params.mseg_enabled > 0.5 { self.mseg1_state.trigger(); self.mseg2_state.trigger(); }
            }
            self.update_held_range();
            return;
        }

        // Mono modes (1=mono, 2=mono-st single trigger, 3 handled above)
        if play_mode == 1 || play_mode == 2 {
            self.note_stack.retain(|&n| n != raw_note);
            self.note_stack.push(raw_note);
            self.age_counter += 1;
            let age = self.age_counter;
            // Retrigger the single mono voice
            let idx = self.voices.iter().position(|v| v.active).unwrap_or_else(|| {
                self.voices.iter().position(|v| !v.active).unwrap_or(0)
            });
            // Same gating as the poly path: portamento_mode 0 (Off) never
            // glides; 2 (Fingered) glides only while another key is held
            // (the new note is already pushed, so len > 1 means a key was
            // held); 1 (Always) glides from the last pitch.
            let porta_mode = self.params.portamento_mode as u8;
            let prev_freq = match porta_mode {
                1 => self.last_note_freq,
                2 => if self.note_stack.len() > 1 { self.last_note_freq } else { None },
                _ => None,
            };
            let voice_params = self.build_voice_params();
            let retrigger = play_mode == 2 && self.note_stack.len() > 1; // ST: don't retrigger env
            if retrigger {
                self.voices[idx].retrigger_note(note, raw_note, &voice_params);
            } else {
                self.voices[idx].note_on(note, raw_note, velocity, age, &voice_params, prev_freq);
            }
            self.last_note_freq = Some(440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0));
            if !retrigger {
                self.lfos[0].trigger_mode(self.params.lfo1_trigger_mode as u8);
                self.lfos[1].trigger_mode(self.params.lfo2_trigger_mode as u8);
                self.lfos[2].trigger_mode(self.params.lfo3_trigger_mode as u8);
                self.lfos[3].trigger_mode(self.params.lfo4_trigger_mode as u8);
            }
            if self.params.mseg_enabled > 0.5 && !retrigger { self.mseg1_state.trigger(); self.mseg2_state.trigger(); }
            self.update_held_range();
            return;
        }

        // Piano mode: if same key is already playing, don't retrigger — let it continue
        if play_mode == 6 && self.voices.iter().any(|v| v.active && v.src_note == raw_note) {
            return;
        }

        self.age_counter += 1;
        let age = self.age_counter;
        let idx = self.voices.iter().position(|v| !v.active).unwrap_or_else(|| {
            match play_mode {
                4 => {
                    // Poly-High: steal lowest note (highest note wins)
                    self.voices.iter().enumerate()
                        .filter(|(_, v)| v.active)
                        .min_by_key(|(_, v)| v.note)
                        .map(|(i, _)| i)
                        .unwrap_or(0)
                }
                5 => {
                    // Poly-Low: steal highest note (lowest note wins)
                    self.voices.iter().enumerate()
                        .filter(|(_, v)| v.active)
                        .max_by_key(|(_, v)| v.note)
                        .map(|(i, _)| i)
                        .unwrap_or(0)
                }
                _ => {
                    // Default (Poly / Piano / Latch): JUCE-style steal order —
                    // voices already in release phase die first (their tails
                    // are almost done), then the oldest sustained voice.
                    let releasing = self.voices.iter().enumerate()
                        .filter(|(_, v)| v.active && v.is_releasing())
                        .min_by_key(|(_, v)| v.age);
                    releasing
                        .or_else(|| self.voices.iter().enumerate()
                            .min_by_key(|(_, v)| v.age))
                        .map(|(i, _)| i)
                        .unwrap_or(0)
                }
            }
        });
        let porta_mode = self.params.portamento_mode as u8;
        let prev_freq = match porta_mode {
            1 => self.last_note_freq,
            2 => if self.has_active_notes() { self.last_note_freq } else { None },
            _ => None,
        };
        let voice_params = self.build_voice_params();
        self.voices[idx].note_on(note, raw_note, velocity, age, &voice_params, prev_freq);
        self.last_note_freq = Some(440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0));
        // Retrigger LFOs on note-on if flagged
        self.lfos[0].trigger_mode(self.params.lfo1_trigger_mode as u8);
        self.lfos[1].trigger_mode(self.params.lfo2_trigger_mode as u8);
        self.lfos[2].trigger_mode(self.params.lfo3_trigger_mode as u8);
        self.lfos[3].trigger_mode(self.params.lfo4_trigger_mode as u8);
        // Trigger MSEGs on note-on (per-part, not per-voice)
        if self.params.mseg_enabled > 0.5 {
            self.mseg1_state.trigger();
            self.mseg2_state.trigger();
        }
        self.update_held_range();
    }

    fn build_voice_params(&self) -> VoiceParams {
        VoiceParams {
            osc_type: self.params.osc_type, osc_detune: self.params.osc_detune,
            fm_ratio: self.params.fm_ratio, fm_index: self.params.fm_index,
            fm_env_amount: self.params.fm_env_amount,
            osc_count: self.params.osc_count, osc1_level: self.params.osc1_level,
            osc2_type: self.params.osc2_type, osc2_detune: self.params.osc2_detune, osc2_level: self.params.osc2_level,
            osc3_type: self.params.osc3_type, osc3_detune: self.params.osc3_detune, osc3_level: self.params.osc3_level,
            filter_cutoff: self.params.filter_cutoff, filter_resonance: self.params.filter_resonance,
            filter_type: self.params.filter_type, filter_env_amount: self.params.filter_env_amount,
            filter_key_track: self.params.filter_key_track,
            filter_routing: self.params.filter_routing, filter2_type: self.params.filter2_type,
            filter2_cutoff: self.params.filter2_cutoff, filter2_resonance: self.params.filter2_resonance,
            noise_level: self.params.noise_level,
            amp_attack: self.params.amp_attack, amp_decay: self.params.amp_decay,
            amp_sustain: self.params.amp_sustain, amp_release: self.params.amp_release,
            amp_hold: self.params.amp_hold,
            filter_attack: self.params.filter_attack, filter_decay: self.params.filter_decay,
            filter_sustain: self.params.filter_sustain, filter_release: self.params.filter_release,
            filter_hold: self.params.filter_hold,
            ks_brightness: self.params.ks_brightness, ks_feedback: self.params.ks_feedback,
            organ_drawbars: self.params.organ_drawbars,
            formant_voice: self.params.formant_voice, formant_vowel: self.params.formant_vowel,
            drum_pitch_amount: self.params.drum_pitch_amount, drum_pitch_decay: self.params.drum_pitch_decay,
            drum_noise_level: self.params.drum_noise_level, drum_noise_decay: self.params.drum_noise_decay,
            drum_noise_color: self.params.drum_noise_color,
            bass_style: self.params.bass_style, bass_tone: self.params.bass_tone, bass_body: self.params.bass_body, bass_pickup: self.params.bass_pickup,
            bow_pressure: self.params.bow_pressure, bow_position: self.params.bow_position, body_type: self.params.body_type,
            lip_tension: self.params.lip_tension, blowing_pressure: self.params.blowing_pressure, bell_type: self.params.bell_type,
            pd_shape: self.params.pd_shape, pd_depth: self.params.pd_depth, pd_env_amount: self.params.pd_env_amount,
            fold_amount: self.params.fold_amount, fold_symmetry: self.params.fold_symmetry, fold_source: self.params.fold_source,
            modal_material: self.params.modal_material, modal_brightness: self.params.modal_brightness,
            modal_damping: self.params.modal_damping, modal_strike_pos: self.params.modal_strike_pos,
            sync_ratio: self.params.sync_ratio, sync_shape: self.params.sync_shape,
            supersaw_detune: self.params.supersaw_detune, supersaw_mix: self.params.supersaw_mix,
            pulse_width: self.params.pulse_width,
            accordion_register: self.params.accordion_register, accordion_bellows: self.params.accordion_bellows,
            sax_reed_stiffness: self.params.sax_reed_stiffness, sax_embouchure: self.params.sax_embouchure,
            sax_blow_pressure: self.params.sax_blow_pressure, sax_type: self.params.sax_type,
            epiano_type: self.params.epiano_type,
            alias_wave_type: self.params.alias_wave_type, alias_crush: self.params.alias_crush,
            window_type: self.params.window_type, window_morph: self.params.window_morph,
            window_formant: self.params.window_formant,
            wavetable_data: self.wavetable_data.clone(),
            wavetable_frames: self.wavetable_frames,
            wavetable_frame_size: self.wavetable_frame_size,
            twist_engine: self.params.twist_engine as u32,
            twist_harmonics: self.params.twist_harmonics,
            twist_timbre: self.params.twist_timbre,
            twist_morph: self.params.twist_morph,
            twist_lpg_decay: self.params.twist_lpg_decay,
            twist_lpg_colour: self.params.twist_lpg_colour,
            twist_aux_mix: self.params.twist_aux_mix,
            env_attack_shape: self.params.env_attack_shape, env_decay_shape: self.params.env_decay_shape,
            env_release_shape: self.params.env_release_shape,
            filter_env_attack_shape: self.params.filter_env_attack_shape,
            filter_env_decay_shape: self.params.filter_env_decay_shape,
            filter_env_release_shape: self.params.filter_env_release_shape,
            velocity_curve: self.params.velocity_curve, vel_to_filter: self.params.vel_to_filter,
            portamento_time: self.params.portamento_time, _portamento_mode: self.params.portamento_mode,
            unison_voices: self.params.unison_voices, unison_detune: self.params.unison_detune,
            unison_spread: self.params.unison_spread,
            fm_cross_depth: self.params.fm_cross_depth,
            filter_env_semitones: self.params.filter_env_semitones,
            osc_ws_mode: self.params.osc_ws_mode as u32,
            osc_ws_drive: self.params.osc_ws_drive * 4.0 + 0.5,
            osc_ws_mix: self.params.osc_ws_mix,
            inter_ws_mode: self.params.inter_ws_mode as u32,
            inter_ws_drive: self.params.inter_ws_drive * 4.0 + 0.5,
            inter_ws_mix: self.params.inter_ws_mix,
            svf_morph: self.params.svf_morph,
            filter_drive: self.params.filter_drive,
            filter_starve: self.params.filter_starve,
        }
    }

    /// Release voices matching the RAW (pre-transpose) note — see `note_on`.
    pub(super) fn note_off(&mut self, raw_note: u8) {
        self.keys_down[raw_note as usize] = false;
        let note = (raw_note as i16 + self.transpose as i16).clamp(0, 127) as u8;
        let play_mode = self.params.play_mode as u8;

        // Latch mode: notes released by toggling via note_on, not note_off
        if play_mode == 3 { return; }

        // Mono modes: manage note stack
        if play_mode == 1 || play_mode == 2 {
            self.note_stack.retain(|&n| n != raw_note);
            if let Some(&prev_note) = self.note_stack.last() {
                // Retrigger with the previous held note
                let vp = self.build_voice_params();
                let prev_transposed = (prev_note as i16 + self.transpose as i16).clamp(0, 127) as u8;
                if let Some(v) = self.voices.iter_mut().find(|v| v.active) {
                    v.retrigger_note(prev_transposed, prev_note, &vp);
                    if play_mode == 1 { v.retrigger_envelope(); }
                }
            } else {
                for voice in &mut self.voices {
                    if voice.active && voice.src_note == raw_note { voice.note_off(); }
                }
                if !self.has_active_notes() {
                    self.mseg1_state.release();
                    self.mseg2_state.release();
                }
            }
            self.update_held_range();
            return;
        }

        self.release_vel = 0.5; // default; would need MIDI note-off velocity from caller

        if self.sustain_pedal > 0.5 && self.sustain_enabled {
            match self.params.sustain_mode as u32 {
                0 => {
                    // Hold all notes — don't release while pedal is down
                    self.sustained_notes[raw_note as usize] = true;
                }
                1 => {
                    // Release note only if other active non-releasing notes are held
                    let other_active = self.voices.iter()
                        .any(|v| v.note != note && v.active && !v.is_releasing());
                    if other_active {
                        for voice in &mut self.voices {
                            if voice.active && voice.src_note == raw_note { voice.note_off(); }
                        }
                    } else {
                        // Held by pedal — must be flagged so pedal-up releases it
                        self.sustained_notes[raw_note as usize] = true;
                    }
                }
                _ => {
                    // Default: hold all
                    self.sustained_notes[raw_note as usize] = true;
                }
            }
        } else {
            for voice in &mut self.voices {
                if voice.active && voice.src_note == raw_note { voice.note_off(); }
            }
        }

        // Release MSEGs when no active held notes remain
        if !self.has_active_notes() {
            self.mseg1_state.release();
            self.mseg2_state.release();
        }
        self.update_held_range();
    }

    /// Release all voices playing `note` with an explicit amp-release duration
    /// (matches by voice.note — the caller builds its needs bitmap by note).
    /// Used for the static pedal-up release (TECHDEBT #7).
    fn note_off_for(&mut self, note: u8, seconds: f32) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note { voice.note_off_for(seconds); }
        }
    }

    fn has_active_notes(&self) -> bool {
        self.voices.iter().any(|v| v.active && !v.is_releasing())
    }

    /// Note-mode gate, note-on side. Called for EVERY in-range note-on routed
    /// to this part (both engines) via SynthEngine::route_note_on.
    pub(super) fn root_on(&mut self, raw: u8) -> RootDecision {
        self.root_held[raw as usize] = true;
        // Latch mode is NOT gated: latch toggles on note-on and the gate would
        // break the toggle contract (tap-off suppressed under root-only).
        if self.note_mode == NOTE_MODE_POLY || self.params.play_mode as u8 == 3 {
            return RootDecision::Sound { release: None, note: raw };
        }
        // Стек-буфер вместо Vec: root_on на аудио-потоке — аллокации запрещены.
        let mut buf = [0u8; 128];
        let mut n = 0usize;
        for (k, &held_flag) in self.root_held.iter().enumerate() {
            if held_flag { buf[n] = k as u8; n += 1; }
        }
        let held = &buf[..n];
        // Якорь корня — min_held (басовый корень), не последняя нажатая:
        // корень звучит в басовой октаве и не прыгает вверх при добавлении
        // верхних нот.
        let min_held = held.iter().copied().min().unwrap_or(raw);
        let target = match self.note_mode {
            NOTE_MODE_ROOT => crate::chord::detect_root(held)
                .map(|pc| {
                    // i16-математика: `raw - pc` в u8 паникует при raw < pc;
                    // delta затем saturating_sub — без паники и обёртки.
                    let delta = (min_held as i16 - pc as i16).rem_euclid(12) as u8;
                    min_held.saturating_sub(delta)
                })
                .unwrap_or(min_held), // фолбэк: некордовый набор = нижняя
            _ => min_held, // Bass: нижняя зажатая
        };
        match self.root_active {
            Some(cur) if cur == target => RootDecision::Suppress, // дубликат/не-корневая нота
            Some(cur) => {
                self.root_active = Some(target);
                RootDecision::Sound { release: Some(cur), note: target }
            }
            None => {
                self.root_active = Some(target);
                RootDecision::Sound { release: None, note: target }
            }
        }
    }

    /// Note-mode gate, note-off side. Returns the new root to re-key to
    /// (Some only when the sounding root actually changed).
    pub(super) fn root_off(&mut self, raw: u8) -> Option<u8> {
        self.root_held[raw as usize] = false;
        if self.note_mode == NOTE_MODE_POLY { return None; }
        // Пересчитать таргет по оставшимся held (тот же стек-буфер +
        // i16/saturating-математика, что в root_on).
        let mut buf = [0u8; 128];
        let mut n = 0usize;
        for (k, &held_flag) in self.root_held.iter().enumerate() {
            if held_flag { buf[n] = k as u8; n += 1; }
        }
        let held = &buf[..n];
        let min_held = match held.iter().copied().min() {
            Some(m) => m,
            None => { self.root_active = None; return None; } // ни одной зажатой
        };
        let target = match self.note_mode {
            NOTE_MODE_ROOT => crate::chord::detect_root(held)
                .map(|pc| {
                    let delta = (min_held as i16 - pc as i16).rem_euclid(12) as u8;
                    min_held.saturating_sub(delta)
                })
                .unwrap_or(min_held),
            _ => min_held,
        };
        if self.root_active == Some(target) { return None; } // не изменился — нет re-key
        self.root_active = Some(target);
        Some(target)
    }

    /// Drains a part on state changes / panic: releases (or kills) DSP voices,
    /// releases this part's SF2 voices, and clears every held-note tracking
    /// field — pedal-held notes, keys down, mono/latch stacks, and the
    /// root-only rekey state. Without the root clear a stale root_active
    /// ghost-suppresses the next note after MIDI-stop / AllNotesOff on
    /// gated (Bass/Root) patches (see root_on/root_off). `kill`: CC120 semantics
    /// (immediate) vs release (CC123 / state change). Caller must have lifted
    /// engine-level sustain via set_sustain(0) first — this fn does not touch
    /// part.sustain_pedal. Zero-allocation (audio thread).
    pub(super) fn drain_notes(&mut self, sampler: &mut SamplerEngine, idx: usize, kill: bool) {
        sampler.part_all_notes_off(idx);
        for voice in &mut self.voices {
            if voice.active {
                if kill { voice.kill(); } else { voice.note_off(); }
            }
        }
        self.sustained_notes = [false; 128];
        self.keys_down = [false; 128];
        self.note_stack.clear();
        self.latch_held.clear();
        self.root_held = [false; 128];
        self.root_active = None;
    }

    /// Pedal-up: разжать pedal-held (не key-held) голоса парта. Общий код
    /// set_sustain и SetPartSustain (QW10: verbatim ×2 — дрейф запрещён).
    /// `secs` = static pedal-up release (#7), None = patch release.
    pub(super) fn release_pedal_held(&mut self, secs: Option<f32>) {
        // Latch voices: released unconditionally (см. latch_held контракт —
        // latched note определяется latch_held, не keys_down).
        for &n in &self.latch_held {
            self.keys_down[n as usize] = false;
        }
        if let Some(secs) = secs {
            // Two passes: needs-битмап держит borrows disjoint.
            let mut needs = [false; 128];
            for voice in &self.voices {
                if voice.active && !voice.is_releasing()
                    && !self.keys_down[voice.src_note as usize]
                {
                    needs[voice.note as usize] = true;
                }
            }
            for (note, &held) in needs.iter().enumerate() {
                if held { self.note_off_for(note as u8, secs); }
            }
        } else {
            for voice in &mut self.voices {
                if voice.active && !voice.is_releasing()
                    && !self.keys_down[voice.src_note as usize]
                {
                    voice.note_off();
                }
            }
        }
        self.sustained_notes = [false; 128];
        // Stale mono/latch entries would resurrect on the next key press.
        self.note_stack.clear();
        self.latch_held.clear();
    }

    /// Latch-mismatch: уход с play_mode==3 (latch) — снять latched голоса.
    /// Общий код ProgramChange и LoadPatch (QW10). Вызывать ДО swap params.
    pub(super) fn release_latch_mismatch(&mut self, new_play_mode: u8) {
        if self.params.play_mode as u8 == 3 && new_play_mode != 3 {
            for voice in &mut self.voices {
                if voice.active && self.latch_held.contains(&voice.src_note) {
                    voice.note_off();
                }
            }
            self.latch_held.clear();
        }
    }

    fn update_held_range(&mut self) {
        let mut lo = 255u8;
        let mut hi = 0u8;
        for v in &self.voices {
            if v.active && !v.is_releasing() {
                if v.note < lo { lo = v.note; }
                if v.note > hi { hi = v.note; }
            }
        }
        if lo > hi {
            // No active non-releasing voices — keep last known values
        } else {
            self.lowest_held = lo;
            self.highest_held = hi;
        }
    }

    pub(super) fn set_sample_rate(&mut self, sample_rate: f32) {
        for voice in &mut self.voices { voice.set_sample_rate(sample_rate); }
        for lfo in &mut self.lfos { lfo.set_sample_rate(sample_rate); }
        self.mseg1_state.set_sample_rate(sample_rate);
        self.mseg2_state.set_sample_rate(sample_rate);
    }

    pub(super) fn tick(&mut self, mods: &ModulationState) -> (f32, f32) {
        if !self.enabled { return (0.0, 0.0); }
        let (mut out_l, mut out_r) = (0.0, 0.0);
        for voice in &mut self.voices {
            // Per-voice poly aftertouch: compute only the AT delta, not full matrix
            let voice_mods = if self.poly_at_in_matrix && voice.active && voice.poly_aftertouch > 0.001 {
                let delta = self.mod_matrix.evaluate_poly_at_delta(voice.poly_aftertouch);
                let pitch_mult = if (mods.pitch_mult - 1.0).abs() < 0.001 && delta.pitch.abs() < 0.001 {
                    1.0
                } else {
                    mods.pitch_mult * if delta.pitch.abs() > 0.001 { (delta.pitch / 12.0).exp2() } else { 1.0 }
                };
                let amp_mod = mods.amp_mod * (1.0 + delta.amplitude).max(0.0);
                ModulationState {
                    pitch_mult,
                    filter_offset: mods.filter_offset,
                    filter_offset_semis: mods.filter_offset_semis + delta.filter_cutoff,
                    amp_mod,
                }
            } else {
                *mods
            };
            let (l, r) = voice.tick(&voice_mods);
            out_l += l; out_r += r;
        }
        let vol = self.volume * self.volume; // perceptual curve (quadratic)
        (out_l * vol, out_r * vol)
    }

    /// Текущее значение CC-live patch-параметра (S2a: SSOT-макрос
    /// cc_patch_params! в patch_params.rs). None = ключ не CC-live.
    pub(super) fn cc_value(&self, key: &str) -> Option<f32> {
        crate::synth::patch_params::cc_value_of(&self.params, key)
    }

    pub(super) fn set_param(&mut self, key: &str, value: f32) {
        // 31 CC-live ключ — SSOT-таблица (S2a); остальные — рукописные arm'ы
        if crate::synth::patch_params::cc_set_param(&mut self.params, key, value) {
            return;
        }
        match key {
            "filter_key_track" => self.params.filter_key_track = value,
            "delay_ping_pong" => self.params.delay_ping_pong = value,
            "ring_mod_freq" => self.params.ring_mod_freq = value,
            "ring_mod_shape" => self.params.ring_mod_shape = value,
            "ring_mod_bias" => self.params.ring_mod_bias = value,
            "ring_mod_linear" => self.params.ring_mod_linear = value,
            "ring_mod_mix" => self.params.ring_mod_mix = value,
            "freq_shift_hz" => self.params.freq_shift_hz = value,
            "freq_shift_feedback" => self.params.freq_shift_feedback = value,
            "freq_shift_delay" => self.params.freq_shift_delay = value,
            "freq_shift_mix" => self.params.freq_shift_mix = value,
            "tape_drive" => self.params.tape_drive = value,
            "tape_saturation" => self.params.tape_saturation = value,
            "tape_bias" => self.params.tape_bias = value,
            "tape_tone" => self.params.tape_tone = value,
            "tape_speed" => self.params.tape_speed = value,
            "tape_mix" => self.params.tape_mix = value,
            "neuron_drive" => self.params.neuron_drive = value,
            "neuron_squash" => self.params.neuron_squash = value,
            "neuron_stab" => self.params.neuron_stab = value,
            "neuron_asym" => self.params.neuron_asym = value,
            "neuron_bias" => self.params.neuron_bias = value,
            "neuron_comb_freq" => self.params.neuron_comb_freq = value,
            "neuron_comb_sep" => self.params.neuron_comb_sep = value,
            "neuron_mix" => self.params.neuron_mix = value,
            "spring_size" => self.params.spring_size = value,
            "spring_decay" => self.params.spring_decay = value,
            "spring_reflections" => self.params.spring_reflections = value,
            "spring_damping" => self.params.spring_damping = value,
            "spring_spin" => self.params.spring_spin = value,
            "spring_chaos" => self.params.spring_chaos = value,
            "spring_mix" => self.params.spring_mix = value,
            "reverb_type" => self.params.reverb_type = value,
            "env_attack_shape" => self.params.env_attack_shape = value,
            "env_decay_shape" => self.params.env_decay_shape = value,
            "env_release_shape" => self.params.env_release_shape = value,
            "filter_env_attack_shape" => self.params.filter_env_attack_shape = value,
            "filter_env_decay_shape" => self.params.filter_env_decay_shape = value,
            "filter_env_release_shape" => self.params.filter_env_release_shape = value,
            "lfo1_retrigger" => self.params.lfo1_retrigger = value,
            "lfo2_retrigger" => self.params.lfo2_retrigger = value,
            "lfo3_retrigger" => self.params.lfo3_retrigger = value,
            "lfo4_retrigger" => self.params.lfo4_retrigger = value,
            "lfo1_trigger_mode" => self.params.lfo1_trigger_mode = value,
            "lfo2_trigger_mode" => self.params.lfo2_trigger_mode = value,
            "lfo3_trigger_mode" => self.params.lfo3_trigger_mode = value,
            "lfo4_trigger_mode" => self.params.lfo4_trigger_mode = value,
            "lfo_deform" => self.params.lfo_deform = value,
            "slfo1_rate"       => self.params.slfo1_rate = value,
            "slfo1_waveform"   => self.params.slfo1_waveform = value,
            "slfo1_deform"     => self.params.slfo1_deform = value,
            "slfo1_tempo_sync" => self.params.slfo1_tempo_sync = value,
            "slfo1_unipolar"   => self.params.slfo1_unipolar = value,
            "slfo2_rate"       => self.params.slfo2_rate = value,
            "slfo2_waveform"   => self.params.slfo2_waveform = value,
            "slfo2_deform"     => self.params.slfo2_deform = value,
            "slfo2_tempo_sync" => self.params.slfo2_tempo_sync = value,
            "slfo2_unipolar"   => self.params.slfo2_unipolar = value,
            "twist_engine"     => self.params.twist_engine = value,
            "twist_harmonics"  => self.params.twist_harmonics = value,
            "twist_timbre"     => self.params.twist_timbre = value,
            "twist_morph"      => self.params.twist_morph = value,
            "twist_lpg_decay"  => self.params.twist_lpg_decay = value,
            "twist_lpg_colour" => self.params.twist_lpg_colour = value,
            "twist_aux_mix"    => self.params.twist_aux_mix = value,
            "lfo2_rate" => self.params.lfo2_rate = value,
            "lfo2_pitch_depth" => self.params.lfo2_pitch_depth = value,
            "lfo2_filter_depth" => self.params.lfo2_filter_depth = value,
            "lfo2_amp_depth" => self.params.lfo2_amp_depth = value,
            "lfo2_deform" => self.params.lfo2_deform = value,
            "lfo3_rate" => self.params.lfo3_rate = value,
            "lfo3_deform" => self.params.lfo3_deform = value,
            "lfo4_rate" => self.params.lfo4_rate = value,
            "lfo4_deform" => self.params.lfo4_deform = value,
            "fm_cross_depth" => self.params.fm_cross_depth = value,
            "seq_pitch_depth" => self.params.seq_pitch_depth = value,
            // In debug builds, fail loud on unknown keys so typos are caught
            // by tests instead of silently no-op'ing live parameter updates.
            // In release builds, ignore (cheap, no audio-thread side effects).
            _ => {
                debug_assert!(false, "Part::set_param: unknown key '{key}'");
            }
        }
    }

    /// Control-rate mod precompute for this part (S4-synth 2). Тело — бывший
    /// цикл из tick_block verbatim; порядок вычислений сохранён:
    /// seq_evt → LFO1-4 → scene LFOs → MSEGs → mod matrix → offsets.
    /// Вызывается ТОЛЬКО для включённых частей (гвард `!part.enabled` остался
    /// в tick_block — заморозка состояния выключенных).
    pub(super) fn update_mods(&mut self, inputs: &EngineModInputs, _li: usize) -> ModulationState {
        // Step sequencer (control rate)
        let seq_evt = self.pitch_seq.tick(inputs.sample_rate, inputs.seq_bpm, inputs.block_len);
        if seq_evt.stepped && seq_evt.gate {
            for voice in &mut self.voices {
                if voice.active && !voice.is_releasing() {
                    voice.retrigger_envelope();
                }
            }
        }
        if seq_evt.stepped && !seq_evt.gate {
            for voice in &mut self.voices {
                if voice.active && !voice.is_releasing() {
                    voice.note_off();
                }
            }
        }
        let seq_raw = if self.pitch_seq.enabled { seq_evt.pitch_offset / 24.0 } else { 0.0 };
        let seq_pitch = seq_raw * 24.0 * self.params.seq_pitch_depth;
        let seq_amp = if self.pitch_seq.enabled && !seq_evt.gate { 0.0 } else { 1.0 };

        let bl = inputs.block_len as f32;

        // Set BPM, tempo sync, unipolar on all LFOs
        let lfo_tempo_syncs = [self.params.lfo1_tempo_sync, self.params.lfo2_tempo_sync,
                               self.params.lfo3_tempo_sync, self.params.lfo4_tempo_sync];
        let lfo_unipolars = [self.params.lfo1_unipolar, self.params.lfo2_unipolar,
                             self.params.lfo3_unipolar, self.params.lfo4_unipolar];
        for i in 0..4 {
            self.lfos[i].set_bpm(inputs.seq_bpm);
            self.lfos[i].tempo_sync = lfo_tempo_syncs[i] > 0.5;
            self.lfos[i].unipolar = lfo_unipolars[i] > 0.5;
        }

        // LFO 1
        let lfo1_wf = LfoWaveform::from_param(self.params.lfo_waveform);
        let lfo1_active = lfo1_wf == LfoWaveform::StepSeq
            || self.lfo1_routed
            || self.params.lfo_pitch_depth > 0.001
            || self.params.lfo_filter_depth > 0.001
            || self.params.lfo_amp_depth > 0.001
            || inputs.aftertouch_smooth > 0.001;
        let lfo1_val = if lfo1_active {
            self.lfos[0].tick_with_deform(inputs.lfo_rate * bl, lfo1_wf, self.params.lfo_deform)
        } else { 0.0 };

        // LFO 2 — also active when routed through mod matrix or in StepSeq mode
        let lfo2_wf = LfoWaveform::from_param(self.params.lfo2_waveform);
        let lfo2_active = lfo2_wf == LfoWaveform::StepSeq || self.lfo2_routed
            || self.params.lfo2_pitch_depth > 0.001
            || self.params.lfo2_filter_depth > 0.001
            || self.params.lfo2_amp_depth > 0.001;
        let lfo2_val = if lfo2_active {
            self.lfos[1].tick_with_deform(self.params.lfo2_rate * bl, lfo2_wf, self.params.lfo2_deform)
        } else { 0.0 };

        // LFO 3 & 4 — tick when routed or in StepSeq mode
        let lfo3_wf = LfoWaveform::from_param(self.params.lfo3_waveform);
        let lfo3_val = if lfo3_wf == LfoWaveform::StepSeq || self.lfo3_routed {
            self.lfos[2].tick_with_deform(self.params.lfo3_rate * bl, lfo3_wf, self.params.lfo3_deform)
        } else { 0.0 };
        let lfo4_wf = LfoWaveform::from_param(self.params.lfo4_waveform);
        let lfo4_val = if lfo4_wf == LfoWaveform::StepSeq || self.lfo4_routed {
            self.lfos[3].tick_with_deform(self.params.lfo4_rate * bl, lfo4_wf, self.params.lfo4_deform)
        } else { 0.0 };

        // Step sequencer envelope retrigger from any LFO in StepSeq mode
        let lfo_wfs = [lfo1_wf, lfo2_wf, lfo3_wf, lfo4_wf];
        for (i, &wf) in lfo_wfs.iter().enumerate() {
            if wf == LfoWaveform::StepSeq {
                let evt = self.lfos[i].last_step_event();
                if evt.stepped && (evt.retrigger_aeg || evt.retrigger_feg) {
                    for voice in &mut self.voices {
                        if voice.active && !voice.is_releasing() {
                            if evt.retrigger_aeg && evt.retrigger_feg {
                                voice.retrigger_envelope();
                            } else if evt.retrigger_aeg {
                                voice.retrigger_amp_env();
                            } else {
                                voice.retrigger_filter_env();
                            }
                        }
                    }
                }
            }
        }

        // Scene LFOs — always tick regardless of note state, never retrigger
        let slfo1_wf = LfoWaveform::from_param(self.params.slfo1_waveform);
        self.scene_lfos[0].set_bpm(inputs.seq_bpm);
        self.scene_lfos[0].tempo_sync = self.params.slfo1_tempo_sync > 0.5;
        self.scene_lfos[0].unipolar  = self.params.slfo1_unipolar  > 0.5;
        let slfo1_val = if self.scene_lfo_routed[0] || slfo1_wf == LfoWaveform::StepSeq {
            let rate = if self.scene_lfos[0].tempo_sync {
                self.params.slfo1_rate
            } else {
                self.params.slfo1_rate * bl
            };
            self.scene_lfos[0].tick_with_deform(rate, slfo1_wf, self.params.slfo1_deform)
        } else { 0.0 };

        let slfo2_wf = LfoWaveform::from_param(self.params.slfo2_waveform);
        self.scene_lfos[1].set_bpm(inputs.seq_bpm);
        self.scene_lfos[1].tempo_sync = self.params.slfo2_tempo_sync > 0.5;
        self.scene_lfos[1].unipolar  = self.params.slfo2_unipolar  > 0.5;
        let slfo2_val = if self.scene_lfo_routed[1] || slfo2_wf == LfoWaveform::StepSeq {
            let rate = if self.scene_lfos[1].tempo_sync {
                self.params.slfo2_rate
            } else {
                self.params.slfo2_rate * bl
            };
            self.scene_lfos[1].tick_with_deform(rate, slfo2_wf, self.params.slfo2_deform)
        } else { 0.0 };

        // MSEGs
        let mseg1_val = if self.params.mseg_enabled > 0.5 {
            self.mseg1_state.tick(&self.mseg1)
        } else { 0.0 };
        let mseg2_val = if self.params.mseg_enabled > 0.5 {
            self.mseg2_state.tick(&self.mseg2)
        } else { 0.0 };

        // Mod matrix
        let key_track = (self.last_note as f32 - 60.0) / 48.0;
        let mod_sources = mod_matrix::ModSources {
            lfo_outputs: [lfo1_val, lfo2_val, lfo3_val, lfo4_val],
            scene_lfo_outputs: [slfo1_val, slfo2_val],
            macro_vals: self.macro_vals,
            amp_env: 0.0, filter_env: 0.0,
            mseg_outputs: [mseg1_val, mseg2_val],
            mod_wheel: effective_mw(self.ignore_mw, inputs.mod_wheel),
            aftertouch: inputs.aftertouch_smooth,
            velocity: self.last_velocity,
            key_track,
            step_seq: seq_raw,
            random_bipolar: self.rand_bipolar,
            random_unipolar: self.rand_unipolar,
            alt_bipolar: self.alt_bipolar,
            alt_unipolar: self.alt_unipolar,
            release_vel: self.release_vel,
            pitch_bend: effective_pb(self.ignore_pb, inputs.pitch_bend_semitones) / 2.0, // normalize -1..+1
            cc: inputs.cc_values,
            breath: self.breath,
            expression: self.expression,
            sustain_pedal: self.sustain_pedal,
            lowest_key: (self.lowest_held as f32 - 60.0) / 48.0,
            highest_key: (self.highest_held as f32 - 60.0) / 48.0,
            latest_key: (self.last_note as f32 - 60.0) / 48.0,
            poly_aftertouch: 0.0, // per-voice; 0 for part-level evaluation
        };
        let mod_offsets = self.mod_matrix.evaluate(&mod_sources);

        // Per-part ignore (TECHDEBT #55): маска PB/MW-вкладов
        let pb_eff = effective_pb(self.ignore_pb, inputs.pitch_bend_semitones);
        let mw_eff = effective_mw(self.ignore_mw, inputs.mod_wheel);
        let lfo_pitch_offset_base = pb_eff
            + lfo1_val * self.params.lfo_pitch_depth * 2.0
            + lfo2_val * self.params.lfo2_pitch_depth * 2.0
            + mw_eff * 0.5 * inputs.vibrato_val
            + inputs.aftertouch_smooth * 0.3 * lfo1_val
            + seq_pitch;
        let pitch_offset = lfo_pitch_offset_base + mod_offsets.pitch;
        let pitch_mult = if pitch_offset.abs() < 0.001 { 1.0 } else { (pitch_offset / 12.0).exp2() };
        // filter_offset is Hz-only (LFO direct routing, aftertouch)
        // filter_offset_semis is exponential (mod matrix)
        let filter_offset = lfo1_val * self.params.lfo_filter_depth * 4000.0
            + lfo2_val * self.params.lfo2_filter_depth * 4000.0
            + inputs.aftertouch_smooth * 2000.0;
        let lfo_amp_base = (1.0 - self.params.lfo_amp_depth * 0.5 * (1.0 - lfo1_val))
            * (1.0 - self.params.lfo2_amp_depth * 0.5 * (1.0 - lfo2_val))
            * seq_amp;
        let amp_mod = lfo_amp_base * (1.0 + mod_offsets.amplitude).max(0.0);

        ModulationState {
            pitch_mult, filter_offset, filter_offset_semis: mod_offsets.filter_cutoff, amp_mod,
        }
    }
}
