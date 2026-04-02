/// GUI interface using egui/eframe.

use std::sync::atomic::Ordering;
use std::time::Duration;

use cpal::HostId;
use eframe::egui;
use rtrb::Producer;

use crate::config::Config;
use crate::midi::{self, NoteState};
use crate::preset::{self, Preset};
use crate::synth::ControlEvent;

const NOTE_NAMES: &[&str] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

pub const BUFFER_SIZES: &[u32] = &[0, 16, 32, 48, 64, 128, 256, 512, 1024, 2048];

const OSC_NAMES: &[&str] = &["Sine", "Saw", "Square", "Triangle", "FM"];
const FILTER_NAMES: &[&str] = &["LowPass", "HighPass", "BandPass"];

fn buffer_label(size: u32) -> String {
    if size == 0 { "Default".to_string() } else { format!("{size}") }
}

pub struct App {
    pub presets: Vec<Preset>,
    pub preset_idx: usize,
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

    /// Edited params — clone of current preset, modified by sliders
    pub edited_params: std::collections::BTreeMap<String, f32>,
    pub params_dirty: bool,

    pub on_midi_reconnect: Option<Box<dyn FnMut(usize) -> Result<(), String>>>,
}

impl eframe::App for App {
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

        // Right: preset list
        egui::SidePanel::right("preset_panel")
            .resizable(true)
            .default_width(160.0)
            .min_width(120.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.strong("Presets");
                ui.add_space(4.0);
                ui.separator();

                let mut new_idx: Option<usize> = None;
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        for (i, preset) in self.presets.iter().enumerate() {
                            let selected = i == self.preset_idx;
                            if ui.selectable_label(selected, &preset.name).clicked() && !selected {
                                new_idx = Some(i);
                            }
                        }
                    });
                if let Some(idx) = new_idx {
                    self.preset_idx = idx;
                    self.load_edited_params();
                    self.send_edited_params();
                    self.save_config();
                }
            });

        // Settings window
        if self.show_settings {
            self.draw_settings(ctx);
        }

        // Central: parameters with sliders
        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_params_editable(ui);
        });
    }
}

impl App {
    /// Load edited_params from the current preset
    pub fn load_edited_params(&mut self) {
        if let Some(p) = self.presets.get(self.preset_idx) {
            self.edited_params = p.params.clone();
        }
        self.params_dirty = false;
    }

    /// Send edited params to synth engine
    fn send_edited_params(&mut self) {
        let preset = Preset {
            name: self.presets.get(self.preset_idx)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Custom".to_string()),
            params: self.edited_params.clone(),
        };
        let _ = self.ctrl_tx.push(ControlEvent::LoadPreset(preset));
    }

    fn save_config(&mut self) {
        self.config.ui.last_preset = self
            .presets
            .get(self.preset_idx)
            .map(|p| p.name.clone());
        let _ = self.config.save();
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

    fn draw_params_editable(&mut self, ui: &mut egui::Ui) {
        let preset_name = self
            .presets
            .get(self.preset_idx)
            .map(|p| p.name.as_str())
            .unwrap_or("(none)");
        ui.strong(preset_name);
        ui.add_space(6.0);

        let mut changed = false;

        // Oscillator
        ui.label("Oscillator");
        ui.horizontal(|ui| {
            let mut osc = self.edited_params.get("osc_type").copied().unwrap_or(0.0) as usize;
            ui.label("Type:");
            egui::ComboBox::from_id_salt("osc_type")
                .selected_text(*OSC_NAMES.get(osc).unwrap_or(&"?"))
                .show_ui(ui, |ui| {
                    for (i, name) in OSC_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut osc, i, *name).changed() {
                            self.edited_params.insert("osc_type".into(), osc as f32);
                            changed = true;
                        }
                    }
                });

            let mut detune = self.edited_params.get("osc_detune").copied().unwrap_or(0.0);
            ui.label("Detune:");
            if ui.add(egui::Slider::new(&mut detune, 0.0..=0.05).step_by(0.001)).changed() {
                self.edited_params.insert("osc_detune".into(), detune);
                changed = true;
            }
        });

        // FM params (only shown when FM osc selected)
        let osc_type = self.edited_params.get("osc_type").copied().unwrap_or(0.0) as u32;
        if osc_type == 4 {
            changed |= self.param_slider(ui, "fm_ratio", "FM Ratio", 0.5, 8.0, false);
            changed |= self.param_slider(ui, "fm_index", "FM Index", 0.1, 10.0, false);
        }

        ui.add_space(6.0);

        // Filter
        ui.label("Filter");
        ui.horizontal(|ui| {
            let mut ft = self.edited_params.get("filter_type").copied().unwrap_or(0.0) as usize;
            ui.label("Type:");
            egui::ComboBox::from_id_salt("filter_type")
                .selected_text(*FILTER_NAMES.get(ft).unwrap_or(&"?"))
                .show_ui(ui, |ui| {
                    for (i, name) in FILTER_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut ft, i, *name).changed() {
                            self.edited_params.insert("filter_type".into(), ft as f32);
                            changed = true;
                        }
                    }
                });
        });

        changed |= self.param_slider(ui, "filter_cutoff", "Cutoff", 20.0, 20000.0, true);
        changed |= self.param_slider(ui, "filter_resonance", "Resonance", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "filter_env_amount", "Env Amount", 0.0, 15000.0, false);

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

        // Volume
        changed |= self.param_slider(ui, "master_volume", "Volume", 0.0, 1.0, false);

        if changed {
            self.params_dirty = true;
            self.send_edited_params();
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if self.params_dirty {
                ui.colored_label(egui::Color32::YELLOW, "Modified");
                ui.separator();
            }
            if ui.button("Save as new preset...").clicked() {
                self.save_as_user_preset();
            }
            if self.params_dirty {
                if ui.button("Reset").clicked() {
                    self.load_edited_params();
                    self.send_edited_params();
                }
            }
        });
    }

    /// Draw a parameter slider, returns true if changed
    fn param_slider(&mut self, ui: &mut egui::Ui, key: &str, label: &str, min: f32, max: f32, logarithmic: bool) -> bool {
        let mut val = self.edited_params.get(key).copied().unwrap_or(min);
        let slider = egui::Slider::new(&mut val, min..=max)
            .text(label)
            .logarithmic(logarithmic);
        let resp = ui.add(slider);
        if resp.changed() {
            self.edited_params.insert(key.into(), val);
            return true;
        }
        false
    }

    fn save_as_user_preset(&mut self) {
        let base_name = self.presets.get(self.preset_idx)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Custom".to_string());

        let new_name = format!("{base_name} (user)");
        let new_preset = Preset {
            name: new_name.clone(),
            params: self.edited_params.clone(),
        };

        if let Ok(path) = preset::save_preset(&new_preset) {
            self.settings_status = format!("Saved: {}", path.display());
            self.presets.push(new_preset);
            self.preset_idx = self.presets.len() - 1;
            self.params_dirty = false;
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
