/// Part management strip — replaces the old "Layer A/B Split" UI.
/// Supports up to MAX_LAYERS (8) parts with per-part routing controls.

use std::sync::atomic::Ordering;

use eframe::egui;

use crate::preset::{self, Performance, PartConfig};
use crate::synth::{ControlEvent, MAX_LAYERS};

use super::App;
use super::{LAYER_NAMES, note_name};

impl App {
    pub(super) fn draw_layer_tabs(&mut self, ui: &mut egui::Ui) {
        // ── Part selection buttons ──────────────────────────────────────────
        ui.horizontal(|ui| {
            for i in 0..MAX_LAYERS {
                if !self.layers[i].enabled { continue; }
                let name = LAYER_NAMES.get(i).copied().unwrap_or("?");
                let is_active = i == self.active_layer
                    && !self.show_drums && !self.show_looper
                    && !self.show_midi_seq && !self.show_fx_chain;

                let muted = self.layers[i].mute;
                let label = if muted {
                    format!("[{name} M]")
                } else {
                    format!("[{name}]")
                };

                let btn = egui::Button::new(
                    egui::RichText::new(&label)
                        .color(if muted { egui::Color32::GRAY } else { egui::Color32::WHITE })
                );
                if ui.add(btn.selected(is_active)).clicked() {
                    self.active_layer = i;
                    self.show_drums = false;
                    self.show_looper = false;
                    self.show_midi_seq = false;
                    self.show_fx_chain = false;
                    self.seq_target_atom.store(1, Ordering::Relaxed);
                }
            }

            // Add Part button (if room exists)
            let enabled_count = self.layers.iter().filter(|l| l.enabled).count();
            if enabled_count < MAX_LAYERS {
                if ui.small_button("+ Part").clicked() {
                    self.add_part();
                }
            }

            ui.separator();

            // Drums / MIDI Seq / FX Chain tabs
            if ui.selectable_label(self.show_drums && !self.show_midi_seq, "Drums").clicked() {
                self.show_drums = true;
                self.show_looper = false;
                self.show_midi_seq = false;
                self.show_fx_chain = false;
                self.seq_target_atom.store(0, Ordering::Relaxed);
            }
            if ui.selectable_label(self.show_midi_seq, "MIDI Seq").clicked() {
                self.show_midi_seq = true;
                self.show_drums = false;
                self.show_looper = false;
                self.show_fx_chain = false;
            }
            if ui.selectable_label(self.show_fx_chain, "FX Chain").clicked() {
                self.show_fx_chain = true;
                self.show_drums = false;
                self.show_looper = false;
                self.show_midi_seq = false;
            }
        });

        // ── Per-part routing strip (for active part) ────────────────────────
        let layer = self.active_layer;
        if !self.show_drums && !self.show_midi_seq && !self.show_fx_chain
            && self.layers.get(layer).map(|l| l.enabled).unwrap_or(false)
        {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let name = LAYER_NAMES.get(layer).copied().unwrap_or("?");
                ui.strong(format!("Part {name}:"));

                // Mute toggle
                let mut muted = self.layers[layer].mute;
                if ui.toggle_value(&mut muted, "Mute").changed() {
                    self.layers[layer].mute = muted;
                    let _ = self.ctrl_tx.push(ControlEvent::SetLayerMute { layer, mute: muted });
                }

                // Volume
                ui.label("Vol:");
                let mut vol = self.layers[layer].volume;
                if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false)).changed() {
                    self.layers[layer].volume = vol;
                    self.send_layer_volume(layer);
                }

                // Pan
                ui.label("Pan:");
                let mut pan = self.layers[layer].pan;
                if ui.add(egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false)).changed() {
                    self.layers[layer].pan = pan;
                    let _ = self.ctrl_tx.push(ControlEvent::SetLayerPan { layer, pan });
                }

                // Transpose
                ui.label("Tr:");
                let mut transpose = self.layers[layer].transpose as i32;
                if ui.add(egui::Slider::new(&mut transpose, -24..=24).show_value(true)).changed() {
                    self.layers[layer].transpose = transpose as i8;
                    let _ = self.ctrl_tx.push(ControlEvent::SetLayerTranspose { layer, semitones: transpose as i8 });
                }

                // Remove Part button (not for Part 1)
                if layer > 0 {
                    if ui.small_button("✕ Remove").clicked() {
                        self.remove_part(layer);
                        return;
                    }
                }
            });

            // Key range + velocity range
            ui.horizontal(|ui| {
                ui.label("Keys:");
                let mut lo = self.layers[layer].min_note;
                let mut hi = self.layers[layer].max_note;
                let lo_name = note_name(lo);
                let hi_name = note_name(hi);
                if ui.add(egui::Slider::new(&mut lo, 0..=127).text(format!("Lo {lo_name}"))).changed() {
                    lo = lo.min(hi);
                    self.layers[layer].min_note = lo;
                    self.send_layer_range(layer);
                }
                if ui.add(egui::Slider::new(&mut hi, 0..=127).text(format!("Hi {hi_name}"))).changed() {
                    hi = hi.max(lo);
                    self.layers[layer].max_note = hi;
                    self.send_layer_range(layer);
                }

                ui.separator();
                ui.label("Vel:");
                let mut vlo = self.layers[layer].vel_min as i32;
                let mut vhi = self.layers[layer].vel_max as i32;
                let vlo_label = format!("Lo {vlo}");
                if ui.add(egui::Slider::new(&mut vlo, 1..=127).text(vlo_label)).changed() {
                    vlo = vlo.min(vhi);
                    self.layers[layer].vel_min = vlo as u8;
                    let vmax = self.layers[layer].vel_max;
                    let _ = self.ctrl_tx.push(ControlEvent::SetLayerVelRange {
                        layer, vel_min: vlo as u8, vel_max: vmax,
                    });
                }
                let vhi_label = format!("Hi {vhi}");
                if ui.add(egui::Slider::new(&mut vhi, 1..=127).text(vhi_label)).changed() {
                    vhi = vhi.max(vlo);
                    self.layers[layer].vel_max = vhi as u8;
                    let vmin = self.layers[layer].vel_min;
                    let _ = self.ctrl_tx.push(ControlEvent::SetLayerVelRange {
                        layer, vel_min: vmin, vel_max: vhi as u8,
                    });
                }
            });

            // Key zone mini-map (color band per part)
            draw_key_zone_map(ui, &self.layers, self.active_layer);
        }

        // ── Performance save/load ───────────────────────────────────────────
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("Perf:");
            ui.add(egui::TextEdit::singleline(&mut self.perf_name)
                .desired_width(110.0)
                .hint_text("name"));
            if ui.button("Save").clicked() && !self.perf_name.trim().is_empty() {
                let parts: Vec<PartConfig> = self.layers.iter().enumerate().map(|(_i, l)| {
                    let preset_name = self.presets.get(l.preset_idx)
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    PartConfig {
                        preset_name,
                        enabled: l.enabled,
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
                .width(120.0)
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
                        // First disable all parts, then load
                        for i in 1..self.layers.len() {
                            self.layers[i].enabled = false;
                            let _ = self.ctrl_tx.push(ControlEvent::SetLayerEnabled { layer: i, enabled: false });
                        }
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

    /// Add a new part (next disabled slot).
    fn add_part(&mut self) {
        let slot = self.layers.iter().position(|l| !l.enabled);
        if let Some(i) = slot {
            self.layers[i].enabled = true;
            self.layers[i].mute = false;
            self.layers[i].min_note = 0;
            self.layers[i].max_note = 127;
            self.layers[i].vel_min = 1;
            self.layers[i].vel_max = 127;
            self.layers[i].pan = 0.0;
            self.layers[i].transpose = 0;
            self.layers[i].volume = 0.8;
            self.layers[i].preset_idx = 0;
            self.layers[i].sf2_mode = false;
            // Send init preset so engine is ready
            let _ = self.ctrl_tx.push(ControlEvent::SetLayerEnabled { layer: i, enabled: true });
            self.send_edited_params(i);
            self.send_layer_volume(i);
            self.send_layer_range(i);
            // Switch to new part
            self.active_layer = i;
            self.show_drums = false;
            self.show_midi_seq = false;
            self.show_fx_chain = false;
        }
    }

    /// Remove a part (layer > 0 only).
    fn remove_part(&mut self, layer: usize) {
        if layer == 0 { return; } // Part 1 always stays
        self.layers[layer].enabled = false;
        self.layers[layer].mute = false;
        let _ = self.ctrl_tx.push(ControlEvent::SetLayerEnabled { layer, enabled: false });
        // Switch to Part 1
        self.active_layer = 0;
    }
}


/// Draw a narrow keyboard zone map showing all enabled parts color-coded.
fn draw_key_zone_map(ui: &mut egui::Ui, layers: &[super::LayerState], active: usize) {
    let part_colors = [
        egui::Color32::from_rgb(70, 130, 200),  // blue
        egui::Color32::from_rgb(70, 190, 100),  // green
        egui::Color32::from_rgb(220, 150, 50),  // orange
        egui::Color32::from_rgb(200, 70, 70),   // red
        egui::Color32::from_rgb(160, 80, 200),  // purple
        egui::Color32::from_rgb(50, 190, 190),  // cyan
        egui::Color32::from_rgb(220, 200, 50),  // yellow
        egui::Color32::from_rgb(190, 100, 150), // pink
    ];

    let total_w = ui.available_width().min(500.0);
    let bar_h = 8.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, bar_h), egui::Sense::hover());
    let painter = ui.painter();

    // Background
    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));

    for (i, layer) in layers.iter().enumerate() {
        if !layer.enabled { continue; }
        let color = if i == active {
            part_colors[i % part_colors.len()]
        } else {
            let c = part_colors[i % part_colors.len()];
            egui::Color32::from_rgba_premultiplied(c.r()/2, c.g()/2, c.b()/2, 180)
        };
        let lo = layer.min_note as f32 / 127.0;
        let hi = (layer.max_note as f32 + 1.0) / 127.0;
        let x0 = rect.left() + lo * total_w;
        let x1 = rect.left() + hi * total_w;
        let zone = egui::Rect::from_min_max(
            egui::pos2(x0, rect.top()),
            egui::pos2(x1.min(rect.right()), rect.bottom()),
        );
        painter.rect_filled(zone, 0.0, color);
    }
    // Octave lines
    for oct in 0..11 {
        let x = rect.left() + (oct * 12) as f32 / 127.0 * total_w;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(0.5, egui::Color32::from_gray(60)),
        );
    }
}
