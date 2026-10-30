//! Part management with Split A/B zones.
//!
//! Zone A = Parts 0-7 (always active)
//! Zone B = Parts 8-15 (activated by Split checkbox)
//!
//! All enabled parts are shown as ROWS (one row per part, all fields inline):
//! label, engine toggle (DSP/SF2), instrument select, Mute, Root, key range,
//! Pan, Tr, Vel, Volume, Remove. Visual spec: docs/mockup_layers2.html.
//!
//! When Split is OFF: Zone A parts play full range 0-127
//! When Split is ON:
//!   Zone A parts → 0 .. split_point-1
//!   Zone B parts → split_point .. 127

use std::sync::atomic::Ordering;
use eframe::egui;
use crate::preset::{self, Performance, PartConfig};
use crate::synth::ControlEvent;
use super::{App, ToastKind};
use super::note_name;
use super::theme;

const PARTS_PER_ZONE: usize = 8;
const ZONE_A: std::ops::Range<usize> = 0..PARTS_PER_ZONE;
const ZONE_B: std::ops::Range<usize> = PARTS_PER_ZONE..PARTS_PER_ZONE * 2;

impl App {
    pub(super) fn draw_part_rows(&mut self, ui: &mut egui::Ui) {
        // ── Split row (top): toggle + point + view tabs ──────────────────
        let split_on = self.parts[ZONE_B].iter().any(|l| l.enabled);
        ui.horizontal(|ui| {
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
                    if self.active_part >= PARTS_PER_ZONE {
                        self.set_active_part(0);
                    }
                }
            }

            if split_on {
                ui.label("Split point:");
                let sp_name = note_name(self.split_point);
                if ui.add(egui::Slider::new(&mut self.split_point, 0..=127)
                    .suffix(format!(" ({sp_name})")))
                    .on_hover_text("Keyboard split note (zone B starts here)")
                    .changed()
                {
                    self.apply_split_ranges();
                }
            }

            // ── View tabs (Drums / MIDI Seq / FX Chain / Pads) ───────────
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
            // NAV long-press opens this too, but NAV SysEx only arrives over
            // Bluetooth — USB users need a GUI path.
            if ui.selectable_label(self.show_pad_perf, "Pads").on_hover_text("Pad Performances (assign performances to pads 36-51)").clicked() {
                self.show_pad_perf = !self.show_pad_perf;
            }
        });

        // ── Part rows (two zones) ────────────────────────────────────────
        let max_h = (ui.available_height() * 0.55).max(120.0);
        egui::ScrollArea::vertical()
            .id_salt("layers_parts_scroll")
            .auto_shrink(false)
            .max_height(max_h)
            .show(ui, |ui| {
            let sp = self.split_point;

            // ── Zone A ───────────────────────────────────────────────────
            let a_header = if split_on {
                format!("◀ Левая рука (A) — 0..{}", sp.saturating_sub(1))
            } else {
                "◀ Левая рука (A)".to_string()
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(a_header).weak().small());
            });
            for i in ZONE_A {
                if !self.parts[i].enabled { continue; }
                ui.push_id(format!("part_row_{i}"), |ui| {
                    self.draw_part_row(ui, i);
                });
            }
            let a_count = ZONE_A.filter(|&i| self.parts[i].enabled).count();
            if a_count < PARTS_PER_ZONE
                && ui.small_button("+ Добавить парт").on_hover_text("Add part to Zone A").clicked()
            {
                self.add_part_to_zone(false);
            }

            // ── Zone B (visible only when split enabled) ─────────────────
            if split_on {
                ui.add_space(theme::SP_XS);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("▶ Правая рука (B) — {sp}..127")).weak().small());
                });
                for i in ZONE_B {
                    if !self.parts[i].enabled { continue; }
                    ui.push_id(format!("part_row_{i}"), |ui| {
                        self.draw_part_row(ui, i);
                    });
                }
                let b_count = ZONE_B.filter(|&i| self.parts[i].enabled).count();
                if b_count < PARTS_PER_ZONE
                    && ui.small_button("+ Добавить парт").on_hover_text("Add part to Zone B").clicked()
                {
                    self.add_part_to_zone(true);
                }
            }
        });

        // ── Keyboard zone map ────────────────────────────────────────────
        if !self.show_drums && !self.show_midi_seq && !self.show_fx_chain {
            draw_key_zone_map(ui, &self.parts, self.active_part);
        }

        // ── Middle-click / ✕ removal confirmation ────────────────────────
        if let Some(p) = self.pending_remove_part {
            let zone = if p < PARTS_PER_ZONE { "A" } else { "B" };
            let num = (p % PARTS_PER_ZONE) + 1;
            let dialog = egui::Modal::new(egui::Id::new("remove_part_dialog")).show(ui.ctx(), |ui| {
                ui.set_width(280.0);
                ui.strong(format!("Remove part {zone}{num}?"));
                ui.label("Its preset edits will be lost.");
                ui.add_space(theme::SP_MD);
                ui.horizontal(|ui| {
                    if ui.button("Remove").clicked() {
                        self.remove_part(p);
                        self.pending_remove_part = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_remove_part = None;
                    }
                });
            });
            if dialog.should_close() {
                self.pending_remove_part = None;
            }
        }

        // ── Performance save/load (above part rows) ──────────────────────
        self.draw_perf_row(ui);
    }

    /// Performance save/load row (shown above part rows).
    fn draw_perf_row(&mut self, ui: &mut egui::Ui) {
        ui.add_space(theme::SP_XS);
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
                        pan: l.pan, transpose: l.transpose, mute: l.mute,
                        vel_min: l.vel_min, vel_max: l.vel_max,
                        root_only: l.root_only,
                    }
                }).collect();
                match preset::save_performance(&Performance {
                    name: self.perf_name.trim().to_string(),
                    category: String::new(),
                    split_point: Some(self.split_point),
                    parts,
                }) {
                    Ok(_path) => { self.show_toast("Performance saved", ToastKind::Success);
                                   self.perf_list = preset::list_performances();
                                   self.perf_list.insert(0, ("(Init)".into(), std::path::PathBuf::new())); }
                    Err(e)    => { self.show_toast(format!("Error: {e}"), ToastKind::Error); }
                }
            }
            ui.separator();
            if ui.button("Reset").on_hover_text("Load the built-in (Init) performance: single part A1 = Init preset, full keyboard, no SF2").clicked()
                && self.load_performance_by_name("(Init)")
            {
                self.show_toast("Reset to (Init)", ToastKind::Success);
            }
            ui.separator();
            let mut load_perf: Option<Performance> = None;
            egui::ComboBox::from_id_salt("perf_load")
                .selected_text(if self.perf_list.is_empty() { "No performances" } else { "Load..." })
                .width(120.0)
                .show_ui(ui, |ui| {
                    for (name, path) in &self.perf_list {
                        if ui.selectable_label(false, name).clicked() {
                            if name == "(Init)" {
                                load_perf = Some(preset::init_performance());
                            } else {
                                load_perf = preset::load_performance(path).ok();
                            }
                        }
                    }
                });
            if let Some(perf) = load_perf {
                {
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
                        self.parts[i].pan = part.pan;
                        self.parts[i].transpose = part.transpose;
                        self.parts[i].mute = part.mute;
                        self.parts[i].root_only = part.root_only;
                        self.parts[i].vel_min = if part.vel_min == 0 { 1 } else { part.vel_min };
                        self.parts[i].vel_max = if part.vel_max == 0 { 127 } else { part.vel_max };
                        self.send_edited_params(i);
                        self.send_part_enabled(i);
                        self.send_part_volume(i);
                        self.send_part_range(i);
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part: i, pan: part.pan });
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part: i, semitones: part.transpose });
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part: i, mute: part.mute });
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartRootOnly { part: i, on: part.root_only });
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartVelRange {
                            part: i, vel_min: self.parts[i].vel_min, vel_max: self.parts[i].vel_max });
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: part.sf2_mode });
                        if part.sf2_mode {
                            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                                part: i, program: part.sf2_program, bank: 0 });
                        }
                    }
                    self.show_toast(format!("Loaded: {}", perf.name), ToastKind::Success);
                }
            }
        });
    }

    /// One row = all fields of a part (mockup_layers2.html).
    fn draw_part_row(&mut self, ui: &mut egui::Ui, i: usize) {
        let is_active = i == self.active_part
            && !self.show_drums && !self.show_looper
            && !self.show_midi_seq && !self.show_fx_chain;
        let zone = if i < PARTS_PER_ZONE { "A" } else { "B" };
        let num = (i % PARTS_PER_ZONE) + 1;

        ui.horizontal(|ui| {
            // Part label — click to select the part for the params editor below
            let label = format!("{zone}{num}");
            let muted = self.parts[i].mute;
            let label_text = if muted {
                egui::RichText::new(label).strong().color(egui::Color32::GRAY)
            } else {
                egui::RichText::new(label).strong()
            };
            if ui.selectable_label(is_active, label_text)
                .on_hover_text(format!("Select part {zone}{num} for editing (M = muted)"))
                .clicked()
            {
                self.set_active_part(i);
                self.show_drums = false; self.show_looper = false;
                self.show_midi_seq = false; self.show_fx_chain = false;
                self.seq_target_atom.store(1, Ordering::Relaxed);
            }

            // Engine toggle (DSP/SF2)
            let sf2 = self.parts[i].sf2_mode;
            let eng_label = if sf2 { "SF2" } else { "DSP" };
            if ui.small_button(eng_label).on_hover_text(
                if sf2 { "Switch to DSP engine" } else { "Switch to SF2 engine" }
            ).clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                self.parts[i].sf2_mode = !sf2;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: !sf2 });
            }

            // Instrument select
            if sf2 {
                if self.sf2_keys_soundfont.is_some() {
                    use crate::synth::sampler::GM_PROGRAM_NAMES;
                    let mut prog = self.parts[i].sf2_program;
                    let cur_name = GM_PROGRAM_NAMES.get(prog as usize).copied().unwrap_or("?");
                    let inst = egui::ComboBox::from_id_salt(format!("inst_sf2_{i}"))
                        .width(170.0)
                        .selected_text(format!("prog {prog}: {cur_name}"))
                        .show_ui(ui, |ui| {
                            for (p, name) in GM_PROGRAM_NAMES.iter().enumerate() {
                                ui.selectable_value(&mut prog, p as u8, format!("prog {p}: {name}"));
                            }
                        });
                    if inst.response.changed() {
                        self.parts[i].sf2_program = prog;
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                            part: i, program: prog, bank: 0 });
                    }
                } else {
                    ui.label(egui::RichText::new("---").weak())
                        .on_hover_text("No SoundFont loaded (load one in Settings)");
                }
            } else {
                // DSP patch select. Copy patch names into locals first:
                // self.patches (immutable) + self.parts (mutable) don't overlap.
                let patch_names: Vec<String> =
                    self.patches.iter().map(|p| p.name.clone()).collect();
                let cur_idx = self.parts[i].patch_idx;
                let cur = patch_names.get(cur_idx).cloned().unwrap_or_else(|| "---".into());
                let mut new_idx = cur_idx;
                egui::ComboBox::from_id_salt(format!("inst_dsp_{i}"))
                    .width(170.0)
                    .selected_text(&cur)
                    .show_ui(ui, |ui| {
                        for (pi, name) in patch_names.iter().enumerate() {
                            ui.selectable_value(&mut new_idx, pi, name);
                        }
                    });
                if new_idx != cur_idx {
                    let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                    self.parts[i].patch_idx = new_idx;
                    self.load_edited_params(i);
                    self.send_edited_params(i);
                    self.save_config();
                }
            }

            // Mute / Root
            let mut m = self.parts[i].mute;
            if ui.checkbox(&mut m, "M").on_hover_text("Mute this part").changed() {
                self.parts[i].mute = m;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part: i, mute: m });
                self.persist_parts_state();
            }
            let mut r = self.parts[i].root_only;
            if ui.checkbox(&mut r, "R").on_hover_text(
                "Root-only (bass split): this part sounds only the lowest held key")
                .changed()
            {
                self.parts[i].root_only = r;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartRootOnly { part: i, on: r });
                self.persist_parts_state();
            }

            // Key range (derived from split — display only)
            ui.label(egui::RichText::new(format!(
                "{}–{}", note_name(self.parts[i].min_note), note_name(self.parts[i].max_note)))
                .small().weak())
                .on_hover_text("Key range");

            // Pan
            ui.label("Pan:");
            let mut pan = self.parts[i].pan;
            let pan_r = ui.add(egui::Slider::new(&mut pan, -1.0..=1.0).fixed_decimals(2))
                .on_hover_text("Double-click to reset");
            if pan_r.double_clicked() { pan = 0.0; }
            if pan_r.changed() || pan_r.double_clicked() {
                self.parts[i].pan = pan;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part: i, pan });
            }

            // Tr
            ui.label("Tr:");
            let mut tr = self.parts[i].transpose as i32;
            let tr_r = ui.add(egui::Slider::new(&mut tr, -24..=24).suffix(" st"))
                .on_hover_text("Transpose in semitones. Double-click to reset");
            if tr_r.double_clicked() { tr = 0; }
            if tr_r.changed() || tr_r.double_clicked() {
                self.parts[i].transpose = tr as i8;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part: i, semitones: tr as i8 });
            }

            // Vel
            ui.label("Vel:");
            let mut vmin = self.parts[i].vel_min as i32;
            let mut vmax = self.parts[i].vel_max as i32;
            let c1 = ui.add(egui::DragValue::new(&mut vmin).range(1..=127).speed(1)).changed();
            ui.label("-");
            let c2 = ui.add(egui::DragValue::new(&mut vmax).range(1..=127).speed(1)).changed();
            if c1 || c2 {
                let vmin = (vmin as u8).max(1);
                let vmax = (vmax as u8).max(vmin);
                self.parts[i].vel_min = vmin;
                self.parts[i].vel_max = vmax;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartVelRange { part: i, vel_min: vmin, vel_max: vmax });
            }

            // Volume
            let mut vol = self.parts[i].volume;
            let vol_r = ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).fixed_decimals(2))
                .on_hover_text("Double-click to reset");
            if vol_r.double_clicked() { vol = 0.8; }
            if vol_r.changed() || vol_r.double_clicked() {
                self.parts[i].volume = vol;
                self.send_part_volume(i);
            }

            // Remove (never for the first part of a zone)
            let is_zone_first = i.is_multiple_of(PARTS_PER_ZONE);
            if !is_zone_first && ui.small_button("✕").on_hover_text("Remove this part").clicked() {
                self.pending_remove_part = Some(i);
            }
        });
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
        self.set_active_part(if part >= PARTS_PER_ZONE { PARTS_PER_ZONE } else { 0 });
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
