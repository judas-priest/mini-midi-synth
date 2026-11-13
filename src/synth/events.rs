//! Event & feedback types crossing the audio-thread boundary (S4-synth 3a):
//! GUI -> engine (`ControlEvent`), MIDI thread -> engine (`MidiEvent`),
//! engine -> GUI (`ParamFeedback`), plus part note-mode helpers shared with
//! `mod.rs`. Move-only out of `mod.rs` — bodies verbatim.

use crate::cc_map::CcMap;
use super::drum::{DrumPattern, DrumSlotParams, NUM_DRUM_SLOTS};
use super::lfo::LfoStepSeqData;
use super::mod_matrix::ModMatrix;
use super::mseg::Mseg;
use super::midi_player;
use super::step_seq;
use super::PatchParams;
use std::sync::Arc;

/// MIDI events sent from the MIDI thread to the audio thread.
#[allow(dead_code)]
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    PitchBend { channel: u8, value: f32 },
    ModWheel { channel: u8, value: f32 },
    ProgramChange { channel: u8, program: u8 },
    Aftertouch { channel: u8, value: f32 },
    PolyAftertouch { channel: u8, note: u8, pressure: f32 },
    ControlChange { channel: u8, cc: u8, value: u8 },
    /// SMK-37 Pro left/right button (SysEx F0 35 59 10 00 xx F7)
    Navigate { pressed: bool }, // true = press, false = release
}

/// Control events sent from GUI to the audio thread.
#[allow(dead_code)]
/// Heavyweight payload for `ControlEvent::LoadPatch`, boxed to keep the
/// `ControlEvent` enum slot small (~2 KB → ~32 B). Built on the GUI thread,
/// shipped through the SPSC queue, consumed once by the audio thread.
pub struct LoadPatchEvent {
    pub part: usize,
    pub params: PatchParams,
    pub mod_matrix: ModMatrix,
    pub mseg1: Option<Mseg>,
    pub mseg2: Option<Mseg>,
    pub pitch_seq: Option<step_seq::PitchSequencer>,
    pub lfo_step_seq: Option<[LfoStepSeqData; 4]>,
    pub wavetable: Option<(Arc<Vec<f32>>, usize, usize)>,
}

/// Полное состояние парта одной транзакцией (ST2): 1 слот кольца вместо
/// 20-22 событий. `patch` = LoadPatchEvent последним (порядок GUI-пуша).
pub struct PartFullState {
    pub enabled: bool,
    pub mute: bool,
    pub volume: f32,
    pub min_note: u8,
    pub max_note: u8,
    pub vel_min: u8,
    pub vel_max: u8,
    pub pan: f32,
    pub transpose: i8,
    pub note_mode: u8,
    pub sustain_enabled: bool,
    pub ignore_pb: bool,
    pub ignore_mw: bool,
    pub sf2_mode: bool,
    pub sf2_program: u8,
    pub macro_vals: [f32; 8],
    pub patch: Option<Box<LoadPatchEvent>>,
}

/// Per-part note mode (TECHDEBT #24-семейство): Poly — все ноты; Bass —
/// нижняя зажатая (бывший root_only); Root — честный корень зажатого аккорда
/// (chord::detect_root), фолбэк на Bass для некордовых наборов.
pub const NOTE_MODE_POLY: u8 = 0;
pub const NOTE_MODE_BASS: u8 = 1;
pub const NOTE_MODE_ROOT: u8 = 2;

/// Root-only gate decision for one note-on (TECHDEBT #6).
pub(crate) enum RootDecision {
    /// Suppressed: an equal-or-lower key is already the sounding root.
    Suppress,
    /// Sound `note`; `release` (raw) is the old root to release first.
    Sound { release: Option<u8>, note: u8 },
}

/// Note-off side решения note-mode гейта (TECHDEBT #24-семейство, latch-
/// семантика аранжерных басов, юзер-решение 2026-11-08): отпускание части
/// аккорда никогда не пере-детектит корень.
pub(crate) enum RootOff {
    /// POLY/ghost — обычный off по raw.
    Pass,
    /// Остались зажатые: off raw ПОДАВЛЕН, голос корня продолжает звучать.
    Latch,
    /// Все клавиши отпущены: off голоса последнего корня и сброс latch.
    Release { note: u8 },
}

/// Per-part ignore: PB/MW-вклад конкретного парта (TECHDEBT #55).
pub(crate) fn effective_pb(ignore: bool, pb: f32) -> f32 { if ignore { 0.0 } else { pb } }
pub(crate) fn effective_mw(ignore: bool, mw: f32) -> f32 { if ignore { 0.0 } else { mw } }

pub enum ControlEvent {
    LoadPatch(Box<LoadPatchEvent>),
    /// Полное состояние парта одной транзакцией (ST2). Обрабатывается
    /// композицией существующих хэндлеров (см. handle_control).
    SetPartFull { part: usize, state: Box<PartFullState> },
    SetPartEnabled { part: usize, enabled: bool },
    SetPartMute { part: usize, mute: bool },
    /// Note mode per part: NOTE_MODE_POLY (all notes) / NOTE_MODE_BASS
    /// (lowest held — the old root-only) / NOTE_MODE_ROOT (chord root).
    /// Clears the root re-key tracking on EVERY change.
    SetPartNoteMode { part: usize, mode: u8 },
    /// Sustain pedal (CC64) affects this part (Sus checkbox). While the pedal
    /// is down, turning it OFF also releases this part's pedal-held notes.
    SetPartSustain { part: usize, on: bool },
    /// Part ignores incoming Pitch Bend (TECHDEBT #55).
    SetPartIgnorePb { part: usize, on: bool },
    /// Part ignores incoming Mod Wheel (TECHDEBT #55).
    SetPartIgnoreMw { part: usize, on: bool },
    SetPartVolume { part: usize, volume: f32 },
    SetPartRange { part: usize, min_note: u8, max_note: u8 },
    SetPartVelRange { part: usize, vel_min: u8, vel_max: u8 },
    SetPartPan { part: usize, pan: f32 },
    SetPartTranspose { part: usize, semitones: i8 },
    /// Mono mode per part (last-note priority). true: poly/piano → mono
    /// (1), Mono-ST (2) → mono (1); false: mono/mono-ST → poly (0). Latch
    /// (3) / Poly-High / Poly-Low are left untouched. Clears the stale
    /// mono note stack on every transition.
    SetPartMono { part: usize, mono: bool },
    /// Legato per part: mono (1) ↔ mono-ST (2). No-op in other modes.
    SetPartLegato { part: usize, legato: bool },
    /// Portamento glide time in milliseconds (0 = instant). Clamped to
    /// 0..2000 ms (patch param `portamento_time` stays in seconds). A
    /// non-zero value also raises portamento_mode 0→1 ("Always").
    SetPartGlide { part: usize, ms: f32 },
    SetMacro { part: usize, index: usize, value: f32 },
    SetCcMap { map: Box<CcMap> },
    SetGlobalParam { key: &'static str, value: f32 },
    // Drum engine controls
    DrumSetStep { slot: u8, step: u8, velocity: u8 },
    DrumSetParam { slot: u8, param: DrumParam },
    /// Mute слота ударных: гасит и секвенсорные, и живые удары (запись в
    /// сетку продолжается — mute это решение микса).
    DrumSetMute { slot: u8, mute: bool },
    DrumSetVolume { volume: f32 },
    DrumSeqPlay { playing: bool },
    DrumSeqBpm { bpm: f32 },
    DrumSeqSwing { swing: f32 },
    DrumSeqPattern { pattern: u8 },
    /// Quantized pattern switch (#32 Phase 5): queued by the GUI, applied at
    /// the next pattern boundary. NOTE: DrumSeqPattern above stays IMMEDIATE
    /// on purpose — it is the hardware console transport path; the GUI
    /// selector is quantized (deliberate asymmetry).
    DrumSeqQueuePattern { pattern: u8 },
    /// Chain mode on/off: at each boundary the first marked pattern after the
    /// current one plays (one pattern per bar).
    DrumSeqChain { on: bool },
    /// Toggle one pattern's membership in the chain.
    DrumSeqChainToggle { pattern: u8 },
    DrumSeqLength { length: u8 },
    /// Time signature for the current pattern: sets `beats_per_bar` and
    /// `length = beats_per_bar * 4` (16th steps). Clamped to 2..=4 —
    /// MAX_STEPS = 16 caps the bar at 4/4 (see docs/TECHDEBT.md §21).
    DrumSeqTimeSig { beats_per_bar: u8 },
    DrumSeqRecord { recording: bool },
    DrumSeqClear,
    DrumSeqUndo,
    DrumSeqRedo,
    DrumLoadKit { patterns: Box<[DrumPattern; 8]>, params: Box<[DrumSlotParams; NUM_DRUM_SLOTS]>, bpm: f32, swing: f32, volume: f32 },
    // Looper controls
    LooperRecord,
    LooperStopRecord,
    LooperTogglePlay,
    LooperUndo,
    LooperClear,
    LooperSetBars { bars: u8 },
    LooperSetQuantize { quantize: u8 },
    /// Restore recorded looper content after an audio reconnect. The payload
    /// owns heap memory and is handed back from handle_control for disposal.
    LooperRestore {
        events: Box<[crate::synth::looper::LooperEventSnapshot]>,
        loop_length_ticks: u32,
    },
    LooperSetLayerMute { layer: u8, mute: bool },
    LooperSetSolo { layer: Option<u8> },
    /// Per-part replay mute (TECHDEBT #30/#34): a muted part's recorded
    /// events are skipped on looper playback; its sounding notes are flushed.
    LooperSetPartMute { part: u8, mute: bool },
    /// Looper record source: true = Mix (every sounding enabled part is
    /// recorded, each event tagged with its own part id), false = Part
    /// (only the active part). Live transport state — not persisted.
    LooperSetSource { mix: bool },
    /// Master transport (#32): toggle play drums + looper together
    MasterPlay,
    /// Master transport: stop drums + looper (drain note-offs)
    MasterStop,
    /// Master transport: arm/disarm recording on drums + looper
    MasterRecord,
    /// Master overdub (layer) toggle: quantized loop Playing -> Overdubbing
    /// at the next boundary; again -> close at the next boundary
    MasterOverdub,
    /// Квантованный захват: добавить цикл к петле (+1)
    CaptureCycleAdd,
    /// Квантованный захват: убрать последний цикл (−1, откат курсора записи)
    CaptureCycleRemove,
    SetSustain { on: bool },
    /// Static pedal-up release duration in seconds (None = patch release)
    SetSustainRelease { seconds: Option<f32> },
    AllNotesOff,
    /// GUI sustain toggle (F1): same path as CC64
    /// Drums: force every hit to velocity 127
    DrumSetFixedVelocity { fixed: bool },
    /// Drums: metronome click on every beat while the sequencer plays
    DrumSetMetronome { on: bool },
    /// Drums: pad-note -> internal slot table (index = in_note-36, value = slot)
    DrumSetRemap { table: Box<[u8; 16]> },
    /// Drums: per-note choke group ids for the SF2 path, indexed (note-30) (0 = none)
    DrumSetChokeGroups { table: Box<[u8; 26]> },
    /// Drums: per-pad velocity override (index = in_note-36, 0 = off)
    DrumSetPadVel { table: Box<[u8; 16]> },
    // SF2 sampler controls
    LoadKeysSoundFont { soundfont: std::sync::Arc<rustysynth::SoundFont> },
    LoadDrumsSoundFont { soundfont: std::sync::Arc<rustysynth::SoundFont> },
    UnloadKeysSoundFont,
    UnloadDrumsSoundFont,
    SetPartSf2Mode { part: usize, enabled: bool },
    SetPartSf2Program { part: usize, program: u8, bank: u8 },
    SetDrumsSf2Mode { enabled: bool },
    SetSf2BlockSize { size: usize },
    SetSf2SampleOffset { ms: f32 },
    /// Global SF2 release multiplier (GM2 CC72 semantics, 1.0 = file defaults)
    SetSf2ReleaseScale { scale: f32 },
    /// Per-engine loudness trim in dB (Settings → Audio). DSP trim scales the
    /// DSP part bus + internal drum engine (NOT the SF2 sampler / looper, the
    /// looper replays through parts); SF2 trim sits on top of SF2_CAL_GAIN.
    SetDspTrim { db: f32 },
    SetSf2Trim { db: f32 },
    // Pitch step sequencer
    SeqSetEnabled { part: usize, enabled: bool },
    SeqSetStep { part: usize, step: u8, pitch: i8, gate: bool, velocity: u8 },
    SeqSetLength { part: usize, length: u8 },
    SeqSetRate { part: usize, rate: u8 },
    SeqSetScale { part: usize, scale: u8 },
    SeqSetSwing { part: usize, swing: f32 },
    // MIDI file sequencer
    MidiSeqLoad { data: Box<midi_player::MidiSeqData> },
    MidiSeqPlay { playing: bool },
    MidiSeqSetBpm { bpm: Option<f32> },
    MidiSeqSetLooping { looping: bool },
    MidiSeqSetTrackInstrument { track_idx: usize, instrument: midi_player::TrackInstrument },
    MidiSeqSetTrackMute { track_idx: usize, muted: bool },
    // Arpeggiator
    SetArpParams { enabled: bool, mode: u8, rate: u8, octaves: u8, gate: f32 },
    /// Arp latch (TECHDEBT 14.4): true — arp keeps playing after keys are
    /// released; false — a frozen latched set is stopped immediately.
    SetArpLatch { latch: bool },
}

impl ControlEvent {
    /// Build a LoadPatch event from a Patch + part index.
    /// Extracts PatchParams, ModMatrix, MSEG, PitchSequencer, wavetable in one place.
    pub fn load_patch_from(part: usize, patch: &crate::preset::Patch) -> Self {
        let params = PatchParams::from_map(&patch.params);
        let mut mod_matrix = ModMatrix::default();
        mod_matrix.load_from_params(&patch.params);
        let mseg1 = if patch.params.get("mseg_enabled").copied().unwrap_or(0.0) > 0.5 {
            Some(Mseg::load_from_params(&patch.params))
        } else {
            None
        };
        let mut pitch_seq = step_seq::PitchSequencer::new();
        pitch_seq.load_from_params(&patch.params);
        let lfo_step_seq = Some([
            LfoStepSeqData::from_params(0, &patch.params),
            LfoStepSeqData::from_params(1, &patch.params),
            LfoStepSeqData::from_params(2, &patch.params),
            LfoStepSeqData::from_params(3, &patch.params),
        ]);
        let wavetable = patch.wavetable_data.as_ref().map(|d| {
            (d.clone(), patch.wavetable_frames, patch.wavetable_frame_size)
        });
        Self::LoadPatch(Box::new(LoadPatchEvent {
            part, params, mod_matrix, mseg1, mseg2: None,
            pitch_seq: Some(pitch_seq), lfo_step_seq, wavetable,
        }))
    }

    /// Build a LoadPatch from edited params (BTreeMap) + optional wavetable from original Patch.
    pub fn load_patch_from_edited(
        part: usize,
        edited: &std::collections::BTreeMap<String, f32>,
        original_patch: Option<&crate::preset::Patch>,
    ) -> Self {
        let params = PatchParams::from_map(edited);
        let mut mod_matrix = ModMatrix::default();
        mod_matrix.load_from_params(edited);
        let mseg1 = if edited.get("mseg_enabled").copied().unwrap_or(0.0) > 0.5 {
            Some(Mseg::load_from_params(edited))
        } else {
            None
        };
        let mut pitch_seq = step_seq::PitchSequencer::new();
        pitch_seq.load_from_params(edited);
        let lfo_step_seq = Some([
            LfoStepSeqData::from_params(0, edited),
            LfoStepSeqData::from_params(1, edited),
            LfoStepSeqData::from_params(2, edited),
            LfoStepSeqData::from_params(3, edited),
        ]);
        let wavetable = original_patch.and_then(|p| {
            p.wavetable_data.as_ref().map(|d| (d.clone(), p.wavetable_frames, p.wavetable_frame_size))
        });
        Self::LoadPatch(Box::new(LoadPatchEvent {
            part, params, mod_matrix, mseg1, mseg2: None,
            pitch_seq: Some(pitch_seq), lfo_step_seq, wavetable,
        }))
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum DrumParam {
    Level(f32),
    Pan(f32),
    Tune(f32),
    Decay(f32),
}

/// Feedback from audio thread to GUI.
pub enum ParamFeedback {
    ParamChanged { key: &'static str, value: f32 },
    CcReceived { cc: u8 },
    ProgramChanged { program: u8 },
    DrumStepRecorded { pattern: u8, slot: u8, step: u8, velocity: u8 },
    /// Engine-side undo/redo restored patterns — GUI mirror must resync
    /// (Box travels to the GUI thread for freeing; zero alloc on audio).
    DrumPatternsSync { patterns: Box<[crate::synth::drum::DrumPattern; 8]> },
    /// Engine-side clear of the current pattern — GUI zeroes its mirror slot.
    DrumCleared { pattern: u8 },
    /// Pickup indicator: knob position (0-127) vs param for a CC that hasn't picked up yet.
    PickupPending { key: &'static str, cc_position: f32 },
    /// CC has picked up — clear the indicator.
    PickupDone { key: &'static str },
    /// SysEx navigate button press/release.
    NavigatePress,
    NavigateRelease,
}
