//! Part management with Split A/B zones.
//!
//! Zone A = Parts 0-3 (always active)
//! Zone B = Parts 4-7 (activated by Split checkbox)
//!
//! When Split is OFF: Zone A parts play full range 0-127
//! When Split is ON:
//!   Zone A parts → 0 .. split_point-1
//!   Zone B parts → split_point .. 127

use std::sync::atomic::Ordering;
use eframe::egui;
use crate::preset::{self, Performance, PartConfig};
use crate::synth::ControlEvent;
use super::App;
use super::note_name;
use super::theme;

const ZONE_A: std::ops::Range<usize> = 0..4;
const ZONE_B: std::ops::Range<usize> = 4..8;

impl App {
    pub(super) fn draw_part_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // ── Zone A part tabs ─────────────────────────────────────────
            for i in ZONE_A {
                if !self.parts[i].enabled { continue; }
                self.draw_part_tab(ui, i);
            }

            // Add to Zone A
            let a_count = ZONE_A.filter(|&i| self.parts[i].enabled).count();
            if a_count < 4 && ui.small_button("+ A").on_hover_text("Add part to Zone A").clicked() {
                self.add_part_to_zone(false);
            }

            // ── Split separator + Zone B ─────────────────────────────────
            ui.separator();

            let split_on = self.parts[ZONE_B].iter().any(|l| l.enabled);
            let mut split = split_on;
            if ui.checkbox(&mut split, "Split").on_hover_text("Enable keyboard split").changed() {
                if split {
                    // Enable the first free B slot
                    if let Some(i) = ZONE_B.into_iter().find(|&i| !self.parts[i].enabled) {
                        self.parts[i].enabled = true;
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: true });
                        self.send_edited_params(i);
                        self.send_part_volume(i);
                    }
                    self.apply_split_ranges();
                } else {
                    // Disable all Zone B parts, restore Zone A to full range
                    for i in ZONE_B {
                        if self.parts[i].enabled {
                            self.parts[i].enabled = false;
                            let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: false });
                        }
                    }
                    for i in ZONE_A {
                        if self.parts[i].enabled {
                            self.parts[i].min_note = 0;
                            self.parts[i].max_note = 127;
                            self.send_part_range(i);
                        }
                    }
                    // Switch active part to Zone A if needed
                    if self.active_part >= 4 {
                        self.set_active_part(0);
                    }
                }
            }

            if split_on {
                for i in ZONE_B {
                    if !self.parts[i].enabled { continue; }
                    self.draw_part_tab(ui, i);
                }
                let b_count = ZONE_B.filter(|&i| self.parts[i].enabled).count();
                if b_count < 4 && ui.small_button("+ B").on_hover_text("Add part to Zone B").clicked() {
                    self.add_part_to_zone(true);
                }
            }

            // ── Other tabs ───────────────────────────────────────────────
            ui.separator();
            if ui.selectable_label(self.show_drums && !self.show_midi_seq, "Drums").on_hover_text("Drum sequencer").clicked() {
                self.show_drums = true; self.show_looper = false;
                self.show_midi_seq = false; self.show_fx_chain = false;
                self.seq_target_atom.store(0, Ordering::Relaxed);
            }
            if ui.selectable_label(self.show_midi_seq, "MIDI Seq").on_hover_text("MIDI file sequencer").clicked() {
                self.show_midi_seq = true; self.show_drums = false;
                self.show_looper = false; self.show_fx_chain = false;
            }
            if ui.selectable_label(self.show_fx_chain, "FX Chain").on_hover_text("16-slot serial FX chain").clicked() {
                self.show_fx_chain = true; self.show_drums = false;
                self.show_looper = false; self.show_midi_seq = false;
            }
        });

        // ── Split point slider ───────────────────────────────────────────
        let split_on = self.parts[ZONE_B].iter().any(|l| l.enabled);
        if split_on {
            ui.horizontal(|ui| {
                ui.label("Split point:");
                let sp_name = note_name(self.split_point);
                if ui.add(egui::Slider::new(&mut self.split_point, 0..=127).text(sp_name)).on_hover_text("Keyboard split note").changed() {
                    self.apply_split_ranges();
                }
            });
        }

        // ── Per-part routing strip ───────────────────────────────────────
        let part = self.active_part;
        if !self.show_drums && !self.show_midi_seq && !self.show_fx_chain
            && self.parts.get(part).map(|l| l.enabled).unwrap_or(false)
        {
            let zone_label = if part < 4 { "A" } else { "B" };
            ui.horizontal(|ui| {
                ui.strong(format!("Part {}{}: ", zone_label, (part % 4) + 1));

                let mut muted = self.parts[part].mute;
                if ui.toggle_value(&mut muted, "Mute").on_hover_text("Mute this part").changed() {
                    self.parts[part].mute = muted;
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part, mute: muted });
                }

                ui.label("Vol:");
                let mut vol = self.parts[part].volume;
                let vol_r = ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).fixed_decimals(2))
                    .on_hover_text("Double-click to reset");
                if vol_r.double_clicked() { vol = 0.8; }
                if vol_r.changed() || vol_r.double_clicked() {
                    self.parts[part].volume = vol;
                    self.send_part_volume(part);
                }

                ui.label("Pan:");
                let mut pan = self.parts[part].pan;
                let pan_r = ui.add(egui::Slider::new(&mut pan, -1.0..=1.0).fixed_decimals(2))
                    .on_hover_text("Double-click to reset");
                if pan_r.double_clicked() { pan = 0.0; }
                if pan_r.changed() || pan_r.double_clicked() {
                    self.parts[part].pan = pan;
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part, pan });
                }

                ui.label("Tr:");
                let mut tr = self.parts[part].transpose as i32;
                let tr_r = ui.add(egui::Slider::new(&mut tr, -24..=24).suffix(" st"))
                    .on_hover_text("Transpose in semitones. Double-click to reset");
                if tr_r.double_clicked() { tr = 0; }
                if tr_r.changed() || tr_r.double_clicked() {
                    self.parts[part].transpose = tr as i8;
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part, semitones: tr as i8 });
                }

                ui.label("Vel:");
                let mut vmin = self.parts[part].vel_min as i32;
                let mut vmax = self.parts[part].vel_max as i32;
                let c1 = ui.add(egui::DragValue::new(&mut vmin).range(1..=127).speed(1)).changed();
                ui.label("-");
                let c2 = ui.add(egui::DragValue::new(&mut vmax).range(1..=127).speed(1)).changed();
                let vel_changed = c1 || c2;
                if vel_changed {
                    let vmin = (vmin as u8).max(1);
                    let vmax = (vmax as u8).max(vmin);
                    self.parts[part].vel_min = vmin;
                    self.parts[part].vel_max = vmax;
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartVelRange { part, vel_min: vmin, vel_max: vmax });
                }

                // Remove: not for first part of each zone
                let is_zone_first = part == 0 || part == 4;
                if !is_zone_first && ui.small_button("✕").on_hover_text("Remove this part").clicked() {
                    self.remove_part(part);
                }
            });

            draw_key_zone_map(ui, &self.parts, self.active_part);
        }

        // ── Performance save/load ────────────────────────────────────────
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("Perf:");
            ui.add(egui::TextEdit::singleline(&mut self.perf_name)
                .desired_width(110.0).hint_text("name"));
            if ui.button("Save").on_hover_text("Save performance").clicked() && !self.perf_name.trim().is_empty() {
                let parts: Vec<PartConfig> = self.parts.iter().map(|l| {
                    let patch_name = self.patches.get(l.patch_idx)
                        .map(|p| p.name.clone()).unwrap_or_default();
                    PartConfig {
                        patch_name, enabled: l.enabled, volume: l.volume,
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
                    for i in 0..self.parts.len() {
                        self.parts[i].enabled = false;
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: false });
                    }
                    for (i, part) in perf.parts.iter().enumerate() {
                        if i >= self.parts.len() { break; }
                        let patch_idx = self.patches.iter()
                            .position(|p| p.name == part.patch_name).unwrap_or(0);
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
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: part.sf2_mode });
                        if part.sf2_mode {
                            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                                part: i, program: part.sf2_program, bank: 0 });
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
        let is_active = i == self.active_part
            && !self.show_drums && !self.show_looper
            && !self.show_midi_seq && !self.show_fx_chain;
        let zone = if i < 4 { "A" } else { "B" };
        let num = (i % 4) + 1;
        let muted = self.parts[i].mute;
        let label = if muted {
            format!("[{zone}{num} M]")
        } else {
            format!("[{zone}{num}]")
        };
        let color = if muted { egui::Color32::GRAY } else { egui::Color32::WHITE };
        if ui.add(egui::Button::new(egui::RichText::new(&label).color(color)).selected(is_active))
            .on_hover_text(format!("Switch to part {zone}{num}"))
            .clicked() {
            self.set_active_part(i);
            self.show_drums = false; self.show_looper = false;
            self.show_midi_seq = false; self.show_fx_chain = false;
            self.seq_target_atom.store(1, Ordering::Relaxed);
        }
    }

    fn add_part_to_zone(&mut self, zone_b: bool) {
        let range = if zone_b { ZONE_B } else { ZONE_A };
        if let Some(i) = range.into_iter().find(|&i| !self.parts[i].enabled) {
            self.parts[i].enabled = true;
            self.parts[i].mute = false;
            self.parts[i].volume = 0.8;
            self.parts[i].patch_idx = 0;
            self.parts[i].sf2_mode = false;
            let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: true });
            self.send_edited_params(i);
            self.send_part_volume(i);
            let split_on = self.parts[ZONE_B].iter().any(|p| p.enabled);
            if split_on {
                self.apply_split_ranges();
            } else {
                // No split — new part gets full range
                self.parts[i].min_note = 0;
                self.parts[i].max_note = 127;
                self.send_part_range(i);
            }
            self.set_active_part(i);
            self.show_drums = false; self.show_midi_seq = false; self.show_fx_chain = false;
        }
    }

    fn remove_part(&mut self, part: usize) {
        self.parts[part].enabled = false;
        self.parts[part].mute = false;
        let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part, enabled: false });
        self.set_active_part(if part >= 4 { 4 } else { 0 });
        // find first enabled in same zone
        let zone = if part >= 4 { ZONE_B } else { ZONE_A };
        if let Some(i) = zone.into_iter().find(|&i| self.parts[i].enabled) {
            self.set_active_part(i);
        }
    }

    /// Set key ranges for all parts based on split_point.
    /// Zone A: 0 .. split_point-1
    /// Zone B: split_point .. 127
    fn apply_split_ranges(&mut self) {
        let sp = self.split_point;
        for i in ZONE_A {
            if self.parts[i].enabled {
                self.parts[i].min_note = 0;
                self.parts[i].max_note = sp.saturating_sub(1);
                self.send_part_range(i);
            }
        }
        for i in ZONE_B {
            if self.parts[i].enabled {
                self.parts[i].min_note = sp;
                self.parts[i].max_note = 127;
                self.send_part_range(i);
            }
        }
    }

}

fn draw_key_zone_map(ui: &mut egui::Ui, parts: &[super::PartState], active: usize) {
    let colors = theme::PART_COLORS;
    let total_w = ui.available_width().min(500.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, 8.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme::BG_PANEL);
    for (i, part) in parts.iter().enumerate() {
        if !part.enabled { continue; }
        let c = colors[i % colors.len()];
        let color = if i == active { c }
            else { egui::Color32::from_rgba_premultiplied(c.r()/2, c.g()/2, c.b()/2, 180) };
        let lo = part.min_note as f32 / 127.0;
        let hi = (part.max_note as f32 + 1.0) / 127.0;
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
            egui::Stroke::new(0.5, theme::STROKE_ZONE_DIV),
        );
    }
}
