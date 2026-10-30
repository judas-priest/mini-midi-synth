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

/// Hotplug poll cadence for the default-output-device name (TECHDEBT #18).
const AUDIO_POLL_INTERVAL: Duration = Duration::from_millis(1500);

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
const LAYER_NAMES: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8"];

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
    /// Root-only (bass split): engine gates this part to the lowest held key
    pub root_only: bool,
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
            pan: 0.0, transpose: 0, root_only: false, sf2_mode: false, sf2_program: 0,
            macro_vals: [0.0; 8],
            macro_names: (1..=8).map(|i| format!("Macro {i}")).collect(),
            xy_macro_x: 0,
            xy_macro_y: 1,
        }
    }
}

/// Result of scanning for .sf2 files, including directories that could not be
/// read — so the UI can explain an empty list instead of just showing one.
pub struct Sf2Scan {
    pub files: Vec<(String, std::path::PathBuf)>,
    pub unreadable: Vec<(std::path::PathBuf, String)>,
}

/// Scan for .sf2 files in the standard data directory.
pub fn scan_sf2_files() -> Sf2Scan {
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut unreadable = Vec::new();

    // On Android /sdcard/Download needs storage permissions this app does not
    // hold, so the scan usually fails there. That has to be reported rather
    // than silently producing an empty list.
    #[cfg(target_os = "android")]
    let dirs_to_scan = vec![sf2_dir(), std::path::PathBuf::from("/sdcard/Download")];
    #[cfg(not(target_os = "android"))]
    let dirs_to_scan = vec![sf2_dir()];

    for dir in &dirs_to_scan {
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("sf2") {
                        let name = path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("?")
                            .to_string();
                        if !files.iter().any(|(n, _)| n == &name) {
                            files.push((name, path));
                        }
                    }
                }
            }
            Err(e) => {
                // A missing private sf2 dir is normal on first run. Anything
                // else — permission denied in particular — is worth surfacing.
                if e.kind() != std::io::ErrorKind::NotFound {
                    unreadable.push((dir.clone(), e.to_string()));
                }
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Sf2Scan { files, unreadable }
}

fn sf2_dir() -> std::path::PathBuf {
    crate::config::app_data_dir().join("sf2")
}

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
    pub drum_metronome: bool,
    /// Previous sequencer step (downbeat-flash edge detection)
    pub drum_prev_step: u8,
    /// Instant of the last beat-1 transition (GUI flash)
    pub drum_beat_flash: Option<std::time::Instant>,
    pub perf_window: usize,   // first perf_list idx visible on slots 1..=7
    pub pat_window: usize,    // first pattern idx visible on slots 1..=7
    pub mute_window: usize,   // first part idx visible on MUTE slots 1..=7
    pub drum_undo_atom: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub drum_redo_atom: std::sync::Arc<std::sync::atomic::AtomicBool>,
    // Bank-2 pad console
    pub console: led_console::LedConsole,
    pub console_rx: Option<std::sync::mpsc::Receiver<(u8, bool)>>,
    pub led_snapshot: [[u8; 3]; 16],
    pub led_dirty: bool,
    pub led_socket: Option<std::sync::mpsc::Sender<String>>,
    pub led_socket_alive: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub led_socket_failed_at: Option<std::time::Instant>,
    pub led_last_send: Option<std::time::Instant>,
    /// Companion process WE spawned (never kill a foreign one).
    pub led_child: Option<std::process::Child>,
    /// Backoff gate for (re)spawn attempts (crash / spawn failure).
    pub led_spawn_backoff: Option<std::time::Instant>,
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
    // Performance save/load
    pub perf_name: String,
    pub perf_list: Vec<(String, std::path::PathBuf)>,
    // Navigate button timing
    pub nav_press_time: Option<std::time::Instant>,
    // Pad performance map: 16 pads (notes 36-51) → performance names
    pub show_pad_perf: bool,
    pub pad_perf_map: [Option<String>; 16],
    pub pad_prev_state: [u8; 16],
    pub last_config_save: std::time::Instant,
    /// Smoothed DSP load percentage shown in the header.
    pub dsp_load: f32,
    pub global_dirty: bool,
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
    // Oscilloscope
    pub scope_buf: std::sync::Arc<crate::synth::ScopeBuffer>,
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
    // Undo/redo for patch parameter editing
    pub undo_stack: std::collections::VecDeque<std::collections::BTreeMap<String, f32>>,
    pub redo_stack: Vec<std::collections::BTreeMap<String, f32>>,
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
    /// Next default-device name poll deadline (desktop hotplug poller).
    pub audio_poll_at: Instant,
    pub _audio_handle: Option<crate::audio::AudioBackend>,
    pub midi_tx_shared: crate::midi::SharedMidiTx,
    /// Heap objects the engine and the audio callback handed back. Dropping
    /// them here is what keeps free() off the audio thread.
    pub engine_trash_rx: crate::trash::TrashRx,
    pub cb_trash_rx: crate::trash::TrashRx,
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_global_to_config();
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
            style.spacing.slider_width = 250.0;
            style.spacing.item_spacing = egui::vec2(8.0, 5.0);
        });

        let has_active_notes = (0..128u8)
            .any(|i| self.note_state[i as usize].load(Ordering::Relaxed) > 0);
        let looper_active = self.looper_atoms.state.load(Ordering::Relaxed) != 0;
        let midi_seq_active = self.midi_seq_play_atom.load(Ordering::Relaxed) != 0;
        if has_active_notes || self.drum_playing || looper_active || midi_seq_active {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // Audio hotplug (TECHDEBT #18): (1) dead-stream flag from the cpal
        // error callback; (2) desktop poller — the system default output
        // device name changed (a device swap does not error the old stream:
        // cpal keeps "playing" into the dead device); (3) a failed reconnect
        // retries every RECONNECT_BACKOFF until the device returns. The first
        // failure of a streak toasts; retries are silent. reconnect_audio is
        // a GUI-thread operation (rebuilds the stream); the audio callback
        // only ever sees the finished new stream.
        let now = Instant::now();
        #[cfg(not(target_os = "android"))]
        if now >= self.audio_poll_at {
            self.audio_poll_at = now + AUDIO_POLL_INTERVAL;
            // Only poll the default-device name when the stream lives on the
            // DEFAULT host (PR #9): the poller queries default_host(), so a
            // JACK-host stream's name ("cpal_client_out") never matches a
            // PipeWire default name ("default") and the comparison would
            // fire a false reconnect on a healthy stream 1.5 s after startup.
            // JACK hotplug is detected via the dead-stream/SR-error atom
            // instead (commit 9bf717e).
            if self.audio_host_id == cpal::default_host().id() {
                if let Some(stored) = self.audio_device_name.as_deref() {
                    let current = crate::audio::default_output_device_name(cpal::default_host().id());
                    if crate::audio::ReconnectPolicy::device_changed(stored, current.as_deref()) {
                        log::info!("[audio] default device changed: {stored:?} -> {current:?}");
                        self.audio_disconnected.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
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

        // Keybind processing (skip when text fields have focus or capturing a rebind)
        let text_editing = ctx.memory(|m| m.focused().is_some());
        if self.keybind_capturing.is_some() {
            if let Some(key) = keybinds::detect_key_press(ctx) {
                if key == egui::Key::Escape {
                    self.keybind_capturing = None;
                } else if let Some(action) = self.keybind_capturing.take() {
                    self.keybinds.set_key_for_action(&action, key);
                    self.config.ui.keybinds = self.keybinds.to_config();
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
        // Beat-1 flash: the step atom shows the NEXT step, so step 0 has just
        // sounded when the atom transitions 0 -> 1 while playing.
        let step_now = self.drum_step_atom.load(Ordering::Relaxed);
        if self.drum_playing && self.drum_prev_step == 0 && step_now == 1 {
            self.drum_beat_flash = Some(std::time::Instant::now());
        }
        self.drum_prev_step = step_now;

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
            // Spawn only when nothing connected (a live foreign companion means
            // led_socket connects and we must not churn-spawn every 5s).
            let backoff_ok = self.led_spawn_backoff
                .map(|t| t.elapsed() > std::time::Duration::from_secs(5))
                .unwrap_or(true);
            if self.led_child.is_none()
                && self.led_socket.is_none()
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

        // LED companion socket lifecycle
        if !self.config.ui.led_companion {
            if self.led_socket.take().is_some() {
                log::info!("[console] companion disabled");
            }
        } else if self.led_socket.is_none()
            && self.led_socket_failed_at
                .map(|t| t.elapsed() > std::time::Duration::from_secs(2))
                .unwrap_or(true)
        {
            match std::os::unix::net::UnixStream::connect("/tmp/mini_midi_synth_led.sock") {
                Ok(stream) => {
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
                Err(_) => {
                    self.led_socket_failed_at = Some(std::time::Instant::now());
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

        // LED snapshot (16 slots = pads 17..=32, top row first)
        {
            // Bottom row shows the ACTIVE target's transport (spec v2): drums
            // atoms at target=0, looper state (Recording=1, Playing=2,
            // Overdubbing=3) at target=1.
            let target = self.seq_target_atom.load(Ordering::Relaxed);
            let lstate = crate::synth::looper::LooperState::from_u8(
                self.looper_atoms.state.load(Ordering::Relaxed));
            let (playing, recording) = if target == 0 {
                (self.drum_playing, self.drum_recording)
            } else {
                (
                    matches!(lstate, crate::synth::looper::LooperState::Playing
                            | crate::synth::looper::LooperState::Overdubbing),
                    matches!(lstate, crate::synth::looper::LooperState::Recording
                            | crate::synth::looper::LooperState::Overdubbing),
                )
            };
            let mut mutes = [false; 16];
            for (i, part) in self.parts.iter().enumerate().take(16) {
                mutes[i] = part.mute;
            }
            let filled = self.perf_list.len();
            let undo_avail = self.drum_undo_atom.load(Ordering::Relaxed);
            let redo_avail = self.drum_redo_atom.load(Ordering::Relaxed);
            let page_depth = match self.console.mode() {
                led_console::Mode::Pattern => self.pat_window,
                led_console::Mode::Mute => self.mute_window / 7,
                _ => self.perf_window / 7,
            };
            let top = self.console.top_colors(self.active_perf_console(), filled,
                                              self.drum_current_pattern as usize, &mutes,
                                              self.perf_window, self.pat_window,
                                              self.mute_window, target);
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

                ui.separator();

                let midi_status = self
                    .midi_connected_port
                    .as_deref()
                    .unwrap_or("(disconnected)");
                ui.label(format!("MIDI: {midi_status}"));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // DSP load against the callback budget: one buffer at the
                    // current rate. Anything approaching 100% will glitch.
                    let budget_us =
                        self.config.audio.buffer_size as f32 / self.sample_rate as f32 * 1e6;
                    let worst_us = crate::metrics::METRICS.take_worst_callback_us() as f32;
                    if budget_us > 0.0 {
                        let load = worst_us / budget_us * 100.0;
                        // Smoothed so the number does not flicker every frame.
                        self.dsp_load = self.dsp_load * 0.8 + load * 0.2;
                    }

                    ui.label(format!(
                        "{} | {}Hz | buf {} | dsp {:.0}%",
                        self.current_host,
                        self.sample_rate,
                        self.config.audio.buffer_size,
                        self.dsp_load,
                    ))
                    .on_hover_text(
                        "Worst audio callback time as a share of the buffer budget. \
                         Approaching 100% means crackle — raise the buffer size.",
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
                ui.label("SF2 rel:");
                let mut scale = self.sf2_release_scale;
                if ui.add(egui::DragValue::new(&mut scale).range(0.05..=3.0).speed(0.05).suffix("\u{00d7}"))
                    .on_hover_text("SF2 release-time scale (1.0 = file defaults)").changed()
                {
                    self.sf2_release_scale = scale;
                    let _ = self.ctrl_tx.push(ControlEvent::SetSf2ReleaseScale { scale });
                    self.global_dirty = true;
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

                ui.separator();

                // Oscilloscope
                let scope_data = self.scope_buf.read();
                let scope_w = 150.0_f32;
                let scope_h = 40.0_f32;
                let (resp, painter) = ui.allocate_painter(
                    egui::vec2(scope_w, scope_h),
                    egui::Sense::hover(),
                );
                let r = resp.rect;
                painter.rect_filled(r, 2.0, theme::BG_WIDGET);
                let mid_y = r.center().y;
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
                // Center line
                painter.line_segment(
                    [egui::pos2(r.left(), mid_y), egui::pos2(r.right(), mid_y)],
                    egui::Stroke::new(0.5, theme::STROKE_SUBTLE),
                );
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

        // TECHDEBT #25: drum step-sequencer docked to the bottom on every tab.
        // egui carves panels in registration order (first bottom panel = the
        // bottom-most strip), so registering AFTER keyboard_panel puts this
        // ABOVE the keyboard/pads strip, and registering BEFORE CentralPanel
        // makes the main field reflow instead of being overlapped.
        egui::TopBottomPanel::bottom("drum_seq_panel")
            .resizable(true)
            .default_height(460.0)
            .height_range(120.0..=640.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    self.draw_drum_seq_docked(ui);
                });
            });

        // Right: patch list (for active part) or GM instrument list (SF2 mode)
        egui::SidePanel::right("preset_panel")
            .resizable(true)
            .default_width(160.0)
            .width_range(120.0..=260.0)
            .show(ctx, |ui| {
                let part = self.active_part;
                let is_sf2 = self.parts[part].sf2_mode && self.sf2_keys_soundfont.is_some();
                let layer_label = LAYER_NAMES.get(part).unwrap_or(&"?");

                ui.add_space(theme::SP_SM);
                if is_sf2 {
                    ui.strong(format!("GM Instruments (Layer {layer_label})"));
                } else {
                    ui.horizontal(|ui| {
                        if ui.small_button("\u{25C0}").on_hover_text("Previous preset").clicked() {
                            // Prev patch
                            let idx = self.parts[part].patch_idx;
                            if idx > 0 {
                                let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                                self.parts[part].patch_idx = idx - 1;
                                self.load_edited_params(part);
                                self.send_edited_params(part);
                                self.save_config();
                            }
                        }
                        let idx = self.parts[part].patch_idx;
                        let total = self.patches.len();
                        ui.strong(format!("Presets ({layer_label}) {}/{}", idx + 1, total));
                        if ui.small_button("\u{25B6}").on_hover_text("Next preset").clicked() {
                            // Next patch
                            let idx = self.parts[part].patch_idx;
                            if idx + 1 < self.patches.len() {
                                let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                                self.parts[part].patch_idx = idx + 1;
                                self.load_edited_params(part);
                                self.send_edited_params(part);
                                self.save_config();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
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
                        if ui.add_enabled(!self.undo_stack.is_empty(), egui::Button::new("\u{21B6}").small()).on_hover_text("Undo (Ctrl+Z)").clicked() {
                            self.undo();
                        }
                        if ui.add_enabled(!self.redo_stack.is_empty(), egui::Button::new("\u{21B7}").small()).on_hover_text("Redo (Ctrl+Shift+Z)").clicked() {
                            self.redo();
                        }
                    });
                }
                ui.add_space(theme::SP_XS);

                // Search bar
                let search_width = ui.available_width();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let text_w = if self.preset_search.is_empty() {
                        search_width - 4.0
                    } else {
                        search_width - 22.0
                    };
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.preset_search)
                            .hint_text("\u{1F50D} Search...")
                            .desired_width(text_w.max(60.0)),
                    );
                    if !self.preset_search.is_empty() && ui.small_button("\u{2715}").on_hover_text("Clear search").clicked() {
                        self.preset_search.clear();
                        r.request_focus();
                    }
                });
                ui.separator();

                let query = self.preset_search.to_lowercase();
                let searching = !query.is_empty();

                if is_sf2 {
                    // GM instrument list with same category style as synth presets
                    use crate::synth::sampler::GM_PROGRAM_NAMES;
                    let current_program = self.parts[part].sf2_program;
                    let mut toggled_cat: Option<&str> = None;

                    let gm_categories: &[(&str, usize, usize)] = &[
                        ("Piano", 0, 8),
                        ("Chromatic Perc", 8, 16),
                        ("Organ", 16, 24),
                        ("Guitar", 24, 32),
                        ("Bass", 32, 40),
                        ("Strings", 40, 48),
                        ("Ensemble", 48, 56),
                        ("Brass", 56, 64),
                        ("Reed", 64, 72),
                        ("Pipe", 72, 80),
                        ("Synth Lead", 80, 88),
                        ("Synth Pad", 88, 96),
                        ("Synth FX", 96, 104),
                        ("Ethnic", 104, 112),
                        ("Percussive", 112, 120),
                        ("Sound FX", 120, 128),
                    ];

                    egui::ScrollArea::vertical()
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            for &(cat_name, start, end) in gm_categories {
                                // Collect matching instruments in this category
                                let matches: Vec<usize> = if searching {
                                    (start..end)
                                        .filter(|&i| GM_PROGRAM_NAMES[i].to_lowercase().contains(&query)
                                            || cat_name.to_lowercase().contains(&query))
                                        .collect()
                                } else {
                                    (start..end).collect()
                                };
                                if searching && matches.is_empty() { continue; }

                                let is_collapsed = !searching && self.collapsed_categories.contains(cat_name);
                                let arrow = if is_collapsed { "\u{25B6}" } else { "\u{25BC}" };
                                if !searching {
                                    if ui.selectable_label(false, format!("{arrow} {cat_name}")).clicked() {
                                        toggled_cat = Some(cat_name);
                                    }
                                } else {
                                    ui.label(egui::RichText::new(cat_name).strong().small());
                                }
                                if !is_collapsed {
                                    for i in matches {
                                        let name = GM_PROGRAM_NAMES[i];
                                        let selected = current_program == i as u8;
                                        if ui.selectable_label(selected, format!("  {name}")).clicked() && !selected {
                                            self.parts[part].sf2_program = i as u8;
                                            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                                                part, program: i as u8, bank: 0,
                                            });
                                            if part == 0 { self.config.sf2.layer_a_program = i as u8; }
                                            else { self.config.sf2.layer_b_program = i as u8; }
                                            let _ = self.config.save();
                                        }
                                    }
                                }
                            }
                        });

                    if let Some(cat) = toggled_cat {
                        let cat_string = cat.to_string();
                        if self.collapsed_categories.contains(&cat_string) {
                            self.collapsed_categories.remove(&cat_string);
                        } else {
                            self.collapsed_categories.insert(cat_string);
                        }
                        self.save_collapsed_categories();
                    }
                } else {
                    // Synth patch list
                    let current_preset_idx = self.parts[part].patch_idx;
                    let mut new_idx: Option<usize> = None;
                    let mut toggled_category: Option<String> = None;

                    let categories: Vec<(String, usize)> = {
                        let mut cats = Vec::new();
                        let mut last = String::new();
                        for (i, p) in self.patches.iter().enumerate() {
                            if p.category != last {
                                cats.push((p.category.clone(), i));
                                last = p.category.clone();
                            }
                        }
                        cats
                    };

                    egui::ScrollArea::vertical()
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            let mut cat_idx = 0;
                            for (i, patch) in self.patches.iter().enumerate() {
                                // Track category boundaries
                                if cat_idx < categories.len() && categories[cat_idx].1 == i {
                                    cat_idx += 1;
                                }

                                // When searching, filter by name or category
                                if searching {
                                    let name_match = patch.name.to_lowercase().contains(&query);
                                    let cat_match = patch.category.to_lowercase().contains(&query);
                                    if !name_match && !cat_match { continue; }
                                }

                                if !searching {
                                    // Normal mode: show category headers
                                    if cat_idx > 0 && categories[cat_idx - 1].1 == i {
                                        let cat = &categories[cat_idx - 1].0;
                                        let is_collapsed = self.collapsed_categories.contains(cat);
                                        let arrow = if is_collapsed { "\u{25B6}" } else { "\u{25BC}" };
                                        if ui.selectable_label(false, format!("{arrow} {cat}")).clicked() {
                                            toggled_category = Some(cat.clone());
                                        }
                                    }
                                    if self.collapsed_categories.contains(&patch.category) {
                                        continue;
                                    }
                                }

                                let selected = i == current_preset_idx;
                                if ui.selectable_label(selected, format!("  {}", patch.name)).clicked() && !selected {
                                    new_idx = Some(i);
                                }
                            }
                        });

                    if let Some(cat) = toggled_category {
                        if self.collapsed_categories.contains(&cat) {
                            self.collapsed_categories.remove(&cat);
                        } else {
                            self.collapsed_categories.insert(cat);
                        }
                        self.save_collapsed_categories();
                    }
                    if let Some(idx) = new_idx {
                        let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                        self.parts[part].patch_idx = idx;
                        self.load_edited_params(part);
                        self.send_edited_params(part);
                        self.save_config();
                    }
                }
            });

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
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    self.draw_drum_kit_controls(ui);
                });
            } else {
                self.draw_looper(ui);
                ui.add_space(theme::SP_XS);
                self.draw_pitch_sequencer(ui);
                ui.separator();
                ui.add_space(theme::SP_MD);
                let part = self.active_part;
                let is_sf2 = self.parts[part].sf2_mode;
                // Synth / SF2 toggle
                if self.sf2_keys_soundfont.is_some() {
                    ui.horizontal(|ui| {
                        ui.label("Mode:");
                        let mut sf2 = is_sf2;
                        if ui.selectable_label(!sf2, "Synth").on_hover_text("Use synth engine").clicked() { sf2 = false; }
                        if ui.selectable_label(sf2, "SF2").on_hover_text("Use SoundFont samples").clicked() { sf2 = true; }
                        if sf2 != is_sf2 {
                            let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                            self.parts[part].sf2_mode = sf2;
                            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part, enabled: sf2 });
                            // Save to config
                            if part == 0 { self.config.sf2.layer_a_sf2 = sf2; }
                            else { self.config.sf2.layer_b_sf2 = sf2; }
                            let _ = self.config.save();
                        }
                    });
                }
                if is_sf2 && self.sf2_keys_soundfont.is_some() {
                    use crate::synth::sampler::GM_PROGRAM_NAMES;
                    let prog = self.parts[part].sf2_program;
                    let name = GM_PROGRAM_NAMES.get(prog as usize).unwrap_or(&"?");
                    ui.colored_label(
                        theme::TEXT_SF2_STATUS,
                        format!("SF2: {} | {}: {} | Effects chain applies", self.sf2_keys_loaded_name, prog, name),
                    );
                } else {
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        self.draw_params_editable(ui);
                    });
                }
            }
        });
    }
}

impl App {
    /// Send initial patches to all parts at startup
    pub fn send_initial_presets(&mut self) {
        // Restore persisted live toggles (mute / root-only) — survives restarts
        for (i, st) in self.config.ui.parts_state.iter().enumerate() {
            if let Some(part) = self.parts.get_mut(i) {
                part.mute = st.mute;
                part.root_only = st.root_only;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part: i, mute: st.mute });
                let _ = self.ctrl_tx.push(ControlEvent::SetPartRootOnly { part: i, on: st.root_only });
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
        // Restore the internal-kit remap before the initial push (if an SF2 kit is
        // configured, load_sf2_drums below overwrites this with the per-kit table).
        if self.sf2_drums_loaded_name.is_empty() {
            if let Some(v) = self.config.sf2.drums_remaps.get("internal") {
                // migrate legacy slot-encoded values (0..16) to kit notes (+36)
                let v: Vec<u8> = v.iter().map(|&x| if x < 16 { x + 36 } else { x }).collect();
                let mut t = [0u8; 16];
                for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or((36 + i) as u8).min(55); }
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
    }

    fn find_or_add_sf2(&mut self, sf2_path: &str) -> Option<usize> {
        let idx = self.sf2_file_list.iter().position(|(_, p)| p.to_string_lossy() == sf2_path);
        if let Some(idx) = idx {
            return Some(idx);
        }
        let path = std::path::PathBuf::from(sf2_path);
        if path.exists() {
            let name = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            self.sf2_file_list.push((name, path));
            Some(self.sf2_file_list.len() - 1)
        } else {
            None
        }
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
        let _ = self.config.save();
        self.last_config_save = std::time::Instant::now();
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
        // Collect triggered actions to avoid borrow conflict
        let triggered: Vec<KeyAction> = self.keybinds.binds().iter()
            .filter(|(key, _)| ctx.input(|i| i.key_pressed(*key)))
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
                        self.seq_target_atom.store(1, Ordering::Relaxed);
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
                    let _ = self.ctrl_tx.push(ControlEvent::LooperClear);
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
                        self.save_config();
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
                        self.save_config();
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
        egui::Window::new("Keybindings")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
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
                        if ui.button(btn_text).clicked() {
                            self.keybind_capturing = Some(action.clone());
                        }
                        if ui.small_button("x").clicked() {
                            // Unbind
                            if let Some(key) = self.keybinds.key_for_action(&action) {
                                self.keybinds.binds_mut().retain(|(k, _)| *k != key);
                                self.config.ui.keybinds = self.keybinds.to_config();
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(theme::SP_SM);
                if ui.button("Reset Defaults").clicked() {
                    self.keybinds = keybinds::Keybinds::defaults();
                    self.config.ui.keybinds = self.keybinds.to_config();
                    self.keybind_capturing = None;
                }
            });
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

    /// Push a snapshot of the active part's edited_params onto the undo stack.
    /// Safe to call multiple times — only pushes if state differs from top of stack.
    pub(super) fn push_undo_snapshot(&mut self) {
        let current = &self.parts[self.active_part].edited_params;
        if self.undo_stack.back() == Some(current) {
            return; // no change since last snapshot
        }
        self.undo_stack.push_back(current.clone());
        self.redo_stack.clear();
        if self.undo_stack.len() > Self::MAX_UNDO {
            self.undo_stack.pop_front();
        }
    }

    fn undo(&mut self) {
        let part = self.active_part;
        if let Some(snapshot) = self.undo_stack.pop_back() {
            self.redo_stack.push(self.parts[part].edited_params.clone());
            self.parts[part].edited_params = snapshot;
            self.send_edited_params(part);
            self.show_toast("Undo", ToastKind::Info);
        }
    }

    fn redo(&mut self) {
        let part = self.active_part;
        if let Some(snapshot) = self.redo_stack.pop() {
            self.undo_stack.push_back(self.parts[part].edited_params.clone());
            self.parts[part].edited_params = snapshot;
            self.send_edited_params(part);
            self.show_toast("Redo", ToastKind::Info);
        }
    }

    fn send_edited_params(&mut self, part: usize) {
        let original = self.patches.get(self.parts[part].patch_idx);
        let _ = self.ctrl_tx.push(ControlEvent::load_patch_from_edited(
            part, &self.parts[part].edited_params, original,
        ));
    }

    fn send_part_enabled(&mut self, part: usize) {
        let enabled = self.parts[part].enabled;
        let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part, enabled });
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
            let p = &self.parts[i];
            let (enabled, mute, volume) = (p.enabled, p.mute, p.volume);
            let (min_note, max_note) = (p.min_note, p.max_note);
            let (vel_min, vel_max) = (p.vel_min, p.vel_max);
            let (pan, transpose) = (p.pan, p.transpose);
            let (sf2_mode, sf2_program) = (p.sf2_mode, p.sf2_program);
            let macro_vals = p.macro_vals;

            let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part: i, mute });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartVolume { part: i, volume });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartRange { part: i, min_note, max_note });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartVelRange { part: i, vel_min, vel_max });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part: i, pan });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part: i, semitones: transpose });
            for (index, value) in macro_vals.iter().enumerate() {
                let _ = self.ctrl_tx.push(ControlEvent::SetMacro { part: i, index, value: *value });
            }
            if sf2_mode {
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: true });
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                    part: i, program: sf2_program, bank: 0,
                });
            }
        }
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

    /// Extract the kit's own drum map (note -> SAMPLE name) for the remap UI.
    /// All instruments/presets are scanned; per-key sample names are collected.
    fn extract_kit_names(&mut self, sf: &rustysynth::SoundFont) {
        self.reset_kit_names();
        let headers = sf.get_sample_headers();
        let mut seen = [false; 26];
        for inst in sf.get_instruments() {
            for r in inst.get_regions() {
                let (lo, hi) = (r.get_key_range_start(), r.get_key_range_end());
                if hi < 30 || lo > 55 { continue; }
                let sid = r.get_sample_id();
                if sid >= headers.len() { continue; }
                let name = headers[sid].get_name().to_string();
                for k in lo.max(30)..=hi.min(55) {
                    let i = (k - 30) as usize;
                    if !seen[i] { self.drum_kit_names[i] = name.clone(); seen[i] = true; }
                }
            }
        }
    }

    fn reset_kit_names(&mut self) {
        for (i, n) in self.drum_kit_names.iter_mut().enumerate() {
            let note = 30 + i as u8;
            *n = if (36..=51).contains(&note) {
                crate::synth::drum::DRUM_NAMES[(note - 36) as usize].to_string()
            } else { String::new() };
        }
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
        let _ = self.config.save();
    }

    fn push_drum_pad_vel(&mut self) {
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetPadVel { table: Box::new(self.drum_pad_vel) });
    }

    /// Wired up by the pad-vel panel in drums.rs.
    fn save_drum_pad_vel(&mut self) {
        let key = self.drum_remap_key();
        self.config.sf2.drums_vels.insert(key, self.drum_pad_vel.to_vec());
        let _ = self.config.save();
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

    /// Parse an SF2 file and return the Arc<SoundFont>, or set sf2_status on error.
    fn parse_sf2(&mut self, path: &std::path::Path) -> Option<std::sync::Arc<rustysynth::SoundFont>> {
        match std::fs::File::open(path) {
            Ok(mut file) => {
                use std::io::BufReader;
                let mut reader = BufReader::new(&mut file);
                match rustysynth::SoundFont::new(&mut reader) {
                    Ok(sf) => Some(std::sync::Arc::new(sf)),
                    Err(e) => { self.show_toast(format!("SF2 parse error: {e}"), ToastKind::Error); None }
                }
            }
            Err(e) => { self.show_toast(format!("SF2 open error: {e}"), ToastKind::Error); None }
        }
    }

    fn load_sf2_keys(&mut self, idx: usize) {
        let Some((name, path)) = self.sf2_file_list.get(idx).cloned() else {
            self.show_toast("SF2 file not found", ToastKind::Error);
            return;
        };
        // Reuse existing drums soundfont if same file
        let sf = if self.sf2_drums_loaded_name == name {
            self.sf2_drums_soundfont.clone()
        } else {
            None
        };
        let sf = match sf {
            Some(s) => s,
            None => match self.parse_sf2(&path) {
                Some(s) => s,
                None => return,
            },
        };
        self.sf2_keys_soundfont = Some(sf.clone());
        let _ = self.ctrl_tx.push(ControlEvent::SetSf2BlockSize { size: self.sf2_block_size });
        let _ = self.ctrl_tx.push(ControlEvent::LoadKeysSoundFont { soundfont: sf });
        self.sf2_keys_loaded_name = name;
        self.show_toast("Keys SF2 loaded", ToastKind::Success);
        // Apply per-part SF2 modes
        for i in 0..2 {
            if self.parts[i].sf2_mode {
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: true });
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                    part: i, program: self.parts[i].sf2_program, bank: 0,
                });
            }
        }
        self.config.sf2.keys_file_path = Some(path.to_string_lossy().to_string());
        self.config.sf2.file_path = self.config.sf2.keys_file_path.clone(); // compat
        let _ = self.config.save();
    }

    fn load_sf2_drums(&mut self, idx: usize) {
        let Some((name, path)) = self.sf2_file_list.get(idx).cloned() else {
            self.show_toast("SF2 file not found", ToastKind::Error);
            return;
        };
        // Reuse existing keys soundfont if same file
        let sf = if self.sf2_keys_loaded_name == name {
            self.sf2_keys_soundfont.clone()
        } else {
            None
        };
        let sf = match sf {
            Some(s) => s,
            None => match self.parse_sf2(&path) {
                Some(s) => s,
                None => return,
            },
        };
        self.sf2_drums_soundfont = Some(sf.clone());
        let _ = self.ctrl_tx.push(ControlEvent::SetSf2BlockSize { size: self.sf2_block_size });
        let _ = self.ctrl_tx.push(ControlEvent::LoadDrumsSoundFont { soundfont: sf.clone() });
        self.sf2_drums_loaded_name = name;
        self.extract_kit_names(&sf);
        self.push_drum_choke_groups();
        self.show_toast("Drums SF2 loaded", ToastKind::Success);
        if self.sf2_drums_enabled {
            let _ = self.ctrl_tx.push(ControlEvent::SetDrumsSf2Mode { enabled: true });
        }
        // Load the per-kit remap table (identity if none saved for this kit).
        if let Some(v) = self.config.sf2.drums_remaps.get(&self.drum_remap_key()) {
            // migrate legacy slot-encoded values (0..16) to kit notes (+36)
            let v: Vec<u8> = v.iter().map(|&x| if x < 16 { x + 36 } else { x }).collect();
            let mut t = [0u8; 16];
            for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or(36 + i as u8).clamp(30, 55); }
            self.drum_remap = t;
        } else {
            for (i, v) in self.drum_remap.iter_mut().enumerate() { *v = (36 + i) as u8; }
        }
        self.push_drum_remap();
        if let Some(v) = self.config.sf2.drums_vels.get(&self.drum_remap_key()) {
            let mut t = [0u8; 16];
            for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or(0).min(127); }
            self.drum_pad_vel = t;
        } else {
            self.drum_pad_vel = [0u8; 16];
        }
        self.push_drum_pad_vel();
        self.config.sf2.drums_file_path = Some(path.to_string_lossy().to_string());
        let _ = self.config.save();
    }

    fn unload_sf2_keys(&mut self) {
        self.sf2_keys_soundfont = None;
        self.sf2_keys_loaded_name.clear();
        for i in 0..2 {
            self.parts[i].sf2_mode = false;
            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: false });
        }
        let _ = self.ctrl_tx.push(ControlEvent::UnloadKeysSoundFont);
        self.config.sf2.keys_file_path = None;
        self.config.sf2.file_path = None;
        let _ = self.config.save();
    }

    fn unload_sf2_drums(&mut self) {
        self.sf2_drums_soundfont = None;
        self.sf2_drums_loaded_name.clear();
        self.sf2_drums_enabled = false;
        self.reset_kit_names();
        self.push_drum_choke_groups(); // back to GM choke layout
        // Internal kit: reset to identity remap.
        for (i, v) in self.drum_remap.iter_mut().enumerate() { *v = (36 + i) as u8; }
        self.push_drum_remap();
        self.drum_pad_vel = [0u8; 16];
        self.push_drum_pad_vel();
        let _ = self.ctrl_tx.push(ControlEvent::SetDrumsSf2Mode { enabled: false });
        let _ = self.ctrl_tx.push(ControlEvent::UnloadDrumsSoundFont);
        self.config.sf2.drums_file_path = None;
        let _ = self.config.save();
    }

    fn save_config(&mut self) {
        self.config.ui.last_preset = self
            .patches
            .get(self.parts[0].patch_idx)
            .map(|p| p.name.clone());
        let _ = self.config.save();
    }

    /// Execute an action from the bank-2 pad console state machine.
    fn execute_console_action(&mut self, action: led_console::Action) {
        match action {
            led_console::Action::LoadPerf(i) => {
                let idx = self.perf_window + i - 1; // slot -> real index
                if let Some((name, _)) = self.perf_list.get(idx) {
                    let name = name.clone();
                    if self.load_performance_by_name(&name) {
                        log::info!("[console] perf slot {} loaded ({})", i, name);
                        self.follow_window(idx, true);
                    }
                }
            }
            led_console::Action::SelectPattern(i) => {
                // консоль шлёт УЖЕ window-local 0-based (slot-1 из tap())
                let idx = self.pat_window + i;
                if idx < 8 {
                    self.drum_current_pattern = idx as u8;
                    let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqPattern { pattern: idx as u8 });
                    self.follow_window(idx, false);
                }
            }
            led_console::Action::ToggleMute(i) => {
                let part = self.mute_window + i; // i уже 0-based (slot-1 из консоли) // slot -> real part
                if let Some(p) = self.parts.get_mut(part) {
                    let mute = !p.mute;
                    p.mute = mute;
                    let _ = self.ctrl_tx.push(crate::synth::ControlEvent::SetPartMute { part, mute });
                    self.persist_parts_state();
                }
            }
            led_console::Action::ToggleTransportTarget => {
                let cur = self.seq_target_atom.load(Ordering::Relaxed);
                let new = if cur == 0 { 1 } else { 0 };
                self.seq_target_atom.store(new, Ordering::Relaxed);
                log::info!("[console] seq_target -> {}",
                           if new == 0 { "drums" } else { "looper" });
            }
            led_console::Action::Undo => {
                let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqUndo);
            }
            led_console::Action::Redo => {
                let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqRedo);
            }
            led_console::Action::StepPerf(dir) => {
                let n = self.perf_list.len();
                if n == 0 { return; }
                let cur = self.active_perf_console().unwrap_or(0);
                let next = (cur as i32 + dir).rem_euclid(n as i32) as usize;
                if let Some((name, _)) = self.perf_list.get(next) {
                    let name = name.clone();
                    self.load_performance_by_name(&name);
                }
                self.follow_window(next, true);
            }
            led_console::Action::StepPattern(dir) => {
                let next = (self.drum_current_pattern as i32 + dir).rem_euclid(8);
                self.drum_current_pattern = next as u8;
                let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqPattern { pattern: next as u8 });
                self.follow_window(next as usize, false);
            }
            led_console::Action::PageBack => {
                self.page_window(-7);
                // не оставлять пурпур за экраном (Important-фикс ревью)
                if let Some(i) = self.active_perf_console() { self.follow_window(i, true); }
                self.follow_window(self.drum_current_pattern as usize, false);
            }
            led_console::Action::PageForward => {
                self.page_window(7);
                if let Some(i) = self.active_perf_console() { self.follow_window(i, true); }
                self.follow_window(self.drum_current_pattern as usize, false);
            }
        }
    }

    /// Auto-follow: keep the active index inside [w, w+7); if it escapes past
    /// the right edge it becomes the FIRST visible slot (user-requested).
    fn follow_window(&mut self, idx: usize, is_perf: bool) {
        if is_perf {
            if !(self.perf_window..self.perf_window + 7).contains(&idx) {
                self.perf_window = idx.min(self.perf_list.len().saturating_sub(1));
            }
        } else if !(self.pat_window..self.pat_window + 7).contains(&idx) {
            self.pat_window = idx.min(7);
        }
    }

    /// Shift the active mode's window by `delta` slots, clamped to the list.
    fn page_window(&mut self, delta: i32) {
        match self.console.mode() {
            led_console::Mode::Home => {
                let n = self.perf_list.len();
                let max_w = n.saturating_sub(7);
                self.perf_window = (self.perf_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            led_console::Mode::Pattern => {
                // 8 patterns, window shows 7: max start = 1
                self.pat_window = (self.pat_window as i32 + delta).clamp(0, 1) as usize;
            }
            led_console::Mode::Mute => {
                let n = self.parts.len();
                let max_w = n.saturating_sub(7);
                self.mute_window = (self.mute_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            _ => {}
        }
    }

    /// Persist live per-part toggles (mute / root-only) to config so they
    /// survive restarts (they are NOT part of performance files' semantics).
    pub fn persist_parts_state(&mut self) {
        self.config.ui.parts_state = self
            .parts
            .iter()
            .map(|p| crate::config::PartUiState { mute: p.mute, root_only: p.root_only })
            .collect();
        self.save_global_to_config();
    }

    /// Index of the current performance in perf_list (None if unknown).
    fn active_perf_console(&self) -> Option<usize> {
        self.perf_list.iter().position(|(n, _)| n == &self.perf_name)
    }

    /// Load a performance by name (for pad perf switching).
    /// Returns true if found and loaded.
    fn load_performance_by_name(&mut self, name: &str) -> bool {
        let perf = if name == "(Init)" {
            preset::init_performance()
        } else {
            let path = self.perf_list.iter()
                .find(|(n, _)| n == name)
                .map(|(_, p)| p.clone());
            let Some(path) = path else { return false; };
            match preset::load_performance(&path) {
                Ok(p) => p,
                Err(_) => return false,
            }
        };
        // NO AllNotesOff here: seamless perf switch — sounding tails ring out
        // naturally (SF2 voices keep the old instrument), held keys keep
        // sustaining without re-attack; new notes use the new perf.
        // Legacy 8-part migration is centralized in preset::load_performance.
        let parts = perf.parts;
        // Restore the stored split point; legacy perfs derive it from the
        // first enabled zone-B part's key_low (fallback: C4=60).
        self.split_point = perf.split_point.unwrap_or_else(|| {
            parts.iter()
                .skip(crate::gui::PARTS_PER_ZONE)
                .find(|p| p.enabled)
                .map(|p| p.key_low)
                .filter(|&v| v > 0)
                .unwrap_or(60)
        });
        self.perf_name = perf.name.clone();
        for (i, part) in parts.iter().enumerate() {
            if i >= self.parts.len() { break; }
            let patch_idx = self.patches.iter()
                .position(|p| p.name == part.patch_name)
                .unwrap_or(0);
            self.parts[i].patch_idx = patch_idx;
            self.parts[i].enabled = part.enabled;
            self.parts[i].volume = part.volume;
            self.parts[i].min_note = part.key_low;
            self.parts[i].max_note = part.key_high;
            self.parts[i].edited_params = part.param_overrides.clone();
            self.parts[i].sf2_mode = part.sf2_mode;
            self.parts[i].sf2_program = part.sf2_program;
            self.parts[i].pan = part.pan;
            self.parts[i].transpose = part.transpose;
            self.parts[i].mute = part.mute;
            self.parts[i].vel_min = if part.vel_min == 0 { 1 } else { part.vel_min };
            self.parts[i].vel_max = if part.vel_max == 0 { 127 } else { part.vel_max };
            self.send_edited_params(i);
            self.send_part_enabled(i);
            self.send_part_volume(i);
            self.send_part_range(i);
            let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part: i, pan: part.pan });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part: i, semitones: part.transpose });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part: i, mute: part.mute });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartVelRange {
                part: i, vel_min: self.parts[i].vel_min, vel_max: self.parts[i].vel_max });
            // Restore SF2 mode for this part
            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: part.sf2_mode });
            if part.sf2_mode {
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                    part: i, program: part.sf2_program, bank: 0,
                });
            }
        }
        // Parts absent from the performance must not keep playing
        // the previous patch with their old voices.
        for i in parts.len()..self.parts.len() {
            if self.parts[i].enabled {
                self.parts[i].enabled = false;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: false });
            }
        }
        true
    }

    fn save_collapsed_categories(&mut self) {
        self.config.ui.collapsed_categories = self.collapsed_categories.iter().cloned().collect();
        let _ = self.config.save();
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
