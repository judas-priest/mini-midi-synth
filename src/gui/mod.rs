/// GUI interface using egui/eframe.

mod params;
mod drums;
mod midi_seq;
mod settings;
mod keyboard;
mod layers;
mod fx_chain;
mod macros;
pub mod keybinds;

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cpal::HostId;
use eframe::egui;
use rtrb::Producer;

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
];
const FILTER_ROUTING_NAMES: &[&str] = &["Single", "Serial", "Parallel"];
const FORMANT_VOICE_NAMES: &[&str] = &["Bass", "Tenor", "Alto", "Soprano"];
const FORMANT_VOWEL_NAMES: &[&str] = &["A (ah)", "E (eh)", "I (ee)", "O (oh)", "U (oo)"];
const LAYER_NAMES: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8"];

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
    pub sf2_mode: bool,
    pub sf2_program: u8,
    /// Macro knob values (0..1) — live, sent to engine via SetMacro
    pub macro_vals: [f32; 8],
    /// Macro knob names (user-editable, shown as label in GUI)
    pub macro_names: Vec<String>,
}

impl PartState {
    pub fn new_empty() -> Self {
        Self {
            patch_idx: 0, edited_params: Default::default(), params_dirty: false,
            enabled: false, mute: false, volume: 0.8,
            min_note: 0, max_note: 127, vel_min: 1, vel_max: 127,
            pan: 0.0, transpose: 0, sf2_mode: false, sf2_program: 0,
            macro_vals: [0.0; 8],
            macro_names: (1..=8).map(|i| format!("Macro {i}")).collect(),
        }
    }
}

/// Scan for .sf2 files in the standard data directory.
pub fn scan_sf2_files() -> Vec<(String, std::path::PathBuf)> {
    let dir = sf2_dir();
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("sf2") {
                let name = path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string();
                result.push((name, path));
            }
        }
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

fn sf2_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("mini_midi_synth")
        .join("sf2")
}

fn note_name(note: u8) -> String {
    let name = NOTE_NAMES[(note % 12) as usize];
    let oct = (note as i8 / 12) - 2;
    format!("{name}{oct}")
}

pub struct App {
    pub _frame_count: u64,
    pub patches: Vec<Patch>,
    pub note_state: NoteState,
    pub pad_state: midi::PadState,
    pub ctrl_tx: Producer<ControlEvent>,
    pub sample_rate: u32,

    pub config: Config,
    pub current_host: String,
    pub available_hosts: Vec<(HostId, &'static str)>,
    pub supported_sample_rates: Vec<u32>,

    pub midi_port_names: Vec<String>,
    pub midi_connected_port: Option<String>,

    pub show_settings: bool,
    pub selected_host_idx: usize,
    pub selected_sample_rate: u32,
    pub selected_buffer_size: u32,
    pub selected_midi_port: Option<String>,
    pub settings_status: String,
    pub is_jack: bool,

    /// Layer states
    pub parts: Vec<PartState>,
    /// Currently edited part (0 = A, 1 = B)
    pub active_part: usize,

    pub on_midi_reconnect: Option<Box<dyn FnMut(usize) -> Result<(), String>>>,

    /// Collapsed patch categories
    pub collapsed_categories: HashSet<String>,

    /// Feedback from audio thread
    pub feedback_rx: Option<rtrb::Consumer<ParamFeedback>>,
    /// CC mapping
    pub cc_map: CcMap,
    /// MIDI Learn target parameter key
    pub midi_learn_target: Option<String>,
    /// Program change atom for patch feedback
    pub _program_change_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
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
    pub drum_volume: f32,
    pub drum_bpm: f32,
    pub drum_swing: f32,
    pub drum_playing: bool,
    pub drum_recording: bool,
    pub drum_current_pattern: u8,
    pub drum_step_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    pub drum_play_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    pub drum_rec_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    // Drum kit save/load
    pub drum_kit_name: String,
    pub drum_kit_list: Vec<(String, std::path::PathBuf)>,
    pub drum_kit_status: String,
    pub drum_midi_import_path: String,
    // Looper
    pub show_looper: bool,
    pub looper_atoms: std::sync::Arc<LooperAtoms>,
    pub looper_display: std::sync::Arc<std::sync::Mutex<LooperDisplay>>,
    pub looper_bars: u8,
    pub looper_quantize: u8,
    pub looper_sync_bpm: bool,
    pub looper_bpm: f32,
    pub looper_clear_confirm: Option<std::time::Instant>,
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
    pub perf_status: String,
    // Navigate button timing
    pub nav_press_time: Option<std::time::Instant>,
    // Pad performance map: 16 pads (notes 36-51) → performance names
    pub show_pad_perf: bool,
    pub pad_perf_map: [Option<String>; 16],
    pub pad_perf_status: String,
    pub pad_prev_state: [u8; 16],
    pub last_config_save: std::time::Instant,
    pub global_dirty: bool,
    // SF2 sampler
    pub sf2_file_list: Vec<(String, std::path::PathBuf)>,
    pub sf2_keys_selected: Option<usize>,
    pub sf2_keys_loaded_name: String,
    pub sf2_drums_selected: Option<usize>,
    pub sf2_drums_loaded_name: String,
    pub sf2_status: String,
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
    pub midi_seq_status: String,
    pub midi_seq_tracks: Vec<midi_seq::MidiSeqTrackGui>,
    pub midi_seq_playing: bool,
    pub midi_seq_looping: bool,
    pub midi_seq_bpm: Option<f32>,
    pub midi_seq_play_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
    pub midi_seq_pos_atom: std::sync::Arc<std::sync::atomic::AtomicU32>,
    /// Result of an async file picker (zenity/kdialog subprocess). None = no pending pick.
    pub midi_seq_file_pick: Option<std::sync::Arc<std::sync::Mutex<Option<String>>>>,
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_global_to_config();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Increase base font size and spacing
        ctx.style_mut(|style| {
            style.text_styles.get_mut(&egui::TextStyle::Body).unwrap().size = 15.0;
            style.text_styles.get_mut(&egui::TextStyle::Button).unwrap().size = 15.0;
            style.text_styles.get_mut(&egui::TextStyle::Monospace).unwrap().size = 14.0;
            style.text_styles.get_mut(&egui::TextStyle::Small).unwrap().size = 13.0;
            style.text_styles.get_mut(&egui::TextStyle::Heading).unwrap().size = 20.0;
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

        // Drain feedback from audio thread
        self.drain_feedback();

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
                            self.pad_perf_status = format!("Loaded: {name}");
                        } else {
                            self.pad_perf_status = format!("Not found: {name}");
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
                } else {
                    let action = self.keybind_capturing.take().unwrap();
                    self.keybinds.set_key_for_action(&action, key);
                    self.config.ui.keybinds = self.keybinds.to_config();
                }
            }
        } else if !text_editing {
            self.process_keybinds(ctx);
        }

        // Sync drum play/rec state from engine (for MIDI-triggered changes)
        self.drum_playing = self.drum_play_atom.load(Ordering::Relaxed) != 0;
        self.drum_recording = self.drum_rec_atom.load(Ordering::Relaxed) != 0;

        // MIDI Learn indicator
        if self.midi_learn_target.is_some() {
            let target_name = self.midi_learn_target.clone().unwrap_or_default();
            egui::TopBottomPanel::top("midi_learn_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::YELLOW, format!("MIDI Learn: move a CC for '{target_name}'..."));
                    if ui.button("Cancel").clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        // handled below
                    }
                });
            });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.midi_learn_target = None;
            }
        }

        // Top bar
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(2.0);
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
                    ui.label(format!(
                        "{} | {}Hz | buf {}",
                        self.current_host, self.sample_rate, self.config.audio.buffer_size
                    ));
                });
            });
            ui.add_space(4.0);
        });

        // Global controls bar
        egui::TopBottomPanel::top("global_controls").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // Master Volume (global)
                let mut vol = self.global_params.get("master_volume").copied().unwrap_or(0.8);
                ui.label("Vol:");
                if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false)).changed() {
                    self.global_params.insert("master_volume".into(), vol);
                    let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "master_volume", value: vol });
                    self.global_dirty = true;
                }

                ui.separator();

                // Master Tone (global)
                let mut tone = self.global_params.get("master_tone").copied().unwrap_or(20000.0);
                ui.label("Tone:");
                if ui.add(egui::Slider::new(&mut tone, 200.0..=20000.0).logarithmic(true).show_value(false)).changed() {
                    self.global_params.insert("master_tone".into(), tone);
                    let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "master_tone", value: tone });
                    self.global_dirty = true;
                }

                ui.separator();

                // Reverb (global)
                let mut reverb = self.global_params.get("reverb_mix").copied().unwrap_or(0.0);
                ui.label("Reverb:");
                if ui.add(egui::Slider::new(&mut reverb, 0.0..=1.0).show_value(false)).changed() {
                    self.global_params.insert("reverb_mix".into(), reverb);
                    let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "reverb_mix", value: reverb });
                    self.global_dirty = true;
                }

                ui.separator();

                // Delay (global)
                let mut delay = self.global_params.get("delay_mix").copied().unwrap_or(0.0);
                ui.label("Delay:");
                if ui.add(egui::Slider::new(&mut delay, 0.0..=1.0).show_value(false)).changed() {
                    self.global_params.insert("delay_mix".into(), delay);
                    let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "delay_mix", value: delay });
                    self.global_dirty = true;
                }

                ui.separator();

                // Pitch Bend Range
                let mut pbr = self.global_params.get("pitch_bend_range").copied().unwrap_or(2.0) as i32;
                ui.label("PB:");
                if ui.add(egui::Slider::new(&mut pbr, 1..=24).suffix("st").show_value(true)).changed() {
                    let val = pbr as f32;
                    self.global_params.insert("pitch_bend_range".into(), val);
                    let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: "pitch_bend_range", value: val });
                    self.global_dirty = true;
                }

                ui.separator();

                // Oscilloscope
                let scope_data = self.scope_buf.read();
                let scope_w = 100.0_f32;
                let scope_h = 24.0_f32;
                let (resp, painter) = ui.allocate_painter(
                    egui::vec2(scope_w, scope_h),
                    egui::Sense::hover(),
                );
                let r = resp.rect;
                painter.rect_filled(r, 2.0, egui::Color32::from_rgb(20, 20, 30));
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
                    painter.add(egui::Shape::line(points, egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 200, 100))));
                }
                // Center line
                painter.line_segment(
                    [egui::pos2(r.left(), mid_y), egui::pos2(r.right(), mid_y)],
                    egui::Stroke::new(0.5, egui::Color32::from_rgb(60, 60, 80)),
                );
            });
            ui.add_space(4.0);
        });

        // Bottom: keyboard
        egui::TopBottomPanel::bottom("keyboard_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            self.draw_keyboard(ui);
            ui.add_space(4.0);
        });

        // Pad perf status (above keyboard)
        if !self.pad_perf_status.is_empty() || self.show_pad_perf {
            egui::TopBottomPanel::bottom("pad_perf_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.show_pad_perf {
                        ui.colored_label(egui::Color32::YELLOW, "PAD PERF: tap a pad to switch performance");
                    }
                    if !self.pad_perf_status.is_empty() {
                        ui.separator();
                        ui.label(egui::RichText::new(&self.pad_perf_status).small().weak());
                    }
                });
            });
        }

        // Right: patch list (for active part) or GM instrument list (SF2 mode)
        egui::SidePanel::right("preset_panel")
            .resizable(true)
            .default_width(160.0)
            .min_width(120.0)
            .max_width(260.0)
            .show(ctx, |ui| {
                let part = self.active_part;
                let is_sf2 = self.parts[part].sf2_mode && self.sf2_keys_soundfont.is_some();
                let layer_label = LAYER_NAMES.get(part).unwrap_or(&"?");

                ui.add_space(4.0);
                if is_sf2 {
                    ui.strong(format!("GM Instruments (Layer {layer_label})"));
                } else {
                    ui.horizontal(|ui| {
                        if ui.small_button("\u{25C0}").clicked() {
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
                        ui.strong(format!("Presets ({layer_label})"));
                        if ui.small_button("\u{25B6}").clicked() {
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
                        if ui.small_button("Init").clicked() {
                            let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                            self.parts[part].edited_params = crate::synth::PatchParams::default().to_map();
                            self.send_edited_params(part);
                        }
                        if ui.small_button("Rnd").clicked() {
                            let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                            self.parts[part].edited_params = crate::synth::PatchParams::random_map();
                            self.send_edited_params(part);
                        }
                    });
                }
                ui.add_space(2.0);

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
                    if !self.preset_search.is_empty() {
                        if ui.small_button("\u{2715}").clicked() {
                            self.preset_search.clear();
                            r.request_focus();
                        }
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
                            let mut shown_cat = String::new();
                            for (i, patch) in self.patches.iter().enumerate() {
                                // Track category boundaries
                                if cat_idx < categories.len() && categories[cat_idx].1 == i {
                                    shown_cat = categories[cat_idx].0.clone();
                                    cat_idx += 1;
                                }

                                // When searching, filter by name or category
                                if searching {
                                    let name_match = patch.name.to_lowercase().contains(&query);
                                    let cat_match = patch.category.to_lowercase().contains(&query);
                                    if !name_match && !cat_match { continue; }

                                    // Show category header once per group
                                    if shown_cat != patch.category || i == categories.iter().find(|(c, _)| c == &patch.category).map(|(_, idx)| *idx).unwrap_or(usize::MAX) {
                                        // We handle this below
                                    }
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

        // Central: part tabs + parameters / drum sequencer
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            self.draw_part_tabs(ui);
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(6.0);
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
                    self.draw_drum_sequencer(ui);
                });
            } else {
                self.draw_looper(ui);
                ui.add_space(2.0);
                self.draw_pitch_sequencer(ui);
                ui.separator();
                ui.add_space(6.0);
                let part = self.active_part;
                let is_sf2 = self.parts[part].sf2_mode;
                // Synth / SF2 toggle
                if self.sf2_keys_soundfont.is_some() {
                    ui.horizontal(|ui| {
                        ui.label("Mode:");
                        let mut sf2 = is_sf2;
                        if ui.selectable_label(!sf2, "Synth").clicked() { sf2 = false; }
                        if ui.selectable_label(sf2, "SF2").clicked() { sf2 = true; }
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
                        egui::Color32::from_rgb(160, 160, 160),
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
        if let Some(l) = self.parts.get(0) { self.config.ui.layer_a_volume = l.volume; }
        if let Some(l) = self.parts.get(1) { self.config.ui.layer_b_volume = l.volume; }
        self.config.ui.pad_perf_map = self.pad_perf_map.to_vec();
        self.config.ui.looper_sync_bpm = self.looper_sync_bpm;
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
                            .map(|k| key_to_str(k))
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
                ui.add_space(5.0);
                if ui.button("Reset Defaults").clicked() {
                    self.keybinds = keybinds::Keybinds::defaults();
                    self.config.ui.keybinds = self.keybinds.to_config();
                    self.keybind_capturing = None;
                }
            });
        self.show_keybinds_window = open;
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
                    Err(e) => { self.sf2_status = format!("Parse error: {e}"); None }
                }
            }
            Err(e) => { self.sf2_status = format!("Open error: {e}"); None }
        }
    }

    fn load_sf2_keys(&mut self, idx: usize) {
        let Some((name, path)) = self.sf2_file_list.get(idx).cloned() else {
            self.sf2_status = "File not found".to_string();
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
        self.sf2_status = "Keys SF2 loaded".to_string();
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
            self.sf2_status = "File not found".to_string();
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
        let _ = self.ctrl_tx.push(ControlEvent::LoadDrumsSoundFont { soundfont: sf });
        self.sf2_drums_loaded_name = name;
        self.sf2_status = "Drums SF2 loaded".to_string();
        if self.sf2_drums_enabled {
            let _ = self.ctrl_tx.push(ControlEvent::SetDrumsSf2Mode { enabled: true });
        }
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

    /// Load a performance by name (for pad perf switching).
    /// Returns true if found and loaded.
    fn load_performance_by_name(&mut self, name: &str) -> bool {
        let path = self.perf_list.iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p.clone());
        let Some(path) = path else { return false; };
        let Ok(perf) = preset::load_performance(&path) else { return false; };
        // AllNotesOff for smooth transition
        let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
        self.perf_name = perf.name.clone();
        for (i, part) in perf.parts.iter().enumerate() {
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
            self.send_edited_params(i);
            self.send_part_enabled(i);
            self.send_part_volume(i);
            self.send_part_range(i);
            // Restore SF2 mode for this part
            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: part.sf2_mode });
            if part.sf2_mode {
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                    part: i, program: part.sf2_program, bank: 0,
                });
            }
        }
        true
    }

    fn save_collapsed_categories(&mut self) {
        self.config.ui.collapsed_categories = self.collapsed_categories.iter().cloned().collect();
        let _ = self.config.save();
    }

    fn _save_window_size(&mut self, ctx: &egui::Context) {
        self._frame_count += 1;

        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));

        // Debug: log every ~5 seconds to file + stdout
        if self._frame_count % 50 == 1 {
            let inner = ctx.input(|i| i.viewport().inner_rect);
            let outer = ctx.input(|i| i.viewport().outer_rect);
            let screen = ctx.screen_rect();
            let msg = format!(
                "[win] inner={:?} outer={:?} screen={:.0}x{:.0} max={} fs={}\n",
                inner.map(|r| (r.width(), r.height())),
                outer.map(|r| (r.width(), r.height())),
                screen.width(), screen.height(),
                maximized, fullscreen,
            );
            print!("{msg}");
            // Also write to file in case terminal swallows output
            let _ = std::fs::write("/tmp/mini_midi_synth_debug.txt", &msg);
        }

        // Save maximized/fullscreen state
        if maximized != self.config.ui.maximized {
            self.config.ui.maximized = maximized;
            let _ = self.config.save();
        }

        if maximized || fullscreen {
            return;
        }

        // Try all available rect sources
        let (w, h) = ctx.input(|i| {
            let vp = i.viewport();
            if let Some(rect) = vp.inner_rect {
                (rect.width(), rect.height())
            } else if let Some(rect) = vp.outer_rect {
                (rect.width(), rect.height())
            } else {
                (0.0, 0.0)
            }
        });

        let (w, h) = if w < 100.0 || h < 100.0 {
            let rect = ctx.screen_rect();
            (rect.width(), rect.height())
        } else {
            (w, h)
        };

        if w > 100.0 && h > 100.0
            && ((w - self.config.ui.window_width).abs() > 5.0
                || (h - self.config.ui.window_height).abs() > 5.0)
        {
            eprintln!("[win] SAVING: {:.0}x{:.0} -> {:.0}x{:.0}", self.config.ui.window_width, self.config.ui.window_height, w, h);
            self.config.ui.window_width = w;
            self.config.ui.window_height = h;
            let _ = self.config.save();
        }
    }

    fn refresh_midi_ports(&mut self) {
        self.midi_port_names = midi::list_ports().unwrap_or_default();
    }

    fn refresh_sample_rates(&mut self) {
        if let Some((host_id, _)) = self.available_hosts.get(self.selected_host_idx) {
            self.supported_sample_rates = crate::audio::supported_sample_rates(*host_id);
            self.is_jack = self.available_hosts[self.selected_host_idx].1 == "JACK";
        }
    }

}
