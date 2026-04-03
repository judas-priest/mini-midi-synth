/// GUI interface using egui/eframe.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cpal::HostId;
use eframe::egui;
use rtrb::Producer;

use crate::cc_map::CcMap;
use crate::config::Config;
use crate::midi::{self, NoteState};
use crate::preset::{self, Preset, DrumKit, Performance, PartConfig};
use crate::synth::{ControlEvent, DrumParam, ParamFeedback, MAX_LAYERS};
use crate::synth::drum::{NUM_DRUM_SLOTS, DRUM_NAMES, DrumSlotParams, DrumPattern};
use crate::synth::looper::LooperAtoms;

const VELOCITY_CURVE_NAMES: &[&str] = &["Linear", "Exponential", "Logarithmic", "Fixed"];
const LFO_WAVEFORM_NAMES: &[&str] = &["Sine", "Triangle", "Square", "Sample & Hold"];
const PORTAMENTO_MODE_NAMES: &[&str] = &["Off", "Always", "Legato"];

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
const SIMPLE_OSC_NAMES: &[&str] = &["Sine", "Saw", "Square", "Triangle", "FM"];
const FILTER_NAMES: &[&str] = &["LowPass", "HighPass", "BandPass", "Formant", "Moog 24dB", "Moog 12dB", "Diode 18dB"];
const FILTER_ROUTING_NAMES: &[&str] = &["Single", "Serial", "Parallel"];
const FORMANT_VOICE_NAMES: &[&str] = &["Bass", "Tenor", "Alto", "Soprano"];
const FORMANT_VOWEL_NAMES: &[&str] = &["A (ah)", "E (eh)", "I (ee)", "O (oh)", "U (oo)"];
const LAYER_NAMES: &[&str] = &["A", "B"];

fn buffer_label(size: u32) -> String {
    if size == 0 { "Default".to_string() } else { format!("{size}") }
}

/// Per-layer GUI state
pub struct LayerState {
    pub preset_idx: usize,
    pub edited_params: std::collections::BTreeMap<String, f32>,
    pub params_dirty: bool,
    pub enabled: bool,
    pub volume: f32,
    pub min_note: u8,
    pub max_note: u8,
    pub sf2_mode: bool,
    pub sf2_program: u8,
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
    pub presets: Vec<Preset>,
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
    pub layers: Vec<LayerState>,
    /// Currently edited layer (0 = A, 1 = B)
    pub active_layer: usize,

    pub on_midi_reconnect: Option<Box<dyn FnMut(usize) -> Result<(), String>>>,

    /// Collapsed preset categories
    pub collapsed_categories: HashSet<String>,

    /// Feedback from audio thread
    pub feedback_rx: Option<rtrb::Consumer<ParamFeedback>>,
    /// CC mapping
    pub cc_map: CcMap,
    /// MIDI Learn target parameter key
    pub midi_learn_target: Option<String>,
    /// Program change atom for preset feedback
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
    pub looper_bars: u8,
    /// 0 = SEQ buttons → drums, 1 = SEQ buttons → looper
    pub seq_target_atom: std::sync::Arc<std::sync::atomic::AtomicU8>,
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
    // Pitch step sequencer
    pub pitch_seq_step_atoms: Vec<std::sync::Arc<std::sync::atomic::AtomicU8>>,
    pub pitch_seq_enabled: [bool; 2],
    pub pitch_seq_steps: [[crate::synth::step_seq::PitchStep; 16]; 2],
    pub pitch_seq_length: [u8; 2],
    pub pitch_seq_rate: [u8; 2],
    pub pitch_seq_scale: [u8; 2],
    pub pitch_seq_swing: [f32; 2],
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
        if has_active_notes || self.drum_playing || looper_active {
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

        // Right: preset list (for active layer) or GM instrument list (SF2 mode)
        egui::SidePanel::right("preset_panel")
            .resizable(true)
            .default_width(160.0)
            .min_width(120.0)
            .show(ctx, |ui| {
                let layer = self.active_layer;
                let is_sf2 = self.layers[layer].sf2_mode && self.sf2_keys_soundfont.is_some();
                let layer_label = LAYER_NAMES.get(layer).unwrap_or(&"?");

                ui.add_space(4.0);
                if is_sf2 {
                    ui.strong(format!("GM Instruments (Layer {layer_label})"));
                } else {
                    ui.strong(format!("Presets (Layer {layer_label})"));
                }
                ui.add_space(4.0);
                ui.separator();

                if is_sf2 {
                    // GM instrument list with same category style as synth presets
                    use crate::synth::sampler::GM_PROGRAM_NAMES;
                    let current_program = self.layers[layer].sf2_program;
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
                                let is_collapsed = self.collapsed_categories.contains(cat_name);
                                let arrow = if is_collapsed { "\u{25B6}" } else { "\u{25BC}" };
                                if ui.selectable_label(false, format!("{arrow} {cat_name}")).clicked() {
                                    toggled_cat = Some(cat_name);
                                }
                                if !is_collapsed {
                                    for i in start..end {
                                        let name = GM_PROGRAM_NAMES[i];
                                        let selected = current_program == i as u8;
                                        if ui.selectable_label(selected, format!("  {name}")).clicked() && !selected {
                                            self.layers[layer].sf2_program = i as u8;
                                            let _ = self.ctrl_tx.push(ControlEvent::SetLayerSf2Program {
                                                layer, program: i as u8, bank: 0,
                                            });
                                            if layer == 0 { self.config.sf2.layer_a_program = i as u8; }
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
                    // Synth preset list
                    let current_preset_idx = self.layers[layer].preset_idx;
                    let mut new_idx: Option<usize> = None;
                    let mut toggled_category: Option<String> = None;

                    let categories: Vec<(String, usize)> = {
                        let mut cats = Vec::new();
                        let mut last = String::new();
                        for (i, p) in self.presets.iter().enumerate() {
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
                            for (i, preset) in self.presets.iter().enumerate() {
                                if cat_idx < categories.len() && categories[cat_idx].1 == i {
                                    let cat = &categories[cat_idx].0;
                                    let is_collapsed = self.collapsed_categories.contains(cat);
                                    let arrow = if is_collapsed { "\u{25B6}" } else { "\u{25BC}" };
                                    if ui.selectable_label(false, format!("{arrow} {cat}")).clicked() {
                                        toggled_category = Some(cat.clone());
                                    }
                                    cat_idx += 1;
                                }
                                if !self.collapsed_categories.contains(&preset.category) {
                                    let selected = i == current_preset_idx;
                                    if ui.selectable_label(selected, format!("  {}", preset.name)).clicked() && !selected {
                                        new_idx = Some(i);
                                    }
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
                        self.layers[layer].preset_idx = idx;
                        self.load_edited_params(layer);
                        self.send_edited_params(layer);
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

        // Central: layer tabs + parameters / drum sequencer
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            self.draw_layer_tabs(ui);
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(6.0);
            if self.show_drums {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    self.draw_drum_sequencer(ui);
                });
            } else {
                self.draw_looper(ui);
                ui.add_space(2.0);
                self.draw_pitch_sequencer(ui);
                ui.separator();
                ui.add_space(6.0);
                let layer = self.active_layer;
                let is_sf2 = self.layers[layer].sf2_mode;
                // Synth / SF2 toggle
                if self.sf2_keys_soundfont.is_some() {
                    ui.horizontal(|ui| {
                        ui.label("Mode:");
                        let mut sf2 = is_sf2;
                        if ui.selectable_label(!sf2, "Synth").clicked() { sf2 = false; }
                        if ui.selectable_label(sf2, "SF2").clicked() { sf2 = true; }
                        if sf2 != is_sf2 {
                            self.layers[layer].sf2_mode = sf2;
                            let _ = self.ctrl_tx.push(ControlEvent::SetLayerSf2Mode { layer, enabled: sf2 });
                            // Save to config
                            if layer == 0 { self.config.sf2.layer_a_sf2 = sf2; }
                            else { self.config.sf2.layer_b_sf2 = sf2; }
                            let _ = self.config.save();
                        }
                    });
                }
                if is_sf2 && self.sf2_keys_soundfont.is_some() {
                    use crate::synth::sampler::GM_PROGRAM_NAMES;
                    let prog = self.layers[layer].sf2_program;
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
    /// Send initial presets to all layers at startup
    pub fn send_initial_presets(&mut self) {
        for i in 0..self.layers.len() {
            self.send_edited_params(i);
        }
        // Send global params (volume, tone) to engine
        for (key, val) in &self.global_params {
            if let Some(static_key) = crate::cc_map::resolve_key(key) {
                let _ = self.ctrl_tx.push(ControlEvent::SetGlobalParam { key: static_key, value: *val });
            }
        }
        // Ensure config has global params persisted (first run migration)
        self.save_global_to_config();

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
        self.config.ui.pad_perf_map = self.pad_perf_map.to_vec();
        let _ = self.config.save();
        self.last_config_save = std::time::Instant::now();
    }

    /// Load edited_params from the current preset for a given layer.
    /// Preserves global params (effects, volume) so they don't reset on preset change.
    pub fn load_edited_params(&mut self, layer: usize) {
        let preset_idx = self.layers[layer].preset_idx;
        if let Some(p) = self.presets.get(preset_idx) {
            self.layers[layer].edited_params = p.params.clone();
        }
        // Global params (volume, tone) are NOT in presets — don't insert them
        self.layers[layer].params_dirty = false;
    }

    /// Send edited params to synth engine for a given layer
    fn send_edited_params(&mut self, layer: usize) {
        let params = crate::synth::PresetParams::from_map(&self.layers[layer].edited_params);
        let _ = self.ctrl_tx.push(ControlEvent::LoadPreset { layer, params });
    }

    fn send_layer_enabled(&mut self, layer: usize) {
        let enabled = self.layers[layer].enabled;
        let _ = self.ctrl_tx.push(ControlEvent::SetLayerEnabled { layer, enabled });
    }

    fn send_layer_volume(&mut self, layer: usize) {
        let volume = self.layers[layer].volume;
        let _ = self.ctrl_tx.push(ControlEvent::SetLayerVolume { layer, volume });
    }

    fn send_layer_range(&mut self, layer: usize) {
        let min_note = self.layers[layer].min_note;
        let max_note = self.layers[layer].max_note;
        let _ = self.ctrl_tx.push(ControlEvent::SetLayerRange { layer, min_note, max_note });
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
        // Apply per-layer SF2 modes
        for i in 0..2 {
            if self.layers[i].sf2_mode {
                let _ = self.ctrl_tx.push(ControlEvent::SetLayerSf2Mode { layer: i, enabled: true });
                let _ = self.ctrl_tx.push(ControlEvent::SetLayerSf2Program {
                    layer: i, program: self.layers[i].sf2_program, bank: 0,
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
            self.layers[i].sf2_mode = false;
            let _ = self.ctrl_tx.push(ControlEvent::SetLayerSf2Mode { layer: i, enabled: false });
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
            .presets
            .get(self.layers[0].preset_idx)
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
            if i >= self.layers.len() { break; }
            let preset_idx = self.presets.iter()
                .position(|p| p.name == part.preset_name)
                .unwrap_or(0);
            self.layers[i].preset_idx = preset_idx;
            self.layers[i].enabled = part.enabled;
            self.layers[i].volume = part.volume;
            self.layers[i].min_note = part.key_low;
            self.layers[i].max_note = part.key_high;
            self.layers[i].edited_params = part.param_overrides.clone();
            self.layers[i].sf2_mode = part.sf2_mode;
            self.layers[i].sf2_program = part.sf2_program;
            self.send_edited_params(i);
            self.send_layer_enabled(i);
            self.send_layer_volume(i);
            self.send_layer_range(i);
            // Restore SF2 mode for this layer
            let _ = self.ctrl_tx.push(ControlEvent::SetLayerSf2Mode { layer: i, enabled: part.sf2_mode });
            if part.sf2_mode {
                let _ = self.ctrl_tx.push(ControlEvent::SetLayerSf2Program {
                    layer: i, program: part.sf2_program, bank: 0,
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

    fn draw_layer_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for i in 0..MAX_LAYERS {
                let label = *LAYER_NAMES.get(i).unwrap_or(&"?");
                let is_active = i == self.active_layer && !self.show_drums && !self.show_looper;
                let is_enabled = self.layers[i].enabled;

                let text = if is_enabled {
                    let range = format!("{}-{}", note_name(self.layers[i].min_note), note_name(self.layers[i].max_note));
                    format!("Layer {label} [{range}]")
                } else {
                    format!("Layer {label} (off)")
                };

                if ui.selectable_label(is_active, &text).clicked() {
                    self.active_layer = i;
                    self.show_drums = false;
                    self.show_looper = false;
                    self.seq_target_atom.store(1, Ordering::Relaxed); // synth → looper
                }
            }

            // Drums tab
            if ui.selectable_label(self.show_drums, "Drums").clicked() {
                self.show_drums = true;
                self.show_looper = false;
                self.seq_target_atom.store(0, Ordering::Relaxed); // drums
            }

            // Looper tab removed — controls are now inline in synth panel

            ui.separator();

            // Layer B enable toggle
            if MAX_LAYERS > 1 {
                let mut layer_b_enabled = self.layers[1].enabled;
                if ui.checkbox(&mut layer_b_enabled, "Split").changed() {
                    self.layers[1].enabled = layer_b_enabled;
                    self.send_layer_enabled(1);
                    if layer_b_enabled {
                        // Default split: A = C-1..B3, B = C4..G9
                        self.layers[0].min_note = 0;
                        self.layers[0].max_note = 59; // B3
                        self.layers[1].min_note = 60; // C4
                        self.layers[1].max_note = 127;
                        self.send_layer_range(0);
                        self.send_layer_range(1);
                    } else {
                        // Disable split: A gets full range
                        self.layers[0].min_note = 0;
                        self.layers[0].max_note = 127;
                        self.send_layer_range(0);
                    }
                }
            }

            // Volume for active layer
            ui.separator();
            let layer = self.active_layer;
            let mut vol = self.layers[layer].volume;
            ui.label("Vol:");
            if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(true)).changed() {
                self.layers[layer].volume = vol;
                self.send_layer_volume(layer);
            }
        });

        // Split point slider (when split enabled)
        if self.layers[1].enabled {
            ui.horizontal(|ui| {
                ui.label("Split point:");
                let mut split = self.layers[1].min_note;
                let label = note_name(split);
                if ui.add(egui::Slider::new(&mut split, 24..=96).text(label)).changed() {
                    self.layers[0].max_note = split.saturating_sub(1);
                    self.layers[1].min_note = split;
                    self.send_layer_range(0);
                    self.send_layer_range(1);
                }
            });
        }

        // Performance save/load
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("Perf:");
            ui.add(egui::TextEdit::singleline(&mut self.perf_name).desired_width(120.0).hint_text("name"));
            if ui.button("Save").clicked() && !self.perf_name.trim().is_empty() {
                let parts: Vec<PartConfig> = self.layers.iter().enumerate().map(|(i, l)| {
                    let preset_name = self.presets.get(l.preset_idx)
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    PartConfig {
                        preset_name,
                        enabled: l.enabled || i == 0,
                        volume: l.volume,
                        key_low: l.min_note,
                        key_high: l.max_note,
                        param_overrides: l.edited_params.clone(),
                        sf2_mode: l.sf2_mode,
                        sf2_program: l.sf2_program,
                    }
                }).collect();
                let perf = Performance {
                    name: self.perf_name.trim().to_string(),
                    category: String::new(),
                    parts,
                };
                match preset::save_performance(&perf) {
                    Ok(path) => {
                        self.perf_status = format!("Saved: {}", path.display());
                        self.perf_list = preset::list_performances();
                    }
                    Err(e) => { self.perf_status = format!("Error: {e}"); }
                }
            }
            ui.separator();
            let mut load_perf: Option<std::path::PathBuf> = None;
            egui::ComboBox::from_id_salt("perf_load")
                .selected_text(if self.perf_list.is_empty() { "No performances" } else { "Load..." })
                .width(140.0)
                .show_ui(ui, |ui| {
                    for (name, path) in &self.perf_list {
                        if ui.selectable_label(false, name).clicked() {
                            load_perf = Some(path.clone());
                        }
                    }
                });
            if let Some(path) = load_perf {
                match preset::load_performance(&path) {
                    Ok(perf) => {
                        self.perf_name = perf.name.clone();
                        for (i, part) in perf.parts.iter().enumerate() {
                            if i >= self.layers.len() { break; }
                            // Find preset by name
                            let preset_idx = self.presets.iter()
                                .position(|p| p.name == part.preset_name)
                                .unwrap_or(0);
                            self.layers[i].preset_idx = preset_idx;
                            self.layers[i].enabled = part.enabled;
                            self.layers[i].volume = part.volume;
                            self.layers[i].min_note = part.key_low;
                            self.layers[i].max_note = part.key_high;
                            self.layers[i].edited_params = part.param_overrides.clone();
                            self.layers[i].sf2_mode = part.sf2_mode;
                            self.layers[i].sf2_program = part.sf2_program;
                            self.send_edited_params(i);
                            self.send_layer_enabled(i);
                            self.send_layer_volume(i);
                            self.send_layer_range(i);
                            let _ = self.ctrl_tx.push(ControlEvent::SetLayerSf2Mode { layer: i, enabled: part.sf2_mode });
                            if part.sf2_mode {
                                let _ = self.ctrl_tx.push(ControlEvent::SetLayerSf2Program {
                                    layer: i, program: part.sf2_program, bank: 0,
                                });
                            }
                        }
                        self.perf_status = format!("Loaded: {}", perf.name);
                    }
                    Err(e) => { self.perf_status = format!("Error: {e}"); }
                }
            }
            if !self.perf_status.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new(&self.perf_status).small().weak());
            }
        });
    }

    fn draw_params_editable(&mut self, ui: &mut egui::Ui) {
        let layer = self.active_layer;
        let preset_idx = self.layers[layer].preset_idx;
        let preset_name = self
            .presets
            .get(preset_idx)
            .map(|p| p.name.as_str())
            .unwrap_or("(none)");

        let layer_label = LAYER_NAMES.get(layer).unwrap_or(&"?");
        ui.strong(format!("{preset_name} (Layer {layer_label})"));
        ui.add_space(6.0);

        let mut changed = false;

        // Oscillator 1
        ui.strong("Oscillator 1");
        ui.horizontal(|ui| {
            let mut osc = self.layers[layer].edited_params.get("osc_type").copied().unwrap_or(0.0) as usize;
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("osc_type_{layer}"))
                .selected_text(*OSC_NAMES.get(osc).unwrap_or(&"?"))
                .show_ui(ui, |ui| {
                    for (i, name) in OSC_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut osc, i, *name).changed() {
                            self.layers[layer].edited_params.insert("osc_type".into(), osc as f32);
                            changed = true;
                        }
                    }
                });

            let mut detune = self.layers[layer].edited_params.get("osc_detune").copied().unwrap_or(0.0);
            ui.label("Detune:");
            if ui.add(egui::Slider::new(&mut detune, 0.0..=0.05).step_by(0.001)).changed() {
                self.layers[layer].edited_params.insert("osc_detune".into(), detune);
                changed = true;
            }
        });

        let osc_type = self.layers[layer].edited_params.get("osc_type").copied().unwrap_or(0.0) as u32;
        let osc_is_simple = osc_type <= 4;

        // Noise level (for all osc types except pure Noise)
        if osc_type != 5 {
            changed |= self.param_slider(ui, "noise_level", "Noise Mix", 0.0, 1.0, false);
        }

        // FM params (FM and FM Piano)
        if osc_type == 4 || osc_type == 8 {
            changed |= self.param_slider(ui, "fm_ratio", "FM Ratio", 0.5, 8.0, false);
            changed |= self.param_slider(ui, "fm_index", "FM Index", 0.1, 10.0, false);
            if osc_type == 4 {
                changed |= self.param_slider(ui, "fm_env_amount", "FM Env Amount", 0.0, 1.0, false);
            }
        }

        // Physical model params (KS, Commuted Piano, Banded WG)
        if matches!(osc_type, 6 | 9 | 10) {
            changed |= self.param_slider(ui, "ks_brightness", "Brightness", 0.05, 1.0, false);
            changed |= self.param_slider(ui, "ks_feedback", "Feedback", 0.9, 0.9999, false);
        }

        // Organ drawbar params
        if osc_type == 7 {
            ui.add_space(4.0);
            ui.strong("Drawbars");
            let drawbar_names = ["16'", "5⅓'", "8'", "4'", "2⅔'", "2'", "1⅗'", "1⅓'", "1'"];
            for (i, name) in drawbar_names.iter().enumerate() {
                let key = format!("drawbar_{}", i + 1);
                let mut val = self.layers[layer].edited_params.get(&key).copied().unwrap_or(0.0);
                if ui.add(egui::Slider::new(&mut val, 0.0..=8.0).step_by(1.0).text(*name)).changed() {
                    self.layers[layer].edited_params.insert(key, val);
                    changed = true;
                }
            }
        }

        // Drum synth params
        if osc_type == 12 {
            ui.add_space(4.0);
            ui.strong("Drum Synth");
            changed |= self.param_slider(ui, "drum_pitch_amount", "Pitch Sweep (st)", 0.0, 72.0, false);
            changed |= self.param_slider(ui, "drum_pitch_decay", "Pitch Decay", 5.0, 200.0, false);
            changed |= self.param_slider(ui, "drum_noise_level", "Noise Level", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "drum_noise_decay", "Noise Decay", 5.0, 200.0, false);
            changed |= self.param_slider(ui, "drum_noise_color", "Noise Color", 0.0, 1.0, false);
        }

        // Bass guitar params
        if osc_type == 13 {
            ui.add_space(4.0);
            ui.strong("Bass Guitar");
            ui.horizontal(|ui| {
                let mut style = self.layers[layer].edited_params.get("bass_style").copied().unwrap_or(0.0) as usize;
                ui.label("Style:");
                egui::ComboBox::from_id_salt(format!("bass_style_{layer}"))
                    .selected_text(*BASS_STYLE_NAMES.get(style).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BASS_STYLE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut style, i, *name).changed() {
                                self.layers[layer].edited_params.insert("bass_style".into(), style as f32);
                                changed = true;
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                let mut pu = self.layers[layer].edited_params.get("bass_pickup").copied().unwrap_or(0.0) as usize;
                ui.label("Pickup:");
                egui::ComboBox::from_id_salt(format!("bass_pickup_{layer}"))
                    .selected_text(*BASS_PICKUP_NAMES.get(pu).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BASS_PICKUP_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut pu, i, *name).changed() {
                                self.layers[layer].edited_params.insert("bass_pickup".into(), pu as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "bass_tone", "Tone", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "bass_body", "Body", 0.0, 1.0, false);
        }

        // Bowed string params
        if osc_type == 14 {
            ui.add_space(4.0);
            ui.strong("Bowed String");
            ui.horizontal(|ui| {
                let mut bt = self.layers[layer].edited_params.get("body_type").copied().unwrap_or(0.0) as usize;
                ui.label("Body:");
                egui::ComboBox::from_id_salt(format!("body_type_{layer}"))
                    .selected_text(*BOWED_BODY_NAMES.get(bt).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BOWED_BODY_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut bt, i, *name).changed() {
                                self.layers[layer].edited_params.insert("body_type".into(), bt as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "bow_pressure", "Bow Pressure", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "bow_position", "Bow Position", 0.0, 1.0, false);
        }

        // Brass params
        if osc_type == 15 {
            ui.add_space(4.0);
            ui.strong("Brass");
            ui.horizontal(|ui| {
                let mut bt = self.layers[layer].edited_params.get("bell_type").copied().unwrap_or(0.0) as usize;
                ui.label("Type:");
                egui::ComboBox::from_id_salt(format!("bell_type_{layer}"))
                    .selected_text(*BRASS_BELL_NAMES.get(bt).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BRASS_BELL_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut bt, i, *name).changed() {
                                self.layers[layer].edited_params.insert("bell_type".into(), bt as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "lip_tension", "Lip Tension", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "blowing_pressure", "Blowing Pressure", 0.0, 1.0, false);
        }

        // Accordion params
        if osc_type == 22 {
            ui.add_space(4.0);
            ui.strong("Accordion");
            ui.horizontal(|ui| {
                let mut reg = self.layers[layer].edited_params.get("accordion_register").copied().unwrap_or(0.0) as usize;
                ui.label("Register:");
                egui::ComboBox::from_id_salt(format!("accordion_reg_{layer}"))
                    .selected_text(*ACCORDION_REGISTER_NAMES.get(reg).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in ACCORDION_REGISTER_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut reg, i, *name).changed() {
                                self.layers[layer].edited_params.insert("accordion_register".into(), reg as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "accordion_bellows", "Bellows Pressure", 0.0, 1.0, false);
        }

        // Saxophone params
        if osc_type == 23 {
            ui.add_space(4.0);
            ui.strong("Saxophone");
            ui.horizontal(|ui| {
                let mut st = self.layers[layer].edited_params.get("sax_type").copied().unwrap_or(1.0) as usize;
                ui.label("Type:");
                egui::ComboBox::from_id_salt(format!("sax_type_{layer}"))
                    .selected_text(*SAX_TYPE_NAMES.get(st).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in SAX_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut st, i, *name).changed() {
                                self.layers[layer].edited_params.insert("sax_type".into(), st as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "sax_reed_stiffness", "Reed Stiffness", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "sax_embouchure", "Embouchure", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "sax_blow_pressure", "Blow Pressure", 0.0, 1.0, false);
        }

        // Electric Piano params
        if osc_type == 24 {
            ui.add_space(4.0);
            ui.strong("Electric Piano");
            ui.horizontal(|ui| {
                let mut ep_t = self.layers[layer].edited_params.get("epiano_type").copied().unwrap_or(0.0) as usize;
                ui.label("Model:");
                egui::ComboBox::from_id_salt(format!("epiano_type_{layer}"))
                    .selected_text(*["Rhodes MkII", "Wurlitzer 200A", "Stage 73"].get(ep_t).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in ["Rhodes MkII", "Wurlitzer 200A", "Stage 73"].iter().enumerate() {
                            if ui.selectable_value(&mut ep_t, i, *name).changed() {
                                self.layers[layer].edited_params.insert("epiano_type".into(), ep_t as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "ks_brightness", "Brightness (Strike)", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "ks_feedback", "Sustain", 0.0, 1.0, false);
        }

        // Square wave — pulse width control
        if osc_type == 2 {
            let pw = self.layers[layer].edited_params.get("pulse_width").copied().unwrap_or(0.5);
            let mut pw_val = pw;
            ui.horizontal(|ui| {
                ui.label("Pulse Width");
                if ui.add(egui::Slider::new(&mut pw_val, 0.05..=0.95)).changed() {
                    self.layers[layer].edited_params.insert("pulse_width".into(), pw_val);
                    self.layers[layer].params_dirty = true;
                }
            });
        }

        // Phase Distortion params
        if osc_type == 16 {
            ui.add_space(4.0);
            ui.strong("Phase Distortion");
            ui.horizontal(|ui| {
                let mut shape = self.layers[layer].edited_params.get("pd_shape").copied().unwrap_or(0.0) as usize;
                ui.label("Shape:");
                egui::ComboBox::from_id_salt(format!("pd_shape_{layer}"))
                    .selected_text(*PD_SHAPE_NAMES.get(shape).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in PD_SHAPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut shape, i, *name).changed() {
                                self.layers[layer].edited_params.insert("pd_shape".into(), shape as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "pd_depth", "Depth (DCW)", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "pd_env_amount", "Env → Depth", 0.0, 1.0, false);
        }

        // Wavefolder params
        if osc_type == 17 {
            ui.add_space(4.0);
            ui.strong("Wavefolder");
            ui.horizontal(|ui| {
                let mut src = self.layers[layer].edited_params.get("fold_source").copied().unwrap_or(0.0) as usize;
                ui.label("Source:");
                egui::ComboBox::from_id_salt(format!("fold_source_{layer}"))
                    .selected_text(*FOLD_SOURCE_NAMES.get(src).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FOLD_SOURCE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut src, i, *name).changed() {
                                self.layers[layer].edited_params.insert("fold_source".into(), src as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "fold_amount", "Fold Amount", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "fold_symmetry", "Symmetry", 0.0, 1.0, false);
        }

        // Modal Resonator params
        if osc_type == 18 {
            ui.add_space(4.0);
            ui.strong("Modal Resonator");
            ui.horizontal(|ui| {
                let mut mat = self.layers[layer].edited_params.get("modal_material").copied().unwrap_or(0.0) as usize;
                ui.label("Material:");
                egui::ComboBox::from_id_salt(format!("modal_material_{layer}"))
                    .selected_text(*MODAL_MATERIAL_NAMES.get(mat).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in MODAL_MATERIAL_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut mat, i, *name).changed() {
                                self.layers[layer].edited_params.insert("modal_material".into(), mat as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "modal_brightness", "Brightness", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "modal_damping", "Damping", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "modal_strike_pos", "Strike Position", 0.0, 1.0, false);
        }

        // Hard Sync params
        if osc_type == 19 {
            ui.add_space(4.0);
            ui.label("Hard Sync");
            ui.horizontal(|ui| {
                let mut shape = self.layers[layer].edited_params.get("sync_shape").copied().unwrap_or(0.0) as usize;
                ui.label("Slave Wave:");
                egui::ComboBox::from_id_salt(format!("sync_shape_{layer}"))
                    .selected_text(*SYNC_SHAPE_NAMES.get(shape).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in SYNC_SHAPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut shape, i, *name).changed() {
                                self.layers[layer].edited_params.insert("sync_shape".into(), shape as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "sync_ratio", "Sync Ratio", 1.0, 16.0, false);
        }

        // Supersaw params
        if osc_type == 20 {
            ui.add_space(4.0);
            ui.label("Supersaw");
            changed |= self.param_slider(ui, "supersaw_detune", "Detune", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "supersaw_mix", "Mix", 0.0, 1.0, false);
        }

        // Multi-osc controls (only for simple osc types)
        if osc_is_simple {
            ui.add_space(4.0);
            let mut osc_count = self.layers[layer].edited_params.get("osc_count").copied().unwrap_or(1.0) as u32;
            ui.horizontal(|ui| {
                ui.label("Osc Count:");
                if ui.add(egui::Slider::new(&mut osc_count, 1..=3)).changed() {
                    self.layers[layer].edited_params.insert("osc_count".into(), osc_count as f32);
                    changed = true;
                }
            });

            if osc_count >= 1 {
                changed |= self.param_slider(ui, "osc1_level", "Osc 1 Level", 0.0, 1.0, false);
            }

            if osc_count >= 2 {
                ui.add_space(4.0);
                ui.strong("Oscillator 2");
                ui.horizontal(|ui| {
                    let mut osc2 = self.layers[layer].edited_params.get("osc2_type").copied().unwrap_or(1.0) as usize;
                    ui.label("Type:");
                    egui::ComboBox::from_id_salt(format!("osc2_type_{layer}"))
                        .selected_text(*SIMPLE_OSC_NAMES.get(osc2).unwrap_or(&"?"))
                        .show_ui(ui, |ui| {
                            for (i, name) in SIMPLE_OSC_NAMES.iter().enumerate() {
                                if ui.selectable_value(&mut osc2, i, *name).changed() {
                                    self.layers[layer].edited_params.insert("osc2_type".into(), osc2 as f32);
                                    changed = true;
                                }
                            }
                        });

                    let mut detune2 = self.layers[layer].edited_params.get("osc2_detune").copied().unwrap_or(0.0);
                    ui.label("Detune:");
                    if ui.add(egui::Slider::new(&mut detune2, -0.05..=0.05).step_by(0.001)).changed() {
                        self.layers[layer].edited_params.insert("osc2_detune".into(), detune2);
                        changed = true;
                    }
                });
                changed |= self.param_slider(ui, "osc2_level", "Osc 2 Level", 0.0, 1.0, false);
            }

            if osc_count >= 3 {
                ui.add_space(4.0);
                ui.strong("Oscillator 3");
                ui.horizontal(|ui| {
                    let mut osc3 = self.layers[layer].edited_params.get("osc3_type").copied().unwrap_or(1.0) as usize;
                    ui.label("Type:");
                    egui::ComboBox::from_id_salt(format!("osc3_type_{layer}"))
                        .selected_text(*SIMPLE_OSC_NAMES.get(osc3).unwrap_or(&"?"))
                        .show_ui(ui, |ui| {
                            for (i, name) in SIMPLE_OSC_NAMES.iter().enumerate() {
                                if ui.selectable_value(&mut osc3, i, *name).changed() {
                                    self.layers[layer].edited_params.insert("osc3_type".into(), osc3 as f32);
                                    changed = true;
                                }
                            }
                        });

                    let mut detune3 = self.layers[layer].edited_params.get("osc3_detune").copied().unwrap_or(0.0);
                    ui.label("Detune:");
                    if ui.add(egui::Slider::new(&mut detune3, -0.05..=0.05).step_by(0.001)).changed() {
                        self.layers[layer].edited_params.insert("osc3_detune".into(), detune3);
                        changed = true;
                    }
                });
                changed |= self.param_slider(ui, "osc3_level", "Osc 3 Level", 0.0, 1.0, false);
            }
        }

        ui.add_space(6.0);

        // Filter 1
        ui.strong("Filter 1");
        ui.horizontal(|ui| {
            let mut ft = self.layers[layer].edited_params.get("filter_type").copied().unwrap_or(0.0) as usize;
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("filter_type_{layer}"))
                .selected_text(*FILTER_NAMES.get(ft).unwrap_or(&"?"))
                .show_ui(ui, |ui| {
                    for (i, name) in FILTER_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut ft, i, *name).changed() {
                            self.layers[layer].edited_params.insert("filter_type".into(), ft as f32);
                            changed = true;
                        }
                    }
                });
        });

        let filter_type = self.layers[layer].edited_params.get("filter_type").copied().unwrap_or(0.0) as u32;
        let is_formant = filter_type == 3;

        if is_formant {
            // Formant filter controls
            ui.horizontal(|ui| {
                let mut fv = self.layers[layer].edited_params.get("formant_voice").copied().unwrap_or(0.0) as usize;
                ui.label("Voice:");
                egui::ComboBox::from_id_salt(format!("formant_voice_{layer}"))
                    .selected_text(*FORMANT_VOICE_NAMES.get(fv).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FORMANT_VOICE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut fv, i, *name).changed() {
                                self.layers[layer].edited_params.insert("formant_voice".into(), fv as f32);
                                changed = true;
                            }
                        }
                    });

                let mut vw = self.layers[layer].edited_params.get("formant_vowel").copied().unwrap_or(0.0) as usize;
                ui.label("Vowel:");
                egui::ComboBox::from_id_salt(format!("formant_vowel_{layer}"))
                    .selected_text(*FORMANT_VOWEL_NAMES.get(vw).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FORMANT_VOWEL_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut vw, i, *name).changed() {
                                self.layers[layer].edited_params.insert("formant_vowel".into(), vw as f32);
                                changed = true;
                            }
                        }
                    });
            });
        } else {
            changed |= self.param_slider(ui, "filter_cutoff", "Cutoff", 20.0, 20000.0, true);
            changed |= self.param_slider(ui, "filter_resonance", "Resonance", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "filter_env_amount", "Env Amount", 0.0, 15000.0, false);
            changed |= self.param_slider(ui, "filter_key_track", "Key Track", 0.0, 1.0, false);
        }

        // Filter routing (not available with formant filter)
        if !is_formant {
            ui.add_space(4.0);
            let mut routing = self.layers[layer].edited_params.get("filter_routing").copied().unwrap_or(0.0) as usize;
            ui.horizontal(|ui| {
                ui.label("Routing:");
                egui::ComboBox::from_id_salt(format!("filter_routing_{layer}"))
                    .selected_text(*FILTER_ROUTING_NAMES.get(routing).unwrap_or(&"Single"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FILTER_ROUTING_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut routing, i, *name).changed() {
                                self.layers[layer].edited_params.insert("filter_routing".into(), routing as f32);
                                changed = true;
                            }
                        }
                    });
            });

            // Filter 2 controls (only when routing != Single)
            if routing >= 1 {
                ui.add_space(4.0);
                ui.strong("Filter 2");
                ui.horizontal(|ui| {
                    let ft2 = self.layers[layer].edited_params.get("filter2_type").copied().unwrap_or(0.0) as usize;
                    ui.label("Type:");
                    // Filter 2: LP/HP/BP + Moog (skip Formant index 3)
                    let f2_names: &[&str] = &["LowPass", "HighPass", "BandPass", "Moog 24dB", "Moog 12dB", "Diode 18dB"];
                    let f2_values: &[usize] = &[0, 1, 2, 4, 5, 6]; // maps to FilterType param values
                    let f2_display = f2_values.iter().position(|&v| v == ft2).unwrap_or(0);
                    let mut f2_sel = f2_display;
                    egui::ComboBox::from_id_salt(format!("filter2_type_{layer}"))
                        .selected_text(*f2_names.get(f2_sel).unwrap_or(&"LowPass"))
                        .show_ui(ui, |ui| {
                            for (i, name) in f2_names.iter().enumerate() {
                                if ui.selectable_value(&mut f2_sel, i, *name).changed() {
                                    let param_val = f2_values[f2_sel];
                                    self.layers[layer].edited_params.insert("filter2_type".into(), param_val as f32);
                                    changed = true;
                                }
                            }
                        });
                });
                changed |= self.param_slider(ui, "filter2_cutoff", "Cutoff 2", 20.0, 20000.0, true);
                changed |= self.param_slider(ui, "filter2_resonance", "Resonance 2", 0.0, 1.0, false);
            }
        }

        ui.add_space(6.0);

        // Amp Envelope
        ui.strong("Amp Envelope");
        changed |= self.param_slider(ui, "amp_attack", "Attack", 0.001, 5.0, true);
        changed |= self.param_slider(ui, "amp_decay", "Decay", 0.0, 5.0, false);
        changed |= self.param_slider(ui, "amp_sustain", "Sustain", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "amp_release", "Release", 0.001, 5.0, true);

        ui.add_space(6.0);

        // Filter Envelope
        ui.strong("Filter Envelope");
        changed |= self.param_slider(ui, "filter_attack", "Attack", 0.001, 5.0, true);
        changed |= self.param_slider(ui, "filter_decay", "Decay", 0.0, 5.0, false);
        changed |= self.param_slider(ui, "filter_sustain", "Sustain", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "filter_release", "Release", 0.001, 5.0, true);

        ui.add_space(6.0);

        // Dynamics
        ui.strong("Dynamics");
        ui.horizontal(|ui| {
            let mut vc = self.layers[layer].edited_params.get("velocity_curve").copied().unwrap_or(0.0) as usize;
            ui.label("Vel Curve:");
            egui::ComboBox::from_id_salt(format!("vel_curve_{layer}"))
                .selected_text(*VELOCITY_CURVE_NAMES.get(vc).unwrap_or(&"Linear"))
                .show_ui(ui, |ui| {
                    for (i, name) in VELOCITY_CURVE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut vc, i, *name).changed() {
                            self.layers[layer].edited_params.insert("velocity_curve".into(), vc as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "vel_to_filter", "Vel->Filter", 0.0, 1.0, false);

        ui.add_space(6.0);

        // LFO
        ui.strong("LFO");
        ui.horizontal(|ui| {
            let mut lw = self.layers[layer].edited_params.get("lfo_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Waveform:");
            egui::ComboBox::from_id_salt(format!("lfo_wf_{layer}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(lw).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut lw, i, *name).changed() {
                            self.layers[layer].edited_params.insert("lfo_waveform".into(), lw as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "lfo_rate", "Rate", 0.1, 20.0, true);
        changed |= self.param_slider(ui, "lfo_pitch_depth", "Pitch Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo_filter_depth", "Filter Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo_amp_depth", "Amp Depth", 0.0, 1.0, false);

        ui.add_space(6.0);

        // Portamento
        ui.strong("Portamento");
        ui.horizontal(|ui| {
            let mut pm = self.layers[layer].edited_params.get("portamento_mode").copied().unwrap_or(0.0) as usize;
            ui.label("Mode:");
            egui::ComboBox::from_id_salt(format!("porta_mode_{layer}"))
                .selected_text(*PORTAMENTO_MODE_NAMES.get(pm).unwrap_or(&"Off"))
                .show_ui(ui, |ui| {
                    for (i, name) in PORTAMENTO_MODE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut pm, i, *name).changed() {
                            self.layers[layer].edited_params.insert("portamento_mode".into(), pm as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "portamento_time", "Time", 0.0, 2.0, false);

        ui.add_space(6.0);

        // Unison
        ui.strong("Unison");
        {
            let mut uv = self.layers[layer].edited_params.get("unison_voices").copied().unwrap_or(1.0) as u32;
            if ui.add(egui::Slider::new(&mut uv, 1..=8).text("Voices")).changed() {
                self.layers[layer].edited_params.insert("unison_voices".into(), uv as f32);
                changed = true;
            }
        }
        changed |= self.param_slider(ui, "unison_detune", "Detune (cents)", 0.0, 50.0, false);
        changed |= self.param_slider(ui, "unison_spread", "Spread", 0.0, 1.0, false);

        ui.add_space(6.0);

        // Effects
        ui.strong("Effects");
        changed |= self.param_slider(ui, "chorus_mix", "Chorus", 0.0, 1.0, false);

        ui.add_space(4.0);
        ui.strong("Delay");
        changed |= self.param_slider(ui, "delay_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "delay_time_l", "Time L", 0.01, 2.0, false);
        changed |= self.param_slider(ui, "delay_time_r", "Time R", 0.01, 2.0, false);
        changed |= self.param_slider(ui, "delay_feedback", "Feedback", 0.0, 0.95, false);
        changed |= self.param_slider(ui, "delay_filter", "Filter", 0.0, 0.95, false);
        {
            let mut pp = self.layers[layer].edited_params.get("delay_ping_pong").copied().unwrap_or(0.0) > 0.5;
            if ui.checkbox(&mut pp, "Ping-Pong").changed() {
                self.layers[layer].edited_params.insert("delay_ping_pong".into(), if pp { 1.0 } else { 0.0 });
                changed = true;
            }
        }

        ui.add_space(4.0);
        ui.strong("Reverb");
        changed |= self.param_slider(ui, "reverb_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "reverb_room_size", "Room Size", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "reverb_damping", "Damping", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "reverb_width", "Width", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "reverb_pre_delay", "Pre-Delay", 0.0, 0.1, false);

        if changed {
            self.layers[layer].params_dirty = true;
            self.send_edited_params(layer);
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if self.layers[layer].params_dirty {
                ui.colored_label(egui::Color32::YELLOW, "Modified");
                ui.separator();
            }
            if ui.button("Save as new preset...").clicked() {
                self.save_as_user_preset();
            }
            if self.layers[layer].params_dirty {
                if ui.button("Reset").clicked() {
                    let l = self.active_layer;
                    self.load_edited_params(l);
                    self.send_edited_params(l);
                }
            }
        });
    }

    fn draw_drum_sequencer(&mut self, ui: &mut egui::Ui) {
        let current_step = self.drum_step_atom.load(Ordering::Relaxed);
        let pat_idx = self.drum_current_pattern as usize;

        // Spacebar = play/pause
        if self.show_drums && ui.input(|i| i.key_pressed(egui::Key::Space)) {
            self.drum_playing = !self.drum_playing;
            let _ = self.ctrl_tx.push(ControlEvent::DrumSeqPlay { playing: self.drum_playing });
        }

        // Transport controls — same layout as looper
        ui.horizontal(|ui| {
            // Play/Stop
            let play_label = if self.drum_playing { "\u{23F9} Stop" } else { "\u{25B6} Play" };
            if ui.button(play_label).clicked() {
                self.drum_playing = !self.drum_playing;
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqPlay { playing: self.drum_playing });
            }

            // Rec
            let rec_label = if self.drum_recording { "\u{23FA} Rec" } else { "\u{26AB} Rec" };
            let rec_color = if self.drum_recording { egui::Color32::RED } else { egui::Color32::GRAY };
            if ui.button(egui::RichText::new(rec_label).color(rec_color)).clicked() {
                self.drum_recording = !self.drum_recording;
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqRecord { recording: self.drum_recording });
            }

            // Undo / Clear
            if ui.button("Undo").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqUndo);
            }
            if ui.button("Clear").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqClear);
            }

            ui.separator();

            // BPM
            ui.label("BPM:");
            if ui.add(egui::DragValue::new(&mut self.drum_bpm).range(40.0..=300.0).speed(0.5)).changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqBpm { bpm: self.drum_bpm });
            }

            ui.separator();

            // Swing + Vol
            ui.label("Swing:");
            if ui.add(egui::Slider::new(&mut self.drum_swing, 0.0..=0.66).show_value(false)).changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqSwing { swing: self.drum_swing });
            }

            ui.separator();
            ui.label("Vol:");
            if ui.add(egui::Slider::new(&mut self.drum_volume, 0.0..=1.0).show_value(false)).changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSetVolume { volume: self.drum_volume });
            }
        });

        // Pattern selector + length
        ui.horizontal(|ui| {
            ui.label("Pattern:");
            for p in 0..8u8 {
                let selected = p == self.drum_current_pattern;
                let label = format!("{}", p + 1);
                if ui.selectable_label(selected, label).clicked() && !selected {
                    self.drum_current_pattern = p;
                    let _ = self.ctrl_tx.push(ControlEvent::DrumSeqPattern { pattern: p });
                }
            }
            ui.separator();
            let mut length = self.drum_patterns[pat_idx].length as i32;
            ui.label("Steps:");
            if ui.add(egui::DragValue::new(&mut length).range(1..=16).speed(0.2)).changed() {
                self.drum_patterns[pat_idx].length = length as u8;
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqLength { length: length as u8 });
            }
        });

        // Save / Load drum kit
        ui.horizontal(|ui| {
            ui.label("Kit:");
            ui.add(egui::TextEdit::singleline(&mut self.drum_kit_name).desired_width(120.0).hint_text("name"));
            if ui.button("Save").clicked() && !self.drum_kit_name.trim().is_empty() {
                let kit = DrumKit {
                    name: self.drum_kit_name.trim().to_string(),
                    patterns: self.drum_patterns.to_vec(),
                    params: self.drum_params.to_vec(),
                    bpm: self.drum_bpm,
                    swing: self.drum_swing,
                    volume: self.drum_volume,
                };
                match preset::save_drum_kit(&kit) {
                    Ok(path) => {
                        self.drum_kit_status = format!("Saved: {}", path.display());
                        self.drum_kit_list = preset::list_drum_kits();
                    }
                    Err(e) => { self.drum_kit_status = format!("Error: {e}"); }
                }
            }
            ui.separator();
            let mut load_kit: Option<std::path::PathBuf> = None;
            egui::ComboBox::from_id_salt("drum_kit_load")
                .selected_text(if self.drum_kit_list.is_empty() { "No kits" } else { "Load..." })
                .width(140.0)
                .show_ui(ui, |ui| {
                    for (name, path) in &self.drum_kit_list {
                        if ui.selectable_label(false, name).clicked() {
                            load_kit = Some(path.clone());
                        }
                    }
                });
            if let Some(path) = load_kit {
                match preset::load_drum_kit(&path) {
                    Ok(kit) => {
                        self.drum_kit_name = kit.name.clone();
                        let mut patterns = [DrumPattern::default(); 8];
                        for i in 0..kit.patterns.len().min(8) {
                            patterns[i] = kit.patterns[i];
                        }
                        let mut params = [DrumSlotParams::default(); NUM_DRUM_SLOTS];
                        for i in 0..kit.params.len().min(NUM_DRUM_SLOTS) {
                            params[i] = kit.params[i];
                        }
                        self.drum_patterns = patterns;
                        self.drum_params = params;
                        self.drum_bpm = kit.bpm;
                        self.drum_swing = kit.swing;
                        self.drum_volume = kit.volume;
                        self.drum_kit_status = format!("Loaded: {}", kit.name);
                        let _ = self.ctrl_tx.push(ControlEvent::DrumLoadKit {
                            patterns: Box::new(patterns),
                            params: Box::new(params),
                            bpm: kit.bpm,
                            swing: kit.swing,
                            volume: kit.volume,
                        });
                    }
                    Err(e) => { self.drum_kit_status = format!("Error: {e}"); }
                }
            }
            if !self.drum_kit_status.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new(&self.drum_kit_status).small().weak());
            }
        });

        // Import MIDI drums
        ui.horizontal(|ui| {
            ui.label("MIDI:");
            ui.add(egui::TextEdit::singleline(&mut self.drum_midi_import_path)
                .desired_width(240.0).hint_text("path to .mid file"));
            if ui.button("Import").clicked() && !self.drum_midi_import_path.trim().is_empty() {
                let path = std::path::PathBuf::from(self.drum_midi_import_path.trim());
                match preset::import_midi_drums(&path) {
                    Ok((patterns_vec, bpm)) => {
                        let mut patterns = [DrumPattern::default(); 8];
                        for i in 0..patterns_vec.len().min(8) {
                            patterns[i] = patterns_vec[i];
                        }
                        self.drum_patterns = patterns;
                        self.drum_bpm = bpm;
                        self.drum_kit_status = format!(
                            "Imported {} bar(s) @ {:.0} BPM from {}",
                            patterns_vec.len().min(8), bpm,
                            path.file_name().unwrap_or_default().to_string_lossy()
                        );
                        let _ = self.ctrl_tx.push(ControlEvent::DrumLoadKit {
                            patterns: Box::new(patterns),
                            params: Box::new(self.drum_params),
                            bpm,
                            swing: self.drum_swing,
                            volume: self.drum_volume,
                        });
                    }
                    Err(e) => { self.drum_kit_status = format!("Import error: {e}"); }
                }
            }
        });

        ui.add_space(4.0);

        // Step grid
        let cell_w = 28.0;
        let cell_h = 22.0;
        let label_width = 80.0;
        let pattern_length = self.drum_patterns[pat_idx].length as usize;

        // Colors per instrument type
        let colors: [egui::Color32; NUM_DRUM_SLOTS] = [
            egui::Color32::from_rgb(220, 60, 60),   // Kick
            egui::Color32::from_rgb(180, 140, 80),   // Side Stick
            egui::Color32::from_rgb(220, 140, 40),   // Snare
            egui::Color32::from_rgb(200, 100, 200),  // Clap
            egui::Color32::from_rgb(220, 160, 40),   // E-Snare
            egui::Color32::from_rgb(120, 200, 80),   // Lo Floor Tom
            egui::Color32::from_rgb(60, 180, 220),   // Closed HH
            egui::Color32::from_rgb(100, 180, 60),   // Hi Floor Tom
            egui::Color32::from_rgb(80, 160, 200),   // Pedal HH
            egui::Color32::from_rgb(80, 160, 60),    // Low Tom
            egui::Color32::from_rgb(60, 200, 240),   // Open HH
            egui::Color32::from_rgb(60, 140, 60),    // Lo-Mid Tom
            egui::Color32::from_rgb(60, 120, 60),    // Hi-Mid Tom
            egui::Color32::from_rgb(200, 200, 60),   // Crash
            egui::Color32::from_rgb(60, 100, 60),    // High Tom
            egui::Color32::from_rgb(180, 180, 60),   // Ride
        ];

        for slot in 0..NUM_DRUM_SLOTS {
            ui.horizontal(|ui| {
                // Instrument label
                let name = DRUM_NAMES[slot];
                ui.allocate_ui(egui::vec2(label_width, cell_h), |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(name).small());
                    });
                });

                // Step buttons
                let spacing = ui.spacing().item_spacing;
                ui.spacing_mut().item_spacing = egui::vec2(1.0, 1.0);

                for step in 0..pattern_length {
                    let vel = self.drum_patterns[pat_idx].steps[slot][step].velocity;
                    let is_current = self.drum_playing && step == current_step as usize;

                    // click_and_drag: click to place/remove, drag vertically to adjust velocity
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(cell_w, cell_h),
                        egui::Sense::click_and_drag(),
                    );

                    // Beat grouping: highlight beat 1 of each group of 4
                    let bg = if step % 4 == 0 {
                        egui::Color32::from_gray(50)
                    } else if step % 2 == 0 {
                        egui::Color32::from_gray(40)
                    } else {
                        egui::Color32::from_gray(32)
                    };

                    let painter = ui.painter();
                    painter.rect_filled(rect, 2.0, bg);

                    if vel > 0 {
                        let fill_frac = vel as f32 / 127.0;
                        let fill_height = fill_frac * rect.height();
                        let fill_rect = egui::Rect::from_min_max(
                            egui::pos2(rect.min.x, rect.max.y - fill_height),
                            rect.max,
                        );
                        // Brighter color for higher velocity
                        let base = colors[slot];
                        let r = (base.r() as f32 * (0.4 + 0.6 * fill_frac)) as u8;
                        let g = (base.g() as f32 * (0.4 + 0.6 * fill_frac)) as u8;
                        let b = (base.b() as f32 * (0.4 + 0.6 * fill_frac)) as u8;
                        painter.rect_filled(fill_rect, 2.0, egui::Color32::from_rgb(r, g, b));
                    }

                    // Current step playhead
                    if is_current {
                        painter.rect_stroke(
                            rect.shrink(0.5),
                            2.0,
                            egui::Stroke::new(2.0, egui::Color32::WHITE),
                            egui::StrokeKind::Inside,
                        );
                    }

                    // Border (subtle)
                    if !is_current {
                        painter.rect_stroke(rect, 2.0, egui::Stroke::new(0.5, egui::Color32::from_gray(55)), egui::StrokeKind::Outside);
                    }

                    // Click: toggle step on/off. Place with default vel 100.
                    if response.clicked() {
                        let new_vel = if vel > 0 { 0 } else { 100u8 };
                        self.drum_patterns[pat_idx].steps[slot][step].velocity = new_vel;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetStep {
                            slot: slot as u8, step: step as u8, velocity: new_vel,
                        });
                    }

                    // Drag vertically: adjust velocity (up = louder)
                    if response.dragged() && vel > 0 {
                        let dy = response.drag_delta().y;
                        let current = vel as f32;
                        // -dy because up = increase
                        let new_val = (current - dy * 2.0).clamp(1.0, 127.0) as u8;
                        if new_val != vel {
                            self.drum_patterns[pat_idx].steps[slot][step].velocity = new_val;
                            let _ = self.ctrl_tx.push(ControlEvent::DrumSetStep {
                                slot: slot as u8, step: step as u8, velocity: new_val,
                            });
                        }
                    }

                    // Right-click: delete
                    if response.secondary_clicked() {
                        self.drum_patterns[pat_idx].steps[slot][step].velocity = 0;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetStep {
                            slot: slot as u8, step: step as u8, velocity: 0,
                        });
                    }

                    // Tooltip with velocity
                    if vel > 0 && response.hovered() {
                        response.on_hover_text(format!("vel: {vel}"));
                    }
                }

                ui.spacing_mut().item_spacing = spacing;
                ui.add_space(8.0);

                // Per-instrument controls
                ui.horizontal(|ui| {
                    let mut level = self.drum_params[slot].level;
                    let mut tune = self.drum_params[slot].tune;
                    let mut decay = self.drum_params[slot].decay;

                    ui.style_mut().spacing.slider_width = 40.0;
                    if ui.add(egui::Slider::new(&mut level, 0.0..=1.0).show_value(false).text("L")).changed() {
                        self.drum_params[slot].level = level;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Level(level) });
                    }
                    if ui.add(egui::Slider::new(&mut tune, -24.0..=24.0).show_value(false).text("T")).changed() {
                        self.drum_params[slot].tune = tune;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Tune(tune) });
                    }
                    if ui.add(egui::Slider::new(&mut decay, 0.1..=4.0).show_value(false).text("D")).changed() {
                        self.drum_params[slot].decay = decay;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Decay(decay) });
                    }
                });
            });
        }
    }

    fn draw_pitch_sequencer(&mut self, ui: &mut egui::Ui) {
        use crate::synth::step_seq::{StepRate, ScaleType};
        let layer = self.active_layer;
        if layer >= 2 { return; }

        let enabled = self.pitch_seq_enabled[layer];

        egui::CollapsingHeader::new("Step Sequencer")
            .default_open(enabled)
            .show(ui, |ui| {
                // Controls row
                ui.horizontal(|ui| {
                    let mut en = enabled;
                    if ui.checkbox(&mut en, "Enable").changed() {
                        self.pitch_seq_enabled[layer] = en;
                        let _ = self.ctrl_tx.push(ControlEvent::SeqSetEnabled { layer, enabled: en });
                    }

                    ui.separator();

                    // Rate
                    let rate = StepRate::from_index(self.pitch_seq_rate[layer]);
                    let rate_name = rate.name();
                    egui::ComboBox::from_id_salt(format!("seq_rate_{layer}"))
                        .selected_text(rate_name)
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            for i in 0..6u8 {
                                let r = StepRate::from_index(i);
                                if ui.selectable_label(i == self.pitch_seq_rate[layer], r.name()).clicked() {
                                    self.pitch_seq_rate[layer] = i;
                                    let _ = self.ctrl_tx.push(ControlEvent::SeqSetRate { layer, rate: i });
                                }
                            }
                        });

                    // Scale
                    let scale = ScaleType::from_index(self.pitch_seq_scale[layer]);
                    egui::ComboBox::from_id_salt(format!("seq_scale_{layer}"))
                        .selected_text(scale.name())
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            for i in 0..7u8 {
                                let s = ScaleType::from_index(i);
                                if ui.selectable_label(i == self.pitch_seq_scale[layer], s.name()).clicked() {
                                    self.pitch_seq_scale[layer] = i;
                                    let _ = self.ctrl_tx.push(ControlEvent::SeqSetScale { layer, scale: i });
                                }
                            }
                        });

                    // Length
                    ui.label("Len:");
                    let mut len = self.pitch_seq_length[layer] as i32;
                    let len_resp = ui.add(egui::DragValue::new(&mut len).range(1..=16).speed(0.1));
                    if len_resp.changed() {
                        self.pitch_seq_length[layer] = len as u8;
                        let _ = self.ctrl_tx.push(ControlEvent::SeqSetLength { layer, length: len as u8 });
                    }

                    // Swing
                    ui.label("Swing:");
                    let mut sw = self.pitch_seq_swing[layer];
                    if ui.add(egui::DragValue::new(&mut sw).range(0.0..=0.66).speed(0.005).fixed_decimals(2)).changed() {
                        self.pitch_seq_swing[layer] = sw;
                        let _ = self.ctrl_tx.push(ControlEvent::SeqSetSwing { layer, swing: sw });
                    }
                });

                if !enabled { return; }

                // Current step from audio thread
                let current_step = if layer < self.pitch_seq_step_atoms.len() {
                    self.pitch_seq_step_atoms[layer].load(std::sync::atomic::Ordering::Relaxed)
                } else { 0 };

                let length = self.pitch_seq_length[layer] as usize;

                // Step grid: pitch bars + gate toggles
                let avail_w = ui.available_width();
                let step_w = (avail_w / length as f32).min(40.0).max(20.0);
                let bar_h = 80.0;

                // Pitch bars
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(step_w * length as f32, bar_h + 20.0),
                    egui::Sense::click_and_drag(),
                );

                let painter = ui.painter_at(rect);

                // Background
                painter.rect_filled(rect, 2.0, egui::Color32::from_gray(30));

                // Center line (pitch = 0)
                let center_y = rect.top() + bar_h * 0.5;
                painter.line_segment(
                    [egui::pos2(rect.left(), center_y), egui::pos2(rect.left() + step_w * length as f32, center_y)],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
                );

                for i in 0..length {
                    let x = rect.left() + i as f32 * step_w;
                    let step = &self.pitch_seq_steps[layer][i];
                    let is_current = i == current_step as usize && enabled;

                    // Step separator
                    if i > 0 {
                        painter.line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.top() + bar_h)],
                            egui::Stroke::new(0.5, egui::Color32::from_gray(50)),
                        );
                    }

                    // Pitch bar
                    let pitch = step.pitch as f32;
                    let max_pitch = 24.0;
                    let bar_frac = (pitch / max_pitch).clamp(-1.0, 1.0);
                    let bar_top;
                    let bar_bottom;
                    if pitch >= 0.0 {
                        bar_bottom = center_y;
                        bar_top = center_y - bar_frac * bar_h * 0.5;
                    } else {
                        bar_top = center_y;
                        bar_bottom = center_y - bar_frac * bar_h * 0.5;
                    }

                    let bar_color = if !step.gate {
                        egui::Color32::from_gray(60)
                    } else if is_current {
                        egui::Color32::from_rgb(0, 220, 180)
                    } else {
                        egui::Color32::from_rgb(0, 160, 220)
                    };

                    let bar_rect = egui::Rect::from_min_max(
                        egui::pos2(x + 2.0, bar_top),
                        egui::pos2(x + step_w - 2.0, bar_bottom),
                    );
                    painter.rect_filled(bar_rect, 1.0, bar_color);

                    // Pitch label
                    if pitch != 0.0 {
                        let label = format!("{:+}", step.pitch);
                        painter.text(
                            egui::pos2(x + step_w * 0.5, if pitch > 0.0 { bar_top - 8.0 } else { bar_bottom + 8.0 }),
                            egui::Align2::CENTER_CENTER,
                            &label,
                            egui::FontId::proportional(9.0),
                            egui::Color32::from_gray(180),
                        );
                    }

                    // Current step highlight
                    if is_current {
                        painter.rect_stroke(
                            egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(step_w, bar_h)),
                            0.0,
                            egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 255, 200)),
                            egui::StrokeKind::Outside,
                        );
                    }

                    // Gate toggle (below bars)
                    let gate_y = rect.top() + bar_h + 4.0;
                    let gate_rect = egui::Rect::from_min_size(
                        egui::pos2(x + 4.0, gate_y),
                        egui::vec2(step_w - 8.0, 12.0),
                    );
                    let gate_color = if step.gate {
                        egui::Color32::from_rgb(0, 200, 120)
                    } else {
                        egui::Color32::from_gray(50)
                    };
                    painter.rect_filled(gate_rect, 2.0, gate_color);
                }

                // Handle mouse interaction
                if response.dragged() || response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let rel_x = pos.x - rect.left();
                        let step_idx = (rel_x / step_w) as usize;
                        if step_idx < length {
                            let rel_y = pos.y - rect.top();
                            if rel_y < bar_h {
                                // Dragging pitch bar
                                let normalized = 1.0 - (rel_y / bar_h); // 0 = bottom, 1 = top
                                let pitch = ((normalized - 0.5) * 2.0 * 24.0).round() as i8;
                                let pitch = pitch.clamp(-24, 24);
                                self.pitch_seq_steps[layer][step_idx].pitch = pitch;
                                let step = &self.pitch_seq_steps[layer][step_idx];
                                let _ = self.ctrl_tx.push(ControlEvent::SeqSetStep {
                                    layer, step: step_idx as u8,
                                    pitch, gate: step.gate, velocity: step.velocity,
                                });
                            } else if response.clicked() {
                                // Clicking gate area
                                let gate = !self.pitch_seq_steps[layer][step_idx].gate;
                                self.pitch_seq_steps[layer][step_idx].gate = gate;
                                let step = &self.pitch_seq_steps[layer][step_idx];
                                let _ = self.ctrl_tx.push(ControlEvent::SeqSetStep {
                                    layer, step: step_idx as u8,
                                    pitch: step.pitch, gate, velocity: step.velocity,
                                });
                            }
                        }
                    }
                }

                // Request repaint while playing for playhead animation
                if enabled {
                    ui.ctx().request_repaint();
                }
            });
    }

    fn draw_looper(&mut self, ui: &mut egui::Ui) {
        use crate::synth::looper::LooperState;

        let state_u8 = self.looper_atoms.state.load(Ordering::Relaxed);
        let position = self.looper_atoms.position.load(Ordering::Relaxed);
        let event_count = self.looper_atoms.event_count.load(Ordering::Relaxed);
        let layer_count = self.looper_atoms.layer_count.load(Ordering::Relaxed);
        let state = LooperState::from_u8(state_u8);

        ui.horizontal(|ui| {
            // Play/Stop — same style as drum sequencer
            let play_label = match state {
                LooperState::Playing | LooperState::Overdubbing => "\u{23F9} Stop",
                _ => "\u{25B6} Play",
            };
            if ui.button(play_label).clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::LooperTogglePlay);
            }

            // Record button — same style as drum sequencer
            let is_rec = matches!(state, LooperState::Recording | LooperState::Overdubbing);
            let rec_label = if is_rec { "\u{23FA} Rec" } else { "\u{26AB} Rec" };
            let rec_color = if is_rec { egui::Color32::RED } else { egui::Color32::GRAY };
            if ui.button(egui::RichText::new(rec_label).color(rec_color)).clicked() {
                match state {
                    LooperState::Idle | LooperState::Playing => {
                        let _ = self.ctrl_tx.push(ControlEvent::LooperRecord);
                    }
                    LooperState::Recording | LooperState::Overdubbing => {
                        let _ = self.ctrl_tx.push(ControlEvent::LooperStopRecord);
                    }
                }
            }

            if ui.button("Undo").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::LooperUndo);
            }
            if ui.button("Clear").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::LooperClear);
            }

            ui.separator();

            // Bars
            for &b in &[1u8, 2, 4, 8] {
                if ui.selectable_label(self.looper_bars == b, format!("{b}")).clicked() {
                    self.looper_bars = b;
                    let _ = self.ctrl_tx.push(ControlEvent::LooperSetBars { bars: b });
                }
            }

            ui.separator();

            // Status
            let status_color = match state {
                LooperState::Recording => egui::Color32::RED,
                LooperState::Overdubbing => egui::Color32::from_rgb(255, 140, 0),
                LooperState::Playing => egui::Color32::GREEN,
                LooperState::Idle => egui::Color32::GRAY,
            };
            let state_label = match state {
                LooperState::Idle => "Idle",
                LooperState::Recording => "REC",
                LooperState::Playing => "Play",
                LooperState::Overdubbing => "OVR",
            };
            ui.colored_label(status_color, state_label);

            if event_count > 0 {
                ui.label(egui::RichText::new(format!("{event_count}ev L{layer_count}")).small().weak());
            }

            // Progress bar
            if state != LooperState::Idle {
                let frac = position as f32 / 255.0;
                ui.add(egui::ProgressBar::new(frac).desired_width(80.0));
            }
        });
    }

    fn draw_pad_perf_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_pad_perf;
        // Same layout as keyboard pads: 2 rows x 8 columns
        // Top (cyan):  E1(40) F1(41) F#1(42) G1(43)  C2(48) C#2(49) D2(50) D#2(51)
        // Bot (pink):  C1(36) C#1(37) D1(38) D#1(39) G#1(44) A1(45) A#1(46) B1(47)
        const PAD_TOP: [u8; 8] = [40, 41, 42, 43, 48, 49, 50, 51];
        const PAD_BOT: [u8; 8] = [36, 37, 38, 39, 44, 45, 46, 47];
        const TOP_COLOR: egui::Color32 = egui::Color32::from_rgb(0, 160, 180);
        const BOT_COLOR: egui::Color32 = egui::Color32::from_rgb(180, 50, 120);
        const EMPTY_COLOR: egui::Color32 = egui::Color32::from_rgb(50, 50, 58);

        egui::Window::new("Pad Performances")
            .open(&mut open)
            .resizable(false)
            .default_width(700.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Tap pad to switch perf. Click cell to assign. Right-click to clear.")
                    .small().weak());
                ui.add_space(4.0);

                let pad_w = 80.0_f32;
                let pad_h = 52.0_f32;
                let gap = 3.0_f32;

                for (row_notes, row_color) in [(&PAD_TOP[..], TOP_COLOR), (&PAD_BOT[..], BOT_COLOR)] {
                    ui.horizontal(|ui| {
                        for &note in row_notes {
                            let idx = (note - 36) as usize;
                            let vel = self.pad_state[note as usize].load(std::sync::atomic::Ordering::Relaxed);
                            let pressed = vel > 0;
                            let has_perf = self.pad_perf_map[idx].is_some();

                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(pad_w, pad_h), egui::Sense::click(),
                            );
                            let painter = ui.painter_at(rect);

                            // Background
                            let bg = if has_perf {
                                if pressed {
                                    egui::Color32::from_rgb(
                                        (row_color.r() as u16 + 70).min(255) as u8,
                                        (row_color.g() as u16 + 70).min(255) as u8,
                                        (row_color.b() as u16 + 70).min(255) as u8,
                                    )
                                } else {
                                    row_color
                                }
                            } else if pressed {
                                egui::Color32::from_rgb(90, 90, 100)
                            } else {
                                EMPTY_COLOR
                            };

                            painter.rect_filled(rect, 4.0, bg);
                            if pressed {
                                painter.rect_stroke(rect, 4.0, egui::Stroke::new(2.0, egui::Color32::WHITE), egui::StrokeKind::Outside);
                            }

                            // Performance name inside
                            if let Some(ref name) = self.pad_perf_map[idx] {
                                let display = if name.len() > 10 { &name[..10] } else { name };
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    display,
                                    egui::FontId::proportional(11.0),
                                    egui::Color32::WHITE,
                                );
                            }

                            // Left click: combo popup to pick perf
                            let popup_id = ui.id().with(format!("pad_popup_{note}"));
                            if response.clicked() {
                                ui.memory_mut(|m| m.toggle_popup(popup_id));
                            }
                            // Right click: clear
                            if response.secondary_clicked() {
                                self.pad_perf_map[idx] = None;
                            }

                            egui::popup_below_widget(ui, popup_id, &response, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                                ui.set_min_width(140.0);
                                if ui.selectable_label(self.pad_perf_map[idx].is_none(), "--- (clear)").clicked() {
                                    self.pad_perf_map[idx] = None;
                                }
                                for (name, _) in &self.perf_list {
                                    let active = self.pad_perf_map[idx].as_deref() == Some(name.as_str());
                                    if ui.selectable_label(active, name).clicked() {
                                        self.pad_perf_map[idx] = Some(name.clone());
                                    }
                                }
                            });
                        }
                    });
                    ui.add_space(gap);
                }

                ui.add_space(2.0);
                if !self.pad_perf_status.is_empty() {
                    ui.label(egui::RichText::new(&self.pad_perf_status).small().weak());
                }
            });
        if self.show_pad_perf && !open {
            // Window closing — save pad map
            self.save_global_to_config();
        }
        self.show_pad_perf = open;
    }

    fn draw_help(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new("MIDI CC Mapping")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.heading("SMK-37 Pro Controller Map");
                ui.add_space(8.0);

                // Preset-scoped bindings
                ui.strong("Preset (reset on preset change, pickup mode)");
                egui::Grid::new("cc_help_preset")
                    .num_columns(3)
                    .spacing([16.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Control"); ui.strong("CC"); ui.strong("Parameter"); ui.end_row();
                        for (cc, binding) in self.cc_map.bindings.iter().enumerate().filter_map(|(i, b)| b.map(|b| (i as u8, b))) {
                            if binding.scope == crate::cc_map::ParamScope::Preset {
                                let ctrl = crate::cc_map::cc_to_control_name(cc);
                                let label = crate::cc_map::find_param_meta(binding.param_key)
                                    .map(|m| m.label).unwrap_or("?");
                                ui.label(format!("{ctrl} (CC{cc})")); ui.label(""); ui.label(label); ui.end_row();
                            }
                        }
                    });

                ui.add_space(8.0);

                // Global bindings
                ui.strong("Global (persist across preset changes)");
                egui::Grid::new("cc_help_global")
                    .num_columns(3)
                    .spacing([16.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Control"); ui.strong("CC"); ui.strong("Parameter"); ui.end_row();
                        for (cc, binding) in self.cc_map.bindings.iter().enumerate().filter_map(|(i, b)| b.map(|b| (i as u8, b))) {
                            if binding.scope == crate::cc_map::ParamScope::Global {
                                let ctrl = crate::cc_map::cc_to_control_name(cc);
                                let label = crate::cc_map::find_param_meta(binding.param_key)
                                    .map(|m| m.label).unwrap_or("?");
                                ui.label(format!("{ctrl} (CC{cc})")); ui.label(""); ui.label(label); ui.end_row();
                            }
                        }
                    });

                ui.add_space(8.0);
                ui.separator();
                ui.label("Preset knobs use pickup mode: after switching presets,");
                ui.label("move the knob past the current value to start controlling.");
            });
        self.show_help = open;
    }

    fn drain_feedback(&mut self) {
        let mut param_changes: Vec<(&str, f32)> = Vec::new();
        let mut learned_cc: Option<(u8, String)> = None;
        let mut program: Option<u8> = None;
        let mut nav_delta: Option<i32> = None;

        if let Some(rx) = &mut self.feedback_rx {
            while let Ok(fb) = rx.pop() {
                match fb {
                    ParamFeedback::ParamChanged { key, value } => {
                        param_changes.push((key, value));
                    }
                    ParamFeedback::CcReceived { cc } => {
                        if let Some(target) = self.midi_learn_target.take() {
                            learned_cc = Some((cc, target));
                        }
                    }
                    ParamFeedback::ProgramChanged { program: prog } => {
                        program = Some(prog);
                    }
                    ParamFeedback::DrumStepRecorded { pattern, slot, step, velocity } => {
                        if (pattern as usize) < self.drum_patterns.len() {
                            self.drum_patterns[pattern as usize].steps[slot as usize][step as usize].velocity = velocity;
                        }
                    }
                    ParamFeedback::PickupPending { key, cc_position } => {
                        self.pickup_indicators.insert(key.to_string(), cc_position);
                    }
                    ParamFeedback::PickupDone { key } => {
                        self.pickup_indicators.remove(key);
                    }
                    ParamFeedback::NavigatePress => {
                        self.nav_press_time = Some(std::time::Instant::now());
                    }
                    ParamFeedback::NavigateRelease => {
                        if let Some(press_time) = self.nav_press_time.take() {
                            let held = press_time.elapsed();
                            if held < std::time::Duration::from_millis(400) {
                                // Short press: toggle keys ↔ drums
                                self.show_drums = !self.show_drums;
                                self.show_looper = false;
                                if !self.show_drums {
                                    self.seq_target_atom.store(1, std::sync::atomic::Ordering::Relaxed);
                                } else {
                                    self.seq_target_atom.store(0, std::sync::atomic::Ordering::Relaxed);
                                }
                            } else {
                                // Long press: toggle pad performance panel
                                self.show_pad_perf = !self.show_pad_perf;
                                if !self.show_pad_perf {
                                    self.pad_perf_status.clear();
                                }
                            }
                        }
                    }
                }
            }
        }

        for (key, value) in param_changes {
            if crate::cc_map::is_global_param(key) {
                // Global params (volume, tone) — don't put in preset
                self.global_params.insert(key.to_string(), value);
                self.global_dirty = true;
            } else {
                // Preset params — update all layers
                for layer_state in &mut self.layers {
                    layer_state.edited_params.insert(key.to_string(), value);
                    layer_state.params_dirty = true;
                }
            }
        }

        if let Some((cc, target)) = learned_cc {
            if let Some(meta) = crate::cc_map::find_param_meta(&target) {
                self.cc_map.bindings[cc as usize] = Some(crate::cc_map::CcBinding {
                    param_key: meta.key,
                    min_val: meta.min,
                    max_val: meta.max,
                    logarithmic: meta.logarithmic,
                    scope: meta.scope,
                });
                self.save_cc_map();
            }
        }

        if let Some(prog) = program {
            if self.presets.get(prog as usize).is_some() {
                self.layers[0].preset_idx = prog as usize;
                self.load_edited_params(0);
            }
        }

        let _ = nav_delta; // no longer used (setlist removed)
    }

    fn save_cc_map(&mut self) {
        self.config.cc_map = Some(crate::cc_map::CcMapRaw::from_cc_map(&self.cc_map));
        let _ = self.config.save();
        let _ = self.ctrl_tx.push(ControlEvent::SetCcMap { map: self.cc_map });
    }

    fn param_slider(&mut self, ui: &mut egui::Ui, key: &str, label: &str, min: f32, max: f32, logarithmic: bool) -> bool {
        let layer = self.active_layer;
        let mut val = self.layers[layer].edited_params.get(key).copied().unwrap_or(min);

        // Build label with CC number and pickup indicator
        let cc_info = self.cc_map.bindings.iter().enumerate()
            .find_map(|(cc, b)| b.filter(|b| b.param_key == key).map(|_| cc));
        let pickup = self.pickup_indicators.get(key).copied();
        let display_label = match (cc_info, pickup) {
            (Some(cc), Some(knob_pos)) => {
                let arrow = if knob_pos < val { "\u{2193}" } else { "\u{2191}" }; // ↓ or ↑
                format!("{label} [CC{cc} {arrow}]")
            }
            (Some(cc), None) => format!("{label} [CC{cc}]"),
            _ => label.to_string(),
        };

        let slider = egui::Slider::new(&mut val, min..=max)
            .text(&display_label)
            .logarithmic(logarithmic);
        let resp = ui.add(slider);
        if resp.secondary_clicked() {
            self.midi_learn_target = Some(key.to_string());
        }
        if resp.changed() {
            self.layers[layer].edited_params.insert(key.into(), val);
            return true;
        }
        false
    }

    fn save_as_user_preset(&mut self) {
        let layer = self.active_layer;
        let base_name = self.presets.get(self.layers[layer].preset_idx)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Custom".to_string());

        let new_name = format!("{base_name} (user)");
        let new_preset = Preset {
            name: new_name.clone(),
            category: "User".to_string(),
            params: self.layers[layer].edited_params.clone(),
        };

        if let Ok(path) = preset::save_preset(&new_preset) {
            self.settings_status = format!("Saved: {}", path.display());
            self.presets.push(new_preset);
            self.layers[layer].preset_idx = self.presets.len() - 1;
            self.layers[layer].params_dirty = false;
            self.save_config();
        }
    }

    fn draw_settings(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        egui::Window::new("Settings")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.heading("Audio");

                if self.is_jack {
                    ui.colored_label(
                        egui::Color32::from_rgb(180, 180, 180),
                        format!("JACK server: {}Hz", self.sample_rate),
                    );
                    ui.add_space(4.0);
                }

                egui::Grid::new("audio_settings")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Backend:");
                        let host_name = self.available_hosts.get(self.selected_host_idx)
                            .map(|(_, n)| *n).unwrap_or("?");
                        let old_idx = self.selected_host_idx;
                        egui::ComboBox::from_id_salt("host")
                            .selected_text(host_name)
                            .show_ui(ui, |ui| {
                                for (i, (_, name)) in self.available_hosts.iter().enumerate() {
                                    ui.selectable_value(&mut self.selected_host_idx, i, *name);
                                }
                            });
                        if self.selected_host_idx != old_idx {
                            self.refresh_sample_rates();
                        }
                        ui.end_row();

                        ui.label("Sample Rate:");
                        egui::ComboBox::from_id_salt("samplerate")
                            .selected_text(format!("{}", self.selected_sample_rate))
                            .show_ui(ui, |ui| {
                                for &rate in &self.supported_sample_rates {
                                    ui.selectable_value(&mut self.selected_sample_rate, rate, format!("{rate} Hz"));
                                }
                            });
                        ui.end_row();

                        ui.label("Buffer Size:");
                        egui::ComboBox::from_id_salt("buffer")
                            .selected_text(buffer_label(self.selected_buffer_size))
                            .show_ui(ui, |ui| {
                                for &size in BUFFER_SIZES {
                                    let label = if size == 0 {
                                        "Default".to_string()
                                    } else {
                                        let ms = size as f32 / self.selected_sample_rate as f32 * 1000.0;
                                        format!("{size} ({ms:.1}ms)")
                                    };
                                    ui.selectable_value(&mut self.selected_buffer_size, size, label);
                                }
                            });
                        ui.end_row();
                    });

                ui.add_space(4.0);
                ui.colored_label(egui::Color32::from_rgb(140, 140, 140), "Restart to apply audio changes.");
                if ui.button("Save Audio Settings").clicked() {
                    if let Some((_, host_name)) = self.available_hosts.get(self.selected_host_idx) {
                        self.config.audio.backend = host_name.to_string();
                        self.config.audio.sample_rate = self.selected_sample_rate;
                        self.config.audio.buffer_size = self.selected_buffer_size;
                        self.save_config();
                        self.settings_status = "Audio saved. Restart to apply.".to_string();
                    }
                }

                ui.add_space(12.0);
                ui.heading("MIDI");

                egui::Grid::new("midi_settings")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Port:");
                        let current_label = self.selected_midi_port.clone()
                            .unwrap_or_else(|| "(none)".to_string());
                        egui::ComboBox::from_id_salt("midi_port")
                            .selected_text(&current_label)
                            .width(280.0)
                            .show_ui(ui, |ui| {
                                let none_val: Option<String> = None;
                                ui.selectable_value(&mut self.selected_midi_port, none_val, "(none)");
                                for name in &self.midi_port_names {
                                    ui.selectable_value(&mut self.selected_midi_port, Some(name.clone()), name.as_str());
                                }
                            });
                        ui.end_row();
                    });

                ui.horizontal(|ui| {
                    if ui.button("\u{21BB} Refresh").clicked() {
                        self.refresh_midi_ports();
                    }
                    if ui.button("Connect").clicked() {
                        self.apply_midi();
                    }
                });

                // SF2 SoundFont section
                ui.add_space(12.0);
                ui.heading("SoundFont (SF2)");

                // Keys SF2
                ui.horizontal(|ui| {
                    let current = if self.sf2_keys_loaded_name.is_empty() {
                        "(none)".to_string()
                    } else {
                        self.sf2_keys_loaded_name.clone()
                    };
                    ui.label("Keys:");
                    egui::ComboBox::from_id_salt("sf2_keys_file")
                        .selected_text(&current)
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(self.sf2_keys_selected.is_none(), "(none)").clicked() {
                                self.sf2_keys_selected = None;
                            }
                            for (i, (name, _)) in self.sf2_file_list.iter().enumerate() {
                                if ui.selectable_label(self.sf2_keys_selected == Some(i), name).clicked() {
                                    self.sf2_keys_selected = Some(i);
                                }
                            }
                        });
                    if ui.button("Load").clicked() {
                        if let Some(idx) = self.sf2_keys_selected {
                            self.load_sf2_keys(idx);
                        } else {
                            self.unload_sf2_keys();
                        }
                    }
                });

                // Drums SF2
                ui.horizontal(|ui| {
                    let current = if self.sf2_drums_loaded_name.is_empty() {
                        "(none)".to_string()
                    } else {
                        self.sf2_drums_loaded_name.clone()
                    };
                    ui.label("Drums:");
                    egui::ComboBox::from_id_salt("sf2_drums_file")
                        .selected_text(&current)
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(self.sf2_drums_selected.is_none(), "(none)").clicked() {
                                self.sf2_drums_selected = None;
                            }
                            for (i, (name, _)) in self.sf2_file_list.iter().enumerate() {
                                if ui.selectable_label(self.sf2_drums_selected == Some(i), name).clicked() {
                                    self.sf2_drums_selected = Some(i);
                                }
                            }
                        });
                    if ui.button("Load").clicked() {
                        if let Some(idx) = self.sf2_drums_selected {
                            self.load_sf2_drums(idx);
                        } else {
                            self.unload_sf2_drums();
                        }
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("\u{21BB} Scan").clicked() {
                        self.sf2_file_list = scan_sf2_files();
                    }
                    if ui.button("Open folder").clicked() {
                        let dir = sf2_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                    }
                });

                if !self.sf2_drums_loaded_name.is_empty() {
                    ui.horizontal(|ui| {
                        let mut drums = self.sf2_drums_enabled;
                        if ui.checkbox(&mut drums, "SF2 Drums").changed() {
                            self.sf2_drums_enabled = drums;
                            let _ = self.ctrl_tx.push(ControlEvent::SetDrumsSf2Mode { enabled: drums });
                            self.config.sf2.drums_sf2 = drums;
                            let _ = self.config.save();
                        }
                    });
                }

                if self.sf2_keys_soundfont.is_some() || self.sf2_drums_soundfont.is_some() {
                    ui.horizontal(|ui| {
                        ui.label("Block size:");
                        let mut bs = self.sf2_block_size;
                        let old = bs;
                        egui::ComboBox::from_id_salt("sf2_block")
                            .selected_text(format!("{bs} samples"))
                            .show_ui(ui, |ui| {
                                for &s in &[8, 16, 32, 64] {
                                    let ms = s as f32 / self.sample_rate as f32 * 1000.0;
                                    ui.selectable_value(&mut bs, s, format!("{s} ({ms:.2}ms)"));
                                }
                            });
                        if bs != old {
                            self.sf2_block_size = bs;
                            let _ = self.ctrl_tx.push(ControlEvent::SetSf2BlockSize { size: bs });
                            self.config.sf2.block_size = bs;
                            let _ = self.config.save();
                        }
                    });
                }

                ui.colored_label(
                    egui::Color32::from_rgb(140, 140, 140),
                    format!("Place .sf2 files in: {}", sf2_dir().display()),
                );

                if !self.sf2_status.is_empty() {
                    ui.colored_label(egui::Color32::YELLOW, &self.sf2_status);
                }

                if !self.settings_status.is_empty() {
                    ui.add_space(6.0);
                    ui.colored_label(egui::Color32::YELLOW, &self.settings_status);
                }
            });
        self.show_settings = open;
    }

    fn apply_midi(&mut self) {
        if let Some(port_name) = &self.selected_midi_port {
            let port_idx = self.midi_port_names.iter().position(|n| n == port_name);
            if let Some(idx) = port_idx {
                if let Some(reconnect) = &mut self.on_midi_reconnect {
                    match reconnect(idx) {
                        Ok(()) => {
                            self.midi_connected_port = Some(port_name.clone());
                            self.config.midi.port_name = Some(port_name.clone());
                            self.settings_status = format!("MIDI: {port_name}");
                            self.save_config();
                        }
                        Err(e) => { self.settings_status = format!("MIDI error: {e}"); }
                    }
                }
            } else {
                self.settings_status = format!("Port not found: {port_name}");
            }
        } else {
            self.midi_connected_port = None;
            self.config.midi.port_name = None;
            self.settings_status = "MIDI disconnected".to_string();
            self.save_config();
        }
    }

    fn draw_keyboard(&self, ui: &mut egui::Ui) {
        // Active notes label above keyboard
        let active: Vec<String> = (0..128u8)
            .filter_map(|i| {
                let vel = self.note_state[i as usize].load(Ordering::Relaxed);
                if vel > 0 {
                    Some(note_name(i))
                } else {
                    None
                }
            })
            .collect();

        if active.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(100, 100, 100), " ");
        } else {
            ui.label(active.join("  "));
        }

        ui.horizontal(|ui| {
            // --- Piano keys ---
            let start_note: u8 = 36;
            let end_note: u8 = 96;

            let total_white = (start_note..end_note)
                .filter(|n| !is_black_key(*n))
                .count() as f32;

            // Reserve space for pads on the right: 8 pads * pad_size + gap
            let pad_size = 32.0_f32;
            let pad_gap = 2.0_f32;
            let pad_block_w = 8.0 * (pad_size + pad_gap) + 12.0; // 12px margin

            let available_w = ui.available_width() - pad_block_w;
            let key_w = (available_w / total_white).min(18.0).max(6.0);
            let key_h = (key_w * 3.5).min(70.0);
            let black_h = key_h * 0.62;

            let (response, painter) = ui.allocate_painter(
                egui::vec2(total_white * key_w, key_h),
                egui::Sense::hover(),
            );
            let rect = response.rect;

            // White keys
            let mut wx = 0.0_f32;
            for note in start_note..end_note {
                if is_black_key(note) { continue; }
                let vel = self.note_state[note as usize].load(Ordering::Relaxed);
                let key_rect = egui::Rect::from_min_size(
                    rect.min + egui::vec2(wx, 0.0),
                    egui::vec2(key_w - 1.0, key_h),
                );
                let color = if vel > 0 {
                    egui::Color32::from_rgb(80, 180, 255)
                } else {
                    egui::Color32::from_rgb(220, 220, 220)
                };
                painter.rect_filled(key_rect, 1.0, color);
                painter.rect_stroke(key_rect, 1.0, egui::Stroke::new(0.5, egui::Color32::from_rgb(120, 120, 120)), egui::StrokeKind::Outside);
                wx += key_w;
            }

            // Black keys
            wx = 0.0;
            for note in start_note..end_note {
                if is_black_key(note) {
                    let vel = self.note_state[note as usize].load(Ordering::Relaxed);
                    let bw = key_w * 0.65;
                    let key_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(wx - bw * 0.5, 0.0),
                        egui::vec2(bw, black_h),
                    );
                    let color = if vel > 0 {
                        egui::Color32::from_rgb(60, 140, 220)
                    } else {
                        egui::Color32::from_rgb(30, 30, 30)
                    };
                    painter.rect_filled(key_rect, 1.0, color);
                } else {
                    wx += key_w;
                }
            }

            ui.add_space(12.0);

            // --- Pads (C1–D#2 = MIDI 36–51, MPC layout as 2×8) ---
            // Top row: E1,F1,F#1,G1, C2,C#2,D2,D#2 — cyan
            // Bottom row: C1,C#1,D1,D#1, G#1,A1,A#1,B1 — pink
            const PAD_TOP: [u8; 8] = [40, 41, 42, 43, 48, 49, 50, 51];
            const PAD_BOT: [u8; 8] = [36, 37, 38, 39, 44, 45, 46, 47];

            let pad_h = (key_h - pad_gap) / 2.0;
            let pad_w = pad_size;
            let total_pad_w = 8.0 * (pad_w + pad_gap);
            let total_pad_h = key_h;

            let (pad_resp, pad_painter) = ui.allocate_painter(
                egui::vec2(total_pad_w, total_pad_h),
                egui::Sense::hover(),
            );
            let pad_origin = pad_resp.rect.min;

            // Top row (cyan)
            for col in 0..8u8 {
                let note = PAD_TOP[col as usize];
                let vel = self.pad_state[note as usize].load(Ordering::Relaxed);
                let pr = egui::Rect::from_min_size(
                    pad_origin + egui::vec2(col as f32 * (pad_w + pad_gap), 0.0),
                    egui::vec2(pad_w, pad_h),
                );
                let color = if vel > 0 {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(0, 200, 210)
                };
                pad_painter.rect_filled(pr, 3.0, color);
                pad_painter.rect_stroke(pr, 3.0, egui::Stroke::new(0.5, egui::Color32::from_rgb(60, 60, 60)), egui::StrokeKind::Outside);
            }

            // Bottom row (pink)
            for col in 0..8u8 {
                let note = PAD_BOT[col as usize];
                let vel = self.pad_state[note as usize].load(Ordering::Relaxed);
                let pr = egui::Rect::from_min_size(
                    pad_origin + egui::vec2(col as f32 * (pad_w + pad_gap), pad_h + pad_gap),
                    egui::vec2(pad_w, pad_h),
                );
                let color = if vel > 0 {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(220, 60, 150)
                };
                pad_painter.rect_filled(pr, 3.0, color);
                pad_painter.rect_stroke(pr, 3.0, egui::Stroke::new(0.5, egui::Color32::from_rgb(60, 60, 60)), egui::StrokeKind::Outside);
            }
        });
    }
}

fn is_black_key(note: u8) -> bool {
    matches!(note % 12, 1 | 3 | 6 | 8 | 10)
}
