/// GUI interface using egui/eframe.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cpal::HostId;
use eframe::egui;
use rtrb::Producer;

use crate::config::Config;
use crate::midi::{self, NoteState};
use crate::preset::{self, Preset};
use crate::synth::{ControlEvent, MAX_LAYERS};

const NOTE_NAMES: &[&str] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

pub const BUFFER_SIZES: &[u32] = &[0, 16, 32, 48, 64, 128, 256, 512, 1024, 2048];

const OSC_NAMES: &[&str] = &[
    "Sine", "Saw", "Square", "Triangle", "FM", "Noise",
    "Karplus-Strong", "Organ", "FM Piano", "Piano (Physical)", "Piano (Banded)",
    "Piano (Additive)",
];
const SIMPLE_OSC_NAMES: &[&str] = &["Sine", "Saw", "Square", "Triangle", "FM"];
const FILTER_NAMES: &[&str] = &["LowPass", "HighPass", "BandPass", "Formant"];
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
}

fn note_name(note: u8) -> String {
    let name = NOTE_NAMES[(note % 12) as usize];
    let oct = (note as i8 / 12) - 1;
    format!("{name}{oct}")
}

pub struct App {
    pub frame_count: u64,
    pub presets: Vec<Preset>,
    pub note_state: NoteState,
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
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.config.save();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let has_active_notes = (0..128u8)
            .any(|i| self.note_state[i as usize].load(Ordering::Relaxed) > 0);
        if has_active_notes {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
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
            ui.add_space(2.0);
        });

        // Bottom: keyboard
        egui::TopBottomPanel::bottom("keyboard_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            self.draw_keyboard(ui);
            ui.add_space(4.0);
        });

        // Right: preset list (for active layer)
        egui::SidePanel::right("preset_panel")
            .resizable(true)
            .default_width(160.0)
            .min_width(120.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                let layer_label = LAYER_NAMES.get(self.active_layer).unwrap_or(&"?");
                ui.strong(format!("Presets (Layer {layer_label})"));
                ui.add_space(4.0);
                ui.separator();

                let current_preset_idx = self.layers[self.active_layer].preset_idx;
                let mut new_idx: Option<usize> = None;
                let mut toggled_category: Option<String> = None;

                // Collect category info to avoid borrow conflicts
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
                    let layer = self.active_layer;
                    self.layers[layer].preset_idx = idx;
                    self.load_edited_params(layer);
                    self.send_edited_params(layer);
                    self.save_config();
                }
            });

        // Settings window
        if self.show_settings {
            self.draw_settings(ctx);
        }

        // Central: layer tabs + parameters
        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_layer_tabs(ui);
            ui.separator();
            ui.add_space(4.0);
            self.draw_params_editable(ui);
        });
    }
}

impl App {
    /// Send initial presets to all layers at startup
    pub fn send_initial_presets(&mut self) {
        for i in 0..self.layers.len() {
            self.send_edited_params(i);
        }
    }

    /// Load edited_params from the current preset for a given layer
    pub fn load_edited_params(&mut self, layer: usize) {
        let preset_idx = self.layers[layer].preset_idx;
        if let Some(p) = self.presets.get(preset_idx) {
            self.layers[layer].edited_params = p.params.clone();
        }
        self.layers[layer].params_dirty = false;
    }

    /// Send edited params to synth engine for a given layer
    fn send_edited_params(&mut self, layer: usize) {
        let preset_idx = self.layers[layer].preset_idx;
        let preset = Preset {
            name: self.presets.get(preset_idx)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Custom".to_string()),
            category: String::new(),
            params: self.layers[layer].edited_params.clone(),
        };
        let _ = self.ctrl_tx.push(ControlEvent::LoadPreset { layer, preset });
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

    fn save_config(&mut self) {
        self.config.ui.last_preset = self
            .presets
            .get(self.layers[0].preset_idx)
            .map(|p| p.name.clone());
        let _ = self.config.save();
    }

    fn save_collapsed_categories(&mut self) {
        self.config.ui.collapsed_categories = self.collapsed_categories.iter().cloned().collect();
        let _ = self.config.save();
    }

    fn save_window_size(&mut self, ctx: &egui::Context) {
        self.frame_count += 1;

        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));

        // Debug: log every ~5 seconds to file + stdout
        if self.frame_count % 50 == 1 {
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
                let is_active = i == self.active_layer;
                let is_enabled = self.layers[i].enabled;

                let text = if is_enabled {
                    let range = format!("{}-{}", note_name(self.layers[i].min_note), note_name(self.layers[i].max_note));
                    format!("Layer {label} [{range}]")
                } else {
                    format!("Layer {label} (off)")
                };

                if ui.selectable_label(is_active, &text).clicked() {
                    self.active_layer = i;
                }
            }

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
        ui.label("Oscillator 1");
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
            ui.label("Drawbars");
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
                ui.label("Oscillator 2");
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
                ui.label("Oscillator 3");
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
        ui.label("Filter 1");
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
                ui.label("Filter 2");
                ui.horizontal(|ui| {
                    let mut ft2 = self.layers[layer].edited_params.get("filter2_type").copied().unwrap_or(0.0) as usize;
                    ui.label("Type:");
                    // Filter 2 only supports LowPass/HighPass/BandPass (no Formant)
                    let f2_names = &FILTER_NAMES[..3];
                    egui::ComboBox::from_id_salt(format!("filter2_type_{layer}"))
                        .selected_text(*f2_names.get(ft2).unwrap_or(&"LowPass"))
                        .show_ui(ui, |ui| {
                            for (i, name) in f2_names.iter().enumerate() {
                                if ui.selectable_value(&mut ft2, i, *name).changed() {
                                    self.layers[layer].edited_params.insert("filter2_type".into(), ft2 as f32);
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
        ui.label("Amp Envelope");
        changed |= self.param_slider(ui, "amp_attack", "Attack", 0.001, 5.0, true);
        changed |= self.param_slider(ui, "amp_decay", "Decay", 0.0, 5.0, false);
        changed |= self.param_slider(ui, "amp_sustain", "Sustain", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "amp_release", "Release", 0.001, 5.0, true);

        ui.add_space(6.0);

        // Filter Envelope
        ui.label("Filter Envelope");
        changed |= self.param_slider(ui, "filter_attack", "Attack", 0.001, 5.0, true);
        changed |= self.param_slider(ui, "filter_decay", "Decay", 0.0, 5.0, false);
        changed |= self.param_slider(ui, "filter_sustain", "Sustain", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "filter_release", "Release", 0.001, 5.0, true);

        ui.add_space(6.0);

        // Volume (per-preset master_volume stored in params)
        changed |= self.param_slider(ui, "master_volume", "Volume", 0.0, 1.0, false);

        ui.add_space(6.0);

        // Effects
        ui.label("Effects");
        changed |= self.param_slider(ui, "chorus_mix", "Chorus", 0.0, 1.0, false);

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

    /// Draw a parameter slider, returns true if changed
    fn param_slider(&mut self, ui: &mut egui::Ui, key: &str, label: &str, min: f32, max: f32, logarithmic: bool) -> bool {
        let layer = self.active_layer;
        let mut val = self.layers[layer].edited_params.get(key).copied().unwrap_or(min);
        let slider = egui::Slider::new(&mut val, min..=max)
            .text(label)
            .logarithmic(logarithmic);
        let resp = ui.add(slider);
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
                    let name = NOTE_NAMES[(i % 12) as usize];
                    let oct = i / 12 - 1;
                    Some(format!("{name}{oct}"))
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

        let start_note: u8 = 36;
        let end_note: u8 = 96;

        let total_white = (start_note..end_note)
            .filter(|n| !is_black_key(*n))
            .count() as f32;

        let available_w = ui.available_width();
        let key_w = (available_w / total_white).min(18.0).max(6.0);
        let key_h = (key_w * 3.5).min(70.0);
        let black_h = key_h * 0.62;

        let (response, painter) = ui.allocate_painter(
            egui::vec2(total_white * key_w, key_h),
            egui::Sense::hover(),
        );
        let rect = response.rect;

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
    }
}

fn is_black_key(note: u8) -> bool {
    matches!(note % 12, 1 | 3 | 6 | 8 | 10)
}
