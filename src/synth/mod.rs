//! Synth engine: layered polyphonic voice pools + MIDI event dispatch.

pub mod dsp_utils;
mod airwindows;
use airwindows::Airwindows;
pub mod fx_chain;
pub mod bass;
pub mod bbd_ensemble;
pub mod bitcrusher;
pub mod bonsai;
pub mod chorus;
pub mod compressor;
pub mod conditioner;
pub mod delay;
pub mod drum;
pub mod envelope;
pub mod eq;
pub mod exciter;
pub mod filter;
pub mod flanger;
pub mod formant;
pub mod freq_shift;
pub mod graphic_eq;
pub mod lfo;
pub mod arpeggiator;
pub mod looper;
pub mod mod_matrix;
pub mod ms_tool;
pub mod mseg;
pub mod resonator;
pub mod rotary;
pub mod sampler;
pub mod epiano;
pub mod neuron;
pub mod oscillator;
pub mod overdrive;
pub mod piano;
pub mod phaser;
pub mod combulator;
pub mod convolution_reverb;
pub mod floaty_delay;
pub mod nimbus;
pub mod reverb;
pub mod reverb2;
pub mod ring_mod;
pub mod treemonster;
pub mod spring_reverb;
pub mod vocoder;
pub mod midi_player;
pub mod step_seq;
pub mod tape;
pub mod tremolo;
pub mod voice;
pub mod wave_shaper;
mod patch_params;
pub use patch_params::PatchParams;

use crate::cc_map::{CcMap, ParamScope};
use crate::preset::Patch;
use bbd_ensemble::BbdEnsemble;
use bitcrusher::Bitcrusher;
use bonsai::Bonsai;
use wave_shaper::WaveShaper;
use ms_tool::MsTool;
use graphic_eq::GraphicEq;
use conditioner::Conditioner;
use exciter::Exciter;
use floaty_delay::FloatyDelay;
use reverb2::Reverb2;
use combulator::Combulator;
use treemonster::Treemonster;
use nimbus::Nimbus;
use vocoder::Vocoder;
use convolution_reverb::ConvolutionReverb;
use chorus::Chorus;
use compressor::Compressor;
use delay::StereoDelay;
use resonator::Resonator;
use rotary::RotarySpeaker;
use drum::DrumEngine;
use eq::ParametricEq;
use flanger::Flanger;
use freq_shift::FreqShift;
use arpeggiator::Arpeggiator;
use looper::MidiLooper;
use neuron::Neuron;
use sampler::SamplerEngine;
use overdrive::Overdrive;
use phaser::Phaser;
use reverb::Reverb;
use ring_mod::RingMod;
use spring_reverb::SpringReverb;
use tape::Tape;
use tremolo::Tremolo;

/// Lock-free oscilloscope buffer shared between audio and GUI threads.
/// Audio writes samples; GUI reads them for display. Uses atomic f32 encoding.
pub const SCOPE_SIZE: usize = 256;
/// Amplitude below which the oscilloscope is considered silent (freeze gate).
const SCOPE_SILENCE: f32 = 1e-4;

/// Freeze-гейт осциллографа с гистерезисом: возвращает true, когда сэмпл
/// надо запушить в буфер. Состояние (`frozen`) живёт в SynthEngine
/// (аудио-поток), НЕ в ScopeBuffer — тот шарится через Arc, push(&self).
#[inline]
fn scope_gate(frozen: &mut bool, sample: f32) -> bool {
    if sample.abs() > SCOPE_SILENCE {
        *frozen = false;
        true
    } else if *frozen {
        // Затихли: линия уже доведена до нуля, дальше не пушим.
        false
    } else {
        // Был сигнал, стал тихим: пушим этот тихий сэмпл (линия доходит
        // до нуля) и защёлкиваем тишину — без мерцания на границе.
        *frozen = true;
        true
    }
}

pub struct ScopeBuffer {
    pub data: [std::sync::atomic::AtomicU32; SCOPE_SIZE],
    pub write_pos: std::sync::atomic::AtomicUsize,
}

impl Default for ScopeBuffer {
    fn default() -> Self { Self::new() }
}

impl ScopeBuffer {
    pub fn new() -> Self {
        Self {
            data: std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0)),
            write_pos: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    #[inline]
    pub fn push(&self, sample: f32, pos: &mut usize) {
        self.data[*pos].store(sample.to_bits(), std::sync::atomic::Ordering::Relaxed);
        *pos = (*pos + 1) & (SCOPE_SIZE - 1);
        self.write_pos.store(*pos, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn read(&self) -> [f32; SCOPE_SIZE] {
        let mut out = [0.0f32; SCOPE_SIZE];
        let wp = self.write_pos.load(std::sync::atomic::Ordering::Relaxed);
        for (i, out_sample) in out.iter_mut().enumerate() {
            let idx = (wp + i) & (SCOPE_SIZE - 1);
            *out_sample = f32::from_bits(self.data[idx].load(std::sync::atomic::Ordering::Relaxed));
        }
        out
    }
}

mod events;
pub use events::{ControlEvent, DrumParam, LoadPatchEvent, MidiEvent, NOTE_MODE_BASS, NOTE_MODE_POLY,
    NOTE_MODE_ROOT, ParamFeedback, PartFullState};
pub(crate) use events::RootDecision;
// effective_pb/effective_mw: the engine path lives in part.rs; this re-export
// is for tests.rs (`super::effective_pb`).
#[allow(unused_imports)]
pub(crate) use events::{effective_mw, effective_pb};

pub const BLOCK_SIZE: usize = 32;

mod part;
pub use part::MAX_PARTS;
// pub(super) items of part.rs — the parent re-imports them privately; children
// (voice.rs, tests.rs) see them via `use super::…` / the `use super::*` glob.
use part::{EngineModInputs, GlobalParams, ModulationState, Part, PickupState};
// tests.rs constructs these through the `use super::*` glob:
#[allow(unused_imports)]
use lfo::Lfo;
#[allow(unused_imports)]
use mod_matrix::ModMatrix;

pub struct SynthEngine {
    parts: Vec<Part>,
    pub drum_engine: DrumEngine,
    pub sampler: SamplerEngine,
    /// DSP-bus trim in dB — scales the summed part outputs + internal drum
    /// engine before the SF2 add (per-block linear gain, see tick_block).
    pub dsp_trim_db: f32,
    pitch_bend_semitones: f32,
    /// Raw normalized pitch-bend wheel value -1..1 (mirror for re-assert on
    /// SF2 channels when a part un-ignores PB, TECHDEBT #55).
    pitch_bend_norm: f32,
    mod_wheel: f32,
    vibrato_phase: f32,
    aftertouch: f32,
    aftertouch_smooth: f32,
    cc_values: [f32; 4],   // assignable CC sources for mod matrix (CC1..CC4)
    pub cc_numbers: [u8; 4],  // which MIDI CC numbers are assigned (default: 1,2,3,4)
    sample_rate: f32,
    // Effects chain (order: EQ → Compressor → Overdrive → Tape → Neuron → Phaser → Flanger → Tremolo → Bitcrusher → RingMod → FreqShift → Chorus → Delay → Reverb/Spring)
    pub eq: ParametricEq,
    pub compressor: Compressor,
    pub overdrive: Overdrive,
    pub tape: Tape,
    pub neuron: Neuron,
    pub phaser: Phaser,
    pub flanger: Flanger,
    pub tremolo: Tremolo,
    pub bitcrusher: Bitcrusher,
    pub ring_mod: RingMod,
    pub freq_shift: FreqShift,
    chorus: Chorus,
    rotary: RotarySpeaker,
    bbd_ensemble: BbdEnsemble,
    resonator: Resonator,
    bonsai: Bonsai,
    wave_shaper: WaveShaper,
    ms_tool: MsTool,
    graphic_eq_fx: GraphicEq,
    conditioner: Conditioner,
    exciter: Exciter,
    floaty_delay: FloatyDelay,
    reverb2: Reverb2,
    combulator: Combulator,
    treemonster: Treemonster,
    nimbus: Nimbus,
    vocoder: Vocoder,
    conv_reverb: ConvolutionReverb,
    airwindows: Airwindows,
    delay: StereoDelay,
    reverb: Reverb,
    pub spring_reverb: SpringReverb,
    pub arpeggiator: Arpeggiator,
    pub looper: MidiLooper,
    cc_map: CcMap,
    patches: Vec<Patch>,
    preset_params_cache: Vec<PatchParams>,
    feedback_tx: Option<rtrb::Producer<ParamFeedback>>,
    /// Where displaced heap objects go so the audio thread never calls free().
    trash: Option<crate::trash::TrashTx>,
    program_change: Option<std::sync::Arc<std::sync::atomic::AtomicU8>>,
    global_params: GlobalParams,
    pickup_states: [PickupState; 128],
    /// Drum-note mode gate for live pads: 0 = pads 36-51 trigger drums on ANY
    /// channel (drum_mode), nonzero = pads play parts. NOT a transport target —
    /// transport notes (81/82/83/123/124) always drive the master transport.
    seq_target: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Quantized master recording (#30 sync): arm pending until the
    /// drum sequencer crosses its pattern boundary (current_step == 0).
    looper_arm_pending: bool,
    /// Как был начат ТЕКУЩИЙ захват: true = квантованный (граница такта,
    /// петля = 1 цикл паттерна), false = свободный (немедленный).
    looper_capture_quantized: bool,
    /// Previous drum step for boundary edge detection.
    looper_prev_step: u8,
    /// Pattern cache for looper boundary resync (#32 Phase 5): when the
    /// sequencer pattern changes at a boundary, the looper cycle is resized
    /// to the new pattern length.
    looper_prev_pattern: u8,
    /// Quantized overdub (layer): arm at the next boundary / close at the
    /// next boundary (MasterOverdub toggle).
    looper_overdub_pending: bool,
    looper_overdub_active: bool,
    /// Cycle length (ticks) of the last quantized capture — drift-correction
    /// applies when the loop length still equals it.
    looper_cycle_ticks: u32,
    /// Досрочное закрытие мультицикл-захвата (закрытие на следующей границе).
    looper_finish_pending: bool,
    /// Целых циклов паттерна, записанных квантованным захватом.
    looper_rec_cycles: u32,
    /// GUI mirror of arm_pending (shows "ARMED" in the looper state label).
    /// Master capture state for GUI: 0 = idle, 1 = armed (waiting for the
    /// boundary), 2 = capturing, 3 = capturing, closing at the next boundary.
    master_rec_state: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Current capture cycle (1-based), for the "REC k/N" readout.
    master_rec_cycle: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Active part index (0-7), shared with GUI for looper part recording
    active_part: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Looper record source: true = Mix (all sounding enabled parts,
    /// default), false = only the active part (LooperSetSource; live
    /// transport state, not persisted).
    looper_source_mix: bool,
    /// Sustain capture (user decision 2026-11-08): note-offs received while
    /// the pedal is down during capture are parked here until pedal-up, so
    /// loop gates match the audible length. Fixed array — RT thread, no
    /// allocs; overflow falls back to immediate recording (see the defer
    /// branch in record_for_looper). Entry = (note, part).
    looper_sustain_pending: [(u8, u8); 16],
    looper_sustain_pending_len: usize,
    /// Sustain pedal state (CC64 from pedal/fader or GUI toggle), shared with GUI
    sustain: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Static pedal-up release duration (None = patch release)
    sustain_release: Option<f32>,
    /// Oscilloscope ring buffer: 256 mono samples shared with GUI via atomics
    scope_buf: std::sync::Arc<ScopeBuffer>,
    scope_write: usize,
    /// Freeze-гистерезис осциллографа: true = тишина (пуш пропускается)
    scope_frozen: bool,
    /// MIDI file sequencer player
    pub midi_player: midi_player::MidiPlayer,
    /// Reusable event buffer for midi_player.process_block (avoids per-block allocation)
    midi_seq_buf: Vec<(u8, midi_player::SeqEventData)>,
    /// Per MIDI-file channel: 0 = DSP layer 0 took the last note-on,
    /// 1 = SF2 took it, 2 = drums. Note-offs must follow the note-on's
    /// route, even if the track instrument changed mid-note.
    seq_route: [u8; 16],
}

mod cached_fx;
use cached_fx::CachedFxParams;


impl SynthEngine {
    /// Тест-аксессор #52а. НЕ для RT-пути.
    pub fn sampler_voices_in_release(&mut self) -> usize {
        self.sampler.voices_in_release()
    }

    pub fn new(sample_rate: f32) -> Self {
        // All 8 parts pre-allocated; only part 0 enabled by default
        let mut parts: Vec<Part> = (0..MAX_PARTS).map(|_| Part::new(sample_rate)).collect();
        parts[0].enabled = true; // parts 1-7 start disabled (zero CPU cost)
        Self {
            parts, drum_engine: DrumEngine::new(sample_rate),
            sampler: SamplerEngine::new(sample_rate),
            dsp_trim_db: 0.0,
            pitch_bend_semitones: 0.0, pitch_bend_norm: 0.0,
            mod_wheel: 0.0, vibrato_phase: 0.0,
            aftertouch: 0.0, aftertouch_smooth: 0.0,
            cc_values: [0.0; 4], cc_numbers: [1, 2, 3, 4],
            sample_rate,
            eq: ParametricEq::new(sample_rate),
            compressor: Compressor::new(sample_rate),
            overdrive: Overdrive::new(sample_rate),
            tape: Tape::new(sample_rate),
            neuron: Neuron::new(sample_rate),
            phaser: Phaser::new(sample_rate),
            flanger: Flanger::new(sample_rate),
            tremolo: Tremolo::new(sample_rate),
            bitcrusher: Bitcrusher::new(sample_rate),
            ring_mod: RingMod::new(sample_rate),
            freq_shift: FreqShift::new(sample_rate),
            arpeggiator: Arpeggiator::new(sample_rate),
            looper: MidiLooper::new(sample_rate),
            chorus: Chorus::new(sample_rate),
            rotary: RotarySpeaker::new(sample_rate),
            bbd_ensemble: BbdEnsemble::new(sample_rate),
            resonator: Resonator::new(sample_rate),
            bonsai: Bonsai::new(sample_rate),
            wave_shaper: WaveShaper::new(sample_rate),
            ms_tool: MsTool::new(sample_rate),
            graphic_eq_fx: GraphicEq::new(sample_rate),
            conditioner: Conditioner::new(sample_rate),
            exciter: Exciter::new(sample_rate),
            floaty_delay: FloatyDelay::new(sample_rate),
            reverb2: Reverb2::new(sample_rate),
            combulator: Combulator::new(sample_rate),
            treemonster: Treemonster::new(sample_rate),
            nimbus: Nimbus::new(sample_rate),
            vocoder: Vocoder::new(sample_rate),
            conv_reverb: ConvolutionReverb::new(sample_rate),
            airwindows: Airwindows::new(sample_rate),
            delay: StereoDelay::new(sample_rate),
            reverb: Reverb::new(sample_rate),
            spring_reverb: SpringReverb::new(sample_rate),
            cc_map: CcMap::default(),
            patches: Vec::new(), preset_params_cache: Vec::new(),
            feedback_tx: None, trash: None, program_change: None,
            global_params: GlobalParams::default(),
            pickup_states: [PickupState::default(); 128],
            seq_target: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(1)), // default: non-drum mode
            looper_arm_pending: false,
            looper_capture_quantized: false,
            looper_prev_step: 0,
            looper_prev_pattern: 0,
            looper_overdub_pending: false,
            looper_overdub_active: false,
            looper_cycle_ticks: 0,
            looper_finish_pending: false,
            looper_rec_cycles: 0,
            master_rec_state: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            master_rec_cycle: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            active_part: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            looper_source_mix: true,
            looper_sustain_pending: [(0, 0); 16],
            looper_sustain_pending_len: 0,
            sustain: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sustain_release: None,
            scope_buf: std::sync::Arc::new(ScopeBuffer::new()),
            scope_write: 0,
            scope_frozen: false,
            midi_player: midi_player::MidiPlayer::new(),
            midi_seq_buf: Vec::new(),
            seq_route: [0u8; 16],
        }
    }

    pub fn seq_target_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.seq_target.clone()
    }

    pub fn master_rec_state_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.master_rec_state.clone()
    }

    pub fn master_rec_cycle_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.master_rec_cycle.clone()
    }

    fn set_master_rec_state(&self, state: u8, cycle: u8) {
        self.master_rec_state.store(state, std::sync::atomic::Ordering::Relaxed);
        self.master_rec_cycle.store(cycle, std::sync::atomic::Ordering::Relaxed);
    }

    /// Ёмкость лупера = логические циклы + 1 запасной: playback_tick не
    /// доходит до конца — wrap'а (и локальных тиков) во время записи нет.
    fn keep_capture_capacity(&mut self) {
        let need = (self.looper_rec_cycles + 1).saturating_mul(self.looper_cycle_ticks);
        if self.looper.loop_len_ticks() < need {
            self.looper.set_loop_len_ticks(need);
        }
    }

    pub fn active_part_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.active_part.clone()
    }

    pub fn sustain_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.sustain.clone()
    }

    /// Единая точка пулла общих хэндлов (ST1). Порядок полей = CommonInit.
    pub fn handles(&self) -> crate::EngineHandles {
        crate::EngineHandles {
            #[cfg(feature = "gui")]
            drum_step_atom: self.drum_engine.step_atom(),
            #[cfg(feature = "gui")]
            drum_play_atom: self.drum_engine.play_atom(),
            #[cfg(feature = "gui")]
            drum_rec_atom: self.drum_engine.rec_atom(),
            #[cfg(feature = "gui")]
            drum_pattern_atom: self.drum_engine.pattern_atom(),
            #[cfg(feature = "gui")]
            drum_undo_atom: self.drum_engine.undo_atom(),
            #[cfg(feature = "gui")]
            drum_redo_atom: self.drum_engine.redo_atom(),
            #[cfg(feature = "gui")]
            looper_atoms: self.looper.atoms(),
            #[cfg(feature = "gui")]
            looper_display: self.looper.display(),
            seq_target_atom: self.seq_target_atom(),
            active_part_atom: self.active_part_atom(),
            #[cfg(feature = "gui")]
            sustain_atom: self.sustain_atom(),
            #[cfg(feature = "gui")]
            master_rec_state_atom: self.master_rec_state_atom(),
            master_rec_cycle_atom: self.master_rec_cycle_atom(),
            #[cfg(feature = "gui")]
            pitch_seq_step_atoms: self.pitch_seq_step_atoms(),
            #[cfg(feature = "gui")]
            scope_buf: self.scope_buffer(),
            #[cfg(feature = "gui")]
            midi_seq_play_atom: self.midi_player_play_atom(),
            #[cfg(feature = "gui")]
            midi_seq_pos_atom: self.midi_player_pos_atom(),
        }
    }

    /// Current SF2 release-time scale (TECHDEBT #7 introspection/tests).
    pub fn sampler_release_scale(&self) -> f32 {
        self.sampler.release_scale()
    }

    /// Apply sustain pedal state (CC64 from pedal/fader, or GUI toggle).
    /// On release, voices whose key is STILL HELD (`keys_down`) keep sounding —
    /// their own key-off will release them. Voices held by the pedal only
    /// (`sustained_notes`) and stale mono/latch entries are released/cleared;
    /// stale flags from a patch switch while the pedal was down would otherwise
    /// leave notes ringing forever.
    fn set_sustain(&mut self, value: u8) {
        let on = value >= 64;
        self.sustain.store(on, std::sync::atomic::Ordering::Relaxed);
        let sr = self.sustain_release;
        for (i, part) in self.parts.iter_mut().enumerate() {
            part.sustain_pedal = if on { 1.0 } else { 0.0 };
            // SF2 voices: rustysynth applies hold-pedal per channel itself —
            // per-part mask, disabled parts get CC64=0 (bass without sustain).
            self.sampler.set_part_sustain(i, on && part.sustain_enabled);
            if !on {
                part.release_pedal_held(sr);
            }
        }
        // Sustain capture: pedal-up flushes the deferred note-offs. The
        // pedal atom was already stored false above, so this is where the
        // deferred offs land (gate = audible length, user decision
        // 2026-11-08).
        if !on {
            self.flush_looper_sustain_pending();
        }
    }



    /// Dispatches an arp note-off to every enabled part with part transpose
    /// applied: SF2 channels are keyed by transposed pitch, DSP parts receive
    /// the raw note (they match offs by src_note). Shared by flush_arp_note_off
    /// and the panic paths (CC120/123, AllNotesOff, SetArpParams-off) so the
    /// SF2 gate voice always gets a matching off, transposed like its on.
    fn dispatch_arp_note_off(&mut self, off: u8) {
        for (i, part) in self.parts.iter_mut().enumerate() {
            if !part.enabled { continue; }
            let transposed = (off as i16 + part.transpose as i16).clamp(0, 127) as u8;
            if part.sf2_mode { self.sampler.note_off(i, transposed); }
            else { part.note_off(off); } // raw: part matches by src_note
        }
    }

    /// Shared part note-on dispatch: range/velocity gate, note-mode gate
    /// (root_on — Bass/Root re-keying, TECHDEBT #6/#24), DSP voice or SF2
    /// sampler. The enabled/mute guards stay
    /// at the call sites, which intentionally differ (offs skip mute).
    /// Returns true only if the note actually SOUNDED in this part — the
    /// looper multitrack records just sounding parts (replay honours each
    /// part's range/transpose), callers that don't record ignore the result.
    fn route_note_on(&mut self, i: usize, raw: u8, velocity: u8) -> bool {
        let (sf2_mode, transpose, lo, hi, vmin, vmax) = {
            let p = &self.parts[i];
            (p.sf2_mode, p.transpose, p.min_note, p.max_note, p.vel_min, p.vel_max)
        };
        if raw < lo || raw > hi || velocity < vmin || velocity > vmax { return false; }
        let RootDecision::Sound { release, note } = self.parts[i].root_on(raw) else { return false };
        let tr = |n: u8| (n as i16 + transpose as i16).clamp(0, 127) as u8;
        if sf2_mode {
            // SF2 mono (play_mode 1/2): новая нота глушит предыдущую.
            // (Legato-семантика «без ретрига» для сэмплера не отличима —
            // note_on всегда ретриггерит; режимы ведут себя одинаково.)
            let mono = { let p = &self.parts[i];
                p.params.play_mode as u8 == 1 || p.params.play_mode as u8 == 2 };
            if mono && self.parts[i].note_mode == crate::synth::NOTE_MODE_POLY {
                // Last-note priority (канон Moog/SoS): стек зажатых клавиш,
                // звучит последняя; заглушаем предыдущую звучащую
                let part = &mut self.parts[i];
                part.sf2_mono_stack.retain(|&(n, _)| n != raw);
                if let Some(&(prev, _)) = part.sf2_mono_stack.last() {
                    self.sampler.note_off(i, tr(prev));
                }
                part.sf2_mono_stack.push((raw, velocity));
            }
            if let Some(old) = release { self.sampler.note_off(i, tr(old)); }
            self.sampler.note_on(i, tr(note), velocity);
        } else {
            if let Some(old) = release {
                // The replaced root leaves outright: a bass layer must not
                // pile up pedal-sustained roots.
                let part = &mut self.parts[i];
                for v in &mut part.voices {
                    if v.active && v.src_note == old { v.note_off(); }
                }
                part.sustained_notes[old as usize] = false;
            }
            self.parts[i].note_on(note, velocity);
        }
        true
    }

    /// Shared part note-off dispatch with note-mode re-keying. Offs skip the
    /// range check (matching today's behavior) so a range change mid-note
    /// cannot orphan a voice.
    fn route_note_off(&mut self, i: usize, raw: u8) {
        let (sf2_mode, transpose, gated, last_vel, old_root) = {
            let p = &self.parts[i];
            (p.sf2_mode, p.transpose, p.note_mode != NOTE_MODE_POLY,
             p.last_velocity, p.root_active)
        };
        let next = self.parts[i].root_off(raw);
        let tr = |n: u8| (n as i16 + transpose as i16).clamp(0, 127) as u8;
        if sf2_mode {
            // Last-note priority: отпустили звучащую клавишу — вернуться к
            // последней из оставшихся зажатых (re-trigger, канон Moog/SoS)
            let (mono, vel) = { let p = &self.parts[i];
                (p.params.play_mode as u8 == 1 || p.params.play_mode as u8 == 2, p.last_velocity) };
            if mono && self.parts[i].note_mode == crate::synth::NOTE_MODE_POLY {
                let part = &mut self.parts[i];
                let had = part.sf2_mono_stack.iter().any(|&(n, _)| n == raw);
                part.sf2_mono_stack.retain(|&(n, _)| n != raw);
                self.sampler.note_off(i, tr(raw));
                if had {
                    if let Some(&(top, top_vel)) = part.sf2_mono_stack.last() {
                        self.sampler.note_on(i, tr(top), if top_vel > 0 { top_vel } else { (vel * 127.0) as u8 });
                    }
                }
                return;
            }
            self.sampler.note_off(i, tr(raw));
            // Re-key в Root-режиме: синтезированный корень (old) может быть
            // ≠ raw — отпустить и его, иначе дрон под новой нотой.
            if let (Some(n), Some(old)) = (next, old_root) {
                if old != raw { self.sampler.note_off(i, tr(old)); }
                self.sampler.note_on(i, tr(n), (last_vel * 127.0) as u8);
            } else if let Some(n) = next {
                self.sampler.note_on(i, tr(n), (last_vel * 127.0) as u8);
            }
        } else {
            self.parts[i].note_off(raw);
            if gated {
                // Bass-split semantics (TECHDEBT #6): key-up = the note is DONE.
                // Force-release the root voice even when the pedal holds it and
                // clear its sustained_notes flag — the pedal must not resurrect
                // a bass root whose key the player has already let go (that
                // would drone the old root under the re-keyed one). Same for
                // the replaced root in route_note_on above. In Root mode the
                // sounding root may be a synthesized target (not `raw`), so
                // release the OLD root when the re-key moved it.
                let part = &mut self.parts[i];
                for v in &mut part.voices {
                    if v.active && v.src_note == raw { v.note_off(); }
                }
                part.sustained_notes[raw as usize] = false;
                if let (Some(n), Some(old)) = (next, old_root) {
                    if old != raw {
                        for v in &mut part.voices {
                            if v.active && v.src_note == old { v.note_off(); }
                        }
                        part.sustained_notes[old as usize] = false;
                    }
                    part.note_on(n, (last_vel * 127.0) as u8);
                }
            }
        }
    }

    /// Last held arp key released: the arpeggiator resets pattern_len, so
    /// tick() will never dispatch the pending gate-off — the sounding voice
    /// would hang forever. Dispatch the off here (same routing as tick_block).
    fn flush_arp_note_off(&mut self) {
        if self.arpeggiator.keeps_playing() {
            return; // tick() still owns gate-offs (includes a latched/frozen set)
        }
        if let Some(off) = self.arpeggiator.flush_current_note() {
            self.dispatch_arp_note_off(off);
            // Record per part (same source-consistency as the note dispatch
            // loop) — used to be a single event tagged with the active part.
            for i in 0..self.parts.len() {
                if !self.parts[i].enabled { continue; }
                self.record_for_looper(off, 0, i as u8, false);
            }
        }
    }

    /// Routes the note-offs returned by looper mute/solo mutators to the
    /// recorded part (SF2 or DSP), so muting stops what is sounding.
    fn route_looper_note_offs(&mut self, note_offs: &[(u8, u8, u8)]) {
        for &(note, vel, part_id) in note_offs {
            if part_id == 255 { break; }
            if vel == 0 {
                if let Some(part) = self.parts.get_mut(part_id as usize) {
                    if part.sf2_mode {
                        self.sampler.note_off(part_id as usize, note);
                    } else {
                        part.note_off(note);
                    }
                }
            }
        }
    }

    /// Looper record gate (Source toggle): in Mix source (default) every
    /// sounding part is recorded — the note dispatch loop tags each event
    /// with its own part id; in Part source only the active part is
    /// recorded. RT-safe: one atomic load + branch.
    ///
    /// Sustain capture: pedal down → note-off is DEFERRED to pedal-up
    /// (gate = audible length; user decision 2026-11-08). `defer_ok` is
    /// true only for user MIDI offs — ARP-offs are rhythmic and are never
    /// deferred. Flush paths pass false to force-write past the defer
    /// branch (the pedal may still be down at a capture exit).
    #[inline]
    fn record_for_looper(&mut self, note: u8, velocity: u8, part: u8, defer_ok: bool) {
        if !self.looper_source_mix
            && part != self.active_part.load(std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        if velocity == 0 && defer_ok
            && self.sustain.load(std::sync::atomic::Ordering::Relaxed)
            && (part as usize) < self.parts.len()
            && self.parts[part as usize].sustain_enabled
            && self.looper.is_capturing()
        {
            // One pending entry per NOTE is enough: the NoteOff broadcast
            // loops over every part, but the looper's active_notes guard
            // drops any off from a part that is not holding the note, so
            // duplicate entries would never produce events. Restrikes under
            // the pedal land here too — the flush must write exactly one off.
            if self.looper_sustain_pending[..self.looper_sustain_pending_len]
                .iter()
                .any(|&(n, _)| n == note)
            {
                return;
            }
            if self.looper_sustain_pending_len < self.looper_sustain_pending.len() {
                self.looper_sustain_pending[self.looper_sustain_pending_len] = (note, part);
                self.looper_sustain_pending_len += 1;
            } else {
                // Pending full: old behavior (write the off immediately),
                // counted so the loss stays visible.
                crate::metrics::METRICS.note_looper_pending_overflow();
                self.looper.record_event(note, velocity, part);
            }
            return;
        }
        self.looper.record_event(note, velocity, part);
    }

    /// Writes all deferred (sustain-held) note-offs to the looper and clears
    /// the pending list. Called from set_sustain on pedal-up and before every
    /// capture exit. Entries are force-written (`defer_ok = false`): during a
    /// capture exit the pedal may still be DOWN, so the defer branch must not
    /// re-catch them. The looper's active_notes guard still applies on the
    /// flush path (a pending off whose note was force-cleared by a wrap is
    /// dropped, not written). Flush on empty pending = no-op.
    fn flush_looper_sustain_pending(&mut self) {
        for i in 0..self.looper_sustain_pending_len {
            let (note, part) = self.looper_sustain_pending[i];
            self.record_for_looper(note, 0, part, false);
        }
        self.looper_sustain_pending_len = 0;
    }

    /// Master transport (#32): start both transports (drums + looper).
    fn master_start(&mut self) {
        self.drum_engine.set_playing(true);
        if matches!(self.looper.state, crate::synth::looper::LooperState::Idle) {
            self.looper.toggle_play();
        }
    }

    /// Master transport: stop both; a recording looper finishes its layer
    /// first, then stops (stop() flags needs_note_offs; actual note-offs
    /// drain on the next looper tick, как и в ручном Stop).
    pub fn master_stop(&mut self) {
        // Sustain capture: any pending note-offs must land in the loop before
        // trim/toggle decide its final content (exit #1 of the capture exits).
        self.flush_looper_sustain_pending();
        self.looper_arm_pending = false;
        // Стоп посреди quantized capture: обрезать луп до записанных
        // циклов (события + длина), иначе он остаётся cycle*64 с тишиной
        // в хвосте (audit rhythm #2).
        if self.looper_capture_quantized && self.looper_rec_cycles > 0 {
            let kept = self.looper_rec_cycles.saturating_mul(self.looper_cycle_ticks);
            self.looper.trim_after(kept);
            self.looper.set_loop_len_ticks(kept);
        }
        self.looper_capture_quantized = false;
        self.looper_overdub_pending = false;
        self.looper_overdub_active = false;
        self.looper_finish_pending = false;
        self.looper_rec_cycles = 0;
        self.looper.set_display_cycle_ticks(None);
        self.set_master_rec_state(0, 0);
        self.drum_engine.set_playing(false);
        self.drum_engine.sequencer.reset();
        match self.looper.state {
            crate::synth::looper::LooperState::Recording
            | crate::synth::looper::LooperState::Overdubbing => {
                self.looper.toggle_record();
                self.looper.toggle_play();
            }
            crate::synth::looper::LooperState::Playing => self.looper.toggle_play(),
            _ => {}
        }
        debug_assert_eq!(self.looper_sustain_pending_len, 0, "master_stop: pending must be flushed");
    }

    /// Master transport: toggle play — if anything plays, stop everything;
    /// otherwise start everything.
    pub fn master_toggle_play(&mut self) {
        let any = self.drum_engine.sequencer.playing
            || matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Playing
                    | crate::synth::looper::LooperState::Overdubbing
                    | crate::synth::looper::LooperState::Recording
            );
        if any {
            self.master_stop();
        } else {
            self.master_start();
        }
    }

    /// Master overdub (layer): loop Playing (quantized) -> Overdubbing
    /// starts at the NEXT pattern boundary; again -> closes at the next
    /// boundary. Loop length never changes.
    pub fn master_toggle_overdub(&mut self) {
        match self.looper.state {
            crate::synth::looper::LooperState::Playing
                if self.looper_capture_quantized =>
            {
                self.looper_overdub_pending = true;
            }
            crate::synth::looper::LooperState::Overdubbing
                if self.looper_overdub_active =>
            {
                self.looper_finish_pending = true;
            }
            _ => {}
        }
    }

    /// Master transport: toggle record arm. Quantized path: drums playing,
    /// pattern >= 2 steps, looper Idle -> capture starts at the NEXT pattern
    /// boundary; второй Rec закрывает на следующей границе — петля = РОВНО
    /// N целых циклов паттерна (сколько сыграл). Everything else (stopped
    /// sequencer, Steps=1, overdub from Playing, disarming) — прежнее
    /// немедленное поведение (Bars свободного режима не трогаются).
    pub fn master_toggle_record(&mut self) {
        let looper_rec = matches!(
            self.looper.state,
            crate::synth::looper::LooperState::Recording
                | crate::synth::looper::LooperState::Overdubbing
        );
        // Мультицикл-захват: второе нажатие = закрыть на следующей границе
        if looper_rec && self.looper_capture_quantized {
            self.looper_finish_pending = true;
            self.set_master_rec_state(3, self.master_rec_cycle.load(std::sync::atomic::Ordering::Relaxed));
            return;
        }
        // DAW practice (Ableton Session: каждая запись = свежий клип):
        // Rec после закрытой квантованной петли = НОВАЯ запись — старая петля
        // стирается, свежий арм. Никакого наслоения на старый круг.
        if !looper_rec
            && self.looper_capture_quantized
            && self.looper.state == crate::synth::looper::LooperState::Playing
        {
            self.looper.clear();
            self.looper_cycle_ticks = 0;
        }
        // Снять арм (toggle-семантика ожидания границы)
        if self.looper_arm_pending {
            self.looper_arm_pending = false;
            self.set_master_rec_state(0, 0);
            self.drum_engine.set_recording(false);
            return;
        }
        // Только сетка пишется (лупер Idle) — разармировать сетку, лупера
        // НЕ трогать (старый toggle в этой ситуации лупер не запускал)
        if !looper_rec && self.drum_engine.sequencer.recording {
            self.drum_engine.set_recording(false);
            return;
        }
        if !looper_rec {
            // ARM
            let pat = self.drum_engine.sequencer.current_pattern as usize;
            let pattern_len = self.drum_engine.sequencer.patterns[pat].length;
            let can_quant = self.drum_engine.sequencer.playing
                && pattern_len > 1
                && self.looper.state == crate::synth::looper::LooperState::Idle;
            self.drum_engine.set_recording(true);
            if !can_quant {
                // Free: секвенсор стоит (autoplay), Steps=1, или overdub из Playing
                if !self.drum_engine.sequencer.playing {
                    self.drum_engine.set_playing(true);
                }
                self.looper_capture_quantized = false;
                if matches!(
                    self.looper.state,
                    crate::synth::looper::LooperState::Idle
                        | crate::synth::looper::LooperState::Playing
                ) {
                    self.looper.toggle_record();
                }
                return;
            }
            // Armed: ждём границы паттерна. Петля = Bars × цикл паттерна
            // (Boss LOOP LENGTH), Bars юзера учитываются в tick_block.
            self.drum_engine.set_playing(true);
            self.looper_arm_pending = true;
            self.looper_capture_quantized = true;
            self.set_master_rec_state(1, 0);
            return;
        }
        // looper_rec && !quantized (free-захват или overdub) — закрыть сразу
        self.master_toggle_record_free_looper();
    }

    /// Former immediate toggle: Recording/Overdubbing -> finish +
    /// set_recording(false); Idle/Playing -> record.
    fn master_toggle_record_free_looper(&mut self) {
        self.drum_engine.set_recording(false);
        if matches!(
            self.looper.state,
            crate::synth::looper::LooperState::Recording
                | crate::synth::looper::LooperState::Overdubbing
                | crate::synth::looper::LooperState::Idle
                | crate::synth::looper::LooperState::Playing
        ) {
            // Sustain capture exit #4: pending offs land in the loop before
            // the close (flush is a no-op when empty).
            self.flush_looper_sustain_pending();
            self.looper.toggle_record();
            debug_assert_eq!(
                self.looper_sustain_pending_len, 0,
                "free close: pending must be flushed"
            );
        }
    }

    pub fn pitch_seq_step_atoms(&self) -> Vec<std::sync::Arc<std::sync::atomic::AtomicU8>> {
        self.parts.iter().map(|l| l.pitch_seq.step_atom()).collect()
    }

    pub fn midi_player_play_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU8> {
        self.midi_player.play_atom.clone()
    }

    pub fn midi_player_pos_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicU32> {
        self.midi_player.position_atom.clone()
    }

    pub fn set_patches(&mut self, patches: Vec<Patch>) {
        self.preset_params_cache = patches.iter().map(|p| PatchParams::from_map(&p.params)).collect();
        self.patches = patches;
    }
    pub fn set_feedback_tx(&mut self, tx: rtrb::Producer<ParamFeedback>) { self.feedback_tx = Some(tx); }
    /// Gives the engine somewhere to put heap objects it displaces (soundfonts,
    /// rustysynth instances) so they are not freed on the audio thread.
    pub fn set_trash_tx(&mut self, tx: crate::trash::TrashTx) { self.trash = Some(tx); }

    /// Sends a displaced soundfont to the GUI thread for freeing. A no-op when
    /// nothing was displaced, and a plain drop when no queue has been wired up
    /// (offline tests and benchmarks, where the audio thread does not exist).
    fn dispose_sf2(&mut self, displaced: sampler::Displaced) {
        if displaced.is_empty() { return; }
        match self.trash.as_mut() {
            Some(t) => t.dispose(crate::trash::Trash::Sf2(displaced)),
            None => drop(displaced),
        }
    }
    pub fn scope_buffer(&self) -> std::sync::Arc<ScopeBuffer> { self.scope_buf.clone() }
    pub fn set_program_change_atom(&mut self, atom: std::sync::Arc<std::sync::atomic::AtomicU8>) {
        self.program_change = Some(atom);
    }

    fn send_feedback(&mut self, fb: ParamFeedback) {
        if let Some(tx) = &mut self.feedback_tx { let _ = tx.push(fb); }
    }

    /// Reset pickup for patch-scoped CCs (call after patch change).
    fn reset_preset_pickups(&mut self) {
        for (cc, state) in self.pickup_states.iter_mut().enumerate() {
            if let Some(binding) = self.cc_map.bindings[cc] {
                if binding.scope == ParamScope::Patch {
                    state.picked_up = false;
                }
            }
        }
    }

    /// Check if a patch-scoped CC has "picked up" the current value (crossed over it).
    /// binding is Copy — no heap allocation.
    fn check_pickup(&mut self, cc: u8, new_cc_value: u8, binding: crate::cc_map::CcBinding) -> bool {
        if binding.scope == ParamScope::Global {
            return true; // globals always apply immediately
        }
        let state = &mut self.pickup_states[cc as usize];
        if state.picked_up {
            state.last_cc_value = Some(new_cc_value);
            return true;
        }
        // Get current param value and convert to CC equivalent
        // S2a: все 31 CC-live ключа через SSOT-таблицу (раньше только 4 arm'а,
        // остальные читались как 0.0 → pickup требовал прогрузки ручки в 0)
        let current_val = self.parts.first()
            .and_then(|l| l.cc_value(binding.param_key))
            .unwrap_or(0.0);
        let target_cc = CcMap::param_to_cc(&binding, current_val);

        if let Some(last) = state.last_cc_value {
            let crossed = (last <= target_cc && new_cc_value >= target_cc)
                || (last >= target_cc && new_cc_value <= target_cc);
            if crossed {
                state.picked_up = true;
                state.last_cc_value = Some(new_cc_value);
                // Notify GUI that pickup completed
                if let Some(ref mut tx) = self.feedback_tx {
                    let _ = tx.push(ParamFeedback::PickupDone { key: binding.param_key });
                }
                return true;
            }
        }
        // Send knob position so GUI shows direction indicator
        let cc_pos = CcMap::cc_to_param(&binding, new_cc_value);
        if let Some(ref mut tx) = self.feedback_tx {
            let _ = tx.push(ParamFeedback::PickupPending { key: binding.param_key, cc_position: cc_pos });
        }
        state.last_cc_value = Some(new_cc_value);
        false
    }

    pub fn handle_event(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::NoteOn { channel, note, velocity } => {
                // SMK-37 Pro transport: SEQ buttons (any channel, notes
                // 123/124) and bank-2 pads ch9 (82 = pad 25 play, 83 = pad 27
                // rec, 81 = pad 26 stop) always drive the master transport.
                // Pad notes must not fall into the trigger path.
                if velocity > 0 && (note == 123 || (channel == 9 && note == 82)) {
                    // Master transport (#32 фаза 4): single transport, no target
                    self.master_toggle_play();
                    return;
                }
                if velocity > 0 && (note == 124 || (channel == 9 && note == 83)) {
                    self.master_toggle_record();
                    return;
                }
                if channel == 9 && velocity > 0 && note == 81 {
                    self.master_stop();
                    return;
                }

                // Route drum notes (36-51): always on ch10, or any channel when drum mode active
                let is_drum_note = (36..=51).contains(&note);
                let drum_mode = self.seq_target.load(std::sync::atomic::Ordering::Relaxed) == 0;
                let sampler_drums = self.sampler.drums_enabled();
                if channel == 9 || (is_drum_note && drum_mode) {
                    // Pad bank 2 legacy undo (64, E4) lives here; 83/82/81
                    // moved above — master transport now, no target.
                    if channel == 9 && velocity > 0 && note == 64 {
                        if let Some(restored) = self.drum_engine.sequencer.undo() {
                            if let Some(ref mut tx) = self.feedback_tx {
                                let _ = tx.push(ParamFeedback::DrumPatternsSync { patterns: restored });
                            }
                        }
                        return;
                    }
                    if velocity > 0 {
                        let velocity = if self.drum_engine.fixed_velocity { 127 } else { velocity };
                        let velocity = self.drum_engine.pad_velocity(note, velocity);
                        // Слот сетки = исходная нота ПЭДА (36-51): ремап меняет
                        // только звук. Гейт по ремапнутой ноте молча убивал
                        // запись для китов с таргетами вне 36..=51
                        // (Metal_Drums_v2 — TECHDEBT #22b).
                        let pad_note = note;
                        let note = self.drum_engine.remapped_note(note);
                        if is_drum_note {
                            let slot = (pad_note - 36) as usize;
                            let sr = self.sample_rate;
                            if let Some((pat, _slot, step)) =
                                self.drum_engine.sequencer.record_hit(slot, velocity, sr)
                            {
                                if let Some(ref mut tx) = self.feedback_tx {
                                    let _ = tx.push(ParamFeedback::DrumStepRecorded {
                                        pattern: pat, slot: _slot, step, velocity,
                                    });
                                }
                            }
                        }
                        // Драмы пишутся ТОЛЬКО в сетку (кубики). В лупер не
                        // пишем: на реплее дублировали удары сетки (решение
                        // юзера 2026-10-31).
                        // Mute слота гасит и живой удар (но не запись в сетку).
                        let slot_muted = pad_note >= 36
                            && self.drum_engine.slot_mutes[(pad_note - 36) as usize];
                        if sampler_drums && !slot_muted {
                            let choke = self.drum_engine.sf2_choke_take(note);
                            if choke != 0 { self.sampler.drum_note_off(choke); }
                            self.sampler.drum_note_on(note, velocity);
                        } else if !slot_muted {
                            // DSP: слот = звук пэда; ремап-таргет вне 36..=51
                            // молчал бы в note_on — живой удар не должен
                            // зависеть от ремапа (remap = только SF2-сэмплы)
                            self.drum_engine.note_on(pad_note, velocity);
                        }
                    } else {
                        let note = self.drum_engine.remapped_note(note);
                        if sampler_drums {
                            self.sampler.drum_note_off(note);
                        } else {
                            self.drum_engine.note_off(note);
                        }
                    }
                } else if self.arpeggiator.enabled {
                    // Route through arpeggiator — it generates notes in tick_block
                    if velocity == 0 {
                        self.arpeggiator.note_off(note);
                        self.flush_arp_note_off();
                    } else {
                        self.arpeggiator.note_on(note, velocity);
                    }
                } else if velocity == 0 {
                    for i in 0..self.parts.len() {
                        if !self.parts[i].enabled { continue; }
                        self.route_note_off(i, note);
                        // Whole-performance recording: tag with the part that
                        // receives the off (looper replay routes per part id).
                        // defer_ok: user MIDI off — subject to sustain capture.
                        self.record_for_looper(note, 0, i as u8, true);
                    }
                } else {
                    for i in 0..self.parts.len() {
                        if !self.parts[i].enabled || self.parts[i].mute { continue; }
                        let sounded = self.route_note_on(i, note, velocity);
                        if sounded {
                            // Записывается ТОЛЬКО реально звучащая часть:
                            // реплей honoured range/transpose каждого парта.
                            // Off-мусор (offs для частей, не звучавших ноту)
                            // отсекается фильтром active_notes в
                            // looper.record_event; split overlap активных нот
                            // двух частей — оверрайд одиночного слота
                            // active_notes остаётся accepted.
                            self.record_for_looper(note, velocity, i as u8, false);
                        }
                    }
                }
            }
            MidiEvent::NoteOff { channel, note } => {
                if channel == 9 {
                    let note = self.drum_engine.remapped_note(note);
                    if self.sampler.drums_enabled() {
                        self.sampler.drum_note_off(note);
                    } else {
                        self.drum_engine.note_off(note);
                    }
                } else if self.arpeggiator.enabled {
                    self.arpeggiator.note_off(note);
                    self.flush_arp_note_off();
                } else {
                    for i in 0..self.parts.len() {
                        self.route_note_off(i, note);
                        // Whole-performance recording: tag with the part that
                        // receives the off (looper replay routes per part id).
                        // defer_ok: user MIDI off — subject to sustain capture.
                        self.record_for_looper(note, 0, i as u8, true);
                    }
                }
            }
            MidiEvent::PitchBend { value, .. } => {
                self.pitch_bend_norm = value;
                // УНИФИКАЦИЯ (юзер, 2026-11-05): масштаб бенда — ТОЛЬКО
                // глобальный PB-фейдер (он же задаёт ранж SF2-каналов через
                // SetGlobalParam→set_pitch_bend_range). Прежний дивергентный
                // путь «Range Up/Down первого включённого парта» убран: один
                // парт молча менял бенд всем остальным. Asymmetric up/down
                // остаются глобальными полями (pitch_bend_up/down).
                let (up, down) = if self.global_params.pitch_bend_up > 0.0 {
                    (self.global_params.pitch_bend_up, self.global_params.pitch_bend_down)
                } else {
                    (self.global_params.pitch_bend_range, self.global_params.pitch_bend_range)
                };
                self.pitch_bend_semitones = if value >= 0.0 { value * up } else { value * down };
                // Все SF2-партии (не только 0..1): канальная адресация в
                // sampler.pitch_bend теперь динамическая (part_channel_of)
                for i in 0..self.parts.len() {
                    if self.parts.get(i).map(|l| l.sf2_mode && !l.ignore_pb).unwrap_or(false) {
                        self.sampler.pitch_bend(i, value);
                    }
                }
            }
            MidiEvent::ModWheel { value, .. } => {
                self.mod_wheel = value;
                // Forward to SF2 sampler as CC1 on ALL SF2 parts (см. PitchBend)
                for i in 0..self.parts.len() {
                    if self.parts.get(i).map(|l| l.sf2_mode && !l.ignore_mw).unwrap_or(false) {
                        self.sampler.mod_wheel(i, value);
                    }
                }
            }
            MidiEvent::ProgramChange { program, .. } => {
                // In SF2 mode, change the SF2 program; in synth mode, change the synth patch
                if self.parts.first().map(|l| l.sf2_mode).unwrap_or(false) {
                    self.sampler.set_part_program(0, program, 0);
                } else if let Some(&params) = self.preset_params_cache.get(program as usize) {
                    if let Some(l) = self.parts.get_mut(0) {
                        // Seamless switch: same latch-mismatch release as
                        // LoadPatch — must run BEFORE the params swap, while
                        // l.params still holds the OLD play_mode.
                        l.release_latch_mismatch(params.play_mode as u8);
                        l.params = params;
                        // Диапазон парта — собственность SetPartRange/сплита:
                        // ProgramChange не перетирает его (#41, резидуал —
                        // LoadPatch уже перестал это делать)
                        if let Some(patch) = self.patches.get(program as usize) {
                            l.mod_matrix.load_from_params(&patch.params);
                            l.pitch_seq.load_from_params(&patch.params);
                        }
                        l.macro_vals = params.macro_vals;
                        self.arpeggiator.set_params(
                            params.arp_enabled > 0.5,
                            params.arp_mode as u8,
                            params.arp_rate as u8,
                            params.arp_octaves as u8,
                            params.arp_gate,
                        );
                        self.arpeggiator.set_latch(params.arp_latch > 0.5);
                        // PC switches params/matrix/pitch-seq/macros/arp;
                        // MSEG/wavetable/LFO-stepseq need full LoadPatch —
                        // design limit
                        l.update_routing_cache();
                    }
                    self.reset_preset_pickups();
                }
                if let Some(atom) = &self.program_change {
                    atom.store(program, std::sync::atomic::Ordering::Relaxed);
                }
                self.send_feedback(ParamFeedback::ProgramChanged { program });
            }
            MidiEvent::Aftertouch { channel, value } => {
                if channel != 9 { self.aftertouch = value; }
            }
            MidiEvent::PolyAftertouch { channel, note, pressure } => {
                if channel != 9 {
                    for part in &mut self.parts {
                        for voice in &mut part.voices {
                            // src_note = raw key; poly AT was always sent raw
                            if voice.src_note == note && voice.active {
                                voice.poly_aftertouch = pressure;
                            }
                        }
                    }
                }
            }
            MidiEvent::ControlChange { cc, value, channel, .. } => {
                // Channel mode messages. 120 = All Sound Off, 123 = All Notes
                // Off. Hardware sends these from the panic button and when the
                // transport stops, so they take priority over any CC mapping.
                if cc == 120 || cc == 123 {
                    // Lift the sustain state everywhere first: rustysynth
                    // gates CC123 on the channel hold pedal, and the GUI
                    // sustain atom must not stay stuck ON.
                    self.set_sustain(0);
                    let all_sound_off = cc == 120; // All Sound Off kills, All Notes Off releases
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        part.drain_notes(&mut self.sampler, i, all_sound_off);
                    }
                    if all_sound_off {
                        self.sampler.all_sound_off();
                    } else {
                        self.sampler.all_notes_off();
                    }
                    self.drum_engine.reset_choke_state();
                    if let Some(off_note) = self.arpeggiator.flush_current_note() {
                        // Same transposed routing as flush_arp_note_off.
                        self.dispatch_arp_note_off(off_note);
                    }
                    // Panic must also drop held/latched arp notes — held
                    // notes surviving CC120 would resume the old pattern on
                    // the next key (AllNotesOff already clears, mod.rs:2211).
                    self.arpeggiator.clear();
                    return;
                }
                // Drum-канал (9 = GM ch10): CC на нём не должен управлять
                // синт-партами (breath/expression/sustain/CC1-4), как
                // Aftertouch/PolyAT выше (audit mod-matrix #CC-channel).
                // Принятый side-effect: channel-9 CC исчезают из CcReceived
                // GUI-фидбека — осознанно (CC-learn на drum-канале не фича).
                if channel == 9 { return; }
                // Check assignable mod sources (CC1..CC4)
                for i in 0..4 {
                    if cc == self.cc_numbers[i] {
                        self.cc_values[i] = value as f32 / 127.0;
                    }
                }
                // Dedicated mod sources: Breath (CC2), Expression (CC11), Sustain (CC64)
                if cc == 64 {
                    self.set_sustain(value);
                } else {
                    for part in &mut self.parts {
                        match cc {
                            2  => part.breath = value as f32 / 127.0,
                            11 => part.expression = value as f32 / 127.0,
                            _ => {}
                        }
                    }
                }
                // CcBinding is Copy — no heap allocation
                // Defense in depth: even a hand-edited config binding on CC64
                // must not steal the sustain pedal.
                let binding = if cc == 64 { None } else { self.cc_map.bindings[cc as usize] };
                if let Some(binding) = binding {
                    if self.check_pickup(cc, value, binding) {
                        let param_val = CcMap::cc_to_param(&binding, value);
                        if binding.scope == ParamScope::Global {
                            self.global_params.set(binding.param_key, param_val);
                        } else {
                            for part in &mut self.parts { part.set_param(binding.param_key, param_val); }
                        }
                        self.send_feedback(ParamFeedback::ParamChanged { key: binding.param_key, value: param_val });
                    }
                }
                self.send_feedback(ParamFeedback::CcReceived { cc });
            }
            MidiEvent::Navigate { pressed } => {
                self.send_feedback(if pressed {
                    ParamFeedback::NavigatePress
                } else {
                    ParamFeedback::NavigateRelease
                });
            }
        }
    }

    /// Applies a control event.
    ///
    /// Returns the event back when it owns heap memory, so the caller can hand
    /// it to the trash queue instead of calling free() on the audio thread.
    /// Returns `None` for the plain-data variants, which cost nothing to drop.
    #[must_use = "heap-owning events must be sent to the trash queue"]
    pub fn handle_control(&mut self, event: ControlEvent) -> Option<ControlEvent> {
        match event {
            ControlEvent::LoadPatch(_)
            | ControlEvent::SetPartFull { .. }
            | ControlEvent::SetPartEnabled { .. }
            | ControlEvent::SetPartMute { .. }
            | ControlEvent::SetPartIgnorePb { .. }
            | ControlEvent::SetPartIgnoreMw { .. }
            | ControlEvent::SetPartNoteMode { .. }
            | ControlEvent::SetPartSustain { .. }
            | ControlEvent::SetPartVolume { .. }
            | ControlEvent::SetPartRange { .. }
            | ControlEvent::SetPartVelRange { .. }
            | ControlEvent::SetPartPan { .. }
            | ControlEvent::SetPartTranspose { .. }
            | ControlEvent::SetPartMono { .. }
            | ControlEvent::SetPartLegato { .. }
            | ControlEvent::SetPartGlide { .. }
            | ControlEvent::SetMacro { .. }
            | ControlEvent::SetCcMap { .. }
            | ControlEvent::SetGlobalParam { .. }
            => self.hc_patch_part(event),
            ControlEvent::DrumSetStep { .. }
            | ControlEvent::DrumSetParam { .. }
            | ControlEvent::DrumSetMute { .. }
            | ControlEvent::DrumSetVolume { .. }
            | ControlEvent::DrumSeqPlay { .. }
            | ControlEvent::DrumSeqBpm { .. }
            | ControlEvent::DrumSeqSwing { .. }
            | ControlEvent::DrumSeqPattern { .. }
            | ControlEvent::DrumSeqQueuePattern { .. }
            | ControlEvent::DrumSeqChain { .. }
            | ControlEvent::DrumSeqChainToggle { .. }
            | ControlEvent::DrumSeqLength { .. }
            | ControlEvent::DrumSeqTimeSig { .. }
            | ControlEvent::DrumSeqRecord { .. }
            | ControlEvent::DrumSeqClear
            | ControlEvent::DrumSeqUndo
            | ControlEvent::DrumSeqRedo
            | ControlEvent::DrumLoadKit { .. }
            | ControlEvent::DrumSetFixedVelocity { .. }
            | ControlEvent::DrumSetMetronome { .. }
            | ControlEvent::DrumSetRemap { .. }
            | ControlEvent::DrumSetChokeGroups { .. }
            | ControlEvent::DrumSetPadVel { .. }
            => self.hc_drums(event),
            ControlEvent::LooperRecord
            | ControlEvent::LooperStopRecord
            | ControlEvent::LooperTogglePlay
            | ControlEvent::LooperUndo
            | ControlEvent::LooperClear
            | ControlEvent::LooperSetBars { .. }
            | ControlEvent::LooperSetQuantize { .. }
            | ControlEvent::LooperRestore { .. }
            | ControlEvent::LooperSetLayerMute { .. }
            | ControlEvent::LooperSetSolo { .. }
            | ControlEvent::LooperSetPartMute { .. }
            | ControlEvent::LooperSetSource { .. }
            | ControlEvent::MasterPlay
            | ControlEvent::MasterStop
            | ControlEvent::MasterOverdub
            | ControlEvent::CaptureCycleAdd
            | ControlEvent::CaptureCycleRemove
            | ControlEvent::MasterRecord
            | ControlEvent::SetSustain { .. }
            | ControlEvent::SetSustainRelease { .. }
            | ControlEvent::AllNotesOff
            => self.hc_looper_transport(event),
            ControlEvent::LoadKeysSoundFont { .. }
            | ControlEvent::LoadDrumsSoundFont { .. }
            | ControlEvent::UnloadKeysSoundFont
            | ControlEvent::UnloadDrumsSoundFont
            | ControlEvent::SetPartSf2Mode { .. }
            | ControlEvent::SetPartSf2Program { .. }
            | ControlEvent::SetDrumsSf2Mode { .. }
            | ControlEvent::SetSf2BlockSize { .. }
            | ControlEvent::SetSf2ReleaseScale { .. }
            | ControlEvent::SetDspTrim { .. }
            | ControlEvent::SetSf2Trim { .. }
            | ControlEvent::SetSf2SampleOffset { .. }
            => self.hc_sf2(event),
            ControlEvent::SeqSetEnabled { .. }
            | ControlEvent::SeqSetStep { .. }
            | ControlEvent::SeqSetLength { .. }
            | ControlEvent::SeqSetRate { .. }
            | ControlEvent::SeqSetScale { .. }
            | ControlEvent::SeqSetSwing { .. }
            | ControlEvent::MidiSeqLoad { .. }
            | ControlEvent::MidiSeqPlay { .. }
            | ControlEvent::MidiSeqSetBpm { .. }
            | ControlEvent::MidiSeqSetLooping { .. }
            | ControlEvent::MidiSeqSetTrackInstrument { .. }
            | ControlEvent::MidiSeqSetTrackMute { .. }
            | ControlEvent::SetArpParams { .. }
            | ControlEvent::SetArpLatch { .. }
            => self.hc_seq(event),
        }
    }

    fn hc_patch_part(&mut self, event: ControlEvent) -> Option<ControlEvent> {
        match event {
            ControlEvent::LoadPatch(mut ev) => {
                // Take the contents out of the box rather than unboxing it:
                // `*ev` would drop the allocation right here, on the audio
                // thread. The emptied box travels back to the GUI thread.
                let part = ev.part;
                let params = ev.params; // PatchParams is Copy
                let mod_matrix = std::mem::take(&mut ev.mod_matrix);
                let mseg1 = ev.mseg1.take();
                let mseg2 = ev.mseg2.take();
                let pitch_seq = ev.pitch_seq.take();
                let lfo_step_seq = ev.lfo_step_seq.take();
                let wavetable = ev.wavetable.take();
                if let Some(l) = self.parts.get_mut(part) {
                    // Seamless switch (TECHDEBT: политика «хвосты живы»):
                    // sounding voices keep the OLD patch (params are copied
                    // into the voice at note_on); pedal-up / key-offs release
                    // them naturally (set_sustain:1484-1518, note_off:835).
                    // Only a latch-mode mismatch releases the latch-held
                    // voices — a non-latch patch cannot toggle them off, and
                    // they would ring forever (audit fact 6).
                    l.release_latch_mismatch(params.play_mode as u8);
                    // Sync macro values from preset into live part state
                    // Load wavetable data if provided. `replace` hands the
                    // previous Arc back into the event instead of dropping the
                    // last reference here — a wavetable is a Vec<f32> of tens
                    // of thousands of samples.
                    let previous_wt = match wavetable {
                        Some((data, frames, fsize)) => {
                            l.wavetable_frames = frames;
                            l.wavetable_frame_size = fsize;
                            l.wavetable_data.replace(data)
                        }
                        None => l.wavetable_data.take(),
                    };
                    ev.wavetable = previous_wt.map(|d| (d, 0, 0));
                    l.macro_vals = params.macro_vals;
                    l.params = params;
                    l.mod_matrix = mod_matrix;
                    // Mseg owns a Vec<MsegSegment>, so plain assignment would
                    // free the outgoing envelope on the audio thread. Swap it
                    // into the event and let the GUI thread drop it.
                    if let Some(mut m) = mseg1 {
                        std::mem::swap(&mut l.mseg1, &mut m);
                        ev.mseg1 = Some(m);
                    }
                    if let Some(mut m) = mseg2 {
                        std::mem::swap(&mut l.mseg2, &mut m);
                        ev.mseg2 = Some(m);
                    }
                    // Copy the pattern, keep our own step atom: see
                    // PitchSequencer::adopt.
                    if let Some(sq) = pitch_seq {
                        l.pitch_seq.adopt(&sq);
                        ev.pitch_seq = Some(sq);
                    }
                    if let Some(ref sp) = lfo_step_seq {
                        for (i, lfo) in l.lfos.iter_mut().enumerate().take(4) {
                            lfo.apply_step_seq(&sp[i]);
                        }
                    }
                    l.update_routing_cache();
                    // Sync arpeggiator params from loaded patch
                    self.arpeggiator.set_params(
                        params.arp_enabled > 0.5,
                        params.arp_mode as u8,
                        params.arp_rate as u8,
                        params.arp_octaves as u8,
                        params.arp_gate,
                    );
                    self.arpeggiator.set_latch(params.arp_latch > 0.5);
                }
                self.reset_preset_pickups();
                return Some(ControlEvent::LoadPatch(ev));
            }
            ControlEvent::SetPartFull { part, mut state } => {
                // ST2: транзакция = 1 слот кольца. Композиция СУЩЕСТВУЮЩИХ
                // хэндлеров в каноническом порядке push_full_part_state —
                // семантический паритет по конструкции (change-detection
                // sf2_mode живёт в SetPartSf2Mode arm'е; SetPartMute НЕ имеет
                // change-detection — паритет: drain'ит в обеих формах).
                // RT: state-box НЕ дропать здесь — trash.
                let macro_vals = state.macro_vals;
                let sf2_mode = state.sf2_mode;
                let sf2_program = state.sf2_program;
                let sub: [ControlEvent; 12] = [
                    ControlEvent::SetPartEnabled { part, enabled: state.enabled },
                    ControlEvent::SetPartMute { part, mute: state.mute },
                    ControlEvent::SetPartVolume { part, volume: state.volume },
                    ControlEvent::SetPartRange { part, min_note: state.min_note, max_note: state.max_note },
                    ControlEvent::SetPartVelRange { part, vel_min: state.vel_min, vel_max: state.vel_max },
                    ControlEvent::SetPartPan { part, pan: state.pan },
                    ControlEvent::SetPartTranspose { part, semitones: state.transpose },
                    ControlEvent::SetPartNoteMode { part, mode: state.note_mode },
                    ControlEvent::SetPartSustain { part, on: state.sustain_enabled },
                    ControlEvent::SetPartIgnorePb { part, on: state.ignore_pb },
                    ControlEvent::SetPartIgnoreMw { part, on: state.ignore_mw },
                    ControlEvent::SetPartSf2Mode { part, enabled: sf2_mode },
                ];
                let mut spent: Option<ControlEvent> = None;
                for ev in sub {
                    if let Some(heap) = self.handle_control(ev) {
                        // среди 12 plain-data arm'ов heap не бывает — guard
                        spent = Some(heap);
                        break;
                    }
                }
                if spent.is_none() {
                    for (index, &value) in macro_vals.iter().enumerate() {
                        if let Some(heap) = self.handle_control(
                            ControlEvent::SetMacro { part, index, value },
                        ) {
                            spent = Some(heap);
                            break;
                        }
                    }
                }
                if spent.is_none() && sf2_mode {
                    if let Some(heap) = self.handle_control(
                        ControlEvent::SetPartSf2Program {
                            part, program: sf2_program, bank: 0,
                        },
                    ) {
                        spent = Some(heap);
                    }
                }
                // patch ПОСЛЕДНИМ (GUI-порядок: full state, затем LoadPatch).
                // Box по указателю: DerefMut part; НЕ распаковывать
                // (`*boxed` = free на аудио-потоке — канон LoadPatch).
                let mut patch_spent: Option<ControlEvent> = None;
                if spent.is_none() {
                    if let Some(boxed) = state.patch.as_mut() {
                        boxed.part = part; // DerefMut, no alloc
                        patch_spent = self.handle_control(
                            ControlEvent::LoadPatch(state.patch.take().unwrap()),
                        );
                    }
                }
                // Trash: defensive guard `spent` (heap среди plain arm'ов не
                // бывает) + сам state-box (иначе free случился бы на
                // аудио-потоке). Spent LoadPatch ПРОБУЛИВАЕТСЯ вызывающему
                // через return — канон LoadPatch-контракта (`#[must_use]`,
                // audio callback отдаёт его в trash); глотать его здесь
                // нельзя (утечка Box).
                if let Some(t) = self.trash.as_mut() {
                    if let Some(heap) = spent {
                        t.dispose(crate::trash::Trash::Control(heap));
                    }
                    t.dispose(crate::trash::Trash::PartFull(state));
                }
                // Другие arm'ы матча — `()` (heap уходит через `return`),
                // поэтому и здесь явный return.
                return patch_spent;
            }
            ControlEvent::SetPartEnabled { part, enabled } => {
                if let Some(l) = self.parts.get_mut(part) {
                    if !enabled {
                        l.drain_notes(&mut self.sampler, part, false);
                    }
                    l.enabled = enabled;
                }
            }
            ControlEvent::SetPartMute { part, mute } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.mute = mute;
                    if mute {
                        // Drain both voice owners + held-note state; SF2 output
                        // is summed globally in tick_block, so also gate at the
                        // source with CC7 (instant; the release drain above
                        // keeps the tail short).
                        l.drain_notes(&mut self.sampler, part, false);
                        self.sampler.set_layer_volume(part, 0.0);
                    } else {
                        // Restore the SF2 channel volume zeroed on mute.
                        self.sampler.set_layer_volume(part, l.volume);
                        // Re-assert the hold-pedal: mute's drain_notes sent
                        // CC64=0 to the channel (part_all_notes_off), so an
                        // unmuted part silently ignored sustain until the
                        // pedal was physically re-pressed (#53-followup).
                        self.sampler
                            .set_part_sustain(part, l.sustain_pedal > 0.5 && l.sustain_enabled);
                    }
                }
            }
            ControlEvent::SetPartIgnorePb { part, on } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.ignore_pb = on;
                    // SF2-канал: сброс в центр немедленно; при un-ignore —
                    // СЫРОЕ нормализованное значение (sampler.pitch_bend
                    // ждёт -1..1: sampler.rs:565 (v+1)*0.5*16383;
                    // pitch_bend_semitones содержит range-масштаб — не годится).
                    if l.sf2_mode {
                        let norm = if l.ignore_pb { 0.0 } else { self.pitch_bend_norm };
                        self.sampler.pitch_bend(part, norm);
                    }
                }
            }
            ControlEvent::SetPartIgnoreMw { part, on } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.ignore_mw = on;
                    // Аналогично PB: ignore → 0, un-ignore → текущее колесо.
                    if l.sf2_mode {
                        let v = if l.ignore_mw { 0.0 } else { self.mod_wheel };
                        self.sampler.mod_wheel(part, v);
                    }
                }
            }
            ControlEvent::SetPartNoteMode { part, mode } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.note_mode = mode;
                    // Reset tracking on EVERY change (включая повторный клик
                    // по активному режиму — семантика старого SetPartRootOnly):
                    // stale root_active across the change would re-key on the
                    // next unrelated note-off; stale root_held would feed a
                    // ghost chord into detect_root.
                    l.root_held = [false; 128];
                    l.root_active = None;
                }
            }
            ControlEvent::SetPartSustain { part, on } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.sustain_enabled = on;
                    if l.sustain_pedal > 0.5 {
                        // Педаль внизу: немедленно применить маску к SF2-каналу
                        // (re-enable тоже: канал должен получить CC64=127 again).
                        self.sampler.set_part_sustain(part, on);
                    }
                    if !on && l.sustain_pedal > 0.5 {
                        l.release_pedal_held(self.sustain_release);
                    }
                }
            }
            ControlEvent::SetPartVolume { part, volume } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.volume = volume;
                    // Muted parts are gated at the sampler with CC7 (see
                    // SetPartMute); keep the gate closed or a volume-knob
                    // tweak would un-mute the part while l.mute stays true.
                    self.sampler.set_layer_volume(part, if l.mute { 0.0 } else { volume });
                }
            }
            ControlEvent::SetPartRange { part, min_note, max_note } => {
                if let Some(l) = self.parts.get_mut(part) { l.min_note = min_note; l.max_note = max_note; }
            }
            ControlEvent::SetPartVelRange { part, vel_min, vel_max } => {
                if let Some(l) = self.parts.get_mut(part) { l.vel_min = vel_min; l.vel_max = vel_max; }
            }
            ControlEvent::SetPartPan { part, pan } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pan = pan.clamp(-1.0, 1.0);
                    // SF2: pass pan through as CC10 on the part's sampler channel
                    // (rustysynth applies per-channel pan internally).  DSP parts
                    // get constant-power pan in tick_block.
                    if l.sf2_mode {
                        let cc = (l.pan.clamp(-1.0, 1.0) * 0.5 + 0.5) * 127.0;
                        self.sampler.set_channel_pan(part, cc as i32);
                    }
                }
            }
            ControlEvent::SetPartTranspose { part, semitones } => {
                // DSP voices match note-offs by src_note (raw), so held voices
                // stay releasable across the change. SF2 voices transpose
                // engine-side and are covered by the state-change drains
                // (SetPartSf2Mode/ProgramChange/panic) — no extra action here.
                if let Some(l) = self.parts.get_mut(part) { l.transpose = semitones; }
            }
            ControlEvent::SetPartMono { part, mono } => {
                if let Some(l) = self.parts.get_mut(part) {
                    let cur = l.params.play_mode as u8;
                    match (mono, cur) {
                        (true, 0) | (true, 6) => l.params.play_mode = 1.0, // poly/piano → mono
                        (true, 2) => l.params.play_mode = 1.0,             // mono-ST → mono
                        (false, 1) | (false, 2) => l.params.play_mode = 0.0, // → poly
                        _ => {} // already in the requested state; latch/high/low untouched
                    }
                    // A stale mono stack from the previous mode must not
                    // resurrect notes in the new mode.
                    l.note_stack.clear();
                }
            }
            ControlEvent::SetPartLegato { part, legato } => {
                if let Some(l) = self.parts.get_mut(part) {
                    let cur = l.params.play_mode as u8;
                    if legato && cur == 1 {
                        l.params.play_mode = 2.0;
                    } else if !legato && cur == 2 {
                        l.params.play_mode = 1.0;
                    }
                }
            }
            ControlEvent::SetPartGlide { part, ms } => {
                if let Some(l) = self.parts.get_mut(part) {
                    let secs = (ms / 1000.0).clamp(0.0, 2.0);
                    l.params.portamento_time = secs;
                    if secs > 0.0 && l.params.portamento_mode < 0.5 {
                        l.params.portamento_mode = 1.0; // "Always" — a glide time without a mode is dead config
                    }
                }
            }
            ControlEvent::SetMacro { part, index, value } => {
                if let Some(l) = self.parts.get_mut(part) {
                    if index < 8 {
                        l.macro_vals[index] = value.clamp(0.0, 1.0);
                        l.params.macro_vals[index] = value.clamp(0.0, 1.0);
                    }
                }
            }
            ControlEvent::SetCcMap { map } => {
                // CcMap is Copy, so this reads through the box without
                // consuming it; the allocation goes back to the GUI thread.
                self.cc_map = *map;
                return Some(ControlEvent::SetCcMap { map });
            }
            ControlEvent::SetGlobalParam { key, value } => {
                self.global_params.set(key, value);
                if key == "pitch_bend_range" {
                    log::debug!("[engine] SetGlobalParam pitch_bend_range={}", value);
                    self.sampler.set_pitch_bend_range(value.clamp(1.0, 24.0) as u8);
                }
            }
            _ => {}
        }
        // Every variant that owns heap memory returns early above; reaching
        // here means the event was plain data and costs nothing to drop.
        None
    }

    fn hc_drums(&mut self, event: ControlEvent) -> Option<ControlEvent> {
        match event {
            ControlEvent::DrumSetStep { slot, step, velocity } => {
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                self.drum_engine.sequencer.patterns[pat].steps[slot as usize][step as usize].velocity = velocity;
            }
            ControlEvent::DrumSetParam { slot, param } => {
                let s = slot as usize;
                match param {
                    DrumParam::Level(v) => self.drum_engine.params[s].level = v,
                    DrumParam::Pan(v) => self.drum_engine.params[s].pan = v,
                    DrumParam::Tune(v) => self.drum_engine.params[s].tune = v,
                    DrumParam::Decay(v) => self.drum_engine.params[s].decay = v,
                }
            }
            ControlEvent::DrumSetMute { slot, mute } => {
                if let Some(m) = self.drum_engine.slot_mutes.get_mut(slot as usize) {
                    *m = mute;
                }
            }
            ControlEvent::DrumSetVolume { volume } => {
                self.drum_engine.volume = volume;
                self.sampler.drum_volume = volume;
            }
            ControlEvent::DrumSeqPlay { playing } => {
                self.drum_engine.sequencer.playing = playing;
                if playing { self.drum_engine.sequencer.reset(); }
            }
            ControlEvent::DrumSeqBpm { bpm } => { self.drum_engine.sequencer.bpm = bpm; }
            ControlEvent::DrumSeqSwing { swing } => { self.drum_engine.sequencer.swing = swing; }
            ControlEvent::DrumSeqPattern { pattern } => {
                // Immediate switch: hardware console transport path (see the
                // DrumSeqQueuePattern note above for the asymmetry).
                self.drum_engine.sequencer.set_pattern_immediate(pattern);
            }
            ControlEvent::DrumSeqQueuePattern { pattern } => {
                self.drum_engine.sequencer.queue_pattern(pattern);
            }
            ControlEvent::DrumSeqChain { on } => {
                self.drum_engine.sequencer.set_chain(on);
            }
            ControlEvent::DrumSeqChainToggle { pattern } => {
                self.drum_engine.sequencer.chain_toggle(pattern);
            }
            ControlEvent::DrumSeqLength { length } => {
                // Clamp: record_hit делит на len (drum.rs) — ноль из кольца
                // это div-by-zero; 0..=255 → 1..=16.
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                self.drum_engine.sequencer.patterns[pat].length = length.clamp(1, 16);
            }
            ControlEvent::DrumSeqTimeSig { beats_per_bar } => {
                // Signature = beats_per_bar; the bar IS `length` steps of 16ths,
                // so length follows: 4 steps per beat at any signature.
                // Clamp 2..=4: MAX_STEPS = 16 caps the bar at 4/4.
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                let bpb = beats_per_bar.clamp(2, 4);
                self.drum_engine.sequencer.patterns[pat].beats_per_bar = bpb;
                self.drum_engine.sequencer.patterns[pat].length = bpb * 4;
            }
            ControlEvent::DrumSeqRecord { recording } => {
                if recording && !self.drum_engine.sequencer.recording {
                    self.drum_engine.sequencer.save_snapshot();
                }
                self.drum_engine.sequencer.recording = recording;
                if recording && !self.drum_engine.sequencer.playing {
                    self.drum_engine.set_playing(true);
                }
            }
            ControlEvent::DrumSeqClear => {
                self.drum_engine.sequencer.clear_pattern();
                if let Some(ref mut tx) = self.feedback_tx {
                    let _ = tx.push(ParamFeedback::DrumCleared {
                        pattern: self.drum_engine.sequencer.current_pattern,
                    });
                }
            }
            ControlEvent::DrumSeqUndo => {
                if let Some(restored) = self.drum_engine.sequencer.undo() {
                    if let Some(ref mut tx) = self.feedback_tx {
                        let _ = tx.push(ParamFeedback::DrumPatternsSync { patterns: restored });
                    }
                }
            }
            ControlEvent::DrumSeqRedo => {
                if let Some(restored) = self.drum_engine.sequencer.redo() {
                    if let Some(ref mut tx) = self.feedback_tx {
                        let _ = tx.push(ParamFeedback::DrumPatternsSync { patterns: restored });
                    }
                }
            }
            ControlEvent::DrumLoadKit { patterns, params, bpm, swing, volume } => {
                self.drum_engine.sequencer.cancel_redo();
                // Both arrays are Copy; the boxes travel back for freeing.
                self.drum_engine.sequencer.patterns = *patterns;
                self.drum_engine.params = *params;
                self.drum_engine.sequencer.bpm = bpm;
                self.drum_engine.sequencer.swing = swing;
                self.drum_engine.volume = volume;
                self.sampler.drum_volume = volume;
                return Some(ControlEvent::DrumLoadKit { patterns, params, bpm, swing, volume });
            }
            ControlEvent::DrumSetFixedVelocity { fixed } => {
                self.drum_engine.fixed_velocity = fixed;
            }
            ControlEvent::DrumSetMetronome { on } => {
                self.drum_engine.metronome = on;
            }
            ControlEvent::DrumSetRemap { table } => {
                // The box travels back for freeing on the GUI thread.
                self.drum_engine.apply_remap_vec(&*table);
                return Some(ControlEvent::DrumSetRemap { table });
            }
            ControlEvent::DrumSetChokeGroups { table } => {
                self.drum_engine.set_choke_groups(&table);
                return Some(ControlEvent::DrumSetChokeGroups { table });
            }
            ControlEvent::DrumSetPadVel { table } => {
                self.drum_engine.apply_pad_vel_vec(&*table);
                return Some(ControlEvent::DrumSetPadVel { table });
            }
            _ => {}
        }
        // Every variant that owns heap memory returns early above; reaching
        // here means the event was plain data and costs nothing to drop.
        None
    }

    fn hc_looper_transport(&mut self, event: ControlEvent) -> Option<ControlEvent> {
        match event {
            ControlEvent::LooperRecord => { self.looper.start_record(); }
            ControlEvent::LooperStopRecord => {
                // Sustain capture exits #6/#7: pending offs land in the loop
                // before the capture ends (no-op when pending is empty).
                self.flush_looper_sustain_pending();
                self.looper.stop_record();
                debug_assert_eq!(
                    self.looper_sustain_pending_len, 0,
                    "LooperStopRecord: pending must be flushed"
                );
            }
            ControlEvent::LooperTogglePlay => {
                self.flush_looper_sustain_pending();
                self.looper.toggle_play();
                debug_assert_eq!(
                    self.looper_sustain_pending_len, 0,
                    "LooperTogglePlay: pending must be flushed"
                );
            }
            ControlEvent::LooperUndo => { self.looper.undo(); }
            ControlEvent::LooperClear => { self.looper.clear(); }
            ControlEvent::LooperSetBars { bars } => { self.looper.bars = bars; }
            ControlEvent::LooperSetQuantize { quantize } => {
                self.looper.quantize = looper::Quantize::from_index(quantize);
            }
            ControlEvent::LooperRestore { events, loop_length_ticks } => {
                self.looper.restore(&events, loop_length_ticks);
                // Heap-owning payload: hand it back for GUI-thread disposal.
                return Some(ControlEvent::LooperRestore { events, loop_length_ticks });
            }
            ControlEvent::LooperSetLayerMute { layer, mute } => {
                let note_offs = self.looper.set_layer_mute(layer, mute);
                self.route_looper_note_offs(&note_offs);
            }
            ControlEvent::LooperSetSolo { layer } => {
                let note_offs = self.looper.set_solo(layer);
                self.route_looper_note_offs(&note_offs);
            }
            ControlEvent::LooperSetPartMute { part, mute } => {
                let note_offs = self.looper.set_part_mute(part, mute);
                self.route_looper_note_offs(&note_offs);
            }
            ControlEvent::LooperSetSource { mix } => {
                self.looper_source_mix = mix;
            }
            ControlEvent::MasterPlay => {
                self.master_toggle_play();
            }
            ControlEvent::MasterStop => {
                self.master_stop();
            }
            ControlEvent::MasterOverdub => {
                self.master_toggle_overdub();
            }
            ControlEvent::CaptureCycleAdd => {
                // +1 цикл: логическая длина растёт (записывать можно дальше);
                // лупер держится с запасом — wrap'а во время записи нет
                if self.looper_capture_quantized && self.looper_rec_cycles > 0 {
                    self.looper_rec_cycles += 1;
                    self.keep_capture_capacity();
                }
            }
            ControlEvent::CaptureCycleRemove => {
                // −1 цикл: логическая длина ужимается, курсор записи откатывается
                if self.looper_capture_quantized && self.looper_rec_cycles > 1 {
                    self.looper_rec_cycles -= 1;
                    let kept = self.looper_rec_cycles.saturating_mul(self.looper_cycle_ticks);
                    self.looper.trim_after(kept);
                }
            }
            ControlEvent::MasterRecord => {
                self.master_toggle_record();
            }
            ControlEvent::SetSustain { on } => {
                self.set_sustain(if on { 127 } else { 0 });
            }
            ControlEvent::SetSustainRelease { seconds } => {
                self.sustain_release = seconds;
            }
            ControlEvent::AllNotesOff => {
                self.set_sustain(0);
                for (i, part) in self.parts.iter_mut().enumerate() {
                    part.drain_notes(&mut self.sampler, i, false);
                }
                self.sampler.all_notes_off();
                self.drum_engine.reset_choke_state();
                // Clear arpeggiator held notes
                if let Some(off_note) = self.arpeggiator.flush_current_note() {
                    // Same transposed routing as flush_arp_note_off.
                    self.dispatch_arp_note_off(off_note);
                }
                self.arpeggiator.clear();
            }
            _ => {}
        }
        // Every variant that owns heap memory returns early above; reaching
        // here means the event was plain data and costs nothing to drop.
        None
    }

    fn hc_sf2(&mut self, event: ControlEvent) -> Option<ControlEvent> {
        match event {
            ControlEvent::LoadKeysSoundFont { soundfont } => {
                // Clone bumps the Arc refcount; the original travels back to
                // the GUI thread with the event, and whatever the sampler
                // displaced goes to the trash queue.
                let displaced = self.sampler.load_keys_soundfont(soundfont.clone());
                self.dispose_sf2(displaced);
                self.sampler.set_pitch_bend_range(self.global_params.pitch_bend_range as u8);
                return Some(ControlEvent::LoadKeysSoundFont { soundfont });
            }
            ControlEvent::LoadDrumsSoundFont { soundfont } => {
                let displaced = self.sampler.load_drums_soundfont(soundfont.clone());
                self.dispose_sf2(displaced);
                return Some(ControlEvent::LoadDrumsSoundFont { soundfont });
            }
            ControlEvent::UnloadKeysSoundFont => {
                let displaced = self.sampler.unload_keys();
                self.dispose_sf2(displaced);
            }
            ControlEvent::UnloadDrumsSoundFont => {
                let displaced = self.sampler.unload_drums();
                self.dispose_sf2(displaced);
            }
            ControlEvent::SetPartSf2Mode { part, enabled } => {
                // No-op switch (perf load pushes current state for every part):
                // draining here would cut SF2 tails on every perf change.
                // Drain only on a REAL mode flip.
                let mode_changed = self.parts.get(part)
                    .map(|l| l.sf2_mode != enabled)
                    .unwrap_or(false);
                if mode_changed {
                    if let Some(l) = self.parts.get_mut(part) {
                        // Engine switch: both voice owners must be drained.
                        // ОБЯЗАТЕЛЬНО: единственный writer sf2_mode — иначе
                        // mode_changed залипает в true и роутинг ломается.
                        l.drain_notes(&mut self.sampler, part, false);
                        l.sf2_mode = enabled;
                    }
                }
                // Allocate/free the channel FIRST: volume/pan/mute events of
                // the perf-load and reconnect push orders arrive BEFORE this
                // switch and early-return while the channel is still absent.
                self.sampler.set_part_mode(part, enabled);
                // (Re)apply per-channel gains: a freshly allocated channel
                // keeps the previous occupant's CC7/CC10 or GM defaults —
                // the "перф напутал громкость/панораму, лечится
                // перезагрузкой" report (2026-11-03).
                if enabled {
                    if let Some(l) = self.parts.get(part) {
                        let cc = (l.pan.clamp(-1.0, 1.0) * 0.5 + 0.5) * 127.0;
                        self.sampler.set_channel_pan(part, cc as i32);
                        self.sampler
                            .set_layer_volume(part, if l.mute { 0.0 } else { l.volume });
                    }
                }
            }
            ControlEvent::SetPartSf2Program { part, program, bank } => {
                // NO drain: rustysynth 0xC0 = channel.set_patch only — sounding
                // voices keep their captured region data (seamless tails);
                // new notes use the new program.
                self.sampler.set_part_program(part, program, bank);
            }
            ControlEvent::SetDrumsSf2Mode { enabled } => {
                self.sampler.set_drums_enabled(enabled);
                self.drum_engine.sf2_mode = enabled;
            }
            ControlEvent::SetSf2BlockSize { size } => {
                let (keys, drums) = self.sampler.set_block_size(size);
                self.dispose_sf2(keys);
                self.dispose_sf2(drums);
            }
            ControlEvent::SetSf2ReleaseScale { scale } => {
                self.sampler.set_release_time_scale(scale);
            }
            ControlEvent::SetDspTrim { db } => {
                self.dsp_trim_db = db.clamp(-24.0, 24.0);
            }
            ControlEvent::SetSf2Trim { db } => {
                self.sampler.set_trim_db(db);
            }
            ControlEvent::SetSf2SampleOffset { ms } => {
                self.sampler.set_sample_offset_ms(ms);
            }
            _ => {}
        }
        // Every variant that owns heap memory returns early above; reaching
        // here means the event was plain data and costs nothing to drop.
        None
    }

    fn hc_seq(&mut self, event: ControlEvent) -> Option<ControlEvent> {
        match event {
            ControlEvent::SeqSetEnabled { part, enabled } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pitch_seq.enabled = enabled;
                    if enabled { l.pitch_seq.reset(); }
                }
            }
            ControlEvent::SeqSetStep { part, step, pitch, gate, velocity } => {
                if let Some(l) = self.parts.get_mut(part) {
                    if (step as usize) < step_seq::MAX_STEPS {
                        l.pitch_seq.steps[step as usize] = step_seq::PitchStep { pitch, gate, velocity };
                    }
                }
            }
            ControlEvent::SeqSetLength { part, length } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pitch_seq.length = length.clamp(1, 16);
                }
            }
            ControlEvent::SeqSetRate { part, rate } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pitch_seq.rate = step_seq::StepRate::from_index(rate);
                }
            }
            ControlEvent::SeqSetScale { part, scale } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pitch_seq.scale = step_seq::ScaleType::from_index(scale);
                }
            }
            ControlEvent::SeqSetSwing { part, swing } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.pitch_seq.swing = swing;
                }
            }
            ControlEvent::MidiSeqLoad { data } => {
                // The old file's sounding notes will never see their note-offs
                // from the new event stream. Lift sustain everywhere FIRST:
                // all_notes_off alone clears only the sampler's pedal state —
                // the engine atom, part.sustain_pedal and sustained_notes
                // would stay ON and DSP would keep sustaining after key-offs
                // until the next CC64 edge (same ordering as the CC120/123 arm).
                self.set_sustain(0);
                self.sampler.all_notes_off();
                // The sequencer no longer owns any channel program
                self.sampler.seq_channels_release();
                // Full per-part drain: closes the root_held/root_active gap
                // (ghost rekey on gated (Bass/Root) patches after loading a new MIDI
                // file). The global all_notes_off above stays for drum/seq
                // channels not mapped to parts; the per-part CC123 is
                // redundant but idempotent.
                for (i, part) in self.parts.iter_mut().enumerate() {
                    part.drain_notes(&mut self.sampler, i, false);
                }
                self.seq_route = [0u8; 16];
                // The Box moves into the player; what it displaced comes back
                // out and goes to the GUI thread for freeing.
                let (old_data, old_tracks) = self.midi_player.load(data);
                if old_data.is_some() || !old_tracks.is_empty() {
                    match self.trash.as_mut() {
                        Some(t) => t.dispose(crate::trash::Trash::MidiSeq {
                            data: old_data,
                            tracks: old_tracks,
                        }),
                        None => drop((old_data, old_tracks)),
                    }
                }
            }
            ControlEvent::MidiSeqPlay { playing } => {
                self.midi_player.playing = playing;
                self.midi_player.play_atom.store(playing as u8, std::sync::atomic::Ordering::Relaxed);
                if !playing {
                    self.midi_player.reset_position();
                    // Lift sustain everywhere first (see MidiSeqLoad): the
                    // engine atom and part flags must agree with the sampler.
                    self.set_sustain(0);
                    // All notes off on all engines
                    self.sampler.all_notes_off();
                    // Playback stopped: give the shared channels back to the parts
                    self.sampler.seq_channels_release();
                    // Full per-part drain: closes the root_held/root_active gap
                    // (ghost rekey on gated (Bass/Root) patches after MIDI-stop).
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        part.drain_notes(&mut self.sampler, i, false);
                    }
                }
            }
            ControlEvent::MidiSeqSetBpm { bpm } => {
                self.midi_player.bpm_override = bpm;
            }
            ControlEvent::MidiSeqSetLooping { looping } => {
                self.midi_player.looping = looping;
            }
            ControlEvent::MidiSeqSetTrackInstrument { track_idx, instrument } => {
                if let Some(track) = self.midi_player.tracks.get_mut(track_idx) {
                    track.instrument = instrument;
                    // Pre-set the SF2 program for this channel so notes play correctly
                    if let midi_player::TrackInstrument::Sf2 { program } = instrument {
                        self.sampler.seq_program_set(track.channel, program);
                    }
                }
            }
            ControlEvent::MidiSeqSetTrackMute { track_idx, muted } => {
                if let Some(track) = self.midi_player.tracks.get_mut(track_idx) {
                    track.muted = muted;
                }
            }
            ControlEvent::SetArpParams { enabled, mode, rate, octaves, gate } => {
                // If disabling, send note-off for current arp note
                if !enabled {
                    if let Some(off_note) = self.arpeggiator.flush_current_note() {
                        // Same transposed routing as flush_arp_note_off.
                        self.dispatch_arp_note_off(off_note);
                    }
                }
                self.arpeggiator.set_params(enabled, mode, rate, octaves, gate);
            }
            ControlEvent::SetArpLatch { latch } => {
                if !latch {
                    // Stop the frozen set first so the sounding arp voice
                    // gets a matching note-off before the set is cleared.
                    if let Some(off_note) = self.arpeggiator.flush_current_note() {
                        // Same transposed routing as flush_arp_note_off.
                        self.dispatch_arp_note_off(off_note);
                    }
                }
                self.arpeggiator.set_latch(latch);
            }
            _ => {}
        }
        // Every variant that owns heap memory returns early above; reaching
        // here means the event was plain data and costs nothing to drop.
        None
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for part in &mut self.parts { part.set_sample_rate(sample_rate); }
        self.drum_engine.set_sample_rate(sample_rate);
        // Called from the audio callback when JACK reports a rate change, so
        // the rebuilt soundfonts must not be freed here.
        let (keys, drums) = self.sampler.set_sample_rate(sample_rate);
        self.dispose_sf2(keys);
        self.dispose_sf2(drums);
        self.eq.set_sample_rate(sample_rate);
        self.compressor.set_sample_rate(sample_rate);
        self.overdrive.set_sample_rate(sample_rate);
        self.tape.set_sample_rate(sample_rate);
        self.neuron.set_sample_rate(sample_rate);
        self.phaser.set_sample_rate(sample_rate);
        self.flanger.set_sample_rate(sample_rate);
        self.tremolo.set_sample_rate(sample_rate);
        self.bitcrusher.set_sample_rate(sample_rate);
        self.ring_mod.set_sample_rate(sample_rate);
        self.freq_shift.set_sample_rate(sample_rate);
        self.chorus.set_sample_rate(sample_rate);
        self.rotary.set_sample_rate(sample_rate);
        self.bbd_ensemble.set_sample_rate(sample_rate);
        self.resonator.set_sample_rate(sample_rate);
        self.bonsai.set_sample_rate(sample_rate);
        self.delay.set_sample_rate(sample_rate);
        self.reverb.set_sample_rate(sample_rate);
        self.spring_reverb.set_sample_rate(sample_rate);
        self.wave_shaper.set_sample_rate(sample_rate);
        self.ms_tool.set_sample_rate(sample_rate);
        self.graphic_eq_fx.set_sample_rate(sample_rate);
        self.conditioner.set_sample_rate(sample_rate);
        self.exciter.set_sample_rate(sample_rate);
        self.floaty_delay.set_sample_rate(sample_rate);
        self.reverb2.set_sample_rate(sample_rate);
        self.combulator.set_sample_rate(sample_rate);
        self.treemonster.set_sample_rate(sample_rate);
        self.nimbus.set_sample_rate(sample_rate);
        self.vocoder.set_sample_rate(sample_rate);
        self.conv_reverb.set_sample_rate(sample_rate);
        self.airwindows.set_sample_rate(sample_rate);
    }

    /// Process a block of audio samples. Control-rate computations (LFO, MSEG,
    /// mod matrix, effect params) happen once per block for efficiency.
    pub fn tick_block(&mut self, buf_l: &mut [f32], buf_r: &mut [f32]) {
        let block_len = buf_l.len();

        // --- Control rate (once per block) ---
        let at_alpha = 1.0 - (-(block_len as f32) / (0.010 * self.sample_rate)).exp();
        self.aftertouch_smooth += at_alpha * (self.aftertouch - self.aftertouch_smooth);

        // Vibrato for mod wheel
        let vibrato_val = if self.mod_wheel > 0.001 {
            self.vibrato_phase += 6.0 / self.sample_rate * block_len as f32;
            if self.vibrato_phase >= 1.0 { self.vibrato_phase -= 1.0; }
            (self.vibrato_phase * std::f32::consts::TAU).sin()
        } else {
            self.vibrato_phase = 0.0;
            0.0
        };

        let seq_bpm = self.drum_engine.sequencer.bpm;
        self.arpeggiator.set_bpm(seq_bpm);
        // Looper follows the same canonical clock (REFACTOR_AUDIT S5): bpm +
        // current pattern's beats_per_bar, synced every block like arp/LFO.
        let cur_pat = self.drum_engine.sequencer.current_pattern as usize;
        let beats_per_bar = self.drum_engine.sequencer.patterns[cur_pat].beats_per_bar;
        self.looper.sync_clock(seq_bpm, beats_per_bar);

        // Quantized master recording: pattern boundary = sequencer wrapped to
        // step 0 (edge-detected). Arm -> start capture (петля = 1 цикл
        // паттерна — размеры лупера и секвенсора совпадают); каждая граница
        // считает целый цикл, второй Rec закрывает на СЛЕДУЮЩЕЙ границе —
        // петля = РОВНО N целых циклов.
        let step = self.drum_engine.sequencer.current_step;
        let prev = self.looper_prev_step;
        self.looper_prev_step = step;
        let playing = self.drum_engine.sequencer.playing;
        let boundary = playing && step == 0 && prev != 0;
        let quantized_looping = self.looper_capture_quantized
            && matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Recording
                    | crate::synth::looper::LooperState::Overdubbing
                    | crate::synth::looper::LooperState::Playing
            );
        // #32 Phase 5: pattern switch at a boundary (quantized GUI queue or
        // immediate console switch) resizes the looper cycle to the new
        // pattern length — while capturing, or while a 1-cycle-of-old-pattern
        // loop plays. Free (non-quantized) loops keep their length. Drift
        // correction below keeps its own guard and needs no change here.
        let cur_pat = self.drum_engine.sequencer.current_pattern;
        let pat_switched = cur_pat != self.looper_prev_pattern;
        if pat_switched { self.looper_prev_pattern = cur_pat; }
        if boundary && pat_switched {
            let one_cycle_of_old = self.looper_cycle_ticks > 0
                && self.looper.loop_len_ticks() == self.looper_cycle_ticks;
            if quantized_looping || one_cycle_of_old {
                let pattern_len =
                    self.drum_engine.sequencer.patterns[cur_pat as usize].length.max(1);
                let bpm = self.drum_engine.sequencer.bpm.max(1.0);
                let samples_per_step =
                    (self.sample_rate as f64 * 60.0) / (bpm as f64 * 4.0);
                let new_cycle = (pattern_len as f64 * samples_per_step) as u32;
                self.looper_cycle_ticks = new_cycle;
                self.keep_capture_capacity();
                if self.looper_rec_cycles > 0 {
                    self.looper.set_loop_len_ticks(
                        self.looper_rec_cycles.saturating_mul(new_cycle));
                } else {
                    self.looper.set_loop_len_ticks(new_cycle);
                }
            }
        }
        if self.looper_arm_pending
            || self.looper_finish_pending
            || quantized_looping
        {
            let lstate = self.looper.state;
            // Квантованный захват может УЖЕ самообернуться в Overdubbing
            // (looper.tick() переключает Recording->Overdubbing по своей длине
            // петли на несколько сэмплов РАНЬШЕ, чем детект границы такта) —
            // закрывать надо и его.
            let capturing = lstate == crate::synth::looper::LooperState::Recording
                || (lstate == crate::synth::looper::LooperState::Overdubbing
                    && self.looper_capture_quantized
                    && !self.looper_overdub_active);
            if boundary && self.looper_overdub_pending && lstate == crate::synth::looper::LooperState::Playing {
                // Слой: старт поверх петли на границе (длина петли не меняется)
                self.looper_overdub_pending = false;
                self.looper_overdub_active = true;
                self.looper.toggle_record(); // Playing -> Overdubbing
            } else if boundary
                && lstate == crate::synth::looper::LooperState::Overdubbing
                && self.looper_overdub_active
            {
                if self.looper_finish_pending {
                    self.looper_finish_pending = false;
                    self.looper_overdub_active = false;
                    // Sustain capture exit #5: pending offs belong in the
                    // layer being closed, not the next one.
                    self.flush_looper_sustain_pending();
                    self.looper.toggle_record(); // Overdubbing -> Playing
                    debug_assert_eq!(
                        self.looper_sustain_pending_len, 0,
                        "layer finish: pending must be flushed"
                    );
                }
            } else if boundary && self.looper_arm_pending {
                self.looper_arm_pending = false;
                self.looper.toggle_record(); // Idle -> Recording (length = bars * bar)
                // Boss RC-семантика: петля = Bars × цикл паттерна; авто-закрытие
                // после Bars циклов; тики записи НЕПРЕРЫВНЫ (wrap только на
                // конце всей петли) — релокация не нужна.
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                let pattern_len = self.drum_engine.sequencer.patterns[pat].length.max(1);
                let bpm = self.drum_engine.sequencer.bpm.max(1.0);
                let samples_per_step = (self.sample_rate as f64 * 60.0) / (bpm as f64 * 4.0);
                let cycle = (pattern_len as f64 * samples_per_step) as u32;
                // Безостановочный захват: лупер с запасом (64 цикла максимум) —
                // playback_tick НЕ оборачивается, тики событий глобальные.
                // Реальная длина = looper_rec_cycles циклов; Rec закрывает.
                self.looper.set_loop_len_ticks(cycle.saturating_mul(64));
                // GUI-позиция = прогресс текущего цикла (см. set_display_cycle_ticks)
                self.looper.set_display_cycle_ticks(Some(cycle));
                self.looper_cycle_ticks = cycle;
                self.looper_rec_cycles = 1;
                self.set_master_rec_state(2, 1); // capturing, cycle 1
            } else if boundary && capturing {
                // Закрытие — ТОЛЬКО по Rec (finish_pending). Безостановочный режим.
                if self.looper_finish_pending {
                    self.looper_finish_pending = false;
                    // Sustain capture exit #2: flush BEFORE trim, so pending
                    // offs are trimmed on the same rules as everything else.
                    self.flush_looper_sustain_pending();
                    let kept = self.looper_rec_cycles.saturating_mul(self.looper_cycle_ticks);
                    self.looper.trim_after(kept);
                    self.looper.set_loop_len_ticks(kept);
                    self.looper.set_display_cycle_ticks(None);
                    self.looper_rec_cycles = 0;
                    // REC/CLOSING-атом надо гасить ЗДЕСЬ: единственные сбросы
                    // были в master_stop/disarm — GUI висел в «REC n тактов»/
                    // «CLOSING» с мёртвыми +1/−1 (audit rhythm #1).
                    self.set_master_rec_state(0, 0);
                    self.drum_engine.set_recording(false); // сетка тоже разармирована
                    self.looper.toggle_record(); // Recording/Overdubbing -> Playing
                    debug_assert_eq!(
                        self.looper_sustain_pending_len, 0,
                        "quantized close: pending must be flushed"
                    );
                } else {
                    // Новый цикл начался: логическая длина +1, ёмкость с запасом
                    self.looper_rec_cycles += 1;
                    self.set_master_rec_state(2,
                        self.looper_rec_cycles.min(255) as u8);
                    self.keep_capture_capacity();
                }
            } else if boundary
                && lstate == crate::synth::looper::LooperState::Playing
                && self.looper.loop_len_ticks() == self.looper_cycle_ticks
            {
                // Drift correction (Mobius-style): только для петли = 1 цикл —
                // каждый круг выравнивает петлю в ноль. Для N>1-цикловых петель
                // seek убивал бы вторую половину (длина != cycle) — не трогаем.
                self.looper.seek_start();
            } else if !playing && capturing {
                // fallback: часы умерли посреди записи — закрыть сейчас
                // (нечётный кусок; длину петли не перекраиваем)
                self.looper_finish_pending = false;
                self.looper.set_display_cycle_ticks(None);
                self.looper_rec_cycles = 0;
                // REC/CLOSING-атом надо гасить ЗДЕСЬ: единственные сбросы
                // были в master_stop/disarm — GUI висел в «REC n тактов»/
                // «CLOSING» с мёртвыми +1/−1 (audit rhythm #1).
                self.set_master_rec_state(0, 0);
                self.drum_engine.set_recording(false);
                // Sustain capture exit #3: clock died mid-capture — pending
                // offs still belong in the loop (flush is a no-op when empty).
                self.flush_looper_sustain_pending();
                self.looper.toggle_record();
                debug_assert_eq!(
                    self.looper_sustain_pending_len, 0,
                    "fallback close: pending must be flushed"
                );
            }
        }

        // Pre-compute modulation state for each part (control rate). Engine-level
        // входы собраны один раз ДО цикла (S4-synth 2); слоты выключенных частей
        // не трогаются (never read while disabled).
        let inputs = EngineModInputs {
            mod_wheel: self.mod_wheel,
            aftertouch_smooth: self.aftertouch_smooth,
            cc_values: self.cc_values,
            pitch_bend_semitones: self.pitch_bend_semitones,
            pitch_bend_norm: self.pitch_bend_norm,
            seq_bpm,
            block_len,
            sample_rate: self.sample_rate,
            vibrato_val,
        };
        // Stack copy (ModulationState is Copy, 8 x 16 B): итерация
        // self.parts.iter_mut() без конфликта заимствований.
        // Поле layer_mods удалено (S4-synth 2, ревью: других читателей нет);
        // выключенные части не читаются, поэтому дефолты вместо «заморозки»
        // наблюдаемо не отличаются.
        let mut layer_mods_snapshot = [ModulationState {
            pitch_mult: 1.0, filter_offset: 0.0, filter_offset_semis: 0.0, amp_mod: 1.0,
        }; MAX_PARTS];
        for (li, part) in self.parts.iter_mut().enumerate() {
            if !part.enabled { continue; } // выключенные части: слот не обновляется (значение по умолчанию — не читается, part.tick скипается)
            layer_mods_snapshot[li] = part.update_mods(&inputs, li);
        }

        // Cache effect params (once per block)
        let ep = self.parts.iter().find(|l| l.enabled).map(|l| &l.params);
        let fx = CachedFxParams::from_preset(ep, &self.global_params);
        let fx_chain = ep.map(|p| p.fx_chain).unwrap_or_default();

        // DSP-bus trim (Settings → Audio): applies to the summed part outputs
        // and the internal drum engine — the same bus the looper replays
        // through — but NOT to the SF2 sampler (it carries its own SF2_CAL_GAIN
        // + sf2_trim_db). dB → linear, once per block.
        let dsp_gain = 10.0_f32.powf(self.dsp_trim_db / 20.0);

        // Легаси-FX инстансы шарятся мастером и 16-слотовой цепочкой: слот
        // ставит mix=1.0, но никто не сбрасывал — после первого использования
        // Phaser/Flanger/Overdrive/Tremolo/Bitcrusher тот тикал на мастере
        // вечно (аудит DSP 2026-11-01, находка 1). Полное разделение — S3.
        {
            use fx_chain::FxSlotType;
            let (mut od, mut ph, mut fl, mut tr, mut bc) = (false, false, false, false, false);
            if let Some(p) = ep {
                for slot in &p.fx_chain.slots {
                    // Гейт как в аплайере (mod.rs:3568): выключенный или
                    // нулевой слот НЕ держит легаси-инстанс включённым.
                    if !slot.enabled || slot.mix <= 0.001 { continue; }
                    match slot.slot_type {
                        FxSlotType::Overdrive => od = true,
                        FxSlotType::Phaser => ph = true,
                        FxSlotType::Flanger => fl = true,
                        FxSlotType::Tremolo => tr = true,
                        FxSlotType::Bitcrusher => bc = true,
                        _ => {}
                    }
                }
            }
            if !od { self.overdrive.mix = 0.0; }
            if !ph { self.phaser.mix = 0.0; }
            if !fl { self.flanger.mix = 0.0; }
            if !tr { self.tremolo.mix = 0.0; }
            if !bc { self.bitcrusher.mix = 0.0; }
        }

        // --- MIDI file sequencer: fire block-level events ---
        // (process_block clears midi_seq_buf itself before filling it)
        {
            self.midi_player.process_block(block_len, self.sample_rate, &mut self.midi_seq_buf);
            for i in 0..self.midi_seq_buf.len() {
                let ch = self.midi_seq_buf[i].0;
                let evt = self.midi_seq_buf[i].1.clone();
                let instr = self.midi_player.instrument_for_channel(ch);
                match evt {
                    midi_player::SeqEventData::NoteOn { note, velocity } => match instr {
                        midi_player::TrackInstrument::Drums => {
                            self.seq_route[ch as usize] = 2;
                            let sf2 = self.drum_engine.sf2_mode;
                            if sf2 {
                                let choke = self.drum_engine.sf2_choke_take(note);
                                if choke != 0 { self.sampler.drum_note_off(choke); }
                                self.sampler.drum_note_on(note, velocity);
                            } else {
                                self.drum_engine.note_on(note, velocity);
                            }
                        }
                        midi_player::TrackInstrument::DspLayer0 => {
                            self.seq_route[ch as usize] = 0;
                            if self.parts.first().map(|p| p.note_mode != NOTE_MODE_POLY).unwrap_or(false) {
                                // Note-mode gate (range/velocity checked inside)
                                if !self.parts.is_empty() { self.route_note_on(0, note, velocity); }
                            } else if let Some(part) = self.parts.get_mut(0) {
                                part.note_on(note, velocity);
                            }
                        }
                        midi_player::TrackInstrument::Sf2 { program } => {
                            self.seq_route[ch as usize] = 1;
                            self.sampler.seq_program_set(ch, program);
                            self.sampler.seq_note_on(ch, note, velocity);
                        }
                    },
                    midi_player::SeqEventData::NoteOff { note } => match self.seq_route[ch as usize] {
                        0 => {
                            if self.parts.first().map(|p| p.note_mode != NOTE_MODE_POLY).unwrap_or(false) {
                                if !self.parts.is_empty() { self.route_note_off(0, note); }
                            } else if let Some(part) = self.parts.get_mut(0) {
                                part.note_off(note);
                            }
                        }
                        1 => self.sampler.seq_note_off(ch, note),
                        _ => {
                            if let midi_player::TrackInstrument::Drums = instr {
                                let sf2 = self.drum_engine.sf2_mode;
                                if sf2 { self.sampler.drum_note_off(note); }
                            }
                        },
                    },
                    midi_player::SeqEventData::ProgramChange { program } => {
                        // Follow file program changes when in SF2 mode
                        if let midi_player::TrackInstrument::Sf2 { .. } = instr {
                            self.sampler.seq_program_set(ch, program);
                        }
                    }
                    _ => {}
                }
            }
        }

        // --- Audio rate (per sample) ---
        for s in 0..block_len {
            // Arpeggiator: generate note events from held notes
            {
                let (arp_on, arp_off) = self.arpeggiator.tick();
                if let Some(off_note) = arp_off {
                    for i in 0..self.parts.len() {
                        if !self.parts[i].enabled { continue; }
                        self.route_note_off(i, off_note);
                        // Source-consistent recording: per part, like the
                        // note dispatch loop (used to be one active-part
                        // event — the loop replayed arps on one part only).
                        // defer_ok=false: ARP-offs are rhythmic — never deferred.
                        self.record_for_looper(off_note, 0, i as u8, false);
                    }
                }
                if let Some((note, vel)) = arp_on {
                    for i in 0..self.parts.len() {
                        if !self.parts[i].enabled || self.parts[i].mute { continue; }
                        // Мультитрек: писать только реально звучащую часть
                        if self.route_note_on(i, note, vel) {
                            self.record_for_looper(note, vel, i as u8, false);
                        }
                    }
                }
            }

            // Looper: replay recorded events (routed to specific part).
            // Драм-события (бывший тег 254) больше не пишутся в лупер —
            // ударку реплеит сетка секвенсора.
            let looper_events = self.looper.tick();
            for &(note, vel, part_id) in &looper_events {
                if note == 0 && vel == 0 && part_id == 255 { break; }
                let pi = part_id as usize;
                if self.parts.get(pi).map(|p| !p.enabled).unwrap_or(true) { continue; }
                if self.parts[pi].note_mode != NOTE_MODE_POLY {
                    // Note-mode gate (range/velocity checked inside)
                    if vel > 0 { self.route_note_on(pi, note, vel); }
                    else { self.route_note_off(pi, note); }
                } else if let Some(part) = self.parts.get_mut(pi) {
                    let transposed = (note as i16 + part.transpose as i16).clamp(0, 127) as u8;
                    if vel > 0 {
                        if part.sf2_mode {
                            self.sampler.note_on(pi, transposed, vel);
                        } else {
                            part.note_on(note, vel); // raw: part transposes internally
                        }
                    } else {
                        if part.sf2_mode {
                            self.sampler.note_off(pi, transposed);
                        } else {
                            part.note_off(note); // raw: part matches by src_note
                        }
                    }
                }
            }

            let (mut out_l, mut out_r) = (0.0_f32, 0.0_f32);

            // Voice ticks (audio rate, using cached modulation)
            let mute_fade_step = 1.0 / (0.005 * self.sample_rate);
            for (li, part) in self.parts.iter_mut().enumerate() {
                if !part.enabled { continue; }
                let (l, r) = part.tick(&layer_mods_snapshot[li]);
                // Mute is a mix gate: a muted part contributes zero within ~5 ms
                // (linear fade both ways, so the gate itself does not click).
                // Voices keep ticking so their release envelopes reach idle and
                // the voice slot is freed.
                part.mute_gain = if part.mute {
                    (part.mute_gain - mute_fade_step).max(0.0)
                } else {
                    (part.mute_gain + mute_fade_step).min(1.0)
                };
                let fade = part.mute_gain;
                // Apply per-part pan (constant-power: sqrt of (0.5 ± pan*0.5))
                let pan = part.pan.clamp(-1.0, 1.0);
                let gain_l = ((0.5 - pan * 0.5) as f64).sqrt() as f32;
                let gain_r = ((0.5 + pan * 0.5) as f64).sqrt() as f32;
                out_l += l * gain_l * fade;
                out_r += r * gain_r * fade;
            }
            out_l *= dsp_gain;
            out_r *= dsp_gain;

            // SF2 + drums
            let (sf2_l, sf2_r) = self.sampler.tick();
            out_l += sf2_l; out_r += sf2_r;
            let ((drum_l, drum_r), drum_triggers) = self.drum_engine.tick();
            out_l += drum_l * dsp_gain; out_r += drum_r * dsp_gain;
            for i in 0..drum_triggers.count {
                let (slot, vel) = drum_triggers.triggers[i];
                // Ремап кита применяется и на playback сетки (#22b-семейство):
                // без него кубик играл сырую ноту слота — «подписано hi-hat,
                // а звучит том» (SF2-киты с ремапом на сэмплы вне 36..=51).
                let note = self.drum_engine.remapped_note(drum::DRUM_NOTE_BASE + slot);
                if self.drum_engine.sf2_mode {
                    let choke = self.drum_engine.sf2_choke_take(note);
                    if choke != 0 { self.sampler.drum_note_off(choke); }
                }
                // L/T действуют и на SF2-киты: velocity-scale + транспозиция
                // (раунд 4 фидбека). P/D ограничением rustysynth — DSP-only.
                let p = &self.drum_engine.params[slot as usize];
                if p.level <= 0.0 { continue; }
                let vel = ((vel as f32 * p.level).round() as u8).clamp(1, 127);
                let note = (note as i32 + p.tune.round() as i32).clamp(0, 127) as u8;
                self.sampler.drum_note_on(note, vel);
            }

            // Effects chain with cached params — skip when mix=0 to save CPU
            let (out_l, out_r) = self.eq.tick(out_l, out_r);
            let (out_l, out_r) = self.compressor.tick(out_l, out_r);
            let (out_l, out_r) = self.overdrive.tick(out_l, out_r);
            let (out_l, out_r) = if fx.tape_mix > 0.001 {
                self.tape.tick(out_l, out_r, fx.tape_drive, fx.tape_saturation, fx.tape_bias, fx.tape_tone, fx.tape_speed, fx.tape_mix)
            } else { (out_l, out_r) };
            let (out_l, out_r) = if fx.neuron_mix > 0.001 {
                self.neuron.tick(out_l, out_r, fx.neuron_drive, fx.neuron_squash, fx.neuron_stab, fx.neuron_asym, fx.neuron_bias, fx.neuron_comb_freq, fx.neuron_comb_sep, fx.neuron_mix)
            } else { (out_l, out_r) };
            let (out_l, out_r) = self.phaser.tick(out_l, out_r);
            let (out_l, out_r) = self.flanger.tick(out_l, out_r);
            let (out_l, out_r) = self.tremolo.tick(out_l, out_r);
            let (out_l, out_r) = self.bitcrusher.tick(out_l, out_r);
            let (out_l, out_r) = if fx.ring_mod_mix > 0.001 {
                self.ring_mod.tick(out_l, out_r, fx.ring_mod_freq, fx.ring_mod_shape, fx.ring_mod_bias, fx.ring_mod_linear, fx.ring_mod_mix)
            } else { (out_l, out_r) };
            let (out_l, out_r) = if fx.freq_shift_mix > 0.001 {
                self.freq_shift.tick(out_l, out_r, fx.freq_shift_hz, fx.freq_shift_feedback, fx.freq_shift_delay, fx.freq_shift_mix)
            } else { (out_l, out_r) };
            // Bonsai saturation (after neuron, before chorus)
            let (out_l, out_r) = if fx.bonsai_mix > 0.001 {
                let mode = bonsai::BonsaiMode::from_param(fx.bonsai_mode);
                self.bonsai.tick(out_l, out_r, fx.bonsai_drive, fx.bonsai_tone, fx.bonsai_asym, mode, fx.bonsai_mix)
            } else { (out_l, out_r) };
            // Resonator (pitched comb bank)
            let (out_l, out_r) = if fx.resonator_mix > 0.001 {
                // Set 4 voices to harmonic series of resonator_freq
                let rf = fx.resonator_freq;
                let rd = fx.resonator_decay;
                self.resonator.set_freq(0, rf);
                self.resonator.set_freq(1, rf * 1.5);
                self.resonator.set_freq(2, rf * 2.0);
                self.resonator.set_freq(3, rf * 3.0);
                for i in 0..4 { self.resonator.set_decay(i, rd); }
                let mono = (out_l + out_r) * 0.5;
                self.resonator.tick(mono, fx.resonator_mix)
            } else { (out_l, out_r) };
            let mono = (out_l + out_r) * 0.5;
            let (cl, cr) = if fx.chorus_mix > 0.001 {
                self.chorus.tick(mono, fx.chorus_mix)
            } else { (mono, mono) };
            // BBD Ensemble (separate from Juno chorus)
            let (cl, cr) = if fx.ensemble_mix > 0.001 {
                let mono2 = (cl + cr) * 0.5;
                self.bbd_ensemble.tick(mono2, fx.ensemble_depth, fx.ensemble_rate, fx.ensemble_mix)
            } else { (cl, cr) };
            let diff = (out_l - out_r) * 0.5;
            let (dl, dr) = if fx.delay_mix > 0.001 {
                self.delay.tick(cl + diff, cr - diff, fx.delay_time_l, fx.delay_time_r, fx.delay_feedback, fx.delay_filter, fx.delay_ping_pong > 0.5, fx.delay_mix)
            } else { (cl + diff, cr - diff) };

            let (rl, rr) = if fx.reverb_type > 0.5 {
                let mix = if fx.spring_mix > 0.001 { fx.spring_mix } else { fx.reverb_mix };
                if mix > 0.001 {
                    self.spring_reverb.tick(dl, dr, fx.spring_size, fx.spring_decay, fx.spring_reflections, fx.spring_damping, fx.spring_spin, fx.spring_chaos, mix)
                } else { (dl, dr) }
            } else if fx.reverb_mix > 0.001 {
                self.reverb.tick(dl, dr, fx.reverb_room_size, fx.reverb_damping, fx.reverb_width, fx.reverb_pre_delay, fx.reverb_mix)
            } else { (dl, dr) };
            // Rotary Speaker (after reverb, before master tone)
            let (rl, rr) = if fx.rotary_mix > 0.001 {
                self.rotary.tick(rl, rr, fx.rotary_speed, fx.rotary_mix)
            } else { (rl, rr) };
            // WaveShaper
            let (rl, rr) = if fx.wave_shaper_mix > 0.001 {
                self.wave_shaper.tick(rl, rr, fx.wave_shaper_drive, fx.wave_shaper_mode as u32, fx.wave_shaper_bias, fx.wave_shaper_mix)
            } else { (rl, rr) };
            // MS Tool
            let (rl, rr) = if fx.ms_mix > 0.001 {
                self.ms_tool.tick(rl, rr, fx.ms_mid_gain, fx.ms_side_gain, fx.ms_rotation, fx.ms_mix)
            } else { (rl, rr) };
            // Graphic EQ (always runs if output_gain != 1.0 or any band != 0.0)
            let any_geq = fx.graphic_eq_gains.iter().any(|&g| g.abs() > 0.001) || (fx.graphic_eq_output - 1.0).abs() > 0.001;
            let (rl, rr) = if any_geq {
                self.graphic_eq_fx.tick(rl, rr, &fx.graphic_eq_gains, fx.graphic_eq_output)
            } else { (rl, rr) };
            // Conditioner
            let (rl, rr) = if fx.conditioner_mix > 0.001 {
                self.conditioner.tick(rl, rr, fx.conditioner_bass_cut, fx.conditioner_width, fx.conditioner_threshold, fx.conditioner_mix)
            } else { (rl, rr) };
            // Exciter
            let (rl, rr) = if fx.exciter_mix > 0.001 {
                self.exciter.tick(rl, rr, fx.exciter_drive, fx.exciter_freq, fx.exciter_presence, fx.exciter_mix)
            } else { (rl, rr) };
            // Floaty Delay
            let (rl, rr) = if fx.floaty_mix > 0.001 {
                self.floaty_delay.tick(rl, rr, fx.floaty_time, fx.floaty_feedback, fx.floaty_wobble, fx.floaty_rate, fx.floaty_damp, fx.floaty_mix)
            } else { (rl, rr) };
            // Reverb2 (FDN)
            let (rl, rr) = if fx.reverb2_mix > 0.001 {
                self.reverb2.tick(rl, rr, fx.reverb2_decay, fx.reverb2_damping, fx.reverb2_size, fx.reverb2_mix)
            } else { (rl, rr) };
            // Combulator
            let (rl, rr) = if fx.combulator_mix > 0.001 {
                self.combulator.tick(rl, rr, fx.combulator_freq, fx.combulator_offset2, fx.combulator_offset3, fx.combulator_feedback, fx.combulator_tone, fx.combulator_mix)
            } else { (rl, rr) };
            // Treemonster
            let (rl, rr) = if fx.treemonster_mix > 0.001 {
                self.treemonster.tick(rl, rr, fx.treemonster_threshold, fx.treemonster_shift, fx.treemonster_ring_mix, fx.treemonster_mix)
            } else { (rl, rr) };
            // Nimbus granular
            let (rl, rr) = if fx.nimbus_mix > 0.001 {
                self.nimbus.tick(rl, rr, fx.nimbus_position, fx.nimbus_size, fx.nimbus_pitch, fx.nimbus_density, fx.nimbus_spread, fx.nimbus_texture, fx.nimbus_mix)
            } else { (rl, rr) };
            // Vocoder
            let (rl, rr) = if fx.vocoder_mix > 0.001 {
                let mod_in = (rl + rr) * 0.5;
                self.vocoder.tick(rl, rr, mod_in, fx.vocoder_env_follow, fx.vocoder_gate, fx.vocoder_mix)
            } else { (rl, rr) };
            // Convolution Reverb
            let (rl, rr) = if fx.conv_reverb_mix > 0.001 {
                self.conv_reverb.tick(rl, rr, fx.conv_reverb_room, fx.conv_reverb_damping, fx.conv_reverb_predelay, fx.conv_reverb_mix)
            } else { (rl, rr) };
            // Airwindows
            let (rl, rr) = if fx.airwindows_mix > 0.001 {
                self.airwindows.tick(rl, rr, fx.airwindows_mode, fx.airwindows_drive, fx.airwindows_mix)
            } else { (rl, rr) };

            // ── New 16-slot FX chain (runs after legacy chain when active) ──
            let (rl, rr) = if fx_chain.active {
                let mut sl = rl;
                let mut sr = rr;
                for slot in &fx_chain.slots {
                    if slot.enabled && slot.slot_type != fx_chain::FxSlotType::None && slot.mix > 0.001 {
                        let slot_copy = *slot;
                        (sl, sr) = self.apply_fx_slot(&slot_copy, sl, sr);
                    }
                }
                (sl, sr)
            } else {
                (rl, rr)
            };

            let sample_rate = self.sample_rate;
            let (tl, tr) = self.global_params.apply_tone(rl, rr, sample_rate);
            let vol = self.global_params.master_volume;
            buf_l[s] = tl * vol;
            buf_r[s] = tr * vol;
            // Write mono mix to oscilloscope buffer (freeze-гейт при тишине:
            // write-индекс замирает — GUI не перерисовывает осциллограф)
            let sample = (buf_l[s] + buf_r[s]) * 0.5;
            if scope_gate(&mut self.scope_frozen, sample) {
                self.scope_buf.push(sample, &mut self.scope_write);
            }
        }
    }

    /// Process one sample through a single FX chain slot. Returns (out_l, out_r).
    /// `mix` is the slot's dry/wet (0..1). Slot's own mix is already checked by caller.
    #[inline]
    fn apply_fx_slot(&mut self, slot: &fx_chain::FxSlot, in_l: f32, in_r: f32) -> (f32, f32) {
        use fx_chain::FxSlotType;
        let p = slot.params;
        let mix = slot.mix;
        let dry_l = in_l;
        let dry_r = in_r;
        let (wet_l, wet_r) = match slot.slot_type {
            FxSlotType::None => return (in_l, in_r),
            FxSlotType::Overdrive => {
                self.overdrive.drive = p[0] * 48.0 + 1.0;
                self.overdrive.tone  = p[1];
                self.overdrive.dist_type = overdrive::DistortionType::from_param(p[2]);
                self.overdrive.mix   = 1.0;
                self.overdrive.tick(in_l, in_r)
            }
            FxSlotType::Tape => {
                self.tape.tick(in_l, in_r, p[0]*3.0, p[1], p[2], p[3], 0.5, 1.0)
            }
            FxSlotType::Neuron => {
                self.neuron.tick(in_l, in_r, p[0], p[1], p[2], 0.0, 0.5, p[3].max(10.0), 0.5, 1.0)
            }
            FxSlotType::Bonsai => {
                let mode = bonsai::BonsaiMode::from_param(p[3]);
                self.bonsai.tick(in_l, in_r, p[0], p[1], p[2], mode, 1.0)
            }
            FxSlotType::WaveShaper => {
                self.wave_shaper.tick(in_l, in_r, p[0]*4.0+0.5, p[1] as u32, p[2]*2.0-1.0, 1.0)
            }
            FxSlotType::Airwindows => {
                self.airwindows.tick(in_l, in_r, p[0] as u32, p[1], 1.0)
            }
            FxSlotType::Chorus => {
                let mono = (in_l + in_r) * 0.5;
                self.chorus.tick(mono, 1.0)
            }
            FxSlotType::BbdEnsemble => {
                let mono = (in_l + in_r) * 0.5;
                self.bbd_ensemble.tick(mono, p[0], p[1], 1.0)
            }
            FxSlotType::Flanger => {
                self.flanger.mix = 1.0;
                self.flanger.tick(in_l, in_r)
            }
            FxSlotType::Phaser => {
                self.phaser.mix = 1.0;
                self.phaser.tick(in_l, in_r)
            }
            FxSlotType::Tremolo => {
                self.tremolo.mix = 1.0;
                self.tremolo.tick(in_l, in_r)
            }
            FxSlotType::Rotary => {
                self.rotary.tick(in_l, in_r, p[0], 1.0)
            }
            FxSlotType::Bitcrusher => {
                self.bitcrusher.mix = 1.0;
                self.bitcrusher.tick(in_l, in_r)
            }
            FxSlotType::Delay => {
                self.delay.tick(in_l, in_r, p[0], p[1], p[2], p[3], false, 1.0)
            }
            FxSlotType::FloatyDelay => {
                self.floaty_delay.tick(in_l, in_r, p[0], p[1], p[2], p[3]*5.0, 0.3, 1.0)
            }
            FxSlotType::Reverb => {
                self.reverb.tick(in_l, in_r, p[0], p[1], p[2], p[3], 1.0)
            }
            FxSlotType::Reverb2 => {
                self.reverb2.tick(in_l, in_r, p[0], p[1], p[2], 1.0)
            }
            FxSlotType::SpringReverb => {
                self.spring_reverb.tick(in_l, in_r, p[0], p[1], p[2], p[3], 0.3, 0.0, 1.0)
            }
            FxSlotType::ConvReverb => {
                self.conv_reverb.tick(in_l, in_r, p[0], p[1], p[2]*0.2, 1.0)
            }
            FxSlotType::Nimbus => {
                self.nimbus.tick(in_l, in_r, p[0], p[1], p[2], p[3], 0.5, 0.5, 1.0)
            }
            FxSlotType::RingMod => {
                self.ring_mod.tick(in_l, in_r, p[0], p[1], p[2], 0.5, 1.0)
            }
            FxSlotType::FreqShift => {
                self.freq_shift.tick(in_l, in_r, p[0]*1000.0, p[1], p[2], 1.0)
            }
            FxSlotType::Resonator => {
                let rf = p[0];
                let rd = p[1];
                self.resonator.set_freq(0, rf);
                self.resonator.set_freq(1, rf * 1.5);
                self.resonator.set_freq(2, rf * 2.0);
                self.resonator.set_freq(3, rf * 3.0);
                for i in 0..4 { self.resonator.set_decay(i, rd); }
                let mono = (in_l + in_r) * 0.5;
                self.resonator.tick(mono, 1.0)
            }
            FxSlotType::Combulator => {
                self.combulator.tick(in_l, in_r, p[0], p[1], p[2], p[3], 0.5, 1.0)
            }
            FxSlotType::Treemonster => {
                self.treemonster.tick(in_l, in_r, p[0], p[1]*24.0-12.0, p[2], 1.0)
            }
            FxSlotType::Vocoder => {
                let mod_in = (in_l + in_r) * 0.5;
                self.vocoder.tick(in_l, in_r, mod_in, p[0], p[1], 1.0)
            }
            FxSlotType::GraphicEq => {
                // p0=low p1=mid p2=high p3=output — map to 11-band array
                let gains = [p[0]*24.0-12.0, p[0]*24.0-12.0, p[0]*24.0-12.0,
                             p[1]*24.0-12.0, p[1]*24.0-12.0, p[1]*24.0-12.0, p[1]*24.0-12.0,
                             p[2]*24.0-12.0, p[2]*24.0-12.0, p[2]*24.0-12.0, p[2]*24.0-12.0];
                self.graphic_eq_fx.tick(in_l, in_r, &gains, p[3])
            }
            FxSlotType::Compressor => {
                self.compressor.tick(in_l, in_r)
            }
            FxSlotType::MsTool => {
                self.ms_tool.tick(in_l, in_r, p[0]*2.0-1.0, p[1]*2.0-1.0, p[2], 1.0)
            }
            FxSlotType::Conditioner => {
                self.conditioner.tick(in_l, in_r, p[0], p[1], p[2], 1.0)
            }
            FxSlotType::Exciter => {
                self.exciter.tick(in_l, in_r, p[0], p[1]*8000.0+200.0, p[2], 1.0)
            }
        };
        // Apply slot's dry/wet mix
        (dry_l + (wet_l - dry_l) * mix, dry_r + (wet_r - dry_r) * mix)
    }

    #[cfg(test)]
    pub fn tick(&mut self) -> (f32, f32) {
        let mut bl = [0.0f32; 1];
        let mut br = [0.0f32; 1];
        self.tick_block(&mut bl, &mut br);
        (bl[0], br[0])
    }
}


#[cfg(test)]
mod tests;
