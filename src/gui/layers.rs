/// Layer tab controls, split mode, and performance save/load.

use std::sync::atomic::Ordering;

use eframe::egui;

use crate::preset::{self, Performance, PartConfig};
use crate::synth::{ControlEvent, MAX_LAYERS};

use super::App;
use super::{LAYER_NAMES, note_name};

impl App {
    pub(super) fn draw_layer_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for i in 0..MAX_LAYERS {
                let label = *LAYER_NAMES.get(i).unwrap_or(&"?");
                let is_active = i == self.active_layer && !self.show_drums && !self.show_looper && !self.show_midi_seq;
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
                    self.show_midi_seq = false;
                    self.seq_target_atom.store(1, Ordering::Relaxed); // synth → looper
                }
            }

            // Drums tab
            if ui.selectable_label(self.show_drums && !self.show_midi_seq, "Drums").clicked() {
                self.show_drums = true;
                self.show_looper = false;
                self.show_midi_seq = false;
                self.seq_target_atom.store(0, Ordering::Relaxed); // drums
            }

            // MIDI Sequencer tab
            if ui.selectable_label(self.show_midi_seq, "MIDI Seq").clicked() {
                self.show_midi_seq = true;
                self.show_drums = false;
                self.show_looper = false;
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
}
