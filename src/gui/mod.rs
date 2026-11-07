//! GUI interface using egui/eframe.

mod params;
mod drums;

/// Parts per split zone (A: 0..N, B: N..2N)
const PARTS_PER_ZONE: usize = 8;
mod midi_seq;
mod settings;
mod keyboard;
mod layers;
mod fx_chain;
mod macros;
mod theme;
pub mod keybinds;
pub mod led_console;
pub mod note_flash;
mod confirm;
mod modal;
mod browser;
mod sf2;
mod tracks;
mod performance;

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::cc_map::CcMap;
use crate::config::Config;
use crate::midi::{self, NoteState};
use crate::preset::{self, Patch};
use crate::synth::{ControlEvent, ParamFeedback};
use crate::synth::drum::{NUM_DRUM_SLOTS, DrumSlotParams, DrumPattern};
use crate::synth::looper::{LooperAtoms, LooperDisplay};

const VELOCITY_CURVE_NAMES: &[&str] = &["Linear", "Exponential", "Logarithmic", "Fixed"];
const LFO_WAVEFORM_NAMES: &[&str] = &["Sine", "Triangle", "Square", "Sample & Hold", "Sawtooth", "Envelope", "Noise", "Smooth Noise"];
const PORTAMENTO_MODE_NAMES: &[&str] = &["Off", "Always", "Fingered (Legato)"];
const PLAY_MODE_NAMES: &[&str] = &["Poly", "Mono", "Mono-ST", "Latch", "Poly-High", "Poly-Low", "Piano"];

const NOTE_NAMES: &[&str] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

pub const BUFFER_SIZES: &[u32] = &[0, 16, 32, 48, 64, 128, 256, 512, 1024, 2048];

const OSC_NAMES: &[&str] = &[
    "Sine", "Saw", "Square", "Triangle", "FM", "Noise",
    "Karplus-Strong", "Organ", "FM Piano", "Piano (Physical)", "Piano (Banded)",
    "Piano (Additive)", "Drum Synth", "Bass Guitar", "Bowed String", "Brass",
    "Phase Dist", "Wavefolder", "Modal", "Hard Sync", "Supersaw",
    "Piano (Inharmonic)", "Accordion", "Saxophone", "E.Piano (Rhodes/Wurli)",
    "Alias (8-bit)", "Window", "Wavetable", "FM3",
    "Twist (Plaits)",  // 29 — Mutable Instruments Plaits: 24 synthesis engines
];
const PD_SHAPE_NAMES: &[&str] = &["Saw", "Square", "Pulse", "DoubleSine", "SawPulse", "Reso1", "Reso2", "Reso3"];
const FOLD_SOURCE_NAMES: &[&str] = &["Sine", "Triangle", "Saw"];
const MODAL_MATERIAL_NAMES: &[&str] = &[
    "Steel Bar", "Aluminum", "Glass", "Wood Block", "Marimba",
    "Vibraphone", "Tubular Bell", "Church Bell", "Membrane", "Timpani",
];
const SYNC_SHAPE_NAMES: &[&str] = &["Saw", "Square", "Triangle"];
const BASS_STYLE_NAMES: &[&str] = &["Finger", "Pick", "Slap"];
const BASS_PICKUP_NAMES: &[&str] = &["Bridge", "Neck", "Both"];
const BOWED_BODY_NAMES: &[&str] = &["Violin", "Viola", "Cello", "Double Bass"];
const BRASS_BELL_NAMES: &[&str] = &["Trumpet", "French Horn", "Trombone", "Tuba"];
const ACCORDION_REGISTER_NAMES: &[&str] = &["Fundamental", "Octave+", "Musette", "Master"];
const SAX_TYPE_NAMES: &[&str] = &["Soprano", "Alto", "Tenor", "Baritone"];
const ALIAS_WAVE_NAMES: &[&str] = &["Sine", "Ramp", "Pulse", "Noise", "Additive"];
const WINDOW_TYPE_NAMES: &[&str] = &["Triangle", "Cosine", "Half-Sine", "Hann"];
const ENV_SHAPE_NAMES: &[&str] = &["Sqrt (Fast)", "Linear", "Quadratic (Slow)", "Exponential"];
const REVERB_TYPE_NAMES: &[&str] = &["Plate", "Spring"];
const WAVE_SHAPER_MODE_NAMES: &[&str] = &[
    "Tanh", "HardClip", "Asymmetric", "SinFold", "TriFold", "Digital", "Diode", "Rectify",
    "Harm2", "Harm3", "Harm4", "Harm5",
    "Softfold", "Singlefold", "Dualfold", "WestCoast",
    "FuzzSoft", "FuzzHeavy", "FuzzCenter", "FuzzEdge", "FuzzSoft2", "FuzzRect",
    "Sin+x", "Sin2x+x", "Atan",
];
const RING_MOD_SHAPE_NAMES: &[&str] = &["Sine", "Saw", "Square"];
const SIMPLE_OSC_NAMES: &[&str] = &["Sine", "Saw", "Square", "Triangle", "FM"];
const FILTER_NAMES: &[&str] = &[
    "LowPass",       // 0
    "HighPass",      // 1
    "BandPass",      // 2
    "Formant",       // 3
    "Moog 24dB",     // 4
    "Moog 12dB",     // 5
    "Diode 18dB",    // 6
    "Comb",          // 7
    "Allpass",       // 8
    "Comb+",         // 9
    "Comb-",         // 10
    "Notch",         // 11
    "LP 24dB",       // 12
    "HP 24dB",       // 13
    "K35 LP",        // 14
    "K35 HP",        // 15
    "BP 24dB",       // 16
    "Notch 24dB",    // 17
    "OB-Xd 2P LP",   // 18
    "OB-Xd 2P HP",   // 19
    "OB-Xd 2P BP",   // 20
    "OB-Xd 2P Notch",// 21
    "OB-Xd 4P",      // 22
    "Tripole 18dB",  // 23
    "Sample & Hold", // 24
    "CutWarp LP",    // 25
    "CutWarp HP",    // 26
    "CutWarp BP",    // 27
    "CutWarp Notch", // 28
    "CutWarp AP",    // 29
    "ResWarp LP",    // 30
    "ResWarp HP",    // 31
    "ResWarp BP",    // 32
    "ResWarp Notch", // 33
    "ResWarp AP",    // 34
    "Vintage Ladder",// 35
    "SVF Morph",     // 36
    "Polivoks LP",   // 37
    "Polivoks BP",   // 38
];
const FILTER_ROUTING_NAMES: &[&str] = &["Single", "Serial", "Parallel"];
const FORMANT_VOICE_NAMES: &[&str] = &["Bass", "Tenor", "Alto", "Soprano"];
const FORMANT_VOWEL_NAMES: &[&str] = &["A (ah)", "E (eh)", "I (ee)", "O (oh)", "U (oo)"];

/// Toast message severity
#[derive(Clone)]
pub enum ToastKind { Success, Error, Info }

/// Active toast notification
#[derive(Clone)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub created: Instant,
}

/// GUI-зеркало macro_vals из edited_params (источник правды — params,
/// зеркало синхронизируется при load перфа и undo; #49: без этого
/// push_full_part_state при audio-reconnect перезаписывал движок stale-миром).
pub(crate) fn macro_vals_from_params(
    params: &std::collections::BTreeMap<String, f32>,
) -> [f32; 8] {
    let mut out = [0.0f32; 8];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = params.get(&format!("macro_{i}")).copied().unwrap_or(0.0);
    }
    out
}

fn buffer_label(size: u32) -> String {
    if size == 0 { "Default".to_string() } else { format!("{size}") }
}

/// Per-part GUI state (one entry per part slot 0-7)
pub struct PartState {
    pub patch_idx: usize,
    pub edited_params: std::collections::BTreeMap<String, f32>,
    pub params_dirty: bool,
    pub enabled: bool,   // part is in scene
    pub mute: bool,      // temporarily silenced
    pub volume: f32,
    pub min_note: u8,
    pub max_note: u8,
    pub vel_min: u8,
    pub vel_max: u8,
    pub pan: f32,        // -1..+1
    pub transpose: i8,   // semitones
    /// Note mode: NOTE_MODE_POLY (all notes) / NOTE_MODE_BASS (lowest held —
    /// the old root-only) / NOTE_MODE_ROOT (chord root)
    pub note_mode: u8,
    /// Sustain pedal (CC64) affects this part (Sus checkbox)
    pub sustain_enabled: bool,
    /// Part ignores Pitch Bend (PB checkbox, TECHDEBT #55)
    pub ignore_pb: bool,
    /// Part ignores Mod Wheel (MW checkbox, TECHDEBT #55)
    pub ignore_mw: bool,
    pub sf2_mode: bool,
    pub sf2_program: u8,
    /// Macro knob values (0..1) — live, sent to engine via SetMacro
    pub macro_vals: [f32; 8],
    /// Macro knob names (user-editable, shown as label in GUI)
    pub macro_names: Vec<String>,
    /// XY pad: which macro index is bound to X axis (0-based)
    pub xy_macro_x: usize,
    /// XY pad: which macro index is bound to Y axis (0-based)
    pub xy_macro_y: usize,
}

impl PartState {
    pub fn new_empty() -> Self {
        Self {
            patch_idx: 0, edited_params: Default::default(), params_dirty: false,
            enabled: false, mute: false, volume: 0.8,
            min_note: 0, max_note: 127, vel_min: 1, vel_max: 127,
            pan: 0.0, transpose: 0, note_mode: crate::synth::NOTE_MODE_POLY, sustain_enabled: true,
            ignore_pb: false, ignore_mw: false,
            sf2_mode: false, sf2_program: 0,
            macro_vals: [0.0; 8],
            macro_names: (1..=8).map(|i| format!("Macro {i}")).collect(),
            xy_macro_x: 0,
            xy_macro_y: 1,
        }
    }
}

/// One undo step for a part (#31 minimal): a full snapshot of the live
/// state — patch params + patch choice + mute/pan/transpose. Captured
/// BEFORE the change it undoes. The undo stack is UNIFIED: parameter
/// edits (push_undo_snapshot) and live part actions (mute/pan/transpose/
/// patch click) share one stack, so Ctrl+Z works for both.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PartUndoSnapshot {
    part: usize,
    edited_params: std::collections::BTreeMap<String, f32>,
    patch_idx: usize,
    mute: bool,
    pan: f32,
    transpose: i8,
}

impl PartUndoSnapshot {
    fn capture(part: usize, p: &PartState) -> Self {
        Self {
            part,
            edited_params: p.edited_params.clone(),
            patch_idx: p.patch_idx,
            mute: p.mute,
            pan: p.pan,
            transpose: p.transpose,
        }
    }
}

/// No-duplicate push shared by all undo sources: a snapshot equal to the
/// stack top (ALL fields) is skipped, any real change invalidates redo,
/// depth capped. Pure function — unit-tested without an App.
fn part_undo_push(
    undo: &mut std::collections::VecDeque<PartUndoSnapshot>,
    redo: &mut Vec<PartUndoSnapshot>,
    snap: PartUndoSnapshot,
) {
    if undo.back() == Some(&snap) {
        return; // no change since last snapshot
    }
    undo.push_back(snap);
    redo.clear();
    if undo.len() > App::MAX_UNDO {
        undo.pop_front();
    }
}

/// Ctrl+Z step: pop the newest snapshot; the CURRENT state of the same
/// part is preserved for redo. None = empty stack (no-op).
fn part_undo_step(
    undo: &mut std::collections::VecDeque<PartUndoSnapshot>,
    redo: &mut Vec<PartUndoSnapshot>,
    current: PartUndoSnapshot,
) -> Option<PartUndoSnapshot> {
    let snap = undo.pop_back()?;
    redo.push(current);
    Some(snap)
}

/// Ctrl+Shift+Z step: pop the newest redo entry; current state goes back
/// onto the undo stack. None = empty redo (no-op).
fn part_redo_step(
    undo: &mut std::collections::VecDeque<PartUndoSnapshot>,
    redo: &mut Vec<PartUndoSnapshot>,
    current: PartUndoSnapshot,
) -> Option<PartUndoSnapshot> {
    let snap = redo.pop()?;
    undo.push_back(current);
    if undo.len() > App::MAX_UNDO {
        undo.pop_front();
    }
    Some(snap)
}

#[allow(unused_imports)] // Sf2Scan re-exported for gui API parity (no in-crate consumer yet)
pub use sf2::{Sf2Scan, scan_sf2_files};
pub(crate) use sf2::sf2_dir;

fn note_name(note: u8) -> String {
    let name = NOTE_NAMES[(note % 12) as usize];
    let oct = (note as i8 / 12) - 2;
    format!("{name}{oct}")
}

type MidiReconnectFn = Box<dyn FnMut(&str) -> Result<(), String>>;

pub struct App {
    pub patches: Vec<Patch>,
    pub note_state: NoteState,
    pub pad_state: midi::PadState,
    pub ctrl_tx: crate::metrics::CountingProducer<ControlEvent>,
    pub sample_rate: u32,

    pub config: Config,
    pub current_host: String,
    #[cfg(not(target_os = "android"))]
    pub available_hosts: Vec<(cpal::HostId, &'static str)>,
    #[cfg(not(target_os = "android"))]
    pub supported_sample_rates: Vec<u32>,

    pub midi_port_names: Vec<String>,
    pub midi_connected_port: Option<String>,

    pub show_settings: bool,
    #[cfg(not(target_os = "android"))]
    pub selected_host_idx: usize,
    pub selected_sample_rate: u32,
    pub selected_buffer_size: u32,
    pub selected_midi_port: Option<String>,
    pub is_jack: bool,

    /// Layer states
    pub parts: Vec<PartState>,
    /// Currently edited part (0 = A, 1 = B)
    pub active_part: usize,

    pub on_midi_reconnect: Option<MidiReconnectFn>,

    /// Collapsed patch categories
    pub collapsed_categories: HashSet<String>,

    /// Feedback from audio thread
    pub feedback_rx: Option<rtrb::Consumer<ParamFeedback>>,
    /// CC mapping
    pub cc_map: CcMap,
    /// MIDI Learn target parameter key
    pub midi_learn_target: Option<&'static str>,
    /// Part pending removal (middle-click on tab, confirmation dialog)
    pub pending_remove_part: Option<usize>,
    /// Program change atom for patch feedback
    pub _program_change_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Sustain pedal state (CC64) from engine — pedal/fader/GUI toggle
    pub sustain_atom: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Master capture state: 0 idle, 1 armed, 2 capturing, 3 closing —
    /// looper state label shows "ARMED" / "REC k/N"
    pub master_rec_state: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Current capture cycle (1-based)
    pub master_rec_cycle: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Static pedal-up release duration (None = off, patch release)
    pub sustain_release: Option<f32>,
    /// SF2 release-time scale (1.0 = file defaults)
    pub sf2_release_scale: f32,
    /// Show help dialog
    pub show_help: bool,
    /// Global params display values (effects, volume, lfo rate)
    pub global_params: std::collections::BTreeMap<String, f32>,
    /// Pickup indicators: param_key → knob position (for showing direction arrow)
    pub pickup_indicators: std::collections::BTreeMap<String, f32>,

    // Drum sequencer GUI state
    pub show_drums: bool,
    pub drum_patterns: [DrumPattern; 8],
    pub drum_params: [DrumSlotParams; NUM_DRUM_SLOTS],
    /// GUI-зеркало mute слотов (источник истины — GUI, как drum_params).
    pub drum_slot_mutes_view: [bool; NUM_DRUM_SLOTS],
    /// Drum note remap table: slot i plays mapped note drum_remap[i].
    pub drum_remap: [u8; 16],
    pub drum_pad_vel: [u8; 16],
    /// Sound names per kit note (index = note - 30, range 30..=55); from the
    /// loaded SF2 kit (fallback: GM names for 36..=51, empty elsewhere)
    pub drum_kit_names: [String; 26],
    pub show_drum_remap: bool,
    /// Remap dialog for a single hardware pad (0..16), from drawn-pad click
    pub remap_pad: Option<usize>,
    pub drum_volume: f32,
    pub drum_fixed_velocity: bool,
    pub drum_bpm: f32,
    pub drum_swing: f32,
    pub drum_playing: bool,
    pub drum_recording: bool,
    pub drum_current_pattern: u8,
    pub drum_step_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    pub drum_play_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    pub drum_rec_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Live pattern from the engine (#32 Phase 5): the ONLY source of truth
    /// for drum_current_pattern — the grid, console view and LED would lie
    /// after a chain switch otherwise.
    pub drum_pattern_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Chain mode (GUI toggle; the engine holds the authoritative mask).
    pub drum_chain_on: bool,
    /// GUI mirror of the chain mask for marking the selector (lossy under
    /// ring overflow like every GUI→engine push).
    pub drum_chain_mask: u8,
    pub drum_metronome: bool,
    /// Previous sequencer step (downbeat-flash edge detection)
    pub drum_prev_step: u8,
    /// Instant of the last beat-1 transition (GUI flash)
    pub drum_beat_flash: Option<std::time::Instant>,
    pub perf_window: usize,   // first perf_list idx visible on slots 1..=7
    /// First folder idx visible on Tracks-mode slots 1..=7 (#57).
    pub tracks_window: usize,
    /// Keyboard-selected folder in the Tracks manager (#35); None = nothing
    /// selected yet (Enter is a no-op until arrows moved once).
    pub tracks_sel: Option<usize>,
    /// Last-frame focus of the rename TextEdit in the Tracks manager —
    /// conservative nav guard (set during draw, read next frame in update()).
    pub tracks_rename_focus: bool,
    pub pat_window: usize,    // first pattern idx visible on slots 1..=7
    pub mute_window: usize,   // first part idx visible on MUTE slots 1..=7
    pub drum_undo_atom: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub drum_redo_atom: std::sync::Arc<std::sync::atomic::AtomicBool>,
    // Bank-2 pad console
    pub console: led_console::LedConsole,
    pub console_rx: Option<std::sync::mpsc::Receiver<(u8, bool)>>,
    pub flash: note_flash::NoteFlashState,
    pub flash_rx: rtrb::Consumer<crate::midi::NoteFlash>,
    pub led_snapshot: [[u8; 3]; 16],
    pub led_dirty: bool,
    pub led_socket: Option<std::sync::mpsc::Sender<String>>,
    pub led_socket_alive: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub led_socket_failed_at: Option<std::time::Instant>,
    /// In-flight connect attempt (runs off the GUI thread — unix connect
    /// blocks on a full peer backlog).
    pub led_connecting:
        Option<std::sync::mpsc::Receiver<std::io::Result<std::os::unix::net::UnixStream>>>,
    pub led_last_send: Option<std::time::Instant>,
    /// Companion process WE spawned (never kill a foreign one).
    pub led_child: Option<std::process::Child>,
    /// Backoff gate for (re)spawn attempts (crash / spawn failure).
    pub led_spawn_backoff: Option<std::time::Instant>,
    /// Last seen console mode — mode-entry window sync (#50 finding 4).
    pub console_prev_mode: Option<led_console::Mode>,
    /// Drag-and-drop part swap (#54): исходный слот драга за хендл ⠿,
    /// живёт в пределах GUI-фрейма/драга (Some пока pointer down).
    pub part_drag_src: Option<usize>,
    /// #54: цель драга (строка под pointer во время драга) — commit на
    /// drag_stopped хендла
    pub part_drag_dst: Option<usize>,
    /// Track Folders: armed delete confirmation (auto-disarm) — #57 UX.
    pub tracks_del_armed: Option<std::time::Instant>,
    // Drum kit save/load
    pub drum_kit_name: String,
    pub drum_kit_list: Vec<(String, std::path::PathBuf)>,
    pub drum_midi_import_path: String,
    // Looper
    pub show_looper: bool,
    pub looper_atoms: std::sync::Arc<LooperAtoms>,
    pub looper_display: std::sync::Arc<std::sync::Mutex<LooperDisplay>>,
    pub looper_bars: u8,
    pub looper_quantize: u8,
    pub looper_clear_confirm: Option<std::time::Instant>,
    pub drum_clear_confirm: Option<std::time::Instant>,
    pub fx_reset_confirm: Option<std::time::Instant>,
    /// Two-stage confirm for Pad Remap «Reset» (remap 1:1 + zero velocities)
    pub pad_remap_reset_confirm: Option<std::time::Instant>,
    pub looper_layer_mute: [bool; 256],
    pub looper_solo_layer: Option<u8>,
    /// Looper record source: true = Mix (all sounding parts, default),
    /// false = active part only. Live transport state — not persisted
    /// (like seq_target); re-pushed on audio reconnect.
    pub looper_source_mix: bool,
    /// Per-part looper replay mute (TECHDEBT #30/#34), indexed by part.
    /// Not persisted; re-pushed on audio reconnect (same as layer mutes).
    pub looper_part_mutes: [bool; 16],
    /// 0 = SEQ buttons → drums, 1 = SEQ buttons → looper
    pub seq_target_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    pub active_part_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    // Keybindings
    pub keybinds: keybinds::Keybinds,
    pub show_keybinds_window: bool,
    pub keybind_capturing: Option<keybinds::KeyAction>,
    pub keybinds_reset_confirm: Option<std::time::Instant>,
    pub unbind_confirm: Option<(keybinds::KeyAction, std::time::Instant)>,
    // Performance save/load
    pub perf_name: String,
    pub perf_list: Vec<(String, std::path::PathBuf)>,
    /// Консольный список перфов (#57): активная папка (алиасы, tombstone =
    /// None-path) или плоский perf_list. Все консольные сайты читают кэш.
    pub console_list: Vec<(String, Option<std::path::PathBuf>)>,
    /// Алиасы активной папки (FolderPerfs). HOME-синий ряд = console_list
    /// (ВСЕГДА полный список перфов, независимо от трека — юзер 2026-11-08);
    /// папка-фильтрация живёт ТОЛЬКО в этом списке.
    pub folder_console_list: Vec<(String, Option<std::path::PathBuf>)>,
    // Navigate button timing
    pub nav_press_time: Option<std::time::Instant>,
    // Pad performance map: 16 pads (notes 36-51) → performance names
    pub show_pad_perf: bool,
    /// Track folders manager modal (#57).
    pub show_tracks_manager: bool,
    pub pad_perf_map: [Option<String>; 16],
    pub pad_prev_state: [u8; 16],
    pub last_config_save: std::time::Instant,
    /// (F3) конфиг изменился в памяти и ждёт записи на диск (flush_config).
    pub config_dirty: bool,
    /// Smoothed DSP load percentage shown in the header.
    pub dsp_load: f32,
    /// Decaying peak-hold of DSP load, shown next to `dsp_load`.
    pub dsp_peak: f32,
    pub global_dirty: bool,
    /// (#39/#44) «Что сейчас крутят»: имя шкалы + отформатированное значение +
    /// момент последнего тика — индикатор в правом углу топ-бара, авто-скрытие
    /// через 2 с после последнего изменения (fade 300 мс; практика OSD HUD).
    pub active_knob: Option<(String, String, std::time::Instant)>,
    // SF2 sampler
    pub sf2_file_list: Vec<(String, std::path::PathBuf)>,
    pub sf2_keys_selected: Option<usize>,
    pub sf2_keys_loaded_name: String,
    pub sf2_drums_selected: Option<usize>,
    pub sf2_drums_loaded_name: String,
    pub sf2_drums_enabled: bool,
    pub sf2_keys_soundfont: Option<std::sync::Arc<rustysynth::SoundFont>>,
    pub sf2_drums_soundfont: Option<std::sync::Arc<rustysynth::SoundFont>>,
    pub sf2_block_size: usize,
    pub sf2_sample_offset_ms: f32,
    // Pitch step sequencer
    pub pitch_seq_step_atoms: Vec<std::sync::Arc<std::sync::atomic::AtomicU8>>,
    pub pitch_seq_enabled: [bool; 2],
    pub pitch_seq_steps: [[crate::synth::step_seq::PitchStep; 16]; 2],
    pub pitch_seq_length: [u8; 2],
    pub pitch_seq_rate: [u8; 2],
    pub pitch_seq_scale: [u8; 2],
    pub pitch_seq_swing: [f32; 2],
    // Toast notifications
    pub toasts: Vec<Toast>,
    // Patch search
    pub preset_search: String,
    /// Some(part) = instrument browser modal is open for this part
    pub inst_browser_part: Option<usize>,
    /// Keyboard nav (VS Code Quick Pick): highlighted row in the browser flat list
    pub inst_browser_index: usize,
    /// Курсор ещё не посажен на текущий инструмент парта (при открытии браузера)
    pub inst_browser_seeded: bool,
    /// Search text at last index reset — detects filter changes
    pub inst_browser_last_search: String,
    /// (#37) центрировать выделенную строку один раз (стрелки / открытие / сброс фильтра);
    /// колесо мыши НЕ ставит флаг — список не прыгает при ручном скролле
    pub inst_browser_scroll: bool,
    // Oscilloscope
    pub scope_buf: std::sync::Arc<crate::synth::ScopeBuffer>,
    /// Последний виденный write-индекс scope_buf: не изменился = тишина
    /// (аудио-гейт не пушит) — рисование осциллографа пропускается.
    pub last_scope_write: usize,
    // Split mode
    pub split_point: u8,   // MIDI note where right zone starts (default 60 = C4)
    // FX Chain panel
    pub show_fx_chain: bool,
    // MIDI file sequencer
    pub show_midi_seq: bool,
    pub midi_seq_path: String,
    pub midi_seq_tracks: Vec<midi_seq::MidiSeqTrackGui>,
    pub midi_seq_playing: bool,
    pub midi_seq_looping: bool,
    pub midi_seq_bpm: Option<f32>,
    pub midi_seq_play_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    pub midi_seq_pos_atom: std::sync::Arc<std::sync::atomic::AtomicU32>,
    /// Result of an async file picker (zenity/kdialog subprocess). None = no pending pick.
    pub midi_seq_file_pick: Option<std::sync::Arc<std::sync::Mutex<Option<String>>>>,
    // Undo/redo (#31): UNIFIED per-part snapshots — patch parameter editing
    // AND live part actions (mute/pan/transpose/patch click) in one stack.
    pub undo_stack: std::collections::VecDeque<PartUndoSnapshot>,
    pub redo_stack: Vec<PartUndoSnapshot>,
    // Audio device disconnect detection + reconnect
    pub audio_disconnected: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Device name the current stream was built on (TECHDEBT #18). `None` =
    /// unknown (device.name() failed) — name poll disabled, dead-stream flag
    /// still works.
    pub audio_device_name: Option<String>,
    /// Host the current stream was built on (PR #9). The default-device name
    /// poll only makes sense when the stream itself lives on the default
    /// host; cross-host names never match and cause false reconnects.
    pub audio_host_id: cpal::HostId,
    /// Hotplug reconnect retry state machine (TECHDEBT #18).
    pub audio_watch: crate::audio::ReconnectPolicy,
    pub _audio_handle: Option<crate::audio::AudioBackend>,
    pub midi_tx_shared: crate::midi::SharedMidiTx,
    /// Heap objects the engine and the audio callback handed back. Dropping
    /// them here is what keeps free() off the audio thread.
    pub engine_trash_rx: crate::trash::TrashRx,
    pub cb_trash_rx: crate::trash::TrashRx,
}

impl App {
    /// Construct from CommonInit + run_gui locals (S4-lib d: literal out of
    /// lib.rs). cfg-гварды полей переезжают 1:1.
    #[cfg(feature = "gui")]
    pub(crate) fn new(
        c: crate::init::CommonInit,
        on_midi_reconnect: MidiReconnectFn,
        layer_a: PartState,
        layer_b: PartState,
        parts_extra: Vec<PartState>,
    ) -> Self {
        let mut app = Self {
            patches: c.patches,
            note_state: c.note_state,
            pad_state: c.pad_state,
            ctrl_tx: c.ctrl_tx,
            sample_rate: c.actual_sr,
            config: c.config.clone(),
            #[cfg(not(target_os = "android"))]
            current_host: c.host_name.to_string(),
            #[cfg(target_os = "android")]
            current_host: "AAudio".to_string(),
            #[cfg(not(target_os = "android"))]
            available_hosts: c.available_hosts.clone(),
            #[cfg(not(target_os = "android"))]
            supported_sample_rates: c.supported_sr,
            midi_port_names: c.midi_port_names,
            midi_connected_port: c.midi_connected_name,
            show_settings: false,
            #[cfg(not(target_os = "android"))]
            selected_host_idx: c.host_idx,
            selected_sample_rate: c.config.audio.sample_rate,
            selected_buffer_size: c.config.audio.buffer_size,
            selected_midi_port: c.config.midi.port_name.clone(),
            is_jack: c.is_jack,
            parts: std::iter::once(layer_a).chain(std::iter::once(layer_b)).chain(parts_extra).collect(),
            active_part: 0,
            on_midi_reconnect: Some(on_midi_reconnect),
            collapsed_categories: c.config.ui.collapsed_categories.iter().cloned().collect(),
            feedback_rx: Some(c.feedback_rx),
            cc_map: CcMap::from_config(&c.config),
            midi_learn_target: None,
            pending_remove_part: None,
            _program_change_atom: c.program_change_atom,
            sustain_atom: c.handles.sustain_atom,
            master_rec_state: c.handles.master_rec_state_atom,
            master_rec_cycle: c.handles.master_rec_cycle_atom,
            sustain_release: c.config.ui.sustain_release,
            sf2_release_scale: c.config.ui.sf2_release_scale,
            show_help: false,
            global_params: {
                let mut gp = std::collections::BTreeMap::new();
                gp.insert("master_volume".to_string(), c.config.ui.master_volume);
                gp.insert("master_tone".to_string(), c.config.ui.master_tone);
                gp.insert("reverb_mix".to_string(), c.config.ui.fader_reverb);
                gp.insert("delay_mix".to_string(), c.config.ui.fader_delay);
                gp.insert("pitch_bend_range".to_string(), c.config.ui.pitch_bend_range as f32);
                gp
            },
            pickup_indicators: std::collections::BTreeMap::new(),
            show_drums: false,
            show_looper: false,
            drum_patterns: [crate::synth::drum::DrumPattern::default(); 8],
            drum_params: [crate::synth::drum::DrumSlotParams::default(); NUM_DRUM_SLOTS],
            drum_slot_mutes_view: [false; NUM_DRUM_SLOTS],
            drum_remap: core::array::from_fn(|i| 36 + i as u8),
            drum_pad_vel: [0u8; 16],
            drum_kit_names: Default::default(), // filled by reset_kit_names() below
            show_drum_remap: false,
            remap_pad: None,
            drum_volume: c.config.ui.drum_volume,
            drum_fixed_velocity: c.config.ui.drum_fixed_velocity,
            drum_bpm: 120.0,
            drum_swing: 0.0,
            drum_playing: false,
            drum_recording: false,
            drum_current_pattern: 0,
            drum_step_atom: c.handles.drum_step_atom,
            drum_play_atom: c.handles.drum_play_atom,
            drum_rec_atom: c.handles.drum_rec_atom,
            drum_pattern_atom: c.handles.drum_pattern_atom,
            drum_chain_on: false,
            drum_chain_mask: 0,
            drum_metronome: c.config.ui.drum_metronome,
            drum_prev_step: 0,
            drum_beat_flash: None,
            perf_window: 1,
            tracks_window: 0,
            tracks_sel: None,
            tracks_rename_focus: false,
            pat_window: 0,
            mute_window: 0,
            drum_undo_atom: c.handles.drum_undo_atom,
            drum_redo_atom: c.handles.drum_redo_atom,
            console: led_console::LedConsole::default(),
            console_rx: Some(c.console_rx),
            flash: note_flash::NoteFlashState::new(),
            flash_rx: c.flash_rx,
            led_snapshot: [[0; 3]; 16],
            led_dirty: true,
            led_socket: None,
            led_socket_alive: None,
            led_socket_failed_at: None,
            led_connecting: None,
            led_last_send: None,
            led_child: None,
            led_spawn_backoff: None,
            console_prev_mode: None,
            part_drag_src: None,
            part_drag_dst: None,
            tracks_del_armed: None,
            drum_kit_name: String::new(),
            drum_kit_list: preset::list_drum_kits(),
            drum_midi_import_path: String::new(),
            looper_atoms: c.handles.looper_atoms,
            looper_display: c.handles.looper_display,
            looper_bars: 4,
            looper_quantize: 0,
            looper_clear_confirm: None,
            drum_clear_confirm: None,
            fx_reset_confirm: None,
            pad_remap_reset_confirm: None,
            looper_layer_mute: [false; 256],
            looper_solo_layer: None,
            looper_source_mix: true,
            looper_part_mutes: [false; 16],
            seq_target_atom: c.handles.seq_target_atom,
            active_part_atom: c.handles.active_part_atom,
            keybinds: keybinds::Keybinds::from_config(&c.config.ui.keybinds),
            show_keybinds_window: false,
            keybind_capturing: None,
            keybinds_reset_confirm: None,
            unbind_confirm: None,
            perf_name: String::new(),
            perf_list: {
                let mut perf_list = preset::list_performances();
                perf_list.insert(0, ("(Init)".into(), std::path::PathBuf::new()));
                perf_list
            },
            // #57: пересчитывается первым кадром (guard в update())
            console_list: Vec::new(),
            folder_console_list: Vec::new(),
            nav_press_time: None,
            show_pad_perf: false,
            show_tracks_manager: false,
            pad_perf_map: {
                let saved = &c.config.ui.pad_perf_map;
                std::array::from_fn(|i| saved.get(i).cloned().flatten())
            },
            pad_prev_state: [0u8; 16],
            last_config_save: std::time::Instant::now(),
            config_dirty: false,
            dsp_load: 0.0,
            dsp_peak: 0.0,
            global_dirty: false,
            active_knob: None,
            sf2_file_list: {
                let scan = scan_sf2_files();
                for (dir, err) in &scan.unreadable {
                    log::warn!("[sf2] cannot read {}: {err}", dir.display());
                    eprintln!("[sf2] cannot read {}: {err}", dir.display());
                }
                scan.files
            },
            sf2_keys_selected: None,
            sf2_keys_loaded_name: String::new(),
            sf2_drums_selected: None,
            sf2_drums_loaded_name: String::new(),
            sf2_drums_enabled: c.config.sf2.drums_sf2,
            sf2_keys_soundfont: None,
            sf2_drums_soundfont: None,
            sf2_block_size: c.config.sf2.block_size,
            sf2_sample_offset_ms: c.config.sf2.sample_offset_ms,
            pitch_seq_step_atoms: c.handles.pitch_seq_step_atoms,
            pitch_seq_enabled: [false; 2],
            pitch_seq_steps: [[crate::synth::step_seq::PitchStep::default(); 16]; 2],
            pitch_seq_length: [16; 2],
            pitch_seq_rate: [2; 2],
            pitch_seq_scale: [0; 2],
            pitch_seq_swing: [0.0; 2],
            preset_search: String::new(),
            inst_browser_part: None,
            inst_browser_index: 0,
            inst_browser_seeded: false,
            inst_browser_last_search: String::new(),
            inst_browser_scroll: false,
            scope_buf: c.handles.scope_buf,
            last_scope_write: 0,
            split_point: c.config.ui.split_point.unwrap_or(60),
            show_fx_chain: false,
            show_midi_seq: false,
            midi_seq_path: String::new(),
            midi_seq_tracks: Vec::new(),
            midi_seq_playing: false,
            midi_seq_looping: true,
            midi_seq_bpm: None,
            midi_seq_play_atom: c.handles.midi_seq_play_atom,
            midi_seq_pos_atom: c.handles.midi_seq_pos_atom,
            midi_seq_file_pick: None,
            undo_stack: std::collections::VecDeque::new(),
            redo_stack: Vec::new(),
            toasts: Vec::new(),
            audio_disconnected: c.audio_disconnected.clone(),
            audio_device_name: c._audio_handle.device_name.clone(),
            audio_host_id: c._audio_handle.host_id,
            audio_watch: crate::audio::ReconnectPolicy::default(),
            _audio_handle: Some(c._audio_handle),
            midi_tx_shared: c.midi_tx_shared.clone(),
            engine_trash_rx: c.engine_trash_rx,
            cb_trash_rx: c.cb_trash_rx,
        };
        app.reset_kit_names();
        app
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_global_to_config();
        self.flush_config(true);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // On Android high-DPI screens, set proper pixel ratio for crisp font rendering
        #[cfg(target_os = "android")]
        ctx.set_pixels_per_point(2.5);

        #[cfg(target_os = "android")]
        ctx.style_mut(|style| {
            style.spacing.interact_size = egui::vec2(48.0, 36.0); // Material Design min touch target
            style.spacing.button_padding = egui::vec2(12.0, 8.0);
            style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        });

        // Increase base font size and spacing
        ctx.style_mut(|style| {
            for (text_style, size) in [
                (egui::TextStyle::Body, 15.0),
                (egui::TextStyle::Button, 13.0),
                (egui::TextStyle::Monospace, 14.0),
                (egui::TextStyle::Small, 13.0),
                (egui::TextStyle::Heading, 20.0),
            ] {
                if let Some(font_id) = style.text_styles.get_mut(&text_style) {
                    font_id.size = size;
                }
            }
            style.spacing.slider_width = theme::SLIDER_PRIMARY;
            style.spacing.item_spacing = egui::vec2(8.0, 5.0);
        });

        let has_active_notes = (0..128u8)
            .any(|i| self.note_state[i as usize].load(Ordering::Relaxed) > 0);
        let looper_active = self.looper_atoms.state.load(Ordering::Relaxed) != 0;
        let midi_seq_active = self.midi_seq_play_atom.load(Ordering::Relaxed) != 0;
        if has_active_notes || self.drum_playing || looper_active || midi_seq_active {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            // Полная тишина: интерфейс статичен; egui сам репейнтит на мышь/клаву.
            // 250 мс — компромисс: индикаторы клавиатуры из аудио-атомиков
            // отображаются с задержкой до 250 мс (принято юзером 2026-11-02).
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        // NoteFlash: быстрые ноты — атомик on→off в кадре, событие терялось
        let seen = self.flash.drain(&mut self.flash_rx);
        if self.flash.any_active() || seen {
            ctx.request_repaint_after(Duration::from_millis(33));
        }

        // Audio hotplug (TECHDEBT #18): (1) dead-stream flag from the cpal
        // error callback; (2) default-sink watch (`pactl get-default-sink`)
        // lives in a background thread spawned in run_gui — a device swap
        // does not error the old stream: cpal keeps "playing" into the dead
        // device, it just sets audio_disconnected; (3) a failed reconnect
        // retries every RECONNECT_BACKOFF until the device returns. The first
        // failure of a streak toasts; retries are silent. reconnect_audio is
        // a GUI-thread operation (rebuilds the stream); the audio callback
        // only ever sees the finished new stream.
        let now = Instant::now();
        if self.audio_disconnected.swap(false, Ordering::Relaxed) || self.audio_watch.retry_due(now) {
            match self.reconnect_audio() {
                Ok(()) => {
                    self.audio_watch.on_success();
                    self.audio_device_name =
                        self._audio_handle.as_ref().and_then(|b| b.device_name.clone());
                    if let Some(b) = self._audio_handle.as_ref() {
                        self.audio_host_id = b.host_id;
                    }
                    self.show_toast("Audio reconnected".to_string(), ToastKind::Success);
                }
                Err(e) => {
                    if self.audio_watch.on_failure(now) {
                        self.show_toast(format!("Audio reconnect failed: {e}"), ToastKind::Error);
                    }
                }
            }
        }

        // Drain feedback from audio thread
        self.drain_feedback();

        // Free whatever the audio thread handed over this frame.
        self.engine_trash_rx.drain();
        self.cb_trash_rx.drain();

        // Pad performance switching (only when pad perf panel is open)
        if self.show_pad_perf {
            for i in 0..16u8 {
                let note = 36 + i;
                let vel = self.pad_state[note as usize].load(Ordering::Relaxed);
                let prev = self.pad_prev_state[i as usize];
                if vel > 0 && prev == 0 {
                    // New pad press — switch performance if mapped
                    if let Some(ref name) = self.pad_perf_map[i as usize] {
                        let name = name.clone();
                        let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                        if self.load_performance_by_name(&name) {
                            self.show_toast(format!("Loaded: {name}"), ToastKind::Success);
                        } else {
                            self.show_toast(format!("Not found: {name}"), ToastKind::Error);
                        }
                    }
                }
                self.pad_prev_state[i as usize] = vel;
            }
        }

        // Autosave global params (vol/tone) 2 seconds after last change
        if self.global_dirty && self.last_config_save.elapsed() > Duration::from_secs(2) {
            self.save_global_to_config();
            self.global_dirty = false;
        }

        // Keybind processing (skip when text fields have focus or capturing a rebind).
        // Esc при capture НЕ обрабатывается здесь: update-loop выполняется ДО
        // отрисовки окна — Esc-стек (отмена capture → закрытие окна) живёт
        // в теле окна Keybindings, иначе первый Esc сделал бы и то и другое.
        let text_editing = ctx.memory(|m| m.focused().is_some());
        if self.keybind_capturing.is_some() {
            if let Some(key) = keybinds::detect_key_press(ctx) {
                if key != egui::Key::Escape {
                    if let Some(action) = self.keybind_capturing.take() {
                        let stolen = self.keybinds.set_key_for_action(&action, key);
                        self.config.ui.keybinds = self.keybinds.to_config();
                        self.mark_config_dirty();
                        if let Some(victim) = stolen {
                            self.show_toast(
                                format!("{}: key was reassigned from {}",
                                    keybinds::key_to_str(key), victim),
                                ToastKind::Info,
                            );
                        }
                    }
                }
            }
        } else if !text_editing {
            self.process_keybinds(ctx);
        }

        // Undo/Redo keyboard shortcuts (Ctrl+Z / Ctrl+Shift+Z)
        if !text_editing {
            if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z)) {
                self.redo();
            } else if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z)) {
                self.undo();
            }
        }

        // Sync drum play/rec state from engine (for MIDI-triggered changes)
        self.drum_playing = self.drum_play_atom.load(Ordering::Relaxed) != 0;
        self.drum_recording = self.drum_rec_atom.load(Ordering::Relaxed) != 0;
        // Pattern mirror (#32 Phase 5): the atom is the single source of
        // truth — a queued switch applies at the boundary, and chain switches
        // happen in the engine, so the GUI never assigns this field directly.
        self.drum_current_pattern = self.drum_pattern_atom.load(Ordering::Relaxed);
        // Beat-1 flash: the step atom shows the NEXT step, so step 0 has just
        // sounded when the atom transitions 0 -> 1 while playing.
        let step_now = self.drum_step_atom.load(Ordering::Relaxed);
        if self.drum_playing && self.drum_prev_step == 0 && step_now == 1 {
            self.drum_beat_flash = Some(std::time::Instant::now());
        }
        self.drum_prev_step = step_now;

        self.console_event_pump();
        self.led_companion_lifecycle();
        self.led_socket_lifecycle();
        self.led_snapshot_send();

        // MIDI Learn indicator
        if let Some(target_name) = self.midi_learn_target {
            egui::TopBottomPanel::top("midi_learn_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::YELLOW, format!("MIDI Learn: turn a knob to assign CC to '{target_name}'..."));
                    if ui.button("Cancel").clicked() {
                        self.midi_learn_target = None;
                    }
                });
            });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.midi_learn_target = None;
            }
        }

        // Top bar
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(theme::SP_XS);
            ui.horizontal(|ui| {
                if ui.button("\u{2699}").on_hover_text("Settings").clicked() {
                    self.show_settings = !self.show_settings;
                    if self.show_settings {
                        self.refresh_midi_ports();
                    }
                }
                if ui.button("?").on_hover_text("CC Mapping Help").clicked() {
                    self.show_help = !self.show_help;
                }
                if ui.button("\u{2328}").on_hover_text("Keybindings").clicked() {
                    self.show_keybinds_window = !self.show_keybinds_window;
                    self.keybind_capturing = None;
                }
                // #30: Dock/Tab тумблер драм-юнита — глифы ⬆/⬇ (U+2B06/2B07)
                // из emoji-icon-font, того же, что рисует ⚙/⌨ (проверено по
                // cmap шрифтов egui; тонких стрелок U+2191 там НЕТ — кнопка
                // была пустой). ⬇ = юнит на вкладке, клик докирует;
                // ⬆ = юнит в доке, клик возвращает на вкладку
                if ui.button(if self.config.ui.dock_bottom_unit { "\u{2B06}" } else { "\u{2B07}" })
                    .on_hover_text(if self.config.ui.dock_bottom_unit {
                        "Seq unit is docked at the window bottom — click to move it to the Drums tab"
                    } else {
                        "Unit lives on the Drums tab — click to dock it to the window bottom"
                    })
                    .clicked()
                {
                    self.config.ui.dock_bottom_unit = !self.config.ui.dock_bottom_unit;
                    self.save_global_to_config();
                }

                ui.separator();

                let midi_status = self
                    .midi_connected_port
                    .as_deref()
                    .unwrap_or("(disconnected)");
                ui.label(format!("MIDI: {midi_status}"));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // (#44) индикатор редактируемой шкалы: авто-скрытие через
                    // 2 с после последнего тика, fade-out последние 300 мс
                    // (практика OSD volume HUD — macOS/GNOME)
                    if let Some((name, val, at)) = &self.active_knob {
                        const HIDE_AFTER: f32 = 2.0;
                        const FADE: f32 = 0.3;
                        let age = at.elapsed().as_secs_f32();
                        if age >= HIDE_AFTER {
                            self.active_knob = None;
                        } else {
                            let alpha = (((HIDE_AFTER - age) / FADE).min(1.0) * 255.0) as u8;
                            let color = ui.visuals().text_color().gamma_multiply(alpha as f32 / 255.0);
                            ui.label(egui::RichText::new(format!("{name}: {val}"))
                                .strong().color(color));
                            ui.separator();
                        }
                    }

                    // DSP load: avg (устойчивая) + peak-hold с медленным спадом
                    // (практика REAPER). Бюджет — от ФАКТИЧЕСКОГО буфера:
                    // negotiate может молча откатить запрошенный.
                    let actual_buf = crate::metrics::METRICS.actual_buffer_size();
                    let buf = if actual_buf > 0 { actual_buf } else { self.config.audio.buffer_size };
                    let budget_us = buf as f32 / self.sample_rate as f32 * 1e6;
                    // take_* читаем БЕЗУСЛОВНО каждый кадр: worst-пики копятся,
                    // и если читать только при видимой телеметрии, при включении
                    // выстрелит минутный максимум (#44).
                    let avg_us = crate::metrics::METRICS.take_avg_callback_us() as f32;
                    let peak_us = crate::metrics::METRICS.take_worst_callback_us() as f32;
                    if budget_us > 0.0 {
                        let avg_load = avg_us / budget_us * 100.0;
                        self.dsp_load = self.dsp_load * 0.8 + avg_load * 0.2;
                        let peak_load = peak_us / budget_us * 100.0;
                        self.dsp_peak = (self.dsp_peak * 0.98).max(peak_load);
                    }

                    if self.config.ui.show_telemetry {
                        ui.label(format!(
                            "{} | {}Hz | buf {} | dsp {:.0}% (peak {:.0}%)",
                            self.current_host,
                            self.sample_rate,
                            buf,
                            self.dsp_load,
                            self.dsp_peak,
                        ))
                        .on_hover_text(
                            "Audio callback time as a share of the buffer budget: \
                             average and decaying peak. Peak approaching 100% means crackle.",
                        );

                        let dropped = crate::metrics::METRICS.total_dropped();
                        let errors = crate::metrics::METRICS.stream_error_count();
                        if dropped > 0 || errors > 0 {
                            ui.separator();
                            ui.colored_label(
                                theme::TEXT_WARN,
                                format!("drops {dropped} | errs {errors}"),
                            )
                            .on_hover_text(
                                "MIDI and control events that never reached the engine, and audio \
                                 stream errors. A lost NoteOff is a stuck note; non-zero here means \
                                 something is wrong.",
                            );
                        }
                    }
                });
            });
            ui.add_space(theme::SP_SM);
        });

        // Global controls bar
        egui::TopBottomPanel::top("global_controls").show(ctx, |ui| {
            ui.add_space(theme::SP_SM);
            ui.horizontal(|ui| {
                // F1 Sustain (CC64) — reflects pedal/fader state, click to toggle
                let sus = self.sustain_atom.load(std::sync::atomic::Ordering::Relaxed);
                ui.label("F1 Sustain:");
                let sus_btn = egui::Button::new(egui::RichText::new(if sus { "ON" } else { "OFF" })
                    .color(if sus { egui::Color32::BLACK } else { theme::TEXT_MUTED }))
                    .fill(if sus { theme::KEY_WHITE_ACTIVE } else { theme::BG_PAD_EMPTY });
                if ui.add(sus_btn).on_hover_text("Sustain pedal (CC64): holds notes. Click to toggle.").clicked() {
                    let _ = self.ctrl_tx.push(ControlEvent::SetSustain { on: !sus });
                }

                // Static pedal-up release (TECHDEBT #7): CC64 value never enters it
                let sr_on = self.sustain_release.is_some();
                if ui.button("SR").on_hover_text(
                    "Pedal-up release: held notes release in a fixed time instead of the patch release.").clicked()
                {
                    self.sustain_release = if sr_on { None } else { Some(0.3) };
                    let _ = self.ctrl_tx.push(ControlEvent::SetSustainRelease { seconds: self.sustain_release });
                    self.save_global_to_config();
                }
                if sr_on {
                    let mut secs = self.sustain_release.unwrap_or(0.3);
                    if ui.add(egui::Slider::new(&mut secs, 0.05..=8.0)
                        .fixed_decimals(2).suffix(" s")).changed() {
                        self.sustain_release = Some(secs);
                        let _ = self.ctrl_tx.push(ControlEvent::SetSustainRelease { seconds: Some(secs) });
                        self.global_dirty = true;
                    }
                }

                ui.separator();

                // F2 Master Tone (global)
                let mut tone = self.global_params.get("master_tone").copied().unwrap_or(20000.0);
                ui.label("F2 Tone:");
                let tone_r = ui.add(egui::Slider::new(&mut tone, 200.0..=20000.0).logarithmic(true).suffix(" Hz").fixed_decimals(0))
                    .on_hover_text("Double-click to reset");
                if tone_r.double_clicked() { tone = 20000.0; }
                if tone_r.changed() || tone_r.double_clicked() {
                    self.global_params.insert("master_tone".into(), tone);
                    let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "master_tone", value: tone });
                    self.global_dirty = true;
                }
                self.knob_feedback(&tone_r, "Tone", format!("{tone:.0} Hz"));
                // (#45) колесо над слайдером = шаг значения; глотаем smooth-дельту,
                // чтобы контейнер не скроллил. Шкала логарифмическая — шаг кратный
                // (колесо-вверх = увеличение).
                if tone_r.hovered() {
                    let dy = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                    if dy != 0.0 {
                        tone = (tone * (1.0 + dy * 0.0025)).clamp(200.0, 20000.0);
                        ui.ctx().input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
                        self.global_params.insert("master_tone".into(), tone);
                        let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "master_tone", value: tone });
                        self.global_dirty = true;
                        self.knob_feedback(&tone_r, "Tone", format!("{tone:.0} Hz"));
                    }
                }

                ui.separator();

                // F3 Reverb (global)
                let mut reverb = self.global_params.get("reverb_mix").copied().unwrap_or(0.0);
                ui.label("F3 Reverb:");
                let rev_r = ui.add(egui::Slider::new(&mut reverb, 0.0..=1.0).fixed_decimals(2))
                    .on_hover_text("Double-click to reset");
                if rev_r.double_clicked() { reverb = 0.0; }
                if rev_r.changed() || rev_r.double_clicked() {
                    self.global_params.insert("reverb_mix".into(), reverb);
                    let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "reverb_mix", value: reverb });
                    self.global_dirty = true;
                }
                self.knob_feedback(&rev_r, "Reverb", format!("{reverb:.2}"));
                // (#45) колесо: см. Tone (линейный шаг 0.0025/px)
                if rev_r.hovered() {
                    let dy = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                    if dy != 0.0 {
                        reverb = (reverb + dy * 0.0025).clamp(0.0, 1.0);
                        ui.ctx().input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
                        self.global_params.insert("reverb_mix".into(), reverb);
                        let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "reverb_mix", value: reverb });
                        self.global_dirty = true;
                        self.knob_feedback(&rev_r, "Reverb", format!("{reverb:.2}"));
                    }
                }

                ui.separator();

                // F4 Master Volume (global)
                let mut vol = self.global_params.get("master_volume").copied().unwrap_or(0.8);
                ui.label("F4 Vol:");
                let vol_r = ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).fixed_decimals(2))
                    .on_hover_text("Double-click to reset");
                if vol_r.double_clicked() { vol = 0.8; }
                if vol_r.changed() || vol_r.double_clicked() {
                    self.global_params.insert("master_volume".into(), vol);
                    let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "master_volume", value: vol });
                    self.global_dirty = true;
                }
                self.knob_feedback(&vol_r, "Vol", format!("{vol:.2}"));
                // (#45) колесо: см. Tone (линейный шаг 0.0025/px)
                if vol_r.hovered() {
                    let dy = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                    if dy != 0.0 {
                        vol = (vol + dy * 0.0025).clamp(0.0, 1.0);
                        ui.ctx().input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
                        self.global_params.insert("master_volume".into(), vol);
                        let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "master_volume", value: vol });
                        self.global_dirty = true;
                        self.knob_feedback(&vol_r, "Vol", format!("{vol:.2}"));
                    }
                }

                ui.separator();

                // Pitch Bend Range
                let mut pbr = self.global_params.get("pitch_bend_range").copied().unwrap_or(2.0) as i32;
                ui.label("PB:");
                let pb_r = ui.add(egui::Slider::new(&mut pbr, 1..=24).suffix(" st"))
                    .on_hover_text("Double-click to reset");
                if pb_r.double_clicked() { pbr = 2; }
                if pb_r.changed() || pb_r.double_clicked() {
                    let val = pbr as f32;
                    self.global_params.insert("pitch_bend_range".into(), val);
                    let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "pitch_bend_range", value: val });
                    self.global_dirty = true;
                }
                self.knob_feedback(&pb_r, "PB", format!("{pbr} st"));
                // (#45) колесо: ~1 шаг на ~20px прокрутки (как Tr)
                if pb_r.hovered() {
                    let dy = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                    if dy != 0.0 {
                        pbr = (pbr + (dy * 0.05).round() as i32).clamp(1, 24);
                        ui.ctx().input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
                        let val = pbr as f32;
                        self.global_params.insert("pitch_bend_range".into(), val);
                        let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "pitch_bend_range", value: val });
                        self.global_dirty = true;
                        self.knob_feedback(&pb_r, "PB", format!("{pbr} st"));
                    }
                }

                ui.separator();

                // Oscilloscope
                let scope_w = 150.0_f32;
                let scope_h = 40.0_f32;
                let (resp, painter) = ui.allocate_painter(
                    egui::vec2(scope_w, scope_h),
                    egui::Sense::hover(),
                );
                let r = resp.rect;
                painter.rect_filled(r, 2.0, theme::BG_WIDGET);
                let mid_y = r.center().y;
                // Center line
                painter.line_segment(
                    [egui::pos2(r.left(), mid_y), egui::pos2(r.right(), mid_y)],
                    egui::Stroke::new(0.5, theme::STROKE_SUBTLE),
                );
                // Freeze при тишине: write-индекс не изменился = аудио-гейт
                // не пушит — пропускаем чтение 256 атомиков и построение Vec.
                let wp = self.scope_buf.write_pos.load(Ordering::Relaxed);
                if wp != self.last_scope_write {
                    self.last_scope_write = wp;
                    let scope_data = self.scope_buf.read();
                    // Draw waveform
                    let n = scope_data.len();
                    let step = n as f32 / scope_w;
                    let points: Vec<egui::Pos2> = (0..scope_w as usize).map(|px| {
                        let idx = (px as f32 * step) as usize;
                        let s = scope_data[idx.min(n - 1)].clamp(-1.0, 1.0);
                        egui::pos2(r.left() + px as f32, mid_y - s * scope_h * 0.45)
                    }).collect();
                    if points.len() >= 2 {
                        painter.add(egui::Shape::line(points, egui::Stroke::new(1.0, theme::SCOPE_WAVE)));
                    }
                }
            });
            ui.add_space(theme::SP_SM);
        });

        // Bottom: keyboard
        egui::TopBottomPanel::bottom("keyboard_panel").show(ctx, |ui| {
            ui.add_space(theme::SP_SM);
            self.draw_keyboard(ui);
            ui.add_space(theme::SP_SM);
        });

        // Pad perf indicator (above keyboard)
        if self.show_pad_perf {
            egui::TopBottomPanel::bottom("pad_perf_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::YELLOW, "PAD PERF: tap a pad to switch performance");
                });
            });
        }

        // TECHDEBT #25/#30: ЕДИНАЯ docked-панель — драм-секвенсор СЛЕВА,
        // лупер СПРАВА (один юнит, один мастер-транспорт сверху). egui carves
        // panels in registration order (first bottom panel = the bottom-most
        // strip), so registering AFTER keyboard_panel puts this ABOVE the
        // keyboard/pads strip, and registering BEFORE CentralPanel makes the
        // main field reflow instead of being overlapped. Колонки — ui.columns:
        // allocate_ui в horizontal сворачивает высоту и разваливает раскладку,
        // а ScrollArea без явных id_salt даёт коллизии ID.
        // #30: флаг dock_bottom_unit исключает одновременный рендер дока и
        // вкладки; при off юнит целиком живёт на вкладке Drums (ниже).
        if self.config.ui.dock_bottom_unit {
            egui::TopBottomPanel::bottom("drum_seq_panel")
                .resizable(true)
                .default_height(420.0)
                .height_range(240.0..=760.0)
                .show(ctx, |ui| {
                    ui.columns(2, |cols| {
                        // ScrollArea::both: вертикальный скролл НЕ режет контент по
                        // горизонтали — сетка драмов вылезала под колонку лупера.
                        // now: контент клипается по ширине колонки, избыток —
                        // горизонтальный скролл, а не наложение.
                        egui::ScrollArea::both().auto_shrink(false)
                            .id_salt("dock_drums_scroll")
                            .show(&mut cols[0], |ui| {
                                self.draw_drum_seq_docked(ui);
                            });
                        egui::ScrollArea::both().auto_shrink(false)
                            .id_salt("dock_looper_scroll")
                            .show(&mut cols[1], |ui| {
                                self.draw_looper(ui);
                            });
                    });
                });
        }

        // Settings window
        if self.show_settings {
            self.draw_settings(ctx);
        }
        // Help window
        if self.show_help {
            self.draw_help(ctx);
        }
        // Pad performance map window
        if self.show_pad_perf {
            self.draw_pad_perf_window(ctx);
        }
        // Keybindings window
        if self.show_keybinds_window {
            self.draw_keybinds_window(ctx);
        }

        // #35: Tracks-менеджер — стрелки/Enter, если окно открыто и rename
        // НЕ в фокусе (канон браузера: ввод до окна)
        if self.show_tracks_manager && !self.tracks_rename_focus {
            let folders = self.config.ui.perf_folders.len();
            if folders > 0 {
                let moved = ctx.input(|i| {
                    let mut d: i32 = 0;
                    if i.key_pressed(egui::Key::ArrowDown) { d += 1; }
                    if i.key_pressed(egui::Key::ArrowUp) { d -= 1; }
                    d
                });
                if moved != 0 {
                    let cur = self.tracks_sel
                        .unwrap_or(self.config.ui.active_folder
                            .unwrap_or(0))
                        .min(folders - 1) as i32;
                    self.tracks_sel = Some((cur + moved).clamp(0, folders as i32 - 1) as usize);
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(sel) = self.tracks_sel {
                        if sel < self.config.ui.perf_folders.len() {
                            self.config.ui.active_folder = Some(sel);
                            self.mark_config_dirty();
                            self.refresh_console_list();
                            self.perf_window = 0;
                        }
                    }
                }
            }
        }

        // Instrument browser modal (from part row)
        self.draw_instrument_browser_modal(ctx);

        // Track folders manager modal (#57)
        self.draw_tracks_manager(ctx);

        // Draw toast notifications
        self.toasts.retain(|t| t.created.elapsed().as_secs_f32() < 3.0);
        for (i, toast) in self.toasts.iter().enumerate() {
            let color = match toast.kind {
                ToastKind::Success => egui::Color32::from_rgb(40, 160, 80),
                ToastKind::Error => egui::Color32::from_rgb(200, 60, 60),
                ToastKind::Info => egui::Color32::from_rgb(60, 140, 200),
            };
            egui::Area::new(egui::Id::new("toast").with(i))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -40.0 - i as f32 * 36.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::default()
                        .fill(color)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(12, 6))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(&toast.message).color(egui::Color32::WHITE).strong());
                        });
                });
        }
        if !self.toasts.is_empty() {
            ctx.request_repaint();
        }

        // Pad remap dialogs — global so pad clicks work on any tab (TECHDEBT #11)
        self.draw_drum_remap_modals(ctx);

        // Central: part tabs + parameters / drum sequencer
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(theme::SP_SM);
            self.draw_part_rows(ui);
            ui.add_space(theme::SP_SM);
            ui.separator();
            ui.add_space(theme::SP_MD);
            if self.show_fx_chain {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    self.draw_fx_chain(ui);
                });
            } else if self.show_midi_seq {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    self.draw_midi_seq(ui);
                });
            } else if self.show_drums {
                if self.config.ui.dock_bottom_unit {
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        self.draw_drum_kit_controls(ui);
                    });
                } else {
                    // #30, dock off: юнит ЦЕЛИКОМ на вкладке Drums — те же
                    // колонки, что в доке (drums-юнит слева, лупер справа),
                    // но в central. Рисуем через тот же код.
                    ui.columns(2, |cols| {
                        egui::ScrollArea::both().auto_shrink(false)
                            .id_salt("tab_drums_scroll")
                            .show(&mut cols[0], |ui| {
                                self.draw_drum_seq_docked(ui);
                                ui.add_space(theme::SP_MD);
                                self.draw_drum_kit_controls(ui);
                            });
                        egui::ScrollArea::both().auto_shrink(false)
                            .id_salt("tab_looper_scroll")
                            .show(&mut cols[1], |ui| {
                                self.draw_looper(ui);
                            });
                    });
                }
            } else {
                // Лупер переехал в единую docked-панель (#30); здесь остаётся
                // питч-секвенсор и параметры активной части. ScrollArea как в
                // остальных ветках, на ВСЮ ветку: без него секвенсор молча
                // обрезается docked-панелью (audit High п.1), а оборачивать
                // только секвенсор нельзя — auto_shrink(false) съел бы высоту
                // параметров ниже. Вложенный ScrollArea параметров удалён.
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    self.draw_pitch_sequencer(ui);
                    ui.separator();
                    ui.add_space(theme::SP_MD);
                    let part = self.active_part;
                    let is_sf2 = self.parts[part].sf2_mode;
                    if is_sf2 && self.sf2_keys_soundfont.is_some() {
                        use crate::synth::sampler::GM_PROGRAM_NAMES;
                        let prog = self.parts[part].sf2_program;
                        let name = GM_PROGRAM_NAMES.get(prog as usize).unwrap_or(&"?");
                        ui.colored_label(
                            theme::TEXT_SF2_STATUS,
                            format!("SF2: {} | {}: {} | Effects chain applies", self.sf2_keys_loaded_name, prog, name),
                        );
                    } else {
                        // Header: patch name + Init/Rnd/Undo/Redo
                        ui.horizontal(|ui| {
                            let idx = self.parts[part].patch_idx;
                            let name = self.patches.get(idx).map(|p| p.name.as_str()).unwrap_or("---");
                            ui.strong(name);
                            ui.separator();
                            if ui.small_button("Init").on_hover_text("Reset to default patch").clicked() {
                                self.push_undo_snapshot();
                                let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                                self.parts[part].edited_params = crate::synth::PatchParams::default().to_map();
                                self.send_edited_params(part);
                            }
                            if ui.small_button("Rnd").on_hover_text("Randomize patch").clicked() {
                                self.push_undo_snapshot();
                                let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                                self.parts[part].edited_params = crate::synth::PatchParams::random_map();
                                self.send_edited_params(part);
                            }
                            ui.separator();
                            if ui.add_enabled(!self.undo_stack.is_empty(), egui::Button::new("\u{2B05}").small()).on_hover_text("Undo (Ctrl+Z)").clicked() {
                                self.undo();
                            }
                            if ui.add_enabled(!self.redo_stack.is_empty(), egui::Button::new("\u{27A1}").small()).on_hover_text("Redo (Ctrl+Shift+Z)").clicked() {
                                self.redo();
                            }
                        });
                        self.draw_params_editable(ui);
                    }
                });
            }
        });

        // (F3) персист-граница: единая точка записи конфига на диск, в самом
        // конце кадра — все dirty-маркеры за кадр = одна запись не чаще 2 с.
        self.flush_config(false);
    }
}

impl App {

    fn console_event_pump(&mut self) {
        // Bank-2 console pads -> state machine -> actions
        let mut acts: Vec<led_console::Action> = Vec::new();
        if let Some(rx) = &self.console_rx {
            while let Ok((note, is_on)) = rx.try_recv() {
                let now = std::time::Instant::now();
                let ev = match (note, is_on) {
                    (60..=67, true) => Some(led_console::PadEvent::Down((note - 60) as usize)),
                    (60..=67, false) => Some(led_console::PadEvent::Up((note - 60) as usize)),
                    (75, true) => Some(led_console::PadEvent::UndoDown),
                    (75, false) => Some(led_console::PadEvent::UndoUp),
                    (78, true) => Some(led_console::PadEvent::Prev),
                    (79, true) => Some(led_console::PadEvent::Next),
                    (80, true) => Some(led_console::PadEvent::PageBack),
                    (76, true) => Some(led_console::PadEvent::PageForward),
                    _ => None,
                };
                if let Some(ev) = ev {
                    if let Some(a) = self.console.on_event(ev, now) {
                        acts.push(a);
                    }
                }
            }
        }
        let _ = self.console.tick(std::time::Instant::now()); // hold -> Menu (snapshot below)
        for a in acts {
            self.execute_console_action(a);
        }
        // Смена режима консоли: окно режима открывается на текущем выборе
        // (finding 4: stale-страница при входе; идемпотентно, раз в смену).
        // refresh_console_list здесь же покрывает первый кадр (prev = None)
        // и все изменения perf_list/папок до входа консоли в режим (#57).
        let m = self.console.mode();
        if self.console_prev_mode != Some(m) {
            self.console_prev_mode = Some(m);
            self.refresh_console_list();
            match m {
                led_console::Mode::Pattern => {
                    self.follow_window(self.drum_current_pattern as usize, false);
                }
                led_console::Mode::Tracks => {
                    // Хвост #57: вход показывает страницу активной папки
                    // (page_start сам clamp'ит при 0 папок → 0)
                    self.tracks_window = led_console::page_start(
                        self.config.ui.active_folder.unwrap_or(0),
                        self.config.ui.perf_folders.len());
                    // Подсказка входа (#57 UX): как рулить с пэдов
                    self.show_toast("TRACKS: tap a folder — MENU — назад",
                        ToastKind::Info);
                }
                led_console::Mode::Mute => self.mute_window = 0,
                led_console::Mode::Home => {
                    if let Some(i) = self.active_perf_console() {
                        self.follow_window(i, true);
                    }
                }
                _ => {}
            }
        }
    }
    fn led_companion_lifecycle(&mut self) {
        // Companion process lifecycle (TECHDEBT #16): spawn on toggle-on /
        // first need, kill only OUR child on toggle-off, respawn with 5s
        // backoff if it dies while enabled.
        if !self.config.ui.led_companion {
            if let Some(mut child) = self.led_child.take() {
                log::info!("[console] killing companion child");
                if let Err(e) = child.kill() {
                    log::warn!("[console] companion kill: {e}");
                }
                if let Err(e) = child.wait() {
                    log::warn!("[console] companion wait: {e}");
                }
            }
            self.led_spawn_backoff = None;
        } else {
            // reap exited child (crash, foreign kill, guard-exit 0)
            if let Some(mut child) = self.led_child.take() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        log::info!("[console] companion child exited: {status}");
                        self.led_spawn_backoff = Some(std::time::Instant::now());
                    }
                    Ok(None) => self.led_child = Some(child),
                    Err(e) => log::warn!("[console] companion try_wait: {e}"),
                }
            }
            // Spawn only when nothing connected AND a connect attempt already
            // failed (a live foreign companion serves the socket within the
            // 2s retry window — spawning earlier creates a duplicate that
            // fights the foreign one over BLE).
            let backoff_ok = self.led_spawn_backoff
                .map(|t| t.elapsed() > std::time::Duration::from_secs(5))
                .unwrap_or(true);
            if self.led_child.is_none()
                && self.led_socket.is_none()
                && self.led_socket_failed_at.is_some()
                && backoff_ok
            {
                let path = crate::config::expand_home(&self.config.ui.led_companion_cmd);
                match std::process::Command::new("python3").arg(&path).spawn() {
                    Ok(child) => {
                        log::info!("[console] spawned companion: {}", path.display());
                        self.led_child = Some(child);
                    }
                    Err(e) => {
                        log::warn!("[console] companion spawn failed: {e}");
                        self.led_spawn_backoff = Some(std::time::Instant::now());
                    }
                }
            }
        }
    }
    fn led_socket_lifecycle(&mut self) {
        // LED companion socket lifecycle.
        // Connect runs on a helper thread: a unix connect() BLOCKS when the
        // peer's accept backlog is full — on the GUI thread that froze the
        // whole app before the first frame (observed with a companion stuck
        // in BLE reconnect).
        if !self.config.ui.led_companion {
            if self.led_socket.take().is_some() {
                log::info!("[console] companion disabled");
            }
            self.led_connecting = None;
        } else if self.led_socket.is_none()
            && self.led_socket_failed_at
                .map(|t| t.elapsed() > std::time::Duration::from_secs(2))
                .unwrap_or(true)
            && self.led_connecting.is_none()
        {
            let (tx, rx) =
                std::sync::mpsc::channel::<std::io::Result<std::os::unix::net::UnixStream>>();
            self.led_connecting = Some(rx);
            std::thread::spawn(move || {
                let res = std::os::unix::net::UnixStream::connect("/tmp/mini_midi_synth_led.sock");
                let _ = tx.send(res);
            });
        }
        if self.led_socket.is_none() {
            if let Some(rx) = &mut self.led_connecting {
                match rx.try_recv() {
                    Ok(Ok(stream)) => {
                        self.led_connecting = None;
                        let (tx, rx) = std::sync::mpsc::channel::<String>();
                        // fresh AtomicBool на каждый коннект: старый writer не может
                        // перезаписать флаг нового (spurious reconnect)
                        let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
                        self.led_socket_alive = Some(alive.clone());
                        std::thread::spawn(move || {
                            use std::io::Write;
                            let mut stream = stream;
                            for msg in rx {
                                if stream.write_all(msg.as_bytes()).is_err() {
                                    alive.store(false, std::sync::atomic::Ordering::Relaxed);
                                    break;
                                }
                            }
                            alive.store(false, std::sync::atomic::Ordering::Relaxed);
                        });
                        self.led_socket = Some(tx);
                        self.led_socket_failed_at = None;
                        self.led_dirty = true; // отправить полный STATE сразу после коннекта
                        log::info!("[console] companion connected");
                    }
                    Ok(Err(_)) => {
                        self.led_connecting = None;
                        self.led_socket_failed_at = Some(std::time::Instant::now());
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.led_connecting = None;
                        self.led_socket_failed_at = Some(std::time::Instant::now());
                    }
                }
            }
        }
        // writer умер (компаньон перезапускается) — забыть сокет, retry через 2с
        let socket_dead = match &self.led_socket_alive {
            Some(a) => !a.load(std::sync::atomic::Ordering::Relaxed),
            None => false,
        };
        if self.led_socket.is_some() && socket_dead {
            self.led_socket = None;
            self.led_socket_alive = None;
            self.led_socket_failed_at = Some(std::time::Instant::now());
        }
    }
    fn led_snapshot_send(&mut self) {
        // LED snapshot (16 slots = pads 17..=32, top row first)
        {
            // Bottom row shows the MASTER transport (single transport, #32
            // фаза 4): the union — LED reflects "something runs".
            let lstate = crate::synth::looper::LooperState::from_u8(
                self.looper_atoms.state.load(Ordering::Relaxed));
            let (playing, recording) = (
                self.drum_playing
                    || matches!(lstate, crate::synth::looper::LooperState::Playing
                            | crate::synth::looper::LooperState::Overdubbing
                            | crate::synth::looper::LooperState::Recording),
                self.drum_recording
                    || matches!(lstate, crate::synth::looper::LooperState::Recording
                            | crate::synth::looper::LooperState::Overdubbing),
            );
            let mut mutes = [false; 16];
            for (i, part) in self.parts.iter().enumerate().take(16) {
                mutes[i] = part.mute;
            }
            let filled = self.console_active_list().len();
            let undo_avail = self.drum_undo_atom.load(Ordering::Relaxed);
            let redo_avail = self.drum_redo_atom.load(Ordering::Relaxed);
            let page_depth = match self.console.mode() {
                led_console::Mode::Pattern => self.pat_window.min(led_console::window_max_start(8)),
                led_console::Mode::Mute => self.mute_window / 7,
                led_console::Mode::Tracks => self.tracks_window / 7,
                // FolderPerfs: PAGE-глубина по перф-страницам — явное плечо
                // (то же, что `_`; самодокументация)
                led_console::Mode::FolderPerfs => self.perf_window / 7,
                _ => self.perf_window / 7,
            };
            let top = match self.console.mode() {
                led_console::Mode::Tracks => self.console.folder_colors(
                    self.config.ui.active_folder,
                    self.config.ui.perf_folders.len(), self.tracks_window),
                led_console::Mode::FolderPerfs => self.console.top_colors(
                    self.active_perf_console(), self.folder_console_list.len(),
                    self.drum_current_pattern as usize, &mutes,
                    self.perf_window, self.pat_window, self.mute_window),
                _ => self.console.top_colors(self.active_perf_console(), filled,
                                             self.drum_current_pattern as usize, &mutes,
                                             self.perf_window, self.pat_window,
                                             self.mute_window),
            };
            let bottom = self.console.bottom_colors(playing, recording, page_depth,
                                                    undo_avail, redo_avail);
            let mut snap = [[0u8; 3]; 16];
            snap[..8].copy_from_slice(&top);
            snap[8..].copy_from_slice(&bottom);
            // Send on change, plus a 2s heartbeat: heals dropped writes and
            // reconnects after a companion restart (otherwise nothing is sent
            // while the state is idle and the console shows stale colors).
            let heartbeat_due = self.led_last_send
                .map(|t| t.elapsed() > std::time::Duration::from_secs(2))
                .unwrap_or(true);
            if snap != self.led_snapshot || self.led_dirty || heartbeat_due {
                self.led_snapshot = snap;
                self.led_dirty = false;
                self.led_last_send = Some(std::time::Instant::now());
                if let Some(sock) = &self.led_socket {
                    let mut msg = String::from("STATE");
                    for c in &snap {
                        msg.push_str(&format!(" {:02X}{:02X}{:02X}", c[0], c[1], c[2]));
                    }
                    msg.push('\n');
                    let _ = sock.send(msg);
                }
            }
        }
    }
    /// Запомнить «что сейчас крутят» — для индикатора в правом углу топ-бара
    /// (#39/#44: значение видно весь drag, тултип не обязателен). Каждый тик
    /// сбрасывает таймер авто-скрытия (2 с, как OSD volume).
    pub(crate) fn knob_feedback(&mut self, r: &egui::Response, name: &str, value: String) {
        // Только ДЕЙСТВИЕ (drag/изменение): простое проведение мыши над
        // слайдером индикатор не зажигает.
        if r.dragged() || r.changed() {
            self.active_knob = Some((name.to_string(), value, std::time::Instant::now()));
        }
    }


    /// Send initial patches to all parts at startup
    pub fn send_initial_presets(&mut self) {
        // Restore persisted live toggles (mute / root-only) — survives restarts
        for (i, st) in self.config.ui.parts_state.iter().enumerate() {
            if let Some(part) = self.parts.get_mut(i) {
                part.mute = st.mute;
                part.note_mode = st.note_mode;
                part.sustain_enabled = st.sustain_enabled;
                part.ignore_pb = st.ignore_pb;
                part.ignore_mw = st.ignore_mw;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part: i, mute: st.mute });
                let _ = self.ctrl_tx.push(ControlEvent::SetPartNoteMode { part: i, mode: st.note_mode });
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSustain { part: i, on: st.sustain_enabled });
                let _ = self.ctrl_tx.push(ControlEvent::SetPartIgnorePb { part: i, on: st.ignore_pb });
                let _ = self.ctrl_tx.push(ControlEvent::SetPartIgnoreMw { part: i, on: st.ignore_mw });
            }
        }
        for i in 0..self.parts.len() {
            self.send_edited_params(i);
        }
        // Send global params (volume, tone) to engine
        for (key, val) in &self.global_params {
            if let Some(static_key) = crate::cc_map::resolve_key(key) {
                let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: static_key, value: *val });
            }
        }
        // Send drum and part volumes to engine
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetVolume { volume: self.drum_volume });
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetFixedVelocity { fixed: self.drum_fixed_velocity });
        let _ = self.ctrl_tx.push(ControlEvent::SetSustainRelease { seconds: self.sustain_release });
        let _ = self.ctrl_tx.push(ControlEvent::SetSf2ReleaseScale { scale: self.sf2_release_scale });
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetMetronome { on: self.drum_metronome });
        // Per-engine loudness trims (Settings → Audio, persisted in config.audio)
        let _ = self.ctrl_tx.push(ControlEvent::SetDspTrim { db: self.config.audio.dsp_trim_db });
        let _ = self.ctrl_tx.push(ControlEvent::SetSf2Trim { db: self.config.audio.sf2_trim_db });
        // Restore the internal-kit remap before the initial push (if an SF2 kit is
        // configured, load_sf2_drums below overwrites this with the per-kit table).
        if self.sf2_drums_loaded_name.is_empty() {
            if let Some(v) = self.config.sf2.drums_remaps.get("internal") {
                // migrate legacy slot-encoded values (0..16) to kit notes (+36)
                let v: Vec<u8> = v.iter().map(|&x| if x < 16 { x + 36 } else { x }).collect();
                let mut t = [0u8; 16];
                for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or((36 + i) as u8).clamp(30, 55); }
                self.drum_remap = t;
            }
            if let Some(v) = self.config.sf2.drums_vels.get("internal") {
                let mut t = [0u8; 16];
                for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or(0).min(127); }
                self.drum_pad_vel = t;
            }
        }
        self.push_drum_remap();
        self.push_drum_pad_vel();
        for i in 0..self.parts.len() {
            self.send_part_volume(i);
        }
        // Ensure config has global params persisted (first run migration)
        self.save_global_to_config();

        // SF2 sample offset
        if self.sf2_sample_offset_ms > 0.0 {
            let _ = self.ctrl_tx.push(ControlEvent::SetSf2SampleOffset { ms: self.sf2_sample_offset_ms });
        }

        // Load SF2 from config if set
        // Keys SF2 (fallback to legacy file_path)
        let keys_path = self.config.sf2.keys_file_path.clone()
            .or_else(|| self.config.sf2.file_path.clone());
        if let Some(sf2_path) = keys_path {
            let idx = self.find_or_add_sf2(&sf2_path);
            if let Some(idx) = idx {
                self.sf2_keys_selected = Some(idx);
                self.load_sf2_keys(idx);
            }
        }
        // Drums SF2
        if let Some(sf2_path) = self.config.sf2.drums_file_path.clone() {
            let idx = self.find_or_add_sf2(&sf2_path);
            if let Some(idx) = idx {
                self.sf2_drums_selected = Some(idx);
                self.load_sf2_drums(idx);
            }
        }

        // Restore last performance (after SF2 load — part sf2_program applies
        // to the loaded soundfont)
        if let Some(name) = self.config.ui.last_performance.clone() {
            if self.load_performance_by_name(&name) {
                log::info!("[init] performance restored: {name}");
            }
        }
    }

    /// (F3) единая точка записи конфига на диск: dirty-маркер + дебаунс 2 с.
    /// force=true — форс-запись (выход приложения).
    fn flush_config(&mut self, force: bool) {
        if !self.config_dirty {
            return;
        }
        let due = self.last_config_save.elapsed().as_secs_f32() >= 2.0;
        if force || due {
            if let Err(e) = self.config.save() {
                log::warn!("[config] flush failed ({e}) — will retry after next change");
            }
            self.last_config_save = std::time::Instant::now();
            self.config_dirty = false;
        }
    }

    /// (F3) конфиг изменился в памяти — дисковая запись отложена до flush_config.
    fn mark_config_dirty(&mut self) {
        self.config_dirty = true;
    }

    fn save_global_to_config(&mut self) {
        self.config.ui.master_volume = self.global_params.get("master_volume").copied().unwrap_or(0.8);
        self.config.ui.master_tone = self.global_params.get("master_tone").copied().unwrap_or(20000.0);
        self.config.ui.fader_reverb = self.global_params.get("reverb_mix").copied().unwrap_or(0.0);
        self.config.ui.fader_delay = self.global_params.get("delay_mix").copied().unwrap_or(0.0);
        self.config.ui.pitch_bend_range = self.global_params.get("pitch_bend_range").copied().unwrap_or(2.0) as u8;
        self.config.ui.drum_volume = self.drum_volume;
        self.config.ui.drum_fixed_velocity = self.drum_fixed_velocity;
        self.config.ui.sustain_release = self.sustain_release;
        self.config.ui.sf2_release_scale = self.sf2_release_scale;
        self.config.ui.drum_metronome = self.drum_metronome;
        if let Some(l) = self.parts.first() { self.config.ui.layer_a_volume = l.volume; }
        if let Some(l) = self.parts.get(1) { self.config.ui.layer_b_volume = l.volume; }
        self.config.ui.pad_perf_map = self.pad_perf_map.to_vec();
        self.config.ui.split_point = Some(self.split_point);
        self.config.ui.keybinds = self.keybinds.to_config();
        self.mark_config_dirty();
    }

    /// Load edited_params from the current patch for a given part.
    /// Preserves global params (effects, volume) so they don't reset on patch change.
    pub fn load_edited_params(&mut self, part: usize) {
        let patch_idx = self.parts[part].patch_idx;
        if let Some(p) = self.patches.get(patch_idx) {
            self.parts[part].edited_params = p.params.clone();
        }
        // Global params (volume, tone) are NOT in presets — don't insert them
        self.parts[part].params_dirty = false;
    }

    /// Send edited params to synth engine for a given part
    fn process_keybinds(&mut self, ctx: &egui::Context) {
        use keybinds::KeyAction;
        // Collect triggered actions to avoid borrow conflict.
        // repeat: false — авто-повтор не должен триггерить хоткеи
        // (иначе удержание X обходило бы 2s-confirm лупера).
        let triggered: Vec<KeyAction> = self.keybinds.binds().iter()
            .filter(|(key, _)| {
                ctx.input(|i| {
                    i.events.iter().any(|e| matches!(e,
                        egui::Event::Key { key: k, pressed: true, repeat: false, .. } if k == key))
                })
            })
            .map(|(_, action)| action.clone())
            .collect();
        for action in triggered {
            match &action {
                KeyAction::SwitchPart(p) => {
                    let p = *p as usize;
                    if p < self.parts.len() && self.parts[p].enabled {
                        self.set_active_part(p);
                        self.show_drums = false;
                        self.show_midi_seq = false;
                        self.show_fx_chain = false;
                        self.seq_target_atom.store(2, Ordering::Relaxed);
                    }
                }
                KeyAction::LooperRecord => {
                    let _ = self.ctrl_tx.push(ControlEvent::LooperRecord);
                }
                KeyAction::LooperTogglePlay => {
                    let _ = self.ctrl_tx.push(ControlEvent::LooperTogglePlay);
                }
                KeyAction::LooperUndo => {
                    let _ = self.ctrl_tx.push(ControlEvent::LooperUndo);
                }
                KeyAction::LooperClear => {
                    // Тот же 2s-confirm, что у кнопки Clear (audit High п.2):
                    // первое нажатие взводит, второе в течение окна — очищает.
                    let armed = matches!(self.looper_clear_confirm, Some(t)
                        if t.elapsed() < confirm::CONFIRM_WINDOW);
                    if armed {
                        let _ = self.ctrl_tx.push(ControlEvent::LooperClear);
                        self.looper_clear_confirm = None;
                    } else {
                        self.looper_clear_confirm = Some(Instant::now());
                    }
                }
                KeyAction::DrumTogglePlay => {
                    self.drum_playing = !self.drum_playing;
                    let _ = self.ctrl_tx.push(ControlEvent::DrumSeqPlay { playing: self.drum_playing });
                }
                KeyAction::ToggleDrumsSynth => {
                    self.show_drums = !self.show_drums;
                    self.show_looper = false;
                    if !self.show_drums {
                        self.seq_target_atom.store(1, Ordering::Relaxed);
                    } else {
                        self.seq_target_atom.store(0, Ordering::Relaxed);
                    }
                }
                KeyAction::PrevPreset => {
                    let part = self.active_part;
                    let idx = self.parts[part].patch_idx;
                    if idx > 0 {
                        let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                        self.parts[part].patch_idx = idx - 1;
                        self.load_edited_params(part);
                        self.send_edited_params(part);
                        self.snapshot_preset_and_mark_dirty();
                    }
                }
                KeyAction::NextPreset => {
                    let part = self.active_part;
                    let idx = self.parts[part].patch_idx;
                    if idx + 1 < self.patches.len() {
                        let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                        self.parts[part].patch_idx = idx + 1;
                        self.load_edited_params(part);
                        self.send_edited_params(part);
                        self.snapshot_preset_and_mark_dirty();
                    }
                }
                KeyAction::TogglePadPerf => {
                    self.show_pad_perf = !self.show_pad_perf;
                }
            }
        }
    }

    pub(super) fn draw_keybinds_window(&mut self, ctx: &egui::Context) {
        use keybinds::{KeyAction, key_to_str};
        let mut open = self.show_keybinds_window;
        // Обёртка modal::ModalWindow. esc_closes(false): у окна внутренний
        // «временный режим» (capture клавиши), Esc-стек — свой, в теле:
        // 1-й Esc отменяет capture, 2-й закрывает окно.
        // esc_close — флаг внутри тела (win_open занят borrow'ом шелла),
        // применяется после .show().
        let mut esc_close = false;
        modal::ModalWindow::new(ctx, "keybindings", "Keybindings", &mut open, 480.0)
            .esc_closes(false)
            .footer_done(false)
            .resizable(false)
            .show(|ui| {
                // Esc-стек — ПЕРВАЯ строка тела: update-loop уже прошёл, окно
                // рисуется после, иначе первое нажатие сделало бы оба шага.
                let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                if esc {
                    if self.keybind_capturing.is_some() {
                        self.keybind_capturing = None; // 1-й Esc: отменить capture
                    } else {
                        esc_close = true;              // 2-й Esc: закрыть окно
                    }
                }
                egui::Grid::new("keybinds_grid").striped(true).show(ui, |ui| {
                    ui.strong("Action");
                    ui.strong("Key");
                    ui.strong("");
                    ui.end_row();

                    for action in KeyAction::all() {
                        ui.label(action.label());
                        let key_label = self.keybinds.key_for_action(&action)
                            .map(key_to_str)
                            .unwrap_or("---");
                        let is_capturing = self.keybind_capturing.as_ref() == Some(&action);
                        let btn_text = if is_capturing { "Press key..." } else { key_label };
                        if ui.button(btn_text)
                            .on_hover_text("Press Escape to cancel")
                            .clicked()
                        {
                            self.keybind_capturing = Some(action.clone());
                        }
                        // Unbind — тот же 2s-confirm, взвод на конкретное действие.
                        // Expire через is_some_and: нельзя мутировать поле под &borrow.
                        if self.unbind_confirm.as_ref().is_some_and(|(_, t)| t.elapsed() >= confirm::CONFIRM_WINDOW) {
                            self.unbind_confirm = None;
                        }
                        let armed = matches!(&self.unbind_confirm, Some((a, _)) if a == &action);
                        let ub_color = if armed { egui::Color32::RED } else { ui.visuals().text_color() };
                        let ub_btn = egui::Button::new(
                            egui::RichText::new(if armed { "Sure?" } else { "×" }).color(ub_color),
                        ).small();
                        if ui.add(ub_btn).on_hover_text("Unbind").clicked() {
                            if armed {
                                if let Some(key) = self.keybinds.key_for_action(&action) {
                                    self.keybinds.binds_mut().retain(|(k, _)| *k != key);
                                    self.config.ui.keybinds = self.keybinds.to_config();
                                    self.mark_config_dirty();
                                }
                                self.unbind_confirm = None;
                            } else {
                                self.unbind_confirm = Some((action.clone(), Instant::now()));
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(theme::SP_SM);
                if confirm::confirm_button(ui, "Reset Defaults", &mut self.keybinds_reset_confirm, true, "Restore default keybindings (custom binds are lost)") {
                    self.keybinds = keybinds::Keybinds::defaults();
                    self.config.ui.keybinds = self.keybinds.to_config();
                    self.mark_config_dirty();
                    self.keybind_capturing = None;
                }
            });
        if esc_close {
            open = false;
        }
        if !open {
            // Закрытие окна любым путём (Esc/✕) при активном capture —
            // не оставлять capture висеть (Negative Matrix плана).
            self.keybind_capturing = None;
        }
        self.show_keybinds_window = open;
    }

    /// Push a toast notification.
    pub fn show_toast(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.toasts.push(Toast {
            message: message.into(),
            kind,
            created: Instant::now(),
        });
    }

    /// Maximum undo depth for patch parameter editing.
    const MAX_UNDO: usize = 50;

    /// Push a snapshot of the active part's live state onto the undo stack.
    /// Safe to call multiple times — only pushes if state differs from top of stack.
    pub(super) fn push_undo_snapshot(&mut self) {
        let part = self.active_part;
        self.push_undo_snapshot_for(part);
    }

    /// Same, for an ARBITRARY part: row widgets (mute/pan/transpose) and the
    /// pad console act on parts that are not the active one (#31).
    pub(super) fn push_undo_snapshot_for(&mut self, part: usize) {
        let snap = PartUndoSnapshot::capture(part, &self.parts[part]);
        part_undo_push(&mut self.undo_stack, &mut self.redo_stack, snap);
    }

    fn undo(&mut self) {
        // The redo entry must hold the current state of the SAME part the
        // popped snapshot belongs to — the stack is shared across parts.
        let Some(top_part) = self.undo_stack.back().map(|s| s.part) else { return };
        let current = PartUndoSnapshot::capture(top_part, &self.parts[top_part]);
        if let Some(snapshot) = part_undo_step(&mut self.undo_stack, &mut self.redo_stack, current) {
            self.restore_part_snapshot(snapshot);
            self.show_toast("Undo", ToastKind::Info);
        }
    }

    fn redo(&mut self) {
        let Some(top_part) = self.redo_stack.last().map(|s| s.part) else { return };
        let current = PartUndoSnapshot::capture(top_part, &self.parts[top_part]);
        if let Some(snapshot) = part_redo_step(&mut self.undo_stack, &mut self.redo_stack, current) {
            self.restore_part_snapshot(snapshot);
            self.show_toast("Redo", ToastKind::Info);
        }
    }

    /// Apply a popped snapshot to its part and re-sync everything downstream:
    /// engine control events are inlined here — no mute/pan/transpose helper
    /// exists to call (#31 review) — plus patch params, persistence and the
    /// active-part switch so follow-up edits land on the restored part.
    fn restore_part_snapshot(&mut self, s: PartUndoSnapshot) {
        let PartUndoSnapshot { part, edited_params, patch_idx, mute, pan, transpose } = s;
        {
            let p = &mut self.parts[part];
            p.edited_params = edited_params;
            p.patch_idx = patch_idx;
            p.mute = mute;
            p.pan = pan;
            p.transpose = transpose;
            // #49: undo откатывает и macro_{i} в edited_params — зеркало
            // обязано последовать, иначе последующий push перезапишет откат
            p.macro_vals = macro_vals_from_params(&p.edited_params);
        }
        let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part, mute });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part, pan });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part, semitones: transpose });
        self.send_edited_params(part);
        self.persist_parts_state();
        // #49: last_preset зеркалит parts[0]; undo патча парта 0 иначе
        // оставляет конфиг на до-undo патче — рестарт вернёт откаченное
        if part == 0 {
            self.snapshot_preset_and_mark_dirty();
        }
        self.set_active_part(part);
    }

    fn send_edited_params(&mut self, part: usize) {
        let original = self.patches.get(self.parts[part].patch_idx);
        let _ = self.ctrl_tx.push(ControlEvent::load_patch_from_edited(
            part, &self.parts[part].edited_params, original,
        ));
    }

    fn send_part_volume(&mut self, part: usize) {
        let volume = self.parts[part].volume;
        let _ = self.ctrl_tx.push(ControlEvent::SetPartVolume { part, volume });
    }

    /// Pushes every per-part setting that is not carried by the patch itself.
    ///
    /// Needed after an audio reconnect, where the engine is brand new and
    /// knows nothing about the GUI's state: without this the fresh engine has
    /// default ranges, no panning, no transposition and no SF2 assignments.
    pub(super) fn send_full_part_state(&mut self) {
        for i in 0..self.parts.len() {
            self.push_full_part_state(i);
        }
    }

    /// Пуш полного состояния ОДНОГО парта в движок (enabled + все параметры).
    /// Используется при спавне/включении парта и при аудио-реконнекте.
    fn push_full_part_state(&mut self, part: usize) {
        // ST2: 1 слот кольца вместо 13-21 события. Патч не входит (None) —
        // его несёт send_edited_params отдельным heap-событием, как раньше.
        let p = &self.parts[part];
        let state = crate::synth::PartFullState {
            enabled: p.enabled, mute: p.mute, volume: p.volume,
            min_note: p.min_note, max_note: p.max_note,
            vel_min: p.vel_min, vel_max: p.vel_max,
            pan: p.pan, transpose: p.transpose,
            note_mode: p.note_mode,
            sustain_enabled: p.sustain_enabled,
            ignore_pb: p.ignore_pb, ignore_mw: p.ignore_mw,
            sf2_mode: p.sf2_mode, sf2_program: p.sf2_program,
            macro_vals: p.macro_vals,
            patch: None,
        };
        let _ = self.ctrl_tx.push(ControlEvent::SetPartFull { part, state: Box::new(state) });
    }

    /// Сброс GUI-слота в дефолты + полный пуш в движок (фикс #40-43a:
    /// прежние add_part_to_zone сбрасывал 5 полей из 11 и не пушил
    /// sf2_mode/pan/transpose/vel_range; send_edited_params уходил БЕЗ
    /// load_edited_params — движок получал параметры прежнего жильца).
    fn reset_and_push_part(&mut self, part: usize, enabled: bool) {
        let p = &mut self.parts[part];
        p.mute = false;
        p.volume = 0.8;
        p.patch_idx = 0;
        p.sf2_mode = false;
        p.sf2_program = 0;
        p.pan = 0.0;
        p.transpose = 0;
        p.vel_min = 1;
        p.vel_max = 127;
        p.note_mode = crate::synth::NOTE_MODE_POLY;
        p.ignore_pb = false;
        p.ignore_mw = false;
        p.enabled = enabled;
        self.load_edited_params(part);   // GUI: параметры патча 0
        self.send_edited_params(part);   // Движок: реальные параметры патча 0
                                         // (фикс #43a: раньше уходили параметры
                                         // прежнего жильца или Default)
        self.push_full_part_state(part);
    }

    /// Свап содержимого двух слотов одной зоны (#54): GUI-структуры,
    /// движок обоих слотов, активный парт, персист. Undo НЕТ (осознанно:
    /// одно-слотовый канон snapshot; свап обратим повторным свапом).
    fn swap_parts(&mut self, a: usize, b: usize) {
        if swap_check(a, b, crate::gui::PARTS_PER_ZONE).is_err() { return; }
        self.parts.swap(a, b);
        // active_part едет с партом (юзер продолжает править тот же звук).
        // ЧЕРЕЗ set_active_part — он синхронизирует и active_part_atom
        // (drums UI zone labels, headless display)
        if self.active_part == a { self.set_active_part(b); }
        else if self.active_part == b { self.set_active_part(a); }
        self.do_swap_parts_push(a, b);
    }

    fn do_swap_parts_push(&mut self, a: usize, b: usize) {
        // ST2: свап = 2 транзакции с патчем внутри (было 28-44 события;
        // разрыв кольца больше не даёт half-swap). Порядок «state потом patch»
        // внутри одной транзакции = прежний порядок «full state, потом
        // LoadPatch» — GUI-порядок сохранён. MIDI-ноты, игравшие в момент
        // свапа, доигрывают в старом слоте (их перенос — out of scope).
        for part in [a, b] {
            let p = &self.parts[part];
            let patch = ControlEvent::load_patch_from_edited(
                part, &p.edited_params,
                self.patches.get(p.patch_idx),
            );
            let patch = match patch {
                ControlEvent::LoadPatch(boxed) => Some(boxed),
                _ => unreachable!(),
            };
            let state = crate::synth::PartFullState {
                enabled: p.enabled, mute: p.mute, volume: p.volume,
                min_note: p.min_note, max_note: p.max_note,
                vel_min: p.vel_min, vel_max: p.vel_max,
                pan: p.pan, transpose: p.transpose,
                note_mode: p.note_mode,
                sustain_enabled: p.sustain_enabled,
                ignore_pb: p.ignore_pb, ignore_mw: p.ignore_mw,
                sf2_mode: p.sf2_mode, sf2_program: p.sf2_program,
                macro_vals: p.macro_vals,
                patch,
            };
            let _ = self.ctrl_tx.push(ControlEvent::SetPartFull { part, state: Box::new(state) });
        }
        self.persist_parts_state();
    }

    /// Re-sends the MIDI CC mapping to a freshly created engine.
    pub(super) fn send_cc_map(&mut self) {
        let _ = self.ctrl_tx.push(ControlEvent::SetCcMap { map: Box::new(self.cc_map) });
    }

    /// Config key for the drum remap table: kit name, or "internal" kit.
    /// Derive SF2 choke groups from the kit's sample names: hat-ish sounds ->
    /// group 1 (mutual), crash/cymbal -> group 2, everything else 0.
    fn derive_choke_groups(&self) -> [u8; 26] {
        let mut t = [0u8; 26];
        for (i, name) in self.drum_kit_names.iter().enumerate() {
            let n = name.to_lowercase();
            if n.contains("hat") || n.contains("hh") {
                t[i] = 1;
            } else if n.contains("crash") || n.contains("cymbal") {
                t[i] = 2;
            }
        }
        t
    }

    fn push_drum_choke_groups(&mut self) {
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetChokeGroups {
            table: Box::new(self.derive_choke_groups()),
        });
    }

    fn drum_remap_key(&self) -> String {
        if self.sf2_drums_loaded_name.is_empty() { "internal".to_string() }
        else { self.sf2_drums_loaded_name.clone() }
    }

    /// Push the current remap table to the engine.
    fn push_drum_remap(&mut self) {
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetRemap { table: Box::new(self.drum_remap) });
    }

    /// Persist the current remap table under the active kit key.
    /// Called from the remap panel in drums.rs.
    fn save_drum_remap(&mut self) {
        let key = self.drum_remap_key();
        self.config.sf2.drums_remaps.insert(key, self.drum_remap.to_vec());
        self.mark_config_dirty();
    }

    fn push_drum_pad_vel(&mut self) {
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetPadVel { table: Box::new(self.drum_pad_vel) });
    }

    /// Wired up by the pad-vel panel in drums.rs.
    fn save_drum_pad_vel(&mut self) {
        let key = self.drum_remap_key();
        self.config.sf2.drums_vels.insert(key, self.drum_pad_vel.to_vec());
        self.mark_config_dirty();
    }

    /// Re-sends the drum kit, tempo and pattern selection to a fresh engine.
    pub(super) fn send_drum_state(&mut self) {
        let _ = self.ctrl_tx.push(ControlEvent::DrumLoadKit {
            patterns: Box::new(self.drum_patterns),
            params: Box::new(self.drum_params),
            bpm: self.drum_bpm,
            swing: self.drum_swing,
            volume: self.drum_volume,
        });
        let _ = self.ctrl_tx.push(ControlEvent::DrumSeqPattern { pattern: self.drum_current_pattern });
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetFixedVelocity { fixed: self.drum_fixed_velocity });
        self.push_drum_remap();
        self.push_drum_pad_vel();
        let _ = self.ctrl_tx.push(ControlEvent::LooperSetBars { bars: self.looper_bars });
        let _ = self.ctrl_tx.push(ControlEvent::LooperSetQuantize { quantize: self.looper_quantize });
        // Choke semantics survive reconnect even if the SF2 restore path bails
        // (missing file). For the internal kit (no SF2 loaded) this pushes a
        // derived table over the engine's GM default — harmless: `sf2_choke_take`
        // is only called in the SF2 branch, the DSP path uses the hardcoded
        // CHOKE_GROUPS, and load_sf2_drums overwrites it for SF2 kits.
        self.push_drum_choke_groups();
    }

    fn set_active_part(&mut self, part: usize) {
        self.active_part = part;
        self.active_part_atom.store(part as u8, std::sync::atomic::Ordering::Relaxed);
    }

    fn send_part_range(&mut self, part: usize) {
        let min_note = self.parts[part].min_note;
        let max_note = self.parts[part].max_note;
        let _ = self.ctrl_tx.push(ControlEvent::SetPartRange { part, min_note, max_note });
    }

    /// Snapshot last_preset + mark dirty (disk write via flush_config).
    fn snapshot_preset_and_mark_dirty(&mut self) {
        self.config.ui.last_preset = self
            .patches
            .get(self.parts[0].patch_idx)
            .map(|p| p.name.clone());
        self.mark_config_dirty();
    }

    /// Execute an action from the bank-2 pad console state machine.
    fn execute_console_action(&mut self, action: led_console::Action) {
        match action {
            led_console::Action::LoadPerf(i) => {
                let idx = self.perf_window + i - 1; // slot -> real index
                let list = self.console_active_list().clone();
                match list.get(idx) {
                    Some((name, Some(_))) => {
                        let name = name.clone();
                        if self.load_performance_by_name(&name) {
                            log::info!("[console] perf slot {} loaded ({})", i, name);
                            self.follow_window(idx, true);
                        }
                    }
                    _ => {
                        // #57 tombstone: битая ссылка папки — не молчать
                        log::warn!("[console] perf slot {}: missing perf (tombstone)", i);
                        self.show_toast("Perf missing — edit Tracks", ToastKind::Error);
                    }
                }
            }
            led_console::Action::TrackSelect(i) => {
                // Тап папки в TRACKS: папка становится активной, консоль
                // входит в её перфы (Mode::FolderPerfs ставит led_console::tap)
                let idx = self.tracks_window + i;
                if idx < self.config.ui.perf_folders.len() {
                    self.config.ui.active_folder = Some(idx);
                    self.mark_config_dirty();
                    self.refresh_console_list();
                    self.perf_window = 0; // TC8: перф-страница с начала
                }
            }
            led_console::Action::SelectPattern(i) => {
                // консоль шлёт УЖЕ window-local 0-based (slot-1 из tap())
                let idx = self.pat_window + i;
                if idx < 8 {
                    let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqPattern { pattern: idx as u8 });
                    self.follow_window(idx, false);
                }
            }
            led_console::Action::ToggleMute(i) => {
                let part = self.mute_window + i; // i уже 0-based (slot-1 из консоли) // slot -> real part
                let mute = self.parts.get(part).map(|p| !p.mute);
                if let Some(mute) = mute {
                    self.push_undo_snapshot_for(part); // #31: снимок ДО изменения
                    self.parts[part].mute = mute;
                    let _ = self.ctrl_tx.push(crate::synth::ControlEvent::SetPartMute { part, mute });
                    self.persist_parts_state();
                }
            }
            led_console::Action::Undo => {
                let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqUndo);
            }
            led_console::Action::Redo => {
                let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqRedo);
            }
            led_console::Action::StepPerf(dir) => {
                let list = self.console_active_list().clone();
                let n = list.len();
                if n == 0 { return; }
                let cur = self.active_perf_console().unwrap_or(0);
                // #57: tombstone-слоты пропускаем (warn, канон warn-on-press),
                // максимум n попыток; follow — одним вызовом после цикла
                let mut next = cur;
                for _ in 0..n {
                    next = (next as i32 + dir).rem_euclid(n as i32) as usize;
                    match list.get(next) {
                        None => break,
                        Some((name, None)) => {
                            log::warn!("[console] step: перф '{name}' не найден (tombstone)");
                        }
                        Some((name, Some(_))) => {
                            let name = name.clone();
                            self.load_performance_by_name(&name);
                            break;
                        }
                    }
                }
                // follow снапится на границу страницы (#50-фикс) — окно
                // больше не уходит в off-grid
                self.follow_window(next, true);
            }
            led_console::Action::StepPattern(dir) => {
                let next = (self.drum_current_pattern as i32 + dir).rem_euclid(8);
                let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqPattern { pattern: next as u8 });
                // follow снапится на границу страницы — degenerate window 7
                // больше не возникает (#50-фикс)
                self.follow_window(next as usize, false);
            }
            led_console::Action::PageBack => {
                // ЧИСТОЕ листание: follow после paging притягивал окно к
                // активному перфу — стрелки становились no-op/реверсом
                // (аудит #50: «не возвращается»; follow остаётся только в
                // mode-entry sync)
                self.page_window(-7);
            }
            led_console::Action::PageForward => {
                self.page_window(7);
            }
        }
    }

    /// Auto-follow: keep the active index inside [w, w+7); if it escapes the
    /// window, the start is clamped to the last LEGAL page (never a degenerate
    /// one — regression #50 finding 2).
    fn follow_window(&mut self, idx: usize, is_perf: bool) {
        if is_perf {
            let n = self.console_active_list().len();
            if let Some(w) =
                led_console::follow_start(idx, self.perf_window, n)
            {
                self.perf_window = w;
            }
        } else if let Some(w) = led_console::follow_start(idx, self.pat_window, 8) {
            self.pat_window = w;
        }
    }

    /// Shift the active mode's window by `delta` slots, clamped to the list.
    fn page_window(&mut self, delta: i32) {
        match self.console.mode() {
            led_console::Mode::Home => {
                let max_w = led_console::window_max_start(self.console_list.len());
                self.perf_window = (self.perf_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            led_console::Mode::Pattern => {
                let max_w = led_console::window_max_start(8); // = 1
                self.pat_window = (self.pat_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            led_console::Mode::Mute => {
                let max_w = led_console::window_max_start(self.parts.len());
                self.mute_window = (self.mute_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            led_console::Mode::Tracks => {
                let max_w = led_console::window_max_start(
                    self.config.ui.perf_folders.len());
                self.tracks_window =
                    (self.tracks_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            led_console::Mode::FolderPerfs => {
                let max_w = led_console::window_max_start(self.folder_console_list.len());
                self.perf_window = (self.perf_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            _ => {}
        }
    }

    /// Persist live per-part toggles (mute / note mode) to config so they
    /// survive restarts (they are NOT part of performance files' semantics).
    pub fn persist_parts_state(&mut self) {
        self.config.ui.parts_state = self
            .parts
            .iter()
            .map(|p| crate::config::PartUiState {
                mute: p.mute, note_mode: p.note_mode,
                root_only: false, // legacy: больше не пишется (skip_serializing)
                sustain_enabled: p.sustain_enabled,
                ignore_pb: p.ignore_pb,
                ignore_mw: p.ignore_mw,
            })
            .collect();
        self.save_global_to_config();
    }

    /// Пересчёт консольного списка перфов (#57): папка или плоский список.
    /// Также логирует tombstones (warn) — один раз на refresh.
    fn refresh_console_list(&mut self) {
        // HOME-ряд = ПОЛНЫЙ список перфов ВСЕГДА (юзер 2026-11-08: трек не
        // фильтрует синее меню; фильтр — только FolderPerfs из TRACKS).
        self.console_list = self.perf_list.iter()
            .map(|(n, p)| (n.clone(), Some(p.clone())))
            .collect();
        let folder = self.config.ui.active_folder
            .and_then(|i| self.config.ui.perf_folders.get(i));
        self.folder_console_list = match folder {
            Some(f) => crate::config::resolve_folder_entries(&self.perf_list, Some(f)),
            None => Vec::new(),
        };
        for (name, path) in &self.folder_console_list {
            if path.is_none() {
                log::warn!("[console] папка-трек: перф '{name}' не найден (tombstone)");
            }
        }
    }

    /// Список, активный для ТЕКУЩЕГО режима консоли: FolderPerfs = алиасы
    /// папки, остальное (HOME) = полный список перфов.
    fn console_active_list(&self) -> &Vec<(String, Option<std::path::PathBuf>)> {
        if self.console.mode() == led_console::Mode::FolderPerfs {
            &self.folder_console_list
        } else {
            &self.console_list
        }
    }

    /// Index of the current performance in console_list (None if unknown;
    /// tombstone-active → None — корректно).
    fn active_perf_console(&self) -> Option<usize> {
        self.console_active_list().iter().position(|(n, _)| n == &self.perf_name)
    }

    fn save_collapsed_categories(&mut self) {
        self.config.ui.collapsed_categories = self.collapsed_categories.iter().cloned().collect();
        self.mark_config_dirty();
    }

    fn refresh_midi_ports(&mut self) {
        self.midi_port_names = midi::list_ports().unwrap_or_default();
    }

    #[cfg(not(target_os = "android"))]
    fn refresh_sample_rates(&mut self) {
        if let Some((host_id, _)) = self.available_hosts.get(self.selected_host_idx) {
            self.supported_sample_rates = crate::audio::supported_sample_rates(*host_id);
            self.is_jack = self.available_hosts[self.selected_host_idx].1 == "JACK";
        }
    }

}

/// TC-C unit tests (#31): the pure stack ops behind the unified
/// PartUndoSnapshot undo — mute/pan/transpose round-trips and the
/// no-duplicate guard. App glue (restore + engine events) is a thin
/// wrapper over exactly these functions.
#[cfg(test)]
mod part_undo_tests {
    use super::*;

    fn snap(mute: bool, pan: f32, transpose: i8) -> PartUndoSnapshot {
        PartUndoSnapshot {
            part: 0,
            edited_params: Default::default(),
            patch_idx: 0,
            mute,
            pan,
            transpose,
        }
    }

    /// TC-C1: mute → Ctrl+Z → unmuted. The snapshot taken BEFORE the
    /// toggle restores the previous state; the current (muted) state
    /// lands on the redo stack for re-application.
    #[test]
    fn mute_then_undo_restores() {
        let mut undo = std::collections::VecDeque::new();
        let mut redo = Vec::new();
        let mut mute = false;
        part_undo_push(&mut undo, &mut redo, snap(mute, 0.0, 0)); // до изменения
        mute = true;
        let restored = part_undo_step(&mut undo, &mut redo, snap(mute, 0.0, 0))
            .expect("undo must pop the pre-change snapshot");
        mute = restored.mute;
        assert!(!mute, "undo must restore unmute");
        assert_eq!(redo.len(), 1, "muted state preserved for redo");
        // Redo re-applies the mute (повторное применение).
        let redone = part_redo_step(&mut undo, &mut redo, snap(mute, 0.0, 0))
            .expect("redo must pop the muted state");
        assert!(redone.mute);
    }

    /// TC-C2: pan/transpose round-trip — undo restores the pre-drag value,
    /// redo returns the dragged one.
    #[test]
    fn pan_transpose_undo_redo() {
        let mut undo = std::collections::VecDeque::new();
        let mut redo = Vec::new();
        let (mut pan, mut tr) = (0.0_f32, 0_i8);
        part_undo_push(&mut undo, &mut redo, snap(false, pan, tr)); // до drag'а
        pan = -0.5;
        tr = -3;
        let restored = part_undo_step(&mut undo, &mut redo, snap(false, pan, tr)).unwrap();
        assert_eq!(restored.pan, 0.0, "undo restores pan");
        assert_eq!(restored.transpose, 0, "undo restores transpose");
        let redone =
            part_redo_step(&mut undo, &mut redo, snap(false, restored.pan, restored.transpose))
                .unwrap();
        assert_eq!(redone.pan, -0.5);
        assert_eq!(redone.transpose, -3);
    }

    /// TC-C3: identical consecutive snapshots are not pushed (per-part
    /// no-duplicate guard across ALL fields), a real change is.
    #[test]
    fn duplicates_skipped() {
        let mut undo = std::collections::VecDeque::new();
        let mut redo = Vec::new();
        part_undo_push(&mut undo, &mut redo, snap(false, 0.0, 0));
        part_undo_push(&mut undo, &mut redo, snap(false, 0.0, 0));
        assert_eq!(undo.len(), 1, "duplicate of stack top must be skipped");
        part_undo_push(&mut undo, &mut redo, snap(false, 0.25, 0));
        assert_eq!(undo.len(), 2, "a real change must be pushed");
        // Same fields but a DIFFERENT part is not a duplicate.
        let mut other = snap(false, 0.25, 0);
        other.part = 1;
        part_undo_push(&mut undo, &mut redo, other);
        assert_eq!(undo.len(), 3);
    }

    /// Negative matrix: undo on an empty stack is a no-op; repeated Ctrl+Z
    /// walks down the stack entry by entry.
    #[test]
    fn empty_stack_noop_and_repeated_undo() {
        let mut undo = std::collections::VecDeque::new();
        let mut redo = Vec::new();
        assert!(part_undo_step(&mut undo, &mut redo, snap(false, 0.0, 0)).is_none());
        assert!(redo.is_empty(), "no-op must not touch redo");

        part_undo_push(&mut undo, &mut redo, snap(false, 0.0, 0));
        part_undo_push(&mut undo, &mut redo, snap(false, 0.1, 0));
        let first = part_undo_step(&mut undo, &mut redo, snap(false, 0.2, 0)).unwrap();
        assert_eq!(first.pan, 0.1);
        let second = part_undo_step(&mut undo, &mut redo, snap(false, 0.1, 0)).unwrap();
        assert_eq!(second.pan, 0.0, "second Ctrl+Z walks to the next entry");
        assert!(part_undo_step(&mut undo, &mut redo, snap(false, 0.0, 0)).is_none());
    }
}

/// Свап партов разрешён только в пределах одной зоны (A↔A, B↔B):
/// диапазоны/сплит-семантика зоны не переезжают (#54).
fn swap_check(a: usize, b: usize, zone_size: usize) -> Result<(), &'static str> {
    if a == b { return Err("same slot"); }
    if (a / zone_size) != (b / zone_size) { return Err("cross-zone swap"); }
    Ok(())
}

#[cfg(test)]
mod swap_tests {
    use super::*;

    #[test]
    fn tc1_same_zone_swap_ok() {
        assert_eq!(swap_check(3, 5, crate::gui::PARTS_PER_ZONE), Ok(()));
    }

    #[test]
    fn tc2_cross_zone_denied() {
        assert!(swap_check(3, 10, crate::gui::PARTS_PER_ZONE).is_err());
    }

    #[test]
    fn tc3_self_swap_denied() {
        assert!(swap_check(5, 5, crate::gui::PARTS_PER_ZONE).is_err());
    }
}

#[cfg(test)]
mod macro_mirror_tests {
    use super::*;

    #[test]
    fn tc1_macro_mirror_from_params() {
        let mut params = std::collections::BTreeMap::new();
        params.insert("macro_0".to_string(), 0.25);
        params.insert("macro_3".to_string(), 0.9);
        params.insert("unrelated".to_string(), 5.0);
        assert_eq!(
            macro_vals_from_params(&params),
            [0.25, 0.0, 0.0, 0.9, 0.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(
            macro_vals_from_params(&std::collections::BTreeMap::new()),
            [0.0f32; 8]
        );
    }
}
