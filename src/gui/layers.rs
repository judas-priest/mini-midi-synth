/// Part management with Split A/B zones.
///
/// Zone A = Parts 0-3 (always active)
/// Zone B = Parts 4-7 (activated by Split checkbox)
///
/// When Split is OFF: Zone A parts play full range 0-127
/// When Split is ON:
///   Zone A parts → 0 .. split_point-1
///   Zone B parts → split_point .. 127

use std::sync::atomic::Ordering;
use eframe::egui;
use crate::preset::{self, Performance, PartConfig};
use crate::synth::ControlEvent;
use super::App;
use super::note_name;

const ZONE_A: std::ops::Range<usize> = 0..4;
const ZONE_B: std::ops::Range<usize> = 4..8;

impl App {
    pub(super) fn draw_layer_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // ── Zone A part tabs ─────────────────────────────────────────
            for i in ZONE_A {
                if !self.layers[i].enabled { continue; }
                self.draw_part_tab(ui, i);
            }

            // Add to Zone A
            let a_count = (ZONE_A).filter(|&i| self.layers[i].enabled).count();
            if a_count < 4 && ui.small_button("+ A").clicked() {
                self.add_part_to_zone(false);
            }

            // ── Split separator + Zone B ─────────────────────────────────
            ui.separator();

            let split_on = self.layers[ZONE_B].iter().any(|l| l.enabled);
            let mut split = split_on;
            if ui.checkbox(&mut split, "Split").changed() {
                if split {
                    // Enable the first free B slot
                    if let Some(i) = (ZONE_B).find(|&i| !self.layers[i].enabled) {
                        self.layers[i].enabled = true;
                        let _ = self.ctrl_tx.push(ControlEvent::SetLayerEnabled { layer: i, enabled: true });
                        self.send_edited_params(i);
                        self.send_layer_volume(i);
                    }
                    self.apply_split_ranges();
                } else {
                    // Disable all Zone B parts, restore Zone A to full range
                    for i in ZONE_B {
                        if self.layers[i].enabled {
                            self.layers[i].enabled = false;
                            let _ = self.ctrl_tx.push(ControlEvent::SetLayerEnabled { layer: i, enabled: false });
                        }
                    }
                    for i in ZONE_A {
                        if self.layers[i].enabled {
                            self.layers[i].min_note = 0;
                            self.layers[i].max_note = 127;
                            self.send_layer_range(i);
                        }
                    }
                    // Switch active layer to Zone A if needed
                    if self.active_layer >= 4 {
                        self.active_layer = 0;
                    }
                }
            }

            if split_on {
                for i in ZONE_B {
                    if !self.layers[i].enabled { continue; }
                    self.draw_part_tab(ui, i);
                }
                let b_count = (ZONE_B).filter(|&i| self.layers[i].enabled).count();
                if b_count < 4 && ui.small_button("+ B").clicked() {
                    self.add_part_to_zone(true);
                }
            }

            // ── Other tabs ───────────────────────────────────────────────
            ui.separator();
            if ui.selectable_label(self.show_drums && !self.show_midi_seq, "Drums").clicked() {
                self.show_drums = true; self.show_looper = false;
                self.show_midi_seq = false; self.show_fx_chain = false;
                self.seq_target_atom.store(0, Ordering::Relaxed);
            }
            if ui.selectable_label(self.show_midi_seq, "MIDI Seq").clicked() {
                self.show_midi_seq = true; self.show_drums = false;
                self.show_looper = false; self.show_fx_chain = false;
            }
            if ui.selectable_label(self.show_fx_chain, "FX Chain").clicked() {
                self.show_fx_chain = true; self.show_drums = false;
                self.show_looper = false; self.show_midi_seq = false;
            }
        });

        // ── Split point slider ───────────────────────────────────────────
        let split_on = self.layers[ZONE_B].iter().any(|l| l.enabled);
        if split_on {
            ui.horizontal(|ui| {
                ui.label("Split point:");
                let sp_name = note_name(self.split_point);
                if ui.add(egui::Slider::new(&mut self.split_point, 0..=127).text(sp_name)).changed() {
                    self.apply_split_ranges();
                }
            });
        }

        // ── Per-part routing strip ───────────────────────────────────────
        let layer = self.active_layer;
        if !self.show_drums && !self.show_midi_seq && !self.show_fx_chain
            && self.layers.get(layer).map(|l| l.enabled).unwrap_or(false)
        {
            let zone_label = if layer < 4 { "A" } else { "B" };
            ui.horizontal(|ui| {
                ui.strong(format!("Part {}{}: ", zone_label, (layer % 4) + 1));

                let mut muted = self.layers[layer].mute;
                if ui.toggle_value(&mut muted, "Mute").changed() {
                    self.layers[layer].mute = muted;
                    let _ = self.ctrl_tx.push(ControlEvent::SetLayerMute { layer, mute: muted });
                }

                ui.label("Vol:");
                let mut vol = self.layers[layer].volume;
                if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false)).changed() {
                    self.layers[layer].volume = vol;
                    self.send_layer_volume(layer);
                }

                ui.label("Pan:");
                let mut pan = self.layers[layer].pan;
                if ui.add(egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false)).changed() {
                    self.layers[layer].pan = pan;
                    let _ = self.ctrl_tx.push(ControlEvent::SetLayerPan { layer, pan });
                }

                ui.label("Tr:");
                let mut tr = self.layers[layer].transpose as i32;
                if ui.add(egui::Slider::new(&mut tr, -24..=24).show_value(true)).changed() {
                    self.layers[layer].transpose = tr as i8;
                    let _ = self.ctrl_tx.push(ControlEvent::SetLayerTranspose { layer, semitones: tr as i8 });
                }

                // Remove: not for first part of each zone
                let is_zone_first = layer == 0 || layer == 4;
                if !is_zone_first && ui.small_button("✕").clicked() {
                    self.remove_part(layer);
                }
            });

            draw_key_zone_map(ui, &self.layers, self.active_layer);
        }

        // ── Performance save/load ────────────────────────────────────────
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("Perf:");
            ui.add(egui::TextEdit::singleline(&mut self.perf_name)
                .desired_width(110.0).hint_text("name"));
            if ui.button("Save").clicked() && !self.perf_name.trim().is_empty() {
                let parts: Vec<PartConfig> = self.layers.iter().enumerate().map(|(_i, l)| {
                    let preset_name = self.presets.get(l.preset_idx)
                        .map(|p| p.name.clone()).unwrap_or_default();
                    PartConfig {
                        preset_name, enabled: l.enabled, volume: l.volume,
                        key_low: l.min_note, key_high: l.max_note,
                        param_overrides: l.edited_params.clone(),
                        sf2_mode: l.sf2_mode, sf2_program: l.sf2_program,
                    }
                }).collect();
                match preset::save_performance(&Performance {
                    name: self.perf_name.trim().to_string(),
                    category: String::new(), parts,
                }) {
                    Ok(path) => { self.perf_status = format!("Saved: {}", path.display());
                                  self.perf_list = preset::list_performances(); }
                    Err(e)   => { self.perf_status = format!("Error: {e}"); }
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
                if let Ok(perf) = preset::load_performance(&path) {
                    self.perf_name = perf.name.clone();
                    for i in 0..self.layers.len() {
                        self.layers[i].enabled = false;
                        let _ = self.ctrl_tx.push(ControlEvent::SetLayerEnabled { layer: i, enabled: false });
                    }
                    for (i, part) in perf.parts.iter().enumerate() {
                        if i >= self.layers.len() { break; }
                        let preset_idx = self.presets.iter()
                            .position(|p| p.name == part.preset_name).unwrap_or(0);
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
                                layer: i, program: part.sf2_program, bank: 0 });
                        }
                    }
                    self.perf_status = format!("Loaded: {}", perf.name);
                }
            }
            if !self.perf_status.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new(&self.perf_status).small().weak());
            }
        });
    }

    fn draw_part_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let is_active = i == self.active_layer
            && !self.show_drums && !self.show_looper
            && !self.show_midi_seq && !self.show_fx_chain;
        let zone = if i < 4 { "A" } else { "B" };
        let num = (i % 4) + 1;
        let muted = self.layers[i].mute;
        let label = if muted {
            format!("[{zone}{num} M]")
        } else {
            format!("[{zone}{num}]")
        };
        let color = if muted { egui::Color32::GRAY } else { egui::Color32::WHITE };
        if ui.add(egui::Button::new(egui::RichText::new(&label).color(color)).selected(is_active)).clicked() {
            self.active_layer = i;
            self.show_drums = false; self.show_looper = false;
            self.show_midi_seq = false; self.show_fx_chain = false;
            self.seq_target_atom.store(1, Ordering::Relaxed);
        }
    }

    fn add_part_to_zone(&mut self, zone_b: bool) {
        let mut range = if zone_b { ZONE_B } else { ZONE_A };
        if let Some(i) = range.find(|&i| !self.layers[i].enabled) {
            self.layers[i].enabled = true;
            self.layers[i].mute = false;
            self.layers[i].volume = 0.8;
            self.layers[i].preset_idx = 0;
            self.layers[i].sf2_mode = false;
            let _ = self.ctrl_tx.push(ControlEvent::SetLayerEnabled { layer: i, enabled: true });
            self.send_edited_params(i);
            self.send_layer_volume(i);
            self.apply_split_ranges();
            self.active_layer = i;
            self.show_drums = false; self.show_midi_seq = false; self.show_fx_chain = false;
        }
    }

    fn remove_part(&mut self, layer: usize) {
        self.layers[layer].enabled = false;
        self.layers[layer].mute = false;
        let _ = self.ctrl_tx.push(ControlEvent::SetLayerEnabled { layer, enabled: false });
        self.active_layer = if layer >= 4 { 4 } else { 0 };
        // find first enabled in same zone
        let mut zone = if layer >= 4 { ZONE_B } else { ZONE_A };
        if let Some(i) = zone.find(|&i| self.layers[i].enabled) {
            self.active_layer = i;
        }
    }

    /// Set key ranges for all parts based on split_point.
    /// Zone A: 0 .. split_point-1
    /// Zone B: split_point .. 127
    fn apply_split_ranges(&mut self) {
        let sp = self.split_point;
        for i in ZONE_A {
            if self.layers[i].enabled {
                self.layers[i].min_note = 0;
                self.layers[i].max_note = sp.saturating_sub(1);
                self.send_layer_range(i);
            }
        }
        for i in ZONE_B {
            if self.layers[i].enabled {
                self.layers[i].min_note = sp;
                self.layers[i].max_note = 127;
                self.send_layer_range(i);
            }
        }
    }

}

fn draw_key_zone_map(ui: &mut egui::Ui, layers: &[super::LayerState], active: usize) {
    let colors = [
        egui::Color32::from_rgb(70, 130, 200),
        egui::Color32::from_rgb(70, 190, 100),
        egui::Color32::from_rgb(220, 150, 50),
        egui::Color32::from_rgb(200, 70, 70),
        egui::Color32::from_rgb(160, 80, 200),
        egui::Color32::from_rgb(50, 190, 190),
        egui::Color32::from_rgb(220, 200, 50),
        egui::Color32::from_rgb(190, 100, 150),
    ];
    let total_w = ui.available_width().min(500.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, 8.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));
    for (i, layer) in layers.iter().enumerate() {
        if !layer.enabled { continue; }
        let c = colors[i % colors.len()];
        let color = if i == active { c }
            else { egui::Color32::from_rgba_premultiplied(c.r()/2, c.g()/2, c.b()/2, 180) };
        let lo = layer.min_note as f32 / 127.0;
        let hi = (layer.max_note as f32 + 1.0) / 127.0;
        let zone = egui::Rect::from_min_max(
            egui::pos2(rect.left() + lo * total_w, rect.top()),
            egui::pos2((rect.left() + hi * total_w).min(rect.right()), rect.bottom()),
        );
        painter.rect_filled(zone, 0.0, color);
    }
    for oct in 0..11 {
        let x = rect.left() + (oct * 12) as f32 / 127.0 * total_w;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(0.5, egui::Color32::from_gray(60)),
        );
    }
}
